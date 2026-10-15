//! One table of rows, one typed column per attribute, shared by entity tables and property-set
//! tables.

use schema::{Attribute, Placement, Presence};

use super::StoreError;
use super::column::Column;
use super::value::{AttributeValue, ValueError};
use crate::EntityId;

/// Rows of one entity or property set: an id per row and a column per attribute.
#[derive(Clone, Debug)]
pub(crate) struct Table {
    name: String,
    ids: Vec<EntityId>,
    attributes: Vec<Slot>,
}

/// One attribute of a table: its schema rules and its column.
#[derive(Clone, Debug)]
struct Slot {
    name: String,
    rule: Rule,
    column: Column,
}

/// Which objects a table holds, which decides its columns and rules (CORE-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Side {
    /// Instances: `instance` and `type_overridable` attributes. An instance's
    /// `type_overridable` value is an optional override, as its type provides the value.
    Instance,
    /// Type objects: `type` and `type_overridable` attributes, required and defaulted as
    /// declared.
    Type,
    /// Component defaults for the types made from it: the type's columns, every one optional.
    Component,
}

impl Side {
    fn holds(self, placement: Placement) -> bool {
        match self {
            Self::Instance => placement != Placement::Type,
            Self::Type | Self::Component => placement != Placement::Instance,
        }
    }

    fn rule(self, attribute: &Attribute) -> Rule {
        match (self, &attribute.presence, attribute.placement) {
            (Self::Component, _, _) | (Self::Instance, _, Placement::TypeOverridable) => {
                Rule::Optional
            }
            (_, Presence::Required, _) => Rule::Required,
            _ => default_value(attribute).map_or(Rule::Optional, Rule::Default),
        }
    }
}

/// What happens when a row has no value for an attribute.
#[derive(Clone, Debug)]
enum Rule {
    /// It is an error.
    Required,
    /// The attribute takes this value.
    Default(AttributeValue),
    /// The attribute stays empty.
    Optional,
}

impl Table {
    /// An empty table named `name` holding the attributes, or properties, set on `side`.
    pub(crate) fn new<'a>(
        name: &str,
        attributes: impl IntoIterator<Item = &'a Attribute>,
        side: Side,
    ) -> Self {
        Self {
            name: name.to_owned(),
            ids: Vec::new(),
            attributes: attributes
                .into_iter()
                .filter(|attribute| side.holds(attribute.placement))
                .map(|attribute| Slot {
                    name: attribute.name.clone(),
                    rule: side.rule(attribute),
                    column: Column::new(&attribute.value),
                })
                .collect(),
        }
    }

    /// The table's entity or property set.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// Appends a row with the given values, defaults filled in, and returns its number.
    ///
    /// Fails, adding nothing, when an attribute is unknown or given twice, a required one is
    /// missing, or a value does not fit its attribute.
    pub(crate) fn insert<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<usize, StoreError> {
        let row = self.complete_row(values)?;
        self.push(id, row)?;
        Ok(self.ids.len() - 1)
    }

    /// The value of `attribute` in `row`, if it has one.
    pub(crate) fn get(
        &self,
        row: usize,
        attribute: &str,
    ) -> Result<Option<AttributeValue>, StoreError> {
        Ok(self.attributes[self.position(attribute)?].column.get(row))
    }

    /// Sets, or with `None` clears, the value of `attribute` in `row`, and returns the previous
    /// value. Clearing an attribute with a default restores the default.
    pub(crate) fn set(
        &mut self,
        row: usize,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, StoreError> {
        let position = self.position(attribute)?;
        let slot = &mut self.attributes[position];
        let value = slot.or_default(value)?;
        slot.column
            .replace(row, value)
            .map_err(|error| slot.value_error(error))
    }

    /// Removes `row` by moving the last row into its place. Returns the values it had, in
    /// declaration order, and the id of the row moved into its place, if any.
    pub(crate) fn swap_remove(
        &mut self,
        row: usize,
    ) -> (Vec<(String, AttributeValue)>, Option<EntityId>) {
        self.ids.swap_remove(row);
        let values = self
            .attributes
            .iter_mut()
            .filter_map(|slot| {
                let value = slot.column.swap_remove(row)?;
                Some((slot.name.clone(), value))
            })
            .collect();
        (values, self.ids.get(row).copied())
    }

    /// Heap memory owned by the table.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.ids.capacity() * size_of::<EntityId>()
            + self.name.capacity()
            + self.attributes.capacity() * size_of::<Slot>()
            + self
                .attributes
                .iter()
                .map(|slot| slot.name.capacity() + slot.column.heap_bytes())
                .sum::<usize>()
    }

    /// One value or `None` per attribute, defaults filled in, in declaration order.
    fn complete_row<N: AsRef<str>>(
        &self,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<Vec<Option<AttributeValue>>, StoreError> {
        let mut row = vec![None; self.attributes.len()];
        for (name, value) in values {
            let name = name.as_ref();
            let position = self.position(name)?;
            if row[position].replace(value).is_some() {
                return Err(StoreError::DuplicateAttribute(name.to_owned()));
            }
        }
        row.into_iter()
            .zip(&self.attributes)
            .map(|(value, slot)| slot.or_default(value))
            .collect()
    }

    /// Appends a row, or nothing if a value does not fit its column.
    fn push(&mut self, id: EntityId, row: Vec<Option<AttributeValue>>) -> Result<(), StoreError> {
        for (pushed, value) in row.into_iter().enumerate() {
            let slot = &mut self.attributes[pushed];
            if let Err(error) = slot.column.push(value) {
                let error = slot.value_error(error);
                for slot in &mut self.attributes[..pushed] {
                    slot.column.pop();
                }
                return Err(error);
            }
        }
        self.ids.push(id);
        Ok(())
    }

    fn position(&self, attribute: &str) -> Result<usize, StoreError> {
        self.attributes
            .iter()
            .position(|slot| slot.name == attribute)
            .ok_or_else(|| StoreError::UnknownAttribute {
                entity: self.name.clone(),
                attribute: attribute.to_owned(),
            })
    }
}

impl Slot {
    /// The value to store when `value` is given, applying the attribute's default and
    /// requirement.
    fn or_default(
        &self,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, StoreError> {
        match (value, &self.rule) {
            (Some(value), _) => Ok(Some(value)),
            (None, Rule::Required) => Err(StoreError::MissingRequired(self.name.clone())),
            (None, Rule::Default(default)) => Ok(Some(default.clone())),
            (None, Rule::Optional) => Ok(None),
        }
    }

    fn value_error(&self, error: ValueError) -> StoreError {
        match error {
            ValueError::Mismatch => StoreError::TypeMismatch(self.name.clone()),
            ValueError::NonFinite => StoreError::NonFinite(self.name.clone()),
        }
    }
}

/// The value an attribute or property takes when nothing sets it.
pub(crate) fn default_value(attribute: &Attribute) -> Option<AttributeValue> {
    match &attribute.presence {
        Presence::Default(value) => Some(value.into()),
        Presence::Required | Presence::Optional => None,
    }
}
