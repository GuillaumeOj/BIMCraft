//! First-class relationships between objects (CORE-03, ADR 0012).

use std::collections::{BTreeSet, HashMap};

use schema::{Cardinality, Entity, EntityKind, Namespace, RoleTarget};

use super::{Model, ModelError};
use crate::{AttributeValue, EntityId, StoreError};

/// The participants of every relationship object, indexed both ways.
#[derive(Clone, Debug, Default)]
pub(super) struct Relationships {
    /// Participants of each relationship object, one list per role in schema order.
    roles: HashMap<EntityId, Vec<Vec<EntityId>>>,
    /// The roles each object plays, sorted so that queries list them in a stable order.
    links: HashMap<EntityId, BTreeSet<Link>>,
}

/// One role of one relationship object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Link {
    pub(super) relationship: EntityId,
    /// Index of the role in the relationship entity's roles.
    pub(super) role: usize,
}

/// A role an object plays in a relationship, as listed by [`Model::relationships_of`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Participation<'a> {
    /// The relationship object.
    pub relationship: EntityId,
    /// The relationship's entity, such as `Containment`.
    pub entity: &'a str,
    /// The role the object plays, such as `elements`.
    pub role: &'a str,
}

impl Relationships {
    /// Records a new relationship object and its participants.
    pub(super) fn insert(&mut self, relationship: EntityId, roles: Vec<Vec<EntityId>>) {
        for (role, participants) in roles.iter().enumerate() {
            for &participant in participants {
                self.links
                    .entry(participant)
                    .or_default()
                    .insert(Link { relationship, role });
            }
        }
        self.roles.insert(relationship, roles);
    }

    /// Forgets a relationship object and returns its participants, if it is one.
    pub(super) fn take(&mut self, relationship: EntityId) -> Option<Vec<Vec<EntityId>>> {
        let roles = self.roles.remove(&relationship)?;
        for (role, participants) in roles.iter().enumerate() {
            for &participant in participants {
                self.unlink(participant, Link { relationship, role });
            }
        }
        Some(roles)
    }

    /// The participants in one role; empty for an unknown relationship or role.
    pub(super) fn role(&self, relationship: EntityId, role: usize) -> &[EntityId] {
        self.roles
            .get(&relationship)
            .and_then(|roles| roles.get(role))
            .map_or(&[], Vec::as_slice)
    }

    /// Every participant of a relationship, all roles together.
    pub(super) fn participants(&self, relationship: EntityId) -> impl Iterator<Item = EntityId> {
        self.roles
            .get(&relationship)
            .into_iter()
            .flatten()
            .flatten()
            .copied()
    }

    /// The roles an object plays.
    pub(super) fn links(&self, object: EntityId) -> impl Iterator<Item = Link> {
        self.links.get(&object).into_iter().flatten().copied()
    }

    /// Adds a participant to a role at `position`, or at the end when the role is shorter.
    pub(super) fn attach(&mut self, link: Link, participant: EntityId, position: usize) {
        if let Some(list) = self
            .roles
            .get_mut(&link.relationship)
            .and_then(|roles| roles.get_mut(link.role))
        {
            list.insert(position.min(list.len()), participant);
            self.links.entry(participant).or_default().insert(link);
        }
    }

    /// Removes a participant from a role and returns where it was.
    pub(super) fn detach(&mut self, link: Link, participant: EntityId) -> Option<usize> {
        let list = self.roles.get_mut(&link.relationship)?.get_mut(link.role)?;
        let position = list.iter().position(|&other| other == participant)?;
        list.remove(position);
        self.unlink(participant, link);
        Some(position)
    }

    fn unlink(&mut self, participant: EntityId, link: Link) {
        let emptied = self.links.get_mut(&participant).is_some_and(|links| {
            links.remove(&link);
            links.is_empty()
        });
        if emptied {
            self.links.remove(&participant);
        }
    }

