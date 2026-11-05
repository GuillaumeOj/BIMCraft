//! Ellipsoids, and conversions between geographic and geocentric coordinates (GEOREF-03).
//!
//! Angles are in radians and lengths in metres throughout. The parameters come in as plain
//! values: the EPSG registry fills them, so no ellipsoid is hard-coded here.

use std::f64::consts::{FRAC_PI_2, PI};

/// A point on or near an ellipsoid: geodetic longitude and latitude, in radians, and ellipsoidal
/// height, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeographicPoint {
    /// Longitude, in radians, positive east of the prime meridian.
    pub longitude: f64,
    /// Geodetic latitude, in radians, positive north, in [−π/2, π/2].
    pub latitude: f64,
    /// Height above the ellipsoid, in metres.
    pub height: f64,
}

impl GeographicPoint {
    /// The point at `longitude`, `latitude` (radians) and `height` (metres).
    #[must_use]
    pub const fn new(longitude: f64, latitude: f64, height: f64) -> Self {
        Self {
            longitude,
            latitude,
            height,
        }
    }

    /// The point, if its numbers are finite and its latitude is within [−π/2, π/2].
    pub(crate) fn checked(self) -> Result<Self, GeodesyError> {
        if !(self.longitude.is_finite() && self.latitude.is_finite() && self.height.is_finite()) {
            return Err(GeodesyError::NonFinite);
        }
        if self.latitude.abs() > FRAC_PI_2 {
            return Err(GeodesyError::LatitudeOutOfRange);
        }
        Ok(self)
    }

    /// The point, as [`GeographicPoint::checked`], and not at a pole, where the convergence and
    /// the scale of a projection are not defined.
    pub(crate) fn checked_off_pole(self) -> Result<Self, GeodesyError> {
        let point = self.checked()?;
        if is_pole(point.latitude) {
            return Err(GeodesyError::OutsideDomain);
        }
        Ok(point)
    }
}

/// A point in Earth-centred, Earth-fixed Cartesian coordinates, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeocentricPoint {
    /// X, towards the prime meridian on the equator, in metres.
    pub x: f64,
    /// Y, towards 90° east on the equator, in metres.
    pub y: f64,
    /// Z, towards the north pole, in metres.
    pub z: f64,
}

impl GeocentricPoint {
    /// The point at `x`, `y` and `z`.
    #[must_use]
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }
}

/// A parameter of an ellipsoid, a projection or a transformation, named in errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parameter {
    /// The ellipsoid's semi-major axis.
    SemiMajorAxis,
    /// The ellipsoid's inverse flattening.
    InverseFlattening,
    /// The latitude of the projection origin.
    LatitudeOfOrigin,
    /// The longitude of the projection origin (central meridian).
    LongitudeOfOrigin,
    /// The scale factor at the projection origin.
    ScaleFactor,
    /// The false easting or false northing.
    FalseOrigin,
    /// The standard parallels of a conic projection.
    StandardParallels,
    /// The translation of a Helmert transformation.
    Translation,
    /// The rotation of a Helmert transformation.
    Rotation,
    /// The scale difference of a Helmert transformation.
    ScaleDifference,
}

impl std::fmt::Display for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SemiMajorAxis => "semi-major axis",
            Self::InverseFlattening => "inverse flattening",
            Self::LatitudeOfOrigin => "latitude of origin",
            Self::LongitudeOfOrigin => "longitude of origin",
            Self::ScaleFactor => "scale factor",
            Self::FalseOrigin => "false easting or northing",
            Self::StandardParallels => "standard parallels",
            Self::Translation => "translation",
            Self::Rotation => "rotation",
            Self::ScaleDifference => "scale difference",
        })
    }
}

/// Why a coordinate operation failed. Messages never contain the values (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeodesyError {
    /// A parameter is missing its constraints: not finite, or out of its range.
    #[error("the {parameter} is invalid")]
    InvalidParameter {
        /// The parameter at fault.
        parameter: Parameter,
    },
    /// A coordinate is NaN or infinite.
    #[error("a coordinate is not a finite number")]
    NonFinite,
    /// A latitude is beyond a pole.
    #[error("the latitude is beyond a pole")]
    LatitudeOutOfRange,
    /// The point is outside the domain of the operation, where it has no finite result.
    #[error("the point is outside the domain of the operation")]
    OutsideDomain,
}

