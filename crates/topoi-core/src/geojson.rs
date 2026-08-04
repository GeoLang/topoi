//! GeoJSON reader/writer for Topoi geometry types.

use crate::geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// A GeoJSON Feature with geometry and properties.
#[derive(Debug, Clone)]
pub struct Feature {
    pub geometry: Option<FeatureGeometry>,
    pub properties: HashMap<String, Value>,
}

/// Geometry types representable in GeoJSON.
#[derive(Debug, Clone)]
pub enum FeatureGeometry {
    Point(Point),
    LineString(LineString),
    Polygon(Polygon),
    MultiPoint(MultiPoint),
    MultiLineString(MultiLineString),
    MultiPolygon(MultiPolygon),
    /// A heterogeneous set of geometries, which may itself contain collections.
    GeometryCollection(Vec<FeatureGeometry>),
}

/// A GeoJSON FeatureCollection.
#[derive(Debug, Clone)]
pub struct FeatureCollection {
    pub features: Vec<Feature>,
}

/// Parse a GeoJSON string into a FeatureCollection.
pub fn read_geojson(json: &str) -> Result<FeatureCollection, crate::Error> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| crate::Error::ParseError(e.to_string()))?;

    match value.get("type").and_then(|t| t.as_str()) {
        Some("FeatureCollection") => {
            let features_val = value
                .get("features")
                .and_then(|f| f.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing features array".into()))?;

            let features: Vec<Feature> = features_val.iter().filter_map(parse_feature).collect();
            Ok(FeatureCollection { features })
        }
        Some("Feature") => {
            let feature = parse_feature(&value)
                .ok_or_else(|| crate::Error::ParseError("invalid feature".into()))?;
            Ok(FeatureCollection {
                features: vec![feature],
            })
        }
        Some(geom_type) if is_geometry_type(geom_type) => {
            let geom = parse_geometry(&value)?;
            Ok(FeatureCollection {
                features: vec![Feature {
                    geometry: Some(geom),
                    properties: HashMap::new(),
                }],
            })
        }
        _ => Err(crate::Error::ParseError("unknown GeoJSON type".into())),
    }
}

/// Write a FeatureCollection as a GeoJSON string.
pub fn write_geojson(fc: &FeatureCollection) -> String {
    let features: Vec<Value> = fc.features.iter().map(feature_to_value).collect();
    let mut obj = Map::new();
    obj.insert("type".into(), Value::String("FeatureCollection".into()));
    obj.insert("features".into(), Value::Array(features));
    serde_json::to_string(&Value::Object(obj)).unwrap()
}

/// Write a FeatureCollection as pretty-printed GeoJSON.
pub fn write_geojson_pretty(fc: &FeatureCollection) -> String {
    let features: Vec<Value> = fc.features.iter().map(feature_to_value).collect();
    let mut obj = Map::new();
    obj.insert("type".into(), Value::String("FeatureCollection".into()));
    obj.insert("features".into(), Value::Array(features));
    serde_json::to_string_pretty(&Value::Object(obj)).unwrap()
}

fn feature_to_value(feature: &Feature) -> Value {
    let mut obj = Map::new();
    obj.insert("type".into(), Value::String("Feature".into()));
    match &feature.geometry {
        Some(geom) => obj.insert("geometry".into(), geometry_to_value(geom)),
        None => obj.insert("geometry".into(), Value::Null),
    };
    let props: Map<String, Value> = feature.properties.clone().into_iter().collect();
    obj.insert("properties".into(), Value::Object(props));
    Value::Object(obj)
}

