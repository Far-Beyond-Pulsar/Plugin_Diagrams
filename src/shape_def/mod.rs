//! SVG-driven shape definition system.
//!
//! Each shape is a small SVG file in `src/shapes/`. The SVG encodes:
//!   • `<param>` elements — draggable parameters (name, label, default, min, max, type)
//!   • `<handle>` elements — where drag handles appear
//!   • `<path role="body|stroke">` — filled or stroke-only paths
//!   • Template `{expr}` tokens in path `d` attributes, evaluated at render time
//!
//! Expression language (evaluated inside `{...}`):
//!   Variables: `w` (width), `h` (height), any param name.
//!   Ops: `+` `-` `*` `/`  (left-to-right, * and / bind tighter)
//!   Example: `{w-r}`, `{w*skew}`, `{h*ell/2}`, `{w/2}`

use std::collections::HashMap;
use gpui::*;

// ── Public types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ParamDef {
    pub name:    String,
    pub label:   String,
    pub default: f32,
    pub min:     f32,
    pub max:     f32,
    pub is_int:  bool,
}

#[derive(Clone, Debug)]
pub struct HandleDef {
    pub param: String,
    /// Template expression for X position.
    pub cx_tmpl: String,
    /// Template expression for Y position.
    pub cy_tmpl: String,
    /// Which axis this handle moves along ("x", "y", or "xy").
    pub axis:   HandleAxis,
    /// If true, dragging right *decreases* the param (e.g. corner radius handle).
    pub invert: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HandleAxis { X, Y, XY }

#[derive(Clone, Debug)]
pub struct PathDef {
    pub d_tmpl:  String,
    pub role:    PathRole,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PathRole {
    Body,   // filled + stroked
    Stroke, // stroke only
}

/// Special built-in renderers for shapes that can't be expressed as static paths.
#[derive(Clone, Debug, PartialEq)]
pub enum SpecialRenderer {
    None,
    Table,
    Connector,
    Text,
}

#[derive(Clone, Debug)]
pub struct ShapeDefinition {
    pub id:               String,
    pub name:             String,
    pub category:         String,
    pub params:           Vec<ParamDef>,
    pub handles:          Vec<HandleDef>,
    pub paths:            Vec<PathDef>,
    pub special_renderer: SpecialRenderer,
}

impl ShapeDefinition {
    /// Default param values as a map.
    pub fn default_params(&self) -> HashMap<String, f32> {
        self.params.iter().map(|p| (p.name.clone(), p.default)).collect()
    }

    /// Evaluate all handle positions given current params, element w/h.
    pub fn eval_handles(&self, w: f32, h: f32, params: &HashMap<String, f32>) -> Vec<([f32; 2], &HandleDef)> {
        let vars = make_vars(w, h, params);
        self.handles.iter().map(|hd| {
            let cx = eval_expr(&hd.cx_tmpl, &vars);
            let cy = eval_expr(&hd.cy_tmpl, &vars);
            ([cx, cy], hd)
        }).collect()
    }
}

// ── ShapeLibrary ──────────────────────────────────────────────────────────────

/// Global registry of shape definitions, loaded from embedded SVG files.
#[derive(Default, Clone)]
pub struct ShapeLibrary {
    pub shapes: Vec<ShapeDefinition>,
}

impl ShapeLibrary {
    pub fn load_builtins() -> Self {
        let svgs: &[(&str, &str)] = &[
            ("rectangle",    include_str!("../shapes/rectangle.svg")),
            ("ellipse",      include_str!("../shapes/ellipse.svg")),
            ("diamond",      include_str!("../shapes/diamond.svg")),
            ("triangle",     include_str!("../shapes/triangle.svg")),
            ("parallelogram",include_str!("../shapes/parallelogram.svg")),
            ("hexagon",      include_str!("../shapes/hexagon.svg")),
            ("cylinder",     include_str!("../shapes/cylinder.svg")),
            ("table",        include_str!("../shapes/table.svg")),
            ("text",         include_str!("../shapes/text.svg")),
            ("connector",    include_str!("../shapes/connector.svg")),
        ];

        let mut shapes = Vec::new();
        for &(id, src) in svgs {
            match parse_svg(id, src) {
                Ok(def) => shapes.push(def),
                Err(e)  => tracing::warn!("Failed to parse shape {}: {}", id, e),
            }
        }
        Self { shapes }
    }

    pub fn get(&self, id: &str) -> Option<&ShapeDefinition> {
        self.shapes.iter().find(|s| s.id == id)
    }

