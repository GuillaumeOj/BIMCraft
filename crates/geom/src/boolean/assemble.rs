//! Turning the kept pieces of faces into a valid solid: the rest of the merge pass of ADR 0001.
//!
//! - Where more than two faces meet at an edge (lumps touching along it), each side is paired
//!   with the face across the same lump; lumps touching at an edge or a point then get their own
//!   copies of the vertices, so no non-manifold edge or vertex is created.
//! - The merge pass's local operations then run on the paired faces (see `merge`): edges no
//!   longer than the tolerance collapse first, then the rest.
//! - A face whose boundary passes through a vertex twice, such as one notched up to a corner,
//!   is split in two along a segment inside it.

use std::collections::{BTreeMap, BTreeSet};

use super::BooleanError;
use super::face::{Pool, Projection, clean_face, plane, ring_contains};
use super::merge::{
    cancel_back_to_back, collapse_short_edges, drop_thin_lumps, merge_coplanar, remove_collinear,
    remove_slivers, sliver_remains,
};
use super::planar::regions;
use crate::brep::longest_edge_squared;
use crate::predicates::orient2d;
use crate::vector::{Vector, cross, dot, edges, length, newell, scale, sub};
use crate::{FaceInput, Orientation, Point2, Point3, Solid};

/// Faces as rings of pool vertices, the outer ring first.
pub(super) type Faces = Vec<Vec<Vec<usize>>>;

/// Builds the solid bounded by the faces; `None` if no face is left.
pub(super) fn assemble(mut faces: Faces, pool: &mut Pool) -> Result<Option<Solid>, BooleanError> {
    clean(&mut faces);
    if faces.is_empty() {
        return Ok(None);
    }
    // The cut merged no vertices within the tolerance of each other: the merge pass does, on
    // the classified result, once lumps that only touch have their own vertices.
    let Some((twins, _)) = paired(&mut faces, pool) else {
        return build(&faces, pool);
    };
    separate_lumps(&mut faces, &twins, pool);
    collapse_short_edges(&mut faces, pool)?;
    cancel_back_to_back(&mut faces);
    let Some(first) = paired(&mut faces, pool) else {
        return build(&faces, pool);
    };
    separate_lumps(&mut faces, &first.0, pool);
    // Each round merges coplanar faces, then zips slivers shut, which can leave coplanar faces
    // side by side: rounds run until no sliver is zipped. Each round compares with the planes
    // of the faces as they are, collapsed and merged so far.
    let mut paired_faces = Some(first);
    while let Some((twins, planes)) = paired_faces {
        faces = remove_collinear(merge_coplanar(&faces, &planes, &twins, pool), pool);
        // Where the solid touches itself along an edge in the middle of a face, merging the
        // face's pieces from both sides left the edge to the faces that meet there; the lumps
        // that only touched through it may now touch at a vertex, so they are separated again.
        paired_faces = paired(&mut faces, pool);
        let Some((twins, _)) = &paired_faces else {
            break;
        };
        separate_lumps(&mut faces, twins, pool);
        if !remove_slivers(&mut faces, pool) {
            break;
        }
        paired_faces = paired(&mut faces, pool);
    }
    let faces = drop_thin_lumps(faces, pool);
    if faces.is_empty() {
        return Ok(None);
    }
    let faces = split_pinched(faces, pool);
    // Faces that do not pair are the solid's checks' to report.
    if paired_faces.is_some() {
        sliver_remains(&faces, pool)?;
    }
    build(&faces, pool)
}

fn planes_of(faces: &Faces, pool: &Pool) -> Vec<(Vector, Vector)> {
    faces.iter().map(|face| plane(face, pool)).collect()
}

/// The twin of each half-edge, and the plane of each face.
type Paired = (Vec<usize>, Vec<(Vector, Vector)>);

/// Cleans the faces, then pairs their half-edges; `None` if an edge cannot be paired.
fn paired(faces: &mut Faces, pool: &Pool) -> Option<Paired> {
    clean(faces);
    let planes = planes_of(faces, pool);
    Some((pair(faces, &planes, pool)?, planes))
}

/// Drops repeated vertices and rings too short to bound anything, then the faces left empty.
fn clean(faces: &mut Faces) {
    faces.iter_mut().for_each(clean_face);
    faces.retain(|face| !face.is_empty());
}

/// The half-edges of the faces, in a fixed order: face by face, ring by ring.
pub(super) struct HalfEdges {
    pub(super) origin: Vec<usize>,
    next: Vec<usize>,
    /// Where each half-edge's origin sits: face, ring and position.
    slot: Vec<(usize, usize, usize)>,
}

