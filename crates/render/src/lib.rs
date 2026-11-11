//! The `BIMCraft` renderer of the 3D viewport (spec §13, VP-04).
//!
//! Model coordinates are double precision (GEO-07), but GPUs compute in single precision, whose
//! grid is 6.25 cm wide at 10⁶ m from the origin. The renderer therefore never sends world
//! coordinates to the GPU: each frame, it computes on the CPU, in `f64`, the transform of every
//! object relative to the camera, and converts only that small result to `f32`
//! (`docs/dev/adr/0020-camera-relative-rendering.md`). A model 10⁷ m from the origin renders like
//! the same model at the origin.
//!
//! A [`MeshData`] holds `f32` vertices relative to an `f64` origin; [`MeshData::from_world`]
//! picks the centre of the bounding box so the local coordinates stay small. A [`Placement`]
//! puts a mesh in the world, and a [`Camera`] looks at it through a [`Projection`]. Each type
//! validates its input, since meshes come from model files: failures are a [`MeshError`], a
//! [`PlacementError`] or a [`CameraError`], whose messages never contain coordinates (SEC-03).
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and computes trigonometry with `libm` (ENG-04).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod camera;
mod mesh;
mod placement;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the GPU pipeline is its first caller (#68)")
)]
mod transform;
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the GPU pipeline uses the rest (#68)")
)]
mod vector;

pub use camera::{Camera, CameraError, Projection};
pub use mesh::{MAX_MESH_INDICES, MAX_MESH_VERTICES, MeshData, MeshError};
pub use placement::{Placement, PlacementError};

/// The largest magnitude of a world coordinate, in metres: of a mesh origin and its local
/// positions, a placement translation, and the eye, target and up vector of a camera. It is a
/// thousand times the size of the Earth and still leaves `f64` a precision better than a
/// micrometre, while keeping every product far from overflow.
pub const MAX_WORLD_COORDINATE: f64 = 1e9;

/// Whether `value` is a finite world coordinate within [`MAX_WORLD_COORDINATE`].
fn in_world(value: f64) -> bool {
    value.abs() <= MAX_WORLD_COORDINATE
}

/// The configuration of the property tests: fixed seed, no persistence, so runs are
/// deterministic (AGENTS.md §5).
#[cfg(test)]
fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        cases: 256,
        rng_seed: proptest::test_runner::RngSeed::Fixed(68),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}
