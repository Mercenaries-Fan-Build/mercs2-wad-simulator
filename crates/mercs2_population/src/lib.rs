//! `mercs2_population` — Population / spawners.
//!
//! **Code map:** `docs/reverse_engineer/population_spawner_code_map.md` (the recovered
//! `PgSysPopulation` runtime, married Xbox↔PC), plus the component census in
//! `docs/mercs2-ecs/02_ai_perception_population.md` and `ai_code_map.md` §3.
//! **Owned Lua namespace(s):** the population verbs on `Ai` (`TweakAttachedSpawners`, `SetSpawnList`, …)
//! and `Pg` (`StartHeliWaveSpawner`, `SetSkirmishTable`, …) — driven via the game's `EngineHost` seam.
//!
//! Per the code map's §11 reimpl disposition, this crate supplies the **mechanism the engine owns** —
//! the per-frame `PgSysPopulation::Update` fan-out (PC `FUN_00502510`) reduced to the pieces that are
//! actually recovered as native code:
//!
//! - [`death`] — the budgeted `DeathCheck`/`DeathCompute` retirement of aged-out bodies (§4, H);
//! - [`density`] — the `>>1 & 0x7f7f7f7f` spawn-history decay + the `10/10/2/2` ambient budgets +
//!   the `TrafficControlEnum` gate (§3/§9, H constants);
//! - [`spawner`] — the four-family `UpdateSimpleSpawners` mechanism over a 768-cap instance pool with
//!   cap-128 family queues, emitting [`SpawnRequest`]s, plus `TweakAttachedSpawners` apply (§6, H);
//! - [`components`] — the population reflection components (hashes/strides/defaults/enums, §9).
//!
//! # Entry point
//!
//! [`PopulationWorld`] is the host-owned bundle of those three sub-mechanisms.
//! [`tick`](PopulationWorld::tick) runs the per-fixed-step update; it **never mutates the entity world
//! itself** — it stages [`SpawnRequest`]s and retired [`mercs2_core::Entity`]s, which the caller drains
//! with [`take_requests`](PopulationWorld::take_requests) /
//! [`take_retired`](PopulationWorld::take_retired) and realizes/despawns on the other side of the
//! resolver seam. [`tweak_attached_spawners`](PopulationWorld::tweak_attached_spawners) is the
//! script-facing lever (`Ai.TweakAttachedSpawners`). `mercs2_engine` re-exports this crate as
//! `mercs2_engine::population` and holds a `PopulationWorld` in its runtime and script host.
//!
//! The ambient-density half is **queried, not ticked**: [`DensityController`] answers *how many* units
//! of a class a zone may emit this frame (`min(headroom, per-tick budget)`, traffic-gated), because
//! that is the granularity the code map actually recovered.
//!
//! # Module map
//!
//! | Module | Owns |
//! | --- | --- |
//! | [`death`] | [`DeathQueue`] / [`PendingDeath`], [`DEATH_BUDGET_PER_FRAME`] (20/frame), [`DEATH_DISTANCE_SQ_TABLE`] (9 squared radii). |
//! | [`density`] | [`DensityController`] / [`DensityBudget`] (`10/10/2/2`), [`decay_spawn_history`], [`traffic_allows`], [`density_faction_participates`]. |
//! | [`spawner`] | [`SimpleSpawnerManager`] / [`SimpleSpawner`] / [`SpawnQueue`] / [`SpawnRequest`] / [`SpawnerAdjust`] / [`SpawnerFamily`] + the recovered caps ([`SIMPLE_SPAWNER_POOL`] 768, [`SPAWN_QUEUE_CAP`] 128, [`SPAWNER_GROUP_COUNT`] 8, [`SPAWNER_STATE_TERMINAL`] 5). |
//! | [`components`] | [`PopulationDensity`], [`PopulationDynamicRoad`], [`PopulationFlow`], [`SkirmishZone`], [`SkirmishSpawnList`], [`SocialUse`], [`RtPopMembership`], [`RuntimeTravelGroup`] + the [`TrafficControl`] / [`DynamicRoadType`] / [`FlowControlType`] / [`NeedType`] enums and the [`SpawnFaction`] spawn-list channels. |
//!
//! **What this crate deliberately does NOT build** (data / Lua / unrecovered per the code map):
//! the main ambient driver body `FUN_00503020` (unread) and the per-player "best-priority containing
//! region" spatial select (a data query) — a zone's density counters are consumed as caps, region
//! containment is the caller's; the **terminal spawn worker** `0x24F3200` (SecuROM-VM dispatched — the
//! seam that turns a [`SpawnRequest`] into an entity, handled outside this crate); the 9 binding-table-
//! only cfunc bodies (§7, undecompiled); the CacheIn/CacheOut kept-ring (streaming-coupled, capacity
//! 64-vs-8 confirm-live §5). Those are represented as inputs/seams, never as invented bodies.

