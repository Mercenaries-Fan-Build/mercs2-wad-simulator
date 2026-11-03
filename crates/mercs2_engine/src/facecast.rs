//! Who speaks each FaceFX anim set, as the shipped scripts bind it.
//!
//! A set names no actor (every retail archive says `NewActor`): `Animation.BindFaceAnimSet(guid, set)`
//! binds it to an entity, and that entity's template names the actor in `FaceFXActor`
//! ([`KitTemplate::face_actor`]). So who a set belongs to is in the tables the briefing scripts read,
//! and this runs those tables on the game's Lua VM:
//!
//! * `WifBriefingData.<mission>.tFaceAnimSets`: `Player1[<hero>]` is that hero's set, `Starter` the
//!   mission's starter's (`WifMissionData.tMissionData[<mission>].sStarter`; the mission's own
//!   `tActors.Starter` replaces the starter's, as `MrxStarter` merges them).
//! * `WifStarterData.<starter>.sFaceFxSet`: that starter's `tActors.Starter.sTemplate`, or, for the
//!   PMC staff, the first rider of `WifPmcInterior._tStarters[<starter>].sSourceObject`
//!   (`Vehicle.GetRiders(source)[1]`), an object placed in the starter's layer: its `Transform`'s
//!   `Parent` is the template that seats the rider.
//! * `WifPmcInterior._tAssetPreload.facefxanimationset[<hero>]`: the hero's own sets.
//! * `hijack_mi35_solano`: `tFaceAnimSets.ActorOne` is the hero's, `ActorTwo` the hijackee's, and the
//!   hijackee's actor is the `facefxactor` the script loads.
//!
//! A hero is the model of its `Original` outfit (`WifPmcInterior._tOutfits`). Every other module the
//! scripts import, and every engine namespace, is an inert stand-in: only data is read.

use std::collections::BTreeMap;

use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::mixr::KitTemplate;

use crate::script::mercs2_luac::rt::{Function, Table};
use crate::script::ScriptHost;

/// The modules that run as shipped.
pub const MODULES: &[&str] = &["WifBriefingData", "WifStarterData", "WifMissionData", "WifPmcInterior", "hijack_mi35_solano"];

/// The stand-ins, and `__cast()`: one row per binding the scripts make.
const GLUE: &str = r##"
print = function() end
local PROXY = {}
local function proxy() return setmetatable({}, PROXY) end
PROXY.__index = function(p, k)
  local s = proxy()
  rawset(p, k, s)
  return s
end
PROXY.__call = function() return proxy() end

local real_import = import
import = function(sName)
  if __REAL[string.lower(sName)] then return real_import(sName) end
  local m = rawget(_G, sName)
  if m == nil then
    m = proxy()
    rawset(_G, sName, m)
  end
  return m
end
setmetatable(_G, {__index = function(_, k)
  if type(k) == "string" and string.match(k, "^%u") then
    local p = proxy()
    rawset(_G, k, p)
    return p
  end
end})

