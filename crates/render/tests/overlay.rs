//! The overlay pass and the display states of objects, on a GPU (VP-01).

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

mod support;

use geom::{Point3, Vector3};
use render::{
    Camera, DisplayState, MeshData, ObjectId, Overlay, Placement, Projection, RenderError,
    Renderer, Rgb, SnapKind, Theme, Tone,
};
use support::{Boxes, FORMAT, Target};

/// Side of the square frames, in pixels.
const SIZE: u32 = 64;

const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];

const PERSPECTIVE: Projection = Projection::Perspective {
    fov_y_radians: core::f64::consts::FRAC_PI_3,
    near: 0.1,
};

/// Pure channels, so linear light and bytes agree exactly: snap magenta, halo white, selection
/// green, hover blue, preview yellow, cut cyan, on black.
fn theme() -> Theme {
    let rgb = |text| Rgb::from_hex(text).unwrap();
    Theme {
        background: rgb("#000000"),
        selection: rgb("#00FF00"),
        hover: rgb("#0000FF"),
        snap: rgb("#FF00FF"),
        preview: rgb("#FFFF00"),
        cut: rgb("#00FFFF"),
        halo: rgb("#FFFFFF"),
    }
}

/// Five metres in front of `centre`, looking along +y: the half-width of the view at `centre`
/// is 2.89 m.
fn camera_at(centre: [f64; 3]) -> Camera {
    Camera {
        eye: Point3::new(centre[0], centre[1] - 5.0, centre[2]),
        target: Point3::new(centre[0], centre[1], centre[2]),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection: PERSPECTIVE,
    }
}

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
    camera: Camera,
}

impl Fixture {
    fn new() -> Self {
        let (device, queue) = support::device();
        let mut renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
        renderer.set_theme(theme());
        let target = Target::new(&device, [SIZE; 2]);
        Self {
            device,
            queue,
            renderer,
            target,
            camera: camera_at([0.0; 3]),
        }
    }

    fn add_cube(&mut self, side: f64, at: [f64; 3], color: [f32; 4]) -> ObjectId {
        let mesh = self.renderer.add_mesh(&cube(side)).unwrap();
        self.renderer
            .add_object(mesh, Placement::from_translation(at), color)
            .unwrap()
    }

    fn set(&mut self, build: impl FnOnce(&mut Overlay)) {
        let mut overlay = Overlay::new();
        build(&mut overlay);
        self.renderer.set_overlay(&overlay).unwrap();
    }

    fn frame(&mut self) -> Frame {
        self.renderer
            .render(
                &self.target.view,
                self.target.size,
                &self.camera,
                theme().clear_color(),
            )
            .unwrap();
        Frame(self.target.read(&self.device, &self.queue))
    }
}

#[derive(PartialEq, Eq)]
struct Frame(Vec<u8>);

impl Frame {
    /// The pixel at column `x` and row `y`, from the top left.
    fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * SIZE + x) * 4) as usize;
        self.0[start..start + 4].try_into().unwrap()
    }

    fn count(&self, color: [u8; 4]) -> usize {
        self.0
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|&&pixel| pixel == color)
            .count()
    }

    fn is_black(&self) -> bool {
        self.count(BLACK) == (SIZE * SIZE) as usize
    }

    /// The largest difference of a channel between two frames.
    fn distance(&self, other: &Self) -> u8 {
        self.0
            .iter()
            .zip(&other.0)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap()
    }
}

/// The column where the world `x` of the plane `y = 0` appears, for the camera at the origin.
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
fn a_frame_without_overlay_is_unchanged() {
    let mut fixture = Fixture::new();
    fixture.add_cube(1.0, [0.0; 3], RED);
    let before = fixture.frame();
    assert!(before.pixel(32, 32)[0] > 200);

    fixture.set(|overlay| {
        overlay
            .line([-2.0, 0.0, 0.0], [2.0, 0.0, 0.0], 4.0, Tone::Snap)
            .unwrap();
        overlay.marker([0.0; 3], SnapKind::Endpoint).unwrap();
    });
    assert!(fixture.frame() != before);

    // An empty overlay, and a cleared one, draw the model alone, pixel for pixel.
    fixture.set(|_| {});
    assert!(fixture.frame() == before);
    fixture.set(|overlay| overlay.marker([0.0; 3], SnapKind::Face).unwrap());
    fixture.renderer.clear_overlay();
    assert!(fixture.frame() == before);
}

