//! Contract state changes on a save, done the way the game does them.
//!
//! A contract is in one of four states in a save ([`ContractState`]): completed (its key is in
//! `tFlowData.tMyFlowData`), in progress or offered (an entry in `tFlowData.tActiveMissions`,
//! `nState` 1 or not), or not unlocked (neither).
//!
//! Completing a contract does what `_OnContainerComplete` does (`mrxmissionflow.lua:153-212`):
//! award its key and milestone keys, pay each key's `MrxRewardData` row, run the flow `Refresh`
//! until no binding fires, drop the active entry, re-offer it if repeatable, and record it as the
//! last completed contract. Each binding that fires runs as shipped ([`GameRules::consequence`]),
//! and every call it makes is applied to the save here.
//!
//! Undoing a completion removes the contract's keys and then, repeatedly, every fired binding
//! whose prerequisite no longer holds: its calls are reversed, the missions it unlocked are locked
//! again (and their own completions undone the same way), and the rewards of every removed key are
//! taken back. The world boundary, which the game overwrites rather than accumulates, is
//! recomputed from the bindings that remain fired.

use std::collections::{BTreeMap, BTreeSet};

use mercs2_formats::save::Profile;
use mercs2_formats::save_lua::{Entry, LuaString, Number, Table, Value};

use crate::game_rules::{Arg, Call, FlowCtx, GameRules, Mission};

/// What `AddCashQty` clamps an amount and a balance to (`mrxpmc.lua:45-60`).
const CASH_LIMIT: f64 = 1_000_000_000.0;

/// The boundary a new game starts with (`vz/xQ!L.lua:823`).
const NEW_GAME_BOUNDARY: &str = "BoundaryIntro";

/// Calls with no effect on the save: presentation, streaming, achievements, and the state machine
/// that sequences them.
const RUNTIME_ONLY: &[&str] = &[
    "MrxState.Enter",
    "MrxState.Exit",
    "Hud.Cinematic.Show",
    "MrxUtil.TeleportHeroesToLocations",
    "MrxLayerManager.ProcessMarkedLayers",
    "WifPmcInterior.SetTeleportCallback",
    "WifPmcInterior.Enter",
    "Pg.GetGuidByName",
    "MrxMissionFlow._BeginBlockingSequence",
    "MrxMissionFlow._EndBlockingSequence",
    "MrxSoundBootstrap.SetPmcRadio",
    "MrxVoSequence.Start",
    "MrxUnlockFanfare.AddUnlockedItem",
    "MrxVerifyManager.CheckTechnoVikingAchievement",
    "WifVzBoundary.DrawExclusionBoundaryOnMap",
    "WifVzBoundary.SetInteriorMode",
    "Player.GetPrimaryPlayer",
    "Player.GetPrimaryCharacter",
];

/// Namespaces whose calls never reach the save (achievements, GUI, music, HQ display, network,
/// game-state requests).
const RUNTIME_ONLY_PREFIXES: &[&str] =
    &["MrxAchievements.", "MrxGui.", "MrxGuiBase.", "MrxMusic.", "MrxSoundCategories.", "MrxHqManager.", "Net.", "Sys."];

/// `(item id, faction)` pairs a shop or silo has unlocked.
type Unlocks = BTreeSet<(String, String)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractState {
    Locked,
    Offered,
    InProgress,
    Completed,
}

impl ContractState {
    pub const ALL: [ContractState; 4] =
        [ContractState::Completed, ContractState::InProgress, ContractState::Offered, ContractState::Locked];

    pub fn label(self) -> &'static str {
        match self {
            ContractState::Locked => "Not unlocked",
            ContractState::Offered => "Offered",
            ContractState::InProgress => "In progress",
            ContractState::Completed => "Completed",
        }
    }
}

/// The edited save: its Lua, the header fields that mirror Lua values, and every change made.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub lua: Value,
    pub cash: u32,
    pub fuel: u32,
    pub costumes: u8,
    /// `sLastMissionName`, which the header carries at `0x2C`.
    pub last_mission: String,
    pub changes: Vec<String>,
}

/// The contract other than `except` that the save has in progress.
fn contract_in_progress(rules: &GameRules, root: &Table, except: &str) -> Option<String> {
    let active = sub(root, "tFlowData").and_then(|f| sub(f, "tActiveMissions"))?;
    active
        .iter_str()
        .filter(|(id, _)| *id != except)
        .filter(|(id, _)| rules.missions.get(*id).is_some_and(|m| m.contract == Some(true)))
        .find(|(_, e)| matches!(e, Value::Table(e) if num(e, "nState") == Some(1.0)))
        .map(|(id, _)| id.to_string())
}

/// `id`'s state in the save whose Lua root is `root`.
pub fn state_of(root: &Table, id: &str) -> ContractState {
    let flow = sub(root, "tFlowData");
    let active = flow.and_then(|f| sub(f, "tActiveMissions")).and_then(|a| sub(a, id));
    let done = flow.and_then(|f| sub(f, "tMyFlowData")).and_then(|k| k.get(id)).is_some();
    match active {
        Some(a) if num(a, "nState") == Some(1.0) => ContractState::InProgress,
        _ if done => ContractState::Completed,
        Some(_) => ContractState::Offered,
        None => ContractState::Locked,
    }
}

/// Why `to` is not offered for `id` in its current state, or `None` when it is.
pub fn unavailable(rules: &GameRules, root: &Table, id: &str, to: ContractState) -> Option<String> {
    let mission = rules.missions.get(id)?;
    let from = state_of(root, id);
    match to {
        _ if to == from => Some(format!("already {}", to.label().to_lowercase())),
        ContractState::Completed if from == ContractState::Locked => Some("not unlocked in this save".into()),
        ContractState::Offered if !mission.has_briefing => {
            Some("no briefing: the game starts this mission as soon as it unlocks".into())
        }
        ContractState::InProgress if mission.contract == Some(true) => {
            contract_in_progress(rules, root, id).map(|other| format!("{other} is the contract in progress"))
        }
        _ => None,
    }
}

/// The flow bindings whose consequence unlocks `id`, run against this save.
pub fn unlocked_by(rules: &GameRules, root: &Table, id: &str) -> Result<Vec<String>, String> {
    let w = Work { root: root.clone(), rules, cash: 0.0, fuel: 0.0, fuel_capacity: 0.0, log: Vec::new() };
    let keys = w.keys();
    let starters = w.starters();
    let costumes = num(root, "nAvailableCostumes").unwrap_or(1.0);
    rules.unlockers_of(id, &FlowCtx { keys: &keys, starters: &starters, costumes })
}

/// Move contract `id` to state `to`.
pub fn transition(lua: &Value, header: &Profile, rules: &GameRules, id: &str, to: ContractState) -> Result<Outcome, String> {
    let root = lua.as_table().map_err(|e| e.to_string())?.clone();
    let mission = rules.missions.get(id).ok_or_else(|| format!("`{id}` is not a mission in WifMissionData"))?.clone();
    if let Some(why) = unavailable(rules, &root, id, to) {
        return Err(format!("{id}: {why}"));
    }
    let mut w = Work {
        root,
        rules,
        cash: f64::from(header.cash),
        fuel: f64::from(header.fuel),
        fuel_capacity: f64::from(header.fuel_capacity),
        log: Vec::new(),
    };
    let from = state_of(&w.root, id);
    match (from, to) {
        (ContractState::Offered | ContractState::InProgress, ContractState::Completed) => w.complete(&mission)?,
        (ContractState::Completed, _) => {
            w.undo_completion(&mission)?;
            w.place(&mission, to)?;
        }
        _ => w.place(&mission, to)?,
    }
    // Retry locations belong to the contract in progress: set when it activates, cleared when it
    // completes or is cancelled (`mrxtaskcontract.lua:55-57, 144, 386`). A save that has them
    // spawns the player there instead of the PMC (`vz/xQ!L.lua:656-662`).
    if contract_in_progress(rules, &w.root, "").is_none() && w.root.remove_str("tRetryLocations").is_some() {
        w.log.push("retry locations cleared: no contract in progress".into());
    }
    let costumes = num(&w.root, "nAvailableCostumes").unwrap_or(1.0);
    let last_mission = match w.root.get("sLastMissionName") {
        Some(Value::String(s)) => s.value.clone(),
        _ => header.active_contract.clone(),
    };
    Ok(Outcome {
        lua: Value::Table(w.root),
        cash: w.cash as u32,
        fuel: w.fuel as u32,
        costumes: costumes.clamp(0.0, 255.0) as u8,
        last_mission,
        changes: w.log,
    })
}

