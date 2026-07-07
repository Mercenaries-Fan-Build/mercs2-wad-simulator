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
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Single-slot getters — empty loadout → nil.
    b.real("GetPrimaryWeapon", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.real("GetSecondaryWeapon", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.real("GetVehicleWeapon", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // Full-loadout getter — empty list (faithful: `for w in GetAllWeapons(c)` iterates nothing).
    b.real("GetAllWeapons", lua.create_function(|_, _: MultiValue| Ok(Vec::<i64>::new()))?)?;

    // Loadout mutators — accepted no-ops.
    b.stub("SetAllWeapons", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("DropWeapon", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("EquipWeapon", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("ReloadAll", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("DestroyAllWeapons", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
