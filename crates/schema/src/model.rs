use std::collections::BTreeMap;

use crate::ValueType;

/// One schema namespace — `core`, or `plugin:<plugin-id>` — in canonical form: every name
/// resolved, every default filled in, entities and property sets sorted by name.
#[derive(Clone, Debug, PartialEq)]
pub struct Namespace {
    /// `core` or `plugin:<plugin-id>`.
    pub name: String,
    /// The current schema version of the namespace, starting at 1.
    pub version: u32,
    /// Entities by name.
    pub entities: BTreeMap<String, Entity>,
    /// Property sets by name.
    pub property_sets: BTreeMap<String, PropertySet>,
}

/// An entity: an element, a spatial structure element or a relationship.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    /// `PascalCase` name, unique in the namespace.
    pub name: String,
    /// What the entity is; relationships may only refer to entities by kind.
    pub kind: EntityKind,
    /// Description for users of the API and the reference documentation.
    pub doc: Option<String>,
    /// The schema version from which the entity is discouraged.
    pub deprecated: Option<u32>,
    /// Names of the property sets the entity carries (CORE-04).
    pub property_sets: Vec<String>,
    /// The entity's type, when instances share a type (CORE-02).
    pub type_def: Option<TypeDef>,
    /// The IFC entity the entity maps to (CORE-10).
    pub ifc: Option<IfcMapping>,
    /// Attributes, in declaration order.
    pub attributes: Vec<Attribute>,
    /// Roles of a relationship entity, in declaration order (CORE-03).
    pub roles: Vec<Role>,
    /// Geometric representations, in declaration order (GEO-06).
    pub representations: Vec<Representation>,
}

/// What an entity is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EntityKind {
    /// A physical or virtual element: wall, beam, opening…
    Element,
    /// A spatial structure element: site, building, storey, space.
    Spatial,
    /// A first-class relationship between objects (CORE-03).
    Relationship,
}

impl EntityKind {
    /// Every kind.
    pub const ALL: [Self; 3] = [Self::Element, Self::Spatial, Self::Relationship];

    /// The name used in definition files and after `kind:` in role targets.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Element => "element",
            Self::Spatial => "spatial",
            Self::Relationship => "relationship",
        }
    }
}

/// The type of an entity: named build-ups shared by instances (CORE-02).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDef {
    /// The type's name, `<Entity>Type` unless the definition overrides it.
    pub name: String,
    /// Description of the type.
    pub doc: Option<String>,
}

/// How an entity maps onto IFC (CORE-10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IfcMapping {
    /// The IFC entity, such as `IfcWall`.
    pub entity: String,
    /// The IFC predefined type, such as `STANDARD`.
    pub predefined_type: Option<String>,
    /// The object type, required with the `USERDEFINED` predefined type.
    pub object_type: Option<String>,
    /// The IFC type entity, `<entity>Type` unless overridden; only for entities with a type.
    pub type_entity: Option<String>,
}

/// An attribute of an entity or a property of a property set.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    /// `snake_case` for attributes, `PascalCase` for properties.
    pub name: String,
    /// The type of its values.
    pub value: ValueType,
    /// Whether a value is required, defaulted or optional.
    pub presence: Presence,
    /// Whether the value is set on the instance, the type, or the type with an override.
    pub placement: Placement,
    /// Description of the attribute.
    pub doc: Option<String>,
}

/// Whether an attribute needs a value.
#[derive(Clone, Debug, PartialEq)]
pub enum Presence {
    /// Every object must set the value.
    Required,
    /// Objects that do not set the value get this one.
    Default(Value),
    /// The value may be absent.
    Optional,
}

/// Where an attribute's value is set (CORE-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Placement {
    /// On each instance.
    Instance,
    /// On the type, shared by all its instances.
    Type,
    /// On the type, and each instance may override it.
    TypeOverridable,
}

/// A literal value, such as an attribute default.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// A boolean.
    Bool(bool),
    /// An integer.
    Integer(i64),
    /// A real number; quantities are in the SI unit of their kind.
    Real(f64),
    /// Text.
    Text(String),
    /// A list.
    List(Vec<Value>),
}

/// A role in a relationship entity (CORE-03).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Role {
    /// `snake_case` name.
    pub name: String,
    /// Which objects may play the role.
    pub targets: Vec<RoleTarget>,
    /// How many objects play the role in one relationship.
    pub cardinality: Cardinality,
    /// Whether an object can play the role in only one relationship of this entity.
    pub exclusive: bool,
}

/// Objects allowed in a role.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoleTarget {
    /// Objects of one entity.
    Entity(String),
    /// Objects of every entity of a kind, written `kind:<kind>`.
    Kind(EntityKind),
}

/// How many objects play a role in one relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cardinality {
    /// Exactly one.
    One,
    /// One or more.
    Many,
}

/// A geometric representation of an entity (GEO-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Representation {
    /// Level of detail, such as 100 or 300.
    pub lod: u16,
    /// Representation context: `body`, `axis`, `footprint`, `analytical`…
    pub context: String,
    /// Path of the function that computes it, such as `wall::footprint`.
    pub generator: String,
    /// Attributes the representation reads; the dependency graph's input (CORE-08).
    pub depends_on: Vec<String>,
}

/// A named group of properties (CORE-04).
#[derive(Clone, Debug, PartialEq)]
pub struct PropertySet {
    /// Name, such as `Pset_WallCommon`.
    pub name: String,
    /// Entities the property set applies to.
    pub applies_to: Vec<String>,
    /// Properties, in declaration order.
    pub properties: Vec<Attribute>,
}
