//! Right properties panel — shows element style + shape-specific params.

use gpui::*;
use parking_lot::RwLock;
use std::sync::Arc;
use ui::{button::Button, Theme};

use crate::elements::element::{DiagramElement, HslaColor};
use crate::shape_def::ShapeLibrary;
use crate::state::DiagramDocument;

type DocArc = Arc<RwLock<DiagramDocument>>;

pub fn render_properties_panel(doc_arc: DocArc, lib: &ShapeLibrary, theme: &Theme) -> impl IntoElement {
    let doc          = doc_arc.read();
    let sel_ids      = doc.selected_ids.clone();
    let elem         = sel_ids.first().and_then(|id| doc.get(id)).cloned();
    let elem_count   = doc.elements.len();
    let canvas_size  = doc.canvas_size;
    drop(doc);

    div()
        .flex().flex_col().w(px(240.0)).h_full()
        .bg(theme.sidebar).border_l_1().border_color(theme.border)
        .child(section_header_plain("Properties", theme))
        .child(
            if let Some(elem) = elem {
                let def = lib.get(&elem.shape_id);
                div().flex().flex_col()
                    .child(section_header(def.map(|d| d.name.as_str()).unwrap_or("Unknown"), theme))
                    // Label
                    .child(prop_row("Label", theme)
                        .child(div().flex_1().text_xs().text_color(theme.foreground).truncate()
                                   .child(if elem.label.is_empty() { "(none)".to_string() } else { elem.label.clone() })))
                    // Fill swatch
                    .child(prop_row("Fill", theme)
                        .child(color_swatch(elem.style.fill))
                        .child(div().text_xs().text_color(theme.foreground.opacity(0.5))
                                   .child(hsla_text(elem.style.fill))))
                    // Stroke swatch
                    .child(prop_row("Stroke", theme)
                        .child(color_swatch(elem.style.stroke)))
                    // Stroke width
                    .child(prop_row("S.Width", theme)
                        .child(val_label(format!("{:.1}", elem.style.stroke_width), theme))
                        .child(nudge("+", { let d = doc_arc.clone(); let id = elem.id.clone(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { e.style.stroke_width = (e.style.stroke_width + 0.5).min(20.0); } } }))
                        .child(nudge("−", { let d = doc_arc.clone(); let id = elem.id.clone(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { e.style.stroke_width = (e.style.stroke_width - 0.5).max(0.5); } } })))
                    // Opacity
                    .child(prop_row("Opacity", theme)
                        .child(val_label(format!("{:.0}%", elem.style.opacity * 100.0), theme))
                        .child(nudge("+", { let d = doc_arc.clone(); let id = elem.id.clone(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { e.style.opacity = (e.style.opacity + 0.05).min(1.0); } } }))
                        .child(nudge("−", { let d = doc_arc.clone(); let id = elem.id.clone(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { e.style.opacity = (e.style.opacity - 0.05).max(0.0); } } })))
                    // Geometry
                    .child(section_header("GEOMETRY", theme))
                    .child(geom_nudge("X", elem.x, &elem.id, doc_arc.clone(), false, false, theme))
                    .child(geom_nudge("Y", elem.y, &elem.id, doc_arc.clone(), false, true, theme))
                    .child(geom_nudge("W", elem.width, &elem.id, doc_arc.clone(), true, false, theme))
                    .child(geom_nudge("H", elem.height, &elem.id, doc_arc.clone(), true, true, theme))
                    // Shape-specific params from SVG definition
                    .child(if let Some(def) = lib.get(&elem.shape_id) {
                        if !def.params.is_empty() {
                            div().flex().flex_col()
                                .child(section_header("PARAMS", theme))
                                .children(def.params.iter().map(|pd| {
                                    let cur = elem.params.get(&pd.name).copied().unwrap_or(pd.default);
                                    let step = if pd.is_int { 1.0 } else { (pd.max - pd.min) / 20.0 };
                                    prop_row(Box::leak(pd.label.clone().into_boxed_str()), theme)
                                        .child(val_label(if pd.is_int { format!("{}", cur as i32) } else { format!("{:.1}", cur) }, theme))
                                        .child(nudge("+", { let d = doc_arc.clone(); let id = elem.id.clone(); let pn = pd.name.clone(); let max = pd.max; let step2 = step; move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { let v = e.params.entry(pn.clone()).or_insert(0.0); *v = (*v + step2).min(max); } } }))
                                        .child(nudge("−", { let d = doc_arc.clone(); let id = elem.id.clone(); let pn = pd.name.clone(); let min = pd.min; let step2 = step; move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { let v = e.params.entry(pn.clone()).or_insert(0.0); *v = (*v - step2).max(min); } } }))
                                }))
                                .into_any_element()
                        } else { div().into_any_element() }
                    } else { div().into_any_element() })
                    .into_any_element()
            } else {
                div().flex().flex_col().p_3().gap_2()
                    .child(section_header("CANVAS", theme))
                    .child(div().text_xs().text_color(theme.foreground.opacity(0.6))
                               .child(format!("{}×{}", canvas_size[0] as u32, canvas_size[1] as u32)))
                    .child(div().text_xs().text_color(theme.foreground.opacity(0.6))
                               .child(format!("{} elements", elem_count)))
                    .child(div().text_xs().text_color(theme.foreground.opacity(0.4)).pt_3()
                               .child("Select an element"))
                    .into_any_element()
            }
        )
}

