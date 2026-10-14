//! Benchmarks for the attribute tables: run with `cargo bench -p core-model`.
//!
//! Each benchmark works on 100,000 wall-like objects, the order of magnitude of the reference
//! models (NFR-08). The baseline lives in docs/dev/performance-baselines.md.

#![expect(
    clippy::expect_used,
    reason = "a benchmark with an invalid fixture cannot run"
)]

use std::hint::black_box;

use core_model::{AttributeValue, EntityId, Store};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, QuantityKind, Value,
    ValueType,
};

const OBJECTS: u64 = 100_000;

fn attribute(name: &str, value: ValueType, presence: Presence) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence,
        placement: Placement::Instance,
        doc: None,
    }
}

/// A wall with a reference, two lengths, a text, a flag and an integer.
fn namespace() -> Namespace {
    let length = || ValueType::Quantity(QuantityKind::Length);
    let wall = Entity {
        name: "Wall".into(),
        kind: EntityKind::Element,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: "IfcWall".into(),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes: vec![
            attribute("axis", ValueType::Ref("Wall".into()), Presence::Required),
            attribute("height", length(), Presence::Default(Value::Real(2.5))),
            attribute("base_offset", length(), Presence::Default(Value::Real(0.0))),
            attribute("name", ValueType::Text, Presence::Optional),
            attribute("is_external", ValueType::Bool, Presence::Optional),
            attribute("layer_count", ValueType::Integer, Presence::Optional),
        ],
        roles: Vec::new(),
        representations: Vec::new(),
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [(wall.name.clone(), wall)].into(),
        property_sets: [].into(),
    }
}

/// Spread-out, deterministic ids, like the random ones of a real model.
fn id(index: u64) -> EntityId {
    EntityId::from_random_bits(
        u128::from(index).wrapping_mul(0x9E37_79B9_7F4A_7C15_F39C_C060_5CED_C835),
    )
}

type Values = [(&'static str, AttributeValue); 5];

/// The attribute values of every object, built outside the timed code.
fn objects() -> Vec<Values> {
    (0..OBJECTS)
        .map(|index| {
            [
                ("axis", AttributeValue::Ref(id(0))),
                ("height", AttributeValue::Real(3.0)),
                ("name", AttributeValue::Text(format!("W{index}"))),
                ("is_external", AttributeValue::Bool(index % 2 == 0)),
                ("layer_count", AttributeValue::Integer(3)),
            ]
        })
        .collect()
}

fn insert_all(store: &mut Store, objects: Vec<Values>) {
    for (index, values) in (0..).zip(objects) {
        store
            .insert(id(index), "Wall", values)
            .expect("the benchmark objects are valid");
    }
}

fn bench_store(criterion: &mut Criterion) {
    let empty = Store::new(&namespace()).expect("the benchmark schema is valid");
    let mut full = empty.clone();
    insert_all(&mut full, objects());
    let mut group = criterion.benchmark_group("store_100k");
    group.sample_size(10);
    group.bench_function("insert", |bench| {
        bench.iter_batched(
            || (empty.clone(), objects()),
            |(mut store, objects)| {
                insert_all(&mut store, objects);
                store
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("get", |bench| {
        bench.iter(|| {
            for index in 0..OBJECTS {
                black_box(full.get(black_box(id(index)), "height").ok());
            }
        });
    });
    group.bench_function("set", |bench| {
        bench.iter_batched_ref(
            || full.clone(),
            |store| {
                for index in 0..OBJECTS {
                    store
                        .set(id(index), "height", Some(AttributeValue::Real(2.7)))
                        .expect("every benchmark object is stored");
                }
            },
            BatchSize::LargeInput,
        );
    });
    group.bench_function("remove", |bench| {
        bench.iter_batched_ref(
            || full.clone(),
            |store| {
                for index in 0..OBJECTS {
                    black_box(
                        store
                            .remove(id(index))
                            .expect("every benchmark object is stored"),
                    );
                }
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(benches, bench_store);
criterion_main!(benches);
