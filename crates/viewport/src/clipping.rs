//! The section box and the clipping planes (VP-03, spec D6): the state, the planes it sends to
//! the renderer, which points it clips (for picking and snapping), the drag of a face handle and
//! the lines drawn for them. Everything is `f64`; points that depend on the eye are handled
//! relative to it, as the rest of the viewport (ADR 0020).

use render::{ClipPlane, CutFaces, MAX_CLIP_PLANES, MAX_WORLD_COORDINATE};

use crate::math::{Vec3, add, cross, dot, normalize, scale, sub};
use crate::view::Ray;

/// The most planes the user adds: the other six of the renderer's twelve are the box's.
pub(crate) const MAX_USER_PLANES: usize = MAX_CLIP_PLANES / 2;

/// A point on a plane is not clipped: this much beyond it is, in metres. Rounding of the planes
/// and of the points never hides a face the plane was picked on.
const CLIP_TOLERANCE: f64 = 1e-6;

/// The smallest extent of the section box on an axis, in metres.
const MIN_EXTENT: f64 = 1e-3;

/// The step of a handle drag with Shift, in metres (board 3.1: 100 mm).
pub(crate) const SHIFT_STEP: f64 = 0.1;

/// The margin "fit to selection" leaves around the selection, in metres.
const FIT_MARGIN: f64 = 0.1;

/// A face of the section box: an axis (0 x, 1 y, 2 z) and its upper or lower side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Face {
    pub(crate) axis: usize,
    pub(crate) upper: bool,
}

impl Face {
    pub(crate) const ALL: [Self; 6] = [
        Self {
            axis: 0,
            upper: false,
        },
        Self {
            axis: 0,
            upper: true,
        },
        Self {
            axis: 1,
            upper: false,
        },
        Self {
            axis: 1,
            upper: true,
        },
        Self {
            axis: 2,
            upper: false,
        },
        Self {
            axis: 2,
            upper: true,
        },
    ];

    /// The name the announcement gives: `x_min`, `z_max`…
    pub(crate) fn name(self) -> &'static str {
        match (self.axis, self.upper) {
            (0, false) => "x_min",
            (0, true) => "x_max",
            (1, false) => "y_min",
            (1, true) => "y_max",
            (_, false) => "z_min",
            (_, true) => "z_max",
        }
    }
}

/// The section box: axis-aligned with the project.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SectionBox {
    pub(crate) enabled: bool,
    pub(crate) min: Vec3,
    pub(crate) max: Vec3,
}

impl SectionBox {
    /// The centre of a face, where its handle sits.
    pub(crate) fn face_centre(&self, face: Face) -> Vec3 {
        let mut centre = core::array::from_fn(|a| f64::midpoint(self.min[a], self.max[a]));
        centre[face.axis] = if face.upper {
            self.max[face.axis]
        } else {
            self.min[face.axis]
        };
        centre
    }

    /// The coordinate of a face.
    pub(crate) fn coordinate(&self, face: Face) -> f64 {
        if face.upper {
            self.max[face.axis]
        } else {
            self.min[face.axis]
        }
    }

    /// Moves a face to `value`, kept at least [`MIN_EXTENT`] from the opposite one; the
    /// coordinate it ends at.
    pub(crate) fn move_face(&mut self, face: Face, value: f64) -> f64 {
        let value = value.clamp(-MAX_WORLD_COORDINATE, MAX_WORLD_COORDINATE);
        let a = face.axis;
        if face.upper {
            self.max[a] = value.max(self.min[a] + MIN_EXTENT);
            self.max[a]
        } else {
            self.min[a] = value.min(self.max[a] - MIN_EXTENT);
            self.min[a]
        }
    }

    /// The twelve edges.
    pub(crate) fn edges(&self) -> Vec<(Vec3, Vec3)> {
        let corner = |bits: usize| -> Vec3 {
            core::array::from_fn(|a| {
                if bits >> a & 1 == 0 {
                    self.min[a]
                } else {
                    self.max[a]
                }
            })
        };
        let mut edges = Vec::with_capacity(12);
        for bits in 0..8 {
            for a in 0..3 {
                if bits >> a & 1 == 0 {
                    edges.push((corner(bits), corner(bits | 1 << a)));
                }
            }
        }
        edges
    }

