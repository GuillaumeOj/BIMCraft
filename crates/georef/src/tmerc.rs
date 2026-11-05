//! Transverse Mercator (EPSG method 9807, and UTM), with the extended Krüger series of
//! Poder and Engsager to sixth order in the third flattening n: the algorithm PROJ's `tmerc` uses
//! by default. It stays accurate to well under a millimetre within about 3 900 km of the central
//! meridian.
//!
//! The geodetic latitude goes to the conformal (Gaussian) latitude, then to the Gauss–Schreiber
//! coordinates on the conformal sphere (ξ′, η′), then through the Krüger series to the
//! ellipsoidal coordinates (ξ, η). References: Karney, "Transverse Mercator with an accuracy of a
//! few nanometers", J. Geodesy 85 (2011); Engsager and Poder, "A highly accurate world wide
//! algorithm for the transverse Mercator mapping (almost)", ICC 2007.

use std::f64::consts::PI;

use crate::ProjectedPoint;
use crate::geodesy::{
    DOMAIN_TOLERANCE, Ellipsoid, GeodesyError, GeographicPoint, Parameter, finite, latitude,
    positive, wrap_angle,
};
use crate::projection::Factors;

/// The order of the series.
const ORDER: usize = 6;

type Series = [f64; ORDER];

/// The largest |η| PROJ accepts: beyond it the series no longer converge.
const MAX_ETA: f64 = 2.623_395_162_778;

/// The parameters of a Transverse Mercator projection, angles in radians, lengths in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransverseMercatorParameters {
    /// Latitude of natural origin.
    pub latitude_of_origin: f64,
    /// Longitude of natural origin (the central meridian).
    pub longitude_of_origin: f64,
    /// Scale factor at natural origin.
    pub scale_factor: f64,
    /// False easting.
    pub false_easting: f64,
    /// False northing.
    pub false_northing: f64,
}

/// A Transverse Mercator projection on an ellipsoid, ready to convert points.
#[derive(Clone, Debug, PartialEq)]
pub struct TransverseMercator {
    ellipsoid: Ellipsoid,
    longitude_of_origin: f64,
    false_easting: f64,
    false_northing: f64,
    /// Ratio of the point scale to the series scale: `k0 · A / a`.
    scale: f64,
    /// Scaled rectifying radius `k0 · A`, in metres.
    qn: f64,
    /// Northing of the origin latitude, subtracted so the origin has the false northing.
    zb: f64,
    /// Geodetic to conformal latitude.
    cbg: Series,
    /// Conformal to geodetic latitude.
    cgb: Series,
    /// Ellipsoidal to spherical (ξ, η) → (ξ′, η′).
    utg: Series,
    /// Spherical to ellipsoidal (ξ′, η′) → (ξ, η): Krüger's α.
    gtu: Series,
}

impl TransverseMercator {
    /// The smallest inverse flattening of the ellipsoid. The series, to sixth order in the third
    /// flattening, lose their accuracy on flatter ellipsoids; every Earth ellipsoid in use has
    /// 1/f near 300.
    pub const MIN_INVERSE_FLATTENING: f64 = 50.0;

    /// The projection with `parameters` on `ellipsoid`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::InvalidParameter`] when a parameter is not finite, the latitude of origin
    /// is beyond a pole, the scale factor is not positive, or the ellipsoid is flatter than
    /// [`TransverseMercator::MIN_INVERSE_FLATTENING`] allows.
    pub fn new(
        ellipsoid: &Ellipsoid,
        parameters: &TransverseMercatorParameters,
    ) -> Result<Self, GeodesyError> {
        if 1.0 / ellipsoid.flattening() < Self::MIN_INVERSE_FLATTENING {
            return Err(GeodesyError::InvalidParameter {
                parameter: Parameter::InverseFlattening,
            });
        }
        let phi0 = latitude(parameters.latitude_of_origin, Parameter::LatitudeOfOrigin)?;
        let k0 = positive(parameters.scale_factor, Parameter::ScaleFactor)?;
        let n = ellipsoid.third_flattening();
        let (cbg, cgb) = latitude_series(n);
        let (utg, gtu) = krueger_series(n);
        let n2 = n * n;
        let rectifying = (1.0 + n2 * (1.0 / 4.0 + n2 * (1.0 / 64.0 + n2 / 256.0))) / (1.0 + n);
        let qn = k0 * ellipsoid.semi_major_axis() * rectifying;
        let z = gatg(&cbg, phi0);
        Ok(Self {
            ellipsoid: *ellipsoid,
            longitude_of_origin: finite(
                parameters.longitude_of_origin,
                Parameter::LongitudeOfOrigin,
            )?,
            false_easting: finite(parameters.false_easting, Parameter::FalseOrigin)?,
            false_northing: finite(parameters.false_northing, Parameter::FalseOrigin)?,
            scale: k0 * rectifying,
            qn,
            zb: -qn * (z + clenshaw_sin(&gtu, 2.0 * z)),
            cbg,
            cgb,
            utg,
            gtu,
        })
    }

