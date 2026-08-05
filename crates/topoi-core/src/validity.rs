//! Validity checks and repair for GeoJSON geometries.

use crate::Error;
use crate::algorithms::segment_intersection;
use crate::geojson::FeatureGeometry;
use crate::geometry::{Coord, LineString, MultiLineString, Polygon, Ring};
use crate::overlay::union;
use crate::predicates::contains;
use serde::{Deserialize, Serialize};

/// What kind of thing is wrong with a geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidityKind {
    /// Fewer coordinates than the geometry type needs.
    TooFewPoints,
    /// A coordinate repeated immediately after itself.
    DuplicateVertices,
    /// A ring crossing itself.
    SelfIntersection,
    /// A hole reaching outside the ring it belongs to.
    HoleOutsideShell,
}

/// One validity problem, with a message meant to be shown to a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidityIssue {
    pub kind: ValidityKind,
    pub message: String,
}

impl ValidityIssue {
    fn new(kind: ValidityKind, message: String) -> Self {
        Self { kind, message }
    }
}

/// Every validity problem in a geometry, empty when it is valid.
///
/// The self-intersection check is quadratic in ring size.
pub fn validate(geometry: &FeatureGeometry) -> Vec<ValidityIssue> {
    let mut issues = Vec::new();
    check(geometry, &mut issues);
    issues
}

fn check(geometry: &FeatureGeometry, issues: &mut Vec<ValidityIssue>) {
    match geometry {
        FeatureGeometry::Point(_) => {}
        FeatureGeometry::MultiPoint(mp) => {
            if mp.points().is_empty() {
                issues.push(ValidityIssue::new(
                    ValidityKind::TooFewPoints,
                    "multipoint has no points".into(),
                ));
            }
        }
        FeatureGeometry::LineString(ls) => check_line(ls.coords(), issues),
        FeatureGeometry::MultiLineString(mls) => {
            for line in mls.linestrings() {
                check_line(line.coords(), issues);
            }
        }
        FeatureGeometry::Polygon(p) => check_polygon(p, issues),
        FeatureGeometry::MultiPolygon(mp) => {
            for p in mp.polygons() {
                check_polygon(p, issues);
            }
        }
        FeatureGeometry::GeometryCollection(members) => {
            for m in members {
                check(m, issues);
            }
        }
    }
}

fn check_line(coords: &[Coord], issues: &mut Vec<ValidityIssue>) {
    if coords.len() < 2 {
        issues.push(ValidityIssue::new(
            ValidityKind::TooFewPoints,
            format!(
                "linestring needs at least 2 points, has {len}",
                len = coords.len()
            ),
        ));
        return;
    }
    if let Some(i) = duplicate_at(coords) {
        issues.push(ValidityIssue::new(
            ValidityKind::DuplicateVertices,
            format!("linestring repeats the vertex at index {i}"),
        ));
    }
}

fn check_polygon(polygon: &Polygon, issues: &mut Vec<ValidityIssue>) {
    check_ring(polygon.exterior(), "exterior ring", issues);
    let shell = Polygon::new(polygon.exterior().clone(), vec![]);
    for (i, hole) in polygon.interiors().iter().enumerate() {
        check_ring(hole, &format!("hole {i}"), issues);
        if hole.coords().iter().any(|c| !contains(&shell, c)) {
            issues.push(ValidityIssue::new(
                ValidityKind::HoleOutsideShell,
                format!("hole {i} reaches outside the exterior ring"),
            ));
        }
    }
}

fn check_ring(ring: &Ring, label: &str, issues: &mut Vec<ValidityIssue>) {
    let coords = open(ring);
    if coords.len() < 3 {
        issues.push(ValidityIssue::new(
            ValidityKind::TooFewPoints,
            format!(
                "{label} needs at least 3 distinct points, has {len}",
                len = coords.len()
            ),
        ));
        return;
    }
    if let Some(i) = duplicate_at(coords) {
        issues.push(ValidityIssue::new(
            ValidityKind::DuplicateVertices,
            format!("{label} repeats the vertex at index {i}"),
        ));
    }
    if self_intersects(coords) {
        issues.push(ValidityIssue::new(
            ValidityKind::SelfIntersection,
            format!("{label} crosses itself"),
        ));
    }
}

