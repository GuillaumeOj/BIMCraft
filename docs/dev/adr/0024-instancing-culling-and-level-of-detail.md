# 0024. Instancing, culling and level of detail

- Status: Accepted
- Requirements: VP-02, NFR-02 (also VP-04, GEO-07, ARCH-01, ENG-04)
- Issues: [#70](https://github.com/GuillaumeOj/BIMCraft/issues/70), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67)

## Context

NFR-02 asks for 60 frames per second while orbiting the reference model of the specification (§14): a building of about 100,000 elements. Building models repeat a few element types many times: columns, beams, ceiling tiles, desks. Before this decision the renderer drew each object with its own draw call and its own slot in a dynamic uniform buffer, and drew every object every frame, whether it was in view, behind the camera or smaller than a pixel. 100,000 draw calls per frame is far beyond what WebGPU in a browser sustains, and most of them draw nothing visible.

[ADR 0020](0020-camera-relative-rendering.md) still holds: each object's model-view is computed on the CPU in `f64` relative to the camera, and only that result reaches the GPU in `f32`. Whatever is added here keeps that rule and runs on native GPUs and WebGPU alike (ARCH-01). The host deduplicates element meshes by content (spec D3), so objects that look alike share one mesh. Occlusion culling comes next ([#70](https://github.com/GuillaumeOj/BIMCraft/issues/70), task B3) and builds on what is decided here.

## Decision

**Instancing.** Objects that share a mesh are drawn by one instanced draw call per level of detail. Each frame the CPU writes one *instance record* per object drawn into a vertex buffer read once per instance (`VertexStepMode::Instance`). A record is 20 `f32` (80 bytes), at these shader locations:

| Floats | Location | Content |
| --- | --- | --- |
| 0–15 | 2–5 | Camera-relative model-view, column-major, computed in `f64` and rounded once (ADR 0020) |
| 16–19 | 6 | Linear RGBA colour |

The layout is one constant (`INSTANCE_FLOATS` in `crates/render/src/world.rs`) and one attribute list. Later passes extend the record after the colour (a state word for the highlight and ghost states of B5, for instance) without moving the model-view. Only the records of the objects drawn are written and uploaded. The buffer doubles when a frame needs more, so it never shrinks below the largest frame and never reallocates in steady state. The number of objects is bounded by the device's largest buffer divided by 80 bytes, within `u32` instance indices: 3.3 million under WebGPU's default 256 MiB.

**Deterministic draw order.** The frame is a list of *buckets*: a mesh, a level, and a range of records drawn by one call. Meshes come in id order, the fine bucket of a mesh before its coarse one, and in each bucket objects come in id order. The order depends neither on the hierarchy nor on culling, so a frame with culling is pixel for pixel the frame without it: two objects at exactly the same depth resolve the same way in both. A second pass (the overlay of B5, the occlusion tests of B3) can walk the same buckets.

**Bounding volume hierarchy.** A dynamic tree over the world bounding box of each object, as in Box2D (Erin Catto): one leaf per object, an insertion descends towards the sibling whose surface area grows least, and the nodes on the way up are refitted and rotated when one child is more than a level deeper than the other. Adding, moving and removing an object update the tree in `O(log n)`, without rebuilding it. A moved object whose bounds did not change leaves the tree alone. Nodes live in one vector with a free list. A query walks the tree with a test that classifies a box as outside, inside, or crossing with a state for its children. The frustum test is one such test; a ray test for picking (B6) is another, without restructuring.

**World bounds.** Each mesh keeps the box of its vertices in its local frame. An object's world box is that box placed by the object's rotation and translation in `f64`: its centre placed, its half-extents through the absolute rotation, which encloses the turned box exactly.

**Frustum culling** on the CPU, in `f64`. The planes of the view volume (left, right, bottom, top and near, plus far for an orthographic camera; a perspective camera has an infinite far plane) are built from the camera's view basis and projection. Each plane is stored as a world normal and an offset relative to the eye. A box is tested on its centre minus the eye, so the large coordinates of a model far from the origin cancel before anything else, as in ADR 0020. The traversal carries the mask of the planes a node still crosses: a node inside a plane drops that plane for its children, and a node inside every plane is accepted with its whole subtree without more tests. The test is conservative: a relative margin of 10⁻⁹ absorbs rounding, so a box is culled only when it is certainly out of view. A frame with culling equals the frame without it, pixel for pixel.

**Level of detail.** The renderer derives one coarse level per mesh: the box of its vertices in its local frame, 12 triangles (`MeshData::bounding_box`). Element meshes are built in the element's local frame (spec D3), so this is the element's oriented bounding box. Only meshes of more than 12 triangles get one; a box is its own coarse level. The *projected size* of an object is the diameter in pixels of the sphere that encloses its local box, around its placed centre, against the target's height:

- perspective: `2r · H / (2 tan(fov/2) · d)`, with `d` the distance in `f64` from the eye to the centre, and an infinite size when the eye is inside the sphere;
- orthographic: `2r · H / h`, with `h` the height of the view.

The sphere does not depend on the rotation, so turning an element does not change its level. An object draws its fine mesh at or above `coarse_below_px`, its coarse level below that, and nothing below `hide_below_px` (small-feature culling).

**Hysteresis.** Less detail is taken as soon as the size falls under a threshold; more detail only once the size reaches the threshold times `1 + hysteresis`. An object whose size wavers inside that band keeps its level, so it does not flicker while the camera moves. An object that is new, or was out of view in the previous frame, takes the level of its size without hysteresis. Changing the settings resets every object the same way.

**Settings** (`RenderSettings`, validated by `Renderer::set_settings`). Thresholds are between 0 and 10⁵ px, the hysteresis between 0 and 1. A threshold of 0 turns its feature off, and `frustum_culling: false` turns culling off; the pixel-comparison tests use these switches.

| Setting | Default | Why |
| --- | --- | --- |
| `frustum_culling` | on | It changes no pixel. |
| `coarse_below_px` | 24 px | Below about 24 px, the I-sections, doors, desks and chairs of a building are not told apart from their boxes. |
| `hide_below_px` | 1 px | Below a pixel an object covers at most one sample, which it would hit or miss by chance anyway. |
| `hysteresis` | 25 % | Wider than the change in projected size of one step of a smooth orbit or zoom. |

**Public API.** `add_mesh`, `remove_mesh`, `add_object`, `set_placement` and `remove_object` keep their signatures and meaning. `render` returns `FrameStats`: the counts of objects culled outside the view, culled as too small, drawn fine, drawn coarse, and the draw calls. `prepare_frame` runs the CPU part of a frame alone, for the benches and the tests of culling and levels. `drawn_level` tells the level an object got in the last prepared frame. `settings` and `set_settings` read and change the settings.

**Reference model and measurements.** `crates/render/tests/support/reference.rs` generates the reference model without randomness: 10 storeys of about 10,000 elements each, 100,150 in all, built from 15 meshes. Each storey has columns, beams, slab panels, a curtain wall, cellular offices with doors and furniture, an open plan, and a ceiling of 0.6 m tiles with lights and sprinkler heads. The model sits at a national-grid offset, (2.5·10⁶, 1.2·10⁶, 0) m. The tests, the unit tests of the crate and the benches share it. `cargo bench -p render --bench frame` measures the frame preparation while orbiting, and the GPU frame time on a hardware adapter. The method and the reference numbers are in [performance-baselines.md](../performance-baselines.md); a weekly workflow keeps criterion's output ([quality.md](../quality.md#benchmarks)). The target for the CPU preparation is a median under 4 ms, which leaves the GPU the rest of the 16.7 ms of a frame at 60 fps.

**Verification.** The tests check behaviour, not time:

- On fixed cameras the counts of objects culled, hidden, fine and coarse are exact. The orbit, aerial and inside counts of the reference model are pinned.
- The levels chosen while dollying out and back show the hysteresis.
- A fixed-seed property test checks that the hierarchy finds exactly what a brute-force scan finds, through any sequence of insertions, moves and removals.
- Unit tests check the frustum planes at 10⁷ m from the origin.
- GPU tests run on lavapipe in CI and are never skipped. Objects sharing a mesh draw the same pixels in one call as separate meshes do in 25. The culled frame equals the unculled frame on part of the reference model. A small detailed object draws exactly its bounding box. A sub-pixel object leaves the frame empty.

## Consequences

- A frame costs one draw call per mesh and level in view, not one per object; with the reference model, at most 30.
- The CPU still visits every object each frame to choose its level and write its record, but tests boxes only against the nodes of the hierarchy that cross the view. The cost grows with the objects in view, not with the size of the model.
- Picking (B6) can walk the same hierarchy with a ray. Occlusion culling (B3) adds a test per spatial cluster before the buckets are filled. Highlight and ghost states (B5) extend the instance record.
- What an object looks like no longer depends only on its own state: its projected size and its previous level matter. `drawn_level` and `FrameStats` make that observable.
- Meshes whose local frame is not the element's (a mesh built in world coordinates, for instance) get a coarse box aligned with the world rather than the element: still enclosing, only looser.
- Occlusion culling is not part of this decision: objects hidden behind others are still drawn.

## Alternatives considered

- **One dynamic uniform slot per object, one draw per object** (the renderer before this decision). It keeps a draw call per object; at 100,000 objects the CPU spends the frame encoding commands.
- **Instance data in a storage buffer indexed by `instance_index`.** Equivalent on native GPUs, but storage buffers in the vertex stage are optional in WebGPU's compatibility mode and limited on some mobile adapters; a vertex buffer read per instance works everywhere.
- **A BVH rebuilt from scratch each frame, or after each edit** (binned SAH). It gives better trees but costs O(n log n) on each change, while a move during a drag must stay cheap. The dynamic tree updates in O(log n).
- **A uniform grid or an octree.** Simple, but building elements range from 4 cm sprinkler heads to 72 m façades: one cell size fits neither, and an octree stores large objects high in the tree, where every query tests them.
- **Culling against bounding spheres only.** Cheaper per test, but long thin elements (beams, walls) have spheres far larger than their boxes, so fewer of them are culled.
- **Discrete LOD from schema-declared coarse representations.** Out of scope for the MVP (spec "Out of scope"); a derived box needs nothing from the model.
- **Switching levels without hysteresis.** Objects near a threshold flicker during a smooth orbit.

## References

- [Specification](../../specifications.md) §13 (VP-02), §14 (NFR-02 and the reference model).
- [ADR 0020](0020-camera-relative-rendering.md): camera-relative rendering.
- E. Catto, *Dynamic Bounding Volume Hierarchies*, GDC 2019, and the dynamic tree of Box2D.
- Issue [#70](https://github.com/GuillaumeOj/BIMCraft/issues/70) and the feature spec of the 3D viewport epic.
