//! JSON-RPC 2.0 messages around the commands and queries (API-03).
//!
//! Messages come from other processes and plugins, so they are hostile: a message is measured
//! before it is parsed, and every malformed, oversized or too deeply nested one gets a JSON-RPC
//! error, never a panic. Batches are supported, each request in them handled on its own.

use core_txn::{ChangeEvent, Origin};
use serde_json::{Map, Value, json};

use crate::{ApiError, Registry, RegistryError, Session};

/// The method of the notification a server sends after each change to the model; its params are
/// the [`ChangeEvent`].
pub const CHANGED: &str = "model.changed";

/// The error codes a server answers with: JSON-RPC's own, then the API's, from -32000 down.
pub mod codes {
    /// The message is not JSON.
    pub const PARSE_ERROR: i64 = -32700;
    /// The message is JSON but not a request, or is too large or too deeply nested.
    pub const INVALID_REQUEST: i64 = -32600;
    /// No command or query has this name.
    pub const METHOD_NOT_FOUND: i64 = -32601;
    /// The params do not decode into the command's or query's input.
    pub const INVALID_PARAMS: i64 = -32602;
    /// A bug in the core, not in the request.
    pub const INTERNAL_ERROR: i64 = -32603;
    /// The model rejected the command.
    pub const REJECTED: i64 = -32000;
    /// Undo was asked with nothing to undo.
    pub const NOTHING_TO_UNDO: i64 = -32001;
    /// Redo was asked with nothing to redo.
    pub const NOTHING_TO_REDO: i64 = -32002;
    /// No object has the id asked for.
    pub const NOT_FOUND: i64 = -32003;
}

/// Bounds on what one message may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The largest message, in bytes.
    pub max_bytes: usize,
    /// The deepest nesting of arrays and objects, the batch array included.
    pub max_depth: usize,
    /// The most requests in one batch.
    pub max_batch: usize,
}

impl Default for Limits {
    /// 1 MiB, 64 levels and 100 requests: far more than any command needs, far less than what
    /// would exhaust memory or the stack. Bulk geometry goes through a binary channel (API-03).
    fn default() -> Self {
        Self {
            max_bytes: 1 << 20,
            max_depth: 64,
            max_batch: 100,
        }
    }
}

/// What handling one message produced: the response to send back, if any (notifications get
/// none), and the change notifications to send to every client.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handled {
    /// The response, or the array of responses to a batch.
    pub response: Option<String>,
    /// One [`CHANGED`] notification per command that changed the model, in order.
    pub notifications: Vec<String>,
}

/// A session served over JSON-RPC.
pub struct Server {
    registry: Registry<Session>,
    session: Session,
    limits: Limits,
}

impl Server {
    /// Serves `session` with the built-in commands and queries and the default [`Limits`].
    ///
    /// # Errors
    ///
    /// Never in practice; see [`Session::registry`].
    pub fn new(session: Session) -> Result<Self, RegistryError> {
        Ok(Self {
            registry: Session::registry()?,
            session,
            limits: Limits::default(),
        })
    }

    /// The same server with other limits.
    #[must_use]
    pub fn with_limits(self, limits: Limits) -> Self {
        Self { limits, ..self }
    }

    /// The session served.
    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Handles one message from a client. `origin` is who the transport knows the client to be;
    /// a message cannot claim another origin.
    pub fn handle(&mut self, message: &[u8], origin: &Origin) -> Handled {
        let mut notifications = Vec::new();
        let response = match self.parse(message) {
            Err(error) => Some(error),
            Ok(Value::Array(batch)) => self.batch(batch, origin, &mut notifications),
            Ok(request) => self.request(request, origin, &mut notifications),
        };
        Handled {
            response: response.map(|response| response.to_string()),
            notifications: notifications.iter().map(Value::to_string).collect(),
        }
    }

    fn parse(&self, message: &[u8]) -> Result<Value, Value> {
        if message.len() > self.limits.max_bytes {
            return Err(failure(
                &Value::Null,
                codes::INVALID_REQUEST,
                "message too large",
            ));
        }
        if depth(message) > self.limits.max_depth {
            return Err(failure(
                &Value::Null,
                codes::INVALID_REQUEST,
                "message nested too deeply",
            ));
        }
        serde_json::from_slice(message)
            .map_err(|_| failure(&Value::Null, codes::PARSE_ERROR, "parse error"))
    }

