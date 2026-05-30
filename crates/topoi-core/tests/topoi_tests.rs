// Comprehensive integration tests for topoi-core.

use topoi_core::*;

// ═══════════════════════════════════════════════════════════════════════════
// Geometry basics
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_coord_distance() {
    let a = Coord::new(0.0, 0.0);
    let b = Coord::new(3.0, 4.0);
    assert!((a.distance_to(&b) - 5.0).abs() < 1e-10);
}

#[test]
fn test_coord_distance_same_point() {
    let a = Coord::new(5.0, 5.0);
    assert_eq!(a.distance_to(&a), 0.0);
}

#[test]
fn test_ring_area_unit_square() {
    let ring = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(1.0, 1.0),
        Coord::new(0.0, 1.0),
        Coord::new(0.0, 0.0),
    ]);
    assert!((ring.area() - 1.0).abs() < 1e-10);
}

#[test]
fn test_ring_is_closed() {
    let closed = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(1.0, 1.0),
        Coord::new(0.0, 0.0),
    ]);
    assert!(closed.is_closed());

    let open = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(1.0, 1.0),
    ]);
    assert!(!open.is_closed());
}

#[test]
fn test_ring_ccw_winding() {
    // CCW ring
    let ccw = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(1.0, 1.0),
        Coord::new(0.0, 1.0),
        Coord::new(0.0, 0.0),
    ]);
    assert!(ccw.is_ccw());

    // CW ring (reversed)
    let cw = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(0.0, 1.0),
        Coord::new(1.0, 1.0),
        Coord::new(1.0, 0.0),
        Coord::new(0.0, 0.0),
    ]);
    assert!(!cw.is_ccw());
}

#[test]
fn test_linestring_length() {
    let ls = LineString::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(3.0, 0.0),
        Coord::new(3.0, 4.0),
    ]);
    // 3 + 4 = 7
    assert!((ls.length() - 7.0).abs() < 1e-10);
}

#[test]
fn test_polygon_area_with_hole() {
    let ext = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(10.0, 0.0),
        Coord::new(10.0, 10.0),
        Coord::new(0.0, 10.0),
        Coord::new(0.0, 0.0),
    ]);
    let hole = Ring::new(vec![
        Coord::new(2.0, 2.0),
        Coord::new(4.0, 2.0),
        Coord::new(4.0, 4.0),
        Coord::new(2.0, 4.0),
        Coord::new(2.0, 2.0),
    ]);
    let poly = Polygon::new(ext, vec![hole]);
    // 100 - 4 = 96
    assert!((poly.area() - 96.0).abs() < 1e-10);
}

#[test]
fn test_polygon_centroid() {
    let ext = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(4.0, 0.0),
        Coord::new(4.0, 4.0),
        Coord::new(0.0, 4.0),
        Coord::new(0.0, 0.0),
    ]);
    let poly = Polygon::new(ext, vec![]);
    let c = poly.centroid();
    assert!((c.x - 2.0).abs() < 1e-10);
    assert!((c.y - 2.0).abs() < 1e-10);
}

// ═══════════════════════════════════════════════════════════════════════════
// Envelope tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_envelope_from_coords() {
    let coords = vec![
        Coord::new(1.0, 2.0),
        Coord::new(5.0, 8.0),
        Coord::new(3.0, 4.0),
    ];
    let env = Envelope::from_coords(&coords).unwrap();
    assert_eq!(env.min_x, 1.0);
    assert_eq!(env.min_y, 2.0);
    assert_eq!(env.max_x, 5.0);
    assert_eq!(env.max_y, 8.0);
}

#[test]
fn test_envelope_dimensions() {
    let env = Envelope::new(0.0, 0.0, 10.0, 5.0);
    assert_eq!(env.width(), 10.0);
    assert_eq!(env.height(), 5.0);
    assert_eq!(env.area(), 50.0);
}

#[test]
fn test_envelope_contains_coord() {
    let env = Envelope::new(0.0, 0.0, 10.0, 10.0);
    assert!(env.contains_coord(&Coord::new(5.0, 5.0)));
    assert!(!env.contains_coord(&Coord::new(11.0, 5.0)));
}

#[test]
fn test_envelope_intersects() {
    let a = Envelope::new(0.0, 0.0, 5.0, 5.0);
    let b = Envelope::new(3.0, 3.0, 8.0, 8.0);
    let c = Envelope::new(6.0, 6.0, 10.0, 10.0);
    assert!(a.intersects(&b));
    assert!(!a.intersects(&c));
}

// ═══════════════════════════════════════════════════════════════════════════
// Predicates
// ═══════════════════════════════════════════════════════════════════════════

fn square_polygon() -> Polygon {
    Polygon::new(
        Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(4.0, 4.0),
            Coord::new(0.0, 4.0),
            Coord::new(0.0, 0.0),
        ]),
        vec![],
    )
}

#[test]
fn test_contains_inside() {
    assert!(contains(&square_polygon(), &Coord::new(2.0, 2.0)));
}

#[test]
fn test_contains_outside() {
    assert!(!contains(&square_polygon(), &Coord::new(5.0, 5.0)));
}

