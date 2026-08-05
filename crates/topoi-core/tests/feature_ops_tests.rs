// Feature-collection level geoprocessing: properties survive, errors name the
// feature that caused them.

use serde_json::Value;
use std::collections::HashMap;
use topoi_core::geojson::{Feature, FeatureCollection, FeatureGeometry, read_geojson};
use topoi_core::*;

const EPS: f64 = 1e-9;

fn ring(coords: &[(f64, f64)]) -> Ring {
    let mut c: Vec<Coord> = coords.iter().map(|&(x, y)| Coord::new(x, y)).collect();
    if c.first() != c.last() {
        c.push(c[0]);
    }
    Ring::new(c)
}

fn rect(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon {
    Polygon::new(
        ring(&[
            (min_x, min_y),
            (max_x, min_y),
            (max_x, max_y),
            (min_x, max_y),
        ]),
        vec![],
    )
}

fn props(pairs: &[(&str, Value)]) -> HashMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

fn feature(geometry: FeatureGeometry, pairs: &[(&str, Value)]) -> Feature {
    Feature {
        geometry: Some(geometry),
        properties: props(pairs),
    }
}

fn fc(features: Vec<Feature>) -> FeatureCollection {
    FeatureCollection { features }
}

fn polygon_feature(p: Polygon, pairs: &[(&str, Value)]) -> Feature {
    feature(FeatureGeometry::Polygon(p), pairs)
}

fn point_feature(x: f64, y: f64, pairs: &[(&str, Value)]) -> Feature {
    feature(FeatureGeometry::Point(Point::new(x, y)), pairs)
}

fn line_feature(coords: &[(f64, f64)], pairs: &[(&str, Value)]) -> Feature {
    let c = coords.iter().map(|&(x, y)| Coord::new(x, y)).collect();
    feature(FeatureGeometry::LineString(LineString::new(c)), pairs)
}

fn area(geometry: &FeatureGeometry) -> f64 {
    match geometry {
        FeatureGeometry::Polygon(p) => p.area(),
        FeatureGeometry::MultiPolygon(mp) => mp.area(),
        other => panic!("expected an areal geometry, got {other:?}"),
    }
}

fn prop<'a>(f: &'a Feature, key: &str) -> &'a Value {
    f.properties
        .get(key)
        .unwrap_or_else(|| panic!("missing property {key} in {:?}", f.properties))
}

// --- buffer ---

#[test]
fn test_fc_buffer_keeps_properties() {
    let input = fc(vec![point_feature(
        0.0,
        0.0,
        &[("name", Value::from("well"))],
    )]);
    let out = fc_buffer(&input, 1.0, 64);
    assert_eq!(out.features.len(), 1);
    assert_eq!(prop(&out.features[0], "name"), &Value::from("well"));
    let disc = area(out.features[0].geometry.as_ref().unwrap());
    assert!((disc - std::f64::consts::PI).abs() < 0.01, "area {disc}");
}

#[test]
fn test_fc_buffer_supports_negative_distance() {
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 10.0, 10.0), &[("id", Value::from(1))]),
        point_feature(0.0, 0.0, &[("id", Value::from(2))]),
    ]);
    let out = fc_buffer(&input, -1.0, 16);
    // the square erodes to 8x8, the point has nothing to erode and is dropped
    assert_eq!(out.features.len(), 1);
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 64.0).abs() < EPS);
}

#[test]
fn test_fc_buffer_passes_geometryless_features_through() {
    let input = fc(vec![Feature {
        geometry: None,
        properties: props(&[("id", Value::from(7))]),
    }]);
    let out = fc_buffer(&input, 5.0, 8);
    assert_eq!(out.features.len(), 1);
    assert!(out.features[0].geometry.is_none());
    assert_eq!(prop(&out.features[0], "id"), &Value::from(7));
}

// --- dissolve ---

#[test]
fn test_fc_dissolve_all_into_one() {
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 2.0, 2.0), &[("k", Value::from("a"))]),
        polygon_feature(rect(1.0, 0.0, 3.0, 2.0), &[("k", Value::from("b"))]),
    ]);
    let out = fc_dissolve(&input, None).unwrap();
    assert_eq!(out.features.len(), 1);
    assert!(out.features[0].properties.is_empty());
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 6.0).abs() < EPS);
}

