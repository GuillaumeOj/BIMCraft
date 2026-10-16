use core_txn::{Command, ModelCommand, Origin};
use proptest::prelude::*;
use serde_json::{Value, json};

use super::*;
use crate::fixtures::{any_command, create_wall, session, snapshot};

fn server() -> Server {
    Server::new(session()).unwrap()
}

/// Sends `message` and returns the parsed response, if any, and the notifications.
fn send(server: &mut Server, message: &Value) -> (Option<Value>, Vec<Value>) {
    let handled = server.handle(message.to_string().as_bytes(), &Origin::Ui);
    parse(&handled)
}

fn parse(handled: &Handled) -> (Option<Value>, Vec<Value>) {
    let response = handled
        .response
        .as_deref()
        .map(|text| serde_json::from_str(text).unwrap());
    let notifications = handled
        .notifications
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    (response, notifications)
}

fn create(raw: u128) -> Value {
    let mut params = serde_json::to_value(create_wall(raw)).unwrap();
    params.as_object_mut().unwrap().remove("command");
    params
}

fn request(id: &Value, method: &str, params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn code(response: &Value) -> i64 {
    response["error"]["code"].as_i64().unwrap()
}

#[test]
fn a_command_answers_with_its_result_and_notifies_the_change() {
    let mut server = server();
    let (response, notifications) =
        send(&mut server, &request(&json!(1), "model.create", &create(7)));
    assert_eq!(
        response,
        Some(json!({ "jsonrpc": "2.0", "id": 1, "result": null }))
    );
    let id = format!("{:032x}", 7);
    assert_eq!(
        notifications,
        [json!({
            "jsonrpc": "2.0",
            "method": CHANGED,
            "params": { "created": [id], "modified": [], "deleted": [] },
        })]
    );
}

#[test]
fn a_query_answers_with_its_result_and_notifies_nothing() {
    let mut server = server();
    let (response, notifications) = send(
        &mut server,
        &request(&json!("q"), "history.state", &json!({})),
    );
    let result = json!({ "can_undo": false, "can_redo": false, "entries": [], "next": null });
    assert_eq!(
        response,
        Some(json!({ "jsonrpc": "2.0", "id": "q", "result": result }))
    );
    assert!(notifications.is_empty());
}

#[test]
fn missing_params_are_an_empty_object() {
    let mut server = server();
    let message = json!({ "jsonrpc": "2.0", "id": 1, "method": "history.state" });
    let (response, _) = send(&mut server, &message);
    assert_eq!(response.unwrap()["result"]["can_undo"], false);
}

#[test]
fn a_notification_gets_no_response_but_still_runs() {
    let mut server = server();
    let message = json!({ "jsonrpc": "2.0", "method": "model.create", "params": create(1) });
    let (response, notifications) = send(&mut server, &message);
    assert_eq!(response, None);
    assert_eq!(notifications.len(), 1);
    let failing = json!({ "jsonrpc": "2.0", "method": "model.missing" });
    assert_eq!(send(&mut server, &failing), (None, vec![]));
}

#[test]
fn a_null_id_is_a_request_not_a_notification() {
    let mut server = server();
    let (response, _) = send(
        &mut server,
        &request(&Value::Null, "history.state", &json!({})),
    );
    assert_eq!(response.unwrap()["id"], Value::Null);
}

#[test]
fn errors_carry_the_request_id_and_their_code() {
    let mut server = server();
    send(&mut server, &request(&json!(0), "model.create", &create(1)));
    let wall = format!("{:032x}", 1);
    let cases = [
        ("model.missing", json!({}), codes::METHOD_NOT_FOUND),
        ("model.delete", json!({ "id": 3 }), codes::INVALID_PARAMS),
        ("model.create", create(1), codes::REJECTED),
        ("history.redo", json!({}), codes::NOTHING_TO_REDO),
        (
            "model.get_object",
            json!({ "id": format!("{:032x}", 9) }),
            codes::NOT_FOUND,
        ),
    ];
    for (method, params, expected) in cases {
        let (response, notifications) =
            send(&mut server, &request(&json!(method), method, &params));
        let response = response.unwrap();
        assert_eq!(response["id"], method);
        assert_eq!(code(&response), expected, "{method}");
        assert!(response.get("result").is_none());
        assert!(notifications.is_empty());
    }
    send(
        &mut server,
        &request(&json!(0), "model.delete", &json!({ "id": wall })),
    );
    send(&mut server, &request(&json!(0), "history.undo", &json!({})));
    send(&mut server, &request(&json!(0), "history.undo", &json!({})));
    let (response, _) = send(&mut server, &request(&json!(0), "history.undo", &json!({})));
    assert_eq!(code(&response.unwrap()), codes::NOTHING_TO_UNDO);
}

#[test]
fn invalid_params_say_what_was_wrong_in_data() {
    let mut server = server();
    let (response, _) = send(&mut server, &request(&json!(1), "model.delete", &json!({})));
    let error = &response.unwrap()["error"];
    assert_eq!(error["message"], "invalid parameters");
    assert!(
        error["data"]["detail"].as_str().unwrap().contains("id"),
        "{error}"
    );
}

#[test]
fn malformed_requests_are_invalid_requests() {
    let mut server = server();
    let cases = [
        (json!(3), Value::Null),
        (json!("text"), Value::Null),
        (
            json!({ "jsonrpc": "2.0", "id": [1], "method": "history.state" }),
            Value::Null,
        ),
        (
            json!({ "jsonrpc": "2.0", "id": {}, "method": "history.state" }),
            Value::Null,
        ),
        (json!({ "id": 1, "method": "history.state" }), json!(1)),
        (
            json!({ "jsonrpc": "1.0", "id": 1, "method": "history.state" }),
            json!(1),
        ),
        (
            json!({ "jsonrpc": 2, "id": 1, "method": "history.state" }),
            json!(1),
        ),
        (json!({ "jsonrpc": "2.0", "id": 1 }), json!(1)),
        (json!({ "jsonrpc": "2.0", "id": 1, "method": 5 }), json!(1)),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "history.state", "params": 5 }),
            json!(1),
        ),
        (
            json!({ "jsonrpc": "2.0", "id": 1, "method": "history.state", "extra": true }),
            json!(1),
        ),
        (
            json!({ "jsonrpc": "2.0", "method": "history.state", "params": "x" }),
            Value::Null,
        ),
    ];
    for (message, id) in cases {
        let (response, _) = send(&mut server, &message);
        let response = response.unwrap();
        assert_eq!(code(&response), codes::INVALID_REQUEST, "{message}");
        assert_eq!(response["id"], id, "{message}");
    }
}