#[test]
fn lines_are_drawn_over_the_model_with_a_halo() {
    let mut fixture = Fixture::new();
    fixture.add_cube(2.0, [0.0; 3], RED);
    // 3 m behind the cube: the model is in front of it, the overlay still shows.
    fixture.set(|overlay| {
        overlay
            .line([-3.0, 3.0, 0.0], [3.0, 3.0, 0.0], 4.0, Tone::Snap)
            .unwrap();
    });
    let frame = fixture.frame();
    // The ink, 4 px wide, over the cube and over the background.
    assert_eq!(frame.pixel(32, 32), MAGENTA);
    assert_eq!(frame.pixel(14, 32), MAGENTA);
    // The halo, 2 px on each side; beyond it the model and the background show again.
    assert_eq!(frame.pixel(32, 35), WHITE);
    assert_eq!(frame.pixel(14, 35), WHITE);
    assert!(frame.pixel(32, 40)[0] > 200);
    assert_eq!(frame.pixel(14, 40), BLACK);
    // The line ends at x = ±3 m behind the cube, 8 m from the camera: columns 11 and 53.
    assert_eq!(frame.pixel(5, 32), BLACK);
    assert_eq!(frame.pixel(59, 32), BLACK);
}

#[test]
fn halos_never_cover_the_ink_of_another_line() {
    let mut fixture = Fixture::new();
    fixture.set(|overlay| {
        overlay
            .line([-2.0, 0.0, 0.0], [2.0, 0.0, 0.0], 4.0, Tone::Snap)
            .unwrap();
        overlay
            .line([0.0, 0.0, -2.0], [0.0, 0.0, 2.0], 4.0, Tone::Hover)
            .unwrap();
    });
    let frame = fixture.frame();
    // Within the halo of the vertical line, the horizontal one keeps its ink.
    assert_eq!(frame.pixel(35, 32), MAGENTA);
    assert_eq!(frame.pixel(32, 40), BLUE);
    assert_eq!(frame.pixel(32, 20), BLUE);
}

#[test]
fn a_line_crossing_the_camera_plane_is_cut() {
    let mut fixture = Fixture::new();
    // From above the origin to behind the camera, 20 m back.
    fixture.set(|overlay| {
        overlay
            .line([0.0, 0.0, 1.0], [0.0, -20.0, 1.0], 4.0, Tone::Snap)
            .unwrap();
    });
    let frame = fixture.frame();
    // It leaves the view through the top, 21 px above the centre at 5 m.
    assert_eq!(frame.pixel(32, 10), MAGENTA);
    assert_eq!(frame.pixel(32, 50), BLACK);
    assert_eq!(frame.pixel(10, 10), BLACK);
}

#[test]
fn markers_have_a_halo_and_a_shape_each() {
    let kinds = [
        SnapKind::Endpoint,
        SnapKind::Midpoint,
        SnapKind::Edge,
        SnapKind::Face,
        SnapKind::Ground,
    ];
    let mut fixture = Fixture::new();
    let frames: Vec<Frame> = kinds
        .iter()
        .map(|&kind| {
            fixture.set(|overlay| overlay.marker([0.0; 3], kind).unwrap());
            fixture.frame()
        })
        .collect();
    for (kind, frame) in kinds.iter().zip(&frames) {
        assert!(frame.count(MAGENTA) > 20, "{kind:?}");
        assert!(frame.count(WHITE) > 20, "{kind:?}");
        // The marker is small: well within a quarter of the frame.
        assert!(frame.count(BLACK) > 3000, "{kind:?}");
        assert_eq!(frame.pixel(2, 2), BLACK);
    }
    for (index, frame) in frames.iter().enumerate() {
        for (other, other_frame) in frames.iter().enumerate().skip(index + 1) {
            assert!(
                frame != other_frame,
                "{:?} and {:?} look alike",
                kinds[index],
                kinds[other]
            );
        }
    }
    // The square is filled with the halo colour inside its stroke; the plus is only strokes.
    assert_eq!(frames[0].pixel(32, 32), WHITE);
    assert_eq!(frames[0].pixel(38, 32), MAGENTA);
    assert_eq!(frames[4].pixel(32, 32), MAGENTA);
    assert_eq!(frames[4].pixel(38, 32), MAGENTA);
    assert_eq!(frames[4].pixel(38, 38), BLACK);
}