    pub fn by_category(&self) -> Vec<(&str, Vec<&ShapeDefinition>)> {
        let mut map: Vec<(&str, Vec<&ShapeDefinition>)> = Vec::new();
        for s in &self.shapes {
            let cat = s.category.as_str();
            if let Some(entry) = map.iter_mut().find(|(c, _)| *c == cat) {
                entry.1.push(s);
            } else {
                map.push((cat, vec![s]));
            }
        }
        map
    }
}

// ── SVG Parser ────────────────────────────────────────────────────────────────

fn parse_svg(id: &str, src: &str) -> Result<ShapeDefinition, String> {
    // Very simple XML pull-parser — not a full XML parser.
    // Handles the subset used by our shape SVGs.

    let name     = attr_val(src, "data-name").unwrap_or(id).to_string();
    let category = attr_val(src, "data-category").unwrap_or("General").to_string();
    let special  = match attr_val(src, "data-renderer") {
        Some("table")     => SpecialRenderer::Table,
        Some("connector") => SpecialRenderer::Connector,
        Some("text")      => SpecialRenderer::Text,
        _                 => SpecialRenderer::None,
    };

    let params  = parse_params(src);
    let handles = parse_handles(src);
    let paths   = parse_paths(src);

    Ok(ShapeDefinition { id: id.to_string(), name, category, params, handles, paths, special_renderer: special })
}

/// Extract the value of `attr="value"` from raw XML text.
fn attr_val<'a>(xml: &'a str, attr: &str) -> Option<&'a str> {
    let needle = format!("{}=\"", attr);
    let start  = xml.find(needle.as_str())? + needle.len();
    let end    = xml[start..].find('"')? + start;
    Some(&xml[start..end])
}

fn parse_params(xml: &str) -> Vec<ParamDef> {
    let mut params = Vec::new();
    let mut rest   = xml;
    while let Some(pos) = rest.find("<param ") {
        rest = &rest[pos + 7..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..tag_end];

        let name    = attr_val(tag, "name").unwrap_or("").to_string();
        let label   = attr_val(tag, "label").unwrap_or(&name).to_string();
        let default = attr_val(tag, "default").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let min     = attr_val(tag, "min").and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let max     = attr_val(tag, "max").and_then(|v| v.parse().ok()).unwrap_or(100.0);
        let is_int  = attr_val(tag, "type").map(|t| t == "int").unwrap_or(false);

        if !name.is_empty() {
            params.push(ParamDef { name, label, default, min, max, is_int });
        }
    }
    params
}

fn parse_handles(xml: &str) -> Vec<HandleDef> {
    let mut handles = Vec::new();
    let mut rest    = xml;
    while let Some(pos) = rest.find("<handle ") {
        rest = &rest[pos + 8..];
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag     = &rest[..tag_end];

        let param   = attr_val(tag, "param").unwrap_or("").to_string();
        let cx_tmpl = attr_val(tag, "cx").unwrap_or("0").to_string();
        let cy_tmpl = attr_val(tag, "cy").unwrap_or("0").to_string();
        let axis    = match attr_val(tag, "axis") {
            Some("y")  => HandleAxis::Y,
            Some("xy") => HandleAxis::XY,
            _          => HandleAxis::X,
        };
        let invert  = attr_val(tag, "invert").map(|v| v == "true").unwrap_or(false);

        if !param.is_empty() {
            handles.push(HandleDef { param, cx_tmpl, cy_tmpl, axis, invert });
        }
    }
    handles
}

fn parse_paths(xml: &str) -> Vec<PathDef> {
    let mut paths = Vec::new();
    let mut rest  = xml;
    while let Some(pos) = rest.find("<path ") {
        rest = &rest[pos + 6..];
        // Find closing '>' (may be '/>').
        let tag_end = rest.find('>').unwrap_or(rest.len());
        let tag = &rest[..tag_end];

        let role = match attr_val(tag, "role") {
            Some("stroke") => PathRole::Stroke,
            _              => PathRole::Body,
        };
        let d_tmpl = attr_val(tag, "d").unwrap_or("").to_string();

        if !d_tmpl.is_empty() {
            paths.push(PathDef { d_tmpl, role });
        }
    }
    paths
}

// ── Expression evaluator ──────────────────────────────────────────────────────

