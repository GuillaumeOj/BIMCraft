//! Clipping planes and the tint of the faces they cut (VP-03, spec D6).
//!
//! A plane removes what lies on the side its normal points to. The viewport sends the planes of
//! the section box (six, each removing what is outside the box) and the user's planes (up to
//! six) as one list; the fragment shader discards what any plane removes. A solid cut open shows
//! the inside of its far faces, which the shader paints in the flat cut colour, so the cut
//! reads as a filled section.
//!
//! Planes are given in world coordinates in `f64`; the frame sends them to the GPU camera
//! relative, in the view's axes, like every transform of the renderer (ADR 0020).

use crate::in_world;
use crate::theme::Rgb;
use crate::transform::ViewFrame;
use crate::vector::{apply, dot, norm, scale, sub};

/// The most planes a frame clips with: six for the section box, six of the user's.
pub const MAX_CLIP_PLANES: usize = 12;

/// Floats of the clipping block of the frame uniform: the planes, then two vectors of four.
pub(crate) const CLIP_FLOATS: usize = MAX_CLIP_PLANES * 4 + 8;

/// Where `(count, tinted, 0, 0)` starts in the clipping block, in floats: after the planes.
pub(crate) const CLIP_PARAMS: usize = MAX_CLIP_PLANES * 4;

/// Where the cut colour starts in the clipping block, in floats.
pub(crate) const CUT_COLOR: usize = CLIP_PARAMS + 4;

/// A plane that removes what lies on the side of its normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipPlane {
    /// A point of the plane, in world metres.
    pub point: [f64; 3],
    /// The normal, pointing to the side that is removed; any length but zero.
    pub normal: [f64; 3],
}

/// What a cut shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CutFaces {
    /// The inside of a cut solid is painted in the cut colour.
    #[default]
    Tinted,
    /// The inside of a cut solid is drawn as any back face.
    None,
}

/// Why a set of planes cannot be used. Messages never contain coordinates (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ClipError {
    /// More than [`MAX_CLIP_PLANES`].
    #[error("a frame clips with at most {MAX_CLIP_PLANES} planes")]
    TooMany,
    /// A point is not finite or lies beyond the world limit, or a normal is not finite.
    #[error("a clipping plane is not finite or lies too far from the world origin")]
    OutOfRange,
    /// A normal has no direction.
    #[error("the normal of a clipping plane has no direction")]
    ZeroNormal,
}

/// The planes a frame clips with, and how it shows the cut.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    planes: Vec<ClipPlane>,
    cut: CutFaces,
}

impl Clip {
    /// Validates `planes`, whose normals become unit vectors.
    ///
    /// # Errors
    ///
    /// [`ClipError`] for more than [`MAX_CLIP_PLANES`] planes, a point or normal out of range, or
    /// a normal of no direction.
    pub fn new(planes: &[ClipPlane], cut: CutFaces) -> Result<Self, ClipError> {
        if planes.len() > MAX_CLIP_PLANES {
            return Err(ClipError::TooMany);
        }
        let mut unit = Vec::with_capacity(planes.len());
        for plane in planes {
            let finite = plane.normal.iter().all(|v| v.is_finite());
            if !finite || !plane.point.iter().all(|&v| in_world(v)) {
                return Err(ClipError::OutOfRange);
            }
            // Scaled by its largest component first, so a huge normal does not overflow and a
            // tiny one does not underflow when its length is taken.
            let largest = plane.normal.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
            if largest == 0.0 {
                return Err(ClipError::ZeroNormal);
            }
            let scaled = scale(plane.normal, 1.0 / largest);
            unit.push(ClipPlane {
                point: plane.point,
                normal: scale(scaled, 1.0 / norm(scaled)),
            });
        }
        Ok(Self { planes: unit, cut })
    }

    /// The planes, with unit normals.
    #[must_use]
    pub fn planes(&self) -> &[ClipPlane] {
        &self.planes
    }

    /// How the cut is shown.
    #[must_use]
    pub const fn cut(&self) -> CutFaces {
        self.cut
    }

