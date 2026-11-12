//! Joins each hole of a face to its outer ring by a bridge: a segment between a hole vertex and
//! a vertex of the outer ring, walked once each way, which turns the face into a single ring.
//!
//! Holes are taken from the one reaching farthest along the first axis: its farthest vertex `m`
//! then sees a vertex of the ring already merged (Eberly, "Triangulation by ear clipping").
//! Candidates are tried nearest first, which only orders the search; a bridge is accepted only
//! if it lies inside the angles at both its ends and meets no edge of any ring but at its ends,
//! all decided exactly.

use std::cmp::Ordering;

use super::polygon::{Chain, Failed, on_segment, same, segments_meet};
use crate::Point2;

/// Bridges every hole into the outer ring, whose first node is `heads[0]`, and returns a node
/// of the single ring left.
pub(super) fn bridge_holes(chain: &mut Chain<'_>, heads: &[usize]) -> Result<usize, Failed> {
    let (&outer, holes) = heads.split_first().ok_or(Failed)?;
    let mut holes: Vec<usize> = holes.iter().map(|&head| farthest(chain, head)).collect();
    holes.sort_by(|&a, &b| lexicographic(chain.point(b), chain.point(a)).then(a.cmp(&b)));
    for (index, &m) in holes.iter().enumerate() {
        let p = find_bridge(chain, outer, m, &holes[index + 1..])?;
        splice(chain, p, m);
    }
    Ok(outer)
}

/// The node of a ring farthest along the first axis, then the second.
fn farthest(chain: &Chain<'_>, head: usize) -> usize {
    chain
        .ring(head)
        .reduce(|best, node| {
            if lexicographic(chain.point(node), chain.point(best)) == Ordering::Greater {
                node
            } else {
                best
            }
        })
        .unwrap_or(head)
}

fn lexicographic(a: Point2, b: Point2) -> Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

/// The node of the merged ring through `outer` that hole node `m` is bridged to.
fn find_bridge(
    chain: &Chain<'_>,
    outer: usize,
    m: usize,
    others: &[usize],
) -> Result<usize, Failed> {
    let target = chain.point(m);
    let mut candidates: Vec<(f64, usize)> = chain
        .ring(outer)
        .map(|node| {
            let p = chain.point(node);
            let (dx, dy) = (p.x - target.x, p.y - target.y);
            (dx * dx + dy * dy, node)
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, p) in candidates {
        if is_bridge(chain, [outer, m].iter().chain(others), m, p)? {
            return Ok(p);
        }
    }
    Err(Failed)
}

/// Whether the segment from hole node `m` to node `p` can be a bridge: inside the angles of the
/// rings at both ends, and meeting no edge of the rings through `rings` but at its own ends.
fn is_bridge<'r>(
    chain: &Chain<'_>,
    rings: impl Iterator<Item = &'r usize>,
    m: usize,
    p: usize,
) -> Result<bool, Failed> {
    let (pm, pp) = (chain.point(m), chain.point(p));
    if same(pm, pp) || !chain.in_cone(p, pm)? || !chain.in_cone(m, pp)? {
        return Ok(false);
    }
    for &head in rings {
        for node in chain.ring(head) {
            let (x, y) = (chain.point(node), chain.point(chain.nodes[node].next));
            if blocks(pm, pp, x, y)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Whether edge `x y` meets the bridge `m p` anywhere but at a shared end.
fn blocks(m: Point2, p: Point2, x: Point2, y: Point2) -> Result<bool, Failed> {
    let end = |q: Point2| same(q, m) || same(q, p);
    match (end(x), end(y)) {
        // The bridge itself, already an edge.
        (true, true) => Ok(true),
        // Sharing an end, they meet elsewhere only if they overlap.
        (true, false) => on_segment(m, p, y),
        (false, true) => on_segment(m, p, x),
        (false, false) => segments_meet(m, p, x, y),
    }
}

/// Links `p` to `m`, runs around the hole back to a copy of `m`, then back to a copy of `p` and
/// on along the outer ring.
fn splice(chain: &mut Chain<'_>, p: usize, m: usize) {
    let (after_p, before_m) = (chain.nodes[p].next, chain.nodes[m].prev);
    let m_copy = chain.push(chain.nodes[m].point);
    let p_copy = chain.push(chain.nodes[p].point);
    chain.nodes[p].next = m;
    chain.nodes[m].prev = p;
    chain.nodes[before_m].next = m_copy;
    chain.nodes[m_copy].prev = before_m;
    chain.nodes[m_copy].next = p_copy;
    chain.nodes[p_copy].prev = m_copy;
    chain.nodes[p_copy].next = after_p;
    chain.nodes[after_p].prev = p_copy;
}

#[cfg(test)]
mod tests {
    use super::blocks;
    use crate::Point2;

    #[test]
    fn an_edge_along_the_bridge_blocks_it() {
        let (m, p) = (Point2::new(0.0, 0.0), Point2::new(2.0, 0.0));
        // The same segment either way, an overlap from a shared end, and a mere touch there.
        assert_eq!(blocks(m, p, p, m), Ok(true));
        assert_eq!(blocks(m, p, m, Point2::new(1.0, 0.0)), Ok(true));
        assert_eq!(blocks(m, p, Point2::new(1.0, 0.0), p), Ok(true));
        assert_eq!(blocks(m, p, p, Point2::new(3.0, 1.0)), Ok(false));
    }
}
