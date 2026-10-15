use super::*;
use crate::fixtures::{
    PSET, Run, batch, component, contain, create_storey, create_wall, delete, diff, id, model,
    real, set, text,
};
use crate::{Document, ModelCommand};

fn ids<const N: usize>(raws: [u128; N]) -> BTreeSet<EntityId> {
    raws.into_iter().map(id).collect()
}

fn event<const C: usize, const M: usize, const D: usize>(
    created: [u128; C],
    modified: [u128; M],
    deleted: [u128; D],
) -> ChangeEvent {
    ChangeEvent {
        created: ids(created),
        modified: ids(modified),
        deleted: ids(deleted),
    }
}

/// Storey 1 contains walls 2, 3 and 4 through containment 10; walls 2 and 3 have type 21, made
/// from component 20. The history starts on that model.
fn document() -> Document<ModelCommand> {
    let mut model = model();
    batch([
        create_storey(1),
        create_wall(2),
        create_wall(3),
        create_wall(4),
        contain(10, 1, &[2, 3, 4]),
        ModelCommand::CreateComponent {
            id: id(20),
            component: component(),
        },
        ModelCommand::CreateType {
            id: id(21),
            component: id(20),
            values: Vec::new(),
        },
        ModelCommand::AssignType {
            instance: id(2),
            type_id: Some(id(21)),
        },
        ModelCommand::AssignType {
            instance: id(3),
            type_id: Some(id(21)),
        },
    ])
    .run(&mut model)
    .unwrap();
    Document::new(model)
}

/// Executes `command` and checks its event against a full comparison of the model.
fn execute(document: &mut Document<ModelCommand>, command: ModelCommand) -> ChangeEvent {
    let before = document.model().clone();
    let event = document.execute(command).unwrap();
    assert_eq!(event, diff(&before, document.model()));
    event
}

#[test]
fn lists_created_modified_and_deleted_objects() {
    let mut document = document();
    let event = execute(
        &mut document,
        batch([create_wall(5), set(2, "name", Some(text("W2"))), delete(4)]),
    );
    // The containment loses wall 4 as a participant.
    assert_eq!(event, self::event([5], [2, 10], [4]));
    assert!(!event.is_empty());
}

#[test]
fn deletions_report_what_they_cascade_to() {
    let mut document = document();
    // The containment loses its structure, so it goes with the storey.
    assert_eq!(execute(&mut document, delete(1)), event([], [], [1, 10]));
    // The instances of a deleted type lose their type.
    assert_eq!(execute(&mut document, delete(21)), event([], [2, 3], [21]));
}

#[test]
fn relationships_properties_and_types_modify_their_object() {
    let mut document = document();
    let commands = [
        (
            ModelCommand::RemoveParticipant {
                relationship: id(10),
                role: "elements".into(),
                participant: id(3),
            },
            10,
        ),
        (
            ModelCommand::AddParticipant {
                relationship: id(10),
                role: "elements".into(),
                participant: id(3),
            },
            10,
        ),
        (
            ModelCommand::SetProperty {
                id: id(4),
                set: PSET.into(),
                property: "Reference".into(),
                value: Some(text("R4")),
            },
            4,
        ),
        (
            ModelCommand::DetachPropertySet {
                id: id(4),
                set: PSET.into(),
            },
            4,
        ),
        (
            ModelCommand::AssignType {
                instance: id(4),
                type_id: Some(id(21)),
            },
            4,
        ),
        (set(21, "thickness", Some(real(0.3))), 21),
    ];
    for (command, modified) in commands {
        assert_eq!(execute(&mut document, command), event([], [modified], []));
    }
}

#[test]
fn components_are_not_objects() {
    let mut document = document();
    let create = ModelCommand::CreateComponent {
        id: id(30),
        component: component(),
    };
    assert!(execute(&mut document, create).is_empty());
    assert!(execute(&mut document, ModelCommand::RemoveComponent { id: id(30) }).is_empty());
}

#[test]
fn reports_the_net_effect_of_a_command() {
    let mut document = document();
    let event = execute(
        &mut document,
        batch([
            // Created then deleted: nothing to report.
            create_wall(5),
            set(5, "name", Some(text("W5"))),
            delete(5),
            // Changed then changed back: nothing either.
            set(2, "height", Some(real(3.0))),
            set(2, "height", None),
            // Deleted then created again with another state: modified.
            delete(4),
            create_wall(4),
            set(4, "name", Some(text("W4"))),
        ]),
    );
    // Deleting wall 4 took it out of the containment, which stays modified.
    assert_eq!(event, self::event([], [4, 10], []));
}

#[test]
fn a_command_that_changes_nothing_emits_an_empty_event() {
    let mut document = document();
    let event = execute(&mut document, set(2, "height", None));
    assert!(event.is_empty());
    assert_eq!(event, ChangeEvent::default());
}

#[test]
fn undo_emits_the_inverse_and_redo_emits_the_event_again() {
    let mut document = document();
    let event = execute(
        &mut document,
        batch([create_wall(5), set(2, "name", Some(text("W2"))), delete(4)]),
    );
    assert_eq!(document.undo(), Ok(self::event([4], [2, 10], [5])));
    assert_eq!(document.redo(), Ok(event));
}

#[test]
fn events_serialise() {
    let event = event([1], [], [2]);
    let json = serde_json::to_string(&event).unwrap();
    assert_eq!(serde_json::from_str::<ChangeEvent>(&json).unwrap(), event);
}