fn geometry_to_value(geom: &FeatureGeometry) -> Value {
    let mut obj = Map::new();
    match geom {
        FeatureGeometry::Point(p) => {
            obj.insert("type".into(), Value::String("Point".into()));
            obj.insert("coordinates".into(), coord_to_value(&p.0));
        }
        FeatureGeometry::LineString(ls) => {
            obj.insert("type".into(), Value::String("LineString".into()));
            let coords: Vec<Value> = ls.coords().iter().map(coord_to_value).collect();
            obj.insert("coordinates".into(), Value::Array(coords));
        }
        FeatureGeometry::Polygon(poly) => {
            obj.insert("type".into(), Value::String("Polygon".into()));
            obj.insert("coordinates".into(), polygon_coords_to_value(poly));
        }
        FeatureGeometry::MultiPoint(mp) => {
            obj.insert("type".into(), Value::String("MultiPoint".into()));
            let coords: Vec<Value> = mp.points().iter().map(|p| coord_to_value(&p.0)).collect();
            obj.insert("coordinates".into(), Value::Array(coords));
        }
        FeatureGeometry::MultiLineString(mls) => {
            obj.insert("type".into(), Value::String("MultiLineString".into()));
            let lines: Vec<Value> = mls
                .linestrings()
                .iter()
                .map(|ls| Value::Array(ls.coords().iter().map(coord_to_value).collect()))
                .collect();
            obj.insert("coordinates".into(), Value::Array(lines));
        }
        FeatureGeometry::MultiPolygon(mp) => {
            obj.insert("type".into(), Value::String("MultiPolygon".into()));
            let polys: Vec<Value> = mp.polygons().iter().map(polygon_coords_to_value).collect();
            obj.insert("coordinates".into(), Value::Array(polys));
        }
        FeatureGeometry::GeometryCollection(members) => {
            obj.insert("type".into(), Value::String("GeometryCollection".into()));
            let geoms: Vec<Value> = members.iter().map(geometry_to_value).collect();
            obj.insert("geometries".into(), Value::Array(geoms));
        }
    }
    Value::Object(obj)
}

fn polygon_coords_to_value(poly: &Polygon) -> Value {
    let mut rings = Vec::new();
    let ext: Vec<Value> = poly
        .exterior()
        .coords()
        .iter()
        .map(coord_to_value)
        .collect();
    rings.push(Value::Array(ext));
    for hole in poly.interiors() {
        let h: Vec<Value> = hole.coords().iter().map(coord_to_value).collect();
        rings.push(Value::Array(h));
    }
    Value::Array(rings)
}

fn coord_to_value(c: &Coord) -> Value {
    Value::Array(vec![Value::from(c.x), Value::from(c.y)])
}

fn parse_feature(value: &Value) -> Option<Feature> {
    let properties = value
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default();

    let geometry = value
        .get("geometry")
        .filter(|g| !g.is_null())
        .and_then(|g| parse_geometry(g).ok());

    Some(Feature {
        geometry,
        properties,
    })
}

