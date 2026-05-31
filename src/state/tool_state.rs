//! Tool and interaction mode state.

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveTool {
    Select,
    Pan,
    /// Drawing a new shape; carries the shape_id string.
    DrawShape(String),
    DrawConnector,
}

impl ActiveTool {
    pub fn draw_shape_id(&self) -> Option<&str> {
        match self { Self::DrawShape(id) => Some(id.as_str()), _ => None }
    }
    pub fn is_draw(&self) -> bool {
        matches!(self, Self::DrawShape(_) | Self::DrawConnector)
    }
}

#[derive(Clone, Debug)]
pub struct ToolState {
    pub active_tool: ActiveTool,
}

impl Default for ToolState {
    fn default() -> Self { Self { active_tool: ActiveTool::Select } }
}
