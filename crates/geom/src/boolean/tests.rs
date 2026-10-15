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

fn difference(a: &Solid, b: &Solid) -> Option<Solid> {
    let result = a.difference(b).unwrap();
    if let Some(solid) = &result {
        solid.validate().unwrap();
    }
    result
}

#[test]
fn an_opening_through_a_wall() {
    // A 4 × 0.2 × 3 m wall with a 1 × 2 m door cut right through it, from the floor: the cut
    // is a notch, so the wall stays genus 0. Hand calculation (ENG-03): 2.4 − 0.4 m³.
    let wall = cuboid([0.0, 0.0, 0.0], [4.0, 0.2, 3.0]);
    let door = cuboid([1.0, -0.1, 0.0], [2.0, 0.3, 2.0]);
    let result = difference(&wall, &door).unwrap();
    assert_eq!(result.shells().len(), 1);
    assert_eq!(result.shells()[0].genus(), 0);
    assert_close(result.volume(), 2.0);
    // A window above the floor goes right through: genus 1, the hole's sides inside.
    let window = cuboid([2.5, -0.1, 1.0], [3.5, 0.3, 2.0]);
    let result = difference(&result, &window).unwrap();
    assert_eq!(result.shells()[0].genus(), 1);
    assert_close(result.volume(), 2.0 - 0.2);
}

#[test]
fn a_flush_cutter_leaves_no_skin() {
    // ADR 0001: the cutter's top is flush with the box's top. A 1 × 1 × 0.5 m pocket.
    let result = difference(
        &cuboid([0.0; 3], [3.0, 3.0, 1.0]),
        &cuboid([1.0, 1.0, 0.5], [2.0, 2.0, 1.0]),
    )
    .unwrap();
    assert_eq!(result.shells().len(), 1);
    assert_eq!(result.shells()[0].genus(), 0);
    assert_close(result.volume(), 9.0 - 0.5);
    // The box's top keeps a square hole: the pocket's mouth.
    assert!(result.to_faces().iter().any(|face| face.holes.len() == 1));
}

#[test]
fn a_skin_thinner_than_the_tolerance_is_removed() {
    // ADR 0001: the pocket would leave a 0.1 µm floor; the floor goes and the pocket opens.
    let skin = 0.1 * LINEAR_TOLERANCE;
    let result = difference(
        &cuboid([0.0; 3], [3.0, 3.0, 1.0]),
        &cuboid([1.0, 1.0, skin], [2.0, 2.0, 2.0]),
    )
    .unwrap();
    assert_eq!(result.shells()[0].genus(), 1);
    // Merging moves vertices by at most the skin's thickness.
    assert!((result.volume() - 8.0).abs() <= skin);
}

#[test]
fn a_cut_leaving_a_micro_edge_merges_it() {
    // ADR 0001: the cutter's corner is 0.3 µm from the box's edge.
    let off = 0.3 * LINEAR_TOLERANCE;
    let result = difference(
        &unit_cube(),
        &cuboid([0.5, -1.0, 0.5], [2.0, 1.0 - off, 2.0]),
    )
    .unwrap();
    assert_eq!(result.shells().len(), 1);
    assert!((result.volume() - 0.75).abs() <= off);
}

#[test]
fn a_cutter_inside_leaves_a_void() {
    let result = difference(&cuboid([0.0; 3], [3.0; 3]), &cuboid([1.0; 3], [2.0; 3])).unwrap();
    let shells = result.shells();
    assert_eq!(shells.len(), 2);
    assert_eq!(shells.iter().filter(|shell| shell.is_void()).count(), 1);
    assert_close(result.volume(), 26.0);
}

#[test]
fn a_cutter_elsewhere_changes_nothing() {
    let result = difference(&unit_cube(), &cuboid([2.0; 3], [3.0; 3])).unwrap();
    assert_eq!(summary(&result), summary(&unit_cube()));
    // Touching along a face, an edge or a corner removes nothing either.
    for cutter in [
        cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0]),
        cuboid([1.0; 3], [2.0; 3]),
    ] {
        let result = difference(&unit_cube(), &cutter).unwrap();
        assert_eq!(summary(&result), summary(&unit_cube()));
    }
}

