//! DiagramViewport — owns the WgpuSurface, History, and all interaction.
//!
//! Deadlock fix: History lives *here* (not inside DiagramDocument).
//! execute_cmd() temporarily moves the history out of self, calls execute
//! while holding the document write lock, then restores it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use gpui::*;
use parking_lot::RwLock;

use crate::elements::element::{
    DiagramElement, RESIZE_HANDLE_COUNT, resize_handle_position, resize_deltas,
};
use crate::elements::render::{render_element, render_rubber_band, render_connection_ports, RenderCtx, render_connector_shape};
use crate::shape_def::{ShapeLibrary, HandleAxis};
use crate::state::{
    DiagramDocument, History,
    commands::{AddElementCommand, MoveBoundsCommand, RemoveElementCommand, SetParamCommand},
    tool_state::ActiveTool,
};

use super::renderer::{DiagramRenderer, DiagramRenderInput};

const SURFACE_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const ZOOM_MIN: f32 = 0.05;
const ZOOM_MAX: f32 = 32.0;
const HANDLE_HIT_R: f32 = 7.0;

type DocArc = Arc<RwLock<DiagramDocument>>;

// ── Interaction state machine ─────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Interaction {
    Idle,
    RubberBand { start: [f32; 2], cur: [f32; 2] },
    Moving     { start: [f32; 2], orig: Vec<(String, f32, f32)> },
    Resizing   { id: String, handle: usize, start: [f32; 2], orig_bounds: [f32; 4] },
    ParamDrag  { id: String, handle_param: String, handle_axis: HandleAxis, handle_invert: bool, start: [f32; 2], orig_elem: DiagramElement },
    Creating   { start: [f32; 2], cur: [f32; 2] },
    Connecting { start: [f32; 2], cur: [f32; 2] },
}

// ── DiagramViewport ───────────────────────────────────────────────────────────

pub struct DiagramViewport {
    focus_handle: FocusHandle,
    pub document: DocArc,
    pub history:  History,
    pub lib:      ShapeLibrary,
    renderer:     Arc<Mutex<DiagramRenderer>>,
    surface:      Option<WgpuSurfaceHandle>,

    pub zoom: f32,
    pan:  Point<f32>,

    is_mid_panning:       bool,
    mid_win_start:        Option<Point<f32>>,
    mid_pan_at_start:     Point<f32>,

    interaction:    Interaction,
    cursor_win:     Option<Point<f32>>,
    hover_id:       Option<String>,

    element_origin: Rc<RefCell<Point<f32>>>,
    element_size:   Rc<RefCell<[f32; 2]>>,
}

