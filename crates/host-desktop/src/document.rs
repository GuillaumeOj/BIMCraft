//! The document the desktop window works on, served over JSON-RPC (UI-01, API-03).

use std::sync::{Mutex, PoisonError};

use api::jsonrpc::{Handled, Server};
use api::{RegistryError, Session};
use core_model::{Model, ModelError};
use core_txn::{ChangeEvent, Origin};
use schema::Namespace;
use schema_gen::{Source, parse_sources};

use crate::scene_channel::SceneChannel;

/// The definition files of `schema/core`, embedded so that the program needs nothing beside it.
/// A test checks them against the directory, as `schema_gen::load_dir` reads it.
pub(crate) const CORE_SCHEMA: &[(&str, &str)] = &[
    (
        "namespace.toml",
        include_str!("../../../schema/core/namespace.toml"),
    ),
    (
        "entities/block.toml",
        include_str!("../../../schema/core/entities/block.toml"),
    ),
    (
        "entities/material.toml",
        include_str!("../../../schema/core/entities/material.toml"),
    ),
    (
        "entities/material_layer.toml",
        include_str!("../../../schema/core/entities/material_layer.toml"),
    ),
    (
        "entities/material_layer_set.toml",
        include_str!("../../../schema/core/entities/material_layer_set.toml"),
    ),
    (
        "entities/material_original.toml",
        include_str!("../../../schema/core/entities/material_original.toml"),
    ),
    (
        "entities/site.toml",
        include_str!("../../../schema/core/entities/site.toml"),
    ),
    (
        "enumerations/behaviour_law.toml",
        include_str!("../../../schema/core/enumerations/behaviour_law.toml"),
    ),
    (
        "enumerations/build_up_usage.toml",
        include_str!("../../../schema/core/enumerations/build_up_usage.toml"),
    ),
    (
        "enumerations/layer_function.toml",
        include_str!("../../../schema/core/enumerations/layer_function.toml"),
    ),
    (
        "enumerations/material_category.toml",
        include_str!("../../../schema/core/enumerations/material_category.toml"),
    ),
];

/// The core schema built into the application, the namespace every document's model and
/// project file follow.
///
/// # Errors
///
/// [`OpenError::Schema`] when the embedded schema does not parse, which the tests rule out.
pub fn core_namespace() -> Result<Namespace, OpenError> {
    let sources: Vec<Source> = CORE_SCHEMA
        .iter()
        .map(|(path, contents)| Source {
            path: (*path).to_owned(),
            contents: (*contents).to_owned(),
        })
        .collect();
    parse_sources(&sources).map_err(OpenError::Schema)
}

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

/// What the document lock guards: the server and the scene kept current from its changes.
struct Open {
    server: Server,
    scene: SceneChannel,
}

/// One open document and the JSON-RPC server the window reaches it through. Every message it
/// handles comes from the window, so its origin is always [`Origin::Ui`].
pub struct DocumentHost {
    open: Mutex<Open>,
}

impl DocumentHost {
    /// Hosts the document `server` serves.
    #[must_use]
    pub fn new(server: Server) -> Self {
        let scene = SceneChannel::new(server.session().document().model());
        Self {
            open: Mutex::new(Open { server, scene }),
        }
    }

    /// Hosts a new, empty document on the core schema.
    ///
    /// # Errors
    ///
    /// An [`OpenError`] when the built-in schema or registry is broken, which the tests rule out.
    pub fn open_new() -> Result<Self, OpenError> {
        let session = Session::new(Model::new(&core_namespace()?)?);
        Ok(Self::new(Server::new(session)?))
    }

    /// Handles one message from the window.
    pub fn handle(&self, message: &str) -> Handled {
        // Handling never panics on bad input (API-03); if a defect did, later messages are still
        // answered rather than all failing on the poisoned lock.
        let mut open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let handled = open.server.handle(message.as_bytes(), &Origin::Ui);
        let events: Vec<ChangeEvent> = handled
            .notifications
            .iter()
            .filter_map(|notification| change_of(notification))
            .collect();
        if !events.is_empty() {
            let Open { server, scene } = &mut *open;
            scene.apply(&events, server.session().document().model());
        }
        handled
    }

    /// The scene delta, encoded, for a viewport that holds revision `since` (`None`: nothing).
    /// The full scene when the host cannot tell what changed since.
    ///
    /// # Errors
    ///
    /// [`SceneTooLarge`] when the encoded delta exceeds what the viewport's decoder accepts: the
    /// host reports it rather than send bytes that would be refused.
    pub fn scene(&self, since: Option<u64>) -> Result<Vec<u8>, SceneTooLarge> {
        self.scene_within(since, scene_wire::MAX_DELTA_BYTES)
    }

    fn scene_within(&self, since: Option<u64>, max_bytes: usize) -> Result<Vec<u8>, SceneTooLarge> {
        let open = self.open.lock().unwrap_or_else(PoisonError::into_inner);
        let bytes = scene_wire::encode(&open.scene.delta(since));
        if bytes.len() > max_bytes {
            return Err(SceneTooLarge);
        }
        Ok(bytes)
    }
}

/// The scene delta is larger than the viewport accepts. It carries no model content (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the scene is too large to send to the viewport")]
pub struct SceneTooLarge;

/// The change a `model.changed` notification carries.
fn change_of(notification: &str) -> Option<ChangeEvent> {
    let message: serde_json::Value = serde_json::from_str(notification).ok()?;
    serde_json::from_value(message.get("params")?.clone()).ok()
}

#[cfg(test)]
mod tests;
