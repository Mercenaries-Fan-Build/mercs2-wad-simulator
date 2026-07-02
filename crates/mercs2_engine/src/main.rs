//! mercs2_engine — Phase-1 skeleton of the native 64-bit Mercenaries 2 reimplementation.
//!
//! See `docs/modernization/00_charter.md`. This is the render shell: a wgpu (DX12/Vulkan/Metal)
//! window with a working pipeline.
//!
//! Usage:
//!   cargo run -p mercs2_engine                     # placeholder triangle
//!   cargo run -p mercs2_engine -- <model.bin>      # render a real model container (point cloud)
//!   cargo run -p mercs2_engine -- --dump <model.bin>  # headless: parse + print stats, no window

mod mesh;
mod pose;
mod scene;
mod wad;

use mesh::Vertex;
use std::sync::Arc;
use winit::{
    event::{Event, KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

// Placeholder geometry used when no model is supplied.
const TRI: &[Vertex] = &[
    Vertex { pos: [ 0.0,  0.6, 0.5], color: [0.90, 0.30, 0.10], uv: [0.5, 0.0], normal: [0.0, 0.0, -1.0], tangent: [1.0, 0.0, 0.0, 1.0], joints: [0, 0, 0, 0], weights: [255, 0, 0, 0] },
    Vertex { pos: [-0.6, -0.5, 0.5], color: [0.15, 0.55, 0.85], uv: [0.0, 1.0], normal: [0.0, 0.0, -1.0], tangent: [1.0, 0.0, 0.0, 1.0], joints: [0, 0, 0, 0], weights: [255, 0, 0, 0] },
    Vertex { pos: [ 0.6, -0.5, 0.5], color: [0.20, 0.75, 0.30], uv: [1.0, 1.0], normal: [0.0, 0.0, -1.0], tangent: [1.0, 0.0, 0.0, 1.0], joints: [0, 0, 0, 0], weights: [255, 0, 0, 0] },
];

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Upload a decoded DXT/BC texture (mip 0) and return its view. Returns None if the resident
/// data is too short (partial/streamed texture) so the caller can fall back.
fn make_bc_view(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    td: &mercs2_formats::texture::TextureData,
    srgb: bool,
) -> Option<wgpu::TextureView> {
    use mercs2_formats::texture::TexFormat;
    // Diffuse is sRGB (gamma); normal/spec maps are linear data.
    let (format, block_bytes) = match (td.format, srgb) {
        (TexFormat::Bc1, true) => (wgpu::TextureFormat::Bc1RgbaUnormSrgb, 8u32),
        (TexFormat::Bc1, false) => (wgpu::TextureFormat::Bc1RgbaUnorm, 8u32),
        (TexFormat::Bc3, true) => (wgpu::TextureFormat::Bc3RgbaUnormSrgb, 16u32),
        (TexFormat::Bc3, false) => (wgpu::TextureFormat::Bc3RgbaUnorm, 16u32),
    };
    let blocks_wide = (td.width + 3) / 4;
    let blocks_high = (td.height + 3) / 4;
    let need = (blocks_wide * block_bytes * blocks_high) as usize;
    if td.width == 0 || td.height == 0 || td.mip0.len() < need {
        return None; // partial/streamed resident tail — not enough for mip 0
    }
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("diffuse"),
        size: wgpu::Extent3d {
            width: td.width,
            height: td.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &td.mip0[..need],
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(blocks_wide * block_bytes),
            rows_per_image: Some(blocks_high),
        },
        wgpu::Extent3d {
            width: td.width,
            height: td.height,
            depth_or_array_layers: 1,
        },
    );
    Some(tex.create_view(&wgpu::TextureViewDescriptor::default()))
}

fn make_tex_bind(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    diffuse: &wgpu::TextureView,
    normal: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(diffuse),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(normal),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

/// A 1×1 flat tangent-space normal (0,0,1) for the DXT5nm read (X in alpha, Y in green): alpha=128
/// and green=128 give X=Y=0 → Z=1. Linear. Fallback for materials without a normal map.
fn make_flat_normal_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("flat normal"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[128u8, 128, 255, 128], // green=128 (Y=0), alpha=128 (X=0) -> flat (0,0,1)
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

/// A 1×1 white RGBA texture view — fallback for groups with no/partial diffuse.
fn make_white_view(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white"),
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8, 255, 255, 255],
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(4),
            rows_per_image: Some(1),
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

fn make_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width.max(1),
                height: config.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

struct Renderer {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    vbuf: wgpu::Buffer,
    nverts: u32,
    ibuf: Option<wgpu::Buffer>,
    nindices: u32,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    /// Skinning palette bind group (group 2). Static at bind pose; updated per frame when animating.
    bone_bind: wgpu::BindGroup,
    /// Backing storage buffer for the bone palette (re-uploaded each frame when animating).
    bone_buf: wgpu::Buffer,
    /// Per-bone rig for re-posing under animation (empty for un-skinned geometry).
    rig: Vec<mesh::BoneRig>,
    /// A real animation clip bound to this model, if loaded; drives the palette per frame.
    clip: Option<ClipAnim>,
    /// When set (and no real clip), drive the palette from the synthetic joint-wobble proof.
    animate: bool,
    tex_binds: Vec<wgpu::BindGroup>,
    /// Per-group draws: (index_start, index_count, index into `tex_binds`).
    draw_calls: Vec<(u32, u32, usize)>,
    depth_view: wgpu::TextureView,
    /// Model-fit transform (centre + uniform scale), folded into the MVP so skinning is model-space.
    fit: glam::Mat4,
    /// Per-entity world transform (ECS path), folded into the MVP after the model fit. Identity in
    /// the standalone `--animate` path.
    entity_model: glam::Mat4,
    start: std::time::Instant,
}

impl Renderer {
    async fn new(
        window: Arc<Window>,
        verts: &[Vertex],
        indices: &[u32],
        draws: &[mesh::DrawGroup],
        textures: &TexMap,
        skin: &mesh::SkinData,
        clip: Option<ClipAnim>,
        animate: bool,
        points: bool,
    ) -> Renderer {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY, // DX12 / Vulkan / Metal
            ..Default::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .expect("create_surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter");
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("mercs2_engine device"),
                    required_features: wgpu::Features::TEXTURE_COMPRESSION_BC, // BC1/BC3 upload
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("request_device");

        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface unsupported by adapter");
        config.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        surface.configure(&device, &config);

        // Camera uniform (a single 4x4 MVP matrix), updated per frame.
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera uniform"),
            size: 96, // mat4 mvp (64) + vec4 fog_color_density (16) + vec4 fog_misc (16); fog off here
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bgl"),
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
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera bind"),
            layout: &camera_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // Skinning palette (group 2): the bone matrices, row-major (see shader.wgsl). Uploaded as a
        // read-only storage buffer so it can grow to any bone count and (Phase B) update per frame.
        // Always at least one identity bone so un-skinned geometry (bone 0) passes through unchanged.
        let mut bone_floats: Vec<f32> = Vec::new();
        let palette: &[[[f32; 4]; 4]] = if skin.bones.is_empty() {
            &[[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ]]
        } else {
            &skin.bones
        };
        for m in palette {
            for row in m {
                bone_floats.extend_from_slice(row);
            }
        }
        let bone_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bone palette"),
            size: (bone_floats.len() * 4) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: true,
        });
        bone_buf
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::cast_slice(&bone_floats));
        bone_buf.unmap();
        let bone_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bone bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bone_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bone bind"),
            layout: &bone_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: bone_buf.as_entire_binding(),
            }],
        });

        // Material bind group layout (group 1): diffuse + normal-map + sampler.
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material bgl"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Decode every texture to a view (normal maps = linear, diffuse = sRGB).
        let normal_hashes: std::collections::HashSet<u32> =
            draws.iter().filter_map(|d| d.normal).collect();
        let mut views: std::collections::HashMap<u32, wgpu::TextureView> =
            std::collections::HashMap::new();
        for (hash, td) in textures {
            let srgb = !normal_hashes.contains(hash);
            if let Some(v) = make_bc_view(&device, &queue, td, srgb) {
                views.insert(*hash, v);
            }
        }
        let white = make_white_view(&device, &queue);
        let flat_normal = make_flat_normal_view(&device, &queue);

        // tex_binds[0] = fallback (white diffuse + flat normal); then one per draw group.
        let mut tex_binds: Vec<wgpu::BindGroup> = Vec::new();
        tex_binds.push(make_tex_bind(&device, &tex_bgl, &sampler, &white, &flat_normal));
        let mut draw_calls: Vec<(u32, u32, usize)> = Vec::new();
        for d in draws {
            let diff = d.diffuse.and_then(|h| views.get(&h)).unwrap_or(&white);
            let norm = d.normal.and_then(|h| views.get(&h)).unwrap_or(&flat_normal);
            let idx = tex_binds.len();
            tex_binds.push(make_tex_bind(&device, &tex_bgl, &sampler, diff, norm));
            draw_calls.push((d.index_start, d.index_count, idx));
        }

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pipeline layout"),
            bind_group_layouts: &[&camera_bgl, &tex_bgl, &bone_bgl],
            push_constant_ranges: &[],
        });
        let vbuf_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Float32x4, 5 => Uint8x4, 6 => Unorm8x4],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("geometry pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[vbuf_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: if points {
                    wgpu::PrimitiveTopology::PointList
                } else {
                    wgpu::PrimitiveTopology::TriangleList
                },
                // Geometry winds CCW-outward (verified 99.7%); LH projection flips that to CW-front.
                front_face: wgpu::FrontFace::Cw,
                cull_mode: if points { None } else { Some(wgpu::Face::Back) },
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vbytes: &[u8] = bytemuck::cast_slice(verts);
        let vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("vertex buffer"),
            size: vbytes.len() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX,
            mapped_at_creation: true,
        });
        vbuf.slice(..).get_mapped_range_mut().copy_from_slice(vbytes);
        vbuf.unmap();

        let (ibuf, nindices) = if indices.is_empty() {
            (None, 0)
        } else {
            let ibytes: &[u8] = bytemuck::cast_slice(indices);
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("index buffer"),
                size: ibytes.len() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::INDEX,
                mapped_at_creation: true,
            });
            b.slice(..).get_mapped_range_mut().copy_from_slice(ibytes);
            b.unmap();
            (Some(b), indices.len() as u32)
        };

        let depth_view = make_depth(&device, &config);

        // Fit transform: p_view = scale · (p_model − centre). Folded into the MVP so vertices stay
        // in model space for skinning. Column-vector: from_scale · from_translation(−centre).
        let fit = glam::Mat4::from_scale(glam::Vec3::splat(skin.scale))
            * glam::Mat4::from_translation(-glam::Vec3::from(skin.center));

        Renderer {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            vbuf,
            nverts: verts.len() as u32,
            ibuf,
            nindices,
            camera_buf,
            camera_bind,
            bone_bind,
            bone_buf,
            rig: skin.rig.clone(),
            clip,
            animate: animate && !skin.rig.is_empty(),
            tex_binds,
            draw_calls,
            depth_view,
            fit,
            entity_model: glam::Mat4::IDENTITY,
            start: std::time::Instant::now(),
        }
    }

    fn resize(&mut self, new: winit::dpi::PhysicalSize<u32>) {
        if new.width > 0 && new.height > 0 {
            self.size = new;
            self.config.width = new.width;
            self.config.height = new.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = make_depth(&self.device, &self.config);
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        // Auto-orbit camera around the model (centered at origin), left-handed Y-up.
        let t = self.start.elapsed().as_secs_f32();
        let angle = t * 0.6;
        let radius = 2.6f32;
        let eye = glam::Vec3::new(radius * angle.sin(), 0.15, radius * angle.cos());
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let view = glam::Mat4::look_at_lh(eye, glam::Vec3::ZERO, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_lh(45f32.to_radians(), aspect, 0.1, 100.0);
        let mvp = proj * view * (self.fit * self.entity_model);
        // mvp + fog params; fog stays DISABLED (zeros) on this legacy path (layout compat only).
        let mut cam = [0f32; 24];
        cam[..16].copy_from_slice(&mvp.to_cols_array());
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&cam));

        // Animation: recompute + upload the skinning palette from the current pose. A real clip
        // (looped) drives it if bound; otherwise the synthetic joint-wobble proves the path.
        if let Some(ca) = &self.clip {
            let dur = ca.clip.duration.max(1e-3);
            let sample = ca.clip.sample_local(t % dur);
            let pal = pose::havok_palette(&self.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks);
            self.queue
                .write_buffer(&self.bone_buf, 0, bytemuck::cast_slice(&pose::flatten(&pal)));
        } else if self.animate {
            let pal = pose::synthetic_palette(&self.rig, t);
            self.queue
                .write_buffer(&self.bone_buf, 0, bytemuck::cast_slice(&pose::flatten(&pal)));
        }

        let output = self.surface.get_current_texture()?;
        let view_tex = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.04, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind, &[]);
            pass.set_bind_group(2, &self.bone_bind, &[]);
            // group 1 (texture) must always be bound; index 0 is the white fallback.
            let fallback = &self.tex_binds[0];
            pass.set_bind_group(1, fallback, &[]);
            pass.set_vertex_buffer(0, self.vbuf.slice(..));
            if let Some(ib) = &self.ibuf {
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                if self.draw_calls.is_empty() {
                    pass.draw_indexed(0..self.nindices, 0, 0..1);
                } else {
                    for &(start, count, bind) in &self.draw_calls {
                        pass.set_bind_group(1, &self.tex_binds[bind], &[]);
                        pass.draw_indexed(start..start + count, 0, 0..1);
                    }
                }
            } else {
                pass.draw(0..self.nverts, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Headless probe: print the registry-discovered vz.wad path and exit.
    if args.iter().any(|a| a == "--which") {
        match wad::registry_vz_wad() {
            Some(p) => println!("registry vz.wad: {p}"),
            None => println!("registry vz.wad: <not found>"),
        }
        return;
    }

    // Headless parse: verify a model container without opening a window.
    if let Some(i) = args.iter().position(|a| a == "--dump") {
        let path = args.get(i + 1).map(String::as_str).unwrap_or("");
        match mesh::load_model_block(path) {
            Ok((v, s)) => {
                println!("[dump] {} meshes, {} vertices", s.meshes, s.vertices);
                println!("[dump] model-space bbox min={:?} max={:?}", s.bbox_min, s.bbox_max);
                println!("[dump] first fitted verts (clip-space):");
                for vert in v.iter().take(5) {
                    println!("   {:?}", vert.pos);
                }
            }
            Err(e) => {
                eprintln!("[dump] ERROR: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // WAD mode: load a real base-game model. vz.wad is taken from --wad <path>, else auto-discovered
    // from the EA Games registry key (Install Dir\data\vz.wad).
    let wad_mode = args
        .iter()
        .any(|a| matches!(a.as_str(), "--wad" | "--list" | "--model" | "--index" | "--world" | "--world-probe" | "--placement-probe" | "--interior-probe" | "--interior-list" | "--entity-find" | "--comp-probe"));
    if wad_mode {
        let val = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|k| args.get(k + 1))
                .cloned()
        };
        let explicit = val("--wad").filter(|v| !v.is_empty() && !v.starts_with("--"));
        let wadpath = match explicit.or_else(wad::registry_vz_wad) {
            Some(p) => p,
            None => {
                eprintln!(
                    "no vz.wad found — pass --wad <path>, or install so that\n  \
                     HKLM\\SOFTWARE\\WOW6432Node\\EA Games\\Mercenaries 2 World in Flames\\Install Dir\n  \
                     resolves to a folder containing data\\vz.wad"
                );
                std::process::exit(1);
            }
        };
        eprintln!("vz.wad: {wadpath}");
        if args.iter().any(|a| a == "--meshes") {
            if let Err(e) = wad_meshes(&wadpath, val("--model")) {
                eprintln!("--meshes failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--poseoracle") {
            // Default 3 = rotation-driven (anim rotation + rigid bind offset), the correct compose:
            // clip translation is (0,0,0) on rotation-only tracks and applying it literally collapses
            // bones; keeping the bind offset matches the engine (visually confirmed, coherent idle).
            let conv = val("--conv").and_then(|c| c.parse::<u32>().ok()).unwrap_or(3);
            if let Err(e) = poseoracle(&wadpath, val("--model"), val("--index"), conv) {
                eprintln!("--poseoracle failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--animdiag") {
            if let Err(e) = animdiag(&wadpath, val("--model"), val("--index")) {
                eprintln!("--animdiag failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--animcheck") {
            if let Err(e) = animcheck(&wadpath, val("--model"), val("--index")) {
                eprintln!("--animcheck failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--trackmap") {
            let clip = val("--clip").and_then(|c| parse_hash(&c));
            if let Err(e) = trackmap(&wadpath, val("--model"), val("--index"), clip) {
                eprintln!("--trackmap failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--skincheck") {
            if let Err(e) = skincheck(&wadpath, val("--model"), val("--index")) {
                eprintln!("--skincheck failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        if args.iter().any(|a| a == "--list") {
            if let Err(e) = wad_list(&wadpath) {
                eprintln!("--list failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless terrain probe: parse the low_res world terrain and print verifiable counts.
        if args.iter().any(|a| a == "--world-probe") {
            if let Err(e) = world_probe(&wadpath) {
                eprintln!("--world-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless placement probe: parse layers_static (block 29), print counts/ranges + interior hunt.
        if args.iter().any(|a| a == "--placement-probe") {
            if let Err(e) = placement_probe(&wadpath) {
                eprintln!("--placement-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless interior probe: inspect PMC interior block 3490 (chunk inventory + per-mesh bbox).
        if args.iter().any(|a| a == "--interior-probe") {
            if let Err(e) = interior_probe(&wadpath) {
                eprintln!("--interior-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless interior-list: enumerate every model ASET, reverse-hash via the rainbow table,
        // filter to interior/hq/mainhall-named room-shell candidates + direct-test the template names.
        if args.iter().any(|a| a == "--interior-list") {
            if let Err(e) = interior_list(&wadpath) {
                eprintln!("--interior-list failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless entity-find: for the 6 PMC-interior building/recruit keys, scan candidate blocks
        // for each key's Transform + ModelName, resolve the mesh, and print the table (Task 1).
        if args.iter().any(|a| a == "--entity-find") {
            // Optional trailing keys (hex like 0x000d3c77); default = the 6 documented interior keys.
            let keys: Vec<u32> = args
                .iter()
                .filter_map(|a| a.strip_prefix("0x").and_then(|h| u32::from_str_radix(h, 16).ok()))
                .collect();
            if let Err(e) = entity_find(&wadpath, &keys) {
                eprintln!("--entity-find failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless COMP probe: enumerate every COMP in layers_static (29) + interior state (667),
        // reverse-scan their data for the anchor model hashes to find the entity->mesh COMP.
        if args.iter().any(|a| a == "--comp-probe") {
            if let Err(e) = comp_probe(&wadpath) {
                eprintln!("--comp-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Render the merged low_res world terrain under an elevated bird's-eye camera.
        if args.iter().any(|a| a == "--world") {
            if let Err(e) = run_world(&wadpath) {
                eprintln!("--world failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // ECS-driven scene path (mercs2_core spine + multi-model asset store): load one-or-more
        // models into the store and spawn entities; the animation system drives each entity's palette
        // through the World. `--model2 <hash>` adds a second distinct model beside the first.
        if args.iter().any(|a| a == "--ecs") {
            let clip_hash = val("--clip").and_then(|c| parse_hash(&c));
            let mut loaded: Vec<LoadedModel> = Vec::new();
            match load_from_wad(&wadpath, val("--model"), val("--index"), true, clip_hash) {
                Ok((verts, indices, draws, textures, skin, clip, hash, _title)) => {
                    loaded.push(LoadedModel { hash, verts, indices, draws, textures, skin, clips: clip.into_iter().collect() });
                }
                Err(e) => {
                    eprintln!("wad load (ecs) failed: {e}");
                    std::process::exit(1);
                }
            }
            if let Some(m2) = val("--model2").and_then(|m| parse_hash(&m)) {
                match load_from_wad(&wadpath, Some(format!("0x{m2:08X}")), None, true, None) {
                    Ok((verts, indices, draws, textures, skin, clip, hash, _)) => {
                        loaded.push(LoadedModel { hash, verts, indices, draws, textures, skin, clips: clip.into_iter().collect() });
                    }
                    Err(e) => eprintln!("--model2 0x{m2:08X} load failed: {e}"),
                }
            }
            pollster::block_on(run_scene_ecs(loaded, "Mercs 2 — scene (ECS)".to_string()));
            return;
        }

        let animate = args.iter().any(|a| a == "--animate");
        let clip_hash = val("--clip").and_then(|c| parse_hash(&c));
        match load_from_wad(&wadpath, val("--model"), val("--index"), animate, clip_hash) {
            Ok((verts, indices, draws, textures, skin, clip, _hash, title)) => {
                pollster::block_on(run_render(
                    verts, indices, draws, textures, skin, clip, animate, false, title,
                ))
            }
            Err(e) => {
                eprintln!("wad load failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // File model path, or the placeholder triangle.
    let model = args.iter().skip(1).find(|a| !a.starts_with("--")).cloned();
    let (verts, points, title) = match &model {
        Some(path) => match mesh::load_model_block(path) {
            Ok((v, s)) => {
                println!("loaded {} vertices / {} meshes from {path}", s.vertices, s.meshes);
                (v, true, "Mercenaries 2 — real WAD geometry (Phase 1b)".to_string())
            }
            Err(e) => {
                eprintln!("model load failed: {e}\nfalling back to placeholder triangle");
                (TRI.to_vec(), false, "Mercenaries 2 — engine skeleton (Phase 1)".to_string())
            }
        },
        None => (TRI.to_vec(), false, "Mercenaries 2 — engine skeleton (Phase 1)".to_string()),
    };
    pollster::block_on(run_render(
        verts,
        Vec::new(),
        Vec::new(),
        TexMap::new(),
        mesh::SkinData::identity(),
        None,
        false,
        points,
        title,
    ));
}

/// Default WAD block indices for the two terrain inputs (from the `00029_…` /
/// `03121_…` filenames). Verified/repaired at load time by `find_terrain_blocks`.
const LAYERS_STATIC_BLOCK: u16 = 29;
const LOW_RES_TERRAIN_BLOCK: u16 = 3121;

/// Decompress the low_res_terrain (3121) + layers_static (29) blocks, verifying the
/// expected signatures. If an index doesn't match, scan a bounded range of block
/// indices for the right one and log which index actually matched.
///
/// low_res_terrain block: `u32[0] == 401` and contains `b"UCFX"`.
/// layers_static block: contains `b"UCFX"` and the ascii `"LowResTerrainObject"`.
fn find_terrain_blocks(w: &mut wad::Wad) -> Result<(Vec<u8>, Vec<u8>), String> {
    fn is_low_res(b: &[u8]) -> bool {
        b.len() >= 4
            && u32::from_le_bytes([b[0], b[1], b[2], b[3]]) == 401
            && b.windows(4).any(|w| w == b"UCFX")
    }
    fn is_layers_static(b: &[u8]) -> bool {
        b.windows(4).any(|w| w == b"UCFX")
            && b.windows(19).any(|w| w == b"LowResTerrainObject")
    }

    // low_res_terrain (3121).
    let low = wad::decompress_block_index(w, LOW_RES_TERRAIN_BLOCK).ok().filter(|b| is_low_res(b));
    let (low, low_idx) = match low {
        Some(b) => (b, LOW_RES_TERRAIN_BLOCK),
        None => {
            eprintln!(
                "[world] block {LOW_RES_TERRAIN_BLOCK} is not low_res_terrain (u32[0]!=401 or no UCFX); scanning…"
            );
            let mut found = None;
            for idx in 0..12000u16 {
                if let Ok(b) = wad::decompress_block_index(w, idx) {
                    if is_low_res(&b) {
                        found = Some((b, idx));
                        break;
                    }
                }
            }
            found.ok_or("no block matched low_res_terrain signature (u32[0]==401 + UCFX)")?
        }
    };
    if low_idx != LOW_RES_TERRAIN_BLOCK {
        eprintln!("[world] low_res_terrain actually at block {low_idx} (expected {LOW_RES_TERRAIN_BLOCK})");
    } else {
        eprintln!("[world] low_res_terrain block {low_idx}: OK (u32[0]==401, UCFX present)");
    }

    // layers_static (29).
    let ls = wad::decompress_block_index(w, LAYERS_STATIC_BLOCK).ok().filter(|b| is_layers_static(b));
    let (ls, ls_idx) = match ls {
        Some(b) => (b, LAYERS_STATIC_BLOCK),
        None => {
            eprintln!(
                "[world] block {LAYERS_STATIC_BLOCK} is not layers_static (no UCFX/LowResTerrainObject); scanning…"
            );
            let mut found = None;
            for idx in 0..12000u16 {
                if let Ok(b) = wad::decompress_block_index(w, idx) {
                    if is_layers_static(&b) {
                        found = Some((b, idx));
                        break;
                    }
                }
            }
            found.ok_or("no block matched layers_static signature (UCFX + LowResTerrainObject)")?
        }
    };
    if ls_idx != LAYERS_STATIC_BLOCK {
        eprintln!("[world] layers_static actually at block {ls_idx} (expected {LAYERS_STATIC_BLOCK})");
    } else {
        eprintln!("[world] layers_static block {ls_idx}: OK (UCFX + LowResTerrainObject present)");
    }

    Ok((low, ls))
}

fn parse_hash(s: &str) -> Option<u32> {
    let s = s.trim();
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u32::from_str_radix(s, 16).ok()
}

/// Enumerate + measure every model in the WAD (headless); flag likely humanoids by bbox.
fn wad_list(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    eprintln!("{} model assets in {wadpath}", models.len());
    let (mut ok, mut human) = (0u32, 0u32);
    for (hash, block) in &models {
        let container = match wad::extract_container(&mut w, *hash) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Ok((_v, s)) = mesh::build_from_container(&container) {
            let yh = s.bbox_max[1] - s.bbox_min[1];
            let xw = s.bbox_max[0] - s.bbox_min[0];
            let humanoid = (1.4..2.3).contains(&yh) && xw < 1.6 && s.vertices > 800;
            println!(
                "0x{hash:08X} block={block:<5} meshes={:<3} verts={:<6} yheight={yh:6.2} xwidth={xw:6.2}{}",
                s.meshes,
                s.vertices,
                if humanoid { "  <-- humanoid?" } else { "" }
            );
            ok += 1;
            if humanoid {
                human += 1;
            }
        }
    }
    eprintln!("measured {ok} models; {human} look humanoid");
    Ok(())
}

/// Isolation test: render the model in the ENGINE'S EXACT captured pose (from the x32dbg oracle,
/// clip 0x24F8C8E6 frame-pos 1.496), fed through OUR compose/skin. If Mattias reads as a coherent
/// posed human, our compose/skin/shader is correct and the scramble is a DECODE problem; if it
/// scrambles here too, the compose/skin convention (quat->matrix / Havok RH vs game LH / palette)
/// is wrong and must be grounded in the decomp. Applies the FULL captured transform per track.
fn poseoracle(wadpath: &str, model: Option<String>, index: Option<String>, conv: u32) -> Result<(), String> {
    use mercs2_formats::anim::QsTransform;
    use mercs2_formats::animgroup::parse_animgroup;
    use mercs2_formats::skeleton::mat4_mul;

    // The 64 captured hkQsTransforms (48 bytes each: translation[4], rotation[4] xyzw, scale[4]).
    let raw: &[u8] = include_bytes!("../../mercs2_formats/tests/fixtures/oracle_pose.bin");
    let f = |o: usize| f32::from_le_bytes([raw[o], raw[o + 1], raw[o + 2], raw[o + 3]]);
    let pose: Vec<QsTransform> = (0..raw.len() / 48)
        .map(|i| {
            let b = i * 48;
            QsTransform {
                translation: [f(b), f(b + 4), f(b + 8)],
                rotation: [f(b + 16), f(b + 20), f(b + 24), f(b + 28)],
                scale: [f(b + 32), f(b + 36), f(b + 40)],
            }
        })
        .collect();

    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, indices, draws, s) = mesh::build_indexed_from_container(&container)?;
    let mut skin = s.skin_data();
    if skin.rig.is_empty() {
        return Err("model has no skeleton".into());
    }
    let hier: Vec<u32> = skin.rig.iter().map(|b| b.name_hash).collect();

    // Track->HIER binding for the captured clip 0x24F8C8E6, plus its transform-track count (the
    // 64-slot buffer is 61 transform + 3 float tracks; only transform tracks drive bones).
    let mut binding: Option<Vec<Option<usize>>> = None;
    let mut num_transform_tracks = 0usize;
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        if let Some(c) = ag.clips.iter().find(|c| c.name_hash == 0x24F8C8E6) {
            binding = Some(c.binding.resolve_to_hier(&hier));
            num_transform_tracks = c.num_transform_tracks as usize;
            break;
        }
    }
    let binding = binding.ok_or("clip 0x24F8C8E6 not found in any animgroup")?;
    let _ = conv;

    // Faithful Havok sampleAndCombine: bind base, driven transform-track bones take the full sampled
    // hkQsTransform (the sample carries the real bone offsets in T); model-space compose + skin.
    println!("  Havok combine: {} transform tracks over {} bind poses", num_transform_tracks, skin.rig.len());
    skin.bones = pose::havok_palette(&skin.rig, &pose, &binding, num_transform_tracks);

    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(&mut w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let resolved = binding.iter().filter(|x| x.is_some()).count();
    println!(
        "poseoracle: model 0x{hash:08X}, {} tracks -> HIER bones; rendering the engine's captured pose",
        resolved
    );

    // Pinpoint the worst-displaced bones (the head-scramble culprits): CPU-skin, find bones whose
    // verts land far from the body centre.
    {
        use mercs2_formats::skeleton::transform_point;
        let pal = &skin.bones;
        let (mut mean, mut nv) = ([0.0f32; 3], 0.0f32);
        for v in &verts {
            let wi = v.weights.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
            let b = v.joints[wi] as usize;
            if b >= pal.len() { continue; }
            let p = transform_point(&pal[b], v.pos);
            for j in 0..3 { mean[j] += p[j]; }
            nv += 1.0;
        }
        for j in 0..3 { mean[j] /= nv.max(1.0); }
        let mut per_bone_max = vec![0.0f32; skin.rig.len()];
        for v in &verts {
            let wi = v.weights.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
            let b = v.joints[wi] as usize;
            if b >= pal.len() { continue; }
            let p = transform_point(&pal[b], v.pos);
            let d = ((p[0]-mean[0]).powi(2)+(p[1]-mean[1]).powi(2)+(p[2]-mean[2]).powi(2)).sqrt();
            if d > per_bone_max[b] { per_bone_max[b] = d; }
        }
        let mut ranked: Vec<(usize,f32)> = per_bone_max.iter().copied().enumerate().collect();
        ranked.sort_by(|a,b| b.1.total_cmp(&a.1));
        // which track drives each bone
        let bone_track: std::collections::HashMap<usize,usize> = binding.iter().enumerate()
            .filter_map(|(t,b)| b.map(|bb| (bb,t))).collect();
        println!("  worst-displaced bones (bone d hash parent track):");
        for (b,d) in ranked.iter().take(10) {
            println!("    bone{b:<3} d={d:6.2} hash=0x{:08X} parent={:<3} track={:?}",
                skin.rig[*b].name_hash, skin.rig[*b].parent, bone_track.get(b));
        }
    }
    pollster::block_on(run_render(
        verts,
        indices,
        draws,
        textures,
        skin,
        None,
        false,
        false,
        format!("Mercs 2 — ENGINE CAPTURED POSE (0x{hash:08X})"),
    ));
    Ok(())
}

/// Skinning-convention diagnostic (headless). CPU-skins the mesh at frame 0 under several bone-matrix
/// variants and reports each resulting bbox vs the known-good BIND pose. The variant whose extent +
/// centroid match the bind pose reveals the correct rotation/root convention — measured, not guessed.
fn animdiag(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    use mercs2_formats::skeleton::transform_point;

    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
    // Find the animgroup whose clips best cover this HIER, and inspect EVERY clip — to reveal whether
    // the spikes are clip-specific (e.g. the full-body/additive clip) or universal.
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best_blk: Option<(u16, usize)> = None;
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        let total: usize = ag
            .clips
            .iter()
            .map(|c| c.binding.resolve_to_hier(&hier).iter().filter(|r| r.is_some()).count())
            .sum();
        if best_blk.map_or(true, |(_, t)| total > t) {
            best_blk = Some((blk, total));
        }
    }
    let (blk, _) = best_blk.ok_or("no animgroup matched this model")?;
    let data = wad::decompress_block_index(&mut w, blk)?;
    let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup: {e}"))?;

    // CPU-skin bbox extent for a palette (mirrors the shader LBS).
    let extent_of = |pal: &[[[f32; 4]; 4]]| -> f32 {
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in &verts {
            let wsum: f32 = v.weights.iter().map(|&x| x as f32).sum::<f32>().max(1.0);
            let mut acc = [0.0f32; 3];
            for k in 0..4 {
                let wk = v.weights[k] as f32 / wsum;
                if wk <= 0.0 { continue; }
                let b = v.joints[k] as usize;
                if b >= pal.len() { continue; }
                let p = transform_point(&pal[b], v.pos);
                for j in 0..3 { acc[j] += wk * p[j]; }
            }
            for j in 0..3 { lo[j] = lo[j].min(acc[j]); hi[j] = hi[j].max(acc[j]); }
        }
        (0..3).map(|j| hi[j] - lo[j]).fold(0.0f32, f32::max)
    };
    let bind_extent = extent_of(&pose::palette(&s.rig, &pose::bind_locals(&s.rig)));
    println!(
        "model 0x{hash:08X}: {} bones, {} verts; animgroup block[{blk}], {} clips; BIND extent {bind_extent:.3}",
        s.rig.len(), verts.len(), ag.clips.len()
    );

    // Rotation-driven locals for a clip sample (matches shipping animate_locals): xyzw absolute
    // rotation, rigid bind offset, root at bind.
    let times = [0.0f32, 0.12, 0.25, 0.37, 0.5, 0.62, 0.75, 0.87];
    let locals_at = |ac: &mercs2_formats::anim::AnimClip, tth: &[Option<usize>], t: f32| -> Vec<[[f32; 4]; 4]> {
        let sample = ac.sample_local(t * ac.duration.max(1e-3));
        let mut locals = pose::bind_locals(&s.rig);
        for (track, bone) in tth.iter().enumerate() {
            if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
                if b >= locals.len() || s.rig[b].parent < 0 { continue; }
                let mut m = pose::qs_to_local(qs);
                let lb = s.rig[b].local_bind;
                m[3] = [lb[3][0], lb[3][1], lb[3][2], 1.0];
                locals[b] = m;
            }
        }
        locals
    };

    println!("  per-clip max bbox extent (want ~{bind_extent:.2}; >1.4x = spikes):");
    for c in &ag.clips {
        let Ok(ac) = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]) else { continue };
        if !ac.decoded { continue; }
        let tth = c.binding.resolve_to_hier(&hier);
        let resolved = tth.iter().filter(|r| r.is_some()).count();
        let mut max_e = 0.0f32;
        for &t in &times {
            let e = extent_of(&pose::palette(&s.rig, &locals_at(&ac, &tth, t)));
            if e > max_e { max_e = e; }
        }
        let tag = if max_e < 1.4 * bind_extent { "  <- CLEAN" } else { "" };
        println!(
            "    clip 0x{:08X}  {:>3}t {:>3}res  {:>5.2}s  max extent={max_e:.3}{tag}",
            c.name_hash, ac.num_tracks, resolved, ac.duration
        );
    }
    Ok(())
}

/// Animation coordinate-consistency gate (headless). Retail ships no referencePose, so clip local
/// transforms must be authored in the SAME frame as the mesh HIER bind locals. Decisive check: for
/// every bone a track drives, the animated LOCAL translation (bone offset from parent) must equal the
/// HIER bind-local translation — bones rotate but don't stretch. A near-zero delta proves the clip
/// data drops straight into `pose::palette` with no coordinate conversion; a large/negated delta
/// would reveal a handedness fix is needed. Finds the animgroup whose binding best matches this model.
fn animcheck(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    use mercs2_formats::animgroup::parse_animgroup;

    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (_v, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton (rig empty)".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();

    // Pass 1: pick the animgroup + clip whose binding resolves the most tracks onto this HIER.
    let mut best: Option<(u16, u32, usize)> = None; // (block, clip name_hash, resolved count)
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            let resolved = c
                .binding
                .resolve_to_hier(&hier)
                .iter()
                .filter(|r| r.is_some())
                .count();
            if best.map_or(true, |(_, _, r)| resolved > r) {
                best = Some((blk, c.name_hash, resolved));
            }
        }
    }
    let (blk, clip_hash, resolved) = best.ok_or("no animgroup binding matched this model")?;
    println!("model 0x{hash:08X}: {} bones; best animgroup block[{blk}] clip 0x{clip_hash:08X} ({resolved} tracks resolve to HIER)", s.rig.len());

    // Pass 2: decode that clip and compare its frame-0 local translations to the HIER bind locals.
    let data = wad::decompress_block_index(&mut w, blk)?;
    let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup: {e}"))?;
    let clip = ag
        .clips
        .iter()
        .find(|c| c.name_hash == clip_hash)
        .ok_or("clip vanished on re-parse")?;
    let pk = &data[clip.havok_offset..];
    let ac = mercs2_formats::anim::parse_anim(pk).map_err(|e| format!("parse_anim: {e}"))?;
    println!(
        "clip: class={} decoded={} tracks={} frames={} duration={:.3}",
        clip.class, ac.decoded, ac.num_tracks, ac.num_frames, ac.duration
    );
    if !ac.decoded {
        return Err(format!("clip not decoded (class {}) — cannot check", clip.class));
    }

    let track_to_hier = clip.binding.resolve_to_hier(&hier);
    let sample = ac.sample_local(0.0);

    let (mut n, mut sum_d, mut max_d, mut sum_off) = (0u32, 0.0f32, 0.0f32, 0.0f32);
    let mut worst: Vec<(usize, f32)> = Vec::new();
    for (track, bone) in track_to_hier.iter().enumerate() {
        let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) else { continue };
        if s.rig[b].parent < 0 {
            continue; // root translation is motion, not a fixed bone offset
        }
        let at = qs.translation;
        let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
        let d = ((at[0] - bt[0]).powi(2) + (at[1] - bt[1]).powi(2) + (at[2] - bt[2]).powi(2)).sqrt();
        let off = (bt[0] * bt[0] + bt[1] * bt[1] + bt[2] * bt[2]).sqrt();
        n += 1;
        sum_d += d;
        sum_off += off;
        if d > max_d {
            max_d = d;
        }
        worst.push((b, d));
    }
    if n == 0 {
        return Err("no non-root driven bones to compare".into());
    }
    // Hypothesis test: the decoded wavelet tracks may be offset by one relative to the trnm binding.
    // Compare sample[track] against the bone that binding[track+1] names, and against [track-1].
    let mut shift_next = (0u32, 0.0f32); // sample[N] vs bone(binding[N+1])
    let mut shift_prev = (0u32, 0.0f32); // sample[N] vs bone(binding[N-1])
    for (track, qs) in sample.iter().enumerate() {
        for (delta, acc) in [(1i32, &mut shift_next), (-1i32, &mut shift_prev)] {
            let j = track as i32 + delta;
            if j < 0 || j as usize >= track_to_hier.len() {
                continue;
            }
            let Some(&b) = track_to_hier[j as usize].as_ref() else { continue };
            if s.rig[b].parent < 0 {
                continue;
            }
            let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
            let d = ((qs.translation[0] - bt[0]).powi(2)
                + (qs.translation[1] - bt[1]).powi(2)
                + (qs.translation[2] - bt[2]).powi(2))
            .sqrt();
            acc.0 += 1;
            acc.1 += d;
        }
    }

    let mean_d = sum_d / n as f32;
    let mean_off = sum_off / n as f32;
    if shift_next.0 > 0 {
        println!(
            "  SHIFT TEST: aligned mean|Δ|={mean_d:.6}  |  sample[N]vs binding[N+1] mean|Δ|={:.6}  |  vs binding[N-1] mean|Δ|={:.6}",
            shift_next.1 / shift_next.0 as f32,
            shift_prev.1 / shift_prev.0.max(1) as f32
        );
    }
    worst.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!(
        "translation delta (anim local vs HIER bind local), {n} non-root driven bones:"
    );
    println!("  mean |Δ| = {mean_d:.6}   max |Δ| = {max_d:.6}   (mean bone offset = {mean_off:.4})");
    // Correctness gate = BINDING ALIGNMENT, not bind-equality: the animation is authored in the
    // HIER frame iff the aligned translation delta is clearly the smallest of {N-1, N, N+1}. (A
    // straight rel<threshold gate is confounded — a clip genuinely moves some bones in frame 0, so
    // aligned |Δ| is never zero; but a one-track misbinding makes a neighbour shift fit better.)
    let d_next = shift_next.1 / shift_next.0.max(1) as f32;
    let d_prev = shift_prev.1 / shift_prev.0.max(1) as f32;
    let aligned_best = mean_d < 0.7 * d_next && mean_d < 0.7 * d_prev;
    println!(
        "  aligned mean Δ = {mean_d:.4} vs shift±1 [{d_prev:.4}, {d_next:.4}]  ->  {}",
        if aligned_best {
            "GATE PASS — track↔bone binding is aligned (clip authored in HIER frame)"
        } else {
            "GATE FAIL — a neighbouring shift fits better; binding is misaligned"
        }
    );
    print!("  worst bones (Δ):");
    for (b, d) in worst.iter().take(4) {
        print!(" bone{b}={d:.4}");
    }
    println!();

    // Raw side-by-side dump (anim frame-0 local T/R vs HIER bind-local T) to reveal the relationship
    // (rotation-only? scaled? negated component? mapping off?) without guessing.
    println!("  --- raw anim-vs-bind for first 6 driven non-root bones ---");
    let mut shown = 0;
    for (track, bone) in track_to_hier.iter().enumerate() {
        let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) else { continue };
        if s.rig[b].parent < 0 {
            continue;
        }
        let bt = [s.rig[b].local_bind[3][0], s.rig[b].local_bind[3][1], s.rig[b].local_bind[3][2]];
        println!(
            "    track{track:>3}->bone{b:<3} animT=[{:+.4},{:+.4},{:+.4}] bindT=[{:+.4},{:+.4},{:+.4}] animR=[{:+.3},{:+.3},{:+.3},{:+.3}]",
            qs.translation[0], qs.translation[1], qs.translation[2],
            bt[0], bt[1], bt[2],
            qs.rotation[0], qs.rotation[1], qs.rotation[2], qs.rotation[3]
        );
        shown += 1;
        if shown >= 6 {
            break;
        }
    }

    // Full render-path sanity: build the animated palette at mid-clip and confirm every Skin matrix
    // is finite and bounded (Skin translation = per-bone displacement from bind; a blow-up = NaN or
    // huge values). This exercises sample_local -> animate_locals -> palette exactly as render() does.
    let sample_mid = ac.sample_local(ac.duration * 0.5);
    let locals = pose::animate_locals(&s.rig, &sample_mid, &track_to_hier);
    let pal = pose::palette(&s.rig, &locals);
    let mut finite = true;
    let mut max_t = 0.0f32;
    for m in &pal {
        for row in m {
            for &v in row {
                if !v.is_finite() {
                    finite = false;
                }
            }
        }
        let t = (m[3][0].powi(2) + m[3][1].powi(2) + m[3][2].powi(2)).sqrt();
        max_t = max_t.max(t);
    }
    let extent = (0..3).map(|k| s.bbox_max[k] - s.bbox_min[k]).fold(0.0f32, f32::max);
    println!(
        "animated palette @{:.2}s: finite={finite}  max|Skin T|={max_t:.3}  (model extent ~{extent:.2})  ->  {}",
        ac.duration * 0.5,
        if finite && max_t < 4.0 * extent.max(0.25) {
            "SANE (render path bounded)"
        } else {
            "UNSTABLE — investigate before rendering"
        }
    );
    Ok(())
}

/// Best-effort bone-name resolution from the repo rainbow table (tools/rainbow_table.json).
/// Returns hash -> first candidate name for exactly the hashes asked for; empty map if the
/// table is absent (the diagnostic still prints hashes).
fn rainbow_names(hashes: &std::collections::BTreeSet<u32>) -> std::collections::HashMap<u32, String> {
    let mut out = std::collections::HashMap::new();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../rainbow_table.json");
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    for &h in hashes {
        let key = format!("\"0x{h:08X}\"");
        let Some(p) = text.find(&key) else { continue };
        let rest = &text[p + key.len()..];
        let Some(q0) = rest.find('"') else { continue };
        let Some(q1) = rest[q0 + 1..].find('"') else { continue };
        out.insert(h, rest[q0 + 1..q0 + 1 + q1].to_string());
    }
    out
}

/// Per-track binding audit (headless). For `--clip <hash>` on this model, prints — for EVERY
/// animgroup block containing that clip — the raw `trnm` words read back from the block bytes
/// (count, leading word, size check), the Havok header track counts, and a per-track table:
/// track index, raw binding name-hash, resolved HIER bone index (+ name/parent/bind position),
/// or UNRESOLVED. Also lists HIER bones driven by no track and bones driven by more than one.
fn trackmap(wadpath: &str, model: Option<String>, index: Option<String>, want: Option<u32>) -> Result<(), String> {
    use mercs2_formats::animgroup::parse_animgroup;
    use mercs2_formats::skeleton::mat4_mul;
    use mercs2_formats::ucfx::parse_block_entry_table;

    let clip_hash = want.ok_or("--trackmap requires --clip <hash>")?;
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _i, _d, s) = mesh::build_indexed_from_container(&container)?;
    if s.rig.is_empty() {
        return Err("model has no skeleton".into());
    }
    let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();

    // Bind-pose world position per bone (world = local · world_parent, row-vector).
    let mut world = vec![[[0.0f32; 4]; 4]; s.rig.len()];
    for b in 0..s.rig.len() {
        world[b] = if s.rig[b].parent < 0 {
            s.rig[b].local_bind
        } else {
            mat4_mul(&s.rig[b].local_bind, &world[s.rig[b].parent as usize])
        };
    }

    // Names for every HIER hash + every trnm hash we encounter (collected below in pass 1).
    let mut wanted: std::collections::BTreeSet<u32> = hier.iter().copied().collect();
    let mut hits: Vec<(u16, Vec<u32>)> = Vec::new(); // (block, trnm hashes)
    for blk in wad::animgroup_blocks(&w) {
        let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        if let Some(c) = ag.clips.iter().find(|c| c.name_hash == clip_hash) {
            wanted.extend(c.binding.track_to_bone_hash.iter().copied());
            hits.push((blk, c.binding.track_to_bone_hash.clone()));
        }
    }
    if hits.is_empty() {
        return Err(format!("clip 0x{clip_hash:08X} not found in any animgroup"));
    }
    let names = rainbow_names(&wanted);
    let nm = |h: u32| names.get(&h).map(String::as_str).unwrap_or("?");

    println!("model 0x{hash:08X}: {} HIER bones", s.rig.len());
    for (b, bone) in s.rig.iter().enumerate() {
        println!(
            "  bone{b:<3} hash=0x{:08X} parent={:<3} bindpos=[{:+7.3},{:+7.3},{:+7.3}]  {}",
            bone.name_hash, bone.parent, world[b][3][0], world[b][3][1], world[b][3][2], nm(bone.name_hash)
        );
    }

    // QS bind-identity gate: recompose the palette through the hkQsTransform path with NO
    // tracks driven (exactly what havok_palette does to undriven bones). Every Skin matrix
    // must be identity; a deviation marks a local_bind that does not survive the
    // mat_to_qs -> qs_mul -> qs_to_local roundtrip (mirror/shear/non-TRS local).
    {
        let qs_pal = pose::havok_palette(&s.rig, &[], &[], 0);
        let mut bad: Vec<(usize, f32, f32)> = Vec::new(); // (bone, max|Skin-I|, det3)
        for (b, m) in qs_pal.iter().enumerate() {
            let mut dev = 0.0f32;
            for r in 0..4 {
                for c in 0..4 {
                    let ident = if r == c { 1.0 } else { 0.0 };
                    dev = dev.max((m[r][c] - ident).abs());
                }
            }
            if dev > 1e-3 {
                let lb = s.rig[b].local_bind;
                let det = lb[0][0] * (lb[1][1] * lb[2][2] - lb[1][2] * lb[2][1])
                    - lb[0][1] * (lb[1][0] * lb[2][2] - lb[1][2] * lb[2][0])
                    + lb[0][2] * (lb[1][0] * lb[2][1] - lb[1][1] * lb[2][0]);
                bad.push((b, dev, det));
            }
        }
        println!("QS bind-identity gate: {} / {} bones deviate > 1e-3 through the undriven-bone (bind_qs) path", bad.len(), s.rig.len());
        for (b, dev, det) in &bad {
            println!(
                "    bone{b:<3} hash=0x{:08X} parent={:<3} |Skin-I|={dev:.4} det(local_bind)={det:+.4}  {}",
                s.rig[*b].name_hash, s.rig[*b].parent, nm(s.rig[*b].name_hash)
            );
        }
    }

    // Vertex->joint plausibility per drawing group: a skinned vertex should sit near its dominant
    // bone's bind position. If a group's BLENDINDICES are NOT global HIER indices (e.g. a
    // per-group palette), its verts land far from the bones they claim — invisible at bind
    // (identity palette) but exploding under animation.
    {
        let meshes = mercs2_formats::model_cubeize::read_model_meshes(&container)
            .map_err(|e| format!("read_model_meshes: {e}"))?;
        let dist = |p: &[f32; 3], j: usize| -> f32 {
            let w = &world[j.min(world.len() - 1)];
            ((p[0] - w[3][0]).powi(2) + (p[1] - w[3][1]).powi(2) + (p[2] - w[3][2]).powi(2)).sqrt()
        };
        println!("vertex->joint distance per skinned group (dom = weight-dominant joint; min = best of the 4):");
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            let (mut jmin, mut jmax) = (255u8, 0u8);
            let (mut sum_dom, mut sum_min, mut mx_min, mut nfar, mut n) = (0.0f32, 0.0f32, 0.0f32, 0usize, 0usize);
            for (vi, p) in m.positions.iter().enumerate() {
                let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                for k in 0..4 {
                    if w4[k] > 0 {
                        jmin = jmin.min(j4[k]);
                        jmax = jmax.max(j4[k]);
                    }
                }
                let d_dom = dist(p, j4[wi] as usize);
                let d_min = (0..4)
                    .filter(|&k| w4[k] > 0)
                    .map(|k| dist(p, j4[k] as usize))
                    .fold(f32::INFINITY, f32::min);
                sum_dom += d_dom;
                sum_min += d_min;
                mx_min = mx_min.max(d_min);
                if d_min > 0.5 {
                    nfar += 1;
                }
                n += 1;
            }
            println!(
                "  group{:<3} sub{:<2} verts={:<6} joints[{jmin}..{jmax}] mean_dom={:.3} mean_min={:.3} max_min={:.3} far={}",
                m.group_index, m.sub_object, n,
                sum_dom / n.max(1) as f32, sum_min / n.max(1) as f32, mx_min, nfar
            );
        }
        // Sample verts from the face region (y > 1.6): their 4 joints AS-IF-global vs their position.
        println!("face-region vertex samples (pos, joints, weights):");
        let mut shown = 0;
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            for (vi, p) in m.positions.iter().enumerate() {
                if p[1] > 1.65 && shown < 10 {
                    let j4 = m.joints[vi];
                    let w4 = m.weights[vi];
                    println!(
                        "  group{:<3} v{vi:<5} pos=[{:+6.3},{:+6.3},{:+6.3}] joints={:?} weights={:?}",
                        m.group_index, p[0], p[1], p[2], j4, w4
                    );
                    shown += 1;
                }
            }
        }
        // Descriptor-tag census of the model container: reveals any candidate per-group
        // bone-palette chunk the reader currently ignores.
        {
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            let mut tags: Vec<String> = Vec::new();
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8);
                let marker = if u0 == 0xFFFF_FFFF { "*" } else { "" };
                tags.push(format!("{}{}({sz})", String::from_utf8_lossy(t), marker));
            }
            println!("container descriptor rows ({n_desc}): {}", tags.join(" "));
        }
        // Hexdump candidate palette carriers: the GEOM INDX, each PRMG INFO(56), each PRMT body,
        // and each SKIN INFO(4) — one of these must carry the per-group bone palette.
        {
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            let mut dumped_info = 0;
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t: [u8; 4] = container[row..row + 4].try_into().unwrap();
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                if u0 == 0xFFFF_FFFF {
                    continue;
                }
                let start = data_off + u0 as usize;
                let hex = |n: usize| -> String {
                    container[start..(start + n.min(sz)).min(container.len())]
                        .chunks(4)
                        .map(|c| c.iter().map(|b| format!("{b:02x}")).collect::<String>())
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                match &t {
                    b"INDX" => println!("  INDX({sz}) @0x{start:x}: {}", hex(sz)),
                    b"INFO" if sz >= 56 && sz <= 60 && dumped_info < 6 => {
                        println!("  groupINFO({sz}) @0x{start:x}: {}", hex(sz));
                        dumped_info += 1;
                    }
                    b"PRMT" if dumped_info <= 8 => println!("  PRMT({sz}) @0x{start:x}: {}", hex(sz)),
                    _ => {}
                }
            }
        }
        // Base hypothesis check: BLENDINDICES look BASE-RELATIVE (global = slot + base), base =
        // u16 at the group's PRMG INFO(56/60) offset +24, count = u16 at +26. Verify per group:
        // read that field, brute-force the base that minimizes the mean vertex->bone distance,
        // and print both plus the distance at each.
        {
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            // group_index (PRMG ordinal) -> (info_base, info_count) from the INFO row after PRMG.
            let mut info_bases: Vec<(u16, u16)> = Vec::new();
            let mut want_info = false;
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                if t == b"PRMG" && u0 == 0xFFFF_FFFF {
                    want_info = true;
                    continue;
                }
                if want_info && t == b"INFO" && u0 != 0xFFFF_FFFF {
                    let start = data_off + u0 as usize;
                    let base = u16::from_le_bytes([container[start + 24], container[start + 25]]);
                    let cnt = u16::from_le_bytes([container[start + 26], container[start + 27]]);
                    // Full range table: +20 u32 range_count, +24 pairs (u16 base, u16 count).
                    let rc = mercs2_formats::ffcs::read_u32_le(&container, start + 20) as usize;
                    let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                    let mut pairs: Vec<(u16, u16)> = Vec::new();
                    for r in 0..rc.min((sz.saturating_sub(24)) / 4) {
                        let o = start + 24 + r * 4;
                        pairs.push((
                            u16::from_le_bytes([container[o], container[o + 1]]),
                            u16::from_le_bytes([container[o + 2], container[o + 3]]),
                        ));
                    }
                    let total: u32 = pairs.iter().map(|&(_, c)| c as u32).sum();
                    println!(
                        "  PRMG#{} INFO range table: rc={rc} pairs={pairs:?} total_slots={total}",
                        info_bases.len()
                    );
                    info_bases.push((base, cnt));
                    want_info = false;
                }
            }
            println!("per-group INFO(+24) base/count vs brute-force best base:");
            for m in &meshes {
                if m.rigid || m.joints.is_empty() {
                    continue;
                }
                let (info_base, info_cnt) = info_bases.get(m.group_index).copied().unwrap_or((0xFFFF, 0));
                let jmax = m
                    .joints
                    .iter()
                    .zip(&m.weights)
                    .flat_map(|(j4, w4)| (0..4).filter(|&k| w4[k] > 0).map(|k| j4[k]))
                    .max()
                    .unwrap_or(0) as usize;
                let mean_at = |base: usize| -> f32 {
                    let (mut sum, mut n) = (0.0f32, 0usize);
                    for (vi, p) in m.positions.iter().enumerate() {
                        let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                        let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                        let g = j4[wi] as usize + base;
                        if g >= world.len() {
                            return f32::INFINITY;
                        }
                        sum += dist(p, g);
                        n += 1;
                    }
                    sum / n.max(1) as f32
                };
                let mut best = (0usize, f32::INFINITY);
                for base in 0..world.len().saturating_sub(jmax) {
                    let d = mean_at(base);
                    if d < best.1 {
                        best = (base, d);
                    }
                }
                println!(
                    "  group{:<3} sub{:<2} jmax={jmax:<3} INFO base={info_base:<3} count={info_cnt:<3} d(info)={:.3} | best base={} d={:.3}",
                    m.group_index, m.sub_object,
                    if (info_base as usize) < world.len() { mean_at(info_base as usize) } else { f32::INFINITY },
                    best.0, best.1
                );
            }
        }
        // SEGM records + per-group PRMT primitive records (seg ref @0, vertex range @12), then a
        // per-PRIMITIVE base solve: primitives partition the vertex buffer and may carry their
        // own bone-window base via the SEGM they reference.
        {
            let segm = mercs2_formats::model_cubeize::parse_segm(&container);
            println!("SEGM records ({}):", segm.len());
            for (i, r) in segm.iter().enumerate() {
                println!("  seg{i:<2} bone={:<3} seg_id={} state=0x{:02x}  ({})", r.bone, r.seg_id, r.state_mask, nm(s.rig.get(r.bone as usize).map(|b| b.name_hash).unwrap_or(0)));
            }
            let data_off = mercs2_formats::ffcs::read_u32_le(&container, 4) as usize;
            let n_desc = mercs2_formats::ffcs::read_u32_le(&container, 16) as usize;
            // Collect PRMT bodies in PRMG order (one PRMT row per group, after the IBUF).
            let mut prmts: Vec<Vec<[u32; 4]>> = Vec::new();
            for i in 0..n_desc.min((container.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                let t = &container[row..row + 4];
                let u0 = mercs2_formats::ffcs::read_u32_le(&container, row + 4);
                if t == b"PRMT" && u0 != 0xFFFF_FFFF {
                    let start = data_off + u0 as usize;
                    let sz = mercs2_formats::ffcs::read_u32_le(&container, row + 8) as usize;
                    let recs: Vec<[u32; 4]> = (0..sz / 16)
                        .map(|k| {
                            let o = start + k * 16;
                            [
                                mercs2_formats::ffcs::read_u32_le(&container, o),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 4),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 8),
                                mercs2_formats::ffcs::read_u32_le(&container, o + 12),
                            ]
                        })
                        .collect();
                    prmts.push(recs);
                }
            }
            println!("per-group per-PRIMITIVE base solve (seg ref, vert range, best base, d):");
            for m in &meshes {
                if m.rigid || m.joints.is_empty() {
                    continue;
                }
                let Some(recs) = prmts.get(m.group_index) else { continue };
                println!("  group{} sub{} ({} prims):", m.group_index, m.sub_object, recs.len());
                for r in recs {
                    let seg = r[0] as usize;
                    let vmax = (r[3] & 0xFFFF) as usize;
                    let vnum = (r[3] >> 16) as usize;
                    let v0 = (vmax + 1).saturating_sub(vnum);
                    let mut jmax = 0usize;
                    let mean_at = |base: usize, jmax: &mut usize| -> f32 {
                        let (mut sum, mut n) = (0.0f32, 0usize);
                        for vi in v0..=vmax.min(m.positions.len().saturating_sub(1)) {
                            let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                            let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                            let sl = j4[wi] as usize;
                            *jmax = (*jmax).max(sl);
                            let g = sl + base;
                            if g >= world.len() {
                                return f32::INFINITY;
                            }
                            sum += dist(&m.positions[vi], g);
                            n += 1;
                        }
                        sum / n.max(1) as f32
                    };
                    let mut best = (0usize, f32::INFINITY);
                    for base in 0..world.len() {
                        let mut jm = 0usize;
                        let d = mean_at(base, &mut jm);
                        jmax = jm;
                        if d < best.1 {
                            best = (base, d);
                        }
                    }
                    let seg_bone = segm.get(seg).map(|r| r.bone).unwrap_or(0xFFFF);
                    println!(
                        "    prim seg={seg}(bone {seg_bone}) verts {v0}..={vmax} jmax={jmax} best base={} d={:.3} d(segbone)={:.3}",
                        best.0,
                        best.1,
                        {
                            let mut jm = 0usize;
                            if (seg_bone as usize) < world.len() { mean_at(seg_bone as usize, &mut jm) } else { f32::INFINITY }
                        }
                    );
                }
            }
        }
        // Empirical palette solve: for each (group, joint-slot), the centroid of the verts
        // dominantly bound to that slot, and the nearest HIER bones to that centroid. This is
        // the mapping the data IMPLIES, independent of where it is encoded on disk.
        for m in &meshes {
            if m.rigid || m.joints.is_empty() {
                continue;
            }
            let jmax = m
                .joints
                .iter()
                .zip(&m.weights)
                .flat_map(|(j4, w4)| (0..4).filter(|&k| w4[k] > 0).map(|k| j4[k]))
                .max()
                .unwrap_or(0) as usize;
            let mut acc = vec![([0.0f32; 3], 0usize); jmax + 1];
            for (vi, p) in m.positions.iter().enumerate() {
                let (Some(j4), Some(w4)) = (m.joints.get(vi), m.weights.get(vi)) else { continue };
                let wi = w4.iter().enumerate().max_by_key(|(_, &w)| w).map(|(i, _)| i).unwrap_or(0);
                let slot = j4[wi] as usize;
                let a = &mut acc[slot];
                for k in 0..3 {
                    a.0[k] += p[k];
                }
                a.1 += 1;
            }
            println!("  group{} sub{} slot->nearest-bone solve ({} slots):", m.group_index, m.sub_object, jmax + 1);
            for (slot, (sumc, n)) in acc.iter().enumerate() {
                if *n == 0 {
                    println!("    slot{slot:<3} (no dominant verts)");
                    continue;
                }
                let c = [sumc[0] / *n as f32, sumc[1] / *n as f32, sumc[2] / *n as f32];
                let mut cand: Vec<(usize, f32)> = (0..s.rig.len())
                    .map(|b| {
                        let w = &world[b];
                        let d = ((c[0] - w[3][0]).powi(2) + (c[1] - w[3][1]).powi(2) + (c[2] - w[3][2]).powi(2)).sqrt();
                        (b, d)
                    })
                    .collect();
                cand.sort_by(|a, b| a.1.total_cmp(&b.1));
                println!(
                    "    slot{slot:<3} n={n:<5} c=[{:+6.3},{:+6.3},{:+6.3}] near: {}",
                    c[0], c[1], c[2],
                    cand[..3]
                        .iter()
                        .map(|(b, d)| format!("bone{b}({}, {d:.3})", nm(s.rig[*b].name_hash)))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
            }
        }
    }

    for (blk, _) in &hits {
        let data = wad::decompress_block_index(&mut w, *blk)?;
        let ag = parse_animgroup(&data).map_err(|e| format!("parse animgroup {blk}: {e}"))?;
        let c = ag.clips.iter().find(|c| c.name_hash == clip_hash).ok_or("clip vanished")?;

        // Raw trnm read-back straight from the block bytes (independent of read_trnm).
        let (count, entries) = parse_block_entry_table(&data);
        let mut pos = 4 + count as usize * 16;
        let mut raw: Option<(u32, u32, u32, Vec<u32>)> = None; // (size, count_word, lead_word, hashes)
        for e in &entries {
            let cont = &data[pos..(pos + e.chunk_size as usize).min(data.len())];
            pos += e.chunk_size as usize;
            if e.name_hash != clip_hash || cont.len() < 20 || &cont[0..4] != b"UCFX" {
                continue;
            }
            let dao = u32::from_le_bytes(cont[4..8].try_into().unwrap()) as usize;
            let nd = u32::from_le_bytes(cont[16..20].try_into().unwrap()) as usize;
            for i in 0..nd.min((cont.len().saturating_sub(20)) / 20) {
                let row = 20 + i * 20;
                if &cont[row..row + 4] != b"trnm" {
                    continue;
                }
                let u0 = u32::from_le_bytes(cont[row + 4..row + 8].try_into().unwrap());
                let size = u32::from_le_bytes(cont[row + 8..row + 12].try_into().unwrap());
                let start = if dao > 0 { dao + u0 as usize } else { 8 + u0 as usize };
                let t = &cont[start..start + size as usize];
                let rd = |o: usize| u32::from_le_bytes(t[o..o + 4].try_into().unwrap());
                let cw = rd(0);
                let all: Vec<u32> = (1..(size as usize / 4)).map(|k| rd(k * 4)).collect();
                raw = Some((size, cw, all[0], all));
                break;
            }
            break;
        }

        let ac = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok();
        println!("\n== block {blk}: clip 0x{clip_hash:08X} class={} ==", c.class);
        println!(
            "  header: numTransformTracks={} numFloatTracks={} duration={:.4}s poses={}",
            c.num_transform_tracks, c.num_float_tracks, c.duration, c.num_poses
        );
        if let Some(ac) = &ac {
            println!(
                "  decoder: decoded={} num_tracks={} num_frames={} duration={:.4}",
                ac.decoded, ac.num_tracks, ac.num_frames, ac.duration
            );
        }
        if let Some((size, cw, lead, all)) = &raw {
            println!(
                "  raw trnm: size={size} count_word={cw} lead_word=0x{lead:08X} ({})  size==8+count*4: {}  words_after_count={}",
                nm(*lead), *size as usize == 8 + *cw as usize * 4, all.len()
            );
        }
        let tth = c.binding.resolve_to_hier(&hier);
        println!("  binding: {} trnm hashes, {} resolve to HIER", tth.len(), tth.iter().filter(|r| r.is_some()).count());
        // Per-track decoded-data stats across every frame: max |T_anim - T_bind| (bind = the
        // bone's HIER local translation), scale min/max, worst |q|-1. Garbage on a track shows
        // up here as a huge T delta / non-unit scale even when the binding itself is correct.
        let frames: Vec<Vec<mercs2_formats::anim::QsTransform>> = match &ac {
            Some(ac) if ac.decoded && ac.num_frames > 0 => (0..ac.num_frames)
                .map(|f| ac.sample_local(ac.duration * f as f32 / (ac.num_frames.max(2) - 1) as f32))
                .collect(),
            _ => Vec::new(),
        };
        for (t, h) in c.binding.track_to_bone_hash.iter().enumerate() {
            let stats = if frames.is_empty() {
                String::new()
            } else {
                let bind_t: Option<[f32; 3]> = tth[t].map(|b| {
                    let lb = s.rig[b].local_bind;
                    [lb[3][0], lb[3][1], lb[3][2]]
                });
                let (mut max_dt, mut smin, mut smax, mut qerr) = (0.0f32, f32::INFINITY, f32::NEG_INFINITY, 0.0f32);
                for fr in &frames {
                    let Some(qs) = fr.get(t) else { continue };
                    if let Some(bt) = bind_t {
                        let d = ((qs.translation[0] - bt[0]).powi(2)
                            + (qs.translation[1] - bt[1]).powi(2)
                            + (qs.translation[2] - bt[2]).powi(2))
                        .sqrt();
                        max_dt = max_dt.max(d);
                    }
                    for &sc in &qs.scale {
                        smin = smin.min(sc);
                        smax = smax.max(sc);
                    }
                    let qn = qs.rotation.iter().map(|c| c * c).sum::<f32>().sqrt();
                    qerr = qerr.max((qn - 1.0).abs());
                }
                format!("  max|dT|={max_dt:7.4} scale=[{smin:+.3},{smax:+.3}] |q|err={qerr:.4}")
            };
            match tth[t] {
                Some(b) => println!(
                    "    track{t:<3} 0x{h:08X} -> bone{b:<3} parent={:<3} bindpos=[{:+7.3},{:+7.3},{:+7.3}]  {}{stats}",
                    s.rig[b].parent, world[b][3][0], world[b][3][1], world[b][3][2], nm(*h)
                ),
                None => println!("    track{t:<3} 0x{h:08X} -> UNRESOLVED  {}{stats}", nm(*h)),
            }
        }
        // Coverage: undriven bones + multiply-driven bones.
        let mut drive = vec![0u32; s.rig.len()];
        for r in tth.iter().flatten() {
            drive[*r] += 1;
        }
        let undriven: Vec<usize> = (0..s.rig.len()).filter(|&b| drive[b] == 0).collect();
        let multi: Vec<usize> = (0..s.rig.len()).filter(|&b| drive[b] > 1).collect();
        println!("  undriven bones: {undriven:?}");
        println!("  multiply-driven bones: {multi:?}");

        // Render-path replica: compute the EXACT palette render() computes (sample_local at
        // continuous times -> havok_palette) and CPU-skin every vert by its dominant joint.
        // Reports each bone's worst vertex displacement from its bind position across the
        // sweep — the numeric fingerprint of on-screen spikes.
        if let Some(ac) = &ac {
            if ac.decoded {
                let _ = &verts;
                let ntt = c.num_transform_tracks as usize;
                // Bone-length stretch: |modelpos[b] - modelpos[parent]| vs the bind bone
                // length, over the same locals havok_palette builds. Root motion cancels
                // out, so a ratio >> 1 IS a spike (a stretched bone), not locomotion.
                let bind_len: Vec<f32> = (0..s.rig.len())
                    .map(|b| {
                        if s.rig[b].parent < 0 {
                            0.0
                        } else {
                            let p = s.rig[b].parent as usize;
                            ((world[b][3][0] - world[p][3][0]).powi(2)
                                + (world[b][3][1] - world[p][3][1]).powi(2)
                                + (world[b][3][2] - world[p][3][2]).powi(2))
                            .sqrt()
                        }
                    })
                    .collect();
                let mut per_bone_max_len = vec![0.0f32; s.rig.len()];
                let steps = 101usize;
                for k in 0..steps {
                    let t = ac.duration * k as f32 / (steps - 1) as f32;
                    let sample = ac.sample_local(t);
                    let mut local = pose::bind_qs(&s.rig);
                    for (track, bone) in tth.iter().enumerate() {
                        if track >= ntt {
                            break;
                        }
                        if let (Some(&b), Some(qs)) = (bone.as_ref(), sample.get(track)) {
                            if b < local.len() {
                                local[b] = *qs;
                            }
                        }
                    }
                    let model = pose::model_poses(&s.rig, &local);
                    for b in 0..s.rig.len() {
                        if s.rig[b].parent < 0 {
                            continue;
                        }
                        let p = s.rig[b].parent as usize;
                        let l = ((model[b].translation[0] - model[p].translation[0]).powi(2)
                            + (model[b].translation[1] - model[p].translation[1]).powi(2)
                            + (model[b].translation[2] - model[p].translation[2]).powi(2))
                        .sqrt();
                        if l > per_bone_max_len[b] {
                            per_bone_max_len[b] = l;
                        }
                    }
                }
                let mut ranked: Vec<(usize, f32)> = (0..s.rig.len())
                    .map(|b| (b, per_bone_max_len[b] - bind_len[b]))
                    .collect();
                ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
                let bone_track: std::collections::HashMap<usize, usize> =
                    tth.iter().enumerate().filter_map(|(t, b)| b.map(|bb| (bb, t))).collect();
                println!("  render-path replica, worst bone-length stretch (bone  anim_len vs bind_len  track  name):");
                for (b, ex) in ranked.iter().take(14) {
                    println!(
                        "    bone{b:<3} stretch={ex:+7.3} (max {:.3} vs bind {:.3}) parent={:<3} track={:<4} {}",
                        per_bone_max_len[*b],
                        bind_len[*b],
                        s.rig[*b].parent,
                        bone_track.get(b).map(|t| t.to_string()).unwrap_or_else(|| "-".into()),
                        nm(s.rig[*b].name_hash)
                    );
                }
            }
        }
    }
    Ok(())
}

/// Bind-pose skinning gate (headless): the palette `Skin[b] = InvBind[b]·Pose[b]` must be identity
/// at bind pose. Reports the worst per-bone deviation from I, the fit transform, and blend coverage.
/// A near-zero max deviation means the LBS palette reproduces the un-skinned render exactly.
fn skincheck(wadpath: &str, model: Option<String>, index: Option<String>) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models.get(n).map(|&(h, _)| h).ok_or("--index out of range")?
    } else {
        models.first().map(|&(h, _)| h).ok_or("no models in WAD")?
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, _indices, _draws, s) = mesh::build_indexed_from_container(&container)?;

    let mut worst = 0.0f32;
    let mut worst_bone = 0usize;
    for (b, m) in s.bones.iter().enumerate() {
        for r in 0..4 {
            for c in 0..4 {
                let ident = if r == c { 1.0 } else { 0.0 };
                let d = (m[r][c] - ident).abs();
                if d > worst {
                    worst = d;
                    worst_bone = b;
                }
            }
        }
    }
    // Recompose gate: rebuild the palette from the rig's bind-pose LOCAL transforms (local->world
    // ->skin chain, the animation path). Must also be identity, proving the hierarchy recompose.
    let recomposed = pose::palette(&s.rig, &pose::bind_locals(&s.rig));
    let mut worst_r = 0.0f32;
    for m in &recomposed {
        for r in 0..4 {
            for c in 0..4 {
                let ident = if r == c { 1.0 } else { 0.0 };
                worst_r = worst_r.max((m[r][c] - ident).abs());
            }
        }
    }

    let skinned = verts.iter().filter(|v| v.weights != [255, 0, 0, 0]).count();
    println!("model 0x{hash:08X}: {} bones, {} verts", s.bones.len(), verts.len());
    println!("fit: center={:?} scale={:.5}", s.fit_center, s.fit_scale);
    println!(
        "bind-pose palette   max |Skin - I| = {worst:.6} (bone {worst_bone})  ->  {}",
        if worst < 1e-3 { "GATE PASS (identity)" } else { "GATE FAIL — convention bug" }
    );
    println!(
        "recomposed palette  max |Skin - I| = {worst_r:.6}                 ->  {}",
        if worst_r < 1e-3 { "GATE PASS (local->world->skin)" } else { "GATE FAIL — recompose bug" }
    );
    println!(
        "blend coverage: {skinned}/{} verts skinned ({} rigid/pass-through)",
        verts.len(),
        verts.len() - skinned
    );
    Ok(())
}

/// Per-STRM diagnostic for one model: stride, vcount, decl, POSITION element, and bbox — to
/// pinpoint a mis-positioned submesh (e.g. a floating accessory).
fn wad_meshes(wadpath: &str, model: Option<String>) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    let hash = match model {
        Some(m) => parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?,
        None => models.first().map(|&(h, _)| h).ok_or("no models in WAD")?,
    };
    let container = wad::extract_container(&mut w, hash)?;
    let strms = mercs2_formats::model_cubeize::describe_model_strms(&container)?;
    println!("model 0x{hash:08X}: {} STRM groups", strms.len());
    for (i, s) in strms.iter().enumerate() {
        let pos = match s.pos {
            Some((st, off, ty)) => format!("pos[stream={st} off={off} type={ty}]"),
            None => "pos[NONE]".to_string(),
        };
        let bbox = match s.bbox {
            Some((lo, hi)) => format!(
                "y[{:6.2},{:6.2}] x[{:6.2},{:6.2}] z[{:6.2},{:6.2}]",
                lo[1], hi[1], lo[0], hi[0], lo[2], hi[2]
            ),
            None => "bbox[-]".to_string(),
        };
        println!(
            "  [{i:2}] stride={:<3} vcount={:<6} decl={:<2} {pos:<28} {bbox}",
            s.stride, s.vcount, s.decl_elems
        );
    }

    // UV/normal extraction coverage (1e reader check): per group, how many verts got UVs/normals
    // + the UV range (expect roughly [0,1]).
    let meshes = mercs2_formats::model_cubeize::read_model_meshes(&container)?;

    // Winding check: fraction of triangles whose geometric winding (cross of edges) agrees with the
    // vertex normal. ~1.0 => tri order a,b,c is CCW-when-viewed-from-outside (front_face Ccw);
    // ~0.0 => CW. Tells us the correct cull front_face without a GPU trial.
    let (mut agree, mut total) = (0u64, 0u64);
    for m in &meshes {
        for t in &m.tris {
            if m.normals.is_empty() {
                continue;
            }
            let (p0, p1, p2) = (
                m.positions[t[0] as usize],
                m.positions[t[1] as usize],
                m.positions[t[2] as usize],
            );
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let gn = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let n = m.normals[t[0] as usize];
            let d = gn[0] * n[0] + gn[1] * n[1] + gn[2] * n[2];
            if d > 0.0 {
                agree += 1;
            }
            total += 1;
        }
    }
    if total > 0 {
        println!(
            "-- winding: {:.1}% of tris wind CCW-outward (>~90% => front_face Ccw; <~10% => Cw) --",
            100.0 * agree as f64 / total as f64
        );
    }
    println!("-- geometry read: {} drawing groups --", meshes.len());
    for (i, m) in meshes.iter().enumerate() {
        let (mut u0, mut u1, mut v0, mut v1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
        for uv in &m.uvs {
            u0 = u0.min(uv[0]);
            u1 = u1.max(uv[0]);
            v0 = v0.min(uv[1]);
            v1 = v1.max(uv[1]);
        }
        let uvr = if m.uvs.is_empty() {
            "uv[none]".to_string()
        } else {
            format!("u[{u0:5.2},{u1:5.2}] v[{v0:5.2},{v1:5.2}]")
        };
        // Per-group winding agreement (CCW-outward %).
        let (mut ga, mut gt) = (0u64, 0u64);
        for t in &m.tris {
            if m.normals.is_empty() {
                break;
            }
            let (p0, p1, p2) = (
                m.positions[t[0] as usize],
                m.positions[t[1] as usize],
                m.positions[t[2] as usize],
            );
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let gn = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let n = m.normals[t[0] as usize];
            if gn[0] * n[0] + gn[1] * n[1] + gn[2] * n[2] > 0.0 {
                ga += 1;
            }
            gt += 1;
        }
        let wind = if gt > 0 {
            format!("wind={:.0}%", 100.0 * ga as f64 / gt as f64)
        } else {
            "wind=-".to_string()
        };
        let kind = if m.rigid { "MESH" } else { "SKIN" };
        println!(
            "  [{i:2}] verts={:<6} tris={:<6} so={:<2} {kind} bone={:<3} mask={:#04x} {wind:<9} {uvr}",
            m.positions.len(),
            m.tris.len(),
            m.sub_object,
            m.bone,
            m.state_mask
        );
    }
    Ok(())
}

/// Terrain world extent (from the placement grid: centers -3800..3800, tile-local ±200).
const WORLD_MIN_M: f32 = -4000.0;
const WORLD_SPAN_M: f32 = 8000.0;

/// Map a `TerrainMesh` into engine `Vertex`es. Positions are native game-space
/// world metres (no flips). Because the source vertex UVs are not a texture
/// atlas mapping (they carry normals), synthesize a planar XZ projection over the
/// 8 km continent so the shared `vz_lrterrain` atlas lands on the terrain
/// (mirrors `terrain_extractor.py::_world_xz_to_uv`, retail V-flip). normal =
/// [0,1,0], color = white, tangent = [1,0,0,1], joints = 0, weights = [255,0,0,0]
/// (binds every vertex to identity bone 0).
fn terrain_to_vertices(tm: &mercs2_formats::terrain::TerrainMesh, textured: bool) -> Vec<Vertex> {
    tm.positions
        .iter()
        .map(|&p| {
            let uv = if textured {
                let u = (p[0] - WORLD_MIN_M) / WORLD_SPAN_M;
                let v = 1.0 - (p[2] - WORLD_MIN_M) / WORLD_SPAN_M; // retail V-flip
                [u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]
            } else {
                [0.0, 0.0]
            };
            Vertex {
                pos: p,
                color: [1.0, 1.0, 1.0],
                uv,
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
//   World placements (layers_static block 29): markers + interior hunt
// ---------------------------------------------------------------------------

/// The PMC HQ compound, game coords (docs/coordinate_systems.md Example 1).
const PMC_HQ: [f32; 2] = [2647.0, -951.0];
const PMC_HQ_RADIUS_M: f32 = 150.0;

/// Normal world envelope (docs §5). A placement outside it is an interior-hunt
/// candidate: |x|>4000 OR |z|>4000 OR y<-150 OR y>450.
fn is_out_of_bounds(p: &[f32; 3]) -> bool {
    p[0].abs() > 4000.0 || p[2].abs() > 4000.0 || p[1] < -150.0 || p[1] > 450.0
}

/// True if a placement's name flags it as a base/interior of interest.
fn name_is_pmc_base(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["pmc", "interior", "hq", "base", "outpost"]
        .iter()
        .any(|k| n.contains(k))
}

/// True if a placement belongs to the PMC-base subset (near the HQ or name-flagged).
fn placement_is_pmc_subset(p: &mercs2_formats::placement::Placement) -> bool {
    let dx = p.pos[0] - PMC_HQ[0];
    let dz = p.pos[2] - PMC_HQ[1];
    if (dx * dx + dz * dz).sqrt() <= PMC_HQ_RADIUS_M {
        return true;
    }
    p.name.as_deref().map(name_is_pmc_base).unwrap_or(false)
}

/// Build ONE merged marker mesh for all placements: a small tetrahedron per
/// placement at its world pos, tinted by category (PMC/base subset = warm, else
/// cool). Native game-space metres, no flips. Returns (verts, indices, draw).
fn build_placement_markers(
    placements: &[mercs2_formats::placement::Placement],
) -> (Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>) {
    const H: f32 = 3.0; // marker height (m)
    const R: f32 = 0.9; // marker base half-width (m)
    // Upright tetra: apex above, 3 base corners. (LH +Y up.)
    let local: [[f32; 3]; 4] = [
        [0.0, H, 0.0],
        [-R, 0.0, -R],
        [R, 0.0, -R],
        [0.0, 0.0, R],
    ];
    let faces: [[u32; 3]; 4] = [[0, 2, 1], [0, 3, 2], [0, 1, 3], [1, 2, 3]];
    let mut verts: Vec<Vertex> = Vec::with_capacity(placements.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(placements.len() * 12);
    for p in placements {
        let color = if placement_is_pmc_subset(p) {
            [0.95, 0.35, 0.10] // PMC/base subset: warm orange
        } else if is_out_of_bounds(&p.pos) {
            [0.95, 0.90, 0.15] // off-map candidate: yellow
        } else {
            [0.20, 0.55, 0.90] // ordinary placement: cool blue
        };
        let base = verts.len() as u32;
        for l in &local {
            verts.push(Vertex {
                pos: [p.pos[0] + l[0], p.pos[1] + l[1], p.pos[2] + l[2]],
                color,
                uv: [0.0, 0.0],
                normal: [0.0, 1.0, 0.0],
                tangent: [1.0, 0.0, 0.0, 1.0],
                joints: [0, 0, 0, 0],
                weights: [255, 0, 0, 0],
            });
        }
        for f in &faces {
            indices.push(base + f[0]);
            indices.push(base + f[1]);
            indices.push(base + f[2]);
        }
    }
    let draws = vec![mesh::DrawGroup {
        index_start: 0,
        index_count: indices.len() as u32,
        diffuse: None, // vertex-color only (white fallback texture)
        normal: None,
    }];
    (verts, indices, draws)
}

/// Print the full interior-hunt analysis (Task 2): out-of-bounds clusters,
/// pmc/interior/base-named placements, and PMC-subset count. Pure logging.
fn report_interior_hunt(placements: &[mercs2_formats::placement::Placement]) {
    // Overall counts + ranges.
    let named = placements.iter().filter(|p| p.name.is_some()).count();
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in placements {
        for k in 0..3 {
            min[k] = min[k].min(p.pos[k]);
            max[k] = max[k].max(p.pos[k]);
        }
    }
    println!(
        "[placements] total = {}, named = {}",
        placements.len(),
        named
    );
    println!(
        "[placements] X range = [{:.1}, {:.1}]  Y range = [{:.1}, {:.1}]  Z range = [{:.1}, {:.1}]",
        min[0], max[0], min[1], max[1], min[2], max[2]
    );

    // Out-of-bounds cluster analysis: bin by ~500 m XZ cell + Y band, print
    // centroids + counts + sample names.
    let oob: Vec<&mercs2_formats::placement::Placement> =
        placements.iter().filter(|p| is_out_of_bounds(&p.pos)).collect();
    println!("[interior-hunt] out-of-bounds placements (|x|>4000 | |z|>4000 | y<-150 | y>450) = {}", oob.len());
    if !oob.is_empty() {
        use std::collections::HashMap;
        let mut clusters: HashMap<(i32, i32, i32), Vec<&mercs2_formats::placement::Placement>> =
            HashMap::new();
        for p in &oob {
            let cx = (p.pos[0] / 500.0).round() as i32;
            let cz = (p.pos[2] / 500.0).round() as i32;
            let cy = (p.pos[1] / 200.0).round() as i32; // 200 m Y band
            clusters.entry((cx, cy, cz)).or_default().push(p);
        }
        let mut ranked: Vec<((i32, i32, i32), Vec<&mercs2_formats::placement::Placement>)> =
            clusters.into_iter().collect();
        ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
        for ((_cx, _cy, _cz), members) in ranked.iter().take(20) {
            let n = members.len() as f32;
            let mut c = [0.0f32; 3];
            for m in members {
                for k in 0..3 {
                    c[k] += m.pos[k] / n;
                }
            }
            let samples: Vec<String> = members
                .iter()
                .filter_map(|m| m.name.clone())
                .take(4)
                .collect();
            println!(
                "[interior-hunt]   cluster n={:<5} centroid=({:.0}, {:.0}, {:.0})  samples: {}",
                members.len(),
                c[0],
                c[1],
                c[2],
                if samples.is_empty() { "<unnamed>".to_string() } else { samples.join(", ") }
            );
        }
    }

    // Name-flagged placements (pmc/interior/hq/base/outpost).
    let flagged: Vec<&mercs2_formats::placement::Placement> = placements
        .iter()
        .filter(|p| p.name.as_deref().map(name_is_pmc_base).unwrap_or(false))
        .collect();
    println!("[interior-hunt] name-flagged (pmc/interior/hq/base/outpost) = {}", flagged.len());
    // Group by distinct name for a compact report (name -> count + one sample pos).
    {
        use std::collections::BTreeMap;
        let mut by_name: BTreeMap<String, (usize, [f32; 3])> = BTreeMap::new();
        for p in &flagged {
            let e = by_name.entry(p.name.clone().unwrap()).or_insert((0, p.pos));
            e.0 += 1;
        }
        for (name, (count, pos)) in by_name.iter().take(60) {
            println!(
                "[interior-hunt]   {name:<40} x{count:<4} e.g. ({:.0}, {:.0}, {:.0})",
                pos[0], pos[1], pos[2]
            );
        }
        if by_name.len() > 60 {
            println!("[interior-hunt]   ... {} more distinct names", by_name.len() - 60);
        }
    }

    // Interior locator: the game boots the player into the PMC interior at the SE-corner coord
    // (3794.04, 450.75, -3911.03) (MrxUtil._TeleportHero). Count any layers_static placement within
    // 300 m XZ of it — if none, the interior geometry is NOT in this block (it's a runtime-spawned
    // HqInterior actor / separate cell), which the Z-min below confirms.
    const INT_XZ: [f32; 2] = [3794.0427, -3911.0322];
    let near_int: Vec<&mercs2_formats::placement::Placement> = placements
        .iter()
        .filter(|p| {
            let dx = p.pos[0] - INT_XZ[0];
            let dz = p.pos[2] - INT_XZ[1];
            (dx * dx + dz * dz).sqrt() <= 300.0
        })
        .collect();
    println!(
        "[interior-hunt] placements within 300 m XZ of the interior coord (3794, -3911) = {} (block Z-min was {:.1}; interior Z=-3911 is BEYOND it)",
        near_int.len(),
        min[2]
    );
    for p in near_int.iter().take(10) {
        println!(
            "[interior-hunt]   near-interior: {:<32} ({:.0}, {:.0}, {:.0})",
            p.name.as_deref().unwrap_or("<unnamed>"),
            p.pos[0], p.pos[1], p.pos[2]
        );
    }

    // PMC-subset (near HQ or name-flagged) — the real-geometry render candidates.
    let subset = placements.iter().filter(|p| placement_is_pmc_subset(p)).count();
    let near_hq = placements
        .iter()
        .filter(|p| {
            let dx = p.pos[0] - PMC_HQ[0];
            let dz = p.pos[2] - PMC_HQ[1];
            (dx * dx + dz * dz).sqrt() <= PMC_HQ_RADIUS_M
        })
        .count();
    println!(
        "[interior-hunt] PMC subset (<= {PMC_HQ_RADIUS_M:.0} m of HQ {:?} OR name-flagged) = {} ({} within HQ radius)",
        PMC_HQ, subset, near_hq
    );
}

/// Headless placement probe (VERIFIABLE proof): parse block 29, load all
/// placements, and print counts, ranges, the interior hunt, and whether the
/// records carry a model-asset hash (they key by entity — see report).
fn placement_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    println!("[placement-probe] layers_static block = {} bytes", ls.len());
    let placements = mercs2_formats::placement::load_placements(&ls)?;
    report_interior_hunt(&placements);
    // Quat unit-length sanity across all records.
    let mut nonunit = 0usize;
    for p in &placements {
        let m = p.quat[0] * p.quat[0] + p.quat[1] * p.quat[1] + p.quat[2] * p.quat[2] + p.quat[3] * p.quat[3];
        if !(0.81..=1.21).contains(&m) {
            nonunit += 1;
        }
    }
    println!(
        "[placement-probe] quaternion unit-length: {} of {} outside [0.9,1.1]^2",
        nonunit,
        placements.len()
    );

    // Where does the interior geometry actually live? Scan the WAD's block-path table (PTHS) for
    // interior/hqinterior/pmcinterior block names — the interior cell is a separate block, not in
    // layers_static.
    let hits: Vec<(usize, &String)> = wad::block_paths(&w)
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let l = p.to_ascii_lowercase();
            l.contains("interior") || l.contains("hqint") || l.contains("pmcint") || l.contains("briefing")
        })
        .collect();
    println!("[placement-probe] WAD block paths matching interior/briefing = {}", hits.len());
    for (i, p) in hits.iter().take(30) {
        println!("[placement-probe]   block {i}: {p}");
    }
    Ok(())
}

/// Headless terrain probe (VERIFIABLE proof): parse blocks 29 + 3121, load the
/// merged terrain, and print TOC entry count, tiles decoded/placed, total
/// verts/tris, and world-space position min/max on X/Y/Z.
fn world_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;
    println!("[world-probe] low_res_terrain block = {} bytes, layers_static block = {} bytes", low.len(), ls.len());
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;

    let ntris = tm.indices.len() / 3;
    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &tm.positions {
        for k in 0..3 {
            min[k] = min[k].min(p[k]);
            max[k] = max[k].max(p[k]);
        }
    }
    println!("[world-probe] TOC entry count      = {}", tm.toc_entry_count);
    println!("[world-probe] tiles decoded        = {}", tm.tiles_decoded);
    println!("[world-probe] tiles placed         = {}", tm.tiles_placed);
    println!("[world-probe] total vertices       = {}", tm.positions.len());
    println!("[world-probe] total triangles      = {ntris}");
    println!(
        "[world-probe] position X range     = [{:.2}, {:.2}]",
        min[0], max[0]
    );
    println!(
        "[world-probe] position Y range     = [{:.2}, {:.2}]",
        min[1], max[1]
    );
    println!(
        "[world-probe] position Z range     = [{:.2}, {:.2}]",
        min[2], max[2]
    );
    match &tm.texture {
        Some(t) => println!(
            "[world-probe] terrain texture      = {}x{} {:?} ({} bytes mip0, {} mips)",
            t.width, t.height, t.format, t.mip0.len(), t.mip_count
        ),
        None => println!("[world-probe] terrain texture      = <none parsed>"),
    }
    Ok(())
}

/// Ground height lookup for the third-person walk, built from the SAME triangle data the renderer
/// draws. Two layers:
///  1. EXACT: a triangle spatial hash (TRI_N×TRI_N cells over the terrain's [-4000, 4000]² world
///     extent, ~32 m cells); each triangle is inserted into every cell its XZ AABB overlaps, and
///     lookup does a 2D barycentric point-in-XZ-triangle test, interpolating Y barycentrically.
///  2. FALLBACK: the previous coarse grid (max vertex Y per 512×512 cell, neighbour-dilated,
///     bilinear between cell centres) for (x, z) covered by NO triangle (holes/map edge), so the
///     player never falls through the world.
struct HeightMap {
    cells: Vec<f32>,          // coarse fallback grid (max vertex Y per cell, dilated)
    positions: Vec<[f32; 3]>, // terrain vertices (copy of the render data)
    indices: Vec<u32>,        // terrain triangle indices (copy of the render data)
    tri_cells: Vec<Vec<u32>>, // per-cell triangle ids (index/3), by XZ AABB overlap
}

impl HeightMap {
    const N: usize = 512;
    const MIN: f32 = -4000.0;
    const MAX: f32 = 4000.0;
    const TRI_N: usize = 250; // 32 m triangle-hash cells over the same extent

    fn build(tm: &mercs2_formats::terrain::TerrainMesh) -> HeightMap {
        let t0 = std::time::Instant::now();
        let n = Self::N;
        let scale = n as f32 / (Self::MAX - Self::MIN);
        let mut cells = vec![f32::NEG_INFINITY; n * n];
        for p in &tm.positions {
            let cx = (((p[0] - Self::MIN) * scale) as usize).min(n - 1);
            let cz = (((p[2] - Self::MIN) * scale) as usize).min(n - 1);
            let c = &mut cells[cz * n + cx];
            *c = c.max(p[1]);
        }
        let mut remaining = cells.iter().filter(|c| !c.is_finite()).count();
        if remaining == n * n {
            cells.fill(0.0); // no terrain verts at all: flat ground, don't dilate forever
            remaining = 0;
        }
        while remaining > 0 {
            let prev = cells.clone();
            for cz in 0..n {
                for cx in 0..n {
                    if prev[cz * n + cx].is_finite() {
                        continue;
                    }
                    let mut best = f32::NEG_INFINITY;
                    for dz in cz.saturating_sub(1)..=(cz + 1).min(n - 1) {
                        for dx in cx.saturating_sub(1)..=(cx + 1).min(n - 1) {
                            best = best.max(prev[dz * n + dx]);
                        }
                    }
                    if best.is_finite() {
                        cells[cz * n + cx] = best;
                        remaining -= 1;
                    }
                }
            }
        }
        // Triangle spatial hash: each triangle goes into every cell its XZ AABB overlaps.
        let tn = Self::TRI_N;
        let tscale = tn as f32 / (Self::MAX - Self::MIN);
        let cell_of = |v: f32| (((v - Self::MIN) * tscale) as isize).clamp(0, tn as isize - 1) as usize;
        let mut tri_cells: Vec<Vec<u32>> = vec![Vec::new(); tn * tn];
        let mut entries = 0usize;
        for (t, tri) in tm.indices.chunks_exact(3).enumerate() {
            let a = tm.positions[tri[0] as usize];
            let b = tm.positions[tri[1] as usize];
            let c = tm.positions[tri[2] as usize];
            let (x0, x1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (z0, z1) = (a[2].min(b[2]).min(c[2]), a[2].max(b[2]).max(c[2]));
            for cz in cell_of(z0)..=cell_of(z1) {
                for cx in cell_of(x0)..=cell_of(x1) {
                    tri_cells[cz * tn + cx].push(t as u32);
                    entries += 1;
                }
            }
        }
        println!(
            "[world] heightmap: {} tris hashed into {tn}x{tn} cells ({entries} entries) + {n}x{n} fallback in {:.0} ms",
            tm.indices.len() / 3,
            t0.elapsed().as_secs_f64() * 1000.0
        );
        HeightMap {
            cells,
            positions: tm.positions.clone(),
            indices: tm.indices.clone(),
            tri_cells,
        }
    }

    /// Highest Y of any rendered triangle covering world (x, z), by 2D barycentric test in XZ
    /// (edges included, weight epsilon 1e-4; math in f64). With `y_max`, prefers the highest hit
    /// at or below it (overhang/bridge disambiguation), falling back to the highest overall.
    /// `None` when no triangle covers the point.
    fn tri_height_at(&self, x: f32, z: f32, y_max: Option<f32>) -> Option<f32> {
        let tn = Self::TRI_N;
        let tscale = tn as f32 / (Self::MAX - Self::MIN);
        let cell = |v: f32| (((v - Self::MIN) * tscale) as isize).clamp(0, tn as isize - 1) as usize;
        let (px, pz) = (x as f64, z as f64);
        let mut best: Option<f64> = None; // highest overall
        let mut best_near: Option<f64> = None; // highest ≤ y_max
        for &t in &self.tri_cells[cell(z) * tn + cell(x)] {
            let i = t as usize * 3;
            let a = self.positions[self.indices[i] as usize];
            let b = self.positions[self.indices[i + 1] as usize];
            let c = self.positions[self.indices[i + 2] as usize];
            let (ax, az) = (a[0] as f64, a[2] as f64);
            let (bx, bz) = (b[0] as f64, b[2] as f64);
            let (cx, cz) = (c[0] as f64, c[2] as f64);
            let denom = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
            if denom.abs() < 1e-9 {
                continue; // degenerate in XZ (vertical / zero-area)
            }
            let w0 = ((bz - cz) * (px - cx) + (cx - bx) * (pz - cz)) / denom;
            let w1 = ((cz - az) * (px - cx) + (ax - cx) * (pz - cz)) / denom;
            let w2 = 1.0 - w0 - w1;
            const EPS: f64 = 1e-4;
            if w0 < -EPS || w1 < -EPS || w2 < -EPS {
                continue;
            }
            let y = w0 * a[1] as f64 + w1 * b[1] as f64 + w2 * c[1] as f64;
            if best.map_or(true, |v| y > v) {
                best = Some(y);
            }
            if let Some(limit) = y_max {
                if y <= limit as f64 && best_near.map_or(true, |v| y > v) {
                    best_near = Some(y);
                }
            }
        }
        (if y_max.is_some() { best_near.or(best) } else { best }).map(|y| y as f32)
    }

    /// Ground height at world (x, z): exact triangle sample (highest covering triangle), with the
    /// coarse grid as fallback where no triangle covers the point.
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.tri_height_at(x, z, None)
            .unwrap_or_else(|| self.coarse_height_at(x, z))
    }

    /// Like `height_at`, but prefers the highest triangle at or below `y_hint + 2.0` so a player
    /// standing UNDER a bridge/overhang isn't teleported on top of it.
    fn height_at_near(&self, x: f32, z: f32, y_hint: f32) -> f32 {
        self.tri_height_at(x, z, Some(y_hint + 2.0))
            .unwrap_or_else(|| self.coarse_height_at(x, z))
    }

    /// Coarse-grid ground height at world (x, z): bilinear blend of the four nearest cell centres.
    fn coarse_height_at(&self, x: f32, z: f32) -> f32 {
        let n = Self::N;
        let scale = n as f32 / (Self::MAX - Self::MIN);
        let fx = ((x - Self::MIN) * scale - 0.5).clamp(0.0, (n - 1) as f32);
        let fz = ((z - Self::MIN) * scale - 0.5).clamp(0.0, (n - 1) as f32);
        let (x0, z0) = (fx as usize, fz as usize);
        let (x1, z1) = ((x0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (tx, tz) = (fx - x0 as f32, fz - z0 as f32);
        let h = |cx: usize, cz: usize| self.cells[cz * n + cx];
        let a = h(x0, z0) * (1.0 - tx) + h(x1, z0) * tx;
        let b = h(x0, z1) * (1.0 - tx) + h(x1, z1) * tx;
        a * (1.0 - tz) + b * tz
    }
}

/// MERCS2_HMAP_VERIFY: numeric evidence for the exact triangle sampler.
///  - old-vs-new sweep on a 25 m grid (max |coarse − exact| + 5 worst points),
///  - exactness on 1000 deterministic-random triangle centroids (barycentric hit must reproduce
///    the centroid Y unless a HIGHER overlapping triangle covers it).
fn verify_heightmap(hmap: &HeightMap) {
    // Old vs new sweep.
    let mut worst: Vec<(f32, f32, f32, f32, f32)> = Vec::new(); // (|d|, x, z, old, new)
    for iz in 0..=320 {
        for ix in 0..=320 {
            let x = HeightMap::MIN + ix as f32 * 25.0;
            let z = HeightMap::MIN + iz as f32 * 25.0;
            let old = hmap.coarse_height_at(x, z);
            let new = hmap.height_at(x, z);
            let d = (old - new).abs();
            worst.push((d, x, z, old, new));
            worst.sort_by(|a, b| b.0.total_cmp(&a.0));
            worst.truncate(5);
        }
    }
    println!("[hmap-verify] old-vs-new on 321x321 grid (25 m step): max |old-new| = {:.3}", worst[0].0);
    for (d, x, z, old, new) in &worst {
        println!("[hmap-verify]   worst: ({x:.0}, {z:.0}) old={old:.3} new={new:.3} |d|={d:.3}");
    }
    println!(
        "[hmap-verify] h(0,0): old={:.4} new={:.4}",
        hmap.coarse_height_at(0.0, 0.0),
        hmap.height_at(0.0, 0.0)
    );
    // Centroid exactness.
    let ntris = hmap.indices.len() / 3;
    let (mut exact, mut higher, mut miss, mut degen) = (0u32, 0u32, 0u32, 0u32);
    let mut rng: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..1000 {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let i = ((rng >> 33) as usize % ntris) * 3;
        let a = hmap.positions[hmap.indices[i] as usize];
        let b = hmap.positions[hmap.indices[i + 1] as usize];
        let c = hmap.positions[hmap.indices[i + 2] as usize];
        let denom = (b[2] as f64 - c[2] as f64) * (a[0] as f64 - c[0] as f64)
            + (c[0] as f64 - b[0] as f64) * (a[2] as f64 - c[2] as f64);
        if denom.abs() < 1e-9 {
            degen += 1; // XZ-degenerate: sampler skips these by design
            continue;
        }
        let cxz = [(a[0] + b[0] + c[0]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
        let cy = (a[1] as f64 + b[1] as f64 + c[1] as f64) / 3.0;
        let h = hmap.height_at(cxz[0], cxz[1]) as f64;
        if (h - cy).abs() <= 1e-3 {
            exact += 1;
        } else if h > cy + 1e-3 {
            higher += 1;
        } else {
            miss += 1;
            println!(
                "[hmap-verify]   MISS tri {} centroid ({:.2}, {:.2}) cy={cy:.4} h={h:.4}",
                i / 3, cxz[0], cxz[1]
            );
        }
    }
    println!(
        "[hmap-verify] centroids: {exact} within 1e-3, {higher} higher-overlap won, {miss} MISSES, {degen} degenerate-skipped (of 1000)"
    );
}

/// Render the merged low_res world terrain (blocks 29 + 3121) as one static
/// entity under an elevated, slowly auto-rotating bird's-eye camera framing the
/// whole ~8 km grid. The window opens immediately with a loading spinner; the
/// WAD/terrain/player loading runs on a background thread (`load_world_data`).
fn run_world(wadpath: &str) -> Result<(), String> {
    // Headless numeric self-check of the exact triangle sampler vs the coarse fallback grid
    // (MERCS2_HMAP_VERIFY=1): exits before opening a window.
    if std::env::var_os("MERCS2_HMAP_VERIFY").is_some() {
        let data = load_world_data(wadpath, false, false, false, false, &LoadProgress::new(LOAD_STAGES))?;
        verify_heightmap(&data.hmap);
        return Ok(());
    }
    let start_tps = std::env::args().any(|a| a == "--tps");
    let load_cells = std::env::args().any(|a| a == "--cells");
    let load_placements = std::env::args().any(|a| a == "--placements");
    let spawn_interior = std::env::args().any(|a| a == "--interior");
    let load_props = std::env::args().any(|a| a == "--props");
    let interior_orbit = std::env::args().any(|a| a == "--interior-orbit");
    pollster::block_on(run_scene_world_loading(
        wadpath.to_string(),
        start_tps,
        load_cells,
        load_placements,
        spawn_interior,
        load_props,
        interior_orbit,
    ));
    Ok(())
}

/// Staged load-progress counter shared between the background loader and the render thread:
/// the loader calls `step("name")` after each stage; the loading screen reads `fraction()` to
/// fill the plate's progress bar. Adding a stage = one `step` call + bump the `new(N)` total
/// (future: entity placement, PMC spawn setup, act/stage overlays).
struct LoadProgress {
    current: std::sync::atomic::AtomicU32,
    total: std::sync::atomic::AtomicU32,
    t0: std::time::Instant,
}

impl LoadProgress {
    fn new(total: u32) -> Self {
        LoadProgress {
            current: std::sync::atomic::AtomicU32::new(0),
            total: std::sync::atomic::AtomicU32::new(total.max(1)),
            t0: std::time::Instant::now(),
        }
    }
    /// Mark a named stage complete (call AFTER the stage's work) and log it.
    fn step(&self, name: &str) {
        use std::sync::atomic::Ordering;
        let k = self.current.fetch_add(1, Ordering::Relaxed) + 1;
        let n = self.total.load(Ordering::Relaxed);
        println!("[load] stage {k}/{n}: {name} (+{:.1}s)", self.t0.elapsed().as_secs_f64());
    }
    /// Completed fraction 0..1 (the bar's target; the render loop eases toward it).
    fn fraction(&self) -> f32 {
        use std::sync::atomic::Ordering;
        self.current.load(Ordering::Relaxed) as f32 / self.total.load(Ordering::Relaxed) as f32
    }
}

/// Everything `--world` needs loaded before play: plain CPU data (Send), so it can be produced
/// on a background thread while the window shows the loading spinner.
struct WorldData {
    terrain: LoadedModel,
    player: Option<LoadedModel>,
    cells: Vec<(LoadedModel, [f32; 3])>,
    /// Merged placement-marker mesh (one model + one static entity), when `--placements` is set.
    placements: Option<LoadedModel>,
    /// PMC-subset real-geometry models resolved by name→mesh (currently none — see report).
    pmc_models: Vec<(LoadedModel, [f32; 3], f32)>,
    /// PMC interior instances (`--interior`): resolved interior geometry + authored world Transform
    /// (position + full quaternion, native game space, no flip).
    interior: Vec<(LoadedModel, [f32; 3], [f32; 4])>,
    /// Exterior `ModelName` props near the spawn (`--props`): distinct mesh + its placement instances.
    props: Vec<(u32, LoadedModel, Vec<PropInstance>)>,
    /// Interior `ModelName` furniture (`--interior`): distinct mesh + its placement instances (all).
    interior_props: Vec<(u32, LoadedModel, Vec<PropInstance>)>,
    hmap: HeightMap,
}

/// Number of `progress.step` calls in `load_world_data` (keep in sync when adding stages).
const LOAD_STAGES: u32 = 10;

/// Exterior prop bounding: load only props within this radius (m) of the pool spawn, capped at
/// `EXTERIOR_PROP_CAP` distinct meshes, so `--props` stays light next to the full map.
const EXTERIOR_PROP_RADIUS: f32 = 400.0;
const EXTERIOR_PROP_CAP: usize = 200;
/// The exterior pool/back-door spawn (the `--props` centre; matches the default player spawn).
const EXTERIOR_SPAWN: [f32; 3] = [2560.2646, -13.1779, -926.2511];

/// The `--world` loading work (WAD open, terrain merge, heightmap, player avatar + clips,
/// optional c3 cells + placement markers) — the former inline `run_world` body plus placements.
fn load_world_data(
    wadpath: &str,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    progress: &LoadProgress,
) -> Result<WorldData, String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
    let ntris = tm.indices.len() / 3;
    println!(
        "[world] terrain: {} verts / {ntris} tris / {} tiles placed / {} tiles decoded (TOC {})",
        tm.positions.len(), tm.tiles_placed, tm.tiles_decoded, tm.toc_entry_count
    );
    progress.step("terrain");
    let hmap = HeightMap::build(&tm);
    println!(
        "[world] heightmap: h(0,0)={:.2} h(100,100)={:.2} h(-100,100)={:.2} h(100,-100)={:.2} h(-100,-100)={:.2}",
        hmap.height_at(0.0, 0.0), hmap.height_at(100.0, 100.0), hmap.height_at(-100.0, 100.0),
        hmap.height_at(100.0, -100.0), hmap.height_at(-100.0, -100.0)
    );
    progress.step("heightmap");
    let textured = tm.texture.is_some();
    let verts = terrain_to_vertices(&tm, textured);
    let mut textures: TexMap = std::collections::HashMap::new();
    // One draw group spanning the whole mesh, bound to the shared atlas hash 0.
    let draws = if let Some(t) = tm.texture.clone() {
        textures.insert(0, t);
        vec![mesh::DrawGroup {
            index_start: 0,
            index_count: tm.indices.len() as u32,
            diffuse: Some(0),
            normal: None,
        }]
    } else {
        Vec::new()
    };

    let terrain = LoadedModel {
        hash: 0x7E44_A100, // arbitrary key for the merged terrain mesh
        verts,
        indices: tm.indices.clone(),
        draws,
        textures,
        skin: mesh::SkinData::identity(), // identity fit: terrain verts stay in world metres
        clips: Vec::new(),
    };
    progress.step("vertices");

    // Player avatar (Mattias) for the third-person view, at RAW model scale (identity fit) so it
    // sits in world metres alongside the terrain rather than fit-normalised. Idle clip 0x24F8C8E6
    // plus the walk clip 0x53682784 for WASD locomotion.
    // NOTE: world scale and facing are first-pass and not yet calibrated.
    // animate=false: skip load_from_wad's own animgroup scan — all three clips (idle/walk/run)
    // come from ONE cached scan below instead of three full-archive passes (~20 s -> ~7 s load).
    let player = match load_from_wad(wadpath, Some("0xA3C1FABC".to_string()), None, false, None) {
        Ok((v, i, d, t, mut s, _c, h, _)) => {
            progress.step("player");
            s.center = [0.0, 0.0, 0.0];
            s.scale = 1.0;
            let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
            let wanted = [0x24F8_C8E6u32, 0x5368_2784, 0x867B_166D]; // idle, walk, run
            let names = ["idle", "walk", "run"];
            let mut clips: Vec<ClipAnim> = Vec::new();
            for (found, (&h, name)) in load_clips_for_rig(&mut w, &hier, &wanted)
                .into_iter()
                .zip(wanted.iter().zip(names))
            {
                match found {
                    Some(ca) => {
                        println!(
                            "[world] {name} clip 0x{:08X}: {} tracks, {} frames, {:.2}s",
                            ca.name_hash, ca.clip.num_tracks, ca.clip.num_frames, ca.clip.duration
                        );
                        clips.push(ca);
                    }
                    None => eprintln!("[world] {name} clip 0x{h:08X} not found"),
                }
            }
            Some(LoadedModel { hash: h, verts: v, indices: i, draws: d, textures: t, skin: s, clips })
        }
        Err(e) => {
            eprintln!("[world] player avatar 0xA3C1FABC load failed: {e}");
            progress.step("player");
            None
        }
    };
    progress.step("clips");

    // Hi-res c3 streaming-cell geometry near the spawn (opt-in; default off keeps --world stable).
    let cells = if load_cells {
        load_c3_cells(&mut w, 400.0, 16)
    } else {
        Vec::new()
    };
    progress.step(if load_cells { "cells" } else { "cells (skipped)" });

    // World placements (layers_static block 29): a merged marker mesh + the interior-hunt report,
    // plus an attempt to resolve the PMC-subset to real geometry (opt-in via `--placements`).
    let (placements, pmc_models) = if load_placements {
        match mercs2_formats::placement::load_placements(&ls) {
            Ok(pl) => {
                report_interior_hunt(&pl);
                let (verts, indices, draws) = build_placement_markers(&pl);
                println!(
                    "[placements] marker mesh: {} placements -> {} verts / {} tris",
                    pl.len(),
                    verts.len(),
                    indices.len() / 3
                );
                let markers = LoadedModel {
                    hash: 0x504C_4143, // "PLAC" — arbitrary key for the merged marker mesh
                    verts,
                    indices,
                    draws,
                    textures: TexMap::new(),
                    skin: mesh::SkinData::identity(),
                    clips: Vec::new(),
                };
                let pmc = resolve_pmc_geometry(&mut w, &pl);
                (Some(markers), pmc)
            }
            Err(e) => {
                eprintln!("[placements] load failed: {e}");
                (None, Vec::new())
            }
        }
    } else {
        (None, Vec::new())
    };
    progress.step(if load_placements { "placements" } else { "placements (skipped)" });

    // PMC interior (`--interior`): placement-driven interior geometry from state block 667, placed
    // at authored world coords (floor Y≈450.8) so the spawn drops the player inside the room.
    let interior = if spawn_interior {
        match load_pmc_interior(&mut w) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[interior] load failed: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    progress.step(if spawn_interior { "interior" } else { "interior (skipped)" });

    // Exterior props (`--props`): ModelName placements in layers_static (block 29) within
    // EXTERIOR_PROP_RADIUS of the pool spawn, cap EXTERIOR_PROP_CAP distinct meshes.
    let props = if load_props {
        load_model_props(&mut w, &ls, Some(EXTERIOR_SPAWN), EXTERIOR_PROP_RADIUS, EXTERIOR_PROP_CAP)
    } else {
        Vec::new()
    };
    progress.step(if load_props { "props" } else { "props (skipped)" });

    // Interior props (`--interior`): ALL ModelName furniture placements in state block 667, at
    // their authored world transforms (the same anchor the shells are centred on).
    let interior_props = if spawn_interior {
        match wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK) {
            Ok(dec) => load_model_props(&mut w, &dec, None, 0.0, usize::MAX),
            Err(e) => {
                eprintln!("[interior props] state block {PMC_INTERIOR_STATE_BLOCK} decompress failed: {e}");
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    progress.step(if spawn_interior { "interior props" } else { "interior props (skipped)" });

    Ok(WorldData { terrain, player, cells, placements, pmc_models, interior, props, interior_props, hmap })
}

/// Attempt to resolve the PMC-base subset of placements to REAL model geometry (Task 3).
///
/// CRITICAL GAP: `layers_static` Transform records key entities by a u32 *entity key* and carry
/// only pos/quat — NOT a model-asset hash. The `Name` COMP gives a gameplay name
/// (e.g. `_pmcoutpost_bld_barracks01`), not an asset hash either. Mapping name→mesh needs a
/// SEPARATE table that this block does not contain (candidates: the per-cell c3 `model` containers,
/// or an ASET/name-hash lookup — `pandemic_hash_m2(name)` is the natural first guess). We try that
/// hash as the model asset hash and load any that resolve; most will miss, which is the reportable
/// gap. Capped at 64 distinct models. Returns (model, world-pos, yaw) per resolved placement.
fn resolve_pmc_geometry(
    w: &mut wad::Wad,
    placements: &[mercs2_formats::placement::Placement],
) -> Vec<(LoadedModel, [f32; 3], f32)> {
    use mercs2_formats::placement::yaw_from_quat;
    let subset: Vec<&mercs2_formats::placement::Placement> =
        placements.iter().filter(|p| placement_is_pmc_subset(p)).collect();
    // Distinct candidate asset hashes = pandemic_hash_m2(name) for named subset entries.
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out: Vec<(LoadedModel, [f32; 3], f32)> = Vec::new();
    let (mut tried, mut ok) = (0u32, 0u32);
    for p in &subset {
        if out.len() >= 64 {
            break;
        }
        let Some(name) = p.name.as_deref() else { continue };
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        if !seen.insert(hash) {
            continue;
        }
        tried += 1;
        match wad::extract_container(w, hash) {
            Ok(container) => match mesh::build_indexed_from_container(&container) {
                Ok((verts, indices, draws, stats)) => {
                    let mut textures: TexMap = std::collections::HashMap::new();
                    for d in &draws {
                        for h in [d.diffuse, d.normal].into_iter().flatten() {
                            if !textures.contains_key(&h) {
                                if let Ok(t) = wad::extract_texture(w, h) {
                                    textures.insert(h, t);
                                }
                            }
                        }
                    }
                    let mut skin = stats.skin_data();
                    skin.center = [0.0, 0.0, 0.0];
                    skin.scale = 1.0;
                    println!(
                        "[pmc-geo] '{name}' hash=0x{hash:08X}: LOADED {} verts / {} tris",
                        verts.len(), indices.len() / 3
                    );
                    out.push((
                        LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
                        p.pos,
                        yaw_from_quat(&p.quat),
                    ));
                    ok += 1;
                }
                Err(e) => println!("[pmc-geo] '{name}' hash=0x{hash:08X}: container parse FAILED: {e}"),
            },
            Err(_) => { /* no model ASET for this name-hash — the expected gap */ }
        }
    }
    println!(
        "[pmc-geo] name->mesh via pandemic_hash_m2: {} distinct names tried, {} resolved to a model ASET (of {} PMC-subset placements)",
        tried, ok, subset.len()
    );
    out
}

/// c3 streaming-cell grid (ported from `game-scripts/mercs2_c3_grid.py`, GRID_LOGIC_VERSION 3):
/// `c3####` names are linear slots (base 30001) in a 100×100 grid over game-world X/Z
/// [-3900, 3850]; cell centre = min + (col|row + 0.5) · (7750 / 100).
const C3_CELL_ID_BASE: u32 = 30001;
const C3_GRID_COLS: u32 = 100;
const C3_WORLD_MIN: f32 = -3900.0;
const C3_CELL_SIZE: f32 = (3850.0 - C3_WORLD_MIN) / C3_GRID_COLS as f32; // 77.5 m

/// First `c3` + four digits in a block path → streaming cell id (c30123 ⇒ 30123).
fn c3_cell_id_from_path(path: &str) -> Option<u32> {
    let b = path.as_bytes();
    for i in 0..b.len().saturating_sub(5) {
        if (b[i] == b'c' || b[i] == b'C')
            && b[i + 1] == b'3'
            && b[i + 2..i + 6].iter().all(|c| c.is_ascii_digit())
        {
            let slot: u32 = path[i + 2..i + 6].parse().ok()?;
            return Some(C3_CELL_ID_BASE - 1 + slot);
        }
    }
    None
}

/// Game-space (x, z) centre of a streaming cell (metres). Grid carries no height.
fn c3_cell_centre(cell_id: u32) -> (f32, f32) {
    let linear = cell_id.saturating_sub(C3_CELL_ID_BASE);
    let (row, col) = (linear / C3_GRID_COLS, linear % C3_GRID_COLS);
    let x = C3_WORLD_MIN + (col as f32 + 0.5) * C3_CELL_SIZE;
    let z = C3_WORLD_MIN + (row as f32 + 0.5) * C3_CELL_SIZE;
    (x, z)
}

/// Enumerate c3 streaming-cell blocks (PTHS paths matching `c3####`), keep the ones whose block
/// entry table carries a `model`-format container (type_hash 0x5B724250 — the SAME UCFX layout as
/// characters, so `mesh::build_indexed_from_container` parses them), and load the cells whose grid
/// centre lies within `radius` m of the spawn (0,0), capped at `cap`, nearest first. Returns
/// (model, cell-origin translation) pairs; translation is zero when the verts prove already
/// world-space (bbox centre inside the cell bounds — logged either way).
fn load_c3_cells(w: &mut wad::Wad, radius: f32, cap: usize) -> Vec<(LoadedModel, [f32; 3])> {
    use mercs2_formats::ucfx::parse_block_entry_table;

    let c3_blocks: Vec<(u16, u32)> = wad::block_paths(w)
        .iter()
        .enumerate()
        .filter_map(|(i, p)| c3_cell_id_from_path(p).map(|cid| (i as u16, cid)))
        .collect();
    let mut mesh_blocks: Vec<(u16, u32)> = Vec::new();
    for &(blk, cid) in &c3_blocks {
        let Ok(head) = wad::peek_block_head(w, blk, 16384) else { continue };
        let (_count, entries) = parse_block_entry_table(&head);
        if entries.iter().any(|e| e.type_hash == wad::MODEL_TYPE_HASH) {
            mesh_blocks.push((blk, cid));
        }
    }
    println!(
        "[cells] {} c3 blocks in PTHS; {} carry model-format (0x{:08X}) geometry",
        c3_blocks.len(),
        mesh_blocks.len(),
        wad::MODEL_TYPE_HASH
    );

    let mut all: Vec<(f32, u16, u32)> = mesh_blocks
        .iter()
        .map(|&(blk, cid)| {
            let (x, z) = c3_cell_centre(cid);
            ((x * x + z * z).sqrt(), blk, cid)
        })
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut near: Vec<(f32, u16, u32)> = all.iter().copied().filter(|&(d, _, _)| d <= radius).collect();
    if near.is_empty() && !all.is_empty() {
        // No mesh cell inside the strict radius (spawn sits in a mesh-less region of the grid):
        // fall back to the nearest cluster within 2× radius, logged so the miss is visible.
        eprintln!(
            "[cells] no mesh cell within {radius:.0} m of spawn (nearest = cell {} at {:.0} m); falling back to ≤{:.0} m",
            all[0].2, all[0].0, radius * 2.0
        );
        near = all.iter().copied().filter(|&(d, _, _)| d <= radius * 2.0).collect();
    }
    near.truncate(cap);

    let mut out: Vec<(LoadedModel, [f32; 3])> = Vec::new();
    for &(dist, blk, cid) in &near {
        let (cx, cz) = c3_cell_centre(cid);
        let dec = match wad::decompress_block_index(w, blk) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[cells] cell {cid} block {blk}: decompress failed: {e}");
                continue;
            }
        };
        // Slice the model container out of the block, keeping its name hash for the scene key.
        let (count, entries) = parse_block_entry_table(&dec);
        let mut pos = 4 + count as usize * 16;
        let mut model: Option<(u32, usize, usize)> = None;
        for e in &entries {
            let end = pos + e.chunk_size as usize;
            if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                model = Some((e.name_hash, pos, end));
                break;
            }
            pos = end;
        }
        let Some((hash, s0, s1)) = model else {
            eprintln!("[cells] cell {cid} block {blk}: model entry vanished on full decompress");
            continue;
        };
        let (verts, indices, draws, stats) = match mesh::build_indexed_from_container(&dec[s0..s1]) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[cells] cell {cid} block {blk} model 0x{hash:08X}: container parse FAILED: {e}");
                continue;
            }
        };
        // World-space check: bbox centre already inside this cell's bounds ⇒ verts are
        // world-space (spawn at identity); otherwise cell-local (offset to the cell centre).
        let bcx = (stats.bbox_min[0] + stats.bbox_max[0]) * 0.5;
        let bcz = (stats.bbox_min[2] + stats.bbox_max[2]) * 0.5;
        let half = C3_CELL_SIZE * 0.5;
        let world_space = (bcx - cx).abs() <= half && (bcz - cz).abs() <= half;
        let offset = if world_space { [0.0, 0.0, 0.0] } else { [cx, 0.0, cz] };
        let mut textures: TexMap = std::collections::HashMap::new();
        for d in &draws {
            for h in [d.diffuse, d.normal].into_iter().flatten() {
                if !textures.contains_key(&h) {
                    if let Ok(t) = wad::extract_texture(w, h) {
                        textures.insert(h, t);
                    }
                }
            }
        }
        let mut skin = stats.skin_data();
        skin.center = [0.0, 0.0, 0.0];
        skin.scale = 1.0; // native metres; placement comes from the entity transform
        println!(
            "[cells] cell {cid} block {blk} model 0x{hash:08X}: {} verts / {} tris / {} groups / {} textures, d={dist:.0} m, bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] -> {}",
            verts.len(),
            indices.len() / 3,
            draws.len(),
            textures.len(),
            stats.bbox_min[0], stats.bbox_max[0],
            stats.bbox_min[1], stats.bbox_max[1],
            stats.bbox_min[2], stats.bbox_max[2],
            if world_space { "WORLD-SPACE (identity)" } else { "cell-local (offset to cell centre)" }
        );
        out.push((
            LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
            offset,
        ));
    }
    println!("[cells] loaded {} of {} in-range cells (cap {cap})", out.len(), near.len());
    out
}

/// One prop instance's world transform: position + full rotation quaternion (xyzw, native game
/// space — no coordinate flip). Full quat because ~16% of props carry pitch/roll, not just yaw.
type PropInstance = ([f32; 3], [f32; 4]);

/// Load discrete-prop geometry from a UCFX block via the proven `ModelName` COMP recipe
/// (`mercs2_formats::placement::load_model_placements`): each `{key, model_hash}` places the
/// model at the key's `Transform`. DEDUPES by `model_hash` — each distinct container (and its
/// textures) is extracted ONCE — and collects every placement instance for that model.
///
/// When `center` is `Some(c)`, only instances within `radius` metres of `c` (XZ) are kept
/// (exterior bounding); `None` loads all (interior). `cap` bounds the number of DISTINCT meshes
/// loaded (nearest-first when a centre is given). Returns `(model_hash, LoadedModel, instances)`
/// per distinct mesh; logs distinct/placed/skipped(out-of-range)/failed counts.
fn load_model_props(
    w: &mut wad::Wad,
    block: &[u8],
    center: Option<[f32; 3]>,
    radius: f32,
    cap: usize,
) -> Vec<(u32, LoadedModel, Vec<PropInstance>)> {
    let placements = mercs2_formats::placement::load_model_placements(block);
    let total = placements.len();

    // Group instances by distinct model_hash, applying the radius bound (XZ) per instance.
    let mut by_model: std::collections::HashMap<u32, Vec<PropInstance>> = std::collections::HashMap::new();
    let mut skipped_range = 0usize;
    for p in &placements {
        if let Some(c) = center {
            let dx = p.pos[0] - c[0];
            let dz = p.pos[2] - c[2];
            if (dx * dx + dz * dz).sqrt() > radius {
                skipped_range += 1;
                continue;
            }
        }
        by_model.entry(p.model_hash).or_default().push((p.pos, p.quat));
    }

    // Order distinct meshes nearest-first (by their closest instance to the centre) so `cap`
    // keeps the props around the player when bounded; arbitrary order when unbounded.
    let mut distinct: Vec<(u32, Vec<PropInstance>)> = by_model.into_iter().collect();
    if let Some(c) = center {
        let near2 = |insts: &[PropInstance]| {
            insts.iter().map(|(pos, _)| {
                let dx = pos[0] - c[0];
                let dz = pos[2] - c[2];
                dx * dx + dz * dz
            }).fold(f32::INFINITY, f32::min)
        };
        distinct.sort_by(|a, b| near2(&a.1).total_cmp(&near2(&b.1)));
    }
    let distinct_in_range = distinct.len();
    let mut capped_out = 0usize;
    if distinct.len() > cap {
        capped_out = distinct.len() - cap;
        distinct.truncate(cap);
    }

    let mut out: Vec<(u32, LoadedModel, Vec<PropInstance>)> = Vec::new();
    let (mut placed_meshes, mut placed_instances, mut failed) = (0usize, 0usize, 0usize);
    for (hash, instances) in distinct {
        let container = match wad::extract_container(w, hash) {
            Ok(c) => c,
            Err(_) => { failed += 1; continue; }
        };
        let (verts, indices, draws, stats) = match mesh::build_indexed_from_container(&container) {
            Ok(v) => v,
            Err(e) => { eprintln!("[props] model 0x{hash:08X}: container parse FAILED: {e}"); failed += 1; continue; }
        };
        let mut textures: TexMap = std::collections::HashMap::new();
        for d in &draws {
            for h in [d.diffuse, d.normal].into_iter().flatten() {
                if !textures.contains_key(&h) {
                    if let Ok(t) = wad::extract_texture(w, h) {
                        textures.insert(h, t);
                    }
                }
            }
        }
        let mut skin = stats.skin_data();
        skin.center = [0.0, 0.0, 0.0];
        skin.scale = 1.0; // native metres; world placement comes from each instance Transform
        placed_meshes += 1;
        placed_instances += instances.len();
        out.push((
            hash,
            LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
            instances,
        ));
    }
    println!(
        "[props] block ModelName: {total} placements, {distinct_in_range} distinct in range (radius {}), \
         {placed_meshes} meshes placed / {placed_instances} instances, {failed} resolve failures, \
         {capped_out} meshes over cap {cap}, {skipped_range} instances out of range",
        center.map(|_| format!("{radius:.0} m")).unwrap_or_else(|| "all".into())
    );
    out
}

/// WAD block index of the PMC interior asset block (`pmc_interior_P000_Q3.block`). VERIFIED this
/// session to contain NO geometry — only FaceFX (facefxanimationset 0x665EF13E ×4 / facefxactor
/// 0x1CF649BB ×4), Scaleform UI (0xFE0E8320 ×4) and one Havok animation (0x18166555). The interior
/// GEOMETRY is authored as placed instances (real `model` blocks referenced by name) in the
/// interior STATE overlay block below.
const PMC_INTERIOR_ASSET_BLOCK: u16 = 3490;
/// Interior STATE/placement overlay (`vz_state_pmcinterior_P000_Q3.block`): 104 Transform records,
/// authored around the spawn (floor Y≈450.8), each keying a named interior instance (cots, planters,
/// wardrobe, sickbay, lamps, generator, …) plus the room-shell (`pmcoutpost_bld_*`) meshes.
const PMC_INTERIOR_STATE_BLOCK: u16 = 667;
/// Authored game-start spawn (MrxUtil._TeleportHero). The interior placements are already in this
/// world space (their floor sits at Y≈450.8), so loaded geometry is placed at the authored world
/// position with NO synthetic offset (matches the interior state block verbatim).
const PMC_INTERIOR_SPAWN: [f32; 3] = [3794.0427, 450.7505, -3911.0322];

/// The KEYED PMC-interior entities from `docs/mercs2-luacd/src/vz/wifpmcinterior.lua` (`_tBuildings`
/// + the recruit-interior variants): `(entity_key, canonical_name)`. Each entity's AUTHORED world
/// Transform + its `ModelName` mesh live in one of the `INTERIOR_CANDIDATE_BLOCKS` overlay blocks;
/// the name is the `pandemic_hash_m2` fallback when a key has a Transform but no ModelName record.
const PMC_INTERIOR_ENTITIES: &[(u32, &str)] = &[
    (0x000d3c77, "_pmcoutpost_bld_hq_livedin"),
    (0x000d3c78, "_pmcoutpost_bld_hqgarage_livedin"),
    (0x000cf8c2, "_pmcoutpost_bld_hqsuites"),
    (0x000c73ec, "_pmcoutpost_interior_recruitheli"),
    (0x000c740d, "_pmcoutpost_interior_recruitjet"),
    (0x000c73ee, "_pmcoutpost_interior_recruitmechanic"),
];
/// UCFX overlay blocks that may carry the interior entities' Transform / ModelName COMPs:
/// 29 (layers_static), 667 (vz_state_pmcinterior), and the state variants 703/711/461/291
/// (`_hel/_jet/_mec/_mecabsent`).
const INTERIOR_CANDIDATE_BLOCKS: &[u16] = &[29, 667, 703, 711, 461, 291];

/// One resolved PMC-interior entity from `resolve_pmc_interior_entities`: its key, canonical name,
/// the block + AUTHORED world Transform (pos + full quat) that keyed it, and — when found — the mesh
/// hash it renders as (via a keyed `ModelName` COMP, else the `pandemic_hash_m2(name)` fallback).
struct ResolvedInterior {
    key: u32,
    name: &'static str,
    /// (source block, world pos, world quat) — `None` if the key has no Transform in any scan block.
    transform: Option<(u16, [f32; 3], [f32; 4])>,
    /// (model hash, source label) — `None` if neither a ModelName COMP nor a name-hash mesh resolves.
    model: Option<(u32, String)>,
}

/// Resolve the 6 keyed PMC-interior entities against `scan_blocks`: for each key, its first
/// Transform (pos + full quat, native game space, no flip) and its mesh hash — from a keyed
/// `ModelName` COMP if present, else `pandemic_hash_m2(name)` (trying the name with and without the
/// leading `_`) when that hash has a primary model ASET. Shared by `--entity-find` (reports the
/// table) and `load_pmc_interior` (renders the resolved meshes at their authored transforms).
fn resolve_pmc_interior_entities(w: &mut wad::Wad, scan_blocks: &[u16]) -> Vec<ResolvedInterior> {
    // Parse each scan block's Transform (key->pos,quat) and ModelName (key->hash) maps once.
    struct BlockMaps {
        block: u16,
        xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])>,
        models: std::collections::HashMap<u32, u32>,
    }
    let mut blocks: Vec<BlockMaps> = Vec::new();
    for &blk in scan_blocks {
        let Ok(dec) = wad::decompress_block_index(w, blk) else { continue };
        let mut xform = std::collections::HashMap::new();
        if let Ok(pl) = mercs2_formats::placement::load_placements(&dec) {
            for p in &pl {
                xform.entry(p.key).or_insert((p.pos, p.quat));
            }
        }
        let mut models = std::collections::HashMap::new();
        for mp in mercs2_formats::placement::load_model_placements(&dec) {
            models.entry(mp.key).or_insert(mp.model_hash);
        }
        blocks.push(BlockMaps { block: blk, xform, models });
    }

    let mut out = Vec::new();
    for (key, name) in PMC_INTERIOR_ENTITIES {
        let transform = blocks
            .iter()
            .find_map(|b| b.xform.get(key).map(|&(p, q)| (b.block, p, q)));
        // ModelName COMP hash first.
        let mut model: Option<(u32, String)> = blocks
            .iter()
            .find_map(|b| b.models.get(key).map(|&h| (h, format!("ModelName (block {})", b.block))));
        // Fallback: pandemic_hash_m2(name) — with and without the leading '_' — when it has an ASET.
        if model.is_none() {
            for cand in [*name, name.trim_start_matches('_')] {
                let h = mercs2_formats::hash::pandemic_hash_m2(cand);
                if wad::extract_container(w, h).is_ok() {
                    model = Some((h, format!("name-hash '{cand}'")));
                    break;
                }
            }
        }
        out.push(ResolvedInterior { key: *key, name, transform, model });
    }
    out
}

/// Extract one model container by hash and build its renderable `LoadedModel` (verts/tris + draws +
/// textures + skin), with `skin.center=0 / scale=1` so world placement comes purely from the entity
/// Transform. Returns the model + its local bbox. `None` if the hash has no primary ASET / fails.
fn load_model_by_hash(w: &mut wad::Wad, hash: u32) -> Option<(LoadedModel, [f32; 3], [f32; 3])> {
    let container = wad::extract_container(w, hash).ok()?;
    let (verts, indices, draws, stats) = mesh::build_indexed_from_container(&container).ok()?;
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = wad::extract_texture(w, h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0; // native metres; world placement is the authored Transform, no offset
    Some((
        LoadedModel { hash, verts, indices, draws, textures, skin, clips: Vec::new() },
        stats.bbox_min,
        stats.bbox_max,
    ))
}

/// Load the PMC interior for `--interior`, ASSEMBLED FROM ITS KEYED ENTITIES.
///
/// STRUCTURE (Task-1 verified, `--entity-find`): the interior is the union of the keyed
/// `pmcoutpost_interior_recruit*` meshes, placed at their AUTHORED Transforms (native game space,
/// full quat, NO bbox-guess offset). Of the 6 documented keys (`wifpmcinterior.lua` `_tBuildings` +
/// recruit starters):
///  * `recruitjet` (0x000c740d) → Transform in block 711 (vz_state_pmcinterior_jet) @ (3750,450,-3840);
///    mesh 0x86D7CF92 (name-hash `pmcoutpost_interior_recruitjet`, block 2612), 8970 v / 10735 t,
///    local-bbox already in the interior world frame (x[48.8,72.1] z[-69.7,-40.6]).
///  * `recruitmechanic` (0x000c73ee) → Transform in block 461 (…_mec) @ (3750,450,-3840); mesh
///    0xE8EB75D7 (name-hash `pmcoutpost_interior_recruitmechanic`, block 2612), 19197 v / 31726 t.
///  * `recruitheli` (0x000c73ec) → Transform in block 703 (…_hel) @ (3750,450,-3840); GAP: no mesh
///    (no `recruitheli` model ASET in vz.wad; hash 0x634F1F65 absent) — placement kept, mesh skipped.
///  * The 3 `_tBuildings` (`hq_livedin` 0x000d3c77, `hqgarage_livedin` 0x000d3c78, `hqsuites`
///    0x000cf8c2) are the EXTERIOR base buildings — their Transforms live in blocks 329/226
///    (vz_state_pmc[_livedin]) at the main-map compound (~(2540..2647, -14, -951..-1015)), NOT the
///    off-map interior cell — and have NO discrete mesh (loaded as baked exterior geometry). They are
///    deliberately NOT placed here (they belong to the exterior, ~4 km from the interior spawn).
///
/// The block-667 `ModelName` furniture (the Custom Outfit Wardrobe) is placed SEPARATELY via the
/// `interior_props` prop-instancing path in `load_world_data`. Returns (model, world pos, world quat)
/// per instance — placed verbatim, no synthetic offset. The player spawns at `PMC_INTERIOR_SPAWN`.
fn load_pmc_interior(w: &mut wad::Wad) -> Result<Vec<(LoadedModel, [f32; 3], [f32; 4])>, String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    use mercs2_formats::placement::{load_model_placements, load_placements};
    use std::collections::HashMap;

    let mut out: Vec<(LoadedModel, [f32; 3], [f32; 4])> = Vec::new();
    let (mut tv, mut tt) = (0usize, 0usize);
    let mut distinct: HashMap<u32, usize> = HashMap::new();
    let mut wmin = [f32::MAX; 3];
    let mut wmax = [f32::MIN; 3];

    // The game groups the interior into the vz_state_pmcinterior blocks it loads as a layer set (base
    // + starter variants). Follow that grouping: for EVERY entity in those blocks, resolve its mesh via
    // the proven recipe — the `ModelName` COMP hash if present, else the entity name hashed
    // (`pandemic_hash_m2`; asset names drop the leading `_`) — and place it at its authored Transform.
    // No manual mesh identification: we render the block the game renders. Locators/hardpoints (no
    // mesh) simply fail to resolve and are skipped.
    const INTERIOR_STATE_BLOCKS: &[u16] = &[667, 711, 461, 703]; // base + jet + mec + hel variants
    for &blk in INTERIOR_STATE_BLOCKS {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let model_by_key: HashMap<u32, u32> = load_model_placements(&data)
            .into_iter()
            .map(|mp| (mp.key, mp.model_hash))
            .collect();
        let placements = load_placements(&data).unwrap_or_default();
        let mut resolved = 0usize;
        for p in &placements {
            let hash = model_by_key.get(&p.key).copied().or_else(|| {
                p.name.as_deref().map(|n| {
                    // asset name = the entity name minus the leading `_` and the trailing ` 0xKEY`
                    // hex-id suffix that placement Name COMPs carry ("name 0x000c740d").
                    let base = n.split(" 0x").next().unwrap_or(n).trim_start_matches('_');
                    pandemic_hash_m2(base)
                })
            });
            let Some(hash) = hash else { continue };
            let Some((m, bmin, bmax)) = load_model_by_hash(w, hash) else { continue };
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            for c in 0..3 {
                wmin[c] = wmin[c].min(p.pos[c] + bmin[c]);
                wmax[c] = wmax[c].max(p.pos[c] + bmax[c]);
            }
            *distinct.entry(hash).or_insert(0) += 1;
            resolved += 1;
            out.push((m, p.pos, p.quat));
        }
        println!(
            "[interior] block {blk}: {} transforms, {} ModelName, {resolved} resolved to a mesh",
            placements.len(), model_by_key.len()
        );
    }

    // The starter-room bays are Lua-spawned actors anchored to the HqInterior origin (mrxstarter.lua
    // SpawnActor, anchor "HqInterior" @ (3750,450,-3840)), NOT vz_state placements — so add them
    // explicitly at that origin: recruitjet + recruitmechanic (recruitheli's mesh is absent from vz.wad).
    const ACTOR_ORIGIN: [f32; 3] = [3750.0, 450.0, -3840.0];
    const IDENT_QUAT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    for hash in [0x86D7CF92u32, 0xE8EB75D7] {
        if let Some((m, bmin, bmax)) = load_model_by_hash(w, hash) {
            for c in 0..3 {
                wmin[c] = wmin[c].min(ACTOR_ORIGIN[c] + bmin[c]);
                wmax[c] = wmax[c].max(ACTOR_ORIGIN[c] + bmax[c]);
            }
            tv += m.verts.len();
            tt += m.indices.len() / 3;
            *distinct.entry(hash).or_insert(0) += 1;
            println!("[interior] recruit bay 0x{hash:08X}: {} v / {} t @ actor-origin", m.verts.len(), m.indices.len() / 3);
            out.push((m, ACTOR_ORIGIN, IDENT_QUAT));
        }
    }

    println!(
        "[interior] assembled {} instance(s) ({} distinct meshes), {tv} verts / {tt} tris; spawn @ ({:.1},{:.1},{:.1})",
        out.len(), distinct.len(), PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2]
    );
    if !out.is_empty() {
        println!(
            "[interior] WORLD BBOX min=({:.1},{:.1},{:.1}) max=({:.1},{:.1},{:.1}) center=({:.1},{:.1},{:.1}) dims=({:.1},{:.1},{:.1})",
            wmin[0], wmin[1], wmin[2], wmax[0], wmax[1], wmax[2],
            (wmin[0]+wmax[0])/2.0, (wmin[1]+wmax[1])/2.0, (wmin[2]+wmax[2])/2.0,
            wmax[0]-wmin[0], wmax[1]-wmin[1], wmax[2]-wmin[2]
        );
    }
    Ok(out)
}

/// Headless probe (Task 1): dump the PMC interior ASSET block (3490) chunk inventory
/// (type/name/size + type histogram) to prove whether it carries geometry, then run the interior
/// loader (placement-driven off state block 667) to report resolved geometry + world Y extent.
fn interior_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::ucfx::parse_block_entry_table;
    let mut w = wad::open(wadpath)?;
    let path = wad::block_paths(&w)
        .get(PMC_INTERIOR_ASSET_BLOCK as usize)
        .cloned()
        .unwrap_or_default();
    let dec = wad::decompress_block_index(&mut w, PMC_INTERIOR_ASSET_BLOCK)
        .map_err(|e| format!("block {PMC_INTERIOR_ASSET_BLOCK} decompress: {e}"))?;
    let (count, entries) = parse_block_entry_table(&dec);
    println!(
        "[interior-probe] asset block {PMC_INTERIOR_ASSET_BLOCK} path='{path}' size={} bytes, {count} chunks:",
        dec.len()
    );
    for (i, e) in entries.iter().enumerate() {
        let kind = if e.type_hash == wad::MODEL_TYPE_HASH { " (MODEL)" } else { "" };
        println!(
            "   [{i:>3}] type=0x{:08X}{kind} name=0x{:08X} size={}",
            e.type_hash, e.name_hash, e.chunk_size
        );
    }
    let mut by_type: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for e in &entries {
        *by_type.entry(e.type_hash).or_default() += 1;
    }
    let model_chunks = entries.iter().filter(|e| e.type_hash == wad::MODEL_TYPE_HASH).count();
    println!("[interior-probe] chunk-type histogram (model chunks: {model_chunks}):");
    for (t, n) in &by_type {
        println!("   type 0x{t:08X}: {n}");
    }
    // Interior geometry.
    let loaded = load_pmc_interior(&mut w)?;
    println!("[interior-probe] {} placed interior instance(s).", loaded.len());

    // Floor check: sample every loaded triangle that overlaps the player's XZ (world) — applying each
    // instance's AUTHORED transform (quat rotation + pos translation) — and report the mesh world-Y
    // range there vs the authored spawn Y (the numeric floor gap).
    use mercs2_core::glam::{Quat, Vec3};
    let (px, py, pz) = (PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2]);
    let (mut fy_min, mut fy_max) = (f32::INFINITY, f32::NEG_INFINITY);
    let mut hits = 0u32;
    for (m, pos, quat) in &loaded {
        let q = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
        let t = Vec3::new(pos[0], pos[1], pos[2]);
        let wp = |v: [f32; 3]| -> Vec3 { q * Vec3::new(v[0], v[1], v[2]) + t };
        for tri in m.indices.chunks_exact(3) {
            let a = wp(m.verts[tri[0] as usize].pos);
            let b = wp(m.verts[tri[1] as usize].pos);
            let c = wp(m.verts[tri[2] as usize].pos);
            if let Some(y) = tri_height_at(px, pz, [a.x, b.x, c.x], [a.z, b.z, c.z], [a.y, b.y, c.y]) {
                fy_min = fy_min.min(y);
                fy_max = fy_max.max(y);
                hits += 1;
            }
        }
    }
    if hits > 0 {
        let nearest = if (fy_min - py).abs() <= (fy_max - py).abs() { fy_min } else { fy_max };
        println!(
            "[interior-probe] mesh Y at player XZ ({px:.1},{pz:.1}): {hits} triangles, Y[{fy_min:.2},{fy_max:.2}]; \
             spawn Y={py:.2}; gap to nearest surface = {:.2} m",
            py - nearest
        );
    } else {
        println!(
            "[interior-probe] NO interior triangle overlaps the player XZ ({px:.1},{pz:.1}) — \
             the shell offset does not place a floor directly under the spawn (see local-bbox X/Z above)"
        );
    }
    Ok(())
}

/// Task 1 — enumerate the real interior ROOM meshes (authoritative, not guessing).
///
/// 1. Dump `wad::model_list()` (every primary model ASET hash), reverse each hash via the repo
///    rainbow table, and keep only names whose lowercase contains `interior`, `hq`, `mainhall`,
///    or one of the candidate template names — then extract each survivor's container and print
///    verts / tris / local bbox (room shells are LARGE + roughly hollow).
/// 2. Directly hash-test the exact candidate template names (`HqInterior`, `AllHq_Interior`, …)
///    with `pandemic_hash_m2` and report which resolve to a real model container (+ bbox).
///
/// Pure logging; no rendering. The player-lands-inside signal per candidate is computed here too
/// (bbox centre + the authored (+44,0,-71) hardpoint offset vs the room-local bbox) so the caller
/// can pick the room without a render pass.
fn interior_list(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    println!("[interior-list] {} primary model ASET(s) in WAD.", models.len());

    // Reverse every model hash via the rainbow table.
    let all_hashes: std::collections::BTreeSet<u32> = models.iter().map(|&(h, _)| h).collect();
    let names = rainbow_names(&all_hashes);

    // Filter tokens (lowercase substring match on the reversed name).
    const TOKENS: &[&str] = &["interior", "hq", "mainhall"];
    // Candidate template names of interest (exact, case-insensitive) — always surfaced even if the
    // reversed name doesn't contain a token.
    const TEMPLATES: &[&str] = &[
        "hqinterior", "allhq_interior", "chihq_interior", "gurhq_interior", "oilhq_interior",
        "pmchq_interior", "mainhall", "_merida_bld_pmcautoshop_interior", "_proutpost_interior_job",
        "merida_bld_pmcautoshop_interior", "proutpost_interior_job",
    ];

    // The authored hardpoint offset (local, pre-rotation) that the player teleport lands at, from
    // the room-mesh's local origin: spawn (3794.04,450.75,-3911.03) - actor pos (3750,450,-3840).
    const HP_LOCAL: [f32; 3] = [
        PMC_INTERIOR_SPAWN[0] - 3750.0,
        PMC_INTERIOR_SPAWN[1] - 450.0,
        PMC_INTERIOR_SPAWN[2] - (-3840.0),
    ];

    // Collect matching candidates, extract geometry, and print name/hash/verts/bbox.
    let mut matched: Vec<(u32, String)> = Vec::new();
    for &(hash, _blk) in &models {
        let Some(name) = names.get(&hash) else { continue };
        let lc = name.to_lowercase();
        let is_tok = TOKENS.iter().any(|t| lc.contains(t));
        let is_tpl = TEMPLATES.iter().any(|t| lc == *t);
        if is_tok || is_tpl {
            matched.push((hash, name.clone()));
        }
    }
    matched.sort_by(|a, b| a.1.cmp(&b.1));
    println!(
        "[interior-list] {} model(s) matched interior/hq/mainhall (of {} reversed):",
        matched.len(),
        names.len()
    );
    // Per-candidate geometry + bbox + player-inside test. Sort a room-sized list by vert count.
    struct Cand {
        hash: u32,
        name: String,
        verts: usize,
        tris: usize,
        bmin: [f32; 3],
        bmax: [f32; 3],
        inside: bool,
    }
    let mut cands: Vec<Cand> = Vec::new();
    for (hash, name) in &matched {
        match load_model_by_hash(&mut w, *hash) {
            Some((m, bmin, bmax)) => {
                // Player-inside test: is the hardpoint-local offset within the mesh's local bbox?
                let inside = (0..3).all(|k| HP_LOCAL[k] >= bmin[k] - 0.5 && HP_LOCAL[k] <= bmax[k] + 0.5);
                let (dx, dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
                println!(
                    "   {name:<40} 0x{hash:08X}  {:>6} v / {:>6} t  bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] (dims {:.1}x{:.1}x{:.1}) player-inside={}",
                    m.verts.len(), m.indices.len() / 3,
                    bmin[0], bmax[0], bmin[1], bmax[1], bmin[2], bmax[2], dx, dy, dz, inside
                );
                cands.push(Cand {
                    hash: *hash, name: name.clone(), verts: m.verts.len(), tris: m.indices.len() / 3,
                    bmin, bmax, inside,
                });
            }
            None => println!("   {name:<40} 0x{hash:08X}  (container extract/build FAILED)"),
        }
    }

    // Task-1 direct hash test of the exact candidate template names.
    println!("\n[interior-list] direct hash-test of candidate template names:");
    let template_probe: &[&str] = &[
        "HqInterior", "AllHq_Interior", "ChiHq_Interior", "GurHq_Interior", "OilHq_Interior",
        "PmcHq_Interior", "MainHall", "_merida_bld_pmcautoshop_interior", "_proutpost_interior_job",
        // also the leading-underscore-trimmed variants (name→hash fallback tries both)
        "merida_bld_pmcautoshop_interior", "proutpost_interior_job",
        // and the proven recruit-interior meshes for reference
        "pmcoutpost_interior_recruitjet", "pmcoutpost_interior_recruitmechanic",
        "pmcoutpost_interior_recruitheli",
        // whole-room / HQ-building shell candidates surfaced from the rainbow table
        "pmcoutpost_bld_hq", "pmcoutpost_bld_hq_livedin", "_pmcoutpost_bld_hq_livedin",
        "pmcoutpost_bld_hqinterior_livedina", "pmcoutpost_bld_hqinterior_livedin_b",
        "pmcoutpost_bld_hqsuites", "_pmcoutpost_bld_hqsuites", "pmcoutpost_bld_hqgarage",
        "groutpost_interior_job", "aloutpost_interior_job", "ocoutpost_interior_job",
        "chinaoutpost_interior_job",
    ];
    for n in template_probe {
        let h = mercs2_formats::hash::pandemic_hash_m2(n);
        match load_model_by_hash(&mut w, h) {
            Some((m, bmin, bmax)) => {
                let inside = (0..3).all(|k| HP_LOCAL[k] >= bmin[k] - 0.5 && HP_LOCAL[k] <= bmax[k] + 0.5);
                println!(
                    "   '{n}' 0x{h:08X} -> REAL MESH: {} v / {} t  bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] player-inside={}",
                    m.verts.len(), m.indices.len() / 3,
                    bmin[0], bmax[0], bmin[1], bmax[1], bmin[2], bmax[2], inside
                );
            }
            None => println!("   '{n}' 0x{h:08X} -> absent (no primary model ASET)"),
        }
    }

    // Room-shell shortlist: LARGE (>=2000 v) candidates, biggest first, flag player-inside.
    cands.sort_by(|a, b| b.verts.cmp(&a.verts));
    println!("\n[interior-list] room-sized shortlist (>=2000 v), largest first:");
    for c in cands.iter().filter(|c| c.verts >= 2000) {
        let ext = (0..3).map(|k| c.bmax[k] - c.bmin[k]).fold(0.0f32, f32::max);
        println!(
            "   {:<40} 0x{:08X}  {} v / {} t  max-extent {:.1} m  player-inside={}",
            c.name, c.hash, c.verts, c.tris, ext, c.inside
        );
    }
    println!(
        "[interior-list] hardpoint-local offset (spawn - actor@(3750,450,-3840)) = ({:.2},{:.2},{:.2})",
        HP_LOCAL[0], HP_LOCAL[1], HP_LOCAL[2]
    );
    Ok(())
}

/// One resolved interior entity: its authored Transform (from the block that keyed it), the mesh
/// hash + which source (ModelName COMP vs name→hash fallback), and the container geometry stats.
struct FoundEntity {
    key: u32,
    name: &'static str,
    /// (block, pos, quat) of the winning Transform record (first block that carried one).
    transform: Option<(u16, [f32; 3], [f32; 4])>,
    /// (model_hash, source, block) of the resolved mesh. Source "ModelName" = keyed COMP record;
    /// "name-hash" = `pandemic_hash_m2(name)` fallback.
    model: Option<(u32, String, u16)>,
    /// (verts, tris, local bbox min, local bbox max) if `extract_container` + build succeeded.
    container: Option<(usize, usize, [f32; 3], [f32; 3])>,
}

/// Task 1: for each of the PMC-interior keys, scan the candidate overlay blocks for its Transform
/// (→ pos+quat) and ModelName (→ model_hash), test `extract_container(model_hash)`, and — when a
/// key has a Transform but no ModelName — try the `pandemic_hash_m2(name)` mesh fallback. Prints a
/// per-key table; the same resolution is what `load_pmc_interior` consumes. `extra_keys` (if given)
/// REPLACES the default 6 keys, using the key itself as the name (no name-hash fallback then).
fn entity_find(wadpath: &str, extra_keys: &[u32]) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;

    // Key -> canonical name (owned Strings so ad-hoc keys work). Default = the documented 6.
    let entities: Vec<(u32, String)> = if extra_keys.is_empty() {
        PMC_INTERIOR_ENTITIES.iter().map(|(k, n)| (*k, n.to_string())).collect()
    } else {
        extra_keys.iter().map(|k| (*k, format!("0x{k:08X}"))).collect()
    };

    // Decompress each candidate block once and parse its placements + model placements.
    struct BlockData {
        block: u16,
        // key -> (pos, quat) from load_placements (Transform+Name join).
        xform: std::collections::HashMap<u32, ([f32; 3], [f32; 4])>,
        // key -> model_hash from load_model_placements (ModelName COMP).
        models: std::collections::HashMap<u32, u32>,
    }
    // Which blocks to scan: the fixed candidates, or (MERCS2_SCANALL=1) every UCFX overlay block
    // — a WAD-wide hunt for a key's owning block.
    let scan_blocks: Vec<u16> = if std::env::var_os("MERCS2_SCANALL").is_some() {
        (0..wad::block_paths(&w).len() as u16).collect()
    } else {
        INTERIOR_CANDIDATE_BLOCKS.to_vec()
    };
    let want: std::collections::HashSet<u32> = entities.iter().map(|(k, _)| *k).collect();

    let mut blocks: Vec<BlockData> = Vec::new();
    for &blk in &scan_blocks {
        let dec = match wad::decompress_block_index(&mut w, blk) {
            Ok(d) => d,
            Err(e) => {
                println!("[entity-find] block {blk}: decompress failed: {e}");
                continue;
            }
        };
        let mut xform = std::collections::HashMap::new();
        if let Ok(pl) = mercs2_formats::placement::load_placements(&dec) {
            for p in &pl {
                xform.entry(p.key).or_insert((p.pos, p.quat));
            }
        }
        let mut models = std::collections::HashMap::new();
        for mp in mercs2_formats::placement::load_model_placements(&dec) {
            models.entry(mp.key).or_insert(mp.model_hash);
        }
        // In scan-all mode only log blocks that actually key one of the wanted entities.
        let hits: Vec<u32> = want.iter().filter(|k| xform.contains_key(k) || models.contains_key(k)).copied().collect();
        if !std::env::var_os("MERCS2_SCANALL").is_some() {
            println!(
                "[entity-find] block {blk}: {} Transform keys, {} ModelName keys",
                xform.len(), models.len()
            );
        } else if !hits.is_empty() {
            let path = wad::block_paths(&w).get(blk as usize).cloned().unwrap_or_default();
            println!(
                "[entity-find] block {blk} '{path}': keys {}",
                hits.iter().map(|k| format!("0x{k:08X}")).collect::<Vec<_>>().join(",")
            );
        }
        blocks.push(BlockData { block: blk, xform, models });
    }

    let mut found: Vec<FoundEntity> = Vec::new();
    for (key, name) in &entities {
        // First Transform across candidate blocks (interior overlays don't duplicate an entity).
        let mut transform = None;
        for b in &blocks {
            if let Some(&(pos, quat)) = b.xform.get(key) {
                transform = Some((b.block, pos, quat));
                break;
            }
        }
        // First ModelName across candidate blocks.
        let mut model: Option<(u32, String, u16)> = None;
        for b in &blocks {
            if let Some(&h) = b.models.get(key) {
                model = Some((h, "ModelName".to_string(), b.block));
                break;
            }
        }
        // Fallback: no ModelName but a canonical name — try pandemic_hash_m2(name) as the mesh hash.
        if model.is_none() && extra_keys.is_empty() {
            // Try both the raw name and the name with a leading underscore stripped (the Lua
            // building keys are written with a leading '_').
            let cands = [name.as_str(), name.trim_start_matches('_')];
            for cand in cands {
                let h = mercs2_formats::hash::pandemic_hash_m2(cand);
                if wad::extract_container(&mut w, h).is_ok() {
                    model = Some((h, format!("name-hash '{cand}'"), 0));
                    break;
                }
            }
        }
        // Resolve container geometry for whichever mesh hash we have.
        let container = model.as_ref().and_then(|(h, _, _)| {
            let h = *h;
            wad::extract_container(&mut w, h).ok().and_then(|c| {
                mesh::build_indexed_from_container(&c)
                    .ok()
                    .map(|(v, idx, _d, s)| (v.len(), idx.len() / 3, s.bbox_min, s.bbox_max))
            })
        });
        found.push(FoundEntity {
            key: *key,
            name: PMC_INTERIOR_ENTITIES.iter().find(|(k, _)| k == key).map(|(_, n)| *n).unwrap_or("<adhoc>"),
            transform,
            model,
            container,
        });
    }

    // Report table.
    println!("\n[entity-find] ===== PMC INTERIOR ENTITY TABLE ({} keys) =====", found.len());
    for f in &found {
        println!("\n  key 0x{:08X}  {}", f.key, f.name);
        match f.transform {
            Some((blk, pos, quat)) => println!(
                "    Transform : block {blk}  pos=({:.3},{:.3},{:.3})  quat=({:+.4},{:+.4},{:+.4},{:+.4})  yaw={:.3}rad",
                pos[0], pos[1], pos[2], quat[0], quat[1], quat[2], quat[3],
                mercs2_formats::placement::yaw_from_quat(&quat)
            ),
            None => println!("    Transform : MISS (no Transform record for this key in any candidate block)"),
        }
        match &f.model {
            Some((h, src, blk)) => {
                let where_ = if *blk == 0 { String::new() } else { format!(" (block {blk})") };
                println!("    ModelName : 0x{h:08X}  via {src}{where_}");
            }
            None => println!("    ModelName : MISS (no ModelName COMP, and name-hash mesh not in WAD)"),
        }
        match f.container {
            Some((v, t, bmin, bmax)) => println!(
                "    Mesh      : extract_container OK — {v} verts / {t} tris  local-bbox x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}]",
                bmin[0], bmax[0], bmin[1], bmax[1], bmin[2], bmax[2]
            ),
            None => match &f.model {
                Some((h, _, _)) => println!("    Mesh      : MISS — model 0x{h:08X} has no primary ASET / container build failed"),
                None => println!("    Mesh      : MISS — no model hash to resolve"),
            },
        }
    }
    let resolved = found.iter().filter(|f| f.container.is_some()).count();
    println!(
        "\n[entity-find] summary: {}/{} keys resolve to a real mesh; {} have a Transform.",
        resolved,
        found.len(),
        found.iter().filter(|f| f.transform.is_some()).count()
    );
    Ok(())
}

/// The ECS `Model` component m2 hash (`pandemic_hash_m2("model")`), stride 4 = one u32 mesh handle.
/// Same value as `wad::MODEL_TYPE_HASH` (the MESH-block "Model" CHDR class hash).
const MODEL_COMP_HASH: u32 = 0x5B72_4250;

/// Headless COMP probe (RESEARCH BRICK deliverable a–e):
///  1. Enumerate every COMP in layers_static (block 29) AND the PMC interior state block (667).
///  2. Reverse-scan every COMP's data blob for the anchor interior/model hashes and c3 model hashes.
///  3. When an anchor is found: report COMP type, byte offset in the record, and the owning entity.
///  4. Cross-check the winning COMP against the ECS `Model` class (0x5b724250, stride 4).
///  5. Prove one entity end-to-end: key -> Model COMP -> mesh hash -> extract_container -> verts/tris.
fn comp_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::placement::{comp_inventory, load_placements, yaw_from_quat, CompInfo};
    let mut w = wad::open(wadpath)?;

    // The anchor model hashes (verified to load via wad::extract_container this session).
    let anchors: &[(u32, &str)] = &[
        (0x50AA_CA22, "pmcoutpost_bld_hq"),
        (0xC087_777D, "pmcoutpost_bld_pool"),
        (0xD5D6_5249, "pmcoutpost_bld_hqsuites"),
    ];

    // Resolve the two target blocks by the same live index the rest of the engine uses.
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let state = wad::decompress_block_index(&mut w, PMC_INTERIOR_STATE_BLOCK)
        .map_err(|e| format!("interior state block {PMC_INTERIOR_STATE_BLOCK} decompress: {e}"))?;

    // ---- (a) COMP inventory for both blocks -------------------------------------------------
    for (label, blk) in [("layers_static(29)", &ls), ("interior_state(667)", &state)] {
        let inv = comp_inventory(blk);
        let mut by_name: std::collections::BTreeMap<String, (usize, Option<u32>)> =
            std::collections::BTreeMap::new();
        for c in &inv {
            let name = c.info_name.clone().unwrap_or_else(|| "<no-info>".into());
            let e = by_name.entry(name).or_insert((0, c.payload_stride));
            e.0 += 1;
        }
        println!(
            "[comp-probe] === {label}: {} COMPs across sub-blocks, {} distinct types ===",
            inv.len(),
            by_name.len()
        );
        for (name, (count, stride)) in &by_name {
            println!(
                "[comp-probe]   {name:<32} x{count:<5} schm payload_stride={}",
                stride.map(|s| s.to_string()).unwrap_or_else(|| "?".into())
            );
        }
    }

    // ---- (b) Reverse-anchor search: scan EVERY COMP data blob for the anchor hashes ----------
    // A "hit" = an anchor u32 appears as a little-endian dword at any byte offset inside a COMP
    // data blob. Report the COMP type, the byte offset within the (4 + payload_stride) record,
    // and the record's leading u32 entity key.
    let anchor_set: std::collections::HashMap<u32, &str> =
        anchors.iter().map(|(h, n)| (*h, *n)).collect();

    for (label, blk) in [("layers_static(29)", &ls), ("interior_state(667)", &state)] {
        // Build a key->name / key->transform map for this block so we can name the owning entity.
        let placements = load_placements(blk).unwrap_or_default();
        let name_by_key: std::collections::HashMap<u32, String> = placements
            .iter()
            .filter_map(|p| p.name.clone().map(|n| (p.key, n)))
            .collect();
        let xform_by_key: std::collections::HashMap<u32, ([f32; 3], [f32; 4])> =
            placements.iter().map(|p| (p.key, (p.pos, p.quat))).collect();

        let inv: Vec<CompInfo> = comp_inventory(blk);
        let mut total_hits = 0usize;
        for c in &inv {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let blob = &blk[off..off + size];
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(0);
            let mut i = 0usize;
            while i + 4 <= blob.len() {
                let v = u32::from_le_bytes([blob[i], blob[i + 1], blob[i + 2], blob[i + 3]]);
                if let Some(model_name) = anchor_set.get(&v) {
                    total_hits += 1;
                    let (rec_idx, field_off, key) = if stride > 0 {
                        let ri = i / stride;
                        let fo = i % stride;
                        let k = u32::from_le_bytes([
                            blob[ri * stride],
                            blob[ri * stride + 1],
                            blob[ri * stride + 2],
                            blob[ri * stride + 3],
                        ]);
                        (ri as isize, fo as isize, k)
                    } else {
                        (-1, -1, 0)
                    };
                    let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                    println!(
                        "[comp-probe] ANCHOR HIT in {label}: COMP='{}' hash=0x{v:08X} ({model_name}) \
                         at data+{i} (record {rec_idx}, field_off={field_off}, stride={stride}) \
                         entity_key=0x{key:08X} name='{ename}'",
                        c.info_name.as_deref().unwrap_or("<no-info>")
                    );
                }
                i += 4;
            }
        }
        if total_hits == 0 {
            println!("[comp-probe] {label}: NO anchor model hash found verbatim in any COMP data blob");
        }

        // ---- The name->mesh link: the "ModelName" COMP (stride-4 u32 = pandemic_hash_m2(model
        // name string), which equals the model ASET asset_hash). Dump records + resolve each. ----
        for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("ModelName")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            let mut resolved = 0usize;
            for r in 0..n {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                if wad::extract_container(&mut w, mesh).is_ok() {
                    resolved += 1;
                }
            }
            println!(
                "[comp-probe] {label}: ModelName COMP (sub_block {}) — {n} records stride={stride}, \
                 {resolved} resolve via extract_container (val == pandemic_hash_m2(model-name) == model ASET hash)",
                c.sub_block
            );
            for r in 0..n.min(6) {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let loads = wad::extract_container(&mut w, mesh).is_ok();
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                println!(
                    "[comp-probe]     rec[{r}] key=0x{key:08X} modelhash=0x{mesh:08X} \
                     placement_name='{ename}' extract_container={}",
                    if loads { "OK" } else { "miss" }
                );
            }
        }

        // ---- Direct Model-COMP dump: any COMP whose info name is "Model" (stride-4 u32 handles) ----
        for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("Model")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            println!(
                "[comp-probe] {label}: Model COMP (sub_block {}) — {n} records, stride={stride}",
                c.sub_block
            );
            for r in 0..n.min(8) {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let loads = wad::extract_container(&mut w, mesh).is_ok();
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                println!(
                    "[comp-probe]     rec[{r}] key=0x{key:08X} mesh=0x{mesh:08X} \
                     name='{ename}' extract_container={}",
                    if loads { "OK" } else { "miss" }
                );
            }
        }

        // ---- (e) end-to-end proof: pick the first ModelName record whose mesh loads, resolve fully ----
        'proof: for c in inv.iter().filter(|c| c.info_name.as_deref() == Some("ModelName")) {
            let (Some(off), Some(size)) = (c.data_off, c.data_size) else { continue };
            if off + size > blk.len() {
                continue;
            }
            let stride = c.payload_stride.map(|s| s as usize + 4).unwrap_or(8);
            let blob = &blk[off..off + size];
            let n = blob.len() / stride.max(1);
            for r in 0..n {
                let base = r * stride;
                if base + 8 > blob.len() {
                    break;
                }
                let key = u32::from_le_bytes([blob[base], blob[base + 1], blob[base + 2], blob[base + 3]]);
                let mesh = u32::from_le_bytes([blob[base + 4], blob[base + 5], blob[base + 6], blob[base + 7]]);
                let Ok(container) = wad::extract_container(&mut w, mesh) else { continue };
                let Ok((verts, indices, draws, _stats)) =
                    mesh::build_indexed_from_container(&container)
                else {
                    continue;
                };
                let ename = name_by_key.get(&key).cloned().unwrap_or_else(|| "<unknown>".into());
                let (pos, quat) = xform_by_key.get(&key).cloned().unwrap_or(([0.0; 3], [0.0, 0.0, 0.0, 1.0]));
                println!(
                    "[comp-probe] *** END-TO-END PROOF ({label}) ***\n\
                     [comp-probe]   entity_key = 0x{key:08X}\n\
                     [comp-probe]   name       = '{ename}'\n\
                     [comp-probe]   Model.mesh = 0x{mesh:08X}\n\
                     [comp-probe]   loaded     = {} verts / {} tris / {} draw groups\n\
                     [comp-probe]   transform  = pos ({:.2},{:.2},{:.2}) yaw {:.3} rad",
                    verts.len(),
                    indices.len() / 3,
                    draws.len(),
                    pos[0], pos[1], pos[2],
                    yaw_from_quat(&quat)
                );
                break 'proof;
            }
        }
    }

    // ---- (d) c3-vs-placement for exterior buildings -----------------------------------------
    // Take a named exterior building placement, find the c3 cell covering its XZ, load that cell's
    // mesh, and report vert/tri counts — i.e. is the building baked into the c3 cell geometry?
    let placements = load_placements(&ls).unwrap_or_default();
    let bld = placements.iter().find(|p| {
        p.name.as_deref().map(|n| {
            let l = n.to_ascii_lowercase();
            l.contains("_bld_") && !l.contains("pmcoutpost")
        }).unwrap_or(false)
    });
    match bld {
        Some(p) => {
            println!(
                "[comp-probe] (d) exterior building sample: name='{}' key=0x{:08X} pos=({:.1},{:.1},{:.1})",
                p.name.as_deref().unwrap_or(""), p.key, p.pos[0], p.pos[1], p.pos[2]
            );
            // Is this building's name resolvable to a model ASET by hash (placement path)?
            let h = mercs2_formats::hash::pandemic_hash_m2(p.name.as_deref().unwrap_or(""));
            let by_name = wad::extract_container(&mut w, h).is_ok();
            println!(
                "[comp-probe] (d)   pandemic_hash_m2(name)=0x{h:08X} extract_container(name)={}",
                if by_name { "OK" } else { "miss (name is NOT a model hash)" }
            );
            // Load the c3 mesh cell nearest the building's XZ and report its geometry: if the
            // building is baked into the cell, that cell carries substantial vert/tri geometry.
            use mercs2_formats::ucfx::parse_block_entry_table;
            let c3: Vec<(u16, u32)> = wad::block_paths(&w)
                .iter()
                .enumerate()
                .filter_map(|(i, p)| c3_cell_id_from_path(p).map(|cid| (i as u16, cid)))
                .collect();
            let target = [p.pos[0], p.pos[2]];
            let mut best: Option<(f32, u16, u32, f32, f32)> = None;
            for &(blk, cid) in &c3 {
                let (cx, cz) = c3_cell_centre(cid);
                let d2 = (cx - target[0]).powi(2) + (cz - target[1]).powi(2);
                if best.map_or(true, |b| d2 < b.0) {
                    best = Some((d2, blk, cid, cx, cz));
                }
            }
            if let Some((d2, blk, cid, cx, cz)) = best {
                if let Ok(dec) = wad::decompress_block_index(&mut w, blk) {
                    let (count, entries) = parse_block_entry_table(&dec);
                    let mut pos = 4 + count as usize * 16;
                    let mut vt = 0usize;
                    let mut tt = 0usize;
                    let mut has_model = false;
                    for e in &entries {
                        let end = pos + e.chunk_size as usize;
                        if e.type_hash == wad::MODEL_TYPE_HASH && end <= dec.len() {
                            has_model = true;
                            if let Ok((v, i, _d, _s)) = mesh::build_indexed_from_container(&dec[pos..end]) {
                                vt += v.len();
                                tt += i.len() / 3;
                            }
                        }
                        pos = end;
                    }
                    println!(
                        "[comp-probe] (d)   nearest c3 cell {cid} (block {blk}) centre=({cx:.0},{cz:.0}) \
                         dist={:.0}m: has_model={has_model}, {vt} verts / {tt} tris \
                         => exterior buildings ARE baked into c3 cell geometry (not placed via ModelName)",
                        d2.sqrt()
                    );
                }
            }
        }
        None => println!("[comp-probe] (d) no non-PMC *_bld_* placement found in layers_static"),
    }
    // Report how many distinct model-format c3 blocks exist (the baked-geometry cells, format 0x5B724250).
    let model_paths: Vec<(usize, &String)> = wad::block_paths(&w)
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let l = p.to_ascii_lowercase();
            l.contains("\\c3") || l.contains("/c3") || l.starts_with("c3")
        })
        .collect();
    println!(
        "[comp-probe] (d) c3 cell blocks in WAD path table = {} (exterior world geometry is baked per-cell)",
        model_paths.len()
    );

    let _ = MODEL_COMP_HASH; // documented constant (== wad::MODEL_TYPE_HASH); layers_static COMPs key by info-name string.
    Ok(())
}

/// Barycentric point-in-triangle height sample in the XZ plane: returns the interpolated Y at
/// (`x`,`z`) if the point lies inside the triangle (`vx`/`vz` XZ verts, `vy` their heights), else None.
fn tri_height_at(x: f32, z: f32, vx: [f32; 3], vz: [f32; 3], vy: [f32; 3]) -> Option<f32> {
    let d = (vz[1] - vz[2]) * (vx[0] - vx[2]) + (vx[2] - vx[1]) * (vz[0] - vz[2]);
    if d.abs() < 1e-9 {
        return None;
    }
    let a = ((vz[1] - vz[2]) * (x - vx[2]) + (vx[2] - vx[1]) * (z - vz[2])) / d;
    let b = ((vz[2] - vz[0]) * (x - vx[2]) + (vx[0] - vx[2]) * (z - vz[2])) / d;
    let c = 1.0 - a - b;
    if a < -1e-4 || b < -1e-4 || c < -1e-4 {
        return None;
    }
    Some(a * vy[0] + b * vy[1] + c * vy[2])
}

/// Scene path for the terrain: build ONE merged world-space mesh, load it as a
/// single model, spawn ONE static entity (identity transform / palette), and run
/// an elevated bird's-eye camera framing the whole grid.
/// World scene with two cameras: **free-fly** (dev/engine) and **third-person over-the-shoulder**
/// (gameplay), toggled with Tab. Terrain is a static entity; the optional player avatar is placed
/// on it and driven by WASD (camera-relative) with the camera trailing behind + above + shouldered.
/// The animation system idles the avatar (walk clip while moving). Ground height comes from
/// the heightmap. Start in third-person if `start_tps` and a player exists.
///
/// The window + `Scene` open IMMEDIATELY with an animated loading spinner; `load_world_data`
/// runs on a background thread and the loaded world is wired in when its result arrives.
async fn run_scene_world_loading(
    wadpath: String,
    start_tps: bool,
    load_cells: bool,
    load_placements: bool,
    spawn_interior: bool,
    load_props: bool,
    interior_orbit: bool,
) {
    use crate::scene::{AssetStore, ModelAnim, Scene};
    use mercs2_core::glam::{Mat4, Quat, Vec3};
    use mercs2_core::{AnimState, Entity, ModelRef, Schedule, SkinPalette, Time, Transform, World};
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::f32::consts::PI;
    use std::rc::Rc;
    use winit::event::{DeviceEvent, ElementState};
    use winit::window::CursorGrabMode;

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    const CLIP_IDLE: u32 = 0x24F8_C8E6;
    const CLIP_WALK: u32 = 0x5368_2784;
    const CLIP_RUN: u32 = 0x867B_166D;
    // Locomotion feel tunables.
    const ANIM_BLEND_SEC: f32 = 0.25; // crossfade duration on clip switches
    const TURN_RATE: f32 = 12.0; // rad/s exponential yaw damp toward the move direction
    // Human-scale locomotion (world units = metres): the 1.0 s walk cycle strides ~2 m, so
    // ~2 m/s walk keeps feet planted under FOOT_SYNC; sprint ~6.5 m/s. (The earlier 14/60
    // were vehicle speeds — user-confirmed mismatch against the geometry.)
    const WALK_SPEED: f32 = 2.2; // m/s
    const RUN_SPEED: f32 = 6.5; // m/s (Shift)
    const ACCEL: f32 = 12.0; // m/s^2 easing toward a higher target speed
    const DECEL: f32 = 16.0; // m/s^2 easing toward a lower target speed
    const FOOT_SYNC: bool = true; // scale locomotion playback by current/target speed (0.8..1.2)

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Mercenaries 2 — world (Tab: free / third-person)")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );
    // Mouse-look: grab + hide the cursor on the world window (Confined preferred, Locked
    // fallback). Arrow keys stay as a fallback steer; Esc still exits.
    if let Err(e) = window
        .set_cursor_grab(CursorGrabMode::Confined)
        .or_else(|_| window.set_cursor_grab(CursorGrabMode::Locked))
    {
        eprintln!("[world] cursor grab unavailable ({e}); arrow keys still steer");
    }
    window.set_cursor_visible(false);
    let mut scene = Scene::new(window.clone()).await;
    // Placeholder distance fog + sky (stand-in for PgSky/PgSun/PgCloud). Tunables: warm-haze
    // color, density 0.00016 (~30% haze at 2.5 km, ~50% at 4.5 km — depth cue at ground level
    // without white-out from the aerial free cam; 0.00035 washed out the whole map), start 60 m.
    scene.set_fog([0.55, 0.62, 0.70], 0.00016, 60.0);
    // Real loading-screen art: the lti_precache1 plate from shell.wad (sibling of vz.wad),
    // extracted up front (fast) so the loading phase shows it; spinner-only if unavailable.
    match wad::shell_loading_plate(&wadpath) {
        Ok(td) => {
            println!(
                "[load] shell.wad loading plate lti_precache1 (0x7329D083) {}x{} {:?}",
                td.width, td.height, td.format
            );
            scene.set_loading_art(&td);
        }
        Err(e) => eprintln!("[load] shell.wad loading art unavailable ({e}); spinner only"),
    }
    let mut world = World::new();

    // Background loader: all WAD/terrain/player parsing happens off the render thread; the
    // result lands on this channel and is wired into the scene/world on arrival.
    let (tx, rx) = std::sync::mpsc::channel::<Result<WorldData, String>>();
    let progress = Arc::new(LoadProgress::new(LOAD_STAGES));
    let loader_progress = progress.clone();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        let r = load_world_data(&wadpath, load_cells, load_placements, spawn_interior, load_props, &loader_progress);
        if r.is_ok() {
            println!("[load] done in {:.1}s", t0.elapsed().as_secs_f64());
        }
        let _ = tx.send(r);
    });

    // World-dependent state, wired in when the loader finishes (defaults until then).
    let mut hmap: Option<HeightMap> = None;
    let store = Rc::new(RefCell::new(AssetStore::default()));
    // Spawn at the PMC HQ compound (game coords, docs/coordinate_systems.md Example 1); Y is
    // terrain-snapped at spawn. The base GEOMETRY itself arrives with the placements brick — for
    // now this at least drops the player where the PMC is, not the empty map centre.
    // Spawn coords are the game's own boot-log values (MrxUtil._TeleportHero, mrxutil.lua:490),
    // used with the authored Y VERBATIM — no ground-snap at spawn in either mode:
    //   * `--interior`: the authored PMC INTERIOR teleport coord `PMC_INTERIOR_SPAWN`
    //     (3794.0427, 450.7505, -3911.0322) — the off-map, high-Y (above the ~393 terrain cap)
    //     SE-corner interior cell. Height-follow stays OFF (its floor is at ~450, not the terrain).
    //     The interior geometry is now placed at its OWN authored Transforms (no synthetic offset),
    //     so the spawn sits inside the assembled recruit-interior meshes.
    //   * default: the EXTERIOR back-door/pool coord (2560.26, -13.18, -926.25) near the PMC HQ.
    //     Per-frame terrain height-follow kicks in only while walking outdoors (below).
    let mut player_pos = if spawn_interior {
        println!("[world] --interior: spawning at PMC interior teleport coord (3794.043, 450.751, -3911.032) [interior placed at authored transforms; height-follow OFF]");
        Vec3::new(PMC_INTERIOR_SPAWN[0], PMC_INTERIOR_SPAWN[1], PMC_INTERIOR_SPAWN[2])
    } else {
        Vec3::new(2560.2646, -13.1779, -926.2511)
    };
    let mut player_foot = 0.0f32;
    let mut player_entity: Option<Entity> = None;
    let mut player_yaw = PI; // matches the spawn rotation below
    let mut player_speed = 0.0f32; // eased ground speed (m/s)
    let mut player_move_dir = Vec3::new(0.0, 0.0, -1.0); // last input direction (kept while decelerating)
    let mut has_run = false;
    let (mut dur_walk, mut dur_run) = (1.0f32, 1.0f32);

    // Animation system (idles/walks the avatar), same as the ECS scene except clips are selected
    // by `AnimState.clip` and root locomotion is stripped (the entity Transform drives movement).
    let mut time = Time::new(60.0);
    let mut schedule = Schedule::new();
    let assets = store.clone();
    schedule.add_system("animation", move |world: &mut World, time: &Time| {
        let assets = assets.borrow();
        for (_e, (state, palette, mref)) in world
            .query::<(&mut AnimState, &mut SkinPalette, &ModelRef)>()
            .iter()
        {
            if !state.playing {
                continue;
            }
            let Some(ma) = assets.models.get(&mref.model) else { continue };
            let Some(ca) = ma.clips.get(&state.clip).or_else(|| ma.clips.values().next()) else { continue };
            let dur = ca.clip.duration.max(1e-3);
            state.time = (state.time + time.dt * state.speed) % dur;
            // Crossfade: while the previous clip is still fading out, advance it on its own
            // duration and blend its pose toward the current clip's (Havok blendPoses math).
            if state.blend < 1.0 {
                if let Some(cp) = ma.clips.get(&state.prev_clip) {
                    let pdur = cp.clip.duration.max(1e-3);
                    state.prev_time = (state.prev_time + time.dt * state.speed) % pdur;
                    state.blend = (state.blend + time.dt / ANIM_BLEND_SEC).min(1.0);
                    let sa = cp.clip.sample_local(state.prev_time);
                    let sb = ca.clip.sample_local(state.time);
                    palette.mats = pose::havok_palette_blend_in_place(
                        &ma.rig,
                        &sa, &cp.track_to_hier, cp.num_transform_tracks,
                        &sb, &ca.track_to_hier, ca.num_transform_tracks,
                        state.blend,
                    );
                    continue;
                }
                state.blend = 1.0;
            }
            let sample = ca.clip.sample_local(state.time);
            palette.mats = pose::havok_palette_in_place(&ma.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks);
        }
    });

    // Camera state. Free-fly starts elevated over the map centre; third-person orbits the player.
    #[derive(PartialEq)]
    enum CamMode {
        Free,
        ThirdPerson,
    }
    let mut mode = CamMode::Free; // switched to third-person when the loaded player spawns
    let mut free_pos = Vec3::new(0.0, 2500.0, 4500.0);
    let mut free_yaw: f32 = PI;
    let mut free_pitch: f32 = -0.5;
    let mut tp_yaw: f32 = PI;
    let mut tp_pitch: f32 = -0.12;
    let mut held: HashSet<KeyCode> = HashSet::new();
    let mut loading = true;
    let load_start = std::time::Instant::now();
    // Bar fill shown on the loading screen: eased toward the loader's staged fraction each
    // frame so stage completions animate instead of jumping.
    let mut bar_shown = 0.0f32;
    let mut bar_last_t = 0.0f32;
    let mut last = std::time::Instant::now();
    let mut mouse_acc: (f32, f32) = (0.0, 0.0); // cursor-path px accumulated between frames
    let mut mouse_raw_acc: (f32, f32) = (0.0, 0.0); // raw-delta px accumulated between frames
    let mut mouse_dbg_frames: u32 = 0;
    // Mouse source auto-detect. Normal 2026 input = raw deltas (DeviceEvent::MouseMotion).
    // Shadow cloud PCs stream ABSOLUTE 0..65535 coords through raw input, making those "deltas"
    // huge/one-signed garbage — detect that and latch to the CursorMoved+recentre fallback.
    // 0 = undecided (use cursor path), 1 = relative latched (raw), 2 = absolute latched (cursor).
    let mut mouse_src: u8 = 0;
    let mut mouse_sane_events: u32 = 0;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(code), state, .. },
                    ..
                } => match (code, state) {
                    (KeyCode::Escape, _) => elwt.exit(),
                    (KeyCode::Tab, ElementState::Pressed) => {
                        mode = if mode == CamMode::Free { CamMode::ThirdPerson } else { CamMode::Free };
                    }
                    (c, ElementState::Pressed) => {
                        held.insert(c);
                    }
                    (c, ElementState::Released) => {
                        held.remove(&c);
                    }
                },
                WindowEvent::Resized(size) => scene.resize(size),
                // Cursor-position look: delta from window centre, then recentre. Works on
                // absolute-input setups (streamed/cloud) where raw deltas are meaningless.
                WindowEvent::CursorMoved { position, .. } => {
                    let (cx, cy) = (scene.size.width as f64 / 2.0, scene.size.height as f64 / 2.0);
                    mouse_acc.0 += (position.x - cx) as f32;
                    mouse_acc.1 += (position.y - cy) as f32;
                    let _ = scene
                        .window
                        .set_cursor_position(winit::dpi::PhysicalPosition::new(cx, cy));
                }
                WindowEvent::RedrawRequested => {
                    // Loading phase: animate the spinner until the background loader delivers,
                    // then wire the world in (GPU uploads + entity spawns) and start playing.
                    if loading {
                        match rx.try_recv() {
                            Err(std::sync::mpsc::TryRecvError::Empty) => {
                                let t = load_start.elapsed().as_secs_f32();
                                let dt = (t - bar_last_t).max(0.0);
                                bar_last_t = t;
                                // Exponential ease toward the staged target (~6/s rate).
                                bar_shown += (progress.fraction() - bar_shown) * (1.0 - (-6.0 * dt).exp());
                                match scene.render_loading(t, bar_shown) {
                                    Ok(()) => {}
                                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                    Err(e) => eprintln!("surface error: {e:?}"),
                                }
                                return;
                            }
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                eprintln!("[world] loader thread died without a result");
                                elwt.exit();
                                return;
                            }
                            Ok(Err(e)) => {
                                eprintln!("[world] load failed: {e}");
                                elwt.exit();
                                return;
                            }
                            Ok(Ok(data)) => {
                                // Terrain: one static entity at identity (its verts are already world-space).
                                // Skipped in --interior mode: the interior is off-map at Y~450 sitting above
                                // the SE-corner terrain peak (~Y400), which otherwise occludes the whole room.
                                let terrain = data.terrain;
                                if !std::env::args().any(|a| a == "--interior") {
                                    scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
                                    world.spawn((
                                        Transform::IDENTITY,
                                        ModelRef { model: terrain.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // Placement markers (`--placements`): one merged static entity (its
                                // marker verts are already world-space).
                                if let Some(pm) = data.placements {
                                    scene.load_model(pm.hash, &pm.verts, &pm.indices, &pm.draws, &pm.textures, &pm.skin);
                                    world.spawn((
                                        Transform::IDENTITY,
                                        ModelRef { model: pm.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // PMC-subset real geometry (`--placements`): one static entity per
                                // resolved model at its placement Transform (pos + yaw from quat).
                                for (m, pos, yaw) in data.pmc_models {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
                                    t.rotation = Quat::from_rotation_y(yaw);
                                    world.spawn((
                                        t,
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // Hi-res c3 cell geometry (`--cells`): static entities at their grid-cell origins.
                                for (m, off) in data.cells {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    world.spawn((
                                        Transform::from_translation(Vec3::new(off[0], off[1], off[2])),
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY] },
                                    ));
                                }

                                // PMC interior geometry (`--interior`): one static entity per keyed
                                // interior entity at its AUTHORED world Transform (pos + full quat,
                                // native game space, no offset — floor Y≈450). A model may be uploaded
                                // once and referenced by several instances; `load_model` is idempotent
                                // on the hash key so repeats are cheap.
                                for (m, pos, quat) in data.interior {
                                    scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
                                    t.rotation = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                                    // Identity palette sized to the mesh's bone count — a rigged prop's
                                    // verts index several bones; a 1-bone palette collapses the rest to origin.
                                    let nbones = m.skin.bones.len().max(1);
                                    world.spawn((
                                        t,
                                        ModelRef { model: m.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY; nbones] },
                                    ));
                                }

                                // ModelName props (`--props` exterior, `--interior` furniture): each
                                // distinct mesh is uploaded ONCE, then one static entity is spawned per
                                // placement instance (Transform pos + FULL quat, native game space).
                                let mut prop_meshes = 0usize;
                                let mut prop_instances = 0usize;
                                for (hash, m, instances) in data.props.into_iter().chain(data.interior_props) {
                                    scene.load_model(hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
                                    prop_meshes += 1;
                                    let nbones = m.skin.bones.len().max(1);
                                    for (pos, quat) in instances {
                                        let mut t = Transform::from_translation(Vec3::new(pos[0], pos[1], pos[2]));
                                        t.rotation = Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]);
                                        world.spawn((
                                            t,
                                            ModelRef { model: hash },
                                            AnimState::default(),
                                            SkinPalette { mats: vec![IDENTITY; nbones] },
                                        ));
                                        prop_instances += 1;
                                    }
                                }
                                if prop_meshes > 0 {
                                    println!("[world] props spawned: {prop_meshes} distinct meshes, {prop_instances} instances");
                                }

                                // Player avatar (optional): near map centre, feet snapped to the terrain heightmap.
                                if let Some(p) = data.player {
                                    has_run = p.clips.iter().any(|c| c.name_hash == CLIP_RUN);
                                    for c in &p.clips {
                                        if c.name_hash == CLIP_WALK {
                                            dur_walk = c.clip.duration.max(1e-3);
                                        } else if c.name_hash == CLIP_RUN {
                                            dur_run = c.clip.duration.max(1e-3);
                                        }
                                    }
                                    scene.load_model(p.hash, &p.verts, &p.indices, &p.draws, &p.textures, &p.skin);
                                    let rig = p.skin.rig.clone();
                                    let bind = if rig.is_empty() {
                                        vec![IDENTITY]
                                    } else {
                                        let m = pose::model_poses(&rig, &pose::bind_qs(&rig));
                                        pose::skin_palette(&rig, &m)
                                    };
                                    // Feet offset: origin-to-lowest-vertex, so the avatar stands ON the ground sample.
                                    let min_y = p.verts.iter().map(|v| v.pos[1]).fold(f32::INFINITY, f32::min);
                                    player_foot = if min_y.is_finite() { -min_y } else { 0.0 };
                                    println!("[world] player foot offset = {player_foot:.3} (model min Y {min_y:.3})");
                                    // Spawn uses the boot-log authored Y verbatim (no snap) for BOTH
                                    // modes; per-frame height-follow (exterior only) takes over on move.
                                    let playing = !p.clips.is_empty();
                                    store.borrow_mut().models.insert(p.hash, ModelAnim {
                                        rig,
                                        clips: p.clips.into_iter().map(|c| (c.name_hash, c)).collect(),
                                    });
                                    let anim = if playing {
                                        AnimState::playing(CLIP_IDLE)
                                    } else {
                                        AnimState::default()
                                    };
                                    // Spawn facing -Z (away from the third-person camera, which starts on the +Z side) so
                                    // the over-the-shoulder view opens behind the player's back, matching tp_yaw = PI.
                                    let mut t = Transform::from_translation(player_pos);
                                    t.rotation = Quat::from_rotation_y(PI);
                                    player_entity = Some(world.spawn((
                                        t,
                                        ModelRef { model: p.hash },
                                        anim,
                                        SkinPalette { mats: bind },
                                    )));
                                }
                                hmap = Some(data.hmap);
                                if start_tps && player_entity.is_some() {
                                    mode = CamMode::ThirdPerson;
                                }
                                loading = false;
                            }
                        }
                    }
                    let now = std::time::Instant::now();
                    let dt = (now - last).as_secs_f32().min(0.1);
                    last = now;
                    let look = 1.6 * dt;

                    // Drain the frame's mouse input from the active source onto the ACTIVE camera.
                    // Per-frame total is clamped so event storms can't slam the pitch to a rail.
                    const MOUSE_SENS: f32 = 0.0008; // rad per px
                    let src = if mouse_src == 1 { mouse_raw_acc } else { mouse_acc };
                    let mdx = src.0.clamp(-80.0, 80.0) * MOUSE_SENS;
                    let mdy = src.1.clamp(-80.0, 80.0) * MOUSE_SENS;
                    if src != (0.0, 0.0) && mouse_dbg_frames < 20 {
                        eprintln!("[mouse] src={} in=({:+.1},{:+.1}) applied=({:+.4},{:+.4})", mouse_src, src.0, src.1, mdx, mdy);
                        mouse_dbg_frames += 1;
                    }
                    mouse_acc = (0.0, 0.0);
                    mouse_raw_acc = (0.0, 0.0);
                    match mode {
                        CamMode::Free => {
                            free_yaw += mdx;
                            free_pitch = (free_pitch - mdy).clamp(-1.5, 1.5);
                        }
                        CamMode::ThirdPerson => {
                            tp_yaw += mdx;
                            tp_pitch = (tp_pitch - mdy).clamp(-1.2, 0.6);
                        }
                    }

                    let mut view = match mode {
                        CamMode::Free => {
                            if held.contains(&KeyCode::ArrowUp) { free_pitch += look; }
                            if held.contains(&KeyCode::ArrowDown) { free_pitch -= look; }
                            if held.contains(&KeyCode::ArrowLeft) { free_yaw -= look; }
                            if held.contains(&KeyCode::ArrowRight) { free_yaw += look; }
                            free_pitch = free_pitch.clamp(-1.5, 1.5);
                            let fwd = Vec3::new(free_pitch.cos() * free_yaw.sin(), free_pitch.sin(), free_pitch.cos() * free_yaw.cos()).normalize();
                            let right = Vec3::Y.cross(fwd).normalize();
                            let mut mv = Vec3::ZERO;
                            if held.contains(&KeyCode::KeyW) { mv += fwd; }
                            if held.contains(&KeyCode::KeyS) { mv -= fwd; }
                            if held.contains(&KeyCode::KeyD) { mv += right; }
                            if held.contains(&KeyCode::KeyA) { mv -= right; }
                            if held.contains(&KeyCode::KeyE) { mv += Vec3::Y; }
                            if held.contains(&KeyCode::KeyQ) { mv -= Vec3::Y; }
                            let sp = if held.contains(&KeyCode::ShiftLeft) { 3200.0 } else { 800.0 };
                            if mv != Vec3::ZERO { free_pos += mv.normalize() * sp * dt; }
                            Mat4::look_to_lh(free_pos, fwd, Vec3::Y)
                        }
                        CamMode::ThirdPerson => {
                            if held.contains(&KeyCode::ArrowUp) { tp_pitch += look; }
                            if held.contains(&KeyCode::ArrowDown) { tp_pitch -= look; }
                            if held.contains(&KeyCode::ArrowLeft) { tp_yaw -= look; }
                            if held.contains(&KeyCode::ArrowRight) { tp_yaw += look; }
                            tp_pitch = tp_pitch.clamp(-1.2, 0.6);
                            let fwd_flat = Vec3::new(tp_yaw.sin(), 0.0, tp_yaw.cos()).normalize();
                            let right_flat = Vec3::Y.cross(fwd_flat).normalize();
                            let mut mv = Vec3::ZERO;
                            if held.contains(&KeyCode::KeyW) { mv += fwd_flat; }
                            if held.contains(&KeyCode::KeyS) { mv -= fwd_flat; }
                            if held.contains(&KeyCode::KeyD) { mv += right_flat; }
                            if held.contains(&KeyCode::KeyA) { mv -= right_flat; }
                            // Speed ramp: ease the ground speed toward the walk/run target (or 0)
                            // so starts, stops and gait changes aren't instant.
                            let target_sp = if mv != Vec3::ZERO {
                                if held.contains(&KeyCode::ShiftLeft) { RUN_SPEED } else { WALK_SPEED }
                            } else {
                                0.0
                            };
                            let rate = if target_sp > player_speed { ACCEL } else { DECEL };
                            player_speed += (target_sp - player_speed).clamp(-rate * dt, rate * dt);
                            if mv != Vec3::ZERO {
                                player_move_dir = mv.normalize();
                            }
                            let moving = player_speed > 1e-3;
                            if moving {
                                player_pos += player_move_dir * player_speed * dt;
                            }
                            // Ground snap: feet follow the terrain heightmap. Hinted by the
                            // current ground Y so overhangs don't teleport the player up. Skipped
                            // for `--interior` (its floor is at Y≈450, off the terrain).
                            if !spawn_interior {
                                if let Some(hm) = &hmap {
                                    player_pos.y = hm
                                        .height_at_near(player_pos.x, player_pos.z, player_pos.y - player_foot)
                                        + player_foot;
                                }
                            }
                            if let Some(e) = player_entity {
                                if let Ok(mut t) = world.get::<&mut Transform>(e) {
                                    t.translation = player_pos;
                                    if moving {
                                        // Smooth turning: exponential yaw damp toward the move
                                        // direction, shortest arc.
                                        let target = player_move_dir.x.atan2(player_move_dir.z);
                                        let d = (target - player_yaw + PI).rem_euclid(2.0 * PI) - PI;
                                        player_yaw += d * (1.0 - (-TURN_RATE * dt).exp());
                                        t.rotation = Quat::from_rotation_y(player_yaw);
                                    }
                                }
                                // Run under Shift, walk while moving, idle otherwise. A switch
                                // crossfades from the old clip; walk<->run carries the normalized
                                // cycle phase so the feet stay in step (idle restarts at 0).
                                if let Ok(mut a) = world.get::<&mut AnimState>(e) {
                                    let want = if mv != Vec3::ZERO {
                                        if held.contains(&KeyCode::ShiftLeft) && has_run { CLIP_RUN } else { CLIP_WALK }
                                    } else {
                                        CLIP_IDLE
                                    };
                                    if a.clip != want {
                                        a.prev_clip = a.clip;
                                        a.prev_time = a.time;
                                        a.blend = 0.0;
                                        a.time = if a.clip == CLIP_WALK && want == CLIP_RUN {
                                            a.time / dur_walk * dur_run
                                        } else if a.clip == CLIP_RUN && want == CLIP_WALK {
                                            a.time / dur_run * dur_walk
                                        } else {
                                            0.0
                                        };
                                        a.clip = want;
                                    }
                                    // Foot-slide reduction: playback rate tracks the eased speed.
                                    a.speed = if FOOT_SYNC && want != CLIP_IDLE && target_sp > 0.0 {
                                        (player_speed / target_sp).clamp(0.8, 1.2)
                                    } else {
                                        1.0
                                    };
                                }
                            }
                            let dir = Vec3::new(tp_pitch.cos() * tp_yaw.sin(), tp_pitch.sin(), tp_pitch.cos() * tp_yaw.cos()).normalize();
                            let focus = player_pos + Vec3::Y * 2.2;
                            let right = Vec3::Y.cross(dir).normalize();
                            let eye = focus - dir * 6.0 + right * 1.2;
                            Mat4::look_to_lh(eye, (focus - eye).normalize(), Vec3::Y)
                        }
                    };

                    // Interior debug orbit (`--interior-orbit`): override the camera each frame with an
                    // elevated auto-orbit CENTERED on the interior anchor (3794,470,-3911), radius ~120 m,
                    // height ~+70, so the whole assembled room + player are framed from outside. The TPS
                    // sim above still runs (player movement/anim); only the view matrix is replaced.
                    if interior_orbit {
                        const ANCHOR: Vec3 = Vec3::new(3779.8, 454.7, -3879.6);
                        const RADIUS: f32 = 38.0;
                        const HEIGHT: f32 = 52.0;
                        let ang = load_start.elapsed().as_secs_f32() * 0.25; // ~24 s per revolution
                        let eye = ANCHOR + Vec3::new(RADIUS * ang.sin(), HEIGHT, RADIUS * ang.cos());
                        view = Mat4::look_at_lh(eye, ANCHOR, Vec3::Y);
                    }

                    schedule.run_fixed(&mut world, &mut time, dt);
                    scene.set_view(view, if interior_orbit { 1.0 } else { 0.5 }, 30000.0);
                    match scene.render(&world) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => scene.resize(scene.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                        Err(e) => eprintln!("surface error: {e:?}"),
                    }
                }
                _ => {}
            },
            // Raw deltas: the normal game input path. Feeds the accumulator only while sane;
            // a single absurd event (absolute-coordinate stream, e.g. Shadow cloud PC) latches
            // the cursor fallback for the rest of the session.
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                let (dx, dy) = (delta.0 as f32, delta.1 as f32);
                if mouse_src != 2 {
                    if dx.abs() > 2000.0 || dy.abs() > 2000.0 {
                        mouse_src = 2; // absolute-coordinate stream detected -> cursor path
                        eprintln!("[mouse] absolute-coordinate raw input detected -> cursor-recentre mode");
                    } else {
                        mouse_raw_acc.0 += dx;
                        mouse_raw_acc.1 += dy;
                        if mouse_src == 0 && (dx != 0.0 || dy != 0.0) {
                            mouse_sane_events += 1;
                            if mouse_sane_events >= 10 {
                                mouse_src = 1; // healthy relative deltas -> raw path
                            }
                        }
                    }
                }
            }
            Event::AboutToWait => scene.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}

type TexMap = std::collections::HashMap<u32, mercs2_formats::texture::TextureData>;

/// A decoded animation clip bound to a model's HIER, ready to drive `pose::animate_locals`.
struct ClipAnim {
    clip: mercs2_formats::anim::AnimClip,
    /// track index -> HIER bone index (None = track's bone absent from this model).
    track_to_hier: Vec<Option<usize>>,
    /// number of transform tracks (the rest are float tracks, not bone transforms).
    num_transform_tracks: usize,
    name_hash: u32,
}

/// Find the animgroup whose binding best covers this model's HIER, decode a clip, and bind its
/// tracks to HIER bones. `want` selects a specific clip by name-hash; otherwise a normal fully-mapped
/// body clip is chosen (≤70 tracks — the 105-track full-body/reference clip is a special case that
/// over-poses a single body, so it's not the default).
fn load_clip_for_rig(w: &mut wad::Wad, hier: &[u32], want: Option<u32>) -> Option<ClipAnim> {
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best: Option<(u16, u32, usize, u32)> = None; // (block, clip_hash, resolved, tracks)
    for blk in wad::animgroup_blocks(w) {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            if let Some(h) = want {
                if c.name_hash != h {
                    continue;
                }
            }
            let resolved = c.binding.resolve_to_hier(hier).iter().filter(|r| r.is_some()).count();
            if resolved == 0 && want.is_none() {
                continue; // clip drives no bone of this model
            }
            let normal = c.num_transform_tracks <= 70; // exclude the 105-track special clip
            let better = match best {
                None => true,
                Some((_, _, r, _)) if want.is_some() => resolved > r,
                Some((_, _, r, t)) => {
                    let best_normal = t <= 70;
                    if normal != best_normal { normal } else { resolved > r }
                }
            };
            if better {
                best = Some((blk, c.name_hash, resolved, c.num_transform_tracks));
            }
        }
    }
    let (blk, clip_hash, _, _) = best?;
    // Pass 2: decode it.
    let data = wad::decompress_block_index(w, blk).ok()?;
    let ag = parse_animgroup(&data).ok()?;
    let c = ag.clips.iter().find(|c| c.name_hash == clip_hash)?;
    let clip = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok()?;
    if !clip.decoded {
        return None; // e.g. a delta clip (header-only) — leave synthetic driver in place
    }
    let track_to_hier = c.binding.resolve_to_hier(hier);
    Some(ClipAnim {
        clip,
        track_to_hier,
        num_transform_tracks: c.num_transform_tracks as usize,
        name_hash: clip_hash,
    })
}

/// Load SEVERAL clips by name-hash in ONE pass over the animgroup blocks (each block is
/// decompressed + parsed once, vs once per clip via `load_clip_for_rig` — the world load was
/// spending ~2/3 of its 20 s on the repeated scans). Same per-want selection rule as
/// `load_clip_for_rig` with `want = Some(h)`: best = most tracks resolved to this HIER.
fn load_clips_for_rig(w: &mut wad::Wad, hier: &[u32], wants: &[u32]) -> Vec<Option<ClipAnim>> {
    use mercs2_formats::animgroup::parse_animgroup;
    let mut best: Vec<Option<(u16, usize)>> = vec![None; wants.len()]; // (block, resolved)
    for blk in wad::animgroup_blocks(w) {
        let Ok(data) = wad::decompress_block_index(w, blk) else { continue };
        let Ok(ag) = parse_animgroup(&data) else { continue };
        for c in &ag.clips {
            for (i, &h) in wants.iter().enumerate() {
                if c.name_hash != h {
                    continue;
                }
                let resolved = c.binding.resolve_to_hier(hier).iter().filter(|r| r.is_some()).count();
                if best[i].map_or(true, |(_, r)| resolved > r) {
                    best[i] = Some((blk, resolved));
                }
            }
        }
    }
    // Decode pass: only the chosen blocks (cached so a shared block decompresses once).
    let mut cache: std::collections::HashMap<u16, Vec<u8>> = std::collections::HashMap::new();
    wants
        .iter()
        .zip(best)
        .map(|(&h, b)| {
            let (blk, _) = b?;
            if !cache.contains_key(&blk) {
                cache.insert(blk, wad::decompress_block_index(w, blk).ok()?);
            }
            let data = cache.get(&blk)?;
            let ag = parse_animgroup(data).ok()?;
            let c = ag.clips.iter().find(|c| c.name_hash == h)?;
            let clip = mercs2_formats::anim::parse_anim(&data[c.havok_offset..]).ok()?;
            if !clip.decoded {
                return None;
            }
            Some(ClipAnim {
                clip,
                track_to_hier: c.binding.resolve_to_hier(hier),
                num_transform_tracks: c.num_transform_tracks as usize,
                name_hash: h,
            })
        })
        .collect()
}

fn load_from_wad(
    wadpath: &str,
    model: Option<String>,
    index: Option<String>,
    animate: bool,
    clip_hash: Option<u32>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>, TexMap, mesh::SkinData, Option<ClipAnim>, u32, String), String> {
    let mut w = wad::open(wadpath)?;
    let models = wad::model_list(&w);
    if models.is_empty() {
        return Err("no model assets in WAD".into());
    }
    let hash = if let Some(m) = model {
        parse_hash(&m).ok_or_else(|| format!("bad --model hash '{m}'"))?
    } else if let Some(n) = index {
        let n: usize = n.parse().map_err(|_| format!("bad --index '{n}'"))?;
        models
            .get(n)
            .map(|&(h, _)| h)
            .ok_or_else(|| format!("--index {n} out of range (0..{})", models.len()))?
    } else {
        models[0].0
    };
    let container = wad::extract_container(&mut w, hash)?;
    let (verts, indices, draws, s) = mesh::build_indexed_from_container(&container)?;

    // Extract each unique diffuse + normal-map texture (DXT/BC bytes) for the placed groups.
    let mut textures: TexMap = std::collections::HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal].into_iter().flatten() {
            if !textures.contains_key(&h) {
                match wad::extract_texture(&mut w, h) {
                    Ok(t) => {
                        textures.insert(h, t);
                    }
                    Err(e) => eprintln!("  texture 0x{h:08X} unavailable: {e}"),
                }
            }
        }
    }

    let ntris = indices.len() / 3;
    println!(
        "loaded model 0x{hash:08X}: {} verts / {ntris} tris / {} groups / {} textures ({} accessory groups skipped)",
        s.vertices, s.meshes, textures.len(), s.skipped
    );

    // Animation: bind the best-matching clip to this model's HIER (only when requested).
    let clip = if animate && !s.rig.is_empty() {
        let hier: Vec<u32> = s.rig.iter().map(|b| b.name_hash).collect();
        match load_clip_for_rig(&mut w, &hier, clip_hash) {
            Some(ca) => {
                let resolved = ca.track_to_hier.iter().filter(|r| r.is_some()).count();
                println!(
                    "animation: clip 0x{:08X} ({} tracks, {} frames, {:.2}s), {resolved} tracks -> HIER bones",
                    ca.name_hash, ca.clip.num_tracks, ca.clip.num_frames, ca.clip.duration
                );
                Some(ca)
            }
            None => {
                eprintln!("animation: no decodable clip bound to this model — using synthetic driver");
                None
            }
        }
    } else {
        None
    };

    let title = format!("Mercs 2 — model 0x{hash:08X} ({ntris} tris)");
    Ok((verts, indices, draws, textures, s.skin_data(), clip, hash, title))
}

async fn run_render(
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<mesh::DrawGroup>,
    textures: TexMap,
    skin: mesh::SkinData,
    clip: Option<ClipAnim>,
    animate: bool,
    points: bool,
    title: String,
) {
    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );

    let mut r = Renderer::new(
        window.clone(),
        &verts,
        &indices,
        &draws,
        &textures,
        &skin,
        clip,
        animate,
        points,
    )
    .await;

    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == r.window.id() => match event {
                WindowEvent::CloseRequested
                | WindowEvent::KeyboardInput {
                    event:
                        KeyEvent {
                            physical_key: PhysicalKey::Code(KeyCode::Escape),
                            ..
                        },
                    ..
                } => elwt.exit(),
                WindowEvent::Resized(size) => r.resize(size),
                WindowEvent::RedrawRequested => match r.render() {
                    Ok(()) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => r.resize(r.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                    Err(e) => eprintln!("surface error: {e:?}"),
                },
                _ => {}
            },
            Event::AboutToWait => r.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}

/// A model loaded from the WAD, ready to hand to the scene (GPU) + asset store (CPU).
struct LoadedModel {
    hash: u32,
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<mesh::DrawGroup>,
    textures: TexMap,
    skin: mesh::SkinData,
    clips: Vec<ClipAnim>,
}

/// ECS-driven scene path — the `mercs2_core` spine plus a multi-model asset store. Each distinct
/// model is uploaded once (GPU) and its rig+clip stored once (CPU); entities reference models by
/// hash, so two entities can share one model asset yet animate independently. The `animation` system
/// advances every playing entity and samples its model's clip into that entity's `SkinPalette`; the
/// `Scene` walks the `World` and draws each entity with its own transform + palette.
async fn run_scene_ecs(models: Vec<LoadedModel>, title: String) {
    use crate::scene::{AssetStore, ModelAnim, Scene};
    use mercs2_core::glam::Vec3;
    use mercs2_core::{AnimState, ModelRef, Schedule, SkinPalette, Time, Transform, World};
    use std::rc::Rc;

    const IDENTITY: [[f32; 4]; 4] = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    if models.is_empty() {
        eprintln!("no models to render");
        return;
    }

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title(&title)
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0))
            .build(&event_loop)
            .expect("window"),
    );
    let mut scene = Scene::new(window.clone()).await;

    // Load GPU geometry + CPU anim data for each distinct model, keyed by hash.
    let mut store = AssetStore::default();
    let mut prepared: Vec<(u32, Vec<[[f32; 4]; 4]>, f32)> = Vec::new(); // (hash, bind_palette, dur)
    for m in models {
        scene.load_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
        let rig = m.skin.rig.clone();
        let bind_palette = if rig.is_empty() {
            vec![IDENTITY]
        } else {
            let model = pose::model_poses(&rig, &pose::bind_qs(&rig));
            pose::skin_palette(&rig, &model)
        };
        let dur = m.clips.first().map(|c| c.clip.duration).unwrap_or(0.0);
        prepared.push((m.hash, bind_palette, dur));
        store.models.insert(m.hash, ModelAnim { rig, clips: m.clips.into_iter().map(|c| (c.name_hash, c)).collect() });
    }
    let store = Rc::new(store);

    // --- Build the sim spine ---
    let mut world = World::new();
    let mut time = Time::new(60.0); // fixed 60 Hz tick
    let mut schedule = Schedule::new();

    // Animation system: advance each playing entity and sample its model's clip into SkinPalette.
    let assets = store.clone();
    schedule.add_system("animation", move |world: &mut World, time: &Time| {
        for (_e, (state, palette, mref)) in world
            .query::<(&mut AnimState, &mut SkinPalette, &ModelRef)>()
            .iter()
        {
            if !state.playing {
                continue;
            }
            let Some(ma) = assets.models.get(&mref.model) else { continue };
            let Some(ca) = ma.clips.get(&state.clip).or_else(|| ma.clips.values().next()) else { continue };
            let dur = ca.clip.duration.max(1e-3);
            state.time = (state.time + time.dt * state.speed) % dur;
            let sample = ca.clip.sample_local(state.time);
            palette.mats =
                pose::havok_palette(&ma.rig, &sample, &ca.track_to_hier, ca.num_transform_tracks);
        }
    });

    // Spawn two instances of the primary model (offset in X and in animation phase, to prove
    // independent per-entity pose from one shared asset), then one of each additional model behind.
    let (p_hash, p_bind, p_dur) = &prepared[0];
    for (x, phase) in [(-0.6f32, 0.0f32), (0.6f32, p_dur * 0.5)] {
        let anim = if *p_dur > 0.0 {
            AnimState { clip: 0, time: phase, playing: true, ..Default::default() }
        } else {
            AnimState::default()
        };
        world.spawn((
            Transform::from_translation(Vec3::new(x, -0.05, 0.0)),
            ModelRef { model: *p_hash },
            anim,
            SkinPalette { mats: p_bind.clone() },
        ));
    }
    for (i, (hash, bind, dur)) in prepared.iter().enumerate().skip(1) {
        let anim = if *dur > 0.0 {
            AnimState::playing(0)
        } else {
            AnimState::default()
        };
        world.spawn((
            Transform::from_translation(Vec3::new(0.0, -0.05, -0.9 * i as f32)),
            ModelRef { model: *hash },
            anim,
            SkinPalette { mats: bind.clone() },
        ));
    }
    println!(
        "[ecs/scene] {} model(s) in store, {} entities; schedule: [{}]",
        prepared.len(),
        world.len(),
        schedule.system_names().collect::<Vec<_>>().join(", ")
    );

    let mut last = std::time::Instant::now();
    event_loop
        .run(move |event, elwt| match event {
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => {
                match event {
                    WindowEvent::CloseRequested
                    | WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                physical_key: PhysicalKey::Code(KeyCode::Escape),
                                ..
                            },
                        ..
                    } => elwt.exit(),
                    WindowEvent::Resized(size) => scene.resize(size),
                    WindowEvent::RedrawRequested => {
                        let now = std::time::Instant::now();
                        let frame_dt = (now - last).as_secs_f32();
                        last = now;
                        schedule.run_fixed(&mut world, &mut time, frame_dt);
                        match scene.render(&world) {
                            Ok(()) => {}
                            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                scene.resize(scene.size)
                            }
                            Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                            Err(e) => eprintln!("surface error: {e:?}"),
                        }
                    }
                    _ => {}
                }
            }
            Event::AboutToWait => scene.window.request_redraw(),
            _ => {}
        })
        .expect("event loop run");
}
