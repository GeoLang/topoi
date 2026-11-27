use crate::error::Error;
use crate::features::geometry_envelope;
use crate::geojson::FeatureGeometry;
use crate::geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
};

pub fn read_wkb(bytes: &[u8]) -> Result<FeatureGeometry, Error> {
    Wkb::new(bytes).geometry()
}

pub fn write_wkb(geometry: &FeatureGeometry) -> Vec<u8> {
    let mut out = Vec::new();
    push_geometry(geometry, &mut out);
    out
}

pub fn write_geopackage_geometry(geometry: &FeatureGeometry, srs_id: i32) -> Vec<u8> {
    let envelope = geometry_envelope(geometry);
    // flag bit 0 marks a little endian header, bits 1-3 value 1 an xy envelope, bit 4 an empty geometry
    let flags = match envelope {
        Some(_) => 0b0000_0011,
        None => 0b0001_0001,
    };
    let mut out = vec![b'G', b'P', 0, flags];
    out.extend_from_slice(&srs_id.to_le_bytes());
    if let Some(envelope) = envelope {
        // geopackage envelopes run min x, max x, min y, max y
        for bound in [
            envelope.min_x,
            envelope.max_x,
            envelope.min_y,
            envelope.max_y,
        ] {
            out.extend_from_slice(&bound.to_le_bytes());
        }
    }
    push_geometry(geometry, &mut out);
    out
}

fn push_header(kind: u32, out: &mut Vec<u8>) {
    out.push(1);
    out.extend_from_slice(&kind.to_le_bytes());
}

fn push_count(count: usize, out: &mut Vec<u8>) {
    let count = u32::try_from(count).expect("wkb counts fit in 32 bits");
    out.extend_from_slice(&count.to_le_bytes());
}

fn push_coord(coord: Coord, out: &mut Vec<u8>) {
    out.extend_from_slice(&coord.x.to_le_bytes());
    out.extend_from_slice(&coord.y.to_le_bytes());
}

fn push_coords(coords: &[Coord], out: &mut Vec<u8>) {
    push_count(coords.len(), out);
    for coord in coords {
        push_coord(*coord, out);
    }
}

fn push_polygon(polygon: &Polygon, out: &mut Vec<u8>) {
    push_header(3, out);
    push_count(1 + polygon.interiors().len(), out);
    push_coords(polygon.exterior().coords(), out);
    for hole in polygon.interiors() {
        push_coords(hole.coords(), out);
    }
}

fn push_geometry(geometry: &FeatureGeometry, out: &mut Vec<u8>) {
    match geometry {
        FeatureGeometry::Point(point) => {
            push_header(1, out);
            push_coord(point.0, out);
        }
        FeatureGeometry::LineString(line) => {
            push_header(2, out);
            push_coords(line.coords(), out);
        }
        FeatureGeometry::Polygon(polygon) => push_polygon(polygon, out),
        FeatureGeometry::MultiPoint(points) => {
            push_header(4, out);
            push_count(points.points().len(), out);
            for point in points.points() {
                push_header(1, out);
                push_coord(point.0, out);
            }
        }
        FeatureGeometry::MultiLineString(lines) => {
            push_header(5, out);
            push_count(lines.linestrings().len(), out);
            for line in lines.linestrings() {
                push_header(2, out);
                push_coords(line.coords(), out);
            }
        }
        FeatureGeometry::MultiPolygon(polygons) => {
            push_header(6, out);
            push_count(polygons.polygons().len(), out);
            for polygon in polygons.polygons() {
                push_polygon(polygon, out);
            }
        }
        FeatureGeometry::GeometryCollection(members) => {
            push_header(7, out);
            push_count(members.len(), out);
            for member in members {
                push_geometry(member, out);
            }
        }
    }
}

