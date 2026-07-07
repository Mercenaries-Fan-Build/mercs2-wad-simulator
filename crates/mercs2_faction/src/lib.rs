//! `mercs2_faction` — the faction / reputation / pursuit ("heat") mechanism.
//!
//! **Silo 13** (`docs/modernization/reimplementation_parallelization_plan.md` §3).
//! **Scoreboard row(s):** cross-cutting (AI 23 / population 24 / HUD 27 / music 21) — no row of its own.
//! **Code map:** `docs/reverse_engineer/faction_reputation_code_map.md` (§10 = the reimpl target),
//! with `ai_code_map.md` for the `Suspect` per-faction wanted component.
//! **Owned Lua surface:** `Ai.AddInfraction`, `Ai.Get/SetRelation`, `Pg.*Pursuit*`, `MrxFactionManager`.
//!
//! Per the code map's §10 disposition this crate supplies the **mechanism the engine owns** — a thin
//! native layer under what was a Lua brain (`mrxfactionmanager.lua`). The faithful pieces:
//!
//! - [`mood`] — the combat→faction **7-key infraction accumulator** + the recovered mood weighting
//!   (`FUN_005e0720` serialized these exact seven keys, §2);
//! - [`attitude`] — the relation `[-100,100]` → attitude-level / **price** / meter policy (§3);
//! - [`pursuit`] — the per-faction **heat** level + dwell countdown (§5);
//! - [`components`] — the `FactionMarker`/`FactionValue`/`FactionZone`/`RtFactionZone`/`Suspect`
//!   reflection components (§4 + AI census);
//! - [`factions`] — the eight faction identities + the recovered initial-relation policy (§3).
//!
//! **Carve rule (plan §4):** leaf crates never depend on each other. The relation **write** is the
//! `Ai.SetRelation` matrix owned by `mercs2_ai`, so this crate does **not** touch that matrix; it
//! emits [`RelationChange`] *intents* the game mirrors into `mercs2_ai::RelationMatrix`. It keeps its
//! own relation model (exactly as `mrxfactionmanager.lua` held `_tFactions[x]` state and *also* called
//! `Ai.SetRelation`) to compute deltas, attitude-level crossings, and pursuit escalation.

pub mod attitude;
pub mod components;
pub mod factions;
pub mod mood;
pub mod pursuit;

pub use attitude::{relation_to_meter, Attitude, RELATION_MAX, RELATION_MIN};
pub use components::{
    FactionMarker, FactionValue, FactionZone, RtFactionZone, Suspect, FACTION_MARKER_HASH,
    FACTION_VALUE_HASH, FACTION_ZONE_HASH, RT_FACTION_ZONE_HASH, SUSPECT_HASH,
};
pub use mood::{civilian_casualty_penalty, InfractionAccumulator, InfractionKind, InfractionSlot};
pub use pursuit::PursuitState;

use std::collections::HashMap;

/// A pending `Ai.SetRelation(from, to, value)` intent — the crate's relation **output**. The game
/// applies these to `mercs2_ai::RelationMatrix` after each faction step (the carve seam: this crate
/// does not depend on `mercs2_ai`). `value` is already clamped to `[-100, 100]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationChange {
    pub from: u32,
    pub to: u32,
    pub value: i32,
}

/// An `Event.Post("Attitude", …)` payload — emitted when a relation change crosses an attitude
/// **level** boundary (Hostile/Neutral/Friendly), exactly as `mrxfactionmanager.lua:587` posts it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttitudeEvent {
    /// The faction whose attitude changed.
    pub faction: u32,
    /// The object of the attitude (typically the PMC).
    pub toward: u32,
    /// Attitude level before the change.
    pub old_attitude: Attitude,
    /// Attitude level after the change.
    pub new_attitude: Attitude,
    /// The new relation value that produced `new_attitude`.
    pub relation: i32,
}

