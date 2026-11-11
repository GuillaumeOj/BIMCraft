//! The renderer on a GPU: what it draws, and how it rejects bad input (VP-04).

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

mod support;

use geom::{Point3, Vector3};
use render::{
    Camera, CameraError, MeshData, MeshError, MeshId, Placement, PlacementError, Projection,
    RenderError, Renderer,
};
use support::{Boxes, FORMAT, Target};

/// Side of the square frames, in pixels.
const SIZE: u32 = 64;

/// The clear colour, and the bytes it becomes in the target.
const CLEAR: [f64; 4] = [0.0, 0.2, 1.0, 1.0];
const CLEAR_BYTES: [u8; 4] = [0, 51, 255, 255];

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

/// Five metres in front of the origin, looking along +y, 60° field of view: the half-width of
/// the view at the origin is 5·tan 30° ≈ 2.89 m.
fn camera(projection: Projection) -> Camera {
    Camera {
        eye: Point3::new(0.0, -5.0, 0.0),
        target: Point3::new(0.0, 0.0, 0.0),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection,
    }
}

const PERSPECTIVE: Projection = Projection::Perspective {
    fov_y_radians: core::f64::consts::FRAC_PI_3,
    near: 0.1,
};

/// A box of side `side` centred on the origin.
fn cube(side: f64) -> MeshData {
    let mut boxes = Boxes::default();
    boxes.add([-side / 2.0; 3], [side / 2.0; 3]);
    boxes.mesh()
}

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    target: Target,
}

impl Fixture {
    fn new() -> Self {
        let (device, queue) = support::device();
        let renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
        let target = Target::new(&device, [SIZE; 2]);
        Self {
            device,
            queue,
            renderer,
            target,
        }
    }

    fn add_cube(&mut self, side: f64, at: [f64; 3], color: [f32; 4]) -> render::ObjectId {
        let mesh = self.renderer.add_mesh(&cube(side)).unwrap();
        self.renderer
            .add_object(mesh, Placement::from_translation(at), color)
            .unwrap()
    }

    /// Renders through `camera` and reads the frame back.
    fn frame_through(&mut self, camera: &Camera) -> Frame {
        self.renderer
            .render(&self.target.view, self.target.size, camera, CLEAR)
            .unwrap();
        Frame(self.target.read(&self.device, &self.queue))
    }

    fn frame(&mut self) -> Frame {
        self.frame_through(&camera(PERSPECTIVE))
    }
}

struct Frame(Vec<u8>);

impl Frame {
    /// The pixel at column `x` and row `y`, from the top left.
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * SIZE + x) * 4) as usize;
        self.0[start..start + 4].try_into().unwrap()
    }

    fn centre(&self) -> [u8; 4] {
        self.pixel(SIZE / 2, SIZE / 2)
    }

    fn is_clear(&self) -> bool {
        self.0
            .as_chunks::<4>()
            .0
            .iter()
            .all(|&pixel| pixel == CLEAR_BYTES)
    }
}

/// The column where the world `x` of the plane `y = 0` appears, for [`camera`].
fn column(x: f64) -> u32 {
    let half_width = 5.0 * libm::tan(core::f64::consts::FRAC_PI_6);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a column of the 64-pixel frame"
    )]
    let column = (f64::from(SIZE) / 2.0 * (1.0 + x / half_width)) as u32;
    column
}

#[test]
fn an_empty_scene_is_the_clear_colour() {
    assert!(Fixture::new().frame().is_clear());
}

#[test]
fn a_cube_is_drawn_in_its_colour_with_lambert_shading() {
    let mut fixture = Fixture::new();
    fixture.add_cube(1.0, [0.0; 3], RED);
    let frame = fixture.frame();
    let [red, green, blue, alpha] = frame.centre();
    // The face towards the camera gets ambient 0.25 plus 0.75 × 0.80 of diffuse light.
    assert!((215..=219).contains(&red), "{red}");
    assert_eq!([green, blue, alpha], [0, 0, 255]);
    assert_eq!(frame.pixel(0, 0), CLEAR_BYTES);
}

#[test]
fn nearer_objects_hide_farther_ones_whatever_the_order() {
    for near_first in [true, false] {
        let mut fixture = Fixture::new();
        let add = |fixture: &mut Fixture, near: bool| {
            if near {
                fixture.add_cube(1.0, [0.0; 3], RED);
            } else {
                fixture.add_cube(2.0, [0.0, 3.0, 0.0], GREEN);
            }
        };
        add(&mut fixture, near_first);
        add(&mut fixture, !near_first);
        let frame = fixture.frame();
        assert!(frame.centre()[0] > 200, "near first: {near_first}");
        // Beside the red cube, the larger green one shows.
        assert!(frame.pixel(column(0.7), SIZE / 2)[1] > 50);
    }
}

