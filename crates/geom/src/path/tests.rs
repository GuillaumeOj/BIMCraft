use core::f64::consts::{FRAC_PI_2, PI};

use super::*;
use crate::MODEL_EXTENT;

pub(crate) fn q(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

pub(crate) fn line(start: Point3, end: Point3) -> PathSegment {
    PathSegment::Line { start, end }
}

pub(crate) fn arc(start: Point3, through: Point3, end: Point3) -> PathSegment {
    PathSegment::Arc {
        start,
        through,
        end,
    }
}

fn the_arc(segment: PathSegment) -> Arc3 {
    let path = Path::new(vec![segment]).unwrap();
    let Curve3::Arc(arc) = path.curves()[0] else {
        unreachable!("an arc was expected")
    };
    arc
}

fn assert_vector(actual: Vector, expected: Vector) {
    for i in 0..3 {
        assert!(
            (actual[i] - expected[i]).abs() < 1e-12,
            "{actual:?} != {expected:?}"
        );
    }
}

#[test]
fn a_chain_of_lines_and_arcs_is_kept_as_given() {
    let segments = vec![
        line(q(0.0, 0.0, 0.0), q(2.0, 0.0, 0.0)),
        arc(
            q(2.0, 0.0, 0.0),
            q(2.0 + libm::sqrt(0.5), 1.0 - libm::sqrt(0.5), 0.0),
            q(3.0, 1.0, 0.0),
        ),
        line(q(3.0, 1.0, 0.0), q(3.0, 1.0, 4.0)),
    ];
    let path = Path::new(segments.clone()).unwrap();
    assert_eq!(path.segments(), segments.as_slice());
    assert_eq!(path.curves().len(), 3);
    assert_eq!(path.curves()[0], Curve3::Line([2.0, 0.0, 0.0]));
    assert_eq!(segments[1].start(), q(2.0, 0.0, 0.0));
    assert_eq!(segments[1].end(), q(3.0, 1.0, 0.0));
}

#[test]
fn an_arc_gets_its_centre_radius_and_sense_from_its_three_points() {
    // Quarter circle of radius 1 around (2, 1, 0), counterclockwise seen from +z: it starts at
    // (2, 0, 0) heading +x, away from the centre along −y.
    let quarter = the_arc(arc(
        q(2.0, 0.0, 0.0),
        q(2.0 + libm::sqrt(0.5), 1.0 - libm::sqrt(0.5), 0.0),
        q(3.0, 1.0, 0.0),
    ));
    assert!((quarter.radius - 1.0).abs() < 1e-12);
    assert!((quarter.sweep - FRAC_PI_2).abs() < 1e-12);
    assert_vector(quarter.outward, [0.0, -1.0, 0.0]);
    assert_vector(quarter.tangent, [1.0, 0.0, 0.0]);
    // The same points the other way: clockwise, so the tangent at (3, 1, 0) heads −y.
    let back = the_arc(arc(
        q(3.0, 1.0, 0.0),
        q(2.0 + libm::sqrt(0.5), 1.0 - libm::sqrt(0.5), 0.0),
        q(2.0, 0.0, 0.0),
    ));
    assert!((back.sweep - FRAC_PI_2).abs() < 1e-12);
    assert_vector(back.tangent, [0.0, -1.0, 0.0]);
    // Three quarters of a circle of radius 2 in the plane x = 5: more than a half turn.
    let large = the_arc(arc(q(5.0, 2.0, 0.0), q(5.0, -2.0, 0.0), q(5.0, 0.0, -2.0)));
    assert!((large.radius - 2.0).abs() < 1e-12);
    assert!((large.sweep - 1.5 * PI).abs() < 1e-12);
    assert_vector(large.outward, [0.0, 1.0, 0.0]);
    assert_vector(large.tangent, [0.0, 0.0, 1.0]);
}

#[test]
fn every_error_is_reported() {
    let o = q(0.0, 0.0, 0.0);
    let x = q(1.0, 0.0, 0.0);
    let check = |segments: Vec<PathSegment>| Path::new(segments).err();
    assert_eq!(check(vec![]), Some(PathError::Empty));
    let many: Vec<PathSegment> = (0..=MAX_PATH_SEGMENTS)
        .map(|i| {
            let i = f64::from(u32::try_from(i).unwrap());
            line(q(i, 0.0, 0.0), q(i + 1.0, 0.0, 0.0))
        })
        .collect();
    assert_eq!(check(many), Some(PathError::TooManySegments));
    for bad in [f64::NAN, f64::INFINITY] {
        assert_eq!(
            check(vec![line(o, x), line(x, q(1.0, bad, 0.0))]),
            Some(PathError::NonFinite { segment: 1 })
        );
        assert_eq!(
            check(vec![arc(o, q(0.5, 0.5, bad), x)]),
            Some(PathError::NonFinite { segment: 0 })
        );
    }
    for bad in [2.0 * MODEL_EXTENT, 1e-300] {
        assert_eq!(
            check(vec![line(o, q(0.0, 0.0, bad))]),
            Some(PathError::OutOfRange { segment: 0 })
        );
    }
    assert_eq!(
        check(vec![line(o, x), line(q(1.0, 1e-9, 0.0), q(2.0, 0.0, 0.0))]),
        Some(PathError::Gap { segment: 0 })
    );
    assert_eq!(
        check(vec![line(o, x), line(x, q(1.0, 1e-6, 0.0))]),
        Some(PathError::ZeroLengthLine { segment: 1 })
    );
    // Coincident points, then collinear ones: the middle point 1 µm off the chord.
    for (through, end) in [
        (o, q(2.0, 0.0, 0.0)),
        (q(1.0, 1.0, 0.0), q(1.0, 1.0, 5e-7)),
        (q(1.0, 1.0, 0.0), x),
        (q(1.0, 0.0, 1e-6), q(2.0, 0.0, 0.0)),
        (q(1.5, 0.0, 1e-6), q(2.0, 0.0, 0.0)),
        (q(3.0, 0.0, 0.0), q(2.0, 0.0, 0.0)),
    ] {
        assert_eq!(
            check(vec![line(q(-1.0, 0.0, 0.0), x), arc(x, through, end)]),
            Some(PathError::DegenerateArc { segment: 1 }),
            "{through:?} {end:?}"
        );
    }
    let open = vec![line(o, x), line(x, q(1.0, 1.0, 0.0))];
    assert!(check(open).is_none());
    let closed = vec![
        line(o, x),
        line(x, q(1.0, 1.0, 0.0)),
        line(q(1.0, 1.0, 0.0), q(0.0, 0.0, 1e-6)),
    ];
    assert_eq!(check(closed), Some(PathError::Closed));
}
