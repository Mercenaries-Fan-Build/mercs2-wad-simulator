//! `Object` engine binding namespace — luaL_Reg table VA 0x00b99608, 87 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Object")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Object";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Object";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99608;

pub const REQUIRED: &[Required] = &[
    Required { name: "GetParent", corpus_calls: 17 },
    Required { name: "IsTemplate", corpus_calls: 1 },
    Required { name: "GetPosition", corpus_calls: 201 },
    Required { name: "SetPosition", corpus_calls: 23 },
    Required { name: "SetPositionToObject", corpus_calls: 0 },
    Required { name: "SetTransformToObject", corpus_calls: 28 },
    Required { name: "GetDistanceFrom", corpus_calls: 11 },
    Required { name: "GetYaw", corpus_calls: 50 },
    Required { name: "SetYaw", corpus_calls: 19 },
    Required { name: "GetName", corpus_calls: 13 },
    Required { name: "SetName", corpus_calls: 9 },
    Required { name: "GetModelName", corpus_calls: 0 },
    Required { name: "SetModelName", corpus_calls: 2 },
    Required { name: "GetVelocity", corpus_calls: 12 },
    Required { name: "GetVelocitySquared", corpus_calls: 0 },
    Required { name: "GetVelocityVector", corpus_calls: 0 },
    Required { name: "GetHealth", corpus_calls: 48 },
    Required { name: "SetHealth", corpus_calls: 9 },
    Required { name: "GetMaxHealth", corpus_calls: 12 },
    Required { name: "GetNodeHealth", corpus_calls: 1 },
    Required { name: "GetLocalizedName", corpus_calls: 25 },
    Required { name: "GetCashValue", corpus_calls: 1 },
    Required { name: "IsAlive", corpus_calls: 139 },
    Required { name: "IsPlayerControlled", corpus_calls: 74 },
    Required { name: "InSeat", corpus_calls: 6 },
    Required { name: "InVehicle", corpus_calls: 2 },
    Required { name: "InsideBoundary", corpus_calls: 8 },
    Required { name: "OutsideBoundary", corpus_calls: 1 },
    Required { name: "Remove", corpus_calls: 83 },
    Required { name: "FadeOut", corpus_calls: 21 },
    Required { name: "Kill", corpus_calls: 29 },
    Required { name: "IsValid", corpus_calls: 2 },
    Required { name: "Revive", corpus_calls: 12 },
    Required { name: "AreEqual", corpus_calls: 0 },
    Required { name: "GetInvincible", corpus_calls: 2 },
    Required { name: "SetInvincible", corpus_calls: 35 },
    Required { name: "SetUnkillable", corpus_calls: 3 },
    Required { name: "SetInfiniteAmmo", corpus_calls: 28 },
    Required { name: "AddLabel", corpus_calls: 7 },
    Required { name: "RemoveLabel", corpus_calls: 4 },
    Required { name: "HasLabel", corpus_calls: 117 },
    Required { name: "IsDisguised", corpus_calls: 1 },
    Required { name: "GetMass", corpus_calls: 5 },
    Required { name: "SetMass", corpus_calls: 0 },
    Required { name: "IsAwake", corpus_calls: 17 },
    Required { name: "IsHibernated", corpus_calls: 5 },
    Required { name: "GetHibernationDistance", corpus_calls: 5 },
    Required { name: "SetHibernationDistance", corpus_calls: 2 },
    Required { name: "RevertHibernationDistance", corpus_calls: 0 },
    Required { name: "TransformLocalToWorld", corpus_calls: 0 },
    Required { name: "GetHardpointPosition", corpus_calls: 12 },
    Required { name: "GetHardpointYaw", corpus_calls: 0 },
    Required { name: "GetHardpointPitch", corpus_calls: 0 },
    Required { name: "ApplyImpulse", corpus_calls: 8 },
    Required { name: "ApplyPointImpulse", corpus_calls: 3 },
    Required { name: "ApplyAngularImpulse", corpus_calls: 2 },
    Required { name: "SetVisible", corpus_calls: 7 },
    Required { name: "IsVisible", corpus_calls: 11 },
    Required { name: "EnablePhysics", corpus_calls: 11 },
    Required { name: "DisablePhysics", corpus_calls: 29 },
    Required { name: "GetPhysicsType", corpus_calls: 3 },
    Required { name: "PlayAnimation", corpus_calls: 4 },
    Required { name: "StopAnimation", corpus_calls: 0 },
    Required { name: "StopAnimationChannel", corpus_calls: 1 },
    Required { name: "StopAllAnimation", corpus_calls: 3 },
    Required { name: "Attach", corpus_calls: 8 },
    Required { name: "Detach", corpus_calls: 7 },
    Required { name: "IsAttached", corpus_calls: 1 },
    Required { name: "GetAttachedObjects", corpus_calls: 1 },
    Required { name: "PlayMaterialAnimation", corpus_calls: 13 },
    Required { name: "StopMaterialAnimation", corpus_calls: 3 },
    Required { name: "OpenGate", corpus_calls: 10 },
    Required { name: "CloseGate", corpus_calls: 15 },
    Required { name: "GetWinchState", corpus_calls: 0 },
    Required { name: "SetWinchState", corpus_calls: 5 },
    Required { name: "HasWinch", corpus_calls: 0 },
    Required { name: "IsWinching", corpus_calls: 0 },
    Required { name: "IsWinched", corpus_calls: 7 },
    Required { name: "AttachCargoToWinch", corpus_calls: 5 },
    Required { name: "DetachCargoFromWinch", corpus_calls: 7 },
    Required { name: "AddQualityRef", corpus_calls: 1 },
    Required { name: "RemoveQualityRef", corpus_calls: 1 },
    Required { name: "QueueAcceleration", corpus_calls: 0 },
    Required { name: "BeginQueuedAcceleration", corpus_calls: 0 },
    Required { name: "GetHeightAboveTerrain", corpus_calls: 0 },
    Required { name: "AddToDisposer", corpus_calls: 4 },
    Required { name: "RemoveFromDisposer", corpus_calls: 0 },
];

