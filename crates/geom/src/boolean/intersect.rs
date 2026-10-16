//! Where the faces of one operand meet the faces of the other.
//!
//! Each pair of faces whose boxes overlap gives the segments where the two faces overlap along
//! the line their planes share. Coplanar pairs exchange their boundaries instead, so each can
//! later be split where the other starts and ends. Segments go to both faces, and each segment
//! where two planes cross remembers the two faces it lies in, so that the pieces along it can
//! later be told which side of each face they lie on.
//!
//! Every vertex is recorded on the lines it was made on, its supports: an edge holds its ends
//! and the vertices where it crosses a plane; the line where two planes meet holds the points
//! where either face's boundary meets the other's plane. Whether a vertex lies on a line is
//! later read from these, never from its distance to the line.

use std::collections::{BTreeMap, BTreeSet};

use super::arrange::{Line, line};
use super::face::{PlanarFace, Pool, meeting_direction};
use crate::tolerance::WITHIN_TOLERANCE;
use crate::vector::{add, dot, edges, length, scale, sub};

/// The segments to insert into each face, as vertex pairs of the pool.
pub(super) type Segments = Vec<Vec<(usize, usize)>>;

/// For each segment where the planes of two faces cross, the faces it lies in.
pub(super) type Cuts = BTreeMap<Line, BTreeSet<usize>>;

/// A line a vertex is known to lie on from how it was made, never from how close it is: an edge
/// of an operand (its ends in increasing order), or the line where the planes of two faces meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Support {
    Edge(usize, usize),
    Meet(usize, usize),
}

/// The vertices on each support: an edge's ends and the vertices where it crosses a plane; the
/// points where two faces meet a plane-crossing line, each judged on both planes.
pub(super) type Supports = BTreeMap<Support, BTreeSet<usize>>;

/// The supports with their vertices renamed.
pub(super) fn renamed_supports(supports: Supports, rename: &[usize]) -> Supports {
    supports
        .into_iter()
        .map(|(support, on)| (support, on.into_iter().map(|v| rename[v]).collect()))
        .collect()
}

/// The cuts with their vertices renamed; those whose ends merge go.
pub(super) fn renamed(cuts: Cuts, rename: &[usize]) -> Cuts {
    let mut out = Cuts::new();
    for ((u, v), on) in cuts {
        if rename[u] != rename[v] {
            out.entry(line(rename[u], rename[v]))
                .or_default()
                .extend(on);
        }
    }
    out
}

