//! The retail particle simulation: what an effect (`mercs2_formats::fxdict::EffectContainer`) does
//! once started. A port of the v1.0 spawn path (`FUN_0048f4f0`, `FUN_0048ae80`, `FUN_00488770`), the
//! integrator (`FUN_00876840`) and the render copy (`FUN_00724570`, `FUN_00493660`). Units, operators
//! and evidence: `docs/reverse_engineer/particle_sim_units.md`.
//!
//! An instance steps once a frame by a variable `dt` in seconds, with no substeps. There is no
//! looping flag in an effect: it emits while its `rate` is above 0, a linear curve holding its last
//! key for ever, so a burst is a `rate` curve that ends at 0. Its duration comes from the template
//! that starts it ([`Starter`]).

use glam::{Mat4, Quat, Vec3};
use mercs2_formats::fxdict::{AnimKey, Atrb, AtrbValue, Colr, EffectContainer, ForceKind, FxRect, COLR_KEYS};
use std::collections::HashMap;

/// A linear curve's key times are 60 Hz frames (`0xD2D898`).
const CURVE_FRAMES_PER_SECOND: f32 = 60.0;
/// A resampled curve holds this many samples, at key times 0..99: percent of the particle's life.
const SAMPLES: usize = 100;
const DEGREES: f32 = 0.0174533;
/// `FUN_0048ac80`'s cap on one emitter's spawns in one update.
const MAX_SPAWN_PER_UPDATE: f32 = 2000.0;
/// The live particle pool (`FUN_0082d040`; 720,000 bytes of 0x30-byte records).
pub const MAX_LIVE_PARTICLES: usize = 15000;
/// An effect with no duration ends past this many seconds with no particle left (`0xBA8990`).
const IDLE_END_SECONDS: f32 = 3.0;
/// `moved` is capped at this many metres per second (`0xB984A8`), as is the velocity particles inherit.
const MAX_INSTANCE_SPEED: f32 = 200.0;
/// The softening in the attractor and vortex denominators (`0xBEA990`).
const FORCE_SOFTENING: f32 = 1.0;
/// `FUN_0048ae80` replaces a direction shorter than this, squared, with +Y.
const MIN_DIRECTION_SQUARED: f32 = 0.99;

const RATE: u32 = 0x062F_0D37;
const RATEVAR: u32 = 0x7065_3182;
const LIFE: u32 = 0xD3AE_67AF;
const LIFEVAR: u32 = 0xC7CF_E6AA;
const SPEED: u32 = 0x15BA_509E;
const SPEEDVAR: u32 = 0x9BE6_2E41;
const SPREAD: u32 = 0x437F_66EC;
const SPREADVAR: u32 = 0xB8A9_5DE3;
const INHERITVEL: u32 = 0xB424_7BF3;
const INHERITVELVAR: u32 = 0xB15A_BB7E;
const SIZE: u32 = 0x8C36_54C2;
const SIZEVAR: u32 = 0xC100_8E25;
const MASS: u32 = 0xC3AE_B321;
const MASSVAR: u32 = 0x4703_5D90;
const SPIN: u32 = 0x497A_1895;
const SPINVAR: u32 = 0x1792_B524;
const START_ANGLE: u32 = 0x35AC_BAB7;
const START_ANGLEVAR: u32 = 0x1B87_8602;
const FORCEGRAVITY: u32 = 0xC558_C9D8;
const FORCEGRAVITYVAR: u32 = 0xD80D_F37F;
const FORCEDRAG: u32 = 0x6CF0_BE14;
const FORCEDRAGVAR: u32 = 0xBE96_8D3B;
const FORCEWIND: u32 = 0xB3DB_B6C0;
const FORCEWINDVAR: u32 = 0x4C49_B137;
const FORCE_ATTRACTOR_FACTOR: u32 = 0x1083_1673;
const FORCE_ATTRACTOR_FACTORVAR: u32 = 0xB619_7EFE;
const FORCE_VORTEX_FACTOR: u32 = 0x720F_22F8;
const FORCE_VORTEX_FACTORVAR: u32 = 0x4712_719F;
const SCALE: u32 = 0xEEE1_A341;
const EMISSIONDIR: u32 = 0x270C_9E9D;
const AMPL: u32 = 0x3DC3_D9DF;
const POS: [u32; 3] = [0x7D11_17EB, 0x9B0F_088E, 0xFD16_5E99];
const ROT: [u32; 3] = [0xF6B5_5F38, 0x20B7_DFED, 0x9EB0_5782];
const SCL: [u32; 3] = [0xC557_4C5F, 0xE355_3D02, 0x655C_C56D];
const DRAG_RADIUS: u32 = 0x2F68_CD8F;
const LOCAL: u32 = 0x201B_5A86;
const RANGE: u32 = 0xE068_6A68;
const DECAY: u32 = 0xC278_3A55;
const RADIAL: u32 = 0xB46F_1F0C;

/// The world's wind, which a `wind` force reads in place of its own values (`*(0xE7ADFC)+0x104`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalWind {
    /// XZ.
    pub direction: [f32; 2],
    pub speed: f32,
}

impl GlobalWind {
    /// The value in the `updated-chris-exports` flying-winch capture. Where the game sets it is not
    /// read.
    pub const CAPTURED: GlobalWind = GlobalWind { direction: [0.958, 0.287], speed: 0.003 };
}

/// What the template starting an effect says about it (`RedEffectComponent`, read by `FUN_00488d70`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Starter {
    /// Seconds of emission, 0 for none: then the effect ends on its own rule.
    pub duration: f32,
    /// Particles per metre the instance moves (`0x62C7746E`).
    pub per_metre: f32,
}

/// One sprite to draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub pos: [f32; 3],
    /// The quad's edge, metres (INFERRED: it needs unit camera axes, which no capture showed).
    pub size: f32,
    /// Red, green, blue, alpha, 0..1.
    pub color: [f32; 4],
    /// `(u, top, w, h)` in the atlas.
    pub rect: [f32; 4],
    pub sin: f32,
    pub cos: f32,
    /// The `COLR` binary16: its magnitude multiplies the colour; below 0 the sprite adds.
    pub intensity: f32,
}

