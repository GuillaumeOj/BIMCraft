//! JSON Schema generation (API-02): the sample snapshot and the published core schema, both
//! checked against the draft they declare. Determinism and inputs are shared with the Rust
//! emitter and tested in `generate.rs`.

// Tests read fixtures and write snapshots, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

use schema_gen::{DRAFT, JsonSchemaOptions, generate_json_schema};
use serde_json::{Value, json};

mod common;

use common::{assert_committed, crate_dir, sample_dir};

fn core_dir() -> PathBuf {
    crate_dir().join("../../schema/core")
}

/// The sample's identifiers are the `u64` of `generated_sample.rs`.
fn sample_options() -> JsonSchemaOptions {
    JsonSchemaOptions {
        id: "urn:bimcraft:schema:sample".into(),
        object_id: json!({ "type": "integer", "minimum": 0 }),
    }
}

/// Object identifiers have no serialized form of their own yet, so the published schema uses
/// their only text form, the 22-character IFC `GlobalId` (CORE-01), whose first character
/// carries the top two of the 128 bits.
fn core_options() -> JsonSchemaOptions {
    JsonSchemaOptions {
        id: "urn:bimcraft:schema:core".into(),
        object_id: json!({
            "type": "string",
            "pattern": "^[0-3][0-9A-Za-z_$]{21}$",
            "description": "The stable 128-bit identifier of an object, as an IFC GlobalId.",
        }),
    }
}

fn generate(dir: &Path, options: &JsonSchemaOptions) -> Result<String, Vec<schema_gen::Error>> {
    generate_json_schema(dir, options).map(|generated| generated.code)
}

fn assert_valid_for_its_draft(text: &str) -> serde_json::Result<()> {
    let document: Value = serde_json::from_str(text)?;
    assert_eq!(document["$schema"], DRAFT);
    let result = jsonschema::meta::validate(&document).map_err(|error| error.to_string());
    assert_eq!(result, Ok(()), "not a valid JSON Schema");
    Ok(())
}

/// The emitted schema is reviewed through this file.
#[test]
fn matches_the_snapshot() {
    let text = generate(&sample_dir(), &sample_options()).unwrap();
    assert_committed(
        &crate_dir().join("tests/snapshots/sample.schema.json"),
        &text,
    )
    .unwrap();
    assert_valid_for_its_draft(&text).unwrap();
}

/// The published schema of the core namespace is regenerated, so CI fails when it is stale.
#[test]
fn published_core_schema_is_up_to_date() {
    let text = generate(&core_dir(), &core_options()).unwrap();
    assert_committed(&core_dir().join("api.schema.json"), &text).unwrap();
    assert_valid_for_its_draft(&text).unwrap();
}

#[test]
fn describes_the_namespace() {
    let document: Value =
        serde_json::from_str(include_str!("snapshots/sample.schema.json")).unwrap();
    assert_eq!(document["$id"], "urn:bimcraft:schema:sample");
    assert_eq!(
        document["description"],
        "Objects of the `core` schema, version 2."
    );
    assert_eq!(document["$defs"]["ObjectId"], sample_options().object_id);
}