/// The host-owned faction mechanism: per-faction infraction accumulators, the manager's relation
/// model, and per-faction pursuit state, plus the two drainable output queues. The game's script host
/// drives `Ai.AddInfraction` / the report tick through this and mirrors the emitted [`RelationChange`]
/// intents into the AI relation matrix + routes [`AttitudeEvent`]s onto the event bus.
pub struct FactionWorld {
    /// The PMC faction GUID — relations are computed *toward* this faction (`ChangeRelation(f,"Pmc")`).
    pmc: u32,
    /// Per-faction 7-key infraction accumulator (`Ai.AddInfraction` accrues here).
    accumulators: HashMap<u32, InfractionAccumulator>,
    /// The manager's own directed relation model, `[-100,100]`, keyed `(from, to)`. Mirrors the AI
    /// matrix the emitted intents write; an unset pair reads `0` (neutral).
    relations: HashMap<(u32, u32), i32>,
    /// Per-faction pursuit ("heat") state.
    pursuit: HashMap<u32, PursuitState>,
    /// Per-faction infraction multiplier (`Ai.SetInfractionMultiplier`) applied to every scripted
    /// `Ai.AddInfraction`. Unset ⇒ `1`; `0` disables the faction's infractions (the shipped
    /// `gurcon002.lua` toggles this `0 ↔ 1` around scripted damage windows).
    infraction_multiplier: HashMap<u32, i32>,
    /// Drainable `Ai.SetRelation` intents.
    relation_changes: Vec<RelationChange>,
    /// Drainable `Attitude` events.
    attitude_events: Vec<AttitudeEvent>,
}

impl FactionWorld {
    /// A faction world whose relations are computed toward `pmc` (the PMC faction GUID).
    pub fn new(pmc: u32) -> Self {
        FactionWorld {
            pmc,
            accumulators: HashMap::new(),
            relations: HashMap::new(),
            pursuit: HashMap::new(),
            relation_changes: Vec::new(),
            attitude_events: Vec::new(),
            infraction_multiplier: HashMap::new(),
        }
    }

    /// A faction world seeded with the recovered **initial relations** (§3) over the eight standard
    /// factions (GUIDs from [`factions::faction_guid`]). Dynamic factions (All/Chi/Gur/Oil/Pir) get
    /// their documented starting relation toward the PMC; every faction gets `+100` self-relation.
    pub fn with_default_relations() -> Self {
        use factions::{faction_guid, SELF_RELATION};
        let pmc = faction_guid("PMC");
        let mut w = FactionWorld::new(pmc);

        // Self-relation = +100 for all eight (init, no attitude event).
        for t in factions::FACTION_TEMPLATES {
            let g = faction_guid(t);
            w.init_relation(g, g, SELF_RELATION);
        }

        // Recovered initial relations toward the PMC (§3 "Initial relations" row), applied in the
        // documented dependency order (All reads Oil, Chi reads Gur):
        let (gur, oil, pir) =
            (faction_guid("Guerilla"), faction_guid("OC"), faction_guid("Pirate"));
        let (all, chi) = (faction_guid("Allied"), faction_guid("China"));
        w.init_relation(pir, pmc, Attitude::Neutral.median()); // Pir = median(Neutral) = 0
        w.init_relation(gur, pmc, Attitude::Friendly.median()); // Gur = median(Friendly)
        w.init_relation(oil, pmc, Attitude::Friendly.median()); // Oil = median(Friendly)
        w.init_relation(all, pmc, w.get_relation(oil, pmc)); // All = GetRelation(Oil, Pmc)
        w.init_relation(chi, pmc, w.get_relation(gur, pmc)); // Chi = GetRelation(Gur, Pmc)
        w
    }

    /// The PMC faction GUID this world scores toward.
    pub fn pmc(&self) -> u32 {
        self.pmc
    }

    // --- infractions -------------------------------------------------------

    /// `Ai.AddInfraction`-side accrual: add `amount` to `faction`'s accumulator under `kind`, stamping
    /// `owner_id` (the faction/owner of the affected entity) into the slot's `id`. Use
    /// [`add_special_infraction`] for `SpecialEvent` (which needs a multiplier).
    ///
    /// [`add_special_infraction`]: FactionWorld::add_special_infraction
    pub fn add_infraction(&mut self, faction: u32, kind: InfractionKind, owner_id: i32, amount: i32) {
        self.accumulators.entry(faction).or_default().add(kind, owner_id, amount);
    }

    /// A `SpecialEvent` infraction: its mood term is `multiplier × amount` (`SpecialEvent[1]×[2]`).
    pub fn add_special_infraction(&mut self, faction: u32, multiplier: i32, amount: i32) {
        self.accumulators.entry(faction).or_default().add_special(multiplier, amount);
    }

    /// `Ai.AddInfraction(offender, faction, amount)` — accrue a scripted infraction against `faction`,
    /// weighted by its current [`infraction_multiplier`](Self::infraction_multiplier) (a `SpecialEvent`
    /// slot: mood term = `multiplier × amount`). A faction whose multiplier is `0` ignores the
    /// infraction, matching the shipped disable/enable pattern.
    pub fn add_scripted_infraction(&mut self, faction: u32, amount: i32) {
        let mult = self.infraction_multiplier(faction);
        if mult == 0 {
            return;
        }
        self.add_special_infraction(faction, mult, amount);
    }

