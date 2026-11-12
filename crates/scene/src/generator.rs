//! Generators: the functions that compute the solids of an element's representation.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use core_model::{EntityId, Model};
use geom::{BooleanError, BrepError, ExtrusionError, ProfileError, Solid};

/// Computes the solids of one representation of an element (GEO-06).
///
/// A generator reads the model and nothing else, so its result is a function of the objects it
/// read: the element, its type, the relationships it takes part in, and the objects it lists in
/// [`Generated::inputs`]. The scene recomputes it when one of them changes, and only then.
pub trait Generator {
    /// The solids of `element`'s representation.
    ///
    /// # Errors
    ///
    /// A [`GeneratorError`] when the element's inputs give no geometry. Its message must carry no
    /// coordinates or model content (SEC-03).
    fn generate(&self, model: &Model, element: EntityId) -> Result<Generated, GeneratorError>;
}

/// Any function of the model and an element is a generator.
impl<F> Generator for F
where
    F: Fn(&Model, EntityId) -> Result<Generated, GeneratorError>,
{
    fn generate(&self, model: &Model, element: EntityId) -> Result<Generated, GeneratorError> {
        self(model, element)
    }
}

/// What a generator computed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generated {
    /// The solids of the representation.
    pub solids: Vec<Solid>,
    /// Other objects the generator read (a layer set, an opening…), beyond the element itself,
    /// its type and the relationships it takes part in, which the scene always tracks. A change
    /// to any of them makes the element's mesh stale.
    pub inputs: BTreeSet<EntityId>,
}

/// Why a generator computed no solid.
///
/// Messages carry no coordinates or model content (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum GeneratorError {
    /// The element lacks something the representation needs.
    #[error("the element lacks an input the representation needs")]
    MissingInput,
    /// The element's inputs describe no valid geometry.
    #[error("the element's inputs describe no valid geometry")]
    InvalidInput,
    /// The profile of the representation is invalid.
    #[error("the profile is invalid: {0}")]
    Profile(#[from] ProfileError),
    /// A solid could not be extruded.
    #[error("extrusion failed: {0}")]
    Extrusion(#[from] ExtrusionError),
    /// A boolean operation failed.
    #[error("boolean operation failed: {0}")]
    Boolean(#[from] BooleanError),
    /// A solid is invalid.
    #[error("the solid is invalid: {0}")]
    Solid(#[from] BrepError),
}

/// Generators by the path the schema names in `Representation::generator`, such as `wall::body`.
#[derive(Default)]
pub struct Generators {
    by_path: BTreeMap<String, Box<dyn Generator>>,
}

impl Generators {
    /// No generator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the generator of representations that name `path`, replacing the previous one.
    pub fn register(&mut self, path: impl Into<String>, generator: impl Generator + 'static) {
        self.by_path.insert(path.into(), Box::new(generator));
    }

    pub(crate) fn get(&self, path: &str) -> Option<&dyn Generator> {
        self.by_path.get(path).map(Box::as_ref)
    }
}

impl fmt::Debug for Generators {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.by_path.keys()).finish()
    }
}
