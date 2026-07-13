# mercs2_population

The population / spawner system of the Mercenaries 2 reimplementation (scoreboard row 24): the
`PgSysPopulation` per-frame tick — budgeted death retirement, ambient crowd/traffic density budgeting,
and the four-family simple-spawner pool that emits spawn requests.

## What it is

A pure-simulation library. It owns the *mechanism* the original engine owns, and stops at the seams
where the original hands off to data or to code that was never recovered.

`PopulationWorld` bundles the three recovered sub-mechanisms and ticks them in the fan-out order the
code map records for `PgSysPopulation::Update` (PC `FUN_00502510`):

1. **Death check/compute** (`death`) — a pending-death list with a **20 entries/frame** budget and a
   per-viewport squared-distance gate. A body retires only when its timer has run out **and** it is
   beyond its gate radius from *every* viewport; an expired-but-nearby body stays queued.
2. **Simple spawners** (`spawner`) — a **768**-instance pool (the `cdbsizes.ini` cap) fanned out over
   four family updaters (Window / NoModel / Hardpoint / Path), each with its own **128**-cap pending
   queue. A spawner is a countdown timer; on elapse it emits one `SpawnRequest` and reloads.
3. **Spawn-queue drain** — the queued requests are pulled out for the caller's spawn resolver.

`density` supplies the ambient half: the per-tick budget (`10/10/2/2` people-far / people-near /
vehicles-far / vehicles-near), the `>>1 & 0x7f7f7f7f` per-byte spawn-history decay, and the
`TrafficControlEnum` gate (`Default` / `NoTraffic` / `NoVehicles` / `NoPeds` / `BanFaction` /
`SingleFaction`) that decides whether a given faction channel may spawn in a zone.

`components` carries the population reflection components with their m2 hashes, record strides and
recovered field defaults: `PopulationDensity`, `PopulationDynamicRoad`, `PopulationFlow`,
`SkirmishZone`, `SkirmishSpawnList`, `SocialUse`, `RtPopMembership`, `RuntimeTravelGroup`, plus the
eight spawn-list channels (`SpawnFaction`: VZ / Pir / Oil / Gur / Chi / Ali / Ped + the shared
`VehicleSpawnList`).

The crate never spawns or despawns entities itself. `tick` stages `SpawnRequest`s and retired
`Entity`s; the caller drains them with `take_requests()` / `take_retired()` and performs the actual
world mutation. `mercs2_engine` re-exports the crate as `mercs2_engine::population` and holds a
`PopulationWorld` in its runtime and script host, which is where the population Lua verbs land.

## Where it comes from

Provenance as the source itself states it:

| Piece | Oracle |
| --- | --- |
| Overall system | `docs/reverse_engineer/population_spawner_code_map.md` (the recovered `PgSysPopulation` runtime, married Xbox↔PC), plus the component census in `docs/mercs2-ecs/02_ai_perception_population.md` and `ai_code_map.md` §3. |
| Per-frame fan-out | `PgSysPopulation::Update`, PC `FUN_00502510` (code map §3). |
| Death system | PC `FUN_00500b40` (DeathCheck) → `FUN_00500ac0` (DeathCompute); pending list `DAT_00dd12e8` / count `DAT_00dd13ec`, timers `DAT_017959d0[i*0xc]`. Budget **20/frame** from the Xbox `li 0x14`; the 9-entry squared-distance table from Xbox `FUN_8235efc0` (code map §4). |
| Density | Spawn-history decay `(w >> 1) & 0x7f7f7f7f` = PC `FUN_004e0510`; per-tick budgets `10/10/2/2` = Xbox `FUN_82367d28` (code map §9). Faction types 7 & 8 are skipped by the density loop. |
| Simple spawners | `UpdateSimpleSpawners`, Xbox `FUN_82338768` ↔ PC `FUN_004e4100`; register `FUN_004e4620`; family procs `FUN_004e1590/1ad0/2110/1d50` draining queues `DAT_00dccb00/ce30/cfd0/d300` (Xbox lists `0x82C1F488/7B8/958/C88`). Class-manager `@0x00DF8510`. Instance field layout `+0x20 … +0x8c` recovered from the consumers (code map §6). |
| Pool caps | `cdbsizes.ini`: `PopulationSimpleSpawner` 768, `PopulationList` 1024, `SpawnerAdjust` 16/16, `SpawnOnDeath` 384/128 (PC image strings). |
| Component hashes/strides | The ECS census (`desc+0x24` stride, pool budget); each `*_HASH` is asserted equal to `mercs2_formats::hash::pandemic_hash_m2(ClassName)` in the crate's own tests. |
| Enums | Recovered from exe `.rdata`: `TrafficControlEnum` @`0xbc75fc` (6), `DynamicRoadTypeEnum` @`0xbc675c` (2), `FlowControlTypeEnum` @`0xbc6734` (2), `NeedTypeEnum` @`0xbc6118` (5). |
| `SPAWN_EVENT_HASH` | `0x7962caf5` — the `Event.Post` hash the spawn pipeline fires, PC `FUN_004b7ab0(0x7962caf5, …)` (code map §9). |

