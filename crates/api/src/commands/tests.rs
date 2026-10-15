use std::collections::BTreeSet;

use core_txn::{ChangeEvent, CommandError, ModelCommand, Origin};
use serde_json::{Value, json};

use crate::fixtures::{create_wall, id, one_of_each, session, set, snapshot, text};
use crate::{ApiError, Kind, Registry, Session};

fn registry() -> Registry<Session> {
    Session::registry().unwrap()
}

/// The API name of each variant. The match is exhaustive, so a new variant does not compile
/// until it is given a name here, and the test below then requires it to be registered.
fn api_name(command: &ModelCommand) -> &'static str {
    match command {
        ModelCommand::Create { .. } => "model.create",
        ModelCommand::Set { .. } => "model.set",
        ModelCommand::Delete { .. } => "model.delete",
        ModelCommand::Relate { .. } => "model.relate",
        ModelCommand::AddParticipant { .. } => "model.add_participant",
        ModelCommand::RemoveParticipant { .. } => "model.remove_participant",
        ModelCommand::SetProperty { .. } => "model.set_property",
        ModelCommand::AttachPropertySet { .. } => "model.attach_property_set",
        ModelCommand::DetachPropertySet { .. } => "model.detach_property_set",
        ModelCommand::CreateComponent { .. } => "model.create_component",
        ModelCommand::RemoveComponent { .. } => "model.remove_component",
        ModelCommand::CreateType { .. } => "model.create_type",
        ModelCommand::AssignType { .. } => "model.assign_type",
        ModelCommand::Batch { .. } => "model.batch",
    }
}

/// The command's serde form without its tag: the parameters a caller sends.
fn params(command: &ModelCommand) -> Value {
    let mut value = serde_json::to_value(command).unwrap();
    value.as_object_mut().unwrap().remove("command");
    value
}

fn call(session: &mut Session, name: &str, params: Value) -> Result<crate::Outcome, ApiError> {
    registry().dispatch(session, name, params, &Origin::Ui)
}

#[test]
fn every_model_command_is_reachable_by_name() {
    let registry = registry();
    let mut session = session();
    let commands = one_of_each();
    let names: BTreeSet<_> = commands.iter().map(api_name).collect();
    assert_eq!(names.len(), 14, "one_of_each covers every variant");
    for command in commands {
        let name = api_name(&command);
        assert_eq!(
            registry.get(name).map(crate::Descriptor::kind),
            Ok(Kind::Command)
        );
        let outcome = registry.dispatch(&mut session, name, params(&command), &Origin::Ui);
        assert!(outcome.is_ok(), "{name}: {outcome:?}");
        assert_eq!(session.document().commands().last(), Some(&command));
    }
}

#[test]
fn a_command_returns_its_change_event() {
    let mut session = session();
    let outcome = call(&mut session, "model.create", params(&create_wall(1))).unwrap();
    assert_eq!(outcome.result, Value::Null);
    let created = ChangeEvent {
        created: [id(1)].into(),
        ..ChangeEvent::default()
    };
    assert_eq!(outcome.event, Some(created));
}

#[test]
fn bad_payloads_are_typed_errors_and_change_nothing() {
    let mut session = session();
    call(&mut session, "model.create", params(&create_wall(1))).unwrap();
    let before = snapshot(&session);
    let wall = "00000000000000000000000000000001";
    let bad = [
        ("wrong type", "model.delete", json!({ "id": 1 })),
        (
            "missing field",
            "model.create",
            json!({ "id": wall, "values": [] }),
        ),
        (
            "unknown value variant",
            "model.set",
            json!({ "id": wall, "attribute": "name", "value": { "Txt": "W" } }),
        ),
        (
            "command field",
            "model.delete",
            json!({ "id": wall, "command": "create" }),
        ),
        (
            "optional field under a wrong name",
            "model.set",
            json!({ "id": wall, "attribute": "name", "values": { "Text": "W" } }),
        ),
        (
            "unknown field in a batch",
            "model.batch",
            json!({ "commands": [{ "command": "delete", "id": wall, "force": true }] }),
        ),
        (
            "unknown field in a nested batch",
            "model.batch",
            json!({ "commands": [{ "command": "batch", "commands": [
                { "command": "set", "id": wall, "attribute": "name", "values": null },
            ] }] }),
        ),
        (
            "untagged command in a batch",
            "model.batch",
            json!({ "commands": [5, { "id": wall }] }),
        ),
        ("array", "model.delete", json!([wall])),
        ("string", "model.delete", json!(wall)),
        ("null", "model.delete", Value::Null),
    ];
    for (case, name, payload) in bad {
        let error = call(&mut session, name, payload);
        assert!(
            matches!(error, Err(ApiError::InvalidParams { .. })),
            "{case}: {error:?}"
        );
        assert_eq!(snapshot(&session), before, "{case}");
    }
}

