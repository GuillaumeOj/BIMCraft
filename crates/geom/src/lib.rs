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
//! A [`Profile`] is a closed 2D region bounded by lines and circular arcs, with holes (GEO-01).
//! [`Profile::new`] validates its loops and rejects open, self-intersecting or badly nested ones
//! with a [`ProfileError`]; a valid profile gives its area and centroid. Coordinates are metres
//! within [`MODEL_EXTENT`], and arcs are kept [`LINEAR_TOLERANCE`] away from other edges.
//!
//! A [`Solid`] is a boundary representation bounded by planar faces. It can only be built
//! valid: [`Solid::from_faces`] checks topology, the Euler–Poincaré formula, planarity,
//! orientation, [`MODEL_EXTENT`] and [`LINEAR_TOLERANCE`], and returns a [`BrepError`] otherwise.
//!
//! [`Solid::extrude`] builds a solid from a profile and a direction (GEO-02), holes included.
//! Arcs become chords within [`ARC_CHORD_TOLERANCE`], since faces are planar; failures are an
//! [`ExtrusionError`].
//!
//! A [`LocationLine`] says which line of a [`LayerSet`] a wall is drawn along (ARC-30), and a
//! [`LocationPoint`] which point of a profile a column or beam sits on (STR-14). Both resolve
//! into an offset of the section from the element's axis (GEO-13); failures are a
//! [`LocationError`].
//!
//! [`Profile::shear_centre`] computes the shear centre of any profile with finite elements, for
//! the location point of profile-based members; failures are a [`ShearCentreError`].

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod brep;
mod location;
mod point;
mod predicates;
mod profile;
mod section;
mod tolerance;

pub use brep::{BrepError, ExtrusionError, FaceInput, Shell, Solid};
pub use location::{
    CardinalPoint, DirectionSense, Face, IfcCardinalPoint, Layer, LayerId, LayerPlacement,
    LayerSet, LocationError, LocationLine, LocationPoint, MAX_LAYERS, Resolved, SnapInfo,
    SnapPoint, SnapPosition,
};
pub use point::{Point2, Point3, Vector3};
pub use predicates::{
    MAX_COORDINATE, MIN_COORDINATE, Orientation, PredicateError, incircle, insphere, orient2d,
    orient3d,
};
pub use profile::{ArcDirection, Edge, LoopId, MAX_PROFILE_EDGES, Profile, ProfileError};
pub use section::{MAX_MESH_NODES, ShearCentreError};
pub use tolerance::{ARC_CHORD_TOLERANCE, LINEAR_TOLERANCE, MODEL_EXTENT};
