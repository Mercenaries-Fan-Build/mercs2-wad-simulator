//! CPU billboard particle system — the FX runtime (registry §7 / `docs/rendering_fx_lighting_gap.md`
//! subsystem E).
//!
//! Mirrors the game's emitter model closely enough to drive the same content: a named effect
//! template ([`EmitterDesc`], populated from the `fxdict`/effect-template parsers in
//! `mercs2_formats::fxdict`) is *started* at a world position — the analogue of Lua's
//! `ObjectState.StartEmitter` — and spawns billboard particles that live for a lifetime, integrate
//! velocity + gravity + drag (from `FRCE`), fade colour/alpha over life (from the `COLR` gradient),
//! and scale along a size curve. Rendering is camera-facing quads with additive or alpha blending in
//! a separate pass drawn *after* the opaque forward pass (particles read depth, don't write it).
//!
//! This is a faithful CPU simulation, not the engine's job-parallel particle pass; the visual model
//! (billboards, additive/alpha, colour-over-life, gravity/drag) matches what the reversed chunks
//! describe. Soft/depth-buffered particles, ribbons/tracers (`Ribbon` 0x059b95b9) and GPU sim are
//! out of scope here (see the gap doc).

use std::collections::HashMap;

use glam::{Mat4, Vec3, Vec4};

pub use mercs2_formats::fxdict::ColorGradient;

/// One GPU billboard instance (matches `particles.wgsl` vertex layout: center, size, color).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
    center: [f32; 3],
    size: f32,
    color: [f32; 4],
}

/// Blend mode for an emitter's particles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// `(SrcAlpha, One)` — glows (fire, sparks, muzzle flash).
    Additive,
    /// `(SrcAlpha, OneMinusSrcAlpha)` — occluding puffs (smoke, dust).
    Alpha,
}

