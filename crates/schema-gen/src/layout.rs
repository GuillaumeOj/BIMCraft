//! What every emitter writes for a namespace: one record per entity, per entity type and per
//! property set, with the fields serialized data carries. Sharing it keeps the Rust types and the
//! JSON Schema in step, so the schema describes exactly the JSON the types read and write.

use schema::{
    Attribute, Cardinality, Entity, Namespace, Placement, Presence, PropertySet, Role, Value,
    ValueType,
};

use crate::Error;

/// A record of serialized data: an object, its type or a property set.
pub struct Struct {
    /// The schema name: `Wall`, `WallType`, `Pset_WallCommon`.
    pub name: String,
    /// Whether it is a property set, whose names follow IFC rather than Rust conventions.
    pub property_set: bool,
    /// Description, one entry per line.
    pub doc: Vec<String>,
    /// Whether the record is discouraged; its doc says from which schema version.
    pub deprecated: bool,
    /// Fields, in declaration order.
    pub fields: Vec<Field>,
}

/// A field of a record.
pub struct Field {
    /// The schema name, kept in serialized data.
    pub name: String,
    /// Description, one entry per line.
    pub doc: Vec<String>,
    /// What the field holds.
    pub ty: FieldType,
    /// Whether the field may be absent, and what absence means.
    pub kind: FieldKind,
}

/// What a field holds.
pub enum FieldType {
    /// A value of an attribute or property.
    Value(ValueType),
    /// The objects playing a role of a relationship.
    Role(Cardinality),
}

/// Whether a field may be absent.
pub enum FieldKind {
    /// Always present.
    Required,
    /// Filled with the default when absent.
    Default(Value),
    /// May be absent.
    Optional,
    /// A list, absent when empty.
    OptionalList,
}

/// The records of a namespace: each entity followed by its type, then the property sets.
///
/// The namespace is validated first, so emitters only see valid input. `name_ref` writes a
/// reference to another record in a description, in the target's markup.
pub fn plan(
    namespace: &Namespace,
    name_ref: fn(&str) -> String,
) -> Result<Vec<Struct>, Vec<Error>> {
    Error::validate(namespace)?;
    let mut structs = Vec::new();
    for entity in namespace.entities.values() {
        structs.push(instance_struct(entity));
        if let Some(type_struct) = type_struct(entity, name_ref) {
            structs.push(type_struct);
        }
    }
    for property_set in namespace.property_sets.values() {
        structs.push(property_set_struct(property_set, name_ref));
    }
    Ok(structs)
}

fn instance_struct(entity: &Entity) -> Struct {
    let mut doc = doc_lines(entity.doc.as_deref(), || format!("A `{}`.", entity.name));
    if let Some(version) = entity.deprecated {
        doc.extend([String::new(), deprecation(version)]);
    }
    let mut fields = Vec::new();
    for attribute in &entity.attributes {
        match attribute.placement {
            Placement::Instance => fields.push(attribute_field(attribute)),
            Placement::TypeOverridable => fields.push(override_field(attribute)),
            Placement::Type => {}
        }
    }
    fields.extend(entity.roles.iter().map(role_field));
    Struct {
        name: entity.name.clone(),
        property_set: false,
        doc,
        deprecated: entity.deprecated.is_some(),
        fields,
    }
}

fn type_struct(entity: &Entity, name_ref: fn(&str) -> String) -> Option<Struct> {
    let type_def = entity.type_def.as_ref()?;
    let mut doc = doc_lines(type_def.doc.as_deref(), || {
        format!("The type of {} objects.", name_ref(&entity.name))
    });
    if let Some(version) = entity.deprecated {
        doc.extend([String::new(), deprecation(version)]);
    }
    let fields = entity
        .attributes
        .iter()
        .filter(|attribute| attribute.placement != Placement::Instance)
        .map(attribute_field)
        .collect();
    Some(Struct {
        name: type_def.name.clone(),
        property_set: false,
        doc,
        deprecated: entity.deprecated.is_some(),
        fields,
    })
}

fn property_set_struct(property_set: &PropertySet, name_ref: fn(&str) -> String) -> Struct {
    let applies_to: Vec<String> = property_set
        .applies_to
        .iter()
        .map(|name| name_ref(name))
        .collect();
    Struct {
        name: property_set.name.clone(),
        property_set: true,
        deprecated: false,
        doc: vec![
            format!("The `{}` property set.", property_set.name),
            String::new(),
            format!("Applies to {}.", applies_to.join(", ")),
        ],
        fields: property_set
            .properties
            .iter()
            .map(attribute_field)
            .collect(),
    }
}

fn attribute_field(attribute: &Attribute) -> Field {
    let kind = match &attribute.presence {
        Presence::Required => FieldKind::Required,
        Presence::Default(value) => FieldKind::Default(value.clone()),
        Presence::Optional if matches!(attribute.value, ValueType::List(_)) => {
            FieldKind::OptionalList
        }
        Presence::Optional => FieldKind::Optional,
    };
    Field {
        name: attribute.name.clone(),
        doc: attribute_doc(attribute),
        ty: FieldType::Value(attribute.value.clone()),
        kind,
    }
}

/// The instance-side field of a `type_overridable` attribute: absent means "use the type's".
fn override_field(attribute: &Attribute) -> Field {
    let mut doc = vec!["Overrides the type's value for this object when set.".to_owned()];
    doc.push(String::new());
    doc.extend(attribute_doc(attribute));
    Field {
        name: attribute.name.clone(),
        doc,
        ty: FieldType::Value(attribute.value.clone()),
        kind: FieldKind::Optional,
    }
}

fn role_field(role: &Role) -> Field {
    let doc = match role.cardinality {
        Cardinality::One => format!("The object playing the `{}` role.", role.name),
        Cardinality::Many => format!("The objects playing the `{}` role.", role.name),
    };
    Field {
        name: role.name.clone(),
        doc: vec![doc],
        ty: FieldType::Role(role.cardinality),
        kind: FieldKind::Required,
    }
}

fn attribute_doc(attribute: &Attribute) -> Vec<String> {
    let mut doc = doc_lines(attribute.doc.as_deref(), || {
        format!("The `{}` value.", attribute.name)
    });
    if let Some(kind) = quantity_of(&attribute.value) {
        doc.extend([String::new(), format!("In {}.", kind.si_unit())]);
    }
    doc
}

fn quantity_of(value: &ValueType) -> Option<schema::QuantityKind> {
    match value {
        ValueType::Quantity(kind) => Some(*kind),
        ValueType::List(inner) => quantity_of(inner),
        _ => None,
    }
}

fn deprecation(version: u32) -> String {
    format!("Deprecated since schema version {version}.")
}

/// The trimmed lines of a description, or the fallback when there is none.
fn doc_lines(doc: Option<&str>, fallback: impl FnOnce() -> String) -> Vec<String> {
    let Some(doc) = doc.map(str::trim).filter(|doc| !doc.is_empty()) else {
        return vec![fallback()];
    };
    doc.lines().map(|line| line.trim().to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_docs_and_falls_back() {
        assert_eq!(doc_lines(Some("  a\n    b  "), String::new), ["a", "b"]);
        assert_eq!(doc_lines(Some("  "), || "fallback".into()), ["fallback"]);
        assert_eq!(doc_lines(None, || "fallback".into()), ["fallback"]);
    }
}
