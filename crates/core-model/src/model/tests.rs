use std::collections::BTreeMap;

use proptest::prelude::*;
use schema::{
    Attribute, Cardinality, Entity, IfcMapping, Placement, Presence, PropertySet, QuantityKind,
    Role, RoleTarget, TypeDef, Value, ValueType,
};

use super::*;

pub(super) fn entity(name: &str, kind: EntityKind, ifc: &str) -> Entity {
    Entity {
        name: name.into(),
        kind,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: ifc.into(),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes: vec![Attribute {
            name: "name".into(),
            value: ValueType::Text,
            presence: Presence::Optional,
            placement: Placement::Instance,
            doc: None,
        }],
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

fn role(name: &str, targets: &[RoleTarget], cardinality: Cardinality, exclusive: bool) -> Role {
    Role {
        name: name.into(),
        targets: targets.to_vec(),
        cardinality,
        exclusive,
    }
}

fn relationship(name: &str, ifc: &str, roles: Vec<Role>) -> Entity {
    Entity {
        roles,
        ..entity(name, EntityKind::Relationship, ifc)
    }
}

/// Walls, openings, groups and storeys, with containment, voiding and group assignment, and a
/// property set for walls.
pub(super) fn namespace() -> Namespace {
    use Cardinality::{Many, One};
    let named = |name: &str| RoleTarget::Entity(name.into());
    let entities = [
        Entity {
            type_def: Some(TypeDef {
                name: "WallType".into(),
                doc: None,
            }),
            property_sets: vec!["Pset_WallCommon".into()],
            ..wall()
        },
        entity("Opening", EntityKind::Element, "IfcOpeningElement"),
        entity("Group", EntityKind::Element, "IfcGroup"),
        entity("Storey", EntityKind::Spatial, "IfcBuildingStorey"),
        relationship(
            "Containment",
            "IfcRelContainedInSpatialStructure",
            vec![
                role(
                    "structure",
                    &[RoleTarget::Kind(EntityKind::Spatial)],
                    One,
                    false,
                ),
                role(
                    "elements",
                    &[RoleTarget::Kind(EntityKind::Element)],
                    Many,
                    true,
                ),
            ],
        ),
        relationship(
            "Voiding",
            "IfcRelVoidsElement",
            vec![
                role("host", &[named("Wall")], One, false),
                role("opening", &[named("Opening")], One, true),
            ],
        ),
        relationship(
            "Assignment",
            "IfcRelAssignsToGroup",
            vec![
                role("group", &[named("Group")], One, false),
                role(
                    "objects",
                    &[
                        RoleTarget::Kind(EntityKind::Element),
                        RoleTarget::Kind(EntityKind::Relationship),
                    ],
                    Many,
                    false,
                ),
            ],
        ),
    ];
    Namespace {
        name: "core".into(),
        version: 1,
        entities: entities
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::from([("Pset_WallCommon".into(), wall_common())]),
    }
}

/// A wall with an attribute of each placement: `name` on instances, `thickness` on the type and
/// `height` on the type with overrides.
fn wall() -> Entity {
    let mut wall = entity("Wall", EntityKind::Element, "IfcWall");
    let length = || ValueType::Quantity(QuantityKind::Length);
    wall.attributes.extend([
        property("thickness", length(), Presence::Required, Placement::Type),
        property(
            "height",
            length(),
            Presence::Default(Value::Real(2.5)),
            Placement::TypeOverridable,
        ),
    ]);
    wall
}

fn property(name: &str, value: ValueType, presence: Presence, placement: Placement) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence,
        placement,
        doc: None,
    }
}

/// A property set for walls with one property of each rule and placement.
pub(super) fn wall_common() -> PropertySet {
    use Placement::{Instance, Type, TypeOverridable};
    PropertySet {
        name: "Pset_WallCommon".into(),
        applies_to: vec!["Wall".into()],
        properties: vec![
            property("Reference", ValueType::Text, Presence::Required, Instance),
            property(
                "IsExternal",
                ValueType::Bool,
                Presence::Default(Value::Bool(true)),
                TypeOverridable,
            ),
            property(
                "LoadBearing",
                ValueType::Bool,
                Presence::Default(Value::Bool(false)),
                Instance,
            ),
            property("FireRating", ValueType::Text, Presence::Optional, Type),
            property(
                "Width",
                ValueType::Quantity(QuantityKind::Length),
                Presence::Default(Value::Real(0.2)),
                Instance,
            ),
        ],
    }
}

