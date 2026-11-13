//! The public extension interfaces of the `BIMCraft` core (ARCH-03, spec §8).
//!
//! Discipline crates (`disc-*`) and third-party plugins reach the core through this crate only,
//! plus the `platform` traits. It holds no logic: each module re-exports, by name, the items an
//! extension may use, so this crate's documentation is the extension surface. Storage internals,
//! the undo history, persistence, import and export, analysis, drawings, the command API, the
//! renderer and the hosts stay out of it.
//!
//! `scripts/check-architecture.ts` rejects any other core dependency in a `disc-*` manifest, and
//! any glob or whole-crate re-export here. `tests/surface.rs` names every item, so changing the
//! surface is always a deliberate, reviewed edit. Decisions:
//! `docs/dev/adr/0024-discipline-extension-surface.md`.
//!
//! This crate builds for native targets and `wasm32-unknown-unknown` (ARCH-01) and never touches
//! the platform directly (ARCH-02).

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

/// Entity definitions: the namespaces, entities, attributes and value types an extension
/// declares and registers, their validation, and the migrations between their versions.
pub mod schema {
    pub use ::schema::{
        Attribute, Cardinality, DeclaredStep, Entity, EntityKind, EnumItem, Enumeration,
        IfcMapping, Item, Location, MAX_DOC_LEN, MAX_ENTITIES, MAX_ENUMERATIONS, MAX_ITEMS,
        MAX_LIST_DEPTH, MAX_NAME_LEN, MAX_PROPERTY_SETS, MAX_STEPS, Migration, MigrationError,
        Namespace, Owner, Placement, Presence, PropertySet, QuantityKind, Representation, Role,
        RoleTarget, Step, StoredAttribute, TypeDef, ValidationError, ValidationErrorKind, Value,
        ValueType, ValueTypeError, validate,
    };
}

/// The model as an extension reads it: object ids, attribute values, components and types, and
/// values that carry their source.
pub mod model {
    pub use ::core_model::{
        AttributeValue, Component, EntityId, GlobalIdError, IdSource, LibraryRef, MAX_SOURCE_LEN,
        Model, ModelError, ObjectState, Participation, Removal, Source, SourceError, SourcedValue,
    };
}

/// Units and quantities: lengths, areas, angles, forces and the other kinds, with the project's
/// display units.
pub mod units {
    pub use ::core_model::units::{
        Angle, AngleFormat, AngleUnit, Area, AreaUnit, Dimension, DisplayFormat, Force, ForceUnit,
        Formatted, FormattedAngle, Length, LengthUnit, MAX_DECIMALS, MAX_TEXT_LEN, Mass, MassUnit,
        Moment, MomentUnit, ParseQuantityError, ProjectUnits, Quantity, Stress, StressUnit, Unit,
        Volume, VolumeUnit,
    };
}

/// Derived data: values computed from the model and from each other, recomputed lazily and
/// incrementally (CORE-08).
pub mod derived {
    pub use ::core_model::derived::{DependencyGraph, DerivedError};
}

/// Changes to the model: commands, the transaction they run in, where they come from, and the
/// event that lists what they changed.
pub mod command {
    pub use ::core_txn::{ChangeEvent, Command, CommandError, ModelCommand, Origin, Transaction};
}

/// Geometry values and the kernel operations on them: points, profiles, paths, solids, location
/// lines, tolerances and exact predicates.
pub mod geometry {
    pub use ::geom::{
        ARC_CHORD_TOLERANCE, ArcDirection, BooleanError, BrepError, CardinalPoint, Containment,
        DirectionSense, Edge, ExtrusionError, Face, FaceInput, IfcCardinalPoint, LINEAR_TOLERANCE,
        Layer, LayerId, LayerPlacement, LayerSet, LocationError, LocationLine, LocationPoint,
        LoopId, MAX_COORDINATE, MAX_LAYERS, MAX_MESH_NODES, MAX_PATH_SEGMENTS, MAX_PROFILE_EDGES,
        MAX_SOLID_FACES, MAX_SWEEP_PIECES, MERGE_CHAIN_WIDTH, MIN_COORDINATE, MODEL_EXTENT,
        Orientation, Path, PathError, PathSegment, Point2, Point3, PredicateError, Profile,
        ProfileError, Resolved, RevolutionAxis, RevolutionError, ShearCentreError, Shell, SnapInfo,
        SnapPoint, SnapPosition, Solid, SweepError, Vector2, Vector3, incircle, insphere, orient2d,
        orient3d,
    };
}

/// Materials, their behaviour laws and sourced properties, the grade library, layered build-ups,
/// and the commands that read and edit them in the model.
pub mod material {
    pub use ::material::{
        BehaviourLaw, Category, ConcreteBilinear, ConcreteParabolaRectangle, ENTITY, Edit,
        EditError, Factor, Hardening, LAYER_ENTITY, LAYER_SET_ENTITY, LIBRARY_FORMAT, LawError,
        LawKind, Layer, LayerFunction, LayerSet, LayerSetError, LayerSetReadError, Library,
        LibraryError, LinearElastic, MAX_LAYERS, MAX_LIBRARY_SIZE, MAX_MATERIALS, MAX_NAME_LEN,
        MAX_PRIORITY, MAX_SOURCE_NAME, MAX_TEXT_LEN, Material, MaterialError, NationalAnnex,
        ORIGINAL_ENTITY, Original, Property, Quantity, Range, ReadError, Sourced, SteelBilinear,
        StrainLimits, Thermal, TimberElasticBrittle, Usage, check_layers, check_name, commands,
        conductivity, delete, delete_layer_set, design_code_key, edit, fresh_layer_ids,
        ifc_priority, layer_id, layer_ids, layer_set_commands, original_id, original_ids,
        originals, priority_from_ifc, read, read_layer_set, rename, replace_layer_set, reset_value,
        set_value, thermal, used_by,
    };
}

/// Where the project lies on Earth, read-only: the georeference, its map conversion, the
/// coordinate reference systems and their projections.
pub mod georef {
    pub use ::georef::site::{SiteError, find_site, read};
    pub use ::georef::{
        AreaOfUse, BasePointError, CrsStatus, Datum, DatumShift, Ellipsoid, EpsgCode, Factors,
        Field, GeocentricPoint, GeodesyError, GeographicCrs, GeographicPoint, Georeference,
        GeoreferenceError, Helmert, HelmertParameters, LambertConformalConic,
        LambertConic1SpParameters, LambertConic2SpParameters, LocalPoint, MapConversion,
        NamedEllipsoid, Parameter, ParseError, PrimeMeridian, ProjectedCrs, ProjectedPoint,
        Projection, Registry, RegistryError, RotationConvention, TransverseMercator,
        TransverseMercatorParameters, XAxisError, rotation_from_x_axis,
    };
}
