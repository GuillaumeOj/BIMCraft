//! The view frustum, tested against world bounds in `f64` relative to the camera (VP-02, VP-04).
//!
//! Each plane is a world normal `n` and an offset `d`: a point `p` is inside when
//! `n · (p − eye) + d ≥ 0`. Subtracting the eye first makes the large coordinates of a model far
//! from the origin cancel in `f64`, as in the transforms of ADR 0020, so culling is as exact at
//! 10⁷ m as at the origin.

use crate::Projection;
use crate::bounds::Aabb;
use crate::bvh::Overlap;
use crate::transform::ViewFrame;
use crate::vector::{dot, norm, sub};

/// The relative margin a box may lie outside a plane by and still count as inside: it absorbs
/// the rounding of the plane and of the bounds, so culling never removes a visible pixel.
const MARGIN: f64 = 1e-9;

#[derive(Clone, Copy, Debug)]
struct Plane {
    normal: [f64; 3],
    /// The absolute values of the normal's components and its length, computed once per frame.
    abs_normal: [f64; 3],
    length: f64,
    offset: f64,
}

/// The planes of a camera's view volume: left, right, bottom, top and near, plus far for an
/// orthographic camera (a perspective one has an infinite far plane).
#[derive(Clone, Debug)]
pub(crate) struct Frustum {
    eye: [f64; 3],
    planes: [Plane; 6],
    count: usize,
}

impl Frustum {
    /// The frustum of a validated camera's view and projection, for a target of `aspect`
    /// (width over height).
    pub(crate) fn new(view: &ViewFrame, projection: &Projection, aspect: f64) -> Self {
        Self::window(view, projection, aspect, [-1.0, -1.0, 1.0, 1.0])
    }

    /// The part of that frustum seen through the window `[left, bottom, right, top]` of the
    /// view, in normalised device coordinates (the whole view is `[-1, -1, 1, 1]`): the volume
    /// a pick or a snap under the pointer looks into. The window may reach beyond the view.
    pub(crate) fn window(
        view: &ViewFrame,
        projection: &Projection,
        aspect: f64,
        [left, bottom, right, top]: [f64; 4],
    ) -> Self {
        // Planes in camera coordinates (x right, y up, looking down −z), inside when
        // `n · q + d ≥ 0` for a point `q` relative to the eye.
        let (camera_planes, count): ([([f64; 3], f64); 6], usize) = match *projection {
            Projection::Perspective {
                fov_y_radians,
                near,
            } => {
                // In view at depth −z when x lies between left·tan_x·(−z) and right·tan_x·(−z).
                let tan_y = libm::tan(fov_y_radians / 2.0);
                let tan_x = tan_y * aspect;
                (
                    [
                        ([1.0, 0.0, left * tan_x], 0.0),
                        ([-1.0, 0.0, -right * tan_x], 0.0),
                        ([0.0, 1.0, bottom * tan_y], 0.0),
                        ([0.0, -1.0, -top * tan_y], 0.0),
                        ([0.0, 0.0, -1.0], -near),
                        ([0.0; 3], 0.0),
                    ],
                    5,
                )
            }
            Projection::Orthographic { height, near, far } => {
                let half_y = height / 2.0;
                let half_x = half_y * aspect;
                (
                    [
                        ([1.0, 0.0, 0.0], -left * half_x),
                        ([-1.0, 0.0, 0.0], right * half_x),
                        ([0.0, 1.0, 0.0], -bottom * half_y),
                        ([0.0, -1.0, 0.0], top * half_y),
                        ([0.0, 0.0, -1.0], -near),
                        ([0.0, 0.0, 1.0], far),
                    ],
                    6,
                )
            }
        };
        // `n · (V · r) = (Vᵀ · n) · r`: the world normal is the transposed view rotation applied
        // to the camera normal.
        let rows = &view.rotation;
        let planes = camera_planes.map(|(normal, offset)| {
            let normal: [f64; 3] =
                core::array::from_fn(|axis| (0..3).map(|k| rows[k][axis] * normal[k]).sum());
            Plane {
                normal,
                abs_normal: normal.map(f64::abs),
                length: norm(normal),
                offset,
            }
        });
        Self {
            eye: view.eye,
            planes,
            count,
        }
    }

    /// The mask of every plane, the state a query starts from.
    pub(crate) fn all_planes(&self) -> u8 {
        (1 << self.count) - 1
    }

    /// Where `bounds` lies, testing only the planes of `mask`: outside one of them, inside all
    /// of them, or across some, with the mask of the planes it crosses.
    pub(crate) fn classify(&self, bounds: &Aabb, mask: u8) -> Overlap<u8> {
        let centre = sub(bounds.centre(), self.eye);
        let half = bounds.half_extents();
        let extent = norm(centre) + norm(half);
        let mut crossing = 0;
        for (index, plane) in self.planes[..self.count].iter().enumerate() {
            let bit = 1 << index;
            if mask & bit == 0 {
                continue;
            }
            let distance = dot(plane.normal, centre) + plane.offset;
            let reach = dot(plane.abs_normal, half);
            let margin = MARGIN * (plane.length * extent + plane.offset.abs());
            if distance + reach < -margin {
                return Overlap::Outside;
            }
            if distance - reach < margin {
                crossing |= bit;
            }
        }
        if crossing == 0 {
            Overlap::Inside
        } else {
            Overlap::Partial(crossing)
        }
    }
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use geom::{Point3, Vector3};

