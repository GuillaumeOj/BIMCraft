//! Layered build-ups of walls and slabs (MAT-03): ordered layers, each with a material, a
//! thickness, a function, a join priority and whether it belongs to the structural core.
//!
//! The rules live here so that the API and the wall and slab code to come (epic #88) check a
//! build-up the same way, and [`LayerSet::problems`] reports every problem at once.

use core_model::EntityId;
use core_model::units::Length;
use geom::{LINEAR_TOLERANCE, Layer as GeomLayer, LayerId, LocationError};
use thiserror::Error;

use crate::material::Category;
use crate::source::{MAX_TEXT_LEN, bounded_text};

/// The most layers a build-up may have.
pub const MAX_LAYERS: usize = 32;
/// The highest join priority number: the lowest priority.
pub const MAX_PRIORITY: u8 = 5;

/// What a layer does, which decides its default priority and whether it starts in the core.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayerFunction {
    /// The load-bearing layer.
    Structure,
    /// A base for another layer.
    Substrate,
    /// Thermal or acoustic insulation.
    Insulation,
    /// A ventilated or still air layer.
    AirGap,
    /// A thin sheet, such as a vapour barrier; it may have no thickness.
    Membrane,
    /// An exposed surface.
    Finish,
}

impl LayerFunction {
    /// Every function.
    pub const ALL: [Self; 6] = [
        Self::Structure,
        Self::Substrate,
        Self::Insulation,
        Self::AirGap,
        Self::Membrane,
        Self::Finish,
    ];

    /// The name stored in the model and used on the wire.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Structure => "structure",
            Self::Substrate => "substrate",
            Self::Insulation => "insulation",
            Self::AirGap => "air_gap",
            Self::Membrane => "membrane",
            Self::Finish => "finish",
        }
    }

    /// The function stored under `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|function| function.name() == name)
    }

    /// The join priority a layer of this function has unless the user changes it: structure 1,
    /// substrate 2, insulation and air gap 3, membrane 4, finish 5.
    #[must_use]
    pub const fn default_priority(self) -> u8 {
        match self {
            Self::Structure => 1,
            Self::Substrate => 2,
            Self::Insulation | Self::AirGap => 3,
            Self::Membrane => 4,
            Self::Finish => 5,
        }
    }

    /// Whether a layer of this function starts in the structural core.
    #[must_use]
    pub const fn default_core(self) -> bool {
        matches!(self, Self::Structure)
    }
}

/// What a build-up is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Usage {
    /// Layers run from the exterior face to the interior face.
    Wall,
    /// Layers run from the top face down.
    Slab,
}

impl Usage {
    /// Both usages.
    pub const ALL: [Self; 2] = [Self::Wall, Self::Slab];

    /// The name stored in the model and used on the wire.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Slab => "slab",
        }
    }

    /// The usage stored under `name`, if any.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|usage| usage.name() == name)
    }
}

/// The priority an IFC export writes for join priority `priority` (1 highest … 5 lowest):
/// `IfcMaterialLayer.Priority` runs from 0 to 100, higher winning, so 1 is 100 and 5 is 0.
#[must_use]
pub const fn ifc_priority(priority: u8) -> u8 {
    (MAX_PRIORITY - priority) * 25
}

/// The join priority an IFC import gives `Priority`: the nearest of 1 to 5.
#[must_use]
pub const fn priority_from_ifc(priority: u8) -> u8 {
    let steps = (priority.saturating_add(12)) / 25;
    let steps = if steps > MAX_PRIORITY - 1 {
        MAX_PRIORITY - 1
    } else {
        steps
    };
    MAX_PRIORITY - steps
}

/// One layer of a build-up.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// The layer object in the model; it identifies the layer whatever its position.
    pub id: EntityId,
    /// The material object the layer is made of.
    pub material: EntityId,
    /// Thickness.
    pub thickness: Length,
    /// What the layer does.
    pub function: LayerFunction,
    /// Join priority, from 1 (highest) to [`MAX_PRIORITY`]: where layered elements meet, the
    /// layer with the higher priority runs through (GEO-11).
    pub priority: u8,
    /// Whether the layer belongs to the structural core.
    pub core: bool,
    /// Name shown in plans and schedules, if any.
    pub name: Option<String>,
}

