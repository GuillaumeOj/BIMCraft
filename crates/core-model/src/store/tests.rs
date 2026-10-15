use std::collections::BTreeMap;

use proptest::prelude::*;
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Placement, Presence, QuantityKind, TypeDef, Value,
    ValueType,
};

use super::*;
use crate::{IdSource, SeededIdSource};

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

/// A wall with one attribute of each kind of rule and placement, and a storey.
fn namespace() -> Namespace {
    use Placement::{Instance, Type, TypeOverridable};
    let length = || ValueType::Quantity(QuantityKind::Length);
    let mut wall = entity(
        "Wall",
        EntityKind::Element,
        "IfcWall",
        vec![
            attribute(
                "axis",
                ValueType::Ref("Storey".into()),
                Presence::Required,
                Instance,
            ),
            attribute(
                "height",
                length(),
                Presence::Default(Value::Real(2.5)),
                TypeOverridable,
            ),
            attribute(
                "layers",
                ValueType::List(Box::new(length())),
                Presence::Optional,
                Type,
            ),
            attribute(
                "base_offset",
                length(),
                Presence::Default(Value::Real(0.0)),
                Instance,
            ),
            attribute("name", ValueType::Text, Presence::Optional, Instance),
            attribute("is_external", ValueType::Bool, Presence::Optional, Instance),
            attribute(
                "layer_count",
                ValueType::Integer,
                Presence::Optional,
                Instance,
            ),
            attribute(
                "tags",
                ValueType::List(Box::new(ValueType::Text)),
                Presence::Optional,
                Instance,
            ),
        ],
    );
    wall.type_def = Some(TypeDef {
        name: "WallType".into(),
        doc: None,
    });
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
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [wall, storey]
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::new(),
    }
}

fn store() -> Store {
    Store::new(&namespace()).unwrap()
}

fn id(raw: u128) -> EntityId {
    EntityId::new(raw)
}

fn real(value: f64) -> AttributeValue {
    AttributeValue::Real(value)
}

fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

/// The required axis of a wall, on storey 100.
fn axis() -> (&'static str, AttributeValue) {
    ("axis", AttributeValue::Ref(id(100)))
}

/// Inserts a wall with the given extra values.
fn insert_wall<const N: usize>(store: &mut Store, raw: u128, values: [(&str, AttributeValue); N]) {
    store
        .insert(id(raw), "Wall", std::iter::once(axis()).chain(values))
        .unwrap();
}

#[test]
fn starts_empty() {
    let store = store();
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
    assert!(!store.contains(id(1)));
    assert_eq!(store.entity_of(id(1)), None);
}

#[test]
fn rejects_invalid_schemas() {
    let mut namespace = namespace();
    namespace.version = 0;
    let Err(StoreError::InvalidSchema(errors)) = Store::new(&namespace) else {
        panic!("a version-0 schema is invalid");
    };
    assert_eq!(errors.len(), 1);
}

#[test]
fn creates_and_reads_objects() {
    let mut store = store();
    insert_wall(&mut store, 1, [("name", text("W1")), ("height", real(3.0))]);
    store
        .insert(id(2), "Storey", [("elevation", real(3.2))])
        .unwrap();

    assert_eq!(store.len(), 2);
    assert!(!store.is_empty());
    assert!(store.contains(id(1)));
    assert_eq!(store.entity_of(id(1)), Some("Wall"));
    assert_eq!(store.entity_of(id(2)), Some("Storey"));
    assert_eq!(store.get(id(1), "name"), Ok(Some(text("W1"))));
    assert_eq!(store.get(id(1), "height"), Ok(Some(real(3.0))));
    assert_eq!(
        store.get(id(1), "axis"),
        Ok(Some(AttributeValue::Ref(id(100))))
    );
    assert_eq!(store.get(id(2), "elevation"), Ok(Some(real(3.2))));
}

