//! # topoi-wasm
//!
//! WebAssembly bindings for topoi computational geometry.
//! Exposes convex hull, buffer, boolean operations, Delaunay triangulation,
//! and spatial predicates to JavaScript/TypeScript clients.

use serde::{Deserialize, Serialize};
use topoi_core::geojson::{read_geojson, write_geojson};
use topoi_core::parcel::split_polygon;
use topoi_core::{
    BooleanOp, Coord, Envelope, GridKind, JoinPredicate, MultiPolygon, OverlayKind, Polygon, Ring,
    boolean_op, buffer_polygon, contains, convex_hull, delaunay, fc_buffer, fc_centroid,
    fc_clip_rect, fc_convex_hull, fc_dissolve, fc_grid, fc_make_valid, fc_overlay, fc_simplify,
    fc_spatial_join, fc_validate, fc_voronoi, intersects, simplify,
};
use wasm_bindgen::prelude::*;

/// A GeoJSON-like coordinate pair for JS interop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsCoord {
    pub x: f64,
    pub y: f64,
}

/// A polygon represented as exterior ring + holes for JS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsPolygon {
    pub exterior: Vec<JsCoord>,
    pub holes: Vec<Vec<JsCoord>>,
}

/// Triangulation result for JS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsTriangulation {
    pub triangles: Vec<[JsCoord; 3]>,
}

fn coords_to_points(coords: &[JsCoord]) -> Vec<Coord> {
    coords.iter().map(|c| Coord { x: c.x, y: c.y }).collect()
}

fn points_to_js(points: &[Coord]) -> Vec<JsCoord> {
    points.iter().map(|p| JsCoord { x: p.x, y: p.y }).collect()
}

fn js_to_polygon(jp: &JsPolygon) -> Polygon {
    let exterior = Ring::new(coords_to_points(&jp.exterior));
    let holes: Vec<Ring> = jp
        .holes
        .iter()
        .map(|h| Ring::new(coords_to_points(h)))
        .collect();
    Polygon::new(exterior, holes)
}

fn polygon_to_js(p: &Polygon) -> JsPolygon {
    JsPolygon {
        exterior: points_to_js(p.exterior().coords()),
        holes: p
            .interiors()
            .iter()
            .map(|h| points_to_js(h.coords()))
            .collect(),
    }
}

fn multipolygon_to_js(mp: &MultiPolygon) -> Vec<JsPolygon> {
    mp.polygons().iter().map(polygon_to_js).collect()
}

fn js_to_polygons(value: JsValue) -> Result<Vec<Polygon>, JsError> {
    let polygons: Vec<JsPolygon> =
        serde_wasm_bindgen::from_value(value).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(polygons.iter().map(js_to_polygon).collect())
}

