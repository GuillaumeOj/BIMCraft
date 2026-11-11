//! Property-based tests of sweeps (GEO-02, ENG-02): centred rectangles, with or without a hole,
//! swept along random polylines must give one shell and the volume area × path length of ADR
//! 0020, and along random arcs a volume just above Pappus'; arbitrary profiles, paths and
//! references must give a valid solid or a typed error, never a panic.

use std::f64::consts::{PI, TAU};

use geom::{
    ARC_CHORD_TOLERANCE, ArcDirection, Edge, MODEL_EXTENT, Path, PathSegment, Point2, Point3,
    Profile, Solid, SweepError, Vector3,
};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// A centred rectangle of half sides `half` (profile x, profile y), maybe with a hole of half
/// its size.
fn bar(half: (f64, f64), hole: bool) -> Result<Profile, TestCaseError> {
    let rectangle = |(w, h): (f64, f64), ccw: bool| {
        let mut corners = [
            Point2::new(-w, -h),
            Point2::new(w, -h),
            Point2::new(w, h),
            Point2::new(-w, h),
        ];
        if !ccw {
            corners.reverse();
        }
        (0..4)
            .map(|i| Edge::Line {
                start: corners[i],
                end: corners[(i + 1) % 4],
            })
            .collect::<Vec<_>>()
    };
    let holes = if hole {
        vec![rectangle((half.0 / 2.0, half.1 / 2.0), false)]
    } else {
        vec![]
    };
    Profile::new(rectangle(half, true), holes).map_err(|e| TestCaseError::fail(e.to_string()))
}

fn unit(azimuth: f64, elevation: f64) -> [f64; 3] {
    let (ca, sa) = (libm::cos(azimuth), libm::sin(azimuth));
    let (ce, se) = (libm::cos(elevation), libm::sin(elevation));
    [ca * ce, sa * ce, se]
}

fn point(p: [f64; 3]) -> Point3 {
    Point3::new(p[0], p[1], p[2])
}

/// A polyline from `start` along each (azimuth, elevation, length) step.
fn polyline(start: [f64; 3], steps: &[(f64, f64, f64)]) -> (Vec<PathSegment>, f64) {
    let mut at = start;
    let mut total = 0.0;
    let segments = steps
        .iter()
        .map(|&(azimuth, elevation, length)| {
            let d = unit(azimuth, elevation);
            let next = [0, 1, 2].map(|i| length.mul_add(d[i], at[i]));
            let segment = PathSegment::Line {
                start: point(at),
                end: point(next),
            };
            total += libm::sqrt((0..3).map(|i| (next[i] - at[i]) * (next[i] - at[i])).sum());
            at = next;
            segment
        })
        .collect();
    (segments, total)
}

fn start_point() -> impl Strategy<Value = [f64; 3]> {
    prop::array::uniform3(-100.0..100.0_f64)
}

fn steps() -> impl Strategy<Value = Vec<(f64, f64, f64)>> {
    prop::collection::vec((-PI..PI, -1.5..1.5_f64, 0.5..5.0_f64), 1..6)
}

fn half_sides() -> impl Strategy<Value = (f64, f64)> {
    (0.01..0.2_f64, 0.01..0.2_f64)
}

