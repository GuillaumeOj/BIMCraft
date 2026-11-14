//! The viewport driving a real renderer, on a GPU (lavapipe in CI): scene deltas, selection, a
//! move preview with ghosts and a frame, without a validation error.

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

use render::{DisplayState, MeshData, Placement, RenderError, Renderer, Theme};
use scene_wire::{Element, Mesh, SceneDelta};
use viewport::{
    Button, ElementId, InputEvent, Intent, Modifiers, Preview, PreviewLine, RendererSink,
    SceneSink, SinkError, Tool, ViewCommand, Viewport, ViewportEvent,
};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const SIZE: [u32; 2] = [96, 64];

fn device() -> (wgpu::Device, wgpu::Queue) {
    match pollster::block_on(render::headless_device()) {
        Ok(device) => device,
        Err(error @ RenderError::NoAdapter { .. }) => panic!(
            "the GPU tests need a GPU adapter, software ones included (lavapipe on Linux: \
             scripts/linux-gpu-deps.sh; WARP on Windows; Metal on macOS): {error}"
        ),
        Err(error) => panic!("the GPU tests could not create a device: {error}"),
    }
}

fn cube(id: u64) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut indices = Vec::new();
    for axis in 0..3 {
        for side in [-0.5_f32, 0.5] {
            let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
            let base = u32::try_from(positions.len()).unwrap();
            for [a, b] in [[-0.5_f32, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]] {
                let mut p = [0.0; 3];
                p[axis] = side;
                p[u] = a;
                p[v] = b;
                positions.push(p);
                let mut n = [0.0; 3];
                n[axis] = side * 2.0;
                normals.push(n);
            }
            indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    Mesh {
        id,
        origin: [0.0; 3],
        positions,
        normals,
        indices,
        bounds_min: [-0.5; 3],
        bounds_max: [0.5; 3],
    }
}

fn element(n: u8, at: [f64; 3]) -> Element {
    let mut id = [0; 16];
    id[0] = n;
    Element {
        id,
        mesh: 7,
        rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        translation: at,
        category: 0,
        discipline: 1,
    }
}

fn id(n: u8) -> ElementId {
    let mut bytes = [0; 16];
    bytes[0] = n;
    ElementId(bytes)
}

fn draw(viewport: &mut Viewport<RendererSink>, device: &wgpu::Device) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport test target"),
        size: wgpu::Extent3d {
            width: SIZE[0],
            height: SIZE[1],
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
    let camera = viewport.camera();
    let size = viewport.size();
    let clear = Theme::light().clear_color();
    viewport
        .sink_mut()
        .renderer_mut()
        .render(&view, size, &camera, clear)
        .unwrap();
}

#[test]
fn the_viewport_drives_a_renderer() {
    let (device, queue) = device();
    let renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
    let mut viewport = Viewport::new(RendererSink::new(renderer), [96.0, 64.0], 1.0);
    viewport.set_theme(Theme::dark());
    let full = SceneDelta {
        full: true,
        revision: 1,
        strings: vec!["Block".to_owned(), "core".to_owned()],
        meshes: vec![cube(7)],
        elements: vec![element(1, [0.0, 0.0, 0.5]), element(2, [3.0, 0.0, 0.5])],
        ..SceneDelta::default()
    };
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    viewport.apply_scene(&scene_wire::encode(&full)).unwrap();
    viewport.command(ViewCommand::ZoomFit);
    // Click the middle of the view, between the cubes: nothing; select through the command.
    let centre = [48.0, 32.0];
    viewport.input(InputEvent::PointerDown {
        pos: centre,
        button: Button::Primary,
        mods: Modifiers::default(),
    });
    viewport.input(InputEvent::PointerUp {
        pos: centre,
        button: Button::Primary,
        mods: Modifiers::default(),
    });
    viewport.command(ViewCommand::Select(vec![id(1)]));
    assert_eq!(viewport.selection(), [id(1)]);
    // A move preview with a ghost, drawn.
    viewport.set_tool(Tool::Move);
    for pos in [[10.0, 40.0], [80.0, 40.0]] {
        viewport.input(InputEvent::PointerMove {
            pos,
            mods: Modifiers::default(),
        });
        viewport.input(InputEvent::PointerDown {
            pos,
            button: Button::Primary,
            mods: Modifiers::default(),
        });
        viewport.input(InputEvent::PointerUp {
            pos,
            button: Button::Primary,
            mods: Modifiers::default(),
        });
    }
    let events = viewport.drain_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, ViewportEvent::Intent(Intent::Move { .. }))),
        "{events:?}"
    );
    assert!(viewport.tick().draw);
    draw(&mut viewport, &device);
    // The moved element comes back moved; another goes with the mesh it alone used.
    let mut moved = SceneDelta {
        revision: 2,
        base: 1,
        meshes: vec![cube(8)],
        elements: vec![element(1, [1.0, 0.0, 0.5])],
        removed_elements: vec![element(2, [0.0; 3]).id],
        ..SceneDelta::default()
    };
    moved.elements[0].mesh = 8;
    viewport.apply_delta(&moved).unwrap();
    let cleanup = SceneDelta {
        revision: 3,
        base: 2,
        removed_meshes: vec![7],
        ..SceneDelta::default()
    };
    viewport.apply_delta(&cleanup).unwrap();
    draw(&mut viewport, &device);
    assert!(pollster::block_on(scope.pop()).is_none());
}

