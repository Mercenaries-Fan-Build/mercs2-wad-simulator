//! How the engine picks among weighted choices — a multi-wave group's waves (`FUN_0083d410`) and a
//! multi-track sound's entries (`FUN_0083fee0`) — reproduced exactly.
//!
//! **The generator** (`FUN_00834a80`, disassembled from the unpacked PC exe; the same sequence is
//! inlined in `FUN_0083d450` and `FUN_0084039a`): one global `u32` state `DAT_00dfcd1c`, advanced
//! twice per draw by `x = x * 0x0019660D + 0x3C6EF35F`. With `u` the first new state and `x` the
//! second, the draw is `f32::from_bits((((x & 0xFFFF01FF) | (u >> 16)) >> 9) | 0x3F800000) - 1.0`,
//! a value in `[0, 1)`. The state is seeded once, at Pal init (`FUN_0082e6c0` at `0x0082E774`), with
//! the low 32 bits of the tick callback `0x0040B360` — `KERNEL32!QueryPerformanceCounter` (its
//! SecuROM stub, emulated from the runtime dump, runs relocated code at `0x00415A20` that calls
//! through the IAT slot `0x00B05124` and returns the 64-bit count in `EDX:EAX`). The mixer's PrepareMix
//! reads the same counter. The seed therefore differs every run — [`PalRng::new`] takes it as an
//! input.
//!
//! **Selection state:** one `u32` per group per loaded soundbank, and `S` per multi-track cue (its
//! `sound_slots`), all initialised to `0xFFFFFFFF` (`FUN_0082e370`, `LAB_0082e290`).
//!
//! **Modes** (the group's `+0x2E` byte / the sound's `+0x03` byte):
//! * 0 — **sequential** (`FUN_0083d540` / `FUN_00840480`): take the state's low byte `b`; if `b` is
//!   the choice count or `0xFF`, zero the state and use `b = 0`; add one to the state; pick `b`.
//! * 1 — **weighted random** (`FUN_0083d450` / `FUN_0084039a`): draw `r`; walk the choices adding
//!   each weight to a running `f32` sum and pick the first whose sum reaches `r` (`r <= sum`); store
//!   the pick in the state. If no sum reaches `r`, nothing is picked.
//! * 2 — **weighted random, no immediate repeat** (`FUN_0083d5c0` / `FUN_008404f0`): when the state
//!   holds a previous pick and there is more than one choice, draw `r` and walk the choices skipping
//!   the previous one, adding `weight + weight[previous] / (count - 1)` each step (the previous
//!   choice's weight shared among the rest), picking the first whose sum reaches `r`. Otherwise mode
//!   1.
//! * any other value — nothing is picked (the instance ends without playing).
//!
//! All arithmetic is single-precision in the order the engine evaluates it.

/// The engine's sound random generator: the engine's one generator ([`mercs2_core::random::Lcg`])
/// over its own state `DAT_00dfcd1c`, seeded as the engine seeds it (the low 32 bits of
/// `QueryPerformanceCounter` at Pal init) through `PalRng::new`. The game's global random state
/// `DAT_00DFCBAC` runs the same generator and is a different state (`mercs2_core::random`).
pub use mercs2_core::random::Lcg as PalRng;

/// The value every selection state starts at.
pub const STATE_INIT: u32 = 0xFFFF_FFFF;

/// Pick among `weights` with `mode`, updating `state` (and drawing from `rng` for the random modes)
/// exactly as the engine does. `None` when the engine picks nothing.
pub fn pick(mode: u8, weights: &[f32], state: &mut u32, rng: &mut PalRng) -> Option<usize> {
    let count = weights.len();
    match mode {
        0 => {
            let mut b = (*state & 0xFF) as usize;
            if b == count || b == 0xFF {
                *state = 0;
                b = 0;
            }
            *state = state.wrapping_add(1);
            // The engine indexes the list with `b` unchecked; a count of 0 would read past it.
            (b < count).then_some(b)
        }
        1 => weighted(weights, state, rng),
        2 => {
            let prev = *state;
            if prev == STATE_INIT || count == 1 {
                return weighted(weights, state, rng);
            }
            let r = rng.next_unit();
            let prev_idx = (prev & 0xFF) as usize;
            let shared = weights.get(prev_idx).copied().unwrap_or(0.0) / (count as i32 - 1) as f32;
            let mut sum = 0.0f32;
            for (i, &w) in weights.iter().enumerate() {
                if i as u32 == prev {
                    continue;
                }
                sum = w + sum + shared;
                if r <= sum {
                    *state = i as u32;
                    return Some(i);
                }
            }
            None
        }
        _ => None,
    }
}

fn weighted(weights: &[f32], state: &mut u32, rng: &mut PalRng) -> Option<usize> {
    let r = rng.next_unit();
    let mut sum = 0.0f32;
    for (i, &w) in weights.iter().enumerate() {
        sum += w;
        if r <= sum {
            *state = i as u32;
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The generator against a hand computation of the disassembled sequence.
    #[test]
    fn generator_matches_the_disassembly() {
        let mut rng = PalRng::new(0x1234_5678);
        let u = 0x1234_5678u32.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
        let x = u.wrapping_mul(0x0019_660D).wrapping_add(0x3C6E_F35F);
        let want = f32::from_bits((((x & 0xFFFF_01FF) | (u >> 16)) >> 9) | 0x3F80_0000) - 1.0;
        assert_eq!(rng.next_unit().to_bits(), want.to_bits());
        assert_eq!(rng.state, x);
        for _ in 0..10_000 {
            let r = rng.next_unit();
            assert!((0.0..1.0).contains(&r), "{r}");
        }
    }

    #[test]
    fn sequential_cycles_from_zero() {
        let mut rng = PalRng::new(1);
        let mut st = STATE_INIT;
        let picks: Vec<_> = (0..7).map(|_| pick(0, &[1.0; 3], &mut st, &mut rng).unwrap()).collect();
        assert_eq!(picks, vec![0, 1, 2, 0, 1, 2, 0]);
        assert_eq!(rng.state, 1, "sequential draws nothing");
    }

    #[test]
    fn weighted_random_follows_the_weights() {
        let mut rng = PalRng::new(7);
        let mut st = STATE_INIT;
        let mut hits = [0usize; 2];
        for _ in 0..20_000 {
            hits[pick(1, &[0.25, 0.75], &mut st, &mut rng).unwrap()] += 1;
        }
        let share = hits[1] as f64 / 20_000.0;
        assert!((share - 0.75).abs() < 0.02, "{share}");
    }

    #[test]
    fn no_repeat_never_repeats() {
        let mut rng = PalRng::new(99);
        let mut st = STATE_INIT;
        let mut last = pick(2, &[0.2, 0.3, 0.5], &mut st, &mut rng).unwrap();
        for _ in 0..5_000 {
            let p = pick(2, &[0.2, 0.3, 0.5], &mut st, &mut rng).unwrap();
            assert_ne!(p, last);
            last = p;
        }
    }

    #[test]
    fn other_modes_pick_nothing() {
        let mut rng = PalRng::new(1);
        let mut st = STATE_INIT;
        assert_eq!(pick(3, &[1.0], &mut st, &mut rng), None);
        assert_eq!(pick(1, &[0.0, 0.0], &mut st, &mut rng), None, "weights that never reach r");
    }
}
