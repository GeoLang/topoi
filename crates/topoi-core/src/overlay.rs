//! General polygon overlay: union, intersection, difference and xor.
//!
//! Backed by the `i_overlay` engine, so concave rings, holes, self-intersecting
//! input and multipolygon operands are all handled. Results are always returned
//! as a `MultiPolygon` because any of these operations can split a single input
//! polygon into several pieces or punch holes into it.

use crate::geometry::{Coord, MultiPolygon, Polygon, Ring, signed_ring_area};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::i_float::float::compatible::FloatPointCompatible;
use i_overlay::i_shape::base::data::Shapes;

/// A boolean set operation between two polygon sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanOp {
    /// Everything covered by either operand.
    Union,
    /// Everything covered by both operands.
    Intersection,
    /// The subject minus the clip.
    Difference,
    /// Covered by exactly one operand (symmetric difference).
    Xor,
}

/// Operand of an overlay operation: a single `Polygon`, a `MultiPolygon`, or a
/// slice of polygons.
pub trait PolygonSet {
    fn as_polygons(&self) -> &[Polygon];
}

impl PolygonSet for Polygon {
    fn as_polygons(&self) -> &[Polygon] {
        std::slice::from_ref(self)
    }
}

impl PolygonSet for MultiPolygon {
    fn as_polygons(&self) -> &[Polygon] {
        self.polygons()
    }
}

impl PolygonSet for [Polygon] {
    fn as_polygons(&self) -> &[Polygon] {
        self
    }
}

impl PolygonSet for Vec<Polygon> {
    fn as_polygons(&self) -> &[Polygon] {
        self
    }
}

// Lets i_overlay consume and produce topoi coordinates directly, so no
// intermediate point type is needed.
impl FloatPointCompatible for Coord {
    type Scalar = f64;

    fn from_xy(x: f64, y: f64) -> Self {
        Coord::new(x, y)
    }

    fn x(&self) -> f64 {
        self.x
    }

    fn y(&self) -> f64 {
        self.y
    }
}

/// Run a boolean operation between two polygon sets.
///
/// ```
/// use topoi_core::{Coord, Polygon, difference, union};
///
/// let outer = Polygon::from_coords(&[
///     Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
///     Coord::new(10.0, 10.0), Coord::new(0.0, 10.0),
/// ]);
/// let inner = Polygon::from_coords(&[
///     Coord::new(4.0, 4.0), Coord::new(6.0, 4.0),
///     Coord::new(6.0, 6.0), Coord::new(4.0, 6.0),
/// ]);
///
/// // One polygon with a hole, area 96
/// let holed = difference(&outer, &inner);
/// assert_eq!(holed.polygons()[0].interiors().len(), 1);
/// assert!((holed.area() - 96.0).abs() < 1e-9);
///
/// // Filling the hole back in
/// let whole = union(&holed, &inner);
/// assert!((whole.area() - 100.0).abs() < 1e-9);
/// ```
pub fn boolean_op<A, B>(subject: &A, clip: &B, op: BooleanOp) -> MultiPolygon
where
    A: PolygonSet + ?Sized,
    B: PolygonSet + ?Sized,
{
    let subj = to_shapes(subject.as_polygons());
    let clip = to_shapes(clip.as_polygons());
    // NonZero is correct because to_shapes normalizes exteriors to CCW and
    // holes to CW, so winding alone decides what is solid.
    let shapes = subj.overlay(&clip, overlay_rule(op), FillRule::NonZero);
    from_shapes(shapes)
}

/// Area covered by either operand.
pub fn union<A, B>(subject: &A, clip: &B) -> MultiPolygon
where
    A: PolygonSet + ?Sized,
    B: PolygonSet + ?Sized,
{
    boolean_op(subject, clip, BooleanOp::Union)
}

/// Area covered by both operands.
pub fn intersection<A, B>(subject: &A, clip: &B) -> MultiPolygon
where
    A: PolygonSet + ?Sized,
    B: PolygonSet + ?Sized,
{
    boolean_op(subject, clip, BooleanOp::Intersection)
}

/// Area of the subject not covered by the clip.
pub fn difference<A, B>(subject: &A, clip: &B) -> MultiPolygon
where
    A: PolygonSet + ?Sized,
    B: PolygonSet + ?Sized,
{
    boolean_op(subject, clip, BooleanOp::Difference)
}

