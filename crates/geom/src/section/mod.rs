//! Section properties beyond area and centroid: the shear centre (STR-14). Rationale:
//! `docs/dev/adr/0011-shear-centre.md`.

mod mesh;
mod warping;

pub use mesh::MAX_MESH_NODES;

use crate::{Point2, Profile, ProfileError};

/// Why the shear centre of a profile could not be computed. Carries no coordinates (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum ShearCentreError {
    /// Replacing arcs by chords failed.
    #[error("the profile's arcs could not be faceted: {0}")]
    Faceting(#[from] ProfileError),
    /// The mesh would need more than [`MAX_MESH_NODES`] nodes.
    #[error("the profile needs a mesh of more than {MAX_MESH_NODES} nodes")]
    MeshTooLarge,
    /// The boundary could not be meshed, for instance at an extremely sharp corner.
    #[error("the profile could not be meshed")]
    MeshFailed,
    /// The warping solve did not converge.
    #[error("the warping solve did not converge")]
    NotConverged,
}

impl Profile {
    /// The shear centre: the point a transverse load must pass through to bend the member
    /// without twisting it, by Trefftz's definition. It is the centroid for doubly symmetric
    /// profiles, and may lie outside the profile, as for a channel.
    ///
    /// Computed with finite elements, to about 1 % of the profile's size; this costs a mesh and
    /// a solve, so callers keep the result.
    ///
    /// # Errors
    ///
    /// [`ShearCentreError`] when faceting the arcs fails, the mesh would be too large or cannot
    /// be built, or the solve does not converge.
    pub fn shear_centre(&self) -> Result<Point2, ShearCentreError> {
        let faceted = self.faceted()?;
        // The mesh covers the faceted profile exactly, so they share their centroid.
        warping::shear_centre(&mesh::mesh(&faceted)?, faceted.centroid())
    }
}

#[cfg(test)]
mod tests;