struct Work<'a> {
    root: Table,
    rules: &'a GameRules,
    cash: f64,
    fuel: f64,
    fuel_capacity: f64,
    log: Vec<String>,
}

impl Work<'_> {
    // ── state reads ─────────────────────────────────────────────────────────────────────────

    fn keys(&self) -> BTreeMap<String, f64> {
        let mut out = BTreeMap::new();
        if let Some(k) = sub(&self.root, "tFlowData").and_then(|f| sub(f, "tMyFlowData")) {
            for (name, v) in k.iter_str() {
                if let Value::Number(n) = v {
                    out.insert(name.to_string(), n.value);
                }
            }
        }
        out
    }

    fn culled(&self) -> Vec<String> {
        sub(&self.root, "tFlowData").and_then(|f| sub(f, "tCulledBindings")).map(array_strings).unwrap_or_default()
    }

    /// Starters the game has at this point: every saved one, plus the starter of every active
    /// mission (load recreates those, `mrxmissionflow.lua:300-307`).
    fn starters(&self) -> BTreeSet<String> {
        let mut out: BTreeSet<String> =
            sub(&self.root, "tStarterData").map(|t| t.iter_str().map(|(k, _)| k.to_string()).collect()).unwrap_or_default();
        if let Some(active) = sub(&self.root, "tFlowData").and_then(|f| sub(f, "tActiveMissions")) {
            for (m, _) in active.iter_str() {
                if let Some(s) = self.rules.missions.get(m).and_then(|m| m.starter.clone()) {
                    out.insert(s);
                }
            }
        }
        out
    }

    fn consequence(&self, binding: &str, keys: &BTreeMap<String, f64>) -> Result<Vec<Call>, String> {
        let starters = self.starters();
        let costumes = num(&self.root, "nAvailableCostumes").unwrap_or(1.0);
        self.rules.consequence(binding, &FlowCtx { keys, starters: &starters, costumes })
    }

    fn flow_mut(&mut self, key: &str) -> Result<&mut Table, String> {
        let flow = need_mut(&mut self.root, "tFlowData")?;
        table_mut(flow, key)
    }

    // ── keys and rewards ────────────────────────────────────────────────────────────────────

    /// `AwardKey(key)`: count it, then pay its reward (`mrxmissionflow.lua:475-490`).
    fn award(&mut self, key: &str) -> Result<(), String> {
        let keys = self.flow_mut("tMyFlowData")?;
        let n = num(keys, key).unwrap_or(0.0) + 1.0;
        set(keys, key, number(n));
        self.log.push(format!("award key {key} ({n})"));
        self.grant(key, 1.0)
    }

    /// `MrxRewardData.DispenseRewards` for `key`'s row, `times` times; negative `times` takes it
    /// back.
    fn grant(&mut self, key: &str, times: f64) -> Result<(), String> {
        let Some(r) = self.rules.rewards.get(key).cloned() else {
            return Ok(());
        };
        if let Some(cash) = r.cash {
            let reason = key
                .get(..9)
                .and_then(|m| self.rules.missions.get(m))
                .filter(|m| m.contract == Some(true))
                .map(|_| "[Generic.Contracts]");
            self.add_cash(cash * times, reason)?;
        }
        if let Some(fuel) = r.fuel {
            self.add_fuel(fuel * times)?;
        }
        for (faction, n) in &r.attitude {
            let mutable = sub(&self.root, "tFactionData")
                .and_then(|f| sub(f, "tMutableFactions"))
                .map(|t| array_strings(t).contains(faction))
                .unwrap_or(false);
            if mutable {
                let rel = table_mut(table_mut(table_mut(&mut self.root, "tFactionData")?, "tRelations")?, faction)?;
                let v = num(rel, "Pmc").unwrap_or(0.0) + n * times;
                set(rel, "Pmc", number(v));
                self.log.push(format!("{faction} attitude {:+}", n * times));
            }
        }
        let (keep_support, keep_equipment) =
            if times < 0.0 { self.unlocks_kept_by_other_keys(key) } else { (Unlocks::new(), Unlocks::new()) };
        for (id, faction) in &r.support {
            let support = table_mut(&mut self.root, "tSupportData")?;
            let Some(Value::Table(item)) = support.get_mut(id) else { continue };
            let status = table_mut(item, "tUnlockStatus")?;
            if times > 0.0 {
                if status.get(faction).is_none() {
                    set(status, faction, number(1.0));
                    self.log.push(format!("unlock {id} at the {faction} shop"));
                }
            } else if !keep_support.contains(&(id.clone(), faction.clone())) && status.remove_str(faction).is_some() {
                self.log.push(format!("lock {id} at the {faction} shop"));
            }
        }
        for (id, faction) in &r.equipment {
            let equip = table_mut(&mut self.root, "tWifEquipmentData")?;
            let status = table_mut(equip, id)?;
            if times > 0.0 {
                if status.get(faction).is_none() {
                    set(status, faction, number(1.0));
                    self.log.push(format!("unlock {id} ({faction})"));
                }
            } else if !keep_equipment.contains(&(id.clone(), faction.clone())) && status.remove_str(faction).is_some() {
                self.log.push(format!("lock {id} ({faction})"));
            }
        }
        for (id, qty) in &r.stockpile {
            let stock = table_mut(table_mut(&mut self.root, "tPmcData")?, "tStockpile")?;
            let delta = qty * times;
            match stock.get_mut(id) {
                Some(Value::Table(item)) => {
                    let v = (num(item, "nAmt").unwrap_or(0.0) + delta).max(0.0);
                    set(item, "nAmt", number(v));
                }
                _ if delta > 0.0 => {
                    let mut item = Table::default();
                    set(&mut item, "nAmt", number(delta));
                    set(&mut item, "bNew", Value::Bool(true));
                    set(stock, id, Value::Table(item));
                }
                _ => {}
            }
            self.log.push(format!("stockpile {id} {delta:+}"));
        }
        Ok(())
    }

    /// Shop and silo unlocks that the rewards of keys other than `except` also grant.
    fn unlocks_kept_by_other_keys(&self, except: &str) -> (Unlocks, Unlocks) {
        let mut support = Unlocks::new();
        let mut equipment = Unlocks::new();
        for k in self.keys().keys().filter(|k| k.as_str() != except) {
            if let Some(r) = self.rules.rewards.get(k) {
                support.extend(r.support.iter().cloned());
                equipment.extend(r.equipment.iter().cloned());
            }
        }
        (support, equipment)
    }

    /// `MrxPmc.AddCashQty` (`mrxpmc.lua:45-79`): clamp, set the balance, count the credit in the
    /// stats. A taken-back reward comes off the credit it was counted as.
    fn add_cash(&mut self, amount: f64, reason: Option<&str>) -> Result<(), String> {
        let amount = amount.clamp(-CASH_LIMIT, CASH_LIMIT);
        let before = self.cash;
        self.cash = (self.cash + amount).clamp(0.0, CASH_LIMIT);
        set(table_mut(&mut self.root, "tPmcData")?, "nCash", number(self.cash));
        let stats = table_mut(&mut self.root, "tStatsData")?;
        let v = num(stats, "nTotalCredit").unwrap_or(0.0) + amount;
        set(stats, "nTotalCredit", number(v));
        if let Some(reason) = reason {
            let l = table_mut(stats, "tListOfCredits")?;
            let v = num(l, reason).unwrap_or(0.0) + amount;
            set(l, reason, number(v));
        }
        self.log.push(format!("cash {:+} ({} → {})", amount, before as i64, self.cash as i64));
        Ok(())
    }

    /// `MrxPmc.AddFuelQty` (`mrxpmc.lua:85-105`): clamp to 0..capacity, count it in the stats.
    fn add_fuel(&mut self, amount: f64) -> Result<(), String> {
        let before = self.fuel;
        let next = self.fuel + amount;
        self.fuel = if next > self.fuel_capacity { self.fuel_capacity } else { next.max(0.0) };
        set(table_mut(&mut self.root, "tPmcData")?, "nFuel", number(self.fuel));
        let stats = table_mut(&mut self.root, "tStatsData")?;
        let v = num(stats, "nFuelIn").unwrap_or(0.0) + amount;
        set(stats, "nFuelIn", number(v));
        self.log.push(format!("fuel {:+} ({} → {})", amount, before as i64, self.fuel as i64));
        Ok(())
    }

    // ── completing ──────────────────────────────────────────────────────────────────────────

    fn complete(&mut self, m: &Mission) -> Result<(), String> {
        self.award(&m.id)?;
        let count = self.keys().get(&m.id).copied().unwrap_or(1.0) as u32;
        for ms in &m.milestones {
            let held = self.keys().contains_key(&ms.key);
            if (m.is_job() && !held) || (!m.is_job() && ms.at == count) {
                self.award(&ms.key)?;
            }
        }
        self.refresh()?;
        if self.flow_mut("tActiveMissions")?.remove_str(&m.id).is_some() {
            self.log.push(format!("{} leaves the active missions", m.id));
        }
        if m.repeatable {
            self.unlock(&m.id)?;
        }
        set(&mut self.root, "sLastMissionName", Value::String(LuaString::new(&m.id)));
        Ok(())
    }

    /// `Refresh` (`mrxmissionflow.lua:512-561`): fire every live, unculled binding whose
    /// prerequisite holds, with key awards deferred to the end of the pass; repeat while any fired.
    fn refresh(&mut self) -> Result<(), String> {
        loop {
            let keys = self.keys();
            let culled: BTreeSet<String> = self.culled().into_iter().collect();
            let mut fired = Vec::new();
            for b in self.rules.binding_names() {
                if self.rules.is_live(b) && !culled.contains(b) && self.rules.prereq_holds(b, &keys)? {
                    fired.push(b.to_string());
                }
            }
            if fired.is_empty() {
                return Ok(());
            }
            let mut deferred = Vec::new();
            for b in &fired {
                self.log.push(format!("binding {b} fires"));
                for call in self.consequence(b, &keys)? {
                    self.apply(&call, &mut deferred)?;
                }
                if !self.rules.is_recurring(b) {
                    let list = self.flow_mut("tCulledBindings")?;
                    let mut names = array_strings(list);
                    names.push(b.clone());
                    set_array(list, strings_to_values(names));
                }
            }
            for k in deferred {
                self.award(&k)?;
            }
        }
    }

    /// The entry a freshly unlocked mission saves as. A mission with a briefing waits latent; one
    /// without starts at once, saving what its task class's `SaveInstance` writes for a fresh
    /// instance (`mrxtaskjob.lua:235-249`, `mrxtaskcontract.lua:438-445`).
    fn fresh_entry(m: &Mission, state: ContractState) -> Table {
        let mut t = Table::default();
        let active = state == ContractState::InProgress || !m.has_briefing;
        set(&mut t, "nState", number(if active { 1.0 } else { 0.0 }));
        if active {
            if m.is_job() {
                set(&mut t, "_nTargetsComplete", number(0.0));
            } else {
                set(&mut t, "tContractState", Value::Table(Table::default()));
            }
        }
        t
    }

    /// `UnlockMission(name)` (`mrxmissionflow.lua:115-131`): the case-insensitive mission id, and
    /// nothing if no such mission or it is already active.
    fn unlock(&mut self, name: &str) -> Result<(), String> {
        let Some(m) = self.rules.missions.values().find(|m| m.id.eq_ignore_ascii_case(name)).cloned() else {
            self.log.push(format!("UnlockMission({name}): no such mission, nothing unlocked"));
            return Ok(());
        };
        let active = self.flow_mut("tActiveMissions")?;
        if active.get(&m.id).is_none() {
            set(active, &m.id, Value::Table(Self::fresh_entry(&m, ContractState::Offered)));
            self.log.push(format!("unlock {}", m.id));
        }
        Ok(())
    }

    /// A starter as `RequestStarter` leaves it once activated (`mrxstarter.lua:305-310`).
    fn ensure_starter(&mut self, s: &str) -> Result<(), String> {
        let starters = table_mut(&mut self.root, "tStarterData")?;
        if starters.get(s).is_none() {
            let mut t = Table::default();
            set(&mut t, "bFanfareDisplayed", Value::Bool(true));
            set(&mut t, "bCardDisplayed", Value::Bool(false));
            set(starters, s, Value::Table(t));
            self.log.push(format!("starter {s} created"));
        }
        Ok(())
    }

    fn apply(&mut self, call: &Call, deferred: &mut Vec<String>) -> Result<(), String> {
        let t = call.target.as_str();
        if let Some(rest) = t.strip_prefix("starter:") {
            let (starter, method) = rest.split_once('.').ok_or_else(|| format!("unrecognised starter call {t}"))?;
            let intro = arg_str(call, 0)?;
            self.ensure_starter(starter)?;
            let s = table_mut(table_mut(&mut self.root, "tStarterData")?, starter)?;
            let intros = table_mut(s, "tIntros")?;
            match method {
                "AddIntro" => {
                    if intros.get(intro).is_none() {
                        set(intros, intro, Value::Bool(false));
                        self.log.push(format!("intro {intro} offered at {starter}"));
                    }
                }
                "RemoveIntro" => {
                    if intros.remove_str(intro).is_some() {
                        self.log.push(format!("intro {intro} removed from {starter}"));
                    }
                }
                _ => return Err(format!("the save editor has no mapping for {t}")),
            }
            return Ok(());
        }
        if let Some(name) = t.strip_prefix("WifVzBoundary.SetupBoundary") {
            return self.setup_boundary(name, &call.target);
        }
        match t {
            "MrxMissionFlow.UnlockMission" => self.unlock(arg_str(call, 0)?)?,
            "MrxMissionFlow.AwardKey" => deferred.push(arg_str(call, 0)?.to_string()),
            "MrxMissionFlow.DestroyMission" => {
                let m = arg_str(call, 0)?.to_string();
                if self.flow_mut("tActiveMissions")?.remove_str(&m).is_some() {
                    self.log.push(format!("{m} removed from the active missions"));
                }
                if let Some(s) = self.rules.missions.get(&m).and_then(|m| m.starter.clone()) {
                    self.ensure_starter(&s)?;
                }
            }
            "MrxMissionFlow.SetGrappleEnabled" => self.set_flag("bGrappleEnabled", call)?,
            "MrxMissionFlow.SetVehicleDisguiseEnabled" => self.set_flag("bVehicleDisguiseEnabled", call)?,
            "MrxMissionFlow.EnableResourceCounters" => self.set_flag("bResourceCountersEnabled", call)?,
            "WifHints.AddActiveHint" => self.add_hint(arg_str(call, 0)?)?,
            "WifHints.RemoveActiveHint" => self.remove_hint(arg_str(call, 0)?)?,
            "WifHints.UnlockAllHints" => {
                for h in self.rules.hints.get(arg_str(call, 0)?).cloned().unwrap_or_default() {
                    self.add_hint(&h)?;
                }
            }
            "WifBios.AddDossierEntry" => {
                let b = arg_str(call, 0)?;
                set(table_mut(&mut self.root, "tActiveBio")?, b, Value::Bool(true));
                self.log.push(format!("dossier {b}"));
            }
            "MrxLayerManager.MarkForAddition" => {
                for l in call.arg(0).strings() {
                    self.add_layer(&l)?;
                }
            }
            "MrxLayerManager.MarkForRemoval" => {
                for l in call.arg(0).strings() {
                    self.remove_layer(&l)?;
                }
            }
            "MrxStarterManager.DestroyStarter" => {
                let s = arg_str(call, 0)?;
                if table_mut(&mut self.root, "tStarterData")?.remove_str(s).is_some() {
                    self.log.push(format!("starter {s} destroyed"));
                }
            }
            "MrxStarterManager.RequestStarter" => self.ensure_starter(arg_str(call, 0)?)?,
            "MrxFactionManager.SetAttitudeMutable" => self.set_mutable(arg_str(call, 0)?)?,
            "MrxVerifyManager.SetKilledIfNotSet" => self.verify_captured(arg_str(call, 0)?)?,
            "MrxVerifyManager.SetSolanoVerified" => self.verify_solano()?,
            "WifVzBoundary.EnableExclusionBoundary" => {
                let n = arg_str(call, 0)?;
                let b = call.arg(1).bool().unwrap_or(false);
                set(table_mut(table_mut(&mut self.root, "tBoundaryData")?, "tExclusionBoundaries")?, n, Value::Bool(b));
                self.log.push(format!("exclusion boundary {n} {}", if b { "on" } else { "off" }));
            }
            "WifPmcInterior.Unlock" => {
                set(table_mut(&mut self.root, "tPmcInteriorData")?, "bUnlocked", Value::Bool(true));
                self.log.push("PMC interior unlocked".into());
            }
            "WifPmcInterior.SetAvailableCostumes" => {
                let n = call.arg(0).num().ok_or("SetAvailableCostumes: count is not a number")?;
                set(&mut self.root, "nAvailableCostumes", number(n));
                self.log.push(format!("outfits available: {n}"));
            }
            "friendlygate.LockGate" => {
                let g = arg_str(call, 0)?;
                let gates = table_mut(&mut self.root, "tLockedGates")?;
                if call.arg(1).bool() == Some(true) {
                    set(gates, g, Value::Bool(true));
                    self.log.push(format!("gate {g} locked"));
                } else if gates.remove_str(g).is_some() {
                    self.log.push(format!("gate {g} unlocked"));
                }
            }
            "Munitions.SetMunitionsTaggable" => {
                let b = call.arg(0).bool().unwrap_or(false);
                set(table_mut(&mut self.root, "tMunitionsData")?, "bMunitionsTaggable", Value::Bool(b));
                self.log.push(format!("munitions taggable: {b}"));
            }
            "MrxTransit.SetSystemEnabled" => {
                let b = call.arg(0).bool().unwrap_or(false);
                set(table_mut(&mut self.root, "tTransitData")?, "bEnabled", Value::Bool(b));
                self.log.push(format!("transit system {}", if b { "on" } else { "off" }));
            }
            "MrxTransit.SetLocationEnabled" => self.transit_enable(call)?,
            "MrxTransit.SetLocationIsNuked" => {
                let n = call.arg(0).num().ok_or("SetLocationIsNuked: location is not a number")?;
                let b = call.arg(1).bool().unwrap_or(false);
                let transit = table_mut(&mut self.root, "tTransitData")?;
                if let Some(zone) = index_mut(transit, n) {
                    set(zone, "bIsNuked", Value::Bool(b));
                    self.log.push(format!("landing zone {n} nuked: {b}"));
                }
            }
            "MrxSupportData.SetHeliPilotRecruited" => self.set_requirement("Copter", call)?,
            "MrxSupportData.SetMechanicRecruited" => self.set_requirement("Mechanic", call)?,
            "MrxSupportData.SetJetPilotRecruited" => self.set_requirement("Pilot", call)?,
            _ if is_runtime_only(t) => {}
            _ => return Err(format!("the save editor has no mapping for {t}{:?}", call.args)),
        }
        Ok(())
    }

    fn set_flag(&mut self, key: &str, call: &Call) -> Result<(), String> {
        match call.arg(0) {
            Arg::Bool(b) => set(&mut self.root, key, Value::Bool(*b)),
            Arg::Nil => {
                self.root.remove_str(key);
            }
            other => return Err(format!("{}: {other:?} is not a boolean", call.target)),
        }
        self.log.push(format!("{key} = {:?}", call.arg(0)));
        Ok(())
    }

    fn set_requirement(&mut self, key: &str, call: &Call) -> Result<(), String> {
        let b = call.arg(0).bool().ok_or_else(|| format!("{}: not a boolean", call.target))?;
        let req = table_mut(table_mut(&mut self.root, "tSupportData")?, "_tRequirementsObtained")?;
        set(req, key, Value::Bool(b));
        self.log.push(format!("{key} recruited: {b}"));
        Ok(())
    }

    fn speaker_of(&self, hint: &str) -> Option<String> {
        self.rules.hints.iter().find(|(_, hs)| hs.iter().any(|h| h == hint)).map(|(s, _)| s.clone())
    }

    /// `WifHints.AddActiveHint` (`wifhints.lua:80-88`): append under its speaker; unknown hints
    /// are dropped.
    fn add_hint(&mut self, hint: &str) -> Result<(), String> {
        let Some(speaker) = self.speaker_of(hint) else { return Ok(()) };
        let list = table_mut(table_mut(&mut self.root, "tActiveHint")?, &speaker)?;
        let mut v = array_strings(list);
        v.push(hint.to_string());
        set_array(list, strings_to_values(v));
        self.log.push(format!("hint {hint}"));
        Ok(())
    }

    /// `WifHints.RemoveActiveHint` (`wifhints.lua:90-107`): drop the first match, and the speaker
    /// when its list empties.
    fn remove_hint(&mut self, hint: &str) -> Result<(), String> {
        let Some(speaker) = self.speaker_of(hint) else { return Ok(()) };
        let hints = table_mut(&mut self.root, "tActiveHint")?;
        let Some(Value::Table(list)) = hints.get_mut(&speaker) else { return Ok(()) };
        let mut v = array_strings(list);
        let Some(i) = v.iter().position(|h| h == hint) else { return Ok(()) };
        v.remove(i);
        if v.is_empty() {
            hints.remove_str(&speaker);
        } else {
            set_array(list, strings_to_values(v));
        }
        self.log.push(format!("hint {hint} removed"));
        Ok(())
    }

    /// `MarkForAddition`: the save lists pending additions with the loaded layers, lowercased
    /// (`mrxlayermanager.lua:57-72, 292-305`).
    fn add_layer(&mut self, name: &str) -> Result<(), String> {
        let l = name.to_lowercase();
        let list = table_mut(&mut self.root, "tLayerData")?;
        let mut v = array_strings(list);
        if !v.contains(&l) {
            v.push(l.clone());
            set_array(list, strings_to_values(v));
            self.log.push(format!("layer + {l}"));
        }
        Ok(())
    }

    fn remove_layer(&mut self, name: &str) -> Result<(), String> {
        let l = name.to_lowercase();
        let list = table_mut(&mut self.root, "tLayerData")?;
        let v = array_strings(list);
        if v.contains(&l) {
            set_array(list, strings_to_values(v.into_iter().filter(|x| *x != l).collect()));
            self.log.push(format!("layer − {l}"));
        }
        Ok(())
    }

    /// `SetAttitudeMutable` (`mrxfactionmanager.lua:486-500`): a faction that can change gets its
    /// starting relation toward the PMC.
    fn set_mutable(&mut self, faction: &str) -> Result<(), String> {
        let relations = self.relations();
        let Some(start) = self.rules.initial_relation(faction, &relations)? else { return Ok(()) };
        let fd = table_mut(&mut self.root, "tFactionData")?;
        let list = table_mut(fd, "tMutableFactions")?;
        let mut v = array_strings(list);
        if !v.iter().any(|f| f == faction) {
            v.push(faction.to_string());
            set_array(list, strings_to_values(v));
        }
        let rel = table_mut(table_mut(fd, "tRelations")?, faction)?;
        set(rel, "Pmc", number(start));
        self.log.push(format!("{faction} attitude can change, starts at {start}"));
        Ok(())
    }

    fn relations(&self) -> BTreeMap<String, BTreeMap<String, f64>> {
        let mut out = BTreeMap::new();
        if let Some(r) = sub(&self.root, "tFactionData").and_then(|f| sub(f, "tRelations")) {
            for (s, row) in r.iter_str() {
                if let Value::Table(row) = row {
                    let e: &mut BTreeMap<String, f64> = out.entry(s.to_string()).or_default();
                    for (o, v) in row.iter_str() {
                        if let Value::Number(n) = v {
                            e.insert(o.to_string(), n.value);
                        }
                    }
                }
            }
        }
        out
    }

    fn verify_targets(&mut self) -> Result<&mut Table, String> {
        let stats = need_mut(&mut self.root, "tStatsData")?;
        let verify = need_mut(stats, "oVerifyData")?;
        need_mut(verify, "tTargetListStatus")
    }

    /// `SetKilledIfNotSet` (`mrxverifymanager.lua:366-375`): an unverified target reads captured.
    fn verify_captured(&mut self, target: &str) -> Result<(), String> {
        let lists = self.verify_targets()?;
        let faction = lists
            .iter_str()
            .find(|(_, t)| matches!(t, Value::Table(t) if t.get(target).is_some()))
            .map(|(f, _)| f.to_string())
            .ok_or_else(|| format!("SetKilledIfNotSet: no faction lists target `{target}`"))?;
        let list = need_mut(lists, &faction)?;
        let status = match list.get(target) {
            Some(Value::String(s)) => s.value.clone(),
            _ => String::new(),
        };
        if status.is_empty() || status == "alive" {
            set(list, target, Value::String(LuaString::new("captured")));
            self.log.push(format!("target {target} captured"));
        }
        Ok(())
    }

    /// `SetSolanoVerified` (`mrxverifymanager.lua:377-387`).
    fn verify_solano(&mut self) -> Result<(), String> {
        let lists = self.verify_targets()?;
        let killed = lists
            .iter_str()
            .filter_map(|(_, t)| if let Value::Table(t) = t { Some(t) } else { None })
            .flat_map(|t| t.iter_str())
            .any(|(_, s)| matches!(s, Value::String(s) if s.value == "killed"));
        let status = if killed { "killed" } else { "captured" };
        set(need_mut(lists, "Pmc")?, "Solano", Value::String(LuaString::new(status)));
        let verify = need_mut(need_mut(&mut self.root, "tStatsData")?, "oVerifyData")?;
        set(verify, "sSolanoStatus", Value::String(LuaString::new(status)));
        self.log.push(format!("Solano {status}"));
        Ok(())
    }

    /// `SetupBoundary*` (`wifvzboundary.lua:5-42`); three of them leave `Boundary02` in place.
    fn setup_boundary(&mut self, suffix: &str, target: &str) -> Result<(), String> {
        let (name, guarded) = boundary_name(suffix).ok_or_else(|| format!("the save editor has no mapping for {target}"))?;
        let bd = table_mut(&mut self.root, "tBoundaryData")?;
        let current = match bd.get("sBoundaryName") {
            Some(Value::String(s)) => s.value.clone(),
            _ => String::new(),
        };
        if guarded && current == "Boundary02" {
            return Ok(());
        }
        if current != name {
            set(bd, "sBoundaryName", Value::String(LuaString::new(name)));
            self.log.push(format!("world boundary {name}"));
        }
        Ok(())
    }

    /// `MrxTransit.SetLocationEnabled` (`mrxtransit.lua:175-197`).
    fn transit_enable(&mut self, call: &Call) -> Result<(), String> {
        let n = call.arg(0).num().ok_or("SetLocationEnabled: location is not a number")?;
        let faction = arg_str(call, 1)?;
        let suppress = call.arg(2).bool().unwrap_or(false);
        // Landing zone 6 is the fake one (`mrxtransit.lua:337-339`).
        if n == 6.0 {
            return Ok(());
        }
        let transit = table_mut(&mut self.root, "tTransitData")?;
        let Some(zone) = index_mut(transit, n) else { return Ok(()) };
        let was = matches!(zone.get("bEnabled"), Some(Value::Bool(true)));
        let played = matches!(zone.get("bHasPlayedFanfare"), Some(Value::Bool(true)));
        if !was && !suppress && !played {
            set(zone, "bHasPlayedFanfare", Value::Bool(true));
        }
        set(zone, "sFactionAbbrev", Value::String(LuaString::new(faction)));
        set(zone, "bEnabled", Value::Bool(true));
        self.log.push(format!("landing zone {n} open ({faction})"));
        Ok(())
    }

    // ── undoing ─────────────────────────────────────────────────────────────────────────────

    fn undo_completion(&mut self, m: &Mission) -> Result<(), String> {
        let keys = self.keys();
        let culled = self.culled();
        let mut removed: BTreeSet<String> = BTreeSet::from([m.id.clone()]);
        // Missions that end up not unlocked: this one, and every mission an undone binding unlocked.
        let mut locked: BTreeSet<String> = BTreeSet::from([m.id.clone()]);
        let mut unfired: Vec<String>;
        let mut calls_of: BTreeMap<String, Vec<Call>> = BTreeMap::new();
        for b in &culled {
            if self.rules.is_live(b) {
                calls_of.insert(b.clone(), self.consequence(b, &keys)?);
            }
        }
        let unlocks = |b: &str, mid: &str| {
            calls_of.get(b).is_some_and(|cs| {
                cs.iter().any(|c| c.target == "MrxMissionFlow.UnlockMission" && c.arg(0).str().is_some_and(|n| n.eq_ignore_ascii_case(mid)))
            })
        };
        loop {
            let before = (removed.len(), locked.len());
            // A locked mission takes its own key and its milestone and per-target keys with it;
            // a job awards milestones while it is still in progress (`mrxtaskjob.lua:154-160`).
            for mid in &locked {
                let prefix = format!("{mid}_");
                removed.extend(keys.keys().filter(|k| *k == mid || k.starts_with(&prefix)).cloned());
            }
            let after = without(&keys, &removed);
            unfired = Vec::new();
            for b in &culled {
                if self.rules.is_live(b) && !self.rules.prereq_holds(b, &after)? {
                    unfired.push(b.clone());
                }
            }
            for b in &unfired {
                for c in &calls_of[b] {
                    let k = match c.target.as_str() {
                        "MrxMissionFlow.UnlockMission" => {
                            let mid = c.arg(0).str().and_then(|n| self.rules.missions.values().find(|m| m.id.eq_ignore_ascii_case(n)));
                            // Still unlocked if a binding that stays fired unlocks it too.
                            if let Some(mid) = mid {
                                if !culled.iter().any(|o| !unfired.contains(o) && unlocks(o, &mid.id)) {
                                    locked.insert(mid.id.clone());
                                }
                            }
                            None
                        }
                        "MrxMissionFlow.AwardKey" => c.arg(0).str().map(str::to_string),
                        t if t.starts_with("starter:") && t.ends_with(".AddIntro") => c.arg(0).str().map(|i| format!("{i}Intro")),
                        _ => None,
                    };
                    if let Some(k) = k.and_then(|k| keys.keys().find(|have| have.eq_ignore_ascii_case(&k)).cloned()) {
                        removed.insert(k);
                    }
                }
            }
            if (removed.len(), locked.len()) == before {
                break;
            }
        }
        let after = without(&keys, &removed);
        // Missions a binding that stays fired also unlocks.
        let kept: BTreeSet<String> = self
            .rules
            .missions
            .keys()
            .filter(|mid| *mid != &m.id && culled.iter().any(|o| !unfired.contains(o) && unlocks(o, mid)))
            .cloned()
            .collect();

        for b in unfired.iter().rev() {
            self.log.push(format!("binding {b} undone"));
            for c in calls_of[b].iter().rev() {
                self.revert(c, &after, &kept)?;
            }
        }
        let gone: BTreeSet<&String> = unfired.iter().collect();
        let list = self.flow_mut("tCulledBindings")?;
        let kept: Vec<String> = array_strings(list).into_iter().filter(|b| !gone.contains(b)).collect();
        set_array(list, strings_to_values(kept));

        for k in &removed {
            let count = keys.get(k).copied().unwrap_or(0.0);
            self.flow_mut("tMyFlowData")?.remove_str(k);
            self.log.push(format!("key {k} removed"));
            self.grant(k, -count)?;
            if self.rules.missions.contains_key(k) && self.flow_mut("tActiveMissions")?.remove_str(k).is_some() {
                self.log.push(format!("{k} leaves the active missions"));
            }
        }
        self.recompute_boundary(&after)
    }

    /// Reverse one call of an undone binding. `after` is the key set once the undo is done;
    /// `kept` the missions a binding that stays fired also unlocks.
    fn revert(&mut self, call: &Call, after: &BTreeMap<String, f64>, kept: &BTreeSet<String>) -> Result<(), String> {
        let t = call.target.as_str();
        if let Some(rest) = t.strip_prefix("starter:") {
            let (starter, method) = rest.split_once('.').ok_or_else(|| format!("unrecognised starter call {t}"))?;
            let intro = arg_str(call, 0)?;
            let Some(Value::Table(s)) = table_mut(&mut self.root, "tStarterData")?.get_mut(starter) else { return Ok(()) };
            let intros = table_mut(s, "tIntros")?;
            match method {
                "AddIntro" => {
                    if intros.remove_str(intro).is_some() {
                        self.log.push(format!("intro {intro} withdrawn from {starter}"));
                    }
                }
                "RemoveIntro" => {
                    let viewed = after.contains_key(&format!("{intro}Intro"));
                    set(intros, intro, Value::Bool(viewed));
                    self.log.push(format!("intro {intro} back at {starter}"));
                }
                _ => return Err(format!("the save editor has no mapping for {t}")),
            }
            return Ok(());
        }
        if t.starts_with("WifVzBoundary.SetupBoundary") {
            return Ok(());
        }
        match t {
            "MrxMissionFlow.UnlockMission" => {
                let name = arg_str(call, 0)?;
                if let Some(m) = self.rules.missions.values().find(|m| m.id.eq_ignore_ascii_case(name)) {
                    let id = m.id.clone();
                    if !kept.contains(&id) && self.flow_mut("tActiveMissions")?.remove_str(&id).is_some() {
                        self.log.push(format!("{id} locked"));
                    }
                }
            }
            "MrxMissionFlow.AwardKey" => {}
            "MrxMissionFlow.DestroyMission" => {
                let name = arg_str(call, 0)?.to_string();
                if !after.contains_key(&name) {
                    self.unlock(&name)?;
                }
            }
            "MrxMissionFlow.SetGrappleEnabled" => self.invert_flag("bGrappleEnabled", call)?,
            "MrxMissionFlow.SetVehicleDisguiseEnabled" => self.invert_flag("bVehicleDisguiseEnabled", call)?,
            "MrxMissionFlow.EnableResourceCounters" => self.invert_flag("bResourceCountersEnabled", call)?,
            "WifHints.AddActiveHint" => self.remove_hint(arg_str(call, 0)?)?,
            "WifHints.RemoveActiveHint" => self.add_hint(arg_str(call, 0)?)?,
            "WifHints.UnlockAllHints" => {
                for h in self.rules.hints.get(arg_str(call, 0)?).cloned().unwrap_or_default() {
                    self.remove_hint(&h)?;
                }
            }
            "WifBios.AddDossierEntry" => {
                let b = arg_str(call, 0)?;
                if table_mut(&mut self.root, "tActiveBio")?.remove_str(b).is_some() {
                    self.log.push(format!("dossier {b} removed"));
                }
            }
            "MrxLayerManager.MarkForAddition" => {
                for l in call.arg(0).strings() {
                    self.remove_layer(&l)?;
                }
            }
            "MrxLayerManager.MarkForRemoval" => {
                for l in call.arg(0).strings() {
                    self.add_layer(&l)?;
                }
            }
            "MrxStarterManager.DestroyStarter" => self.ensure_starter(arg_str(call, 0)?)?,
            "MrxStarterManager.RequestStarter" => {
                let s = arg_str(call, 0)?.to_string();
                let in_use = sub(&self.root, "tFlowData")
                    .and_then(|f| sub(f, "tActiveMissions"))
                    .map(|a| a.iter_str().any(|(m, _)| self.rules.missions.get(m).and_then(|m| m.starter.as_deref()) == Some(s.as_str())))
                    .unwrap_or(false);
                if !in_use && table_mut(&mut self.root, "tStarterData")?.remove_str(&s).is_some() {
                    self.log.push(format!("starter {s} removed"));
                }
            }
            "MrxFactionManager.SetAttitudeMutable" => {
                let f = arg_str(call, 0)?;
                let list = table_mut(table_mut(&mut self.root, "tFactionData")?, "tMutableFactions")?;
                let v = array_strings(list);
                if v.iter().any(|x| x == f) {
                    set_array(list, strings_to_values(v.into_iter().filter(|x| x != f).collect()));
                    self.log.push(format!("{f} attitude fixed again"));
                }
            }
            "MrxVerifyManager.SetKilledIfNotSet" => {
                let target = arg_str(call, 0)?;
                let lists = self.verify_targets()?;
                let mut reset = false;
                for e in lists.entries.iter_mut() {
                    if let Value::Table(list) = &mut e.value {
                        if matches!(list.get(target), Some(Value::String(s)) if s.value == "captured") {
                            set(list, target, Value::String(LuaString::new("")));
                            reset = true;
                        }
                    }
                }
                if reset {
                    self.log.push(format!("target {target} unverified"));
                }
            }
            "MrxVerifyManager.SetSolanoVerified" => {
                let lists = self.verify_targets()?;
                set(need_mut(lists, "Pmc")?, "Solano", Value::String(LuaString::new("")));
                let verify = need_mut(need_mut(&mut self.root, "tStatsData")?, "oVerifyData")?;
                set(verify, "sSolanoStatus", Value::String(LuaString::new("alive")));
                self.log.push("Solano unverified".into());
            }
            "WifVzBoundary.EnableExclusionBoundary" => {
                let n = arg_str(call, 0)?;
                let bd = table_mut(&mut self.root, "tBoundaryData")?;
                if let Some(Value::Table(ex)) = bd.get_mut("tExclusionBoundaries") {
                    if ex.remove_str(n).is_some() {
                        self.log.push(format!("exclusion boundary {n} removed"));
                    }
                }
            }
            "WifPmcInterior.Unlock" => {
                set(table_mut(&mut self.root, "tPmcInteriorData")?, "bUnlocked", Value::Bool(false));
                self.log.push("PMC interior locked".into());
            }
            "WifPmcInterior.SetAvailableCostumes" => {
                let n = call.arg(0).num().ok_or("SetAvailableCostumes: count is not a number")?;
                let back = (n - 1.0).max(1.0);
                set(&mut self.root, "nAvailableCostumes", number(back));
                self.log.push(format!("outfits available: {back}"));
            }
            "friendlygate.LockGate" => {
                let g = arg_str(call, 0)?;
                let gates = table_mut(&mut self.root, "tLockedGates")?;
                if call.arg(1).bool() == Some(true) {
                    if gates.remove_str(g).is_some() {
                        self.log.push(format!("gate {g} unlocked"));
                    }
                } else {
                    set(gates, g, Value::Bool(true));
                    self.log.push(format!("gate {g} locked"));
                }
            }
            "Munitions.SetMunitionsTaggable" => {
                let b = call.arg(0).bool().unwrap_or(false);
                set(table_mut(&mut self.root, "tMunitionsData")?, "bMunitionsTaggable", Value::Bool(!b));
                self.log.push(format!("munitions taggable: {}", !b));
            }
            "MrxTransit.SetSystemEnabled" => {
                let b = call.arg(0).bool().unwrap_or(false);
                set(table_mut(&mut self.root, "tTransitData")?, "bEnabled", Value::Bool(!b));
                self.log.push(format!("transit system {}", if b { "off" } else { "on" }));
            }
            "MrxTransit.SetLocationEnabled" => {
                let n = call.arg(0).num().ok_or("SetLocationEnabled: location is not a number")?;
                let transit = table_mut(&mut self.root, "tTransitData")?;
                if let Some(zone) = index_mut(transit, n) {
                    set(zone, "bEnabled", Value::Bool(false));
                    zone.remove_str("sFactionAbbrev");
                    self.log.push(format!("landing zone {n} closed"));
                }
            }
            "MrxTransit.SetLocationIsNuked" => {
                let n = call.arg(0).num().ok_or("SetLocationIsNuked: location is not a number")?;
                let b = call.arg(1).bool().unwrap_or(false);
                let transit = table_mut(&mut self.root, "tTransitData")?;
                if let Some(zone) = index_mut(transit, n) {
                    set(zone, "bIsNuked", Value::Bool(!b));
                }
            }
            "MrxSupportData.SetHeliPilotRecruited" | "MrxSupportData.SetMechanicRecruited" | "MrxSupportData.SetJetPilotRecruited" => {
                return Err(format!("{t} is only called by the Start binding, which never un-fires"));
            }
            _ if is_runtime_only(t) => {}
            _ => return Err(format!("the save editor has no mapping for {t}{:?}", call.args)),
        }
        Ok(())
    }

    fn invert_flag(&mut self, key: &str, call: &Call) -> Result<(), String> {
        let b = call.arg(0).bool().ok_or_else(|| format!("{}: not a boolean", call.target))?;
        set(&mut self.root, key, Value::Bool(!b));
        self.log.push(format!("{key} = {}", !b));
        Ok(())
    }

    /// The boundary the bindings still fired leave: the new-game boundary, then every
    /// `SetupBoundary*` they call, in firing order.
    fn recompute_boundary(&mut self, keys: &BTreeMap<String, f64>) -> Result<(), String> {
        let mut name = NEW_GAME_BOUNDARY.to_string();
        for b in self.culled() {
            if !self.rules.is_live(&b) {
                continue;
            }
            for c in self.consequence(&b, keys)? {
                if let Some(suffix) = c.target.strip_prefix("WifVzBoundary.SetupBoundary") {
                    let (n, guarded) = boundary_name(suffix).ok_or_else(|| format!("the save editor has no mapping for {}", c.target))?;
                    if !(guarded && name == "Boundary02") {
                        name = n.to_string();
                    }
                }
            }
        }
        let bd = table_mut(&mut self.root, "tBoundaryData")?;
        let current = match bd.get("sBoundaryName") {
            Some(Value::String(s)) => s.value.clone(),
            _ => String::new(),
        };
        if current != name {
            set(bd, "sBoundaryName", Value::String(LuaString::new(&name)));
            self.log.push(format!("world boundary {name}"));
        }
        Ok(())
    }

    // ── placing an unlocked contract ────────────────────────────────────────────────────────

    fn place(&mut self, m: &Mission, to: ContractState) -> Result<(), String> {
        let active = self.flow_mut("tActiveMissions")?;
        match to {
            ContractState::Locked => {
                if active.remove_str(&m.id).is_some() {
                    self.log.push(format!("{} locked", m.id));
                }
            }
            ContractState::Offered | ContractState::InProgress => match active.get_mut(&m.id) {
                Some(Value::Table(entry)) => {
                    let n = if to == ContractState::InProgress { 1.0 } else { 0.0 };
                    set(entry, "nState", number(n));
                    if to == ContractState::Offered {
                        // A contract waiting at its briefing saves without contract progress
                        // (`mrxtaskcontract.lua:438-445`).
                        entry.remove_str("tContractState");
                    } else if !m.is_job() && entry.get("tContractState").is_none() {
                        set(entry, "tContractState", Value::Table(Table::default()));
                    }
                    self.log.push(format!("{} {}", m.id, to.label().to_lowercase()));
                }
                Some(other) => return Err(format!("tActiveMissions.{} is a {}", m.id, other.kind())),
                None => {
                    set(active, &m.id, Value::Table(Self::fresh_entry(m, to)));
                    self.log.push(format!("{} {}", m.id, to.label().to_lowercase()));
                }
            },
            ContractState::Completed => return Err(format!("{}: completing goes through complete()", m.id)),
        }
        // A contract that activates with no retry locations takes its start locations
        // (`mrxtaskcontract.lua:55-57`).
        if to == ContractState::InProgress && m.contract == Some(true) && self.root.get("tRetryLocations").is_none() {
            if let Some(locs) = self.rules.start_locations(&m.id)? {
                let mut t = Table::default();
                set_array(&mut t, strings_to_values(locs.clone()));
                set(&mut self.root, "tRetryLocations", Value::Table(t));
                self.log.push(format!("retry locations {}", locs.join(", ")));
            }
        }
        Ok(())
    }
}

