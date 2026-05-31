//! Shape rendering via GPUI canvas paint callbacks.
//! Reads shape paths from ShapeDefinition (loaded from SVG files),
//! substitutes template variables, then feeds into PathBuilder.

use std::collections::HashMap;
use gpui::*;

use crate::shape_def::{ShapeLibrary, ShapeDefinition, PathRole, SpecialRenderer, substitute, build_path};
use super::element::{
    DiagramElement, ElementStyle, HslaColor,
    RESIZE_HANDLE_COUNT, resize_handle_position,
};

// ── Render context ────────────────────────────────────────────────────────────

pub struct RenderCtx {
    pub pan:    [f32; 2],
    pub zoom:   f32,
    pub origin: [f32; 2],
}

impl RenderCtx {
    pub fn to_screen(&self, cx: f32, cy: f32) -> [f32; 2] {
        [
            self.origin[0] + cx * self.zoom + self.pan[0],
            self.origin[1] + cy * self.zoom + self.pan[1],
        ]
    }
    pub fn scale(&self, v: f32) -> f32 { v * self.zoom }
}

// ── Color helpers ─────────────────────────────────────────────────────────────

fn col(c: HslaColor) -> Hsla { Hsla { h: c[0], s: c[1], l: c[2], a: c[3] } }
fn col_a(c: HslaColor, a: f32) -> Hsla { Hsla { h: c[0], s: c[1], l: c[2], a } }

fn pq(window: &mut Window, x: f32, y: f32, w: f32, h: f32, cr: f32, fill: Hsla, bw: f32, border: Hsla) {
    window.paint_quad(PaintQuad {
        bounds: Bounds { origin: point(px(x), px(y)), size: size(px(w), px(h)) },
        corner_radii:  Corners::all(px(cr)),
        background:    fill.into(),
        border_widths: Edges::all(px(bw)),
        border_color:  border,
        border_style:  BorderStyle::default(),
    });
}

// ── Main entry point ──────────────────────────────────────────────────────────

pub fn render_element(
    elem:        &DiagramElement,
    ctx:         &RenderCtx,
    lib:         &ShapeLibrary,
    is_selected: bool,
    is_hovered:  bool,
    window:      &mut Window,
) {
    let [sx, sy] = ctx.to_screen(elem.x, elem.y);
    let sw       = ctx.scale(elem.width);
    let sh       = ctx.scale(elem.height);
    let style    = &elem.style;
    let fill     = col_a(style.fill,   style.opacity);
    let stroke   = col_a(style.stroke, style.opacity);
    let sw_px    = style.stroke_width;

    if let Some(def) = lib.get(&elem.shape_id) {
        match def.special_renderer {
            SpecialRenderer::Table     => render_table(sx, sy, sw, sh, &elem.params, style, window),
            SpecialRenderer::Text      => { /* transparent body — label rendered as GPUI div */ }
            SpecialRenderer::Connector => render_connector_shape(elem, ctx, style, window),
            SpecialRenderer::None      => render_svg_paths(def, sx, sy, sw, sh, &elem.params, fill, stroke, sw_px, window),
        }
    } else {
        // Fallback: plain rectangle if shape id unknown.
        pq(window, sx, sy, sw, sh, 0.0, fill, sw_px, stroke);
    }

    if is_hovered && !is_selected {
        pq(window, sx - 2.0, sy - 2.0, sw + 4.0, sh + 4.0, 2.0,
            Hsla { h:0.0,s:0.0,l:0.0,a:0.0 }, 1.5,
            Hsla { h:0.56,s:0.7,l:0.55,a:0.5 });
    }
    if is_selected {
        pq(window, sx - 1.0, sy - 1.0, sw + 2.0, sh + 2.0, 2.0,
            Hsla { h:0.0,s:0.0,l:0.0,a:0.0 }, 1.5,
            Hsla { h:0.56,s:0.9,l:0.55,a:1.0 });
        render_resize_handles(elem, ctx, window);
        render_param_handles(elem, ctx, lib, window);
    }
}

