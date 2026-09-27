//! 3D listeners and the engine's positional mix maths, read from the disassembly of the unpacked PC
//! exe and its runtime memory dump.
//!
//! * **Listeners** (`PalSoundEngine`, `0x019C6170`): four slots of `0x60` bytes from `0x019C6190`, each
//!   a D3D row-major world matrix (rows 0–2 the basis, row 3 the position, `+0x30`) and a velocity
//!   (`+0x40`). `SetListener` (`FUN_00836230`) copies both in; the game builds the matrix from the
//!   camera's normalised quaternion (`D3DXMatrixRotationQuaternion`) and position (`FUN_00606560`).
//!   `GetClosestListener` (`FUN_00836280`) picks the nearest active slot, but the **mix reads slot 0
//!   only**: `FUN_00838850`, `FUN_0083ade0` and `FUN_0083d090` address `0x019C61C0` (slot 0's
//!   position) and `0x019C61D0` (its velocity) directly.
//! * **Speaker gains** of an emitter source ([`speaker_gains`], `FUN_0083d090`).
//! * **Doppler** of an emitter source ([`source_doppler`], `FUN_0083ade0`) and of a wave in it
//!   ([`wave_doppler`], `FUN_0083b120` and `FUN_00839ae0`).
//! * **Distance volume** of a positional wave ([`distance_volume`], `FUN_0083d3a0`) at the emitter's
//!   distance to listener 0 ([`listener_distance`], `FUN_00838850`).
//!
//! All arithmetic is single precision in the order the exe performs it, except where a comment says
//! the exe computes on the x87 stack.

use mercs2_core::glam::{Mat4, Vec3};

/// Max simultaneous listeners (`FUN_00836280` walks 4; `engine+0x1c[4]` active flags).
pub const MAX_LISTENERS: usize = 4;

/// Speed of sound in metres/second — the constant behind the instance start-delay (`FUN_008369e0`).
pub const SPEED_OF_SOUND: f32 = 343.0;

/// The proximity radius inside which every speaker gets extra gain (`PalSoundEngine +0x1D8`,
/// `0x019C6348`): `FUN_00835fd0` copies it from the engine-init descriptor's `+0x14`, which
/// `FUN_006067b0` fills from `DAT_00DF6804` — 1.0 in the shipped image, replaced only by a
/// command-line option (`FUN_004c2c20`, option hash `0x21C3DCBE`) that is not modelled here.
pub const PROXIMITY_RADIUS: f32 = 1.0;

/// `DAT_00BEB460` (`0x3B3FA030`, ≈ 1/342): the Doppler factor per unit of closing speed.
pub const DOPPLER_PER_SPEED: f32 = f32::from_bits(0x3B3F_A030);

/// The five speaker directions in listener space (`0x019C67A0`, set once by `FUN_0083d090`): 0.7 is
/// `DAT_00DFDDD8`, −0.7 `DAT_00BEB45C`, −0.0 `DAT_00BEAA2C`. The table holds a sixth entry (0, 0, 0)
/// that is transformed and never read.
pub const SPEAKERS: [[f32; 3]; 5] = [
    [0.7, 0.0, 0.7],
    [-0.7, 0.0, 0.7],
    [0.7, 0.0, -0.7],
    [-0.7, 0.0, -0.7],
    [-0.0, 0.0, 1.0],
];

/// One audio listener: a listener slot's matrix rows and velocity.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    /// Whether this listener slot participates (`engine+0x1c[i]`).
    pub active: bool,
    /// World position (matrix row 3).
    pub position: Vec3,
    /// Matrix row 0 (the listener's +X), as stored — the mix does not normalise it.
    pub side: Vec3,
    /// Matrix row 1 (the listener's +Y).
    pub up: Vec3,
    /// Matrix row 2 (the listener's +Z, its facing).
    pub forward: Vec3,
    /// Velocity (slot `+0x40`).
    pub velocity: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            active: false,
            position: Vec3::ZERO,
            side: Vec3::X,
            up: Vec3::Y,
            forward: Vec3::Z, // canonical space: +Z north/forward (docs/coordinate_systems.md)
            velocity: Vec3::ZERO,
        }
    }
}

