# Topoi

Pure-Rust computational geometry engine for the TileTopia-HQ GIS stack.

## Features

- **Geometry types** — Point, LineString, Polygon, MultiPolygon, Ring
- **Spatial predicates** — point-in-polygon (ray casting), envelope intersection
- **Measurements** — area, length, centroid
- **Buffering** — vertex-offset polygon buffer (convex)
- **Envelope** — axis-aligned bounding box operations

## Usage

```rust
use topoi_core::{Coord, Polygon, Ring, contains};

let ring = Ring::new(vec![
    Coord::new(0.0, 0.0),
    Coord::new(4.0, 0.0),
    Coord::new(4.0, 4.0),
    Coord::new(0.0, 4.0),
    Coord::new(0.0, 0.0),
]);
let polygon = Polygon::new(ring, vec![]);
assert!(contains(&polygon, &Coord::new(2.0, 2.0)));
```

## CLI

```sh
topoi contains --px 2 --py 2 --ring 0,0,4,0,4,4,0,4,0,0
topoi area --ring 0,0,4,0,4,4,0,4,0,0
```

## License

AGPL-3.0-or-later