    /// The Gauss–Schreiber coordinates (ξ′, η′) of `point` on the conformal sphere, with the
    /// conformal latitude and the longitude from the central meridian.
    fn spherical(&self, point: GeographicPoint) -> Spherical {
        let lambda = wrap_angle(point.longitude - self.longitude_of_origin);
        let chi = gatg(&self.cbg, point.latitude);
        let (sin_chi, cos_chi) = libm::sincos(chi);
        let (sin_lambda, cos_lambda) = libm::sincos(lambda);
        let xi = libm::atan2(sin_chi, cos_lambda * cos_chi);
        let eta = libm::asinh(libm::tan(libm::atan2(
            sin_lambda * cos_chi,
            libm::hypot(sin_chi, cos_chi * cos_lambda),
        )));
        Spherical {
            sin_chi,
            cos_chi,
            sin_lambda,
            cos_lambda,
            xi,
            eta,
        }
    }

    /// The ellipsoidal coordinates (ξ, η) of `spherical`, through the Krüger series, if the
    /// series reach them.
    fn ellipsoidal(&self, spherical: &Spherical) -> Result<(f64, f64), GeodesyError> {
        let Spherical { xi, eta, .. } = *spherical;
        let (d_xi, d_eta) = clenshaw_complex_sin(&self.gtu, 2.0 * xi, 2.0 * eta);
        let (xi, eta) = (xi + d_xi, eta + d_eta);
        if eta.is_nan() || eta.abs() > MAX_ETA {
            return Err(GeodesyError::OutsideDomain);
        }
        Ok((xi, eta))
    }

    /// The projected coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] or [`GeodesyError::LatitudeOutOfRange`] for an invalid point,
    /// and [`GeodesyError::OutsideDomain`] when the point is too close to one of the two points
    /// of the equator 90° from the central meridian, where the projection goes to infinity and
    /// the series no longer converge.
    pub fn forward(&self, point: GeographicPoint) -> Result<ProjectedPoint, GeodesyError> {
        let point = point.checked()?;
        let (xi, eta) = self.ellipsoidal(&self.spherical(point))?;
        Ok(ProjectedPoint::new(
            self.qn * eta + self.false_easting,
            self.qn * xi + self.zb + self.false_northing,
            point.height,
        ))
    }

    /// The geographic coordinates of `point`; the height passes through unchanged.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::NonFinite`] for a non-finite coordinate, and
    /// [`GeodesyError::OutsideDomain`] when the point is beyond the reach of the series or
    /// beyond the antimeridian of the central meridian: the series are periodic in northing, so
    /// such a point would otherwise come back as a wrong but plausible one.
    pub fn inverse(&self, point: ProjectedPoint) -> Result<GeographicPoint, GeodesyError> {
        if !point.is_finite() {
            return Err(GeodesyError::NonFinite);
        }
        let xi = (point.northings - self.false_northing - self.zb) / self.qn;
        let eta = (point.eastings - self.false_easting) / self.qn;
        // ξ is ±π/2 at the poles and ±π where the antimeridian crosses the equator.
        if eta.abs() > MAX_ETA || xi.abs() > PI + DOMAIN_TOLERANCE {
            return Err(GeodesyError::OutsideDomain);
        }
        let (d_xi, d_eta) = clenshaw_complex_sin(&self.utg, 2.0 * xi, 2.0 * eta);
        let (xi, eta) = (xi + d_xi, libm::atan(libm::sinh(eta + d_eta)));
        let (sin_xi, cos_xi) = libm::sincos(xi);
        let (sin_eta, cos_eta) = libm::sincos(eta);
        let lambda = libm::atan2(sin_eta, cos_eta * cos_xi);
        let chi = libm::atan2(sin_xi * cos_eta, libm::hypot(sin_eta, cos_eta * cos_xi));
        Ok(GeographicPoint::new(
            wrap_angle(lambda + self.longitude_of_origin),
            gatg(&self.cgb, chi),
            point.height,
        ))
    }