impl HalfEdges {
    pub(super) fn new(faces: &Faces) -> Self {
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

    pub(super) fn face(&self, h: usize) -> usize {
        self.slot[h].0
    }

    pub(super) fn target(&self, h: usize) -> usize {
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
        let normals: Vec<(bool, Vector)> = (sides.iter())
            .map(|&h| (forward(&h), planes[half_edges.face(h)].0))
            .collect();
        for (k, twin) in twins_around(pool.get(u), pool.get(v), &normals)? {
            twins[sides[k]] = sides[twin];
            twins[sides[twin]] = sides[k];
        }
    }
    Some(twins)
}

/// The twin of each side running from `u` to `v`, the sides given as for [`around_edge`]: as
/// pairs of positions in `sides`, the forward side first. The solid lies clockwise of a forward
/// side (seen along the edge), so its twin is the side just before; `None` if that side runs
/// forward too.
pub(super) fn twins_around(
    u: Vector,
    v: Vector,
    sides: &[(bool, Vector)],
) -> Option<Vec<(usize, usize)>> {
    let order = around_edge(u, v, sides.iter().copied());
    let mut twins = Vec::new();
    for (i, &k) in order.iter().enumerate() {
        if !sides[k].0 {
            continue;
        }
        let twin = order[(i + order.len() - 1) % order.len()];
        if sides[twin].0 {
            return None;
        }
        twins.push((k, twin));
    }
    Some(twins)
}

/// The sides of the faces along an edge from `u` to `v`, given as whether each runs from `u`
/// to `v` and its face's normal, in order around the edge: sorted by the direction each face
/// leaves it in, equal directions by their order given.
pub(super) fn around_edge(
    u: Vector,
    v: Vector,
    sides: impl Iterator<Item = (bool, Vector)>,
) -> Vec<usize> {
    let axis = sub(v, u);
    let (x, y) = perpendicular(axis);
    let mut around: Vec<(f64, usize)> = sides
        .enumerate()
        .map(|(i, (forward, normal))| {
            let along = if forward { axis } else { scale(axis, -1.0) };
            let into = cross(normal, along);
            (libm::atan2(dot(into, y), dot(into, x)), i)
        })
        .collect();
    around.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    around.into_iter().map(|(_, i)| i).collect()
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

/// Splits each face whose rings pass through a vertex more than once into faces whose rings do
/// not, which the solid's checks require. Such a face is left where a piece cut off a face
/// touches its boundary at a single vertex: a notch reaching a corner, or two holes touching.
/// It cannot keep the notch as a hole either: a hole touching the outer ring leaves the face a
/// disc, not a ring, and the Euler–Poincaré count of the shell would be off by one.
fn split_pinched(faces: Faces, pool: &Pool) -> Faces {
    faces
        .into_iter()
        .flat_map(|face| {
            if face.iter().all(|ring| repeat(ring).is_none()) {
                return vec![face];
            }
            let projection = Projection::along(plane(&face, pool).0);
            let at = |v: usize| projection.apply(pool.get(v));
            let mut rings = face;
            while let Some((r, repeat)) = rings
                .iter()
                .enumerate()
                .find_map(|(r, ring)| Some((r, repeat(ring)?)))
            {
                let Some(chord) = chord(&rings, r, repeat, pool, at) else {
                    break;
                };
                let ring = rings.swap_remove(r);
                rings.extend(split_ring(&ring, chord));
            }
            regions(rings, at)
                .regions
                .into_iter()
                .map(|region| std::iter::once(region.outer).chain(region.holes).collect())
                .collect()
        })
        .collect()
}

/// The first two positions in a ring holding the same vertex.
fn repeat(ring: &[usize]) -> Option<(usize, usize)> {
    let mut first = BTreeMap::new();
    ring.iter()
        .enumerate()
        .find_map(|(j, &v)| first.insert(v, j).map(|i| (i, j)))
}

/// The positions in ring `r`, one strictly between its two passes through a vertex and one
/// strictly outside them, whose vertices a segment inside the face joins: of those, the segment
/// that leaves the narrower part widest, as the solid's checks measure width (twice the area
/// over the longest edge), so neither part is thin. `None` if there is none.
fn chord(
    rings: &[Vec<usize>],
    r: usize,
    (first, second): (usize, usize),
    pool: &Pool,
    at: impl Fn(usize) -> Point2 + Copy,
) -> Option<(usize, usize)> {
    let ring = &rings[r];
    let pinch = ring[first];
    let between = (first + 1..second).filter(|&k| ring[k] != pinch);
    let outside: Vec<usize> = (second + 1..ring.len())
        .chain(0..first)
        .filter(|&k| ring[k] != pinch)
        .collect();
    let width_squared = |part: &[usize]| {
        let points: Vec<Vector> = part.iter().map(|&v| pool.get(v)).collect();
        let area = newell(&points);
        dot(area, area) / longest_edge_squared(&points)
    };
    let narrower = |chord: (usize, usize)| {
        let [one, other] = split_ring(ring, chord);
        width_squared(&one).min(width_squared(&other))
    };
    between
        .flat_map(|k| outside.iter().map(move |&l| (k, l)))
        .filter(|&(k, l)| ring[k] != ring[l] && inside(rings, (ring[k], ring[l]), at))
        .map(|chord| (narrower(chord), chord))
        .max_by(|one, other| one.0.total_cmp(&other.0))
        .map(|(_, chord)| chord)
}

/// Whether the segment between two vertices runs inside the face: it meets no edge of the rings
/// but at its ends, decided exactly, and its middle is inside.
fn inside(rings: &[Vec<usize>], (from, to): (usize, usize), at: impl Fn(usize) -> Point2) -> bool {
    let (start, end) = (at(from), at(to));
    let side = |a, b, point| orient2d(a, b, point).unwrap_or(Orientation::Zero);
    let within = |a: Point2, b: Point2, point: Point2| {
        (a.x.min(b.x)..=a.x.max(b.x)).contains(&point.x)
            && (a.y.min(b.y)..=a.y.max(b.y)).contains(&point.y)
    };
    // An edge leaving an end meets the segment again only if it runs along it.
    let along = |vertex: usize, other: usize| {
        let (near, far) = if vertex == from {
            (start, end)
        } else {
            (end, start)
        };
        let point = at(other);
        side(near, far, point) == Orientation::Zero
            && (point.x - near.x) * (far.x - near.x) + (point.y - near.y) * (far.y - near.y) > 0.0
    };
    let crosses = |a: Point2, b: Point2| {
        let (side_a, side_b) = (side(start, end, a), side(start, end, b));
        if side_a == Orientation::Zero && side_b == Orientation::Zero {
            return within(start, end, a) || within(start, end, b) || within(a, b, start);
        }
        side_a != side_b && side(a, b, start) != side(a, b, end)
    };
    let is_end = |vertex: usize| vertex == from || vertex == to;
    let meets = |(u, v): (usize, usize)| match (is_end(u), is_end(v)) {
        (true, _) => along(u, v),
        (false, true) => along(v, u),
        (false, false) => crosses(at(u), at(v)),
    };
    if rings.iter().flat_map(|ring| edges(ring)).any(meets) {
        return false;
    }
    let middle = Point2::new(f64::midpoint(start.x, end.x), f64::midpoint(start.y, end.y));
    rings.iter().fold(false, |odd, ring| {
        odd ^ ring_contains(ring.iter().map(|&v| at(v)), middle)
    })
}

/// The two rings a ring splits into along a segment between positions `k` and `l`, each
/// keeping both ends.
fn split_ring(ring: &[usize], (k, l): (usize, usize)) -> [Vec<usize>; 2] {
    let n = ring.len();
    let walk = |from: usize, to: usize| {
        let steps = (to + n - from) % n;
        (0..=steps).map(|s| ring[(from + s) % n]).collect()
    };
    [walk(k, l), walk(l, k)]
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
    fn nothing_left_is_none() {
        let mut pool = Pool::of(vec![[0.0; 3], [1.0, 0.0, 0.0]]);
        assert_eq!(assemble(vec![vec![vec![0, 1, 0]]], &mut pool), Ok(None));
    }

    #[test]
    fn a_notch_reaching_a_corner_is_split_off() {
        // A 4 m square whose ring, after its corner 0, runs round a triangular notch back to 0.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [4.0, 4.0, 0.0],
            [0.0, 4.0, 0.0],
            [1.0, 2.0, 0.0],
            [2.0, 1.0, 0.0],
        ]);
        let faces = split_pinched(vec![vec![vec![0, 1, 2, 3, 0, 4, 5]]], &pool);
        assert_eq!(faces.len(), 2);
        assert!(faces.iter().flatten().all(|ring| repeat(ring).is_none()));
    }

    #[test]
    fn lobes_touching_at_a_vertex_have_no_chord() {
        // Two triangles meeting at vertex 0: every segment between them leaves the face, so the
        // face is left for the solid's checks to report.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, -1.0, 0.0],
            [2.0, 1.0, 0.0],
            [-2.0, 1.0, 0.0],
            [-2.0, -1.0, 0.0],
        ]);
        let face = vec![vec![0, 1, 2, 0, 3, 4]];
        assert_eq!(split_pinched(vec![face.clone()], &pool), [face]);
    }

    #[test]
    fn a_segment_along_an_edge_is_not_inside() {
        // The diagonal of a square, inside it until a triangular hole has an edge along it.
        let points = [
            (0.0, 0.0),
            (4.0, 0.0),
            (4.0, 4.0),
            (0.0, 4.0),
            (1.0, 1.0),
            (2.0, 2.0),
            (2.0, 1.0),
        ];
        let at = |v: usize| Point2::new(points[v].0, points[v].1);
        assert!(inside(&[vec![0, 1, 2, 3]], (0, 2), at));
        assert!(!inside(&[vec![0, 1, 2, 3], vec![4, 5, 6]], (0, 2), at));
    }
}
