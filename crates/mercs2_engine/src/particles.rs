//! CPU billboard particle system — the FX runtime (registry §7 / `docs/rendering_fx_lighting_gap.md`
//! subsystem E).
//!
//! Draws two things in a pass *after* the opaque forward pass (particles read depth, don't write
//! it): an authored effect, run by the port of the retail sim ([`crate::fx_sim`]) and textured from
//! the `vfx` atlas; and an engine-authored [`EmitterDesc`] with no asset behind it (impact puffs, the
//! stand-ins for an effect that does not resolve), simulated simply and drawn as soft round sprites.
//!
//! Soft/depth-buffered particles, ribbons/tracers (`Ribbon` 0x059b95b9) and GPU sim are out of scope
//! here (see the gap doc).

use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::fx_sim::{EffectSim, Starter};
use mercs2_formats::fxdict::{EffectContainer, FxRect};

pub use mercs2_formats::fxdict::Colr;

/// One GPU billboard instance (matches `particles.wgsl` vertex layout: center, size, color).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    center: [f32; 3],
    size: f32,
    color: [f32; 4],
}

/// One GPU sprite of an effect (matches `fx.wgsl`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SpriteRaw {
    center: [f32; 3],
    size: f32,
    color: [f32; 4],
    rect: [f32; 4],
    sin_cos: [f32; 2],
    intensity: f32,
    pad: f32,
}

/// A started effect.
#[derive(Debug, Clone)]
pub struct ActiveEffect {
    pub id: EmitterId,
    pub sim: EffectSim,
    /// Start over when it ends, rather than go.
    pub repeat: bool,
    pub paused: bool,
    /// Sim seconds per real second.
    pub speed: f32,
}

/// Blend mode for an emitter's particles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// `(SrcAlpha, One)` — glows (fire, sparks, muzzle flash).
    Additive,
    /// `(SrcAlpha, OneMinusSrcAlpha)` — occluding puffs (smoke, dust).
    Alpha,
}

/// One emitter's tunables: what the sim needs to spawn + evolve particles. Populate these from a
/// parsed `mercs2_formats::fxdict::EffectContainer` ([`EmitterDesc::from_effect`]: FRCE forces,
/// COLR, PTYP), or build one directly for engine-authored effects.
#[derive(Debug, Clone)]
pub struct EmitterDesc {
    /// Continuous spawn rate (particles/second). 0 with `burst>0` = one-shot.
    pub spawn_rate: f32,
    /// One-shot burst count emitted on start (in addition to `spawn_rate`).
    pub burst: u32,
    /// Base particle lifetime (seconds) and +/- jitter fraction (0..1).
    pub lifetime: f32,
    pub lifetime_jitter: f32,
    /// Initial speed along `direction`, and its jitter fraction.
    pub start_speed: f32,
    pub speed_jitter: f32,
    /// Base emission direction (need not be normalised; zero = omni) and cone half-angle (radians).
    pub direction: Vec3,
    pub cone: f32,
    /// Constant acceleration (`FRCE` gravity), world units/s^2.
    pub gravity: Vec3,
    /// Linear drag coefficient (`FRCE` drag): `v *= 1 - drag*dt` each step.
    pub drag: f32,
    /// Size (world units) at spawn and at death (linear curve).
    pub start_size: f32,
    pub end_size: f32,
    /// Colour-over-life (from `COLR`), sampled by normalised age as red, green, blue, alpha (see
    /// `fxdict::ColrKey`). The sim ignores `half_bits`.
    pub gradient: Colr,
    /// Multiplier applied to every gradient sample (tint / HDR-ish brightness for additive).
    pub color_scale: Vec4,
    pub blend: BlendMode,
    /// Hard cap on simultaneously live particles for this emitter.
    pub max_particles: usize,
    /// Whether the emitter keeps spawning until explicitly stopped (`true`) or is a finite burst.
    pub looping: bool,
}

impl Default for EmitterDesc {
    fn default() -> Self {
        EmitterDesc {
            spawn_rate: 20.0,
            burst: 0,
            lifetime: 2.0,
            lifetime_jitter: 0.2,
            start_speed: 1.0,
            speed_jitter: 0.4,
            direction: Vec3::Y,
            cone: 0.4,
            gravity: Vec3::ZERO,
            drag: 0.0,
            start_size: 0.5,
            end_size: 1.5,
            gradient: Colr::uniform([255, 255, 255, 255], 0),
            color_scale: Vec4::ONE,
            blend: BlendMode::Alpha,
            max_particles: 256,
            looping: true,
        }
    }
}

