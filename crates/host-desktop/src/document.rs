//! The document the desktop window works on, served over JSON-RPC (UI-01, API-03).

use std::sync::{Mutex, PoisonError};

use api::jsonrpc::{Handled, Server};
use api::{RegistryError, Session};
use core_model::{Model, ModelError};
use core_txn::Origin;
use schema_gen::{Source, parse_sources};

/// The files of `schema/core`, embedded so that the program needs nothing beside it. The tests
/// check that the list matches the directory.
pub(crate) const CORE_SCHEMA: &[(&str, &str)] = &[(
    "namespace.toml",
    include_str!("../../../schema/core/namespace.toml"),
)];

/// Why a document could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The embedded core schema did not parse; a build defect, never the user's input.
    #[error("the built-in core schema is invalid: {0:?}")]
    Schema(Vec<schema_gen::Error>),
    /// The model could not be built on the schema.
    #[error(transparent)]
    Model(#[from] ModelError),
    /// The commands and queries could not be registered.
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// One open document and the JSON-RPC server the window reaches it through. Every message it
/// handles comes from the window, so its origin is always [`Origin::Ui`].
pub struct DocumentHost {
    server: Mutex<Server>,
}

impl DocumentHost {
    /// Hosts the document `server` serves.
    #[must_use]
    pub fn new(server: Server) -> Self {
        Self {
            server: Mutex::new(server),
        }
    }

    /// Hosts a new, empty document on the core schema.
    ///
    /// # Errors
    ///
    /// An [`OpenError`] when the built-in schema or registry is broken, which the tests rule out.
    pub fn open_new() -> Result<Self, OpenError> {
        let sources: Vec<Source> = CORE_SCHEMA
            .iter()
            .map(|(path, contents)| Source {
                path: (*path).to_owned(),
                contents: (*contents).to_owned(),
            })
            .collect();
        let namespace = parse_sources(&sources).map_err(OpenError::Schema)?;
        let session = Session::new(Model::new(&namespace)?);
        Ok(Self::new(Server::new(session)?))
    }

    /// Handles one message from the window.
    pub fn handle(&self, message: &str) -> Handled {
        // Handling never panics on bad input (API-03); if a defect did, later messages are still
        // answered rather than all failing on the poisoned lock.
        let mut server = self.server.lock().unwrap_or_else(PoisonError::into_inner);
        server.handle(message.as_bytes(), &Origin::Ui)
    }
}

#[cfg(test)]
mod tests;