    use super::*;
    use crate::Camera;

    /// A camera at `eye` looking along +y, with a 90° vertical field of view: at a distance `d`
    /// the view spans `±d` vertically and `±d·aspect` horizontally.
    fn frustum(eye: [f64; 3], projection: Projection, aspect: f64) -> Frustum {
        let camera = Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(eye[0], eye[1] + 10.0, eye[2]),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection,
        };
        Frustum::new(&ViewFrame::new(&camera).unwrap(), &projection, aspect)
    }

    /// Whether `bounds` is at least partly inside every plane.
    fn sees(frustum: &Frustum, bounds: &Aabb) -> bool {
        !matches!(
            frustum.classify(bounds, frustum.all_planes()),
            Overlap::Outside
        )
    }

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: FRAC_PI_2,
        near: 0.5,
    };

    const ORTHOGRAPHIC: Projection = Projection::Orthographic {
        height: 4.0,
        near: 1.0,
        far: 50.0,
    };

    /// A box of side `side` centred on `centre`.
    fn at(eye: [f64; 3], centre: [f64; 3], side: f64) -> Aabb {
        let centre: [f64; 3] = core::array::from_fn(|axis| eye[axis] + centre[axis]);
        Aabb {
            min: centre.map(|v| v - side / 2.0),
            max: centre.map(|v| v + side / 2.0),
        }
    }

    #[test]
    fn perspective_planes_bound_the_view_at_any_distance_from_the_origin() {
        for eye in [[0.0; 3], [1e6, 1e6, 1e2], [1e7, -1e7, 1e2]] {
            let frustum = frustum(eye, PERSPECTIVE, 2.0);
            // 10 m ahead the view spans x in ±20 m and z in ±10 m.
            let cases = [
                ([0.0, 10.0, 0.0], true),
                ([19.9, 10.0, 0.0], true),
                ([20.1, 10.0, 0.0], false),
                ([-20.1, 10.0, 0.0], false),
                ([0.0, 10.0, 9.9], true),
                ([0.0, 10.0, 10.1], false),
                ([0.0, 10.0, -10.1], false),
                // Behind the near plane, and behind the camera.
                ([0.0, 0.6, 0.0], true),
                ([0.0, 0.4, 0.0], false),
                ([0.0, -10.0, 0.0], false),
                // Infinitely far is still in view.
                ([0.0, 1e8, 0.0], true),
            ];
            for (centre, inside) in cases {
                let bounds = at(eye, centre, 0.001);
                assert_eq!(sees(&frustum, &bounds), inside, "{centre:?} from {eye:?}");
            }
        }
    }

    #[test]
    fn orthographic_planes_include_the_far_plane() {
        let eye = [1e7, 1e7, 0.0];
        let frustum = frustum(eye, ORTHOGRAPHIC, 1.5);
        // The view is 4 m high and 6 m wide, from 1 m to 50 m ahead.
        let cases = [
            ([2.9, 20.0, 1.9], true),
            ([3.1, 20.0, 0.0], false),
            ([0.0, 20.0, -2.1], false),
            ([0.0, 0.9, 0.0], false),
            ([0.0, 49.9, 0.0], true),
            ([0.0, 50.1, 0.0], false),
        ];
        for (centre, inside) in cases {
            assert_eq!(
                sees(&frustum, &at(eye, centre, 0.001)),
                inside,
                "{centre:?}"
            );
        }
    }

    #[test]
    fn a_box_across_a_plane_is_partial_with_that_plane_only() {
        let frustum = frustum([0.0; 3], PERSPECTIVE, 1.0);
        let all = frustum.all_planes();
        assert_eq!(all, 0b1_1111);
        assert!(matches!(
            frustum.classify(&at([0.0; 3], [0.0, 10.0, 0.0], 1.0), all),
            Overlap::Inside
        ));
        // Across the right plane (x = 10 at 10 m).
        let Overlap::Partial(mask) = frustum.classify(&at([0.0; 3], [10.0, 10.0, 0.0], 1.0), all)
        else {
            panic!("expected a partial overlap");
        };
        assert_eq!(mask, 0b10);
        // Planes left out of the mask are not tested: inside the others is inside.
        assert!(matches!(
            frustum.classify(&at([0.0; 3], [10.0, 10.0, 0.0], 1.0), all & !mask),
            Overlap::Inside
        ));
        // A huge box around the camera crosses every plane.
        assert!(matches!(
            frustum.classify(&at([0.0; 3], [0.0; 3], 100.0), all),
            Overlap::Partial(0b1_1111)
        ));
    }
}