impl EmitterDesc {
    /// A grey smoke plume: rises, expands, fades out. Alpha-blended. Handy visible-test default and
    /// a reasonable stand-in for `global_particle_smoke_*` until real templates are wired.
    pub fn demo_smoke() -> Self {
        // Grey that lightens slightly then fades alpha to 0 over life.
        let gradient = Colr::from_fn(|t| {
            let v = (140.0 + 60.0 * t) as u8;
            let a = ((1.0 - t) * 180.0) as u8;
            ([v, v, v, a], 0)
        });
        EmitterDesc {
            spawn_rate: 24.0,
            burst: 0,
            lifetime: 2.5,
            lifetime_jitter: 0.25,
            start_speed: 1.2,
            speed_jitter: 0.5,
            direction: Vec3::Y,
            cone: 0.35,
            gravity: Vec3::new(0.0, 0.4, 0.0), // gentle buoyant rise
            drag: 0.6,
            start_size: 0.4,
            end_size: 2.2,
            gradient,
            color_scale: Vec4::ONE,
            blend: BlendMode::Alpha,
            max_particles: 300,
            looping: true,
        }
    }

    /// A fire/spark burst: fast, bright, additive, short-lived, gravity-pulled.
    pub fn demo_fire() -> Self {
        // White-hot -> orange -> dark red, alpha fading.
        let gradient = Colr::from_fn(|t| {
            let g = (220.0 * (1.0 - t)).max(30.0) as u8;
            let b = (120.0 * (1.0 - t * 2.0).max(0.0)) as u8;
            let a = ((1.0 - t) * 220.0) as u8;
            ([255, g, b, a], 0)
        });
        EmitterDesc {
            spawn_rate: 60.0,
            burst: 0,
            lifetime: 1.1,
            lifetime_jitter: 0.3,
            start_speed: 2.2,
            speed_jitter: 0.6,
            direction: Vec3::Y,
            cone: 0.5,
            gravity: Vec3::new(0.0, -1.0, 0.0),
            drag: 0.2,
            start_size: 0.6,
            end_size: 0.05,
            gradient,
            color_scale: Vec4::splat(1.0),
            blend: BlendMode::Additive,
            max_particles: 400,
            looping: true,
        }
    }

    /// A **one-shot** dust/smoke puff for a bullet impact: a non-looping burst that emits once and is
    /// reaped once its particles die. Unlike [`demo_smoke`](Self::demo_smoke) (a *continuous* plume,
    /// `looping: true`), this does NOT accumulate — the fix for impact emitters piling up forever.
    pub fn impact_puff() -> Self {
        EmitterDesc {
            spawn_rate: 0.0,
            burst: 10,
            looping: false,
            lifetime: 0.5,
            lifetime_jitter: 0.3,
            start_speed: 1.4,
            start_size: 0.12,
            end_size: 0.5,
            ..Self::demo_smoke()
        }
    }

    /// A **one-shot** fireball burst for an explosion impact (non-looping → auto-reaped).
    pub fn impact_fire() -> Self {
        EmitterDesc {
            spawn_rate: 0.0,
            burst: 26,
            looping: false,
            lifetime: 0.7,
            ..Self::demo_fire()
        }
    }
}

/// A static, persistent additive glow billboard — the faithful cheap rendering of an authored
/// *static* environmental FX that is a single textured card, not a spewing emitter. Used for the PMC
/// hall's `global_particle_env_godray2` light shafts: era-appropriate (2007) additive soft "god-ray"
/// glows descending from the dome/skylight, placed high above the floor. Position/size/tint come from
/// the authored placement + the effect's `TRFM`/`COLR` (see `mercs2_game::world`). Rendered as a
/// camera-facing additive soft sprite (the same pipeline the additive particles use).
#[derive(Debug, Clone, Copy)]
pub struct GlowCard {
    pub pos: [f32; 3],
    /// World-space diameter of the glow.
    pub size: f32,
    /// Straight RGBA tint; alpha weights the additive contribution.
    pub color: [f32; 4],
}

/// A single live particle.
#[derive(Clone, Copy)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    lifetime: f32,
}

