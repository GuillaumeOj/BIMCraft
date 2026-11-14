//! The `elements.*` commands: edits to the placement of elements that the viewport asks for
//! (VP-01, NFR-03).
//!
//! `elements.move` runs as one batch, so it is one undo step, is recorded with the caller's
//! origin and reports one change event, however many elements move.

use std::collections::BTreeSet;

use core_model::{AttributeValue, EntityId, Model};
use core_txn::{ModelCommand, Origin};
use scene_wire::MAX_WORLD_COORDINATE;
use serde::Deserialize;
use serde_json::json;

use crate::georeference::point_schema;
use crate::schema::{array_of, id, object};
use crate::{ApiError, Descriptor, Outcome, Session, Signature, Version};

const V1: Version = Version::new(1, 0, 0);

/// The most elements one `elements.move` may carry. The UI never splits a move.
pub const MAX_MOVED: usize = 10_000;

/// The largest translation component, in metres.
pub const MAX_TRANSLATION: f64 = 1e6;

/// The entities `elements.move` knows how to translate, with the attribute that holds their
/// location: three lengths. Walls and the rest join as they arrive.
const MOVABLE: [(&str, &str); 1] = [("Block", "location")];

/// Why an `elements.*` call was refused. Each has a stable `reason` the UI maps to a message,
/// and none repeats a value of the model (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ElementsError {
    /// No id was given.
    #[error("no element to move")]
    Empty,
    /// More than [`MAX_MOVED`] ids were given.
    #[error("too many elements to move at once")]
    TooMany,
    /// An id is not an object of the model.
    #[error("an element does not exist")]
    UnknownElement,
    /// An element has no placement the command knows how to translate.
    #[error("an element cannot be moved")]
    NotMovable,
    /// The translation is not finite or beyond [`MAX_TRANSLATION`], or an element would leave
    /// the world extent.
    #[error("the translation is out of range")]
    OutOfRange,
}

impl ElementsError {
    /// The stable `snake_case` name of the error, the `data.reason` of its response.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooMany => "too_many",
            Self::UnknownElement => "unknown_element",
            Self::NotMovable => "not_movable",
            Self::OutOfRange => "out_of_range",
        }
    }
}

/// The element commands.
pub(crate) fn descriptors() -> [Descriptor<Session>; 1] {
    let signature = Signature {
        params: object(
            &[("ids", array_of(id())), ("translation", point_schema())],
            &[],
        ),
        result: json!({ "type": "null" }),
    };
    [Descriptor::command(
        "elements.move",
        V1,
        "Moves elements by a translation, as one undo step.",
        signature,
        move_elements,
    )]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveParams {
    ids: Vec<EntityId>,
    translation: [f64; 3],
}

fn move_elements(
    session: &mut Session,
    MoveParams { ids, translation }: MoveParams,
    origin: &Origin,
) -> Result<Outcome<()>, ApiError> {
    let command = plan(session.document().model(), &ids, translation)?;
    let event = session.document_mut().execute(command, origin.clone())?;
    Ok(Outcome {
        result: (),
        event: Some(event),
    })
}

/// The command that moves `ids`, or why it is refused; nothing changes either way.
fn plan(model: &Model, ids: &[EntityId], translation: [f64; 3]) -> Result<ModelCommand, ApiError> {
    if ids.is_empty() {
        return Err(ElementsError::Empty.into());
    }
    if ids.len() > MAX_MOVED {
        return Err(ElementsError::TooMany.into());
    }
    if translation
        .iter()
        .any(|c| !c.is_finite() || c.abs() > MAX_TRANSLATION)
    {
        return Err(ElementsError::OutOfRange.into());
    }
    let mut seen = BTreeSet::new();
    let mut commands = Vec::with_capacity(ids.len());
    for &id in ids {
        if !seen.insert(id) {
            // Moving an element twice is a client defect, not a refusal the user can meet.
            return Err(ApiError::InvalidParams {
                detail: "ids are not distinct".to_owned(),
            });
        }
        commands.push(translated(model, id, translation)?);
    }
    Ok(ModelCommand::Batch { commands })
}

/// The command that sets the location of `id` translated by `translation`.
fn translated(
    model: &Model,
    id: EntityId,
    translation: [f64; 3],
) -> Result<ModelCommand, ElementsError> {
    let entity = model.entity_of(id).ok_or(ElementsError::UnknownElement)?;
    let (_, attribute) = MOVABLE
        .iter()
        .find(|(name, _)| *name == entity)
        .ok_or(ElementsError::NotMovable)?;
    let location = location(model, id, attribute)?;
    let moved: Vec<AttributeValue> = location
        .iter()
        .zip(translation)
        .map(|(c, t)| c + t)
        .map(|c| {
            if c.abs() <= MAX_WORLD_COORDINATE {
                Ok(AttributeValue::Real(c))
            } else {
                Err(ElementsError::OutOfRange)
            }
        })
        .collect::<Result<_, _>>()?;
    Ok(ModelCommand::Set {
        id,
        attribute: (*attribute).to_owned(),
        value: Some(AttributeValue::List(moved)),
    })
}

/// The three coordinates stored in `attribute`; an element whose location is unset or malformed
/// is not movable.
fn location(model: &Model, id: EntityId, attribute: &str) -> Result<[f64; 3], ElementsError> {
    let value = model
        .get(id, attribute)
        .ok()
        .flatten()
        .ok_or(ElementsError::NotMovable)?;
    let AttributeValue::List(items) = value.plain() else {
        return Err(ElementsError::NotMovable);
    };
    let reals: Vec<f64> = items
        .iter()
        .map(|item| match item.plain() {
            AttributeValue::Real(v) => Some(*v),
            _ => None,
        })
        .collect::<Option<_>>()
        .ok_or(ElementsError::NotMovable)?;
    <[f64; 3]>::try_from(reals).map_err(|_| ElementsError::NotMovable)
}

#[cfg(test)]
mod tests;
