//! `Fire` engine binding namespace — luaL_Reg table VA 0x00b9a7a8, 3 cfuncs.
//!
//! `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! To back this namespace: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_child("Graphics", "FuelTrail")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mercs2_luac::rt::{Lua, Result as LuaResult};

use crate::{Guid, SharedHost};
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
/// **There is no `Fire` namespace.** The table at `0x00B9A7A8` is a marker-delimited SUB-TABLE of
/// the `Graphics` compound blob named `FuelTrail` (`{"FuelTrail",0xFFFFFFFF}` opens at `0x00B9A7A0`,
/// `{"FuelTrail",0xFFFFFFFE}` closes at `0x00B9A7C0`), so the real path is
/// `Graphics.FuelTrail.{Ignite,Extinguish,Put}`. In retail `Ignite`/`Extinguish` point at the shared
/// no-op stub `0x006D5640` and `Put` returns without effect; there are ZERO script call sites.
/// Corrected 2026-07-26.
pub const NAMESPACE: &str = "Graphics.FuelTrail";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Graphics.FuelTrail";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a7a8;

pub const REQUIRED: &[Required] = &[
    Required { name: "Ignite", corpus_calls: 0 },
    Required { name: "Extinguish", corpus_calls: 0 },
    Required { name: "Put", corpus_calls: 0 },
];

/// Fire FX driver. We don't own the fire/particle system in the reimpl, so ignite/extinguish are
/// faithful no-ops. None of these are called by the game Lua corpus.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Ignite/extinguish drive the real per-object burning state (the fire FX/particle rendering is a
    // render-pass concern; the burning flag is engine state gameplay + the renderer read).
    let h = host.clone();
    b.real("Ignite", lua.create_function(move |_, o: Guid| { h.borrow_mut().fire_ignite(o.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("Extinguish", lua.create_function(move |_, o: Guid| { h.borrow_mut().fire_extinguish(o.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("Put", lua.create_function(move |_, o: Guid| { h.borrow_mut().fire_extinguish(o.raw()); Ok(()) })?)?;

    // Nested under `Graphics`, matching the retail marker-row sub-table.
    b.install_child("Graphics", "FuelTrail")
}
