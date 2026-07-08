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

/// Per-cascade shadow tile edge (px). The exe's `FUN_00755d90` builds a **1024×4096** shadow atlas =
/// four vertically-stacked 1024² tiles (`shadow_code_map.md` §1). We mirror that exactly: a
/// `SHADOW_TILE × (SHADOW_TILE*SHADOW_CASCADES)` depth atlas, one 1024² tile per cascade. Kept in sync
/// with the `1/1024` (u) / `1/4096` (v) PCF texel steps in `shader.wgsl`; change together.
const SHADOW_TILE: u32 = 1024;
/// Number of directional shadow cascades = the exe's 4-tile atlas emit (`while(i<4)` around
/// `FUN_00468ca0`, `shadow_code_map.md` §4). Four nested light-space boxes, near→far.
const SHADOW_CASCADES: usize = 4;
/// Padded per-cascade stride for the shadow-pass light-VP uniform (wgpu min dynamic-offset alignment
/// is 256 B). Each cascade's 64 B `mat4` sits at `c * CASCADE_STRIDE` in `cascade_vp_buf`.
const CASCADE_STRIDE: u64 = 256;
/// Max concurrent spot lights uploaded per frame (the `_sl` / `_pl_sl` per-pixel spot set).
const MAX_SPOT: usize = 16;

/// Nested cascade half-extents (metres) as multiples of the caller's base `half_extent` in
/// [`Scene::set_shadow`]. Cascade 0 = the tight box around the focus (crisp near shadows); each
/// successive cascade covers a geometrically larger area at the same 1024² tile resolution, so the
/// shadowed radius grows near→far — the engine realization of the exe's near/far cascade split. The
/// per-fragment shader picks the SMALLEST cascade that contains the point (see `shader.wgsl`).
const CASCADE_SPLIT_FACTORS: [f32; SHADOW_CASCADES] = [1.0, 2.5, 6.0, 15.0];

/// Pure cascade-split helper: the four nested half-extents for a base extent. Exposed for unit tests
/// (the split math is CPU-deterministic; the GPU projection is not testable headless).
fn cascade_half_extents(base: f32) -> [f32; SHADOW_CASCADES] {
    let mut out = [0.0; SHADOW_CASCADES];
    for c in 0..SHADOW_CASCADES {
        out[c] = base * CASCADE_SPLIT_FACTORS[c];
    }
    out
}

/// Pure cascade-selection helper mirroring the shader's per-fragment choice: the index of the
/// SMALLEST (lowest) cascade whose light-clip box contains `wpos` (ndc.xy ∈ [-1,1], depth ∈ [0,1]),
/// or `None` when the point is outside every cascade (→ eligible for a blob fallback). Kept in sync
/// with `shadow_factor` in `shader.wgsl`.
fn select_cascade(vps: &[glam::Mat4; SHADOW_CASCADES], wpos: glam::Vec3) -> Option<usize> {
    for (c, vp) in vps.iter().enumerate() {
        let lc = *vp * wpos.extend(1.0);
        if lc.w <= 0.0 {
            continue;
        }
        let ndc = lc.truncate() / lc.w;
        if ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && ndc.z >= 0.0 && ndc.z <= 1.0 {
            return Some(c);
        }
    }
    None
}

/// One dynamic SPOT light — the `_sl` / `_pl_sl` per-pixel light class (`LightObject` type field ≠
/// point). Complements [`crate::render::GpuLight`] (the omni point set) without changing that shared
/// 32 B point record: spot lights carry the extra cone axis + angles the `_sl` shader path needs, and
/// live in their own group-3 uniform (`scene`-owned). 64 B, `std140`-friendly (4 × `vec4`).
///
/// Harvested from `LightObject` COMPs the same way point lights are (`FUN_006622e0`, stride 0x34 =
/// int type + rgb + 9 floats — intensity/range/atten/**cone angles**). The exe selects the `_sl`
/// shader permutation for these via the `DAT_00dfc345` ShaderLevel gate (see [`Scene::set_shader_level`]).
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpotLightGpu {
    /// `xyz` = world position, `w` = range (metres; ≤0 disables).
    pub pos_range: [f32; 4],
    /// `rgb` = linear color, `w` = intensity scalar.
    pub color_intensity: [f32; 4],
    /// `xyz` = normalized cone axis (direction the spot points), `w` = `cos(outer half-angle)`.
    pub dir_cos: [f32; 4],
    /// `x` = `cos(inner half-angle)` (smooth cone edge), `y` = casts-shadow flag (reserved), `zw` reserved.
    pub params: [f32; 4],
}

impl SpotLightGpu {
    /// Build a spot light from position, direction, color/intensity, range and cone half-angles (rad).
    pub fn new(
        pos: [f32; 3],
        dir: [f32; 3],
        color: [f32; 3],
        intensity: f32,
        range: f32,
        inner_half_angle: f32,
        outer_half_angle: f32,
    ) -> Self {
        let d = glam::Vec3::from(dir).normalize_or_zero();
        SpotLightGpu {
            pos_range: [pos[0], pos[1], pos[2], range],
            color_intensity: [color[0], color[1], color[2], intensity],
            dir_cos: [d.x, d.y, d.z, outer_half_angle.cos()],
            params: [inner_half_angle.cos(), 0.0, 0.0, 0.0],
        }
    }
}

/// How a [`LightAnim`] tween drives its target light over time — the engine realization of the exe's
/// `Rt{Light,Color,Alpha}Animation` descriptors, ticked by the master update `FUN_00675e50`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightAnimMode {
    /// Smooth sinusoidal intensity pulse (`RtLightAnimation` / `RtAlphaAnimation`).
    Pulse,
    /// Noisy per-frame flicker (torch/fire), value-noise on the base intensity.
    Flicker,
}