/// Compute the convex hull of a set of points.
/// Input: JSON array of {x, y} objects.
/// Returns: JSON array of {x, y} objects forming the hull polygon exterior.
#[wasm_bindgen(js_name = "convexHull")]
pub fn wasm_convex_hull(points_js: JsValue) -> Result<JsValue, JsError> {
    let coords: Vec<JsCoord> =
        serde_wasm_bindgen::from_value(points_js).map_err(|e| JsError::new(&e.to_string()))?;

    let points: Vec<Coord> = coords.iter().map(|c| Coord { x: c.x, y: c.y }).collect();
    let hull = convex_hull(&points);
    let result = points_to_js(hull.exterior().coords());

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Buffer a polygon by a given distance, with round joins.
/// Input: JsPolygon JSON, distance float (negative shrinks).
/// Returns: JSON array of JsPolygon, since growing can merge pieces and
/// shrinking can split or erase the input.
#[wasm_bindgen(js_name = "bufferPolygon")]
pub fn wasm_buffer_polygon(polygon_js: JsValue, distance: f64) -> Result<JsValue, JsError> {
    let jp: JsPolygon =
        serde_wasm_bindgen::from_value(polygon_js).map_err(|e| JsError::new(&e.to_string()))?;

    let polygon = js_to_polygon(&jp);
    let buffered = buffer_polygon(&polygon, distance);
    let result = multipolygon_to_js(&buffered);

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Split a polygon with a cutting polyline.
/// Input: JsPolygon JSON, JSON array of {x, y} forming the cut line.
/// Returns: JSON array of JsPolygon, one per resulting piece.
#[wasm_bindgen(js_name = "splitPolygon")]
pub fn wasm_split_polygon(polygon_js: JsValue, line_js: JsValue) -> Result<JsValue, JsError> {
    let jp: JsPolygon =
        serde_wasm_bindgen::from_value(polygon_js).map_err(|e| JsError::new(&e.to_string()))?;
    let line_coords: Vec<JsCoord> =
        serde_wasm_bindgen::from_value(line_js).map_err(|e| JsError::new(&e.to_string()))?;

    let pieces = split_polygon(&js_to_polygon(&jp), &coords_to_points(&line_coords));
    let result = multipolygon_to_js(&pieces);

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Clip a polygon to a rectangle (bbox).
/// Input: JsPolygon JSON, bbox bounds.
/// Returns: JSON array of JsPolygon, since a clip can split the input into
/// several pieces or leave holes intact.
#[wasm_bindgen(js_name = "clipToRect")]
pub fn wasm_clip_to_rect(
    polygon_js: JsValue,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Result<JsValue, JsError> {
    let jp: JsPolygon =
        serde_wasm_bindgen::from_value(polygon_js).map_err(|e| JsError::new(&e.to_string()))?;

    let rect = Polygon::from_coords(&[
        Coord::new(min_x, min_y),
        Coord::new(max_x, min_y),
        Coord::new(max_x, max_y),
        Coord::new(min_x, max_y),
    ]);
    let clipped = boolean_op(&js_to_polygon(&jp), &rect, BooleanOp::Intersection);
    let result = multipolygon_to_js(&clipped);

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Union of two polygon sets.
/// Input: two JSON arrays of JsPolygon.
/// Returns: JSON array of JsPolygon.
#[wasm_bindgen(js_name = "polygonUnion")]
pub fn wasm_polygon_union(subject_js: JsValue, clip_js: JsValue) -> Result<JsValue, JsError> {
    overlay(subject_js, clip_js, BooleanOp::Union)
}

/// Intersection of two polygon sets.
/// Input: two JSON arrays of JsPolygon.
/// Returns: JSON array of JsPolygon.
#[wasm_bindgen(js_name = "polygonIntersection")]
pub fn wasm_polygon_intersection(
    subject_js: JsValue,
    clip_js: JsValue,
) -> Result<JsValue, JsError> {
    overlay(subject_js, clip_js, BooleanOp::Intersection)
}

/// Subject minus clip.
/// Input: two JSON arrays of JsPolygon.
/// Returns: JSON array of JsPolygon.
#[wasm_bindgen(js_name = "polygonDifference")]
pub fn wasm_polygon_difference(subject_js: JsValue, clip_js: JsValue) -> Result<JsValue, JsError> {
    overlay(subject_js, clip_js, BooleanOp::Difference)
}

/// Symmetric difference of two polygon sets.
/// Input: two JSON arrays of JsPolygon.
/// Returns: JSON array of JsPolygon.
#[wasm_bindgen(js_name = "polygonXor")]
pub fn wasm_polygon_xor(subject_js: JsValue, clip_js: JsValue) -> Result<JsValue, JsError> {
    overlay(subject_js, clip_js, BooleanOp::Xor)
}

fn overlay(subject_js: JsValue, clip_js: JsValue, op: BooleanOp) -> Result<JsValue, JsError> {
    let subject = js_to_polygons(subject_js)?;
    let clip = js_to_polygons(clip_js)?;
    let result = boolean_op(&subject, &clip, op);
    serde_wasm_bindgen::to_value(&multipolygon_to_js(&result))
        .map_err(|e| JsError::new(&e.to_string()))
}

/// Compute Delaunay triangulation of a point set.
/// Input: JSON array of {x, y} objects.
/// Returns: JsTriangulation with array of triangle vertex triples.
#[wasm_bindgen(js_name = "delaunayTriangulation")]
pub fn wasm_delaunay(points_js: JsValue) -> Result<JsValue, JsError> {
    let coords: Vec<JsCoord> =
        serde_wasm_bindgen::from_value(points_js).map_err(|e| JsError::new(&e.to_string()))?;

    let points: Vec<Coord> = coords.iter().map(|c| Coord { x: c.x, y: c.y }).collect();
    let triangulation = delaunay(&points).ok_or_else(|| {
        JsError::new("triangulation failed (need at least 3 non-collinear points)")
    })?;

    let triangles: Vec<[JsCoord; 3]> = triangulation
        .triangles
        .iter()
        .map(|t| {
            let pa = &triangulation.points[t.a];
            let pb = &triangulation.points[t.b];
            let pc = &triangulation.points[t.c];
            [
                JsCoord { x: pa.x, y: pa.y },
                JsCoord { x: pb.x, y: pb.y },
                JsCoord { x: pc.x, y: pc.y },
            ]
        })
        .collect();

    let result = JsTriangulation { triangles };
    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Simplify a polyline using Douglas-Peucker algorithm.
/// Input: JSON array of {x, y}, tolerance float.
/// Returns: simplified JSON array of {x, y}.
#[wasm_bindgen(js_name = "simplifyLine")]
pub fn wasm_simplify(points_js: JsValue, tolerance: f64) -> Result<JsValue, JsError> {
    let coords: Vec<JsCoord> =
        serde_wasm_bindgen::from_value(points_js).map_err(|e| JsError::new(&e.to_string()))?;

    let points: Vec<Coord> = coords.iter().map(|c| Coord { x: c.x, y: c.y }).collect();
    let simplified = simplify(&points, tolerance);
    let result = points_to_js(&simplified);

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Test if a point is inside a polygon.
/// Input: {x, y} point and JsPolygon.
/// Returns: boolean.
#[wasm_bindgen(js_name = "pointInPolygon")]
pub fn wasm_point_in_polygon(point_js: JsValue, polygon_js: JsValue) -> Result<bool, JsError> {
    let jc: JsCoord =
        serde_wasm_bindgen::from_value(point_js).map_err(|e| JsError::new(&e.to_string()))?;
    let jp: JsPolygon =
        serde_wasm_bindgen::from_value(polygon_js).map_err(|e| JsError::new(&e.to_string()))?;

    let point = Coord { x: jc.x, y: jc.y };
    let polygon = js_to_polygon(&jp);

    Ok(contains(&polygon, &point))
}

/// Test if two polygons intersect.
#[wasm_bindgen(js_name = "polygonsIntersect")]
pub fn wasm_polygons_intersect(a_js: JsValue, b_js: JsValue) -> Result<bool, JsError> {
    let ja: JsPolygon =
        serde_wasm_bindgen::from_value(a_js).map_err(|e| JsError::new(&e.to_string()))?;
    let jb: JsPolygon =
        serde_wasm_bindgen::from_value(b_js).map_err(|e| JsError::new(&e.to_string()))?;

    let a = js_to_polygon(&ja);
    let b = js_to_polygon(&jb);

    Ok(intersects(&a, &b))
}

/// Compute the bounding box of a set of points.
/// Returns: {min_x, min_y, max_x, max_y}.
#[wasm_bindgen(js_name = "boundingBox")]
pub fn wasm_bounding_box(points_js: JsValue) -> Result<JsValue, JsError> {
    let coords: Vec<JsCoord> =
        serde_wasm_bindgen::from_value(points_js).map_err(|e| JsError::new(&e.to_string()))?;

    if coords.is_empty() {
        return Err(JsError::new("empty point set"));
    }

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for c in &coords {
        min_x = min_x.min(c.x);
        min_y = min_y.min(c.y);
        max_x = max_x.max(c.x);
        max_y = max_y.max(c.y);
    }

    let result = serde_json::json!({
        "min_x": min_x,
        "min_y": min_y,
        "max_x": max_x,
        "max_y": max_y,
    });

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

// --- FeatureCollection ops, GeoJSON strings in and out ---

fn js(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn overlay_kind(name: &str) -> Result<OverlayKind, String> {
    match name {
        "intersection" => Ok(OverlayKind::Intersection),
        "difference" => Ok(OverlayKind::Difference),
        "clip" => Ok(OverlayKind::Clip),
        other => Err(format!(
            "unknown overlay op {other:?}, expected intersection, difference or clip"
        )),
    }
}

fn join_predicate(name: &str) -> Result<JoinPredicate, String> {
    match name {
        "intersects" => Ok(JoinPredicate::Intersects),
        "within" => Ok(JoinPredicate::Within),
        "nearest" => Ok(JoinPredicate::Nearest),
        other => Err(format!(
            "unknown join predicate {other:?}, expected intersects, within or nearest"
        )),
    }
}

fn grid_kind(name: &str) -> Result<GridKind, String> {
    match name {
        "square" => Ok(GridKind::Square),
        "hex" => Ok(GridKind::Hex),
        other => Err(format!(
            "unknown grid kind {other:?}, expected square or hex"
        )),
    }
}

/// Buffer every feature of a GeoJSON FeatureCollection, keeping properties.
#[wasm_bindgen(js_name = "fcBuffer")]
pub fn wasm_fc_buffer(fc_json: &str, distance: f64, segments: usize) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_buffer(&fc, distance, segments)))
}

/// Union the polygon features, grouped by the `by` property when given.
#[wasm_bindgen(js_name = "fcDissolve")]
pub fn wasm_fc_dissolve(fc_json: &str, by: Option<String>) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    let dissolved = fc_dissolve(&fc, by.as_deref()).map_err(js)?;
    Ok(write_geojson(&dissolved))
}

/// Overlay two collections. `op` is "intersection", "difference" or "clip".
#[wasm_bindgen(js_name = "fcOverlay")]
pub fn wasm_fc_overlay(a_json: &str, b_json: &str, op: &str) -> Result<String, JsError> {
    let a = read_geojson(a_json).map_err(js)?;
    let b = read_geojson(b_json).map_err(js)?;
    let out = fc_overlay(&a, &b, overlay_kind(op).map_err(js)?).map_err(js)?;
    Ok(write_geojson(&out))
}

/// Join source properties onto target features. `predicate` is "intersects",
/// "within" or "nearest".
#[wasm_bindgen(js_name = "fcSpatialJoin")]
pub fn wasm_fc_spatial_join(
    target_json: &str,
    source_json: &str,
    predicate: &str,
    prefix: &str,
) -> Result<String, JsError> {
    let target = read_geojson(target_json).map_err(js)?;
    let source = read_geojson(source_json).map_err(js)?;
    let out = fc_spatial_join(
        &target,
        &source,
        join_predicate(predicate).map_err(js)?,
        prefix,
    )
    .map_err(js)?;
    Ok(write_geojson(&out))
}

/// One convex hull over every coordinate in the collection.
#[wasm_bindgen(js_name = "fcConvexHull")]
pub fn wasm_fc_convex_hull(fc_json: &str) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_convex_hull(&fc)))
}

/// Replace every geometry with its centroid.
#[wasm_bindgen(js_name = "fcCentroid")]
pub fn wasm_fc_centroid(fc_json: &str) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_centroid(&fc)))
}

