use geom::{Point3, Vector3};
use render::Projection;

use super::*;
use crate::math::norm;
use crate::test_support::{FakeSink, cubes, id};

const SIZE: [u32; 2] = [200, 200];

/// Unit cubes at `positions` (offset by `offset`), seen from 10 m south of `offset`, looking
/// north.
fn scene(
    offset: f64,
    positions: &[Vec3],
    projection: Projection,
) -> (SceneCache, FakeSink, Camera) {
    let (mut cache, mut sink) = (SceneCache::default(), FakeSink::default());
    cache
        .apply(&cubes(1, [offset; 3], positions), &mut sink)
        .unwrap();
    let camera = Camera {
        eye: Point3::new(offset, offset - 10.0, offset),
        target: Point3::new(offset, offset, offset),
        up: Vector3::new(0.0, 0.0, 1.0),
        projection,
    };
    (cache, sink, camera)
}

const PERSPECTIVE: Projection = Projection::Perspective {
    fov_y_radians: 1.0,
    near: 0.1,
};

const PARALLEL: Projection = Projection::Orthographic {
    height: 8.0,
    near: 0.1,
    far: 100.0,
};

fn pick_in(
    (cache, sink, camera): &mut (SceneCache, FakeSink, Camera),
    pos: [f64; 2],
    pickable: &dyn Pickable,
) -> Option<(Hit, Vec3)> {
    let hit = pick(cache, sink, camera, SIZE, pos, pickable)?;
    let eye = [camera.eye.x, camera.eye.y, camera.eye.z];
    Some((hit, add(eye, hit.relative)))
}

#[test]
fn the_nearest_face_under_the_pointer_is_picked_at_any_distance_from_the_origin() {
    for projection in [PERSPECTIVE, PARALLEL] {
        for (offset, tolerance) in [(0.0, 1e-12), (1e6, 1e-9), (1e7, 1e-8)] {
            // A cube in front of another.
            let mut scene = scene(offset, &[[0.0; 3], [0.0, 3.0, 0.0]], projection);
            let (hit, point) = pick_in(&mut scene, [100.0, 100.0], &Everything).unwrap();
            assert_eq!(hit.element, id(1));
            let local = sub(point, [offset; 3]);
            assert!(
                norm(sub(local, [0.0, -0.5, 0.0])) < tolerance,
                "{projection:?} {offset}: {local:?}"
            );
            assert!(norm(sub(hit.normal, [0.0, -1.0, 0.0])) < 1e-12);
            assert!(
                (hit.t - 9.5).abs() < tolerance
                    || matches!(projection, Projection::Orthographic { .. })
            );
            // Without the front one among the candidates, or refusing it, the one behind.
            let view = View::new(&scene.2, SIZE);
            let ray = view.ray([100.0, 100.0]);
            let behind = nearest(&scene.0, &view, &ray, &[id(2)], &Everything, f64::INFINITY);
            assert_eq!(behind.map(|hit| hit.element), Some(id(2)));
            // Nothing before a limit short of the front face.
            let short = nearest(
                &scene.0,
                &view,
                &ray,
                &[id(1), id(2)],
                &Everything,
                hit.t - 0.1,
            );
            assert_eq!(short, None);
            let (refused, _) = pick_in(&mut scene, [100.0, 100.0], &Not(id(1))).unwrap();
            assert_eq!(refused.element, id(2));
            // Off the cubes, nothing.
            assert_eq!(pick_in(&mut scene, [5.0, 5.0], &Everything), None);
        }
    }
}

#[test]
fn faces_are_picked_from_behind_too_and_the_normal_faces_the_eye() {
    // The eye inside a cube: its far face is hit from the inside.
    let mut scene = scene(0.0, &[[0.0, -10.0, 0.0]], PERSPECTIVE);
    let (hit, point) = pick_in(&mut scene, [100.0, 100.0], &Everything).unwrap();
    assert!(norm(sub(point, [0.0, -9.5, 0.0])) < 1e-12, "{point:?}");
    assert!(norm(sub(hit.normal, [0.0, -1.0, 0.0])) < 1e-12);
}

#[test]
fn a_ray_meets_a_box_within_its_limit() {
    let ray = Ray {
        origin: [0.0; 3],
        direction: [0.0, 1.0, 0.0],
    };
    let (min, max) = ([-1.0, 4.0, -1.0], [1.0, 6.0, 1.0]);
    assert!(ray_meets_box(&ray, min, max, f64::INFINITY));
    assert!(ray_meets_box(&ray, min, max, 4.5));
    assert!(!ray_meets_box(&ray, min, max, 3.0));
    // Beside the box, along an axis the ray does not move on.
    assert!(!ray_meets_box(
        &ray,
        [2.0, 4.0, -1.0],
        [3.0, 6.0, 1.0],
        f64::INFINITY
    ));
    // Behind the origin.
    assert!(!ray_meets_box(
        &ray,
        [-1.0, -6.0, -1.0],
        [1.0, -4.0, 1.0],
        f64::INFINITY
    ));
}

#[test]
fn degenerate_triangles_and_parallel_rays_miss() {
    let ray = Ray {
        origin: [0.0; 3],
        direction: [0.0, 1.0, 0.0],
    };
    let flat = [[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0]];
    assert_eq!(intersect(&ray, &flat), None);
    let along = [[0.0, 1.0, -1.0], [0.0, 2.0, 1.0], [0.0, 3.0, -1.0]];
    assert_eq!(intersect(&ray, &along), None);
    let behind = [[-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [0.0, -1.0, 1.0]];
    assert_eq!(intersect(&ray, &behind), None);
    let beside = [[1.0, 1.0, -1.0], [3.0, 1.0, -1.0], [2.0, 1.0, 1.0]];
    assert_eq!(intersect(&ray, &beside), None);
    let above = [[-1.0, 1.0, 0.5], [1.0, 1.0, 0.5], [0.0, 1.0, 2.0]];
    assert_eq!(intersect(&ray, &above), None);
}

#[test]
fn an_element_the_cache_lacks_is_ignored() {
    let scene = scene(0.0, &[[0.0; 3]], PERSPECTIVE);
    let view = View::new(&scene.2, SIZE);
    let ray = view.ray([100.0, 100.0]);
    assert_eq!(
        nearest(&scene.0, &view, &ray, &[id(9)], &Everything, f64::INFINITY),
        None
    );
}

/// Refuses one element.
struct Not(ElementId);

impl Pickable for Not {
    fn pickable(&self, element: ElementId, _point: Vec3) -> bool {
        element != self.0
    }
}
