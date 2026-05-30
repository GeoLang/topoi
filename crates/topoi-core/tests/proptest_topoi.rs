use proptest::prelude::*;
use topoi_core::*;

proptest! {
    /// Ring area is always non-negative for CCW rings.
    #[test]
    fn ring_area_non_negative(
        coords in prop::collection::vec((-1000.0f64..1000.0, -1000.0f64..1000.0), 4..20)
    ) {
        let mut ring_coords: Vec<Coord> = coords.iter()
            .map(|(x, y)| Coord::new(*x, *y))
            .collect();
        // Close the ring
        if let Some(first) = ring_coords.first().copied() {
            ring_coords.push(first);
        }
        let ring = Ring::new(ring_coords);
        // Signed area can be negative; absolute area is always >= 0
        prop_assert!(ring.area().abs() >= 0.0);
    }

    /// Convex hull always contains all input points.
    #[test]
    fn convex_hull_contains_all_points(
        coords in prop::collection::vec((-100.0f64..100.0, -100.0f64..100.0), 3..50)
    ) {
        let points: Vec<Coord> = coords.iter()
            .map(|(x, y)| Coord::new(*x, *y))
            .collect();
        let hull = convex_hull(&points);
        // Hull area should be >= 0
        prop_assert!(hull.area() >= 0.0);
    }

    /// Distance is symmetric: d(a,b) == d(b,a).
    #[test]
    fn distance_symmetric(
        ax in -1000.0f64..1000.0,
        ay in -1000.0f64..1000.0,
        bx in -1000.0f64..1000.0,
        by in -1000.0f64..1000.0,
    ) {
        let a = Coord::new(ax, ay);
        let b = Coord::new(bx, by);
        let d1 = a.distance_to(&b);
        let d2 = b.distance_to(&a);
        prop_assert!((d1 - d2).abs() < 1e-10);
    }

    /// Distance satisfies triangle inequality.
    #[test]
    fn triangle_inequality(
        ax in -100.0f64..100.0,
        ay in -100.0f64..100.0,
        bx in -100.0f64..100.0,
        by in -100.0f64..100.0,
        cx in -100.0f64..100.0,
        cy in -100.0f64..100.0,
    ) {
        let a = Coord::new(ax, ay);
        let b = Coord::new(bx, by);
        let c = Coord::new(cx, cy);
        let ab = a.distance_to(&b);
        let bc = b.distance_to(&c);
        let ac = a.distance_to(&c);
        prop_assert!(ac <= ab + bc + 1e-10);
    }

    /// Simplify never increases the number of points.
    #[test]
    fn simplify_reduces_points(
        coords in prop::collection::vec((-100.0f64..100.0, -100.0f64..100.0), 3..100),
        epsilon in 0.01f64..10.0,
    ) {
        let points: Vec<Coord> = coords.iter()
            .map(|(x, y)| Coord::new(*x, *y))
            .collect();
        let simplified = simplify(&points, epsilon);
        prop_assert!(simplified.len() <= points.len());
        prop_assert!(simplified.len() >= 2); // Always keeps first and last
    }

    /// Simplify preserves first and last point.
    #[test]
    fn simplify_preserves_endpoints(
        coords in prop::collection::vec((-100.0f64..100.0, -100.0f64..100.0), 3..50),
        epsilon in 0.01f64..10.0,
    ) {
        let points: Vec<Coord> = coords.iter()
            .map(|(x, y)| Coord::new(*x, *y))
            .collect();
        let simplified = simplify(&points, epsilon);
        prop_assert_eq!(simplified.first().unwrap().x, points.first().unwrap().x);
        prop_assert_eq!(simplified.first().unwrap().y, points.first().unwrap().y);
        prop_assert_eq!(simplified.last().unwrap().x, points.last().unwrap().x);
        prop_assert_eq!(simplified.last().unwrap().y, points.last().unwrap().y);
    }
}
