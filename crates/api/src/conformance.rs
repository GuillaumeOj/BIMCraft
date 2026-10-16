//! The suite every transport must pass (API-04): commands, queries, errors, undo and redo,
//! batches and change notifications, all through the transport under test.
//!
//! A transport's tests call [`run`] with a function that connects a client to the [`Server`] it
//! is given, and assert the result is `Ok`. Each case gets a fresh server on [`namespace`].

use std::collections::BTreeMap;
use std::fmt;

use core_model::Model;
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, ValueType,
};
use serde_json::{Value, json};

use crate::jsonrpc::{CHANGED, Limits, Server, codes};
use crate::{Session, Transport};

/// A case the transport failed, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The case, such as `"undo and redo"`.
    pub case: &'static str,
    /// What went wrong.
    pub reason: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.case, self.reason)
    }
}

/// The schema the suite's servers serve: walls with an optional name.
#[must_use]
pub fn namespace() -> Namespace {
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
        entities: BTreeMap::from([("Wall".into(), wall)]),
        property_sets: BTreeMap::new(),
    }
}

type Case<T> = fn(&mut Client<T>) -> Result<(), String>;

/// Runs every case, each on a transport `connect` makes to a fresh [`Server`] on
/// [`namespace`] with the default [`Limits`], and returns the cases that failed.
///
/// # Errors
///
/// The failures, when there are any.
pub fn run<T: Transport>(mut connect: impl FnMut(Server) -> T) -> Result<(), Vec<Failure>> {
    let cases: [(&'static str, Case<T>); 7] = [
        ("commands", commands),
        ("queries", queries),
        ("errors", errors),
        ("limits", limits),
        ("undo and redo", undo_and_redo),
        ("batches", batches),
        ("notifications", notifications),
    ];
    let mut failures = Vec::new();
    for (case, check) in cases {
        let outcome = server().and_then(|server| check(&mut Client::new(connect(server))));
        if let Err(reason) = outcome {
            failures.push(Failure { case, reason });
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn server() -> Result<Server, String> {
    let model = Model::new(&namespace()).map_err(|error| error.to_string())?;
    Server::new(Session::new(model)).map_err(|error| error.to_string())
}

/// The suite's side of a connection: JSON in and out, every unexpected answer an error.
struct Client<T> {
    transport: T,
    next_id: u64,
}

impl<T: Transport> Client<T> {
    fn new(transport: T) -> Self {
        Self {
            transport,
            next_id: 0,
        }
    }

    fn send(&mut self, message: &str) -> Result<Option<Value>, String> {
        let response = self
            .transport
            .send(message)
            .map_err(|error| error.to_string())?;
        response
            .map(|text| {
                serde_json::from_str(&text).map_err(|_| "a response is not JSON".to_owned())
            })
            .transpose()
    }

    /// Sends a request with a new id and returns its response, checking the id comes back.
    fn request(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self
            .send(&message.to_string())?
            .ok_or("a request got no response")?;
        expect(
            response["jsonrpc"] == "2.0",
            "a response is not JSON-RPC 2.0",
        )?;
        expect(
            response["id"] == id,
            "a response does not carry its request's id",
        )?;
        Ok(response)
    }

    /// Sends a request that must succeed and returns its result.
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let response = self.request(method, params)?;
        match response.get("result") {
            Some(result) if response.get("error").is_none() => Ok(result.clone()),
            _ => Err(format!("{method} failed")),
        }
    }

    /// Sends `message`, which must fail with `code`.
    fn fails(&mut self, message: &str, code: i64) -> Result<(), String> {
        let response = self
            .send(message)?
            .ok_or("an invalid message got no response")?;
        expect(
            response["error"]["code"] == code,
            &format!("expected error {code}"),
        )?;
        expect(
            response.get("result").is_none(),
            "an error response also has a result",
        )
    }

    /// The change events notified since the last call.
    fn changes(&mut self) -> Result<Vec<Value>, String> {
        self.transport
            .notifications()
            .iter()
            .map(|text| {
                let notification: Value =
                    serde_json::from_str(text).map_err(|_| "a notification is not JSON")?;
                expect(
                    notification["method"] == CHANGED,
                    "a notification is not a change",
                )?;
                expect(notification.get("id").is_none(), "a notification has an id")?;
                Ok(notification["params"].clone())
            })
            .collect()
    }
}

fn expect(condition: bool, reason: &str) -> Result<(), String> {
    if condition {
        Ok(())
    } else {
        Err(reason.to_owned())
    }
}

fn id(raw: u128) -> String {
    format!("{raw:032x}")
}

fn create(raw: u128) -> Value {
    json!({ "id": id(raw), "entity": "Wall", "values": [["name", { "Text": "W" }]] })
}

fn change(created: &[u128], modified: &[u128], deleted: &[u128]) -> Value {
    let ids = |raws: &[u128]| raws.iter().map(|&raw| id(raw)).collect::<Vec<_>>();
    json!({ "created": ids(created), "modified": ids(modified), "deleted": ids(deleted) })
}

fn commands<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    expect(
        client.call("model.create", &create(1))? == Value::Null,
        "create returns null",
    )?;
    expect(
        client.changes()? == [change(&[1], &[], &[])],
        "create notifies the new object",
    )?;
    let set = json!({ "id": id(1), "attribute": "name", "value": { "Text": "W2" } });
    client.call("model.set", &set)?;
    expect(
        client.changes()? == [change(&[], &[1], &[])],
        "set notifies the changed object",
    )
}

fn queries<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    client.call("model.create", &create(2))?;
    client.call("model.create", &create(1))?;
    client.changes()?;
    let object = client.call("model.get_object", &json!({ "id": id(1) }))?;
    expect(object["entity"] == "Wall", "get_object returns the entity")?;
    expect(
        object["attributes"] == json!([["name", { "Text": "W" }]]),
        "get_object returns the attributes",
    )?;
    let page = client.call(
        "model.list_objects",
        &json!({ "entity": "Wall", "limit": 1 }),
    )?;
    expect(
        page == json!({ "items": [id(1)], "next": id(1) }),
        "list_objects pages by id",
    )?;
    expect(client.changes()?.is_empty(), "queries notify nothing")
}

fn errors<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    client.fails("{", codes::PARSE_ERROR)?;
    client.fails(
        r#"{"jsonrpc":"1.0","id":1,"method":"history.state"}"#,
        codes::INVALID_REQUEST,
    )?;
    let cases = [
        ("model.missing", json!({}), codes::METHOD_NOT_FOUND),
        ("model.delete", json!({ "id": 1 }), codes::INVALID_PARAMS),
        ("model.delete", json!({ "id": id(9) }), codes::REJECTED),
        ("model.get_object", json!({ "id": id(9) }), codes::NOT_FOUND),
    ];
    for (method, params, code) in cases {
        let response = client.request(method, &params)?;
        expect(
            response["error"]["code"] == code,
            &format!("{method} fails with {code}"),
        )?;
    }
    expect(client.changes()?.is_empty(), "failed calls notify nothing")
}

