//! Boundary representation of solids bounded by planar faces (GEO-03).
//!
//! A [`Solid`] is a set of closed shells. Each shell is a connected, orientable 2-manifold made
//! of planar faces; a face is an outer ring of vertices plus zero or more hole rings. Inside, each
//! edge is a pair of opposite half-edges, so walking around a face or a vertex needs no search.
//!
//! Every way to get a [`Solid`] goes through [`Solid::from_faces`], which runs
//! [`Solid::validate`] before returning, so an invalid solid never exists, in debug or release
//! builds. [`Solid::extrude`] builds the faces of a prism and hands them to it. Operations that
//! will later edit a solid in place must call [`Solid::validate`] on their result too, as ADR 0001
//! requires of the booleans.
//!
//! Validation checks topology, the Euler–Poincaré formula, planarity, orientation and the
//! tolerance rules of `docs/dev/adr/0001-geometry-tolerance-policy.md`. It does not check that
//! faces avoid each other, that holes lie inside their face, or that voids lie inside an outer
//! shell: those are geometric intersection questions, which the booleans answer when they build
//! their result.

mod build;
mod error;
mod extrude;
mod locate;
mod validate;

pub use error::BrepError;
pub use extrude::ExtrusionError;
pub use locate::Containment;
pub(crate) use validate::{FaceFault, enclosed_volume, face_plane, longest_edge_squared};

use crate::Point3;

/// A face given by vertex indices into the solid's vertex list.
///
/// Seen from outside the solid, the outer ring turns counter-clockwise and each hole turns
/// clockwise.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct FaceInput {
    /// The outer boundary.
    pub outer: Vec<usize>,
    /// The holes, if any.
    pub holes: Vec<Vec<usize>>,
}

/// A closed, connected part of a solid's boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct Shell {
    faces: Vec<usize>,
    genus: u32,
    volume: f64,
}

impl Shell {
    /// Indices of the faces of this shell, in increasing order.
    #[must_use]
    pub fn faces(&self) -> &[usize] {
        &self.faces
    }

    /// The number of through holes, from the Euler–Poincaré formula: 0 for a box, 1 for a box
    /// with a hole through it.
    #[must_use]
    pub const fn genus(&self) -> u32 {
        self.genus
    }

    /// The signed volume enclosed, in cubic metres: positive for an outer shell, negative for
    /// a void.
    #[must_use]
    pub const fn volume(&self) -> f64 {
        self.volume
    }

    /// Whether this shell bounds a void inside the solid rather than its outside.
    #[must_use]
    pub fn is_void(&self) -> bool {
        self.volume < 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfEdge {
    origin: usize,
    twin: usize,
    next: usize,
    ring: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ring {
    first: usize,
    face: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Face {
    /// The outer ring first, then the holes.
    rings: Vec<usize>,
}

/// A valid solid bounded by planar faces.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    vertices: Vec<Point3>,
    half_edges: Vec<HalfEdge>,
    rings: Vec<Ring>,
    faces: Vec<Face>,
    shells: Vec<Shell>,
}

impl Solid {
    /// The vertices, in the order given to [`Solid::from_faces`].
    #[must_use]
    pub fn vertices(&self) -> &[Point3] {
        &self.vertices
    }

    /// The number of edges; each edge is shared by exactly two face sides.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.half_edges.len() / 2
    }

    /// The number of faces.
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// The shells, ordered by their smallest face index.
    #[must_use]
    pub fn shells(&self) -> &[Shell] {
        &self.shells
    }

    /// The volume of the solid, in cubic metres: outer shells minus voids.
    #[must_use]
    pub fn volume(&self) -> f64 {
        self.shells.iter().map(Shell::volume).sum()
    }

    /// The faces as given to [`Solid::from_faces`], so a solid can be rebuilt from them.
    #[must_use]
    pub fn to_faces(&self) -> Vec<FaceInput> {
        self.faces
            .iter()
            .map(|face| {
                let mut rings = face.rings.iter().map(|&r| self.ring_vertices(r).collect());
                FaceInput {
                    outer: rings.next().unwrap_or_default(),
                    holes: rings.collect(),
                }
            })
            .collect()
    }

    /// The half-edges of a ring, in order. Only call on rings whose links are known to close.
    fn ring_half_edges(&self, ring: usize) -> impl Iterator<Item = usize> + '_ {
        let first = self.rings[ring].first;
        std::iter::successors(Some(first), move |&h| {
            Some(self.half_edges[h].next).filter(|&next| next != first)
        })
    }

    fn ring_vertices(&self, ring: usize) -> impl Iterator<Item = usize> + '_ {
        self.ring_half_edges(ring)
            .map(|h| self.half_edges[h].origin)
    }

    fn face_of(&self, half_edge: usize) -> usize {
        self.rings[self.half_edges[half_edge].ring].face
    }
}

#[cfg(test)]
mod tests;
