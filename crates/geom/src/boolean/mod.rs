//! Booleans of solids bounded by planar faces (GEO-03), following the tolerance policy of
//! `docs/dev/adr/0001-geometry-tolerance-policy.md`.
//!
//! Every operation runs the same steps:
//!
//! 1. Faces of the two operands that lie in one plane within the tolerance are put onto exactly
//!    one plane, and share its number (merge pass, step 1).
//! 2. Each face of one operand is intersected with the faces of the other whose boxes overlap
//!    it, on the operands' own coordinates: no vertex is merged with another before the cut
//!    (ADR 0001, constructed geometry). A vertex lies on a plane if it was made on it, or lies
//!    within rounding of it; otherwise it lies on the side its distance says, however near. Where
//!    an edge crosses a plane, the vertex is computed once and rounded once, and is shared by
//!    every face along the edge. Which stretches of the line two planes share lie in both faces
//!    is counted exactly from where their boundaries cross it. Coplanar faces exchange their
//!    boundaries. Every vertex records the planes it lies on and the lines it was made on: the
//!    edges it lies on and the lines where two planes meet.
//! 3. Vertices made on three of the same planes, or within rounding of each other, are one
//!    point: they become one vertex.
//! 4. Each face is cut along those segments into pieces. Every edge and segment is split once
//!    for all the faces along it, at the vertices made on it, then where its pieces cross
//!    another's, so neighbouring pieces share their edges exactly. Whether pieces cross is
//!    decided exactly, and a crossing made on three planes a vertex was made on is that vertex;
//!    whether a piece lies inside its face is read from the face's boundary as split; every piece
//!    keeps the plane of its input face.
//! 5. Each piece is classified against the other operand: inside, outside, or on one of its
//!    faces, facing the same way or the opposite way. The side is read along the edges where
//!    the other operand's faces cut the pieces, from those faces' planes, and carried across
//!    the edges they do not hold; only pieces the other operand does not reach are classified
//!    from a point inside them. Pieces of faces of the two operands with exactly the same
//!    boundary lie on each other. The operation keeps some classes and drops the others; kept
//!    pieces of the same face merge back into as few faces as possible.
//! 6. The kept faces are stitched into a solid (merge pass, steps 1 to 4): lumps that only touch
//!    along an edge or at a point get their own vertices, edges no longer than the tolerance
//!    collapse, faces left back to back cancel, adjacent coplanar faces merge, vertices between
//!    collinear edges go, faces narrower than the tolerance are zipped shut onto the face across
//!    their longest edge, and lumps or voids no thicker than the tolerance on average vanish.
//!    Each of these operations is applied only if the faces it leaves pass the solid's checks,
//!    so it never makes a valid result invalid; a feature within the tolerance that none can
//!    remove is [`BooleanError::FeaturesTooClose`]. The result is checked like any solid; an
//!    invalid one is an error, never a panic.
//!
//! The result is regularised: pieces of zero thickness, such as a face two operands share, are
//! dropped.
//!
//! Once coplanar faces share a plane (step 1), the cut decides on the operands as they are:
//! which side of a plane a vertex lies on and whether pieces cross never depend on the tolerance,
//! so two of its decisions cannot disagree about a feature within it. The merge pass reads
//! "within the tolerance" the same way everywhere: a distance at most the tolerance, up to
//! rounding.

mod align;
mod arrange;
mod assemble;
mod classify;
mod error;
mod face;
mod intersect;
mod merge;
mod planar;
mod snap;

pub use error::BooleanError;

use self::classify::Class;
use self::face::{PlanarFace, Pool};
use self::planar::{OUTSIDE, Regions, faces_of};
use crate::Solid;

/// A boolean operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Union,
    Difference,
    Intersection,
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
            // The cutter's faces inside the first solid bound the cut, turned to face out of
            // it. Where the two solids' faces meet back to back the first solid's face stays;
            // where they coincide the cut removes it, so a flush cutter leaves no skin.
            Self::Difference => match operand {
                Operand::First => class == Class::Outside || class == Class::OnOpposite,
                Operand::Second => class == Class::Inside,
            },
            // Faces back to back bound nothing both solids hold, so solids that only touch
            // leave nothing.
            Self::Intersection => {
                class == Class::Inside || (class == Class::OnSame && operand == Operand::First)
            }
        }
    }

    /// Whether the kept faces of `operand` are turned inside out.
    fn reverses(self, operand: Operand) -> bool {
        self == Self::Difference && operand == Operand::Second
    }
}

