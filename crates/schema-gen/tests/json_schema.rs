//! JSON Schema generation (API-02): the sample snapshot, the published core schema, and both
//! documents checked against the draft they declare.

// Tests read fixtures and write snapshots, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

use schema_gen::{DRAFT, JsonSchemaOptions, generate_json_schema};
use serde_json::{Value, json};

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sample_dir() -> PathBuf {
    crate_dir().join("tests/fixtures/sample")
}

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

/// Compares `text` with the committed file; `UPDATE_SNAPSHOTS=1` rewrites the file first.
fn assert_committed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(path, text)?;
    }
    let expected = std::fs::read_to_string(path)?;
    assert!(
        text == expected,
        "generated JSON Schema differs from {}; rerun with UPDATE_SNAPSHOTS=1 and review the diff",
        path.display()
    );
    Ok(())
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
}

/// The published schema of the core namespace is regenerated, so CI fails when it is stale.
#[test]
fn published_core_schema_is_up_to_date() {
    let text = generate(&core_dir(), &core_options()).unwrap();
    assert_committed(&core_dir().join("api.schema.json"), &text).unwrap();
}

#[test]
fn documents_validate_against_their_draft() {
    assert_valid_for_its_draft(&generate(&sample_dir(), &sample_options()).unwrap()).unwrap();
    assert_valid_for_its_draft(&generate(&core_dir(), &core_options()).unwrap()).unwrap();
}

#[test]
fn describes_the_namespace() {
    let document: Value =
        serde_json::from_str(&generate(&sample_dir(), &sample_options()).unwrap()).unwrap();
    assert_eq!(document["$id"], "urn:bimcraft:schema:sample");
    assert_eq!(
        document["description"],
        "Objects of the `core` schema, version 2."
    );
    assert_eq!(document["$defs"]["ObjectId"], sample_options().object_id);
}

#[test]
fn same_input_same_output() {
    assert_eq!(
        generate(&sample_dir(), &sample_options()).unwrap(),
        generate(&sample_dir(), &sample_options()).unwrap()
    );
}

#[test]
fn reruns_on_any_change_in_the_namespace() {
    let generated = generate_json_schema(&sample_dir(), &sample_options()).unwrap();
    assert_eq!(generated.inputs, [sample_dir()]);
}

#[test]
fn reports_unreadable_namespaces() {
    let errors = generate_json_schema(&crate_dir().join("missing"), &sample_options()).unwrap_err();
    assert_eq!(errors.len(), 1);
}
