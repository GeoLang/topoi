// Buffering and polyline splitting: concave rings, holes, collapse and multi-piece results.

use topoi_core::parcel::split_polygon;
use topoi_core::*;

const EPS: f64 = 1e-6;

fn poly(coords: &[(f64, f64)]) -> Polygon {
    Polygon::from_coords(
        &coords
            .iter()
            .map(|&(x, y)| Coord::new(x, y))
            .collect::<Vec<_>>(),
    )
}

fn ring(coords: &[(f64, f64)]) -> Ring {
    let mut c: Vec<Coord> = coords.iter().map(|&(x, y)| Coord::new(x, y)).collect();
    c.push(c[0]);
    Ring::new(c)
}

fn rect(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon {
    poly(&[
        (min_x, min_y),
        (max_x, min_y),
        (max_x, max_y),
        (min_x, max_y),
    ])
}

/// An L: the 4x4 box with its top-right 2x2 quadrant removed, area 12.
fn l_shape() -> Polygon {
    poly(&[
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0, 2.0),
        (2.0, 2.0),
        (2.0, 4.0),
        (0.0, 4.0),
    ])
}

/// A U opening upward: 6x4 box with a 2x3 notch cut down from the top, area 18.
fn u_shape() -> Polygon {
    poly(&[
        (0.0, 0.0),
        (6.0, 0.0),
        (6.0, 4.0),
        (4.0, 4.0),
        (4.0, 1.0),
        (2.0, 1.0),
        (2.0, 4.0),
        (0.0, 4.0),
    ])
}

/// A comb with three teeth: 10x4 box with two 2x3 notches, area 28.
fn comb() -> Polygon {
    poly(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (8.0, 4.0),
        (8.0, 1.0),
        (6.0, 1.0),
        (6.0, 4.0),
        (4.0, 4.0),
        (4.0, 1.0),
        (2.0, 1.0),
        (2.0, 4.0),
        (0.0, 4.0),
    ])
}

/// Two 4x4 squares joined by a bar one unit tall, area 34.
fn dumbbell() -> Polygon {
    poly(&[
        (0.0, 0.0),
        (4.0, 0.0),
        (4.0, 1.5),
        (6.0, 1.5),
        (6.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (6.0, 4.0),
        (6.0, 2.5),
        (4.0, 2.5),
        (4.0, 4.0),
        (0.0, 4.0),
    ])
}

// ═══════════════════════════════════════════════════════════════════════════
// Buffer: concave inputs
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_buffer_grows_concave_polygon() {
    let l = l_shape();
    let result = buffer_polygon(&l, 1.0);
    assert_eq!(result.polygons().len(), 1);
    assert!(result.area() > l.area());
    // The old bisector buffer blew concave corners out past the true offset.
    // The grown L stays inside the 4x4 box expanded by 1 on every side.
    assert!(result.area() < 36.0, "got {}", result.area());
}

#[test]
fn test_buffer_shrinks_concave_polygon() {
    let result = buffer_polygon(&l_shape(), -0.5);
    assert_eq!(result.polygons().len(), 1);
    assert!(result.area() < l_shape().area());
    assert!(result.area() > 0.0);
}

#[test]
fn test_buffer_square_exact_shrink() {
    // Shrinking a convex ring makes no arcs, so the area is exact.
    let result = buffer_polygon(&rect(0.0, 0.0, 10.0, 10.0), -1.0);
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 64.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_buffer_square_round_corners() {
    // 12x12 with the four corners replaced by quarter arcs of radius 1.
    let expected = 144.0 - 4.0 + std::f64::consts::PI;
    let result = buffer_polygon(&rect(0.0, 0.0, 10.0, 10.0), 1.0);
    assert_eq!(result.polygons().len(), 1);
    // Arcs are approximated by segments, so the polygon is slightly inside.
    let area = result.area();
    assert!(area < expected, "got {area}, expected under {expected}");
    assert!(
        area > expected - 0.1,
        "got {area}, expected near {expected}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Buffer: holes
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_buffer_shrinks_a_hole() {
    let holed = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)])],
    );
    let result = buffer_polygon(&holed, 1.0);
    assert_eq!(result.polygons().len(), 1);
    let out = &result.polygons()[0];
    assert_eq!(out.interiors().len(), 1);
    // The 6x6 hole loses one unit on each side, so it lands near 4x4.
    let hole_area = out.interiors()[0].area();
    assert!(hole_area < 36.0 && hole_area > 12.0, "got {hole_area}");
}

