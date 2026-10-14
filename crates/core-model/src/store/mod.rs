//! Typed attribute tables holding every stored object (CORE-05, ADR 0007).

mod column;
mod value;

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use schema::{Namespace, Placement, Presence, ValidationError};

use self::column::Column;
pub use self::value::AttributeValue;
use self::value::ValueError;
use crate::EntityId;

/// Every object of a model, one table per entity of the schema and one typed column per
/// attribute, addressed by [`EntityId`] (CORE-05).
///
/// Tables are dense: deleting an object moves the last row of its table into its place. A table
/// holds the attributes set on instances, `instance` and `type_overridable` ones (CORE-02);
/// `type` attributes belong to the type object.
///
/// The store checks values against the schema but not references: a `ref:` value may name an
/// object that does not exist (yet), and keeping references consistent is the job of
/// relationships and transactions.
#[derive(Clone, Debug)]
pub struct Store {
    /// Sorted by entity name, as the namespace lists them.
    tables: Vec<Table>,
    index: HashMap<EntityId, Location>,
}

#[derive(Clone, Debug)]
struct Table {
    entity: String,
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

/// What happens when an object has no value for an attribute.
#[derive(Clone, Debug)]
enum Rule {
    /// It is an error.
    Required,
    /// The attribute takes this value.
    Default(AttributeValue),
    /// The attribute stays empty.
    Optional,
}

#[derive(Clone, Copy, Debug)]
struct Location {
    table: usize,
    row: usize,
}

/// An object taken out of a [`Store`]; [`Store::insert`] puts it back unchanged.
#[derive(Clone, Debug, PartialEq)]
pub struct RemovedEntity {
    /// The object's entity.
    pub entity: String,
    /// The attributes that had a value, in declaration order.
    pub values: Vec<(String, AttributeValue)>,
}

/// Why a [`Store`] operation failed. Nothing was changed.
///
/// The messages name entities and attributes from the schema but never repeat values or ids,
/// which come from a user's model (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum StoreError {
    /// The namespace does not pass [`schema::validate`].
    #[error("the schema does not pass validation")]
    InvalidSchema(Vec<ValidationError>),
    /// The schema has no such entity.
    #[error("unknown entity \"{0}\"")]
    UnknownEntity(String),
    /// The entity has no such attribute on its instances.
    #[error("entity \"{entity}\" has no instance attribute \"{attribute}\"")]
    UnknownAttribute {
        /// The entity.
        entity: String,
        /// The unknown attribute.
        attribute: String,
    },
    /// An object with this id is already stored.
    #[error("an object with this id already exists")]
    DuplicateId(EntityId),
    /// No object with this id is stored.
    #[error("no object with this id")]
    NotFound(EntityId),
    /// The same attribute is given twice.
    #[error("attribute \"{0}\" is given more than once")]
    DuplicateAttribute(String),
    /// A required attribute has no value.
    #[error("attribute \"{0}\" is required")]
    MissingRequired(String),
    /// A value does not have the attribute's value type.
    #[error("attribute \"{0}\" does not accept a value of this type")]
    TypeMismatch(String),
    /// A real value is NaN or infinite.
    #[error("attribute \"{0}\" only accepts finite numbers")]
    NonFinite(String),
}

impl Store {
    /// Creates an empty store with one table per entity of `namespace`.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::InvalidSchema`] when the namespace does not pass
    /// [`schema::validate`].
    pub fn new(namespace: &Namespace) -> Result<Self, StoreError> {
        schema::validate(namespace).map_err(StoreError::InvalidSchema)?;
        let tables = namespace
            .entities
            .values()
            .map(|entity| Table {
                entity: entity.name.clone(),
                ids: Vec::new(),
                attributes: entity
                    .attributes
                    .iter()
                    .filter(|attribute| attribute.placement != Placement::Type)
                    .map(|attribute| Slot {
                        name: attribute.name.clone(),
                        // On instances, a type_overridable attribute is an optional override:
                        // the type provides its value and default.
                        rule: match (&attribute.presence, attribute.placement) {
                            (Presence::Required, Placement::Instance) => Rule::Required,
                            (Presence::Default(value), Placement::Instance) => {
                                Rule::Default(value.into())
                            }
                            _ => Rule::Optional,
                        },
                        column: Column::new(&attribute.value),
                    })
                    .collect(),
            })
            .collect();
        Ok(Self {
            tables,
            index: HashMap::new(),
        })
    }