#[test]
fn back_faces_are_drawn() {
    let mut fixture = Fixture::new();
    let triangle = MeshData::from_world(
        &[
            Point3::new(-1.0, 0.0, -1.0),
            Point3::new(1.0, 0.0, -1.0),
            Point3::new(0.0, 0.0, 1.0),
        ],
        vec![[0.0, -1.0, 0.0]; 3],
        vec![0, 1, 2],
    )
    .unwrap();
    let mesh = fixture.renderer.add_mesh(&triangle).unwrap();
    fixture
        .renderer
        .add_object(mesh, Placement::IDENTITY, RED)
        .unwrap();
    for eye_y in [-5.0, 5.0] {
        let mut camera = camera(PERSPECTIVE);
        camera.eye = Point3::new(0.0, eye_y, 0.0);
        assert!(fixture.frame_through(&camera).centre()[0] > 200, "{eye_y}");
    }
}

#[test]
fn an_orthographic_camera_draws_too() {
    let mut fixture = Fixture::new();
    fixture.add_cube(1.0, [0.0; 3], RED);
    let frame = fixture.frame_through(&camera(Projection::Orthographic {
        height: 4.0,
        near: 0.1,
        far: 10.0,
    }));
    assert!(frame.centre()[0] > 200);
    // Without perspective, the 1 m cube spans a quarter of the 4 m view: 16 pixels.
    assert!(frame.pixel(SIZE / 2 + 7, SIZE / 2)[0] > 200);
    assert_eq!(frame.pixel(SIZE / 2 + 9, SIZE / 2), CLEAR_BYTES);
}

#[test]
fn objects_can_be_added_moved_and_removed() {
    let mut fixture = Fixture::new();
    // More objects than the first uniform buffers hold, so they grow.
    let xs = [-2.0, -1.0, 0.0, 1.0, 2.0];
    let objects: Vec<_> = xs
        .iter()
        .map(|&x| fixture.add_cube(0.5, [x, 0.0, 0.0], RED))
        .collect();
    let frame = fixture.frame();
    for &x in &xs {
        assert!(frame.pixel(column(x), SIZE / 2)[0] > 200, "{x}");
    }

    fixture
        .renderer
        .set_placement(objects[2], Placement::from_translation([0.0, 0.0, 2.0]))
        .unwrap();
    fixture.renderer.remove_object(objects[0]).unwrap();
    let frame = fixture.frame();
    assert_eq!(frame.centre(), CLEAR_BYTES);
    assert_eq!(frame.pixel(column(-2.0), SIZE / 2), CLEAR_BYTES);
    assert!(frame.pixel(column(2.0), SIZE / 2)[0] > 200);

    assert_eq!(
        fixture.renderer.remove_object(objects[0]),
        Err(RenderError::UnknownObject(objects[0]))
    );
}

#[test]
fn removing_a_mesh_removes_its_objects() {
    let mut fixture = Fixture::new();
    let mesh = fixture.renderer.add_mesh(&cube(1.0)).unwrap();
    let object = fixture
        .renderer
        .add_object(mesh, Placement::IDENTITY, RED)
        .unwrap();
    fixture.renderer.remove_mesh(mesh).unwrap();
    assert!(fixture.frame().is_clear());
    assert_eq!(
        fixture.renderer.set_placement(object, Placement::IDENTITY),
        Err(RenderError::UnknownObject(object))
    );
    assert_eq!(
        fixture.renderer.remove_mesh(mesh),
        Err(RenderError::UnknownMesh(mesh))
    );
    assert_eq!(
        fixture.renderer.add_object(mesh, Placement::IDENTITY, RED),
        Err(RenderError::UnknownMesh(mesh))
    );
}

#[test]
fn the_target_may_change_size() {
    let mut fixture = Fixture::new();
    fixture.add_cube(1.0, [0.0; 3], RED);
    fixture.frame();
    let wide = Target::new(&fixture.device, [32, 16]);
    fixture
        .renderer
        .render(&wide.view, wide.size, &camera(PERSPECTIVE), CLEAR)
        .unwrap();
    let pixels = wide.read(&fixture.device, &fixture.queue);
    let centre = ((8 * 32 + 16) * 4) as usize;
    assert!(pixels[centre] > 200);
}

