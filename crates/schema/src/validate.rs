use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::value_type::MAX_LIST_DEPTH;
use crate::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, PropertySet,
    RoleTarget, Value, ValueType,
};

/// Most entities a namespace may define.
pub const MAX_ENTITIES: usize = 4096;
/// Most property sets a namespace may define.
pub const MAX_PROPERTY_SETS: usize = 4096;
/// Most items in any list of a definition: attributes, roles, properties, targets…
pub const MAX_ITEMS: usize = 256;
/// Longest name, in bytes.
pub const MAX_NAME_LEN: usize = 64;
/// Longest description, in bytes.
pub const MAX_DOC_LEN: usize = 4096;

/// Where in a namespace a validation error was found.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Location {
    /// The definition the error is in.
    pub owner: Owner,
    /// The part of the definition, when the error is not about the definition as a whole.
    pub item: Option<Item>,
}

/// A top-level definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Owner {
    /// The namespace itself.
    Namespace,
    /// An entity, by name.
    Entity(String),
    /// A property set, by name.
    PropertySet(String),
}

/// A part of a definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Item {
    /// The entity's `[type]`.
    Type,
    /// The entity's `[ifc]` mapping.
    Ifc,
    /// An attribute, by name.
    Attribute(String),
    /// A property, by name.
    Property(String),
    /// A role, by name.
    Role(String),
    /// A representation, by position from 0.
    Representation(usize),
}

/// One problem found by [`validate`].
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("{location}: {kind}")]
pub struct ValidationError {
    /// Where the problem is.
    pub location: Location,
    /// What the problem is.
    pub kind: ValidationErrorKind,
}

/// What is wrong.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ValidationErrorKind {
    /// A name does not follow its naming rule.
    #[error("\"{name}\" is not a valid name: expected {rule}")]
    InvalidName {
        /// The offending name.
        name: String,
        /// The rule, in words.
        rule: &'static str,
    },
    /// A description is longer than [`MAX_DOC_LEN`].
    #[error("description is longer than {MAX_DOC_LEN} bytes")]
    DocTooLong,
    /// A list is longer than its limit.
    #[error("more than {limit} {what}")]
    TooMany {
        /// What the list holds.
        what: &'static str,
        /// The limit.
        limit: usize,
    },
    /// The schema version is 0.
    #[error("schema versions start at 1")]
    ZeroVersion,
    /// The deprecation version is 0 or later than the namespace version.
    #[error("deprecated from version {version}, which is not between 1 and the current version")]
    InvalidDeprecation {
        /// The deprecation version.
        version: u32,
    },
    /// A name is defined twice where it must be unique.
    #[error("\"{name}\" is defined more than once")]
    Duplicate {
        /// The repeated name.
        name: String,
    },
    /// A reference names no entity of the namespace.
    #[error("unknown entity \"{name}\"")]
    UnknownEntity {
        /// The unknown name.
        name: String,
    },
    /// An entity lists a property set the namespace does not define.
    #[error("unknown property set \"{name}\"")]
    UnknownPropertySet {
        /// The unknown name.
        name: String,
    },
    /// An entity lists a property set whose `applies_to` does not name it.
    #[error("property set \"{name}\" does not apply to this entity")]
    PropertySetNotApplicable {
        /// The property set.
        name: String,
    },
    /// An attribute is placed on a type, but the entity has none.
    #[error("placed on the type, but the entity has no [type]")]
    PlacementWithoutType,
    /// A default value does not match the value type.
    #[error("default value does not match the value type {expected}")]
    DefaultMismatch {
        /// The declared value type.
        expected: ValueType,
    },
    /// A real default is NaN or infinite.
    #[error("default value is not a finite number")]
    NonFiniteDefault,
    /// A value type is valid but cannot be used yet.
    #[error("{0} are not supported yet")]
    Unsupported(&'static str),
    /// A non-relationship entity declares roles.
    #[error("only relationship entities have roles")]
    RolesOutsideRelationship,
    /// A relationship entity declares no role.
    #[error("a relationship entity needs at least one role")]
    RelationshipWithoutRoles,
    /// A role allows no target.
    #[error("a role needs at least one target")]
    RoleWithoutTargets,
    /// The entity has no IFC mapping (CORE-10).
    #[error("every entity needs an [ifc] mapping (CORE-10)")]
    MissingIfcMapping,
    /// `USERDEFINED` without an `object_type`, or an `object_type` with another predefined type.
    #[error("object_type is required with, and only allowed with, predefined_type USERDEFINED")]
    ObjectTypeMismatch,
    /// `type_entity` on an entity that has no type.
    #[error("type_entity is only allowed on entities with a [type]")]
    TypeEntityWithoutType,
    /// Two entities map to the same IFC entity, predefined type and object type.
    #[error("the same IFC mapping as entity \"{other}\"")]
    DuplicateIfcMapping {
        /// The entity mapped first.
        other: String,
    },
    /// `depends_on` names something that is not an attribute of the entity.
    #[error("depends on \"{name}\", which is not an attribute of the entity")]
    UnknownDependency {
        /// The unknown attribute.
        name: String,
    },
    /// Two representations share a level of detail and a context.
    #[error("another representation already has level of detail {lod} in context \"{context}\"")]
    DuplicateRepresentation {
        /// The level of detail.
        lod: u16,
        /// The context.
        context: String,
    },
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.owner {
            Owner::Namespace => f.write_str("namespace")?,
            Owner::Entity(name) => write!(f, "entity {name}")?,
            Owner::PropertySet(name) => write!(f, "property set {name}")?,
        }
        match &self.item {
            None => Ok(()),
            Some(Item::Type) => f.write_str(", [type]"),
            Some(Item::Ifc) => f.write_str(", [ifc]"),
            Some(Item::Attribute(name)) => write!(f, ", attribute {name}"),
            Some(Item::Property(name)) => write!(f, ", property {name}"),
            Some(Item::Role(name)) => write!(f, ", role {name}"),
            Some(Item::Representation(index)) => write!(f, ", representation #{}", index + 1),
        }
    }
}