pub(super) fn real(value: f64) -> AttributeValue {
    AttributeValue::Real(value)
}

pub(super) fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

pub(super) fn bool(value: bool) -> AttributeValue {
    AttributeValue::Bool(value)
}

pub(super) fn id(raw: u128) -> EntityId {
    EntityId::new(raw)
}

fn ids<const N: usize>(raws: [u128; N]) -> Vec<EntityId> {
    raws.into_iter().map(id).collect()
}

pub(super) const STOREY: u128 = 1;
const OTHER_STOREY: u128 = 2;
pub(super) const WALLS: [u128; 3] = [11, 12, 13];
const OPENING: u128 = 21;
const GROUP: u128 = 31;
pub(super) const CONTAINMENT: u128 = 100;

/// Two storeys, three walls, an opening and a group, no relationship.
pub(super) fn model() -> Model {
    let mut model = Model::new(&namespace()).unwrap();
    let objects = [
        (STOREY, "Storey"),
        (OTHER_STOREY, "Storey"),
        (WALLS[0], "Wall"),
        (WALLS[1], "Wall"),
        (WALLS[2], "Wall"),
        (OPENING, "Opening"),
        (GROUP, "Group"),
    ];
    for (raw, entity) in objects {
        model.insert(id(raw), entity, NO_VALUES).unwrap();
    }
    model
}

pub(super) const NO_VALUES: [(&str, AttributeValue); 0] = [];

/// The model with the first two walls contained in the first storey.
fn contained() -> Model {
    let mut model = model();
    contain(&mut model, CONTAINMENT, STOREY, &WALLS[..2]).unwrap();
    model
}

fn contain(model: &mut Model, raw: u128, storey: u128, walls: &[u128]) -> Result<(), ModelError> {
    let roles = [
        ("structure", vec![id(storey)]),
        ("elements", walls.iter().copied().map(id).collect()),
    ];
    model.relate(id(raw), "Containment", roles, NO_VALUES)
}

/// The roles an object plays, as (relationship, entity, role).
fn played(model: &Model, raw: u128) -> Vec<(u128, String, String)> {
    model
        .relationships_of(id(raw))
        .map(|participation| {
            (
                participation.relationship.get(),
                participation.entity.to_owned(),
                participation.role.to_owned(),
            )
        })
        .collect()
}

fn participants(model: &Model, raw: u128, role: &str) -> Vec<EntityId> {
    model.participants(id(raw), role).unwrap().to_vec()
}

#[test]
fn starts_empty_and_rejects_invalid_schemas() {
    let model = Model::new(&namespace()).unwrap();
    assert!(model.is_empty());
    assert_eq!(model.len(), 0);
    let mut invalid = namespace();
    invalid.version = 0;
    assert!(matches!(
        Model::new(&invalid),
        Err(ModelError::Store(StoreError::InvalidSchema(_)))
    ));
}

#[test]
fn stores_objects_through_the_attribute_tables() {
    let mut model = model();
    assert_eq!(model.len(), 7);
    assert!(!model.is_empty());
    assert!(model.contains(id(STOREY)));
    assert_eq!(model.entity_of(id(WALLS[0])), Some("Wall"));
    let name = AttributeValue::Text("W1".into());
    assert_eq!(
        model.set(id(WALLS[0]), "name", Some(name.clone())),
        Ok(None)
    );
    assert_eq!(model.get(id(WALLS[0]), "name"), Ok(Some(name)));
    assert_eq!(
        model.insert(id(STOREY), "Storey", NO_VALUES),
        Err(ModelError::Store(StoreError::DuplicateId(id(STOREY))))
    );
}

#[test]
fn relationship_entities_need_their_roles() {
    let mut model = model();
    assert_eq!(
        model.insert(id(CONTAINMENT), "Containment", NO_VALUES),
        Err(ModelError::RelationshipEntity("Containment".into()))
    );
    assert!(!model.contains(id(CONTAINMENT)));
}

