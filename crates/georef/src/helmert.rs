//! The seven-parameter Helmert transformation between geocentric frames: EPSG methods 1033 and
//! 9606 (position vector) and 1032 and 9607 (coordinate frame). It uses the small-angle rotation
//! matrix that EPSG defines and PROJ's `helmert` applies by default (EPSG Guidance Note 7-2,
//! §4.4.3).

use std::f64::consts::PI;

use crate::geodesy::{
    Ellipsoid, GeocentricPoint, GeodesyError, GeographicPoint, Parameter, finite,
};

/// How the rotations of a Helmert transformation are signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationConvention {
    /// Position vector (EPSG 1033, 9606): the rotations turn the position vector. IERS and ISO
    /// 19111 use it.
    PositionVector,
    /// Coordinate frame (EPSG 1032, 9607): the rotations turn the coordinate axes, so they have
    /// the opposite sign.
    CoordinateFrame,
}

/// The parameters of a seven-parameter Helmert transformation, in the units EPSG publishes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HelmertParameters {
    /// Translation along X, Y and Z, in metres.
    pub translation: [f64; 3],
    /// Rotation about X, Y and Z, in arc-seconds.
    pub rotation: [f64; 3],
    /// Scale difference, in parts per million.
    pub scale_difference: f64,
    /// How the rotations are signed.
    pub convention: RotationConvention,
}

/// A Helmert transformation, ready to convert points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Helmert {
    translation: [f64; 3],
    /// The scaled rotation matrix, in position-vector form: `X′ = T + matrix · X`.
    matrix: [[f64; 3]; 3],
    /// Its exact inverse.
    inverse: [[f64; 3]; 3],
}

/// Arc-seconds to radians.
const ARC_SECOND: f64 = PI / 648_000.0;

impl Helmert {
    /// The transformation with `parameters`.
    ///
    /// # Errors
    ///
    /// [`GeodesyError::InvalidParameter`] when a parameter is not finite or the scale difference
    /// is −10⁶ ppm or less (no positive scale).
    pub fn new(parameters: &HelmertParameters) -> Result<Self, GeodesyError> {
        let [tx, ty, tz] = parameters.translation;
        let translation = [
            finite(tx, Parameter::Translation)?,
            finite(ty, Parameter::Translation)?,
            finite(tz, Parameter::Translation)?,
        ];
        let sign = match parameters.convention {
            RotationConvention::PositionVector => ARC_SECOND,
            RotationConvention::CoordinateFrame => -ARC_SECOND,
        };
        let [rx, ry, rz] = parameters.rotation;
        let rx = finite(rx, Parameter::Rotation)? * sign;
        let ry = finite(ry, Parameter::Rotation)? * sign;
        let rz = finite(rz, Parameter::Rotation)? * sign;
        let scale = 1.0 + finite(parameters.scale_difference, Parameter::ScaleDifference)? * 1e-6;
        if scale <= 0.0 {
            return Err(GeodesyError::InvalidParameter {
                parameter: Parameter::ScaleDifference,
            });
        }
        let matrix = [
            [scale, -scale * rz, scale * ry],
            [scale * rz, scale, -scale * rx],
            [-scale * ry, scale * rx, scale],
        ];
        Ok(Self {
            translation,
            matrix,
            inverse: invert(&matrix),
        })
    }

    /// `point` in the target frame.
    #[must_use]
    pub fn forward(&self, point: GeocentricPoint) -> GeocentricPoint {
        let [x, y, z] = multiply(&self.matrix, [point.x, point.y, point.z]);
        let [tx, ty, tz] = self.translation;
        GeocentricPoint::new(x + tx, y + ty, z + tz)
    }

    /// `point` back in the source frame: the exact inverse of [`Helmert::forward`], up to
    /// rounding.
    #[must_use]
    pub fn inverse(&self, point: GeocentricPoint) -> GeocentricPoint {
        let [tx, ty, tz] = self.translation;
        let [x, y, z] = multiply(&self.inverse, [point.x - tx, point.y - ty, point.z - tz]);
        GeocentricPoint::new(x, y, z)
    }

