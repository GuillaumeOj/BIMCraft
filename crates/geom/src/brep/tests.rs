use super::*;
use crate::MODEL_EXTENT;

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn face(outer: &[usize]) -> FaceInput {
    FaceInput {
        outer: outer.to_vec(),
        holes: Vec::new(),
    }
}

/// Corners of an axis-aligned box; corner `i` has bit 0 for x, bit 1 for y, bit 2 for z.
fn box_vertices(min: [f64; 3], max: [f64; 3]) -> Vec<Point3> {
    (0..8)
        .map(|i| {
            let pick = |bit: usize, axis: usize| if i & bit == 0 { min[axis] } else { max[axis] };
            p(pick(1, 0), pick(2, 1), pick(4, 2))
        })
        .collect()
}

/// Faces of a box whose corners start at `first`, each ring counter-clockwise from outside.
fn box_faces(first: usize) -> Vec<FaceInput> {
    [
        [0, 2, 3, 1],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 6, 7, 3],
        [0, 4, 6, 2],
        [1, 3, 7, 5],
    ]
    .iter()
    .map(|ring| face(&ring.map(|v| v + first)))
    .collect()
}

fn reversed(input: &FaceInput) -> FaceInput {
    let flip = |ring: &Vec<usize>| ring.iter().rev().copied().collect();
    FaceInput {
        outer: flip(&input.outer),
        holes: input.holes.iter().map(flip).collect(),
    }
}

fn unit_cube() -> Solid {
    Solid::from_faces(box_vertices([0.0; 3], [1.0; 3]), &box_faces(0)).unwrap()
}

/// A 3 × 3 × 1 m box with a 1 × 1 m square hole through it, top to bottom.
fn holed_box() -> (Vec<Point3>, Vec<FaceInput>) {
    let mut vertices = box_vertices([0.0; 3], [3.0, 3.0, 1.0]);
    vertices.extend(box_vertices([1.0, 1.0, 0.0], [2.0, 2.0, 1.0]));
    let mut faces = box_faces(0);
    faces[0].holes.push(vec![8, 9, 11, 10]);
    faces[1].holes.push(vec![12, 14, 15, 13]);
    // The walls of the hole face into it.
    for wall in [
        [8, 10, 14, 12],
        [9, 13, 15, 11],
        [8, 12, 13, 9],
        [10, 11, 15, 14],
    ] {
        faces.push(face(&wall));
    }
    (vertices, faces)
}

#[test]
fn cube_counts_and_volume() {
    let cube = unit_cube();
    assert_eq!(cube.vertices().len(), 8);
    assert_eq!(cube.edge_count(), 12);
    assert_eq!(cube.face_count(), 6);
    let [shell] = cube.shells() else {
        panic!("one shell expected");
    };
    assert_eq!(shell.faces(), [0, 1, 2, 3, 4, 5]);
    assert_eq!(shell.genus(), 0);
    assert!(!shell.is_void());
    assert!((cube.volume() - 1.0).abs() < 1e-12);
    assert_eq!(cube.validate(), Ok(()));
}

#[test]
fn faces_round_trip() {
    let cube = unit_cube();
    assert_eq!(cube.to_faces(), box_faces(0));
    let rebuilt = Solid::from_faces(cube.vertices().to_vec(), &cube.to_faces());
    assert_eq!(rebuilt, Ok(cube));
}

#[test]
fn tetrahedron_is_valid() {
    let vertices = vec![
        p(0.0, 0.0, 0.0),
        p(1.0, 0.0, 0.0),
        p(0.0, 1.0, 0.0),
        p(0.0, 0.0, 1.0),
    ];
    let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]].map(|ring| face(&ring));
    let solid = Solid::from_faces(vertices, &faces).unwrap();
    assert_eq!(solid.edge_count(), 6);
    assert!((solid.volume() - 1.0 / 6.0).abs() < 1e-12);
}

#[test]
fn box_with_a_through_hole_has_genus_one() {
    let (vertices, faces) = holed_box();
    let solid = Solid::from_faces(vertices, &faces).unwrap();
    assert_eq!(solid.edge_count(), 24);
    assert_eq!(solid.shells()[0].genus(), 1);
    assert!((solid.volume() - 8.0).abs() < 1e-12);
    assert_eq!(solid.to_faces(), faces);
}

#[test]
fn inner_void_is_a_negative_shell() {
    let mut vertices = box_vertices([0.0; 3], [3.0; 3]);
    vertices.extend(box_vertices([1.0; 3], [2.0; 3]));
    let mut faces = box_faces(0);
    faces.extend(box_faces(8).iter().map(reversed));
    let solid = Solid::from_faces(vertices, &faces).unwrap();
    let [outer, void] = solid.shells() else {
        panic!("two shells expected");
    };
    assert!(!outer.is_void());
    assert!(void.is_void());
    assert_eq!(void.faces(), [6, 7, 8, 9, 10, 11]);
    assert!((void.volume() + 1.0).abs() < 1e-12);
    assert!((solid.volume() - 26.0).abs() < 1e-12);
}