pub fn read_geopackage_geometry(bytes: &[u8]) -> Result<FeatureGeometry, Error> {
    // bytes 0-1 magic "GP", byte 2 version, byte 3 flags, bytes 4-7 srs id
    if bytes.len() < 8 || bytes[0] != b'G' || bytes[1] != b'P' {
        return Err(Error::ParseError(
            "geopackage geometry: missing GP header".into(),
        ));
    }
    let flags = bytes[3];
    // flag bit 4 marks an empty geometry, whose wkb is a placeholder
    if flags & 0b0001_0000 != 0 {
        return Ok(FeatureGeometry::GeometryCollection(Vec::new()));
    }
    let envelope_size = match (flags >> 1) & 0b111 {
        0 => 0,
        1 => 32,
        2 | 3 => 48,
        4 => 64,
        envelope_type => {
            return Err(Error::ParseError(format!(
                "geopackage geometry: invalid envelope type {envelope_type}"
            )));
        }
    };
    let wkb_offset = 8 + envelope_size;
    if wkb_offset >= bytes.len() {
        return Err(Error::ParseError(format!(
            "geopackage geometry: {} bytes end before the wkb at byte {wkb_offset}",
            bytes.len()
        )));
    }
    read_wkb(&bytes[wkb_offset..])
}

// every nested geometry carries its own byte order byte, so the position and
// the endianness travel together
struct Wkb<'a> {
    data: &'a [u8],
    pos: usize,
    little_endian: bool,
}

