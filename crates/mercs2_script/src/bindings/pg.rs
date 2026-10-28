//! `Pg` engine binding namespace — luaL_Reg table VA 0x00b99328, 80 cfuncs.
//!
//! `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! To back this namespace: add real bindings inside [`install`] via `b.real(..)` (or
//! `b.stub(..)` for a deliberate faithful no-op), then `b.install_global("Pg")`. Nothing else in
//! the crate changes — the coverage harness (see `super`) picks up the delta automatically.

use mercs2_luac::rt::{Lua, MultiValue, Result as LuaResult, Table, Value};

use super::{Installed, NsBuilder, Required};
use crate::attribution::LuaFnExt;
use crate::{Guid, SharedHost, SpawnArgs, SpawnTemplate};

/// `lua_isnumber` + `lua_tonumber` (Lua 5.1: a numeric string converts).
fn lua_number(v: Option<&Value>) -> Option<f32> {
    match v? {
        Value::Number(n) => Some(*n),
        Value::String(s) => String::from_utf8(s.as_bytes()).ok()?.trim().parse().ok(),
        _ => None,
    }
}

/// A guid argument as `FUN_0059FF50` reads it: a lightuserdata; a full userdata is not modelled.
fn guid_arg(v: Option<&Value>) -> LuaResult<Option<u64>> {
    match v {
        Some(Value::LightUserData(_)) => Ok(Some(Guid::from_value(v.expect("matched")).raw())),
        Some(Value::Other(_, "userdata")) => {
            Err(mercs2_luac::rt::Error::RuntimeError("Pg.Spawn: a full-userdata guid is not modelled by this host".into()))
        }
        _ => Ok(None),
    }
}

