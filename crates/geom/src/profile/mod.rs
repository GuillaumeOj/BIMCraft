//! Closed 2D profiles of lines and circular arcs, with holes (GEO-01).

mod edge;
mod mass;
mod validate;

use core::fmt;

use edge::Curve;
pub use edge::{ArcDirection, Edge};
use mass::{Moments, loop_moments};

use crate::Point2;

/// Largest number of edges a profile may have, holes included.
///
/// Validation compares every pair of edges; the bound keeps hostile input from making it run
/// for minutes.
pub const MAX_PROFILE_EDGES: usize = 4096;

/// Identifies a loop of a profile in errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LoopId {
    /// The outer boundary.
    Outer,
    /// A hole, by its position in the input (from 0).
    Hole(usize),
}

impl fmt::Display for LoopId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Outer => f.write_str("the outer loop"),
            Self::Hole(index) => write!(f, "hole {index}"),
        }
    }
}

/// Why a profile was rejected. Edges are identified by their position in the input loop (from
/// 0); coordinates are model content and are not carried (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ProfileError {
    /// The profile has more than [`MAX_PROFILE_EDGES`] edges.
    #[error("the profile has more than {MAX_PROFILE_EDGES} edges")]
    TooManyEdges,
    /// A loop has no edge.
    #[error("{0} has no edges")]
    EmptyLoop(LoopId),
    /// A coordinate is NaN or infinite.
    #[error("edge {edge} of {loop_id} has a coordinate that is not a finite number")]
    NonFinite {
        /// The loop.
        loop_id: LoopId,
        /// The edge.
        edge: usize,
    },
    /// A coordinate is outside the model extent, or so close to zero that exact predicates
    /// cannot handle it.
    #[error("edge {edge} of {loop_id} has a coordinate outside the model extent")]
    OutOfRange {
        /// The loop.
        loop_id: LoopId,
        /// The edge.
        edge: usize,
    },
    /// An edge does not end exactly where the next one starts.
    #[error("{loop_id} is open: edge {edge} does not end where the next edge starts")]
    Open {
        /// The loop.
        loop_id: LoopId,
        /// The edge whose end is not the next edge's start.
        edge: usize,
    },
    /// A line starts and ends at the same point.
    #[error("edge {edge} of {loop_id} is a line of zero length")]
    ZeroLengthLine {
        /// The loop.
        loop_id: LoopId,
        /// The edge.
        edge: usize,
    },
    /// An arc's start and end are not at the same distance from its centre.
    #[error("edge {edge} of {loop_id} is an arc whose ends are not on the same circle")]
    ArcRadiusMismatch {
        /// The loop.
        loop_id: LoopId,
        /// The edge.
        edge: usize,
    },
    /// An arc has a radius below the tolerance, or distinct ends at the same angle.
    #[error("edge {edge} of {loop_id} is a degenerate arc")]
    DegenerateArc {
        /// The loop.
        loop_id: LoopId,
        /// The edge.
        edge: usize,
    },
    /// Two edges of a loop cross, touch or overlap, or come closer than the tolerance when one
    /// of them is an arc.
    #[error("edges {first} and {second} of {loop_id} intersect")]
    SelfIntersecting {
        /// The loop.
        loop_id: LoopId,
        /// The first edge.
        first: usize,
        /// The second edge.
        second: usize,
    },
    /// Two loops cross, touch, or come closer than the tolerance where arcs are involved.
    #[error("{first} and {second} intersect")]
    LoopsIntersect {
        /// The first loop.
        first: LoopId,
        /// The second loop.
        second: LoopId,
    },
    /// A hole lies outside the outer loop.
    #[error("hole {0} is outside the outer loop")]
    HoleOutsideOuter(usize),
    /// A hole lies inside another hole.
    #[error("hole {inner} is inside hole {outer}")]
    NestedHoles {
        /// The enclosing hole.
        outer: usize,
        /// The enclosed hole.
        inner: usize,
    },
}

/// A valid closed profile: an outer loop and holes, each made of lines and circular arcs.
///
/// Loops are closed (each edge ends exactly where the next one starts), do not intersect
/// themselves or each other, and every hole lies inside the outer loop and outside the other
/// holes. The outer loop turns counterclockwise and holes clockwise, whatever the input order.
#[derive(Clone, Debug, PartialEq)]
pub struct Profile {
    outer: Vec<Edge>,
    holes: Vec<Vec<Edge>>,
    origin: Point2,
    moments: Moments,
}

