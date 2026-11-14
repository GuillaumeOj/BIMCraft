//! The scene cache: what the viewport holds of the scene, kept in step with the renderer by the
//! deltas of the scene channel (`api-contract.md` §1), with the geometry picking and snapping
//! need on the CPU.

use std::collections::{BTreeMap, BTreeSet};

use render::{MeshData, Placement};
use scene_wire::SceneDelta;

use crate::ElementId;
use crate::math::{Vec3, add, apply, widen};
use crate::sink::{SceneSink, SinkError};

/// The cosine of the angle between the normals of two triangles above which their shared edge is
/// no feature edge: 1°, so the diagonals of a triangulated face are never snapped to.
const COPLANAR_COSINE: f64 = 0.999_847_695;

/// Why a delta was refused. Messages never contain coordinates or model content (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum SceneError {
    /// The bytes are not a valid delta.
    #[error(transparent)]
    Decode(#[from] scene_wire::WireError),
    /// The delta applies to another revision than the one held, and is not full.
    #[error("the delta does not apply to the revision the viewport holds")]
    BaseMismatch,
    /// An element refers to a mesh neither held nor in the delta.
    #[error("an element refers to an unknown mesh")]
    UnknownMesh,
    /// A removed mesh is still placed by an element.
    #[error("a removed mesh is still in use")]
    MeshInUse,
    /// The renderer refused the scene: the viewport dropped everything and needs a full delta.
    #[error("the renderer refused the scene")]
    Render(#[from] SinkError),
}

/// A mesh as the viewport keeps it for picking and snapping.
#[derive(Debug)]
pub(crate) struct CachedMesh {
    pub(crate) origin: Vec3,
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) indices: Vec<u32>,
    /// The local bounding box, relative to `origin`.
    pub(crate) bounds: [[f32; 3]; 2],
    /// The feature edges, as pairs of vertex indices: borders, and edges between triangles that
    /// are not coplanar.
    pub(crate) edges: Vec<[u32; 2]>,
}

/// An element as the viewport keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CachedElement {
    pub(crate) mesh: u64,
    pub(crate) placement: Placement,
}

/// What a delta changed, for the selection and the previews.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Applied {
    /// Elements added, changed or removed (with the full delta, all that were held).
    pub(crate) touched: BTreeSet<ElementId>,
}

/// A bounding sphere, in world metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sphere {
    pub(crate) centre: Vec3,
    pub(crate) radius: f64,
}

/// The meshes and elements the viewport holds.
#[derive(Debug, Default)]
pub(crate) struct SceneCache {
    revision: Option<u64>,
    meshes: BTreeMap<u64, CachedMesh>,
    elements: BTreeMap<ElementId, CachedElement>,
}

impl SceneCache {
    /// The revision held, or `None` before the first delta.
    pub(crate) fn revision(&self) -> Option<u64> {
        self.revision
    }

    pub(crate) fn element(&self, id: ElementId) -> Option<(&CachedElement, &CachedMesh)> {
        let element = self.elements.get(&id)?;
        Some((element, self.meshes.get(&element.mesh)?))
    }

    pub(crate) fn contains(&self, id: ElementId) -> bool {
        self.elements.contains_key(&id)
    }

    /// The world bounds of an element, `(min, max)`.
    pub(crate) fn bounds(&self, id: ElementId) -> Option<(Vec3, Vec3)> {
        let (element, mesh) = self.element(id)?;
        Some(world_bounds(element, mesh))
    }

    /// The bounding sphere of `ids` (every element when `None`), or `None` for none.
    pub(crate) fn sphere(&self, ids: Option<&[ElementId]>) -> Option<Sphere> {
        let mut bounds: Option<(Vec3, Vec3)> = None;
        let mut grow = |id: ElementId| {
            if let Some((min, max)) = self.bounds(id) {
                bounds = Some(match bounds {
                    None => (min, max),
                    Some((lo, hi)) => (
                        core::array::from_fn(|a| lo[a].min(min[a])),
                        core::array::from_fn(|a| hi[a].max(max[a])),
                    ),
                });
            }
        };
        match ids {
            Some(ids) => ids.iter().copied().for_each(&mut grow),
            None => self.elements.keys().copied().for_each(&mut grow),
        }
        let (min, max) = bounds?;
        let centre = core::array::from_fn(|a| min[a] + (max[a] - min[a]) / 2.0);
        let radius = libm::sqrt((0..3).map(|a| (max[a] - min[a]) * (max[a] - min[a])).sum()) / 2.0;
        Some(Sphere { centre, radius })
    }

