//! Why an element has no mesh.

use geom::TessellationError;

use crate::GeneratorError;

/// Why an element could not be meshed.
///
/// Variants name what failed by solid and face index, never by coordinates, and carry no model
/// content: coordinates and attribute values are model content and must stay out of logs and
/// crash reports (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SceneError {
    /// The representation names a generator no one registered.
    #[error("no generator is registered for `{generator}`")]
    NoGenerator {
        /// The path the schema gives the representation, such as `wall::body`.
        generator: String,
    },
    /// The generator could not compute the element's solids.
    #[error("the generator failed: {0}")]
    Generator(#[from] GeneratorError),
    /// A solid of the element has a face that cannot be triangulated.
    #[error("solid {solid} cannot be tessellated: {source}")]
    Tessellation {
        /// Index of the solid among the element's solids.
        solid: usize,
        /// Why.
        source: TessellationError,
    },
    /// The element's meshes together have more vertices than 32-bit indices address.
    #[error("the meshes of the element's solids have too many vertices to merge")]
    TooManyVertices,
    /// The dependency graph refused the element's dependencies.
    #[error("the dependency graph refused the element's dependencies")]
    Dependencies,
}
