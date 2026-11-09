//! Materials in the model: the core schema's `Material` entity, whose property values are
//! sourced attributes, so they are saved in the project file with their sources (ADR 0019).

use std::collections::BTreeMap;

use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::ModelCommand;
use thiserror::Error;

use crate::law::LawKind;
use crate::material::{Category, Material, MaterialError};
use crate::property::Property;
use crate::source::Sourced;

mod edit;
mod layer_sets;

pub use edit::{
    Edit, EditError, ORIGINAL_ENTITY, Original, check_name, delete, edit, original_id,
    original_ids, originals, rename, reset_value, set_value, used_by,
};
pub use layer_sets::{
    LAYER_ENTITY, LAYER_SET_ENTITY, LayerSetReadError, check_layers, conductivity,
    delete_layer_set, fresh_layer_ids, layer_ids, layer_set_commands, read_layer_set,
    replace_layer_set,
};

/// The core schema entity that stores materials.
pub const ENTITY: &str = "Material";

/// An id derived from `seed` and `tag`, so a replayed command makes the same object (CORE-11):
/// FNV-1a over 128 bits, no randomness.
pub(crate) fn derived_id(seed: EntityId, tag: &[u8], extra: &[u8]) -> EntityId {
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;
    let mut state = seed.get() ^ 0x6C62_272E_07BB_0142_62B8_2175_6295_C58D;
    for byte in tag.iter().chain(extra) {
        state = (state ^ u128::from(*byte)).wrapping_mul(PRIME);
    }
    EntityId::from_random_bits(state)
}

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

/// The command that stores `material` as a new object `id`: one object holding its name,
/// category, law, design code and every property value with its source. Run it through a
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
    if let Some(key) = material.library_key() {
        values.push(text("library_key", key));
    }
    values.extend(
        material
            .properties()
            .map(|(property, value)| (property.name().to_owned(), sourced_value(value))),
    );
    ModelCommand::Create {
        id,
        entity: ENTITY.to_owned(),
        values,
    }
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
    let library_key = attribute(model, id, "library_key")?;
    let mut properties = BTreeMap::new();
    // Every property, so that a stored value outside the category is refused, not hidden.
    for property in Property::ALL {
        if let Some(value) = sourced_real(model.get(id, property.name())?) {
            properties.insert(property, value);
        }
    }
    let material = Material::new(&name, category, law, design_code.as_deref(), properties)?;
    Ok(material.with_library_key(library_key.as_deref().unwrap_or_default())?)
}

fn text(name: &str, value: &str) -> (String, AttributeValue) {
    (name.to_owned(), AttributeValue::Text(value.to_owned()))
}

/// Whether another object of `entity` than `except` already has `name`, ignoring case and
/// surrounding spaces: materials and build-ups are unique by name within their entity.
fn name_in_use(
    model: &Model,
    entity: &str,
    name: &str,
    except: Option<EntityId>,
) -> Result<bool, ModelError> {
    let lowered = name.trim().to_lowercase();
    for id in model.store().ids() {
        if model.entity_of(id) == Some(entity)
            && Some(id) != except
            && attribute(model, id, "name")?.is_some_and(|other| other.to_lowercase() == lowered)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn attribute(model: &Model, id: EntityId, name: &str) -> Result<Option<String>, ModelError> {
    Ok(match model.get(id, name)? {
        Some(AttributeValue::Text(text)) => Some(text),
        _ => None,
    })
}

/// A material property as a sourced attribute value.
fn sourced_value(value: &Sourced<f64>) -> AttributeValue {
    AttributeValue::sourced(AttributeValue::Real(value.value), value.source.clone())
}

/// A sourced attribute value read back as a material property; `None` for anything else.
fn sourced_real(value: Option<AttributeValue>) -> Option<Sourced<f64>> {
    let Some(AttributeValue::Sourced(sourced)) = value else {
        return None;
    };
    let AttributeValue::Real(real) = sourced.value else {
        return None;
    };
    Some(Sourced::new(real, sourced.source))
}

#[cfg(test)]
mod tests;
