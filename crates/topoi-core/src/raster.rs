//! Rasterization of geometries onto a caller-specified grid window.
//!
//! Coverage rules are chosen so that rasterizing a geometry clipped to a
//! pixel-aligned sub-window burns exactly the sub-window's slice of the whole
//! rasterization, which is what tiled callers need for seam-free output:
//!
//! - Polygons burn a cell iff the cell center is inside (even-odd, holes
//!   respected). Centers sit half a cell off any pixel-aligned clip edge.
//! - Lines burn every cell the segment geometrically intersects (grid
//!   traversal, not Bresenham, so coverage does not depend on endpoints).
//! - Points burn one cell, x half-open `[min, max)`, y top-edge inclusive
//!   `(min, max]`, matching half-open tile membership on both axes.
//!
//! One caveat: a segment passing exactly through a cell corner is a float
//! tie, and clipping can move it to either side, so such a segment may burn
//! one corner-adjacent cell differently between the whole and clipped runs.

use crate::clipping::clip_segment_rect;
use crate::geojson::FeatureGeometry;
use crate::geometry::{Coord, LineString, Polygon};

/// Output grid geometry for [`rasterize`].
///
/// Row 0 starts at `origin_y` and rows grow toward +y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridWindow {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: usize,
    pub height: usize,
    pub cell_size: f64,
}

/// Burn geometry-value pairs onto a grid, in slice order (last wins).
///
/// Returns a `width * height` row-major grid, NaN where nothing burned.
pub fn rasterize(shapes: &[(FeatureGeometry, f64)], window: &GridWindow) -> Vec<f64> {
    let mut grid = vec![f64::NAN; window.width * window.height];
    for (geometry, value) in shapes {
        burn_geometry(&mut grid, window, geometry, *value);
    }
    grid
}

fn burn_geometry(grid: &mut [f64], w: &GridWindow, geometry: &FeatureGeometry, value: f64) {
    match geometry {
        FeatureGeometry::Point(p) => burn_point(grid, w, &p.0, value),
        FeatureGeometry::MultiPoint(mp) => {
            for p in mp.points() {
                burn_point(grid, w, &p.0, value);
            }
        }
        FeatureGeometry::LineString(l) => burn_line(grid, w, l, value),
        FeatureGeometry::MultiLineString(ml) => {
            for l in ml.linestrings() {
                burn_line(grid, w, l, value);
            }
        }
        FeatureGeometry::Polygon(p) => fill_polygon(grid, w, p, value),
        FeatureGeometry::MultiPolygon(mp) => {
            for p in mp.polygons() {
                fill_polygon(grid, w, p, value);
            }
        }
        FeatureGeometry::GeometryCollection(members) => {
            for member in members {
                burn_geometry(grid, w, member, value);
            }
        }
    }
}

fn burn_line(grid: &mut [f64], w: &GridWindow, line: &LineString, value: f64) {
    for pair in line.coords().windows(2) {
        burn_segment(grid, w, pair[0], pair[1], value);
    }
}

fn burn_point(grid: &mut [f64], w: &GridWindow, p: &Coord, value: f64) {
    let col = ((p.x - w.origin_x) / w.cell_size).floor() as i64;
    let row = ((p.y - w.origin_y) / w.cell_size).ceil() as i64 - 1;
    if col >= 0 && (col as usize) < w.width && row >= 0 && (row as usize) < w.height {
        grid[row as usize * w.width + col as usize] = value;
    }
}

fn fill_polygon(grid: &mut [f64], w: &GridWindow, poly: &Polygon, value: f64) {
    let mut crossings = Vec::new();
    for row in 0..w.height {
        let y = w.origin_y + (row as f64 + 0.5) * w.cell_size;
        crossings.clear();
        ring_crossings(poly.exterior().coords(), y, &mut crossings);
        for hole in poly.interiors() {
            ring_crossings(hole.coords(), y, &mut crossings);
        }
        crossings.sort_by(|a, b| a.total_cmp(b));
        for pair in crossings.as_chunks::<2>().0 {
            let start = ((pair[0] - w.origin_x) / w.cell_size - 0.5).ceil() as i64;
            let end = ((pair[1] - w.origin_x) / w.cell_size - 0.5).ceil() as i64;
            for col in start.max(0)..end.min(w.width as i64) {
                grid[row * w.width + col as usize] = value;
            }
        }
    }
}

