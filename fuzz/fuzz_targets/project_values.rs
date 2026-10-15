//! Feeds arbitrary bytes as the stored values of a project file's objects to `persist`, which
//! reads them back into a model (PERS-01). Opening must never panic: a malformed id, text or
//! list blob is a typed error.

#![no_main]

use std::cell::RefCell;
use std::collections::BTreeMap;

use core_model::Store;
use libfuzzer_sys::fuzz_target;
use persist::{Connection, Schema, open_model, open_project, save_model};
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, QuantityKind,
    ValueType,
};

fn attribute(name: &str, value: ValueType) -> Attribute {
    Attribute {
        name: name.into(),
        value,
        presence: Presence::Optional,
        placement: Placement::Instance,
        doc: None,
    }
}

fn list_of(value: ValueType) -> ValueType {
    ValueType::List(Box::new(value))
}

/// A space whose attributes cover each blob-encoded value type.
fn namespace() -> Namespace {
    let space = Entity {
        name: "Space".into(),
        kind: EntityKind::Spatial,
        doc: None,
        deprecated: None,
        property_sets: Vec::new(),
        type_def: None,
        ifc: Some(IfcMapping {
            entity: "IfcSpace".into(),
            predefined_type: None,
            object_type: None,
            type_entity: None,
        }),
        attributes: vec![
            attribute("name", ValueType::Text),
            attribute("bounds", list_of(ValueType::Ref("Space".into()))),
            attribute("labels", list_of(list_of(ValueType::Text))),
            attribute(
                "heights",
                list_of(ValueType::Quantity(QuantityKind::Length)),
            ),
        ],
        roles: Vec::new(),
        representations: Vec::new(),
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: BTreeMap::from([("Space".to_owned(), space)]),
        property_sets: BTreeMap::new(),
    }
}

/// A project holding the namespace's tables, built once and reused by every input.
fn project(namespace: &Namespace) -> Connection {
    let mut conn = Connection::open_in_memory().expect("in-memory database");
    open_project(&mut conn).expect("new project");
    let store = Store::new(namespace).expect("valid namespace");
    save_model(&mut conn, &Schema::new(namespace), &store).expect("empty model");
    conn
}

thread_local! {
    static PROJECT: RefCell<(Namespace, Connection)> = RefCell::new({
        let namespace = namespace();
        let conn = project(&namespace);
        (namespace, conn)
    });
}

fuzz_target!(|columns: [Vec<u8>; 5]| {
    PROJECT.with_borrow_mut(|(namespace, conn)| {
        let [id, name, bounds, labels, heights] = columns;
        conn.execute("DELETE FROM \"core/Space\"", [])
            .expect("clear the table");
        conn.execute(
            "INSERT INTO \"core/Space\" VALUES (?1, CAST(?2 AS TEXT), ?3, ?4, ?5)",
            (id, name, bounds, labels, heights),
        )
        .expect("store the bytes");
        let _ = open_model(conn, &Schema::new(namespace));
    });
});
