//! Triangle meshes of solids, for display (GEO-05). Rationale: ADR 0022.
//!
//! Each face is triangulated on its own, from its own vertices: it is projected on the
//! coordinate plane that drops the largest component of its normal, its holes are bridged into
//! its outer ring, and ears are clipped from the ring that results. Every orientation and
//! point-in-triangle question is decided by the exact [`orient2d`](crate::orient2d), so the
//! result never depends on rounding. A face simple only within the tolerance, which clipping
//! cannot finish, may get slivers no wider than [`MERGE_CHAIN_WIDTH`](crate::MERGE_CHAIN_WIDTH)
//! turning the wrong way, found by a search over its triangulations.

mod bridge;
mod ear;
mod polygon;
mod search;

use polygon::{Failed, triangulate};

use crate::boolean::Projection;
use crate::vector::{add, coords, newell, unit};
use crate::{Point2, Point3, Solid, Vector3};

/// A triangle mesh with one normal per vertex, flat-shaded (GEO-05).
///
/// Vertices are not shared between faces: each carries the outward unit normal of its face, as
/// a renderer shading faces flat expects. Triangles turn counterclockwise seen from outside.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangleMesh {
    /// Vertex positions, in metres, face after face.
    pub positions: Vec<Point3>,
    /// The outward unit normal of each position's face, one per position.
    pub normals: Vec<Vector3>,
    /// Three indices into [`positions`](Self::positions) per triangle.
    pub indices: Vec<u32>,
}

impl TriangleMesh {
    /// The number of triangles.
    #[must_use]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

/// Why a solid cannot be turned into a triangle mesh.
///
/// Variants name the face at fault by index, never by coordinates: coordinates are model
/// content and must stay out of logs and crash reports (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum TessellationError {
    /// The face's vertices admit no triangulation, even with slivers: its rings cross, or come
    /// within a few tolerances of themselves, more widely than a sliver can cover. Validation
    /// does not rule it out, as it does not check that faces are simple: a solid that passes
    /// [`Solid::validate`] may still have such a face (ADR 0022).
    #[error("face {face} cannot be triangulated")]
    Untriangulable {
        /// Index of the face.
        face: usize,
    },
    /// The mesh would have more vertices than 32-bit indices address. This bounds the index
    /// width only; a renderer may cap meshes lower.
    #[error("the mesh has too many vertices for 32-bit indices at face {face}")]
    TooManyVertices {
        /// Index of the first face that does not fit.
        face: usize,
    },
}

impl Solid {
    /// Turns the solid into a triangle mesh, face by face.
    ///
    /// A face with `n` vertices over all its rings and `h` holes gives exactly `n + 2h − 2`
    /// triangles, all made of its own vertices.
    ///
    /// # Errors
    ///
    /// [`TessellationError`] naming the first face that cannot be triangulated or indexed.
    pub fn tessellate(&self) -> Result<TriangleMesh, TessellationError> {
        let mut mesh = TriangleMesh::default();
        let mut vertices = Vec::new();
        let mut ends = Vec::new();
        for face in 0..self.face_count() {
            self.face_vertices(face, &mut vertices, &mut ends);
            self.tessellate_face(face, &vertices, &ends, &mut mesh)?;
        }
        Ok(mesh)
    }

    /// Appends one face, given as the vertex indices of its rings one after the other and where
    /// each ring ends, to the mesh.
    fn tessellate_face(
        &self,
        face: usize,
        vertices: &[usize],
        ends: &[usize],
        mesh: &mut TriangleMesh,
    ) -> Result<(), TessellationError> {
        let untriangulable = TessellationError::Untriangulable { face };
        let points: Vec<Point3> = vertices.iter().map(|&v| self.vertices()[v]).collect();
        let corners: Vec<_> = points.iter().map(|&p| coords(p)).collect();
        // Validation checked the face; only its area vector is needed.
        let mut start = 0;
        let mut area = [0.0; 3];
        for &end in ends {
            area = add(area, newell(&corners[start..end]));
            start = end;
        }
        let [x, y, z] = unit(area).ok_or(untriangulable)?;
        // The projection the booleans decide faces in, so their rings are simple in it too.
        let projection = Projection::along(area);
        let flat: Vec<Point2> = corners.iter().map(|&c| projection.apply(c)).collect();
        let triangles = triangulate(&flat, ends).map_err(|Failed| untriangulable)?;
        let base = mesh.positions.len();
        let too_many = TessellationError::TooManyVertices { face };
        // Every vertex is a corner of some triangle, so this checks every position's index.
        for local in triangles.into_iter().flatten() {
            let index = u32::try_from(base + local).map_err(|_| too_many)?;
            mesh.indices.push(index);
        }
        let normal = Vector3::new(x, y, z);
        mesh.normals
            .extend(std::iter::repeat_n(normal, points.len()));
        mesh.positions.extend(points);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
