//! The model commands, undo and redo (API-01, API-05).

use core_txn::{ChangeEvent, ModelCommand, Origin};
use serde_json::{Map, Value};

use crate::descriptor::{NoParams, decode};
use crate::{ApiError, Descriptor, Outcome, Session, Version};

const V1: Version = Version::new(1, 0, 0);

/// Each [`ModelCommand`] variant: its API name, its serde tag and a summary.
const MODEL_COMMANDS: [(&str, &str, &str); 14] = [
    (
        "model.create",
        "create",
        "Creates an object that is not a relationship.",
    ),
    (
        "model.set",
        "set",
        "Sets, or clears, one attribute of an object.",
    ),
    ("model.delete", "delete", "Deletes an object."),
    (
        "model.relate",
        "relate",
        "Creates a relationship between objects.",
    ),
    (
        "model.add_participant",
        "add_participant",
        "Adds an object to a role of a relationship.",
    ),
    (
        "model.remove_participant",
        "remove_participant",
        "Removes an object from a role of a relationship.",
    ),
    (
        "model.set_property",
        "set_property",
        "Sets, or clears, one property of an object.",
    ),
    (
        "model.attach_property_set",
        "attach_property_set",
        "Attaches a property set to an object.",
    ),
    (
        "model.detach_property_set",
        "detach_property_set",
        "Detaches a property set from an object.",
    ),
    (
        "model.create_component",
        "create_component",
        "Creates a component from a library.",
    ),
    (
        "model.remove_component",
        "remove_component",
        "Removes a component no type uses.",
    ),
    (
        "model.create_type",
        "create_type",
        "Creates a type from a component.",
    ),
    (
        "model.assign_type",
        "assign_type",
        "Assigns a type to an object, or removes it.",
    ),
    (
        "model.batch",
        "batch",
        "Runs several commands as one undoable step.",
    ),
];

/// The model commands, then undo and redo.
pub(crate) fn descriptors() -> impl Iterator<Item = Descriptor<Session>> {
    let model = MODEL_COMMANDS.into_iter().map(|(name, tag, summary)| {
        Descriptor::command(
            name,
            V1,
            summary,
            move |session: &mut Session, params: Map<String, Value>, origin: &Origin| {
                execute(session, model_command(tag, params)?, origin)
            },
        )
    });
    model.chain([
        Descriptor::command("history.undo", V1, "Undoes the last command.", undo),
        Descriptor::command("history.redo", V1, "Redoes the last undone command.", redo),
    ])
}

/// Decodes the fields of the variant tagged `tag`. The tag comes from the name called, never
/// from the parameters, so the existing serde form of [`ModelCommand`] is the only decoder.
fn model_command(tag: &str, mut params: Map<String, Value>) -> Result<ModelCommand, ApiError> {
    if params.contains_key("command") {
        return Err(ApiError::InvalidParams {
            detail: "unknown field `command`".to_owned(),
        });
    }
    params.insert("command".to_owned(), Value::String(tag.to_owned()));
    decode(Value::Object(params))
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
