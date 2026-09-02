//! Topoi — Pure-Rust computational geometry engine.
//!
//! Boolean operations, buffering, Voronoi diagrams, Delaunay triangulation,
//! and the `contains` and `intersects` predicates for 2D geometries.

mod algorithms;
mod buffer;
mod centroid;
mod clipping;
mod delaunay;
mod envelope;
mod error;
mod features;
pub mod geojson;
mod geometry;
mod grid;
mod overlay;
pub mod parcel;
mod predicates;
mod raster;
mod rtree;
mod validity;
mod voronoi;

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
pub use features::{
    FeatureIssues, GridKind, JoinPredicate, OverlayKind, ValidityReport, fc_buffer, fc_centroid,
    fc_clip_rect, fc_convex_hull, fc_dissolve, fc_grid, fc_make_valid, fc_overlay, fc_simplify,
    fc_spatial_join, fc_validate, fc_voronoi,
};
pub use geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
    signed_ring_area,
};
pub use grid::{hex_grid, square_grid};
pub use overlay::{BooleanOp, PolygonSet, boolean_op, difference, intersection, union, xor};
pub use predicates::{contains, intersects};
pub use raster::{GridWindow, rasterize};
pub use rtree::RTree;
pub use validity::{ValidityIssue, ValidityKind, make_valid, validate};
pub use voronoi::voronoi_polygons;
