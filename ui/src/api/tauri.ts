import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Transport } from "./client";
import { TransportError } from "./jsonrpc";

/** The desktop window's command that carries one message (host_desktop::ipc::COMMAND). */
const COMMAND = "api";
/** The command that returns a scene delta as raw bytes (host_desktop::ipc::SCENE_COMMAND). */
const SCENE_COMMAND = "scene";
/** The event that carries the core's notifications (host_desktop::ipc::NOTIFICATION). */
const NOTIFICATION = "api://notification";

// Tauri rejects with a string or, for some IPC failures, an object: keep its content.
const failure = (error: unknown) =>
  new TransportError(typeof error === "string" ? error : JSON.stringify(error));

/** The transport of the desktop window: Tauri's IPC to the document in the same program. */
export const tauriTransport: Transport = {
  async send(message) {
    try {
      return await invoke<string | null>(COMMAND, { message });
    } catch (error) {
      throw failure(error);
    }
  },
  async scene(since) {
    let bytes: unknown;
    try {
      bytes = await invoke<unknown>(SCENE_COMMAND, { since });
    } catch (error) {
      throw failure(error);
    }
    // A raw `tauri::ipc::Response` arrives as an ArrayBuffer; anything else is not a scene.
    if (!(bytes instanceof ArrayBuffer)) throw new TransportError("malformed scene response");
    return bytes;
  },
  listen(onMessage) {
    return listen<string>(NOTIFICATION, (event) => onMessage(event.payload));
  },
};