/// The value `half` encodes.
fn binary16(half: u16) -> f32 {
    let sign = if half & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((half >> 10) & 0x1F) as i32;
    let frac = (half & 0x3FF) as f32 / 1024.0;
    sign * match exp {
        0 => frac * 2f32.powi(-14),
        31 => f32::INFINITY,
        _ => (1.0 + frac) * 2f32.powi(exp - 15),
    }
}

/// `FUN_00490960`'s curve read: the first key later than `x`, lerped from the one before (from
/// `(0, keys[0].value)` for the first), the last key's value past the end. No key reads 0.
fn sample_linear(keys: &[AnimKey], x: f32) -> f32 {
    let Some(last) = keys.last() else { return 0.0 };
    match keys.iter().position(|k| k.time > x) {
        None => last.value,
        Some(0) => keys[0].value,
        Some(i) => {
            let (a, b) = (keys[i - 1], keys[i]);
            a.value + (b.value - a.value) * ((x - a.time) / (b.time - a.time))
        }
    }
}

fn value_of(a: &Atrb) -> f32 {
    match a.value {
        AtrbValue::F32(v) => v,
        AtrbValue::U32(v) => v as f32,
    }
}

fn find<'a>(attrs: &'a [Atrb], hash: u32, what: &str) -> Result<&'a Atrb, String> {
    attrs.iter().find(|a| a.hash == hash).ok_or_else(|| format!("{what} has no attribute 0x{hash:08X}"))
}

/// An attribute with a linear curve, read at emitter time.
#[derive(Debug, Clone, PartialEq)]
struct Linear {
    value: f32,
    keys: Option<Vec<AnimKey>>,
}

impl Linear {
    fn read(attrs: &[Atrb], hash: u32, what: &str) -> Result<Linear, String> {
        let a = find(attrs, hash, what)?;
        Ok(Linear { value: value_of(a), keys: a.curve.clone() })
    }

    /// The value `t` seconds into the emitter's clock.
    fn at(&self, t: f32) -> f32 {
        match &self.keys {
            Some(k) => sample_linear(k, t * CURVE_FRAMES_PER_SECOND),
            None => self.value,
        }
    }
}

/// An attribute and its variance: `value + var · (1 − 2u)`, `u` rolled on each read (`FUN_00490960`).
#[derive(Debug, Clone, PartialEq)]
struct Varied {
    base: Linear,
    var: Linear,
}

impl Varied {
    fn read(attrs: &[Atrb], hash: u32, var: u32, what: &str) -> Result<Varied, String> {
        Ok(Varied { base: Linear::read(attrs, hash, what)?, var: Linear::read(attrs, var, what)? })
    }

    fn roll(&self, t: f32, rng: &mut u32) -> f32 {
        self.base.at(t) + self.var.at(t) * (1.0 - 2.0 * rand01(rng))
    }
}

/// An attribute resampled at load to [`SAMPLES`] points of a particle's life (`FUN_00493150`), with a
/// variance taken from the particle's random byte.
#[derive(Debug, Clone, PartialEq)]
struct Resampled {
    samples: [f32; SAMPLES],
    var: f32,
}

impl Resampled {
    fn read(attrs: &[Atrb], hash: u32, var: u32, what: &str) -> Result<Resampled, String> {
        let a = find(attrs, hash, what)?;
        let mut samples = [value_of(a); SAMPLES];
        if let Some(k) = &a.curve {
            for (i, s) in samples.iter_mut().enumerate() {
                *s = sample_linear(k, i as f32);
            }
        }
        Ok(Resampled { samples, var: value_of(find(attrs, var, what)?) })
    }

    /// `FUN_00490d90`: the sample at `99 · age01`, lerped to the next.
    fn at(&self, age01: f32) -> f32 {
        let at = age01.clamp(0.0, 1.0) * (SAMPLES - 1) as f32;
        let i = at as usize;
        let next = self.samples[(i + 1).min(SAMPLES - 1)];
        self.samples[i] + (next - self.samples[i]) * (at - i as f32)
    }
}

/// A per-particle attribute: `value + var · U`, `U` from the particle's random byte.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Factor {
    value: f32,
    var: f32,
}

impl Factor {
    fn read(attrs: &[Atrb], hash: u32, var: u32, what: &str) -> Result<Factor, String> {
        Ok(Factor { value: value_of(find(attrs, hash, what)?), var: value_of(find(attrs, var, what)?) })
    }

    fn at(&self, u: f32) -> f32 {
        self.value + self.var * u
    }
}

