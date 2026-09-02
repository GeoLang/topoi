# Topoi

[![CI](https://github.com/GeoLang/topoi/actions/workflows/ci.yml/badge.svg)](https://github.com/GeoLang/topoi/actions)
[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)

Pure-Rust computational geometry engine for the GeoLang GIS stack.

[Documentation](https://geolang.github.io/topoi/) · [GitHub](https://github.com/GeoLang/topoi)

## Features

- **Geometry types**: Point, LineString, Polygon, MultiPoint, MultiLineString, MultiPolygon, Ring, Envelope
- **Spatial predicates**: point-in-polygon (ray casting), `contains` with holes excluded, `intersects` for two polygons by ring crossings and vertex containment, envelope intersection and containment
- **Measurements**: area, signed area, length, distance, and `centroid` for any geometry, resolved by dimension (areal beats linear beats puntal)
- **Buffering**: `buffer_geometry` for any geometry, points into discs and lines into round-capped capsules, and polygon offsetting with round, miter or bevel joins, positive or negative, on concave polygons and polygons with holes
- **Convex hull**: Graham scan algorithm
- **Delaunay triangulation**: incremental with Voronoi dual (circumcenters)
- **Voronoi cells**: `voronoi_polygons` builds cells clipped to an envelope by half-plane clipping, index-aligned with the input sites
- **Grids**: `square_grid` and `hex_grid` (pointy-top, `cell_size` is the circumradius) cover an envelope completely and refuse a cell size that would ask for over a million cells
- **Boolean operations**: general polygon overlay (union, intersection, difference, xor) on concave polygons, polygons with holes and multipolygons, via [i_overlay](https://crates.io/crates/i_overlay)
- **Clipping**: Sutherland-Hodgman fast path for convex clip windows, rectangle clipping, Liang-Barsky segment and polyline clipping, and `clip_to_boundary` for any geometry against a `MultiPolygon`
- **Simplification**: Douglas-Peucker polyline/polygon simplification
- **Segment intersection**: 2D segment intersection point, collinear overlaps included
- **Validity**: `validate` reports too few points, repeated vertices, ring self-intersection and holes reaching outside their shell. `make_valid` repairs a geometry through a self-overlay, which splits a bowtie into a multipolygon and normalizes winding
- **Rasterization**: `rasterize` burns geometry-value pairs onto a `GridWindow`, polygons by scanline, lines by grid traversal, points by half-open cell membership, so tiled output matches whole-window output along pixel-aligned seams
- **R-tree spatial index**: bulk-loaded, bounding-box queries, nearest-neighbor
- **GeoJSON I/O**: read/write FeatureCollections over the full RFC 7946 geometry model, nested GeometryCollections included
- **FeatureCollection operations**: `fc_buffer`, `fc_dissolve`, `fc_overlay` (intersection, difference, clip), `fc_spatial_join` (intersects, within, nearest), `fc_convex_hull`, `fc_centroid`, `fc_simplify`, `fc_clip_rect`, `fc_voronoi`, `fc_grid`, `fc_validate` and `fc_make_valid`, all keeping feature properties
- **Parcel operations**: split a polygon set with a cutting polyline, merge adjacent or overlapping polygons
- **WebAssembly SDK**: `topoi-wasm` crate exposing convex hull, buffer, clip, split, Delaunay, simplify, point-in-polygon, boolean overlay, and bounding box to JavaScript via `wasm-bindgen`, plus a GeoJSON-string binding for every FeatureCollection op (`fcBuffer`, `fcDissolve`, `fcOverlay` and the rest), taking and returning whole collections as strings

## Usage

Requires Rust 1.88 or newer, the minimum for the `i_overlay` 7.0 dependency.

```rust
use topoi_core::{Coord, Polygon, Ring, contains, convex_hull, delaunay, simplify};

// Point-in-polygon
let ring = Ring::new(vec![
    Coord::new(0.0, 0.0),
    Coord::new(4.0, 0.0),
    Coord::new(4.0, 4.0),
    Coord::new(0.0, 4.0),
    Coord::new(0.0, 0.0),
]);
let polygon = Polygon::new(ring, vec![]);
assert!(contains(&polygon, &Coord::new(2.0, 2.0)));

// Convex hull
let points = vec![
    Coord::new(0.0, 0.0), Coord::new(1.0, 3.0),
    Coord::new(3.0, 1.0), Coord::new(2.0, 2.0),
];
let hull = convex_hull(&points);

// Delaunay triangulation
let tri = delaunay(&points);
let voronoi = tri.voronoi_vertices();

// Simplification
let line = vec![
    Coord::new(0.0, 0.0), Coord::new(1.0, 0.1),
    Coord::new(2.0, 0.0), Coord::new(3.0, 0.0),
];
let simplified = simplify(&line, 0.2);
```

### Boolean overlay

`union`, `intersection`, `difference` and `xor` take a `Polygon`, a
`MultiPolygon` or a slice of polygons on either side, and always return a
`MultiPolygon` because any of them can split the input or punch holes in it.

```rust
use topoi_core::{Coord, Polygon, difference, union};

let outer = Polygon::from_coords(&[
    Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
    Coord::new(10.0, 10.0), Coord::new(0.0, 10.0),
]);
let inner = Polygon::from_coords(&[
    Coord::new(4.0, 4.0), Coord::new(6.0, 4.0),
    Coord::new(6.0, 6.0), Coord::new(4.0, 6.0),
]);

// One polygon with a hole, area 96
let holed = difference(&outer, &inner);
assert_eq!(holed.polygons()[0].interiors().len(), 1);

// Filling the hole back in
let whole = union(&holed, &inner);
assert!((whole.area() - 100.0).abs() < 1e-9);
```

Interior rings are treated as holes whatever winding they were given, and
results follow the GeoJSON right-hand rule: exteriors counter-clockwise, holes
clockwise.

### Buffering and splitting

`buffer_polygon` offsets a polygon set outward for a positive distance and
inward for a negative one. `parcel::split_polygon` cuts a polygon set with a
polyline. Both take the same operands as the boolean ops and return a
`MultiPolygon`, because a buffer can merge or erase pieces and a cut can produce
any number of them.

```rust
use topoi_core::{Coord, JoinStyle, Polygon, buffer_polygon, buffer_polygon_with_join};
use topoi_core::parcel::split_polygon;

let square = Polygon::from_coords(&[
    Coord::new(0.0, 0.0), Coord::new(10.0, 0.0),
    Coord::new(10.0, 10.0), Coord::new(0.0, 10.0),
]);

// Shrinking a convex ring adds no arcs, so this is exactly 8x8
let inset = buffer_polygon(&square, -1.0);
assert!((inset.area() - 64.0).abs() < 1e-9);

// A negative distance past the inradius leaves nothing
assert!(buffer_polygon(&square, -6.0).polygons().is_empty());

// Sharp corners instead of arcs
let mitred = buffer_polygon_with_join(&square, 1.0, JoinStyle::Miter);
assert!((mitred.area() - 144.0).abs() < 1e-9);

// Cut into two halves
let halves = split_polygon(&square, &[Coord::new(-1.0, 5.0), Coord::new(11.0, 5.0)]);
assert_eq!(halves.polygons().len(), 2);
```

The cutting line is used as given, so it has to cross the boundary to cut. A
line that stops inside leaves the polygon whole.

## CLI

```sh
topoi contains --px 2 --py 2 --ring 0,0,4,0,4,4,0,4,0,0
topoi area --ring 0,0,4,0,4,4,0,4,0,0
topoi overlay --op union --subject 0,0,2,0,2,2,0,2 --clip 1,1,3,1,3,3,1,3
```

## Architecture

```
topoi-core:   geometry types, algorithms, predicates, R-tree
topoi-wasm:   wasm-bindgen bindings for the browser
topoi-cli:    command-line interface
```

## License

AGPL-3.0-or-later, see [LICENSE](LICENSE).

Copyright (C) 2026 Grok Image Compression Inc.