    /// Number of stored objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether no object is stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Whether an object with this id is stored.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.index.contains_key(&id)
    }

    /// The entity of the object with this id.
    #[must_use]
    pub fn entity_of(&self, id: EntityId) -> Option<&str> {
        let location = self.index.get(&id)?;
        Some(&self.tables[location.table].entity)
    }

    /// Stores a new object of `entity` with the given attribute values.
    ///
    /// Attributes without a value take their default, or stay empty when optional.
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the id is taken, the entity or an attribute is unknown, an
    /// attribute is given twice, a required one is missing, or a value does not fit its
    /// attribute.
    pub fn insert<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        entity: &str,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), StoreError> {
        let Entry::Vacant(entry) = self.index.entry(id) else {
            return Err(StoreError::DuplicateId(id));
        };
        let table_index = self
            .tables
            .binary_search_by(|table| table.entity.as_str().cmp(entity))
            .map_err(|_| StoreError::UnknownEntity(entity.to_owned()))?;
        let table = &mut self.tables[table_index];
        let row = table.complete_row(values)?;
        table.push(id, row)?;
        entry.insert(Location {
            table: table_index,
            row: table.ids.len() - 1,
        });
        Ok(())
    }

    /// The value of `attribute` on the object with this id, if it has one.
    ///
    /// # Errors
    ///
    /// Fails when no object has this id or its entity has no such instance attribute.
    pub fn get(&self, id: EntityId, attribute: &str) -> Result<Option<AttributeValue>, StoreError> {
        let location = self.locate(id)?;
        let table = &self.tables[location.table];
        Ok(table.slot(attribute)?.column.get(location.row))
    }

    /// Sets, or with `None` clears, the value of `attribute` on the object with this id, and
    /// returns the previous value. Clearing an attribute with a default restores the default.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when no object has this id, its entity has no such instance
    /// attribute, a required attribute would be cleared, or the value does not fit.
    pub fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, StoreError> {
        let location = self.locate(id)?;
        let slot = self.tables[location.table].slot_mut(attribute)?;
        let value = slot.or_default(value)?;
        slot.column
            .replace(location.row, value)
            .map_err(|error| slot.value_error(error))
    }

    /// Removes the object with this id and returns it.
    ///
    /// # Errors
    ///
    /// Fails when no object has this id.
    pub fn remove(&mut self, id: EntityId) -> Result<RemovedEntity, StoreError> {
        let location = self.index.remove(&id).ok_or(StoreError::NotFound(id))?;
        let table = &mut self.tables[location.table];
        table.ids.swap_remove(location.row);
        let values = table
            .attributes
            .iter_mut()
            .filter_map(|slot| {
                let value = slot.column.swap_remove(location.row)?;
                Some((slot.name.clone(), value))
            })
            .collect();
        if let Some(&moved) = table.ids.get(location.row) {
            self.index.insert(moved, location);
        }
        Ok(RemovedEntity {
            entity: table.entity.clone(),
            values,
        })
    }

    /// An estimate of the heap memory the store holds, in bytes (NFR-08).
    ///
    /// Columns and id lists count their allocated capacity and the text and lists they own. The
    /// index is estimated from the layout of the standard hash map: one bucket per entry slot,
    /// a power of two with at least an eighth of the buckets free, plus one control byte per
    /// bucket and one group of trailing control bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let tables: usize = self
            .tables
            .iter()
            .map(|table| {
                table.ids.capacity() * size_of::<EntityId>()
                    + table.entity.capacity()
                    + table.attributes.capacity() * size_of::<Slot>()
                    + table
                        .attributes
                        .iter()
                        .map(|slot| slot.name.capacity() + slot.column.heap_bytes())
                        .sum::<usize>()
            })
            .sum();
        self.tables.capacity() * size_of::<Table>() + tables + index_heap_bytes(&self.index)
    }

    fn locate(&self, id: EntityId) -> Result<Location, StoreError> {
        self.index.get(&id).copied().ok_or(StoreError::NotFound(id))
    }
}

/// Heap bytes of a standard hash map; see [`Store::heap_bytes`].
fn index_heap_bytes(index: &HashMap<EntityId, Location>) -> usize {
    /// Control bytes the map adds after its buckets, the width of one SIMD group.
    const GROUP_WIDTH: usize = 16;
    if index.capacity() == 0 {
        return 0;
    }
    let buckets = (index.capacity() * 8 / 7).next_power_of_two();
    buckets * (size_of::<(EntityId, Location)>() + 1) + GROUP_WIDTH
}

impl Table {
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
                entity: self.entity.clone(),
                attribute: attribute.to_owned(),
            })
    }

    fn slot(&self, attribute: &str) -> Result<&Slot, StoreError> {
        Ok(&self.attributes[self.position(attribute)?])
    }

    fn slot_mut(&mut self, attribute: &str) -> Result<&mut Slot, StoreError> {
        let position = self.position(attribute)?;
        Ok(&mut self.attributes[position])
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

#[cfg(test)]
mod tests;