    /// The planes that keep what is inside.
    fn planes(&self) -> [ClipPlane; 6] {
        Face::ALL.map(|face| {
            let mut normal = [0.0; 3];
            normal[face.axis] = if face.upper { 1.0 } else { -1.0 };
            ClipPlane {
                point: self.face_centre(face),
                normal,
            }
        })
    }
}

/// The box that encloses `min` and `max`, grown by the fit margin.
pub(crate) fn fitted(min: Vec3, max: Vec3) -> SectionBox {
    SectionBox {
        enabled: true,
        min: min.map(|v| v - FIT_MARGIN),
        max: max.map(|v| v + FIT_MARGIN),
    }
}

/// Whether `min` and `max` can be the extents of a box.
pub(crate) fn valid_extents(min: Vec3, max: Vec3) -> bool {
    (0..3).all(|a| {
        min[a].is_finite()
            && max[a].is_finite()
            && min[a].abs() <= MAX_WORLD_COORDINATE
            && max[a].abs() <= MAX_WORLD_COORDINATE
            && min[a] + MIN_EXTENT <= max[a]
    })
}

/// The section box, the user's planes and how a cut shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Clipping {
    pub(crate) section: Option<SectionBox>,
    pub(crate) planes: Vec<ClipPlane>,
    pub(crate) cut: CutFaces,
}

impl Default for Clipping {
    fn default() -> Self {
        Self {
            section: None,
            planes: Vec::new(),
            cut: CutFaces::Tinted,
        }
    }
}

impl Clipping {
    /// Every plane the renderer clips with: the box's, if it is on, then the user's.
    pub(crate) fn all_planes(&self) -> Vec<ClipPlane> {
        let mut all = Vec::with_capacity(MAX_CLIP_PLANES);
        if let Some(section) = self.section.filter(|s| s.enabled) {
            all.extend(section.planes());
        }
        all.extend_from_slice(&self.planes);
        all
    }

    /// Adds a plane; `false` beyond the limit, or for a point or normal that cannot be used.
    pub(crate) fn add(&mut self, point: Vec3, normal: Vec3) -> bool {
        let Some(normal) = normalize(normal) else {
            return false;
        };
        if self.planes.len() >= MAX_USER_PLANES
            || point
                .iter()
                .any(|v| !v.is_finite() || v.abs() > MAX_WORLD_COORDINATE)
        {
            return false;
        }
        self.planes.push(ClipPlane { point, normal });
        true
    }

    pub(crate) fn flip(&mut self, index: usize) -> bool {
        self.planes
            .get_mut(index)
            .map(|plane| plane.normal = scale(plane.normal, -1.0))
            .is_some()
    }

    pub(crate) fn remove(&mut self, index: usize) -> bool {
        (index < self.planes.len())
            .then(|| self.planes.remove(index))
            .is_some()
    }
}

/// Whether any of `planes` removes `point`.
pub(crate) fn planes_clip(planes: &[ClipPlane], point: Vec3) -> bool {
    planes
        .iter()
        .any(|plane| dot(plane.normal, sub(point, plane.point)) > CLIP_TOLERANCE)
}

/// The handle of a face under a pixel: the nearest within `radius` pixels, given the projected
/// centres (`None` when behind the eye).
pub(crate) fn nearest_handle(
    projected: &[(Face, Option<[f64; 2]>)],
    pos: [f64; 2],
    radius: f64,
) -> Option<Face> {
    projected
        .iter()
        .filter_map(|&(face, at)| {
            let at = at?;
            let d = libm::hypot(at[0] - pos[0], at[1] - pos[1]);
            (d <= radius).then_some((d, face))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, face)| face)
}

