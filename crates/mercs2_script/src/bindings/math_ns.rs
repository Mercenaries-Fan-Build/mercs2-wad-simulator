//! `Math` engine binding namespace — luaL_Reg table VA 0x00b99be8, 17 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Math")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Math";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Math";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99be8;

pub const REQUIRED: &[Required] = &[
    Required { name: "abs", corpus_calls: 4 },
    Required { name: "floor", corpus_calls: 14 },
    Required { name: "ceil", corpus_calls: 3 },
    Required { name: "round", corpus_calls: 0 },
    Required { name: "max", corpus_calls: 9 },
    Required { name: "min", corpus_calls: 8 },
    Required { name: "exp", corpus_calls: 0 },
    Required { name: "pow", corpus_calls: 0 },
    Required { name: "deg", corpus_calls: 0 },
    Required { name: "rad", corpus_calls: 0 },
    Required { name: "randi", corpus_calls: 28 },
    Required { name: "randf", corpus_calls: 10 },
    Required { name: "GetXZHeading", corpus_calls: 10 },
    Required { name: "Normalize", corpus_calls: 32 },
    Required { name: "CrossProduct", corpus_calls: 0 },
    Required { name: "Length", corpus_calls: 5 },
    Required { name: "PolarToRect", corpus_calls: 1 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
