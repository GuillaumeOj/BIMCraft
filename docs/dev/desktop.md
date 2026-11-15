# Desktop window

The desktop program, `fabrilo`, is a [Tauri 2](https://tauri.app) window around the web interface in `ui/`, with the document in the same process (spec §13, `UI-01`). The `desktop-app` crate is the program and opens the window; the `host-desktop` crate holds the document and the IPC, so they build and are tested without a webview.

## How the window reaches the core

- `host_desktop::DocumentHost` holds one document and its JSON-RPC server (`api::jsonrpc::Server`, see [api.md](api.md)). Every message it handles has the `Ui` origin: the window cannot claim another.
- The window sends each JSON-RPC message through the Tauri command `api` (`host_desktop::ipc::COMMAND`) as `{ "message": "<JSON text>" }` and gets the response text back, or `null` for a notification.
- `model.changed` notifications reach every window as the Tauri event `api://notification` (`ipc::NOTIFICATION`), whose payload is the notification text.
- The window may call only that command, the `scene` command of the 3D view (`ipc::SCENE_COMMAND`) and the update commands below, and listen to events: `crates/desktop-app/capabilities/main.json` grants nothing else, and its `build.rs` declares only those application commands (`host_desktop::ipc::COMMANDS`).
- The window may also open the user guide in the system browser, for the help links of the 3D view: `main.json` grants `opener:allow-open-url` from Tauri's opener plugin (`tauri-plugin-opener`, registered in `crates/desktop-app/src/main.rs`) and nothing else of it: no other URL scheme, no path opening, no reveal in the file manager. Its scope allows only `https://github.com/GuillaumeOj/BIMCraft/blob/main/docs/user/*` and denies any URL containing `..` or an encoded dot (`%2e`), so a link cannot climb out of that folder. Every other URL is refused by the plugin. The UI calls it as `invoke("plugin:opener|open_url", { url })` from `@tauri-apps/api/core`, inside `ui/src/api/` only. When the user guide moves, change the scope with the links.
- `crates/host-desktop/tests/ipc.rs` runs the transport conformance suite (`API-04`) through Tauri's IPC with its mock runtime, so it needs no window or webview. It also checks that the window reaches the update commands and no other command. The mock runtime does not apply capabilities, so `desktop-app`'s tests check that `capabilities/main.json` grants, and `build.rs` declares, exactly `ipc::COMMANDS`, the two event permissions and the user-guide opener: the capability is what keeps the updater plugin's own commands out of reach.

The UI side of this channel is the typed client in `ui/src/api/` (`tauriTransport`, `ApiClient`), the only code allowed to import `@tauri-apps/*`.

## Updates

The program updates itself from two channels, stable and beta (`ENG-05`), through four Tauri commands in `host_desktop::update`. Arguments and results are JSON with camelCase keys; every failure rejects with `{ code, message }`, where `code` is one of `unavailable`, `network`, `manifest`, `signature`, `no_update`, `not_downloaded` or `install` and `message` is English for logs (the window shows its own text by code).

| Command | Arguments | Result | Errors |
| --- | --- | --- | --- |
| `app_info` | none | `{ version, updatable }` | never fails |
| `update_check` | `{ channel: "stable" \| "beta" }` | `{ version, date, notes }` or `null` | `unavailable`, `network`, `manifest` |
| `update_download` | none | `null` once downloaded and verified | `unavailable`, `no_update`, `network`, `signature` |
| `update_install` | none | does not return: the program restarts | `unavailable`, `not_downloaded`, `install` |

- **Channels.** The window names a channel, never an address: `Channel::endpoint` maps `stable` and `beta` to `https://guillaumeoj.github.io/BIMCraft/updates/stable.json` and `…/beta.json`, the manifests that the release workflow publishes on GitHub Pages. Any other channel fails Tauri's argument check. Only a higher version is offered, never a downgrade.
- **State.** `UpdateHost` keeps the offer of the latest check asked: a check answered after a later one is returned to its caller but not kept, and a failed check keeps the previous offer. A new version replaces the offer and drops its package; the same version again keeps a package downloaded or downloading, and otherwise takes the new answer (a release re-signed for the same version, say). A download whose offer a check replaced fails with `no_update` and stops reporting progress. `update_download` emits `update://progress` events (`{ downloaded, total }`) at most every 100 ms and once at the end; a second call while one runs waits for it and shares its result, and a call after success returns at once. `update_install` needs a completed download; a second call while it installs does nothing.
- **Nothing starts by itself.** The host never checks, downloads or installs unless the window asks; the window decides when, from the user's settings.
- **Who can update.** `app_info.updatable` is `true` only in release builds whose configuration has an updater public key (`plugins.updater.pubkey`, merged in by the release workflow): debug builds, CI windows and local release builds never update themselves, and every update command then fails with `unavailable`. The updater plugin is only registered in that case.
- **Where the code is.** `host-desktop` holds the commands and the state, behind the `Updates` and `Package` traits, and is unit-tested with a fake of them (no network, no installer). `crates/desktop-app/src/updater.rs` implements the traits on `tauri-plugin-updater`, with a 30 s timeout on the check and on each stall of a download; it is the only code that reaches the network or the installers, and only in an updatable build. The plugin verifies each package's signature against the public key before it is installed, so a package that does not verify is never installed, whatever the transport. `tauri.conf.json` sets `plugins.updater.requireSignedVersion`: the version a package was signed for must be the one the manifest announces, so a tampered manifest cannot pair a new version number with an older signed package (a downgrade); every release must therefore be signed with its version recorded. On Windows the NSIS installer runs in passive mode and restarts the program; on macOS the app is replaced in place, then `update_install` answers and the program restarts.
- **Why not JSON-RPC.** Updating the program is not a document operation. The JSON-RPC API also serves plugins, scripts and network clients (see [api.md](api.md)), which must never be able to download or install software; Tauri commands only reach the window.
- **Why not the plugin's JavaScript API.** The window gets none of the updater plugin's commands (and no process plugin to restart): through them it could pick its own endpoint or install without our checks. The capability grants our four commands only; `ui/src/api/updates.ts` is the only UI module that calls them.

## Running it

1. Install the Linux libraries if you are on Linux: `scripts/linux-desktop-deps.sh` (Debian and Ubuntu; GTK 3 and WebKitGTK 4.1). macOS and Windows need nothing beyond Rust: the window uses WKWebView and WebView2.
2. Start the UI development server: `cd ui && bun run dev`.
3. In another terminal: `cargo run -p desktop-app`. The window loads the development server, so UI changes reload live.

To build the program with the UI inside it, as a release does: `scripts/check-all.sh desktop`. It bundles `ui/dist` and builds `desktop-app` with `tauri/custom-protocol`, which embeds the bundle. CI runs it in the `rust` job on Linux, macOS and Windows.

## Configuration

- `crates/desktop-app/tauri.conf.json`: window size, the content security policy and the icons. The policy allows `'wasm-unsafe-eval'` for scripts, which WebAssembly needs to compile the viewport module ([viewport.md](viewport.md)); it allows no JavaScript `eval`.
- `crates/desktop-app/icons/`: the app tile of the FigCraft design ("B" on verdigris), at 512 px (`icon.png`) and 256 px (`icon.ico`, needed to build on Windows).
- `crates/desktop-app/gen/` and `crates/desktop-app/permissions/autogenerated/` are written by `tauri-build` on each build and are not committed.

## Dependencies

`tauri-plugin-updater` is built without its default features: `rustls-tls` (no native TLS or OpenSSL) and `system-proxy` (offices behind a proxy), without `zip`, which only Tauri 1-style update packages need. Its HTTPS client brings `webpki-root-certs` (Mozilla's root certificates, CDLA-Permissive-2.0) for wasm32 only; `deny.toml` allows that licence for that crate.

Tauri 2 uses GTK 3 on Linux, whose Rust bindings bring two advisories that cannot be fixed here: `proc-macro-error` is unmaintained (RUSTSEC-2024-0370) and `glib` 0.18 has an unsound iterator nobody calls (RUSTSEC-2024-0429). Both are ignored, with their reasons, in `deny.toml` and `.cargo/audit.toml`; remove the entries once Tauri moves off GTK 3. `deny.toml` also skips duplicate-version warnings inside Tauri's own dependency tree.
