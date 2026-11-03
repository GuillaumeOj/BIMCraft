//! Lambert Conformal Conic, with one standard parallel (EPSG method 9801) or two (EPSG method
//! 9802, used by Lambert-93 and the CC42–CC50 zones), on an ellipsoid. Formulas: EPSG Guidance
//! Note 7-2, §3.2.1; the inverse latitude by Newton's method on the isometric latitude, as in
//! Karney, "Transverse Mercator with an accuracy of a few nanometers", J. Geodesy 85 (2011).

use std::f64::consts::{FRAC_PI_2, PI};

use crate::ProjectedPoint;
use crate::geodesy::{
    DOMAIN_TOLERANCE, Ellipsoid, GeodesyError, GeographicPoint, Parameter, finite, is_pole,
    latitude, positive, wrap_angle,
};
use crate::projection::Factors;

/// The parameters of a Lambert Conformal Conic projection with two standard parallels, angles in
/// radians, lengths in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LambertConic2SpParameters {
    /// Latitude of false origin.
    pub latitude_of_origin: f64,
    /// Longitude of false origin.
    pub longitude_of_origin: f64,
    /// Latitude of first standard parallel.
    pub first_parallel: f64,
    /// Latitude of second standard parallel.
    pub second_parallel: f64,
    /// Easting at false origin.
    pub false_easting: f64,
    /// Northing at false origin.
    pub false_northing: f64,
}

/// The parameters of a Lambert Conformal Conic projection with one standard parallel, angles in
/// radians, lengths in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LambertConic1SpParameters {
    /// Latitude of natural origin, the standard parallel.
    pub latitude_of_origin: f64,
    /// Longitude of natural origin.
    pub longitude_of_origin: f64,
    /// Scale factor at natural origin.
    pub scale_factor: f64,
    /// False easting.
    pub false_easting: f64,
    /// False northing.
    pub false_northing: f64,
}

/// A Lambert Conformal Conic projection on an ellipsoid, ready to convert points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LambertConformalConic {
    ellipsoid: Ellipsoid,
    longitude_of_origin: f64,
    false_easting: f64,
    false_northing: f64,
    /// The cone constant n.
    n: f64,
    /// `a · k0 · F`: the radius ρ of a parallel is `c · exp(−n·ψ)`.
    c: f64,
    /// The radius of the parallel through the origin.
    rho0: f64,
}

/// `latitude`, if it is finite and strictly between the poles.
fn parallel(latitude: f64, parameter: Parameter) -> Result<f64, GeodesyError> {
    if finite(latitude, parameter)?.abs() < FRAC_PI_2 {
        Ok(latitude)
    } else {
        Err(GeodesyError::InvalidParameter { parameter })
    }
}

impl LambertConformalConic {
    /// The projection with two standard parallels, `parameters`, on `ellipsoid`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::InvalidParameter`] when a parameter is not finite, a standard parallel is
    /// at or beyond a pole, the parallels are symmetric about the equator (no cone), or the
    /// false origin is beyond a pole or at the pole away from the cone's apex.
    pub fn two_standard_parallels(
        ellipsoid: &Ellipsoid,
        parameters: &LambertConic2SpParameters,
    ) -> Result<Self, GeodesyError> {
        let e = ellipsoid.eccentricity();
        let lat1 = parallel(parameters.first_parallel, Parameter::StandardParallels)?;
        let lat2 = parallel(parameters.second_parallel, Parameter::StandardParallels)?;
        let (m1, iso1) = (m(ellipsoid, lat1), isometric_latitude(lat1, e));
        let n = if (lat1 - lat2).abs() < 1e-10 {
            libm::sin(lat1)
        } else {
            (libm::log(m1) - libm::log(m(ellipsoid, lat2))) / (isometric_latitude(lat2, e) - iso1)
        };
        if n.abs() < 1e-10 {
            return Err(GeodesyError::InvalidParameter {
                parameter: Parameter::StandardParallels,
            });
        }
        let c = ellipsoid.semi_major_axis() * m1 * libm::exp(n * iso1) / n;
        Self::build(
            ellipsoid,
            n,
            c,
            parameters.latitude_of_origin,
            parameters.longitude_of_origin,
            [parameters.false_easting, parameters.false_northing],
        )
    }

