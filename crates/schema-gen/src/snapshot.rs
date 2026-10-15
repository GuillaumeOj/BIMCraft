//! Snapshots: the canonical model of a released schema version as JSON (ADR 0002,
//! `snapshots/NNNN.json`).
//!
//! The JSON mirrors [`schema::Namespace`] field by field. Maps become arrays sorted by name,
//! value types and role targets use their definition-file spelling, and reals are strings in
//! Rust's shortest round-trip form, so reading a snapshot back gives exactly the same model.

use std::collections::BTreeMap;

use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Presence, PropertySet, Representation,
    Role, RoleTarget, TypeDef, Value,
};
use serde::{Deserialize, Serialize};

use crate::definition::{CardinalityDef, KindDef, PlacementDef};

/// The snapshot of `namespace`: pretty-printed JSON ending with a newline. Same model, same
/// bytes.
///
/// # Errors
///
/// Returns the serializer's message if the model cannot be written as JSON.
pub fn snapshot(namespace: &Namespace) -> Result<String, String> {
    let json = serde_json::to_string_pretty(&NamespaceJson::from(namespace))
        .map_err(|error| error.to_string())?;
    Ok(json + "\n")
}

/// Reads a snapshot back into the model it was written from.
///
/// # Errors
///
/// Returns a message when the text is not a snapshot: invalid JSON, unknown or missing keys,
/// an unknown value type, kind or role target, or a real that is not a number.
pub fn parse_snapshot(text: &str) -> Result<Namespace, String> {
    let json: NamespaceJson = serde_json::from_str(text).map_err(|error| error.to_string())?;
    json.try_into()
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamespaceJson {
    name: String,
    version: u32,
    entities: Vec<EntityJson>,
    property_sets: Vec<PropertySetJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntityJson {
    name: String,
    kind: KindDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deprecated: Option<u32>,
    property_sets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    type_def: Option<TypeDefJson>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ifc: Option<IfcJson>,
    attributes: Vec<AttributeJson>,
    roles: Vec<RoleJson>,
    representations: Vec<RepresentationJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TypeDefJson {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IfcJson {
    entity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    predefined_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    object_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    type_entity: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AttributeJson {
    name: String,
    value: String,
    presence: PresenceJson,
    placement: PlacementDef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum PresenceJson {
    Required,
    Default(ValueJson),
    Optional,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ValueJson {
    Bool(bool),
    Integer(i64),
    Real(String),
    Text(String),
    List(Vec<ValueJson>),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RoleJson {
    name: String,
    targets: Vec<String>,
    cardinality: CardinalityDef,
    exclusive: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RepresentationJson {
    lod: u16,
    context: String,
    generator: String,
    depends_on: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertySetJson {
    name: String,
    applies_to: Vec<String>,
    properties: Vec<AttributeJson>,
}

impl From<&Namespace> for NamespaceJson {
    fn from(namespace: &Namespace) -> Self {
        Self {
            name: namespace.name.clone(),
            version: namespace.version,
            entities: namespace.entities.values().map(EntityJson::from).collect(),
            property_sets: namespace
                .property_sets
                .values()
                .map(|set| PropertySetJson {
                    name: set.name.clone(),
                    applies_to: set.applies_to.clone(),
                    properties: set.properties.iter().map(AttributeJson::from).collect(),
                })
                .collect(),
        }
    }
}

impl From<&Entity> for EntityJson {
    fn from(entity: &Entity) -> Self {
        Self {
            name: entity.name.clone(),
            kind: entity.kind.into(),
            doc: entity.doc.clone(),
            deprecated: entity.deprecated,
            property_sets: entity.property_sets.clone(),
            type_def: entity.type_def.as_ref().map(|type_def| TypeDefJson {
                name: type_def.name.clone(),
                doc: type_def.doc.clone(),
            }),
            ifc: entity.ifc.as_ref().map(|ifc| IfcJson {
                entity: ifc.entity.clone(),
                predefined_type: ifc.predefined_type.clone(),
                object_type: ifc.object_type.clone(),
                type_entity: ifc.type_entity.clone(),
            }),
            attributes: entity.attributes.iter().map(AttributeJson::from).collect(),
            roles: entity
                .roles
                .iter()
                .map(|role| RoleJson {
                    name: role.name.clone(),
                    targets: role.targets.iter().map(target_name).collect(),
                    cardinality: role.cardinality.into(),
                    exclusive: role.exclusive,
                })
                .collect(),
            representations: entity
                .representations
                .iter()
                .map(|representation| RepresentationJson {
                    lod: representation.lod,
                    context: representation.context.clone(),
                    generator: representation.generator.clone(),
                    depends_on: representation.depends_on.clone(),
                })
                .collect(),
        }
    }
}

impl From<&Attribute> for AttributeJson {
    fn from(attribute: &Attribute) -> Self {
        Self {
            name: attribute.name.clone(),
            value: attribute.value.to_string(),
            presence: match &attribute.presence {
                Presence::Required => PresenceJson::Required,
                Presence::Default(value) => PresenceJson::Default(value.into()),
                Presence::Optional => PresenceJson::Optional,
            },
            placement: attribute.placement.into(),
            doc: attribute.doc.clone(),
        }
    }
}

impl From<&Value> for ValueJson {
    fn from(value: &Value) -> Self {
        match value {
            Value::Bool(value) => Self::Bool(*value),
            Value::Integer(value) => Self::Integer(*value),
            Value::Real(value) => Self::Real(value.to_string()),
            Value::Text(value) => Self::Text(value.clone()),
            Value::List(items) => Self::List(items.iter().map(Self::from).collect()),
        }
    }
}

/// A role target as definition files write it: an entity name or `kind:<kind>`.
fn target_name(target: &RoleTarget) -> String {
    match target {
        RoleTarget::Entity(name) => name.clone(),
        RoleTarget::Kind(kind) => format!("kind:{}", kind.name()),
    }
}

impl TryFrom<NamespaceJson> for Namespace {
    type Error = String;

    fn try_from(json: NamespaceJson) -> Result<Self, String> {
        let entities = json
            .entities
            .into_iter()
            .map(|entity| Ok((entity.name.clone(), entity.try_into()?)));
        let entities = unique(entities, "entity")?;
        let property_sets = json.property_sets.into_iter().map(|set| {
            let properties = set
                .properties
                .into_iter()
                .map(Attribute::try_from)
                .collect::<Result<_, _>>()?;
            let set = PropertySet {
                name: set.name,
                applies_to: set.applies_to,
                properties,
            };
            Ok((set.name.clone(), set))
        });
        let property_sets = unique(property_sets, "property set")?;
        Ok(Self {
            name: json.name,
            version: json.version,
            entities,
            property_sets,
        })
    }
}

impl TryFrom<EntityJson> for Entity {
    type Error = String;

    fn try_from(json: EntityJson) -> Result<Self, String> {
        let roles = json
            .roles
            .into_iter()
            .map(|role| {
                Ok(Role {
                    name: role.name,
                    targets: role
                        .targets
                        .iter()
                        .map(|target| parse_target(target))
                        .collect::<Result<_, String>>()?,
                    cardinality: role.cardinality.into(),
                    exclusive: role.exclusive,
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Self {
            name: json.name,
            kind: json.kind.into(),
            doc: json.doc,
            deprecated: json.deprecated,
            property_sets: json.property_sets,
            type_def: json.type_def.map(|type_def| TypeDef {
                name: type_def.name,
                doc: type_def.doc,
            }),
            ifc: json.ifc.map(|ifc| IfcMapping {
                entity: ifc.entity,
                predefined_type: ifc.predefined_type,
                object_type: ifc.object_type,
                type_entity: ifc.type_entity,
            }),
            attributes: json
                .attributes
                .into_iter()
                .map(Attribute::try_from)
                .collect::<Result<_, _>>()?,
            roles,
            representations: json
                .representations
                .into_iter()
                .map(|representation| Representation {
                    lod: representation.lod,
                    context: representation.context,
                    generator: representation.generator,
                    depends_on: representation.depends_on,
                })
                .collect(),
        })
    }
}

impl TryFrom<AttributeJson> for Attribute {
    type Error = String;

    fn try_from(json: AttributeJson) -> Result<Self, String> {
        Ok(Self {
            value: json.value.parse().map_err(|error| format!("{error}"))?,
            presence: match json.presence {
                PresenceJson::Required => Presence::Required,
                PresenceJson::Default(value) => Presence::Default(value.try_into()?),
                PresenceJson::Optional => Presence::Optional,
            },
            placement: json.placement.into(),
            name: json.name,
            doc: json.doc,
        })
    }
}

impl TryFrom<ValueJson> for Value {
    type Error = String;

    fn try_from(json: ValueJson) -> Result<Self, String> {
        Ok(match json {
            ValueJson::Bool(value) => Self::Bool(value),
            ValueJson::Integer(value) => Self::Integer(value),
            ValueJson::Real(text) => Self::Real(
                text.parse()
                    .map_err(|_| format!("\"{text}\" is not a real number"))?,
            ),
            ValueJson::Text(value) => Self::Text(value),
            ValueJson::List(items) => Self::List(
                items
                    .into_iter()
                    .map(Self::try_from)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

/// Collects named items, refusing a name listed twice: a map would silently keep only one.
fn unique<T>(
    items: impl Iterator<Item = Result<(String, T), String>>,
    what: &str,
) -> Result<BTreeMap<String, T>, String> {
    let mut map = BTreeMap::new();
    for item in items {
        let (name, item) = item?;
        if map.insert(name.clone(), item).is_some() {
            return Err(format!("the {what} \"{name}\" is listed twice"));
        }
    }
    Ok(map)
}

/// A role target as definition files write it: an entity name or `kind:<kind>`.
pub(crate) fn parse_target(text: &str) -> Result<RoleTarget, String> {
    let Some(kind) = text.strip_prefix("kind:") else {
        return Ok(RoleTarget::Entity(text.to_owned()));
    };
    EntityKind::ALL
        .into_iter()
        .find(|candidate| candidate.name() == kind)
        .map(RoleTarget::Kind)
        .ok_or_else(|| format!("unknown entity kind \"{kind}\""))
}

#[cfg(test)]
mod tests {
    use schema::{Cardinality, Placement};

    use super::*;
    use crate::{load_dir, parse_sources};

    fn sample() -> Namespace {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample");
        parse_sources(&load_dir(&dir).unwrap()).unwrap()
    }

    #[test]
    fn a_snapshot_reads_back_into_the_same_model() {
        let namespace = sample();
        let text = snapshot(&namespace).unwrap();
        assert!(text.ends_with("}\n"));
        assert_eq!(parse_snapshot(&text), Ok(namespace.clone()));
        assert_eq!(snapshot(&parse_snapshot(&text).unwrap()).unwrap(), text);
    }

    #[test]
    fn every_value_and_kind_round_trips() {
        let mut namespace = sample();
        let values = Value::List(vec![
            Value::Bool(true),
            Value::Integer(-4),
            Value::Real(0.1 + 0.2),
            Value::Real(-1e-300),
            Value::Text("béton".into()),
            Value::List(Vec::new()),
        ]);
        let entity = namespace.entities.values_mut().next().unwrap();
        entity.attributes[0].presence = Presence::Default(values);
        entity.kind = EntityKind::Relationship;
        entity.roles.push(Role {
            name: "parts".into(),
            targets: vec![
                RoleTarget::Kind(EntityKind::Element),
                RoleTarget::Kind(EntityKind::Spatial),
                RoleTarget::Kind(EntityKind::Relationship),
                RoleTarget::Entity("Wall".into()),
            ],
            cardinality: Cardinality::One,
            exclusive: true,
        });
        entity.attributes[0].placement = Placement::Type;
        assert_eq!(
            parse_snapshot(&snapshot(&namespace).unwrap()),
            Ok(namespace)
        );
    }

    #[test]
    fn malformed_snapshots_are_reported() {
        let text = snapshot(&sample()).unwrap();
        let cases = [
            ("{", "EOF"),
            (
                &text.replacen("\"version\"", "\"extra\": 1, \"version\"", 1),
                "unknown field `extra`",
            ),
            (
                &text.replacen("\"quantity:length\"", "\"quantity:speed\"", 1),
                "unknown quantity kind \"speed\"",
            ),
            (
                &text.replacen("\"real\": \"", "\"real\": \"x", 1),
                "is not a real number",
            ),
        ];
        for (text, expected) in cases {
            let error = parse_snapshot(text).unwrap_err();
            assert!(error.contains(expected), "{error}");
        }
        assert_eq!(
            parse_target("kind:wing"),
            Err("unknown entity kind \"wing\"".into())
        );
    }

    #[test]
    fn names_listed_twice_are_refused() {
        let text = snapshot(&sample()).unwrap();
        for (list, expected) in [
            ("entities", "the entity \"Beam\" is listed twice"),
            (
                "property_sets",
                "the property set \"Pset_WallCommon\" is listed twice",
            ),
        ] {
            let mut json: serde_json::Value = serde_json::from_str(&text).unwrap();
            let items = json[list].as_array_mut().unwrap();
            items.push(items[0].clone());
            assert_eq!(
                parse_snapshot(&json.to_string()),
                Err(expected.to_owned()),
                "{list}"
            );
        }
    }
}
