#![expect(clippy::float_cmp, reason = "small integers, exact in f64")]

use geom::{Point3, Vector3};
use render::Projection;

use super::*;
use crate::pick::Everything;
use crate::test_support::{FakeSink, cubes, id};

const SIZE: [u32; 2] = [400, 400];
const TOLERANCE: f64 = 10.0;

/// A unit cube standing on the ground at (`offset`, `offset`, 0), and optionally a second one
/// east of it, seen from the south-east, above, about 4 m away.
struct Scene {
    cache: SceneCache,
    sink: FakeSink,
    camera: Camera,
    offset: Vec3,
}

impl Scene {
    fn new(offset: f64, positions: &[Vec3], projection: Projection) -> Self {
        let offset = [offset, offset, 0.0];
        let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
        cache
            .apply(&cubes(1, offset, positions), &mut sink)
            .unwrap();
        let eye = add(offset, [1.5, -3.0, 2.0]);
        let target = add(offset, [0.0, 0.0, 0.5]);
        let camera = Camera {
            eye: Point3::new(eye[0], eye[1], eye[2]),
            target: Point3::new(target[0], target[1], target[2]),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection,
        };
        Self {
            cache,
            sink,
            camera,
            offset,
        }
    }

    fn one(offset: f64) -> Self {
        Self::new(offset, &[[0.0, 0.0, 0.5]], PERSPECTIVE)
    }

    /// The pixel of a point given relative to the offset.
    fn pixel(&self, local: Vec3) -> [f64; 2] {
        let view = View::new(&self.camera, SIZE);
        let [x, y, _] = view
            .project(sub(add(self.offset, local), view.eye))
            .unwrap();
        [x, y]
    }

    fn snap(
        &mut self,
        pos: [f64; 2],
        enabled: bool,
        skip: &dyn Fn(ElementId) -> bool,
    ) -> Option<(Option<SnapKind>, Vec3)> {
        let query = Query {
            cache: &self.cache,
            camera: &self.camera,
            size: SIZE,
            pos,
            tolerance: TOLERANCE,
            enabled,
            skip,
            pickable: &Everything,
        };
        let snap = snap(&query, &mut self.sink)?;
        Some((snap.kind, sub(snap.point, self.offset)))
    }

    fn at(&mut self, pos: [f64; 2]) -> Option<(Option<SnapKind>, Vec3)> {
        self.snap(pos, true, &|_| false)
    }
}

const PERSPECTIVE: Projection = Projection::Perspective {
    fov_y_radians: 1.0,
    near: 0.01,
};

/// The top corner nearest the eye, and the far bottom corner, hidden behind the cube.
const FRONT_TOP: Vec3 = [0.5, -0.5, 1.0];
const HIDDEN: Vec3 = [-0.5, 0.5, 0.0];

fn close(a: Vec3, b: Vec3, tolerance: f64) -> bool {
    norm(sub(a, b)) <= tolerance
}

#[test]
fn vertices_come_before_midpoints_edges_faces_and_the_ground() {
    for (offset, tolerance) in [(0.0, 1e-12), (1e6, 1e-9), (1e7, 1e-8)] {
        let mut scene = Scene::one(offset);
        let corner = scene.pixel(FRONT_TOP);
        // Near a vertex.
        let (kind, point) = scene.at([corner[0] + 3.0, corner[1] + 2.0]).unwrap();
        assert_eq!(kind, Some(SnapKind::Endpoint), "{offset}");
        assert!(close(point, FRONT_TOP, tolerance), "{point:?}");
        // Near the middle of the top front edge.
        let middle = scene.pixel([0.0, -0.5, 1.0]);
        let (kind, point) = scene.at([middle[0], middle[1] + 2.0]).unwrap();
        assert_eq!(kind, Some(SnapKind::Midpoint));
        assert!(close(point, [0.0, -0.5, 1.0], tolerance), "{point:?}");
        // Along that edge, far from both: the nearest point of the edge.
        let quarter = scene.pixel([0.25, -0.5, 1.0]);
        let (kind, point) = scene.at([quarter[0], quarter[1] + 1.0]).unwrap();
        assert_eq!(kind, Some(SnapKind::Edge));
        assert!(
            (point[1] + 0.5).abs() < 1e-6 && (point[2] - 1.0).abs() < 1e-6,
            "{point:?}"
        );
        assert!((point[0] - 0.25).abs() < 0.02, "{point:?}");
        // In the middle of the front face.
        let face = scene.pixel([0.0, -0.5, 0.5]);
        let (kind, point) = scene.at(face).unwrap();
        assert_eq!(kind, Some(SnapKind::Face));
        assert!((point[1] + 0.5).abs() < 1e-6, "{point:?}");
        // On the ground, away from the cube.
        let ground = scene.pixel([-1.5, -1.0, 0.0]);
        let (kind, point) = scene.at(ground).unwrap();
        assert_eq!(kind, Some(SnapKind::Ground));
        assert!(close(point, [-1.5, -1.0, 0.0], 1e-6), "{point:?}");
        assert!(point[2].abs() < 1e-12);
    }
}