impl Solid {
    /// The union of two solids: every point in either of them.
    ///
    /// Solids that only touch along an edge or at a point stay separate lumps of the result.
    /// Gaps no wider than [`crate::LINEAR_TOLERANCE`] close. The result is never empty; it is an
    /// [`Option`] like the other operations' so they can be used alike.
    ///
    /// # Errors
    ///
    /// Returns a [`BooleanError`] if near-coincident vertices or faces cannot be merged safely,
    /// or if the result is not a valid solid.
    pub fn union(&self, other: &Self) -> Result<Option<Self>, BooleanError> {
        run(self, other, Operation::Union)
    }

    /// This solid with another cut out of it: every point in this solid and not in the other.
    /// Openings and voids in walls and slabs are made this way.
    ///
    /// A cutter flush with a face leaves no skin over the cut, and a cut that would leave a
    /// layer no thicker than [`crate::LINEAR_TOLERANCE`] removes it. A cutter strictly inside
    /// leaves a void. `None` if the cutter covers this solid.
    ///
    /// # Errors
    ///
    /// Returns a [`BooleanError`] if near-coincident vertices or faces cannot be merged safely,
    /// or if the result is not a valid solid.
    pub fn difference(&self, cutter: &Self) -> Result<Option<Self>, BooleanError> {
        run(self, cutter, Operation::Difference)
    }

    /// The intersection of two solids: every point in both of them.
    ///
    /// Solids that only touch along a face, an edge or a point, or whose overlap is no thicker
    /// than [`crate::LINEAR_TOLERANCE`] on average, have no intersection: `None`.
    ///
    /// # Errors
    ///
    /// Returns a [`BooleanError`] if near-coincident vertices or faces cannot be merged safely,
    /// or if the result is not a valid solid.
    pub fn intersection(&self, other: &Self) -> Result<Option<Self>, BooleanError> {
        run(self, other, Operation::Intersection)
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
            let number = faces.len();
            faces.push(PlanarFace::new(rings, &pool, number));
        }
    }
    let split = first.face_count();
    align::align(&mut faces, split, &mut pool);
    for face in &faces {
        for &v in face.rings.iter().flatten() {
            pool.lies_on(v, std::iter::once(face.plane));
        }
    }
    pool.place_all();
    let (mut segments, cuts, supports) = intersect::segments(&faces, split, &mut pool);
    let rename = snap::coincident(&mut pool);
    for face in &mut faces {
        face.remap(&rename, &pool);
    }
    for (u, v) in segments.iter_mut().flatten() {
        (*u, *v) = (rename[*u], rename[*v]);
    }
    let cuts = intersect::renamed(cuts, &rename);
    let supports = intersect::renamed_supports(supports, &rename);
    let chains = arrange::node(&faces, &segments, &supports, &mut pool);
    let pieces: Vec<arrange::Pieces> = (faces.iter().enumerate())
        .map(|(index, face)| arrange::split(face, index, &chains, &pool))
        .collect();
    let holding = chains.holding(&pool);
    let classes = classify::classes(&faces, split, &pieces, &holding, &cuts, &pool);
    let mut kept = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let operand = if index < split {
            Operand::First
        } else {
            Operand::Second
        };
        let keep: Vec<bool> = classes[index]
            .iter()
            .map(|class| class.is_some_and(|class| operation.keeps(operand, class)))
            .collect();
        let mut parts = kept_parts(face, &pieces[index].regions, &keep, &pool);
        if operation.reverses(operand) {
            parts.iter_mut().flatten().for_each(|ring| ring.reverse());
        }
        kept.extend(parts);
    }
    assemble::assemble(kept, &mut pool)
}

/// The parts of a cut face to keep, as faces: the pieces marked `kept`, merged back together
/// where they touch.
fn kept_parts(
    face: &PlanarFace,
    pieces: &Regions,
    kept: &[bool],
    pool: &Pool,
) -> Vec<Vec<Vec<usize>>> {
    let at = |v: usize| face.projection.apply(pool.get(v));
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
