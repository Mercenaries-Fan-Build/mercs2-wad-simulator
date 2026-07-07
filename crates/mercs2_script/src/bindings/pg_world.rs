//! `Pg` engine binding namespace — luaL_Reg table VA 0x00b99e28, 24 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Pg")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "PgWorld";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Pg";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99e28;

pub const REQUIRED: &[Required] = &[
    Required { name: "SpawnHomingProjectile", corpus_calls: 0 },
    Required { name: "CreateRegion", corpus_calls: 0 },
    Required { name: "Subdue", corpus_calls: 0 },
    Required { name: "GetModelBBoxExtents", corpus_calls: 0 },
    Required { name: "SpawnWithModel", corpus_calls: 0 },
    Required { name: "FormatTime", corpus_calls: 0 },
    Required { name: "DrawPath", corpus_calls: 0 },
    Required { name: "IsInstallable", corpus_calls: 0 },
    Required { name: "InstallToHDD", corpus_calls: 0 },
    Required { name: "UseExistingInstall", corpus_calls: 0 },
    Required { name: "Search", corpus_calls: 0 },
    Required { name: "DumpAssets", corpus_calls: 0 },
    Required { name: "DumpAssetsDiff", corpus_calls: 0 },
    Required { name: "DumpTextures", corpus_calls: 0 },
    Required { name: "DumpAssetMemory", corpus_calls: 0 },
    Required { name: "DumpMemory", corpus_calls: 0 },
    Required { name: "LoadScript", corpus_calls: 0 },
    Required { name: "LoadFunctions", corpus_calls: 0 },
    Required { name: "LoadData", corpus_calls: 0 },
    Required { name: "DescribeGuid", corpus_calls: 0 },
    Required { name: "SetQGrey", corpus_calls: 0 },
    Required { name: "ActivateAlarm", corpus_calls: 0 },
    Required { name: "ToggleAlarm", corpus_calls: 0 },
    Required { name: "DumpStats", corpus_calls: 0 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