#[test]
fn a_cutter_covering_everything_leaves_nothing() {
    assert!(difference(&unit_cube(), &cuboid([-1.0; 3], [2.0; 3])).is_none());
    assert!(difference(&unit_cube(), &unit_cube()).is_none());
}

#[test]
fn turned_cuts_keep_their_volume() {
    // A corner cut off a turned cube: 1 − 0.125 m³.
    let turn = (0.7, 1.9);
    let result = difference(
        &turned([0.0; 3], [1.0; 3], turn),
        &turned([0.5; 3], [1.5; 3], turn),
    )
    .unwrap();
    assert_close(result.volume(), 0.875);
}

#[test]
fn putting_back_a_flush_cut_restores_the_slab() {
    // A hole through a turned slab, flush with both faces, then the cutter put back: the slab
    // is whole again, one box with no hole. The cutter's top lies in the plane of the slab's
    // top, inside the hole, which the winding number must see as outside the slab.
    let turn = (1.610_129_385_758_991_1, 0.0);
    let slab = turned([2.0, 0.0, 2.5], [3.5, 1.5, 3.0], turn);
    let cutter = turned([2.5, 0.5, 2.5], [3.0, 1.0, 3.0], turn);
    let holed = difference(&slab, &cutter).unwrap();
    assert_eq!(holed.shells()[0].genus(), 1);
    let whole = union(&holed, &cutter);
    assert_eq!((whole.vertices().len(), whole.face_count()), (8, 6));
    assert_close(whole.volume(), 1.125);
}

fn intersection(a: &Solid, b: &Solid) -> Option<Solid> {
    let result = a.intersection(b).unwrap();
    if let Some(solid) = &result {
        solid.validate().unwrap();
    }
    result
}

#[test]
fn overlapping_boxes_share_their_overlap() {
    // Hand calculation (ENG-03): two 2 m cubes overlapping in a 1 m cube.
    let result = intersection(&cuboid([0.0; 3], [2.0; 3]), &cuboid([1.0; 3], [3.0; 3])).unwrap();
    assert_eq!(summary(&result), ((8, 12, 6), vec![CUBIC_METRE]));
}

#[test]
fn a_box_inside_another_is_the_intersection() {
    let inner = cuboid([1.0; 3], [2.0; 3]);
    let outer = cuboid([0.0; 3], [3.0; 3]);
    assert_eq!(
        summary(&intersection(&outer, &inner).unwrap()),
        summary(&inner)
    );
    assert_eq!(
        summary(&intersection(&inner, &outer).unwrap()),
        summary(&inner)
    );
    assert_eq!(
        summary(&intersection(&inner, &inner).unwrap()),
        summary(&inner)
    );
}

#[test]
fn boxes_sharing_a_face_plane_keep_it_once() {
    // Both boxes stand on z = 0: the overlap's bottom is that shared plane.
    let result = intersection(
        &cuboid([0.0; 3], [2.0, 2.0, 1.0]),
        &cuboid([1.0, 1.0, 0.0], [3.0, 3.0, 2.0]),
    )
    .unwrap();
    assert_eq!(summary(&result), ((8, 12, 6), vec![CUBIC_METRE]));
}

#[test]
fn touching_boxes_have_no_intersection() {
    // ADR 0001: touching faces, edges and corners give nothing.
    for other in [
        cuboid([1.0, 0.0, 0.0], [2.0, 1.0, 1.0]),
        cuboid([1.0, 1.0, 0.0], [2.0, 2.0, 1.0]),
        cuboid([1.0; 3], [2.0; 3]),
        cuboid([2.0; 3], [3.0; 3]),
    ] {
        assert!(intersection(&unit_cube(), &other).is_none());
    }
}

