//! Build-time EXPRESS tooling (ISO 10303-11): parses a schema, resolves it into a model of its
//! types and entities, and generates Rust from that model (spec §11, ADR 0009).
//!
//! `interop-ifc` uses it from its `build.rs` to turn the official IFC 4.3 schema into entity
//! classes that decode STEP records.
//!
//! Every input is treated as hostile: the source size is bounded by [`MAX_INPUT_LEN`] and the
//! nesting of constructs by [`MAX_DEPTH`], and every problem is returned as an [`Error`].
//! Generation is deterministic: the same schema always gives byte-identical code.

// Build-time only (not in scripts/wasm-crates.txt): reading the schema file is this crate's job,
// so it opts out of the ARCH-02 forbidden-API lints that bind the core crates.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

pub mod ast;
mod error;
mod lexer;
pub mod model;
mod parser;
mod rust;

pub use error::{Error, ErrorKind, Position};
pub use model::{Model, resolve};
pub use parser::parse;
pub use rust::{RustOptions, emit_rust};

/// Largest schema accepted, in bytes: ten times the IFC 4.3 schema. Every token is held in
/// memory, at most one per byte, so this also bounds the tokens to a few hundred megabytes.
pub const MAX_INPUT_LEN: usize = 4 * 1024 * 1024;

/// Deepest nesting of expressions, statements, types and declarations.
pub const MAX_DEPTH: usize = 128;

/// Generated code and the files it was generated from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The generated source code.
    pub code: String,
    /// What to pass to `cargo::rerun-if-changed`.
    pub inputs: Vec<PathBuf>,
}

/// Reads, parses and resolves the schema in `path`, and generates its Rust types.
///
/// # Errors
///
/// Returns every [`Error`] found: an unreadable file, a syntax error, resolution problems, and
/// names that cannot become Rust identifiers.
pub fn generate_rust(path: &Path, options: &RustOptions) -> Result<Generated, Vec<Error>> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        let message = format!("cannot read {}: {error}", path.display());
        vec![Error::new(ErrorKind::Io(message))]
    })?;
    let schema = parse(&source).map_err(|error| vec![error])?;
    let model = resolve(&schema)?;
    let code = emit_rust(&model, options)?;
    Ok(Generated {
        code,
        inputs: vec![path.to_owned()],
    })
}

/// Property-test settings shared by the crate. A fixed seed keeps runs reproducible; with no
/// persistence, no regression files are written next to the sources.
#[cfg(test)]
fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x0E59_4E55),
        failure_persistence: None,
        ..proptest::test_runner::Config::default()
    }
}
