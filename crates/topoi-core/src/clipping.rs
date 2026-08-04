use crate::geojson::FeatureGeometry;
use crate::geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};
use crate::overlay::{self, intersection};
use crate::predicates::contains;

/// Sutherland-Hodgman polygon clipping algorithm.
///
/// Fast path for clipping against a convex window, which is what viewport and
/// tile clipping need. The clip polygon must be convex and wound
/// counter-clockwise, and a concave subject whose clipped result is disconnected
/// comes back as one ring joined by degenerate edges rather than as separate
/// pieces. Use [`intersection`](crate::intersection) for the general case.
///
/// Both polygons are given as ordered vertex lists (closed or open — last vertex
/// is implicitly connected back to first).
///
/// Returns the clipped polygon vertices, or empty if fully outside.
pub fn clip_polygon(subject: &[Coord], clip: &[Coord]) -> Vec<Coord> {
    if subject.is_empty() || clip.len() < 3 {
        return Vec::new();
    }

    let mut output = subject.to_vec();

    for i in 0..clip.len() {
        if output.is_empty() {
            return Vec::new();
        }

        let input = output;
        output = Vec::new();

        let edge_start = clip[i];
        let edge_end = clip[(i + 1) % clip.len()];

        for j in 0..input.len() {
            let current = input[j];
            let previous = input[(j + input.len() - 1) % input.len()];

            let curr_inside = is_inside(&current, &edge_start, &edge_end);
            let prev_inside = is_inside(&previous, &edge_start, &edge_end);

            if curr_inside {
                if !prev_inside {
                    // Entering: add intersection then current
                    if let Some(intersection) =
                        line_intersection(&previous, &current, &edge_start, &edge_end)
                    {
                        output.push(intersection);
                    }
                }
                output.push(current);
            } else if prev_inside {
                // Leaving: add intersection only
                if let Some(intersection) =
                    line_intersection(&previous, &current, &edge_start, &edge_end)
                {
                    output.push(intersection);
                }
            }
        }
    }

    output
}

/// Clip a polygon against an axis-aligned bounding box.
///
/// A rectangle is always convex, so this is the fast path of [`clip_polygon`]
/// and carries the same caveat about disconnected results.
pub fn clip_polygon_rect(
    subject: &[Coord],
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Vec<Coord> {
    let clip_rect = [
        Coord::new(min_x, min_y),
        Coord::new(max_x, min_y),
        Coord::new(max_x, max_y),
        Coord::new(min_x, max_y),
    ];
    clip_polygon(subject, &clip_rect)
}

/// Compute the intersection area of two polygon rings.
///
/// Goes through the general overlay engine, so concave rings are handled.
pub fn intersection_area(poly_a: &[Coord], poly_b: &[Coord]) -> f64 {
    overlay::intersection(&Polygon::from_coords(poly_a), &Polygon::from_coords(poly_b)).area()
}

/// Clip a segment to a closed axis-aligned rectangle (Liang-Barsky).
///
/// Returns the clipped endpoints, or `None` if the segment misses the
/// rectangle. A segment touching only the boundary is kept.
pub fn clip_segment_rect(
    a: Coord,
    b: Coord,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Option<(Coord, Coord)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    for (p, q) in [
        (-dx, a.x - min_x),
        (dx, max_x - a.x),
        (-dy, a.y - min_y),
        (dy, max_y - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                t0 = t0.max(r);
            } else {
                if r < t0 {
                    return None;
                }
                t1 = t1.min(r);
            }
        }
    }
    Some((
        Coord::new(a.x + t0 * dx, a.y + t0 * dy),
        Coord::new(a.x + t1 * dx, a.y + t1 * dy),
    ))
}

