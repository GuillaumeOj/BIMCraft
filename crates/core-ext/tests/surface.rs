//! Names every item of the extension surface (ARCH-03), so that removing one from `core-ext`
//! breaks this test target: shrinking the surface that disciplines and plugins build on is then
//! always a deliberate, reviewed change. Growing it means adding the new item here too.
//!
//! The items are re-exported, not merely imported, so the compiler requires each to exist without
//! the test having to use it.

/// Every item of `core_ext::command`.
pub mod command {
    pub use core_ext::command::{
        ChangeEvent, Command, CommandError, ModelCommand, Origin, Transaction,
    };
}

/// Every item of `core_ext::derived`.
pub mod derived {
    pub use core_ext::derived::{DependencyGraph, DerivedError};
}

/// Every item of `core_ext::geometry`.
pub mod geometry {
    pub use core_ext::geometry::{
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

/// Every item of `core_ext::georef`.
pub mod georef {
    pub use core_ext::georef::{
        AreaOfUse, BasePointError, CrsStatus, Datum, DatumShift, Ellipsoid, EpsgCode, Factors,
        Field, GeocentricPoint, GeodesyError, GeographicCrs, GeographicPoint, Georeference,
        GeoreferenceError, Helmert, HelmertParameters, LambertConformalConic,
        LambertConic1SpParameters, LambertConic2SpParameters, LocalPoint, MapConversion,
        NamedEllipsoid, Parameter, ParseError, PrimeMeridian, ProjectedCrs, ProjectedPoint,
        Projection, Registry, RegistryError, RotationConvention, SiteError, TransverseMercator,
        TransverseMercatorParameters, XAxisError, find_site, read, rotation_from_x_axis,
    };
}

/// Every item of `core_ext::material`.
pub mod material {
    pub use core_ext::material::{
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

/// Every item of `core_ext::model`.
pub mod model {
    pub use core_ext::model::{
        AttributeValue, Component, EntityId, GlobalIdError, IdSource, LibraryRef, MAX_SOURCE_LEN,
        Model, ModelError, ObjectState, Participation, Removal, Source, SourceError, SourcedValue,
    };
}

/// Every item of `core_ext::schema`.
pub mod schema {
    pub use core_ext::schema::{
        Attribute, Cardinality, DeclaredStep, Entity, EntityKind, EnumItem, Enumeration,
        IfcMapping, Item, Location, MAX_DOC_LEN, MAX_ENTITIES, MAX_ENUMERATIONS, MAX_ITEMS,
        MAX_LIST_DEPTH, MAX_NAME_LEN, MAX_PROPERTY_SETS, MAX_STEPS, Migration, MigrationError,
        Namespace, Owner, Placement, Presence, PropertySet, QuantityKind, Representation, Role,
        RoleTarget, Step, StoredAttribute, TypeDef, ValidationError, ValidationErrorKind, Value,
        ValueType, ValueTypeError, validate,
    };
}

/// Every item of `core_ext::units`.
pub mod units {
    pub use core_ext::units::{
        Angle, AngleFormat, AngleUnit, Area, AreaUnit, Dimension, DisplayFormat, Force, ForceUnit,
        Formatted, FormattedAngle, Length, LengthUnit, MAX_DECIMALS, MAX_TEXT_LEN, Mass, MassUnit,
        Moment, MomentUnit, ParseQuantityError, ProjectUnits, Quantity, Stress, StressUnit, Unit,
        Volume, VolumeUnit,
    };
}