fn make_vars(w: f32, h: f32, params: &HashMap<String, f32>) -> HashMap<String, f32> {
    let mut vars = HashMap::new();
    vars.insert("w".to_string(), w);
    vars.insert("h".to_string(), h);
    for (k, v) in params {
        vars.insert(k.clone(), *v);
    }
    vars
}

/// Substitute all `{expr}` tokens in `template` and return the resulting string.
pub fn substitute(template: &str, vars: &HashMap<String, f32>) -> String {
    let mut result = String::new();
    let mut chars  = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let mut expr = String::new();
            for c2 in chars.by_ref() {
                if c2 == '}' { break; }
                expr.push(c2);
            }
            let val = eval_expr(expr.trim(), vars);
            result.push_str(&format!("{:.6}", val));
        } else {
            result.push(c);
        }
    }
    result
}

pub fn eval_expr(expr: &str, vars: &HashMap<String, f32>) -> f32 {
    let (val, _) = parse_add(expr.trim(), vars);
    val
}

fn parse_add<'a>(input: &'a str, vars: &HashMap<String, f32>) -> (f32, &'a str) {
    let (mut val, mut rest) = parse_mul(input, vars);
    loop {
        let t = rest.trim_start();
        if t.starts_with('+') {
            let (v2, r2) = parse_mul(t[1..].trim_start(), vars);
            val += v2; rest = r2;
        } else if t.starts_with('-') {
            let (v2, r2) = parse_mul(t[1..].trim_start(), vars);
            val -= v2; rest = r2;
        } else { break; }
    }
    (val, rest)
}

fn parse_mul<'a>(input: &'a str, vars: &HashMap<String, f32>) -> (f32, &'a str) {
    let (mut val, mut rest) = parse_atom(input, vars);
    loop {
        let t = rest.trim_start();
        if t.starts_with('*') {
            let (v2, r2) = parse_atom(t[1..].trim_start(), vars);
            val *= v2; rest = r2;
        } else if t.starts_with('/') {
            let (v2, r2) = parse_atom(t[1..].trim_start(), vars);
            if v2 != 0.0 { val /= v2; } rest = r2;
        } else { break; }
    }
    (val, rest)
}

fn parse_atom<'a>(input: &'a str, vars: &HashMap<String, f32>) -> (f32, &'a str) {
    let input = input.trim_start();

    // Parenthesised sub-expression.
    if input.starts_with('(') {
        let (val, rest) = parse_add(&input[1..], vars);
        let rest = rest.trim_start();
        let rest = if rest.starts_with(')') { &rest[1..] } else { rest };
        return (val, rest);
    }

    // Number literal (possibly negative).
    if input.starts_with(|c: char| c.is_ascii_digit() || c == '.') || input.starts_with('-') {
        let neg = input.starts_with('-');
        let s   = if neg { &input[1..] } else { input };
        let len = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').count();
        let num: f32 = s[..len].parse().unwrap_or(0.0);
        let rest = &s[len..];
        return (if neg { -num } else { num }, rest);
    }

    // Identifier (variable name).
    let len = input.chars().take_while(|c| c.is_alphanumeric() || *c == '_').count();
    if len > 0 {
        let name = &input[..len];
        let val  = vars.get(name).copied().unwrap_or(0.0);
        return (val, &input[len..]);
    }

    (0.0, input)
}

// ── SVG Path parser → PathBuilder calls ──────────────────────────────────────

