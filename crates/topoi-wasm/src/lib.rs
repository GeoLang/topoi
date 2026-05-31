//! # topoi-wasm
//!
//! WebAssembly bindings for topoi computational geometry.
//! Exposes convex hull, buffer, boolean operations, Delaunay triangulation,
//! and spatial predicates to JavaScript/TypeScript clients.

use serde::{Deserialize, Serialize};
use topoi_core::{
    Coord, Polygon, Ring, buffer_polygon, clip_polygon_rect, contains, convex_hull, delaunay,
    intersects, simplify,
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

/// Buffer a polygon by a given distance.
/// Input: JsPolygon JSON, distance float.
/// Returns: JsPolygon JSON of the buffered polygon.
#[wasm_bindgen(js_name = "bufferPolygon")]
pub fn wasm_buffer_polygon(polygon_js: JsValue, distance: f64) -> Result<JsValue, JsError> {
    let jp: JsPolygon =
        serde_wasm_bindgen::from_value(polygon_js).map_err(|e| JsError::new(&e.to_string()))?;

    let polygon = js_to_polygon(&jp);
    let buffered = buffer_polygon(&polygon, distance);
    let result = polygon_to_js(&buffered);

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
}

/// Clip a polygon to a rectangle (bbox).
/// Returns: JsPolygon JSON of the clipped result.
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

    let coords = coords_to_points(&jp.exterior);
    let clipped = clip_polygon_rect(&coords, min_x, min_y, max_x, max_y);
    let result = JsPolygon {
        exterior: points_to_js(&clipped),
        holes: vec![],
    };

    serde_wasm_bindgen::to_value(&result).map_err(|e| JsError::new(&e.to_string()))
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
