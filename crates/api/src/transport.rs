//! Transports carry JSON-RPC messages between a client and a [`Server`] (API-04). Every
//! transport, in-process, local IPC or network, passes the same conformance suite
//! (`api::conformance`, behind the `conformance` feature).

use core_txn::Origin;

use crate::jsonrpc::Server;

/// Why a message could not be carried.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("transport failed: {reason}")]
pub struct TransportError {
    /// What failed, such as a closed connection. It never holds the message.
    pub reason: String,
}

/// A client's end of a connection to a server.
///
/// Calls are blocking and carry text, so the trait builds everywhere, `wasm32-unknown-unknown`
/// included; an asynchronous transport adapts to it in its tests.
pub trait Transport {
    /// Sends one JSON-RPC message, a request, a notification or a batch, and returns the
    /// server's response, or `None` when it sends none, as for notifications.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when the message did not reach the server or the response did not
    /// come back.
    fn send(&mut self, message: &str) -> Result<Option<String>, TransportError>;

    /// Takes the notifications the server sent since the last call, oldest first.
    fn notifications(&mut self) -> Vec<String>;
}

/// A transport to a server in the same process: no serialisation beyond the messages
/// themselves, no failure. Tests, the browser build and the desktop host use it.
pub struct InProcess {
    server: Server,
    origin: Origin,
    notifications: Vec<String>,
}

impl InProcess {
    /// A connection to `server` for a client known as `origin`.
    #[must_use]
    pub fn new(server: Server, origin: Origin) -> Self {
        Self {
            server,
            origin,
            notifications: Vec::new(),
        }
    }

    /// The server, to read its session.
    #[must_use]
    pub fn server(&self) -> &Server {
        &self.server
    }
}

impl Transport for InProcess {
    fn send(&mut self, message: &str) -> Result<Option<String>, TransportError> {
        let handled = self.server.handle(message.as_bytes(), &self.origin);
        self.notifications.extend(handled.notifications);
        Ok(handled.response)
    }

    fn notifications(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notifications)
    }
}

#[cfg(test)]
mod tests;
