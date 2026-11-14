//! Picking in every view preset and both projections, over the blocks of the e2e fixture (VP-01).
//!
//! Where a block is drawn and what lies under a pixel are worked out here from the camera alone,
//! as `render` builds `P · V`, not through the viewport's own ray and projection code.

use render::Projection;

use super::*;
use crate::cache::world_bounds;
use crate::math::{cross, normalize, scale};

/// The full scene of the e2e tests (`ui/e2e/fixtures/README.md`): blocks 1 and 2 are 4 × 2 × 3 m
/// at (0, 0, 0) and (6, 0, 0), block 3 is 2 × 2 × 6 m at (0, 6, 0), turned by a quarter turn.
const BLOCKS: &[u8] = include_bytes!("../../../../../ui/e2e/fixtures/scene-blocks.bin");

const PRESETS: [Preset; 9] = [
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

/// Where a block is sampled along each axis, its centre first.
const STEPS: [f64; 5] = [0.5, 0.1, 0.3, 0.7, 0.9];

/// The camera's axes and screen.
struct Screen {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    projection: Projection,
    size: [f64; 2],
}

impl Screen {
    fn new(camera: &Camera, size: [u32; 2]) -> Self {
        let eye = [camera.eye.x, camera.eye.y, camera.eye.z];
        let target = [camera.target.x, camera.target.y, camera.target.z];
        let forward = normalize(sub(target, eye)).unwrap();
        let right = normalize(cross(forward, [camera.up.x, camera.up.y, camera.up.z])).unwrap();
        Self {
            eye,
            right,
            up: cross(right, forward),
            forward,
            projection: camera.projection,
            size: size.map(f64::from),
        }
    }

    /// Half the view in x and y: at 1 m in perspective, in metres in parallel.
    fn half(&self) -> [f64; 2] {
        let half_y = match self.projection {
            Projection::Perspective { fov_y_radians, .. } => libm::tan(fov_y_radians / 2.0),
            Projection::Orthographic { height, .. } => height / 2.0,
        };
        [half_y * self.size[0] / self.size[1], half_y]
    }

    /// The pixel where `point` is drawn.
    fn pixel(&self, point: Vec3) -> [f64; 2] {
        let relative = sub(point, self.eye);
        let depth = match self.projection {
            Projection::Perspective { .. } => dot(relative, self.forward),
            Projection::Orthographic { .. } => 1.0,
        };
        let [half_x, half_y] = self.half();
        let ndc_x = dot(relative, self.right) / depth / half_x;
        let ndc_y = dot(relative, self.up) / depth / half_y;
        [
            f64::midpoint(ndc_x, 1.0) * self.size[0],
            f64::midpoint(1.0, -ndc_y) * self.size[1],
        ]
    }

    /// The world ray drawn at `pixel`: from the eye in perspective, from the image plane in
    /// parallel.
    fn ray(&self, pixel: [f64; 2]) -> (Vec3, Vec3) {
        let [half_x, half_y] = self.half();
        let x = (2.0 * pixel[0] / self.size[0] - 1.0) * half_x;
        let y = (1.0 - 2.0 * pixel[1] / self.size[1]) * half_y;
        let across = add(scale(self.right, x), scale(self.up, y));
        match self.projection {
            Projection::Perspective { .. } => (self.eye, add(self.forward, across)),
            Projection::Orthographic { .. } => (add(self.eye, across), self.forward),
        }
    }

    fn contains(&self, pixel: [f64; 2]) -> bool {
        (0.0..=self.size[0]).contains(&pixel[0]) && (0.0..=self.size[1]).contains(&pixel[1])
    }
}

/// Where the ray enters the box, if it meets it.
fn enters(origin: Vec3, direction: Vec3, [min, max]: [Vec3; 2]) -> Option<f64> {
    let (mut enter, mut leave) = (0.0_f64, f64::INFINITY);
    for axis in 0..3 {
        if direction[axis] == 0.0 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
            continue;
        }
        let t0 = (min[axis] - origin[axis]) / direction[axis];
        let t1 = (max[axis] - origin[axis]) / direction[axis];
        enter = enter.max(t0.min(t1));
        leave = leave.min(t0.max(t1));
    }
    (enter <= leave).then_some(enter)
}

/// The world boxes of elements 1, 2 and 3: the blocks are axis-aligned, so these are their
/// shapes.
fn boxes(viewport: &Viewport<FakeSink>) -> Vec<(ElementId, [Vec3; 2])> {
    (1..=3)
        .map(|n| {
            let (element, mesh) = viewport.cache.element(id(n)).unwrap();
            let (min, max) = world_bounds(element, mesh);
            (id(n), [min, max])
        })
        .collect()
}