#[test]
fn test_fc_dissolve_by_property() {
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 2.0, 2.0), &[("zone", Value::from("a"))]),
        polygon_feature(rect(1.0, 0.0, 3.0, 2.0), &[("zone", Value::from("a"))]),
        polygon_feature(rect(10.0, 0.0, 12.0, 2.0), &[("zone", Value::from("b"))]),
    ]);
    let out = fc_dissolve(&input, Some("zone")).unwrap();
    assert_eq!(out.features.len(), 2);
    assert_eq!(prop(&out.features[0], "zone"), &Value::from("a"));
    assert_eq!(out.features[0].properties.len(), 1);
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 6.0).abs() < EPS);
    assert_eq!(prop(&out.features[1], "zone"), &Value::from("b"));
    assert!((area(out.features[1].geometry.as_ref().unwrap()) - 4.0).abs() < EPS);
}

#[test]
fn test_fc_dissolve_rejects_non_polygons() {
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 1.0, 1.0), &[]),
        point_feature(5.0, 5.0, &[]),
    ]);
    let err = fc_dissolve(&input, None).unwrap_err().to_string();
    assert!(err.contains("feature 1"), "{err}");
    assert!(err.contains("point"), "{err}");
}

// --- overlay ---

fn overlay_inputs() -> (FeatureCollection, FeatureCollection) {
    let a = fc(vec![
        polygon_feature(
            rect(0.0, 0.0, 4.0, 4.0),
            &[("id", Value::from(1)), ("zone", Value::from("west"))],
        ),
        polygon_feature(rect(20.0, 20.0, 24.0, 24.0), &[("id", Value::from(2))]),
    ]);
    let b = fc(vec![polygon_feature(
        rect(2.0, 0.0, 6.0, 4.0),
        &[
            ("zone", Value::from("east")),
            ("owner", Value::from("acme")),
        ],
    )]);
    (a, b)
}

#[test]
fn test_fc_overlay_intersection_merges_properties() {
    let (a, b) = overlay_inputs();
    let out = fc_overlay(&a, &b, OverlayKind::Intersection).unwrap();
    assert_eq!(out.features.len(), 1);
    let f = &out.features[0];
    assert!((area(f.geometry.as_ref().unwrap()) - 8.0).abs() < EPS);
    assert_eq!(prop(f, "id"), &Value::from(1));
    // zone clashes so it comes over prefixed, owner does not
    assert_eq!(prop(f, "zone"), &Value::from("west"));
    assert_eq!(prop(f, "b_zone"), &Value::from("east"));
    assert_eq!(prop(f, "owner"), &Value::from("acme"));
}

#[test]
fn test_fc_overlay_difference_keeps_untouched_features() {
    let (a, b) = overlay_inputs();
    let out = fc_overlay(&a, &b, OverlayKind::Difference).unwrap();
    assert_eq!(out.features.len(), 2);
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 8.0).abs() < EPS);
    assert_eq!(prop(&out.features[0], "zone"), &Value::from("west"));
    assert!(!out.features[0].properties.contains_key("owner"));
    // the far away feature is untouched
    assert!((area(out.features[1].geometry.as_ref().unwrap()) - 16.0).abs() < EPS);
}

#[test]
fn test_fc_overlay_difference_drops_fully_covered_features() {
    let a = fc(vec![polygon_feature(rect(1.0, 1.0, 2.0, 2.0), &[])]);
    let b = fc(vec![polygon_feature(rect(0.0, 0.0, 5.0, 5.0), &[])]);
    let out = fc_overlay(&a, &b, OverlayKind::Difference).unwrap();
    assert!(out.features.is_empty());
}

#[test]
fn test_fc_overlay_clip_keeps_only_a_properties() {
    let (a, b) = overlay_inputs();
    let out = fc_overlay(&a, &b, OverlayKind::Clip).unwrap();
    assert_eq!(out.features.len(), 1);
    let f = &out.features[0];
    assert!((area(f.geometry.as_ref().unwrap()) - 8.0).abs() < EPS);
    assert_eq!(prop(f, "zone"), &Value::from("west"));
    assert!(!f.properties.contains_key("owner"));
}

