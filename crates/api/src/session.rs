//! The context the built-in commands and queries run on.

use core_model::Model;
use core_txn::{Document, ModelCommand};

use crate::{Registry, RegistryError, commands, queries};

/// One open model and its history: what a client works on through the API.
#[derive(Clone, Debug)]
pub struct Session {
    document: Document<ModelCommand>,
}

impl Session {
    /// A session on `model`, with an empty history.
    #[must_use]
    pub fn new(model: Model) -> Self {
        Self {
            document: Document::new(model),
        }
    }

    /// The model and its history. Clients change them only through commands.
    #[must_use]
    pub fn document(&self) -> &Document<ModelCommand> {
        &self.document
    }

    pub(crate) fn document_mut(&mut self) -> &mut Document<ModelCommand> {
        &mut self.document
    }

    /// Every built-in command and query.
    ///
    /// # Errors
    ///
    /// Never in practice: built-in names are valid and distinct, which the tests check.
    pub fn registry() -> Result<Registry<Self>, RegistryError> {
        let mut registry = Registry::new();
        for descriptor in commands::descriptors().chain(queries::descriptors()) {
            registry.register(descriptor)?;
        }
        Ok(registry)
    }
}
