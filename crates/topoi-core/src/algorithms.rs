use crate::geometry::{Coord, Polygon, Ring};

/// Compute the convex hull of a set of points using the Graham scan algorithm.
pub fn convex_hull(points: &[Coord]) -> Polygon {
    if points.len() < 3 {
        return Polygon::new(Ring::new(points.to_vec()), vec![]);
    }

    // Find the lowest point (and leftmost if tied)
    let mut start = 0;
    for i in 1..points.len() {
        if points[i].y < points[start].y
            || (points[i].y == points[start].y && points[i].x < points[start].x)
        {
            start = i;
        }
    }

    let pivot = points[start];

    // Sort by polar angle relative to pivot
    let mut sorted: Vec<Coord> = points.to_vec();
    sorted.swap(0, start);
    sorted[1..].sort_by(|a, b| {
        let angle_a = (a.y - pivot.y).atan2(a.x - pivot.x);
        let angle_b = (b.y - pivot.y).atan2(b.x - pivot.x);
        angle_a
            .partial_cmp(&angle_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Graham scan
    let mut hull: Vec<Coord> = Vec::new();
    for p in &sorted {
        while hull.len() >= 2 {
            let n = hull.len();
            if cross(&hull[n - 2], &hull[n - 1], p) <= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(*p);
    }

    // Close the ring
    if hull.len() >= 3 {
        hull.push(hull[0]);
    }

    Polygon::new(Ring::new(hull), vec![])
}

/// Cross product of vectors OA and OB (positive = CCW turn).
fn cross(o: &Coord, a: &Coord, b: &Coord) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Test if two line segments (p1-p2) and (p3-p4) intersect.
/// Returns the intersection point if they do.
pub fn segment_intersection(p1: Coord, p2: Coord, p3: Coord, p4: Coord) -> Option<Coord> {
    let d1x = p2.x - p1.x;
    let d1y = p2.y - p1.y;
    let d2x = p4.x - p3.x;
    let d2y = p4.y - p3.y;

    let denom = d1x * d2y - d1y * d2x;
    if denom.abs() < 1e-12 {
        return None; // Parallel or collinear
    }

    let t = ((p3.x - p1.x) * d2y - (p3.y - p1.y) * d2x) / denom;
    let u = ((p3.x - p1.x) * d1y - (p3.y - p1.y) * d1x) / denom;

    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some(Coord::new(p1.x + t * d1x, p1.y + t * d1y))
    } else {
        None
    }
}

/// Simplify a linestring using the Douglas-Peucker algorithm.
pub fn simplify(coords: &[Coord], epsilon: f64) -> Vec<Coord> {
    if coords.len() <= 2 {
        return coords.to_vec();
    }

    // Find the point with the maximum distance from the line between first and last
    let mut max_dist = 0.0;
    let mut max_idx = 0;

    let first = coords[0];
    let last = coords[coords.len() - 1];

    for (i, coord) in coords.iter().enumerate().skip(1).take(coords.len() - 2) {
        let d = point_to_line_distance(coord, &first, &last);
        if d > max_dist {
            max_dist = d;
            max_idx = i;
        }
    }

    if max_dist > epsilon {
        // Recursively simplify
        let mut left = simplify(&coords[..=max_idx], epsilon);
        let right = simplify(&coords[max_idx..], epsilon);
        left.pop(); // Remove duplicate point
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

fn point_to_line_distance(point: &Coord, line_start: &Coord, line_end: &Coord) -> f64 {
    let dx = line_end.x - line_start.x;
    let dy = line_end.y - line_start.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq == 0.0 {
        return point.distance_to(line_start);
    }

    let numerator = ((point.x - line_start.x) * dy - (point.y - line_start.y) * dx).abs();
    numerator / len_sq.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Hull should have 4 unique vertices + closing point = 5
        assert_eq!(hull.exterior().coords().len(), 5);
    }

    #[test]
    fn test_segment_intersection_cross() {
        let p = segment_intersection(
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 2.0),
            Coord::new(0.0, 2.0),
            Coord::new(2.0, 0.0),
        );
        assert!(p.is_some());
        let pt = p.unwrap();
        assert!((pt.x - 1.0).abs() < 1e-10);
        assert!((pt.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_segment_no_intersection() {
        let p = segment_intersection(
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(0.0, 1.0),
            Coord::new(1.0, 1.0),
        );
        assert!(p.is_none());
    }

    #[test]
    fn test_simplify() {
        let coords = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.1),
            Coord::new(2.0, -0.1),
            Coord::new(3.0, 5.0),
            Coord::new(4.0, 6.0),
            Coord::new(5.0, 7.0),
            Coord::new(6.0, 8.1),
            Coord::new(7.0, 9.0),
            Coord::new(8.0, 9.0),
            Coord::new(9.0, 9.0),
        ];
        let simplified = simplify(&coords, 1.0);
        assert!(simplified.len() < coords.len());
        // First and last must be preserved
        assert_eq!(simplified[0], coords[0]);
        assert_eq!(simplified[simplified.len() - 1], coords[coords.len() - 1]);
    }
}