/// `U` of a particle's random byte: one byte drives all its variances, so they correlate.
fn unit_of(random: u8) -> f32 {
    2.0 * (0.5 - random as f32 / 255.0)
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Uniform in `[0, 1)`.
fn rand01(state: &mut u32) -> f32 {
    (xorshift(state) >> 8) as f32 / (1u32 << 24) as f32
}

/// `dir` turned by `theta` off itself, then by `phi` about itself (`FUN_00488890`).
fn perturb(dir: Vec3, theta: f32, phi: f32) -> Vec3 {
    let off = Quat::from_axis_angle(dir.any_orthonormal_vector(), theta) * dir;
    Quat::from_axis_angle(dir, phi) * off
}

/// `FUN_00823370`, the product of the axis rotations: about Z, then Y, then X.
fn euler_degrees(r: [f32; 3]) -> Quat {
    Quat::from_rotation_x(r[0] * DEGREES) * Quat::from_rotation_y(r[1] * DEGREES) * Quat::from_rotation_z(r[2] * DEGREES)
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    pos: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    /// `age / life` when last stepped, as the render copy reads it.
    age01: f32,
    /// The byte that drives every variance.
    random: u8,
    /// The flipbook counter, starting at the random byte.
    frame: u32,
}

/// What every emitter's spawn reads from one update.
struct SpawnFrame {
    t: f32,
    dt: f32,
    /// Metres the instance moved, capped.
    moved: f32,
    per_metre: f32,
    /// Its displacement, along which spawns smear, and its velocity.
    delta: Vec3,
    velocity: Vec3,
    instance: Mat4,
}

#[derive(Debug, Clone)]
struct EmitterSim {
    /// The `GEOM` records it samples; `None` for the 9 retail emitters that spawn nothing.
    records: Option<Vec<[f32; 13]>>,
    pos: [Linear; 3],
    rot: [Linear; 3],
    scl: [Linear; 3],
    rate: Varied,
    life: Varied,
    speed: Varied,
    spread: Varied,
    inheritvel: Varied,
    size: Resampled,
    mass: Factor,
    gravity: Factor,
    drag: Factor,
    wind: Factor,
    attractor: Factor,
    vortex: Factor,
    spin: Factor,
    start_angle: Factor,
    colr: Colr,
    /// The atlas rectangles of its frames.
    frames: Vec<[f32; 4]>,
    /// `PTYP` bit 0: the sprite turns to the screen-projected velocity.
    velocity_aligned: bool,
    /// `PTYP` bit 1: frames advance one a sim update, else they span the life.
    flipbook: bool,
    /// Sim updates per flipbook frame, and the counter's wrap.
    scale: u32,
    wrap: u32,
    accum: f32,
    particles: Vec<Particle>,
}

impl EmitterSim {
    fn read(effect: &EffectContainer, i: usize, rects: &HashMap<u32, FxRect>) -> Result<EmitterSim, String> {
        let e = &effect.emitters[i];
        let what = format!("emitter {i}");
        let p = &e.particle.attributes;
        let ch = |h: u32| Linear::read(&e.channels, h, &what);
        let records = match e.geom {
            Some(g) => {
                let shape = effect.shapes.get(g.shape_index as usize).ok_or_else(|| format!("{what}: no shape {}", g.shape_index))?;
                Some(shape.records.iter().take(g.word_00 as usize).copied().collect())
            }
            None => None,
        };
        // A key the search misses draws the whole atlas (`FUN_00491510`).
        let frames: Vec<[f32; 4]> =
            e.particle.text.frames.iter().map(|k| rects.get(k).map_or([0.0, 0.0, 1.0, 1.0], |r| [r.u, r.top(), r.w, r.h])).collect();
        if frames.is_empty() {
            return Err(format!("{what}: no TEXT frame"));
        }
        let scale = (value_of(find(p, SCALE, &what)?) as u32).max(1);
        let first = |hash: u32, var: u32| -> Result<Factor, String> {
            let r = Resampled::read(p, hash, var, &what)?;
            Ok(Factor { value: r.samples[0], var: r.var })
        };
        Ok(EmitterSim {
            records,
            pos: [ch(POS[0])?, ch(POS[1])?, ch(POS[2])?],
            rot: [ch(ROT[0])?, ch(ROT[1])?, ch(ROT[2])?],
            scl: [ch(SCL[0])?, ch(SCL[1])?, ch(SCL[2])?],
            rate: Varied::read(p, RATE, RATEVAR, &what)?,
            life: Varied::read(p, LIFE, LIFEVAR, &what)?,
            speed: Varied::read(p, SPEED, SPEEDVAR, &what)?,
            spread: Varied::read(p, SPREAD, SPREADVAR, &what)?,
            inheritvel: Varied::read(p, INHERITVEL, INHERITVELVAR, &what)?,
            size: Resampled::read(p, SIZE, SIZEVAR, &what)?,
            mass: Factor::read(p, MASS, MASSVAR, &what)?,
            gravity: Factor::read(p, FORCEGRAVITY, FORCEGRAVITYVAR, &what)?,
            drag: Factor::read(p, FORCEDRAG, FORCEDRAGVAR, &what)?,
            wind: Factor::read(p, FORCEWIND, FORCEWINDVAR, &what)?,
            attractor: Factor::read(p, FORCE_ATTRACTOR_FACTOR, FORCE_ATTRACTOR_FACTORVAR, &what)?,
            vortex: Factor::read(p, FORCE_VORTEX_FACTOR, FORCE_VORTEX_FACTORVAR, &what)?,
            // Both read at t = 0, so a curve gives its first sample only (`0x00724620`).
            spin: first(SPIN, SPINVAR)?,
            start_angle: first(START_ANGLE, START_ANGLEVAR)?,
            colr: e.particle.colr.clone(),
            velocity_aligned: e.particle.flags & 1 != 0,
            flipbook: e.particle.flags & 2 != 0,
            wrap: (scale * frames.len() as u32) & 0xFF,
            scale,
            frames,
            accum: 0.0,
            particles: Vec::new(),
        })
    }

    /// The emitter's frame at `t` in the instance's: `S · R · T` (`FUN_004916f0`), channels without
    /// variance.
    fn matrix(&self, t: f32, instance: Mat4) -> Mat4 {
        let v = |c: &[Linear; 3]| [c[0].at(t), c[1].at(t), c[2].at(t)];
        instance * Mat4::from_scale_rotation_translation(Vec3::from(v(&self.scl)), euler_degrees(v(&self.rot)), Vec3::from(v(&self.pos)))
    }

    /// The rectangle a particle draws.
    fn rect(&self, p: &Particle) -> [f32; 4] {
        let n = self.frames.len();
        let i = if self.flipbook {
            (p.frame / self.scale) as usize % n
        } else {
            let slot = ((p.age01.clamp(0.0, 1.0) * (COLR_KEYS - 1) as f32) as usize).min(COLR_KEYS - 1);
            slot * n / COLR_KEYS
        };
        self.frames[i]
    }

    /// Spawn this update's particles (`FUN_0048ae80`), at most `room`.
    fn spawn(&mut self, f: &SpawnFrame, room: usize, rng: &mut u32) -> usize {
        self.accum += (self.rate.roll(f.t, rng).max(0.0) + f.moved * f.per_metre) * f.dt;
        let n = (self.accum.min(MAX_SPAWN_PER_UPDATE) as usize).min(room);
        let Some(records) = self.records.as_ref().filter(|r| !r.is_empty()) else {
            self.accum = 0.0;
            return 0;
        };
        let spread = self.spread.roll(f.t, rng) * DEGREES;
        let at = self.matrix(f.t, f.instance);
        for _ in 0..n {
            let rec = records[xorshift(rng) as usize % records.len()];
            let point = |i: usize| Vec3::new(rec[i], rec[i + 1], rec[i + 2]);
            let (normal, p, a, b) = (point(1), point(4), point(7), point(10));
            let s = rand01(rng);
            let r = rand01(rng) * (1.0 - s);
            let world = at.transform_point3(p + a * s + b * r);
            let mut dir = at.transform_vector3(normal).normalize_or_zero();
            if dir.length_squared() < MIN_DIRECTION_SQUARED {
                dir = Vec3::Y;
            }
            let theta = rand01(rng) * spread;
            let phi = rand01(rng) * std::f32::consts::TAU;
            let random = (xorshift(rng) >> 16) as u8;
            let particle = Particle {
                pos: world - f.delta * rand01(rng),
                vel: perturb(dir, theta, phi) * self.speed.roll(f.t, rng) + f.velocity * self.inheritvel.roll(f.t, rng) * 0.01,
                age: 0.0,
                life: self.life.roll(f.t, rng).max(0.0),
                age01: 0.0,
                random,
                frame: random as u32,
            };
            self.particles.push(particle);
        }
        self.accum = (self.accum - n as f32).max(0.0);
        n
    }
}

#[derive(Debug, Clone)]
enum ForceRead {
    Gravity,
    Wind,
    Drag { radius: f32 },
    Attractor { local: bool, range: f32, decay: f32 },
    Vortex { local: bool, range: f32, decay: f32, radial: f32 },
}

#[derive(Debug, Clone)]
struct ForceSim {
    read: ForceRead,
    ampl: Linear,
    pos: [Linear; 3],
    rot: [Linear; 3],
}

/// One force as this update sees it.
struct ForceNow {
    ampl: f32,
    /// The force's frame, and where its origin lands.
    to_world: Mat4,
    origin: Vec3,
}

/// `falloff`: `t = r / range`; `decay ≥ 0` is `(1 − t)^decay`, else `t^(−decay)`.
fn falloff(r: f32, range: f32, decay: f32) -> f32 {
    let t = r / range;
    if decay >= 0.0 {
        (1.0 - t).powf(decay)
    } else {
        t.powf(-decay)
    }
}

/// A running effect.
#[derive(Debug, Clone)]
pub struct EffectSim {
    emitters: Vec<EmitterSim>,
    forces: Vec<ForceSim>,
    starter: Starter,
    /// Any emitter's `emissiondir` is not 0: the instance's rotation counts for nothing.
    identity_rotation: bool,
    position: Vec3,
    rotation: Quat,
    last_position: Vec3,
    stepped: bool,
    time: f32,
    seed: u32,
    rng: u32,
    pub wind: GlobalWind,
}

impl EffectSim {
    /// An instance of `effect` at `position`, its frames resolved through `rects` (the `fxdict`).
    pub fn new(effect: &EffectContainer, rects: &HashMap<u32, FxRect>, starter: Starter, position: Vec3, rotation: Quat, seed: u32) -> Result<EffectSim, String> {
        effect.check_emitter_shapes()?;
        let mut emitters = Vec::with_capacity(effect.emitters.len());
        let mut identity_rotation = false;
        for i in 0..effect.emitters.len() {
            emitters.push(EmitterSim::read(effect, i, rects)?);
            identity_rotation |= value_of(find(&effect.emitters[i].particle.attributes, EMISSIONDIR, "an emitter")?) != 0.0;
        }
        let mut forces = Vec::with_capacity(effect.forces.len());
        for f in &effect.forces {
            let a = &f.attributes;
            let what = "a force";
            let number = |h: u32| find(a, h, what).map(value_of);
            let read = match f.kind {
                ForceKind::Gravity { .. } => ForceRead::Gravity,
                ForceKind::Wind { .. } => ForceRead::Wind,
                ForceKind::Drag { .. } => ForceRead::Drag { radius: number(DRAG_RADIUS)? },
                ForceKind::Attractor { .. } => ForceRead::Attractor { local: number(LOCAL)? != 0.0, range: number(RANGE)?, decay: number(DECAY)? },
                ForceKind::Vortex { .. } => {
                    ForceRead::Vortex { local: number(LOCAL)? != 0.0, range: number(RANGE)?, decay: number(DECAY)?, radial: number(RADIAL)? }
                }
            };
            let ch = |h: u32| Linear::read(a, h, what);
            forces.push(ForceSim {
                read,
                ampl: ch(AMPL)?,
                pos: [ch(POS[0])?, ch(POS[1])?, ch(POS[2])?],
                rot: [ch(ROT[0])?, ch(ROT[1])?, ch(ROT[2])?],
            });
        }
        let seed = seed | 1;
        Ok(EffectSim {
            emitters,
            forces,
            starter,
            identity_rotation,
            position,
            rotation,
            last_position: position,
            stepped: false,
            time: 0.0,
            seed,
            rng: seed,
            wind: GlobalWind::CAPTURED,
        })
    }

    /// The emitter clock, seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    pub fn live_particles(&self) -> usize {
        self.emitters.iter().map(|e| e.particles.len()).sum()
    }

    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Move the instance: the next update's displacement spawns along the path and gives particles
    /// their inherited velocity.
    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    /// Whether the instance has ended (`FUN_0048f4f0`).
    pub fn is_finished(&self) -> bool {
        let quiet = self.live_particles() == 0;
        if self.starter.duration > 0.0 {
            quiet && self.time >= self.starter.duration
        } else {
            quiet && self.time > IDLE_END_SECONDS
        }
    }

    /// Back to time 0 with the same random stream, so a replay is the same run.
    pub fn restart(&mut self) {
        self.time = 0.0;
        self.rng = self.seed;
        self.stepped = false;
        self.last_position = self.position;
        for e in &mut self.emitters {
            e.accum = 0.0;
            e.particles.clear();
        }
    }

    /// Replay to `t` seconds in steps of `dt`.
    pub fn seek(&mut self, t: f32, dt: f32) {
        self.restart();
        while self.time < t {
            self.step(dt);
        }
    }

    fn instance(&self) -> Mat4 {
        let rotation = if self.identity_rotation { Quat::IDENTITY } else { self.rotation };
        Mat4::from_rotation_translation(rotation, self.position)
    }

    /// One update of `dt` seconds: spawn at the current clock, advance it, step every particle.
    pub fn step(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }
        let instance = self.instance();
        let delta = self.position - self.last_position;
        let velocity = if !self.stepped {
            Vec3::ZERO
        } else if delta.length() > dt * MAX_INSTANCE_SPEED {
            delta.normalize() * MAX_INSTANCE_SPEED / dt
        } else {
            delta / dt
        };
        let t = self.time;
        let frame = SpawnFrame {
            t,
            dt,
            moved: delta.length().min(dt * MAX_INSTANCE_SPEED),
            per_metre: self.starter.per_metre,
            delta,
            velocity,
            instance,
        };
        let forces: Vec<ForceNow> = self.forces.iter().map(|f| Self::force_now(f, t, instance)).collect();
        if self.starter.duration <= 0.0 || t < self.starter.duration {
            let mut room = MAX_LIVE_PARTICLES.saturating_sub(self.live_particles());
            for e in &mut self.emitters {
                room -= e.spawn(&frame, room, &mut self.rng);
            }
        }
        self.time += dt;
        let wind = self.wind;
        for e in &mut self.emitters {
            step_particles(e, &self.forces, &forces, dt, wind);
        }
        self.last_position = self.position;
        self.stepped = true;
    }

    fn force_now(f: &ForceSim, t: f32, instance: Mat4) -> ForceNow {
        let v = |c: &[Linear; 3]| [c[0].at(t), c[1].at(t), c[2].at(t)];
        let to_world = instance * Mat4::from_rotation_translation(euler_degrees(v(&f.rot)), Vec3::from(v(&f.pos)));
        ForceNow { ampl: f.ampl.at(t), to_world, origin: to_world.w_axis.truncate() }
    }

    /// Every live particle as a sprite. `right`, `up` and `forward` are the camera's, for the
    /// velocity-aligned ones.
    pub fn sprites(&self, right: Vec3, up: Vec3, forward: Vec3, mut out: impl FnMut(Sprite)) {
        // `FUN_00493d50`: camera right and −up turned 45° about forward.
        let eighth = Quat::from_axis_angle(forward, std::f32::consts::FRAC_PI_4);
        let (right45, up45) = (eighth * right, eighth * -up);
        for e in &self.emitters {
            for p in &e.particles {
                let u = unit_of(p.random);
                let angle = (e.start_angle.at(u) + p.age * e.spin.at(u)) * DEGREES;
                let (mut sin, mut cos) = angle.sin_cos();
                if e.velocity_aligned {
                    let (cx, cy) = (right45.dot(p.vel), up45.dot(p.vel));
                    let l = (cx * cx + cy * cy).sqrt();
                    if l > 0.0 {
                        (sin, cos) = (cy / l, cx / l);
                    }
                }
                let key = ((p.age01.clamp(0.0, 1.0) * (COLR_KEYS - 1) as f32) as usize).min(COLR_KEYS - 1);
                out(Sprite {
                    pos: p.pos.to_array(),
                    size: e.size.at(p.age01) + e.size.var * u,
                    color: e.colr.sample(p.age01),
                    rect: e.rect(p),
                    sin,
                    cos,
                    intensity: binary16(e.colr.keys[key].half_bits),
                });
            }
        }
    }
}

