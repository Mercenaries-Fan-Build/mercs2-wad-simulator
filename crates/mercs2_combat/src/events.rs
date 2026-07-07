//! Combat event name-hashes — the identities the weapon system posts on the shared event bus
//! (`mercs2_core::EventBus`, Keystone B).
//!
//! The engine identifies every event by a 32-bit `pandemic_hash_m2(name)` (the `.rdata` string rides
//! along only as a debug label). This crate is asset-agnostic like the bus, so it never invents a
//! hash: the constants below are the **verified** values, and [`tests::consts_match_hasher`] asserts
//! each equals `mercs2_formats::hash::pandemic_hash_m2(name)` — one source of truth, usable as a
//! `const` at emit sites.
//!
//! Sources: the homing FSM event strings are read first-hand in the code map
//! (`weapons_combat_code_map.md` §4 — `s_HomingLockStart/Update/Clear`, `s_HomingLaunched`);
//! `DamageMsg`/`DestroyMsg` are the destruction-FSM messages the damage applier feeds (§5.3A, matching
//! `state_machine_destruction_code_map.md`). `WeaponEvent` is the Lua-facing combat listener (§7).

/// `HomingLockStart` — lock acquired, lock-timer starts (FSM state 2, `FUN_0052dce0`).
pub const HOMING_LOCK_START: u32 = 0x1afd_c974;
/// `HomingLockUpdate` — lock held (FSM state 3).
pub const HOMING_LOCK_UPDATE: u32 = 0x2346_8b8f;
/// `HomingLockClear` — lock lost / consumed (FSM state 1).
pub const HOMING_LOCK_CLEAR: u32 = 0x1b04_b117;
/// `HomingLaunched` — a homing missile was fired (`FUN_0052d120`).
pub const HOMING_LAUNCHED: u32 = 0x72da_5e0b;
/// `WeaponEvent` — the generic Lua-facing combat listener (fired on a shot; §7).
pub const WEAPON_EVENT: u32 = 0x0414_0b63;
/// `DamageMsg` — a damage event into the destruction state machine (code map §5.3A).
pub const DAMAGE_MSG: u32 = 0xc650_7ee1;
/// `DestroyMsg` — an entity reached zero health (code map §5.3A).
pub const DESTROY_MSG: u32 = 0x1ed7_ad78;

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::hash::pandemic_hash_m2;

    #[test]
    fn consts_match_hasher() {
        assert_eq!(HOMING_LOCK_START, pandemic_hash_m2("HomingLockStart"));
        assert_eq!(HOMING_LOCK_UPDATE, pandemic_hash_m2("HomingLockUpdate"));
        assert_eq!(HOMING_LOCK_CLEAR, pandemic_hash_m2("HomingLockClear"));
        assert_eq!(HOMING_LAUNCHED, pandemic_hash_m2("HomingLaunched"));
        assert_eq!(WEAPON_EVENT, pandemic_hash_m2("WeaponEvent"));
        assert_eq!(DAMAGE_MSG, pandemic_hash_m2("DamageMsg"));
        assert_eq!(DESTROY_MSG, pandemic_hash_m2("DestroyMsg"));
    }
}
