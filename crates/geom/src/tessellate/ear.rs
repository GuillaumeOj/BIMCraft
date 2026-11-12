//! Ear clipping of a single ring, bridged holes included (Meisters, "Polygons have ears").
//!
//! An ear is a node whose neighbours it can be cut off from: the triangle they make turns
//! strictly counterclockwise and nothing of the ring enters it. Every simple polygon of four
//! nodes or more has two, so clipping one at a time ends in a triangle. Ear status is decided
//! afresh at each visit, never cached: a clip changes its neighbours' triangles, and, around
//! the copies of a bridged vertex, which edges leave a position.
//!
//! A face is planar and keeps its features apart only within the tolerance, so its rings may
//! cross where they come within a few tolerances of each other, and a ring that crosses itself
//! can run out of ears, or admit no triangulation into counterclockwise triangles at all. A
//! ring too large for the search of [`super::search`] is then clipped relaxed: once no node is
//! an ear, it takes a sliver, a triangle no wider than
//! [`MERGE_CHAIN_WIDTH`](crate::MERGE_CHAIN_WIDTH) whichever way it turns, which no display
//! shows. Wider defects have none, and the face fails.

use super::polygon::{Chain, Failed, Node, is_sliver, orient, same, within};
use crate::{Orientation, Point2};

/// Clips ears from the ring through `start` until a triangle is left, and returns every
/// triangle as three point indices. Only when `relaxed` does it take slivers, and only once no
/// node is an ear.
pub(super) fn clip(
    chain: &mut Chain<'_>,
    start: usize,
    relaxed: bool,
) -> Result<Vec<[usize; 3]>, Failed> {
    let mut remaining = chain.ring(start).count();
    let mut triangles = Vec::with_capacity(remaining.saturating_sub(2));
    let mut node = start;
    // Nodes visited since the last clip: once every node has been, none is an ear.
    let mut misses = 0;
    while remaining > 3 {
        if is_ear(chain, node)? {
            node = cut(chain, node, &mut triangles);
            remaining -= 1;
            misses = 0;
        } else {
            misses += 1;
            node = chain.nodes[node].next;
            if misses >= remaining {
                if !relaxed {
                    return Err(Failed);
                }
                let sliver = find_sliver(chain, node).ok_or(Failed)?;
                node = cut(chain, sliver, &mut triangles);
                remaining -= 1;
                misses = 0;
            }
        }
    }
    let Node { prev, next, .. } = chain.nodes[node];
    let (p, a, q) = (chain.point(prev), chain.point(node), chain.point(next));
    if orient(p, a, q)? != Orientation::Positive && !(relaxed && is_distinct_sliver(chain, node)) {
        return Err(Failed);
    }
    cut(chain, node, &mut triangles);
    Ok(triangles)
}

/// Cuts the triangle of `node` and its neighbours off the ring, and returns the next node.
fn cut(chain: &mut Chain<'_>, node: usize, triangles: &mut Vec<[usize; 3]>) -> usize {
    let Node { point, prev, next } = chain.nodes[node];
    triangles.push([chain.nodes[prev].point, point, chain.nodes[next].point]);
    chain.nodes[prev].next = next;
    chain.nodes[next].prev = prev;
    next
}

/// The first node from `start` along the ring whose triangle is a sliver, if any.
fn find_sliver(chain: &Chain<'_>, start: usize) -> Option<usize> {
    chain
        .ring(start)
        .find(|&node| is_distinct_sliver(chain, node))
}

/// Whether the triangle of `node` and its neighbours is a sliver of three distinct points, not
/// two copies of a bridged vertex.
fn is_distinct_sliver(chain: &Chain<'_>, node: usize) -> bool {
    let Node { prev, next, point } = chain.nodes[node];
    let (before, after) = (chain.nodes[prev].point, chain.nodes[next].point);
    point != before
        && point != after
        && before != after
        && is_sliver(chain.point(prev), chain.point(node), chain.point(next))
}

/// Whether the triangle of `node` and its neighbours turns strictly counterclockwise and no
/// other node lies in it, nor any edge from a copy of one of its corners enters it.
pub(super) fn is_ear(chain: &Chain<'_>, node: usize) -> Result<bool, Failed> {
    let Node { prev, next, .. } = chain.nodes[node];
    let (p, a, q) = (chain.point(prev), chain.point(node), chain.point(next));
    if orient(p, a, q)? != Orientation::Positive {
        return Ok(false);
    }
    // Each corner with the corners after and before it, counterclockwise.
    let corners = [(p, a, q), (a, q, p), (q, p, a)];
    let (low, high) = (
        Point2::new(p.x.min(a.x).min(q.x), p.y.min(a.y).min(q.y)),
        Point2::new(p.x.max(a.x).max(q.x), p.y.max(a.y).max(q.y)),
    );
    for other in chain.ring(next).skip(1) {
        if other == prev {
            break;
        }
        let v = chain.point(other);
        if !within(low, high, v) {
            continue;
        }
        if let Some(&(corner, after, before)) = corners.iter().find(|(c, ..)| same(*c, v)) {
            let Node {
                prev: w0, next: w1, ..
            } = chain.nodes[other];
            for w in [chain.point(w0), chain.point(w1)] {
                if enters(corner, after, before, w)? {
                    return Ok(false);
                }
            }
        } else if inside_closed(p, a, q, v)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether the edge from `corner` towards `w` starts into the triangle: strictly between the
/// sides towards the corners after and before it.
fn enters(corner: Point2, after: Point2, before: Point2, w: Point2) -> Result<bool, Failed> {
    Ok(orient(corner, after, w)? == Orientation::Positive
        && orient(corner, before, w)? == Orientation::Negative)
}

/// Whether `v` lies in the closed counterclockwise triangle `p a q`.
fn inside_closed(p: Point2, a: Point2, q: Point2, v: Point2) -> Result<bool, Failed> {
    Ok(orient(p, a, v)? != Orientation::Negative
        && orient(a, q, v)? != Orientation::Negative
        && orient(q, p, v)? != Orientation::Negative)
}
