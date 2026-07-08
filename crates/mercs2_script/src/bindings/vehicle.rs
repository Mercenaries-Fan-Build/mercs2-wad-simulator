//! `Vehicle` engine binding namespace — luaL_Reg table VA 0x00b98918, 40 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Vehicle")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

fn guid_opt(g: u64) -> Option<i64> {
    if g == 0 { None } else { Some(g as i64) }
}

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Vehicle";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Vehicle";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b98918;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetRiders", corpus_calls: 22 },
    Required { name: "GetDriver", corpus_calls: 122 },
    Required { name: "GetFromSeat", corpus_calls: 0 },
    Required { name: "GetFromRider", corpus_calls: 30 },
    Required { name: "GetSeatFromRider", corpus_calls: 7 },
    Required { name: "GetRiderFromSeat", corpus_calls: 0 },
    Required { name: "GetSeatByType", corpus_calls: 7 },
    Required { name: "GetSeatParams", corpus_calls: 1 },
    Required { name: "SetCanPlayerUse", corpus_calls: 2 },
    Required { name: "GetSeatToSeat", corpus_calls: 2 },
    Required { name: "IsSeatBlocked", corpus_calls: 2 },
    Required { name: "IsSeatALadder", corpus_calls: 2 },
    Required { name: "TransferToSeat", corpus_calls: 2 },
    Required { name: "Enter", corpus_calls: 22 },
    Required { name: "EnterBySeatGuid", corpus_calls: 4 },
    Required { name: "Exit", corpus_calls: 27 },
    Required { name: "HijackComplete", corpus_calls: 1 },
    Required { name: "HijackAbort", corpus_calls: 1 },
    Required { name: "HijackAbortDone", corpus_calls: 2 },
    Required { name: "EnableTurret", corpus_calls: 6 },
    Required { name: "SetTurretPitch", corpus_calls: 1 },
    Required { name: "SetTurretYaw", corpus_calls: 0 },
    Required { name: "OpenDoor", corpus_calls: 2 },
    Required { name: "CloseDoor", corpus_calls: 3 },
    Required { name: "IsFlying", corpus_calls: 4 },
    Required { name: "IsFlipped", corpus_calls: 5 },
    Required { name: "SpinHeli", corpus_calls: 0 },
    Required { name: "StartTankHijackMotion", corpus_calls: 0 },
    Required { name: "StopTankHijackMotion", corpus_calls: 2 },
    Required { name: "IsHijackRemote", corpus_calls: 1 },
    Required { name: "HijackStart", corpus_calls: 1 },
    Required { name: "SetHijackState", corpus_calls: 1 },
    Required { name: "SetHijackSuccess", corpus_calls: 1 },
    Required { name: "CancelHijack", corpus_calls: 2 },
    Required { name: "Usable", corpus_calls: 13 },
    Required { name: "IsHijackBad", corpus_calls: 0 },
    Required { name: "RestoreHealth", corpus_calls: 0 },
    Required { name: "RestoreAmmo", corpus_calls: 1 },
    Required { name: "SetParts", corpus_calls: 27 },
    Required { name: "ClearControls", corpus_calls: 1 },
];

