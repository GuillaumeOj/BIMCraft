//! Fixture paths and the snapshot rule shared by the generation tests.

use std::path::{Path, PathBuf};

pub fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub fn sample_dir() -> PathBuf {
    crate_dir().join("tests/fixtures/sample")
}

/// Compares generated `text` with the committed file at `path`, so a stale file fails CI;
/// `UPDATE_SNAPSHOTS=1` rewrites the file first, for review in the diff.
pub fn assert_committed(path: &Path, text: &str) -> std::io::Result<()> {
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(path, text)?;
    }
    let expected = std::fs::read_to_string(path)?;
    assert!(
        text == expected,
        "generated output differs from {}; rerun with UPDATE_SNAPSHOTS=1 and review the diff",
        path.display()
    );
    Ok(())
}
