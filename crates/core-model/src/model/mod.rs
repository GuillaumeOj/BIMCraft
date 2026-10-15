//! The model: stored objects, the relationships between them (CORE-03, ADR 0012), their
//! property sets (CORE-04, ADR 0013), and components, types and instances (CORE-02, ADR 0014).

mod error;
mod property_sets;
mod relationships;
mod types;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::Hash;

use schema::{Attribute, EntityKind, Namespace, Placement};

pub use self::error::ModelError;
use self::property_sets::PropertyTable;
pub use self::relationships::Participation;
use self::relationships::{Link, Relationships};
pub use self::types::{Component, LibraryRef};
use self::types::{Object, Types};
use crate::store::default_value;
use crate::{AttributeValue, EntityId, RemovedEntity, Store, StoreError};

/// Every object of a model, the relationships between them, their property sets, and the
/// components and types they are made from.
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
/// # Property sets
///
/// Properties are grouped in property sets (CORE-04) with typed values; quantities are stored in
/// the SI unit of their kind and read and written as [`Quantity`](crate::units::Quantity) values
/// of the matching dimension (CORE-09). The schema's built-in sets and the sets users and
/// plugins add with [`Model::define_property_set`] are one mechanism: same definition, same
/// validation, same storage and same checks on every value. An object carries a set it applies
/// to once it has a value for it; reading a property it has no value for gives the default.
///
/// # Components, types and instances
///
/// The three are separate (CORE-02). A [`Component`] is a reusable definition, such as a hinged
/// door; a type is an object made from it with [`Model::create_type`], such as a 90 × 215 cm
/// door, stored in the table named after the entity's type (`DoorType`); an instance is an object
/// of the entity, linked to a type with [`Model::assign_type`].
///
/// Attributes and properties say where their value is set: `instance` ones on each instance,
/// `type` ones on the type only, and `type_overridable` ones on the type with an optional override
/// on each instance. [`Model::get`] and [`Model::property`] read an instance's own value, else
/// its type's, else the default, so a change on the type reaches every instance that does not
/// override it. Setting a `type` value on an instance fails with [`ModelError::NotOverridable`].
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
///    the walls it contained, and removing a relationship deletes only that relationship;
/// 4. the property values of every deleted object go with it;
/// 5. deleting a type leaves its instances untyped. A component is removed with
///    [`Model::remove_component`], once no type is made from it.
///
/// The returned [`Removal`] lists what was deleted and [`Model::restore`] puts it back exactly,
/// participants in their former positions.
#[derive(Clone, Debug)]
pub struct Model {
    namespace: Namespace,
    store: Store,
    relationships: Relationships,
    /// Values of every property set, built-in and custom, by name.
    property_sets: BTreeMap<String, PropertyTable>,
    types: Types,
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
    /// The property sets it carried, with their values.
    properties: Vec<(String, Vec<(String, AttributeValue)>)>,
    /// The type of an instance.
    type_of: Option<EntityId>,
    /// The instances of a type.
    instances: Vec<EntityId>,
    /// The component a type was made from.
    component: Option<EntityId>,
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
            property_sets: namespace
                .property_sets
                .values()
                .map(|set| (set.name.clone(), PropertyTable::new(set)))
                .collect(),
            types: Types::new(namespace),
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

    /// Stores a new object that is not a relationship, or a type object with no component under
    /// the type's name; see [`Store::insert`].
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the id is taken, the entity is a relationship (use
    /// [`Model::relate`]) or the attribute tables reject the object.
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
        self.check_free(id)?;
        Ok(self.store.insert(id, entity, values)?)
    }

    /// The value of an attribute of the object with this id: its own value, else, for an
    /// instance and an attribute set on the type, its type's value, else the default.
    ///
    /// # Errors
    ///
    /// Fails when no object has this id, its entity has no such attribute, or the object is a
    /// type and the attribute is set on each instance.
    pub fn get(&self, id: EntityId, attribute: &str) -> Result<Option<AttributeValue>, ModelError> {
        let object = self.object(id)?;
        let entity = object.entity();
        let Some(definition) = find_attribute(&entity.attributes, attribute) else {
            return Ok(self.store.get(id, attribute)?);
        };
        self.resolve(id, object, &entity.name, definition, |id, _| {
            self.store.get(id, attribute).ok().flatten()
        })
    }