#[test]
fn fills_instance_defaults_and_leaves_overrides_and_optionals_empty() {
    let mut store = store();
    insert_wall(&mut store, 1, []);
    assert_eq!(store.get(id(1), "base_offset"), Ok(Some(real(0.0))));
    // The type provides the height; an instance only stores an override.
    assert_eq!(store.get(id(1), "height"), Ok(None));
    assert_eq!(store.get(id(1), "name"), Ok(None));
}

#[test]
fn type_attributes_are_not_stored_on_instances() {
    let mut store = store();
    insert_wall(&mut store, 1, []);
    let unknown = StoreError::UnknownAttribute {
        entity: "Wall".into(),
        attribute: "layers".into(),
    };
    assert_eq!(store.get(id(1), "layers"), Err(unknown.clone()));
    assert_eq!(
        store.insert(
            id(2),
            "Wall",
            [axis(), ("layers", AttributeValue::List(vec![]))]
        ),
        Err(unknown)
    );
}

#[test]
fn insert_errors_store_nothing() {
    let mut store = store();
    insert_wall(&mut store, 1, []);
    let cases = [
        (id(1), "Wall", vec![axis()], StoreError::DuplicateId(id(1))),
        (
            id(2),
            "Roof",
            vec![axis()],
            StoreError::UnknownEntity("Roof".into()),
        ),
        (
            id(2),
            "Wall",
            vec![axis(), ("colour", text("red"))],
            StoreError::UnknownAttribute {
                entity: "Wall".into(),
                attribute: "colour".into(),
            },
        ),
        (
            id(2),
            "Wall",
            vec![axis(), axis()],
            StoreError::DuplicateAttribute("axis".into()),
        ),
        (
            id(2),
            "Wall",
            vec![("name", text("W2"))],
            StoreError::MissingRequired("axis".into()),
        ),
        // The type error comes after earlier columns took a value, which must be undone.
        (
            id(2),
            "Wall",
            vec![axis(), ("name", text("W2")), ("layer_count", real(1.0))],
            StoreError::TypeMismatch("layer_count".into()),
        ),
        (
            id(2),
            "Wall",
            vec![axis(), ("base_offset", real(f64::NAN))],
            StoreError::NonFinite("base_offset".into()),
        ),
    ];
    for (raw, entity, values, error) in cases {
        assert_eq!(store.insert(raw, entity, values), Err(error));
        assert_eq!(store.len(), 1);
    }
    // The rolled-back row left no trace: the next insert lands in row 1.
    insert_wall(&mut store, 2, [("name", text("W2"))]);
    assert_eq!(store.get(id(2), "name"), Ok(Some(text("W2"))));
    assert_eq!(store.get(id(2), "layer_count"), Ok(None));
    assert_eq!(store.get(id(1), "name"), Ok(None));
}

#[test]
fn updates_values_and_returns_the_previous_one() {
    let mut store = store();
    insert_wall(&mut store, 1, [("name", text("W1"))]);
    assert_eq!(
        store.set(id(1), "name", Some(text("W1b"))),
        Ok(Some(text("W1")))
    );
    assert_eq!(store.get(id(1), "name"), Ok(Some(text("W1b"))));
    assert_eq!(store.set(id(1), "name", None), Ok(Some(text("W1b"))));
    assert_eq!(store.get(id(1), "name"), Ok(None));
    let tags = AttributeValue::List(vec![text("fire"), text("acoustic")]);
    assert_eq!(store.set(id(1), "tags", Some(tags.clone())), Ok(None));
    assert_eq!(store.get(id(1), "tags"), Ok(Some(tags)));
}

#[test]
fn clearing_restores_defaults_and_keeps_required_values() {
    let mut store = store();
    insert_wall(&mut store, 1, [("base_offset", real(0.3))]);
    assert_eq!(store.set(id(1), "base_offset", None), Ok(Some(real(0.3))));
    assert_eq!(store.get(id(1), "base_offset"), Ok(Some(real(0.0))));
    assert_eq!(
        store.set(id(1), "axis", None),
        Err(StoreError::MissingRequired("axis".into()))
    );
    assert_eq!(
        store.get(id(1), "axis"),
        Ok(Some(AttributeValue::Ref(id(100))))
    );
}