/// `value`, if it is finite, else an error naming `parameter`.
pub(crate) fn finite(value: f64, parameter: Parameter) -> Result<f64, GeodesyError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(GeodesyError::InvalidParameter { parameter })
    }
}

/// `value`, if it is finite and greater than zero, else an error naming `parameter`.
pub(crate) fn positive(value: f64, parameter: Parameter) -> Result<f64, GeodesyError> {
    if finite(value, parameter)? > 0.0 {
        Ok(value)
    } else {
        Err(GeodesyError::InvalidParameter { parameter })
    }
}

/// `value`, if it is a finite latitude within [−π/2, π/2], else an error naming `parameter`.
pub(crate) fn latitude(value: f64, parameter: Parameter) -> Result<f64, GeodesyError> {
    if finite(value, parameter)?.abs() <= FRAC_PI_2 {
        Ok(value)
    } else {
        Err(GeodesyError::InvalidParameter { parameter })
    }
}

/// How far, in radians, a value may stray past the edge of a domain through rounding: 10⁻¹²
/// rad, a few micrometres on the ground.
pub(crate) const DOMAIN_TOLERANCE: f64 = 1e-12;

/// Whether `latitude` is a pole, within [`DOMAIN_TOLERANCE`], where projections degenerate.
pub(crate) fn is_pole(latitude: f64) -> bool {
    (latitude.abs() - FRAC_PI_2).abs() < DOMAIN_TOLERANCE
}

/// `angle` brought into [−π, π], as PROJ does for longitudes.
pub(crate) fn wrap_angle(angle: f64) -> f64 {
    libm::remainder(angle, 2.0 * PI)
}

/// A reference ellipsoid of revolution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ellipsoid {
    semi_major_axis: f64,
    flattening: f64,
    eccentricity_squared: f64,
}

impl Ellipsoid {
    /// The ellipsoid with `semi_major_axis` (metres) and `inverse_flattening` (1/f). An infinite
    /// inverse flattening is a sphere.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::InvalidParameter`] when the semi-major axis is not finite and positive, or
    /// the inverse flattening is not greater than 1.
    pub fn new(semi_major_axis: f64, inverse_flattening: f64) -> Result<Self, GeodesyError> {
        let semi_major_axis = positive(semi_major_axis, Parameter::SemiMajorAxis)?;
        if inverse_flattening.is_nan() || inverse_flattening <= 1.0 {
            return Err(GeodesyError::InvalidParameter {
                parameter: Parameter::InverseFlattening,
            });
        }
        let flattening = 1.0 / inverse_flattening;
        Ok(Self {
            semi_major_axis,
            flattening,
            eccentricity_squared: flattening * (2.0 - flattening),
        })
    }

    /// The semi-major axis, in metres.
    #[must_use]
    pub const fn semi_major_axis(&self) -> f64 {
        self.semi_major_axis
    }

    /// The semi-minor axis, in metres.
    #[must_use]
    pub fn semi_minor_axis(&self) -> f64 {
        self.semi_major_axis * (1.0 - self.flattening)
    }

    /// The flattening f.
    #[must_use]
    pub const fn flattening(&self) -> f64 {
        self.flattening
    }

    /// The first eccentricity squared, e² = f·(2 − f).
    #[must_use]
    pub const fn eccentricity_squared(&self) -> f64 {
        self.eccentricity_squared
    }

    /// The first eccentricity e.
    #[must_use]
    pub fn eccentricity(&self) -> f64 {
        self.eccentricity_squared.sqrt()
    }

    /// The third flattening n = f / (2 − f).
    pub(crate) fn third_flattening(&self) -> f64 {
        self.flattening / (2.0 - self.flattening)
    }

    /// `sqrt(1 − e²·sin²φ)` for `sin_phi` = sin φ.
    pub(crate) fn w(&self, sin_phi: f64) -> f64 {
        (1.0 - self.eccentricity_squared * sin_phi * sin_phi).sqrt()
    }

    /// The geocentric coordinates of `point`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] or [`GeodesyError::LatitudeOutOfRange`] for an invalid point.
    pub fn to_geocentric(&self, point: GeographicPoint) -> Result<GeocentricPoint, GeodesyError> {
        let point = point.checked()?;
        let (sin_phi, cos_phi) = libm::sincos(point.latitude);
        let (sin_lambda, cos_lambda) = libm::sincos(point.longitude);
        let normal = self.semi_major_axis / self.w(sin_phi);
        let r = (normal + point.height) * cos_phi;
        Ok(GeocentricPoint::new(
            r * cos_lambda,
            r * sin_lambda,
            (normal * (1.0 - self.eccentricity_squared) + point.height) * sin_phi,
        ))
    }

