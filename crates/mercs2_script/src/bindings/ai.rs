//! `Ai` engine binding namespace — luaL_Reg table VA 0x00b9a938, 66 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Ai")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult, Value};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Ai";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Ai";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b9a938;

pub const REQUIRED: &[Required] = &[
    Required { name: "Temp", corpus_calls: 0 },
    Required { name: "Goal", corpus_calls: 122 },
    Required { name: "DefaultGoal", corpus_calls: 1 },
    Required { name: "RemoveGoal", corpus_calls: 17 },
    Required { name: "Squad", corpus_calls: 9 },
    Required { name: "Role", corpus_calls: 14 },
    Required { name: "Anchor", corpus_calls: 10 },
    Required { name: "SetFacing", corpus_calls: 0 },
    Required { name: "LivingWorld", corpus_calls: 1 },
    Required { name: "Water", corpus_calls: 0 },
    Required { name: "Feed", corpus_calls: 0 },
    Required { name: "Rest", corpus_calls: 0 },
    Required { name: "Talk", corpus_calls: 0 },
    Required { name: "Admire", corpus_calls: 0 },
    Required { name: "Enable", corpus_calls: 5 },
    Required { name: "Deliver", corpus_calls: 3 },
    Required { name: "HeliLand", corpus_calls: 0 },
    Required { name: "HeliTakeoff", corpus_calls: 0 },
    Required { name: "HeliDropZoneInfo", corpus_calls: 0 },
    Required { name: "GoIn", corpus_calls: 0 },
    Required { name: "EveryoneOut", corpus_calls: 0 },
    Required { name: "Deploy", corpus_calls: 7 },
    Required { name: "TestDropZone", corpus_calls: 6 },
    Required { name: "SetHaste", corpus_calls: 6 },
    Required { name: "GetPerceivability", corpus_calls: 0 },
    Required { name: "SetPerceivability", corpus_calls: 0 },
    Required { name: "PlanSetConditions", corpus_calls: 0 },
    Required { name: "PlanSetGoal", corpus_calls: 0 },
    Required { name: "Plan", corpus_calls: 0 },
    Required { name: "PlanIterate", corpus_calls: 0 },
    Required { name: "PlanClear", corpus_calls: 0 },
    Required { name: "SetTrafficSpawning", corpus_calls: 0 },
    Required { name: "SetSidewalkSpawning", corpus_calls: 0 },
    Required { name: "SetRoadSpawning", corpus_calls: 0 },
    Required { name: "SetLaneActive", corpus_calls: 3 },
    Required { name: "TweakAttachedSpawners", corpus_calls: 31 },
    Required { name: "TweakAttachedSpawnersInGroup", corpus_calls: 14 },
    Required { name: "ShowObjectSpawners", corpus_calls: 0 },
    Required { name: "GetSpawnList", corpus_calls: 0 },
    Required { name: "GetSpawnListChangeInfo", corpus_calls: 0 },
    Required { name: "SetSpawnList", corpus_calls: 0 },
    Required { name: "ClearSpawnListChanges", corpus_calls: 0 },
    Required { name: "ResetAllSpawnLists", corpus_calls: 0 },
    Required { name: "SetExclusionZone", corpus_calls: 0 },
    Required { name: "AddRoadException", corpus_calls: 0 },
    Required { name: "RemoveRoadException", corpus_calls: 0 },
    Required { name: "RemoveExclusionZone", corpus_calls: 2 },
    Required { name: "AddSubject", corpus_calls: 2 },
    Required { name: "RemoveSubject", corpus_calls: 2 },
    Required { name: "RemoveAllSubjects", corpus_calls: 0 },
    Required { name: "GetSubjectData", corpus_calls: 0 },
    Required { name: "ThreatPerception", corpus_calls: 0 },
    Required { name: "GetAttrib", corpus_calls: 0 },
    Required { name: "GetState", corpus_calls: 1 },
    Required { name: "SetState", corpus_calls: 6 },
    Required { name: "GetFeeling", corpus_calls: 2 },
    Required { name: "SetFeeling", corpus_calls: 1 },
    Required { name: "GetRelation", corpus_calls: 10 },
    Required { name: "SetRelation", corpus_calls: 13 },
    Required { name: "ChangeRelation", corpus_calls: 0 },
    Required { name: "GetFactionGuid", corpus_calls: 2 },
    Required { name: "AddInfraction", corpus_calls: 8 },
    Required { name: "SetInfractionMultiplier", corpus_calls: 6 },
    Required { name: "SetAttitude", corpus_calls: 0 },
    Required { name: "SetDriveThroughMassRatio", corpus_calls: 0 },
    Required { name: "SetPriorityTarget", corpus_calls: 2 },
];

/// Boot slice: only `Ai.Enable` is wired, as a no-op — it's the one `Ai.*` cfunc the animate-actor
/// branch of `_SpawnActorComplete` touches. The other 65 (goals, squads, plans, spawner tweaks,
/// faction relation/mood) are for later silos.
/// The `Ai.*` order surface: goals/relations/state, forwarded to the AI mechanism through
/// [`crate::EngineHost`] (the real host backs it with `mercs2_ai::AiWorld` — the recovered action ring
/// + relation matrix + `AiBehavior` flags; AI code map §8). The *goal vocabulary* is authored data, so
/// these post/set the mechanism rather than running a compiled planner. The planner/cover/squad
/// orchestration cfuncs (Plan*, Deploy, HeliLand…) stay a later pass.
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    // Ai.Goal(guid, "verb")  OR  Ai.Goal{AIGuid=guid, Goal='verb', ...}  (code map §5 table form).
    let h = host.clone();
    b.real("Goal", lua.create_function(move |_, (first, maybe_goal): (Value, Option<String>)| {
        let (guid, goal): (i64, String) = match first {
            Value::Table(t) => (
                t.get::<i64>("AIGuid").or_else(|_| t.get::<i64>("Guid")).unwrap_or(0),
                t.get::<String>("Goal").unwrap_or_default(),
            ),
            Value::Integer(i) => (i, maybe_goal.unwrap_or_default()),
            Value::Number(n) => (n as i64, maybe_goal.unwrap_or_default()),
            _ => (0, maybe_goal.unwrap_or_default()),
        };
        Ok(h.borrow_mut().ai_goal(guid as u64, &goal))
    })?)?;

    let h = host.clone();
    b.real("SetRelation", lua.create_function(move |_, (from, to, value): (i64, i64, i64)| {
        h.borrow_mut().ai_set_relation(from as u64, to as u64, value);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetRelation", lua.create_function(move |_, (from, to): (i64, i64)| {
        Ok(h.borrow().ai_get_relation(from as u64, to as u64))
    })?)?;
    let h = host.clone();
    b.real("SetState", lua.create_function(move |_, (guid, state, on): (i64, String, Option<bool>)| {
        Ok(h.borrow_mut().ai_set_state(guid as u64, &state, on.unwrap_or(true)))
    })?)?;

    b.stub("Enable", lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    b.install_global(GLOBAL)
}
