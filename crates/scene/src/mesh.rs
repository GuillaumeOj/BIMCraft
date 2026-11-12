//! The mesh of an element: the meshes of all its solids, merged.

use geom::TriangleMesh;

use crate::SceneError;

/// The mesh of an element: its solids, each tessellated and merged into one flat-shaded mesh.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElementMesh {
    /// The merged mesh.
    pub mesh: TriangleMesh,
}

/// The most vertices a merged mesh may have: positions that 32-bit indices address.
const MAX_VERTICES: u64 = 1 << 32;

impl ElementMesh {
    /// Merges the meshes of an element's solids, in order.
    ///
    /// # Errors
    ///
    /// [`SceneError::TooManyVertices`] if 32-bit indices cannot address every vertex.
    pub(crate) fn merge(parts: Vec<TriangleMesh>) -> Result<Self, SceneError> {
        merge_with_limit(parts, MAX_VERTICES).map(|mesh| Self { mesh })
    }
}

/// Merges meshes into one, failing past `max_vertices` positions: the limit is a parameter so a
/// test can reach it without allocating four billion vertices.
pub(crate) fn merge_with_limit(
    parts: Vec<TriangleMesh>,
    max_vertices: u64,
) -> Result<TriangleMesh, SceneError> {
    let mut merged = TriangleMesh::default();
    for part in parts.into_iter().filter(|part| !part.positions.is_empty()) {
        let base = merged.positions.len();
        let fits = base
            .checked_add(part.positions.len())
            .and_then(|total| u64::try_from(total).ok())
            .is_some_and(|total| total <= max_vertices);
        let offset = u32::try_from(base).map_err(|_| SceneError::TooManyVertices)?;
        if !fits {
            return Err(SceneError::TooManyVertices);
        }
        for index in part.indices {
            merged.indices.push(
                index
                    .checked_add(offset)
                    .ok_or(SceneError::TooManyVertices)?,
            );
        }
        merged.positions.extend(part.positions);
        merged.normals.extend(part.normals);
    }
    Ok(merged)
}
