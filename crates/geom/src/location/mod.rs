//! Location lines of layered sections (ARC-30, GEO-13).
//!
//! Conventions and the IFC mapping are in `docs/dev/adr/0010-location-lines.md`.

mod layers;

pub use layers::{
    DirectionSense, Face, Layer, LayerId, LayerPlacement, LayerSet, LocationLine, MAX_LAYERS,
    Resolved, SnapInfo, SnapPoint, SnapPosition,
};

/// Why a location line could not be resolved. Layers are identified by their position
/// in the input (from 0); dimensions are model content and are not carried (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum LocationError {
    /// A layer set has no layer.
    #[error("the layer set has no layers")]
    EmptyLayerSet,
    /// A layer set has more than [`MAX_LAYERS`] layers.
    #[error("the layer set has more than {MAX_LAYERS} layers")]
    TooManyLayers,
    /// A layer thickness is not a positive number within the model extent.
    #[error("layer {0} has an invalid thickness")]
    InvalidThickness(usize),
    /// Two layers share an id.
    #[error("layer {0} has the same id as an earlier layer")]
    DuplicateLayer(usize),
    /// The layers add up to more than the model extent.
    #[error("the layers are thicker than the model extent")]
    SectionTooThick,
    /// The location refers to a layer that is not in the set.
    #[error("the location refers to a layer that is not in the layer set")]
    UnknownLayer,
    /// An offset is not a finite number within the model extent.
    #[error("the offset is not a finite number within the model extent")]
    InvalidOffset,
    /// The location falls outside the section.
    #[error("the location falls outside the section")]
    OffsetOutsideSection,
}

#[cfg(test)]
mod tests;