impl Profile {
    /// Validates the loops and builds the profile, reversing loops that turn the wrong way.
    ///
    /// # Errors
    ///
    /// [`ProfileError`] when a loop is empty, open, has an out-of-range coordinate or a degenerate
    /// edge, when edges intersect, or when holes are not inside the outer loop and apart from
    /// each other.
    pub fn new(outer: Vec<Edge>, holes: Vec<Vec<Edge>>) -> Result<Self, ProfileError> {
        let edge_count = outer.len() + holes.iter().map(Vec::len).sum::<usize>();
        if edge_count > MAX_PROFILE_EDGES {
            return Err(ProfileError::TooManyEdges);
        }
        let loops: Vec<(LoopId, &[Edge])> = core::iter::once((LoopId::Outer, outer.as_slice()))
            .chain(
                holes
                    .iter()
                    .enumerate()
                    .map(|(i, hole)| (LoopId::Hole(i), hole.as_slice())),
            )
            .collect();
        for &(id, edges) in &loops {
            validate::check_loop(id, edges)?;
        }
        let curves: Vec<Vec<Curve>> = loops
            .iter()
            .map(|(_, edges)| edges.iter().map(Edge::curve).collect())
            .collect();
        for (&(id, edges), curves) in loops.iter().zip(&curves) {
            validate::check_simple(id, edges, curves)?;
        }
        for (i, (&(first, _), a)) in loops.iter().zip(&curves).enumerate() {
            for (&(second, _), b) in loops.iter().zip(&curves).skip(i + 1) {
                validate::check_apart((first, a), (second, b))?;
            }
        }
        check_nesting(&outer, &holes)?;
        Ok(Self::oriented(outer, holes))
    }

    /// The outer loop, counterclockwise.
    #[must_use]
    pub fn outer(&self) -> &[Edge] {
        &self.outer
    }

    /// The holes, each clockwise, in input order.
    pub fn holes(&self) -> impl ExactSizeIterator<Item = &[Edge]> {
        self.holes.iter().map(Vec::as_slice)
    }

    /// Area enclosed by the outer loop minus the holes, in square metres; always positive.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.moments.area
    }

    /// Centroid (centre of area) of the profile.
    #[must_use]
    pub fn centroid(&self) -> Point2 {
        Point2::new(
            self.origin.x + self.moments.x / self.moments.area,
            self.origin.y + self.moments.y / self.moments.area,
        )
    }

    /// Reverses loops so the outer one turns counterclockwise and holes clockwise, and sums the
    /// moments. Coordinates are taken from the first vertex, which keeps them small.
    fn oriented(outer: Vec<Edge>, holes: Vec<Vec<Edge>>) -> Self {
        let origin = outer[0].start();
        let mut moments = Moments::default();
        let mut orient = |edges: Vec<Edge>, counterclockwise: bool| {
            let m = loop_moments(&edges, origin);
            if (m.area > 0.0) == counterclockwise {
                moments = moments.add(m);
                edges
            } else {
                // Recomputed rather than negated, so the result depends only on the stored loop.
                let edges = reversed(&edges);
                moments = moments.add(loop_moments(&edges, origin));
                edges
            }
        };
        let outer = orient(outer, true);
        let holes = holes.into_iter().map(|hole| orient(hole, false)).collect();
        Self {
            outer,
            holes,
            origin,
            moments,
        }
    }
}

fn reversed(edges: &[Edge]) -> Vec<Edge> {
    edges.iter().rev().map(Edge::reversed).collect()
}

/// Every hole inside the outer loop and outside the other holes. Loops are already known not to
/// meet, so testing one vertex of each hole decides it.
fn check_nesting(outer: &[Edge], holes: &[Vec<Edge>]) -> Result<(), ProfileError> {
    for (inner, hole) in holes.iter().enumerate() {
        let vertex = hole[0].start();
        if !validate::encloses(outer, vertex) {
            return Err(ProfileError::HoleOutsideOuter(inner));
        }
        if let Some(outer) = holes
            .iter()
            .enumerate()
            .position(|(j, other)| j != inner && validate::encloses(other, vertex))
        {
            return Err(ProfileError::NestedHoles { outer, inner });
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
