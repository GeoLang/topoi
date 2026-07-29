// General polygon overlay: concave rings, holes, shared edges, multipolygons.

use topoi_core::*;

const EPS: f64 = 1e-9;

fn ring(coords: &[(f64, f64)]) -> Ring {
    let mut c: Vec<Coord> = coords.iter().map(|&(x, y)| Coord::new(x, y)).collect();
    if c.first() != c.last() {
        c.push(c[0]);
    }
    Ring::new(c)
}

fn square(min: f64, max: f64) -> Polygon {
    Polygon::new(
        ring(&[(min, min), (max, min), (max, max), (min, max)]),
        vec![],
    )
}

fn rect(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon {
    Polygon::new(
        ring(&[
            (min_x, min_y),
            (max_x, min_y),
            (max_x, max_y),
            (min_x, max_y),
        ]),
        vec![],
    )
}

/// An L shape inside the 0..4 box, area 12: the 4x4 square with the top-right
/// 2x2 quadrant removed.
fn l_shape() -> Polygon {
    Polygon::new(
        ring(&[
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 2.0),
            (2.0, 2.0),
            (2.0, 4.0),
            (0.0, 4.0),
        ]),
        vec![],
    )
}

/// A 10x10 square with a centred 2x2 hole, area 96.
fn square_with_hole() -> Polygon {
    Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)])],
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Concave inputs
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_concave_subject_area_is_preserved() {
    // Sanity check on the fixture before using it below.
    assert!((l_shape().area() - 12.0).abs() < EPS);
}

#[test]
fn test_intersection_with_concave_subject() {
    // The right half of the L: x in 2..4, y in 0..2, so area 4.
    // Sutherland-Hodgman would clip the concave L against this box wrongly.
    let result = intersection(&l_shape(), &rect(2.0, 0.0, 4.0, 4.0));
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 4.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_union_with_concave_subject_fills_notch() {
    let result = union(&l_shape(), &rect(2.0, 2.0, 4.0, 4.0));
    assert_eq!(result.polygons().len(), 1);
    // L (12) plus the missing quadrant (4) is the full 4x4 square.
    assert!((result.area() - 16.0).abs() < EPS, "got {}", result.area());
    assert_eq!(result.polygons()[0].interiors().len(), 0);
}

#[test]
fn test_difference_splits_into_two_pieces() {
    // A horizontal band through the middle of a square leaves top and bottom.
    let result = difference(&square(0.0, 4.0), &rect(-1.0, 1.0, 5.0, 3.0));
    assert_eq!(result.polygons().len(), 2);
    assert!((result.area() - 8.0).abs() < EPS, "got {}", result.area());
}

// ═══════════════════════════════════════════════════════════════════════════
// Holes
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_difference_creates_a_hole() {
    let result = difference(&square(0.0, 10.0), &square(4.0, 6.0));
    assert_eq!(result.polygons().len(), 1);
    assert_eq!(result.polygons()[0].interiors().len(), 1);
    assert!((result.area() - 96.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_subject_hole_survives_intersection() {
    let result = intersection(&square_with_hole(), &rect(0.0, 0.0, 10.0, 10.0));
    assert_eq!(result.polygons().len(), 1);
    assert_eq!(result.polygons()[0].interiors().len(), 1);
    assert!((result.area() - 96.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_union_fills_a_hole() {
    let result = union(&square_with_hole(), &square(4.0, 6.0));
    assert_eq!(result.polygons().len(), 1);
    assert_eq!(result.polygons()[0].interiors().len(), 0);
    assert!((result.area() - 100.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_intersection_inside_a_hole_is_empty() {
    let result = intersection(&square_with_hole(), &square(4.5, 5.5));
    assert!(result.polygons().is_empty());
    assert_eq!(result.area(), 0.0);
}

#[test]
fn test_hole_winding_is_normalized() {
    // Same polygon, but the hole is wound counter-clockwise like the exterior.
    // The hole must still read as a hole.
    let ccw_hole = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)])],
    );
    let cw_hole = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (4.0, 6.0), (6.0, 6.0), (6.0, 4.0)])],
    );
    assert!(ccw_hole.interiors()[0].is_ccw());
    assert!(!cw_hole.interiors()[0].is_ccw());

    let a = intersection(&ccw_hole, &rect(0.0, 0.0, 10.0, 10.0));
    let b = intersection(&cw_hole, &rect(0.0, 0.0, 10.0, 10.0));
    assert!((a.area() - 96.0).abs() < EPS, "got {}", a.area());
    assert!((b.area() - 96.0).abs() < EPS, "got {}", b.area());
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared edges and touching inputs
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_union_across_shared_edge() {
    let left = rect(0.0, 0.0, 2.0, 2.0);
    let right = rect(2.0, 0.0, 4.0, 2.0);
    let result = union(&left, &right);
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 8.0).abs() < EPS, "got {}", result.area());
    // The shared edge is gone, so the merged rectangle has 4 distinct corners.
    assert_eq!(result.polygons()[0].exterior().coords().len(), 5);
}

