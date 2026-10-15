//! Atomic transactions (CORE-06).

use core_model::{AttributeValue, Component, EntityId, Model, ModelError, Removal};

use crate::{Command, CommandError};

/// Names and values as a model method takes them, owned for the journal.
type Values = Vec<(String, AttributeValue)>;

/// One change a transaction made, with what it takes to undo and redo it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Change {
    /// An object was created; undone by removing it.
    Inserted {
        id: EntityId,
        entity: String,
        values: Values,
    },
    /// An attribute went from `old` to `new`.
    Set {
        id: EntityId,
        attribute: String,
        old: Option<AttributeValue>,
        new: Option<AttributeValue>,
    },
    /// An object was removed with the relationships it left empty; undone by restoring them.
    Removed { id: EntityId, removal: Removal },
    /// A relationship was created; undone by removing it.
    Related {
        id: EntityId,
        entity: String,
        roles: Vec<(String, Vec<EntityId>)>,
        values: Values,
    },
    /// A participant joined a role at `position`, or left it from there.
    Participant {
        relationship: EntityId,
        role: String,
        participant: EntityId,
        position: usize,
        joined: bool,
    },
    /// A property went from `old` to `new`; `attached` when that gave the object the set.
    Property {
        id: EntityId,
        set: String,
        property: String,
        old: Option<AttributeValue>,
        new: Option<AttributeValue>,
        attached: bool,
    },
    /// An object was given a property set with these values, or lost it with them.
    PropertySet {
        id: EntityId,
        set: String,
        values: Values,
        attached: bool,
    },
    /// A component was created, or removed.
    Component {
        id: EntityId,
        component: Component,
        created: bool,
    },
    /// A type was made from a component; undone by removing it.
    TypeCreated {
        id: EntityId,
        component: EntityId,
        values: Values,
    },
    /// An instance's type went from `old` to `new`.
    TypeAssigned {
        instance: EntityId,
        old: Option<EntityId>,
        new: Option<EntityId>,
    },
}

/// Names and values as a model method takes them, without copying the names.
pub(crate) fn borrowed(
    values: &[(String, AttributeValue)],
) -> impl Iterator<Item = (&str, AttributeValue)> {
    values
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
}

fn owned<N: AsRef<str>>(values: impl IntoIterator<Item = (N, AttributeValue)>) -> Values {
    values
        .into_iter()
        .map(|(name, value)| (name.as_ref().to_owned(), value))
        .collect()
}

impl Change {
    /// Makes the change again (`forward`) or reverts it. Each change replays exactly what the
    /// model did, so it applies again to the model it was recorded on.
    fn apply(&self, model: &mut Model, forward: bool) -> Result<(), ModelError> {
        // The direction in which the change creates something; the other one takes it away.
        let creating = |created: bool| created == forward;
        match self {
            Self::Inserted { id, entity, values } if forward => {
                model.insert(*id, entity, borrowed(values))
            }
            Self::Related {
                id,
                entity,
                roles,
                values,
            } if forward => model.relate(*id, entity, roles.clone(), borrowed(values)),
            Self::TypeCreated {
                id,
                component,
                values,
            } if forward => model.create_type(*id, *component, borrowed(values)),
            Self::Inserted { id, .. } | Self::Related { id, .. } | Self::TypeCreated { id, .. } => {
                model.remove(*id).map(drop)
            }
            Self::Removed { id, .. } if forward => model.remove(*id).map(drop),
            Self::Removed { removal, .. } => model.restore(removal.clone()),
            Self::Set {
                id,
                attribute,
                old,
                new,
            } => {
                let value = if forward { new } else { old };
                model.set(*id, attribute, value.clone()).map(drop)
            }
            Self::Participant {
                relationship,
                role,
                participant,
                position,
                joined,
            } => {
                if creating(*joined) {
                    model
                        .insert_participant(*relationship, role, *participant, *position)
                        .map(drop)
                } else {
                    model
                        .remove_participant(*relationship, role, *participant)
                        .map(drop)
                }
            }
            Self::Property {
                id,
                set,
                attached: true,
                ..
            } if !forward => model.detach_property_set(*id, set).map(drop),
            Self::Property {
                id,
                set,
                property,
                old,
                new,
                ..
            } => {
                let value = if forward { new } else { old };
                model
                    .set_property(*id, set, property, value.clone())
                    .map(drop)
            }
            Self::PropertySet {
                id,
                set,
                values,
                attached,
            } => {
                if creating(*attached) {
                    model.attach_property_set(*id, set, borrowed(values))
                } else {
                    model.detach_property_set(*id, set).map(drop)
                }
            }
            Self::Component {
                id,
                component,
                created,
            } => {
                if creating(*created) {
                    model.create_component(*id, component.clone())
                } else {
                    model.remove_component(*id).map(drop)
                }
            }
            Self::TypeAssigned { instance, old, new } => {
                let type_id = if forward { new } else { old };
                model.assign_type(*instance, *type_id).map(drop)
            }
        }
    }
}