/// A named effect template: the tunables the sim needs to spawn + evolve particles. Populate this
/// from a parsed `mercs2_formats::fxdict::EffectTemplate` (EMIT timing, FRCE forces, COLR gradient,
/// PTYP/POFF), or build one directly for engine-authored effects.
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
    /// Colour-over-life gradient (from `COLR`), sampled by normalised age.
    pub gradient: ColorGradient,
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
            gradient: ColorGradient::default(),
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
        let mut stops = [[0u8; 4]; mercs2_formats::fxdict::COLR_STOPS];
        let n = stops.len();
        for (i, s) in stops.iter_mut().enumerate() {
            let t = i as f32 / (n - 1) as f32;
            // Grey that lightens slightly then fades alpha to 0 over life.
            let v = (140.0 + 60.0 * t) as u8;
            let a = ((1.0 - t) * 180.0) as u8;
            *s = [v, v, v, a];
        }
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
            gradient: ColorGradient { stops },
            color_scale: Vec4::ONE,
            blend: BlendMode::Alpha,
            max_particles: 300,
            looping: true,
        }
    }

    /// A fire/spark burst: fast, bright, additive, short-lived, gravity-pulled.
    pub fn demo_fire() -> Self {
        let mut stops = [[0u8; 4]; mercs2_formats::fxdict::COLR_STOPS];
        let n = stops.len();
        for (i, s) in stops.iter_mut().enumerate() {
            let t = i as f32 / (n - 1) as f32;
            // White-hot -> orange -> dark red, alpha fading.
            let r = 255u8;
            let g = (220.0 * (1.0 - t)).max(30.0) as u8;
            let b = (120.0 * (1.0 - t * 2.0).max(0.0)) as u8;
            let a = ((1.0 - t) * 220.0) as u8;
            *s = [r, g, b, a];
        }
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
            gradient: ColorGradient { stops },
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

    /// Build an [`EmitterDesc`] from a parsed [`EffectTemplate`](mercs2_formats::fxdict::EffectTemplate)
    /// — the authored-effect → runtime wire that replaces the name-heuristic `demo_*` presets
    /// (`docs/modernization/rendering_fx_lighting_gap.md` §E; `mercs2_game::world` flagged this as the
    /// pending decode). Uses only the **reliably-parsed** chunks; unpinned data is left at the base
    /// default it starts from (so a partial template degrades gracefully, never fabricates).
    ///
    /// - `COLR` gradient → colour/alpha over life (verified chunk).
    /// - `FRCE` forces → `Gravity`/`Wind` sum into the constant accel; `Drag` → linear damping
    ///   (`// WILDSTAR/FRCE:` the force *kind* classification is the FRCE hypothesis; `Vortex` isn't
    ///   modelled by the billboard sim).
    /// - `PTYP` bit0 → additive (glow) vs alpha blend (hypothesis, per the `fxdict` note).
    /// - `POFF` → spawn offset, applied by the caller at start (not stored on the desc).
    /// - `EMIT` timing floats have an **unpinned** positional order → NOT decoded into
    ///   lifetime/spawn_rate here (same honesty boundary as the weapon-stat offsets); timing stays at
    ///   `base`'s values until a live capture pins the float order.
    pub fn from_effect_template(t: &mercs2_formats::fxdict::EffectTemplate, base: EmitterDesc) -> Self {
        use mercs2_formats::fxdict::ForceKind;
        let mut d = base;
        if let Some(g) = t.gradient {
            d.gradient = g;
        }
        let mut accel = Vec3::ZERO;
        let mut saw_force = false;
        for f in &t.forces {
            match f.kind {
                ForceKind::Gravity | ForceKind::Wind => {
                    accel += Vec3::new(f.params[0], f.params[1], f.params[2]);
                    saw_force = true;
                }
                ForceKind::Drag => {
                    d.drag = f.params[0].max(0.0);
                    saw_force = true;
                }
                // Vortex/Unknown: retained in the parse, but the CPU billboard sim has no operator.
                ForceKind::Vortex | ForceKind::Unknown => {}
            }
        }
        if saw_force && accel != Vec3::ZERO {
            d.gravity = accel;
        }
        if let Some(pt) = t.ptype {
            d.blend = if pt.bit0() { BlendMode::Additive } else { BlendMode::Alpha };
        }
        d
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
}

const INSTANCE_SIZE: usize = std::mem::size_of::<InstanceRaw>();

fn make_instance_buffer(device: &wgpu::Device, cap: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("particle instances"),
        size: (cap.max(1) * INSTANCE_SIZE) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
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
        // mat4 view_proj (64) + vec4 cam_right (16) + vec4 cam_up (16) = 96 B.
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particle camera uniform"),
            size: 96,
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
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
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
                    entry_point: "vs_main",
                    buffers: &[vbuf_layout.clone()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
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
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
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
        }
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
        self.emitters.len()
    }

    /// Total live particle count across all emitters (test/diagnostic).
    pub fn live_particle_count(&self) -> usize {
        self.emitters.iter().map(|e| e.particles.len()).sum()
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
        eye: Vec3,
    ) {
        let mut uni = [0f32; 24];
        uni[..16].copy_from_slice(&view_proj.to_cols_array());
        uni[16..19].copy_from_slice(&cam_right.to_array());
        uni[20..23].copy_from_slice(&cam_up.to_array());
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&uni));

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

    #[test]
    fn effect_template_overrides_base_with_authored_data() {
        use mercs2_formats::fxdict::{
            ColorGradient, EffectTemplate, Force, ForceKind, ParticleType, COLR_STOPS,
        };
        let f = |kind, params| Force { inner_hash: 0, kind, params, param_count: 4 };
        let t = EffectTemplate {
            gradient: Some(ColorGradient { stops: [[255, 0, 0, 255]; COLR_STOPS] }),
            forces: vec![
                f(ForceKind::Gravity, [0.0, -9.8, 0.0, 0.0]),
                f(ForceKind::Drag, [0.5, 0.0, 0.0, 0.0]),
            ],
            ptype: Some(ParticleType { flags: 0x01 }), // bit0 → additive
            ..Default::default()
        };
        // Base = smoke (alpha, grey). The template must override with the authored values.
        let d = EmitterDesc::from_effect_template(&t, EmitterDesc::demo_smoke());
        assert_eq!(d.gravity, Vec3::new(0.0, -9.8, 0.0), "FRCE Gravity → accel");
        assert!((d.drag - 0.5).abs() < 1e-6, "FRCE Drag → damping");
        assert_eq!(d.blend, BlendMode::Additive, "PTYP bit0 → additive");
        assert_eq!(d.gradient.sample(0.0)[0], 1.0, "COLR red overrode the smoke grey");
        assert_eq!(d.gradient.sample(0.0)[1], 0.0);
        // A wholly-empty template must leave the base untouched (graceful degrade, no fabrication).
        let base = EmitterDesc::demo_fire();
        let d2 = EmitterDesc::from_effect_template(&EffectTemplate::default(), base.clone());
        assert_eq!(d2.blend, base.blend);
        assert_eq!(d2.gravity, base.gravity);
    }
}