#[test]
fn markers_stay_visible_over_a_model_of_any_colour() {
    let mut fixture = Fixture::new();
    fixture.add_cube(2.0, [0.0; 3], RED);
    // Behind the cube: no depth test.
    fixture.set(|overlay| overlay.marker([0.0, 2.0, 0.0], SnapKind::Endpoint).unwrap());
    let frame = fixture.frame();
    assert_eq!(frame.pixel(32, 32), WHITE);
    // At 7 m, a 14 px marker is the same size on the screen: its stroke is magenta all round.
    assert_eq!(frame.pixel(38, 32), MAGENTA);
    assert_eq!(frame.pixel(32, 25), MAGENTA);
}

#[test]
fn a_marker_ten_million_metres_away_sits_on_the_same_pixel() {
    let near = {
        let mut fixture = Fixture::new();
        fixture.set(|overlay| {
            overlay.marker([0.3, 0.0, 0.2], SnapKind::Face).unwrap();
            overlay
                .line([-1.0, 0.0, -1.0], [1.0, 0.0, -1.0], 3.0, Tone::Preview)
                .unwrap();
        });
        fixture.frame()
    };
    for shift in [[1e6, 1e6, 1e2], [1e7, 1e7, 1e2]] {
        let mut fixture = Fixture::new();
        fixture.camera = camera_at(shift);
        let at = |p: [f64; 3]| [p[0] + shift[0], p[1] + shift[1], p[2] + shift[2]];
        fixture.set(|overlay| {
            overlay.marker(at([0.3, 0.0, 0.2]), SnapKind::Face).unwrap();
            overlay
                .line(
                    at([-1.0, 0.0, -1.0]),
                    at([1.0, 0.0, -1.0]),
                    3.0,
                    Tone::Preview,
                )
                .unwrap();
        });
        let far = fixture.frame();
        assert!(near.count(MAGENTA) > 10);
        // Rounding the camera-relative coordinates may move an edge by a hair, never a pixel.
        assert!(near.distance(&far) <= 2, "{shift:?}");
    }
}

#[test]
fn selected_and_hovered_objects_take_the_theme_colours() {
    let mut fixture = Fixture::new();
    let object = fixture.add_cube(1.0, [0.0; 3], RED);
    let normal = fixture.frame();
    assert_eq!(
        fixture.renderer.display_state(object),
        Ok(DisplayState::Normal)
    );

    fixture
        .renderer
        .set_display_state(object, DisplayState::Selected)
        .unwrap();
    assert_eq!(
        fixture.renderer.display_state(object),
        Ok(DisplayState::Selected)
    );
    // Red mixed 60 % towards green, with the 0.85 light of the face turned to the camera.
    let [r, g, b, _] = fixture.frame().pixel(32, 32);
    assert!((84..=90).contains(&r), "{r}");
    assert!((127..=133).contains(&g), "{g}");
    assert_eq!(b, 0);

    fixture
        .renderer
        .set_display_state(object, DisplayState::Hovered)
        .unwrap();
    // Mixed 40 % towards blue.
    let [r, g, b, _] = fixture.frame().pixel(32, 32);
    assert!((127..=133).contains(&r), "{r}");
    assert_eq!(g, 0);
    assert!((84..=90).contains(&b), "{b}");

    fixture
        .renderer
        .set_display_state(object, DisplayState::Normal)
        .unwrap();
    assert!(fixture.frame() == normal);

    // A new theme recolours the next frame.
    fixture
        .renderer
        .set_display_state(object, DisplayState::Selected)
        .unwrap();
    fixture.renderer.set_theme(Theme {
        selection: Rgb::from_hex("#0000FF").unwrap(),
        ..theme()
    });
    assert_eq!(fixture.renderer.theme().selection.to_hex(), "#0000FF");
    assert!(fixture.frame().pixel(32, 32)[2] > 100);
}

