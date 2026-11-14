//! The scene channel: the document's meshes and placements, kept current from change events and
//! served to the viewport as binary deltas (VP-01, GEO-05, NFR-03; `scene-wire` format).
//!
//! One [`SceneChannel`] holds the [`Scene`] of a document (view `body`, finest level of detail),
//! the revision the viewport is told, and a log of what each of the last [`LOG_LENGTH`] revisions
//! changed: ids only, never geometry. A delta for a caller that holds revision `since` is built
//! from the current state and the ids the log lists after `since`, so an edit costs what it
//! changed, not the model. Meshes are deduplicated by content hash: elements with the same local
//! geometry share one, and a move changes a placement and sends no mesh.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use core_model::{EntityId, Model};
use core_txn::ChangeEvent;
use scene::{Placement, Scene, SceneUpdate, View, builtin};
use scene_wire::{
    Element, MAX_MESH_INDICES, MAX_MESH_VERTICES, MAX_WORLD_COORDINATE, Mesh, SceneDelta,
};

/// How many revisions the log keeps. A caller further behind gets the full scene.
pub const LOG_LENGTH: usize = 256;

/// The finest level of detail of the `body` view: above any a schema declares.
const FINEST: u16 = u16::MAX;

/// Computes the id of a mesh from its content; injectable so a test can force a collision.
type MeshHash = fn(&Mesh) -> u64;

/// What the viewport is told about one element.
#[derive(Clone, Debug, PartialEq)]
struct Record {
    mesh: u64,
    placement: Placement,
    category: String,
    discipline: String,
}

/// A mesh the viewport holds, and how many elements draw it.
#[derive(Debug)]
struct Stored {
    mesh: Mesh,
    users: usize,
}

/// What one revision changed.
#[derive(Debug, Default)]
struct Revision {
    /// Elements added or changed.
    changed: BTreeSet<EntityId>,
    /// Elements removed.
    removed: BTreeSet<EntityId>,
    /// Meshes the viewport did not have.
    meshes_added: BTreeSet<u64>,
    /// Meshes the viewport must drop.
    meshes_removed: BTreeSet<u64>,
}

impl Revision {
    fn is_empty(&self) -> bool {
        self.changed.is_empty() && self.removed.is_empty()
    }

    fn add_mesh(&mut self, id: u64) {
        // Dropped then drawn again within one revision: the viewport keeps what it has.
        if !self.meshes_removed.remove(&id) {
            self.meshes_added.insert(id);
        }
    }

    fn remove_mesh(&mut self, id: u64) {
        // Drawn then dropped within one revision: the viewport never needs it.
        if !self.meshes_added.remove(&id) {
            self.meshes_removed.insert(id);
        }
    }

    fn change(&mut self, id: EntityId) {
        self.removed.remove(&id);
        self.changed.insert(id);
    }

    fn remove(&mut self, id: EntityId) {
        self.changed.remove(&id);
        self.removed.insert(id);
    }
}

/// The scene of one document, as the viewport is told it.
#[derive(Debug)]
pub struct SceneChannel {
    scene: Scene,
    revision: u64,
    elements: BTreeMap<EntityId, Record>,
    meshes: BTreeMap<u64, Stored>,
    /// The last revisions, oldest first, consecutive, the newest being `revision`.
    log: VecDeque<(u64, Revision)>,
    hash: MeshHash,
}

impl SceneChannel {
    /// The channel of `model`, meshed once.
    #[must_use]
    pub fn new(model: &Model) -> Self {
        Self::with_hash(model, content_hash)
    }

    fn with_hash(model: &Model, hash: MeshHash) -> Self {
        let mut generators = scene::Generators::new();
        builtin::register(&mut generators);
        let mut channel = Self {
            scene: Scene::new(generators, View::new("body", FINEST)),
            revision: 0,
            elements: BTreeMap::new(),
            meshes: BTreeMap::new(),
            log: VecDeque::new(),
            hash,
        };
        let update = channel.scene.rebuild(model);
        channel.record(&update, model);
        channel
    }

