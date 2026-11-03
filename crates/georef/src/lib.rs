//! Georeferencing for `BIMCraft` (spec §7): where a project lies on Earth.
//!
//! A [`Georeference`] links the local engineering frame of the model to a projected coordinate
//! reference system, named by its [`EpsgCode`] (GEOREF-01): the projected coordinates of the
//! local origin (the base point), the grid rotation, the scale and an optional survey point.
//! [`MapConversion`] converts points between the two systems with the semantics of IFC 4.3
//! `IfcMapConversion` (GEOREF-02). Projected coordinates never enter the geometry kernel
//! (ADR 0001): modelling stays in the local frame.
//!
//! The georeference is stored on the project's single `Site` object; the [`site`] module reads
//! and writes it there. Decisions: `docs/dev/adr/0017-georeference-storage-and-ifc-mapping.md`.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and computes trigonometry with `libm`, so results are identical
//! on every target (ENG-04).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod conversion;
mod georeference;

pub use conversion::{MapConversion, XAxisError, rotation_from_x_axis};
pub use georeference::{
    EpsgCode, Field, Georeference, GeoreferenceError, LocalPoint, ProjectedPoint,
};

/// Property-test settings shared by the crate. A fixed seed keeps runs reproducible; with no
/// persistence, no regression files are written next to the sources.
#[cfg(test)]
fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}