#[test]
fn test_contains_with_hole() {
    let ext = Ring::new(vec![
        Coord::new(0.0, 0.0),
        Coord::new(10.0, 0.0),
        Coord::new(10.0, 10.0),
        Coord::new(0.0, 10.0),
        Coord::new(0.0, 0.0),
    ]);
    let hole = Ring::new(vec![
        Coord::new(3.0, 3.0),
        Coord::new(7.0, 3.0),
        Coord::new(7.0, 7.0),
        Coord::new(3.0, 7.0),
        Coord::new(3.0, 3.0),
    ]);
    let poly = Polygon::new(ext, vec![hole]);
    assert!(contains(&poly, &Coord::new(1.0, 1.0))); // inside exterior, outside hole
    assert!(!contains(&poly, &Coord::new(5.0, 5.0))); // inside hole
}

#[test]
fn test_intersects_overlapping() {
    let a = square_polygon();
    let b = Polygon::new(
        Ring::new(vec![
            Coord::new(2.0, 2.0),
            Coord::new(6.0, 2.0),
            Coord::new(6.0, 6.0),
            Coord::new(2.0, 6.0),
            Coord::new(2.0, 2.0),
        ]),
        vec![],
    );
    assert!(intersects(&a, &b));
}

#[test]
fn test_intersects_separate() {
    let a = square_polygon();
    let b = Polygon::new(
        Ring::new(vec![
            Coord::new(10.0, 10.0),
            Coord::new(14.0, 10.0),
            Coord::new(14.0, 14.0),
            Coord::new(10.0, 14.0),
            Coord::new(10.0, 10.0),
        ]),
        vec![],
    );
    assert!(!intersects(&a, &b));
}

// ═══════════════════════════════════════════════════════════════════════════
// Algorithms
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_convex_hull_square() {
    let points = vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(0.5, 0.5), // interior point
        Coord::new(1.0, 1.0),
        Coord::new(0.0, 1.0),
    ];
    let hull = convex_hull(&points);
    // Hull should have 5 coords (4 corners + closing point)
    assert_eq!(hull.exterior().coords().len(), 5);
    // Interior point should not be on hull
    assert!((hull.area() - 1.0).abs() < 1e-10);
}

#[test]
fn test_convex_hull_triangle() {
    let points = vec![
        Coord::new(0.0, 0.0),
        Coord::new(4.0, 0.0),
        Coord::new(2.0, 3.0),
    ];
    let hull = convex_hull(&points);
    assert!(hull.area() > 0.0);
}

#[test]
fn test_segment_intersection_crossing() {
    let result = segment_intersection(
        Coord::new(0.0, 0.0),
        Coord::new(4.0, 4.0),
        Coord::new(0.0, 4.0),
        Coord::new(4.0, 0.0),
    );
    let p = result.unwrap();
    assert!((p.x - 2.0).abs() < 1e-10);
    assert!((p.y - 2.0).abs() < 1e-10);
}

#[test]
fn test_segment_intersection_parallel() {
    let result = segment_intersection(
        Coord::new(0.0, 0.0),
        Coord::new(4.0, 0.0),
        Coord::new(0.0, 1.0),
        Coord::new(4.0, 1.0),
    );
    assert!(result.is_none());
}

#[test]
fn test_segment_intersection_non_crossing() {
    let result = segment_intersection(
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.0),
        Coord::new(2.0, 0.0),
        Coord::new(3.0, 0.0),
    );
    assert!(result.is_none());
}

#[test]
fn test_simplify_already_simple() {
    let coords = vec![Coord::new(0.0, 0.0), Coord::new(10.0, 10.0)];
    let result = simplify(&coords, 1.0);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_simplify_removes_redundant() {
    let coords = vec![
        Coord::new(0.0, 0.0),
        Coord::new(1.0, 0.01), // nearly on the line
        Coord::new(2.0, 0.0),
        Coord::new(3.0, 0.0),
        Coord::new(4.0, 0.0),
    ];
    let result = simplify(&coords, 0.1);
    assert!(result.len() < coords.len());
}

// ═══════════════════════════════════════════════════════════════════════════
// Delaunay triangulation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_delaunay_basic() {
    let points = vec![
        Coord::new(0.0, 0.0),
        Coord::new(4.0, 0.0),
        Coord::new(2.0, 3.0),
        Coord::new(4.0, 3.0),
    ];
    let tri = delaunay(&points).unwrap();
    assert!(!tri.triangles.is_empty());
    // 4 points should produce 2 triangles
    assert_eq!(tri.triangles.len(), 2);
}

#[test]
fn test_delaunay_grid() {
    // 3x3 grid of points = 9 points
    let mut points = Vec::new();
    for y in 0..3 {
        for x in 0..3 {
            points.push(Coord::new(x as f64, y as f64));
        }
    }
    let tri = delaunay(&points).unwrap();
    assert!(!tri.triangles.is_empty());
    // 9 points on a grid produces 8 triangles
    assert_eq!(tri.triangles.len(), 8);
}
