use crate::geometry::{Coord, Polygon};
use crate::overlay;

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
    use crate::geometry::signed_ring_area;

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
}
