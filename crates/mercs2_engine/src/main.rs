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
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
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
        let mvp = proj * view * self.fit;
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&mvp.to_cols_array()));

        // Animation: recompute + upload the skinning palette from the current pose. A real clip
        // (looped) drives it if bound; otherwise the synthetic joint-wobble proves the path.
        if let Some(ca) = &self.clip {
            let dur = ca.clip.duration.max(1e-3);
            let sample = ca.clip.sample_local(t % dur);
            let locals = pose::animate_locals(&self.rig, &sample, &ca.track_to_hier);
            let pal = pose::palette(&self.rig, &locals);
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
        .any(|a| matches!(a.as_str(), "--wad" | "--list" | "--model" | "--index"));
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
        let animate = args.iter().any(|a| a == "--animate");
        let clip_hash = val("--clip").and_then(|c| parse_hash(&c));
        match load_from_wad(&wadpath, val("--model"), val("--index"), animate, clip_hash) {
            Ok((verts, indices, draws, textures, skin, clip, title)) => {
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

    // Faithful Havok sampleAndCombine: start every bone at its bind local pose, then overwrite the
    // bones driven by a real TRANSFORM track with the sampled hkQsTransform (full T/R/S). Undriven
    // bones and float-track slots keep bind (no (0,0,0) collapse). Then model-space compose + skin.
    let mut local_qs = pose::bind_qs(&skin.rig);
    for (track, bone) in binding.iter().enumerate() {
        if track >= num_transform_tracks {
            break; // float tracks — not transforms
        }
        if let (Some(&b), Some(qs)) = (bone.as_ref(), pose.get(track)) {
            if b >= local_qs.len() {
                continue;
            }
            let mut rot = qs.rotation;
            let qn = (rot.iter().map(|c| c * c).sum::<f32>()).sqrt();
            if qn > 1e-6 {
                for c in rot.iter_mut() {
                    *c /= qn;
                }
            }
            // Overwrite ROTATION from the sample; keep the bind translation/scale (the sample's
            // (0,0,0) translation = absent DOFs, which Havok fills from the reference/bind pose).
            local_qs[b].rotation = rot;
        }
    }
    println!("  Havok combine: {} transform tracks over {} bind poses", num_transform_tracks, local_qs.len());
    let model = pose::model_poses(&skin.rig, &local_qs);
    skin.bones = pose::skin_palette(&skin.rig, &model);

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

type TexMap = std::collections::HashMap<u32, mercs2_formats::texture::TextureData>;

/// A decoded animation clip bound to a model's HIER, ready to drive `pose::animate_locals`.
struct ClipAnim {
    clip: mercs2_formats::anim::AnimClip,
    /// track index -> HIER bone index (None = track's bone absent from this model).
    track_to_hier: Vec<Option<usize>>,
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
    Some(ClipAnim { clip, track_to_hier, name_hash: clip_hash })
}

fn load_from_wad(
    wadpath: &str,
    model: Option<String>,
    index: Option<String>,
    animate: bool,
    clip_hash: Option<u32>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>, TexMap, mesh::SkinData, Option<ClipAnim>, String), String> {
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
    Ok((verts, indices, draws, textures, s.skin_data(), clip, title))
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
