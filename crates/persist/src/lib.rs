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
//! model and [`save_model`] to write it.

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

mod codec;
mod layout;
mod model;
#[cfg(test)]
mod test_support;

use core_model::StoreError;
pub use model::{open_model, save_model};
pub use rusqlite::Connection;
use rusqlite::{ErrorCode, OptionalExtension, Transaction};

/// File format version written by this build: the layout of the tables the file keeps about
/// itself. [`open_project`] refuses files with a higher one. The model's own tables follow the
/// schema version of their namespace instead.
const SCHEMA_VERSION: u32 = 1;

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
    #[error("the project uses schema version {found}; this version reads up to {supported}")]
    NewerVersion {
        /// Schema version stored in the file.
        found: u32,
        /// Highest schema version this build reads.
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

/// Prepares a database for use as a project file.
///
/// An empty database becomes a new project. Everything runs in one transaction, so an interrupted
/// call leaves the file as it was (PERS-02). Migrations from older schema versions (PERS-01) join
/// the `match` below when the schema first changes.
///
/// # Errors
///
/// [`PersistError::NotAProject`] for a database of another application,
/// [`PersistError::NewerVersion`] for a file this build cannot read, and
/// [`PersistError::Database`] when `SQLite` fails.
pub fn open_project(conn: &mut Connection) -> Result<(), PersistError> {
    let tx = conn.transaction()?;
    match stored_version(&tx)? {
        0 => create_schema(&tx)?,
        SCHEMA_VERSION => {}
        found => {
            return Err(PersistError::NewerVersion {
                found,
                supported: SCHEMA_VERSION,
            });
        }
    }
    tx.commit()?;
    Ok(())
}

/// Writes one project property, replacing any previous value.
///
/// # Errors
///
/// [`PersistError::Database`] when `SQLite` fails, for instance on a database not prepared by
/// [`open_project`].
pub fn set_property(conn: &Connection, key: &str, value: &str) -> Result<(), PersistError> {
    conn.execute(
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
    let value = conn
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

/// Creates the current schema in an empty database.
fn create_schema(tx: &Transaction<'_>) -> Result<(), PersistError> {
    tx.execute_batch(
        "CREATE TABLE project_property (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;
         CREATE TABLE schema_namespace (name TEXT PRIMARY KEY NOT NULL, version INTEGER NOT NULL) STRICT;",
    )?;
    tx.pragma_update(None, "application_id", APPLICATION_ID)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
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
        assert_eq!((id, version), (APPLICATION_ID, SCHEMA_VERSION));
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
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        let err = open_project(&mut conn).unwrap_err();
        assert!(matches!(
            err,
            PersistError::NewerVersion { found, supported: SCHEMA_VERSION } if found == SCHEMA_VERSION + 1
        ));
        assert_eq!(
            err.to_string(),
            format!(
                "the project uses schema version {}; this version reads up to {SCHEMA_VERSION}",
                SCHEMA_VERSION + 1
            )
        );
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION + 1);
        assert_eq!(property(&conn, "name").unwrap(), None);
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
        let conn = Connection::open_in_memory().unwrap();
        let err = set_property(&conn, "name", "Tower").unwrap_err();
        assert!(matches!(err, PersistError::Database(_)));
        assert_eq!(err.to_string(), "the project database failed");
    }
}
