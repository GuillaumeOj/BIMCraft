//! Version history (PERS-01, ADR 0002): the core namespace's snapshots and migrations must match
//! its definitions, so a schema change without a new version or a migration fails CI.

// Tests read namespace directories and write snapshots, which the ARCH-02 lints would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};

use schema_gen::{Source, load_history, parse_sources, snapshot, snapshot_path};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn messages(errors: &[schema_gen::Error]) -> String {
    let messages: Vec<_> = errors.iter().map(ToString::to_string).collect();
    messages.join("\n")
}

/// The core namespace's history is complete and consistent.
///
/// With `UPDATE_SNAPSHOTS=1`, the snapshot of the current version is written first when it is
/// missing. A snapshot that exists is never rewritten: released versions are immutable, so
/// changing the definitions needs a new version.
#[test]
fn the_core_history_matches_the_definitions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        write_missing_snapshot(&dir).unwrap();
    }
    let history = load_history(&dir).unwrap_or_else(|errors| panic!("{}", messages(&errors)));
    assert_eq!(
        history.migrations.len() + 1,
        usize::try_from(history.namespace.version).unwrap()
    );
}

fn write_missing_snapshot(dir: &Path) -> Result<()> {
    let sources = schema_gen::load_dir(dir)?;
    let namespace = parse_sources(&sources).map_err(|errors| messages(&errors))?;
    let path = dir.join(snapshot_path(namespace.version));
    if !path.exists() {
        std::fs::create_dir_all(dir.join("snapshots"))?;
        std::fs::write(path, snapshot(&namespace)?)?;
    }
    Ok(())
}

/// A namespace directory under the system temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str, files: &[(String, String)]) -> std::io::Result<Self> {
        let dir =
            std::env::temp_dir().join(format!("schema-gen-history-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let namespace = Self(dir);
        for (path, contents) in files {
            let path = namespace.0.join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, contents)?;
        }
        Ok(namespace)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn namespace_file(version: u32) -> String {
    format!("format = 1\n[namespace]\nname = \"core\"\nversion = {version}\n")
}

fn storey_file(attribute: &str) -> String {
    format!(
        "format = 1\n[entity]\nname = \"Storey\"\nkind = \"spatial\"\n[ifc]\nentity = \"IfcBuildingStorey\"\n[[attribute]]\nname = \"{attribute}\"\nvalue = \"text\"\n"
    )
}

/// The snapshot of a version whose storey has `attribute`.
fn snapshot_file(version: u32, attribute: &str) -> Result<(String, String)> {
    let sources = [
        Source {
            path: "namespace.toml".into(),
            contents: namespace_file(version),
        },
        Source {
            path: "entities/storey.toml".into(),
            contents: storey_file(attribute),
        },
    ];
    let namespace = parse_sources(&sources).map_err(|errors| messages(&errors))?;
    Ok((snapshot_path(version), snapshot(&namespace)?))
}

fn files(extra: &[(&str, &str)]) -> Result<Vec<(String, String)>> {
    let mut files = vec![
        ("namespace.toml".to_owned(), namespace_file(2)),
        ("entities/storey.toml".to_owned(), storey_file("name")),
        snapshot_file(1, "label")?,
        snapshot_file(2, "name")?,
        (
            "migrations/0002.toml".to_owned(),
            "format = 1\n[[step]]\nop = \"rename_attribute\"\nentity = \"Storey\"\nold_name = \"label\"\nnew_name = \"name\"\n".to_owned(),
        ),
    ];
    files.extend(
        extra
            .iter()
            .map(|(path, contents)| ((*path).to_owned(), (*contents).to_owned())),
    );
    Ok(files)
}

#[test]
fn a_namespace_directory_loads_with_its_history() {
    let dir = TempDir::new("complete", &files(&[]).unwrap()).unwrap();
    let history = load_history(&dir.0).unwrap();
    assert_eq!(history.namespace.version, 2);
    assert_eq!(history.migrations.len(), 1);
}

#[test]
fn history_directories_hold_only_their_files() {
    for (stray, expected) in [
        (
            "snapshots/notes.md",
            "snapshots/notes.md: snapshots/ may hold only *.json files",
        ),
        (
            "migrations/0002.json",
            "migrations/0002.json: migrations/ may hold only *.toml files",
        ),
    ] {
        let dir = TempDir::new("stray", &files(&[(stray, "")]).unwrap()).unwrap();
        let errors = load_history(&dir.0).unwrap_err();
        assert_eq!(errors[0].to_string(), expected);
    }
    let dir = TempDir::new("broken", &files(&[("entities/storey.toml", "")]).unwrap()).unwrap();
    assert!(load_history(&dir.0).is_err());
}