impl DiagramViewport {
    pub fn new(document: DocArc, lib: ShapeLibrary, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle:  cx.focus_handle(),
            document,
            history:       History::default(),
            lib,
            renderer:      Arc::new(Mutex::new(DiagramRenderer::new())),
            surface:       None,
            zoom:          1.0,
            pan:           Point::new(64.0, 64.0),
            is_mid_panning:    false,
            mid_win_start:     None,
            mid_pan_at_start:  Point::default(),
            interaction:   Interaction::Idle,
            cursor_win:    None,
            hover_id:      None,
            element_origin: Rc::new(RefCell::new(Point::default())),
            element_size:   Rc::new(RefCell::new([0.0, 0.0])),
        }
    }

    // ── Execute a command — no deadlock ───────────────────────────────────────

    fn exec(&mut self, cmd: Box<dyn crate::state::Command>) {
        // Move history out, write-lock doc, execute, restore history.
        let mut hist = std::mem::take(&mut self.history);
        let mut doc  = self.document.write();
        let _ = hist.execute(&mut *doc, cmd);
        drop(doc);
        self.history = hist;
    }

    // ── Coordinate helpers ────────────────────────────────────────────────────

    fn to_local(&self, win: Point<f32>) -> Point<f32> {
        let o = *self.element_origin.borrow();
        Point::new(win.x - o.x, win.y - o.y)
    }

    fn canvas_of_local(&self, local: Point<f32>) -> [f32; 2] {
        [(local.x - self.pan.x) / self.zoom, (local.y - self.pan.y) / self.zoom]
    }

    fn canvas_of_win(&self, win: Point<f32>) -> [f32; 2] {
        self.canvas_of_local(self.to_local(win))
    }

    fn win_f32(p: Point<Pixels>) -> Point<f32> { Point::new(f32::from(p.x), f32::from(p.y)) }

    fn render_ctx(&self) -> RenderCtx {
        let o = *self.element_origin.borrow();
        RenderCtx { pan: [self.pan.x, self.pan.y], zoom: self.zoom, origin: [o.x, o.y] }
    }

    // ── Hit testing ───────────────────────────────────────────────────────────

    fn hit_test_interaction(&self, canvas_pos: [f32; 2]) -> HitKind {
        let doc = self.document.read();

        // Resize handles on selected elements.
        for id in &doc.selected_ids {
            if let Some(elem) = doc.get(id) {
                for h in 0..RESIZE_HANDLE_COUNT {
                    let hp = resize_handle_position(elem.bounds(), h);
                    if dist(canvas_pos, hp) <= HANDLE_HIT_R / self.zoom {
                        return HitKind::Resize(id.clone(), h);
                    }
                }
                for ([hx, hy], hd) in elem.param_handles(&self.lib) {
                    if dist(canvas_pos, [hx, hy]) <= HANDLE_HIT_R / self.zoom {
                        return HitKind::Param(
                            id.clone(),
                            hd.param.clone(),
                            hd.axis.clone(),
                            hd.invert,
                            elem.clone(),
                        );
                    }
                }
            }
        }
        if let Some(id) = doc.hit_test(canvas_pos[0], canvas_pos[1]) {
            return HitKind::Element(id.to_string());
        }
        HitKind::None
    }

    // ── Mouse handlers ────────────────────────────────────────────────────────

    fn left_down(&mut self, win: Point<f32>, shift: bool) {
        let cp   = self.canvas_of_win(win);
        let tool = self.document.read().tool_state.active_tool.clone();

        match tool {
            ActiveTool::Pan => {
                self.is_mid_panning    = true;
                self.mid_win_start     = Some(win);
                self.mid_pan_at_start  = self.pan;
            }
            ActiveTool::DrawConnector => {
                self.interaction = Interaction::Connecting { start: cp, cur: cp };
            }
            ActiveTool::DrawShape(_) => {
                self.interaction = Interaction::Creating { start: cp, cur: cp };
            }
            ActiveTool::Select => {
                match self.hit_test_interaction(cp) {
                    HitKind::Resize(id, h) => {
                        let ob = self.document.read().get(&id).map(|e| e.bounds()).unwrap_or_default();
                        self.interaction = Interaction::Resizing { id, handle: h, start: cp, orig_bounds: ob };
                    }
                    HitKind::Param(id, param, axis, invert, orig_elem) => {
                        self.interaction = Interaction::ParamDrag { id, handle_param: param, handle_axis: axis, handle_invert: invert, start: cp, orig_elem };
                    }
                    HitKind::Element(id) => {
                        {
                            let mut doc = self.document.write();
                            if shift { doc.toggle_select(id.clone()); }
                            else if !doc.is_selected(&id) { doc.select_one(id.clone()); }
                        }
                        let orig: Vec<_> = {
                            let doc = self.document.read();
                            doc.selected_ids.iter()
                                .filter_map(|sid| doc.get(sid).map(|e| (sid.clone(), e.x, e.y)))
                                .collect()
                        };
                        self.interaction = Interaction::Moving { start: cp, orig };
                    }
                    HitKind::None => {
                        if !shift { self.document.write().clear_selection(); }
                        self.interaction = Interaction::RubberBand { start: cp, cur: cp };
                    }
                }
            }
        }
    }

    fn mouse_move(&mut self, win: Point<f32>) {
        let cp = self.canvas_of_win(win);

        if self.is_mid_panning {
            if let Some(s) = self.mid_win_start {
                self.pan.x = self.mid_pan_at_start.x + (win.x - s.x);
                self.pan.y = self.mid_pan_at_start.y + (win.y - s.y);
            }
            return;
        }

        self.hover_id = self.document.read().hit_test(cp[0], cp[1]).map(|s| s.to_string());

        match self.interaction.clone() {
            Interaction::RubberBand { start, .. } => {
                self.interaction = Interaction::RubberBand { start, cur: cp };
            }
            Interaction::Moving { start, orig } => {
                let dx = cp[0] - start[0]; let dy = cp[1] - start[1];
                let mut doc = self.document.write();
                for (id, ox, oy) in &orig {
                    if let Some(e) = doc.get_mut(id) { e.x = ox + dx; e.y = oy + dy; }
                }
                self.interaction = Interaction::Moving { start, orig };
            }
            Interaction::Resizing { id, handle, start, orig_bounds } => {
                let dx = cp[0] - start[0]; let dy = cp[1] - start[1];
                let [fdx, fdy, fdw, fdh] = resize_deltas(handle);
                let mut doc = self.document.write();
                if let Some(e) = doc.get_mut(&id) {
                    e.x      = (orig_bounds[0] + fdx * dx).min(orig_bounds[0] + orig_bounds[2] - 10.0);
                    e.y      = (orig_bounds[1] + fdy * dy).min(orig_bounds[1] + orig_bounds[3] - 10.0);
                    e.width  = (orig_bounds[2] + fdw * dx).max(10.0);
                    e.height = (orig_bounds[3] + fdh * dy).max(10.0);
                }
                self.interaction = Interaction::Resizing { id, handle, start, orig_bounds };
            }
            Interaction::ParamDrag { id, handle_param, handle_axis, handle_invert, start, orig_elem } => {
                let dx = cp[0] - start[0]; let dy = cp[1] - start[1];
                let mut new_elem = orig_elem.clone();
                // Build a temporary HandleDef for apply_handle_drag
                let hd = crate::shape_def::HandleDef {
                    param:    handle_param.clone(),
                    cx_tmpl:  String::new(),
                    cy_tmpl:  String::new(),
                    axis:     handle_axis.clone(),
                    invert:   handle_invert,
                };
                new_elem.apply_handle_drag(&hd, dx, dy, &self.lib);
                let mut doc = self.document.write();
                if let Some(e) = doc.get_mut(&id) { *e = new_elem; }
                self.interaction = Interaction::ParamDrag { id, handle_param, handle_axis, handle_invert, start, orig_elem };
            }
            Interaction::Creating { start, .. } => {
                self.interaction = Interaction::Creating { start, cur: cp };
            }
            Interaction::Connecting { start, .. } => {
                self.interaction = Interaction::Connecting { start, cur: cp };
            }
            Interaction::Idle => {}
        }
    }

    fn left_up(&mut self, win: Point<f32>) {
        let cp           = self.canvas_of_win(win);
        let interaction  = std::mem::replace(&mut self.interaction, Interaction::Idle);
        self.is_mid_panning = false; self.mid_win_start = None;

        match interaction {
            Interaction::RubberBand { start, cur } => {
                let rx = start[0].min(cur[0]); let ry = start[1].min(cur[1]);
                let rw = (start[0] - cur[0]).abs(); let rh = (start[1] - cur[1]).abs();
                if rw > 4.0 || rh > 4.0 {
                    let ids = self.document.read().hit_rect(rx, ry, rw, rh);
                    self.document.write().select_set(ids);
                }
            }
            Interaction::Moving { start, orig } => {
                let dx = cp[0] - start[0]; let dy = cp[1] - start[1];
                if dx.abs() > 0.5 || dy.abs() > 0.5 {
                    let changes: Vec<_> = {
                        let doc = self.document.read();
                        orig.iter().filter_map(|(id, ox, oy)| {
                            doc.get(id).map(|e| (id.clone(), [*ox, *oy, e.width, e.height], e.bounds()))
                        }).collect()
                    };
                    if !changes.is_empty() {
                        self.exec(Box::new(MoveBoundsCommand::new(changes)));
                    }
                }
            }
            Interaction::Resizing { id, orig_bounds, .. } => {
                let new_bounds = self.document.read().get(&id).map(|e| e.bounds()).unwrap_or(orig_bounds);
                self.exec(Box::new(MoveBoundsCommand::new(vec![(id, orig_bounds, new_bounds)])));
            }
            Interaction::ParamDrag { id, orig_elem, .. } => {
                let new_elem = self.document.read().get(&id).cloned().unwrap_or(orig_elem.clone());
                let cmd = SetParamCommand::new(&self.document.read(), id.clone(), new_elem);
                self.exec(Box::new(cmd));
            }
            Interaction::Creating { start, cur } => {
                let x = start[0].min(cur[0]); let y = start[1].min(cur[1]);
                let w = (start[0] - cur[0]).abs().max(20.0);
                let h = (start[1] - cur[1]).abs().max(20.0);
                let shape_id = self.document.read().tool_state.active_tool.draw_shape_id()
                    .map(|s| s.to_string());
                if let Some(sid) = shape_id {
                    let elem = DiagramElement::new(&sid, x, y, w, h, &self.lib);
                    let elem_id = elem.id.clone();
                    self.exec(Box::new(AddElementCommand { elem }));
                    self.document.write().select_one(elem_id);
                    self.document.write().tool_state.active_tool = ActiveTool::Select;
                }
            }
            Interaction::Connecting { start, cur } => {
                let w = cur[0] - start[0]; let h = cur[1] - start[1];
                if (w * w + h * h).sqrt() > 10.0 {
                    let mut params = std::collections::HashMap::new();
                    params.insert("end_arrow".to_string(), 1.0);
                    params.insert("start_arrow".to_string(), 0.0);
                    let elem = DiagramElement {
                        id:       uuid::Uuid::new_v4().to_string(),
                        shape_id: "connector".to_string(),
                        x: start[0], y: start[1], width: w, height: h,
                        label:    String::new(),
                        style:    crate::elements::element::ElementStyle::default(),
                        params,
                        z_order:  0,
                    };
                    let eid = elem.id.clone();
                    self.exec(Box::new(AddElementCommand { elem }));
                    self.document.write().select_one(eid);
                    self.document.write().tool_state.active_tool = ActiveTool::Select;
                }
            }
            Interaction::Idle => {}
        }
    }

    // ── Canvas overlay (shapes + handles + overlays) ──────────────────────────

    fn shapes_overlay(&self, cx: &Context<Self>) -> impl IntoElement {
        let origin_rc   = self.element_origin.clone();
        let size_rc     = self.element_size.clone();
        let pan         = [self.pan.x, self.pan.y];
        let zoom        = self.zoom;
        let hover_id    = self.hover_id.clone();
        let interaction = self.interaction.clone();

        let elements    = self.document.read().elements_sorted().to_vec();
        let selected    = self.document.read().selected_ids.clone();
        let lib         = self.lib.clone();

        // Scale params by zoom before passing to renderer.
        let elements: Vec<_> = elements.into_iter().map(|mut e| {
            for v in e.params.values_mut() { *v *= zoom; }
            e
        }).collect();

        gpui::canvas(
            |_bounds, _win, _cx| {},
            move |bounds, _pre, window, _cx| {
                let ox = f32::from(bounds.origin.x);
                let oy = f32::from(bounds.origin.y);
                *origin_rc.borrow_mut() = Point::new(ox, oy);
                *size_rc.borrow_mut()   = [f32::from(bounds.size.width), f32::from(bounds.size.height)];

                let ctx = RenderCtx { pan, zoom, origin: [ox, oy] };

                for elem in &elements {
                    let is_sel = selected.contains(&elem.id);
                    let is_hov = hover_id.as_deref() == Some(&elem.id);
                    render_element(elem, &ctx, &lib, is_sel, is_hov, window);
                }

                // Connection ports on hovered non-connector.
                if let Some(ref hid) = hover_id {
                    if let Some(elem) = elements.iter().find(|e| &e.id == hid) {
                        if !elem.is_connector() {
                            render_connection_ports(elem, &ctx, window);
                        }
                    }
                }

                // Interaction preview overlays.
                match &interaction {
                    Interaction::RubberBand { start, cur } => {
                        let [s0, s1] = to_screen(*start, pan, zoom, [ox, oy]);
                        let [c0, c1] = to_screen(*cur,   pan, zoom, [ox, oy]);
                        let (rx, rw) = if c0 >= s0 { (s0, c0-s0) } else { (c0, s0-c0) };
                        let (ry, rh) = if c1 >= s1 { (s1, c1-s1) } else { (c1, s1-c1) };
                        render_rubber_band(rx, ry, rw, rh, window);
                    }
                    Interaction::Creating { start, cur } => {
                        let [s0, s1] = to_screen(*start, pan, zoom, [ox, oy]);
                        let [c0, c1] = to_screen(*cur,   pan, zoom, [ox, oy]);
                        let (rx, rw) = if c0 >= s0 { (s0, c0-s0) } else { (c0, s0-c0) };
                        let (ry, rh) = if c1 >= s1 { (s1, c1-s1) } else { (c1, s1-c1) };
                        window.paint_quad(PaintQuad {
                            bounds: Bounds { origin: point(px(rx), px(ry)), size: size(px(rw.max(2.0)), px(rh.max(2.0))) },
                            corner_radii:  Corners::all(px(0.0)),
                            background:    Hsla { h:0.56, s:0.6, l:0.65, a:0.2 }.into(),
                            border_widths: Edges::all(px(1.5)),
                            border_color:  Hsla { h:0.56, s:0.9, l:0.55, a:0.9 },
                            border_style:  BorderStyle::default(),
                        });
                    }
                    Interaction::Connecting { start, cur } => {
                        let [s0, s1] = to_screen(*start, pan, zoom, [ox, oy]);
                        let [c0, c1] = to_screen(*cur,   pan, zoom, [ox, oy]);
                        let mut sp = PathBuilder::stroke(px(2.0));
                        let _ = sp.move_to(point(px(s0), px(s1)));
                        let _ = sp.line_to(point(px(c0), px(c1)));
                        if let Ok(p) = sp.build() { window.paint_path(p, Hsla { h:0.56,s:0.9,l:0.45,a:0.9 }); }
                    }
                    _ => {}
                }
            },
        )
        .absolute().inset_0()
    }
}