#[test]
fn test_fc_overlay_clip_handles_lines_and_points() {
    let a = fc(vec![
        line_feature(&[(-2.0, 2.0), (8.0, 2.0)], &[("id", Value::from(1))]),
        point_feature(1.0, 1.0, &[("id", Value::from(2))]),
        point_feature(50.0, 50.0, &[("id", Value::from(3))]),
    ]);
    let b = fc(vec![polygon_feature(rect(0.0, 0.0, 4.0, 4.0), &[])]);
    let out = fc_overlay(&a, &b, OverlayKind::Clip).unwrap();
    assert_eq!(out.features.len(), 2);
    match out.features[0].geometry.as_ref().unwrap() {
        FeatureGeometry::LineString(ls) => {
            assert_eq!(ls.coords().first().unwrap().x, 0.0);
            assert_eq!(ls.coords().last().unwrap().x, 4.0);
        }
        other => panic!("expected a linestring, got {other:?}"),
    }
    assert_eq!(prop(&out.features[1], "id"), &Value::from(2));
}

#[test]
fn test_fc_overlay_rejects_non_polygon_clip_layer() {
    let (a, _) = overlay_inputs();
    let b = fc(vec![line_feature(&[(0.0, 0.0), (1.0, 1.0)], &[])]);
    let err = fc_overlay(&a, &b, OverlayKind::Clip)
        .unwrap_err()
        .to_string();
    assert!(err.contains("feature 0"), "{err}");
    assert!(err.contains("linestring"), "{err}");
}

#[test]
fn test_fc_overlay_rejects_non_polygon_subject() {
    let a = fc(vec![line_feature(&[(0.0, 0.0), (1.0, 1.0)], &[])]);
    let b = fc(vec![polygon_feature(rect(0.0, 0.0, 4.0, 4.0), &[])]);
    assert!(fc_overlay(&a, &b, OverlayKind::Intersection).is_err());
}

// --- spatial join ---

#[test]
fn test_fc_spatial_join_point_in_polygon() {
    let target = fc(vec![
        point_feature(1.0, 1.0, &[("id", Value::from(1))]),
        point_feature(90.0, 90.0, &[("id", Value::from(2))]),
    ]);
    let source = fc(vec![polygon_feature(
        rect(0.0, 0.0, 4.0, 4.0),
        &[("zone", Value::from("west")), ("id", Value::from(99))],
    )]);
    let out = fc_spatial_join(&target, &source, JoinPredicate::Within, "src_").unwrap();
    assert_eq!(out.features.len(), 2);
    assert_eq!(prop(&out.features[0], "zone"), &Value::from("west"));
    // id clashes with the target's own id, so it arrives prefixed
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    assert_eq!(prop(&out.features[0], "src_id"), &Value::from(99));
    // unmatched target keeps exactly what it had
    assert_eq!(out.features[1].properties.len(), 1);
}

#[test]
fn test_fc_spatial_join_polygon_intersects() {
    let target = fc(vec![polygon_feature(
        rect(0.0, 0.0, 4.0, 4.0),
        &[("id", Value::from(1))],
    )]);
    let source = fc(vec![
        // envelope overlaps but the shapes do not
        polygon_feature(
            Polygon::new(ring(&[(5.0, 5.0), (9.0, 5.0), (9.0, 9.0)]), vec![]),
            &[("zone", Value::from("no"))],
        ),
        polygon_feature(rect(3.0, 3.0, 8.0, 8.0), &[("zone", Value::from("yes"))]),
    ]);
    let out = fc_spatial_join(&target, &source, JoinPredicate::Intersects, "s_").unwrap();
    assert_eq!(prop(&out.features[0], "zone"), &Value::from("yes"));
}

#[test]
fn test_fc_spatial_join_nearest_by_centroid() {
    let target = fc(vec![point_feature(0.0, 0.0, &[("id", Value::from(1))])]);
    let source = fc(vec![
        line_feature(&[(10.0, 0.0), (10.0, 4.0)], &[("name", Value::from("far"))]),
        line_feature(&[(2.0, 0.0), (2.0, 4.0)], &[("name", Value::from("near"))]),
    ]);
    let out = fc_spatial_join(&target, &source, JoinPredicate::Nearest, "s_").unwrap();
    assert_eq!(prop(&out.features[0], "name"), &Value::from("near"));
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
}

#[test]
fn test_fc_spatial_join_rejects_line_target_for_intersects() {
    let target = fc(vec![line_feature(&[(0.0, 0.0), (2.0, 2.0)], &[])]);
    let source = fc(vec![polygon_feature(rect(0.0, 0.0, 4.0, 4.0), &[])]);
    let err = fc_spatial_join(&target, &source, JoinPredicate::Intersects, "s_")
        .unwrap_err()
        .to_string();
    assert!(err.contains("feature 0"), "{err}");
    assert!(err.contains("linestring"), "{err}");
}

