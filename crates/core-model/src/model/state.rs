//! The state of one object, and model equality.

use super::Model;
use crate::{AttributeValue, EntityId};

/// Everything a model holds about one object: what tells whether a change modified it (CORE-07).
///
/// Values are the stored ones, so an instance that inherits a value from its type does not
/// change when the type does. The roles an object plays belong to the relationships, not to it.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectState {
    /// The object's entity, or type name for a type object.
    pub entity: String,
    /// The attributes that have a stored value, in declaration order.
    pub values: Vec<(String, AttributeValue)>,
    /// The participants of a relationship, role by role in declaration order; empty for other
    /// objects.
    pub roles: Vec<Vec<EntityId>>,
    /// The property sets the object carries, sorted by name, with their stored values.
    pub properties: Vec<(String, Vec<(String, AttributeValue)>)>,
    /// The type of an instance.
    pub type_of: Option<EntityId>,
    /// The component a type object was made from.
    pub component: Option<EntityId>,
}

impl Model {
    /// The ids of every object, relationships included, in a deterministic order that changes
    /// when objects are removed.
    pub fn ids(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.store.ids()
    }

    /// The state of the object with this id, if it exists.
    #[must_use]
    pub fn state(&self, id: EntityId) -> Option<ObjectState> {
        let object = self.store.entity(id)?;
        Some(ObjectState {
            entity: object.entity,
            values: object.values,
            roles: self
                .relationships
                .roles_of(id)
                .map(<[_]>::to_vec)
                .unwrap_or_default(),
            properties: self
                .property_sets
                .iter()
                .filter_map(|(name, table)| Some((name.clone(), table.values(id)?)))
                .collect(),
            type_of: self.types.type_of(id),
            component: self.component_of(id),
        })
    }
}

/// Two models are equal when they have the same schema, custom property sets included, the same
/// components, and the same objects in the same state, whatever the order they are stored in.
impl PartialEq for Model {
    fn eq(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.types.same_components(&other.types)
            && self.len() == other.len()
            && self.ids().all(|id| self.state(id) == other.state(id))
    }
}

#[cfg(test)]
mod tests;
