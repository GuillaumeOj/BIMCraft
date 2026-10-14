//! End-to-end generation from the sample schema: snapshot, rustfmt, determinism and inputs.

// Tests read fixtures and run rustfmt, which the ARCH-02 lints of clippy.toml would flag.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use express::{ErrorKind, RustOptions, generate_rust};

fn crate_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn sample() -> PathBuf {
    crate_dir().join("tests/fixtures/sample.exp")
}

/// The runtime the snapshot is compiled against, in `interop-ifc`'s tests.
fn options() -> RustOptions {
    RustOptions {
        runtime: "interop_ifc::ifc::decode".into(),
        doc_url: Some("https://example.org/lexical/{name}.htm".into()),
    }
}

fn generate_sample() -> Result<String, Vec<express::Error>> {
    generate_rust(&sample(), &options()).map(|generated| generated.code)
}

/// The emitted code, formatted by rustfmt, as it is committed and reviewed.
fn formatted_sample() -> std::io::Result<String> {
    let code = generate_sample()
        .map_err(|errors| std::io::Error::other(format!("generation failed: {errors:?}")))?;
    let config = crate_dir().join("../../rustfmt.toml");
    let mut rustfmt = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--config-path"])
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = rustfmt.stdin.take() {
        stdin.write_all(code.as_bytes())?;
    }
    let output = rustfmt.wait_with_output()?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(std::io::Error::other(message));
    }
    String::from_utf8(output.stdout).map_err(std::io::Error::other)
}

/// The emitted code is reviewed through this file, formatted so it passes the formatting checks;
/// `UPDATE_SNAPSHOTS=1` rewrites it.
#[test]
fn matches_the_snapshot() {
    let snapshot = crate_dir().join("tests/snapshots/sample.rs");
    let code = formatted_sample().unwrap();
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
fn reruns_when_the_schema_changes() {
    assert_eq!(
        generate_rust(&sample(), &options()).unwrap().inputs,
        [sample()]
    );
}

#[test]
fn same_input_same_output() {
    assert_eq!(generate_sample(), generate_sample());
}

#[test]
fn reports_unreadable_and_invalid_schemas() {
    let missing = crate_dir().join("tests/fixtures/missing.exp");
    let errors = generate_rust(&missing, &options()).unwrap_err();
    assert!(matches!(&errors[0].kind, ErrorKind::Io(message) if message.contains("missing.exp")));

    let dir = std::env::temp_dir().join(format!("express-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        ("syntax.exp", "SCHEMA s; ENTITY END_SCHEMA;"),
        (
            "resolve.exp",
            "SCHEMA s; TYPE t = Nope; END_TYPE; END_SCHEMA;",
        ),
        (
            "rust.exp",
            "SCHEMA s; TYPE Vec = REAL; END_TYPE; END_SCHEMA;",
        ),
    ];
    for (name, source) in cases {
        let path = dir.join(name);
        std::fs::write(&path, source).unwrap();
        assert_eq!(
            generate_rust(&path, &options()).unwrap_err().len(),
            1,
            "{name}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
