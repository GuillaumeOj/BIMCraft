//! `BIMCraft` project files, stored as `SQLite` databases (specification §5.3).
//!
//! This crate never opens a file: the host opens the [`Connection`] on its own storage (a path on
//! desktop, the browser's OPFS through a `SQLite` VFS) and hands it in, so the crate stays free of
//! file system access (ARCH-02) and builds for `wasm32-unknown-unknown` (ARCH-01). It owns what
//! is inside the file: the file format and schema versions (PERS-01), the model's objects,
//! migrations and transactions. The binding choice is recorded in
//! `docs/dev/adr/0006-sqlite-binding.md`, the file layout in
//! `docs/dev/adr/0016-project-file-layout.md`.
//!
//! A host calls [`open_project`] on every connection first, then [`open_model`] to read the
//! model and [`save_model`] to write it. Files from earlier versions are migrated inside those
//! calls (PERS-01): reading migrates in a transaction that is rolled back, so only writing
//! changes the file.
//!
//! Autosave (PERS-02, NFR-06) writes a recovery copy, never the project file: see [`Autosave`]
//! and [`autosave_model`].

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod autosave;
mod codec;
mod layout;
mod migrate;
mod model;
#[cfg(test)]
mod test_support;

pub use autosave::{Autosave, AutosaveInterval, IntervalOutOfRange, autosave_model, autosaved_at};
use core_model::StoreError;
pub use model::{open_model, save_model};
pub use rusqlite::{Connection, Transaction};
use rusqlite::{ErrorCode, OptionalExtension};
use schema::{Migration, Namespace};

/// File format version written by this build: the layout of the tables the file keeps about
/// itself (`project_property`, `schema_namespace`). [`open_project`] refuses files with a
/// higher one. The model's own tables follow the schema version of their namespace instead.
pub const FORMAT_VERSION: u32 = 1;

/// Brings a file's own tables from one format version to the next.
type FormatMigration = fn(&Transaction<'_>) -> Result<(), PersistError>;

/// `FORMAT_MIGRATIONS[i]` migrates format version `i + 1` to `i + 2`. Changing
/// `create_format` needs a new [`FORMAT_VERSION`] and a migration here; the tests compare the
/// tables with `tests/fixtures/format-NNNN.sql` and open a file of every earlier version.
const FORMAT_MIGRATIONS: [FormatMigration; 0] = [];

/// Custom migration code (a `custom` step of a schema migration, ADR 0002), run in the
/// transaction that migrates the file.
pub type CustomStep = fn(&Transaction<'_>) -> Result<(), PersistError>;

/// A namespace, and what brings files written by its earlier versions up to it (PERS-01).
#[derive(Clone, Copy, Debug)]
pub struct Schema<'a> {
    /// The current version of the namespace.
    pub namespace: &'a Namespace,
    /// The migration to each version after the first, as `schema_gen::History` resolves them.
    pub migrations: &'a [Migration],
    /// The functions `custom` steps name.
    pub custom_steps: &'a [(&'a str, CustomStep)],
}

impl<'a> Schema<'a> {
    /// A namespace without history: files must hold its current version.
    #[must_use]
    pub const fn new(namespace: &'a Namespace) -> Self {
        Self {
            namespace,
            migrations: &[],
            custom_steps: &[],
        }
    }
}

/// Marks a database as a `BIMCraft` project in the file header ("BIMC" in ASCII), so other
/// `SQLite` files are refused instead of being modified.
const APPLICATION_ID: i32 = 0x4249_4D43;