#[test]
fn update_errors_change_nothing() {
    let mut store = store();
    insert_wall(&mut store, 1, [("is_external", AttributeValue::Bool(true))]);
    assert_eq!(
        store.set(id(9), "name", None),
        Err(StoreError::NotFound(id(9)))
    );
    assert_eq!(
        store.set(id(1), "colour", None),
        Err(StoreError::UnknownAttribute {
            entity: "Wall".into(),
            attribute: "colour".into(),
        })
    );
    assert_eq!(
        store.set(id(1), "is_external", Some(text("yes"))),
        Err(StoreError::TypeMismatch("is_external".into()))
    );
    assert_eq!(
        store.set(id(1), "height", Some(real(f64::INFINITY))),
        Err(StoreError::NonFinite("height".into()))
    );
    assert_eq!(
        store.get(id(1), "is_external"),
        Ok(Some(AttributeValue::Bool(true)))
    );
    assert_eq!(store.get(id(1), "height"), Ok(None));
}

#[test]
fn reads_of_missing_objects_fail() {
    let store = store();
    assert_eq!(store.get(id(1), "name"), Err(StoreError::NotFound(id(1))));
}

#[test]
fn deletes_any_row_and_keeps_the_others() {
    let mut store = store();
    let count = |raw: u128| AttributeValue::Integer(i64::try_from(raw).unwrap());
    for raw in 1..=5 {
        insert_wall(&mut store, raw, [("layer_count", count(raw))]);
    }
    // Last, first and middle rows: each moves a different row into the gap.
    for removed in [5, 1, 3] {
        let entity = store.remove(id(removed)).unwrap();
        assert_eq!(entity.entity, "Wall");
        assert!(!store.contains(id(removed)));
    }
    assert_eq!(store.len(), 2);
    for kept in [2, 4] {
        assert_eq!(store.get(id(kept), "layer_count"), Ok(Some(count(kept))));
    }
    assert_eq!(store.remove(id(3)), Err(StoreError::NotFound(id(3))));
}

#[test]
fn a_removed_object_inserts_back_unchanged() {
    let mut store = store();
    insert_wall(&mut store, 1, [("name", text("W1")), ("height", real(3.0))]);
    let removed = store.remove(id(1)).unwrap();
    assert_eq!(
        removed,
        RemovedEntity {
            entity: "Wall".into(),
            values: vec![
                ("axis".into(), AttributeValue::Ref(id(100))),
                ("height".into(), real(3.0)),
                ("base_offset".into(), real(0.0)),
                ("name".into(), text("W1")),
            ],
        }
    );
    store
        .insert(id(1), &removed.entity, removed.values.clone())
        .unwrap();
    assert_eq!(store.remove(id(1)), Ok(removed));
}

#[test]
fn errors_name_attributes_but_never_values_or_ids() {
    let id = EntityId::new(0xABCD);
    let messages = [
        (
            StoreError::InvalidSchema(Vec::new()),
            "the schema does not pass validation",
        ),
        (
            StoreError::UnknownEntity("Roof".into()),
            "unknown entity \"Roof\"",
        ),
        (
            StoreError::UnknownAttribute {
                entity: "Wall".into(),
                attribute: "colour".into(),
            },
            "entity \"Wall\" has no instance attribute \"colour\"",
        ),
        (
            StoreError::DuplicateId(id),
            "an object with this id already exists",
        ),
        (StoreError::NotFound(id), "no object with this id"),
        (
            StoreError::DuplicateAttribute("name".into()),
            "attribute \"name\" is given more than once",
        ),
        (
            StoreError::MissingRequired("axis".into()),
            "attribute \"axis\" is required",
        ),
        (
            StoreError::TypeMismatch("name".into()),
            "attribute \"name\" does not accept a value of this type",
        ),
        (
            StoreError::NonFinite("height".into()),
            "attribute \"height\" only accepts finite numbers",
        ),
    ];
    for (error, message) in messages {
        assert_eq!(error.to_string(), message);
    }
}