impl Listener {
    /// Set from a listener world matrix whose rows are glam's axes (a D3D row-major matrix loaded
    /// with `Mat4::from_cols_array`), as `SetListener` copies it.
    pub fn from_matrix(m: &Mat4) -> Listener {
        Listener {
            active: true,
            position: m.w_axis.truncate(),
            side: m.x_axis.truncate(),
            up: m.y_axis.truncate(),
            forward: m.z_axis.truncate(),
            velocity: Vec3::ZERO,
        }
    }

    /// `D3DXVec3TransformNormal(v, matrix)` as `d3dx9_36.dll`'s SSE2 path computes it (the path its
    /// CPU dispatch `0x00579941` installs when the processor reports SSE2 and not 3DNow!; entry
    /// `0x0074F5FE`): per component `(y × row1 + x × row0) + z × row2`, single precision.
    pub fn transform_normal(&self, v: [f32; 3]) -> [f32; 3] {
        let [x, y, z] = v;
        let (r0, r1, r2) = (self.side, self.up, self.forward);
        [
            (y * r1.x + x * r0.x) + z * r2.x,
            (y * r1.y + x * r0.y) + z * r2.y,
            (y * r1.z + x * r0.z) + z * r2.z,
        ]
    }
}

/// The listener set (`PalSoundEngine` listener array). Up to [`MAX_LISTENERS`] active.
#[derive(Clone, Debug)]
pub struct ListenerSet {
    listeners: [Listener; MAX_LISTENERS],
}

impl Default for ListenerSet {
    fn default() -> Self {
        let mut listeners = [Listener::default(); MAX_LISTENERS];
        listeners[0].active = true; // single-player: listener 0 always live
        ListenerSet { listeners }
    }
}

impl ListenerSet {
    /// `PalSoundEngine::SetListener` (`FUN_00836230`): install/replace listener `i`.
    pub fn set(&mut self, i: usize, l: Listener) {
        if i < MAX_LISTENERS {
            self.listeners[i] = l;
        }
    }

    /// Read-only view of an active listener slot.
    pub fn get(&self, i: usize) -> Option<&Listener> {
        self.listeners.get(i).filter(|l| l.active)
    }

    /// Slot 0 as stored, active or not — the listener the mix reads (module docs).
    pub fn mix_listener(&self) -> &Listener {
        &self.listeners[0]
    }

    /// Number of active listeners.
    pub fn active_count(&self) -> usize {
        self.listeners.iter().filter(|l| l.active).count()
    }

