//! Generators built into the scene, for the entities of the core schema that have a
//! representation.
//!
//! They are plain functions of the model, so the desktop host and the browser host register the
//! same ones (ARCH-01).

use core_model::{AttributeValue, EntityId, Model};
use geom::{Edge, Point2, Profile, Solid, Vector3};
use scene_wire::MAX_WORLD_COORDINATE;

use crate::{Generated, GeneratorError, Generators, Placement};

/// The path the core schema's `Block` names for its `body` representation.
pub const BLOCK_BODY: &str = "block::body";

/// Registers every built-in generator.
pub fn register(generators: &mut Generators) {
    generators.register(BLOCK_BODY, block_body);
}

/// The body of a `Block`: a box of `size_x` by `size_y` by `size_z` whose base is centred on
/// `location` and turned by `rotation` about the vertical axis.
///
/// The local solid depends on the three sizes only, so blocks of one size share a mesh; the
/// location and rotation are the placement.
///
/// # Errors
///
/// [`GeneratorError::MissingInput`] when an attribute is unset, [`GeneratorError::InvalidInput`]
/// when a size is not positive, the location does not hold three lengths or lies beyond the
/// world extent, or a value is not finite.
pub fn block_body(model: &Model, block: EntityId) -> Result<Generated, GeneratorError> {
    let location = location_of(model, block)?;
    // Unset is the default; a value of another type is as invalid as it is for the sizes.
    let rotation = match real(model, block, "rotation") {
        Err(GeneratorError::MissingInput) => 0.0,
        other => other?,
    };
    let size = [
        real(model, block, "size_x")?,
        real(model, block, "size_y")?,
        real(model, block, "size_z")?,
    ];
    if !rotation.is_finite() || size.iter().any(|s| !s.is_finite() || *s <= 0.0) {
        return Err(GeneratorError::InvalidInput);
    }
    Ok(Generated {
        solids: vec![box_solid(size)?],
        placement: Placement::about_z(rotation, location),
        ..Generated::default()
    })
}

/// The location of a block: three lengths in metres, within the world extent.
fn location_of(model: &Model, block: EntityId) -> Result<[f64; 3], GeneratorError> {
    let value = model
        .get(block, "location")
        .map_err(|_| GeneratorError::InvalidInput)?
        .ok_or(GeneratorError::MissingInput)?;
    let AttributeValue::List(items) = value.plain() else {
        return Err(GeneratorError::InvalidInput);
    };
    let coordinates: Vec<f64> = items
        .iter()
        .map(|item| match item.plain() {
            AttributeValue::Real(v) => Some(*v),
            _ => None,
        })
        .collect::<Option<_>>()
        .ok_or(GeneratorError::InvalidInput)?;
    match coordinates[..] {
        [x, y, z] if [x, y, z].iter().all(|c| c.abs() <= MAX_WORLD_COORDINATE) => Ok([x, y, z]),
        _ => Err(GeneratorError::InvalidInput),
    }
}

fn real(model: &Model, id: EntityId, attribute: &str) -> Result<f64, GeneratorError> {
    match model
        .get(id, attribute)
        .map_err(|_| GeneratorError::InvalidInput)?
    {
        Some(value) => match value.plain() {
            AttributeValue::Real(v) => Ok(*v),
            _ => Err(GeneratorError::InvalidInput),
        },
        None => Err(GeneratorError::MissingInput),
    }
}

/// A box centred on the origin in x and y, standing on z = 0.
fn box_solid([x, y, z]: [f64; 3]) -> Result<Solid, GeneratorError> {
    let (hx, hy) = (x / 2.0, y / 2.0);
    let corners = [
        Point2::new(-hx, -hy),
        Point2::new(hx, -hy),
        Point2::new(hx, hy),
        Point2::new(-hx, hy),
    ];
    let edges = (0..corners.len())
        .map(|i| Edge::Line {
            start: corners[i],
            end: corners[(i + 1) % corners.len()],
        })
        .collect();
    let profile = Profile::new(edges, Vec::new())?;
    Ok(Solid::extrude(&profile, Vector3::new(0.0, 0.0, z))?)
}