pub mod components;
pub mod death;
pub mod density;
pub mod slot_table;
pub mod spawner;

pub use components::{
    DynamicRoadType, FlowControlType, NeedType, PopulationDensity, PopulationDynamicRoad,
    PopulationFlow, RtPopMembership, RuntimeTravelGroup, SkirmishSpawnList, SkirmishZone, SocialUse,
    SpawnFaction, TrafficControl,
};
pub use death::{DeathQueue, PendingDeath, DEATH_BUDGET_PER_FRAME, DEATH_DISTANCE_SQ_TABLE};
pub use density::{
    decay_spawn_history, density_faction_participates, traffic_allows, DensityBudget,
    DensityController,
};
pub use slot_table::{faction_template_hash, faction_template_name, FACTION_TEMPLATE_NAMES};
pub use spawner::{
    SimpleSpawner, SimpleSpawnerManager, SpawnQueue, SpawnRequest, SpawnerAdjust, SpawnerFamily,
    SIMPLE_SPAWNER_POOL, SPAWNER_GROUP_COUNT, SPAWNER_STATE_TERMINAL, SPAWN_QUEUE_CAP,
};

use mercs2_core::glam::Vec3;
use mercs2_core::{Time, World};

/// The `Event.Post` hash the spawn pipeline fires when a unit spawns (PC `FUN_004b7ab0(0x7962caf5,…)`,
/// code map §9). Carried so the reimpl's spawn resolver can post the same event the game does.
pub const SPAWN_EVENT_HASH: u32 = 0x7962_caf5;

/// Default ambient live-population ceiling the density gate throttles against.
///
/// **CONFIRM-LIVE (region cap):** the *faithful* per-region ceiling is the WAD-authored **desired
/// count** the game stores per player in `DAT_00ed55c8[]` / `DAT_00ed55b0[]`, written by region-select
/// (`FUN_004d8490`→`FUN_004d60e0`) from the `PopulationDensity` COMP records
/// (`docs/reverse_engineer/render_distance_and_density_levers.md` §1–2: the ambient rule is literally
/// `desired − live > 0 → spawn`, and the `10/10/2/2` per-tick budgets only fill that deficit *faster*,
/// they do not raise the ceiling). That desired-count data is not statically recoverable (it ships in
/// the vz_state overlays; COMP extraction is unimplemented). Until that feed exists, bound live ambient
/// actors by the recovered `PopulationSimpleSpawner` pool ceiling ([`SIMPLE_SPAWNER_POOL`] = 768,
/// `cdbsizes.ini`) so emission halts at a **recovered counter** instead of running away. Callers with a
/// real desired count set [`PopulationWorld::live_cap`] per zone.
pub const POP_LIVE_CAP: u32 = SIMPLE_SPAWNER_POOL; // CONFIRM-LIVE: exact per-region cap = WAD desired count