    /// `Ai.SetInfractionMultiplier(faction, mult)` — set the standing multiplier applied to future
    /// scripted infractions against `faction`. `0` disables them.
    pub fn set_infraction_multiplier(&mut self, faction: u32, multiplier: i32) {
        self.infraction_multiplier.insert(faction, multiplier);
    }

    /// The faction's current infraction multiplier (`1` if never set).
    pub fn infraction_multiplier(&self, faction: u32) -> i32 {
        self.infraction_multiplier.get(&faction).copied().unwrap_or(1)
    }

    /// Read a faction's current accumulator (before it is reported).
    pub fn accumulator(&self, faction: u32) -> InfractionAccumulator {
        self.accumulators.get(&faction).copied().unwrap_or_default()
    }

    // --- the report tick ---------------------------------------------------

    /// `MrxFactionManager.Report` → `FinishedReporting`: weight `faction`'s accumulator into a mood,
    /// apply the resulting relation delta toward the PMC (emitting a [`RelationChange`] intent and an
    /// [`AttitudeEvent`] on a level cross), clear the accumulator, then escalate pursuit if the
    /// relation hit `≤ -100`. No-op (returns `false`) if the accumulator is empty.
    pub fn report(&mut self, faction: u32) -> bool {
        let acc = self.accumulator(faction);
        if acc.is_empty() {
            return false;
        }
        let delta = acc.relation_delta();
        self.accumulators.remove(&faction);
        let new_rel = self.get_relation(faction, self.pmc).saturating_add(delta);
        self.apply_relation(faction, self.pmc, new_rel, true);
        self.maybe_escalate_pursuit(faction);
        true
    }

    /// Apply the recovered **civilian-casualty** collateral penalty to a faction's relation toward the
    /// PMC (`mrxfactionmanager.lua:815-823`): the penalty for `total_civ_kills` (see
    /// [`mood::civilian_casualty_penalty`]) is added to the relation. Emits the same intent/event as a
    /// report and can escalate pursuit. Returns the applied (negative) penalty.
    pub fn report_civilian_casualties(&mut self, faction: u32, total_civ_kills: u32) -> i64 {
        let penalty = civilian_casualty_penalty(total_civ_kills);
        // Fold into the relation (the relation clamps at -100 regardless of the raw magnitude).
        let clamped_delta = penalty.max(i32::MIN as i64) as i32;
        let new_rel = self.get_relation(faction, self.pmc).saturating_add(clamped_delta);
        self.apply_relation(faction, self.pmc, new_rel, true);
        self.maybe_escalate_pursuit(faction);
        penalty
    }

    // --- relations ---------------------------------------------------------

    /// `Ai.SetRelation(from, to, value)` (script-driven): set the relation, emit the intent, and emit
    /// an [`AttitudeEvent`] if the attitude level changed. The full public setter.
    pub fn set_relation(&mut self, from: u32, to: u32, value: i32) {
        self.apply_relation(from, to, value, true);
    }

    /// `Ai.SetRelation(…, bInit=true)`: seed a relation (still emits the [`RelationChange`] intent so
    /// the AI matrix is initialised) but **suppress** the attitude event, matching the Lua `bInit`
    /// path that sets up starting relations without firing an attitude-level notification.
    pub fn init_relation(&mut self, from: u32, to: u32, value: i32) {
        self.apply_relation(from, to, value, false);
    }

    /// `Ai.GetRelation(from, to)` — the manager's directed relation, `0` (neutral) if unset.
    pub fn get_relation(&self, from: u32, to: u32) -> i32 {
        self.relations.get(&(from, to)).copied().unwrap_or(0)
    }

    /// The core write: clamp, store, emit the intent, and (when `emit_event`) emit an attitude event
    /// on a level cross.
    fn apply_relation(&mut self, from: u32, to: u32, value: i32, emit_event: bool) {
        let clamped = value.clamp(RELATION_MIN, RELATION_MAX);
        let old = self.get_relation(from, to);
        self.relations.insert((from, to), clamped);
        self.relation_changes.push(RelationChange { from, to, value: clamped });
        if emit_event {
            let old_att = Attitude::classify(old);
            let new_att = Attitude::classify(clamped);
            if old_att != new_att {
                self.attitude_events.push(AttitudeEvent {
                    faction: from,
                    toward: to,
                    old_attitude: old_att,
                    new_attitude: new_att,
                    relation: clamped,
                });
            }
        }
    }

    // --- policy readouts ---------------------------------------------------