/// Where along the axis of `face` the pointer ray passes nearest the line through the face's
/// centre: the new coordinate of the face. `None` when the ray runs parallel to the axis.
/// `eye` is the camera position, since the ray is relative to it.
pub(crate) fn drag_coordinate(
    section: &SectionBox,
    face: Face,
    ray: &Ray,
    eye: Vec3,
) -> Option<f64> {
    let centre = sub(section.face_centre(face), eye);
    // Closest points of two lines (Ericson): the ray P + sU and the axis Q + tV.
    let offset = sub(ray.origin, centre);
    let along = ray.direction[face.axis];
    let (toward, back) = (dot(ray.direction, offset), offset[face.axis]);
    // The ray is a unit vector, the axis too: the determinant is 1 − along².
    let denominator = 1.0 - along * along;
    if denominator < 1e-9 {
        return None;
    }
    let t = (back - along * toward) / denominator;
    Some(section.face_centre(face)[face.axis] + t)
}

/// Rounds `value` to the nearest multiple of `step`.
pub(crate) fn snap_to_step(value: f64, step: f64) -> f64 {
    libm::round(value / step) * step
}

/// An orthonormal basis `(u, v)` of the plane with unit normal `normal`.
fn plane_basis(normal: Vec3) -> (Vec3, Vec3) {
    let helper = if normal[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let u = normalize(cross(normal, helper)).unwrap_or([1.0, 0.0, 0.0]);
    (u, cross(normal, u))
}

/// The lines that show a plane: a dashed square of half-side `half` around its point, and an
/// arrow along its normal.
pub(crate) fn plane_lines(plane: &ClipPlane, half: f64) -> Vec<(Vec3, Vec3)> {
    const DASHES: u8 = 8;
    let (u, v) = plane_basis(plane.normal);
    let corner = |su: f64, sv: f64| add(plane.point, add(scale(u, su * half), scale(v, sv * half)));
    let corners = [
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
    ];
    let mut lines = Vec::new();
    for (i, &from) in corners.iter().enumerate() {
        let to = corners[(i + 1) % 4];
        for k in 0..DASHES {
            // Dash and gap alternate: 6 and 4 in the design, here 3 of every 5.
            let at = |f: f64| add(from, scale(sub(to, from), f));
            let start = f64::from(k) / f64::from(DASHES);
            lines.push((at(start), at(start + 0.6 / f64::from(DASHES))));
        }
    }
    let length = half * 0.5;
    let tip = add(plane.point, scale(plane.normal, length));
    lines.push((plane.point, tip));
    for side in [u, scale(u, -1.0)] {
        let back = add(
            tip,
            add(
                scale(plane.normal, -length * 0.25),
                scale(side, length * 0.2),
            ),
        );
        lines.push((tip, back));
    }
    lines
}

/// The size of the square a plane is drawn with: a fraction of the scene, at least a metre.
pub(crate) fn plane_half(scene_radius: f64) -> f64 {
    (scene_radius * 0.5).max(1.0)
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "exact values: axes, sums of small integers"
)]
mod tests {
    use super::*;
    use crate::math::norm;

    impl Clipping {
        fn clips(&self, point: Vec3) -> bool {
            planes_clip(&self.all_planes(), point)
        }

        fn is_active(&self) -> bool {
            !self.all_planes().is_empty()
        }
    }

    fn unit_box() -> SectionBox {
        SectionBox {
            enabled: true,
            min: [0.0; 3],
            max: [2.0, 4.0, 6.0],
        }
    }

    #[test]
    fn the_box_keeps_what_is_inside() {
        let mut clipping = Clipping {
            section: Some(unit_box()),
            ..Clipping::default()
        };
        assert_eq!(clipping.all_planes().len(), 6);
        assert!(!clipping.clips([1.0, 2.0, 3.0]));
        assert!(!clipping.clips([2.0, 4.0, 6.0]), "a face is not clipped");
        for outside in [
            [2.1, 2.0, 3.0],
            [-0.1, 2.0, 3.0],
            [1.0, 2.0, 6.1],
            [1.0, -1.0, 3.0],
        ] {
            assert!(clipping.clips(outside), "{outside:?}");
        }
        clipping.section = Some(SectionBox {
            enabled: false,
            ..unit_box()
        });
        assert!(!clipping.is_active() && !clipping.clips([9.0, 9.0, 9.0]));
    }

