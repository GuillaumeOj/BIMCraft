//! A sample namespace with every value type, and strategies for random models of it.

use std::collections::BTreeMap;

use core_model::{AttributeValue, EntityId, Store};
use proptest::prelude::*;
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, QuantityKind,
    TypeDef, Value, ValueType,
};

/// Fixed seed and no regression files, like the other crates' property tests.
pub fn proptest_config() -> proptest::test_runner::Config {
    proptest::test_runner::Config {
        rng_seed: proptest::test_runner::RngSeed::Fixed(0x00C0_FFEE),
        failure_persistence: None,
        cases: 64,
        ..proptest::test_runner::Config::default()
    }
}

fn attribute(name: &str, value: ValueType, presence: Presence, placement: Placement) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence,
        placement,
        doc: None,
    }
}

fn optional(name: &str, value: ValueType) -> Attribute {
    attribute(name, value, Presence::Optional, Placement::Instance)
}

fn list_of(value: ValueType) -> ValueType {
    ValueType::List(Box::new(value))
}

fn length() -> ValueType {
    ValueType::Quantity(QuantityKind::Length)
}

/// An element entity mapped to `Ifc<name>`.
pub fn entity(name: &str, attributes: Vec<Attribute>) -> Entity {
    Entity {
        name: name.into(),
        kind: EntityKind::Element,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: format!("Ifc{name}"),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes,
        roles: Vec::new(),
        representations: Vec::new(),
    }
}

/// A storey, and a wall with one attribute of every value type the schema accepts and every
/// rule and placement.
pub fn namespace() -> Namespace {
    let mut storey = Entity {
        kind: EntityKind::Spatial,
        ..entity("BuildingStorey", vec![optional("elevation", length())])
    };
    storey.name = "Storey".into();
    let mut wall = entity(
        "Wall",
        vec![
            attribute(
                "axis",
                ValueType::Ref("Storey".into()),
                Presence::Required,
                Placement::Instance,
            ),
            attribute(
                "height",
                length(),
                Presence::Default(Value::Real(2.5)),
                Placement::TypeOverridable,
            ),
            attribute(
                "layers",
                list_of(length()),
                Presence::Optional,
                Placement::Type,
            ),
            optional("flags", list_of(list_of(ValueType::Bool))),
            optional("layer_count", ValueType::Integer),
            optional("name", ValueType::Text),
            optional("is_external", ValueType::Bool),
            optional("tags", list_of(ValueType::Text)),
            optional("ratio", ValueType::Real),
            optional("hosts", list_of(ValueType::Ref("Storey".into()))),
            optional("counts", list_of(ValueType::Integer)),
            optional("offsets", list_of(length())),
        ],
    );
    wall.type_def = Some(TypeDef {
        name: "WallType".into(),
        doc: None,
    });
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [storey, wall]
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::new(),
    }
}

/// A wall with every instance attribute set.
pub fn object() -> Vec<(&'static str, AttributeValue)> {
    let id = AttributeValue::Ref(EntityId::new(u128::MAX));
    let real = AttributeValue::Real;
    vec![
        ("axis", id.clone()),
        ("height", real(3.1)),
        (
            "flags",
            AttributeValue::List(vec![AttributeValue::List(vec![AttributeValue::Bool(true)])]),
        ),
        ("layer_count", AttributeValue::Integer(-3)),
        ("name", AttributeValue::Text("Mur nord".into())),
        ("is_external", AttributeValue::Bool(false)),
        (
            "tags",
            AttributeValue::List(vec![AttributeValue::Text("A".into())]),
        ),
        ("ratio", real(-1e-12)),
        ("hosts", AttributeValue::List(vec![id])),
        (
            "counts",
            AttributeValue::List(vec![AttributeValue::Integer(i64::MAX)]),
        ),
        ("offsets", AttributeValue::List(vec![real(0.5), real(-0.0)])),
    ]
}

/// Every object of a store with owned names: id, entity, attribute values.
pub type Objects = Vec<(EntityId, String, Vec<(String, AttributeValue)>)>;

/// The objects of `store`, sorted by id, for comparisons.
pub fn sorted_objects(store: &Store) -> Objects {
    let mut objects: Vec<_> = store
        .ids()
        .filter_map(|id| {
            let object = store.entity(id)?;
            Some((id, object.entity, object.values))
        })
        .collect();
    objects.sort_by_key(|(id, ..)| *id);
    objects
}

fn finite() -> impl Strategy<Value = AttributeValue> {
    any::<f64>()
        .prop_filter("finite", |number| number.is_finite())
        .prop_map(AttributeValue::Real)
}

fn reference() -> impl Strategy<Value = AttributeValue> {
    any::<u128>().prop_map(|raw| AttributeValue::Ref(EntityId::new(raw)))
}

fn list(item: impl Strategy<Value = AttributeValue>) -> impl Strategy<Value = AttributeValue> {
    proptest::collection::vec(item, 0..4).prop_map(AttributeValue::List)
}

fn text() -> impl Strategy<Value = AttributeValue> {
    any::<String>().prop_map(AttributeValue::Text)
}

fn integer() -> impl Strategy<Value = AttributeValue> {
    any::<i64>().prop_map(AttributeValue::Integer)
}

/// The values of a random wall: the required axis, and any subset of the others.
fn wall() -> impl Strategy<Value = Vec<(&'static str, AttributeValue)>> {
    let bool = || any::<bool>().prop_map(AttributeValue::Bool);
    (
        reference(),
        (
            proptest::option::of(finite()),
            proptest::option::of(list(list(bool()))),
            proptest::option::of(integer()),
            proptest::option::of(text()),
            proptest::option::of(bool()),
            proptest::option::of(list(text())),
        ),
        (
            proptest::option::of(finite()),
            proptest::option::of(list(reference())),
            proptest::option::of(list(integer())),
            proptest::option::of(list(finite())),
        ),
    )
        .prop_map(
            |(
                axis,
                (height, flags, count, name, external, tags),
                (ratio, hosts, counts, offsets),
            )| {
                let optional = [
                    ("height", height),
                    ("flags", flags),
                    ("layer_count", count),
                    ("name", name),
                    ("is_external", external),
                    ("tags", tags),
                    ("ratio", ratio),
                    ("hosts", hosts),
                    ("counts", counts),
                    ("offsets", offsets),
                ];
                std::iter::once(("axis", axis))
                    .chain(
                        optional
                            .into_iter()
                            .filter_map(|(name, value)| Some((name, value?))),
                    )
                    .collect()
            },
        )
}

/// A random model of [`namespace`]: walls, wall types and storeys under distinct ids.
pub fn store_strategy() -> impl Strategy<Value = Store> {
    let object = prop_oneof![
        wall().prop_map(|values| ("Wall", values)),
        (
            proptest::option::of(finite()),
            proptest::option::of(list(finite()))
        )
            .prop_map(|(height, layers)| {
                let values = [("height", height), ("layers", layers)]
                    .into_iter()
                    .filter_map(|(name, value)| Some((name, value?)))
                    .collect();
                ("WallType", values)
            }),
        proptest::option::of(finite()).prop_map(|elevation| (
            "Storey",
            elevation
                .map(|value| ("elevation", value))
                .into_iter()
                .collect()
        )),
    ];
    proptest::collection::btree_map(any::<u128>(), object, 0..12).prop_map(|objects| {
        let mut store = Store::new(&namespace()).unwrap_or_else(|_| unreachable!());
        for (raw, (entity, values)) in objects {
            store
                .insert(EntityId::new(raw), entity, values)
                .unwrap_or_else(|_| unreachable!());
        }
        store
    })
}
