//! Building a solid from faces given as vertex indices.

use std::collections::BTreeMap;

use super::{BrepError, Face, FaceInput, HalfEdge, Ring, Solid};
use crate::Point3;

impl Solid {
    /// Builds a solid from its vertices and faces, then validates it.
    ///
    /// Seen from outside, each outer ring turns counter-clockwise and each hole clockwise. Faces
    /// may form several shells: separate lumps, or voids inside a lump (turned inside out).
    ///
    /// # Errors
    ///
    /// Returns the first [`BrepError`] found; the order of checks is deterministic.
    pub fn from_faces(vertices: Vec<Point3>, faces: &[FaceInput]) -> Result<Self, BrepError> {
        if faces.is_empty() {
            return Err(BrepError::Empty);
        }
        let mut half_edges = Vec::new();
        let mut rings = Vec::new();
        let mut solid_faces = Vec::with_capacity(faces.len());
        // A BTreeMap, not a HashMap, so the first error reported never depends on hashing.
        let mut directed = BTreeMap::new();
        // The last ring each vertex was seen on, to find repeats without sorting each ring.
        let mut last_ring = vec![usize::MAX; vertices.len()];
        for (face, input) in faces.iter().enumerate() {
            let mut face_rings = Vec::with_capacity(1 + input.holes.len());
            for ring in std::iter::once(&input.outer).chain(&input.holes) {
                check_ring(ring, rings.len(), &mut last_ring, face)?;
                let first = half_edges.len();
                for (i, &origin) in ring.iter().enumerate() {
                    let next = (i + 1) % ring.len();
                    if directed.insert((origin, ring[next]), first + i).is_some() {
                        return Err(BrepError::NonManifoldEdge { face });
                    }
                    half_edges.push(HalfEdge {
                        origin,
                        twin: first + i,
                        next: first + next,
                        ring: rings.len(),
                    });
                }
                face_rings.push(rings.len());
                rings.push(Ring { first, face });
            }
            solid_faces.push(Face { rings: face_rings });
        }
        for (&(from, to), &h) in &directed {
            let Some(&twin) = directed.get(&(to, from)) else {
                return Err(BrepError::OpenEdge {
                    face: rings[half_edges[h].ring].face,
                });
            };
            half_edges[h].twin = twin;
        }
        let mut solid = Self {
            vertices,
            half_edges,
            rings,
            faces: solid_faces,
            shells: Vec::new(),
        };
        solid.shells = solid.check()?;
        Ok(solid)
    }

    /// Groups faces into shells: the connected components of faces across shared edges, numbered
    /// in order of their smallest face. Only call once the rings and twins are known to be valid.
    pub(super) fn find_shells(&self) -> Vec<Vec<usize>> {
        const UNSET: usize = usize::MAX;
        let mut shell_of = vec![UNSET; self.faces.len()];
        let mut shells = Vec::new();
        for seed in 0..self.faces.len() {
            if shell_of[seed] != UNSET {
                continue;
            }
            let shell = shells.len();
            shell_of[seed] = shell;
            let mut members = vec![seed];
            let mut pending = vec![seed];
            while let Some(face) = pending.pop() {
                for &ring in &self.faces[face].rings {
                    for h in self.ring_half_edges(ring) {
                        let neighbour = self.face_of(self.half_edges[h].twin);
                        if shell_of[neighbour] == UNSET {
                            shell_of[neighbour] = shell;
                            members.push(neighbour);
                            pending.push(neighbour);
                        }
                    }
                }
            }
            members.sort_unstable();
            shells.push(members);
        }
        shells
    }
}

fn check_ring(
    ring: &[usize],
    index: usize,
    last_ring: &mut [usize],
    face: usize,
) -> Result<(), BrepError> {
    for &v in ring {
        let seen = last_ring
            .get_mut(v)
            .ok_or(BrepError::IndexOutOfRange { face })?;
        if *seen == index {
            return Err(BrepError::DegenerateRing { face });
        }
        *seen = index;
    }
    if ring.len() < 3 {
        return Err(BrepError::DegenerateRing { face });
    }
    Ok(())
}
