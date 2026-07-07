//! `Gui` engine binding namespace — luaL_Reg table VA 0x00b9a398, 38 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Gui")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Gui";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Gui";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a398;

pub const REQUIRED: &[Required] = &[
    Required { name: "AddObjective", corpus_calls: 0 },
    Required { name: "LoadTexture", corpus_calls: 27 },
    Required { name: "GetReticlePosition", corpus_calls: 5 },
    Required { name: "LoadFont", corpus_calls: 1 },
    Required { name: "IsPdaOnSelect", corpus_calls: 2 },
    Required { name: "IsXboxController", corpus_calls: 1 },
    Required { name: "ControllerInUse", corpus_calls: 3 },
    Required { name: "FindGuiLocation", corpus_calls: 2 },
    Required { name: "_MarkerAdd", corpus_calls: 0 },
    Required { name: "_MarkerAddTripwire", corpus_calls: 0 },
    Required { name: "_MarkerAddDisc", corpus_calls: 0 },
    Required { name: "_MarkerAdd3D", corpus_calls: 0 },
    Required { name: "_MarkerSetBlipLimit", corpus_calls: 0 },
    Required { name: "_MarkerAddOld", corpus_calls: 0 },
    Required { name: "_MarkerRemove", corpus_calls: 0 },
    Required { name: "_MarkerSetLocation", corpus_calls: 0 },
    Required { name: "_MarkerSetColor", corpus_calls: 0 },
    Required { name: "_MarkerSetFollowGuid", corpus_calls: 0 },
    Required { name: "_MarkerSetScale", corpus_calls: 0 },
    Required { name: "_MarkerPulse", corpus_calls: 0 },
    Required { name: "_MarkerHaltPulse", corpus_calls: 0 },
    Required { name: "SetFactionMarkerVisibleDistance", corpus_calls: 0 },
    Required { name: "EnableFactionMarkers", corpus_calls: 0 },
    Required { name: "SetFactionMarkerSize", corpus_calls: 0 },
    Required { name: "SetVehicleEntranceMarkerVisibleDistance", corpus_calls: 0 },
    Required { name: "EnableVehicleEntranceMarkers", corpus_calls: 0 },
    Required { name: "SetVehicleEntranceMarkerSize", corpus_calls: 0 },
    Required { name: "EnablePickupMarkers", corpus_calls: 0 },
    Required { name: "SetPickupMarkerSize", corpus_calls: 4 },
    Required { name: "SetPickupMarkerVisibleDistance", corpus_calls: 2 },
    Required { name: "EnablePlayerMarkers", corpus_calls: 10 },
    Required { name: "GetLanguageNum", corpus_calls: 0 },
    Required { name: "GetLanguageName", corpus_calls: 3 },
    Required { name: "DoSigninCheck", corpus_calls: 2 },
    Required { name: "OnShellLoaded", corpus_calls: 2 },
    Required { name: "OnGlobalExit", corpus_calls: 1 },
    Required { name: "ShowLoadingHints", corpus_calls: 4 },
    Required { name: "OutputToPIX", corpus_calls: 0 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
