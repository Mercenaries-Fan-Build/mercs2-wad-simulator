//! `Human` engine binding namespace — luaL_Reg table VA 0x00b99ef0, 21 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Human")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Human";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Human";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99ef0;

pub const REQUIRED: &[Required] = &[
    Required { name: "DoAction", corpus_calls: 8 },
    Required { name: "SetState", corpus_calls: 24 },
    Required { name: "Knockdown", corpus_calls: 4 },
    Required { name: "SetPreemptiveRagdoll", corpus_calls: 4 },
    Required { name: "ForceExitSeatNoSnap", corpus_calls: 10 },
    Required { name: "Emote", corpus_calls: 0 },
    Required { name: "PlayRawAnimation", corpus_calls: 9 },
    Required { name: "PersistTransform", corpus_calls: 5 },
    Required { name: "IsSwimming", corpus_calls: 2 },
    Required { name: "IsCarrying", corpus_calls: 5 },
    Required { name: "Drop", corpus_calls: 5 },
    Required { name: "IsGrappling", corpus_calls: 3 },
    Required { name: "StopGrappling", corpus_calls: 3 },
    Required { name: "EnableWeapons", corpus_calls: 2 },
    Required { name: "DisableWeapons", corpus_calls: 27 },
    Required { name: "SetFireLock", corpus_calls: 4 },
    Required { name: "EquipWeapon", corpus_calls: 0 },
    Required { name: "StowWeapon", corpus_calls: 0 },
    Required { name: "SetAllowCorpseCleanup", corpus_calls: 3 },
    Required { name: "Scrub", corpus_calls: 2 },
    Required { name: "SetJostleEnabled", corpus_calls: 2 },
];

/// Humanoid state surface. `SetState`/`DoAction` (the boot-relevant teleport + civ/hijack drivers) and
/// the carry/swim/grapple queries are **real** — they record onto / read the host's per-human state.
/// The remaining animation/weapon/ragdoll *actions* (Knockdown, PlayRawAnimation, DisableWeapons, …)
/// are faithful no-ops: this build has no ragdoll/weapon-lock runtime, so the game's Lua control flow
/// runs unchanged and simply produces no ragdoll/anim side effect.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // --- driven state (real: recorded on the host, keyed by GUID) ---
    // Human.SetState(guid, stance, action) — mrxutil.lua:314 teleport calls ("upright","idle"); the
    // civ/hijack tables drive ("InVehicle", <animation>). `action` is optional (SetState(g,"Cower")).
    let h = host.clone();
    b.real(
        "SetState",
        lua.create_function(move |_, (guid, stance, action): (i64, String, Option<String>)| {
            h.borrow_mut().human_set_state(guid as u64, &stance, action.as_deref().unwrap_or(""));
            Ok(())
        })?,
    )?;
    let h = host.clone();
    b.real(
        "DoAction",
        lua.create_function(move |_, (guid, action): (i64, String)| {
            h.borrow_mut().human_do_action(guid as u64, &action);
            Ok(())
        })?,
    )?;

    // --- queries (real: read host state) ---
    let h = host.clone();
    b.real("IsSwimming", lua.create_function(move |_, guid: i64| Ok(h.borrow().human_is_swimming(guid as u64)))?)?;
    let h = host.clone();
    b.real("IsCarrying", lua.create_function(move |_, guid: i64| Ok(h.borrow().human_is_carrying(guid as u64)))?)?;
    let h = host.clone();
    b.real("IsGrappling", lua.create_function(move |_, guid: i64| Ok(h.borrow().human_is_grappling(guid as u64)))?)?;

    // --- animation / weapon / ragdoll actions: faithful no-ops (no ragdoll/weapon-lock runtime yet) ---
    for name in [
        "Knockdown",
        "SetPreemptiveRagdoll",
        "ForceExitSeatNoSnap",
        "Emote",
        "PlayRawAnimation",
        "PersistTransform",
        "Drop",
        "StopGrappling",
        "EnableWeapons",
        "DisableWeapons",
        "SetFireLock",
        "EquipWeapon",
        "StowWeapon",
        "SetAllowCorpseCleanup",
        "Scrub",
        "SetJostleEnabled",
    ] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    b.install_global(GLOBAL)
}