fn without(keys: &BTreeMap<String, f64>, removed: &BTreeSet<String>) -> BTreeMap<String, f64> {
    keys.iter().filter(|(k, _)| !removed.contains(*k)).map(|(k, v)| (k.clone(), *v)).collect()
}

fn is_runtime_only(target: &str) -> bool {
    RUNTIME_ONLY.contains(&target) || RUNTIME_ONLY_PREFIXES.iter().any(|p| target.starts_with(p))
}

/// `SetupBoundary<suffix>` → (boundary name, whether `Boundary02` blocks it).
fn boundary_name(suffix: &str) -> Option<(&'static str, bool)> {
    Some(match suffix {
        "Intro" => ("BoundaryIntro", false),
        "00" => ("Boundary00", false),
        "INTRO_OIL" => ("INTRO_OIL", false),
        "POST_OIL" => ("POST_OIL", false),
        "POST_EVA_PRE_PIR" => ("POST_EVA_PRE_PIR", true),
        "POST_EVA_POST_PIR" => ("POST_EVA_POST_PIR", true),
        "PMCCON003" => ("BoundaryPMCCON003", true),
        "02" => ("Boundary02", false),
        _ => return None,
    })
}

fn arg_str(call: &Call, i: usize) -> Result<&str, String> {
    call.arg(i).str().ok_or_else(|| format!("{}: argument {} is {:?}, not a string", call.target, i + 1, call.arg(i)))
}

