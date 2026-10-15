//! Feeds pairs of boxes to the booleans of the geometry kernel (GEO-03). Corners lie on a 0.25 m
//! grid, so faces, edges and corners of the two boxes coincide often, and both boxes are turned
//! together by any of 256 × 256 angles, so the coincidences are not axis-aligned. Nothing may
//! panic; an operation may fail with a typed error, but any solid it returns must be valid, a
//! union holds at least the larger box and at most both, and a difference at most the first box
//! and at least what the second cannot remove from it.

#![no_main]

use std::f64::consts::TAU;

use geom::{FaceInput, Point3, Solid};
use libfuzzer_sys::fuzz_target;

/// The box between two corners given in quarter metres, turned about z then x; `None` if flat,
/// or if turning left a coordinate the kernel rejects (one too close to zero for the exact
/// predicates).
fn cuboid(a: [i8; 3], b: [i8; 3], (about_z, about_x): (f64, f64)) -> Option<(Solid, f64)> {
    let min = [0, 1, 2].map(|i| f64::from(a[i].min(b[i])) / 4.0);
    let max = [0, 1, 2].map(|i| f64::from(a[i].max(b[i])) / 4.0);
    let volume: f64 = (0..3).map(|i| max[i] - min[i]).product();
    if volume == 0.0 {
        return None;
    }
    let (sz, cz) = (libm::sin(about_z), libm::cos(about_z));
    let (sx, cx) = (libm::sin(about_x), libm::cos(about_x));
    let vertices = (0..8)
        .map(|i| {
            let pick = |bit: usize, axis: usize| if i & bit == 0 { min[axis] } else { max[axis] };
            let (x, y, z) = (pick(1, 0), pick(2, 1), pick(4, 2));
            let (x, y) = (cz * x - sz * y, sz * x + cz * y);
            Point3::new(x, cx * y - sx * z, sx * y + cx * z)
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
    let solid = Solid::from_faces(vertices, &faces).ok()?;
    Some((solid, volume))
}

/// The corners of two boxes in quarter metres, and the two turning angles in steps of a 256th of
/// a turn.
type Input = ([i8; 6], [i8; 6], u8, u8);

fuzz_target!(|input: Input| {
    let (first, second, about_z, about_x) = input;
    let turn = (
        f64::from(about_z) * TAU / 256.0,
        f64::from(about_x) * TAU / 256.0,
    );
    let corners = |c: [i8; 6]| ([c[0], c[1], c[2]], [c[3], c[4], c[5]]);
    let ((a0, a1), (b0, b1)) = (corners(first), corners(second));
    let (Some((a, va)), Some((b, vb))) = (cuboid(a0, a1, turn), cuboid(b0, b1, turn)) else {
        return;
    };
    if let Ok(result) = a.union(&b) {
        let union = result.expect("a union is never empty");
        assert_eq!(union.validate(), Ok(()));
        let volume = union.volume();
        assert!(volume <= va + vb + 1e-6 && volume >= va.max(vb) - 1e-6);
    }
    if let Ok(result) = a.difference(&b) {
        let volume = result.map_or(0.0, |cut| {
            assert_eq!(cut.validate(), Ok(()));
            cut.volume()
        });
        assert!(volume <= va + 1e-6 && volume >= va - vb - 1e-6);
    }
});
