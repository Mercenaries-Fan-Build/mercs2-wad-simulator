//! `Atmosphere` engine binding namespace — luaL_Reg table VA 0x00b9a578, 37 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Atmosphere")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Atmosphere";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Atmosphere";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a578;

pub const REQUIRED: &[Required] = &[
    Required { name: "Begin", corpus_calls: 11 },
    Required { name: "End", corpus_calls: 11 },
    Required { name: "SetTime", corpus_calls: 1 },
    Required { name: "SetTimeSpeed", corpus_calls: 1 },
    Required { name: "SetLightIntensity", corpus_calls: 1 },
    Required { name: "SetLightModifier", corpus_calls: 0 },
    Required { name: "SetLightAngle", corpus_calls: 0 },
    Required { name: "SetAmbientColor", corpus_calls: 1 },
    Required { name: "SetAmbientCube", corpus_calls: 1 },
    Required { name: "SetRimColor", corpus_calls: 0 },
    Required { name: "SetTurbinity", corpus_calls: 0 },
    Required { name: "SetInscatteringMultiplier", corpus_calls: 1 },
    Required { name: "SetExtinctionMultiplier", corpus_calls: 1 },
    Required { name: "SetBetaRayMultiplier", corpus_calls: 1 },
    Required { name: "SetBetaMieMultiplier", corpus_calls: 1 },
    Required { name: "SetHenyeyGreensteinConst", corpus_calls: 1 },
    Required { name: "SetAtmosphere", corpus_calls: 0 },
    Required { name: "SetHaze", corpus_calls: 0 },
    Required { name: "SetWindDirection", corpus_calls: 0 },
    Required { name: "SetParticlesPerSecond", corpus_calls: 0 },
    Required { name: "Change", corpus_calls: 0 },
    Required { name: "ChangeLineRegionSetting", corpus_calls: 20 },
    Required { name: "GetLineRegionSetting", corpus_calls: 0 },
    Required { name: "GetLineRegion", corpus_calls: 0 },
    Required { name: "Restore", corpus_calls: 0 },
    Required { name: "GetCurrentSetting", corpus_calls: 0 },
    Required { name: "EnableImmediatelyChangeMode", corpus_calls: 2 },
    Required { name: "SetRainSpeed", corpus_calls: 0 },
    Required { name: "SetRainDensity", corpus_calls: 0 },
    Required { name: "GetValue", corpus_calls: 3 },
    Required { name: "SetValue", corpus_calls: 109 },
    Required { name: "GetColorValue", corpus_calls: 0 },
    Required { name: "SetColorValue", corpus_calls: 78 },
    Required { name: "GetIntValue", corpus_calls: 0 },
    Required { name: "SetIntValue", corpus_calls: 0 },
    Required { name: "IsInterpolating", corpus_calls: 1 },
    Required { name: "SetSky", corpus_calls: 5 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