function __cast()
  local B = import("WifBriefingData")
  local S = import("WifStarterData")
  local M = import("WifMissionData")
  local P = import("WifPmcInterior")
  local rows = {}
  local function add(set, kind, value, via, layer)
    rows[#rows + 1] = {set = set, kind = kind, value = value, via = via, layer = layer}
  end
  local function hero(sHero, set, via)
    add(set, "model", P._tOutfits[string.lower(sHero)][1].Model, sHero .. " in " .. via)
  end
  local function starter(id, tBriefing, set, via)
    local tOver = tBriefing and rawget(tBriefing, "tActors") and rawget(tBriefing.tActors, "Starter")
    local d = id and rawget(S, id)
    local tActors = d and rawget(d, "tActors")
    local tActor = tOver or (tActors and rawget(tActors, "Starter"))
    if tActor then return add(set, "template", tActor.sTemplate, id .. " in " .. via) end
    local tPmc = id and rawget(P._tStarters, id)
    if tPmc then
      local sLayer = rawget(tPmc, "sLayerName") or P._GetStarterLayers()[1]
      return add(set, "source", tPmc.sSourceObject, id .. " in " .. via, sLayer)
    end
    add(set, "unbound", tostring(id) .. " has no Starter actor", via)
  end
  for sMission, c in pairs(B) do
    local t = type(c) == "table" and rawget(c, "tFaceAnimSets")
    if t then
      for sActor, v in pairs(t) do
        if sActor == "Player1" then
          for sHero, set in pairs(v) do hero(sHero, set, "WifBriefingData." .. sMission) end
        elseif sActor == "Starter" then
          starter(M.tMissionData[sMission].sStarter, c, v, "WifBriefingData." .. sMission)
        else
          add(v, "unbound", "actor " .. sActor .. " of " .. sMission, "WifBriefingData")
        end
      end
    end
  end
  for id, d in pairs(S) do
    local set = type(d) == "table" and rawget(d, "sFaceFxSet")
    if set then starter(id, nil, set, "WifStarterData") end
  end
  for sHero, tSets in pairs(P._tAssetPreload.facefxanimationset) do
    for _, set in ipairs(tSets) do hero(sHero, set, "WifPmcInterior") end
  end
  local tActors = {}
  rawset(Pg, "LoadAsset", function(sName, sType)
    if sType == "facefxactor" then tActors[#tActors + 1] = sName end
  end)
  local H = import("hijack_mi35_solano")
  H.Init()
  for sHero in pairs(P._tAssetPreload.facefxanimationset) do
    rawset(MrxUtil, "GetPrimaryCharacterName", function() return sHero end)
    -- The table is set early in Initialize; what follows drives stand-ins and may not finish.
    pcall(H.Initialize, H, {}, "hijacker", "hijackee", "seat", "vehicle")
    local t = rawget(H, "tFaceAnimSets") or rawget(_G, "tFaceAnimSets")
    hero(sHero, t.ActorOne.ActorAnim, "hijack_mi35_solano")
    for _, sActor in ipairs(tActors) do
      add(t.ActorTwo.ActorAnim, "actor", sActor, "hijack_mi35_solano")
    end
  end
  return rows
end
"##;

/// Who speaks a set: the template, its model and its actor, and which script table says so.
#[derive(Debug, Clone, PartialEq)]
pub struct Speaker {
    pub template: String,
    pub model: u32,
    pub actor: u32,
    pub via: String,
}

/// Every set the scripts bind, by the set's asset name hash; a binding that leads to no face is the
/// reason why.
#[derive(Debug, Default)]
pub struct Cast {
    pub sets: BTreeMap<u32, Result<Speaker, String>>,
}

impl Cast {
    /// Run the scripts (`fetch` gives a module's LuaQ bytecode by its lower-case name) and resolve
    /// each binding against the world's templates; `layer` gives a placement layer's block by name.
    pub fn read(
        fetch: impl Fn(&str) -> Result<Vec<u8>, String> + 'static,
        mut layer: impl FnMut(&str) -> Result<Vec<u8>, String>,
        templates: &[KitTemplate],
    ) -> Result<Cast, String> {
        let host = ScriptHost::with_module_source(move |name| fetch(&name.to_ascii_lowercase())).map_err(|e| format!("script host: {e}"))?;
        let lua = host.lua().clone();
        let g = lua.globals();
        let real = lua.create_table().map_err(|e| e.to_string())?;
        for name in MODULES {
            real.set(name.to_ascii_lowercase(), true).map_err(|e| e.to_string())?;
        }
        g.set("__REAL", real).map_err(|e| e.to_string())?;
        lua.load(GLUE).set_name("@facecast_glue").exec().map_err(|e| format!("glue: {e}"))?;
        let cast: Function = g.get("__cast").map_err(|e| e.to_string())?;
        let rows: Table = cast.call(()).map_err(|e| format!("reading the scripts' FaceFX tables: {e}"))?;

        // Each layer once: its objects' keys by name, and their `Parent` template keys. An object
        // written `name 0xKEY` is its key.
        type Layer = (BTreeMap<String, u32>, std::collections::HashMap<u32, u32>);
        let mut layers: BTreeMap<String, Result<Layer, String>> = BTreeMap::new();
        let mut placed = |name: &str, object: &str| -> Result<u32, String> {
            let read = layers.entry(name.to_string()).or_insert_with(|| {
                let block = layer(name)?;
                let places = mercs2_formats::placement::load_placements(&block)?;
                let keys = places.into_iter().filter_map(|p| Some((p.name?, p.key))).collect();
                Ok((keys, mercs2_formats::placement::transform_parents(&block)))
            });
            let (keys, parents) = read.as_ref().map_err(|e| format!("layer {name}: {e}"))?;
            let key = object.rsplit_once(" 0x").and_then(|(_, h)| u32::from_str_radix(h, 16).ok()).or_else(|| keys.get(object).copied());
            key.and_then(|k| parents.get(&k).copied()).ok_or_else(|| format!("layer {name} places no object {object:?} with a template"))
        };

        let mut out = Cast::default();
        for row in rows.sequence_values::<Table>() {
            let row = row.map_err(|e| e.to_string())?;
            let field = |k: &str| row.get::<String>(k).map_err(|e| format!("{k}: {e}"));
            let (set, kind, value, via) = (field("set")?, field("kind")?, field("value")?, field("via")?);
            let on_layer = row.get::<Option<String>>("layer").map_err(|e| format!("layer: {e}"))?;
            let speaker = speaker(&kind, &value, on_layer.as_deref(), &mut placed, templates)
                .map(|(t, model, actor)| Speaker { template: t, model, actor, via: via.clone() });
            let hash = pandemic_hash_m2(&set);
            let replaces = out.sets.get(&hash).is_some_and(|old| matches!(old, Err(_)) && speaker.is_ok());
            if replaces || !out.sets.contains_key(&hash) {
                out.sets.insert(hash, speaker.map_err(|e| format!("{set} ({via}): {e}")));
            }
        }
        Ok(out)
    }
}

/// The template a binding lands on: its name, model and actor. `placed` gives a placed object's
/// template key by layer and object name.
fn speaker(
    kind: &str,
    value: &str,
    layer: Option<&str>,
    placed: &mut dyn FnMut(&str, &str) -> Result<u32, String>,
    templates: &[KitTemplate],
) -> Result<(String, u32, u32), String> {
    let by_name = |n: &str| templates.iter().find(|t| t.name.as_deref() == Some(n)).ok_or_else(|| format!("no template is named {n:?}"));
    let by_key = |k: u32| templates.iter().find(|t| t.key == k).ok_or_else(|| format!("template 0x{k:08X} has no model"));
    let t = match kind {
        "model" => {
            let m = pandemic_hash_m2(value);
            templates
                .iter()
                .find(|t| t.model == m && t.face_actor.is_some())
                .ok_or_else(|| format!("no template wearing {value} has a FaceFXActor"))?
        }
        "template" => by_name(value)?,
        // `Vehicle.GetRiders(source)[1]`: the first rider of the placed object's template.
        "source" => {
            let layer = layer.ok_or_else(|| format!("{value} names no layer"))?;
            let source = by_key(placed(layer, value)?)?;
            let rider = *source.riders.first().ok_or_else(|| format!("template {} seats no rider", source.label()))?;
            by_key(rider)?
        }
        "actor" => {
            let a = pandemic_hash_m2(value);
            templates.iter().find(|t| t.face_actor == Some(a)).ok_or_else(|| format!("no template has FaceFXActor {value}"))?
        }
        _ => return Err(value.to_string()),
    };
    let name = t.label();
    let actor = t.face_actor.ok_or_else(|| format!("template {name} has no FaceFXActor, so the game binds no face to it"))?;
    Ok((name, t.model, actor))
}

/// The template that shows actor `actor`: the one that is named by the same hash as its model when
/// there is one, else the first in key order.
pub fn wearer(templates: &[KitTemplate], actor: u32) -> Option<&KitTemplate> {
    let wears = || templates.iter().filter(move |t| t.face_actor == Some(actor));
    wears().find(|t| t.model == actor).or_else(|| wears().next())
}

/// The LuaQ bytecode inside a script container.
pub fn script_luaq(container: &[u8]) -> Result<Vec<u8>, String> {
    let lay = mercs2_formats::scripts_block::parse_container(container)?;
    Ok(container[lay.luaq_off..lay.csum_off].to_vec())
}

/// The decompressed block of placement layer `layer`, by its `<layer>_P000` path, from the last of
/// `wads` that holds it (the WAD stack's last-mounted-wins).
pub fn layer_block(wads: &mut [crate::wad::Wad], layer: &str) -> Result<Vec<u8>, String> {
    let needle = format!("\\{}_p000", layer.to_ascii_lowercase());
    for w in wads.iter_mut().rev() {
        let at = crate::wad::block_paths(w).iter().position(|p| p.to_ascii_lowercase().contains(&needle));
        if let Some(bi) = at {
            return crate::wad::decompress_block_index(w, bi as u16);
        }
    }
    Err(format!("no block of layer {layer} in any open WAD"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(key: u32, name: &str, model: u32, face_actor: Option<u32>, riders: &[u32]) -> KitTemplate {
        KitTemplate { key, name: Some(name.into()), model, face_actor, riders: riders.to_vec(), ..Default::default() }
    }

    fn nothing_placed(layer: &str, object: &str) -> Result<u32, String> {
        Err(format!("{layer}: {object}"))
    }

    #[test]
    fn a_model_binding_finds_the_template_with_a_face() {
        let m = pandemic_hash_m2("pmc_hum_jen");
        let ts = [template(1, "Costume", m, None, &[]), template(2, "Jen", m, Some(0x76F2_5A90), &[])];
        assert_eq!(speaker("model", "pmc_hum_jen", None, &mut nothing_placed, &ts), Ok(("Jen".into(), m, 0x76F2_5A90)));
    }

    #[test]
    fn a_placed_source_speaks_through_its_templates_first_rider() {
        let ts = [template(1, "PmcHq_Interior", 10, None, &[2]), template(2, "Fiona Taylor", 20, Some(30), &[])];
        let mut placed = |layer: &str, object: &str| {
            assert_eq!((layer, object), ("Vz_State_PmcInterior", "HqInterior"));
            Ok(1)
        };
        assert_eq!(speaker("source", "HqInterior", Some("Vz_State_PmcInterior"), &mut placed, &ts), Ok(("Fiona Taylor".into(), 20, 30)));
    }

    #[test]
    fn a_template_with_no_face_says_so() {
        let ts = [template(1, "Patterson", 10, None, &[])];
        let e = speaker("template", "Patterson", None, &mut nothing_placed, &ts).unwrap_err();
        assert!(e.contains("no FaceFXActor"), "{e}");
        assert!(speaker("template", "Nobody", None, &mut nothing_placed, &ts).is_err());
        let unbound = speaker("unbound", "AllStarter9 has no Starter actor", None, &mut nothing_placed, &ts);
        assert_eq!(unbound, Err("AllStarter9 has no Starter actor".to_string()));
    }

    #[test]
    fn the_wearer_of_an_actor_prefers_the_model_of_the_same_name() {
        let ts = [template(1, "MattiasV2", 10, Some(30), &[]), template(2, "mattias", 30, Some(30), &[])];
        assert_eq!(wearer(&ts, 30).map(|t| t.key), Some(2));
        let ts = [template(1, "Jen", 10, Some(30), &[])];
        assert_eq!(wearer(&ts, 30).map(|t| t.key), Some(1));
        assert!(wearer(&ts, 31).is_none());
    }
}
