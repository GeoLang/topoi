//! Buffering, backed by the i_overlay outline and stroke engines.

use crate::geojson::FeatureGeometry;
use crate::geometry::{Coord, LineString, MultiPolygon, Polygon, Ring};
use crate::overlay::{PolygonSet, from_shapes, to_shapes, union};
use i_overlay::mesh::outline::offset::OutlineOffset;
use i_overlay::mesh::stroke::offset::StrokeOffset;
use i_overlay::mesh::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};
use std::f64::consts::PI;

/// Round joins take the maximum segment length over the arc radius, so this is
/// about 32 segments for a full circle.
const ROUND_JOIN_ANGLE: f64 = PI / 16.0;

/// Corners sharper than this get cut off instead of spiking far out.
const MITER_LIMIT_ANGLE: f64 = PI / 3.0;

/// How a buffer outline turns a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinStyle {
    /// Arc corners, which is what a GIS buffer normally means.
    Round,
    /// Sharp corners, cut off past a limit so spikes stay bounded.
    Miter,
    /// Corners cut off flat.
    Bevel,
}

/// Buffer a polygon set by a distance, with round joins.
///
/// Positive grows, negative shrinks. Concave rings, holes and multipolygon
/// operands all work. The result is a `MultiPolygon` because growing can merge
/// separate pieces and shrinking can split one piece into several or erase it
/// entirely.
///
/// ```
/// use topoi_core::{Coord, JoinStyle, Polygon, buffer_polygon, buffer_polygon_with_join};
///
/// let square = Polygon::from_coords(&[
///     Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
///     Coord::new(10.0, 10.0), Coord::new(0.0, 10.0),
/// ]);
///
/// // Shrinking a convex ring adds no arcs, so this is exactly 8x8
/// let inset = buffer_polygon(&square, -1.0);
/// assert!((inset.area() - 64.0).abs() < 1e-9);
///
/// // A negative distance past the inradius leaves nothing
/// assert!(buffer_polygon(&square, -6.0).polygons().is_empty());
///
/// // Sharp corners instead of arcs
/// let mitred = buffer_polygon_with_join(&square, 1.0, JoinStyle::Miter);
/// assert!((mitred.area() - 144.0).abs() < 1e-9);
/// ```
pub fn buffer_polygon<S>(subject: &S, distance: f64) -> MultiPolygon
where
    S: PolygonSet + ?Sized,
{
    buffer_polygon_with_join(subject, distance, JoinStyle::Round)
}

/// Buffer a polygon set by a distance with an explicit join style.
pub fn buffer_polygon_with_join<S>(subject: &S, distance: f64, join: JoinStyle) -> MultiPolygon
where
    S: PolygonSet + ?Sized,
{
    let line_join = match join {
        JoinStyle::Round => LineJoin::Round(ROUND_JOIN_ANGLE),
        JoinStyle::Miter => LineJoin::Miter(MITER_LIMIT_ANGLE),
        JoinStyle::Bevel => LineJoin::Bevel,
    };
    buffer_polygons(subject.as_polygons(), distance, line_join)
}

/// Buffer any GeoJSON geometry by a distance, with round joins and caps.
///
/// `segments` is the arc resolution as sides of a full circle: points become
/// `segments`-gon discs, and corners and line ends get arcs at the same
/// resolution. A negative distance shrinks polygons and leaves points and
/// lines empty, since neither has an area to take away from. Members of a
/// GeometryCollection are buffered separately and unioned, so overlapping
/// pieces come back merged.
///
/// ```
/// use topoi_core::{Coord, LineString, buffer_geometry, geojson::FeatureGeometry};
/// use std::f64::consts::PI;
///
/// let line = FeatureGeometry::LineString(LineString::new(vec![
///     Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
/// ]));
///
/// // A capsule: a 10 by 4 body plus two half discs of radius 2
/// let capsule = buffer_geometry(&line, 2.0, 64);
/// let expected = 10.0 * 4.0 + PI * 4.0;
/// assert!((capsule.area() - expected).abs() < 0.02 * expected);
///
/// // Nothing to shrink
/// assert!(buffer_geometry(&line, -2.0, 64).polygons().is_empty());
/// ```
pub fn buffer_geometry(geometry: &FeatureGeometry, distance: f64, segments: usize) -> MultiPolygon {
    let mut parts = Vec::new();
    collect_buffer(geometry, distance, segments, &mut parts);
    if parts.len() == 1 {
        return parts.swap_remove(0);
    }
    let polygons: Vec<Polygon> = parts
        .iter()
        .flat_map(|part| part.polygons().iter().cloned())
        .collect();
    if polygons.is_empty() {
        return MultiPolygon::new(Vec::new());
    }
    // Union against nothing still resolves overlaps within the subject.
    union(&polygons, &Vec::<Polygon>::new())
}