#[test]
fn relationships_are_objects_queryable_both_ways() {
    let mut model = model();
    let name = AttributeValue::Text("Level 1".into());
    let roles = [
        ("elements", ids([WALLS[0], WALLS[1]])),
        ("structure", ids([STOREY])),
    ];
    model
        .relate(
            id(CONTAINMENT),
            "Containment",
            roles,
            [("name", name.clone())],
        )
        .unwrap();

    assert_eq!(model.entity_of(id(CONTAINMENT)), Some("Containment"));
    assert_eq!(model.get(id(CONTAINMENT), "name"), Ok(Some(name)));
    assert_eq!(
        participants(&model, CONTAINMENT, "structure"),
        ids([STOREY])
    );
    assert_eq!(
        participants(&model, CONTAINMENT, "elements"),
        ids([WALLS[0], WALLS[1]])
    );
    let containment = |role: &str| (CONTAINMENT, "Containment".to_owned(), role.to_owned());
    assert_eq!(played(&model, STOREY), [containment("structure")]);
    assert_eq!(played(&model, WALLS[1]), [containment("elements")]);
    assert_eq!(played(&model, WALLS[2]), []);
    assert_eq!(played(&model, 999), []);
}

#[test]
fn an_object_lists_every_role_it_plays_by_relationship_id() {
    let mut model = contained();
    let voiding = [("host", ids([WALLS[0]])), ("opening", ids([OPENING]))];
    model.relate(id(50), "Voiding", voiding, NO_VALUES).unwrap();
    let assignment = [("group", ids([GROUP])), ("objects", ids([WALLS[0]]))];
    model
        .relate(id(200), "Assignment", assignment, NO_VALUES)
        .unwrap();
    assert_eq!(
        played(&model, WALLS[0]),
        [
            (50, "Voiding".into(), "host".into()),
            (CONTAINMENT, "Containment".into(), "elements".into()),
            (200, "Assignment".into(), "objects".into()),
        ]
    );
}

/// A relationship to create, as entity and roles, and the error it must fail with.
type RelateCase = (&'static str, Vec<(&'static str, Vec<EntityId>)>, ModelError);

/// Relationships that [`contained`] must refuse, one per check.
fn relate_error_cases() -> Vec<RelateCase> {
    let wall = |index: usize| id(WALLS[index]);
    vec![
        ("Wall", vec![], ModelError::NotARelationship("Wall".into())),
        (
            "Roof",
            vec![],
            ModelError::Store(StoreError::UnknownEntity("Roof".into())),
        ),
        (
            "Containment",
            vec![("floor", ids([STOREY]))],
            ModelError::UnknownRole {
                entity: "Containment".into(),
                role: "floor".into(),
            },
        ),
        (
            "Containment",
            vec![("structure", ids([STOREY])), ("structure", ids([STOREY]))],
            ModelError::RepeatedRole("structure".into()),
        ),
        (
            "Containment",
            vec![("structure", ids([OTHER_STOREY]))],
            ModelError::MissingRole("elements".into()),
        ),
        (
            "Containment",
            vec![("structure", ids([OTHER_STOREY])), ("elements", vec![])],
            ModelError::MissingRole("elements".into()),
        ),
        (
            "Containment",
            vec![
                ("structure", ids([STOREY, OTHER_STOREY])),
                ("elements", vec![wall(2)]),
            ],
            ModelError::OneParticipant("structure".into()),
        ),
        (
            "Containment",
            vec![
                ("structure", ids([OTHER_STOREY])),
                ("elements", vec![wall(2), wall(2)]),
            ],
            ModelError::RepeatedParticipant(wall(2)),
        ),
        (
            "Containment",
            vec![("structure", ids([OTHER_STOREY])), ("elements", ids([999]))],
            ModelError::ParticipantNotFound(id(999)),
        ),
        (
            "Containment",
            vec![("structure", vec![wall(2)]), ("elements", vec![wall(2)])],
            ModelError::TargetNotAllowed {
                role: "structure".into(),
                entity: "Wall".into(),
            },
        ),
        (
            "Voiding",
            vec![("host", ids([OPENING])), ("opening", ids([OPENING]))],
            ModelError::TargetNotAllowed {
                role: "host".into(),
                entity: "Opening".into(),
            },
        ),
        (
            "Containment",
            vec![
                ("structure", ids([OTHER_STOREY])),
                ("elements", vec![wall(0)]),
            ],
            ModelError::ExclusiveRole {
                relationship: "Containment".into(),
                role: "elements".into(),
            },
        ),
    ]
}

