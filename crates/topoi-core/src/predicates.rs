use crate::geometry::{Coord, Polygon, Ring};

/// Point-in-polygon test using ray casting.
pub fn contains(polygon: &Polygon, point: &Coord) -> bool {
    if !point_in_ring(polygon.exterior(), point) {
        return false;
    }
    // Check that point is not inside any hole
    for hole in polygon.interiors() {
        if point_in_ring(hole, point) {
            return false;
        }
    }
    true
}

/// Test if two polygons' bounding boxes intersect (quick rejection test).
pub fn intersects(a: &Polygon, b: &Polygon) -> bool {
    use crate::Envelope;
    let env_a = Envelope::from_coords(a.exterior().coords());
    let env_b = Envelope::from_coords(b.exterior().coords());
    match (env_a, env_b) {
        (Some(ea), Some(eb)) => ea.intersects(&eb),
        _ => false,
    }
}

/// Ray casting algorithm for point-in-ring.
fn point_in_ring(ring: &Ring, point: &Coord) -> bool {
    let coords = ring.coords();
    let n = coords.len();
    if n < 3 {
        return false;
    }

    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let yi = coords[i].y;
        let yj = coords[j].y;
        if ((yi > point.y) != (yj > point.y))
            && (point.x < (coords[j].x - coords[i].x) * (point.y - yi) / (yj - yi) + coords[i].x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Polygon {
        let ring = Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(4.0, 4.0),
            Coord::new(0.0, 4.0),
            Coord::new(0.0, 0.0),
        ]);
        Polygon::new(ring, vec![])
    }

    #[test]
    fn test_point_inside() {
        assert!(contains(&square(), &Coord::new(2.0, 2.0)));
    }

    #[test]
    fn test_point_outside() {
        assert!(!contains(&square(), &Coord::new(5.0, 5.0)));
    }

    #[test]
    fn test_point_in_hole() {
        let exterior = Ring::new(vec![
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
        let polygon = Polygon::new(exterior, vec![hole]);
        assert!(!contains(&polygon, &Coord::new(5.0, 5.0)));
        assert!(contains(&polygon, &Coord::new(1.0, 1.0)));
    }
}