#[test]
fn an_unknown_command_is_a_typed_error() {
    let mut session = session();
    let error = call(&mut session, "model.explode", json!({}));
    assert_eq!(
        error,
        Err(ApiError::UnknownName("model.explode".to_owned()))
    );
}

#[test]
fn a_rejected_command_changes_nothing() {
    let mut session = session();
    let before = snapshot(&session);
    let error = call(
        &mut session,
        "model.set",
        params(&set(9, "name", Some(text("W")))),
    );
    assert!(
        matches!(error, Err(ApiError::Rejected(CommandError::Model(_)))),
        "{error:?}"
    );
    assert_eq!(snapshot(&session), before);
}

#[test]
fn the_callers_origin_is_recorded_in_the_history() {
    let mut session = session();
    let plugin = Origin::Plugin {
        name: "walls".to_owned(),
    };
    registry()
        .dispatch(
            &mut session,
            "model.create",
            params(&create_wall(1)),
            &plugin,
        )
        .unwrap();
    let entries: Vec<_> = session.document().history().collect();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].command, "create");
    assert_eq!(entries[0].origin, &plugin);
}

#[test]
fn undo_and_redo_return_their_change_events() {
    let mut session = session();
    assert_eq!(
        call(&mut session, "history.undo", json!({})),
        Err(ApiError::NothingToUndo)
    );
    assert_eq!(
        call(&mut session, "history.redo", json!({})),
        Err(ApiError::NothingToRedo)
    );

    call(&mut session, "model.create", params(&create_wall(1))).unwrap();
    let undone = call(&mut session, "history.undo", json!({})).unwrap();
    assert_eq!(
        undone.event.map(|event| event.deleted),
        Some([id(1)].into())
    );
    assert!(!session.document().model().contains(id(1)));

    let redone = call(&mut session, "history.redo", json!({})).unwrap();
    assert_eq!(
        redone.event.map(|event| event.created),
        Some([id(1)].into())
    );
    assert!(session.document().model().contains(id(1)));
}

#[test]
fn undo_and_redo_take_an_empty_object() {
    let mut session = session();
    call(&mut session, "model.create", params(&create_wall(1))).unwrap();
    let before = snapshot(&session);
    for name in ["history.undo", "history.redo"] {
        for payload in [json!({ "steps": 2 }), Value::Null] {
            let error = call(&mut session, name, payload);
            assert!(
                matches!(error, Err(ApiError::InvalidParams { .. })),
                "{name}: {error:?}"
            );
        }
    }
    assert_eq!(snapshot(&session), before);
}

#[test]
fn model_errors_keep_their_message() {
    let mut session = session();
    let Err(error) = call(
        &mut session,
        "model.delete",
        params(&ModelCommand::Delete { id: id(9) }),
    ) else {
        panic!("nothing to delete");
    };
    assert!(
        !error.to_string().contains("0000000"),
        "no ids in messages: {error}"
    );
    assert_eq!(ApiError::NothingToUndo.to_string(), "nothing to undo");
    assert_eq!(ApiError::NothingToRedo.to_string(), "nothing to redo");
}

#[test]
fn a_batch_still_takes_its_commands_with_their_tags() {
    let mut session = session();
    let batch = json!({ "commands": [
        serde_json::to_value(create_wall(1)).unwrap(),
        { "command": "set", "id": format!("{:032x}", 1), "attribute": "name" },
    ] });
    assert!(call(&mut session, "model.batch", batch).is_ok());
    let unknown = json!({ "commands": [{ "command": "explode" }] });
    let error = call(&mut session, "model.batch", unknown);
    assert!(
        matches!(error, Err(ApiError::InvalidParams { .. })),
        "{error:?}"
    );
}
