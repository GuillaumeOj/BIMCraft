use core_txn::{ModelCommand, Origin};
use proptest::prelude::*;
use serde_json::{Value, json};

use crate::fixtures::{
    PSET, any_command, create_storey, create_wall, id, one_of_each, session, snapshot, text,
};
use crate::{ApiError, Kind, MAX_PAGE_SIZE, Registry, Session};

fn registry() -> Registry<Session> {
    Session::registry().unwrap()
}

fn run(session: &mut Session, command: ModelCommand) {
    session.document_mut().execute(command, Origin::Ui).unwrap();
}

fn query(session: &Session, name: &str, params: Value) -> Result<Value, ApiError> {
    let mut copy = session.clone();
    let outcome = registry().dispatch(&mut copy, name, params, &Origin::Ui)?;
    assert_eq!(outcome.event, None, "a query reports no change");
    Ok(outcome.result)
}

fn hex(raw: u128) -> String {
    format!("{raw:032x}")
}

fn walls(count: u128) -> Session {
    let mut session = session();
    run(&mut session, create_storey(1000));
    // Created out of id order, so the page order cannot come from creation order.
    for raw in (1..=count).rev() {
        run(&mut session, create_wall(raw));
    }
    session
}

fn page(session: &Session, params: Value) -> (Vec<u128>, Option<u128>) {
    let result = query(session, "model.list_objects", params).unwrap();
    let raw = |value: &Value| u128::from_str_radix(value.as_str().unwrap(), 16).unwrap();
    let items = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(raw)
        .collect();
    let next = (!result["next"].is_null()).then(|| raw(&result["next"]));
    (items, next)
}

#[test]
fn every_query_is_registered_as_a_query() {
    let registry = registry();
    for name in [
        "history.state",
        "model.get_object",
        "model.list_objects",
        "model.property_sets",
    ] {
        assert_eq!(
            registry.get(name).map(crate::Descriptor::kind),
            Ok(Kind::Query),
            "{name}"
        );
    }
}

#[test]
fn get_object_returns_its_stored_attributes() {
    let mut session = session();
    run(
        &mut session,
        crate::fixtures::create_named_wall(1, Some(text("W1"))),
    );
    let result = query(&session, "model.get_object", json!({ "id": hex(1) })).unwrap();
    assert_eq!(
        result,
        json!({
            "id": hex(1),
            "entity": "Wall",
            "attributes": [["name", { "Text": "W1" }]],
            "type_of": null,
            "component": null,
        })
    );
}

#[test]
fn reading_a_missing_object_is_not_found() {
    let session = session();
    for name in ["model.get_object", "model.property_sets"] {
        assert_eq!(
            query(&session, name, json!({ "id": hex(7) })),
            Err(ApiError::NotFound)
        );
    }
    assert_eq!(ApiError::NotFound.to_string(), "no object with this id");
}

#[test]
fn list_objects_pages_in_id_order() {
    let session = walls(5);
    let first = page(&session, json!({ "entity": "Wall", "limit": 2 }));
    assert_eq!(first, (vec![1, 2], Some(2)));
    let second = page(
        &session,
        json!({ "entity": "Wall", "limit": 2, "after": hex(2) }),
    );
    assert_eq!(second, (vec![3, 4], Some(4)));
    let last = page(
        &session,
        json!({ "entity": "Wall", "limit": 2, "after": hex(4) }),
    );
    assert_eq!(last, (vec![5], None));
}

#[test]
fn a_page_that_ends_exactly_on_the_last_object_has_no_next() {
    let session = walls(4);
    assert_eq!(
        page(&session, json!({ "entity": "Wall", "limit": 4 })),
        (vec![1, 2, 3, 4], None)
    );
    let after_last = page(&session, json!({ "entity": "Wall", "after": hex(4) }));
    assert_eq!(after_last, (vec![], None));
}

#[test]
fn list_objects_keeps_to_the_entity_asked_for() {
    let session = walls(2);
    assert_eq!(
        page(&session, json!({ "entity": "Storey" })),
        (vec![1000], None)
    );
    assert_eq!(page(&session, json!({ "entity": "Beam" })), (vec![], None));
}

#[test]
fn the_page_size_is_bounded() {
    let session = walls(super::DEFAULT_PAGE_SIZE as u128 + 1);
    let (items, next) = page(&session, json!({ "entity": "Wall" }));
    assert_eq!(items.len(), super::DEFAULT_PAGE_SIZE);
    assert!(next.is_some());
    let (items, _) = page(
        &session,
        json!({ "entity": "Wall", "limit": MAX_PAGE_SIZE }),
    );
    assert_eq!(items.len(), super::DEFAULT_PAGE_SIZE + 1);
    for limit in [0, MAX_PAGE_SIZE + 1] {
        let error = query(
            &session,
            "model.list_objects",
            json!({ "entity": "Wall", "limit": limit }),
        );
        assert!(
            matches!(error, Err(ApiError::InvalidParams { .. })),
            "{limit}: {error:?}"
        );
    }
}