#[test]
fn the_renderer_sink_maps_ids_and_refuses_unknown_ones() {
    let (device, queue) = device();
    let mut sink = RendererSink::new(Renderer::new(&device, &queue, FORMAT).unwrap());
    let unknown = id(9);
    assert_eq!(sink.remove_object(unknown), Err(SinkError::UnknownElement));
    assert_eq!(
        sink.set_display_state(unknown, DisplayState::Selected),
        Err(SinkError::UnknownElement)
    );
    assert_eq!(
        sink.set_placement(unknown, Placement::IDENTITY),
        Err(SinkError::UnknownElement)
    );
    assert_eq!(sink.remove_mesh(3), Err(SinkError::UnknownMesh));
    assert_eq!(
        sink.add_object(unknown, 3, Placement::IDENTITY),
        Err(SinkError::UnknownMesh)
    );
    let mesh = cube(1);
    let data = MeshData {
        origin: mesh.origin,
        positions: mesh.positions,
        normals: mesh.normals,
        indices: mesh.indices,
    };
    sink.add_mesh(1, &data).unwrap();
    // The same key again replaces the mesh.
    sink.add_mesh(1, &data).unwrap();
    sink.add_object(id(1), 1, Placement::IDENTITY).unwrap();
    sink.add_object(id(1), 1, Placement::from_translation([2.0, 0.0, 0.0]))
        .unwrap();
    let mut preview = Preview::default();
    preview.ghosts.push((unknown, [0.0; 3]));
    assert_eq!(sink.set_preview(&preview), Err(SinkError::UnknownElement));
    preview.ghosts = vec![(id(1), [1.0, 0.0, 0.0])];
    preview.lines.push(PreviewLine {
        from: [0.0; 3],
        to: [f64::NAN; 3],
        tone: render::Tone::Preview,
    });
    assert!(matches!(
        sink.set_preview(&preview),
        Err(SinkError::Overlay(_))
    ));
    preview.lines.clear();
    preview.markers.push(([0.0; 3], render::SnapKind::Ground));
    preview.boxes.push(([0.0; 3], [1.0; 3]));
    sink.set_preview(&preview).unwrap();
    sink.set_preview(&Preview::default()).unwrap();
    sink.set_display_state(id(1), DisplayState::Hovered)
        .unwrap();
    sink.remove_object(id(1)).unwrap();
    sink.remove_mesh(1).unwrap();
}
