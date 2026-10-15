//! Location lines of layered sections: walls now, slabs later (ARC-30).

use core::fmt;

use super::LocationError;
use crate::tolerance::is_model_coordinate;
use crate::{LINEAR_TOLERANCE, MODEL_EXTENT};

/// Largest number of layers in a layer set; keeps hostile input from making lookups slow.
pub const MAX_LAYERS: usize = 256;

/// Identifies a layer for as long as it exists, whatever its position in the set.
///
/// Location lines refer to layers by id, so reordering layers keeps the user's choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayerId(pub u64);

/// One layer of a layered section.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    /// Stable id.
    pub id: LayerId,
    /// Thickness, in metres.
    pub thickness: f64,
    /// Whether the layer is part of the structural core (MAT-03).
    pub core: bool,
}

/// A face of a layer, or of the whole section.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// The face towards the exterior.
    Exterior,
    /// The face towards the interior.
    Interior,
}

/// Where on a layer a snap point sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapPosition {
    /// The layer's exterior face.
    ExteriorFace,
    /// Halfway through the layer.
    Centre,
    /// The layer's interior face.
    InteriorFace,
}

impl fmt::Display for SnapPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExteriorFace => "exterior face",
            Self::Centre => "centre",
            Self::InteriorFace => "interior face",
        })
    }
}

/// A snap point, named by its layer's id so that it survives reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SnapPoint {
    /// The layer.
    pub layer: LayerId,
    /// Where on the layer.
    pub position: SnapPosition,
}

/// The line a layered element is drawn along (ARC-30).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LocationLine {
    /// The middle of the core layers, or of the whole section when no layer is core.
    #[default]
    CoreCentre,
    /// A face of the whole section, whatever layer is there.
    WallFace(Face),
    /// A face or the centre of one layer.
    Snap(SnapPoint),
    /// An offset from one face of one layer, positive towards the interior, in metres.
    Custom {
        /// The layer.
        layer: LayerId,
        /// The face of that layer the offset starts from.
        face: Face,
        /// The offset, in metres, positive towards the interior.
        offset: f64,
    },
}

/// A snap point of a layer set with its position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapInfo {
    /// The snap point.
    pub point: SnapPoint,
    /// The layer's current position in the set, from 1, as shown to users.
    pub layer_number: usize,
    /// Distance from the section's exterior face, in metres.
    pub distance: f64,
}

impl SnapInfo {
    /// The English name shown in pickers, e.g. "Layer 3 — Structural core · centre", given the
    /// layer's function name. The UI builds its localised name from the same fields (UI-04).
    #[must_use]
    pub fn name(&self, function: &str) -> String {
        format!(
            "Layer {} — {function} · {}",
            self.layer_number, self.point.position
        )
    }
}

/// A resolved location line, possibly after falling back to the core centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Resolved {
    /// Distance of the location line from the section's exterior face, in metres.
    pub distance: f64,
    /// Why the requested location line was replaced by the core centre, if it was.
    pub fallback: Option<LocationError>,
}

/// The layers of a section from its exterior face to its interior face, validated.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    layers: Vec<Layer>,
    /// Distance of every face from the section's exterior face: layer `i` runs from `faces[i]`
    /// to `faces[i + 1]`, and the last entry is the total thickness.
    faces: Vec<f64>,
}

impl LayerSet {
    /// Validates the layers, listed from the exterior face to the interior face.
    ///
    /// # Errors
    ///
    /// [`LocationError`] when the set is empty or has more than [`MAX_LAYERS`] layers, when a
    /// thickness is not positive and within [`MODEL_EXTENT`], when two layers share an id, or
    /// when the layers add up to more than [`MODEL_EXTENT`].
    pub fn new(layers: Vec<Layer>) -> Result<Self, LocationError> {
        if layers.is_empty() {
            return Err(LocationError::EmptyLayerSet);
        }
        if layers.len() > MAX_LAYERS {
            return Err(LocationError::TooManyLayers);
        }
        let mut faces = Vec::with_capacity(layers.len() + 1);
        let mut total = 0.0;
        for (i, layer) in layers.iter().enumerate() {
            if !(layer.thickness > 0.0 && is_model_coordinate(layer.thickness)) {
                return Err(LocationError::InvalidThickness(i));
            }
            if layers[..i].iter().any(|earlier| earlier.id == layer.id) {
                return Err(LocationError::DuplicateLayer(i));
            }
            faces.push(total);
            total += layer.thickness;
        }
        if total > MODEL_EXTENT {
            return Err(LocationError::SectionTooThick);
        }
        faces.push(total);
        Ok(Self { layers, faces })
    }

