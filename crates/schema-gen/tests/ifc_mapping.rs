//! IFC mapping table (CORE-10): the sample snapshot and the published core table. A missing
//! `[ifc]` section fails validation before any table is written (`errors.rs`).

// Tests read fixtures and write snapshots, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use schema_gen::generate_ifc_mapping;

mod common;

use common::{assert_committed, crate_dir, sample_dir};

/// The sample covers predefined types, `USERDEFINED` with an object type, derived and explicit
/// type entities, and relationships without a predefined type.
#[test]
fn matches_the_snapshot() {
    let generated = generate_ifc_mapping(&sample_dir()).unwrap();
    assert_eq!(generated.inputs, [sample_dir()]);
    assert_committed(
        &crate_dir().join("tests/snapshots/sample.ifc-mapping.md"),
        &generated.code,
    )
    .unwrap();
}

/// The published table of the core namespace is regenerated, so CI fails when it is stale.
#[test]
fn published_core_table_is_up_to_date() {
    let generated = generate_ifc_mapping(&crate_dir().join("../../schema/core")).unwrap();
    assert_committed(
        &crate_dir().join("../../docs/dev/ifc-mapping.md"),
        &generated.code,
    )
    .unwrap();
}
