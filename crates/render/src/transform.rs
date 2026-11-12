//! Camera-relative transforms, computed in `f64` and sent to the GPU in `f32` (VP-04, GEO-07).
//!
//! The GPU computes `clip = P · MV · v` in `f32`, with `v` a local vertex of a mesh. Rounding a
//! world coordinate to `f32` costs 6.25 cm at 10⁶ m and 1 m at 10⁷ m, so `MV` is built here
//! with the camera at the origin: its large terms (the world positions of the object and the
//! eye) cancel in `f64`, and only the result, whose size is the distance to the camera, is
//! rounded to `f32` (`docs/dev/adr/0020-camera-relative-rendering.md`).
//!
//! Matrices are column-major `[[f32; 4]; 4]`, an array of columns: the layout of a WGSL
//! `mat4x4<f32>` uniform.

use crate::vector::{Rotation, add, apply, mul, sub};
use crate::{Camera, CameraError, Placement, Projection};

/// The narrowest and widest aspect ratios of a view, width over height.
const ASPECT_RANGE: core::ops::RangeInclusive<f64> = 1e-4..=1e4;

/// A 4×4 matrix sent to the GPU, column-major: `m[column][row]`.
pub(crate) type Matrix = [[f32; 4]; 4];

/// The view of a validated camera, in `f64`: where it is and how it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ViewFrame {
    /// The world position of the camera, in metres.
    eye: [f64; 3],
    /// Rows: the right, up and backward axes of the camera in world coordinates.
    rotation: Rotation,
}

impl ViewFrame {
    /// Validates `camera`, its projection aside, and computes its view, once per frame.
    pub(crate) fn new(camera: &Camera) -> Result<Self, CameraError> {
        Ok(Self {
            eye: [camera.eye.x, camera.eye.y, camera.eye.z],
            rotation: camera.view_basis()?,
        })
    }
}

/// Which point the coordinates sent to the GPU are measured from, in the precision tests.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderOrigin {
    /// The eye of the camera: the transform is camera-relative, the large terms cancel in `f64`.
    Eye,
    /// The world origin, as a classic pipeline does: the negative control of these tests, which
    /// shows the jitter the camera-relative transform removes.
    World,
}

#[cfg(test)]
impl RenderOrigin {
    /// The model-view matrix of a mesh with origin `mesh_origin` placed by `placement`.
    fn model_view(self, view: &ViewFrame, mesh_origin: [f64; 3], placement: &Placement) -> Matrix {
        match self {
            Self::Eye => camera_relative_model_view(view, mesh_origin, placement),
            Self::World => naive_model_view(view, mesh_origin, placement),
        }
    }
}

/// The model-view matrix with the camera at the origin, all in `f64`, rounded once to `f32`.
///
/// A local vertex `v` is at `R · (o + v) + t` in the world and at `V · (R · (o + v) + t − eye)`
/// from the camera, which is `(V · R) · v + V · (R · o + (t − eye))`: the large `t` and `eye`
/// cancel before anything is rounded to `f32`.
pub(crate) fn camera_relative_model_view(
    view: &ViewFrame,
    mesh_origin: [f64; 3],
    placement: &Placement,
) -> Matrix {
    let rotation = mul(&view.rotation, &placement.rotation);
    let offset = add(
        apply(&placement.rotation, mesh_origin),
        sub(placement.translation, view.eye),
    );
    to_gpu(&rotation, apply(&view.rotation, offset))
}

/// The model-view matrix of a classic pipeline: the world model matrix and the world view matrix
/// are each rounded to `f32`, then multiplied in `f32`, so world coordinates reach `f32`.
#[cfg(test)]
fn naive_model_view(view: &ViewFrame, mesh_origin: [f64; 3], placement: &Placement) -> Matrix {
    let model = to_gpu(
        &placement.rotation,
        add(
            apply(&placement.rotation, mesh_origin),
            placement.translation,
        ),
    );
    let eye_in_view = apply(&view.rotation, view.eye);
    let world_view = to_gpu(&view.rotation, eye_in_view.map(|v| -v));
    mul_f32(&world_view, &model)
}