    /// The layers, from the exterior face to the interior face.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// Total thickness, in metres.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.faces[self.layers.len()]
    }

    /// Distance of the core centre from the exterior face: the middle of the span from the first
    /// core layer to the last one, or of the whole section when no layer is core.
    #[must_use]
    pub fn core_centre(&self) -> f64 {
        let first = self.layers.iter().position(|layer| layer.core);
        let last = self.layers.iter().rposition(|layer| layer.core);
        match (first, last) {
            (Some(first), Some(last)) => f64::midpoint(self.faces[first], self.faces[last + 1]),
            _ => self.total() / 2.0,
        }
    }

    /// Every snap point from the exterior face to the interior face: each layer's exterior face
    /// and centre, then the last layer's interior face. A face shared by two layers is listed
    /// once, as the exterior face of the inner layer.
    #[must_use]
    pub fn snap_points(&self) -> Vec<SnapInfo> {
        let info = |i: usize, position| SnapInfo {
            point: SnapPoint {
                layer: self.layers[i].id,
                position,
            },
            layer_number: i + 1,
            distance: self.snap_distance(i, position),
        };
        let last = self.layers.len() - 1;
        (0..self.layers.len())
            .flat_map(|i| {
                [
                    info(i, SnapPosition::ExteriorFace),
                    info(i, SnapPosition::Centre),
                ]
            })
            .chain([info(last, SnapPosition::InteriorFace)])
            .collect()
    }

    /// Distance of the location line from the exterior face, in metres.
    ///
    /// # Errors
    ///
    /// [`LocationError::UnknownLayer`] when the line refers to a layer not in the set,
    /// [`LocationError::InvalidOffset`] for a custom offset that is not a finite number within
    /// the model extent, and [`LocationError::OffsetOutsideSection`] when it falls outside the
    /// section by more than [`LINEAR_TOLERANCE`].
    pub fn resolve(&self, line: &LocationLine) -> Result<f64, LocationError> {
        match *line {
            LocationLine::CoreCentre => Ok(self.core_centre()),
            LocationLine::WallFace(Face::Exterior) => Ok(0.0),
            LocationLine::WallFace(Face::Interior) => Ok(self.total()),
            LocationLine::Snap(SnapPoint { layer, position }) => {
                Ok(self.snap_distance(self.index_of(layer)?, position))
            }
            LocationLine::Custom {
                layer,
                face,
                offset,
            } => {
                let index = self.index_of(layer)?;
                if !is_model_coordinate(offset) {
                    return Err(LocationError::InvalidOffset);
                }
                let base = match face {
                    Face::Exterior => self.faces[index],
                    Face::Interior => self.faces[index + 1],
                };
                let (distance, total) = (base + offset, self.total());
                if distance < -LINEAR_TOLERANCE || distance > total + LINEAR_TOLERANCE {
                    return Err(LocationError::OffsetOutsideSection);
                }
                Ok(distance.clamp(0.0, total))
            }
        }
    }

    /// Like [`resolve`](Self::resolve), but falls back to the core centre when the line cannot
    /// be resolved against this set, for instance after its layer was removed, and says why.
    #[must_use]
    pub fn resolve_or_core_centre(&self, line: &LocationLine) -> Resolved {
        match self.resolve(line) {
            Ok(distance) => Resolved {
                distance,
                fallback: None,
            },
            Err(error) => Resolved {
                distance: self.core_centre(),
                fallback: Some(error),
            },
        }
    }

    /// The location line for a distance from the exterior face read from IFC: the core centre or
    /// a snap point within [`LINEAR_TOLERANCE`], otherwise a custom offset from the exterior face
    /// of the first layer, at exactly that distance.
    ///
    /// # Errors
    ///
    /// [`LocationError::InvalidOffset`] for a distance that is not a finite number within the
    /// model extent, and [`LocationError::OffsetOutsideSection`] for one outside the section.
    pub fn line_at(&self, distance: f64) -> Result<LocationLine, LocationError> {
        let first = self.layers[0].id;
        let line = LocationLine::Custom {
            layer: first,
            face: Face::Exterior,
            offset: distance,
        };
        self.resolve(&line)?;
        let near = |d: f64| (d - distance).abs() <= LINEAR_TOLERANCE;
        if near(self.core_centre()) {
            return Ok(LocationLine::CoreCentre);
        }
        Ok(self
            .snap_points()
            .into_iter()
            .find(|snap| near(snap.distance))
            .map_or(line, |snap| LocationLine::Snap(snap.point)))
    }

    fn index_of(&self, id: LayerId) -> Result<usize, LocationError> {
        self.layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or(LocationError::UnknownLayer)
    }

    fn snap_distance(&self, index: usize, position: SnapPosition) -> f64 {
        let (start, end) = (self.faces[index], self.faces[index + 1]);
        match position {
            SnapPosition::ExteriorFace => start,
            SnapPosition::Centre => f64::midpoint(start, end),
            SnapPosition::InteriorFace => end,
        }
    }
}