/// Why a build-up was rejected. Messages give layer positions from 1, never values (SEC-03).
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum LayerSetError {
    /// The build-up has no name.
    #[error("the build-up has no name")]
    EmptyName,
    /// A name or description is longer than [`MAX_TEXT_LEN`] characters.
    #[error("a text of the build-up is longer than {MAX_TEXT_LEN} characters")]
    TooLong,
    /// A name or description holds a control character, such as a line break.
    #[error("a text of the build-up contains a control character")]
    ControlCharacter,
    /// The build-up has no layer.
    #[error("the build-up has no layers")]
    NoLayers,
    /// The build-up has more than [`MAX_LAYERS`] layers.
    #[error("the build-up has more than {MAX_LAYERS} layers")]
    TooManyLayers,
    /// A layer is not thicker than the model tolerance (a membrane may have none), or not finite.
    #[error("layer {0} has an invalid thickness")]
    InvalidThickness(usize),
    /// A layer's priority is outside 1 to [`MAX_PRIORITY`].
    #[error("layer {0} has a priority outside 1 to {MAX_PRIORITY}")]
    InvalidPriority(usize),
    /// A layer object appears twice.
    #[error("layer {0} appears earlier in the build-up")]
    DuplicateLayer(usize),
    /// The core layers are not next to each other, so the core has no single centre.
    #[error("the core layers are not next to each other")]
    CoreNotContiguous,
    /// The layers add up to more than the model extent.
    #[error("the build-up is thicker than the model extent")]
    TooThick,
    /// A layer's material is not a material of the project.
    #[error("the material of layer {0} is not a material of the project")]
    UnknownMaterial(usize),
    /// An air gap is not made of a generic material.
    #[error("the material of air gap {0} is not a generic material")]
    InvalidMaterial(usize),
}

/// A validated build-up: layers in order from the exterior face to the interior face, or from
/// the top face down for slabs (ADR 0010).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    name: String,
    usage: Usage,
    description: Option<String>,
    layers: Vec<Layer>,
    geometry: geom::LayerSet,
}

impl LayerSet {
    /// The build-up `name` for `usage` made of `layers`, from the exterior face inwards.
    ///
    /// # Errors
    ///
    /// The first problem [`LayerSet::problems`] finds, except the checks on materials, which
    /// need the project.
    pub fn new(
        name: &str,
        usage: Usage,
        description: Option<&str>,
        layers: Vec<Layer>,
    ) -> Result<Self, LayerSetError> {
        if let Some(problem) = Self::problems(name, description, &layers, |_| None)
            .into_iter()
            .find(|problem| {
                !matches!(
                    problem,
                    LayerSetError::UnknownMaterial(_) | LayerSetError::InvalidMaterial(_)
                )
            })
        {
            return Err(problem);
        }
        // The texts and the layers are valid, so these cannot fail.
        let name = bounded_text(name)
            .ok()
            .flatten()
            .ok_or(LayerSetError::EmptyName)?;
        let description = description
            .map(bounded_text)
            .transpose()
            .map_err(text_error)?
            .flatten();
        let geometry = geometry(&layers).map_err(|error| match error {
            LocationError::EmptyLayerSet => LayerSetError::NoLayers,
            LocationError::TooManyLayers => LayerSetError::TooManyLayers,
            LocationError::InvalidThickness(index) => LayerSetError::InvalidThickness(index + 1),
            _ => LayerSetError::TooThick,
        })?;
        Ok(Self {
            name: name.to_owned(),
            usage,
            description: description.map(str::to_owned),
            layers,
            geometry,
        })
    }

    /// Every problem of a build-up, in order, so a client can show them all at once. The
    /// category of a layer's material comes from `category_of`, which answers `None` for an
    /// object that is not a material; a build-up whose materials are not known passes `|_| None`
    /// and ignores the two material problems.
    #[must_use]
    pub fn problems(
        name: &str,
        description: Option<&str>,
        layers: &[Layer],
        category_of: impl Fn(EntityId) -> Option<Category>,
    ) -> Vec<LayerSetError> {
        let mut problems = Vec::new();
        match bounded_text(name) {
            Ok(Some(_)) => {}
            Ok(None) => problems.push(LayerSetError::EmptyName),
            Err(error) => problems.push(text_error(error)),
        }
        if let Some(Err(error)) = description.map(bounded_text) {
            problems.push(text_error(error));
        }
        if layers.is_empty() {
            problems.push(LayerSetError::NoLayers);
        }
        if layers.len() > MAX_LAYERS {
            problems.push(LayerSetError::TooManyLayers);
        }
        for (number, layer) in (1..).zip(layers) {
            layer_problems(number, layer, layers, &category_of, &mut problems);
        }
        let first = layers.iter().position(|layer| layer.core);
        let last = layers.iter().rposition(|layer| layer.core);
        if let (Some(first), Some(last)) = (first, last)
            && !layers[first..=last].iter().all(|layer| layer.core)
        {
            problems.push(LayerSetError::CoreNotContiguous);
        }
        if !layers.is_empty() && layers.iter().all(|layer| layer.thickness.si() == 0.0) {
            problems.push(LayerSetError::InvalidThickness(1));
        }
        if problems.is_empty() && geometry(layers).is_err() {
            problems.push(LayerSetError::TooThick);
        }
        problems
    }

