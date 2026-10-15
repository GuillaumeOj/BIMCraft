//! Why a boolean operation fails.

use crate::BrepError;

/// Why a boolean operation could not produce a valid solid.
///
/// Like [`BrepError`], variants never carry coordinates: they are model content and must stay
/// out of logs and crash reports (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
pub enum BooleanError {
    /// Vertices closer than the tolerance form a chain wider than ten times the tolerance, so
    /// merging them would move geometry visibly (ADR 0001, merge pass step 2).
    #[error("near-coincident vertices form a chain too wide to merge")]
    MergeChainTooWide,
    /// The faces left after the operation do not bound a valid solid.
    #[error("the result is not a valid solid: {0}")]
    InvalidResult(#[from] BrepError),
}