fn collect_buffer(
    geometry: &FeatureGeometry,
    distance: f64,
    segments: usize,
    out: &mut Vec<MultiPolygon>,
) {
    match geometry {
        FeatureGeometry::Point(p) => out.extend(disc(p.0, distance, segments)),
        FeatureGeometry::MultiPoint(mp) => {
            out.extend(
                mp.points()
                    .iter()
                    .filter_map(|p| disc(p.0, distance, segments)),
            );
        }
        FeatureGeometry::LineString(ls) => out.extend(capsule(ls, distance, segments)),
        FeatureGeometry::MultiLineString(mls) => {
            out.extend(
                mls.linestrings()
                    .iter()
                    .filter_map(|ls| capsule(ls, distance, segments)),
            );
        }
        FeatureGeometry::Polygon(poly) => out.push(buffer_polygons(
            poly.as_polygons(),
            distance,
            LineJoin::Round(arc_angle(segments)),
        )),
        FeatureGeometry::MultiPolygon(mp) => out.push(buffer_polygons(
            mp.polygons(),
            distance,
            LineJoin::Round(arc_angle(segments)),
        )),
        FeatureGeometry::GeometryCollection(members) => {
            for member in members {
                collect_buffer(member, distance, segments, out);
            }
        }
    }
}

fn buffer_polygons(polygons: &[Polygon], distance: f64, join: LineJoin<f64>) -> MultiPolygon {
    if distance == 0.0 {
        return MultiPolygon::new(polygons.to_vec());
    }

    // to_shapes normalizes exteriors to CCW and holes to CW, which the outline
    // engine needs: a reversed contour offsets the wrong way and vanishes.
    let shapes = to_shapes(polygons);
    let style = OutlineStyle::new(distance).line_join(join);

    from_shapes(shapes.outline(&style))
}