/// The host-owned population mechanism — the `PgSysPopulation` state the fixed schedule ticks. Bundles
/// the three recovered sub-mechanisms (death retirement, ambient density budgeting, simple spawners)
/// the way [`crate`]'s doc describes the `FUN_00502510` fan-out. The game's `EngineHost` forwards the
/// population Lua verbs here; [`tick`](Self::tick) runs the per-frame update, and the resolver seam
/// drains [`take_requests`](Self::take_requests).
pub struct PopulationWorld {
    /// The budgeted death-retirement queue (`DeathCheck`/`DeathCompute`).
    pub deaths: DeathQueue,
    /// The ambient-density budget + spawn-history decay + traffic gate.
    pub density: DensityController,
    /// The 768-cap simple-spawner pool + four cap-128 family queues.
    pub spawners: SimpleSpawnerManager,
    /// The live ambient-population ceiling the density gate throttles emissions against — the reimpl's
    /// analogue of the game's per-player WAD **desired count** (`DAT_00ed55c8[]`/`ed55b0[]`). Defaults
    /// to [`POP_LIVE_CAP`]; a caller with a real per-zone desired count overwrites it. Emission stops
    /// entirely once [`live`](Self::live) reaches this — the anti-runaway ceiling.
    pub live_cap: u32,
    /// Spawn requests emitted this tick, awaiting the resolver seam (drained by [`take_requests`]).
    ///
    /// [`take_requests`]: PopulationWorld::take_requests
    requests: Vec<SpawnRequest>,
    /// Entities retired this tick by the death system, awaiting despawn by the caller.
    retired: Vec<mercs2_core::Entity>,
    /// Current live ambient-population count — the reimpl's analogue of the game's live-count DATs
    /// (`DAT_016d3078` &c.): incremented by each request emitted (the caller realizes them 1:1),
    /// decremented as the death system retires bodies. `desired − live` (i.e. `live_cap − live`) is the
    /// headroom the density gate emits toward, exactly as the ambient rule `desired − live > 0 → spawn`
    /// does (`render_distance_and_density_levers.md` §2). Tracked here rather than by scanning the ECS
    /// because the game itself keeps explicit counters, and no faithful "ambient" ECS tag exists.
    live: u32,
    /// Packed 4-byte spawn-history accumulator, decayed each tick by the recovered `>>1 & 0x7f7f7f7f`
    /// op ([`decay_spawn_history`]) so a zone that spawned recently ages out of its throttle (code map
    /// §3 density-decay step; the accumulator's role in the WAD region-select throttle is data /
    /// CONFIRM-LIVE, so it is recorded + decayed here, not used to fabricate a stronger gate).
    spawn_history: u32,
    /// One-shot diagnostic latch: log the spawner-proximity picture (total / in-range / nearest) the
    /// first tick with a viewport, so a live boot shows whether the activation-radius gate found any
    /// spawner near the player or excluded them all. Debug observability, not game state.
    diag_done: bool,
}

impl Default for PopulationWorld {
    fn default() -> Self {
        PopulationWorld {
            deaths: DeathQueue::default(),
            density: DensityController::default(),
            spawners: SimpleSpawnerManager::default(),
            live_cap: POP_LIVE_CAP,
            requests: Vec::new(),
            retired: Vec::new(),
            live: 0,
            spawn_history: 0,
            diag_done: false,
        }
    }
}

impl PopulationWorld {
    pub fn new() -> Self {
        PopulationWorld::default()
    }

    /// Current live ambient-population count the density gate throttles against (see the [`live`] field).
    /// `live_cap − live` is the emission headroom.
    ///
    /// [`live`]: PopulationWorld::live
    pub fn live(&self) -> u32 {
        self.live
    }

