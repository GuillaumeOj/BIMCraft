//! Picking on the CPU (VP-01, spec D5): the ray through the pointer, the renderer's hierarchy of
//! objects for the candidates, then the triangles of their meshes, all in `f64` relative to the
//! eye. A vertex is placed as `R·p + (R·origin + t − eye)`: the large terms cancel in the
//! bracket before the small local position is added, so a pick 10⁷ m from the origin is as exact
//! as one at it (ADR 0020).

use render::Camera;

use crate::ElementId;
use crate::cache::{CachedElement, CachedMesh, SceneCache, world_bounds};
use crate::math::{Vec3, add, apply, cross, dot, normalize, scale, sub, widen};
use crate::sink::SceneSink;
use crate::view::{Ray, View};

/// Which elements and points may be picked or snapped to: hidden elements and clipped-away
/// points never are (the view's gate, `interaction/sectioning.rs`).
pub(crate) trait Pickable {
    fn pickable(&self, element: ElementId, point: Vec3) -> bool;
}

/// Everything may be picked: the tests of picking and snapping.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Everything;

#[cfg(test)]
impl Pickable for Everything {
    fn pickable(&self, _element: ElementId, _point: Vec3) -> bool {
        true
    }
}

/// What the pointer is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Hit {
    pub(crate) element: ElementId,
    /// The point, relative to the eye.
    pub(crate) relative: Vec3,
    /// The unit normal of the face, turned towards the eye.
    pub(crate) normal: Vec3,
    /// The distance along the ray.
    pub(crate) t: f64,
}

/// The triangles of an element relative to the eye.
pub(crate) struct Placed<'a> {
    pub(crate) mesh: &'a CachedMesh,
    rotation: [[f64; 3]; 3],
    offset: Vec3,
}

impl<'a> Placed<'a> {
    pub(crate) fn new(element: &CachedElement, mesh: &'a CachedMesh, eye: Vec3) -> Self {
        let rotation = element.placement.rotation;
        let offset = sub(
            add(apply(&rotation, mesh.origin), element.placement.translation),
            eye,
        );
        Self {
            mesh,
            rotation,
            offset,
        }
    }

    /// Vertex `index`, relative to the eye.
    pub(crate) fn vertex(&self, index: u32) -> Vec3 {
        let local = self
            .mesh
            .positions
            .get(index as usize)
            .copied()
            .map(widen)
            .unwrap_or_default();
        add(apply(&self.rotation, local), self.offset)
    }

    pub(crate) fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[a, b, c]| [self.vertex(a), self.vertex(b), self.vertex(c)])
    }
}

/// The elements whose bounds meet the view through the rectangle `half` pixels around `pos`.
pub(crate) fn candidates(
    sink: &mut impl SceneSink,
    camera: &Camera,
    size: [u32; 2],
    pos: [f64; 2],
    half: f64,
) -> Vec<ElementId> {
    let mut found = Vec::new();
    let rect = [pos[0] - half, pos[1] - half, pos[0] + half, pos[1] + half];
    // A refused query (an invalid camera, which the controller never makes) finds nothing.
    drop(sink.elements_in_rect(camera, size, rect, &mut |id| found.push(id)));
    found.sort_unstable();
    found.dedup();
    found
}

/// The nearest triangle `ray` meets among `elements` closer than `before`, which `pickable`
/// accepts. An element whose bounds the ray misses, or meets only beyond the best hit so far,
/// is not looked into.
pub(crate) fn nearest(
    cache: &SceneCache,
    view: &View,
    ray: &Ray,
    elements: &[ElementId],
    pickable: &dyn Pickable,
    before: f64,
) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for &id in elements {
        let Some((element, mesh)) = cache.element(id) else {
            continue;
        };
        let limit = best.map_or(before, |best| best.t);
        let (min, max) = world_bounds(element, mesh);
        if !ray_meets_box(ray, sub(min, view.eye), sub(max, view.eye), limit) {
            continue;
        }
        let placed = Placed::new(element, mesh, view.eye);
        for triangle in placed.triangles() {
            let Some((t, normal)) = intersect(ray, &triangle) else {
                continue;
            };
            if t >= best.map_or(before, |best| best.t) {
                continue;
            }
            let relative = ray.at(t);
            // Before the near plane, nothing is drawn.
            if view.project(relative).is_none() || !pickable.pickable(id, add(view.eye, relative)) {
                continue;
            }
            best = Some(Hit {
                element: id,
                relative,
                normal,
                t,
            });
        }
    }
    best
}

/// What is under the pixel `pos`: the nearest pickable face.
pub(crate) fn pick(
    cache: &SceneCache,
    sink: &mut impl SceneSink,
    camera: &Camera,
    size: [u32; 2],
    pos: [f64; 2],
    pickable: &dyn Pickable,
) -> Option<Hit> {
    let view = View::new(camera, size);
    let elements = candidates(sink, camera, size, pos, 0.5);
    nearest(
        cache,
        &view,
        &view.ray(pos),
        &elements,
        pickable,
        f64::INFINITY,
    )
}

/// Whether the ray meets the box (relative to the eye) before `limit` (slab test). The box is
/// grown by a hair, so rounding never hides a face on its boundary.
pub(crate) fn ray_meets_box(ray: &Ray, min: Vec3, max: Vec3, limit: f64) -> bool {
    let (mut enter, mut leave) = (0.0_f64, limit);
    for axis in 0..3 {
        let margin = 1e-9 * (max[axis] - min[axis]).abs().max(1.0);
        let (low, high) = (min[axis] - margin, max[axis] + margin);
        let (origin, direction) = (ray.origin[axis], ray.direction[axis]);
        if direction == 0.0 {
            if origin < low || origin > high {
                return false;
            }
            continue;
        }
        let (t0, t1) = ((low - origin) / direction, (high - origin) / direction);
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
    }
    enter <= leave
}

/// Where `ray` crosses the triangle (both sides), with the unit normal facing the ray
/// (Möller–Trumbore, in `f64`).
fn intersect(ray: &Ray, [first, second, third]: &[Vec3; 3]) -> Option<(f64, Vec3)> {
    let ab = sub(*second, *first);
    let ac = sub(*third, *first);
    let normal_to_ray = cross(ray.direction, ac);
    let determinant = dot(ab, normal_to_ray);
    let size = dot(ab, ab).max(dot(ac, ac));
    if determinant.abs() <= 1e-14 * size || !determinant.is_finite() {
        return None;
    }
    let inverse = 1.0 / determinant;
    let from_first = sub(ray.origin, *first);
    let barycentric_b = dot(from_first, normal_to_ray) * inverse;
    if !(0.0..=1.0).contains(&barycentric_b) {
        return None;
    }
    let across = cross(from_first, ab);
    let barycentric_c = dot(ray.direction, across) * inverse;
    if barycentric_c < 0.0 || barycentric_b + barycentric_c > 1.0 {
        return None;
    }
    let t = dot(ac, across) * inverse;
    if t <= 0.0 {
        return None;
    }
    let normal = normalize(cross(ab, ac))?;
    let facing = if dot(normal, ray.direction) > 0.0 {
        scale(normal, -1.0)
    } else {
        normal
    };
    Some((t, facing))
}

#[cfg(test)]
mod tests;