/// Opaque handle to a started emitter instance (mirrors an `ObjectState` emitter attachment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmitterId(pub u64);

/// A started emitter: a template placed at a world position with its live particles.
struct ActiveEmitter {
    id: EmitterId,
    desc: EmitterDesc,
    origin: Vec3,
    particles: Vec<Particle>,
    spawn_accum: f32,
    /// While `true` the emitter keeps spawning; `stop` clears it and lets particles finish.
    spawning: bool,
    rng: u32,
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}
/// Uniform 0..1.
fn rand01(state: &mut u32) -> f32 {
    (xorshift(state) >> 8) as f32 / (1u32 << 24) as f32
}
/// Uniform -1..1.
fn rand_sym(state: &mut u32) -> f32 {
    rand01(state) * 2.0 - 1.0
}

impl ActiveEmitter {
    fn spawn_one(&mut self) {
        if self.particles.len() >= self.desc.max_particles {
            return;
        }
        let d = &self.desc;
        // Direction within a cone about `direction` (or omni if direction ~ 0).
        let base = if d.direction.length_squared() > 1e-6 {
            d.direction.normalize()
        } else {
            Vec3::Y
        };
        let jitter = Vec3::new(rand_sym(&mut self.rng), rand_sym(&mut self.rng), rand_sym(&mut self.rng))
            * d.cone;
        let dir = (base + jitter).normalize_or_zero();
        let speed = d.start_speed * (1.0 + rand_sym(&mut self.rng) * d.speed_jitter);
        let life = (d.lifetime * (1.0 + rand_sym(&mut self.rng) * d.lifetime_jitter)).max(0.05);
        self.particles.push(Particle {
            pos: self.origin,
            vel: dir * speed,
            age: 0.0,
            lifetime: life,
        });
    }

    fn update(&mut self, dt: f32) {
        let d = self.desc.clone();
        // Integrate + age existing particles; drop the dead.
        let damp = (1.0 - d.drag * dt).clamp(0.0, 1.0);
        self.particles.retain_mut(|p| {
            p.age += dt;
            if p.age >= p.lifetime {
                return false;
            }
            p.vel += d.gravity * dt;
            p.vel *= damp;
            p.pos += p.vel * dt;
            true
        });
        // Spawn new particles while active.
        if self.spawning {
            self.spawn_accum += d.spawn_rate * dt;
            while self.spawn_accum >= 1.0 {
                self.spawn_accum -= 1.0;
                self.spawn_one();
            }
            if !d.looping {
                // Finite emitters spawn their burst once (handled at start) then stop spawning.
                self.spawning = false;
            }
        }
    }

    /// Whether this emitter is finished (stopped and no particles left) and can be reaped.
    fn is_dead(&self) -> bool {
        !self.spawning && self.particles.is_empty()
    }

    /// Append this emitter's live particles as GPU instances into the matching blend bucket.
    fn emit_instances(&self, out: &mut Vec<InstanceRaw>) {
        let d = &self.desc;
        for p in &self.particles {
            let t = (p.age / p.lifetime).clamp(0.0, 1.0);
            let size = d.start_size + (d.end_size - d.start_size) * t;
            let c = d.gradient.sample(t);
            let color = [
                c[0] * d.color_scale.x,
                c[1] * d.color_scale.y,
                c[2] * d.color_scale.z,
                c[3] * d.color_scale.w,
            ];
            out.push(InstanceRaw { center: p.pos.into(), size, color });
        }
    }
}

/// GPU + CPU state for the particle pass.
pub struct ParticleSystem {
    pipeline_add: wgpu::RenderPipeline,
    pipeline_alpha: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    uniform_bind: wgpu::BindGroup,
    /// Instance buffers, grown on demand, for each blend bucket.
    add_buf: wgpu::Buffer,
    add_cap: usize,
    add_count: u32,
    alpha_buf: wgpu::Buffer,
    alpha_cap: usize,
    alpha_count: u32,
    /// Named effect templates (key = effect name hash, as Lua's `StartEmitter` names them).
    templates: HashMap<u32, EmitterDesc>,
    emitters: Vec<ActiveEmitter>,
    next_id: u64,
    /// Static additive glow cards (e.g. the PMC hall god-ray light shafts). Persistent; drawn every
    /// frame into the additive bucket, no simulation.
    glow_cards: Vec<GlowCard>,
    effects: Vec<ActiveEffect>,
    /// The `fxdict`, by frame key, set with the atlas.
    rects: HashMap<u32, FxRect>,
    /// Last frame's `dt`: retail steps by the mean of the last two (`FUN_0048e550`).
    last_dt: f32,
    fx_pipeline: wgpu::RenderPipeline,
    fx_layout: wgpu::BindGroupLayout,
    fx_sampler: wgpu::Sampler,
    fx_bind: Option<wgpu::BindGroup>,
    fx_buf: wgpu::Buffer,
    fx_cap: usize,
    fx_count: u32,
    /// The colour target encodes sRGB: the sprite's values are gamma, as retail's are.
    srgb_target: bool,
}