    /// Sets, or with `None` clears, an attribute of the object with this id and returns its
    /// previous stored value. On a type, the change reaches every instance that does not
    /// override it; on an instance, a `type_overridable` value is an override, and clearing it
    /// brings back the type's value.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the attribute is set on the type and the object is an
    /// instance, is set on instances and the object is a type, or the attribute tables reject
    /// the change.
    pub fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let object = self.object(id)?;
        let entity = object.entity();
        if let Some(definition) = find_attribute(&entity.attributes, attribute) {
            object.check_placement(&entity.name, definition)?;
        }
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
            let detached: Vec<Detached> = self
                .relationships
                .detach_all(next)
                .into_iter()
                .map(|(link, position)| Detached { link, position })
                .collect();
            pending.extend(
                detached
                    .iter()
                    .map(|detached| detached.link)
                    .filter(|link| {
                        self.relationships
                            .role(link.relationship, link.role)
                            .is_empty()
                    })
                    .map(|link| link.relationship),
            );
            let roles = self.relationships.take(next);
            let type_of = self.types.unassign(next);
            let (instances, component) = self.types.release(next);
            let is_type = self.object(next)?.is_type();
            let properties = self
                .property_sets
                .iter_mut()
                .filter_map(|(name, table)| {
                    Some((name.clone(), table.side_mut(is_type).remove(next)?))
                })
                .collect();
            let entity = self.store.remove(next)?;
            objects.push(RemovedObject {
                id: next,
                entity,
                roles,
                detached,
                properties,
                type_of,
                instances,
                component,
            });
        }
        Ok(Removal { objects })
    }

    /// Puts back what [`Model::remove`] deleted, as it was.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the model changed since the removal so that an id is taken
    /// again, a participant, relationship, type or component the removal refers to is gone, a
    /// participant now plays an exclusive role it would play again, or an instance of a removed
    /// type was given another type.
    pub fn restore(&mut self, removal: Removal) -> Result<(), ModelError> {
        let restored: BTreeSet<EntityId> = removal.ids().collect();
        let present = |id: &EntityId| restored.contains(id) || self.store.contains(*id);
        for object in &removal.objects {
            self.check_free(object.id)?;
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
            let mut linked = object.type_of.iter().chain(&object.instances).chain(
                object
                    .detached
                    .iter()
                    .map(|detached| &detached.link.relationship),
            );
            if let Some(&gone) = linked.find(|id| !present(id)) {
                return Err(StoreError::NotFound(gone).into());
            }
            if let Some(&retyped) = object
                .instances
                .iter()
                .find(|&&instance| self.types.type_of(instance).is_some())
            {
                return Err(ModelError::InstanceRetyped(retyped));
            }
            if let Some(component) = object.component.filter(|&id| !self.types.has_component(id)) {
                return Err(ModelError::ComponentNotFound(component));
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
            let is_type = self.object(object.id)?.is_type();
            for (set, values) in object.properties {
                self.property_table(&set)?
                    .side_mut(is_type)
                    .insert(object.id, values)?;
            }
            if let Some(type_id) = object.type_of {
                self.types.assign(object.id, type_id);
            }
            for instance in object.instances {
                self.types.assign(instance, object.id);
            }
            if let Some(component) = object.component {
                self.types.link_component(object.id, component);
            }
        }
        Ok(())
    }

    /// The value of an attribute or property of an object (CORE-02): its own value, else, for an
    /// instance and a value set on the type, its type's value, else the default. `stored` reads
    /// a stored value of an object, saying whether that object is a type.
    fn resolve(
        &self,
        id: EntityId,
        object: Object<'_>,
        owner: &str,
        attribute: &Attribute,
        stored: impl Fn(EntityId, bool) -> Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let own = match (object, attribute.placement) {
            (Object::Type(_), _) => {
                object.check_placement(owner, attribute)?;
                stored(id, true)
            }
            (Object::Instance(_), Placement::Type) => None,
            (Object::Instance(_), _) => stored(id, false),
        };
        let inherits = !object.is_type() && attribute.placement != Placement::Instance;
        Ok(own
            .or_else(|| {
                let type_id = self.types.type_of(id).filter(|_| inherits)?;
                stored(type_id, true)
            })
            .or_else(|| default_value(attribute)))
    }
}

/// Removes `value` from the set under `key`, and the set once it is empty.
fn remove_from<K: Eq + Hash, V: Ord>(map: &mut HashMap<K, BTreeSet<V>>, key: &K, value: &V) {
    let emptied = map.get_mut(key).is_some_and(|set| {
        set.remove(value);
        set.is_empty()
    });
    if emptied {
        map.remove(key);
    }
}

fn find_attribute<'a>(attributes: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
    attributes.iter().find(|attribute| attribute.name == name)
}

#[cfg(test)]
mod property_set_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod type_tests;
