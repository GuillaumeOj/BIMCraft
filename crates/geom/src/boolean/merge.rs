//! The merge pass of ADR 0001 after the faces are paired: local operations on the faces of the
//! result, each applied only if the faces it leaves meet the solid's own checks.
//!
//! - Adjacent coplanar faces merge (step 3).
//! - Vertices left between two collinear edges go.
//! - A face narrower than the tolerance is zipped shut: its vertices lie within the tolerance of
//!   its longest edge, so they join the face across that edge instead (step 4).
//! - Lumps and voids no thicker than the tolerance on average vanish (steps 1 and 4).
//!
//! No operation moves a vertex, and none is applied if a face it touches would then break a
//! rule of the solid's checks: bent beyond the tolerance, no wider than it, or with an edge no
//! longer than it. So the pass never makes a valid result invalid. A face narrower than the
//! tolerance that no operation removes is reported as [`BooleanError::FeaturesTooClose`].

use std::collections::{BTreeMap, BTreeSet};

use super::BooleanError;
use super::assemble::{Faces, HalfEdges};
use super::face::{Pool, Projection, area_and_centre, clean_face, segment_distance_squared};
use super::planar::faces_of;
use super::snap::{find, union};
use crate::brep::{FaceFault, enclosed_volume, face_plane};
use crate::tolerance::{WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED};
use crate::vector::{Vector, distance_squared, dot, edges, lexicographic, sub};

/// The plane of a face as the solid's checks find it, or the rule of its own it breaks.
pub(super) fn checked(face: &[Vec<usize>], pool: &Pool) -> Result<(Vector, Vector), FaceFault> {
    let mut points = Vec::new();
    let mut ends = Vec::with_capacity(face.len());
    for ring in face {
        points.extend(ring.iter().map(|&v| pool.get(v)));
        ends.push(points.len());
    }
    face_plane(&points, &ends)
}

/// Whether a ring has an edge within the tolerance, which the solid's checks reject.
fn has_short_edge(ring: &[usize], pool: &Pool) -> bool {
    edges(ring).any(|(u, v)| distance_squared(pool.get(u), pool.get(v)) <= WITHIN_TOLERANCE_SQUARED)
}

/// Whether a face meets every rule of the solid's checks that depends on it alone: rings of
/// three vertices or more, no edge within the tolerance, and [`checked`].
fn holds(face: &[Vec<usize>], pool: &Pool) -> bool {
    face.iter()
        .all(|ring| ring.len() >= 3 && !has_short_edge(ring, pool))
        && checked(face, pool).is_ok()
}

