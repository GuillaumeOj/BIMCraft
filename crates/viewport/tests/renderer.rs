//! The viewport driving a real renderer, on a GPU (lavapipe in CI): scene deltas, selection, a
//! move preview with ghosts and a frame, without a validation error.

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

#[path = "../../render/tests/support/mod.rs"]
#[allow(
    dead_code,
    reason = "the renderer's GPU test support: its box meshes serve its own tests"
)]
mod support;

use std::collections::BTreeMap;

use render::{DisplayState, MeshData, Placement, Renderer, Theme};
use scene_wire::{Element, Mesh, SceneDelta};
use support::{FORMAT, Target, device};
use viewport::{
    Button, ElementId, InputEvent, Intent, Modifiers, Preset, Preview, PreviewLine, ProjectionMode,
    RendererSink, SceneSink, SinkError, Tool, ViewCommand, Viewport, ViewportEvent,
};

const SIZE: [u32; 2] = [96, 64];

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
            // Counterclockwise seen from outside, as geom tessellates.
            let order = if side < 0.0 {
                [0, 2, 1, 0, 3, 2]
            } else {
                [0, 1, 2, 0, 2, 3]
            };
            indices.extend(order.map(|corner| base + corner));
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
    // The moved element comes back moved on a new mesh; another goes; the old mesh, which no
    // object places any more, goes in the same delta.
    let mut moved = SceneDelta {
        revision: 2,
        base: 1,
        meshes: vec![cube(8)],
        elements: vec![element(1, [1.0, 0.0, 0.5])],
        removed_elements: vec![element(2, [0.0; 3]).id],
        removed_meshes: vec![7],
        ..SceneDelta::default()
    };
    moved.elements[0].mesh = 8;
    viewport.apply_delta(&moved).unwrap();
    assert!(
        viewport
            .drain_events()
            .iter()
            .all(|event| !matches!(event, ViewportEvent::Error(_)))
    );
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

/// The full scene of the e2e tests (`ui/e2e/fixtures/README.md`): blocks 1 and 2 are 4 × 2 × 3 m
/// at (0, 0, 0) and (6, 0, 0), block 3 is 2 × 2 × 6 m at (0, 6, 0).
const BLOCKS: &[u8] = include_bytes!("../../../ui/e2e/fixtures/scene-blocks.bin");

/// The target of the picking test.
const PICK_SIZE: [u32; 2] = [192, 128];

/// Renders the viewport's view into `target` and reads its pixels back, row by row from the
/// top left.
fn frame(
    viewport: &mut Viewport<RendererSink>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &Target,
) -> Vec<[u8; 4]> {
    let camera = viewport.camera();
    let clear = Theme::light().clear_color();
    viewport
        .sink_mut()
        .renderer_mut()
        .render(&target.view, target.size, &camera, clear)
        .unwrap();
    target.read(device, queue).as_chunks::<4>().0.to_vec()
}

/// Clicks the centre of pixel `[x, y]` and leaves the view, so no hover tints the next frame.
fn click_pixel(viewport: &mut Viewport<RendererSink>, [x, y]: [u32; 2]) {
    let pos = [f64::from(x) + 0.5, f64::from(y) + 0.5];
    let mods = Modifiers::default();
    viewport.input(InputEvent::PointerMove { pos, mods });
    viewport.input(InputEvent::PointerDown {
        pos,
        button: Button::Primary,
        mods,
    });
    viewport.input(InputEvent::PointerUp {
        pos,
        button: Button::Primary,
        mods,
    });
    viewport.input(InputEvent::PointerLeave);
}

/// In every preset and both projections, after a zoom fit over the e2e blocks, a click selects
/// what is drawn under the pointer: nothing over the background, else the block that turns to
/// the selection colour there once selected, and no other block's pixels change.
#[test]
fn a_click_selects_what_is_drawn_under_it_in_every_view() {
    let (device, queue) = device();
    let renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
    let mut viewport = Viewport::new(RendererSink::new(renderer), PICK_SIZE.map(f64::from), 1.0);
    viewport.set_theme(Theme::light());
    viewport.apply_scene(BLOCKS).unwrap();
    let target = Target::new(&device, PICK_SIZE);
    let [width, height] = PICK_SIZE;
    let at = |pixels: &[[u8; 4]], x: u32, y: u32| pixels[(y * width + x) as usize];
    let presets = [
        Preset::Top,
        Preset::Front,
        Preset::Back,
        Preset::Left,
        Preset::Right,
        Preset::IsoSe,
        Preset::IsoSw,
        Preset::IsoNe,
        Preset::IsoNw,
    ];
    for mode in [ProjectionMode::Perspective, ProjectionMode::Parallel] {
        for preset in presets {
            viewport.command(ViewCommand::Select(Vec::new()));
            viewport.command(ViewCommand::Projection(mode));
            viewport.command(ViewCommand::ViewPreset(preset));
            viewport.command(ViewCommand::ZoomFit);
            let before = frame(&mut viewport, &device, &queue, &target);
            let background = at(&before, 0, 0);
            // The picked pixels of each block, away from every edge of the frame.
            let mut picked: BTreeMap<ElementId, Vec<[u32; 2]>> = BTreeMap::new();
            for y in (4..height - 4).step_by(8) {
                for x in (4..width - 4).step_by(8) {
                    let colour = at(&before, x, y);
                    let flat = [(x - 3, y), (x + 3, y), (x, y - 3), (x, y + 3)]
                        .iter()
                        .all(|&(nx, ny)| at(&before, nx, ny) == colour);
                    if !flat {
                        continue;
                    }
                    click_pixel(&mut viewport, [x, y]);
                    let selection = viewport.selection().to_vec();
                    let context = format!("{mode:?} {preset:?} at {x} {y}");
                    if colour == background {
                        assert!(selection.is_empty(), "{context}: {selection:?}");
                        continue;
                    }
                    let [element] = selection[..] else {
                        panic!("{context}: {selection:?}");
                    };
                    picked.entry(element).or_default().push([x, y]);
                }
            }
            assert!(picked.len() >= 2, "{mode:?} {preset:?}: {picked:?}");
            for element in picked.keys() {
                viewport.command(ViewCommand::Select(vec![*element]));
                let selected = frame(&mut viewport, &device, &queue, &target);
                for (other, others) in &picked {
                    for &[x, y] in others {
                        let changed = at(&selected, x, y) != at(&before, x, y);
                        assert_eq!(
                            changed,
                            other == element,
                            "{mode:?} {preset:?}: {element:?} selected, {other:?} picked at {x} {y}"
                        );
                    }
                }
            }
        }
    }
}