#[test]
fn the_tolerance_is_a_number_of_pixels() {
    let mut scene = Scene::one(0.0);
    // The top corner on the right of the screen: moving right leaves the cube.
    let right = scene.pixel([0.5, 0.5, 1.0]);
    let (kind, _) = scene.at([right[0] + TOLERANCE - 0.1, right[1]]).unwrap();
    assert_eq!(kind, Some(SnapKind::Endpoint));
    let beyond = scene.at([right[0] + TOLERANCE + 0.1, right[1]]);
    assert_ne!(beyond.map(|(kind, _)| kind), Some(Some(SnapKind::Endpoint)));
}

#[test]
fn hidden_vertices_are_never_snapped_to() {
    let mut scene = Scene::one(0.0);
    let hidden = scene.pixel(HIDDEN);
    let (kind, point) = scene.at(hidden).unwrap();
    assert!(!close(point, HIDDEN, 1e-3), "{kind:?} {point:?}");
}

#[test]
fn without_snapping_the_pointer_gives_the_plain_face_or_ground_point() {
    let mut scene = Scene::one(0.0);
    let corner = scene.pixel(FRONT_TOP);
    let near_corner = [corner[0] - 4.0, corner[1] + 4.0];
    let (kind, point) = scene.snap(near_corner, false, &|_| false).unwrap();
    assert_eq!(kind, None);
    assert!(!close(point, FRONT_TOP, 1e-3));
    let ground = scene.pixel([-1.5, -1.0, 0.0]);
    let (kind, point) = scene.snap(ground, false, &|_| false).unwrap();
    assert_eq!(kind, None);
    assert!(close(point, [-1.5, -1.0, 0.0], 1e-6));
    // Looking up at the sky, nothing.
    assert_eq!(scene.snap([200.0, 0.0], false, &|_| false), None);
}

#[test]
fn skipped_elements_are_not_snapped_to() {
    let mut scene = Scene::one(0.0);
    let corner = scene.pixel(FRONT_TOP);
    let (kind, _) = scene.snap(corner, true, &|e| e == id(1)).unwrap();
    assert_eq!(kind, Some(SnapKind::Ground));
}

#[test]
fn a_parallel_view_snaps_too() {
    let projection = Projection::Orthographic {
        height: 3.0,
        near: 0.01,
        far: 100.0,
    };
    for offset in [0.0, 1e7] {
        let mut scene = Scene::new(offset, &[[0.0, 0.0, 0.5]], projection);
        let corner = scene.pixel(FRONT_TOP);
        let (kind, point) = scene.at([corner[0] + 2.0, corner[1]]).unwrap();
        assert_eq!(kind, Some(SnapKind::Endpoint));
        assert!(close(point, FRONT_TOP, 1e-8), "{point:?}");
        let hidden = scene.pixel(HIDDEN);
        let (_, point) = scene.at(hidden).unwrap();
        assert!(!close(point, HIDDEN, 1e-3));
    }
}

#[test]
fn the_ground_out_of_reach_gives_nothing() {
    let view = View::new(
        &Camera {
            eye: Point3::new(0.0, 0.0, 10.0),
            target: Point3::new(0.0, 10.0, 20.0),
            up: Vector3::new(0.0, 0.0, 1.0),
            projection: PERSPECTIVE,
        },
        SIZE,
    );
    assert_eq!(ground(&view, &view.ray([200.0, 200.0])), None);
    let ray = Ray {
        origin: [0.0; 3],
        direction: [0.0, 1.0, 0.0],
    };
    assert_eq!(ground(&view, &ray), None);
}

#[test]
fn the_nearest_point_of_a_segment_is_clamped_to_its_ends() {
    let ray = Ray {
        origin: [0.0; 3],
        direction: [0.0, 1.0, 0.0],
    };
    let (a, b) = ([-1.0, 5.0, 1.0], [1.0, 5.0, 1.0]);
    assert_eq!(closest_on_segment(&ray, a, b), [0.0, 5.0, 1.0]);
    assert_eq!(
        closest_on_segment(&ray, [2.0, 5.0, 0.0], [3.0, 5.0, 0.0]),
        [2.0, 5.0, 0.0]
    );
    // Parallel to the ray: its first end.
    assert_eq!(
        closest_on_segment(&ray, [1.0, 2.0, 0.0], [1.0, 3.0, 0.0]),
        [1.0, 2.0, 0.0]
    );
}

#[test]
fn more_candidates_than_examined_still_snap() {
    // More elements under the pointer than the cap (a dense model): the nearest ones are
    // examined and the corner is still found.
    let positions = vec![[0.0, 0.0, 0.5]; MAX_CANDIDATES + 1];
    let mut scene = Scene::new(0.0, &positions, PERSPECTIVE);
    let corner = scene.pixel(FRONT_TOP);
    let (kind, point) = scene.at(corner).unwrap();
    assert_eq!(kind, Some(SnapKind::Endpoint));
    assert!(close(point, FRONT_TOP, 1e-9));
}