/// Intersects every face of the first operand (`faces[..split]`) with every face of the second
/// whose bounding box overlaps it.
pub(super) fn segments(
    faces: &[PlanarFace],
    split: usize,
    pool: &mut Pool,
) -> (Segments, Cuts, Supports) {
    let mut out = vec![Vec::new(); faces.len()];
    let mut cuts = Cuts::new();
    let mut supports = Supports::new();
    let mut crossings = BTreeMap::new();
    let mut neighbours = vec![Vec::new(); pool.len()];
    for (u, v) in faces
        .iter()
        .flat_map(|face| &face.rings)
        .flat_map(|ring| edges(ring))
    {
        neighbours[u].push(v);
        neighbours[v].push(u);
        let (a, b) = line(u, v);
        supports
            .entry(Support::Edge(a, b))
            .or_default()
            .extend([a, b]);
    }
    for (f, g) in candidate_pairs(faces, split) {
        let mut pair = Pair {
            faces,
            pool: &mut *pool,
            crossings: &mut crossings,
            neighbours: &neighbours,
        };
        let meeting = pair.meet(f, g);
        if !meeting.points.is_empty() {
            supports.insert(Support::Meet(f, g), meeting.points);
        }
        for (u, v) in meeting.segments {
            out[f].push((u, v));
            out[g].push((u, v));
            // Coplanar faces' segments are their own edges, which say where each face lies.
            if !meeting.coplanar && u != v {
                cuts.entry(line(u, v)).or_default().extend([f, g]);
            }
        }
    }
    for (&((a, b), _), &vertex) in &crossings {
        supports
            .entry(Support::Edge(a, b))
            .or_default()
            .insert(vertex);
    }
    (out, cuts, supports)
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

/// How two faces meet.
#[derive(Default)]
struct Meeting {
    /// Whether they lie in one plane, within the tolerance.
    coplanar: bool,
    /// The segments to insert into both.
    segments: Vec<(usize, usize)>,
    /// The points where the boundary of either meets the plane of the other, all on the line
    /// the planes share.
    points: BTreeSet<usize>,
}

impl Pair<'_> {
    fn meet(&mut self, first: usize, second: usize) -> Meeting {
        let (one, other) = (&self.faces[first], &self.faces[second]);
        let sides_a = self.sides(one, other);
        let sides_b = self.sides(other, one);
        if apart(&sides_a) || apart(&sides_b) {
            return Meeting::default();
        }
        let flat = |sides: &[Vec<i8>]| sides.iter().flatten().all(|&s| s == 0);
        // The same test as `PlanarFace::coplanar`, from the sides already worked out.
        if flat(&sides_a) || flat(&sides_b) {
            let boundaries = one.rings.iter().chain(&other.rings);
            return Meeting {
                coplanar: true,
                segments: boundaries.flat_map(|ring| edges(ring)).collect(),
                points: BTreeSet::new(),
            };
        }
        // Exactly parallel planes with vertices on both sides of the tolerance band share no
        // line: ordering points along a zero direction would join them at random.
        let Some(direction) = meeting_direction(one, other) else {
            return Meeting::default();
        };
        let sides_a = self.settled(one, sides_a, other);
        let sides_b = self.settled(other, sides_b, one);
        let one_points = self.on_plane(first, &sides_a, second);
        let other_points = self.on_plane(second, &sides_b, first);
        let pool = &*self.pool;
        // Each point toggles whether the line is inside one face or the other, with the
        // vertices on the plane taken just above it or just below.
        let mut points: Vec<(f64, usize, [bool; 4])> = (one_points.into_iter())
            .map(|(v, [up, down])| (v, [up, down, false, false]))
            .chain((other_points.into_iter()).map(|(v, [up, down])| (v, [false, false, up, down])))
            .map(|(v, toggles)| (dot(direction, pool.get(v)), v, toggles))
            .collect();
        points.sort_by(|p, q| p.0.total_cmp(&q.0).then(p.1.cmp(&q.1)));
        let mut inside = [false; 4];
        let mut segments = Vec::new();
        for (i, &(_, v, toggles)) in points.iter().enumerate() {
            for (side, toggle) in inside.iter_mut().zip(toggles) {
                *side ^= toggle;
            }
            // Inside or on the boundary of each face: inside it one way or the other.
            if let Some(&(_, w, _)) = points.get(i + 1)
                && (inside[0] || inside[1])
                && (inside[2] || inside[3])
                && w != v
            {
                segments.push((v, w));
            }
        }
        Meeting {
            coplanar: false,
            segments,
            points: points.iter().map(|p| p.1).collect(),
        }
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
    /// the plane, and the crossings of its edges; each with whether the boundary passes from one
    /// side of the plane to the other there, with the vertices on the plane taken just above it,
    /// then just below. Taken above, the boundary passes at such a vertex once for each of its
    /// two edges leading below; taken below, once for each leading above.
    fn on_plane(&mut self, f: usize, sides: &[Vec<i8>], g: usize) -> Vec<(usize, [bool; 2])> {
        let mut points = Vec::new();
        for (ring, sides) in self.faces[f].rings.iter().zip(sides) {
            let before = sides.iter().copied().cycle().skip(sides.len() - 1);
            for (((u, v), (su, sv)), sb) in edges(ring).zip(edges(sides)).zip(before) {
                if su == 0 {
                    points.push((u, [(sb < 0) != (sv < 0), (sb > 0) != (sv > 0)]));
                } else if su * sv < 0 {
                    points.push((self.crossing(u.min(v), u.max(v), g), [true, true]));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The unit square in z = 0 facing up, then a square in x = 0.5 from y = 0.2 to 0.8,
    /// facing +x, from height `low` to `high`.
    fn square_and_wall(low: f64, high: f64) -> (Vec<PlanarFace>, Pool) {
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.2, low],
            [0.5, 0.8, low],
            [0.5, 0.8, high],
            [0.5, 0.2, high],
        ]);
        let faces = vec![
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool),
            PlanarFace::new(vec![vec![4, 5, 6, 7]], &pool),
        ];
        (faces, pool)
    }

    fn cut(low: f64, high: f64) -> Vec<(usize, usize)> {
        let (faces, mut pool) = square_and_wall(low, high);
        segments(&faces, 1, &mut pool).0[0].clone()
    }

    #[test]
    fn a_wall_through_a_face_cuts_it_where_both_hold_the_line() {
        // The wall's edges cross the square's plane at (0.5, 0.2) and (0.5, 0.8); the square's
        // own edges cross the wall's plane at y = 0 and y = 1, outside the wall.
        let (faces, mut pool) = square_and_wall(-1.0, 1.0);
        let (segments, cuts, supports) = segments(&faces, 1, &mut pool);
        assert_eq!(segments[0].len(), 1);
        let (u, v) = segments[0][0];
        let ys = [pool.get(u)[1], pool.get(v)[1]];
        assert!(ys.contains(&0.2) && ys.contains(&0.8));
        assert_eq!(cuts[&line(u, v)], [0, 1].into());
        // Both ends, and the square's crossings beyond the wall, lie on the planes' line.
        assert_eq!(supports[&Support::Meet(0, 1)].len(), 4);
    }

    #[test]
    fn a_wall_standing_on_a_face_cuts_it_along_its_foot() {
        // The wall's foot lies on the plane: the stretch along it lies on the wall's boundary and
        // in the square, so it counts whichever side the wall stands on. Hanging below the
        // square, the wall meets it along its top edge.
        assert_eq!(cut(0.0, 1.0), [(4, 5)]);
        assert_eq!(cut(-1.0, 0.0), [(7, 6)]);
    }

    #[test]
    fn a_wall_above_a_face_cuts_nothing() {
        assert!(cut(0.5, 1.0).is_empty());
    }
}
