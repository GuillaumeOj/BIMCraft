use serde_json::{Value, json};

use super::*;
use crate::fixtures::session;

fn transport(origin: Origin) -> InProcess {
    InProcess::new(Server::new(session()).unwrap(), origin)
}

#[test]
fn messages_reach_the_server_with_the_transports_origin() {
    let plugin = Origin::Plugin {
        name: "p".to_owned(),
    };
    let mut transport = transport(plugin.clone());
    let create = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "model.create",
        "params": { "id": format!("{:032x}", 1), "entity": "Wall", "values": [] },
    });
    let response = transport.send(&create.to_string()).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["result"],
        Value::Null
    );
    let history = transport.server().session().document().history().next();
    assert_eq!(history.map(|entry| entry.origin.clone()), Some(plugin));
}

#[test]
fn notifications_are_kept_until_taken() {
    let mut transport = transport(Origin::Ui);
    let create = |raw: u128| {
        json!({
            "jsonrpc": "2.0",
            "method": "model.create",
            "params": { "id": format!("{raw:032x}"), "entity": "Wall", "values": [] },
        })
        .to_string()
    };
    assert_eq!(transport.send(&create(1)), Ok(None));
    assert_eq!(transport.send(&create(2)), Ok(None));
    assert_eq!(transport.notifications().len(), 2);
    assert!(transport.notifications().is_empty());
}

#[test]
fn a_transport_error_names_its_reason() {
    let error = TransportError {
        reason: "closed".to_owned(),
    };
    assert_eq!(error.to_string(), "transport failed: closed");
}