    /// The revision the viewport is told after the last change.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Takes in what `events` changed in `model`, one revision for all of them, none when the
    /// scene looks the same.
    pub fn apply(&mut self, events: &[ChangeEvent], model: &Model) {
        for event in events {
            self.scene.apply(event, model);
        }
        let update = self.scene.update(model);
        self.record(&update, model);
    }

    /// The delta that brings a viewport holding revision `since` (`None`: nothing) to the
    /// current revision: empty when it is current, the changes when the log reaches back to it,
    /// the full scene otherwise.
    #[must_use]
    pub fn delta(&self, since: Option<u64>) -> SceneDelta {
        match since {
            Some(since) if since == self.revision => SceneDelta {
                revision: self.revision,
                base: since,
                ..SceneDelta::default()
            },
            Some(since) if self.log_reaches(since) => self.changes_after(since),
            _ => self.full(),
        }
    }

    fn log_reaches(&self, since: u64) -> bool {
        since < self.revision
            && self
                .log
                .front()
                .is_some_and(|(first, _)| *first <= since + 1)
    }

    fn full(&self) -> SceneDelta {
        let mut strings = Strings::default();
        let elements = self
            .elements
            .iter()
            .map(|(id, record)| element(*id, record, &mut strings))
            .collect();
        SceneDelta {
            full: true,
            revision: self.revision,
            base: 0,
            strings: strings.into_vec(),
            meshes: self.meshes.values().map(|s| s.mesh.clone()).collect(),
            elements,
            ..SceneDelta::default()
        }
    }

    fn changes_after(&self, since: u64) -> SceneDelta {
        let mut net = Revision::default();
        for (_, revision) in self.log.iter().filter(|(number, _)| *number > since) {
            for &id in &revision.meshes_removed {
                net.remove_mesh(id);
            }
            for &id in &revision.meshes_added {
                net.add_mesh(id);
            }
            for &id in &revision.removed {
                net.remove(id);
            }
            for &id in &revision.changed {
                net.change(id);
            }
        }
        let mut strings = Strings::default();
        let elements = net
            .changed
            .iter()
            .filter_map(|id| Some(element(*id, self.elements.get(id)?, &mut strings)))
            .collect();
        SceneDelta {
            full: false,
            revision: self.revision,
            base: since,
            strings: strings.into_vec(),
            meshes: net
                .meshes_added
                .iter()
                .filter_map(|id| Some(self.meshes.get(id)?.mesh.clone()))
                .collect(),
            removed_meshes: net.meshes_removed.into_iter().collect(),
            elements,
            removed_elements: net.removed.iter().map(|id| id_bytes(*id)).collect(),
        }
    }

    /// Records what `update` changed as the next revision, if it changed anything the viewport
    /// sees.
    fn record(&mut self, update: &SceneUpdate, model: &Model) {
        let mut revision = Revision::default();
        for &id in update.added.iter().chain(&update.changed) {
            let record = self.record_of(model, id);
            self.set(id, record, &mut revision);
        }
        // An element that failed to mesh is dropped from the viewport; the failure is reported
        // elsewhere, later.
        for &id in update.removed.iter().chain(update.failed.keys()) {
            self.set(id, None, &mut revision);
        }
        if revision.is_empty() {
            return;
        }
        self.revision += 1;
        self.log.push_back((self.revision, revision));
        if self.log.len() > LOG_LENGTH {
            self.log.pop_front();
        }
    }

    /// What the viewport is told about `id`, interning its mesh; `None` when it has no mesh the
    /// channel can carry.
    fn record_of(&mut self, model: &Model, id: EntityId) -> Option<(Mesh, Record)> {
        let element = self.scene.mesh(model, id)?;
        let mesh = wire_mesh(element)?;
        let placement = element.placement;
        if !placement_is_valid(&placement) {
            return None;
        }
        let record = Record {
            mesh: 0,
            placement,
            category: model.entity_of(id)?.to_owned(),
            discipline: model.namespace().name.clone(),
        };
        Some((mesh, record))
    }

