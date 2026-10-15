use core_txn::{Command, ModelCommand, Origin};
use proptest::prelude::*;
use serde_json::{Value, json};

use super::*;
use crate::fixtures::{any_command, one_of_each, session};
use crate::{Descriptor, Session, Version};

fn entities() -> Value {
    serde_json::from_str(include_str!("../../../../schema/core/api.schema.json")).unwrap()
}

fn document() -> Value {
    protocol_schema(&Session::registry().unwrap(), entities())
}

/// Checks instances against the schema at `pointer` in the protocol schema, references
/// resolved against the whole document.
struct Check(jsonschema::Validator);

impl Check {
    fn at(document: &Value, pointer: &str) -> Self {
        let mut wrapper = document.clone();
        wrapper["$ref"] = json!(format!("#{pointer}"));
        Self(jsonschema::validator_for(&wrapper).unwrap())
    }

    fn assert_valid(&self, instance: &Value) {
        let result = self.0.validate(instance).map_err(|error| error.to_string());
        assert_eq!(result, Ok(()), "{instance}");
    }
}

fn entry_pointer(kind: &str, name: &str, part: &str) -> String {
    format!("/$defs/{kind}/{name}/properties/{part}")
}

#[test]
fn the_schema_is_valid_for_the_draft_it_declares() {
    let document = document();
    assert_eq!(document["$schema"], DRAFT);
    assert_eq!(document["$id"], ID);
    let result = jsonschema::meta::validate(&document).map_err(|error| error.to_string());
    assert_eq!(result, Ok(()));
}

#[test]
fn every_command_and_query_appears_with_its_name_and_version() {
    let document = document();
    let registry = Session::registry().unwrap();
    for descriptor in registry.iter() {
        let kind = match descriptor.kind() {
            Kind::Command => "commands",
            Kind::Query => "queries",
        };
        let entry = &document["$defs"][kind][descriptor.name()];
        assert_eq!(entry["properties"]["name"]["const"], descriptor.name());
        assert_eq!(
            entry["properties"]["version"]["const"],
            descriptor.version().to_string()
        );
        assert_eq!(entry["description"], descriptor.summary());
    }
    let listed = ["commands", "queries"]
        .iter()
        .map(|kind| document["$defs"][kind].as_object().unwrap().len())
        .sum::<usize>();
    assert_eq!(listed, registry.iter().count());
}

#[test]
fn the_entity_schema_is_embedded_unchanged() {
    assert_eq!(document()["$defs"]["entities"], entities());
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    clippy::unnecessary_wraps,
    reason = "query handlers take this signature"
)]
fn unit(_: &(), (): ()) -> Result<(), crate::ApiError> {
    Ok(())
}

#[test]
fn older_versions_are_listed_under_their_version() {
    let signature = || Signature {
        params: json!({ "type": "null" }),
        result: none(),
    };
    let mut registry = Registry::new();
    for major in [1, 2] {
        let descriptor = Descriptor::query(
            "test.read",
            Version::new(major, 0, 0),
            "Reads.",
            signature(),
            unit,
        );
        registry.register(descriptor).unwrap();
    }
    let document = protocol_schema(&registry, json!({}));
    let queries = &document["$defs"]["queries"];
    assert_eq!(
        queries["test.read"]["properties"]["version"]["const"],
        "2.0.0"
    );
    assert_eq!(
        queries["test.read@1.0.0"]["properties"]["version"]["const"],
        "1.0.0"
    );
}

/// The command's serde form without its tag, as a caller sends it.
fn params(command: &ModelCommand) -> Value {
    let mut value = serde_json::to_value(command).unwrap();
    value.as_object_mut().unwrap().remove("command");
    value
}