    /// Applies a decoded delta to the cache and to `sink`. A delta that does not fit what is held
    /// changes nothing. When the sink refuses something midway, everything is dropped (cache and
    /// sink) and the revision is forgotten, so the next request asks for the full scene.
    pub(crate) fn apply(
        &mut self,
        delta: &SceneDelta,
        sink: &mut impl SceneSink,
    ) -> Result<Applied, SceneError> {
        self.check(delta)?;
        let mut applied = Applied::default();
        match self.write(delta, sink, &mut applied) {
            Ok(()) => {
                self.revision = Some(delta.revision);
                Ok(applied)
            }
            Err(error) => {
                self.clear(sink, &mut applied);
                self.revision = None;
                Err(SceneError::Render(error))
            }
        }
    }

    /// Checks the delta against what is held, without changing anything. Removals of ids not
    /// held are tolerated: the host's log may name an element added and removed since the base.
    fn check(&self, delta: &SceneDelta) -> Result<(), SceneError> {
        if !delta.full && Some(delta.base) != self.revision {
            return Err(SceneError::BaseMismatch);
        }
        let removed_meshes: BTreeSet<u64> = delta.removed_meshes.iter().copied().collect();
        let added_meshes: BTreeSet<u64> = delta.meshes.iter().map(|mesh| mesh.id).collect();
        let held =
            |id: u64| !delta.full && self.meshes.contains_key(&id) && !removed_meshes.contains(&id);
        if delta
            .elements
            .iter()
            .any(|element| !added_meshes.contains(&element.mesh) && !held(element.mesh))
        {
            return Err(SceneError::UnknownMesh);
        }
        if delta.full {
            return Ok(());
        }
        // After the delta, the elements still held that it neither removes nor changes keep
        // their mesh.
        let removed: BTreeSet<ElementId> = delta
            .removed_elements
            .iter()
            .map(|&id| ElementId(id))
            .collect();
        let changed: BTreeSet<ElementId> = delta.elements.iter().map(|e| ElementId(e.id)).collect();
        let in_use = self
            .elements
            .iter()
            .filter(|(id, _)| !removed.contains(id) && !changed.contains(id))
            .map(|(_, element)| element.mesh)
            .chain(delta.elements.iter().map(|element| element.mesh))
            .any(|mesh| removed_meshes.contains(&mesh) && !added_meshes.contains(&mesh));
        if in_use {
            return Err(SceneError::MeshInUse);
        }
        Ok(())
    }

