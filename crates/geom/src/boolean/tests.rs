use super::*;
use crate::{Edge, FaceInput, LINEAR_TOLERANCE, Point2, Point3, Profile, Vector3};

/// Corners of an axis-aligned box; corner `i` has bit 0 for x, bit 1 for y, bit 2 for z.
fn cuboid(min: [f64; 3], max: [f64; 3]) -> Solid {
    let vertices = (0..8)
        .map(|i| {
            let pick = |bit: usize, axis: usize| if i & bit == 0 { min[axis] } else { max[axis] };
            Point3::new(pick(1, 0), pick(2, 1), pick(4, 2))
        })
        .collect();
    let faces: Vec<FaceInput> = [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ]
    .iter()
    .map(|ring| FaceInput {
        outer: ring.to_vec(),
        holes: Vec::new(),
    })
    .collect();
    Solid::from_faces(vertices, &faces).unwrap()
}

/// The box turned about z, then about x.
fn turned(min: [f64; 3], max: [f64; 3], (about_z, about_x): (f64, f64)) -> Solid {
    let cuboid = cuboid(min, max);
    let (sz, cz) = (libm::sin(about_z), libm::cos(about_z));
    let (sx, cx) = (libm::sin(about_x), libm::cos(about_x));
    let vertices = cuboid
        .vertices()
        .iter()
        .map(|p| {
            let (x, y) = (cz * p.x - sz * p.y, sz * p.x + cz * p.y);
            let (y, z) = (cx * y - sx * p.z, sx * y + cx * p.z);
            Point3::new(x, y, z)
        })
        .collect();
    Solid::from_faces(vertices, &cuboid.to_faces()).unwrap()
}

fn unit_cube() -> Solid {
    cuboid([0.0; 3], [1.0; 3])
}

/// Vertex, edge and face counts, then the volume of each shell in order, in mm³ so that
/// rounding in the last bits does not matter.
fn summary(solid: &Solid) -> ((usize, usize, usize), Vec<i64>) {
    #[expect(clippy::cast_possible_truncation, reason = "test volumes are small")]
    let rounded = |v: f64| (v * 1e9).round() as i64;
    (
        (
            solid.vertices().len(),
            solid.edge_count(),
            solid.face_count(),
        ),
        solid
            .shells()
            .iter()
            .map(|shell| rounded(shell.volume()))
            .collect(),
    )
}

const CUBIC_METRE: i64 = 1_000_000_000;

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

fn union(a: &Solid, b: &Solid) -> Solid {
    let result = a.union(b).unwrap().unwrap();
    result.validate().unwrap();
    result
}

#[test]
fn disjoint_boxes_stay_two_lumps() {
    let result = union(&unit_cube(), &cuboid([2.0; 3], [3.0; 3]));
    assert_eq!(
        summary(&result),
        ((16, 24, 12), vec![CUBIC_METRE, CUBIC_METRE])
    );
}

#[test]
fn overlapping_boxes_merge() {
    // Hand calculation (ENG-03): 8 + 8 − 1 for two 2 m cubes sharing a 1 m cube.
    let result = union(&cuboid([0.0; 3], [2.0; 3]), &cuboid([1.0; 3], [3.0; 3]));
    assert_close(result.volume(), 15.0);
    assert_eq!(result.shells().len(), 1);
    // Each cube loses one corner region: 7 + 7 vertices of their own plus 6 where they cross.
    assert_eq!(result.vertices().len(), 20);
    assert_eq!(result.face_count(), 12);
}

#[test]
fn a_box_inside_another_disappears() {
    let outer = cuboid([0.0; 3], [3.0; 3]);
    let result = union(&outer, &cuboid([1.0; 3], [2.0; 3]));
    assert_eq!(summary(&result), summary(&outer));
    let result = union(&cuboid([1.0; 3], [2.0; 3]), &outer);
    assert_eq!(summary(&result), summary(&outer));
}

#[test]
fn identical_boxes_give_the_box() {
    let result = union(&unit_cube(), &unit_cube());
    assert_eq!(summary(&result), ((8, 12, 6), vec![CUBIC_METRE]));
}

#[test]
fn boxes_sharing_a_face_become_one_box() {
    // ADR 0001: coplanar faces.
    let result = union(&unit_cube(), &cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]));
    assert_eq!(summary(&result), ((8, 12, 6), vec![2 * CUBIC_METRE]));
}

