//! The starting inputs of the `scene_wire_decode` fuzz target: valid deltas, checked to be up
//! to date (`UPDATE_SNAPSHOTS=1` rewrites them). An integration test, so that it may read files:
//! the library itself never does (ARCH-02).

#![allow(clippy::disallowed_methods)]

use std::path::Path;

use scene_wire::{Element, Mesh, SceneDelta, decode, encode};

fn sample() -> SceneDelta {
    let mesh = |id| Mesh {
        id,
        origin: [10.0, -20.0, 30.0],
        positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        normals: vec![[0.0, 0.0, 1.0]; 3],
        indices: vec![0, 1, 2],
        bounds_min: [0.0, 0.0, 0.0],
        bounds_max: [1.0, 1.0, 0.0],
    };
    let element = |id, mesh| Element {
        id: [id; 16],
        mesh,
        rotation: [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        translation: [1.0, 2.0, 3.0],
        category: 0,
        discipline: 1,
    };
    SceneDelta {
        full: false,
        revision: 7,
        base: 6,
        strings: vec!["Block".into(), "core".into()],
        meshes: vec![mesh(1), mesh(2)],
        removed_meshes: vec![9, 10],
        elements: vec![element(1, 1), element(2, 2)],
        removed_elements: vec![[3; 16]],
    }
}

#[test]
fn the_fuzz_seeds_are_up_to_date() {
    let seeds = [
        ("empty-delta", SceneDelta::default()),
        ("delta", sample()),
        (
            "full-scene",
            SceneDelta {
                full: true,
                base: 0,
                ..sample()
            },
        ),
    ];
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/scene_wire_decode");
    for (name, delta) in seeds {
        let path = dir.join(name);
        let bytes = encode(&delta);
        assert_eq!(decode(&bytes).as_ref(), Ok(&delta), "{name} is valid");
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(
            std::fs::read(&path).ok(),
            Some(bytes),
            "{name}: run with UPDATE_SNAPSHOTS=1"
        );
    }
}
