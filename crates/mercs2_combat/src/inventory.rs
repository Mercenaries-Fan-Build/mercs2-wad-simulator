//! `Human.Inventory.*` — the loadout operations (`inventory_equipment_code_map.md`).
//!
//! **Ownership is settled here, not in `mercs2_player`.** The map's §10 states it on four independent
//! axes: the state lives on the *character* entity (`RuntimeInventory` is a component of the human, and
//! `RuntimeVehicleInventory` of the vehicle) and never on the player object, which `player_code_map.md`
//! §2.2 shows carries no weapon field at all; the taxonomy is `EquipmentTypeEnum` on the *item*; the
//! slot machinery sits in the same `0x0051–0x0052` module as the weapon-system driver `FUN_0051CFF0`,
//! not near the player cluster; and NPCs are eligible for the same components. It is weapon state
//! carried on a human.
//!
//! # The `bLocked` gate is 19 sites, not 6
//!
//! `RuntimeInventory+0x2C & 0x08` disables **nineteen** engine functions, and a reimpl that gates only
//! the obvious equip/give/apply six behaves differently in two places that matter. The table below maps
//! each retail gate to the function here that stands for it; rows with no reimpl counterpart are
//! recorded as unmapped rather than quietly dropped.
//!
//! | # | retail gate | reimpl site |
//! |--:|---|---|
//! | 2 | `FUN_0051C140` | [`weapon_visibility_tick`] |
//! | **3** | **`FUN_0051CFF0`** — the weapon-system tick; the lookup is **INLINED**, so a `mov ecx,imm` scan misses it | **`crate::WeaponSystem::tick`** ← a locked human's weapon system must not tick |
//! | 4 | `FUN_0051DFA0` | [`detach`] |
//! | 5,6,7 | `FUN_00527540` / `670` / `730` | [`holster_primary`] / [`draw_primary`] |
//! | 9,10,11,12 | `FUN_00527950` / `a90` / `b50` / `c70` | [`equip`] / [`give`] / [`rotate_secondary`] |
//! | 13 | `FUN_005283F0` | [`apply_loadout`] |
//! | 15 | `FUN_0052A3B0` | [`push_down_secondary`] |
//! | **18** | **`FUN_006F9260`** — the *second* native loadout-apply path | **the population/spawner loadout path** (`mercs2_engine::spawn`) |
//! | 1,8,14,16,17,19 | `0x004EAB30`, `0x00527870`, `FUN_00529DB8`, `0x005AD5C6`, `0x0061A8E0`, `.securom 0x02466BA0` | unattributed — recorded, not dropped |
//!
//! Rows 3 and 18 are the two a naive reading misses.
//!
//! **`SetAllWeapons` itself carries no gate** — the gate is one call deeper, on [`apply_loadout`]. That
//! asymmetry is what makes the shipped `SetAllWeapons` → `DisableWeapons` order work while the reverse
//! is a no-op, and all five PMC contract missions depend on it.

use hecs::Entity;
use mercs2_core::{HumanState, World};

use crate::components::{CarriedBy, Equipment, EquipmentType, PendingDestroy, RuntimeInventory};

/// The per-class cap `GetAllWeapons` reports.
///
/// **A deliberate divergence.** Retail's fill loop at `0x005BEED0` has no bound check, so a 7th primary
/// lands in `sec[0]` and a 13th carried weapon overwrites the live iterator struct — a latent stack
/// smash. We cap and drop the overflow. Recorded here so nobody "fixes" the cap back out to match.
pub const MAX_PER_CLASS: usize = 6;

/// The `+0x2C & 0x08` gate.
///
/// Every gated retail site early-outs to the epilogue returning false: a locked human silently rejects
/// every equip, stow, give and loadout write.
#[must_use]
pub fn gate(inv: &RuntimeInventory) -> bool {
    !inv.flags.locked()
}

/// Every weapon `holder` carries, in insertion order.
///
/// Walks the carry edges rather than a list on the human — retail consults the per-edge flag, which a
/// flat `Vec` has nowhere to store.
pub fn carried(world: &World, holder: Entity) -> Vec<(Entity, CarriedBy)> {
    let mut v: Vec<(Entity, CarriedBy)> = world
        .query::<&CarriedBy>()
        .iter()
        .filter(|(_, e)| e.holder == holder)
        .map(|(w, e)| (w, *e))
        .collect();
    v.sort_by_key(|(_, e)| e.seq);
    v
}