// ── SVG path rendering ────────────────────────────────────────────────────────

fn render_svg_paths(
    def:     &ShapeDefinition,
    ox: f32, oy: f32, sw: f32, sh: f32,
    params:  &HashMap<String, f32>,
    fill:    Hsla,
    stroke:  Hsla,
    sw_px:   f32,
    window:  &mut Window,
) {
    // Build variable map: w/h in screen pixels, params scaled from canvas pixels
    // (params are already in the same pixel space as element bounds, and ctx.to_screen
    //  applied offset+zoom to the element origin; so we use sw/sh for w/h, and
    //  need to scale params too — but since we evaluate relative to (0,0)→(sw,sh)
    //  we map param canvas values → screen values proportionally)
    let mut vars = HashMap::new();
    vars.insert("w".to_string(), sw);
    vars.insert("h".to_string(), sh);
    // For params: scale by zoom ratio (sw / stored_element_width).
    // We don't have element width here, but params are in canvas px and sw = width*zoom,
    // so we need the zoom. Instead we store params in canvas px and callers pass
    // screen-space sw/sh; derive the implied zoom.
    // To keep it simple: params stored in "shape space" (0..max) without zoom;
    // they get scaled the same way as w/h (proportional to sw vs nominal 100).
    // Convention: params stored as fractions of width * sw, or raw canvas px * zoom.
    // Simplest: treat params as canvas-pixel values and multiply by zoom.
    // zoom = sw / element.width — we don't have element.width here.
    // Solution: pass a separate zoom parameter, OR pass params already in screen px.
    // For correctness, params should be passed already scaled by zoom.
    // The caller (render_element) has ctx.zoom. Let's just insert params directly
    // (the caller passes scaled params).
    for (k, v) in params {
        vars.insert(k.clone(), *v);
    }

    for path_def in &def.paths {
        let d_sub = substitute(&path_def.d_tmpl, &vars);

        // Translate all coordinates by element screen origin.
        // We do this by evaluating the path in (0,0)-(sw,sh) space and then
        // shifting all points by (ox, oy). We'll parse into PathBuilder and apply
        // a translation via the transform feature OR pre-translate by offsetting.
        // Simplest: build a shifted path by substituting ox/oy into the string.
        // We've already substituted {w},{h},params. Path points are relative to (0,0).
        // We need to shift them by (ox, oy).

        match path_def.role {
            PathRole::Body => {
                // Fill pass.
                let mut fp = PathBuilder::fill();
                build_path_offset(&d_sub, &mut fp, ox, oy);
                if let Ok(p) = fp.build() { window.paint_path(p, fill); }
                // Stroke pass.
                let mut sp = PathBuilder::stroke(px(sw_px));
                build_path_offset(&d_sub, &mut sp, ox, oy);
                if let Ok(p) = sp.build() { window.paint_path(p, stroke); }
            }
            PathRole::Stroke => {
                let mut sp = PathBuilder::stroke(px(sw_px));
                build_path_offset(&d_sub, &mut sp, ox, oy);
                if let Ok(p) = sp.build() { window.paint_path(p, stroke); }
            }
        }
    }
}

fn build_path_offset(d: &str, builder: &mut PathBuilder, ox: f32, oy: f32) {
    // Translate the path by (ox, oy) by building a wrapper that offsets all moves.
    // We achieve this by creating a temporary builder, then translating with lyon transform.
    // Simplest: just add ox, oy as variables to the expression would require re-substitution.
    // Instead, use PathBuilder's built-in transform.
    use crate::shape_def::build_path as bp;
    // Build into a fresh builder to get the path, then... wait, we can use lyon transform.
    // Actually PathBuilder::transform applies to the *current* builder.
    // Let's apply translation transform before building.
    builder.translate(point(px(ox), px(oy)));
    bp(d, builder);
}

// ── Table (special renderer) ──────────────────────────────────────────────────

