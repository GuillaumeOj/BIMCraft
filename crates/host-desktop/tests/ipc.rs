//! The window's IPC channel passes the transport conformance suite (API-04), driven through
//! Tauri's IPC with its mock runtime: the same command and events as the real window, without
//! a webview.

// The client helpers run outside #[test] functions; a failed setup is a failed test.
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex, PoisonError};

use api::conformance::run;
use api::jsonrpc::Server;
use api::{Transport, TransportError};
use host_desktop::{DocumentHost, ipc};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
};
use tauri::webview::InvokeRequest;
use tauri::{App, Listener, WebviewWindow, WebviewWindowBuilder};

/// A client end of the window's channel: requests go through Tauri's IPC to the `api` command,
/// notifications come back as `api://notification` events.
struct TauriIpc {
    // Keeps the application, and so its webview and listeners, alive.
    _app: App<MockRuntime>,
    webview: WebviewWindow<MockRuntime>,
    notifications: Arc<Mutex<Vec<String>>>,
}

fn connect(server: Server) -> TauriIpc {
    let app = ipc::serve(mock_builder(), DocumentHost::new(server))
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

impl Transport for TauriIpc {
    fn send(&mut self, message: &str) -> Result<Option<String>, TransportError> {
        let request = InvokeRequest {
            cmd: ipc::COMMAND.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(serde_json::json!({ "message": message })),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        };
        let response =
            get_ipc_response(&self.webview, request).map_err(|error| TransportError {
                reason: error.to_string(),
            })?;
        response.deserialize().map_err(|error| TransportError {
            reason: error.to_string(),
        })
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
    assert_eq!(run(connect), Ok(()));
}

#[test]
fn a_new_document_is_empty_and_served_to_the_window() {
    let mut client = connect_to_new_document();
    let response = client
        .send(r#"{"jsonrpc":"2.0","id":1,"method":"history.state","params":{}}"#)
        .unwrap()
        .unwrap();
    let response: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(response["id"], 1);
    assert_eq!(response["result"]["can_undo"], false, "{response}");
    assert_eq!(response["result"]["can_redo"], false, "{response}");
}

fn connect_to_new_document() -> TauriIpc {
    let app = ipc::serve(mock_builder(), DocumentHost::open_new().unwrap())
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .unwrap();
    TauriIpc {
        _app: app,
        webview,
        notifications: Arc::default(),
    }
}
