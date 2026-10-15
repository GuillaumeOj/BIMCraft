//! The command and query API: every user-visible operation is a named, versioned command or
//! query with typed input and output (API-01), registered in a [`Registry`] that every transport
//! dispatches through.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod commands;
mod descriptor;
mod error;
pub mod jsonrpc;
mod queries;
mod registry;
pub mod schema;
mod session;
mod version;

pub use descriptor::{Descriptor, Kind, Outcome};
pub use error::{ApiError, RegistryError};
pub use queries::{DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE};
pub use registry::Registry;
pub use schema::{Signature, protocol_schema};
pub use session::Session;
pub use version::Version;

#[cfg(test)]
mod fixtures;

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
