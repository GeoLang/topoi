//! Topoi — Pure-Rust computational geometry engine.
//!
//! Boolean operations, buffering, Voronoi diagrams, Delaunay triangulation,
//! and topological predicates (DE-9IM) for 2D geometries.

mod algorithms;
mod buffer;
mod envelope;
mod error;
mod geometry;
mod predicates;

pub use algorithms::{convex_hull, segment_intersection, simplify};
pub use buffer::buffer_polygon;
pub use envelope::Envelope;
pub use error::Error;
pub use geometry::{Coord, LineString, MultiPolygon, Point, Polygon, Ring};
pub use predicates::{contains, intersects};
