//! The scene: element meshes kept current from change events (GEO-05, CORE-08).

use std::collections::{BTreeMap, BTreeSet};

use core_model::derived::DependencyGraph;
use core_model::{AttributeValue, EntityId, Model};
use core_txn::ChangeEvent;
use schema::Representation;

use crate::mesh::ElementMesh;
use crate::{Generated, Generators, SceneError, View};

/// A node of the dependency graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// The mesh of an element, computed from the objects it depends on.
    Mesh(EntityId),
    /// An object of the model, read by meshes: an input of the graph.
    Object(EntityId),
}

/// The meshes of the model's elements, kept current incrementally (GEO-05).
///
/// [`Scene::rebuild`] meshes every element with a representation in the scene's [`View`] once.
/// After each transaction, undo or redo, [`Scene::apply`] marks stale the meshes its
/// [`ChangeEvent`] can have changed, and [`Scene::update`] re-meshes those and only those, and
/// reports what changed since the last update. Nothing is computed before it is needed, and
/// [`Scene::mesh`] re-meshes a stale element on demand.
///
/// The mesh of an element depends on its own object, its type, every relationship it takes part
/// in, the objects its own and its type's attributes refer to, and the objects its generator
/// reports reading ([`Generated::inputs`]): an object-level granularity, which change events
/// provide (ADR 0023).
#[derive(Debug)]
pub struct Scene {
    generators: Generators,
    view: View,
    graph: DependencyGraph<Key, ()>,
    /// The mesh of each element that has one.
    meshes: BTreeMap<EntityId, ElementMesh>,
    /// Why each element without a mesh has none.
    failures: BTreeMap<EntityId, SceneError>,
    /// Elements to mesh again, or for the first time.
    stale: BTreeSet<EntityId>,
    /// Elements whose state changed since the last update was reported.
    dirty: BTreeSet<EntityId>,
    /// What the last update told about each element: whether it had a mesh.
    reported: BTreeMap<EntityId, bool>,
}

/// What an update changed, for the viewport to upload and for tests to count.
///
/// It is the net effect since the previous update, as the viewport last saw it: an element is
/// reported once, whatever happened to it in between.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneUpdate {
    /// Elements that now have a mesh and had none.
    pub added: BTreeSet<EntityId>,
    /// Elements that had a mesh and have a new one.
    pub changed: BTreeSet<EntityId>,
    /// Elements that were deleted.
    pub removed: BTreeSet<EntityId>,
    /// Elements that could not be meshed, and why. They are left without a mesh: one that had a
    /// mesh loses it, and must be dropped from the viewport.
    pub failed: BTreeMap<EntityId, SceneError>,
}

impl SceneUpdate {
    /// The number of elements the update meshed: [`added`](Self::added) and
    /// [`changed`](Self::changed).
    #[must_use]
    pub fn remeshed(&self) -> usize {
        self.added.len() + self.changed.len()
    }
}

impl Scene {
    /// An empty scene that meshes the representations `view` picks, with `generators`. Call
    /// [`Scene::rebuild`] to mesh a model.
    #[must_use]
    pub fn new(generators: Generators, view: View) -> Self {
        Self {
            generators,
            view,
            graph: DependencyGraph::new(),
            meshes: BTreeMap::new(),
            failures: BTreeMap::new(),
            stale: BTreeSet::new(),
            dirty: BTreeSet::new(),
            reported: BTreeMap::new(),
        }
    }

    /// Meshes every element of `model` that has a matching representation, forgetting what the
    /// scene held. The elements it knew that the model no longer has are `removed`.
    pub fn rebuild(&mut self, model: &Model) -> SceneUpdate {
        self.graph = DependencyGraph::new();
        self.meshes.clear();
        self.failures.clear();
        self.stale.clear();
        self.dirty.extend(self.reported.keys().copied());
        for id in model.ids() {
            if self.representation(model, id).is_some() {
                self.mark_stale(id);
            }
        }
        self.update(model)
    }