    /// `GetPriceScale`: the shop price multiplier for `faction` (from its attitude toward the PMC):
    /// `None` = Hostile / will not sell; `Some(1.5)` Neutral; `Some(1.0)` Friendly.
    pub fn price_multiplier(&self, faction: u32) -> Option<f32> {
        self.attitude(faction).price_multiplier()
    }

    /// A faction's attitude level toward the PMC.
    pub fn attitude(&self, faction: u32) -> Attitude {
        Attitude::classify(self.get_relation(faction, self.pmc))
    }

    /// A faction's HUD meter value `[0,100]` (from its relation toward the PMC).
    pub fn meter(&self, faction: u32) -> f32 {
        relation_to_meter(self.get_relation(faction, self.pmc))
    }

    // --- pursuit -----------------------------------------------------------

    /// A faction's current pursuit ("heat") level `0..=3`.
    pub fn pursuit_level(&self, faction: u32) -> u8 {
        self.pursuit.get(&faction).map(|p| p.level).unwrap_or(0)
    }

    /// The faction's full pursuit state (level + dwell timer + lock).
    pub fn pursuit_state(&self, faction: u32) -> PursuitState {
        self.pursuit.get(&faction).copied().unwrap_or_default()
    }

    /// `Pg.LockPursuit(uGuid, level)` — pin a faction's pursuit level.
    pub fn lock_pursuit(&mut self, faction: u32, level: u8) {
        self.pursuit.entry(faction).or_default().lock(level);
    }

    /// `Pg.ClearPursuitLock` — unpin a faction's pursuit level.
    pub fn clear_pursuit_lock(&mut self, faction: u32) {
        if let Some(p) = self.pursuit.get_mut(&faction) {
            p.clear_lock();
        }
    }

    /// `IncrementPursuit`: if `faction`'s relation toward the PMC is `≤ -100`, bump its heat a level
    /// (capped at 3) and arm the level's dwell countdown.
    fn maybe_escalate_pursuit(&mut self, faction: u32) {
        if self.get_relation(faction, self.pmc) <= attitude::PURSUIT_ESCALATE_AT {
            let p = self.pursuit.entry(faction).or_default();
            if p.increment() {
                p.settle(); // ride the level's full recovered dwell (120 / 300 s)
            }
        }
    }

    // --- tick + outputs ----------------------------------------------------

    /// Advance every faction's pursuit dwell countdown by `dt` seconds (levels auto-decay per §5).
    /// Infractions do not passively decay — they persist until a report consumes them — so nothing
    /// else settles here (matching the code map: the native side owns only the pursuit countdown).
    pub fn tick(&mut self, dt: f32) {
        for p in self.pursuit.values_mut() {
            p.tick(dt);
        }
    }

    /// Drain the pending `Ai.SetRelation` intents (the game mirrors them into
    /// `mercs2_ai::RelationMatrix`).
    pub fn take_relation_changes(&mut self) -> Vec<RelationChange> {
        std::mem::take(&mut self.relation_changes)
    }

    /// Drain the pending `Attitude` events (the game routes them onto the event bus / HUD / PDA).
    pub fn take_attitude_events(&mut self) -> Vec<AttitudeEvent> {
        std::mem::take(&mut self.attitude_events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hostile report drops the PMC relation, emits a matching `SetRelation` intent, and (crossing
    /// Neutral→Hostile) an `Attitude` event — the whole §2→§3 loop end to end.
    #[test]
    fn report_drops_relation_and_emits_intent_and_event() {
        let mut w = FactionWorld::new(PMC_placeholder());
        let faction = 1;
        // Seed a neutral-ish relation.
        w.set_relation(faction, w.pmc(), 0);
        let _ = w.take_relation_changes();
        let _ = w.take_attitude_events();

        // Two kills (×50) = mood 100 → relation delta -100 → new relation -100 (clamped).
        w.add_infraction(faction, InfractionKind::DestroyPerson, faction as i32, 2);
        assert!(w.report(faction));

        assert_eq!(w.get_relation(faction, w.pmc()), -100);
        let changes = w.take_relation_changes();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], RelationChange { from: faction, to: w.pmc(), value: -100 });

        let events = w.take_attitude_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].old_attitude, Attitude::Neutral);
        assert_eq!(events[0].new_attitude, Attitude::Hostile);