#[test]
fn a_gap_inside_the_tolerance_closes() {
    // ADR 0001: 0.5 µm apart, the cubes fuse as if they shared a face.
    let shift = 0.5 * LINEAR_TOLERANCE;
    let result = union(
        &unit_cube(),
        &cuboid([1.0 + shift, 0.0, 0.0], [2.0 + shift, 1.0, 1.0]),
    );
    assert_eq!(result.vertices().len(), 8);
    assert_eq!(result.face_count(), 6);
    // The far end of the second cube stays where it was.
    assert_close(result.volume(), 2.0 + shift);
}

#[test]
fn a_gap_outside_the_tolerance_stays() {
    // ADR 0001: 2 µm apart, the gap is real.
    let shift = 2.0 * LINEAR_TOLERANCE;
    let result = union(
        &unit_cube(),
        &cuboid([1.0 + shift, 0.0, 0.0], [2.0 + shift, 1.0, 1.0]),
    );
    assert_eq!(
        summary(&result),
        ((16, 24, 12), vec![CUBIC_METRE, CUBIC_METRE])
    );
}

#[test]
fn boxes_touching_along_an_edge_stay_two_lumps() {
    // ADR 0001: no non-manifold edge.
    let result = union(&unit_cube(), &cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0]));
    assert_eq!(
        summary(&result),
        ((16, 24, 12), vec![CUBIC_METRE, CUBIC_METRE])
    );
}

#[test]
fn boxes_touching_at_a_corner_stay_two_lumps() {
    // ADR 0001: no non-manifold vertex.
    let result = union(&unit_cube(), &cuboid([1.0; 3], [2.0; 3]));
    assert_eq!(
        summary(&result),
        ((16, 24, 12), vec![CUBIC_METRE, CUBIC_METRE])
    );
}

#[test]
fn a_step_merges_coplanar_faces() {
    // A 2 × 1 × 1 slab with a 1 × 1 × 1 cube on its left half: an L-shaped section.
    let result = union(
        &cuboid([0.0; 3], [2.0, 1.0, 1.0]),
        &cuboid([0.0, 0.0, 1.0], [1.0, 1.0, 2.0]),
    );
    assert_close(result.volume(), 3.0);
    // An L-shaped prism: two L faces, six rectangles.
    assert_eq!((result.vertices().len(), result.face_count()), (12, 8));
}

#[test]
fn a_micro_edge_merges() {
    // ADR 0001: the second box sits 0.3 µm into the first, which would leave 0.3 µm edges.
    // Its bottom merges with the first box's top; the solid's checks reject any edge shorter than
    // the tolerance.
    let off = 0.3 * LINEAR_TOLERANCE;
    let result = union(
        &unit_cube(),
        &cuboid([0.5, 0.0, 1.0 - off], [1.5, 1.0, 2.0]),
    );
    assert_eq!(result.shells().len(), 1);
    assert!((result.volume() - 2.0).abs() <= off);
}

#[test]
fn boxes_touching_over_part_of_a_face_fuse() {
    let result = union(
        &cuboid([1.5, 2.0, 0.5], [3.0, 3.0, 1.0]),
        &cuboid([1.5, 1.0, 0.0], [2.0, 2.5, 0.5]),
    );
    assert_eq!(result.shells().len(), 1);
    assert_close(result.volume(), 0.75 + 0.375);
}

#[test]
fn boxes_touching_along_part_of_an_edge_stay_two_lumps() {
    let result = union(
        &cuboid([0.0, 1.0, 1.5], [0.5, 1.5, 3.0]),
        &cuboid([0.5, 0.0, 1.0], [1.0, 1.0, 2.0]),
    );
    assert_eq!(result.shells().len(), 2);
    assert_close(result.volume(), 0.375 + 0.5);
}

#[test]
fn turned_boxes_touching_along_part_of_an_edge_stay_two_lumps() {
    let turn = (4.592_741_478_144_402, 5.246_832_646_232_279);
    let result = union(
        &turned([0.0, 1.0, 1.5], [0.5, 1.5, 3.0], turn),
        &turned([0.5, 0.0, 1.0], [1.0, 1.0, 2.0], turn),
    );
    assert_eq!(result.shells().len(), 2);
    assert_close(result.volume(), 0.375 + 0.5);
}

