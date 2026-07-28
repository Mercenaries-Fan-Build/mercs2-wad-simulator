//! The per-frame player systems (`player_code_map.md` §1).
//!
//! Retail runs **two** player roster passes from the layer-4 per-system call list, both opening by
//! reading the `Players` live count and iterating by dense index with `dt`:
//!
//! | call site | callee | what it is |
//! |---|---|---|
//! | `0x004C9861` | `FUN_0062E810` | roster pass A |
//! | `0x004C9869` | `FUN_0041FE20` | a per-player world **probe** — *not* a roster tick (§9.2) |
//! | `0x004C9900` | `FUN_0062E7B0` | roster pass B |
//! | `0x004c990c` | `FUN_00532f80` | the vehicle-control pump — **`vehicle_code_map.md:122`**, corroborated by `docs/data/vehicle_code_map.json` |
//!
//! **Ordering, and a recorded tension.** `player_code_map.md` §1's *diagram* lists the vehicle pump
//! *before* the roster passes, but publishes no address for it; the three roster/probe addresses it
//! does publish (`0x004C9861`, `0x004C9869`, `0x004C9900`) all precede the pump's call site, which
//! comes from the **vehicle** map. Byte order in `FUN_004C9740` is the harder evidence, so this system
//! registers **before** `vehicle::drive_step_system`. Logged in `DEFERRED.md` as a map-vs-map tension
//! rather than silently resolved.
//!
//! (Note the pump's address is written lowercase in its source map. A case-sensitive grep for
//! `0x004C990C` finds nothing and makes this look unsourced — it is not.)
//!
//! **`FUN_0041FE20` is deliberately not implemented.** It is a Havok cast whose semantic is inferred
//! from geometry, gated on a hash constant that is **unnamed**: every token in both the PC and Xbox
//! images was hashed under both `pandemic_hash` and `pandemic_hash_m2` with no match (§9.1 S3). The
//! two constants are carried below as named-unknowns so the next reader does not re-derive the dead
//! end, and no name is invented for them.

use mercs2_core::{Health, World};

use crate::object::PlayerMode;
use crate::PlayerWorld;

/// The feature gate `FUN_0041FE20` tests before running the per-player world probe.
///
/// **UNNAMED HASH (§9.1 S3) — do not invent a name.** Pushed at `0x00420013`, consumed by
/// `FUN_006886A0`. Exhausted: every `[A-Za-z][A-Za-z0-9_.]{2,40}` token from `mercs2_unpacked.exe` and
/// from the Xbox strings dump was hashed under both engine hash functions; no match. The same sweep
/// *did* resolve `0xFA62754E → "PDA"`, so the method works — these names simply are not strings in
/// either image. Next step per the map: harvest candidates from `vz.wad` string tables via
/// `mercs2_probe`, not from the exe.
pub const PROBE_FEATURE_HASH: u32 = 0x892C_F579;

/// The Havok filter constant `FUN_0041FE20` casts with (`0x00420095`, `0x00420131`).
/// **UNNAMED HASH (§9.1 S3) — do not invent a name.** Same exhaustion as [`PROBE_FEATURE_HASH`].
pub const PROBE_FILTER_HASH: u32 = 0x223F_6FDA;

/// The player roster tick — retail's passes A and B (`FUN_0062E810` / `FUN_0062E7B0`).
///
/// Pass B's callee `FUN_006A1880` is the implementable half: it reads the attached character
/// (`player+0x20`), tests `byte [player+0x66] == 4`, queries the character's health, and gates on the
/// tick byte `player+0x45C`. That maps onto [`PlayerMode::BOUNDARY_DEATH`], `mercs2_core::Health` and
/// [`crate::PlayerObject::tick_gate`] with nothing invented.
///
/// **A no-op over a `World` carrying none of its components**, so it is safe to tick from frame 1 —
/// the same data-driven idling the vehicle/AI systems use.
///
/// `resolve` maps a character GUID to its entity. It is a parameter rather than something this system
/// looks up because the guid↔entity registry (`mercs2_core::GuidMap`) is owned by the caller, not by
/// this crate; passing it keeps the system testable without a host.
///
/// Returns the number of players whose character was found dead while in the boundary-death mode — the
/// condition pass B tests. Acting on it (the respawn/teleport) belongs to the caller, which owns the
/// spawn path.
pub fn player_roster_system(
    world: &World,
    player: &mut PlayerWorld,
    mut resolve: impl FnMut(u64) -> Option<mercs2_core::Entity>,
    dt: f32,
) -> usize {
    let _ = dt; // Passes A and B take dt; nothing recovered here integrates over it yet.
    let mut due = 0;
    for p in player.roster.iter_mut() {
        // `+0x45C` gates the pass (`0x006A18C2`/`0x006A18CF`), and an unjoined slot is not ticked.
        if !p.tick_gate || !p.is_joined() || p.character == 0 {
            continue;
        }
        let health = resolve(p.character).and_then(|e| world.get::<&Health>(e).ok());
        if is_boundary_death_due(p.mode, health.as_deref()) {
            due += 1;
        }
    }
    due
}