    /// `point`, geographic on `source`, transformed to geographic on `target` through
    /// geocentric coordinates.
    ///
    /// # Errors
    ///
    /// As [`Ellipsoid::to_geocentric`] and [`Ellipsoid::to_geographic`].
    pub fn forward_geographic(
        &self,
        source: &Ellipsoid,
        target: &Ellipsoid,
        point: GeographicPoint,
    ) -> Result<GeographicPoint, GeodesyError> {
        target.to_geographic(self.forward(source.to_geocentric(point)?))
    }

    /// `point`, geographic on `target`, transformed back to geographic on `source`.
    ///
    /// # Errors
    ///
    /// As [`Ellipsoid::to_geocentric`] and [`Ellipsoid::to_geographic`].
    pub fn inverse_geographic(
        &self,
        source: &Ellipsoid,
        target: &Ellipsoid,
        point: GeographicPoint,
    ) -> Result<GeographicPoint, GeodesyError> {
        source.to_geographic(self.inverse(target.to_geocentric(point)?))
    }
}

fn multiply(matrix: &[[f64; 3]; 3], [x, y, z]: [f64; 3]) -> [f64; 3] {
    matrix.map(|[a, b, c]| a * x + b * y + c * z)
}

/// The inverse of `m`, by cofactors. The Helmert matrix is the identity up to a few millionths,
/// so its determinant is near 1 and the inverse is well conditioned.
fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let cofactor =
        |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let adjugate = [
        [
            cofactor(1, 2, 1, 2),
            -cofactor(0, 2, 1, 2),
            cofactor(0, 1, 1, 2),
        ],
        [
            -cofactor(1, 2, 0, 2),
            cofactor(0, 2, 0, 2),
            -cofactor(0, 1, 0, 2),
        ],
        [
            cofactor(1, 2, 0, 1),
            -cofactor(0, 2, 0, 1),
            cofactor(0, 1, 0, 1),
        ],
    ];
    let determinant =
        m[0][0] * adjugate[0][0] + m[0][1] * adjugate[1][0] + m[0][2] * adjugate[2][0];
    adjugate.map(|row| row.map(|value| value / determinant))
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::test_support::{DEG, grs80, wgs84};

    /// WGS 72 to WGS 84 (EPSG:1238) in `convention`, with the rotation signed for it.
    fn wgs72_to_wgs84(convention: RotationConvention) -> Helmert {
        let rz = match convention {
            RotationConvention::PositionVector => 0.554,
            RotationConvention::CoordinateFrame => -0.554,
        };
        Helmert::new(&HelmertParameters {
            translation: [0.0, 0.0, 4.5],
            rotation: [0.0, 0.0, rz],
            scale_difference: 0.219,
            convention,
        })
        .unwrap()
    }

    /// EPSG Guidance Note 7-2 (IOGP Publication 373-7-2, 2019), §4.4.3.1 and §4.4.3.2
    /// examples: the same transformation in both conventions gives the same point. The note's
    /// source coordinates are rounded to the centimetre, so the result is checked to 1 cm.
    #[test]
    fn transforms_the_guidance_note_example() {
        let source = GeocentricPoint::new(3_657_660.66, 255_768.55, 5_201_382.11);
        for convention in [
            RotationConvention::PositionVector,
            RotationConvention::CoordinateFrame,
        ] {
            let helmert = wgs72_to_wgs84(convention);
            let target = helmert.forward(source);
            assert!((target.x - 3_657_660.78).abs() < 0.01, "{target:?}");
            assert!((target.y - 255_778.43).abs() < 0.01, "{target:?}");
            assert!((target.z - 5_201_387.75).abs() < 0.01, "{target:?}");
            let back = helmert.inverse(target);
            assert!((back.x - source.x).abs() < 1e-8);
            assert!((back.y - source.y).abs() < 1e-8);
            assert!((back.z - source.z).abs() < 1e-8);
        }
    }

    #[test]
    fn the_conventions_turn_opposite_ways() {
        let parameters = HelmertParameters {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 1.0],
            scale_difference: 0.0,
            convention: RotationConvention::PositionVector,
        };
        let point = GeocentricPoint::new(6e6, 0.0, 0.0);
        let position = Helmert::new(&parameters).unwrap().forward(point);
        let frame = Helmert::new(&HelmertParameters {
            convention: RotationConvention::CoordinateFrame,
            ..parameters
        })
        .unwrap()
        .forward(point);
        assert!(position.y > 0.0);
        assert_eq!(frame.y.to_bits(), (-position.y).to_bits());
    }

    #[test]
    fn transforms_geographic_points_between_ellipsoids() {
        let helmert = wgs72_to_wgs84(RotationConvention::PositionVector);
        let point = GeographicPoint::new(2.0 * DEG, 53.0 * DEG, 50.0);
        let target = helmert
            .forward_geographic(&grs80(), &wgs84(), point)
            .unwrap();
        assert!((target.latitude - point.latitude).abs() > 1e-9);
        let back = helmert
            .inverse_geographic(&grs80(), &wgs84(), target)
            .unwrap();
        assert!((back.latitude - point.latitude).abs() < 1e-14);
        assert!((back.longitude - point.longitude).abs() < 1e-14);
        assert!((back.height - point.height).abs() < 1e-8);
        assert_eq!(
            helmert.forward_geographic(&grs80(), &wgs84(), GeographicPoint::new(0.0, 2.0, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange)
        );
    }

    #[test]
    fn rejects_invalid_parameters() {
        let valid = HelmertParameters {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale_difference: 0.0,
            convention: RotationConvention::PositionVector,
        };
        let error = |parameter| Err(GeodesyError::InvalidParameter { parameter });
        for (parameters, parameter) in [
            (
                HelmertParameters {
                    translation: [0.0, f64::NAN, 0.0],
                    ..valid
                },
                Parameter::Translation,
            ),
            (
                HelmertParameters {
                    translation: [0.0, 0.0, f64::INFINITY],
                    ..valid
                },
                Parameter::Translation,
            ),
            (
                HelmertParameters {
                    translation: [f64::NAN, 0.0, 0.0],
                    ..valid
                },
                Parameter::Translation,
            ),
            (
                HelmertParameters {
                    rotation: [f64::NAN, 0.0, 0.0],
                    ..valid
                },
                Parameter::Rotation,
            ),
            (
                HelmertParameters {
                    rotation: [0.0, f64::NAN, 0.0],
                    ..valid
                },
                Parameter::Rotation,
            ),
            (
                HelmertParameters {
                    rotation: [0.0, 0.0, f64::NAN],
                    ..valid
                },
                Parameter::Rotation,
            ),
            (
                HelmertParameters {
                    scale_difference: f64::NAN,
                    ..valid
                },
                Parameter::ScaleDifference,
            ),
            (
                HelmertParameters {
                    scale_difference: -1e6,
                    ..valid
                },
                Parameter::ScaleDifference,
            ),
        ] {
            assert_eq!(Helmert::new(&parameters), error(parameter));
        }
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        #[test]
        fn round_trips(
            translation in prop::array::uniform3(-1e3..1e3_f64),
            rotation in prop::array::uniform3(-10.0..10.0_f64),
            scale_difference in -50.0..50.0_f64,
            point in prop::array::uniform3(-7e6..7e6_f64),
            frame: bool,
        ) {
            let helmert = Helmert::new(&HelmertParameters {
                translation,
                rotation,
                scale_difference,
                convention: if frame { RotationConvention::CoordinateFrame } else { RotationConvention::PositionVector },
            }).unwrap();
            let [x, y, z] = point;
            let back = helmert.inverse(helmert.forward(GeocentricPoint::new(x, y, z)));
            prop_assert!((back.x - x).abs() < 1e-8);
            prop_assert!((back.y - y).abs() < 1e-8);
            prop_assert!((back.z - z).abs() < 1e-8);
        }
    }
}
