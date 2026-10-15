//! Properties of profiles (GEO-01, ENG-02): areas against exact rational arithmetic and closed
//! forms, invariance under translation and loop direction, and no panic on arbitrary input.

use core::f64::consts::{PI, TAU};

use geom::{ArcDirection, Edge, MODEL_EXTENT, Point2, Profile};
use num_rational::BigRational;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn exact(v: f64) -> BigRational {
    BigRational::from_float(v).unwrap_or_else(|| unreachable!("coordinates are finite"))
}

fn polygon(points: &[Point2]) -> Vec<Edge> {
    (0..points.len())
        .map(|i| Edge::Line {
            start: points[i],
            end: points[(i + 1) % points.len()],
        })
        .collect()
}

fn reversed(edges: &[Edge]) -> Vec<Edge> {
    edges.iter().rev().map(Edge::reversed).collect()
}

fn translated(edges: &[Edge], dx: f64, dy: f64) -> Vec<Edge> {
    let shift = |p: Point2| Point2::new(p.x + dx, p.y + dy);
    edges
        .iter()
        .map(|edge| match *edge {
            Edge::Line { start, end } => Edge::Line {
                start: shift(start),
                end: shift(end),
            },
            Edge::Arc {
                start,
                end,
                centre,
                direction,
            } => Edge::Arc {
                start: shift(start),
                end: shift(end),
                centre: shift(centre),
                direction,
            },
        })
        .collect()
}

/// Twice the signed area of a polygon, exactly.
fn exact_double_area(points: &[Point2]) -> BigRational {
    (0..points.len())
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % points.len()]);
            exact(a.x) * exact(b.y) - exact(b.x) * exact(a.y)
        })
        .sum()
}

fn relative_error(actual: f64, expected: f64) -> f64 {
    (actual - expected).abs() / expected.abs()
}

/// Vertices of a convex polygon: points of a circle at increasing angles.
fn convex_polygon() -> impl Strategy<Value = Vec<Point2>> {
    (
        prop::collection::vec(0.05..1.0f64, 3..16),
        0.01..100.0f64,
        -1e3..1e3f64,
        -1e3..1e3f64,
    )
        .prop_map(|(steps, radius, cx, cy)| {
            let total: f64 = steps.iter().sum();
            let mut angle = 0.0;
            steps
                .iter()
                .map(|step| {
                    angle += step / total * TAU;
                    Point2::new(
                        libm::cos(angle).mul_add(radius, cx),
                        libm::sin(angle).mul_add(radius, cy),
                    )
                })
                .collect()
        })
}

/// A rectangle with a round hole fully inside it, at least 1 mm from its sides.
fn plate_with_hole() -> impl Strategy<Value = (f64, f64, f64, f64, f64)> {
    (0.1..50.0f64, 0.1..50.0f64).prop_flat_map(|(width, height)| {
        let max_radius = (width.min(height) / 2.0 - 1e-3).max(1e-3);
        (Just(width), Just(height), 1e-3..=max_radius).prop_flat_map(|(width, height, radius)| {
            let margin = radius + 1e-3;
            (
                Just(width),
                Just(height),
                Just(radius),
                margin..=(width - margin).max(margin),
                margin..=(height - margin).max(margin),
            )
        })
    })
}

fn plate(width: f64, height: f64) -> Vec<Edge> {
    polygon(&[
        Point2::new(0.0, 0.0),
        Point2::new(width, 0.0),
        Point2::new(width, height),
        Point2::new(0.0, height),
    ])
}

fn circle(cx: f64, cy: f64, radius: f64) -> Vec<Edge> {
    let start = Point2::new(cx + radius, cy);
    vec![Edge::Arc {
        start,
        end: start,
        centre: Point2::new(cx, cy),
        direction: ArcDirection::CounterClockwise,
    }]
}

/// Anything, including non-finite and out-of-range values.
fn any_coordinate() -> impl Strategy<Value = f64> {
    prop_oneof![
        (-4..4i32).prop_map(f64::from),
        -2.0 * MODEL_EXTENT..2.0 * MODEL_EXTENT,
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(1e-40),
    ]
}

fn any_point() -> impl Strategy<Value = Point2> {
    (any_coordinate(), any_coordinate()).prop_map(|(x, y)| Point2::new(x, y))
}

