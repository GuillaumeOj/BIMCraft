//! Property-based tests of revolution (GEO-02, ENG-02): random rectangles, with or without a
//! hole, on either side of a random axis and touching it or not, turned by random angles must
//! give the shells and volume of ADR 0020; arbitrary loops, axes and angles must give a valid
//! solid or a typed error.

use std::f64::consts::{PI, TAU};

use geom::{
    ArcDirection, Edge, MODEL_EXTENT, Point2, Profile, RevolutionAxis, RevolutionError, Solid,
    Vector2,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// A rectangle in the frame of the axis (`along` the axis, `across` it), maybe with a hole.
#[derive(Clone, Debug)]
struct Case {
    origin: Point2,
    heading: f64,
    along: f64,
    length: f64,
    across: f64,
    width: f64,
    /// Whether the section lies to the right of the axis.
    right: bool,
    hole: bool,
    angle: f64,
}

impl Case {
    fn axis(&self) -> RevolutionAxis {
        RevolutionAxis {
            origin: self.origin,
            direction: Vector2::new(libm::cos(self.heading), libm::sin(self.heading)),
        }
    }

    fn corner(&self, along: f64, across: f64) -> Point2 {
        let across = if self.right { -across } else { across };
        let (c, s) = (libm::cos(self.heading), libm::sin(self.heading));
        Point2::new(
            self.origin.x + along * c - across * s,
            self.origin.y + along * s + across * c,
        )
    }

    /// The rectangle `[a0, a1] × [d0, d1]` of the axis frame.
    fn rectangle(&self, [a0, a1, d0, d1]: [f64; 4]) -> Vec<Edge> {
        let corners = [
            self.corner(a0, d0),
            self.corner(a1, d0),
            self.corner(a1, d1),
            self.corner(a0, d1),
        ];
        (0..4)
            .map(|i| Edge::Line {
                start: corners[i],
                end: corners[(i + 1) % 4],
            })
            .collect()
    }

    fn profile(&self) -> Result<Profile, TestCaseError> {
        let (a, l, d, w) = (self.along, self.length, self.across, self.width);
        let outer = self.rectangle([a, a + l, d, d + w]);
        let hole = self.rectangle([a + 0.25 * l, a + 0.75 * l, d + 0.25 * w, d + 0.75 * w]);
        let holes = if self.hole { vec![hole] } else { vec![] };
        Profile::new(outer, holes).map_err(|e| TestCaseError::fail(e.to_string()))
    }

    fn touches_axis(&self) -> bool {
        self.across == 0.0
    }

    fn full(&self) -> bool {
        self.angle >= TAU
    }
}

fn cases() -> impl Strategy<Value = Case> {
    (
        (-100.0..100.0_f64, -100.0..100.0_f64),
        -PI..PI,
        (-50.0..50.0_f64, 0.1..20.0_f64),
        (prop_oneof![Just(0.0), 0.01..30.0_f64], 0.1..20.0_f64),
        (any::<bool>(), any::<bool>()),
        prop_oneof![Just(TAU), Just(PI / 2.0), 0.05..TAU - 0.5],
    )
        .prop_map(
            |((x, y), heading, (along, length), (across, width), (right, hole), angle)| Case {
                origin: Point2::new(x, y),
                heading,
                along,
                length,
                across,
                width,
                right,
                hole,
                angle,
            },
        )
}

fn any_coordinate() -> impl Strategy<Value = f64> {
    prop_oneof![
        (-4..4_i32).prop_map(f64::from),
        -2.0 * MODEL_EXTENT..2.0 * MODEL_EXTENT,
        Just(f64::NAN),
        Just(f64::INFINITY),
        Just(1e-40),
    ]
}

fn any_loop() -> impl Strategy<Value = Vec<Edge>> {
    let point = || (any_coordinate(), any_coordinate()).prop_map(|(x, y)| Point2::new(x, y));
    prop::collection::vec((point(), point(), any::<Option<bool>>()), 1..8).prop_map(|raw| {
        (0..raw.len())
            .map(|i| {
                let (start, centre, arc) = raw[i];
                let end = raw[(i + 1) % raw.len()].0;
                match arc {
                    None => Edge::Line { start, end },
                    Some(ccw) => Edge::Arc {
                        start,
                        end,
                        centre,
                        direction: if ccw {
                            ArcDirection::CounterClockwise
                        } else {
                            ArcDirection::Clockwise
                        },
                    },
                }
            })
            .collect()
    })
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 500,
        rng_seed: RngSeed::Fixed(74),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn revolved_rectangles_have_the_shells_and_volume_of_the_formula(case in cases()) {
        let profile = case.profile()?;
        let solid = Solid::revolve(&profile, case.axis(), case.angle)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(solid.validate(), Ok(()));

        // Side faces: one per step for each profile edge off the axis.
        let side_edges = 4 * usize::from(case.hole) + 4 - usize::from(case.touches_axis());
        let end_faces = if case.full() { 0 } else { 2 };
        let side_faces = solid.face_count() - end_faces;
        prop_assert_eq!(side_faces % side_edges, 0);
        let steps = side_faces / side_edges;
        let steps = f64::from(u32::try_from(steps).map_err(|e| TestCaseError::fail(e.to_string()))?);
        prop_assert!(case.angle / steps <= PI / 2.0);

        let holes = u32::from(case.hole);
        let shells = solid.shells();
        if case.full() {
            // Ring (genus 1) unless it touches the axis (genus 0), plus a ring-shaped void.
            prop_assert_eq!(shells.len(), 1 + usize::from(case.hole));
            prop_assert_eq!(shells[0].genus(), u32::from(!case.touches_axis()));
            prop_assert_eq!(shells.iter().filter(|s| s.is_void()).count(), usize::from(case.hole));
            prop_assert!(shells.iter().skip(1).all(|s| s.genus() == 1));
        } else {
            prop_assert_eq!(shells.len(), 1);
            prop_assert_eq!(shells[0].genus(), holes);
        }

        // V = n · sin α · A_f · r̄: the first moment of a rectangle (and of its hole) about the
        // axis is its area times the distance of its centre, the same for both.
        let centre = case.across + case.width / 2.0;
        let area = profile.area();
        let expected = steps * libm::sin(case.angle / steps) * area * centre;
        prop_assert!(
            (solid.volume() - expected).abs() <= 1e-7 * expected,
            "{} != {expected}", solid.volume()
        );
        // The faceted solid lies inside the exact one (Pappus).
        prop_assert!(solid.volume() <= area * case.angle * centre * (1.0 + 1e-9));
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn arbitrary_input_gives_a_valid_solid_or_an_error(
        outer in any_loop(),
        holes in prop::collection::vec(any_loop(), 0..3),
        axis in prop::array::uniform4(any_coordinate()),
        angle in prop_oneof![any_coordinate(), 0.0..TAU, Just(TAU)],
    ) {
        let Ok(profile) = Profile::new(outer, holes) else {
            return Ok(());
        };
        let [ox, oy, dx, dy] = axis;
        let axis = RevolutionAxis {
            origin: Point2::new(ox, oy),
            direction: Vector2::new(dx, dy),
        };
        let result = Solid::revolve(&profile, axis, angle);
        if !(angle > 0.0 && angle <= TAU) {
            // The axis is checked first, so an invalid axis may be reported instead.
            prop_assert!(matches!(
                result,
                Err(RevolutionError::AngleOutOfRange
                    | RevolutionError::NonFiniteAxis
                    | RevolutionError::AxisOutsideExtent
                    | RevolutionError::ZeroAxisDirection)
            ));
        } else if let Ok(solid) = result {
            prop_assert_eq!(solid.validate(), Ok(()));
            prop_assert!(solid.volume() > 0.0);
        }
    }
}
