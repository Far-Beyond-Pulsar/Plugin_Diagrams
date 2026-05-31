//! Top toolbar: undo/redo, select/pan, zoom label.

use gpui::*;
use parking_lot::RwLock;
use std::sync::Arc;
use ui::{button::{Button, ButtonVariants}, IconName, Theme};

use crate::state::DiagramDocument;
use crate::state::tool_state::ActiveTool;
use crate::state::History;

type DocArc = Arc<RwLock<DiagramDocument>>;

pub fn render_toolbar(doc: DocArc, history: &History, zoom: f32, theme: &Theme) -> impl IntoElement {
    let can_undo    = history.can_undo();
    let can_redo    = history.can_redo();
    let active_tool = doc.read().tool_state.active_tool.clone();
    let zoom_pct    = (zoom * 100.0).round() as u32;

    div()
        .flex().w_full().h(px(48.0))
        .bg(theme.sidebar.opacity(0.98))
        .border_b_1().border_color(theme.border.opacity(0.8))
        .items_center().px_2().gap_1()
        // Select / Pan
        .child(tool_btn_static("Select", IconName::DragHandGesture, "Select (V)",
            &active_tool == &ActiveTool::Select, doc.clone(), ActiveTool::Select))
        .child(tool_btn_static("Pan",    IconName::DragHandGesture, "Pan (H)",
            &active_tool == &ActiveTool::Pan,    doc.clone(), ActiveTool::Pan))
        .child(sep(theme))
        // Zoom
        .child(div().text_xs().text_color(theme.foreground.opacity(0.6)).px_2()
                    .child(format!("{}%", zoom_pct)))
}

fn tool_btn_static(id: &'static str, icon: IconName, tooltip: &str, is_active: bool, doc: DocArc, tool: ActiveTool) -> Button {
    let mut btn = Button::new(id).icon(icon).tooltip(tooltip);
    if is_active { btn = btn.primary(); }
    btn.on_click(move |_, _, _| { doc.write().tool_state.active_tool = tool.clone(); })
}

fn sep(theme: &Theme) -> impl IntoElement {
    div().w(px(1.0)).h(px(24.0)).bg(theme.border.opacity(0.5)).mx_1()
}
