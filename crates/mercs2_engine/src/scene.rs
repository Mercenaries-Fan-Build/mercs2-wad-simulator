//! Multi-entity, multi-model scene renderer for the ECS path.
//!
//! Unlike the single-model `Renderer` (used by `--animate`), `Scene` owns the shared GPU state
//! (device/pipeline/layouts) once, an **asset store** of per-model GPU resources keyed by model
//! hash, and per-entity resources (each entity's own MVP + skinning palette). It walks the
//! `mercs2_core` `World` each frame and draws every entity that has a `Transform`, a `ModelRef`
//! pointing at a loaded model, and a `SkinPalette`. Two entities can share one model asset (instancing)
//! yet hold independent poses and world transforms.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use winit::window::Window;

use mercs2_core::{Entity, ModelRef, SkinPalette, Transform, World};

use crate::{make_bc_view, make_depth, make_flat_normal_view, make_tex_bind, make_white_view};
use crate::{ClipAnim, TexMap, DEPTH_FORMAT};
use mercs2_engine::mesh::{self, Vertex};
use mercs2_engine::pose;

/// CPU-side per-model data the animation system needs (read-only after load, shared via `Rc`).
pub struct ModelAnim {
    pub rig: Vec<mesh::BoneRig>,
    /// Clips keyed by name hash; `AnimState.clip` selects one (0 / unknown falls back to any).
    pub clips: HashMap<u32, ClipAnim>,
}

/// The CPU asset store: model-hash -> the animation inputs for that model.
#[derive(Default)]
pub struct AssetStore {
    pub models: HashMap<u32, ModelAnim>,
}

/// GPU-side per-model resources (geometry + materials). Built once per distinct model.
struct ModelGpu {
    vbuf: wgpu::Buffer,
    ibuf: Option<wgpu::Buffer>,
    nindices: u32,
    nverts: u32,
    /// (index_start, index_count, tex_binds index) per draw group.
    draw_calls: Vec<(u32, u32, usize)>,
    tex_binds: Vec<wgpu::BindGroup>,
    /// Number of bones in this model's palette (>=1; 1 = unskinned identity).
    bone_count: usize,
    /// Model-fit (centre + uniform scale) so each model is normalised for viewing.
    fit: glam::Mat4,
}

/// Per-entity GPU resources: its own MVP uniform (group 0) and skinning palette (group 2).
struct EntityGpu {
    mvp_buf: wgpu::Buffer,
    mvp_bind: wgpu::BindGroup,
    bone_buf: wgpu::Buffer,
    bone_bind: wgpu::BindGroup,
    bone_count: usize,
}

pub struct Scene {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pub size: winit::dpi::PhysicalSize<u32>,
    pipeline: wgpu::RenderPipeline,
    camera_bgl: wgpu::BindGroupLayout,
    bone_bgl: wgpu::BindGroupLayout,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    flat_normal: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    models: HashMap<u32, ModelGpu>,
    entities: HashMap<Entity, EntityGpu>,
    /// Per-model set of draw-call indices to SKIP at render (e.g. low-res terrain tiles hidden where
    /// their hi-res terrainmesh counterpart is resident — the terrain LOD swap).
    hidden_draws: HashMap<u32, HashSet<usize>>,
    start: std::time::Instant,
    /// Explicit view matrix + (near, far) supplied per frame by a caller-driven camera
    /// (the fly / third-person camera). `None` = default close orbit around the origin (`--ecs`).
    view_cam: Option<(glam::Mat4, f32, f32)>,
    /// Distance-fog params (color, density, start). `None` = fog + sky pass disabled, so the
    /// `--ecs` / `--animate` visuals are unchanged. PLACEHOLDER for PgSky/PgSun/PgCloud.
    fog: Option<([f32; 3], f32, f32)>,
    sky_pipeline: wgpu::RenderPipeline,
    sky_buf: wgpu::Buffer,
    sky_bind: wgpu::BindGroup,
    loading_pipeline: wgpu::RenderPipeline,
    loading_buf: wgpu::Buffer,
    loading_bind: wgpu::BindGroup,
    /// Group 1 of the loading pass: the shell.wad plate once `set_loading_art` delivers it
    /// (white fallback + aspect 0.0 = "no art", the shader keeps the plain clear color).
    loading_art_bind: wgpu::BindGroup,
    loading_art_aspect: f32,
}

