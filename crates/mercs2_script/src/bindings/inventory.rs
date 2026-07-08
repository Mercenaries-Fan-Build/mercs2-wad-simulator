//! `Inventory` engine binding namespace — luaL_Reg table VA 0x00b99fa0, 9 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Inventory")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Inventory";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Inventory";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99fa0;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetPrimaryWeapon", corpus_calls: 11 },
    Required { name: "GetSecondaryWeapon", corpus_calls: 6 },
    Required { name: "GetVehicleWeapon", corpus_calls: 0 },
    Required { name: "GetAllWeapons", corpus_calls: 32 },
    Required { name: "SetAllWeapons", corpus_calls: 34 },
    Required { name: "DropWeapon", corpus_calls: 17 },
    Required { name: "EquipWeapon", corpus_calls: 4 },
    Required { name: "ReloadAll", corpus_calls: 3 },
    Required { name: "DestroyAllWeapons", corpus_calls: 0 },
];

/// A human's weapon loadout. The native inventory component isn't owned yet, so the weapon getters
/// report an empty loadout (`nil` for a single slot, empty table for `GetAllWeapons`, so the game's
/// `for w in tWeapons` iteration is a faithful no-op) and the mutators are accepted no-ops. A later
/// silo backs these with the real inventory component (see report — needs `inventory_*` host methods).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Slot getters read the real per-character loadout; 0 → nil so `if not w` control flow holds.
    let h = host.clone();
    b.real("GetPrimaryWeapon", lua.create_function(move |_, c: i64| {
        let g = h.borrow().inventory_primary(c as u64);
        Ok(if g == 0 { None } else { Some(g as i64) })
    })?)?;
    let h = host.clone();
    b.real("GetSecondaryWeapon", lua.create_function(move |_, c: i64| {
        let g = h.borrow().inventory_secondary(c as u64);
        Ok(if g == 0 { None } else { Some(g as i64) })
    })?)?;
    let h = host.clone();
    b.real("GetAllWeapons", lua.create_function(move |_, c: i64| {
        Ok(h.borrow().inventory_weapons(c as u64).into_iter().map(|w| w as i64).collect::<Vec<_>>())
    })?)?;

    // Loadout mutators → the real loadout.
    let h = host.clone();
    b.real("SetAllWeapons", lua.create_function(move |_, (c, weapons): (i64, Vec<i64>)| {
        h.borrow_mut().inventory_set_weapons(c as u64, weapons.into_iter().map(|w| w as u64).collect());
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("EquipWeapon", lua.create_function(move |_, (c, w): (i64, i64)| { h.borrow_mut().inventory_equip(c as u64, w as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("DropWeapon", lua.create_function(move |_, (c, w): (i64, i64)| { h.borrow_mut().inventory_drop(c as u64, w as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("DestroyAllWeapons", lua.create_function(move |_, c: i64| { h.borrow_mut().inventory_destroy_all(c as u64); Ok(()) })?)?;

    // GetVehicleWeapon → nil until the vehicle↔weapon link exists. ReloadAll(character) → reload every
    // weapon in the character's loadout to capacity (real, via the weapon ammo store).
    b.real("GetVehicleWeapon", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    let h = host.clone();
    b.real("ReloadAll", lua.create_function(move |_, c: i64| {
        let weapons = h.borrow().inventory_weapons(c as u64);
        for w in weapons {
            h.borrow_mut().weapon_reload(w);
        }
        Ok(())
    })?)?;

    let installed = b.install_global(GLOBAL)?;
    // The engine registers the inventory cfunc table as a child of `Human` — the game's Lua reaches it
    // exclusively as `Human.Inventory.*` (e.g. the masterscript's `Human.Inventory.ReloadAll`). Mirror
    // the top-level `Inventory` global onto `Human.Inventory` (Human installs first, at ns! order).
    if let (Ok(human), Ok(inv)) = (
        lua.globals().get::<mlua::Table>("Human"),
        lua.globals().get::<mlua::Table>(GLOBAL),
    ) {
        human.set("Inventory", inv)?;
    }
    Ok(installed)
}
