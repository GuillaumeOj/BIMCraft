//! Undo and redo (CORE-06).

use core_model::{Model, ModelError};

use crate::transaction::{Change, redo_all, undo_all};
use crate::{Command, CommandError, Transaction};

/// A model and the history of the commands applied to it.
///
/// The model changes only through [`execute`](Self::execute), [`undo`](Self::undo) and
/// [`redo`](Self::redo), so every change is a command run as one transaction, and the history
/// can always undo and redo it exactly.
#[derive(Clone, Debug)]
pub struct Document<C> {
    model: Model,
    /// Applied commands, oldest first.
    done: Vec<Record<C>>,
    /// Undone commands, the next one to redo last.
    undone: Vec<Record<C>>,
}

/// A command of the history and the changes it made.
#[derive(Clone, Debug)]
struct Record<C> {
    command: C,
    changes: Vec<Change>,
}

/// Why the history could not undo or redo.
///
/// The messages never repeat values or ids, which come from a user's model (SEC-03).
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum HistoryError {
    /// No command is left to undo.
    #[error("nothing to undo")]
    NothingToUndo,
    /// No undone command is left to redo.
    #[error("nothing to redo")]
    NothingToRedo,
    /// The model rejected a recorded change. The history only replays changes that applied to
    /// the same model, so this means the model was changed outside the history.
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl<C: Command> Document<C> {
    /// Starts a history on `model`; the commands applied from now on can be undone.
    #[must_use]
    pub fn new(model: Model) -> Self {
        Self {
            model,
            done: Vec::new(),
            undone: Vec::new(),
        }
    }

    /// The model.
    #[must_use]
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The applied commands, oldest first: replayed on the model the history started from, they
    /// rebuild the current model.
    pub fn commands(&self) -> impl Iterator<Item = &C> {
        self.done.iter().map(|record| &record.command)
    }

    /// Whether a command can be undone.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// Whether an undone command can be redone.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Applies `command` as one transaction and records it. Commands undone before can no
    /// longer be redone.
    ///
    /// # Errors
    ///
    /// Returns the command's error; the model and the history are then unchanged.
    pub fn execute(&mut self, command: C) -> Result<(), CommandError> {
        let changes = Transaction::run(&mut self.model, &command)?;
        self.undone.clear();
        self.done.push(Record { command, changes });
        Ok(())
    }

    /// Reverts the last applied command.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToUndo`] when no command was applied.
    pub fn undo(&mut self) -> Result<(), HistoryError> {
        let record = self.done.pop().ok_or(HistoryError::NothingToUndo)?;
        undo_all(&mut self.model, &record.changes)?;
        self.undone.push(record);
        Ok(())
    }

    /// Applies the last undone command again. It replays the recorded changes rather than
    /// running the command, so the model becomes exactly what it was before the undo.
    ///
    /// # Errors
    ///
    /// Returns [`HistoryError::NothingToRedo`] when no command was undone.
    pub fn redo(&mut self) -> Result<(), HistoryError> {
        let record = self.undone.pop().ok_or(HistoryError::NothingToRedo)?;
        redo_all(&mut self.model, &record.changes)?;
        self.done.push(record);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
