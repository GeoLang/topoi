use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
use topoi_core::*;

fn bench_convex_hull(c: &mut Criterion) {
    let points: Vec<Coord> = (0..1000)
        .map(|i| {
            let angle = (i as f64) * 0.00628;
            Coord::new(angle.cos() * 100.0, angle.sin() * 100.0)
        })
        .collect();

    c.bench_function("convex_hull_1000_points", |b| {
        b.iter(|| convex_hull(black_box(&points)))
    });
}

fn bench_point_in_polygon(c: &mut Criterion) {
    let poly = Polygon::new(
        Ring::new(vec![
            Coord::new(0.0, 0.0),
            Coord::new(100.0, 0.0),
            Coord::new(100.0, 100.0),
            Coord::new(0.0, 100.0),
            Coord::new(0.0, 0.0),
        ]),
        vec![],
    );
    let point = Coord::new(50.0, 50.0);

    c.bench_function("point_in_polygon", |b| {
        b.iter(|| contains(black_box(&poly), black_box(&point)))
    });
}

fn bench_simplify(c: &mut Criterion) {
    let coords: Vec<Coord> = (0..1000)
        .map(|i| {
            let x = i as f64 * 0.1;
            Coord::new(x, (x * 0.1).sin() * 10.0 + (i % 3) as f64 * 0.01)
        })
        .collect();

    c.bench_function("simplify_1000_points", |b| {
        b.iter(|| simplify(black_box(&coords), black_box(0.5)))
    });
}

criterion_group!(
    benches,
    bench_convex_hull,
    bench_point_in_polygon,
    bench_simplify
);
criterion_main!(benches);
