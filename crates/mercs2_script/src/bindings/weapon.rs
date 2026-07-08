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
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Ammo getters → the real per-weapon ammo state.
    let h = host.clone();
    b.real("GetClipAmmo", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_clip(w as u64) as i64))?)?;
    let h = host.clone();
    b.real("GetMaxClipAmmo", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_max_clip(w as u64) as i64))?)?;
    let h = host.clone();
    b.real("GetReserveAmmo", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_reserve(w as u64) as i64))?)?;
    let h = host.clone();
    b.real("GetMaxReserveAmmo", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_max_reserve(w as u64) as i64))?)?;

    // Classification predicates → the real weapon flags.
    let h = host.clone();
    b.real("IsDesignator", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_is_designator(w as u64)))?)?;
    let h = host.clone();
    b.real("IsPrimary", lua.create_function(move |_, w: i64| Ok(h.borrow().weapon_is_primary(w as u64)))?)?;

    // Ammo/reload setters → the real ammo state.
    let h = host.clone();
    b.real("SetClipAmmo", lua.create_function(move |_, (w, n): (i64, i64)| { h.borrow_mut().weapon_set_ammo(w as u64, Some(n as i32), None); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetReserveAmmo", lua.create_function(move |_, (w, n): (i64, i64)| { h.borrow_mut().weapon_set_ammo(w as u64, None, Some(n as i32)); Ok(()) })?)?;
    let h = host.clone();
    b.real("Reload", lua.create_function(move |_, w: i64| { h.borrow_mut().weapon_reload(w as u64); Ok(()) })?)?;

    b.install_global(GLOBAL)
}
