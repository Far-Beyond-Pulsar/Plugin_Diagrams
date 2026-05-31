//! Generic undo/redo history.
//!
//! Commands take `&mut DiagramDocument` directly — no Arc inside commands,
//! so execute() can be called while the caller holds a write-lock on the same doc.

use std::fmt;
use thiserror::Error;

use super::document::DiagramDocument;

#[derive(Error, Debug)]
pub enum CommandError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Undo failed: {0}")]
    UndoFailed(String),
}

pub type CommandResult<T> = Result<T, CommandError>;

pub trait Command: Send + Sync + fmt::Debug {
    fn execute(&mut self, doc: &mut DiagramDocument) -> CommandResult<()>;
    fn undo(&mut self, doc: &mut DiagramDocument)    -> CommandResult<()>;
    fn description(&self) -> &str;
}

pub struct History {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_size:   usize,
}

impl History {
    pub fn new(max_size: usize) -> Self {
        Self { undo_stack: Vec::new(), redo_stack: Vec::new(), max_size }
    }

    pub fn execute(&mut self, doc: &mut DiagramDocument, mut cmd: Box<dyn Command>) -> CommandResult<()> {
        cmd.execute(doc)?;
        self.redo_stack.clear();
        self.undo_stack.push(cmd);
        if self.undo_stack.len() > self.max_size { self.undo_stack.remove(0); }
        Ok(())
    }

    pub fn undo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        if let Some(mut cmd) = self.undo_stack.pop() {
            cmd.undo(doc)?;
            self.redo_stack.push(cmd);
            Ok(())
        } else {
            Err(CommandError::UndoFailed("Nothing to undo".into()))
        }
    }

    pub fn redo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        if let Some(mut cmd) = self.redo_stack.pop() {
            cmd.execute(doc)?;
            self.undo_stack.push(cmd);
            Ok(())
        } else {
            Err(CommandError::ExecutionFailed("Nothing to redo".into()))
        }
    }

    pub fn can_undo(&self) -> bool { !self.undo_stack.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo_stack.is_empty() }
}

impl Default for History {
    fn default() -> Self { Self::new(100) }
}
