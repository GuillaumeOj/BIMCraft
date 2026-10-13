//! Exact geometric predicates (GEO-03); the contract is in the crate docs.
//!
//! The arithmetic is Shewchuk's adaptive floating-point expansions (the `robust` crate): a fast
//! path for the common case, refined only when the result is too close to zero to trust. It uses
//! only `+`, `-`, `*` and `abs` on `f64`, so it is deterministic on every target (ENG-04).

use robust::{Coord, Coord3D};

use crate::{Point2, Point3};

/// Largest coordinate magnitude accepted, 2¹⁰⁰ (about 1.3 × 10³⁰).
///
/// `insphere` multiplies five coordinate differences; with this bound no product overflows.
pub const MAX_COORDINATE: f64 = 1_267_650_600_228_229_401_496_703_205_376.0;

/// Smallest non-zero coordinate magnitude accepted, 2⁻¹⁰⁰ (about 7.9 × 10⁻³¹).
///
/// With this bound the error terms of the exact stage never underflow.
pub const MIN_COORDINATE: f64 = 1.0 / MAX_COORDINATE;

/// The exact sign returned by a predicate. Each predicate documents what the sign means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Orientation {
    /// The determinant is strictly positive.
    Positive,
    /// The determinant is strictly negative.
    Negative,
    /// The determinant is exactly zero: the input is degenerate.
    Zero,
}

impl Orientation {
    /// Returns the opposite sign; [`Orientation::Zero`] stays zero.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
            Self::Zero => Self::Zero,
        }
    }

    fn of(determinant: f64) -> Self {
        // In-range input cannot produce NaN; one would mean the coordinate bounds are wrong, and
        // reading it as Zero would silently report a false degeneracy.
        debug_assert!(!determinant.is_nan(), "predicate determinant is NaN");
        if determinant > 0.0 {
            Self::Positive
        } else if determinant < 0.0 {
            Self::Negative
        } else {
            Self::Zero
        }
    }
}

/// Why a predicate refused its input.
///
/// The offending value is not carried: coordinates are model content and must stay out of logs
/// and crash reports (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum PredicateError {
    /// A coordinate is NaN or infinite.
    #[error("a coordinate is not a finite number")]
    NonFinite,
    /// A coordinate is non-zero and outside [`MIN_COORDINATE`, `MAX_COORDINATE`] in magnitude.
    #[error("a coordinate is outside the range where predicates are exact")]
    OutOfRange,
}

fn check(value: f64) -> Result<f64, PredicateError> {
    if !value.is_finite() {
        return Err(PredicateError::NonFinite);
    }
    let magnitude = value.abs();
    if value != 0.0 && !(MIN_COORDINATE..=MAX_COORDINATE).contains(&magnitude) {
        return Err(PredicateError::OutOfRange);
    }
    Ok(value)
}

fn coord(p: Point2) -> Result<Coord<f64>, PredicateError> {
    Ok(Coord {
        x: check(p.x)?,
        y: check(p.y)?,
    })
}

fn coord3(p: Point3) -> Result<Coord3D<f64>, PredicateError> {
    Ok(Coord3D {
        x: check(p.x)?,
        y: check(p.y)?,
        z: check(p.z)?,
    })
}

/// Tells on which side of the directed line `pa → pb` the point `pc` lies.
///
/// [`Orientation::Positive`] when `pa`, `pb`, `pc` turn counterclockwise (`pc` is on the left),
/// [`Orientation::Negative`] when they turn clockwise and [`Orientation::Zero`] when they are
/// collinear.
///
/// # Errors
///
/// [`PredicateError`] when a coordinate is not finite or outside the supported range.
pub fn orient2d(pa: Point2, pb: Point2, pc: Point2) -> Result<Orientation, PredicateError> {
    Ok(Orientation::of(robust::orient2d(
        coord(pa)?,
        coord(pb)?,
        coord(pc)?,
    )))
}

