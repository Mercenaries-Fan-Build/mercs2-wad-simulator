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
use crate::{Guid, SharedHost};

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
        lua.create_function(move |_, (guid, name): (Guid, String)| {
            h.borrow_mut().object_set_name(guid.raw(), &name);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "SetPosition",
        lua.create_function(move |_, (guid, x, y, z): (Guid, f32, f32, f32)| {
            h.borrow_mut().object_set_position(guid.raw(), [x, y, z]);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "SetYaw",
        lua.create_function(move |_, (guid, yaw): (Guid, f32)| {
            h.borrow_mut().object_set_yaw(guid.raw(), yaw);
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "GetPosition",
        lua.create_function(move |_, guid: Guid| {
            let p = h.borrow_mut().object_get_position(guid.raw());
            Ok((p[0], p[1], p[2]))
        })?,
    )?;
    let h = host.clone();
    b.real(
        "GetYaw",
        lua.create_function(move |_, guid: Guid| Ok(h.borrow_mut().object_get_yaw(guid.raw())))?,
    )?;

    // --- health / life / labels (the highest-traffic Object cfuncs) ---
    let h = host.clone();
    b.real("GetHealth", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_health(guid.raw())))?)?;
    let h = host.clone();
    b.real("SetHealth", lua.create_function(move |_, (guid, hp): (Guid, f32)| { h.borrow_mut().object_set_health(guid.raw(), hp); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetMaxHealth", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_max_health(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetVelocity", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_velocity(guid.raw())))?)?;
    let h = host.clone();
    b.real("IsAlive", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_alive(guid.raw())))?)?;
    let h = host.clone();
    // Kill also fires the object's ObjectDeath handlers (the condition-feed via the shared event mgr).
    b.real("Kill", lua.create_function(move |lua, guid: Guid| {
        h.borrow_mut().object_kill(guid.raw());
        super::event::fire_object_death(lua, guid.raw())?;
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("Revive", lua.create_function(move |_, guid: Guid| { h.borrow_mut().object_revive(guid.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("Remove", lua.create_function(move |_, guid: Guid| { h.borrow_mut().object_remove(guid.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetName", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_name(guid.raw())))?)?;
    let h = host.clone();
    b.real("AddLabel", lua.create_function(move |_, (guid, label): (Guid, String)| { h.borrow_mut().object_add_label(guid.raw(), &label); Ok(()) })?)?;
    let h = host.clone();
    b.real("RemoveLabel", lua.create_function(move |_, (guid, label): (Guid, String)| { h.borrow_mut().object_remove_label(guid.raw(), &label); Ok(()) })?)?;
    let h = host.clone();
    // Tolerant of a nil guid (data-setup code like MrxUtil.GetFaction probes objects that only spawn at
    // runtime) → false, matching the lenient engine rather than erroring on the arg.
    b.real("HasLabel", lua.create_function(move |_, (guid, label): (Guid, String)| {
        Ok(if guid.is_some() { h.borrow().object_has_label(guid.raw(), &label) } else { false })
    })?)?;
    let h = host.clone();
    b.real("SetInvincible", lua.create_function(move |_, (guid, on): (Guid, bool)| { h.borrow_mut().object_set_invincible(guid.raw(), on); Ok(()) })?)?;

    // --- identity / naming (real: host state) ---
    let h = host.clone();
    b.real("GetParent", lua.create_function(move |_, guid: Guid| {
        Ok(Guid(h.borrow().object_parent(guid.raw())))
    })?)?;
    let h = host.clone();
    b.real("GetModelName", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_model_name(guid.raw())))?)?;
    let h = host.clone();
    b.real("SetModelName", lua.create_function(move |_, (guid, name): (Guid, String)| { h.borrow_mut().object_set_model_name(guid.raw(), &name); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetLocalizedName", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_localized_name(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetCashValue", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_cash_value(guid.raw())))?)?;

    // --- validity / control / disguise (real: host state) ---
    let h = host.clone();
    b.real("IsValid", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_valid(guid.raw())))?)?;
    let h = host.clone();
    // ⚠ Returns a player HANDLE (nil when none), not a boolean — the shipped Lua binds the result and
    // passes it to `Player.*`. See `EngineHost::object_is_player_controlled`.
    b.real("IsPlayerControlled", lua.create_function(move |_, guid: Guid| {
        Ok(Guid(h.borrow().object_is_player_controlled(guid.raw())))
    })?)?;
    let h = host.clone();
    b.real("IsDisguised", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_disguised(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetInvincible", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_get_invincible(guid.raw())))?)?;
    let h = host.clone();
    b.real("IsTemplate", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_template(guid.raw())))?)?;

    // --- comparison / distance (real: computed from host positions) ---
    b.real("AreEqual", lua.create_function(|_, (a, b): (Guid, Guid)| Ok(a == b))?)?;
    let h = host.clone();
    b.real("GetDistanceFrom", lua.create_function(move |_, (a, b): (Guid, Guid)| Ok(h.borrow_mut().object_distance(a.raw(), b.raw())))?)?;

    // --- physics / mass / velocity (real getters + record setters) ---
    let h = host.clone();
    b.real("GetMass", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_mass(guid.raw())))?)?;
    let h = host.clone();
    b.real("SetMass", lua.create_function(move |_, (guid, m): (Guid, f32)| { h.borrow_mut().object_set_mass(guid.raw(), m); Ok(()) })?)?;
    let h = host.clone();
    b.real("GetPhysicsType", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_physics_type(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetVelocityVector", lua.create_function(move |_, guid: Guid| { let v = h.borrow().object_velocity_vector(guid.raw()); Ok((v[0], v[1], v[2])) })?)?;
    let h = host.clone();
    b.real("GetVelocitySquared", lua.create_function(move |_, guid: Guid| { let s = h.borrow().object_velocity(guid.raw()); Ok(s * s) })?)?;
    let h = host.clone();
    b.real("EnablePhysics", lua.create_function(move |_, guid: Guid| { h.borrow_mut().object_set_physics_enabled(guid.raw(), true); Ok(()) })?)?;

    // --- visibility (real: host state) ---
    let h = host.clone();
    b.real("IsVisible", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_visible(guid.raw())))?)?;
    let h = host.clone();
    b.real("SetVisible", lua.create_function(move |_, (guid, on): (Guid, bool)| { h.borrow_mut().object_set_visible(guid.raw(), on); Ok(()) })?)?;

    // --- hibernation / streaming (real: host state; part of the world-streaming spec) ---
    let h = host.clone();
    b.real("IsAwake", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_awake(guid.raw())))?)?;
    let h = host.clone();
    b.real("IsHibernated", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_hibernated(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetHibernationDistance", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_hibernation_distance(guid.raw())))?)?;
    let h = host.clone();
    b.real("SetHibernationDistance", lua.create_function(move |_, (guid, d): (Guid, f32)| { h.borrow_mut().object_set_hibernation_distance(guid.raw(), d); Ok(()) })?)?;

    // --- attachment (real getters) ---
    let h = host.clone();
    b.real("IsAttached", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_is_attached(guid.raw())))?)?;
    let h = host.clone();
    b.real("GetAttachedObjects", lua.create_function(move |lua, guid: Guid| {
        let t = lua.create_table()?;
        for (i, g) in h.borrow().object_attached_objects(guid.raw()).into_iter().enumerate() {
            t.set(i + 1, Guid(g))?;
        }
        Ok(t)
    })?)?;

    // --- life-adjacent actions (real: host state) ---
    let h = host.clone();
    b.real("FadeOut", lua.create_function(move |_, guid: Guid| { h.borrow_mut().object_fade_out(guid.raw()); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetUnkillable", lua.create_function(move |_, (guid, on): (Guid, bool)| { h.borrow_mut().object_set_unkillable(guid.raw(), on); Ok(()) })?)?;
    let h = host.clone();
    b.real("SetInfiniteAmmo", lua.create_function(move |_, (guid, on): (Guid, bool)| { h.borrow_mut().object_set_infinite_ammo(guid.raw(), on); Ok(()) })?)?;

    // --- const-default getters (faithful: unmodelled → neutral, so Lua never hits nil) ---
    b.real("GetNodeHealth", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    // `InSeat`/`InVehicle` are the same question of the same state — retail answers both from the
    // character's `RiderLink` (`object_entity_core_code_map.md` §"InSeat"/"InVehicle"), so they are
    // backed by one host query rather than one being live and the other a stale `false`.
    let h = host.clone();
    b.real("InSeat", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_in_seat(guid.raw())))?)?;
    let h = host.clone();
    b.real("InVehicle", lua.create_function(move |_, guid: Guid| Ok(h.borrow().object_in_seat(guid.raw())))?)?;
    b.real("InsideBoundary", lua.create_function(|_, _: MultiValue| Ok(true))?)?;
    b.real("OutsideBoundary", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetHeightAboveTerrain", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    b.real("HasWinch", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsWinching", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("IsWinched", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("GetWinchState", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    // Hardpoint queries: faithful stand-in — return the object's own transform (no per-hardpoint rig yet).
    let h = host.clone();
    b.real("GetHardpointPosition", lua.create_function(move |_, (guid, _hp): (Guid, Option<String>)| { let p = h.borrow_mut().object_get_position(guid.raw()); Ok((p[0], p[1], p[2])) })?)?;
    let h = host.clone();
    b.real("GetHardpointYaw", lua.create_function(move |_, (guid, _hp): (Guid, Option<String>)| Ok(h.borrow_mut().object_get_yaw(guid.raw())))?)?;
    b.real("GetHardpointPitch", lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    // Local→world transform: faithful identity passthrough (returns the point unchanged) so callers get
    // usable coords rather than nil (no per-object basis modelled yet).
    b.real("TransformLocalToWorld", lua.create_function(|_, (_guid, x, y, z): (Guid, f32, f32, f32)| Ok((x, y, z)))?)?;

    // Anchor/attachment: Attach/Detach drive the real host attachment graph (GetParent/IsAttached/
    // GetAttachedObjects read it). SetTransformToObject (snap-to-anchor) has no per-object basis yet.
    b.stub(
        "SetTransformToObject",
        lua.create_function(|_, _: MultiValue| Ok(()))?,
    )?;
    let h = host.clone();
    b.real("Attach", lua.create_function(move |_, (child, parent): (Guid, Guid)| {
        h.borrow_mut().object_attach(child.raw(), parent.raw());
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("Detach", lua.create_function(move |_, child: Guid| {
        h.borrow_mut().object_detach(child.raw());
        Ok(())
    })?)?;
    // DisablePhysics records the physics-disabled state on the host (mrxutil teleport disables it).
    let h = host.clone();
    b.real("DisablePhysics", lua.create_function(move |_, guid: Guid| { h.borrow_mut().object_set_physics_enabled(guid.raw(), false); Ok(()) })?)?;

    // --- animation / winch / cargo / impulse / disposer / accel actions → recorded object commands
    // the anim/physics/winch runtime drains (verb + args = the requested action). ---
    super::record_all(&mut b, lua, host, "Object", &[
        "SetPositionToObject",
        "PlayAnimation",
        "StopAnimation",
        "StopAnimationChannel",
        "StopAllAnimation",
        "PlayMaterialAnimation",
        "StopMaterialAnimation",
        "OpenGate",
        "CloseGate",
        "SetWinchState",
        "AttachCargoToWinch",
        "DetachCargoFromWinch",
        "ApplyImpulse",
        "ApplyPointImpulse",
        "ApplyAngularImpulse",
        "QueueAcceleration",
        "BeginQueuedAcceleration",
        "AddQualityRef",
        "RemoveQualityRef",
        "AddToDisposer",
        "RemoveFromDisposer",
        "RevertHibernationDistance",
    ])?;

    b.install_global(GLOBAL)
}
