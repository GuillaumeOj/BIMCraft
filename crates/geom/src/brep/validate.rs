//! Validity checks of a solid.
//!
//! Distances are compared squared, so only the shell areas need a square root (`f64::sqrt` is
//! exactly rounded, so deterministic: ADR 0003). Every tolerance
//! is [`LINEAR_TOLERANCE`], applied as ADR 0001 defines it: a face is planar if its vertices lie
//! within the tolerance of its plane.

use super::{BrepError, Shell, Solid};
use crate::predicates::check;
use crate::{LINEAR_TOLERANCE, MODEL_EXTENT, Point3};

const TOLERANCE_SQUARED: f64 = LINEAR_TOLERANCE * LINEAR_TOLERANCE;

type Vector = [f64; 3];

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
                // Tiny non-zero values pass the extent but break the exact predicates later.
                if c.abs() > MODEL_EXTENT || check(c).is_err() {
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
            if h < edge.twin && dot(d, d) < TOLERANCE_SQUARED {
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
        let mut ring_normals = Vec::new();
        for (face, data) in self.faces.iter().enumerate() {
            points.clear();
            ring_normals.clear();
            let mut outer_longest = 0.0;
            for &ring in &data.rings {
                let start = points.len();
                points.extend(self.ring_vertices(ring).map(|v| self.point(v)));
                let ring = &points[start..];
                let normal = newell(ring);
                let longest = longest_edge_squared(ring);
                // Twice the area over the longest edge bounds the width from above. Holes are
                // checked too: a narrower ring's orientation is noise.
                if dot(normal, normal) < TOLERANCE_SQUARED * longest {
                    return Err(BrepError::DegenerateFace { face });
                }
                if start == 0 {
                    outer_longest = longest;
                }
                ring_normals.push(normal);
            }
            let normal = ring_normals.iter().fold([0.0; 3], |sum, &n| add(sum, n));
            let length_squared = dot(normal, normal);
            if length_squared < TOLERANCE_SQUARED * outer_longest {
                return Err(BrepError::DegenerateFace { face });
            }
            if ring_normals[1..].iter().any(|&n| dot(n, normal) >= 0.0) {
                return Err(BrepError::MisorientedHole { face });
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "a face has far fewer than 2^52 vertices"
            )]
            let centre = scale(
                points.iter().fold([0.0; 3], |sum, &p| add(sum, p)),
                1.0 / points.len() as f64,
            );
            let limit = TOLERANCE_SQUARED * length_squared;
            if points.iter().any(|&p| {
                let off_plane = dot(normal, sub(p, centre));
                off_plane * off_plane > limit
            }) {
                return Err(BrepError::NonPlanarFace { face });
            }
            planes.push((normal, centre));
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
        // Each face is the base of a cone from a point of the shell: six times its volume is the
        // area vector dotted with the height. A shell point, not the origin, keeps products small.
        let apex = planes[faces[0]].1;
        let (six_volume, twice_area) = faces.iter().fold((0.0, 0.0), |(v, a), &f| {
            let (normal, centre) = planes[f];
            (
                v + dot(normal, sub(centre, apex)),
                a + dot(normal, normal).sqrt(),
            )
        });
        let volume = six_volume / 6.0;
        // Twice the volume over the area is the mean thickness: a slab's area counts both sides.
        if 4.0 * volume.abs() <= LINEAR_TOLERANCE * twice_area {
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

/// The area vector of a ring by Newell's method: exact for planar rings, a best fit otherwise.
fn newell(ring: &[Vector]) -> Vector {
    edges(ring).fold([0.0; 3], |[x, y, z], (p, q)| {
        [
            x + (p[1] - q[1]) * (p[2] + q[2]),
            y + (p[2] - q[2]) * (p[0] + q[0]),
            z + (p[0] - q[0]) * (p[1] + q[1]),
        ]
    })
}

fn longest_edge_squared(ring: &[Vector]) -> f64 {
    edges(ring)
        .map(|(p, q)| {
            let d = sub(q, p);
            dot(d, d)
        })
        .fold(0.0, f64::max)
}

/// The edges of a ring as pairs of consecutive points, closing back to the first.
fn edges(ring: &[Vector]) -> impl Iterator<Item = (Vector, Vector)> + '_ {
    ring.iter()
        .copied()
        .zip(ring.iter().copied().cycle().skip(1))
}

const fn coords(p: Point3) -> Vector {
    [p.x, p.y, p.z]
}

fn sub(a: Vector, b: Vector) -> Vector {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: Vector, b: Vector) -> Vector {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Vector, k: f64) -> Vector {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn dot(a: Vector, b: Vector) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
