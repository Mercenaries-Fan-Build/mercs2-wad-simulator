//! `Player` engine binding namespace — luaL_Reg table VA 0x00b98fc0, 107 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Player")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Player";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Player";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98fc0;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetCharacter", corpus_calls: 93 },
    Required { name: "GetControlledObject", corpus_calls: 13 },
    Required { name: "GetSeat", corpus_calls: 0 },
    Required { name: "GetName", corpus_calls: 0 },
    Required { name: "GetCameraXZHeading", corpus_calls: 3 },
    Required { name: "GetViewport", corpus_calls: 0 },
    Required { name: "GetViewportId", corpus_calls: 4 },
    Required { name: "GetCamera", corpus_calls: 25 },
    Required { name: "TeleportCamera", corpus_calls: 3 },
    Required { name: "CheckSpawnPos", corpus_calls: 0 },
    Required { name: "SetPDAMapMode", corpus_calls: 3 },
    Required { name: "SetPDAMapModeCallback", corpus_calls: 7 },
    Required { name: "SetPDAMapModeCancelCallback", corpus_calls: 1 },
    Required { name: "RequestPDAMapModeExit", corpus_calls: 1 },
    Required { name: "RequestPDAMapModeCancel", corpus_calls: 1 },
    Required { name: "GetTargetUnderReticle", corpus_calls: 4 },
    Required { name: "SetSatelliteScanMode", corpus_calls: 1 },
    Required { name: "SetupSatelliteScan", corpus_calls: 0 },
    Required { name: "SetSatelliteScanCallbacks", corpus_calls: 0 },
    Required { name: "AddSatelliteScanTarget", corpus_calls: 0 },
    Required { name: "SetSatelliteScanPaused", corpus_calls: 0 },
    Required { name: "SetCinematicMode", corpus_calls: 9 },
    Required { name: "InCinematicMode", corpus_calls: 2 },
    Required { name: "SetOutBoundary", corpus_calls: 5 },
    Required { name: "GetOutBoundary", corpus_calls: 0 },
    Required { name: "IsInWarningZone", corpus_calls: 0 },
    Required { name: "AddBoundary", corpus_calls: 2 },
    Required { name: "RemoveBoundary", corpus_calls: 1 },
    Required { name: "RemoveAllBoundary", corpus_calls: 1 },
    Required { name: "GetAllBoundaryGuid", corpus_calls: 0 },
    Required { name: "SetBoundaryCallback", corpus_calls: 1 },
    Required { name: "IsPositionOutBoundary", corpus_calls: 2 },
    Required { name: "IsBoundaryDeath", corpus_calls: 5 },
    Required { name: "SetInputEnabled", corpus_calls: 5 },
    Required { name: "SetSurvivalMode", corpus_calls: 4 },
    Required { name: "SetHealthClamp", corpus_calls: 4 },
    Required { name: "SetSurvivalModeCallback", corpus_calls: 0 },
    Required { name: "IsCoopMultiplayer", corpus_calls: 5 },
    Required { name: "GetPrimaryPlayer", corpus_calls: 64 },
    Required { name: "GetSecondaryPlayer", corpus_calls: 27 },
    Required { name: "GetPrimaryCharacter", corpus_calls: 96 },
    Required { name: "GetSecondaryCharacter", corpus_calls: 143 },
    Required { name: "GetMaximumPlayers", corpus_calls: 4 },
    Required { name: "GetCurrentPlayers", corpus_calls: 18 },
    Required { name: "GetPlayer", corpus_calls: 13 },
    Required { name: "GetAllPlayers", corpus_calls: 83 },
    Required { name: "GetPlayerId", corpus_calls: 3 },
    Required { name: "IsJoined", corpus_calls: 0 },
    Required { name: "IsLocal", corpus_calls: 53 },
    Required { name: "IsRemote", corpus_calls: 6 },
    Required { name: "GetLocalId", corpus_calls: 0 },
    Required { name: "GetMaximumLocalPlayers", corpus_calls: 0 },
    Required { name: "GetCurrentLocalPlayers", corpus_calls: 0 },
    Required { name: "GetLocalPlayer", corpus_calls: 107 },
    Required { name: "GetLocalPlayerId", corpus_calls: 3 },
    Required { name: "GetLocalCharacter", corpus_calls: 165 },
    Required { name: "GetAnyCharacter", corpus_calls: 223 },
    Required { name: "GetAllCharacters", corpus_calls: 26 },
    Required { name: "CreatePlayer", corpus_calls: 2 },
    Required { name: "DestroyPlayer", corpus_calls: 2 },
    Required { name: "ClearPlayerDB", corpus_calls: 2 },
    Required { name: "AttachToCharacter", corpus_calls: 4 },
    Required { name: "DetachFromCharacter", corpus_calls: 4 },
    Required { name: "BindToLocal", corpus_calls: 2 },
    Required { name: "BindToRemote", corpus_calls: 2 },
    Required { name: "Unbind", corpus_calls: 2 },
    Required { name: "SetPlayerJoinedCallback", corpus_calls: 2 },
    Required { name: "SetPlayerLeftCallback", corpus_calls: 2 },
    Required { name: "RemovePlayerJoinedCallback", corpus_calls: 2 },
    Required { name: "RemovePlayerLeftCallback", corpus_calls: 2 },
    Required { name: "GetPlayerStart", corpus_calls: 4 },
    Required { name: "SetPlayerStart", corpus_calls: 0 },
    Required { name: "ClaimSeat", corpus_calls: 0 },
    Required { name: "UnClaimSeat", corpus_calls: 0 },
    Required { name: "GetRetryPosition", corpus_calls: 0 },
    Required { name: "SetWaitForInGame", corpus_calls: 3 },
    Required { name: "GetAllTargetMarkerPos", corpus_calls: 4 },
    Required { name: "SetSeatMovementLocks", corpus_calls: 7 },
    Required { name: "SetVehicleControlsLock", corpus_calls: 0 },
    Required { name: "GetControlBindingType", corpus_calls: 2 },
    Required { name: "ClearGPS", corpus_calls: 5 },
    Required { name: "SetScopeEnabled", corpus_calls: 6 },
    Required { name: "GetCash", corpus_calls: 8 },
    Required { name: "SetCash", corpus_calls: 8 },
    Required { name: "AddCash", corpus_calls: 1 },
    Required { name: "GetFuel", corpus_calls: 7 },
    Required { name: "SetFuel", corpus_calls: 12 },
    Required { name: "AddFuel", corpus_calls: 1 },
    Required { name: "GetFuelCapacity", corpus_calls: 7 },
    Required { name: "SetFuelCapacity", corpus_calls: 1 },
    Required { name: "GetProfileCharacter", corpus_calls: 0 },
    Required { name: "SetProfileCharacter", corpus_calls: 0 },
    Required { name: "GetProfileUpgrade", corpus_calls: 0 },
    Required { name: "SetProfileUpgrade", corpus_calls: 0 },
    Required { name: "GetProfileCostume", corpus_calls: 5 },
    Required { name: "SetProfileCostume", corpus_calls: 4 },
    Required { name: "GetAvailableCostumes", corpus_calls: 2 },
    Required { name: "SetAvailableCostumes", corpus_calls: 3 },
    Required { name: "SetOutfit", corpus_calls: 8 },
    Required { name: "SetGrappleEnabled", corpus_calls: 1 },
    Required { name: "SetInPmc", corpus_calls: 6 },
    Required { name: "SetAimMode", corpus_calls: 17 },
    Required { name: "SetVehicleDisguise", corpus_calls: 6 },
    Required { name: "GetVehicleDisguise", corpus_calls: 6 },
    Required { name: "VehicleDisguise", corpus_calls: 2 },
    Required { name: "GetVehicleDisguiseState", corpus_calls: 2 },
    Required { name: "SetSwimmingSearchRadius", corpus_calls: 0 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
