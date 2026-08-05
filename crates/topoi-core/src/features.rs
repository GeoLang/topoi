//! Feature-collection level operations, the unit a geoprocessing toolbox works
//! in.
//!
//! Properties are carried across unless an op says otherwise. A feature with no
//! geometry passes through untouched where the op maps geometries one to one
//! (buffer, centroid, simplify, make_valid, spatial join) and is dropped where
//! the op consumes geometry to decide what comes out (dissolve, overlay, clip,
//! voronoi).

use crate::Error;
use crate::algorithms::{convex_hull, simplify};
use crate::buffer::buffer_geometry;
use crate::centroid::centroid;
use crate::clipping::{clip_linestring_rect, clip_polygon_rect, clip_to_boundary};
use crate::envelope::Envelope;
use crate::geojson::{Feature, FeatureCollection, FeatureGeometry};
use crate::geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring, closed_ring,
};
use crate::grid::{hex_grid, square_grid};
use crate::overlay::{difference, intersection, union};
use crate::predicates::contains;
use crate::rtree::RTree;
use crate::validity::{ValidityIssue, make_valid, validate};
use crate::voronoi::voronoi_polygons;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

type Properties = HashMap<String, Value>;

/// Buffer every feature, keeping its properties.
///
/// A negative distance shrinks polygons and empties points and lines, and a
/// feature whose buffer comes out empty is dropped.
pub fn fc_buffer(fc: &FeatureCollection, distance: f64, segments: usize) -> FeatureCollection {
    let features = fc
        .features
        .iter()
        .filter_map(|f| match &f.geometry {
            None => Some(f.clone()),
            Some(g) => shape(buffer_geometry(g, distance, segments))
                .map(|geom| feature(geom, f.properties.clone())),
        })
        .collect();
    FeatureCollection { features }
}

/// Union the polygon features together, one output feature per distinct value of
/// the `by` property, or all into one when `by` is `None`.
///
/// Output properties are the group key alone when `by` is set and empty
/// otherwise, since nothing else survives a merge.
pub fn fc_dissolve(fc: &FeatureCollection, by: Option<&str>) -> Result<FeatureCollection, Error> {
    let mut groups: Vec<(Value, Vec<Polygon>)> = Vec::new();
    let mut slots: HashMap<String, usize> = HashMap::new();
    for (i, f) in fc.features.iter().enumerate() {
        let Some(g) = &f.geometry else { continue };
        let polygons = polygons_of(g).ok_or_else(|| unsupported(i, g, "dissolve"))?;
        let key = by
            .map(|k| f.properties.get(k).cloned().unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        let slot = *slots.entry(key.to_string()).or_insert_with(|| {
            groups.push((key.clone(), Vec::new()));
            groups.len() - 1
        });
        groups[slot].1.extend(polygons.iter().cloned());
    }

    let features = groups
        .into_iter()
        .filter_map(|(key, polygons)| {
            let geom = shape(self_union(&polygons))?;
            let mut properties = Properties::new();
            if let Some(k) = by {
                properties.insert(k.to_string(), key);
            }
            Some(feature(geom, properties))
        })
        .collect();
    Ok(FeatureCollection { features })
}

/// How `fc_overlay` combines two collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayKind {
    /// One feature per intersecting pair, carrying both sides' properties.
    Intersection,
    /// Each A feature minus the B polygons that touch it.
    Difference,
    /// Each A feature cut down to the B polygons.
    Clip,
}

