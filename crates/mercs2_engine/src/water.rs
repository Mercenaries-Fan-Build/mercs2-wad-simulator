//! `WaterNode` — the translucent water-surface render node (render-graph `PassId::WaterSurface`, the
//! exe's `FUN_00487540`/`FUN_00487dd0` slot). A screenshot-match of PgWater: one flat quad per wet
//! watermap cell, drawn as a fog-blended translucent plane over the scene — not the retail
//! reflection/refraction/wake stack (tech-free per the water scope). It is the first concrete
//! [`RenderNode`], built once from a CPU water mesh + the scene's fog/water palette; each frame it
//! re-uploads the camera and draws with alpha blending against the shared scene depth (occluded by
//! terrain in front of it, but not depth-writing, so submerged geometry shows through).

use wgpu::util::DeviceExt;

use crate::render::DEPTH_FORMAT;
use crate::render_graph::{PassCtx, PassId, RenderNode};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WaterUniform {
    view_proj: [[f32; 4]; 4],
    cam_pos: [f32; 4],
    shallow: [f32; 4],
    deep: [f32; 4],
    fog: [f32; 4],
    params: [f32; 4],
}

/// Palette + fog for the water surface, so it matches whatever `Scene::set_fog` the world uses.
#[derive(Clone, Copy)]
pub struct WaterStyle {
    /// Shallow (top-down) tint RGB + base alpha (`.a`).
    pub shallow: [f32; 4],
    /// Deep / grazing (fresnel) tint RGB + the fresnel alpha boost (`.a`).
    pub deep: [f32; 4],
    /// Scene fog color RGB + density (`.a`) — mirror `Scene::set_fog` so far water dissolves like land.
    pub fog: [f32; 4],
    /// Fog start distance (m) — mirror `Scene::set_fog`'s start.
    pub fog_start: f32,
}

impl Default for WaterStyle {
    fn default() -> Self {
        // Caribbean shallows → deep teal, tuned against the Maracaibo daylight palette.
        WaterStyle {
            shallow: [0.10, 0.42, 0.52, 0.55],
            deep: [0.02, 0.12, 0.22, 0.35],
            fog: [0.55, 0.62, 0.70, 0.00016],
            fog_start: 60.0,
        }
    }
}

/// The water render node: a translucent surface drawn in the `WaterSurface` slot.
pub struct WaterNode {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    vbuf: wgpu::Buffer,
    ibuf: wgpu::Buffer,
    index_count: u32,
    style: WaterStyle,
}

impl WaterNode {
    /// Build the node from a CPU water mesh (`positions` `[x,y,z]` + `u32` `indices`, e.g. from
    /// `mercs2_water::Watermap::surface_mesh`) and the scene's fog/water `style`. Returns `None` for an
    /// empty mesh (no wet cells) so the caller can skip registering it.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        positions: &[[f32; 3]],
        indices: &[u32],
        style: WaterStyle,
    ) -> Option<Self> {
        if positions.is_empty() || indices.is_empty() {
            return None;
        }
        let shader = device.create_shader_module(wgpu::include_wgsl!("water.wgsl"));
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("water bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("water uniform"),
            size: std::mem::size_of::<WaterUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("water bind"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
        });
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water vbuf"),
            contents: bytemuck::cast_slice(positions),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water ibuf"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("water pipeline layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("water pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_water",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: (3 * std::mem::size_of::<f32>()) as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_water",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    // Straight alpha over the scene: out = src*a + dst*(1-a).
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None, // seen from above or below (swimming) — draw both faces
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false, // translucent: tested against the scene, but not occluding
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        Some(WaterNode {
            pipeline,
            bind_group,
            uniform,
            vbuf,
            ibuf,
            index_count: indices.len() as u32,
            style,
        })
    }
}

impl RenderNode for WaterNode {
    fn id(&self) -> PassId {
        PassId::WaterSurface
    }

    fn record(&self, ctx: &mut PassCtx<'_>) {
        let u = WaterUniform {
            view_proj: ctx.view_proj.to_cols_array_2d(),
            cam_pos: [ctx.cam_pos.x, ctx.cam_pos.y, ctx.cam_pos.z, 0.0],
            shallow: self.style.shallow,
            deep: self.style.deep,
            fog: self.style.fog,
            params: [self.style.fog_start, ctx.time, 0.0, 0.0],
        };
        ctx.queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&u));

        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("water surface pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vbuf.slice(..));
        pass.set_index_buffer(self.ibuf.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..1);
    }
}
