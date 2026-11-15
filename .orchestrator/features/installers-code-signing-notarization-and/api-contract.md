# Auto-update — contract between the desktop host and the UI

These are **Tauri commands**, not JSON-RPC methods of the `api` crate: updating the program is not a document operation, and plugins, scripts and network clients of the document API must never reach it. Task back-1 adds them in `host-desktop` (registered by `desktop-app`, declared in `build.rs`, granted in `capabilities/main.json`); task front-1 calls them from `ui/src/api/updates.ts`, the only UI module that invokes them (it may import `@tauri-apps/api/core` and `event`, like `tauri.ts`).

Arguments and results are JSON (camelCase keys, as Tauri serialises command arguments). Every command rejects with an `UpdateError` on failure.

## Types

```ts
type Channel = "stable" | "beta";

interface AppInfo {
  version: string;          // the running program, semver, e.g. "0.3.0" or "0.4.0-beta.1"
  updatable: boolean;       // false in debug builds and builds without an updater key: hide update controls
}

interface UpdateOffer {
  version: string;          // the newer version, semver
  date: string | null;      // publication date, RFC 3339, when the manifest has one
  notes: string;            // release notes as PLAIN TEXT (may be empty); the UI never renders it as HTML or Markdown
}

interface UpdateProgress {  // payload of the event "update://progress"
  downloaded: number;       // bytes received so far
  total: number | null;     // bytes expected, when the server says
}

interface UpdateError {
  code:
    | "unavailable"         // updates are not possible in this build (updatable = false)
    | "network"             // no connection, timeout, HTTP error
    | "manifest"            // the channel's manifest is malformed or has no package for this platform
    | "signature"           // the downloaded package's signature does not verify: nothing is installed
    | "no_update"           // download/install called with no offer from the last check
    | "not_downloaded"      // install called before a completed download
    | "install";            // the installer failed to start or to replace the program
  message: string;          // English, for logs only; the UI shows its own translated text by code
}
```

## Commands

### `app_info`

- args: none
- result: `AppInfo`. Never fails. Works offline.

### `update_check`

- args: `{ channel: Channel }`
- result: `UpdateOffer | null` — `null` when the running version is the newest of that channel (equal or higher; never a downgrade).
- The host keeps the offer for `update_download`. A new check replaces it (and drops any downloaded package of another version).
- Errors: `unavailable`, `network`, `manifest`.
- Timeout: 30 s.

### `update_download`

- args: none
- Downloads the package of the current offer and verifies its signature. While it runs, the host emits `update://progress` events (`UpdateProgress`), at most every 100 ms and once at the end.
- result: `null` when the package is downloaded and verified. Calling it again after success resolves at once.
- Errors: `unavailable`, `no_update`, `network`, `signature`.

### `update_install`

- args: none
- Installs the downloaded package and restarts the program. On success it does not return (the program exits); the UI must therefore have asked the user first (UX brief).
- Errors: `unavailable`, `not_downloaded`, `install`.

## Rules

- The channel is an enum; the endpoint URL lives only in the host (spec D4). An unknown channel is rejected by Tauri's argument deserialisation.
- The host never starts a check, a download or an install by itself: the UI decides when (UX brief), so the user's switch "check automatically" is the only trigger at start-up.
- Commands run off the main thread (async Tauri commands) and are safe to call concurrently: a second `update_download` while one runs waits for the same download.
- In the browser build and the Playwright e2e, `ui/src/api/updates.ts` is fed by the stubbed Tauri IPC; when the Tauri IPC is absent, `appInfo()` resolves `{ version: "0.0.0-dev", updatable: false }` and the update UI stays hidden.