/// Overlay collection `a` with the polygons of collection `b`.
///
/// `b` must hold polygons. `a` may be anything for `Clip`, and must hold
/// polygons for the other two ops.
pub fn fc_overlay(
    a: &FeatureCollection,
    b: &FeatureCollection,
    op: OverlayKind,
) -> Result<FeatureCollection, Error> {
    let clips = clip_layer(b, "overlay")?;

    if op == OverlayKind::Clip {
        let all: Vec<Polygon> = clips
            .iter()
            .flat_map(|c| c.polygons.iter().cloned())
            .collect();
        let boundary = self_union(&all);
        let features = a
            .features
            .iter()
            .filter_map(|f| {
                let g = f.geometry.as_ref()?;
                clip_to_boundary(g, &boundary).map(|geom| feature(geom, f.properties.clone()))
            })
            .collect();
        return Ok(FeatureCollection { features });
    }

    let envelopes: Vec<Envelope> = clips.iter().map(|c| c.envelope).collect();
    let rtree = RTree::new(&envelopes);
    let mut features = Vec::new();
    for (i, f) in a.features.iter().enumerate() {
        let Some(g) = &f.geometry else { continue };
        let subject = polygons_of(g).ok_or_else(|| unsupported(i, g, "overlay"))?;
        let Some(env) = geometry_envelope(g) else {
            continue;
        };
        let mut hits = rtree.search(&env);
        hits.sort_unstable();

        match op {
            OverlayKind::Intersection => {
                for hit in hits {
                    let clip = &clips[hit];
                    if let Some(geom) = shape(intersection(subject, &clip.polygons)) {
                        let mut properties = f.properties.clone();
                        merge_properties(&mut properties, &f.properties, clip.properties, "b_");
                        features.push(feature(geom, properties));
                    }
                }
            }
            OverlayKind::Difference => {
                if hits.is_empty() {
                    features.push(f.clone());
                    continue;
                }
                let clip: Vec<Polygon> = hits
                    .iter()
                    .flat_map(|&h| clips[h].polygons.iter().cloned())
                    .collect();
                if let Some(geom) = shape(difference(subject, &clip)) {
                    features.push(feature(geom, f.properties.clone()));
                }
            }
            OverlayKind::Clip => unreachable!("handled above"),
        }
    }
    Ok(FeatureCollection { features })
}

/// How `fc_spatial_join` matches a target feature to a source feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinPredicate {
    /// Polygons that overlap, or points inside a polygon.
    Intersects,
    /// Points inside a polygon.
    Within,
    /// Closest source feature by centroid distance, always a match.
    Nearest,
}

/// Copy the properties of the first matching source feature onto each target
/// feature, prefixing source keys that clash with `prefix`.
///
/// `Intersects` and `Within` need polygon source features, and a target that is
/// a point or a polygon (`Within` points only). `Nearest` works for any types.
/// Unmatched target features pass through unchanged.
pub fn fc_spatial_join(
    target: &FeatureCollection,
    source: &FeatureCollection,
    predicate: JoinPredicate,
    prefix: &str,
) -> Result<FeatureCollection, Error> {
    if predicate != JoinPredicate::Nearest {
        for (j, sf) in source.features.iter().enumerate() {
            if let Some(g) = &sf.geometry
                && polygons_of(g).is_none()
            {
                return Err(Error::InvalidGeometry(format!(
                    "source feature {j} is a {}, spatial join needs polygons",
                    kind_of(g)
                )));
            }
        }
    }

    // sites index the source features that can match at all, so the r-tree and
    // the source collection do not have to line up
    let mut sites: Vec<(usize, Envelope)> = Vec::new();
    for (j, sf) in source.features.iter().enumerate() {
        let Some(g) = &sf.geometry else { continue };
        let env = match predicate {
            JoinPredicate::Nearest => centroid(g).map(|c| Envelope::new(c.x, c.y, c.x, c.y)),
            _ => geometry_envelope(g),
        };
        if let Some(env) = env {
            sites.push((j, env));
        }
    }
    let rtree = RTree::new(&sites.iter().map(|(_, e)| *e).collect::<Vec<_>>());

    let mut features = Vec::with_capacity(target.features.len());
    for (i, f) in target.features.iter().enumerate() {
        let Some(g) = &f.geometry else {
            features.push(f.clone());
            continue;
        };
        let matched = match predicate {
            JoinPredicate::Nearest => centroid(g)
                .and_then(|c| rtree.nearest(&c, 1).first().map(|&(site, _)| sites[site].0)),
            _ => first_match(i, g, predicate, &sites, &rtree, source)?,
        };
        let mut properties = f.properties.clone();
        if let Some(j) = matched {
            merge_properties(
                &mut properties,
                &f.properties,
                &source.features[j].properties,
                prefix,
            );
        }
        features.push(Feature {
            geometry: Some(g.clone()),
            properties,
        });
    }
    Ok(FeatureCollection { features })
}

fn first_match(
    index: usize,
    geometry: &FeatureGeometry,
    predicate: JoinPredicate,
    sites: &[(usize, Envelope)],
    rtree: &RTree,
    source: &FeatureCollection,
) -> Result<Option<usize>, Error> {
    let matcher = matcher(index, geometry, predicate)?;
    let Some(env) = geometry_envelope(geometry) else {
        return Ok(None);
    };
    let mut hits = rtree.search(&env);
    hits.sort_unstable();
    for hit in hits {
        let j = sites[hit].0;
        let polygons = source.features[j]
            .geometry
            .as_ref()
            .and_then(polygons_of)
            .unwrap_or_default();
        if matcher.matches(polygons) {
            return Ok(Some(j));
        }
    }
    Ok(None)
}