#[test]
fn an_overlap_thinner_than_the_tolerance_is_empty() {
    // ADR 0001: cubes overlapping by 0.5 µm.
    let overlap = 0.5 * LINEAR_TOLERANCE;
    let other = cuboid([1.0 - overlap, 0.0, 0.0], [2.0, 1.0, 1.0]);
    assert!(intersection(&unit_cube(), &other).is_none());
}

#[test]
fn turned_boxes_share_their_overlap() {
    let turn = (2.3, 0.4);
    let result = intersection(
        &turned([0.0; 3], [2.0; 3], turn),
        &turned([1.0; 3], [3.0; 3], turn),
    )
    .unwrap();
    assert_close(result.volume(), 1.0);
}

// Features between one and a few tolerances wide (ADR 0001, "Exactly one tolerance"). Each case
// below failed before every test read "within the tolerance" the same way.

/// Every operation of `a` and `b`, checked valid: the union, the difference and the
/// intersection.
fn all_three(a: &Solid, b: &Solid) -> [Option<Solid>; 3] {
    [Some(union(a, b)), difference(a, b), intersection(a, b)]
}

fn volume_of(solid: Option<&Solid>) -> f64 {
    solid.map_or(0.0, Solid::volume)
}

#[test]
fn a_gap_of_exactly_one_tolerance_closes() {
    // At several positions, so the gap rounds to just under the tolerance at some and just
    // over at others: it closes everywhere.
    for x in [0.5, 1.0, 1.5, 2.5, 3.0, 7.5] {
        let first = cuboid([x - 1.0, 0.0, 0.0], [x, 1.0, 1.0]);
        let second = cuboid([x + LINEAR_TOLERANCE, 0.0, 0.0], [x + 1.0, 1.0, 1.0]);
        let result = union(&first, &second);
        assert_eq!((result.vertices().len(), result.face_count()), (8, 6));
    }
}

#[test]
fn an_overlap_of_exactly_one_tolerance_is_empty() {
    for z in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
        let below = cuboid([0.0, 0.0, z - 1.0], [1.0, 1.0, z]);
        let above = cuboid([0.0, 0.5, z - LINEAR_TOLERANCE], [1.0, 2.0, z + 1.0]);
        assert!(intersection(&below, &above).is_none());
        // What the cut leaves is the box, give or take the layer.
        let cut = difference(&below, &above).unwrap();
        assert!((cut.volume() - 1.0).abs() <= 2.0 * LINEAR_TOLERANCE);
    }
}

#[test]
fn a_ledge_beside_a_closed_gap_stays_flat() {
    // The lower box stops 1 µm under the upper one, which closes, and 1.5 µm in from its
    // front, which leaves a ledge 1.5 µm wide on the upper box's underside. Before, the ledge
    // kept the upper box's plane on one side and the lower box's on the other: not flat.
    let upper = cuboid([0.5, 0.0, 2.0], [2.0, 0.5, 3.0]);
    let lower = cuboid(
        [1.0, 1.5 * LINEAR_TOLERANCE, 1.5],
        [2.5, 1.5, 2.0 - LINEAR_TOLERANCE],
    );
    let result = union(&upper, &lower);
    assert_eq!(result.shells().len(), 1);
    assert!((result.volume() - (0.75 + 1.125)).abs() <= 1e-5);
}

#[test]
fn corners_within_the_tolerance_of_each_others_planes_meet() {
    // The second box's corner lies 1 µm past the first's side and 0.5 µm short of its back:
    // within the tolerance of both planes, √1.25 µm from the corner. The boxes touch along
    // an edge, so they stay two lumps; before, the corner was cut onto both edges and left a
    // face open.
    let first = cuboid([1.5, 1.5, 2.0], [3.0, 2.5, 2.5]);
    let second = cuboid(
        [0.5, 2.5 - 0.5 * LINEAR_TOLERANCE, 2.0],
        [1.5 + LINEAR_TOLERANCE, 3.5, 3.0],
    );
    let [joined, cut, shared] = all_three(&first, &second);
    let joined = joined.unwrap();
    assert_eq!(joined.shells().len(), 2);
    assert!((joined.volume() - 1.75).abs() <= 1e-5);
    assert!((volume_of(cut.as_ref()) - 0.75).abs() <= 1e-5);
    assert!(shared.is_none());
}

