//! Build-time schema tooling (spec §5.1, ADR 0002): reads the TOML definition files of a schema
//! namespace into the canonical [`schema::Namespace`], validates it, and generates code from it.
//!
//! A crate that owns a schema calls [`generate_rust`] from its `build.rs`, writes the code into
//! `OUT_DIR`, prints `cargo::rerun-if-changed` for every returned input, and `include!`s the file:
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let options = schema_gen::RustOptions { id_type: "crate::EntityId".into() };
//! let generated = schema_gen::generate_rust("schema/core".as_ref(), &options)
//!     .map_err(|errors| errors.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))?;
//! for input in &generated.inputs {
//!     println!("cargo::rerun-if-changed={}", input.display());
//! }
//! let out = std::path::PathBuf::from(std::env::var("OUT_DIR")?).join("schema.rs");
//! std::fs::write(out, generated.code)?;
//! # Ok(())
//! # }
//! ```
//!
//! Generation is deterministic: the same definitions always give byte-identical code.

// Build-time only (not in scripts/wasm-crates.txt): reading definition files from disk is this
// crate's job, so it opts out of the ARCH-02 forbidden-API lints that bind the core crates.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

mod definition;
mod error;
mod load;
mod parse;
mod rust;

use std::path::{Path, PathBuf};

pub use error::{Error, ErrorKind};
pub use load::load_dir;
pub use parse::{FORMAT, MAX_FILE_SIZE, Source, parse_sources};
pub use rust::{RustOptions, emit_rust};

/// Generated code and the files it was generated from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The generated source code.
    pub code: String,
    /// What to pass to `cargo::rerun-if-changed`: the namespace directory, which Cargo scans
    /// recursively, so editing, adding or removing any definition reruns the build script.
    pub inputs: Vec<PathBuf>,
}

/// Reads the namespace in `dir` and generates its Rust types.
///
/// # Errors
///
/// Returns every [`Error`] found: unreadable files, invalid TOML, definitions that do not
/// follow the format, and validation failures.
pub fn generate_rust(dir: &Path, options: &RustOptions) -> Result<Generated, Vec<Error>> {
    let sources = load_dir(dir).map_err(|error| vec![error])?;
    let namespace = parse_sources(&sources)?;
    let code = emit_rust(&namespace, options)?;
    Ok(Generated {
        code,
        inputs: vec![dir.to_owned()],
    })
}
