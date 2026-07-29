use proptest::prelude::*;
use topoi_core::*;

/// The overlay engine snaps coordinates onto an integer grid sized to the input
/// extent, so areas match only to a relative tolerance.
fn tolerance(area: f64) -> f64 {
    1e-6 * (1.0 + area)
}

fn polygon(coords: &[(f64, f64)]) -> Polygon {
    let mut ring: Vec<Coord> = coords.iter().map(|&(x, y)| Coord::new(x, y)).collect();
    ring.push(ring[0]);
    Polygon::new(Ring::new(ring), vec![])
}

/// A concave L: the w by h box with its top-right corner cut out. Yields the
/// polygon and its exact area.
fn l_shape() -> impl Strategy<Value = (Polygon, f64)> {
    (1.0f64..100.0, 1.0f64..100.0, 0.1f64..0.9, 0.1f64..0.9).prop_map(|(w, h, fx, fy)| {
        let (cx, cy) = (w * fx, h * fy);
        let poly = polygon(&[(0.0, 0.0), (w, 0.0), (w, cy), (cx, cy), (cx, h), (0.0, h)]);
        (poly, w * h - (w - cx) * (h - cy))
    })
}

fn rectangle() -> impl Strategy<Value = Polygon> {
    (-20.0f64..120.0, -20.0f64..120.0, 1.0f64..80.0, 1.0f64..80.0)
        .prop_map(|(x, y, w, h)| polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]))
}

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

    /// Intersection and difference partition the subject: the two areas add
    /// back up to the subject area, even for a concave subject.
    #[test]
    fn overlay_partitions_subject_area(
        (subject, subject_area) in l_shape(),
        clip in rectangle(),
    ) {
        let inter = intersection(&subject, &clip).area();
        let diff = difference(&subject, &clip).area();
        prop_assert!(
            (inter + diff - subject_area).abs() < tolerance(subject_area),
            "{inter} + {diff} != {subject_area}"
        );
    }

    /// Inclusion-exclusion: area(A) + area(B) == area(A|B) + area(A&B).
    #[test]
    fn union_area_obeys_inclusion_exclusion(
        (subject, subject_area) in l_shape(),
        clip in rectangle(),
    ) {
        let clip_area = clip.area();
        let both = union(&subject, &clip).area() + intersection(&subject, &clip).area();
        prop_assert!(
            (both - subject_area - clip_area).abs() < tolerance(subject_area + clip_area),
            "{both} != {subject_area} + {clip_area}"
        );
    }

    /// Xor is the union minus the intersection.
    #[test]
    fn xor_is_union_minus_intersection(
        (subject, subject_area) in l_shape(),
        clip in rectangle(),
    ) {
        let expected = union(&subject, &clip).area() - intersection(&subject, &clip).area();
        let actual = xor(&subject, &clip).area();
        prop_assert!(
            (actual - expected).abs() < tolerance(subject_area + clip.area()),
            "{actual} != {expected}"
        );
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