/// How one target geometry is tested against candidate source polygons.
enum Matcher<'a> {
    /// Every point inside for `Within`, any point inside otherwise.
    Points {
        points: Vec<Coord>,
        all: bool,
    },
    Overlap(&'a [Polygon]),
}

impl Matcher<'_> {
    fn matches(&self, polygons: &[Polygon]) -> bool {
        match self {
            Matcher::Points { points, all } => {
                let inside = |c: &Coord| polygons.iter().any(|p| contains(p, c));
                if *all {
                    points.iter().all(inside)
                } else {
                    points.iter().any(inside)
                }
            }
            Matcher::Overlap(subject) => !intersection(*subject, polygons).polygons().is_empty(),
        }
    }
}

fn matcher(
    index: usize,
    geometry: &FeatureGeometry,
    predicate: JoinPredicate,
) -> Result<Matcher<'_>, Error> {
    // a point matches the polygon holding it, whichever way round it is asked
    let points = match geometry {
        FeatureGeometry::Point(p) => Some(vec![p.0]),
        FeatureGeometry::MultiPoint(mp) => Some(mp.points().iter().map(|p| p.0).collect()),
        _ => None,
    };
    if let Some(points) = points {
        return Ok(Matcher::Points {
            points,
            all: predicate == JoinPredicate::Within,
        });
    }
    match (predicate, polygons_of(geometry)) {
        (JoinPredicate::Intersects, Some(subject)) => Ok(Matcher::Overlap(subject)),
        _ => Err(unsupported_join(index, geometry, predicate)),
    }
}

/// One convex hull over every coordinate in the collection, with no properties.
///
/// A collection holding fewer than three distinct positions has no hull, and
/// comes back empty.
pub fn fc_convex_hull(fc: &FeatureCollection) -> FeatureCollection {
    let mut coords = Vec::new();
    for f in &fc.features {
        if let Some(g) = &f.geometry {
            collect_coords(g, &mut coords);
        }
    }
    let hull = convex_hull(&coords);
    let features = if hull.exterior().coords().len() < 4 {
        Vec::new()
    } else {
        vec![feature(FeatureGeometry::Polygon(hull), Properties::new())]
    };
    FeatureCollection { features }
}

/// Replace every geometry with its centroid, keeping properties.
pub fn fc_centroid(fc: &FeatureCollection) -> FeatureCollection {
    let features =
        fc.features
            .iter()
            .filter_map(|f| match &f.geometry {
                None => Some(f.clone()),
                Some(g) => centroid(g)
                    .map(|c| feature(FeatureGeometry::Point(Point(c)), f.properties.clone())),
            })
            .collect();
    FeatureCollection { features }
}

/// Douglas-Peucker every linestring and polygon ring, keeping properties.
///
/// A ring that collapses below four coordinates is dropped, and so is a feature
/// whose geometry collapses entirely.
pub fn fc_simplify(fc: &FeatureCollection, tolerance: f64) -> FeatureCollection {
    let features = fc
        .features
        .iter()
        .filter_map(|f| match &f.geometry {
            None => Some(f.clone()),
            Some(g) => {
                simplify_geometry(g, tolerance).map(|geom| feature(geom, f.properties.clone()))
            }
        })
        .collect();
    FeatureCollection { features }
}

fn simplify_geometry(geometry: &FeatureGeometry, tolerance: f64) -> Option<FeatureGeometry> {
    match geometry {
        FeatureGeometry::Point(_) | FeatureGeometry::MultiPoint(_) => Some(geometry.clone()),
        FeatureGeometry::LineString(ls) => {
            let coords = simplify(ls.coords(), tolerance);
            (coords.len() >= 2).then(|| FeatureGeometry::LineString(LineString::new(coords)))
        }
        FeatureGeometry::MultiLineString(mls) => {
            let parts: Vec<LineString> = mls
                .linestrings()
                .iter()
                .map(|l| simplify(l.coords(), tolerance))
                .filter(|c| c.len() >= 2)
                .map(LineString::new)
                .collect();
            (!parts.is_empty())
                .then(|| FeatureGeometry::MultiLineString(MultiLineString::new(parts)))
        }
        FeatureGeometry::Polygon(p) => simplify_polygon(p, tolerance).map(FeatureGeometry::Polygon),
        FeatureGeometry::MultiPolygon(mp) => {
            let polygons: Vec<Polygon> = mp
                .polygons()
                .iter()
                .filter_map(|p| simplify_polygon(p, tolerance))
                .collect();
            (!polygons.is_empty())
                .then(|| FeatureGeometry::MultiPolygon(MultiPolygon::new(polygons)))
        }
        FeatureGeometry::GeometryCollection(members) => {
            let kept: Vec<FeatureGeometry> = members
                .iter()
                .filter_map(|m| simplify_geometry(m, tolerance))
                .collect();
            (!kept.is_empty()).then_some(FeatureGeometry::GeometryCollection(kept))
        }
    }
}

