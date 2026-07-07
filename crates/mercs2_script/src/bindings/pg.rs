//! `Pg` engine binding namespace — luaL_Reg table VA 0x00b99328, 80 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! A later silo owns filling this file: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Pg")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mlua::{Lua, MultiValue, Result as LuaResult};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Pg";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Pg";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99328;

pub const REQUIRED: &[Required] = &[
    Required { name: "LoadingStaticLayers", corpus_calls: 2 },
    Required { name: "GetLoadingStaticLayers", corpus_calls: 1 },
    Required { name: "IsStaticLayer", corpus_calls: 3 },
    Required { name: "UnloadingStaticLayers", corpus_calls: 6 },
    Required { name: "GetUnloadingStaticLayers", corpus_calls: 3 },
    Required { name: "ResetSingletonDone", corpus_calls: 5 },
    Required { name: "LoadLayer", corpus_calls: 1 },
    Required { name: "UnloadLayer", corpus_calls: 2 },
    Required { name: "ReloadLayer", corpus_calls: 1 },
    Required { name: "AssetExists", corpus_calls: 1 },
    Required { name: "LoadAsset", corpus_calls: 36 },
    Required { name: "UnloadAsset", corpus_calls: 31 },
    Required { name: "ReloadAsset", corpus_calls: 0 },
    Required { name: "Spawn", corpus_calls: 130 },
    Required { name: "SpawnRelative", corpus_calls: 0 },
    Required { name: "SpawnFromCamera", corpus_calls: 13 },
    Required { name: "GetGuidByName", corpus_calls: 1240 },
    Required { name: "GetAllGuidsByName", corpus_calls: 0 },
    Required { name: "GetObjectsInArea", corpus_calls: 17 },
    Required { name: "GetAwakeObjects", corpus_calls: 2 },
    Required { name: "GetAllLandingZones", corpus_calls: 2 },
    Required { name: "FastCollectHelicopters", corpus_calls: 1 },
    Required { name: "FastCollectJets", corpus_calls: 0 },
    Required { name: "FastCollectFlying", corpus_calls: 1 },
    Required { name: "FastCollectTanks", corpus_calls: 3 },
    Required { name: "FastCollectCars", corpus_calls: 0 },
    Required { name: "FastCollectGroundVehicles", corpus_calls: 6 },
    Required { name: "FastCollectGroundVehiclesExceptTanks", corpus_calls: 0 },
    Required { name: "FastCollectHumans", corpus_calls: 11 },
    Required { name: "FastCollectBoats", corpus_calls: 0 },
    Required { name: "FastCollectUsables", corpus_calls: 0 },
    Required { name: "FastCollectProps", corpus_calls: 0 },
    Required { name: "FastCollectBuildings", corpus_calls: 4 },
    Required { name: "SpawnPlayer", corpus_calls: 0 },
    Required { name: "SpawnPlayerAdvanced", corpus_calls: 0 },
    Required { name: "AddContextAction", corpus_calls: 27 },
    Required { name: "RemoveContextAction", corpus_calls: 32 },
    Required { name: "FindPointFromCamera", corpus_calls: 48 },
    Required { name: "IsPointInBoundary", corpus_calls: 4 },
    Required { name: "GetLineRegionPoints", corpus_calls: 2 },
    Required { name: "SetBoundaryRadius", corpus_calls: 1 },
    Required { name: "GetBoundaryRadius", corpus_calls: 0 },
    Required { name: "SetWarningRadius", corpus_calls: 0 },
    Required { name: "GetWarningRadius", corpus_calls: 0 },
    Required { name: "GetTetherDiameterStart", corpus_calls: 2 },
    Required { name: "GetTetherDiameterEnd", corpus_calls: 0 },
    Required { name: "Rumble", corpus_calls: 5 },
    Required { name: "EnableRoad", corpus_calls: 0 },
    Required { name: "EnableIntersection", corpus_calls: 2 },
    Required { name: "SaveGame", corpus_calls: 3 },
    Required { name: "LoadGame", corpus_calls: 2 },
    Required { name: "ContractActivated", corpus_calls: 1 },
    Required { name: "ContractCancelled", corpus_calls: 1 },
    Required { name: "ContractCompleted", corpus_calls: 1 },
    Required { name: "LoadIsRetry", corpus_calls: 10 },
    Required { name: "GetDistantSpawnPointOnPath", corpus_calls: 3 },
    Required { name: "AchievementIsGranted", corpus_calls: 1 },
    Required { name: "AchievementAddCount", corpus_calls: 2 },
    Required { name: "LockPursuit", corpus_calls: 1 },
    Required { name: "ClearPursuitLock", corpus_calls: 1 },
    Required { name: "SetPursuit", corpus_calls: 1 },
    Required { name: "SetPursuitSeconds", corpus_calls: 1 },
    Required { name: "AdjustPursuitLevel", corpus_calls: 0 },
    Required { name: "AdjustPursuitTimer", corpus_calls: 0 },
    Required { name: "GetPursuitState", corpus_calls: 1 },
    Required { name: "RestrictAllPursuit", corpus_calls: 0 },
    Required { name: "RestrictPursuitFaction", corpus_calls: 0 },
    Required { name: "RestrictPursuitType", corpus_calls: 0 },
    Required { name: "SetMaxPursuitLevel", corpus_calls: 0 },
    Required { name: "SetMaxPursuitTime", corpus_calls: 0 },
    Required { name: "SetPursuitLevelTimes", corpus_calls: 1 },
    Required { name: "ClearPursuitRestrictions", corpus_calls: 1 },
    Required { name: "TweakPursuitParam", corpus_calls: 0 },
    Required { name: "SetCustomPursuit", corpus_calls: 3 },
    Required { name: "ClearCustomPursuit", corpus_calls: 2 },
    Required { name: "StartHeliWaveSpawner", corpus_calls: 1 },
    Required { name: "StopHeliWaveSpawner", corpus_calls: 1 },
    Required { name: "SetSkirmishTable", corpus_calls: 0 },
    Required { name: "AddSkirmishTemplate", corpus_calls: 0 },
    Required { name: "SetGlobalSkirmishState", corpus_calls: 0 },
];