    /// `PgSysPopulation::Update` (PC `FUN_00502510`) — the per-fixed-step population tick, mirroring the
    /// recovered fan-out order (§3): **death check/compute → (density decay, folded into the density
    /// controller) → simple-spawner families → spawn-queue drain**. `viewports` are the camera anchors
    /// the death distance gate measures against. Retired entities and spawn requests are staged for the
    /// caller to drain ([`take_retired`](Self::take_retired) / [`take_requests`](Self::take_requests));
    /// the actual entity despawn/spawn is the resolver seam, not owned here.
    ///
    /// Idle-safe: with no pending deaths and no registered spawners it does nothing, the same
    /// data-driven way the sibling systems idle until their content exists.
    pub fn tick(&mut self, _world: &mut World, time: &Time, viewports: &[Vec3]) {
        // 1. Death check/compute — retire aged-out, far bodies (budget 20/frame). Retiring frees live
        //    headroom, exactly as the game decrements its live-count DATs (`DAT_016d3078` &c.) on death,
        //    so a saturated zone reopens for spawning as bodies age out and leave.
        let retired = self.deaths.check(time.dt, viewports);
        self.live = self.live.saturating_sub(retired.len() as u32);
        self.retired.extend(retired);
        // 2. Spawn-history decay — the recovered per-byte halving `>>1 & 0x7f7f7f7f` applied each tick
        //    (code map §3 density-decay step) so recent-spawn history ages out of its throttle.
        self.spawn_history = decay_spawn_history(self.spawn_history);
        // 3. Simple-spawner families — advance timers, enqueue fired requests to the cap-128 queues.
        //    Gated by the recovered activation-radius test (`UpdateSimpleSpawners`, §6): only spawners
        //    within a `viewport`'s activation radius fire — the rest are cached-out and hold their
        //    countdown. This is the PROXIMITY half (WHICH spawners fire, clustered around the player);
        //    it composes with the density gate below (HOW MANY requests are let through per tick).
        self.spawners.update(time.dt, viewports);
        // One-shot proximity diagnostic: how many of the registered spawners sit within their
        // activation radius of the player, and how far the NEAREST one is — the datum that says whether
        // the gate is correctly clustering or excluding everything at this location.
        if !self.diag_done {
            if let Some(vp) = viewports.first() {
                self.diag_done = true;
                let sp = self.spawners.spawners();
                let mut nearest = f32::MAX;
                let mut in_range = 0u32;
                for s in sp {
                    let t = s.transform.translation;
                    let d = ((t.x - vp.x).powi(2) + (t.z - vp.z).powi(2)).sqrt();
                    nearest = nearest.min(d);
                    if s.within_activation_radius(viewports) {
                        in_range += 1;
                    }
                }
                println!(
                    "[pop-diag] {} spawners registered; {in_range} within activation radius of focus; nearest spawner {:.0} m away (focus {:.0},{:.0},{:.0})",
                    sp.len(), if nearest.is_finite() { nearest } else { -1.0 }, vp.x, vp.y, vp.z
                );
            }
        }
        // 4. Density-gated drain — THE anti-runaway gate. The recovered ambient rule is spawn-toward-
        //    desired (`render_distance_and_density_levers.md` §2: `desired − live > 0 → spawn`): emit at
        //    most `min(per-tick budget, headroom)`, where headroom = `live_cap − live` (the WAD desired
        //    count minus the live count) and the per-tick budget is the recovered `10 people / 2
        //    vehicles` fill RATE (§"Spawn PLACEMENT + density", Xbox `FUN_82367d28` = 10/10/2/2) — which
        //    only fills the deficit faster, it does NOT raise the ceiling. A saturated population
        //    (`live ≥ live_cap`) emits 0 → no runaway; requests over the gate are refused this tick
        //    (they are not realized), the same way the game's spawn-budget gate declines them.
        let mut headroom = self.live_cap.saturating_sub(self.live); // hard desired-count ceiling
        let mut ped_left = self.density.per_tick_emit_budget(false); // 10 people/tick fill rate
        let mut veh_left = self.density.per_tick_emit_budget(true); //  2 vehicles/tick fill rate
        let mut emitted = 0u32;
        for req in self.spawners.drain_requests() {
            if headroom == 0 {
                break; // desired count reached — refuse the remaining requests this tick
            }
            let class_left = if req.faction.is_vehicle() { &mut veh_left } else { &mut ped_left };
            if *class_left == 0 {
                continue; // this class's per-tick fill rate is spent; a later tick may emit it
            }
            *class_left -= 1;
            headroom -= 1;
            // Record the emission into the decaying spawn-history accumulator (saturating; the low byte
            // is what `decay_spawn_history` halves back down over subsequent ticks).
            self.spawn_history = self.spawn_history.saturating_add(1);
            self.requests.push(req);
            emitted += 1;
        }
        self.live += emitted;
    }

    /// Drain the spawn requests emitted so far — the resolver seam realizes each into an entity (and
    /// posts [`SPAWN_EVENT_HASH`]). Empties the staging buffer.
    pub fn take_requests(&mut self) -> Vec<SpawnRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Drain the entities the death system retired — the caller despawns them from the `World`.
    pub fn take_retired(&mut self) -> Vec<mercs2_core::Entity> {
        std::mem::take(&mut self.retired)
    }

