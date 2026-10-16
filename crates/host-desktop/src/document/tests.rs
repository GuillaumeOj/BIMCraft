use std::collections::BTreeSet;
use std::path::Path;

use super::*;

/// Every file under `dir`, as a path relative to `root` with `/` separators.
fn files(root: &Path, dir: &Path, found: &mut BTreeSet<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(root, &path, found);
        } else {
            let relative = path.strip_prefix(root).unwrap();
            found.insert(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn the_embedded_core_schema_lists_every_definition_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    let mut found = BTreeSet::new();
    files(&root, &root, &mut found);
    // Released snapshots and the generated JSON Schema are not definitions.
    found.retain(|path| {
        Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "toml")
    });
    let embedded: BTreeSet<String> = CORE_SCHEMA
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    assert_eq!(embedded, found, "update CORE_SCHEMA in document.rs");
}

#[test]
fn a_new_document_answers_from_the_window() {
    let host = DocumentHost::open_new().unwrap();
    let handled = host.handle(r#"{"jsonrpc":"2.0","id":7,"method":"history.state","params":{}}"#);
    let response: serde_json::Value = serde_json::from_str(&handled.response.unwrap()).unwrap();
    assert_eq!(response["result"]["can_undo"], false);
    assert!(handled.notifications.is_empty());
}

#[test]
fn a_poisoned_lock_does_not_stop_the_document() {
    let host = DocumentHost::open_new().unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = host.server.lock().unwrap();
        panic!("poison");
    }));
    assert!(host.server.is_poisoned());
    let handled = host.handle(r#"{"jsonrpc":"2.0","id":1,"method":"history.state","params":{}}"#);
    assert!(handled.response.is_some());
}

#[test]
fn a_broken_schema_is_reported() {
    let error = OpenError::Schema(Vec::new());
    assert_eq!(error.to_string(), "the built-in core schema is invalid: []");
}
