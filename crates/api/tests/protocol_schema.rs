//! The published protocol schema (API-02) is regenerated from the registry, so CI fails when the
//! committed file is stale.

// The test reads and writes the committed file, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use api::{Session, protocol_schema};
use serde_json::Value;

fn repository() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

/// Compares generated `text` with the committed file at `path`; `UPDATE_SNAPSHOTS=1` rewrites
/// the file first, for review in the diff.
fn assert_committed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(path, text)?;
    }
    let expected = std::fs::read_to_string(path)?;
    assert!(
        text == expected,
        "generated output differs from {}; rerun with UPDATE_SNAPSHOTS=1 and review the diff",
        path.display()
    );
    Ok(())
}

#[test]
fn published_protocol_schema_is_up_to_date() -> Result<(), Box<dyn std::error::Error>> {
    // The entity schema is the one schema-gen publishes, and its own test keeps it fresh.
    let entities: Value = serde_json::from_str(&std::fs::read_to_string(
        repository().join("schema/core/api.schema.json"),
    )?)?;
    let document = protocol_schema(&Session::registry()?, entities);
    let mut text = serde_json::to_string_pretty(&document)?;
    text.push('\n');
    assert_committed(&repository().join("schema/api/protocol.schema.json"), &text)?;
    Ok(())
}
