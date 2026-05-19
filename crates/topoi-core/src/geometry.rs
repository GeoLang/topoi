/// A 2D coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coord {
    pub x: f64,
    pub y: f64,
}

impl Coord {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Self) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// A point geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point(pub Coord);

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self(Coord::new(x, y))
    }
}

/// A linear ring (closed sequence of coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    coords: Vec<Coord>,
}

impl Ring {
    pub fn new(coords: Vec<Coord>) -> Self {
        Self { coords }
    }

    pub fn coords(&self) -> &[Coord] {
        &self.coords
    }

    pub fn is_closed(&self) -> bool {
        self.coords.len() >= 4 && self.coords.first() == self.coords.last()
    }

    /// Signed area (positive = CCW, negative = CW).
    pub fn signed_area(&self) -> f64 {
        let n = self.coords.len();
        if n < 3 {
            return 0.0;
        }
        let mut area = 0.0;
        for i in 0..n - 1 {
            let j = (i + 1) % n;
            area += self.coords[i].x * self.coords[j].y;
            area -= self.coords[j].x * self.coords[i].y;
        }
        area / 2.0
    }

    pub fn area(&self) -> f64 {
        self.signed_area().abs()
    }

    /// Check if the ring has counter-clockwise winding.
    pub fn is_ccw(&self) -> bool {
        self.signed_area() > 0.0
    }
}

/// A line string (open sequence of coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct LineString {
    coords: Vec<Coord>,
}

impl LineString {
    pub fn new(coords: Vec<Coord>) -> Self {
        Self { coords }
    }

    pub fn coords(&self) -> &[Coord] {
        &self.coords
    }

    pub fn length(&self) -> f64 {
        self.coords
            .windows(2)
            .map(|w| w[0].distance_to(&w[1]))
            .sum()
    }
}

/// A polygon with an exterior ring and optional interior rings (holes).
#[derive(Debug, Clone, PartialEq)]
pub struct Polygon {
    exterior: Ring,
    interiors: Vec<Ring>,
}

impl Polygon {
    pub fn new(exterior: Ring, interiors: Vec<Ring>) -> Self {
        Self {
            exterior,
            interiors,
        }
    }

    pub fn exterior(&self) -> &Ring {
        &self.exterior
    }

    pub fn interiors(&self) -> &[Ring] {
        &self.interiors
    }

    pub fn area(&self) -> f64 {
        let ext_area = self.exterior.area();
        let holes_area: f64 = self.interiors.iter().map(|r| r.area()).sum();
        ext_area - holes_area
    }

    pub fn centroid(&self) -> Coord {
        let coords = self.exterior.coords();
        let n = coords.len();
        if n == 0 {
            return Coord::new(0.0, 0.0);
        }
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut signed_area = 0.0;
        for i in 0..n - 1 {
            let x0 = coords[i].x;
            let y0 = coords[i].y;
            let x1 = coords[i + 1].x;
            let y1 = coords[i + 1].y;
            let a = x0 * y1 - x1 * y0;
            signed_area += a;
            cx += (x0 + x1) * a;
            cy += (y0 + y1) * a;
        }
        signed_area *= 0.5;
        cx /= 6.0 * signed_area;
        cy /= 6.0 * signed_area;
        Coord::new(cx, cy)
    }
}

/// A collection of polygons.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiPolygon {
    polygons: Vec<Polygon>,
}

impl MultiPolygon {
    pub fn new(polygons: Vec<Polygon>) -> Self {
        Self { polygons }
    }

    pub fn polygons(&self) -> &[Polygon] {
        &self.polygons
    }

    pub fn area(&self) -> f64 {
        self.polygons.iter().map(|p| p.area()).sum()
    }
}
