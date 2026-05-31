//! Left shape palette — uniform grid of large preview buttons.
//! One button per shape definition loaded from SVG files.

use gpui::*;
use parking_lot::RwLock;
use std::sync::Arc;
use ui::Theme;

use crate::shape_def::{ShapeLibrary, ShapeDefinition, PathRole, SpecialRenderer, substitute, build_path};
use crate::state::DiagramDocument;
use crate::state::tool_state::ActiveTool;

type DocArc = Arc<RwLock<DiagramDocument>>;

pub fn render_shape_palette(doc: DocArc, lib: &ShapeLibrary, theme: &Theme) -> impl IntoElement {
    let active_tool = doc.read().tool_state.active_tool.clone();

    div()
        .flex().flex_col()
        .w(px(180.0)).h_full()
        .bg(theme.sidebar)
        .border_r_1().border_color(theme.border)
        // Header
        .child(
            div().flex().h(px(36.0)).px_2().items_center()
                 .border_b_1().border_color(theme.border.opacity(0.5))
                 .child(div().text_sm().font_weight(FontWeight::SEMIBOLD)
                             .text_color(theme.foreground).child("Shapes"))
        )
        // Scrollable grid
        .child(
            div().flex_1().overflow_hidden()
                 .p(px(6.0))
                 .flex().flex_wrap().gap(px(4.0))
                 .children(lib.shapes.iter().filter(|s| s.id != "connector").map(|shape| {
                     let is_active = active_tool == ActiveTool::DrawShape(shape.id.clone());
                     render_shape_button(shape, is_active, doc.clone(), theme)
                 }))
                 // Connector button at the bottom spanning full width
                 .child({
                     let conn_active = active_tool == ActiveTool::DrawConnector;
                     render_connector_button(conn_active, doc.clone(), theme)
                 })
        )
}

fn render_shape_button(
    shape:     &ShapeDefinition,
    is_active: bool,
    doc:       DocArc,
    theme:     &Theme,
) -> impl IntoElement {
    let bg = if is_active { theme.accent.opacity(0.2) } else { theme.background };
    let label_color = if is_active { theme.accent } else { theme.foreground.opacity(0.8) };

    let shape_id   = shape.id.clone();
    let shape_name = shape.name.clone();

    // Mini shape preview: render into a 64×40 canvas area.
    let preview = render_shape_preview(shape, theme, is_active);

    div()
        .w(px(82.0)).flex().flex_col().items_center()
        .rounded(px(6.0)).p(px(4.0)).gap(px(3.0))
        .bg(bg)
        .border_1().border_color(if is_active { theme.accent.opacity(0.6) } else { theme.border.opacity(0.3) })
        .hover(|s| s.bg(theme.background.blend(theme.foreground.opacity(0.1))))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, _, _| {
            doc.write().tool_state.active_tool = ActiveTool::DrawShape(shape_id.clone());
        })
        .child(preview)
        .child(
            div().w_full().text_center()
                 .text_xs().text_color(label_color)
                 .truncate().child(shape_name)
        )
}

fn render_shape_preview(shape: &ShapeDefinition, theme: &Theme, is_active: bool) -> impl IntoElement {
    let fill_hsla   = if is_active {
        Hsla { h: theme.accent.h, s: 0.4, l: 0.75, a: 1.0 }
    } else {
        Hsla { h: 0.592, s: 0.35, l: 0.72, a: 1.0 }
    };
    let stroke_hsla = if is_active {
        Hsla { h: theme.accent.h, s: 0.7, l: 0.45, a: 1.0 }
    } else {
        Hsla { h: 0.592, s: 0.5,  l: 0.42, a: 1.0 }
    };

    let paths     = shape.paths.clone();
    let defaults  = shape.default_params();
    let special   = shape.special_renderer.clone();

    gpui::canvas(
        |_bounds, _win, _cx| {},
        move |bounds, _pre, window, _cx| {
            let bx = f32::from(bounds.origin.x);
            let by_ = f32::from(bounds.origin.y);
            let bw = f32::from(bounds.size.width);
            let bh = f32::from(bounds.size.height);

            // Add small margin.
            let margin = 6.0f32;
            let sw = bw - margin * 2.0;
            let sh = bh - margin * 2.0;
            let ox = bx + margin;
            let oy = by_ + margin;

            match special {
                SpecialRenderer::Table => render_preview_table(ox, oy, sw, sh, fill_hsla, stroke_hsla, &defaults, window),
                SpecialRenderer::Text  => render_preview_text(ox, oy, sw, sh, stroke_hsla, window),
                SpecialRenderer::Connector | SpecialRenderer::None => {
                    // Build vars: w/h = preview size, params at defaults (no zoom scaling for preview).
                    let mut vars = std::collections::HashMap::new();
                    vars.insert("w".to_string(), sw);
                    vars.insert("h".to_string(), sh);
                    for (k, v) in &defaults { vars.insert(k.clone(), *v * (sw / 100.0).max(0.3)); }

                    for pd in &paths {
                        let d_sub = substitute(&pd.d_tmpl, &vars);
                        match pd.role {
                            PathRole::Body => {
                                let mut fp = PathBuilder::fill();
                                fp.translate(point(px(ox), px(oy)));
                                build_path(&d_sub, &mut fp);
                                if let Ok(p) = fp.build() { window.paint_path(p, fill_hsla); }

                                let mut sp = PathBuilder::stroke(px(1.5));
                                sp.translate(point(px(ox), px(oy)));
                                build_path(&d_sub, &mut sp);
                                if let Ok(p) = sp.build() { window.paint_path(p, stroke_hsla); }
                            }
                            PathRole::Stroke => {
                                let mut sp = PathBuilder::stroke(px(1.5));
                                sp.translate(point(px(ox), px(oy)));
                                build_path(&d_sub, &mut sp);
                                if let Ok(p) = sp.build() { window.paint_path(p, stroke_hsla); }
                            }
                        }
                    }
                }
            }
        },
    )
    .w(px(72.0)).h(px(46.0))
    .bg(Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.06 })
    .rounded(px(3.0))
}