/// Read `Pg.Spawn`'s arguments as `0x5D5D20` does (Xbox `0x82336B70`), with an index that moves only
/// past what it reads: a string template name, else a guid; three numbers of position
/// (`None` without them: the binding pushes nil); an optional number of yaw (0 otherwise); two
/// booleans, each also taking a nil (`FUN_0059F6D0` accepts type 0 or 1), defaulting to false and
/// true; an optional guid.
fn parse_spawn(args: &[Value]) -> LuaResult<Option<SpawnArgs>> {
    let mut i = 0;
    let template = match args.first() {
        Some(Value::String(s)) => {
            i += 1;
            SpawnTemplate::Name(
                String::from_utf8(s.as_bytes()).map_err(|e| mercs2_luac::rt::Error::RuntimeError(format!("Pg.Spawn: template name: {e}")))?,
            )
        }
        // `lua_isstring` takes a number too and converts it with the VM's number format, which is
        // not decoded; no shipped script passes one.
        Some(Value::Number(n)) => {
            return Err(mercs2_luac::rt::Error::RuntimeError(format!("Pg.Spawn: a number template ({n}) is not modelled")))
        }
        v => match guid_arg(v)? {
            Some(g) => {
                i += 1;
                SpawnTemplate::Guid(g)
            }
            None => SpawnTemplate::Guid(0),
        },
    };
    let (Some(x), Some(y), Some(z)) = (lua_number(args.get(i)), lua_number(args.get(i + 1)), lua_number(args.get(i + 2))) else {
        return Ok(None);
    };
    i += 3;
    let yaw = match lua_number(args.get(i)) {
        Some(v) => {
            i += 1;
            v
        }
        None => 0.0,
    };
    let mut flag = |default: bool| match args.get(i) {
        Some(Value::Boolean(b)) => {
            i += 1;
            *b
        }
        Some(Value::Nil) => {
            i += 1;
            false
        }
        _ => default,
    };
    let remove_alpha_animation = flag(false);
    let net = flag(true);
    let collision_source = guid_arg(args.get(i))?.unwrap_or(0);
    Ok(Some(SpawnArgs { template, pos: [x, y, z], yaw, remove_alpha_animation, net, collision_source }))
}

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
/// surface (regions, homing projectiles, heli waves, skirmish tables) is for later systems — note the
/// dev/asset-dump bindings that share the `Pg` global live in `pg_world` (table 0x00b99e28).
pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let h = host.clone();
    b.real(
        "GetGuidByName",
        // Tolerant of a nil/absent name (the game calls this with an unresolved cargo/marker template,
        // e.g. mrxsupportdelivery) → nil, matching the lenient engine rather than erroring on the arg.
        lua.create_function(move |_, name: Option<String>| {
            let guid = match name {
                Some(n) if !n.is_empty() => h.borrow_mut().guid_by_name(&n),
                _ => 0,
            };
            // A handle crosses as lightuserdata (retail tag 2); 0 stays nil (`crate::guid`).
            Ok::<Guid, mercs2_luac::rt::Error>(Guid(guid))
        })?,
    )?;

    let h = host.clone();
    // Pg.Spawn(template | guid, x, y, z, [yaw], [arg6], [arg7], [arg8]) -> guid | nil
    b.real(
        "Spawn",
        lua.create_function(move |_, args: MultiValue| {
            let args: Vec<Value> = args.into_iter().collect();
            let guid = match parse_spawn(&args)? {
                Some(spawn) => h.borrow_mut().pg_spawn(&spawn).map_err(mercs2_luac::rt::Error::RuntimeError)?,
                None => 0,
            };
            Ok::<Guid, mercs2_luac::rt::Error>(Guid(guid))
        })?,
    )?;

    // ===== Static-layer state queries (mrxlayermanager reads these as booleans). =====
    // No static-layer streaming state on the host yet → faithful "nothing loading / not static" so the
    // layer manager's add/remove guards run authentically instead of hitting a nil.
    // `Pg.AssetExists(name, type)` → true: the level's layers/assets exist (else MrxLayerManager culls
    // every layer as "does not exist" and the world never streams in).
    b.real("AssetExists", lua.create_function(|_, _: MultiValue| Ok(true))?)?;

    // The static-layer teardown flag (`[0x1175F58]`) and the layer record's static bit.
    let h = host.clone();
    b.real("UnloadingStaticLayers", lua.create_function(move |_, on: Option<bool>| {
        h.borrow_mut().set_unloading_static_layers(on.unwrap_or(false));
        Ok(())
    })?)?;
    let h = host.clone();
    b.real("GetUnloadingStaticLayers", lua.create_function(move |_, _: MultiValue| Ok(h.borrow().unloading_static_layers()))?)?;
    let h = host.clone();
    b.real("IsStaticLayer", lua.create_function(move |_, layer: String| Ok(h.borrow().is_static_layer(&layer)))?)?;
    // `Pg.LoadingStaticLayers` / `GetLoadingStaticLayers` stay a fixed false below: their flag's role
    // in the layer load is not decoded (`mrxtask.lua:312-315` only saves and restores it).

    for name in [
        "LoadingStaticLayers",
        "GetLoadingStaticLayers",
        "IsPointInBoundary",
        "LoadGame",
        "LoadIsRetry",
        "AchievementIsGranted",
    ] {
        b.real(name, lua.create_function(|_, _: MultiValue| Ok(false))?)?;
    }

    // `Pg.GetAllLandingZones(nSlot)` — the world's transit pads for **co-op player slot** `nSlot`,
    // keyed by **absolute landing-zone number**.
    //
    // The key is sparse, not a dense 1..n index: retail `vz` ships zones 1..8, 12, 15..18, 20..25,
    // 27..30 (23 of them, each with a pad per slot — measured off the shipped `layers_static` block,
    // not inferred). `MrxTransit.Reset` writes `_tLandingZones[nIndex]` straight from the iteration
    // key (`corpus/mercs2-luacd/src/resident/mrxtransit.lua:334-342`), `vz/wifhqdata.lua`'s
    // `nLandingZone` fields enumerate absolute numbers, and missions address them the same way
    // (`MrxTransit.SetLocationIsNuked(30, …)`, `vz/wifmissionflow.lua:1245`). Handing back a densely
    // packed table would silently renumber every pad.
    //
    // Values are object handles — `Reset` feeds each to `Object.GetLocalizedName`/`GetPosition` — so
    // they cross as lightuserdata like every other guid.
    let h = host.clone();
    b.real(
        "GetAllLandingZones",
        lua.create_function(move |lua, slot: Option<u32>| {
            let t = lua.create_table()?;
            for (zone, guid) in h.borrow().landing_zones(slot.unwrap_or(1)) {
                t.set(zone, crate::Guid(guid))?;
            }
            Ok::<Table, mercs2_luac::rt::Error>(t)
        })?,
    )?;

    // ===== Collection / query getters (the game reads the returned list). =====
    // No world-object index on the host yet → faithful empty list so the mission Lua's `for _,g in
    // tObjs` / `#tObjs` loops run with zero results instead of nil-indexing.
    for name in [
        "GetAllGuidsByName",
        "GetObjectsInArea",
        "GetAwakeObjects",
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
            lua.create_function(|_, _: MultiValue| Ok::<Guid, mercs2_luac::rt::Error>(Guid::NONE))?,
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
    // behavior by later world/AI systems; the game's Lua control flow runs unchanged here.
    // Layer/asset load, context actions, boundary + pursuit control, contract lifecycle, heli-wave/
    // skirmish spawners → recorded Pg commands the world/pursuit/streaming runtime drains.
    //
    // Layer load/unload/reload (`docs/reverse_engineer/layer_load_code_map.md` §2–3): the host makes
    // the request (a refused one returns nil and tracks nothing); an accepted one with a callback
    // enters the request tracker, which `__flush_layer_loads` polls each script update against the
    // layer record (`FUN_005A28F0`), firing `cb(sRequestType, sLayerName, "layer", bSuccess)` once the
    // request completes. The callback never runs inside the request call itself.
    // `Pg.LoadLayer(sName, bDynamic, fCallback, tCallbackData, bLoadingScreen)`;
    // `Pg.ReloadLayer` / `Pg.UnloadLayer(sName, fCallback, tCallbackData, bLoadingScreen)`
    // (`mrxlayermanager.lua:224-236`).
    for (name, req) in [("LoadLayer", "Load"), ("ReloadLayer", "Reload"), ("UnloadLayer", "Unload")] {
        let h = host.clone();
        b.real(name, lua.create_function(move |lua, args: mercs2_luac::rt::MultiValue| {
            use mercs2_luac::rt::Value;
            let args: Vec<Value> = args.into_iter().collect();
            let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Nil);
            let layer: String = match arg(0) {
                Value::String(s) => s.to_string_lossy(),
                v => return Err(mercs2_luac::rt::Error::RuntimeError(format!("Pg.{name}: layer name expected, got {}", v.type_name()))),
            };
            let (dynamic, first) = if req == "Load" { (matches!(arg(1), Value::Boolean(true)), 2) } else { (false, 1) };
            if !h.borrow_mut().layer_request(req, &layer, dynamic) {
                return Ok(Value::Nil);
            }
            if let Value::Function(cb) = arg(first) {
                let pend: mercs2_luac::rt::Table = match lua.globals().get("__pending_layer_loads") {
                    Ok(t) => t,
                    Err(_) => { let t = lua.create_table()?; lua.globals().set("__pending_layer_loads", t.clone())?; t }
                };
                let entry = lua.create_table()?;
                entry.set("cb", cb)?;
                entry.set("data", arg(first + 1))?;
                entry.set("req", req)?;
                entry.set("layer", layer)?;
                pend.push(entry)?;
            }
            Ok(Value::Boolean(true))
        })?)?;
    }
    // The tracker poll the resident pump runs each script update: fire each completed request's
    // callback, keep the pending ones.
    let h = host.clone();
    b.extra("__flush_layer_loads", lua.create_function(move |lua, ()| {
        let Ok(pend) = lua.globals().get::<mercs2_luac::rt::Table>("__pending_layer_loads") else { return Ok(()) };
        let entries: Vec<mercs2_luac::rt::Table> = pend.clone().sequence_values::<mercs2_luac::rt::Table>().collect::<Result<_, _>>()?;
        let mut done = Vec::new();
        let still = lua.create_table()?;
        for e in entries {
            let req: String = e.get("req")?;
            let layer: String = e.get("layer")?;
            match h.borrow().layer_request_status(&req, &layer) {
                Some(success) => done.push((e, req, layer, success)),
                None => still.push(e)?,
            }
        }
        lua.globals().set("__pending_layer_loads", still)?;
        // The callback receives its callback data unpacked, then (sLayerName, "layer", bSuccess):
        // `_LayerStatusChange(sRequestType, sLayerName, sLayerType, bSuccess)` is registered with
        // `{"Load"}` / `{"Reload"}` / `{"Unload"}` (`mrxlayermanager.lua:204-206, 245`).
        for (e, _req, layer, success) in done {
            let cb: mercs2_luac::rt::Function = e.get("cb")?;
            let mut cargs: Vec<mercs2_luac::rt::Value> = match e.get::<Option<Table>>("data")? {
                Some(t) => t.sequence_values::<mercs2_luac::rt::Value>().collect::<Result<_, _>>()?,
                None => Vec::new(),
            };
            cargs.push(mercs2_luac::rt::IntoLua::into_lua(layer, lua)?);
            cargs.push(mercs2_luac::rt::IntoLua::into_lua("layer", lua)?);
            cargs.push(mercs2_luac::rt::Value::Boolean(success));
            cb.call_attr::<()>(mercs2_luac::rt::Variadic::from_iter(cargs), "Pg.LayerLoadComplete")?;
        }
        Ok(())
    })?)?;

    super::record_all(&mut b, lua, host, "Pg", &[
        "ResetSingletonDone",
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
    ])?;

    b.install_global(GLOBAL)
}

