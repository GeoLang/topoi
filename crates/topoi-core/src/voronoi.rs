//! Voronoi cells, built by clipping the envelope with one half-plane per site.
//!
//! Half-plane clipping is quadratic in the number of sites but has no
//! degenerate cases: collinear sites, duplicates and sites outside the
//! envelope all fall out of the same loop.

use crate::envelope::Envelope;
use crate::geometry::{Coord, Polygon, closed_polygon};

/// Voronoi cells for `points`, clipped to `envelope` and index-aligned with the
/// input so callers can carry per-point attributes across.
///
/// A cell that survives nothing (a duplicate of an earlier site, or one clipped
/// away entirely) comes back as a polygon with an empty ring.
pub fn voronoi_polygons(points: &[Coord], envelope: &Envelope) -> Vec<Polygon> {
    points
        .iter()
        .enumerate()
        .map(|(i, site)| closed_polygon(cell(points, i, site, envelope)))
        .collect()
}

fn cell(points: &[Coord], i: usize, site: &Coord, envelope: &Envelope) -> Vec<Coord> {
    let mut ring = vec![
        Coord::new(envelope.min_x, envelope.min_y),
        Coord::new(envelope.max_x, envelope.min_y),
        Coord::new(envelope.max_x, envelope.max_y),
        Coord::new(envelope.min_x, envelope.max_y),
    ];
    for (j, other) in points.iter().enumerate() {
        if i == j {
            continue;
        }
        // duplicate sites have no bisector, so the first index keeps the cell
        if site == other {
            if j < i {
                return Vec::new();
            }
            continue;
        }
        ring = clip_bisector(&ring, site, other);
        if ring.len() < 3 {
            return Vec::new();
        }
    }
    ring
}

/// Keep the part of a convex ring that is closer to `site` than to `other`.
fn clip_bisector(ring: &[Coord], site: &Coord, other: &Coord) -> Vec<Coord> {
    let dx = other.x - site.x;
    let dy = other.y - site.y;
    let mid = dx * (site.x + other.x) / 2.0 + dy * (site.y + other.y) / 2.0;
    let side = |p: &Coord| dx * p.x + dy * p.y - mid;

    let n = ring.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let (sa, sb) = (side(&a), side(&b));
        if sa <= 0.0 {
            out.push(a);
        }
        if (sa < 0.0) != (sb < 0.0) && sa != 0.0 && sb != 0.0 {
            let t = sa / (sa - sb);
            out.push(Coord::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::intersection;
    use crate::predicates::contains;

    fn env() -> Envelope {
        Envelope::new(0.0, 0.0, 10.0, 10.0)
    }

    fn sites() -> Vec<Coord> {
        vec![
            Coord::new(2.0, 2.0),
            Coord::new(8.0, 3.0),
            Coord::new(5.0, 7.0),
            Coord::new(1.0, 9.0),
            Coord::new(9.0, 9.0),
        ]
    }

    #[test]
    fn test_cell_contains_its_own_site() {
        let points = sites();
        let cells = voronoi_polygons(&points, &env());
        assert_eq!(cells.len(), points.len());
        for (cell, site) in cells.iter().zip(&points) {
            assert!(contains(cell, site), "cell missing its site {site:?}");
        }
    }

    #[test]
    fn test_cells_are_disjoint() {
        let cells = voronoi_polygons(&sites(), &env());
        for i in 0..cells.len() {
            for j in i + 1..cells.len() {
                let overlap = intersection(&cells[i], &cells[j]).area();
                assert!(overlap < 1e-9, "cells {i} and {j} overlap by {overlap}");
            }
        }
    }

    #[test]
    fn test_cells_tile_the_envelope() {
        let envelope = env();
        let total: f64 = voronoi_polygons(&sites(), &envelope)
            .iter()
            .map(|c| c.area())
            .sum();
        assert!(
            (total - envelope.area()).abs() < 1e-9,
            "cells cover {total}, envelope is {}",
            envelope.area()
        );
    }

    #[test]
    fn test_single_site_takes_the_whole_envelope() {
        let cells = voronoi_polygons(&[Coord::new(3.0, 4.0)], &env());
        assert!((cells[0].area() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_no_sites() {
        assert!(voronoi_polygons(&[], &env()).is_empty());
    }

    #[test]
    fn test_two_sites_split_the_envelope() {
        let cells = voronoi_polygons(&[Coord::new(2.0, 5.0), Coord::new(8.0, 5.0)], &env());
        assert!((cells[0].area() - 50.0).abs() < 1e-9);
        assert!((cells[1].area() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn test_collinear_sites() {
        let points: Vec<Coord> = (1..=5).map(|i| Coord::new(i as f64, 5.0)).collect();
        let cells = voronoi_polygons(&points, &env());
        let total: f64 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 100.0).abs() < 1e-9);
        for (cell, site) in cells.iter().zip(&points) {
            assert!(contains(cell, site));
        }
    }

    #[test]
    fn test_duplicate_sites_keep_one_cell() {
        let points = vec![
            Coord::new(3.0, 3.0),
            Coord::new(3.0, 3.0),
            Coord::new(7.0, 7.0),
        ];
        let cells = voronoi_polygons(&points, &env());
        assert!(cells[0].area() > 0.0);
        assert_eq!(cells[1].exterior().coords().len(), 0);
        let total: f64 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_site_outside_the_envelope() {
        let points = vec![Coord::new(-50.0, -50.0), Coord::new(5.0, 5.0)];
        let cells = voronoi_polygons(&points, &env());
        let total: f64 = cells.iter().map(|c| c.area()).sum();
        assert!((total - 100.0).abs() < 1e-9);
    }
}
