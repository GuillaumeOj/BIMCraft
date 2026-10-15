//! Turning the kept pieces of faces into a valid solid: the rest of the merge pass of ADR 0001.
//!
//! - Vertices lying on an edge of another face split that edge, so neighbouring faces share
//!   their edges exactly.
//! - Where more than two faces meet at an edge (lumps touching along it), each side is paired
//!   with the face across the same lump; lumps touching at an edge or a point then get their own
//!   copies of the vertices, so no non-manifold edge or vertex is created.
//! - Adjacent coplanar faces merge, and vertices left between two collinear edges go.

use std::collections::{BTreeMap, BTreeSet};

use super::BooleanError;
use super::face::{
    Pool, Projection, area_and_centre, clean_face, plane, segment_distance_squared, sorted_by_x,
    vertices_on_edge,
};
use super::planar::faces_of;
use super::snap::{find, snap, union};
use crate::brep::enclosed_volume;
use crate::tolerance::{WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED};
use crate::vector::{Vector, cross, dot, edges, length, scale, sub};
use crate::{FaceInput, Point3, Solid};

/// Faces as rings of pool vertices, the outer ring first.
pub(super) type Faces = Vec<Vec<Vec<usize>>>;

/// Builds the solid bounded by the faces; `None` if no face is left.
pub(super) fn assemble(mut faces: Faces, pool: &mut Pool) -> Result<Option<Solid>, BooleanError> {
    // Cutting faces adds vertices, which may land on each other from neighbouring faces. Only
    // the vertices kept take part: those of dropped pieces must not make a chain fail.
    let mut used: Vec<usize> = faces.iter().flatten().flatten().copied().collect();
    used.sort_unstable();
    used.dedup();
    let rename = snap(pool, &used)?;
    for vertex in faces.iter_mut().flatten().flatten() {
        *vertex = rename[*vertex];
    }
    split_at_vertices(&mut faces, pool);
    clean(&mut faces);
    if faces.is_empty() {
        return Ok(None);
    }
    // Renaming vertices below moves none of them, so the planes hold throughout.
    let planes: Vec<(Vector, Vector)> = faces.iter().map(|face| plane(face, pool)).collect();
    let Some(twins) = pair(&faces, &planes, pool) else {
        return build(&faces, pool);
    };
    separate_lumps(&mut faces, &twins, pool);
    let faces = merge_coplanar(&faces, &planes, &twins, pool);
    let faces = drop_thin_lumps(remove_collinear(faces, pool), pool);
    if faces.is_empty() {
        return Ok(None);
    }
    build(&faces, pool)
}

/// Splits every edge at the vertices of other faces lying on it, within the tolerance.
fn split_at_vertices(faces: &mut Faces, pool: &Pool) {
    let by_x = sorted_by_x(pool, faces.iter().flatten().flatten().copied());
    for ring in faces.iter_mut().flatten() {
        let mut split = Vec::with_capacity(ring.len());
        for edge in edges(ring) {
            split.push(edge.0);
            split.extend(vertices_on_edge(pool, &by_x, edge));
        }
        *ring = split;
    }
}

/// Drops repeated vertices and rings too short to bound anything, then the faces left empty.
fn clean(faces: &mut Faces) {
    faces.iter_mut().for_each(clean_face);
    faces.retain(|face| !face.is_empty());
}

/// The half-edges of the faces, in a fixed order: face by face, ring by ring.
struct HalfEdges {
    origin: Vec<usize>,
    next: Vec<usize>,
    /// Where each half-edge's origin sits: face, ring and position.
    slot: Vec<(usize, usize, usize)>,
}

impl HalfEdges {
    fn new(faces: &Faces) -> Self {
        let mut half_edges = Self {
            origin: Vec::new(),
            next: Vec::new(),
            slot: Vec::new(),
        };
        for (f, face) in faces.iter().enumerate() {
            for (r, ring) in face.iter().enumerate() {
                let first = half_edges.origin.len();
                for (i, &v) in ring.iter().enumerate() {
                    half_edges.origin.push(v);
                    half_edges.next.push(first + (i + 1) % ring.len());
                    half_edges.slot.push((f, r, i));
                }
            }
        }
        half_edges
    }

    fn face(&self, h: usize) -> usize {
        self.slot[h].0
    }

    fn target(&self, h: usize) -> usize {
        self.origin[self.next[h]]
    }
}

