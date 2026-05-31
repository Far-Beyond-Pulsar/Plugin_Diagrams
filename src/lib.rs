//! Plugin_Diagrams — draw.io-style interactive diagram editor.

mod plugin;
mod panel;
mod panels;
pub mod canvas;
pub mod elements;
pub mod state;
pub mod shape_def;
mod ui;

pub use plugin::DiagramsPlugin;
pub use panel::DiagramsEditorPanel;