fn reference() -> impl Strategy<Value = Vector3> {
    prop_oneof![
        Just(Vector3::new(0.0, 0.0, 1.0)),
        prop::array::uniform3(-1.0..1.0_f64).prop_map(|[x, y, z]| Vector3::new(x, y, z)),
    ]
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

fn any_point3() -> impl Strategy<Value = Point3> {
    prop::array::uniform3(any_coordinate()).prop_map(point)
}

fn any_loop() -> impl Strategy<Value = Vec<Edge>> {
    let point = || (any_coordinate(), any_coordinate()).prop_map(|(x, y)| Point2::new(x, y));
    prop::collection::vec((point(), point(), any::<Option<bool>>()), 1..6).prop_map(|raw| {
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

/// Segments chained by construction: each starts where the previous one ends.
fn any_path() -> impl Strategy<Value = Vec<PathSegment>> {
    (
        any_point3(),
        prop::collection::vec((any_point3(), any_point3(), any::<bool>()), 1..5),
    )
        .prop_map(|(start, raw)| {
            let mut at = start;
            raw.into_iter()
                .map(|(through, end, is_arc)| {
                    let segment = if is_arc {
                        PathSegment::Arc {
                            start: at,
                            through,
                            end,
                        }
                    } else {
                        PathSegment::Line { start: at, end }
                    };
                    at = end;
                    segment
                })
                .collect()
        })
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 300,
        rng_seed: RngSeed::Fixed(74),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn rectangles_along_polylines_have_area_times_length(
        start in start_point(),
        steps in steps(),
        half in half_sides(),
        hole in any::<bool>(),
        reference in reference(),
    ) {
        let (segments, total) = polyline(start, &steps);
        let Ok(path) = Path::new(segments) else {
            return Ok(());
        };
        let profile = bar(half, hole)?;
        match Solid::sweep(&profile, &path, reference) {
            Ok(solid) => {
                prop_assert_eq!(solid.validate(), Ok(()));
                prop_assert_eq!(solid.shells().len(), 1);
                prop_assert_eq!(solid.shells()[0].genus(), u32::from(hole));
                // The centroid runs the path, so each piece adds the area times its length.
                let expected = profile.area() * total;
                prop_assert!(
                    (solid.volume() - expected).abs() <= 1e-9 * expected,
                    "{} != {}", solid.volume(), expected
                );
            }
            // Random corners can be too sharp, legs too close or the reference along the path;
            // nothing else may fail.
            Err(error) => prop_assert!(matches!(
                error,
                SweepError::ReferenceAlongPath
                    | SweepError::DoublesBack { .. }
                    | SweepError::MitresCross { .. }
                    | SweepError::SelfIntersecting { .. }
            ), "{error:?}"),
        }
    }

    #[test]
    fn rectangles_along_arcs_lie_just_above_pappus(
        centre in start_point(),
        radius in 0.5..20.0_f64,
        sweep in 0.1..TAU - 0.1,
        normal in (-PI..PI, -1.5..1.5_f64),
        half in half_sides(),
    ) {
        // An orthonormal frame of the arc's plane: e1 normal to n, e2 = n × e1.
        let n = unit(normal.0, normal.1);
        let helper = if n[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
        let dot = helper[0] * n[0] + helper[1] * n[1] + helper[2] * n[2];
        let raw = [0, 1, 2].map(|i| helper[i] - dot * n[i]);
        let size = libm::sqrt(raw.iter().map(|c| c * c).sum());
        let e1 = raw.map(|c| c / size);
        let e2 = [
            n[1] * e1[2] - n[2] * e1[1],
            n[2] * e1[0] - n[0] * e1[2],
            n[0] * e1[1] - n[1] * e1[0],
        ];
        let at = |angle: f64| {
            let (c, s) = (libm::cos(angle), libm::sin(angle));
            point([0, 1, 2].map(|i| radius * c.mul_add(e1[i], s * e2[i]) + centre[i]))
        };
        let segment = PathSegment::Arc { start: at(0.0), through: at(sweep / 2.0), end: at(sweep) };
        let path = Path::new(vec![segment]).map_err(|e| TestCaseError::fail(e.to_string()))?;
        let profile = bar(half, false)?;
        // The reference along the arc's axis keeps the profile's x axis in the arc's plane.
        let reference = Vector3::new(n[0], n[1], n[2]);
        let solid = Solid::sweep(&profile, &path, reference)
            .map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(solid.validate(), Ok(()));
        prop_assert_eq!(solid.shells().len(), 1);
        // Pappus: V = θ · A · R, the centroid on the arc. The tangent polygon lies outside the
        // circle, so the faceted volume is larger, by at most the tolerance times the curved
        // area it replaces: the perimeter times the farthest arc length.
        let pappus = sweep * profile.area() * radius;
        let excess = solid.volume() - pappus;
        let perimeter = 4.0 * (half.0 + half.1);
        let reach = libm::hypot(half.0, half.1);
        prop_assert!(excess >= -1e-12 * pappus, "{} below Pappus {pappus}", solid.volume());
        prop_assert!(
            excess <= ARC_CHORD_TOLERANCE * perimeter * (radius + reach) * sweep,
            "{} too far above Pappus {pappus}", solid.volume()
        );
    }

    #[test]
    fn arbitrary_input_gives_a_valid_solid_or_an_error(
        outer in any_loop(),
        segments in any_path(),
        reference in prop::array::uniform3(any_coordinate()),
    ) {
        let (Ok(profile), Ok(path)) = (Profile::new(outer, vec![]), Path::new(segments)) else {
            return Ok(());
        };
        let [x, y, z] = reference;
        if let Ok(solid) = Solid::sweep(&profile, &path, Vector3::new(x, y, z)) {
            prop_assert_eq!(solid.validate(), Ok(()));
            prop_assert!(solid.volume() > 0.0);
            prop_assert_eq!(solid.shells().len(), 1);
        }
    }
}