/// Reverts `changes`, last first.
pub(crate) fn undo_all(model: &mut Model, changes: &[Change]) -> Result<(), ModelError> {
    changes
        .iter()
        .rev()
        .try_for_each(|change| change.apply(model, false))
}

/// Makes `changes` again, first first.
pub(crate) fn redo_all(model: &mut Model, changes: &[Change]) -> Result<(), ModelError> {
    changes
        .iter()
        .try_for_each(|change| change.apply(model, true))
}

/// The model as a command sees it while it runs: every write is recorded, so the whole command
/// can be rolled back, undone and redone (CORE-06).
///
/// Each write method behaves as the [`Model`] method of the same name. Adding a property set
/// definition is not a change to the model's content and is not offered here.
#[derive(Debug)]
pub struct Transaction<'a> {
    model: &'a mut Model,
    changes: Vec<Change>,
}

impl<'a> Transaction<'a> {
    fn new(model: &'a mut Model) -> Self {
        Self {
            model,
            changes: Vec::new(),
        }
    }

    /// Runs `command` on `model` as one transaction and returns the changes it made. Either the
    /// command applies fully, or the model is left as it was.
    ///
    /// # Errors
    ///
    /// Returns the command's error, after rolling back what it had changed.
    pub(crate) fn run<C: Command>(
        model: &'a mut Model,
        command: &C,
    ) -> Result<Vec<Change>, CommandError> {
        let mut transaction = Self::new(model);
        match command.apply(&mut transaction) {
            Ok(()) => Ok(transaction.changes),
            Err(error) => {
                // Each recorded change succeeded on this model, so its inverse applies.
                undo_all(transaction.model, &transaction.changes)?;
                Err(error)
            }
        }
    }

    /// The model, with the changes made so far.
    #[must_use]
    pub fn model(&self) -> &Model {
        self.model
    }

    /// Makes a change that returns nothing the caller needs, and records it.
    fn perform(&mut self, change: Change) -> Result<(), ModelError> {
        change.apply(self.model, true)?;
        self.changes.push(change);
        Ok(())
    }