/// The projection matrix, reverse-Z, for WebGPU clip space (depth 0 to 1): depth is 1 at the
/// near plane and decreases with the distance (`docs/dev/adr/0020-camera-relative-rendering.md`).
///
/// The perspective projection has an infinite far plane: depth is `near / distance`. The
/// orthographic projection maps the near plane to 1 and the far plane to 0. `aspect` is the
/// width of the target divided by its height.
pub(crate) fn projection_matrix(
    projection: &Projection,
    aspect: f64,
) -> Result<Matrix, CameraError> {
    projection.validate()?;
    if !ASPECT_RANGE.contains(&aspect) {
        return Err(CameraError::InvalidAspect);
    }
    // Columns, as on the GPU.
    let columns: [[f64; 4]; 4] = match *projection {
        Projection::Perspective {
            fov_y_radians,
            near,
        } => {
            let focal = 1.0 / libm::tan(fov_y_radians / 2.0);
            [
                [focal / aspect, 0.0, 0.0, 0.0],
                [0.0, focal, 0.0, 0.0],
                [0.0, 0.0, 0.0, -1.0],
                [0.0, 0.0, near, 0.0],
            ]
        }
        Projection::Orthographic { height, near, far } => {
            let half_height = height / 2.0;
            let depth = far - near;
            [
                [1.0 / (half_height * aspect), 0.0, 0.0, 0.0],
                [0.0, 1.0 / half_height, 0.0, 0.0],
                [0.0, 0.0, 1.0 / depth, 0.0],
                [0.0, 0.0, far / depth, 1.0],
            ]
        }
    };
    // The ranges of the projection and of the aspect keep every entry a normal f32 number.
    Ok(columns.map(|column| column.map(to_f32)))
}

/// The only conversion from `f64` to `f32` of the transforms.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the GPU computes in f32; values are camera distances, not world coordinates"
)]
fn to_f32(value: f64) -> f32 {
    value as f32
}

/// The 4×4 matrix `[rotation | translation]` in `f32`, column-major.
fn to_gpu(rotation: &Rotation, translation: [f64; 3]) -> Matrix {
    core::array::from_fn(|column| {
        match column {
            3 => [translation[0], translation[1], translation[2], 1.0],
            _ => [
                rotation[0][column],
                rotation[1][column],
                rotation[2][column],
                0.0,
            ],
        }
        .map(to_f32)
    })
}

/// `a · b` in `f32`, as the GPU would multiply them.
#[cfg(test)]
fn mul_f32(a: &Matrix, b: &Matrix) -> Matrix {
    b.map(|column| mul_vector(a, column))
}

/// `m · v` in `f32`, summing the columns in order, without fused multiply-add: the operations of
/// the vertex shader.
#[cfg(test)]
fn mul_vector(m: &Matrix, v: [f32; 4]) -> [f32; 4] {
    core::array::from_fn(|row| {
        m[0][row] * v[0] + m[1][row] * v[1] + m[2][row] * v[2] + m[3][row] * v[3]
    })
}

#[cfg(test)]
mod tests {
    use core::f64::consts::{FRAC_PI_3, TAU};

    use geom::{Point3, Vector3};
    use proptest::prelude::*;

    use super::*;

