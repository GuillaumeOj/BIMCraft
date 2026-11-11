//! The camera: an `f64` eye looking at an `f64` target (GEO-07), and its projection.

use core::f64::consts::PI;

use geom::{Point3, Vector3};

/// The shortest distance between the eye and the target, in metres.
const MIN_VIEW_DISTANCE: f64 = 1e-9;

/// The smallest sine of the angle between the view direction and the up vector.
const MIN_UP_SINE: f64 = 1e-6;

/// How the camera projects the view onto the screen. Both use reverse-Z: depth is 1 at the near
/// plane and decreases with the distance, which keeps depth resolution far from the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// Perspective with an infinite far plane.
    Perspective {
        /// Vertical field of view, in radians, strictly between 0 and π.
        fov_y_radians: f64,
        /// Distance of the near plane, in metres, positive.
        near: f64,
    },
    /// Parallel projection.
    Orthographic {
        /// Height of the view, in metres, positive; its width follows the aspect ratio.
        height: f64,
        /// Distance of the near plane, in metres, positive.
        near: f64,
        /// Distance of the far plane, in metres, beyond the near plane.
        far: f64,
    },
}

impl Projection {
    /// Checks that the parameters are finite and in range.
    ///
    /// # Errors
    ///
    /// The first [`CameraError`] found.
    pub fn validate(&self) -> Result<(), CameraError> {
        match *self {
            Self::Perspective {
                fov_y_radians,
                near,
            } => {
                if !(fov_y_radians.is_finite() && near.is_finite()) {
                    return Err(CameraError::NonFinite);
                }
                if !(fov_y_radians > 0.0 && fov_y_radians < PI) {
                    return Err(CameraError::InvalidFieldOfView);
                }
                check_near(near)
            }
            Self::Orthographic { height, near, far } => {
                if !(height.is_finite() && near.is_finite() && far.is_finite()) {
                    return Err(CameraError::NonFinite);
                }
                if height <= 0.0 {
                    return Err(CameraError::InvalidHeight);
                }
                check_near(near)?;
                if far <= near {
                    return Err(CameraError::FarNotBeyondNear);
                }
                Ok(())
            }
        }
    }
}

fn check_near(near: f64) -> Result<(), CameraError> {
    if near > 0.0 {
        Ok(())
    } else {
        Err(CameraError::InvalidNear)
    }
}

/// A look-at camera, in `f64` world coordinates (GEO-07).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Position of the camera, in metres.
    pub eye: Point3,
    /// Point the camera looks at, in metres.
    pub target: Point3,
    /// Which way is up on the screen; any vector not parallel to the view direction.
    pub up: Vector3,
    /// How the view is projected.
    pub projection: Projection,
}

