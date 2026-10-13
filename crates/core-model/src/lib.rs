//! The `BIMCraft` model kernel: entities, attribute tables, relationships, IDs and units (spec §5).
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod id;
pub mod units;

#[cfg(any(test, feature = "test-support"))]
pub use id::SeededIdSource;
pub use id::{EntityId, GlobalIdError, IdSource};
