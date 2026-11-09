//! Build-ups in the model: the core schema's `MaterialLayerSet` and `MaterialLayer` entities.

use core_model::units::Length;
use core_model::{AttributeValue, EntityId, Model, ModelError};
use core_txn::ModelCommand;
use thiserror::Error;

use super::{ENTITY as MATERIAL, attribute};
use crate::layers::{Layer, LayerSet, LayerSetError};

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
    let mut commands: Vec<ModelCommand> = set
        .layers()
        .iter()
        .map(|layer| {
            let mut values = vec![
                value("material", AttributeValue::Ref(layer.material)),
                value("thickness", AttributeValue::Real(layer.thickness.si())),
                value("priority", AttributeValue::Integer(layer.priority)),
                value("is_core", AttributeValue::Bool(layer.core)),
            ];
            if let Some(name) = &layer.name {
                values.push(value("name", AttributeValue::Text(name.clone())));
            }
            ModelCommand::Create {
                id: layer.id,
                entity: LAYER_ENTITY.to_owned(),
                values,
            }
        })
        .collect();
    let layers = set
        .layers()
        .iter()
        .map(|layer| AttributeValue::Ref(layer.id))
        .collect();
    commands.push(ModelCommand::Create {
        id,
        entity: LAYER_SET_ENTITY.to_owned(),
        values: vec![
            value("name", AttributeValue::Text(set.name().to_owned())),
            value("layers", AttributeValue::List(layers)),
        ],
    });
    ModelCommand::Batch { commands }
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
    Ok(LayerSet::new(&name, layers)?)
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
    // Required values are always there; a missing one reads as invalid, never as a default.
    let thickness = model
        .get(id, "thickness")?
        .and_then(|value| value.as_real());
    Ok(Layer {
        id,
        material,
        thickness: Length::from_si(thickness.unwrap_or(f64::NAN)),
        priority: match model.get(id, "priority")? {
            Some(AttributeValue::Integer(priority)) => priority,
            _ => -1,
        },
        core: model.get(id, "is_core")? == Some(AttributeValue::Bool(true)),
        name: attribute(model, id, "name")?,
    })
}

fn value(name: &str, value: AttributeValue) -> (String, AttributeValue) {
    (name.to_owned(), value)
}

#[cfg(test)]
mod tests;
