//! A recording renderer and fixed scenes for the tests.

use std::collections::{BTreeMap, BTreeSet};

use render::{Camera, DisplayState, MeshData, Placement, RenderError, Theme};
use scene_wire::{Element, Mesh, SceneDelta};

use crate::ElementId;
use crate::math::{Vec3, sub};
use crate::sink::{Preview, SceneSink, SinkError};

/// One call the fake received.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Call {
    AddMesh(u64),
    RemoveMesh(u64),
    AddObject(ElementId, u64),
    SetPlacement(ElementId, [f64; 3]),
    RemoveObject(ElementId),
    State(ElementId, DisplayState),
    Preview(Preview),
    Theme,
}

/// A renderer that records what it is told and answers rectangle queries from the world boxes
/// of its objects, projected on the CPU.
#[derive(Debug, Default)]
pub(crate) struct FakeSink {
    pub(crate) calls: Vec<Call>,
    pub(crate) meshes: BTreeMap<u64, [Vec3; 2]>,
    pub(crate) objects: BTreeMap<ElementId, (u64, Placement)>,
    pub(crate) states: BTreeMap<ElementId, DisplayState>,
    pub(crate) preview: Preview,
    /// The most ghosts a preview may hold, as a device's largest buffer would.
    pub(crate) max_ghosts: Option<usize>,
    /// Refuses the object added after this many more.
    pub(crate) objects_before_failure: Option<usize>,
}

impl FakeSink {
    pub(crate) fn take_calls(&mut self) -> Vec<Call> {
        core::mem::take(&mut self.calls)
    }

    fn world_box(&self, element: ElementId) -> Option<[Vec3; 2]> {
        let (mesh, placement) = self.objects.get(&element)?;
        let [lo, hi] = *self.meshes.get(mesh)?;
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for corner in 0..8 {
            let local: Vec3 =
                core::array::from_fn(|a| if corner >> a & 1 == 0 { lo[a] } else { hi[a] });
            let world: Vec3 = core::array::from_fn(|r| {
                (0..3)
                    .map(|c| placement.rotation[r][c] * local[c])
                    .sum::<f64>()
                    + placement.translation[r]
            });
            for a in 0..3 {
                min[a] = min[a].min(world[a]);
                max[a] = max[a].max(world[a]);
            }
        }
        Some([min, max])
    }
}

impl SceneSink for FakeSink {
    fn add_mesh(&mut self, key: u64, mesh: &MeshData) -> Result<(), SinkError> {
        mesh.validate()?;
        let origin = mesh.origin;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for a in 0..3 {
                lo[a] = lo[a].min(origin[a] + f64::from(p[a]));
                hi[a] = hi[a].max(origin[a] + f64::from(p[a]));
            }
        }
        self.meshes.insert(key, [lo, hi]);
        self.calls.push(Call::AddMesh(key));
        Ok(())
    }

    fn remove_mesh(&mut self, key: u64) -> Result<(), SinkError> {
        self.meshes.remove(&key).ok_or(SinkError::UnknownMesh)?;
        self.calls.push(Call::RemoveMesh(key));
        Ok(())
    }

    fn add_object(
        &mut self,
        element: ElementId,
        mesh: u64,
        placement: Placement,
    ) -> Result<(), SinkError> {
        if let Some(left) = &mut self.objects_before_failure {
            if *left == 0 {
                return Err(SinkError::Render(RenderError::TooManyObjects { max: 0 }));
            }
            *left -= 1;
        }
        if !self.meshes.contains_key(&mesh) {
            return Err(SinkError::UnknownMesh);
        }
        self.objects.insert(element, (mesh, placement));
        self.calls.push(Call::AddObject(element, mesh));
        Ok(())
    }

    fn set_placement(&mut self, element: ElementId, placement: Placement) -> Result<(), SinkError> {
        let object = self
            .objects
            .get_mut(&element)
            .ok_or(SinkError::UnknownElement)?;
        object.1 = placement;
        self.calls
            .push(Call::SetPlacement(element, placement.translation));
        Ok(())
    }

    fn remove_object(&mut self, element: ElementId) -> Result<(), SinkError> {
        self.objects
            .remove(&element)
            .ok_or(SinkError::UnknownElement)?;
        self.states.remove(&element);
        self.calls.push(Call::RemoveObject(element));
        Ok(())
    }

    fn set_display_state(
        &mut self,
        element: ElementId,
        state: DisplayState,
    ) -> Result<(), SinkError> {
        if !self.objects.contains_key(&element) {
            return Err(SinkError::UnknownElement);
        }
        if state == DisplayState::Normal {
            self.states.remove(&element);
        } else {
            self.states.insert(element, state);
        }
        self.calls.push(Call::State(element, state));
        Ok(())
    }

    fn set_preview(&mut self, preview: &Preview) -> Result<(), SinkError> {
        if let Some(max) = self.max_ghosts {
            let ghost_states = self
                .states
                .values()
                .filter(|&&s| s == DisplayState::Ghost)
                .count();
            if preview.ghosts.len() + ghost_states > max {
                return Err(SinkError::Render(RenderError::OverlayTooLarge));
            }
        }
        if preview
            .ghosts
            .iter()
            .any(|(id, _)| !self.objects.contains_key(id))
        {
            return Err(SinkError::UnknownElement);
        }
        self.preview = preview.clone();
        self.calls.push(Call::Preview(preview.clone()));
        Ok(())
    }

    fn set_theme(&mut self, _theme: Theme) {
        self.calls.push(Call::Theme);
    }

    fn elements_in_rect(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        rect: [f64; 4],
        found: &mut dyn FnMut(ElementId),
    ) -> Result<(), SinkError> {
        camera.validate().map_err(RenderError::from)?;
        let view = crate::view::View::new(camera, size);
        // Conservative, as the renderer's frustum test: any corner of the box inside the
        // rectangle, or the rectangle's centre ray through the box, reports the element.
        let centre = [
            f64::midpoint(rect[0], rect[2]),
            f64::midpoint(rect[1], rect[3]),
        ];
        let ray = view.ray(centre);
        let ids: Vec<ElementId> = self.objects.keys().copied().collect();
        for id in ids {
            let Some([min, max]) = self.world_box(id) else {
                continue;
            };
            let corner_in = (0..8).any(|corner| {
                let p: Vec3 =
                    core::array::from_fn(|a| if corner >> a & 1 == 0 { min[a] } else { max[a] });
                view.project(sub(p, view.eye)).is_some_and(|[x, y, _]| {
                    x >= rect[0] && x <= rect[2] && y >= rect[1] && y <= rect[3]
                })
            });
            if corner_in || ray_meets_box(&ray, sub(min, view.eye), sub(max, view.eye)) {
                found(id);
            }
        }
        Ok(())
    }
}