    fn write(
        &mut self,
        delta: &SceneDelta,
        sink: &mut impl SceneSink,
        applied: &mut Applied,
    ) -> Result<(), SinkError> {
        if delta.full {
            self.clear(sink, applied);
        } else {
            for &id in &delta.removed_elements {
                let id = ElementId(id);
                if self.elements.remove(&id).is_some() {
                    sink.remove_object(id)?;
                    applied.touched.insert(id);
                }
            }
            for id in &delta.removed_meshes {
                if self.meshes.remove(id).is_some() {
                    sink.remove_mesh(*id)?;
                }
            }
        }
        for mesh in &delta.meshes {
            if self.meshes.contains_key(&mesh.id) {
                // Immutable once sent: the same id is the same mesh.
                continue;
            }
            sink.add_mesh(
                mesh.id,
                &MeshData {
                    origin: mesh.origin,
                    positions: mesh.positions.clone(),
                    normals: mesh.normals.clone(),
                    indices: mesh.indices.clone(),
                },
            )?;
            self.meshes.insert(
                mesh.id,
                CachedMesh {
                    origin: mesh.origin,
                    positions: mesh.positions.clone(),
                    indices: mesh.indices.clone(),
                    bounds: [mesh.bounds_min, mesh.bounds_max],
                    edges: feature_edges(&mesh.positions, &mesh.indices),
                },
            );
        }
        for element in &delta.elements {
            let id = ElementId(element.id);
            let new = CachedElement {
                mesh: element.mesh,
                placement: placement(&element.rotation, element.translation),
            };
            match self.elements.get(&id) {
                Some(old) if old.mesh == new.mesh => {
                    if old.placement != new.placement {
                        sink.set_placement(id, new.placement)?;
                    }
                }
                Some(_) => {
                    sink.remove_object(id)?;
                    sink.add_object(id, new.mesh, new.placement)?;
                }
                None => sink.add_object(id, new.mesh, new.placement)?,
            }
            self.elements.insert(id, new);
            applied.touched.insert(id);
        }
        Ok(())
    }

    /// Drops everything, from the sink too, as far as it lets itself be emptied.
    fn clear(&mut self, sink: &mut impl SceneSink, applied: &mut Applied) {
        for id in core::mem::take(&mut self.elements).into_keys() {
            // Emptying what may already be gone: refusals do not matter.
            drop(sink.remove_object(id));
            applied.touched.insert(id);
        }
        for id in core::mem::take(&mut self.meshes).into_keys() {
            drop(sink.remove_mesh(id));
        }
    }
}

/// The placement of a column-major rotation (a proper one, the decoder checked) and a
/// translation.
fn placement(rotation: &[f64; 9], translation: [f64; 3]) -> Placement {
    Placement {
        rotation: core::array::from_fn(|row| {
            core::array::from_fn(|column| rotation[column * 3 + row])
        }),
        translation,
    }
}

/// The world bounds of an element: its local box placed, `(min, max)`.
pub(crate) fn world_bounds(element: &CachedElement, mesh: &CachedMesh) -> (Vec3, Vec3) {
    let [lo, hi] = mesh.bounds.map(widen);
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for corner in 0..8 {
        let local = core::array::from_fn(|a| if corner >> a & 1 == 0 { lo[a] } else { hi[a] });
        let world = place(element, mesh, local);
        for a in 0..3 {
            min[a] = min[a].min(world[a]);
            max[a] = max[a].max(world[a]);
        }
    }
    (min, max)
}

/// The world point of a local position of `mesh` placed by `element`.
pub(crate) fn place(element: &CachedElement, mesh: &CachedMesh, local: Vec3) -> Vec3 {
    let rotation = &element.placement.rotation;
    add(
        add(apply(rotation, mesh.origin), element.placement.translation),
        apply(rotation, local),
    )
}

/// The feature edges of a triangle list: borders, and edges between triangles more than 1°
/// apart. Edges with more than two triangles are features too.
fn feature_edges(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[u32; 2]> {
    let vertex = |i: u32| {
        positions
            .get(i as usize)
            .copied()
            .map(widen)
            .unwrap_or_default()
    };
    let mut sides: BTreeMap<[u32; 2], Vec<Vec3>> = BTreeMap::new();
    for &[a, b, c] in indices.as_chunks::<3>().0 {
        let normal = crate::math::normalize(crate::math::cross(
            crate::math::sub(vertex(b), vertex(a)),
            crate::math::sub(vertex(c), vertex(a)),
        ))
        .unwrap_or_default();
        for [p, q] in [[a, b], [b, c], [c, a]] {
            if p != q {
                sides.entry([p.min(q), p.max(q)]).or_default().push(normal);
            }
        }
    }
    sides
        .into_iter()
        .filter(|(_, normals)| match normals.as_slice() {
            [n, m] => crate::math::dot(*n, *m) < COPLANAR_COSINE,
            _ => true,
        })
        .map(|(edge, _)| edge)
        .collect()
}

#[cfg(test)]
mod tests;
