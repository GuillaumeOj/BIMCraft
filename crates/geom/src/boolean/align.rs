//! Putting faces that lie in one plane within the tolerance onto exactly one plane (ADR 0001,
//! merge pass step 1).
//!
//! Two faces of different operands whose planes are within the tolerance of each other over
//! the face are coplanar, and every later test treats them so. If each kept its own plane, the
//! faces around them would have to bend to join the two: a face a few tolerances wide, with one
//! side on each plane, is no longer flat. So the face whose plane comes later in a fixed order
//! moves onto the other's plane: its vertices are projected onto it. The order depends only on
//! the planes, so swapping the operands gives the same result. Each projection moves a vertex
//! by at most the tolerance, along the normal, and a corner of one operand lying within the
//! tolerance of the planes around a corner of the other lands on it, where the vertex merge
//! finds it.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::face::{PlanarFace, Pool};
use super::intersect::candidate_pairs;
use super::snap::{find, union};
use crate::vector::{Vector, dot, largest_axis, scale, sub};

/// Projects the vertices of each face of one operand (`faces[..split]`) or the other onto the
/// first, in plane order, of the faces of the other operand whose boxes overlap it and on whose
/// plane it lies, if that plane comes before its own; then works out the moved faces' planes
/// again.
pub(super) fn align(faces: &mut [PlanarFace], split: usize, pool: &mut Pool) {
    let mut target: Vec<Option<usize>> = vec![None; faces.len()];
    // Faces already on exactly one plane share its number.
    let mut same: Vec<usize> = (0..faces.len()).collect();
    for (f, g) in candidate_pairs(faces, split) {
        let (first, later) = match plane_order(&faces[f], &faces[g]) {
            Ordering::Less => (f, g),
            Ordering::Greater => (g, f),
            Ordering::Equal => {
                union(&mut same, f, g);
                continue;
            }
        };
        // Only the moving face's vertices are known to be within the tolerance of the other
        // plane; the other way round, a large face could move far more than the tolerance.
        if !faces[later].lies_within_tolerance_of(&faces[first], pool) {
            continue;
        }
        let best = target[later]
            .filter(|&other| plane_order(&faces[other], &faces[first]).is_lt())
            .unwrap_or(first);
        target[later] = Some(best);
    }
    // A target that itself moves is followed to where it goes, as far as the face still lies on
    // that plane, so faces judged coplanar end on one plane. Plane order falls at each step.
    let resolve = |face: usize, mut t: usize| {
        while let Some(next) =
            target[t].filter(|&next| faces[face].lies_within_tolerance_of(&faces[next], pool))
        {
            t = next;
        }
        t
    };
    // Each moved vertex goes onto every plane its faces move to, in plane order.
    let mut planes: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    // A moved face takes the number of the plane it goes onto, its target's before that moves
    // in turn: a face whose target moves on to a plane it does not lie on is not on that one.
    let mut onto: Vec<usize> = (0..faces.len()).collect();
    for (f, face) in faces.iter().enumerate() {
        if let Some(t) = target[f].map(|t| resolve(f, t)) {
            onto[f] = t;
            for &v in face.rings.iter().flatten() {
                planes.entry(v).or_default().push(t);
            }
        }
    }
    for (f, face) in faces.iter_mut().enumerate() {
        face.plane = find(&mut same, onto[f]);
    }
    for (&v, planes) in &mut planes {
        planes.sort_by(|&f, &g| plane_order(&faces[f], &faces[g]).then(f.cmp(&g)));
        planes.dedup();
        for &t in planes.iter() {
            let p = pool.get(v);
            pool.set(v, sub(p, scale(faces[t].normal, faces[t].distance(p))));
        }
    }
    for face in faces.iter_mut() {
        if face.rings.iter().flatten().any(|v| planes.contains_key(v)) {
            *face = PlanarFace::new(std::mem::take(&mut face.rings), pool, face.plane);
        }
    }
}

/// A fixed order of nearly parallel planes: by their offset along the normal turned towards
/// positive coordinates, then by that normal.
fn plane_order(a: &PlanarFace, b: &PlanarFace) -> Ordering {
    let (na, nb) = (towards_positive(a.normal), towards_positive(b.normal));
    dot(na, a.centre())
        .total_cmp(&dot(nb, b.centre()))
        .then_with(|| (0..3).fold(Ordering::Equal, |o, i| o.then(na[i].total_cmp(&nb[i]))))
}

/// The normal or its opposite, whichever has a positive largest component.
fn towards_positive(n: Vector) -> Vector {
    if n[largest_axis(n)] < 0.0 {
        scale(n, -1.0)
    } else {
        n
    }
}
