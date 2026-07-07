//! The donut / turn sine lookup table.
//!
//! The car drive model (`vehicle_code_map.md` §4 step 4, `FUN_0044a970`) samples a precomputed
//! sine table `DAT_00cf2900[(t·f)&0x1fff]` for the donut lateral-wobble term and, in the same math,
//! as the sine/cosine source for the steered lateral direction. `0x1fff` = 8191, so the table is a
//! **8192-entry sine wave over one full turn** (`2π`). We reconstruct it here — the *values* are a
//! plain sine (a table any build regenerates deterministically), only the size/index-mask is
//! load-bearing and read from the exe.

use std::f32::consts::TAU;

/// Number of samples in the donut sine table (`& 0x1fff` ⇒ 0x2000 entries). Read from the exe.
pub const DONUT_LUT_LEN: usize = 0x2000;
/// Index mask applied by the engine before the table fetch (`(t·f) & 0x1fff`).
pub const DONUT_LUT_MASK: u32 = 0x1fff;

/// An 8192-entry sine table indexed by a phase counter masked with `0x1fff`, exactly as
/// `DAT_00cf2900` is consumed in the drive math.
#[derive(Clone)]
pub struct DonutLut {
    table: Vec<f32>,
}

impl Default for DonutLut {
    fn default() -> Self {
        Self::new()
    }
}

impl DonutLut {
    /// Build the table: `table[i] = sin(2π · i / 8192)`.
    pub fn new() -> Self {
        let table = (0..DONUT_LUT_LEN)
            .map(|i| (TAU * i as f32 / DONUT_LUT_LEN as f32).sin())
            .collect();
        Self { table }
    }

    /// Raw fetch the engine performs: `DAT_00cf2900[phase & 0x1fff]`.
    #[inline]
    pub fn sample(&self, phase: u32) -> f32 {
        self.table[(phase & DONUT_LUT_MASK) as usize]
    }

    /// `sin(radians)` sourced from the table — the engine's sine source for the steered lateral
    /// direction and the donut wobble (converts radians → the phase index, then fetches).
    #[inline]
    pub fn sin(&self, radians: f32) -> f32 {
        let phase = (radians / TAU * DONUT_LUT_LEN as f32).rem_euclid(DONUT_LUT_LEN as f32);
        self.sample(phase as u32)
    }

    /// `cos(radians)` = `sin(radians + π/2)` via the same table (quarter-turn phase offset).
    #[inline]
    pub fn cos(&self, radians: f32) -> f32 {
        self.sin(radians + TAU * 0.25)
    }
}
