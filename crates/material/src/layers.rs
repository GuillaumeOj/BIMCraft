//! Layered build-ups of walls and slabs (MAT-03): ordered layers, each with a material, a
//! thickness, a join priority and whether it belongs to the structural core.

use core_model::EntityId;
use core_model::units::Length;
use geom::{LINEAR_TOLERANCE, Layer as GeomLayer, LayerId, LocationError};
use thiserror::Error;

use crate::source::{MAX_TEXT_LEN, bounded_text};

/// The highest join priority, as IFC's `IfcMaterialLayer.Priority`.
pub const MAX_PRIORITY: i64 = 100;

/// One layer of a build-up.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// The layer object in the model; it identifies the layer whatever its position.
    pub id: EntityId,
    /// The material object the layer is made of.
    pub material: EntityId,
    /// Thickness.
    pub thickness: Length,
    /// Join priority, from 0 to [`MAX_PRIORITY`]: where layered elements meet, the layer with
    /// the higher priority runs through (GEO-11).
    pub priority: i64,
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
    /// A name is longer than [`MAX_TEXT_LEN`] characters.
    #[error("a name in the build-up is longer than {MAX_TEXT_LEN} characters")]
    TooLong,
    /// The build-up has no layer.
    #[error("the build-up has no layers")]
    Empty,
    /// The build-up has more layers than the geometry kernel takes.
    #[error("the build-up has more than {} layers", geom::MAX_LAYERS)]
    TooMany,
    /// A layer is not thicker than the model tolerance, or not finite.
    #[error("layer {0} has an invalid thickness")]
    Thickness(usize),
    /// A layer's priority is outside 0 to [`MAX_PRIORITY`].
    #[error("layer {0} has a priority outside 0 to {MAX_PRIORITY}")]
    Priority(usize),
    /// A layer object appears twice.
    #[error("layer {0} appears earlier in the build-up")]
    Duplicate(usize),
    /// The core layers are not next to each other, so the core has no single centre.
    #[error("the core layers are not next to each other")]
    CoreNotContiguous,
    /// The layers add up to more than the model extent.
    #[error("the build-up is thicker than the model extent")]
    TooThick,
}

/// A validated build-up: layers in order from the exterior face to the interior face, or from
/// the top face down for slabs (ADR 0010).
#[derive(Clone, Debug, PartialEq)]
pub struct LayerSet {
    name: String,
    layers: Vec<Layer>,
    geometry: geom::LayerSet,
}

impl LayerSet {
    /// The build-up `name` made of `layers`, from the exterior face inwards.
    ///
    /// # Errors
    ///
    /// Fails when the name is empty or too long, there is no layer or too many, a layer is too
    /// thin or has an invalid priority, a layer appears twice, the core layers are apart, or the
    /// whole is thicker than the model extent.
    pub fn new(name: &str, layers: Vec<Layer>) -> Result<Self, LayerSetError> {
        let too_long = |_| LayerSetError::TooLong;
        let name = bounded_text(name)
            .map_err(too_long)?
            .ok_or(LayerSetError::EmptyName)?;
        for (number, layer) in (1..).zip(&layers) {
            if let Some(layer_name) = &layer.name {
                bounded_text(layer_name).map_err(too_long)?;
            }
            let thickness = layer.thickness.si();
            if thickness.is_nan() || thickness <= LINEAR_TOLERANCE {
                return Err(LayerSetError::Thickness(number));
            }
            if !(0..=MAX_PRIORITY).contains(&layer.priority) {
                return Err(LayerSetError::Priority(number));
            }
            if layers[..number - 1]
                .iter()
                .any(|earlier| earlier.id == layer.id)
            {
                return Err(LayerSetError::Duplicate(number));
            }
        }
        let first = layers.iter().position(|layer| layer.core);
        let last = layers.iter().rposition(|layer| layer.core);
        if let (Some(first), Some(last)) = (first, last)
            && !layers[first..=last].iter().all(|layer| layer.core)
        {
            return Err(LayerSetError::CoreNotContiguous);
        }
        let geometry = geom::LayerSet::new(
            layers
                .iter()
                .enumerate()
                .map(|(index, layer)| GeomLayer {
                    id: layer_id(index),
                    thickness: layer.thickness.si(),
                    core: layer.core,
                })
                .collect(),
        )
        .map_err(|error| match error {
            LocationError::EmptyLayerSet => LayerSetError::Empty,
            LocationError::TooManyLayers => LayerSetError::TooMany,
            LocationError::InvalidThickness(index) => LayerSetError::Thickness(index + 1),
            _ => LayerSetError::TooThick,
        })?;
        Ok(Self {
            name: name.to_owned(),
            layers,
            geometry,
        })
    }

    /// The build-up's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
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