/// Merges adjacent faces that lie in the same plane within the tolerance and face the same way.
pub(super) fn merge_coplanar(
    faces: &Faces,
    planes: &[(Vector, Vector)],
    twins: &[usize],
    pool: &Pool,
) -> Faces {
    let half_edges = HalfEdges::new(faces);
    // Whether two groups of faces, taken as one, lie within the tolerance of one face's plane.
    // The merged face must also pass the solid's checks, but that is not enough on its own: a
    // face a few tolerances wide, square to its neighbour, would pass, its width split on
    // either side of the plane fitted to both. Requiring every face's plane would reject a small
    // face tilted by a rounding error next to a large one. Groups are tested whole, not face by
    // face: a face a few tolerances wide may lie within the tolerance of two neighbours that are
    // not coplanar, and would otherwise join them into one bent face.
    let flat_together = |groups: [&[usize]; 2]| {
        groups.into_iter().flatten().any(|&f| {
            let (normal, centre) = planes[f];
            (groups.into_iter().flatten())
                .flat_map(|&g| faces[g].iter().flatten())
                .all(|&v| dot(normal, sub(pool.get(v), centre)).abs() <= WITHIN_TOLERANCE)
        })
    };
    let at = |f: usize| {
        let projection = Projection::along(planes[f].0);
        move |v: usize| projection.apply(pool.get(v))
    };
    let edge = |h: usize| (half_edges.origin[h], half_edges.target(h));
    let mut group: Vec<usize> = (0..faces.len()).collect();
    let mut members: Vec<Vec<usize>> = (0..faces.len()).map(|f| vec![f]).collect();
    // The half-edges between each group and the rest, and the faces a merged group makes.
    let mut boundary: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); faces.len()];
    for h in 0..twins.len() {
        boundary[half_edges.face(h)].insert(h);
    }
    let mut merged: Vec<Option<Faces>> = vec![None; faces.len()];
    // Groups refused once are refused again along their other shared edges, until one of them
    // grows: keyed by both roots and their sizes.
    let mut refused = BTreeSet::new();
    for (h, &twin) in twins.iter().enumerate() {
        let (f, g) = (half_edges.face(h), half_edges.face(twin));
        if f >= g {
            continue;
        }
        let (rf, rg) = (find(&mut group, f), find(&mut group, g));
        // Faces sharing several edges are tested once: later edges find them merged.
        let key = (rf, rg, members[rf].len(), members[rg].len());
        if rf == rg || dot(planes[f].0, planes[g].0) <= 0.0 || refused.contains(&key) {
            continue;
        }
        if !flat_together([&members[rf], &members[rg]]) {
            refused.insert(key);
            continue;
        }
        let around: BTreeSet<usize> = boundary[rf]
            .iter()
            .chain(&boundary[rg])
            .copied()
            .filter(|&h| ![rf, rg].contains(&find(&mut group, half_edges.face(twins[h]))))
            .collect();
        let outline: BTreeSet<(usize, usize)> = around.iter().map(|&h| edge(h)).collect();
        // Where the solid touches itself along an edge in the middle of the merged face, its
        // pieces from both sides leave a ring running along the edge and back, enclosing
        // nothing: it goes, and the edge is left to the faces that meet there.
        let mut made = faces_of(&outline, at(f));
        made.iter_mut().for_each(clean_face);
        made.retain(|face| !face.is_empty());
        if made.is_empty() || made.iter().any(|face| checked(face, pool).is_err()) {
            refused.insert(key);
            continue;
        }
        let (root, other) = (rf.min(rg), rf.max(rg));
        group[other] = root;
        let moved = std::mem::take(&mut members[other]);
        members[root].extend(moved);
        boundary[root] = around;
        // Only roots are read from here on; the absorbed group's state is let go.
        boundary[other] = BTreeSet::new();
        merged[root] = Some(made);
    }
    let mut result = Vec::new();
    for (f, face) in faces.iter().enumerate() {
        if find(&mut group, f) != f {
            continue;
        }
        match merged[f].take() {
            Some(made) => result.extend(made),
            None => result.push(face.clone()),
        }
    }
    result
}

/// Removes the vertices with only two neighbours that lie, within the tolerance, on the segment
/// between them. Such a vertex sits on the edge of two faces only, so dropping it moves the edge
/// by no more than the tolerance; one between two faces left unmerged at a fold stays, or the
/// faces would bend. The test depends only on the vertex and its neighbours, so both faces
/// along the edge agree. Vertices are taken in order of position, so the operands' order does
/// not matter, and one goes only if both its faces still pass the solid's checks without it.
pub(super) fn remove_collinear(mut faces: Faces, pool: &Pool) -> Faces {
    let mut neighbours: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    let mut owners: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for ring in face {
            for (u, v) in edges(ring) {
                neighbours.entry(u).or_default().insert(v);
                neighbours.entry(v).or_default().insert(u);
                owners.entry(u).or_default().insert(f);
            }
        }
    }
    let mut order: Vec<usize> = neighbours.keys().copied().collect();
    order.sort_by(|&v, &w| lexicographic(pool.get(v), pool.get(w)).then(v.cmp(&w)));
    for v in order {
        let mut around = neighbours[&v].iter().copied();
        let (Some(u), Some(w), None) = (around.next(), around.next(), around.next()) else {
            continue;
        };
        // Where u and w are already joined, the edge left would bound four faces.
        if neighbours[&u].contains(&w)
            || segment_distance_squared(pool.get(v), pool.get(u), pool.get(w))
                > WITHIN_TOLERANCE_SQUARED
        {
            continue;
        }
        let without: Vec<(usize, Vec<Vec<usize>>)> = owners[&v]
            .iter()
            .map(|&f| {
                let face = faces[f]
                    .iter()
                    .map(|ring| ring.iter().copied().filter(|&x| x != v).collect())
                    .collect();
                (f, face)
            })
            .collect();
        if !without.iter().all(|(_, face)| holds(face, pool)) {
            continue;
        }
        for (f, face) in without {
            faces[f] = face;
        }
        for (from, to) in [(u, w), (w, u)] {
            let set = neighbours.entry(from).or_default();
            set.remove(&v);
            set.insert(to);
        }
        neighbours.remove(&v);
    }
    faces
}