#[test]
fn bad_meshes_objects_and_colours_are_rejected() {
    let mut fixture = Fixture::new();
    let mut empty = cube(1.0);
    empty.indices.clear();
    assert_eq!(fixture.renderer.add_mesh(&empty), Err(MeshError::Empty));

    let mesh: MeshId = fixture.renderer.add_mesh(&cube(1.0)).unwrap();
    let mut sheared = Placement::IDENTITY;
    sheared.rotation[0][1] = 0.5;
    assert_eq!(
        fixture.renderer.add_object(mesh, sheared, RED),
        Err(RenderError::Placement(PlacementError::NotOrthonormal))
    );
    for color in [
        [f32::NAN, 0.0, 0.0, 1.0],
        [1.5, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, -0.1],
    ] {
        assert_eq!(
            fixture
                .renderer
                .add_object(mesh, Placement::IDENTITY, color),
            Err(RenderError::InvalidColor)
        );
    }
    let object = fixture
        .renderer
        .add_object(mesh, Placement::IDENTITY, RED)
        .unwrap();
    assert_eq!(
        fixture.renderer.set_placement(object, sheared),
        Err(RenderError::Placement(PlacementError::NotOrthonormal))
    );
}

#[test]
fn bad_targets_cameras_and_clear_colours_are_rejected() {
    let mut fixture = Fixture::new();
    let view = fixture.target.view.clone();
    let good = camera(PERSPECTIVE);
    let mut render =
        |size, camera: &Camera, clear| fixture.renderer.render(&view, size, camera, clear);

    assert_eq!(
        render([0, SIZE], &good, CLEAR),
        Err(RenderError::ZeroSizedTarget)
    );
    assert!(matches!(
        render([SIZE, u32::MAX], &good, CLEAR),
        Err(RenderError::TargetTooLarge { .. })
    ));
    assert_eq!(
        render([SIZE, SIZE / 2], &good, CLEAR),
        Err(RenderError::TargetMismatch)
    );
    assert_eq!(
        render([SIZE; 2], &good, [0.0, f64::INFINITY, 0.0, 1.0]),
        Err(RenderError::InvalidClearColor)
    );
    let mut looking_at_itself = good;
    looking_at_itself.target = looking_at_itself.eye;
    assert_eq!(
        render([SIZE; 2], &looking_at_itself, CLEAR),
        Err(RenderError::Camera(CameraError::EyeAtTarget))
    );
    let mut no_field_of_view = good;
    no_field_of_view.projection = Projection::Perspective {
        fov_y_radians: 0.0,
        near: 0.1,
    };
    assert_eq!(
        render([SIZE; 2], &no_field_of_view, CLEAR),
        Err(RenderError::Camera(CameraError::InvalidFieldOfView))
    );
}

#[test]
fn targets_of_another_format_or_usage_are_rejected() {
    let mut fixture = Fixture::new();
    let other = |format, usage| {
        fixture
            .device
            .create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: SIZE,
                    height: SIZE,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default())
    };
    let views = [
        other(
            wgpu::TextureFormat::Bgra8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ),
        other(FORMAT, wgpu::TextureUsages::TEXTURE_BINDING),
    ];
    for view in views {
        assert_eq!(
            fixture
                .renderer
                .render(&view, [SIZE; 2], &camera(PERSPECTIVE), CLEAR),
            Err(RenderError::TargetMismatch)
        );
    }
}

#[test]
fn formats_without_a_float_colour_are_rejected() {
    let (device, queue) = support::device();
    for format in [
        wgpu::TextureFormat::Depth32Float,
        wgpu::TextureFormat::Rgba8Uint,
    ] {
        assert_eq!(
            Renderer::new(&device, &queue, format).err(),
            Some(RenderError::UnsupportedFormat(format))
        );
    }
}

#[test]
fn errors_name_the_problem_without_coordinates() {
    let message = RenderError::NoAdapter {
        backends: wgpu::Backends::VULKAN,
        reason: "none found".to_owned(),
    }
    .to_string();
    assert!(message.contains("no GPU adapter") && message.contains("VULKAN"));
    assert!(message.contains("mesa-vulkan-drivers"));
    assert_eq!(
        RenderError::TooManyObjects { max: 7 }.to_string(),
        "the renderer holds the 7 objects this device can draw in one frame"
    );
}
