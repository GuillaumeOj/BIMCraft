//! Property sets: named groups of typed properties carried by objects (CORE-04, ADR 0013).

use std::collections::HashMap;

use schema::{Attribute, PropertySet, ValueType};

use super::types::Object;
use super::{Model, ModelError, find_attribute};
use crate::store::{Side, Table};
use crate::units::{Quantity, Unit};
use crate::{AttributeValue, EntityId, StoreError};

/// The values of one property set, built-in or custom alike: one table for instances and one
/// for type objects, as each holds different properties (CORE-02).
#[derive(Clone, Debug)]
pub(super) struct PropertyTable {
    instances: Rows,
    types: Rows,
}

/// One side of a property set, one row per object that carries it.
#[derive(Clone, Debug)]
pub(super) struct Rows {
    table: Table,
    rows: HashMap<EntityId, usize>,
}

impl PropertyTable {
    pub(super) fn new(set: &PropertySet) -> Self {
        let rows = |side| Rows {
            table: Table::new(&set.name, &set.properties, side),
            rows: HashMap::new(),
        };
        Self {
            instances: rows(Side::Instance),
            types: rows(Side::Type),
        }
    }

    fn side(&self, is_type: bool) -> &Rows {
        if is_type {
            &self.types
        } else {
            &self.instances
        }
    }

    pub(super) fn side_mut(&mut self, is_type: bool) -> &mut Rows {
        if is_type {
            &mut self.types
        } else {
            &mut self.instances
        }
    }

    fn carries(&self, id: EntityId) -> bool {
        self.instances.carries(id) || self.types.carries(id)
    }
}

impl Rows {
    /// Whether the object carries the set.
    fn carries(&self, id: EntityId) -> bool {
        self.rows.contains_key(&id)
    }

    /// Adds a row for the object.
    pub(super) fn insert<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        let row = self
            .table
            .insert(id, values)
            .map_err(|error| self.value_error(error))?;
        self.rows.insert(id, row);
        Ok(())
    }

    /// The stored value of a property of the object; `None` when the object does not carry the
    /// set or has no value.
    fn get(&self, id: EntityId, property: &str) -> Option<AttributeValue> {
        let row = *self.rows.get(&id)?;
        self.table.get(row, property).ok().flatten()
    }

    /// Sets or clears a property of the object, adding its row when it gives a value, and
    /// returns the previous stored value.
    fn set(
        &mut self,
        id: EntityId,
        property: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        match (self.rows.get(&id), value) {
            (Some(&row), value) => self
                .table
                .set(row, property, value)
                .map_err(|error| self.value_error(error)),
            (None, Some(value)) => self.insert(id, [(property, value)]).map(|()| None),
            (None, None) => Ok(None),
        }
    }

    /// Removes the object's row and returns its values.
    pub(super) fn remove(&mut self, id: EntityId) -> Option<Vec<(String, AttributeValue)>> {
        let row = self.rows.remove(&id)?;
        let (values, moved) = self.table.swap_remove(row);
        if let Some(moved) = moved {
            self.rows.insert(moved, row);
        }
        Some(values)
    }

    fn value_error(&self, error: StoreError) -> ModelError {
        ModelError::PropertyValue {
            set: self.table.name().to_owned(),
            error,
        }
    }
}

impl Model {
    /// Adds a property set defined by a user or a plugin (CORE-04).
    ///
    /// Custom sets follow the same rules as the schema's built-in sets: they are checked by
    /// [`schema::validate`], and their values are stored, read and checked in the same way.
    ///
    /// # Errors
    ///
    /// Fails, adding nothing, when a set of that name exists or the set does not pass
    /// validation, for example because it applies to an unknown entity.
    pub fn define_property_set(&mut self, set: PropertySet) -> Result<(), ModelError> {
        if self.namespace.property_sets.contains_key(&set.name) {
            return Err(ModelError::PropertySetExists(set.name));
        }
        let name = set.name.clone();
        self.namespace.property_sets.insert(name.clone(), set);
        if let Err(errors) = schema::validate(&self.namespace) {
            self.namespace.property_sets.remove(&name);
            return Err(ModelError::InvalidPropertySet(errors));
        }
        let table = PropertyTable::new(&self.namespace.property_sets[&name]);
        self.property_sets.insert(name, table);
        Ok(())
    }

    /// The definition of a property set, built-in or custom: its properties and the value type
    /// of each, which gives the unit of quantities (CORE-09).
    #[must_use]
    pub fn property_set(&self, name: &str) -> Option<&PropertySet> {
        self.namespace.property_sets.get(name)
    }

    /// The property sets the object with this id carries, sorted by name.
    pub fn property_sets_of(&self, id: EntityId) -> impl Iterator<Item = &str> {
        self.property_sets
            .iter()
            .filter(move |(_, table)| table.carries(id))
            .map(|(name, _)| name.as_str())
    }

