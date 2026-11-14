//! Instancing, culling and levels of detail on a GPU (VP-02): what they skip never changes a
//! pixel it should not, and what they draw is what the CPU decided.

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

#[path = "support/reference.rs"]
#[allow(dead_code, reason = "shared with the benches, which use its cameras")]
mod reference;
mod support;

use geom::{Point3, Vector3};
use render::{
    Camera, DetailLevel, FrameStats, MeshData, ObjectId, Placement, Projection, RenderError,
    RenderSettings, Renderer,
};
use support::{Boxes, FORMAT, Target};

const CLEAR: [f64; 4] = [0.0, 0.2, 1.0, 1.0];

/// Culling and levels of detail off: every object drawn with its own mesh.
const EVERYTHING: RenderSettings = RenderSettings {
    frustum_culling: false,
    coarse_below_px: 0.0,
    hide_below_px: 0.0,
    hysteresis: 0.0,
    occlusion_culling: false,
    occlusion_queries: 1,
};

/// Draws a frame of `size` and reads it back, with what it drew.
fn frame(renderer: &mut Renderer, camera: &Camera, size: [u32; 2]) -> (Vec<u8>, FrameStats) {
    let (device, queue) = support::device();
    let target = Target::new(&device, size);
    let stats = renderer.render(&target.view, size, camera, CLEAR).unwrap();
    (target.read(&device, &queue), stats)
}

fn renderer() -> Renderer {
    let (device, queue) = support::device();
    Renderer::new(&device, &queue, FORMAT).unwrap()
}

/// Looking at the origin from `eye`, z up, 60°.
fn looking_from(eye: [f64; 3]) -> Camera {
    Camera {
        eye: Point3::new(eye[0], eye[1], eye[2]),
        target: Point3::new(0.0, 0.0, 0.0),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection: Projection::Perspective {
            fov_y_radians: core::f64::consts::FRAC_PI_3,
            near: 0.1,
        },
    }
}

/// A box of side `side` centred on the origin.
fn cube(side: f64) -> MeshData {
    let mut boxes = Boxes::default();
    boxes.add([-side / 2.0; 3], [side / 2.0; 3]);
    boxes.mesh()
}

/// A cross of three bars: 36 triangles, so it has a coarse level.
fn cross() -> MeshData {
    let mut boxes = Boxes::default();
    boxes.add([-1.0, -0.1, -0.1], [1.0, 0.1, 0.1]);
    boxes.add([-0.1, -1.0, -0.1], [0.1, 1.0, 0.1]);
    boxes.add([-0.1, -0.1, -1.0], [0.1, 0.1, 1.0]);
    boxes.mesh()
}

/// A 5 × 5 grid of overlapping cubes, turned and coloured each its own way, seen at an angle.
fn grid() -> Vec<(Placement, [f32; 4])> {
    let mut objects = Vec::new();
    for i in 0..5_u8 {
        for j in 0..5_u8 {
            let angle = 0.3 * f64::from(i * 5 + j);
            let (s, c) = (libm::sin(angle), libm::cos(angle));
            let placement = Placement {
                rotation: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
                translation: [f64::from(i) * 0.8 - 1.6, f64::from(j) * 0.8 - 1.6, 0.0],
            };
            let color = [f32::from(i) / 4.0, f32::from(j) / 4.0, 0.5, 1.0];
            objects.push((placement, color));
        }
    }
    objects
}

