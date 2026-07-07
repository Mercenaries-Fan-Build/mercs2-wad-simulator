//! `String` engine binding namespace — luaL_Reg table VA 0x00dfda70, 13 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("String")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "StringExt";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "String";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00dfda70;

pub const REQUIRED: &[Required] = &[
    Required { name: "charAt", corpus_calls: 0 },
    Required { name: "charCodeAt", corpus_calls: 0 },
    Required { name: "concat", corpus_calls: 0 },
    Required { name: "indexOf", corpus_calls: 0 },
    Required { name: "lastIndexOf", corpus_calls: 0 },
    Required { name: "slice", corpus_calls: 0 },
    Required { name: "split", corpus_calls: 0 },
    Required { name: "substr", corpus_calls: 0 },
    Required { name: "substring", corpus_calls: 0 },
    Required { name: "toLowerCase", corpus_calls: 0 },
    Required { name: "toString", corpus_calls: 0 },
    Required { name: "toUpperCase", corpus_calls: 0 },
    Required { name: "valueOf", corpus_calls: 0 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