    #[test]
    fn the_box_clips_the_same_ten_million_metres_away() {
        let far = 1.0e7;
        let section = SectionBox {
            enabled: true,
            min: [far, far, 0.0],
            max: [far + 2.0, far + 4.0, 6.0],
        };
        let clipping = Clipping {
            section: Some(section),
            ..Clipping::default()
        };
        assert!(!clipping.clips([far + 1.0, far + 2.0, 3.0]));
        assert!(clipping.clips([far + 2.0 + 1e-3, far + 2.0, 3.0]));
    }

    #[test]
    fn user_planes_are_limited_flipped_and_removed() {
        let mut clipping = Clipping::default();
        assert!(!clipping.add([0.0; 3], [0.0; 3]), "no direction");
        assert!(!clipping.add([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]));
        for i in 0..MAX_USER_PLANES {
            assert!(clipping.add(
                [f64::from(u8::try_from(i).unwrap()), 0.0, 0.0],
                [2.0, 0.0, 0.0]
            ));
        }
        assert!(
            !clipping.add([9.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            "the seventh"
        );
        assert_eq!(clipping.planes[0].normal, [1.0, 0.0, 0.0]);
        assert!(clipping.is_active());
        assert!(clipping.flip(0) && !clipping.flip(9));
        assert_eq!(clipping.planes[0].normal, [-1.0, 0.0, 0.0]);
        assert!(clipping.remove(0) && !clipping.remove(9));
        assert_eq!(clipping.planes.len(), MAX_USER_PLANES - 1);
    }

    #[test]
    fn a_plane_removes_the_side_of_its_normal() {
        let mut clipping = Clipping::default();
        clipping.add([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]);
        assert!(clipping.clips([0.0, 0.0, 1.5]));
        assert!(!clipping.clips([0.0, 0.0, 1.0]) && !clipping.clips([0.0, 0.0, 0.5]));
        clipping.flip(0);
        assert!(clipping.clips([0.0, 0.0, 0.5]) && !clipping.clips([0.0, 0.0, 1.5]));
    }

    #[test]
    fn the_box_and_the_planes_together_fill_the_renderers_budget() {
        let mut clipping = Clipping {
            section: Some(unit_box()),
            ..Clipping::default()
        };
        for _ in 0..MAX_USER_PLANES {
            clipping.add([1.0; 3], [1.0, 0.0, 0.0]);
        }
        assert_eq!(clipping.all_planes().len(), MAX_CLIP_PLANES);
    }

    #[test]
    fn faces_move_but_never_cross() {
        let mut section = unit_box();
        let x_max = Face {
            axis: 0,
            upper: true,
        };
        assert_eq!(section.move_face(x_max, 5.0), 5.0);
        assert_eq!(
            section.move_face(x_max, -3.0),
            MIN_EXTENT,
            "kept above the minimum"
        );
        let x_min = Face {
            axis: 0,
            upper: false,
        };
        assert_eq!(section.move_face(x_min, 9.0), 0.0);
        assert_eq!(section.coordinate(x_max), MIN_EXTENT);
        assert_eq!(
            section.face_centre(Face {
                axis: 2,
                upper: true
            }),
            [MIN_EXTENT / 2.0, 2.0, 6.0]
        );
        assert_eq!(section.edges().len(), 12);
        assert_eq!(
            Face::ALL.map(Face::name),
            ["x_min", "x_max", "y_min", "y_max", "z_min", "z_max"]
        );
    }

    #[test]
    fn extents_are_validated_and_fitting_adds_a_margin() {
        assert!(valid_extents([0.0; 3], [1.0; 3]));
        assert!(!valid_extents([0.0; 3], [0.0, 1.0, 1.0]), "flat");
        assert!(!valid_extents([2.0, 0.0, 0.0], [1.0; 3]), "inverted");
        assert!(!valid_extents([f64::NAN, 0.0, 0.0], [1.0; 3]));
        assert!(!valid_extents(
            [0.0; 3],
            [2.0 * MAX_WORLD_COORDINATE, 1.0, 1.0]
        ));
        let fit = fitted([0.0; 3], [1.0; 3]);
        assert_eq!((fit.min, fit.max, fit.enabled), ([-0.1; 3], [1.1; 3], true));
    }

    #[test]
    fn a_handle_is_the_nearest_face_within_reach() {
        let projected = [
            (Face::ALL[0], Some([10.0, 10.0])),
            (Face::ALL[1], Some([14.0, 10.0])),
            (Face::ALL[2], None),
            (Face::ALL[3], Some([100.0, 100.0])),
        ];
        assert_eq!(
            nearest_handle(&projected, [13.0, 10.0], 12.0),
            Some(Face::ALL[1])
        );
        assert_eq!(nearest_handle(&projected, [300.0, 0.0], 12.0), None);
        assert_eq!(nearest_handle(&[], [0.0; 2], 12.0), None);
    }

    #[test]
    fn dragging_a_handle_follows_the_ray_along_its_axis() {
        let section = unit_box();
        let face = Face {
            axis: 0,
            upper: true,
        };
        // The eye 10 m in front (−y), looking along +y; the ray aims at x = 3 on the plane y = 2.
        let eye = [1.0, -10.0, 3.0];
        let target = [3.0, 2.0, 3.0];
        let direction = normalize(sub(target, eye)).unwrap();
        let ray = Ray {
            origin: [0.0; 3],
            direction,
        };
        let x = drag_coordinate(&section, face, &ray, eye).unwrap();
        // The point of the axis line (y = 2, z = 3) nearest the ray is at x = 3.
        assert!((x - 3.0).abs() < 1e-9, "{x}");
        // A ray along the axis cannot move the face.
        let along = Ray {
            origin: [0.0; 3],
            direction: [1.0, 0.0, 0.0],
        };
        assert_eq!(drag_coordinate(&section, face, &along, eye), None);
        assert_eq!(snap_to_step(2.96, SHIFT_STEP), 3.0);
        assert!((snap_to_step(-0.04, SHIFT_STEP)).abs() < 1e-12);
    }

    #[test]
    fn the_same_drag_works_ten_million_metres_away() {
        let far = 1.0e7;
        let section = SectionBox {
            enabled: true,
            min: [far; 3],
            max: [far + 2.0, far + 4.0, far + 6.0],
        };
        let face = Face {
            axis: 0,
            upper: true,
        };
        let eye = [far + 1.0, far - 10.0, far + 3.0];
        let direction = normalize(sub([far + 3.0, far + 2.0, far + 3.0], eye)).unwrap();
        let ray = Ray {
            origin: [0.0; 3],
            direction,
        };
        let x = drag_coordinate(&section, face, &ray, eye).unwrap();
        assert!((x - (far + 3.0)).abs() < 1e-6, "{x}");
    }

    #[test]
    fn a_plane_is_outlined_in_its_own_plane_with_an_arrow() {
        let plane = ClipPlane {
            point: [1.0, 2.0, 3.0],
            normal: [0.0, 0.0, 1.0],
        };
        let lines = plane_lines(&plane, 2.0);
        assert_eq!(lines.len(), 4 * 8 + 3);
        // The dashes lie in the plane; the arrow leaves it along the normal.
        for &(from, to) in &lines[..32] {
            for point in [from, to] {
                assert!(dot(plane.normal, sub(point, plane.point)).abs() < 1e-12);
            }
        }
        assert_eq!(lines[32], (plane.point, [1.0, 2.0, 4.0]));
        // The helper axis changes for a normal along z, still an orthonormal basis.
        let (u, v) = plane_basis([1.0, 0.0, 0.0]);
        assert!(dot(u, v).abs() < 1e-12 && (norm(u) - 1.0).abs() < 1e-12);
        assert_eq!(plane_half(0.0), 1.0);
        assert_eq!(plane_half(10.0), 5.0);
    }
}
