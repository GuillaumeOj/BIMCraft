//! End-to-end generation from the sample schema: snapshot, rustfmt, determinism and inputs.

// Tests read fixtures and run rustfmt, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use proptest::prelude::*;
use schema_gen::{RustOptions, generate_rust, load_dir, parse_sources};

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sample_dir() -> PathBuf {
    crate_dir().join("tests/fixtures/sample")
}

fn options() -> RustOptions {
    RustOptions {
        id_type: "crate::EntityId".into(),
    }
}

fn generate_sample() -> Result<String, Vec<schema_gen::Error>> {
    generate_rust(&sample_dir(), &options()).map(|generated| generated.code)
}

/// The emitted code is reviewed through this file; `UPDATE_SNAPSHOTS=1` rewrites it.
#[test]
fn matches_the_snapshot() {
    let snapshot = crate_dir().join("tests/snapshots/sample.rs");
    let code = generate_sample().unwrap();
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&snapshot, &code).unwrap();
    }
    let expected = std::fs::read_to_string(&snapshot).unwrap();
    assert!(
        code == expected,
        "generated code differs from {}; rerun with UPDATE_SNAPSHOTS=1 and review the diff",
        snapshot.display()
    );
}

#[test]
fn passes_rustfmt() {
    let config = crate_dir().join("../../rustfmt.toml");
    let mut rustfmt = Command::new("rustfmt")
        .args(["--edition", "2024", "--check", "--config-path"])
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rustfmt is part of the pinned toolchain");
    rustfmt
        .stdin
        .take()
        .unwrap()
        .write_all(generate_sample().unwrap().as_bytes())
        .unwrap();
    let output = rustfmt.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "rustfmt would change the generated code:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lists_every_input() {
    let generated = generate_rust(&sample_dir(), &options()).unwrap();
    let relative: Vec<_> = generated
        .inputs
        .iter()
        .map(|path| path.strip_prefix(sample_dir()).unwrap().to_owned())
        .collect();
    let expected = [
        "namespace.toml",
        "entities",
        "entities/beam.toml",
        "entities/opening.toml",
        "entities/recess.toml",
        "entities/storey.toml",
        "entities/wall.toml",
        "relationships",
        "relationships/containment.toml",
        "relationships/voiding.toml",
        "property-sets",
        "property-sets/Pset_WallCommon.toml",
    ];
    assert_eq!(relative, expected.map(PathBuf::from));
}

#[test]
fn same_input_same_output() {
    assert_eq!(generate_sample().unwrap(), generate_sample().unwrap());
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 32,
        rng_seed: prop::test_runner::RngSeed::Fixed(0x5C4E_3A03),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// The order files are given in never changes the result.
    #[test]
    fn file_order_does_not_matter(seed in any::<u64>()) {
        let mut sources = load_dir(&sample_dir()).unwrap().sources;
        let expected = parse_sources(&sources).unwrap();
        // A seeded Fisher–Yates shuffle with a small LCG keeps the permutation reproducible.
        let mut state = seed;
        for index in (1..sources.len()).rev() {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let other = usize::try_from(state >> 33).unwrap() % (index + 1);
            sources.swap(index, other);
        }
        prop_assert_eq!(parse_sources(&sources).unwrap(), expected);
    }
}
