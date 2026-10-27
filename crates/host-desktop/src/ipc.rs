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

/// `builder` serving `host` to its windows.
pub fn serve<R: Runtime>(builder: Builder<R>, host: DocumentHost) -> Builder<R> {
    builder
        .manage(host)
        .invoke_handler(tauri::generate_handler![api])
}
