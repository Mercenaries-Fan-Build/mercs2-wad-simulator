//! `Bloom` engine binding namespace — luaL_Reg table VA 0x00b9a6b0, 7 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Bloom")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Bloom";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Bloom";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a6b0;

pub const REQUIRED: &[Required] = &[
    Required { name: "SetBlurRadius", corpus_calls: 1 },
    Required { name: "SetThreshold", corpus_calls: 1 },
    Required { name: "SetMultiplier", corpus_calls: 1 },
    Required { name: "SetAmount", corpus_calls: 0 },
    Required { name: "SetTargetLuminance", corpus_calls: 0 },
    Required { name: "SetAdaptiveLuminancePercent", corpus_calls: 0 },
    Required { name: "SetAdaptiveLuminanceScale", corpus_calls: 0 },
];

/// HDR-bloom post tuning setters — presentation only. The fixed-function renderer has no bloom pass,
/// so each is a faithful no-op. All are setters; none return a value the game's Lua reads.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;
    for name in [
        "SetBlurRadius",
        "SetThreshold",
        "SetMultiplier",
        "SetAmount",
        "SetTargetLuminance",
        "SetAdaptiveLuminancePercent",
        "SetAdaptiveLuminanceScale",
    ] {
        b.stub(name, lua.create_function(|_, _: mlua::MultiValue| Ok(()))?)?;
    }
    b.install_global(GLOBAL)
}