impl Scene {
    pub async fn new(window: Arc<Window>) -> Scene {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).expect("create_surface");
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
                    label: Some("mercs2_engine scene device"),
                    required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
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

        // group 0: per-entity MVP uniform (+ fog params, read by the fragment stage).
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
        // group 2: per-entity bone palette (read-only storage).
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
        // group 1: material (diffuse + normal + sampler).
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
        let white = make_white_view(&device, &queue);
        let flat_normal = make_flat_normal_view(&device, &queue);

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[&camera_bgl, &tex_bgl, &bone_bgl],
            push_constant_ranges: &[],
        });
        let vbuf_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Float32x4, 5 => Uint8x4, 6 => Unorm8x4],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene geometry pipeline"),
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
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
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
        let depth_view = make_depth(&device, &config);

        // Sky pass (PLACEHOLDER for the game's PgSky/PgSun/PgCloud shader stack): a fullscreen
        // gradient dome, no vertex buffers, drawn first at the far plane with depth writes off.
        let sky_shader = device.create_shader_module(wgpu::include_wgsl!("sky.wgsl"));
        let sky_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky bgl"),
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
        // mat4 inv_view_proj (64) + vec4 sun_dir (16) + vec4 params (16) = 96 B.
        let sky_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniform"),
            size: 96,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky bind"),
            layout: &sky_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sky_buf.as_entire_binding() }],
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky pipeline layout"),
            bind_group_layouts: &[&sky_bgl],
            push_constant_ranges: &[],
        });
        let sky_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky pipeline"),
            layout: Some(&sky_layout),
            vertex: wgpu::VertexState {
                module: &sky_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sky_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Loading-screen pass: the same fullscreen-triangle trick as the sky, drawn alone (no
        // world) while the background loader runs; the uniform carries (time, aspect) for the
        // arc spinner in loading.wgsl.
        let loading_shader = device.create_shader_module(wgpu::include_wgsl!("loading.wgsl"));
        let loading_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("loading bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        // vec4 params: (time, aspect, art aspect, progress) = 16 B.
        let loading_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("loading uniform"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let loading_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loading bind"),
            layout: &loading_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: loading_buf.as_entire_binding() }],
        });
        let loading_art_bind = make_tex_bind(&device, &tex_bgl, &sampler, &white, &flat_normal);
        let loading_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("loading pipeline layout"),
            bind_group_layouts: &[&loading_bgl, &tex_bgl],
            push_constant_ranges: &[],
        });
        let loading_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("loading pipeline"),
            layout: Some(&loading_layout),
            vertex: wgpu::VertexState {
                module: &loading_shader,
                entry_point: "vs_main",
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &loading_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        Scene {
            window,
            surface,
            device,
            queue,
            config,
            size,
            pipeline,
            camera_bgl,
            bone_bgl,
            tex_bgl,
            sampler,
            white,
            flat_normal,
            depth_view,
            models: HashMap::new(),
            hidden_draws: HashMap::new(),
            entities: HashMap::new(),
            start: std::time::Instant::now(),
            view_cam: None,
            fog: None,
            sky_pipeline,
            sky_buf,
            sky_bind,
            loading_pipeline,
            loading_buf,
            loading_bind,
            loading_art_bind,
            loading_art_aspect: 0.0,
        }
    }

    /// Provide the real loading-screen background (the shell.wad plate): upload + bind it and
    /// record its aspect; `render_loading` letterboxes it behind the spinner from then on.
    pub fn set_loading_art(&mut self, td: &mercs2_formats::texture::TextureData) {
        if let Some(v) = make_bc_view(&self.device, &self.queue, td, true) {
            self.loading_art_bind =
                make_tex_bind(&self.device, &self.tex_bgl, &self.sampler, &v, &self.flat_normal);
            self.loading_art_aspect = td.width as f32 / td.height.max(1) as f32;
        }
    }

    /// Render the loading screen alone (no world): the letterboxed shell.wad plate (if
    /// `set_loading_art` was called) over the same dark clear color + the spinner arc.
    /// `t` = seconds since the loading screen appeared (drives the rotation);
    /// `progress` = 0..1 staged-load fraction (fills the plate's bar frame).
    pub fn render_loading(&mut self, t: f32, progress: f32) -> Result<(), wgpu::SurfaceError> {
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        self.queue.write_buffer(
            &self.loading_buf,
            0,
            bytemuck::cast_slice(&[t, aspect, self.loading_art_aspect, progress]),
        );
        let output = self.surface.get_current_texture()?;
        let view_tex = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("loading frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("loading pass"),
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
            pass.set_pipeline(&self.loading_pipeline);
            pass.set_bind_group(0, &self.loading_bind, &[]);
            pass.set_bind_group(1, &self.loading_art_bind, &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Set an explicit view matrix (+ near/far) for this frame, from a caller-driven camera.
    /// Overrides the default close-orbit auto-camera. The projection is built in `render` from
    /// the current aspect ratio.
    pub fn set_view(&mut self, view: glam::Mat4, near: f32, far: f32) {
        self.view_cam = Some((view, near, far));
    }

    /// Enable distance fog + the sky pass (PLACEHOLDER for PgSky/PgSun/PgCloud).
    /// `density` drives `1 - exp(-density * (view_depth - start))`; the sky horizon matches `color`.
    pub fn set_fog(&mut self, color: [f32; 3], density: f32, start: f32) {
        self.fog = Some((color, density, start));
    }

    /// Upload a model's geometry + materials into the store, keyed by hash. Idempotent per hash.
    pub fn load_model(
        &mut self,
        hash: u32,
        verts: &[Vertex],
        indices: &[u32],
        draws: &[mesh::DrawGroup],
        textures: &TexMap,
        skin: &mesh::SkinData,
    ) {
        if self.models.contains_key(&hash) {
            return;
        }
        // Decode each texture to a view (normal maps linear, diffuse sRGB).
        let normal_hashes: HashSet<u32> = draws.iter().filter_map(|d| d.normal).collect();
        let mut views: HashMap<u32, wgpu::TextureView> = HashMap::new();
        for (h, td) in textures {
            let srgb = !normal_hashes.contains(h);
            if let Some(v) = make_bc_view(&self.device, &self.queue, td, srgb) {
                views.insert(*h, v);
            } else if std::env::var("MERCS2_TEXDBG").is_ok() {
                eprintln!(
                    "[texdbg] make_bc_view FAILED for 0x{h:08X}: {}x{} fmt={:?} {}mips {}B",
                    td.width, td.height, td.format, td.mip_count, td.all_mips.len()
                );
            }
        }
        let mut tex_binds =
            vec![make_tex_bind(&self.device, &self.tex_bgl, &self.sampler, &self.white, &self.flat_normal)];
        let mut draw_calls: Vec<(u32, u32, usize)> = Vec::new();
        for d in draws {
            let diff = d.diffuse.and_then(|h| views.get(&h)).unwrap_or(&self.white);
            let norm = d.normal.and_then(|h| views.get(&h)).unwrap_or(&self.flat_normal);
            let idx = tex_binds.len();
            tex_binds.push(make_tex_bind(&self.device, &self.tex_bgl, &self.sampler, diff, norm));
            draw_calls.push((d.index_start, d.index_count, idx));
        }

        let vbytes: &[u8] = bytemuck::cast_slice(verts);
        let vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("scene vbuf"),
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
            let b = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("scene ibuf"),
                size: ibytes.len() as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::INDEX,
                mapped_at_creation: true,
            });
            b.slice(..).get_mapped_range_mut().copy_from_slice(ibytes);
            b.unmap();
            (Some(b), indices.len() as u32)
        };

        let fit = glam::Mat4::from_scale(glam::Vec3::splat(skin.scale))
            * glam::Mat4::from_translation(-glam::Vec3::from(skin.center));
        let bone_count = skin.bones.len().max(1);

        self.models.insert(
            hash,
            ModelGpu {
                vbuf,
                ibuf,
                nindices,
                nverts: verts.len() as u32,
                draw_calls,
                tex_binds,
                bone_count,
                fit,
            },
        );
    }

    /// Whether a model is currently uploaded (keyed by hash).
    pub fn has_model(&self, hash: u32) -> bool {
        self.models.contains_key(&hash)
    }

    /// Hide or show a single draw call of a model (by draw-call index). Used for the terrain LOD
    /// swap: hide a low-res tile's draw group while its hi-res terrainmesh is resident.
    pub fn set_draw_hidden(&mut self, hash: u32, draw_index: usize, hidden: bool) {
        let set = self.hidden_draws.entry(hash).or_default();
        if hidden {
            set.insert(draw_index);
        } else {
            set.remove(&draw_index);
        }
    }

    /// Bone count of a loaded model (>=1; 1 = unskinned identity). 0 if the model isn't loaded.
    /// The streaming executor sizes a woken prop's identity `SkinPalette` to this so a rigged prop's
    /// verts (weighted to bone >= 1) don't collapse to the origin under a 1-bone palette.
    pub fn model_bone_count(&self, hash: u32) -> usize {
        self.models.get(&hash).map(|m| m.bone_count).unwrap_or(0)
    }

    /// Free a model's GPU resources (geometry + material bind groups), if loaded. The streaming
    /// executor calls this once the last entity referencing a model hibernates/unloads — net-new
    /// GPU UNLOAD (nothing in the codebase freed GPU before the streaming runtime). wgpu buffers
    /// and bind groups drop with the removed `ModelGpu`.
    pub fn unload_model(&mut self, hash: u32) {
        self.models.remove(&hash);
    }

    /// Drop a despawned entity's per-entity GPU resources (its MVP uniform + bone palette buffer).
    /// Call when the ECS entity is despawned so its buffers/bind groups are freed and the entity
    /// map does not leak. Safe to call for an unknown entity.
    pub fn forget_entity(&mut self, e: Entity) {
        self.entities.remove(&e);
    }

    /// Create per-entity GPU resources (MVP + bone palette) sized to the model, once.
    fn ensure_entity(&mut self, e: Entity, bone_count: usize) {
        if self.entities.contains_key(&e) {
            return;
        }
        let mvp_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entity mvp"),
            size: 96, // mat4 mvp (64) + vec4 fog_color_density (16) + vec4 fog_misc (16)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mvp_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entity mvp bind"),
            layout: &self.camera_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: mvp_buf.as_entire_binding() }],
        });
        let bone_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("entity bones"),
            size: (bone_count * 64) as wgpu::BufferAddress, // 16 floats * 4 bytes per bone
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bone_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("entity bone bind"),
            layout: &self.bone_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: bone_buf.as_entire_binding() }],
        });
        self.entities
            .insert(e, EntityGpu { mvp_buf, mvp_bind, bone_buf, bone_bind, bone_count });
    }

    pub fn resize(&mut self, new: winit::dpi::PhysicalSize<u32>) {
        if new.width > 0 && new.height > 0 {
            self.size = new;
            self.config.width = new.width;
            self.config.height = new.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_view = make_depth(&self.device, &self.config);
        }
    }

    /// Draw every drawable entity in the world. Auto-orbit camera framing the origin.
    pub fn render(&mut self, world: &World) -> Result<(), wgpu::SurfaceError> {
        let t = self.start.elapsed().as_secs_f32();
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let view_proj = if let Some((view, near, far)) = self.view_cam {
            // Caller-driven fly / third-person camera: explicit view, projection from aspect.
            let proj = glam::Mat4::perspective_lh(60f32.to_radians(), aspect, near, far);
            proj * view
        } else {
            let angle = t * 0.5;
            let radius = 3.4f32;
            let eye = glam::Vec3::new(radius * angle.sin(), 0.3, radius * angle.cos());
            let view = glam::Mat4::look_at_lh(eye, glam::Vec3::ZERO, glam::Vec3::Y);
            let proj = glam::Mat4::perspective_lh(45f32.to_radians(), aspect, 0.1, 100.0);
            proj * view
        };

        // Snapshot the drawable entities (copy out to release the world borrow before touching self).
        let mut items: Vec<(Entity, glam::Mat4, u32, Vec<[[f32; 4]; 4]>)> = Vec::new();
        for (e, (xform, mref, pal)) in world.query::<(&Transform, &ModelRef, &SkinPalette)>().iter() {
            items.push((e, xform.matrix(), mref.model, pal.mats.clone()));
        }

        // Phase 1: ensure per-entity resources and upload their MVP + palette.
        for (e, entity_model, model_hash, palette) in &items {
            let Some(mg) = self.models.get(model_hash) else { continue };
            let (bone_count, fit) = (mg.bone_count, mg.fit);
            self.ensure_entity(*e, bone_count);
            let mvp = view_proj * (*entity_model * fit); // entity transform placed in fitted world space
            let eg = &self.entities[e];
            // mvp (16 floats) + fog_color_density + fog_misc (fog disabled unless set_fog was called).
            let mut uni = [0f32; 24];
            uni[..16].copy_from_slice(&mvp.to_cols_array());
            if let Some((color, density, fog_start)) = self.fog {
                uni[16..20].copy_from_slice(&[color[0], color[1], color[2], density]);
                uni[20] = 1.0; // fog enable
                uni[21] = fog_start;
            }
            self.queue.write_buffer(&eg.mvp_buf, 0, bytemuck::cast_slice(&uni));
            if !palette.is_empty() {
                // Clamp to the entity's allocated bone count so a mismatched palette can't overflow.
                let n = palette.len().min(eg.bone_count);
                self.queue
                    .write_buffer(&eg.bone_buf, 0, bytemuck::cast_slice(&pose::flatten(&palette[..n])));
            }
        }

        // Sky uniform: inverse view-proj (for per-pixel ray reconstruction) + sun dir + horizon
        // color (= fog color). Only when the fog/sky pass is enabled.
        if let Some((color, _density, _start)) = self.fog {
            let inv = view_proj.inverse();
            let sun = glam::Vec3::new(0.3, 0.35, 0.6).normalize();
            let mut su = [0f32; 24];
            su[..16].copy_from_slice(&inv.to_cols_array());
            su[16..20].copy_from_slice(&[sun.x, sun.y, sun.z, 0.0]);
            su[20..23].copy_from_slice(&color);
            self.queue.write_buffer(&self.sky_buf, 0, bytemuck::cast_slice(&su));
        }

        // Phase 2: record the pass.
        let output = self.surface.get_current_texture()?;
        let view_tex = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene frame") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
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
            // Sky first (fullscreen triangle at the far plane); entities then draw over it.
            if self.fog.is_some() {
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(0, &self.sky_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            pass.set_pipeline(&self.pipeline);
            for (e, _m, model_hash, _p) in &items {
                let Some(mg) = self.models.get(model_hash) else { continue };
                let Some(eg) = self.entities.get(e) else { continue };
                pass.set_bind_group(0, &eg.mvp_bind, &[]);
                pass.set_bind_group(2, &eg.bone_bind, &[]);
                pass.set_vertex_buffer(0, mg.vbuf.slice(..));
                if let Some(ib) = &mg.ibuf {
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    let hidden = self.hidden_draws.get(model_hash);
                    for (di, &(start, count, bind)) in mg.draw_calls.iter().enumerate() {
                        if hidden.is_some_and(|h| h.contains(&di)) {
                            continue; // e.g. low-res terrain tile hidden under resident hi-res
                        }
                        pass.set_bind_group(1, &mg.tex_binds[bind], &[]);
                        pass.draw_indexed(start..start + count, 0, 0..1);
                    }
                    if mg.draw_calls.is_empty() {
                        pass.set_bind_group(1, &mg.tex_binds[0], &[]);
                        pass.draw_indexed(0..mg.nindices, 0, 0..1);
                    }
                } else {
                    pass.set_bind_group(1, &mg.tex_binds[0], &[]);
                    pass.draw(0..mg.nverts, 0..1);
                }
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}