impl<'a> Wkb<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            little_endian: true,
        }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let slice = self
            .pos
            .checked_add(n)
            .and_then(|end| self.data.get(self.pos..end))
            .ok_or_else(|| {
                Error::ParseError(format!(
                    "wkb: {} bytes end before byte {} + {n}",
                    self.data.len(),
                    self.pos
                ))
            })?;
        self.pos += n;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn count(&mut self) -> Result<u32, Error> {
        let bytes: [u8; 4] = self.take(4)?.try_into().expect("take returns 4 bytes");
        Ok(if self.little_endian {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        })
    }

    fn number(&mut self) -> Result<f64, Error> {
        let bytes: [u8; 8] = self.take(8)?.try_into().expect("take returns 8 bytes");
        Ok(if self.little_endian {
            f64::from_le_bytes(bytes)
        } else {
            f64::from_be_bytes(bytes)
        })
    }

    fn coord(&mut self) -> Result<Coord, Error> {
        Ok(Coord::new(self.number()?, self.number()?))
    }

    fn coords(&mut self) -> Result<Vec<Coord>, Error> {
        let n = self.count()?;
        (0..n).map(|_| self.coord()).collect()
    }

    fn polygon(&mut self) -> Result<Polygon, Error> {
        let num_rings = self.count()?;
        let mut rings = (0..num_rings)
            .map(|_| self.coords().map(Ring::new))
            .collect::<Result<Vec<_>, _>>()?;
        if rings.is_empty() {
            return Err(Error::ParseError("wkb: polygon with no rings".into()));
        }
        let exterior = rings.remove(0);
        Ok(Polygon::new(exterior, rings))
    }

    fn members(&mut self) -> Result<Vec<FeatureGeometry>, Error> {
        let n = self.count()?;
        (0..n).map(|_| self.geometry()).collect()
    }

    fn geometry(&mut self) -> Result<FeatureGeometry, Error> {
        self.little_endian = self.byte()? == 1;
        // xy only: the type code keeps its low byte and z or m ordinates are not read
        match self.count()? & 0xFF {
            1 => Ok(FeatureGeometry::Point(Point(self.coord()?))),
            2 => Ok(FeatureGeometry::LineString(LineString::new(self.coords()?))),
            3 => Ok(FeatureGeometry::Polygon(self.polygon()?)),
            4 => {
                let points = self
                    .members()?
                    .into_iter()
                    .map(|g| match g {
                        FeatureGeometry::Point(p) => Ok(p),
                        _ => Err(member_error("multipoint", "point")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(FeatureGeometry::MultiPoint(MultiPoint::new(points)))
            }
            5 => {
                let lines = self
                    .members()?
                    .into_iter()
                    .map(|g| match g {
                        FeatureGeometry::LineString(l) => Ok(l),
                        _ => Err(member_error("multilinestring", "linestring")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(FeatureGeometry::MultiLineString(MultiLineString::new(
                    lines,
                )))
            }
            6 => {
                let polygons = self
                    .members()?
                    .into_iter()
                    .map(|g| match g {
                        FeatureGeometry::Polygon(p) => Ok(p),
                        _ => Err(member_error("multipolygon", "polygon")),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(FeatureGeometry::MultiPolygon(MultiPolygon::new(polygons)))
            }
            7 => Ok(FeatureGeometry::GeometryCollection(self.members()?)),
            kind => Err(Error::ParseError(format!(
                "wkb: unknown geometry type {kind}"
            ))),
        }
    }
}

fn member_error(collection: &str, member: &str) -> Error {
    Error::ParseError(format!("wkb: {collection} member is not a {member}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coords(pairs: &[(f64, f64)]) -> Vec<Coord> {
        pairs.iter().map(|&(x, y)| Coord::new(x, y)).collect()
    }

    fn square(x: f64, y: f64, side: f64) -> Polygon {
        Polygon::from_coords(&coords(&[
            (x, y),
            (x + side, y),
            (x + side, y + side),
            (x, y + side),
            (x, y),
        ]))
    }

    fn line() -> LineString {
        LineString::new(coords(&[(0.0, 0.0), (1.5, -2.0), (3.0, 4.25)]))
    }

    fn holed() -> Polygon {
        Polygon::new(
            Ring::new(square(0.0, 0.0, 10.0).exterior().coords().to_vec()),
            vec![
                Ring::new(square(2.0, 2.0, 3.0).exterior().coords().to_vec()),
                Ring::new(square(6.0, 6.0, 1.0).exterior().coords().to_vec()),
            ],
        )
    }

    // each geometry beside the wkb type code the ogc spec gives it
    fn every_type() -> Vec<(FeatureGeometry, u32)> {
        vec![
            (FeatureGeometry::Point(Point::new(1.25, -7.5)), 1),
            (FeatureGeometry::LineString(line()), 2),
            (FeatureGeometry::Polygon(holed()), 3),
            (
                FeatureGeometry::MultiPoint(MultiPoint::new(vec![
                    Point::new(0.0, 1.0),
                    Point::new(2.0, 3.0),
                ])),
                4,
            ),
            (
                FeatureGeometry::MultiLineString(MultiLineString::new(vec![
                    line(),
                    LineString::new(coords(&[(9.0, 9.0), (8.0, 7.0)])),
                ])),
                5,
            ),
            (
                FeatureGeometry::MultiPolygon(MultiPolygon::new(vec![
                    holed(),
                    square(20.0, 0.0, 1.0),
                ])),
                6,
            ),
            (
                FeatureGeometry::GeometryCollection(vec![
                    FeatureGeometry::Point(Point::new(5.0, 5.0)),
                    FeatureGeometry::GeometryCollection(vec![FeatureGeometry::LineString(line())]),
                ]),
                7,
            ),
        ]
    }

    fn big_endian_line(line: &LineString) -> Vec<u8> {
        let mut out = vec![0];
        out.extend_from_slice(&2u32.to_be_bytes());
        out.extend_from_slice(&(line.coords().len() as u32).to_be_bytes());
        for c in line.coords() {
            out.extend_from_slice(&c.x.to_be_bytes());
            out.extend_from_slice(&c.y.to_be_bytes());
        }
        out
    }

    fn little_endian_header(kind: u32, count: Option<u32>) -> Vec<u8> {
        let mut out = vec![1];
        out.extend_from_slice(&kind.to_le_bytes());
        if let Some(count) = count {
            out.extend_from_slice(&count.to_le_bytes());
        }
        out
    }

    #[test]
    fn every_type_round_trips_through_write_wkb() {
        for (geometry, _) in every_type() {
            assert_eq!(read_wkb(&write_wkb(&geometry)).unwrap(), geometry);
        }
    }

    #[test]
    fn write_wkb_writes_little_endian_ogc_type_codes() {
        for (geometry, kind) in every_type() {
            let bytes = write_wkb(&geometry);
            assert_eq!(bytes[0], 1, "{geometry:?}");
            assert_eq!(bytes[1..5], kind.to_le_bytes(), "{geometry:?}");
        }
        let mut point = little_endian_header(1, None);
        point.extend_from_slice(&1.25f64.to_le_bytes());
        point.extend_from_slice(&(-7.5f64).to_le_bytes());
        assert_eq!(
            write_wkb(&FeatureGeometry::Point(Point::new(1.25, -7.5))),
            point
        );
    }

    #[test]
    fn big_endian_members_decode_inside_a_little_endian_collection() {
        let point = FeatureGeometry::Point(Point::new(3.0, 4.0));
        let mut bytes = little_endian_header(7, Some(2));
        bytes.extend(big_endian_line(&line()));
        bytes.extend(write_wkb(&point));
        assert_eq!(
            read_wkb(&bytes).unwrap(),
            FeatureGeometry::GeometryCollection(vec![FeatureGeometry::LineString(line()), point])
        );
    }

    #[test]
    fn truncated_input_errors_at_every_length() {
        for (geometry, _) in every_type() {
            let bytes = write_wkb(&geometry);
            for len in 0..bytes.len() {
                assert!(read_wkb(&bytes[..len]).is_err(), "{geometry:?} at {len}");
            }
        }
    }

    #[test]
    fn unknown_type_and_wrong_member_error() {
        assert!(read_wkb(&little_endian_header(8, None)).is_err());

        let mut bytes = little_endian_header(4, Some(1));
        bytes.extend(write_wkb(&FeatureGeometry::LineString(line())));
        assert!(read_wkb(&bytes).is_err());
    }

    #[test]
    fn huge_ring_count_errors() {
        assert!(read_wkb(&little_endian_header(3, Some(u32::MAX))).is_err());
    }

    #[test]
    fn every_type_round_trips_through_a_geopackage_blob() {
        for (geometry, _) in every_type() {
            let blob = write_geopackage_geometry(&geometry, 32633);
            assert_eq!(blob[..4], [b'G', b'P', 0, 0b0000_0011], "{geometry:?}");
            assert_eq!(blob[4..8], 32633i32.to_le_bytes(), "{geometry:?}");
            assert_eq!(read_geopackage_geometry(&blob).unwrap(), geometry);
        }
    }

    #[test]
    fn geopackage_blob_carries_the_xy_envelope() {
        let blob = write_geopackage_geometry(
            &FeatureGeometry::MultiPolygon(MultiPolygon::new(vec![
                holed(),
                square(20.0, -3.0, 1.0),
            ])),
            4326,
        );
        let bounds: Vec<f64> = blob[8..40]
            .as_chunks::<8>()
            .0
            .iter()
            .map(|b| f64::from_le_bytes(*b))
            .collect();
        assert_eq!(bounds, [0.0, 21.0, -3.0, 10.0]);
    }

    #[test]
    fn empty_geometry_sets_the_empty_flag_without_an_envelope() {
        let empty = FeatureGeometry::GeometryCollection(Vec::new());
        let blob = write_geopackage_geometry(&empty, 4326);
        assert_eq!(blob[3], 0b0001_0001);
        assert_eq!(blob[8..], write_wkb(&empty));
        assert_eq!(read_geopackage_geometry(&blob).unwrap(), empty);
    }

    #[test]
    fn geopackage_header_skips_each_envelope_size() {
        let geometry = FeatureGeometry::Polygon(square(1.0, 2.0, 3.0));
        let header = write_geopackage_geometry(&geometry, 4326)[..8].to_vec();
        for (envelope_type, envelope_size) in [(0u8, 0), (1, 32), (2, 48), (3, 48), (4, 64)] {
            let mut blob = header.clone();
            blob[3] = (envelope_type << 1) | 1;
            blob.extend(std::iter::repeat_n(0xAB, envelope_size));
            blob.extend(write_wkb(&geometry));
            let decoded = read_geopackage_geometry(&blob).unwrap();
            assert_eq!(decoded, geometry, "envelope type {envelope_type}");
        }
    }

    #[test]
    fn geopackage_header_errors() {
        let point = FeatureGeometry::Point(Point::new(0.0, 0.0));
        assert!(read_geopackage_geometry(&write_wkb(&point)).is_err());

        let blob = write_geopackage_geometry(&point, 4326);
        let mut bad_envelope = blob.clone();
        bad_envelope[3] = (5 << 1) | 1;
        assert!(read_geopackage_geometry(&bad_envelope).is_err());

        let header_only = &blob[..40];
        assert!(read_geopackage_geometry(header_only).is_err());

        let truncated = &blob[..blob.len() - 1];
        assert!(read_geopackage_geometry(truncated).is_err());
    }
}
