use core_model::StoreError;

use super::*;
use crate::ModelCommand;
use crate::fixtures::{
    PSET, Run, batch, contain, create_storey, create_wall, delete, id, model, one_of_each, real,
    set, text,
};

/// A storey containing walls 2, 3 and 4, wall 2 carrying the property set.
fn contained() -> Model {
    let mut model = model();
    batch([
        create_storey(1),
        create_wall(2),
        create_wall(3),
        create_wall(4),
        contain(10, 1, &[2, 3, 4]),
        ModelCommand::AttachPropertySet {
            id: id(2),
            set: PSET.into(),
            values: vec![("Reference".into(), text("R2"))],
        },
    ])
    .run(&mut model)
    .unwrap();
    model
}

/// Checks that the changes `command` makes on `model` undo and redo exactly.
fn assert_reversible(model: &mut Model, command: &ModelCommand) {
    let before = model.clone();
    let (changes, _) = Transaction::run(model, command).unwrap();
    let after = model.clone();
    undo_all(model, &changes).unwrap();
    assert_eq!(*model, before, "undo of {command:?}");
    redo_all(model, &changes).unwrap();
    assert_eq!(*model, after, "redo of {command:?}");
}

#[test]
fn every_change_undoes_and_redoes_exactly() {
    let mut model = model();
    for command in one_of_each() {
        assert_reversible(&mut model, &command);
    }
}

#[test]
fn changes_that_carry_more_than_their_command_undo_exactly() {
    let property = |value| ModelCommand::SetProperty {
        id: id(2),
        set: PSET.into(),
        property: "Reference".into(),
        value,
    };
    let commands = [
        // A property of a set the object already carries, then cleared.
        property(Some(text("R2b"))),
        property(None),
        // A participant from the middle of its role.
        ModelCommand::RemoveParticipant {
            relationship: id(10),
            role: "elements".into(),
            participant: id(3),
        },
        set(2, "height", Some(real(3.0))),
        // The storey, with the containment it leaves without structure.
        delete(1),
    ];
    let mut model = contained();
    for command in commands {
        assert_reversible(&mut model, &command);
    }
}

#[test]
fn a_failing_command_leaves_the_model_unchanged() {
    let mut model = model();
    create_wall(50).run(&mut model).unwrap();
    let before = model.clone();
    // Every step but the last succeeds, so the transaction must roll them all back.
    let failing = ModelCommand::Batch {
        commands: [one_of_each(), vec![delete(1), delete(99)]].concat(),
    };
    assert_eq!(
        failing.run(&mut model),
        Err(CommandError::Model(StoreError::NotFound(id(99)).into()))
    );
    assert_eq!(model, before);
}

#[test]
fn commands_read_the_model_as_they_change_it() {
    let mut model = model();
    let mut transaction = Transaction::new(&mut model);
    transaction
        .insert(id(1), "Storey", [("elevation", real(3.0))])
        .unwrap();
    assert_eq!(
        transaction.model().get(id(1), "elevation"),
        Ok(Some(real(3.0)))
    );
    assert_eq!(
        transaction.set(id(1), "elevation", None),
        Ok(Some(real(3.0)))
    );
    assert_eq!(transaction.remove(id(1)).unwrap().ids().count(), 1);
    assert!(transaction.model().is_empty());
}

#[test]
fn writes_return_what_the_model_returns() {
    let mut model = contained();
    let mut transaction = Transaction::new(&mut model);
    assert_eq!(
        transaction.remove_participant(id(10), "elements", id(3)),
        Ok(1)
    );
    let values = transaction.detach_property_set(id(2), PSET).unwrap();
    assert!(values.contains(&("Reference".into(), text("R2"))));
    assert_eq!(
        transaction.set_property(id(3), PSET, "Reference", None),
        Ok(None)
    );
    let component = crate::fixtures::component();
    transaction
        .create_component(id(20), component.clone())
        .unwrap();
    assert_eq!(transaction.remove_component(id(20)), Ok(component));
    assert_eq!(transaction.assign_type(id(2), None), Ok(None));
}