fn render_preview_table(ox: f32, oy: f32, w: f32, h: f32, fill: Hsla, stroke: Hsla, params: &std::collections::HashMap<String, f32>, window: &mut Window) {
    let rows = params.get("rows").copied().unwrap_or(3.0).round().max(1.0) as u32;
    let cols = params.get("cols").copied().unwrap_or(3.0).round().max(1.0) as u32;
    let row_h = h / rows as f32;
    let col_w = w / cols as f32;

    // Header fill
    window.paint_quad(PaintQuad {
        bounds: Bounds { origin: point(px(ox), px(oy)), size: size(px(w), px(row_h)) },
        corner_radii: Corners::all(px(0.0)),
        background: Hsla { h: fill.h, s: fill.s, l: fill.l * 0.75, a: 1.0 }.into(),
        border_widths: Edges::all(px(0.0)),
        border_color: stroke, border_style: BorderStyle::default(),
    });
    // Body fill
    window.paint_quad(PaintQuad {
        bounds: Bounds { origin: point(px(ox), px(oy + row_h)), size: size(px(w), px(h - row_h)) },
        corner_radii: Corners::all(px(0.0)),
        background: fill.into(),
        border_widths: Edges::all(px(0.0)),
        border_color: stroke, border_style: BorderStyle::default(),
    });
    // Grid lines
    for r in 0..=rows {
        let ly = oy + r as f32 * row_h;
        let mut sp = PathBuilder::stroke(px(1.0));
        let _ = sp.move_to(point(px(ox), px(ly))); let _ = sp.line_to(point(px(ox+w), px(ly)));
        if let Ok(p) = sp.build() { window.paint_path(p, stroke); }
    }
    for c in 0..=cols {
        let lx = ox + c as f32 * col_w;
        let mut sp = PathBuilder::stroke(px(1.0));
        let _ = sp.move_to(point(px(lx), px(oy))); let _ = sp.line_to(point(px(lx), px(oy+h)));
        if let Ok(p) = sp.build() { window.paint_path(p, stroke); }
    }
}

fn render_preview_text(ox: f32, oy: f32, w: f32, h: f32, stroke: Hsla, window: &mut Window) {
    // Draw a "T" shape.
    let cx = ox + w * 0.5;
    let top = oy + h * 0.15;
    let bot = oy + h * 0.85;
    for (x0, y0, x1, y1) in [(ox + w*0.15, top, ox + w*0.85, top), (cx, top, cx, bot)] {
        let mut sp = PathBuilder::stroke(px(2.0));
        let _ = sp.move_to(point(px(x0), px(y0))); let _ = sp.line_to(point(px(x1), px(y1)));
        if let Ok(p) = sp.build() { window.paint_path(p, stroke); }
    }
}

fn render_connector_button(is_active: bool, doc: DocArc, theme: &Theme) -> impl IntoElement {
    let bg = if is_active { theme.accent.opacity(0.2) } else { theme.background };
    let label_color = if is_active { theme.accent } else { theme.foreground.opacity(0.8) };

    let preview = gpui::canvas(
        |_bounds, _win, _cx| {},
        move |bounds, _pre, window, _cx| {
            let bx  = f32::from(bounds.origin.x) + 6.0;
            let by_ = f32::from(bounds.origin.y) + f32::from(bounds.size.height) * 0.5;
            let bx2 = bx + f32::from(bounds.size.width) - 12.0;

            let color = Hsla { h: 0.592, s: 0.5, l: 0.42, a: 1.0 };
            let mut sp = PathBuilder::stroke(px(1.5));
            let _ = sp.move_to(point(px(bx), px(by_)));
            let _ = sp.line_to(point(px(bx2), px(by_)));
            if let Ok(p) = sp.build() { window.paint_path(p, color); }

            // Arrow head
            let sz = 6.0f32;
            let mut fp = PathBuilder::fill();
            let _ = fp.move_to(point(px(bx2), px(by_)));
            let _ = fp.line_to(point(px(bx2 - sz), px(by_ - sz * 0.5)));
            let _ = fp.line_to(point(px(bx2 - sz), px(by_ + sz * 0.5)));
            let _ = fp.close();
            if let Ok(p) = fp.build() { window.paint_path(p, color); }
        },
    ).w(px(72.0)).h(px(46.0)).bg(Hsla { h:0.0,s:0.0,l:1.0,a:0.06 }).rounded(px(3.0));

    div()
        .w_full().flex().flex_col().items_center().rounded(px(6.0)).p(px(4.0)).gap(px(3.0))
        .bg(bg).border_1().border_color(if is_active { theme.accent.opacity(0.6) } else { theme.border.opacity(0.3) })
        .hover(|s| s.bg(theme.background.blend(theme.foreground.opacity(0.1))))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, move |_, _, _| {
            doc.write().tool_state.active_tool = ActiveTool::DrawConnector;
        })
        .child(preview)
        .child(div().w_full().text_center().text_xs().text_color(label_color).child("Connector"))
}