/// Zips shut the faces narrower than the tolerance, as the solid's checks find them, where they
/// can be, and tells whether it zipped any. A face whose other vertices lie within the
/// tolerance of its longest edge, in order along it, is removed, and the face across that edge
/// runs through those vertices instead. No vertex moves, and the surface moves by no more than
/// the tolerance. Every edge the faces had or have must still be met once each way, or the
/// sliver stays. Slivers are taken in order of position, so the operands' order does not
/// matter.
pub(super) fn remove_slivers(faces: &mut Faces, pool: &Pool) -> bool {
    let mut owners: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        own(&mut owners, face, f);
    }
    let sliver = |face: &[Vec<usize>]| checked(face, pool) == Err(FaceFault::Degenerate);
    let mut by_position: Vec<(Vec<Vector>, usize)> = (0..faces.len())
        .filter(|&f| faces[f].len() == 1 && sliver(&faces[f]))
        .map(|f| {
            let mut points: Vec<Vector> = faces[f][0].iter().map(|&v| pool.get(v)).collect();
            points.sort_by(|&p, &q| lexicographic(p, q));
            (points, f)
        })
        .collect();
    by_position.sort_by(|(one, f), (other, g)| {
        (one.iter().zip(other))
            .map(|(&p, &q)| lexicographic(p, q))
            .find(|order| order.is_ne())
            .unwrap_or_else(|| one.len().cmp(&other.len()))
            .then(f.cmp(g))
    });
    // A zip never makes a sliver (the face it changes must hold), so the list is made once; a
    // sliver that took another in is no longer one, and is skipped. Zipping one can let an
    // earlier one zip, so the list is walked again until none does.
    let mut slivers: Vec<usize> = by_position.into_iter().map(|(_, f)| f).collect();
    let mut zipped_any = false;
    while let Some((k, f, (g, face))) = slivers.iter().enumerate().find_map(|(k, &f)| {
        let zipped = sliver(&faces[f]).then(|| zip(f, faces, &owners, pool))??;
        Some((k, f, zipped))
    }) {
        slivers.remove(k);
        disown(&mut owners, &std::mem::take(&mut faces[f]), f);
        disown(&mut owners, &std::mem::replace(&mut faces[g], face), g);
        own(&mut owners, &faces[g], g);
        zipped_any = true;
    }
    zipped_any
}

/// Records face `f` as an owner of each of its directed edges.
fn own(owners: &mut BTreeMap<(usize, usize), Vec<usize>>, face: &[Vec<usize>], f: usize) {
    for edge in face.iter().flat_map(|ring| edges(ring)) {
        owners.entry(edge).or_default().push(f);
    }
}

/// Forgets face `f` as an owner of each of the directed edges it had.
fn disown(owners: &mut BTreeMap<(usize, usize), Vec<usize>>, face: &[Vec<usize>], f: usize) {
    for edge in face.iter().flat_map(|ring| edges(ring)) {
        if let Some(of) = owners.get_mut(&edge) {
            of.retain(|&g| g != f);
        }
    }
}