fn to_screen(cp: [f32; 2], pan: [f32; 2], zoom: f32, origin: [f32; 2]) -> [f32; 2] {
    [origin[0] + cp[0] * zoom + pan[0], origin[1] + cp[1] * zoom + pan[1]]
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dx = a[0]-b[0]; let dy = a[1]-b[1]; (dx*dx+dy*dy).sqrt()
}

enum HitKind { Resize(String, usize), Param(String, String, HandleAxis, bool, DiagramElement), Element(String), None }

// ── GPUI impls ────────────────────────────────────────────────────────────────

impl Focusable for DiagramViewport {
    fn focus_handle(&self, _cx: &App) -> FocusHandle { self.focus_handle.clone() }
}
impl EventEmitter<()> for DiagramViewport {}

impl Render for DiagramViewport {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();

        if self.surface.is_none() {
            if let Some(s) = window.create_wgpu_surface(1920, 1080, SURFACE_FMT) {
                self.surface = Some(s);
            }
        }

        if let Some(ref surface) = self.surface {
            if !surface.is_resize_pending() {
                if let Some((view, (w, h))) = surface.back_view_with_size() {
                    let cs  = self.document.read().canvas_size;
                    let vps = *self.element_size.borrow();
                    let inp = DiagramRenderInput {
                        pan_offset: [self.pan.x, self.pan.y], zoom: self.zoom,
                        canvas_size: cs, viewport_size: vps,
                    };
                    if let Ok(mut r) = self.renderer.try_lock() {
                        r.render_frame(surface.device(), surface.queue(), &view, w, h, surface.format(), &inp);
                    }
                    drop(view); surface.swap_buffers();
                }
            }
        }

