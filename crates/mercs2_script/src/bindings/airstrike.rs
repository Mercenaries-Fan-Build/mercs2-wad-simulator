//! `Airstrike` engine binding namespace — luaL_Reg table VA 0x00b9a8c8, 12 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Airstrike")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Airstrike";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Airstrike";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a8c8;

pub const REQUIRED: &[Required] = &[
    Required { name: "SpawnCarpetBombLine", corpus_calls: 2 },
    Required { name: "SpawnPlaneNew", corpus_calls: 0 },
    Required { name: "SpawnOrdnance", corpus_calls: 20 },
    Required { name: "SpawnTargettedOrdnance", corpus_calls: 7 },
    Required { name: "ConeSpawn", corpus_calls: 14 },
    Required { name: "FindExitPoint", corpus_calls: 0 },
    Required { name: "EquipDesignator", corpus_calls: 3 },
    Required { name: "RemoveDesignator", corpus_calls: 2 },
    Required { name: "RefillDesignator", corpus_calls: 2 },
    Required { name: "Flyby", corpus_calls: 21 },
    Required { name: "SpawnDirectedObject", corpus_calls: 3 },
    Required { name: "FindDesignatorOwner", corpus_calls: 1 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