/// The carry edge for `weapon`, if it is held by `holder` — retail's `FUN_00649440` find-edge, O(1)
/// here because the edge is a component on the child.
pub fn find_edge(world: &World, weapon: Entity, holder: Entity) -> Option<CarriedBy> {
    world.get::<&CarriedBy>(weapon).ok().map(|e| *e).filter(|e| e.holder == holder)
}

/// The next insertion sequence for `holder`.
fn next_seq(world: &World, holder: Entity) -> u32 {
    carried(world, holder).last().map(|(_, e)| e.seq + 1).unwrap_or(0)
}

/// The slot class of a weapon, read from the **item** (`Equipment`), not from the holder.
pub fn class_of(world: &World, weapon: Entity) -> EquipmentType {
    world.get::<&Equipment>(weapon).map(|e| e.class).unwrap_or_default()
}

/// `Human.Inventory` give — attach `weapon` to `holder`. Gated.
pub fn give(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    let Ok(inv) = world.get::<&RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    drop(inv);
    let seq = next_seq(world, holder);
    let _ = world.insert_one(weapon, CarriedBy { holder, flags: 0, seq });
    true
}

/// `FUN_0051DFA0` — drop the carry edge. Gated.
pub fn detach(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    let Ok(inv) = world.get::<&RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    drop(inv);
    if find_edge(world, weapon, holder).is_none() {
        return false;
    }
    let _ = world.remove_one::<CarriedBy>(weapon);
    clear_slots_referencing(world, holder, weapon);
    true
}

/// Clear every slot on `holder`'s record that still points at `weapon` — so a detached weapon cannot
/// remain "equipped".
fn clear_slots_referencing(world: &mut World, holder: Entity, weapon: Entity) {
    let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return };
    let clear = |slot: &mut Option<Entity>| {
        if *slot == Some(weapon) {
            *slot = None;
        }
    };
    let i = &mut *inv;
    clear(&mut i.equipped_primary);
    clear(&mut i.equipped_secondary);
    clear(&mut i.equipped_vehicle);
    clear(&mut i.last_primary);
    clear(&mut i.last_secondary);
    clear(&mut i.last_last_secondary);
    clear(&mut i.pending_pickup);
    clear(&mut i.weapon_in_use);
}

/// `FUN_00527730` — draw a primary into the equipped slot, demoting the current one to
/// [`last_primary`](RuntimeInventory::last_primary). Gated.
pub fn draw_primary(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    if inv.equipped_primary != Some(weapon) {
        inv.last_primary = inv.equipped_primary;
        inv.equipped_primary = Some(weapon);
    }
    inv.current_equip_action = inv.current_equip_action.wrapping_add(1);
    true
}

/// `FUN_00527540`/`FUN_00527670` — holster the equipped primary. Gated.
pub fn holster_primary(world: &mut World, holder: Entity) -> bool {
    let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    if let Some(w) = inv.equipped_primary.take() {
        inv.last_primary = Some(w);
    }
    true
}

/// `FUN_00527C70` — rotate the **three-rung** secondary carousel. Gated.
///
/// The recovered tail (§5), which is *not* a plain 3-cycle:
///
/// ```text
/// if (h[0x10] == 0) { restore; return false; }   // nothing stowed to promote -> FAIL
/// h[0x04] = h[0x10];                             // ★ promote
/// h[0x10] = h[0x14] ? h[0x14] : old;             // ★ demote — `old` when the 3rd rung is empty
/// if (h[0x14]) h[0x14] = old;
/// ```
///
/// Two details an unconditional `a→b→c→a` cycle gets wrong, and both bite on the **shipped**
/// two-secondary case (§8.3), where `+0x14` is empty:
///
/// 1. With `+0x14` empty, retail puts `old` in `+0x10` and **leaves `+0x14` at 0**. A blind cycle
///    puts `None` in `+0x10` and `old` in `+0x14`, so the next rotation promotes nothing.
/// 2. With `+0x10` empty there is nothing to promote: retail **restores and returns false** rather
///    than blanking the equipped slot.
pub fn rotate_secondary(world: &mut World, holder: Entity) -> bool {
    let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    // `h[0x10] == 0` -> restore (we mutated nothing yet) and fail.
    if inv.last_secondary.is_none() {
        return false;
    }
    let old = inv.equipped_secondary;
    inv.equipped_secondary = inv.last_secondary;
    match inv.last_last_secondary {
        Some(third) => {
            inv.last_secondary = Some(third);
            inv.last_last_secondary = old;
        }
        // The two-secondary case: `old` demotes into `+0x10` and `+0x14` stays empty.
        None => inv.last_secondary = old,
    }
    true
}

