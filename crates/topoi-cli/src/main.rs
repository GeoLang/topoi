use clap::{Parser, Subcommand};
use topoi_core::{Coord, Polygon, Ring, contains};

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
}

fn parse_ring(values: &[f64]) -> Ring {
    let coords: Vec<Coord> = values
        .chunks_exact(2)
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
    }
}
