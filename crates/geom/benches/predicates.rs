//! Benchmarks for the exact predicates: run with `cargo bench -p geom`.
//!
//! Each predicate is measured on well-separated input, which the fast floating-point stage
//! settles, and on (nearly) degenerate input, which forces the exact stages.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use geom::{Point2, Point3, incircle, insphere, orient2d, orient3d};

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn bench_orient2d(criterion: &mut Criterion) {
    let (a, b) = (p2(0.1, 0.1), p2(12.3, 12.3));
    let mut group = criterion.benchmark_group("orient2d");
    for (name, query) in [("separated", p2(3.0, 7.0)), ("collinear", p2(7.7, 7.7))] {
        group.bench_function(name, |bench| {
            bench.iter(|| orient2d(black_box(a), black_box(b), black_box(query)));
        });
    }
    group.finish();
}

fn bench_orient3d(criterion: &mut Criterion) {
    let (a, b, d) = (p3(0.1, 0.2, 0.3), p3(1.7, 0.4, 2.9), p3(-0.3, 2.2, 1.3));
    // b + d - a lies in the plane; rounding leaves it a few ulps away at most.
    let coplanar = p3(b.x + d.x - a.x, b.y + d.y - a.y, b.z + d.z - a.z);
    let mut group = criterion.benchmark_group("orient3d");
    for (name, query) in [
        ("separated", p3(5.0, 5.0, -5.0)),
        ("nearly_coplanar", coplanar),
    ] {
        group.bench_function(name, |bench| {
            bench.iter(|| orient3d(black_box(a), black_box(b), black_box(d), black_box(query)));
        });
    }
    group.finish();
}

fn bench_incircle(criterion: &mut Criterion) {
    // The offset rounds the circle points, so the last query is nearly, not exactly, cocircular.
    let on = |x: f64, y: f64| p2(x + 0.1, y + 0.1);
    let (a, b, d) = (on(5.0, 0.0), on(3.0, 4.0), on(-4.0, 3.0));
    let mut group = criterion.benchmark_group("incircle");
    for (name, query) in [
        ("separated", on(0.0, 0.0)),
        ("nearly_cocircular", on(0.0, -5.0)),
    ] {
        group.bench_function(name, |bench| {
            bench.iter(|| incircle(black_box(a), black_box(b), black_box(d), black_box(query)));
        });
    }
    group.finish();
}

fn bench_insphere(criterion: &mut Criterion) {
    let on = |x: f64, y: f64, z: f64| p3(x + 0.1, y + 0.1, z + 0.1);
    let (a, b, d, e) = (
        on(5.0, 0.0, 0.0),
        on(0.0, 5.0, 0.0),
        on(-3.0, 4.0, 0.0),
        on(0.0, 0.0, -5.0),
    );
    let mut group = criterion.benchmark_group("insphere");
    for (name, query) in [
        ("separated", on(0.0, 0.0, 0.0)),
        ("nearly_cospherical", on(0.0, 3.0, 4.0)),
    ] {
        group.bench_function(name, |bench| {
            bench.iter(|| {
                insphere(
                    black_box(a),
                    black_box(b),
                    black_box(d),
                    black_box(e),
                    black_box(query),
                )
            });
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_orient2d,
    bench_orient3d,
    bench_incircle,
    bench_insphere
);
criterion_main!(benches);
