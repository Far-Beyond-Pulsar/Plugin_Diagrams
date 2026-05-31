//! WGPU renderer for the diagram canvas — grid background only.
//!
//! Shape rendering is handled by the GPUI canvas overlay in viewport.rs.
//! This renderer just provides the dot-grid backdrop and canvas shadow.

use wgpu::util::DeviceExt as _;
use super::types::DiagramRenderInput;

#[repr(C)]
#[derive(Copy, Clone)]
struct GridUniforms {
    pan_offset:    [f32; 2],
    zoom:          f32,
    _pad0:         f32,
    viewport_size: [f32; 2],
    canvas_size:   [f32; 2],
}

fn as_bytes<T: Copy>(v: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(v as *const T as *const u8, std::mem::size_of::<T>()) }
}

struct GpuState {
    pipeline:    wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group:  wgpu::BindGroup,
}

pub struct DiagramRenderer {
    state: Option<GpuState>,
}

impl DiagramRenderer {
    pub fn new() -> Self { Self { state: None } }

    pub fn render_frame(
        &mut self,
        device: &wgpu::Device,
        queue:  &wgpu::Queue,
        view:   &wgpu::TextureView,
        width:  u32,
        height: u32,
        format: wgpu::TextureFormat,
        input:  &DiagramRenderInput,
    ) {
        if self.state.is_none() {
            self.state = Some(Self::create_state(device, format));
        }
        let state = self.state.as_mut().unwrap();

        let [vp_w, vp_h] = if input.viewport_size[0] > 0.0 && input.viewport_size[1] > 0.0 {
            input.viewport_size
        } else {
            [width as f32, height as f32]
        };

        let uniforms = GridUniforms {
            pan_offset:    input.pan_offset,
            zoom:          input.zoom,
            _pad0:         0.0,
            viewport_size: [vp_w, vp_h],
            canvas_size:   input.canvas_size,
        };
        queue.write_buffer(&state.uniform_buf, 0, as_bytes(&uniforms));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("diagram_grid_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("diagram_grid_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice:    None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load:  wgpu::LoadOp::Clear(wgpu::Color { r: 0.97, g: 0.97, b: 0.98, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes:         None,
                occlusion_query_set:      None,
                multiview_mask:           None,
            });
            pass.set_pipeline(&state.pipeline);
            pass.set_bind_group(0, &state.bind_group, &[]);
            pass.draw(0..6, 0..1);
        }
        queue.submit(std::iter::once(encoder.finish()));
    }

    fn create_state(device: &wgpu::Device, format: wgpu::TextureFormat) -> GpuState {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label:  Some("diagram_grid_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label:   Some("diagram_grid_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding:    0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty:         wgpu::BindingType::Buffer {
                    ty:                 wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size:   None,
                },
                count: None,
            }],
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label:              Some("diagram_grid_uniform"),
            size:               std::mem::size_of::<GridUniforms>() as u64,
            usage:              wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label:   Some("diagram_grid_bg"),
            layout:  &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label:              Some("diagram_grid_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size:     0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label:  Some("diagram_grid_pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module:              &shader,
                entry_point:         Some("vs_main"),
                buffers:             &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology:  wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample:   wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module:              &shader,
                entry_point:         Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend:      Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache:          None,
        });

        GpuState { pipeline, uniform_buf, bind_group }
    }
}
