use core_model::StoreError;
use proptest::prelude::*;

use super::*;
use crate::fixtures::{any_command, create_wall, id, model};
use crate::{ModelCommand, replay};

fn document() -> Document<ModelCommand> {
    Document::new(model())
}

#[test]
fn starts_with_nothing_to_undo_or_redo() {
    let mut document = document();
    assert!(!document.can_undo());
    assert!(!document.can_redo());
    assert_eq!(document.undo(), Err(HistoryError::NothingToUndo));
    assert_eq!(document.redo(), Err(HistoryError::NothingToRedo));
    assert_eq!(document.commands().count(), 0);
}

#[test]
fn undoes_and_redoes_commands_in_order() {
    let mut document = document();
    document.execute(create_wall(1)).unwrap();
    document.execute(create_wall(2)).unwrap();
    assert!(document.can_undo());

    document.undo().unwrap();
    assert!(document.model().contains(id(1)));
    assert!(!document.model().contains(id(2)));
    assert!(document.can_redo());
    assert_eq!(document.commands().collect::<Vec<_>>(), [&create_wall(1)]);

    document.undo().unwrap();
    assert!(document.model().is_empty());

    document.redo().unwrap();
    document.redo().unwrap();
    assert_eq!(document.model().len(), 2);
    assert!(!document.can_redo());
}

#[test]
fn a_new_command_discards_what_was_undone() {
    let mut document = document();
    document.execute(create_wall(1)).unwrap();
    document.undo().unwrap();
    document.execute(create_wall(2)).unwrap();
    assert!(!document.can_redo());
    assert_eq!(document.redo(), Err(HistoryError::NothingToRedo));
}

#[test]
fn a_failing_command_changes_neither_the_model_nor_the_history() {
    let mut document = document();
    document.execute(create_wall(1)).unwrap();
    document.execute(create_wall(2)).unwrap();
    document.undo().unwrap();
    let before = document.model().clone();

    let failing = ModelCommand::Batch {
        commands: vec![
            ModelCommand::Delete { id: id(1) },
            create_wall(1),
            create_wall(1),
        ],
    };
    assert_eq!(
        document.execute(failing),
        Err(CommandError::Model(StoreError::DuplicateId(id(1)).into()))
    );
    assert_eq!(document.model(), &before);
    assert_eq!(document.commands().count(), 1);
    assert!(document.can_redo());
}

#[test]
fn errors_describe_themselves() {
    assert_eq!(HistoryError::NothingToUndo.to_string(), "nothing to undo");
    assert_eq!(HistoryError::NothingToRedo.to_string(), "nothing to redo");
    assert_eq!(
        HistoryError::from(ModelError::from(StoreError::NotFound(id(0xABCD)))).to_string(),
        "no object with this id"
    );
}

#[derive(Clone, Debug)]
enum Step {
    Execute(ModelCommand),
    Undo,
    Redo,
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        3 => any_command().prop_map(Step::Execute),
        1 => Just(Step::Undo),
        1 => Just(Step::Redo),
    ]
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    /// Whatever the sequence of do, undo and redo, the model is the replay of the applied
    /// commands, and undoing everything gives back the empty model.
    #[test]
    fn any_sequence_keeps_the_model_consistent(
        steps in proptest::collection::vec(step(), 0..64),
    ) {
        let mut document = document();
        // The expected history: applied commands, and undone ones with the next to redo last.
        let mut done: Vec<ModelCommand> = Vec::new();
        let mut undone: Vec<ModelCommand> = Vec::new();
        for step in steps {
            match step {
                Step::Execute(command) => {
                    if document.execute(command.clone()).is_ok() {
                        done.push(command);
                        undone.clear();
                    }
                }
                Step::Undo => {
                    prop_assert_eq!(document.undo().is_ok(), !done.is_empty());
                    undone.extend(done.pop());
                }
                Step::Redo => {
                    prop_assert_eq!(document.redo().is_ok(), !undone.is_empty());
                    done.extend(undone.pop());
                }
            }
            let mut expected = model();
            prop_assert_eq!(replay(&mut expected, &done), Ok(()));
            prop_assert_eq!(document.model(), &expected);
            prop_assert!(document.commands().eq(done.iter()));
            prop_assert_eq!(document.can_redo(), !undone.is_empty());
        }
        while document.can_undo() {
            document.undo().unwrap();
        }
        prop_assert_eq!(document.model(), &model());
    }
}
