use core_model::Model;
use core_txn::Origin;
use schema_gen::{Source, parse_sources};
use serde_json::{Value, json};

use super::*;
use crate::jsonrpc::{Server, codes};

fn model() -> Model {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: include_str!("../../../../schema/core/namespace.toml").into(),
        },
        Source {
            path: "entities/block.toml".into(),
            contents: include_str!("../../../../schema/core/entities/block.toml").into(),
        },
        Source {
            path: "entities/site.toml".into(),
            contents: include_str!("../../../../schema/core/entities/site.toml").into(),
        },
    ];
    Model::new(&parse_sources(&sources).unwrap()).unwrap()
}

fn session() -> Session {
    Session::new(model())
}

fn call(session: &mut Session, name: &str, params: Value) -> Result<Outcome, ApiError> {
    Session::registry()
        .unwrap()
        .dispatch(session, name, params, &Origin::Ui)
}

fn id(n: u128) -> Value {
    json!(format!("{n:032x}"))
}

fn add_block(session: &mut Session, n: u128, location: [f64; 3]) {
    let command = ModelCommand::Create {
        id: EntityId::new(n),
        entity: "Block".into(),
        values: vec![
            (
                "location".into(),
                AttributeValue::List(location.map(AttributeValue::Real).to_vec()),
            ),
            ("size_x".into(), AttributeValue::Real(1.0)),
            ("size_y".into(), AttributeValue::Real(1.0)),
            ("size_z".into(), AttributeValue::Real(1.0)),
        ],
    };
    session.document_mut().execute(command, Origin::Ui).unwrap();
}

fn location_of(session: &Session, n: u128) -> Vec<f64> {
    let value = session
        .document()
        .model()
        .get(EntityId::new(n), "location")
        .unwrap()
        .unwrap();
    let AttributeValue::List(items) = value else {
        panic!("a list");
    };
    items
        .into_iter()
        .map(|item| match item {
            AttributeValue::Real(v) => v,
            other => panic!("a real, got {other:?}"),
        })
        .collect()
}

