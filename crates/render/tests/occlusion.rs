//! Occlusion culling on a GPU (VP-02): clusters hidden behind a wall are skipped once the GPU has
//! found them hidden, and no frame differs by a pixel from the frame drawn without occlusion
//! culling, even when objects come into view or the wall goes away.

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

mod support;

use geom::{Point3, Vector3};
use render::{Camera, FrameStats, ObjectId, Placement, Projection, RenderSettings, Renderer};
use support::{Boxes, FORMAT, Target};

const CLEAR: [f64; 4] = [0.0, 0.2, 1.0, 1.0];

const SIZE: [u32; 2] = [128, 128];

/// The cubes of a group: 9 × 9 cubes of 1 m, 2 m apart.
const GROUP: usize = 9 * 9;

/// The frames a test waits at most for the query results: each frame reads its target back,
/// which waits for the GPU, so the results of a frame are there at the next one.
const SETTLE_FRAMES: usize = 3;

/// Every object in view drawn with its own mesh, with occlusion culling on or off.
fn settings(occlusion_culling: bool) -> RenderSettings {
    RenderSettings {
        frustum_culling: true,
        coarse_below_px: 0.0,
        hide_below_px: 0.0,
        hysteresis: 0.0,
        occlusion_culling,
        ..RenderSettings::default()
    }
}

/// A wall 12 m wide and high, 10 m ahead of the eye at the origin, which looks along +y. Behind
/// it, 60 m ahead, a group of cubes it hides from the eye; 30 m ahead and to the right, a group
/// beside the wall in view, and one further right, out of view until the camera turns right.
struct Scene {
    renderer: Renderer,
    wall: ObjectId,
    hidden: Vec<ObjectId>,
    entering: Vec<ObjectId>,
}

fn scene(occlusion_culling: bool) -> Scene {
    let (device, queue) = support::device();
    let mut renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
    renderer.set_settings(settings(occlusion_culling)).unwrap();
    let mut wall = Boxes::default();
    wall.add([-6.0, -0.1, -6.0], [6.0, 0.1, 6.0]);
    let wall_mesh = renderer.add_mesh(&wall.mesh()).unwrap();
    let wall = renderer
        .add_object(
            wall_mesh,
            Placement::from_translation([0.0, 10.0, 0.0]),
            [0.6, 0.6, 0.6, 1.0],
        )
        .unwrap();
    let mut cube = Boxes::default();
    cube.add([-0.5; 3], [0.5; 3]);
    let cube = renderer.add_mesh(&cube.mesh()).unwrap();
    let mut group = |x: f64, y: f64, color: [f32; 4]| -> Vec<ObjectId> {
        (0..9_u32)
            .flat_map(|i| (0..9_u32).map(move |j| (i, j)))
            .map(|(i, j)| {
                let at = [x + 2.0 * f64::from(i), y, 2.0 * f64::from(j) - 8.0];
                renderer
                    .add_object(cube, Placement::from_translation(at), color)
                    .unwrap()
            })
            .collect()
    };
    let hidden = group(-8.0, 60.0, [1.0, 0.0, 0.0, 1.0]);
    group(20.0, 30.0, [0.0, 1.0, 0.0, 1.0]);
    let entering = group(41.0, 30.0, [1.0, 1.0, 0.0, 1.0]);
    Scene {
        renderer,
        wall,
        hidden,
        entering,
    }
}

/// From the origin, 90°, turned `degrees` from +y towards +x.
fn turned(degrees: f64) -> Camera {
    let angle = degrees * core::f64::consts::PI / 180.0;
    Camera {
        eye: Point3::new(0.0, 0.0, 0.0),
        target: Point3::new(libm::sin(angle), libm::cos(angle), 0.0),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection: Projection::Perspective {
            fov_y_radians: core::f64::consts::FRAC_PI_2,
            near: 0.1,
        },
    }
}

/// Draws a frame and reads it back, which waits for the GPU and so delivers the frame's query
/// results to the next frame.
fn frame(renderer: &mut Renderer, camera: &Camera) -> (Vec<u8>, FrameStats) {
    let (device, queue) = support::device();
    let target = Target::new(&device, SIZE);
    let stats = renderer.render(&target.view, SIZE, camera, CLEAR).unwrap();
    (target.read(&device, &queue), stats)
}

