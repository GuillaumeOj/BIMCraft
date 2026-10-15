//! Reference documentation (COM-04): the sample pages and the published core pages, both
//! regenerated so that CI fails when a committed page is stale.

// Tests read fixtures and write snapshots, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::Path;

use schema_gen::{ReferencePages, generate_reference};

mod common;

use common::{assert_committed, crate_dir, sample_dir};

fn assert_pages_committed(pages: &ReferencePages, dir: &Path) -> std::io::Result<()> {
    for (name, text) in pages.files() {
        assert_committed(&dir.join(name), text)?;
    }
    Ok(())
}

/// The emitted pages are reviewed through these files.
#[test]
fn matches_the_snapshot() {
    let pages = generate_reference(&sample_dir()).unwrap();
    assert_pages_committed(&pages, &crate_dir().join("tests/snapshots/reference")).unwrap();
}

/// The core pages of the reference book are regenerated, so CI fails when they are stale.
#[test]
fn published_core_reference_is_up_to_date() {
    let pages = generate_reference(&crate_dir().join("../../schema/core")).unwrap();
    assert_pages_committed(&pages, &crate_dir().join("../../docs/reference/schema")).unwrap();
}
