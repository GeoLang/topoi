//! Centroid of any GeoJSON geometry, resolved by dimension.
//!
//! Areal members outrank linear ones and linear outrank puntal, so a mixed
//! collection is placed by its polygons alone and its lines and points are
//! ignored. A member that degenerates (a polygon of zero area, a line of zero
//! length) contributes no weight at its own dimension, so it drops to the next
//! one down instead of dividing by zero.

use crate::geojson::FeatureGeometry;
use crate::geometry::{Coord, LineString, Polygon};

/// Centroid of a geometry, or None when it holds no coordinates at all.
///
/// ```
/// use topoi_core::{Coord, LineString, Point, centroid, geojson::FeatureGeometry};
///
/// let line = FeatureGeometry::LineString(LineString::new(vec![
///     Coord::new(0.0, 0.0), Coord::new(4.0, 0.0), Coord::new(4.0, 4.0),
/// ]));
/// // Length-weighted mean of the segment midpoints
/// let c = centroid(&line).unwrap();
/// assert!((c.x - 3.0).abs() < 1e-9 && (c.y - 1.0).abs() < 1e-9);
///
/// // The point is linear-dimension noise next to the line, so it is ignored
/// let mixed = FeatureGeometry::GeometryCollection(vec![
///     FeatureGeometry::Point(Point::new(100.0, 100.0)),
///     line,
/// ]);
/// let c = centroid(&mixed).unwrap();
/// assert!((c.x - 3.0).abs() < 1e-9 && (c.y - 1.0).abs() < 1e-9);
/// ```
pub fn centroid(geometry: &FeatureGeometry) -> Option<Coord> {
    let mut acc = Centroids::default();
    acc.add(geometry);
    acc.areal
        .centroid()
        .or_else(|| acc.linear.centroid())
        .or_else(|| acc.puntal.centroid())
}

/// Weighted sum of member centroids at one dimension.
#[derive(Default)]
struct Accumulator {
    weight: f64,
    x: f64,
    y: f64,
}

impl Accumulator {
    fn add(&mut self, weight: f64, c: Coord) {
        self.weight += weight;
        self.x += weight * c.x;
        self.y += weight * c.y;
    }

    fn centroid(&self) -> Option<Coord> {
        (self.weight != 0.0).then(|| Coord::new(self.x / self.weight, self.y / self.weight))
    }
}

/// All three dimensions accumulated at once, so a degenerate member has its
/// fallback already computed when the dimension above it comes out empty.
#[derive(Default)]
struct Centroids {
    areal: Accumulator,
    linear: Accumulator,
    puntal: Accumulator,
}

impl Centroids {
    fn add(&mut self, geometry: &FeatureGeometry) {
        match geometry {
            FeatureGeometry::Point(p) => self.puntal.add(1.0, p.0),
            FeatureGeometry::MultiPoint(mp) => {
                for p in mp.points() {
                    self.puntal.add(1.0, p.0);
                }
            }
            FeatureGeometry::LineString(ls) => self.add_line(ls),
            FeatureGeometry::MultiLineString(mls) => {
                for ls in mls.linestrings() {
                    self.add_line(ls);
                }
            }
            FeatureGeometry::Polygon(poly) => self.add_polygon(poly),
            FeatureGeometry::MultiPolygon(mp) => {
                for poly in mp.polygons() {
                    self.add_polygon(poly);
                }
            }
            FeatureGeometry::GeometryCollection(members) => {
                for member in members {
                    self.add(member);
                }
            }
        }
    }

    fn add_line(&mut self, line: &LineString) {
        self.add_coords(line.coords());
    }

    fn add_polygon(&mut self, polygon: &Polygon) {
        if let Some((area, c)) = ring_centroid(polygon.exterior().coords()) {
            self.areal.add(area.abs(), c);
            for hole in polygon.interiors() {
                if let Some((hole_area, hole_c)) = ring_centroid(hole.coords()) {
                    self.areal.add(-hole_area.abs(), hole_c);
                }
            }
        }
        self.add_coords(polygon.exterior().coords());
        for hole in polygon.interiors() {
            self.add_coords(hole.coords());
        }
    }

    fn add_coords(&mut self, coords: &[Coord]) {
        for pair in coords.windows(2) {
            let length = pair[0].distance_to(&pair[1]);
            let mid = Coord::new((pair[0].x + pair[1].x) / 2.0, (pair[0].y + pair[1].y) / 2.0);
            self.linear.add(length, mid);
        }
        for c in coords {
            self.puntal.add(1.0, *c);
        }
    }
}

