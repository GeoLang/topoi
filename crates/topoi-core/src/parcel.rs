//! Parcel operations: split by line, merge adjacent or overlapping polygons.

use crate::geometry::{Coord, MultiPolygon, Polygon, signed_ring_area};
use crate::overlay::{PolygonSet, from_shapes, to_shapes, union};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::slice::FloatSlice;

/// Split a polygon set with a cutting polyline.
///
/// The line is used exactly as given, so it has to cross the boundary to cut:
/// a line that stops inside the polygon leaves it whole. Concave rings and holes
/// are handled, and a line may cut the input into any number of pieces, so the
/// result is a `MultiPolygon`.
///
/// ```
/// use topoi_core::parcel::split_polygon;
/// use topoi_core::{Coord, Polygon};
///
/// // A U opening upward, area 18
/// let u = Polygon::from_coords(&[
///     Coord::new(0.0, 0.0), Coord::new(6.0, 0.0), Coord::new(6.0, 4.0),
///     Coord::new(4.0, 4.0), Coord::new(4.0, 1.0), Coord::new(2.0, 1.0),
///     Coord::new(2.0, 4.0), Coord::new(0.0, 4.0),
/// ]);
///
/// // Cutting across both arms leaves the base plus one piece per arm
/// let pieces = split_polygon(&u, &[Coord::new(-1.0, 2.0), Coord::new(7.0, 2.0)]);
/// assert_eq!(pieces.polygons().len(), 3);
/// assert!((pieces.area() - 18.0).abs() < 1e-9);
/// ```
pub fn split_polygon<S>(subject: &S, line: &[Coord]) -> MultiPolygon
where
    S: PolygonSet + ?Sized,
{
    if line.len() < 2 {
        return MultiPolygon::new(subject.as_polygons().to_vec());
    }

    let shapes = to_shapes(subject.as_polygons());
    from_shapes(shapes.slice_by(&line, FillRule::NonZero))
}

/// Merge two adjacent or overlapping polygons into one.
///
/// Runs a general union, so the inputs may be concave and may overlap instead of
/// only touching along an edge. Returns None when the union cannot be expressed
/// as a single ring: disjoint inputs, or a union that encloses a hole.
pub fn merge_polygons(poly_a: &[Coord], poly_b: &[Coord]) -> Option<Vec<Coord>> {
    if poly_a.len() < 3 || poly_b.len() < 3 {
        return None;
    }

    let merged = union(&Polygon::from_coords(poly_a), &Polygon::from_coords(poly_b));
    let [polygon] = merged.polygons() else {
        return None;
    };
    if !polygon.interiors().is_empty() {
        return None;
    }

    // This module treats rings as implicitly closed, so drop the repeated coord.
    let mut result = polygon.exterior().coords().to_vec();
    if result.len() > 1 && result.first() == result.last() {
        result.pop();
    }

    if result.len() >= 3 {
        Some(result)
    } else {
        None
    }
}

/// Compute signed area of a polygon (positive = CCW).
pub fn polygon_area(coords: &[Coord]) -> f64 {
    signed_ring_area(coords)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Coord> {
        vec![
            Coord::new(0.0, 0.0),
            Coord::new(10.0, 0.0),
            Coord::new(10.0, 10.0),
            Coord::new(0.0, 10.0),
        ]
    }

    fn halves(result: &MultiPolygon) {
        assert_eq!(result.polygons().len(), 2);
        for piece in result.polygons() {
            assert!((piece.area() - 50.0).abs() < 1e-6, "got {}", piece.area());
        }
    }

    #[test]
    fn test_split_square_vertically() {
        let sq = Polygon::from_coords(&square());
        let result = split_polygon(&sq, &[Coord::new(5.0, -1.0), Coord::new(5.0, 11.0)]);
        halves(&result);
    }

    #[test]
    fn test_split_square_horizontally() {
        let sq = Polygon::from_coords(&square());
        let result = split_polygon(&sq, &[Coord::new(-1.0, 5.0), Coord::new(11.0, 5.0)]);
        halves(&result);
    }

    #[test]
    fn test_split_no_intersection() {
        let sq = Polygon::from_coords(&square());
        // Line outside polygon
        let result = split_polygon(&sq, &[Coord::new(20.0, 0.0), Coord::new(20.0, 10.0)]);
        assert_eq!(result.polygons().len(), 1);
        assert!((result.area() - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_merge_adjacent_rectangles() {
        // Two rectangles sharing an edge at x=5
        let left = vec![
            Coord::new(0.0, 0.0),
            Coord::new(5.0, 0.0),
            Coord::new(5.0, 10.0),
            Coord::new(0.0, 10.0),
        ];
        let right = vec![
            Coord::new(5.0, 0.0),
            Coord::new(10.0, 0.0),
            Coord::new(10.0, 10.0),
            Coord::new(5.0, 10.0),
        ];
        let result = merge_polygons(&left, &right);
        assert!(result.is_some());
        let merged = result.unwrap();
        let area = polygon_area(&merged).abs();
        assert!((area - 100.0).abs() < 1e-6);
    }

    #[test]
    fn test_merge_no_shared_edge() {
        let a = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
        ];
        let b = vec![
            Coord::new(5.0, 5.0),
            Coord::new(6.0, 5.0),
            Coord::new(6.0, 6.0),
        ];
        assert!(merge_polygons(&a, &b).is_none());
    }

    #[test]
    fn test_polygon_area() {
        let sq = square();
        assert!((polygon_area(&sq).abs() - 100.0).abs() < 1e-6);
    }
}