/// Crossing x of each ring edge with the horizontal line at `y`, half-open in
/// y so a scanline through a vertex counts once.
fn ring_crossings(coords: &[Coord], y: f64, out: &mut Vec<f64>) {
    let n = coords.len();
    for i in 0..n {
        let a = coords[i];
        let b = coords[(i + 1) % n];
        if (a.y <= y && y < b.y) || (b.y <= y && y < a.y) {
            out.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
        }
    }
}

fn burn_segment(grid: &mut [f64], w: &GridWindow, a: Coord, b: Coord, value: f64) {
    let cs = w.cell_size;
    let max_x = w.origin_x + w.width as f64 * cs;
    let max_y = w.origin_y + w.height as f64 * cs;
    let Some((a, b)) = clip_segment_rect(a, b, w.origin_x, w.origin_y, max_x, max_y) else {
        return;
    };

    let cell = |v: f64, origin: f64, count: usize| -> i64 {
        (((v - origin) / cs).floor() as i64).clamp(0, count as i64 - 1)
    };
    let mut cx = cell(a.x, w.origin_x, w.width);
    let mut cy = cell(a.y, w.origin_y, w.height);
    let end_cx = cell(b.x, w.origin_x, w.width);
    let end_cy = cell(b.y, w.origin_y, w.height);

    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let (step_x, mut t_max_x, t_delta_x) = axis_traversal(a.x, dx, w.origin_x, cx, cs);
    let (step_y, mut t_max_y, t_delta_y) = axis_traversal(a.y, dy, w.origin_y, cy, cs);

    let steps = (end_cx - cx).abs() + (end_cy - cy).abs();
    grid[cy as usize * w.width + cx as usize] = value;
    for _ in 0..steps {
        if t_max_x < t_max_y {
            cx += step_x;
            t_max_x += t_delta_x;
        } else {
            cy += step_y;
            t_max_y += t_delta_y;
        }
        if cx < 0 || (cx as usize) >= w.width || cy < 0 || (cy as usize) >= w.height {
            return;
        }
        grid[cy as usize * w.width + cx as usize] = value;
    }
}