    /// Marks stale what `event` can have changed. Computes nothing: [`Scene::update`] or
    /// [`Scene::mesh`] does.
    ///
    /// - a created or modified object makes stale every mesh that depends on it;
    /// - a deleted element loses its mesh, and a deleted object makes stale the meshes that
    ///   depended on it, a type, a relationship or an input;
    /// - a created element with a matching representation is meshed by the next update;
    /// - a created or modified relationship makes stale the meshes of its current participants,
    ///   since no generator can have read it before.
    pub fn apply(&mut self, event: &ChangeEvent, model: &Model) {
        for &id in &event.deleted {
            self.forget(id);
        }
        for &id in event
            .created
            .iter()
            .chain(&event.modified)
            .chain(&event.deleted)
        {
            for stale in self.graph.invalidate([Key::Object(id)]) {
                if let Key::Mesh(element) = stale {
                    self.mark_stale(element);
                }
            }
        }
        for &id in &event.created {
            if self.representation(model, id).is_some() {
                self.mark_stale(id);
            }
        }
        for &id in event.created.iter().chain(&event.modified) {
            for participant in participants(model, id) {
                if self.is_tracked(participant) {
                    self.mark_stale(participant);
                }
            }
        }
    }

    /// Meshes the stale elements, and only them, and reports what changed since the last update.
    pub fn update(&mut self, model: &Model) -> SceneUpdate {
        for id in std::mem::take(&mut self.stale) {
            self.remesh(model, id);
        }
        self.flush()
    }

    /// The current mesh of an element, meshing it first if it is stale. `None` for an element
    /// that has none: it is not meshed, was deleted, or failed.
    ///
    /// The next [`Scene::update`] still reports the element.
    pub fn mesh(&mut self, model: &Model, element: EntityId) -> Option<&ElementMesh> {
        if self.stale.remove(&element) {
            self.remesh(model, element);
        }
        self.meshes.get(&element)
    }

