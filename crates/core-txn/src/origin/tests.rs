use super::*;
use crate::fixtures::{create_wall, id, model, real, set, text};
use crate::{Document, ModelCommand};

fn plugin() -> Origin {
    Origin::Plugin {
        name: "stair-generator".into(),
    }
}

fn ai_session() -> Origin {
    Origin::AiSession { id: "s-42".into() }
}

#[test]
fn the_history_lists_each_command_with_its_origin() {
    let mut document = Document::new(model());
    document.execute(create_wall(1), Origin::Ui).unwrap();
    document
        .execute(set(1, "name", Some(text("W1"))), plugin())
        .unwrap();
    document
        .execute(ModelCommand::Delete { id: id(1) }, ai_session())
        .unwrap();

    let entries: Vec<_> = document.history().collect();
    assert_eq!(
        entries,
        [
            HistoryEntry {
                command: "create",
                origin: &Origin::Ui,
                undone: false,
            },
            HistoryEntry {
                command: "set",
                origin: &plugin(),
                undone: false,
            },
            HistoryEntry {
                command: "delete",
                origin: &ai_session(),
                undone: false,
            },
        ]
    );
}

#[test]
fn undo_and_redo_keep_the_origin() {
    let mut document = Document::new(model());
    document.execute(create_wall(1), Origin::Ui).unwrap();
    document.execute(create_wall(2), plugin()).unwrap();
    document.execute(create_wall(3), ai_session()).unwrap();
    document.undo().unwrap();
    document.undo().unwrap();

    let origins = |document: &Document<ModelCommand>| {
        document
            .history()
            .map(|entry| (entry.origin.clone(), entry.undone))
            .collect::<Vec<_>>()
    };
    // Undone commands follow the applied ones, in the order a redo applies them.
    assert_eq!(
        origins(&document),
        [(Origin::Ui, false), (plugin(), true), (ai_session(), true)]
    );
    document.redo().unwrap();
    assert_eq!(
        origins(&document),
        [(Origin::Ui, false), (plugin(), false), (ai_session(), true)]
    );
}

#[test]
fn origins_serialise_as_tagged_objects() {
    let cases = [
        (Origin::Ui, serde_json::json!({ "kind": "ui" })),
        (
            plugin(),
            serde_json::json!({ "kind": "plugin", "name": "stair-generator" }),
        ),
        (
            ai_session(),
            serde_json::json!({ "kind": "ai_session", "id": "s-42" }),
        ),
    ];
    for (origin, json) in cases {
        assert_eq!(serde_json::to_value(&origin).unwrap(), json);
        assert_eq!(serde_json::from_value::<Origin>(json).unwrap(), origin);
    }
}

fn entry(origin: &Origin, undone: bool) -> HistoryEntry<'_> {
    HistoryEntry {
        command: "set",
        origin,
        undone,
    }
}

#[test]
fn entries_display_the_command_and_its_origin() {
    assert_eq!(
        entry(&Origin::Ui, false).to_string(),
        "set from the user interface"
    );
    assert_eq!(
        entry(&plugin(), true).to_string(),
        "set from plugin \"stair-generator\" (undone)"
    );
    assert_eq!(
        entry(&ai_session(), false).to_string(),
        "set from AI session \"s-42\""
    );
}

#[test]
fn origins_from_outside_are_escaped() {
    let forged = Origin::Plugin {
        name: "x\" (undone)\nset from the user interface".into(),
    };
    assert_eq!(
        forged.to_string(),
        r#"plugin "x\" (undone)\nset from the user interface""#
    );
    assert!(!entry(&forged, false).to_string().contains('\n'));
}

/// SEC-03: what the history exposes for display or logging never holds model content.
#[test]
fn entries_never_hold_model_content() {
    let mut document = Document::new(model());
    let wall = 0x00AB_CDEF_0123_4567_89AB_CDEF_0123_4567;
    document
        .execute(
            ModelCommand::Create {
                id: id(wall),
                entity: "Wall".into(),
                values: vec![
                    ("name".into(), text("Secret wall")),
                    ("height".into(), real(3.25)),
                ],
            },
            plugin(),
        )
        .unwrap();
    let entry = document.history().next().unwrap();
    for shown in [entry.to_string(), format!("{entry:?}")] {
        for content in [
            "Secret",
            "3.25",
            "Wall",
            "abcdef",
            "ABCDEF",
            &id(wall).to_global_id(),
        ] {
            assert!(!shown.contains(content), "{shown} shows {content}");
        }
    }
}
