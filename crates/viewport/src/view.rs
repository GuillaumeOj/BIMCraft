//! Between the screen and the world, in `f64` relative to the eye (ADR 0020): a ray through a
//! pixel, and the pixel of a point. Points are handed over as offsets from the eye, so a model
//! 10⁷ m from the origin maps exactly as one at the origin.

use render::{Camera, Projection};

use crate::math::{Vec3, add, cross, dot, normalize, scale, sub};

/// A ray, its origin relative to the eye.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ray {
    /// Where it starts, relative to the eye: the eye itself in perspective, a point of the
    /// image plane in parallel.
    pub(crate) origin: Vec3,
    /// Unit direction.
    pub(crate) direction: Vec3,
}

impl Ray {
    /// The point at `t`, relative to the eye.
    pub(crate) fn at(&self, t: f64) -> Vec3 {
        add(self.origin, scale(self.direction, t))
    }
}

/// A validated camera on a target of `size` pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    pub(crate) eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    projection: Projection,
    size: [f64; 2],
    /// Half the view, at a distance of 1 m (perspective) or in metres (parallel): `[x, y]`.
    half: [f64; 2],
}

impl View {
    /// The view of a camera that [`Camera::validate`] accepts, on a target of `size` pixels.
    pub(crate) fn new(camera: &Camera, size: [u32; 2]) -> Self {
        let eye = [camera.eye.x, camera.eye.y, camera.eye.z];
        let target = [camera.target.x, camera.target.y, camera.target.z];
        let up_hint = [camera.up.x, camera.up.y, camera.up.z];
        let forward = normalize(sub(target, eye)).unwrap_or([0.0, 1.0, 0.0]);
        let right = normalize(cross(forward, up_hint)).unwrap_or([1.0, 0.0, 0.0]);
        let up = cross(right, forward);
        let size = size.map(|side| f64::from(side.max(1)));
        let aspect = size[0] / size[1];
        let half_y = match camera.projection {
            Projection::Perspective { fov_y_radians, .. } => libm::tan(fov_y_radians / 2.0),
            Projection::Orthographic { height, .. } => height / 2.0,
        };
        Self {
            eye,
            right,
            up,
            forward,
            projection: camera.projection,
            size,
            half: [half_y * aspect, half_y],
        }
    }

    /// The ray through the pixel `pos` (from the top left, y down).
    pub(crate) fn ray(&self, pos: [f64; 2]) -> Ray {
        let x = (2.0 * pos[0] / self.size[0] - 1.0) * self.half[0];
        let y = (1.0 - 2.0 * pos[1] / self.size[1]) * self.half[1];
        let offset = add(scale(self.right, x), scale(self.up, y));
        match self.projection {
            Projection::Perspective { .. } => Ray {
                origin: [0.0; 3],
                direction: normalize(add(self.forward, offset)).unwrap_or(self.forward),
            },
            Projection::Orthographic { .. } => Ray {
                origin: offset,
                direction: self.forward,
            },
        }
    }

    /// The pixel of a point given relative to the eye, with its depth along the view: `None`
    /// when it lies before the near plane.
    pub(crate) fn project(&self, point: Vec3) -> Option<[f64; 3]> {
        let depth = dot(point, self.forward);
        let near = match self.projection {
            Projection::Perspective { near, .. } | Projection::Orthographic { near, .. } => near,
        };
        if depth.is_nan() || depth < near {
            return None;
        }
        let scale = match self.projection {
            Projection::Perspective { .. } => depth,
            Projection::Orthographic { .. } => 1.0,
        };
        let x = dot(point, self.right) / (scale * self.half[0]);
        let y = dot(point, self.up) / (scale * self.half[1]);
        Some([
            f64::midpoint(x, 1.0) * self.size[0],
            f64::midpoint(-y, 1.0) * self.size[1],
            depth,
        ])
    }

    /// The depth of a point relative to the eye, along the view.
    pub(crate) fn depth(&self, point: Vec3) -> f64 {
        dot(point, self.forward)
    }

    /// The size of a pixel in metres at `depth` along the view.
    pub(crate) fn pixel_size(&self, depth: f64) -> f64 {
        let span = 2.0 * self.half[1] / self.size[1];
        match self.projection {
            Projection::Perspective { .. } => span * depth,
            Projection::Orthographic { .. } => span,
        }
    }

    pub(crate) fn is_perspective(&self) -> bool {
        matches!(self.projection, Projection::Perspective { .. })
    }

    /// The pixel at the centre of the target.
    pub(crate) fn centre(&self) -> [f64; 2] {
        [self.size[0] / 2.0, self.size[1] / 2.0]
    }
}

#[cfg(test)]
#[expect(clippy::float_cmp, reason = "small integers, exact in f64")]
mod tests {
    use geom::{Point3, Vector3};

    use super::*;

    fn camera(offset: f64, projection: Projection) -> Camera {
        Camera {
            eye: Point3::new(offset, offset - 10.0, offset),
            target: Point3::new(offset, offset, offset),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection,
        }
    }

    const PERSPECTIVE: Projection = Projection::Perspective {
        fov_y_radians: core::f64::consts::FRAC_PI_2,
        near: 0.1,
    };

    const PARALLEL: Projection = Projection::Orthographic {
        height: 20.0,
        near: 0.1,
        far: 100.0,
    };

    #[test]
    fn a_ray_through_a_pixel_projects_back_onto_it() {
        for projection in [PERSPECTIVE, PARALLEL] {
            for offset in [0.0, 1e7] {
                let view = View::new(&camera(offset, projection), [200, 100]);
                for pos in [[100.0, 50.0], [0.0, 0.0], [37.5, 81.25]] {
                    let ray = view.ray(pos);
                    let [x, y, depth] = view.project(ray.at(7.0)).unwrap();
                    assert!(
                        (x - pos[0]).abs() < 1e-9 && (y - pos[1]).abs() < 1e-9,
                        "{x} {y}"
                    );
                    assert!(depth > 0.0);
                }
            }
        }
    }

    #[test]
    fn the_centre_ray_looks_at_the_target() {
        let view = View::new(&camera(0.0, PERSPECTIVE), [200, 100]);
        let ray = view.ray([100.0, 50.0]);
        assert_eq!(ray.origin, [0.0; 3]);
        assert!((ray.direction[1] - 1.0).abs() < 1e-15);
        // The top left of a 2:1 view, 90° high: 2 m right of the axis per metre, 1 m up.
        let corner = view.ray([0.0, 0.0]);
        let along = corner.direction[1];
        assert!((corner.direction[0] / along + 2.0).abs() < 1e-12);
        assert!((corner.direction[2] / along - 1.0).abs() < 1e-12);
        assert!((view.pixel_size(10.0) - 0.2).abs() < 1e-12);
        let parallel = View::new(&camera(0.0, PARALLEL), [200, 100]);
        assert_eq!(parallel.ray([0.0, 0.0]).origin, [-20.0, 0.0, 10.0]);
        assert!((parallel.pixel_size(1e6) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn points_before_the_near_plane_have_no_pixel() {
        let view = View::new(&camera(0.0, PERSPECTIVE), [100, 100]);
        assert_eq!(view.project([0.0, 0.05, 0.0]), None);
        assert_eq!(view.project([0.0, -1.0, 0.0]), None);
        assert_eq!(view.project([f64::NAN; 3]), None);
        assert!(view.project([0.0, 0.1, 0.0]).is_some());
        assert!((view.depth([3.0, 4.0, 5.0]) - 4.0).abs() < 1e-15);
    }
}
