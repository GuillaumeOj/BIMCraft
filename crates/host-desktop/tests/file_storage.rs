//! `FileStorage` against real files, each test in its own temporary directory.

// Reads the directory back to check what the storage left on disk.
#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::Path;
use std::thread;

use host_desktop::FileStorage;
use platform::testing::now_or_never;
use platform::{Storage, StorageError};
use tempfile::TempDir;

#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn run<T>(future: impl Future<Output = T>) -> T {
    now_or_never(future).unwrap()
}

/// Names of the entries in `dir`, sorted.
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reads_back_the_latest_write_in_new_folders() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path().join("store"));
    let storage: &dyn Storage = &storage;

    run(storage.write("projects/tower/model.bim", b"one")).unwrap();
    run(storage.write("projects/tower/model.bim", b"two")).unwrap();

    assert_eq!(
        run(storage.read("projects/tower/model.bim")),
        Ok(b"two".to_vec())
    );
    let folder = dir.path().join("store").join("projects").join("tower");
    assert_eq!(entries(&folder), ["model.bim"]);
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn removes_values() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path());

    run(storage.write("a", b"")).unwrap();
    assert_eq!(run(storage.read("a")), Ok(Vec::new()));
    run(storage.remove("a")).unwrap();

    assert_eq!(run(storage.read("a")), Err(StorageError::NotFound));
    assert!(entries(dir.path()).is_empty());
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reports_missing_values() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path());
    run(storage.write("value", b"x")).unwrap();

    for key in ["missing", "missing/value", "value/below"] {
        assert_eq!(run(storage.read(key)), Err(StorageError::NotFound), "{key}");
        assert_eq!(
            run(storage.remove(key)),
            Err(StorageError::NotFound),
            "{key}"
        );
    }
}

#[test]
fn concurrent_writes_to_one_key_keep_one_whole_value() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path());
    let values: Vec<Vec<u8>> = (0..8_u8).map(|i| vec![i; 1 << 20]).collect();

    thread::scope(|scope| {
        for value in &values {
            let storage = &storage;
            scope.spawn(move || run(storage.write("model.bim", value)).unwrap());
        }
    });

    let stored = run(storage.read("model.bim")).unwrap();
    assert!(values.contains(&stored));
    assert_eq!(entries(dir.path()), ["model.bim"]);
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn rejects_keys_outside_the_root_without_touching_the_disk() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path().join("store"));

    for result in [
        run(storage.write("../escaped", b"x")),
        run(storage.remove("../escaped")),
        run(storage.read("../escaped")).map(drop),
    ] {
        assert!(matches!(result, Err(StorageError::Other { .. })));
    }
    assert!(entries(dir.path()).is_empty());
}

#[test]
#[cfg(test)] // Lets the helper unwrap, as tests may (clippy.toml).
fn reports_folders_as_failures_and_cleans_up() {
    let dir = TempDir::new().unwrap();
    let storage = FileStorage::new(dir.path());
    run(storage.write("folder/value", b"x")).unwrap();

    let read = run(storage.read("folder"));
    let removed = run(storage.remove("folder"));
    let written = run(storage.write("folder", b"y"));

    for result in [read.map(drop), removed, written] {
        let Err(StorageError::Other { message }) = result else {
            panic!("expected a failure, got {result:?}");
        };
        assert!(!message.contains(&*dir.path().to_string_lossy()));
    }
    assert_eq!(entries(dir.path()), ["folder"]);
    assert_eq!(run(storage.read("folder/value")), Ok(b"x".to_vec()));
}
