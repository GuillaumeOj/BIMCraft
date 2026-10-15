//! Opens, writes and reopens a project file on the native file system, the way the desktop host
//! will (issues #34 and #35). Opts out of ARCH-02 to write and clean up its temporary files.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::PathBuf;

use core_model::{AttributeValue, EntityId, Store};
use persist::{
    Connection, PersistError, open_model, open_project, property, save_model, set_property,
};
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, ValueType,
};

/// A file in the system temporary directory, removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "bimcraft-persist-{}-{name}.bim",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn a_project_file_survives_closing_and_reopening() {
    let file = TempFile::new("reopen");

    let mut conn = Connection::open(&file.0).unwrap();
    open_project(&mut conn).unwrap();
    set_property(&conn, "name", "Tower").unwrap();
    conn.close().unwrap();

    let header = std::fs::read(&file.0).unwrap();
    assert!(header.starts_with(b"SQLite format 3\0"));

    let mut conn = Connection::open(&file.0).unwrap();
    open_project(&mut conn).unwrap();
    assert_eq!(property(&conn, "name").unwrap().as_deref(), Some("Tower"));
}

/// A namespace with one entity holding a text attribute.
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
        attributes: vec![Attribute {
            name: "name".into(),
            value: ValueType::Text,
            presence: Presence::Optional,
            placement: Placement::Instance,
            doc: None,
        }],
        roles: Vec::new(),
        representations: Vec::new(),
    };
    Namespace {
        name: "core".into(),
        version: 1,
        entities: [("Space".to_owned(), space)].into(),
        property_sets: std::collections::BTreeMap::new(),
    }
}

/// Saves a model of `count` named spaces into `file`.
fn save_spaces(file: &TempFile, count: u128) -> Result<Store, Box<dyn std::error::Error>> {
    let namespace = namespace();
    let mut store = Store::new(&namespace)?;
    for raw in 0..count {
        let name = AttributeValue::Text(format!("Room {raw:04}"));
        store.insert(EntityId::new(raw), "Space", [("name", name)])?;
    }
    let mut conn = Connection::open(&file.0)?;
    open_project(&mut conn)?;
    save_model(&mut conn, &namespace, &store)?;
    conn.close().map_err(|(_, error)| error)?;
    Ok(store)
}

#[test]
fn a_model_survives_closing_and_reopening() {
    let file = TempFile::new("model");
    let store = save_spaces(&file, 3).unwrap();

    let mut conn = Connection::open(&file.0).unwrap();
    open_project(&mut conn).unwrap();
    let reopened = open_model(&mut conn, &namespace()).unwrap();
    assert_eq!(reopened.len(), store.len());
    for raw in 0..3 {
        let id = EntityId::new(raw);
        assert_eq!(reopened.get(id, "name"), store.get(id, "name"));
    }
}

#[test]
fn a_file_that_is_not_a_database_is_not_a_project() {
    let file = TempFile::new("garbage");
    std::fs::write(&file.0, [0x42; 4096]).unwrap();
    let mut conn = Connection::open(&file.0).unwrap();
    let error = open_project(&mut conn).unwrap_err();
    assert!(matches!(error, PersistError::NotAProject), "{error:?}");
}

#[test]
fn a_truncated_file_is_damaged() {
    let file = TempFile::new("truncated");
    save_spaces(&file, 2000).unwrap();
    let bytes = std::fs::read(&file.0).unwrap();
    std::fs::write(&file.0, &bytes[..bytes.len() / 2]).unwrap();

    let mut conn = Connection::open(&file.0).unwrap();
    let error = open_project(&mut conn)
        .and_then(|()| open_model(&mut conn, &namespace()).map(drop))
        .unwrap_err();
    assert!(matches!(error, PersistError::Corrupt), "{error:?}");
}