#[test]
fn objects_sharing_a_mesh_draw_in_one_call_the_same_pixels_as_separate_meshes() {
    let camera = looking_from([3.0, -6.0, 4.0]);
    let size = [96, 96];
    let mut shared = renderer();
    let mesh = shared.add_mesh(&cube(1.0)).unwrap();
    for &(placement, color) in &grid() {
        shared.add_object(mesh, placement, color).unwrap();
    }
    let mut separate = renderer();
    for &(placement, color) in &grid() {
        let mesh = separate.add_mesh(&cube(1.0)).unwrap();
        separate.add_object(mesh, placement, color).unwrap();
    }
    let (instanced, instanced_stats) = frame(&mut shared, &camera, size);
    let (one_by_one, separate_stats) = frame(&mut separate, &camera, size);
    assert_eq!(instanced_stats.draw_calls, 1);
    assert_eq!(separate_stats.draw_calls, 25);
    assert_eq!((instanced_stats.fine, separate_stats.fine), (25, 25));
    assert!(instanced == one_by_one, "the instanced frame differs");
    // The frame is not trivially empty.
    assert!(
        instanced
            .as_chunks::<4>()
            .0
            .iter()
            .any(|&p| p != [0, 51, 255, 255])
    );
}

#[test]
fn frustum_culling_skips_objects_without_changing_a_pixel() {
    let mut renderer = renderer();
    let model = reference::building(3);
    model.load(&mut renderer);
    // On the second storey, looking along the building, far from the world origin.
    let z = reference::STOREY_HEIGHT + 1.6;
    let at = |x: f64, y: f64| {
        Point3::new(
            reference::ORIGIN[0] + x,
            reference::ORIGIN[1] + y,
            reference::ORIGIN[2] + z,
        )
    };
    let camera = Camera {
        eye: at(10.0, 18.0),
        target: at(72.0, 22.0),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection: Projection::Perspective {
            fov_y_radians: core::f64::consts::FRAC_PI_3,
            near: 0.1,
        },
    };
    let size = [192, 108];
    renderer.set_settings(EVERYTHING).unwrap();
    let (unculled, all) = frame(&mut renderer, &camera, size);
    renderer
        .set_settings(RenderSettings {
            frustum_culling: true,
            ..EVERYTHING
        })
        .unwrap();
    let (culled, stats) = frame(&mut renderer, &camera, size);
    assert_eq!(all.culled_frustum, 0);
    assert!(
        stats.culled_frustum > model.objects.len() / 4,
        "only {} of {} culled",
        stats.culled_frustum,
        model.objects.len()
    );
    assert_eq!(stats.fine + stats.culled_frustum, model.objects.len());
    assert!(culled == unculled, "the culled frame differs");
}

#[test]
fn a_small_detailed_object_draws_its_bounding_box() {
    let camera = looking_from([0.0, -40.0, 0.0]);
    let size = [64, 64];
    // 40 m away, the cross's sphere (radius √2.02 m) is about 4 px wide: coarse, not hidden.
    let mut detailed = renderer();
    let mesh = detailed.add_mesh(&cross()).unwrap();
    let object = detailed
        .add_object(mesh, Placement::IDENTITY, [0.9, 0.5, 0.1, 1.0])
        .unwrap();
    let mut boxed = renderer();
    let mesh = boxed.add_mesh(&cross().bounding_box().unwrap()).unwrap();
    boxed
        .add_object(mesh, Placement::IDENTITY, [0.9, 0.5, 0.1, 1.0])
        .unwrap();
    let (coarse, stats) = frame(&mut detailed, &camera, size);
    assert_eq!((stats.fine, stats.coarse), (0, 1));
    assert_eq!(detailed.drawn_level(object), Ok(Some(DetailLevel::Coarse)));
    let (expected, _) = frame(&mut boxed, &camera, size);
    assert!(
        coarse == expected,
        "the coarse level is not the bounding box"
    );
    // Without levels of detail it draws the cross, which differs from its box.
    detailed.set_settings(EVERYTHING).unwrap();
    let (fine, stats) = frame(&mut detailed, &camera, size);
    assert_eq!((stats.fine, stats.coarse), (1, 0));
    assert_eq!(detailed.drawn_level(object), Ok(Some(DetailLevel::Fine)));
    assert!(fine != expected, "the cross looks like its box");
}

