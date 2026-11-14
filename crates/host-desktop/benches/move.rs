//! The core part of moving an element (NFR-03), on a model of the reference size: run with
//! `cargo bench -p host-desktop --bench move`. Method and numbers:
//! `docs/dev/performance-baselines.md`.
//!
//! One iteration is what the host does between the drop and the bytes the viewport receives:
//! decode the `elements.move` message, run the transaction, re-mesh the moved element, update
//! the scene channel, and build and encode the delta. IPC and the frame come on top; the target
//! for this part is under 50 ms.

#![allow(clippy::unwrap_used)]

use std::hint::black_box;

use api::Session;
use api::jsonrpc::Server;
use core_model::{AttributeValue, EntityId, Model};
use criterion::{Criterion, criterion_group, criterion_main};
use host_desktop::{DocumentHost, core_namespace};

/// The reference size: the 10-storey building of the renderer's reference model has about this
/// many elements.
const BLOCKS: u128 = 100_000;

/// Blocks in a grid, in a few sizes so that meshes are shared as they are in a real model.
fn reference_model() -> Model {
    let mut model = Model::new(&core_namespace().unwrap()).unwrap();
    let real = |v: f64| AttributeValue::Real(v);
    for n in 0..BLOCKS {
        let (column, row) = (n % 400, n / 400);
        #[expect(clippy::cast_precision_loss, reason = "a grid index")]
        let (x, y) = (column as f64 * 3.0, row as f64 * 3.0);
        let size = 0.5 + f64::from(u32::try_from(n % 8).unwrap()) * 0.25;
        let values = [
            (
                "location",
                AttributeValue::List(vec![real(x), real(y), real(0.0)]),
            ),
            ("size_x", real(size)),
            ("size_y", real(size)),
            ("size_z", real(3.0)),
        ];
        model.insert(EntityId::new(n + 1), "Block", values).unwrap();
    }
    model
}

fn move_one_block(c: &mut Criterion) {
    let model = reference_model();
    let host = DocumentHost::new(Server::new(Session::new(model)).unwrap());
    let mut seen = Some(scene_wire::decode(&host.scene(None)).unwrap().revision);
    let mut direction = 1.0_f64;
    // The measured path really moves the element and sends one placement: no mesh.
    let probe = host.handle(&request(50_000, 1.0));
    assert_eq!(probe.notifications.len(), 1);
    let delta = scene_wire::decode(&host.scene(seen)).unwrap();
    assert_eq!((delta.elements.len(), delta.meshes.len()), (1, 0));
    seen = Some(delta.revision);
    c.bench_function("move/one_block_in_100k", |b| {
        b.iter(|| {
            direction = -direction;
            let handled = host.handle(&request(50_000, direction));
            assert_eq!(handled.notifications.len(), 1);
            let bytes = host.scene(seen);
            seen = Some(bytes_revision(&bytes));
            black_box(bytes)
        });
    });
}

fn request(block: u128, dx: f64) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"elements.move","params":{{"ids":["{block:032x}"],"translation":[{dx},0.0,0.0]}}}}"#
    )
}

/// The revision of an encoded delta: the `u64` after the magic, the version and the flags.
fn bytes_revision(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[8..16].try_into().unwrap())
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = move_one_block
}
criterion_main!(benches);
