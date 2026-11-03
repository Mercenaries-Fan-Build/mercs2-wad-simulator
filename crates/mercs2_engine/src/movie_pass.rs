//! Draws movie frames: each a texture stretched over its rect, as retail's quad at `0x00853940` does
//! (full 0..1 texture coordinates, white vertex colour, no blend with the widget's colour).

use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::movie::MovieDraw;

const SHADER: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>) -> Out {
    var o: Out;
    o.pos = vec4<f32>(pos, 0.0, 1.0);
    o.uv = uv;
    return o;
}
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@fragment fn fs(i: Out) -> @location(0) vec4<f32> { return textureSample(tex, samp, i.uv); }
"#;

struct Slot {
    size: [u32; 2],
    texture: wgpu::Texture,
    group: wgpu::BindGroup,
}

pub struct MoviePass {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    slots: HashMap<u64, Slot>,
    /// This frame's quads: the widget each draws and its vertices.
    quads: Vec<(u64, wgpu::Buffer)>,
}

impl MoviePass {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, stencil: wgpu::TextureFormat) -> MoviePass {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("movie bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("movie pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("movie"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("movie pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: stencil,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("movie sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        MoviePass { pipeline, layout, sampler, slots: HashMap::new(), quads: Vec::new() }
    }

    /// Upload changed frames and lay out this frame's quads on a target of `target` pixels.
    pub fn prepare(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, movies: &[MovieDraw], target: [u32; 2]) {
        self.slots.retain(|id, _| movies.iter().any(|m| m.widget == *id));
        self.quads.clear();
        for m in movies {
            let stale = self.slots.get(&m.widget).is_none_or(|s| s.size != m.size);
            if stale {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("movie frame"),
                    size: wgpu::Extent3d { width: m.size[0], height: m.size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("movie bind"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    ],
                });
                self.slots.insert(m.widget, Slot { size: m.size, texture, group });
            }
            if m.fresh || stale {
                let slot = &self.slots[&m.widget];
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &slot.texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    m.rgba,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(m.size[0] * 4), rows_per_image: Some(m.size[1]) },
                    wgpu::Extent3d { width: m.size[0], height: m.size[1], depth_or_array_layers: 1 },
                );
            }
            let ndc = |x: f32, y: f32| [x / target[0] as f32 * 2.0 - 1.0, 1.0 - y / target[1] as f32 * 2.0];
            let [x1, y1, x2, y2] = m.rect;
            let corners = [(ndc(x1, y1), [0.0, 0.0]), (ndc(x1, y2), [0.0, 1.0]), (ndc(x2, y1), [1.0, 0.0]), (ndc(x2, y2), [1.0, 1.0])];
            let vertices: Vec<f32> = corners.iter().flat_map(|(p, uv)| [p[0], p[1], uv[0], uv[1]]).collect();
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("movie quad"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.quads.push((m.widget, buffer));
        }
    }

    /// Record quad `index` of [`Self::prepare`]'s list.
    pub fn record<'p>(&'p self, pass: &mut wgpu::RenderPass<'p>, index: usize) {
        let (widget, buffer) = &self.quads[index];
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.slots[widget].group, &[]);
        pass.set_vertex_buffer(0, buffer.slice(..));
        pass.draw(0..4, 0..1);
    }
}