const INSTANCE_SIZE: usize = std::mem::size_of::<InstanceRaw>();
const SPRITE_SIZE: usize = std::mem::size_of::<SpriteRaw>();
/// mat4 view_proj + vec4 right + vec4 up + vec4 forward.
const CAMERA_BYTES: u64 = 112;

fn make_vertex_buffer(device: &wgpu::Device, bytes: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle instances"),
        size: bytes.max(1) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn make_instance_buffer(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
    make_vertex_buffer(device, cap * INSTANCE_SIZE)
}

impl ParticleSystem {
    /// Build the particle pipelines against the scene's colour + depth targets. The depth state
    /// tests (Less) but does not write, so particles are occluded by nearer opaque geometry yet
    /// don't corrupt the depth buffer for subsequent transparent draws.
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("particles.wgsl"));

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("particle camera bgl"),
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
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle camera uniform"),
            size: CAMERA_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("particle camera bind"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform_buf.as_entire_binding() }],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("particle pipeline layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        let vbuf_layout = wgpu::VertexBufferLayout {
            array_stride: INSTANCE_SIZE as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32, 2 => Float32x4],
        };

        let make_pipeline = |blend: wgpu::BlendState, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(vbuf_layout.clone())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: Some(blend),
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
                    format: depth_format,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        // Additive: SrcAlpha * src + 1 * dst (glow). Alpha channel accumulates but is unused (opaque
        // target). Alpha: standard over-blend.
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let alpha = wgpu::BlendState::ALPHA_BLENDING;

        let pipeline_add = make_pipeline(additive, "particle additive pipeline");
        let pipeline_alpha = make_pipeline(alpha, "particle alpha pipeline");

        let fx_shader = device.create_shader_module(wgpu::include_wgsl!("fx.wgsl"));
        let fx_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("effect sprite bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let fx_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("effect sprite pipeline layout"),
            bind_group_layouts: &[Some(&fx_layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let fx_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("effect sprite pipeline"),
            layout: Some(&fx_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &fx_shader,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: SPRITE_SIZE as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3, 1 => Float32, 2 => Float32x4, 3 => Float32x4, 4 => Float32x2, 5 => Float32
                    ],
                })],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fx_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
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
                format: depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let fx_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("effect atlas sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        ParticleSystem {
            pipeline_add,
            pipeline_alpha,
            uniform_buf,
            uniform_bind,
            add_buf: make_instance_buffer(device, 256),
            add_cap: 256,
            add_count: 0,
            alpha_buf: make_instance_buffer(device, 256),
            alpha_cap: 256,
            alpha_count: 0,
            templates: HashMap::new(),
            emitters: Vec::new(),
            next_id: 1,
            glow_cards: Vec::new(),
            effects: Vec::new(),
            rects: HashMap::new(),
            last_dt: 0.0,
            fx_pipeline,
            fx_layout,
            fx_sampler,
            fx_bind: None,
            fx_buf: make_vertex_buffer(device, 256 * SPRITE_SIZE),
            fx_cap: 256,
            fx_count: 0,
            srgb_target: color_format.is_srgb(),
        }
    }

    /// Set what effects draw from: the `vfx` atlas and the `fxdict`'s rectangles.
    pub fn set_atlas(&mut self, device: &wgpu::Device, view: &wgpu::TextureView, rects: &[FxRect]) {
        self.rects = rects.iter().map(|r| (r.key, *r)).collect();
        self.fx_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("effect sprite bind"),
            layout: &self.fx_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&self.fx_sampler) },
            ],
        }));
    }

    /// Whether [`Self::set_atlas`] has run.
    pub fn has_atlas(&self) -> bool {
        self.fx_bind.is_some()
    }

    /// Start `effect` at a world position and rotation. An error when the atlas is not set or the
    /// effect cannot run.
    pub fn start_effect(&mut self, effect: &EffectContainer, starter: Starter, pos: Vec3, rot: Quat, repeat: bool) -> Result<EmitterId, String> {
        if !self.has_atlas() {
            return Err("an effect started before the vfx atlas was set".to_string());
        }
        let id = EmitterId(self.next_id);
        self.next_id += 1;
        let sim = EffectSim::new(effect, &self.rects, starter, pos, rot, (id.0 as u32).wrapping_mul(2654435761))?;
        self.effects.push(ActiveEffect { id, sim, repeat, paused: false, speed: 1.0 });
        Ok(id)
    }

    pub fn effect_mut(&mut self, id: EmitterId) -> Option<&mut ActiveEffect> {
        self.effects.iter_mut().find(|e| e.id == id)
    }

    /// Take a started effect away at once, particles and all.
    pub fn remove_effect(&mut self, id: EmitterId) {
        self.effects.retain(|e| e.id != id);
    }

    /// Replace the set of static additive glow cards (persistent environmental FX like the PMC hall
    /// god-ray light shafts). An empty slice clears them.
    pub fn set_glow_cards(&mut self, cards: &[GlowCard]) {
        self.glow_cards = cards.to_vec();
    }

    /// Number of active glow cards (diagnostic / transparent-pass gate).
    pub fn glow_card_count(&self) -> usize {
        self.glow_cards.len()
    }

    /// Register a named effect template (key = the effect name hash Lua's `StartEmitter` uses).
    pub fn register_template(&mut self, name_hash: u32, desc: EmitterDesc) {
        self.templates.insert(name_hash, desc);
    }

    /// Start a registered effect at a world position (the analogue of `ObjectState.StartEmitter`).
    /// Returns `None` if no template with that hash is registered.
    pub fn start_emitter(&mut self, name_hash: u32, origin: Vec3) -> Option<EmitterId> {
        let desc = self.templates.get(&name_hash)?.clone();
        Some(self.start_emitter_desc(desc, origin))
    }

    /// Start an ad-hoc effect from an explicit descriptor at a world position.
    pub fn start_emitter_desc(&mut self, desc: EmitterDesc, origin: Vec3) -> EmitterId {
        let id = EmitterId(self.next_id);
        self.next_id += 1;
        let burst = desc.burst;
        let mut e = ActiveEmitter {
            id,
            desc,
            origin,
            particles: Vec::new(),
            spawn_accum: 0.0,
            spawning: true,
            rng: 0x9E3779B9 ^ (id.0 as u32).wrapping_mul(2654435761),
        };
        for _ in 0..burst {
            e.spawn_one();
        }
        self.emitters.push(e);
        id
    }

    /// Stop an emitter: it ceases spawning; existing particles finish their lives and the emitter is
    /// reaped once empty. (Mirrors `ObjectState.StopEmitter`.)
    pub fn stop_emitter(&mut self, id: EmitterId) {
        if let Some(e) = self.emitters.iter_mut().find(|e| e.id == id) {
            e.spawning = false;
        }
    }

    /// Number of currently-live emitters (started, not yet reaped).
    pub fn active_emitter_count(&self) -> usize {
        self.emitters.len() + self.effects.len()
    }

    /// Total live particle count across all emitters (test/diagnostic).
    pub fn live_particle_count(&self) -> usize {
        self.emitters.iter().map(|e| e.particles.len()).sum::<usize>() + self.effects.iter().map(|e| e.sim.live_particles()).sum::<usize>()
    }

    /// Advance the simulation by `dt` seconds and reap finished emitters.
    pub fn update(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        for e in &mut self.emitters {
            e.update(dt);
        }
        self.emitters.retain(|e| !e.is_dead());
        let step = (self.last_dt + dt) * 0.5;
        self.last_dt = dt;
        for e in self.effects.iter_mut().filter(|e| !e.paused) {
            e.sim.step(step * e.speed);
            if e.repeat && e.sim.is_finished() {
                e.sim.restart();
            }
        }
        self.effects.retain(|e| !e.sim.is_finished());
    }

    /// Upload the camera uniform + rebuild/upload the instance buffers. Call once per frame BEFORE
    /// beginning the render pass. `view_proj` must be the SAME matrix the scene draws geometry with
    /// (so particles register with the world); `eye` drives back-to-front sorting of alpha
    /// particles.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_proj: Mat4,
        cam_right: Vec3,
        cam_up: Vec3,
        cam_forward: Vec3,
        eye: Vec3,
    ) {
        let mut uni = [0f32; 28];
        uni[..16].copy_from_slice(&view_proj.to_cols_array());
        uni[16..19].copy_from_slice(&cam_right.to_array());
        uni[20..23].copy_from_slice(&cam_up.to_array());
        uni[24..27].copy_from_slice(&cam_forward.to_array());
        uni[27] = if self.srgb_target { 1.0 } else { 0.0 };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&uni));

        let mut sprites: Vec<SpriteRaw> = Vec::new();
        for e in &self.effects {
            e.sim.sprites(cam_right, cam_up, cam_forward, |s| {
                sprites.push(SpriteRaw {
                    center: s.pos,
                    size: s.size,
                    color: s.color,
                    rect: s.rect,
                    sin_cos: [s.sin, s.cos],
                    intensity: s.intensity,
                    pad: 0.0,
                })
            });
        }
        if sprites.len() > self.fx_cap {
            self.fx_cap = sprites.len().next_power_of_two();
            self.fx_buf = make_vertex_buffer(device, self.fx_cap * SPRITE_SIZE);
        }
        if !sprites.is_empty() {
            queue.write_buffer(&self.fx_buf, 0, bytemuck::cast_slice(&sprites));
        }
        self.fx_count = sprites.len() as u32;

        let mut add_inst: Vec<InstanceRaw> = Vec::new();
        let mut alpha_inst: Vec<InstanceRaw> = Vec::new();
        for e in &self.emitters {
            match e.desc.blend {
                BlendMode::Additive => e.emit_instances(&mut add_inst),
                BlendMode::Alpha => e.emit_instances(&mut alpha_inst),
            }
        }
        // Static glow cards are additive, persistent billboards (no sim).
        for g in &self.glow_cards {
            add_inst.push(InstanceRaw { center: g.pos, size: g.size, color: g.color });
        }
        // Sort alpha particles back-to-front for correct over-blending (additive is order-free).
        alpha_inst.sort_by(|a, b| {
            let da = (Vec3::from(a.center) - eye).length_squared();
            let db = (Vec3::from(b.center) - eye).length_squared();
            db.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
        });

        Self::upload_bucket(device, queue, &add_inst, &mut self.add_buf, &mut self.add_cap);
        self.add_count = add_inst.len() as u32;
        Self::upload_bucket(device, queue, &alpha_inst, &mut self.alpha_buf, &mut self.alpha_cap);
        self.alpha_count = alpha_inst.len() as u32;
    }

    fn upload_bucket(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        inst: &[InstanceRaw],
        buf: &mut wgpu::Buffer,
        cap: &mut usize,
    ) {
        if inst.is_empty() {
            return;
        }
        if inst.len() > *cap {
            *cap = inst.len().next_power_of_two();
            *buf = make_instance_buffer(device, *cap);
        }
        queue.write_buffer(buf, 0, bytemuck::cast_slice(inst));
    }

    /// Record the particle draws into an already-open render pass (after the opaque geometry). The
    /// pass must target the same colour + depth attachments the pipelines were built for.
    pub fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        pass.set_bind_group(0, &self.uniform_bind, &[]);
        if self.alpha_count > 0 {
            pass.set_pipeline(&self.pipeline_alpha);
            pass.set_vertex_buffer(0, self.alpha_buf.slice(..));
            pass.draw(0..6, 0..self.alpha_count);
        }
        if self.add_count > 0 {
            pass.set_pipeline(&self.pipeline_add);
            pass.set_vertex_buffer(0, self.add_buf.slice(..));
            pass.draw(0..6, 0..self.add_count);
        }
        if let (Some(bind), true) = (&self.fx_bind, self.fx_count > 0) {
            pass.set_bind_group(0, bind, &[]);
            pass.set_pipeline(&self.fx_pipeline);
            pass.set_vertex_buffer(0, self.fx_buf.slice(..));
            pass.draw(0..6, 0..self.fx_count);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pure-CPU sim tests (no GPU). We construct ActiveEmitter-equivalent behaviour via the public
    // update path by driving a headless emitter list — but ParticleSystem::new needs a device, so
    // these test the emitter integration logic directly through a small harness.

    fn drive(desc: EmitterDesc, steps: usize, dt: f32) -> ActiveEmitter {
        let mut e = ActiveEmitter {
            id: EmitterId(1),
            desc,
            origin: Vec3::ZERO,
            particles: Vec::new(),
            spawn_accum: 0.0,
            spawning: true,
            rng: 12345,
        };
        for _ in 0..steps {
            e.update(dt);
        }
        e
    }

    #[test]
    fn continuous_emitter_spawns_and_caps() {
        let desc = EmitterDesc {
            spawn_rate: 100.0,
            lifetime: 10.0,
            lifetime_jitter: 0.0,
            max_particles: 16,
            looping: true,
            ..EmitterDesc::default()
        };
        // 100/s over 1s would spawn ~100 but cap is 16.
        let e = drive(desc, 100, 0.01);
        assert!(e.particles.len() <= 16);
        assert!(e.particles.len() >= 8, "should have spawned a bunch, got {}", e.particles.len());
    }

    #[test]
    fn particles_expire_after_lifetime() {
        let desc = EmitterDesc {
            spawn_rate: 0.0,
            burst: 5,
            lifetime: 0.5,
            lifetime_jitter: 0.0,
            looping: false,
            ..EmitterDesc::default()
        };
        let mut e = ActiveEmitter {
            id: EmitterId(1),
            desc,
            origin: Vec3::ZERO,
            particles: Vec::new(),
            spawn_accum: 0.0,
            spawning: true,
            rng: 999,
        };
        for _ in 0..5 {
            e.spawn_one();
        }
        assert_eq!(e.particles.len(), 5);
        // After > lifetime and no respawn (non-looping), all die.
        for _ in 0..20 {
            e.update(0.05);
        }
        assert_eq!(e.particles.len(), 0);
        assert!(e.is_dead());
    }

    #[test]
    fn gravity_and_drag_move_particles() {
        let desc = EmitterDesc {
            spawn_rate: 0.0,
            burst: 1,
            lifetime: 100.0,
            lifetime_jitter: 0.0,
            start_speed: 0.0,
            speed_jitter: 0.0,
            cone: 0.0,
            direction: Vec3::Y,
            gravity: Vec3::new(0.0, -10.0, 0.0),
            drag: 0.0,
            looping: false,
            ..EmitterDesc::default()
        };
        let mut e = ActiveEmitter {
            id: EmitterId(1),
            desc,
            origin: Vec3::ZERO,
            particles: Vec::new(),
            spawn_accum: 0.0,
            spawning: true,
            rng: 1,
        };
        e.spawn_one();
        for _ in 0..10 {
            e.update(0.1); // 1 second total
        }
        // Under -10 gravity for ~1s, the particle should have fallen well below the origin.
        assert!(e.particles[0].pos.y < -3.0, "y = {}", e.particles[0].pos.y);
    }

    #[test]
    fn instances_track_size_curve() {
        let desc = EmitterDesc {
            spawn_rate: 0.0,
            burst: 1,
            lifetime: 1.0,
            lifetime_jitter: 0.0,
            start_size: 1.0,
            end_size: 3.0,
            start_speed: 0.0,
            cone: 0.0,
            looping: false,
            ..EmitterDesc::default()
        };
        let mut e = ActiveEmitter {
            id: EmitterId(1),
            desc,
            origin: Vec3::ZERO,
            particles: Vec::new(),
            spawn_accum: 0.0,
            spawning: true,
            rng: 7,
        };
        e.spawn_one();
        let mut out = Vec::new();
        e.emit_instances(&mut out);
        assert_eq!(out.len(), 1);
        assert!((out[0].size - 1.0).abs() < 1e-4); // freshly spawned -> start_size
        e.update(0.5); // halfway through life
        out.clear();
        e.emit_instances(&mut out);
        assert!((out[0].size - 2.0).abs() < 0.1); // ~mid of 1..3
    }

    #[test]
    fn demo_presets_build() {
        let s = EmitterDesc::demo_smoke();
        assert_eq!(s.blend, BlendMode::Alpha);
        let f = EmitterDesc::demo_fire();
        assert_eq!(f.blend, BlendMode::Additive);
    }
}