fn parse_geometry(value: &Value) -> Result<FeatureGeometry, crate::Error> {
    let geom_type = value
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| crate::Error::ParseError("missing geometry type".into()))?;

    match geom_type {
        "Point" => {
            let coords = value
                .get("coordinates")
                .and_then(|c| c.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?;
            let c = parse_coord(coords)?;
            Ok(FeatureGeometry::Point(Point::new(c.x, c.y)))
        }
        "LineString" => {
            let coords = parse_coord_array(
                value
                    .get("coordinates")
                    .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?,
            )?;
            Ok(FeatureGeometry::LineString(LineString::new(coords)))
        }
        "Polygon" => {
            let rings = value
                .get("coordinates")
                .and_then(|c| c.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?;
            let poly = parse_polygon_rings(rings)?;
            Ok(FeatureGeometry::Polygon(poly))
        }
        "MultiPoint" => {
            let coords = parse_coord_array(
                value
                    .get("coordinates")
                    .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?,
            )?;
            let points = coords.into_iter().map(Point).collect();
            Ok(FeatureGeometry::MultiPoint(MultiPoint::new(points)))
        }
        "MultiLineString" => {
            let lines_val = value
                .get("coordinates")
                .and_then(|c| c.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?;
            let mut linestrings = Vec::new();
            for lv in lines_val {
                linestrings.push(LineString::new(parse_coord_array(lv)?));
            }
            Ok(FeatureGeometry::MultiLineString(MultiLineString::new(
                linestrings,
            )))
        }
        "MultiPolygon" => {
            let polys_val = value
                .get("coordinates")
                .and_then(|c| c.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing coordinates".into()))?;
            let mut polygons = Vec::new();
            for pv in polys_val {
                let rings = pv
                    .as_array()
                    .ok_or_else(|| crate::Error::ParseError("invalid polygon".into()))?;
                polygons.push(parse_polygon_rings(rings)?);
            }
            Ok(FeatureGeometry::MultiPolygon(MultiPolygon::new(polygons)))
        }
        "GeometryCollection" => {
            let members = value
                .get("geometries")
                .and_then(|g| g.as_array())
                .ok_or_else(|| crate::Error::ParseError("missing geometries array".into()))?;
            let geometries = members
                .iter()
                .map(parse_geometry)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FeatureGeometry::GeometryCollection(geometries))
        }
        _ => Err(crate::Error::ParseError(format!(
            "unsupported geometry type: {geom_type}"
        ))),
    }
}

fn parse_polygon_rings(rings: &[Value]) -> Result<Polygon, crate::Error> {
    if rings.is_empty() {
        return Err(crate::Error::ParseError("empty polygon".into()));
    }
    let exterior_coords = parse_coord_array(&rings[0])?;
    let interiors: Vec<Ring> = rings[1..]
        .iter()
        .filter_map(|r| parse_coord_array(r).ok().map(Ring::new))
        .collect();

    Ok(Polygon::new(Ring::new(exterior_coords), interiors))
}

fn parse_coord_array(value: &Value) -> Result<Vec<Coord>, crate::Error> {
    let arr = value
        .as_array()
        .ok_or_else(|| crate::Error::ParseError("expected array".into()))?;
    arr.iter()
        .map(|c| {
            let coords = c
                .as_array()
                .ok_or_else(|| crate::Error::ParseError("expected coordinate array".into()))?;
            parse_coord(coords)
        })
        .collect()
}

fn parse_coord(arr: &[Value]) -> Result<Coord, crate::Error> {
    if arr.len() < 2 {
        return Err(crate::Error::ParseError(
            "coordinate needs at least 2 values".into(),
        ));
    }
    let x = arr[0]
        .as_f64()
        .ok_or_else(|| crate::Error::ParseError("invalid x coordinate".into()))?;
    let y = arr[1]
        .as_f64()
        .ok_or_else(|| crate::Error::ParseError("invalid y coordinate".into()))?;
    Ok(Coord::new(x, y))
}

fn is_geometry_type(t: &str) -> bool {
    matches!(
        t,
        "Point"
            | "LineString"
            | "Polygon"
            | "MultiPolygon"
            | "MultiPoint"
            | "MultiLineString"
            | "GeometryCollection"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_feature_collection() {
        let json = r#"{
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "geometry": {"type": "Point", "coordinates": [1.0, 2.0]},
                    "properties": {"name": "test"}
                },
                {
                    "type": "Feature",
                    "geometry": {"type": "Polygon", "coordinates": [[[0,0],[1,0],[1,1],[0,1],[0,0]]]},
                    "properties": {}
                }
            ]
        }"#;

        let fc = read_geojson(json).unwrap();
        assert_eq!(fc.features.len(), 2);

        match &fc.features[0].geometry {
            Some(FeatureGeometry::Point(p)) => {
                assert_eq!(p.0.x, 1.0);
                assert_eq!(p.0.y, 2.0);
            }
            _ => panic!("expected point"),
        }

        assert_eq!(
            fc.features[0].properties.get("name").unwrap().as_str(),
            Some("test")
        );
    }

    #[test]
    fn test_roundtrip() {
        let fc = FeatureCollection {
            features: vec![
                Feature {
                    geometry: Some(FeatureGeometry::Point(Point::new(10.0, 20.0))),
                    properties: {
                        let mut m = HashMap::new();
                        m.insert("id".into(), Value::from(42));
                        m
                    },
                },
                Feature {
                    geometry: Some(FeatureGeometry::LineString(LineString::new(vec![
                        Coord::new(0.0, 0.0),
                        Coord::new(1.0, 1.0),
                        Coord::new(2.0, 0.0),
                    ]))),
                    properties: HashMap::new(),
                },
            ],
        };

        let json = write_geojson(&fc);
        let parsed = read_geojson(&json).unwrap();
        assert_eq!(parsed.features.len(), 2);

        match &parsed.features[0].geometry {
            Some(FeatureGeometry::Point(p)) => {
                assert_eq!(p.0.x, 10.0);
                assert_eq!(p.0.y, 20.0);
            }
            _ => panic!("expected point"),
        }
    }

    #[test]
    fn test_roundtrip_multipoint() {
        let fc = FeatureCollection {
            features: vec![Feature {
                geometry: Some(FeatureGeometry::MultiPoint(MultiPoint::new(vec![
                    Point::new(1.0, 2.0),
                    Point::new(3.0, 4.0),
                ]))),
                properties: HashMap::new(),
            }],
        };

        let parsed = read_geojson(&write_geojson(&fc)).unwrap();
        match &parsed.features[0].geometry {
            Some(FeatureGeometry::MultiPoint(mp)) => {
                assert_eq!(mp.points().len(), 2);
                assert_eq!(mp.points()[1].0, Coord::new(3.0, 4.0));
            }
            other => panic!("expected multipoint, got {other:?}"),
        }
    }

    #[test]
    fn test_roundtrip_multilinestring() {
        let fc = FeatureCollection {
            features: vec![Feature {
                geometry: Some(FeatureGeometry::MultiLineString(MultiLineString::new(
                    vec![
                        LineString::new(vec![Coord::new(0.0, 0.0), Coord::new(1.0, 1.0)]),
                        LineString::new(vec![
                            Coord::new(5.0, 5.0),
                            Coord::new(6.0, 5.0),
                            Coord::new(7.0, 8.0),
                        ]),
                    ],
                ))),
                properties: HashMap::new(),
            }],
        };

        let parsed = read_geojson(&write_geojson(&fc)).unwrap();
        match &parsed.features[0].geometry {
            Some(FeatureGeometry::MultiLineString(mls)) => {
                assert_eq!(mls.linestrings().len(), 2);
                assert_eq!(mls.linestrings()[1].coords().len(), 3);
                assert_eq!(mls.linestrings()[1].coords()[2], Coord::new(7.0, 8.0));
            }
            other => panic!("expected multilinestring, got {other:?}"),
        }
    }

    #[test]
    fn test_roundtrip_nested_geometry_collection() {
        let inner = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::Point(Point::new(9.0, 9.0)),
            FeatureGeometry::Polygon(Polygon::from_coords(&[
                Coord::new(0.0, 0.0),
                Coord::new(2.0, 0.0),
                Coord::new(2.0, 2.0),
            ])),
        ]);
        let fc = FeatureCollection {
            features: vec![Feature {
                geometry: Some(FeatureGeometry::GeometryCollection(vec![
                    FeatureGeometry::LineString(LineString::new(vec![
                        Coord::new(0.0, 0.0),
                        Coord::new(1.0, 0.0),
                    ])),
                    inner,
                ])),
                properties: HashMap::new(),
            }],
        };

        let json = write_geojson(&fc);
        assert!(json.contains("\"geometries\""));
        let parsed = read_geojson(&json).unwrap();
        match &parsed.features[0].geometry {
            Some(FeatureGeometry::GeometryCollection(members)) => {
                assert_eq!(members.len(), 2);
                assert!(matches!(members[0], FeatureGeometry::LineString(_)));
                match &members[1] {
                    FeatureGeometry::GeometryCollection(nested) => {
                        assert_eq!(nested.len(), 2);
                        assert!(matches!(nested[0], FeatureGeometry::Point(_)));
                        assert!(matches!(nested[1], FeatureGeometry::Polygon(_)));
                    }
                    other => panic!("expected nested collection, got {other:?}"),
                }
            }
            other => panic!("expected geometry collection, got {other:?}"),
        }
    }

    #[test]
    fn test_read_bare_geometry_collection() {
        let json = r#"{
            "type": "GeometryCollection",
            "geometries": [
                {"type": "Point", "coordinates": [1, 2]},
                {"type": "MultiPoint", "coordinates": [[3, 4], [5, 6]]}
            ]
        }"#;
        let fc = read_geojson(json).unwrap();
        match &fc.features[0].geometry {
            Some(FeatureGeometry::GeometryCollection(members)) => assert_eq!(members.len(), 2),
            other => panic!("expected geometry collection, got {other:?}"),
        }
    }

    #[test]
    fn test_read_single_geometry() {
        let json = r#"{"type": "Point", "coordinates": [5.5, 6.6]}"#;
        let fc = read_geojson(json).unwrap();
        assert_eq!(fc.features.len(), 1);
    }

    #[test]
    fn test_polygon_with_hole() {
        let json = r#"{
            "type": "Feature",
            "geometry": {
                "type": "Polygon",
                "coordinates": [
                    [[0,0],[10,0],[10,10],[0,10],[0,0]],
                    [[2,2],[8,2],[8,8],[2,8],[2,2]]
                ]
            },
            "properties": {}
        }"#;

        let fc = read_geojson(json).unwrap();
        match &fc.features[0].geometry {
            Some(FeatureGeometry::Polygon(p)) => {
                assert_eq!(p.exterior().coords().len(), 5);
                assert_eq!(p.interiors().len(), 1);
                assert_eq!(p.interiors()[0].coords().len(), 5);
            }
            _ => panic!("expected polygon"),
        }
    }
}
