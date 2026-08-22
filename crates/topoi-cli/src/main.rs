use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use topoi_core::geojson::{Feature, FeatureCollection, FeatureGeometry, write_geojson};
use topoi_core::{BooleanOp, Coord, Polygon, Ring, boolean_op, contains};

#[derive(Parser)]
#[command(name = "topoi", version, about = "Computational geometry CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test if a point is inside a polygon (reads WKT-like coords from stdin)
    Contains {
        /// Point X coordinate
        #[arg(long)]
        px: f64,
        /// Point Y coordinate
        #[arg(long)]
        py: f64,
        /// Polygon vertices as x1,y1,x2,y2,... (must form a closed ring)
        #[arg(long, value_delimiter = ',')]
        ring: Vec<f64>,
    },
    /// Compute the area of a polygon
    Area {
        /// Polygon vertices as x1,y1,x2,y2,...
        #[arg(long, value_delimiter = ',')]
        ring: Vec<f64>,
    },
    /// Boolean overlay of two polygons, printed as GeoJSON
    Overlay {
        /// Which set operation to apply
        #[arg(long, value_enum)]
        op: Op,
        /// Subject vertices as x1,y1,x2,y2,...
        #[arg(long, value_delimiter = ',')]
        subject: Vec<f64>,
        /// Clip vertices as x1,y1,x2,y2,...
        #[arg(long, value_delimiter = ',')]
        clip: Vec<f64>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Op {
    Union,
    Intersection,
    Difference,
    Xor,
}

impl From<Op> for BooleanOp {
    fn from(op: Op) -> Self {
        match op {
            Op::Union => BooleanOp::Union,
            Op::Intersection => BooleanOp::Intersection,
            Op::Difference => BooleanOp::Difference,
            Op::Xor => BooleanOp::Xor,
        }
    }
}

fn parse_ring(values: &[f64]) -> Ring {
    let coords: Vec<Coord> = values
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| Coord::new(c[0], c[1]))
        .collect();
    Ring::new(coords)
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Contains { px, py, ring } => {
            let polygon = Polygon::new(parse_ring(&ring), vec![]);
            let result = contains(&polygon, &Coord::new(px, py));
            println!("{result}");
        }
        Commands::Area { ring } => {
            let polygon = Polygon::new(parse_ring(&ring), vec![]);
            println!("{:.6}", polygon.area());
        }
        Commands::Overlay { op, subject, clip } => {
            let subject = Polygon::new(parse_ring(&subject), vec![]);
            let clip = Polygon::new(parse_ring(&clip), vec![]);
            let result = boolean_op(&subject, &clip, op.into());
            let fc = FeatureCollection {
                features: vec![Feature {
                    geometry: Some(FeatureGeometry::MultiPolygon(result)),
                    properties: HashMap::new(),
                }],
            };
            println!("{}", write_geojson(&fc));
        }
    }
}