    /// The projection with one standard parallel, `parameters`, on `ellipsoid`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::InvalidParameter`] when a parameter is not finite, the latitude of origin
    /// is on the equator or at or beyond a pole, or the scale factor is not positive.
    pub fn one_standard_parallel(
        ellipsoid: &Ellipsoid,
        parameters: &LambertConic1SpParameters,
    ) -> Result<Self, GeodesyError> {
        let lat0 = parallel(parameters.latitude_of_origin, Parameter::LatitudeOfOrigin)?;
        let n = libm::sin(lat0);
        if n.abs() < 1e-10 {
            return Err(GeodesyError::InvalidParameter {
                parameter: Parameter::LatitudeOfOrigin,
            });
        }
        let k0 = positive(parameters.scale_factor, Parameter::ScaleFactor)?;
        let iso0 = isometric_latitude(lat0, ellipsoid.eccentricity());
        let c = ellipsoid.semi_major_axis() * k0 * m(ellipsoid, lat0) * libm::exp(n * iso0) / n;
        Self::build(
            ellipsoid,
            n,
            c,
            lat0,
            parameters.longitude_of_origin,
            [parameters.false_easting, parameters.false_northing],
        )
    }

    fn build(
        ellipsoid: &Ellipsoid,
        n: f64,
        c: f64,
        latitude_of_origin: f64,
        longitude_of_origin: f64,
        [false_easting, false_northing]: [f64; 2],
    ) -> Result<Self, GeodesyError> {
        let latitude_of_origin = latitude(latitude_of_origin, Parameter::LatitudeOfOrigin)?;
        // A false origin at the apex is valid (ρ0 = 0), as in PROJ; the other pole is not.
        let rho0 = radius(ellipsoid, n, c, latitude_of_origin).map_err(|_| {
            GeodesyError::InvalidParameter {
                parameter: Parameter::LatitudeOfOrigin,
            }
        })?;
        Ok(Self {
            ellipsoid: *ellipsoid,
            longitude_of_origin: finite(longitude_of_origin, Parameter::LongitudeOfOrigin)?,
            false_easting: finite(false_easting, Parameter::FalseOrigin)?,
            false_northing: finite(false_northing, Parameter::FalseOrigin)?,
            n,
            c,
            rho0,
        })
    }

    fn rho(&self, latitude: f64) -> Result<f64, GeodesyError> {
        radius(&self.ellipsoid, self.n, self.c, latitude)
    }

    /// The projected coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] or [`GeodesyError::LatitudeOutOfRange`] for an invalid point,
    /// and [`GeodesyError::OutsideDomain`] for the pole away from the cone's apex.
    pub fn forward(&self, point: GeographicPoint) -> Result<ProjectedPoint, GeodesyError> {
        let point = point.checked()?;
        let rho = self.rho(point.latitude)?;
        let theta = self.n * wrap_angle(point.longitude - self.longitude_of_origin);
        let (sin_theta, cos_theta) = libm::sincos(theta);
        Ok(ProjectedPoint::new(
            self.false_easting + rho * sin_theta,
            self.false_northing + self.rho0 - rho * cos_theta,
            point.height,
        ))
    }

    /// The geographic coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] for a non-finite coordinate, and
    /// [`GeodesyError::OutsideDomain`] for a point in the gap of the unrolled cone, which no
    /// longitude reaches.
    pub fn inverse(&self, point: ProjectedPoint) -> Result<GeographicPoint, GeodesyError> {
        if !point.is_finite() {
            return Err(GeodesyError::NonFinite);
        }
        let mut dx = point.eastings - self.false_easting;
        let mut dy = self.rho0 - (point.northings - self.false_northing);
        if self.n < 0.0 {
            dx = -dx;
            dy = -dy;
        }
        let rho = libm::hypot(dx, dy);
        if rho == 0.0 {
            return Ok(GeographicPoint::new(
                wrap_angle(self.longitude_of_origin),
                FRAC_PI_2.copysign(self.n),
                point.height,
            ));
        }
        let theta = libm::atan2(dx, dy);
        // The cone unrolls to a sector of half-angle π·|n| around the apex.
        if theta.abs() > PI * self.n.abs() + DOMAIN_TOLERANCE {
            return Err(GeodesyError::OutsideDomain);
        }
        let psi = -libm::log(rho / self.c.abs()) / self.n;
        Ok(GeographicPoint::new(
            wrap_angle(theta / self.n + self.longitude_of_origin),
            latitude_from_isometric(psi, self.ellipsoid.eccentricity()),
            point.height,
        ))
    }

