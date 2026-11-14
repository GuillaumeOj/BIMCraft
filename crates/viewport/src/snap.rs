//! Snapping (VP-01, spec D5): the point a tool takes from the pointer. Candidates within a
//! tolerance in pixels, by priority: a vertex (endpoint), the middle of an edge, the nearest
//! point of an edge, a point of a face, then the ground plane z = 0. Only feature edges count, so
//! the diagonals of a triangulated face are never snapped to, and a candidate hidden behind a
//! face is skipped. Everything is computed in `f64` relative to the eye, as picking.

use render::{Camera, MAX_WORLD_COORDINATE, SnapKind};

use crate::ElementId;
use crate::cache::{SceneCache, world_bounds};
use crate::math::{Vec3, add, distance_2d, dot, norm, normalize, scale, sub};
use crate::pick::{Pickable, Placed, candidates, nearest};
use crate::sink::SceneSink;
use crate::view::{Ray, View};

/// The default tolerance, in CSS pixels.
pub const DEFAULT_TOLERANCE: f64 = 10.0;

/// The most elements whose vertices and edges are examined, the nearest first: an MVP bound on
/// the work of one pointer move over a dense model.
pub(crate) const MAX_CANDIDATES: usize = 64;

/// The most candidates tested for being hidden before giving up on vertices and edges.
const MAX_VISIBILITY_TESTS: usize = 16;

/// Where the pointer snaps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Snap {
    /// What it snapped to, or `None` for the plain point under the pointer (snapping off).
    pub(crate) kind: Option<SnapKind>,
    /// The point, in world metres.
    pub(crate) point: Vec3,
}

/// What a snap looks at.
pub(crate) struct Query<'a> {
    pub(crate) cache: &'a SceneCache,
    pub(crate) camera: &'a Camera,
    /// The target, in device pixels.
    pub(crate) size: [u32; 2],
    /// The pointer, in device pixels.
    pub(crate) pos: [f64; 2],
    /// The tolerance, in device pixels.
    pub(crate) tolerance: f64,
    /// Whether to snap at all: otherwise the plain face or ground point.
    pub(crate) enabled: bool,
    /// Elements never snapped to (those being moved).
    pub(crate) skip: &'a dyn Fn(ElementId) -> bool,
    pub(crate) pickable: &'a dyn Pickable,
}

/// A vertex or edge candidate.
struct Candidate {
    rank: u8,
    pixels: f64,
    depth: f64,
    kind: SnapKind,
    relative: Vec3,
}

/// The point the pointer snaps to, or `None` when it is over nothing and the ground plane is
/// out of reach.
pub(crate) fn snap(query: &Query<'_>, sink: &mut impl SceneSink) -> Option<Snap> {
    let view = View::new(query.camera, query.size);
    let ray = view.ray(query.pos);
    let elements: Vec<ElementId> =
        candidates(sink, query.camera, query.size, query.pos, query.tolerance)
            .into_iter()
            .filter(|&id| !(query.skip)(id))
            .collect();
    let found = |kind, relative| Snap {
        kind: query.enabled.then_some(kind),
        point: add(view.eye, relative),
    };
    if query.enabled
        && let Some(snap) = vertex_or_edge(query, &view, &ray, &elements)
    {
        return Some(snap);
    }
    if let Some(hit) = nearest(
        query.cache,
        &view,
        &ray,
        &elements,
        query.skip,
        query.pickable,
    ) {
        return Some(found(SnapKind::Face, hit.relative));
    }
    ground(&view, &ray).map(|relative| found(SnapKind::Ground, relative))
}

