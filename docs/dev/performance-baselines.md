# Performance baselines

Reference numbers to compare against when a hot path changes. Timings depend on the machine, so compare runs on the same machine and update this page in the pull request that moves a number. The CI of pull requests does not run the benchmarks; a weekly workflow keeps their output as an artefact ([quality.md](quality.md#benchmarks)). CI does gate memory per object.

## Attribute tables (CORE-05)

The model of [ADR 0007](adr/0007-attribute-tables.md), measured with 100,000 wall-like objects, each with a reference, two lengths, a short name, a flag and an integer.

### Time

`cargo bench -p core-model`, release profile. Apple M3, Rust 1.98.1, macOS, 2026-10-14. Median of 10 samples.

| Benchmark | 100,000 objects | Per object |
| --- | --- | --- |
| `store_100k/insert` | 23.4 ms | 234 ns |
| `store_100k/get` (one attribute) | 2.10 ms | 21 ns |
| `store_100k/set` (one attribute) | 2.99 ms | 30 ns |
| `store_100k/remove` | 30.7 ms | 307 ns |

Remove includes returning the removed values. The insert figure was taken while the benchmark still built each object's name inside the timed loop, so it is an upper bound; replace it with the next measurement on an idle machine.

### Memory (NFR-08)

`Store::heap_bytes` after inserting the objects of the `memory_per_object_stays_within_budget` test in `crates/core-model`: **166 bytes per object** on 64-bit targets, about 16.6 MB for the 100,000 objects. Roughly 95 bytes are the values themselves, 28 the spare capacity of growing vectors and 43 the id index. The test fails above a budget of 200 bytes per object; raise it only with a reason in the pull request.

## Frame preparation (VP-02, NFR-02)

The CPU part of a frame of the renderer ([ADR 0025](adr/0025-instancing-culling-and-level-of-detail.md)): frustum culling through the hierarchy, the walk of the occlusion clusters, the choice of each object's level of detail, and the writing of the instance records of the objects drawn. The model is the reference model of `crates/render/tests/support/reference.rs`: 10 storeys, 100,150 elements, 15 meshes, at a national-grid offset. The view is full HD (1920 × 1080) with a 60° vertical field of view and the default settings. NFR-02 leaves this work 4 ms of the 16.7 ms of a frame at 60 fps; the GPU gets the rest.

### Method

`cargo bench -p render --bench frame`, release profile:

- `prepare/orbit` moves the camera 10° around the building at each iteration, 110 m from its axis and 45 m up; `prepare/orbit_no_occlusion` does the same with occlusion culling off.
- `prepare/aerial` views the whole building from high above a corner.
- `prepare/inside` stands on the sixth storey, looking along the building.
- `update/move_100` moves 100 objects.
- `gpu/frame` and `gpu/frame_no_occlusion` time a whole frame until the GPU is done, with occlusion culling on and off, on a hardware adapter only. Occlusion culling saves GPU time only there: `prepare` issues no queries, so its frames skip nothing.

The benches need a GPU adapter to create the renderer, even though the `prepare` groups do not use the GPU. The `Benchmarks` workflow runs them weekly on lavapipe ([quality.md](quality.md#benchmarks)).

### Reference numbers

These numbers come from a timing probe, not from criterion: the agent sandbox that wrote this section has no GPU adapter, so `cargo bench` could not run there. The probe timed the same CPU path as `Renderer::prepare_frame` (camera validation, projection, culling, levels of detail and instance records), 200 frames per camera, in a release build. Apple Silicon Mac, Rust 1.98.1, macOS, 2026-11-14. Replace these numbers with the criterion medians at the next run on a machine with a GPU.

| View | Median | 95th percentile | Objects drawn | Instance bytes |
| --- | --- | --- | --- | --- |
| Orbit | 1.82 ms | 2.36 ms | 98,150 | 7.85 MB |
| Aerial | 1.90 ms | 2.37 ms | 98,150 (13,630 coarse) | 7.85 MB |
| Inside | 1.35 ms | 1.76 ms | 70,226 (4,534 coarse) | 5.62 MB |

With occlusion culling on, the same probe, 2026-11-14 (B3), timed the walk of the 5,780 clusters on top of the rest, without any query result, so nothing was skipped. "No queries" is what `prepare_frame` does; "1,024 queries" also chooses the clusters to test and writes their boxes, as `render` does.

| View | Occlusion off | On, no queries | On, 1,024 queries |
| --- | --- | --- | --- |
| Orbit | 1.78 ms | 2.15 ms | 2.03 ms |
| Aerial | 1.86 ms | 2.04 ms | 2.22 ms |
| Inside | 1.42 ms | 1.53 ms | 1.75 ms |

Medians of 200 frames; the 95th percentiles stay under 2.5 ms. The frames that skip hidden clusters write fewer instance records, so they cost less than these. What occlusion culling saves on the GPU needs `gpu/frame` on a hardware adapter; no number yet.

Each object drawn costs one 80-byte instance record uploaded per frame. Without occlusion culling, the outside views draw everything except the 2,000 sprinkler heads, which are under a pixel; with it, the elements behind the façades are skipped once the GPU has found them hidden.

## Moving an element: the core path (VP-01, NFR-03)

What the desktop host does between the drop of a move and the bytes the viewport receives: decode the `elements.move` message, run the transaction, re-mesh the moved block, update the scene channel, then build and encode the delta for the revision the viewport holds. IPC and the frame come on top; NFR-03 leaves this part 50 ms.

`cargo bench -p host-desktop --bench move`, release profile. The model has 100,000 blocks in a grid, in eight sizes, so meshes are shared as in a real model; the bench checks that a move sends one placement and no mesh. Apple M3, Rust 1.98.1, macOS, 2026-11-14.

| Benchmark | Median |
| --- | --- |
| `move/one_block_in_100k` | 9.7 µs |

The cost follows what the edit changed, not the model: the scene meshes the moved block only (GEO-05), and the delta is built from the change log. The scene of the whole model is built once when the document opens.
