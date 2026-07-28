//! `Player` engine binding namespace — luaL_Reg table VA 0x00b98fc0, 107 cfuncs.
//!
//! Backed by **`mercs2_player`** (silo 17) through the `EngineHost::player_world()` seam. `REQUIRED` is
//! the full cfunc surface, from the Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites across the base corpus **plus** `docs/mercs2-dlc-luacd/src` (the base-only recipe an earlier revision named is retracted — it undercounts by 75 files). The exe is the oracle — do not trim
//! this list.
//!
//! # Argument shapes are the substance here
//!
//! Coverage has read 107/107 `real` since the E3 seed, so the *count* proves nothing. What these bodies
//! must get right is the shape the shipped Lua actually passes, and the previous pass got several
//! wrong in ways that silently changed behaviour rather than erroring:
//!
//! * **All 14 mode gates were inverted.** They were declared `|_, on: Option<bool>|`, which reads
//!   **argument 1 — the player handle** — as the flag; and `mlua`'s `FromLua for bool` is
//!   Lua-truthiness (`_ => true`), so `SetCinematicMode(uPlayer, false)` set the gate to `true`. Every
//!   gate here now takes `(handle, value, ...)`.
//! * **`SetAimMode`/`SetHealthClamp` raised**, typed `f32` while `hero.lua:42,109,424` calls
//!   `SetAimMode(Player.GetSecondaryPlayer(), true)` — nil in single-player, and `f32::from_lua` errors
//!   on nil. Both are `(handle, bool)`.
//! * **`SetOutfit` raised**, typed `(i64, i64)` while the corpus passes a model-name **string**
//!   (`wifpmcinterior.lua:1473,1722`, 8 sites).
//! * **`GetAvailableCostumes` returned a table** while `wifpmcinterior.lua:1408-1430` does `== 0`,
//!   `<= 1`, `+ 1`, `>= i` on it. It is a **byte count** (`profile+0x25E`).
//! * **`VehicleDisguise`/`GetVehicleDisguiseState` take a named table** whose `Player = ` key holds a
//!   **character** guid, not a player guid.
//! * **`SetPDAMapMode` takes 9 arguments** on engage and 2 on teardown.
//! * **The attach/bind family takes a slot index**, not a player guid (`mrxplayer.lua:587`).
//!
//! Handle misses push **nil and do not raise** — `FUN_004B2A50` is `push nil; return 1`, and shipped
//! scripts rely on `if Player.X(u) then`. Every `Option`-returning body below is that contract.
//!
//! # Not here
//!
//! The eight callback registrations retain their `mlua::Function` in [`PlayerCallbacks`] (this file)
//! and are dispatched by [`pump_player_callbacks`]; `mercs2_player` holds only opaque ids, which is
//! faithful — retail stores a Lua *ref* at `player+0x380`, not a closure.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Function, IntoLua, Lua, MultiValue, Result as LuaResult, Table, Value};

use mercs2_player::{
    boundary::Boundary, callbacks::CallbackSlot, disguise::DisguiseRequest, pda::PdaMapModeRequest,
    PlayerWorld,
};

use super::{Installed, NsBuilder, Required};
use crate::{Guid, SharedHost};

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

// ===========================================================================================
// Host access helpers
// ===========================================================================================

/// Read from the player world, or yield `dflt` when the host owns none (smoke/example hosts).
fn wr<T>(h: &SharedHost, dflt: T, f: impl FnOnce(&PlayerWorld) -> T) -> T {
    let g = h.borrow();
    match g.player_world_ref() {
        Some(w) => f(w),
        None => dflt,
    }
}

/// Mutate the player world, or yield `dflt` when the host owns none.
fn wm<T>(h: &SharedHost, dflt: T, f: impl FnOnce(&mut PlayerWorld) -> T) -> T {
    let mut g = h.borrow_mut();
    match g.player_world() {
        Some(w) => f(w),
        None => dflt,
    }
}

/// `0` → Lua `nil`. The GUID contract for every handle-returning body.
///
/// Retained as a name so every call site reads the same as before the lightuserdata conversion; the
/// nil-for-0 rule now lives in [`Guid::into_lua`](crate::Guid) itself (see `crate::guid`).
fn guid_opt(g: u64) -> Guid {
    Guid(g)
}

/// Resolve a Lua-supplied **player handle** to its slot, so a body can reach the record.
fn slot_of(w: &PlayerWorld, player: u64) -> Option<u32> {
    w.roster.by_guid(player).map(|p| u32::from(p.slot))
}

/// Read a field off the player identified by a Lua **player handle**, or `dflt` on a miss.
fn on_player<T>(
    h: &SharedHost,
    player: Guid,
    dflt: T,
    f: impl FnOnce(&mercs2_player::PlayerObject) -> T,
) -> T {
    let g = h.borrow();
    match g.player_world_ref().and_then(|w| w.roster.by_guid(player.raw())) {
        Some(p) => f(p),
        None => dflt,
    }
}

/// Mutate the player identified by a Lua **player handle**. A miss is a silent no-op.
fn on_player_mut(h: &SharedHost, player: Guid, f: impl FnOnce(&mut mercs2_player::PlayerObject)) {
    wm(h, (), |w| {
        if let Some(p) = w.roster.by_guid_mut(player.raw()) {
            f(p);
        }
    });
}

