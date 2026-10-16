//! Splitting faces along the segments the other operand left on them.
//!
//! Every line, an edge of a face or a segment, is noded once for all the faces along it: split
//! where it crosses another line of a face it lies in (decided with the exact `orient2d`), and at
//! every vertex within the tolerance of it, such as where it crosses a plane of the other
//! operand. Two faces sharing a line then split it the same way, so their pieces meet edge to
//! edge without a later pass splitting edges to make them agree. Each face's lines, so split,
//! are planar, and following them gives the regions the face is cut into.

use std::collections::{BTreeMap, BTreeSet};

use super::face::{PlanarFace, Pool, segment_distance_squared, sorted_by_x, vertices_on_edge};
use super::intersect::Segments;
use super::planar::{Regions, chain, regions};
use super::snap::cell_of;
use crate::predicates::orient2d;
use crate::tolerance::WITHIN_TOLERANCE_SQUARED;
use crate::vector::{Vector, add, distance_squared, dot, edges, scale, sub};
use crate::{Orientation, Point2};

/// A line between two vertices, its ends in increasing order.
pub(super) type Line = (usize, usize);

/// The line between two vertices.
pub(super) const fn line(u: usize, v: usize) -> Line {
    if u < v { (u, v) } else { (v, u) }
}

/// The lines of every face, and for every line the vertices splitting it, in order from its
/// first end.
pub(super) struct Chains {
    lines: Vec<Vec<Line>>,
    along: BTreeMap<Line, Vec<usize>>,
}

impl Chains {
    /// For each edge of the pieces, its ends in increasing order, the lines it is part of, the
    /// longest first: several lines may run along one edge, such as an edge of one operand along
    /// which a face of the other is cut.
    pub(super) fn holding(&self, pool: &Pool) -> BTreeMap<Line, Vec<Line>> {
        let mut holding: BTreeMap<Line, Vec<Line>> = BTreeMap::new();
        for (&(u, v), along) in &self.along {
            let mut from = u;
            for &to in along.iter().chain(std::iter::once(&v)) {
                holding.entry(line(from, to)).or_default().push((u, v));
                from = to;
            }
        }
        let length = |&(u, v): &Line| distance_squared(pool.get(u), pool.get(v));
        for lines in holding.values_mut() {
            lines.sort_by(|a, b| length(b).total_cmp(&length(a)).then(a.cmp(b)));
        }
        holding
    }
}

/// The lines of a face: its edges and the segments cut on it, without repeats.
fn lines_of(face: &PlanarFace, segments: &[(usize, usize)]) -> Vec<Line> {
    let lines: BTreeSet<Line> = face
        .rings
        .iter()
        .flat_map(|ring| edges(ring))
        .chain(segments.iter().copied())
        .filter(|&(u, v)| u != v)
        .map(|(u, v)| line(u, v))
        .collect();
    lines.into_iter().collect()
}