/// Checks that a namespace is consistent and within the size limits.
///
/// Every problem is reported, not only the first, sorted by location. A namespace that passes
/// can be registered and generated from without further checks, so this is the gate for
/// untrusted plugin bundles too (AGENTS.md §8).
///
/// # Errors
///
/// Returns every [`ValidationError`] found.
pub fn validate(namespace: &Namespace) -> Result<(), Vec<ValidationError>> {
    let mut errors = Errors::default();
    validate_namespace(namespace, &mut errors);
    let mut errors = errors.0;
    if errors.is_empty() {
        Ok(())
    } else {
        errors.sort_by(|a, b| a.location.cmp(&b.location));
        Err(errors)
    }
}

#[derive(Default)]
struct Errors(Vec<ValidationError>);

impl Errors {
    fn push(&mut self, owner: &Owner, item: Option<Item>, kind: ValidationErrorKind) {
        self.0.push(ValidationError {
            location: Location {
                owner: owner.clone(),
                item,
            },
            kind,
        });
    }

    /// Reports every `(count, limit, what)` over its limit; true when any is.
    fn too_many(
        &mut self,
        owner: &Owner,
        item: Option<&Item>,
        checks: &[(usize, usize, &'static str)],
    ) -> bool {
        let mut over = false;
        for &(count, limit, what) in checks {
            if count > limit {
                let kind = ValidationErrorKind::TooMany { what, limit };
                self.push(owner, item.cloned(), kind);
                over = true;
            }
        }
        over
    }
}

fn validate_namespace(namespace: &Namespace, errors: &mut Errors) {
    let owner = Owner::Namespace;
    check_name(&namespace.name, NameRule::Namespace, &owner, None, errors);
    if namespace.version == 0 {
        errors.push(&owner, None, ValidationErrorKind::ZeroVersion);
    }
    // Stop before any per-definition work: an oversized bundle is rejected cheaply.
    let limits = [
        (namespace.entities.len(), MAX_ENTITIES, "entities"),
        (
            namespace.property_sets.len(),
            MAX_PROPERTY_SETS,
            "property sets",
        ),
    ];
    if errors.too_many(&owner, None, &limits) {
        return;
    }
    check_type_names(namespace, errors);
    check_ifc_uniqueness(namespace, errors);
    for entity in namespace.entities.values() {
        validate_entity(namespace, entity, errors);
    }
    for property_set in namespace.property_sets.values() {
        validate_property_set(namespace, property_set, errors);
    }
}

/// Type names share the entity namespace: generated code and the API name both.
fn check_type_names(namespace: &Namespace, errors: &mut Errors) {
    let mut seen: BTreeSet<&str> = namespace.entities.keys().map(String::as_str).collect();
    for entity in namespace.entities.values() {
        if let Some(type_def) = &entity.type_def
            && !seen.insert(&type_def.name)
        {
            errors.push(
                &Owner::Entity(entity.name.clone()),
                Some(Item::Type),
                ValidationErrorKind::Duplicate {
                    name: type_def.name.clone(),
                },
            );
        }
    }
}

/// Import rebuilds an entity from its IFC mapping, so no two entities may share one.
fn check_ifc_uniqueness(namespace: &Namespace, errors: &mut Errors) {
    let mut seen: BTreeMap<(&str, Option<&str>, Option<&str>), &str> = BTreeMap::new();
    for entity in namespace.entities.values() {
        let Some(ifc) = &entity.ifc else { continue };
        let key = (
            ifc.entity.as_str(),
            ifc.predefined_type.as_deref(),
            ifc.object_type.as_deref(),
        );
        if let Some(other) = seen.insert(key, &entity.name) {
            errors.push(
                &Owner::Entity(entity.name.clone()),
                Some(Item::Ifc),
                ValidationErrorKind::DuplicateIfcMapping {
                    other: other.to_owned(),
                },
            );
        }
    }
}

fn validate_entity(namespace: &Namespace, entity: &Entity, errors: &mut Errors) {
    let owner = Owner::Entity(entity.name.clone());
    check_name(&entity.name, NameRule::Pascal, &owner, None, errors);
    check_doc(entity.doc.as_deref(), &owner, None, errors);
    if let Some(version) = entity.deprecated
        && (version == 0 || version > namespace.version)
    {
        errors.push(
            &owner,
            None,
            ValidationErrorKind::InvalidDeprecation { version },
        );
    }
    let limits = [
        (entity.attributes.len(), MAX_ITEMS, "attributes"),
        (entity.roles.len(), MAX_ITEMS, "roles"),
        (entity.representations.len(), MAX_ITEMS, "representations"),
        (entity.property_sets.len(), MAX_ITEMS, "property sets"),
    ];
    if errors.too_many(&owner, None, &limits) {
        return;
    }
    if let Some(type_def) = &entity.type_def {
        check_name(
            &type_def.name,
            NameRule::Pascal,
            &owner,
            Some(Item::Type),
            errors,
        );
        check_doc(type_def.doc.as_deref(), &owner, Some(Item::Type), errors);
    }
    match &entity.ifc {
        Some(ifc) => check_ifc(ifc, entity.type_def.is_some(), &owner, errors),
        None => errors.push(&owner, None, ValidationErrorKind::MissingIfcMapping),
    }
    check_entity_property_sets(namespace, entity, &owner, errors);
    let mut fields = BTreeSet::new();
    for attribute in &entity.attributes {
        let item = Item::Attribute(attribute.name.clone());
        check_attribute(namespace, attribute, NameRule::Snake, &owner, &item, errors);
        if attribute.placement != Placement::Instance && entity.type_def.is_none() {
            errors.push(
                &owner,
                Some(item.clone()),
                ValidationErrorKind::PlacementWithoutType,
            );
        }
        check_unique(&mut fields, &attribute.name, &owner, item, errors);
    }
    check_roles(namespace, entity, &owner, &mut fields, errors);
    check_representations(entity, &owner, errors);
}

fn check_ifc(ifc: &IfcMapping, has_type: bool, owner: &Owner, errors: &mut Errors) {
    let item = || Some(Item::Ifc);
    check_name(&ifc.entity, NameRule::Ifc, owner, item(), errors);
    if let Some(type_entity) = &ifc.type_entity {
        check_name(type_entity, NameRule::Ifc, owner, item(), errors);
        if !has_type {
            errors.push(owner, item(), ValidationErrorKind::TypeEntityWithoutType);
        }
    }
    if let Some(predefined) = &ifc.predefined_type {
        check_name(predefined, NameRule::Upper, owner, item(), errors);
    }
    if let Some(object_type) = &ifc.object_type {
        check_name(object_type, NameRule::Text, owner, item(), errors);
    }
    let user_defined = ifc.predefined_type.as_deref() == Some("USERDEFINED");
    if user_defined != ifc.object_type.is_some() {
        errors.push(owner, item(), ValidationErrorKind::ObjectTypeMismatch);
    }
}

fn check_entity_property_sets(
    namespace: &Namespace,
    entity: &Entity,
    owner: &Owner,
    errors: &mut Errors,
) {
    let mut seen = BTreeSet::new();
    for name in &entity.property_sets {
        if !seen.insert(name) {
            let kind = ValidationErrorKind::Duplicate { name: name.clone() };
            errors.push(owner, None, kind);
            continue;
        }
        match namespace.property_sets.get(name) {
            None => {
                let kind = ValidationErrorKind::UnknownPropertySet { name: name.clone() };
                errors.push(owner, None, kind);
            }
            Some(property_set) if !property_set.applies_to.contains(&entity.name) => {
                let kind = ValidationErrorKind::PropertySetNotApplicable { name: name.clone() };
                errors.push(owner, None, kind);
            }
            Some(_) => {}
        }
    }
}

fn check_roles(
    namespace: &Namespace,
    entity: &Entity,
    owner: &Owner,
    fields: &mut BTreeSet<String>,
    errors: &mut Errors,
) {
    let is_relationship = entity.kind == EntityKind::Relationship;
    if is_relationship && entity.roles.is_empty() {
        errors.push(owner, None, ValidationErrorKind::RelationshipWithoutRoles);
    }
    if !is_relationship && !entity.roles.is_empty() {
        errors.push(owner, None, ValidationErrorKind::RolesOutsideRelationship);
    }
    for role in &entity.roles {
        let item = || Some(Item::Role(role.name.clone()));
        check_name(&role.name, NameRule::Snake, owner, item(), errors);
        if role.targets.is_empty() {
            errors.push(owner, item(), ValidationErrorKind::RoleWithoutTargets);
        }
        let limits = [(role.targets.len(), MAX_ITEMS, "targets")];
        if errors.too_many(owner, item().as_ref(), &limits) {
            continue;
        }
        for target in &role.targets {
            if let RoleTarget::Entity(name) = target {
                check_entity_exists(namespace, name, owner, item(), errors);
            }
        }
        check_unique(
            fields,
            &role.name,
            owner,
            Item::Role(role.name.clone()),
            errors,
        );
    }
}

fn check_representations(entity: &Entity, owner: &Owner, errors: &mut Errors) {
    let attributes: BTreeSet<&str> = entity.attributes.iter().map(|a| a.name.as_str()).collect();
    let mut seen = BTreeSet::new();
    for (index, representation) in entity.representations.iter().enumerate() {
        let item = || Some(Item::Representation(index));
        check_name(
            &representation.context,
            NameRule::Snake,
            owner,
            item(),
            errors,
        );
        check_name(
            &representation.generator,
            NameRule::Path,
            owner,
            item(),
            errors,
        );
        if !seen.insert((representation.lod, representation.context.as_str())) {
            let kind = ValidationErrorKind::DuplicateRepresentation {
                lod: representation.lod,
                context: representation.context.clone(),
            };
            errors.push(owner, item(), kind);
        }
        let limits = [(representation.depends_on.len(), MAX_ITEMS, "dependencies")];
        if errors.too_many(owner, item().as_ref(), &limits) {
            continue;
        }
        for name in &representation.depends_on {
            if !attributes.contains(name.as_str()) {
                let kind = ValidationErrorKind::UnknownDependency { name: name.clone() };
                errors.push(owner, item(), kind);
            }
        }
    }
}

fn validate_property_set(namespace: &Namespace, property_set: &PropertySet, errors: &mut Errors) {
    let owner = Owner::PropertySet(property_set.name.clone());
    check_name(
        &property_set.name,
        NameRule::PropertySet,
        &owner,
        None,
        errors,
    );
    let limits = [
        (
            property_set.applies_to.len(),
            MAX_ITEMS,
            "entities in applies_to",
        ),
        (property_set.properties.len(), MAX_ITEMS, "properties"),
    ];
    if errors.too_many(&owner, None, &limits) {
        return;
    }
    let mut applies_to = BTreeSet::new();
    for name in &property_set.applies_to {
        if !applies_to.insert(name) {
            let kind = ValidationErrorKind::Duplicate { name: name.clone() };
            errors.push(&owner, None, kind);
        }
        check_entity_exists(namespace, name, &owner, None, errors);
    }
    let mut names = BTreeSet::new();
    for property in &property_set.properties {
        let item = Item::Property(property.name.clone());
        check_attribute(namespace, property, NameRule::Pascal, &owner, &item, errors);
        check_unique(&mut names, &property.name, &owner, item, errors);
    }
}

fn check_attribute(
    namespace: &Namespace,
    attribute: &Attribute,
    rule: NameRule,
    owner: &Owner,
    item: &Item,
    errors: &mut Errors,
) {
    let item = || Some(item.clone());
    check_name(&attribute.name, rule, owner, item(), errors);
    check_doc(attribute.doc.as_deref(), owner, item(), errors);
    // Bundles reach here without going through `ValueType::from_str`, so nesting is checked
    // again, without recursion, before anything recurses through the type.
    let limits = [(
        list_depth(&attribute.value),
        MAX_LIST_DEPTH,
        "nested list levels",
    )];
    if errors.too_many(owner, item().as_ref(), &limits) {
        return;
    }
    check_value_type(namespace, &attribute.value, owner, item(), errors);
    if let Presence::Default(value) = &attribute.presence {
        if !value_matches(value, &attribute.value) {
            let kind = ValidationErrorKind::DefaultMismatch {
                expected: attribute.value.clone(),
            };
            errors.push(owner, item(), kind);
        } else if !is_finite(value) {
            errors.push(owner, item(), ValidationErrorKind::NonFiniteDefault);
        }
    }
}

fn list_depth(mut value: &ValueType) -> usize {
    let mut depth = 0;
    while let ValueType::List(inner) = value {
        depth += 1;
        value = inner;
    }
    depth
}

fn check_value_type(
    namespace: &Namespace,
    value: &ValueType,
    owner: &Owner,
    item: Option<Item>,
    errors: &mut Errors,
) {
    match value {
        ValueType::Ref(name) => check_entity_exists(namespace, name, owner, item, errors),
        // ADR 0002 names `enum:<Name>` but defines no file to declare enumerations in yet.
        ValueType::Enum(_) => errors.push(
            owner,
            item,
            ValidationErrorKind::Unsupported("enumeration value types"),
        ),
        ValueType::List(inner) => check_value_type(namespace, inner, owner, item, errors),
        ValueType::Bool
        | ValueType::Integer
        | ValueType::Real
        | ValueType::Text
        | ValueType::Quantity(_) => {}
    }
}

fn value_matches(value: &Value, value_type: &ValueType) -> bool {
    match (value, value_type) {
        (Value::Bool(_), ValueType::Bool)
        | (Value::Integer(_), ValueType::Integer)
        | (Value::Real(_), ValueType::Real | ValueType::Quantity(_))
        | (Value::Text(_), ValueType::Text) => true,
        (Value::List(items), ValueType::List(inner)) => {
            items.iter().all(|item| value_matches(item, inner))
        }
        _ => false,
    }
}

fn is_finite(value: &Value) -> bool {
    match value {
        Value::Real(number) => number.is_finite(),
        Value::List(items) => items.iter().all(is_finite),
        Value::Bool(_) | Value::Integer(_) | Value::Text(_) => true,
    }
}

fn check_entity_exists(
    namespace: &Namespace,
    name: &str,
    owner: &Owner,
    item: Option<Item>,
    errors: &mut Errors,
) {
    if !namespace.entities.contains_key(name) {
        let kind = ValidationErrorKind::UnknownEntity {
            name: name.to_owned(),
        };
        errors.push(owner, item, kind);
    }
}

fn check_unique(
    seen: &mut BTreeSet<String>,
    name: &str,
    owner: &Owner,
    item: Item,
    errors: &mut Errors,
) {
    if !seen.insert(name.to_owned()) {
        let kind = ValidationErrorKind::Duplicate {
            name: name.to_owned(),
        };
        errors.push(owner, Some(item), kind);
    }
}

fn check_doc(doc: Option<&str>, owner: &Owner, item: Option<Item>, errors: &mut Errors) {
    if doc.is_some_and(|doc| doc.len() > MAX_DOC_LEN) {
        errors.push(owner, item, ValidationErrorKind::DocTooLong);
    }
}

/// Naming rules. ASCII only, so names are safe as Rust, Python and JSON identifiers.
#[derive(Clone, Copy)]
enum NameRule {
    /// `core` or `plugin:<plugin-id>`, the id in lowercase words joined by hyphens.
    Namespace,
    /// `Wall`, `IsExternal`.
    Pascal,
    /// `base_offset`.
    Snake,
    /// `Pset_WallCommon`.
    PropertySet,
    /// `IfcWall`.
    Ifc,
    /// `STANDARD`, `USERDEFINED`.
    Upper,
    /// `wall::footprint`.
    Path,
    /// Any text without control characters, such as an IFC object type.
    Text,
}

impl NameRule {
    const fn description(self) -> &'static str {
        match self {
            Self::Namespace => "\"core\" or \"plugin:<id>\" with a lowercase, hyphenated id",
            Self::Pascal => "PascalCase ASCII letters and digits",
            Self::Snake => "snake_case ASCII letters and digits",
            Self::PropertySet => "ASCII letters, digits and underscores, starting with a letter",
            Self::Ifc => "an IFC name such as IfcWall",
            Self::Upper => "UPPER_CASE ASCII letters, digits and underscores",
            Self::Path => "a Rust path such as wall::footprint",
            Self::Text => "text without control characters",
        }
    }

    fn accepts(self, name: &str) -> bool {
        match self {
            Self::Namespace => {
                name == "core" || name.strip_prefix("plugin:").is_some_and(is_plugin_id)
            }
            Self::Pascal => is_pascal(name),
            Self::Snake => is_snake(name),
            Self::PropertySet => {
                name.starts_with(|c: char| c.is_ascii_alphabetic())
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            }
            Self::Ifc => name.strip_prefix("Ifc").is_some_and(is_pascal),
            Self::Upper => {
                name.starts_with(|c: char| c.is_ascii_uppercase())
                    && name
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            }
            Self::Path => {
                let mut segments = name.split("::");
                segments.clone().count() >= 2 && segments.all(is_snake)
            }
            Self::Text => !name.is_empty() && !name.chars().any(char::is_control),
        }
    }
}

fn is_pascal(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_snake(name: &str) -> bool {
    is_lower_words(name, '_')
}

fn is_plugin_id(id: &str) -> bool {
    is_lower_words(id, '-')
}

/// Lowercase ASCII words of letters and digits, the first starting with a letter, joined by
/// single `separator`s.
fn is_lower_words(name: &str, separator: char) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.split(separator).all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn check_name(name: &str, rule: NameRule, owner: &Owner, item: Option<Item>, errors: &mut Errors) {
    if name.len() > MAX_NAME_LEN || !rule.accepts(name) {
        let kind = ValidationErrorKind::InvalidName {
            name: name.to_owned(),
            rule: rule.description(),
        };
        errors.push(owner, item, kind);
    }
}

#[cfg(test)]
mod tests;
