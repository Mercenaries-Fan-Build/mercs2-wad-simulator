//! `Weapon` engine binding namespace — luaL_Reg table VA 0x00b98860, 9 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Weapon")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Weapon";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Weapon";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98860;

pub const REQUIRED: &[Required] = &[
    Required { name: "SetClipAmmo", corpus_calls: 0 },
    Required { name: "GetClipAmmo", corpus_calls: 0 },
    Required { name: "GetMaxClipAmmo", corpus_calls: 0 },
    Required { name: "SetReserveAmmo", corpus_calls: 10 },
    Required { name: "GetReserveAmmo", corpus_calls: 3 },
    Required { name: "GetMaxReserveAmmo", corpus_calls: 5 },
    Required { name: "Reload", corpus_calls: 0 },
    Required { name: "IsDesignator", corpus_calls: 0 },
    Required { name: "IsPrimary", corpus_calls: 0 },
];

/// Per-weapon ammo/loadout accessors. The native `RuntimeWeapon` component isn't owned yet, so ammo
/// getters return a faithful `0` (empty) and the predicate getters return `false`; the setters/reload
/// are accepted no-ops. A later silo backs these with the real weapon component (see report — needs
/// `weapon_*` host methods).
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Ammo getters — faithful empty (0).
    b.real("GetClipAmmo", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetMaxClipAmmo", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetReserveAmmo", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetMaxReserveAmmo", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;

    // Classification predicates — faithful false.
    b.real("IsDesignator", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsPrimary", lua.create_function(|_, _: MultiValue| Ok(false))?)?;

    // Ammo/reload setters — accepted no-ops.
    b.stub("SetClipAmmo", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("SetReserveAmmo", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub("Reload", lua.create_function(|_, _: MultiValue| Ok(()))?)?;

    b.install_global(GLOBAL)
}