fn simplify_polygon(polygon: &Polygon, tolerance: f64) -> Option<Polygon> {
    let exterior = simplify_ring(polygon.exterior(), tolerance)?;
    let interiors = polygon
        .interiors()
        .iter()
        .filter_map(|r| simplify_ring(r, tolerance))
        .collect();
    Some(Polygon::new(exterior, interiors))
}

fn simplify_ring(ring: &Ring, tolerance: f64) -> Option<Ring> {
    let coords = simplify(ring.coords(), tolerance);
    (coords.len() >= 4).then(|| Ring::new(coords))
}

/// Clip every geometry to an axis-aligned rectangle, keeping properties.
///
/// Polygon rings are clipped one at a time, so a hole crossing the rectangle
/// edge comes back cut to the same rectangle.
pub fn fc_clip_rect(
    fc: &FeatureCollection,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
) -> FeatureCollection {
    let rect = Envelope::new(min_x, min_y, max_x, max_y);
    let features = fc
        .features
        .iter()
        .filter_map(|f| {
            let g = f.geometry.as_ref()?;
            clip_geometry(g, &rect).map(|geom| feature(geom, f.properties.clone()))
        })
        .collect();
    FeatureCollection { features }
}

fn clip_geometry(geometry: &FeatureGeometry, rect: &Envelope) -> Option<FeatureGeometry> {
    let clip_line = |coords: &[Coord]| {
        clip_linestring_rect(coords, rect.min_x, rect.min_y, rect.max_x, rect.max_y)
    };
    match geometry {
        FeatureGeometry::Point(p) => rect.contains_coord(&p.0).then(|| geometry.clone()),
        FeatureGeometry::MultiPoint(mp) => {
            let kept: Vec<Point> = mp
                .points()
                .iter()
                .filter(|p| rect.contains_coord(&p.0))
                .copied()
                .collect();
            (!kept.is_empty()).then(|| FeatureGeometry::MultiPoint(MultiPoint::new(kept)))
        }
        FeatureGeometry::LineString(ls) => lines(clip_line(ls.coords())),
        FeatureGeometry::MultiLineString(mls) => lines(
            mls.linestrings()
                .iter()
                .flat_map(|l| clip_line(l.coords()))
                .collect(),
        ),
        FeatureGeometry::Polygon(p) => clip_polygon_to_rect(p, rect).map(FeatureGeometry::Polygon),
        FeatureGeometry::MultiPolygon(mp) => {
            let polygons: Vec<Polygon> = mp
                .polygons()
                .iter()
                .filter_map(|p| clip_polygon_to_rect(p, rect))
                .collect();
            (!polygons.is_empty())
                .then(|| FeatureGeometry::MultiPolygon(MultiPolygon::new(polygons)))
        }
        FeatureGeometry::GeometryCollection(members) => {
            let kept: Vec<FeatureGeometry> = members
                .iter()
                .filter_map(|m| clip_geometry(m, rect))
                .collect();
            (!kept.is_empty()).then_some(FeatureGeometry::GeometryCollection(kept))
        }
    }
}

fn clip_polygon_to_rect(polygon: &Polygon, rect: &Envelope) -> Option<Polygon> {
    let clip_ring = |ring: &Ring| {
        let coords = clip_polygon_rect(
            ring.coords(),
            rect.min_x,
            rect.min_y,
            rect.max_x,
            rect.max_y,
        );
        (coords.len() >= 3).then(|| closed_ring(coords))
    };
    let exterior = clip_ring(polygon.exterior())?;
    let interiors = polygon.interiors().iter().filter_map(clip_ring).collect();
    Some(Polygon::new(exterior, interiors))
}