// ── save_lua helpers ────────────────────────────────────────────────────────────────────────

fn number(v: f64) -> Value {
    Value::Number(Number::from_f64(v))
}

fn strings_to_values(v: Vec<String>) -> Vec<Value> {
    v.into_iter().map(|s| Value::String(LuaString::new(s))).collect()
}

fn sub<'a>(t: &'a Table, key: &str) -> Option<&'a Table> {
    match t.get(key) {
        Some(Value::Table(x)) => Some(x),
        _ => None,
    }
}

fn num(t: &Table, key: &str) -> Option<f64> {
    match t.get(key) {
        Some(Value::Number(n)) => Some(n.value),
        _ => None,
    }
}

fn set(t: &mut Table, key: &str, v: Value) {
    match t.get_mut(key) {
        Some(slot) => *slot = v,
        None => t.entries.push(Entry::keyed(key, v)),
    }
}

/// The table at `key`, created empty if absent.
fn table_mut<'a>(t: &'a mut Table, key: &str) -> Result<&'a mut Table, String> {
    if t.get(key).is_none() {
        t.entries.push(Entry::keyed(key, Value::Table(Table::default())));
    }
    match t.get_mut(key) {
        Some(Value::Table(x)) => Ok(x),
        Some(other) => Err(format!("{key} is a {}, not a table", other.kind())),
        None => Err(format!("{key}: missing after insert")),
    }
}