/// The best visible vertex, midpoint or edge point within the tolerance.
fn vertex_or_edge(
    query: &Query<'_>,
    view: &View,
    ray: &Ray,
    elements: &[ElementId],
) -> Option<Snap> {
    let mut nearest_first: Vec<(f64, ElementId)> = elements
        .iter()
        .filter_map(|&id| {
            let (element, mesh) = query.cache.element(id)?;
            let (min, max) = world_bounds(element, mesh);
            let centre = sub(scale(add(min, max), 0.5), view.eye);
            Some((norm(centre), id))
        })
        .collect();
    nearest_first.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut found = Vec::new();
    for &(_, id) in nearest_first.iter().take(MAX_CANDIDATES) {
        let Some((element, mesh)) = query.cache.element(id) else {
            continue;
        };
        let placed = Placed::new(element, mesh, view.eye);
        let mut consider = |kind: SnapKind, rank: u8, relative: Vec3| {
            let Some([x, y, depth]) = view.project(relative) else {
                return;
            };
            let pixels = distance_2d([x, y], query.pos);
            if pixels <= query.tolerance && query.pickable.pickable(id, add(view.eye, relative)) {
                found.push(Candidate {
                    rank,
                    pixels,
                    depth,
                    kind,
                    relative,
                });
            }
        };
        for index in 0..u32::try_from(mesh.positions.len()).unwrap_or(u32::MAX) {
            consider(SnapKind::Endpoint, 0, placed.vertex(index));
        }
        for &[p, q] in &mesh.edges {
            let (a, b) = (placed.vertex(p), placed.vertex(q));
            consider(SnapKind::Midpoint, 1, scale(add(a, b), 0.5));
            consider(SnapKind::Edge, 2, closest_on_segment(ray, a, b));
        }
    }
    found.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then(a.pixels.total_cmp(&b.pixels))
            .then(a.depth.total_cmp(&b.depth))
    });
    found
        .iter()
        .take(MAX_VISIBILITY_TESTS)
        .find(|candidate| visible(query, view, elements, candidate.relative))
        .map(|candidate| Snap {
            kind: Some(candidate.kind),
            point: add(view.eye, candidate.relative),
        })
}

/// Whether nothing among `elements` hides the point (relative to the eye) from the eye.
fn visible(query: &Query<'_>, view: &View, elements: &[ElementId], relative: Vec3) -> bool {
    let (ray, distance) = ray_to(view, relative);
    let blocker = nearest(
        query.cache,
        view,
        &ray,
        elements,
        query.skip,
        query.pickable,
    );
    blocker.is_none_or(|hit| hit.t >= distance - (1e-6 * distance + 1e-6))
}

/// The ray from the eye (perspective) or from the image plane (parallel) to a point, and the
/// distance along it.
fn ray_to(view: &View, relative: Vec3) -> (Ray, f64) {
    let centre = view.ray(view.centre());
    if view.is_perspective() {
        let distance = norm(relative);
        let direction = normalize(relative).unwrap_or(centre.direction);
        return (
            Ray {
                origin: [0.0; 3],
                direction,
            },
            distance,
        );
    }
    let depth = dot(relative, centre.direction);
    (
        Ray {
            origin: sub(relative, scale(centre.direction, depth)),
            direction: centre.direction,
        },
        depth,
    )
}

/// The point of the segment `a`–`b` nearest to the ray.
fn closest_on_segment(ray: &Ray, a: Vec3, b: Vec3) -> Vec3 {
    let along = sub(b, a);
    let from = sub(a, ray.origin);
    let (uu, ud) = (dot(along, along), dot(along, ray.direction));
    let denominator = uu - ud * ud;
    let s = if denominator <= 1e-12 * uu {
        0.0
    } else {
        ((ud * dot(ray.direction, from) - dot(along, from)) / denominator).clamp(0.0, 1.0)
    };
    add(a, scale(along, s))
}

/// Where the ray meets the ground plane z = 0, relative to the eye, if in front of the near
/// plane and within the world.
fn ground(view: &View, ray: &Ray) -> Option<Vec3> {
    let height = view.eye[2] + ray.origin[2];
    let t = -height / ray.direction[2];
    if !(t > 0.0 && t.is_finite()) {
        return None;
    }
    let mut relative = ray.at(t);
    // Exactly on the plane, whatever the rounding.
    relative[2] = -view.eye[2];
    let world = add(view.eye, relative);
    let in_world = world.iter().all(|v| v.abs() <= MAX_WORLD_COORDINATE);
    (in_world && view.project(relative).is_some()).then_some(relative)
}

#[cfg(test)]
mod tests;
