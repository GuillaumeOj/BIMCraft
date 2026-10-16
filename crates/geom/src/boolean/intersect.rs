//! Where the faces of one operand meet the faces of the other.
//!
//! Each pair of faces whose boxes overlap gives the segments where the two faces overlap along
//! the line their planes share. Coplanar pairs exchange their boundaries instead, so each can
//! later be split where the other starts and ends. Segments go to both faces.

use std::collections::BTreeMap;

use super::face::{PlanarFace, Pool, meeting_direction};
use crate::tolerance::WITHIN_TOLERANCE;
use crate::vector::{add, dot, edges, length, scale, sub};

/// The segments to insert into each face, as vertex pairs of the pool.
pub(super) type Segments = Vec<Vec<(usize, usize)>>;

/// Intersects every face of the first operand (`faces[..split]`) with every face of the second
/// whose bounding box overlaps it.
pub(super) fn segments(faces: &[PlanarFace], split: usize, pool: &mut Pool) -> Segments {
    let mut out = vec![Vec::new(); faces.len()];
    let mut crossings = BTreeMap::new();
    let mut neighbours = vec![Vec::new(); pool.len()];
    for (u, v) in faces
        .iter()
        .flat_map(|face| &face.rings)
        .flat_map(|ring| edges(ring))
    {
        neighbours[u].push(v);
        neighbours[v].push(u);
    }
    for (f, g) in candidate_pairs(faces, split) {
        let mut pair = Pair {
            faces,
            pool: &mut *pool,
            crossings: &mut crossings,
            neighbours: &neighbours,
        };
        for segment in pair.meet(f, g) {
            out[f].push(segment);
            out[g].push(segment);
        }
    }
    out
}

/// The pairs of a face of the first operand (`faces[..split]`) and a face of the second whose
/// bounding boxes overlap, in a fixed order. Faces are sorted by `x` first, so far-apart faces
/// are never compared.
pub(super) fn candidate_pairs(faces: &[PlanarFace], split: usize) -> Vec<(usize, usize)> {
    let mut second: Vec<usize> = (split..faces.len()).collect();
    second.sort_by(|&f, &g| {
        faces[f]
            .min_x()
            .total_cmp(&faces[g].min_x())
            .then(f.cmp(&g))
    });
    let mut pairs = Vec::new();
    for f in 0..split {
        let reach = faces[f].max_x() + WITHIN_TOLERANCE;
        let end = second.partition_point(|&g| faces[g].min_x() <= reach);
        pairs.extend(
            second[..end]
                .iter()
                .filter(|&&g| faces[f].overlaps(&faces[g]))
                .map(|&g| (f, g)),
        );
    }
    pairs
}

/// The vertex where an edge crosses a face's plane, keyed by the edge and the face so that both
/// faces along the edge get the same vertex.
type Crossings = BTreeMap<((usize, usize), usize), usize>;

struct Pair<'a> {
    faces: &'a [PlanarFace],
    pool: &'a mut Pool,
    crossings: &'a mut Crossings,
    /// The vertices joined to each vertex by an edge of either operand: a vertex the operands
    /// share after snapping has the edges of both.
    neighbours: &'a [Vec<usize>],
}