/// Draws frames until the hidden group is skipped, checking that each equals `expected`;
/// returns the stats of the last one.
fn settle(scene: &mut Scene, camera: &Camera, expected: &[u8]) -> FrameStats {
    for _ in 0..SETTLE_FRAMES {
        let (pixels, stats) = frame(&mut scene.renderer, camera);
        assert!(
            pixels == expected,
            "a frame differs while the queries settle"
        );
        if stats.culled_occluded > 0 {
            return stats;
        }
    }
    panic!("no cluster was found hidden after {SETTLE_FRAMES} frames");
}

#[test]
fn a_wall_hides_clusters_without_changing_a_pixel() {
    let mut culled = scene(true);
    let mut reference = scene(false);
    let camera = turned(0.0);
    let (expected, all) = frame(&mut reference.renderer, &camera);
    assert_eq!((all.culled_occluded, all.occlusion_queries), (0, 0));
    // The first frame has no results yet: it draws everything in view and tests the clusters.
    let (first, stats) = frame(&mut culled.renderer, &camera);
    assert_eq!((stats.culled_occluded, stats.fine), (0, all.fine));
    assert!(stats.occlusion_queries > 0);
    assert!(first == expected, "the first frame differs");
    // Then the hidden group is skipped, and only it.
    let stats = settle(&mut culled, &camera, &expected);
    assert_eq!(stats.culled_occluded, GROUP);
    assert_eq!(stats.fine, all.fine - GROUP);
    for &id in &culled.hidden {
        assert_eq!(culled.renderer.drawn_level(id), Ok(None));
    }
    assert!(culled.renderer.drawn_level(culled.wall).unwrap().is_some());
    // It stays skipped while the view does not change.
    let (pixels, stats) = frame(&mut culled.renderer, &camera);
    assert_eq!(stats.culled_occluded, GROUP);
    assert!(pixels == expected, "a culled frame differs");
    // Removing the wall reveals the group in the same frame.
    culled.renderer.remove_object(culled.wall).unwrap();
    reference.renderer.remove_object(reference.wall).unwrap();
    let (revealed, _) = frame(&mut reference.renderer, &camera);
    assert!(revealed != expected, "the wall hid nothing");
    let (pixels, stats) = frame(&mut culled.renderer, &camera);
    assert_eq!(stats.culled_occluded, 0);
    assert!(pixels == revealed, "the revealed frame differs");
}

#[test]
fn objects_coming_into_view_are_drawn_the_same_frame() {
    let mut culled = scene(true);
    let mut reference = scene(false);
    let ahead = turned(0.0);
    let (expected, _) = frame(&mut reference.renderer, &ahead);
    settle(&mut culled, &ahead, &expected);
    for &id in &culled.entering {
        assert_eq!(culled.renderer.drawn_level(id), Ok(None));
    }
    // Turning right brings the last group into view. Turning in place changes what hides what
    // nowhere, so the hidden group stays skipped; the new one is drawn at once.
    let right = turned(20.0);
    let (expected, all) = frame(&mut reference.renderer, &right);
    let (pixels, stats) = frame(&mut culled.renderer, &right);
    assert_eq!(stats.culled_occluded, GROUP);
    assert_eq!(stats.fine, all.fine - GROUP);
    for &id in &culled.entering {
        assert!(culled.renderer.drawn_level(id).unwrap().is_some());
    }
    assert!(pixels == expected, "the frame after turning differs");
}

#[test]
fn query_settings_are_validated() {
    let (device, queue) = support::device();
    let mut renderer = Renderer::new(&device, &queue, FORMAT).unwrap();
    let defaults = RenderSettings::default();
    assert!(defaults.occlusion_culling);
    assert_eq!(defaults.occlusion_queries, 1024);
    for occlusion_queries in [0, RenderSettings::MAX_OCCLUSION_QUERIES + 1] {
        let bad = RenderSettings {
            occlusion_queries,
            ..defaults
        };
        assert!(renderer.set_settings(bad).is_err());
    }
    let widest = RenderSettings {
        occlusion_queries: RenderSettings::MAX_OCCLUSION_QUERIES,
        ..defaults
    };
    renderer.set_settings(widest).unwrap();
    assert_eq!(renderer.settings(), widest);
}
