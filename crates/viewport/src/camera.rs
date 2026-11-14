//! The camera controller: orbit, pan, zoom to the pointer, framing, presets and the projection
//! (`design/ux-notes.md`, flow 1).
//!
//! The view direction is a yaw about the world z axis and a pitch above or below the horizon,
//! clamped to ±90°: the up vector, derived from both, is always perpendicular to the view, so
//! the camera never degenerates, even looking straight down. Every operation computes the next
//! camera and keeps the current one when [`Camera::validate`] would refuse it.

use core::f64::consts::{FRAC_PI_2, PI};

use geom::{Point3, Vector3};
use render::{Camera, MAX_WORLD_COORDINATE, Projection};

use crate::cache::Sphere;
use crate::event::{Preset, ProjectionMode};
use crate::math::{Vec3, add, cross, dot, norm, scale, sub};

/// The vertical field of view of the perspective, 50°.
const FIELD_OF_VIEW: f64 = 50.0 * PI / 180.0;

/// The closest and farthest the camera gets from its target, in metres.
const MIN_DISTANCE: f64 = 1e-3;
const MAX_DISTANCE: f64 = 1e7;

/// How much larger than the bounding sphere a fitted view is.
const FIT_MARGIN: f64 = 1.1;

/// The smallest radius a fitted sphere counts for, in metres: a point still gets a view.
const MIN_FIT_RADIUS: f64 = 0.05;

/// The pitch of the isometric presets: the elevation of a cube's diagonal.
const ISO_PITCH: f64 = -0.615_479_708_670_387_3;

/// Where the camera looks from, and how.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CameraController {
    target: Vec3,
    distance: f64,
    yaw: f64,
    pitch: f64,
    mode: ProjectionMode,
    /// The height of the parallel view, in metres.
    height: f64,
}

impl Default for CameraController {
    /// 20 m from the origin, from the south-east, in perspective.
    fn default() -> Self {
        let distance = 20.0;
        Self {
            target: [0.0; 3],
            distance,
            yaw: 3.0 * PI / 4.0,
            pitch: ISO_PITCH,
            mode: ProjectionMode::Perspective,
            height: parallel_height(distance),
        }
    }
}

impl CameraController {
    pub(crate) fn mode(&self) -> ProjectionMode {
        self.mode
    }

    pub(crate) fn target(&self) -> Vec3 {
        self.target
    }

    pub(crate) fn distance(&self) -> f64 {
        self.distance
    }

    #[cfg(test)]
    pub(crate) fn pitch(&self) -> f64 {
        self.pitch
    }

    pub(crate) fn forward(&self) -> Vec3 {
        forward(self.yaw, self.pitch)
    }

