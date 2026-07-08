//! `Fire` engine binding namespace — luaL_Reg table VA 0x00b9a7a8, 3 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Fire")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Fire";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Fire";
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
    b.real("Ignite", lua.create_function(move |_, o: i64| { h.borrow_mut().fire_ignite(o as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("Extinguish", lua.create_function(move |_, o: i64| { h.borrow_mut().fire_extinguish(o as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("Put", lua.create_function(move |_, o: i64| { h.borrow_mut().fire_extinguish(o as u64); Ok(()) })?)?;

    b.install_global(GLOBAL)
}
