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

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

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

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