/// Tells on which side of the plane through `pa`, `pb`, `pc` the point `pd` lies.
///
/// [`Orientation::Positive`] when `pd` is below the plane, "below" meaning that `pa`, `pb`, `pc` appear
/// counterclockwise seen from above; [`Orientation::Negative`] when `pd` is above and
/// [`Orientation::Zero`] when the four points are coplanar.
///
/// # Errors
///
/// [`PredicateError`] when a coordinate is not finite or outside the supported range.
pub fn orient3d(
    pa: Point3,
    pb: Point3,
    pc: Point3,
    pd: Point3,
) -> Result<Orientation, PredicateError> {
    Ok(Orientation::of(robust::orient3d(
        coord3(pa)?,
        coord3(pb)?,
        coord3(pc)?,
        coord3(pd)?,
    )))
}

/// Tells whether `pd` lies inside the circle through `pa`, `pb` and `pc`.
///
/// With `pa`, `pb`, `pc` counterclockwise: [`Orientation::Positive`] when `pd` is inside,
/// [`Orientation::Negative`] when it is outside and [`Orientation::Zero`] when the four points are
/// cocircular. Clockwise `pa`, `pb`, `pc` reverse the sign.
///
/// # Errors
///
/// [`PredicateError`] when a coordinate is not finite or outside the supported range.
pub fn incircle(
    pa: Point2,
    pb: Point2,
    pc: Point2,
    pd: Point2,
) -> Result<Orientation, PredicateError> {
    Ok(Orientation::of(robust::incircle(
        coord(pa)?,
        coord(pb)?,
        coord(pc)?,
        coord(pd)?,
    )))
}

