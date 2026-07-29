# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- General polygon overlay backed by `i_overlay`: `union`, `intersection`,
  `difference`, `xor` and `boolean_op`, handling concave rings, holes,
  self-intersections and multipolygon operands. Operands are anything
  implementing `PolygonSet` (`Polygon`, `MultiPolygon`, `Vec<Polygon>`,
  `[Polygon]`) and results are always a `MultiPolygon`.
- `topoi overlay --op <union|intersection|difference|xor>` CLI command, printing
  GeoJSON.
- `polygonUnion`, `polygonIntersection`, `polygonDifference` and `polygonXor`
  WebAssembly bindings.
- `Polygon::from_coords` and `signed_ring_area`.

### Changed

- `intersection_area` and `union_area` go through the overlay engine, so they are
  correct for concave rings instead of convex operands only.
- `parcel::merge_polygons` runs a real union, so inputs may be concave and may
  overlap instead of only sharing an edge. It still returns None when the result
  cannot be expressed as a single ring.
- The `clipToRect` WebAssembly binding returns an array of polygons and keeps
  holes, since a clip can split the input.
- `Ring::signed_area` no longer assumes the ring repeats its first coordinate.
- Minimum supported Rust version is 1.88, required by `i_overlay`.

### Removed

- `polygon_intersection`, which was an alias for `clip_polygon` limited to convex
  operands. Use `intersection`.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
