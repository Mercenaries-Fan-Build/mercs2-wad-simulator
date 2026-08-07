//! Slot → template resolution + the recovered pursuit/skirmish seed-table structure (CF-1).
//!
//! # What the binary actually holds (and what it does NOT)
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
//! into the image. The seeder `FUN_004d742c` (read in full this session) writes **no template ids at
//! all**: it *zeroes* the count lanes and the headers, then sets the scalar defaults
//! ([`PURSUIT_DEFAULT_MODE`], faction/level = [`PURSUIT_UNSET`], density rings 200/400/700). So the
//! per-slot template is **not statically recoverable** — exactly the "some slots can't be recovered
//! statically" case.
//!
//! # What we CAN resolve faithfully
//!
//! A spawner still carries its **faction/list channel** ([`SpawnFaction`], the recovered `+0x58`
//! index). The faithful, non-fabricated resolution is *channel → a real, corpus-verified faction
//! human template*: a fired spawner then realizes a **real, faction-correct Character** instead of the
//! invisible `template 0` prop. The names below are genuine assets recovered from the corpus (the ASET
//! name registry / `docs/modernization/model_name_map.md`, hash-verified where noted); the *exact*
//! per-`(mode, faction, level)` crowd pick a given spawner would have drawn from its WAD spawn-list is
//! the **CONFIRM-LIVE** gap (it needs the runtime `int[]` the seeder leaves empty). No template id here
//! is invented — every one hashes a real recovered name.

use crate::components::SpawnFaction;

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

// ---------------------------------------------------------------------------------------------
// Faction channel → a real, corpus-verified human template (the recoverable part of CF-1).
// ---------------------------------------------------------------------------------------------

/// One faction human template per [`SpawnFaction`] on-foot channel — each a REAL, generic/ambient
/// model that is present in the retail `vz.wad` ASET table AND builds through the same loader the live
/// population path uses (`mercs2_engine::game_world::load_model_by_hash`, game_world.rs:1305). Verified
/// this session by reverse-hashing every `vz.wad` ASET through `tools/rainbow_table.json` (the human
/// universe) and load-testing each candidate; hash = `pandemic_hash_m2(name)`:
///   * `vz_hum_soldierelite_a` — VZ army soldier   `0x7C0BEDAA` — in `vz.wad` ASET / builds (9266 v).
///   * `pr_hum_starter02`      — Pirate crowd grunt `0xB5A9D582` — primary model / builds (5745 v).
///   * `oc_hum_fireman`        — OC / UP worker     `0x1C66AE79` — primary model / builds (4196 v).
///   * `gr_hum_starter_1`      — Guerrilla starter  `0xEEAABF91` — primary model / builds (7212 v).
///   * `ch_hum_starter02`      — Chinese crowd grunt`0x0CB2C6B5` — primary model / builds (6830 v).
///   * `al_hum_starter01`      — Allied starter     `0xB58E1BB8` — primary model / builds (7920 v).
///   * `civ_hum_beachfemale_a` — civilian pedestrian`0xFA572E52` — in `vz.wad` ASET / builds (5712 v).
///
/// Ped is the DEFAULT ambient channel: `civ_hum_beachfemale_{a..d}` is the largest generic-civilian
/// crowd FAMILY named in `vz.wad` (four variants), so `_a` is the most representative common pedestrian.
/// (No `civ_hum` model is a *primary* ASET — every civilian ships as a non-primary model container — yet
/// the world is full of civilians, which is exactly why the loader builds non-primary human containers.)
///
/// 3 were replaced because their old names hashed to NOTHING in `vz.wad` and so loaded no mesh (the NPC
/// was invisible): `vz_hum_soldier_a` (Vz) and `civ_hum_casual` (Ped) do not exist as assets, and
/// `ch_hum_prisoner` (Chi) — though it does build via `load_model_by_hash` — is a specific "prisoner"
/// role (a non-primary model) reported not to appear at a live boot; all three now use a generic model
/// that both exists and builds. Pir was additionally upgraded from the NAMED boss `pr_hum_boss` (still a
/// valid load) to the generic crowd `pr_hum_starter02`, per "prefer a common/crowd human over a boss".
///
/// The shared `VehicleSpawnList` channel has no single human template → `None` (a vehicle-channel
/// spawner falls through to `template 0`, i.e. the veh-naming/prop path, handled by the resolver).
///
/// CONFIRM-LIVE: which of a channel's SEVERAL crowd models a given `(mode, faction, level)` slot draws
/// is the runtime `int[]` the seeder leaves empty — not recoverable statically. One representative,
/// faction-correct, real model per channel is the honest stand-in until that table is captured live.
pub fn faction_template_name(faction: SpawnFaction) -> Option<&'static str> {
    Some(match faction {
        SpawnFaction::Vz => "vz_hum_soldierelite_a",
        SpawnFaction::Pir => "pr_hum_starter02",
        SpawnFaction::Oil => "oc_hum_fireman",
        SpawnFaction::Gur => "gr_hum_starter_1",
        SpawnFaction::Chi => "ch_hum_starter02",
        SpawnFaction::Ali => "al_hum_starter01",
        SpawnFaction::Ped => "civ_hum_beachfemale_a",
        SpawnFaction::Vehicle => return None,
    })
}

