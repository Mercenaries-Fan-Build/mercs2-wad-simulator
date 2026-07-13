# mercs2_core

The simulation spine of the Mercenaries 2 reimplementation: the entity `World`, the fixed-timestep
clock, the ordered system schedule and the core components — renderer- and asset-agnostic.

## What it is

`mercs2_core` is the kernel every other sim crate (`mercs2_vehicle`, `mercs2_combat`, `mercs2_anim`,
`mercs2_population`, `mercs2_ai`, `mercs2_script`, `mercs2_engine`, …) depends on. It owns:

* **`World` + components** — `hecs` is the storage substrate; the *shape* follows the original engine
  (an entity is a bag of reflection-addressable components). `Transform` / `ModelRef` / `AnimState` /
  `SkinPalette` are the hand-typed hot-path components the sim actually simulates.
* **`Time`** — the fixed-timestep accumulator. Real (variable) frame deltas are folded in, scaled by
  `timescale`, and drained in `fixed_dt` chunks, clamped by `max_steps` so a long stall cannot spiral.
* **`Schedule`** — an ordered list of named systems. **Registration order is execution order**; there
  is deliberately no auto-scheduler, because the engine has a *defined* update sequence and the exe is
  the oracle for observable behaviour.
* **The keystone mechanisms** the rest of the reimplementation plugs into: the reflection/component
  registry (Keystone A), the name-hash event bus (Keystone B), the master layer stack (Keystone C),
  the GUID map, the world-streaming decision core, the `ObjectFilter` script query, the global
  render/post-FX parameter state, and the `PhysicsQuery` seam.

There is no wgpu, no file I/O and no asset knowledge in this crate. Modules that decide things emit
plain data (e.g. `StreamDiff`) for the host to execute.

Canonical space ≡ game space: left-handed, +Y up, +Z north, +X east (`docs/coordinate_systems.md`).
The asset-load basis transform is the identity.

## Where it comes from

Each module states the oracle it was derived from; in the crate's own words:

| Module | Original mechanism / oracle |
| --- | --- |
| `registry` (Keystone A) | The shared component-class registrar `FUN_0064a770` — per-class descriptor with a `CopyFromStream` vtable, class name, record stride, pool budget (`0x100` default, overridden by `cdbsizes.ini`), `0x9e3779b9` seed. Evidence: `docs/mercs2-ecs/` (232 classes), `docs/modernization/pangea_engine_alignment.md` §1. |
| `event` (Keystone B) | The one shared GUI/Net/AI event bus: `ToggleHud @8255e488` and `NetEventCallback @825d3ce8` marshal through the same quartet (`FUN_8241d458` → `FUN_82878c50` → `FUN_82420690` → `FUN_8256eb28`). Verified: 32-bit name-hash identity, 4 typed arg kinds, argc ≤ 7, 2048-slot frame cap. |
| `frame` (Keystone C) | RunFrame `FUN_00630ef0` (9 stages) + the master tick `FUN_004c14f0 → FUN_004c15e0` (5-slot layer stack ticked 0→4, init `FUN_004c1170`, `DAT_017bbcf4 = 5`, target `DAT_017bbcfc = 4`). `docs/reverse_engineer/scheduler_tick_code_map.md`. |
| `Time` | The decoupled fixed-sim + variable-render split, RunFrame stages 3–4; `dt * timescale` (`_DAT_0198dc48 += dt*timescale`, `FUN_004c14f0`). |
| `streaming` | The world-streaming decision layer: per-entity `HibernationControl` distances (`FUN_00640a40`, class defaults 100/160/60/20), the global LOD-budget governor `FUN_0084ae70`, and the PgSysPopulation region cache-in/out. `docs/modernization/world_streaming_spec.md` §10, `world_streaming_code_map.md`. |
| `guidmap` | The engine's resident guidmap singleton (`0x385EA82C`) that `Pg.GetGuidByName` / `Object.*` resolve against. |
| `physics_query` | The engine's shared query layer: `hkpWorldRayCaster` / `CastRay`, `LthkpWorld::getClosestPoints` (`FUN_008db880`), the `hkpCharacterProxy` swept-capsule controller (`HumanPhysics::Activate` builder `FUN_004255c0`). `docs/reverse_engineer/physics_code_map.md` §3/§4. |
| `object_filter` | The `ObjectFilter.*` Lua namespace; the label boolean-expression grammar is recovered verbatim from the shipped mission scripts. |
| `render_state` | The parameter state the `Atmosphere` / `Bloom` / `Graphics` / `Fade` Lua namespaces set and the render passes read (the shipped Lua drives `Atmosphere.SetValue`/`SetColorValue`/`SetIntValue` far more than the typed setters, so the store is keyed maps). |