/// A regular `segments`-gon approximating the disc of `radius` about a point.
fn disc(center: Coord, radius: f64, segments: usize) -> Option<MultiPolygon> {
    if radius <= 0.0 {
        return None;
    }
    let sides = segments.max(3);
    let mut coords: Vec<Coord> = (0..sides)
        .map(|i| {
            let angle = 2.0 * PI * i as f64 / sides as f64;
            Coord::new(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect();
    coords.push(coords[0]);
    Some(MultiPolygon::new(vec![Polygon::new(
        Ring::new(coords),
        vec![],
    )]))
}

/// The swept area of a line, with round joins at the bends and round caps at
/// both ends.
fn capsule(line: &LineString, radius: f64, segments: usize) -> Option<MultiPolygon> {
    let coords = line.coords();
    if radius <= 0.0 || coords.len() < 2 {
        return None;
    }
    let angle = arc_angle(segments);
    let style = StrokeStyle::new(2.0 * radius)
        .start_cap(LineCap::Round(angle))
        .end_cap(LineCap::Round(angle))
        .line_join(LineJoin::Round(angle));
    Some(from_shapes(coords.stroke(style, false)))
}

/// Arc resolution as i_overlay wants it: maximum segment length over the arc
/// radius, which for a circle of `segments` sides is the turn per side.
fn arc_angle(segments: usize) -> f64 {
    2.0 * PI / segments.max(3) as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{MultiLineString, MultiPoint, Point};

    const SEGMENTS: usize = 64;

    fn point(x: f64, y: f64) -> FeatureGeometry {
        FeatureGeometry::Point(Point::new(x, y))
    }

    fn line(coords: &[(f64, f64)]) -> LineString {
        LineString::new(coords.iter().map(|(x, y)| Coord::new(*x, *y)).collect())
    }

    #[test]
    fn test_zero_buffer() {
        let ring = Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
            Coord::new(0.0, 1.0),
            Coord::new(0.0, 0.0),
        ]);
        let poly = Polygon::new(ring.clone(), vec![]);
        let result = buffer_polygon(&poly, 0.0);
        assert_eq!(result.polygons().len(), 1);
        assert_eq!(result.polygons()[0].exterior().coords(), ring.coords());
    }

    #[test]
    fn test_positive_buffer_increases_area() {
        let ring = Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(4.0, 4.0),
            Coord::new(0.0, 4.0),
            Coord::new(0.0, 0.0),
        ]);
        let poly = Polygon::new(ring, vec![]);
        let result = buffer_polygon(&poly, 1.0);
        assert!(result.area() > poly.area());
    }

    #[test]
    fn test_join_styles_all_produce_a_polygon() {
        let poly = Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(0.0, 4.0),
        ]);
        for join in [JoinStyle::Round, JoinStyle::Miter, JoinStyle::Bevel] {
            let result = buffer_polygon_with_join(&poly, 1.0, join);
            assert_eq!(result.polygons().len(), 1, "{join:?}");
            assert!(result.area() > poly.area(), "{join:?}");
        }
    }

    #[test]
    fn test_buffer_point_is_a_disc() {
        let result = buffer_geometry(&point(3.0, 4.0), 2.0, SEGMENTS);
        assert_eq!(result.polygons().len(), 1);
        assert_eq!(result.polygons()[0].exterior().coords().len(), SEGMENTS + 1);
        // The inscribed n-gon is a shade under the disc it approximates
        let disc = PI * 4.0;
        assert!(result.area() < disc);
        assert!(
            (result.area() - disc).abs() < 0.005 * disc,
            "{}",
            result.area()
        );
    }

    #[test]
    fn test_buffer_linestring_is_a_capsule() {
        let geom = FeatureGeometry::LineString(line(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0)]));
        let result = buffer_geometry(&geom, 1.5, SEGMENTS);
        // Straight body of both segments, plus a full disc worth of round cap
        // and corner, minus nothing since the bend is a right angle
        let expected = 2.0 * 1.5 * 15.0 + PI * 1.5 * 1.5;
        assert!(
            (result.area() - expected).abs() < 0.02 * expected,
            "{} vs {expected}",
            result.area()
        );
    }

    #[test]
    fn test_buffer_negative_distance_empties_points_and_lines() {
        assert!(
            buffer_geometry(&point(0.0, 0.0), -1.0, SEGMENTS)
                .polygons()
                .is_empty()
        );
        let geom = FeatureGeometry::LineString(line(&[(0.0, 0.0), (10.0, 0.0)]));
        assert!(buffer_geometry(&geom, -1.0, SEGMENTS).polygons().is_empty());
        let mp = FeatureGeometry::MultiPoint(MultiPoint::new(vec![Point::new(0.0, 0.0)]));
        assert!(buffer_geometry(&mp, -1.0, SEGMENTS).polygons().is_empty());
    }

    #[test]
    fn test_buffer_negative_distance_shrinks_polygons() {
        let square = Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(10.0, 0.0),
            Coord::new(10.0, 10.0),
            Coord::new(0.0, 10.0),
        ]);
        let result = buffer_geometry(&FeatureGeometry::Polygon(square), -1.0, SEGMENTS);
        assert!((result.area() - 64.0).abs() < 1e-9, "{}", result.area());
    }

    #[test]
    fn test_buffer_multipoint_keeps_disjoint_discs_apart() {
        let mp = MultiPoint::new(vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]);
        let result = buffer_geometry(&FeatureGeometry::MultiPoint(mp), 1.0, SEGMENTS);
        assert_eq!(result.polygons().len(), 2);
    }

    #[test]
    fn test_buffer_geometry_collection_unions_members() {
        let collection = FeatureGeometry::GeometryCollection(vec![
            point(0.0, 0.0),
            FeatureGeometry::MultiLineString(MultiLineString::new(vec![line(&[
                (0.0, 0.0),
                (4.0, 0.0),
            ])])),
            FeatureGeometry::Polygon(Polygon::from_coords(&[
                Coord::new(4.0, -1.0),
                Coord::new(6.0, -1.0),
                Coord::new(6.0, 1.0),
                Coord::new(4.0, 1.0),
            ])),
        ]);
        let result = buffer_geometry(&collection, 1.0, SEGMENTS);
        assert_eq!(result.polygons().len(), 1, "overlapping members must merge");

        // The disc sits entirely inside the line's own capsule, so the union is
        // the capsule plus what the buffered square adds beyond it
        let capsule = 2.0 * 4.0 + PI;
        assert!(result.area() > capsule, "{}", result.area());
        assert!(result.area() < capsule + 4.0 * 4.0, "{}", result.area());
    }
}
