//! Property-based tests of extrusion (GEO-02, ENG-02): random convex polygons, rectangles with a
//! hole and circles, extruded along random upward directions anywhere in the model extent.

use std::f64::consts::TAU;

use geom::{ARC_CHORD_TOLERANCE, ArcDirection, Edge, Point2, Profile, Solid, Vector3};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// Profile loops with what their extrusion should give.
#[derive(Clone, Debug)]
struct Case {
    outer: Vec<Edge>,
    holes: Vec<Vec<Edge>>,
    /// Expected face count; `None` for faceted arcs, whose chord count is not computed here.
    faces: Option<usize>,
    genus: u32,
    /// How much area the chords may cut off the profile.
    deficit: f64,
}

fn polygon(points: &[Point2]) -> Vec<Edge> {
    (0..points.len())
        .map(|i| Edge::Line {
            start: points[i],
            end: points[(i + 1) % points.len()],
        })
        .collect()
}

fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Edge> {
    polygon(&[
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
}

fn coordinate() -> impl Strategy<Value = f64> {
    -1_000.0..1_000.0_f64
}

fn length() -> impl Strategy<Value = f64> {
    1e-2..50.0_f64
}

/// A convex polygon inscribed in a circle, its corners at least 1/64 turn apart.
fn polygons() -> impl Strategy<Value = Case> {
    (
        prop::sample::subsequence((0..64).collect::<Vec<u32>>(), 3..=12),
        prop::array::uniform2(coordinate()),
        length(),
    )
        .prop_map(|(steps, [cx, cy], radius)| {
            let ring: Vec<Point2> = steps
                .iter()
                .map(|&k| {
                    let angle = TAU * f64::from(k) / 64.0;
                    Point2::new(
                        cx + radius * libm::cos(angle),
                        cy + radius * libm::sin(angle),
                    )
                })
                .collect();
            Case {
                faces: Some(ring.len() + 2),
                outer: polygon(&ring),
                holes: Vec::new(),
                genus: 0,
                deficit: 0.0,
            }
        })
}

/// A rectangle with a rectangular hole at least 1 mm from every side.
fn holed_rectangles() -> impl Strategy<Value = Case> {
    (
        prop::array::uniform2(coordinate()),
        prop::array::uniform2(0.01..0.98_f64),
        prop::array::uniform2(0.01..0.98_f64),
        prop::array::uniform2(0.1..50.0_f64),
    )
        .prop_map(|([x, y], [x0, x1], [y0, y1], [w, h])| {
            let (x0, x1) = (x0.min(x1), x0.max(x1) + 0.01);
            let (y0, y1) = (y0.min(y1), y0.max(y1) + 0.01);
            let hole = rectangle(x + x0 * w, y + y0 * h, x + x1 * w, y + y1 * h);
            Case {
                outer: rectangle(x, y, x + w, y + h),
                holes: vec![hole],
                faces: Some(10),
                genus: 1,
                deficit: 0.0,
            }
        })
}

/// A circle, whose chords cut at most `ARC_CHORD_TOLERANCE` into it along its whole length.
fn circles() -> impl Strategy<Value = Case> {
    (prop::array::uniform2(coordinate()), length()).prop_map(|([cx, cy], radius)| {
        let start = Point2::new(cx + radius, cy);
        let circle = Edge::Arc {
            start,
            end: start,
            centre: Point2::new(cx, cy),
            direction: ArcDirection::CounterClockwise,
        };
        Case {
            outer: vec![circle],
            holes: Vec::new(),
            faces: None,
            genus: 0,
            deficit: TAU * radius * ARC_CHORD_TOLERANCE,
        }
    })
}

fn directions() -> impl Strategy<Value = Vector3> {
    (-50.0..50.0_f64, -50.0..50.0_f64, 1e-2..50.0_f64).prop_map(|(x, y, z)| Vector3::new(x, y, z))
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 500,
        rng_seed: RngSeed::Fixed(51),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn extrusions_are_valid_prisms(
        case in prop_oneof![polygons(), holed_rectangles(), circles()],
        direction in directions(),
    ) {
        let profile = Profile::new(case.outer, case.holes);
        let profile = profile.map_err(|e| TestCaseError::fail(e.to_string()))?;
        let solid = Solid::extrude(&profile, direction);
        let solid = solid.map_err(|e| TestCaseError::fail(e.to_string()))?;
        let [shell] = solid.shells() else {
            return Err(TestCaseError::fail("one shell expected"));
        };
        prop_assert_eq!(shell.genus(), case.genus);
        if let Some(faces) = case.faces {
            prop_assert_eq!(solid.face_count(), faces);
        }
        // Euler for a prism over n edges: 3n edges, n + 2 faces.
        prop_assert_eq!(solid.edge_count(), 3 * (solid.face_count() - 2));
        let max = profile.area() * direction.z;
        let min = (profile.area() - case.deficit) * direction.z;
        let tolerance = 1e-9 * max;
        prop_assert!(
            (min - tolerance..=max + tolerance).contains(&solid.volume()),
            "{} not in [{min}, {max}]",
            solid.volume()
        );
        prop_assert_eq!(solid.validate(), Ok(()));
    }
}