    /// The meridian convergence and point scale factor at `point` (Karney 2011, §2 and eq. 25).
    ///
    /// # Errors
    ///
    /// As [`TransverseMercator::forward`], and [`GeodesyError::OutsideDomain`] at the poles,
    /// where the convergence is not defined.
    pub fn factors(&self, point: GeographicPoint) -> Result<Factors, GeodesyError> {
        let point = point.checked_off_pole()?;
        let spherical = self.spherical(point);
        self.ellipsoidal(&spherical)?;
        let Spherical {
            sin_chi,
            cos_chi,
            sin_lambda,
            cos_lambda,
            xi,
            eta,
        } = spherical;
        let (sin_lat, cos_lat) = libm::sincos(point.latitude);
        // On the conformal sphere.
        let gamma_sphere = libm::atan2(sin_lambda * sin_chi, cos_lambda);
        let k_sphere = self.ellipsoid.w(sin_lat) * cos_chi
            / (cos_lat * (1.0 - cos_chi * cos_chi * sin_lambda * sin_lambda).sqrt());
        // The derivative of the Krüger series, dζ/dζ′ = p − i·q.
        let (mut p, mut q) = (1.0, 0.0);
        for (j, alpha) in (1_u8..).zip(self.gtu) {
            let order = 2.0 * f64::from(j);
            let (sin_xi, cos_xi) = libm::sincos(order * xi);
            p += order * alpha * cos_xi * libm::cosh(order * eta);
            q += order * alpha * sin_xi * libm::sinh(order * eta);
        }
        Ok(Factors {
            convergence: gamma_sphere + libm::atan2(q, p),
            scale: self.scale * k_sphere * libm::hypot(p, q),
        })
    }
}

/// A point on the conformal sphere, see [`TransverseMercator::spherical`].
#[derive(Clone, Copy)]
struct Spherical {
    /// Sine and cosine of the conformal latitude χ.
    sin_chi: f64,
    cos_chi: f64,
    /// Sine and cosine of the longitude from the central meridian λ.
    sin_lambda: f64,
    cos_lambda: f64,
    xi: f64,
    eta: f64,
}

/// The series between geodetic and conformal latitude (`cbg`: geodetic to conformal, `cgb`:
/// back), to n⁶.
fn latitude_series(n: f64) -> (Series, Series) {
    let n2 = n * n;
    let n3 = n2 * n;
    let n4 = n3 * n;
    let n5 = n4 * n;
    let n6 = n5 * n;
    let cbg = [
        n * (-2.0
            + n * (2.0 / 3.0
                + n * (4.0 / 3.0
                    + n * (-82.0 / 45.0 + n * (32.0 / 45.0 + n * (4642.0 / 4725.0)))))),
        n2 * (5.0 / 3.0
            + n * (-16.0 / 15.0 + n * (-13.0 / 9.0 + n * (904.0 / 315.0 + n * (-1522.0 / 945.0))))),
        n3 * (-26.0 / 15.0 + n * (34.0 / 21.0 + n * (8.0 / 5.0 + n * (-12686.0 / 2835.0)))),
        n4 * (1237.0 / 630.0 + n * (-12.0 / 5.0 + n * (-24832.0 / 14175.0))),
        n5 * (-734.0 / 315.0 + n * (109_598.0 / 31185.0)),
        n6 * (444_337.0 / 155_925.0),
    ];
    let cgb = [
        n * (2.0
            + n * (-2.0 / 3.0
                + n * (-2.0 + n * (116.0 / 45.0 + n * (26.0 / 45.0 + n * (-2854.0 / 675.0)))))),
        n2 * (7.0 / 3.0
            + n * (-8.0 / 5.0 + n * (-227.0 / 45.0 + n * (2704.0 / 315.0 + n * (2323.0 / 945.0))))),
        n3 * (56.0 / 15.0 + n * (-136.0 / 35.0 + n * (-1262.0 / 105.0 + n * (73814.0 / 2835.0)))),
        n4 * (4279.0 / 630.0 + n * (-332.0 / 35.0 + n * (-399_572.0 / 14175.0))),
        n5 * (4174.0 / 315.0 + n * (-144_838.0 / 6237.0)),
        n6 * (601_676.0 / 22275.0),
    ];
    (cbg, cgb)
}