    /// Whether both directions of the index describe the same links.
    #[cfg(test)]
    pub(super) fn is_consistent(&self) -> bool {
        let forward: BTreeSet<(EntityId, Link)> = self
            .roles
            .iter()
            .flat_map(|(&relationship, roles)| {
                roles
                    .iter()
                    .enumerate()
                    .flat_map(move |(role, participants)| {
                        participants
                            .iter()
                            .map(move |&participant| (participant, Link { relationship, role }))
                    })
            })
            .collect();
        let backward: BTreeSet<(EntityId, Link)> = self
            .links
            .iter()
            .flat_map(|(&participant, links)| links.iter().map(move |&link| (participant, link)))
            .collect();
        forward == backward && self.links.values().all(|links| !links.is_empty())
    }
}

impl Model {
    /// Stores a new relationship object of `entity` relating `roles`, each a role name and its
    /// participants, with the given attribute values.
    ///
    /// # Errors
    ///
    /// Fails, storing nothing, when the entity is not a relationship, a role is unknown, given
    /// twice or missing, a `one` role has several participants, a participant does not exist,
    /// appears twice or is not an allowed target, an exclusive role is already played in another
    /// relationship of the entity, or the attribute tables reject the object.
    pub fn relate<R: AsRef<str>, N: AsRef<str>>(
        &mut self,
        id: EntityId,
        entity: &str,
        roles: impl IntoIterator<Item = (R, Vec<EntityId>)>,
        values: impl IntoIterator<Item = (N, AttributeValue)>,
    ) -> Result<(), ModelError> {
        let definition = relationship_entity(&self.namespace, entity)?;
        if self.store.contains(id) {
            return Err(StoreError::DuplicateId(id).into());
        }
        let mut given = vec![None; definition.roles.len()];
        for (name, participants) in roles {
            let role = role_index(definition, name.as_ref())?;
            if given[role].replace(participants).is_some() {
                return Err(ModelError::RepeatedRole(name.as_ref().to_owned()));
            }
        }
        let mut seen = BTreeSet::new();
        let mut participants = Vec::with_capacity(given.len());
        for (role, (list, role_definition)) in given.into_iter().zip(&definition.roles).enumerate()
        {
            let list: Vec<EntityId> = list.unwrap_or_default();
            if list.is_empty() {
                return Err(ModelError::MissingRole(role_definition.name.clone()));
            }
            if role_definition.cardinality == Cardinality::One && list.len() > 1 {
                return Err(ModelError::OneParticipant(role_definition.name.clone()));
            }
            for &participant in &list {
                if !seen.insert(participant) {
                    return Err(ModelError::RepeatedParticipant(participant));
                }
                self.check_participant(definition, role, participant)?;
            }
            participants.push(list);
        }
        self.store.insert(id, entity, values)?;
        self.relationships.insert(id, participants);
        Ok(())
    }

    /// The participants of the relationship object `relationship` in `role`, in the order they
    /// were added.
    ///
    /// # Errors
    ///
    /// Fails when no object has this id, it is not a relationship, or its entity has no such
    /// role.
    pub fn participants(
        &self,
        relationship: EntityId,
        role: &str,
    ) -> Result<&[EntityId], ModelError> {
        let (_, index) = self.locate_role(relationship, role)?;
        Ok(self.relationships.role(relationship, index))
    }

