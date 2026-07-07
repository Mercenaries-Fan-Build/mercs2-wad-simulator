//! `socket` engine binding namespace — luaL_Reg table VA 0x00cdf098, 3 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("socket")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Socket";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "socket";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00cdf098;

pub const REQUIRED: &[Required] = &[
    Required { name: "getaddrinfo", corpus_calls: 0 },
    Required { name: "getnameinfo", corpus_calls: 0 },
    Required { name: "freeaddrinfo", corpus_calls: 0 },
];

/// Low-level DNS/socket resolver helpers. Faithful for a single-player boot: all no-op stubs — an
/// offline SP session never resolves or opens sockets. (LuaSocket-style resolver surface.)
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    for r in REQUIRED {
        b.stub(
            r.name,
            lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
        )?;
    }
    b.install_global(GLOBAL)
}