/// Step direction, parameter of the first cell-boundary crossing, and
/// parameter increment per cell for one traversal axis.
fn axis_traversal(start: f64, delta: f64, origin: f64, cell: i64, cs: f64) -> (i64, f64, f64) {
    if delta > 0.0 {
        let boundary = origin + (cell + 1) as f64 * cs;
        (1, (boundary - start) / delta, cs / delta)
    } else if delta < 0.0 {
        let boundary = origin + cell as f64 * cs;
        (-1, (boundary - start) / delta, cs / -delta)
    } else {
        (0, f64::INFINITY, f64::INFINITY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{MultiLineString, MultiPoint, MultiPolygon, Point, Ring};

    fn window(
        origin_x: f64,
        origin_y: f64,
        width: usize,
        height: usize,
        cell_size: f64,
    ) -> GridWindow {
        GridWindow {
            origin_x,
            origin_y,
            width,
            height,
            cell_size,
        }
    }

    fn square(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Ring {
        Ring::new(vec![
            Coord::new(min_x, min_y),
            Coord::new(max_x, min_y),
            Coord::new(max_x, max_y),
            Coord::new(min_x, max_y),
            Coord::new(min_x, min_y),
        ])
    }

    fn burned(grid: &[f64], w: &GridWindow) -> Vec<(usize, usize)> {
        let mut cells: Vec<_> = grid
            .iter()
            .enumerate()
            .filter(|(_, v)| !v.is_nan())
            .map(|(i, _)| (i % w.width, i / w.width))
            .collect();
        cells.sort_unstable();
        cells
    }

    #[test]
    fn test_fill_square_center_rule() {
        let w = window(0.0, 0.0, 4, 4, 1.0);
        let poly = Polygon::new(square(1.0, 1.0, 3.0, 3.0), vec![]);
        let grid = rasterize(&[(FeatureGeometry::Polygon(poly), 7.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(1, 1), (1, 2), (2, 1), (2, 2)]);
        assert_eq!(grid[5], 7.0);
        assert!(grid[0].is_nan());
    }

    #[test]
    fn test_fill_polygon_with_hole() {
        let w = window(0.0, 0.0, 8, 8, 1.0);
        let poly = Polygon::new(square(0.0, 0.0, 8.0, 8.0), vec![square(2.0, 2.0, 6.0, 6.0)]);
        let grid = rasterize(&[(FeatureGeometry::Polygon(poly), 1.0)], &w);
        assert!(!grid[0].is_nan());
        assert!(grid[27].is_nan(), "hole interior must stay empty");
        assert_eq!(grid[9], 1.0);
        assert_eq!(burned(&grid, &w).len(), 64 - 16);
    }

    #[test]
    fn test_multipolygon_and_last_wins() {
        let w = window(0.0, 0.0, 4, 1, 1.0);
        let a = Polygon::new(square(0.0, 0.0, 3.0, 1.0), vec![]);
        let b = Polygon::new(square(2.0, 0.0, 4.0, 1.0), vec![]);
        let grid = rasterize(
            &[
                (FeatureGeometry::Polygon(a), 1.0),
                (
                    FeatureGeometry::MultiPolygon(MultiPolygon::new(vec![b])),
                    2.0,
                ),
            ],
            &w,
        );
        assert_eq!(grid, vec![1.0, 1.0, 2.0, 2.0]);
    }

    #[test]
    fn test_line_traversal_cells() {
        let w = window(0.0, 0.0, 3, 3, 1.0);
        let line = LineString::new(vec![Coord::new(0.5, 0.2), Coord::new(2.5, 2.2)]);
        let grid = rasterize(&[(FeatureGeometry::LineString(line), 5.0)], &w);
        assert_eq!(
            burned(&grid, &w),
            vec![(0, 0), (1, 0), (1, 1), (2, 1), (2, 2)]
        );
    }

    #[test]
    fn test_line_axis_aligned() {
        let w = window(0.0, 0.0, 4, 4, 1.0);
        let h = LineString::new(vec![Coord::new(0.5, 1.5), Coord::new(3.5, 1.5)]);
        let v = LineString::new(vec![Coord::new(2.5, 0.5), Coord::new(2.5, 3.5)]);
        let grid = rasterize(
            &[
                (FeatureGeometry::LineString(h), 1.0),
                (FeatureGeometry::LineString(v), 2.0),
            ],
            &w,
        );
        for col in 0..4 {
            assert!(!grid[4 + col].is_nan(), "row 1 col {col}");
        }
        for row in 0..4 {
            assert_eq!(grid[row * 4 + 2], 2.0, "col 2 row {row}");
        }
    }

    #[test]
    fn test_line_outside_window() {
        let w = window(0.0, 0.0, 2, 2, 1.0);
        let line = LineString::new(vec![Coord::new(5.0, 5.0), Coord::new(6.0, 6.0)]);
        let grid = rasterize(&[(FeatureGeometry::LineString(line), 1.0)], &w);
        assert!(grid.iter().all(|v| v.is_nan()));
    }

    #[test]
    fn test_point_membership() {
        let w = window(0.0, 0.0, 2, 2, 1.0);
        // y on a cell boundary joins the cell below (top edge inclusive)
        let grid = rasterize(&[(FeatureGeometry::Point(Point::new(0.5, 1.0)), 1.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(0, 0)]);
        // x on a cell boundary joins the cell to the right (half-open)
        let grid = rasterize(&[(FeatureGeometry::Point(Point::new(1.0, 0.5)), 1.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(1, 0)]);
        // bottom edge is exclusive, top edge of the window is inclusive
        let grid = rasterize(&[(FeatureGeometry::Point(Point::new(0.5, 0.0)), 1.0)], &w);
        assert!(grid.iter().all(|v| v.is_nan()));
        let grid = rasterize(&[(FeatureGeometry::Point(Point::new(0.5, 2.0)), 1.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(0, 1)]);
    }

    #[test]
    fn test_multipoint_burns_every_point() {
        let w = window(0.0, 0.0, 3, 3, 1.0);
        let mp = MultiPoint::new(vec![
            Point::new(0.5, 0.5),
            Point::new(2.5, 2.5),
            // outside the window, dropped like a lone point would be
            Point::new(9.0, 9.0),
        ]);
        let grid = rasterize(&[(FeatureGeometry::MultiPoint(mp), 4.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(0, 0), (2, 2)]);
        assert_eq!(grid[0], 4.0);
    }

    #[test]
    fn test_multilinestring_burns_every_line() {
        let w = window(0.0, 0.0, 4, 4, 1.0);
        let mls = MultiLineString::new(vec![
            LineString::new(vec![Coord::new(0.5, 0.5), Coord::new(3.5, 0.5)]),
            LineString::new(vec![Coord::new(0.5, 3.5), Coord::new(3.5, 3.5)]),
        ]);
        let grid = rasterize(&[(FeatureGeometry::MultiLineString(mls), 6.0)], &w);
        assert_eq!(
            burned(&grid, &w),
            vec![
                (0, 0),
                (0, 3),
                (1, 0),
                (1, 3),
                (2, 0),
                (2, 3),
                (3, 0),
                (3, 3)
            ]
        );
    }

    #[test]
    fn test_geometry_collection_recurses() {
        let w = window(0.0, 0.0, 4, 2, 1.0);
        let collection = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::Point(Point::new(0.5, 0.5)),
            FeatureGeometry::GeometryCollection(vec![FeatureGeometry::Polygon(Polygon::new(
                square(2.0, 0.0, 4.0, 1.0),
                vec![],
            ))]),
        ]);
        let grid = rasterize(&[(collection, 8.0)], &w);
        assert_eq!(burned(&grid, &w), vec![(0, 0), (2, 0), (3, 0)]);
    }

    #[test]
    fn test_tiled_equals_whole() {
        let shapes = vec![
            (
                FeatureGeometry::Polygon(Polygon::new(
                    Ring::new(vec![
                        Coord::new(1.3, 0.4),
                        Coord::new(6.7, 1.2),
                        Coord::new(5.1, 3.8),
                        Coord::new(2.2, 3.1),
                        Coord::new(1.3, 0.4),
                    ]),
                    vec![],
                )),
                3.0,
            ),
            (
                FeatureGeometry::LineString(LineString::new(vec![
                    Coord::new(0.2, 3.7),
                    Coord::new(7.8, 0.4),
                ])),
                9.0,
            ),
        ];
        let whole = rasterize(&shapes, &window(0.0, 0.0, 8, 4, 1.0));
        let left = rasterize(&shapes, &window(0.0, 0.0, 4, 4, 1.0));
        let right = rasterize(&shapes, &window(4.0, 0.0, 4, 4, 1.0));
        for row in 0..4 {
            for col in 0..4 {
                let a = whole[row * 8 + col];
                let b = left[row * 4 + col];
                assert!(
                    a == b || (a.is_nan() && b.is_nan()),
                    "left {col},{row}: {a} vs {b}"
                );
                let a = whole[row * 8 + col + 4];
                let b = right[row * 4 + col];
                assert!(
                    a == b || (a.is_nan() && b.is_nan()),
                    "right {col},{row}: {a} vs {b}"
                );
            }
        }
    }
}
