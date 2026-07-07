//! `Debug` engine binding namespace — luaL_Reg table VA 0x00b98828, 6 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Debug")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Debug";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Debug";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98828;

pub const REQUIRED: &[Required] = &[
    Required { name: "Printf", corpus_calls: 1619 },
    Required { name: "LogError", corpus_calls: 0 },
    Required { name: "LogWarning", corpus_calls: 0 },
    Required { name: "LogInfo", corpus_calls: 0 },
    Required { name: "Assert", corpus_calls: 0 },
    Required { name: "GetCallstack", corpus_calls: 7 },
];

/// `Debug.Printf` is the game's `[lua]` log stream — the reimpl backs it with a real sink (the one
/// place we deliberately diverge from retail, where it too routes to the `0x006D5640` return-0 stub,
/// because the log is load-bearing for bring-up). The remaining `Debug.*` are the retail return-0
/// dev stubs (`0x006D5640`); a no-op here is faithful to retail.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    let printf = lua.create_function(move |lua, args: MultiValue| {
        let s = args
            .iter()
            .next()
            .and_then(|v| lua.coerce_string(v.clone()).ok().flatten())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        h.borrow_mut().log("lua", &s);
        Ok(())
    })?;
    b.real("Printf", printf.clone())?;
    // `print = Debug.Printf` glue in the bootstrap → expose the alias (not part of REQUIRED).
    b.extra("Print", printf)?;

    // Retail-stubbed dev bindings (return-0). No-op is faithful.
    for name in ["LogError", "LogWarning", "LogInfo", "Assert", "GetCallstack"] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    b.install_global(GLOBAL)
}
