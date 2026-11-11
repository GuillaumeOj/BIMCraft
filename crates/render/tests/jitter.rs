//! The visual test of VP-04 and GEO-07: a model 10⁶ m and 10⁷ m from the world origin renders
//! like the same model at the origin, without jitter while the camera moves
//! (`docs/dev/adr/0020-camera-relative-rendering.md`, spec of #68, D4).
//!
//! A scene with fine detail (a comb of 5 mm slats 5 mm apart, and a turned cube) is seen from
//! about 3 m while the camera moves by 0.1 mm per frame. The sweep is rendered with the scene and
//! the camera at the origin, then translated far away; every translated frame must match the
//! frame of the same step at the origin. A negative control renders the sweep at 10⁶ m with the
//! coordinates measured from the world origin, as a classic pipeline does: it must fail, which
//! proves the comparison sees jitter.
//!
//! The first frame of each sweep, and every frame that should match and does not (with its
//! reference), are written as PPM images to
//! `target/render-jitter/` (or `target/llvm-cov-target/render-jitter/` under coverage).

// Helpers of a test: a failure is a test failure.
#![allow(clippy::panic, clippy::unwrap_used)]

mod support;

use std::path::PathBuf;

use geom::{Point3, Vector3};
use render::{Camera, Placement, Projection, Renderer};
use support::{Boxes, FORMAT, Target};

/// Side of the square frames, in pixels.
const SIZE: u32 = 256;

/// Number of frames of the camera sweep.
const FRAMES: u32 = 16;

/// Camera movement between two frames, in metres: about 0.03 pixel at the comb.
const STEP: [f64; 3] = [1e-4, 0.5e-4, 0.25e-4];

/// The narrow field of view makes a pixel about 3 mm wide at the comb, so 5 mm slats span
/// a pixel and a half and a shift of a pixel is visible.
const FIELD_OF_VIEW: f64 = 15.0 * core::f64::consts::PI / 180.0;

/// Width of a slat and of the gap between two slats, in metres.
const SLAT: f64 = 0.005;

/// The largest difference of a channel between a frame and its reference: 1/255.
const MAX_CHANNEL_DIFFERENCE: u8 = 1;

/// The largest share of pixels that may differ between a frame and its reference: 0.01 %.
const MAX_DIFFERING_SHARE: f64 = 1e-4;

/// Background colour, linear RGBA.
const CLEAR: [f64; 4] = [0.05, 0.05, 0.08, 1.0];

/// How a sweep differs from the sweep at the origin, over all its frames.
#[derive(Debug, Default)]
struct Comparison {
    /// The largest difference of a channel, in 1/255.
    max_difference: u8,
    /// The largest share of differing pixels in a frame.
    max_differing_share: f64,
    /// Frames that fail the criterion.
    failing_frames: Vec<u32>,
}

impl Comparison {
    fn passes(&self) -> bool {
        self.failing_frames.is_empty()
    }
}

/// The scene and the camera sweep, translated by `shift`.
struct Scene {
    renderer: Renderer,
    shift: [f64; 3],
}

impl Scene {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, shift: [f64; 3]) -> Self {
        let mut renderer = Renderer::new(device, queue, FORMAT).unwrap();
        // The comb, built from world coordinates: its mesh origin is far away with the scene.
        let mut comb = Boxes::default();
        for slat in 0..40 {
            let x = -0.2 + f64::from(slat) * 2.0 * SLAT;
            comb.add(
                add(shift, [x, -0.01, 0.0]),
                add(shift, [x + SLAT, 0.01, 0.6]),
            );
        }
        let comb = renderer.add_mesh(&comb.mesh()).unwrap();
        renderer
            .add_object(comb, Placement::IDENTITY, [0.9, 0.75, 0.4, 1.0])
            .unwrap();
        // The cube, built at the origin and placed far away, turned by 30° about the vertical.
        let mut cube = Boxes::default();
        cube.add([-0.075; 3], [0.075; 3]);
        let cube = renderer.add_mesh(&cube.mesh()).unwrap();
        let (sin, cos) = libm::sincos(core::f64::consts::FRAC_PI_6);
        let placement = Placement {
            rotation: [[cos, -sin, 0.0], [sin, cos, 0.0], [0.0, 0.0, 1.0]],
            translation: add(shift, [0.32, 0.05, 0.3]),
        };
        renderer
            .add_object(cube, placement, [0.3, 0.6, 0.9, 1.0])
            .unwrap();
        Self { renderer, shift }
    }

    fn camera(&self, frame: u32) -> Camera {
        let offset = STEP.map(|step| step * f64::from(frame));
        let eye = add(add(self.shift, [0.1, -3.0, 0.8]), offset);
        let target = add(add(self.shift, [0.05, 0.0, 0.3]), offset);
        Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(target[0], target[1], target[2]),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: Projection::Perspective {
                fov_y_radians: FIELD_OF_VIEW,
                near: 0.1,
            },
        }
    }

    /// Every frame of the sweep, as RGBA bytes.
    fn sweep(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) -> Vec<Vec<u8>> {
        let target = Target::new(device, [SIZE; 2]);
        (0..FRAMES)
            .map(|frame| {
                let camera = self.camera(frame);
                self.renderer
                    .render(&target.view, target.size, &camera, CLEAR)
                    .unwrap();
                target.read(device, queue)
            })
            .collect()
    }
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