    /// The meridian convergence and point scale factor at `point`.
    ///
    /// # Errors
    ///
    /// As [`LambertConformalConic::forward`], and [`GeodesyError::OutsideDomain`] at the poles,
    /// where the scale is not defined.
    pub fn factors(&self, point: GeographicPoint) -> Result<Factors, GeodesyError> {
        let point = point.checked_off_pole()?;
        let rho = self.rho(point.latitude)?;
        Ok(Factors {
            convergence: self.n * wrap_angle(point.longitude - self.longitude_of_origin),
            scale: rho * self.n
                / (self.ellipsoid.semi_major_axis() * m(&self.ellipsoid, point.latitude)),
        })
    }
}

/// The radius of the parallel through `latitude` on the cone with constant `n` and scale `c`:
/// zero at the apex, and an error at the other pole, which is at infinity.
fn radius(ellipsoid: &Ellipsoid, n: f64, c: f64, latitude: f64) -> Result<f64, GeodesyError> {
    if is_pole(latitude) {
        return if latitude * n > 0.0 {
            Ok(0.0)
        } else {
            Err(GeodesyError::OutsideDomain)
        };
    }
    Ok(c * libm::exp(-n * isometric_latitude(latitude, ellipsoid.eccentricity())))
}

/// `cos φ / sqrt(1 − e²·sin²φ)`: the radius of the parallel over the semi-major axis.
fn m(ellipsoid: &Ellipsoid, latitude: f64) -> f64 {
    let (sin_phi, cos_phi) = libm::sincos(latitude);
    cos_phi / ellipsoid.w(sin_phi)
}

/// The isometric latitude ψ = asinh(tan φ) − e·atanh(e·sin φ), so that EPSG's t = exp(−ψ).
fn isometric_latitude(latitude: f64, e: f64) -> f64 {
    libm::asinh(libm::tan(latitude)) - e * libm::atanh(e * libm::sin(latitude))
}