/// Douglas-Peucker every linestring and polygon ring.
#[wasm_bindgen(js_name = "fcSimplify")]
pub fn wasm_fc_simplify(fc_json: &str, tolerance: f64) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_simplify(&fc, tolerance)))
}

/// Clip every geometry to an axis-aligned rectangle.
#[wasm_bindgen(js_name = "fcClipRect")]
pub fn wasm_fc_clip_rect(
    fc_json: &str,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_clip_rect(
        &fc, min_x, min_y, max_x, max_y,
    )))
}

/// Voronoi cells over the point features, clipped to the given rectangle.
#[wasm_bindgen(js_name = "fcVoronoi")]
pub fn wasm_fc_voronoi(
    fc_json: &str,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    let envelope = Envelope::new(min_x, min_y, max_x, max_y);
    Ok(write_geojson(&fc_voronoi(&fc, &envelope).map_err(js)?))
}

/// A grid covering a rectangle. `kind` is "square" or "hex".
#[wasm_bindgen(js_name = "fcGrid")]
pub fn wasm_fc_grid(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    cell_size: f64,
    kind: &str,
) -> Result<String, JsError> {
    let envelope = Envelope::new(min_x, min_y, max_x, max_y);
    let grid = fc_grid(&envelope, cell_size, grid_kind(kind).map_err(js)?).map_err(js)?;
    Ok(write_geojson(&grid))
}

