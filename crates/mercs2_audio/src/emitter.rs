//! An emitter's source holder and the per-frame update that moves it with its object, read from the
//! disassembly of the unpacked PC exe (runtime dump `securom_dump/image.bin`).
//!
//! **The holder** (`PalSoundSource`, vtable `0x00BE21C4`) is what a positional cue plays through: its
//! position at `+0x2C` and its velocity at `+0x5C` feed the emitter source's speaker gains, distance
//! volume and Doppler ([`crate::spatial`]). Slot `+0x14` (`0x00838310`) sets the position, slot
//! `+0x10` (`0x00838330`) the velocity (and the changed flag `+0x90`). The engine never reads a
//! physics or object velocity: the velocity is the holder's own finite difference.
//!
//! **Creation** (`FUN_00603B30`, when a cue is started on an object that has no emitter record yet):
//! the Pal engine makes a holder (engine vtable `+0x0C`), sets its position to the object's position
//! captured with the cue message and its velocity to `DAT_011766F0`..`DAT_011766F8` (zero).
//!
//! **The per-frame update** (`FUN_006036C0`, called for each emitter record by `FUN_006034B0` from
//! `PgSoundPlayer::Update` `FUN_006073C0`, before the Pal update `FUN_0082EE60`), when the record's
//! object is in the sound object table (`DAT_01175FAC`, entry `+0x88` the object's position):
//!
//! 1. three draws from the game's global random state (`DAT_00DFCBAC`, [`mercs2_core::random`]) make
//!    a direction `(third, second, first)` (`0x006038A6`..`0x0060399E`), which `FUN_00401630`
//!    normalises: `len = sqrt((x·x + y·y) + z·z)` (`fsqrt`, stored single), `(0, 0, 0)` when
//!    `len == 0`, else each component × `1 / len`;
//! 2. a fourth draw picks the jitter: [`JITTER_POSITIVE`] when `0.5 > draw` (`DAT_00BBB99C`), else
//!    [`JITTER_NEGATIVE`];
//! 3. the new position is the object's position + direction × jitter (`0x00603A22`..`0x00603A80`);
//! 4. the velocity is `(new position − holder +0x2C) × (1 / dt)` (`0x00603A8C`..`0x00603ABD`), or
//!    `DAT_011766F0`..`DAT_011766F8` (zero) when `dt == DAT_00B9B690` (0.0);
//! 5. `SetPosition`, then `SetVelocity` (`0x00603AC3`..`0x00603ADF`).
//!
//! Every update takes four draws (eight steps of the state), whatever it computes.

use mercs2_core::glam::Vec3;
use mercs2_core::random::Lcg;

/// `DAT_00BEB524` (`0x3951B717`, +0.0002): the jitter when the sign draw is below one half.
pub const JITTER_POSITIVE: f32 = f32::from_bits(0x3951_B717);
/// `DAT_00BEB520` (`0xB951B717`, −0.0002): the jitter otherwise.
pub const JITTER_NEGATIVE: f32 = f32::from_bits(0xB951_B717);
/// `DAT_00BBB99C` (0.5): the sign draw's split.
pub const JITTER_SPLIT: f32 = 0.5;

/// An emitter's source holder: its position (`+0x2C`) and velocity (`+0x5C`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Holder {
    /// The position the mix reads (`+0x2C`).
    pub position: Vec3,
    /// The velocity the Doppler factor reads (`+0x5C`).
    pub velocity: Vec3,
}

impl Holder {
    /// `FUN_00603B30`: a new holder at `position`, at rest.
    pub fn at(position: Vec3) -> Holder {
        Holder { position, velocity: Vec3::ZERO }
    }

    /// `FUN_006036C0`'s update (module docs): move the holder to its object's `object_position`
    /// plus the jitter, and set its velocity to the finite difference over `dt`. Takes four draws
    /// from `rng`, the game's global random state.
    pub fn update(&mut self, object_position: Vec3, dt: f32, rng: &mut Lcg) {
        let first = rng.next_unit();
        let second = rng.next_unit();
        let third = rng.next_unit();
        let dir = normalise([third, second, first]);
        let jitter = if JITTER_SPLIT > rng.next_unit() { JITTER_POSITIVE } else { JITTER_NEGATIVE };
        let position = Vec3::new(
            object_position.x + dir[0] * jitter,
            object_position.y + dir[1] * jitter,
            object_position.z + dir[2] * jitter,
        );
        // `ucomiss dt, 0.0` / `lahf` / `test ah, 0x44` / `jnp`: only an equal compare skips the
        // difference (an unordered one computes it).
        let velocity = if dt == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / dt;
            Vec3::new(
                (position.x - self.position.x) * inv,
                (position.y - self.position.y) * inv,
                (position.z - self.position.z) * inv,
            )
        };
        self.position = position;
        self.velocity = velocity;
    }
}

