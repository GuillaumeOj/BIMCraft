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
mod registry;
mod session;
mod version;

pub use descriptor::{Descriptor, Kind, Outcome};
pub use error::{ApiError, RegistryError};
pub use registry::Registry;
pub use session::Session;
pub use version::Version;

#[cfg(test)]
mod fixtures;
