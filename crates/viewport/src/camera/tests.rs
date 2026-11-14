use proptest::prelude::*;

use super::*;
use crate::view::View;

const SIZE: [u32; 2] = [300, 200];

fn pixel(controller: &CameraController, scene: Option<Sphere>, point: Vec3) -> [f64; 2] {
    let camera = controller.camera(scene);
    let view = View::new(&camera, SIZE);
    let [x, y, _] = view.project(sub(point, view.eye)).unwrap();
    [x, y]
}

fn close(a: [f64; 2], b: [f64; 2], tolerance: f64) -> bool {
    (a[0] - b[0]).abs() <= tolerance && (a[1] - b[1]).abs() <= tolerance
}

fn far_scene() -> Sphere {
    Sphere {
        centre: [1e7, -1e7, 3.0],
        radius: 50.0,
    }
}

fn looking_at(sphere: Sphere) -> CameraController {
    let mut controller = CameraController::default();
    controller.fit(sphere, 1.5);
    controller
}

#[test]
fn the_default_camera_is_valid_and_looks_from_the_south_east() {
    let controller = CameraController::default();
    let camera = controller.camera(None);
    assert_eq!(camera.validate(), Ok(()));
    assert!(camera.eye.x > 0.0 && camera.eye.y < 0.0 && camera.eye.z > 0.0);
    assert_eq!(controller.mode(), ProjectionMode::Perspective);
}

#[test]
fn zooming_keeps_the_point_under_the_pointer() {
    for mode in [ProjectionMode::Perspective, ProjectionMode::Parallel] {
        let scene = far_scene();
        let mut controller = looking_at(scene);
        controller.set_mode(mode, Some(scene));
        let anchor = add(scene.centre, [10.0, 5.0, -2.0]);
        let before = pixel(&controller, Some(scene), anchor);
        controller.zoom(0.5, anchor, Some(scene));
        assert!(
            close(pixel(&controller, Some(scene), anchor), before, 1e-6),
            "{mode:?}"
        );
        controller.zoom(3.0, anchor, Some(scene));
        assert!(
            close(pixel(&controller, Some(scene), anchor), before, 1e-6),
            "{mode:?}"
        );
        // A factor that means nothing changes nothing.
        let held = controller;
        for factor in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            controller.zoom(factor, anchor, Some(scene));
        }
        assert_eq!(controller, held);
    }
}

#[test]
fn zooming_stops_at_its_bounds() {
    let mut controller = CameraController::default();
    for _ in 0..200 {
        controller.zoom(0.1, [0.0; 3], None);
    }
    assert!((controller.distance() - MIN_DISTANCE).abs() < 1e-12);
    for _ in 0..200 {
        controller.zoom(10.0, controller.target(), None);
    }
    assert!(controller.distance() <= MAX_DISTANCE);
    assert_eq!(controller.camera(None).validate(), Ok(()));
}

#[test]
fn orbiting_keeps_the_pivot_on_its_pixel() {
    let scene = far_scene();
    let mut controller = looking_at(scene);
    let pivot = add(scene.centre, [3.0, -4.0, 1.0]);
    let before = pixel(&controller, Some(scene), pivot);
    controller.orbit(0.4, -0.2, pivot, Some(scene));
    assert!(close(pixel(&controller, Some(scene), pivot), before, 1e-6));
    // Straight down at most: the pitch stops at the vertical, the camera stays valid.
    controller.orbit(0.0, -10.0, pivot, Some(scene));
    assert!((controller.pitch() + FRAC_PI_2).abs() < 1e-12);
    assert_eq!(controller.camera(Some(scene)).validate(), Ok(()));
    controller.orbit(0.0, 20.0, pivot, Some(scene));
    assert!((controller.pitch() - FRAC_PI_2).abs() < 1e-12);
}

#[test]
fn panning_moves_the_view_in_the_screen_plane() {
    let mut controller = CameraController::default();
    let target = controller.target();
    let forward = controller.forward();
    controller.pan(2.0, -1.0, None);
    let moved = sub(controller.target(), target);
    assert!((norm(moved) - libm::sqrt(5.0)).abs() < 1e-12);
    assert!(dot(moved, forward).abs() < 1e-12);
    // Panning right moves the target to the right of the screen.
    let before = pixel(&CameraController::default(), None, target);
    let after = pixel(&controller, None, target);
    assert!(
        after[0] < before[0] && after[1] < before[1],
        "{before:?} {after:?}"
    );
}

#[test]
fn a_fitted_view_shows_the_whole_sphere() {
    for aspect in [0.5, 1.5] {
        let size = if aspect < 1.0 { [200, 400] } else { [300, 200] };
        for mode in [ProjectionMode::Perspective, ProjectionMode::Parallel] {
            let sphere = far_scene();
            let mut controller = CameraController::default();
            controller.set_mode(mode, Some(sphere));
            controller.fit(sphere, aspect);
            let camera = controller.camera(Some(sphere));
            let view = View::new(&camera, size);
            for axis in 0..3 {
                for sign in [-1.0, 1.0] {
                    let mut point = sphere.centre;
                    point[axis] += sign * sphere.radius;
                    let [x, y, _] = view.project(sub(point, view.eye)).unwrap();
                    let inside = (0.0..=f64::from(size[0])).contains(&x)
                        && (0.0..=f64::from(size[1])).contains(&y);
                    assert!(inside, "{aspect} {mode:?}: {x} {y}");
                }
            }
        }
    }
}

