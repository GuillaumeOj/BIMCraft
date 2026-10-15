//! Change events (CORE-07).

use std::collections::{BTreeMap, BTreeSet};

use core_model::{EntityId, Model, ObjectState};
use serde::{Deserialize, Serialize};

/// The objects a transaction, an undo or a redo changed (CORE-07).
///
/// Each list holds the net effect, comparing each object's [`ObjectState`] before and after: an
/// object created then deleted by the same command appears nowhere, and one deleted then created
/// again with another state is modified. Components are definitions, not objects, and do not
/// appear.
///
/// States hold stored values, so a change to a type lists the type, not the instances that
/// inherit the changed value: [`Model::inheriting_instances`] lists those for whatever derives
/// data from them (CORE-08).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// Objects that did not exist before and exist after.
    pub created: BTreeSet<EntityId>,
    /// Objects that exist before and after in a different state.
    pub modified: BTreeSet<EntityId>,
    /// Objects that existed before and no longer exist.
    pub deleted: BTreeSet<EntityId>,
}

impl ChangeEvent {
    /// Compares the objects as they were before a change, `None` for those that did not exist,
    /// with what `model` now holds.
    pub(crate) fn between(before: BTreeMap<EntityId, Option<ObjectState>>, model: &Model) -> Self {
        let mut event = Self::default();
        for (id, before) in before {
            // An object that did not exist before needs no state, only its existence.
            match before {
                None if model.contains(id) => event.created.insert(id),
                None => false,
                Some(before) => match model.state(id) {
                    None => event.deleted.insert(id),
                    Some(after) => before != after && event.modified.insert(id),
                },
            };
        }
        event
    }

    /// Whether nothing changed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }

    /// The event of the opposite change, as undoing it emits.
    #[must_use]
    pub fn inverse(&self) -> Self {
        Self {
            created: self.deleted.clone(),
            modified: self.modified.clone(),
            deleted: self.created.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
