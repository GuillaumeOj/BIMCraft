//! The `BIMCraft` renderer of the 3D viewport (spec §13.2, VP-04).
//!
//! Model coordinates are double precision (GEO-07), but GPUs compute in single precision, whose
//! grid is 6.25 cm wide at 10⁶ m from the origin. The renderer therefore never sends world
//! coordinates to the GPU: each frame, it computes on the CPU, in `f64`, the transform of every
//! object relative to the camera, and converts only that small result to `f32`
//! (`docs/dev/adr/0020-camera-relative-rendering.md`). A model 10⁷ m from the origin renders like
//! the same model at the origin.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and computes trigonometry with `libm` (ENG-04).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]