    /// The geographic coordinates of `point`.
    ///
    /// Bowring's formula, iterated three times on the parametric latitude: converged to the
    /// rounding of `f64` for any point outside the Earth's core.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] for a non-finite coordinate, and
    /// [`GeodesyError::OutsideDomain`] for the centre of the ellipsoid, which has no latitude.
    pub fn to_geographic(&self, point: GeocentricPoint) -> Result<GeographicPoint, GeodesyError> {
        let GeocentricPoint { x, y, z } = point;
        if !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return Err(GeodesyError::NonFinite);
        }
        let p = libm::hypot(x, y);
        if p == 0.0 && z == 0.0 {
            return Err(GeodesyError::OutsideDomain);
        }
        let a = self.semi_major_axis;
        let b = self.semi_minor_axis();
        let e2 = self.eccentricity_squared;
        let second_e2 = e2 / (1.0 - e2);
        let mut beta = libm::atan2(z * a, p * b);
        let mut latitude = 0.0;
        let (mut sin_phi, mut cos_phi) = (0.0, 1.0);
        for step in 0..3 {
            if step > 0 {
                beta = libm::atan2(b * sin_phi, a * cos_phi);
            }
            let (sin_beta, cos_beta) = libm::sincos(beta);
            latitude = libm::atan2(
                z + second_e2 * b * sin_beta * sin_beta * sin_beta,
                p - e2 * a * cos_beta * cos_beta * cos_beta,
            );
            (sin_phi, cos_phi) = libm::sincos(latitude);
        }
        // A guard: rounding could only ever move the latitude a hair past a pole, where the
        // sine and cosine already computed stay correct to the last bit.
        let latitude = latitude.clamp(-FRAC_PI_2, FRAC_PI_2);
        Ok(GeographicPoint::new(
            libm::atan2(y, x),
            latitude,
            p * cos_phi + z * sin_phi - a * self.w(sin_phi),
        ))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{DEG, grs80, wgs84};

    #[test]
    fn derives_the_grs80_constants() {
        let grs80 = grs80();
        assert_eq!(grs80.semi_major_axis().to_bits(), 6_378_137.0_f64.to_bits());
        assert!((grs80.semi_minor_axis() - 6_356_752.314_140).abs() < 1e-6);
        assert!((grs80.eccentricity_squared() - 0.006_694_380_022_90).abs() < 1e-14);
        assert!((grs80.eccentricity() - 0.081_819_191_042_8).abs() < 1e-12);
        assert!((1.0 / grs80.flattening() - 298.257_222_101).abs() < 1e-9);
    }

    #[test]
    fn a_sphere_has_no_flattening() {
        let sphere = Ellipsoid::new(6_371_000.0, f64::INFINITY).unwrap();
        assert_eq!(sphere.flattening().to_bits(), 0.0_f64.to_bits());
        assert_eq!(
            sphere.semi_minor_axis().to_bits(),
            6_371_000.0_f64.to_bits()
        );
    }

    #[test]
    fn rejects_invalid_ellipsoids() {
        let semi_major = Err(GeodesyError::InvalidParameter {
            parameter: Parameter::SemiMajorAxis,
        });
        let flattening = Err(GeodesyError::InvalidParameter {
            parameter: Parameter::InverseFlattening,
        });
        assert_eq!(Ellipsoid::new(0.0, 298.0), semi_major);
        assert_eq!(Ellipsoid::new(-1.0, 298.0), semi_major);
        assert_eq!(Ellipsoid::new(f64::NAN, 298.0), semi_major);
        assert_eq!(Ellipsoid::new(f64::INFINITY, 298.0), semi_major);
        assert_eq!(Ellipsoid::new(6e6, 1.0), flattening);
        assert!(Ellipsoid::new(6e6, 1.5).is_ok());
        assert_eq!(Ellipsoid::new(6e6, f64::NAN), flattening);
        assert_eq!(Ellipsoid::new(6e6, -298.0), flattening);
    }

