// Handlers take the signature the registry asks for, whatever they need of it.
#![allow(
    clippy::unnecessary_wraps,
    clippy::needless_pass_by_value,
    clippy::trivially_copy_pass_by_ref
)]

use core_txn::{ChangeEvent, Origin};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;

use super::*;

#[derive(Default)]
struct Counter {
    value: i64,
}

#[derive(Deserialize)]
struct Add {
    by: i64,
}

fn add(counter: &mut Counter, input: Add, _origin: &Origin) -> Result<Outcome<i64>, ApiError> {
    counter.value += input.by;
    Ok(Outcome {
        result: counter.value,
        event: Some(ChangeEvent::default()),
    })
}

fn read(counter: &Counter, (): ()) -> Result<i64, ApiError> {
    Ok(counter.value)
}

fn fail(_: &Counter, (): ()) -> Result<i64, ApiError> {
    Err(ApiError::UnknownName("inner".to_owned()))
}

struct Unencodable;

impl Serialize for Unencodable {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("never encodes"))
    }
}

fn unencodable(_: &Counter, (): ()) -> Result<Unencodable, ApiError> {
    Ok(Unencodable)
}

const V1: Version = Version::new(1, 0, 0);

fn sig() -> Signature {
    Signature {
        params: serde_json::json!({}),
        result: serde_json::json!({}),
    }
}

#[test]
fn a_command_decodes_its_input_and_encodes_its_output() {
    let descriptor = Descriptor::command("counter.add", V1, "Adds to the counter.", sig(), add);
    let mut counter = Counter::default();
    let outcome = descriptor.call(&mut counter, json!({ "by": 3 }), &Origin::Ui);
    assert_eq!(
        outcome,
        Ok(Outcome {
            result: json!(3),
            event: Some(ChangeEvent::default()),
        })
    );
    assert_eq!(counter.value, 3);
    assert_eq!(descriptor.kind(), Kind::Command);
    assert_eq!(descriptor.name(), "counter.add");
    assert_eq!(descriptor.version(), V1);
    assert_eq!(descriptor.summary(), "Adds to the counter.");
}

#[test]
fn a_query_returns_no_event() {
    let descriptor = Descriptor::query("counter.read", V1, "Reads the counter.", sig(), read);
    let mut counter = Counter { value: 7 };
    let outcome = descriptor.call(&mut counter, Value::Null, &Origin::Ui);
    assert_eq!(
        outcome,
        Ok(Outcome {
            result: json!(7),
            event: None,
        })
    );
    assert_eq!(descriptor.kind(), Kind::Query);
}

#[test]
fn bad_parameters_are_rejected_before_the_handler_runs() {
    let descriptor = Descriptor::command("counter.add", V1, "Adds to the counter.", sig(), add);
    let mut counter = Counter::default();
    for params in [json!({}), json!({ "by": "two" }), json!("by")] {
        let error = descriptor.call(&mut counter, params, &Origin::Ui);
        assert!(
            matches!(error, Err(ApiError::InvalidParams { .. })),
            "{error:?}"
        );
    }
    assert_eq!(counter.value, 0);
}

#[test]
fn an_invalid_parameters_message_never_quotes_the_parameters() {
    let descriptor = Descriptor::command("counter.add", V1, "Adds to the counter.", sig(), add);
    let Err(error) = descriptor.call(
        &mut Counter::default(),
        json!({ "by": "secret" }),
        &Origin::Ui,
    ) else {
        panic!("a string is not a number");
    };
    assert_eq!(error.to_string(), "invalid parameters");
    let ApiError::InvalidParams { detail } = error else {
        panic!("{error:?}");
    };
    assert!(
        detail.contains("secret"),
        "the caller sees what was wrong: {detail}"
    );
}

#[test]
fn handler_errors_pass_through() {
    let descriptor = Descriptor::query("counter.fail", V1, "Fails.", sig(), fail);
    let error = descriptor.call(&mut Counter::default(), Value::Null, &Origin::Ui);
    assert_eq!(error, Err(ApiError::UnknownName("inner".to_owned())));
}

#[test]
fn a_result_that_cannot_be_encoded_is_an_internal_error() {
    let descriptor = Descriptor::query("counter.broken", V1, "Breaks.", sig(), unencodable);
    let error = descriptor.call(&mut Counter::default(), Value::Null, &Origin::Ui);
    assert_eq!(error, Err(ApiError::Internal));
}
