//! The model: stored objects and the relationships between them (CORE-03, ADR 0012).

mod error;
mod relationships;

use std::collections::BTreeSet;

use schema::{EntityKind, Namespace};

pub use self::error::ModelError;
pub use self::relationships::Participation;
use self::relationships::{Link, Relationships};
use crate::{AttributeValue, EntityId, RemovedEntity, Store, StoreError};

/// Every object of a model and the relationships between them.
///
/// A model owns the attribute tables of a namespace ([`Store`]) and keeps every
/// cross-object rule in one place.
///
/// # Relationships
///
/// Relationships are objects (CORE-03): an object of a relationship entity, such as
/// `Containment`, has its own [`EntityId`] and attributes, and relates other objects through the
/// roles its entity declares. [`Model::relate`] checks the roles against the schema: every role
/// is given, a `one` role has exactly one participant, participants exist and are allowed
/// targets, and an object plays an `exclusive` role in at most one relationship of that entity.
/// [`Model::participants`] reads a relationship's roles and [`Model::relationships_of`] the roles
/// an object plays, so relationships can be queried both ways.
///
/// # Deleting objects
///
/// [`Model::remove`] follows one rule, mirroring IFC, where a relationship cannot exist without
/// its required ends:
///
/// 1. the object leaves every role it plays;
/// 2. a relationship left with an empty role is deleted too, and so on for relationships that
///    relate it;
/// 3. no other object is deleted: removing a storey deletes its containment relationship but not
///    the walls it contained, and removing a relationship deletes only that relationship.
///
/// The returned [`Removal`] lists what was deleted and [`Model::restore`] puts it back exactly,
/// participants in their former positions.
#[derive(Clone, Debug)]
pub struct Model {
    namespace: Namespace,
    store: Store,
    relationships: Relationships,
}

/// What [`Model::remove`] deleted; [`Model::restore`] puts it back.
#[derive(Clone, Debug, PartialEq)]
pub struct Removal {
    /// In deletion order: the removed object first, then the relationships deleted with it.
    objects: Vec<RemovedObject>,
}

#[derive(Clone, Debug, PartialEq)]
struct RemovedObject {
    id: EntityId,
    entity: RemovedEntity,
    /// The participants of a relationship object, when it still had them.
    roles: Option<Vec<Vec<EntityId>>>,
    /// The roles the object left, in the order it left them.
    detached: Vec<Detached>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Detached {
    link: Link,
    position: usize,
}

impl Removal {
    /// The deleted objects: the removed one first, then the relationships deleted with it.
    pub fn ids(&self) -> impl Iterator<Item = EntityId> {
        self.objects.iter().map(|object| object.id)
    }
}

impl Model {
    /// Creates an empty model of `namespace`.
    ///
    /// # Errors
    ///
    /// Fails when the namespace does not pass [`schema::validate`].
    pub fn new(namespace: &Namespace) -> Result<Self, ModelError> {
        Ok(Self {
            store: Store::new(namespace)?,
            namespace: namespace.clone(),
            relationships: Relationships::default(),
        })
    }

    /// Number of stored objects, relationships included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Whether no object is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }

    /// Whether an object with this id is stored.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.store.contains(id)
    }

    /// The entity of the object with this id.
    #[must_use]
    pub fn entity_of(&self, id: EntityId) -> Option<&str> {
        self.store.entity_of(id)
    }

    /// Stores a new object that is not a relationship; see [`Store::insert`].
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the entity is a relationship (use [`Model::relate`]) or the
    /// attribute tables reject the object.
    pub fn insert<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        entity: &str,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        if self
            .namespace
            .entities
            .get(entity)
            .is_some_and(|definition| definition.kind == EntityKind::Relationship)
        {
            return Err(ModelError::RelationshipEntity(entity.to_owned()));
        }
        Ok(self.store.insert(id, entity, values)?)
    }

    /// The value of an attribute; see [`Store::get`].
    ///
    /// # Errors
    ///
    /// Fails when no object has this id or its entity has no such instance attribute.
    pub fn get(&self, id: EntityId, attribute: &str) -> Result<Option<AttributeValue>, ModelError> {
        Ok(self.store.get(id, attribute)?)
    }

    /// Sets or clears an attribute and returns the previous value; see [`Store::set`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the attribute tables reject the change.
    pub fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        Ok(self.store.set(id, attribute, value)?)
    }

    /// Removes the object with this id, and the relationships it leaves with an empty role, as
    /// described in [Deleting objects](Model#deleting-objects).
    ///
    /// # Errors
    ///
    /// Fails when no object has this id.
    pub fn remove(&mut self, id: EntityId) -> Result<Removal, ModelError> {
        if !self.store.contains(id) {
            return Err(StoreError::NotFound(id).into());
        }
        let mut objects = Vec::new();
        let mut pending = vec![id];
        while let Some(next) = pending.pop() {
            // A relationship emptied twice in one removal is queued twice.
            if !self.store.contains(next) {
                continue;
            }
            let mut detached = Vec::new();
            for link in self.relationships.links(next).collect::<Vec<_>>() {
                if let Some(position) = self.relationships.detach(link, next) {
                    detached.push(Detached { link, position });
                }
                if self
                    .relationships
                    .role(link.relationship, link.role)
                    .is_empty()
                {
                    pending.push(link.relationship);
                }
            }
            let roles = self.relationships.take(next);
            let entity = self.store.remove(next)?;
            objects.push(RemovedObject {
                id: next,
                entity,
                roles,
                detached,
            });
        }
        Ok(Removal { objects })
    }

    /// Puts back what [`Model::remove`] deleted, as it was.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the model changed since the removal so that an id is taken
    /// again, a participant or relationship the removal refers to is gone, or a participant now
    /// plays an exclusive role it would play again.
    pub fn restore(&mut self, removal: Removal) -> Result<(), ModelError> {
        let restored: BTreeSet<EntityId> = removal.ids().collect();
        let present = |id: &EntityId| restored.contains(id) || self.store.contains(*id);
        for object in &removal.objects {
            if self.store.contains(object.id) {
                return Err(StoreError::DuplicateId(object.id).into());
            }
            if let Some(&missing) = object
                .roles
                .iter()
                .flatten()
                .flatten()
                .find(|id| !present(id))
            {
                return Err(ModelError::ParticipantNotFound(missing));
            }
            // An exclusive role may have been given to another relationship meanwhile.
            if let (Some(roles), Some(entity)) = (
                &object.roles,
                self.namespace.entities.get(&object.entity.entity),
            ) {
                for (role, participants) in roles.iter().enumerate() {
                    for &participant in participants {
                        self.check_exclusive(entity, role, participant)?;
                    }
                }
            }
            if let Some(gone) = object
                .detached
                .iter()
                .find(|detached| !present(&detached.link.relationship))
            {
                return Err(StoreError::NotFound(gone.link.relationship).into());
            }
        }
        for object in removal.objects.into_iter().rev() {
            self.store
                .insert(object.id, &object.entity.entity, object.entity.values)?;
            if let Some(roles) = object.roles {
                self.relationships.insert(object.id, roles);
            }
            for detached in object.detached.into_iter().rev() {
                self.relationships
                    .attach(detached.link, object.id, detached.position);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