#[test]
fn test_buffer_closes_a_small_hole() {
    let holed = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.5, 4.5), (5.5, 4.5), (5.5, 5.5), (4.5, 5.5)])],
    );
    // A one unit hole cannot survive losing one unit on each side.
    let result = buffer_polygon(&holed, 1.0);
    assert_eq!(result.polygons().len(), 1);
    assert!(result.polygons()[0].interiors().is_empty());
}

#[test]
fn test_negative_buffer_grows_a_hole() {
    let holed = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)])],
    );
    let result = buffer_polygon(&holed, -1.0);
    assert_eq!(result.polygons().len(), 1);
    let out = &result.polygons()[0];
    assert_eq!(out.interiors().len(), 1);
    assert!(out.interiors()[0].area() > 4.0);
}

// ═══════════════════════════════════════════════════════════════════════════
// Buffer: collapse and split
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_negative_buffer_collapses_polygon_to_nothing() {
    let result = buffer_polygon(&rect(0.0, 0.0, 2.0, 2.0), -2.0);
    assert!(result.polygons().is_empty());
    assert_eq!(result.area(), 0.0);
}

#[test]
fn test_negative_buffer_at_exactly_half_width_collapses() {
    // A 2x2 square has an inradius of 1, so -1 leaves nothing with area.
    let result = buffer_polygon(&rect(0.0, 0.0, 2.0, 2.0), -1.0);
    assert!(result.area() < EPS, "got {}", result.area());
}

#[test]
fn test_negative_buffer_splits_dumbbell() {
    let result = buffer_polygon(&dumbbell(), -1.0);
    // The connecting bar is only one unit tall, so it cannot survive.
    assert_eq!(
        result.polygons().len(),
        2,
        "got {:?}",
        result.polygons().len()
    );
    assert!(result.area() > 0.0);
    assert!(result.area() < dumbbell().area());
}

#[test]
fn test_positive_buffer_merges_disjoint_polygons() {
    let pair = MultiPolygon::new(vec![rect(0.0, 0.0, 2.0, 2.0), rect(3.0, 0.0, 5.0, 2.0)]);
    // The gap is one unit, so growing each side by 0.6 closes it.
    let result = buffer_polygon(&pair, 0.6);
    assert_eq!(result.polygons().len(), 1);
}

#[test]
fn test_buffer_keeps_disjoint_polygons_apart() {
    let pair = MultiPolygon::new(vec![rect(0.0, 0.0, 2.0, 2.0), rect(5.0, 0.0, 7.0, 2.0)]);
    let result = buffer_polygon(&pair, 0.5);
    assert_eq!(result.polygons().len(), 2);
}

#[test]
fn test_buffer_empty_input() {
    let empty = MultiPolygon::new(vec![]);
    assert!(buffer_polygon(&empty, 1.0).polygons().is_empty());
    assert!(buffer_polygon(&empty, -1.0).polygons().is_empty());
    assert!(buffer_polygon(&empty, 0.0).polygons().is_empty());
}

#[test]
fn test_buffer_result_winding() {
    let result = buffer_polygon(&rect(0.0, 0.0, 4.0, 4.0), 1.0);
    let out = &result.polygons()[0];
    assert!(out.exterior().is_ccw());
    assert!(out.exterior().is_closed());
}

#[test]
fn test_buffer_join_styles_differ_on_a_sharp_corner() {
    // A thin sliver has a very sharp tip, where miter reaches further than bevel.
    let sliver = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 1.0)]);
    let round = buffer_polygon_with_join(&sliver, 0.5, JoinStyle::Round).area();
    let miter = buffer_polygon_with_join(&sliver, 0.5, JoinStyle::Miter).area();
    let bevel = buffer_polygon_with_join(&sliver, 0.5, JoinStyle::Bevel).area();
    assert!(bevel < round, "bevel {bevel} should be under round {round}");
    assert!(round < miter, "round {round} should be under miter {miter}");
}

// ═══════════════════════════════════════════════════════════════════════════
// Split: concave inputs and more than two pieces
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_split_concave_polygon_into_three_pieces() {
    let u = u_shape();
    let result = split_polygon(&u, &[Coord::new(-1.0, 2.0), Coord::new(7.0, 2.0)]);
    assert_eq!(result.polygons().len(), 3);
    assert!(
        (result.area() - u.area()).abs() < EPS,
        "got {}",
        result.area()
    );
    // The base keeps 10 of the 18, and the two arms take 4 each.
    let mut areas: Vec<f64> = result.polygons().iter().map(|p| p.area()).collect();
    areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert!((areas[0] - 4.0).abs() < EPS, "got {areas:?}");
    assert!((areas[1] - 4.0).abs() < EPS, "got {areas:?}");
    assert!((areas[2] - 10.0).abs() < EPS, "got {areas:?}");
}