#[test]
fn test_fc_spatial_join_rejects_non_polygon_source() {
    let target = fc(vec![point_feature(0.0, 0.0, &[])]);
    let source = fc(vec![point_feature(1.0, 1.0, &[])]);
    let err = fc_spatial_join(&target, &source, JoinPredicate::Within, "s_")
        .unwrap_err()
        .to_string();
    assert!(err.contains("source feature 0"), "{err}");
}

// --- hull, centroid, simplify, clip ---

#[test]
fn test_fc_convex_hull_over_everything() {
    let input = fc(vec![
        point_feature(0.0, 0.0, &[("id", Value::from(1))]),
        point_feature(4.0, 0.0, &[]),
        line_feature(&[(4.0, 4.0), (0.0, 4.0)], &[]),
    ]);
    let out = fc_convex_hull(&input);
    assert_eq!(out.features.len(), 1);
    assert!(out.features[0].properties.is_empty());
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 16.0).abs() < EPS);
}

#[test]
fn test_fc_convex_hull_of_too_few_points() {
    let input = fc(vec![point_feature(1.0, 1.0, &[])]);
    assert!(fc_convex_hull(&input).features.is_empty());
}

#[test]
fn test_fc_centroid_keeps_properties() {
    let input = fc(vec![polygon_feature(
        rect(0.0, 0.0, 4.0, 4.0),
        &[("id", Value::from(1))],
    )]);
    let out = fc_centroid(&input);
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    match out.features[0].geometry.as_ref().unwrap() {
        FeatureGeometry::Point(p) => {
            assert!((p.0.x - 2.0).abs() < EPS && (p.0.y - 2.0).abs() < EPS);
        }
        other => panic!("expected a point, got {other:?}"),
    }
}

#[test]
fn test_fc_simplify_keeps_properties_and_drops_collapsed() {
    let input = fc(vec![
        line_feature(
            &[(0.0, 0.0), (1.0, 0.05), (2.0, 0.0), (3.0, 0.0)],
            &[("id", Value::from(1))],
        ),
        polygon_feature(rect(0.0, 0.0, 0.001, 0.001), &[("id", Value::from(2))]),
    ]);
    let out = fc_simplify(&input, 0.5);
    assert_eq!(out.features.len(), 1);
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    match out.features[0].geometry.as_ref().unwrap() {
        FeatureGeometry::LineString(ls) => assert_eq!(ls.coords().len(), 2),
        other => panic!("expected a linestring, got {other:?}"),
    }
}

#[test]
fn test_fc_clip_rect_by_type() {
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 10.0, 10.0), &[("id", Value::from(1))]),
        line_feature(&[(-5.0, 2.0), (20.0, 2.0)], &[("id", Value::from(2))]),
        point_feature(3.0, 3.0, &[("id", Value::from(3))]),
        point_feature(30.0, 3.0, &[("id", Value::from(4))]),
    ]);
    let out = fc_clip_rect(&input, 0.0, 0.0, 4.0, 4.0);
    assert_eq!(out.features.len(), 3);
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 16.0).abs() < EPS);
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    match out.features[1].geometry.as_ref().unwrap() {
        FeatureGeometry::LineString(ls) => {
            assert_eq!(ls.coords()[0], Coord::new(0.0, 2.0));
            assert_eq!(ls.coords()[1], Coord::new(4.0, 2.0));
        }
        other => panic!("expected a linestring, got {other:?}"),
    }
    assert_eq!(prop(&out.features[2], "id"), &Value::from(3));
}

// --- voronoi, grid ---

#[test]
fn test_fc_voronoi_carries_point_properties() {
    let input = fc(vec![
        point_feature(2.0, 5.0, &[("name", Value::from("a"))]),
        point_feature(8.0, 5.0, &[("name", Value::from("b"))]),
    ]);
    let out = fc_voronoi(&input, &Envelope::new(0.0, 0.0, 10.0, 10.0)).unwrap();
    assert_eq!(out.features.len(), 2);
    assert_eq!(prop(&out.features[0], "name"), &Value::from("a"));
    assert_eq!(prop(&out.features[1], "name"), &Value::from("b"));
    let total: f64 = out
        .features
        .iter()
        .map(|f| area(f.geometry.as_ref().unwrap()))
        .sum();
    assert!((total - 100.0).abs() < EPS);
}