/// `FUN_0052A3B0` — push a new secondary down the carousel: `[+0x14] = [+0x04]; [+0x04] = new`. Gated.
pub fn push_down_secondary(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    inv.last_last_secondary = inv.equipped_secondary;
    inv.equipped_secondary = Some(weapon);
    true
}

/// `Human.Inventory.EquipWeapon(uChar, uWeapon)` `0x005BF4E0` — equip a carried weapon into the slot its
/// **own** `Equipment` class selects. Gated. Pushes a boolean to Lua.
/// ⚠ The secondary path is **`FUN_00527C70`** (the rotation), not `FUN_0052A3B0` (the push-down) —
/// §4.7. They differ: the rotation carries the outgoing weapon into `+0x10`, while the push-down only
/// touches `+0x04`/`+0x14` and so drops the current `last_secondary` on the floor. An earlier revision
/// routed here to the push-down, and its own test comment recorded the symptom ("`a` fell out of the
/// carousel") without recognising it as the bug.
pub fn equip(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    if find_edge(world, weapon, holder).is_none() {
        return false;
    }
    match class_of(world, weapon) {
        EquipmentType::Primary => draw_primary(world, holder, weapon),
        EquipmentType::Secondary => {
            // `FUN_00527C70(char, weapon)`: stage the incoming weapon into `+0x10` so the rotation
            // promotes it (`h[0x10] != weapon -> swap(h[0x10], h[0x14]); if h[0x10]==0 h[0x10]=weapon`),
            // then run the shared tail.
            {
                let Ok(mut inv) = world.get::<&mut RuntimeInventory>(holder) else { return false };
                if !gate(&inv) {
                    return false;
                }
                if inv.equipped_secondary == Some(weapon) {
                    return true; // already equipped, nothing to do (§5 line 12)
                }
                if inv.last_secondary != Some(weapon) {
                    let i = &mut *inv;
                    std::mem::swap(&mut i.last_secondary, &mut i.last_last_secondary);
                }
                if inv.last_secondary.is_none() {
                    inv.last_secondary = Some(weapon);
                }
            }
            rotate_secondary(world, holder)
        }
    }
}

/// `Human.Inventory.DropWeapon(uChar, uWeapon)` `0x005BF420` — detach and clear any slot holding it.
/// Pushes a boolean.
///
/// The `mrxshootinggallery` sequence `GetPrimaryWeapon → Drop → GetPrimaryWeapon` still yields a
/// weapon — but **not because `Drop` promotes anything**. §4.6/§8.3 explain it by `GetPrimaryWeapon`'s
/// own fallback to `+0x0C`: the drop clears `+0x00`, and the *getter* reads `+0x0C`.
///
/// An earlier revision here wrote the promotion explicitly, which additionally **consumed** `+0x0C`.
/// Retail leaves it, so a second drop behaved differently. Clearing the slots is all this does.
pub fn drop_weapon(world: &mut World, holder: Entity, weapon: Entity) -> bool {
    if find_edge(world, weapon, holder).is_none() {
        return false;
    }
    {
        let Ok(inv) = world.get::<&RuntimeInventory>(holder) else { return false };
        if !gate(&inv) {
            return false;
        }
    }
    let _ = world.remove_one::<CarriedBy>(weapon);
    clear_slots_referencing(world, holder, weapon);
    true
}

/// `Human.Inventory.GetPrimaryWeapon(uChar)` `0x005BE9B0` — equipped primary, falling back to
/// `+0x0C`.
pub fn primary_weapon(world: &World, holder: Entity) -> Option<Entity> {
    let inv = world.get::<&RuntimeInventory>(holder).ok()?;
    inv.equipped_primary.or(inv.last_primary)
}

/// `Human.Inventory.GetSecondaryWeapon(uChar)` `0x005BEB30` — equipped secondary, falling back to
/// `+0x10`.
pub fn secondary_weapon(world: &World, holder: Entity) -> Option<Entity> {
    let inv = world.get::<&RuntimeInventory>(holder).ok()?;
    inv.equipped_secondary.or(inv.last_secondary)
}

/// `Human.Inventory.GetVehicleWeapon(uChar)` `0x005BECB0` — `+0x08`, **no fallback**. Retail also
/// returns nil when it is 0.
pub fn vehicle_weapon(world: &World, holder: Entity) -> Option<Entity> {
    world.get::<&RuntimeInventory>(holder).ok()?.equipped_vehicle
}

