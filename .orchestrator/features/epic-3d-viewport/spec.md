# 3D viewport — feature spec

Epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67). Requirements: **VP-01**, **VP-02**, **VP-03**, **VP-04**, **GEO-05**, **GEO-07** (`docs/specifications.md` §6, §13.2), with NFR-02 (≥ 60 fps orbiting the reference model) and NFR-03 (< 100 ms feedback after a typical edit). Milestone: Phase 1 – MVP.

## Where the epic stands

| Sub-issue | Requirements | State |
| --- | --- | --- |
| [#68](https://github.com/GuillaumeOj/BIMCraft/issues/68) wgpu renderer with camera-relative origin | VP-04, GEO-07 | **Closed**: `crates/render`, ADR 0020 (on this branch). |
| [#69](https://github.com/GuillaumeOj/BIMCraft/issues/69) Incremental tessellation | GEO-05 | **Closed**: `crates/scene`, ADR 0022/0023 (on `main`, PR #345; brought into the integration branch before the first task). |
| [#70](https://github.com/GuillaumeOj/BIMCraft/issues/70) Instancing, culling and LOD switching | VP-02, NFR-02 | Open: this feature. |
| [#71](https://github.com/GuillaumeOj/BIMCraft/issues/71) Picking, snapping and drag previews in Rust | VP-01, NFR-03 | Open: this feature. |
| [#72](https://github.com/GuillaumeOj/BIMCraft/issues/72) Section box, clipping planes, isolate and hide | VP-03 | Open: this feature. |

VP-04, GEO-07 and GEO-05 are already covered by tests. This feature keeps them covered (every new path that maps between screen and world works in `f64` relative to the camera, ADR 0020) and closes VP-01, VP-02 and VP-03.

## Goal

The 3D view of the application shows the model and lets the user work in it. The renderer runs as WebAssembly inside the window, on a `<canvas>` with WebGPU. The core sends it the meshes of the model as binary buffers, and only what changed after each edit. Everything that happens at pointer speed runs in Rust in the viewport module: orbit, pan and zoom, picking, snapping, drag previews, section-box handles. Only finished actions (move these elements by this vector, create this block) go to the core through the command API. Large models stay smooth through instancing, frustum and occlusion culling and level-of-detail switching. Users can cut the model with a section box and clipping planes, and hide or isolate elements by category, discipline or selection.

## What users get (MVP)

1. **A live 3D view** in the shell's viewport slot: the model's elements, shaded, updated within NFR-03 after every command, undo and redo.
2. **Navigation**: orbit, pan, zoom (mouse, trackpad and keyboard), zoom to fit, zoom to selection, preset views (top, front, side, isometric); perspective or parallel projection.
3. **Selection**: hover highlight and click to select (Shift adds, Ctrl/Cmd toggles, Esc clears), picked in Rust (VP-01). The selection is announced to screen readers.
4. **Generic blocks** (decision **D1**, needs the user's OK): a block element (IFC `IfcBuildingElementProxy`) the user places with two snapped clicks and a height. It is the first element the core schema can draw; walls, slabs and columns come with their epics and reuse everything here.
5. **Move with snapping**: drag selected elements, or use the Move tool (base point, then target point), with snapping to endpoints, midpoints, edges, faces and the ground plane, a live preview drawn by the renderer, and typed distances. Dropping sends one undoable command.
6. **Section box and clipping planes** (designed in board 3.1): a box with face handles, extents editable in the View panel, cut faces tinted; one or more clipping planes.
7. **Visibility** (designed in board 3.2): hide and isolate by category, by discipline or by selection; end isolate and reset visibility. Visibility is view state, not model state (UI-01).

Out of scope: WebGL2 fallback (VP-05, V1: on a webview without WebGPU, notably Linux WebKitGTK, the viewport shows an explanatory message instead of the 3D view), GPU picking within 16 ms (VP-06, V1: picking here is on the CPU), point clouds (VP-07), presentation styles (VP-08: shaded only, no edges or textures; the "Shaded with edges" menu of the boards shows "Shaded" only), cut caps (cut faces are tinted back faces, not capped solids), saved views and view persistence, plan and elevation views (NFR-04), schema-declared coarse representations as LOD levels (the renderer derives its own coarse levels), the command palette (UI-03: every action gets a keyboard shortcut now and joins the palette when it exists), IFC import/export of blocks (only the mapping table entry).

## Delivery: one stacked PR per open sub-issue

| # | Sub-issue | Delivers | User docs |
| --- | --- | --- | --- |
| 1 | #70 VP-02 | Instancing, frustum culling, LOD and small-feature culling, occlusion culling in `render`; the synthetic reference model and the frame-time benchmark; ADR 0024. | N/A (internal change) |
| 2 | #71 VP-01 | The scene feed (element-local meshes and placements, the `scene-wire` binary format, the host's `scene` channel), the block element and the `elements.move` command, the `viewport` crate (camera controller, picking, snapping, previews, overlays) compiled to wasm, the viewport in the UI with navigation, selection, place-block and move; e2e; ADR 0025. | `navigate-the-3d-view`, `select-and-move-elements`, `place-a-block` (EN + FR) |
| 3 | #72 VP-03 | Clipping planes, section box, cut-face tint and visibility in `render` and `viewport`; the section box and visibility UI of boards 3.1 and 3.2; e2e; ADR 0026. | `cut-the-model-with-a-section-box`, `hide-and-isolate-elements` (EN + FR) |

The wasm embedding and the binary channel have no sub-issue of their own; they ship in PR 2 with #71, which cannot be tested without them (decision **D2**). Merge order on the integration branch follows the stack: the lead holds merges so that each PR ends at a known commit and names the three boundary commits in the PR request; the stack goes out with `bun scripts/push-pr.ts --stack` (`Closes #70` / `#71` / `#72`, milestone, project). Commits reference their issue and requirement IDs (`feat(render): instance objects that share a mesh (VP-02, Refs #70)`). The epic #67 closes when the last PR merges. `.orchestrator/` does not ship: the lead removes this feature's folder in a final commit before the PR request.

## Decisions

### D1. A generic block element so the viewport has something to show (user's OK needed)

The core schema has no element with a representation yet (ADR 0023: "No entity of the core schema has a representation yet"), so the real application would show an empty view and NFR-03 ("move a wall") could not be measured or tested end to end. Proposal:

- New core schema entity **`Block`** (kind `element`; IFC `IfcBuildingElementProxy`, mapping table entry only): attributes `location` (a point, three lengths, metres), `rotation` (plane angle about the vertical axis), `size_x`, `size_y`, `size_z` (positive lengths). One representation: context `body`, generator `block::body`, a box made with the existing extrusion (`geom`, ADR 0021). Namespace version bump, snapshot and regenerated files per `docs/dev/schema-versions.md`; the backend's plan settles how a point is stored with the existing value types.
- The `block::body` generator is built into the `scene` crate (`scene::builtin`, wasm-compatible, so the browser host reuses it); the desktop host registers it.
- Category = the entity's name (`Block`), discipline = its schema namespace (`core`) until discipline crates exist (D7). The design's Walls, Slabs and Columns appear as soon as those entities exist.
- Alternative: no block, the viewport shows only test fixtures until the walls epic. Rejected: VP-01 and NFR-03 would be untested in the real app, and the user guide would describe a feature no user can try.

### D2. The viewport runs in the webview as WebAssembly; interaction never leaves Rust (VP-01)

- New crate **`viewport`** (layer 5, beside `render`; `scripts/wasm-crates.txt`; not critical, ≥ 90 % lines). It owns a `render::Renderer` on a canvas surface, the scene cache, the camera controller, picking, snapping, previews, overlays, the section box and visibility. Everything except the browser glue is plain Rust tested natively: the glue (`wasm-bindgen`, `web-sys`) lives in one `#[cfg(target_arch = "wasm32")]` module kept thin. Pointer, wheel and keyboard listeners are attached to the canvas by the wasm module itself; React only mounts the canvas, feeds scene bytes, sends view commands (show the section box, isolate…) and listens to a few coarse events (selection changed, an intent to commit, an announcement for the live region). No pointer move ever crosses into React state.
- The frame loop is `requestAnimationFrame` driven from Rust, and draws only when something changed (camera, scene, preview, visibility, resize).
- Finished actions leave the viewport as **intents** (`move`, `place_block`), which the UI sends as commands through the command API (UI-01, API-01). The core re-meshes the elements (GEO-05) and the change comes back through the scene channel, which replaces the preview.
- Build: `scripts/build-viewport.sh` runs `cargo build -p viewport --target wasm32-unknown-unknown --release` then `wasm-bindgen --target web` into `ui/src/viewport/pkg/` (generated, git-ignored, ignored by Biome and typos). The UI build, the Vitest run (which mocks the module) and the Playwright web server call it first. `wasm-bindgen-cli` is pinned in `scripts/tool-versions.env` at the version of `wasm-bindgen` in `Cargo.lock` (0.2.129), with its `# renovate:` comment, installed by `scripts/setup.sh` and CI; the UI and e2e CI jobs gain the Rust toolchain and the `wasm32-unknown-unknown` target. New direct dependencies `wasm-bindgen`, `web-sys`, `js-sys`, `wasm-bindgen-futures` are already in `Cargo.lock` through `wgpu`; the commit body justifies them (AGENTS.md §4).
- **ADR 0025 "Viewport in WebAssembly with a binary scene channel"** records D2–D5.

### D3. Element-local meshes and placements, so instancing is possible (GEO-05 kept)

- A generator returns its solids in the element's local frame plus an `f64` rigid placement (rotation and translation); `ElementMesh` gains the placement. Existing behaviour is kept: same staleness rules, same `SceneUpdate`, same counts of re-meshed elements (the #69 tests stay green, adapted only for the new field).
- The host converts each element mesh to a render mesh (`f32` vertices around an `f64` origin, ADR 0020) and **deduplicates meshes by content hash**: elements with identical local geometry share one mesh, which the renderer instances (D4). Moving an element changes only its placement: the delta carries an instance record and no mesh.

### D4. Rendering large models (VP-02, NFR-02, #70)

- **Instancing**: objects that share a mesh are drawn with one instanced draw call from a per-frame instance buffer of camera-relative `f32` model-view matrices, computed in `f64` on the CPU (ADR 0020 unchanged; the "Consequences" paragraph that announced this is honoured).
- **Frustum culling** on the CPU, in `f64` camera-relative coordinates, against each object's world bounding sphere and box, through a bounding volume hierarchy rebuilt incrementally when objects move.
- **Level of detail**: each mesh gets a coarse level derived by the renderer (its oriented bounding box, 12 triangles); an object whose projected size falls under a threshold in pixels draws its coarse level, and under about one pixel it is not drawn (small-feature culling). Thresholds are renderer settings with tested defaults; switching has hysteresis so objects do not flicker.
- **Occlusion culling**: GPU occlusion queries (`wgpu::QueryType::Occlusion`, available on WebGPU and native) on the bounding boxes of spatial clusters, with the previous frame's results (temporal coherence) and a conservative rule: a cluster not yet tested, or newly in view, is drawn. The backend's plan fixes the clustering and the query budget per frame.
- **Reference model**: a deterministic synthetic generator of a 10-storey office building of about 100,000 elements (columns, beams, slabs, walls, windows and doors as boxes and extrusions, heavy repetition, as in spec §14), in the `render` crate's bench support (test-only code, no schema needed).
- **Benchmarks, tracked**: criterion benches `render/benches/frame.rs` (CPU frame preparation on the reference model while orbiting: culling, LOD, instance buffers; target < 4 ms median, leaving the GPU the rest of the 16.7 ms) and, when a hardware adapter is present, GPU frame time. A non-blocking scheduled CI job runs the benches and uploads criterion's output as an artefact; `docs/dev/performance.md` records the method and the reference numbers. Tests assert behaviour, not time: culling and LOD decisions are checked on fixed cameras (how many objects drawn, which level), and a GPU test checks that the culled frame equals the unculled frame pixel by pixel.
- **ADR 0024 "Instancing, culling and level of detail"**.

### D5. Picking, snapping and previews in Rust (VP-01, NFR-03, #71)

- **Picking** on the CPU in `f64`: the ray through the pointer is built relative to the camera, tested against the object hierarchy (D4), then against the triangles of candidate meshes in each mesh's local frame (`f32` vertices promoted to `f64`). It honours visibility and clipping (D6): hidden or clipped-away geometry is never picked. Result: element id, world point, face normal.
- **Snapping**: candidates within a tolerance in pixels (default 10 px), by priority: endpoint (mesh vertex), midpoint of an edge, nearest point on an edge, point on a face, then the ground plane (z = 0 of the project; the active level once levels exist). The renderer draws the snap marker; the viewport announces its kind ("Endpoint", "Midpoint"…) to the live region at a throttled rate. Snapping can be suspended while a key is held (Alt).
- **Previews and temporary geometry** are drawn by the renderer in an overlay pass: ghosts of the moved elements, the rubber-band line and its distance, the block outline being placed, the section box and its handles, snap markers, hover and selection highlights. Nothing of this is model state.
- **Typed input**: during a move or a placement, typing a number sets the distance along the current direction (Enter confirms), so every pointer action has a keyboard path (UI-06).
- **NFR-03 path**: drop → `elements.move` → transaction → scene update (one element re-meshed, or only its placement) → delta → viewport → frame. A host test counts what is re-meshed and sent; a criterion bench in the host measures the core part on the reference-size model (target < 50 ms, leaving room for IPC and the frame).

### D6. Clipping and visibility (VP-03, #72)

- **Clipping planes** in the fragment shader: up to 6 planes for the section box plus up to 6 user planes, passed camera-relative (`f64` on the CPU, `f32` uniform). Cut faces: back faces that become visible through a cut are drawn in a flat tint (the boards' "Cut faces: Tinted"). The section box is an oriented box (axis-aligned with the project for the MVP), edited by dragging face handles in the viewport (with snapping, Shift for 100 mm steps as in board 3.1) or by typing extents in the View panel; "Fit box to selection", "Reset box", "Turn off".
- **Visibility**: per-object flags computed from three inputs, in this order: hidden categories, hidden disciplines, hidden selection; then isolation (by categories, disciplines or selection) shows only what is isolated. Hidden objects are neither drawn nor picked nor snapped to. Esc or "End isolate" ends isolation; "Reset visibility" clears everything.
- Visibility and the section box are view state held by the viewport, mirrored to the UI for its panels; they are lost when the window closes (saved views come later).
- **ADR 0026 "Clipping and visibility in the viewport"** (short).

### D7. The binary scene channel

- New crate **`scene-wire`** (layer 1 with `platform`, depends on `thiserror` only; wasm; **critical**, ≥ 98 % lines, `proptest` and a `cargo fuzz` target: it parses IPC input, ENG-02, SEC). It defines the versioned binary format of a scene delta (api-contract.md) with an encoder and a validating decoder: counts, sizes and offsets checked against the buffer, finite values, ranges of ADR 0020, total size capped; errors carry no coordinates or model content (SEC-03).
- The host keeps one `Scene` per document (view: `body`, the finest level of detail), applies every change event to it, and serves deltas through a new Tauri command **`scene`** that returns raw bytes (`tauri::ipc::Response`, an `ArrayBuffer` in JavaScript). The UI fetches a delta after each `model.changed` notification and at start, and hands the bytes to the viewport untouched: JavaScript never parses geometry.
- Element records carry the element id (16 bytes), its category and discipline (as indices into a string table in the same delta), its mesh id and its placement. Categories and disciplines are names from the schema, never user text.
- Why not JSON-RPC: geometry as JSON is several times larger and slow to parse; spec §14's risk table asks for a binary channel. The JSON-RPC API is unchanged apart from `elements.move` (api-contract.md).

## Test plan

- **Rust**: unit and property tests in `render`, `viewport`, `scene`, `scene-wire` (fuzz target + seeds), `api` (`elements.move`), `host-desktop` (scene channel: initial delta, delta after create/move/delete/undo/redo, counts of re-meshed elements and of meshes sent). GPU tests (lavapipe in CI, never skipped): instanced frame equals non-instanced frame; culled frame equals unculled frame; clipped frame against a reference; overlay pass draws on top. Picking and snapping tested natively on fixed scenes and cameras, including 10⁶ m and 10⁷ m from the origin (no loss of precision, ADR 0020).
- **UI**: Vitest + Testing Library for the viewport component (wasm module mocked), the scene feed in `ui/src/api/` (critical: 98 % lines and branches), panels and popovers; strings in EN and FR.
- **e2e (Playwright)**: the fake core answers the `scene` command with fixture bytes produced by a Rust test (`UPDATE_SNAPSHOTS=1` regenerates them; the test fails when they are stale). Flows: navigate and select; place a block; move it with a snap and a typed distance, undo; section box on and dragged; hide a category, isolate the selection, end isolate; the no-WebGPU message. Chromium runs with WebGPU on SwiftShader (`--enable-unsafe-webgpu` and the SwiftShader adapter flags); the frontend proves this works on CI's Linux runner in its first task, and tells the lead at once if it does not.
- Coverage: 90 % overall; 98 % in critical crates (`scene`, `scene-wire`, `api`) and in `ui/src/api/`.

## Risks

- **WebGPU in the webviews**: available in WebView2 (Windows) and in WKWebView on recent macOS; not in WebKitGTK (Linux) until VP-05. The no-WebGPU message names this and links the user guide.
- **WebGPU in CI's headless Chromium**: see the test plan. If SwiftShader does not work on the Linux runner, the lead picks a fallback (run the viewport e2e specs on the macOS runner, where Chromium's WebGPU uses Metal; or another adapter flag) before F1 goes further.
- **Bundle size**: wgpu in wasm is about 1–2 MB; the module is loaded lazily when the viewport mounts.
- **Merge conflicts** on `Cargo.toml`, `Cargo.lock` and `scripts/*.txt` between parallel backend tasks: small; owners sync the integration branch and resolve.

## Design

The section box (3.1) and visibility (3.2) boards exist on page `3 · Architecture modelling`. The UX and UI tasks add the missing states (navigation controls, selection, place block, move and snapping, clipping planes, empty view, loading, no WebGPU): see `ux-brief.md`. The design file is the project file `/Users/guillaume/Library/CloudStorage/GoogleDrive-guillaume.ojardias@gmail.com/My Drive/BIMCraft.figcraft` (live, unversioned, shared: re-read before changing, change only your boards, commit nothing of it). New boards go on page `3 · Architecture modelling` from 3.21 on, light and dark, `Mode`/`Core` variables, `Brand guide` components, French strings in the boards. PNG exports and a `README.md` go in `.orchestrator/features/epic-3d-viewport/design/`. Deviations from the design are listed in the PR.

## Tasks (created after approval)

The lead first brings `main` into the integration branch (it lacks `crates/scene`).

| Id | Node | Task | Depends on | PR |
| --- | --- | --- | --- | --- |
| B1 | back | `scene-wire` crate: format, encoder, validating decoder, proptest, fuzz target | — | 2 |
| B2 | back | `render`: instancing, frustum culling, LOD and small-feature culling, reference model, frame bench, scheduled bench job, ADR 0024 (plan first, opus) | — | 1 |
| B3 | back | `render`: occlusion culling (opus) | B2 | 1 |
| B4 | back | `scene` local meshes and placements, `Block` entity and `block::body`, `elements.move`, host scene channel with dedup, NFR-03 bench (plan first) | B1 | 2 |
| B5 | back | `render`: overlay pass and per-object highlight/ghost states | B3 | 2 |
| B6 | back | `viewport` crate: scene cache, camera controller, picking, snapping, previews, intents, typed input (plan first, opus) | B1, B5 | 2 |
| B7 | back | `viewport` wasm glue, `scripts/build-viewport.sh`, tool pin, CI jobs, ADR 0025 | B6 | 2 |
| B8 | back | `render` + `viewport`: clipping planes, section box, cut tint, visibility, ADR 0026 | B7 | 3 |
| X1 | ux | Flows and wireframes for the missing viewport states (`ux-brief.md`) | — | — |
| U1 | ui | High-fidelity boards, light and dark, exports | X1 | — |
| F1 | front | Viewport in the shell: lazy wasm mount, scene feed in `ui/src/api/`, navigation controls, selection, place block, move, live region, no-WebGPU state, i18n, tests, e2e (WebGPU on CI first), user docs | B4, B7, U1 | 2 |
| F2 | front | Section box and visibility UI (boards 3.1, 3.2), shortcuts, tests, e2e, user docs | F1, B8 | 3 |
