import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Transport } from "./client";
import { TransportError } from "./jsonrpc";

/** The desktop window's command that carries one message (host_desktop::ipc::COMMAND). */
const COMMAND = "api";
/** The event that carries the core's notifications (host_desktop::ipc::NOTIFICATION). */
const NOTIFICATION = "api://notification";

/** The transport of the desktop window: Tauri's IPC to the document in the same program. */
export const tauriTransport: Transport = {
  async send(message) {
    try {
      return await invoke<string | null>(COMMAND, { message });
    } catch (error) {
      // Tauri rejects with a string or, for some IPC failures, an object: keep its content.
      throw new TransportError(typeof error === "string" ? error : JSON.stringify(error));
    }
  },
  listen(onMessage) {
    return listen<string>(NOTIFICATION, (event) => onMessage(event.payload));
  },
};