/// `Human.Inventory.GetAllWeapons(uChar [, bExcludeFlagged])` `0x005BED60` — **one** array table:
/// primaries (equipped first), then secondaries (equipped first). Capped at [`MAX_PER_CLASS`] per class.
///
/// ⚠ **One table, not two.** An earlier revision of this function returned `(primaries, secondaries)`
/// as two Lua values, and its comment asserted that was required. That is the opposite of the oracle:
/// §4.4 reads the epilogue as `FUN_005A1270(N, &L)` = `lua_createtable` + N × `rawseti` counting down,
/// then **`return 1`** (`mov eax,1` at `0x005BF14E`) — a single array. §7.3 says the same from the Lua
/// side (`local tEquipment = Human.Inventory.GetAllWeapons(uCharGuid, true)`, a single-value
/// assignment, iterated with `pairs`). Returning two values made every shipped consumer — `mrxplayer`'s
/// save/restore and all five PMC `tP1Weapons = GetAllWeapons(...)` → `SetAllWeapons(...)` round
/// trips — silently drop the secondaries.
///
/// §10 item 6's "pairs the two result tables positionally" refers to the tables from the **two calls**
/// (`mrxplayer.lua:666` and `:702`), not to two returns from one call.
///
/// Ordering is still load-bearing: push order becomes array order, so equipped-first is what makes the
/// positional save/restore line up.
///
/// `exclude_flagged` drops edges carrying the **exclude** bit `0x02`
/// ([`CarriedBy::EXCLUDED`], `0x005BEE8F`) — *not* the equipped bit, which an earlier revision had
/// confused it with.
pub fn get_all(world: &World, holder: Entity, exclude_flagged: bool) -> Vec<Entity> {
    let equipped_primary = world.get::<&RuntimeInventory>(holder).ok().and_then(|i| i.equipped_primary);
    let equipped_secondary =
        world.get::<&RuntimeInventory>(holder).ok().and_then(|i| i.equipped_secondary);

    let (mut prim, mut sec) = (Vec::new(), Vec::new());
    for (w, edge) in carried(world, holder) {
        if exclude_flagged && edge.is_excluded() {
            continue;
        }
        match class_of(world, w) {
            EquipmentType::Primary => prim.push(w),
            EquipmentType::Secondary => sec.push(w),
        }
    }
    // Equipped first, order otherwise preserved.
    let hoist = |v: &mut Vec<Entity>, first: Option<Entity>| {
        if let Some(f) = first {
            if let Some(i) = v.iter().position(|&w| w == f) {
                let w = v.remove(i);
                v.insert(0, w);
            }
        }
    };
    hoist(&mut prim, equipped_primary);
    hoist(&mut sec, equipped_secondary);
    prim.truncate(MAX_PER_CLASS);
    sec.truncate(MAX_PER_CLASS);
    prim.extend(sec);
    prim
}

/// `FUN_005283F0` — apply a loadout: at most 2 primaries + 2 secondaries. **This is the gated call**,
/// one level below `SetAllWeapons`.
pub fn apply_loadout(world: &mut World, holder: Entity, weapons: &[Entity]) -> bool {
    let Ok(inv) = world.get::<&RuntimeInventory>(holder) else { return false };
    if !gate(&inv) {
        return false;
    }
    drop(inv);

    let (mut np, mut ns) = (0, 0);
    for &w in weapons {
        match class_of(world, w) {
            EquipmentType::Primary if np < 2 => {
                give(world, holder, w);
                draw_primary(world, holder, w);
                np += 1;
            }
            EquipmentType::Secondary if ns < 2 => {
                give(world, holder, w);
                push_down_secondary(world, holder, w);
                ns += 1;
            }
            // Past 2+2 the weapon is carried but not drawn — retail's apply only fills four slots.
            _ => {
                give(world, holder, w);
            }
        }
    }
    true
}

/// `Human.Inventory.SetAllWeapons(uChar, …)` `0x005BF160` — **destroy first, then apply at most 2+2**.
/// Pushes a boolean.
///
/// The destroy is a **deferred queue push**, not a synchronous reap (§4.9): the old instance GUIDs stay
/// resolvable until [`drain_pending_destroy`] runs, which is what makes the shipped
/// snapshot-then-restore pattern legal. Note `SetAllWeapons` itself is **not** gated — [`apply_loadout`]
/// is, one call deeper.
pub fn set_all_weapons(world: &mut World, holder: Entity, weapons: &[Entity]) -> bool {
    destroy_all_weapons(world, holder);
    apply_loadout(world, holder, weapons)
}

