//! Commands and transactions: every change to a `BIMCraft` model is a serialisable [`Command`],
//! so history, macros, replay and collaboration share one mechanism (CORE-11), and runs as an
//! atomic [`Transaction`] that a [`Document`]'s history can undo and redo (CORE-06). Each
//! transaction, undo and redo emits a [`ChangeEvent`] listing the objects it changed (CORE-07).
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod command;
mod event;
mod history;
mod transaction;

pub use command::{Command, CommandError, ModelCommand, ReplayError, replay};
pub use event::ChangeEvent;
pub use history::{Document, HistoryError};
pub use transaction::Transaction;

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
