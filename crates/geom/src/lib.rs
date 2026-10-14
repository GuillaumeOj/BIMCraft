//! The `BIMCraft` geometry kernel (spec §6): exact predicates first, then profiles, solids and
//! booleans.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), never touches the
//! platform directly (ARCH-02) and uses no platform `libm`, so results are identical on every
//! target (ENG-04).
//!
//! The predicates ([`orient2d`], [`orient3d`], [`incircle`], [`insphere`]) return the exact sign
//! of their determinant, even for nearly degenerate input (GEO-03). Exactness requires every
//! coordinate to be zero or between [`MIN_COORDINATE`] and [`MAX_COORDINATE`] in magnitude;
//! other input is rejected with a [`PredicateError`]. These bounds only protect exactness: the
//! model extent of the tolerance policy (`docs/dev/adr/0001-geometry-tolerance-policy.md`) is far
//! narrower and is enforced where coordinates enter the kernel.
//!
//! A [`Solid`] is a boundary representation bounded by planar faces. It can only be built
//! valid: [`Solid::from_faces`] checks topology, the Euler–Poincaré formula, planarity,
//! orientation, [`MODEL_EXTENT`] and [`LINEAR_TOLERANCE`], and returns a [`BrepError`] otherwise.

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod brep;
mod point;
mod predicates;

pub use brep::{BrepError, FaceInput, Shell, Solid};
pub use point::{Point2, Point3};
pub use predicates::{
    MAX_COORDINATE, MIN_COORDINATE, Orientation, PredicateError, incircle, insphere, orient2d,
    orient3d,
};

/// The kernel's single linear tolerance, 1 µm, in metres.
///
/// Fixed, never user- or project-settable, so results are the same everywhere (ENG-04).
/// Rationale: `docs/dev/adr/0001-geometry-tolerance-policy.md`.
pub const LINEAR_TOLERANCE: f64 = 1e-6;

/// The largest coordinate magnitude a solid accepts, 10 km, in metres.
///
/// Modelling happens in a local engineering frame near the origin. Rationale:
/// `docs/dev/adr/0001-geometry-tolerance-policy.md`.
pub const MODEL_EXTENT: f64 = 1e4;
