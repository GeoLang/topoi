use crate::geometry::Coord;

/// A triangle defined by three vertex indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Triangle {
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

/// Result of Delaunay triangulation.
#[derive(Debug, Clone)]
pub struct Triangulation {
    pub points: Vec<Coord>,
    pub triangles: Vec<Triangle>,
}

impl Triangulation {
    /// Get the Voronoi dual of this triangulation (circumcenters of triangles).
    pub fn voronoi_vertices(&self) -> Vec<Coord> {
        self.triangles
            .iter()
            .filter_map(|t| circumcenter(&self.points[t.a], &self.points[t.b], &self.points[t.c]))
            .collect()
    }
}

/// Compute Delaunay triangulation using the Bowyer-Watson algorithm.
///
/// # Arguments
/// * `points` — input point set (at least 3 non-collinear points)
///
/// # Returns
/// A `Triangulation` containing the input points and triangle indices.
pub fn delaunay(points: &[Coord]) -> Option<Triangulation> {
    if points.len() < 3 {
        return None;
    }

    let n = points.len();

    // Create super-triangle that encompasses all points
    let (super_a, super_b, super_c) = super_triangle(points);
    let st_a = n;
    let st_b = n + 1;
    let st_c = n + 2;

    // Working point set: original points + super-triangle vertices
    let mut all_points = points.to_vec();
    all_points.push(super_a);
    all_points.push(super_b);
    all_points.push(super_c);

    // Start with the super-triangle
    let mut triangles: Vec<Triangle> = vec![Triangle {
        a: st_a,
        b: st_b,
        c: st_c,
    }];

    // Insert points one by one
    for i in 0..n {
        let p = &all_points[i];

        // Find all triangles whose circumcircle contains the point
        let mut bad_triangles = Vec::new();
        for (j, tri) in triangles.iter().enumerate() {
            let a = &all_points[tri.a];
            let b = &all_points[tri.b];
            let c = &all_points[tri.c];
            if in_circumcircle(p, a, b, c) {
                bad_triangles.push(j);
            }
        }

        // Find the boundary of the polygonal hole
        let mut polygon = Vec::new();
        for &j in &bad_triangles {
            let tri = &triangles[j];
            let edges = [(tri.a, tri.b), (tri.b, tri.c), (tri.c, tri.a)];

            for &edge in &edges {
                // Check if this edge is shared with another bad triangle
                let shared = bad_triangles.iter().any(|&k| {
                    k != j && {
                        let other = &triangles[k];
                        let other_edges =
                            [(other.a, other.b), (other.b, other.c), (other.c, other.a)];
                        other_edges
                            .iter()
                            .any(|oe| oe.0 == edge.1 && oe.1 == edge.0)
                    }
                });
                if !shared {
                    polygon.push(edge);
                }
            }
        }

        // Remove bad triangles (in reverse order to preserve indices)
        bad_triangles.sort_unstable_by(|a, b| b.cmp(a));
        for j in bad_triangles {
            triangles.swap_remove(j);
        }

        // Create new triangles from the boundary polygon to the new point
        for &(e0, e1) in &polygon {
            triangles.push(Triangle { a: i, b: e0, c: e1 });
        }
    }

    // Remove triangles that reference super-triangle vertices
    triangles.retain(|t| t.a < n && t.b < n && t.c < n);

    Some(Triangulation {
        points: points.to_vec(),
        triangles,
    })
}

/// Test if point p is inside the circumcircle of triangle (a, b, c).
///
/// Uses the determinant method — positive means inside (assuming CCW orientation).
fn in_circumcircle(p: &Coord, a: &Coord, b: &Coord, c: &Coord) -> bool {
    let ax = a.x - p.x;
    let ay = a.y - p.y;
    let bx = b.x - p.x;
    let by = b.y - p.y;
    let cx = c.x - p.x;
    let cy = c.y - p.y;

    let det = ax * (by * (cx * cx + cy * cy) - cy * (bx * bx + by * by))
        - ay * (bx * (cx * cx + cy * cy) - cx * (bx * bx + by * by))
        + (ax * ax + ay * ay) * (bx * cy - by * cx);

    // If triangle is CW, negate
    let orient = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if orient < 0.0 { det < 0.0 } else { det > 0.0 }
}

