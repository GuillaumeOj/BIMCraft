//! Materials in the model: the core schema's `Material` entity and its property sets.

use std::collections::BTreeMap;

use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::ModelCommand;
use thiserror::Error;

use crate::law::LawKind;
use crate::material::{Category, Material, MaterialError};
use crate::property::Property;
use crate::source::{Source, SourceError, Sourced};

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
    /// A property has a source but no value.
    #[error("the property {} has a source but no value", .0.name())]
    SourceWithoutValue(Property),
    /// A property has a value but no source.
    #[error("the property {} has no source", .0.name())]
    MissingSource(Property),
    /// A property's source does not read back.
    #[error("the source of property {} is not valid: {}", .0.name(), .1)]
    InvalidSource(Property, SourceError),
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
        sets.entry(property.property_set()).or_default().extend([
            (
                property.name().to_owned(),
                AttributeValue::Real(sourced.value),
            ),
            text(property.source_name(), &sourced.source.to_string()),
        ]);
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
        let set = property.property_set();
        let value = model.property(id, set, property.name())?;
        let source = model.property(id, set, property.source_name())?;
        let (value, source) = match (value, source) {
            (Some(AttributeValue::Real(value)), Some(AttributeValue::Text(source))) => {
                (value, source)
            }
            (Some(_), _) => return Err(ReadError::MissingSource(property)),
            (None, Some(_)) => return Err(ReadError::SourceWithoutValue(property)),
            (None, None) => continue,
        };
        let source =
            Source::parse(&source).map_err(|error| ReadError::InvalidSource(property, error))?;
        properties.insert(property, Sourced::new(value, source));
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
    Ok(match model.get(id, name)? {
        Some(AttributeValue::Text(text)) => Some(text),
        _ => None,
    })
}

#[cfg(test)]
mod tests;