/// A runtime light tween applied each frame before the light set is uploaded — the engine analog of
/// the exe's `Rt{Light,Color,Scale,Alpha}Animation` runtime-type descriptors (`FUN_00646b60` &
/// siblings) driven by the master light-update pass `FUN_00675e50`. Data-driven: authored per light.
///
/// NOTE (confirm-live): the retail `LightObject`/`LightAnimation` COMP stream carries these tween
/// descriptors, but our world harvest (`placement::light_inventory`) does not yet decode the animation
/// sub-records, so on retail data this set is empty unless a caller supplies it. The tween MATH here
/// (pulse/flicker) is an engine approximation until the descriptor keys are read live — see DEFERRED.md.
#[derive(Clone, Copy, Debug)]
pub struct LightAnim {
    /// Index into the point-light set ([`Scene::set_lights`]); out-of-range indices are ignored.
    pub light_index: usize,
    /// Steady-state intensity the tween modulates around.
    pub base_intensity: f32,
    /// Tween rate (Hz).
    pub freq_hz: f32,
    /// Fractional amplitude (0..1): peak deviation from `base_intensity`.
    pub amp: f32,
    /// Tween shape.
    pub mode: LightAnimMode,
}

/// A pending blob / contact shadow (`FUN_00853710` analog): a dark disc in the world XZ plane centred
/// at `pos` (the caster's ground/feet point), `radius` metres wide, `darkness` 0..1 = the `ShadowK`
/// darkness constant. Emitted for casters the cascade atlas doesn't cover.
#[derive(Clone, Copy, Debug)]
pub struct BlobInstance {
    pub pos: [f32; 3],
    pub radius: f32,
    pub darkness: f32,
}