#[test]
fn ghost_objects_blend_and_hide_nothing() {
    let mut fixture = Fixture::new();
    let ghost = fixture.add_cube(1.0, [0.0; 3], RED);
    fixture
        .renderer
        .set_display_state(ghost, DisplayState::Ghost)
        .unwrap();
    // Over the background: the yellow preview colour at 40 %, lit like the model (217), and
    // blended twice since the ghost writes no depth: its front face, then its back face, which is
    // 0.4 · 217 + 0.6 · (0.4 · 217) = 139.
    let frame = fixture.frame();
    let [r, g, b, _] = frame.pixel(32, 32);
    assert!(
        (136..=142).contains(&r) && (136..=142).contains(&g),
        "{r} {g}"
    );
    assert_eq!(b, 0);
    assert_eq!(frame.pixel(5, 5), BLACK);

    // Over a model behind it: the model shows through, and the ghost tints it. The ghost writes
    // no depth, so a model farther than it is still drawn.
    let behind = fixture.add_cube(3.0, [0.0, 3.0, 0.0], GREEN);
    let [r, g, b, _] = fixture.frame().pixel(32, 32);
    assert!((136..=142).contains(&r), "{r}");
    assert!((213..=220).contains(&g), "{g}");
    assert_eq!(b, 0);

    // A model in front of the ghost hides it, as it hides any model.
    fixture.renderer.remove_object(behind).unwrap();
    fixture.add_cube(0.5, [0.0, -2.0, 0.0], RED);
    let [r, g, _, _] = fixture.frame().pixel(32, 32);
    assert!(r > 200 && g == 0, "{r} {g}");
}

#[test]
fn ghost_instances_follow_an_extra_transform_without_touching_the_model() {
    let mut fixture = Fixture::new();
    let object = fixture.add_cube(1.0, [-1.0, 0.0, 0.0], RED);
    let before = fixture.frame();
    let left = column(-1.0);
    assert!(before.pixel(left, 32)[0] > 200);

    for offset in [2.0, 1.0, 2.0] {
        fixture.set(|overlay| {
            overlay
                .ghost(object, Placement::from_translation([offset, 0.0, 0.0]))
                .unwrap();
        });
        let frame = fixture.frame();
        // The model stays where it is; the ghost, blended over the background, is moved.
        assert_eq!(frame.pixel(left, 32), before.pixel(left, 32));
        let [r, g, b, _] = frame.pixel(column(-1.0 + offset), 32);
        assert!(
            (136..=142).contains(&r) && (136..=142).contains(&g) && b == 0,
            "{offset}: {r} {g} {b}"
        );
        assert_eq!(frame.pixel(5, 5), BLACK);
    }
    // Without the ghost, the frame is the model alone again.
    fixture.set(|_| {});
    assert!(fixture.frame() == before);
}

#[test]
fn many_ghosts_of_one_mesh_are_instanced_and_a_removed_object_is_skipped() {
    let mut fixture = Fixture::new();
    let mesh = fixture.renderer.add_mesh(&cube(0.5)).unwrap();
    let objects: Vec<_> = (0..40)
        .map(|index| {
            fixture
                .renderer
                .add_object(
                    mesh,
                    Placement::from_translation([f64::from(index) * 0.1 - 2.0, 0.0, 0.0]),
                    RED,
                )
                .unwrap()
        })
        .collect();
    let mut overlay = Overlay::new();
    for &object in &objects {
        overlay
            .ghost(object, Placement::from_translation([0.0, 0.0, 1.0]))
            .unwrap();
    }
    fixture.renderer.set_overlay(&overlay).unwrap();
    let with_ghosts = fixture.frame();
    // Ghosts that do not stand in the way of the object they follow draw above it.
    assert!(with_ghosts.count(BLACK) < (SIZE * SIZE) as usize);

    // The overlay refers to an object that goes away: nothing is drawn for it, nothing fails.
    fixture.renderer.remove_object(objects[0]).unwrap();
    fixture.frame();
    assert_eq!(
        fixture.renderer.set_overlay(&overlay),
        Err(RenderError::UnknownObject(objects[0]))
    );
}