#[test]
fn command_results_and_events_match_the_schema() {
    let document = document();
    let registry = Session::registry().unwrap();
    let event = Check::at(&document, "/$defs/ChangeEvent");
    let mut session = session();
    let mut calls: Vec<_> = one_of_each()
        .iter()
        .map(|command| (format!("model.{}", command.name()), params(command)))
        .collect();
    calls.push(("history.undo".to_owned(), json!({})));
    calls.push(("history.redo".to_owned(), json!({})));
    for (name, params) in calls {
        Check::at(&document, &entry_pointer("commands", &name, "params")).assert_valid(&params);
        let outcome = registry
            .dispatch(&mut session, &name, params, &Origin::Ui)
            .unwrap();
        Check::at(&document, &entry_pointer("commands", &name, "result"))
            .assert_valid(&outcome.result);
        event.assert_valid(&serde_json::to_value(outcome.event.unwrap()).unwrap());
    }
}

#[test]
fn query_parameters_and_results_match_the_schema() {
    let document = document();
    let registry = Session::registry().unwrap();
    let mut session = session();
    let plugin = Origin::Plugin {
        name: "p".to_owned(),
    };
    let ai = Origin::AiSession { id: "s".to_owned() };
    for (command, origin) in one_of_each()
        .into_iter()
        .zip([Origin::Ui, plugin, ai].iter().cycle())
    {
        session
            .document_mut()
            .execute(command, origin.clone())
            .unwrap();
    }
    session.document_mut().undo().unwrap();
    let wall = json!(format!("{:032x}", 3));
    let calls = [
        ("history.state", json!({})),
        ("model.get_object", json!({ "id": wall })),
        ("model.get_object", json!({ "id": format!("{:032x}", 21) })),
        (
            "model.list_objects",
            json!({ "entity": "Wall", "limit": 1 }),
        ),
        (
            "model.list_objects",
            json!({ "entity": "Wall", "after": wall, "limit": null }),
        ),
        ("model.property_sets", json!({ "id": wall })),
    ];
    for (name, params) in calls {
        Check::at(&document, &entry_pointer("queries", name, "params")).assert_valid(&params);
        let Ok(outcome) = registry.dispatch(&mut session, name, params, &Origin::Ui) else {
            continue;
        };
        Check::at(&document, &entry_pointer("queries", name, "result"))
            .assert_valid(&outcome.result);
    }
}

#[test]
fn the_schema_rejects_what_the_decoder_rejects() {
    let document = document();
    let delete = Check::at(
        &document,
        &entry_pointer("commands", "model.delete", "params"),
    );
    for bad in [
        json!({ "id": 1 }),
        json!({}),
        json!({ "id": "1", "extra": 1 }),
    ] {
        assert!(!delete.0.is_valid(&bad), "{bad}");
    }
    let set = Check::at(&document, &entry_pointer("commands", "model.set", "params"));
    let value = json!({ "id": format!("{:032x}", 1), "attribute": "a", "value": { "Txt": "W" } });
    assert!(!set.0.is_valid(&value));
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    /// The schemas are written by hand, so any serde form of a command must match them.
    #[test]
    fn every_serialised_command_matches_the_schema(command in any_command()) {
        let document = document();
        Check::at(&document, "/$defs/ModelCommand")
            .assert_valid(&serde_json::to_value(&command).unwrap());
        let name = format!("model.{}", command.name());
        Check::at(&document, &entry_pointer("commands", &name, "params"))
            .assert_valid(&params(&command));
    }
}

#[test]
fn every_value_and_origin_matches_the_schema() {
    let document = document();
    let value = Check::at(&document, "/$defs/AttributeValue");
    for instance in [
        json!({ "Bool": true }),
        json!({ "Integer": -3 }),
        json!({ "Real": 2.5 }),
        json!({ "Text": "W" }),
        json!({ "Ref": format!("{:032x}", 9) }),
        json!({ "List": [{ "List": [] }, { "Real": 1 }] }),
    ] {
        value.assert_valid(&instance);
    }
    let origin = Check::at(&document, "/$defs/Origin");
    for instance in [
        Origin::Ui,
        Origin::Plugin {
            name: "p".to_owned(),
        },
        Origin::AiSession { id: "s".to_owned() },
    ] {
        origin.assert_valid(&serde_json::to_value(instance).unwrap());
    }
}