fn geom_nudge(label: &'static str, val: f32, id: &str, doc: DocArc, is_size: bool, is_y: bool, theme: &Theme) -> impl IntoElement {
    prop_row(label, theme)
        .child(val_label(format!("{:.0}", val), theme))
        .child(nudge("+", { let d = doc.clone(); let id = id.to_string(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { match (is_size, is_y) { (false,false) => e.x+=1.0, (false,true) => e.y+=1.0, (true,false) => e.width=(e.width+1.0), (true,true) => e.height=(e.height+1.0), } } } }))
        .child(nudge("−", { let d = doc.clone(); let id = id.to_string(); move |_,_,_| { if let Some(e) = d.write().get_mut(&id) { match (is_size, is_y) { (false,false) => e.x-=1.0, (false,true) => e.y-=1.0, (true,false) => e.width=(e.width-1.0).max(10.0), (true,true) => e.height=(e.height-1.0).max(10.0), } } } }))
}

// ── Widgets ───────────────────────────────────────────────────────────────────

fn section_header_plain(text: &'static str, theme: &Theme) -> impl IntoElement {
    div().flex().h(px(36.0)).px_2().items_center()
         .border_b_1().border_color(theme.border.opacity(0.5))
         .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(text))
}

fn section_header(text: &str, theme: &Theme) -> impl IntoElement {
    div().flex().w_full().h(px(26.0)).px_2().items_center()
         .bg(theme.background.blend(theme.foreground.opacity(0.03)))
         .border_t_1().border_b_1().border_color(theme.border.opacity(0.25))
         .child(div().text_xs().font_weight(FontWeight::SEMIBOLD)
                     .text_color(theme.foreground.opacity(0.5))
                     .child(text.to_uppercase()))
}

fn prop_row(label: &str, theme: &Theme) -> Div {
    div().flex().w_full().h(px(30.0)).px_2().items_center().gap_1()
         .border_b_1().border_color(theme.border.opacity(0.15))
         .child(div().w(px(56.0)).text_xs().text_color(theme.foreground.opacity(0.5)).child(label.to_string()))
}

fn val_label(text: String, theme: &Theme) -> impl IntoElement {
    div().flex_1().text_xs().text_color(theme.foreground).child(text)
}

fn nudge(label: &'static str, handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static) -> impl IntoElement {
    div().w(px(18.0)).h(px(18.0)).flex().items_center().justify_center()
         .rounded(px(3.0)).bg(Hsla { h:0.0,s:0.0,l:0.5,a:0.12 })
         .hover(|s| s.bg(Hsla { h:0.0,s:0.0,l:0.5,a:0.25 }))
         .cursor_pointer()
         .text_xs().child(label)
         .on_mouse_down(MouseButton::Left, move |ev, win, cx| handler(ev, win, cx))
}

fn color_swatch(c: HslaColor) -> impl IntoElement {
    div().w(px(18.0)).h(px(18.0)).rounded(px(3.0))
         .bg(Hsla { h: c[0], s: c[1], l: c[2], a: c[3] })
         .border_1().border_color(Hsla { h:0.0,s:0.0,l:0.0,a:0.2 })
}

fn hsla_text(c: HslaColor) -> String {
    format!("H{:.0}° S{:.0}%", c[0]*360.0, c[1]*100.0)
}