    /// The build-up as the geometry kernel resolves location lines on it. Layer `i` has
    /// [`LayerId`] `i`, so the kernel's layer ids index [`LayerSet::layers`].
    #[must_use]
    pub const fn geometry(&self) -> &geom::LayerSet {
        &self.geometry
    }
}

/// The kernel's id of the layer at `index`.
#[must_use]
pub fn layer_id(index: usize) -> LayerId {
    LayerId(u64::try_from(index).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests {
    use core_model::units::LengthUnit::Millimetre;

    use super::*;

    fn layer(raw: u128, millimetres: f64, priority: i64, core: bool) -> Layer {
        Layer {
            id: EntityId::new(raw),
            material: EntityId::new(100),
            thickness: Length::new(millimetres, Millimetre),
            priority,
            core,
            name: None,
        }
    }

    /// A 300 mm exterior wall: 20 mm render, 100 mm insulation, 160 mm concrete, 20 mm plaster.
    fn wall() -> Vec<Layer> {
        vec![
            layer(1, 20.0, 10, false),
            layer(2, 100.0, 30, false),
            layer(3, 160.0, 100, true),
            layer(4, 20.0, 10, false),
        ]
    }

    #[test]
    fn a_build_up_adds_its_layers_and_centres_on_its_core() {
        let set = LayerSet::new(" Exterior wall 300 ", wall()).unwrap();
        assert_eq!(set.name(), "Exterior wall 300");
        assert_eq!(set.layers().len(), 4);
        assert!((set.total_thickness().get(Millimetre) - 300.0).abs() < 1e-9);
        // Core from 120 mm to 280 mm: its centre is 200 mm from the exterior face.
        assert!((set.geometry().core_centre() - 0.2).abs() < 1e-12);
        assert_eq!(set.geometry().layers()[2].id, layer_id(2));
    }

    #[test]
    fn invalid_build_ups_are_rejected_with_the_reason() {
        let with = |index: usize, change: fn(&mut Layer)| {
            let mut layers = wall();
            change(&mut layers[index]);
            LayerSet::new("W", layers)
        };
        let long = "x".repeat(MAX_TEXT_LEN + 1);
        assert_eq!(LayerSet::new(" ", wall()), Err(LayerSetError::EmptyName));
        assert_eq!(LayerSet::new(&long, wall()), Err(LayerSetError::TooLong));
        assert_eq!(LayerSet::new("W", Vec::new()), Err(LayerSetError::Empty));
        let too_many = (0..=geom::MAX_LAYERS as u128)
            .map(|raw| layer(raw, 1.0, 0, false))
            .collect();
        assert_eq!(LayerSet::new("W", too_many), Err(LayerSetError::TooMany));
        let mut named = wall();
        named[0].name = Some(long);
        assert_eq!(LayerSet::new("W", named), Err(LayerSetError::TooLong));
        let thin = with(1, |layer| layer.thickness = Length::from_si(1e-7));
        assert_eq!(thin, Err(LayerSetError::Thickness(2)));
        let nan = with(0, |layer| layer.thickness = Length::from_si(f64::NAN));
        assert_eq!(nan, Err(LayerSetError::Thickness(1)));
        let huge = with(0, |layer| layer.thickness = Length::from_si(2e4));
        assert_eq!(huge, Err(LayerSetError::Thickness(1)));
        assert_eq!(
            with(3, |layer| layer.priority = 101),
            Err(LayerSetError::Priority(4))
        );
        assert_eq!(
            with(3, |layer| layer.priority = -1),
            Err(LayerSetError::Priority(4))
        );
        assert_eq!(
            with(3, |layer| layer.id = EntityId::new(1)),
            Err(LayerSetError::Duplicate(4))
        );
        assert_eq!(
            with(0, |layer| layer.core = true),
            Err(LayerSetError::CoreNotContiguous)
        );
        let thick = (1..=3).map(|raw| layer(raw, 4e6, 0, false)).collect();
        assert_eq!(LayerSet::new("W", thick), Err(LayerSetError::TooThick));
    }

    #[test]
    fn a_build_up_without_core_centres_on_its_middle() {
        let mut layers = wall();
        layers[2].core = false;
        let set = LayerSet::new("W", layers).unwrap();
        assert!((set.geometry().core_centre() - 0.15).abs() < 1e-12);
    }

    #[test]
    fn errors_give_positions_not_values() {
        assert_eq!(
            LayerSetError::Priority(2).to_string(),
            "layer 2 has a priority outside 0 to 100"
        );
        assert_eq!(
            LayerSetError::TooMany.to_string(),
            "the build-up has more than 256 layers"
        );
        assert_eq!(
            LayerSetError::CoreNotContiguous.to_string(),
            "the core layers are not next to each other"
        );
    }
}