**Deliberately not built** (per the code map's §11 reimpl disposition): the main ambient driver body
`FUN_00503020` (unread), the per-player "best-priority containing region" spatial select (a data
query), the terminal spawn worker `0x24F3200` (SecuROM-VM dispatched — the seam that turns a
`SpawnRequest` into an entity), the 9 binding-table-only cfunc bodies (§7), and the CacheIn/CacheOut
kept-ring (streaming-coupled). These are represented as inputs and seams, never as invented bodies.

## Usage

```rust
use mercs2_population::{
    PopulationWorld, SimpleSpawner, SpawnFaction, SpawnerAdjust, SpawnerFamily,
    SPAWNER_STATE_TERMINAL,
};
use mercs2_core::glam::Vec3;
use mercs2_core::{Time, Transform, World};

let mut world = World::new();
let mut pop = PopulationWorld::new();

// Register a window spawner: fires every 1.0s on the VZ spawn-list channel, group 3.
pop.spawners
    .register(SimpleSpawner {
        interval: 1.0,
        countdown: 1.0,
        reload: 1.0,
        faction: SpawnFaction::Vz,
        family: SpawnerFamily::Window,
        group: 3,
        transform: Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ..SimpleSpawner::default()
    })
    .unwrap();

// Tick the system. `viewports` are the camera anchors the death distance gate measures against.
let time = Time::new(60.0);
pop.tick(&mut world, &time, &[Vec3::ZERO]);

// The resolver seam: realize each request into an entity (and post SPAWN_EVENT_HASH).
for req in pop.take_requests() {
    let _ = (req.template, req.transform, req.faction, req.group, req.family);
}
// The caller despawns whatever the death system retired.
for e in pop.take_retired() {
    let _ = world.despawn(e);
}

// `Ai.TweakAttachedSpawners` — the 8-group bit loop; here: terminate group 3 only.
let touched = pop.tweak_attached_spawners(&SpawnerAdjust {
    group_mask: 1 << 3,
    spawner_state: Some(SPAWNER_STATE_TERMINAL),
    ..SpawnerAdjust::default()
});
assert_eq!(touched, 1);
```

The ambient-density half is queried rather than ticked — it answers *how many* of a class a zone may
emit this frame:

```rust
use mercs2_population::{
    decay_spawn_history, traffic_allows, DensityController, PopulationDensity, SpawnFaction,
    TrafficControl,
};

let ctl = DensityController::new();                       // budget defaults to 10/10/2/2
let zone = PopulationDensity { traffic: TrafficControl::NoVehicles, ..Default::default() };

assert_eq!(ctl.ped_allowance(&zone, 100, 0, true, SpawnFaction::Ped), 10); // min(headroom, budget)
assert_eq!(ctl.vehicle_allowance(&zone, 100, 0, true), 0);                 // gated off by the zone
assert!(!traffic_allows(TrafficControl::NoVehicles, SpawnFaction::Vehicle, None, None));
assert_eq!(decay_spawn_history(0xFFFF_FFFF), 0x7F7F_7F7F);                 // per-byte halving
```

## Modules

| Module | Owns |
| --- | --- |
| (crate root) | `PopulationWorld` — the host-owned bundle (`deaths` / `density` / `spawners`), the `tick` fan-out, the `take_requests` / `take_retired` drains, `tweak_attached_spawners`, and `SPAWN_EVENT_HASH`. |
| `death` | `DeathQueue` / `PendingDeath` + `DEATH_BUDGET_PER_FRAME` (20) and `DEATH_DISTANCE_SQ_TABLE` (9 squared radii) — the round-robin, distance-gated retirement of aged-out bodies. |
| `density` | `DensityController` / `DensityBudget` + `decay_spawn_history`, `traffic_allows`, `density_faction_participates` — the per-tick ambient budgets, the spawn-history decay op, and the traffic-control gate. |
| `spawner` | `SimpleSpawnerManager` / `SimpleSpawner` / `SpawnQueue` / `SpawnRequest` / `SpawnerAdjust` / `SpawnerFamily` + the recovered caps (`SIMPLE_SPAWNER_POOL` 768, `SPAWN_QUEUE_CAP` 128, `SPAWNER_GROUP_COUNT` 8, `SPAWNER_STATE_TERMINAL` 5, `WINDOW_RADIUS_SQ` 160²). |
| `components` | The population reflection components with their m2 hashes, strides, pool budgets and recovered defaults, plus the `TrafficControl` / `DynamicRoadType` / `FlowControlType` / `NeedType` enums and the `SpawnFaction` spawn-list channels. |

## Notes / gotchas

* **The crate emits requests; it does not spawn.** The terminal commit (`SpawnRequest` → entity) is the
  SecuROM-VM-dispatched worker `0x24F3200` in the original — a seam handled by the caller's resolver.
  Same on the other side: `tick` stages retired entities, the caller despawns them.
* **Confirm-live items are flagged in the source, not smoothed over.**
  * The death distance table's numeric constants were read on **Xbox** (`FUN_8235efc0`); the PC gate
    uses the same distance-select logic but the values were not read out on PC (§10.4).
  * The PC family-proc ↔ queue ↔ family pairing is by size/position, **not proven** — the four procs
    are structurally interchangeable. `Window` is the safest anchor (its Xbox fn carries the
    `'Have %d window Spawners'` string and the `OccupiedBuildingSpawnCallback` dispatch) (§10.2).
  * The fold from the 3 raw type discriminators (`+0x63/+0x64/+0x68`) to the 4-member
    `SimpleSpawnerTypeEnum` is confirm-live, so `SimpleSpawner::type_disc` is kept raw (§10.2).
* **`PopulationDensity::caps` is five opaque ints.** The exact per-slot meaning lives in the unread
  ambient driver `FUN_00503020`, so the columns are carried verbatim and consumed as caps by
  `DensityController`. Likewise `SkirmishSpawnList::slots` is six raw ints — the engine mechanism picks
  *a slot*, the *meaning* is content-authored data.
* **Which region contains the player is the caller's decision.** The "best-priority containing region"
  select is a spatial data query; this crate owns only the budgeted, traffic-gated *count* decision.
* **Bounds behave like the exe.** The 128-cap spawn queue **drops** overflow (counted by
  `SpawnQueue::dropped()`), it does not overwrite; `SimpleSpawnerManager::register` **refuses** the
  769th spawner (returns `None`).
* **`PopulationSimpleSpawner` is a class-manager, not a flat descriptor** — which is why it never
  appeared in the 231-class registry TSVs.
* The density faction loop **skips faction types 7 and 8** (`DENSITY_SKIP_FACTION_TYPES`); those channel
  indices are not ambient-populated.
</content>
</invoke>
