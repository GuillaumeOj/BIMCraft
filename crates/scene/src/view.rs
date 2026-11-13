//! Which representation of an element the scene meshes (GEO-06).

use schema::Representation;

/// The representation a scene meshes: in `context`, the one with the greatest level of detail not
/// above `lod`. An entity with none is not meshed.
///
/// A plan view asks for the `footprint` context, a 3D view for `body` at the level of detail its
/// zoom calls for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The representation context: `body`, `axis`, `footprint`…
    pub context: String,
    /// The highest level of detail to mesh.
    pub lod: u16,
}

impl View {
    /// A view of `context` up to level of detail `lod`.
    #[must_use]
    pub fn new(context: impl Into<String>, lod: u16) -> Self {
        Self {
            context: context.into(),
            lod,
        }
    }

    /// The representation the view meshes among an entity's, if any: the greatest level of detail
    /// in its context not above its own, the last declared on a tie.
    pub(crate) fn pick<'a>(
        &self,
        representations: &'a [Representation],
    ) -> Option<&'a Representation> {
        representations
            .iter()
            .filter(|r| r.context == self.context && r.lod <= self.lod)
            .max_by_key(|r| r.lod)
    }
}
