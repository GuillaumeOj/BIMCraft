# wgpu renderer — API contract

This feature adds no command API (`api` crate) and no UI. The contract below is the **Rust API of the `render` crate**, which later viewport issues (#69–#72) build on. Names are indicative: the backend may adjust them in its T1 plan, but not the semantics.

## Types

```rust
/// A triangle mesh with f32 vertices relative to an f64 origin (GEO-07, VP-04).
pub struct MeshData {
    pub origin: [f64; 3],          // world-independent mesh origin, metres
    pub positions: Vec<[f32; 3]>,  // relative to `origin`
    pub normals: Vec<[f32; 3]>,    // one per position, unit length
    pub indices: Vec<u32>,         // triangle list
}

impl MeshData {
    /// Builds a mesh from f64 positions; the origin is the centre of their bounding box.
    pub fn from_world(positions: &[geom::Point3], normals: &[[f32; 3]], indices: &[u32])
        -> Result<Self, MeshError>;
    /// Checks finiteness, index count % 3 == 0, indices in range, matching lengths, size bounds.
    pub fn validate(&self) -> Result<(), MeshError>;
}

/// Places a mesh in the world: rotation (orthonormal, row-major 3×3) then translation, in f64.
pub struct Placement { pub rotation: [[f64; 3]; 3], pub translation: [f64; 3] }

pub enum Projection {
    Perspective { fov_y_radians: f64, near: f64 },             // reverse-Z, infinite far
    Orthographic { height: f64, near: f64, far: f64 },         // reverse-Z
}

pub struct Camera {
    pub eye: geom::Point3,     // f64 world position
    pub target: geom::Point3,
    pub up: geom::Vector3,
    pub projection: Projection,
}

pub struct MeshId(/* opaque */);
pub struct ObjectId(/* opaque */);

#[derive(thiserror::Error)] pub enum MeshError { /* non-finite, bad index, length mismatch, too large, empty */ }
#[derive(thiserror::Error)] pub enum RenderError { /* invalid camera, unknown id, zero-sized target, no adapter… */ }
```

## Renderer

```rust
impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, color_format: wgpu::TextureFormat) -> Self;
    pub fn add_mesh(&mut self, mesh: &MeshData) -> Result<MeshId, MeshError>;
    pub fn remove_mesh(&mut self, id: MeshId) -> Result<(), RenderError>;   // also removes its objects
    pub fn add_object(&mut self, mesh: MeshId, placement: Placement, color: [f32; 4]) -> Result<ObjectId, RenderError>;
    pub fn set_placement(&mut self, id: ObjectId, placement: Placement) -> Result<(), RenderError>;
    pub fn remove_object(&mut self, id: ObjectId) -> Result<(), RenderError>;
    /// Draws every object into `target` (size in pixels), seen from `camera`.
    pub fn render(&mut self, target: &wgpu::TextureView, size: [u32; 2], camera: &Camera, clear: [f64; 4])
        -> Result<(), RenderError>;
}

/// Requests a device without a surface (tests, tools). Async: never blocks (ARCH-02).
pub async fn headless_device() -> Result<(wgpu::Device, wgpu::Queue), RenderError>;
```

## Pure functions (unit-tested on the CPU)

- `camera_relative_model_view(camera: &Camera, mesh_origin: [f64; 3], placement: &Placement) -> [[f32; 4]; 4]`: D1 of the spec, all work in f64, one conversion to f32 at the end.
- `projection_matrix(projection: &Projection, aspect: f64) -> [[f32; 4]; 4]`: reverse-Z.
- A `#[cfg(test)]` naive variant (world matrices rounded to f32, multiplied in f32) for the CPU precision tests only. The GPU negative control of D4 uses the public API.

## Invariants tested

- Translating the whole scene and the camera by the same vector T (|T| up to 10⁷ m) leaves `camera_relative_model_view` unchanged to within 1 ulp of f32 per entry.
- No public matrix sent to the GPU contains the camera's world coordinates.
- Bad meshes and cameras are rejected with typed errors and never panic.
