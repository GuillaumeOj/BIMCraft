//! The `BIMCraft` model kernel: entities, attribute tables, relationships, IDs and units (spec §5),
//! and the dependency graph that keeps derived data up to date (CORE-08).
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

pub mod derived;
mod id;
mod model;
mod store;
pub mod units;

#[cfg(any(test, feature = "test-support"))]
pub use id::SeededIdSource;
pub use id::{EntityId, GlobalIdError, IdSource};
pub use model::{Component, LibraryRef, Model, ModelError, ObjectState, Participation, Removal};
pub use store::{AttributeValue, RemovedEntity, Store, StoreError};

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
