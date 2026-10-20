//! The constants of the tolerance policy (GEO-03); the rationale is in
//! `docs/dev/adr/0001-geometry-tolerance-policy.md`. Changing any value changes stored results.

/// The largest coordinate magnitude the kernel accepts, 10 km, in metres: every coordinate must
/// lie in [−`MODEL_EXTENT`, +`MODEL_EXTENT`]. Modelling happens in a local engineering frame near
/// the origin; input outside the extent is rejected, never clamped.
pub const MODEL_EXTENT: f64 = 1e4;

/// The kernel's single linear tolerance, 1 µm, in metres. Fixed, never user- or
/// project-settable, so results are the same everywhere (ENG-04). A distance is within the
/// tolerance when it is at most this, up to rounding: a feature exactly 1 µm wide merges.
pub const LINEAR_TOLERANCE: f64 = 1e-6;

/// The widest a group of vertices merged into one may be along each axis, 10 µm, in metres:
/// a wider chain of vertices each within the tolerance of the next fails the operation instead
/// of moving geometry silently (ADR 0001, merge pass step 2).
pub const MERGE_CHAIN_WIDTH: f64 = 10.0 * LINEAR_TOLERANCE;

/// The largest distance the kernel reads as zero: [`LINEAR_TOLERANCE`] plus 1 nm for rounding.
///
/// A feature exactly one tolerance wide, as entered, is then within the tolerance whichever way
/// its coordinates round, so every test agrees on it; with the bare tolerance, one test could
/// round it in and the next out (ADR 0001). The allowance is hundreds of times the rounding of
/// coordinates within [`MODEL_EXTENT`] and a thousandth of the tolerance.
pub(crate) const WITHIN_TOLERANCE: f64 = LINEAR_TOLERANCE + ROUNDING;

/// The allowance for rounding in [`WITHIN_TOLERANCE`], 1 nm: below it, two computations of one
/// distance may disagree.
pub(crate) const ROUNDING: f64 = 1e-9;

/// [`WITHIN_TOLERANCE`] squared, for comparing squared distances.
pub(crate) const WITHIN_TOLERANCE_SQUARED: f64 = WITHIN_TOLERANCE * WITHIN_TOLERANCE;

/// Whether a finite coordinate is one the kernel accepts: within [`MODEL_EXTENT`], and zero or
/// large enough for the exact predicates (tiny non-zero values would break them later).
pub(crate) fn is_model_coordinate(value: f64) -> bool {
    value.abs() <= MODEL_EXTENT && crate::predicates::check(value).is_ok()
}

/// The largest distance, 1 mm, in metres, between a profile arc and the chords that replace it
/// when a solid is built with planar faces only. Fixed like [`LINEAR_TOLERANCE`]; the rationale is
/// in `docs/dev/adr/0008-arc-faceting.md`.
pub const ARC_CHORD_TOLERANCE: f64 = 1e-3;