/// Seat/rider queries + enter/exit + doors/flight, forwarded to the vehicle system through
/// [`crate::EngineHost`] (the real host backs it with `mercs2_vehicle`). Empty-seat / on-foot GUIDs
/// map to Lua `nil` so the game's `if not uDriver` control flow is authentic. The hijack FSM +
/// turret-aim cfuncs are a later pass.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real("GetDriver", lua.create_function(move |_, veh: i64| Ok(guid_opt(h.borrow().vehicle_driver(veh as u64))))?)?;
    let h = host.clone();
    b.real("GetRiders", lua.create_function(move |_, veh: i64| {
        Ok(h.borrow().vehicle_riders(veh as u64).into_iter().map(|g| g as i64).collect::<Vec<_>>())
    })?)?;
    let h = host.clone();
    b.real("GetFromRider", lua.create_function(move |_, rider: i64| Ok(guid_opt(h.borrow().vehicle_from_rider(rider as u64))))?)?;
    let h = host.clone();
    b.real("GetSeatFromRider", lua.create_function(move |_, rider: i64| Ok(h.borrow().vehicle_seat_from_rider(rider as u64)))?)?;
    let h = host.clone();
    b.real("GetSeatByType", lua.create_function(move |_, (veh, ty): (i64, String)| Ok(h.borrow().vehicle_seat_by_type(veh as u64, &ty)))?)?;
    let h = host.clone();
    b.real("Enter", lua.create_function(move |_, (veh, rider, seat): (i64, i64, Option<String>)| {
        Ok(h.borrow_mut().vehicle_enter(veh as u64, rider as u64, seat.as_deref().unwrap_or("d")))
    })?)?;
    let h = host.clone();
    b.real("Exit", lua.create_function(move |_, rider: i64| Ok(h.borrow_mut().vehicle_exit(rider as u64)))?)?;
    let h = host.clone();
    b.real("Usable", lua.create_function(move |_, veh: i64| Ok(h.borrow().vehicle_usable(veh as u64)))?)?;
    let h = host.clone();
    b.real("IsFlying", lua.create_function(move |_, veh: i64| Ok(h.borrow().vehicle_is_flying(veh as u64)))?)?;
    let h = host.clone();
    b.real("IsFlipped", lua.create_function(move |_, veh: i64| Ok(h.borrow().vehicle_is_flipped(veh as u64)))?)?;
    let h = host.clone();
    b.real("SetParts", lua.create_function(move |_, veh: i64| { h.borrow_mut().vehicle_set_parts(veh as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("OpenDoor", lua.create_function(move |_, veh: i64| { h.borrow_mut().vehicle_set_door(veh as u64, true); Ok(()) })?)?;
    let h = host.clone();
    b.real("CloseDoor", lua.create_function(move |_, veh: i64| { h.borrow_mut().vehicle_set_door(veh as u64, false); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetCanPlayerUse", lua.create_function(move |_, (veh, can): (i64, bool)| { h.borrow_mut().vehicle_set_can_player_use(veh as u64, can); Ok(()) })?)?;
    let h = host.clone();
    b.real("EnableTurret", lua.create_function(move |_, (veh, on): (i64, Option<bool>)| { h.borrow_mut().vehicle_enable_turret(veh as u64, on.unwrap_or(true)); Ok(()) })?)?;
    let h = host.clone();
    b.real("ClearControls", lua.create_function(move |_, veh: i64| { h.borrow_mut().vehicle_clear_controls(veh as u64); Ok(()) })?)?;

    // --- faithful-default GETTERS (game reads the return; seat/hijack state not modelled yet) ---
    // seat-lookup queries → empty seat / on-foot → nil so `if not uRider` control flow is authentic.
    b.real("GetFromSeat", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.real("GetRiderFromSeat", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.real("GetSeatParams", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    b.real("GetSeatToSeat", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;
    // seat predicate queries → not blocked / not a ladder.
    b.real("IsSeatBlocked", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsSeatALadder", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    // hijack predicate queries → not a remote / not a bad hijack.
    b.real("IsHijackRemote", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsHijackBad", lua.create_function(|_, _: MultiValue| Ok(false))?)?;

    // --- Hijack FSM → `mercs2_vehicle::HijackFsm` on the host (returns the resulting state name). ---
    // Each verb takes the vehicle guid as its first arg; the lifecycle is driven by the mission Lua.
    for (verb, event) in [
        ("HijackStart", "start"),
        ("StartTankHijackMotion", "tank_motion_on"),
        ("StopTankHijackMotion", "tank_motion_off"),
        ("SetHijackSuccess", "success"),
        ("HijackComplete", "complete"),
        ("HijackAbort", "abort"),
        ("HijackAbortDone", "abort_done"),
        ("CancelHijack", "cancel"),
    ] {
        let h = host.clone();
        let ev = event;
        b.real(verb, lua.create_function(move |_, veh: i64| {
            Ok(h.borrow_mut().vehicle_hijack_event(veh as u64, ev))
        })?)?;
    }
    // Vehicle.SetHijackState(veh, name) — explicit override.
    let h = host.clone();
    b.real("SetHijackState", lua.create_function(move |_, (veh, name): (i64, String)| {
        Ok(h.borrow_mut().vehicle_hijack_event(veh as u64, &format!("set:{name}")))
    })?)?;

    // --- Turret / rotor articulation → `mercs2_vehicle::TurretAim` on the host. ---
    let h = host.clone();
    b.real("SetTurretPitch", lua.create_function(move |_, (veh, pitch): (i64, f32)| {
        h.borrow_mut().vehicle_set_turret(veh as u64, Some(pitch), None, None);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetTurretYaw", lua.create_function(move |_, (veh, yaw): (i64, f32)| {
        h.borrow_mut().vehicle_set_turret(veh as u64, None, Some(yaw), None);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SpinHeli", lua.create_function(move |_, (veh, on): (i64, Option<bool>)| {
        h.borrow_mut().vehicle_set_turret(veh as u64, None, None, Some(on.unwrap_or(true)));
        Ok(())
    })?)?;

    // Vehicle.RestoreHealth(veh) — restore the vehicle object to full health (Object health seam).
    let h = host.clone();
    b.real("RestoreHealth", lua.create_function(move |_, veh: i64| {
        let max = h.borrow().object_max_health(veh as u64);
        h.borrow_mut().object_set_health(veh as u64, max);
        Ok(())
    })?)?;

    // --- seat occupancy + weapon restore → real host state. ---
    let h = host.clone();
    b.real("EnterBySeatGuid", lua.create_function(move |_, (human, seat): (i64, i64)| {
        h.borrow_mut().human_enter_seat(human as u64, seat as u64);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("TransferToSeat", lua.create_function(move |_, (human, seat): (i64, i64)| {
        h.borrow_mut().human_enter_seat(human as u64, seat as u64);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("RestoreAmmo", lua.create_function(move |_, weapon: i64| {
        h.borrow_mut().weapon_restore_ammo(weapon as u64);
        Ok(())
    })?)?;

    b.install_global(GLOBAL)
}
