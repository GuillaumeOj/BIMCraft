//! Where commands come from (API-05).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Who issued a command (API-05).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    /// The user, through the application's interface.
    Ui,
    /// A plugin.
    Plugin {
        /// The plugin's name.
        name: String,
    },
    /// An AI assistant session.
    AiSession {
        /// The session's identifier.
        id: String,
    },
}

/// Plugin names and session ids come from outside the application, so they are written quoted
/// and escaped: a quote or a line break in them cannot forge another log line.
impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ui => f.write_str("the user interface"),
            Self::Plugin { name } => write!(f, "plugin {name:?}"),
            Self::AiSession { id } => write!(f, "AI session {id:?}"),
        }
    }
}

/// One command of a history as it may be shown or logged: what kind of command it was and where
/// it came from, never its arguments, which hold model content (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryEntry<'a> {
    /// The command's [`name`](crate::Command::name).
    pub command: &'static str,
    /// Who issued it.
    pub origin: &'a Origin,
    /// Whether it is undone, and so waits to be redone.
    pub undone: bool,
}

impl fmt::Display for HistoryEntry<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} from {}", self.command, self.origin)?;
        if self.undone {
            f.write_str(" (undone)")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