#[test]
fn test_fc_voronoi_rejects_non_points() {
    let input = fc(vec![
        point_feature(1.0, 1.0, &[]),
        polygon_feature(rect(0.0, 0.0, 1.0, 1.0), &[]),
    ]);
    let err = fc_voronoi(&input, &Envelope::new(0.0, 0.0, 10.0, 10.0))
        .unwrap_err()
        .to_string();
    assert!(err.contains("feature 1"), "{err}");
}

#[test]
fn test_fc_grid_cell_ids() {
    let envelope = Envelope::new(0.0, 0.0, 10.0, 10.0);
    let squares = fc_grid(&envelope, 5.0, GridKind::Square).unwrap();
    assert_eq!(squares.features.len(), 4);
    assert_eq!(prop(&squares.features[0], "cell_id"), &Value::from(0));
    assert_eq!(prop(&squares.features[3], "cell_id"), &Value::from(3));

    let hexes = fc_grid(&envelope, 3.0, GridKind::Hex).unwrap();
    assert!(!hexes.features.is_empty());
    for (i, f) in hexes.features.iter().enumerate() {
        assert_eq!(prop(f, "cell_id"), &Value::from(i as i64));
    }
}

#[test]
fn test_fc_grid_rejects_bad_cell_size() {
    let envelope = Envelope::new(0.0, 0.0, 10.0, 10.0);
    assert!(fc_grid(&envelope, 0.0, GridKind::Square).is_err());
    assert!(fc_grid(&envelope, -2.0, GridKind::Hex).is_err());
}

// --- validate, make_valid ---

#[test]
fn test_fc_validate_reports_by_index() {
    let bowtie = Polygon::from_coords(&[
        Coord::new(0.0, 0.0),
        Coord::new(2.0, 2.0),
        Coord::new(2.0, 0.0),
        Coord::new(0.0, 2.0),
    ]);
    let input = fc(vec![
        polygon_feature(rect(0.0, 0.0, 1.0, 1.0), &[]),
        polygon_feature(bowtie, &[]),
    ]);
    let report = fc_validate(&input);
    assert!(!report.valid);
    assert_eq!(report.invalid.len(), 1);
    assert_eq!(report.invalid[0].feature, 1);
    assert_eq!(
        report.invalid[0].issues[0].kind,
        ValidityKind::SelfIntersection
    );

    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("self_intersection"), "{json}");
}

#[test]
fn test_fc_make_valid_keeps_properties() {
    let bowtie = Polygon::from_coords(&[
        Coord::new(0.0, 0.0),
        Coord::new(2.0, 2.0),
        Coord::new(2.0, 0.0),
        Coord::new(0.0, 2.0),
    ]);
    let input = fc(vec![polygon_feature(bowtie, &[("id", Value::from(1))])]);
    let out = fc_make_valid(&input).unwrap();
    assert_eq!(prop(&out.features[0], "id"), &Value::from(1));
    assert!((area(out.features[0].geometry.as_ref().unwrap()) - 2.0).abs() < EPS);
    assert!(fc_validate(&out).valid);
}

#[test]
fn test_fc_make_valid_names_the_failing_feature() {
    let input = fc(vec![
        point_feature(0.0, 0.0, &[]),
        line_feature(&[(1.0, 1.0), (1.0, 1.0)], &[]),
    ]);
    let err = fc_make_valid(&input).unwrap_err().to_string();
    assert!(err.contains("feature 1"), "{err}");
}

// --- geojson round trip, the shape the wasm bindings use ---

#[test]
fn test_ops_over_parsed_geojson() {
    let json = r#"{
        "type": "FeatureCollection",
        "features": [
            {"type": "Feature", "properties": {"zone": "a"},
             "geometry": {"type": "Polygon", "coordinates": [[[0,0],[4,0],[4,4],[0,4],[0,0]]]}},
            {"type": "Feature", "properties": {"zone": "a"},
             "geometry": {"type": "Polygon", "coordinates": [[[3,0],[8,0],[8,4],[3,4],[3,0]]]}}
        ]
    }"#;
    let parsed = read_geojson(json).unwrap();
    let dissolved = fc_dissolve(&parsed, Some("zone")).unwrap();
    assert_eq!(dissolved.features.len(), 1);
    assert!((area(dissolved.features[0].geometry.as_ref().unwrap()) - 32.0).abs() < EPS);
    assert_eq!(prop(&dissolved.features[0], "zone"), &Value::from("a"));
}