#[test]
fn separate_lumps_are_separate_shells() {
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices.extend(box_vertices([2.0; 3], [3.0; 3]));
    let mut faces = box_faces(0);
    faces.extend(box_faces(8));
    let solid = Solid::from_faces(vertices, &faces).unwrap();
    assert_eq!(solid.shells().len(), 2);
    assert!((solid.volume() - 2.0).abs() < 1e-12);
}

#[test]
fn collinear_vertex_on_an_edge_is_allowed() {
    // A vertex 1 mm from a corner, on the edge shared by the bottom and front faces.
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices.push(p(1e-3, 0.0, 0.0));
    let mut faces = box_faces(0);
    faces[0] = face(&[0, 2, 3, 1, 8]);
    faces[2] = face(&[0, 8, 1, 5, 4]);
    assert!(Solid::from_faces(vertices, &faces).is_ok());
}

#[test]
fn rejects_no_faces() {
    assert_eq!(Solid::from_faces(Vec::new(), &[]), Err(BrepError::Empty));
}

#[test]
fn rejects_unknown_vertex() {
    let mut faces = box_faces(0);
    faces[3].outer[1] = 99;
    let err = Solid::from_faces(box_vertices([0.0; 3], [1.0; 3]), &faces);
    assert_eq!(err, Err(BrepError::IndexOutOfRange { face: 3 }));
}

#[test]
fn rejects_degenerate_rings() {
    let vertices = || box_vertices([0.0; 3], [1.0; 3]);
    let mut faces = box_faces(0);
    faces[2].outer = vec![0, 1];
    let err = Solid::from_faces(vertices(), &faces);
    assert_eq!(err, Err(BrepError::DegenerateRing { face: 2 }));
    let mut faces = box_faces(0);
    faces[4].outer = vec![0, 4, 6, 4];
    let err = Solid::from_faces(vertices(), &faces);
    assert_eq!(err, Err(BrepError::DegenerateRing { face: 4 }));
}

#[test]
fn rejects_flipped_face() {
    let mut faces = box_faces(0);
    faces[0] = reversed(&faces[0]);
    let err = Solid::from_faces(box_vertices([0.0; 3], [1.0; 3]), &faces);
    assert!(matches!(err, Err(BrepError::NonManifoldEdge { .. })));
}

#[test]
fn rejects_missing_face() {
    let mut faces = box_faces(0);
    faces.pop();
    let err = Solid::from_faces(box_vertices([0.0; 3], [1.0; 3]), &faces);
    assert!(matches!(err, Err(BrepError::OpenEdge { .. })));
}

#[test]
fn rejects_inside_out_solid() {
    let faces: Vec<_> = box_faces(0).iter().map(reversed).collect();
    let err = Solid::from_faces(box_vertices([0.0; 3], [1.0; 3]), &faces);
    assert_eq!(err, Err(BrepError::WrongOrientation));
}

#[test]
fn rejects_vertex_off_its_face_plane() {
    // Lifting one corner of the top face by d twists it: every corner ends d / 4 off its plane.
    let lifted = |d: f64| {
        let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
        vertices[7].z += d;
        Solid::from_faces(vertices, &box_faces(0))
    };
    assert!(lifted(3e-6).is_ok());
    assert_eq!(lifted(5e-6), Err(BrepError::NonPlanarFace { face: 1 }));
}

#[test]
fn rejects_edge_shorter_than_tolerance() {
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices.push(p(0.3e-6, 0.0, 0.0));
    let mut faces = box_faces(0);
    faces[0] = face(&[0, 2, 3, 1, 8]);
    faces[2] = face(&[0, 8, 1, 5, 4]);
    let err = Solid::from_faces(vertices, &faces);
    assert_eq!(err, Err(BrepError::ShortEdge { face: 0 }));
}

#[test]
fn rejects_bad_coordinates() {
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices[5].y = f64::NAN;
    let err = Solid::from_faces(vertices, &box_faces(0));
    assert_eq!(err, Err(BrepError::NonFinite { vertex: 5 }));

    let near_edge = MODEL_EXTENT - 0.5;
    let vertices = box_vertices([near_edge, 0.0, 0.0], [near_edge + 1.0, 1.0, 1.0]);
    let err = Solid::from_faces(vertices, &box_faces(0));
    assert_eq!(err, Err(BrepError::OutsideModelExtent { vertex: 1 }));
}

#[test]
fn rejects_unused_vertex() {
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices.push(p(5.0, 5.0, 5.0));
    let err = Solid::from_faces(vertices, &box_faces(0));
    assert_eq!(err, Err(BrepError::IsolatedVertex { vertex: 8 }));
}