/// Closed loops of arbitrary lines and arcs: each edge starts where the previous one ends.
fn any_loop() -> impl Strategy<Value = Vec<Edge>> {
    prop::collection::vec(
        (any_point(), any_point(), any::<bool>(), any::<bool>()),
        1..8,
    )
    .prop_map(|raw| {
        let n = raw.len();
        (0..n)
            .map(|i| {
                let (start, centre, is_arc, ccw) = raw[i];
                let end = raw[(i + 1) % n].0;
                if is_arc {
                    let direction = if ccw {
                        ArcDirection::CounterClockwise
                    } else {
                        ArcDirection::Clockwise
                    };
                    Edge::Arc {
                        start,
                        end,
                        centre,
                        direction,
                    }
                } else {
                    Edge::Line { start, end }
                }
            })
            .collect()
    })
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1_000,
        rng_seed: RngSeed::Fixed(49),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn convex_polygon_area_matches_exact_arithmetic(points in convex_polygon()) {
        let profile = Profile::new(polygon(&points), vec![]).expect("convex polygons are valid");
        let expected = exact_double_area(&points);
        let error = exact(2.0 * profile.area()) - &expected;
        let bound = exact(1e-9) * &expected;
        prop_assert!(error.clone() < bound.clone() && -error < bound);
    }

    #[test]
    fn plate_with_round_hole_matches_closed_form(
        (width, height, radius, cx, cy) in plate_with_hole()
    ) {
        let profile = Profile::new(plate(width, height), vec![circle(cx, cy, radius)])
            .expect("the hole is inside");
        let hole = PI * radius * radius;
        let area = width * height - hole;
        prop_assert!(relative_error(profile.area(), area) < 1e-9);
        let c = profile.centroid();
        let expected_x = (width * height * width / 2.0 - hole * cx) / area;
        let expected_y = (width * height * height / 2.0 - hole * cy) / area;
        prop_assert!((c.x - expected_x).abs() < 1e-9 * width);
        prop_assert!((c.y - expected_y).abs() < 1e-9 * height);
    }

    #[test]
    fn translation_moves_the_centroid_and_keeps_the_area(
        (width, height, radius, cx, cy) in plate_with_hole(),
        dx in -1e3..1e3f64,
        dy in -1e3..1e3f64,
    ) {
        let base = Profile::new(plate(width, height), vec![circle(cx, cy, radius)])
            .expect("the hole is inside");
        let moved = Profile::new(
            translated(&plate(width, height), dx, dy),
            vec![translated(&circle(cx, cy, radius), dx, dy)],
        )
        .expect("translation keeps validity");
        prop_assert!(relative_error(moved.area(), base.area()) < 1e-9);
        let (a, b) = (base.centroid(), moved.centroid());
        prop_assert!((b.x - a.x - dx).abs() < 1e-9 * (1.0 + dx.abs()));
        prop_assert!((b.y - a.y - dy).abs() < 1e-9 * (1.0 + dy.abs()));
    }

    #[test]
    fn loop_direction_does_not_matter((width, height, radius, cx, cy) in plate_with_hole()) {
        let outer = plate(width, height);
        let hole = circle(cx, cy, radius);
        let canonical = Profile::new(outer.clone(), vec![reversed(&hole)]);
        let flipped = Profile::new(reversed(&outer), vec![hole]);
        prop_assert_eq!(canonical, flipped);
    }

    #[test]
    fn arbitrary_input_never_panics(outer in any_loop(), holes in prop::collection::vec(any_loop(), 0..3)) {
        if let Ok(profile) = Profile::new(outer, holes) {
            prop_assert!(profile.area() > 0.0);
        }
    }

    #[test]
    fn a_disc_is_bounded_by_its_circle(
        radius in 1e-2..10.0f64,
        cx in -1e2..1e2f64,
        cy in -1e2..1e2f64,
    ) {
        let disc = Profile::new(circle(cx, cy, radius), vec![]).expect("a circle is valid");
        let expected = [cx - radius, cy - radius, cx + radius, cy + radius];
        for (got, want) in disc.bounds().into_iter().zip(expected) {
            // The radius is measured from the stored points, so it may differ in the last bits.
            prop_assert!((got - want).abs() <= 1e-12 * (1.0 + want.abs()));
        }
    }
}
