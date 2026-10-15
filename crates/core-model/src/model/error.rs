use schema::ValidationError;

use crate::units::Dimension;
use crate::{EntityId, StoreError};

/// Why a [`Model`](super::Model) operation failed. Nothing was changed.
///
/// The messages name entities, attributes, roles and property sets from the schema but never
/// repeat values or ids, which come from a user's model (SEC-03).
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
    /// The schema has no property set of this name, and none was defined.
    #[error("unknown property set \"{0}\"")]
    UnknownPropertySet(String),
    /// A property set of this name already exists.
    #[error("property set \"{0}\" already exists")]
    PropertySetExists(String),
    /// A custom property set does not pass [`schema::validate`].
    #[error("the property set does not pass validation")]
    InvalidPropertySet(Vec<ValidationError>),
    /// The property set does not apply to the object's entity.
    #[error("property set \"{set}\" does not apply to entity \"{entity}\"")]
    NotApplicable {
        /// The property set.
        set: String,
        /// The object's entity.
        entity: String,
    },
    /// The property set has no such property.
    #[error("property set \"{set}\" has no property \"{property}\"")]
    UnknownProperty {
        /// The property set.
        set: String,
        /// The unknown property.
        property: String,
    },
    /// The object already carries the property set.
    #[error("the object already carries property set \"{0}\"")]
    AlreadyAttached(String),
    /// The object does not carry the property set.
    #[error("the object does not carry property set \"{0}\"")]
    NotAttached(String),
    /// A property value was rejected.
    #[error("property set \"{set}\": {error}")]
    PropertyValue {
        /// The property set.
        set: String,
        /// Why the value was rejected.
        error: StoreError,
    },
    /// The value is set on the type and instances cannot override it (CORE-02).
    #[error("\"{name}\" of \"{owner}\" is set on the type and cannot be overridden")]
    NotOverridable {
        /// The entity or property set.
        owner: String,
        /// The attribute or property.
        name: String,
    },
    /// The property does not hold quantities of the requested dimension (CORE-09).
    #[error("property \"{property}\" does not hold quantities of {dimension}")]
    QuantityMismatch {
        /// The property.
        property: String,
        /// The requested dimension.
        dimension: Dimension,
    },
    /// The entity has no type (CORE-02).
    #[error("entity \"{0}\" has no type")]
    NoType(String),
    /// The object is a type object where an instance is expected.
    #[error("the object is a type, not an instance")]
    NotAnInstance,
    /// The object is not a type of the instance's entity.
    #[error("a \"{type_entity}\" cannot be the type of a \"{entity}\"")]
    WrongType {
        /// The instance's entity.
        entity: String,
        /// The entity of the object given as type.
        type_entity: String,
    },
    /// The value is set on each instance and type objects do not hold it (CORE-02).
    #[error("\"{name}\" of \"{owner}\" is set on each instance, not on the type")]
    NotOnType {
        /// The entity or property set.
        owner: String,
        /// The attribute or property.
        name: String,
    },
    /// No component has this id.
    #[error("no component with this id")]
    ComponentNotFound(EntityId),
    /// Types made from the component still exist.
    #[error("the component still has types")]
    ComponentInUse,
}