    fn batch(
        &mut self,
        batch: Vec<Value>,
        origin: &Origin,
        notifications: &mut Vec<Value>,
    ) -> Option<Value> {
        if batch.is_empty() {
            return Some(failure(&Value::Null, codes::INVALID_REQUEST, "empty batch"));
        }
        if batch.len() > self.limits.max_batch {
            return Some(failure(
                &Value::Null,
                codes::INVALID_REQUEST,
                "batch too large",
            ));
        }
        let responses: Vec<Value> = batch
            .into_iter()
            .filter_map(|request| self.request(request, origin, notifications))
            .collect();
        // A batch of notifications only gets no response at all.
        (!responses.is_empty()).then_some(Value::Array(responses))
    }

    /// Handles one request; a notification, a request without an id, gets no response, even
    /// when it fails.
    fn request(
        &mut self,
        request: Value,
        origin: &Origin,
        notifications: &mut Vec<Value>,
    ) -> Option<Value> {
        let (id, method, params) = match Request::from_value(request) {
            Ok(request) => (request.id, request.method, request.params),
            Err(id) => return Some(failure(&id, codes::INVALID_REQUEST, "invalid request")),
        };
        let outcome = self
            .registry
            .dispatch(&mut self.session, &method, params, origin);
        if let Ok(outcome) = &outcome
            && let Some(event) = outcome.event.as_ref().filter(|event| !event.is_empty())
        {
            notifications.push(changed(event));
        }
        let id = id?;
        Some(match outcome {
            Ok(outcome) => json!({ "jsonrpc": "2.0", "id": id, "result": outcome.result }),
            Err(error) => error_response(&id, &error),
        })
    }
}

/// A request's parts: `id` is `None` for a notification.
struct Request {
    id: Option<Value>,
    method: String,
    params: Value,
}

impl Request {
    /// Reads a request, or returns the id to answer the error with: the request's own when it
    /// is valid, `null` otherwise.
    fn from_value(value: Value) -> Result<Self, Value> {
        let Value::Object(mut object) = value else {
            return Err(Value::Null);
        };
        let id = match object.remove("id") {
            None => None,
            Some(id @ (Value::Null | Value::Number(_) | Value::String(_))) => Some(id),
            Some(_) => return Err(Value::Null),
        };
        let error_id = || id.clone().unwrap_or(Value::Null);
        let version = object.remove("jsonrpc");
        let method = object.remove("method");
        let params = object
            .remove("params")
            .unwrap_or_else(|| Value::Object(Map::new()));
        let (Some(Value::String(version)), Some(Value::String(method))) = (version, method) else {
            return Err(error_id());
        };
        if version != "2.0" || !object.is_empty() || !(params.is_object() || params.is_array()) {
            return Err(error_id());
        }
        Ok(Self { id, method, params })
    }
}

fn failure(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// The error response to `error`. Messages never hold model content (SEC-03); the decoder's
/// detail, which may quote the params, goes back to the client that sent them as `data`.
fn error_response(id: &Value, error: &ApiError) -> Value {
    let code = match error {
        ApiError::UnknownName(_) | ApiError::UnknownVersion { .. } => codes::METHOD_NOT_FOUND,
        ApiError::InvalidParams { detail } => {
            let mut response = failure(id, codes::INVALID_PARAMS, &error.to_string());
            response["error"]["data"] = json!({ "detail": detail });
            return response;
        }
        ApiError::NotFound => codes::NOT_FOUND,
        ApiError::Rejected(_) => codes::REJECTED,
        ApiError::NothingToUndo => codes::NOTHING_TO_UNDO,
        ApiError::NothingToRedo => codes::NOTHING_TO_REDO,
        ApiError::Internal => codes::INTERNAL_ERROR,
    };
    failure(id, code, &error.to_string())
}

fn changed(event: &ChangeEvent) -> Value {
    json!({ "jsonrpc": "2.0", "method": CHANGED, "params": event })
}

/// The deepest nesting of arrays and objects in `message`, brackets inside strings ignored. It
/// runs before the parser, in one pass and constant memory, so a deeply nested message is
/// refused without recursing into it. Malformed input gives some depth; the parser rejects it
/// afterwards.
fn depth(message: &[u8]) -> usize {
    let (mut depth, mut deepest) = (0usize, 0);
    let (mut in_string, mut escaped) = (false, false);
    for &byte in message {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

#[cfg(test)]
mod tests;
