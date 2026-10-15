//! The model commands, undo and redo (API-01, API-05).

use core_txn::{ChangeEvent, ModelCommand, Origin};
use serde_json::{Map, Value, json};

use crate::descriptor::{NoParams, decode};
use crate::schema::{array_of, def, none, nullable, object, pair};
use crate::{ApiError, Descriptor, Outcome, Session, Signature, Version};

const V1: Version = Version::new(1, 0, 0);

/// The required and optional fields of a variant, with their schemas.
type Fields = (Vec<(&'static str, Value)>, Vec<(&'static str, Value)>);

/// A [`ModelCommand`] variant: its API name, its serde tag, a summary and its fields.
type Variant = (&'static str, &'static str, &'static str, fn() -> Fields);

const MODEL_COMMANDS: [Variant; 14] = [
    (
        "model.create",
        "create",
        "Creates an object that is not a relationship.",
        || {
            (
                vec![("id", id()), ("entity", text()), ("values", values())],
                vec![],
            )
        },
    ),
    (
        "model.set",
        "set",
        "Sets, or clears, one attribute of an object.",
        || {
            (
                vec![("id", id()), ("attribute", text())],
                vec![("value", value())],
            )
        },
    ),
    ("model.delete", "delete", "Deletes an object.", || {
        (vec![("id", id())], vec![])
    }),
    (
        "model.relate",
        "relate",
        "Creates a relationship between objects.",
        || {
            let roles = array_of(pair(array_of(id())));
            let fields = vec![
                ("id", id()),
                ("entity", text()),
                ("roles", roles),
                ("values", values()),
            ];
            (fields, vec![])
        },
    ),
    (
        "model.add_participant",
        "add_participant",
        "Adds an object to a role of a relationship.",
        participant,
    ),
    (
        "model.remove_participant",
        "remove_participant",
        "Removes an object from a role of a relationship.",
        participant,
    ),
    (
        "model.set_property",
        "set_property",
        "Sets, or clears, one property of an object.",
        || {
            (
                vec![("id", id()), ("set", text()), ("property", text())],
                vec![("value", value())],
            )
        },
    ),
    (
        "model.attach_property_set",
        "attach_property_set",
        "Attaches a property set to an object.",
        || {
            (
                vec![("id", id()), ("set", text()), ("values", values())],
                vec![],
            )
        },
    ),
    (
        "model.detach_property_set",
        "detach_property_set",
        "Detaches a property set from an object.",
        || (vec![("id", id()), ("set", text())], vec![]),
    ),
    (
        "model.create_component",
        "create_component",
        "Creates a component from a library.",
        || {
            let library = object(
                &[
                    ("id", text()),
                    (
                        "version",
                        json!({ "type": "integer", "minimum": 0, "maximum": u32::MAX }),
                    ),
                ],
                &[],
            );
            let component = object(
                &[
                    ("entity", text()),
                    ("library", library),
                    ("defaults", values()),
                ],
                &[],
            );
            (vec![("id", id()), ("component", component)], vec![])
        },
    ),
    (
        "model.remove_component",
        "remove_component",
        "Removes a component no type uses.",
        || (vec![("id", id())], vec![]),
    ),
    (
        "model.create_type",
        "create_type",
        "Creates a type from a component.",
        || {
            (
                vec![("id", id()), ("component", id()), ("values", values())],
                vec![],
            )
        },
    ),
    (
        "model.assign_type",
        "assign_type",
        "Assigns a type to an object, or removes it.",
        || (vec![("instance", id())], vec![("type_id", nullable(id()))]),
    ),
    (
        "model.batch",
        "batch",
        "Runs several commands as one undoable step.",
        || (vec![("commands", array_of(def("ModelCommand")))], vec![]),
    ),
];

fn id() -> Value {
    def("EntityId")
}

fn text() -> Value {
    json!({ "type": "string" })
}

fn value() -> Value {
    nullable(def("AttributeValue"))
}

fn values() -> Value {
    array_of(def("NameValue"))
}

fn participant() -> Fields {
    (
        vec![
            ("relationship", id()),
            ("role", text()),
            ("participant", id()),
        ],
        vec![],
    )
}

/// The serde form of a [`ModelCommand`], as nested in a batch: its fields and its tag.
pub(crate) fn model_command_schema() -> Value {
    let variants: Vec<Value> = MODEL_COMMANDS
        .iter()
        .map(|(_, tag, _, fields)| {
            let (mut required, optional) = fields();
            required.push(("command", json!({ "const": tag })));
            object(&required, &optional)
        })
        .collect();
    json!({ "description": "A model command, tagged with its kind.", "oneOf": variants })
}

/// The model commands, then undo and redo.
pub(crate) fn descriptors() -> impl Iterator<Item = Descriptor<Session>> {
    let model = MODEL_COMMANDS
        .into_iter()
        .map(|(name, tag, summary, fields)| {
            let (required, optional) = fields();
            let signature = Signature {
                params: object(&required, &optional),
                result: none(),
            };
            Descriptor::command(
                name,
                V1,
                summary,
                signature,
                move |session: &mut Session, params: Map<String, Value>, origin: &Origin| {
                    execute(session, model_command(tag, params)?, origin)
                },
            )
        });
    let no_params = || Signature {
        params: object(&[], &[]),
        result: none(),
    };
    model.chain([
        Descriptor::command(
            "history.undo",
            V1,
            "Undoes the last command.",
            no_params(),
            undo,
        ),
        Descriptor::command(
            "history.redo",
            V1,
            "Redoes the last undone command.",
            no_params(),
            redo,
        ),
    ])
}

/// Decodes the fields of the variant tagged `tag`. The tag comes from the name called, never
/// from the parameters, so the existing serde form of [`ModelCommand`] is the only decoder.
fn model_command(tag: &str, mut params: Map<String, Value>) -> Result<ModelCommand, ApiError> {
    check_fields(tag, &params, false)?;
    params.insert("command".to_owned(), Value::String(tag.to_owned()));
    decode(Value::Object(params))
}

/// Rejects fields the variant tagged `tag` does not have, in it and in the commands of a batch.
/// Serde ignores them, so an optional field under a wrong name, such as `values` for `value`, would
/// otherwise run the command without it: a set would clear the attribute. `tagged` says whether
/// `fields` carry their `command` tag, as commands nested in a batch do.
fn check_fields(tag: &str, fields: &Map<String, Value>, tagged: bool) -> Result<(), ApiError> {
    // An unknown tag is left to the decoder, which names it.
    let Some((_, _, _, known)) = MODEL_COMMANDS.iter().find(|variant| variant.1 == tag) else {
        return Ok(());
    };
    let (required, optional) = known();
    let is_known = |name: &str| {
        (tagged && name == "command")
            || required
                .iter()
                .chain(&optional)
                .any(|(field, _)| *field == name)
    };
    if let Some(unknown) = fields.keys().find(|name| !is_known(name)) {
        return Err(ApiError::InvalidParams {
            detail: format!("unknown field `{unknown}`"),
        });
    }
    let nested = fields
        .get("commands")
        .and_then(Value::as_array)
        .filter(|_| tag == "batch");
    for command in nested.into_iter().flatten() {
        if let Some(command) = command.as_object()
            && let Some(Value::String(tag)) = command.get("command")
        {
            check_fields(tag, command, true)?;
        }
    }
    Ok(())
}

fn execute(
    session: &mut Session,
    command: ModelCommand,
    origin: &Origin,
) -> Result<Outcome<()>, ApiError> {
    changed(session.document_mut().execute(command, origin.clone())?)
}

// Undo and redo are not recorded in the history, so their origin is not needed.
fn undo(session: &mut Session, _: NoParams, _: &Origin) -> Result<Outcome<()>, ApiError> {
    changed(session.document_mut().undo()?)
}

fn redo(session: &mut Session, _: NoParams, _: &Origin) -> Result<Outcome<()>, ApiError> {
    changed(session.document_mut().redo()?)
}

/// What every model command, undo and redo return: nothing, and the change they made.
#[expect(clippy::unnecessary_wraps, reason = "command handlers return a Result")]
fn changed(event: ChangeEvent) -> Result<Outcome<()>, ApiError> {
    Ok(Outcome {
        result: (),
        event: Some(event),
    })
}

#[cfg(test)]
mod tests;