fn limits<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
    client.fails(&deep, codes::INVALID_REQUEST)?;
    let name = "a".repeat(Limits::default().max_bytes);
    let large =
        json!({ "jsonrpc": "2.0", "id": 1, "method": "model.set", "params": { "name": name } });
    client.fails(&large.to_string(), codes::INVALID_REQUEST)
}

fn undo_and_redo<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    let undo = client.request("history.undo", &json!({}))?;
    expect(
        undo["error"]["code"] == codes::NOTHING_TO_UNDO,
        "nothing to undo at first",
    )?;
    client.call("model.create", &create(1))?;
    client.call("history.undo", &json!({}))?;
    let state = client.call("history.state", &json!({}))?;
    expect(
        state["can_redo"] == true && state["can_undo"] == false,
        "undo moves the command to redo",
    )?;
    client.call("history.redo", &json!({}))?;
    let redo = client.request("history.redo", &json!({}))?;
    expect(
        redo["error"]["code"] == codes::NOTHING_TO_REDO,
        "nothing left to redo",
    )?;
    let expected = [
        change(&[1], &[], &[]),
        change(&[], &[], &[1]),
        change(&[1], &[], &[]),
    ];
    expect(
        client.changes()? == expected,
        "create, undo and redo each notify their change",
    )
}

fn batches<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    let batch = json!([
        { "jsonrpc": "2.0", "id": "a", "method": "model.create", "params": create(1) },
        { "jsonrpc": "2.0", "method": "model.create", "params": create(2) },
        { "jsonrpc": "2.0", "id": "b", "method": "history.state" },
    ]);
    let responses = client
        .send(&batch.to_string())?
        .ok_or("a batch got no response")?;
    let ids: Vec<&Value> = responses
        .as_array()
        .ok_or("a batch response is not an array")?
        .iter()
        .map(|response| &response["id"])
        .collect();
    expect(
        ids == [&json!("a"), &json!("b")],
        "a batch answers each request, not notifications",
    )?;
    expect(
        client.changes()?.len() == 2,
        "each command of a batch notifies its change",
    )?;
    client.fails("[]", codes::INVALID_REQUEST)
}

fn notifications<T: Transport>(client: &mut Client<T>) -> Result<(), String> {
    let notification = json!({ "jsonrpc": "2.0", "method": "model.create", "params": create(1) });
    expect(
        client.send(&notification.to_string())?.is_none(),
        "a notification gets no response",
    )?;
    expect(
        client.changes()? == [change(&[1], &[], &[])],
        "a notification still notifies its change",
    )?;
    let state = client.call("history.state", &json!({}))?;
    expect(
        state["entries"]
            .as_array()
            .is_some_and(|entries| entries.len() == 1),
        "a notification still runs",
    )
}

#[cfg(test)]
mod tests;