    /// The build-up's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the build-up is for.
    #[must_use]
    pub const fn usage(&self) -> Usage {
        self.usage
    }

    /// The description, if any.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// The layers, from the exterior face inwards.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The sum of the layer thicknesses.
    #[must_use]
    pub fn total_thickness(&self) -> Length {
        Length::from_si(self.geometry.total())
    }

    /// The build-up as the geometry kernel resolves location lines on it. Layers without
    /// thickness have no geometry; the kernel's [`LayerId`] of the others is [`layer_id`] of
    /// their position in [`LayerSet::layers`].
    #[must_use]
    pub const fn geometry(&self) -> &geom::LayerSet {
        &self.geometry
    }

    /// The thermal resistance of each layer, d / λ, and their sum, where the conductivity
    /// `conductivity` gives a value (W/(m·K)). Air gaps and layers whose material has no λ have
    /// none: the surface and air-layer resistances belong to the thermal module.
    #[must_use]
    pub fn thermal(&self, conductivity: impl Fn(&Layer) -> Option<f64>) -> Thermal {
        thermal(self.layers.iter().map(|layer| (layer, conductivity(layer))))
    }
}

/// The thermal resistance of a build-up, in m²·K/W, from the layers whose material has a
/// conductivity.
#[derive(Clone, Debug, PartialEq)]
pub struct Thermal {
    /// The resistance of each layer, `None` where there is no conductivity.
    pub layers: Vec<Option<f64>>,
    /// The sum over the layers that have one; `None` only when none has.
    pub total: Option<f64>,
    /// How many layers have no resistance.
    pub layers_without_data: usize,
}

/// The [`Thermal`] of `layers` with their conductivity. Used by [`LayerSet::thermal`] and for a
/// draft that is not a valid build-up yet.
pub fn thermal<'a>(layers: impl IntoIterator<Item = (&'a Layer, Option<f64>)>) -> Thermal {
    let resistances: Vec<Option<f64>> = layers
        .into_iter()
        .map(|(layer, lambda)| {
            let lambda = lambda.filter(|lambda| *lambda > 0.0 && lambda.is_finite())?;
            (layer.function != LayerFunction::AirGap).then(|| layer.thickness.si() / lambda)
        })
        .collect();
    let given: Vec<f64> = resistances.iter().flatten().copied().collect();
    Thermal {
        total: (!given.is_empty()).then(|| given.iter().sum()),
        layers_without_data: resistances.len() - given.len(),
        layers: resistances,
    }
}

fn layer_problems(
    number: usize,
    layer: &Layer,
    layers: &[Layer],
    category_of: &impl Fn(EntityId) -> Option<Category>,
    problems: &mut Vec<LayerSetError>,
) {
    if let Some(Err(error)) = layer.name.as_deref().map(bounded_text) {
        problems.push(text_error(error));
    }
    let thickness = layer.thickness.si();
    // A membrane may have no thickness at all, but not a vanishing one; no other layer may.
    let thin = if layer.function == LayerFunction::Membrane && !layer.core {
        !thickness.is_finite()
            || thickness < 0.0
            || (thickness > 0.0 && thickness <= LINEAR_TOLERANCE)
    } else {
        !thickness.is_finite() || thickness <= LINEAR_TOLERANCE
    };
    if thin {
        problems.push(LayerSetError::InvalidThickness(number));
    }
    if !(1..=MAX_PRIORITY).contains(&layer.priority) {
        problems.push(LayerSetError::InvalidPriority(number));
    }
    if layers[..number - 1]
        .iter()
        .any(|earlier| earlier.id == layer.id)
    {
        problems.push(LayerSetError::DuplicateLayer(number));
    }
    match category_of(layer.material) {
        Some(category)
            if layer.function == LayerFunction::AirGap && category != Category::Generic =>
        {
            problems.push(LayerSetError::InvalidMaterial(number));
        }
        _ => {}
    }
}

fn text_error(error: crate::SourceError) -> LayerSetError {
    match error {
        crate::SourceError::ControlCharacter => LayerSetError::ControlCharacter,
        _ => LayerSetError::TooLong,
    }
}

/// The geometry of the layers that have a thickness; a membrane without one adds nothing to it.
fn geometry(layers: &[Layer]) -> Result<geom::LayerSet, LocationError> {
    geom::LayerSet::new(
        layers
            .iter()
            .enumerate()
            .filter(|(_, layer)| layer.thickness.si() > 0.0)
            .map(|(index, layer)| GeomLayer {
                id: layer_id(index),
                thickness: layer.thickness.si(),
                core: layer.core,
            })
            .collect(),
    )
}

/// The kernel's id of the layer at `index`.
#[must_use]
pub fn layer_id(index: usize) -> LayerId {
    LayerId(u64::try_from(index).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests;
