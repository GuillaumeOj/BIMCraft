use core_txn::Origin;
use serde_json::Value;

use super::*;
use crate::{InProcess, TransportError};

#[test]
fn the_in_process_transport_conforms() {
    assert_eq!(run(|server| InProcess::new(server, Origin::Ui)), Ok(()));
}

/// Wraps the in-process transport and breaks one thing about it.
struct Broken {
    inner: InProcess,
    fault: Fault,
    notified: usize,
}

#[derive(Clone, Copy)]
enum Fault {
    DropsNotifications,
    DropsResponses,
    Fails,
    Garbles,
    WrongIds,
    BadNotifications,
    /// Rewrites each response the server sent, a batch element by element.
    Rewrites(fn(Value) -> Value),
}

fn each_response(text: &str, rewrite: fn(Value) -> Value) -> String {
    match serde_json::from_str(text).unwrap() {
        Value::Array(responses) => Value::Array(responses.into_iter().map(rewrite).collect()),
        response => rewrite(response),
    }
    .to_string()
}

/// Every error becomes a success.
fn hides_errors(mut response: Value) -> Value {
    if response["error"].take().is_object() {
        response.as_object_mut().unwrap().remove("error");
        response["result"] = Value::Null;
    }
    response
}

/// Every success becomes an internal error.
fn fails_everything(mut response: Value) -> Value {
    if response.as_object_mut().unwrap().remove("result").is_some() {
        response["error"] = json!({ "code": codes::INTERNAL_ERROR, "message": "internal error" });
    }
    response
}

/// Errors also carry a result.
fn adds_results(mut response: Value) -> Value {
    response["result"] = Value::Null;
    response
}

/// Results are replaced by a fixed object.
fn wrong_results(mut response: Value) -> Value {
    if response.get("result").is_some() {
        response["result"] = json!({ "items": [], "can_undo": true });
    }
    response
}

impl Transport for Broken {
    fn send(&mut self, message: &str) -> Result<Option<String>, TransportError> {
        let response = self.inner.send(message)?;
        match self.fault {
            Fault::DropsResponses => Ok(None),
            Fault::Fails => Err(TransportError {
                reason: "closed".to_owned(),
            }),
            Fault::Garbles => Ok(response.map(|_| "not json".to_owned())),
            Fault::WrongIds => Ok(response.map(|text| text.replace("\"id\":", "\"id\":0,\"x\":"))),
            Fault::Rewrites(rewrite) => Ok(response.map(|text| each_response(&text, rewrite))),
            Fault::DropsNotifications | Fault::BadNotifications => Ok(response),
        }
    }

    fn notifications(&mut self) -> Vec<String> {
        let notifications = self.inner.notifications();
        match self.fault {
            Fault::DropsNotifications => Vec::new(),
            Fault::BadNotifications => notifications
                .into_iter()
                .map(|text| {
                    self.notified += 1;
                    match self.notified % 3 {
                        1 => "{".to_owned(),
                        2 => text.replace(CHANGED, "model.other"),
                        _ => text.replace("{\"jsonrpc\"", "{\"id\":1,\"jsonrpc\""),
                    }
                })
                .collect(),
            _ => notifications,
        }
    }
}

fn failures(fault: Fault) -> Vec<Failure> {
    let result = run(|server| Broken {
        inner: InProcess::new(server, Origin::Ui),
        fault,
        notified: 0,
    });
    result.expect_err("a broken transport fails the suite")
}

#[test]
fn a_transport_that_loses_notifications_fails() {
    let cases: Vec<_> = failures(Fault::DropsNotifications)
        .iter()
        .map(|f| f.case)
        .collect();
    assert_eq!(
        cases,
        ["commands", "undo and redo", "batches", "notifications"]
    );
}

#[test]
fn every_case_fails_when_messages_do_not_come_back() {
    for fault in [Fault::DropsResponses, Fault::Fails, Fault::Garbles] {
        let failed = failures(fault);
        assert_eq!(failed.len(), 7, "{failed:?}");
    }
}

#[test]
fn responses_must_carry_their_request_id() {
    let cases: Vec<_> = failures(Fault::WrongIds).iter().map(|f| f.case).collect();
    // Errors to messages too large or too deep to read have no id to carry.
    let expected = [
        "commands",
        "queries",
        "errors",
        "undo and redo",
        "batches",
        "notifications",
    ];
    assert_eq!(cases, expected);
}

#[test]
fn malformed_notifications_fail() {
    let reasons: Vec<_> = failures(Fault::BadNotifications)
        .into_iter()
        .map(|f| f.reason)
        .collect();
    assert!(
        reasons
            .iter()
            .any(|reason| reason == "a notification is not JSON"),
        "{reasons:?}"
    );
}

#[test]
fn notifications_must_be_changes_without_ids() {
    let mut client = Client::new(Broken {
        inner: InProcess::new(server().unwrap(), Origin::Ui),
        fault: Fault::BadNotifications,
        notified: 0,
    });
    let create = |raw| json!({ "jsonrpc": "2.0", "method": "model.create", "params": create(raw) });
    for (raw, reason) in [
        (1, "a notification is not JSON"),
        (2, "a notification is not a change"),
        (3, "a notification has an id"),
    ] {
        client.send(&create(raw).to_string()).unwrap();
        assert_eq!(client.changes(), Err(reason.to_owned()));
    }
}

#[test]
fn a_failure_names_its_case() {
    let failure = Failure {
        case: "queries",
        reason: "get_object failed".to_owned(),
    };
    assert_eq!(failure.to_string(), "queries: get_object failed");
}

#[test]
fn responses_that_lie_about_success_fail() {
    for rewrite in [hides_errors, fails_everything, adds_results, wrong_results] {
        let failed = failures(Fault::Rewrites(rewrite));
        assert!(failed.len() >= 4, "{failed:?}");
    }
}
