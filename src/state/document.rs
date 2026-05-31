//! DiagramDocument — pure element store with no history (history lives in viewport).

use crate::elements::element::DiagramElement;
use crate::shape_def::ShapeLibrary;
use super::tool_state::ToolState;

pub struct DiagramDocument {
    pub elements:     Vec<DiagramElement>,
    pub selected_ids: Vec<String>,
    pub tool_state:   ToolState,
    pub canvas_size:  [f32; 2],
    pub is_dirty:     bool,
}

impl std::fmt::Debug for DiagramDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiagramDocument").field("n_elements", &self.elements.len()).finish()
    }
}

impl DiagramDocument {
    pub fn new(canvas_w: f32, canvas_h: f32) -> Self {
        Self {
            elements:     Vec::new(),
            selected_ids: Vec::new(),
            tool_state:   ToolState::default(),
            canvas_size:  [canvas_w, canvas_h],
            is_dirty:     false,
        }
    }

    // ── CRUD ──────────────────────────────────────────────────────────────────

    pub fn get(&self, id: &str) -> Option<&DiagramElement> {
        self.elements.iter().find(|e| e.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut DiagramElement> {
        self.elements.iter_mut().find(|e| e.id == id)
    }

    pub fn add(&mut self, elem: DiagramElement) {
        self.is_dirty = true;
        self.elements.push(elem);
        self.sort_by_z();
    }

    pub fn remove(&mut self, id: &str) -> Option<DiagramElement> {
        if let Some(pos) = self.elements.iter().position(|e| e.id == id) {
            self.is_dirty = true;
            Some(self.elements.remove(pos))
        } else { None }
    }

    pub fn elements_sorted(&self) -> &[DiagramElement] { &self.elements }

    fn sort_by_z(&mut self) { self.elements.sort_by_key(|e| e.z_order); }

    // ── Hit testing ───────────────────────────────────────────────────────────

    pub fn hit_test(&self, cx: f32, cy: f32) -> Option<&str> {
        for elem in self.elements.iter().rev() {
            if elem.is_connector() {
                if connector_hit(elem, cx, cy) { return Some(&elem.id); }
            } else if elem.contains_canvas_point(cx, cy) {
                return Some(&elem.id);
            }
        }
        None
    }

    pub fn hit_rect(&self, rx: f32, ry: f32, rw: f32, rh: f32) -> Vec<String> {
        self.elements.iter()
            .filter(|e| e.x < rx + rw && e.x + e.width  > rx &&
                        e.y < ry + rh && e.y + e.height > ry)
            .map(|e| e.id.clone())
            .collect()
    }

    // ── Selection ─────────────────────────────────────────────────────────────

    pub fn select_one(&mut self, id: String)          { self.selected_ids = vec![id]; }
    pub fn toggle_select(&mut self, id: String)       {
        if let Some(pos) = self.selected_ids.iter().position(|x| x == &id) {
            self.selected_ids.remove(pos);
        } else { self.selected_ids.push(id); }
    }
    pub fn select_set(&mut self, ids: Vec<String>)    { self.selected_ids = ids; }
    pub fn clear_selection(&mut self)                 { self.selected_ids.clear(); }
    pub fn is_selected(&self, id: &str) -> bool       { self.selected_ids.iter().any(|x| x == id) }

    pub fn snapshot_selected_bounds(&self) -> Vec<(String, [f32; 4])> {
        self.selected_ids.iter()
            .filter_map(|id| self.get(id).map(|e| (id.clone(), e.bounds())))
            .collect()
    }
}

fn connector_hit(elem: &DiagramElement, cx: f32, cy: f32) -> bool {
    let margin = 4.0;
    cx >= elem.x - margin && cx <= elem.x + elem.width  + margin &&
    cy >= elem.y - margin && cy <= elem.y + elem.height + margin
}