    /// Makes the object carry property set `set`, with the given values. Properties without a
    /// value take their default, or stay empty when optional. A type object holds the set's
    /// `type` and `type_overridable` properties, an instance its `instance` and
    /// `type_overridable` ones.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the object or set is unknown, the set does not apply to the
    /// object's entity, the object already carries it, a property is unknown or not held by the
    /// object (see [`Model::set_property`]), a property is given twice, a required one is missing, or a value does not fit.
    pub fn attach_property_set<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        set: &str,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        let (definition, object) = self.applicable_set(id, set)?;
        let values: Vec<(N, AttributeValue)> = values.into_iter().collect();
        for (name, _) in &values {
            settable_property(definition, object, name.as_ref())?;
        }
        let is_type = object.is_type();
        let table = self.property_table(set)?.side_mut(is_type);
        if table.carries(id) {
            return Err(ModelError::AlreadyAttached(set.to_owned()));
        }
        table.insert(id, values)
    }

    /// Stops the object carrying property set `set` and returns the values it had.
    ///
    /// # Errors
    ///
    /// Fails when the object or set is unknown, the set does not apply, or the object does not
    /// carry it.
    pub fn detach_property_set(
        &mut self,
        id: EntityId,
        set: &str,
    ) -> Result<Vec<(String, AttributeValue)>, ModelError> {
        let is_type = self.applicable_set(id, set)?.1.is_type();
        self.property_table(set)?
            .side_mut(is_type)
            .remove(id)
            .ok_or_else(|| ModelError::NotAttached(set.to_owned()))
    }

    /// The value of property `property` of set `set` on the object: its own value, else, for an
    /// instance and a property set on the type, its type's value, else the property's default.
    ///
    /// # Errors
    ///
    /// Fails when the object, set or property is unknown, the set does not apply to the
    /// object's entity, or the object is a type and the property is set on each instance.
    pub fn property(
        &self,
        id: EntityId,
        set: &str,
        property: &str,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let (definition, object) = self.applicable_set(id, set)?;
        let attribute = find_property(definition, property)?;
        let table = self.property_sets.get(set);
        self.resolve(id, object, set, attribute, |id, is_type| {
            table.and_then(|table| table.side(is_type).get(id, property))
        })
    }

    /// Sets, or with `None` clears, a property of the object and returns its previous stored
    /// value. The object starts carrying the set if it did not. On a type, the change reaches
    /// every instance that does not override it.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the object, set or property is unknown, the set does not
    /// apply, the property is set on the type and the object is an instance
    /// ([`ModelError::NotOverridable`]) or set on instances and the object is a type, a
    /// required property would be missing, or the value does not fit.
    pub fn set_property(
        &mut self,
        id: EntityId,
        set: &str,
        property: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let (definition, object) = self.applicable_set(id, set)?;
        settable_property(definition, object, property)?;
        let is_type = object.is_type();
        self.property_table(set)?
            .side_mut(is_type)
            .set(id, property, value)
    }

    /// A quantity property, in the dimension of `U` (CORE-09).
    ///
    /// # Errors
    ///
    /// Fails like [`Model::property`], and when the property does not hold quantities of the
    /// dimension of `U`.
    pub fn quantity_property<U: Unit>(
        &self,
        id: EntityId,
        set: &str,
        property: &str,
    ) -> Result<Option<Quantity<U>>, ModelError> {
        self.check_quantity::<U>(set, property)?;
        Ok(match self.property(id, set, property)? {
            Some(AttributeValue::Real(si)) => Some(Quantity::from_si(si)),
            _ => None,
        })
    }

    /// Sets or clears a quantity property and returns its previous stored value; see
    /// [`Model::set_property`].
    ///
    /// # Errors
    ///
    /// Fails like [`Model::set_property`], and when the property does not hold quantities of the
    /// dimension of `U`.
    pub fn set_quantity_property<U: Unit>(
        &mut self,
        id: EntityId,
        set: &str,
        property: &str,
        value: Option<Quantity<U>>,
    ) -> Result<Option<Quantity<U>>, ModelError> {
        self.check_quantity::<U>(set, property)?;
        let value = value.map(|quantity| AttributeValue::Real(quantity.si()));
        Ok(match self.set_property(id, set, property, value)? {
            Some(AttributeValue::Real(si)) => Some(Quantity::from_si(si)),
            _ => None,
        })
    }

    /// The definition of `set`, which must apply to the entity of the object with this id, and
    /// what the object is. A set applies to the types of the entities it applies to.
    fn applicable_set(
        &self,
        id: EntityId,
        set: &str,
    ) -> Result<(&PropertySet, Object<'_>), ModelError> {
        let object = self.object(id)?;
        let definition = self.known_set(set)?;
        let entity = &object.entity().name;
        if definition.applies_to.contains(entity) {
            Ok((definition, object))
        } else {
            Err(ModelError::NotApplicable {
                set: set.to_owned(),
                entity: entity.clone(),
            })
        }
    }

    fn known_set(&self, set: &str) -> Result<&PropertySet, ModelError> {
        self.namespace
            .property_sets
            .get(set)
            .ok_or_else(|| ModelError::UnknownPropertySet(set.to_owned()))
    }

    pub(super) fn property_table(&mut self, set: &str) -> Result<&mut PropertyTable, ModelError> {
        self.property_sets
            .get_mut(set)
            .ok_or_else(|| ModelError::UnknownPropertySet(set.to_owned()))
    }

    /// Checks that a property holds quantities of the dimension of `U`.
    fn check_quantity<U: Unit>(&self, set: &str, property: &str) -> Result<(), ModelError> {
        let attribute = find_property(self.known_set(set)?, property)?;
        match attribute.value {
            // A quantity kind and a dimension match when they share their SI unit.
            ValueType::Quantity(kind) if U::SI.symbol() == kind.si_unit() => Ok(()),
            _ => Err(ModelError::QuantityMismatch {
                property: property.to_owned(),
                dimension: U::DIMENSION,
            }),
        }
    }
}

fn find_property<'a>(set: &'a PropertySet, property: &str) -> Result<&'a Attribute, ModelError> {
    find_attribute(&set.properties, property).ok_or_else(|| ModelError::UnknownProperty {
        set: set.name.clone(),
        property: property.to_owned(),
    })
}

/// Checks that the object can hold a value for the property (CORE-02).
fn settable_property(
    set: &PropertySet,
    object: Object<'_>,
    property: &str,
) -> Result<(), ModelError> {
    object.check_placement(&set.name, find_property(set, property)?)
}
