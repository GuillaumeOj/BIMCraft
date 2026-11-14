//! The window's channel to its document: JSON-RPC messages over Tauri's IPC (API-04).
//!
//! The window calls the [`COMMAND`] command with one message and gets the response back, or
//! `null` for notifications. Change notifications reach every window as [`NOTIFICATION`]
//! events, whose payload is the notification message.

use tauri::{AppHandle, Builder, Emitter, Runtime, State};

use crate::DocumentHost;

/// The Tauri command that carries a JSON-RPC message from the window.
pub const COMMAND: &str = "api";

/// The Tauri event that carries a `model.changed` notification to the windows.
pub const NOTIFICATION: &str = "api://notification";

// Tauri hands commands their arguments by value. `async`: Tauri runs it off the main thread, so a
// long command never freezes the window; the document's lock keeps messages one at a time.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
fn api<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, DocumentHost>,
    message: String,
) -> Option<String> {
    let handled = host.handle(&message);
    for notification in handled.notifications {
        // Emitting fails only when no window can receive events, so nobody misses it.
        let _ = app.emit(NOTIFICATION, notification);
    }
    handled.response
}

/// The Tauri command that returns the scene delta for a viewport holding revision `since`, as raw
/// bytes (api-contract.md section 1): an `ArrayBuffer` in JavaScript, never parsed there as JSON.
pub const SCENE_COMMAND: &str = "scene";

// Tauri hands commands their arguments by value.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
fn scene(
    host: State<'_, DocumentHost>,
    since: Option<u64>,
) -> Result<tauri::ipc::Response, String> {
    let bytes = host.scene(since).map_err(|error| error.to_string())?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// `builder` serving `host` to its windows.
pub fn serve<R: Runtime>(builder: Builder<R>, host: DocumentHost) -> Builder<R> {
    builder
        .manage(host)
        .invoke_handler(tauri::generate_handler![api, scene])
}
