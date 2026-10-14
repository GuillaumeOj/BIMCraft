//! Why the dependency graph rejects a request.

/// Why a [`DependencyGraph`](super::DependencyGraph) call failed.
///
/// Messages give sizes only, never keys: keys name model objects, which are model content and
/// must stay out of logs and crash reports (SEC-03). Callers read the keys from the fields.
#[derive(Clone, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum DerivedError<K> {
    /// The new dependencies would make a node depend on itself, directly or through others.
    #[error("the dependencies form a cycle through {} nodes", .path.len().saturating_sub(1))]
    Cycle {
        /// The cycle, first and last key equal; each key depends on the next one.
        path: Vec<K>,
    },
    /// The requested node was never declared.
    #[error("the node is not in the dependency graph")]
    UnknownNode,
}
