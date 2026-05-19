use crate::geometry::{Coord, Polygon, Ring};

/// Create a buffer polygon around a convex polygon by offsetting edges outward.
/// This is a simplified implementation that works for convex polygons.
/// For a full implementation, Minkowski sum or Vatti clipping would be used.
pub fn buffer_polygon(polygon: &Polygon, distance: f64) -> Polygon {
    if distance == 0.0 {
        return polygon.clone();
    }

    let coords = polygon.exterior().coords();
    let n = coords.len();
    if n < 4 {
        return polygon.clone();
    }

    // Approximate buffer by offsetting each vertex along the bisector of adjacent edges
    let mut buffered = Vec::with_capacity(n);
    for i in 0..n - 1 {
        let prev = if i == 0 { n - 2 } else { i - 1 };
        let next = (i + 1) % (n - 1);

        // Edge vectors
        let dx1 = coords[i].x - coords[prev].x;
        let dy1 = coords[i].y - coords[prev].y;
        let dx2 = coords[next].x - coords[i].x;
        let dy2 = coords[next].y - coords[i].y;

        // Normals (outward for CCW polygon)
        let len1 = (dx1 * dx1 + dy1 * dy1).sqrt();
        let len2 = (dx2 * dx2 + dy2 * dy2).sqrt();

        if len1 == 0.0 || len2 == 0.0 {
            buffered.push(coords[i]);
            continue;
        }

        let nx1 = dy1 / len1;
        let ny1 = -dx1 / len1;
        let nx2 = dy2 / len2;
        let ny2 = -dx2 / len2;

        // Average normal direction (bisector approximation)
        let nx = nx1 + nx2;
        let ny = ny1 + ny2;
        let nlen = (nx * nx + ny * ny).sqrt();

        if nlen < 1e-10 {
            buffered.push(coords[i]);
            continue;
        }

        // Scale to maintain distance from original edges
        let scale = distance / (nx * nx1 + ny * ny1).max(0.1);
        buffered.push(Coord::new(
            coords[i].x + nx / nlen * scale,
            coords[i].y + ny / nlen * scale,
        ));
    }
    // Close the ring
    if let Some(&first) = buffered.first() {
        buffered.push(first);
    }

    Polygon::new(Ring::new(buffered), vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_buffer() {
        let ring = Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
            Coord::new(0.0, 1.0),
            Coord::new(0.0, 0.0),
        ]);
        let poly = Polygon::new(ring.clone(), vec![]);
        let result = buffer_polygon(&poly, 0.0);
        assert_eq!(result.exterior().coords(), ring.coords());
    }

    #[test]
    fn test_positive_buffer_increases_area() {
        let ring = Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(4.0, 4.0),
            Coord::new(0.0, 4.0),
            Coord::new(0.0, 0.0),
        ]);
        let poly = Polygon::new(ring, vec![]);
        let result = buffer_polygon(&poly, 1.0);
        assert!(result.area() > poly.area());
    }
}