/// The block drawn at `pixel`.
fn drawn(screen: &Screen, boxes: &[(ElementId, [Vec3; 2])], pixel: [f64; 2]) -> Option<ElementId> {
    let (origin, direction) = screen.ray(pixel);
    boxes
        .iter()
        .filter_map(|&(id, bounds)| Some((enters(origin, direction, bounds)?, id)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// A pixel where `block` is drawn with every pixel 2 px around it, among points inside the
/// block, its centre first: `None` when another block hides it.
fn visible_pixel(
    screen: &Screen,
    boxes: &[(ElementId, [Vec3; 2])],
    block: ElementId,
) -> Option<[f64; 2]> {
    let [min, max] = boxes.iter().find(|(id, _)| *id == block)?.1;
    let points = STEPS.iter().flat_map(|&u| {
        STEPS.iter().flat_map(move |&v| {
            STEPS.iter().map(move |&w| {
                let along = [u, v, w];
                core::array::from_fn(|a| min[a] + (max[a] - min[a]) * along[a])
            })
        })
    });
    points.map(|point| screen.pixel(point)).find(|&pixel| {
        [[0.0, 0.0], [-2.0, 0.0], [2.0, 0.0], [0.0, -2.0], [0.0, 2.0]]
            .iter()
            .all(|&[dx, dy]| drawn(screen, boxes, offset(pixel, dx, dy)) == Some(block))
    })
}

#[test]
fn a_click_on_each_block_selects_it_in_every_view() {
    for mode in [ProjectionMode::Perspective, ProjectionMode::Parallel] {
        for preset in PRESETS {
            let mut viewport = Viewport::new(FakeSink::default(), SIZE, 1.0);
            viewport.apply_scene(BLOCKS).unwrap();
            viewport.command(ViewCommand::Projection(mode));
            viewport.command(ViewCommand::ViewPreset(preset));
            viewport.command(ViewCommand::ZoomFit);
            let screen = Screen::new(&viewport.camera(), viewport.size());
            let boxes = boxes(&viewport);
            let mut hidden = Vec::new();
            for &(block, [min, max]) in &boxes {
                // Zoom fit frames every corner of every block.
                for corner in 0..8 {
                    let point =
                        core::array::from_fn(
                            |a| if corner >> a & 1 == 0 { min[a] } else { max[a] },
                        );
                    let pixel = screen.pixel(point);
                    assert!(
                        screen.contains(pixel),
                        "{mode:?} {preset:?}: {pixel:?} out of the frame"
                    );
                }
                let Some(pixel) = visible_pixel(&screen, &boxes, block) else {
                    hidden.push(block);
                    continue;
                };
                click(&mut viewport, pixel, NONE);
                assert_eq!(
                    viewport.selection(),
                    [block],
                    "{mode:?} {preset:?} at {pixel:?}"
                );
            }
            // In parallel, from the west, block 1 hides block 2 behind it, and the other way round
            // from the east. In perspective the eye is off their axis and the far one peeks out.
            let expected = match (mode, preset) {
                (ProjectionMode::Parallel, Preset::Left) => vec![id(2)],
                (ProjectionMode::Parallel, Preset::Right) => vec![id(1)],
                _ => Vec::new(),
            };
            assert_eq!(hidden, expected, "{mode:?} {preset:?}");
        }
    }
}

#[test]
fn the_top_view_has_north_up_and_east_right() {
    for mode in [ProjectionMode::Perspective, ProjectionMode::Parallel] {
        let mut viewport = Viewport::new(FakeSink::default(), SIZE, 1.0);
        viewport.apply_scene(BLOCKS).unwrap();
        viewport.command(ViewCommand::Projection(mode));
        viewport.command(ViewCommand::ViewPreset(Preset::Top));
        viewport.command(ViewCommand::ZoomFit);
        let screen = Screen::new(&viewport.camera(), viewport.size());
        let centre = |n: usize| {
            let [min, max] = boxes(&viewport)[n].1;
            screen.pixel(scale(add(min, max), 0.5))
        };
        // Block 2 is east of block 1: to its right, on the same row.
        assert!(centre(1)[0] > centre(0)[0] + 1.0, "{mode:?}");
        assert!((centre(1)[1] - centre(0)[1]).abs() < 1e-6, "{mode:?}");
        // Block 3 is north of block 1: above it on the screen (y down).
        assert!(centre(2)[1] < centre(0)[1] - 1.0, "{mode:?}");
    }
}
