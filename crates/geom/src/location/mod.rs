//! Location lines of layered sections and location points of profiles (ARC-30, STR-14, GEO-13).
//!
//! Conventions and the IFC mapping are in `docs/dev/adr/0010-location-lines.md`.

mod layers;
mod point;

pub use layers::{
    DirectionSense, Face, Layer, LayerId, LayerPlacement, LayerSet, LocationLine, MAX_LAYERS,
    Resolved, SnapInfo, SnapPoint, SnapPosition,
};
pub use point::{CardinalPoint, IfcCardinalPoint, LocationPoint};

use crate::ShearCentreError;

/// Why a location line or point could not be resolved. Layers are identified by their position
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
    /// The shear centre was asked for but could not be computed for this profile.
    #[error("the shear centre of this profile could not be computed")]
    ShearCentreUnavailable(#[source] ShearCentreError),
    /// An IFC cardinal point code outside 1–19.
    #[error("{0} is not an IFC cardinal point")]
    UnknownCardinalPoint(u8),
}

#[cfg(test)]
mod tests;