fn lines(parts: Vec<Vec<Coord>>) -> Option<FeatureGeometry> {
    match parts.len() {
        0 => None,
        1 => Some(FeatureGeometry::LineString(LineString::new(
            parts.into_iter().next().expect("one part"),
        ))),
        _ => Some(FeatureGeometry::MultiLineString(MultiLineString::new(
            parts.into_iter().map(LineString::new).collect(),
        ))),
    }
}

/// Voronoi cells over the point features, each cell carrying its point's
/// properties. A multipoint feature contributes one cell per point.
pub fn fc_voronoi(
    points_fc: &FeatureCollection,
    envelope: &Envelope,
) -> Result<FeatureCollection, Error> {
    let mut sites: Vec<(Coord, &Properties)> = Vec::new();
    for (i, f) in points_fc.features.iter().enumerate() {
        let Some(g) = &f.geometry else { continue };
        match g {
            FeatureGeometry::Point(p) => sites.push((p.0, &f.properties)),
            FeatureGeometry::MultiPoint(mp) => {
                sites.extend(mp.points().iter().map(|p| (p.0, &f.properties)));
            }
            other => return Err(unsupported(i, other, "voronoi")),
        }
    }

    let coords: Vec<Coord> = sites.iter().map(|(c, _)| *c).collect();
    let features = voronoi_polygons(&coords, envelope)
        .into_iter()
        .zip(&sites)
        .filter(|(cell, _)| cell.exterior().is_closed())
        .map(|(cell, (_, properties))| {
            feature(FeatureGeometry::Polygon(cell), (*properties).clone())
        })
        .collect();
    Ok(FeatureCollection { features })
}

/// Which grid `fc_grid` builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKind {
    Square,
    Hex,
}

/// A grid covering `envelope`, one feature per cell with a `cell_id` property.
pub fn fc_grid(
    envelope: &Envelope,
    cell_size: f64,
    kind: GridKind,
) -> Result<FeatureCollection, Error> {
    let cells = match kind {
        GridKind::Square => square_grid(envelope, cell_size)?,
        GridKind::Hex => hex_grid(envelope, cell_size)?,
    };
    let features = cells
        .into_iter()
        .enumerate()
        .map(|(i, cell)| {
            let mut properties = Properties::new();
            properties.insert("cell_id".into(), Value::from(i as i64));
            feature(FeatureGeometry::Polygon(cell), properties)
        })
        .collect();
    Ok(FeatureCollection { features })
}

/// Validity issues of one feature, by its index in the collection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureIssues {
    pub feature: usize,
    pub issues: Vec<ValidityIssue>,
}

/// Which features of a collection are invalid, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidityReport {
    pub valid: bool,
    pub invalid: Vec<FeatureIssues>,
}

/// Check every feature, reporting only the ones with problems.
pub fn fc_validate(fc: &FeatureCollection) -> ValidityReport {
    let invalid: Vec<FeatureIssues> = fc
        .features
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let issues = validate(f.geometry.as_ref()?);
            (!issues.is_empty()).then_some(FeatureIssues { feature: i, issues })
        })
        .collect();
    ValidityReport {
        valid: invalid.is_empty(),
        invalid,
    }
}

/// Repair every feature, keeping properties.
pub fn fc_make_valid(fc: &FeatureCollection) -> Result<FeatureCollection, Error> {
    let mut features = Vec::with_capacity(fc.features.len());
    for (i, f) in fc.features.iter().enumerate() {
        match &f.geometry {
            None => features.push(f.clone()),
            Some(g) => {
                let geom = make_valid(g).map_err(|e| at(i, e))?;
                features.push(feature(geom, f.properties.clone()));
            }
        }
    }
    Ok(FeatureCollection { features })
}

/// One clip operand: its polygons, their extent, and the properties an
/// intersection carries over.
struct ClipFeature<'a> {
    polygons: Vec<Polygon>,
    envelope: Envelope,
    properties: &'a Properties,
}

fn clip_layer<'a>(fc: &'a FeatureCollection, op: &str) -> Result<Vec<ClipFeature<'a>>, Error> {
    let mut layer = Vec::new();
    for (i, f) in fc.features.iter().enumerate() {
        let Some(g) = &f.geometry else { continue };
        let polygons = polygons_of(g).ok_or_else(|| unsupported(i, g, op))?;
        let Some(envelope) = geometry_envelope(g) else {
            continue;
        };
        layer.push(ClipFeature {
            polygons: polygons.to_vec(),
            envelope,
            properties: &f.properties,
        });
    }
    Ok(layer)
}

