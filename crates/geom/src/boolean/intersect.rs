//! Where the faces of one operand meet the faces of the other.
//!
//! Each pair of faces whose boxes overlap gives the segments where the two faces overlap along
//! the line their planes share. Coplanar pairs exchange their boundaries instead, so each can
//! later be split where the other starts and ends. Segments go to both faces.

use std::collections::BTreeMap;

use super::face::{PlanarFace, Pool, meeting_direction};
use crate::vector::{add, dot, edges, scale, sub};

/// The segments to insert into each face, as vertex pairs of the pool.
pub(super) type Segments = Vec<Vec<(usize, usize)>>;

/// Intersects every face of the first operand (`faces[..split]`) with every face of the second
/// whose bounding box overlaps it, sorting by `x` first so far-apart faces are never compared.
pub(super) fn segments(faces: &[PlanarFace], split: usize, pool: &mut Pool) -> Segments {
    let mut out = vec![Vec::new(); faces.len()];
    let mut crossings = BTreeMap::new();
    let mut second: Vec<usize> = (split..faces.len()).collect();
    second.sort_by(|&f, &g| {
        faces[f]
            .min_x()
            .total_cmp(&faces[g].min_x())
            .then(f.cmp(&g))
    });
    let mut pair = Pair {
        faces,
        pool,
        crossings: &mut crossings,
    };
    for f in 0..split {
        let reach = faces[f].max_x() + crate::LINEAR_TOLERANCE;
        let end = second.partition_point(|&g| faces[g].min_x() <= reach);
        for &g in &second[..end] {
            if !faces[f].overlaps(&faces[g]) {
                continue;
            }
            for segment in pair.meet(f, g) {
                out[f].push(segment);
                out[g].push(segment);
            }
        }
    }
    out
}

/// The vertex where an edge crosses a face's plane, keyed by the edge and the face so that both
/// faces along the edge get the same vertex.
type Crossings = BTreeMap<((usize, usize), usize), usize>;

struct Pair<'a> {
    faces: &'a [PlanarFace],
    pool: &'a mut Pool,
    crossings: &'a mut Crossings,
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