    /// The clipping block of the frame uniform seen through `view`: for each plane the normal in
    /// the view's axes and the signed distance of the eye along it, so a fragment at the
    /// view-space position `p` is removed when `dot(n, p) + d > 0`. Then `(count, tinted, 0, 0)`
    /// and the cut colour.
    pub(crate) fn uniform(&self, view: &ViewFrame, cut: Rgb) -> [f32; CLIP_FLOATS] {
        let mut block = [0.0; CLIP_FLOATS];
        for (slot, plane) in self.planes.iter().enumerate() {
            let normal = apply(&view.rotation, plane.normal);
            // The distance of the eye from the plane: large terms cancel here, in f64.
            let offset = dot(plane.normal, sub(view.eye, plane.point));
            let at = slot * 4;
            for (target, value) in block[at..at + 3].iter_mut().zip(normal) {
                *target = narrow(value);
            }
            block[at + 3] = narrow(offset);
        }
        // At most `MAX_CLIP_PLANES`, so it fits a byte.
        block[CLIP_PARAMS] = f32::from(u8::try_from(self.planes.len()).unwrap_or(0));
        block[CLIP_PARAMS + 1] = f32::from(u8::from(self.cut == CutFaces::Tinted));
        let [r, g, b] = cut.linear();
        block[CUT_COLOR..CUT_COLOR + 4].copy_from_slice(&[r, g, b, 1.0]);
        block
    }
}

/// The planes in the view's axes are unit-sized and the offset is a distance in metres below
/// 2·10⁹: far inside `f32`'s range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "rounded once, after the f64 arithmetic (ADR 0020)"
)]
fn narrow(value: f64) -> f32 {
    value as f32
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the values compared are exact: unit axes, small integers and zeros"
)]
mod tests {
    use super::*;
    use crate::MAX_WORLD_COORDINATE;
    use geom::{Point3, Vector3};

    use crate::{Camera, Projection};