    /// EPSG Guidance Note 7-2 (IOGP Publication 373-7-2, 2019), §4.1.1 example: a point on
    /// WGS 84 at 53°48'33.82"N, 2°07'46.38"E, 73 m.
    #[test]
    fn converts_the_guidance_note_example() {
        let point = GeographicPoint::new(
            (2.0 + 7.0 / 60.0 + 46.38 / 3600.0) * DEG,
            (53.0 + 48.0 / 60.0 + 33.82 / 3600.0) * DEG,
            73.0,
        );
        let geocentric = wgs84().to_geocentric(point).unwrap();
        assert!((geocentric.x - 3_771_793.968).abs() < 1e-3);
        assert!((geocentric.y - 140_253.342).abs() < 1e-3);
        assert!((geocentric.z - 5_124_304.349).abs() < 1e-3);
        let back = wgs84().to_geographic(geocentric).unwrap();
        assert!((back.longitude - point.longitude).abs() < 1e-14);
        assert!((back.latitude - point.latitude).abs() < 1e-14);
        assert!((back.height - point.height).abs() < 1e-8);
    }

    #[test]
    fn converts_the_poles_and_the_equator() {
        let grs80 = grs80();
        let b = grs80.semi_minor_axis();
        for (z, latitude) in [(b + 10.0, FRAC_PI_2), (-(b + 10.0), -FRAC_PI_2)] {
            let pole = grs80
                .to_geographic(GeocentricPoint::new(0.0, 0.0, z))
                .unwrap();
            assert_eq!(pole.latitude.to_bits(), latitude.to_bits());
            assert!((pole.height - 10.0).abs() < 1e-8);
        }
        let equator = grs80
            .to_geographic(GeocentricPoint::new(0.0, -6_378_237.0, 0.0))
            .unwrap();
        assert_eq!(equator.latitude.to_bits(), 0.0_f64.to_bits());
        assert!((equator.longitude + FRAC_PI_2).abs() < 1e-15);
        assert!((equator.height - 100.0).abs() < 1e-8);
    }

    #[test]
    fn rejects_points_without_a_result() {
        let grs80 = grs80();
        assert_eq!(
            grs80.to_geographic(GeocentricPoint::new(0.0, 0.0, 0.0)),
            Err(GeodesyError::OutsideDomain)
        );
        assert_eq!(
            grs80.to_geographic(GeocentricPoint::new(f64::NAN, 0.0, 0.0)),
            Err(GeodesyError::NonFinite)
        );
        for point in [
            GeographicPoint::new(0.0, f64::INFINITY, 0.0),
            GeographicPoint::new(f64::NAN, 0.0, 0.0),
            GeographicPoint::new(0.0, 0.0, f64::NAN),
        ] {
            assert_eq!(grs80.to_geocentric(point), Err(GeodesyError::NonFinite));
        }
        assert_eq!(
            grs80.to_geocentric(GeographicPoint::new(0.0, 1.6, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange)
        );
    }

    #[test]
    fn wraps_angles_into_a_turn() {
        assert!((wrap_angle(3.0 * PI / 2.0) + FRAC_PI_2).abs() < 1e-15);
        assert!((wrap_angle(-3.0 * PI / 2.0) - FRAC_PI_2).abs() < 1e-15);
        assert_eq!(wrap_angle(0.25).to_bits(), 0.25_f64.to_bits());
    }

    #[test]
    fn names_every_parameter() {
        let names = [
            Parameter::SemiMajorAxis,
            Parameter::InverseFlattening,
            Parameter::LatitudeOfOrigin,
            Parameter::LongitudeOfOrigin,
            Parameter::ScaleFactor,
            Parameter::FalseOrigin,
            Parameter::StandardParallels,
            Parameter::Translation,
            Parameter::Rotation,
            Parameter::ScaleDifference,
        ]
        .map(|parameter| GeodesyError::InvalidParameter { parameter }.to_string());
        assert_eq!(names[0], "the semi-major axis is invalid");
        assert_eq!(names[9], "the scale difference is invalid");
        let mut unique = names.to_vec();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn geocentric_round_trips(
            longitude in -PI..PI,
            latitude in -FRAC_PI_2..FRAC_PI_2,
            height in -1e4..1e5_f64,
        ) {
            let point = GeographicPoint::new(longitude, latitude, height);
            let back = grs80().to_geographic(grs80().to_geocentric(point).unwrap()).unwrap();
            prop_assert!((back.latitude - latitude).abs() < 1e-14);
            prop_assert!(wrap_angle(back.longitude - longitude).abs() * libm::cos(latitude) < 1e-14);
            prop_assert!((back.height - height).abs() < 1e-7);
        }
    }
}