/// `Human.Inventory.DestroyAllWeapons(uChar)` `0x005BF630` — queue every carried weapon for destruction.
/// Retail pushes **nothing** from this cfunc.
pub fn destroy_all_weapons(world: &mut World, holder: Entity) {
    let held: Vec<Entity> = carried(world, holder).into_iter().map(|(w, _)| w).collect();
    for w in held {
        let _ = world.remove_one::<CarriedBy>(w);
        let _ = world.insert_one(w, PendingDestroy);
    }
    // ⚠ The record is deliberately **left alone**. §4.9 consequence 2: "It never touches
    // `RuntimeInventory`. There is no `mov ecx,0x17BF3D8` … It does not clear
    // `+0x00/+0x04/+0x0C/+0x10/+0x14`." Since the destroy is deferred, retail's slots keep pointing at
    // instances that are still live until the reap — and an earlier revision here zeroed the whole
    // record, which closed exactly the window the shipped snapshot-restore pattern needs.
}

/// Reap the deferred destroy queue. Called once per frame by the engine, **after** the script pump, so
/// a script that destroys and re-applies within one frame still sees valid handles.
///
/// Returns the despawned entities. A weapon that was re-attached in the meantime is **cancelled**, not
/// reaped — that is the snapshot-restore path.
pub fn drain_pending_destroy(world: &mut World) -> Vec<Entity> {
    let queued: Vec<Entity> = world.query::<&PendingDestroy>().iter().map(|(e, _)| e).collect();
    let mut reaped = Vec::new();
    for e in queued {
        if world.get::<&CarriedBy>(e).is_ok() {
            // Re-attached before the reap: cancel.
            let _ = world.remove_one::<PendingDestroy>(e);
            continue;
        }
        if world.despawn(e).is_ok() {
            reaped.push(e);
        }
    }
    reaped
}

/// `Human.Inventory.ReloadAll(uChar, bSomething)` `0x005BF6B0`.
///
/// ⚠ **Argument 2 is required**: retail bails when it is absent, and the two shipped DLC call sites were
/// written against that. `None` here is that bail — the binding pushes `nil`.
pub fn reload_all(world: &mut World, holder: Entity, arg2: Option<bool>) -> Option<bool> {
    arg2?;
    let inv = world.get::<&RuntimeInventory>(holder).ok()?;
    if !gate(&inv) {
        return Some(false);
    }
    Some(true)
}

/// `Human.EnableWeapons` / `DisableWeapons` — `FUN_005BE050`.
///
/// Returns `None` (→ Lua nil) for a character with no `RuntimeInventory`, matching retail.
///
/// **`EnableWeapons` is not a pure flag clear.** It also zeroes
/// [`weapon_in_use`](RuntimeInventory::weapon_in_use) (`.securom 0x0246BF5A`) and re-equips the last
/// secondary when none is equipped (`0x0246BF8F`). `DisableWeapons` holsters.
///
/// `mercs2_core::HumanState::weapons_enabled` is kept as a **one-way mirror** of the LOCKED bit, so
/// `HumanState::can_fire()` cannot silently disagree with the authoritative state.
/// `RuntimeInventory.flags::LOCKED` is authoritative; never write the mirror directly.
///
/// ⚠ An earlier revision justified the mirror by claiming `mercs2_anim`/`mercs2_ai` consume
/// `can_fire()`. They do not: `grep -rn can_fire crates/` finds only `mercs2_core` (the definition and
/// its own tests), one assertion in `mercs2_engine::spawn`, and a *locally computed* `can_fire` in
/// `mercs2_game::world` that never reads `HumanState`. The mirror is maintained so the field is not
/// wrong, not because a named consumer depends on it.
pub fn set_weapons_enabled(world: &mut World, holder: Entity, on: bool) -> Option<bool> {
    // `FUN_005BE050` **owns** the flag, so its own state transition is not subject to the gate it
    // installs. Order matters: clearing the lock first would let a concurrent path act mid-transition,
    // and setting it first would make the disable path's holster refuse itself. So the transition runs
    // against the pre-existing lock state, and the flag write lands last.
    world.get::<&RuntimeInventory>(holder).ok()?; // nil for a character with no inventory

    if on {
        let mut inv = world.get::<&mut RuntimeInventory>(holder).ok()?;
        inv.weapon_in_use = None; // off the turret (`.securom 0x0246BF5A`)
        // Re-equip the last secondary when none is equipped (`0x0246BF8F`) — done inline rather than
        // through the gated `rotate_secondary`, for the reason above.
        if inv.equipped_secondary.is_none() {
            inv.equipped_secondary = inv.last_secondary.take();
            inv.last_secondary = inv.last_last_secondary.take();
        }
        inv.flags.set_locked(false);
    } else {
        let mut inv = world.get::<&mut RuntimeInventory>(holder).ok()?;
        // Holster: the equipped primary demotes to the fallback slot rather than being discarded.
        if let Some(w) = inv.equipped_primary.take() {
            inv.last_primary = Some(w);
        }
        inv.flags.set_locked(true);
    }

    if let Ok(mut hs) = world.get::<&mut HumanState>(holder) {
        hs.weapons_enabled = on; // MIRROR — authoritative state is the LOCKED bit above.
    }
    Some(true)
}