/// Clip a polyline to a closed axis-aligned rectangle.
///
/// Returns the parts inside the rectangle, each a polyline of at least two
/// vertices, consecutive surviving segments joined back into one part.
pub fn clip_linestring_rect(
    coords: &[Coord],
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Vec<Vec<Coord>> {
    let mut parts: Vec<Vec<Coord>> = Vec::new();
    for pair in coords.windows(2) {
        let Some((a, b)) = clip_segment_rect(pair[0], pair[1], min_x, min_y, max_x, max_y) else {
            continue;
        };
        if a.x == b.x && a.y == b.y {
            continue;
        }
        match parts.last_mut() {
            Some(part) if part.last() == Some(&a) => part.push(b),
            _ => parts.push(vec![a, b]),
        }
    }
    parts
}

/// Clip any geometry to a polygonal boundary, or `None` when nothing of it
/// survives.
///
/// Polygons go through the general overlay engine, so holes and concave
/// boundaries are handled, and the result comes back as a `Polygon` when one
/// piece survives and a `MultiPolygon` when several do. Lines are cut at
/// their crossings with the boundary rings, keeping the pieces whose midpoint
/// lies inside. Points survive when the boundary contains them. A collection
/// clips member by member and keeps whatever survives.
///
/// ```
/// use topoi_core::geojson::FeatureGeometry;
/// use topoi_core::{Coord, MultiPolygon, Point, Polygon, clip_to_boundary};
///
/// let boundary = MultiPolygon::new(vec![Polygon::from_coords(&[
///     Coord::new(0.0, 0.0), Coord::new(4.0, 0.0),
///     Coord::new(4.0, 4.0), Coord::new(0.0, 4.0),
/// ])]);
/// let outside = FeatureGeometry::Point(Point::new(9.0, 9.0));
/// assert!(clip_to_boundary(&outside, &boundary).is_none());
/// ```
pub fn clip_to_boundary(
    geometry: &FeatureGeometry,
    boundary: &MultiPolygon,
) -> Option<FeatureGeometry> {
    match geometry {
        FeatureGeometry::Point(p) => inside(boundary, &p.0).then(|| geometry.clone()),
        FeatureGeometry::MultiPoint(mp) => {
            let kept: Vec<Point> = mp
                .points()
                .iter()
                .filter(|p| inside(boundary, &p.0))
                .copied()
                .collect();
            (!kept.is_empty()).then(|| FeatureGeometry::MultiPoint(MultiPoint::new(kept)))
        }
        FeatureGeometry::LineString(l) => lines(clip_line(l.coords(), boundary)),
        FeatureGeometry::MultiLineString(mls) => lines(
            mls.linestrings()
                .iter()
                .flat_map(|l| clip_line(l.coords(), boundary))
                .collect(),
        ),
        FeatureGeometry::Polygon(p) => polygons(intersection(p, boundary)),
        FeatureGeometry::MultiPolygon(mp) => polygons(intersection(mp, boundary)),
        FeatureGeometry::GeometryCollection(members) => {
            let kept: Vec<FeatureGeometry> = members
                .iter()
                .filter_map(|m| clip_to_boundary(m, boundary))
                .collect();
            (!kept.is_empty()).then_some(FeatureGeometry::GeometryCollection(kept))
        }
    }
}

fn polygons(clipped: MultiPolygon) -> Option<FeatureGeometry> {
    match clipped.polygons() {
        [] => None,
        [single] => Some(FeatureGeometry::Polygon(single.clone())),
        _ => Some(FeatureGeometry::MultiPolygon(clipped.clone())),
    }
}

fn lines(parts: Vec<Vec<Coord>>) -> Option<FeatureGeometry> {
    match parts.len() {
        0 => None,
        1 => Some(FeatureGeometry::LineString(LineString::new(
            parts.into_iter().next().expect("one part"),
        ))),
        _ => Some(FeatureGeometry::MultiLineString(MultiLineString::new(
            parts.into_iter().map(LineString::new).collect(),
        ))),
    }
}

fn inside(boundary: &MultiPolygon, c: &Coord) -> bool {
    boundary.polygons().iter().any(|p| contains(p, c))
}

/// Cut a line at every boundary crossing and keep the pieces whose midpoint
/// is inside, holes included.
fn clip_line(coords: &[Coord], boundary: &MultiPolygon) -> Vec<Vec<Coord>> {
    let mut parts: Vec<Vec<Coord>> = Vec::new();
    for seg in coords.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let mut ts = crossings(a, b, boundary);
        ts.push(0.0);
        ts.push(1.0);
        ts.sort_by(f64::total_cmp);
        for w in ts.windows(2) {
            let (t0, t1) = (w[0], w[1]);
            if t1 <= t0 {
                continue;
            }
            if !inside(boundary, &at(a, b, (t0 + t1) / 2.0)) {
                continue;
            }
            let (p0, p1) = (at(a, b, t0), at(a, b, t1));
            match parts.last_mut() {
                Some(part) if part.last() == Some(&p0) => part.push(p1),
                _ => parts.push(vec![p0, p1]),
            }
        }
    }
    parts
}

