//! Frame benchmarks of the renderer on the reference model (VP-02, NFR-02): run with
//! `cargo bench -p render --bench frame`. Method and reference numbers:
//! `docs/dev/performance-baselines.md`.
//!
//! - `prepare/*`: the CPU part of a frame, which `Renderer::prepare_frame` runs alone: frustum
//!   culling through the hierarchy, levels of detail, and the instance records of the objects
//!   drawn, in a full HD view. `prepare/orbit` moves the camera 10° around the building at each
//!   iteration; NFR-02 leaves it 4 ms of the 16.7 ms of a frame at 60 fps.
//! - `update/move_100`: moving 100 objects, which updates the hierarchy.
//! - `gpu/frame`: a whole frame, CPU and GPU, waiting for the GPU to finish. Only on a hardware
//!   adapter: a software one (lavapipe, WARP) measures the CPU running a rasteriser, not a GPU.

// The benchmark reports why it skips a group, on the console like criterion itself.
#![allow(clippy::print_stderr, clippy::unwrap_used, clippy::panic)]

#[path = "../tests/support/reference.rs"]
mod reference;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use render::{ObjectId, Placement, Renderer, headless_device};

/// The colour format of the offscreen target of `gpu/frame`.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Loaded {
    device: wgpu::Device,
    renderer: Renderer,
    objects: Vec<(ObjectId, Placement)>,
}

/// The full reference model in a renderer, or `None` without any GPU adapter.
fn load() -> Option<Loaded> {
    let (device, queue) = match pollster::block_on(headless_device()) {
        Ok(device) => device,
        Err(error) => {
            eprintln!("render benches skipped: {error}");
            return None;
        }
    };
    let mut renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
    let model = reference::building(reference::STOREYS);
    let ids = model.load(&mut renderer);
    let objects = ids
        .into_iter()
        .zip(model.objects.iter().map(|&(_, placement, _)| placement))
        .collect();
    Some(Loaded {
        device,
        renderer,
        objects,
    })
}

fn bench_frame(criterion: &mut Criterion) {
    let Some(Loaded {
        device,
        mut renderer,
        objects,
    }) = load()
    else {
        return;
    };
    let mut group = criterion.benchmark_group("prepare");
    let mut step = 0;
    group.bench_function("orbit", |bench| {
        bench.iter(|| {
            step = (step + 1) % 36;
            black_box(renderer.prepare_frame(&reference::orbit(step, 36), reference::VIEW))
        });
    });
    for (name, camera) in [
        ("aerial", reference::aerial()),
        ("inside", reference::inside()),
    ] {
        group.bench_function(name, |bench| {
            bench.iter(|| black_box(renderer.prepare_frame(&camera, reference::VIEW)));
        });
    }
    group.finish();

    let mut group = criterion.benchmark_group("update");
    let mut shift = 0.0;
    group.bench_function("move_100", |bench| {
        bench.iter(|| {
            shift = 0.1 - shift;
            for &(id, placement) in objects.iter().step_by(objects.len() / 100) {
                let mut moved = placement;
                moved.translation[0] += shift;
                renderer.set_placement(id, moved).unwrap();
            }
        });
    });
    group.finish();

    let adapter = device.adapter_info();
    if adapter.device_type == wgpu::DeviceType::Cpu {
        eprintln!(
            "gpu/frame skipped: {} is a software adapter, whose frame time is CPU time",
            adapter.name
        );
        return;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bench target"),
        size: wgpu::Extent3d {
            width: reference::VIEW[0],
            height: reference::VIEW[1],
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut group = criterion.benchmark_group("gpu");
    group.bench_function("frame", |bench| {
        bench.iter(|| {
            step = (step + 1) % 36;
            let camera = reference::orbit(step, 36);
            renderer
                .render(&view, reference::VIEW, &camera, [1.0; 4])
                .unwrap();
            device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_frame);
criterion_main!(benches);