    /// `PalSoundEngine::GetClosestListener` (`FUN_00836280`): index + distance of the nearest active
    /// listener to `pos`. `None` if no listener is active.
    pub fn closest(&self, pos: Vec3) -> Option<(usize, f32)> {
        self.listeners
            .iter()
            .enumerate()
            .filter(|(_, l)| l.active)
            .map(|(i, l)| (i, l.position.distance(pos)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

/// `x` clamped to `[0, 1]` as the two `comiss` tests do it (`0 > x` gives 0, else `x > 1` gives 1):
/// NaN and −0.0 pass through, exactly as `f32::clamp` treats them.
fn clamp01_comiss(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// `FUN_0083d090`: the five speaker gains of an emitter source at `source`, heard by `listener`
/// (slot 0) with proximity radius `radius` ([`PROXIMITY_RADIUS`]).
///
/// The direction to the source is horizontal: `dx`, `dz` from the listener, `dist = sqrt(dz² + dx²)`,
/// `u = (dx / dist, 0 × (1 / dist), dz / dist)` (all zero at `dist == 0`). Within the radius
/// `prox = 1 − dist / radius`, else 0. Each speaker direction ([`SPEAKERS`]) goes to world space
/// ([`Listener::transform_normal`]) as `t`, and its gain is
/// `clamp01(clamp01((t.z × u.z + t.x × u.x) + u.y × t.y) + prox)`.
///
/// The source object stores the five at `+0x1C`..`+0x2C`; its commit (`FUN_0083afc0`) applies them to
/// output channels 0, 1, 4, 5 and 2 in that order (front left, front right, back left, back right,
/// centre), and channel 3 (LFE) takes `+0x30`, which only the source constructor (`0x0083446D`,
/// 0.0) and the 2D prepare (1.0) write.
pub fn speaker_gains(source: Vec3, listener: &Listener, radius: f32) -> [f32; 5] {
    let dx = source.x - listener.position.x;
    let dz = source.z - listener.position.z;
    let dist = (dz * dz + dx * dx).sqrt();
    let mut prox = 0.0f32;
    if radius > dist {
        prox = 1.0 - dist / radius;
    }
    let (ux, uy, uz) = if dist == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        let inv = 1.0 / dist;
        (inv * dx, inv * 0.0, dz * inv)
    };
    let mut out = [0.0f32; 5];
    for (o, v) in out.iter_mut().zip(SPEAKERS) {
        let t = listener.transform_normal(v);
        let dot = (t[2] * uz + t[0] * ux) + uy * t[1];
        let s = clamp01_comiss(dot) + prox;
        *o = clamp01_comiss(s);
    }
    out
}

/// `FUN_0083ade0`'s Doppler factor for an emitter source (source `+0x34`): with `u` the unit vector
/// from listener 0 to the source (`dist = sqrt((dx² + dz²) + dy²)`, all zero at `dist == 0`),
/// `1 − ((Δv.x × u.x + Δv.y × u.y) + Δv.z × u.z) × DOPPLER_PER_SPEED`, `Δv` the source's velocity
/// minus the listener's. A receding source gives less than 1.
pub fn source_doppler(position: Vec3, velocity: Vec3, listener: &Listener) -> f32 {
    let dx = position.x - listener.position.x;
    let dz = position.z - listener.position.z;
    let dy = position.y - listener.position.y;
    let dist = ((dx * dx + dz * dz) + dy * dy).sqrt();
    let (ux, uy, uz) = if dist == 0.0 {
        (0.0, 0.0, 0.0)
    } else {
        let inv = 1.0 / dist;
        (inv * dx, inv * dy, inv * dz)
    };
    let vx = velocity.x - listener.velocity.x;
    let vy = velocity.y - listener.velocity.y;
    let vz = velocity.z - listener.velocity.z;
    let dot = (vx * ux + vy * uy) + vz * uz;
    1.0 - dot * DOPPLER_PER_SPEED
}

/// A wave's Doppler factor in an emitter source: `FUN_0083b120` scales the source's factor `d` by the
/// wave's Doppler scale `w` (wave `+0x70`, its group's `+0x28`) when `w > 0` — `1 + (d − 1) × w`,
/// computed on the x87 stack and stored as single precision (the product of two singles is exact in
/// double, so double arithmetic gives the same value) — and passes 1.0 otherwise; `FUN_00839ae0`
/// clamps it to `[0.1, 2.0]` (`DAT_00B92B58`, `DAT_00B92874`) and stores it at wave `+0xA8`.
pub fn wave_doppler(source: f32, scale: f32) -> f32 {
    let v = if scale > 0.0 { ((f64::from(source) - 1.0) * f64::from(scale) + 1.0) as f32 } else { 1.0 };
    let lo = f32::from_bits(0x3DCC_CCCD);
    if lo > v {
        lo
    } else if v > 2.0 {
        2.0
    } else {
        v
    }
}

/// `FUN_00838850`: an emitter's distance to listener 0, `sqrt((dz² + dy²) + dx²)` with `d` the
/// listener's position minus the emitter's.
pub fn listener_distance(position: Vec3, listener: &Listener) -> f32 {
    let dz = listener.position.z - position.z;
    let dy = listener.position.y - position.y;
    let dx = listener.position.x - position.x;
    ((dz * dz + dy * dy) + dx * dx).sqrt()
}

/// `FUN_0083d3a0`: a positional wave's distance volume (wave `+0xAC` through vtable `+0xEC`), from
/// its group's minimum distance (`+0x18`), maximum distance (`+0x1C`) and exponent (`+0x24`): 1.0 up to
/// the minimum, 0.0 from the maximum on, and between them
/// `1 − (f32) pow((f64) ((dist − min) / (max − min)), (f64) exponent)`.
pub fn distance_volume(dist: f32, min: f32, max: f32, exponent: f32) -> f32 {
    if min >= dist {
        return 1.0;
    }
    if dist >= max {
        return 0.0;
    }
    let t = (dist - min) / (max - min);
    1.0 - f64::from(t).powf(f64::from(exponent)) as f32
}

/// Instance start delay in seconds (`PalSoundInstance::Start` `FUN_008369e0`, field `+0x70`):
/// distance to the closest listener divided by the speed of sound.
pub fn start_delay_secs(distance: f32) -> f32 {
    (distance / SPEED_OF_SOUND).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listener turned 90° about +Y: row 0 = −Z, row 1 = +Y, row 2 = +X.
    fn turned() -> Listener {
        Listener {
            active: true,
            position: Vec3::new(10.0, 2.0, -4.0),
            side: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::Y,
            forward: Vec3::X,
            velocity: Vec3::new(0.5, 0.0, -1.0),
        }
    }

    #[test]
    fn speakers_follow_the_traced_maths() {
        let l = turned();
        let src = Vec3::new(13.0, 7.0, -8.0);
        let got = speaker_gains(src, &l, 5.0);
        // Step by step as FUN_0083d090 computes it.
        let (dx, dz) = (13.0f32 - 10.0, -8.0f32 - -4.0);
        let dist = (dz * dz + dx * dx).sqrt();
        let prox = 1.0 - dist / 5.0;
        let inv = 1.0 / dist;
        let (ux, uy, uz) = (inv * dx, inv * 0.0, dz * inv);
        for (i, v) in SPEAKERS.iter().enumerate() {
            let t = [
                (v[1] * 0.0 + v[0] * 0.0) + v[2] * 1.0,
                (v[1] * 1.0 + v[0] * 0.0) + v[2] * 0.0,
                (v[1] * 0.0 + -v[0]) + v[2] * 0.0,
            ];
            let dot = (t[2] * uz + t[0] * ux) + uy * t[1];
            let want = (dot.clamp(0.0, 1.0) + prox).clamp(0.0, 1.0);
            assert_eq!(got[i].to_bits(), want.to_bits(), "speaker {i}");
        }
        // Out of the radius a speaker facing away is silent; straight ahead the centre is full.
        let ahead = speaker_gains(Vec3::new(40.0, 0.0, -4.0), &l, 1.0);
        assert_eq!(ahead[4], 1.0, "the centre faces +X here");
        assert_eq!(ahead[2], 0.0, "a back speaker");
        // On the listener: no direction, full proximity gain.
        assert_eq!(speaker_gains(l.position, &l, 1.0), [1.0; 5]);
    }

    #[test]
    fn doppler_follows_the_traced_maths() {
        let l = turned();
        let (p, v) = (Vec3::new(20.0, 2.0, -4.0), Vec3::new(30.0, 0.0, 0.0));
        let d = source_doppler(p, v, &l);
        let want = 1.0 - (((30.0f32 - 0.5) * 1.0 + 0.0 * 0.0) + (0.0f32 - -1.0) * 0.0) * DOPPLER_PER_SPEED;
        assert_eq!(d.to_bits(), want.to_bits());
        assert!(d < 1.0, "a receding source drops in pitch");
        assert_eq!(wave_doppler(d, 0.0), 1.0, "no scale, no Doppler");
        assert_eq!(wave_doppler(d, 0.5), ((f64::from(d) - 1.0) * 0.5 + 1.0) as f32);
        assert_eq!(wave_doppler(-5.0, 1.0), f32::from_bits(0x3DCC_CCCD), "clamped to 0.1");
        assert_eq!(wave_doppler(5.0, 1.0), 2.0, "clamped to 2");
    }

    #[test]
    fn distance_volume_follows_the_traced_maths() {
        assert_eq!(distance_volume(5.0, 5.0, 50.0, 2.0), 1.0, "at the minimum");
        assert_eq!(distance_volume(50.0, 5.0, 50.0, 2.0), 0.0, "at the maximum");
        let t = (20.0f32 - 5.0) / (50.0 - 5.0);
        assert_eq!(distance_volume(20.0, 5.0, 50.0, 2.0), 1.0 - f64::from(t).powf(2.0) as f32);
        let l = turned();
        let p = Vec3::new(13.0, 6.0, -8.0);
        let want = (((-4.0f32 - -8.0) * (-4.0 - -8.0) + (2.0f32 - 6.0) * (2.0 - 6.0)) + (10.0f32 - 13.0) * (10.0 - 13.0)).sqrt();
        assert_eq!(listener_distance(p, &l), want);
    }
}