#[cfg(test)]
mod spawn_parse_tests {
    use super::*;

    fn parse(lua: &Lua, src: &str) -> Option<SpawnArgs> {
        lua.globals().set("__g", Guid(0x8000_75E7)).unwrap();
        lua.globals().set("__s", Guid(0x4000_0010)).unwrap();
        let v: MultiValue = lua.load(&format!("return {src}")).eval().unwrap();
        parse_spawn(&v.into_iter().collect::<Vec<_>>()).unwrap()
    }

    /// The shipped call shapes: `MrxUtil.SpawnActor`'s `(t, 0,0,0, 0, false, true)`, a guid template
    /// with a collision source (`soldier.lua:37`), and `MrxPlayer.CreatePlayerCharacter`'s trailing
    /// `false` where the guid would be.
    #[test]
    fn reads_the_shipped_shapes() {
        let lua = Lua::new().unwrap();
        let a = parse(&lua, "\"Veyron\", 1, 2, 3, 90, false, true").unwrap();
        assert_eq!(a, SpawnArgs { template: SpawnTemplate::Name("Veyron".into()), pos: [1.0, 2.0, 3.0], yaw: 90.0, remove_alpha_animation: false, net: true, collision_source: 0 });
        let b = parse(&lua, "__g, 1, 2, 3, 0, false, true, __s").unwrap();
        assert_eq!((b.template, b.collision_source), (SpawnTemplate::Guid(0x8000_75E7), 0x4000_0010));
        let c = parse(&lua, "\"chris\", 1, 2, 3, 45, false, false, false").unwrap();
        assert_eq!((c.net, c.collision_source), (false, 0));
    }