/// The face across the longest edge of `sliver` and what it becomes once the sliver is zipped
/// onto it; `None` if the sliver cannot be zipped.
fn zip(
    sliver: usize,
    faces: &Faces,
    owners: &BTreeMap<(usize, usize), Vec<usize>>,
    pool: &Pool,
) -> Option<(usize, Vec<Vec<usize>>)> {
    let ring = &faces[sliver][0];
    let count = ring.len();
    let length = |i: usize| distance_squared(pool.get(ring[i]), pool.get(ring[(i + 1) % count]));
    let longest = (0..count).fold(0, |best, i| if length(i) > length(best) { i } else { best });
    let (a, b) = (ring[longest], ring[(longest + 1) % count]);
    // From b round to a, the other way along the edge.
    let chain: Vec<usize> = (2..count).map(|k| ring[(longest + k) % count]).collect();
    let (start, along) = (pool.get(a), sub(pool.get(b), pool.get(a)));
    let fraction = |v: usize| dot(sub(pool.get(v), start), along) / dot(along, along);
    let in_order = std::iter::once(1.0)
        .chain(chain.iter().map(|&v| fraction(v)))
        .chain(std::iter::once(0.0))
        .collect::<Vec<f64>>()
        .windows(2)
        .all(|pair| pair[0] > pair[1]);
    let near = chain.iter().all(|&v| {
        segment_distance_squared(pool.get(v), start, pool.get(b)) <= WITHIN_TOLERANCE_SQUARED
    });
    if !in_order || !near {
        return None;
    }
    let across = (owners.get(&(b, a))?.first().copied()).filter(|&g| g != sliver)?;
    let mut face = faces[across].clone();
    let (r, at) = face.iter().enumerate().find_map(|(r, ring)| {
        let at = (1..=ring.len()).find(|&j| ring[j - 1] == b && ring[j % ring.len()] == a)?;
        Some((r, at))
    })?;
    face[r].splice(at..at, chain);
    // Where the face across also runs along the sliver, back to back with it, the two cancel.
    clean_face(&mut face);
    let mut seen = BTreeSet::new();
    let once = face.iter().flatten().all(|&v| seen.insert(v));
    let removed: BTreeSet<(usize, usize)> = (faces[sliver].iter().chain(&faces[across]))
        .flat_map(|ring| edges(ring))
        .collect();
    let added: BTreeSet<(usize, usize)> = face.iter().flat_map(|ring| edges(ring)).collect();
    // Every edge the faces had or have is then still met once each way, by the faces around.
    let sides = |edge: (usize, usize)| {
        let kept = owners.get(&edge).map_or(0, |of| {
            of.iter().filter(|&&g| g != sliver && g != across).count()
        });
        kept + usize::from(added.contains(&edge))
    };
    let paired = removed.union(&added).all(|&(u, v)| {
        let (forward, backward) = (sides((u, v)), sides((v, u)));
        forward <= 1 && forward == backward
    });
    (once && paired && holds(&face, pool)).then_some((across, face))
}

/// Whether merging vertices within the tolerance bent a face beyond it, as the solid's checks
/// find it. The pieces of a face lie in its plane, so only a merge can bend one, and the merge
/// pass cannot straighten it without moving other vertices.
pub(super) fn bent_by_merging(faces: &Faces, pool: &Pool) -> Result<(), BooleanError> {
    if faces
        .iter()
        .any(|face| checked(face, pool) == Err(FaceFault::NonPlanar))
    {
        return Err(BooleanError::FeaturesTooClose);
    }
    Ok(())
}

/// Whether a face no operation could remove is narrower than the tolerance, or has an edge no
/// longer than it: features of the operands within the tolerance of each other that the merge
/// pass cannot resolve without bending a face or breaking the solid apart.
pub(super) fn sliver_remains(faces: &Faces, pool: &Pool) -> Result<(), BooleanError> {
    if faces.iter().any(|face| {
        face.iter().any(|ring| has_short_edge(ring, pool))
            || checked(face, pool) == Err(FaceFault::Degenerate)
    }) {
        return Err(BooleanError::FeaturesTooClose);
    }
    Ok(())
}

