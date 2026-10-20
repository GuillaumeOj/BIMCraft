//! Splitting faces along the segments the other operand left on them.
//!
//! Every line, an edge of a face or a segment, is noded once for all the faces along it: split at
//! the vertices its generators put on it, such as where it crosses a plane of the other operand,
//! then where its pieces cross the pieces of another line of a face it lies in (decided with the
//! exact `orient2d`). Two faces sharing a line then split it the same way, so their pieces meet
//! edge to edge without a later pass splitting edges to make them agree. Each face's lines, so
//! split, are planar, and following them gives the regions the face is cut into. Which regions
//! lie inside the face is read from its boundary, as split.

use std::collections::{BTreeMap, BTreeSet};

use super::classify::spread;
use super::face::{PlanarFace, Pool, edge_planes};
use super::intersect::{Segments, Support, Supports};
use super::planar::{OUTSIDE, Regions, chain, regions};
use crate::predicates::orient2d;
use crate::vector::{add, distance_squared, dot, edges, scale, sub};
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
        for (&l, along) in &self.along {
            for pair in chain_of(l, along).windows(2) {
                holding.entry(line(pair[0], pair[1])).or_default().push(l);
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

/// Nodes the lines of all faces once. Each is first split at every vertex of the arrangement
/// lying on one of its supports: the edge of an operand it runs along, or, for a segment cut on a
/// face, the line where two planes meet that holds both its ends. Whether a vertex lies on a line
/// is never judged by its distance from it. An edge is split only at the points made on it: the
/// planes of two faces at a small angle meet along a line whose points, each within the
/// tolerance of both planes, may lie several tolerances apart across it.
///
/// The pieces so split are then split where they cross the pieces of other lines of a face they
/// lie in, decided exactly on the pieces themselves: a vertex a line is split at lies on it up to
/// rounding only, and a crossing decided on the straight line need not hold for the line bent
/// there. A crossing lies on the planes of both lines, and is the vertex already made on three of
/// them, if there is one (ADR 0001, constructed geometry).
pub(super) fn node(
    faces: &[PlanarFace],
    segments: &Segments,
    supports: &Supports,
    pool: &mut Pool,
) -> Chains {
    let lines: Vec<Vec<Line>> = faces
        .iter()
        .zip(segments)
        .map(|(face, segments)| lines_of(face, segments))
        .collect();
    let edge_planes = edge_planes(faces);
    let planes_of = |support: &Support| match *support {
        Support::Edge(a, b) => edge_planes.get(&(a, b)).cloned().unwrap_or_default(),
        Support::Meet(f, g) => BTreeSet::from([faces[f].plane, faces[g].plane]),
    };
    let mut on: BTreeMap<usize, BTreeSet<Support>> = BTreeMap::new();
    for (&support, vertices) in supports {
        for &v in vertices {
            on.entry(v).or_default().insert(support);
        }
    }
    let held_by: BTreeMap<Line, Vec<Support>> = (lines.iter().flatten())
        .map(|&l| (l, held(l, &on)))
        .collect();
    // The planes each line lies on: those of the faces along the edge it runs along, or the two
    // planes whose meeting line it is.
    let planes: BTreeMap<Line, BTreeSet<usize>> = (held_by.iter())
        .map(|(&l, held)| (l, held.iter().flat_map(planes_of).collect()))
        .collect();
    // Only vertices some line ends at split others: a crossing no cut reaches marks nothing.
    let ends: BTreeSet<usize> = lines.iter().flatten().flat_map(|&(u, v)| [u, v]).collect();
    let mut along: BTreeMap<Line, Vec<usize>> = (held_by.iter())
        .map(|(&l, held)| {
            let made = held.iter().filter_map(|support| supports.get(support));
            let split = made
                .flatten()
                .copied()
                .filter(|w| ends.contains(w))
                .collect();
            (l, ordered(l, split, pool))
        })
        .collect();
    // Crossings lie on the supports of both lines, so lines along them elsewhere split there too.
    let mut crossed: BTreeMap<Support, BTreeSet<usize>> = BTreeMap::new();
    for (face, lines) in faces.iter().zip(&lines) {
        for (l, vertex) in cross_pieces(face, lines, &planes, &mut along, pool) {
            for &support in &held_by[&l] {
                crossed.entry(support).or_default().insert(vertex);
            }
        }
    }
    for (l, split) in &mut along {
        let elsewhere: Vec<usize> = (held_by[l].iter())
            .filter_map(|support| crossed.get(support))
            .flatten()
            .copied()
            .collect();
        if !elsewhere.is_empty() {
            split.extend(elsewhere);
            *split = ordered(*l, std::mem::take(split), pool);
        }
    }
    Chains { lines, along }
}

/// Splits the pieces of a face's lines where they cross, deciding it in the face's own
/// projection so that its lines stay planar: faces a tolerance apart may project differently.
/// Lines crossing in several faces, such as edges of coplanar faces, share one vertex: later
/// faces find them split. A crossing that is a vertex already made, such as an end of one of the
/// pieces, splits the other there, which may bend it, so the bent pieces are crossed again until
/// none is split. Each pass splits at least
/// one piece, so as many passes as the face has lines are a bound rounding cannot push past.
/// Returns each line split with the vertex splitting it.
fn cross_pieces(
    face: &PlanarFace,
    lines: &[Line],
    planes: &BTreeMap<Line, BTreeSet<usize>>,
    along: &mut BTreeMap<Line, Vec<usize>>,
    pool: &mut Pool,
) -> Vec<(Line, usize)> {
    let at = |pool: &Pool, v: usize| face.projection.apply(pool.get(v));
    let mut all = Vec::new();
    // Pieces left whole by a pass were all tested against each other already.
    let mut tested: BTreeSet<Line> = BTreeSet::new();
    for _ in 0..=lines.len() {
        let mut owners: BTreeMap<Line, Vec<Line>> = BTreeMap::new();
        for &l in lines {
            for pair in chain_of(l, &along[&l]).windows(2) {
                owners.entry((pair[0], pair[1])).or_default().push(l);
            }
        }
        let pieces: Vec<Line> = owners.keys().copied().collect();
        let mut found: BTreeMap<Line, Vec<usize>> = BTreeMap::new();
        for (first, second) in pairs_of_boxes(&pieces, |v| at(pool, v)) {
            if tested.contains(&first) && tested.contains(&second) {
                continue;
            }
            let (one, other) = (&owners[&first], &owners[&second]);
            // Pieces of one line meet only at their ends.
            if one.iter().any(|l| other.contains(l)) {
                continue;
            }
            let on = || {
                (one.iter().chain(other))
                    .filter_map(|l| planes.get(l))
                    .flatten()
                    .copied()
                    .collect()
            };
            if let Some(vertex) = crossing(first, second, on, pool, &at) {
                for &l in one.iter().chain(other) {
                    found.entry(l).or_default().push(vertex);
                }
            }
        }
        tested = pieces.into_iter().collect();
        let mut split_any = false;
        for (l, vertices) in found {
            if let Some(split) = along.get_mut(&l) {
                let before = split.len();
                all.extend(vertices.iter().map(|&vertex| (l, vertex)));
                split.extend(vertices);
                *split = ordered(l, std::mem::take(split), pool);
                split_any |= split.len() > before;
            }
        }
        if !split_any {
            break;
        }
    }
    all
}

/// The supports holding both ends of a line; only the edges among them if it runs along an edge
/// of an operand, which is exact where the line where two planes meet need not be.
fn held((u, v): Line, on: &BTreeMap<usize, BTreeSet<Support>>) -> Vec<Support> {
    let (Some(at_u), Some(at_v)) = (on.get(&u), on.get(&v)) else {
        return Vec::new();
    };
    let both: Vec<Support> = at_u.intersection(at_v).copied().collect();
    let is_edge = |support: &Support| matches!(support, Support::Edge(..));
    if both.iter().any(is_edge) {
        both.into_iter().filter(is_edge).collect()
    } else {
        both
    }
}

/// A line's ends with the vertices splitting it in between, in order.
fn chain_of((u, v): Line, split: &[usize]) -> Vec<usize> {
    std::iter::once(u)
        .chain(split.iter().copied())
        .chain(std::iter::once(v))
        .collect()
}

/// The vertices splitting a line, without its ends, repeats or vertices beyond its ends, in
/// order from its first end.
fn ordered(l: Line, split: Vec<usize>, pool: &Pool) -> Vec<usize> {
    let mut chain: Vec<(f64, usize)> = split
        .into_iter()
        .filter(|&w| w != l.0 && w != l.1)
        .filter_map(|w| Some((within(w, l, pool)?, w)))
        .collect();
    // A vertex found twice has the same key both times, so its copies end up together.
    chain.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    chain.dedup_by_key(|&mut (_, w)| w);
    chain.into_iter().map(|(_, w)| w).collect()
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

/// Where a vertex projects onto a line, measured along it from its first end in units of its
/// squared length; `None` unless strictly between its ends.
fn within(vertex: usize, (u, v): Line, pool: &Pool) -> Option<f64> {
    let (origin, span) = (pool.get(u), sub(pool.get(v), pool.get(u)));
    let position = dot(sub(pool.get(vertex), origin), span);
    (position > 0.0 && position < dot(span, span)).then_some(position)
}

/// A face cut into pieces: the regions its lines bound, and which of them lie inside the face.
pub(super) struct Pieces {
    pub(super) regions: Regions,
    pub(super) inside: Vec<bool>,
}

/// Cuts face `index` of the faces given to [`node`] along its lines as they were split there.
/// The regions include those outside the face where segments reach there; which side of the
/// face's boundary each lies on is read from the boundary itself, as split, never from a point
/// inside it: a boundary split at a vertex off its straight line no longer runs along it.
pub(super) fn split(face: &PlanarFace, index: usize, chains: &Chains, pool: &Pool) -> Pieces {
    let mut pieces = BTreeSet::new();
    for &l in &chains.lines[index] {
        for pair in chain_of(l, &chains.along[&l]).windows(2) {
            pieces.insert((pair[0], pair[1]));
            pieces.insert((pair[1], pair[0]));
        }
    }
    let at = |v: usize| face.projection.apply(pool.get(v));
    let regions = regions(chain(&pieces, at), at);
    let inside = inside(face, chains, &regions);
    Pieces { regions, inside }
}

/// Which regions lie inside the face: those on the left of its boundary, followed in the
/// direction of its rings, and those reached from them across the face's cuts. Across its
/// boundary, the regions on the right lie outside. Where the boundary runs along a piece and
/// back, as at a corner so sharp that its two edges meet the same vertices, it bounds nothing,
/// and the regions on both sides lie outside.
fn inside(face: &PlanarFace, chains: &Chains, regions: &Regions) -> Vec<bool> {
    let mut boundary = BTreeSet::new();
    for (a, b) in face.rings.iter().flat_map(|ring| edges(ring)) {
        let (u, v) = line(a, b);
        let mut chain = chain_of((u, v), &chains.along[&(u, v)]);
        if a != u {
            chain.reverse();
        }
        boundary.extend(chain.windows(2).map(|pair| (pair[0], pair[1])));
    }
    let count = regions.regions.len();
    let mut label: Vec<Option<bool>> = vec![None; count];
    let mut links: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (&(u, v), &region) in &regions.left {
        if region == OUTSIDE {
            continue;
        }
        match (boundary.contains(&(u, v)), boundary.contains(&(v, u))) {
            (true, false) => label[region] = Some(true),
            (_, true) => {
                label[region].get_or_insert(false);
            }
            (false, false) => {
                if let Some(&across) = regions.left.get(&(v, u))
                    && across != OUTSIDE
                {
                    links[region].push(across);
                }
            }
        }
    }
    let labelled = (0..count).filter(|&r| label[r].is_some()).collect();
    spread(&links, &mut label, labelled);
    label.into_iter().map(|l| l == Some(true)).collect()
}

/// The vertex where two pieces of lines cross in their interiors, if they do. The decision is
/// exact; the point is computed once, on the given planes of the two lines, and is the vertex
/// already made on three of them if there is one ([`Pool::on_planes`]): an end of either piece,
/// say, which then splits the other.
fn crossing(
    first: Line,
    second: Line,
    planes: impl FnOnce() -> BTreeSet<usize>,
    pool: &mut Pool,
    at: &impl Fn(&Pool, usize) -> Point2,
) -> Option<usize> {
    let ends = [first.0, first.1, second.0, second.1];
    if ends[..2].iter().any(|end| ends[2..].contains(end)) {
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
    Some(pool.on_planes(add(start, scale(sub(end, start), fraction)), &planes()))
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
        let face = PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 0);
        let segments = vec![vec![(0, 2), (1, 3)]];
        let chains = node(
            std::slice::from_ref(&face),
            &segments,
            &Supports::new(),
            &mut pool,
        );
        let pieces = split(&face, 0, &chains, &pool);
        assert_eq!(pieces.regions.regions.len(), 4);
        assert!(distance_squared(pool.get(4), [0.5, 0.5, 0.0]) < 1e-30);
        assert!(
            pieces
                .regions
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
            PlanarFace::new(vec![vec![0, 1, 2]], &pool, 1),
            PlanarFace::new(vec![vec![0, 2, 3]], &pool, 2),
        ];
        // The cut reaches the diagonal where it crosses a plane: vertex 4 lies on that edge.
        let supports = Supports::from([(Support::Edge(0, 2), BTreeSet::from([0, 2, 4]))]);
        let chains = node(
            &faces,
            &vec![vec![(1, 4)], Vec::new()],
            &supports,
            &mut pool,
        );
        let first = split(&faces[0], 0, &chains, &pool);
        let second = split(&faces[1], 1, &chains, &pool);
        assert_eq!(first.regions.regions.len(), 2);
        assert_eq!(second.regions.regions.len(), 1);
        assert_eq!(
            second.regions.regions[0]
                .outer
                .iter()
                .filter(|&&v| v == 4)
                .count(),
            1
        );
    }

    #[test]
    fn a_crossing_splits_every_line_along_the_same_support() {
        // The unit square's diagonals cross at its centre. Another face's edge from corner 0
        // to (2, 2) runs along the first diagonal, both on the line where two planes meet: it
        // is split at the far corner and at the crossing found in the square.
        let mut pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 0.0, 1.0],
        ]);
        let faces = [
            PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 3),
            PlanarFace::new(vec![vec![0, 4, 5]], &pool, 4),
        ];
        let supports = Supports::from([(Support::Meet(0, 1), BTreeSet::from([0, 2, 4]))]);
        let segments = vec![vec![(0, 2), (1, 3)], Vec::new()];
        let chains = node(&faces, &segments, &supports, &mut pool);
        assert_eq!(chains.along[&(0, 4)], [6, 2]);
        assert!(distance_squared(pool.get(6), [0.5, 0.5, 0.0]) < 1e-30);
    }

    #[test]
    fn an_edge_is_split_only_at_the_points_made_on_it() {
        // Both ends of the edge also lie on a line where two planes meet: only the points made
        // on the edge split it. A cut is split at the points on its planes' line; a line with an
        // end on neither is held by nothing.
        let on = BTreeMap::from([
            (
                0,
                BTreeSet::from([Support::Edge(0, 1), Support::Meet(3, 4)]),
            ),
            (
                1,
                BTreeSet::from([Support::Edge(0, 1), Support::Meet(3, 4)]),
            ),
        ]);
        assert_eq!(held((0, 1), &on), [Support::Edge(0, 1)]);
        let cut = BTreeMap::from([
            (0, BTreeSet::from([Support::Meet(3, 4)])),
            (1, BTreeSet::from([Support::Meet(3, 4)])),
        ]);
        assert_eq!(held((0, 1), &cut), [Support::Meet(3, 4)]);
        assert!(held((0, 2), &cut).is_empty());
    }

    #[test]
    fn a_boundary_running_out_and_back_bounds_nothing() {
        // A square whose last edge, from 3 back to 0, is split at vertex 4 on its first edge,
        // as when a corner is so sharp that both its edges meet the same vertex: the boundary
        // runs from 0 to 4 and back. The square beyond 4 is inside; the triangle a cut closes
        // against the spike is not.
        let pool = Pool::of(vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.5, -0.5, 0.0],
        ]);
        let face = PlanarFace::new(vec![vec![0, 1, 2, 3]], &pool, 5);
        let chains = Chains {
            lines: vec![vec![(0, 1), (1, 2), (2, 3), (0, 3), (0, 5), (4, 5)]],
            along: BTreeMap::from([
                ((0, 1), vec![4]),
                ((1, 2), Vec::new()),
                ((2, 3), Vec::new()),
                ((0, 3), vec![4]),
                ((0, 5), Vec::new()),
                ((4, 5), Vec::new()),
            ]),
        };
        let pieces = split(&face, 0, &chains, &pool);
        let inside: Vec<&Vec<usize>> = (pieces.regions.regions.iter())
            .zip(&pieces.inside)
            .filter(|&(_, &inside)| inside)
            .map(|(region, _)| &region.outer)
            .collect();
        assert_eq!(inside.len(), 1);
        assert!(!inside[0].contains(&0));
    }
}