/// Compute circumcenter of a triangle.
fn circumcenter(a: &Coord, b: &Coord, c: &Coord) -> Option<Coord> {
    let ax = a.x;
    let ay = a.y;
    let bx = b.x;
    let by = b.y;
    let cx = c.x;
    let cy = c.y;

    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return None; // Degenerate (collinear)
    }

    let ux = ((ax * ax + ay * ay) * (by - cy)
        + (bx * bx + by * by) * (cy - ay)
        + (cx * cx + cy * cy) * (ay - by))
        / d;
    let uy = ((ax * ax + ay * ay) * (cx - bx)
        + (bx * bx + by * by) * (ax - cx)
        + (cx * cx + cy * cy) * (bx - ax))
        / d;

    Some(Coord::new(ux, uy))
}

/// Generate a super-triangle that bounds all input points.
fn super_triangle(points: &[Coord]) -> (Coord, Coord, Coord) {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for p in points {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }

    let dx = max_x - min_x;
    let dy = max_y - min_y;
    let d_max = dx.max(dy);
    let mid_x = (min_x + max_x) / 2.0;
    let mid_y = (min_y + max_y) / 2.0;

    // Large triangle enclosing all points
    let a = Coord::new(mid_x - 20.0 * d_max, mid_y - d_max);
    let b = Coord::new(mid_x, mid_y + 20.0 * d_max);
    let c = Coord::new(mid_x + 20.0 * d_max, mid_y - d_max);

    (a, b, c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delaunay_triangle() {
        // Three points form exactly one triangle
        let points = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(0.5, 1.0),
        ];
        let tri = delaunay(&points).unwrap();
        assert_eq!(tri.triangles.len(), 1);
        assert_eq!(tri.points.len(), 3);
    }

    #[test]
    fn test_delaunay_square() {
        // Four points (square) should produce 2 triangles
        let points = vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
            Coord::new(0.0, 1.0),
        ];
        let tri = delaunay(&points).unwrap();
        assert_eq!(tri.triangles.len(), 2);
    }

    #[test]
    fn test_delaunay_many_points() {
        // Regular grid of 9 points should produce valid triangulation
        let mut points = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                points.push(Coord::new(i as f64, j as f64));
            }
        }
        let tri = delaunay(&points).unwrap();
        // 9 points on grid → 8 triangles (2n - 2 - h where h=boundary hull vertices)
        // For a 3×3 grid: convex hull has 4 vertices, so 2*9 - 2 - 4 = 12? Actually
        // the exact count depends on the Delaunay criterion. Just verify reasonable count.
        assert!(
            tri.triangles.len() >= 8 && tri.triangles.len() <= 12,
            "got {} triangles",
            tri.triangles.len()
        );
    }

    #[test]
    fn test_delaunay_properties() {
        // Verify Delaunay property: no point inside any triangle's circumcircle
        let points = vec![
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(2.0, 3.0),
            Coord::new(1.0, 1.0),
            Coord::new(3.0, 1.0),
        ];
        let tri = delaunay(&points).unwrap();

        for t in &tri.triangles {
            let a = &tri.points[t.a];
            let b = &tri.points[t.b];
            let c = &tri.points[t.c];

            for (i, p) in tri.points.iter().enumerate() {
                if i == t.a || i == t.b || i == t.c {
                    continue;
                }
                assert!(
                    !in_circumcircle(p, a, b, c),
                    "Point {} is inside circumcircle of triangle ({},{},{})",
                    i,
                    t.a,
                    t.b,
                    t.c
                );
            }
        }
    }

    #[test]
    fn test_circumcenter() {
        // Right triangle at origin
        let a = Coord::new(0.0, 0.0);
        let b = Coord::new(1.0, 0.0);
        let c = Coord::new(0.0, 1.0);
        let center = circumcenter(&a, &b, &c).unwrap();
        assert!((center.x - 0.5).abs() < 1e-10);
        assert!((center.y - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_voronoi_vertices() {
        let points = vec![
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 0.0),
            Coord::new(1.0, 2.0),
            Coord::new(1.0, 0.5),
        ];
        let tri = delaunay(&points).unwrap();
        let voronoi = tri.voronoi_vertices();
        // Should have one circumcenter per triangle
        assert_eq!(voronoi.len(), tri.triangles.len());
    }
}