/// Boot slice: `Pg.GetGuidByName` (name → runtime GUID; 0 → Lua `nil` so the game's `if not uGuid`
/// is authentic) and `Pg.Spawn` (the bottom-out of `MrxUtil.SpawnActor`). The rest of the `Pg` world
/// surface (regions, homing projectiles, heli waves, skirmish tables) is for later silos — note the
/// dev/asset-dump bindings that share the `Pg` global live in `pg_world` (table 0x00b99e28).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real(
        "GetGuidByName",
        lua.create_function(move |_, name: String| {
            let guid = h.borrow_mut().guid_by_name(&name);
            Ok::<Option<i64>, mlua::Error>((guid != 0).then_some(guid as i64))
        })?,
    )?;

    let h = host.clone();
    // Pg.Spawn(template, x, y, z, yaw, [bLink], [bHighDetail]) -> guid | nil
    b.real(
        "Spawn",
        lua.create_function(
            move |_,
                  (template, x, y, z, yaw, _link, high): (
                String,
                f32,
                f32,
                f32,
                f32,
                Option<bool>,
                Option<bool>,
            )| {
                let guid =
                    h.borrow_mut().pg_spawn(&template, [x, y, z], yaw, high.unwrap_or(false));
                Ok::<Option<i64>, mlua::Error>((guid != 0).then_some(guid as i64))
            },
        )?,
    )?;

    // ===== Static-layer state queries (mrxlayermanager reads these as booleans). =====
    // No static-layer streaming state on the host yet → faithful "nothing loading / not static" so the
    // layer manager's add/remove guards run authentically instead of hitting a nil.
    for name in [
        "LoadingStaticLayers",
        "GetLoadingStaticLayers",
        "IsStaticLayer",
        "UnloadingStaticLayers",
        "GetUnloadingStaticLayers",
        "AssetExists",
        "IsPointInBoundary",
        "LoadGame",
        "LoadIsRetry",
        "AchievementIsGranted",
    ] {
        b.real(name, lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    }

    // ===== Collection / query getters (the game reads the returned list). =====
    // No world-object index on the host yet → faithful empty list so the mission Lua's `for _,g in
    // tObjs` / `#tObjs` loops run with zero results instead of nil-indexing.
    for name in [
        "GetAllGuidsByName",
        "GetObjectsInArea",
        "GetAwakeObjects",
        "GetAllLandingZones",
        "FastCollectHelicopters",
        "FastCollectJets",
        "FastCollectFlying",
        "FastCollectTanks",
        "FastCollectCars",
        "FastCollectGroundVehicles",
        "FastCollectGroundVehiclesExceptTanks",
        "FastCollectHumans",
        "FastCollectBoats",
        "FastCollectUsables",
        "FastCollectProps",
        "FastCollectBuildings",
        "GetPursuitState",
    ] {
        b.real(name, lua.create_function(|lua, _: MultiValue| lua.create_table())?)?;
    }

    // ===== Scalar radius/tether getters (read as numbers). =====
    for name in [
        "GetBoundaryRadius",
        "GetWarningRadius",
        "GetTetherDiameterStart",
        "GetTetherDiameterEnd",
    ] {
        b.real(name, lua.create_function(|_, _: MultiValue| Ok(0.0f32))?)?;
    }

    // ===== Spawn-family cfuncs that return a guid (no camera/relative spawn seam yet → nil = "no
    // object", the same faithful convention as `Pg.Spawn` failing). =====
    for name in ["SpawnRelative", "SpawnFromCamera", "SpawnPlayer", "SpawnPlayerAdvanced"] {
        b.real(
            name,
            lua.create_function(|_, _: MultiValue| Ok::<Option<i64>, mlua::Error>(None))?,
        )?;
    }

    // Pg.FindPointFromCamera(dist, altitude, ...) -> x, y, z. No camera transform yet → origin.
    b.real(
        "FindPointFromCamera",
        lua.create_function(|_, _: MultiValue| Ok((0.0f32, 0.0f32, 0.0f32)))?,
    )?;
    // Pg.GetLineRegionPoints(region, invert) -> tX, tY (two coord lists) → two empty tables.
    b.real(
        "GetLineRegionPoints",
        lua.create_function(|lua, _: MultiValue| {
            Ok((lua.create_table()?, lua.create_table()?))
        })?,
    )?;
    // Pg.GetDistantSpawnPointOnPath(...) -> res, x, y, z, yaw. res=false → callers take the backup point.
    b.real(
        "GetDistantSpawnPointOnPath",
        lua.create_function(|_, _: MultiValue| Ok((false, 0.0f32, 0.0f32, 0.0f32, 0.0f32)))?,
    )?;

    // ===== Side-effect actions (return value the game ignores) → faithful no-ops. =====
    // Layer/asset streaming, context actions, region radii, rumble, roads, save/contract signals,
    // achievements, the whole pursuit-director surface, heli-wave + skirmish spawners. Wired to real
    // behavior by later world/AI silos; the game's Lua control flow runs unchanged here.
    for name in [
        "ResetSingletonDone",
        "LoadLayer",
        "UnloadLayer",
        "ReloadLayer",
        "LoadAsset",
        "UnloadAsset",
        "ReloadAsset",
        "AddContextAction",
        "RemoveContextAction",
        "SetBoundaryRadius",
        "SetWarningRadius",
        "Rumble",
        "EnableRoad",
        "EnableIntersection",
        "SaveGame",
        "ContractActivated",
        "ContractCancelled",
        "ContractCompleted",
        "AchievementAddCount",
        "LockPursuit",
        "ClearPursuitLock",
        "SetPursuit",
        "SetPursuitSeconds",
        "AdjustPursuitLevel",
        "AdjustPursuitTimer",
        "RestrictAllPursuit",
        "RestrictPursuitFaction",
        "RestrictPursuitType",
        "SetMaxPursuitLevel",
        "SetMaxPursuitTime",
        "SetPursuitLevelTimes",
        "ClearPursuitRestrictions",
        "TweakPursuitParam",
        "SetCustomPursuit",
        "ClearCustomPursuit",
        "StartHeliWaveSpawner",
        "StopHeliWaveSpawner",
        "SetSkirmishTable",
        "AddSkirmishTemplate",
        "SetGlobalSkirmishState",
    ] {
        b.stub(name, lua.create_function(|_, _: MultiValue| Ok(()))?)?;
    }

    b.install_global(GLOBAL)
}
