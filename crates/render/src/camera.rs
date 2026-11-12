//! The camera: an `f64` eye looking at an `f64` target (GEO-07), and its projection.

use core::f64::consts::PI;

use geom::{Point3, Vector3};

use crate::MAX_WORLD_COORDINATE;
use crate::vector::{Rotation, cross, norm, scale, sub};

/// The shortest distance between the eye and the target, in metres.
const MIN_VIEW_DISTANCE: f64 = 1e-9;

/// The smallest sine of the angle between the view direction and the up vector.
const MIN_UP_SINE: f64 = 1e-6;

/// The shortest near distance, view height and near-to-far depth of a projection, in metres.
/// With the other bounds of [`Projection::validate`], every entry of the projection matrix is a
/// normal `f32` number, neither infinite nor rounded to zero.
const MIN_PROJECTION_LENGTH: f64 = 1e-6;

/// The narrowest field of view, and the margin below π of the widest, in radians.
const MIN_FIELD_OF_VIEW: f64 = 1e-6;

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
    /// Checks that the parameters are finite and in range: the field of view between 10⁻⁶ rad
    /// and π − 10⁻⁶ rad; the near plane, the view height and the far plane between a micrometre
    /// and [`MAX_WORLD_COORDINATE`], the far plane at least a micrometre beyond the near plane.
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
                if !(MIN_FIELD_OF_VIEW..=PI - MIN_FIELD_OF_VIEW).contains(&fov_y_radians) {
                    return Err(CameraError::InvalidFieldOfView);
                }
                check_length(near, CameraError::InvalidNear)
            }
            Self::Orthographic { height, near, far } => {
                if !(height.is_finite() && near.is_finite() && far.is_finite()) {
                    return Err(CameraError::NonFinite);
                }
                check_length(height, CameraError::InvalidHeight)?;
                check_length(near, CameraError::InvalidNear)?;
                if far - near < MIN_PROJECTION_LENGTH || far > MAX_WORLD_COORDINATE {
                    return Err(CameraError::InvalidFar);
                }
                Ok(())
            }
        }
    }
}

fn check_length(length: f64, error: CameraError) -> Result<(), CameraError> {
    if (MIN_PROJECTION_LENGTH..=MAX_WORLD_COORDINATE).contains(&length) {
        Ok(())
    } else {
        Err(error)
    }
}

/// A look-at camera, in `f64` world coordinates (GEO-07).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Position of the camera, in metres.
    pub eye: Point3,
    /// Point the camera looks at, in metres.
    pub target: Point3,
    /// Which way is up on the screen; any finite vector not parallel to the view direction.
    pub up: Vector3,
    /// How the view is projected.
    pub projection: Projection,
}

impl Camera {
    /// Checks that the camera is finite, that its eye and target lie within
    /// [`MAX_WORLD_COORDINATE`], that it is not degenerate, and that its projection is valid.
    ///
    /// # Errors
    ///
    /// The first [`CameraError`] found.
    pub fn validate(&self) -> Result<(), CameraError> {
        self.view_basis()?;
        self.projection.validate()
    }

