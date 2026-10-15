//! Components, types and instances (CORE-02, ADR 0014).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use schema::{Attribute, Entity, Namespace, Placement, TypeDef};

use super::{Model, ModelError, find_attribute};
use crate::store::{Side, Table};
use crate::{AttributeValue, EntityId, StoreError};

/// A reusable parametric definition of an element, such as a hinged door, from which types are
/// made (spec §16.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// The entity its types and instances belong to, such as `Door`; it must have a type.
    pub entity: String,
    /// Where the component comes from, exported with its types (INT-07).
    pub library: LibraryRef,
    /// Values of type attributes that types made from the component start with.
    pub defaults: Vec<(String, AttributeValue)>,
}

/// The library reference of a component: its id and version (INT-07, INT-08).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryRef {
    /// The component's id in its library.
    pub id: String,
    /// The component's version.
    pub version: u32,
}

/// The links between components, types and instances.
#[derive(Clone, Debug, Default)]
pub(super) struct Types {
    /// Type names, such as `WallType`, and the entity each one types.
    entities: BTreeMap<String, String>,
    components: HashMap<EntityId, Component>,
    /// The component each type was made from.
    component_of: HashMap<EntityId, EntityId>,
    type_of: HashMap<EntityId, EntityId>,
    /// The instances of each type, sorted.
    instances: HashMap<EntityId, BTreeSet<EntityId>>,
}

impl Types {
    pub(super) fn new(namespace: &Namespace) -> Self {
        let entities = namespace
            .entities
            .values()
            .filter_map(|entity| {
                let type_def = entity.type_def.as_ref()?;
                Some((type_def.name.clone(), entity.name.clone()))
            })
            .collect();
        Self {
            entities,
            ..Self::default()
        }
    }

    /// The entity typed by the type named `name`.
    fn entity_of_type(&self, name: &str) -> Option<&str> {
        self.entities.get(name).map(String::as_str)
    }

    pub(super) fn has_component(&self, id: EntityId) -> bool {
        self.components.contains_key(&id)
    }

    pub(super) fn type_of(&self, instance: EntityId) -> Option<EntityId> {
        self.type_of.get(&instance).copied()
    }

    fn instances(&self, type_id: EntityId) -> impl Iterator<Item = EntityId> {
        self.instances.get(&type_id).into_iter().flatten().copied()
    }

    /// Links an instance to a type, replacing its previous type, which is returned.
    pub(super) fn assign(&mut self, instance: EntityId, type_id: EntityId) -> Option<EntityId> {
        let previous = self.unassign(instance);
        self.type_of.insert(instance, type_id);
        self.instances.entry(type_id).or_default().insert(instance);
        previous
    }

    /// Unlinks an instance from its type and returns the type.
    pub(super) fn unassign(&mut self, instance: EntityId) -> Option<EntityId> {
        let type_id = self.type_of.remove(&instance)?;
        super::remove_from(&mut self.instances, &type_id, &instance);
        Some(type_id)
    }

    /// Unlinks every instance of a type and its component, and returns them.
    pub(super) fn release(&mut self, type_id: EntityId) -> (Vec<EntityId>, Option<EntityId>) {
        let instances = self.instances.remove(&type_id).unwrap_or_default();
        for instance in &instances {
            self.type_of.remove(instance);
        }
        (
            instances.into_iter().collect(),
            self.component_of.remove(&type_id),
        )
    }

    pub(super) fn link_component(&mut self, type_id: EntityId, component: EntityId) {
        self.component_of.insert(type_id, component);
    }
}

/// What an object is with respect to types.
#[derive(Clone, Copy, Debug)]
pub(super) enum Object<'a> {
    /// An object of an entity, typed or not.
    Instance(&'a Entity),
    /// A type object of an entity.
    Type(&'a Entity),
}

impl<'a> Object<'a> {
    pub(super) fn entity(self) -> &'a Entity {
        match self {
            Self::Instance(entity) | Self::Type(entity) => entity,
        }
    }

    pub(super) fn is_type(self) -> bool {
        matches!(self, Self::Type(_))
    }

    /// Checks that a value of `attribute`, of the entity or of one of its property sets, may be
    /// set on this object.
    pub(super) fn check_placement(
        self,
        owner: &str,
        attribute: &Attribute,
    ) -> Result<(), ModelError> {
        match (self, attribute.placement) {
            (Self::Instance(_), Placement::Type) => Err(ModelError::NotOverridable {
                owner: owner.to_owned(),
                name: attribute.name.clone(),
            }),
            (Self::Type(_), Placement::Instance) => Err(ModelError::NotOnType {
                owner: owner.to_owned(),
                name: attribute.name.clone(),
            }),
            _ => Ok(()),
        }
    }
}

impl Model {
    /// Stores a component.
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the id is taken, the entity is unknown or has no type, or a
    /// default is not a type attribute of the entity, is given twice or does not fit.
    pub fn create_component(
        &mut self,
        id: EntityId,
        component: Component,
    ) -> Result<(), ModelError> {
        self.check_free(id)?;
        let entity = self
            .namespace
            .entities
            .get(&component.entity)
            .ok_or_else(|| StoreError::UnknownEntity(component.entity.clone()))?;
        let type_def = type_def(entity)?;
        // The defaults must fit a type's columns, none of which is required of a component.
        Table::new(&type_def.name, &entity.attributes, Side::Component)
            .insert(id, component.defaults.iter().cloned())?;
        self.types.components.insert(id, component);
        Ok(())
    }

