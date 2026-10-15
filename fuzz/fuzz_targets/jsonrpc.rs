//! Feeds arbitrary bytes to the JSON-RPC server as one message from a client (API-03). Whatever
//! the bytes, handling must never panic, and must answer with JSON-RPC: a response or a batch of
//! them, or nothing for notifications, and change notifications.

#![no_main]

use api::Session;
use api::conformance::namespace;
use api::jsonrpc::{CHANGED, Limits, Server};
use core_model::Model;
use core_txn::Origin;
use libfuzzer_sys::fuzz_target;
use serde_json::Value;

fn assert_response(response: &Value) {
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(response.get("id").is_some());
    assert!(response.get("result").is_some() != response.get("error").is_some());
}

fuzz_target!(|message: &[u8]| {
    // Walls with a name, so well-formed commands can succeed.
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
