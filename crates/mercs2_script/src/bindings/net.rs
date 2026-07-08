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
use super::{Installed, NsBuilder, Required};

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

/// Session/host query getters that gate control flow in the game Lua and therefore need real bodies
/// returning the single-player-faithful default. All of these are `false` in an SP boot; `IsServer`
/// (below) is the lone `true` because the retail engine runs the local SP game as its own authoritative
/// server (scripts guard authoritative logic with `if Net.IsServer() then ...`).
///
/// Notes on the corpus-confirmed values:
/// - `IsActive` → `false`: `pircon002.lua:475` splits the mission cash reward in half and sets a
///   Player2 bonus when `Net.IsActive()`; the `else` branch (full reward, no P2) is the SP path. A
///   `true` here would silently halve SP rewards.
/// - `IsMultiplayer`/`IsClient` → `false`: SP is a single local player, never a joined client.
/// - `AutoClient`/`AutoServer`/`AutoLobby` → `false`: `gamebootstrap.Start()` only auto-connects /
///   auto-hosts when these are set; `false` routes a normal boot to the shell / `LoadLevel` path.
/// - `IsPlatformConnected`/`IsConnectedToInternet`/`IsMatchmakingInternet`/`IsOnlineConnected` →
///   `false`: no online/matchmaking session in an offline SP boot.
/// - `IsReadyToTether` → `false`: co-op tether has no meaning without a second player.
/// - `HasPlayerUnlockedCode` → `false`: retail default until the player enters a promo/unlock code
///   (gates the extra wardrobe outfit in `wifpmcinterior.lua`).

/// SP-faithful boot slice. The control-flow session/host getters get real bodies returning the SP
/// default (`IsServer` → `true`; everything in [`REAL_FALSE_GETTERS`] → `false`). Every other cfunc —
/// replication (`SendCustomEvent`, all `SendEvent_*`), lobby/session control (`StartServer`,
/// `ConnectToServer`, `EnterLobby`, …), the net-routed briefing/HUD setters, telemetry
/// (`GrantAchievement`, `UpdatePresence`) and the uncalled online-dialog helpers — is a faithful
/// no-op stub for a single-player boot. The `mercs2_net` session/host model is wired at a separate
/// seam, deliberately not here.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Session-mode getters → the real NetState (default: SP is its own offline server).
    let h = host.clone();
    b.real("IsServer", lua.create_function(move |_, ()| Ok(h.borrow().net_is_server()))?)?;
    let h = host.clone();
    b.real("IsClient", lua.create_function(move |_, ()| Ok(h.borrow().net_is_client()))?)?;
    let h = host.clone();
    b.real("IsActive", lua.create_function(move |_, ()| Ok(h.borrow().net_is_active()))?)?;
    let h = host.clone();
    b.real("IsMultiplayer", lua.create_function(move |_, ()| Ok(h.borrow().net_is_multiplayer()))?)?;
    let h = host.clone();
    b.real("IsLobby", lua.create_function(move |_, ()| Ok(h.borrow().net_is_lobby()))?)?;
    let h = host.clone();
    b.real("GetHostName", lua.create_function(move |_, ()| Ok(h.borrow().net_host_name()))?)?;

    // Session control → the real NetState.
    let h = host.clone();
    b.real("StartServer", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_session_start("server", None); Ok(()) })?)?;
    let h = host.clone();
    b.real("ConnectToServer", lua.create_function(move |_, hn: Option<String>| { h.borrow_mut().net_session_start("client", hn.as_deref()); Ok(()) })?)?;
    let h = host.clone();
    b.real("EnterLobby", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_session_start("lobby", None); Ok(()) })?)?;
    let h = host.clone();
    b.real("AutoServer", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_session_start("server", None); Ok(true) })?)?;
    let h = host.clone();
    b.real("AutoClient", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_session_start("client", None); Ok(true) })?)?;
    let h = host.clone();
    b.real("AutoLobby", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_session_start("lobby", None); Ok(true) })?)?;
    let h = host.clone();
    b.real("Stop", lua.create_function(move |_, _: mlua::MultiValue| { h.borrow_mut().net_stop(); Ok(()) })?)?;

    // Offline platform/matchmaking getters — all false in an offline single-player boot.
    const OFFLINE_FALSE: &[&str] = &[
        "IsConnectedToInternet", "IsPlatformConnected", "IsMatchmakingInternet", "IsOnlineConnected",
        "IsReadyToTether", "HasPlayerUnlockedCode",
    ];
    for &name in OFFLINE_FALSE {
        b.real(name, lua.create_function(|_, ()| Ok(false))?)?;
    }

    // Everything else: a faithful no-op for SP (replication, net-routed events/setters, telemetry,
    // online dialogs) — the co-op restore silo backs these against mercs2_net.
    const BACKED: &[&str] = &[
        "IsServer", "IsClient", "IsActive", "IsMultiplayer", "IsLobby", "GetHostName", "StartServer",
        "ConnectToServer", "EnterLobby", "AutoServer", "AutoClient", "AutoLobby", "Stop",
    ];
    for r in REQUIRED {
        if BACKED.contains(&r.name) || OFFLINE_FALSE.contains(&r.name) {
            continue;
        }
        b.stub(
            r.name,
            lua.create_function(|_, _: mlua::MultiValue| Ok(()))?,
        )?;
    }

    b.install_global(GLOBAL)
}