/// Nodes the lines of all faces once: each is split at its crossings with the other lines of
/// each face it lies in, then at every vertex of any line or crossing within the tolerance of it.
/// Vertices of other faces count too: a corner of one operand on an edge of the other splits it
/// even where no face holds both.
pub(super) fn node(faces: &[PlanarFace], segments: &Segments, pool: &mut Pool) -> Chains {
    let lines: Vec<Vec<Line>> = faces
        .iter()
        .zip(segments)
        .map(|(face, segments)| lines_of(face, segments))
        .collect();
    let mut splits: BTreeMap<Line, Vec<usize>> = lines
        .iter()
        .flatten()
        .map(|&line| (line, Vec::new()))
        .collect();
    let mut vertices = Nearby::default();
    for &(u, v) in splits.keys() {
        vertices.insert(u, pool);
        vertices.insert(v, pool);
    }
    // Each face decides in its own projection whether two of its lines cross, so that its lines
    // stay planar: faces a tolerance apart may project differently. Lines crossing in several
    // faces, such as edges of coplanar faces, share one vertex: later faces find it taken.
    for (face, lines) in faces.iter().zip(&lines) {
        let at = |pool: &Pool, v: usize| face.projection.apply(pool.get(v));
        for (first, second) in pairs_of_boxes(lines, |v| at(pool, v)) {
            if let Some(vertex) = crossing(first, second, &vertices, pool, &at) {
                for line in [first, second] {
                    if let Some(split) = splits.get_mut(&line) {
                        split.push(vertex);
                    }
                }
                vertices.insert(vertex, pool);
            }
        }
    }
    let by_x = sorted_by_x(pool, vertices.all());
    let along = splits
        .into_iter()
        .map(|((u, v), mut split)| {
            split.extend(vertices_on_edge(pool, &by_x, (u, v)));
            let (p, d) = (pool.get(u), sub(pool.get(v), pool.get(u)));
            let mut chain: Vec<(f64, usize)> = split
                .into_iter()
                .map(|w| (dot(sub(pool.get(w), p), d), w))
                .collect();
            // A vertex found twice has the same key both times, so its copies end up together.
            chain.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            chain.dedup_by_key(|&mut (_, w)| w);
            ((u, v), chain.into_iter().map(|(_, w)| w).collect())
        })
        .collect();
    Chains { lines, along }
}

/// Vertices bucketed by cells one tolerance wide, as the vertex merge does, so the vertices near
/// a point are found without looking at all of them.
#[derive(Default)]
struct Nearby(BTreeMap<[i64; 3], Vec<usize>>);

impl Nearby {
    /// Adds a vertex; adding one twice is harmless.
    fn insert(&mut self, vertex: usize, pool: &Pool) {
        self.0
            .entry(cell_of(pool.get(vertex)))
            .or_default()
            .push(vertex);
    }

    fn all(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.values().flatten().copied()
    }

    /// Whether a vertex lies within the tolerance of a point.
    fn any_within(&self, point: Vector, pool: &Pool) -> bool {
        let [x, y, z] = cell_of(point);
        (-1..=1)
            .flat_map(|dx| {
                (-1..=1).flat_map(move |dy| (-1..=1).map(move |dz| [x + dx, y + dy, z + dz]))
            })
            .filter_map(|cell| self.0.get(&cell))
            .flatten()
            .any(|&vertex| distance_squared(point, pool.get(vertex)) <= WITHIN_TOLERANCE_SQUARED)
    }
}

/// The pairs of lines, as given and in order, whose boxes in the plane overlap. Lines are
/// sorted and swept along x, so only those overlapping in x are compared.
fn pairs_of_boxes(lines: &[Line], at: impl Fn(usize) -> Point2) -> Vec<(Line, Line)> {
    let boxes: Vec<[f64; 4]> = lines
        .iter()
        .map(|&(u, v)| {
            let (p, q) = (at(u), at(v));
            [p.x.min(q.x), p.x.max(q.x), p.y.min(q.y), p.y.max(q.y)]
        })
        .collect();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&i, &j| boxes[i][0].total_cmp(&boxes[j][0]).then(i.cmp(&j)));
    let mut pairs = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        for &j in order[k + 1..]
            .iter()
            .take_while(|&&j| boxes[j][0] <= boxes[i][1])
        {
            if boxes[j][2] <= boxes[i][3] && boxes[i][2] <= boxes[j][3] {
                pairs.push((lines[i.min(j)], lines[i.max(j)]));
            }
        }
    }
    pairs
}

/// Cuts face `index` of the faces given to [`node`] along its lines as they were split there;
/// the regions include those outside the face, if segments reach there.
pub(super) fn split(face: &PlanarFace, index: usize, chains: &Chains, pool: &Pool) -> Regions {
    let mut pieces = BTreeSet::new();
    for &(u, v) in &chains.lines[index] {
        let mut from = u;
        for &to in chains.along[&(u, v)].iter().chain(std::iter::once(&v)) {
            pieces.insert((from, to));
            pieces.insert((to, from));
            from = to;
        }
    }
    let at = |v: usize| face.projection.apply(pool.get(v));
    regions(chain(&pieces, at), at)
}