#[test]
fn relate_errors_store_nothing() {
    let mut model = contained();
    let wall = |index: usize| id(WALLS[index]);
    let cases = relate_error_cases();
    for (entity, roles, error) in cases {
        assert_eq!(
            model.relate(id(101), entity, roles, NO_VALUES),
            Err(error),
            "{entity}"
        );
        assert!(!model.contains(id(101)));
        assert_eq!(played(&model, WALLS[2]), []);
        assert_eq!(played(&model, OTHER_STOREY), []);
    }
    assert_eq!(
        contain(&mut model, CONTAINMENT, OTHER_STOREY, &[WALLS[2]]),
        Err(ModelError::Store(StoreError::DuplicateId(id(CONTAINMENT))))
    );
    let roles = [
        ("structure", ids([OTHER_STOREY])),
        ("elements", vec![wall(2)]),
    ];
    let colour = [("colour", AttributeValue::Text("red".into()))];
    assert!(matches!(
        model.relate(id(101), "Containment", roles, colour),
        Err(ModelError::Store(StoreError::UnknownAttribute { .. }))
    ));
    assert_eq!(played(&model, WALLS[2]), []);
}

#[test]
fn exclusive_roles_only_count_relationships_of_the_same_entity() {
    let mut model = contained();
    // Already contained, the wall can still be assigned to a group, any number of times.
    for raw in [200, 201] {
        let roles = [("group", ids([GROUP])), ("objects", ids([WALLS[0]]))];
        model
            .relate(id(raw), "Assignment", roles, NO_VALUES)
            .unwrap();
    }
    // A relationship can itself be a participant.
    model
        .add_participant(id(200), "objects", id(CONTAINMENT))
        .unwrap();
    assert_eq!(
        participants(&model, 200, "objects"),
        ids([WALLS[0], CONTAINMENT])
    );
}

#[test]
fn participants_are_added_and_removed() {
    let mut model = contained();
    model
        .add_participant(id(CONTAINMENT), "elements", id(WALLS[2]))
        .unwrap();
    assert_eq!(participants(&model, CONTAINMENT, "elements"), ids(WALLS));
    let position = model
        .remove_participant(id(CONTAINMENT), "elements", id(WALLS[0]))
        .unwrap();
    assert_eq!(position, 0);
    assert_eq!(
        participants(&model, CONTAINMENT, "elements"),
        ids([WALLS[1], WALLS[2]])
    );
    // Inserting at the returned position undoes the removal exactly.
    model
        .insert_participant(id(CONTAINMENT), "elements", id(WALLS[0]), position)
        .unwrap();
    assert_eq!(participants(&model, CONTAINMENT, "elements"), ids(WALLS));
    model
        .remove_participant(id(CONTAINMENT), "elements", id(WALLS[0]))
        .unwrap();
    assert_eq!(played(&model, WALLS[0]), []);
    assert!(model.contains(id(WALLS[0])));
}