#[test]
fn rejects_solids_touching_at_a_corner() {
    let mut vertices = box_vertices([0.0; 3], [1.0; 3]);
    vertices.extend(box_vertices([1.0; 3], [2.0; 3]).into_iter().skip(1));
    // The second box's first corner is the first box's last one, vertex 7.
    let mut faces = box_faces(0);
    faces.extend(box_faces(7));
    let err = Solid::from_faces(vertices, &faces);
    assert_eq!(err, Err(BrepError::NonManifoldVertex { vertex: 7 }));
}

#[test]
fn rejects_sliver_face() {
    // A triangular prism whose triangles are only `h` high.
    let prism = |h: f64| {
        let vertices = vec![
            p(0.0, 0.0, 0.0),
            p(1.0, 0.0, 0.0),
            p(0.5, h, 0.0),
            p(0.0, 0.0, 1.0),
            p(1.0, 0.0, 1.0),
            p(0.5, h, 1.0),
        ];
        let faces = [
            &[0, 2, 1][..],
            &[3, 4, 5],
            &[0, 1, 4, 3],
            &[1, 2, 5, 4],
            &[2, 0, 3, 5],
        ]
        .map(face);
        Solid::from_faces(vertices, &faces)
    };
    assert!(prism(1e-3).is_ok());
    assert_eq!(prism(0.5e-6), Err(BrepError::DegenerateFace { face: 0 }));
}

#[test]
fn rejects_hole_turning_like_its_face() {
    // The whole hole turned inside out: rings and walls consistent, but the hole is reversed.
    let (vertices, mut faces) = holed_box();
    for f in &mut faces[..2] {
        f.holes = f
            .holes
            .iter()
            .map(|h| h.iter().rev().copied().collect())
            .collect();
    }
    for wall in &mut faces[6..] {
        *wall = reversed(wall);
    }
    let err = Solid::from_faces(vertices, &faces);
    assert_eq!(err, Err(BrepError::MisorientedHole { face: 0 }));
}

#[test]
fn rejects_shell_without_volume() {
    // Two triangles back to back: closed, but flat.
    let vertices = vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0)];
    let err = Solid::from_faces(vertices, &[face(&[0, 1, 2]), face(&[0, 2, 1])]);
    assert_eq!(err, Err(BrepError::ThinShell { shell: 0 }));
}

// Solids are only built valid, so the remaining checks guard future in-place edits; they are
// exercised by corrupting a valid cube.

#[test]
fn detects_inconsistent_twins() {
    let mut cube = unit_cube();
    cube.half_edges[0].twin = 0;
    assert_eq!(
        cube.validate(),
        Err(BrepError::InconsistentTwin { face: 0 })
    );
}

#[test]
fn detects_broken_rings() {
    let mut cube = unit_cube();
    // Skipping a half-edge leaves it on no ring.
    cube.half_edges[0].next = 2;
    assert_eq!(cube.validate(), Err(BrepError::BrokenRing { face: 0 }));

    let mut cube = unit_cube();
    // Jumping into another face's ring.
    cube.half_edges[0].next = 4;
    assert_eq!(cube.validate(), Err(BrepError::BrokenRing { face: 0 }));

    let mut cube = unit_cube();
    cube.half_edges[1].next = 0;
    assert_eq!(cube.validate(), Err(BrepError::DegenerateRing { face: 0 }));
}

#[test]
fn slab_thicker_than_tolerance_is_kept() {
    let slab = Solid::from_faces(box_vertices([0.0; 3], [1.0, 1.0, 1.5e-6]), &box_faces(0));
    assert!(slab.is_ok());
}

#[test]
fn rejects_void_larger_than_its_outside() {
    let mut vertices = box_vertices([1.0; 3], [2.0; 3]);
    vertices.extend(box_vertices([0.0; 3], [3.0; 3]));
    let mut faces = box_faces(0);
    faces.extend(box_faces(8).iter().map(reversed));
    let err = Solid::from_faces(vertices, &faces);
    assert_eq!(err, Err(BrepError::WrongOrientation));
}

#[test]
fn rejects_face_reduced_to_a_rim_by_its_hole() {
    // Every ring is wide, but the hole leaves a 0.1 µm rim around the top and bottom faces.
    let (mut vertices, faces) = holed_box();
    let rim = 0.1e-6;
    for v in &mut vertices[8..] {
        v.x = if v.x < 1.5 { rim } else { 3.0 - rim };
        v.y = if v.y < 1.5 { rim } else { 3.0 - rim };
    }
    let err = Solid::from_faces(vertices, &faces);
    assert_eq!(err, Err(BrepError::DegenerateFace { face: 0 }));
}
