//! Property-based tests of the B-rep on random valid solids (GEO-03, ENG-02): boxes, prisms on
//! convex polygons, and boxes with a hole through them, anywhere in the model extent.

use std::f64::consts::TAU;

use geom::{BrepError, FaceInput, Point3, Solid};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

/// A valid solid with what its validation should report.
#[derive(Clone, Debug)]
struct Case {
    vertices: Vec<Point3>,
    faces: Vec<FaceInput>,
    genus: u32,
    volume: f64,
}

fn face(outer: impl IntoIterator<Item = usize>) -> FaceInput {
    FaceInput {
        outer: outer.into_iter().collect(),
        holes: Vec::new(),
    }
}

fn reversed(input: &FaceInput) -> FaceInput {
    let flip = |ring: &Vec<usize>| ring.iter().rev().copied().collect();
    FaceInput {
        outer: flip(&input.outer),
        holes: input.holes.iter().map(flip).collect(),
    }
}

/// Corners of an axis-aligned box; corner `i` has bit 0 for x, bit 1 for y, bit 2 for z.
fn box_vertices(min: [f64; 3], max: [f64; 3]) -> Vec<Point3> {
    (0..8)
        .map(|i| {
            let pick = |bit: usize, axis: usize| if i & bit == 0 { min[axis] } else { max[axis] };
            Point3::new(pick(1, 0), pick(2, 1), pick(4, 2))
        })
        .collect()
}

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
    .map(|ring| face(ring.map(|v| v + first)))
    .collect()
}

fn coordinate() -> impl Strategy<Value = f64> {
    -1_000.0..1_000.0_f64
}

fn length() -> impl Strategy<Value = f64> {
    1e-3..50.0_f64
}

fn boxes() -> impl Strategy<Value = Case> {
    (
        prop::array::uniform3(coordinate()),
        prop::array::uniform3(length()),
    )
        .prop_map(|(min, size)| {
            let max = [min[0] + size[0], min[1] + size[1], min[2] + size[2]];
            Case {
                vertices: box_vertices(min, max),
                faces: box_faces(0),
                genus: 0,
                // Measured on the rounded corners, as the solid sees them.
                volume: (max[0] - min[0]) * (max[1] - min[1]) * (max[2] - min[2]),
            }
        })
}

/// A prism on a convex polygon inscribed in a circle, its corners at least 1/64 turn apart.
fn prisms() -> impl Strategy<Value = Case> {
    (
        prop::sample::subsequence((0..64).collect::<Vec<u32>>(), 3..=12),
        prop::array::uniform3(coordinate()),
        length(),
        length(),
    )
        .prop_map(|(steps, centre, radius, height)| {
            let ring: Vec<[f64; 2]> = steps
                .iter()
                .map(|&k| {
                    let angle = TAU * f64::from(k) / 64.0;
                    [
                        centre[0] + radius * libm::cos(angle),
                        centre[1] + radius * libm::sin(angle),
                    ]
                })
                .collect();
            let n = ring.len();
            let (bottom, top) = (centre[2], centre[2] + height);
            let mut vertices: Vec<Point3> = ring
                .iter()
                .map(|&[x, y]| Point3::new(x, y, bottom))
                .collect();
            vertices.extend(ring.iter().map(|&[x, y]| Point3::new(x, y, top)));
            let mut faces = vec![face((0..n).rev()), face(n..2 * n)];
            faces.extend((0..n).map(|i| {
                let j = (i + 1) % n;
                face([i, j, n + j, n + i])
            }));
            let twice_area: f64 = (0..n)
                .map(|i| {
                    let ([x0, y0], [x1, y1]) = (ring[i], ring[(i + 1) % n]);
                    (x0 - centre[0]) * (y1 - centre[1]) - (x1 - centre[0]) * (y0 - centre[1])
                })
                .sum();
            Case {
                vertices,
                faces,
                genus: 0,
                volume: twice_area / 2.0 * (top - bottom),
            }
        })
}

/// A box with a rectangular hole through it, top to bottom, at least 1 mm from every side.
fn holed_boxes() -> impl Strategy<Value = Case> {
    (
        prop::array::uniform3(coordinate()),
        prop::array::uniform2(0.01..0.98_f64),
        prop::array::uniform2(0.01..0.98_f64),
        prop::array::uniform3(0.1..50.0_f64),
    )
        .prop_map(|(min, [x0, x1], [y0, y1], size)| {
            let (x0, x1) = (x0.min(x1), x0.max(x1) + 0.01);
            let (y0, y1) = (y0.min(y1), y0.max(y1) + 0.01);
            let at = |axis: usize, t: f64| min[axis] + t * size[axis];
            let max = [at(0, 1.0), at(1, 1.0), at(2, 1.0)];
            let mut vertices = box_vertices(min, max);
            let hole_min = [at(0, x0), at(1, y0), min[2]];
            let hole_max = [at(0, x1), at(1, y1), max[2]];
            vertices.extend(box_vertices(hole_min, hole_max));
            let mut faces = box_faces(0);
            faces[0].holes.push(vec![8, 9, 11, 10]);
            faces[1].holes.push(vec![12, 14, 15, 13]);
            for wall in [
                [8, 10, 14, 12],
                [9, 13, 15, 11],
                [8, 12, 13, 9],
                [10, 11, 15, 14],
            ] {
                faces.push(face(wall));
            }
            let area = |lo: [f64; 3], hi: [f64; 3]| (hi[0] - lo[0]) * (hi[1] - lo[1]);
            Case {
                vertices,
                faces,
                genus: 1,
                volume: (area(min, max) - area(hole_min, hole_max)) * (max[2] - min[2]),
            }
        })
}

fn solids() -> impl Strategy<Value = Case> {
    prop_oneof![boxes(), prisms(), holed_boxes()]
}

fn config() -> ProptestConfig {
    ProptestConfig {
        cases: 1_000,
        rng_seed: RngSeed::Fixed(50),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn random_solids_are_valid(case in solids()) {
        let solid = Solid::from_faces(case.vertices.clone(), &case.faces);
        let solid = solid.map_err(|e| TestCaseError::fail(e.to_string()))?;
        let [shell] = solid.shells() else {
            return Err(TestCaseError::fail("one shell expected"));
        };
        prop_assert_eq!(shell.genus(), case.genus);
        prop_assert!((solid.volume() - case.volume).abs() <= 1e-9 * case.volume);
        prop_assert_eq!(solid.validate(), Ok(()));
        let rebuilt = Solid::from_faces(solid.vertices().to_vec(), &solid.to_faces());
        prop_assert_eq!(rebuilt, Ok(solid));
    }

    #[test]
    fn turning_a_solid_inside_out_is_rejected(case in solids()) {
        let faces: Vec<_> = case.faces.iter().map(reversed).collect();
        prop_assert_eq!(Solid::from_faces(case.vertices, &faces), Err(BrepError::WrongOrientation));
    }

    #[test]
    fn removing_a_face_opens_the_solid(case in solids(), pick in any::<prop::sample::Index>()) {
        let mut faces = case.faces;
        faces.remove(pick.index(faces.len()));
        let result = Solid::from_faces(case.vertices, &faces);
        prop_assert!(matches!(result, Err(BrepError::OpenEdge { .. })), "{result:?}");
    }
}
