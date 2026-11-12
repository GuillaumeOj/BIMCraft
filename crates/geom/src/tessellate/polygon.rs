//! A face's rings in its plane as linked chains of nodes, and the exact tests on them that
//! hole bridging and ear clipping share.
//!
//! Positions are compared exactly, never within a tolerance: the only coincident nodes are the
//! copies hole bridging makes of a vertex, and the triangulation must tell them from distinct
//! vertices however close.

use super::search::MAX_SEARCH_NODES;
use super::{bridge, ear, search};
use crate::{MERGE_CHAIN_WIDTH, Orientation, Point2, orient2d};

/// A face the triangulation gave up on. It carries nothing: the caller names the face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Failed;

/// Triangulates a polygon given as the points of its rings one after the other, the outer ring
/// first, counterclockwise, then the holes, clockwise, and where each ring ends. Returns three
/// point indices per triangle, counterclockwise: `n + 2h − 2` triangles for `n` points and `h`
/// holes.
///
/// Exact faces are clipped ear by ear. A face planar and simple only within the tolerance may
/// cross itself so that clipping runs out of ears. A ring of at most [`MAX_SEARCH_NODES`] nodes
/// is then searched for the triangulation with the fewest and smallest slivers turning the
/// wrong way, and fails if it has none; a larger ring is clipped taking slivers where no ear is
/// left, which clips exactly as the strict pass does until then (see [`ear`] and [`search`]).
pub(crate) fn triangulate(points: &[Point2], ends: &[usize]) -> Result<Vec<[usize; 3]>, Failed> {
    let (mut chain, heads) = Chain::new(points, ends)?;
    let start = bridge::bridge_holes(&mut chain, &heads)?;
    if chain.ring(start).count() > MAX_SEARCH_NODES {
        return ear::clip(&mut chain, start, true);
    }
    if let Ok(triangles) = ear::clip(&mut chain.clone(), start, false) {
        return Ok(triangles);
    }
    search::search(&chain, start)?.ok_or(Failed)
}

/// A node of a ring: a point and its neighbours along the ring.
#[derive(Clone, Copy, Debug)]
pub(super) struct Node {
    /// Index of the node's point.
    pub(super) point: usize,
    pub(super) prev: usize,
    pub(super) next: usize,
}

/// Rings of nodes, linked in both directions, over a face's points.
#[derive(Clone)]
pub(super) struct Chain<'a> {
    points: &'a [Point2],
    pub(super) nodes: Vec<Node>,
}

impl<'a> Chain<'a> {
    /// One ring per range of points, and the first node of each. Node `i` is point `i`.
    pub(super) fn new(points: &'a [Point2], ends: &[usize]) -> Result<(Self, Vec<usize>), Failed> {
        let mut nodes = Vec::with_capacity(points.len() + 2 * ends.len());
        let mut heads = Vec::with_capacity(ends.len());
        for &end in ends {
            let start = nodes.len();
            if end < start + 3 || end > points.len() {
                return Err(Failed);
            }
            for point in start..end {
                nodes.push(Node {
                    point,
                    prev: if point == start { end - 1 } else { point - 1 },
                    next: if point + 1 == end { start } else { point + 1 },
                });
            }
            heads.push(start);
        }
        Ok((Self { points, nodes }, heads))
    }

    pub(super) fn point(&self, node: usize) -> Point2 {
        self.points[self.nodes[node].point]
    }

    /// The nodes of the ring through `start`, from it, in order.
    pub(super) fn ring(&self, start: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(Some(start), move |&node| {
            Some(self.nodes[node].next).filter(|&next| next != start)
        })
    }

    /// Adds a node for `point` and returns it; the caller links it.
    pub(super) fn push(&mut self, point: usize) -> usize {
        self.nodes.push(Node {
            point,
            prev: 0,
            next: 0,
        });
        self.nodes.len() - 1
    }

    /// Whether `b` lies strictly inside the angle the ring makes at `node`, on its inner side
    /// (the left, rings turning counterclockwise around what they bound).
    pub(super) fn in_cone(&self, node: usize, b: Point2) -> Result<bool, Failed> {
        let Node { prev, next, .. } = self.nodes[node];
        let (a, before, after) = (self.point(node), self.point(prev), self.point(next));
        if orient(a, after, before)? == Orientation::Negative {
            // A reflex angle: inside unless in the convex angle outside it.
            Ok(!(orient(a, b, after)? != Orientation::Negative
                && orient(b, a, before)? != Orientation::Negative))
        } else {
            Ok(orient(a, b, before)? == Orientation::Positive
                && orient(b, a, after)? == Orientation::Positive)
        }
    }
}

/// [`orient2d`], whose refusals only bad input can cause, as a [`Failed`] triangulation.
pub(super) fn orient(a: Point2, b: Point2, c: Point2) -> Result<Orientation, Failed> {
    orient2d(a, b, c).map_err(|_| Failed)
}

/// Whether two points are the same, exactly.
pub(super) fn same(a: Point2, b: Point2) -> bool {
    a == b
}

/// Whether `p` lies on the closed segment `a b`.
pub(super) fn on_segment(a: Point2, b: Point2, p: Point2) -> Result<bool, Failed> {
    Ok(within(a, b, p) && orient(a, b, p)? == Orientation::Zero)
}

/// Whether the closed segments `a b` and `c d` share a point.
pub(super) fn segments_meet(a: Point2, b: Point2, c: Point2, d: Point2) -> Result<bool, Failed> {
    use Orientation::{Negative, Positive, Zero};
    let opposite = |s: Orientation, t: Orientation| {
        matches!((s, t), (Positive, Negative) | (Negative, Positive))
    };
    let (o1, o2) = (orient(a, b, c)?, orient(a, b, d)?);
    let (o3, o4) = (orient(c, d, a)?, orient(c, d, b)?);
    Ok((opposite(o1, o2) && opposite(o3, o4))
        || (o1 == Zero && within(a, b, c))
        || (o2 == Zero && within(a, b, d))
        || (o3 == Zero && within(c, d, a))
        || (o4 == Zero && within(c, d, b)))
}

/// Whether `p` lies in the closed box spanned by `a` and `b`; exact, being only comparisons.
pub(super) fn within(a: Point2, b: Point2, p: Point2) -> bool {
    a.x.min(b.x) <= p.x && p.x <= a.x.max(b.x) && a.y.min(b.y) <= p.y && p.y <= a.y.max(b.y)
}

/// Whether a triangle is no wider than [`MERGE_CHAIN_WIDTH`]: twice its area over its longest
/// side, compared squared.
pub(super) fn is_sliver(p: Point2, a: Point2, q: Point2) -> bool {
    let doubled = (a.x - p.x) * (q.y - p.y) - (q.x - p.x) * (a.y - p.y);
    let longest = [(p, a), (a, q), (q, p)]
        .iter()
        .map(|&(s, e)| (e.x - s.x) * (e.x - s.x) + (e.y - s.y) * (e.y - s.y))
        .fold(0.0, f64::max);
    doubled * doubled <= MERGE_CHAIN_WIDTH * MERGE_CHAIN_WIDTH * longest
}
