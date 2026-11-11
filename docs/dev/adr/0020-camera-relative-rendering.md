# 0020. Camera-relative rendering

- Status: Accepted
- Requirements: VP-04, GEO-07 (also ENG-04, ARCH-01, ARCH-02, SEC-03)
- Issues: [#68](https://github.com/GuillaumeOj/BIMCraft/issues/68), epic [#67](https://github.com/GuillaumeOj/BIMCraft/issues/67)

## Context

Model coordinates are stored in `f64` (GEO-07). Projects are often placed in a national grid or a geocentric frame, so a building may sit 10⁶ m or 10⁷ m from the world origin. GPUs compute in `f32`, whose 24-bit significand spaces representable values 6.25 cm apart at 10⁶ m and 1 m apart at 10⁷ m. A renderer that sends world coordinates to the GPU, as model and view matrices or as vertices, snaps every vertex to that grid: thin elements collapse, edges zigzag, and the whole model jitters while the camera moves. VP-04 requires rendering relative to a local origin near the camera.

The renderer (`crates/render`) runs on native GPUs and on WebGPU in the browser (ARCH-01). WGSL has no `f64`, so the double-precision part must stay on the CPU.

Depth has the same problem in another form: a standard depth buffer spends most of its resolution next to the near plane, and large models at large camera distances z-fight.

## Decision

**Camera-relative transforms in `f64` on the CPU.**

- A mesh stores its vertices in `f32`, relative to a per-mesh origin in `f64`. Built from world positions, the origin is the centre of their bounding box, so local coordinates stay small: a 100 m element keeps a precision better than 10 µm.
- An object places a mesh in the world with an `f64` placement: an orthonormal rotation `R` (reflections allowed) and a translation `t`. A local vertex `v` of a mesh with origin `o` is at `R · (o + v) + t`.
- Each frame, for each object, the renderer computes in `f64` the model-view matrix with the camera at the origin: rotation `V · R` and translation `V · (R · o + (t − eye))`, where `V` is the view rotation. The large terms cancel in `f64` (about 2·10⁻⁹ m of error at 10⁷ m), and only this result, whose size is the distance to the camera, is rounded to `f32`, once.
- The vertex shader computes `clip = P · MV · v` in `f32`. No matrix sent to the GPU contains the camera's world position.
- Internally, the renderer measures GPU coordinates from a **render origin**, the eye. A test-only variant pins it to the world origin, a classic pipeline whose world model and view matrices are rounded to `f32` and multiplied in `f32`. It exists only as the negative control of the tests below; it is not public API.
- Matrices are column-major `[[f32; 4]; 4]`, the layout of a WGSL `mat4x4<f32>` uniform. The trigonometry of the projection uses `libm` (ENG-04).

**Reverse-Z depth.** Depth is `Depth32Float` with reverse-Z: the near plane maps to 1, distance decreases depth, the depth test is `Greater`. The perspective projection has an infinite far plane (depth `near / distance`); the orthographic projection maps an explicit near plane to 1 and far plane to 0. Floating-point depth values are densest near 0, which reverse-Z puts far from the camera, so resolution stays nearly uniform in relative terms.

**Validation.** Meshes come from model files: positions, normals, indices, placements and cameras are checked (finite values, world coordinates within 10⁹ m, unit normals, whole triangles, indices in range, counts that fit `u32` and the GPU buffer limits, orthonormal rotations, non-degenerate cameras, and projection parameters and aspect ratios within ranges where every entry of the `f32` projection matrix is a normal number) and rejected with typed errors whose messages never contain coordinates (SEC-03).

**Verification.**

- CPU tests emulate the `f32` operations of the vertex shader and compare the projected vertices with an `f64` reference. On a 256 px viewport, seen from about 3 m, the camera-relative path stays within 0.01 px at 10⁶ m and 10⁷ m; the naive path is off by more than a pixel.
- A fixed-seed property test translates the scene and the camera by the same vector, up to 10⁷ m, and checks that each entry of the `f32` model-view moves by at most one `f32` ulp *at the entry's scale*: the spacing of `f32` values at `max(|a|, |b|, 1)`. A strict ulp of each entry cannot hold for entries that should be 0, which the `f64` rounding of `target − eye` far from the origin makes about 10⁻¹⁰.
- A GPU visual test renders a camera sweep in sub-millimetre steps at the origin, at 10⁶ m and at 10⁷ m, and compares the frames pixel by pixel; the same sweep with the render origin at the world origin must fail the comparison, which proves that the test sees jitter.

## Consequences

- Precision no longer depends on where the project sits in the world: georeferenced models need no offset applied by the user.
- The CPU recomputes one matrix per object per frame, O(objects). Instancing (#70) may move them to an instance buffer behind the same public API.
- Everything that maps between screen and world, such as picking and snapping (#71) or clipping planes (#72), must do the same: work in `f64` relative to the camera, never in `f32` world coordinates.
- Mesh data never needs re-uploading when the camera moves.

## Alternatives considered

- **Floating origin snapped to a grid, re-based when the camera moves away.** It needs a threshold, re-uploads or re-transforms data at each re-base, and still loses precision between re-bases. Recomputing the per-object matrices every frame costs less and has no threshold.
- **High/low split vertices emulating double precision in the shader** (each coordinate as the sum of two `f32`). Twice the vertex memory and extra shader work, for precision that the per-mesh origin already gives.
- **`f64` in the shader.** WGSL has no `f64`, and native support is slow or missing on many GPUs.
- **`f32` model and view matrices multiplied on the GPU.** The classic pipeline: it is exactly what jitters, and it is kept only as the negative control.
- **A standard depth range (near to 0, far to 1).** It wastes the floating-point precision of the depth buffer next to the camera and z-fights far away.

## References

- [Specification](../../specifications.md) §6 (GEO-07) and §13 (VP-04).
- [ADR 0003](0003-deterministic-math-library.md): deterministic maths library.
- Issue [#68](https://github.com/GuillaumeOj/BIMCraft/issues/68) and its feature spec.
