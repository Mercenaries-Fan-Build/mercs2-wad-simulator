//! `mercs2_population` — Population / spawners (scoreboard row 24).
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
pub use spawner::{
    SimpleSpawner, SimpleSpawnerManager, SpawnQueue, SpawnRequest, SpawnerAdjust, SpawnerFamily,
    SIMPLE_SPAWNER_POOL, SPAWNER_GROUP_COUNT, SPAWNER_STATE_TERMINAL, SPAWN_QUEUE_CAP,
};

use mercs2_core::glam::Vec3;
use mercs2_core::{Time, World};

/// The `Event.Post` hash the spawn pipeline fires when a unit spawns (PC `FUN_004b7ab0(0x7962caf5,…)`,
/// code map §9). Carried so the reimpl's spawn resolver can post the same event the game does.
pub const SPAWN_EVENT_HASH: u32 = 0x7962_caf5;

/// The host-owned population mechanism — the `PgSysPopulation` state the fixed schedule ticks. Bundles
/// the three recovered sub-mechanisms (death retirement, ambient density budgeting, simple spawners)
/// the way [`crate`]'s doc describes the `FUN_00502510` fan-out. The game's `EngineHost` forwards the
/// population Lua verbs here; [`tick`](Self::tick) runs the per-frame update, and the resolver seam
/// drains [`take_requests`](Self::take_requests).
#[derive(Default)]
pub struct PopulationWorld {
    /// The budgeted death-retirement queue (`DeathCheck`/`DeathCompute`).
    pub deaths: DeathQueue,
    /// The ambient-density budget + spawn-history decay + traffic gate.
    pub density: DensityController,
    /// The 768-cap simple-spawner pool + four cap-128 family queues.
    pub spawners: SimpleSpawnerManager,
    /// Spawn requests emitted this tick, awaiting the resolver seam (drained by [`take_requests`]).
    ///
    /// [`take_requests`]: PopulationWorld::take_requests
    requests: Vec<SpawnRequest>,
    /// Entities retired this tick by the death system, awaiting despawn by the caller.
    retired: Vec<mercs2_core::Entity>,
}

impl PopulationWorld {
    pub fn new() -> Self {
        PopulationWorld::default()
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
        // 1. Death check/compute — retire aged-out, far bodies (budget 20/frame).
        self.retired.extend(self.deaths.check(time.dt, viewports));
        // 2. Simple-spawner families — advance timers, enqueue fired requests to the cap-128 queues.
        self.spawners.update(time.dt);
        // 3. Post-update spawn-queue drain — pull the deferred requests for the resolver.
        self.requests.extend(self.spawners.drain_requests());
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