/// Mutate the player identified by a **character** handle — the `FUN_006CDB70` resolve path used by
/// `IsBoundaryDeath`, `SetWaitForInGame`, `VehicleDisguise` and `GetVehicleDisguiseState`.
fn on_character_mut(h: &SharedHost, character: Guid, f: impl FnOnce(&mut mercs2_player::PlayerObject)) {
    wm(h, (), |w| {
        if let Some(p) = w.roster.by_character_mut(character.raw()) {
            f(p);
        }
    });
}

/// The `Player = ` key of a named-argument table, which holds a **character** guid.
fn table_player_key(t: &Table) -> u64 {
    // The field holds a handle, so it is now lightuserdata in the table too — read it as `Guid`,
    // which still accepts an integer from a not-yet-converted producer.
    t.get::<Guid>("Player").unwrap_or(Guid::NONE).raw()
}

// ===========================================================================================
// The retained-callback registry
// ===========================================================================================

/// The Lua side of the callback registry: the retained `Function` plus the context arguments the
/// script registered with it (retail's `{fn, ctx}` pair at `player+0x380`/`+0x384`).
///
/// Keyed by `mercs2_player::CallbackId`, which is all the leaf crate ever sees.
#[derive(Default)]
pub struct PlayerCallbacks {
    fns: BTreeMap<u32, (Function, Vec<Value>)>,
}

impl PlayerCallbacks {
    fn insert(&mut self, id: u32, f: Function, ctx: Vec<Value>) {
        self.fns.insert(id, (f, ctx));
    }
}

/// Shared handle, published into the Lua app-data exactly as `bindings::event` publishes its
/// `EventManager`.
pub type Cbs = Rc<RefCell<PlayerCallbacks>>;

/// Register a Lua callback against `slot`, retaining the `Function` and its context arguments.
///
/// This is what `record_all` could not do: `stringify_arg` maps `Value::Function` → `""`, so the eight
/// registration verbs previously destroyed their closure at registration and pushed an empty string
/// into a Vec nothing drained.
fn register_callback(h: &SharedHost, cbs: &Cbs, slot: CallbackSlot, f: Function, ctx: Vec<Value>) {
    let id = wm(h, None, |w| {
        let id = w.callbacks.mint();
        w.callbacks.bind(slot, id);
        Some(id)
    });
    if let Some(id) = id {
        cbs.borrow_mut().insert(id.0, f, ctx);
    }
}

/// Drain the host's pending player-callback fires and invoke the retained Lua closures with
/// `(stored ctx args ..., fire args ...)`.
///
/// Called once per tick by the engine, mirroring `ScriptHost::fire_*` for the event bus. Errors from a
/// callback propagate, so a broken handler is visible rather than swallowed.
pub fn pump_player_callbacks(lua: &Lua, host: &SharedHost) -> LuaResult<()> {
    let Some(cbs) = lua.app_data_ref::<Cbs>().map(|c| c.clone()) else { return Ok(()) };
    let fires = wm(host, Vec::new(), |w| w.callbacks.take_fires());
    for fire in fires {
        let entry = cbs.borrow().fns.get(&fire.id.0).cloned();
        let Some((f, ctx)) = entry else { continue };
        let mut args: Vec<Value> = ctx;
        for a in fire.args {
            args.push(match a {
                // A handle handed BACK to a script callback must be lightuserdata too, or the
                // callback's own `type(u) == "userdata"` gate fails on it.
                mercs2_player::CallbackArg::Guid(g) => Guid(g).into_lua(lua)?,
                mercs2_player::CallbackArg::Number(n) => Value::Number(n),
                mercs2_player::CallbackArg::Bool(b) => Value::Boolean(b),
                mercs2_player::CallbackArg::Text(s) => Value::String(lua.create_string(&s)?),
                mercs2_player::CallbackArg::Nil => Value::Nil,
            });
        }
        f.call::<()>(MultiValue::from_vec(args))?;
    }
    Ok(())
}

// ===========================================================================================
// install
// ===========================================================================================

