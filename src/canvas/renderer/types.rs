//! Types shared between the diagram renderer and viewport.

#[derive(Clone, Debug, Default)]
pub struct DiagramRenderInput {
    pub pan_offset:    [f32; 2],
    pub zoom:          f32,
    pub canvas_size:   [f32; 2],
    pub viewport_size: [f32; 2],
}