/// Signed area and centroid of a ring, or None when the ring encloses nothing.
///
/// The ring is implicitly closed, so a repeated final coordinate is harmless.
fn ring_centroid(coords: &[Coord]) -> Option<(f64, Coord)> {
    let n = coords.len();
    if n < 3 {
        return None;
    }
    let mut area2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let a = coords[i];
        let b = coords[(i + 1) % n];
        let cross = a.x * b.y - b.x * a.y;
        area2 += cross;
        cx += (a.x + b.x) * cross;
        cy += (a.y + b.y) * cross;
    }
    if area2 == 0.0 {
        return None;
    }
    Some((
        area2 / 2.0,
        Coord::new(cx / (3.0 * area2), cy / (3.0 * area2)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{
        LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
    };

    fn assert_at(c: Option<Coord>, x: f64, y: f64) {
        let c = c.expect("expected a centroid");
        assert!(
            (c.x - x).abs() < 1e-9 && (c.y - y).abs() < 1e-9,
            "got {c:?}"
        );
    }

    fn square(min: f64, max: f64) -> Polygon {
        Polygon::from_coords(&[
            Coord::new(min, min),
            Coord::new(max, min),
            Coord::new(max, max),
            Coord::new(min, max),
        ])
    }

    #[test]
    fn test_centroid_point_and_multipoint() {
        assert_at(
            centroid(&FeatureGeometry::Point(Point::new(3.0, 7.0))),
            3.0,
            7.0,
        );
        let mp = MultiPoint::new(vec![
            Point::new(0.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(1.0, 3.0),
        ]);
        assert_at(centroid(&FeatureGeometry::MultiPoint(mp)), 1.0, 1.0);
    }

    #[test]
    fn test_centroid_linestring_is_length_weighted() {
        // A long segment and a short one: the plain mean of the midpoints
        // would be 2.5, the length-weighted answer is 2.0
        let line = LineString::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(3.0, 0.0),
            Coord::new(4.0, 0.0),
        ]);
        assert_at(centroid(&FeatureGeometry::LineString(line)), 2.0, 0.0);
    }

    #[test]
    fn test_centroid_multilinestring() {
        let mls = MultiLineString::new(vec![
            LineString::new(vec![Coord::new(0.0, 0.0), Coord::new(4.0, 0.0)]),
            LineString::new(vec![Coord::new(0.0, 2.0), Coord::new(4.0, 2.0)]),
        ]);
        assert_at(centroid(&FeatureGeometry::MultiLineString(mls)), 2.0, 1.0);
    }

    #[test]
    fn test_centroid_polygon_with_hole() {
        // A centred hole leaves the centroid where it was
        let poly = Polygon::new(
            Ring::new(square(0.0, 10.0).exterior().coords().to_vec()),
            vec![Ring::new(square(4.0, 6.0).exterior().coords().to_vec())],
        );
        assert_at(centroid(&FeatureGeometry::Polygon(poly)), 5.0, 5.0);
    }

    #[test]
    fn test_centroid_multipolygon_is_area_weighted() {
        // Centres (1, 1) and (10.5, 10.5) with weights 4 and 1
        let mp = MultiPolygon::new(vec![square(0.0, 2.0), square(10.0, 11.0)]);
        assert_at(centroid(&FeatureGeometry::MultiPolygon(mp)), 2.9, 2.9);
    }

    #[test]
    fn test_centroid_collection_uses_highest_dimension() {
        let areal = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::Point(Point::new(500.0, 500.0)),
            FeatureGeometry::LineString(LineString::new(vec![
                Coord::new(-100.0, 0.0),
                Coord::new(-100.0, 50.0),
            ])),
            FeatureGeometry::Polygon(square(0.0, 4.0)),
        ]);
        assert_at(centroid(&areal), 2.0, 2.0);

        // Drop the polygon and the line takes over, the point still ignored
        let linear = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::Point(Point::new(500.0, 500.0)),
            FeatureGeometry::LineString(LineString::new(vec![
                Coord::new(-100.0, 0.0),
                Coord::new(-100.0, 50.0),
            ])),
        ]);
        assert_at(centroid(&linear), -100.0, 25.0);
    }

    #[test]
    fn test_centroid_nested_collection() {
        let nested = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::GeometryCollection(vec![FeatureGeometry::Polygon(square(0.0, 2.0))]),
            FeatureGeometry::Polygon(square(10.0, 11.0)),
        ]);
        assert_at(centroid(&nested), 2.9, 2.9);
    }

    #[test]
    fn test_centroid_degenerate_polygon_falls_back_to_line() {
        // Collinear ring: no area, so the linear centroid of the ring wins
        let flat = Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 0.0),
            Coord::new(4.0, 0.0),
        ]);
        assert_at(centroid(&FeatureGeometry::Polygon(flat)), 2.0, 0.0);
    }

    #[test]
    fn test_centroid_degenerate_line_falls_back_to_points() {
        let dot = LineString::new(vec![Coord::new(5.0, 6.0), Coord::new(5.0, 6.0)]);
        assert_at(centroid(&FeatureGeometry::LineString(dot)), 5.0, 6.0);
    }

    #[test]
    fn test_centroid_empty_is_none() {
        assert!(centroid(&FeatureGeometry::GeometryCollection(vec![])).is_none());
        assert!(centroid(&FeatureGeometry::MultiPoint(MultiPoint::new(vec![]))).is_none());
    }
}