/// Every on-foot faction template name, in [`SpawnFaction`] channel order (Vz..Ped) — the set the
/// engine's `SpawnResolver` pre-registers at boot so a hash-only population request resolves to a
/// `Character` (SEAM-1). Excludes the vehicle channel.
pub const FACTION_TEMPLATE_NAMES: [&str; 7] = [
    "vz_hum_soldierelite_a", // Vz  0x7C0BEDAA — in vz.wad ASET / builds
    "pr_hum_starter02",      // Pir 0xB5A9D582 — primary model / builds
    "oc_hum_fireman",        // Oil 0x1C66AE79 — primary model / builds
    "gr_hum_starter_1",      // Gur 0xEEAABF91 — primary model / builds
    "ch_hum_starter02",      // Chi 0x0CB2C6B5 — primary model / builds
    "al_hum_starter01",      // Ali 0xB58E1BB8 — primary model / builds
    "civ_hum_beachfemale_a", // Ped 0xFA572E52 — in vz.wad ASET / builds
];

/// `pandemic_hash_m2` of the channel's [`faction_template_name`], or `0` for a channel with no human
/// template (the vehicle channel). This is the value [`crate::SimpleSpawner::update`] stamps into the
/// emitted request's `template` in place of the old literal `0`.
pub fn faction_template_hash(faction: SpawnFaction) -> u32 {
    faction_template_name(faction)
        .map(mercs2_formats::hash::pandemic_hash_m2)
        .unwrap_or(0)
}

/// Map a `SkirmishSpawnList` faction/list-index slot value to a [`SpawnFaction`] channel, clamping
/// out-of-range values to the civilian [`SpawnFaction::Ped`] ambient default. Used by the world loader
/// to derive a spawner's faction from its attached spawn-list (CF-2) rather than hardcoding it.
///
/// CONFIRM-LIVE: the `SkirmishSpawnList`'s six ints are documented as "faction/unit/count indices"
/// (`components.rs`); we take the FIRST as the faction/list channel. The exact slot ordering is not
/// instruction-proven, so a value outside `0..=6` is treated as "unknown" → `Ped`.
pub fn faction_from_list_slot(slot: i32) -> SpawnFaction {
    match slot {
        0 => SpawnFaction::Vz,
        1 => SpawnFaction::Pir,
        2 => SpawnFaction::Oil,
        3 => SpawnFaction::Gur,
        4 => SpawnFaction::Chi,
        5 => SpawnFaction::Ali,
        6 => SpawnFaction::Ped,
        _ => SpawnFaction::Ped,
    }
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

    /// Every on-foot channel resolves to a NON-ZERO template hash (a real faction human), and the
    /// vehicle channel resolves to 0 (no human template).
    #[test]
    fn every_onfoot_channel_resolves_to_a_real_template() {
        for f in [
            SpawnFaction::Vz,
            SpawnFaction::Pir,
            SpawnFaction::Oil,
            SpawnFaction::Gur,
            SpawnFaction::Chi,
            SpawnFaction::Ali,
            SpawnFaction::Ped,
        ] {
            assert!(faction_template_name(f).is_some(), "{f:?} must have a template name");
            assert_ne!(faction_template_hash(f), 0, "{f:?} must resolve to a non-zero template hash");
        }
        assert!(faction_template_name(SpawnFaction::Vehicle).is_none());
        assert_eq!(faction_template_hash(SpawnFaction::Vehicle), 0);
    }

    /// The pre-register name set matches the per-channel resolver (same names, channel order), so the
    /// hash a spawner emits is one the resolver has registered as a `Character`.
    #[test]
    fn preregister_set_matches_per_channel_names() {
        let channels = [
            SpawnFaction::Vz,
            SpawnFaction::Pir,
            SpawnFaction::Oil,
            SpawnFaction::Gur,
            SpawnFaction::Chi,
            SpawnFaction::Ali,
            SpawnFaction::Ped,
        ];
        for (i, f) in channels.iter().enumerate() {
            assert_eq!(faction_template_name(*f), Some(FACTION_TEMPLATE_NAMES[i]));
        }
    }

    /// Every recovered template name carries the `_hum_` membership token, so the engine's
    /// `classify_template` routes it to a `Character` (not a prop/vehicle).
    #[test]
    fn all_templates_are_human_named() {
        for n in FACTION_TEMPLATE_NAMES {
            assert!(n.contains("_hum_"), "{n} must be a human template (drives Character classification)");
        }
    }

    /// The list-slot → faction map covers the 7 on-foot channels and clamps unknowns to Ped.
    #[test]
    fn list_slot_maps_to_faction() {
        assert_eq!(faction_from_list_slot(0), SpawnFaction::Vz);
        assert_eq!(faction_from_list_slot(6), SpawnFaction::Ped);
        assert_eq!(faction_from_list_slot(-1), SpawnFaction::Ped, "unknown → ambient Ped");
        assert_eq!(faction_from_list_slot(99), SpawnFaction::Ped, "out of range → ambient Ped");
    }
}