    /// The component with this id.
    #[must_use]
    pub fn component(&self, id: EntityId) -> Option<&Component> {
        self.types.components.get(&id)
    }

    /// Removes the component with this id and returns it.
    ///
    /// # Errors
    ///
    /// Fails when no component has this id, or types made from it still exist.
    pub fn remove_component(&mut self, id: EntityId) -> Result<Component, ModelError> {
        if self
            .types
            .component_of
            .values()
            .any(|&component| component == id)
        {
            return Err(ModelError::ComponentInUse);
        }
        self.types
            .components
            .remove(&id)
            .ok_or(ModelError::ComponentNotFound(id))
    }

    /// Stores a new type object made from `component`, with the given values; values not given
    /// come from the component's defaults, then from the schema's. The defaults are copied: a
    /// later change of the component does not change existing types.
    ///
    /// A type with no component, such as one imported from IFC, is stored with
    /// [`Model::insert`] under the type's name, such as `WallType`.
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the id is taken, the component does not exist, or the
    /// attribute tables reject the values.
    pub fn create_type<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        component: EntityId,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        self.check_free(id)?;
        let definition = self
            .types
            .components
            .get(&component)
            .ok_or(ModelError::ComponentNotFound(component))?;
        let mut values: Vec<(String, AttributeValue)> = values
            .into_iter()
            .map(|(name, value)| (name.as_ref().to_owned(), value))
            .collect();
        let defaults: Vec<_> = definition
            .defaults
            .iter()
            .filter(|(name, _)| values.iter().all(|(given, _)| given != name))
            .cloned()
            .collect();
        values.extend(defaults);
        let entity = self
            .namespace
            .entities
            .get(&definition.entity)
            .ok_or_else(|| StoreError::UnknownEntity(definition.entity.clone()))?;
        self.store.insert(id, &type_def(entity)?.name, values)?;
        self.types.link_component(id, component);
        Ok(())
    }

    /// The component a type object was made from.
    #[must_use]
    pub fn component_of(&self, type_id: EntityId) -> Option<EntityId> {
        self.types.component_of.get(&type_id).copied()
    }

    /// Makes `type_id` the type of `instance`, or with `None` leaves it untyped, and returns
    /// its previous type. The instance then inherits every value it does not override.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when either object does not exist, the instance's entity has no
    /// type, or the type is not a type of that entity.
    pub fn assign_type(
        &mut self,
        instance: EntityId,
        type_id: Option<EntityId>,
    ) -> Result<Option<EntityId>, ModelError> {
        let Object::Instance(entity) = self.object(instance)? else {
            return Err(ModelError::NotAnInstance);
        };
        type_def(entity)?;
        let Some(type_id) = type_id else {
            return Ok(self.types.unassign(instance));
        };
        match self.object(type_id)? {
            Object::Type(typed) if typed.name == entity.name => {
                Ok(self.types.assign(instance, type_id))
            }
            _ => Err(ModelError::WrongType {
                entity: entity.name.clone(),
                type_entity: self.entity_of(type_id).unwrap_or_default().to_owned(),
            }),
        }
    }

    /// The type of an instance, if it has one.
    #[must_use]
    pub fn type_of(&self, instance: EntityId) -> Option<EntityId> {
        self.types.type_of(instance)
    }

    /// The instances of a type, sorted by id.
    pub fn instances_of(&self, type_id: EntityId) -> impl Iterator<Item = EntityId> {
        self.types.instances(type_id)
    }

    /// The instances of a type whose value of `attribute` comes from the type: those without
    /// an override. A change of the type's value changes theirs, so derived data that reads it
    /// is stale (CORE-08).
    ///
    /// # Errors
    ///
    /// Fails when no object has this id or it holds no such attribute.
    pub fn inheriting_instances(
        &self,
        type_id: EntityId,
        attribute: &str,
    ) -> Result<impl Iterator<Item = EntityId>, ModelError> {
        self.store.get(type_id, attribute)?;
        let overridable = find_attribute(&self.object(type_id)?.entity().attributes, attribute)
            .is_some_and(|definition| definition.placement == Placement::TypeOverridable);
        Ok(self.types.instances(type_id).filter(move |&instance| {
            !overridable || matches!(self.store.get(instance, attribute), Ok(None))
        }))
    }

    /// What the object with this id is, and its entity's definition.
    pub(super) fn object(&self, id: EntityId) -> Result<Object<'_>, ModelError> {
        let name = self.store.entity_of(id).ok_or(StoreError::NotFound(id))?;
        let entities = &self.namespace.entities;
        if let Some(entity) = entities.get(name) {
            return Ok(Object::Instance(entity));
        }
        self.types
            .entity_of_type(name)
            .and_then(|entity| entities.get(entity))
            .map(Object::Type)
            .ok_or_else(|| StoreError::UnknownEntity(name.to_owned()).into())
    }

    /// Fails when an object or component already has this id.
    pub(super) fn check_free(&self, id: EntityId) -> Result<(), ModelError> {
        if self.store.contains(id) || self.types.has_component(id) {
            Err(StoreError::DuplicateId(id).into())
        } else {
            Ok(())
        }
    }
}

/// The type definition of an entity, which must have one.
fn type_def(entity: &Entity) -> Result<&TypeDef, ModelError> {
    entity
        .type_def
        .as_ref()
        .ok_or_else(|| ModelError::NoType(entity.name.clone()))
}