#[test]
fn text_that_is_not_json_is_a_parse_error() {
    let mut server = server();
    for message in [
        &b"{"[..],
        b"",
        b"\xff\xfe",
        b"{\"jsonrpc\": \"2.0\",}",
        b"[1,]",
    ] {
        let (response, _) = parse(&server.handle(message, &Origin::Ui));
        let response = response.unwrap();
        assert_eq!(code(&response), codes::PARSE_ERROR);
        assert_eq!(response["id"], Value::Null);
    }
}

#[test]
fn oversized_and_too_deep_messages_are_refused_before_parsing() {
    let limits = Limits {
        max_bytes: 64,
        max_depth: 3,
        max_batch: 2,
    };
    let mut server = server().with_limits(limits);
    let large = format!("\"{}\"", "a".repeat(64));
    let (response, _) = parse(&server.handle(large.as_bytes(), &Origin::Ui));
    assert_eq!(response.as_ref().map(code), Some(codes::INVALID_REQUEST));
    assert_eq!(response.unwrap()["error"]["message"], "message too large");
    // Unbalanced, so a parser would fail anyway; the depth check answers first.
    let (response, _) = parse(&server.handle(b"[[[[", &Origin::Ui));
    assert_eq!(
        response.unwrap()["error"]["message"],
        "message nested too deeply"
    );
    let (response, _) = parse(&server.handle(b"[[[]]]", &Origin::Ui));
    assert_eq!(code(&response.unwrap()[0]), codes::INVALID_REQUEST);
}

