//! `BIMCraft` project files, stored as `SQLite` databases (specification §5.3).
//!
//! This crate never opens a file: the host opens the [`Connection`] on its own storage (a path on
//! desktop, the browser's OPFS through a `SQLite` VFS) and hands it in, so the crate stays free of
//! file system access (ARCH-02) and builds for `wasm32-unknown-unknown` (ARCH-01). It owns what
//! is inside the file: the schema version (PERS-01), migrations and transactions. The binding
//! choice is recorded in `docs/dev/adr/0005-sqlite-binding.md`.

// ARCH-02: forbid, not deny, so nothing in this crate can opt out. See docs/dev/quality.md.
#![forbid(clippy::disallowed_methods, clippy::disallowed_types)]

pub use rusqlite::Connection;
use rusqlite::{OptionalExtension, Transaction};

/// Schema version written by this build. Files carrying an older version are migrated on open.
pub const SCHEMA_VERSION: u32 = 1;

/// Marks a database as a `BIMCraft` project in the file header ("BIMC" in ASCII), so other
/// `SQLite` files are refused instead of being modified.
pub const APPLICATION_ID: i32 = 0x4249_4D43;

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
    /// `SQLite` reported an error; the source keeps its code for diagnostics.
    #[error("the project database failed")]
    Database(#[from] rusqlite::Error),
}

/// Prepares a database for use as a project file and returns its schema version.
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
pub fn open_project(conn: &mut Connection) -> Result<u32, PersistError> {
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
    Ok(SCHEMA_VERSION)
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
fn stored_version(tx: &Transaction<'_>) -> Result<u32, PersistError> {
    let application_id: i32 = tx.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let user_version: i32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if application_id == APPLICATION_ID {
        return u32::try_from(user_version).map_err(|_| PersistError::NotAProject);
    }
    let has_tables: bool =
        tx.query_row("SELECT EXISTS (SELECT 1 FROM sqlite_schema)", [], |row| {
            row.get(0)
        })?;
    if application_id != 0 || user_version != 0 || has_tables {
        return Err(PersistError::NotAProject);
    }
    Ok(0)
}

/// Creates the current schema in an empty database.
fn create_schema(tx: &Transaction<'_>) -> Result<(), PersistError> {
    tx.execute_batch(
        "CREATE TABLE project_property (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL) STRICT;",
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
        assert_eq!(open_project(&mut conn).unwrap(), SCHEMA_VERSION);
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
        assert_eq!(open_project(&mut conn).unwrap(), SCHEMA_VERSION);
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
    }

    #[test]
    fn a_negative_version_is_not_a_project() {
        let mut conn = new_project();
        conn.pragma_update(None, "user_version", -1).unwrap();
        assert!(matches!(
            open_project(&mut conn),
            Err(PersistError::NotAProject)
        ));
    }

    #[test]
    fn databases_of_other_applications_are_refused_and_left_alone() {
        let cases: [&str; 3] = [
            "PRAGMA application_id = 7;",
            "PRAGMA user_version = 3;",
            "CREATE TABLE invoices (id INTEGER);",
        ];
        for setup in cases {
            let mut conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(setup).unwrap();
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
