//! The command abstraction (CORE-11).

use core_model::{AttributeValue, Component, EntityId, Model, ModelError};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Transaction;
use crate::transaction::borrowed;

/// One change to a model, described as data.
///
/// A command carries everything it needs, new ids included, so applying the same command to
/// the same model always gives the same result, and a recorded log of commands rebuilds a model
/// from scratch ([`replay`]). Serialisation is what lets history, macros, replay and
/// collaboration share this one mechanism (CORE-11).
pub trait Command: Serialize + DeserializeOwned {
    /// A short, stable name for the kind of command, such as `"create"`. It never contains
    /// values from the model.
    fn name(&self) -> &'static str;

    /// Applies the command through `transaction`, which records every change so that a failure
    /// rolls the whole command back and the history can undo and redo it (CORE-06).
    ///
    /// # Errors
    ///
    /// Returns a [`CommandError`] when the command does not fit the model.
    fn apply(&self, transaction: &mut Transaction<'_>) -> Result<(), CommandError>;
}

/// Why a command could not be applied.
///
/// The messages never repeat values or ids, which come from a user's model (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CommandError {
    /// The model rejected a change.
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// The built-in commands, one per change the model offers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ModelCommand {
    /// Creates an object that is not a relationship; see [`Model::insert`].
    Create {
        /// The new object's id, chosen before the command runs so that replay reuses it.
        id: EntityId,
        /// The object's entity.
        entity: String,
        /// Attribute values; the others take their default or stay empty.
        values: Vec<(String, AttributeValue)>,
    },
    /// Sets, or with `None` clears, one attribute of an object; see [`Model::set`].
    Set {
        /// The object.
        id: EntityId,
        /// The attribute.
        attribute: String,
        /// The new value, or `None` to clear it.
        value: Option<AttributeValue>,
    },
    /// Deletes an object and the relationships it leaves empty; see [`Model::remove`].
    Delete {
        /// The object.
        id: EntityId,
    },
    /// Creates a relationship object; see [`Model::relate`].
    Relate {
        /// The new relationship's id.
        id: EntityId,
        /// The relationship entity, such as `Containment`.
        entity: String,
        /// The participants of each role.
        roles: Vec<(String, Vec<EntityId>)>,
        /// Attribute values of the relationship.
        values: Vec<(String, AttributeValue)>,
    },
    /// Adds a participant to a role of a relationship; see [`Model::add_participant`].
    AddParticipant {
        /// The relationship.
        relationship: EntityId,
        /// The role.
        role: String,
        /// The new participant.
        participant: EntityId,
    },
    /// Removes a participant from a role of a relationship; see [`Model::remove_participant`].
    RemoveParticipant {
        /// The relationship.
        relationship: EntityId,
        /// The role.
        role: String,
        /// The participant to remove.
        participant: EntityId,
    },
    /// Sets, or with `None` clears, one property of an object; see [`Model::set_property`].
    SetProperty {
        /// The object.
        id: EntityId,
        /// The property set.
        set: String,
        /// The property.
        property: String,
        /// The new value, or `None` to clear it.
        value: Option<AttributeValue>,
    },
    /// Gives an object a property set; see [`Model::attach_property_set`].
    AttachPropertySet {
        /// The object.
        id: EntityId,
        /// The property set.
        set: String,
        /// Property values; the others take their default or stay empty.
        values: Vec<(String, AttributeValue)>,
    },
    /// Takes a property set and its values off an object; see [`Model::detach_property_set`].
    DetachPropertySet {
        /// The object.
        id: EntityId,
        /// The property set.
        set: String,
    },
    /// Creates a component; see [`Model::create_component`].
    CreateComponent {
        /// The new component's id.
        id: EntityId,
        /// The component.
        component: Component,
    },
    /// Removes a component no type is made from; see [`Model::remove_component`].
    RemoveComponent {
        /// The component.
        id: EntityId,
    },
    /// Creates a type object from a component; see [`Model::create_type`].
    CreateType {
        /// The new type's id.
        id: EntityId,
        /// The component it is made from.
        component: EntityId,
        /// Type attribute values; the others take the component's or the schema's default.
        values: Vec<(String, AttributeValue)>,
    },
    /// Gives an instance a type, or with `None` none; see [`Model::assign_type`].
    AssignType {
        /// The instance.
        instance: EntityId,
        /// The type, or `None` to leave the instance untyped.
        type_id: Option<EntityId>,
    },
    /// Applies several commands, in order, as one: if one fails, none applies.
    ///
    /// Batches nest; deserialisers bound the depth (`serde_json` stops at 128 levels).
    Batch {
        /// The commands.
        commands: Vec<ModelCommand>,
    },
}

