use crate::error::Error;
use crate::geojson::FeatureGeometry;
use crate::geometry::{
    Coord, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon, Ring,
};

pub fn read_wkb(bytes: &[u8]) -> Result<FeatureGeometry, Error> {
    Wkb::new(bytes).geometry()
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

    struct Writer {
        out: Vec<u8>,
        little_endian: bool,
    }

    impl Writer {
        fn new(little_endian: bool) -> Self {
            Self {
                out: Vec::new(),
                little_endian,
            }
        }

        fn count(&mut self, n: usize) {
            let n = n as u32;
            let bytes = if self.little_endian {
                n.to_le_bytes()
            } else {
                n.to_be_bytes()
            };
            self.out.extend_from_slice(&bytes);
        }

        fn number(&mut self, v: f64) {
            let bytes = if self.little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            };
            self.out.extend_from_slice(&bytes);
        }

        fn coords(&mut self, coords: &[Coord]) {
            self.count(coords.len());
            for c in coords {
                self.number(c.x);
                self.number(c.y);
            }
        }

        fn header(&mut self, kind: usize) {
            self.out.push(u8::from(self.little_endian));
            self.count(kind);
        }

        fn geometry(&mut self, geometry: &FeatureGeometry) {
            match geometry {
                FeatureGeometry::Point(p) => {
                    self.header(1);
                    self.number(p.0.x);
                    self.number(p.0.y);
                }
                FeatureGeometry::LineString(l) => {
                    self.header(2);
                    self.coords(l.coords());
                }
                FeatureGeometry::Polygon(p) => {
                    self.header(3);
                    self.count(1 + p.interiors().len());
                    self.coords(p.exterior().coords());
                    for hole in p.interiors() {
                        self.coords(hole.coords());
                    }
                }
                FeatureGeometry::MultiPoint(mp) => {
                    self.header(4);
                    self.count(mp.points().len());
                    for p in mp.points() {
                        self.geometry(&FeatureGeometry::Point(*p));
                    }
                }
                FeatureGeometry::MultiLineString(mls) => {
                    self.header(5);
                    self.count(mls.linestrings().len());
                    for l in mls.linestrings() {
                        self.geometry(&FeatureGeometry::LineString(l.clone()));
                    }
                }
                FeatureGeometry::MultiPolygon(mp) => {
                    self.header(6);
                    self.count(mp.polygons().len());
                    for p in mp.polygons() {
                        self.geometry(&FeatureGeometry::Polygon(p.clone()));
                    }
                }
                FeatureGeometry::GeometryCollection(members) => {
                    self.header(7);
                    self.count(members.len());
                    for m in members {
                        self.geometry(m);
                    }
                }
            }
        }
    }

    fn wkb(geometry: &FeatureGeometry, little_endian: bool) -> Vec<u8> {
        let mut writer = Writer::new(little_endian);
        writer.geometry(geometry);
        writer.out
    }

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

    fn every_type() -> Vec<FeatureGeometry> {
        let holed = Polygon::new(
            Ring::new(square(0.0, 0.0, 10.0).exterior().coords().to_vec()),
            vec![Ring::new(
                square(2.0, 2.0, 3.0).exterior().coords().to_vec(),
            )],
        );
        vec![
            FeatureGeometry::Point(Point::new(1.25, -7.5)),
            FeatureGeometry::LineString(line()),
            FeatureGeometry::Polygon(holed.clone()),
            FeatureGeometry::MultiPoint(MultiPoint::new(vec![
                Point::new(0.0, 1.0),
                Point::new(2.0, 3.0),
            ])),
            FeatureGeometry::MultiLineString(MultiLineString::new(vec![
                line(),
                LineString::new(coords(&[(9.0, 9.0), (8.0, 7.0)])),
            ])),
            FeatureGeometry::MultiPolygon(MultiPolygon::new(vec![holed, square(20.0, 0.0, 1.0)])),
            FeatureGeometry::GeometryCollection(vec![
                FeatureGeometry::Point(Point::new(5.0, 5.0)),
                FeatureGeometry::GeometryCollection(vec![FeatureGeometry::LineString(line())]),
            ]),
        ]
    }

    fn gp_blob(envelope_type: u8, wkb: &[u8]) -> Vec<u8> {
        let envelope_size = [0, 32, 48, 48, 64][envelope_type as usize];
        let mut out = vec![b'G', b'P', 0, (envelope_type << 1) | 1];
        out.extend_from_slice(&4326i32.to_le_bytes());
        out.extend(std::iter::repeat_n(0xAB, envelope_size));
        out.extend_from_slice(wkb);
        out
    }

    #[test]
    fn every_type_round_trips_in_both_byte_orders() {
        for geometry in every_type() {
            for little_endian in [true, false] {
                let decoded = read_wkb(&wkb(&geometry, little_endian)).unwrap();
                assert_eq!(decoded, geometry, "little endian {little_endian}");
            }
        }
    }

    #[test]
    fn nested_geometries_carry_their_own_byte_order() {
        let point = FeatureGeometry::Point(Point::new(3.0, 4.0));
        let mut bytes = Writer::new(true);
        bytes.header(7);
        bytes.count(2);
        bytes.out.extend(wkb(&point, false));
        bytes.out.extend(wkb(&point, true));
        let decoded = read_wkb(&bytes.out).unwrap();
        assert_eq!(
            decoded,
            FeatureGeometry::GeometryCollection(vec![point.clone(), point])
        );
    }

    #[test]
    fn truncated_input_errors_at_every_length() {
        for geometry in every_type() {
            let bytes = wkb(&geometry, true);
            for len in 0..bytes.len() {
                assert!(read_wkb(&bytes[..len]).is_err(), "{geometry:?} at {len}");
            }
        }
    }

    #[test]
    fn unknown_type_and_wrong_member_error() {
        let mut bytes = Writer::new(true);
        bytes.header(8);
        assert!(read_wkb(&bytes.out).is_err());

        let mut bytes = Writer::new(true);
        bytes.header(4);
        bytes.count(1);
        bytes
            .out
            .extend(wkb(&FeatureGeometry::LineString(line()), true));
        assert!(read_wkb(&bytes.out).is_err());
    }

    #[test]
    fn huge_ring_count_errors() {
        let mut bytes = Writer::new(true);
        bytes.header(3);
        bytes.count(u32::MAX as usize);
        assert!(read_wkb(&bytes.out).is_err());
    }

    #[test]
    fn geopackage_header_skips_each_envelope_size() {
        let geometry = FeatureGeometry::Polygon(square(1.0, 2.0, 3.0));
        let wkb = wkb(&geometry, true);
        for envelope_type in 0..=4 {
            let decoded = read_geopackage_geometry(&gp_blob(envelope_type, &wkb)).unwrap();
            assert_eq!(decoded, geometry, "envelope type {envelope_type}");
        }
    }

    #[test]
    fn geopackage_header_errors() {
        let wkb = wkb(&FeatureGeometry::Point(Point::new(0.0, 0.0)), true);
        assert!(read_geopackage_geometry(&wkb).is_err());

        let mut bad_envelope = gp_blob(0, &wkb);
        bad_envelope[3] = 5 << 1;
        assert!(read_geopackage_geometry(&bad_envelope).is_err());

        let header_only = gp_blob(1, &[]);
        assert!(read_geopackage_geometry(&header_only).is_err());

        let truncated = gp_blob(0, &wkb[..wkb.len() - 1]);
        assert!(read_geopackage_geometry(&truncated).is_err());
    }

    #[test]
    fn geopackage_empty_flag_is_an_empty_collection() {
        let mut blob = gp_blob(0, &wkb(&FeatureGeometry::Point(Point::new(0.0, 0.0)), true));
        blob[3] |= 0b0001_0000;
        assert_eq!(
            read_geopackage_geometry(&blob).unwrap(),
            FeatureGeometry::GeometryCollection(Vec::new())
        );
    }
}
