//! DiagramsEditorPanel — top-level docking panel.

use gpui::*;
use parking_lot::RwLock;
use std::sync::Arc;
use ui::{dock::{Panel, PanelEvent}, ActiveTheme, IconName};

use crate::canvas::DiagramViewport;
use crate::panels::render_properties_panel;
use crate::shape_def::ShapeLibrary;
use crate::state::DiagramDocument;
use crate::ui::shape_palette::render_shape_palette;
use crate::ui::toolbar::render_toolbar;

pub struct DiagramsEditorPanel {
    focus_handle: FocusHandle,
    pub document: Arc<RwLock<DiagramDocument>>,
    pub lib:      ShapeLibrary,
    canvas:       Entity<DiagramViewport>,
}

impl DiagramsEditorPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let lib      = ShapeLibrary::load_builtins();
        let document = Arc::new(RwLock::new(DiagramDocument::new(1920.0, 1080.0)));
        let lib2     = lib.clone();
        let canvas   = cx.new(|cx| DiagramViewport::new(document.clone(), lib2, cx));
        Self { focus_handle: cx.focus_handle(), document, lib, canvas }
    }
}

impl Panel for DiagramsEditorPanel {
    fn panel_name(&self) -> &'static str { "diagrams_editor" }
    fn title(&self, _window: &Window, _cx: &App) -> AnyElement { "Diagram Editor".into_any_element() }
    fn tab_icon(&self, _cx: &App) -> Option<IconName> { Some(IconName::EditPencil) }
}

impl EventEmitter<PanelEvent> for DiagramsEditorPanel {}
impl Focusable for DiagramsEditorPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle { self.focus_handle.clone() }
}

impl Render for DiagramsEditorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let zoom  = cx.read_entity(&self.canvas, |vp, _| vp.zoom);
        let hist  = cx.read_entity(&self.canvas, |vp, _| {
            // We can't move history out; pass can_undo/redo booleans.
            (vp.history.can_undo(), vp.history.can_redo())
        });

        // Build a minimal History-like view for the toolbar.
        struct HistView(bool, bool);
        impl HistView {
            fn can_undo(&self) -> bool { self.0 }
            fn can_redo(&self) -> bool { self.1 }
        }
        let hv = HistView(hist.0, hist.1);

        // Toolbar needs to trigger undo/redo on the canvas entity.
        let canvas_undo = self.canvas.clone();
        let canvas_redo = self.canvas.clone();

        div()
            .size_full().flex().flex_col().bg(theme.background)
            // Toolbar
            .child(
                div().flex().w_full().h(px(48.0))
                     .bg(theme.sidebar.opacity(0.98))
                     .border_b_1().border_color(theme.border.opacity(0.8))
                     .items_center().px_2().gap_1()
                     // Undo
                     .child({
                         use ui::button::{Button, ButtonVariants};
                         use ui::IconName;
                         let mut btn = Button::new("undo").icon(IconName::Undo).tooltip("Undo (Ctrl+Z)");
                         if hv.can_undo() {
                             btn = btn.on_click(move |_, _, cx| {
                                 canvas_undo.update(cx, |vp, cx| {
                                     let mut hist = std::mem::take(&mut vp.history);
                                     let mut doc  = vp.document.write();
                                     let _ = hist.undo(&mut *doc);
                                     drop(doc);
                                     vp.history = hist;
                                     cx.notify();
                                 });
                             });
                         }
                         btn
                     })
                     // Redo
                     .child({
                         use ui::button::{Button, ButtonVariants};
                         use ui::IconName;
                         let mut btn = Button::new("redo").icon(IconName::Redo).tooltip("Redo (Ctrl+Shift+Z)");
                         if hv.can_redo() {
                             btn = btn.on_click(move |_, _, cx| {
                                 canvas_redo.update(cx, |vp, cx| {
                                     let mut hist = std::mem::take(&mut vp.history);
                                     let mut doc  = vp.document.write();
                                     let _ = hist.redo(&mut *doc);
                                     drop(doc);
                                     vp.history = hist;
                                     cx.notify();
                                 });
                             });
                         }
                         btn
                     })
                     .child(div().w(px(1.0)).h(px(24.0)).bg(theme.border.opacity(0.5)).mx_1())
                     // Select / Pan toggle
                     .child({
                         use ui::button::{Button, ButtonVariants};
                         use ui::IconName;
                         use crate::state::tool_state::ActiveTool;
                         let is_sel = self.document.read().tool_state.active_tool == ActiveTool::Select;
                         let d = self.document.clone();
                         let mut btn = Button::new("tool-select").icon(IconName::DragHandGesture).tooltip("Select (V)");
                         if is_sel { btn = btn.primary(); }
                         btn.on_click(move |_,_,_| { d.write().tool_state.active_tool = ActiveTool::Select; })
                     })
                     .child({
                         use ui::button::{Button, ButtonVariants};
                         use ui::IconName;
                         use crate::state::tool_state::ActiveTool;
                         let is_pan = self.document.read().tool_state.active_tool == ActiveTool::Pan;
                         let d = self.document.clone();
                         let mut btn = Button::new("tool-pan").icon(IconName::DragHandGesture).tooltip("Pan (H)");
                         if is_pan { btn = btn.primary(); }
                         btn.on_click(move |_,_,_| { d.write().tool_state.active_tool = ActiveTool::Pan; })
                     })
                     .child(div().w(px(1.0)).h(px(24.0)).bg(theme.border.opacity(0.5)).mx_1())
                     // Zoom label
                     .child(div().text_xs().text_color(theme.foreground.opacity(0.6)).px_2()
                                 .child(format!("{}%", (zoom * 100.0).round() as u32)))
            )
            // Body
            .child(
                div().flex().flex_1().min_h_0()
                     .child(render_shape_palette(self.document.clone(), &self.lib, theme))
                     .child(div().flex_1().min_w_0().relative().child(self.canvas.clone()))
                     .child(render_properties_panel(self.document.clone(), &self.lib, theme))
            )
    }
}