Where a detail is a design choice rather than a byte-verified fact, the source says so — see
`event.rs` ("what is verified vs. a design choice": the on-the-wire format is unknown because the
exe's router collapses to unrecovered stubs).

## Usage

```rust
use mercs2_core::{
    event::{Event, EventArg, EventBus},
    frame::{LayerStack, LAYER_GAME},
    streaming::{BlockUnit, Extent2, EntityUnit, StreamingConfig, StreamingManager},
    ModelRef, Schedule, Time, Transform, World,
};
use mercs2_core::glam::Vec3;

// 1. World + fixed tick + ordered schedule.
let mut world = World::new();
world.spawn((Transform::from_translation(Vec3::new(3794.0, 451.0, -3911.0)),
             ModelRef { model: 0xA3C1_FABC }));

let mut time = Time::new(60.0);          // 60 Hz fixed sim
let mut sched = Schedule::new();
sched.add_system("physics", |_w, _t| { /* ... */ });
sched.add_system("animation", |_w, _t| { /* ... */ });  // registration order == tick order
let steps = sched.run_fixed(&mut world, &mut time, 0.0166); // fold a real frame delta, drain steps

// 2. The master layer stack: boot at 0, climb to LAYER_GAME (4), one slot per advance.
let mut layers = LayerStack::booting();
while !layers.settled() { let _t = layers.advance(); }
assert_eq!(layers.active(), LAYER_GAME);

// 3. The event bus: name-hash keyed, ≤7 typed args, deferred queue drained once per tick.
let mut bus = EventBus::new();
let toggle_hud = 0x1B2C_8599u32;              // caller hashes the name (pandemic_hash_m2)
bus.on(toggle_hud, |e: &Event| { let _ = &e.args; });
bus.emit_hashed(toggle_hud).arg(EventArg::Int(1)).queue();
let _dispatched = bus.dispatch_all();

// 4. The streaming decision core: catalog in, StreamDiff out. No I/O, no GPU.
let mut mgr = StreamingManager::new(StreamingConfig::default());
mgr.add_block(BlockUnit {
    block: 3490,
    extent: Extent2::from_center_half(3794.0, -3911.0, 64.0),
    stream_out: 700.0,
    always_resident: false,
});
mgr.add_entity(EntityUnit { key: 1, pos: [3800.0, 451.0, -3900.0], dist: [100, 160, 60, 20] });
let diff = mgr.update([3794.0, 451.0, -3911.0]);   // load_blocks / unload_blocks / wake / hibernate / tier_changes
assert!(diff.load_blocks.contains(&3490));
```

`steps` is the number of fixed sim steps executed this frame; a host that runs its own fixed-step body
instead of a `Schedule` calls `Time::advance_frame(real_dt)` and loops that many times.

## Modules

| Module | Owns |
| --- | --- |
| (crate root) | `World`/`Entity` + `glam`/`hecs` re-exports, `Time` (fixed-step clock), `Schedule` (ordered systems), and the core components `Transform` / `ModelRef` / `AnimState` / `SkinPalette`. |
| `registry` | Keystone A — `ComponentRegistry` / `ComponentDescriptor` / `FieldLayout` / `FieldKind`: component classes keyed by type-hash, with `cdbsizes.ini` pool budgets (`load_budgets`) and the reflected field schema. |
| `event` | Keystone B — `EventBus` / `Event` / `EventArg` / `SubId`: name-hash pub/sub, ≤7 typed args (`MAX_EVENT_ARGS`), immediate `emit` plus a bounded deferred queue (`DEFAULT_QUEUE_CAP` = 2048) drained by `dispatch_all`. |
| `frame` | Keystone C — `LayerStack` / `LayerTransition` / `LAYER_COUNT` / `LAYER_GAME`: the 5-slot application-layer stack, index-only (the per-layer `Update` bodies live in the host). |
| `streaming` | The world-streaming decision core: `StreamingManager` → `StreamDiff` (block residency, entity wake/hibernate, LOD tiers), `GlobalLodGovernor`, `RegionCache`, `lod_tier`, `DEFAULT_DISTANCES`. |
| `guidmap` | `GuidMap`: name-hash → `Entity` and guid ↔ `Entity`, plus the reserved `HERO_GUID` / `LOCAL_PLAYER_GUID` handles and `FIRST_DYNAMIC_GUID` mint base. |
| `object_filter` | `ObjectFilter` / `ObjectFilterRegistry` / `eval_label_expr`: the `ObjectFilter.*` label boolean-expression + include/exclude sets, minted through a handle registry. |
| `render_state` | `RenderState` and its `AtmosphereState` / `BloomState` / `GraphicsState` / `FadeState` sub-states — the parameters the presentation Lua sets and the render passes read. |
| `physics_query` | The `PhysicsQuery` trait + `RayHit` / `ClosestPoint`: the collision-query contract the sim silos compile against instead of depending on `mercs2_physics`. |

## Notes / gotchas

* **This crate never hashes names.** `registry`, `event` and `guidmap` all key on a precomputed `u32`
  name-hash; the engine hash (`pandemic_hash_m2`) lives at the byte-decode boundary in
  `mercs2_formats` so there is a single implementation and no drift. Callers pass the hash in.
* **Registration order is the tick order.** `Schedule` intentionally has no dependency solver — an
  auto-scheduler would reorder work away from the oracle.
* **Two different LOD tiers compose.** The per-entity `HibernationControl` tier (`lod_tier`, class
  defaults 100/160/60/20) is distinct from the global memory-pressure LOD-budget tier
  (`GlobalLodGovernor`, engine `FUN_0084ae70`); the global tier acts as a *coarseness floor* on the
  per-entity tier inside `StreamingManager::update`.
* **c3 "tier" is a size index, not a LOD level.** `StreamingConfig::tier_stream_out` is indexed by
  the c3 chain tier, which is loose-quadtree *depth* ≈ object size (big landmarks bucket shallow,
  small props deep) — not a detail level of a shared surface.
* **Streaming throttles only the additive direction.** `block_budget` / `entity_budget` cap loads and
  wakes per update; unloads and hibernations are never throttled.
* **Event-bus overflow is dropped, never blocked.** A full deferred queue increments
  `dropped_count()`; `EmitBuilder::arg` past the 7-arg cap drops the arg and sets `overflowed()`
  (use `Event::try_push` when you want a hard `Err`).
* `DEFERRED.md` lists the non-blocking gaps deliberately left open (region-catalog wiring, the full
  population spawner, live-captured LOD-budget thresholds, the 8-vs-64 kept-ring discrepancy). Each
  is tagged with whether it is a faithfulness blocker.
</content>
</invoke>
