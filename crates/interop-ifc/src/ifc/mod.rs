//! IFC 4.3 entity classes, generated from the official EXPRESS schema (ADR 0009).
//!
//! Each entity that can be instantiated is a struct with its explicit attributes, inherited
//! ones first, in STEP order; [`Entity`] holds any of them and decodes a [`step::Record`]
//! into the right one. Defined types are newtypes, enumerations are enums, and selects are
//! enums of their member types. References to other instances stay unresolved
//! ([`decode::Ref`]).
//!
//! [`step::Record`]: crate::step::Record

pub mod decode;

#[allow(
    clippy::too_many_lines,
    reason = "generated matches and tests have one arm or check per declaration of the schema"
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/ifc4x3.rs"));
}

pub use generated::*;