#[test]
fn participant_errors_change_nothing() {
    let mut model = contained();
    let add_cases = [
        (
            id(999),
            "elements",
            id(WALLS[2]),
            ModelError::Store(StoreError::NotFound(id(999))),
        ),
        (
            id(STOREY),
            "elements",
            id(WALLS[2]),
            ModelError::NotARelationship("Storey".into()),
        ),
        (
            id(CONTAINMENT),
            "floor",
            id(WALLS[2]),
            ModelError::UnknownRole {
                entity: "Containment".into(),
                role: "floor".into(),
            },
        ),
        (
            id(CONTAINMENT),
            "structure",
            id(OTHER_STOREY),
            ModelError::OneParticipant("structure".into()),
        ),
        (
            id(CONTAINMENT),
            "elements",
            id(WALLS[0]),
            ModelError::RepeatedParticipant(id(WALLS[0])),
        ),
        (
            id(CONTAINMENT),
            "elements",
            id(STOREY),
            ModelError::RepeatedParticipant(id(STOREY)),
        ),
        (
            id(CONTAINMENT),
            "elements",
            id(OTHER_STOREY),
            ModelError::TargetNotAllowed {
                role: "elements".into(),
                entity: "Storey".into(),
            },
        ),
        (
            id(CONTAINMENT),
            "elements",
            id(999),
            ModelError::ParticipantNotFound(id(999)),
        ),
    ];
    for (relationship, role, participant, error) in add_cases {
        assert_eq!(
            model.add_participant(relationship, role, participant),
            Err(error)
        );
    }
    let mut other = model.clone();
    contain(&mut other, 101, OTHER_STOREY, &[WALLS[2]]).unwrap();
    assert_eq!(
        other.add_participant(id(CONTAINMENT), "elements", id(WALLS[2])),
        Err(ModelError::ExclusiveRole {
            relationship: "Containment".into(),
            role: "elements".into(),
        })
    );

    assert_eq!(
        model.remove_participant(id(CONTAINMENT), "elements", id(WALLS[2])),
        Err(ModelError::NotParticipant("elements".into()))
    );
    assert_eq!(
        model.remove_participant(id(CONTAINMENT), "structure", id(STOREY)),
        Err(ModelError::EmptyRole("structure".into()))
    );
    assert_eq!(
        model.participants(id(CONTAINMENT), "floor"),
        Err(ModelError::UnknownRole {
            entity: "Containment".into(),
            role: "floor".into(),
        })
    );
    assert_eq!(
        participants(&model, CONTAINMENT, "structure"),
        ids([STOREY])
    );
    assert_eq!(
        participants(&model, CONTAINMENT, "elements"),
        ids([WALLS[0], WALLS[1]])
    );
}

#[test]
fn deleting_a_participant_keeps_a_relationship_that_still_has_its_roles() {
    let mut model = contained();
    let removal = model.remove(id(WALLS[0])).unwrap();
    assert_eq!(removal.ids().collect::<Vec<_>>(), ids([WALLS[0]]));
    assert!(model.contains(id(CONTAINMENT)));
    assert_eq!(
        participants(&model, CONTAINMENT, "elements"),
        ids([WALLS[1]])
    );
}

#[test]
fn deleting_the_last_participant_of_a_role_deletes_the_relationship_only() {
    let mut model = contained();
    let removal = model.remove(id(STOREY)).unwrap();
    assert_eq!(
        removal.ids().collect::<Vec<_>>(),
        ids([STOREY, CONTAINMENT])
    );
    assert!(!model.contains(id(CONTAINMENT)));
    for wall in WALLS {
        assert!(model.contains(id(wall)));
        assert_eq!(played(&model, wall), []);
    }
    assert_eq!(
        model.participants(id(CONTAINMENT), "elements"),
        Err(ModelError::Store(StoreError::NotFound(id(CONTAINMENT))))
    );
}

#[test]
fn deleting_a_relationship_deletes_nothing_else() {
    let mut model = contained();
    let removal = model.remove(id(CONTAINMENT)).unwrap();
    assert_eq!(removal.ids().collect::<Vec<_>>(), ids([CONTAINMENT]));
    assert_eq!(model.len(), 7);
    assert_eq!(played(&model, STOREY), []);
    assert_eq!(played(&model, WALLS[0]), []);
    assert_eq!(
        model.remove(id(CONTAINMENT)),
        Err(ModelError::Store(StoreError::NotFound(id(CONTAINMENT))))
    );
}

#[test]
fn deletion_cascades_through_relationships_of_relationships() {
    let mut model = contained();
    let roles = [("group", ids([GROUP])), ("objects", ids([CONTAINMENT]))];
    model
        .relate(id(200), "Assignment", roles, NO_VALUES)
        .unwrap();
    let removal = model.remove(id(STOREY)).unwrap();
    assert_eq!(
        removal.ids().collect::<Vec<_>>(),
        ids([STOREY, CONTAINMENT, 200])
    );
    assert!(model.contains(id(GROUP)));
    assert_eq!(played(&model, GROUP), []);
}

#[test]
fn a_relationship_emptied_twice_is_deleted_once() {
    let mut model = model();
    // The group is the only element of containment 101, and the only group of assignment 100,
    // whose only object is containment 101. Removing the group empties containment 101, which
    // empties assignment 100 a second time.
    contain(&mut model, 101, STOREY, &[GROUP]).unwrap();
    let roles = [("group", ids([GROUP])), ("objects", ids([101]))];
    model
        .relate(id(100), "Assignment", roles, NO_VALUES)
        .unwrap();
    let before = model.clone();
    let removal = model.remove(id(GROUP)).unwrap();
    assert_eq!(removal.ids().collect::<Vec<_>>(), ids([GROUP, 101, 100]));
    assert_eq!(model.len(), 6);
    model.restore(removal).unwrap();
    assert_same(&model, &before);
}