/// Pairs each half-edge with the opposite one across the same lump. `None` if an edge cannot
/// be paired, which the solid's checks then report.
fn pair(faces: &Faces, planes: &[(Vector, Vector)], pool: &Pool) -> Option<Vec<usize>> {
    let half_edges = HalfEdges::new(faces);
    let mut by_edge: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for h in 0..half_edges.origin.len() {
        let (u, v) = (half_edges.origin[h], half_edges.target(h));
        by_edge.entry((u.min(v), u.max(v))).or_default().push(h);
    }
    let mut twins = vec![usize::MAX; half_edges.origin.len()];
    for (&(u, v), sides) in &by_edge {
        let forward = |&h: &usize| half_edges.origin[h] == u;
        let ahead = sides.iter().filter(|h| forward(h)).count();
        if 2 * ahead != sides.len() {
            return None;
        }
        // Around the edge, sorted by the direction each face leaves it in. The solid lies
        // clockwise of a forward side (seen along the edge), so its twin is the side just before.
        let axis = sub(pool.get(v), pool.get(u));
        let (x, y) = perpendicular(axis);
        let mut around: Vec<(f64, usize)> = sides
            .iter()
            .map(|&h| {
                let along = if forward(&h) { axis } else { scale(axis, -1.0) };
                let into = cross(planes[half_edges.face(h)].0, along);
                (libm::atan2(dot(into, y), dot(into, x)), h)
            })
            .collect();
        around.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (i, &(_, h)) in around.iter().enumerate() {
            let (_, twin) = around[(i + around.len() - 1) % around.len()];
            if forward(&h) {
                if forward(&twin) {
                    return None;
                }
                twins[h] = twin;
                twins[twin] = h;
            }
        }
    }
    Some(twins)
}

/// Two unit vectors perpendicular to `axis` and to each other.
fn perpendicular(axis: Vector) -> (Vector, Vector) {
    let k = (0..3).fold(0, |best, i| {
        if axis[i].abs() < axis[best].abs() {
            i
        } else {
            best
        }
    });
    let mut helper = [0.0; 3];
    helper[k] = 1.0;
    let x = cross(axis, helper);
    let x = scale(x, 1.0 / length(x));
    let y = cross(axis, x);
    (x, scale(y, 1.0 / length(y)))
}

/// Gives each fan of faces around a vertex its own vertex, so lumps touching at an edge or a
/// point keep their own vertices.
fn separate_lumps(faces: &mut Faces, twins: &[usize], pool: &mut Pool) {
    let half_edges = HalfEdges::new(faces);
    let mut visited = vec![false; twins.len()];
    let mut seen = BTreeSet::new();
    for start in 0..twins.len() {
        if visited[start] {
            continue;
        }
        let vertex = half_edges.origin[start];
        let renamed = if seen.insert(vertex) {
            vertex
        } else {
            pool.duplicate(vertex)
        };
        let mut h = start;
        while !visited[h] {
            visited[h] = true;
            let (f, r, i) = half_edges.slot[h];
            faces[f][r][i] = renamed;
            h = half_edges.next[twins[h]];
        }
    }
}

/// Merges adjacent faces that lie in the same plane within the tolerance and face the same way.
fn merge_coplanar(
    faces: &Faces,
    planes: &[(Vector, Vector)],
    twins: &[usize],
    pool: &Pool,
) -> Faces {
    let half_edges = HalfEdges::new(faces);
    // Whether the two faces, taken as one, are planar within the tolerance: the test the solid's
    // checks will apply to the merged face. It is not enough on its own: a face a few
    // tolerances wide, square to its neighbour, would pass, its width split on either side of
    // the plane fitted to both. So the vertices of both must also lie within the tolerance of
    // one face's plane; requiring both planes would reject a small face tilted by a rounding
    // error next to a large one.
    let flat_together = |f: usize, g: usize| {
        let both: Vec<Vec<usize>> = faces[f].iter().chain(&faces[g]).cloned().collect();
        let near = |(normal, centre): (Vector, Vector)| {
            both.iter()
                .flatten()
                .all(|&v| dot(normal, sub(pool.get(v), centre)).abs() <= WITHIN_TOLERANCE)
        };
        near(plane(&both, pool)) && (near(planes[f]) || near(planes[g]))
    };
    let mut group: Vec<usize> = (0..faces.len()).collect();
    for (h, &twin) in twins.iter().enumerate() {
        let (f, g) = (half_edges.face(h), half_edges.face(twin));
        // Faces sharing several edges are tested once: later edges find them merged.
        if f < g
            && find(&mut group, f) != find(&mut group, g)
            && dot(planes[f].0, planes[g].0) > 0.0
            && flat_together(f, g)
        {
            union(&mut group, f, g);
        }
    }
    let mut size = vec![0_usize; faces.len()];
    for f in 0..faces.len() {
        size[find(&mut group, f)] += 1;
    }
    // The edges between a group and the rest, in one pass over the half-edges.
    let mut boundaries: BTreeMap<usize, BTreeSet<(usize, usize)>> = BTreeMap::new();
    for (h, &twin) in twins.iter().enumerate() {
        let (own, across) = (group[half_edges.face(h)], group[half_edges.face(twin)]);
        if own != across {
            let edge = (half_edges.origin[h], half_edges.target(h));
            boundaries.entry(own).or_default().insert(edge);
        }
    }
    let mut merged = Vec::new();
    for (f, face) in faces.iter().enumerate() {
        match size[f] {
            0 => continue,
            1 => {
                merged.push(face.clone());
                continue;
            }
            _ => {}
        }
        let projection = Projection::along(planes[f].0);
        let at = move |v: usize| projection.apply(pool.get(v));
        merged.extend(faces_of(&boundaries[&f], at));
    }
    merged
}