    /// Side of the square viewport of the precision tests, in pixels.
    const VIEWPORT: f64 = 256.0;

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: FRAC_PI_3,
        near: 0.1,
    };

    /// The translations of the scene that the precision tests check.
    const FAR_AWAY: [[f64; 3]; 2] = [[1e6, 1e6, 1e2], [1e7, 1e7, 1e2]];

    fn rotation_z(angle: f64) -> Rotation {
        let (s, c) = (libm::sin(angle), libm::cos(angle));
        [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]]
    }

    fn point(p: [f64; 3]) -> Point3 {
        Point3::new(p[0], p[1], p[2])
    }

    /// A test scene, translated by `shift`: a mesh with an origin off the `f32` grid, placed with
    /// a rotation, seen from about 3 m.
    struct Scene {
        mesh_origin: [f64; 3],
        placement: Placement,
        camera: Camera,
    }

    impl Scene {
        fn at(shift: [f64; 3]) -> Self {
            Self {
                mesh_origin: [0.0123, 0.0456, 0.789],
                placement: Placement {
                    rotation: rotation_z(0.3),
                    translation: add(shift, [0.031, 0.017, 0.0]),
                },
                camera: Camera {
                    eye: point(add(shift, [0.4, -3.1, 1.6])),
                    target: point(add(shift, [0.0, 0.0, 0.8])),
                    up: Vector3::new(0.0, 0.0, 1.0),
                    projection: PERSPECTIVE,
                },
            }
        }

        fn model_view(&self, origin: RenderOrigin) -> Matrix {
            let view = ViewFrame::new(&self.camera).unwrap();
            origin.model_view(&view, self.mesh_origin, &self.placement)
        }

        /// Where the GPU draws local vertex `v`, in pixels: the operations of the vertex shader
        /// in `f32`, then the perspective divide and the viewport transform.
        fn gpu_pixel(&self, origin: RenderOrigin, v: [f32; 3]) -> [f64; 2] {
            let projection = projection_matrix(&self.camera.projection, 1.0).unwrap();
            let eye_space = mul_vector(&self.model_view(origin), [v[0], v[1], v[2], 1.0]);
            let clip = mul_vector(&projection, eye_space);
            [clip[0] / clip[3], clip[1] / clip[3]].map(|ndc| to_pixel(f64::from(ndc)))
        }

        /// Where local vertex `v` truly is on the screen, in pixels, computed in `f64` from the
        /// definition of the camera.
        fn exact_pixel(&self, v: [f32; 3]) -> [f64; 2] {
            let local = add(self.mesh_origin, v.map(f64::from));
            let world = add(
                apply(&self.placement.rotation, local),
                self.placement.translation,
            );
            let eye = [self.camera.eye.x, self.camera.eye.y, self.camera.eye.z];
            let basis = self.camera.view_basis().unwrap();
            let camera = apply(&basis, sub(world, eye));
            let focal = 1.0 / libm::tan(FRAC_PI_3 / 2.0);
            [camera[0], camera[1]].map(|c| to_pixel(focal * c / -camera[2]))
        }

        /// The largest screen error of the GPU over sample vertices, in pixels.
        fn max_error(&self, origin: RenderOrigin) -> f64 {
            samples()
                .map(|v| {
                    let (gpu, exact) = (self.gpu_pixel(origin, v), self.exact_pixel(v));
                    (gpu[0] - exact[0]).abs().max((gpu[1] - exact[1]).abs())
                })
                .fold(0.0, f64::max)
        }
    }

    fn to_pixel(ndc: f64) -> f64 {
        (ndc * 0.5 + 0.5) * VIEWPORT
    }

    /// Local vertices of a 1 m element, at offsets that are not on any power-of-two grid.
    fn samples() -> impl Iterator<Item = [f32; 3]> {
        let steps = [-0.4137_f32, 0.0291, 0.3779];
        steps.into_iter().flat_map(move |x| {
            steps
                .into_iter()
                .flat_map(move |y| steps.into_iter().map(move |z| [x, y, z]))
        })
    }

    /// One `f32` ulp at the scale of an entry: the spacing of `f32` values at
    /// `max(|a|, |b|, 1)`. A strict ulp of each entry cannot hold for entries that should be 0:
    /// the `f64` rounding of `target − eye` far from the origin makes them about 10⁻¹⁰.
    fn ulp_at_scale(a: f32, b: f32) -> f32 {
        let scale = a.abs().max(b.abs()).max(1.0);
        scale.next_up() - scale
    }

    fn assert_within_one_ulp(a: &Matrix, b: &Matrix) {
        for (column_a, column_b) in a.iter().zip(b) {
            for (&x, &y) in column_a.iter().zip(column_b) {
                assert!(
                    (x - y).abs() <= ulp_at_scale(x, y),
                    "{x} and {y} differ by more than one ulp in {a:?} and {b:?}"
                );
            }
        }
    }

    #[test]
    fn the_model_view_moves_the_camera_to_the_origin() {
        let camera = Camera {
            eye: Point3::new(0.0, -3.0, 1.0),
            target: Point3::new(0.0, 0.0, 1.0),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: PERSPECTIVE,
        };
        let view = ViewFrame::new(&camera).unwrap();
        let placement = Placement::from_translation([0.0, 0.0, 0.5]);
        let model_view = camera_relative_model_view(&view, [0.0, 0.0, 0.5], &placement);
        // World x stays right, world z becomes up, world y points away from the camera.
        assert_eq!(
            model_view,
            [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, -3.0, 1.0],
            ]
        );
    }

    #[test]
    fn rotations_of_the_placement_reach_the_model_view() {
        let scene = Scene::at([0.0; 3]);
        let model_view = scene.model_view(RenderOrigin::Eye);
        let basis = scene.camera.view_basis().unwrap();
        let expected = mul(&basis, &rotation_z(0.3));
        for column in 0..3 {
            for row in 0..3 {
                assert_eq!(
                    model_view[column][row].to_bits(),
                    to_f32(expected[row][column]).to_bits()
                );
            }
            assert_eq!(model_view[column][3].to_bits(), 0.0_f32.to_bits());
        }
        assert_eq!(model_view[3][3].to_bits(), 1.0_f32.to_bits());
    }

    #[test]
    fn an_invalid_camera_has_no_view() {
        let camera = Camera {
            target: Point3::new(0.0, -3.1, 1.6),
            ..Scene::at([0.0; 3]).camera
        };
        let camera = Camera {
            eye: camera.target,
            ..camera
        };
        assert_eq!(ViewFrame::new(&camera), Err(CameraError::EyeAtTarget));
    }

    #[test]
    fn both_origins_agree_near_the_world_origin() {
        let scene = Scene::at([0.0; 3]);
        assert_within_one_ulp(
            &scene.model_view(RenderOrigin::Eye),
            &scene.model_view(RenderOrigin::World),
        );
    }

    #[test]
    fn translating_scene_and_camera_far_away_leaves_the_model_view_unchanged() {
        let reference = Scene::at([0.0; 3]).model_view(RenderOrigin::Eye);
        for shift in FAR_AWAY {
            let far = Scene::at(shift).model_view(RenderOrigin::Eye);
            assert_within_one_ulp(&reference, &far);
        }
    }

    #[test]
    fn the_naive_model_view_changes_far_away() {
        let reference = Scene::at([0.0; 3]).model_view(RenderOrigin::World);
        let far = Scene::at(FAR_AWAY[0]).model_view(RenderOrigin::World);
        let drift = (reference[3][0] - far[3][0])
            .abs()
            .max((reference[3][1] - far[3][1]).abs())
            .max((reference[3][2] - far[3][2]).abs());
        assert!(drift > 1e-3, "{drift}");
    }

    #[test]
    fn camera_relative_vertices_land_within_a_hundredth_of_a_pixel() {
        for shift in [[0.0; 3], FAR_AWAY[0], FAR_AWAY[1]] {
            let error = Scene::at(shift).max_error(RenderOrigin::Eye);
            assert!(error < 0.01, "{error} px at {shift:?}");
        }
    }

    #[test]
    fn naive_vertices_jump_by_more_than_a_pixel_far_away() {
        let error = Scene::at([0.0; 3]).max_error(RenderOrigin::World);
        assert!(error < 0.01, "{error} px at the origin");
        for shift in FAR_AWAY {
            let error = Scene::at(shift).max_error(RenderOrigin::World);
            assert!(error > 1.0, "{error} px at {shift:?}");
        }
    }

    #[test]
    fn the_perspective_projection_is_reverse_z_with_an_infinite_far_plane() {
        let projection = Projection::Perspective {
            fov_y_radians: FRAC_PI_3,
            near: 0.5,
        };
        let matrix = projection_matrix(&projection, 2.0).unwrap();
        let focal = to_f32(1.0 / libm::tan(FRAC_PI_3 / 2.0));
        assert_eq!(
            matrix,
            [
                [focal / 2.0, 0.0, 0.0, 0.0],
                [0.0, focal, 0.0, 0.0],
                [0.0, 0.0, 0.0, -1.0],
                [0.0, 0.0, 0.5, 0.0],
            ]
        );
        let depth = |distance: f32| {
            let clip = mul_vector(&matrix, [0.0, 0.0, -distance, 1.0]);
            clip[2] / clip[3]
        };
        assert_eq!(depth(0.5).to_bits(), 1.0_f32.to_bits());
        let mut previous = 1.0;
        for distance in [1.0, 10.0, 1e3, 1e6, 1e9] {
            let d = depth(distance);
            assert!(d > 0.0 && d < previous, "{d} at {distance} m");
            previous = d;
        }
    }

    #[test]
    fn the_orthographic_projection_is_reverse_z() {
        let projection = Projection::Orthographic {
            height: 4.0,
            near: 1.0,
            far: 9.0,
        };
        let matrix = projection_matrix(&projection, 0.5).unwrap();
        let clip = |p: [f32; 3]| mul_vector(&matrix, [p[0], p[1], p[2], 1.0]);
        assert_eq!(clip([0.0, 0.0, -1.0])[2].to_bits(), 1.0_f32.to_bits());
        assert_eq!(clip([0.0, 0.0, -9.0])[2].to_bits(), 0.0_f32.to_bits());
        assert_eq!(clip([0.0, 0.0, -5.0])[2].to_bits(), 0.5_f32.to_bits());
        // The view is 4 m high and 2 m wide.
        assert_eq!(
            clip([1.0, 2.0, -5.0]).map(f32::to_bits),
            [1.0_f32, 1.0, 0.5, 1.0].map(f32::to_bits)
        );
    }

    #[test]
    fn invalid_projections_and_aspects_are_rejected() {
        for aspect in [0.0, -1.0, 9e-5, 1.1e4, f64::NAN, f64::INFINITY] {
            assert_eq!(
                projection_matrix(&PERSPECTIVE, aspect),
                Err(CameraError::InvalidAspect)
            );
        }
        let flat = Projection::Orthographic {
            height: 0.0,
            near: 1.0,
            far: 2.0,
        };
        assert_eq!(
            projection_matrix(&flat, 1.0),
            Err(CameraError::InvalidHeight)
        );
    }

    /// Checks that the non-zero entries of a projection, at `cells` (column, row), are normal
    /// `f32` numbers and every other entry is 0.
    fn assert_representable(matrix: &Matrix, cells: &[(usize, usize)]) {
        for (column, values) in matrix.iter().enumerate() {
            for (row, value) in values.iter().enumerate() {
                if cells.contains(&(column, row)) {
                    assert!(value.is_normal(), "{matrix:?}");
                } else {
                    assert_eq!(value.to_bits(), 0.0_f32.to_bits(), "{matrix:?}");
                }
            }
        }
    }

    #[test]
    fn the_most_extreme_valid_projections_fit_in_f32() {
        let (min, max) = (1e-6, crate::MAX_WORLD_COORDINATE);
        for aspect in [1e-4, 1e4] {
            for fov_y_radians in [min, core::f64::consts::PI - min] {
                for near in [min, max] {
                    let projection = Projection::Perspective {
                        fov_y_radians,
                        near,
                    };
                    let matrix = projection_matrix(&projection, aspect).unwrap();
                    assert_representable(&matrix, &[(0, 0), (1, 1), (2, 3), (3, 2)]);
                }
            }
            for height in [min, max] {
                for (near, far) in [(min, 2.0 * min), (min, max), (max - 1.0, max)] {
                    let projection = Projection::Orthographic { height, near, far };
                    let matrix = projection_matrix(&projection, aspect).unwrap();
                    assert_representable(&matrix, &[(0, 0), (1, 1), (2, 2), (3, 2), (3, 3)]);
                }
            }
        }
    }

    fn small() -> impl Strategy<Value = [f64; 3]> {
        proptest::array::uniform3(-50.0..50.0)
    }

    proptest! {
        #![proptest_config(crate::proptest_config())]

        /// Translating the scene and the camera by the same vector, up to 10⁷ m away, leaves
        /// the camera-relative model-view within one ulp (at the scale of each entry), whether
        /// the translation is carried by the placement or by the mesh origin.
        #[test]
        fn the_model_view_is_invariant_under_translation(
            shift in proptest::array::uniform3(-1e7..1e7),
            mesh_origin in small(),
            translation in small(),
            target in small(),
            angle in 0.0..TAU,
            distance in 0.5..20.0,
            heading in 0.0..TAU,
            height in -10.0..10.0,
            in_mesh_origin in any::<bool>(),
        ) {
            let eye = add(target, [distance * libm::cos(heading), distance * libm::sin(heading), height]);
            let build = |shift: [f64; 3]| {
                let (origin, translation) = if in_mesh_origin {
                    // An identity placement, as for a mesh built from world positions.
                    (add(add(mesh_origin, translation), shift), [0.0; 3])
                } else {
                    (mesh_origin, add(translation, shift))
                };
                let rotation = if in_mesh_origin { Placement::IDENTITY.rotation } else { rotation_z(angle) };
                let camera = Camera {
                    eye: point(add(eye, shift)),
                    target: point(add(target, shift)),
                    up: Vector3::new(0.0, 0.0, 1.0),
                    projection: PERSPECTIVE,
                };
                let view = ViewFrame::new(&camera).unwrap();
                camera_relative_model_view(&view, origin, &Placement { rotation, translation })
            };
            assert_within_one_ulp(&build([0.0; 3]), &build(shift));
        }
    }
}
