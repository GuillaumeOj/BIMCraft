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
//! finds it. A vertex on a plane both operands share, through a face of its own that stays
//! put, moves along that plane instead, if that is no farther than the tolerance: leaving it
//! would cut its face there into slivers a fraction of a degree off the other operand's face.
//! The other faces around a moved vertex keep their planes: [`collar`] cuts them where they
//! would bend.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use super::collar;
use super::face::{PlanarFace, Pool};
use super::intersect::candidate_pairs;
use super::snap::{find, union};
use crate::tolerance::WITHIN_TOLERANCE;
use crate::vector::{Vector, across, add, dot, largest_axis, scale, unit};

/// Projects the vertices of each face of one operand (`faces[..split]`) or the other onto the
/// first, in plane order, of the faces of the other operand whose boxes overlap it and on whose
/// plane it lies, if that plane comes before its own, keeping a vertex on the planes both
/// operands share where it can ([`onto_plane`]); then cuts the faces bent by the move along
/// collars ([`collar::collar`]). Returns the faces and where the second operand's start.
pub(super) fn align(
    mut faces: Vec<PlanarFace>,
    split: usize,
    pool: &mut Pool,
) -> (Vec<PlanarFace>, usize) {
    let mut target: Vec<Option<usize>> = vec![None; faces.len()];
    // Faces already on exactly one plane share its number.
    let mut same: Vec<usize> = (0..faces.len()).collect();
    for (f, g) in candidate_pairs(&faces, split) {
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
    let held = held_planes(&faces, split, &onto);
    for (&v, planes) in &mut planes {
        planes.sort_by(|&f, &g| plane_order(&faces[f], &faces[g]).then(f.cmp(&g)));
        planes.dedup();
        let held = held.get(&v).map_or(&[][..], Vec::as_slice);
        for &t in planes.iter() {
            let p = pool.get(v);
            let basis = basis(&faces, held, faces[t].plane);
            pool.set(v, add(p, onto_plane(&faces[t], p, &basis)));
        }
    }
    let moved = planes.into_keys().collect();
    collar::collar(faces, split, &moved, pool)
}

/// For each vertex, the faces whose planes it stays on if it moves, one per plane, in plane order:
/// those of its faces that do not move and that a face of the other operand lies on too. Moving
/// off such a plane would leave a sliver of its face beside the other operand's, a shallow angle
/// off it.
fn held_planes(faces: &[PlanarFace], split: usize, onto: &[usize]) -> BTreeMap<usize, Vec<usize>> {
    let mut operands = vec![[false; 2]; faces.len()];
    for (f, face) in faces.iter().enumerate() {
        operands[face.plane][usize::from(f >= split)] = true;
    }
    let mut held: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        if onto[f] == f && operands[face.plane] == [true, true] {
            for &v in face.rings.iter().flatten() {
                held.entry(v).or_default().push(f);
            }
        }
    }
    // By plane order, not by number or index, so that swapping the operands gives the same move.
    for on in held.values_mut() {
        on.sort_by(|&f, &g| plane_order(&faces[f], &faces[g]));
        on.dedup_by_key(|f| faces[*f].plane);
    }
    held
}

/// An orthonormal basis of the normals of the `held` faces' planes, leaving out plane number
/// `onto`: a vertex goes onto that plane, and already lies on it if it holds it.
fn basis(faces: &[PlanarFace], held: &[usize], onto: usize) -> Vec<Vector> {
    let mut basis: Vec<Vector> = Vec::new();
    for &f in held.iter().filter(|&&f| faces[f].plane != onto) {
        if let Some(e) = unit(off_basis(faces[f].normal, &basis)) {
            basis.push(e);
        }
    }
    basis
}

/// `normal` less its components along the orthonormal `basis`.
fn off_basis(normal: Vector, basis: &[Vector]) -> Vector {
    basis.iter().fold(normal, |n, &e| across(n, e))
}

/// The shortest move putting `p` on the plane of `face` while keeping it on the planes whose
/// normals the orthonormal `basis` spans, if it is no longer than the tolerance; otherwise the
/// move along the face's normal.
fn onto_plane(face: &PlanarFace, p: Vector, basis: &[Vector]) -> Vector {
    let distance = face.distance(p);
    let along = off_basis(face.normal, basis);
    let squared = dot(along, along);
    if squared > 0.0 && distance.abs() <= WITHIN_TOLERANCE * squared.sqrt() {
        scale(along, -distance / squared)
    } else {
        scale(face.normal, -distance)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::{length, sub};

    /// A square face in the plane `slope · x + z = 0`.
    fn tilted(slope: f64) -> PlanarFace {
        let pool = Pool::of(vec![
            [0.0, -1.0, 0.0],
            [1.0, -1.0, -slope],
            [1.0, 1.0, -slope],
            [0.0, 1.0, 0.0],
        ]);
        PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 0)
    }

    #[test]
    fn a_moved_vertex_stays_on_the_planes_it_holds() {
        // At 45° to z = 0, a vertex 0.5 µm off along it goes to the planes' line.
        let p = [0.5e-6, 0.0, 0.0];
        let moved = add(p, onto_plane(&tilted(1.0), p, &[[0.0, 0.0, 1.0]]));
        assert!(moved[2] == 0.0 && moved[0].abs() < 1e-15, "{moved:?}");
        // Held by no plane, it moves along the face's normal, off z = 0.
        let moved = add(p, onto_plane(&tilted(1.0), p, &[]));
        assert!((moved[2] + 0.25e-6).abs() < 1e-15, "{moved:?}");
    }

    #[test]
    fn a_vertex_going_onto_a_plane_it_holds_is_not_held_off_it() {
        // Two held faces on planes 0 and 1: going onto plane 1, only plane 0 holds the vertex.
        let mut faces = vec![tilted(1.0), tilted(-1.0)];
        faces[1].plane = 1;
        let held = basis(&faces, &[0, 1], 1);
        assert_eq!(held.len(), 1);
        assert!(length(sub(held[0], faces[0].normal)) < 1e-15, "{held:?}");
        assert_eq!(basis(&faces, &[0, 1], 2).len(), 2);
    }

    #[test]
    fn a_vertex_held_too_far_from_where_it_goes_moves_along_the_normal() {
        // The face is 0.06° off z = 0: staying on z = 0 would take the vertex 0.5 mm along it.
        let face = tilted(1e-3);
        let p = [0.5e-3, 0.0, 0.0];
        let step = onto_plane(&face, p, &[[0.0, 0.0, 1.0]]);
        assert!(length(step) < WITHIN_TOLERANCE, "{step:?}");
        assert!(face.distance(add(p, step)).abs() < 1e-15);
    }
}
