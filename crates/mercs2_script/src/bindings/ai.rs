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

/// Extract an AI actor guid from an order argument: a bare integer, or a `{AIGuid=…}`/`{Guid=…}` table
/// (code map §5 order-table form). `0` when absent.
fn guid_of(v: &Value) -> i64 {
    match v {
        Value::Integer(i) => *i,
        Value::Number(n) => *n as i64,
        Value::Table(t) => t.get::<i64>("AIGuid").or_else(|_| t.get::<i64>("Guid")).unwrap_or(0),
        _ => 0,
    }
}

/// Pull the spawner-adjust fields the host cares about out of a `TweakAttachedSpawners` options table:
/// `(SpawnerState, forceRespawn)`. `ForceRespawn`/`Respawn` truthy ⇒ force an immediate respawn.
fn spawner_opts(opts: &Option<mlua::Table>) -> (Option<String>, bool) {
    match opts {
        Some(t) => {
            let state = t.get::<String>("SpawnerState").ok();
            let respawn = t.get::<bool>("ForceRespawn").or_else(|_| t.get::<bool>("Respawn")).unwrap_or(false);
            (state, respawn)
        }
        None => (None, false),
    }
}

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

/// The `Ai.*` surface, wired to the recovered mechanisms through [`crate::EngineHost`]:
/// - goals/orders (`Goal`/`Role`/`Anchor`/`Squad`/`Deploy`/`Plan*`/…) → the `mercs2_ai::AiWorld`
///   1024-slot action ring (`DirectAction`), hash-addressed by verb — the engine owns the ring, the
///   goal vocabulary is authored data (AI code map §5/§8), so posting the verb IS the faithful body;
/// - relations/mood (`SetRelation`/`GetRelation`/`AddInfraction`/`SetInfractionMultiplier`/
///   `SetAttitude`/`ChangeRelation`) → `mercs2_faction::FactionWorld` (the mood bridge + `[-100,100]`
///   relation model that drives price/pursuit/attitude);
/// - living-world spawners (`TweakAttachedSpawners`/`…InGroup`) → `mercs2_population::PopulationWorld`.
/// The UNBACKED residue (perception subjects, spawn-list channels, exclusion zones, road/lane spawning)
/// no-ops honestly and is tracked in `docs/modernization/binding_burndown.md`.
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

    // Relation get/set tolerate a nil faction guid (faction-manager setup queries relations for
    // factions that aren't resolved yet) → neutral 0 / no-op, matching the lenient engine.
    let h = host.clone();
    b.real("SetRelation", lua.create_function(move |_, (from, to, value): (Option<i64>, Option<i64>, i64)| {
        if let (Some(f), Some(t)) = (from, to) {
            h.borrow_mut().ai_set_relation(f as u64, t as u64, value);
        }
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetRelation", lua.create_function(move |_, (from, to): (Option<i64>, Option<i64>)| {
        Ok(match (from, to) {
            (Some(f), Some(t)) => h.borrow().ai_get_relation(f as u64, t as u64),
            _ => 0,
        })
    })?)?;
    let h = host.clone();
    b.real("SetState", lua.create_function(move |_, (guid, state, on): (i64, String, Option<bool>)| {
        Ok(h.borrow_mut().ai_set_state(guid as u64, &state, on.unwrap_or(true)))
    })?)?;

    // Ai.DefaultGoal(tParameters) — the table-form goal post (mrxai.lua:19). Faithfully routes to the
    // same action ring as Ai.Goal (there is no separate compiled "default goal" body; AI code map §5/§8).
    let h = host.clone();
    b.real("DefaultGoal", lua.create_function(move |_, params: Value| {
        let (guid, goal): (i64, String) = match params {
            Value::Table(t) => (
                t.get::<i64>("AIGuid").or_else(|_| t.get::<i64>("Guid")).unwrap_or(0),
                t.get::<String>("Goal").unwrap_or_default(),
            ),
            _ => (0, String::new()),
        };
        Ok(h.borrow_mut().ai_goal(guid as u64, &goal))
    })?)?;

    // --- Order verbs → the recovered 1024-slot action ring (`ai_order` → `AiWorld::order`). ---
    // Every `Ai.*` order directive (goal-adjacent: role/anchor/squad/deploy/haste/heli/…) posts its
    // hash-addressed verb to the ring the (data/Lua) brain consumes — the engine-owned mechanism, NOT a
    // no-op (AI code map §5/§8). Each accepts a bare guid OR a `{AIGuid=…}` table (code map §5 form).
    for verb in [
        "Temp", "RemoveGoal", "Squad", "Role", "Anchor", "SetFacing", "Water", "Feed", "Rest", "Talk",
        "Admire", "Enable", "Deliver", "HeliLand", "HeliTakeoff", "GoIn", "EveryoneOut", "Deploy",
        "SetHaste", "SetPriorityTarget", "PlanSetConditions", "PlanSetGoal", "Plan", "PlanIterate",
        "PlanClear",
    ] {
        let h = host.clone();
        let verb_name = verb;
        b.real(verb, lua.create_function(move |_, first: Value| {
            let guid = guid_of(&first);
            Ok(h.borrow_mut().ai_order(guid as u64, verb_name))
        })?)?;
    }

    // --- Faction/reputation → the recovered mood bridge (`mercs2_faction::FactionWorld`). ---
    // Ai.AddInfraction(offender, faction, amount).
    let h = host.clone();
    b.real("AddInfraction", lua.create_function(move |_, (offender, faction, amount): (i64, i64, Option<i64>)| {
        h.borrow_mut().ai_add_infraction(offender as u64, faction as u64, amount.unwrap_or(0));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("SetInfractionMultiplier", lua.create_function(move |_, (faction, mult): (i64, i64)| {
        h.borrow_mut().ai_set_infraction_multiplier(faction as u64, mult);
        Ok(())
    })?)?;
    // Ai.SetAttitude/ChangeRelation(faction, toward, value) — directed relation write (drives price/pursuit).
    let h = host.clone();
    b.real("SetAttitude", lua.create_function(move |_, (faction, toward, value): (i64, i64, i64)| {
        h.borrow_mut().ai_set_attitude(faction as u64, toward as u64, value);
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("ChangeRelation", lua.create_function(move |_, (faction, toward, value): (i64, i64, i64)| {
        h.borrow_mut().ai_set_attitude(faction as u64, toward as u64, value);
        Ok(())
    })?)?;

    // --- Living-world spawners → the population manager (`PopulationWorld::tweak_attached_spawners`). ---
    // Ai.TweakAttachedSpawners(target, {SpawnerState="on"/"off", SecondsPerCycle=…, …}) — apply an adjust
    // to all groups; the InGroup form scopes to a single group via `Group`/`GroupIndex`.
    let h = host.clone();
    b.real("TweakAttachedSpawners", lua.create_function(move |_, (target, opts): (i64, Option<mlua::Table>)| {
        let (state, respawn) = spawner_opts(&opts);
        Ok(h.borrow_mut().ai_tweak_spawners(target as u64, 0xFF, state.as_deref(), respawn))
    })?)?;
    let h = host.clone();
    b.real("TweakAttachedSpawnersInGroup", lua.create_function(move |_, (target, group, opts): (i64, i64, Option<mlua::Table>)| {
        let (state, respawn) = spawner_opts(&opts);
        let mask = 1u8.checked_shl(group as u32).unwrap_or(0);
        Ok(h.borrow_mut().ai_tweak_spawners(target as u64, mask, state.as_deref(), respawn))
    })?)?;

    // --- Getters the game reads → real-state defaults (see burn-down: perception/subject/spawn-list
    // models are not built yet, so these read the neutral value the game reads when unset). ---
    b.real("GetFeeling", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetPerceivability", lua.create_function(|_, _: MultiValue| Ok(0i64))?)?;
    b.real("GetState", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    b.real("TestDropZone", lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    // GUID getter: 0 → nil so the game's `if not uFaction` control flow is authentic.
    b.real("GetFactionGuid", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("GetAttrib", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("GetSubjectData", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("GetSpawnList", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("GetSpawnListChangeInfo", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;
    b.real("HeliDropZoneInfo", lua.create_function(|_, _: MultiValue| Ok(Value::Nil))?)?;

    // --- UNBACKED residue → honest no-ops (NOT "faithful"): these want a perception-subject list, a
    // spawn-list channel model, exclusion zones, or road/lane spawning toggles the engine does not have
    // yet. Tracked in docs/modernization/binding_burndown.md — de-stub as those systems land. ---
    super::record_all(&mut b, lua, host, "Ai", &[
        "LivingWorld", "SetPerceivability", "SetTrafficSpawning", "SetSidewalkSpawning",
        "SetRoadSpawning", "SetLaneActive", "ShowObjectSpawners", "SetSpawnList",
        "ClearSpawnListChanges", "ResetAllSpawnLists", "SetExclusionZone", "AddRoadException",
        "RemoveRoadException", "RemoveExclusionZone", "AddSubject", "RemoveSubject", "RemoveAllSubjects",
        "ThreatPerception", "SetFeeling", "SetDriveThroughMassRatio",
    ])?;

    b.install_global(GLOBAL)
}