impl Command for ModelCommand {
    fn name(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::Set { .. } => "set",
            Self::Delete { .. } => "delete",
            Self::Relate { .. } => "relate",
            Self::AddParticipant { .. } => "add_participant",
            Self::RemoveParticipant { .. } => "remove_participant",
            Self::SetProperty { .. } => "set_property",
            Self::AttachPropertySet { .. } => "attach_property_set",
            Self::DetachPropertySet { .. } => "detach_property_set",
            Self::CreateComponent { .. } => "create_component",
            Self::RemoveComponent { .. } => "remove_component",
            Self::CreateType { .. } => "create_type",
            Self::AssignType { .. } => "assign_type",
            Self::Batch { .. } => "batch",
        }
    }

    fn apply(&self, transaction: &mut Transaction<'_>) -> Result<(), CommandError> {
        let applied = match self {
            Self::Create { id, entity, values } => {
                transaction.insert(*id, entity, borrowed(values))
            }
            Self::Set {
                id,
                attribute,
                value,
            } => transaction.set(*id, attribute, value.clone()).map(drop),
            Self::Delete { id } => transaction.remove(*id).map(drop),
            Self::Relate {
                id,
                entity,
                roles,
                values,
            } => transaction.relate(*id, entity, roles.clone(), borrowed(values)),
            Self::AddParticipant {
                relationship,
                role,
                participant,
            } => transaction
                .add_participant(*relationship, role, *participant)
                .map(drop),
            Self::RemoveParticipant {
                relationship,
                role,
                participant,
            } => transaction
                .remove_participant(*relationship, role, *participant)
                .map(drop),
            Self::SetProperty {
                id,
                set,
                property,
                value,
            } => transaction
                .set_property(*id, set, property, value.clone())
                .map(drop),
            Self::AttachPropertySet { id, set, values } => {
                transaction.attach_property_set(*id, set, borrowed(values))
            }
            Self::DetachPropertySet { id, set } => {
                transaction.detach_property_set(*id, set).map(drop)
            }
            Self::CreateComponent { id, component } => {
                transaction.create_component(*id, component.clone())
            }
            Self::RemoveComponent { id } => transaction.remove_component(*id).map(drop),
            Self::CreateType {
                id,
                component,
                values,
            } => transaction.create_type(*id, *component, borrowed(values)),
            Self::AssignType { instance, type_id } => {
                transaction.assign_type(*instance, *type_id).map(drop)
            }
            Self::Batch { commands } => {
                return commands
                    .iter()
                    .try_for_each(|command| command.apply(transaction));
            }
        };
        Ok(applied?)
    }
}

/// A command of a log that [`replay`] could not apply.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
#[error("command {index} of the log failed")]
pub struct ReplayError {
    /// Zero-based position of the command in the log.
    pub index: usize,
    /// Why it failed.
    #[source]
    pub source: CommandError,
}

/// Applies a recorded log of commands to `model`, in order, each as one transaction. Replaying
/// the log of every command applied to a model, on an empty model of the same schema, rebuilds
/// that model.
///
/// # Errors
///
/// Stops at the first command that fails and returns its position. The commands before it stay
/// applied; the failing one changes nothing.
pub fn replay<'a, C: Command + 'a>(
    model: &mut Model,
    log: impl IntoIterator<Item = &'a C>,
) -> Result<(), ReplayError> {
    log.into_iter()
        .enumerate()
        .try_for_each(|(index, command)| {
            Transaction::run(model, command)
                .map(drop)
                .map_err(|source| ReplayError { index, source })
        })
}

#[cfg(test)]
mod tests;
