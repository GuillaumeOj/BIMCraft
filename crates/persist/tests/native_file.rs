//! Opens, writes and reopens a project file on the native file system, the way the desktop host
//! will (issue #34). Opts out of ARCH-02 to clean up its temporary file.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::PathBuf;

use persist::{Connection, SCHEMA_VERSION, open_project, property, set_property};

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
    assert_eq!(open_project(&mut conn).unwrap(), SCHEMA_VERSION);
    set_property(&conn, "name", "Tower").unwrap();
    conn.close().unwrap();

    let header = std::fs::read(&file.0).unwrap();
    assert!(header.starts_with(b"SQLite format 3\0"));

    let mut conn = Connection::open(&file.0).unwrap();
    assert_eq!(open_project(&mut conn).unwrap(), SCHEMA_VERSION);
    assert_eq!(property(&conn, "name").unwrap().as_deref(), Some("Tower"));
}
