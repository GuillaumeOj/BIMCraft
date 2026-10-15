//! Opens files written by every earlier version (PERS-01): one committed fixture per file format
//! version and per version of a sample namespace, each opened and compared with the model the
//! current version expects.
//!
//! Fixtures are written once with `UPDATE_SNAPSHOTS=1 cargo test -p persist --test migrations`
//! and never rewritten: they stand for files users already have. A change to the file's own
//! tables without a new format version fails against `format-NNNN.sql`.
//!
//! Opts out of ARCH-02 to read the fixtures and work on copies of them.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use core_model::{AttributeValue, EntityId, Store};
use persist::{
    Connection, CustomStep, FORMAT_VERSION, PersistError, Schema, Transaction, open_model,
    open_project, property, save_model, set_property,
};
use schema::{
    Attribute, DeclaredStep, Entity, EntityKind, IfcMapping, Migration, Namespace, Placement,
    Presence, Value, ValueType,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Writes a fixture with `write` when it is missing and `UPDATE_SNAPSHOTS` is set.
fn write_once(name: &str, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    let path = fixture(name);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() && !path.exists() {
        std::fs::create_dir_all(fixture(""))?;
        write(&path)?;
    }
    assert!(
        path.exists(),
        "{name} is missing: write it with `UPDATE_SNAPSHOTS=1 cargo test -p persist --test migrations`"
    );
    Ok(())
}

/// A copy of a fixture under the system temporary directory, removed when dropped.
struct Copy(PathBuf);

impl Copy {
    fn of(name: &str) -> Result<Self> {
        let path = std::env::temp_dir().join(format!("bimcraft-{}-{name}", std::process::id()));
        std::fs::copy(fixture(name), &path)?;
        Ok(Self(path))
    }
}

impl Drop for Copy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

// --- File format -------------------------------------------------------------------------------

/// The file's own tables, as `CREATE` statements sorted by name.
fn format_tables(conn: &Connection) -> Result<String> {
    let mut statement = conn.prepare(
        "SELECT sql FROM sqlite_schema WHERE name NOT LIKE '%/%' AND sql IS NOT NULL ORDER BY name",
    )?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(tables.join(";\n") + ";\n")
}

#[test]
fn the_format_tables_match_their_version() -> Result<()> {
    let name = format!("format-{FORMAT_VERSION:04}.sql");
    let mut conn = Connection::open_in_memory()?;
    open_project(&mut conn)?;
    let tables = format_tables(&conn)?;
    write_once(&name, |path| Ok(std::fs::write(path, &tables)?))?;
    assert_eq!(
        tables,
        std::fs::read_to_string(fixture(&name))?,
        "the file's own tables changed: raise FORMAT_VERSION and add a format migration"
    );
    Ok(())
}

#[test]
fn files_of_every_format_version_open() -> Result<()> {
    let current = std::fs::read_to_string(fixture(&format!("format-{FORMAT_VERSION:04}.sql")))?;
    for version in 1..=FORMAT_VERSION {
        let name = format!("format-{version:04}.bim");
        write_once(&name, |path| {
            let mut conn = Connection::open(path)?;
            open_project(&mut conn)?;
            set_property(&conn, "name", "Tower")?;
            Ok(())
        })?;
        let copy = Copy::of(&name)?;
        let mut conn = Connection::open(&copy.0)?;
        open_project(&mut conn)?;
        assert_eq!(property(&conn, "name")?.as_deref(), Some("Tower"), "{name}");
        set_property(&conn, "city", "Lyon")?;
        assert_eq!(format_tables(&conn)?, current, "{name}");
    }
    Ok(())
}

// --- Schema versions of a sample namespace --------------------------------------------------------

fn attribute(name: &str, value: &str, presence: Presence) -> Attribute {
    Attribute {
        name: name.into(),
        value: value.parse().unwrap_or(ValueType::Text),
        presence,
        placement: Placement::Instance,
        doc: None,
    }
}

fn optional(name: &str, value: &str) -> Attribute {
    attribute(name, value, Presence::Optional)
}

fn required(name: &str, value: &str) -> Attribute {
    attribute(name, value, Presence::Required)
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

/// Version 1 stores storeys with a label and walls with a thickness in millimetres; version 2
/// renames storeys to levels and labels to names, converts thicknesses to metres, drops a code,
/// adds a wall height and slabs; version 3 requires level names and an `is_external` flag that
/// custom code sets.
fn sample(version: u32) -> Namespace {
    let spatial = EntityKind::Spatial;
    let element = EntityKind::Element;
    let entities = if version == 1 {
        vec![
            entity(
                "Storey",
                spatial,
                "IfcBuildingStorey",
                vec![optional("label", "text"), optional("elevation", "real")],
            ),
            entity(
                "Wall",
                element,
                "IfcWall",
                vec![
                    required("storey", "ref:Storey"),
                    optional("thickness", "real"),
                    optional("old_code", "integer"),
                ],
            ),
        ]
    } else {
        let name = if version == 2 {
            optional("name", "text")
        } else {
            required("name", "text")
        };
        let mut wall = vec![
            required("storey", "ref:Level"),
            optional("thickness", "quantity:length"),
            attribute(
                "height",
                "quantity:length",
                Presence::Default(Value::Real(2.5)),
            ),
        ];
        if version == 3 {
            wall.push(required("is_external", "bool"));
        }
        vec![
            entity(
                "Level",
                spatial,
                "IfcBuildingStorey",
                vec![name, optional("elevation", "real")],
            ),
            entity("Wall", element, "IfcWall", wall),
            entity(
                "Slab",
                element,
                "IfcSlab",
                vec![optional("depth", "quantity:length")],
            ),
        ]
    };
    Namespace {
        name: "core".into(),
        version,
        entities: entities
            .into_iter()
            .map(|entity| (entity.name.clone(), entity))
            .collect(),
        property_sets: BTreeMap::new(),
    }
}

fn declared(version: u32) -> Vec<DeclaredStep> {
    if version == 2 {
        vec![
            DeclaredStep::RenameEntity {
                old_name: "Storey".into(),
                new_name: "Level".into(),
            },
            DeclaredStep::RenameAttribute {
                entity: "Level".into(),
                old_name: "label".into(),
                new_name: "name".into(),
            },
            DeclaredStep::ChangeValue {
                entity: "Wall".into(),
                attribute: "thickness".into(),
                value: "quantity:length".parse().unwrap_or(ValueType::Real),
                factor: 0.001,
            },
        ]
    } else {
        vec![
            DeclaredStep::SetValue {
                entity: "Level".into(),
                attribute: "name".into(),
                value: Value::Text("Unnamed".into()),
            },
            DeclaredStep::Custom {
                entities: vec!["Wall".into()],
                function: "mark_external".into(),
            },
        ]
    }
}

/// The custom step of version 3: every existing wall is internal.
fn mark_external(tx: &Transaction<'_>) -> std::result::Result<(), PersistError> {
    tx.execute("UPDATE \"core/Wall\" SET \"is_external\" = 0", [])?;
    Ok(())
}

const CUSTOM_STEPS: [(&str, CustomStep); 1] = [("mark_external", mark_external)];

/// The migrations to versions 2 and 3, resolved like `schema_gen::History` resolves them.
fn migrations() -> std::result::Result<Vec<Migration>, Vec<schema::MigrationError>> {
    (2..=3)
        .map(|version| schema::resolve(&sample(version - 1), &declared(version), &sample(version)))
        .collect()
}

fn id(raw: u128) -> EntityId {
    EntityId::new(raw)
}

fn text(value: &str) -> AttributeValue {
    AttributeValue::Text(value.into())
}

type Objects = Vec<(EntityId, &'static str, Vec<(&'static str, AttributeValue)>)>;

/// The model saved in the fixture of `version`.
fn saved_model(version: u32) -> Objects {
    use AttributeValue::{Bool, Real, Ref};
    match version {
        1 => vec![
            (
                id(1),
                "Storey",
                vec![("label", text("Ground")), ("elevation", Real(0.0))],
            ),
            (id(2), "Storey", vec![("elevation", Real(3.0))]),
            (
                id(10),
                "Wall",
                vec![
                    ("storey", Ref(id(1))),
                    ("thickness", Real(200.0)),
                    ("old_code", AttributeValue::Integer(7)),
                ],
            ),
            (id(11), "Wall", vec![("storey", Ref(id(2)))]),
        ],
        2 => vec![
            (id(1), "Level", vec![("name", text("Ground"))]),
            (
                id(10),
                "Wall",
                vec![
                    ("storey", Ref(id(1))),
                    ("thickness", Real(0.2)),
                    ("height", Real(3.0)),
                ],
            ),
            (id(20), "Slab", vec![("depth", Real(0.25))]),
        ],
        _ => vec![
            (id(1), "Level", vec![("name", text("Ground"))]),
            (
                id(10),
                "Wall",
                vec![
                    ("storey", Ref(id(1))),
                    ("height", Real(3.0)),
                    ("is_external", Bool(true)),
                ],
            ),
        ],
    }
}

/// What the model saved by `version` becomes in version 3.
fn expected_model(version: u32) -> Objects {
    use AttributeValue::{Bool, Real, Ref};
    let wall = |raw, storey, thickness: Option<f64>, height| {
        let mut values = vec![("storey", Ref(id(storey)))];
        values.extend(thickness.map(|thickness| ("thickness", Real(thickness))));
        values.extend([("height", Real(height)), ("is_external", Bool(false))]);
        (id(raw), "Wall", values)
    };
    match version {
        1 => vec![
            (
                id(1),
                "Level",
                vec![("name", text("Ground")), ("elevation", Real(0.0))],
            ),
            (
                id(2),
                "Level",
                vec![("name", text("Unnamed")), ("elevation", Real(3.0))],
            ),
            wall(10, 1, Some(200.0 * 0.001), 2.5),
            wall(11, 2, None, 2.5),
        ],
        2 => vec![
            (id(1), "Level", vec![("name", text("Ground"))]),
            wall(10, 1, Some(0.2), 3.0),
            (id(20), "Slab", vec![("depth", Real(0.25))]),
        ],
        _ => saved_model(3),
    }
}

/// `objects` in a store of `namespace`.
fn store(namespace: &Namespace, objects: Objects) -> Result<Store> {
    let mut store = Store::new(namespace)?;
    for (id, entity, values) in objects {
        store.insert(id, entity, values)?;
    }
    Ok(store)
}

/// Every object of a store with owned names: id, entity, attribute values.
type OwnedObjects = Vec<(EntityId, String, Vec<(String, AttributeValue)>)>;

/// Every object of `store` sorted by id.
fn sorted(store: &Store) -> OwnedObjects {
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

#[test]
fn files_of_every_schema_version_open_in_the_current_one() -> Result<()> {
    let current = sample(3);
    let migrations = migrations().unwrap();
    let schema = Schema {
        namespace: &current,
        migrations: &migrations,
        custom_steps: &CUSTOM_STEPS,
    };
    for version in 1..=current.version {
        let name = format!("sample-{version:04}.bim");
        write_once(&name, |path| {
            let namespace = sample(version);
            let mut conn = Connection::open(path)?;
            open_project(&mut conn)?;
            save_model(
                &mut conn,
                &Schema::new(&namespace),
                &store(&namespace, saved_model(version))?,
            )?;
            Ok(())
        })?;
        let expected = sorted(&store(&current, expected_model(version))?);

        let copy = Copy::of(&name)?;
        let before = std::fs::read(&copy.0)?;
        let mut conn = Connection::open(&copy.0)?;
        open_project(&mut conn)?;
        let opened = open_model(&mut conn, &schema)?;
        assert_eq!(sorted(&opened), expected, "{name}");
        assert_eq!(
            std::fs::read(&copy.0)?,
            before,
            "{name}: opening changed the file"
        );

        // Saving writes the current version: the file opens without migrations afterwards.
        save_model(&mut conn, &schema, &opened)?;
        let reopened = open_model(&mut conn, &Schema::new(&current))?;
        assert_eq!(sorted(&reopened), expected, "{name}");
    }
    Ok(())
}

#[test]
fn a_file_needing_unavailable_code_is_refused() -> Result<()> {
    let current = sample(3);
    let migrations = migrations().unwrap();
    let schema = Schema {
        namespace: &current,
        migrations: &migrations,
        custom_steps: &[],
    };
    let copy = Copy::of("sample-0002.bim")?;
    let mut conn = Connection::open(&copy.0)?;
    open_project(&mut conn)?;
    let error = open_model(&mut conn, &schema).unwrap_err();
    assert!(matches!(error, PersistError::UnknownCustomStep(name) if name == "mark_external"));
    Ok(())
}
