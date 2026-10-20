//! Validity checks of a solid.
//!
//! Distances are compared squared, so only the shell areas need a square root (`f64::sqrt` is
//! exactly rounded, so deterministic: ADR 0003). Every tolerance
//! is [`LINEAR_TOLERANCE`](crate::LINEAR_TOLERANCE), applied as ADR 0001 defines it: a face is
//! planar if its vertices lie within the tolerance of its plane, and an edge, a face or a shell
//! within the tolerance is too small. Within means at most the tolerance, up to rounding, as
//! everywhere in the kernel.

use super::{BrepError, Shell, Solid};
use crate::tolerance::{WITHIN_TOLERANCE, WITHIN_TOLERANCE_SQUARED, is_model_coordinate};
use crate::vector::{Vector, add, coords, dot, edges, newell, scale, sub};

impl Solid {
    /// Checks every validity rule of the module documentation.
    ///
    /// [`Solid::from_faces`] already runs it; operations that edit a solid call it on their
    /// result.
    ///
    /// # Errors
    ///
    /// Returns the first [`BrepError`] found; the order of checks is deterministic.
    pub fn validate(&self) -> Result<(), BrepError> {
        self.check().map(drop)
    }

    /// Validates the solid and returns its shells, worked out again from its faces.
    pub(super) fn check(&self) -> Result<Vec<Shell>, BrepError> {
        self.check_vertices()?;
        self.check_rings()?;
        self.check_edges()?;
        self.check_vertex_fans()?;
        let planes = self.face_planes()?;
        let groups = self.find_shells();
        // Each vertex has a single fan, so it belongs to exactly one shell.
        let mut shell_of_face = vec![0; self.faces.len()];
        for (shell, faces) in groups.iter().enumerate() {
            for &face in faces {
                shell_of_face[face] = shell;
            }
        }
        let mut shell_of_vertex = vec![0; self.vertices.len()];
        let mut half_edges = vec![0; groups.len()];
        for (h, edge) in self.half_edges.iter().enumerate() {
            let shell = shell_of_face[self.face_of(h)];
            shell_of_vertex[edge.origin] = shell;
            half_edges[shell] += 1;
        }
        let mut vertices = vec![0; groups.len()];
        for shell in shell_of_vertex {
            vertices[shell] += 1;
        }
        let shells = groups
            .into_iter()
            .enumerate()
            .map(|(index, faces)| {
                let counts = (vertices[index], half_edges[index] / 2);
                self.measure_shell(index, faces, counts, &planes)
            })
            .collect::<Result<Vec<_>, _>>()?;
        // Voids are negative, so a positive total also rejects a void larger than its outside.
        if shells.iter().map(Shell::volume).sum::<f64>() <= 0.0 {
            return Err(BrepError::WrongOrientation);
        }
        Ok(shells)
    }

    fn check_vertices(&self) -> Result<(), BrepError> {
        for (vertex, p) in self.vertices.iter().enumerate() {
            for c in coords(*p) {
                if !c.is_finite() {
                    return Err(BrepError::NonFinite { vertex });
                }
                if !is_model_coordinate(c) {
                    return Err(BrepError::OutsideModelExtent { vertex });
                }
            }
        }
        Ok(())
    }

    /// Each ring's links form one cycle of at least three half-edges, and every half-edge lies
    /// on exactly one ring.
    fn check_rings(&self) -> Result<(), BrepError> {
        let mut visited = vec![false; self.half_edges.len()];
        for (index, ring) in self.rings.iter().enumerate() {
            let broken = BrepError::BrokenRing { face: ring.face };
            let mut h = ring.first;
            let mut len = 0;
            loop {
                let edge = self.half_edges[h];
                if edge.ring != index || visited[h] {
                    return Err(broken);
                }
                visited[h] = true;
                len += 1;
                h = edge.next;
                if h == ring.first {
                    break;
                }
            }
            if len < 3 {
                return Err(BrepError::DegenerateRing { face: ring.face });
            }
        }
        match visited.iter().position(|&seen| !seen) {
            Some(h) => Err(BrepError::BrokenRing {
                face: self.face_of(h),
            }),
            None => Ok(()),
        }
    }

