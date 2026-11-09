//! Materials (MAT-01): a [`Material`] is a named grade with a [`BehaviourLaw`], which gives the
//! stress for a strain, and design-code [`Property`] values. Every value carries the [`Source`]
//! it comes from, a document and clause number; this crate never holds standards text (REP-05).
//!
//! [`Library::builtin`] holds the common European grades (MAT-02), and [`Library::extend`] adds
//! libraries from users and plugins in the same format.
//!
//! [`commands`] turns a material into a model command on the core schema's `Material` entity,
//! and [`read`] reads one back.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod annex;
mod law;
mod library;
mod material;
mod model;
mod property;
mod source;

pub use annex::{Factor, NationalAnnex};
pub use core_model::{Source, SourceError};
pub use law::{
    BehaviourLaw, ConcreteBilinear, ConcreteParabolaRectangle, Hardening, LawError, LawKind,
    LinearElastic, SteelBilinear, StrainLimits, TimberElasticBrittle,
};
pub use library::{
    LIBRARY_FORMAT, Library, LibraryError, MAX_LIBRARY_SIZE, MAX_MATERIALS, MAX_SOURCE_NAME,
    design_code_key,
};
pub use material::{Category, Material, MaterialError};
pub use model::{
    ENTITY, EditError, ORIGINAL_ENTITY, Original, ReadError, commands, delete, originals, read,
    reset_value, set_value,
};
pub use property::Property;
pub use source::{MAX_TEXT_LEN, Sourced};

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