/// Boot slice: the transform/name mutators the `MrxUtil.SpawnActor` recipe uses. `SetTransformToObject`
/// / `Attach` / `DisablePhysics` are accepted as no-ops so the full `SpawnActor` + `_SpawnActorComplete`
/// body runs without erroring (wired to real behavior by a later silo). The other ~79 `Object.*` cfuncs
/// (health, physics impulses, animation, winch, hibernation) are for later silos.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real(
        "SetName",
        lua.create_function(move |_, (guid, name): (i64, String)| {
            h.borrow_mut().object_set_name(guid as u64, &name);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "SetPosition",
        lua.create_function(move |_, (guid, x, y, z): (i64, f32, f32, f32)| {
            h.borrow_mut().object_set_position(guid as u64, [x, y, z]);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "SetYaw",
        lua.create_function(move |_, (guid, yaw): (i64, f32)| {
            h.borrow_mut().object_set_yaw(guid as u64, yaw);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "GetPosition",
        lua.create_function(move |_, guid: i64| {
            let p = h.borrow_mut().object_get_position(guid as u64);
            Ok((p[0], p[1], p[2]))
        })?,
    )?;
    let h = host.clone();
    b.real(
        "GetYaw",
        lua.create_function(move |_, guid: i64| Ok(h.borrow_mut().object_get_yaw(guid as u64)))?,
    )?;

    // --- health / life / labels (the highest-traffic Object cfuncs) ---
    let h = host.clone();
    b.real("GetHealth", lua.create_function(move |_, guid: i64| Ok(h.borrow().object_health(guid as u64)))?)?;
    let h = host.clone();
    b.real("SetHealth", lua.create_function(move |_, (guid, hp): (i64, f32)| { h.borrow_mut().object_set_health(guid as u64, hp); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetMaxHealth", lua.create_function(move |_, guid: i64| Ok(h.borrow().object_max_health(guid as u64)))?)?;
    let h = host.clone();
    b.real("GetVelocity", lua.create_function(move |_, guid: i64| Ok(h.borrow().object_velocity(guid as u64)))?)?;
    let h = host.clone();
    b.real("IsAlive", lua.create_function(move |_, guid: i64| Ok(h.borrow().object_is_alive(guid as u64)))?)?;
    let h = host.clone();
    // Kill also fires the object's ObjectDeath handlers (the condition-feed via the shared event mgr).
    b.real("Kill", lua.create_function(move |lua, guid: i64| {
        h.borrow_mut().object_kill(guid as u64);
        super::event::fire_object_death(lua, guid as u64)?;
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("Revive", lua.create_function(move |_, guid: i64| { h.borrow_mut().object_revive(guid as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("Remove", lua.create_function(move |_, guid: i64| { h.borrow_mut().object_remove(guid as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetName", lua.create_function(move |_, guid: i64| Ok(h.borrow().object_name(guid as u64)))?)?;
    let h = host.clone();
    b.real("AddLabel", lua.create_function(move |_, (guid, label): (i64, String)| { h.borrow_mut().object_add_label(guid as u64, &label); Ok(()) })?)?;
    let h = host.clone();
    b.real("RemoveLabel", lua.create_function(move |_, (guid, label): (i64, String)| { h.borrow_mut().object_remove_label(guid as u64, &label); Ok(()) })?)?;
    let h = host.clone();
    b.real("HasLabel", lua.create_function(move |_, (guid, label): (i64, String)| Ok(h.borrow().object_has_label(guid as u64, &label)))?)?;
    let h = host.clone();
    b.real("SetInvincible", lua.create_function(move |_, (guid, on): (i64, bool)| { h.borrow_mut().object_set_invincible(guid as u64, on); Ok(()) })?)?;

    // Anchor/attachment + physics toggles: no-ops so the full SpawnActor body runs.
    b.stub(
        "SetTransformToObject",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    b.stub("Attach", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.stub(
        "DisablePhysics",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;

    b.install_global(GLOBAL)
}