#[test]
fn restore_puts_everything_back_in_place() {
    let mut model = model();
    contain(&mut model, CONTAINMENT, STOREY, &WALLS).unwrap();
    let voiding = [("host", ids([WALLS[1]])), ("opening", ids([OPENING]))];
    model.relate(id(50), "Voiding", voiding, NO_VALUES).unwrap();
    let before = model.clone();

    // The middle wall leaves the middle of the containment and empties the voiding.
    let removal = model.remove(id(WALLS[1])).unwrap();
    assert_eq!(removal.ids().collect::<Vec<_>>(), ids([WALLS[1], 50]));
    assert_eq!(
        participants(&model, CONTAINMENT, "elements"),
        ids([WALLS[0], WALLS[2]])
    );
    model.restore(removal).unwrap();
    assert_same(&model, &before);
}

#[test]
fn restore_fails_without_change_when_the_model_moved_on() {
    let mut model = contained();
    let removal = model.remove(id(STOREY)).unwrap();
    model.insert(id(STOREY), "Storey", NO_VALUES).unwrap();
    assert_eq!(
        model.restore(removal.clone()),
        Err(ModelError::Store(StoreError::DuplicateId(id(STOREY))))
    );
    model.remove(id(STOREY)).unwrap();
    model.remove(id(WALLS[0])).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::ParticipantNotFound(id(WALLS[0])))
    );
    assert!(!model.contains(id(STOREY)));
    assert!(!model.contains(id(CONTAINMENT)));

    let mut model = contained();
    let removal = model.remove(id(WALLS[0])).unwrap();
    model.remove(id(CONTAINMENT)).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::Store(StoreError::NotFound(id(CONTAINMENT))))
    );
    assert!(!model.contains(id(WALLS[0])));
}

#[test]
fn restore_keeps_exclusive_roles_exclusive() {
    let mut model = contained();
    let removal = model.remove(id(STOREY)).unwrap();
    // The walls the containment held move to the other storey meanwhile.
    contain(&mut model, 101, OTHER_STOREY, &WALLS[..2]).unwrap();
    assert_eq!(
        model.restore(removal),
        Err(ModelError::ExclusiveRole {
            relationship: "Containment".into(),
            role: "elements".into(),
        })
    );
    assert!(!model.contains(id(STOREY)));
    assert!(!model.contains(id(CONTAINMENT)));
    assert_eq!(
        participants(&model, 101, "elements"),
        ids([WALLS[0], WALLS[1]])
    );
}

