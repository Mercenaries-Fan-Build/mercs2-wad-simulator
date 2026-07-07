//! 3D listeners, distance attenuation, stereo pan and Doppler pitch.
//!
//! **Oracle:**
//! * `PalSoundEngine::GetClosestListener` **`FUN_00836280`** — 4 listeners, position is the
//!   translation column of each listener matrix at `engine+0x50 + i*0x60`; returns the closest index
//!   (audio_code_map.md §3.1, §8).
//! * `PalSoundInstance::GetWaveVolumeScale` **`FUN_00837f00`** — calls GetClosestListener, scales by
//!   distance.
//! * `PalSoundWaveDX8 CalculateVolume` **`FUN_0083ade0`** — Doppler pitch + per-listener channel gains
//!   (`DAT_00fc34b0`, §3.4).
//! * `PalSoundInstance::Start` **`FUN_008369e0`** — **start delay = distance-to-closest-listener ×
//!   inv-speed-of-sound** (stored at instance `+0x70`), so a far cue is heard late.
//!
//! The exe supports up to **4 simultaneous listeners** (split-screen co-op); a cue attenuates against
//! whichever is closest.

use mercs2_core::glam::{Mat4, Vec3};

/// Max simultaneous listeners (`FUN_00836280` walks 4; `engine+0x1c[4]` active flags).
pub const MAX_LISTENERS: usize = 4;

/// Speed of sound in metres/second — the constant behind the instance start-delay (`FUN_008369e0`)
/// and the Doppler ratio (`FUN_0083ade0`).
pub const SPEED_OF_SOUND: f32 = 343.0;

/// One audio listener: an oriented point in world space with a velocity (for Doppler).
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    /// Whether this listener slot participates (`engine+0x1c[i]`).
    pub active: bool,
    /// World position (matrix translation at `engine+0x50 + i*0x60`).
    pub position: Vec3,
    /// Forward (look) direction, unit length. Right = forward × up.
    pub forward: Vec3,
    /// Up direction, unit length.
    pub up: Vec3,
    /// Velocity for Doppler (`engine +0x60 + i*0x60`).
    pub velocity: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            active: false,
            position: Vec3::ZERO,
            forward: Vec3::Z, // canonical space: +Z north/forward (docs/coordinate_systems.md)
            up: Vec3::Y,
            velocity: Vec3::ZERO,
        }
    }
}

impl Listener {
    /// Set from a listener transform matrix (translation + basis), as the exe reads it from the
    /// `0x60`-stride listener block.
    pub fn from_matrix(m: &Mat4) -> Listener {
        Listener {
            active: true,
            position: m.w_axis.truncate(),
            forward: m.z_axis.truncate().normalize_or_zero(),
            up: m.y_axis.truncate().normalize_or_zero(),
            velocity: Vec3::ZERO,
        }
    }

    /// Right-hand basis vector (for stereo pan).
    pub fn right(&self) -> Vec3 {
        self.forward.cross(self.up).normalize_or_zero()
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

    /// Read-only view of a listener slot.
    pub fn get(&self, i: usize) -> Option<&Listener> {
        self.listeners.get(i).filter(|l| l.active)
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

/// Linear distance attenuation in `[0, 1]`: full within `min_dist`, ramping to silence at `max_dist`.
///
/// Models the exe's `MaxDistCheck` cull + distance volume scale (`GetWaveVolumeScale` `FUN_00837f00`).
/// A degenerate `max <= min` collapses to a hard on/off at `min`. This is the "falls off with
/// distance" contract the mixer applies to every positional voice.
pub fn distance_attenuation(dist: f32, min_dist: f32, max_dist: f32) -> f32 {
    if dist <= min_dist {
        return 1.0;
    }
    if max_dist <= min_dist || dist >= max_dist {
        return if dist >= max_dist { 0.0 } else { 1.0 };
    }
    // Linear roll-off between min and max.
    let t = (dist - min_dist) / (max_dist - min_dist);
    (1.0 - t).clamp(0.0, 1.0)
}

/// Constant-power stereo pan gains `(left, right)` for `source` heard by `listener`.
///
/// Azimuth is the source's angle in the listener's right/forward plane; a fully-right source →
/// `(0, 1)`, straight ahead → `(≈0.707, ≈0.707)` (constant-power law, matching the DX8 channel-gain
/// table `DAT_00fc34b0` behaviour, `FUN_0083ade0`).
pub fn stereo_pan(source: Vec3, listener: &Listener) -> (f32, f32) {
    let to_src = source - listener.position;
    let flat = to_src - listener.up * to_src.dot(listener.up); // project out the up axis
    if flat.length_squared() < 1e-8 {
        return (std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
    }
    let dir = flat.normalize();
    // pan in [-1, 1]: -1 fully left, +1 fully right.
    let pan = dir.dot(listener.right()).clamp(-1.0, 1.0);
    // constant-power: map pan∈[-1,1] to angle∈[0, π/2].
    let angle = (pan * 0.5 + 0.5) * std::f32::consts::FRAC_PI_2;
    (angle.cos(), angle.sin())
}

/// Doppler pitch ratio from relative radial velocity (`FUN_0083ade0`). `> 1` when the source and
/// listener close, `< 1` when they separate. Clamped to a sane musical range.
pub fn doppler_pitch(source_pos: Vec3, source_vel: Vec3, listener: &Listener) -> f32 {
    let to_listener = listener.position - source_pos;
    let dist = to_listener.length();
    if dist < 1e-4 {
        return 1.0;
    }
    let dir = to_listener / dist;
    // Positive radial velocity = closing.
    let v_src = source_vel.dot(dir);
    let v_lis = listener.velocity.dot(dir);
    let ratio = (SPEED_OF_SOUND + v_lis) / (SPEED_OF_SOUND - v_src).max(1.0);
    ratio.clamp(0.5, 2.0)
}

/// Instance start delay in seconds (`PalSoundInstance::Start` `FUN_008369e0`, field `+0x70`):
/// distance to the closest listener divided by the speed of sound.
pub fn start_delay_secs(distance: f32) -> f32 {
    (distance / SPEED_OF_SOUND).max(0.0)
}
