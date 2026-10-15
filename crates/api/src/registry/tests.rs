// Handlers take the signature the registry asks for, whatever they need of it.
#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::trivially_copy_pass_by_ref
)]

use core_txn::Origin;
use serde_json::json;

use super::*;

fn version(ctx: &u32, (): ()) -> Result<u32, ApiError> {
    Ok(*ctx)
}

fn one(_: &u32, (): ()) -> Result<u32, ApiError> {
    Ok(1)
}

fn two(_: &u32, (): ()) -> Result<u32, ApiError> {
    Ok(2)
}

fn set(ctx: &mut u32, value: u32, _: &Origin) -> Result<Outcome<()>, ApiError> {
    *ctx = value;
    Ok(Outcome {
        result: (),
        event: None,
    })
}

const V1: Version = Version::new(1, 0, 0);

fn sig() -> crate::Signature {
    crate::Signature {
        params: serde_json::json!({}),
        result: serde_json::json!({}),
    }
}
const V2: Version = Version::new(2, 0, 0);

fn registry() -> Registry<u32> {
    let mut registry = Registry::new();
    for descriptor in [
        Descriptor::query("test.which", V1, "Returns 1.", sig(), one),
        Descriptor::query("test.which", V2, "Returns 2.", sig(), two),
        Descriptor::query("test.read", V1, "Reads.", sig(), version),
        Descriptor::command("test.set", V1, "Sets.", sig(), set),
    ] {
        assert_eq!(registry.register(descriptor), Ok(()));
    }
    registry
}

#[test]
fn the_same_name_and_version_cannot_be_registered_twice() {
    let mut registry = registry();
    let again = Descriptor::query("test.which", V1, "Returns 1 again.", sig(), one);
    assert_eq!(
        registry.register(again),
        Err(RegistryError::Duplicate {
            name: "test.which",
            version: V1,
        })
    );
    assert_eq!(
        registry
            .get_version("test.which", V1)
            .map(Descriptor::summary),
        Ok("Returns 1.")
    );
}

#[test]
fn lookup_by_name_returns_the_highest_version() {
    let registry = registry();
    assert_eq!(registry.get("test.which").map(Descriptor::version), Ok(V2));
    assert_eq!(
        registry
            .get_version("test.which", V1)
            .map(Descriptor::version),
        Ok(V1)
    );
}

#[test]
fn unknown_names_and_versions_are_typed_errors() {
    let registry = registry();
    assert!(matches!(
        registry.get("test.missing"),
        Err(ApiError::UnknownName(name)) if name == "test.missing"
    ));
    assert!(matches!(
        registry.get_version("test.missing", V1),
        Err(ApiError::UnknownName(name)) if name == "test.missing"
    ));
    assert!(matches!(
        registry.get_version("test.read", V2),
        Err(ApiError::UnknownVersion { name, version: V2 }) if name == "test.read"
    ));
}

#[test]
fn iteration_is_sorted_by_name_then_version() {
    let registry = registry();
    let listed: Vec<_> = registry.iter().map(|d| (d.name(), d.version())).collect();
    assert_eq!(
        listed,
        [
            ("test.read", V1),
            ("test.set", V1),
            ("test.which", V1),
            ("test.which", V2),
        ]
    );
}

#[test]
fn dispatch_runs_the_named_descriptor() {
    let registry = registry();
    let mut ctx = 0;
    let set = registry.dispatch(&mut ctx, "test.set", json!(5), &Origin::Ui);
    assert_eq!(set.map(|outcome| outcome.result), Ok(json!(null)));
    let read = registry.dispatch(&mut ctx, "test.read", json!(null), &Origin::Ui);
    assert_eq!(read.map(|outcome| outcome.result), Ok(json!(5)));
    let missing = registry.dispatch(&mut ctx, "test.missing", json!(null), &Origin::Ui);
    assert!(matches!(missing, Err(ApiError::UnknownName(_))));
}

#[test]
fn names_are_dotted_snake_case() {
    for valid in ["model.set", "model.set_property", "a.b.c", "io2.read_v2"] {
        assert!(is_valid_name(valid), "{valid}");
    }
    for invalid in [
        "",
        "model",
        "model.",
        ".set",
        "model..set",
        "Model.set",
        "model.Set",
        "model.2set",
        "model.set-property",
        "rpc.discover",
        "model._set",
        "modèle.set",
    ] {
        assert!(!is_valid_name(invalid), "{invalid}");
    }
}

#[test]
fn an_invalid_name_is_not_registered() {
    let mut registry = Registry::new();
    let descriptor = Descriptor::query("Bad name", V1, "Bad.", sig(), one);
    assert_eq!(
        registry.register(descriptor),
        Err(RegistryError::InvalidName("Bad name"))
    );
    assert_eq!(registry.iter().count(), 0);
}

#[test]
fn errors_name_what_failed_without_parameters() {
    assert_eq!(
        RegistryError::Duplicate {
            name: "a.b",
            version: V1
        }
        .to_string(),
        "a.b 1.0.0 is already registered"
    );
    assert_eq!(
        RegistryError::InvalidName("x").to_string(),
        "\"x\" is not a valid command or query name"
    );
    assert_eq!(
        ApiError::UnknownName("a.b".to_owned()).to_string(),
        "unknown command or query \"a.b\""
    );
    assert_eq!(
        ApiError::UnknownVersion {
            name: "a.b".to_owned(),
            version: V2
        }
        .to_string(),
        "a.b has no version 2.0.0"
    );
    assert_eq!(ApiError::Internal.to_string(), "internal error");
}