/// Krüger's series between ellipsoidal and spherical Gauss–Schreiber coordinates (`utg`:
/// ellipsoidal to spherical, Karney's β with the sign flipped; `gtu`: spherical to ellipsoidal,
/// Karney's α), to n⁶.
fn krueger_series(n: f64) -> (Series, Series) {
    let n2 = n * n;
    let n3 = n2 * n;
    let n4 = n3 * n;
    let n5 = n4 * n;
    let n6 = n5 * n;
    let utg = [
        n * (-0.5
            + n * (2.0 / 3.0
                + n * (-37.0 / 96.0
                    + n * (1.0 / 360.0 + n * (81.0 / 512.0 + n * (-96199.0 / 604_800.0)))))),
        n2 * (-1.0 / 48.0
            + n * (-1.0 / 15.0
                + n * (437.0 / 1440.0 + n * (-46.0 / 105.0 + n * (1_118_711.0 / 3_870_720.0))))),
        n3 * (-17.0 / 480.0 + n * (37.0 / 840.0 + n * (209.0 / 4480.0 + n * (-5569.0 / 90720.0)))),
        n4 * (-4397.0 / 161_280.0 + n * (11.0 / 504.0 + n * (830_251.0 / 7_257_600.0))),
        n5 * (-4583.0 / 161_280.0 + n * (108_847.0 / 3_991_680.0)),
        n6 * (-20_648_693.0 / 638_668_800.0),
    ];
    let gtu = [
        n * (0.5
            + n * (-2.0 / 3.0
                + n * (5.0 / 16.0
                    + n * (41.0 / 180.0 + n * (-127.0 / 288.0 + n * (7891.0 / 37800.0)))))),
        n2 * (13.0 / 48.0
            + n * (-3.0 / 5.0
                + n * (557.0 / 1440.0 + n * (281.0 / 630.0 + n * (-1_983_433.0 / 1_935_360.0))))),
        n3 * (61.0 / 240.0
            + n * (-103.0 / 140.0 + n * (15061.0 / 26880.0 + n * (167_603.0 / 181_440.0)))),
        n4 * (49561.0 / 161_280.0 + n * (-179.0 / 168.0 + n * (6_601_661.0 / 7_257_600.0))),
        n5 * (34729.0 / 80640.0 + n * (-3_418_889.0 / 1_995_840.0)),
        n6 * (212_378_941.0 / 319_334_400.0),
    ];
    (utg, gtu)
}

/// `b + Σ p[j]·sin(2(j+1)·b)`, by Clenshaw summation: a latitude converted by the series `p`.
fn gatg(p: &Series, b: f64) -> f64 {
    b + clenshaw_sin(p, 2.0 * b)
}

/// `Σ a[j]·sin((j+1)·arg)`, by Clenshaw summation.
fn clenshaw_sin(a: &Series, arg: f64) -> f64 {
    let (sin_arg, cos_arg) = libm::sincos(arg);
    let r = 2.0 * cos_arg;
    let (mut hr, mut hr1) = (0.0, 0.0);
    for &c in a.iter().rev() {
        let hr2 = hr1;
        hr1 = hr;
        hr = -hr2 + r * hr1 + c;
    }
    sin_arg * hr
}