/// `FUN_00876840` over one emitter's particles.
fn step_particles(e: &mut EmitterSim, forces: &[ForceSim], now: &[ForceNow], dt: f32, wind: GlobalWind) {
    let (wx, wz) = (wind.direction[0], wind.direction[1]);
    let wind_length = (wx * wx + wz * wz).sqrt();
    let wind_ampl = wind_length * wind.speed * 100.0;
    let wind_dir = if wind_length > 0.0 { Vec3::new(wx / wind_length, 0.0, wz / wind_length) } else { Vec3::ZERO };
    let mut i = 0;
    while i < e.particles.len() {
        let mut p = e.particles[i];
        if p.age > p.life {
            e.particles.swap_remove(i);
            continue;
        }
        p.age01 = if p.life > 0.0 { p.age / p.life } else { 1.0 };
        let u = unit_of(p.random);
        let mass = e.mass.at(u);
        let (mut force, mut c, mut wind_sum) = (Vec3::ZERO, 0.0f32, 0.0f32);
        for (f, n) in forces.iter().zip(now) {
            match f.read {
                ForceRead::Gravity => force += Vec3::NEG_Y * n.ampl * e.gravity.at(u) * mass,
                ForceRead::Wind => {
                    force += wind_dir * wind_ampl * e.wind.at(u);
                    wind_sum += wind_ampl.sqrt();
                }
                ForceRead::Drag { radius } => c += n.ampl * radius * radius * std::f32::consts::PI * e.drag.at(u),
                ForceRead::Attractor { local, range, decay } => {
                    let d = p.pos - n.origin;
                    let r = d.length();
                    let s = e.attractor.at(u);
                    if !local {
                        force += d * (s * n.ampl) / (r * r + FORCE_SOFTENING);
                    } else if r <= range {
                        force += d * (s * n.ampl * falloff(r, range, decay)) / (r * r * r + FORCE_SOFTENING);
                    }
                }
                ForceRead::Vortex { local, range, decay, radial } => {
                    let d = p.pos - n.origin;
                    let r = d.length();
                    let s = e.vortex.at(u);
                    if !local {
                        force += (d.cross(p.vel) + d.normalize_or_zero() * radial) * s * n.ampl / (r * r * r + FORCE_SOFTENING);
                    } else if r <= range {
                        let to_local = n.to_world.inverse();
                        let x = to_local.transform_point3(p.pos);
                        let v = to_local.transform_vector3(p.vel);
                        let w = Vec3::new(n.ampl * falloff(r, range, decay), 0.0, 0.0);
                        // `x.x` in Z is what the code does.
                        let spin = Vec3::new(0.0, x.y, x.x).normalize_or_zero() * radial * 5.0;
                        force += n.to_world.transform_vector3(w.cross(v) + spin) * s;
                    }
                }
            }
        }
        if wind_sum > 0.0 {
            c *= wind_sum;
        }
        let speed = p.vel.length();
        if mass < c * dt * speed {
            c = mass / (speed * dt);
        }
        force -= p.vel * c * speed;
        p.vel += force * ((dt / mass) / (1.0 + c * dt / mass));
        p.age += dt;
        if e.wrap != 0 {
            p.frame = (p.frame + 1) % e.wrap;
        }
        p.pos += p.vel * dt;
        e.particles[i] = p;
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::fxdict::{
        AttrDef, Emitter, EmitterGeom, EmitterShape, Force, ParticleType, Text, ValueKind, FRCE_ATTRACTOR_ATTRIBUTES,
        FRCE_COMMON_ATTRIBUTES, FRCE_DRAG_ATTRIBUTES, FRCE_VORTEX_ATTRIBUTES, PTYP_ATTRIBUTES_AFTER_COLR,
        PTYP_ATTRIBUTES_BEFORE_COLR, TRFM_CHANNELS,
    };

    const DT: f32 = 1.0 / 60.0;
    /// A point at the origin, its normal +Y.
    const POINT: [f32; 13] = [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

    fn zero(d: &AttrDef) -> Atrb {
        match d.kind {
            ValueKind::F32 => Atrb::f32(d.hash, 0.0),
            ValueKind::U32 => Atrb::u32(d.hash, 0),
        }
    }

    /// An effect of one emitter, `set` applied to its `PTYP` attributes by hash; scale 1, mass 1.
    fn effect(set: &[(u32, f32)], forces: Vec<Force>) -> EffectContainer {
        let mut attributes: Vec<Atrb> = PTYP_ATTRIBUTES_BEFORE_COLR.iter().chain(PTYP_ATTRIBUTES_AFTER_COLR.iter()).map(zero).collect();
        let mut put = |hash: u32, v: f32| {
            attributes.iter_mut().find(|a| a.hash == hash).expect("a PTYP attribute").value = AtrbValue::F32(v);
        };
        put(MASS, 1.0);
        for (h, v) in set {
            put(*h, *v);
        }
        let channels = TRFM_CHANNELS.iter().map(|d| if SCL.contains(&d.hash) { Atrb::f32(d.hash, 1.0) } else { zero(d) }).collect();
        EffectContainer {
            shapes: vec![EmitterShape { records: vec![POINT] }],
            emitters: vec![Emitter {
                transform: [[0.0; 4]; 4],
                channels,
                geom: Some(EmitterGeom { shape_index: 0, word_00: 1 }),
                particle: ParticleType { flags: 0, attributes, colr: Colr::uniform([255, 255, 255, 255], 0x3C00), text: Text { frames: vec![7, 8] } },
            }],
            forces,
        }
    }

    fn force(kind: ForceKind, extra: &[AttrDef], ampl: f32, set: &[(u32, f32)]) -> Force {
        let mut attributes: Vec<Atrb> = FRCE_COMMON_ATTRIBUTES.iter().chain(extra.iter()).map(zero).collect();
        attributes[0].value = AtrbValue::F32(ampl);
        for (h, v) in set {
            attributes.iter_mut().find(|a| a.hash == *h).expect("a FRCE attribute").value = AtrbValue::F32(*v);
        }
        Force { kind, attributes }
    }

    fn gravity(ampl: f32) -> Force {
        force(ForceKind::Gravity { magnitude: 0.0, direction: [0.0, 1.0, 0.0] }, &[], ampl, &[])
    }

    fn sim(e: &EffectContainer) -> EffectSim {
        EffectSim::new(e, &HashMap::new(), Starter::default(), Vec3::ZERO, Quat::IDENTITY, 12345).unwrap_or_else(|x| panic!("{x}"))
    }

    fn only_particle(s: &EffectSim) -> Particle {
        let list: Vec<Particle> = s.emitters.iter().flat_map(|e| e.particles.iter().copied()).collect();
        assert_eq!(list.len(), 1);
        list[0]
    }

    /// A burst of one: a `rate` curve that spawns on the first update, then stops.
    fn one_particle(mut e: EffectContainer) -> EffectContainer {
        let a = e.emitters[0].particle.attributes.iter_mut().find(|a| a.hash == RATE).expect("rate");
        *a = Atrb::f32(RATE, 0.0).with_curve(vec![AnimKey { time: 0.0, value: 1.5 / DT }, AnimKey { time: 0.5, value: 1.5 / DT }, AnimKey { time: 0.6, value: 0.0 }]);
        e
    }

    /// Linear keys are 60 Hz frames: 100 is 1.67 s, and the last key then holds for ever.
    #[test]
    fn a_linear_curve_reads_seconds_times_sixty_and_holds_its_last_key() {
        let keys = [AnimKey { time: 0.0, value: 0.0 }, AnimKey { time: 60.0, value: 10.0 }, AnimKey { time: 100.0, value: 30.0 }];
        let c = Linear { value: 99.0, keys: Some(keys.to_vec()) };
        assert_eq!(c.at(0.5), 5.0);
        assert!((c.at(1.0) - 10.0).abs() < 1e-4);
        assert!((c.at(1.0 + 20.0 / 60.0) - 20.0).abs() < 1e-3);
        assert_eq!(c.at(1000.0), 30.0);
        assert_eq!(Linear { value: 99.0, keys: None }.at(5.0), 99.0);
        assert_eq!(Linear { value: 99.0, keys: Some(vec![]) }.at(5.0), 0.0);
    }

    /// A resampled curve's keys are percent of life.
    #[test]
    fn a_resampled_curve_reads_percent_of_life() {
        let a = Atrb::f32(SIZE, 1.0).with_curve(vec![AnimKey { time: 0.0, value: 0.0 }, AnimKey { time: 100.0, value: 10.0 }]);
        let attrs = [a, Atrb::f32(SIZEVAR, 0.0)];
        let r = Resampled::read(&attrs, SIZE, SIZEVAR, "t").unwrap();
        assert!((r.at(0.5) - 4.95).abs() < 1e-4, "{}", r.at(0.5));
        assert!((r.at(1.0) - 9.9).abs() < 1e-4, "99 is the last sample: {}", r.at(1.0));
    }

    #[test]
    fn rate_is_particles_per_second() {
        let mut s = sim(&effect(&[(RATE, 100.0), (LIFE, 100.0)], vec![]));
        for _ in 0..60 {
            s.step(DT);
        }
        assert!((s.live_particles() as i32 - 100).abs() <= 1, "{}", s.live_particles());
    }

    /// Live: −2.0043 for ampl 2; mass 2 falls as mass 1 does.
    #[test]
    fn gravity_ignores_mass_and_its_direction_and_scales_by_its_factor() {
        for (mass, factor) in [(1.0, 1.0), (2.0, 1.0), (1.0, 0.5)] {
            let mut s = sim(&one_particle(effect(&[(LIFE, 100.0), (MASS, mass), (FORCEGRAVITY, factor)], vec![gravity(10.0)])));
            s.step(DT);
            let v0 = only_particle(&s).vel.y;
            for _ in 0..59 {
                s.step(DT);
            }
            let v = only_particle(&s).vel.y - v0;
            assert!((v - -10.0 * factor * 59.0 * DT).abs() < 0.05, "mass {mass} factor {factor}: {v}");
        }
    }

    /// `ampl · r² · π`, quadratic in speed: a falling particle settles where it balances gravity.
    #[test]
    fn drag_is_quadratic_in_speed_and_reads_its_radius() {
        let drag = force(ForceKind::Drag { magnitude: 0.0 }, &FRCE_DRAG_ATTRIBUTES, 75.0, &[(DRAG_RADIUS, 0.1)]);
        let mut s = sim(&one_particle(effect(&[(LIFE, 100.0), (FORCEGRAVITY, 1.0), (FORCEDRAG, 1.0)], vec![gravity(10.0), drag])));
        for _ in 0..600 {
            s.step(DT);
        }
        let want = (10.0 / (75.0 * 0.01 * std::f32::consts::PI)).sqrt();
        let got = -only_particle(&s).vel.y;
        assert!((got - want).abs() < 0.01, "{got} vs terminal {want}");
    }

    #[test]
    fn wind_reads_the_world_not_its_record() {
        let wind = force(ForceKind::Wind { magnitude: 500.0, direction: [0.0, 1.0, 0.0] }, &[], 500.0, &[]);
        let mut s = sim(&one_particle(effect(&[(LIFE, 100.0), (FORCEWIND, 1.0)], vec![wind])));
        s.wind = GlobalWind { direction: [1.0, 0.0], speed: 0.01 };
        for _ in 0..60 {
            s.step(DT);
        }
        let v = only_particle(&s).vel;
        assert!((v.x - 1.0).abs() < 0.05 && v.y == 0.0 && v.z == 0.0, "{v}");
    }

    /// Positive repels; the `local` form stops at its range.
    #[test]
    fn an_attractor_repels_inside_its_range_only() {
        let attractor = |range: f32| {
            force(
                ForceKind::Attractor { magnitude: 0.0, flag_13c: 1, param_130: 0.0, param_134: 0.0, vector_104: [0.0; 3] },
                &FRCE_ATTRACTOR_ATTRIBUTES,
                10.0,
                &[(LOCAL, 1.0), (RANGE, range)],
            )
        };
        let start = |range: f32| {
            let mut e = one_particle(effect(&[(LIFE, 100.0), (FORCE_ATTRACTOR_FACTOR, 1.0)], vec![attractor(range)]));
            e.shapes[0].records[0][4] = 1.0;
            let mut s = sim(&e);
            for _ in 0..10 {
                s.step(DT);
            }
            only_particle(&s)
        };
        assert!(start(5.0).vel.x > 0.0);
        assert_eq!(start(0.5).vel.x, 0.0);
    }

    /// The global form bends a moving particle about its centre.
    #[test]
    fn a_vortex_turns_a_moving_particle() {
        let kind = ForceKind::Vortex { magnitude: 0.0, flag_13c: 0, param_130: 0.0, param_134: 0.0, param_138: 0.0, vector_104: [0.0; 3], vector_110: [0.0; 3], vector_11c: [0.0; 3] };
        let vortex = force(kind, &FRCE_VORTEX_ATTRIBUTES, 10.0, &[]);
        let mut e = one_particle(effect(&[(LIFE, 100.0), (SPEED, 1.0), (FORCE_VORTEX_FACTOR, 1.0)], vec![vortex]));
        e.shapes[0].records[0][4] = 1.0;
        let mut s = sim(&e);
        for _ in 0..10 {
            s.step(DT);
        }
        assert!(only_particle(&s).vel.z.abs() > 0.01);
    }

    #[test]
    fn spread_stays_inside_its_cone_in_degrees() {
        let mut s = sim(&effect(&[(RATE, 2000.0), (LIFE, 100.0), (SPEED, 1.0), (SPREAD, 30.0)], vec![]));
        for _ in 0..30 {
            s.step(DT);
        }
        let list = &s.emitters[0].particles;
        assert!(list.len() > 500);
        let least = list.iter().map(|p| p.vel.normalize().dot(Vec3::Y)).fold(1.0, f32::min);
        assert!(least >= 30f32.to_radians().cos() - 1e-3, "{least}");
        assert!(least < 0.9, "the cone is used: {least}");
    }

    /// With no duration an effect ends past 3 s with no particle; with one it stops emitting there.
    #[test]
    fn an_effect_ends_on_its_duration_or_past_three_seconds() {
        let mut s = sim(&one_particle(effect(&[(LIFE, 0.5)], vec![])));
        while s.time() < 2.9 {
            s.step(DT);
            assert!(!s.is_finished());
        }
        while s.time() < 3.1 {
            s.step(DT);
        }
        assert!(s.is_finished());

        let e = effect(&[(RATE, 100.0), (LIFE, 0.1)], vec![]);
        let starter = Starter { duration: 1.0, per_metre: 0.0 };
        let mut s = EffectSim::new(&e, &HashMap::new(), starter, Vec3::ZERO, Quat::IDENTITY, 9).unwrap();
        while s.time() < 0.95 {
            s.step(DT);
            assert!(!s.is_finished());
        }
        while s.time() < 1.5 {
            s.step(DT);
        }
        assert!(s.is_finished());
    }

    /// A held `rate` never ends.
    #[test]
    fn a_held_rate_never_ends() {
        let mut s = sim(&effect(&[(RATE, 50.0), (LIFE, 0.5)], vec![]));
        for _ in 0..1200 {
            s.step(DT);
        }
        assert!(!s.is_finished() && s.live_particles() > 0);
    }

    #[test]
    fn colr_keys_carry_the_multiplier_and_its_sign_adds() {
        let mut e = one_particle(effect(&[(LIFE, 10.0), (SIZE, 2.0)], vec![]));
        e.emitters[0].particle.colr = Colr::uniform([255, 128, 0, 255], 0xC000);
        let mut s = sim(&e);
        s.step(DT);
        let mut got = Vec::new();
        s.sprites(Vec3::X, Vec3::Y, Vec3::Z, |x| got.push(x));
        assert_eq!(got[0].intensity, -2.0);
        assert_eq!(got[0].size, 2.0);
        assert!((got[0].color[1] - 128.0 / 255.0).abs() < 1e-5);
    }

    #[test]
    fn binary16_decodes_the_retail_values() {
        assert_eq!(binary16(0x3C00), 1.0);
        assert_eq!(binary16(0xBC00), -1.0);
        assert_eq!(binary16(0xC500), -5.0);
        assert_eq!(binary16(0xCD00), -20.0);
    }

    /// Bit 1 clear: the frames span the life. Bit 1 set: one per sim update and `scale`.
    #[test]
    fn frames_span_the_life_or_flip_per_update() {
        let rects: HashMap<u32, FxRect> =
            [(7, FxRect { key: 7, u: 0.0, v: 0.0, w: 0.5, h: 0.5 }), (8, FxRect { key: 8, u: 0.5, v: 0.0, w: 0.5, h: 0.5 })].into_iter().collect();
        let e = one_particle(effect(&[(LIFE, 1.0)], vec![]));
        let mut s = EffectSim::new(&e, &rects, Starter::default(), Vec3::ZERO, Quat::IDENTITY, 3).unwrap();
        s.step(DT);
        let em = &s.emitters[0];
        let mut p = em.particles[0];
        p.age01 = 0.1;
        assert_eq!(em.rect(&p)[0], 0.0);
        p.age01 = 0.9;
        assert_eq!(em.rect(&p)[0], 0.5);

        let mut e = e;
        e.emitters[0].particle.flags = 2;
        e.emitters[0].particle.attributes.iter_mut().find(|a| a.hash == SCALE).unwrap().value = AtrbValue::U32(3);
        let mut s = EffectSim::new(&e, &rects, Starter::default(), Vec3::ZERO, Quat::IDENTITY, 3).unwrap();
        s.step(DT);
        assert_eq!(s.emitters[0].wrap, 6);
        let frame = s.emitters[0].particles[0].frame;
        s.emitters[0].accum = 0.0;
        s.emitters[0].rate = Varied { base: Linear { value: 0.0, keys: None }, var: Linear { value: 0.0, keys: None } };
        s.step(DT);
        assert_eq!(s.emitters[0].particles[0].frame, (frame + 1) % 6);
    }

    #[test]
    fn a_missed_frame_key_draws_the_whole_atlas() {
        let mut s = sim(&one_particle(effect(&[(LIFE, 1.0)], vec![])));
        s.step(DT);
        assert_eq!(s.emitters[0].rect(&s.emitters[0].particles[0]), [0.0, 0.0, 1.0, 1.0]);
    }

    /// Spin in degrees a second on top of the start angle.
    #[test]
    fn a_sprite_turns_by_its_spin() {
        let mut s = sim(&one_particle(effect(&[(LIFE, 10.0), (SPIN, 90.0)], vec![])));
        for _ in 0..60 {
            s.step(DT);
        }
        let mut got = Vec::new();
        s.sprites(Vec3::X, Vec3::Y, Vec3::Z, |x| got.push(x));
        assert!((got[0].sin - 1.0).abs() < 0.01 && got[0].cos.abs() < 0.05, "{got:?}");
    }

    /// A moving instance hands on `inheritvel` percent of its velocity.
    #[test]
    fn particles_inherit_a_percentage_of_the_instance_velocity() {
        let mut s = sim(&effect(&[(RATE, 600.0), (LIFE, 100.0), (INHERITVEL, 50.0)], vec![]));
        s.step(DT);
        s.emitters[0].particles.clear();
        s.set_position(Vec3::new(1.0 / 6.0, 0.0, 0.0));
        s.step(DT);
        assert!(!s.emitters[0].particles.is_empty());
        assert!((s.emitters[0].particles[0].vel.x - 5.0).abs() < 1e-3);
    }

    /// A replay is the same run, which is what the scrub stands on.
    #[test]
    fn a_seek_replays_the_same_run() {
        let e = effect(&[(RATE, 50.0), (LIFE, 2.0), (SPEED, 3.0), (SPREAD, 40.0)], vec![gravity(5.0)]);
        let mut s = sim(&e);
        s.seek(1.0, DT);
        let first: Vec<Vec3> = s.emitters[0].particles.iter().map(|p| p.pos).collect();
        s.seek(1.0, DT);
        let second: Vec<Vec3> = s.emitters[0].particles.iter().map(|p| p.pos).collect();
        assert!(!first.is_empty());
        assert_eq!(first, second);
    }

    /// The pool holds 15,000.
    #[test]
    fn the_live_pool_caps_at_fifteen_thousand() {
        let mut s = sim(&effect(&[(RATE, 1_000_000.0), (LIFE, 100.0)], vec![]));
        for _ in 0..30 {
            s.step(DT);
        }
        assert_eq!(s.live_particles(), MAX_LIVE_PARTICLES);
    }
}
