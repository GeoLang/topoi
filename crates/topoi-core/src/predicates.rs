use crate::Envelope;
use crate::algorithms::segment_intersection;
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

/// Test if two polygons intersect.
pub fn intersects(a: &Polygon, b: &Polygon) -> bool {
    let env_a = Envelope::from_coords(a.exterior().coords());
    let env_b = Envelope::from_coords(b.exterior().coords());
    match (env_a, env_b) {
        (Some(ea), Some(eb)) if ea.intersects(&eb) => {}
        _ => return false,
    }

    if edges_intersect(a, b) {
        return true;
    }
    vertex_in_polygon(a, b) || vertex_in_polygon(b, a)
}

fn edges_intersect(a: &Polygon, b: &Polygon) -> bool {
    for ra in polygon_rings(a) {
        for rb in polygon_rings(b) {
            if ring_edges_intersect(ra, rb) {
                return true;
            }
        }
    }
    false
}

fn ring_edges_intersect(a: &Ring, b: &Ring) -> bool {
    let ac = open_coords(a);
    let bc = open_coords(b);
    let na = ac.len();
    let nb = bc.len();
    if na < 2 || nb < 2 {
        return false;
    }
    for i in 0..na {
        let p1 = ac[i];
        let p2 = ac[(i + 1) % na];
        for j in 0..nb {
            let p3 = bc[j];
            let p4 = bc[(j + 1) % nb];
            if segment_intersection(p1, p2, p3, p4).is_some() {
                return true;
            }
        }
    }
    false
}

fn vertex_in_polygon(poly: &Polygon, other: &Polygon) -> bool {
    for ring in polygon_rings(poly) {
        for p in open_coords(ring) {
            if contains(other, p) {
                return true;
            }
        }
    }
    false
}

fn polygon_rings(polygon: &Polygon) -> impl Iterator<Item = &Ring> {
    std::iter::once(polygon.exterior()).chain(polygon.interiors())
}

fn open_coords(ring: &Ring) -> &[Coord] {
    let coords = ring.coords();
    match coords {
        [first, .., last] if first == last => &coords[..coords.len() - 1],
        _ => coords,
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

    #[test]
    fn test_intersects_disjoint_overlapping_envelopes() {
        let a = Polygon::new(
            Ring::new(vec![
                Coord::new(0.0, 1.0),
                Coord::new(1.0, 0.0),
                Coord::new(2.0, 1.0),
                Coord::new(1.0, 2.0),
                Coord::new(0.0, 1.0),
            ]),
            vec![],
        );
        let b = Polygon::new(
            Ring::new(vec![
                Coord::new(1.5, 2.0),
                Coord::new(2.5, 1.0),
                Coord::new(3.5, 2.0),
                Coord::new(2.5, 3.0),
                Coord::new(1.5, 2.0),
            ]),
            vec![],
        );
        assert!(!intersects(&a, &b));
    }

    #[test]
    fn test_intersects_nested() {
        let inner = Polygon::new(
            Ring::new(vec![
                Coord::new(1.0, 1.0),
                Coord::new(2.0, 1.0),
                Coord::new(2.0, 2.0),
                Coord::new(1.0, 2.0),
                Coord::new(1.0, 1.0),
            ]),
            vec![],
        );
        assert!(intersects(&square(), &inner));
    }

    #[test]
    fn test_intersects_inside_hole() {
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
        let with_hole = Polygon::new(exterior, vec![hole]);
        let inner = Polygon::new(
            Ring::new(vec![
                Coord::new(4.0, 4.0),
                Coord::new(6.0, 4.0),
                Coord::new(6.0, 6.0),
                Coord::new(4.0, 6.0),
                Coord::new(4.0, 4.0),
            ]),
            vec![],
        );
        assert!(!intersects(&with_hole, &inner));
    }
}
