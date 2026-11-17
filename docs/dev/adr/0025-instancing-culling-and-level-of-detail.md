# 0025. Instancing, culling and level of detail

- Status: Accepted
- Requirements: VP-02, NFR-02 (also VP-04, GEO-07, ARCH-01, ENG-04)
- Issues: [#70](https://github.com/GuillaumeOj/BIMCraft/issues/70), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67)

## Context

NFR-02 asks for 60 frames per second while orbiting the reference model of the specification (§14): a building of about 100,000 elements. Building models repeat a few element types many times: columns, beams, ceiling tiles, desks. Before this decision the renderer drew each object with its own draw call and its own slot in a dynamic uniform buffer, and drew every object every frame, whether it was in view, behind the camera or smaller than a pixel. 100,000 draw calls per frame is far beyond what WebGPU in a browser sustains, and most of them draw nothing visible.

[ADR 0020](0020-camera-relative-rendering.md) still holds: each object's model-view is computed on the CPU in `f64` relative to the camera, and only that result reaches the GPU in `f32`. Whatever is added here keeps that rule and runs on native GPUs and WebGPU alike (ARCH-01). The host deduplicates element meshes by content (spec D3), so objects that look alike share one mesh. Occlusion culling, added afterwards ([#70](https://github.com/GuillaumeOj/BIMCraft/issues/70), task B3), builds on the hierarchy decided here; its section below amends this record.

Inside a building, most elements are hidden behind slabs, walls and façades. Frustum culling keeps them all when they are in front of the camera, so the GPU still draws them. WebGPU has occlusion queries (`wgpu::QueryType::Occlusion`: how many samples of a draw passed the depth test), but no conditional rendering and no way to read a result in the same frame: results come back through a buffer mapped asynchronously, a frame or more later, and nothing may block in a browser.

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

**Occlusion culling.** Objects are tested in *clusters*, not one by one: a cluster is a node of the hierarchy with at most 128 objects below it whose box is compact, that is, of a surface area at most 3 times that of its objects' boxes together. Each node keeps its count of leaves and the sum of its leaves' areas, updated with the bounds. Without the compactness rule, a lone wall joins the 70 objects behind it in one box that the wall itself makes visible; with it, the reference model has 5,780 clusters. Each frame:

1. The world walks the hierarchy down to the clusters in view (`Bvh::clusters`, `crates/render/src/occlusion.rs`). A node is skipped, with every object below it, when a *trusted* result found it hidden: a result of a query issued while the node was in view without a break, after the last edit of the hierarchy or the settings, at most 4 frames ago. Otherwise a cluster is drawn.
2. When trusted results found both children of a node above the clusters hidden, and no newer one found the node visible, the node is skipped and tested in their place (the pull-up of coherent hierarchical culling, CHC++). Over a few frames a hidden region collapses to a handful of queries.
3. After the opaque pass, the same render pass draws the box of each node chosen for a query, enlarged by 10⁻⁵ of its radius plus its distance, from a unit cube and a camera-relative model-view computed in `f64` like every object's. The pipeline tests depth with `GreaterEqual` and writes neither colour nor depth, inside one occlusion query per box. A box across the near plane is never queried, since clipping could hide what it holds; its objects are drawn.
4. The results are resolved into a buffer, copied into a readback buffer, and mapped asynchronously. A later frame reads the mapped buffers it finds (`Device::poll` without waiting runs the callbacks natively; the browser runs them itself) and records each result against the frame that issued it. At most 3 readback buffers are on their way; when all are, the frame issues no queries.

The budget of queries per frame (`occlusion_queries`, 1,024 by default, at most 4,096, WebGPU's largest query set) goes first to the nodes the frame skips, so that what they would reveal is found soon, then to the clusters queried longest ago.

The rule is conservative: a node without a trusted result is drawn. Adding, moving or removing an object, or changing the settings, distrusts every earlier result, so removing a wall shows what it hid in the same frame; a node newly in view is drawn until a query issued while it is in view answers; turning the camera in place changes what hides what nowhere. What remains is the latency of the results: an object revealed by a camera that moves sideways (parallax) or by a level-of-detail switch of its occluder appears one or two frames late, at most 4. The draw order does not change, so a frame without reveals is pixel for pixel the frame without occlusion culling.

**Settings** (`RenderSettings`, validated by `Renderer::set_settings`). Thresholds are between 0 and 10⁵ px, the hysteresis between 0 and 1, the occlusion queries between 1 and 4,096. A threshold of 0 turns its feature off, `frustum_culling: false` turns frustum culling off and `occlusion_culling: false` occlusion culling; the pixel-comparison tests use these switches.

| Setting | Default | Why |
| --- | --- | --- |
| `frustum_culling` | on | It changes no pixel. |
| `coarse_below_px` | 24 px | Below about 24 px, the I-sections, doors, desks and chairs of a building are not told apart from their boxes. |
| `hide_below_px` | 1 px | Below a pixel an object covers at most one sample, which it would hit or miss by chance anyway. |
| `hysteresis` | 25 % | Wider than the change in projected size of one step of a smooth orbit or zoom. |
| `occlusion_culling` | on | It changes no pixel of a still view; a moving view can show a revealed object a frame or two late. |
| `occlusion_queries` | 1,024 | A sixth of the reference model's clusters per frame, hidden ones first: a hidden region needs one query per frame once pulled up, and the boxes stay cheap draws next to the objects. |

**Public API.** `add_mesh`, `remove_mesh`, `add_object`, `set_placement` and `remove_object` keep their signatures and meaning. `render` returns `FrameStats`: the counts of objects culled outside the view, culled as hidden behind others, culled as too small, drawn fine, drawn coarse, the draw calls and the occlusion queries. `prepare_frame` runs the CPU part of a frame alone, for the benches and the tests of culling and levels; it uses the occlusion results already delivered but issues no queries. `drawn_level` tells the level an object got in the last prepared frame. `settings` and `set_settings` read and change the settings.

**Reference model and measurements.** `crates/render/tests/support/reference.rs` generates the reference model without randomness: 10 storeys of about 10,000 elements each, 100,150 in all, built from 15 meshes. Each storey has columns, beams, slab panels, a curtain wall, cellular offices with doors and furniture, an open plan, and a ceiling of 0.6 m tiles with lights and sprinkler heads. The model sits at a national-grid offset, (2.5·10⁶, 1.2·10⁶, 0) m. The tests, the unit tests of the crate and the benches share it. `cargo bench -p render --bench frame` measures the frame preparation while orbiting, and the GPU frame time on a hardware adapter, each with occlusion culling on and off. The method and the reference numbers are in [performance-baselines.md](../performance-baselines.md); a weekly workflow keeps criterion's output ([quality.md](../quality.md#benchmarks)). The target for the CPU preparation is a median under 4 ms, which leaves the GPU the rest of the 16.7 ms of a frame at 60 fps.

**Verification.** The tests check behaviour, not time:

- On fixed cameras the counts of objects culled, hidden, fine and coarse are exact. The orbit, aerial and inside counts of the reference model are pinned.
- The levels chosen while dollying out and back show the hysteresis.
- A fixed-seed property test checks that the hierarchy finds exactly what a brute-force scan finds, through any sequence of insertions, moves and removals.
- Unit tests check the frustum planes at 10⁷ m from the origin.
- GPU tests run on lavapipe in CI and are never skipped. Objects sharing a mesh draw the same pixels in one call as separate meshes do in 25. The culled frame equals the unculled frame on part of the reference model. A small detailed object draws exactly its bounding box. A sub-pixel object leaves the frame empty.
- Occlusion culling is tested on the CPU with the results a GPU would give behind a wall: when a cluster is skipped, when a result expires or is distrusted, the pull-up of hidden siblings, the order of the budget, the query boxes far from the origin, and the near plane. On a GPU, a wall hides a group of 81 cubes: once the results are in, exactly that group is skipped, and every frame equals the frame without occlusion culling pixel for pixel, as do the frame after the wall is removed and the frame after the camera turns a new group into view.

## Consequences

- A frame costs one draw call per mesh and level in view, not one per object; with the reference model, at most 30.
- The CPU still visits every object each frame to choose its level and write its record, but tests boxes only against the nodes of the hierarchy that cross the view. The cost grows with the objects in view, not with the size of the model.
- Picking (B6) can walk the same hierarchy with a ray. Highlight and ghost states (B5) extend the instance record.
- What an object looks like no longer depends only on its own state: its projected size and its previous level matter. `drawn_level` and `FrameStats` make that observable.
- Meshes whose local frame is not the element's (a mesh built in world coordinates, for instance) get a coarse box aligned with the world rather than the element: still enclosing, only looser.
- With occlusion culling, an object revealed by a moving camera can appear up to a few frames late (one or two at 60 fps on a GPU that keeps up, at most 4). Tests that compare pixels after a camera move turn occlusion culling off, or turn the camera in place.
- Each frame draws up to 1,024 more small boxes, each between its own begin and end of query; the cost moves from drawing hidden objects to drawing their boxes. The walk of the clusters costs the CPU 0.1 to 0.4 ms on the reference model.
- Results are kept per node of the hierarchy and distrusted wholesale on every edit, so a drag that moves objects every frame gets no occlusion culling until it stops.

## Alternatives considered

- **One dynamic uniform slot per object, one draw per object** (the renderer before this decision). It keeps a draw call per object; at 100,000 objects the CPU spends the frame encoding commands.
- **Instance data in a storage buffer indexed by `instance_index`.** Equivalent on native GPUs, but storage buffers in the vertex stage are optional in WebGPU's compatibility mode and limited on some mobile adapters; a vertex buffer read per instance works everywhere.
- **A BVH rebuilt from scratch each frame, or after each edit** (binned SAH). It gives better trees but costs O(n log n) on each change, while a move during a drag must stay cheap. The dynamic tree updates in O(log n).
- **A uniform grid or an octree.** Simple, but building elements range from 4 cm sprinkler heads to 72 m façades: one cell size fits neither, and an octree stores large objects high in the tree, where every query tests them.
- **Culling against bounding spheres only.** Cheaper per test, but long thin elements (beams, walls) have spheres far larger than their boxes, so fewer of them are culled.
- **Discrete LOD from schema-declared coarse representations.** Out of scope for the MVP (spec "Out of scope"); a derived box needs nothing from the model.
- **Switching levels without hysteresis.** Objects near a threshold flicker during a smooth orbit.
- **Occlusion results read back in the same frame.** It needs a blocking wait for the GPU, which a browser forbids and which stalls the pipeline natively.
- **Conditional rendering or GPU-driven culling** (a compute pass testing boxes against a depth pyramid and writing indirect draws). WebGPU has no conditional rendering; a depth pyramid needs compute and indirect draws everywhere, a larger change better made with the rest of a GPU-driven renderer.
- **A software rasteriser of occluders on the CPU.** It needs hand-picked occluders and costs the CPU the time the 4 ms target leaves to the rest of the frame.
- **Clusters by count alone, or a query per object.** By count alone, large occluders share a box with what they hide; per object, the queries of the reference model would outnumber its draw calls a thousandfold.
- **Results kept per subtree with version stamps**, so an edit distrusts only the clusters it touched. Moving an occluder reveals clusters it does not touch, so the stamps alone would let them pop late; a sound version would need the region the occluder swept. Distrusting everything is exact and simple; edits come at user speed, except during drags.

## References

- [Specification](../../specifications.md) §13 (VP-02), §14 (NFR-02 and the reference model).
- [ADR 0020](0020-camera-relative-rendering.md): camera-relative rendering.
- E. Catto, *Dynamic Bounding Volume Hierarchies*, GDC 2019, and the dynamic tree of Box2D.
- O. Mattausch, J. Bittner, M. Wimmer, *CHC++: Coherent Hierarchical Culling Revisited*, Eurographics 2008.
- Issue [#70](https://github.com/GuillaumeOj/BIMCraft/issues/70) and the feature spec of the 3D viewport epic.
