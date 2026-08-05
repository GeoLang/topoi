# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- 2026-08-05: `voronoi_polygons`, cells clipped to an envelope and index-aligned
  with the input sites, built by half-plane clipping so collinear, duplicate and
  outside-the-envelope sites all work.
- 2026-08-05: `square_grid` and `hex_grid` (pointy-top, `cell_size` is the
  circumradius), covering an envelope completely and refusing a cell size that
  would ask for over a million cells.
- 2026-08-05: `validate` and `make_valid` with a serializable `ValidityIssue`:
  too few points, repeated vertices, ring self-intersection and holes reaching
  outside their shell. Repair puts polygons through a self-overlay, which splits
  a bowtie into a multipolygon and normalizes winding.
- 2026-08-05: FeatureCollection level ops, all keeping properties: `fc_buffer`,
  `fc_dissolve`, `fc_overlay` (intersection, difference, clip), `fc_spatial_join`
  (intersects, within, nearest), `fc_convex_hull`, `fc_centroid`, `fc_simplify`,
  `fc_clip_rect`, `fc_voronoi`, `fc_grid`, `fc_validate` and `fc_make_valid`.
  Errors name the feature index and the reason.
- 2026-08-05: GeoJSON-string WebAssembly bindings for every FeatureCollection op
  (`fcBuffer`, `fcDissolve`, `fcOverlay`, ...), taking and returning whole
  collections as strings, with enum arguments as lowercase names.
- `MultiPoint` and `MultiLineString` geometry types, and the matching
  `FeatureGeometry::MultiPoint`, `FeatureGeometry::MultiLineString` and
  `FeatureGeometry::GeometryCollection` variants, so GeoJSON I/O covers the
  whole RFC 7946 geometry model including nested collections.
- `centroid` for any `FeatureGeometry`, resolved by dimension: area-weighted
  for polygons, length-weighted for lines, mean for points, and for a mixed
  collection only the highest dimension present counts. A member with no
  extent at its own dimension falls to the next one down.
- `buffer_geometry`, buffering any geometry at a caller-chosen arc resolution.
  Points become discs, lines become round-capped capsules, polygons offset as
  before, and collection members are unioned. A negative distance shrinks
  polygons and leaves points and lines empty.
- `rasterize` and `GridWindow`: burn geometry-value pairs onto a grid, NaN
  background, last wins. Polygons fill by scanline (even-odd, holes), lines
  burn every cell the segment intersects via grid traversal, points use
  half-open cell membership, so tiled output matches whole-window output
  along pixel-aligned seams.
- `clip_segment_rect` (Liang-Barsky) and `clip_linestring_rect`, clipping
  segments and polylines to an axis-aligned rectangle, polylines split into
  parts where they leave the rectangle.
- `clip_to_boundary`, clipping any `FeatureGeometry` to a `MultiPolygon`
  boundary: polygons through the overlay engine, shaped back to a `Polygon`
  or a `MultiPolygon`, lines cut at their crossings with the boundary rings
  and kept where the midpoint is inside, points by containment, collections
  member by member. Vertices the boundary does not touch stay bit-exact.

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
- `JoinStyle` and `buffer_polygon_with_join`, for round, miter or bevel corners.
- `splitPolygon` WebAssembly binding.

### Changed

- `intersection_area` and `union_area` go through the overlay engine, so they are
  correct for concave rings instead of convex operands only.
- `parcel::merge_polygons` runs a real union, so inputs may be concave and may
  overlap instead of only sharing an edge. It still returns None when the result
  cannot be expressed as a single ring.
- The `clipToRect` WebAssembly binding returns an array of polygons and keeps
  holes, since a clip can split the input.
- `Ring::signed_area` no longer assumes the ring repeats its first coordinate.
- `buffer_polygon` uses the `i_overlay` outline engine instead of a vertex
  bisector approximation. It takes any `PolygonSet` and returns a `MultiPolygon`,
  handles concave rings and holes, and shrinks correctly for negative distances,
  including collapsing to nothing and splitting into separate pieces. The
  `bufferPolygon` WebAssembly binding returns an array of polygons to match.
- `parcel::split_polygon` cuts with a polyline through the `i_overlay` slice
  engine, replacing the two-crossing scan. It now takes `(subject, line)` and
  returns a `MultiPolygon`, so a cut may produce more than two pieces, and it
  handles concave rings and holes. A line that does not cross the boundary leaves
  the subject whole instead of returning None.
- Minimum supported Rust version is 1.88, required by `i_overlay`.

### Removed

- `polygon_intersection`, which was an alias for `clip_polygon` limited to convex
  operands. Use `intersection`.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