/// A ring without its closing coordinate, so segment `i` is always
/// `coords[i]` to `coords[i + 1 mod n]`.
fn open(ring: &Ring) -> &[Coord] {
    let coords = ring.coords();
    match coords {
        [first, .., last] if first == last => &coords[..coords.len() - 1],
        _ => coords,
    }
}

fn duplicate_at(coords: &[Coord]) -> Option<usize> {
    coords.windows(2).position(|w| w[0] == w[1])
}

fn self_intersects(coords: &[Coord]) -> bool {
    let n = coords.len();
    for i in 0..n {
        let a = (coords[i], coords[(i + 1) % n]);
        for j in i + 2..n {
            // segments sharing an endpoint always "intersect" there
            if i == 0 && j == n - 1 {
                continue;
            }
            let b = (coords[j], coords[(j + 1) % n]);
            if segment_intersection(a.0, a.1, b.0, b.1).is_some() {
                return true;
            }
        }
    }
    false
}

/// Repair a geometry: polygons go through a self-overlay that resolves
/// self-intersections and winding, lines drop repeated vertices, points pass
/// through.
///
/// Fails when nothing of the geometry survives the repair.
pub fn make_valid(geometry: &FeatureGeometry) -> Result<FeatureGeometry, Error> {
    match geometry {
        FeatureGeometry::Point(_) | FeatureGeometry::MultiPoint(_) => Ok(geometry.clone()),
        FeatureGeometry::LineString(ls) => {
            let coords = dedup(ls.coords());
            if coords.len() < 2 {
                return Err(Error::InvalidGeometry(
                    "linestring collapses to a single point".into(),
                ));
            }
            Ok(FeatureGeometry::LineString(LineString::new(coords)))
        }
        FeatureGeometry::MultiLineString(mls) => {
            let parts: Vec<LineString> = mls
                .linestrings()
                .iter()
                .map(|l| dedup(l.coords()))
                .filter(|c| c.len() >= 2)
                .map(LineString::new)
                .collect();
            if parts.is_empty() {
                return Err(Error::InvalidGeometry(
                    "multilinestring collapses to points".into(),
                ));
            }
            Ok(FeatureGeometry::MultiLineString(MultiLineString::new(
                parts,
            )))
        }
        FeatureGeometry::Polygon(p) => valid_polygons(std::slice::from_ref(p)),
        FeatureGeometry::MultiPolygon(mp) => valid_polygons(mp.polygons()),
        FeatureGeometry::GeometryCollection(members) => members
            .iter()
            .map(make_valid)
            .collect::<Result<Vec<_>, _>>()
            .map(FeatureGeometry::GeometryCollection),
    }
}

/// Union against nothing, which resolves self-intersections and normalizes
/// winding within the subject.
fn valid_polygons(polygons: &[Polygon]) -> Result<FeatureGeometry, Error> {
    let fixed = union(polygons, &Vec::<Polygon>::new());
    match fixed.polygons() {
        [] => Err(Error::InvalidGeometry(
            "polygon has no area left after repair".into(),
        )),
        [single] => Ok(FeatureGeometry::Polygon(single.clone())),
        _ => Ok(FeatureGeometry::MultiPolygon(fixed)),
    }
}