    /// Every role the object with this id plays, sorted by relationship id then role; nothing
    /// for an unknown id.
    pub fn relationships_of(&self, id: EntityId) -> impl Iterator<Item = Participation<'_>> {
        self.relationships.links(id).filter_map(move |link| {
            let entity = self.store.entity_of(link.relationship)?;
            let role = self.namespace.entities.get(entity)?.roles.get(link.role)?;
            Some(Participation {
                relationship: link.relationship,
                entity,
                role: &role.name,
            })
        })
    }

    /// Adds `participant` at the end of a `many` role of an existing relationship object.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the relationship or role is unknown, the role is `one`,
    /// the participant does not exist, already takes part in the relationship, is not an
    /// allowed target, or already plays this exclusive role elsewhere.
    pub fn add_participant(
        &mut self,
        relationship: EntityId,
        role: &str,
        participant: EntityId,
    ) -> Result<(), ModelError> {
        let (definition, index) = self.locate_role(relationship, role)?;
        if definition.roles[index].cardinality == Cardinality::One {
            return Err(ModelError::OneParticipant(role.to_owned()));
        }
        if self
            .relationships
            .participants(relationship)
            .any(|other| other == participant)
        {
            return Err(ModelError::RepeatedParticipant(participant));
        }
        self.check_participant(definition, index, participant)?;
        let link = Link {
            relationship,
            role: index,
        };
        self.relationships.attach(link, participant, usize::MAX);
        Ok(())
    }

    /// Removes `participant` from a role of a relationship object; the participant itself stays.
    ///
    /// # Errors
    ///
    /// Fails, changing nothing, when the relationship or role is unknown, the object does not
    /// play the role there, or it is the role's last participant: remove the relationship
    /// instead.
    pub fn remove_participant(
        &mut self,
        relationship: EntityId,
        role: &str,
        participant: EntityId,
    ) -> Result<(), ModelError> {
        let (_, index) = self.locate_role(relationship, role)?;
        let list = self.relationships.role(relationship, index);
        if !list.contains(&participant) {
            return Err(ModelError::NotParticipant(role.to_owned()));
        }
        if list.len() == 1 {
            return Err(ModelError::EmptyRole(role.to_owned()));
        }
        let link = Link {
            relationship,
            role: index,
        };
        self.relationships.detach(link, participant);
        Ok(())
    }

    /// The entity definition of a stored relationship object and the index of one of its roles.
    fn locate_role(
        &self,
        relationship: EntityId,
        role: &str,
    ) -> Result<(&Entity, usize), ModelError> {
        let entity = self
            .store
            .entity_of(relationship)
            .ok_or(StoreError::NotFound(relationship))?;
        let definition = relationship_entity(&self.namespace, entity)?;
        Ok((definition, role_index(definition, role)?))
    }

    /// Checks that `participant` exists and may play role `role` of `relationship`.
    fn check_participant(
        &self,
        relationship: &Entity,
        role: usize,
        participant: EntityId,
    ) -> Result<(), ModelError> {
        let entity = self
            .store
            .entity_of(participant)
            .ok_or(ModelError::ParticipantNotFound(participant))?;
        let definition = &relationship.roles[role];
        let kind = self
            .namespace
            .entities
            .get(entity)
            .map(|entity| entity.kind);
        if !definition
            .targets
            .iter()
            .any(|target| accepts(target, entity, kind))
        {
            return Err(ModelError::TargetNotAllowed {
                role: definition.name.clone(),
                entity: entity.to_owned(),
            });
        }
        let taken = definition.exclusive
            && self.relationships.links(participant).any(|link| {
                link.role == role
                    && self.store.entity_of(link.relationship) == Some(relationship.name.as_str())
            });
        if taken {
            return Err(ModelError::ExclusiveRole {
                relationship: relationship.name.clone(),
                role: definition.name.clone(),
            });
        }
        Ok(())
    }
}

/// The definition of `entity`, which must be a relationship.
fn relationship_entity<'a>(
    namespace: &'a Namespace,
    entity: &str,
) -> Result<&'a Entity, ModelError> {
    let definition = namespace
        .entities
        .get(entity)
        .ok_or_else(|| StoreError::UnknownEntity(entity.to_owned()))?;
    if definition.kind == EntityKind::Relationship {
        Ok(definition)
    } else {
        Err(ModelError::NotARelationship(entity.to_owned()))
    }
}

fn role_index(relationship: &Entity, role: &str) -> Result<usize, ModelError> {
    relationship
        .roles
        .iter()
        .position(|definition| definition.name == role)
        .ok_or_else(|| ModelError::UnknownRole {
            entity: relationship.name.clone(),
            role: role.to_owned(),
        })
}

/// Whether an object of `entity`, of kind `kind`, matches `target`.
fn accepts(target: &RoleTarget, entity: &str, kind: Option<EntityKind>) -> bool {
    match target {
        RoleTarget::Entity(name) => name == entity,
        RoleTarget::Kind(target) => kind == Some(*target),
    }
}
