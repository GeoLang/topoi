//! Square and hexagon grids covering an envelope.

use crate::Error;
use crate::envelope::Envelope;
use crate::geometry::{Coord, Polygon, closed_polygon};

/// Past this a grid is a mistake rather than a request, and building it would
/// exhaust memory in a browser tab.
const MAX_CELLS: usize = 1_000_000;

/// Square cells covering `envelope`, row-major from its lower-left corner.
///
/// Edge cells extend past the envelope so the cover is complete.
pub fn square_grid(envelope: &Envelope, cell_size: f64) -> Result<Vec<Polygon>, Error> {
    let (cols, rows) = dims(envelope, cell_size, cell_size, 0)?;
    let mut cells = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let x0 = envelope.min_x + col as f64 * cell_size;
            let y0 = envelope.min_y + row as f64 * cell_size;
            let (x1, y1) = (x0 + cell_size, y0 + cell_size);
            cells.push(closed_polygon(vec![
                Coord::new(x0, y0),
                Coord::new(x1, y0),
                Coord::new(x1, y1),
                Coord::new(x0, y1),
            ]));
        }
    }
    Ok(cells)
}

/// Pointy-top hexagon cells covering `envelope`, row by row from the bottom.
///
/// `cell_size` is the circumradius, so a cell is `2 * cell_size` tall and
/// `sqrt(3) * cell_size` wide. Cells overlapping the envelope at all are kept,
/// so the cover is complete and edge cells extend past it.
pub fn hex_grid(envelope: &Envelope, cell_size: f64) -> Result<Vec<Polygon>, Error> {
    let width = 3.0_f64.sqrt() * cell_size;
    let step_y = 1.5 * cell_size;
    let (cols, rows) = dims(envelope, width, step_y, 3)?;

    let mut cells = Vec::new();
    for row in 0..rows {
        let cy = envelope.min_y - step_y + row as f64 * step_y;
        let offset = if row % 2 == 1 { width / 2.0 } else { 0.0 };
        for col in 0..cols {
            let cx = envelope.min_x - width + col as f64 * width + offset;
            let bounds = Envelope::new(
                cx - width / 2.0,
                cy - cell_size,
                cx + width / 2.0,
                cy + cell_size,
            );
            if !bounds.intersects(envelope) {
                continue;
            }
            cells.push(closed_polygon(vec![
                Coord::new(cx, cy + cell_size),
                Coord::new(cx - width / 2.0, cy + cell_size / 2.0),
                Coord::new(cx - width / 2.0, cy - cell_size / 2.0),
                Coord::new(cx, cy - cell_size),
                Coord::new(cx + width / 2.0, cy - cell_size / 2.0),
                Coord::new(cx + width / 2.0, cy + cell_size / 2.0),
            ]));
        }
    }
    Ok(cells)
}

/// Column and row counts for a step size, `margin` extra of each to let a
/// staggered grid start outside the envelope.
fn dims(
    envelope: &Envelope,
    step_x: f64,
    step_y: f64,
    margin: usize,
) -> Result<(usize, usize), Error> {
    let bad_step = |step: f64| step <= 0.0 || !step.is_finite();
    if bad_step(step_x) || bad_step(step_y) {
        return Err(Error::InvalidGeometry(
            "grid cell size must be a positive finite number".into(),
        ));
    }
    // capped so the limit check below cannot overflow
    let count = |extent: f64, step: f64| {
        let n = (extent / step).ceil();
        if n.is_finite() && n >= 1.0 {
            (n as usize).min(MAX_CELLS + 1)
        } else {
            1
        }
    };
    let cols = count(envelope.width(), step_x).saturating_add(margin);
    let rows = count(envelope.height(), step_y).saturating_add(margin);
    if cols.checked_mul(rows).is_none_or(|n| n > MAX_CELLS) {
        return Err(Error::InvalidGeometry(format!(
            "grid of {cols} by {rows} cells is over the {MAX_CELLS} cell limit"
        )));
    }
    Ok((cols, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicates::contains;

    fn env() -> Envelope {
        Envelope::new(0.0, 0.0, 10.0, 10.0)
    }

    #[test]
    fn test_square_grid_counts_and_covers() {
        let cells = square_grid(&env(), 2.5).unwrap();
        assert_eq!(cells.len(), 16);
        let total: f64 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_square_grid_edge_cells_overhang() {
        let cells = square_grid(&env(), 3.0).unwrap();
        assert_eq!(cells.len(), 16);
        let total: f64 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 144.0).abs() < 1e-9);
    }

    #[test]
    fn test_square_cells_are_closed_rings() {
        let cells = square_grid(&env(), 5.0).unwrap();
        assert!(cells.iter().all(|c| c.exterior().is_closed()));
    }

    #[test]
    fn test_hex_cell_shape() {
        let cells = hex_grid(&env(), 2.0).unwrap();
        let expected = 1.5 * 3.0_f64.sqrt() * 4.0;
        for cell in &cells {
            assert_eq!(cell.exterior().coords().len(), 7);
            assert!((cell.area() - expected).abs() < 1e-9, "got {}", cell.area());
        }
    }

    #[test]
    fn test_hex_grid_covers_the_envelope() {
        let cells = hex_grid(&env(), 1.7).unwrap();
        for i in 0..7 {
            for j in 0..7 {
                let probe = Coord::new(0.31 + i as f64 * 1.43, 0.17 + j as f64 * 1.41);
                assert!(
                    cells.iter().any(|c| contains(c, &probe)),
                    "no cell covers {probe:?}"
                );
            }
        }
    }

    #[test]
    fn test_hex_cells_are_disjoint() {
        use crate::overlay::intersection;
        let cells = hex_grid(&env(), 3.0).unwrap();
        for i in 0..cells.len() {
            for j in i + 1..cells.len() {
                let overlap = intersection(&cells[i], &cells[j]).area();
                assert!(overlap < 1e-9, "cells {i} and {j} overlap by {overlap}");
            }
        }
    }

    #[test]
    fn test_rejects_bad_cell_size() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(square_grid(&env(), bad).is_err(), "accepted {bad}");
            assert!(hex_grid(&env(), bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn test_rejects_absurd_cell_count() {
        let err = square_grid(&env(), 1e-6).unwrap_err();
        assert!(err.to_string().contains("cell limit"), "{err}");
        assert!(hex_grid(&Envelope::new(0.0, 0.0, 1e9, 1e9), 1.0).is_err());
    }

    #[test]
    fn test_degenerate_envelope_gives_one_cell() {
        let cells = square_grid(&Envelope::new(5.0, 5.0, 5.0, 5.0), 1.0).unwrap();
        assert_eq!(cells.len(), 1);
    }
}
