//! mercs2_engine — Phase-1 skeleton of the native 64-bit Mercenaries 2 reimplementation.
//!
//! See `docs/modernization/00_charter.md`. This is the render shell: a wgpu (DX12/Vulkan/Metal)
//! window with a working pipeline.
//!
//! Usage:
//!   cargo run -p mercs2_engine                     # placeholder triangle
//!   cargo run -p mercs2_engine -- <model.bin>      # render a real model container (point cloud)
//!   cargo run -p mercs2_engine -- --dump <model.bin>  # headless: parse + print stats, no window

// Render-agnostic engine modules now live in the crate's library (`lib.rs`) so the sibling
// mercs2_game / mercs2_probe binaries share them; the bin consumes them via the crate name.
use mercs2_engine::{mesh, pose, wad};

// The streaming-world render + its shared render types/loaders now live in the library
// (`render`, `scene`, `game_world`) so `mercs2_game` drives them in-process. Glob so the bin's
// remaining run modes + render-coupled probes keep their bare call sites (make_*, LoadedModel,
// ClipAnim, TexMap, LoadProgress, terrain_to_vertices, load_* loaders, run_game_world, …).
use mercs2_engine::render::*;
use mercs2_engine::game_world::*;

// Shared world/asset helpers (constants, HeightMap, streaming-catalog builder, reverse-hash utils)
// now live in the engine library so the run modes here and the `mercs2_probe` diagnostics
// (`mercs2_engine::diag`) share one implementation. Glob so bare call sites keep working.
use mercs2_engine::worldutil::*;
use mercs2_engine::mesh::Vertex;
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
        // Diagnostic/export flags moved to the `mercs2_probe` binary (`mercs2_engine::diag`); only the
        // render/run modes + the render-coupled probes that still drag bin-local render types remain.
        .any(|a| matches!(a.as_str(), "--wad" | "--model" | "--index" | "--world" | "--world-probe" | "--interior-probe" | "--interior-list" | "--pmc-shell" | "--destruct" | "--interior-assemble" | "--lod-probe" | "--align-probe" | "--hires-terrain" | "--stream"));
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
        // Headless terrain probe: parse the low_res world terrain and print verifiable counts.
        if args.iter().any(|a| a == "--world-probe") {
            if let Err(e) = world_probe(&wadpath) {
                eprintln!("--world-probe failed: {e}");
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
        // Headless PMC shell resolver: resolve wifpmcinterior.lua's `_tBuildings` shell names (the
        // interior "livedin" HQ buildings) -> their meshes, by NAME. Identifies the hall/floor mesh.
        if args.iter().any(|a| a == "--pmc-shell") {
            if let Err(e) = pmc_shell_probe(&wadpath) {
                eprintln!("--pmc-shell failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless interior assembly: run load_pmc_interior and print its floor/furniture Y diagnostics
        // (no game window needed) to pin the shell-floor-vs-furniture height mismatch.
        if args.iter().any(|a| a == "--interior-assemble") {
            match wad::open(&wadpath) {
                Ok(mut w) => {
                    let _ = load_pmc_interior(&mut w);
                }
                Err(e) => {
                    eprintln!("--interior-assemble failed: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        // Headless destruction probe: is a model destructible (has SWIT), and how do its mesh groups
        // split into intact / break_piece / static? `--destruct <hexhash>`.
        if args.iter().any(|a| a == "--destruct") {
            let h = val("--destruct")
                .and_then(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                .unwrap_or(0x3E629E14);
            if let Err(e) = destruction_probe(&wadpath, h) {
                eprintln!("--destruct failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless hi-res terrain assembly check: load all 400 terrainmeshes (TerrainObject->Transform
        // placement + POFF sub-tiles) and report the assembled world bounds / counts vs the low-res.
        if args.iter().any(|a| a == "--hires-terrain") {
            if let Err(e) = hires_terrain_probe(&wadpath) {
                eprintln!("--hires-terrain failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless alignment probe: measure how far object placements float above/below the terrain,
        // and test coordinate transforms (transpose/flip) of the sampling XZ to reveal a mapping bug.
        if args.iter().any(|a| a == "--align-probe") {
            if let Err(e) = align_probe(&wadpath) {
                eprintln!("--align-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Headless LOD RCA: (a) per-prop SEGM state_mask distribution (does a prop mesh carry
        // multi-tier LOD sub-objects?), (b) fine-cell leaf-block tier + extent structure.
        if args.iter().any(|a| a == "--lod-probe") {
            if let Err(e) = lod_probe(&wadpath) {
                eprintln!("--lod-probe failed: {e}");
                std::process::exit(1);
            }
            return;
        }
        // Control-driven streaming world with a free-fly camera (the default boot; also reachable
        // explicitly via --stream). Loads/unloads blocks + wakes/hibernates props by proximity.
        if args.iter().any(|a| a == "--stream") {
            pollster::block_on(run_game_world(wadpath.clone(), spawn_arg(&args), overlays_arg(&args)));
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

    // Default boot = the streaming world (user mandate). With NO positional file arg and no
    // explicit `--triangle`, if a vz.wad is discoverable, boot the control-driven streaming world
    // with a free-fly camera instead of the placeholder triangle. `--triangle`, an explicit file
    // model path, or no discoverable wad all fall through to the file-model / triangle path below.
    let has_positional = args.iter().skip(1).any(|a| !a.starts_with("--"));
    let force_triangle = args.iter().any(|a| a == "--triangle");
    if !has_positional && !force_triangle {
        if let Some(wadpath) = wad::registry_vz_wad() {
            eprintln!("vz.wad: {wadpath}");
            eprintln!("[boot] no args -> streaming world (free-fly camera). Use --triangle for the skeleton.");
            pollster::block_on(run_game_world(wadpath, spawn_arg(&args), overlays_arg(&args)));
            return;
        }
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





/// Headless hi-res terrain assembly check: place all 400 terrainmeshes via `TerrainObject`->Transform
/// (POFF applied) and report the assembled world bounds / counts against the low-res terrain.
fn hires_terrain_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let tiles = mercs2_formats::placement::load_terrain_tiles(&ls);
    println!("[hires] TerrainObject tiles: {}", tiles.len());

    // Debug the first tile's resolution path.
    if let Some(t) = tiles.first() {
        match wad::extract_container_typed(&mut w, t.terrainmesh_hash, TERRAINMESH_TYPE_HASH) {
            Ok(c) => match mesh::build_indexed_from_container(&c) {
                Ok((v, _, _, _)) => println!("[hires] tile0 0x{:08X}: extract OK ({} B), build OK ({} verts)", t.terrainmesh_hash, c.len(), v.len()),
                Err(e) => println!("[hires] tile0 0x{:08X}: extract OK ({} B), BUILD FAILED: {e}", t.terrainmesh_hash, c.len()),
            },
            Err(e) => println!("[hires] tile0 0x{:08X}: EXTRACT FAILED: {e}", t.terrainmesh_hash),
        }
    }

    let (mut min, mut max) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    let (mut nverts, mut ntris, mut loaded, mut missing) = (0usize, 0usize, 0usize, 0usize);
    let (mut n_tex, mut draws_diff, mut draws_tot) = (0usize, 0usize, 0usize);
    for t in &tiles {
        match load_terrainmesh_tile(&mut w, t.terrainmesh_hash, t.pos) {
            Some(m) => {
                loaded += 1;
                n_tex += m.textures.len();
                draws_diff += m.draws.iter().filter(|d| d.diffuse.is_some()).count();
                draws_tot += m.draws.len();
                nverts += m.verts.len();
                ntris += m.indices.len() / 3;
                for v in &m.verts {
                    for k in 0..3 {
                        min[k] = min[k].min(v.pos[k]);
                        max[k] = max[k].max(v.pos[k]);
                    }
                }
            }
            None => missing += 1,
        }
    }
    println!("[hires] placed {loaded}/{} tiles ({missing} missing) | {nverts} verts / {ntris} tris", tiles.len());
    println!("[hires] SPLAT textures: {n_tex} total loaded; {draws_diff}/{draws_tot} draws have a diffuse layer bound");
    println!("[hires] assembled world bounds: X[{:.0},{:.0}] Y[{:.0},{:.0}] Z[{:.0},{:.0}]", min[0], max[0], min[1], max[1], min[2], max[2]);
    println!("[hires] (low-res terrain spans X/Z[-4000,4000] Y[-168,436]; hi-res should match)");
    Ok(())
}






/// Alignment probe: are placements and terrain in the SAME frame? Objects rest ON the ground in-game,
/// so the authored placement Y should ≈ terrain height at the placement XZ. Measures |Y - terrain_h|
/// under the IDENTITY sampling and under transpose/flip transforms of the sampling XZ — if a
/// transformed sampling fits far better than identity, that transform IS the terrain↔world mapping
/// bug (transpose = row/col swap, flip = axis-sign error). No clamping; pure diagnosis.
fn align_probe(wadpath: &str) -> Result<(), String> {
    let mut w = wad::open(wadpath)?;
    let (low, ls) = find_terrain_blocks(&mut w)?;
    let tm = mercs2_formats::terrain::load_terrain(&low, &ls)?;
    let hmap = HeightMap::build(&tm);
    let placements = mercs2_formats::placement::load_placements(&ls)?;

    // Report the raw extents so an origin/scale mismatch shows up immediately.
    let (mut pmin, mut pmax) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &placements {
        for k in 0..3 {
            pmin[k] = pmin[k].min(p.pos[k]);
            pmax[k] = pmax[k].max(p.pos[k]);
        }
    }
    let (mut tmin, mut tmax) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for p in &tm.positions {
        for k in 0..3 {
            tmin[k] = tmin[k].min(p[k]);
            tmax[k] = tmax[k].max(p[k]);
        }
    }
    println!("[align] placements: {} | X[{:.0},{:.0}] Y[{:.0},{:.0}] Z[{:.0},{:.0}]", placements.len(), pmin[0], pmax[0], pmin[1], pmax[1], pmin[2], pmax[2]);
    println!("[align] terrain   :        X[{:.0},{:.0}] Y[{:.0},{:.0}] Z[{:.0},{:.0}]", tmin[0], tmax[0], tmin[1], tmax[1], tmin[2], tmax[2]);

    // Candidate samplings of (x,z): the 8 axis-aligned symmetries (4 rotations × mirror).
    let transforms: &[(&str, fn(f32, f32) -> (f32, f32))] = &[
        ("identity   ( x, z)", |x, z| (x, z)),
        ("flipX      (-x, z)", |x, z| (-x, z)),
        ("flipZ      ( x,-z)", |x, z| (x, -z)),
        ("flipXZ     (-x,-z)", |x, z| (-x, -z)),
        ("transpose  ( z, x)", |x, z| (z, x)),
        ("transp flipX(-z, x)", |x, z| (-z, x)),
        ("transp flipZ( z,-x)", |x, z| (z, -x)),
        ("transp flipXZ(-z,-x)", |x, z| (-z, -x)),
    ];

    for (name, f) in transforms {
        let mut deltas: Vec<f32> = Vec::new();
        for p in &placements {
            let (sx, sz) = f(p.pos[0], p.pos[2]);
            if sx < -3900.0 || sx > 3800.0 || sz < -3900.0 || sz > 3800.0 {
                continue; // sample outside the terrain footprint
            }
            let h = hmap.height_at(sx, sz);
            if !h.is_finite() {
                continue;
            }
            deltas.push((p.pos[1] - h).abs());
        }
        if deltas.is_empty() {
            println!("[align]   {name}: no in-bounds samples");
            continue;
        }
        deltas.sort_by(|a, b| a.total_cmp(b));
        let n = deltas.len();
        let med = deltas[n / 2];
        let within10 = deltas.iter().filter(|&&d| d < 10.0).count();
        let within30 = deltas.iter().filter(|&&d| d < 30.0).count();
        println!(
            "[align]   {name}: n={n:<6} median|dY|={med:7.1}m  within10m={:5.1}%  within30m={:5.1}%",
            100.0 * within10 as f32 / n as f32,
            100.0 * within30 as f32 / n as f32
        );
    }
    println!("[align] NOTE: objects rest on the ground, so the BEST-fitting sampling reveals the true terrain↔world mapping. If identity is not clearly best, the terrain assembly frame is wrong.");

    // --- c3 CELL placement check: the streamed buildings/hi-res-terrain. Load a sample of c3
    // geometry cells the SAME way the streamer does (load_one_c3_cell) and measure whether the
    // placed geometry sits on the terrain, and how the world-space-vs-cell-local heuristic decided.
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    let c3_geom: Vec<u16> = idx
        .by_class(mercs2_formats::world_index::BlockClass::C3Cell)
        .filter(|b| b.has_model_geometry)
        .map(|b| b.block_index)
        .collect();
    let step = (c3_geom.len() / 200).max(1);
    let mut n = 0usize;
    let mut offset_applied = 0usize;
    let mut floats: Vec<f32> = Vec::new();
    for &bi in c3_geom.iter().step_by(step) {
        // Raw (pre-offset) bounds for the first few, to see local-vs-world per axis.
        let raw = load_one_c3_cell(&mut w, bi);
        let Some((m, off)) = raw else { continue };
        n += 1;
        if off != [0.0, 0.0, 0.0] {
            offset_applied += 1;
        }
        // Placed geometry: min corner + centre XZ.
        let (mut rmn, mut rmx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in &m.verts {
            for k in 0..3 {
                rmn[k] = rmn[k].min(v.pos[k]);
                rmx[k] = rmx[k].max(v.pos[k]);
            }
        }
        if n <= 6 {
            let path = wad::block_paths(&w).get(bi as usize).cloned().unwrap_or_default();
            let cxp = (rmn[0] + rmx[0]) * 0.5 + off[0];
            let czp = (rmn[2] + rmx[2]) * 0.5 + off[2];
            let th = hmap.height_at(cxp, czp);
            println!(
                "[align]   c3 {path}: RAW x[{:.1},{:.1}] y[{:.1},{:.1}] z[{:.1},{:.1}] | off=({:.1},{:.1}) placedXZ=({:.0},{:.0}) terrainY={th:.1}",
                rmn[0], rmx[0], rmn[1], rmx[1], rmn[2], rmx[2], off[0], off[2], cxp, czp
            );
            // Does the block ENCODE the cell's world position (an authored transform)? Scan the
            // decompressed bytes for an f32 within 8 m of the expected world X (= off[0]) and print
            // the surrounding float triple — an authored translation would sit here with the real Y.
            if let Ok(dec) = wad::decompress_block_index(&mut w, bi) {
                let want = off[0];
                let mut hits = 0;
                let mut i = 0usize;
                while i + 4 <= dec.len() && hits < 4 {
                    let v = f32::from_le_bytes([dec[i], dec[i + 1], dec[i + 2], dec[i + 3]]);
                    if v.is_finite() && (v - want).abs() < 8.0 && off[0].abs() > 100.0 {
                        let g = |o: usize| f32::from_le_bytes([dec[o], dec[o + 1], dec[o + 2], dec[o + 3]]);
                        let a = if i >= 4 { g(i - 4) } else { 0.0 };
                        let b = if i + 8 <= dec.len() { g(i + 4) } else { 0.0 };
                        println!("[align]     world-X-like f32 @{i}: [.. {a:.1} | {v:.1} | {b:.1} ..]");
                        hits += 1;
                    }
                    i += 4;
                }
                if hits == 0 && off[0].abs() > 100.0 {
                    println!("[align]     (no f32 within 8m of world-X {want:.0} found in block — no authored translation present)");
                }
            }
        }
        let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for v in &m.verts {
            for k in 0..3 {
                mn[k] = mn[k].min(v.pos[k] + off[k]);
                mx[k] = mx[k].max(v.pos[k] + off[k]);
            }
        }
        let (cxp, czp) = ((mn[0] + mx[0]) * 0.5, (mn[2] + mx[2]) * 0.5);
        if cxp < -3900.0 || cxp > 3800.0 || czp < -3900.0 || czp > 3800.0 {
            continue;
        }
        let h = hmap.height_at(cxp, czp);
        if h.is_finite() {
            // base of the cell geometry vs terrain height beneath it
            floats.push((mn[1] - h).abs());
        }
    }
    floats.sort_by(|a, b| a.total_cmp(b));
    if !floats.is_empty() {
        let med = floats[floats.len() / 2];
        let within10 = floats.iter().filter(|&&d| d < 10.0).count();
        println!(
            "[align] c3 cells sampled={n} | cell-local offset applied to {offset_applied} | base-vs-terrain median|dY|={med:.1}m within10m={:.1}%",
            100.0 * within10 as f32 / floats.len() as f32
        );
    } else {
        println!("[align] c3 cells sampled={n} | offset applied to {offset_applied} | (no in-bounds height samples)");
    }
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








// ---------------------------------------------------------------------------
//   World placements (layers_static block 29): markers + interior hunt
// ---------------------------------------------------------------------------





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
        group_index: 0,
    }];
    (verts, indices, draws)
}




/// One streamable prop's spawn recipe: the mesh it renders as + its authored world Transform
/// (pos + full quat, native game space, no flip), joined from the `ModelName`/`Transform` COMPs.


/// Headless LOD reverse-engineering probe. Answers two build-blocking questions with real data:
///  (a) PER-PROP LOD: do the 464 `ModelName` prop meshes carry multi-tier LOD sub-objects (distinct
///      `SEGM.state_mask` values within one container), or is LOD a building/vehicle-only feature?
///      The renderer currently hardcodes `LOD_BIT=0x01` (keeps tier-0 sub-objects, skips the rest).
///  (b) FINE-CELL QUADTREE: for a multi-tier c3 cell, are the fine leaf blocks spatially DISJOINT
///      (a real quadtree we can stream per-subregion by distance) or overlapping?
fn lod_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::model_cubeize::parse_segm;
    use mercs2_formats::placement::load_model_placements;
    use mercs2_formats::world_index::BlockClass;
    let mut w = wad::open(wadpath)?;

    // ---- (a) per-prop SEGM state_mask distribution --------------------------------------------
    let idx = {
        let (archive, file) = wad::archive_and_file(&mut w);
        mercs2_formats::world_index::WorldIndex::build(archive, file)
    };
    let (_low, ls) = find_terrain_blocks(&mut w)?;
    let mut prop_hashes: Vec<u32> = load_model_placements(&ls).iter().map(|p| p.model_hash).collect();
    prop_hashes.sort_unstable();
    prop_hashes.dedup();

    let mut mask_hist: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
    let mut models_resolved = 0usize;
    let mut models_multi_mask = 0usize; // >1 distinct non-trivial state_mask among sub-objects
    let mut models_with_higher_tier = 0usize; // any sub-object needing a bit other than 0x01
    let mut examples: Vec<(u32, Vec<u8>)> = Vec::new();
    for &h in &prop_hashes {
        let Ok(container) = wad::extract_container(&mut w, h) else { continue };
        let segs = parse_segm(&container);
        if segs.is_empty() {
            continue;
        }
        models_resolved += 1;
        let mut distinct: std::collections::BTreeSet<u8> = std::collections::BTreeSet::new();
        for s in &segs {
            *mask_hist.entry(s.state_mask).or_insert(0) += 1;
            distinct.insert(s.state_mask);
        }
        // "Multi-tier" = sub-objects disagree on their mask (some tier-restricted).
        let nontrivial: std::collections::BTreeSet<u8> =
            distinct.iter().copied().filter(|&m| m != 0 && m != 0x0F).collect();
        if distinct.len() > 1 {
            models_multi_mask += 1;
            if examples.len() < 8 {
                examples.push((h, segs.iter().map(|s| s.state_mask).collect()));
            }
        }
        if segs.iter().any(|s| s.state_mask != 0 && s.state_mask != 0x0F && (s.state_mask & 0x01) == 0) {
            models_with_higher_tier += 1;
        }
        let _ = nontrivial;
    }
    println!("[lod-probe] (a) PER-PROP LOD — distinct prop meshes: {}, resolved w/ SEGM: {models_resolved}", prop_hashes.len());
    println!("[lod-probe]   sub-object state_mask histogram (mask -> sub-object count):");
    for (mask, count) in &mask_hist {
        let note = match mask {
            0 => " (unmasked = always drawn)",
            0x0F => " (all tiers)",
            0x01 => " (tier-0 / finest only — what the renderer keeps today)",
            _ => "",
        };
        println!("[lod-probe]     0x{mask:02x} x{count}{note}");
    }
    println!("[lod-probe]   models with >1 distinct sub-object mask (intra-model LOD): {models_multi_mask}");
    println!("[lod-probe]   models with a sub-object NOT visible at tier-0 (would need a swap): {models_with_higher_tier}");
    for (h, masks) in &examples {
        println!("[lod-probe]     e.g. 0x{h:08X}: masks {masks:02x?}");
    }

    // ---- (b) fine-cell quadtree structure ------------------------------------------------------
    // Pick the multi-tier cell with the most fine (tier<3) geometry blocks and dump their extents.
    let mut per_cell: std::collections::HashMap<u32, Vec<(u16, u8, [f32; 6])>> =
        std::collections::HashMap::new();
    for b in idx.by_class(BlockClass::C3Cell) {
        if !b.has_model_geometry {
            continue;
        }
        let (Some(cid), Some(a)) = (b.lod.base_cell_id, b.extent) else { continue };
        per_cell.entry(cid).or_default().push((
            b.block_index,
            b.lod.tier.unwrap_or(3),
            [a.min[0], a.min[1], a.min[2], a.max[0], a.max[1], a.max[2]],
        ));
    }
    let sample = per_cell
        .iter()
        .filter(|(_, v)| v.iter().any(|(_, t, _)| *t >= 3) && v.iter().filter(|(_, t, _)| *t < 3).count() >= 2)
        .max_by_key(|(_, v)| v.iter().filter(|(_, t, _)| *t < 3).count());
    match sample {
        Some((cid, blocks)) => {
            let fine = blocks.iter().filter(|(_, t, _)| *t < 3).count();
            println!("[lod-probe] (b) FINE QUADTREE — sample cell {cid}: {} blocks ({fine} fine tier<3, {} coarse tier3)", blocks.len(), blocks.len() - fine);
            println!("[lod-probe]   index extent is formula-derived (all == cell AABB); loading REAL geometry bounds:");
            let mut sorted = blocks.clone();
            sorted.sort_by_key(|(_, t, _)| std::cmp::Reverse(*t));
            // Load each block's TRUE world-space vertex bounds to see if fine blocks subdivide the
            // cell (distinct sub-region AABBs = a real quadtree) or are redundant full-cell detail.
            let mut distinct_real: std::collections::HashSet<(i32, i32, i32, i32)> =
                std::collections::HashSet::new();
            for (blk, tier, _) in sorted.iter().take(16) {
                match load_one_c3_cell(&mut w, *blk) {
                    Some((m, off)) => {
                        let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                        for v in &m.verts {
                            let p = [v.pos[0] + off[0], v.pos[1] + off[1], v.pos[2] + off[2]];
                            for k in 0..3 {
                                mn[k] = mn[k].min(p[k]);
                                mx[k] = mx[k].max(p[k]);
                            }
                        }
                        distinct_real.insert((mn[0] as i32, mn[2] as i32, mx[0] as i32, mx[2] as i32));
                        println!(
                            "[lod-probe]     blk {blk:<5} tier c{tier}  REAL x[{:>8.1},{:>8.1}] z[{:>8.1},{:>8.1}]  ({:.0}x{:.0} m, {} v)",
                            mn[0], mx[0], mn[2], mx[2], mx[0] - mn[0], mx[2] - mn[2], m.verts.len()
                        );
                    }
                    None => println!("[lod-probe]     blk {blk:<5} tier c{tier}  (no geometry loaded)"),
                }
            }
            println!(
                "[lod-probe]   -> {} DISTINCT real XZ footprints among the {} sampled blocks {}",
                distinct_real.len(),
                sorted.len().min(16),
                if distinct_real.len() > 1 { "= fine blocks SUBDIVIDE (real quadtree)" } else { "= redundant full-cell detail (NO subdivision)" }
            );
        }
        None => println!("[lod-probe] (b) FINE QUADTREE — no multi-tier cell with >=2 fine blocks found"),
    }

    // ---- (c) tier <-> real object SIZE across a sample: is `tier` a LOD level (a cell's tiers
    // share one footprint at different detail) or a spatial-index depth (each block a distinct
    // object, small objects bucketed deeper)? Load real AABBs for up to N blocks per tier. --------
    let mut by_tier: [Vec<u16>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for b in idx.by_class(BlockClass::C3Cell) {
        if b.has_model_geometry {
            let t = b.lod.tier.unwrap_or(3).min(3) as usize;
            by_tier[t].push(b.block_index);
        }
    }
    println!("[lod-probe] (c) TIER<->SIZE — median real XZ extent per tier (sampled):");
    for t in 0..4 {
        let sample: Vec<u16> = by_tier[t].iter().step_by((by_tier[t].len() / 60).max(1)).copied().take(60).collect();
        let mut sizes: Vec<f32> = Vec::new();
        let mut vsum = 0usize;
        for &blk in &sample {
            if let Some((m, _)) = load_one_c3_cell(&mut w, blk) {
                let (mut mn, mut mx) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
                for v in &m.verts {
                    for k in 0..3 {
                        mn[k] = mn[k].min(v.pos[k]);
                        mx[k] = mx[k].max(v.pos[k]);
                    }
                }
                sizes.push((mx[0] - mn[0]).max(mx[2] - mn[2]));
                vsum += m.verts.len();
            }
        }
        sizes.sort_by(|a, b| a.total_cmp(b));
        let med = sizes.get(sizes.len() / 2).copied().unwrap_or(0.0);
        let maxs = sizes.last().copied().unwrap_or(0.0);
        println!(
            "[lod-probe]   tier c{t}: {} blocks total, sampled {} -> median max-XZ-extent {med:.1} m, largest {maxs:.1} m, avg {} v",
            by_tier[t].len(), sizes.len(), if sizes.is_empty() { 0 } else { vsum / sizes.len() }
        );
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
    // Terrain normals: prove they're real (varied), not the old flat up-normal.
    let n_nonup = tm.normals.iter().filter(|n| (n[1] - 1.0).abs() > 0.02).count();
    let ny_min = tm.normals.iter().map(|n| n[1]).fold(f32::INFINITY, f32::min);
    println!(
        "[world-probe] terrain normals        = {} verts, {n_nonup} non-flat ({:.0}%), min normal.y {ny_min:.3}",
        tm.normals.len(),
        100.0 * n_nonup as f32 / tm.normals.len().max(1) as f32
    );
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
            group_index: 0,
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
/// Resolve the PMC interior shell buildings named in `wifpmcinterior.lua` `_tBuildings` to their
/// meshes — identifying the enclosing hall/floor mesh by NAME (no geometric guessing). For each,
/// report the mesh's bbox/size and whether it contains the player hardpoint-local offset.
/// Is `hash` a destructible model (has `SWIT`), and how do its mesh groups split into
/// intact / break_piece / static? Confirms whether a "ruined"-looking building is really the intact
/// body co-rendered WITH its break pieces (fixable by hiding break pieces) vs. an inherently ruined mesh.
fn destruction_probe(wadpath: &str, hash: u32) -> Result<(), String> {
    use mercs2_formats::orchestrator::{self, DestructionState};
    let mut w = wad::open(wadpath)?;
    let container = wad::extract_container(&mut w, hash).map_err(|e| format!("{e:?}"))?;
    let hier = orchestrator::parse_hier(&container);
    let swit = orchestrator::parse_swit(&container);
    let indx = orchestrator::parse_indx(&container);
    println!(
        "[destruct] 0x{hash:08X}: HIER {} nodes, SWIT {} entries, INDX {} mesh->node",
        hier.len(),
        swit.len(),
        indx.len()
    );
    match orchestrator::classify(&container) {
        None => println!(
            "[destruct] NOT destructible (no SWIT/HIER) — single-state mesh; a 'ruined' look is \
             inherent to this mesh, not co-rendered break pieces."
        ),
        Some(d) => {
            let (mut s, mut i, mut b) = (0usize, 0, 0);
            for n in &d.nodes {
                match n.state {
                    DestructionState::Static => s += 1,
                    DestructionState::Intact => i += 1,
                    DestructionState::BreakPiece => b += 1,
                }
            }
            println!(
                "[destruct] DESTRUCTIBLE: {} switch group(s); NODES static={s} intact={i} break_piece={b}",
                d.switch_group_count
            );
            // Per mesh-group (INDX order) state tally — this is what a render filter would key on.
            let mut tally: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for mg in 0..d.indx.len() {
                let st = d.state_of_mesh(mg).map(|x| x.as_str()).unwrap_or("?(no-indx)");
                *tally.entry(st).or_insert(0) += 1;
            }
            println!("[destruct] per-mesh-group states (INDX order): {tally:?}");
            for w in &d.warnings {
                println!("[destruct]   warning: {w}");
            }
            // Raw SEGM tiers: build_indexed renders ONLY state_mask&0x01 (or mask==0). If the intact
            // room is on a non-0x01 tier it gets dropped and we render a damaged tier -> ruined look.
            if let Ok(meshes) = mercs2_formats::model_cubeize::read_model_meshes(&container) {
                println!("[destruct] {} raw SEGM mesh group(s):", meshes.len());
                for m in &meshes {
                    let mut mn = [f32::MAX; 3];
                    let mut mx = [f32::MIN; 3];
                    for p in &m.positions {
                        for c in 0..3 {
                            mn[c] = mn[c].min(p[c]);
                            mx[c] = mx[c].max(p[c]);
                        }
                    }
                    let kept = m.state_mask == 0 || (m.state_mask & 0x01) != 0;
                    println!(
                        "[destruct]   grp {} mask=0x{:02X} {} rigid={} bone={}: {} verts {} tris bbox ({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1})",
                        m.group_index, m.state_mask, if kept { "KEEP" } else { "SKIP" }, m.rigid, m.bone,
                        m.positions.len(), m.tris.len(), mn[0], mn[1], mn[2], mx[0], mx[1], mx[2]
                    );
                    // Export this tier as its own OBJ into the three.js viewer (batch_hqstates pack)
                    // so intact-vs-rubble can be judged visually.
                    let outdir = format!(
                        "{}/../../../../output/review/batch_hqstates/{:08x}_grp{}_mask{:02x}",
                        env!("CARGO_MANIFEST_DIR"), hash, m.group_index, m.state_mask
                    );
                    if std::fs::create_dir_all(&outdir).is_ok() {
                        let mut obj = format!("# 0x{hash:08X} grp {} mask 0x{:02X} — LOCAL geometry\n", m.group_index, m.state_mask);
                        for p in &m.positions {
                            obj.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
                        }
                        for t in &m.tris {
                            obj.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
                        }
                        let _ = std::fs::write(format!("{outdir}/mesh.obj"), obj);
                    }
                }
                println!("[destruct] exported per-tier OBJs to output/review/batch_hqstates/ (view pack 'hqstates')");
            }
            // Correlate real draw geometry to state so intact-vs-rubble is distinguishable by shape.
            if let Ok((verts, indices, draws, _)) = mesh::build_indexed_from_container(&container) {
                println!("[destruct] {} draw group(s) (group_index -> INDX node -> state, geometry):", draws.len());
                for dg in &draws {
                    let node = d.indx.get(dg.group_index).copied();
                    let st = d.state_of_mesh(dg.group_index).map(|x| x.as_str()).unwrap_or("?");
                    let mut mn = [f32::MAX; 3];
                    let mut mx = [f32::MIN; 3];
                    for i in dg.index_start..dg.index_start + dg.index_count {
                        let p = verts[indices[i as usize] as usize].pos;
                        for c in 0..3 {
                            mn[c] = mn[c].min(p[c]);
                            mx[c] = mx[c].max(p[c]);
                        }
                    }
                    println!(
                        "[destruct]   grp {} node={node:?} state={st}: {} tris, bbox ({:.1},{:.1},{:.1})..({:.1},{:.1},{:.1}) dims({:.1},{:.1},{:.1})",
                        dg.group_index, dg.index_count / 3,
                        mn[0], mn[1], mn[2], mx[0], mx[1], mx[2], mx[0] - mn[0], mx[1] - mn[1], mx[2] - mn[2]
                    );
                }
            }
        }
    }
    Ok(())
}

fn pmc_shell_probe(wadpath: &str) -> Result<(), String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    let mut w = wad::open(wadpath)?;
    const HP_LOCAL: [f32; 3] = [
        PMC_INTERIOR_SPAWN[0] - 3750.0,
        PMC_INTERIOR_SPAWN[1] - 450.0,
        PMC_INTERIOR_SPAWN[2] - (-3840.0),
    ];
    println!(
        "[pmc-shell] player hardpoint-local (spawn - actor@(3750,450,-3840)) = ({:.1},{:.1},{:.1})",
        HP_LOCAL[0], HP_LOCAL[1], HP_LOCAL[2]
    );
    // wifpmcinterior.lua _tBuildings = the interior "livedin" HQ buildings; plus exterior counterparts
    // and generic fallbacks for comparison.
    const NAMES: &[&str] = &[
        "pmcoutpost_bld_hq_livedin",
        "pmcoutpost_bld_hqgarage_livedin",
        "pmcoutpost_bld_hqsuites",
        "pmcoutpost_bld_hq",
        "pmcoutpost_bld_hqgarage",
        "pmcoutpost_interior",
        "pmcoutpost_interior_job",
    ];
    for n in NAMES {
        let hash = pandemic_hash_m2(n);
        print!("[pmc-shell] '{n}' -> 0x{hash:08X} : ");
        match load_model_by_hash(&mut w, hash) {
            Some((m, bmin, bmax)) => {
                let (dx, dy, dz) = (bmax[0] - bmin[0], bmax[1] - bmin[1], bmax[2] - bmin[2]);
                let inside = (0..3).all(|k| HP_LOCAL[k] >= bmin[k] - 0.5 && HP_LOCAL[k] <= bmax[k] + 0.5);
                println!(
                    "MESH {}v/{}t bbox min=({:.1},{:.1},{:.1}) max=({:.1},{:.1},{:.1}) dims=({:.1},{:.1},{:.1}) hardpoint-inside={}",
                    m.verts.len(), m.indices.len() / 3,
                    bmin[0], bmin[1], bmin[2], bmax[0], bmax[1], bmax[2], dx, dy, dz, inside
                );
            }
            None => println!("no mesh ASET for this hash"),
        }
    }
    Ok(())
}

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
        // PMC main-hall / HQ-interior guesses following the resolving patterns
        // (`Xoutpost_interior_job`, `pmcoutpost_interior_*`, merida = the PMC town)
        "pmcoutpost_interior_job", "pmcoutpost_interior_hq", "pmcoutpost_interior_mainhall",
        "pmcoutpost_interior", "pmcoutpost_interior_main", "pmcoutpost_interior_hall",
        "pmcoutpost_interior_room", "pmcoutpost_interior_recruitfiona", "pmcoutpost_interior_base",
        "pmcoutpost_interior_recruit", "pmcoutpost_bld_hq_interior", "pmchq_interior",
        "pmc_interior_hq", "merida_bld_pmchq_interior", "meridahq_interior", "merida_interior_job",
        "merida_bld_pmc_interior", "pmcoutpost_interior_hqmain", "pmcoutpost_mainhall",
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
    use mercs2_engine::scene::{AssetStore, ModelAnim, Scene};
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


/// The control-driven streaming world with a free-fly camera (the no-arg default boot; also
/// `--stream`). Mirrors the original engine's ONE streaming system (spec §10): a background loader
/// builds the block index + Layer-2 decision catalog, then each frame the pure `StreamingManager`
/// turns the camera position into a load/unload/wake/hibernate diff, and this executor performs the
/// GPU work — LOAD c3-cell geometry + WAKE `ModelName` props (via the proven recipes), and the
/// net-new UNLOAD path (despawn + free GPU). Free-fly camera reuses the Shadow-PC dual-source mouse
/// input (CursorMoved+recentre fallback, never DeviceEvent on absolute-coordinate streams).
/// Parse `--spawn=X,Y,Z` (comma-separated world coords) into an initial free-fly camera position.
/// `mercs2_game` passes the authentic PMC-interior start; absent = the default exterior bird's-eye.
fn spawn_arg(args: &[String]) -> Option<[f32; 3]> {
    let v = args.iter().find_map(|a| a.strip_prefix("--spawn="))?;
    let mut it = v.split(',').map(|s| s.trim().parse::<f32>());
    match (it.next(), it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y)), Some(Ok(z))) => Some([x, y, z]),
        _ => {
            eprintln!("[spawn] ignoring malformed --spawn={v} (want X,Y,Z)");
            None
        }
    }
}

/// Read the active vz_state overlay layer names from `--overlays=<file>` (one per line; `mercs2_game`
/// writes them from the save's `SaveState`). Absent/unreadable = base world only.
fn overlays_arg(args: &[String]) -> Vec<String> {
    let Some(path) = args.iter().find_map(|a| a.strip_prefix("--overlays=")) else {
        return Vec::new();
    };
    match std::fs::read_to_string(path) {
        Ok(s) => s
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
        Err(e) => {
            eprintln!("[overlays] read {path}: {e}");
            Vec::new()
        }
    }
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

/// ECS-driven scene path — the `mercs2_core` spine plus a multi-model asset store. Each distinct
/// model is uploaded once (GPU) and its rig+clip stored once (CPU); entities reference models by
/// hash, so two entities can share one model asset yet animate independently. The `animation` system
/// advances every playing entity and samples its model's clip into that entity's `SkinPalette`; the
/// `Scene` walks the `World` and draws each entity with its own transform + palette.
async fn run_scene_ecs(models: Vec<LoadedModel>, title: String) {
    use mercs2_engine::scene::{AssetStore, ModelAnim, Scene};
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
