# wgpu renderer with camera-relative origin — feature spec

Issue [#68](https://github.com/GuillaumeOj/BIMCraft/issues/68), part of the 3D viewport epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67). Requirements: **VP-04**, **GEO-07** (`docs/specifications.md` §5, §13.2). Milestone: Phase 1 – MVP.

## Goal

Create the `render` crate: a wgpu renderer that draws triangle meshes whose coordinates are stored in double precision, through a camera whose position is also double precision. Every draw is expressed relative to the camera on the CPU in `f64` before anything reaches the GPU in `f32`. A model placed 10⁶ m (and 10⁷ m) from the world origin renders exactly like the same model placed at the origin: no vertex jitter, no wobble while the camera moves.

## Scope

In scope:

- New crate `crates/render` (already declared in layer 5 of `scripts/crate-layers.txt`; spec §3 lists it as "wgpu renderer, native and WebGPU").
- Camera (orbit-ready look-at camera, perspective and orthographic projections), mesh upload, objects placed in the world, one shaded opaque pass into any `wgpu::TextureView`.
- Camera-relative transform maths, unit-tested on the CPU.
- A GPU **visual test**: offscreen rendering at the origin, at 10⁶ m and at 10⁷ m, compared pixel by pixel, plus a negative control that proves the test sees jitter when the camera-relative step is turned off.
- Builds for native and `wasm32-unknown-unknown` (WebGPU).
- ADR 0020 "Camera-relative rendering".

Out of scope (later issues of the epic):

- Wiring the renderer into the UI viewport (canvas, wasm-bindgen glue, input handling). The issue is an internal change (“User docs: N/A”); the UI gets the renderer once there is model geometry to show and interaction to drive it (#71). **No UI, UX or frontend task in this feature.**
- Tessellation of model elements (#69), instancing, culling, LOD (#70), picking and snapping (#71), clipping (#72), WebGL2 fallback (VP-05).

## Decisions

### D1. Camera-relative transforms computed in f64 on the CPU

- Mesh vertex positions are `f32`, **relative to a per-mesh origin stored in `f64`**. A helper builds a mesh from world `f64` positions and picks the centre of their bounding box as the mesh origin, so local coordinates stay small (a 100 m element keeps sub-10 µm precision in `f32`).
- An object places a mesh in the world with an `f64` placement (rotation + translation).
- Each frame, for each object, the renderer computes in `f64` the model-view matrix of the object **with the camera at the origin**: `V_rot · [R_obj | R_obj·o_mesh + t_obj − eye]`. Only this product is converted to `f32` and uploaded. The view matrix sent to the GPU never contains the camera's world position. The large terms cancel in `f64`, where the cancellation error at 10⁷ m is about 2·10⁻⁹ m.
- The vertex shader computes `clip = P · MV · v_local` in `f32`, with values whose magnitude is the camera distance, not the world coordinate.
- Why not a floating origin snapped to a grid and re-based when the camera moves away: it needs a threshold, re-uploads on re-base and still loses precision between re-bases. Why not "high/low" split vertices emulating double precision in the shader: twice the vertex memory and shader cost for no gain here. Per-object matrices are recomputed each frame, which costs O(objects) on the CPU; issue #70 (instancing) may move them to an instance buffer, behind the same public API.
- The renderer has no switch to a world-origin pipeline. The naive f32 model-view exists only as a `#[cfg(test)]` function, for the CPU precision tests. The GPU negative control of D4 goes through the public API.

### D2. Depth precision: reverse-Z

Depth uses `Depth32Float` with reverse-Z (near → 1, far → 0, compare `Greater`) and an infinite far plane for the perspective projection. This keeps depth resolution at large camera distances and avoids z-fighting between coplanar-adjacent faces of large models. Orthographic projection uses reverse-Z with explicit near and far.

### D3. Rendering scope

- One opaque pass: clear colour, depth test, back-face culling off (BIM solids from IFC are not always consistently oriented), per-object colour, one directional light with Lambert shading plus ambient, so tests can see faces. WGSL shaders.
- No MSAA (deterministic pixels for the visual test; antialiasing comes with the viewport work).
- The renderer is created from a `wgpu::Device`, `wgpu::Queue` and target `wgpu::TextureFormat` supplied by the host. It does not create windows or surfaces: the host does (desktop window or browser canvas, later). Async adapter/device requests stay with the host; the crate exposes a helper `async fn` to request a headless device for tests and tools, and never blocks (ARCH-02). Tests use `pollster` as a dev-dependency.
- Inputs are validated (SEC: meshes come from model files): finite numbers, index count multiple of 3, indices in range, non-empty, bounded sizes (counts that fit `u32` and the device limits), finite and non-degenerate camera. Typed `thiserror` errors; no panic on bad input. Error messages never contain coordinates (SEC-03).

### D4. The visual test (acceptance: “no jitter at 10⁶ m”)

Integration test `crates/render/tests/jitter.rs`, on the public API only (no hook, no cargo feature; `tests/` may use `std::fs`, ARCH-02), rendering offscreen (e.g. 256×256, `Rgba8Unorm`, read back to the CPU):

1. A test scene with fine detail: e.g. a comb of slats 5 mm wide, 5 mm apart, a few metres long, plus a cube, seen from about 3 m, so a 1-pixel shift is visible.
2. A **camera sweep**: N frames (≥ 16) where the eye moves by sub-millimetre steps (e.g. 0.1 mm) along a path. The sweep is rendered three times: scene and camera at the origin (reference), translated by (10⁶, 10⁶, 10²) m, and by (10⁷, 10⁷, 10²) m.
3. **Pass**: every translated frame matches the reference frame of the same step: max channel difference ≤ 1/255 and at most 0.01 % of pixels differing. (Bit-identical output is expected when translations are exactly representable; the tolerance only absorbs a rounding of the `f64` cancellation.)
4. **Negative control**: the same sweep at 10⁶ m, fed as a classic f32 pipeline would see it, must **fail** the criterion of step 3. The mesh has origin 0 and its world positions rounded to `f32`, which is a world-space f32 vertex buffer. The eye and target of each frame are rounded through `f32`. Vertices and camera snap to the 6.25 cm `f32` grid. This proves the test detects f32 world-coordinate error on the same scene and pipeline.
5. On failure, the test writes the differing frames as images under the target directory (e.g. `target/render-jitter/`) so a human can look at them. CI uploads that folder as an artefact on failure.

CPU unit tests in the crate complement it: project sample points through the exact `f32` operations of the shader and compare with an `f64` reference; at 10⁶ and 10⁷ m the screen error stays below 0.01 px, while the naive path exceeds 1 px.

**GPU in CI.** The test needs a GPU adapter; software adapters are fine: lavapipe (Mesa Vulkan) on Linux, WARP (DX12) on Windows, Metal on macOS runners. CI installs `mesa-vulkan-drivers` on Linux in the Rust and coverage jobs (unpinned distribution package, like `scripts/linux-desktop-deps.sh`). The test is **not** skipped when no adapter is found: it fails with a message that names the missing adapter (AGENTS §1.2). If one CI OS cannot get an adapter, the backend reports it to the lead, who brings it to the user.

### D5. Platform and dependencies

- `render` is added to `scripts/wasm-crates.txt`: it must build for `wasm32-unknown-unknown` (WebGPU backend) and it starts with `#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]` like the core crates. Trigonometry (projection, test paths) uses `libm` (ENG-04).
- New dependencies, exact versions, released ≥ 14 days ago, justified in the commit body (AGENTS §4): `wgpu` (default features off; WGSL plus the Metal, Vulkan, DX12 and WebGPU backends), `bytemuck` (vertex and uniform buffers as bytes). Dev-dependencies: `pollster` (block on async in tests), and an image encoder only if justified (a plain binary PPM writer is acceptable instead).
- No `glam`: `f64` points and vectors reuse `geom::Point3`/`geom::Vector3` (layer 2, allowed), and the few 3×3/4×4 operations live in a private module of the crate. The public API uses plain arrays and geom types, so no third-party maths type leaks.
- `cargo deny check` stays clean. If wgpu's tree needs a licence that is not in `deny.toml`, the backend reports it: the lead accepts a permissive licence (with a comment in `deny.toml`) and asks the user for anything else. Duplicate-version warnings from wgpu's tree may be skipped with a `skip-tree` entry and a reason, as done for tauri.
- `render` is not a critical area (AGENTS §5): ≥ 90 % lines, aiming higher. Every public item documented. Zero clippy warnings (pedantic).

### D6. Documentation

- ADR 0020 "Camera-relative rendering" in `docs/dev/adr/` (D1, D2, D4 summary, the alternatives rejected), added to the ADR index.
- `docs/specifications.md` is not changed (VP-04 and GEO-07 already state the requirement).
- User docs: **N/A (internal change)**, stated in the PR.

## Delivery

One pull request on `orch/issue-68/integration`, closing #68, with `VP-04`/`GEO-07` in commits and PR. The PR body states "User docs: N/A (internal change)" and shows the visual test result (frames at the origin and at 10⁶ m, and the negative control).

## Tasks (all backend)

| # | Owner | Task | Depends on |
| --- | --- | --- | --- |
| T1 | back | `render` crate skeleton (manifest, lints, layers, `wasm-crates.txt`), camera, placements, mesh data and validation, camera-relative matrix maths and the CPU precision tests (D1, D2, D3 validation, D4 CPU part); ADR 0020 | — |
| T2 | back | wgpu pipeline: WGSL shader, GPU mesh and object storage, render pass into a texture view, headless device helper, readback; the visual test `tests/jitter.rs` with its negative control; CI GPU adapter (lavapipe) and failure artefact; wasm32 build; `cargo deny` | T1 |

T1 runs with a plan first. Both tasks use Opus (new crate, numerical subtlety, GPU in CI).
