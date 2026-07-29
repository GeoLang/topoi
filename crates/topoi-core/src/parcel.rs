//! Parcel operations: split by line, merge adjacent or overlapping polygons.

use crate::geometry::{Coord, Polygon, signed_ring_area};
use crate::overlay::union;

/// Split a polygon by a line defined by two points.
///
/// Returns two polygons (left and right of the split line).
/// If the line does not intersect the polygon in exactly two places,
/// returns None.
pub fn split_polygon(
    polygon: &[Coord],
    line_start: Coord,
    line_end: Coord,
) -> Option<(Vec<Coord>, Vec<Coord>)> {
    if polygon.len() < 3 {
        return None;
    }

    // Find intersection points of the line with polygon edges
    let mut intersections: Vec<(usize, Coord)> = Vec::new();

    let n = polygon.len();
    for i in 0..n {
        let a = polygon[i];
        let b = polygon[(i + 1) % n];
        if let Some(pt) = line_segment_intersection(line_start, line_end, a, b) {
            intersections.push((i, pt));
        }
    }

    if intersections.len() != 2 {
        return None;
    }

    let (idx1, pt1) = intersections[0];
    let (idx2, pt2) = intersections[1];

    // Build two sub-polygons
    // Polygon A: pt1 → edges from idx1+1 to idx2 → pt2 → back to pt1
    // Polygon B: pt2 → edges from idx2+1 to idx1 → pt1 → back to pt2
    let mut poly_a = Vec::new();
    let mut poly_b = Vec::new();

    poly_a.push(pt1);
    let mut i = (idx1 + 1) % n;
    loop {
        poly_a.push(polygon[i]);
        if i == idx2 {
            break;
        }
        i = (i + 1) % n;
    }
    poly_a.push(pt2);

    poly_b.push(pt2);
    i = (idx2 + 1) % n;
    loop {
        poly_b.push(polygon[i]);
        if i == idx1 {
            break;
        }
        i = (i + 1) % n;
    }
    poly_b.push(pt1);

    Some((poly_a, poly_b))
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

/// Line-segment intersection. Returns intersection point if segments cross.
fn line_segment_intersection(p1: Coord, p2: Coord, p3: Coord, p4: Coord) -> Option<Coord> {
    let d1x = p2.x - p1.x;
    let d1y = p2.y - p1.y;
    let d2x = p4.x - p3.x;
    let d2y = p4.y - p3.y;

    let denom = d1x * d2y - d1y * d2x;
    if denom.abs() < 1e-12 {
        return None; // parallel
    }

    let t = ((p3.x - p1.x) * d2y - (p3.y - p1.y) * d2x) / denom;
    let u = ((p3.x - p1.x) * d1y - (p3.y - p1.y) * d1x) / denom;

    // t can be any value (infinite line), u must be in [0,1] (segment)
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    // For split operations, t should also indicate the line crosses through
    // (we allow any t since split line is infinite)
    let _ = t;

    Some(Coord::new(p3.x + u * d2x, p3.y + u * d2y))
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

    #[test]
    fn test_split_square_vertically() {
        let sq = square();
        let result = split_polygon(&sq, Coord::new(5.0, -1.0), Coord::new(5.0, 11.0));
        assert!(result.is_some());
        let (a, b) = result.unwrap();
        let area_a = polygon_area(&a).abs();
        let area_b = polygon_area(&b).abs();
        assert!((area_a - 50.0).abs() < 1e-6);
        assert!((area_b - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_split_square_horizontally() {
        let sq = square();
        let result = split_polygon(&sq, Coord::new(-1.0, 5.0), Coord::new(11.0, 5.0));
        assert!(result.is_some());
        let (a, b) = result.unwrap();
        let area_a = polygon_area(&a).abs();
        let area_b = polygon_area(&b).abs();
        assert!((area_a - 50.0).abs() < 1e-6);
        assert!((area_b - 50.0).abs() < 1e-6);
    }

    #[test]
    fn test_split_no_intersection() {
        let sq = square();
        // Line outside polygon
        let result = split_polygon(&sq, Coord::new(20.0, 0.0), Coord::new(20.0, 10.0));
        assert!(result.is_none());
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