#[test]
fn translucent_boxes_blend_and_are_hidden_by_the_model() {
    let mut fixture = Fixture::new();
    fixture.set(|overlay| {
        overlay
            .translucent_box([-1.0; 3], [1.0; 3], Tone::Selection)
            .unwrap();
    });
    // The two faces on the line of sight each take a quarter of the green: 1 − 0.75² = 44 %.
    let frame = fixture.frame();
    let [r, g, b, _] = frame.pixel(32, 32);
    assert_eq!((r, b), (0, 0));
    assert!((109..=115).contains(&g), "{g}");
    assert_eq!(frame.pixel(5, 5), BLACK);

    // A model in front of the box hides it; the box shows over a model behind it.
    let wall = fixture.add_cube(0.5, [0.0, -2.0, 0.0], RED);
    let [r, g, _, _] = fixture.frame().pixel(32, 32);
    assert!(r > 200 && g == 0, "{r} {g}");
    fixture.renderer.remove_object(wall).unwrap();
    fixture.add_cube(0.5, [0.0, 3.0, 0.0], RED);
    let [r, g, _, _] = fixture.frame().pixel(32, 32);
    // The red model (217) under the two faces of the box: 217 · 0.75², and 255 · 0.4375 of green.
    assert!(
        (119..=125).contains(&r) && (109..=115).contains(&g),
        "{r} {g}"
    );
}

#[test]
fn the_overlay_is_drawn_through_an_orthographic_camera_too() {
    let mut fixture = Fixture::new();
    fixture.camera.projection = Projection::Orthographic {
        height: 4.0,
        near: 0.1,
        far: 20.0,
    };
    fixture.set(|overlay| overlay.marker([0.0; 3], SnapKind::Ground).unwrap());
    assert_eq!(fixture.frame().pixel(32, 32), MAGENTA);
}

#[test]
fn the_overlay_is_replaced_each_frame_without_error() {
    let mut fixture = Fixture::new();
    let object = fixture.add_cube(1.0, [0.0; 3], RED);
    let mut overlay = Overlay::new();
    for index in 0..30 {
        overlay.clear();
        let offset = f64::from(index) * 0.05;
        overlay.marker([offset, 0.0, 0.0], SnapKind::Edge).unwrap();
        overlay
            .ghost(object, Placement::from_translation([offset, 0.0, 0.0]))
            .unwrap();
        fixture.renderer.set_overlay(&overlay).unwrap();
        assert!(!fixture.frame().is_black());
    }
}

#[test]
fn unknown_objects_are_refused() {
    let mut fixture = Fixture::new();
    let object = fixture.add_cube(1.0, [0.0; 3], RED);
    fixture.renderer.remove_object(object).unwrap();
    assert_eq!(
        fixture
            .renderer
            .set_display_state(object, DisplayState::Selected),
        Err(RenderError::UnknownObject(object))
    );
    assert_eq!(
        fixture.renderer.display_state(object),
        Err(RenderError::UnknownObject(object))
    );
    let mut overlay = Overlay::new();
    overlay.ghost(object, Placement::IDENTITY).unwrap();
    // The previous overlay is kept when the new one is refused.
    fixture.set(|overlay| overlay.marker([0.0; 3], SnapKind::Face).unwrap());
    assert_eq!(
        fixture.renderer.set_overlay(&overlay),
        Err(RenderError::UnknownObject(object))
    );
    assert!(!fixture.frame().is_black());
}

#[test]
fn objects_under_a_rectangle_of_pixels_are_found() {
    let mut fixture = Fixture::new();
    let object = fixture.add_cube(1.0, [0.0; 3], RED);
    let centre = f64::from(SIZE) / 2.0;
    let mut found = Vec::new();
    fixture
        .renderer
        .objects_in_rect(
            &fixture.camera,
            [SIZE; 2],
            [centre - 1.0, centre - 1.0, centre + 1.0, centre + 1.0],
            |id| found.push(id),
        )
        .unwrap();
    assert_eq!(found, [object]);
    found.clear();
    fixture
        .renderer
        .objects_in_rect(&fixture.camera, [SIZE; 2], [0.0, 0.0, 2.0, 2.0], |id| {
            found.push(id);
        })
        .unwrap();
    assert_eq!(found, [] as [ObjectId; 0]);
    assert_eq!(
        fixture
            .renderer
            .objects_in_rect(&fixture.camera, [SIZE; 2], [2.0, 0.0, 1.0, 1.0], |_| {}),
        Err(RenderError::InvalidRect)
    );
}