/// The ends stay bit-exact, so a segment the boundary does not touch keeps
/// its source vertices.
fn at(a: Coord, b: Coord, t: f64) -> Coord {
    match t {
        0.0 => a,
        1.0 => b,
        t => Coord::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)),
    }
}

/// Parameters along `a`-`b` where it meets a boundary ring edge.
fn crossings(a: Coord, b: Coord, boundary: &MultiPolygon) -> Vec<f64> {
    let mut ts = Vec::new();
    for poly in boundary.polygons() {
        for ring in std::iter::once(poly.exterior()).chain(poly.interiors()) {
            let coords = ring.coords();
            for i in 0..coords.len() {
                let (c, d) = (coords[i], coords[(i + 1) % coords.len()]);
                if let Some(t) = segment_param(a, b, c, d) {
                    ts.push(t);
                }
            }
        }
    }
    ts
}

fn segment_param(a: Coord, b: Coord, c: Coord, d: Coord) -> Option<f64> {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let (cdx, cdy) = (d.x - c.x, d.y - c.y);
    let denom = abx * cdy - aby * cdx;
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = ((c.x - a.x) * cdy - (c.y - a.y) * cdx) / denom;
    let u = ((c.x - a.x) * aby - (c.y - a.y) * abx) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

/// Determine if a point is on the "inside" (left side) of a directed edge.
fn is_inside(point: &Coord, edge_start: &Coord, edge_end: &Coord) -> bool {
    // Cross product of edge vector and point-start vector
    let cross = (edge_end.x - edge_start.x) * (point.y - edge_start.y)
        - (edge_end.y - edge_start.y) * (point.x - edge_start.x);
    cross >= 0.0
}

/// Compute intersection of two line segments (treated as infinite lines).
fn line_intersection(p1: &Coord, p2: &Coord, p3: &Coord, p4: &Coord) -> Option<Coord> {
    let x1 = p1.x;
    let y1 = p1.y;
    let x2 = p2.x;
    let y2 = p2.y;
    let x3 = p3.x;
    let y3 = p3.y;
    let x4 = p4.x;
    let y4 = p4.y;

    let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
    if denom.abs() < 1e-12 {
        return None; // Parallel lines
    }

    let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;

    Some(Coord::new(x1 + t * (x2 - x1), y1 + t * (y2 - y1)))
}

/// Compute the union area of two polygon rings.
///
/// Goes through the general overlay engine, so concave rings and disjoint
/// operands are handled.
pub fn union_area(poly_a: &[Coord], poly_b: &[Coord]) -> f64 {
    overlay::union(&Polygon::from_coords(poly_a), &Polygon::from_coords(poly_b)).area()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Ring, signed_ring_area};

    fn polygon_area(vertices: &[Coord]) -> f64 {
        signed_ring_area(vertices).abs()
    }

    #[test]
    fn test_clip_polygon_fully_inside() {
        // Small square inside a larger square
        let subject = vec![
            Coord::new(1.0, 1.0),
            Coord::new(2.0, 1.0),
            Coord::new(2.0, 2.0),
            Coord::new(1.0, 2.0),
        ];
        let clip = vec![
            Coord::new(0.0, 0.0),
            Coord::new(3.0, 0.0),
            Coord::new(3.0, 3.0),
            Coord::new(0.0, 3.0),
        ];
        let result = clip_polygon(&subject, &clip);
        assert_eq!(result.len(), 4);
        let area = polygon_area(&result);
        assert!((area - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_clip_polygon_fully_outside() {
        let subject = vec![
            Coord::new(5.0, 5.0),
            Coord::new(6.0, 5.0),
            Coord::new(6.0, 6.0),
            Coord::new(5.0, 6.0),
        ];
        let clip = vec![
            Coord::new(0.0, 0.0),
            Coord::new(3.0, 0.0),
            Coord::new(3.0, 3.0),
            Coord::new(0.0, 3.0),
        ];
        let result = clip_polygon(&subject, &clip);
        assert!(result.is_empty());
    }

    #[test]
    fn test_clip_polygon_partial_overlap() {
        // Subject overlaps half the clip region
        let subject = vec![
            Coord::new(-1.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 2.0),
            Coord::new(-1.0, 2.0),
        ];
        let clip = vec![
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 0.0),
            Coord::new(2.0, 2.0),
            Coord::new(0.0, 2.0),
        ];
        let result = clip_polygon(&subject, &clip);
        let area = polygon_area(&result);
        // Intersection should be the 1x2 overlap region
        assert!((area - 2.0).abs() < 1e-10, "expected 2.0, got {area}");
    }

    #[test]
    fn test_clip_polygon_rect() {
        let triangle = vec![
            Coord::new(0.5, 0.5),
            Coord::new(1.5, 0.5),
            Coord::new(1.0, 1.5),
        ];
        let result = clip_polygon_rect(&triangle, 0.0, 0.0, 1.0, 1.0);
        // Triangle partially inside [0,1]x[0,1]
        assert!(result.len() >= 3);
        let area = polygon_area(&result);
        // Should be less than the full triangle area (0.5)
        assert!(area > 0.0 && area < 0.5);
    }

    #[test]
    fn test_clip_segment_rect() {
        let clipped = clip_segment_rect(
            Coord::new(-1.0, 0.5),
            Coord::new(3.0, 0.5),
            0.0,
            0.0,
            2.0,
            2.0,
        );
        assert_eq!(clipped, Some((Coord::new(0.0, 0.5), Coord::new(2.0, 0.5))));
        let missed = clip_segment_rect(
            Coord::new(-1.0, 3.0),
            Coord::new(3.0, 3.0),
            0.0,
            0.0,
            2.0,
            2.0,
        );
        assert_eq!(missed, None);
    }

    #[test]
    fn test_clip_linestring_rect_multi_part() {
        // enters, leaves, re-enters: two parts
        let coords = vec![
            Coord::new(-1.0, 0.5),
            Coord::new(1.0, 0.5),
            Coord::new(1.0, 5.0),
            Coord::new(1.5, 5.0),
            Coord::new(1.5, 1.0),
        ];
        let parts = clip_linestring_rect(&coords, 0.0, 0.0, 2.0, 2.0);
        assert_eq!(parts.len(), 2);
        assert_eq!(
            parts[0],
            vec![
                Coord::new(0.0, 0.5),
                Coord::new(1.0, 0.5),
                Coord::new(1.0, 2.0),
            ]
        );
        assert_eq!(parts[1], vec![Coord::new(1.5, 2.0), Coord::new(1.5, 1.0)]);
    }

    #[test]
    fn test_clip_linestring_rect_outside() {
        let coords = vec![Coord::new(5.0, 5.0), Coord::new(6.0, 6.0)];
        assert!(clip_linestring_rect(&coords, 0.0, 0.0, 2.0, 2.0).is_empty());
    }

    #[test]
    fn test_intersection_area_two_squares() {
        let sq1 = vec![
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 0.0),
            Coord::new(2.0, 2.0),
            Coord::new(0.0, 2.0),
        ];
        let sq2 = vec![
            Coord::new(1.0, 1.0),
            Coord::new(3.0, 1.0),
            Coord::new(3.0, 3.0),
            Coord::new(1.0, 3.0),
        ];
        let area = intersection_area(&sq1, &sq2);
        assert!((area - 1.0).abs() < 1e-10, "expected 1.0, got {area}");
    }

    #[test]
    fn test_union_area_two_squares() {
        let sq1 = vec![
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 0.0),
            Coord::new(2.0, 2.0),
            Coord::new(0.0, 2.0),
        ];
        let sq2 = vec![
            Coord::new(1.0, 1.0),
            Coord::new(3.0, 1.0),
            Coord::new(3.0, 3.0),
            Coord::new(1.0, 3.0),
        ];
        let area = union_area(&sq1, &sq2);
        // 4 + 4 - 1 = 7
        assert!((area - 7.0).abs() < 1e-10, "expected 7.0, got {area}");
    }

    fn rect(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Ring {
        Ring::new(vec![
            Coord::new(min_x, min_y),
            Coord::new(max_x, min_y),
            Coord::new(max_x, max_y),
            Coord::new(min_x, max_y),
            Coord::new(min_x, min_y),
        ])
    }

    /// A 16 x 16 square boundary.
    fn boundary() -> MultiPolygon {
        MultiPolygon::new(vec![Polygon::new(rect(4.0, 12.0, 20.0, 28.0), vec![])])
    }

    /// A horizontal line at y = 20 with a vertex at every unit from x0 to x1.
    fn unit_line(x0: i32, x1: i32) -> Vec<Coord> {
        (x0..=x1).map(|x| Coord::new(f64::from(x), 20.0)).collect()
    }

    #[test]
    fn test_clip_to_boundary_keeps_points_by_containment() {
        let inside = FeatureGeometry::Point(Point::new(10.0, 20.0));
        let outside = FeatureGeometry::Point(Point::new(1.0, 20.0));
        let Some(FeatureGeometry::Point(kept)) = clip_to_boundary(&inside, &boundary()) else {
            panic!("the point inside the boundary must survive");
        };
        assert_eq!(kept, Point::new(10.0, 20.0));
        assert!(clip_to_boundary(&outside, &boundary()).is_none());

        // a hole in the boundary excludes the points it covers
        let holed = MultiPolygon::new(vec![Polygon::new(
            rect(4.0, 12.0, 20.0, 28.0),
            vec![rect(8.0, 16.0, 12.0, 24.0)],
        )]);
        assert!(clip_to_boundary(&inside, &holed).is_none());
        let beside_the_hole = FeatureGeometry::Point(Point::new(16.0, 20.0));
        assert!(matches!(
            clip_to_boundary(&beside_the_hole, &holed),
            Some(FeatureGeometry::Point(p)) if p == Point::new(16.0, 20.0)
        ));
    }

    #[test]
    fn test_clip_to_boundary_keeps_the_members_inside_a_multipoint() {
        let mp = FeatureGeometry::MultiPoint(MultiPoint::new(vec![
            Point::new(1.0, 20.0),
            Point::new(10.0, 20.0),
            Point::new(16.0, 20.0),
        ]));
        let Some(FeatureGeometry::MultiPoint(kept)) = clip_to_boundary(&mp, &boundary()) else {
            panic!("the two members inside must survive as a multipoint");
        };
        assert_eq!(
            kept.points(),
            &[Point::new(10.0, 20.0), Point::new(16.0, 20.0)]
        );

        let all_outside = FeatureGeometry::MultiPoint(MultiPoint::new(vec![
            Point::new(1.0, 20.0),
            Point::new(30.0, 20.0),
        ]));
        assert!(clip_to_boundary(&all_outside, &boundary()).is_none());
    }

    #[test]
    fn test_clip_to_boundary_cuts_a_line_at_the_crossings() {
        let line = FeatureGeometry::LineString(LineString::new(unit_line(0, 24)));
        let Some(FeatureGeometry::LineString(cut)) = clip_to_boundary(&line, &boundary()) else {
            panic!("the crossing line must survive as one linestring");
        };
        // the ends land on the boundary and the vertices between them are the
        // source coordinates, bit for bit
        assert_eq!(cut.coords(), unit_line(4, 20));

        let outside = FeatureGeometry::LineString(LineString::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(3.0, 0.0),
        ]));
        assert!(clip_to_boundary(&outside, &boundary()).is_none());
    }

    #[test]
    fn test_clip_to_boundary_splits_a_line_into_parts() {
        // two boundary squares with a gap, so one line comes back in pieces
        let split = MultiPolygon::new(vec![
            Polygon::new(rect(0.0, 16.0, 8.0, 24.0), vec![]),
            Polygon::new(rect(16.0, 16.0, 24.0, 24.0), vec![]),
        ]);
        let line = FeatureGeometry::LineString(LineString::new(unit_line(0, 24)));
        let Some(FeatureGeometry::MultiLineString(parts)) = clip_to_boundary(&line, &split) else {
            panic!("a line crossing both squares must come back as parts");
        };
        assert_eq!(parts.linestrings().len(), 2);
        assert_eq!(parts.linestrings()[0].coords(), unit_line(0, 8));
        assert_eq!(parts.linestrings()[1].coords(), unit_line(16, 24));
    }

    #[test]
    fn test_clip_to_boundary_shapes_polygon_results() {
        // one surviving piece stays a polygon, and a hole inside the boundary
        // is preserved: the 16 x 16 overlap minus a 4 x 4 hole
        let zone = FeatureGeometry::Polygon(Polygon::new(
            rect(0.0, 8.0, 24.0, 32.0),
            vec![rect(6.0, 14.0, 10.0, 18.0)],
        ));
        let Some(FeatureGeometry::Polygon(clipped)) = clip_to_boundary(&zone, &boundary()) else {
            panic!("one surviving piece must come back as a polygon");
        };
        assert!(
            (clipped.area() - 240.0).abs() < 1e-9,
            "expected 240.0, got {}",
            clipped.area()
        );

        // a bar across two disjoint boundary squares splits in two
        let split = MultiPolygon::new(vec![
            Polygon::new(rect(0.0, 0.0, 2.0, 10.0), vec![]),
            Polygon::new(rect(8.0, 0.0, 10.0, 10.0), vec![]),
        ]);
        let bar =
            FeatureGeometry::Polygon(Polygon::from_coords(rect(0.0, 4.0, 10.0, 6.0).coords()));
        let Some(FeatureGeometry::MultiPolygon(pieces)) = clip_to_boundary(&bar, &split) else {
            panic!("two surviving pieces must come back as a multipolygon");
        };
        assert_eq!(pieces.polygons().len(), 2);
        assert!((pieces.area() - 8.0).abs() < 1e-9, "got {}", pieces.area());

        let away =
            FeatureGeometry::Polygon(Polygon::from_coords(rect(40.0, 40.0, 44.0, 44.0).coords()));
        assert!(clip_to_boundary(&away, &boundary()).is_none());
    }

    #[test]
    fn test_clip_to_boundary_recurses_into_collections() {
        let members = vec![
            FeatureGeometry::Polygon(Polygon::from_coords(rect(6.0, 14.0, 10.0, 18.0).coords())),
            FeatureGeometry::Point(Point::new(1.0, 20.0)),
            FeatureGeometry::LineString(LineString::new(unit_line(0, 24))),
        ];
        let Some(FeatureGeometry::GeometryCollection(kept)) =
            clip_to_boundary(&FeatureGeometry::GeometryCollection(members), &boundary())
        else {
            panic!("the surviving members must come back as a collection");
        };
        assert_eq!(kept.len(), 2, "the point outside the boundary is dropped");
        assert!(matches!(kept[0], FeatureGeometry::Polygon(_)));
        assert!(matches!(kept[1], FeatureGeometry::LineString(_)));

        let nested =
            FeatureGeometry::GeometryCollection(vec![FeatureGeometry::GeometryCollection(vec![
                FeatureGeometry::Point(Point::new(1.0, 20.0)),
            ])]);
        assert!(clip_to_boundary(&nested, &boundary()).is_none());
    }
}