/// Tells whether `pe` lies inside the sphere through `pa`, `pb`, `pc` and `pd`.
///
/// With [`orient3d`]`(pa, pb, pc, pd)` positive: [`Orientation::Positive`] when `pe` is inside,
/// [`Orientation::Negative`] when it is outside and [`Orientation::Zero`] when the five points are
/// cospherical. A negative orientation of `pa`, `pb`, `pc`, `pd` reverses the sign.
///
/// # Errors
///
/// [`PredicateError`] when a coordinate is not finite or outside the supported range.
pub fn insphere(
    pa: Point3,
    pb: Point3,
    pc: Point3,
    pd: Point3,
    pe: Point3,
) -> Result<Orientation, PredicateError> {
    Ok(Orientation::of(robust::insphere(
        coord3(pa)?,
        coord3(pb)?,
        coord3(pc)?,
        coord3(pd)?,
        coord3(pe)?,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: Point2 = Point2::new(0.0, 0.0);
    const X: Point2 = Point2::new(1.0, 0.0);
    const Y: Point2 = Point2::new(0.0, 1.0);

    fn p3(x: f64, y: f64, z: f64) -> Point3 {
        Point3::new(x, y, z)
    }

    #[test]
    fn orient2d_follows_turn_direction() {
        assert_eq!(orient2d(O, X, Y), Ok(Orientation::Positive));
        assert_eq!(orient2d(O, Y, X), Ok(Orientation::Negative));
        assert_eq!(orient2d(O, X, Point2::new(2.0, 0.0)), Ok(Orientation::Zero));
    }

    #[test]
    fn orient2d_resolves_one_ulp_off_a_line() {
        let a = Point2::new(0.1, 0.1);
        let b = Point2::new(0.3, 0.3);
        let on = Point2::new(0.2, 0.2);
        let left = Point2::new(0.2, 0.2_f64.next_up());
        let right = Point2::new(0.2, 0.2_f64.next_down());
        // 0.1, 0.2 and 0.3 are not exactly representable, but x = y keeps the points collinear.
        assert_eq!(orient2d(a, b, on), Ok(Orientation::Zero));
        assert_eq!(orient2d(a, b, left), Ok(Orientation::Positive));
        assert_eq!(orient2d(a, b, right), Ok(Orientation::Negative));
    }

    #[test]
    fn orient3d_is_positive_below_counterclockwise_triangle() {
        let (a, b, c) = (p3(0.0, 0.0, 0.0), p3(1.0, 0.0, 0.0), p3(0.0, 1.0, 0.0));
        assert_eq!(
            orient3d(a, b, c, p3(0.2, 0.2, -1.0)),
            Ok(Orientation::Positive)
        );
        assert_eq!(
            orient3d(a, b, c, p3(0.2, 0.2, 1.0)),
            Ok(Orientation::Negative)
        );
        assert_eq!(orient3d(a, b, c, p3(5.0, -3.0, 0.0)), Ok(Orientation::Zero));
    }

    #[test]
    fn incircle_is_positive_inside_counterclockwise_circle() {
        let (a, b, c) = (X, Y, Point2::new(-1.0, 0.0));
        assert_eq!(incircle(a, b, c, O), Ok(Orientation::Positive));
        assert_eq!(
            incircle(a, b, c, Point2::new(2.0, 0.0)),
            Ok(Orientation::Negative)
        );
        assert_eq!(
            incircle(a, b, c, Point2::new(0.0, -1.0)),
            Ok(Orientation::Zero)
        );
        assert_eq!(incircle(c, b, a, O), Ok(Orientation::Negative));
    }

    #[test]
    fn insphere_is_positive_inside_positively_oriented_sphere() {
        let (a, b, c, d) = (
            p3(1.0, 0.0, 0.0),
            p3(0.0, 1.0, 0.0),
            p3(-1.0, 0.0, 0.0),
            p3(0.0, 0.0, -1.0),
        );
        assert_eq!(orient3d(a, b, c, d), Ok(Orientation::Positive));
        let centre = p3(0.0, 0.0, 0.0);
        assert_eq!(insphere(a, b, c, d, centre), Ok(Orientation::Positive));
        assert_eq!(
            insphere(a, b, c, d, p3(0.0, 0.0, 2.0)),
            Ok(Orientation::Negative)
        );
        assert_eq!(
            insphere(a, b, c, d, p3(0.0, 0.0, 1.0)),
            Ok(Orientation::Zero)
        );
    }

    #[test]
    fn reversed_swaps_strict_signs_and_keeps_zero() {
        assert_eq!(Orientation::Positive.reversed(), Orientation::Negative);
        assert_eq!(Orientation::Negative.reversed(), Orientation::Positive);
        assert_eq!(Orientation::Zero.reversed(), Orientation::Zero);
    }

    #[test]
    fn range_bounds_are_powers_of_two() {
        assert_eq!(MAX_COORDINATE.to_bits(), libm::scalbn(1.0, 100).to_bits());
        assert_eq!(MIN_COORDINATE.to_bits(), libm::scalbn(1.0, -100).to_bits());
    }

    #[test]
    fn accepts_zero_and_range_bounds() {
        for v in [0.0, -0.0, MIN_COORDINATE, -MAX_COORDINATE] {
            assert_eq!(check(v), Ok(v));
        }
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let result = orient2d(O, X, Point2::new(v, 0.0));
            assert_eq!(result, Err(PredicateError::NonFinite));
        }
        assert_eq!(
            PredicateError::NonFinite.to_string(),
            "a coordinate is not a finite number"
        );
    }

    #[test]
    fn rejects_coordinates_outside_exact_range() {
        let huge = MAX_COORDINATE * 2.0;
        let tiny = MIN_COORDINATE / 2.0;
        assert_eq!(check(huge), Err(PredicateError::OutOfRange));
        assert_eq!(check(-tiny), Err(PredicateError::OutOfRange));
        assert_eq!(
            PredicateError::OutOfRange.to_string(),
            "a coordinate is outside the range where predicates are exact"
        );
    }

    #[test]
    fn every_argument_is_validated() {
        let bad2 = Point2::new(0.0, f64::NAN);
        let bad3 = p3(0.0, 0.0, f64::NAN);
        let ok3 = p3(0.0, 0.0, 0.0);
        assert!(orient2d(O, O, bad2).is_err());
        assert!(incircle(O, O, O, bad2).is_err());
        assert!(orient3d(ok3, ok3, ok3, bad3).is_err());
        assert!(insphere(ok3, ok3, ok3, ok3, bad3).is_err());
        assert!(coord3(p3(f64::NAN, 0.0, 0.0)).is_err());
        assert!(coord3(p3(0.0, f64::NAN, 0.0)).is_err());
    }
}
