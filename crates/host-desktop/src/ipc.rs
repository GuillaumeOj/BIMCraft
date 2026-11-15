//! The window's channel to its document: JSON-RPC messages over Tauri's IPC (API-04).
//!
//! The window calls the [`COMMAND`] command with one message and gets the response back, or
//! `null` for notifications. Change notifications reach every window as [`NOTIFICATION`]
//! events, whose payload is the notification message.
//!
//! The same handler serves the [`update`](crate::update) commands, which are not JSON-RPC
//! methods: updating the program is not a document operation, so plugins, scripts and network
//! clients of the document API never reach them.

use tauri::{AppHandle, Builder, Emitter, Runtime, State};

use crate::DocumentHost;
use crate::update;

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

/// The commands the window may call: [`COMMAND`], [`SCENE_COMMAND`] and the update commands.
pub const COMMANDS: [&str; 6] = [
    COMMAND,
    SCENE_COMMAND,
    "app_info",
    "update_check",
    "update_download",
    "update_install",
];

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

/// `builder` serving `host` to its windows, and the program's update state, whose updates come
/// from `updates`, or which cannot update itself when that is `None` (see [`update::can_update`]).
pub fn serve<R: Runtime>(
    builder: Builder<R>,
    host: DocumentHost,
    updates: Option<update::UpdatesSource<R>>,
) -> Builder<R> {
    update::register(builder, updates)
        .manage(host)
        .invoke_handler(tauri::generate_handler![
            api,
            scene,
            update::app_info,
            update::update_check,
            update::update_download,
            update::update_install
        ])
}
