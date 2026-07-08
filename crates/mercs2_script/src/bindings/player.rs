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

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

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

/// Economy (cash/fuel) + the player/character GUID getters — the most-called `Player` cfuncs
/// (`GetAnyCharacter` 223, `GetLocalCharacter` 165, `GetCash`/`SetCash`, `GetFuel`/`SetFuel`). Backed
/// by [`crate::EngineHost`]; a `0` GUID maps to Lua `nil` so the game's `if not uChar` control flow is
/// authentic. The remaining ~90 `Player.*` cfuncs (viewports, PDA, boundaries, MP session) are later.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // --- economy ---
    let h = host.clone();
    b.real("GetCash", lua.create_function(move |_, ()| Ok(h.borrow().player_cash()))?)?;
    let h = host.clone();
    b.real("SetCash", lua.create_function(move |_, n: i64| { h.borrow_mut().player_set_cash(n); Ok(()) })?)?;
    let h = host.clone();
    b.real("AddCash", lua.create_function(move |_, n: i64| {
        let total = h.borrow().player_cash() + n;
        h.borrow_mut().player_set_cash(total);
        Ok(total)
    })?)?;
    let h = host.clone();
    b.real("GetFuel", lua.create_function(move |_, ()| Ok(h.borrow().player_fuel()))?)?;
    let h = host.clone();
    b.real("SetFuel", lua.create_function(move |_, n: i64| { h.borrow_mut().player_set_fuel(n); Ok(()) })?)?;
    let h = host.clone();
    b.real("AddFuel", lua.create_function(move |_, n: i64| {
        let total = h.borrow().player_fuel() + n;
        h.borrow_mut().player_set_fuel(total);
        Ok(total)
    })?)?;
    let h = host.clone();
    b.real("GetFuelCapacity", lua.create_function(move |_, ()| Ok(h.borrow().player_fuel_capacity()))?)?;
    let h = host.clone();
    b.real("SetFuelCapacity", lua.create_function(move |_, n: i64| { h.borrow_mut().player_set_fuel_capacity(n); Ok(()) })?)?;

    // --- player / character GUID getters (0 -> nil) ---
    fn guid_opt(g: u64) -> Option<i64> {
        if g == 0 { None } else { Some(g as i64) }
    }
    let h = host.clone();
    b.real("GetLocalPlayer", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_local_player())))?)?;
    let h = host.clone();
    b.real("GetAnyCharacter", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_any_character())))?)?;
    let h = host.clone();
    b.real("GetLocalCharacter", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_local_character())))?)?;
    let h = host.clone();
    b.real("GetPrimaryCharacter", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_primary_character())))?)?;
    let h = host.clone();
    b.real("GetSecondaryCharacter", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_secondary_character())))?)?;
    let h = host.clone();
    b.real("IsLocal", lua.create_function(move |_, guid: i64| Ok(h.borrow().player_is_local(guid as u64)))?)?;
    let h = host.clone();
    b.real("IsRemote", lua.create_function(move |_, guid: i64| Ok(!h.borrow().player_is_local(guid as u64)))?)?;

    // --- identity / session (real getters backed by host player↔character state) ---
    let h = host.clone();
    b.real("GetPlayer", lua.create_function(move |_, id: Option<i64>| Ok(guid_opt(h.borrow().player_get_player(id.unwrap_or(0)))))?)?;
    let h = host.clone();
    b.real("GetPrimaryPlayer", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_primary_player())))?)?;
    let h = host.clone();
    b.real("GetSecondaryPlayer", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow().player_secondary_player())))?)?;
    let h = host.clone();
    b.real("GetCharacter", lua.create_function(move |_, player: i64| Ok(guid_opt(h.borrow().player_character_of(player as u64))))?)?;
    let h = host.clone();
    b.real("GetControlledObject", lua.create_function(move |_, player: i64| Ok(guid_opt(h.borrow().player_controlled_object(player as u64))))?)?;
    let h = host.clone();
    b.real("GetPlayerId", lua.create_function(move |_, player: i64| Ok(h.borrow().player_id_of(player as u64)))?)?;
    let h = host.clone();
    b.real("GetLocalPlayerId", lua.create_function(move |_, ()| Ok(h.borrow().player_id_of(h.borrow().player_local_player())))?)?;
    let h = host.clone();
    b.real("GetLocalId", lua.create_function(move |_, ()| Ok(h.borrow().player_id_of(h.borrow().player_local_player())))?)?;
    let h = host.clone();
    b.real("GetName", lua.create_function(move |_, player: i64| Ok(h.borrow().player_name(player as u64)))?)?;
    let h = host.clone();
    b.real("GetMaximumPlayers", lua.create_function(move |_, ()| Ok(h.borrow().player_max_players()))?)?;
    let h = host.clone();
    b.real("GetMaximumLocalPlayers", lua.create_function(move |_, ()| Ok(h.borrow().player_max_players()))?)?;
    let h = host.clone();
    b.real("GetCurrentPlayers", lua.create_function(move |_, ()| Ok(h.borrow().player_current_players()))?)?;
    let h = host.clone();
    b.real("GetCurrentLocalPlayers", lua.create_function(move |_, ()| Ok(h.borrow().player_current_players()))?)?;
    let h = host.clone();
    b.real("IsCoopMultiplayer", lua.create_function(move |_, ()| Ok(h.borrow().player_is_coop()))?)?;
    let h = host.clone();
    b.real("IsJoined", lua.create_function(move |_, player: i64| Ok(h.borrow().player_is_joined(player as u64)))?)?;
    // GUID list getters -> Lua array table (1-based).
    fn guid_table(lua: &Lua, guids: Vec<u64>) -> LuaResult<mlua::Table> {
        let t = lua.create_table()?;
        for (i, g) in guids.into_iter().enumerate() {
            t.set(i + 1, g as i64)?;
        }
        Ok(t)
    }
    let h = host.clone();
    b.real("GetAllPlayers", lua.create_function(move |lua, ()| guid_table(lua, h.borrow().player_all_players()))?)?;
    let h = host.clone();
    b.real("GetAllCharacters", lua.create_function(move |lua, ()| guid_table(lua, h.borrow().player_all_characters()))?)?;

    // --- profile (real: the host's hero fields; the game reads these for the LOOK/economy) ---
    let h = host.clone();
    b.real("GetSelectedCharacter", lua.create_function(move |_, ()| Ok(h.borrow().player_selected_character()))?)?;
    let h = host.clone();
    b.real("GetProfileCharacter", lua.create_function(move |_, ()| Ok(h.borrow().player_profile_character()))?)?;
    let h = host.clone();
    b.real("GetProfileUpgrade", lua.create_function(move |_, ()| Ok(h.borrow().player_profile_upgrade()))?)?;
    let h = host.clone();
    b.real("GetProfileCostume", lua.create_function(move |_, ()| Ok(h.borrow().player_profile_costume()))?)?;
    let h = host.clone();
    b.real("GetAvailableCostumes", lua.create_function(move |lua, ()| {
        let t = lua.create_table()?;
        for (i, c) in h.borrow().player_available_costumes().into_iter().enumerate() {
            t.set(i + 1, c)?;
        }
        Ok(t)
    })?)?;
    let h = host.clone();
    b.real("SetProfileCostume", lua.create_function(move |_, costume: i64| { h.borrow_mut().player_set_profile_costume(costume); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetOutfit", lua.create_function(move |_, (character, outfit): (i64, i64)| { h.borrow_mut().player_set_outfit(character as u64, outfit); Ok(()) })?)?;

    // --- binding actions (real: track the player↔character binding on the host) ---
    let h = host.clone();
    b.real("AttachToCharacter", lua.create_function(move |_, (player, character): (i64, i64)| { h.borrow_mut().player_attach_to_character(player as u64, character as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("DetachFromCharacter", lua.create_function(move |_, player: i64| { h.borrow_mut().player_detach_from_character(player as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("BindToLocal", lua.create_function(move |_, player: i64| { h.borrow_mut().player_bind_local(player as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("BindToRemote", lua.create_function(move |_, player: i64| { h.borrow_mut().player_bind_remote(player as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("Unbind", lua.create_function(move |_, player: i64| { h.borrow_mut().player_unbind(player as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("CreatePlayer", lua.create_function(move |_, ()| Ok(guid_opt(h.borrow_mut().player_create())))?)?;
    let h = host.clone();
    b.real("DestroyPlayer", lua.create_function(move |_, player: i64| { h.borrow_mut().player_destroy(player as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("ClearPlayerDB", lua.create_function(move |_, ()| { h.borrow_mut().player_clear_db(); Ok(()) })?)?;

    // --- const-default getters (faithful: nothing modelled → neutral values, so Lua never hits nil) ---
    b.real("InCinematicMode", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsInWarningZone", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsBoundaryDeath", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsPositionOutBoundary", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetControlBindingType", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetCameraXZHeading", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("GetVehicleDisguiseState", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetViewportId", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("CheckSpawnPos", lua.create_function(|_, _: MultiValue| Ok(true))?)?;
    // Viewport / boundary / spawn-start handles: nothing modelled → nil (authentic `if not uX` flow).
    b.real("GetViewport", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetOutBoundary", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetPlayerStart", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetRetryPosition", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    // Handle/target getters with nothing to return → nil (the game's `if not uX` control flow is authentic).
    b.real("GetCamera", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetTargetUnderReticle", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetVehicleDisguise", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    b.real("GetSeat", lua.create_function(|_, _: MultiValue| Ok(mlua::Value::Nil))?)?;
    // Empty-list getters (iterating them is a faithful no-op).
    b.real("GetAllTargetMarkerPos", lua.create_function(|lua, _: MultiValue| lua.create_table())?)?;
    b.real("GetAllBoundaryGuid", lua.create_function(|lua, _: MultiValue| lua.create_table())?)?;

    // --- player-mode boolean gates → the real player-mode store (engine reads these). ---
    // (name, mode-key) pairs; the trailing bool arg (default true) sets the gate.
    for (name, key) in [
        ("SetCinematicMode", "cinematic_mode"),
        ("SetInputEnabled", "input_enabled"),
        ("SetSurvivalMode", "survival_mode"),
        ("SetWaitForInGame", "wait_for_ingame"),
        ("SetInPmc", "in_pmc"),
        ("SetGrappleEnabled", "grapple_enabled"),
        ("SetScopeEnabled", "scope_enabled"),
        ("SetSeatMovementLocks", "seat_movement_lock"),
        ("SetVehicleControlsLock", "vehicle_controls_lock"),
        ("SetVehicleDisguise", "vehicle_disguise"),
        ("VehicleDisguise", "vehicle_disguise"),
        ("SetPDAMapMode", "pda_map_mode"),
        ("SetSatelliteScanMode", "satellite_scan_mode"),
        ("SetSatelliteScanPaused", "satellite_scan_paused"),
    ] {
        let h = host.clone();
        let k = key;
        b.real(name, lua.create_function(move |_, on: Option<bool>| {
            h.borrow_mut().player_set_mode(k, on.unwrap_or(true));
            Ok(())
        })?)?;
    }
    // ClearGPS — a one-shot that clears the GPS route flag.
    let h = host.clone();
    b.real("ClearGPS", lua.create_function(move |_, _: MultiValue| { h.borrow_mut().player_set_mode("gps_active", false); Ok(()) })?)?;

    // Player-mode scalars → the real scalar store.
    for (name, key) in [
        ("SetHealthClamp", "health_clamp"),
        ("SetSwimmingSearchRadius", "swim_search_radius"),
        ("SetAimMode", "aim_mode"),
    ] {
        let h = host.clone();
        let k = key;
        b.real(name, lua.create_function(move |_, v: f32| { h.borrow_mut().player_set_mode_scalar(k, v); Ok(()) })?)?;
    }

    // Callbacks + PDA/satellite/boundary UI + profile-write + camera teleport + seat claim + player
    // join/leave hooks → recorded Player commands the corresponding runtime systems drain.
    super::record_all(&mut b, lua, host, "Player", &[
        "SetSurvivalModeCallback",
        "SetProfileCharacter",
        "SetProfileUpgrade",
        "SetAvailableCostumes",
        "TeleportCamera",
        "SetPDAMapModeCallback",
        "SetPDAMapModeCancelCallback",
        "RequestPDAMapModeExit",
        "RequestPDAMapModeCancel",
        "SetupSatelliteScan",
        "SetSatelliteScanCallbacks",
        "AddSatelliteScanTarget",
        "SetOutBoundary",
        "AddBoundary",
        "RemoveBoundary",
        "RemoveAllBoundary",
        "SetBoundaryCallback",
        "SetPlayerJoinedCallback",
        "SetPlayerLeftCallback",
        "RemovePlayerJoinedCallback",
        "RemovePlayerLeftCallback",
        "SetPlayerStart",
        "ClaimSeat",
        "UnClaimSeat",
    ])?;

    b.install_global(GLOBAL)
}
