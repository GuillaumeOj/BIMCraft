//! Materials in the model: the core schema's `Material` entity and its property sets.

use std::collections::BTreeMap;

use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::ModelCommand;
use thiserror::Error;

use crate::law::LawKind;
use crate::material::{Category, Material, MaterialError};
use crate::property::Property;
use crate::source::Sourced;

/// The core schema entity that stores materials.
pub const ENTITY: &str = "Material";

/// Why a material could not be read from the model. Messages never repeat values (SEC-03).
#[derive(Clone, Debug, Error, PartialEq)]
pub enum ReadError {
    /// No material has this id.
    #[error("the object is not a material")]
    NotAMaterial,
    /// The stored category is not one this version knows.
    #[error("the material's category is unknown")]
    UnknownCategory,
    /// The stored behaviour law is not one this version knows.
    #[error("the material's behaviour law is unknown")]
    UnknownLaw,
    /// The stored values do not make a valid material.
    #[error(transparent)]
    Invalid(#[from] MaterialError),
    /// The model rejected a read.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// The command that stores `material` as a new object `id`: the entity, then one property set
/// per group of properties it gives, each value with its source. Run it through a
/// `core_txn::Document` to make it undoable.
#[must_use]
pub fn commands(id: EntityId, material: &Material) -> ModelCommand {
    let mut values = vec![
        text("name", material.name()),
        text("category", material.category().name()),
        text("behaviour_law", material.law().kind().name()),
    ];
    if let Some(code) = material.design_code() {
        values.push(text("design_code", code));
    }
    let mut commands = vec![ModelCommand::Create {
        id,
        entity: ENTITY.to_owned(),
        values,
    }];
    let mut sets: BTreeMap<&str, Vec<(String, AttributeValue)>> = BTreeMap::new();
    for (property, sourced) in material.properties() {
        let value =
            AttributeValue::sourced(AttributeValue::Real(sourced.value), sourced.source.clone());
        sets.entry(property.property_set())
            .or_default()
            .push((property.name().to_owned(), value));
    }
    commands.extend(
        sets.into_iter()
            .map(|(set, values)| ModelCommand::AttachPropertySet {
                id,
                set: set.to_owned(),
                values,
            }),
    );
    ModelCommand::Batch { commands }
}

/// Reads material `id` back from the model.
///
/// # Errors
///
/// Fails when the object is not a material, or its stored values are unknown, unsourced or
/// invalid.
pub fn read(model: &Model, id: EntityId) -> Result<Material, ReadError> {
    if model.entity_of(id) != Some(ENTITY) {
        return Err(ReadError::NotAMaterial);
    }
    let name = attribute(model, id, "name")?.unwrap_or_default();
    let category = attribute(model, id, "category")?
        .and_then(|name| Category::from_name(&name))
        .ok_or(ReadError::UnknownCategory)?;
    let law = attribute(model, id, "behaviour_law")?
        .and_then(|name| LawKind::from_name(&name))
        .ok_or(ReadError::UnknownLaw)?;
    let design_code = attribute(model, id, "design_code")?;
    let mut properties = BTreeMap::new();
    for property in Property::ALL {
        // The model keeps every value of these sourced properties with its source (ADR 0018).
        let Some(AttributeValue::Sourced(sourced)) =
            model.property(id, property.property_set(), property.name())?
        else {
            continue;
        };
        if let AttributeValue::Real(value) = sourced.value {
            properties.insert(property, Sourced::new(value, sourced.source));
        }
    }
    Ok(Material::new(
        &name,
        category,
        law,
        design_code.as_deref(),
        properties,
    )?)
}

fn text(name: &str, value: &str) -> (String, AttributeValue) {
    (name.to_owned(), AttributeValue::Text(value.to_owned()))
}

fn attribute(model: &Model, id: EntityId, name: &str) -> Result<Option<String>, ModelError> {
    Ok(model
        .get(id, name)?
        .as_ref()
        .and_then(AttributeValue::as_text)
        .map(str::to_owned))
}

#[cfg(test)]
mod tests;