    /// Makes `new` what the viewport is told about `id`, and logs the difference.
    fn set(&mut self, id: EntityId, new: Option<(Mesh, Record)>, revision: &mut Revision) {
        let old = self.elements.remove(&id);
        // Intern the new mesh before releasing the old one: an element whose geometry is
        // unchanged keeps its mesh, and a move sends none.
        let new = new.map(|(mesh, mut record)| {
            record.mesh = self.intern(mesh, revision);
            record
        });
        if let Some(old) = &old {
            self.release(old.mesh, revision);
        }
        match new {
            Some(record) => {
                if old.as_ref() != Some(&record) {
                    revision.change(id);
                }
                self.elements.insert(id, record);
            }
            None if old.is_some() => revision.remove(id),
            None => {}
        }
    }

    /// The id of the stored mesh with `mesh`'s geometry, storing it first when there is none. Two
    /// geometries with one hash get different ids: the second probes upwards.
    fn intern(&mut self, mut mesh: Mesh, revision: &mut Revision) -> u64 {
        let mut id = (self.hash)(&mesh);
        loop {
            match self.meshes.get_mut(&id) {
                Some(stored) if same_geometry(&stored.mesh, &mesh) => {
                    stored.users += 1;
                    return id;
                }
                Some(_) => id = id.wrapping_add(1),
                None => {
                    mesh.id = id;
                    self.meshes.insert(id, Stored { mesh, users: 1 });
                    revision.add_mesh(id);
                    return id;
                }
            }
        }
    }

    fn release(&mut self, id: u64, revision: &mut Revision) {
        let Some(stored) = self.meshes.get_mut(&id) else {
            return;
        };
        stored.users -= 1;
        if stored.users == 0 {
            self.meshes.remove(&id);
            revision.remove_mesh(id);
        }
    }

    /// The number of distinct meshes held.
    #[must_use]
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }
}

/// The category and discipline names of a delta, each once, in order of first use.
#[derive(Default)]
struct Strings {
    index: BTreeMap<String, u32>,
    order: Vec<String>,
}

impl Strings {
    fn position(&mut self, name: &str) -> u32 {
        if let Some(&position) = self.index.get(name) {
            return position;
        }
        let position = u32::try_from(self.order.len()).unwrap_or(u32::MAX);
        self.index.insert(name.to_owned(), position);
        self.order.push(name.to_owned());
        position
    }

    fn into_vec(self) -> Vec<String> {
        self.order
    }
}

fn element(id: EntityId, record: &Record, strings: &mut Strings) -> Element {
    Element {
        id: id_bytes(id),
        mesh: record.mesh,
        rotation: record.placement.rotation,
        translation: record.placement.translation,
        category: strings.position(&record.category),
        discipline: strings.position(&record.discipline),
    }
}

/// The 16 bytes of an id, little-endian (api-contract.md).
fn id_bytes(id: EntityId) -> [u8; 16] {
    id.get().to_le_bytes()
}

/// A rotation the decoder accepts: finite, orthonormal columns, determinant +1; a translation
/// within the world extent. Generators build theirs from an angle, so this only guards against
/// a defect or a hostile model.
fn placement_is_valid(placement: &Placement) -> bool {
    const TOLERANCE: f64 = 1e-9;
    let r = &placement.rotation;
    let dot = |a: usize, b: usize| r[a] * r[b] + r[a + 1] * r[b + 1] + r[a + 2] * r[b + 2];
    let determinant = r[0] * (r[4] * r[8] - r[7] * r[5]) - r[3] * (r[1] * r[8] - r[7] * r[2])
        + r[6] * (r[1] * r[5] - r[4] * r[2]);
    r.iter().all(|v| v.is_finite())
        && (dot(0, 0) - 1.0).abs() < TOLERANCE
        && (dot(3, 3) - 1.0).abs() < TOLERANCE
        && (dot(6, 6) - 1.0).abs() < TOLERANCE
        && dot(0, 3).abs() < TOLERANCE
        && dot(0, 6).abs() < TOLERANCE
        && dot(3, 6).abs() < TOLERANCE
        && (determinant - 1.0).abs() < TOLERANCE
        && placement
            .translation
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_WORLD_COORDINATE)
}

