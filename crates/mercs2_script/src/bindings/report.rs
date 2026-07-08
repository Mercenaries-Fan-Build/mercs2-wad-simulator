//! `Report` engine binding namespace — luaL_Reg table VA 0x00b98f64, 5 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Report")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Report";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Report";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98f64;

pub const REQUIRED: &[Required] = &[
    Required { name: "Init", corpus_calls: 2 },
    Required { name: "GetInfractions", corpus_calls: 1 },
    Required { name: "Completed", corpus_calls: 1 },
    Required { name: "Failed", corpus_calls: 1 },
    Required { name: "SetDelay", corpus_calls: 3 },
];

/// Faction-infraction telemetry/reporting surface. Faithful for a single-player boot: all no-op
/// stubs. The sole getter, `GetInfractions`, is consumed under an `if tInfractions then` guard in
/// `mrxfactionmanager.lua`, so a no-op (nil) return simply skips the optional mood-adjustment path —
/// a faithful degrade for a build with reporting disabled.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // The faction-report lifecycle → the real faction manager (mercs2_faction mood report).
    let h = host.clone();
    b.real("Init", lua.create_function(move |_, faction: i64| { h.borrow_mut().report_init(faction as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetDelay", lua.create_function(move |_, secs: f32| { h.borrow_mut().report_set_delay(secs); Ok(()) })?)?;
    let h = host.clone();
    b.real("Completed", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().report_finish(true); Ok(()) })?)?;
    let h = host.clone();
    b.real("Failed", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().report_finish(false); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetInfractions", lua.create_function(move |_, _: mlua::MultiValue| Ok(h.borrow().report_infractions()))?)?;

    b.install_global(GLOBAL)
}