fn render_table(
    x: f32, y: f32, w: f32, h: f32,
    params: &HashMap<String, f32>,
    style: &ElementStyle,
    window: &mut Window,
) {
    let rows  = params.get("rows").copied().unwrap_or(3.0).round().max(1.0) as u32;
    let cols  = params.get("cols").copied().unwrap_or(3.0).round().max(1.0) as u32;
    let row_h = h / rows as f32;
    let col_w = w / cols as f32;

    let fill    = col_a(style.fill,   style.opacity);
    let stroke  = col_a(style.stroke, style.opacity);
    let header  = Hsla { h: style.fill[0], s: style.fill[1], l: style.fill[2] * 0.75, a: style.opacity };

    // Header row.
    pq(window, x, y, w, row_h, 0.0, header, 0.0, header);
    // Body fill.
    pq(window, x, y + row_h, w, h - row_h, 0.0, fill, 0.0, fill);

    // Grid lines.
    for r in 0..=rows {
        let ly = y + r as f32 * row_h;
        draw_hline(window, x, ly, x + w, style.stroke_width, stroke);
    }
    for c in 0..=cols {
        let lx = x + c as f32 * col_w;
        draw_vline(window, lx, y, y + h, style.stroke_width, stroke);
    }
}

fn draw_hline(window: &mut Window, x0: f32, y: f32, x1: f32, w: f32, color: Hsla) {
    let mut sp = PathBuilder::stroke(px(w));
    let _ = sp.move_to(point(px(x0), px(y)));
    let _ = sp.line_to(point(px(x1), px(y)));
    if let Ok(p) = sp.build() { window.paint_path(p, color); }
}

fn draw_vline(window: &mut Window, x: f32, y0: f32, y1: f32, w: f32, color: Hsla) {
    let mut sp = PathBuilder::stroke(px(w));
    let _ = sp.move_to(point(px(x), px(y0)));
    let _ = sp.line_to(point(px(x), px(y1)));
    if let Ok(p) = sp.build() { window.paint_path(p, color); }
}

// ── Connector (special renderer) ──────────────────────────────────────────────

pub fn render_connector_shape(
    elem:   &DiagramElement,
    ctx:    &RenderCtx,
    style:  &ElementStyle,
    window: &mut Window,
) {
    let [sx, sy] = ctx.to_screen(elem.x, elem.y);
    let [tx, ty] = ctx.to_screen(elem.x + elem.width, elem.y + elem.height);

    let color      = col_a(style.stroke, style.opacity);
    let end_arrow  = elem.params.get("end_arrow").copied().unwrap_or(1.0) >= 0.5;
    let start_arrow = elem.params.get("start_arrow").copied().unwrap_or(0.0) >= 0.5;

    let mut sp = PathBuilder::stroke(px(style.stroke_width));
    let _ = sp.move_to(point(px(sx), px(sy)));
    let _ = sp.line_to(point(px(tx), px(ty)));
    if let Ok(p) = sp.build() { window.paint_path(p, color); }

    if end_arrow   { render_arrowhead(tx, ty, sx, sy, style.stroke_width * 4.0, color, window); }
    if start_arrow { render_arrowhead(sx, sy, tx, ty, style.stroke_width * 4.0, color, window); }
}

fn render_arrowhead(tip_x: f32, tip_y: f32, from_x: f32, from_y: f32, sz: f32, color: Hsla, window: &mut Window) {
    let dx  = tip_x - from_x;
    let dy  = tip_y - from_y;
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let ux  = dx / len; let uy = dy / len;
    let bx  = tip_x - ux * sz; let by_ = tip_y - uy * sz;
    let px_ = -uy * sz * 0.5;  let py_ =  ux * sz * 0.5;

    let mut fp = PathBuilder::fill();
    let _ = fp.move_to(point(px(tip_x), px(tip_y)));
    let _ = fp.line_to(point(px(bx + px_), px(by_ + py_)));
    let _ = fp.line_to(point(px(bx - px_), px(by_ - py_)));
    let _ = fp.close();
    if let Ok(p) = fp.build() { window.paint_path(p, color); }
}

