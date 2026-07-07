//! `Net` engine binding namespace — luaL_Reg table VA 0x00b998d0, 92 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Net")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Net";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Net";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b998d0;

pub const REQUIRED: &[Required] = &[
    Required { name: "IsPlatformConnected", corpus_calls: 2 },
    Required { name: "IsMultiplayer", corpus_calls: 20 },
    Required { name: "IsConnectedToInternet", corpus_calls: 1 },
    Required { name: "IsEnabled", corpus_calls: 0 },
    Required { name: "IsActive", corpus_calls: 24 },
    Required { name: "IsLobby", corpus_calls: 0 },
    Required { name: "IsClient", corpus_calls: 169 },
    Required { name: "IsServer", corpus_calls: 284 },
    Required { name: "IsDedicated", corpus_calls: 0 },
    Required { name: "AutoLobby", corpus_calls: 2 },
    Required { name: "AutoClient", corpus_calls: 2 },
    Required { name: "AutoServer", corpus_calls: 2 },
    Required { name: "GetHostName", corpus_calls: 4 },
    Required { name: "IsMatchmakingInternet", corpus_calls: 10 },
    Required { name: "IsOnlineConnected", corpus_calls: 4 },
    Required { name: "ShouldPlayOnline", corpus_calls: 0 },
    Required { name: "DialogBoxPlayOffline", corpus_calls: 0 },
    Required { name: "DialogBoxMustBeSignInToLive", corpus_calls: 0 },
    Required { name: "IsOnlineEnabled", corpus_calls: 0 },
    Required { name: "DialogBoxPlayLocal", corpus_calls: 0 },
    Required { name: "EnterLobby", corpus_calls: 4 },
    Required { name: "ResetServerList", corpus_calls: 0 },
    Required { name: "ConnectToServer", corpus_calls: 10 },
    Required { name: "StartServer", corpus_calls: 4 },
    Required { name: "Stop", corpus_calls: 4 },
    Required { name: "QuitGame", corpus_calls: 5 },
    Required { name: "EnterFriendsLobby", corpus_calls: 2 },
    Required { name: "ExitFriendsLobby", corpus_calls: 2 },
    Required { name: "SendEvent_ShowMessage", corpus_calls: 1 },
    Required { name: "SendEvent_AddObjective", corpus_calls: 0 },
    Required { name: "SendEvent_RemoveObjective", corpus_calls: 1 },
    Required { name: "SendEvent_AddRadarObjective", corpus_calls: 3 },
    Required { name: "SendEvent_RemoveRadarObjective", corpus_calls: 3 },
    Required { name: "SendEvent_AddMarkerObjective", corpus_calls: 22 },
    Required { name: "SendEvent_RemoveMarkerObjective", corpus_calls: 26 },
    Required { name: "SendEvent_AddPdaObjective", corpus_calls: 1 },
    Required { name: "SendEvent_RemovePdaObjective", corpus_calls: 1 },
    Required { name: "SetLastHeroTeleportLocation", corpus_calls: 1 },
    Required { name: "SendEvent_TeleportPlayer", corpus_calls: 1 },
    Required { name: "SendEvent_TeleportPlayerToHardPoint", corpus_calls: 1 },
    Required { name: "SendEvent_Fanfare", corpus_calls: 1 },
    Required { name: "SendEvent_CloseFanfare", corpus_calls: 1 },
    Required { name: "SendEvent_ObjectiveMessage", corpus_calls: 2 },
    Required { name: "SendEvent_Support", corpus_calls: 1 },
    Required { name: "SendEvent_AddSupportItem", corpus_calls: 1 },
    Required { name: "SendEvent_RemoveSupportItem", corpus_calls: 1 },
    Required { name: "SendEvent_RecruitsUnlocked", corpus_calls: 5 },
    Required { name: "SendEvent_RevivePlayer", corpus_calls: 4 },
    Required { name: "SendEvent_RequestPosition", corpus_calls: 2 },
    Required { name: "SendEvent_SetObjectiveTraySlotText", corpus_calls: 2 },
    Required { name: "SendEvent_SetObjectiveTraySlotImage", corpus_calls: 1 },
    Required { name: "SendEvent_ClearObjectiveTraySlot", corpus_calls: 1 },
    Required { name: "SendEvent_ShowMovie", corpus_calls: 2 },
    Required { name: "SendEvent_HideMovie", corpus_calls: 2 },
    Required { name: "BeginLayerEventGroup", corpus_calls: 2 },
    Required { name: "EndLayerEventGroup", corpus_calls: 2 },
    Required { name: "GrantAchievement", corpus_calls: 3 },
    Required { name: "KickPlayer", corpus_calls: 0 },
    Required { name: "ApplyCachedFactionRelations", corpus_calls: 2 },
    Required { name: "SendEvent_EnableHeroWeapons", corpus_calls: 0 },
    Required { name: "SendEvent_AddDangerousBuilding", corpus_calls: 1 },
    Required { name: "SendEvent_RemoveDangerousBuilding", corpus_calls: 1 },
    Required { name: "SendEvent_SetOccupiedDangerousBuilding", corpus_calls: 1 },
    Required { name: "SendEvent_AddRandomDangerousBuilding", corpus_calls: 1 },
    Required { name: "SendEvent_TextFanfare", corpus_calls: 1 },
    Required { name: "SendEvent_CardFanfare", corpus_calls: 1 },
    Required { name: "SendEvent_HVTFanfare", corpus_calls: 1 },
    Required { name: "SendEvent_UnlockFanfare", corpus_calls: 1 },
    Required { name: "SendEvent_BatchUnlockFanfare", corpus_calls: 2 },
    Required { name: "SendEvent_ForceClientTether", corpus_calls: 1 },
    Required { name: "SendEvent_PursuitMessage", corpus_calls: 3 },
    Required { name: "SetPursuitReportingState", corpus_calls: 4 },
    Required { name: "SendEvent_AddHqPdaBlip", corpus_calls: 1 },
    Required { name: "SendEvent_RemoveHqPdaBlip", corpus_calls: 1 },
    Required { name: "SendEvent_AddPmcPdaBlip", corpus_calls: 2 },
    Required { name: "SendEvent_RemovePmcPdaBlip", corpus_calls: 2 },
    Required { name: "SendEvent_AddPDAMission", corpus_calls: 2 },
    Required { name: "SendEvent_RemovePDAMission", corpus_calls: 3 },
    Required { name: "LoadMissionSpiel", corpus_calls: 2 },
    Required { name: "UnloadMissionSpiel", corpus_calls: 2 },
    Required { name: "SetBriefingInterior", corpus_calls: 8 },
    Required { name: "SetBriefingStarters", corpus_calls: 4 },
    Required { name: "SetBriefingCheapCinematic", corpus_calls: 10 },
    Required { name: "SetLoadingScreen", corpus_calls: 15 },
    Required { name: "SetShootingGalleryBorder", corpus_calls: 1 },
    Required { name: "SetTutorialMessage", corpus_calls: 2 },
    Required { name: "SendEvent_JoinPOForceRequest", corpus_calls: 1 },
    Required { name: "SendCustomEvent", corpus_calls: 191 },
    Required { name: "DoneReloadingLayers", corpus_calls: 6 },
    Required { name: "IsReadyToTether", corpus_calls: 1 },
    Required { name: "HasPlayerUnlockedCode", corpus_calls: 3 },
    Required { name: "UpdatePresence", corpus_calls: 0 },
];

/// Not yet implemented — installs no global; every [`REQUIRED`] entry counts as a remaining stub.
pub fn install(_lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    Ok(Installed::none())
}