/// Why a project file could not be opened or written.
///
/// Messages never carry file paths or model content (SEC-03).
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// The database belongs to another application.
    #[error("the file is not a BIMCraft project")]
    NotAProject,
    /// The file was saved by a newer version of the application.
    #[error("the project uses file format {found}; this version reads up to {supported}")]
    NewerVersion {
        /// File format version stored in the file.
        found: u32,
        /// Highest file format version this build reads.
        supported: u32,
    },
    /// The file holds a schema namespace in a version newer than this build knows.
    #[error(
        "the project uses version {found} of the \"{namespace}\" schema; this version reads up to {supported}"
    )]
    NewerSchema {
        /// The namespace, `core` or `plugin:<plugin-id>`.
        namespace: String,
        /// Version stored in the file.
        found: u32,
        /// Version this build knows.
        supported: u32,
    },
    /// The file is damaged: `SQLite` found it malformed, or its tables or values do not have
    /// the shape this build writes.
    #[error("the project file is damaged")]
    Corrupt,
    /// The storage cannot keep a rollback journal, so a crash during a save could damage the
    /// file (PERS-02).
    #[error("the project storage cannot guarantee safe saves")]
    NotDurable,
    /// No migration leads to a version the file must pass through.
    #[error("no migration of the \"{namespace}\" schema to version {version}")]
    MissingMigration {
        /// The namespace.
        namespace: String,
        /// The version without a migration.
        version: u32,
    },
    /// A migration names custom code this build does not provide.
    #[error("the migration step \"{0}\" is not available")]
    UnknownCustomStep(String),
    /// The objects in the file break the rules of the schema, or the schema itself is invalid.
    #[error("the project does not fit the schema")]
    Model(#[from] StoreError),
    /// `SQLite` reported an error; the source keeps its code for diagnostics.
    #[error("the project database failed")]
    Database(#[source] rusqlite::Error),
}

impl From<rusqlite::Error> for PersistError {
    /// Keeps the two `SQLite` failures a user can act on apart from the others.
    fn from(error: rusqlite::Error) -> Self {
        match error.sqlite_error_code() {
            Some(ErrorCode::NotADatabase) => Self::NotAProject,
            Some(ErrorCode::DatabaseCorrupt) => Self::Corrupt,
            _ => Self::Database(error),
        }
    }
}

/// Prepares a connection and its database for use as a project file.
///
/// The connection is made crash-safe first (PERS-02): a rollback journal and full syncs, so a
/// transaction either reaches the disk whole or not at all. An empty database then becomes a
/// new project, in one transaction, so an interrupted call leaves the file as it was. A file of
/// an earlier format is accepted unchanged: it is migrated when it is read or written.
///
/// # Errors
///
/// [`PersistError::NotDurable`] when the storage refuses a rollback journal,
/// [`PersistError::NotAProject`] for a database of another application,
/// [`PersistError::NewerVersion`] for a file this build cannot read, and
/// [`PersistError::Database`] when `SQLite` fails.
pub fn open_project(conn: &mut Connection) -> Result<(), PersistError> {
    // Checked before the journal mode changes, which rewrites the file header: a file of
    // another application, or of a newer version, is refused untouched.
    let found = stored_version(&begin(conn)?)?;
    check_format(found)?;
    make_durable(conn)?;
    if found == 0 {
        let tx = begin(conn)?;
        // Checked again inside the transaction that writes.
        if stored_version(&tx)? == 0 {
            create_format(&tx)?;
        }
        tx.commit()?;
    }
    Ok(())
}

/// Starts a transaction on `conn`.
///
/// An interrupt (`sqlite3_interrupt`, a progress handler) can stop `BEGIN` after it took effect:
/// the call fails but the connection is left inside a transaction that nothing would end. It is
/// rolled back here, so the connection stays usable.
fn begin(conn: &Connection) -> Result<Transaction<'_>, PersistError> {
    conn.unchecked_transaction().map_err(|error| {
        if !conn.is_autocommit() {
            // Best effort: the original error is the one worth reporting.
            let _ = conn.execute_batch("ROLLBACK");
        }
        error.into()
    })
}

/// Sets the connection up so that a crash at any moment leaves the last committed state.
///
/// - `journal_mode = DELETE`: `SQLite` copies every page it is about to change into a rollback
///   journal before touching the file, and the next connection rolls an interrupted
///   transaction back from it. Unlike WAL, it needs no shared memory, so it also works on the
///   browser's OPFS storage, and the project stays one file once closed.
/// - `synchronous = FULL`: the journal reaches the disk before the file changes, and the file
///   before the journal is deleted.
/// - `fullfsync`: on macOS, a sync also flushes the drive's own cache; elsewhere it does
///   nothing.
///
/// An in-memory database keeps its journal in memory; it has no file to protect.
fn make_durable(conn: &Connection) -> Result<(), PersistError> {
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))?;
    let in_memory = conn.path().is_none_or(str::is_empty);
    if !durable(&mode, in_memory) {
        return Err(PersistError::NotDurable);
    }
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "fullfsync", true)?;
    Ok(())
}

/// Whether a connection whose journal mode is now `mode` survives a crash.
fn durable(mode: &str, in_memory: bool) -> bool {
    mode.eq_ignore_ascii_case("delete") || in_memory && mode.eq_ignore_ascii_case("memory")
}

fn check_format(found: u32) -> Result<(), PersistError> {
    if found > FORMAT_VERSION {
        Err(PersistError::NewerVersion {
            found,
            supported: FORMAT_VERSION,
        })
    } else {
        Ok(())
    }
}

/// Brings the file's own tables to [`FORMAT_VERSION`] inside `tx`.
fn upgrade_format(tx: &Transaction<'_>) -> Result<(), PersistError> {
    match stored_version(tx)? {
        // Not prepared by `open_project`: the tables `tx` needs do not exist.
        0 => Err(PersistError::NotAProject),
        found => {
            check_format(found)?;
            run_format_migrations(tx, found, &FORMAT_MIGRATIONS)
        }
    }
}