        // relation hit -100 → pursuit escalated to level 1.
        assert_eq!(w.pursuit_level(faction), 1);
        // accumulator was consumed.
        assert!(w.accumulator(faction).is_empty());
    }

    /// Reporting an empty accumulator is a no-op.
    #[test]
    fn empty_report_is_noop() {
        let mut w = FactionWorld::new(7);
        assert!(!w.report(1));
        assert!(w.take_relation_changes().is_empty());
        assert!(w.take_attitude_events().is_empty());
    }

    /// `init_relation` seeds + emits the intent but fires no attitude event (the `bInit` path).
    #[test]
    fn init_relation_suppresses_event() {
        let mut w = FactionWorld::new(7);
        w.init_relation(1, 7, 66); // straight to Friendly
        assert_eq!(w.get_relation(1, 7), 66);
        assert_eq!(w.take_relation_changes().len(), 1);
        assert!(w.take_attitude_events().is_empty(), "bInit fires no attitude event");
    }

    /// Price policy reads through the relation: Friendly 1.0×, Neutral 1.5×, Hostile no-sell.
    #[test]
    fn price_multiplier_tracks_attitude() {
        let mut w = FactionWorld::new(7);
        w.init_relation(1, 7, 80);
        assert_eq!(w.attitude(1), Attitude::Friendly);
        assert_eq!(w.price_multiplier(1), Some(1.0));
        w.init_relation(1, 7, 0);
        assert_eq!(w.price_multiplier(1), Some(1.5));
        w.init_relation(1, 7, -80);
        assert_eq!(w.price_multiplier(1), None);
        assert_eq!(w.meter(1), relation_to_meter(-80));
    }

    /// Default-relations constructor reproduces the recovered §3 initial relations.
    #[test]
    fn default_relations_match_recovered_initials() {
        use factions::faction_guid;
        let w = FactionWorld::with_default_relations();
        let pmc = w.pmc();
        assert_eq!(w.get_relation(faction_guid("Pirate"), pmc), 0); // median(Neutral)
        assert_eq!(w.get_relation(faction_guid("Guerilla"), pmc), 66); // median(Friendly)
        assert_eq!(w.get_relation(faction_guid("OC"), pmc), 66);
        assert_eq!(
            w.get_relation(faction_guid("Allied"), pmc),
            w.get_relation(faction_guid("OC"), pmc),
            "All = GetRelation(Oil, Pmc)"
        );
        assert_eq!(
            w.get_relation(faction_guid("China"), pmc),
            w.get_relation(faction_guid("Guerilla"), pmc),
            "Chi = GetRelation(Gur, Pmc)"
        );
        // self-relation +100
        let all = faction_guid("Allied");
        assert_eq!(w.get_relation(all, all), 100);
        // dynamic gate
        assert!(factions::is_dynamic("Allied"));
        assert!(!factions::is_dynamic("Civ"));
    }

    /// Pursuit escalates on a -100 relation and auto-decays after the level's dwell.
    #[test]
    fn pursuit_escalates_then_decays() {
        let mut w = FactionWorld::new(7);
        w.set_relation(1, 7, -100); // straight to the pursuit threshold... but set_relation alone
                                    // does not escalate; escalation rides the report/civilian path.
        assert_eq!(w.pursuit_level(1), 0);

        // Drive it through a report so pursuit escalates.
        w.add_infraction(1, InfractionKind::DestroyPerson, 1, 10); // huge mood → clamps to -100
        w.report(1);
        assert_eq!(w.pursuit_level(1), 1);

        // decay: level-1 dwell is 120 s.
        w.tick(119.0);
        assert_eq!(w.pursuit_level(1), 1);
        w.tick(2.0);
        assert_eq!(w.pursuit_level(1), 0);
    }

    /// The civilian-casualty path applies the recovered penalty and can escalate pursuit.
    #[test]
    fn civilian_casualties_penalise_and_escalate() {
        let mut w = FactionWorld::new(7);
        w.init_relation(1, 7, 100);
        let _ = w.take_relation_changes();
        let pen = w.report_civilian_casualties(1, 20); // -10000 penalty
        assert_eq!(pen, -10_000);
        assert_eq!(w.get_relation(1, 7), -100, "penalty clamps the relation to the floor");
        assert_eq!(w.pursuit_level(1), 1);
    }

    /// The reflection components are real ECS components usable in a `mercs2_core::World`.
    #[test]
    fn components_spawn_into_world() {
        use mercs2_core::World;
        let mut world = World::new();
        let e = world.spawn((
            FactionMarker { faction_id: 3 },
            FactionZone { zone_faction_id: 3 },
            Suspect::default(),
        ));
        assert_eq!(world.get::<&FactionMarker>(e).unwrap().faction_id, 3);
        assert_eq!(world.get::<&Suspect>(e).unwrap().per_faction, [0i32; 8]);
    }
}

// A tiny helper so the doc-ish test above reads clearly; the PMC guid is arbitrary in unit tests.
#[cfg(test)]
#[allow(non_snake_case)]
fn PMC_placeholder() -> u32 {
    0xdead_beef
}
