//! The window's IPC channel passes the transport conformance suite (API-04), driven through
//! Tauri's IPC with its mock runtime: the same command and events as the real window, without
//! a webview.

// The client helpers run outside #[test] functions; a failed setup is a failed test.
#![allow(clippy::unwrap_used)]

use std::fmt::Display;
use std::sync::{Arc, Mutex, PoisonError};

use api::conformance::run;
use api::{Transport, TransportError};
use host_desktop::{DocumentHost, ipc};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

/// The window's own origin, which Tauri lets call app commands: Windows serves the app over
/// HTTP, the other platforms over the `tauri` scheme.
const APP_URL: &str = if cfg!(windows) {
    "http://tauri.localhost"
} else {
    "tauri://localhost"
};

fn failed(error: impl Display) -> TransportError {
    TransportError {
        reason: error.to_string(),
    }
}

/// A client end of the window's channel: requests go through Tauri's IPC to the `api` command,
/// notifications come back as `api://notification` events.
struct TauriIpc {
    // Keeps the application, and so its webview and listeners, alive.
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    notifications: Arc<Mutex<Vec<String>>>,
}

fn connect(host: DocumentHost) -> TauriIpc {
    let app = ipc::serve(mock_builder(), host, None)
        .build(mock_context(noop_assets()))
        .unwrap();
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let received = Arc::clone(&notifications);
    app.listen_any(ipc::NOTIFICATION, move |event| {
        let message: String = serde_json::from_str(event.payload()).unwrap();
        received.lock().unwrap().push(message);
    });
    let webview = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    TauriIpc {
        _app: app,
        webview,
        notifications,
    }
}

impl TauriIpc {
    /// Calls `command` with `args` as the window would: its result, or what it rejected with.
    fn call(&self, command: &str, args: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: APP_URL.parse().unwrap(),
            body: InvokeBody::Json(args),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        };
        get_ipc_response(&self.webview, request).map(|body| body.deserialize().unwrap())
    }
}

impl Transport for TauriIpc {
    fn send(&mut self, message: &str) -> Result<Option<String>, TransportError> {
        let response = self
            .call(ipc::COMMAND, json!({ "message": message }))
            .map_err(failed)?;
        serde_json::from_value(response).map_err(failed)
    }

    fn notifications(&mut self) -> Vec<String> {
        let mut received = self
            .notifications
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *received)
    }
}

#[test]
fn the_window_ipc_passes_the_conformance_suite() {
    assert_eq!(run(|server| connect(DocumentHost::new(server))), Ok(()));
}

#[test]
fn a_new_document_is_empty_and_served_to_the_window() {
    let mut client = connect(DocumentHost::open_new().unwrap());
    let response = client
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"history.state","params":{}}"#)
        .unwrap()
        .unwrap();
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["can_undo"], false, "{response}");
    assert_eq!(response["result"]["can_redo"], false, "{response}");
}

/// Asks the `scene` command for a delta; the answer is raw bytes, not JSON.
fn scene_bytes(client: &TauriIpc, since: &Value) -> Vec<u8> {
    let request = InvokeRequest {
        cmd: ipc::SCENE_COMMAND.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: APP_URL.parse().unwrap(),
        body: InvokeBody::Json(json!({ "since": since })),
        headers: tauri::http::HeaderMap::default(),
        invoke_key: INVOKE_KEY.to_owned(),
    };
    match get_ipc_response(&client.webview, request).unwrap() {
        tauri::ipc::InvokeResponseBody::Raw(bytes) => bytes,
        tauri::ipc::InvokeResponseBody::Json(json) => unreachable!("expected bytes, got {json}"),
    }
}

#[test]
fn the_window_reaches_the_update_commands() {
    let client = connect(DocumentHost::open_new().unwrap());
    // Without updates, as in debug builds and builds without an updater key.
    assert_eq!(
        client.call("app_info", json!({})),
        Ok(json!({ "version": "0.1.0", "updatable": false }))
    );
    let calls = [
        ("update_check", json!({ "channel": "stable" })),
        ("update_download", json!({})),
        ("update_install", json!({})),
    ];
    for (command, args) in calls {
        let error = client.call(command, args).unwrap_err();
        assert_eq!(error["code"], "unavailable", "{command}: {error}");
    }
}

#[test]
fn the_scene_command_answers_with_raw_bytes_a_viewport_can_decode() {
    let mut client = connect(DocumentHost::open_new().unwrap());
    let first = scene_bytes(&client, &Value::Null);
    let delta = scene_wire::decode(&first).unwrap();
    assert!(delta.full && delta.elements.is_empty());

    let place = r#"{"jsonrpc":"2.0","id":2,"method":"model.create","params":{"id":"00000000000000000000000000000001","entity":"Block","values":[["location",{"List":[{"Real":0.0},{"Real":0.0},{"Real":0.0}]}],["size_x",{"Real":1.0}],["size_y",{"Real":1.0}],["size_z",{"Real":1.0}]]}}"#;
    let response = client.send(place).unwrap().unwrap();
    assert!(!response.contains("error"), "{response}");
    let update = scene_wire::decode(&scene_bytes(&client, &json!(delta.revision))).unwrap();
    assert!(!update.full);
    assert_eq!(update.elements.len(), 1);
    assert_eq!(update.revision, delta.revision + 1);
}

#[test]
fn the_window_reaches_nothing_else() {
    let client = connect(DocumentHost::open_new().unwrap());
    for command in [
        "plugin:updater|check",
        "plugin:updater|download_and_install",
        "plugin:process|restart",
        "update",
        "restart",
    ] {
        assert!(!ipc::COMMANDS.contains(&command));
        assert!(client.call(command, json!({})).is_err(), "{command}");
    }
}