/// `Σ a[j]·sin((j+1)·(arg_r + i·arg_i))`, by complex Clenshaw summation: (real, imaginary).
fn clenshaw_complex_sin(a: &Series, arg_r: f64, arg_i: f64) -> (f64, f64) {
    let (sin_r, cos_r) = libm::sincos(arg_r);
    let (sinh_i, cosh_i) = (libm::sinh(arg_i), libm::cosh(arg_i));
    let r = 2.0 * cos_r * cosh_i;
    let i = -2.0 * sin_r * sinh_i;
    let (mut hr, mut hr1, mut hi, mut hi1) = (0.0, 0.0, 0.0, 0.0);
    for &c in a.iter().rev() {
        let (hr2, hi2) = (hr1, hi1);
        hr1 = hr;
        hi1 = hi;
        hr = -hr2 + r * hr1 - i * hi1 + c;
        hi = -hi2 + i * hr1 + r * hi1;
    }
    let r = sin_r * cosh_i;
    let i = cos_r * sinh_i;
    (r * hr - i * hi, r * hi + i * hr)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{DEG, grs80, utm, wgs84};

    fn airy() -> Ellipsoid {
        Ellipsoid::new(6_377_563.396, 299.324_964_6).unwrap()
    }

    /// OSGB 1936 / British National Grid (EPSG:27700).
    fn british_national_grid() -> TransverseMercator {
        TransverseMercator::new(
            &airy(),
            &TransverseMercatorParameters {
                latitude_of_origin: 49.0 * DEG,
                longitude_of_origin: -2.0 * DEG,
                scale_factor: 0.999_601_271_7,
                false_easting: 400_000.0,
                false_northing: -100_000.0,
            },
        )
        .unwrap()
    }

    /// The distance on the ground, in metres, between two nearby geographic points.
    fn ground_distance(a: GeographicPoint, b: GeographicPoint) -> f64 {
        let dlat = (a.latitude - b.latitude) * 6.4e6;
        let dlon = wrap_angle(a.longitude - b.longitude) * libm::cos(a.latitude) * 6.4e6;
        libm::hypot(dlat, dlon)
    }

    /// EPSG Guidance Note 7-2 (IOGP Publication 373-7-2, 2019), §3.5.3.1 example.
    #[test]
    fn projects_the_guidance_note_example() {
        let tm = british_national_grid();
        let point = GeographicPoint::new(0.5 * DEG, 50.5 * DEG, 12.0);
        let projected = tm.forward(point).unwrap();
        assert!(
            (projected.eastings - 577_274.99).abs() < 0.01,
            "{projected:?}"
        );
        assert!(
            (projected.northings - 69_740.50).abs() < 0.01,
            "{projected:?}"
        );
        assert_eq!(projected.height.to_bits(), 12.0_f64.to_bits());
        let back = tm.inverse(projected).unwrap();
        assert!(ground_distance(back, point) < 1e-6);
        assert_eq!(back.height.to_bits(), 12.0_f64.to_bits());
    }

    #[test]
    fn puts_the_origin_at_the_false_origin() {
        let tm = british_national_grid();
        let origin = tm
            .forward(GeographicPoint::new(-2.0 * DEG, 49.0 * DEG, 0.0))
            .unwrap();
        assert!((origin.eastings - 400_000.0).abs() < 1e-9);
        assert!((origin.northings + 100_000.0).abs() < 1e-9);
        let south = utm(&wgs84(), 33, true)
            .forward(GeographicPoint::new(15.0 * DEG, 0.0, 0.0))
            .unwrap();
        assert!((south.eastings - 500_000.0).abs() < 1e-9);
        assert!((south.northings - 10_000_000.0).abs() < 1e-9);
    }

    #[test]
    fn projects_the_central_meridian_to_the_scaled_meridian_arc() {
        // GRS 80 meridian arc from the equator to 45°N: 4 984 944.378 m (Karney's GeodSolve).
        let north = utm(&grs80(), 31, false)
            .forward(GeographicPoint::new(3.0 * DEG, 45.0 * DEG, 0.0))
            .unwrap();
        assert!((north.northings - 0.9996 * 4_984_944.378).abs() < 1e-3);
        assert!((north.eastings - 500_000.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_parameters() {
        let valid = TransverseMercatorParameters {
            latitude_of_origin: 0.0,
            longitude_of_origin: 0.0,
            scale_factor: 1.0,
            false_easting: 0.0,
            false_northing: 0.0,
        };
        let error = |parameter| Err(GeodesyError::InvalidParameter { parameter });
        let cases = [
            (
                TransverseMercatorParameters {
                    latitude_of_origin: 1.6,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                TransverseMercatorParameters {
                    latitude_of_origin: f64::NAN,
                    ..valid
                },
                Parameter::LatitudeOfOrigin,
            ),
            (
                TransverseMercatorParameters {
                    longitude_of_origin: f64::NAN,
                    ..valid
                },
                Parameter::LongitudeOfOrigin,
            ),
            (
                TransverseMercatorParameters {
                    scale_factor: 0.0,
                    ..valid
                },
                Parameter::ScaleFactor,
            ),
            (
                TransverseMercatorParameters {
                    scale_factor: f64::INFINITY,
                    ..valid
                },
                Parameter::ScaleFactor,
            ),
            (
                TransverseMercatorParameters {
                    false_easting: f64::NAN,
                    ..valid
                },
                Parameter::FalseOrigin,
            ),
            (
                TransverseMercatorParameters {
                    false_northing: f64::NAN,
                    ..valid
                },
                Parameter::FalseOrigin,
            ),
        ];
        assert_eq!(
            TransverseMercator::new(&Ellipsoid::new(6e6, 1.5).unwrap(), &valid),
            error(Parameter::InverseFlattening)
        );
        for (parameters, parameter) in cases {
            assert_eq!(
                TransverseMercator::new(&grs80(), &parameters),
                error(parameter)
            );
        }
    }

    #[test]
    fn rejects_points_outside_the_domain() {
        let tm = utm(&grs80(), 31, false);
        for longitude in [93.0 * DEG, -87.0 * DEG] {
            assert_eq!(
                tm.forward(GeographicPoint::new(longitude, 0.0, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
            assert_eq!(
                tm.factors(GeographicPoint::new(longitude, 0.0, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
        }
        assert_eq!(
            tm.forward(GeographicPoint::new(0.0, f64::NAN, 0.0)),
            Err(GeodesyError::NonFinite)
        );
        assert_eq!(
            tm.forward(GeographicPoint::new(0.0, -1.6, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange)
        );
        assert_eq!(
            tm.inverse(ProjectedPoint::new(f64::NAN, 0.0, 0.0)),
            Err(GeodesyError::NonFinite)
        );
        assert_eq!(
            tm.inverse(ProjectedPoint::new(2e7, 0.0, 0.0)),
            Err(GeodesyError::OutsideDomain)
        );
    }

    #[test]
    fn projects_the_poles_onto_the_central_meridian() {
        let tm = utm(&grs80(), 31, false);
        for latitude in [FRAC_PI_2, -FRAC_PI_2] {
            let pole = tm
                .forward(GeographicPoint::new(40.0 * DEG, latitude, 0.0))
                .unwrap();
            assert!((pole.eastings - 500_000.0).abs() < 1e-6);
            let back = tm.inverse(pole).unwrap();
            assert!((back.latitude - latitude).abs() < 1e-12);
        }
    }

    /// Regression: the series are periodic in northing, so a point beyond the antimeridian used
    /// to come back as a plausible point near the equator.
    #[test]
    fn rejects_northings_beyond_the_antimeridian() {
        let tm = utm(&grs80(), 31, false);
        for northings in [1e9, -1e9, 1e300] {
            assert_eq!(
                tm.inverse(ProjectedPoint::new(500_000.0, northings, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
        }
    }

    /// Regression: forward projects points more than 90° from the central meridian past the
    /// poles, and the inverse used to reject them.
    #[test]
    fn round_trips_beyond_the_poles() {
        let tm = utm(&grs80(), 31, false);
        for (longitude, latitude) in [(123.0, 80.0), (100.0, 60.0), (150.0, 45.0), (183.0, 10.0)] {
            let point = GeographicPoint::new(longitude * DEG, latitude * DEG, 0.0);
            let back = tm.inverse(tm.forward(point).unwrap()).unwrap();
            assert!(ground_distance(back, point) < 1e-6, "{back:?} {point:?}");
        }
    }

    /// Regression: at a pole the convergence is not defined and the scale was computed from
    /// rounding noise.
    #[test]
    fn has_no_factors_at_the_poles() {
        let tm = utm(&grs80(), 31, false);
        for latitude in [FRAC_PI_2, -FRAC_PI_2] {
            assert_eq!(
                tm.factors(GeographicPoint::new(40.0 * DEG, latitude, 0.0)),
                Err(GeodesyError::OutsideDomain)
            );
        }
        assert_eq!(
            tm.factors(GeographicPoint::new(0.0, f64::NAN, 0.0)),
            Err(GeodesyError::NonFinite)
        );
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Within 40° of the central meridian, beyond any UTM zone, forward then inverse comes
        /// back to the point within a micrometre.
        #[test]
        fn round_trips(
            zone in 1_u8..=60,
            south: bool,
            offset in -40.0..40.0_f64,
            latitude in -89.0..89.0_f64,
        ) {
            let tm = utm(&grs80(), zone, south);
            let central = f64::from(zone) * 6.0 - 183.0;
            let point = GeographicPoint::new((central + offset) * DEG, latitude * DEG, 0.0);
            let back = tm.inverse(tm.forward(point).unwrap()).unwrap();
            prop_assert!(ground_distance(back, point) < 1e-6);
        }

        /// Northings grow with latitude along the central meridian, eastings with longitude
        /// along a parallel.
        #[test]
        fn is_monotonic(a in -84.0..84.0_f64, b in -84.0..84.0_f64, c in -3.5..3.5_f64, d in -3.5..3.5_f64) {
            let tm = utm(&wgs84(), 31, false);
            let at = |longitude: f64, latitude: f64| {
                tm.forward(GeographicPoint::new((3.0 + longitude) * DEG, latitude * DEG, 0.0)).unwrap()
            };
            prop_assert_eq!(a < b, at(0.0, a).northings < at(0.0, b).northings);
            prop_assert_eq!(c < d, at(c, a).eastings < at(d, a).eastings);
        }
    }
}
