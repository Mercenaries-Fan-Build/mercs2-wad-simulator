//! The game's own mission rules, read from the shipped scripts: `WifMissionData` (missions,
//! milestones, starters), `MrxRewardData` (what each key pays) and `WifMissionFlow` (the flow
//! bindings that unlock missions and change the world when keys are awarded).
//!
//! The scripts run on the game's Lua VM out of the WADs. The modules whose data the rules need run
//! as shipped ([`REAL_MODULES`]); every other module, and every engine namespace, is a recorder: a
//! call through it is logged as a [`Call`] and changes nothing. Running a binding's consequence
//! therefore yields the exact list of calls the game would make, which `save_flow` maps onto the
//! save.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use mercs2_engine::script::mercs2_luac::rt::{Function, Table, Value, Variadic};
use mercs2_engine::script::ScriptHost;

use crate::app::WadStack;

/// Modules that run as shipped. The flow, mission and reward data plus the pure helpers they call
/// (`MrxUtil`, starter and HQ data for start locations, `WifHints` for its hint table,
/// `MrxFactionManager` for each faction's starting relation).
pub const REAL_MODULES: &[&str] = &[
    "WifMissionData",
    "WifMissionFlow",
    "MrxMissionFlow",
    "MrxRewardData",
    "MrxUtil",
    "WifStarterData",
    "WifHqData",
    "WifHints",
    "MrxFactionManager",
];

/// `MrxMissionFlow` functions the bindings read and that stay as shipped; every other function in
/// it is replaced by a recorder (`UnlockMission`, `AwardKey`, `DestroyMission`, …).
const FLOW_KEEP: &[&str] =
    &["HasKey", "GetKeyValue", "GetMissionStartLocations", "GetBriefingStartLocations", "GetCaseSensitiveMissionId"];

/// One argument of a recorded call.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    /// A table: its array part, then its other string-keyed fields.
    Table { list: Vec<Arg>, map: BTreeMap<String, Arg> },
    /// A value handed out by a recorder (`MrxState.STATE_WAITFORGAME`, `GetHq("OilHq")`, …).
    Proxy(String),
    Func,
}