fn feature(geometry: FeatureGeometry, properties: Properties) -> Feature {
    Feature {
        geometry: Some(geometry),
        properties,
    }
}

/// Copy `extra` into `properties`, prefixing keys that are already in `own`.
fn merge_properties(
    properties: &mut Properties,
    own: &Properties,
    extra: &Properties,
    prefix: &str,
) {
    for (k, v) in extra {
        let key = if own.contains_key(k) {
            format!("{prefix}{k}")
        } else {
            k.clone()
        };
        properties.insert(key, v.clone());
    }
}

/// A multipolygon as a feature geometry, or `None` when it is empty.
fn shape(mp: MultiPolygon) -> Option<FeatureGeometry> {
    match mp.polygons() {
        [] => None,
        [single] => Some(FeatureGeometry::Polygon(single.clone())),
        _ => Some(FeatureGeometry::MultiPolygon(mp)),
    }
}

/// Union a polygon set against nothing, which resolves overlaps within it.
fn self_union(polygons: &[Polygon]) -> MultiPolygon {
    if polygons.is_empty() {
        return MultiPolygon::new(Vec::new());
    }
    union(polygons, &Vec::<Polygon>::new())
}

fn polygons_of(geometry: &FeatureGeometry) -> Option<&[Polygon]> {
    match geometry {
        FeatureGeometry::Polygon(p) => Some(std::slice::from_ref(p)),
        FeatureGeometry::MultiPolygon(mp) => Some(mp.polygons()),
        _ => None,
    }
}

fn geometry_envelope(geometry: &FeatureGeometry) -> Option<Envelope> {
    let mut coords = Vec::new();
    collect_coords(geometry, &mut coords);
    Envelope::from_coords(&coords)
}

fn collect_coords(geometry: &FeatureGeometry, out: &mut Vec<Coord>) {
    match geometry {
        FeatureGeometry::Point(p) => out.push(p.0),
        FeatureGeometry::MultiPoint(mp) => out.extend(mp.points().iter().map(|p| p.0)),
        FeatureGeometry::LineString(ls) => out.extend_from_slice(ls.coords()),
        FeatureGeometry::MultiLineString(mls) => {
            for l in mls.linestrings() {
                out.extend_from_slice(l.coords());
            }
        }
        FeatureGeometry::Polygon(p) => collect_polygon_coords(p, out),
        FeatureGeometry::MultiPolygon(mp) => {
            for p in mp.polygons() {
                collect_polygon_coords(p, out);
            }
        }
        FeatureGeometry::GeometryCollection(members) => {
            for m in members {
                collect_coords(m, out);
            }
        }
    }
}

fn collect_polygon_coords(polygon: &Polygon, out: &mut Vec<Coord>) {
    out.extend_from_slice(polygon.exterior().coords());
    for hole in polygon.interiors() {
        out.extend_from_slice(hole.coords());
    }
}

fn kind_of(geometry: &FeatureGeometry) -> &'static str {
    match geometry {
        FeatureGeometry::Point(_) => "point",
        FeatureGeometry::LineString(_) => "linestring",
        FeatureGeometry::Polygon(_) => "polygon",
        FeatureGeometry::MultiPoint(_) => "multipoint",
        FeatureGeometry::MultiLineString(_) => "multilinestring",
        FeatureGeometry::MultiPolygon(_) => "multipolygon",
        FeatureGeometry::GeometryCollection(_) => "geometrycollection",
    }
}

fn unsupported(index: usize, geometry: &FeatureGeometry, op: &str) -> Error {
    Error::InvalidGeometry(format!(
        "feature {index} is a {}, {op} needs polygons",
        kind_of(geometry)
    ))
}

fn unsupported_join(index: usize, geometry: &FeatureGeometry, predicate: JoinPredicate) -> Error {
    Error::InvalidGeometry(format!(
        "feature {index} is a {}, which spatial join cannot match with {predicate:?}",
        kind_of(geometry)
    ))
}

/// Point an error at the feature it came from.
fn at(index: usize, error: Error) -> Error {
    match error {
        Error::InvalidGeometry(m) => Error::InvalidGeometry(format!("feature {index}: {m}")),
        Error::TopologyError(m) => Error::TopologyError(format!("feature {index}: {m}")),
        Error::ParseError(m) => Error::ParseError(format!("feature {index}: {m}")),
    }
}
