// The API client: UI code imports from here only (UI-01). Biome forbids Tauri elsewhere.
export { ApiClient, type Transport } from "./client";
export { ApiError, TransportError } from "./jsonrpc";
export type * from "./protocol.gen";
export { tauriTransport } from "./tauri";
