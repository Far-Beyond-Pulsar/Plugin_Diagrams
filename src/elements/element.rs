//! Data-driven diagram element — shape is identified by an SVG shape id,
//! with runtime parameters stored as a name→value map.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::shape_def::{ShapeLibrary, HandleAxis};

// ── Color ─────────────────────────────────────────────────────────────────────

/// HSLA color stored as [h, s, l, a], each 0..1.
pub type HslaColor = [f32; 4];

pub const DEFAULT_FILL:   HslaColor = [0.592, 0.40, 0.78, 1.0];
pub const DEFAULT_STROKE: HslaColor = [0.592, 0.56, 0.44, 1.0];
pub const DEFAULT_TEXT:   HslaColor = [0.0,   0.0,  0.1,  1.0];

// ── Style ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ElementStyle {
    pub fill:         HslaColor,
    pub stroke:       HslaColor,
    pub stroke_width: f32,
    pub font_size:    f32,
    pub font_color:   HslaColor,
    pub opacity:      f32,
}

impl Default for ElementStyle {
    fn default() -> Self {
        Self {
            fill:         DEFAULT_FILL,
            stroke:       DEFAULT_STROKE,
            stroke_width: 2.0,
            font_size:    12.0,
            font_color:   DEFAULT_TEXT,
            opacity:      1.0,
        }
    }
}

// ── Resize handles ────────────────────────────────────────────────────────────

pub const RESIZE_HANDLE_COUNT: usize = 8;

/// Canvas-space position of resize handle `i` (0=TL … 7=BR).
pub fn resize_handle_position(bounds: [f32; 4], i: usize) -> [f32; 2] {
    let [x, y, w, h] = bounds;
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    match i {
        0 => [x,     y     ], 1 => [cx,    y     ], 2 => [x + w, y     ],
        3 => [x,     cy    ],                        4 => [x + w, cy    ],
        5 => [x,     y + h ], 6 => [cx,    y + h ], 7 => [x + w, y + h ],
        _ => [cx,    cy    ],
    }
}

/// [dx_origin, dy_origin, dw, dh] scale factors when mouse moves (dx, dy).
pub fn resize_deltas(handle: usize) -> [f32; 4] {
    match handle {
        0 => [1.0,  1.0, -1.0, -1.0],
        1 => [0.0,  1.0,  0.0, -1.0],
        2 => [0.0,  1.0,  1.0, -1.0],
        3 => [1.0,  0.0, -1.0,  0.0],
        4 => [0.0,  0.0,  1.0,  0.0],
        5 => [1.0,  0.0, -1.0,  1.0],
        6 => [0.0,  0.0,  0.0,  1.0],
        7 => [0.0,  0.0,  1.0,  1.0],
        _ => [0.0,  0.0,  0.0,  0.0],
    }
}

// ── DiagramElement ────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiagramElement {
    pub id:       String,
    pub shape_id: String,
    pub x:        f32,
    pub y:        f32,
    pub width:    f32,
    pub height:   f32,
    pub label:    String,
    pub style:    ElementStyle,
    /// Shape-specific adjustable parameters (name → canvas-pixel value).
    pub params:   HashMap<String, f32>,
    pub z_order:  i32,
}

impl DiagramElement {
    pub fn new(shape_id: &str, x: f32, y: f32, w: f32, h: f32, lib: &ShapeLibrary) -> Self {
        let params = lib.get(shape_id)
            .map(|def| def.default_params())
            .unwrap_or_default();
        let label  = lib.get(shape_id)
            .map(|def| def.name.clone())
            .unwrap_or_else(|| shape_id.to_string());
        Self {
            id:       uuid::Uuid::new_v4().to_string(),
            shape_id: shape_id.to_string(),
            x, y, width: w, height: h,
            label,
            style:    ElementStyle::default(),
            params,
            z_order:  0,
        }
    }

    pub fn bounds(&self) -> [f32; 4] { [self.x, self.y, self.width, self.height] }

    pub fn set_bounds(&mut self, b: [f32; 4]) {
        self.x = b[0]; self.y = b[1]; self.width = b[2]; self.height = b[3];
    }

    pub fn contains_canvas_point(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }

    pub fn is_connector(&self) -> bool { self.shape_id == "connector" }

    /// Canvas-space N, E, S, W connection port positions.
    pub fn connection_ports(&self) -> [[f32; 2]; 4] {
        let cx = self.x + self.width  * 0.5;
        let cy = self.y + self.height * 0.5;
        [
            [cx,              self.y           ],
            [self.x + self.width, cy           ],
            [cx,              self.y + self.height],
            [self.x,          cy               ],
        ]
    }

    /// Canvas-space param handle positions, derived from the shape definition.
    pub fn param_handles<'a>(&self, lib: &'a ShapeLibrary) -> Vec<([f32; 2], &'a crate::shape_def::HandleDef)> {
        if let Some(def) = lib.get(&self.shape_id) {
            def.eval_handles(self.width, self.height, &self.params)
                .into_iter()
                .map(|([hx, hy], hd)| ([self.x + hx, self.y + hy], hd))
                .collect()
        } else {
            vec![]
        }
    }

    /// Apply a param handle drag: update the relevant param.
    pub fn apply_handle_drag(&mut self, handle_def: &crate::shape_def::HandleDef, dx: f32, dy: f32, lib: &ShapeLibrary) {
        let param_def = lib.get(&self.shape_id)
            .and_then(|def| def.params.iter().find(|p| p.name == handle_def.param));
        let Some(pd) = param_def else { return };

        let cur = self.params.get(&pd.name).copied().unwrap_or(pd.default);
        let raw_delta = match handle_def.axis {
            HandleAxis::X  => dx,
            HandleAxis::Y  => dy,
            HandleAxis::XY => (dx * dx + dy * dy).sqrt() * dx.signum(),
        };
        let delta = if handle_def.invert { -raw_delta } else { raw_delta };
        let new_val = (cur + delta).clamp(pd.min, pd.max);
        let new_val = if pd.is_int { new_val.round() } else { new_val };
        self.params.insert(pd.name.clone(), new_val);
    }
}