#[test]
fn presets_look_the_way_they_say() {
    let expected: [(Preset, Vec3); 9] = [
        (Preset::Top, [0.0, 0.0, -1.0]),
        (Preset::Front, [0.0, 1.0, 0.0]),
        (Preset::Back, [0.0, -1.0, 0.0]),
        (Preset::Left, [1.0, 0.0, 0.0]),
        (Preset::Right, [-1.0, 0.0, 0.0]),
        (Preset::IsoSe, [-1.0, 1.0, -1.0]),
        (Preset::IsoSw, [1.0, 1.0, -1.0]),
        (Preset::IsoNe, [-1.0, -1.0, -1.0]),
        (Preset::IsoNw, [1.0, -1.0, -1.0]),
    ];
    for (preset, direction) in expected {
        let mut controller = CameraController::default();
        controller.preset(preset, None);
        let direction = scale(direction, 1.0 / norm(direction));
        assert!(
            norm(sub(controller.forward(), direction)) < 1e-12,
            "{preset:?}"
        );
        assert_eq!(controller.camera(None).validate(), Ok(()));
    }
    // From the top, north is up on the screen.
    let mut controller = CameraController::default();
    controller.preset(Preset::Top, None);
    let north = pixel(&controller, None, [0.0, 5.0, 0.0]);
    let south = pixel(&controller, None, [0.0, -5.0, 0.0]);
    assert!(north[1] < south[1] && (north[0] - south[0]).abs() < 1e-9);
}

#[test]
fn switching_the_projection_keeps_the_size_at_the_target() {
    let scene = far_scene();
    let mut controller = looking_at(scene);
    let edge = add(
        scene.centre,
        scale(cross(controller.forward(), [0.0, 0.0, 1.0]), 10.0),
    );
    let width = |c: &CameraController| {
        pixel(c, Some(scene), edge)[0] - pixel(c, Some(scene), scene.centre)[0]
    };
    let perspective = width(&controller);
    controller.set_mode(ProjectionMode::Parallel, Some(scene));
    assert_eq!(controller.mode(), ProjectionMode::Parallel);
    assert!((width(&controller) - perspective).abs() < 1e-6 * perspective.abs());
    controller.set_mode(ProjectionMode::Perspective, Some(scene));
    assert!((width(&controller) - perspective).abs() < 1e-6 * perspective.abs());
}

#[test]
fn an_invalid_step_keeps_the_camera() {
    let mut controller = CameraController::default();
    let held = controller;
    controller.pan(f64::NAN, 0.0, None);
    controller.orbit(0.1, 0.0, [f64::INFINITY; 3], None);
    controller.pan(2.0 * MAX_WORLD_COORDINATE, 0.0, None);
    assert_eq!(controller, held);
}

#[test]
fn angles_wrap_into_a_half_open_turn() {
    assert!((wrap(3.0 * PI) - PI).abs() < 1e-12);
    assert!((wrap(-PI) - PI).abs() < 1e-12);
    assert!((wrap(0.5) - 0.5).abs() < 1e-15);
}

#[derive(Clone, Debug)]
enum Operation {
    Orbit(f64, f64, Vec3),
    Pan(f64, f64),
    Zoom(f64, Vec3),
    Fit(Vec3, f64, f64),
    Preset(usize),
    Mode(bool),
}

fn point() -> impl Strategy<Value = Vec3> {
    prop::array::uniform3(-1e7..1e7_f64)
}

fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (-10.0..10.0_f64, -10.0..10.0_f64, point())
            .prop_map(|(y, p, at)| Operation::Orbit(y, p, at)),
        (-1e6..1e6_f64, -1e6..1e6_f64).prop_map(|(r, u)| Operation::Pan(r, u)),
        (1e-3..1e3_f64, point()).prop_map(|(f, at)| Operation::Zoom(f, at)),
        (point(), 0.0..1e5_f64, 0.1..10.0_f64).prop_map(|(c, r, a)| Operation::Fit(c, r, a)),
        (0..9_usize).prop_map(Operation::Preset),
        any::<bool>().prop_map(Operation::Mode),
    ]
}

proptest! {
    #![proptest_config(crate::proptest_config())]

    /// Whatever the user does, the camera stays valid and never upside down.
    #[test]
    fn every_sequence_of_operations_keeps_a_valid_camera(operations in prop::collection::vec(operation(), 1..40)) {
        let scene = Some(far_scene());
        let presets = [
            Preset::Top, Preset::Front, Preset::Back, Preset::Left, Preset::Right,
            Preset::IsoSe, Preset::IsoSw, Preset::IsoNe, Preset::IsoNw,
        ];
        let mut controller = CameraController::default();
        for operation in operations {
            match operation {
                Operation::Orbit(yaw, pitch, pivot) => controller.orbit(yaw, pitch, pivot, scene),
                Operation::Pan(right, up) => controller.pan(right, up, scene),
                Operation::Zoom(factor, anchor) => controller.zoom(factor, anchor, scene),
                Operation::Fit(centre, radius, aspect) => controller.fit(Sphere { centre, radius }, aspect),
                Operation::Preset(index) => controller.preset(presets[index], scene),
                Operation::Mode(parallel) => controller.set_mode(
                    if parallel { ProjectionMode::Parallel } else { ProjectionMode::Perspective },
                    scene,
                ),
            }
            prop_assert_eq!(controller.camera(scene).validate(), Ok(()));
            prop_assert!(controller.pitch().abs() <= FRAC_PI_2);
            prop_assert!((MIN_DISTANCE..=MAX_DISTANCE).contains(&controller.distance()));
            let up = controller.camera(scene).up;
            prop_assert!(up.z >= -1e-12);
        }
    }
}
