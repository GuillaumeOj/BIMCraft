//! Generates the IFC 4.3 entity classes of `interop_ifc::ifc` from the vendored EXPRESS schema
//! (ADR 0009).

// A build script runs natively at build time: writing to OUT_DIR is its job, and Cargo reads its
// instructions from standard output.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::print_stdout,
    reason = "a build script writes OUT_DIR and talks to Cargo through standard output"
)]

use std::error::Error;
use std::path::{Path, PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let options = express::RustOptions {
        runtime: "crate::ifc::decode".into(),
        doc_url: Some(
            "https://standards.buildingsmart.org/IFC/RELEASE/IFC4_3/HTML/lexical/{name}.htm".into(),
        ),
    };
    let schema = Path::new("schema/IFC4X3_ADD2.exp");
    let generated = express::generate_rust(schema, &options).map_err(|errors| {
        let messages: Vec<String> = errors
            .iter()
            .map(|error| format!("{}: {error}", schema.display()))
            .collect();
        messages.join("\n")
    })?;
    for input in &generated.inputs {
        println!("cargo::rerun-if-changed={}", input.display());
    }
    let out = PathBuf::from(std::env::var("OUT_DIR")?).join("ifc4x3.rs");
    std::fs::write(out, generated.code)?;
    Ok(())
}
