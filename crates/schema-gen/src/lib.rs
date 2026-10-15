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
//! [`generate_json_schema`] gives the JSON Schema of the same data (API-02); the core one is
//! published as `schema/core/api.schema.json`.
//!
//! [`generate_ifc_mapping`] gives the table of how each entity maps onto IFC 4.3 (CORE-10); the
//! core one is published as `docs/dev/ifc-mapping.md`.
//!
//! Generation is deterministic: the same definitions always give byte-identical code.
//!
//! [`load_history`] checks a namespace's version history (`snapshots/`, `migrations/`) against
//! its definitions and resolves the migrations between versions (PERS-01).

// Build-time only (not in scripts/wasm-crates.txt): reading definition files from disk is this
// crate's job, so it opts out of the ARCH-02 forbidden-API lints that bind the core crates.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

mod definition;
mod error;
mod history;
mod ifc_table;
mod json_schema;
mod layout;
mod load;
mod parse;
mod rust;
mod snapshot;

use std::path::{Path, PathBuf};

pub use error::{Error, ErrorKind};
pub use history::{History, MAX_SNAPSHOT_SIZE, check_history, load_history, snapshot_path};
pub use ifc_table::emit_ifc_mapping;
pub use json_schema::{DRAFT, JsonSchemaOptions, emit_json_schema};
pub use load::load_dir;
pub use parse::{FORMAT, MAX_FILE_SIZE, Source, parse_sources};
pub use rust::{RustOptions, emit_rust};
pub use snapshot::{parse_snapshot, snapshot};

/// Generated code and the files it was generated from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Generated {
    /// The generated source code.
    pub code: String,
    /// What to pass to `cargo::rerun-if-changed`: the namespace directory, which Cargo scans
    /// recursively, so editing, adding or removing any definition reruns the build script.
    pub inputs: Vec<PathBuf>,
}

/// Reads the namespace in `dir` and generates its JSON Schema (API-02).
///
/// # Errors
///
/// Returns every [`Error`] found, as [`generate_rust`] does.
pub fn generate_json_schema(
    dir: &Path,
    options: &JsonSchemaOptions,
) -> Result<Generated, Vec<Error>> {
    generate(dir, options, emit_json_schema)
}

/// Reads the namespace in `dir` and generates its IFC mapping table, as Markdown (CORE-10).
///
/// # Errors
///
/// Returns every [`Error`] found, as [`generate_rust`] does; among them, an entity with no
/// `[ifc]` section.
pub fn generate_ifc_mapping(dir: &Path) -> Result<Generated, Vec<Error>> {
    generate(dir, &(), |namespace, ()| Ok(emit_ifc_mapping(namespace)))
}

/// Reads the namespace in `dir` and generates its Rust types.
///
/// # Errors
///
/// Returns every [`Error`] found: unreadable files, invalid TOML, definitions that do not
/// follow the format, and validation failures.
pub fn generate_rust(dir: &Path, options: &RustOptions) -> Result<Generated, Vec<Error>> {
    generate(dir, options, emit_rust)
}

fn generate<O>(
    dir: &Path,
    options: &O,
    emit: fn(&schema::Namespace, &O) -> Result<String, Vec<Error>>,
) -> Result<Generated, Vec<Error>> {
    let sources = load_dir(dir).map_err(|error| vec![error])?;
    let namespace = parse_sources(&sources)?;
    let code = emit(&namespace, options)?;
    Ok(Generated {
        code,
        inputs: vec![dir.to_owned()],
    })
}