#[test]
fn objects_under_a_pixel_are_not_drawn() {
    let mut renderer = renderer();
    let mesh = renderer.add_mesh(&cube(0.01)).unwrap();
    let speck = renderer
        .add_object(mesh, Placement::IDENTITY, [1.0; 4])
        .unwrap();
    let camera = looking_from([0.0, -50.0, 0.0]);
    let (pixels, stats) = frame(&mut renderer, &camera, [64, 64]);
    assert_eq!((stats.culled_small, stats.fine), (1, 0));
    assert_eq!(stats.draw_calls, 0);
    assert_eq!(renderer.drawn_level(speck), Ok(None));
    assert!(
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|&p| p == [0, 51, 255, 255])
    );
}

#[test]
fn prepare_frame_decides_without_drawing() {
    let mut renderer = renderer();
    let mesh = renderer.add_mesh(&cube(1.0)).unwrap();
    let ahead = renderer
        .add_object(mesh, Placement::IDENTITY, [1.0; 4])
        .unwrap();
    let behind = renderer
        .add_object(
            mesh,
            Placement::from_translation([0.0, -20.0, 0.0]),
            [1.0; 4],
        )
        .unwrap();
    let camera = looking_from([0.0, -10.0, 0.0]);
    let stats = renderer.prepare_frame(&camera, [64, 64]).unwrap();
    assert_eq!((stats.objects, stats.fine, stats.culled_frustum), (2, 1, 1));
    assert_eq!(renderer.drawn_level(ahead), Ok(Some(DetailLevel::Fine)));
    assert_eq!(renderer.drawn_level(behind), Ok(None));
    assert_eq!(
        renderer.prepare_frame(&camera, [0, 64]),
        Err(RenderError::ZeroSizedTarget)
    );
    assert!(matches!(
        renderer.prepare_frame(&camera, [u32::MAX, 64]),
        Err(RenderError::TargetTooLarge { .. })
    ));
    let mut bad = camera;
    bad.target = bad.eye;
    assert!(matches!(
        renderer.prepare_frame(&bad, [64, 64]),
        Err(RenderError::Camera(_))
    ));
    renderer.remove_object(ahead).unwrap();
    assert_eq!(
        renderer.drawn_level(ahead),
        Err(RenderError::UnknownObject(ahead))
    );
}

#[test]
fn settings_are_validated() {
    let mut renderer = renderer();
    assert_eq!(renderer.settings(), RenderSettings::default());
    let bad = RenderSettings {
        hysteresis: 2.0,
        ..RenderSettings::default()
    };
    assert_eq!(
        renderer.set_settings(bad),
        Err(RenderError::InvalidSettings)
    );
    assert_eq!(renderer.settings(), RenderSettings::default());
    renderer.set_settings(EVERYTHING).unwrap();
    assert_eq!(renderer.settings(), EVERYTHING);
    assert_eq!(
        RenderError::InvalidSettings.to_string(),
        "the render settings are out of range"
    );
}

#[test]
fn moving_an_object_into_view_draws_it() {
    let mut renderer = renderer();
    let mesh = renderer.add_mesh(&cube(1.0)).unwrap();
    let object: ObjectId = renderer
        .add_object(
            mesh,
            Placement::from_translation([0.0, -20.0, 0.0]),
            [1.0, 0.0, 0.0, 1.0],
        )
        .unwrap();
    let camera = looking_from([0.0, -10.0, 0.0]);
    let (_, stats) = frame(&mut renderer, &camera, [64, 64]);
    assert_eq!(stats.fine, 0);
    renderer.set_placement(object, Placement::IDENTITY).unwrap();
    let (pixels, stats) = frame(&mut renderer, &camera, [64, 64]);
    assert_eq!(stats.fine, 1);
    let centre = (32 * 64 + 32) * 4;
    // The face turned to the camera is lit 0.25 + 0.75 · 0.80 = 0.85 (the ambient share plus the
    // diffuse light), so full red comes out as 217.
    assert_eq!(pixels[centre..centre + 4], [217, 0, 0, 255]);
}