    /// `Ai.TweakAttachedSpawners` / `…InGroup` — the primary script-facing lever (§7). Applies a
    /// [`SpawnerAdjust`] over the 8-group bit loop; returns how many spawners it touched.
    pub fn tweak_attached_spawners(&mut self, adjust: &SpawnerAdjust) -> u32 {
        self.spawners.apply_adjust(adjust)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_core::Transform;

    #[test]
    fn spawn_event_hash_is_recovered_constant() {
        assert_eq!(SPAWN_EVENT_HASH, 0x7962_caf5);
    }

    /// A registered spawner fires through the full tick and its request surfaces via `take_requests`,
    /// proving the `Update` fan-out (spawner update → queue drain) end to end.
    #[test]
    fn tick_emits_spawn_requests_from_registered_spawners() {
        let mut world = World::new();
        let mut pop = PopulationWorld::new();
        pop.spawners
            .register(SimpleSpawner {
                interval: 1.0,
                countdown: 1.0,
                reload: 1.0,
                faction: SpawnFaction::Vz,
                family: SpawnerFamily::Window,
                transform: Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
                ..SimpleSpawner::default()
            })
            .unwrap();

        let time = Time::new(1.0); // dt = 1.0s → one tick crosses the 1.0s interval
        pop.tick(&mut world, &time, &[Vec3::ZERO]);
        // The clock's dt is 1.0 (fixed_dt for 1 Hz); the spawner's countdown 1.0 - 1.0 <= 0 fires.
        let reqs = pop.take_requests();
        assert_eq!(reqs.len(), 1, "the window spawner fired once");
        assert_eq!(reqs[0].faction, SpawnFaction::Vz);
        assert_eq!(reqs[0].transform.translation, Vec3::new(10.0, 0.0, 0.0));
        assert!(pop.take_requests().is_empty(), "requests drained");
    }

    /// **Activation-radius clustering fix.** A spawner far from every viewport emits nothing through
    /// the full tick (cached-out); once a viewport comes within its activation radius it fires. This is
    /// the recovered `UpdateSimpleSpawners` proximity gate (§6) — ambient actors spawn near the hero,
    /// not scattered across the whole ~1.3 km × 5 km map (the live bug).
    #[test]
    fn tick_gates_spawners_by_viewport_proximity() {
        let mut world = World::new();
        let mut pop = PopulationWorld::new();
        pop.spawners
            .register(SimpleSpawner {
                interval: 1.0,
                countdown: 1.0,
                reload: 1.0,
                faction: SpawnFaction::Ped,
                family: SpawnerFamily::Window,
                // 2 km from the origin — far outside the 160 m Window activation radius.
                transform: Transform::from_translation(Vec3::new(2000.0, 0.0, 0.0)),
                ..SimpleSpawner::default()
            })
            .unwrap();
        let time = Time::new(1.0); // dt 1.0 would cross the 1.0s interval — IF the spawner were active.

        // Hero far away: the spawner is cached-out — nothing fires no matter how long we wait, and its
        // countdown does not drain.
        for _ in 0..5 {
            pop.tick(&mut world, &time, &[Vec3::ZERO]);
            assert!(pop.take_requests().is_empty(), "far spawner stays cached-out");
        }

        // Hero moves within the 160 m Window radius (50 m from the spawner): it resumes and fires.
        pop.tick(&mut world, &time, &[Vec3::new(1950.0, 0.0, 0.0)]);
        let reqs = pop.take_requests();
        assert_eq!(reqs.len(), 1, "in-range spawner fires");
        assert_eq!(reqs[0].transform.translation, Vec3::new(2000.0, 0.0, 0.0));
    }

    /// **Anti-runaway guarantee.** With a stationary viewport and many always-firing spawners (the
    /// live repro: 29 spawners firing every tick), the population climbs to the desired-count ceiling
    /// and STAYS there — it does not realize N new actors every tick forever. Before the density gate
    /// was wired, `tick` drained every request unconditionally → +29 actors/tick, unbounded.
    #[test]
    fn population_stabilizes_at_bounded_live_count_under_constant_spawn_pressure() {
        let mut world = World::new();
        let mut pop = PopulationWorld::new();
        // A small stand-in for the WAD per-region desired count so saturation is reached quickly. The
        // gate is identical at the recovered 768 default; the cap is data (see `POP_LIVE_CAP`).
        pop.live_cap = 25;
        // 29 always-firing spawners — the exact count from the live runaway report.
        for i in 0..29u8 {
            pop.spawners
                .register(SimpleSpawner {
                    interval: 1.0,
                    countdown: 1.0,
                    reload: 1.0,
                    faction: SpawnFaction::Ped,
                    family: SpawnerFamily::NoModel,
                    group: i % SPAWNER_GROUP_COUNT,
                    transform: Transform::from_translation(Vec3::new(i as f32, 0.0, 0.0)),
                    ..SimpleSpawner::default()
                })
                .unwrap();
        }
        let time = Time::new(1.0); // dt 1.0 crosses each 1.0s interval → all 29 fire every tick

        // Drive many ticks, draining emitted requests each tick as the runtime caller would.
        for _ in 0..200 {
            pop.tick(&mut world, &time, &[Vec3::ZERO]);
            let _ = pop.take_requests(); // realized by the caller; `live` already tracks the count
        }
        // Bounded: never exceeds the desired-count ceiling despite 29 spawners firing every tick.
        assert!(
            pop.live() <= 25,
            "live population must not exceed the desired-count cap; got {}",
            pop.live()
        );
        // Saturated AND stable: filled to the ceiling, and a further tick emits nothing.
        assert_eq!(pop.live(), 25, "population fills to the desired-count ceiling");
        pop.tick(&mut world, &time, &[Vec3::ZERO]);
        assert!(
            pop.take_requests().is_empty(),
            "a saturated population emits zero — the anti-runaway gate holds"
        );
        assert_eq!(pop.live(), 25, "and stays at the ceiling, not +29/tick");
    }

    /// The gate throttles, it does not permanently stop: as the death system retires bodies, live
    /// headroom frees and emission resumes the very next tick (up to the per-tick fill rate).
    #[test]
    fn retiring_bodies_reopens_spawn_headroom() {
        let mut world = World::new();
        let mut pop = PopulationWorld::new();
        pop.live_cap = 5;
        // Enough spawners that supply is never the limit (8 fire every tick).
        for i in 0..8u8 {
            pop.spawners
                .register(SimpleSpawner {
                    interval: 1.0,
                    countdown: 1.0,
                    reload: 1.0,
                    faction: SpawnFaction::Ped,
                    family: SpawnerFamily::NoModel,
                    group: i % SPAWNER_GROUP_COUNT,
                    ..SimpleSpawner::default()
                })
                .unwrap();
        }
        let time = Time::new(1.0);
        // Saturate to the cap.
        for _ in 0..10 {
            pop.tick(&mut world, &time, &[Vec3::ZERO]);
            let _ = pop.take_requests();
        }
        assert_eq!(pop.live(), 5, "saturated at the ceiling");
        pop.tick(&mut world, &time, &[Vec3::ZERO]);
        assert!(pop.take_requests().is_empty(), "no headroom → no emission");

        // Retire two far, expired bodies via the death queue → live drops, headroom frees.
        let a = world.spawn(());
        let b = world.spawn(());
        pop.deaths.push(PendingDeath { entity: a, timer: 0.0, gate: 0, position: Vec3::new(1000.0, 0.0, 0.0) });
        pop.deaths.push(PendingDeath { entity: b, timer: 0.0, gate: 0, position: Vec3::new(1000.0, 0.0, 0.0) });
        pop.tick(&mut world, &time, &[Vec3::ZERO]);
        assert_eq!(pop.take_retired(), vec![b, a], "both far+expired bodies retired");
        // Spawning resumed: the freed headroom (2) was refilled this same tick, back to the ceiling.
        assert_eq!(pop.take_requests().len(), 2, "emission resumes into the freed headroom");
        assert_eq!(pop.live(), 5, "refilled to the ceiling — throttled, not stopped");
    }

    /// The death half of the tick retires a far, expired body and surfaces it via `take_retired`.
    #[test]
    fn tick_retires_dead_bodies() {
        let mut world = World::new();
        let body = world.spawn(());
        let mut pop = PopulationWorld::new();
        pop.deaths.push(PendingDeath {
            entity: body,
            timer: 0.0,
            gate: 0,
            position: Vec3::new(1000.0, 0.0, 0.0),
        });
        let time = Time::new(60.0);
        pop.tick(&mut world, &time, &[Vec3::ZERO]);
        assert_eq!(pop.take_retired(), vec![body]);
    }

    /// `tweak_attached_spawners` drives the group bit loop from the world bundle.
    #[test]
    fn tweak_attached_spawners_routes_to_manager() {
        let mut pop = PopulationWorld::new();
        pop.spawners
            .register(SimpleSpawner { group: 2, ..SimpleSpawner::default() })
            .unwrap();
        let touched = pop.tweak_attached_spawners(&SpawnerAdjust {
            group_mask: 1 << 2,
            spawner_state: Some(SPAWNER_STATE_TERMINAL),
            ..SpawnerAdjust::default()
        });
        assert_eq!(touched, 1);
        assert!(pop.spawners.spawners()[0].is_terminal());
    }

    /// An empty population world ticks without doing anything (idle-safe).
    #[test]
    fn empty_world_tick_is_noop() {
        let mut world = World::new();
        let mut pop = PopulationWorld::new();
        let time = Time::new(60.0);
        pop.tick(&mut world, &time, &[]);
        assert!(pop.take_requests().is_empty());
        assert!(pop.take_retired().is_empty());
    }
}
