//! IFC import and export for `BIMCraft` (spec §11).
//!
//! The [`step`] module reads ISO 10303-21 (STEP Part 21) files, the text encoding of IFC, as a
//! stream of instances with bounded memory, so a hostile file cannot exhaust memory or crash the
//! importer.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02): it reads from any [`std::io::BufRead`], such as the bytes a
//! `platform::Storage` returns.

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

pub mod step;

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
