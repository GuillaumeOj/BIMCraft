# 0026. Viewport in WebAssembly with a binary scene channel

- Status: Accepted
- Requirements: VP-01, NFR-03 (also VP-02, GEO-05, UI-01, ARCH-01, ARCH-02, SEC-03)
- Issues: [#71](https://github.com/GuillaumeOj/BIMCraft/issues/71), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67)

## Context

The 3D viewport must follow the pointer at frame rate: hover highlights, snap markers, a move preview that tracks the cursor, orbit and pan. NFR-03 asks for moving an element in well under a frame budget plus the core's share. The UI is React in a webview (Tauri on the desktop, a browser for the web build); the core is Rust. [ADR 0020](0020-camera-relative-rendering.md) and [ADR 0025](0025-instancing-culling-and-level-of-detail.md) already fixed how the renderer draws: `f64` camera-relative transforms, instancing, culling.

Two questions remained: where the interaction code (picking, snapping, tools, camera) runs, and how geometry reaches the renderer. Doing the interaction in TypeScript would duplicate the geometry kernel's tolerances in a second language, put pointer events through React, and force picking against meshes held in JavaScript. Sending geometry as JSON-RPC would make it several times larger and slow to parse (specification §14's risk table).

## Decision

**The viewport runs in the webview as WebAssembly (spec D2).** The `viewport` crate holds everything that happens at pointer speed: the scene cache, the camera controller, picking, snapping, the tools (select, move, place a block) and the overlay previews, all as plain Rust that native tests drive with synthetic input. It owns a `render::Renderer` that draws into the canvas through a WebGPU surface. The crate is a core crate: it builds for `wasm32-unknown-unknown` (ARCH-01), touches no platform API (ARCH-02) and uses `libm` (ENG-04). One `#[cfg(target_arch = "wasm32")]` module, `crates/viewport/src/web/`, is the only code that knows the browser, and it only converts values, creates the surface, attaches listeners and schedules frames; every decision stays in the native-tested part.

**The canvas is the viewport's, React only mounts it.** The module attaches its own pointer, wheel and keyboard listeners to the canvas and removes them when the object is freed. A pointer move never enters React state. React sends *view commands* (zoom to fit, presets, projection, snapping, the tool) and listens to coarse *events*: the selection, the hovered element, the state of the tool, announcements for the live region (as keys the UI translates, UI-04), the label to draw next to a preview, and finished actions. The contract of the module's JavaScript interface is `.orchestrator`'s `api-contract.md` §2, recorded with the TypeScript wrapper in `ui/src/viewport/`.

**The frame loop is driven from Rust and draws only when something changed.** An input, a command, a scene delta, a resize or a theme change asks for one `requestAnimationFrame`; a frame draws if the viewport's state is dirty, and asks for the next one only while something is still due (a waiting preview, a throttled announcement). An idle viewport costs nothing.

**Finished actions are intents (spec D5).** A drop of a moved selection or a placed block leaves the viewport as `{ kind: "move" | "place_block", … }`. The UI turns it into a command of the command API (`elements.move`, `model.create`, UI-01), the core re-meshes what changed (GEO-05), and the change returns through the scene channel, where the new geometry replaces the preview. The viewport never changes the model.

**Picking, snapping and previews are CPU work in `f64`, relative to the eye (spec D5).** Rays, snap candidates and previews use the same camera-relative scheme as rendering, so a model ten million metres from the origin picks as it does at the origin. Typing a number during a move or a placement sets the distance, so every pointer action has a keyboard path (UI-06).

**The scene reaches the viewport as a versioned binary delta (spec D3, D7).** The `scene-wire` crate defines the format and a validating decoder (counts, sizes and offsets checked against the buffer, finite values, the ranges of ADR 0020, a total size cap; no coordinates in errors). The host serves deltas through a Tauri command `scene` that returns raw bytes; the UI hands them to `Viewport.applyScene` untouched, so JavaScript never parses geometry. Meshes are element-local and deduplicated by content: moving an element sends one placement and no mesh, which is what makes NFR-03 reachable. The JSON-RPC API is unchanged apart from `elements.move`.

**The canvas is drawn through an sRGB twin of its format.** The renderer works in linear light and expects an sRGB colour target; a browser canvas offers only `bgra8unorm` or `rgba8unorm`. The glue renders into an offscreen texture of the sRGB twin and copies it to the canvas texture, a copy WebGPU allows between formats that differ only in the transfer function. The cost is one full-frame copy, negligible next to the frame; the alternative, a second set of pipelines for a non-sRGB target, would duplicate the renderer's shading for one browser quirk.

**Hostile input and failure never panic.** Every value that crosses from JavaScript (commands, themes, tool names, sizes, ids, scene bytes) is validated on the Rust side and answered with a JavaScript error. wgpu's default error handler panics, so the glue installs its own: an out-of-memory error and a lost device become `error` events (`out_of_memory`, `device_lost`); other GPU errors are written to the console. A panic hook writes only the source location to the console, never the message, which could carry model content (SEC-03); the usual `console_error_panic_hook` crate would add a dependency for that one line. The page's event listener may call back into the viewport, so events are queued while the viewport is borrowed and delivered afterwards.

**The build is `scripts/build-viewport.sh`.** A release build of the crate for `wasm32-unknown-unknown`, then `wasm-bindgen --target web` into `ui/src/viewport/pkg/` (generated, git-ignored, skipped by Biome and typos). `wasm-bindgen-cli` must equal the `wasm-bindgen` crate in `Cargo.lock`, because the tool rejects a module built with another version: `WASM_BINDGEN_VERSION` in `scripts/tool-versions.env` pins it, a Renovate rule moves the CLI and the crates in one pull request, and `scripts/setup.sh` and CI install it. The UI build, the `dev` server, the typecheck and the Playwright web server run the script first; Vitest mocks the module and needs no Rust build. The UI and end-to-end CI jobs gain the Rust toolchain and the `wasm32-unknown-unknown` target.

New direct dependencies of `viewport`, for `wasm32` only: `wasm-bindgen`, `web-sys` (the features used: canvas, events, `console`), `js-sys` and `wasm-bindgen-futures`. They are the browser bindings wgpu's WebGPU backend already locks, at the same versions, so no copy is added to the build; they are maintained by the Rust and WebAssembly working group, MIT or Apache-2.0 (accepted by `deny.toml`), and build for `wasm32-unknown-unknown`, the only target they are used for.

## Consequences

- Pointer-speed interaction is tested natively and deterministically, with no browser: the glue is the only code only an end-to-end test reaches.
- The module is about 0.8 MB (250 KB gzipped) with the renderer; the UI loads it lazily, on first use of the viewport.
- The viewport needs WebGPU. `webgpuAvailable()` lets the UI show a no-WebGPU state; a WebGL fallback is a later feature (VP-05).
- A browser build needs a Rust toolchain and `wasm-bindgen-cli`, for contributors and for CI jobs that run the UI.
- Device loss is reported when the page next calls the viewport or a frame runs, not at once, because wgpu's callback cannot touch the page's objects; the UI treats the `device_lost` event as final and recreates the viewport.
