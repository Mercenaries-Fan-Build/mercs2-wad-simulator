//! The recovered pursuit/skirmish seed-table structure (CF-1).
//!
//! # What the binary holds
//!
//! The runtime enemy-wave selector `FUN_004d9840` (VERIFIED,
//! `docs/reverse_engineer/render_distance_and_density_levers.md §2`) indexes a per-`(mode, faction,
//! level)` **list header** at
//! ```text
//! psVar3 = &DAT_016d1718 + (level + (mode + faction*6) * 0x14) * 8;   // short[4] header
//! //   psVar3[0] = slot count,  psVar3[1] = current index,  *(psVar3+2) = int* template-id array
//! ```
//! i.e. the concrete template ids live in a **runtime-populated `int[]`** hung off each header,
//! filled at world-load from the WAD `SkirmishSpawnList` / `PopulationList` COMP data — **not** baked
//! into the image. The seeder `FUN_004d742c` writes **no template ids at all**: it *zeroes* the count
//! lanes and the headers, then sets the scalar defaults ([`PURSUIT_DEFAULT_MODE`], faction/level =
//! [`PURSUIT_UNSET`], density rings 200/400/700).
//!
//! What a simple spawner spawns is its own `UnitList` / `UnitListWeighting` over the `PopulationList`
//! data — [`crate::units`].

// ---------------------------------------------------------------------------------------------
// Recovered pursuit/skirmish desired-count table geometry (render_distance_and_density_levers.md §2).
// These describe the COUNT table `FUN_004d742c` seeds — carried for faithfulness + the faction axis.
// ---------------------------------------------------------------------------------------------

/// Player-state modes (`DAT_00ed27b8`, 0..5) — the first index axis. VERIFIED.
pub const PURSUIT_MODES: usize = 6;
/// Factions (`DAT_00ed27dc`, bounded `5 < f`) — VZ/Pir/Oil/Gur/Chi/Ali. VERIFIED.
pub const PURSUIT_FACTIONS: usize = 6;
/// Row stride `0x14` (20) — `(mode + faction*6) * 0x14`. VERIFIED.
pub const PURSUIT_STRIDE: usize = 0x14;
/// Pursuit levels (`DAT_00ed27d8`, clamped 0..3) added as the byte lane. VERIFIED.
pub const PURSUIT_LEVELS: usize = 4;

/// `FUN_004d742c` default player mode (`DAT_00ed27b8 = 4`). VERIFIED from the disassembly.
pub const PURSUIT_DEFAULT_MODE: i32 = 4;
/// `FUN_004d742c` faction/level sentinel (`DAT_00ed27dc = DAT_00ed27d8 = -1`). VERIFIED.
pub const PURSUIT_UNSET: i32 = -1;
/// The three density distance rings `FUN_004d742c` seeds (`_DAT_00ed27bc/c0 = 200/400`,
/// `DAT_00ed2ae8 = 700`). VERIFIED from the disassembly.
pub const DENSITY_RING_NEAR: i32 = 200;
pub const DENSITY_RING_MID: i32 = 400;
pub const DENSITY_RING_FAR: i32 = 700;

/// Flatten the recovered index `(mode + faction*6) * 0x14 + level` exactly as `FUN_004d9840` computes
/// it. Kept as the faithful addressing helper (the header stride is 8 bytes / 4 shorts on top of this).
pub fn pursuit_index(mode: usize, faction: usize, level: usize) -> usize {
    (mode + faction * PURSUIT_FACTIONS) * PURSUIT_STRIDE + level
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The recovered index formula matches `FUN_004d9840`'s `(mode + faction*6)*0x14 + level`.
    #[test]
    fn pursuit_index_matches_disassembly() {
        assert_eq!(pursuit_index(0, 0, 0), 0);
        assert_eq!(pursuit_index(4, 0, 0), 4 * 0x14); // default mode, VZ, level 0
        // faction axis strides by 6 modes, whole rows stride by 0x14.
        assert_eq!(pursuit_index(0, 1, 0), 6 * 0x14);
        assert_eq!(pursuit_index(1, 2, 3), (1 + 2 * 6) * 0x14 + 3);
    }

    /// The seeder's scalar defaults are the recovered constants.
    #[test]
    fn recovered_seed_scalars() {
        assert_eq!(PURSUIT_DEFAULT_MODE, 4);
        assert_eq!(PURSUIT_UNSET, -1);
        assert_eq!((DENSITY_RING_NEAR, DENSITY_RING_MID, DENSITY_RING_FAR), (200, 400, 700));
        assert_eq!(PURSUIT_FACTIONS, 6);
        assert_eq!(PURSUIT_STRIDE, 0x14);
    }
}