impl Arg {
    pub fn str(&self) -> Option<&str> {
        match self {
            Arg::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn num(&self) -> Option<f64> {
        match self {
            Arg::Num(n) => Some(*n),
            _ => None,
        }
    }
    pub fn bool(&self) -> Option<bool> {
        match self {
            Arg::Bool(b) => Some(*b),
            _ => None,
        }
    }
    /// The strings of a string-or-list argument (`MarkForAddition("a")` / `MarkForAddition({"a","b"})`).
    pub fn strings(&self) -> Vec<String> {
        match self {
            Arg::Str(s) => vec![s.clone()],
            Arg::Table { list, .. } => list.iter().filter_map(|a| a.str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    }
}

/// One call the game would make: `target` is `Module.Function`, `Namespace.Function`, or
/// `<object>.Method` for a method on an object a recorder handed out (`starter:PmcBoss.AddIntro`).
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub target: String,
    pub args: Vec<Arg>,
}

impl Call {
    pub fn arg(&self, i: usize) -> &Arg {
        self.args.get(i).unwrap_or(&Arg::Nil)
    }
}

/// `WifMissionData.tMissionData[id]`, after the module's `Init`.
#[derive(Debug, Clone)]
pub struct Mission {
    pub id: String,
    pub faction: Option<String>,
    pub starter: Option<String>,
    /// The starter has `WifStarterData`, so the mission waits at a briefing when unlocked.
    pub has_briefing: bool,
    /// `bContract` as `Init` sets it: `Some(true)` contract, `Some(false)` job.
    pub contract: Option<bool>,
    pub repeatable: bool,
    pub levels: Option<u32>,
    pub critical_path: bool,
    pub layers: Vec<String>,
    pub milestones: Vec<Milestone>,
}

impl Mission {
    pub fn is_job(&self) -> bool {
        self.contract == Some(false)
    }
    /// The string-table key `WifMissionData.GetMissionTitle` returns.
    pub fn title_key(&self) -> String {
        format!("[{}.Title]", self.id)
    }
    /// The world layers the mission loads: `tLayers`, or `Vz_State_<id>` (`mrxmissionflow.lua:249`).
    pub fn world_layers(&self) -> Vec<String> {
        if self.layers.is_empty() {
            vec![format!("Vz_State_{}", self.id)]
        } else {
            self.layers.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Milestone {
    pub at: u32,
    pub key: String,
}

/// `MrxRewardData._tRewards[key]`, after the module's `Init`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reward {
    pub cash: Option<f64>,
    pub fuel: Option<f64>,
    /// `{faction = change}` toward the PMC.
    pub attitude: Vec<(String, f64)>,
    /// `(id, faction)` shop unlocks.
    pub support: Vec<(String, String)>,
    /// `(id, faction)` silo unlocks.
    pub equipment: Vec<(String, String)>,
    /// `(id, quantity)`.
    pub stockpile: Vec<(String, f64)>,
}

/// The save state a binding's consequence can read while it runs.
pub struct FlowCtx<'a> {
    pub keys: &'a BTreeMap<String, f64>,
    /// Starters that exist (`MrxStarterManager.GetStarter` returns an object for these).
    pub starters: &'a BTreeSet<String>,
    pub costumes: f64,
}

struct Binding {
    name: String,
    prereq: Function,
    conseq: Option<Function>,
    recurring: bool,
}

pub struct GameRules {
    host: ScriptHost,
    calls: Rc<RefCell<Vec<Call>>>,
    flow: Table,
    factions: Table,
    bindings: Vec<Binding>,
    pub missions: BTreeMap<String, Mission>,
    pub rewards: HashMap<String, Reward>,
    /// `WifHints._tHints`: speaker → hint names.
    pub hints: BTreeMap<String, Vec<String>>,
}

/// Installed before any module runs. `__record(target, ...)` is the Rust log.
const GLUE: &str = r##"
local PROXY_MT = {}
local NAMES = setmetatable({}, {__mode = "k"})
__PROXY_MT = PROXY_MT

local function new_proxy(name)
  local p = setmetatable({}, PROXY_MT)
  NAMES[p] = name
  return p
end
__new_proxy = new_proxy

local function export(v)
  if type(v) == "table" and NAMES[v] then
    return {__proxy = NAMES[v]}
  end
  return v
end

local function unpackn(t)
  if type(t) ~= "table" then return end
  return unpack(t)
end

local function record(target, ...)
  local n = select("#", ...)
  local args = {...}
  for i = 1, n do args[i] = export(args[i]) end
  __record(target, n, unpack(args, 1, n))
end
__record_call = record

local tTeleportCallback
local SPECIAL = {}
__SPECIAL = SPECIAL

SPECIAL["MrxCheatBootstrap.IsSkipModeEnabled"] = function() return false end
SPECIAL["MrxUtil.GetCharacterIdentity"] = function() return nil end
SPECIAL["MrxState.Enter"] = function(nState, fEnter, tEnter, fReady, tReady)
  record("MrxState.Enter", nState)
  if type(fEnter) == "function" then fEnter(unpackn(tEnter)) end
  if type(fReady) == "function" then fReady(unpackn(tReady)) end
end
SPECIAL["MrxState.Exit"] = function(nState, fCallback, tData)
  record("MrxState.Exit", nState)
  if type(fCallback) == "function" then fCallback(unpackn(tData)) end
end
SPECIAL["Hud.Cinematic.Show"] = function(tArgs)
  record("Hud.Cinematic.Show", tArgs.sMovie)
  if type(tArgs.fCallback) == "function" then tArgs.fCallback(unpackn(tArgs.tCallbackData)) end
end
SPECIAL["MrxUtil.TeleportHeroesToLocations"] = function(tLocations, fCallback, tData)
  record("MrxUtil.TeleportHeroesToLocations", tLocations)
  if type(fCallback) == "function" then fCallback(unpackn(tData)) end
end
SPECIAL["MrxLayerManager.ProcessMarkedLayers"] = function(fCallback, tData)
  record("MrxLayerManager.ProcessMarkedLayers")
  if type(fCallback) == "function" then fCallback(unpackn(tData)) end
end
SPECIAL["WifPmcInterior.SetTeleportCallback"] = function(fCallback, tData)
  record("WifPmcInterior.SetTeleportCallback")
  tTeleportCallback = {fCallback, tData}
end
SPECIAL["WifPmcInterior.Enter"] = function(...)
  record("WifPmcInterior.Enter", ...)
  local t = tTeleportCallback
  tTeleportCallback = nil
  if t and type(t[1]) == "function" then t[1](unpackn(t[2])) end
end
SPECIAL["WifPmcInterior.GetAvailableCostumes"] = function() return __nCostumes end
SPECIAL["WifPmcInterior.SetAvailableCostumes"] = function(n)
  record("WifPmcInterior.SetAvailableCostumes", n)
  return {}
end
SPECIAL["MrxStarterManager.GetStarter"] = function(sName)
  if __tStarters[sName] then return new_proxy("starter:" .. sName) end
end
SPECIAL["MrxFactionManager.GetRelation"] = function(sSubject, sObject)
  local t = __tRelations[sSubject]
  local n = t and t[sObject]
  if type(n) ~= "number" then
    error("the save has no relation " .. tostring(sSubject) .. " -> " .. tostring(sObject))
  end
  return n
end
SPECIAL["Pg.GetGuidByName"] = function(sName)
  record("Pg.GetGuidByName", sName)
  local sHex = string.match(sName, "0x(%x+)$")
  if not sHex then return new_proxy("Pg.GetGuidByName(" .. sName .. ")") end
  return "0x" .. string.upper(sHex)
end

local function dispatch(target, ...)
  local f = SPECIAL[target]
  if f then return f(...) end
  record(target, ...)
  return new_proxy(target .. "()")
end
__dispatch = dispatch

PROXY_MT.__index = function(p, k)
  local sub = new_proxy(NAMES[p] .. "." .. tostring(k))
  rawset(p, k, sub)
  return sub
end
PROXY_MT.__call = function(p, ...)
  local name = NAMES[p]
  local first = ...
  if type(first) == "table" and NAMES[first] and string.sub(name, 1, #NAMES[first] + 1) == NAMES[first] .. "." then
    return dispatch(name, select(2, ...))
  end
  return dispatch(name, ...)
end

setmetatable(_G, {__index = function(_, k)
  if type(k) == "string" and string.match(k, "^%u") then
    local p = new_proxy(k)
    rawset(_G, k, p)
    return p
  end
end})

local real_import = import
import = function(sName)
  if __REAL[string.lower(sName)] then return real_import(sName) end
  local m = rawget(_G, sName)
  if m == nil then
    m = new_proxy(sName)
    rawset(_G, sName, m)
  end
  return m
end

-- Replace a real module's functions with recorders, keeping the names in `tKeep`.
function __record_module(sName, m, tKeep)
  local tFuncs = {}
  for k, v in pairs(m) do
    if type(v) == "function" and not (tKeep and tKeep[k]) then tFuncs[k] = true end
  end
  for k in pairs(tFuncs) do
    local target = sName .. "." .. k
    m[k] = function(...) return dispatch(target, ...) end
  end
end
"##;

impl GameRules {
    /// Load the rules from the open WAD stack.
    pub fn load(w: &mut WadStack) -> Result<GameRules, String> {
        let mut chunks = HashMap::new();
        for name in REAL_MODULES {
            chunks.insert(name.to_ascii_lowercase(), w.script_bytecode(&name.to_ascii_lowercase())?);
        }
        let host = ScriptHost::with_module_source(move |name| {
            chunks
                .get(&name.to_ascii_lowercase())
                .cloned()
                .ok_or_else(|| format!("`{name}` is not one of the modules the save editor runs"))
        })
        .map_err(|e| format!("script host: {e}"))?;
        let lua = host.lua().clone();
        let calls: Rc<RefCell<Vec<Call>>> = Rc::default();
        let log = calls.clone();
        let mt_slot: Rc<RefCell<Option<Table>>> = Rc::default();
        let mt_read = mt_slot.clone();
        let record = lua
            .create_function(move |_, mut args: Variadic<Value>| {
                let mut it = std::mem::take(&mut args.0).into_iter();
                let target = match it.next() {
                    Some(Value::String(s)) => s.to_string_lossy(),
                    _ => return Err(lua_err("__record: no target")),
                };
                let n = it.next().and_then(|v| v.as_i64()).unwrap_or(0) as usize;
                let mt = mt_read.borrow();
                let mut out: Vec<Arg> = it.take(n).map(|v| to_arg(&v, mt.as_ref(), 0)).collect();
                out.resize(n, Arg::Nil);
                log.borrow_mut().push(Call { target, args: out });
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        let g = lua.globals();
        g.set("__record", record).map_err(|e| e.to_string())?;
        let real = lua.create_table().map_err(|e| e.to_string())?;
        for name in REAL_MODULES {
            real.set(name.to_ascii_lowercase(), true).map_err(|e| e.to_string())?;
        }
        g.set("__REAL", real).map_err(|e| e.to_string())?;
        g.set("__tStarters", lua.create_table().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        g.set("__nCostumes", 1.0).map_err(|e| e.to_string())?;
        g.set("__tRelations", lua.create_table().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        lua.load(GLUE).set_name("@save_editor_glue").exec().map_err(|e| format!("glue: {e}"))?;
        *mt_slot.borrow_mut() = Some(g.get::<Table>("__PROXY_MT").map_err(|e| e.to_string())?);

        let flow = host.import_module("WifMissionFlow").map_err(|e| format!("WifMissionFlow: {e}"))?;
        let mission_data = host.import_module("WifMissionData").map_err(|e| format!("WifMissionData: {e}"))?;
        let reward_data = host.import_module("MrxRewardData").map_err(|e| format!("MrxRewardData: {e}"))?;
        let util = host.import_module("MrxUtil").map_err(|e| format!("MrxUtil: {e}"))?;
        let hints_mod = host.import_module("WifHints").map_err(|e| format!("WifHints: {e}"))?;
        let mmf = host.import_module("MrxMissionFlow").map_err(|e| format!("MrxMissionFlow: {e}"))?;

        let record_module: Function = g.get("__record_module").map_err(|e| e.to_string())?;
        let keep = lua.create_table().map_err(|e| e.to_string())?;
        for k in FLOW_KEEP {
            keep.set(*k, true).map_err(|e| e.to_string())?;
        }
        record_module.call::<()>(("MrxMissionFlow", mmf.clone(), keep)).map_err(|e| e.to_string())?;
        record_module.call::<()>(("WifHints", hints_mod.clone(), Value::Nil)).map_err(|e| e.to_string())?;
        let factions = host.import_module("MrxFactionManager").map_err(|e| format!("MrxFactionManager: {e}"))?;
        let keep = lua.create_table().map_err(|e| e.to_string())?;
        keep.set("GetAttitudeMedianValue", true).map_err(|e| e.to_string())?;
        record_module.call::<()>(("MrxFactionManager", factions.clone(), keep)).map_err(|e| e.to_string())?;
        let special: Table = g.get("__SPECIAL").map_err(|e| e.to_string())?;
        let get_relation: Function = special.get("MrxFactionManager.GetRelation").map_err(|e| e.to_string())?;
        factions.set("GetRelation", get_relation).map_err(|e| e.to_string())?;
        for k in ["TeleportHeroesToLocations", "GetCharacterIdentity"] {
            let f: Function = special.get(format!("MrxUtil.{k}")).map_err(|e| e.to_string())?;
            util.set(k, f).map_err(|e| e.to_string())?;
        }

        let starter_data = host.import_module("WifStarterData").map_err(|e| format!("WifStarterData: {e}"))?;
        let missions = read_missions(&mission_data, &starter_data)?;
        let rewards = read_rewards(&reward_data)?;
        let hints = read_hints(&hints_mod)?;
        let original: Function = flow.get("GetOriginalFlowData").map_err(|e| format!("GetOriginalFlowData: {e}"))?;
        let table: Table = original.call(()).map_err(|e| format!("GetOriginalFlowData(): {e}"))?;
        let mut bindings = Vec::new();
        for pair in table.pairs::<String, Table>() {
            let (name, b) = pair.map_err(|e| format!("flow binding: {e}"))?;
            let prereq = match b.get::<Value>("fPrereq").map_err(|e| e.to_string())? {
                Value::Function(f) => f,
                other => return Err(format!("flow binding `{name}`: fPrereq is a {}", other.type_name())),
            };
            let conseq = match b.get::<Value>("fConseq").map_err(|e| e.to_string())? {
                Value::Function(f) => Some(f),
                Value::Nil => None,
                other => return Err(format!("flow binding `{name}`: fConseq is a {}", other.type_name())),
            };
            let recurring = b.get::<Value>("bRecurring").map_err(|e| e.to_string())?.is_truthy();
            bindings.push(Binding { name, prereq, conseq, recurring });
        }
        bindings.sort_by(|a, b| a.name.cmp(&b.name));
        calls.borrow_mut().clear();
        Ok(GameRules { host, calls, flow: mmf, factions, bindings, missions, rewards, hints })
    }

    /// The relation toward the PMC that `SetAttitudeMutable(faction)` seeds on a faction the game
    /// lets change (`bDynamic`), from its `fGetInitialRelation` / `nInitialRelation`
    /// (`mrxfactionmanager.lua:486-500`). `relations` is the save's `tFactionData.tRelations`
    /// (subject → object → value). `None` for a faction whose attitude cannot change.
    pub fn initial_relation(
        &self,
        faction: &str,
        relations: &BTreeMap<String, BTreeMap<String, f64>>,
    ) -> Result<Option<f64>, String> {
        let lua = self.host.lua();
        let all: Table = self.factions.get("_tFactions").map_err(|e| format!("_tFactions: {e}"))?;
        let Some(data) = opt_table(&all, faction)? else {
            return Err(format!("MrxFactionManager has no faction `{faction}`"));
        };
        if opt_bool(&data, "bDynamic")? != Some(true) {
            return Ok(None);
        }
        let t = lua.create_table().map_err(|e| e.to_string())?;
        for (s, row) in relations {
            let r = lua.create_table().map_err(|e| e.to_string())?;
            for (o, v) in row {
                r.set(o.as_str(), *v).map_err(|e| e.to_string())?;
            }
            t.set(s.as_str(), r).map_err(|e| e.to_string())?;
        }
        lua.globals().set("__tRelations", t).map_err(|e| e.to_string())?;
        match data.get::<Value>("fGetInitialRelation").map_err(|e| e.to_string())? {
            Value::Function(f) => {
                let v: Value = f.call(()).map_err(|e| format!("{faction}.fGetInitialRelation: {e}"))?;
                v.as_f32().map(|n| Some(f64::from(n))).ok_or_else(|| format!("{faction}.fGetInitialRelation returned a {}", v.type_name()))
            }
            Value::Nil => opt_num(&data, "nInitialRelation")?
                .map(Some)
                .ok_or_else(|| format!("faction `{faction}` has neither fGetInitialRelation nor nInitialRelation")),
            other => Err(format!("{faction}.fGetInitialRelation is a {}", other.type_name())),
        }
    }

    /// Every flow binding by name, sorted.
    pub fn binding_names(&self) -> impl Iterator<Item = &str> {
        self.bindings.iter().map(|b| b.name.as_str())
    }

    /// A binding the game evaluates on `Refresh`: it has a consequence, and is culled once fired
    /// unless it recurs (`mrxmissionflow.lua:531-540`).
    pub fn is_live(&self, name: &str) -> bool {
        self.binding(name).map(|b| b.conseq.is_some()).unwrap_or(false)
    }

    pub fn is_recurring(&self, name: &str) -> bool {
        self.binding(name).map(|b| b.recurring).unwrap_or(false)
    }

    fn binding(&self, name: &str) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.name == name)
    }

    fn set_ctx(&self, ctx: &FlowCtx) -> Result<(), String> {
        let lua = self.host.lua();
        let keys = lua.create_table().map_err(|e| e.to_string())?;
        for (k, v) in ctx.keys {
            keys.set(k.as_str(), *v).map_err(|e| e.to_string())?;
        }
        self.flow.set("_tMyFlowData", keys).map_err(|e| e.to_string())?;
        let starters = lua.create_table().map_err(|e| e.to_string())?;
        for s in ctx.starters {
            starters.set(s.as_str(), true).map_err(|e| e.to_string())?;
        }
        let g = lua.globals();
        g.set("__tStarters", starters).map_err(|e| e.to_string())?;
        g.set("__nCostumes", ctx.costumes).map_err(|e| e.to_string())
    }

    /// Whether binding `name`'s `fPrereq` holds for `keys`.
    pub fn prereq_holds(&self, name: &str, keys: &BTreeMap<String, f64>) -> Result<bool, String> {
        let b = self.binding(name).ok_or_else(|| format!("no flow binding `{name}`"))?;
        let none = BTreeSet::new();
        self.set_ctx(&FlowCtx { keys, starters: &none, costumes: 1.0 })?;
        let v: Value = b.prereq.call(()).map_err(|e| format!("{name}.fPrereq: {e}"))?;
        Ok(v.is_truthy())
    }

    /// Run binding `name`'s `fConseq` against `ctx` and return every call it makes.
    pub fn consequence(&self, name: &str, ctx: &FlowCtx) -> Result<Vec<Call>, String> {
        let b = self.binding(name).ok_or_else(|| format!("no flow binding `{name}`"))?;
        let f = b.conseq.as_ref().ok_or_else(|| format!("flow binding `{name}` has no fConseq"))?;
        self.set_ctx(ctx)?;
        self.calls.borrow_mut().clear();
        f.call::<()>(()).map_err(|e| format!("{name}.fConseq: {e}"))?;
        Ok(std::mem::take(&mut *self.calls.borrow_mut()))
    }

    /// Where a mission's players spawn: `MrxMissionFlow.GetMissionStartLocations` as shipped
    /// (`mrxmissionflow.lua:411-421`). `None` when it has none.
    pub fn start_locations(&self, mission: &str) -> Result<Option<Vec<String>>, String> {
        let f: Function = self.flow.get("GetMissionStartLocations").map_err(|e| format!("GetMissionStartLocations: {e}"))?;
        match f.call::<Value>(mission.to_string()).map_err(|e| format!("GetMissionStartLocations({mission}): {e}"))? {
            Value::Table(t) => {
                let mut out = Vec::new();
                for v in t.sequence_values::<String>() {
                    out.push(v.map_err(|e| format!("GetMissionStartLocations({mission}): {e}"))?);
                }
                Ok(Some(out))
            }
            Value::Nil => Ok(None),
            other => Err(format!("GetMissionStartLocations({mission}) returned a {}", other.type_name())),
        }
    }

    /// The bindings whose consequence calls `UnlockMission(mission)` when run against `ctx`.
    pub fn unlockers_of(&self, mission: &str, ctx: &FlowCtx) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for b in &self.bindings {
            if b.conseq.is_none() {
                continue;
            }
            let calls = self.consequence(&b.name, ctx)?;
            if calls.iter().any(|c| c.target == "MrxMissionFlow.UnlockMission" && c.arg(0).str() == Some(mission)) {
                out.push(b.name.clone());
            }
        }
        Ok(out)
    }
}

fn lua_err(m: &str) -> mercs2_engine::script::mercs2_luac::rt::Error {
    mercs2_engine::script::mercs2_luac::rt::Error::RuntimeError(m.to_string())
}

fn to_arg(v: &Value, proxy_mt: Option<&Table>, depth: usize) -> Arg {
    match v {
        Value::Nil => Arg::Nil,
        Value::Boolean(b) => Arg::Bool(*b),
        Value::Number(_) => Arg::Num(v.as_f32().map(f64::from).unwrap_or(f64::NAN)),
        Value::String(s) => Arg::Str(s.to_string_lossy()),
        Value::Function(_) => Arg::Func,
        Value::Table(t) => {
            if let Ok(Value::String(name)) = t.raw_get::<Value>("__proxy") {
                return Arg::Proxy(name.to_string_lossy());
            }
            if proxy_mt.is_some() && t.get_metatable().as_ref() == proxy_mt {
                return Arg::Proxy(String::new());
            }
            if depth > 4 {
                return Arg::Table { list: Vec::new(), map: BTreeMap::new() };
            }
            let mut list = Vec::new();
            for v in t.sequence_values::<Value>().flatten() {
                list.push(to_arg(&v, proxy_mt, depth + 1));
            }
            let mut map = BTreeMap::new();
            for (k, v) in t.pairs::<Value, Value>().flatten() {
                if let Value::String(k) = k {
                    map.insert(k.to_string_lossy(), to_arg(&v, proxy_mt, depth + 1));
                }
            }
            Arg::Table { list, map }
        }
        _ => Arg::Proxy(v.type_name().to_string()),
    }
}

fn opt_str(t: &Table, k: &str) -> Result<Option<String>, String> {
    match t.get::<Value>(k).map_err(|e| e.to_string())? {
        Value::String(s) => Ok(Some(s.to_string_lossy())),
        Value::Nil => Ok(None),
        other => Err(format!("{k} is a {}", other.type_name())),
    }
}

fn opt_num(t: &Table, k: &str) -> Result<Option<f64>, String> {
    match t.get::<Value>(k).map_err(|e| e.to_string())? {
        v @ Value::Number(_) => Ok(v.as_f32().map(f64::from)),
        Value::Nil => Ok(None),
        other => Err(format!("{k} is a {}", other.type_name())),
    }
}

fn opt_bool(t: &Table, k: &str) -> Result<Option<bool>, String> {
    match t.get::<Value>(k).map_err(|e| e.to_string())? {
        Value::Boolean(b) => Ok(Some(b)),
        Value::Nil => Ok(None),
        other => Err(format!("{k} is a {}", other.type_name())),
    }
}

fn opt_table(t: &Table, k: &str) -> Result<Option<Table>, String> {
    match t.get::<Value>(k).map_err(|e| e.to_string())? {
        Value::Table(t) => Ok(Some(t)),
        Value::Nil => Ok(None),
        other => Err(format!("{k} is a {}", other.type_name())),
    }
}

fn read_missions(module: &Table, starters: &Table) -> Result<BTreeMap<String, Mission>, String> {
    let data: Table = module.get("tMissionData").map_err(|e| format!("tMissionData: {e}"))?;
    let mut out = BTreeMap::new();
    for pair in data.pairs::<String, Table>() {
        let (id, m) = pair.map_err(|e| format!("tMissionData: {e}"))?;
        let ctx = |e: String| format!("tMissionData.{id}: {e}");
        let starter = opt_str(&m, "sStarter").map_err(ctx)?;
        let has_briefing = match &starter {
            Some(s) => matches!(starters.get::<Value>(s.as_str()).map_err(|e| ctx(e.to_string()))?, Value::Table(_)),
            None => false,
        };
        let mut layers = Vec::new();
        if let Some(t) = opt_table(&m, "tLayers").map_err(ctx)? {
            for v in t.sequence_values::<String>() {
                layers.push(v.map_err(|e| ctx(e.to_string()))?);
            }
        }
        let mut milestones = Vec::new();
        if let Some(t) = opt_table(&m, "tMilestones").map_err(ctx)? {
            for ms in t.pairs::<Value, Table>() {
                let (_, ms) = ms.map_err(|e| ctx(e.to_string()))?;
                let key = opt_str(&ms, "sKey").map_err(ctx)?.ok_or_else(|| ctx("milestone without sKey".into()))?;
                let at = opt_num(&ms, "nMilestone").map_err(ctx)?.ok_or_else(|| ctx(format!("{key}: no nMilestone")))?;
                milestones.push(Milestone { at: at as u32, key });
            }
        }
        milestones.sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.key.cmp(&b.key)));
        let mission = Mission {
            faction: opt_str(&m, "sFactionId").map_err(ctx)?,
            has_briefing,
            starter,
            contract: opt_bool(&m, "bContract").map_err(ctx)?,
            repeatable: opt_bool(&m, "bRepeatable").map_err(ctx)?.unwrap_or(false),
            levels: opt_num(&m, "nLevels").map_err(ctx)?.map(|n| n as u32),
            critical_path: opt_bool(&m, "bCriticalPathMission").map_err(ctx)?.unwrap_or(false),
            layers,
            milestones,
            id: id.clone(),
        };
        out.insert(id, mission);
    }
    Ok(out)
}

/// `{id, faction, bHidden}` tuples as `MrxRewardData.Init` normalises them.
fn pairs_of(t: &Table, k: &str) -> Result<Vec<(String, Value)>, String> {
    let mut out = Vec::new();
    if let Some(list) = opt_table(t, k)? {
        for item in list.sequence_values::<Table>() {
            let item = item.map_err(|e| format!("{k}: {e}"))?;
            let id: String = item.get(1).map_err(|e| format!("{k}[1]: {e}"))?;
            let second: Value = item.get(2).map_err(|e| format!("{k}[2]: {e}"))?;
            out.push((id, second));
        }
    }
    Ok(out)
}

fn read_rewards(module: &Table) -> Result<HashMap<String, Reward>, String> {
    let data: Table = module.get("_tRewards").map_err(|e| format!("_tRewards: {e}"))?;
    let mut out = HashMap::new();
    for pair in data.pairs::<String, Table>() {
        let (key, r) = pair.map_err(|e| format!("_tRewards: {e}"))?;
        let ctx = |e: String| format!("_tRewards.{key}: {e}");
        let mut attitude = Vec::new();
        if let Some(t) = opt_table(&r, "tAttitude").map_err(ctx)? {
            for p in t.pairs::<String, Value>() {
                let (f, v) = p.map_err(|e| ctx(e.to_string()))?;
                attitude.push((f, v.as_f32().map(f64::from).ok_or_else(|| ctx("tAttitude value is not a number".into()))?));
            }
        }
        attitude.sort_by(|a, b| a.0.cmp(&b.0));
        let faction_of = |(id, v): (String, Value)| -> Result<(String, String), String> {
            match v {
                Value::String(s) => Ok((id, s.to_string_lossy())),
                other => Err(ctx(format!("{id}: faction is a {}", other.type_name()))),
            }
        };
        let support = pairs_of(&r, "tSupport").map_err(ctx)?.into_iter().map(faction_of).collect::<Result<_, _>>()?;
        let equipment = pairs_of(&r, "tEquipment").map_err(ctx)?.into_iter().map(faction_of).collect::<Result<_, _>>()?;
        let stockpile = pairs_of(&r, "tStockpile")
            .map_err(ctx)?
            .into_iter()
            .map(|(id, v)| v.as_f32().map(|n| (id.clone(), f64::from(n))).ok_or_else(|| ctx(format!("{id}: quantity is not a number"))))
            .collect::<Result<_, _>>()?;
        out.insert(
            key.clone(),
            Reward {
                cash: opt_num(&r, "nCash").map_err(ctx)?,
                fuel: opt_num(&r, "nFuel").map_err(ctx)?,
                attitude,
                support,
                equipment,
                stockpile,
            },
        );
    }
    Ok(out)
}

fn read_hints(module: &Table) -> Result<BTreeMap<String, Vec<String>>, String> {
    let data: Table = module.get("_tHints").map_err(|e| format!("_tHints: {e}"))?;
    let mut out = BTreeMap::new();
    for pair in data.pairs::<String, Table>() {
        let (speaker, hints) = pair.map_err(|e| format!("_tHints: {e}"))?;
        let mut names: Vec<String> = hints.pairs::<String, Value>().filter_map(|p| p.ok().map(|(k, _)| k)).collect();
        names.sort();
        out.insert(speaker, names);
    }
    Ok(out)
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;

    fn rules() -> GameRules {
        let wad = crate::app::retail_vz_wad();
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        GameRules::load(&mut w).unwrap_or_else(|e| panic!("{e}"))
    }

    /// Counts read out of the decompiled scripts: 69 missions (`wifmissiondata.lua:8-1244`), 213
    /// reward rows (`mrxrewarddata.lua:48-1420`), 67 flow bindings (`wifmissionflow.lua:34-1405`).
    #[test]
    fn the_shipped_rules_load() {
        let r = rules();
        assert_eq!(r.missions.len(), 69);
        assert_eq!(r.rewards.len(), 213);
        assert_eq!(r.binding_names().count(), 67);
        let pmc1 = &r.missions["PmcCon001"];
        assert_eq!(pmc1.contract, Some(true));
        assert_eq!(r.missions["GurJob002"].contract, Some(false));
        let gur1 = &r.missions["GurJob001"];
        assert_eq!(gur1.milestones.iter().map(|m| m.at).collect::<Vec<_>>(), vec![1, 5, 10, 20, 40]);
        assert_eq!(r.rewards["OilCon002"].fuel, Some(250.0));
        assert!(!r.is_live("PmcJob001"), "PmcJob001 has no fConseq (wifmissionflow.lua:675-679)");
        assert!(r.hints.contains_key("Fiona"));
    }

    /// The prereq of `PmcCon002_JetCon001` is `HasKey("PmcCon002") and HasKey("JetCon001")`.
    #[test]
    fn prereqs_read_the_given_keys() {
        let r = rules();
        let mut keys = BTreeMap::new();
        keys.insert("PmcCon002".to_string(), 1.0);
        assert!(!r.prereq_holds("PmcCon002_JetCon001", &keys).unwrap());
        keys.insert("JetCon001".to_string(), 1.0);
        assert!(r.prereq_holds("PmcCon002_JetCon001", &keys).unwrap());
    }

    /// All starts at Oil's relation (`mrxfactionmanager.lua:73-75`); Civ cannot change.
    #[test]
    fn initial_relations_come_from_the_faction_table() {
        let r = rules();
        let mut rel: BTreeMap<String, BTreeMap<String, f64>> = BTreeMap::new();
        rel.entry("Oil".into()).or_default().insert("Pmc".into(), 42.5);
        assert_eq!(r.initial_relation("All", &rel).unwrap(), Some(42.5));
        assert_eq!(r.initial_relation("Civ", &rel).unwrap(), None);
        let gur = r.initial_relation("Gur", &rel).unwrap().expect("Gur is dynamic");
        assert_eq!(r.initial_relation("Oil", &rel).unwrap(), Some(gur), "both start at the Friendly median");
    }

    /// PmcCon001's consequence runs its whole movie → layers → interior callback chain
    /// (`wifmissionflow.lua:120-175`).
    #[test]
    fn a_consequence_runs_through_its_callbacks() {
        let r = rules();
        let keys: BTreeMap<String, f64> = [("PmcCon001".to_string(), 1.0)].into();
        let none = BTreeSet::new();
        let calls = r.consequence("PmcCon001", &FlowCtx { keys: &keys, starters: &none, costumes: 1.0 }).unwrap();
        let unlocked: Vec<&str> = calls
            .iter()
            .filter(|c| c.target == "MrxMissionFlow.UnlockMission")
            .filter_map(|c| c.arg(0).str())
            .collect();
        assert_eq!(unlocked, ["OilCon020", "PmcCon031", "PmcJob001"]);
        assert!(calls.iter().any(|c| c.target == "friendlygate.LockGate" && c.arg(0).str() == Some("0x000F9A64")));
        assert!(calls.iter().any(|c| c.target == "MrxLayerManager.MarkForRemoval"
            && c.arg(0).strings() == ["Vz_State_Pmc_Pristine"]));
    }
}