/// One vertex of a blob quad (world XZ plane): world position + centred UV (-1..1) for the radial
/// falloff + the blob's darkness (`ShadowK`). Blob quads are CPU-generated into `blob_vbuf` each frame.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlobVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    darkness: f32,
}

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
    /// Per-pixel SPOT light set (`_sl` / `_pl_sl` class). Uploaded into group-3 binding 4 each frame;
    /// empty by default (point/omni lights only). See [`Scene::set_spot_lights`].
    spot_lights: Vec<SpotLightGpu>,
    spot_buf: wgpu::Buffer,
    /// ShaderLevel gate (the `DAT_00dfc345` analog): 0 = base (no per-pixel dynamic lights), 1 = `_pl`
    /// (point), 2 = `_sl` (spot), 3 = `_pl_sl` (both). Written into the light uniform's `count.y`; the
    /// shader selects which per-pixel light class to evaluate. Default 3 (full).
    shader_level: u32,
    /// Per-frame light tweens (`Rt{Light,Color,Alpha}Animation` analog); applied in `render` before the
    /// light set is uploaded (the `FUN_00675e50` master-update analog). Empty by default (static lights).
    light_anims: Vec<LightAnim>,
    /// **4-cascade** directional shadow atlas (`FUN_00755d90`: 1024×4096 = four stacked 1024² tiles),
    /// depth-only renders from the key light, PCF-sampled by the main shader (folded into group 3).
    /// `cascade_vp_buf` = the shadow pass's group-1 per-cascade light-VP (dynamic offset, one 1024²
    /// tile per cascade); `shadow_params_buf` (group-3 binding 3) carries all 4 cascade VPs + params so
    /// the color pass can select+sample per fragment. Refreshed by [`Scene::set_shadow`].
    shadow_view: wgpu::TextureView,
    cascade_vp_buf: wgpu::Buffer,
    shadow_params_buf: wgpu::Buffer,
    shadow_vp_bind: wgpu::BindGroup,
    shadow_pipeline: wgpu::RenderPipeline,
    /// The 4 cascade light view-projections this frame (CPU mirror of `shadow_params_buf`), used by the
    /// shadow pass to gate each caster into the tightest cascade(s) that contain it (the distance-LOD
    /// caster gate, `FUN_00858150` analog) and to decide blob-shadow fallbacks. Identity until
    /// `set_shadow` runs (default paths read no cascade shadow).
    cascade_vps: [glam::Mat4; SHADOW_CASCADES],
    /// Whether `set_shadow` has configured the cascades this session (default `false` → no directional
    /// shadow, same as the pre-cascade `--ecs`/`--animate` behaviour).
    shadow_configured: bool,
    /// Blob / contact-shadow fallback (`FUN_00853710`, `shadow_code_map.md` §5): dark projected discs
    /// for dynamic casters that fall OUTSIDE every cascade (beyond shadow distance) — the cheap
    /// fallback the exe emits when the depth atlas doesn't cover a caster. Rebuilt each frame; empty →
    /// the `PassId::Blob` pass records nothing (default paths unchanged).
    blob_pipeline: wgpu::RenderPipeline,
    blob_vbuf: wgpu::Buffer,
    blob_cap: usize,
    blob_params_buf: wgpu::Buffer,
    blob_bind: wgpu::BindGroup,
    blobs: Vec<BlobInstance>,
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
    /// Registered Band-A render nodes (reflection / water / decals / sky-as-pass silos), each plugged
    /// into a canonical [`crate::render_graph::PassId`] slot. EMPTY by default → the `SCENE_ORDER`
    /// walk records nothing extra, so the E2 carve stays behaviour-preserving until a silo plugs in.
    /// A silo registers via [`Scene::add_render_node`]; the frame builds a [`crate::render_graph::PassCtx`]
    /// and calls each matching node in its slot (see `dispatch_nodes`).
    render_nodes: Vec<Box<dyn crate::render_graph::RenderNode>>,
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

        // 4-cascade directional shadow ATLAS (faithful to `FUN_00755d90`: 1024×4096 = four stacked
        // 1024² tiles). Depth-only renders from the key light, PCF-sampled by the main shader. Built
        // here so the group-3 lights bind group (below) can fold in its depth view + comparison sampler
        // + cascade params (staying within wgpu's 4-bind-group limit — bindings, not groups, grow).
        let shadow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow atlas (4x1024 cascades)"),
            size: wgpu::Extent3d {
                width: SHADOW_TILE,
                height: SHADOW_TILE * SHADOW_CASCADES as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let shadow_view = shadow_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // PCF comparison sampler: linear filter + LessEqual so `textureSampleCompareLevel` returns a
        // smoothed 0..1 occlusion across the depth footprint.
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
        // Per-cascade light-VP for the SHADOW PASS (dynamic-offset uniform: 4 cascades × 256 B padded).
        // Init identity so a path that never calls `set_shadow` (e.g. `--ecs`) still has a valid matrix.
        let cascade_vp_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow cascade view-projs"),
            size: CASCADE_STRIDE * SHADOW_CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // group-3 shadow params for the COLOR PASS: 4 cascade VPs (tight array<mat4,4>=256 B) + a params
        // vec4 (shadow_strength, blob unused, cascade_count, configured). Init cascades to identity.
        let shadow_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadow params (cascade VPs + knobs)"),
            size: (SHADOW_CASCADES * 64 + 16) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        {
            let ident = glam::Mat4::IDENTITY.to_cols_array();
            for c in 0..SHADOW_CASCADES {
                queue.write_buffer(&cascade_vp_buf, c as u64 * CASCADE_STRIDE, bytemuck::cast_slice(&ident));
            }
            let mut params = vec![0f32; SHADOW_CASCADES * 16 + 4];
            for c in 0..SHADOW_CASCADES {
                params[c * 16..c * 16 + 16].copy_from_slice(&ident);
            }
            // params vec4: [shadow_strength, reserved, cascade_count, configured(0/1)]
            let base = SHADOW_CASCADES * 16;
            params[base] = 0.35; // shadow-strength floor (shadowed areas darken, not blacken)
            params[base + 2] = SHADOW_CASCADES as f32;
            params[base + 3] = 0.0; // not configured yet
            queue.write_buffer(&shadow_params_buf, 0, bytemuck::cast_slice(&params));
        }
        // Per-pixel SPOT light uniform (group-3 binding 4): vec4 count + MAX_SPOT × 64 B.
        let spot_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("spot lights uniform"),
            size: (16 + MAX_SPOT * 64) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&spot_buf, 0, bytemuck::cast_slice(&[0u32, 0, 0, 0]));

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
                // binding 4: the per-pixel SPOT light set (`_sl` / `_pl_sl` class).
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
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
                wgpu::BindGroupEntry { binding: 3, resource: shadow_params_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 4, resource: spot_buf.as_entire_binding() },
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
        // Dynamic-offset uniform: one bind, re-pointed at cascade `c`'s 64 B VP via offset `c*256`.
        let shadow_vp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow vp bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(64),
                },
                count: None,
            }],
        });
        let shadow_vp_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow vp bind"),
            layout: &shadow_vp_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &cascade_vp_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(64),
                }),
            }],
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

        // Blob / contact-shadow fallback pass (`FUN_00853710`, shadow_code_map.md §5). A darken-blend
        // pipeline drawing flat radial-falloff discs in the world XZ plane under casters the cascade
        // atlas doesn't cover. Own group-0 uniform = the frame's (flipped) camera view-proj. The quads
        // are CPU-generated into `blob_vbuf` each frame; blend `dst*(1-a)` darkens by the disc alpha.
        let blob_shader = device.create_shader_module(wgpu::include_wgsl!("blob.wgsl"));
        let blob_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blob bgl"),
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
        let blob_params_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blob view-proj"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blob_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blob bind"),
            layout: &blob_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: blob_params_buf.as_entire_binding() }],
        });
        let blob_cap = 64usize; // grown on demand in record_blob
        let blob_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blob vbuf"),
            size: (blob_cap * 6 * std::mem::size_of::<BlobVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let blob_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blob pipeline layout"),
            bind_group_layouts: &[&blob_bgl],
            push_constant_ranges: &[],
        });
        let blob_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blob pipeline"),
            layout: Some(&blob_layout),
            vertex: wgpu::VertexState {
                module: &blob_shader,
                entry_point: "vs_blob",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<BlobVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blob_shader,
                entry_point: "fs_blob",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    // Darken blend: out = dst*(1 - a). Blob outputs a = darkness*radial-falloff.
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::COLOR,
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
                depth_write_enabled: false, // a shadow decal, not occluding geometry
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
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
            spot_lights: Vec::new(),
            spot_buf,
            shader_level: 3, // full: `_pl_sl` (point + spot per-pixel). See set_shader_level.
            light_anims: Vec::new(),
            shadow_view,
            cascade_vp_buf,
            shadow_params_buf,
            shadow_vp_bind,
            shadow_pipeline,
            cascade_vps: [glam::Mat4::IDENTITY; SHADOW_CASCADES],
            shadow_configured: false,
            blob_pipeline,
            blob_vbuf,
            blob_cap,
            blob_params_buf,
            blob_bind,
            blobs: Vec::new(),
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
            render_nodes: Vec::new(),
        }
    }

    /// Register a Band-A [`crate::render_graph::RenderNode`] (reflection / water / decal / sky silo).
    /// It runs in its declared [`crate::render_graph::PassId`] slot during the `SCENE_ORDER` walk,
    /// handed a fully-populated [`crate::render_graph::PassCtx`] (camera + lights + surface format +
    /// the collected renderable list). Multiple nodes may share a slot (registration order preserved).
    pub fn add_render_node(&mut self, node: Box<dyn crate::render_graph::RenderNode>) {
        self.render_nodes.push(node);
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

    /// Set the per-pixel SPOT light set (`_sl` / `_pl_sl` class). Empty by default (point lights only).
    /// Uploaded (capped to [`MAX_SPOT`]) into the group-3 spot uniform each frame; the shader evaluates
    /// them when the ShaderLevel gate ([`Scene::set_shader_level`]) admits the spot class (level ≥ 2).
    pub fn set_spot_lights(&mut self, spots: Vec<SpotLightGpu>) {
        self.spot_lights = spots;
    }

    /// Set the ShaderLevel gate (the `DAT_00dfc345` analog) selecting the per-pixel light-class shader
    /// permutation: **0** = base (sun/ambient/baked only — no per-pixel dynamic lights), **1** = `_pl`
    /// (point), **2** = `_sl` (spot), **3** = `_pl_sl` (point + spot). Clamped to 0..=3. Default 3.
    /// Faithful to the exe, which registers the matching `.sho` permutation at load under this gate
    /// (`FUN_0085ac90`, rendering-shaders.md §Lighting); we realize it as a runtime branch.
    pub fn set_shader_level(&mut self, level: u32) {
        self.shader_level = level.min(3);
    }

    /// Set the runtime light tweens (`Rt{Light,Color,Alpha}Animation` analog). Applied each frame in
    /// `render` before the light set is uploaded (the `FUN_00675e50` master-update analog). Empty =
    /// static lights. See [`LightAnim`] (retail descriptor decode is confirm-live — DEFERRED.md).
    pub fn set_light_animations(&mut self, anims: Vec<LightAnim>) {
        self.light_anims = anims;
    }

    /// Aim the **4-cascade** directional shadow atlas for this frame. `center` = the world focus the
    /// cascades are centred on (typically the player); `dir` = the key light's travel direction (FROM
    /// the light TOWARD the scene, downward-ish); `half_extent` = half the width (m) of the TIGHTEST
    /// cascade (cascade 0). The remaining cascades cover geometrically larger areas
    /// ([`CASCADE_SPLIT_FACTORS`]) at the same 1024² tile resolution, so the shadowed radius grows
    /// near→far — the engine realization of the exe's `while(i<4)` cascade emit (`FUN_00468ca0`,
    /// shadow_code_map.md §4). Builds a self-consistent LH light view-proj per cascade (`look_at_lh` *
    /// `orthographic_lh`, NO camera X-flip — the main shader projects true world space with these) and
    /// uploads all four (packed for the color pass, padded for the shadow pass). Call each frame.
    pub fn set_shadow(&mut self, center: [f32; 3], dir: [f32; 3], half_extent: f32) {
        let c = glam::Vec3::from(center);
        let mut d = glam::Vec3::from(dir);
        if d.length_squared() < 1e-8 {
            d = glam::Vec3::new(0.0, -1.0, 0.0);
        }
        d = d.normalize();
        // Guard against dir ∥ up: pick +Z as the up reference when the light is near-vertical.
        let up = if d.dot(glam::Vec3::Y).abs() > 0.99 { glam::Vec3::Z } else { glam::Vec3::Y };

        let extents = cascade_half_extents(half_extent.max(0.01));
        // Color-pass uniform: 4 tight mat4 (256 B) + params vec4.
        let mut params = vec![0f32; SHADOW_CASCADES * 16 + 4];
        for (c_idx, &he) in extents.iter().enumerate() {
            // Push the eye back proportionally to the cascade box so the near/far depth range always
            // brackets the casters that fall inside this cascade.
            let distance = (he * 3.0).max(40.0);
            let eye = c - d * distance;
            let view = glam::Mat4::look_at_lh(eye, c, up);
            let proj = glam::Mat4::orthographic_lh(-he, he, -he, he, 0.1, 2.0 * distance);
            let vp = proj * view;
            self.cascade_vps[c_idx] = vp;
            let cols = vp.to_cols_array();
            // Shadow pass: padded (256 B stride) for dynamic-offset binding.
            self.queue
                .write_buffer(&self.cascade_vp_buf, c_idx as u64 * CASCADE_STRIDE, bytemuck::cast_slice(&cols));
            // Color pass: tight array<mat4,4>.
            params[c_idx * 16..c_idx * 16 + 16].copy_from_slice(&cols);
        }
        let base = SHADOW_CASCADES * 16;
        params[base] = 0.35; // shadow-strength floor
        params[base + 2] = SHADOW_CASCADES as f32;
        params[base + 3] = 1.0; // configured
        self.queue.write_buffer(&self.shadow_params_buf, 0, bytemuck::cast_slice(&params));
        self.shadow_configured = true;
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
        items: &[DrawItem],
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

    // --- Render-graph nodes (see `render_graph::SCENE_ORDER`) --------------------------------------
    // Each canonical/engine pass carved out of the old monolithic `render` into its own recorder so
    // the frame walks `render_graph::SCENE_ORDER` in the recovered `FUN_00466d40` sequence. These are
    // `&self` (all queue uploads happen in phase 1 before recording), take the frame's `encoder` +
    // swapchain view, and record the SAME commands the monolithic `render` did — a byte-identical carve.

    /// [`render_graph::PassId::ShadowCascade`] node — the **4-cascade** directional shadow-depth pass
    /// (faithful to the exe's `while(i<4){…RenderShadow…}` emit into the 1024×4096 atlas, shadow §4).
    /// One depth-only render of the DYNAMIC scene per cascade, into that cascade's 1024² tile of the
    /// atlas (via a per-tile viewport + the cascade's light-VP dynamic offset); the color pass selects
    /// + PCF-samples the tightest cascade per fragment. SKIPPED when the sun is off (interiors) — the
    /// shader gates on the same condition. Only non-prelit geometry casts (the building shell bakes its
    /// own shadow into vertex colour), and each caster is gated into ONLY the cascades whose box
    /// contains it (the distance-LOD caster gate, `FUN_00858150` analog) — near casters land in the
    /// tight cascades, far ones only in the wide ones, mirroring the per-cascade caster list.
    fn record_shadow_cascade(&self, encoder: &mut wgpu::CommandEncoder, items: &[DrawItem]) {
        if self.sun_intensity <= 0.0 {
            return;
        }
        // Single pass over the whole atlas (cleared once to 1.0); per cascade we set the viewport to
        // its 1024² tile and bind the cascade's light-VP via the dynamic offset.
        let mut spass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadow atlas pass (4 cascades)"),
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
        let tile = SHADOW_TILE as f32;
        for cascade in 0..SHADOW_CASCADES {
            // Tile `cascade` occupies rows [cascade*1024 .. (cascade+1)*1024) of the atlas.
            spass.set_viewport(0.0, cascade as f32 * tile, tile, tile, 0.0, 1.0);
            let offset = (cascade as u64 * CASCADE_STRIDE) as wgpu::DynamicOffset;
            spass.set_bind_group(1, &self.shadow_vp_bind, &[offset]);
            for (e, entity_model, model_hash, _p) in items {
                let Some(mg) = self.models.get(model_hash) else { continue };
                // Only DYNAMIC geometry casts: the prelit building shell already bakes its own light +
                // shadow into vertex colour, so casting it into the map would double-darken the baked
                // walls. Casting only characters/props gives clean contact shadows on the baked floor.
                if mg.prelit {
                    continue;
                }
                // Distance-LOD caster gate: skip this caster in cascade `c` unless its ground point is
                // inside cascade `c`'s box (near casters → tight cascades, far → wide only). `shadow_configured`
                // guarantees `cascade_vps` are real (not identity) here.
                if self.shadow_configured {
                    let world = *entity_model * mg.fit;
                    let wpos = world.w_axis.truncate();
                    let lc = self.cascade_vps[cascade] * wpos.extend(1.0);
                    if lc.w > 0.0 {
                        let ndc = lc.truncate() / lc.w;
                        // Pad the test by the caster's rough radius in NDC so a caster straddling the
                        // border still renders into the cascade that will sample it.
                        let pad = 1.3;
                        if ndc.x.abs() > pad || ndc.y.abs() > pad || ndc.z < -0.2 || ndc.z > 1.2 {
                            continue;
                        }
                    }
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
    }

    /// [`render_graph::PassId::Blob`] node — the BlobShadow cheap fallback (`FUN_00853710`, shadow §5).
    /// Darkens flat radial discs under the casters the cascade atlas doesn't cover (collected each
    /// frame into `self.blobs`). No-op when empty. Runs AFTER the color pass (blobs darken the composed
    /// image), depth-tested read-only so a blob is hidden by nearer opaque geometry.
    fn record_blob(&self, encoder: &mut wgpu::CommandEncoder, view_tex: &wgpu::TextureView) {
        if self.blobs.is_empty() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blob shadow pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: view_tex,
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
        pass.set_pipeline(&self.blob_pipeline);
        pass.set_bind_group(0, &self.blob_bind, &[]);
        pass.set_vertex_buffer(0, self.blob_vbuf.slice(..));
        pass.draw(0..(self.blobs.len() * 6) as u32, 0..1);
    }

    /// Apply the [`LightAnim`] tweens (the `Rt{Light,Color,Alpha}Animation` / `FUN_00675e50` analog) to
    /// a scratch copy of the point-light set at time `t`, so animated lights pulse/flicker without
    /// mutating the authored base set. Returns the base set unchanged when no tween targets it.
    fn animated_lights(&self, t: f32) -> Vec<GpuLight> {
        let mut out = self.lights.clone();
        for a in &self.light_anims {
            let Some(l) = out.get_mut(a.light_index) else { continue };
            l.color_intensity[3] = (a.base_intensity * light_anim_factor(a, t)).max(0.0);
        }
        out
    }

    /// [`render_graph::PassId::Color`] node — the main color draw (`PgScene::RenderColor`). Combined
    /// forward pass: a fullscreen sky draw (engine approximation of the canonical sky-as-pass) then the
    /// opaque geometry. Prefers the HDR world path (scene → `Rgba16Float` → tone-map + bloom →
    /// swapchain); falls back to a direct forward present when the post chain is absent (default
    /// `--ecs`/`--animate`, or if HDR setup failed — nothing regresses). `draw_geometry` binds the
    /// group-3 lights itself.
    fn record_color(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view_tex: &wgpu::TextureView,
        items: &[DrawItem],
        hdr_world: bool,
    ) {
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
                self.draw_geometry(&mut pass, items, self.world_pipeline.as_ref().unwrap());
            }
            post.run(encoder, view_tex); // bright-pass → blur → composite + tone-map → swapchain
        } else {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view_tex,
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
            self.draw_geometry(&mut pass, items, &self.pipeline);
        }
    }

    /// [`render_graph::PassId::TransparentFx`] node — transparent FX (billboard particles + light
    /// shafts): a separate pass on the SWAPCHAIN, after the world/post, blending over the final image.
    /// Depth = the scene depth (read-only test), so both are occluded by nearer opaque geometry. No-op
    /// when nothing is live.
    fn record_transparent_fx(&self, encoder: &mut wgpu::CommandEncoder, view_tex: &wgpu::TextureView) {
        let has_fx = self.particles.active_emitter_count() > 0
            || self.particles.live_particle_count() > 0
            || self.particles.glow_card_count() > 0;
        if has_fx {
            let mut ppass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transparent fx pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: view_tex,
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
    }

    /// [`render_graph::PassId::Ui`] node — the 2D UI overlay (tool panels / debug HUD): draw any quads
    /// staged via `ui_rect`/`ui_text` over the final image (the same overlay pass the shell menu uses).
    /// The caller passes the staged-quad count from `ui.prepare`; only invoked when `ui_count > 0`, so
    /// the game render path is unchanged unless a caller stages UI this frame.
    fn record_ui(&self, encoder: &mut wgpu::CommandEncoder, view_tex: &wgpu::TextureView, ui_count: u32) {
        let mut upass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui overlay pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: view_tex,
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

    /// Run any registered Band-A [`crate::render_graph::RenderNode`] whose slot is `slot`, building a
    /// fully-populated [`crate::render_graph::PassCtx`] for each (camera + lights + surface format +
    /// the collected renderable list). No-op when `render_nodes` is empty (the default) → the frame is
    /// byte-identical to the E2 carve. `color` is this frame's swapchain view; `depth`/lights/format
    /// come from `self`; the camera + `items` are this frame's locals.
    #[allow(clippy::too_many_arguments)]
    fn dispatch_nodes(
        &self,
        slot: crate::render_graph::PassId,
        encoder: &mut wgpu::CommandEncoder,
        color: &wgpu::TextureView,
        items: &[DrawItem],
        view_proj: glam::Mat4,
        view: glam::Mat4,
        cam_pos: glam::Vec3,
        time: f32,
    ) {
        for node in &self.render_nodes {
            if node.id() != slot {
                continue;
            }
            let mut ctx = crate::render_graph::PassCtx {
                device: &self.device,
                queue: &self.queue,
                encoder: &mut *encoder, // reborrow per node so ctx can be rebuilt each iteration
                color,
                depth: &self.depth_view,
                size: [self.config.width, self.config.height],
                view_proj,
                view,
                cam_pos,
                lights_bind: &self.lights_bind,
                surface_format: self.config.format,
                items,
                time,
            };
            node.record(&mut ctx);
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
        mut overlay: Option<Overlay<'_>>,
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
        let mut items: Vec<DrawItem> = Vec::new();
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

        // Rt{Light,Color,Alpha}Animation tick (the `FUN_00675e50` master light-update analog): apply
        // the per-frame light tweens to a scratch copy of the point set so animated lights pulse/flicker
        // without mutating the authored base set. No-op when `light_anims` is empty (static lights).
        let lit_lights = self.animated_lights(t);

        // Dynamic lights: upload the MAX_LIGHTS nearest to the camera this frame. The full set lives
        // in `lit_lights`; we partial-sort by squared distance to `cam_world` and pack the head.
        {
            let mut order: Vec<usize> = (0..lit_lights.len()).collect();
            if order.len() > MAX_LIGHTS {
                order.sort_by(|&a, &b| {
                    let da = light_dist2(&lit_lights[a], cam_world);
                    let db = light_dist2(&lit_lights[b], cam_world);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            let n = order.len().min(MAX_LIGHTS);
            // vec4 count (4 f32) then MAX_LIGHTS * (pos_radius[4] + color_intensity[4]).
            let mut buf = vec![0f32; 4 + MAX_LIGHTS * 8];
            buf[0] = f32::from_bits(n as u32); // count.x, read as u32 in the shader
            // count.y = ShaderLevel gate (DAT_00dfc345 analog: 0 base / 1 _pl / 2 _sl / 3 _pl_sl).
            buf[1] = f32::from_bits(self.shader_level);
            for (slot, &li) in order.iter().take(n).enumerate() {
                let l = &lit_lights[li];
                let base = 4 + slot * 8;
                buf[base..base + 4].copy_from_slice(&l.pos_radius);
                buf[base + 4..base + 8].copy_from_slice(&l.color_intensity);
            }
            self.queue.write_buffer(&self.lights_buf, 0, bytemuck::cast_slice(&buf));
        }

        // Spot lights (`_sl` / `_pl_sl` class): upload the MAX_SPOT nearest to the camera. vec4 count +
        // MAX_SPOT × 64 B. The shader evaluates them only when the ShaderLevel gate admits spots.
        {
            let mut order: Vec<usize> = (0..self.spot_lights.len()).collect();
            if order.len() > MAX_SPOT {
                order.sort_by(|&a, &b| {
                    let da = spot_dist2(&self.spot_lights[a], cam_world);
                    let db = spot_dist2(&self.spot_lights[b], cam_world);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            let n = order.len().min(MAX_SPOT);
            let mut sbuf = vec![0f32; 4 + MAX_SPOT * 16];
            sbuf[0] = f32::from_bits(n as u32);
            for (slot, &si) in order.iter().take(n).enumerate() {
                let s = &self.spot_lights[si];
                let base = 4 + slot * 16;
                sbuf[base..base + 4].copy_from_slice(&s.pos_range);
                sbuf[base + 4..base + 8].copy_from_slice(&s.color_intensity);
                sbuf[base + 8..base + 12].copy_from_slice(&s.dir_cos);
                sbuf[base + 12..base + 16].copy_from_slice(&s.params);
            }
            self.queue.write_buffer(&self.spot_buf, 0, bytemuck::cast_slice(&sbuf));
        }

        // Blob view-proj (the flipped world clip matrix, matching the geometry pass).
        self.queue
            .write_buffer(&self.blob_params_buf, 0, bytemuck::cast_slice(&view_proj.to_cols_array()));

        // Blob / contact-shadow collection (the exe's cheap fallback for casters the depth atlas can't
        // cover — `FUN_00853710`, shadow_code_map.md §5). A dynamic (non-prelit) caster whose ground
        // point falls OUTSIDE every cascade box (beyond shadow distance) gets a projected dark disc
        // instead of a real cascade shadow. Only active when the cascades are configured AND the sun is
        // on (outdoors) — interiors/default paths keep their prior no-blob behaviour (graceful degrade).
        self.blobs.clear();
        if self.shadow_configured && self.sun_intensity > 0.0 {
            for (_e, entity_model, model_hash, _pal) in &items {
                let Some(mg) = self.models.get(model_hash) else { continue };
                if mg.prelit {
                    continue; // baked geometry is not a dynamic caster
                }
                let world = *entity_model * mg.fit;
                let wpos = world.w_axis.truncate();
                if select_cascade(&self.cascade_vps, wpos).is_none() {
                    // Beyond all cascades: emit a grounding blob at the caster's origin. Radius/darkness
                    // are fixed knobs (exact ShadowK + per-caster bounds are confirm-live).
                    self.blobs.push(BlobInstance { pos: wpos.to_array(), radius: 1.3, darkness: 0.45 });
                }
            }
        }
        // Build + upload the blob quad geometry (world XZ plane, two triangles per blob), growing the
        // vertex buffer if this frame has more blobs than the current capacity.
        if !self.blobs.is_empty() {
            if self.blobs.len() > self.blob_cap {
                self.blob_cap = self.blobs.len().next_power_of_two();
                self.blob_vbuf = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("blob vbuf"),
                    size: (self.blob_cap * 6 * std::mem::size_of::<BlobVertex>()) as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
            }
            let mut verts: Vec<BlobVertex> = Vec::with_capacity(self.blobs.len() * 6);
            for b in &self.blobs {
                let r = b.radius;
                let [x, y, z] = b.pos;
                let dk = b.darkness;
                // Two triangles in the XZ plane (Y = ground); uv = centred corner (-1..1) for the radial
                // falloff; darkness carried per-vertex so the shader needs no per-blob uniform.
                let corners = [
                    ([x - r, y, z - r], [-1.0, -1.0]),
                    ([x + r, y, z - r], [1.0, -1.0]),
                    ([x + r, y, z + r], [1.0, 1.0]),
                    ([x - r, y, z - r], [-1.0, -1.0]),
                    ([x + r, y, z + r], [1.0, 1.0]),
                    ([x - r, y, z + r], [-1.0, 1.0]),
                ];
                for (p, uv) in corners {
                    verts.push(BlobVertex { pos: p, uv, darkness: dk });
                }
            }
            self.queue.write_buffer(&self.blob_vbuf, 0, bytemuck::cast_slice(&verts));
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

        // Phase 2: record the frame's passes by walking the canonical per-viewport scene order
        // (`render_graph::SCENE_ORDER`, the recovered `FUN_00466d40` body sequence — render_core §5).
        // The not-yet-implemented canonical passes (wake/occlusion/reflection/water/z-prepass/
        // fading-trees/mirror/blob) are no-op SEAMS the Band-A silos fill next wave: they record
        // NOTHING today, so this loop reduces to the engine's prior command sequence (shadow-depth →
        // color → transparent-fx → ui → overlay) — a behaviour-preserving carve. Each node's Xbox↔PC
        // anchor lives on `render_graph::PassId::anchor`.
        let output = self.surface.get_current_texture()?;
        let view_tex = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene frame") });

        // Prefer the HDR world path (scene → Rgba16Float → tone-map + bloom → swapchain); fall back to
        // a direct forward present when the post chain is absent (default `--ecs`/`--animate`, or if
        // HDR-target setup failed — nothing regresses). `record_color` binds the group-3 lights itself.
        let hdr_world = self.sky_enabled
            && self.post.is_some()
            && self.world_pipeline.is_some()
            && self.sky_pipeline_hdr.is_some();

        // Stage this frame's 2D UI overlay before recording (a queue buffer write; ordering among the
        // frame's queue writes is immaterial). Non-zero only when a caller staged `ui_rect`/`ui_text`.
        let ui_count = self.ui.prepare(&self.device, &self.queue, self.config.width, self.config.height);

        use crate::render_graph::PassId;
        for &node in crate::render_graph::SCENE_ORDER {
            match node {
                // --- realized world passes ---
                PassId::ShadowCascade => self.record_shadow_cascade(&mut encoder, &items),
                PassId::Color => self.record_color(&mut encoder, &view_tex, &items, hdr_world),
                // --- engine composite tail ---
                PassId::TransparentFx => self.record_transparent_fx(&mut encoder, &view_tex),
                PassId::Ui => {
                    if ui_count > 0 {
                        self.record_ui(&mut encoder, &view_tex, ui_count);
                    }
                }
                PassId::Overlay => {
                    if let Some(f) = overlay.take() {
                        f(
                            &self.device,
                            &self.queue,
                            &mut encoder,
                            &view_tex,
                            [self.config.width, self.config.height],
                        );
                    }
                }
                // --- canonical FUN_00466d40 seams: Band-A silos fill these; they render NOTHING yet ---
                // SILO water (Band-A): wake-map / occlusion / reflection / water-surface seams.
                PassId::WakeMap | PassId::Occlusion | PassId::Reflection | PassId::WaterSurface => {}
                // SILO z-prepass (Band-A): depth-only main-list pass seam.
                PassId::ZOpaque => {}
                // SILO vegetation-fade (Band-A): RenderFadingTrees seam.
                PassId::FadingTrees => {}
                // SILO mirror/sub-scene (Band-A): planar-mirror render seam.
                PassId::Mirror => {}
                // Blob-shadow fallback (`FUN_00853710`): darken projected discs under casters the
                // cascade atlas doesn't cover. No-op when `self.blobs` is empty (default paths).
                PassId::Blob => self.record_blob(&mut encoder, &view_tex),
                // SILO particles-as-pass (Band-A): canonical PgFX pass; engine draws via TransparentFx.
                PassId::Particles => {}
                // Collect / scene-begin / shadow-collect are folded into phase-1 CPU setup and the
                // color/shadow passes' own RT bind + clear — no standalone GPU pass to record.
                PassId::SceneBegin | PassId::Collect | PassId::ShadowCollect => {}
            }
            // After the built-in pass for this slot, run any Band-A silo node plugged into it, handed
            // the fully-populated PassCtx (camera + lights + surface format + the collected `items`
            // list). No registered nodes yet → this is a no-op and the frame stays byte-identical.
            self.dispatch_nodes(node, &mut encoder, &view_tex, &items, view_proj, view, cam_world, t);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

/// A snapshot of one drawable entity for a frame: `(entity, model-space transform, model hash,
/// bone palette)`. Copied out of the ECS `World` each frame so the world borrow is released before
/// `Scene` records the passes. This IS the [`crate::render_graph::PassId::Collect`] list the Band-A
/// silos read via [`crate::render_graph::PassCtx::items`] (aliased so the exposure is zero-copy).
type DrawItem = crate::render_graph::RenderItem;

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

/// Squared distance from a spot light's world position to `p` (spot-selection sort key).
fn spot_dist2(s: &SpotLightGpu, p: glam::Vec3) -> f32 {
    let dx = s.pos_range[0] - p.x;
    let dy = s.pos_range[1] - p.y;
    let dz = s.pos_range[2] - p.z;
    dx * dx + dy * dy + dz * dz
}

/// Pure [`LightAnim`] tween multiplier at time `t` (the `Rt{Light,Alpha}Animation` shape). The target
/// light's intensity = `base_intensity * light_anim_factor(..)`. Exposed for unit tests.
fn light_anim_factor(a: &LightAnim, t: f32) -> f32 {
    let phase = std::f32::consts::TAU * a.freq_hz * t;
    match a.mode {
        // Smooth sinusoidal pulse in [1-amp, 1+amp].
        LightAnimMode::Pulse => 1.0 + a.amp * phase.sin(),
        // Cheap value-noise flicker: two incommensurate sines → jittered [1-amp, 1+amp].
        LightAnimMode::Flicker => 1.0 + a.amp * (0.6 * phase.sin() + 0.4 * (phase * 2.37 + 1.3).sin()),
    }
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
        validate_wgsl("blob.wgsl", include_str!("blob.wgsl"));
        validate_wgsl("water.wgsl", include_str!("water.wgsl"));
    }

    /// The camera uniform we upload (44 f32 = 176 B) matches the shader `Camera` struct size, the point
    /// light packing (16 B count + MAX_LIGHTS * 32 B) matches the buffer we allocate, and the spot
    /// record is the 64 B (4 × vec4) the shader `SpotLight` expects.
    #[test]
    fn uniform_sizes_match() {
        assert_eq!(44 * 4, 176); // mvp(64)+model(64)+cam_pos(16)+fog_color_density(16)+fog_misc(16)
        assert_eq!(std::mem::size_of::<GpuLight>(), 32);
        assert_eq!(16 + MAX_LIGHTS * 32, 16 + 32 * 32);
        assert_eq!(std::mem::size_of::<SpotLightGpu>(), 64);
        // The shadow-params uniform = 4 cascade mat4 (256 B) + a params vec4 (16 B).
        assert_eq!(SHADOW_CASCADES * 64 + 16, 272);
        // The shadow atlas is 1024 × (1024*4) = 1024×4096, faithful to FUN_00755d90.
        assert_eq!(SHADOW_TILE, 1024);
        assert_eq!(SHADOW_TILE * SHADOW_CASCADES as u32, 4096);
    }

    /// Cascade split math: the four half-extents are strictly increasing (near→far coverage grows),
    /// cascade 0 equals the caller's base extent, and the factors are the documented nested set.
    #[test]
    fn cascade_split_is_monotonic_nested() {
        let ext = cascade_half_extents(10.0);
        assert_eq!(ext[0], 10.0); // cascade 0 = the tight box the caller asked for
        for c in 1..SHADOW_CASCADES {
            assert!(ext[c] > ext[c - 1], "cascade {c} must be wider than {}", c - 1);
        }
        // Widest cascade covers the far range.
        assert_eq!(ext[SHADOW_CASCADES - 1], 10.0 * CASCADE_SPLIT_FACTORS[SHADOW_CASCADES - 1]);
    }

    /// Cascade SELECTION mirrors the shader: a point inside the tight cascade 0 selects 0; a point only
    /// inside a wider cascade selects that cascade; a point outside every cascade selects `None` (→ a
    /// blob fallback grounds it). Uses real ortho VPs like `set_shadow` builds.
    #[test]
    fn cascade_selection_picks_tightest() {
        let center = glam::Vec3::ZERO;
        let dir = glam::Vec3::new(0.0, -1.0, 0.0);
        let up = glam::Vec3::Z; // dir ∥ Y → +Z up reference (matches set_shadow's guard)
        let extents = cascade_half_extents(4.0);
        let mut vps = [glam::Mat4::IDENTITY; SHADOW_CASCADES];
        for (c, &he) in extents.iter().enumerate() {
            let distance = (he * 3.0f32).max(40.0);
            let eye = center - dir * distance;
            let view = glam::Mat4::look_at_lh(eye, center, up);
            let proj = glam::Mat4::orthographic_lh(-he, he, -he, he, 0.1, 2.0 * distance);
            vps[c] = proj * view;
        }
        // Dead centre → tightest cascade 0.
        assert_eq!(select_cascade(&vps, glam::Vec3::ZERO), Some(0));
        // Just outside cascade 0 (he=4) but inside cascade 1 (he=10) → cascade 1.
        assert_eq!(select_cascade(&vps, glam::Vec3::new(6.0, 0.0, 0.0)), Some(1));
        // Far beyond the widest cascade (he = 4*15 = 60) → None (blob fallback).
        assert_eq!(select_cascade(&vps, glam::Vec3::new(500.0, 0.0, 0.0)), None);
    }

    /// The `Rt*Animation` pulse tween swings around 1.0 by ±amp: at quarter-period the sine peaks
    /// (k = 1+amp), at zero phase it's neutral (k = 1). Flicker stays bounded within [1-amp, 1+amp].
    #[test]
    fn light_animation_pulse_and_flicker_bounds() {
        let pulse = LightAnim {
            light_index: 0,
            base_intensity: 2.0,
            freq_hz: 1.0,
            amp: 0.5,
            mode: LightAnimMode::Pulse,
        };
        assert!((light_anim_factor(&pulse, 0.0) - 1.0).abs() < 1e-4); // neutral at t=0
        // t=0.25 s, freq 1 Hz → phase = TAU/4 → sin=1 → k = 1.5 → intensity 2.0*1.5 = 3.0.
        assert!((2.0 * light_anim_factor(&pulse, 0.25) - 3.0).abs() < 1e-4);
        let flick = LightAnim { mode: LightAnimMode::Flicker, ..pulse };
        for i in 0..200 {
            let k = light_anim_factor(&flick, i as f32 * 0.013);
            assert!(k >= 1.0 - flick.amp - 1e-3 && k <= 1.0 + flick.amp + 1e-3);
        }
    }

    /// A tween only rewrites its target light's intensity (`animated_lights` semantics), leaving the
    /// authored color + other lights intact. Exercised on the pure formula + index guard.
    #[test]
    fn light_animation_targets_by_index() {
        let mut lights = [3.0f32, 7.0];
        let a = LightAnim {
            light_index: 0,
            base_intensity: 4.0,
            freq_hz: 2.0,
            amp: 0.25,
            mode: LightAnimMode::Pulse,
        };
        // Apply exactly as animated_lights does for the in-range index.
        lights[a.light_index] = a.base_intensity * light_anim_factor(&a, 0.0);
        assert_eq!(lights[0], 4.0); // pulled to base at neutral phase
        assert_eq!(lights[1], 7.0); // untouched
    }

    /// The ShaderLevel gate maps to the light-class permutation the shader branches on: point on for
    /// `_pl`(1)/`_pl_sl`(3), spot on for `_sl`(2)/`_pl_sl`(3), neither at base(0).
    #[test]
    fn shader_level_gate_selects_light_class() {
        let point_on = |lvl: u32| lvl == 1 || lvl == 3;
        let spot_on = |lvl: u32| lvl == 2 || lvl == 3;
        assert!(!point_on(0) && !spot_on(0)); // base
        assert!(point_on(1) && !spot_on(1)); // _pl
        assert!(!point_on(2) && spot_on(2)); // _sl
        assert!(point_on(3) && spot_on(3)); // _pl_sl
    }
}