    /// The rows of the view rotation, from world axes to camera axes: right, up and backwards,
    /// since the camera looks down its −z axis. Validates everything but the projection.
    pub(crate) fn view_basis(&self) -> Result<Rotation, CameraError> {
        let eye = [self.eye.x, self.eye.y, self.eye.z];
        let target = [self.target.x, self.target.y, self.target.z];
        let up = [self.up.x, self.up.y, self.up.z];
        if !eye.iter().chain(&target).chain(&up).all(|v| v.is_finite()) {
            return Err(CameraError::NonFinite);
        }
        if !eye.iter().chain(&target).all(|&v| crate::in_world(v)) {
            return Err(CameraError::OutOfRange);
        }
        let view = sub(target, eye);
        let distance = norm(view);
        if distance <= MIN_VIEW_DISTANCE {
            return Err(CameraError::EyeAtTarget);
        }
        let forward = scale(view, 1.0 / distance);
        // Dividing by the largest component first keeps the up vector from underflowing or
        // overflowing, whatever its length.
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

/// Why a camera cannot be used. Messages never contain coordinates (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CameraError {
    /// A coordinate or a projection parameter is not finite.
    #[error("the camera has a value that is not finite")]
    NonFinite,
    /// The eye or the target lies beyond [`MAX_WORLD_COORDINATE`].
    #[error("the camera lies too far from the world origin")]
    OutOfRange,
    /// The eye and the target are the same point, so there is no view direction.
    #[error("the camera looks at its own position")]
    EyeAtTarget,
    /// The up vector is zero or parallel to the view direction.
    #[error("the up vector of the camera is zero or parallel to the view direction")]
    UpParallelToView,
    /// The field of view is not within the range of [`Projection::validate`].
    #[error("the field of view of the camera is not between 0 and 180 degrees")]
    InvalidFieldOfView,
    /// The near plane is not within the range of [`Projection::validate`].
    #[error("the near plane of the camera is not at a usable distance in front of it")]
    InvalidNear,
    /// The orthographic view height is not within the range of [`Projection::validate`].
    #[error("the view height of the camera is not a usable positive length")]
    InvalidHeight,
    /// The far plane is not beyond the near plane, or lies beyond [`MAX_WORLD_COORDINATE`].
    #[error("the far plane of the camera is not at a usable distance beyond its near plane")]
    InvalidFar,
    /// The aspect ratio of the target is not between 10⁻⁴ and 10⁴.
    #[error("the aspect ratio of the view is not a usable positive number")]
    InvalidAspect,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::vector::is_orthonormal;

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
        assert!(is_orthonormal(&slanted.view_basis().unwrap(), 1e-15));
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
    fn eyes_and_targets_too_far_from_the_origin_are_rejected() {
        let far = MAX_WORLD_COORDINATE * 1.5;
        let mut c = camera();
        c.eye = Point3::new(far, 0.0, 0.0);
        assert_eq!(c.validate(), Err(CameraError::OutOfRange));
        let mut c = camera();
        c.target = Point3::new(0.0, far, 0.0);
        assert_eq!(c.validate(), Err(CameraError::OutOfRange));
        // The up vector is a direction: its length does not matter.
        for length in [far, f64::MAX, 1e-300] {
            let c = Camera {
                up: Vector3::new(0.0, 0.0, length),
                ..camera()
            };
            assert_eq!(c.view_basis(), camera().view_basis());
        }
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
        for fov_y_radians in [0.0, 1e-7, -0.5, PI - 1e-7, PI, 4.0] {
            let p = Projection::Perspective {
                fov_y_radians,
                near: 0.1,
            };
            assert_eq!(p.validate(), Err(CameraError::InvalidFieldOfView));
        }
        for near in [0.0, -1.0, 1e-7, MAX_WORLD_COORDINATE * 1.5] {
            let p = Projection::Perspective {
                fov_y_radians: 1.0,
                near,
            };
            assert_eq!(p.validate(), Err(CameraError::InvalidNear));
        }
        assert_eq!(PERSPECTIVE.validate(), Ok(()));
        for (fov_y_radians, near) in [
            (MIN_FIELD_OF_VIEW, MIN_PROJECTION_LENGTH),
            (PI - MIN_FIELD_OF_VIEW, MAX_WORLD_COORDINATE),
        ] {
            let p = Projection::Perspective {
                fov_y_radians,
                near,
            };
            assert_eq!(p.validate(), Ok(()));
        }
    }

    #[test]
    fn orthographic_parameters_are_checked() {
        let ortho = |height, near, far| Projection::Orthographic { height, near, far }.validate();
        assert_eq!(ortho(10.0, 0.1, 100.0), Ok(()));
        let (min, max) = (MIN_PROJECTION_LENGTH, MAX_WORLD_COORDINATE);
        assert_eq!(ortho(min, min, 2.0 * min), Ok(()));
        assert_eq!(ortho(max, max - 1.0, max), Ok(()));
        for height in [0.0, -1.0, 1e-7, max * 1.5] {
            assert_eq!(ortho(height, 0.1, 100.0), Err(CameraError::InvalidHeight));
        }
        for near in [0.0, 1e-7, max * 1.5] {
            assert_eq!(ortho(10.0, near, max * 2.0), Err(CameraError::InvalidNear));
        }
        for (near, far) in [(1.0, 1.0), (2.0, 1.0), (1.0, 1.0 + 1e-7), (1.0, max * 1.5)] {
            assert_eq!(ortho(10.0, near, far), Err(CameraError::InvalidFar));
        }
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
        assert_eq!(c.validate(), Err(CameraError::InvalidFar));
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
            CameraError::InvalidFar,
            CameraError::InvalidAspect,
        ] {
            let message = error.to_string();
            assert!(message.starts_with("the "), "{message}");
            assert!(!message.contains('-') && !message.contains('.'));
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
        #![proptest_config(crate::proptest_config())]

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