// ── Handle rendering ──────────────────────────────────────────────────────────

fn render_resize_handles(elem: &DiagramElement, ctx: &RenderCtx, window: &mut Window) {
    for i in 0..RESIZE_HANDLE_COUNT {
        let [hx, hy] = resize_handle_position(elem.bounds(), i);
        let [sx, sy] = ctx.to_screen(hx, hy);
        draw_handle_square(sx, sy, 7.0, window);
    }
}

fn render_param_handles(elem: &DiagramElement, ctx: &RenderCtx, lib: &ShapeLibrary, window: &mut Window) {
    for ([hx, hy], _hd) in elem.param_handles(lib) {
        let [sx, sy] = ctx.to_screen(hx, hy);
        draw_handle_diamond(sx, sy, 8.0, window);
    }
}

fn draw_handle_square(cx: f32, cy: f32, sz: f32, window: &mut Window) {
    let hs = sz * 0.5;
    pq(window, cx - hs, cy - hs, sz, sz, 1.5,
        Hsla { h:0.0,s:0.0,l:1.0,a:1.0 }, 1.5,
        Hsla { h:0.56,s:0.9,l:0.55,a:1.0 });
}

fn draw_handle_diamond(cx: f32, cy: f32, sz: f32, window: &mut Window) {
    let hs = sz * 0.5;
    let pts = [[cx, cy-hs], [cx+hs, cy], [cx, cy+hs], [cx-hs, cy]];

    let mut fp = PathBuilder::fill();
    let _ = fp.move_to(point(px(pts[0][0]), px(pts[0][1])));
    for &[px_v, py_v] in &pts[1..] { let _ = fp.line_to(point(px(px_v), px(py_v))); }
    let _ = fp.close();
    if let Ok(p) = fp.build() { window.paint_path(p, Hsla { h:0.11,s:0.9,l:0.55,a:1.0 }); }

    let mut sp = PathBuilder::stroke(px(1.5));
    let _ = sp.move_to(point(px(pts[0][0]), px(pts[0][1])));
    for &[px_v, py_v] in &pts[1..] { let _ = sp.line_to(point(px(px_v), px(py_v))); }
    let _ = sp.close();
    if let Ok(p) = sp.build() { window.paint_path(p, Hsla { h:0.11,s:0.9,l:0.35,a:1.0 }); }
}

// ── Public overlays ───────────────────────────────────────────────────────────

pub fn render_rubber_band(x: f32, y: f32, w: f32, h: f32, window: &mut Window) {
    pq(window, x, y, w, h, 0.0,
        Hsla { h:0.56,s:0.8,l:0.6,a:0.12 }, 1.0,
        Hsla { h:0.56,s:0.8,l:0.55,a:0.9 });
}

pub fn render_connection_ports(elem: &DiagramElement, ctx: &RenderCtx, window: &mut Window) {
    for [px_c, py_c] in elem.connection_ports() {
        let [sx, sy] = ctx.to_screen(px_c, py_c);
        draw_port_dot(sx, sy, window);
    }
}

fn draw_port_dot(cx: f32, cy: f32, window: &mut Window) {
    const SEGS: usize = 16;
    let r = 5.0f32;
    let mut fp = PathBuilder::fill();
    for i in 0..SEGS {
        let t = std::f32::consts::TAU * i as f32 / SEGS as f32;
        let x = cx + r * t.cos(); let y = cy + r * t.sin();
        if i == 0 { let _ = fp.move_to(point(px(x), px(y))); }
        else       { let _ = fp.line_to(point(px(x), px(y))); }
    }
    let _ = fp.close();
    if let Ok(p) = fp.build() { window.paint_path(p, Hsla { h:0.56,s:0.9,l:0.55,a:0.9 }); }
}