/// Removes the vertices with only two neighbours that lie, within the tolerance, on the segment
/// between them. Such a vertex sits on the edge of two faces only, so dropping it changes no
/// shape; one between two faces left unmerged at a fold stays, or the faces would bend. The test
/// depends only on the vertex and its neighbours, so both faces along the edge agree.
fn remove_collinear(mut faces: Faces, pool: &Pool) -> Faces {
    let mut neighbours: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for ring in faces.iter().flatten() {
        for (u, v) in edges(ring) {
            neighbours.entry(u).or_default().insert(v);
            neighbours.entry(v).or_default().insert(u);
        }
    }
    let removable: BTreeSet<usize> = neighbours
        .iter()
        .filter_map(|(&v, around)| {
            let mut around = around.iter();
            match (around.next(), around.next(), around.next()) {
                (Some(&u), Some(&w), None) => Some((v, u, w)),
                _ => None,
            }
        })
        .filter(|&(v, u, w)| {
            segment_distance_squared(pool.get(v), pool.get(u), pool.get(w))
                <= WITHIN_TOLERANCE_SQUARED
        })
        .map(|(v, _, _)| v)
        .collect();
    for ring in faces.iter_mut().flatten() {
        ring.retain(|v| !removable.contains(v));
    }
    faces
}

/// Removes the lumps, and the voids, no thicker than the tolerance on average: the solid's checks
/// would reject them, and the result is regularised, so they vanish (ADR 0001, merge pass
/// steps 1 and 4). Lumps share no vertex by now, so faces sharing one belong to the same lump.
fn drop_thin_lumps(faces: Faces, pool: &Pool) -> Faces {
    let mut lump: Vec<usize> = (0..faces.len()).collect();
    let mut owner: BTreeMap<usize, usize> = BTreeMap::new();
    for (f, face) in faces.iter().enumerate() {
        for &v in face.iter().flatten() {
            let g = *owner.entry(v).or_insert(f);
            let (rf, rg) = (find(&mut lump, f), find(&mut lump, g));
            lump[rf.max(rg)] = rf.min(rg);
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

/// Numbers the used vertices in order and builds the solid, which validates it.
fn build(faces: &Faces, pool: &Pool) -> Result<Option<Solid>, BooleanError> {
    let used: BTreeSet<usize> = faces.iter().flatten().flatten().copied().collect();
    let index: BTreeMap<usize, usize> = used.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    let vertices = used
        .iter()
        .map(|&v| {
            let [x, y, z] = pool.get(v);
            Point3::new(x, y, z)
        })
        .collect();
    let rename = |ring: &Vec<usize>| ring.iter().map(|v| index[v]).collect();
    let faces: Vec<FaceInput> = faces
        .iter()
        .map(|face| FaceInput {
            outer: rename(&face[0]),
            holes: face[1..].iter().map(rename).collect(),
        })
        .collect();
    Ok(Some(Solid::from_faces(vertices, &faces)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BrepError;

    fn planes(faces: &Faces, pool: &Pool) -> Vec<(Vector, Vector)> {
        faces.iter().map(|face| plane(face, pool)).collect()
    }

    #[test]
    fn an_open_edge_is_reported() {
        let mut pool = Pool::of(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        let faces = vec![vec![vec![0, 1, 2]]];
        assert_eq!(pair(&faces, &planes(&faces, &pool), &pool), None);
        assert_eq!(
            assemble(faces, &mut pool),
            Err(BooleanError::InvalidResult(BrepError::OpenEdge { face: 0 }))
        );
    }

    #[test]
    fn sides_that_cannot_pair_around_an_edge_are_reported() {
        // Four triangles on the edge from 0 to 1, the two running from 0 to 1 next to each
        // other: no lump lies between a side and its twin.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 0.5],
            [0.0, 1.0, 0.5],
            [-1.0, 0.0, 0.5],
            [0.0, -1.0, 0.5],
        ]);
        let faces = vec![
            vec![vec![0, 1, 2]],
            vec![vec![0, 1, 3]],
            vec![vec![1, 0, 4]],
            vec![vec![1, 0, 5]],
        ];
        assert_eq!(pair(&faces, &planes(&faces, &pool), &pool), None);
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
    fn nothing_left_is_none() {
        let mut pool = Pool::of(vec![[0.0; 3], [1.0, 0.0, 0.0]]);
        assert_eq!(assemble(vec![vec![vec![0, 1, 0]]], &mut pool), Ok(None));
    }
}
