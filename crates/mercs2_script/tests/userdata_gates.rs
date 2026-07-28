//! The shipped `type(u) == "userdata"` gates, run against the converted binding surface.
//!
//! Retail hands GUIDs to Lua as **lightuserdata** (`FUN_0059FF50` accepts only the userdata type tags;
//! `Player.GetAnyCharacter` pushes its sentinel with type tag 2 — see [`mercs2_script::Guid`]), and the
//! shipped scripts type-check on that. There are 114 `"userdata"` comparisons in the vendored corpus:
//! roughly 54 gates that must **pass** and roughly 60 guards that must **not** fire. While handles were
//! Lua integers every one of them was wrong.
//!
//! `guid.rs` proves the newtype in isolation. This file proves the *bindings* — each case runs the
//! actual corpus gate, cited by file and line, against the real namespace body.

use std::cell::RefCell;
use std::rc::Rc;

use mercs2_script::{EngineHost, ScriptHost, SharedHost};

/// A host that mints filter handles and owns one designator, so the handle-returning bindings under
/// test have something real to return. Everything else inherits the `EngineHost` defaults.
#[derive(Default)]
struct GateHost {
    next_filter: u64,
    members: Vec<u64>,
}

impl EngineHost for GateHost {
    fn log(&mut self, _source: &str, _msg: &str) {}
    fn get_level_name(&self) -> String {
        "vz".into()
    }
    fn guid_by_name(&mut self, _name: &str) -> u64 {
        0
    }
    fn pg_spawn(&mut self, _template: &str, _pos: [f32; 3], _yaw: f32, _high_detail: bool) -> u64 {
        0
    }
    fn object_set_name(&mut self, _guid: u64, _name: &str) {}
    fn object_set_position(&mut self, _guid: u64, _pos: [f32; 3]) {}
    fn object_set_yaw(&mut self, _guid: u64, _yaw: f32) {}
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}
    fn add_layers(&mut self, _layers: &[String]) {}

    fn object_filter_create(&mut self) -> u64 {
        self.next_filter += 1;
        0x1000_0000 + self.next_filter
    }
    fn object_filter_add(&mut self, _filter: u64, guid: u64, exclude: bool) {
        if !exclude {
            self.members.push(guid);
        }
    }
    fn object_filter_objects(&self, _filter: u64) -> Vec<u64> {
        self.members.clone()
    }
    /// A designator with a live owner, so `Airstrike.FindDesignatorOwner` returns a real handle.
    fn airstrike_designator_owner(&self) -> u64 {
        0x2000_0007
    }
}

fn host() -> ScriptHost {
    let h: SharedHost = Rc::new(RefCell::new(GateHost::default()));
    let sh = ScriptHost::bare().expect("bare host");
    sh.register_engine(h).expect("register engine");
    sh
}

/// `ObjectFilter.Create` mints a handle, and the objects that come back out of `GetObjects` pass the
/// gate the task-objective code puts them through.
///
/// `mrxtaskobjectiveaction.lua:21` reads the member list with
/// `local tGuids = ObjectFilter.GetObjects(self._uTgtObjFilter, false)`, and the same objective's
/// `_TargetActioned` / `_TargetDestroyed` (:31, :40) refuse to act on an element unless
/// `type(uGuid) == "userdata"`. With integer handles that gate never opened and the objective could
/// never retire a target.
#[test]
fn object_filter_handles_and_members_are_userdata() {
    let sh = host();
    let (filter_ty, member_ty, matches): (String, String, bool) = sh
        .eval(
            r#"
            local uFilter = ObjectFilter.Create()
            local uTarget = ObjectFilter.Create()   -- any engine handle will do as a member
            ObjectFilter.AddObject(uFilter, uTarget, false)
            local tGuids = ObjectFilter.GetObjects(uFilter, false)
            return type(uFilter), type(tGuids[1]), tGuids[1] == uTarget
            "#,
        )
        .unwrap();
    assert_eq!(filter_ty, "userdata", "a filter handle must be lightuserdata");
    assert_eq!(member_ty, "userdata", "mrxtaskobjectiveaction.lua:31 gates on this");
    assert!(matches, "a handle must compare equal to itself across the boundary");
}

/// `Sys.StringToGuid` produces a handle, not a number: `wiftutorialgatehonk.lua:10` assigns it to
/// `uGateGuid` and `wifpmcgarage.lua:243` to `uVehicle`, both of which then travel into handle slots.
#[test]
fn string_to_guid_returns_a_handle() {
    let sh = host();
    let (ty, unparseable_is_nil): (String, bool) = sh
        .eval(
            r#"
            local uGate = Sys.StringToGuid("0x000f9a64")
            return type(uGate), Sys.StringToGuid("not a guid") == nil
            "#,
        )
        .unwrap();
    assert_eq!(ty, "userdata");
    assert!(unparseable_is_nil, "an unparseable literal is a miss, and a miss is nil");
}

/// `Airstrike.FindDesignatorOwner` returns a player handle that survives
/// `mrxsupportmanager.lua:138`'s `if uPlayerGuid then` and can key
/// `CurrentlyEquippedSupport[uPlayerGuid]` (:139).
#[test]
fn designator_owner_is_a_usable_handle() {
    let sh = host();
    let (ty, keyed): (String, bool) = sh
        .eval(
            r#"
            local uOwner = Airstrike.FindDesignatorOwner(nil)
            local t = {}
            t[uOwner] = "support"
            return type(uOwner), t[Airstrike.FindDesignatorOwner(nil)] == "support"
            "#,
        )
        .unwrap();
    assert_eq!(ty, "userdata");
    assert!(keyed, "handles must be stable table keys — the corpus uses 402 of them");
}

/// The **nil-handle contract**: a handle miss is a silent neutral answer, never a raised error. The
/// shipped code chains lookups (`ObjectFilter.Eval(uFilter, Pg.GetGuidByName(sName))`) and lets a
/// failed lookup fall through, so a raise here would abort the caller's whole callback chain.
#[test]
fn missing_handles_do_not_raise_in_the_converted_namespaces() {
    let sh = host();
    for src in [
        "ObjectFilter.Eval(nil, nil)",
        "ObjectFilter.AddObject(nil, nil)",
        "ObjectFilter.GetObjects(Pg.GetGuidByName('nope'))",
        "ObjectState.SetState(nil, 'idle')",
        "ObjectState.StartEmitter(Pg.GetGuidByName('nope'), 'smoke')",
        "Ai.AddInfraction(nil, nil, 100)",
        "Ai.SetInfractionMultiplier(Pg.GetGuidByName('Guerilla'), 0.1)",
        "Sound.CueSound(0, 'ui_static')",
        "Sound.StopSound(nil, 'ui_static')",
        "Camera.Shake(nil, 'ShakeCameraMedium', nil, 6, 5)",
        "Weapon.Reload(nil)",
        // The fire table nests under `Graphics` as retail's marker-delimited sub-table.
        "Graphics.FuelTrail.Ignite(nil)",
        "Timer.Start(Pg.GetGuidByName('nope'))",
        "Event.Create(Event.ObjectDeath, {Pg.GetGuidByName('nope')}, function() end)",
    ] {
        sh.exec(src, "@nil-handle")
            .unwrap_or_else(|e| panic!("{src} must not raise on a missing handle: {e}"));
    }
}