/// Whether a player should enter boundary death this tick: possessing a dead character while in the
/// `+0x66 == 4` mode (`FUN_006A1880` `0x006A1896`).
///
/// Split out from [`player_roster_system`] because the caller owns guid→entity resolution and the
/// health lookup; this is the predicate, testable on its own.
pub fn is_boundary_death_due(mode: PlayerMode, character_health: Option<&Health>) -> bool {
    mode == PlayerMode::BOUNDARY_DEATH && character_health.is_some_and(|h| h.is_dead())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The system must be a no-op over an empty world — the invariant that lets it tick from frame 1
    /// before anything has streamed in.
    #[test]
    fn the_roster_system_is_a_no_op_over_an_empty_world() {
        let world = World::new();
        let mut player = PlayerWorld::new();
        assert_eq!(player_roster_system(&world, &mut player, |_| None, 1.0 / 60.0), 0);
        assert!(player.roster.is_empty());
    }

    /// ...and over a populated roster whose characters have no entities yet.
    #[test]
    fn the_roster_system_tolerates_a_roster_without_entities() {
        let world = World::new();
        let mut player = PlayerWorld::single_player();
        crate::possession::attach_to_character(&mut player.roster, 0, 0x1234, Default::default());
        assert_eq!(player_roster_system(&world, &mut player, |_| None, 1.0 / 60.0), 0);
        assert_eq!(player.roster.current_players(), 1, "the roster is untouched");
    }

    /// With a resolvable, dead character in the boundary-death mode, pass B reports it — and the tick
    /// gate `+0x45C` suppresses it, which is the whole reason that byte is modelled.
    #[test]
    fn pass_b_reports_boundary_death_and_the_tick_gate_suppresses_it() {
        let mut world = World::new();
        let e = world.spawn((Health { cur: 0.0, max: 100.0 },));
        let mut player = PlayerWorld::single_player();
        crate::possession::attach_to_character(&mut player.roster, 0, 0x1234, Default::default());
        let resolve = |g: u64| (g == 0x1234).then_some(e);

        // Alive-mode player: nothing due even though the character is dead.
        assert_eq!(player_roster_system(&world, &mut player, resolve, 0.016), 0);

        player.roster.get_mut(0).unwrap().mode = PlayerMode::BOUNDARY_DEATH;
        assert_eq!(player_roster_system(&world, &mut player, resolve, 0.016), 1);

        player.roster.get_mut(0).unwrap().tick_gate = false;
        assert_eq!(
            player_roster_system(&world, &mut player, resolve, 0.016),
            0,
            "+0x45C gates the pass entirely"
        );
    }

    /// Boundary death needs **both** the mode and a dead character — either alone is not it.
    #[test]
    fn boundary_death_needs_both_the_mode_and_a_dead_character() {
        let alive = Health::default();
        let dead = Health { cur: 0.0, ..Default::default() };

        assert!(is_boundary_death_due(PlayerMode::BOUNDARY_DEATH, Some(&dead)));
        assert!(!is_boundary_death_due(PlayerMode::BOUNDARY_DEATH, Some(&alive)), "alive: not due");
        assert!(!is_boundary_death_due(PlayerMode(0), Some(&dead)), "wrong mode: not due");
        assert!(!is_boundary_death_due(PlayerMode::BOUNDARY_DEATH, None), "unpossessed: not due");
    }

    /// The two unnamed hashes are carried verbatim and are **not** the engine hash of any plausible
    /// name — the guard that stops someone "resolving" them by inventing one.
    #[test]
    fn the_probe_hashes_stay_unnamed() {
        assert_eq!(PROBE_FEATURE_HASH, 0x892C_F579);
        assert_eq!(PROBE_FILTER_HASH, 0x223F_6FDA);
        for guess in ["probe", "player", "playerprobe", "perception", "worldprobe", "havok"] {
            assert_ne!(mercs2_formats::hash::pandemic_hash_m2(guess), PROBE_FEATURE_HASH);
            assert_ne!(mercs2_formats::hash::pandemic_hash_m2(guess), PROBE_FILTER_HASH);
        }
    }
}
