use core_model::StoreError;
use proptest::prelude::*;

use super::*;
use crate::fixtures::{
    PSET, Run, any_value, contain, create_wall, delete, id, model, model_command, one_of_each, set,
    text,
};

fn round_trip(command: &ModelCommand) -> ModelCommand {
    serde_json::from_str(&serde_json::to_string(command).unwrap()).unwrap()
}

#[test]
fn commands_serialise_as_tagged_objects() {
    let command = set(1, "name", Some(text("W1")));
    assert_eq!(
        serde_json::to_value(&command).unwrap(),
        serde_json::json!({
            "command": "set",
            "id": "00000000000000000000000000000001",
            "attribute": "name",
            "value": { "Text": "W1" },
        })
    );
}

#[test]
fn every_command_round_trips() {
    for command in one_of_each() {
        assert_eq!(round_trip(&command), command);
    }
}

#[test]
fn malformed_commands_do_not_deserialise() {
    for json in [
        r#"{"command":"explode"}"#,
        r#"{"command":"delete"}"#,
        r#"{"command":"delete","id":"1"}"#,
    ] {
        assert!(
            serde_json::from_str::<ModelCommand>(json).is_err(),
            "{json}"
        );
    }
}

#[test]
fn commands_have_stable_names() {
    let names: Vec<_> = one_of_each().iter().map(Command::name).collect();
    assert_eq!(
        names,
        [
            "create",
            "create",
            "create",
            "set",
            "relate",
            "add_participant",
            "remove_participant",
            "attach_property_set",
            "set_property",
            "detach_property_set",
            "create_component",
            "create_type",
            "assign_type",
            "delete",
            "remove_component",
            "batch",
        ]
    );
}

#[test]
fn commands_edit_the_model() {
    let mut model = model();
    for command in one_of_each() {
        command.run(&mut model).unwrap();
    }
    assert_eq!(model.get(id(2), "name"), Ok(Some(text("W2"))));
    assert_eq!(model.participants(id(10), "elements"), Ok(&[id(3)][..]));
    assert_eq!(
        model.property(id(3), PSET, "Reference"),
        Ok(Some(text("R3")))
    );
    assert_eq!(model.property_sets_of(id(2)).count(), 0);
    assert_eq!(model.type_of(id(2)), None);
    assert_eq!(model.component(id(20)), None);
    assert!(!model.contains(id(4)));
}

#[test]
fn commands_that_do_not_fit_the_model_fail() {
    let mut model = model();
    create_wall(1).run(&mut model).unwrap();
    assert_eq!(
        create_wall(1).run(&mut model),
        Err(CommandError::Model(StoreError::DuplicateId(id(1)).into()))
    );
    assert_eq!(
        delete(2).run(&mut model),
        Err(CommandError::Model(StoreError::NotFound(id(2)).into()))
    );
    assert_eq!(
        contain(10, 5, &[1]).run(&mut model),
        Err(CommandError::Model(ModelError::ParticipantNotFound(id(5))))
    );
}

#[test]
fn replay_reports_the_failing_command() {
    let log = [
        create_wall(1),
        create_wall(2),
        create_wall(1),
        create_wall(3),
    ];
    let mut model = model();
    let error = replay(&mut model, &log).unwrap_err();
    assert_eq!(
        error,
        ReplayError {
            index: 2,
            source: CommandError::Model(StoreError::DuplicateId(id(1)).into()),
        }
    );
    assert_eq!(model.len(), 2);
}

#[test]
fn errors_never_repeat_values_or_ids() {
    let id = EntityId::new(0xABCD);
    let error = ReplayError {
        index: 4,
        source: CommandError::Model(StoreError::NotFound(id).into()),
    };
    assert_eq!(error.to_string(), "command 4 of the log failed");
    let source = std::error::Error::source(&error).unwrap().to_string();
    assert_eq!(source, "no object with this id");
    assert!(!source.contains("abcd"));
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    #[test]
    fn any_command_round_trips(command in model_command(), value in any_value()) {
        prop_assert_eq!(round_trip(&command), command);
        let set = set(1, "name", Some(value));
        prop_assert_eq!(round_trip(&set), set);
    }

    /// Recording every command that succeeded, then replaying the serialised log on an empty
    /// model, rebuilds the model.
    #[test]
    fn replaying_the_recorded_log_reproduces_the_model(
        commands in proptest::collection::vec(model_command(), 0..48),
    ) {
        let mut model = model();
        let mut log = Vec::new();
        for command in commands {
            if command.run(&mut model).is_ok() {
                log.push(command);
            }
        }
        let recorded: Vec<ModelCommand> =
            serde_json::from_str(&serde_json::to_string(&log).unwrap()).unwrap();

        let mut rebuilt = crate::fixtures::model();
        prop_assert_eq!(replay(&mut rebuilt, &recorded), Ok(()));
        prop_assert_eq!(rebuilt, model);
    }
}