/// The table at `key`, which the save must already have.
fn need_mut<'a>(t: &'a mut Table, key: &str) -> Result<&'a mut Table, String> {
    match t.get_mut(key) {
        Some(Value::Table(x)) => Ok(x),
        Some(other) => Err(format!("{key} is a {}, not a table", other.kind())),
        None => Err(format!("the save has no {key}")),
    }
}

/// The table at numeric key `n` (`tTransitData[3]`).
fn index_mut(t: &mut Table, n: f64) -> Option<&mut Table> {
    t.entries.iter_mut().find_map(|e| match (&e.key, &mut e.value) {
        (Some(Value::Number(k)), Value::Table(x)) if k.value == n => Some(x),
        _ => None,
    })
}

/// The string values of an array table, in order.
fn array_strings(t: &Table) -> Vec<String> {
    t.entries
        .iter()
        .filter(|e| e.key.is_none() || matches!(e.key, Some(Value::Number(_))))
        .filter_map(|e| match &e.value {
            Value::String(s) => Some(s.value.clone()),
            _ => None,
        })
        .collect()
}

/// Replace an array table's elements with `vals`, numbered 1..n as the game writes them.
fn set_array(t: &mut Table, vals: Vec<Value>) {
    t.entries.retain(|e| !(e.key.is_none() || matches!(e.key, Some(Value::Number(_)))));
    for (i, v) in vals.into_iter().enumerate() {
        t.entries.push(Entry::index(i + 1, v));
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use crate::app::WadStack;
    use mercs2_formats::{game_paths, save, save_lua};

    fn rules() -> GameRules {
        let wad = mercs2_engine::wad::resolve_vz_wad(None)
            .expect("retail vz.wad — set vz_wad in .mercs2-local.toml or VZ_WAD");
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        GameRules::load(&mut w).unwrap_or_else(|e| panic!("{e}"))
    }

    fn fixture(name: &str) -> (Profile, Value) {
        let bytes = std::fs::read(game_paths::save_fixtures().join(name)).unwrap();
        let p = save::parse(&bytes).unwrap();
        let lua = p.decompress_lua().unwrap();
        let v = save_lua::parse_chunk(std::str::from_utf8(&lua).unwrap()).unwrap();
        (p, v)
    }

    fn root(v: &Value) -> &Table {
        v.as_table().unwrap()
    }

    fn keyset(t: Option<&Table>) -> BTreeSet<String> {
        t.map(|t| t.iter_str().map(|(k, _)| k.to_string()).collect()).unwrap_or_default()
    }

    fn flow<'a>(v: &'a Value, key: &str) -> Option<&'a Table> {
        sub(root(v), "tFlowData").and_then(|f| sub(f, key))
    }

    fn strings(t: Option<&Table>) -> Vec<String> {
        t.map(array_strings).unwrap_or_default()
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    fn boundary(v: &Value) -> String {
        match sub(root(v), "tBoundaryData").and_then(|b| b.get("sBoundaryName")) {
            Some(Value::String(s)) => s.value.clone(),
            _ => String::new(),
        }
    }

    /// Completing PmcCon001 in the save made just before it (`auto_6A499D08`, PmcCon001 in
    /// progress) gives the flow state of the game's own autosave made just after it
    /// (`auto_6A0BE454`): same keys, fired bindings, active missions, dossiers, hints, gates,
    /// boundary, interior.
    #[test]
    fn completing_matches_the_games_next_autosave() {
        let r = rules();
        let (p, before) = fixture("auto_6A499D08.profile");
        let (_, game) = fixture("auto_6A0BE454.profile");
        let out = transition(&before, &p, &r, "PmcCon001", ContractState::Completed).unwrap_or_else(|e| panic!("{e}"));
        let ours = &out.lua;
        assert_eq!(keyset(flow(ours, "tMyFlowData")), keyset(flow(&game, "tMyFlowData")));
        assert_eq!(sorted(strings(flow(ours, "tCulledBindings"))), sorted(strings(flow(&game, "tCulledBindings"))));
        assert_eq!(keyset(flow(ours, "tActiveMissions")), keyset(flow(&game, "tActiveMissions")));
        for key in ["tActiveBio", "tLockedGates"] {
            assert_eq!(keyset(sub(root(ours), key)), keyset(sub(root(&game), key)), "{key}");
        }
        assert_eq!(boundary(ours), boundary(&game));
        assert_eq!(
            sub(root(ours), "tActiveHint").map(|h| h.to_str_map()),
            sub(root(&game), "tActiveHint").map(|h| h.to_str_map())
        );
        assert_eq!(
            sub(root(ours), "tPmcInteriorData").and_then(|t| t.get("bUnlocked")),
            sub(root(&game), "tPmcInteriorData").and_then(|t| t.get("bUnlocked"))
        );
        let pay = r.rewards["PmcCon001"].cash.unwrap_or(0.0) as u32;
        assert_eq!(out.cash, p.cash + pay);
        assert!(root(&before).get("tRetryLocations").is_some(), "saved mid-contract");
        assert_eq!(root(ours).get("tRetryLocations"), root(&game).get("tRetryLocations"), "spawn at the PMC, not the contract");
        let (gp, _) = fixture("auto_6A0BE454.profile");
        assert_eq!(out.last_mission, gp.active_contract);
    }

    /// The Chris save was made mid-PmcCon001 with retry locations at its start; completing it must
    /// not leave them, or the load spawns the player back at PmcCon001's start.
    #[test]
    fn completing_the_contract_in_progress_clears_its_retry_locations() {
        let r = rules();
        let (p, v) = fixture("Chris Jacobs_6A499ED6.profile");
        assert!(root(&v).get("tRetryLocations").is_some());
        let out = transition(&v, &p, &r, "PmcCon001", ContractState::Completed).unwrap_or_else(|e| panic!("{e}"));
        assert!(root(&out.lua).get("tRetryLocations").is_none());
        assert_eq!(out.last_mission, "PmcCon001");
    }

    /// Putting a contract in progress gives it the start locations the game would (and only one
    /// contract can be in progress).
    #[test]
    fn a_contract_put_in_progress_gets_its_start_locations() {
        let r = rules();
        let (p, v) = fixture("auto_6A0BE454.profile");
        assert!(root(&v).get("tRetryLocations").is_none());
        let out = transition(&v, &p, &r, "OilCon020", ContractState::InProgress).unwrap_or_else(|e| panic!("{e}"));
        let want = r.start_locations("OilCon020").unwrap().expect("OilCon020 has start locations");
        assert_eq!(strings(sub(root(&out.lua), "tRetryLocations")), want);
        let why = unavailable(&r, root(&out.lua), "PmcCon031", ContractState::InProgress);
        assert_eq!(why.as_deref(), Some("OilCon020 is the contract in progress"));
    }

    /// Undoing PmcCon001 in the save made after it gives back the save made before it, and
    /// completing it again returns to where it started.
    #[test]
    fn undo_then_redo_round_trips() {
        let r = rules();
        let (p, after) = fixture("auto_6A0BE454.profile");
        let (_, game_before) = fixture("auto_6A499D08.profile");
        let undone = transition(&after, &p, &r, "PmcCon001", ContractState::InProgress).unwrap_or_else(|e| panic!("{e}"));
        let u = &undone.lua;
        assert_eq!(keyset(flow(u, "tMyFlowData")), keyset(flow(&game_before, "tMyFlowData")));
        assert_eq!(keyset(flow(u, "tActiveMissions")), keyset(flow(&game_before, "tActiveMissions")));
        assert_eq!(boundary(u), boundary(&game_before));
        for key in ["tActiveBio", "tLockedGates"] {
            assert_eq!(keyset(sub(root(u), key)), keyset(sub(root(&game_before), key)), "{key}");
        }
        assert_eq!(f64::from(undone.cash), f64::from(p.cash) - r.rewards["PmcCon001"].cash.unwrap_or(0.0));

        let mut p2 = p.clone();
        p2.cash = undone.cash;
        p2.fuel = undone.fuel;
        let redone = transition(&undone.lua, &p2, &r, "PmcCon001", ContractState::Completed).unwrap_or_else(|e| panic!("{e}"));
        let d = &redone.lua;
        assert_eq!(keyset(flow(d, "tMyFlowData")), keyset(flow(&after, "tMyFlowData")));
        assert_eq!(keyset(flow(d, "tActiveMissions")), keyset(flow(&after, "tActiveMissions")));
        assert_eq!(sorted(strings(sub(root(d), "tLayerData"))), sorted(strings(sub(root(&after), "tLayerData"))), "layers");
        assert_eq!(redone.cash, p.cash);
    }

    /// Undoing PmcCon001 deep into a playthrough takes everything that followed from it.
    #[test]
    fn undo_cascades_through_what_it_unlocked() {
        let r = rules();
        let (p, v) = fixture("Mattias Nilsson_63430745.profile");
        let out = transition(&v, &p, &r, "PmcCon001", ContractState::Locked).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(keyset(flow(&out.lua, "tMyFlowData")), BTreeSet::from(["VzaCon001".to_string()]));
        assert_eq!(strings(flow(&out.lua, "tCulledBindings")), ["Start", "VzaCon001"]);
        assert!(keyset(flow(&out.lua, "tActiveMissions")).is_empty(), "{:?}", keyset(flow(&out.lua, "tActiveMissions")));
        assert_eq!(boundary(&out.lua), "Boundary00");
    }

    /// PmcCon004 is unlocked by AllCon003's binding and by ChiCon003's (`wifmissionflow.lua:1026,
    /// 1246`). With both fired, undoing ChiCon003 leaves PmcCon004 where it was.
    #[test]
    fn a_mission_another_fired_binding_unlocks_stays_unlocked() {
        let r = rules();
        let (mut p, mut v) = fixture("Mattias Nilsson_6A0E523C.profile");
        for id in ["AllCon003", "ChiCon003"] {
            let s = state_of(root(&v), id);
            assert!(matches!(s, ContractState::Offered | ContractState::Completed), "{id} is {s:?} in the fixture");
            if s != ContractState::Completed {
                let out = transition(&v, &p, &r, id, ContractState::Completed).unwrap_or_else(|e| panic!("{id}: {e}"));
                p.cash = out.cash;
                p.fuel = out.fuel;
                v = out.lua;
            }
        }
        let both = state_of(root(&v), "PmcCon004");
        assert_ne!(both, ContractState::Locked);
        let undone = transition(&v, &p, &r, "ChiCon003", ContractState::Offered).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(state_of(root(&undone.lua), "PmcCon004"), both);
        assert!(strings(flow(&undone.lua, "tCulledBindings")).contains(&"AllCon003".to_string()));
        assert!(!strings(flow(&undone.lua, "tCulledBindings")).contains(&"ChiCon003".to_string()));
    }

    /// Every fired binding in every fixture maps onto the save: undoing each completed contract
    /// raises no unmapped call.
    #[test]
    fn every_fixture_completion_can_be_undone() {
        let r = rules();
        for name in game_paths::SAVE_FIXTURES {
            let (p, v) = fixture(name);
            for (k, _) in flow(&v, "tMyFlowData").map(|t| t.to_str_map()).unwrap_or_default() {
                if r.missions.contains_key(&k) {
                    transition(&v, &p, &r, &k, ContractState::Locked).unwrap_or_else(|e| panic!("{name} {k}: {e}"));
                }
            }
        }
    }
}
