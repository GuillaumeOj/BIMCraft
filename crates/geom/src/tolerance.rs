//! The constants of the tolerance policy (GEO-03); the rationale is in
//! `docs/dev/adr/0001-geometry-tolerance-policy.md`. Changing either value changes stored results.

/// The largest coordinate magnitude the kernel accepts, 10 km, in metres: every coordinate must
/// lie in [−`MODEL_EXTENT`, +`MODEL_EXTENT`]. Modelling happens in a local engineering frame near
/// the origin; input outside the extent is rejected, never clamped.
pub const MODEL_EXTENT: f64 = 1e4;

/// The kernel's single linear tolerance, 1 µm, in metres. Fixed, never user- or
/// project-settable, so results are the same everywhere (ENG-04).
pub const LINEAR_TOLERANCE: f64 = 1e-6;