#[test]
fn errors_name_entities_and_roles_but_never_ids() {
    let id = id(0xABCD);
    let messages = [
        (
            ModelError::Store(StoreError::NotFound(id)),
            "no object with this id",
        ),
        (
            ModelError::RelationshipEntity("Containment".into()),
            "entity \"Containment\" is a relationship and needs its roles",
        ),
        (
            ModelError::NotARelationship("Wall".into()),
            "entity \"Wall\" is not a relationship",
        ),
        (
            ModelError::UnknownRole {
                entity: "Containment".into(),
                role: "floor".into(),
            },
            "relationship \"Containment\" has no role \"floor\"",
        ),
        (
            ModelError::RepeatedRole("host".into()),
            "role \"host\" is given more than once",
        ),
        (
            ModelError::MissingRole("host".into()),
            "role \"host\" needs at least one participant",
        ),
        (
            ModelError::OneParticipant("host".into()),
            "role \"host\" takes exactly one participant",
        ),
        (
            ModelError::EmptyRole("host".into()),
            "role \"host\" cannot be left without participants",
        ),
        (
            ModelError::ParticipantNotFound(id),
            "a participant does not exist",
        ),
        (
            ModelError::RepeatedParticipant(id),
            "an object takes part in the relationship more than once",
        ),
        (
            ModelError::TargetNotAllowed {
                role: "host".into(),
                entity: "Opening".into(),
            },
            "role \"host\" does not accept entity \"Opening\"",
        ),
        (
            ModelError::ExclusiveRole {
                relationship: "Containment".into(),
                role: "elements".into(),
            },
            "the object already plays role \"elements\" in another \"Containment\"",
        ),
        (
            ModelError::NotParticipant("elements".into()),
            "the object does not play role \"elements\" in this relationship",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}

/// Every object id the tests and the property test use.
const ALL_IDS: std::ops::Range<u128> = 0..256;

/// Asserts that two models hold the same objects, values and relationships, and that the
/// relationship index is consistent.
fn assert_same(model: &Model, expected: &Model) {
    assert!(model.relationships.is_consistent());
    assert_eq!(model, expected);
}

/// One step of a random sequence of model operations on two storeys, six walls and up to four
/// containments.
#[derive(Clone, Debug)]
enum Op {
    Contain { raw: u8, storey: u8, walls: Vec<u8> },
    Add { raw: u8, wall: u8 },
    Leave { raw: u8, wall: u8 },
    Remove { object: u8 },
}

fn op() -> impl Strategy<Value = Op> {
    let containment = 0..4u8;
    let wall = 0..6u8;
    prop_oneof![
        (
            containment.clone(),
            0..2u8,
            proptest::collection::vec(wall.clone(), 0..4)
        )
            .prop_map(|(raw, storey, walls)| Op::Contain { raw, storey, walls }),
        (containment.clone(), wall.clone()).prop_map(|(raw, wall)| Op::Add { raw, wall }),
        (containment, wall).prop_map(|(raw, wall)| Op::Leave { raw, wall }),
        (0..12u8).prop_map(|object| Op::Remove { object }),
    ]
}

/// Ids: storeys 1–2, walls 11–16, containments 100–103.
fn random_model() -> Model {
    let mut model = Model::new(&namespace()).unwrap();
    for raw in [1, 2] {
        model.insert(id(raw), "Storey", NO_VALUES).unwrap();
    }
    for raw in 11..=16 {
        model.insert(id(raw), "Wall", NO_VALUES).unwrap();
    }
    model
}

/// Maps a generated index onto the ids of [`random_model`] and containments.
fn object(index: u8) -> u128 {
    match index {
        0..2 => u128::from(index) + 1,
        2..8 => u128::from(index) + 9,
        _ => u128::from(index) + 92,
    }
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    /// Whatever the operations, both directions of the index agree, every participant exists,
    /// and every removal is undone exactly by its restore.
    #[test]
    fn relationships_stay_consistent(ops in proptest::collection::vec(op(), 0..48)) {
        let mut model = random_model();
        for op in ops {
            let wall = |index: u8| id(u128::from(index) + 11);
            let containment = |raw: u8| id(u128::from(raw) + 100);
            let _ = match op {
                Op::Contain { raw, storey, walls } => {
                    let walls: Vec<u128> = walls.iter().map(|&index| u128::from(index) + 11).collect();
                    contain(&mut model, containment(raw).get(), u128::from(storey) + 1, &walls)
                }
                Op::Add { raw, wall: index } => {
                    model.add_participant(containment(raw), "elements", wall(index)).map(drop)
                }
                Op::Leave { raw, wall: index } => {
                    model.remove_participant(containment(raw), "elements", wall(index)).map(drop)
                }
                Op::Remove { object: index } => {
                    let before = model.clone();
                    let removed = model.remove(id(object(index)));
                    if let Ok(removal) = removed {
                        let after = model.clone();
                        model.restore(removal.clone()).unwrap();
                        assert_same(&model, &before);
                        model = after;
                        prop_assert!(removal.ids().all(|removed| !model.contains(removed)));
                    }
                    Ok(())
                }
            };
            prop_assert!(model.relationships.is_consistent());
            for raw in ALL_IDS.filter(|&raw| model.contains(id(raw))) {
                for participation in model.relationships_of(id(raw)) {
                    prop_assert!(model.contains(participation.relationship));
                    let listed = model.participants(participation.relationship, participation.role).unwrap();
                    prop_assert!(listed.contains(&id(raw)));
                    prop_assert!(!listed.is_empty());
                }
                if model.entity_of(id(raw)) == Some("Containment") {
                    for role in ["structure", "elements"] {
                        let listed = model.participants(id(raw), role).unwrap();
                        prop_assert!(!listed.is_empty());
                        prop_assert!(listed.iter().all(|&participant| model.contains(participant)));
                    }
                }
            }
        }
    }
}
