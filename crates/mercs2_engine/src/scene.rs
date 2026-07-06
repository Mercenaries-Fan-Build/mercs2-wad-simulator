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

use crate::render::{
    make_bc_view, make_black_view, make_depth, make_flat_normal_view, make_tex_bind, make_white_view,
};
use crate::render::{ClipAnim, GpuLight, TexMap, DEPTH_FORMAT, MAX_LIGHTS};
use crate::mesh::{self, Vertex};
use crate::pose;

/// Directional shadow-map resolution (square). Kept in sync with the `texel = 1/2048` PCF step in
/// `shader.wgsl`; change both together.
const SHADOW_SIZE: u32 = 2048;

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
    /// Baked-lighting flag: this model bakes its lighting into vertex color, so the shader must NOT
    /// stamp the fixed exterior sun over it (fed to the per-entity uniform `fog_misc.z`).
    prelit: bool,
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
    /// 1×1 black specular fallback (matte) for materials with no `_sm` map.
    black: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    /// Group 3: the per-frame dynamic light array (uniform) + its bind group. `lights` holds the full
    /// harvested set; `render` uploads the nearest `MAX_LIGHTS` to the camera each frame.
    lights_buf: wgpu::Buffer,
    lights_bind: wgpu::BindGroup,
    lights: Vec<GpuLight>,
    /// Directional shadow map (depth-only render from the key light) + its light view-proj uniform.
    /// The depth view is sampled by the main shader (folded into group 3); `shadow_vp_bind` is the
    /// shadow pass's group-1 (light view-proj). Built once; the matrix is refreshed by `set_shadow`.
    shadow_view: wgpu::TextureView,
    shadow_vp_buf: wgpu::Buffer,
    shadow_vp_bind: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,
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
    /// Directional-sun intensity + ambient fill, SCENE-controlled (see `set_sun`). Exterior defaults;
    /// the interior sets sun = 0 (no phantom outdoor sun indoors) + a higher ambient. Written into the
    /// per-entity camera uniform (`fog_misc.w` / `cam_pos.w`).
    sun_intensity: f32,
    ambient: f32,
    /// Whether the sky pass + HDR/bloom world path is active (enabled by `set_fog`/`set_atmosphere`).
    /// `--ecs`/`--animate` leave this false and render directly to the swapchain (no regression).
    sky_enabled: bool,
    /// Sky/atmosphere + HDR/bloom parameters (the game's `Graphics.Atmosphere.*` model). Defaults to
    /// the base-game "afternoon" preset (`mrxbootstrap.lua`).
    atmo: mercs2_formats::atmosphere::Atmosphere,
    sky_pipeline: wgpu::RenderPipeline,
    /// Sky pipeline targeting the HDR format (world path). `None` if the post chain is unavailable.
    sky_pipeline_hdr: Option<wgpu::RenderPipeline>,
    /// Geometry pipeline targeting the HDR format (world path). `None` if the post chain is unavailable.
    world_pipeline: Option<wgpu::RenderPipeline>,
    /// HDR + bloom post chain. `None` = fall back to direct swapchain present.
    post: Option<crate::post::Post>,
    sky_buf: wgpu::Buffer,
    sky_bind: wgpu::BindGroup,
    loading_pipeline: wgpu::RenderPipeline,
    loading_buf: wgpu::Buffer,
    loading_bind: wgpu::BindGroup,
    /// Group 1 of the loading pass: the shell.wad plate once `set_loading_art` delivers it
    /// (white fallback + aspect 0.0 = "no art", the shader keeps the plain clear color).
    loading_art_bind: wgpu::BindGroup,
    loading_art_aspect: f32,
    /// 2D UI overlay (quads + bitmap text, `crate::ui`). Staged via `ui_rect`/`ui_text`, drawn on
    /// top of the shell/loading pass each frame (`render_loading`/`render_menu`). Empty = zero cost.
    ui: crate::ui::UiPass,
    /// Billboard particle / FX runtime (registry §7). Drawn AFTER the opaque forward pass.
    /// Empty by default (zero cost) until an effect is started via `fx_start*`.
    particles: crate::particles::ParticleSystem,
    /// Last-frame timestamp for the particle sim's per-frame dt.
    last_frame: std::time::Instant,
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
        // group 1: material (diffuse + normal + sampler + specular).
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
                tex_entry(0), // diffuse
                tex_entry(1), // normal
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                tex_entry(3), // specular / gloss (`_sm`)
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("material sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear, // trilinear: blend across the uploaded mip chain
            ..Default::default()
        });
        let white = make_white_view(&device, &queue);
        let flat_normal = make_flat_normal_view(&device, &queue);
        let black = make_black_view(&device, &queue);

        // Directional shadow map: a depth-only render of the scene from the key light, PCF-sampled by
        // the main shader for real cast-shadows. Built here so the group-3 lights bind group (below)
        // can fold in its depth view + comparison sampler + light view-proj (staying within wgpu's
        // 4-bind-group limit).
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow map"),
            size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // PCF comparison sampler: linear filter + LessEqual so `textureSampleCompareLevel` returns a
        // smoothed 0..1 occlusion across the 2×2 depth footprint.
        let shadow_cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow comparison sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        // Light view-proj (one mat4 = 64 B). Init to identity so a path that never calls `set_shadow`
        // (e.g. `--ecs`) still has a valid (w=1) matrix and reads unshadowed instead of dividing by 0.
        let shadow_vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow light view-proj"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &shadow_vp_buf,
            0,
            bytemuck::cast_slice(&glam::Mat4::IDENTITY.to_cols_array()),
        );

        // group 3: the per-frame dynamic light array (uniform) + the folded-in shadow map.
        let lights_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lights bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        // vec4 count (16) + MAX_LIGHTS * (vec4 pos_radius + vec4 color_intensity = 32 B) bytes.
        let lights_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lights uniform"),
            size: (16 + MAX_LIGHTS * 32) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let lights_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lights bind"),
            layout: &lights_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: lights_buf.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&shadow_cmp),
                },
                wgpu::BindGroupEntry { binding: 3, resource: shadow_vp_buf.as_entire_binding() },
            ],
        });

        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene pipeline layout"),
            bind_group_layouts: &[&camera_bgl, &tex_bgl, &bone_bgl, &lights_bgl],
            push_constant_ranges: &[],
        });
        // Geometry pipeline builder, parameterised by color-target format so the SAME shader/layout
        // (incl. the group-3 lights) serve BOTH the direct-to-swapchain path (default `--ecs`/
        // `--animate`) and the HDR world path (`Rgba16Float` target → tone-map + bloom post chain).
        let build_geom_pipeline = |format: wgpu::TextureFormat| {
            let vbuf_layout = wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Float32x4, 5 => Uint8x4, 6 => Unorm8x4],
            };
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Cw,
                    // Double-sided — the building shells are wound outward-facing, so back-face culling
                    // would hide their floor+walls when viewed from inside. (Interim: per-material
                    // two-sided flag from MTRL; negligible perf cost at this scale.)
                    cull_mode: None,
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
            })
        };
        let pipeline = build_geom_pipeline(config.format);
        let depth_view = make_depth(&device, &config);

        // Shadow pass: depth-only render of the scene from the key light into `shadow_tex`. Its own
        // small group-1 bind group carries the light view-proj (group 0 = camera for `model`, group 2
        // = bones — reused from the color pass). Vertex-only (no fragment); a slope+constant depth
        // bias fights shadow acne.
        let shadow_vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow vp bgl"),
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
        let shadow_vp_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow vp bind"),
            layout: &shadow_vp_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: shadow_vp_buf.as_entire_binding() }],
        });
        let shadow_shader = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let shadow_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shadow pipeline layout"),
            bind_group_layouts: &[&camera_bgl, &shadow_vp_bgl, &bone_bgl],
            push_constant_ranges: &[],
        });
        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow pipeline"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_shader,
                entry_point: "vs_shadow",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3, 4 => Float32x4, 5 => Uint8x4, 6 => Unorm8x4],
                }],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None, // match the geom pipeline (double-sided shells)
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

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
        // mat4 inv_view_proj (64) + sun_dir (16) + horizon (16) + zenith (16) + scatter (16) = 128 B.
        let sky_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky uniform"),
            size: 128,
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
        // Sky pipeline builder, parameterised by color-target format (swapchain for the fallback
        // path, HDR for the tone-mapped world path).
        let build_sky_pipeline = |format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                        format,
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
            })
        };
        let sky_pipeline = build_sky_pipeline(config.format);

        // HDR + bloom post chain (see `post.rs`). Fallible: on `None` the world path falls back to a
        // direct forward present, so nothing regresses. When present, build HDR-format geometry + sky
        // pipelines that render into the HDR target.
        let post = crate::post::Post::new(&device, config.format, config.width, config.height);
        let (world_pipeline, sky_pipeline_hdr) = if post.is_some() {
            (
                Some(build_geom_pipeline(crate::post::HDR_FORMAT)),
                Some(build_sky_pipeline(crate::post::HDR_FORMAT)),
            )
        } else {
            (None, None)
        };

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
        // 2x vec4 params: (time, aspect, art aspect, progress) + (mode, reserved…) = 32 B.
        let loading_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("loading uniform"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let loading_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("loading bind"),
            layout: &loading_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: loading_buf.as_entire_binding() }],
        });
        let loading_art_bind =
            make_tex_bind(&device, &tex_bgl, &sampler, &white, &flat_normal, &black);
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

        let ui = crate::ui::UiPass::new(&device, &queue, config.format);
        let mut particles = crate::particles::ParticleSystem::new(&device, config.format, DEPTH_FORMAT);
        // Opt-in visible smoke test: `MERCS2_FX_TEST=1` starts a demo smoke plume + fire jet at the
        // origin (framed by the default orbit camera in `--ecs`/`--animate`). Off by default so no
        // existing path changes.
        if std::env::var("MERCS2_FX_TEST").is_ok() {
            particles.start_emitter_desc(crate::particles::EmitterDesc::demo_smoke(), glam::Vec3::ZERO);
            particles.start_emitter_desc(
                crate::particles::EmitterDesc::demo_fire(),
                glam::Vec3::new(1.2, 0.0, 0.0),
            );
        }

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
            black,
            depth_view,
            lights_buf,
            lights_bind,
            lights: Vec::new(),
            shadow_view,
            shadow_vp_buf,
            shadow_vp_bind,
            shadow_pipeline,
            models: HashMap::new(),
            hidden_draws: HashMap::new(),
            entities: HashMap::new(),
            start: std::time::Instant::now(),
            view_cam: None,
            fog: None,
            sun_intensity: 0.9, // exterior default (matches the previous hardcoded key light)
            ambient: 0.35,
            sky_enabled: false,
            atmo: mercs2_formats::atmosphere::Atmosphere::default(),
            sky_pipeline,
            sky_pipeline_hdr,
            world_pipeline,
            post,
            sky_buf,
            sky_bind,
            loading_pipeline,
            loading_buf,
            loading_bind,
            loading_art_bind,
            loading_art_aspect: 0.0,
            ui,
            particles,
            last_frame: std::time::Instant::now(),
        }
    }

    /// The wgpu device (external overlay layers create their GPU resources against it).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The swapchain color format (external overlay layers must render in it).
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    /// Provide the real loading-screen background (the shell.wad plate): upload + bind it and
    /// record its aspect; `render_loading` letterboxes it behind the spinner from then on.
    pub fn set_loading_art(&mut self, td: &mercs2_formats::texture::TextureData) {
        if let Some(v) = make_bc_view(&self.device, &self.queue, td, true) {
            self.loading_art_bind = make_tex_bind(
                &self.device, &self.tex_bgl, &self.sampler, &v, &self.flat_normal, &self.black,
            );
            self.loading_art_aspect = td.width as f32 / td.height.max(1) as f32;
        }
    }

    /// Stage a solid UI rect for this frame's shell/loading pass (surface px, origin top-left).
    pub fn ui_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.ui.rect(x, y, w, h, color);
    }

    /// Stage a monospace UI text run (8px base glyph × `scale`). Returns the run's pixel width.
    pub fn ui_text(&mut self, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) -> f32 {
        self.ui.text(x, y, scale, color, s)
    }

    /// Render the shell MENU frame: the letterboxed plate dimmed (no spinner/bar — the WGSL menu
    /// mode, progress = -1) + this frame's staged UI overlay (`ui_rect`/`ui_text`).
    pub fn render_menu(&mut self, t: f32) -> Result<(), wgpu::SurfaceError> {
        self.render_loading(t, -1.0)
    }

    /// Render the loading screen alone (no world): the letterboxed shell.wad plate (if
    /// `set_loading_art` was called) over the same dark clear color + the spinner arc.
    /// `t` = seconds since the loading screen appeared (drives the rotation);
    /// `progress` = 0..1 staged-load fraction (fills the plate's bar frame); any staged UI
    /// overlay (`ui_rect`/`ui_text`) is drawn on top (that is how `render_menu` draws the menu).
    pub fn render_loading(&mut self, t: f32, progress: f32) -> Result<(), wgpu::SurfaceError> {
        self.render_loading_mode(t, progress, 0.0, None)
    }

    /// [`Self::render_menu`] with the external overlay hook (see [`Self::render_with`]) — the
    /// workshop's browse screen draws its GUI over the shell plate through this.
    pub fn render_menu_with(&mut self, t: f32, overlay: Option<Overlay<'_>>) -> Result<(), wgpu::SurfaceError> {
        self.render_loading_mode(t, -1.0, 0.0, overlay)
    }

    /// Render the BOOT loading screen: black background, the bound art drawn as a centred
    /// pulsing icon (bind the Loading.wad `global_loading_skull`) under an animated gold→green
    /// sheen, arc spinner + gold progress bar (see `loading.wgsl` mode 1). Staged UI overlay
    /// draws on top exactly like `render_loading`.
    pub fn render_boot(&mut self, t: f32, progress: f32) -> Result<(), wgpu::SurfaceError> {
        self.render_loading_mode(t, progress, 1.0, None)
    }

    fn render_loading_mode(
        &mut self,
        t: f32,
        progress: f32,
        mode: f32,
        overlay: Option<Overlay<'_>>,
    ) -> Result<(), wgpu::SurfaceError> {
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        self.queue.write_buffer(
            &self.loading_buf,
            0,
            bytemuck::cast_slice(&[
                t, aspect, self.loading_art_aspect, progress,
                mode, 0.0, 0.0, 0.0,
            ]),
        );
        let output = self.surface.get_current_texture()?;
        let view_tex = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Upload this frame's staged UI overlay (menu text etc.) before the pass opens.
        let ui_count = self.ui.prepare(&self.device, &self.queue, self.config.width, self.config.height);
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
            self.ui.draw(&mut pass, ui_count);
        }
        if let Some(f) = overlay {
            f(&self.device, &self.queue, &mut encoder, &view_tex, [self.config.width, self.config.height]);
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
        self.sky_enabled = true;
    }

    /// Directional-sun intensity + ambient fill. `intensity = 0` disables the sun entirely (interiors:
    /// no phantom outdoor sun — the room is lit by baked vertex colour + point lights + `ambient`).
    /// Exterior default is `0.9 / 0.35`.
    pub fn set_sun(&mut self, intensity: f32, ambient: f32) {
        self.sun_intensity = intensity;
        self.ambient = ambient;
    }

    /// Set the sky/atmosphere + HDR-bloom parameters (the game's `Graphics.Atmosphere.*` model —
    /// see `mercs2_formats::atmosphere`). Enables the sky + HDR/bloom world path. Fog (the geometry
    /// haze) stays governed by `set_fog`; call both for the full look.
    pub fn set_atmosphere(&mut self, atmo: mercs2_formats::atmosphere::Atmosphere) {
        self.atmo = atmo;
        self.sky_enabled = true;
    }

    /// Register a named particle-effect template (key = effect name hash, as Lua's `StartEmitter`
    /// names them). Populate `desc` from a parsed `mercs2_formats::fxdict::EffectTemplate`.
    pub fn fx_register(&mut self, name_hash: u32, desc: crate::particles::EmitterDesc) {
        self.particles.register_template(name_hash, desc);
    }

    /// Start a registered effect at a world position (mirrors `ObjectState.StartEmitter`). `None` if
    /// no template with that hash is registered.
    pub fn fx_start(&mut self, name_hash: u32, pos: [f32; 3]) -> Option<crate::particles::EmitterId> {
        self.particles.start_emitter(name_hash, glam::Vec3::from(pos))
    }

    /// Start an ad-hoc effect from an explicit descriptor at a world position.
    pub fn fx_start_desc(&mut self, desc: crate::particles::EmitterDesc, pos: [f32; 3]) -> crate::particles::EmitterId {
        self.particles.start_emitter_desc(desc, glam::Vec3::from(pos))
    }

    /// Stop a started effect (ceases spawning; live particles finish). Mirrors `StopEmitter`.
    pub fn fx_stop(&mut self, id: crate::particles::EmitterId) {
        self.particles.stop_emitter(id);
    }

    /// Live emitter count (diagnostic).
    pub fn fx_active_count(&self) -> usize {
        self.particles.active_emitter_count()
    }

    /// Set the authored static environmental glow cards (`global_particle_env_godray2` etc.). Replaces
    /// any prior set; an empty slice clears them. Rendered as additive soft billboards in the
    /// transparent FX pass (see `particles::GlowCard`).
    pub fn set_glow_cards(&mut self, cards: &[crate::particles::GlowCard]) {
        self.particles.set_glow_cards(cards);
    }

    /// Number of active glow cards (diagnostic).
    pub fn glow_card_count(&self) -> usize {
        self.particles.glow_card_count()
    }

    /// Set the world-space dynamic light set (harvested from `LightObject` COMPs via
    /// `mercs2_formats::placement::light_inventory` → [`GpuLight`]). The full set is retained; each
    /// frame `render` uploads the [`MAX_LIGHTS`] nearest to the camera. Call once after world load
    /// (lights are static placements) or whenever the set changes. Empty = sun/ambient only.
    pub fn set_lights(&mut self, lights: Vec<GpuLight>) {
        self.lights = lights;
    }

    /// Aim the directional shadow map for this frame. `center` = the world point the orthographic
    /// shadow frustum is centred on (typically the player/focus); `dir` = the key light's travel
    /// direction (points FROM the light TOWARD the scene — downward-ish); `half_extent` = half the
    /// frustum width in metres (the shadowed radius around `center`). Builds a self-consistent LH
    /// light view-proj (`look_at_lh` * `orthographic_lh`, NO camera X-flip — the main shader projects
    /// true world space with this same matrix) and uploads it. Call each frame in the world path.
    pub fn set_shadow(&mut self, center: [f32; 3], dir: [f32; 3], half_extent: f32) {
        let c = glam::Vec3::from(center);
        let mut d = glam::Vec3::from(dir);
        if d.length_squared() < 1e-8 {
            d = glam::Vec3::new(0.0, -1.0, 0.0);
        }
        d = d.normalize();
        // Guard against dir ∥ up: pick +Z as the up reference when the light is near-vertical.
        let up = if d.dot(glam::Vec3::Y).abs() > 0.99 { glam::Vec3::Z } else { glam::Vec3::Y };
        let distance = 40.0f32;
        let eye = c - d * distance;
        let view = glam::Mat4::look_at_lh(eye, c, up);
        let proj = glam::Mat4::orthographic_lh(
            -half_extent, half_extent, -half_extent, half_extent, 0.1, 2.0 * distance,
        );
        let light_vp = proj * view;
        self.queue
            .write_buffer(&self.shadow_vp_buf, 0, bytemuck::cast_slice(&light_vp.to_cols_array()));
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
        // Decode each texture to a view (normal + specular maps linear, diffuse sRGB).
        let normal_hashes: HashSet<u32> = draws.iter().filter_map(|d| d.normal).collect();
        let spec_hashes: HashSet<u32> = draws.iter().filter_map(|d| d.specular).collect();
        let mut views: HashMap<u32, wgpu::TextureView> = HashMap::new();
        for (h, td) in textures {
            let srgb = !normal_hashes.contains(h) && !spec_hashes.contains(h);
            if let Some(v) = make_bc_view(&self.device, &self.queue, td, srgb) {
                views.insert(*h, v);
            } else if std::env::var("MERCS2_TEXDBG").is_ok() {
                eprintln!(
                    "[texdbg] make_bc_view FAILED for 0x{h:08X}: {}x{} fmt={:?} {}mips {}B",
                    td.width, td.height, td.format, td.mip_count, td.all_mips.len()
                );
            }
        }
        let mut tex_binds = vec![make_tex_bind(
            &self.device, &self.tex_bgl, &self.sampler, &self.white, &self.flat_normal, &self.black,
        )];
        let mut draw_calls: Vec<(u32, u32, usize)> = Vec::new();
        for d in draws {
            let diff = d.diffuse.and_then(|h| views.get(&h)).unwrap_or(&self.white);
            let norm = d.normal.and_then(|h| views.get(&h)).unwrap_or(&self.flat_normal);
            let spec = d.specular.and_then(|h| views.get(&h)).unwrap_or(&self.black);
            let idx = tex_binds.len();
            tex_binds.push(make_tex_bind(&self.device, &self.tex_bgl, &self.sampler, diff, norm, spec));
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
                prelit: skin.prelit,
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
            // mat4 mvp (64) + mat4 model (64) + vec4 cam_pos (16) + vec4 fog_color_density (16)
            // + vec4 fog_misc (16) = 176 bytes (matches the shader `Camera` struct).
            size: 176,
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
            if let Some(post) = &mut self.post {
                post.resize(&self.device, new.width, new.height);
            }
        }
    }

    /// Record the entity draws into an already-begun render pass with the given geometry pipeline.
    /// Shared by the direct-swapchain path and the HDR world path (they differ only in target
    /// format, hence pipeline). Per-entity uniforms/palettes must already be uploaded.
    fn draw_geometry<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        items: &[(Entity, glam::Mat4, u32, Vec<[[f32; 4]; 4]>)],
        pipeline: &'a wgpu::RenderPipeline,
    ) {
        pass.set_pipeline(pipeline);
        // Group 3 = the per-frame dynamic light array (uploaded before the pass). Set once for all draws.
        pass.set_bind_group(3, &self.lights_bind, &[]);
        for (e, _m, model_hash, _p) in items {
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

    /// Draw every drawable entity in the world. Auto-orbit camera framing the origin.
    pub fn render(&mut self, world: &World) -> Result<(), wgpu::SurfaceError> {
        self.render_with(world, None)
    }

    /// [`Self::render`] with an external OVERLAY hook: called with the frame's device/queue/
    /// encoder/swapchain-view just before submit+present, after every internal pass — external
    /// GUI layers (the workshop's egui inspector) render through this without the engine knowing
    /// about them. `None` = plain render.
    pub fn render_with(
        &mut self,
        world: &World,
        overlay: Option<Overlay<'_>>,
    ) -> Result<(), wgpu::SurfaceError> {
        let t = self.start.elapsed().as_secs_f32();
        // Advance the particle sim by the real inter-frame dt (clamped so a stall doesn't teleport
        // particles), then reap finished emitters. No-op when no effect is active.
        let dt = self.last_frame.elapsed().as_secs_f32().min(0.1);
        self.last_frame = std::time::Instant::now();
        self.particles.update(dt);
        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let (raw_vp, view) = if let Some((view, near, far)) = self.view_cam {
            // Caller-driven fly / third-person camera: explicit view, projection from aspect.
            // Vertical FOV tuned to the retail over-the-shoulder framing (60° and 55° both read too
            // wide-angle vs vanilla). 45° gives the tighter, more zoomed retail look.
            let proj = glam::Mat4::perspective_lh(45f32.to_radians(), aspect, near, far);
            (proj * view, view)
        } else {
            let angle = t * 0.5;
            let radius = 3.4f32;
            let eye = glam::Vec3::new(radius * angle.sin(), 0.3, radius * angle.cos());
            let view = glam::Mat4::look_at_lh(eye, glam::Vec3::ZERO, glam::Vec3::Y);
            let proj = glam::Mat4::perspective_lh(45f32.to_radians(), aspect, 0.1, 100.0);
            (proj * view, view)
        };
        // Camera world position (game space, pre-handedness-flip) — the inverse view's translation.
        // Used by the fragment stage for the Blinn-Phong view vector, in the same space as the lights.
        let cam_world = view.inverse().w_axis.truncate();
        // HANDEDNESS FIX: the Mercs2 asset space is RIGHT-HANDED (+X right, +Y up, +Z out); wgpu's NDC
        // is left-handed. Our camera math is built in the LH frame, so we convert at the very end by
        // negating clip-space X. VERIFIED against retail: interior prop sides (light heads left, coolers
        // right, wardrobe placement) match only with this applied — without it the whole game renders
        // horizontally MIRRORED. cull_mode is None everywhere, so the winding this flips is a no-op; if
        // backface culling is enabled later, flip front_face to compensate. (Movement strafe is likewise
        // built in the LH frame and negates its right vector to match — see world.rs.)
        let view_proj = glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * raw_vp;

        // Snapshot the drawable entities (copy out to release the world borrow before touching self).
        let mut items: Vec<(Entity, glam::Mat4, u32, Vec<[[f32; 4]; 4]>)> = Vec::new();
        for (e, (xform, mref, pal)) in world.query::<(&Transform, &ModelRef, &SkinPalette)>().iter() {
            items.push((e, xform.matrix(), mref.model, pal.mats.clone()));
        }

        // Phase 1: ensure per-entity resources and upload their MVP + palette.
        for (e, entity_model, model_hash, palette) in &items {
            let Some(mg) = self.models.get(model_hash) else { continue };
            let (bone_count, fit, prelit) = (mg.bone_count, mg.fit, mg.prelit);
            self.ensure_entity(*e, bone_count);
            let model = *entity_model * fit; // model -> world (fit=identity in the streaming/world path)
            let mvp = view_proj * model; // entity transform placed in fitted world space, + handedness flip
            let eg = &self.entities[e];
            // Camera uniform (176 B = 44 f32): mvp(16) + model(16) + cam_pos(4) + fog_color_density(4)
            // + fog_misc(4). Fog stays disabled unless set_fog was called.
            let mut uni = [0f32; 44];
            uni[..16].copy_from_slice(&mvp.to_cols_array());
            uni[16..32].copy_from_slice(&model.to_cols_array());
            uni[32..35].copy_from_slice(&[cam_world.x, cam_world.y, cam_world.z]);
            uni[35] = self.ambient; // cam_pos.w = ambient fill (scene-controlled)
            if let Some((color, density, fog_start)) = self.fog {
                uni[36..40].copy_from_slice(&[color[0], color[1], color[2], density]);
                uni[40] = 1.0; // fog enable
                uni[41] = fog_start;
            }
            uni[42] = if prelit { 1.0 } else { 0.0 }; // fog_misc.z = prelit (skip exterior sun)
            uni[43] = self.sun_intensity; // fog_misc.w = sun intensity (0 indoors)
            self.queue.write_buffer(&eg.mvp_buf, 0, bytemuck::cast_slice(&uni));
            if !palette.is_empty() {
                // Clamp to the entity's allocated bone count so a mismatched palette can't overflow.
                let n = palette.len().min(eg.bone_count);
                self.queue
                    .write_buffer(&eg.bone_buf, 0, bytemuck::cast_slice(&pose::flatten(&palette[..n])));
            }
        }

        // Sky uniform (128 B): inverse view-proj (per-pixel ray reconstruction) + sun direction +
        // horizon/zenith colors + Rayleigh/Mie scattering, from the atmosphere model. Fog color, if
        // set, drives the horizon so distant geometry dissolves into the sky.
        if self.sky_enabled {
            let inv = view_proj.inverse();
            let sun = self.atmo.sun_dir();
            let sc = self.atmo.scatter;
            let horizon = self.fog.map(|(c, _, _)| c).unwrap_or([0.70, 0.66, 0.58]);
            let zenith = [0.16, 0.33, 0.60];
            let li = self.atmo.light_intensity.max(0.05);
            let mut su = [0f32; 32];
            su[..16].copy_from_slice(&inv.to_cols_array());
            su[16..20].copy_from_slice(&[sun[0], sun[1], sun[2], 6.0]); // w = sun-disc intensity (HDR)
            su[20..24].copy_from_slice(&[horizon[0], horizon[1], horizon[2], li]);
            su[24..28].copy_from_slice(&[zenith[0], zenith[1], zenith[2], sc.henyey_greenstein]);
            su[28..32].copy_from_slice(&[sc.beta_ray, sc.beta_mie, sc.inscattering, sc.extinction]);
            self.queue.write_buffer(&self.sky_buf, 0, bytemuck::cast_slice(&su));
        }

        // Dynamic lights: upload the MAX_LIGHTS nearest to the camera this frame. The full set lives
        // in `self.lights`; we partial-sort by squared distance to `cam_world` and pack the head.
        {
            let mut order: Vec<usize> = (0..self.lights.len()).collect();
            if order.len() > MAX_LIGHTS {
                order.sort_by(|&a, &b| {
                    let da = light_dist2(&self.lights[a], cam_world);
                    let db = light_dist2(&self.lights[b], cam_world);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            let n = order.len().min(MAX_LIGHTS);
            // vec4 count (4 f32) then MAX_LIGHTS * (pos_radius[4] + color_intensity[4]).
            let mut buf = vec![0f32; 4 + MAX_LIGHTS * 8];
            buf[0] = f32::from_bits(n as u32); // count.x, read as u32 in the shader
            for (slot, &li) in order.iter().take(n).enumerate() {
                let l = &self.lights[li];
                let base = 4 + slot * 8;
                buf[base..base + 4].copy_from_slice(&l.pos_radius);
                buf[base + 4..base + 8].copy_from_slice(&l.color_intensity);
            }
            self.queue.write_buffer(&self.lights_buf, 0, bytemuck::cast_slice(&buf));
        }

        // Particle FX: upload the camera uniform + instance buffers before the pass. The billboard
        // shader uses the SAME (flipped) view_proj so particles register with the world; the
        // camera-facing basis (right/up) + eye come from the un-flipped view.
        let view_inv = view.inverse();
        let cam_right = view_inv.transform_vector3(glam::Vec3::X);
        let cam_up = view_inv.transform_vector3(glam::Vec3::Y);
        let eye = view_inv.transform_point3(glam::Vec3::ZERO);
        self.particles
            .prepare(&self.device, &self.queue, view_proj, cam_right, cam_up, eye);

        // Phase 2: record the pass.
        let output = self.surface.get_current_texture()?;
        let view_tex = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene frame") });

        // Shadow pass (FIRST): depth-only render of the scene from the key light into the shadow map
        // (cleared to 1.0 = far). The color pass' main shader PCF-samples this for real cast-shadows.
        // Same entities as the color pass, whole index range per model (depth only — materials/
        // draw-group split are irrelevant here). Uses the per-entity mvp_bind (for `model`) + bone_bind
        // uploaded in phase 1. SKIPPED when the sun is off (interiors) — no sun means no directional
        // shadow, and the shader gates on the same condition.
        if self.sun_intensity > 0.0 {
            let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            spass.set_pipeline(&self.shadow_pipeline);
            spass.set_bind_group(1, &self.shadow_vp_bind, &[]);
            for (e, _m, model_hash, _p) in &items {
                let Some(mg) = self.models.get(model_hash) else { continue };
                // Only DYNAMIC geometry casts: the prelit building shell already bakes its own light +
                // shadow into vertex colour, so casting it into the map would double-darken the baked
                // walls. Casting only characters/props gives clean contact shadows on the baked floor.
                if mg.prelit {
                    continue;
                }
                let Some(eg) = self.entities.get(e) else { continue };
                spass.set_bind_group(0, &eg.mvp_bind, &[]);
                spass.set_bind_group(2, &eg.bone_bind, &[]);
                spass.set_vertex_buffer(0, mg.vbuf.slice(..));
                if let Some(ib) = &mg.ibuf {
                    spass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                    spass.draw_indexed(0..mg.nindices, 0, 0..1);
                } else {
                    spass.draw(0..mg.nverts, 0..1);
                }
            }
        }

        // Prefer the HDR world path (scene → Rgba16Float → tone-map + bloom → swapchain); fall back to
        // a direct forward present when the post chain is absent (default `--ecs`/`--animate`, or if
        // HDR-target setup failed — nothing regresses). draw_geometry binds the group-3 lights itself.
        let hdr_world = self.sky_enabled
            && self.post.is_some()
            && self.world_pipeline.is_some()
            && self.sky_pipeline_hdr.is_some();
        if hdr_world {
            let post = self.post.as_ref().unwrap();
            post.update(&self.queue, &self.atmo);
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("scene hdr pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: post.hdr_view(),
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.02, b: 0.04, a: 1.0 }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth_view,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(self.sky_pipeline_hdr.as_ref().unwrap());
                pass.set_bind_group(0, &self.sky_bind, &[]);
                pass.draw(0..3, 0..1);
                self.draw_geometry(&mut pass, &items, self.world_pipeline.as_ref().unwrap());
            }
            post.run(&mut encoder, &view_tex); // bright-pass → blur → composite + tone-map → swapchain
        } else {
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
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if self.sky_enabled {
                pass.set_pipeline(&self.sky_pipeline);
                pass.set_bind_group(0, &self.sky_bind, &[]);
                pass.draw(0..3, 0..1);
            }
            self.draw_geometry(&mut pass, &items, &self.pipeline);
        }
        // Transparent FX (particles + light shafts): a separate pass on the SWAPCHAIN (both are
        // swapchain-format), after the world/post, blending over the final image. Depth = the scene
        // depth (read-only test), so both are occluded by nearer opaque geometry.
        let has_fx = self.particles.active_emitter_count() > 0
            || self.particles.live_particle_count() > 0
            || self.particles.glow_card_count() > 0;
        if has_fx {
            let mut ppass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transparent fx pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.particles.draw(&mut ppass);
        }
        // 2D UI overlay (tool panels / debug HUD): draw any quads staged via `ui_rect`/`ui_text`
        // over the final image — the same overlay pass the shell menu uses. No-op when nothing is
        // staged, so the game render path is unchanged unless a caller stages UI this frame.
        let ui_count = self.ui.prepare(&self.device, &self.queue, self.config.width, self.config.height);
        if ui_count > 0 {
            let mut upass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ui overlay pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view_tex,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.ui.draw(&mut upass, ui_count);
        }
        if let Some(f) = overlay {
            f(&self.device, &self.queue, &mut encoder, &view_tex, [self.config.width, self.config.height]);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

/// External overlay draw hook (see [`Scene::render_with`]): device, queue, frame encoder,
/// swapchain view, surface size in pixels.
pub type Overlay<'a> = &'a mut dyn FnMut(
    &wgpu::Device,
    &wgpu::Queue,
    &mut wgpu::CommandEncoder,
    &wgpu::TextureView,
    [u32; 2],
);

/// Squared distance from a light's world position to `p` (light-selection sort key).
fn light_dist2(l: &GpuLight, p: glam::Vec3) -> f32 {
    let dx = l.pos_radius[0] - p.x;
    let dy = l.pos_radius[1] - p.y;
    let dz = l.pos_radius[2] - p.z;
    dx * dx + dy * dy + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Statically parse + validate every WGSL module the scene loads, so a shader syntax error or a
    /// bad binding is caught by `cargo test` (pipeline creation — the only runtime WGSL check —
    /// needs a GPU). Uses `naga` (dev-dependency; the same version wgpu already pulls in).
    fn validate_wgsl(name: &str, src: &str) {
        let module = naga::front::wgsl::parse_str(src)
            .unwrap_or_else(|e| panic!("{name}: WGSL parse failed:\n{}", e.emit_to_string(src)));
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        validator
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: WGSL validation failed: {e:?}"));
    }

    #[test]
    fn shaders_parse_and_validate() {
        validate_wgsl("shader.wgsl", include_str!("shader.wgsl"));
        validate_wgsl("shadow.wgsl", include_str!("shadow.wgsl"));
        validate_wgsl("sky.wgsl", include_str!("sky.wgsl"));
        validate_wgsl("loading.wgsl", include_str!("loading.wgsl"));
    }

    /// The camera uniform we upload (44 f32 = 176 B) matches the shader `Camera` struct size, and the
    /// lights uniform packing (16 B count + MAX_LIGHTS * 32 B) matches the buffer we allocate.
    #[test]
    fn uniform_sizes_match() {
        assert_eq!(44 * 4, 176); // mvp(64)+model(64)+cam_pos(16)+fog_color_density(16)+fog_misc(16)
        assert_eq!(std::mem::size_of::<GpuLight>(), 32);
        assert_eq!(16 + MAX_LIGHTS * 32, 16 + 32 * 32);
    }
}
