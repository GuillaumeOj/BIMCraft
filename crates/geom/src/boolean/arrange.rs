//! Splitting a face along the segments the other operand left on it.
//!
//! The face's boundary and the segments are noded: edges that cross are split at a new vertex
//! (crossings decided with the exact `orient2d`), and edges passing within the tolerance of a
//! vertex are split there. The result is planar, so following its edges gives the regions the
//! face is cut into.

use std::collections::BTreeSet;

use super::face::{PlanarFace, Pool, segment_distance_squared, sorted_by_x, vertices_on_edge};
use super::planar::{Regions, chain, regions};
use crate::predicates::orient2d;
use crate::tolerance::WITHIN_TOLERANCE_SQUARED;
use crate::vector::{add, distance_squared, dot, edges, scale, sub};
use crate::{Orientation, Point2};

/// Cuts a face along segments; the regions include those outside the face, if segments reach
/// there.
pub(super) fn split(face: &PlanarFace, segments: &[(usize, usize)], pool: &mut Pool) -> Regions {
    let mut lines: BTreeSet<(usize, usize)> = BTreeSet::new();
    for (u, v) in face
        .rings
        .iter()
        .flat_map(|ring| edges(ring))
        .chain(segments.iter().copied())
    {
        if u != v {
            lines.insert((u.min(v), u.max(v)));
        }
    }
    let lines: Vec<(usize, usize)> = lines.into_iter().collect();
    let mut cuts: Vec<Vec<usize>> = vec![Vec::new(); lines.len()];
    let mut vertices: BTreeSet<usize> = lines.iter().flat_map(|&(u, v)| [u, v]).collect();
    let at = |pool: &Pool, v: usize| face.projection.apply(pool.get(v));
    // Sort and sweep along x, so only lines whose boxes overlap are compared.
    let boxes: Vec<[f64; 4]> = lines
        .iter()
        .map(|&(u, v)| {
            let (p, q) = (at(pool, u), at(pool, v));
            [p.x.min(q.x), p.x.max(q.x), p.y.min(q.y), p.y.max(q.y)]
        })
        .collect();
    let mut order: Vec<usize> = (0..lines.len()).collect();
    order.sort_by(|&i, &j| boxes[i][0].total_cmp(&boxes[j][0]).then(i.cmp(&j)));
    for (k, &i) in order.iter().enumerate() {
        for &j in order[k + 1..]
            .iter()
            .take_while(|&&j| boxes[j][0] <= boxes[i][1])
        {
            if boxes[j][2] > boxes[i][3] || boxes[i][2] > boxes[j][3] {
                continue;
            }
            if let Some(vertex) = crossing(lines[i], lines[j], &vertices, pool, &at) {
                cuts[i].push(vertex);
                cuts[j].push(vertex);
                vertices.insert(vertex);
            }
        }
    }
    let by_x = sorted_by_x(pool, vertices.iter().copied());
    let mut pieces = BTreeSet::new();
    for (&(u, v), cuts) in lines.iter().zip(&mut cuts) {
        let (p, d) = (pool.get(u), sub(pool.get(v), pool.get(u)));
        let along = |w: usize| dot(sub(pool.get(w), p), d);
        cuts.extend(vertices_on_edge(pool, &by_x, (u, v)));
        cuts.sort_by(|&w, &x| along(w).total_cmp(&along(x)).then(w.cmp(&x)));
        cuts.dedup();
        let stops: Vec<usize> = std::iter::once(u)
            .chain(cuts.iter().copied())
            .chain(std::iter::once(v))
            .collect();
        for pair in stops.windows(2) {
            if pair[0] != pair[1] {
                pieces.insert((pair[0], pair[1]));
                pieces.insert((pair[1], pair[0]));
            }
        }
    }
    let pool = &*pool;
    regions(chain(&pieces, |v| at(pool, v)), |v| at(pool, v))
}

/// The vertex where two edges cross in their interiors, if they do. The decision is exact; the
/// point is computed once. A crossing within the tolerance of an existing vertex is left to the
/// splitting of edges at vertices instead, which puts that vertex on both edges.
fn crossing(
    first: (usize, usize),
    second: (usize, usize),
    vertices: &BTreeSet<usize>,
    pool: &mut Pool,
    at: &impl Fn(&Pool, usize) -> Point2,
) -> Option<usize> {
    let ends = [first.0, first.1, second.0, second.1];
    if first.0 == second.0 || first.0 == second.1 || first.1 == second.0 || first.1 == second.1 {
        return None;
    }
    // Edges that touch or overlap within the tolerance are split at each other's ends instead:
    // nearly collinear edges would otherwise cross at an arbitrary point along their overlap.
    let near = |vertex: usize, (start, end): (usize, usize)| {
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
    let taken = vertices
        .iter()
        .any(|&vertex| distance_squared(point, pool.get(vertex)) <= WITHIN_TOLERANCE_SQUARED);
    (!taken).then(|| pool.push(point))
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
        let pieces = split(&face, &[(0, 2), (1, 3)], &mut pool);
        assert_eq!(pieces.regions.len(), 4);
        assert!(distance_squared(pool.get(4), [0.5, 0.5, 0.0]) < 1e-30);
        assert!(
            pieces
                .regions
                .iter()
                .all(|region| region.outer.contains(&4))
        );
    }
}