/// Runs `migrations` from version `from` on, as [`FORMAT_MIGRATIONS`] lists them.
fn run_format_migrations(
    tx: &Transaction<'_>,
    from: u32,
    migrations: &[FormatMigration],
) -> Result<(), PersistError> {
    let start = usize::try_from(from - 1).map_err(|_| PersistError::Corrupt)?;
    let pending = migrations.get(start..).unwrap_or_default();
    if pending.is_empty() {
        return Ok(());
    }
    for migrate in pending {
        migrate(tx)?;
    }
    let to = u32::try_from(migrations.len() + 1).map_err(|_| PersistError::Corrupt)?;
    tx.pragma_update(None, "user_version", to)?;
    Ok(())
}

/// Writes one project property, replacing any previous value.
///
/// # Errors
///
/// [`PersistError::Database`] when `SQLite` fails, for instance on a database not prepared by
/// [`open_project`].
pub fn set_property(conn: &Connection, key: &str, value: &str) -> Result<(), PersistError> {
    let tx = begin(conn)?;
    upgrade_format(&tx)?;
    write_property(&tx, key, value)?;
    tx.commit()?;
    Ok(())
}

/// Writes one project property inside `tx`, whose file format is already up to date.
fn write_property(tx: &Transaction<'_>, key: &str, value: &str) -> Result<(), PersistError> {
    tx.execute(
        "INSERT INTO project_property (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

/// Reads one project property, or `None` when it was never written.
///
/// # Errors
///
/// [`PersistError::Database`] when `SQLite` fails.
pub fn property(conn: &Connection, key: &str) -> Result<Option<String>, PersistError> {
    // Never committed: reading leaves an older file as it is.
    let tx = begin(conn)?;
    upgrade_format(&tx)?;
    let value = tx
        .query_row(
            "SELECT value FROM project_property WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()?;
    Ok(value)
}

/// Reads the schema version, refusing databases that are not ours.
///
/// Version 0 means "no project yet": accepted only for an empty database, so a foreign file that
/// happens to carry our `application_id` is never written to.
fn stored_version(tx: &Transaction<'_>) -> Result<u32, PersistError> {
    let (application_id, user_version): (i32, i32) = tx.query_row(
        "SELECT application_id, user_version FROM pragma_application_id, pragma_user_version",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match (application_id, user_version) {
        (APPLICATION_ID, version @ 1..) => Ok(version.unsigned_abs()),
        (0 | APPLICATION_ID, 0) if is_empty(tx)? => Ok(0),
        _ => Err(PersistError::NotAProject),
    }
}

fn is_empty(tx: &Transaction<'_>) -> Result<bool, PersistError> {
    let empty = tx.query_row(
        "SELECT NOT EXISTS (SELECT 1 FROM sqlite_schema)",
        [],
        |row| row.get(0),
    )?;
    Ok(empty)
}

/// Creates the current format's tables in an empty database.
fn create_format(tx: &Transaction<'_>) -> Result<(), PersistError> {
    tx.execute_batch(
        "CREATE TABLE project_property (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;
         CREATE TABLE schema_namespace (name TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) STRICT;",
    )?;
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", FORMAT_VERSION)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_project() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        open_project(&mut conn).unwrap();
        conn
    }

    #[test]
    fn an_empty_database_becomes_a_project() {
        let conn = new_project();
        let id: i32 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!((id, version), (APPLICATION_ID, FORMAT_VERSION));
    }

    #[test]
    fn opening_a_current_project_again_keeps_its_content() {
        let mut conn = new_project();
        set_property(&conn, "name", "Tower").unwrap();
        open_project(&mut conn).unwrap();
        assert_eq!(property(&conn, "name").unwrap().as_deref(), Some("Tower"));
    }

    #[test]
    fn properties_are_written_replaced_and_read_back() {
        let conn = new_project();
        assert_eq!(property(&conn, "name").unwrap(), None);
        set_property(&conn, "name", "Tower").unwrap();
        set_property(&conn, "name", "Tower B").unwrap();
        assert_eq!(property(&conn, "name").unwrap().as_deref(), Some("Tower B"));
    }

    #[test]
    fn files_from_a_newer_version_are_refused_untouched() {
        let mut conn = new_project();
        conn.pragma_update(None, "user_version", FORMAT_VERSION + 1)
            .unwrap();
        let err = open_project(&mut conn).unwrap_err();
        assert!(matches!(
            err,
            PersistError::NewerVersion { found, supported: FORMAT_VERSION } if found == FORMAT_VERSION + 1
        ));
        assert_eq!(
            err.to_string(),
            format!(
                "the project uses file format {}; this version reads up to {FORMAT_VERSION}",
                FORMAT_VERSION + 1
            )
        );
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, FORMAT_VERSION + 1);
        assert!(matches!(
            property(&conn, "name"),
            Err(PersistError::NewerVersion { .. })
        ));
    }

    #[test]
    fn an_empty_database_with_our_marker_becomes_a_project() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        open_project(&mut conn).unwrap();
    }

    #[test]
    fn databases_of_other_applications_are_refused_and_left_alone() {
        let cases = [
            "PRAGMA application_id = 7;".to_owned(),
            "PRAGMA user_version = 3;".to_owned(),
            "CREATE TABLE invoices (id INTEGER);".to_owned(),
            format!(
                "PRAGMA application_id = {APPLICATION_ID}; CREATE TABLE invoices (id INTEGER);"
            ),
            format!("PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = -1;"),
        ];
        for setup in cases {
            let mut conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(&setup).unwrap();
            let err = open_project(&mut conn).unwrap_err();
            assert!(matches!(err, PersistError::NotAProject), "{setup}");
            assert_eq!(err.to_string(), "the file is not a BIMCraft project");
            let tables: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name = 'project_property'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(tables, 0, "{setup}");
        }
    }

    #[test]
    fn database_errors_hide_the_sqlite_message() {
        let conn = new_project();
        conn.execute_batch("DROP TABLE project_property").unwrap();
        let err = set_property(&conn, "name", "Tower").unwrap_err();
        assert!(matches!(err, PersistError::Database(_)));
        assert_eq!(err.to_string(), "the project database failed");
    }

    #[test]
    fn databases_not_prepared_as_projects_are_refused() {
        let conn = Connection::open_in_memory().unwrap();
        assert!(matches!(
            set_property(&conn, "name", "Tower"),
            Err(PersistError::NotAProject)
        ));
        assert!(matches!(
            property(&conn, "name"),
            Err(PersistError::NotAProject)
        ));
    }

    /// Regression: an interrupt can stop `BEGIN` after it took effect, which left the
    /// connection inside a transaction and every later save failing.
    #[test]
    fn an_interrupted_begin_leaves_the_connection_usable() {
        let conn = new_project();
        let mut interrupted = 0;
        for steps in 1..=8 {
            let mut fired = false;
            conn.progress_handler(
                steps,
                Some(move || {
                    let interrupt = !fired;
                    fired = true;
                    interrupt
                }),
            )
            .unwrap();
            let started = begin(&conn).map(drop);
            conn.progress_handler(0, None::<fn() -> bool>).unwrap();
            interrupted += usize::from(started.is_err());
            assert!(conn.is_autocommit(), "after {steps} steps");
        }
        assert!(interrupted > 0);
    }

    #[test]
    fn only_a_rollback_journal_is_durable() {
        assert!(durable("delete", false));
        assert!(durable("memory", true));
        for mode in ["wal", "memory", "off", "truncate", "persist"] {
            assert!(!durable(mode, false), "{mode}");
        }
        assert_eq!(
            PersistError::NotDurable.to_string(),
            "the project storage cannot guarantee safe saves"
        );
    }

    #[test]
    fn every_earlier_format_has_a_migration() {
        assert_eq!(
            u32::try_from(FORMAT_MIGRATIONS.len() + 1),
            Ok(FORMAT_VERSION)
        );
    }

    #[test]
    fn format_migrations_run_from_the_stored_version() {
        fn add_a(tx: &Transaction<'_>) -> Result<(), PersistError> {
            tx.execute_batch("CREATE TABLE a (x INTEGER)")?;
            Ok(())
        }
        fn add_b(tx: &Transaction<'_>) -> Result<(), PersistError> {
            tx.execute_batch("CREATE TABLE b (x INTEGER)")?;
            Ok(())
        }
        let tables = |conn: &Connection| -> Vec<String> {
            let mut statement = conn
                .prepare("SELECT name FROM sqlite_schema WHERE name IN ('a', 'b') ORDER BY name")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .collect::<Result<_, _>>()
                .unwrap()
        };
        let migrations: [FormatMigration; 2] = [add_a, add_b];
        for (from, expected) in [(1, vec!["a", "b"]), (2, vec!["b"]), (3, vec![])] {
            let mut conn = new_project();
            let tx = conn.transaction().unwrap();
            run_format_migrations(&tx, from, &migrations).unwrap();
            tx.commit().unwrap();
            assert_eq!(tables(&conn), expected, "from {from}");
            let version: u32 = conn
                .pragma_query_value(None, "user_version", |r| r.get(0))
                .unwrap();
            let expected_version = if from == 3 { FORMAT_VERSION } else { 3 };
            assert_eq!(version, expected_version, "from {from}");
        }
    }
}