/// `FUN_00401630`: `v / |v|`, or zero when `|v| == 0`.
fn normalise(v: [f32; 3]) -> [f32; 3] {
    let len = ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return [0.0; 3];
    }
    let inv = 1.0 / len;
    [v[0] * inv, inv * v[1], inv * v[2]]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The jitter the update adds for a given generator state, drawn as the exe draws it.
    fn expected_jitter(state: u32) -> Vec3 {
        let mut r = Lcg::new(state);
        let (a, b, c) = (r.next_unit(), r.next_unit(), r.next_unit());
        let len = ((c * c + b * b) + a * a).sqrt();
        let inv = 1.0 / len;
        let s = if 0.5 > r.next_unit() { f32::from_bits(0x3951_B717) } else { f32::from_bits(0xB951_B717) };
        Vec3::new(c * inv * s, inv * b * s, inv * a * s)
    }

    /// The velocity is the finite difference of the jittered positions over `dt`.
    #[test]
    fn the_velocity_is_the_finite_difference_over_dt() {
        let mut rng = Lcg::new(0x1234_5678);
        let mut h = Holder::at(Vec3::new(10.0, 0.0, 0.0));
        let dt = 1.0 / 30.0;
        h.update(Vec3::new(10.0, 0.0, 0.0), dt, &mut rng);
        let p1 = h.position;
        let s1 = rng.state;
        h.update(Vec3::new(11.0, 0.0, 0.0), dt, &mut rng);
        let j = expected_jitter(s1);
        let p2 = Vec3::new(11.0 + j.x, 0.0 + j.y, 0.0 + j.z);
        assert_eq!(h.position, p2);
        let inv = 1.0 / dt;
        assert_eq!(h.velocity, Vec3::new((p2.x - p1.x) * inv, (p2.y - p1.y) * inv, (p2.z - p1.z) * inv));
        assert!((h.velocity.x - 30.0).abs() < 0.1, "1 m in 1/30 s is about 30 m/s: {}", h.velocity.x);
    }

    /// `dt == 0` gives zero velocity, and the position still moves (with its jitter).
    #[test]
    fn a_zero_dt_gives_zero_velocity() {
        let mut rng = Lcg::new(99);
        let mut h = Holder::at(Vec3::ZERO);
        h.velocity = Vec3::new(5.0, 5.0, 5.0);
        let s0 = rng.state;
        h.update(Vec3::new(3.0, 4.0, 5.0), 0.0, &mut rng);
        assert_eq!(h.velocity, Vec3::ZERO);
        let j = expected_jitter(s0);
        assert_eq!(h.position, Vec3::new(3.0 + j.x, 4.0 + j.y, 5.0 + j.z));
    }

    /// Every update takes four draws from the generator it is given — eight steps of its state.
    #[test]
    fn an_update_takes_four_draws_from_the_given_generator() {
        let mut rng = Lcg::game();
        let mut reference = Lcg::game();
        let mut h = Holder::at(Vec3::ZERO);
        for frame in 0..5 {
            h.update(Vec3::new(frame as f32, 0.0, 0.0), 1.0 / 60.0, &mut rng);
            for _ in 0..4 {
                reference.next_unit();
            }
            assert_eq!(rng, reference, "frame {frame}");
        }
        // A zero dt draws the same four.
        h.update(Vec3::ZERO, 0.0, &mut rng);
        for _ in 0..4 {
            reference.next_unit();
        }
        assert_eq!(rng, reference);
    }

    /// The jitter is ±0.0002 along a unit direction: its length is 0.0002 to single precision.
    #[test]
    fn the_jitter_is_two_ten_thousandths_along_a_unit_direction() {
        let mut rng = Lcg::game();
        let mut h = Holder::at(Vec3::ZERO);
        for _ in 0..16 {
            h.update(Vec3::ZERO, 1.0, &mut rng);
            assert!((h.position.length() - 0.0002).abs() < 1e-9, "{}", h.position.length());
        }
    }
}
