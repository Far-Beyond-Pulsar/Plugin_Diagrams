//! Diagram commands for undo/redo.
//! Each command stores pure data (no Arc). History passes &mut DiagramDocument.

use crate::elements::element::DiagramElement;
use super::history::{Command, CommandError, CommandResult};
use super::document::DiagramDocument;

// ── Add element ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct AddElementCommand {
    pub elem: DiagramElement,
}

impl Command for AddElementCommand {
    fn execute(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        doc.add(self.elem.clone());
        Ok(())
    }
    fn undo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        doc.remove(&self.elem.id);
        Ok(())
    }
    fn description(&self) -> &str { "Add Element" }
}

// ── Remove element ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct RemoveElementCommand {
    id:     String,
    backup: Option<DiagramElement>,
}

impl RemoveElementCommand {
    pub fn new(id: String) -> Self { Self { id, backup: None } }
}

impl Command for RemoveElementCommand {
    fn execute(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        self.backup = doc.remove(&self.id);
        Ok(())
    }
    fn undo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        if let Some(e) = self.backup.clone() { doc.add(e); Ok(()) }
        else { Err(CommandError::UndoFailed("No backup".into())) }
    }
    fn description(&self) -> &str { "Delete Element" }
}

// ── Move / resize ─────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct MoveBoundsCommand {
    /// (id, old_bounds, new_bounds)
    changes: Vec<(String, [f32; 4], [f32; 4])>,
}

impl MoveBoundsCommand {
    pub fn new(changes: Vec<(String, [f32; 4], [f32; 4])>) -> Self { Self { changes } }
}

impl Command for MoveBoundsCommand {
    fn execute(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        for (id, _, new) in &self.changes {
            if let Some(e) = doc.get_mut(id) { e.set_bounds(*new); }
        }
        Ok(())
    }
    fn undo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        for (id, old, _) in &self.changes {
            if let Some(e) = doc.get_mut(id) { e.set_bounds(*old); }
        }
        Ok(())
    }
    fn description(&self) -> &str { "Move" }
}

// ── Set param ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SetParamCommand {
    id:        String,
    old_elem:  Option<DiagramElement>,
    new_elem:  DiagramElement,
}

impl SetParamCommand {
    pub fn new(doc: &DiagramDocument, id: String, new_elem: DiagramElement) -> Self {
        let old_elem = doc.get(&id).cloned();
        Self { id, old_elem, new_elem }
    }
}

impl Command for SetParamCommand {
    fn execute(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        let ne = self.new_elem.clone();
        if let Some(e) = doc.get_mut(&self.id) { *e = ne; }
        Ok(())
    }
    fn undo(&mut self, doc: &mut DiagramDocument) -> CommandResult<()> {
        if let Some(oe) = &self.old_elem {
            let oe = oe.clone();
            if let Some(e) = doc.get_mut(&self.id) { *e = oe; }
        }
        Ok(())
    }
    fn description(&self) -> &str { "Edit Shape" }
}
