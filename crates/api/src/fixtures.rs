//! A small schema and commands shared by the tests, after core-txn's.

use std::collections::BTreeMap;

use core_model::{AttributeValue, Component, EntityId, LibraryRef, Model};
use core_txn::ModelCommand;
use schema::{
    Attribute, Cardinality, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence,
    PropertySet, QuantityKind, Role, RoleTarget, TypeDef, Value, ValueType,
};

fn attribute(name: &str, value: ValueType, presence: Presence, placement: Placement) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence,
        placement,
        doc: None,
    }
}

fn entity(name: &str, kind: EntityKind, ifc: &str, attributes: Vec<Attribute>) -> Entity {
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
        attributes,
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

fn role(name: &str, target: EntityKind, cardinality: Cardinality, exclusive: bool) -> Role {
    Role {
        name: name.into(),
        targets: vec![RoleTarget::Kind(target)],
        cardinality,
        exclusive,
    }
}

/// Walls with a type and a property set, storeys, and the containment of walls in storeys. A
/// wall may be in several containments, so random commands relate walls more often.
pub fn namespace() -> Namespace {
    use Placement::{Instance, Type, TypeOverridable};
    let length = || ValueType::Quantity(QuantityKind::Length);
    let wall = Entity {
        type_def: Some(TypeDef {
            name: "WallType".into(),
            doc: None,
        }),
        property_sets: vec![PSET.into()],
        ..entity(
            "Wall",
            EntityKind::Element,
            "IfcWall",
            vec![
                attribute("name", ValueType::Text, Presence::Optional, Instance),
                attribute(
                    "height",
                    length(),
                    Presence::Default(Value::Real(2.5)),
                    TypeOverridable,
                ),
                attribute("thickness", length(), Presence::Required, Type),
            ],
        )
    };
    let storey = entity(
        "Storey",
        EntityKind::Spatial,
        "IfcBuildingStorey",
        vec![attribute(
            "elevation",
            length(),
            Presence::Optional,
            Instance,
        )],
    );
    let containment = Entity {
        roles: vec![
            role("structure", EntityKind::Spatial, Cardinality::One, false),
            role("elements", EntityKind::Element, Cardinality::Many, false),
        ],
        ..entity(
            "Containment",
            EntityKind::Relationship,
            "IfcRelContainedInSpatialStructure",
            Vec::new(),
        )
    };
    let wall_common = PropertySet {
        name: PSET.into(),
        applies_to: vec!["Wall".into()],
        properties: vec![
            attribute("Reference", ValueType::Text, Presence::Optional, Instance),
            attribute(
                "IsExternal",
                ValueType::Bool,
                Presence::Default(Value::Bool(true)),
                TypeOverridable,
            ),
        ],
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [wall, storey, containment]
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::from([(PSET.into(), wall_common)]),
    }
}

/// The property set of walls.
pub const PSET: &str = "Pset_WallCommon";

/// An empty model of [`namespace`].
pub fn model() -> Model {
    Model::new(&namespace()).unwrap()
}

pub fn id(raw: u128) -> EntityId {
    EntityId::new(raw)
}

pub fn real(value: f64) -> AttributeValue {
    AttributeValue::Real(value)
}

pub fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

/// Creates a wall.
pub fn create_wall(raw: u128) -> ModelCommand {
    create_named_wall(raw, None)
}

/// Creates a wall, with a name if one is given.
pub fn create_named_wall(raw: u128, name: Option<AttributeValue>) -> ModelCommand {
    ModelCommand::Create {
        id: id(raw),
        entity: "Wall".into(),
        values: name.map(|name| ("name".into(), name)).into_iter().collect(),
    }
}

/// Creates a storey.
pub fn create_storey(raw: u128) -> ModelCommand {
    ModelCommand::Create {
        id: id(raw),
        entity: "Storey".into(),
        values: Vec::new(),
    }
}

/// Sets, or with `None` clears, an attribute of object `raw`.
pub fn set(raw: u128, attribute: &str, value: Option<AttributeValue>) -> ModelCommand {
    ModelCommand::Set {
        id: id(raw),
        attribute: attribute.into(),
        value,
    }
}

/// Deletes object `raw`.
pub fn delete(raw: u128) -> ModelCommand {
    ModelCommand::Delete { id: id(raw) }
}

/// Contains `walls` in `storey` with relationship `raw`.
pub fn contain(raw: u128, storey: u128, walls: &[u128]) -> ModelCommand {
    ModelCommand::Relate {
        id: id(raw),
        entity: "Containment".into(),
        roles: vec![
            ("structure".into(), vec![id(storey)]),
            ("elements".into(), walls.iter().copied().map(id).collect()),
        ],
        values: Vec::new(),
    }
}

/// A wall component whose types are 0.2 m thick.
pub fn component() -> Component {
    Component {
        entity: "Wall".into(),
        library: LibraryRef {
            id: "acme/partition".into(),
            version: 3,
        },
        defaults: vec![("thickness".into(), real(0.2))],
    }
}

/// Applies `commands` as one.
pub fn batch<const N: usize>(commands: [ModelCommand; N]) -> ModelCommand {
    ModelCommand::Batch {
        commands: commands.into(),
    }
}

/// One command of each kind, in an order that applies to an empty model.
pub fn one_of_each() -> Vec<ModelCommand> {
    vec![
        create_storey(1),
        create_wall(2),
        create_wall(3),
        set(2, "name", Some(text("W2"))),
        contain(10, 1, &[2]),
        ModelCommand::AddParticipant {
            relationship: id(10),
            role: "elements".into(),
            participant: id(3),
        },
        ModelCommand::RemoveParticipant {
            relationship: id(10),
            role: "elements".into(),
            participant: id(2),
        },
        ModelCommand::AttachPropertySet {
            id: id(2),
            set: PSET.into(),
            values: vec![("Reference".into(), text("R2"))],
        },
        ModelCommand::SetProperty {
            id: id(3),
            set: PSET.into(),
            property: "Reference".into(),
            value: Some(text("R3")),
        },
        ModelCommand::DetachPropertySet {
            id: id(2),
            set: PSET.into(),
        },
        ModelCommand::CreateComponent {
            id: id(20),
            component: component(),
        },
        ModelCommand::CreateType {
            id: id(21),
            component: id(20),
            values: vec![("thickness".into(), real(0.3))],
        },
        ModelCommand::AssignType {
            instance: id(2),
            type_id: Some(id(21)),
        },
        delete(21),
        ModelCommand::RemoveComponent { id: id(20) },
        batch([create_wall(4), delete(4)]),
    ]
}

/// Everything a call could change in a session: each object's state and the history.
#[derive(Debug, PartialEq)]
pub struct Snapshot {
    objects: BTreeMap<EntityId, Option<core_model::ObjectState>>,
    history: Vec<(&'static str, core_txn::Origin, bool)>,
}

pub fn snapshot(session: &crate::Session) -> Snapshot {
    let document = session.document();
    let model = document.model();
    Snapshot {
        objects: model.ids().map(|id| (id, model.state(id))).collect(),
        history: document
            .history()
            .map(|entry| (entry.command, entry.origin.clone(), entry.undone))
            .collect(),
    }
}

/// A session on an empty [`model`].
pub fn session() -> crate::Session {
    crate::Session::new(model())
}