/// Area covered by exactly one operand (symmetric difference).
pub fn xor<A, B>(subject: &A, clip: &B) -> MultiPolygon
where
    A: PolygonSet + ?Sized,
    B: PolygonSet + ?Sized,
{
    boolean_op(subject, clip, BooleanOp::Xor)
}

fn overlay_rule(op: BooleanOp) -> OverlayRule {
    match op {
        BooleanOp::Union => OverlayRule::Union,
        BooleanOp::Intersection => OverlayRule::Intersect,
        BooleanOp::Difference => OverlayRule::Difference,
        BooleanOp::Xor => OverlayRule::Xor,
    }
}

/// Convert topoi polygons into i_overlay shapes, forcing exteriors CCW and
/// holes CW so the interior rings really do read as holes whatever winding the
/// caller used.
pub(crate) fn to_shapes(polygons: &[Polygon]) -> Vec<Vec<Vec<Coord>>> {
    polygons
        .iter()
        .map(|polygon| {
            let mut shape = Vec::with_capacity(1 + polygon.interiors().len());
            shape.push(to_contour(polygon.exterior(), true));
            for hole in polygon.interiors() {
                shape.push(to_contour(hole, false));
            }
            shape
        })
        .collect()
}

/// A ring as an open contour with the requested winding. i_overlay closes
/// contours itself, so the repeated closing coordinate is dropped.
fn to_contour(ring: &Ring, ccw: bool) -> Vec<Coord> {
    let coords = ring.coords();
    let open = if ring.is_closed() {
        &coords[..coords.len() - 1]
    } else {
        coords
    };
    let mut contour = open.to_vec();
    if (signed_ring_area(&contour) > 0.0) != ccw {
        contour.reverse();
    }
    contour
}

pub(crate) fn from_shapes(shapes: Shapes<Coord>) -> MultiPolygon {
    let polygons = shapes
        .into_iter()
        .map(|shape| {
            let mut rings = shape.into_iter().map(to_ring);
            let exterior = rings.next().unwrap_or_else(|| Ring::new(Vec::new()));
            Polygon::new(exterior, rings.collect())
        })
        .collect();
    MultiPolygon::new(polygons)
}

/// i_overlay returns open contours, topoi rings repeat the first coordinate.
fn to_ring(mut contour: Vec<Coord>) -> Ring {
    if let Some(first) = contour.first().copied()
        && contour.last() != Some(&first)
    {
        contour.push(first);
    }
    Ring::new(contour)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: f64, max: f64) -> Polygon {
        Polygon::new(
            Ring::new(vec![
                Coord::new(min, min),
                Coord::new(max, min),
                Coord::new(max, max),
                Coord::new(min, max),
                Coord::new(min, min),
            ]),
            vec![],
        )
    }

    #[test]
    fn test_to_contour_drops_closing_coord() {
        let contour = to_contour(square(0.0, 1.0).exterior(), true);
        assert_eq!(contour.len(), 4);
    }

    #[test]
    fn test_to_contour_normalizes_winding() {
        let ccw = to_contour(square(0.0, 1.0).exterior(), true);
        assert!(signed_ring_area(&ccw) > 0.0);
        let cw = to_contour(square(0.0, 1.0).exterior(), false);
        assert!(signed_ring_area(&cw) < 0.0);
        // Same ring, opposite order.
        assert_eq!(cw.len(), ccw.len());
    }

    #[test]
    fn test_to_ring_closes_contour() {
        let ring = to_ring(vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 1.0),
        ]);
        assert!(ring.is_closed());
        assert_eq!(ring.coords().len(), 4);
    }

    #[test]
    fn test_to_ring_leaves_empty_alone() {
        let ring = to_ring(Vec::new());
        assert!(ring.coords().is_empty());
    }

    #[test]
    fn test_union_of_overlapping_squares() {
        let result = union(&square(0.0, 2.0), &square(1.0, 3.0));
        assert_eq!(result.polygons().len(), 1);
        assert!((result.area() - 7.0).abs() < 1e-9, "got {}", result.area());
    }

    #[test]
    fn test_intersection_of_overlapping_squares() {
        let result = intersection(&square(0.0, 2.0), &square(1.0, 3.0));
        assert_eq!(result.polygons().len(), 1);
        assert!((result.area() - 1.0).abs() < 1e-9, "got {}", result.area());
    }
}
