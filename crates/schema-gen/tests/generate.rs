//! End-to-end generation from the sample schema: snapshot, rustfmt, determinism and inputs.

// Tests read fixtures and run rustfmt, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::io::Write as _;
use std::process::{Command, Stdio};

use proptest::prelude::*;
use schema_gen::{RustOptions, generate_rust, load_dir, parse_sources};

mod common;

use common::{assert_committed, crate_dir, sample_dir};

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
    let code = generate_sample().unwrap();
    assert_committed(&crate_dir().join("tests/snapshots/sample.rs"), &code).unwrap();
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
fn reruns_on_any_change_in_the_namespace() {
    let generated = generate_rust(&sample_dir(), &options()).unwrap();
    assert_eq!(generated.inputs, [sample_dir()]);
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
        let mut sources = load_dir(&sample_dir()).unwrap();
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