impl Camera {
    /// Checks that the camera is finite, within
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE), not degenerate, and that its
    /// projection is valid.
    ///
    /// # Errors
    ///
    /// The first [`CameraError`] found.
    pub fn validate(&self) -> Result<(), CameraError> {
        self.view_basis().map(|_| ())
    }

    /// The rows of the view rotation, from world axes to camera axes: right, up and backwards,
    /// since the camera looks down its −z axis. Validates the camera.
    pub(crate) fn view_basis(&self) -> Result<[[f64; 3]; 3], CameraError> {
        let eye = [self.eye.x, self.eye.y, self.eye.z];
        let target = [self.target.x, self.target.y, self.target.z];
        let up = [self.up.x, self.up.y, self.up.z];
        let values = || eye.iter().chain(&target).chain(&up);
        if !values().all(|v| v.is_finite()) {
            return Err(CameraError::NonFinite);
        }
        self.projection.validate()?;
        if !values().all(|&v| crate::in_world(v)) {
            return Err(CameraError::OutOfRange);
        }
        let view = sub(target, eye);
        let distance = norm(view);
        if distance <= MIN_VIEW_DISTANCE {
            return Err(CameraError::EyeAtTarget);
        }
        let forward = scale(view, 1.0 / distance);
        // Dividing by the largest component first keeps a tiny up vector from underflowing.
        let largest = up.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        if largest == 0.0 {
            return Err(CameraError::UpParallelToView);
        }
        let up = up.map(|v| v / largest);
        let side = cross(forward, up);
        let side_length = norm(side);
        if side_length <= MIN_UP_SINE * norm(up) {
            return Err(CameraError::UpParallelToView);
        }
        let right = scale(side, 1.0 / side_length);
        let true_up = cross(right, forward);
        Ok([right, true_up, scale(forward, -1.0)])
    }
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Why a camera cannot be used. Messages never contain coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CameraError {
    /// A coordinate or a projection parameter is not finite.
    #[error("the camera has a value that is not finite")]
    NonFinite,
    /// The eye, the target or the up vector lies beyond
    /// [`MAX_WORLD_COORDINATE`](crate::MAX_WORLD_COORDINATE).
    #[error("the camera lies too far from the world origin")]
    OutOfRange,
    /// The eye and the target are the same point, so there is no view direction.
    #[error("the camera looks at its own position")]
    EyeAtTarget,
    /// The up vector is zero or parallel to the view direction.
    #[error("the up vector of the camera is zero or parallel to the view direction")]
    UpParallelToView,
    /// The field of view is not strictly between 0 and π.
    #[error("the field of view of the camera is not between 0 and 180 degrees")]
    InvalidFieldOfView,
    /// The near plane is not in front of the camera.
    #[error("the near plane of the camera is not in front of it")]
    InvalidNear,
    /// The orthographic view height is not positive.
    #[error("the view height of the camera is not positive")]
    InvalidHeight,
    /// The far plane is not beyond the near plane.
    #[error("the far plane of the camera is not beyond its near plane")]
    FarNotBeyondNear,
    /// The aspect ratio of the target is not a positive finite number.
    #[error("the aspect ratio of the view is not a positive number")]
    InvalidAspect,
    /// The projection parameters are valid but so extreme that the single-precision matrix of
    /// the GPU cannot represent them.
    #[error("the projection of the camera is too extreme for the GPU")]
    ProjectionOutOfRange,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::RngSeed;

    use super::*;
    use crate::MAX_WORLD_COORDINATE;

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: 1.0,
        near: 0.1,
    };

    fn camera() -> Camera {
        Camera {
            eye: Point3::new(0.0, -3.0, 1.0),
            target: Point3::new(0.0, 0.0, 1.0),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: PERSPECTIVE,
        }
    }

    #[test]
    fn a_camera_looking_along_y_has_the_expected_basis() {
        let basis = camera().view_basis().unwrap();
        assert_eq!(basis, [[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, -1.0, 0.0]]);
        assert_eq!(camera().validate(), Ok(()));
    }

    #[test]
    fn the_basis_is_orthonormal_for_a_slanted_up() {
        let mut slanted = camera();
        slanted.eye = Point3::new(1e7 + 2.0, 1e7 - 1.0, 120.0);
        slanted.target = Point3::new(1e7, 1e7, 100.0);
        slanted.up = Vector3::new(0.3, 0.1, 5.0);
        let basis = slanted.view_basis().unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let dot: f64 = (0..3).map(|k| basis[i][k] * basis[j][k]).sum();
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((dot - expected).abs() < 1e-15);
            }
        }
    }

    #[test]
    fn values_that_are_not_finite_are_rejected() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut cameras = Vec::new();
            for axis in 0..3 {
                let mut c = camera();
                let mut v = [0.0; 3];
                v[axis] = bad;
                c.eye = Point3::new(v[0], v[1] - 3.0, v[2]);
                cameras.push(c);
                let mut c = camera();
                c.target = Point3::new(v[0], v[1], v[2]);
                cameras.push(c);
                let mut c = camera();
                c.up = Vector3::new(v[0], v[1], v[2] + 1.0);
                cameras.push(c);
            }
            for projection in [
                Projection::Perspective {
                    fov_y_radians: bad,
                    near: 0.1,
                },
                Projection::Perspective {
                    fov_y_radians: 1.0,
                    near: bad,
                },
                Projection::Orthographic {
                    height: bad,
                    near: 0.1,
                    far: 10.0,
                },
                Projection::Orthographic {
                    height: 1.0,
                    near: bad,
                    far: 10.0,
                },
                Projection::Orthographic {
                    height: 1.0,
                    near: 0.1,
                    far: bad,
                },
            ] {
                cameras.push(Camera {
                    projection,
                    ..camera()
                });
            }
            for c in cameras {
                assert_eq!(c.validate(), Err(CameraError::NonFinite), "{c:?}");
            }
        }
    }

    #[test]
    fn cameras_too_far_from_the_origin_are_rejected() {
        let far = MAX_WORLD_COORDINATE * 1.5;
        let mut c = camera();
        c.eye = Point3::new(far, 0.0, 0.0);
        assert_eq!(c.validate(), Err(CameraError::OutOfRange));
        let mut c = camera();
        c.target = Point3::new(0.0, far, 0.0);
        assert_eq!(c.validate(), Err(CameraError::OutOfRange));
        let mut c = camera();
        c.up = Vector3::new(0.0, 0.0, far);
        assert_eq!(c.validate(), Err(CameraError::OutOfRange));
    }

    #[test]
    fn a_camera_looking_at_itself_is_rejected() {
        let mut c = camera();
        c.target = c.eye;
        assert_eq!(c.validate(), Err(CameraError::EyeAtTarget));
        c.target = Point3::new(c.eye.x + 1e-10, c.eye.y, c.eye.z);
        assert_eq!(c.validate(), Err(CameraError::EyeAtTarget));
    }

    #[test]
    fn a_zero_or_parallel_up_is_rejected() {
        for up in [
            Vector3::new(0.0, 0.0, 0.0),
            Vector3::new(0.0, 1.0, 0.0),
            Vector3::new(0.0, -2.0, 0.0),
            Vector3::new(1e-9, 1.0, 0.0),
        ] {
            let c = Camera { up, ..camera() };
            assert_eq!(c.validate(), Err(CameraError::UpParallelToView));
        }
        let nearly = Camera {
            up: Vector3::new(1e-3, 1.0, 0.0),
            ..camera()
        };
        assert_eq!(nearly.validate(), Ok(()));
    }

    #[test]
    fn perspective_parameters_are_checked() {
        for fov_y_radians in [0.0, -0.5, PI, 4.0] {
            let p = Projection::Perspective {
                fov_y_radians,
                near: 0.1,
            };
            assert_eq!(p.validate(), Err(CameraError::InvalidFieldOfView));
        }
        for near in [0.0, -1.0] {
            let p = Projection::Perspective {
                fov_y_radians: 1.0,
                near,
            };
            assert_eq!(p.validate(), Err(CameraError::InvalidNear));
        }
        assert_eq!(PERSPECTIVE.validate(), Ok(()));
    }

    #[test]
    fn orthographic_parameters_are_checked() {
        let ortho = |height, near, far| Projection::Orthographic { height, near, far }.validate();
        assert_eq!(ortho(10.0, 0.1, 100.0), Ok(()));
        assert_eq!(ortho(0.0, 0.1, 100.0), Err(CameraError::InvalidHeight));
        assert_eq!(ortho(-1.0, 0.1, 100.0), Err(CameraError::InvalidHeight));
        assert_eq!(ortho(10.0, 0.0, 100.0), Err(CameraError::InvalidNear));
        assert_eq!(ortho(10.0, 1.0, 1.0), Err(CameraError::FarNotBeyondNear));
        assert_eq!(ortho(10.0, 2.0, 1.0), Err(CameraError::FarNotBeyondNear));
    }

    #[test]
    fn an_invalid_projection_invalidates_the_camera() {
        let c = Camera {
            projection: Projection::Orthographic {
                height: 1.0,
                near: 1.0,
                far: 0.5,
            },
            ..camera()
        };
        assert_eq!(c.validate(), Err(CameraError::FarNotBeyondNear));
    }

    #[test]
    fn messages_never_contain_coordinates() {
        for error in [
            CameraError::NonFinite,
            CameraError::OutOfRange,
            CameraError::EyeAtTarget,
            CameraError::UpParallelToView,
            CameraError::InvalidFieldOfView,
            CameraError::InvalidNear,
            CameraError::InvalidHeight,
            CameraError::FarNotBeyondNear,
            CameraError::InvalidAspect,
            CameraError::ProjectionOutOfRange,
        ] {
            let message = error.to_string();
            assert!(message.starts_with("the "), "{message}");
            assert!(!message.contains('-') && !message.contains('.'));
        }
    }

    fn config() -> ProptestConfig {
        ProptestConfig {
            cases: 256,
            rng_seed: RngSeed::Fixed(68),
            failure_persistence: None,
            ..ProptestConfig::default()
        }
    }

    fn any_value() -> impl Strategy<Value = f64> {
        prop_oneof![
            -10.0f64..10.0,
            Just(0.0),
            Just(f64::NAN),
            Just(f64::INFINITY),
            any::<f64>()
        ]
    }

    proptest! {
        #![proptest_config(config())]

        /// Arbitrary cameras are accepted or rejected with an error, never a panic, and an
        /// accepted camera has an orthonormal basis.
        #[test]
        fn validation_never_panics(
            v in proptest::array::uniform9(any_value()),
            p in proptest::array::uniform3(any_value()),
            orthographic in any::<bool>(),
        ) {
            let projection = if orthographic {
                Projection::Orthographic { height: p[0], near: p[1], far: p[2] }
            } else {
                Projection::Perspective { fov_y_radians: p[0], near: p[1] }
            };
            let c = Camera {
                eye: Point3::new(v[0], v[1], v[2]),
                target: Point3::new(v[3], v[4], v[5]),
                up: Vector3::new(v[6], v[7], v[8]),
                projection,
            };
            if let Ok(basis) = c.view_basis() {
                for row in basis {
                    prop_assert!((norm(row) - 1.0).abs() < 1e-12);
                }
            }
        }
    }
}