/// The geodetic latitude of the isometric latitude `psi`: Newton's method on tan φ, which
/// converges in at most five steps to the rounding of `f64` (Karney 2011, eqs. 19–21).
fn latitude_from_isometric(psi: f64, e: f64) -> f64 {
    let taup = libm::sinh(psi);
    let e2m = 1.0 - e * e;
    let tolerance = f64::EPSILON.sqrt() / 10.0;
    let mut tau = taup / e2m;
    // Within 10⁻⁸ rad of a pole (or ±∞), tan φ is the answer to the rounding of `f64`, and
    // squaring it would overflow: GeographicLib's `tauf` stops here too.
    if tau.is_nan() || tau.abs() >= 2.0 / f64::EPSILON.sqrt() {
        return libm::atan(tau);
    }
    for _ in 0..5 {
        let secant = libm::hypot(1.0, tau);
        let sigma = libm::sinh(e * libm::atanh(e * tau / secant));
        let taupa = libm::hypot(1.0, sigma) * tau - sigma * secant;
        let step =
            (taup - taupa) * (1.0 + e2m * tau * tau) / (e2m * secant * libm::hypot(1.0, taupa));
        tau += step;
        if step.abs() < tolerance * tau.abs().max(1.0) {
            break;
        }
    }
    libm::atan(tau)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{DEG, grs80, lambert93};

    fn dms(degrees: f64, minutes: f64, seconds: f64) -> f64 {
        (degrees + minutes / 60.0 + seconds / 3600.0) * DEG
    }

    /// NAD27 / Texas South Central (EPSG:32040), in US survey feet: the ellipsoid's semi-major
    /// axis is in feet, so the projection works in feet.
    fn texas_south_central() -> LambertConformalConic {
        let clarke_feet = Ellipsoid::new(20_925_832.16, 294.978_698_2).unwrap();
        LambertConformalConic::two_standard_parallels(
            &clarke_feet,
            &LambertConic2SpParameters {
                latitude_of_origin: dms(27.0, 50.0, 0.0),
                longitude_of_origin: -99.0 * DEG,
                first_parallel: dms(28.0, 23.0, 0.0),
                second_parallel: dms(30.0, 17.0, 0.0),
                false_easting: 2_000_000.0,
                false_northing: 0.0,
            },
        )
        .unwrap()
    }

    /// JAD69 / Jamaica National Grid (EPSG:24200).
    fn jamaica() -> LambertConformalConic {
        let clarke = Ellipsoid::new(6_378_206.4, 294.978_698_2).unwrap();
        LambertConformalConic::one_standard_parallel(
            &clarke,
            &LambertConic1SpParameters {
                latitude_of_origin: 18.0 * DEG,
                longitude_of_origin: -77.0 * DEG,
                scale_factor: 1.0,
                false_easting: 250_000.0,
                false_northing: 150_000.0,
            },
        )
        .unwrap()
    }

    /// A cone of the southern hemisphere, so n < 0.
    fn southern() -> LambertConformalConic {
        LambertConformalConic::one_standard_parallel(
            &grs80(),
            &LambertConic1SpParameters {
                latitude_of_origin: -30.0 * DEG,
                longitude_of_origin: 135.0 * DEG,
                scale_factor: 0.9999,
                false_easting: 1_000_000.0,
                false_northing: 5_000_000.0,
            },
        )
        .unwrap()
    }

    fn assert_round_trip(lcc: &LambertConformalConic, point: GeographicPoint) {
        let back = lcc.inverse(lcc.forward(point).unwrap()).unwrap();
        assert!(
            (back.latitude - point.latitude).abs() < 1e-13,
            "{back:?} {point:?}"
        );
        assert!(
            wrap_angle(back.longitude - point.longitude).abs() < 1e-13,
            "{back:?} {point:?}"
        );
    }

    /// EPSG Guidance Note 7-2 (IOGP Publication 373-7-2, 2019), §3.2.1.1 example (2SP).
    #[test]
    fn projects_the_two_parallel_guidance_note_example() {
        let lcc = texas_south_central();
        let point = GeographicPoint::new(-96.0 * DEG, 28.5 * DEG, 0.0);
        let projected = lcc.forward(point).unwrap();
        assert!(
            (projected.eastings - 2_963_503.91).abs() < 0.01,
            "{projected:?}"
        );
        assert!(
            (projected.northings - 254_759.80).abs() < 0.01,
            "{projected:?}"
        );
        assert_round_trip(&lcc, point);
    }

    /// EPSG Guidance Note 7-2 (IOGP Publication 373-7-2, 2019), §3.2.1.1 example (1SP).
    #[test]
    fn projects_the_one_parallel_guidance_note_example() {
        let lcc = jamaica();
        let point = GeographicPoint::new(-dms(76.0, 56.0, 37.26), dms(17.0, 55.0, 55.80), 0.0);
        let projected = lcc.forward(point).unwrap();
        assert!(
            (projected.eastings - 255_966.58).abs() < 0.01,
            "{projected:?}"
        );
        assert!(
            (projected.northings - 142_493.51).abs() < 0.01,
            "{projected:?}"
        );
        assert_round_trip(&lcc, point);
    }

    /// The cone constant n = 0.725 607 765 053 267 is IGN's (NTG 71, Lambert-93 constants).
    #[test]
    fn has_unit_scale_on_the_standard_parallels_of_lambert93() {
        let lcc = lambert93();
        for latitude in [44.0, 49.0] {
            let factors = lcc
                .factors(GeographicPoint::new(5.0 * DEG, latitude * DEG, 0.0))
                .unwrap();
            assert!((factors.scale - 1.0).abs() < 1e-14);
            assert!((factors.convergence - 0.725_607_765_053_267 * 2.0 * DEG).abs() < 1e-14);
        }
        let origin = lcc
            .forward(GeographicPoint::new(3.0 * DEG, 46.5 * DEG, 0.0))
            .unwrap();
        assert!((origin.eastings - 700_000.0).abs() < 1e-9);
        assert!((origin.northings - 6_600_000.0).abs() < 1e-9);
    }

    #[test]
    fn puts_the_apex_at_the_pole_the_cone_points_to() {
        for (lcc, apex, other) in [
            (lambert93(), FRAC_PI_2, -FRAC_PI_2),
            (southern(), -FRAC_PI_2, FRAC_PI_2),
        ] {
            let at_apex = lcc.forward(GeographicPoint::new(1.0, apex, 5.0)).unwrap();
            let pole = lcc.inverse(at_apex).unwrap();
            assert_eq!(pole.latitude.to_bits(), apex.to_bits());
            assert_eq!(pole.height.to_bits(), 5.0_f64.to_bits());
            assert_eq!(
                lcc.forward(GeographicPoint::new(1.0, other, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
            assert_eq!(
                lcc.factors(GeographicPoint::new(1.0, apex, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
        }
    }

    #[test]
    fn handles_the_southern_hemisphere() {
        let lcc = southern();
        let factors = lcc
            .factors(GeographicPoint::new(140.0 * DEG, -30.0 * DEG, 0.0))
            .unwrap();
        assert!((factors.scale - 0.9999).abs() < 1e-14);
        assert!(factors.convergence < 0.0);
        assert_round_trip(&lcc, GeographicPoint::new(140.0 * DEG, -35.0 * DEG, 0.0));
    }

    #[test]
    fn treats_equal_parallels_as_one() {
        let lcc = LambertConformalConic::two_standard_parallels(
            &grs80(),
            &LambertConic2SpParameters {
                latitude_of_origin: 45.0 * DEG,
                longitude_of_origin: 0.0,
                first_parallel: 45.0 * DEG,
                second_parallel: 45.0 * DEG,
                false_easting: 0.0,
                false_northing: 0.0,
            },
        )
        .unwrap();
        let factors = lcc
            .factors(GeographicPoint::new(0.0, 45.0 * DEG, 0.0))
            .unwrap();
        assert!((factors.scale - 1.0).abs() < 1e-14);
    }

    #[test]
    fn rejects_invalid_two_parallel_parameters() {
        let valid = LambertConic2SpParameters {
            latitude_of_origin: 46.5 * DEG,
            longitude_of_origin: 3.0 * DEG,
            first_parallel: 49.0 * DEG,
            second_parallel: 44.0 * DEG,
            false_easting: 700_000.0,
            false_northing: 6_600_000.0,
        };
        let error = |parameter| Err(GeodesyError::InvalidParameter { parameter });
        let two = [
            (
                LambertConic2SpParameters {
                    first_parallel: 30.0 * DEG,
                    second_parallel: -30.0 * DEG,
                    ..valid
                },
                Parameter::StandardParallels,
            ),
            (
                LambertConic2SpParameters {
                    first_parallel: FRAC_PI_2,
                    ..valid
                },
                Parameter::StandardParallels,
            ),
            (
                LambertConic2SpParameters {
                    second_parallel: f64::NAN,
                    ..valid
                },
                Parameter::StandardParallels,
            ),
            (
                LambertConic2SpParameters {
                    latitude_of_origin: -FRAC_PI_2,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                LambertConic2SpParameters {
                    latitude_of_origin: 2.0,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                LambertConic2SpParameters {
                    latitude_of_origin: f64::NAN,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                LambertConic2SpParameters {
                    longitude_of_origin: f64::NAN,
                    ..valid
                },
                Parameter::LongitudeOfOrigin,
            ),
            (
                LambertConic2SpParameters {
                    false_easting: f64::INFINITY,
                    ..valid
                },
                Parameter::FalseOrigin,
            ),
            (
                LambertConic2SpParameters {
                    false_northing: f64::NAN,
                    ..valid
                },
                Parameter::FalseOrigin,
            ),
        ];
        for (parameters, parameter) in two {
            assert_eq!(
                LambertConformalConic::two_standard_parallels(&grs80(), &parameters),
                error(parameter)
            );
        }
    }

    #[test]
    fn rejects_invalid_one_parallel_parameters() {
        let error = |parameter| Err(GeodesyError::InvalidParameter { parameter });
        let valid = LambertConic1SpParameters {
            latitude_of_origin: 18.0 * DEG,
            longitude_of_origin: 0.0,
            scale_factor: 1.0,
            false_easting: 0.0,
            false_northing: 0.0,
        };
        let one = [
            (
                LambertConic1SpParameters {
                    latitude_of_origin: 0.0,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                LambertConic1SpParameters {
                    latitude_of_origin: 2.0,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                LambertConic1SpParameters {
                    scale_factor: -1.0,
                    ..valid
                },
                Parameter::ScaleFactor,
            ),
            (
                LambertConic1SpParameters {
                    scale_factor: f64::NAN,
                    ..valid
                },
                Parameter::ScaleFactor,
            ),
        ];
        for (parameters, parameter) in one {
            assert_eq!(
                LambertConformalConic::one_standard_parallel(&grs80(), &parameters),
                error(parameter)
            );
        }
    }

    #[test]
    fn rejects_invalid_points() {
        let lcc = lambert93();
        assert_eq!(
            lcc.forward(GeographicPoint::new(f64::NAN, 0.8, 0.0)),
            Err(GeodesyError::NonFinite)
        );
        assert_eq!(
            lcc.factors(GeographicPoint::new(0.0, 2.0, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange)
        );
        assert_eq!(
            lcc.inverse(ProjectedPoint::new(0.0, f64::INFINITY, 0.0)),
            Err(GeodesyError::NonFinite)
        );
    }

    /// Regression: a point very far from the apex is near the pole at infinity, not NaN.
    #[test]
    fn inverts_points_far_from_the_apex() {
        let lcc = lambert93();
        for eastings in [1e130, 1e300] {
            let point = lcc
                .inverse(ProjectedPoint::new(eastings, 0.0, 0.0))
                .unwrap();
            assert!((point.latitude + FRAC_PI_2).abs() < 1e-8, "{point:?}");
        }
    }

    /// Regression: the unrolled cone covers π·n either side of its axis; beyond it, no
    /// longitude projects there.
    #[test]
    fn rejects_points_in_the_gap_of_the_cone() {
        let lcc = lambert93();
        let apex = lcc
            .forward(GeographicPoint::new(0.0, FRAC_PI_2, 0.0))
            .unwrap();
        let beyond_apex = ProjectedPoint::new(apex.eastings, apex.northings + 1e6, 0.0);
        assert_eq!(lcc.inverse(beyond_apex), Err(GeodesyError::OutsideDomain));
        let antimeridian = lcc
            .forward(GeographicPoint::new(-177.0 * DEG, 60.0 * DEG, 0.0))
            .unwrap();
        assert!(lcc.inverse(antimeridian).is_ok());
    }

    /// Regression: a false origin at the apex is valid (ρ0 = 0), at the other pole it is not.
    #[test]
    fn accepts_a_false_origin_at_the_apex() {
        let parameters = LambertConic2SpParameters {
            latitude_of_origin: FRAC_PI_2,
            longitude_of_origin: 190.0 * DEG,
            first_parallel: 60.0 * DEG,
            second_parallel: 70.0 * DEG,
            false_easting: 1e6,
            false_northing: 2e6,
        };
        let lcc = LambertConformalConic::two_standard_parallels(&grs80(), &parameters).unwrap();
        let apex = lcc
            .forward(GeographicPoint::new(0.3, FRAC_PI_2, 0.0))
            .unwrap();
        assert!((apex.eastings - 1e6).abs() < 1e-6 && (apex.northings - 2e6).abs() < 1e-6);
        // The apex keeps the longitude of origin, wrapped like every other result.
        let back = lcc.inverse(ProjectedPoint::new(1e6, 2e6, 0.0)).unwrap();
        assert!((back.longitude + 170.0 * DEG).abs() < 1e-12, "{back:?}");
        assert_eq!(
            LambertConformalConic::two_standard_parallels(
                &grs80(),
                &LambertConic2SpParameters {
                    latitude_of_origin: -FRAC_PI_2,
                    ..parameters
                }
            ),
            Err(GeodesyError::InvalidParameter {
                parameter: Parameter::LatitudeOfOrigin
            })
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn round_trips(longitude in -60.0..66.0_f64, latitude in -60.0..89.9_f64) {
            assert_round_trip(&lambert93(), GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0));
        }

        #[test]
        fn round_trips_in_the_south(longitude in 90.0..180.0_f64, latitude in -89.9..60.0_f64) {
            assert_round_trip(&southern(), GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0));
        }

        /// On the central meridian, northings grow with latitude.
        #[test]
        fn is_monotonic(a in 30.0..60.0_f64, b in 30.0..60.0_f64) {
            let lcc = lambert93();
            let northing = |latitude: f64| {
                lcc.forward(GeographicPoint::new(3.0 * DEG, latitude * DEG, 0.0)).unwrap().northings
            };
            prop_assert_eq!(a < b, northing(a) < northing(b));
        }
    }
}
