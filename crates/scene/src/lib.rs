//! The scene: the meshes of a model's elements, kept current incrementally (GEO-05, CORE-08).
//!
//! The viewport draws triangle meshes, and re-meshing the whole model after every command does
//! not scale (NFR-02). A [`Scene`] meshes each element once, through the [`Generator`] its
//! schema names for the representation a [`View`] picks (GEO-06), and keeps those meshes current
//! from the [`core_txn::ChangeEvent`] of each transaction, undo and redo (CORE-07): [`Scene::apply`] marks
//! stale the elements the event can have changed, and [`Scene::update`] re-meshes those and only
//! those, reporting them in a [`SceneUpdate`].
//!
//! What an element's mesh depends on is kept in a [`core_model::derived::DependencyGraph`]: the
//! element, its type, the relationships it takes part in, the objects its attributes refer to,
//! and the objects its generator reports reading in [`Generated::inputs`]. It is redeclared after
//! each generation, so it follows the model. Object-level granularity is all change events give;
//! ADR 0023 records the rules and the alternatives.
//!
//! Failures are per element: a generator that fails, a solid that cannot be tessellated or
//! meshes too large to merge put the element in [`SceneUpdate::failed`] with a [`SceneError`],
//! leave it without a mesh, and do not stop the others. Errors carry no coordinates or model
//! content (SEC-03). Iteration is ordered, so results are deterministic.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod error;
mod generator;
mod mesh;
mod scene;
mod view;

pub use error::SceneError;
pub use generator::{Generated, Generator, GeneratorError, Generators};
pub use mesh::ElementMesh;
pub use scene::{Scene, SceneUpdate};
pub use view::View;

#[cfg(test)]
mod tests;
