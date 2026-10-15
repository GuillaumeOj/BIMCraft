//! The JSON Schema of the protocol (API-02): the building blocks each descriptor's parameters
//! and result are written with, and the document that gathers every registered command and query
//! with the entity schema.

use serde_json::{Map, Value, json};

use crate::{Descriptor, Kind, Registry};

/// The JSON Schema draft the protocol schema declares.
pub const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// The `$id` of the protocol schema.
pub const ID: &str = "urn:bimcraft:api";

/// The JSON Schema of a descriptor's parameters and result.
#[derive(Clone, Debug, PartialEq)]
pub struct Signature {
    /// What the parameters must look like.
    pub params: Value,
    /// What the result looks like.
    pub result: Value,
}

/// A reference to one of the shared definitions of the protocol schema, such as `EntityId`.
#[must_use]
pub fn def(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{name}") })
}

/// `schema`, or `null`.
#[must_use]
pub fn nullable(schema: Value) -> Value {
    with(
        "anyOf",
        Value::Array(vec![schema, json!({ "type": "null" })]),
        json!({}),
    )
}

/// An array of items matching `schema`.
#[must_use]
pub fn array_of(schema: Value) -> Value {
    with("items", schema, json!({ "type": "array" }))
}

/// A JSON object with exactly these fields, the `required` ones always present.
#[must_use]
pub fn object(required: &[(&str, Value)], optional: &[(&str, Value)]) -> Value {
    let properties: Map<String, Value> = required
        .iter()
        .chain(optional)
        .map(|(name, schema)| ((*name).to_owned(), schema.clone()))
        .collect();
    let required: Vec<&str> = required.iter().map(|(name, _)| *name).collect();
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

/// A two-item array: a name and a value of `schema`, as name/value pairs are serialised.
#[must_use]
pub fn pair(schema: Value) -> Value {
    let items = Value::Array(vec![json!({ "type": "string" }), schema]);
    with(
        "prefixItems",
        items,
        json!({ "type": "array", "items": false, "minItems": 2 }),
    )
}

/// `base`, an object, with `keyword` set to `schema`.
fn with(keyword: &str, schema: Value, mut base: Value) -> Value {
    if let Value::Object(map) = &mut base {
        map.insert(keyword.to_owned(), schema);
    }
    base
}

/// The `null` result of a call that returns nothing.
#[must_use]
pub fn none() -> Value {
    json!({ "type": "null" })
}

/// One tagged variant of an attribute value, such as `{"Real": 2.5}`.
fn variant(tag: &str, schema: &Value) -> Value {
    object(&[(tag, schema.clone())], &[])
}

/// The definitions every descriptor refers to with [`def`].
fn shared() -> Map<String, Value> {
    let entity_id = json!({
        "description": "The stable 128-bit identifier of an object, as 32 lower-case hexadecimal digits.",
        "type": "string",
        "pattern": "^[0-9a-f]{32}$",
    });
    let value = json!({
        "description": "An attribute or property value; reals are in SI units.",
        "oneOf": [
            variant("Bool", &json!({ "type": "boolean" })),
            variant("Integer", &json!({ "type": "integer" })),
            variant("Real", &json!({ "type": "number" })),
            variant("Text", &json!({ "type": "string" })),
            variant("Ref", &def("EntityId")),
            variant("List", &array_of(def("AttributeValue"))),
        ],
    });
    let text = || json!({ "type": "string" });
    let origin = json!({
        "description": "Who issued a command.",
        "oneOf": [
            object(&[("kind", json!({ "const": "ui" }))], &[]),
            object(&[("kind", json!({ "const": "plugin" })), ("name", text())], &[]),
            object(&[("kind", json!({ "const": "ai_session" })), ("id", text())], &[]),
        ],
    });
    let ids = || json!({ "type": "array", "items": def("EntityId"), "uniqueItems": true });
    let event = object(
        &[("created", ids()), ("modified", ids()), ("deleted", ids())],
        &[],
    );
    Map::from_iter([
        ("AttributeValue".to_owned(), value),
        ("ChangeEvent".to_owned(), event),
        ("EntityId".to_owned(), entity_id),
        ("NameValue".to_owned(), pair(def("AttributeValue"))),
        (
            "ModelCommand".to_owned(),
            crate::commands::model_command_schema(),
        ),
        ("Origin".to_owned(), origin),
    ])
}

/// One registered command or query: its name and version, and the shape of its parameters and
/// result.
fn entry(name: &str, version: &str, summary: &str, signature: &Signature) -> Value {
    json!({
        "description": summary,
        "type": "object",
        "properties": {
            "name": { "const": name },
            "version": { "const": version },
            "params": signature.params,
            "result": signature.result,
        },
    })
}

/// The protocol schema: every command and query of `registry` under `$defs/commands` and
/// `$defs/queries`, keyed by name, the definitions they share (`ChangeEvent` is what a command
/// reports), and `entities`, the entity schema, embedded unchanged under `$defs/entities`.
#[must_use]
pub fn protocol_schema<Ctx>(registry: &Registry<Ctx>, entities: Value) -> Value {
    let mut commands = Map::new();
    let mut queries = Map::new();
    for descriptor in registry.iter() {
        let version = descriptor.version().to_string();
        let entry = entry(
            descriptor.name(),
            &version,
            descriptor.summary(),
            descriptor.signature(),
        );
        let list = match descriptor.kind() {
            Kind::Command => &mut commands,
            Kind::Query => &mut queries,
        };
        // The newest version is listed under the bare name, older ones under `name@version`.
        let newest = registry.get(descriptor.name()).map(Descriptor::version);
        let key = if newest == Ok(descriptor.version()) {
            descriptor.name().to_owned()
        } else {
            format!("{}@{version}", descriptor.name())
        };
        list.insert(key, entry);
    }
    let mut defs = shared();
    defs.insert("commands".to_owned(), Value::Object(commands));
    defs.insert("queries".to_owned(), Value::Object(queries));
    defs.insert("entities".to_owned(), entities);
    json!({
        "$schema": DRAFT,
        "$id": ID,
        "$comment": "@generated by the api crate from its registry. Do not edit.",
        "title": "BIMCraft command API",
        "description": "The commands and queries clients send, the changes they report, and the objects they read.",
        "$defs": defs,
    })
}

#[cfg(test)]
mod tests;