/// Removes the lumps, and the voids, no thicker than the tolerance on average: the solid's checks
/// would reject them, and the result is regularised, so they vanish (ADR 0001, merge pass
/// steps 1 and 4). Lumps share no vertex by now, so faces sharing one belong to the same lump,
/// and removing one leaves the others as they were.
pub(super) fn drop_thin_lumps(faces: Faces, pool: &Pool) -> Faces {
    let mut lump: Vec<usize> = (0..faces.len()).collect();
    let mut owner: BTreeMap<usize, usize> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for &v in face.iter().flatten() {
            let g = *owner.entry(v).or_insert(f);
            union(&mut lump, f, g);
        }
    }
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for f in 0..faces.len() {
        members.entry(find(&mut lump, f)).or_default().push(f);
    }
    let thin: BTreeSet<usize> = members
        .values()
        .filter(|members| {
            let planes: Vec<(Vector, Vector)> = members
                .iter()
                .map(|&f| area_and_centre(&faces[f], pool))
                .collect();
            enclosed_volume(&planes).1
        })
        .flatten()
        .copied()
        .collect();
    faces
        .into_iter()
        .enumerate()
        .filter(|(f, _)| !thin.contains(f))
        .map(|(_, face)| face)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::assemble::assemble;
    use super::*;

    /// The corners of a tetrahedron with three faces on the coordinate planes, then the points
    /// of slivers along its edges: `extra` lie within a micrometre of an edge, beyond the
    /// tetrahedron's faces.
    fn tetrahedron(extra: &[Vector]) -> Pool {
        let mut points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        points.extend_from_slice(extra);
        Pool::of(points)
    }

    #[test]
    fn slivers_are_zipped_onto_the_faces_across_their_longest_edges() {
        // Vertex 4 sits 0.5 µm off the edge from 0 to 1, vertex 5 off the edge from 0 to 2;
        // each sliver bends back over both faces along its edge, so neither merges with them.
        let mut pool = tetrahedron(&[[0.5, -0.4e-6, 0.3e-6], [0.4e-6, 0.5, -0.3e-6]]);
        let faces = vec![
            vec![vec![0, 2, 1, 4]],
            vec![vec![0, 1, 3]],
            vec![vec![0, 3, 2, 5]],
            vec![vec![1, 2, 3]],
            vec![vec![0, 4, 1]],
            vec![vec![0, 5, 2]],
        ];
        // Zipped shut, the slivers leave their vertices between collinear edges, which go.
        let solid = assemble(faces, &mut pool).unwrap().unwrap();
        assert_eq!((solid.vertices().len(), solid.face_count()), (4, 4));
        assert!((solid.volume() - 1.0 / 6.0).abs() < 1e-12);
    }

    #[test]
    fn a_sliver_zigzagging_along_its_edge_is_features_too_close() {
        // A sliver along the edge from 0 to 1 whose other vertices run back the wrong way: 4
        // near 1, then 5 near 0. Nothing it touches could take them in order.
        let mut pool = tetrahedron(&[[0.9, -0.4e-6, 0.3e-6], [0.1, -0.4e-6, 0.3e-6]]);
        let faces = vec![
            vec![vec![0, 2, 1, 5, 4]],
            vec![vec![0, 1, 3]],
            vec![vec![0, 3, 2]],
            vec![vec![1, 2, 3]],
            vec![vec![0, 4, 5, 1]],
        ];
        assert_eq!(
            assemble(faces, &mut pool),
            Err(BooleanError::FeaturesTooClose)
        );
    }

    #[test]
    fn only_vertices_between_their_neighbours_go() {
        // Two faces back to back; vertex 3 is the middle of the edge from 1 to 2, while 0, 1
        // and 2 are corners with two neighbours each.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [1.0, 1.0, 0.0],
        ]);
        let faces = vec![vec![vec![0, 1, 3, 2]], vec![vec![2, 3, 1, 0]]];
        assert_eq!(
            remove_collinear(faces, &pool),
            [vec![vec![0, 1, 2]], vec![vec![2, 1, 0]]]
        );
    }

    #[test]
    fn a_vertex_stays_between_vertices_already_joined() {
        // Vertex 2 lies 0.5 µm off the edge from 0 to 1 that two other faces already share:
        // without it, that edge would bound four faces.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [1.0, 0.5e-6, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, -1.0],
        ]);
        let faces = vec![
            vec![vec![0, 2, 1, 3]],
            vec![vec![1, 2, 0, 4]],
            vec![vec![0, 1, 5]],
            vec![vec![1, 0, 6]],
        ];
        assert_eq!(remove_collinear(faces.clone(), &pool), faces);
    }

    #[test]
    fn a_vertex_stays_if_a_face_would_not_hold_without_it() {
        // Two triangles back to back, vertex 2 just off the segment from 0 to 1: without it,
        // neither would have three vertices.
        let pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.5e-6, 0.0]]);
        let faces = vec![vec![vec![0, 1, 2]], vec![vec![0, 2, 1]]];
        assert_eq!(remove_collinear(faces.clone(), &pool), faces);
    }
}
