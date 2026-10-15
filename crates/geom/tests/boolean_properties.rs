//! Property-based tests of the booleans on random pairs of boxes (GEO-03, ENG-02): union,
//! difference and intersection.
//!
//! Boxes on a coarse grid share faces, edges and corners often, which exercises the degenerate
//! cases; boxes at random coordinates, and pairs turned together to any angle, exercise the
//! general ones. The expected volumes come from the exact overlap of axis-aligned boxes, which a
//! common rotation leaves unchanged. Grid boxes moved by up to half the tolerance check that
//! near-coincident faces, edges and corners still merge (ADR 0001); boxes turned independently
//! check the general position, where only bounds on the volume are known.

use geom::{FaceInput, Point3, Solid};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

type Corners = ([f64; 3], [f64; 3]);

/// Two boxes and the rotation, about z then x, applied to both.
#[derive(Clone, Debug)]
struct Case {
    a: Corners,
    b: Corners,
    turn: (f64, f64),
}

fn rotate(p: [f64; 3], (about_z, about_x): (f64, f64)) -> Point3 {
    let (sz, cz) = (libm::sin(about_z), libm::cos(about_z));
    let (sx, cx) = (libm::sin(about_x), libm::cos(about_x));
    let [x, y, z] = p;
    let (x, y) = (cz * x - sz * y, sz * x + cz * y);
    let (y, z) = (cx * y - sx * z, sx * y + cx * z);
    Point3::new(x, y, z)
}

fn cuboid((min, max): Corners, turn: (f64, f64)) -> Result<Solid, TestCaseError> {
    let vertices = (0..8)
        .map(|i| {
            let pick = |bit: usize, axis: usize| if i & bit == 0 { min[axis] } else { max[axis] };
            rotate([pick(1, 0), pick(2, 1), pick(4, 2)], turn)
        })
        .collect();
    let faces: Vec<FaceInput> = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ]
    .iter()
    .map(|ring| FaceInput {
        outer: ring.to_vec(),
        holes: Vec::new(),
    })
    .collect();
    Solid::from_faces(vertices, &faces).map_err(|e| TestCaseError::fail(e.to_string()))
}

fn volume((min, max): Corners) -> f64 {
    (0..3).map(|i| max[i] - min[i]).product()
}

/// The volume both boxes share, exactly as the axis-aligned boxes do.
fn overlap((a_min, a_max): Corners, (b_min, b_max): Corners) -> f64 {
    (0..3)
        .map(|i| (a_max[i].min(b_max[i]) - a_min[i].max(b_min[i])).max(0.0))
        .product()
}

/// A box whose corners lie on a 0.5 m grid, so faces often coincide.
fn grid_box() -> impl Strategy<Value = Corners> {
    let span = || {
        (0_u8..6, 1_u8..4).prop_map(|(lo, len)| (f64::from(lo) / 2.0, f64::from(lo + len) / 2.0))
    };
    (span(), span(), span()).prop_map(|((x0, x1), (y0, y1), (z0, z1))| ([x0, y0, z0], [x1, y1, z1]))
}

/// A box anywhere near the origin, at least 1 cm thick.
fn free_box() -> impl Strategy<Value = Corners> {
    let span = || (-5.0..5.0_f64, 0.01..5.0_f64).prop_map(|(lo, len)| (lo, lo + len));
    (span(), span(), span()).prop_map(|((x0, x1), (y0, y1), (z0, z1))| ([x0, y0, z0], [x1, y1, z1]))
}

/// A grid box whose corners are each moved by −0.5, 0 or +0.5 µm along each axis: inside the
/// tolerance of the grid.
fn nudged_box() -> impl Strategy<Value = Corners> {
    let nudge = || prop::array::uniform3(-1_i8..=1);
    (grid_box(), nudge(), nudge()).prop_map(|((min, max), lo, hi)| {
        let moved = |c: [f64; 3], by: [i8; 3]| [0, 1, 2].map(|i| c[i] + f64::from(by[i]) * 0.5e-6);
        (moved(min, lo), moved(max, hi))
    })
}