/// The vertex where two lines cross in their interiors, if they do. The decision is exact; the
/// point is computed once. A crossing within the tolerance of an existing vertex is left to the
/// splitting of lines at vertices instead, which puts that vertex on both lines.
fn crossing(
    first: Line,
    second: Line,
    vertices: &Nearby,
    pool: &mut Pool,
    at: &impl Fn(&Pool, usize) -> Point2,
) -> Option<usize> {
    let ends = [first.0, first.1, second.0, second.1];
    if ends[..2].iter().any(|end| ends[2..].contains(end)) {
        return None;
    }
    // Lines that touch or overlap within the tolerance are split at each other's ends instead:
    // nearly collinear lines would otherwise cross at an arbitrary point along their overlap.
    let near = |vertex: usize, (start, end): Line| {
        segment_distance_squared(pool.get(vertex), pool.get(start), pool.get(end))
            <= WITHIN_TOLERANCE_SQUARED
    };
    if near(first.0, second)
        || near(first.1, second)
        || near(second.0, first)
        || near(second.1, first)
    {
        return None;
    }
    let [a, b, c, d] = ends.map(|vertex| at(pool, vertex));
    let side = |from, to, point| orient2d(from, to, point).unwrap_or(Orientation::Zero);
    let opposite = |one: Orientation, other: Orientation| {
        one != Orientation::Zero && other != Orientation::Zero && one != other
    };
    if !opposite(side(a, b, c), side(a, b, d)) || !opposite(side(c, d, a), side(c, d, b)) {
        return None;
    }
    let (along, across) = (
        Point2::new(b.x - a.x, b.y - a.y),
        Point2::new(d.x - c.x, d.y - c.y),
    );
    let fraction = ((c.x - a.x) * across.y - (c.y - a.y) * across.x)
        / (along.x * across.y - along.y * across.x);
    let (start, end) = (pool.get(first.0), pool.get(first.1));
    let point = add(start, scale(sub(end, start), fraction));
    (!vertices.any_within(point, pool)).then(|| pool.push(point))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossing_cuts_meet_at_a_new_vertex() {
        // A unit square cut along both diagonals: four triangles around a new centre vertex.
        let mut pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ]);
        let face = PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool);
        let segments = vec![vec![(0, 2), (1, 3)]];
        let chains = node(std::slice::from_ref(&face), &segments, &mut pool);
        let pieces = split(&face, 0, &chains, &pool);
        assert_eq!(pieces.regions.len(), 4);
        assert!(distance_squared(pool.get(4), [0.5, 0.5, 0.0]) < 1e-30);
        assert!(
            pieces
                .regions
                .iter()
                .all(|region| region.outer.contains(&4))
        );
    }

    #[test]
    fn faces_along_a_line_split_it_alike() {
        // A unit square as two triangles sharing its diagonal. A segment cut on the first ends
        // at vertex 4, the middle of the diagonal: the second triangle, which has no line ending
        // there, must still split the diagonal at 4, or the two would not share an edge.
        let mut pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.5, 0.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2]], &pool),
            PlanarFace::new(vec![vec![0, 2, 3]], &pool),
        ];
        let chains = node(&faces, &vec![vec![(1, 4)], Vec::new()], &mut pool);
        let first = split(&faces[0], 0, &chains, &pool);
        let second = split(&faces[1], 1, &chains, &pool);
        assert_eq!(first.regions.len(), 2);
        assert_eq!(second.regions.len(), 1);
        assert_eq!(
            second.regions[0].outer.iter().filter(|&&v| v == 4).count(),
            1
        );
    }
}