#[test]
fn overlapping_boxes_sharing_two_planes_merge() {
    // Hand calculation (ENG-03): 1.125 + 1.5 − 0.5.
    for turn in [(0.0, 0.0), (4.632_243_998_541_608, 1.536_123_907_819_626_7)] {
        let result = union(
            &turned([1.5, 0.0, 0.0], [3.0, 0.5, 1.5], turn),
            &turned([2.0, 0.0, 0.5], [3.0, 1.0, 2.0], turn),
        );
        assert_eq!(result.shells().len(), 1);
        assert_close(result.volume(), 2.125);
    }
}

#[test]
fn a_step_just_over_the_tolerance_is_kept() {
    // Side by side, one box 2 µm taller: the step is a real face, 2 µm high.
    let step = 2.0 * LINEAR_TOLERANCE;
    let result = union(
        &cuboid([0.0, 0.5, 0.0], [0.5, 1.5, 0.5]),
        &cuboid([0.0, 0.0, 0.0], [0.5, 0.5, 0.5 + step]),
    );
    assert_eq!(result.shells().len(), 1);
    // Two L-shaped sides and six rectangles, the step among them.
    assert_eq!(result.face_count(), 8);
    assert_close(result.volume(), 0.375 + 0.25 * step);
}

#[test]
fn a_face_half_a_tolerance_short_merges_in_either_order() {
    // The second box's end is 0.5 µm short of the first's: the two end faces merge into one,
    // whichever operand comes first.
    let a = cuboid([0.5, 1.5, 2.0], [1.0, 2.0, 3.0]);
    let b = cuboid([0.5, 0.5, 1.0], [1.0, 2.0 - 0.5 * LINEAR_TOLERANCE, 2.5]);
    for (first, second) in [(&a, &b), (&b, &a)] {
        let result = union(first, second);
        // An L-shaped prism.
        assert_eq!((result.vertices().len(), result.face_count()), (12, 8));
    }
}

#[test]
fn a_peg_through_a_hole_fills_it() {
    // A 4 × 4 × 1 m plate with a 1 m square hole, and a 1 × 1 × 3 m peg through the hole, its
    // sides on the hole's walls. Hand calculation (ENG-03): 15 + 3 m³, and no hole left.
    let square = |x0: f64, y0: f64, x1: f64, y1: f64| {
        let corners = [
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ];
        (0..4)
            .map(|i| Edge::Line {
                start: corners[i],
                end: corners[(i + 1) % 4],
            })
            .collect::<Vec<_>>()
    };
    let profile =
        Profile::new(square(0.0, 0.0, 4.0, 4.0), vec![square(1.0, 1.0, 2.0, 2.0)]).unwrap();
    let plate = Solid::extrude(&profile, Vector3::new(0.0, 0.0, 1.0)).unwrap();
    assert_eq!(plate.shells()[0].genus(), 1);
    let result = union(&plate, &cuboid([1.0, 1.0, -1.0], [2.0, 2.0, 2.0]));
    assert_eq!(result.shells().len(), 1);
    assert_eq!(result.shells()[0].genus(), 0);
    assert_close(result.volume(), 18.0);
}

#[test]
fn a_peg_flush_with_a_plate_fills_its_hole() {
    // A turned 1.5 × 1.5 × 0.5 m plate with a 0.5 m square hole, and a peg filling the hole
    // exactly, flush with both faces: one plain box again. The peg's top lies in the plane of
    // the plate's top, inside the hole, which the winding number must see as outside the plate.
    let (sz, cz) = (libm::sin(1.61), libm::cos(1.61));
    let turn = |x: f64, y: f64| Point2::new(cz * x - sz * y, sz * x + cz * y);
    let square = |x0: f64, y0: f64, x1: f64, y1: f64| {
        let corners = [turn(x0, y0), turn(x1, y0), turn(x1, y1), turn(x0, y1)];
        (0..4)
            .map(|i| Edge::Line {
                start: corners[i],
                end: corners[(i + 1) % 4],
            })
            .collect::<Vec<_>>()
    };
    let up = Vector3::new(0.0, 0.0, 0.5);
    let plate = Profile::new(square(2.0, 0.0, 3.5, 1.5), vec![square(2.5, 0.5, 3.0, 1.0)]).unwrap();
    let plate = Solid::extrude(&plate, up).unwrap();
    let peg = Profile::new(square(2.5, 0.5, 3.0, 1.0), vec![]).unwrap();
    let peg = Solid::extrude(&peg, up).unwrap();
    let whole = union(&plate, &peg);
    assert_eq!((whole.vertices().len(), whole.face_count()), (8, 6));
    assert_close(whole.volume(), 1.125);
}