fn ray_meets_box(ray: &crate::view::Ray, min: Vec3, max: Vec3) -> bool {
    let (mut enter, mut leave) = (0.0_f64, f64::INFINITY);
    for a in 0..3 {
        if ray.direction[a].abs() < 1e-300 {
            if ray.origin[a] < min[a] || ray.origin[a] > max[a] {
                return false;
            }
            continue;
        }
        let t0 = (min[a] - ray.origin[a]) / ray.direction[a];
        let t1 = (max[a] - ray.origin[a]) / ray.direction[a];
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
    }
    enter <= leave
}

/// The id of element `n`.
pub(crate) const fn id(n: u8) -> ElementId {
    let mut bytes = [0; 16];
    bytes[0] = n;
    ElementId(bytes)
}

/// A box mesh from `min` to `max` (local to its origin `origin`), flat shaded: 24 vertices, 12
/// triangles, with id `key`.
pub(crate) fn box_mesh(key: u64, origin: Vec3, min: [f32; 3], max: [f32; 3]) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for side in [0, 1] {
            let mut normal = [0.0_f32; 3];
            normal[axis] = if side == 0 { -1.0 } else { 1.0 };
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = u32::try_from(positions.len()).unwrap_or(0);
            for [a, b] in [[0, 0], [1, 0], [1, 1], [0, 1]] {
                let mut p = [0.0_f32; 3];
                p[axis] = if side == 0 { min[axis] } else { max[axis] };
                p[u] = if a == 0 { min[u] } else { max[u] };
                p[v] = if b == 0 { min[v] } else { max[v] };
                positions.push(p);
                normals.push(normal);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh {
        id: key,
        origin,
        positions,
        normals,
        indices,
        bounds_min: min,
        bounds_max: max,
    }
}

/// An element placing `mesh` translated by `at`.
pub(crate) fn element(n: u8, mesh: u64, at: Vec3) -> Element {
    Element {
        id: id(n).0,
        mesh,
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        translation: at,
        category: 0,
        discipline: 1,
    }
}

/// A full delta of the unit cubes (mesh 7, centred on its origin) placed at `positions`, all
/// offset by `offset`, as elements 1, 2, 3…
pub(crate) fn cubes(revision: u64, offset: Vec3, positions: &[Vec3]) -> SceneDelta {
    SceneDelta {
        full: true,
        revision,
        base: 0,
        strings: vec!["Block".to_owned(), "core".to_owned()],
        meshes: vec![box_mesh(7, [0.0; 3], [-0.5; 3], [0.5; 3])],
        elements: positions
            .iter()
            .zip(1..)
            .map(|(p, n)| element(n, 7, crate::math::add(offset, *p)))
            .collect(),
        ..SceneDelta::default()
    }
}

/// The ids of `ns`.
pub(crate) fn ids(ns: &[u8]) -> BTreeSet<ElementId> {
    ns.iter().map(|&n| id(n)).collect()
}
