//! `Airstrike` engine binding namespace — luaL_Reg table VA 0x00b9a8c8, 12 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Airstrike")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use crate::SharedHost;
use super::{Installed, NsBuilder, Required};

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Airstrike";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Airstrike";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a8c8;

pub const REQUIRED: &[Required] = &[
    Required { name: "SpawnCarpetBombLine", corpus_calls: 2 },
    Required { name: "SpawnPlaneNew", corpus_calls: 0 },
    Required { name: "SpawnOrdnance", corpus_calls: 20 },
    Required { name: "SpawnTargettedOrdnance", corpus_calls: 7 },
    Required { name: "ConeSpawn", corpus_calls: 14 },
    Required { name: "FindExitPoint", corpus_calls: 0 },
    Required { name: "EquipDesignator", corpus_calls: 3 },
    Required { name: "RemoveDesignator", corpus_calls: 2 },
    Required { name: "RefillDesignator", corpus_calls: 2 },
    Required { name: "Flyby", corpus_calls: 21 },
    Required { name: "SpawnDirectedObject", corpus_calls: 3 },
    Required { name: "FindDesignatorOwner", corpus_calls: 1 },
];

/// Support / airstrike spawns + laser designator. Spawning ordnance/planes and the designator
/// lifecycle drive systems we don't own yet, so those are faithful no-ops; the two queries
/// (`FindExitPoint`, `FindDesignatorOwner`) return a faithful nil (no result). A later silo backs the
/// spawns with the real support system (see report — needs `airstrike_*` / spawn host methods).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // FindDesignatorOwner → the real per-player designator registry (0 → nil).
    let h = host.clone();
    b.real("FindDesignatorOwner", lua.create_function(move |_, _: MultiValue| {
        let o = h.borrow().airstrike_designator_owner();
        Ok(if o == 0 { None } else { Some(o as i64) })
    })?)?;
    // No exit-point pathing yet.
    b.real("FindExitPoint", lua.create_function(|_, _: MultiValue| Ok(Option::<i64>::None))?)?;

    // Designator lifecycle → the real per-player designator state.
    let h = host.clone();
    b.real("EquipDesignator", lua.create_function(move |_, p: i64| { h.borrow_mut().airstrike_equip_designator(p as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("RemoveDesignator", lua.create_function(move |_, p: i64| { h.borrow_mut().airstrike_remove_designator(p as u64); Ok(()) })?)?;
    let h = host.clone();
    b.real("RefillDesignator", lua.create_function(move |_, p: i64| { h.borrow_mut().airstrike_refill_designator(p as u64); Ok(()) })?)?;

    // Ordnance/plane spawns → recorded airstrike requests (kind + position) the runtime realizes.
    for (name, kind) in [
        ("SpawnOrdnance", "ordnance"),
        ("SpawnTargettedOrdnance", "targetted_ordnance"),
        ("SpawnCarpetBombLine", "carpet_bomb"),
        ("SpawnPlaneNew", "plane"),
        ("ConeSpawn", "cone"),
        ("Flyby", "flyby"),
        ("SpawnDirectedObject", "directed_object"),
    ] {
        let h = host.clone();
        let k = kind;
        b.real(name, lua.create_function(move |_, args: MultiValue| {
            // Pull the first three numeric args as the spawn position (common leading-arg shape).
            // Lua integers and floats both count (as_f32 only matches floats).
            let nums: Vec<f32> = args
                .iter()
                .filter_map(|v| v.as_f32().or_else(|| v.as_i64().map(|i| i as f32)))
                .collect();
            let pos = [
                nums.first().copied().unwrap_or(0.0),
                nums.get(1).copied().unwrap_or(0.0),
                nums.get(2).copied().unwrap_or(0.0),
            ];
            h.borrow_mut().airstrike_spawn(k, pos);
            Ok(())
        })?)?;
    }

    b.install_global(GLOBAL)
}
