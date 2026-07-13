//! `mercs2_player` — the player concern: economy (cash/fuel), player-controller, character/disguise.
//!
//! **Silo 17** (`docs/modernization/reimplementation_parallelization_plan.md` §3; Wave-0 seam review
//! seam G — `Player` is the 2nd-highest-traffic Lua namespace, 107 call sites, and spans economy +
//! player-controller, so it gets its own crate rather than bloating vehicle/faction).
//! **Scoreboard row(s):** cross-cutting (economy singleton `[0x1176054]`, no single scoreboard row).
//! **Code map:** `docs/reverse_engineer/save_serialize_code_map.md` (economy/profile singleton) +
//! the money/fuel datatype notes (signed i32 on `[0x1176054]`, 1B Lua soft-clamp).
//! **Owned Lua namespace(s):** `Player` (Get/SetCash, Get/Set/AddFuel, FuelCapacity,
//! GetPrimaryCharacter, VehicleDisguise, …); `Human.Inventory` (SetAllWeapons/SetReserveAmmo — player
//! loadout) is a candidate to co-own here vs `mercs2_combat` — decide when the silo starts.
//!
//! **WAVE-1 SILO — scaffold only.** No subsystem logic lives here yet: this crate exists so the
//! Wave-1 owner can fill it without write-colliding on `mercs2_engine`/`mercs2_game` (the carve
//! rules, plan §4). It depends only on `mercs2_core` (ECS/events/time + the `PhysicsQuery` seam) and
//! `mercs2_formats`; it never depends on another leaf crate. The Wave-1 pass implements the subsystem
//! against the code maps above with the exe as the oracle and zero stubbed Lua.
//!
//! # Current state
//!
//! Zero public items and zero modules: this file is the header plus a `scaffold_links` test that
//! constructs a [`mercs2_core::Time`] to prove the dependency edge resolves. No other crate in the
//! workspace depends on `mercs2_player` yet. Anything claiming a `Player` API here is describing
//! future work, not shipped code.

#[cfg(test)]
mod tests {
    /// The scaffold links and its `mercs2_core` dependency resolves. Replaced by real tests in Wave 1.
    #[test]
    fn scaffold_links() {
        let _ = mercs2_core::Time::new(60.0);
        assert_eq!(2 + 2, 4);
    }
}