fn dedup(coords: &[Coord]) -> Vec<Coord> {
    let mut out: Vec<Coord> = Vec::with_capacity(coords.len());
    for c in coords {
        if out.last() != Some(c) {
            out.push(*c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn bowtie() -> FeatureGeometry {
        FeatureGeometry::Polygon(Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(2.0, 2.0),
            Coord::new(2.0, 0.0),
            Coord::new(0.0, 2.0),
        ]))
    }

    fn ring(coords: &[(f64, f64)]) -> Ring {
        Ring::new(coords.iter().map(|(x, y)| Coord::new(*x, *y)).collect())
    }

    #[test]
    fn test_square_is_valid() {
        let square = FeatureGeometry::Polygon(Polygon::new(
            ring(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]),
            vec![],
        ));
        assert!(validate(&square).is_empty());
    }

    #[test]
    fn test_bowtie_is_self_intersecting() {
        let issues = validate(&bowtie());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].kind, ValidityKind::SelfIntersection);
        assert!(issues[0].message.contains("crosses itself"));
    }

    #[test]
    fn test_make_valid_splits_a_bowtie() {
        let fixed = make_valid(&bowtie()).unwrap();
        match &fixed {
            FeatureGeometry::MultiPolygon(mp) => {
                assert_eq!(mp.polygons().len(), 2);
                assert!((mp.area() - 2.0).abs() < 1e-9, "area {}", mp.area());
            }
            other => panic!("expected multipolygon, got {other:?}"),
        }
        assert!(validate(&fixed).is_empty());
    }

    #[test]
    fn test_hole_outside_shell_is_flagged() {
        let holed = FeatureGeometry::Polygon(Polygon::new(
            ring(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]),
            vec![ring(&[
                (10.0, 10.0),
                (12.0, 10.0),
                (12.0, 12.0),
                (10.0, 10.0),
            ])],
        ));
        let issues = validate(&holed);
        assert!(
            issues
                .iter()
                .any(|i| i.kind == ValidityKind::HoleOutsideShell),
            "{issues:?}"
        );
    }

    #[test]
    fn test_hole_inside_shell_is_valid() {
        let holed = FeatureGeometry::Polygon(Polygon::new(
            ring(&[
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
                (0.0, 0.0),
            ]),
            vec![ring(&[
                (2.0, 2.0),
                (4.0, 2.0),
                (4.0, 4.0),
                (2.0, 4.0),
                (2.0, 2.0),
            ])],
        ));
        assert!(validate(&holed).is_empty(), "{:?}", validate(&holed));
    }

    #[test]
    fn test_too_few_points() {
        let line = FeatureGeometry::LineString(LineString::new(vec![Coord::new(0.0, 0.0)]));
        assert_eq!(validate(&line)[0].kind, ValidityKind::TooFewPoints);

        let sliver = FeatureGeometry::Polygon(Polygon::from_coords(&[
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 1.0),
        ]));
        assert_eq!(validate(&sliver)[0].kind, ValidityKind::TooFewPoints);
    }

    #[test]
    fn test_duplicate_vertices() {
        let line = FeatureGeometry::LineString(LineString::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(1.0, 0.0),
            Coord::new(2.0, 0.0),
        ]));
        let issues = validate(&line);
        assert_eq!(issues[0].kind, ValidityKind::DuplicateVertices);
        assert!(issues[0].message.contains("index 1"), "{issues:?}");

        let fixed = make_valid(&line).unwrap();
        match fixed {
            FeatureGeometry::LineString(ls) => assert_eq!(ls.coords().len(), 3),
            other => panic!("expected linestring, got {other:?}"),
        }
    }

    #[test]
    fn test_make_valid_rejects_a_collapsed_line() {
        let line = FeatureGeometry::LineString(LineString::new(vec![
            Coord::new(1.0, 1.0),
            Coord::new(1.0, 1.0),
        ]));
        assert!(make_valid(&line).is_err());
    }

    #[test]
    fn test_points_pass_through() {
        let point = FeatureGeometry::Point(Point::new(3.0, 4.0));
        assert!(validate(&point).is_empty());
        match make_valid(&point).unwrap() {
            FeatureGeometry::Point(p) => assert_eq!(p.0, Coord::new(3.0, 4.0)),
            other => panic!("expected point, got {other:?}"),
        }
    }

    #[test]
    fn test_issue_serializes_with_a_message() {
        let issue = ValidityIssue::new(ValidityKind::SelfIntersection, "boom".into());
        let json = serde_json::to_string(&issue).unwrap();
        assert_eq!(json, r#"{"kind":"self_intersection","message":"boom"}"#);
    }

    #[test]
    fn test_collection_reports_member_issues() {
        let collection = FeatureGeometry::GeometryCollection(vec![
            FeatureGeometry::Point(Point::new(0.0, 0.0)),
            bowtie(),
        ]);
        assert_eq!(validate(&collection).len(), 1);
    }
}