    /// The camera for a scene within `scene`: the parallel view stands back far enough to see
    /// all of it.
    pub(crate) fn camera(&self, scene: Option<Sphere>) -> Camera {
        let (forward, up) = (forward(self.yaw, self.pitch), up(self.yaw, self.pitch));
        let (eye, projection) = match self.mode {
            ProjectionMode::Perspective => (
                sub(self.target, scale(forward, self.distance)),
                Projection::Perspective {
                    fov_y_radians: FIELD_OF_VIEW,
                    near: (self.distance * 1e-3).clamp(MIN_DISTANCE, 1.0),
                },
            ),
            ProjectionMode::Parallel => {
                let extent = scene.map_or(0.0, |s| norm(sub(s.centre, self.target)) + s.radius);
                let back = self.distance + extent;
                (
                    sub(self.target, scale(forward, back)),
                    Projection::Orthographic {
                        height: self.height,
                        near: MIN_DISTANCE,
                        far: (2.0 * back + 1.0).min(MAX_WORLD_COORDINATE),
                    },
                )
            }
        };
        Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(self.target[0], self.target[1], self.target[2]),
            up: Vector3::new(up[0], up[1], up[2]),
            projection,
        }
    }

    /// Turns the view by `yaw` about the vertical and `pitch` about the horizontal of the screen
    /// (radians), around `pivot`: the pivot stays where it is on the screen.
    pub(crate) fn orbit(&mut self, yaw: f64, pitch: f64, pivot: Vec3, scene: Option<Sphere>) {
        let mut next = *self;
        next.yaw = wrap(self.yaw + yaw);
        next.pitch = (self.pitch + pitch).clamp(-FRAC_PI_2, FRAC_PI_2);
        let offset = sub(self.target, pivot);
        let local = [
            dot(offset, self.right()),
            dot(offset, up(self.yaw, self.pitch)),
            dot(offset, self.forward()),
        ];
        let turned = add(
            add(
                scale(next.right(), local[0]),
                scale(up(next.yaw, next.pitch), local[1]),
            ),
            scale(next.forward(), local[2]),
        );
        next.target = add(pivot, turned);
        self.adopt(next, scene);
    }

    /// Slides the view by `right` and `up` metres in the plane of the screen.
    pub(crate) fn pan(&mut self, right: f64, up_by: f64, scene: Option<Sphere>) {
        let mut next = *self;
        let shift = add(
            scale(self.right(), right),
            scale(up(self.yaw, self.pitch), up_by),
        );
        next.target = add(self.target, shift);
        self.adopt(next, scene);
    }

    /// Zooms by `factor` (below 1 closer) towards `anchor`, a world point that stays on its pixel.
    pub(crate) fn zoom(&mut self, factor: f64, anchor: Vec3, scene: Option<Sphere>) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let mut next = *self;
        let ratio = match self.mode {
            ProjectionMode::Perspective => {
                next.distance = (self.distance * factor).clamp(MIN_DISTANCE, MAX_DISTANCE);
                next.distance / self.distance
            }
            ProjectionMode::Parallel => {
                let (low, high) = (parallel_height(MIN_DISTANCE), parallel_height(MAX_DISTANCE));
                next.height = (self.height * factor).clamp(low, high);
                next.height / self.height
            }
        };
        next.target = add(anchor, scale(sub(self.target, anchor), ratio));
        self.adopt(next, scene);
    }

    /// Frames `sphere` (a target of `aspect`, width over height), keeping the direction.
    pub(crate) fn fit(&mut self, sphere: Sphere, aspect: f64) {
        let radius = sphere.radius.max(MIN_FIT_RADIUS) * FIT_MARGIN;
        let tan_y = libm::tan(FIELD_OF_VIEW / 2.0);
        let half = libm::atan(tan_y * aspect.min(1.0));
        let mut next = *self;
        next.target = sphere.centre;
        next.distance = (radius / libm::sin(half)).clamp(MIN_DISTANCE, MAX_DISTANCE);
        next.height = 2.0 * radius / aspect.min(1.0);
        self.adopt(next, Some(sphere));
    }

    /// Looks from a preset direction.
    pub(crate) fn preset(&mut self, preset: Preset, scene: Option<Sphere>) {
        let (yaw, pitch) = match preset {
            Preset::Top => (FRAC_PI_2, -FRAC_PI_2),
            Preset::Front => (FRAC_PI_2, 0.0),
            Preset::Back => (-FRAC_PI_2, 0.0),
            Preset::Left => (0.0, 0.0),
            Preset::Right => (PI, 0.0),
            Preset::IsoSe => (3.0 * PI / 4.0, ISO_PITCH),
            Preset::IsoSw => (PI / 4.0, ISO_PITCH),
            Preset::IsoNe => (-3.0 * PI / 4.0, ISO_PITCH),
            Preset::IsoNw => (-PI / 4.0, ISO_PITCH),
        };
        let mut next = *self;
        next.yaw = yaw;
        next.pitch = pitch;
        self.adopt(next, scene);
    }

    /// Switches the projection, keeping the size of what is at the target. The current one
    /// changes nothing: the other mode's size is stale, its zoom not kept up to date.
    pub(crate) fn set_mode(&mut self, mode: ProjectionMode, scene: Option<Sphere>) {
        if mode == self.mode {
            return;
        }
        let mut next = *self;
        next.mode = mode;
        match mode {
            ProjectionMode::Parallel => next.height = parallel_height(self.distance),
            ProjectionMode::Perspective => {
                next.distance = (self.height / (2.0 * libm::tan(FIELD_OF_VIEW / 2.0)))
                    .clamp(MIN_DISTANCE, MAX_DISTANCE);
            }
        }
        self.adopt(next, scene);
    }

    fn right(&self) -> Vec3 {
        cross(self.forward(), up(self.yaw, self.pitch))
    }

    /// Takes `next` if its camera is valid.
    fn adopt(&mut self, next: Self, scene: Option<Sphere>) {
        if next.camera(scene).validate().is_ok() {
            *self = next;
        }
    }
}

/// The height of a parallel view showing what a perspective view shows at `distance`.
fn parallel_height(distance: f64) -> f64 {
    2.0 * distance * libm::tan(FIELD_OF_VIEW / 2.0)
}

fn forward(yaw: f64, pitch: f64) -> Vec3 {
    let (cos_pitch, sin_pitch) = (libm::cos(pitch), libm::sin(pitch));
    [
        cos_pitch * libm::cos(yaw),
        cos_pitch * libm::sin(yaw),
        sin_pitch,
    ]
}

/// The up of the screen: perpendicular to the view, in the vertical plane through it.
fn up(yaw: f64, pitch: f64) -> Vec3 {
    let (cos_pitch, sin_pitch) = (libm::cos(pitch), libm::sin(pitch));
    [
        -sin_pitch * libm::cos(yaw),
        -sin_pitch * libm::sin(yaw),
        cos_pitch,
    ]
}

/// An angle brought within (−π, π].
fn wrap(angle: f64) -> f64 {
    let turned = libm::remainder(angle, 2.0 * PI);
    if turned <= -PI {
        turned + 2.0 * PI
    } else {
        turned
    }
}

#[cfg(test)]
mod tests;
