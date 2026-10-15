//! Errors of the registry and of the calls dispatched through it.

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
/// model content (SEC-03).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
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
    /// The result could not be encoded: a bug in the command or query, not in the call.
    #[error("the result could not be encoded")]
    Internal,
}
