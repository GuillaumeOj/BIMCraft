//! The scene deltas the UI tests and e2e load (`ui/e2e/fixtures/scene-*.bin`) are written from
//! known documents by this test, and it fails when they are stale. `UPDATE_SNAPSHOTS=1`
//! regenerates them. `ui/e2e/fixtures/README.md` says what each holds.

// The test reads and writes the committed files, which the ARCH-02 lints of clippy.toml flag.
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_types,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};

use host_desktop::DocumentHost;
use scene_wire::{SceneDelta, decode};
use serde_json::{Value, json};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/e2e/fixtures")
}

fn call(host: &DocumentHost, method: &str, params: &Value) {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let handled = host.handle(&request.to_string());
    let response: Value = serde_json::from_str(&handled.response.unwrap()).unwrap();
    assert!(response.get("error").is_none(), "{response}");
}

fn block(host: &DocumentHost, n: u32, location: [f64; 3], rotation: f64, size: [f64; 3]) {
    let real = |v: f64| json!({ "Real": v });
    call(
        host,
        "model.create",
        &json!({
            "id": format!("{n:032x}"),
            "entity": "Block",
            "values": [
                ["location", { "List": location.map(real) }],
                ["rotation", real(rotation)],
                ["size_x", real(size[0])],
                ["size_y", real(size[1])],
                ["size_z", real(size[2])],
            ],
        }),
    );
}

/// Writes or checks one fixture.
fn assert_committed(name: &str, bytes: &[u8]) {
    let path = fixtures().join(name);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(fixtures()).unwrap();
        std::fs::write(&path, bytes).unwrap();
    }
    let expected = std::fs::read(&path).unwrap_or_default();
    assert!(
        bytes == expected,
        "{name} is stale; rerun with UPDATE_SNAPSHOTS=1 and review the diff"
    );
}

/// Three blocks: two share a size (and so a mesh), one is turned by a quarter turn.
fn three_blocks() -> DocumentHost {
    let host = DocumentHost::open_new().unwrap();
    block(&host, 1, [0.0, 0.0, 0.0], 0.0, [4.0, 2.0, 3.0]);
    block(&host, 2, [6.0, 0.0, 0.0], 0.0, [4.0, 2.0, 3.0]);
    block(
        &host,
        3,
        [0.0, 6.0, 0.0],
        std::f64::consts::FRAC_PI_2,
        [2.0, 2.0, 6.0],
    );
    host
}

#[test]
fn the_empty_scene_fixture_is_current() {
    let host = DocumentHost::open_new().unwrap();
    assert_committed("scene-empty.bin", &host.scene(None));
}

#[test]
fn the_blocks_fixture_is_current() {
    let bytes = three_blocks().scene(None);
    let delta: SceneDelta = decode(&bytes).unwrap();
    assert_eq!(
        (delta.full, delta.elements.len(), delta.meshes.len()),
        (true, 3, 2)
    );
    assert_committed("scene-blocks.bin", &bytes);
}

#[test]
fn the_move_fixture_is_current() {
    let host = three_blocks();
    let revision = decode(&host.scene(None)).unwrap().revision;
    call(
        &host,
        "elements.move",
        &json!({ "ids": [format!("{:032x}", 2)], "translation": [0.0, 3.0, 0.0] }),
    );
    let bytes = host.scene(Some(revision));
    let delta = decode(&bytes).unwrap();
    assert_eq!(
        (delta.full, delta.base, delta.revision),
        (false, revision, revision + 1)
    );
    assert_eq!((delta.elements.len(), delta.meshes.len()), (1, 0));
    assert_committed("scene-move.bin", &bytes);
}
