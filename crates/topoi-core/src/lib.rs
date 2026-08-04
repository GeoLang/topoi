//! Topoi — Pure-Rust computational geometry engine.
//!
//! Boolean operations, buffering, Voronoi diagrams, Delaunay triangulation,
//! and topological predicates (DE-9IM) for 2D geometries.

mod algorithms;
mod buffer;
mod centroid;
mod clipping;
mod delaunay;
mod envelope;
mod error;
pub mod geojson;
mod geometry;
mod overlay;
pub mod parcel;
mod predicates;
mod raster;
mod rtree;

pub use algorithms::{convex_hull, segment_intersection, simplify};
pub use buffer::{JoinStyle, buffer_geometry, buffer_polygon, buffer_polygon_with_join};
pub use centroid::centroid;
pub use clipping::{
    clip_linestring_rect, clip_polygon, clip_polygon_rect, clip_segment_rect, clip_to_boundary,
    intersection_area, union_area,
};
pub use delaunay::{Triangle, Triangulation, delaunay};
pub use envelope::Envelope;
pub use error::Error;
pub use geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
    signed_ring_area,
};
pub use overlay::{BooleanOp, PolygonSet, boolean_op, difference, intersection, union, xor};
pub use predicates::{contains, intersects};
pub use raster::{GridWindow, rasterize};
pub use rtree::RTree;
