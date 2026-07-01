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
    tex_binds: Vec<wgpu::BindGroup>,
    /// Per-group draws: (index_start, index_count, index into `tex_binds`).
    draw_calls: Vec<(u32, u32, usize)>,
    depth_view: wgpu::TextureView,
    start: std::time::Instant,
}

impl Renderer {
    async fn new(
        window: Arc<Window>,
        verts: &[Vertex],
        indices: &[u32],
        draws: &[mesh::DrawGroup],
        textures: &TexMap,
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
            bind_group_layouts: &[&camera_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });
        let vbuf_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Float32x4],
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
            tex_binds,
            draw_calls,
            depth_view,
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
        let mvp = proj * view;
        self.queue
            .write_buffer(&self.camera_buf, 0, bytemuck::cast_slice(&mvp.to_cols_array()));

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
        if args.iter().any(|a| a == "--list") {
            if let Err(e) = wad_list(&wadpath) {
                eprintln!("--list failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        match load_from_wad(&wadpath, val("--model"), val("--index")) {
            Ok((verts, indices, draws, textures, title)) => {
                pollster::block_on(run_render(verts, indices, draws, textures, false, title))
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
    pollster::block_on(run_render(verts, Vec::new(), Vec::new(), TexMap::new(), points, title));
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

fn load_from_wad(
    wadpath: &str,
    model: Option<String>,
    index: Option<String>,
) -> Result<(Vec<Vertex>, Vec<u32>, Vec<mesh::DrawGroup>, TexMap, String), String> {
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
    Ok((
        verts,
        indices,
        draws,
        textures,
        format!("Mercs 2 — model 0x{hash:08X} ({ntris} tris)"),
    ))
}

async fn run_render(
    verts: Vec<Vertex>,
    indices: Vec<u32>,
    draws: Vec<mesh::DrawGroup>,
    textures: TexMap,
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

    let mut r = Renderer::new(window.clone(), &verts, &indices, &draws, &textures, points).await;

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
