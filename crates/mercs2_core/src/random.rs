//! The engine's linear congruential generator and the game's one global random state.
//!
//! **The generator.** Every random draw in the unpacked PC exe we have read steps a `u32` state
//! twice by `x = x * 0x0019660D + 0x3C6EF35F`. With `u` the first new state and `x` the second, the
//! draw is `f32::from_bits((((x & 0xFFFF01FF) | (u >> 16)) >> 9) | 0x3F800000) - 1.0`, a value in
//! `[0, 1)` (the `1.0` is `DAT_00B9B664`). The sequence is inlined at each call site; the sound
//! engine's `FUN_00834a80` and the emitter update `FUN_006036c0` (`0x006038A6`..`0x006039DC`) are two
//! of them. [`Lcg`] is that generator; it holds no state of its own beyond the `u32`.
//!
//! **Two states run it.** The Pal sound engine draws from `DAT_00DFCD1C`, which Pal init reseeds
//! from `QueryPerformanceCounter` (`mercs2_audio::select`). The rest of the game — 176 stores to it
//! in the runtime dump (`securom_dump/image.bin`), among them the sound emitter jitter of
//! `FUN_006036c0` — draws from `DAT_00DFCBAC`, the game's global random state. No store that seeds
//! it was found: 169 of the 176 follow an inline step, and the other 7 store a register at the end
//! of a loop (INFERRED to be the loop's stepped state; not each traced).
//!
//! **Its initial value** is [`GAME_RNG_SEED`], `0x94153A94`. The dumps taken before runtime init
//! (`mercs2_nodrm_v2.exe`, `mercs2_nodrm_v3.exe`: the Pal engine pointer `DAT_011763FC` is still 0)
//! hold it in `DAT_00DFCBAC`, and so do `DAT_00DFCD1C` and `DAT_00DFCBB8` in `image.bin`, which was
//! also dumped before Pal init. `image.bin`'s `DAT_00DFCBAC` is `0xD36E7EE6`, which is this seed
//! stepped 514 times (257 draws) — out of a period of 2³², so the seed is the state's start.
//!
//! **Where the game-wide instance lives.** One [`Lcg`] seeded with [`GAME_RNG_SEED`] stands for
//! `DAT_00DFCBAC`. It is owned by `mercs2_engine::script_host::GameScriptHost` and handed out as a
//! shared handle (`GameScriptHost::game_rng`); every system that mirrors a draw the exe takes from
//! `DAT_00DFCBAC` takes it from there, so the draws interleave in one sequence as they do in the
//! game.

/// The multiplier of the step.
pub const LCG_MUL: u32 = 0x0019_660D;
/// The increment of the step.
pub const LCG_ADD: u32 = 0x3C6E_F35F;
/// `DAT_00DFCBAC`'s value before the game draws from it (module docs).
pub const GAME_RNG_SEED: u32 = 0x9415_3A94;

/// The engine's random generator (module docs): a `u32` state and the draw the exe inlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lcg {
    /// The generator state.
    pub state: u32,
}

impl Lcg {
    /// A generator holding `seed`.
    pub const fn new(seed: u32) -> Lcg {
        Lcg { state: seed }
    }

    /// The game's global random state as it starts (`DAT_00DFCBAC` = [`GAME_RNG_SEED`]).
    pub const fn game() -> Lcg {
        Lcg::new(GAME_RNG_SEED)
    }

    /// One draw in `[0, 1)`, advancing the state twice.
    pub fn next_unit(&mut self) -> f32 {
        let u = self.state.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        let x = u.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        self.state = x;
        f32::from_bits((((x & 0xFFFF_01FF) | (u >> 16)) >> 9) | 0x3F80_0000) - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A draw steps the state twice and takes its mantissa from both new states.
    #[test]
    fn a_draw_steps_twice_and_builds_the_mantissa_from_both_states() {
        let mut r = Lcg::new(1);
        let u = 1u32.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        let x = u.wrapping_mul(LCG_MUL).wrapping_add(LCG_ADD);
        let v = r.next_unit();
        assert_eq!(r.state, x);
        assert_eq!(v.to_bits(), (f32::from_bits((((x & 0xFFFF_01FF) | (u >> 16)) >> 9) | 0x3F80_0000) - 1.0).to_bits());
        assert!((0.0..1.0).contains(&v));
    }

    /// `image.bin`'s `DAT_00DFCBAC` (`0xD36E7EE6`) is the seed after 257 draws.
    #[test]
    fn the_runtime_dump_state_is_the_seed_after_257_draws() {
        let mut r = Lcg::game();
        for _ in 0..257 {
            r.next_unit();
        }
        assert_eq!(r.state, 0xD36E_7EE6);
    }
}