#[test]
fn test_intersection_across_shared_edge_is_empty() {
    let left = rect(0.0, 0.0, 2.0, 2.0);
    let right = rect(2.0, 0.0, 4.0, 2.0);
    let result = intersection(&left, &right);
    assert!(result.polygons().is_empty());
}

#[test]
fn test_union_of_identical_polygons() {
    let result = union(&l_shape(), &l_shape());
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 12.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_difference_of_identical_polygons_is_empty() {
    let result = difference(&l_shape(), &l_shape());
    assert!(result.polygons().is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// Disjoint and empty inputs
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_union_of_disjoint_polygons() {
    let result = union(&square(0.0, 2.0), &square(5.0, 7.0));
    assert_eq!(result.polygons().len(), 2);
    assert!((result.area() - 8.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_intersection_of_disjoint_polygons_is_empty() {
    let result = intersection(&square(0.0, 2.0), &square(5.0, 7.0));
    assert!(result.polygons().is_empty());
    assert_eq!(result.area(), 0.0);
}

#[test]
fn test_difference_with_disjoint_clip_returns_subject() {
    let result = difference(&square(0.0, 2.0), &square(5.0, 7.0));
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 4.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_difference_by_covering_clip_is_empty() {
    let result = difference(&square(1.0, 2.0), &square(0.0, 10.0));
    assert!(result.polygons().is_empty());
}

#[test]
fn test_empty_operands() {
    let empty = MultiPolygon::new(vec![]);
    assert!(union(&empty, &empty).polygons().is_empty());
    assert!(
        intersection(&empty, &square(0.0, 1.0))
            .polygons()
            .is_empty()
    );
    assert!(difference(&empty, &square(0.0, 1.0)).polygons().is_empty());

    let from_empty = union(&empty, &square(0.0, 2.0));
    assert!((from_empty.area() - 4.0).abs() < EPS);
}

// ═══════════════════════════════════════════════════════════════════════════
// MultiPolygon operands
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_multipolygon_subject_intersection() {
    // Two 2x2 squares with a gap, clipped by a band that crosses both.
    let subject = MultiPolygon::new(vec![rect(0.0, 0.0, 2.0, 2.0), rect(4.0, 0.0, 6.0, 2.0)]);
    let result = intersection(&subject, &rect(-1.0, 0.5, 7.0, 1.5));
    assert_eq!(result.polygons().len(), 2);
    assert!((result.area() - 4.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_multipolygon_union_merges_overlapping_members() {
    let subject = MultiPolygon::new(vec![rect(0.0, 0.0, 3.0, 2.0), rect(2.0, 0.0, 5.0, 2.0)]);
    let clip = MultiPolygon::new(vec![rect(5.0, 0.0, 6.0, 2.0)]);
    let result = union(&subject, &clip);
    assert_eq!(result.polygons().len(), 1);
    // 0..6 by 0..2, with the 2..3 overlap counted once.
    assert!((result.area() - 12.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_vec_and_slice_operands() {
    let subject: Vec<Polygon> = vec![rect(0.0, 0.0, 2.0, 2.0), rect(4.0, 0.0, 6.0, 2.0)];
    let bridge = rect(2.0, 0.0, 4.0, 2.0);

    let from_vec = union(&subject, &bridge);
    let from_slice = union(&subject[..], &bridge);

    for result in [from_vec, from_slice] {
        assert_eq!(result.polygons().len(), 1);
        assert!((result.area() - 12.0).abs() < EPS, "got {}", result.area());
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Xor and boolean_op dispatch
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_xor_of_overlapping_squares() {
    let a = square(0.0, 2.0);
    let b = square(1.0, 3.0);
    let result = xor(&a, &b);
    // union 7 minus intersection 1.
    assert!((result.area() - 6.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_xor_of_nested_squares_makes_a_hole() {
    let result = xor(&square(0.0, 10.0), &square(4.0, 6.0));
    assert_eq!(result.polygons().len(), 1);
    assert_eq!(result.polygons()[0].interiors().len(), 1);
    assert!((result.area() - 96.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_boolean_op_matches_named_functions() {
    let a = l_shape();
    let b = rect(1.0, 1.0, 5.0, 5.0);
    for (op, expected) in [
        (BooleanOp::Union, union(&a, &b).area()),
        (BooleanOp::Intersection, intersection(&a, &b).area()),
        (BooleanOp::Difference, difference(&a, &b).area()),
        (BooleanOp::Xor, xor(&a, &b).area()),
    ] {
        assert!(
            (boolean_op(&a, &b, op).area() - expected).abs() < EPS,
            "{op:?} disagrees"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Routed legacy helpers
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_intersection_area_handles_concave_rings() {
    // Concave L against the box covering its right column.
    let l: Vec<Coord> = l_shape().exterior().coords().to_vec();
    let clip: Vec<Coord> = rect(2.0, 0.0, 4.0, 4.0).exterior().coords().to_vec();
    assert!((intersection_area(&l, &clip) - 4.0).abs() < EPS);
}

#[test]
fn test_union_area_of_disjoint_rings() {
    let a: Vec<Coord> = square(0.0, 2.0).exterior().coords().to_vec();
    let b: Vec<Coord> = square(5.0, 7.0).exterior().coords().to_vec();
    assert!((union_area(&a, &b) - 8.0).abs() < EPS);
}

#[test]
fn test_merge_polygons_handles_overlap() {
    // These overlap rather than share an edge, which the old shared-vertex
    // merge could not do.
    let a = vec![
        Coord::new(0.0, 0.0),
        Coord::new(3.0, 0.0),
        Coord::new(3.0, 2.0),
        Coord::new(0.0, 2.0),
    ];
    let b = vec![
        Coord::new(2.0, 0.0),
        Coord::new(5.0, 0.0),
        Coord::new(5.0, 2.0),
        Coord::new(2.0, 2.0),
    ];
    let merged = parcel::merge_polygons(&a, &b).unwrap();
    assert!((parcel::polygon_area(&merged).abs() - 10.0).abs() < EPS);
}

#[test]
fn test_merge_polygons_rejects_result_with_hole() {
    // Two C shapes facing each other enclose a hole, which a single ring
    // cannot express.
    let left = vec![
        Coord::new(0.0, 0.0),
        Coord::new(3.0, 0.0),
        Coord::new(3.0, 1.0),
        Coord::new(1.0, 1.0),
        Coord::new(1.0, 3.0),
        Coord::new(3.0, 3.0),
        Coord::new(3.0, 4.0),
        Coord::new(0.0, 4.0),
    ];
    let right = vec![
        Coord::new(3.0, 0.0),
        Coord::new(4.0, 0.0),
        Coord::new(4.0, 4.0),
        Coord::new(3.0, 4.0),
    ];
    assert!(parcel::merge_polygons(&left, &right).is_none());
}
