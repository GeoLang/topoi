# Topoi

Pure-Rust computational geometry engine for the GeoLang GIS stack.

[Documentation](https://geolang.github.io/topoi/) · [GitHub](https://github.com/GeoLang/topoi)

## Features

- **Geometry types** — Point, LineString, Polygon, MultiPolygon, Ring, Envelope
- **Spatial predicates** — point-in-polygon (ray casting), envelope intersection, `contains`, `intersects`
- **Measurements** — area, signed area, length, centroid, distance
- **Buffering** — vertex-offset polygon buffer (convex)
- **Convex hull** — Graham scan algorithm
- **Delaunay triangulation** — incremental with Voronoi dual (circumcenters)
- **Boolean operations** — polygon intersection, union area, intersection area
- **Polygon clipping** — Sutherland-Hodgman (convex clip polygon), rectangle clipping
- **Simplification** — Douglas-Peucker polyline/polygon simplification
- **Segment intersection** — exact 2D line segment intersection detection
- **R-tree spatial index** — bulk-loaded, bounding-box queries, nearest-neighbor
- **GeoJSON I/O** — read/write FeatureCollections
- **Parcel operations** — subdivision and merge utilities
- **WebAssembly SDK** — `topoi-wasm` crate exposing convex hull, buffer, clip, Delaunay, simplify, point-in-polygon, polygon intersection, and bounding box to JavaScript via `wasm-bindgen`

## Usage

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

## CLI

```sh
topoi contains --px 2 --py 2 --ring 0,0,4,0,4,4,0,4,0,0
topoi area --ring 0,0,4,0,4,4,0,4,0,0
```

## Architecture

```
topoi-core    — geometry types, algorithms, predicates, R-tree
topoi-cli     — command-line interface
```

## License

AGPL-3.0-or-later