#[test]
fn test_split_comb_into_four_pieces() {
    let c = comb();
    let result = split_polygon(&c, &[Coord::new(-1.0, 2.0), Coord::new(11.0, 2.0)]);
    assert_eq!(result.polygons().len(), 4);
    assert!(
        (result.area() - c.area()).abs() < EPS,
        "got {}",
        result.area()
    );
}

#[test]
fn test_split_concave_l_diagonally() {
    let l = l_shape();
    let result = split_polygon(&l, &[Coord::new(-1.0, -1.0), Coord::new(5.0, 5.0)]);
    assert!(result.polygons().len() >= 2);
    assert!(
        (result.area() - l.area()).abs() < EPS,
        "got {}",
        result.area()
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Split: holes, non-crossing lines, polylines
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_split_polygon_with_hole() {
    let holed = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)])],
    );
    let result = split_polygon(&holed, &[Coord::new(-1.0, 5.0), Coord::new(11.0, 5.0)]);
    assert_eq!(result.polygons().len(), 2);
    // The cut passes through the hole, so each half carries a notch, no rings.
    for piece in result.polygons() {
        assert!(piece.interiors().is_empty());
        assert!((piece.area() - 48.0).abs() < EPS, "got {}", piece.area());
    }
    assert!((result.area() - 96.0).abs() < EPS);
}

#[test]
fn test_split_beside_a_hole_keeps_the_hole() {
    let holed = Polygon::new(
        ring(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]),
        vec![ring(&[(4.0, 4.0), (6.0, 4.0), (6.0, 6.0), (4.0, 6.0)])],
    );
    // Cut at y = 2, below the hole entirely.
    let result = split_polygon(&holed, &[Coord::new(-1.0, 2.0), Coord::new(11.0, 2.0)]);
    assert_eq!(result.polygons().len(), 2);
    let with_hole = result
        .polygons()
        .iter()
        .filter(|p| !p.interiors().is_empty())
        .count();
    assert_eq!(with_hole, 1);
    assert!((result.area() - 96.0).abs() < EPS);
}

#[test]
fn test_split_line_stopping_inside_does_not_cut() {
    let sq = rect(0.0, 0.0, 10.0, 10.0);
    let result = split_polygon(&sq, &[Coord::new(-1.0, 5.0), Coord::new(5.0, 5.0)]);
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 100.0).abs() < EPS);
}

#[test]
fn test_split_by_multi_segment_polyline() {
    // A V dipping into the square carves one piece off the bottom edge.
    let sq = rect(0.0, 0.0, 6.0, 4.0);
    let result = split_polygon(
        &sq,
        &[
            Coord::new(1.0, -1.0),
            Coord::new(3.0, 2.0),
            Coord::new(5.0, -1.0),
        ],
    );
    assert_eq!(result.polygons().len(), 2);
    assert!((result.area() - 24.0).abs() < EPS, "got {}", result.area());
}

#[test]
fn test_split_multipolygon_subject() {
    let pair = MultiPolygon::new(vec![rect(0.0, 0.0, 2.0, 2.0), rect(4.0, 0.0, 6.0, 2.0)]);
    let result = split_polygon(&pair, &[Coord::new(-1.0, 1.0), Coord::new(7.0, 1.0)]);
    // Both members get halved.
    assert_eq!(result.polygons().len(), 4);
    assert!((result.area() - 8.0).abs() < EPS);
}

#[test]
fn test_split_degenerate_line_returns_subject() {
    let sq = rect(0.0, 0.0, 4.0, 4.0);
    let result = split_polygon(&sq, &[Coord::new(1.0, 1.0)]);
    assert_eq!(result.polygons().len(), 1);
    assert!((result.area() - 16.0).abs() < EPS);
}

#[test]
fn test_split_empty_subject() {
    let empty = MultiPolygon::new(vec![]);
    let result = split_polygon(&empty, &[Coord::new(0.0, 0.0), Coord::new(1.0, 1.0)]);
    assert!(result.polygons().is_empty());
}
