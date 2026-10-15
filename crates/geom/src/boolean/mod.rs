//! Booleans of solids bounded by planar faces (GEO-03), following the tolerance policy of
//! `docs/dev/adr/0001-geometry-tolerance-policy.md`.
//!
//! Every operation runs the same steps:
//!
//! 1. Each face of one operand is intersected with the faces of the other whose boxes overlap
//!    it. Where an edge crosses a plane, the vertex is computed once and rounded once, and is
//!    shared by every face along the edge. Coplanar faces exchange their boundaries.
//! 2. Vertices closer than the tolerance merge (merge pass, step 2).
//! 3. Each face is cut along those segments into pieces. Whether a cut crosses another is
//!    decided exactly; every piece keeps the plane of its input face.
//! 4. Each piece is classified against the other operand: inside, outside, or on one of its
//!    faces, facing the same way or the opposite way. The operation keeps some classes and drops
//!    the others; kept pieces of the same face merge back into as few faces as possible.
//! 5. The kept faces are stitched into a solid: edges split where other faces' vertices lie on
//!    them, lumps that only touch along an edge or at a point get their own vertices, adjacent
//!    coplanar faces merge, and vertices between collinear edges go (merge pass, steps 1 to 4).
//!    The result is checked like any solid; an invalid one is an error, never a panic.
//!
//! The result is regularised: pieces of zero thickness, such as a face two operands share, are
//! dropped.

mod arrange;
mod assemble;
mod classify;
mod error;
mod face;
mod intersect;
mod planar;
mod snap;

pub use error::BooleanError;

use self::classify::{Class, classify};
use self::face::{PlanarFace, Pool};
use self::planar::{OUTSIDE, Regions, faces_of, interior_point};
use crate::{Point2, Solid};

/// A boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Union,
}

/// Which operand a face comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operand {
    First,
    Second,
}

impl Operation {
    /// Whether the operation keeps a piece of a face of `operand` lying where `class` says.
    fn keeps(self, operand: Operand, class: Class) -> bool {
        match self {
            // A face both operands share is kept once, from the first.
            Self::Union => {
                class == Class::Outside || (class == Class::OnSame && operand == Operand::First)
            }
        }
    }
}

impl Solid {
    /// The union of two solids: every point in either of them.
    ///
    /// Solids that only touch along an edge or at a point stay separate lumps of the result.
    /// Gaps narrower than [`crate::LINEAR_TOLERANCE`] close. The result is never empty; it is an
    /// [`Option`] like the other operations' so they can be used alike.
    ///
    /// # Errors
    ///
    /// Returns a [`BooleanError`] if near-coincident vertices cannot be merged safely, or if the
    /// result is not a valid solid.
    pub fn union(&self, other: &Self) -> Result<Option<Self>, BooleanError> {
        run(self, other, Operation::Union)
    }
}

/// Runs an operation; `None` if nothing is left.
fn run(first: &Solid, second: &Solid, operation: Operation) -> Result<Option<Solid>, BooleanError> {
    let mut pool = Pool::new(first, second);
    let mut faces = Vec::with_capacity(first.face_count() + second.face_count());
    for (solid, offset) in [(first, 0), (second, first.vertices().len())] {
        for face in solid.to_faces() {
            let rings = std::iter::once(face.outer)
                .chain(face.holes)
                .map(|ring| ring.into_iter().map(|v| v + offset).collect())
                .collect();
            faces.push(PlanarFace::new(rings, &pool));
        }
    }
    let split = first.face_count();
    let segments = intersect::segments(&faces, split, &mut pool);
    let rename = snap::snap(&pool, &(0..pool.len()).collect::<Vec<_>>())?;
    for face in &mut faces {
        face.remap(&rename, &pool);
    }
    let mut kept = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let (operand, other) = if index < split {
            (Operand::First, &faces[split..])
        } else {
            (Operand::Second, &faces[..split])
        };
        let cuts: Vec<(usize, usize)> = segments[index]
            .iter()
            .map(|&(u, v)| (rename[u], rename[v]))
            .collect();
        let pieces = arrange::split(face, &cuts, &mut pool);
        let keep = |q: Point2| operation.keeps(operand, classify(face.lift(q), face, other, &pool));
        kept.extend(kept_parts(face, &pieces, keep, &pool));
    }
    assemble::assemble(kept, &mut pool)
}

/// The parts of a cut face to keep, as faces: the pieces `keep` accepts at an interior point,
/// merged back together where they touch.
fn kept_parts(
    face: &PlanarFace,
    pieces: &Regions,
    keep: impl Fn(Point2) -> bool,
    pool: &Pool,
) -> Vec<Vec<Vec<usize>>> {
    let at = |v: usize| face.projection.apply(pool.get(v));
    let kept: Vec<bool> = pieces
        .regions
        .iter()
        .map(|region| interior_point(region, at).is_some_and(|q| face.contains(pool, q) && keep(q)))
        .collect();
    let kept_side = |region: usize| region != OUTSIDE && kept[region];
    let boundary = pieces
        .left
        .iter()
        .filter(|&(&(u, v), &region)| {
            kept_side(region) && !kept_side(pieces.left.get(&(v, u)).copied().unwrap_or(OUTSIDE))
        })
        .map(|(&edge, _)| edge)
        .collect();
    faces_of(&boundary, at)
}

#[cfg(test)]
mod tests;