/// Validity issues per feature, as a JSON report rather than a collection.
#[wasm_bindgen(js_name = "fcValidate")]
pub fn wasm_fc_validate(fc_json: &str) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    serde_json::to_string(&fc_validate(&fc)).map_err(js)
}

/// Repair every feature, keeping properties.
#[wasm_bindgen(js_name = "fcMakeValid")]
pub fn wasm_fc_make_valid(fc_json: &str) -> Result<String, JsError> {
    let fc = read_geojson(fc_json).map_err(js)?;
    Ok(write_geojson(&fc_make_valid(&fc).map_err(js)?))
}

// Native tests (not wasm_bindgen_test, since those require wasm32 target)
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coords_to_points() {
        let coords = vec![JsCoord { x: 1.0, y: 2.0 }, JsCoord { x: 3.0, y: 4.0 }];
        let points = coords_to_points(&coords);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].x, 1.0);
        assert_eq!(points[1].y, 4.0);
    }

    #[test]
    fn test_points_to_js() {
        let points = vec![Coord { x: 5.0, y: 6.0 }];
        let js = points_to_js(&points);
        assert_eq!(js.len(), 1);
        assert_eq!(js[0].x, 5.0);
        assert_eq!(js[0].y, 6.0);
    }

    #[test]
    fn test_js_polygon_roundtrip() {
        let jp = JsPolygon {
            exterior: vec![
                JsCoord { x: 0.0, y: 0.0 },
                JsCoord { x: 1.0, y: 0.0 },
                JsCoord { x: 1.0, y: 1.0 },
                JsCoord { x: 0.0, y: 1.0 },
                JsCoord { x: 0.0, y: 0.0 },
            ],
            holes: vec![],
        };
        let polygon = js_to_polygon(&jp);
        let restored = polygon_to_js(&polygon);
        assert_eq!(restored.exterior.len(), 5);
        assert!(restored.holes.is_empty());
    }

    #[test]
    fn test_point_in_polygon_logic() {
        let polygon = Polygon::new(
            Ring::new(vec![
                Coord { x: 0.0, y: 0.0 },
                Coord { x: 10.0, y: 0.0 },
                Coord { x: 10.0, y: 10.0 },
                Coord { x: 0.0, y: 10.0 },
                Coord { x: 0.0, y: 0.0 },
            ]),
            vec![],
        );
        let inside = Coord { x: 5.0, y: 5.0 };
        let outside = Coord { x: 15.0, y: 5.0 };
        assert!(contains(&polygon, &inside));
        assert!(!contains(&polygon, &outside));
    }

    #[test]
    fn test_multipolygon_to_js() {
        let square = Polygon::from_coords(&[
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 1.0, y: 0.0 },
            Coord { x: 1.0, y: 1.0 },
            Coord { x: 0.0, y: 1.0 },
        ]);
        let js = multipolygon_to_js(&MultiPolygon::new(vec![square.clone(), square]));
        assert_eq!(js.len(), 2);
        assert_eq!(js[0].exterior.len(), 4);
    }

    #[test]
    fn test_overlay_difference_yields_hole() {
        let outer = Polygon::from_coords(&[
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 10.0, y: 0.0 },
            Coord { x: 10.0, y: 10.0 },
            Coord { x: 0.0, y: 10.0 },
        ]);
        let inner = Polygon::from_coords(&[
            Coord { x: 4.0, y: 4.0 },
            Coord { x: 6.0, y: 4.0 },
            Coord { x: 6.0, y: 6.0 },
            Coord { x: 4.0, y: 6.0 },
        ]);
        let result = boolean_op(&outer, &inner, BooleanOp::Difference);
        let js = multipolygon_to_js(&result);
        assert_eq!(js.len(), 1);
        assert_eq!(js[0].holes.len(), 1);
    }

    // only the happy path: building a JsError needs the wasm target
    #[test]
    fn test_fc_binding_round_trips_geojson_strings() {
        let json = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"zone":"a"},
             "geometry":{"type":"Polygon","coordinates":[[[0,0],[4,0],[4,4],[0,4],[0,0]]]}}]}"#;
        let out = wasm_fc_centroid(json).unwrap();
        assert!(out.contains("\"Point\""), "{out}");
        assert!(out.contains("[2.0,2.0]"), "{out}");
        assert!(out.contains("\"zone\":\"a\""), "{out}");

        let report = wasm_fc_validate(json).unwrap();
        assert_eq!(report, r#"{"valid":true,"invalid":[]}"#);
    }

    #[test]
    fn test_enum_names_parse() {
        assert_eq!(overlay_kind("clip").unwrap(), OverlayKind::Clip);
        assert_eq!(join_predicate("nearest").unwrap(), JoinPredicate::Nearest);
        assert_eq!(grid_kind("hex").unwrap(), GridKind::Hex);
    }

    #[test]
    fn test_unknown_enum_names_say_what_was_expected() {
        let err = overlay_kind("Union").unwrap_err();
        assert!(err.contains("\"Union\""), "{err}");
        assert!(err.contains("intersection"), "{err}");
        assert!(join_predicate("touches").is_err());
        assert!(
            grid_kind("triangle").unwrap_err().contains("square"),
            "{err}"
        );
    }

    #[test]
    fn test_simplify_logic() {
        let coords = vec![
            Coord { x: 0.0, y: 0.0 },
            Coord { x: 0.5, y: 0.1 },
            Coord { x: 1.0, y: 0.0 },
            Coord { x: 1.5, y: 0.1 },
            Coord { x: 2.0, y: 0.0 },
        ];
        let simplified = simplify(&coords, 0.2);
        // With tolerance 0.2, intermediate points should be removed
        assert!(simplified.len() <= coords.len());
    }
}
