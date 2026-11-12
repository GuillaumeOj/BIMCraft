//! The fallback for a small ring that ear clipping could not finish: among all its
//! triangulations, the one with the fewest slivers turning the wrong way, then the smallest
//! area of them, by dynamic programming over its sub-chains (as for the minimum-weight
//! triangulation of a polygon).
//!
//! Every triangle chosen turns strictly counterclockwise or is a sliver (see
//! [`is_sliver`]). The triangles of any triangulation of a ring add up, with their signs, to the
//! ring's winding number, so when all turn counterclockwise they cover a simple ring exactly
//! once; slivers are where a ring crossing itself within the tolerance leaves no other choice.

use std::cmp::Ordering;

use super::polygon::{Chain, Failed, is_sliver, orient};
use crate::Orientation;

/// The most nodes a ring may have to be searched: the search takes time cubic in it.
pub(super) const MAX_SEARCH_NODES: usize = 256;

/// What a triangulation costs: its slivers turning the wrong way, then their doubled area.
#[derive(Clone, Copy, Debug)]
struct Cost {
    slivers: usize,
    area: f64,
}

impl Cost {
    const ZERO: Self = Self {
        slivers: 0,
        area: 0.0,
    };

    fn add(self, other: Self) -> Self {
        Self {
            slivers: self.slivers + other.slivers,
            area: self.area + other.area,
        }
    }

    fn cmp(self, other: Self) -> Ordering {
        self.slivers
            .cmp(&other.slivers)
            .then(self.area.total_cmp(&other.area))
    }
}

/// The cheapest triangulation of the ring through `start`, as three point indices per
/// triangle; `None` if it has none. The caller keeps rings to [`MAX_SEARCH_NODES`] nodes.
pub(super) fn search(chain: &Chain<'_>, start: usize) -> Result<Option<Vec<[usize; 3]>>, Failed> {
    let nodes: Vec<usize> = chain.ring(start).collect();
    let count = nodes.len();
    let points: Vec<usize> = nodes.iter().map(|&node| chain.nodes[node].point).collect();
    let at = |i: usize, j: usize| i * count + j;
    // best[at(i, j)]: the cheapest triangulation of the sub-chain from node i to node j closed
    // by the segment j i, and the node its triangle on that segment takes.
    let mut best: Vec<Option<(Cost, usize)>> = vec![None; count * count];
    for i in 0..count - 1 {
        best[at(i, i + 1)] = Some((Cost::ZERO, i));
    }
    for span in 2..count {
        for i in 0..count - span {
            let j = i + span;
            for k in i + 1..j {
                let (Some((left, _)), Some((right, _))) = (best[at(i, k)], best[at(k, j)]) else {
                    continue;
                };
                let Some(own) = triangle_cost(chain, [nodes[i], nodes[k], nodes[j]])? else {
                    continue;
                };
                let total = left.add(right).add(own);
                if best[at(i, j)].is_none_or(|(cost, _)| total.cmp(cost) == Ordering::Less) {
                    best[at(i, j)] = Some((total, k));
                }
            }
        }
    }
    if best[at(0, count - 1)].is_none() {
        return Ok(None);
    }
    let mut triangles = Vec::with_capacity(count - 2);
    let mut pending = vec![(0, count - 1)];
    while let Some((i, j)) = pending.pop() {
        if j - i < 2 {
            continue;
        }
        // Every sub-chain of a triangulation found has one too.
        let (_, k) = best[at(i, j)].ok_or(Failed)?;
        triangles.push([points[i], points[k], points[j]]);
        pending.extend([(i, k), (k, j)]);
    }
    Ok(Some(triangles))
}

/// What one triangle of nodes costs: nothing if it turns strictly counterclockwise, its doubled
/// area if it is a sliver of distinct points, and `None`, not allowed, otherwise.
fn triangle_cost(chain: &Chain<'_>, corners: [usize; 3]) -> Result<Option<Cost>, Failed> {
    let [p, a, q] = corners.map(|node| chain.point(node));
    if orient(p, a, q)? == Orientation::Positive {
        return Ok(Some(Cost::ZERO));
    }
    let [first, second, third] = corners.map(|node| chain.nodes[node].point);
    if first == second || second == third || first == third || !is_sliver(p, a, q) {
        return Ok(None);
    }
    let doubled = (a.x - p.x) * (q.y - p.y) - (q.x - p.x) * (a.y - p.y);
    Ok(Some(Cost {
        slivers: 1,
        area: doubled.abs(),
    }))
}
