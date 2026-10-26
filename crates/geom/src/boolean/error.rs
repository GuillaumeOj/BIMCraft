//! Why a boolean operation fails.

use crate::BrepError;

/// Why a boolean operation could not produce a valid solid.
///
/// Like [`BrepError`], variants never carry coordinates: they are model content and must stay
/// out of logs and crash reports (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum BooleanError {
    /// Vertices joined by edges no longer than the tolerance form a chain wider than ten times
    /// the tolerance, so collapsing them into one would move geometry visibly (ADR 0001, merge
    /// pass step 2).
    #[error("near-coincident vertices form a chain too wide to merge")]
    MergeChainTooWide,
    /// Features of the operands lie within the tolerance of each other in a way the merge pass
    /// cannot resolve: a face narrower than the tolerance, or an edge no longer than it, that
    /// cannot be removed without bending a face beyond the tolerance or breaking the solid apart
    /// (ADR 0001, merge pass). The
    /// operands are valid, but too close to each other there for a valid result.
    #[error("features of the operands lie too close to each other to merge")]
    FeaturesTooClose,
    /// The faces left after the operation do not bound a valid solid.
    #[error("the result is not a valid solid: {0}")]
    InvalidResult(#[from] BrepError),
}
