//! Typed attribute tables holding every stored object (CORE-05, ADR 0007).

mod column;
mod table;
mod value;

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use schema::{Namespace, ValidationError};

pub(crate) use self::table::{Table, default_value};
pub use self::value::AttributeValue;
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
            .map(|entity| Table::instances(&entity.name, &entity.attributes))
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
        Some(self.tables[location.table].name())
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
            .binary_search_by(|table| table.name().cmp(entity))
            .map_err(|_| StoreError::UnknownEntity(entity.to_owned()))?;
        let row = self.tables[table_index].insert(id, values)?;
        entry.insert(Location {
            table: table_index,
            row,
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
        self.tables[location.table].get(location.row, attribute)
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
        self.tables[location.table].set(location.row, attribute, value)
    }

    /// Removes the object with this id and returns it.
    ///
    /// # Errors
    ///
    /// Fails when no object has this id.
    pub fn remove(&mut self, id: EntityId) -> Result<RemovedEntity, StoreError> {
        let location = self.index.remove(&id).ok_or(StoreError::NotFound(id))?;
        let table = &mut self.tables[location.table];
        let (values, moved) = table.swap_remove(location.row);
        if let Some(moved) = moved {
            self.index.insert(moved, location);
        }
        Ok(RemovedEntity {
            entity: table.name().to_owned(),
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
        let tables: usize = self.tables.iter().map(Table::heap_bytes).sum();
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

#[cfg(test)]
mod tests;