/// Parse a substituted SVG `d` string and feed it into a PathBuilder.
/// Returns false if parsing failed.
pub fn build_path(d: &str, builder: &mut PathBuilder) -> bool {
    let mut chars    = d.chars().peekable();
    let mut cur_x    = 0f32;
    let mut cur_y    = 0f32;
    let mut start_x  = 0f32;
    let mut start_y  = 0f32;
    let mut last_cmd = ' ';

    loop {
        skip_ws(&mut chars);
        let Some(&c) = chars.peek() else { break };

        // SVG path commands: letter or continuation of previous command.
        let cmd = if c.is_ascii_alphabetic() {
            let cmd = c;
            chars.next();
            last_cmd = cmd;
            cmd
        } else {
            // Implicit command: L after M, l after m.
            match last_cmd {
                'M' => 'L',
                'm' => 'l',
                c   => c,
            }
        };

        match cmd {
            'M' | 'm' => {
                let x = read_num(&mut chars);
                skip_sep(&mut chars);
                let y = read_num(&mut chars);
                if cmd == 'm' { cur_x += x; cur_y += y; } else { cur_x = x; cur_y = y; }
                start_x = cur_x; start_y = cur_y;
                builder.move_to(point(px(cur_x), px(cur_y)));
            }
            'L' | 'l' => {
                let x = read_num(&mut chars);
                skip_sep(&mut chars);
                let y = read_num(&mut chars);
                if cmd == 'l' { cur_x += x; cur_y += y; } else { cur_x = x; cur_y = y; }
                builder.line_to(point(px(cur_x), px(cur_y)));
            }
            'H' | 'h' => {
                let x = read_num(&mut chars);
                if cmd == 'h' { cur_x += x; } else { cur_x = x; }
                builder.line_to(point(px(cur_x), px(cur_y)));
            }
            'V' | 'v' => {
                let y = read_num(&mut chars);
                if cmd == 'v' { cur_y += y; } else { cur_y = y; }
                builder.line_to(point(px(cur_x), px(cur_y)));
            }
            'Q' | 'q' => {
                let cx = read_num(&mut chars); skip_sep(&mut chars);
                let cy = read_num(&mut chars); skip_sep(&mut chars);
                let x  = read_num(&mut chars); skip_sep(&mut chars);
                let y  = read_num(&mut chars);
                let (acx, acy, ax, ay) = if cmd == 'q' {
                    (cur_x + cx, cur_y + cy, cur_x + x, cur_y + y)
                } else { (cx, cy, x, y) };
                builder.curve_to(point(px(ax), px(ay)), point(px(acx), px(acy)));
                cur_x = ax; cur_y = ay;
            }
            'C' | 'c' => {
                let c1x = read_num(&mut chars); skip_sep(&mut chars);
                let c1y = read_num(&mut chars); skip_sep(&mut chars);
                let c2x = read_num(&mut chars); skip_sep(&mut chars);
                let c2y = read_num(&mut chars); skip_sep(&mut chars);
                let x   = read_num(&mut chars); skip_sep(&mut chars);
                let y   = read_num(&mut chars);
                let (ac1x, ac1y, ac2x, ac2y, ax, ay) = if cmd == 'c' {
                    (cur_x+c1x, cur_y+c1y, cur_x+c2x, cur_y+c2y, cur_x+x, cur_y+y)
                } else { (c1x, c1y, c2x, c2y, x, y) };
                builder.cubic_bezier_to(
                    point(px(ax), px(ay)),
                    point(px(ac1x), px(ac1y)),
                    point(px(ac2x), px(ac2y)),
                );
                cur_x = ax; cur_y = ay;
            }
            'A' | 'a' => {
                let rx      = read_num(&mut chars); skip_sep(&mut chars);
                let ry      = read_num(&mut chars); skip_sep(&mut chars);
                let rot     = read_num(&mut chars); skip_sep(&mut chars);
                let large   = read_flag(&mut chars);  skip_sep(&mut chars);
                let sweep   = read_flag(&mut chars);  skip_sep(&mut chars);
                let x       = read_num(&mut chars);   skip_sep(&mut chars);
                let y       = read_num(&mut chars);
                let (ax, ay) = if cmd == 'a' { (cur_x + x, cur_y + y) } else { (x, y) };
                builder.arc_to(
                    point(px(rx), px(ry)),
                    px(rot),
                    large, sweep,
                    point(px(ax), px(ay)),
                );
                cur_x = ax; cur_y = ay;
            }
            'Z' | 'z' => {
                builder.close();
                cur_x = start_x; cur_y = start_y;
            }
            _ => { chars.next(); } // skip unknown
        }
    }
    true
}

fn skip_ws(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) { chars.next(); }
}

fn skip_sep(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek().map(|c| c.is_whitespace() || *c == ',').unwrap_or(false) { chars.next(); }
}

fn read_num(chars: &mut std::iter::Peekable<std::str::Chars>) -> f32 {
    skip_sep(chars);
    let mut s = String::new();
    if chars.peek() == Some(&'-') { s.push('-'); chars.next(); }
    while chars.peek().map(|c| c.is_ascii_digit() || *c == '.' || *c == 'e' || *c == 'E').unwrap_or(false) {
        s.push(chars.next().unwrap());
    }
    s.parse().unwrap_or(0.0)
}

fn read_flag(chars: &mut std::iter::Peekable<std::str::Chars>) -> bool {
    skip_sep(chars);
    match chars.peek() {
        Some('1') => { chars.next(); true  }
        Some('0') => { chars.next(); false }
        _         => false
    }
}