    /// Each edge has two sides running in opposite directions and is not too short.
    fn check_edges(&self) -> Result<(), BrepError> {
        for (h, edge) in self.half_edges.iter().enumerate() {
            let twin = self.half_edges[edge.twin];
            let target = self.half_edges[edge.next].origin;
            let twin_target = self.half_edges[twin.next].origin;
            if edge.twin == h
                || twin.twin != h
                || twin.origin != target
                || twin_target != edge.origin
            {
                return Err(BrepError::InconsistentTwin {
                    face: self.face_of(h),
                });
            }
            // Both sides have the same length: measure each edge once.
            let d = sub(self.point(target), self.point(edge.origin));
            if h < edge.twin && dot(d, d) <= WITHIN_TOLERANCE_SQUARED {
                return Err(BrepError::ShortEdge {
                    face: self.face_of(h),
                });
            }
        }
        Ok(())
    }

    /// The faces around each vertex form a single fan, so solids never touch at a point.
    fn check_vertex_fans(&self) -> Result<(), BrepError> {
        // Per vertex: how many half-edges leave it, and one of them.
        let mut outgoing = vec![(0_usize, None); self.vertices.len()];
        for (h, edge) in self.half_edges.iter().enumerate() {
            let (count, any) = &mut outgoing[edge.origin];
            *count += 1;
            *any = Some(h);
        }
        for (vertex, (count, start)) in outgoing.into_iter().enumerate() {
            let Some(start) = start else {
                return Err(BrepError::IsolatedVertex { vertex });
            };
            // Turning around a vertex: the edge after the twin also leaves the vertex.
            let mut h = start;
            let mut fan = 0;
            loop {
                h = self.half_edges[self.half_edges[h].twin].next;
                fan += 1;
                if h == start {
                    break;
                }
            }
            if fan != count {
                return Err(BrepError::NonManifoldVertex { vertex });
            }
        }
        Ok(())
    }

    /// Checks each face and returns its plane: the area vector (normal to the face, twice its
    /// area long) and the centre of its vertices.
    fn face_planes(&self) -> Result<Vec<(Vector, Vector)>, BrepError> {
        let mut planes = Vec::with_capacity(self.faces.len());
        // Reused across faces to avoid allocating per face.
        let mut points = Vec::new();
        let mut ends = Vec::new();
        for (face, data) in self.faces.iter().enumerate() {
            points.clear();
            ends.clear();
            for &ring in &data.rings {
                points.extend(self.ring_vertices(ring).map(|v| self.point(v)));
                ends.push(points.len());
            }
            planes.push(face_plane(&points, &ends).map_err(|fault| fault.of(face))?);
        }
        Ok(planes)
    }

    /// Measures a shell from its face planes, given its vertex and edge counts.
    fn measure_shell(
        &self,
        index: usize,
        faces: Vec<usize>,
        (vertices, edges): (usize, usize),
        planes: &[(Vector, Vector)],
    ) -> Result<Shell, BrepError> {
        let holes = faces.iter().map(|&f| self.faces[f].rings.len() - 1).sum();
        let genus = genus(vertices, edges, faces.len(), holes)
            .ok_or(BrepError::EulerViolation { shell: index })?;
        let shell: Vec<(Vector, Vector)> = faces.iter().map(|&f| planes[f]).collect();
        let (volume, thin) = enclosed_volume(&shell);
        if thin {
            return Err(BrepError::ThinShell { shell: index });
        }
        Ok(Shell {
            faces,
            genus,
            volume,
        })
    }

    fn point(&self, vertex: usize) -> Vector {
        coords(self.vertices[vertex])
    }
}

/// Why a face breaks a validity rule of its own, whatever the faces around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FaceFault {
    /// A ring, or the whole face, no wider than the tolerance.
    Degenerate,
    /// A hole turning the same way as the outer ring.
    MisorientedHole,
    /// A vertex farther than the tolerance from the face's plane.
    NonPlanar,
}

impl FaceFault {
    const fn of(self, face: usize) -> BrepError {
        match self {
            Self::Degenerate => BrepError::DegenerateFace { face },
            Self::MisorientedHole => BrepError::MisorientedHole { face },
            Self::NonPlanar => BrepError::NonPlanarFace { face },
        }
    }
}