    /// The yaw is optional and does not consume a non-number: `pmccon034.lua:293` passes
    /// `(t, x, y, z, false, true)`, which is yaw 0 with arg6 false and arg7 true.
    #[test]
    fn a_missing_yaw_does_not_move_the_index() {
        let lua = Lua::new().unwrap();
        let a = parse(&lua, "\"El Grande (Driver)\", 1, 2, 3, false, true").unwrap();
        assert_eq!((a.yaw, a.remove_alpha_animation, a.net), (0.0, false, true));
        let b = parse(&lua, "\"x\", 1, 2, 3").unwrap();
        assert_eq!((b.yaw, b.remove_alpha_animation, b.net), (0.0, false, true), "defaults");
        let c = parse(&lua, "\"x\", 1, 2, 3, 0, true").unwrap();
        assert!(c.remove_alpha_animation && c.net);
    }

    /// A nil where a boolean is read counts as false and is consumed (`FUN_0059F6D0` takes type 0).
    #[test]
    fn a_nil_flag_is_false() {
        let lua = Lua::new().unwrap();
        let a = parse(&lua, "\"x\", 1, 2, 3, 0, nil, nil").unwrap();
        assert_eq!((a.remove_alpha_animation, a.net), (false, false));
    }

    /// Without three numbers of position the binding pushes nil (`0x5D5DC0`).
    #[test]
    fn no_position_is_nil() {
        let lua = Lua::new().unwrap();
        assert!(parse(&lua, "\"x\", 1, 2").is_none());
        assert!(parse(&lua, "\"x\"").is_none());
    }
}