    fn view(eye: [f64; 3]) -> ViewFrame {
        let camera = Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(eye[0], eye[1] + 1.0, eye[2]),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: Projection::Perspective {
                fov_y_radians: 1.0,
                near: 0.1,
            },
        };
        ViewFrame::new(&camera).unwrap()
    }

    fn plane(point: [f64; 3], normal: [f64; 3]) -> ClipPlane {
        ClipPlane { point, normal }
    }

    #[test]
    fn normals_become_unit_vectors_and_bad_planes_are_refused() {
        let clip = Clip::new(&[plane([0.0; 3], [0.0, 0.0, 4.0])], CutFaces::Tinted).unwrap();
        assert_eq!(clip.planes()[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(clip.cut(), CutFaces::Tinted);
        let many = vec![plane([0.0; 3], [1.0, 0.0, 0.0]); MAX_CLIP_PLANES + 1];
        assert_eq!(Clip::new(&many, CutFaces::None), Err(ClipError::TooMany));
        assert!(Clip::new(&many[..MAX_CLIP_PLANES], CutFaces::None).is_ok());
        for bad in [
            plane([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]),
            plane([2.0 * MAX_WORLD_COORDINATE, 0.0, 0.0], [1.0, 0.0, 0.0]),
            plane([0.0; 3], [f64::INFINITY, 0.0, 0.0]),
        ] {
            assert_eq!(
                Clip::new(&[bad], CutFaces::None),
                Err(ClipError::OutOfRange)
            );
        }
        assert_eq!(
            Clip::new(&[plane([0.0; 3], [0.0; 3])], CutFaces::None),
            Err(ClipError::ZeroNormal)
        );
        // Huge and tiny normals keep their direction.
        for scale in [1e200, 1e-200] {
            let clip = Clip::new(&[plane([0.0; 3], [scale, 0.0, 0.0])], CutFaces::None).unwrap();
            assert_eq!(clip.planes()[0].normal, [1.0, 0.0, 0.0]);
        }
    }

    #[test]
    fn the_uniform_holds_view_space_planes_relative_to_the_eye() {
        // The eye 10⁷ m from the origin, looking along +y: view x is world x, view y is world z,
        // view z (backward) is world −y. The plane x = 10⁷ + 2 removes what is beyond it.
        let eye = [1.0e7, 0.0, 0.0];
        let clip = Clip::new(
            &[plane([1.0e7 + 2.0, 5.0, 5.0], [1.0, 0.0, 0.0])],
            CutFaces::Tinted,
        )
        .unwrap();
        let block = clip.uniform(&view(eye), Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(block[..4], [1.0, 0.0, 0.0, -2.0]);
        // The first fragment 3 m to the right of the eye is removed, one 1 m to its right is not.
        let removed = |x: f32| block[0] * x + block[3] > 0.0;
        assert!(removed(3.0) && !removed(1.0));
        assert_eq!(block[CLIP_PARAMS..CLIP_PARAMS + 2], [1.0, 1.0]);
        assert_eq!(block[CUT_COLOR..CUT_COLOR + 4], [1.0, 0.0, 0.0, 1.0]);
        // Unused slots are zero, so they would never remove anything.
        assert!(block[4..CLIP_PARAMS].iter().all(|&v| v == 0.0));
    }

    /// What the shader computes for a world point `q`: `dot(n, p) + d` with `p = R · (q − eye)`
    /// its camera-space position.
    fn shader_side(block: &[f32; CLIP_FLOATS], view: &ViewFrame, q: [f64; 3]) -> f32 {
        let p = apply(&view.rotation, sub(q, view.eye)).map(narrow);
        block[0] * p[0] + block[1] * p[1] + block[2] * p[2] + block[3]
    }

    #[test]
    fn the_shader_removes_the_side_of_the_normal_in_any_view() {
        // A Top view looking down −z with +y up, and an oblique view from below.
        let views = [
            ([3.0, 4.0, 20.0], [3.0, 4.0, 0.0], [0.0, 1.0, 0.0]),
            ([-7.0, 9.0, -4.0], [1.0, 2.0, 3.0], [0.0, 0.0, 1.0]),
        ];
        let planes = [
            plane([5.0, 4.0, 0.0], [1.0, 0.0, 0.0]),
            plane([0.0, 0.0, 1.5], [0.0, 0.0, -1.0]),
            plane([1.0, 1.0, 1.0], [1.0, -2.0, 0.5]),
        ];
        for (eye, target, up) in views {
            let camera = Camera {
                eye: Point3::new(eye[0], eye[1], eye[2]),
                target: Point3::new(target[0], target[1], target[2]),
                up: Vector3::new(up[0], up[1], up[2]),
                projection: Projection::Orthographic {
                    height: 10.0,
                    near: 0.1,
                    far: 100.0,
                },
            };
            let view = ViewFrame::new(&camera).unwrap();
            for plane in planes {
                let clip = Clip::new(&[plane], CutFaces::Tinted).unwrap();
                let block = clip.uniform(&view, Rgb { r: 0, g: 0, b: 0 });
                let n = clip.planes()[0].normal;
                for offset in [-2.0, -0.5, 0.5, 2.0] {
                    let q = [0, 1, 2].map(|i| plane.point[i] + n[i] * offset + 0.3);
                    let world = dot(n, sub(q, plane.point));
                    let side = shader_side(&block, &view, q);
                    assert_eq!(side > 0.0, world > 0.0, "{eye:?} {plane:?} {offset}");
                    assert!((f64::from(side) - world).abs() < 1e-4);
                }
            }
        }
    }

    #[test]
    fn the_top_view_axes_are_east_north_and_up() {
        // Looking down −z with north up, the camera's right is +x and its backward axis +z: a
        // plane removing what is above z = 1 removes, in the camera space, what is nearer the eye.
        let camera = Camera {
            eye: Point3::new(0.0, 0.0, 10.0),
            target: Point3::new(0.0, 0.0, 0.0),
            up: Vector3::new(0.0, 1.0, 0.0),
            projection: Projection::Perspective {
                fov_y_radians: 1.0,
                near: 0.1,
            },
        };
        let view = ViewFrame::new(&camera).unwrap();
        assert_eq!(
            view.rotation,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        let clip = Clip::new(&[plane([0.0, 0.0, 1.0], [0.0, 0.0, 1.0])], CutFaces::Tinted).unwrap();
        let block = clip.uniform(&view, Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(block[..4], [0.0, 0.0, 1.0, 9.0]);
    }

    #[test]
    fn untinted_cuts_say_so_and_an_empty_clip_clips_nothing() {
        let clip = Clip::new(&[], CutFaces::None).unwrap();
        let block = clip.uniform(&view([0.0; 3]), Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(block[CLIP_PARAMS..CLIP_PARAMS + 2], [0.0, 0.0]);
        assert_eq!(Clip::default().cut(), CutFaces::Tinted);
    }
}