fn refusal(result: Result<Outcome, ApiError>) -> ElementsError {
    match result {
        Err(ApiError::Elements(error)) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn move_params(ids: &[Value], translation: [f64; 3]) -> Value {
    json!({ "ids": ids, "translation": translation })
}

#[test]
fn moves_every_element_as_one_undo_step_and_one_event() {
    let mut s = session();
    add_block(&mut s, 1, [0.0, 0.0, 0.0]);
    add_block(&mut s, 2, [5.0, 5.0, 0.0]);
    let outcome = call(
        &mut s,
        "elements.move",
        move_params(&[id(1), id(2)], [1.0, 2.0, 3.0]),
    )
    .unwrap();
    assert_eq!(outcome.result, Value::Null);
    let event = outcome.event.unwrap();
    assert_eq!(event.modified.len(), 2);
    assert!(event.created.is_empty() && event.deleted.is_empty());
    assert_eq!(location_of(&s, 1), [1.0, 2.0, 3.0]);
    assert_eq!(location_of(&s, 2), [6.0, 7.0, 3.0]);

    call(&mut s, "history.undo", json!({})).unwrap();
    assert_eq!(location_of(&s, 1), [0.0, 0.0, 0.0]);
    assert_eq!(location_of(&s, 2), [5.0, 5.0, 0.0]);
    call(&mut s, "history.redo", json!({})).unwrap();
    assert_eq!(location_of(&s, 2), [6.0, 7.0, 3.0]);
}

#[test]
fn an_invalid_id_refuses_the_whole_command() {
    let mut s = session();
    add_block(&mut s, 1, [0.0; 3]);
    let result = call(
        &mut s,
        "elements.move",
        move_params(&[id(1), id(99)], [1.0, 0.0, 0.0]),
    );
    assert_eq!(refusal(result), ElementsError::UnknownElement);
    assert_eq!(location_of(&s, 1), [0.0; 3]);
}

#[test]
fn refuses_an_empty_list() {
    let mut s = session();
    let result = call(&mut s, "elements.move", move_params(&[], [1.0, 0.0, 0.0]));
    assert_eq!(refusal(result), ElementsError::Empty);
}

#[test]
fn the_limit_is_ten_thousand_elements() {
    let mut s = session();
    let ids: Vec<Value> = (0..=10_000).map(id).collect();
    let result = call(&mut s, "elements.move", move_params(&ids, [0.0; 3]));
    assert_eq!(refusal(result), ElementsError::TooMany);
    // At the limit the ids are checked one by one: the first is unknown.
    let result = call(
        &mut s,
        "elements.move",
        move_params(&ids[..10_000], [0.0; 3]),
    );
    assert_eq!(refusal(result), ElementsError::UnknownElement);
}

#[test]
fn refuses_an_element_without_a_known_placement() {
    let mut s = session();
    let site = ModelCommand::Create {
        id: EntityId::new(5),
        entity: "Site".into(),
        values: vec![],
    };
    s.document_mut().execute(site, Origin::Ui).unwrap();
    let result = call(&mut s, "elements.move", move_params(&[id(5)], [1.0; 3]));
    assert_eq!(refusal(result), ElementsError::NotMovable);
}

#[test]
fn refuses_a_block_whose_location_is_not_three_lengths() {
    let mut s = session();
    add_block(&mut s, 1, [0.0; 3]);
    for value in [
        AttributeValue::List(vec![AttributeValue::Real(1.0)]),
        AttributeValue::List(vec![
            AttributeValue::Real(1.0),
            AttributeValue::Text("x".into()),
            AttributeValue::Real(1.0),
        ]),
    ] {
        let set = ModelCommand::Set {
            id: EntityId::new(1),
            attribute: "location".into(),
            value: Some(value),
        };
        // The schema may already refuse the value; the cases it accepts are checked.
        if s.document_mut().execute(set, Origin::Ui).is_ok() {
            let result = call(&mut s, "elements.move", move_params(&[id(1)], [1.0; 3]));
            assert_eq!(refusal(result), ElementsError::NotMovable);
        }
    }
}

#[test]
fn the_translation_is_bounded() {
    let mut s = session();
    add_block(&mut s, 1, [0.0; 3]);
    for translation in [[1e6 + 1.0, 0.0, 0.0], [0.0, -1e6 - 1.0, 0.0]] {
        let result = call(&mut s, "elements.move", move_params(&[id(1)], translation));
        assert_eq!(refusal(result), ElementsError::OutOfRange);
    }
    let edge = call(
        &mut s,
        "elements.move",
        move_params(&[id(1)], [1e6, 0.0, -1e6]),
    );
    assert!(edge.is_ok());
}

#[test]
fn the_result_stays_in_the_world_extent() {
    let mut s = session();
    add_block(&mut s, 1, [999_500_000.0, 0.0, 0.0]);
    let result = call(
        &mut s,
        "elements.move",
        move_params(&[id(1)], [600_000.0, 0.0, 0.0]),
    );
    assert_eq!(refusal(result), ElementsError::OutOfRange);
    assert!((location_of(&s, 1)[0] - 999_500_000.0).abs() < 1e-6);
}

#[test]
fn duplicate_ids_are_invalid_params() {
    let mut s = session();
    add_block(&mut s, 1, [0.0; 3]);
    let result = call(
        &mut s,
        "elements.move",
        move_params(&[id(1), id(1)], [1.0; 3]),
    );
    assert!(matches!(result, Err(ApiError::InvalidParams { .. })));
    assert_eq!(location_of(&s, 1), [0.0; 3]);
}

#[test]
fn a_refusal_over_json_rpc_carries_its_reason() {
    let mut server = Server::new(session()).unwrap();
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "elements.move",
        "params": { "ids": [], "translation": [0.0, 0.0, 0.0] },
    });
    let handled = server.handle(request.to_string().as_bytes(), &Origin::Ui);
    let response: Value = serde_json::from_str(&handled.response.unwrap()).unwrap();
    assert_eq!(response["error"]["code"], codes::REJECTED);
    assert_eq!(response["error"]["data"]["reason"], "empty");
    assert_eq!(handled.notifications, [] as [String; 0]);
}

#[test]
fn every_reason_has_a_distinct_stable_name() {
    let reasons: BTreeSet<_> = [
        ElementsError::Empty,
        ElementsError::TooMany,
        ElementsError::UnknownElement,
        ElementsError::NotMovable,
        ElementsError::OutOfRange,
    ]
    .into_iter()
    .map(|e| (e.reason(), e.to_string()))
    .collect();
    assert_eq!(reasons.len(), 5);
}

#[test]
fn the_history_shows_a_creation_as_model_create_and_a_move_as_model_batch() {
    let mut s = session();
    add_block(&mut s, 1, [0.0; 3]);
    call(&mut s, "elements.move", move_params(&[id(1)], [1.0; 3])).unwrap();
    let state = call(&mut s, "history.state", json!({})).unwrap().result;
    let names: Vec<&str> = state["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["command"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["model.create", "model.batch"]);
}