impl Pair<'_> {
    fn meet(&mut self, first: usize, second: usize) -> Vec<(usize, usize)> {
        let (one, other) = (&self.faces[first], &self.faces[second]);
        let sides_a = self.sides(one, other);
        let sides_b = self.sides(other, one);
        if apart(&sides_a) || apart(&sides_b) {
            return Vec::new();
        }
        let flat = |sides: &[Vec<i8>]| sides.iter().flatten().all(|&s| s == 0);
        // The same test as `PlanarFace::coplanar`, from the sides already worked out.
        if flat(&sides_a) || flat(&sides_b) {
            return one
                .rings
                .iter()
                .chain(&other.rings)
                .flat_map(|ring| edges(ring))
                .collect();
        }
        // Exactly parallel planes with vertices on both sides of the tolerance band share no
        // line: ordering points along a zero direction would join them at random.
        let Some(direction) = meeting_direction(one, other) else {
            return Vec::new();
        };
        let sides_a = self.settled(one, sides_a, other);
        let sides_b = self.settled(other, sides_b, one);
        let mut points = self.on_plane(first, &sides_a, second);
        points.extend(self.on_plane(second, &sides_b, first));
        let pool = &*self.pool;
        let mut points: Vec<(f64, usize)> = points
            .into_iter()
            .map(|v| (dot(direction, pool.get(v)), v))
            .collect();
        points.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
        points.dedup_by_key(|p| p.1);
        points
            .windows(2)
            .filter_map(|pair| {
                let (u, v) = (pair[0].1, pair[1].1);
                let middle = scale(add(pool.get(u), pool.get(v)), 0.5);
                (one.covers(pool, middle) && other.covers(pool, middle)).then_some((u, v))
            })
            .collect()
    }

    /// The side of `plane` each vertex of `face` lies on, ring by ring.
    fn sides(&self, face: &PlanarFace, plane: &PlanarFace) -> Vec<Vec<i8>> {
        face.rings
            .iter()
            .map(|ring| ring.iter().map(|&v| plane.side(self.pool.get(v))).collect())
            .collect()
    }

    /// The sides of [`Self::sides`], with each vertex within the tolerance of the plane put on
    /// the side it actually lies on if one of its edges crosses the plane farther than the
    /// tolerance from it. Such an edge runs along the plane at a shallow angle, and the cuts of
    /// the faces along it must agree: a face whose cut stopped at the vertex would leave the
    /// sliver between the vertex and the crossing open, while a neighbouring face's cut runs on
    /// to the crossing. The test depends only on the vertex, its edges and the plane, so every
    /// face around the vertex agrees.
    fn settled(
        &self,
        face: &PlanarFace,
        mut sides: Vec<Vec<i8>>,
        plane: &PlanarFace,
    ) -> Vec<Vec<i8>> {
        for (ring, sides) in face.rings.iter().zip(&mut sides) {
            for (&v, side) in ring.iter().zip(sides.iter_mut()) {
                if *side != 0 {
                    continue;
                }
                let p = self.pool.get(v);
                let d = plane.distance(p);
                let crosses_far = self.neighbours[v].iter().any(|&w| {
                    let q = self.pool.get(w);
                    let e = plane.distance(q);
                    // `e` beyond the tolerance is `plane.side(q) != 0`, without a second product.
                    d * e < 0.0
                        && e.abs() > WITHIN_TOLERANCE
                        && d / (d - e) * length(sub(q, p)) > WITHIN_TOLERANCE
                });
                if crosses_far {
                    *side = if d > 0.0 { 1 } else { -1 };
                }
            }
        }
        sides
    }

    /// The points where the boundary of face `f` meets the plane of face `g`: its vertices on
    /// the plane, and the crossings of its edges.
    fn on_plane(&mut self, f: usize, sides: &[Vec<i8>], g: usize) -> Vec<usize> {
        let mut points = Vec::new();
        for (ring, sides) in self.faces[f].rings.iter().zip(sides) {
            for ((u, v), (su, sv)) in edges(ring).zip(edges(sides)) {
                if su == 0 {
                    points.push(u);
                } else if su * sv < 0 {
                    points.push(self.crossing(u.min(v), u.max(v), g));
                }
            }
        }
        points
    }

    fn crossing(&mut self, start: usize, end: usize, face: usize) -> usize {
        let key = ((start, end), face);
        if let Some(&vertex) = self.crossings.get(&key) {
            return vertex;
        }
        let plane = &self.faces[face];
        let (from, to) = (self.pool.get(start), self.pool.get(end));
        let (above, below) = (plane.distance(from), plane.distance(to));
        let vertex = self
            .pool
            .push(add(from, scale(sub(to, from), above / (above - below))));
        self.crossings.insert(key, vertex);
        vertex
    }
}

/// Whether every vertex lies strictly on the same side.
fn apart(sides: &[Vec<i8>]) -> bool {
    let mut all = sides.iter().flatten();
    all.clone().all(|&s| s > 0) || all.all(|&s| s < 0)
}