/// The largest channel difference and the number of differing pixels between two frames.
fn difference(a: &[u8], b: &[u8]) -> (u8, usize) {
    a.as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0)
        .fold((0, 0), |(max, count), (p, q)| {
            let pixel = p
                .iter()
                .zip(q)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            (max.max(pixel), count + usize::from(pixel > 0))
        })
}

/// Compares a sweep with the reference sweep, frame by frame, and writes its first frame as an
/// image named after `label`. When the sweep should match (`expect_match`), every failing frame
/// is written too, with its reference.
fn compare(
    label: &str,
    reference: &[Vec<u8>],
    frames: &[Vec<u8>],
    expect_match: bool,
) -> Comparison {
    let mut comparison = Comparison::default();
    let pixels = f64::from(SIZE * SIZE);
    for (frame, (expected, actual)) in (0..).zip(reference.iter().zip(frames)) {
        let (max, count) = difference(expected, actual);
        #[expect(clippy::cast_precision_loss, reason = "at most 65 536 pixels")]
        let share = count as f64 / pixels;
        comparison.max_difference = comparison.max_difference.max(max);
        comparison.max_differing_share = comparison.max_differing_share.max(share);
        let fails = max > MAX_CHANNEL_DIFFERENCE || share > MAX_DIFFERING_SHARE;
        if fails {
            comparison.failing_frames.push(frame);
        }
        if fails && expect_match {
            write_frame(&format!("{label}-{frame:02}-reference"), expected);
        }
        if (fails && expect_match) || frame == 0 {
            write_frame(&format!("{label}-{frame:02}"), actual);
        }
    }
    report(label, &comparison);
    comparison
}

/// Where the images go: `render-jitter/` in the target directory.
fn output_dir() -> PathBuf {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    tmp.parent().unwrap_or(&tmp).join("render-jitter")
}

/// Writes `pixels` as a binary PPM image, a format any image viewer opens.
#[expect(
    clippy::disallowed_methods,
    reason = "a native test writing its artefacts, not core code (ARCH-02)"
)]
fn write_frame(name: &str, pixels: &[u8]) {
    let dir = output_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut image = format!("P6\n{SIZE} {SIZE}\n255\n").into_bytes();
    image.extend(pixels.as_chunks::<4>().0.iter().flat_map(|p| &p[..3]));
    std::fs::write(dir.join(format!("{name}.ppm")), image).unwrap();
}

/// Prints the numbers of a comparison, for the pull request and for a failing run.
#[expect(
    clippy::print_stderr,
    reason = "the test reports its measures; cargo shows them with --nocapture"
)]
fn report(label: &str, comparison: &Comparison) {
    eprintln!(
        "{label}: max channel difference {}/255, at most {:.4} % differing pixels per frame, \
         {} failing frames of {FRAMES}",
        comparison.max_difference,
        comparison.max_differing_share * 100.0,
        comparison.failing_frames.len(),
    );
}

#[test]
fn far_from_the_origin_the_frames_match_the_frames_at_the_origin() {
    let (device, queue) = support::device();
    let reference = Scene::new(&device, &queue, [0.0; 3]).sweep(&device, &queue);
    write_frame("origin-00", &reference[0]);

    // The scene is visible, and the sweep moves it on the screen: the test can see a shift.
    // The top left corner shows the background.
    let first = &reference[0];
    let background = first[..4].repeat(first.len() / 4);
    let (_, covered) = difference(first, &background);
    assert!(covered > 5_000, "the scene covers {covered} pixels only");
    let moved = difference(&reference[0], &reference[reference.len() - 1]);
    assert!(moved.1 > 0, "the camera sweep does not change the image");

    let mut failures = Vec::new();
    for (label, shift) in [("1e6", [1e6, 1e6, 1e2]), ("1e7", [1e7, 1e7, 1e2])] {
        let frames = Scene::new(&device, &queue, shift).sweep(&device, &queue);
        let comparison = compare(label, &reference, &frames, true);
        if !comparison.passes() {
            failures.push(format!(
                "{label}: frames {:?} differ from the origin (max {}/255, {:.4} % of pixels)",
                comparison.failing_frames,
                comparison.max_difference,
                comparison.max_differing_share * 100.0
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "jitter far from the origin; images in {}:\n{}",
        output_dir().display(),
        failures.join("\n")
    );

    // Negative control: measuring coordinates from the world origin rounds the vertices to the
    // 6.25 cm grid of f32 at 10⁶ m, which the comparison must see.
    let mut naive = Scene::new(&device, &queue, [1e6, 1e6, 1e2]);
    naive.renderer.measure_from_world_origin();
    let frames = naive.sweep(&device, &queue);
    let comparison = compare("1e6-naive", &reference, &frames, false);
    assert!(
        comparison.failing_frames.len() == reference.len(),
        "the negative control renders frames without jitter, so the test cannot detect it"
    );
}
