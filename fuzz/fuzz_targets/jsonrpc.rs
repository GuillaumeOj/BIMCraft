//! Feeds arbitrary bytes to the JSON-RPC server as one message from a client (API-03). Whatever
//! the bytes, handling must never panic, and must answer with JSON-RPC: a response or a batch of
//! them, or nothing for notifications, and change notifications.

#![no_main]

use api::Session;
use api::jsonrpc::{CHANGED, Limits, Server};
use core_model::Model;
use core_txn::Origin;
use libfuzzer_sys::fuzz_target;
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, ValueType,
};
use serde_json::Value;

/// Walls with a name, so well-formed commands can succeed.
fn namespace() -> Namespace {
    let wall = Entity {
        name: "Wall".into(),
        kind: EntityKind::Element,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: "IfcWall".into(),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes: vec![Attribute {
            name: "name".into(),
            value: ValueType::Text,
            presence: Presence::Optional,
            placement: Placement::Instance,
            doc: None,
        }],
        roles: Vec::new(),
        representations: Vec::new(),
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [("Wall".into(), wall)].into(),
        property_sets: Default::default(),
    }
}

fn assert_response(response: &Value) {
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(response.get("id").is_some());
    assert!(response.get("result").is_some() != response.get("error").is_some());
}

fuzz_target!(|message: &[u8]| {
    let model = Model::new(&namespace()).expect("the namespace is valid");
    // Small limits, so the fuzzer reaches them.
    let limits = Limits {
        max_bytes: 4096,
        max_depth: 16,
        max_batch: 8,
    };
    let mut server = Server::new(Session::new(model))
        .expect("the built-in registry is valid")
        .with_limits(limits);
    let handled = server.handle(message, &Origin::Ui);
    if let Some(text) = handled.response {
        match serde_json::from_str::<Value>(&text).expect("the response is JSON") {
            Value::Array(responses) => responses.iter().for_each(assert_response),
            response => assert_response(&response),
        }
    }
    for text in handled.notifications {
        let notification: Value = serde_json::from_str(&text).expect("the notification is JSON");
        assert_eq!(notification["method"], CHANGED);
    }
});
