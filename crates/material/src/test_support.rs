//! The core schema the tests store materials and build-ups in.

use std::path::Path;
use std::sync::LazyLock;

use core_model::Model;
use schema::Namespace;

/// The core schema, loaded once for every test.
pub static CORE: LazyLock<Namespace> = LazyLock::new(|| {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schema/core");
    schema_gen::load_namespace(&dir).unwrap()
});

/// An empty model of the core schema.
pub fn model() -> Model {
    Model::new(&CORE).unwrap()
}