#[test]
fn a_notch_two_tolerances_wide_is_kept() {
    // The boxes overlap along a 1 m edge, 2 µm deep each way. The notch's end faces are 2 µm
    // squares, whose middle is 1 µm from the other box's faces: inside it, though within the
    // tolerance. The overlap itself, a stick 2 µm thick on average 1 µm, vanishes.
    let first = cuboid([1.0, 0.0, 2.0], [2.5, 0.5, 2.5]);
    let second = cuboid(
        [1.5, 0.5 - 2.0 * LINEAR_TOLERANCE, 1.0],
        [2.5, 2.0, 2.0 + 2.0 * LINEAR_TOLERANCE],
    );
    let [joined, cut, shared] = all_three(&first, &second);
    assert_eq!(joined.unwrap().shells().len(), 1);
    assert_eq!(cut.unwrap().shells().len(), 1);
    assert!(shared.is_none());
}

#[test]
fn a_step_a_few_tolerances_high_is_not_merged_into_its_neighbours() {
    // The second box reaches 1.5 µm below the first: the step between their bottoms is a face
    // 1.5 µm high, square to them. A plane fitted to it and its neighbour passes within the
    // tolerance of both, but it must not merge with it.
    let first = cuboid([0.0, 0.5, 1.0], [1.5, 1.5, 1.5]);
    let second = cuboid(
        [-0.5 * LINEAR_TOLERANCE, 0.5, 1.0 - 1.5 * LINEAR_TOLERANCE],
        [0.5, 1.5, 2.5],
    );
    let result = union(&first, &second);
    assert_eq!(result.shells().len(), 1);
    assert!((result.volume() - 1.25).abs() <= 1e-5);
}

#[test]
fn turned_boxes_sharing_planes_through_the_origin() {
    // Faces of both boxes lie in the same planes, one through the origin: moving a vertex onto
    // the shared plane must keep its zero coordinates usable by the exact predicates.
    let turn = (0.133_473_269_484_559_54, 3.034_849_336_434_191);
    let result = union(
        &turned([0.0, 1.0, 1.5], [0.5, 1.5, 2.0], turn),
        &turned([0.0, 0.0, 2.0], [1.0, 1.5, 3.5], turn),
    );
    assert_eq!(result.shells().len(), 1);
    assert_close(result.volume(), 2.25 + 0.125);
}

/// A prism over the polygon with corners at `corners` sixteenths of a turn on a circle, maybe
/// with a square hole a quarter of the radius from the centre each way, extruded along
/// `direction`.
fn prism(
    corners: &[u32],
    (centre, radius): (Point2, f64),
    with_hole: bool,
    along: Vector3,
) -> Solid {
    let polygon = |points: &[Point2]| {
        (0..points.len())
            .map(|i| Edge::Line {
                start: points[i],
                end: points[(i + 1) % points.len()],
            })
            .collect::<Vec<_>>()
    };
    let at = |x: f64, y: f64| Point2::new(centre.x + x, centre.y + y);
    let outer: Vec<Point2> = corners
        .iter()
        .map(|&k| {
            let angle = std::f64::consts::TAU * f64::from(k) / 16.0;
            at(radius * libm::cos(angle), radius * libm::sin(angle))
        })
        .collect();
    let side = radius / 4.0;
    let hole = [
        at(-side, -side),
        at(side, -side),
        at(side, side),
        at(-side, side),
    ];
    let holes = if with_hole {
        vec![polygon(&hole)]
    } else {
        Vec::new()
    };
    Solid::extrude(&Profile::new(polygon(&outer), holes).unwrap(), along).unwrap()
}

