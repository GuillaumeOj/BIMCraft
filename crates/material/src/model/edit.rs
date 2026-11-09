//! Editing a stored material's values: a value the user enters replaces the library's, which a
//! `MaterialOriginal` object keeps, so resetting the value restores it (MAT-02, ADR 0019).

use std::collections::{BTreeMap, BTreeSet};

use core_model::{AttributeValue, EntityId, Model, ModelError, Source};
use core_txn::ModelCommand;
use thiserror::Error;

use super::{ENTITY, ReadError, read, sourced_real, sourced_value};
use crate::material::MaterialError;
use crate::property::Property;
use crate::source::Sourced;

/// The core schema entity that keeps the library value of an edited property.
pub const ORIGINAL_ENTITY: &str = "MaterialOriginal";

/// Why a material could not be edited. Messages never repeat values (SEC-03).
#[derive(Clone, Debug, Error, PartialEq)]
pub enum EditError {
    /// The stored material does not read back.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The edited values do not make a valid material.
    #[error(transparent)]
    Invalid(#[from] MaterialError),
    /// The same property is edited twice in one change.
    #[error("the property {} is edited more than once", .0.name())]
    Repeated(Property),
    /// The property keeps no library value to restore.
    #[error("the property {} has no library value to restore", .0.name())]
    NothingToReset(Property),
    /// The model rejected a read.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// A stored library value of an edited property.
#[derive(Clone, Debug, PartialEq)]
pub struct Original {
    /// The `MaterialOriginal` object holding it.
    pub id: EntityId,
    /// The edited property.
    pub property: Property,
    /// The library value with its source.
    pub value: Sourced<f64>,
}

/// Every `MaterialOriginal` object of material `id`, well formed or not.
///
/// # Errors
///
/// Fails when the model rejects a read.
pub fn original_ids(model: &Model, id: EntityId) -> Result<Vec<EntityId>, ModelError> {
    let mut found = Vec::new();
    for object in model.store().ids() {
        if model.entity_of(object) == Some(ORIGINAL_ENTITY)
            && model.get(object, "material")? == Some(AttributeValue::Ref(id))
        {
            found.push(object);
        }
    }
    Ok(found)
}

/// The library values kept for the edited properties of material `id`, in [`Property`] order.
///
/// # Errors
///
/// Fails when the model rejects a read; objects that are not well formed are skipped.
pub fn originals(model: &Model, id: EntityId) -> Result<Vec<Original>, ModelError> {
    let mut originals = Vec::new();
    for object in original_ids(model, id)? {
        let key = model.get(object, "key")?;
        let property = key
            .as_ref()
            .and_then(AttributeValue::as_text)
            .and_then(Property::from_name);
        if let (Some(property), Some(value)) = (property, sourced_real(model.get(object, "value")?))
        {
            originals.push(Original {
                id: object,
                property,
                value,
            });
        }
    }
    originals.sort_by_key(|original| original.property);
    Ok(originals)
}

/// One change to a property of a stored material.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    /// Enter this value, which then has the source "Entered by user".
    Set(f64),
    /// Restore the library value and source; for a value the user entered that the library never
    /// gave, clear it; a value still as the library gave it stays.
    Reset,
}

/// The command that applies `edits` to material `id`, one change per property, as one batch.
/// The values change together, so they are checked together: `fy` and `fu` can both be raised.
/// The first time a value cited from a document is replaced, a new `MaterialOriginal` object
/// `original_id(property)` keeps it.
///
/// # Errors
///
/// Fails when `id` is not a valid material, a property is edited twice, or the edited values do
/// not make a valid material.
pub fn edit(
    model: &Model,
    id: EntityId,
    edits: &[(Property, Edit)],
    original_id: impl Fn(Property) -> EntityId,
) -> Result<ModelCommand, EditError> {
    let material = read(model, id)?;
    let kept = originals(model, id)?;
    let mut properties: BTreeMap<Property, Sourced<f64>> = material
        .properties()
        .map(|(property, value)| (property, value.clone()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut commands = Vec::new();
    for &(property, change) in edits {
        if !seen.insert(property) {
            return Err(EditError::Repeated(property));
        }
        let original = kept.iter().find(|original| original.property == property);
        match (change, original) {
            (Edit::Set(value), _) => {
                let entered = Sourced::new(value, Source::user());
                if let (Some(current), None) = (properties.get(&property), original)
                    && !current.source.is_user()
                {
                    commands.push(keep_original(id, property, current, original_id(property)));
                }
                commands.push(set(id, property, Some(&entered)));
                properties.insert(property, entered);
            }
            (Edit::Reset, Some(original)) => {
                commands.push(set(id, property, Some(&original.value)));
                commands.push(ModelCommand::Delete { id: original.id });
                properties.insert(property, original.value.clone());
            }
            (Edit::Reset, None) => {
                if properties
                    .get(&property)
                    .is_some_and(|value| value.source.is_user())
                {
                    commands.push(set(id, property, None));
                    properties.remove(&property);
                }
            }
        }
    }
    material.with_properties(properties)?;
    Ok(ModelCommand::Batch { commands })
}

/// The command that sets `property` of material `id` to `value`, entered by the user. The first
/// time a value cited from a document is replaced, a new `MaterialOriginal` object `original`
/// keeps it; `original` is unused otherwise.
///
/// # Errors
///
/// Fails when `id` is not a valid material, or the new value does not make one.
pub fn set_value(
    model: &Model,
    id: EntityId,
    property: Property,
    value: f64,
    original: EntityId,
) -> Result<ModelCommand, EditError> {
    edit(model, id, &[(property, Edit::Set(value))], |_| original)
}

/// The command that restores the library value and source of `property` of material `id`,
/// and deletes the `MaterialOriginal` object that kept it.
///
/// # Errors
///
/// Fails when `id` is not a valid material, the property was never edited, or the restored
/// value no longer fits the material's other values.
pub fn reset_value(
    model: &Model,
    id: EntityId,
    property: Property,
) -> Result<ModelCommand, EditError> {
    read(model, id)?;
    if !originals(model, id)?
        .iter()
        .any(|original| original.property == property)
    {
        return Err(EditError::NothingToReset(property));
    }
    // A reset never keeps a new original, so no id is asked for.
    edit(model, id, &[(property, Edit::Reset)], |_| id)
}

/// The command that deletes material `id` with the library values kept for it.
///
/// # Errors
///
/// Fails when `id` is not a material, or the model rejects a read.
pub fn delete(model: &Model, id: EntityId) -> Result<ModelCommand, EditError> {
    if model.entity_of(id) != Some(ENTITY) {
        return Err(ReadError::NotAMaterial.into());
    }
    let commands = original_ids(model, id)?
        .into_iter()
        .chain([id])
        .map(|object| ModelCommand::Delete { id: object })
        .collect();
    Ok(ModelCommand::Batch { commands })
}

fn set(id: EntityId, property: Property, value: Option<&Sourced<f64>>) -> ModelCommand {
    ModelCommand::Set {
        id,
        attribute: property.name().to_owned(),
        value: value.map(sourced_value),
    }
}

/// The command that creates the `MaterialOriginal` `original` keeping `value` of `property`.
fn keep_original(
    material: EntityId,
    property: Property,
    value: &Sourced<f64>,
    original: EntityId,
) -> ModelCommand {
    ModelCommand::Create {
        id: original,
        entity: ORIGINAL_ENTITY.to_owned(),
        values: vec![
            ("material".to_owned(), AttributeValue::Ref(material)),
            (
                "key".to_owned(),
                AttributeValue::Text(property.name().to_owned()),
            ),
            ("value".to_owned(), sourced_value(value)),
        ],
    }
}

#[cfg(test)]
mod tests;