/// Gate site 2 (`FUN_0051C140`) — the weapon-visibility tick. A locked human's visibility does not
/// advance.
pub fn weapon_visibility_tick(world: &mut World, holder: Entity) -> bool {
    let Ok(inv) = world.get::<&RuntimeInventory>(holder) else { return false };
    gate(&inv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::WeaponVisibility;

    fn human(world: &mut World) -> Entity {
        world.spawn((RuntimeInventory::default(), HumanState::default()))
    }
    fn weapon(world: &mut World, class: EquipmentType) -> Entity {
        world.spawn((Equipment { class },))
    }

    /// A human carries an equipped primary **and** an equipped secondary **at the same time** — the
    /// single-`equipped`-index model could not express this, which made the two getters mutually
    /// exclusive.
    #[test]
    fn primary_and_secondary_are_simultaneously_equipped() {
        let mut w = World::new();
        let h = human(&mut w);
        let rifle = weapon(&mut w, EquipmentType::Primary);
        let pistol = weapon(&mut w, EquipmentType::Secondary);

        apply_loadout(&mut w, h, &[rifle, pistol]);
        assert_eq!(primary_weapon(&w, h), Some(rifle));
        assert_eq!(secondary_weapon(&w, h), Some(pistol), "both slots are live at once");
    }

    /// The secondary carousel is **three** rungs deep. A two-field model returns the wrong weapon on
    /// the second rotation.
    #[test]
    fn the_secondary_rotation_matches_the_recovered_tail() {
        let mut w = World::new();
        let h = human(&mut w);
        let (a, b, c) = (
            weapon(&mut w, EquipmentType::Secondary),
            weapon(&mut w, EquipmentType::Secondary),
            weapon(&mut w, EquipmentType::Secondary),
        );

        // --- `h[0x10] == 0`: nothing stowed to promote -> restore and FAIL, do not blank `+0x04`. ---
        {
            let mut inv = w.get::<&mut RuntimeInventory>(h).unwrap();
            inv.equipped_secondary = Some(a);
        }
        assert!(!rotate_secondary(&mut w, h), "no `+0x10` to promote -> false");
        assert_eq!(secondary_weapon(&w, h), Some(a), "and the equipped slot survives");

        // --- Two-secondary case (`+0x14` empty), which §8.3 says is the shipped one:
        //     `old` demotes into `+0x10` and `+0x14` STAYS EMPTY. A blind 3-cycle would park `old`
        //     in `+0x14` and leave `+0x10` empty, so the next rotation would promote nothing. ---
        {
            let mut inv = w.get::<&mut RuntimeInventory>(h).unwrap();
            inv.equipped_secondary = Some(a);
            inv.last_secondary = Some(b);
            inv.last_last_secondary = None;
        }
        assert!(rotate_secondary(&mut w, h));
        {
            let inv = w.get::<&RuntimeInventory>(h).unwrap();
            assert_eq!(inv.equipped_secondary, Some(b), "promote");
            assert_eq!(inv.last_secondary, Some(a), "old demotes into +0x10, not +0x14");
            assert_eq!(inv.last_last_secondary, None, "+0x14 stays empty");
        }
        // ...so it keeps rotating rather than stalling.
        assert!(rotate_secondary(&mut w, h));
        assert_eq!(secondary_weapon(&w, h), Some(a));

        // --- Three-rung case: the full carousel. ---
        {
            let mut inv = w.get::<&mut RuntimeInventory>(h).unwrap();
            inv.equipped_secondary = Some(a);
            inv.last_secondary = Some(b);
            inv.last_last_secondary = Some(c);
        }
        assert!(rotate_secondary(&mut w, h));
        let inv = w.get::<&RuntimeInventory>(h).unwrap();
        assert_eq!(inv.equipped_secondary, Some(b));
        assert_eq!(inv.last_secondary, Some(c));
        assert_eq!(inv.last_last_secondary, Some(a), "old lands in +0x14 only when it was occupied");
    }

    /// **The gate asymmetry the PMC missions depend on**: `SetAllWeapons` is not gated, but
    /// `apply_loadout` is — so `SetAllWeapons` → `DisableWeapons` works while the reverse is a no-op.
    #[test]
    fn set_all_weapons_is_ungated_but_apply_loadout_is_not() {
        let mut w = World::new();
        let h = human(&mut w);
        let rifle = weapon(&mut w, EquipmentType::Primary);

        // Locked first, then apply: the loadout is rejected one call deeper.
        set_weapons_enabled(&mut w, h, false);
        assert!(!set_all_weapons(&mut w, h, &[rifle]), "locked: apply_loadout refuses");
        assert_eq!(primary_weapon(&w, h), None);

        // Unlock, apply, then lock: the loadout stands.
        set_weapons_enabled(&mut w, h, true);
        assert!(set_all_weapons(&mut w, h, &[rifle]));
        set_weapons_enabled(&mut w, h, false);
        assert_eq!(
            w.get::<&RuntimeInventory>(h).unwrap().last_primary,
            Some(rifle),
            "DisableWeapons holsters rather than discarding"
        );
    }

    /// Destroy is **deferred**: the handles stay valid until the reap, and a re-attach cancels it. This
    /// is what makes `mrxplayer`'s snapshot-restore legal.
    #[test]
    fn destroy_is_deferred_and_a_reattach_cancels_it() {
        let mut w = World::new();
        let h = human(&mut w);
        let rifle = weapon(&mut w, EquipmentType::Primary);
        apply_loadout(&mut w, h, &[rifle]);

        destroy_all_weapons(&mut w, h);
        assert!(w.contains(rifle), "still resolvable — the reap has not run");

        // Snapshot-restore: hand it back before the reap.
        give(&mut w, h, rifle);
        assert!(drain_pending_destroy(&mut w).is_empty(), "re-attached, so cancelled");
        assert!(w.contains(rifle));

        // Left alone, it is reaped.
        destroy_all_weapons(&mut w, h);
        assert_eq!(drain_pending_destroy(&mut w), vec![rifle]);
        assert!(!w.contains(rifle));
    }

    /// `GetAllWeapons` is **equipped-first** and capped per class. The ordering is what `mrxplayer`
    /// pairs positionally across a save/restore.
    #[test]
    fn get_all_is_equipped_first_and_capped() {
        let mut w = World::new();
        let h = human(&mut w);
        let guns: Vec<Entity> =
            (0..8).map(|_| weapon(&mut w, EquipmentType::Primary)).collect();
        for &g in &guns {
            give(&mut w, h, g);
        }
        draw_primary(&mut w, h, guns[5]); // equip one from the middle

        let all = get_all(&w, h, false);
        assert_eq!(all[0], guns[5], "the equipped primary comes first");
        assert_eq!(all.len(), MAX_PER_CLASS, "capped — retail's unbounded loop is a stack smash");
    }

    /// **One array table**, primaries then secondaries — not two Lua values.
    ///
    /// §4.4 reads the epilogue as `lua_createtable` + N × `rawseti` then `return 1`, and §7.3 shows the
    /// Lua side taking it as a single value. Returning a pair made every shipped
    /// `GetAllWeapons` → `SetAllWeapons` round trip drop its secondaries.
    #[test]
    fn get_all_is_one_list_primaries_then_secondaries() {
        let mut w = World::new();
        let h = human(&mut w);
        let rifle = weapon(&mut w, EquipmentType::Primary);
        let pistol = weapon(&mut w, EquipmentType::Secondary);
        apply_loadout(&mut w, h, &[rifle, pistol]);

        let all = get_all(&w, h, false);
        assert_eq!(all, vec![rifle, pistol], "one flat list, primaries first");
    }

    /// `bExcludeFlagged` filters on the **exclude** bit `0x02`, not the equipped bit `0x01`.
    #[test]
    fn exclude_flagged_filters_the_exclude_bit_not_the_equipped_bit() {
        let mut w = World::new();
        let h = human(&mut w);
        let a = weapon(&mut w, EquipmentType::Primary);
        let b = weapon(&mut w, EquipmentType::Primary);
        apply_loadout(&mut w, h, &[a, b]);

        // `b` is the equipped primary; excluding must NOT drop it.
        assert!(get_all(&w, h, true).contains(&b), "the equipped bit is not the exclude bit");

        // Now mark `a`'s edge excluded and it disappears from the filtered list.
        w.get::<&mut CarriedBy>(a).unwrap().flags |= CarriedBy::EXCLUDED;
        let filtered = get_all(&w, h, true);
        assert!(!filtered.contains(&a), "bit 0x02 excludes");
        assert!(get_all(&w, h, false).contains(&a), "and only when arg 2 is true");
    }

    /// Dropping the equipped primary still yields a weapon from `GetPrimaryWeapon` — via the **getter's
    /// fallback to `+0x0C`**, not via a promotion write in `Drop`. Retail leaves `+0x0C` intact.
    #[test]
    fn dropping_the_equipped_primary_falls_back_without_consuming_the_slot() {
        let mut w = World::new();
        let h = human(&mut w);
        let (a, b) = (weapon(&mut w, EquipmentType::Primary), weapon(&mut w, EquipmentType::Primary));
        apply_loadout(&mut w, h, &[a, b]);
        assert_eq!(primary_weapon(&w, h), Some(b), "the last drawn is equipped");

        assert!(drop_weapon(&mut w, h, b));
        assert_eq!(primary_weapon(&w, h), Some(a), "the previous primary takes over");
    }

    /// `EnableWeapons` is not a pure flag clear: it zeroes `weapon_in_use` and re-equips the last
    /// secondary. And the `HumanState` mirror tracks it, so `can_fire()` stays correct.
    #[test]
    fn enable_weapons_does_more_than_clear_the_flag() {
        let mut w = World::new();
        let h = human(&mut w);
        let pistol = weapon(&mut w, EquipmentType::Secondary);
        {
            let mut inv = w.get::<&mut RuntimeInventory>(h).unwrap();
            inv.weapon_in_use = Some(pistol);
            inv.last_secondary = Some(pistol);
            inv.weapon_visibility = WeaponVisibility(2);
        }
        set_weapons_enabled(&mut w, h, false);
        assert!(!w.get::<&HumanState>(h).unwrap().can_fire(), "the mirror follows the lock");

        set_weapons_enabled(&mut w, h, true);
        let inv = w.get::<&RuntimeInventory>(h).unwrap();
        assert_eq!(inv.weapon_in_use, None, "off the turret");
        assert_eq!(inv.equipped_secondary, Some(pistol), "the last secondary is re-equipped");
        drop(inv);
        assert!(w.get::<&HumanState>(h).unwrap().can_fire());
    }

    /// A character with no `RuntimeInventory` yields nil, not a panic or a false.
    #[test]
    fn a_character_without_an_inventory_yields_nil() {
        let mut w = World::new();
        let bare = w.spawn((HumanState::default(),));
        assert_eq!(set_weapons_enabled(&mut w, bare, true), None);
        assert_eq!(reload_all(&mut w, bare, Some(true)), None);
        assert_eq!(primary_weapon(&w, bare), None);
    }

    /// `ReloadAll` **requires** its second argument — retail bails without it, and the DLC call sites
    /// were written against that bail.
    #[test]
    fn reload_all_requires_its_second_argument() {
        let mut w = World::new();
        let h = human(&mut w);
        assert_eq!(reload_all(&mut w, h, None), None, "no arg 2 -> nil, no reload");
        assert_eq!(reload_all(&mut w, h, Some(false)), Some(true));
    }

    /// The gate really does disable the whole mutator set, not just the obvious few.
    #[test]
    fn the_lock_disables_every_mutator() {
        let mut w = World::new();
        let h = human(&mut w);
        let g = weapon(&mut w, EquipmentType::Primary);
        give(&mut w, h, g);
        set_weapons_enabled(&mut w, h, false);

        let g2 = weapon(&mut w, EquipmentType::Primary);
        assert!(!give(&mut w, h, g2));
        assert!(!detach(&mut w, h, g));
        assert!(!draw_primary(&mut w, h, g));
        assert!(!holster_primary(&mut w, h));
        assert!(!rotate_secondary(&mut w, h));
        assert!(!push_down_secondary(&mut w, h, g));
        assert!(!apply_loadout(&mut w, h, &[g]));
        assert!(!equip(&mut w, h, g));
        assert!(!drop_weapon(&mut w, h, g));
        assert!(!weapon_visibility_tick(&mut w, h), "gate site 2");
    }
}