/// Checks the rules a face must meet on its own and returns its plane: the area vector (normal
/// to the face, twice its area long) and the centre of its vertices. The face is given as the
/// points of its rings one after the other, the outer ring first, and where each ring ends.
/// The merge pass of the booleans runs it on the faces an operation would leave, so it never
/// leaves one the solid's checks reject.
pub(crate) fn face_plane(points: &[Vector], ends: &[usize]) -> Result<(Vector, Vector), FaceFault> {
    let mut normal = [0.0; 3];
    let mut outer_longest = 0.0;
    let mut start = 0;
    for &end in ends {
        let ring = &points[start..end];
        let ring_normal = newell(ring);
        let longest = longest_edge_squared(ring);
        // Twice the area over the longest edge bounds the width from above. Holes are checked
        // too: a narrower ring's orientation is noise.
        if dot(ring_normal, ring_normal) <= WITHIN_TOLERANCE_SQUARED * longest {
            return Err(FaceFault::Degenerate);
        }
        if start == 0 {
            outer_longest = longest;
        }
        normal = add(normal, ring_normal);
        start = end;
    }
    let length_squared = dot(normal, normal);
    if length_squared <= WITHIN_TOLERANCE_SQUARED * outer_longest {
        return Err(FaceFault::Degenerate);
    }
    let mut start = ends.first().copied().unwrap_or(0);
    for &end in ends.iter().skip(1) {
        if dot(newell(&points[start..end]), normal) >= 0.0 {
            return Err(FaceFault::MisorientedHole);
        }
        start = end;
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "a face has far fewer than 2^52 vertices"
    )]
    let centre = scale(
        points.iter().fold([0.0; 3], |sum, &p| add(sum, p)),
        1.0 / points.len() as f64,
    );
    let limit = WITHIN_TOLERANCE_SQUARED * length_squared;
    if points.iter().any(|&p| {
        let off_plane = dot(normal, sub(p, centre));
        off_plane * off_plane > limit
    }) {
        return Err(FaceFault::NonPlanar);
    }
    Ok((normal, centre))
}

/// The signed volume a closed shell encloses, given the plane of each face as its area vector
/// (normal to the face, twice its area long) and the centre of its vertices; and whether it is
/// thinner than the tolerance on average, which no valid shell is.
pub(crate) fn enclosed_volume(planes: &[(Vector, Vector)]) -> (f64, bool) {
    // Each face is the base of a cone from a point of the shell: six times its volume is the
    // area vector dotted with the height. A shell point, not the origin, keeps products small.
    let apex = planes.first().map_or([0.0; 3], |&(_, centre)| centre);
    let (six_volume, twice_area) = planes.iter().fold((0.0, 0.0), |(v, a), &(normal, centre)| {
        (
            v + dot(normal, sub(centre, apex)),
            a + dot(normal, normal).sqrt(),
        )
    });
    let volume = six_volume / 6.0;
    // Twice the volume over the area is the mean thickness: a slab's area counts both sides.
    (volume, 4.0 * volume.abs() <= WITHIN_TOLERANCE * twice_area)
}

/// The genus G from the Euler–Poincaré formula of one shell, V − E + F − H = 2 − 2G, where H
/// counts the holes in faces. `None` if no whole, non-negative G fits.
fn genus(vertices: usize, edges: usize, faces: usize, holes: usize) -> Option<u32> {
    let twice_genus = (2 + edges + holes).checked_sub(vertices + faces)?;
    if twice_genus % 2 == 0 {
        u32::try_from(twice_genus / 2).ok()
    } else {
        None
    }
}

/// The squared length of the longest edge of a ring: twice its area over it bounds its width.
pub(crate) fn longest_edge_squared(ring: &[Vector]) -> f64 {
    edges(ring)
        .map(|(p, q)| {
            let d = sub(q, p);
            dot(d, d)
        })
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use super::genus;

    #[test]
    fn genus_follows_euler_poincare() {
        // Cube: V8 E12 F6.
        assert_eq!(genus(8, 12, 6, 0), Some(0));
        // Box with a square hole through it: V16 E32 F16, no face holes once split into quads.
        assert_eq!(genus(16, 32, 16, 0), Some(1));
        // The same box drawn with two faces carrying a hole ring: V16 E24 F10 H2.
        assert_eq!(genus(16, 24, 10, 2), Some(1));
    }

    #[test]
    fn genus_rejects_counts_no_surface_has() {
        // Odd: V − E + F − H = 1.
        assert_eq!(genus(8, 12, 5, 0), None);
        // Above 2: two spheres counted as one shell.
        assert_eq!(genus(16, 24, 12, 0), None);
    }
}
