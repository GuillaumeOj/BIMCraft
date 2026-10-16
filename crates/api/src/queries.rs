//! The read-only queries the UI and plugins need first (API-01).

use core_model::{AttributeValue, EntityId, ObjectState};
use core_txn::Origin;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::schema::{array_of, def, nullable, object};
use crate::{ApiError, Descriptor, Session, Signature, Version};

const V1: Version = Version::new(1, 0, 0);

/// The page size when the caller gives none.
pub const DEFAULT_PAGE_SIZE: usize = 100;

/// The largest page a caller may ask for, so one call never returns the whole model.
pub const MAX_PAGE_SIZE: usize = 1000;

/// The queries, sorted by name.
pub(crate) fn descriptors() -> [Descriptor<Session>; 4] {
    let id = || def("EntityId");
    let text = || json!({ "type": "string" });
    let boolean = || json!({ "type": "boolean" });
    let by_id = || object(&[("id", id())], &[]);
    let values = || array_of(def("NameValue"));
    let entry = object(
        &[
            ("command", text()),
            ("origin", def("Origin")),
            ("undone", boolean()),
        ],
        &[],
    );
    let limit = || json!({ "type": "integer", "minimum": 1, "maximum": MAX_PAGE_SIZE });
    let index = || json!({ "type": "integer", "minimum": 0 });
    let history_state = Signature {
        params: object(
            &[],
            &[("start", nullable(index())), ("limit", nullable(limit()))],
        ),
        result: object(
            &[
                ("can_undo", boolean()),
                ("can_redo", boolean()),
                ("entries", array_of(entry)),
                ("next", nullable(index())),
            ],
            &[],
        ),
    };
    let get_object = Signature {
        params: by_id(),
        result: object(
            &[
                ("id", id()),
                ("entity", text()),
                ("attributes", values()),
                ("type_of", nullable(id())),
                ("component", nullable(id())),
            ],
            &[],
        ),
    };
    let list_objects = Signature {
        params: object(
            &[("entity", text())],
            &[("after", nullable(id())), ("limit", nullable(limit()))],
        ),
        result: object(&[("items", array_of(id())), ("next", nullable(id()))], &[]),
    };
    let property_sets = Signature {
        params: by_id(),
        result: array_of(object(&[("set", text()), ("values", values())], &[])),
    };
    [
        Descriptor::query(
            "history.state",
            V1,
            "Reads what undo and redo would do.",
            history_state,
            history,
        ),
        Descriptor::query(
            "model.get_object",
            V1,
            "Reads an object and its attributes.",
            get_object,
            object_of,
        ),
        Descriptor::query(
            "model.list_objects",
            V1,
            "Lists the objects of an entity.",
            list_objects,
            list,
        ),
        Descriptor::query(
            "model.property_sets",
            V1,
            "Reads the property sets of an object.",
            property_sets,
            sets,
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ById {
    id: EntityId,
}

/// An object as stored: values set on its type or left to their default are not listed.
#[derive(Debug, PartialEq, Serialize)]
struct Object {
    id: EntityId,
    entity: String,
    attributes: Vec<(String, AttributeValue)>,
    type_of: Option<EntityId>,
    component: Option<EntityId>,
}

fn state(session: &Session, id: EntityId) -> Result<ObjectState, ApiError> {
    session
        .document()
        .model()
        .state(id)
        .ok_or(ApiError::NotFound)
}

fn object_of(session: &Session, ById { id }: ById) -> Result<Object, ApiError> {
    let state = state(session, id)?;
    Ok(Object {
        id,
        entity: state.entity,
        attributes: state.values,
        type_of: state.type_of,
        component: state.component,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListObjects {
    entity: String,
    #[serde(default)]
    after: Option<EntityId>,
    #[serde(default)]
    limit: Option<usize>,
}

/// One page of ids, and the cursor to pass as `after` for the next one.
#[derive(Debug, PartialEq, Serialize)]
struct Page {
    items: Vec<EntityId>,
    next: Option<EntityId>,
}

/// Objects are paged in id order, which, unlike storage order, does not change as objects are
/// removed, so a cursor stays valid between calls.
fn list(
    session: &Session,
    ListObjects {
        entity,
        after,
        limit,
    }: ListObjects,
) -> Result<Page, ApiError> {
    let limit = page_size(limit)?;
    let model = session.document().model();
    // The model keeps no index by entity, so each page scans every object.
    let mut ids: Vec<EntityId> = model
        .ids()
        .filter(|&id| after.is_none_or(|after| id > after))
        .filter(|&id| model.entity_of(id) == Some(entity.as_str()))
        .collect();
    ids.sort_unstable();
    let next = (ids.len() > limit).then(|| ids[limit - 1]);
    ids.truncate(limit);
    Ok(Page { items: ids, next })
}

#[derive(Debug, PartialEq, Serialize)]
struct PropertySet {
    set: String,
    values: Vec<(String, AttributeValue)>,
}

fn sets(session: &Session, ById { id }: ById) -> Result<Vec<PropertySet>, ApiError> {
    let mut sets: Vec<_> = state(session, id)?
        .properties
        .into_iter()
        .map(|(set, values)| PropertySet { set, values })
        .collect();
    sets.sort_unstable_by(|a, b| a.set.cmp(&b.set));
    Ok(sets)
}

/// The page size asked for, or the default.
fn page_size(limit: Option<usize>) -> Result<usize, ApiError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_SIZE);
    if (1..=MAX_PAGE_SIZE).contains(&limit) {
        Ok(limit)
    } else {
        Err(ApiError::InvalidParams {
            detail: format!("limit must be between 1 and {MAX_PAGE_SIZE}"),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryState {
    #[serde(default)]
    start: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

/// The history as it may be shown: command names and origins, never their arguments (SEC-03).
#[derive(Debug, PartialEq, Serialize)]
struct History {
    can_undo: bool,
    can_redo: bool,
    entries: Vec<Entry>,
    next: Option<usize>,
}

#[derive(Debug, PartialEq, Serialize)]
struct Entry {
    command: String,
    origin: Origin,
    undone: bool,
}

/// One page of the history, oldest command first, from position `start`; `next` is the
/// position of the following page. Every command in the history is a model command, so each
/// entry names the method that runs it, such as `model.create`.
fn history(
    session: &Session,
    HistoryState { start, limit }: HistoryState,
) -> Result<History, ApiError> {
    let limit = page_size(limit)?;
    let start = start.unwrap_or(0);
    let document = session.document();
    let mut entries = document.history().skip(start);
    let page = entries
        .by_ref()
        .take(limit)
        .map(|entry| Entry {
            command: format!("model.{}", entry.command),
            origin: entry.origin.clone(),
            undone: entry.undone,
        })
        .collect();
    Ok(History {
        can_undo: document.can_undo(),
        can_redo: document.can_redo(),
        entries: page,
        next: entries.next().map(|_| start + limit),
    })
}

#[cfg(test)]
mod tests;
