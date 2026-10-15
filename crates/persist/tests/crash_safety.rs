//! A crash during a save never damages the project file (PERS-02, issue #37).
//!
//! Fault injection, deterministic and without sleeps:
//!
//! - **A crash at every write.** While a save runs, a hook copies the project file and its
//!   rollback journal each time a row is written, and once more when the transaction commits.
//!   Each copy is what the disk would hold if the computer stopped at that moment. Every copy
//!   must open, and hold the previous save exactly. A tiny page cache makes `SQLite` write
//!   changed pages into the file before the commit, so the copies include half-written files.
//! - **An interrupted save.** `SQLite`'s progress handler aborts the save after more and more
//!   steps; the file must still hold the previous save.
//!
//! Opts out of ARCH-02 to copy files.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use core_model::{AttributeValue, EntityId, Store};
use persist::{Connection, PersistError, Schema, open_model, open_project, save_model};
use schema::{
    Attribute, Entity, EntityKind, IfcMapping, Namespace, Placement, Presence, ValueType,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// A directory under the system temporary directory, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Result<Self> {
        let dir =
            std::env::temp_dir().join(format!("bimcraft-crash-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir)?;
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn namespace() -> Namespace {
    let attribute = |name: &str, value| Attribute {
        name: name.into(),
        value,
        presence: Presence::Optional,
        placement: Placement::Instance,
        doc: None,
    };
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
            attribute("area", "quantity:area".parse().unwrap_or(ValueType::Real)),
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

/// `count` spaces named after `label`; long names fill many pages.
fn model(label: &str, count: u128) -> Result<Store> {
    let mut store = Store::new(&namespace())?;
    for raw in 0..count {
        let name = format!("{label} {raw:04} {}", "x".repeat(200));
        #[allow(clippy::cast_precision_loss)]
        let area = raw as f64;
        store.insert(
            EntityId::new(raw),
            "Space",
            [
                ("name", AttributeValue::Text(name)),
                ("area", AttributeValue::Real(area)),
            ],
        )?;
    }
    Ok(store)
}

/// Every object of `store`, sorted by id, for comparisons.
fn objects(store: &Store) -> Vec<(EntityId, Vec<(String, AttributeValue)>)> {
    let mut objects: Vec<_> = store
        .ids()
        .filter_map(|id| Some((id, store.entity(id)?.values)))
        .collect();
    objects.sort_by_key(|(id, _)| *id);
    objects
}

/// Opens a project file and reads its model.
fn open(path: &Path) -> std::result::Result<Store, PersistError> {
    let mut conn = Connection::open(path)?;
    open_project(&mut conn)?;
    open_model(&mut conn, &Schema::new(&namespace()))
}

/// A project file holding `store`, with its connection still open.
fn project(path: &Path, store: &Store) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    open_project(&mut conn)?;
    save_model(&mut conn, &Schema::new(&namespace()), store)?;
    Ok(conn)
}

/// Copies the file and its journal, if any, as `crash-NNNN.bim` in `dir`: the disk as a crash
/// at this moment would leave it.
fn snapshot(file: &Path, dir: &Path, index: usize) -> std::io::Result<bool> {
    let target = dir.join(format!("crash-{index:04}.bim"));
    std::fs::copy(file, &target)?;
    let journal = journal(file);
    let has_journal = journal.exists();
    if has_journal {
        std::fs::copy(&journal, self::journal(&target))?;
    }
    Ok(has_journal)
}

fn journal(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push("-journal");
    PathBuf::from(name)
}

/// What the hooks saw: how many snapshots, how many with a journal, and copy failures.
#[derive(Default)]
struct Snapshots {
    taken: usize,
    with_journal: usize,
    failures: Vec<String>,
}

impl Snapshots {
    fn take(&mut self, file: &Path, dir: &Path) {
        match snapshot(file, dir, self.taken) {
            Ok(has_journal) => self.with_journal += usize::from(has_journal),
            Err(error) => self.failures.push(error.to_string()),
        }
        self.taken += 1;
    }
}

#[test]
fn a_crash_at_any_write_leaves_the_previous_save() -> Result<()> {
    let dir = TempDir::new("every-write")?;
    let file = dir.0.join("project.bim");
    let before = model("Before", 40)?;
    let after = model("After", 60)?;
    let mut conn = project(&file, &before)?;
    let original = std::fs::read(&file)?;

    // One page of cache: changed pages go to the file long before the commit.
    conn.pragma_update(None, "cache_size", 1)?;
    let crashes = dir.0.join("crashes");
    std::fs::create_dir_all(&crashes)?;
    let snapshots = Arc::new(Mutex::new(Snapshots::default()));
    let (on_write, on_commit) = (Arc::clone(&snapshots), Arc::clone(&snapshots));
    let (write_file, write_dir) = (file.clone(), crashes.clone());
    conn.update_hook(Some(move |_, _: &str, _: &str, _| {
        if let Ok(mut snapshots) = on_write.lock() {
            snapshots.take(&write_file, &write_dir);
        }
    }))?;
    let (commit_file, commit_dir) = (file.clone(), crashes.clone());
    conn.commit_hook(Some(move || {
        if let Ok(mut snapshots) = on_commit.lock() {
            snapshots.take(&commit_file, &commit_dir);
        }
        false
    }))?;
    save_model(&mut conn, &Schema::new(&namespace()), &after)?;
    conn.update_hook(None::<fn(_, &str, &str, _)>)?;
    conn.commit_hook(None::<fn() -> bool>)?;

    let snapshots = snapshots.lock().map_err(|_| "a hook panicked")?;
    assert!(snapshots.failures.is_empty(), "{:?}", snapshots.failures);
    // One per inserted object, plus the commit.
    assert_eq!(snapshots.taken, 61);
    assert_eq!(snapshots.with_journal, snapshots.taken);
    let expected = objects(&before);
    let mut half_written = 0;
    for index in 0..snapshots.taken {
        let copy = crashes.join(format!("crash-{index:04}.bim"));
        half_written += usize::from(std::fs::read(&copy)? != original);
        let reopened = open(&copy).map_err(|error| format!("crash {index}: {error}"))?;
        assert_eq!(objects(&reopened), expected, "crash {index}");
    }
    assert!(half_written > 0, "no crash caught a half-written file");
    assert_eq!(objects(&open(&file)?), objects(&after));
    Ok(())
}

#[test]
fn an_interrupted_save_leaves_the_previous_save() -> Result<()> {
    let dir = TempDir::new("interrupted")?;
    let file = dir.0.join("project.bim");
    let before = model("Before", 20)?;
    let after = model("After", 30)?;
    let mut conn = project(&file, &before)?;
    let schema_namespace = namespace();
    let schema = Schema::new(&schema_namespace);

    let expected = objects(&before);
    let mut steps = 1;
    let mut interruptions = 0;
    loop {
        // Interrupts once, so the rollback that follows runs to its end.
        let mut fired = false;
        conn.progress_handler(
            steps,
            Some(move || {
                let interrupt = !fired;
                fired = true;
                interrupt
            }),
        )?;
        let result = save_model(&mut conn, &schema, &after);
        conn.progress_handler(0, None::<fn() -> bool>)?;
        if result.is_ok() {
            break;
        }
        interruptions += 1;
        assert!(
            conn.is_autocommit(),
            "a transaction was left open after {steps} steps"
        );
        assert!(
            matches!(result, Err(PersistError::Database(_))),
            "{result:?}"
        );
        assert_eq!(objects(&open(&file)?), expected, "after {steps} steps");
        steps += steps / 4 + 1;
    }
    assert!(interruptions > 10, "only {interruptions} interruptions");
    assert_eq!(objects(&open(&file)?), objects(&after));
    Ok(())
}

#[test]
fn project_connections_use_a_synced_rollback_journal() -> Result<()> {
    let dir = TempDir::new("pragmas")?;
    let mut conn = Connection::open(dir.0.join("project.bim"))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "OFF")?;
    open_project(&mut conn)?;
    let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    let synchronous: i64 = conn.pragma_query_value(None, "synchronous", |row| row.get(0))?;
    assert_eq!((mode.as_str(), synchronous), ("delete", 2));
    Ok(())
}

#[test]
fn refused_files_keep_their_journal_mode() -> Result<()> {
    let dir = TempDir::new("refused")?;
    for (name, setup) in [
        (
            "foreign.db",
            "PRAGMA application_id = 7; CREATE TABLE invoices (id INTEGER);",
        ),
        (
            "newer.bim",
            "PRAGMA application_id = 1112100163; PRAGMA user_version = 99; CREATE TABLE t (x);",
        ),
    ] {
        let path = dir.0.join(name);
        let other = Connection::open(&path)?;
        other.pragma_update(None, "journal_mode", "WAL")?;
        other.execute_batch(setup)?;
        other.close().map_err(|(_, error)| error)?;

        let mut conn = Connection::open(&path)?;
        assert!(open_project(&mut conn).is_err(), "{name}");
        let mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
        assert_eq!(mode, "wal", "{name}");
    }
    Ok(())
}
