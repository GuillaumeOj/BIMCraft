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
    /// A georeference command or query failed; see [`GeorefError`].
    #[error(transparent)]
    Georef(#[from] GeorefError),
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

/// Why a `georef.*` call failed. Each has a stable `reason` the UI maps to a message, and none
/// repeats a value of the model (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GeorefError {
    /// The EPSG code is not in the registry.
    #[error("the coordinate reference system is unknown")]
    UnknownCrs,
    /// The EPSG code is a geographic CRS, which cannot georeference a project.
    #[error("the coordinate reference system is not projected")]
    NotProjected,
    /// The scale is outside `Georeference::SCALE_RANGE`.
    #[error("the scale is out of range")]
    InvalidScale,
    /// A point is outside the domain of the projection. `index` is the position of the first
    /// such point of a conversion.
    #[error("a point is outside the domain of the projection")]
    OutsideDomain {
        /// The position of the first bad point, for a conversion.
        index: Option<usize>,
    },
    /// The project has no georeference.
    #[error("the project is not georeferenced")]
    NotGeoreferenced,
    /// The model holds more than one `Site`.
    #[error("the model holds more than one site")]
    MultipleSites,
    /// The project has no `Site` and the call did not name the id of the one to create.
    #[error("the id of the new site is missing")]
    MissingSiteId,
}

impl GeorefError {
    /// The stable `snake_case` name of the error, the `data.reason` of its response.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::UnknownCrs => "unknown_crs",
            Self::NotProjected => "not_projected",
            Self::InvalidScale => "invalid_scale",
            Self::OutsideDomain { .. } => "outside_domain",
            Self::NotGeoreferenced => "not_georeferenced",
            Self::MultipleSites => "multiple_sites",
            Self::MissingSiteId => "missing_site_id",
        }
    }
}