#[test]
fn heap_bytes_grow_with_objects_and_count_the_index() {
    let mut store = store();
    let empty = store.heap_bytes();
    assert_eq!(index_heap_bytes(&store.index), 0);
    insert_wall(&mut store, 1, [("name", text("abc"))]);
    assert!(store.heap_bytes() > empty);
    let index = index_heap_bytes(&store.index);
    assert!(index >= store.index.capacity() * size_of::<(EntityId, Location)>());
}

/// Objects in the memory measurement; NFR-08's reference models hold this order of objects.
const MEASURED_OBJECTS: usize = 100_000;

/// The memory budget per wall-like object, in bytes, from the measurement in
/// docs/dev/performance-baselines.md plus a margin. Raise it only with a reason.
const BYTES_PER_OBJECT_BUDGET: usize = 200;

#[test]
fn memory_per_object_stays_within_budget() {
    let mut store = store();
    let mut ids = SeededIdSource::new(23);
    let storey = ids.next_id();
    for index in 0..MEASURED_OBJECTS {
        let values = [
            ("axis", AttributeValue::Ref(storey)),
            ("height", real(3.0)),
            ("name", text(&format!("W{index}"))),
            ("is_external", AttributeValue::Bool(index % 2 == 0)),
            ("layer_count", AttributeValue::Integer(3)),
        ];
        store.insert(ids.next_id(), "Wall", values).unwrap();
    }
    let per_object = store.heap_bytes() / MEASURED_OBJECTS;
    assert!(
        per_object <= BYTES_PER_OBJECT_BUDGET,
        "{per_object} bytes per object"
    );
}

/// One step of a random sequence of store operations.
#[derive(Clone, Debug)]
enum Op {
    Insert { raw: u8, name: Option<String> },
    Set { raw: u8, name: Option<String> },
    Remove { raw: u8 },
}

fn op() -> impl Strategy<Value = Op> {
    // A few ids, so operations often hit existing objects.
    let raw = 0..8u8;
    let name = proptest::option::of("[a-z]{0,4}");
    prop_oneof![
        (raw.clone(), name.clone()).prop_map(|(raw, name)| Op::Insert { raw, name }),
        (raw.clone(), name).prop_map(|(raw, name)| Op::Set { raw, name }),
        raw.prop_map(|raw| Op::Remove { raw }),
    ]
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    /// The store behaves like a map from id to name, whatever the order of operations.
    #[test]
    fn matches_a_reference_map(ops in proptest::collection::vec(op(), 0..64)) {
        let mut store = store();
        let mut reference: BTreeMap<u8, Option<String>> = BTreeMap::new();
        for op in ops {
            let name_value = |name: Option<String>| name.map(AttributeValue::Text);
            match op {
                Op::Insert { raw, name } => {
                    let mut values = vec![axis()];
                    values.extend(name_value(name.clone()).map(|value| ("name", value)));
                    let result = store.insert(id(raw.into()), "Wall", values);
                    prop_assert_eq!(result.is_ok(), !reference.contains_key(&raw));
                    reference.entry(raw).or_insert(name);
                }
                Op::Set { raw, name } => {
                    let result = store.set(id(raw.into()), "name", name_value(name.clone()));
                    let expected = reference
                        .get_mut(&raw)
                        .map(|previous| name_value(std::mem::replace(previous, name)))
                        .ok_or(StoreError::NotFound(id(raw.into())));
                    prop_assert_eq!(result, expected);
                }
                Op::Remove { raw } => {
                    let removed = store.remove(id(raw.into()));
                    prop_assert_eq!(removed.is_ok(), reference.remove(&raw).is_some());
                }
            }
            prop_assert_eq!(store.len(), reference.len());
            for raw in 0..8u8 {
                let expected = reference.get(&raw).map(|name| name_value(name.clone()));
                prop_assert_eq!(store.get(id(raw.into()), "name").ok(), expected);
            }
        }
    }
}