/// The mesh in render's shape: `f32` positions around the `f64` centre of the bounding box
/// (ADR 0020), so equal local geometry gives equal meshes wherever it stands. `None` for a mesh
/// that is empty, has a non-finite or out-of-range value, or is too large for the wire format.
fn wire_mesh(mesh: &scene::ElementMesh) -> Option<Mesh> {
    let source = &mesh.mesh;
    if source.positions.is_empty()
        || source.normals.len() != source.positions.len()
        || !source.indices.len().is_multiple_of(3)
        || !source
            .positions
            .iter()
            .all(|p| [p.x, p.y, p.z].iter().all(|v| v.is_finite()))
        || !source
            .normals
            .iter()
            .all(|n| [n.x, n.y, n.z].iter().all(|v| v.is_finite()))
    {
        return None;
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for p in &source.positions {
        for (axis, value) in [p.x, p.y, p.z].into_iter().enumerate() {
            min[axis] = min[axis].min(value);
            max[axis] = max[axis].max(value);
        }
    }
    let origin = [0, 1, 2].map(|a| f64::midpoint(min[a], max[a]));
    let finite = origin
        .iter()
        .all(|v| v.is_finite() && v.abs() <= MAX_WORLD_COORDINATE);
    let vertices = source.positions.len();
    let in_range = source
        .indices
        .iter()
        .all(|&i| usize::try_from(i).is_ok_and(|i| i < vertices));
    if !finite
        || !in_range
        || vertices > MAX_MESH_VERTICES
        || source.indices.len() > MAX_MESH_INDICES
    {
        return None;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "positions are rounded to f32 on purpose"
    )]
    let positions: Vec<[f32; 3]> = source
        .positions
        .iter()
        .map(|p| {
            [
                (p.x - origin[0]) as f32,
                (p.y - origin[1]) as f32,
                (p.z - origin[2]) as f32,
            ]
        })
        .collect();
    #[expect(clippy::cast_possible_truncation, reason = "unit normals fit f32")]
    let normals: Vec<[f32; 3]> = source
        .normals
        .iter()
        .map(|n| [n.x as f32, n.y as f32, n.z as f32])
        .collect();
    let (bounds_min, bounds_max) = bounds(&positions);
    Some(Mesh {
        id: 0,
        origin,
        positions,
        normals,
        indices: source.indices.clone(),
        bounds_min,
        bounds_max,
    })
}

/// The box that encloses the `f32` positions exactly (the decoder checks it).
fn bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for p in positions {
        for axis in 0..3 {
            min[axis] = min[axis].min(p[axis]);
            max[axis] = max[axis].max(p[axis]);
        }
    }
    (min, max)
}

/// Whether two meshes have the same geometry, whatever their ids.
fn same_geometry(a: &Mesh, b: &Mesh) -> bool {
    a.origin.map(f64::to_bits) == b.origin.map(f64::to_bits)
        && a.indices == b.indices
        && a.positions.len() == b.positions.len()
        && bits(&a.positions).eq(bits(&b.positions))
        && bits(&a.normals).eq(bits(&b.normals))
}

fn bits(values: &[[f32; 3]]) -> impl Iterator<Item = [u32; 3]> + '_ {
    values.iter().map(|v| v.map(f32::to_bits))
}

/// FNV-1a over the bytes of the geometry: the origin, the positions, the normals and the
/// indices.
fn content_hash(mesh: &Mesh) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    for v in mesh.origin {
        feed(&v.to_le_bytes());
    }
    for v in mesh.positions.iter().chain(&mesh.normals).flatten() {
        feed(&v.to_le_bytes());
    }
    for v in &mesh.indices {
        feed(&v.to_le_bytes());
    }
    hash
}

#[cfg(test)]
mod tests;