    /// The representation of `id` the scene's view picks, if `id` is an element that has one.
    fn representation<'m>(&self, model: &'m Model, id: EntityId) -> Option<&'m Representation> {
        let entity = model.namespace().entities.get(model.entity_of(id)?)?;
        self.view.pick(&entity.representations)
    }

    /// Whether the scene meshes `id`, or is about to.
    fn is_tracked(&self, id: EntityId) -> bool {
        self.meshes.contains_key(&id) || self.failures.contains_key(&id) || self.stale.contains(&id)
    }

    fn mark_stale(&mut self, id: EntityId) {
        self.stale.insert(id);
        self.dirty.insert(id);
    }

    /// Drops everything the scene holds about a deleted object.
    fn forget(&mut self, id: EntityId) {
        self.graph.remove(&Key::Mesh(id));
        self.meshes.remove(&id);
        self.failures.remove(&id);
        self.stale.remove(&id);
        self.dirty.insert(id);
    }

    /// Meshes one element again and records the result and its dependencies.
    fn remesh(&mut self, model: &Model, id: EntityId) {
        let Some(representation) = self.representation(model, id) else {
            // Not an element any more, or not in this model: nothing to mesh.
            self.forget(id);
            return;
        };
        self.dirty.insert(id);
        let (mut dependencies, outcome): (BTreeSet<Key>, _) =
            match self.compute(model, id, &representation.generator) {
                Ok((mesh, inputs)) => (inputs.into_iter().map(Key::Object).collect(), Ok(mesh)),
                // The inputs a failed generator read are unknown: keep the ones it read before, so
                // that fixing one of them meshes the element again.
                Err(error) => (
                    self.graph.dependencies(&Key::Mesh(id)).copied().collect(),
                    Err(error),
                ),
            };
        dependencies.extend(base_dependencies(model, id));
        let declared = self.declare(id, dependencies);
        match outcome.and_then(|mesh| declared.map(|()| mesh)) {
            Ok(mesh) => {
                self.failures.remove(&id);
                self.meshes.insert(id, mesh);
            }
            Err(error) => {
                self.meshes.remove(&id);
                self.failures.insert(id, error);
            }
        }
    }

    /// The mesh of an element and the objects its generator read.
    fn compute(
        &self,
        model: &Model,
        id: EntityId,
        generator: &str,
    ) -> Result<(ElementMesh, BTreeSet<EntityId>), SceneError> {
        let generator = self
            .generators
            .get(generator)
            .ok_or_else(|| SceneError::NoGenerator {
                generator: generator.to_owned(),
            })?;
        let Generated { solids, inputs } = generator.generate(model, id)?;
        let parts = solids
            .iter()
            .enumerate()
            .map(|(solid, s)| {
                s.tessellate()
                    .map_err(|source| SceneError::Tessellation { solid, source })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((ElementMesh::merge(parts)?, inputs))
    }

    /// Records what the mesh of `id` depends on, and marks it up to date.
    fn declare(&mut self, id: EntityId, dependencies: BTreeSet<Key>) -> Result<(), SceneError> {
        let mesh = Key::Mesh(id);
        self.graph
            .set_dependencies(mesh, dependencies)
            .map_err(|_| SceneError::Dependencies)?;
        // Nothing is stored in the graph but what is stale and what is not: getting the node
        // marks it and its inputs up to date.
        self.graph
            .get(&mesh, |_, _| ())
            .map(drop)
            .map_err(|_| SceneError::Dependencies)
    }

    /// Tells what changed since the last flush, compared with what it told then.
    fn flush(&mut self) -> SceneUpdate {
        let mut update = SceneUpdate::default();
        for id in std::mem::take(&mut self.dirty) {
            let now = if self.meshes.contains_key(&id) {
                Some(true)
            } else {
                self.failures.contains_key(&id).then_some(false)
            };
            match (self.reported.get(&id).copied(), now) {
                (Some(true), Some(true)) => {
                    update.changed.insert(id);
                }
                (_, Some(true)) => {
                    update.added.insert(id);
                }
                (_, Some(false)) => {
                    if let Some(error) = self.failures.get(&id) {
                        update.failed.insert(id, error.clone());
                    }
                }
                (Some(_), None) => {
                    update.removed.insert(id);
                }
                (None, None) => {}
            }
            match now {
                Some(meshed) => self.reported.insert(id, meshed),
                None => self.reported.remove(&id),
            };
        }
        update
    }
}

/// What the mesh of an element always depends on: its own object, its type, the relationships it
/// takes part in and the objects its own and its type's attributes refer to. A generator reads
/// the model through these, whether or not it gets as far as saying so.
fn base_dependencies(model: &Model, id: EntityId) -> BTreeSet<Key> {
    let mut dependencies = BTreeSet::from([Key::Object(id)]);
    let kind = model.type_of(id);
    dependencies.extend(kind.map(Key::Object));
    dependencies.extend(
        model
            .relationships_of(id)
            .map(|role| Key::Object(role.relationship)),
    );
    for object in std::iter::once(id).chain(kind) {
        if let Some(state) = model.state(object) {
            dependencies.extend(references(&state.values).map(Key::Object));
        }
    }
    dependencies
}

/// The objects attribute values refer to, in lists and sourced values too.
pub(crate) fn references(values: &[(String, AttributeValue)]) -> impl Iterator<Item = EntityId> {
    let mut pending: Vec<&AttributeValue> = values.iter().map(|(_, value)| value).collect();
    let mut found = Vec::new();
    // Iterative: nesting is bounded by the schema, but values are model content.
    while let Some(value) = pending.pop() {
        match value {
            AttributeValue::Ref(target) => found.push(*target),
            AttributeValue::List(items) => pending.extend(items),
            AttributeValue::Sourced(sourced) => pending.push(&sourced.value),
            _ => {}
        }
    }
    found.into_iter()
}

/// The objects that take part in `relationship` now, in any role; none for an object that is not
/// a relationship.
fn participants(model: &Model, relationship: EntityId) -> Vec<EntityId> {
    let Some(entity) = model
        .entity_of(relationship)
        .and_then(|name| model.namespace().entities.get(name))
    else {
        return Vec::new();
    };
    entity
        .roles
        .iter()
        .filter_map(|role| model.participants(relationship, &role.name).ok())
        .flatten()
        .copied()
        .collect()
}