#[test]
fn a_prism_poking_out_beside_a_shared_corner() {
    // Both prisms stand on z = 0 and share a bottom corner. The second, slanted the other way,
    // pokes out through a side face of the first in a triangle whose tip is that corner: what is
    // left of the side face is a ring passing through the corner twice, which no face may be.
    // Hand calculation (ENG-03): a polygon inscribed in a circle of radius r has the area
    // r²/2 · Σ sin θ over the angles θ between its corners; the shear by the extrusion keeps it at
    // every height. First: 9/8 · (sin 90° + 2 sin 67.5° + sin 135°) · 1 = 3.999 224 m³. Second:
    // 9/8 · (sin 112.5° + sin 22.5° − sin 135°) · 0.5 = 0.337 194 m³. Undoing the common shear
    // along x turns the overlap at height z into the triangle against the quadrilateral moved
    // z/2 along y; integrating its clipped area over z gives 0.249 807 m³.
    let circle = (Point2::new(0.0, 1.0), 1.5);
    let first = prism(&[3, 7, 10, 13], circle, false, Vector3::new(-1.0, 0.5, 1.0));
    let second = prism(&[7, 12, 13], circle, false, Vector3::new(-0.5, 0.0, 0.5));
    assert_close(first.volume(), 3.999_224_077);
    assert_close(second.volume(), 0.337_194_103);
    let shared = 0.249_807_489;
    for (one, other) in [(&first, &second), (&second, &first)] {
        let joined = union(one, other);
        assert_eq!(joined.shells().len(), 1);
        assert!((joined.volume() - (3.999_224_077 + 0.337_194_103 - shared)).abs() < 1e-8);
        let overlap = intersection(one, other).unwrap();
        assert!((overlap.volume() - shared).abs() < 1e-8);
    }
    let cut = difference(&first, &second).unwrap();
    assert!((cut.volume() - (3.999_224_077 - shared)).abs() < 1e-8);
    let cut = difference(&second, &first).unwrap();
    assert!((cut.volume() - (0.337_194_103 - shared)).abs() < 1e-8);
}

#[test]
fn a_hole_wall_rising_from_a_side_along_the_floor() {
    use std::f64::consts::SQRT_2;

    // Both prisms stand on z = 0. The bottom edge of the second's hole lies along the first's
    // side, and the hole's wall leans away from it: the union touches itself along that edge,
    // which runs through the middle of the merged floor and ends at a corner of the first prism,
    // where a wedge-shaped gap between the wall and the side touches the floor. Hand calculation
    // (ENG-03), areas as in `a_prism_poking_out_beside_a_shared_corner`: first,
    // 2 · (sin 135° + sin 45° + sin 180°) · 0.5 = √2 m³; second,
    // (2 · (sin 90° + 2 sin 135°) − 1) · 0.5 = 0.5 + √2 m³. Undoing the second's shear, the
    // overlap at height z is the first triangle against the second's triangle less its hole,
    // both moved by −z along x and y; integrating its clipped area over z gives 0.095 866 m³.
    let first = prism(
        &[0, 6, 8],
        (Point2::new(0.0, -1.5), 2.0),
        false,
        Vector3::new(0.0, 0.0, 0.5),
    );
    let second = prism(
        &[0, 4, 10],
        (Point2::new(1.5, -1.0), 2.0),
        true,
        Vector3::new(-0.5, -0.5, 0.5),
    );
    let (first_volume, second_volume) = (SQRT_2, 0.5 + SQRT_2);
    assert_close(first.volume(), first_volume);
    assert_close(second.volume(), second_volume);
    let shared = 0.095_865_648;
    for (one, other) in [(&first, &second), (&second, &first)] {
        let joined = union(one, other);
        assert_eq!(joined.shells().len(), 1);
        assert!((joined.volume() - (first_volume + second_volume - shared)).abs() < 1e-8);
        let overlap = intersection(one, other).unwrap();
        assert!((overlap.volume() - shared).abs() < 1e-8);
    }
    let cut = difference(&first, &second).unwrap();
    assert!((cut.volume() - (first_volume - shared)).abs() < 1e-8);
}