/// The direction the layers run in from the exterior face, along the element's local y axis
/// (IFC `IfcDirectionSenseEnum`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DirectionSense {
    /// Towards +y: a wall that is not flipped.
    #[default]
    Positive,
    /// Towards −y: a flipped wall.
    Negative,
}

impl DirectionSense {
    /// The other sense.
    #[must_use]
    pub const fn reversed(self) -> Self {
        match self {
            Self::Positive => Self::Negative,
            Self::Negative => Self::Positive,
        }
    }

    /// +1 when the layers run towards +y, −1 otherwise. Multiplying by ±1 is exact.
    const fn sign(self) -> f64 {
        match self {
            Self::Positive => 1.0,
            Self::Negative => -1.0,
        }
    }
}

/// Where a layered section sits across its location line, which is the element's axis (y = 0),
/// in the terms of IFC `IfcMaterialLayerSetUsage`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerPlacement {
    /// Local y of the section's exterior face (IFC `OffsetFromReferenceLine`), in metres.
    pub offset_from_reference_line: f64,
    /// The direction the layers run in from the exterior face.
    pub direction_sense: DirectionSense,
}

impl LayerPlacement {
    /// The placement of a section whose location line is `distance` from its exterior face.
    #[must_use]
    pub const fn new(distance: f64, direction_sense: DirectionSense) -> Self {
        Self {
            offset_from_reference_line: -direction_sense.sign() * distance,
            direction_sense,
        }
    }

    /// Distance of the location line from the exterior face.
    #[must_use]
    pub const fn distance(&self) -> f64 {
        -self.direction_sense.sign() * self.offset_from_reference_line
    }

    /// Local y of the interior face, for a section `total` thick.
    #[must_use]
    pub const fn interior_y(&self, total: f64) -> f64 {
        self.offset_from_reference_line + self.direction_sense.sign() * total
    }

    /// The section mirrored about its location line (a wall flip).
    #[must_use]
    pub const fn flipped(&self) -> Self {
        Self {
            offset_from_reference_line: -self.offset_from_reference_line,
            direction_sense: self.direction_sense.reversed(),
        }
    }

    /// How far, along local +y, the axis moves when the location line moves to `distance` from
    /// the exterior face while the section stays where it is (ARC-30).
    #[must_use]
    pub const fn axis_shift(&self, distance: f64) -> f64 {
        self.offset_from_reference_line
            - Self::new(distance, self.direction_sense).offset_from_reference_line
    }
}
