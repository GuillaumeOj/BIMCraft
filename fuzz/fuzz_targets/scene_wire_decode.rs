//! Feeds arbitrary bytes to the scene delta decoder (VP-01), the parser of what the host sends
//! the viewport. Nothing may panic; an accepted delta must respect every limit and range of the
//! format, and must encode back to the very bytes it was read from.

#![no_main]

use libfuzzer_sys::fuzz_target;
use scene_wire::{MAX_DELTA_BYTES, decode, encode};

fuzz_target!(|data: &[u8]| {
    let Ok(delta) = decode(data) else {
        return;
    };
    assert!(data.len() <= MAX_DELTA_BYTES);
    for mesh in &delta.meshes {
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert!(!mesh.indices.is_empty() && mesh.indices.len() % 3 == 0);
    }
    for element in &delta.elements {
        let strings = u32::try_from(delta.strings.len()).expect("a count under the limit");
        assert!(element.category < strings && element.discipline < strings);
    }
    assert_eq!(encode(&delta), data);
});
