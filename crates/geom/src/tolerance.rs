//! The constants of the tolerance policy (GEO-03); the rationale is in
//! `docs/dev/adr/0001-geometry-tolerance-policy.md`. Changing any value changes stored results.

/// The largest coordinate magnitude the kernel accepts, 10 km, in metres: every coordinate must
/// lie in [−`MODEL_EXTENT`, +`MODEL_EXTENT`]. Modelling happens in a local engineering frame near
/// the origin; input outside the extent is rejected, never clamped.
pub const MODEL_EXTENT: f64 = 1e4;

/// The kernel's single linear tolerance, 1 µm, in metres. Fixed, never user- or
/// project-settable, so results are the same everywhere (ENG-04).
pub const LINEAR_TOLERANCE: f64 = 1e-6;

/// The largest distance, 1 mm, in metres, between a profile arc and the chords that replace it
/// when a solid is built with planar faces only. Fixed like [`LINEAR_TOLERANCE`]; the rationale is
/// in `docs/dev/adr/0008-arc-faceting.md`.
pub const ARC_CHORD_TOLERANCE: f64 = 1e-3;
