//! Editing a stored material's values: a value the user enters replaces the library's, which a
//! `MaterialOriginal` object keeps, so resetting the value restores it (MAT-02, ADR 0019).

use core_model::{AttributeValue, EntityId, Model, ModelError, Source};
use core_txn::ModelCommand;
use thiserror::Error;

use super::{ENTITY, ReadError, read};
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

/// The library values kept for the edited properties of material `id`, in [`Property`] order.
///
/// # Errors
///
/// Fails when the model rejects a read; objects that are not well formed are skipped.
pub fn originals(model: &Model, id: EntityId) -> Result<Vec<Original>, ModelError> {
    let mut originals = Vec::new();
    for object in model.store().ids() {
        if model.entity_of(object) != Some(ORIGINAL_ENTITY)
            || model.get(object, "material")? != Some(AttributeValue::Ref(id))
        {
            continue;
        }
        let key = model.get(object, "key")?;
        let property = key
            .as_ref()
            .and_then(AttributeValue::as_text)
            .and_then(Property::from_name);
        if let (Some(property), Some(AttributeValue::Sourced(sourced))) =
            (property, model.get(object, "value")?)
            && let AttributeValue::Real(value) = sourced.value
        {
            originals.push(Original {
                id: object,
                property,
                value: Sourced::new(value, sourced.source),
            });
        }
    }
    originals.sort_by_key(|original| original.property);
    Ok(originals)
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
    let material = read(model, id)?;
    let entered = Sourced::new(value, Source::user());
    material.with_property(property, Some(entered.clone()))?;
    let mut commands = Vec::new();
    let kept = originals(model, id)?
        .iter()
        .any(|kept| kept.property == property);
    if let Some(current) = material.property(property)
        && !current.source.is_user()
        && !kept
    {
        commands.push(ModelCommand::Create {
            id: original,
            entity: ORIGINAL_ENTITY.to_owned(),
            values: vec![
                ("material".to_owned(), AttributeValue::Ref(id)),
                (
                    "key".to_owned(),
                    AttributeValue::Text(property.name().to_owned()),
                ),
                ("value".to_owned(), sourced_value(current)),
            ],
        });
    }
    commands.push(set(id, property, &entered));
    Ok(ModelCommand::Batch { commands })
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
    let material = read(model, id)?;
    let original = originals(model, id)?
        .into_iter()
        .find(|original| original.property == property)
        .ok_or(EditError::NothingToReset(property))?;
    material.with_property(property, Some(original.value.clone()))?;
    Ok(ModelCommand::Batch {
        commands: vec![
            set(id, property, &original.value),
            ModelCommand::Delete { id: original.id },
        ],
    })
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
    let commands = originals(model, id)?
        .into_iter()
        .map(|original| ModelCommand::Delete { id: original.id })
        .chain([ModelCommand::Delete { id }])
        .collect();
    Ok(ModelCommand::Batch { commands })
}

pub(super) fn sourced_value(value: &Sourced<f64>) -> AttributeValue {
    AttributeValue::sourced(AttributeValue::Real(value.value), value.source.clone())
}

fn set(id: EntityId, property: Property, value: &Sourced<f64>) -> ModelCommand {
    ModelCommand::Set {
        id,
        attribute: property.name().to_owned(),
        value: Some(sourced_value(value)),
    }
}

#[cfg(test)]
mod tests;
