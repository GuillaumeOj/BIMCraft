//! The canonical schema model and its validator (spec §5.1, ADR 0002).
//!
//! Every schema namespace, core or plugin, ends up as one [`Namespace`]: a sorted, normalised
//! description of its entities and property sets. The build-time generators (`schema-gen`) and
//! the runtime registry share this model, so [`validate`] checks it the same way whether it was
//! built from the core's definition files or loaded from an untrusted plugin bundle.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01), takes contents,
//! never paths, and does no I/O of its own (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod model;
mod validate;
mod value_type;

pub use model::{
    Attribute, Cardinality, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence,
    PropertySet, Representation, Role, RoleTarget, TypeDef, Value,
};
pub use validate::{
    Item, Location, MAX_DOC_LEN, MAX_ENTITIES, MAX_ITEMS, MAX_NAME_LEN, MAX_PROPERTY_SETS, Owner,
    ValidationError, ValidationErrorKind, validate,
};
pub use value_type::{MAX_LIST_DEPTH, QuantityKind, ValueType, ValueTypeError};