        let wgpu_elem: AnyElement = if let Some(ref s) = self.surface {
            wgpu_surface(s.clone()).defer_resize_until_mouse_up(true).absolute().inset_0().into_any_element()
        } else {
            div().absolute().inset_0().bg(rgba(0x1e1e23ff))
                .flex().items_center().justify_center()
                .child(div().text_color(rgba(0x888899ff)).text_sm().child("Initialising…"))
                .into_any_element()
        };

        let overlay = self.shapes_overlay(cx);

        div()
            .size_full().relative().overflow_hidden()
            .track_focus(&self.focus_handle)
            .key_context("DiagramViewport")
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _win, cx| {
                let k = &ev.keystroke.key;
                if k == "Delete" || k == "Backspace" {
                    let ids: Vec<_> = this.document.read().selected_ids.clone();
                    for id in ids {
                        this.exec(Box::new(RemoveElementCommand::new(id)));
                    }
                    this.document.write().clear_selection();
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Middle, cx.listener(|this, ev: &MouseDownEvent, _win, _cx| {
                let w = Self::win_f32(ev.position);
                this.is_mid_panning = true; this.mid_win_start = Some(w); this.mid_pan_at_start = this.pan;
            }))
            .on_mouse_up(MouseButton::Middle, cx.listener(|this, _ev: &MouseUpEvent, _win, _cx| {
                this.is_mid_panning = false; this.mid_win_start = None;
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _win, _cx| {
                this.left_down(Self::win_f32(ev.position), ev.modifiers.shift);
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, ev: &MouseUpEvent, _win, _cx| {
                this.left_up(Self::win_f32(ev.position));
            }))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _win, cx| {
                this.cursor_win = Some(Self::win_f32(ev.position));
                this.mouse_move(Self::win_f32(ev.position));
                cx.notify();
            }))
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _win, cx| {
                let w     = Self::win_f32(ev.position);
                let local = this.to_local(w);
                let focus = this.canvas_of_local(local);
                let raw: f32 = match ev.delta { ScrollDelta::Pixels(p) => p.y.into(), ScrollDelta::Lines(l) => l.y * 32.0 };
                let factor   = if raw < 0.0 { 1.1f32 } else { 0.9f32 };
                let new_zoom = (this.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
                this.pan.x   = local.x - focus[0] * new_zoom;
                this.pan.y   = local.y - focus[1] * new_zoom;
                this.zoom    = new_zoom;
                cx.notify();
            }))
            .child(wgpu_elem)
            .child(overlay)
    }
}