    /// See [`Model::insert`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::insert`] does.
    pub fn insert<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        entity: &str,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        self.perform(Change::Inserted {
            id,
            entity: entity.to_owned(),
            values: owned(values),
        })
    }

    /// See [`Model::set`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::set`] does.
    pub fn set(
        &mut self,
        id: EntityId,
        attribute: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let old = self.model.set(id, attribute, value.clone())?;
        let change = Change::Set {
            id,
            attribute: attribute.to_owned(),
            old: old.clone(),
            new: value,
        };
        self.changes.push(change);
        Ok(old)
    }

    /// See [`Model::remove`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::remove`] does.
    pub fn remove(&mut self, id: EntityId) -> Result<Removal, ModelError> {
        let removal = self.model.remove(id)?;
        let change = Change::Removed {
            id,
            removal: removal.clone(),
        };
        self.changes.push(change);
        Ok(removal)
    }

    /// See [`Model::relate`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::relate`] does.
    pub fn relate<R: AsRef<str>, N: AsRef<str>>(
        &mut self,
        id: EntityId,
        entity: &str,
        roles: impl IntoIterator<Item = (R, Vec<EntityId>)>,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        self.perform(Change::Related {
            id,
            entity: entity.to_owned(),
            roles: roles
                .into_iter()
                .map(|(role, participants)| (role.as_ref().to_owned(), participants))
                .collect(),
            values: owned(values),
        })
    }

    /// See [`Model::add_participant`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::add_participant`] does.
    pub fn add_participant(
        &mut self,
        relationship: EntityId,
        role: &str,
        participant: EntityId,
    ) -> Result<usize, ModelError> {
        let position = self
            .model
            .add_participant(relationship, role, participant)?;
        self.changes.push(Change::Participant {
            relationship,
            role: role.to_owned(),
            participant,
            position,
            joined: true,
        });
        Ok(position)
    }

    /// See [`Model::remove_participant`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::remove_participant`] does.
    pub fn remove_participant(
        &mut self,
        relationship: EntityId,
        role: &str,
        participant: EntityId,
    ) -> Result<usize, ModelError> {
        let position = self
            .model
            .remove_participant(relationship, role, participant)?;
        self.changes.push(Change::Participant {
            relationship,
            role: role.to_owned(),
            participant,
            position,
            joined: false,
        });
        Ok(position)
    }

    /// See [`Model::set_property`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::set_property`] does.
    pub fn set_property(
        &mut self,
        id: EntityId,
        set: &str,
        property: &str,
        value: Option<AttributeValue>,
    ) -> Result<Option<AttributeValue>, ModelError> {
        let carries_set = |model: &Model| model.property_sets_of(id).any(|name| name == set);
        let carried = carries_set(self.model);
        let old = self.model.set_property(id, set, property, value.clone())?;
        let change = Change::Property {
            id,
            set: set.to_owned(),
            property: property.to_owned(),
            old: old.clone(),
            new: value,
            // Setting a property of a set the object does not carry gives it the set.
            attached: !carried && carries_set(self.model),
        };
        self.changes.push(change);
        Ok(old)
    }

    /// See [`Model::attach_property_set`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::attach_property_set`] does.
    pub fn attach_property_set<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        set: &str,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        self.perform(Change::PropertySet {
            id,
            set: set.to_owned(),
            values: owned(values),
            attached: true,
        })
    }

    /// See [`Model::detach_property_set`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::detach_property_set`] does.
    pub fn detach_property_set(&mut self, id: EntityId, set: &str) -> Result<Values, ModelError> {
        let values = self.model.detach_property_set(id, set)?;
        let change = Change::PropertySet {
            id,
            set: set.to_owned(),
            values: values.clone(),
            attached: false,
        };
        self.changes.push(change);
        Ok(values)
    }

    /// See [`Model::create_component`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::create_component`] does.
    pub fn create_component(
        &mut self,
        id: EntityId,
        component: Component,
    ) -> Result<(), ModelError> {
        self.perform(Change::Component {
            id,
            component,
            created: true,
        })
    }

    /// See [`Model::remove_component`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::remove_component`] does.
    pub fn remove_component(&mut self, id: EntityId) -> Result<Component, ModelError> {
        let component = self.model.remove_component(id)?;
        let change = Change::Component {
            id,
            component: component.clone(),
            created: false,
        };
        self.changes.push(change);
        Ok(component)
    }

    /// See [`Model::create_type`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::create_type`] does.
    pub fn create_type<N: AsRef<str>>(
        &mut self,
        id: EntityId,
        component: EntityId,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        self.perform(Change::TypeCreated {
            id,
            component,
            values: owned(values),
        })
    }

    /// See [`Model::assign_type`].
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, as [`Model::assign_type`] does.
    pub fn assign_type(
        &mut self,
        instance: EntityId,
        type_id: Option<EntityId>,
    ) -> Result<Option<EntityId>, ModelError> {
        let old = self.model.assign_type(instance, type_id)?;
        let change = Change::TypeAssigned {
            instance,
            old,
            new: type_id,
        };
        self.changes.push(change);
        Ok(old)
    }
}

#[cfg(test)]
mod tests;
