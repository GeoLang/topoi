use crate::geometry::Coord;

/// Sutherland-Hodgman polygon clipping algorithm.
///
/// Clips a subject polygon against a convex clip polygon.
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
/// More efficient than general polygon clipping for rectangular clips.
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

/// Compute the intersection (boolean AND) of two convex polygons.
///
/// Uses Sutherland-Hodgman since clipping a convex polygon by a convex polygon
/// always produces a convex result.
pub fn polygon_intersection(poly_a: &[Coord], poly_b: &[Coord]) -> Vec<Coord> {
    clip_polygon(poly_a, poly_b)
}

/// Compute the area of a polygon using the shoelace formula.
fn polygon_area(vertices: &[Coord]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = vertices.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += vertices[i].x * vertices[j].y;
        area -= vertices[j].x * vertices[i].y;
    }
    area.abs() / 2.0
}

/// Compute the intersection area of two convex polygons.
pub fn intersection_area(poly_a: &[Coord], poly_b: &[Coord]) -> f64 {
    let clipped = polygon_intersection(poly_a, poly_b);
    polygon_area(&clipped)
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

/// Compute the union area of two convex polygons.
/// Union area = area(A) + area(B) - intersection_area(A, B)
pub fn union_area(poly_a: &[Coord], poly_b: &[Coord]) -> f64 {
    polygon_area(poly_a) + polygon_area(poly_b) - intersection_area(poly_a, poly_b)
}

#[cfg(test)]
mod tests {
    use super::*;

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
