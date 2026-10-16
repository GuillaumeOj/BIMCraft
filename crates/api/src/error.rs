//! Errors of the registry and of the calls dispatched through it.

use core_txn::{CommandError, HistoryError};

use crate::Version;

/// Why a descriptor could not be registered.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    /// The name is not lower-case `snake_case` segments joined by dots, at least two of them,
    /// such as `model.set`, or is in the `rpc.` area JSON-RPC reserves.
    #[error("{0:?} is not a valid command or query name")]
    InvalidName(&'static str),
    /// A descriptor with this name and version is already registered.
    #[error("{name} {version} is already registered")]
    Duplicate {
        /// The descriptor's name.
        name: &'static str,
        /// The descriptor's version.
        version: Version,
    },
}

/// Why a command or query failed. Messages never hold the call's parameters, which may carry
/// model content (SEC-03). A failed call leaves the model and its history unchanged.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ApiError {
    /// No command or query has this name.
    #[error("unknown command or query {0:?}")]
    UnknownName(String),
    /// The command or query exists, but not in this version.
    #[error("{name} has no version {version}")]
    UnknownVersion {
        /// The name asked for.
        name: String,
        /// The version asked for.
        version: Version,
    },
    /// The parameters do not decode into the command's or query's input.
    #[error("invalid parameters")]
    InvalidParams {
        /// What the decoder rejected. It may quote the parameters, so it is meant for the caller
        /// that sent them and is never logged.
        detail: String,
    },
    /// No object has the id asked for.
    #[error("no object with this id")]
    NotFound,
    /// The model rejected the command.
    #[error(transparent)]
    Rejected(#[from] CommandError),
    /// Undo was asked with nothing to undo.
    #[error("nothing to undo")]
    NothingToUndo,
    /// Redo was asked with nothing to redo.
    #[error("nothing to redo")]
    NothingToRedo,
    /// A bug in the core, not in the call: a result that cannot be encoded, or a history that no
    /// longer fits its model.
    #[error("internal error")]
    Internal,
}

impl From<HistoryError> for ApiError {
    fn from(error: HistoryError) -> Self {
        match error {
            HistoryError::NothingToUndo => Self::NothingToUndo,
            HistoryError::NothingToRedo => Self::NothingToRedo,
            // Only a model changed behind the history's back makes a recorded change fail again.
            HistoryError::Model(_) => Self::Internal,
        }
    }
}
