# Notes for ADR 0026 (B7), from B6

What the `viewport` crate (B6) decided that ADR 0026 "Viewport in WebAssembly with a binary scene channel" should record.

## MVP limits

- **Snapping examines at most 64 elements per pointer move** (`snap::MAX_CANDIDATES`): the elements whose bounds meet the tolerance rectangle under the pointer, nearest first (by the distance from the eye to the centre of their bounds). Their vertices and feature edges are scanned on the CPU. On a dense model, a vertex of the 65th-nearest element is not offered. At most 16 candidates are then tested for being hidden (a ray from the eye against the same elements); beyond that the snap falls back to the face or the ground. Revisit with GPU picking (VP-06).
- Picking is on the CPU (VP-06 is V1): the renderer's object hierarchy gives the candidates (`Renderer::objects_in_rect`, a frustum through a pixel rectangle, in `f64` relative to the eye), then the triangles of their meshes in each mesh's local frame.

## Behaviour the glue must respect

- `Viewport::tick()` returns `Tick { draw, again }`. Render a frame only when `draw` is true. Keep calling `tick` on every animation frame while `again` is true, even when nothing else happens: a move's ghosts wait up to 120 frames for the scene delta that shows the moved elements (then vanish, for a refused move), and a throttled announcement waits for its frame.
- Input positions are CSS pixels. The viewport works in device pixels (`resize(size_css, devicePixelRatio)`), and the snap tolerance is 10 CSS pixels.
- Wheel events: `pan: true` for a trackpad's two-finger scroll (no Ctrl), `false` for a mouse wheel or a pinch-as-wheel. `Pinch` takes the ratio of the spread since the last event.
- `Key::Alt` and the `alt` modifier on every event drive the snap suspension. `Blur` releases it.
- The section box, clipping planes and visibility commands (B8) are not part of `ViewCommand` yet: the glue must refuse them with a clear error until B8 adds them.
- Announcements: a snap kind never replaces a waiting action outcome ("Moved by…", "Block placed…"). `selection.cleared` is sent, not `selection.count` with 0.
- A scene delta that does not fit (`BaseMismatch`, `UnknownMesh`, `MeshInUse`) or does not decode changes nothing: `applyScene` throws, and the UI asks again with `since: revision()`. One the renderer refuses drops the scene, emits `error{scene_rejected}`, and `revision()` becomes `null`, so the next request is a full scene.