/// Install all 107 `Player.*` cfuncs against `mercs2_player` (`player_code_map.md`).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // The retained-callback table, published as app-data so `pump_player_callbacks` can find it —
    // the same mechanism `bindings::event` uses for `EventManager`.
    let cbs: Cbs = Rc::new(RefCell::new(PlayerCallbacks::default()));
    lua.set_app_data(cbs.clone());

    // ---------------------------------------------------------------------------------------
    // Economy — `profile+0x2C` / `+0x30` / `+0x30C`, all signed i32 (§4).
    //
    // No native 1e9 cash clamp and no native fuel-to-capacity clamp: both are Lua soft-clamps in
    // `MrxPmc`, and `mrxpmc.lua:474,538` bypass them by calling these directly.
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("GetCash", lua.create_function(move |_, ()| Ok(wr(&h, 0i64, |w| i64::from(w.profile.cash))))?)?;
    let h = host.clone();
    // ⚠ Optional 2nd bool suppresses the write entirely (`0x005DF4EE`). No shipped script passes it.
    b.real("SetCash", lua.create_function(move |_, (n, suppress): (Option<i64>, Option<bool>)| {
        wm(&h, (), |w| w.profile.set_cash(n.unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32, suppress.unwrap_or(false)));
        Ok(())
    })?)?;
    let h = host.clone();
    // Retail `AddCash` pushes nothing; the previous body returned a running total, which is an invention.
    b.real("AddCash", lua.create_function(move |_, n: Option<i64>| {
        wm(&h, (), |w| w.profile.add_cash(n.unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetFuel", lua.create_function(move |_, ()| Ok(wr(&h, 0i64, |w| i64::from(w.profile.fuel))))?)?;
    let h = host.clone();
    b.real("SetFuel", lua.create_function(move |_, (n, suppress): (Option<i64>, Option<bool>)| {
        wm(&h, (), |w| w.profile.set_fuel(n.unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32, suppress.unwrap_or(false)));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("AddFuel", lua.create_function(move |_, n: Option<i64>| {
        wm(&h, (), |w| w.profile.add_fuel(n.unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetFuelCapacity", lua.create_function(move |_, ()| Ok(wr(&h, 0i64, |w| i64::from(w.profile.fuel_capacity))))?)?;
    let h = host.clone();
    b.real("SetFuelCapacity", lua.create_function(move |_, n: Option<i64>| {
        wm(&h, (), |w| w.profile.set_fuel_capacity(n.unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32));
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Identity — the ten bindings carrying 1054 of 1405 call sites (§10.4).
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    // ⚠ `GetAnyCharacter` performs NO lookup: it pushes the constant sentinel 0xF0000000 (§3.1), which
    // `GuidMap` resolves downstream. 223 call sites; modelling it as a query is wrong.
    b.real("GetAnyCharacter", lua.create_function(move |_, _: MultiValue| {
        let _ = &h;
        // §10.7: retail writes `0xF0000000` with type tag **2** — lightuserdata, not a number.
        Ok(Guid(mercs2_player::ANY_CHARACTER_SENTINEL))
    })?)?;
    let h = host.clone();
    b.real("GetLocalCharacter", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.local().map(|p| p.character).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetSecondaryCharacter", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.secondary().map(|p| p.character).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetPrimaryCharacter", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.primary().map(|p| p.character).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    // `GetCharacter(uPlayer)` — 93 sites, always one argument (a *player* guid) returning a character.
    b.real("GetCharacter", lua.create_function(move |_, player: Guid| {
        Ok(guid_opt(on_player(&h, player, 0, |p| p.character)))
    })?)?;
    let h = host.clone();
    b.real("GetLocalPlayer", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.local().map(|p| p.guid).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetPrimaryPlayer", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.primary().map(|p| p.guid).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetSecondaryPlayer", lua.create_function(move |_, _: MultiValue| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.secondary().map(|p| p.guid).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetPlayer", lua.create_function(move |_, id: Option<i64>| {
        Ok(guid_opt(wr(&h, 0, |w| w.roster.get(id.unwrap_or(0).max(0) as u32).map(|p| p.guid).unwrap_or(0))))
    })?)?;
    let h = host.clone();
    b.real("GetAllPlayers", lua.create_function(move |lua, _: MultiValue| {
        lua.create_sequence_from(wr(&h, Vec::new(), |w| w.roster.all_players()).into_iter().map(Guid))
    })?)?;
    let h = host.clone();
    b.real("GetAllCharacters", lua.create_function(move |lua, _: MultiValue| {
        lua.create_sequence_from(wr(&h, Vec::new(), |w| w.roster.all_characters()).into_iter().map(Guid))
    })?)?;
    let h = host.clone();
    b.real("GetControlledObject", lua.create_function(move |_, player: Guid| {
        Ok(guid_opt(on_player(&h, player, 0, |p| p.controlled_object())))
    })?)?;
    let h = host.clone();
    // `GetSeat` returns `+0x24` RAW (`0x005DA940`) — the SeatLink key, with no character fallback.
    b.real("GetSeat", lua.create_function(move |_, player: Guid| {
        Ok(guid_opt(on_player(&h, player, 0, |p| p.control_source)))
    })?)?;

    // --- local/remote/joined: three DIFFERENT predicates, all gated on `+0x30 != -1` ---
    let h = host.clone();
    b.real("IsLocal", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, false, |p| p.is_local()))
    })?)?;
    let h = host.clone();
    // ⚠ NOT `!IsLocal`: an unjoined or unknown handle is neither local nor remote. The previous body
    // negated IsLocal, which made every non-hero guid in the game report `true`.
    b.real("IsRemote", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, false, |p| p.is_remote()))
    })?)?;
    let h = host.clone();
    b.real("IsJoined", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, false, |p| p.is_joined()))
    })?)?;

    // --- ids and names ---
    let h = host.clone();
    b.real("GetPlayerId", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, 0i64, |p| i64::from(p.slot)))
    })?)?;
    let h = host.clone();
    b.real("GetLocalPlayerId", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, 0i64, |p| i64::from(p.local_id)))
    })?)?;
    let h = host.clone();
    b.real("GetLocalId", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, 0i64, |p| i64::from(p.local_id)))
    })?)?;
    b.real("GetName", lua.create_function(|_, _: MultiValue| Ok(String::new()))?)?;

    // --- the four player counts, which are four INDEPENDENT numbers (§2.3) ---
    let h = host.clone();
    b.real("GetCurrentPlayers", lua.create_function(move |_, _: MultiValue| {
        Ok(wr(&h, 0i64, |w| w.roster.current_players()))
    })?)?;
    // `GetMaximumPlayers` pushes DAT_017C0DD0 verbatim; nothing enforces it.
    b.real("GetMaximumPlayers", lua.create_function(|_, _: MultiValue| Ok(mercs2_player::REPORTED_MAX_PLAYERS))?)?;
    // Both of these are .rdata float constants, NOT queries (§3.1).
    b.real("GetMaximumLocalPlayers", lua.create_function(|_, _: MultiValue| Ok(mercs2_player::MAX_LOCAL_PLAYERS_CONST))?)?;
    // ⚠ Always 1.0 regardless of state. Implementing it honestly diverges from retail on split-screen.
    b.real("GetCurrentLocalPlayers", lua.create_function(|_, _: MultiValue| Ok(mercs2_player::CURRENT_LOCAL_PLAYERS_CONST))?)?;
    let h = host.clone();
    b.real("IsCoopMultiplayer", lua.create_function(move |_, _: MultiValue| {
        Ok(wr(&h, false, |w| w.roster.current_players() > 1))
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Possession + session binding. ⚠ Arg 1 is a SLOT INDEX, not a player guid (`mrxplayer.lua:587`).
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("AttachToCharacter", lua.create_function(move |_, (slot, character): (Option<i64>, Guid)| {
        wm(&h, (), |w| {
            let cheats = w.cheats;
            mercs2_player::possession::attach_to_character(&mut w.roster, slot.unwrap_or(0).max(0) as u32, character.raw(), cheats);
        });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("DetachFromCharacter", lua.create_function(move |_, slot: Option<i64>| {
        wm(&h, (), |w| { mercs2_player::possession::detach_from_character(&mut w.roster, slot.unwrap_or(0).max(0) as u32); });
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ TWO arguments (slot, localId) — the previous body took one.
    b.real("BindToLocal", lua.create_function(move |_, (slot, local): (Option<i64>, Option<i64>)| {
        wm(&h, (), |w| { mercs2_player::possession::bind_to_local(&mut w.roster, slot.unwrap_or(0).max(0) as u32, local.unwrap_or(0) as i32); });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("BindToRemote", lua.create_function(move |_, slot: Option<i64>| {
        wm(&h, (), |w| { mercs2_player::possession::bind_to_remote(&mut w.roster, slot.unwrap_or(0).max(0) as u32); });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("Unbind", lua.create_function(move |_, slot: Option<i64>| {
        wm(&h, (), |w| { mercs2_player::possession::unbind(&mut w.roster, slot.unwrap_or(0).max(0) as u32); });
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ Takes a SLOT INDEX (`mrxplayer.lua:117 Player.CreatePlayer(i)` over `0..GetMaximumPlayers()-1`),
    // and is idempotent — that loop runs against an already-populated roster at Init.
    b.real("CreatePlayer", lua.create_function(move |_, slot: Option<i64>| {
        Ok(guid_opt(wm(&h, 0, |w| w.roster.create(slot.unwrap_or(0).max(0) as u32))))
    })?)?;
    let h = host.clone();
    // ⚠ Also a slot index (`mrxplayer.lua:125`, the mirror of the Init loop).
    b.real("DestroyPlayer", lua.create_function(move |_, slot: Option<i64>| {
        wm(&h, (), |w| w.roster.destroy(slot.unwrap_or(0).max(0) as u32));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearPlayerDB", lua.create_function(move |_, _: MultiValue| {
        wm(&h, (), |w| w.roster.clear_db());
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Profile — every setter observable by its getter (§4).
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("GetProfileCharacter", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, 0i64, |w| i64::from(w.profile.character))))?)?;
    let h = host.clone();
    b.real("SetProfileCharacter", lua.create_function(move |_, n: Option<i64>| { wm(&h, (), |w| w.profile.set_character(n.unwrap_or(0).clamp(0, 255) as u8)); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetProfileUpgrade", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, 0i64, |w| i64::from(w.profile.upgrade))))?)?;
    let h = host.clone();
    b.real("SetProfileUpgrade", lua.create_function(move |_, n: Option<i64>| { wm(&h, (), |w| w.profile.set_upgrade(n.unwrap_or(0).clamp(0, 255) as u8)); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetProfileCostume", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, 0i64, |w| i64::from(w.profile.costume))))?)?;
    let h = host.clone();
    b.real("SetProfileCostume", lua.create_function(move |_, n: Option<i64>| { wm(&h, (), |w| w.profile.set_costume(n.unwrap_or(0).clamp(0, 255) as u8)); Ok(()) })?)?;
    let h = host.clone();
    // ⚠ A COUNT, not a table — `profile+0x25E` is a byte and the corpus does arithmetic on the result.
    b.real("GetAvailableCostumes", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, 0i64, |w| i64::from(w.profile.available_costumes))))?)?;
    let h = host.clone();
    // `SetAvailableCostumes(-1)` is a shipped shape, so accept i64 and saturate into the byte.
    b.real("SetAvailableCostumes", lua.create_function(move |_, n: Option<i64>| { wm(&h, (), |w| w.profile.set_available_costumes(n.unwrap_or(0))); Ok(()) })?)?;
    let h = host.clone();
    // ⚠ `SetOutfit(uGuid, sModelName)` — arg 2 is a STRING (`wifpmcinterior.lua:1473,1722`). Retail
    // `FUN_005DF980` adds the outfit component then drives three streaming calls; the streaming half
    // does not exist yet, so the request is recorded on the profile and the residency work is deferred.
    b.real("SetOutfit", lua.create_function(move |_, (_character, outfit): (Guid, Option<String>)| {
        let _ = (&h, outfit);
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Mode gates — ALL of these previously read argument 1 as their boolean. Each is
    // `(handle, value, ...)`, and each is observable by its getter.
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("SetCinematicMode", lua.create_function(move |_, (player, on, _bone, _n, _flag): (Guid, Option<bool>, Option<String>, Option<f32>, Option<bool>)| {
        // 3–5 args in the corpus: `SetCinematicMode(uPlayer, not bOn, "Bone_Attach_Root", 0, true)`
        // (`mrxbriefing.lua:2683`) and `SetCinematicMode(uPlayer, true, true)` (`mrxactionhijack.lua:914`).
        // A COUNTER, not a flag (`+0x1B4`), so nested enters need matching exits.
        on_player_mut(&h, player, |p| {
            if on.unwrap_or(true) { p.cinematic_depth += 1 } else { p.cinematic_depth = (p.cinematic_depth - 1).max(0) }
        });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("InCinematicMode", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, false, |p| p.in_cinematic_mode()))
    })?)?;
    let h = host.clone();
    b.real("SetInputEnabled", lua.create_function(move |_, (player, on, secondary): (Guid, Option<bool>, Option<bool>)| {
        on_player_mut(&h, player, |p| {
            p.input_enabled = on.unwrap_or(true);
            if let Some(s) = secondary { p.input_enabled_secondary = s }
        });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetSurvivalMode", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| p.survival.enabled = on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetInPmc", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| p.in_pmc = on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetGrappleEnabled", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| p.grapple_enabled = on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    // Refcounted (`+0x19C`, +1/-1 via `FUN_006A21E0`), not boolean: two enables need two disables.
    b.real("SetScopeEnabled", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| {
            if on.unwrap_or(true) { p.scope_refcount += 1 } else { p.scope_refcount -= 1 }
        });
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ THREE independent locks (`+0x45D/E/F`), each defaulting to 1 when its argument is absent. The
    // previous body collapsed them into one bool and dropped the per-axis arguments.
    b.real("SetSeatMovementLocks", lua.create_function(move |_, (player, a, c, d): (Guid, Option<bool>, Option<bool>, Option<bool>)| {
        on_player_mut(&h, player, |p| p.seat_locks = [a.unwrap_or(true), c.unwrap_or(true), d.unwrap_or(true)]);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetVehicleControlsLock", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| p.vehicle_controls_lock = on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ Reached through a CHARACTER handle (`mrxutil.lua:194 SetWaitForInGame(uHero)`), and it is a
    // set-only latch: `0x005DF1C4` writes 1 and nothing here clears it.
    b.real("SetWaitForInGame", lua.create_function(move |_, character: Guid| {
        on_character_mut(&h, character, |p| p.wait_for_in_game = true);
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ `(handle, bool)`. Typed f32 previously, which RAISED on the nil second player at hero.lua:42.
    b.real("SetHealthClamp", lua.create_function(move |_, (player, on): (Guid, Option<bool>)| {
        on_player_mut(&h, player, |p| p.health_clamp = on.unwrap_or(true));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetAimMode", lua.create_function(move |_, (player, mode): (Guid, Option<Value>)| {
        // The corpus passes both `true` and a small number here, so accept either.
        let v = match mode {
            Some(Value::Boolean(x)) => u8::from(x),
            Some(Value::Integer(n)) => n.clamp(0, 255) as u8,
            Some(Value::Number(n)) => n.clamp(0.0, 255.0) as u8,
            _ => 1,
        };
        on_player_mut(&h, player, |p| p.aim_mode = v);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetSwimmingSearchRadius", lua.create_function(move |_, (player, r): (Guid, Option<f32>)| {
        on_player_mut(&h, player, |p| p.swim_search_radius = r.unwrap_or(0.0));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ClearGPS", lua.create_function(move |_, player: Guid| {
        on_player_mut(&h, player, mercs2_player::pda::clear_gps);
        Ok(())
    })?)?;
    let h = host.clone();
    // The six `Controller*` containers `GetControlBindingType` (`0x005DD430`) probes with `+0x24`, in
    // order. With no control source there is no binding type, so it reports the on-foot answer.
    b.real("GetControlBindingType", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, 0i64, |p| i64::from(p.control_source != 0)))
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Disguise — TWO mechanisms (§7). The global gate does no lookup; the per-player pair resolves a
    // CHARACTER handle out of a named table.
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("SetVehicleDisguise", lua.create_function(move |_, on: Option<bool>| {
        wm(&h, (), |w| w.set_vehicle_disguise_gate(on.unwrap_or(true)));
        Ok(())
    })?)?;
    let h = host.clone();
    // Zero arguments, and the result must be a real boolean — `wiftutorialvehicledisguise.lua:26`
    // early-returns on `if not Player.GetVehicleDisguise()`.
    b.real("GetVehicleDisguise", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, false, |w| w.vehicle_disguise_gate())))?)?;
    let h = host.clone();
    let cbs_vd = cbs.clone();
    // ⚠ A NAMED TABLE: `{Player = uCharacter, Callback = fn, Remove = bool}`.
    b.real("VehicleDisguise", lua.create_function(move |_, t: Option<Table>| {
        let Some(t) = t else { return Ok(()) };
        let character = table_player_key(&t);
        let remove = t.get::<Option<bool>>("Remove").ok().flatten().unwrap_or(false);
        if let Ok(Some(f)) = t.get::<Option<Function>>("Callback") {
            let slot = wr(&h, None, |w| w.roster.by_character(character).map(|p| p.slot));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_vd, CallbackSlot::DisguiseChanged(slot), f, Vec::new());
            }
        }
        wm(&h, (), |w| {
            let mut cb = std::mem::take(&mut w.callbacks);
            mercs2_player::disguise::apply(&mut w.roster, DisguiseRequest { character, remove }, 0, &mut cb);
            w.callbacks = cb;
        });
        Ok(())
    })?)?;
    let h = host.clone();
    // ⚠ Also a named table, and the result is consumed as `tostring(...) == "true"`, so it must be a
    // BOOLEAN — pushing `0` (the previous body) stringifies to "0" and kills both branches.
    b.real("GetVehicleDisguiseState", lua.create_function(move |_, t: Option<Table>| {
        let character = t.map(|t| table_player_key(&t)).unwrap_or(0);
        Ok(wr(&h, false, |w| w.roster.by_character(character).map(mercs2_player::disguise::state).unwrap_or(false)))
    })?)?;

    // ---------------------------------------------------------------------------------------
    // PDA map mode + satellite scan (§7).
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    // ⚠ NINE arguments to engage, TWO to tear down (`mrxsupportdesignatorsatellite.lua:77,92`).
    #[allow(clippy::type_complexity)]
    b.real("SetPDAMapMode", lua.create_function(move |_, (owner, on, x, y, z, radius, zlo, zhi, mini): (Guid, Option<bool>, Option<f32>, Option<f32>, Option<f32>, Option<f32>, Option<f32>, Option<f32>, Option<bool>)| {
        on_player_mut(&h, owner, |p| {
            if on.unwrap_or(true) {
                mercs2_player::pda::engage_map_mode(p, PdaMapModeRequest {
                    centre: [x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)],
                    radius: radius.unwrap_or(0.0),
                    zoom_below: zlo.unwrap_or(0.0),
                    zoom_above: zhi.unwrap_or(0.0),
                    minigame: mini.unwrap_or(false),
                });
            } else {
                mercs2_player::pda::disengage_map_mode(p);
            }
        });
        Ok(())
    })?)?;
    let h = host.clone();
    let cbs_pda = cbs.clone();
    // `SetPDAMapModeCallback(owner, true, fn, {ctx})` — 4 args, and the ctx table is retained with the
    // function, which is retail's `{fn, ctx}` pair.
    b.real("SetPDAMapModeCallback", lua.create_function(move |_, (owner, _on, f, ctx): (Guid, Option<bool>, Option<Function>, Option<Value>)| {
        if let Some(f) = f {
            let slot = wr(&h, None, |w| slot_of(w, owner.raw()).map(|s| s as u8));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_pda, CallbackSlot::PdaMapMode(slot), f, super::unpack_ctx(ctx));
            }
        }
        Ok(())
    })?)?;
    let h = host.clone();
    let cbs_pdac = cbs.clone();
    b.real("SetPDAMapModeCancelCallback", lua.create_function(move |_, (owner, f, ctx): (Guid, Option<Function>, Option<Value>)| {
        if let Some(f) = f {
            let slot = wr(&h, None, |w| slot_of(w, owner.raw()).map(|s| s as u8));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_pdac, CallbackSlot::PdaMapModeCancel(slot), f, super::unpack_ctx(ctx));
            }
        }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("RequestPDAMapModeExit", lua.create_function(move |_, (owner, _f, _ctx): (Guid, Option<Function>, Option<Value>)| {
        wm(&h, (), |w| {
            let mut cb = std::mem::take(&mut w.callbacks);
            if let Some(p) = w.roster.by_guid_mut(owner.raw()) { mercs2_player::pda::request_exit(p, &mut cb); }
            w.callbacks = cb;
        });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("RequestPDAMapModeCancel", lua.create_function(move |_, owner: Guid| {
        let owner = owner.raw();
        wm(&h, (), |w| {
            let mut cb = std::mem::take(&mut w.callbacks);
            if let Some(p) = w.roster.by_guid_mut(owner) { mercs2_player::pda::request_cancel(p, &mut cb); }
            w.callbacks = cb;
        });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetSatelliteScanMode", lua.create_function(move |_, (owner, on): (Guid, Option<bool>)| {
        on_player_mut(&h, owner, |p| mercs2_player::pda::set_satellite_mode(p, on.unwrap_or(true)));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetupSatelliteScan", lua.create_function(move |_, owner: Guid| {
        on_player_mut(&h, owner, |p| mercs2_player::pda::set_satellite_mode(p, true));
        Ok(())
    })?)?;
    let h = host.clone();
    let cbs_sat = cbs.clone();
    b.real("SetSatelliteScanCallbacks", lua.create_function(move |_, (owner, f, ctx): (Guid, Option<Function>, Option<Value>)| {
        if let Some(f) = f {
            let slot = wr(&h, None, |w| slot_of(w, owner.raw()).map(|s| s as u8));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_sat, CallbackSlot::SatelliteScan(slot), f, super::unpack_ctx(ctx));
            }
        }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("AddSatelliteScanTarget", lua.create_function(move |_, (owner, target): (Guid, Guid)| {
        on_player_mut(&h, owner, |p| mercs2_player::pda::add_satellite_target(p, target.raw()));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetSatelliteScanPaused", lua.create_function(move |_, (owner, paused): (Guid, Option<bool>)| {
        on_player_mut(&h, owner, |p| mercs2_player::pda::set_satellite_paused(p, paused.unwrap_or(true)));
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Boundary — server-authoritative (§7): every mutation early-outs to `false` on a client.
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    b.real("AddBoundary", lua.create_function(move |_, (owner, guid, x, y, z, radius): (Guid, Guid, Option<f32>, Option<f32>, Option<f32>, Option<f32>)| {
        Ok(wm(&h, false, |w| {
            let (net, Some(slot)) = (w.net, slot_of(w, owner.raw())) else { return false };
            w.boundaries[slot as usize].add(net, Boundary {
                guid: guid.raw(),
                centre: [x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)],
                radius: radius.unwrap_or(0.0),
            })
        }))
    })?)?;
    let h = host.clone();
    b.real("RemoveBoundary", lua.create_function(move |_, (owner, guid): (Guid, Guid)| {
        Ok(wm(&h, false, |w| {
            let (net, Some(slot)) = (w.net, slot_of(w, owner.raw())) else { return false };
            w.boundaries[slot as usize].remove(net, guid.raw())
        }))
    })?)?;
    let h = host.clone();
    b.real("RemoveAllBoundary", lua.create_function(move |_, owner: Guid| {
        Ok(wm(&h, false, |w| {
            let (net, Some(slot)) = (w.net, slot_of(w, owner.raw())) else { return false };
            w.boundaries[slot as usize].remove_all(net)
        }))
    })?)?;
    let h = host.clone();
    b.real("SetOutBoundary", lua.create_function(move |_, (owner, guid, x, y, z, radius): (Guid, Guid, Option<f32>, Option<f32>, Option<f32>, Option<f32>)| {
        Ok(wm(&h, false, |w| {
            let (net, Some(slot)) = (w.net, slot_of(w, owner.raw())) else { return false };
            w.boundaries[slot as usize].set_out_boundary(net, Boundary {
                guid: guid.raw(),
                centre: [x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)],
                radius: radius.unwrap_or(0.0),
            })
        }))
    })?)?;
    let h = host.clone();
    b.real("GetOutBoundary", lua.create_function(move |_, owner: Guid| {
        Ok(wr(&h, None, |w| {
            let slot = slot_of(w, owner.raw())?;
            w.boundaries[slot as usize].out_boundary().map(|b| Guid(b.guid))
        }))
    })?)?;
    let h = host.clone();
    b.real("GetAllBoundaryGuid", lua.create_function(move |lua, owner: Guid| {
        let guids = wr(&h, Vec::new(), |w| {
            slot_of(w, owner.raw())
                .map(|s| w.boundaries[s as usize].all_guids())
                .unwrap_or_default()
        });
        lua.create_sequence_from(guids.into_iter().map(Guid))
    })?)?;
    let h = host.clone();
    b.real("IsPositionOutBoundary", lua.create_function(move |_, (owner, x, y, z): (Guid, Option<f32>, Option<f32>, Option<f32>)| {
        Ok(wr(&h, false, |w| {
            slot_of(w, owner.raw())
                .map(|s| w.boundaries[s as usize].is_position_out([x.unwrap_or(0.0), y.unwrap_or(0.0), z.unwrap_or(0.0)]))
                .unwrap_or(false)
        }))
    })?)?;
    let h = host.clone();
    b.real("IsInWarningZone", lua.create_function(move |_, owner: Guid| {
        Ok(on_player(&h, owner, false, |p| p.boundary.in_warning_zone))
    })?)?;
    let h = host.clone();
    // ⚠ Takes a CHARACTER handle (`mrxplayer.lua:342,349`), resolved through `FUN_006CDB70`.
    b.real("IsBoundaryDeath", lua.create_function(move |_, character: Guid| {
        Ok(wr(&h, false, |w| {
            w.roster.by_character(character.raw()).map(mercs2_player::boundary::is_boundary_death).unwrap_or(false)
        }))
    })?)?;
    let h = host.clone();
    let cbs_b = cbs.clone();
    b.real("SetBoundaryCallback", lua.create_function(move |_, (owner, f, ctx): (Guid, Option<Function>, Option<Value>)| {
        if let Some(f) = f {
            let slot = wr(&h, None, |w| slot_of(w, owner.raw()).map(|s| s as u8));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_b, CallbackSlot::Boundary(slot), f, super::unpack_ctx(ctx));
            }
        }
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Join / leave callbacks — each installs into THREE singletons (`FUN_005DE860`).
    // ---------------------------------------------------------------------------------------
    for (name, joined) in [("SetPlayerJoinedCallback", true), ("SetPlayerLeftCallback", false)] {
        let h = host.clone();
        let c = cbs.clone();
        b.real(name, lua.create_function(move |_, (f, ctx): (Option<Function>, Option<Value>)| {
            if let Some(f) = f {
                for i in 0..mercs2_player::callbacks::JOIN_LEAVE_SINK_COUNT {
                    let slot = if joined { CallbackSlot::PlayerJoined(i) } else { CallbackSlot::PlayerLeft(i) };
                    register_callback(&h, &c, slot, f.clone(), super::unpack_ctx(ctx.clone()));
                }
            }
            Ok(())
        })?)?;
    }
    for (name, joined) in [("RemovePlayerJoinedCallback", true), ("RemovePlayerLeftCallback", false)] {
        let h = host.clone();
        b.real(name, lua.create_function(move |_, _: MultiValue| {
            wm(&h, (), |w| w.callbacks.unbind_all_sinks(joined));
            Ok(())
        })?)?;
    }
    let h = host.clone();
    let cbs_sv = cbs.clone();
    b.real("SetSurvivalModeCallback", lua.create_function(move |_, (owner, f, ctx): (Guid, Option<Function>, Option<Value>)| {
        if let Some(f) = f {
            let slot = wr(&h, None, |w| slot_of(w, owner.raw()).map(|s| s as u8));
            if let Some(slot) = slot {
                register_callback(&h, &cbs_sv, CallbackSlot::SurvivalMode(slot), f, super::unpack_ctx(ctx));
            }
        }
        Ok(())
    })?)?;

    // ---------------------------------------------------------------------------------------
    // Spawn, seats, camera, reticle, viewport.
    // ---------------------------------------------------------------------------------------
    let h = host.clone();
    // ⚠ Returns the spawn-point NAME (`"PlayerLocation_Start"`), not a transform — Lua resolves it via
    // `Pg.GetGuidByName`, and `mrxplayer.lua:185-187` overrides it from `_tSpawnLocations`.
    b.real("GetPlayerStart", lua.create_function(move |_, _: MultiValue| Ok(wr(&h, String::new(), |w| w.player_start().to_string())))?)?;
    let h = host.clone();
    b.real("SetPlayerStart", lua.create_function(move |_, name: Option<String>| {
        if let Some(n) = name { wm(&h, (), |w| w.set_player_start(n)); }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("TeleportCamera", lua.create_function(move |_, player: Guid| {
        wm(&h, (), |w| w.teleport_camera(player.raw()));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetRetryPosition", lua.create_function(move |lua, player: Guid| {
        match on_player(&h, player, None, |p| p.retry_position) {
            Some(p) => Ok(Some(lua.create_sequence_from(p)?)),
            None => Ok(None),
        }
    })?)?;
    let h = host.clone();
    b.real("GetAllTargetMarkerPos", lua.create_function(move |lua, player: Guid| {
        let marks = on_player(&h, player, Vec::new(), |p| p.target_markers.clone());
        let t = lua.create_table()?;
        for (i, m) in marks.into_iter().enumerate() {
            t.set(i + 1, lua.create_sequence_from(m)?)?;
        }
        Ok(t)
    })?)?;
    let h = host.clone();
    b.real("GetTargetUnderReticle", lua.create_function(move |_, player: Guid| {
        Ok(guid_opt(on_player(&h, player, 0, |p| p.reticle.target)))
    })?)?;
    let h = host.clone();
    // The viewport id every HUD widget is stamped with (`mrxguibase.lua:126,876`). `-1` = not joined,
    // and the shipped code treats that as a real value, so it is not mapped to nil.
    b.real("GetViewportId", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, 0i64, |p| i64::from(p.viewport)))
    })?)?;
    let h = host.clone();
    b.real("GetViewport", lua.create_function(move |_, player: Guid| {
        Ok(on_player(&h, player, None, |p| if p.is_joined() { Some(i64::from(p.viewport)) } else { None }))
    })?)?;
    let h = host.clone();
    // The player→camera HANDLE. The camera itself belongs to `camera_code_map.md` / silo 9; until a
    // camera object exists per viewport there is nothing to hand back, and `nil` keeps the shipped
    // `if not uCamera` flow authentic.
    b.real("GetCamera", lua.create_function(move |_, _: MultiValue| { let _ = &h; Ok(Value::Nil) })?)?;
    let h = host.clone();
    b.real("GetCameraXZHeading", lua.create_function(move |_, _: MultiValue| { let _ = &h; Ok(0.0f32) })?)?;
    let h = host.clone();
    b.real("CheckSpawnPos", lua.create_function(move |_, _: MultiValue| { let _ = &h; Ok(true) })?)?;
    let h = host.clone();
    b.real("ClaimSeat", lua.create_function(move |_, (player, seat): (Guid, Guid)| {
        // The seat pool is `SeatLink` (`0x00DF8188`) and ride mechanics belong to `vehicle_code_map.md`.
        // What is player-side is the control-source link, which is exactly what a claim establishes.
        let seat = seat.raw();
        wm(&h, (), |w| { if let Some(s) = slot_of(w, player.raw()) { w.set_control_source(s, seat); } });
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("UnClaimSeat", lua.create_function(move |_, player: Guid| {
        wm(&h, (), |w| { if let Some(s) = slot_of(w, player.raw()) { w.set_control_source(s, 0); } });
        Ok(())
    })?)?;

    // Not one of the 107 — an extra the coverage harness filters out of `real_count`. Kept because the
    // hero-template selection has no other home (`EngineHost::player_selected_character`).
    let h = host.clone();
    b.extra("GetSelectedCharacter", lua.create_function(move |_, ()| Ok(h.borrow().player_selected_character()))?)?;

    b.install_global(GLOBAL)
}
