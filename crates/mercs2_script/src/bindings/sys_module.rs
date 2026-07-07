//! `_SYS` engine binding namespace — luaL_Reg table VA 0x00b9a854, 6 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! `_SYS` is the engine's low-level module-system bootstrap surface (`docs/lua_engine_bindings_audit.md`
//! §_SYS): `_IMPORT(env, name)` / `_INHERIT(env, name)` / `_DYNAMIC_IMPORT` / `_DYNAMIC_REMOVE` /
//! `_MODULEINDEX` / `_GETFENV(level)`. In this reimpl the authentic module system is implemented
//! **natively in Rust** (`crate::ModuleSystem::import`/`inherit`, wired via the compat prelude), so
//! these raw C primitives are not the load path here. `_GETFENV` is genuinely computable and gets a
//! real body (returns the environment table); the five module-loader primitives are faithful no-ops
//! (`b.stub`) — the native module system, not these cfuncs, resolves imports in this build.

use mlua::{Lua, MultiValue, Result as LuaResult, Value};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "SysModule";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "_SYS";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a854;

pub const REQUIRED: &[Required] = &[
    Required { name: "_IMPORT", corpus_calls: 0 },
    Required { name: "_INHERIT", corpus_calls: 0 },
    Required { name: "_DYNAMIC_IMPORT", corpus_calls: 0 },
    Required { name: "_DYNAMIC_REMOVE", corpus_calls: 0 },
    Required { name: "_MODULEINDEX", corpus_calls: 0 },
    Required { name: "_GETFENV", corpus_calls: 0 },
];

/// `_GETFENV` is real (returns the environment); the module-loader primitives are faithful no-ops.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // _GETFENV(level) -> environment table. Lua 5.4 has no per-level function environments; the
    // authentic single shared script env is the globals table, which is what callers read/write.
    b.real(
        "_GETFENV",
        lua.create_function(|lua, _level: Option<i64>| Ok(lua.globals()))?,
    )?;

    // Module-loader primitives: the native ModuleSystem (Rust) is the real load path in this build, so
    // these raw engine cfuncs are deliberate no-ops here. _IMPORT/_INHERIT return nil (the engine's
    // own success return); the dynamic + index hooks likewise no-op.
    for name in ["_IMPORT", "_INHERIT", "_DYNAMIC_IMPORT", "_DYNAMIC_REMOVE", "_MODULEINDEX"] {
        b.stub(
            name,
            lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?,
        )?;
    }

    b.install_global(GLOBAL)
}
