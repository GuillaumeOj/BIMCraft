use crate::{EntityId, StoreError};

/// Why a [`Model`](super::Model) operation failed. Nothing was changed.
///
/// The messages name entities, attributes and roles from the schema but never repeat values or
/// ids, which come from a user's model (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ModelError {
    /// The attribute tables rejected the operation.
    #[error(transparent)]
    Store(#[from] StoreError),
    /// Relationship objects are created with [`Model::relate`](super::Model::relate), which
    /// takes their roles.
    #[error("entity \"{0}\" is a relationship and needs its roles")]
    RelationshipEntity(String),
    /// The entity, or the entity of the object, is not a relationship.
    #[error("entity \"{0}\" is not a relationship")]
    NotARelationship(String),
    /// The relationship entity has no such role.
    #[error("relationship \"{entity}\" has no role \"{role}\"")]
    UnknownRole {
        /// The relationship entity.
        entity: String,
        /// The unknown role.
        role: String,
    },
    /// The same role is given twice.
    #[error("role \"{0}\" is given more than once")]
    RepeatedRole(String),
    /// A role has no participant.
    #[error("role \"{0}\" needs at least one participant")]
    MissingRole(String),
    /// A `one` role is given several participants, or a second one.
    #[error("role \"{0}\" takes exactly one participant")]
    OneParticipant(String),
    /// Removing the participant would leave its role empty; remove the relationship instead.
    #[error("role \"{0}\" cannot be left without participants")]
    EmptyRole(String),
    /// A participant is not stored.
    #[error("a participant does not exist")]
    ParticipantNotFound(EntityId),
    /// An object appears more than once in one relationship.
    #[error("an object takes part in the relationship more than once")]
    RepeatedParticipant(EntityId),
    /// The role does not accept objects of this entity.
    #[error("role \"{role}\" does not accept entity \"{entity}\"")]
    TargetNotAllowed {
        /// The role.
        role: String,
        /// The participant's entity.
        entity: String,
    },
    /// The object already plays this exclusive role in another relationship of the entity.
    #[error("the object already plays role \"{role}\" in another \"{relationship}\"")]
    ExclusiveRole {
        /// The relationship entity.
        relationship: String,
        /// The exclusive role.
        role: String,
    },
    /// The object does not play this role in the relationship.
    #[error("the object does not play role \"{0}\" in this relationship")]
    NotParticipant(String),
}