#[test]
fn a_batch_answers_each_request_in_order() {
    let mut server = server();
    let batch = json!([
        request(&json!(1), "model.create", &create(1)),
        json!({ "jsonrpc": "2.0", "method": "model.create", "params": create(2) }),
        request(&json!(2), "model.create", &create(1)),
        "not a request",
        request(&json!(3), "history.state", &json!({})),
    ]);
    let (response, notifications) = send(&mut server, &batch);
    let response = response.unwrap();
    let responses = response.as_array().unwrap();
    assert_eq!(responses.len(), 4, "the notification gets no response");
    assert_eq!(responses[0]["result"], Value::Null);
    assert_eq!(code(&responses[1]), codes::REJECTED);
    assert_eq!(code(&responses[2]), codes::INVALID_REQUEST);
    assert_eq!(
        responses[3]["result"]["entries"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        notifications.len(),
        2,
        "one per command that changed the model"
    );
}

#[test]
fn a_batch_of_notifications_gets_no_response() {
    let mut server = server();
    let notification = json!({ "jsonrpc": "2.0", "method": "history.state" });
    assert_eq!(
        send(&mut server, &json!([notification, notification])),
        (None, vec![])
    );
}

#[test]
fn empty_and_oversized_batches_are_one_invalid_request() {
    let mut server = server().with_limits(Limits {
        max_batch: 2,
        ..Limits::default()
    });
    let state = request(&json!(1), "history.state", &json!({}));
    for (batch, message) in [
        (json!([]), "empty batch"),
        (json!([state, state, state]), "batch too large"),
    ] {
        let (response, _) = send(&mut server, &batch);
        let response = response.unwrap();
        assert_eq!(code(&response), codes::INVALID_REQUEST);
        assert_eq!(response["error"]["message"], message);
    }
}

#[test]
fn a_change_with_no_effect_notifies_nothing() {
    let mut server = server();
    let empty = json!({ "commands": [] });
    let (response, notifications) = send(&mut server, &request(&json!(1), "model.batch", &empty));
    assert_eq!(response.unwrap()["result"], Value::Null);
    assert!(notifications.is_empty());
    assert_eq!(server.session().document().history().count(), 1);
}

#[test]
fn the_transport_origin_is_recorded() {
    let mut server = server();
    let plugin = Origin::Plugin {
        name: "p".to_owned(),
    };
    let message = request(&json!(1), "model.create", &create(1)).to_string();
    server.handle(message.as_bytes(), &plugin);
    let origin = server
        .session()
        .document()
        .history()
        .next()
        .map(|entry| entry.origin.clone());
    assert_eq!(origin, Some(plugin));
}

#[test]
fn the_default_limits_bound_size_depth_and_batches() {
    let limits = Limits::default();
    assert_eq!(limits.max_bytes, 1 << 20);
    assert_eq!(limits.max_depth, 64);
    assert_eq!(limits.max_batch, 100);
}

#[test]
fn depth_ignores_brackets_inside_strings() {
    assert_eq!(depth(b"1"), 0);
    assert_eq!(depth(br#"{"a":[1,{"b":[]}]}"#), 4);
    assert_eq!(depth(br#"["[[[", "\"[[", "\\"]"#), 1);
    assert_eq!(depth(b"]]][["), 2);
}

/// Every response is a JSON-RPC 2.0 response: an id, and a result or an error with an integer
/// code and a message, never both.
fn assert_response(response: &Value) {
    assert_eq!(response["jsonrpc"], "2.0", "{response}");
    let object = response.as_object().unwrap();
    assert!(object.contains_key("id"), "{response}");
    match (object.get("result"), object.get("error")) {
        (Some(_), None) => {}
        (None, Some(error)) => {
            assert!(error["code"].is_i64(), "{response}");
            assert!(error["message"].is_string(), "{response}");
        }
        _ => panic!("a result or an error: {response}"),
    }
}

fn assert_valid_output(handled: &Handled) {
    if let Some(text) = &handled.response {
        let response: Value = serde_json::from_str(text).unwrap();
        match &response {
            Value::Array(responses) => {
                assert!(!responses.is_empty());
                responses.iter().for_each(assert_response);
            }
            response => assert_response(response),
        }
    }
    for text in &handled.notifications {
        let notification: Value = serde_json::from_str(text).unwrap();
        assert_eq!(notification["method"], CHANGED);
        assert!(notification.get("id").is_none());
    }
}

fn reference_depth(value: &Value) -> usize {
    match value {
        Value::Array(items) => 1 + items.iter().map(reference_depth).max().unwrap_or(0),
        Value::Object(map) => 1 + map.values().map(reference_depth).max().unwrap_or(0),
        _ => 0,
    }
}

fn any_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        "[\\[\\]{}\"\\\\a-z.]{0,6}".prop_map(Value::String),
    ];
    leaf.prop_recursive(6, 48, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            proptest::collection::btree_map("[a-z]{0,6}", inner, 0..4)
                .prop_map(|map| Value::Object(map.into_iter().collect())),
        ]
    })
}

/// Requests close to valid ones, so most reach the registry.
fn any_request() -> impl Strategy<Value = Value> {
    let method = prop::sample::select(vec![
        "model.create",
        "model.set",
        "model.delete",
        "model.batch",
        "history.undo",
        "history.redo",
        "history.state",
        "model.get_object",
        "model.list_objects",
        "nope",
    ]);
    let params = prop_oneof![
        any_command().prop_map(|command| {
            let mut value = serde_json::to_value(command).unwrap();
            value.as_object_mut().unwrap().remove("command");
            value
        }),
        any_json(),
    ];
    let id = prop_oneof![Just(None), any::<u8>().prop_map(|id| Some(json!(id)))];
    (method, params, id).prop_map(|(method, params, id)| {
        let mut request = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        if let Some(id) = id {
            request["id"] = id;
        }
        request
    })
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    #[test]
    fn any_bytes_get_a_valid_answer(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let mut server = server().with_limits(Limits { max_bytes: 128, max_depth: 4, max_batch: 3 });
        let before = snapshot(server.session());
        let handled = server.handle(&bytes, &Origin::Ui);
        assert_valid_output(&handled);
        prop_assert!(handled.response.is_some() || handled.notifications.is_empty());
        if handled.notifications.is_empty() {
            prop_assert_eq!(snapshot(server.session()), before);
        }
    }

    #[test]
    fn any_json_gets_a_valid_answer(value in any_json()) {
        let mut server = server().with_limits(Limits { max_depth: 4, ..Limits::default() });
        assert_valid_output(&server.handle(value.to_string().as_bytes(), &Origin::Ui));
    }

    #[test]
    fn any_requests_get_valid_answers(
        requests in proptest::collection::vec(any_request(), 1..6),
        batched in any::<bool>(),
    ) {
        let mut server = server();
        let messages = if batched { vec![Value::Array(requests)] } else { requests };
        for message in messages {
            assert_valid_output(&server.handle(message.to_string().as_bytes(), &Origin::Ui));
        }
    }

    #[test]
    fn the_depth_scan_matches_the_parsed_depth(value in any_json()) {
        prop_assert_eq!(depth(value.to_string().as_bytes()), reference_depth(&value));
    }

    /// A command sent as a request is the command the history records.
    #[test]
    fn requests_round_trip_to_the_history(command in any_command()) {
        let mut server = server();
        let mut params = serde_json::to_value(&command).unwrap();
        params.as_object_mut().unwrap().remove("command");
        let method = format!("model.{}", command.name());
        let (response, _) = send(&mut server, &request(&json!(1), &method, &params));
        let response = response.unwrap();
        prop_assert_eq!(&response["id"], &json!(1));
        let recorded: Vec<&ModelCommand> = server.session().document().commands().collect();
        if response.get("result").is_some() {
            prop_assert_eq!(recorded, [&command]);
        } else {
            prop_assert!(recorded.is_empty());
        }
    }
}

#[test]
fn errors_the_server_cannot_provoke_still_have_codes() {
    let version = ApiError::UnknownVersion {
        name: "model.set".to_owned(),
        version: crate::Version::new(9, 0, 0),
    };
    assert_eq!(
        code(&error_response(&json!(1), &version)),
        codes::METHOD_NOT_FOUND
    );
    let internal = error_response(&json!(1), &ApiError::Internal);
    assert_eq!(code(&internal), codes::INTERNAL_ERROR);
    assert_eq!(internal["error"]["message"], "internal error");
}