fn cases() -> impl Strategy<Value = Case> {
    let turn = (0.0..std::f64::consts::TAU, 0.0..std::f64::consts::TAU);
    prop_oneof![
        (grid_box(), grid_box()).prop_map(|(a, b)| Case {
            a,
            b,
            turn: (0.0, 0.0)
        }),
        (grid_box(), grid_box(), turn.clone()).prop_map(|(a, b, turn)| Case { a, b, turn }),
        (free_box(), free_box(), turn.clone()).prop_map(|(a, b, turn)| Case { a, b, turn }),
        (grid_box(), nudged_box(), turn).prop_map(|(a, b, turn)| Case { a, b, turn }),
    ]
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1_000,
        rng_seed: RngSeed::Fixed(52),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

/// The result of an operation, checked valid, or a failure naming the error.
fn checked(
    result: Result<Option<Solid>, geom::BooleanError>,
) -> Result<Option<Solid>, TestCaseError> {
    let result = result.map_err(|e| TestCaseError::fail(e.to_string()))?;
    if let Some(solid) = &result {
        solid
            .validate()
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
    }
    Ok(result)
}

fn volume_of(solid: Option<&Solid>) -> f64 {
    solid.map_or(0.0, Solid::volume)
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn union_adds_volumes_less_the_overlap(case in cases()) {
        let (a, b) = (cuboid(case.a, case.turn)?, cuboid(case.b, case.turn)?);
        let union = checked(a.union(&b))?;
        let expected = volume(case.a) + volume(case.b) - overlap(case.a, case.b);
        // Merging moves vertices by less than the tolerance; elsewhere the volume is exact.
        prop_assert!((volume_of(union.as_ref()) - expected).abs() <= 1e-4);
    }

    #[test]
    fn union_does_not_depend_on_operand_order(case in cases()) {
        let (a, b) = (cuboid(case.a, case.turn)?, cuboid(case.b, case.turn)?);
        let forward = checked(a.union(&b))?;
        let backward = checked(b.union(&a))?;
        let counts = |s: Option<&Solid>| s.map(|s| (s.vertices().len(), s.edge_count(), s.face_count(), s.shells().len()));
        prop_assert_eq!(counts(forward.as_ref()), counts(backward.as_ref()));
        prop_assert!((volume_of(forward.as_ref()) - volume_of(backward.as_ref())).abs() <= 1e-9 * volume_of(forward.as_ref()).max(1.0));
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn difference_removes_the_overlap(case in cases()) {
        let (a, b) = (cuboid(case.a, case.turn)?, cuboid(case.b, case.turn)?);
        let cut = checked(a.difference(&b))?;
        let expected = volume(case.a) - overlap(case.a, case.b);
        prop_assert!((volume_of(cut.as_ref()) - expected).abs() <= 1e-4);
        // Putting the cutter back gives the union.
        let union = match &cut {
            Some(cut) => checked(cut.union(&b))?,
            None => Some(b.clone()),
        };
        let expected = volume(case.a) + volume(case.b) - overlap(case.a, case.b);
        prop_assert!((volume_of(union.as_ref()) - expected).abs() <= 1e-4);
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn intersection_is_the_overlap(case in cases()) {
        let (a, b) = (cuboid(case.a, case.turn)?, cuboid(case.b, case.turn)?);
        let shared = checked(a.intersection(&b))?;
        prop_assert!((volume_of(shared.as_ref()) - overlap(case.a, case.b)).abs() <= 1e-4);
        let shared_back = checked(b.intersection(&a))?;
        prop_assert!((volume_of(shared.as_ref()) - volume_of(shared_back.as_ref())).abs() <= 1e-4);
        // The cut and the overlap together make up the first box.
        let cut = checked(a.difference(&b))?;
        let whole = volume_of(cut.as_ref()) + volume_of(shared.as_ref());
        prop_assert!((whole - volume(case.a)).abs() <= 1e-4);
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn booleans_of_boxes_turned_apart_are_valid(
        a in free_box(),
        b in free_box(),
        turn_a in (0.0..std::f64::consts::TAU, 0.0..std::f64::consts::TAU),
        turn_b in (0.0..std::f64::consts::TAU, 0.0..std::f64::consts::TAU),
    ) {
        let (a_solid, b_solid) = (cuboid(a, turn_a)?, cuboid(b, turn_b)?);
        let forward = volume_of(checked(a_solid.union(&b_solid))?.as_ref());
        let backward = volume_of(checked(b_solid.union(&a_solid))?.as_ref());
        prop_assert!(forward <= volume(a) + volume(b) + 1e-9 && forward + 1e-9 >= volume(a).max(volume(b)));
        prop_assert!((forward - backward).abs() <= 1e-9 * forward);
        // What the cut removes is what the union adds back, and inclusion–exclusion holds.
        let cut = volume_of(checked(a_solid.difference(&b_solid))?.as_ref());
        prop_assert!((cut - (forward - volume(b))).abs() <= 1e-9 * forward);
        let shared = volume_of(checked(a_solid.intersection(&b_solid))?.as_ref());
        prop_assert!((forward + shared - volume(a) - volume(b)).abs() <= 1e-9 * forward);
    }
}