#[test]
fn property_sets_lists_each_attached_set() {
    let mut session = session();
    run(&mut session, create_wall(1));
    assert_eq!(
        query(&session, "model.property_sets", json!({ "id": hex(1) })),
        Ok(json!([]))
    );
    run(
        &mut session,
        ModelCommand::AttachPropertySet {
            id: id(1),
            set: PSET.into(),
            values: vec![("Reference".into(), text("R1"))],
        },
    );
    let result = query(&session, "model.property_sets", json!({ "id": hex(1) }));
    assert_eq!(
        result,
        Ok(json!([{ "set": PSET, "values": [["Reference", { "Text": "R1" }]] }]))
    );
}

#[test]
fn history_state_lists_commands_by_name_and_origin() {
    let mut session = session();
    assert_eq!(
        query(&session, "history.state", json!({})),
        Ok(json!({ "can_undo": false, "can_redo": false, "entries": [], "next": null }))
    );
    run(&mut session, create_wall(1));
    let plugin = Origin::Plugin {
        name: "p".to_owned(),
    };
    session
        .document_mut()
        .execute(create_wall(2), plugin)
        .unwrap();
    session.document_mut().undo().unwrap();
    assert_eq!(
        query(&session, "history.state", json!({})),
        Ok(json!({
            "can_undo": true,
            "can_redo": true,
            "entries": [
                { "command": "model.create", "origin": { "kind": "ui" }, "undone": false },
                { "command": "model.create", "origin": { "kind": "plugin", "name": "p" }, "undone": true },
            ],
            "next": null,
        }))
    );
}

#[test]
fn history_state_pages_the_history() {
    let mut session = session();
    for raw in 1..=5 {
        run(&mut session, create_wall(raw));
    }
    let page = |params| {
        let result = query(&session, "history.state", params).unwrap();
        (
            result["entries"].as_array().unwrap().len(),
            result["next"].clone(),
        )
    };
    assert_eq!(page(json!({ "limit": 2 })), (2, json!(2)));
    assert_eq!(page(json!({ "start": 2, "limit": 2 })), (2, json!(4)));
    assert_eq!(page(json!({ "start": 4, "limit": 2 })), (1, Value::Null));
    assert_eq!(page(json!({ "start": 5 })), (0, Value::Null));
    assert_eq!(page(json!({ "start": usize::MAX })), (0, Value::Null));
    for limit in [0, MAX_PAGE_SIZE + 1] {
        let error = query(&session, "history.state", json!({ "limit": limit }));
        assert!(
            matches!(error, Err(ApiError::InvalidParams { .. })),
            "{limit}"
        );
    }
}

#[test]
fn queries_reject_unknown_parameters() {
    let session = walls(1);
    let bad = [
        ("history.state", json!({ "all": true })),
        ("model.get_object", json!({ "id": hex(1), "deep": true })),
        (
            "model.list_objects",
            json!({ "entity": "Wall", "offset": 3 }),
        ),
        ("model.property_sets", json!({})),
    ];
    for (name, params) in bad {
        let error = query(&session, name, params);
        assert!(
            matches!(error, Err(ApiError::InvalidParams { .. })),
            "{name}: {error:?}"
        );
    }
}

fn any_query() -> impl Strategy<Value = (&'static str, Value)> {
    let raw = || 0..10u128;
    prop_oneof![
        Just(("history.state", json!({}))),
        raw().prop_map(|raw| ("model.get_object", json!({ "id": hex(raw) }))),
        raw().prop_map(|raw| ("model.property_sets", json!({ "id": hex(raw) }))),
        (
            prop::sample::select(vec!["Wall", "Storey", "Containment", "WallType", "Beam"]),
            prop::option::of(raw()),
            0..4usize,
        )
            .prop_map(|(entity, after, limit)| {
                let params = json!({ "entity": entity, "after": after.map(hex), "limit": limit });
                ("model.list_objects", params)
            }),
    ]
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    #[test]
    fn queries_never_change_the_model_or_its_history(
        commands in proptest::collection::vec(any_command(), 0..12),
        queries in proptest::collection::vec(any_query(), 1..8),
    ) {
        let registry = registry();
        let mut session = session();
        for command in one_of_each().into_iter().take(3).chain(commands) {
            // Many random commands do not fit the model; only the ones that do are kept.
            let _ = session.document_mut().execute(command, Origin::Ui);
        }
        let before = snapshot(&session);
        for (name, params) in queries {
            let _ = registry.dispatch(&mut session, name, params, &Origin::Ui);
            prop_assert_eq!(&snapshot(&session), &before);
        }
    }
}
