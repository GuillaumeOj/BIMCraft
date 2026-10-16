use std::path::Path;

use schema_gen::load_dir;

use super::*;

#[test]
fn the_embedded_core_schema_is_the_schema_core_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    let on_disk: Vec<(String, String)> = load_dir(&root)
        .unwrap()
        .into_iter()
        .map(|source| (source.path, source.contents))
        .collect();
    let embedded: Vec<(String, String)> = CORE_SCHEMA
        .iter()
        .map(|(path, contents)| ((*path).to_owned(), (*contents).to_owned()))
        .collect();
    assert_eq!(embedded, on_disk, "update CORE_SCHEMA in document.rs");
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
