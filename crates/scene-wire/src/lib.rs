//! The binary format of the scene deltas sent from the core to the viewport (VP-01).
//!
//! A [`SceneDelta`] holds what changed in the scene since a revision: meshes and elements added
//! or changed, meshes and elements removed. [`encode`] writes it as the version 1 format of
//! `api-contract.md` (little-endian, sections in a fixed order) and [`decode`] reads it back.
//!
//! The decoder treats its input as hostile (AGENTS.md §8): every length is checked against the
//! buffer before anything is read or allocated, so no allocation is larger than the input; every
//! count is capped by a constant of this crate; floats are finite and within the ranges of
//! [ADR 0020](../../docs/dev/adr/0020-camera-relative-rendering.md); trailing bytes, unknown
//! versions and unknown flags are rejected. What it accepts, [`encode`] writes back to the same
//! bytes. Errors name positions and counts, never coordinates or model content (SEC-03).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod check;
mod decode;
mod delta;
mod encode;
mod error;
#[cfg(test)]
mod tests;

pub use decode::decode;
pub use delta::{Element, Mesh, SceneDelta};
pub use encode::encode;
pub use error::{Section, WireError};

/// The first four bytes of every delta.
pub const MAGIC: [u8; 4] = *b"BCSW";

/// The version of the format this crate reads and writes.
pub const VERSION: u16 = 1;

/// The flag bit that tells the viewport to drop everything it holds before applying the delta.
pub const FLAG_FULL: u16 = 1;

/// The size of the header, in bytes.
pub const HEADER_LEN: usize = 32;

/// The largest delta, in bytes: what a GPU buffer and the webview accept comfortably.
pub const MAX_DELTA_BYTES: usize = 256 << 20;

/// Revisions stay below this (2^53), so the viewport holds them as JavaScript numbers.
pub const REVISION_LIMIT: u64 = 1 << 53;

/// The most vertices of a mesh (the same limit as the renderer's `MAX_MESH_VERTICES`).
pub const MAX_MESH_VERTICES: usize = 1 << 24;

/// The most indices of a mesh (the same limit as the renderer's `MAX_MESH_INDICES`).
pub const MAX_MESH_INDICES: usize = 3 << 24;

/// The most meshes added by one delta.
pub const MAX_MESHES: usize = 1 << 20;

/// The most elements added or changed by one delta.
pub const MAX_ELEMENTS: usize = 1 << 22;

/// The most strings (category and discipline names) of one delta.
pub const MAX_STRINGS: usize = 1 << 16;

/// The longest string, in bytes of UTF-8.
pub const MAX_STRING_BYTES: usize = 256;

/// The largest absolute coordinate, in metres (ADR 0020). The same value as the renderer's
/// `MAX_WORLD_COORDINATE`, which this crate, in a lower layer, cannot import.
pub const MAX_WORLD_COORDINATE: f64 = 1e9;

/// How far from unit length a normal may be: the rounding of a normal computed in `f64` and
/// stored in `f32` is many orders of magnitude smaller (the renderer's tolerance).
pub const NORMAL_LENGTH_TOLERANCE: f64 = 1e-3;

/// How far from orthonormal the columns of a rotation may be: the rounding of a rotation
/// computed in `f64` is smaller by many orders of magnitude. The same value as the renderer's
/// `ORTHONORMAL_TOLERANCE`, so a delta this crate accepts is never refused when placed.
pub const ROTATION_TOLERANCE: f64 = 1e-9;
