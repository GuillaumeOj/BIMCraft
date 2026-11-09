//! Build-ups in the model: the core schema's `MaterialLayerSet` and `MaterialLayer` entities.
//!
//! A build-up refers to its layers through a required list, and each layer to its material, so
//! the generic delete refuses to remove an object that is still referred to (CORE-03):
//! [`delete_layer_set`] removes the build-up first, then its layers, in one command.

use std::collections::BTreeSet;

use core_model::units::Length;
use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::ModelCommand;
use thiserror::Error;

use super::{ENTITY as MATERIAL, attribute};
use crate::layers::{Layer, LayerFunction, LayerSet, LayerSetError, Usage};

/// The core schema entity that stores build-ups.
pub const LAYER_SET_ENTITY: &str = "MaterialLayerSet";
/// The core schema entity that stores the layers of build-ups.
pub const LAYER_ENTITY: &str = "MaterialLayer";

/// Why a build-up could not be read from the model. Messages never repeat values (SEC-03).
#[derive(Clone, Debug, Error, PartialEq)]
pub enum LayerSetReadError {
    /// No build-up has this id.
    #[error("the object is not a build-up")]
    NotALayerSet,
    /// A layer of the build-up is not a layer object.
    #[error("item {0} of the build-up is not a layer")]
    NotALayer(usize),
    /// A layer's material is not a material object.
    #[error("the material of layer {0} is not a material")]
    NotAMaterial(usize),
    /// The stored usage or function is not one this version knows.
    #[error("the build-up holds a value this version does not know")]
    Unknown,
    /// The stored values do not make a valid build-up.
    #[error(transparent)]
    Invalid(#[from] LayerSetError),
    /// The model rejected a read.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// The command that stores `set` as a new build-up object `id`, with a new layer object for each
/// of its layers under the layer's id. Run it through a `core_txn::Document` to make it
/// undoable.
#[must_use]
pub fn layer_set_commands(id: EntityId, set: &LayerSet) -> ModelCommand {
    let mut commands: Vec<ModelCommand> = set.layers().iter().map(create_layer).collect();
    let mut values = vec![
        value("name", AttributeValue::Text(set.name().to_owned())),
        value("usage", AttributeValue::Text(set.usage().name().to_owned())),
        value("layers", layer_refs(set)),
    ];
    if let Some(description) = set.description() {
        values.push(value(
            "description",
            AttributeValue::Text(description.to_owned()),
        ));
    }
    commands.push(ModelCommand::Create {
        id,
        entity: LAYER_SET_ENTITY.to_owned(),
        values,
    });
    ModelCommand::Batch { commands }
}

/// The command that makes build-up `id` equal to `set`: layers whose id is already in the
/// build-up are updated in place and keep their identity (ADR 0010), the others are created,
/// and the build-up's layers that `set` leaves out are deleted, all in one command.
///
/// # Errors
///
/// Fails when `id` is not a build-up, or the model rejects a read.
pub fn replace_layer_set(
    model: &Model,
    id: EntityId,
    set: &LayerSet,
) -> Result<ModelCommand, LayerSetReadError> {
    let before = layer_ids(model, id)?;
    let keep: BTreeSet<EntityId> = set.layers().iter().map(|layer| layer.id).collect();
    let mut commands = Vec::new();
    for layer in set.layers() {
        commands.push(if before.contains(&layer.id) {
            update_layer(layer)
        } else {
            create_layer(layer)
        });
    }
    let mut values = vec![
        ("name", Some(AttributeValue::Text(set.name().to_owned()))),
        (
            "usage",
            Some(AttributeValue::Text(set.usage().name().to_owned())),
        ),
        (
            "description",
            set.description()
                .map(|text| AttributeValue::Text(text.to_owned())),
        ),
        ("layers", Some(layer_refs(set))),
    ];
    commands.extend(
        values
            .drain(..)
            .map(|(attribute, value)| ModelCommand::Set {
                id,
                attribute: attribute.to_owned(),
                value,
            }),
    );
    // The build-up no longer lists them, so nothing refers to them any more.
    commands.extend(
        before
            .into_iter()
            .filter(|layer| !keep.contains(layer))
            .map(|layer| ModelCommand::Delete { id: layer }),
    );
    Ok(ModelCommand::Batch { commands })
}

/// The command that deletes build-up `id` and its layers in one step: the build-up first, so
/// that nothing refers to the layers when they go.
///
/// # Errors
///
/// Fails when `id` is not a build-up, or the model rejects a read.
pub fn delete_layer_set(model: &Model, id: EntityId) -> Result<ModelCommand, LayerSetReadError> {
    let commands = std::iter::once(id)
        .chain(layer_ids(model, id)?)
        .map(|object| ModelCommand::Delete { id: object })
        .collect();
    Ok(ModelCommand::Batch { commands })
}

/// The ids of the layers build-up `id` lists, in order, whatever they hold.
///
/// # Errors
///
/// Fails when `id` is not a build-up, or the model rejects a read.
pub fn layer_ids(model: &Model, id: EntityId) -> Result<Vec<EntityId>, LayerSetReadError> {
    if model.entity_of(id) != Some(LAYER_SET_ENTITY) {
        return Err(LayerSetReadError::NotALayerSet);
    }
    Ok(match model.get(id, "layers")? {
        Some(AttributeValue::List(items)) => {
            items.iter().filter_map(AttributeValue::as_entity).collect()
        }
        _ => Vec::new(),
    })
}

/// Reads build-up `id` back from the model.
///
/// # Errors
///
/// Fails when the object is not a build-up, a layer or its material is missing, or the stored
/// values do not make a valid build-up.
pub fn read_layer_set(model: &Model, id: EntityId) -> Result<LayerSet, LayerSetReadError> {
    if model.entity_of(id) != Some(LAYER_SET_ENTITY) {
        return Err(LayerSetReadError::NotALayerSet);
    }
    let name = attribute(model, id, "name")?.unwrap_or_default();
    let usage = attribute(model, id, "usage")?
        .and_then(|stored| Usage::from_name(&stored))
        .ok_or(LayerSetReadError::Unknown)?;
    let items = match model.get(id, "layers")? {
        Some(AttributeValue::List(items)) => items,
        _ => Vec::new(),
    };
    let mut layers = Vec::with_capacity(items.len());
    for (number, item) in (1..).zip(items) {
        let layer = item
            .as_entity()
            .ok_or(LayerSetReadError::NotALayer(number))?;
        layers.push(read_layer(model, layer, number)?);
    }
    let description = attribute(model, id, "description")?;
    Ok(LayerSet::new(&name, usage, description.as_deref(), layers)?)
}

fn read_layer(model: &Model, id: EntityId, number: usize) -> Result<Layer, LayerSetReadError> {
    if model.entity_of(id) != Some(LAYER_ENTITY) {
        return Err(LayerSetReadError::NotALayer(number));
    }
    let material = model
        .get(id, "material")?
        .and_then(|value| value.as_entity())
        .filter(|&material| model.entity_of(material) == Some(MATERIAL))
        .ok_or(LayerSetReadError::NotAMaterial(number))?;
    let function = attribute(model, id, "function")?
        .and_then(|stored| LayerFunction::from_name(&stored))
        .ok_or(LayerSetReadError::Unknown)?;
    // Required values are always there; a missing one reads as invalid, never as a default.
    let thickness = model
        .get(id, "thickness")?
        .and_then(|value| value.as_real());
    let priority = match model.get(id, "priority")? {
        Some(AttributeValue::Integer(priority)) => u8::try_from(priority).unwrap_or(0),
        _ => 0,
    };
    Ok(Layer {
        id,
        material,
        thickness: Length::from_si(thickness.unwrap_or(f64::NAN)),
        function,
        priority,
        core: model.get(id, "is_core")? == Some(AttributeValue::Bool(true)),
        name: attribute(model, id, "name")?,
    })
}

/// The layer attributes, each with the value to set (`None` clears it).
fn layer_values(layer: &Layer) -> [(&'static str, Option<AttributeValue>); 6] {
    [
        ("material", Some(AttributeValue::Ref(layer.material))),
        (
            "thickness",
            Some(AttributeValue::Real(layer.thickness.si())),
        ),
        (
            "function",
            Some(AttributeValue::Text(layer.function.name().to_owned())),
        ),
        (
            "priority",
            Some(AttributeValue::Integer(i64::from(layer.priority))),
        ),
        ("is_core", Some(AttributeValue::Bool(layer.core))),
        ("name", layer.name.clone().map(AttributeValue::Text)),
    ]
}

fn create_layer(layer: &Layer) -> ModelCommand {
    ModelCommand::Create {
        id: layer.id,
        entity: LAYER_ENTITY.to_owned(),
        values: layer_values(layer)
            .into_iter()
            .filter_map(|(attribute, value)| Some((attribute.to_owned(), value?)))
            .collect(),
    }
}

fn update_layer(layer: &Layer) -> ModelCommand {
    ModelCommand::Batch {
        commands: layer_values(layer)
            .into_iter()
            .map(|(attribute, value)| ModelCommand::Set {
                id: layer.id,
                attribute: attribute.to_owned(),
                value,
            })
            .collect(),
    }
}

fn layer_refs(set: &LayerSet) -> AttributeValue {
    AttributeValue::List(
        set.layers()
            .iter()
            .map(|layer| AttributeValue::Ref(layer.id))
            .collect(),
    )
}

fn value(name: &str, value: AttributeValue) -> (String, AttributeValue) {
    (name.to_owned(), value)
}

#[cfg(test)]
mod tests;
