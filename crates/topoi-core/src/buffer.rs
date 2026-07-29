//! Polygon buffering, backed by the i_overlay outline engine.

use crate::geometry::MultiPolygon;
use crate::overlay::{PolygonSet, from_shapes, to_shapes};
use i_overlay::mesh::outline::offset::OutlineOffset;
use i_overlay::mesh::style::{LineJoin, OutlineStyle};
use std::f64::consts::PI;

/// Round joins take the maximum segment length over the arc radius, so this is
/// about 32 segments for a full circle.
const ROUND_JOIN_ANGLE: f64 = PI / 16.0;

/// Corners sharper than this get cut off instead of spiking far out.
const MITER_LIMIT_ANGLE: f64 = PI / 3.0;

/// How a buffer outline turns a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinStyle {
    /// Arc corners, which is what a GIS buffer normally means.
    Round,
    /// Sharp corners, cut off past a limit so spikes stay bounded.
    Miter,
    /// Corners cut off flat.
    Bevel,
}

/// Buffer a polygon set by a distance, with round joins.
///
/// Positive grows, negative shrinks. Concave rings, holes and multipolygon
/// operands all work. The result is a `MultiPolygon` because growing can merge
/// separate pieces and shrinking can split one piece into several or erase it
/// entirely.
///
/// ```
/// use topoi_core::{Coord, JoinStyle, Polygon, buffer_polygon, buffer_polygon_with_join};
///
/// let square = Polygon::from_coords(&[
///     Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
///     Coord::new(10.0, 10.0), Coord::new(0.0, 10.0),
/// ]);
///
/// // Shrinking a convex ring adds no arcs, so this is exactly 8x8
/// let inset = buffer_polygon(&square, -1.0);
/// assert!((inset.area() - 64.0).abs() < 1e-9);
///
/// // A negative distance past the inradius leaves nothing
/// assert!(buffer_polygon(&square, -6.0).polygons().is_empty());
///
/// // Sharp corners instead of arcs
/// let mitred = buffer_polygon_with_join(&square, 1.0, JoinStyle::Miter);
/// assert!((mitred.area() - 144.0).abs() < 1e-9);
/// ```
pub fn buffer_polygon<S>(subject: &S, distance: f64) -> MultiPolygon
where
    S: PolygonSet + ?Sized,
{
    buffer_polygon_with_join(subject, distance, JoinStyle::Round)
}

/// Buffer a polygon set by a distance with an explicit join style.
pub fn buffer_polygon_with_join<S>(subject: &S, distance: f64, join: JoinStyle) -> MultiPolygon
where
    S: PolygonSet + ?Sized,
{
    let polygons = subject.as_polygons();
    if distance == 0.0 {
        return MultiPolygon::new(polygons.to_vec());
    }

    // to_shapes normalizes exteriors to CCW and holes to CW, which the outline
    // engine needs: a reversed contour offsets the wrong way and vanishes.
    let shapes = to_shapes(polygons);
    let style = OutlineStyle::new(distance).line_join(match join {
        JoinStyle::Round => LineJoin::Round(ROUND_JOIN_ANGLE),
        JoinStyle::Miter => LineJoin::Miter(MITER_LIMIT_ANGLE),
        JoinStyle::Bevel => LineJoin::Bevel,
    });

    from_shapes(shapes.outline(&style))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Coord, Polygon, Ring};

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
        assert_eq!(result.polygons().len(), 1);
        assert_eq!(result.polygons()[0].exterior().coords(), ring.coords());
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

    #[test]
    fn test_join_styles_all_produce_a_polygon() {
        let poly = Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(4.0, 0.0),
            Coord::new(0.0, 4.0),
        ]);
        for join in [JoinStyle::Round, JoinStyle::Miter, JoinStyle::Bevel] {
            let result = buffer_polygon_with_join(&poly, 1.0, join);
            assert_eq!(result.polygons().len(), 1, "{join:?}");
            assert!(result.area() > poly.area(), "{join:?}");
        }
    }
}
