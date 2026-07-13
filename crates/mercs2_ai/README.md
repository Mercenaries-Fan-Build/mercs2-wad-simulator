# mercs2_ai

The AI mechanism of the Mercenaries 2 reimplementation: the hash-addressed 1024-slot action bus, the
`[-100,100]` relation matrix, the per-entity perception update, and the `Ai*` reflection components.

## What it is

Silo 11 / scoreboard row 23. This crate supplies the engine-side **mechanism** for AI — deliberately
*not* a compiled planner, because Mercenaries 2 does not have one. Per the AI code map's §8 reimpl
disposition, the AI "brain" (goal selection, cover FSM, squad tactics) is a **data/Lua goal vocabulary
dispatched over a hash-addressed action bus**, so what the engine owns — and what this crate
implements — is:

* **`AiActionBus`** (`bus`) — the recovered `DirectAction` ring. Capacity `RING_CAP = 0x400` (1024);
  posts over the cap are **dropped, not overwritten** (and counted via `dropped()`), matching the
  recovered `if (count < 0x400)` gate. Entries are `AiAction { guid, hash }` — the `{guid, hash, 0}`
  0xc-byte record the exe writes. `goal_action_hash()` addresses a verb by `pandemic_hash_m2` of its
  lowercased form, so Lua's `"Attack"` and the native `"attack"` agree.
* **`RelationMatrix`** (`relation`) — the directed attitude matrix behind `Ai.SetRelation` /
  `Ai.GetRelation`, clamped to `[RELATION_MIN, RELATION_MAX]` = `[-100, 100]`. Directed, not
  symmetric; an unset pair reads `0` (neutral). `is_hostile(from, to)` = attitude < 0.
* **`update_perception`** (`perception`) — the per-entity perception-record maintenance, the closest
  thing to an AI "think" step that is actually recovered as native code. Each frame it recomputes
  every target's `PerceptionRecord` (observers / aware / hostile observers / hostile aware) from
  observer positions, sight range and the relation matrix. Records are derived, reset each pass, never
  accumulated.
* **The AI reflection components** (`components`) — `AiBehavior`, `AiSkill`, `Perception`, `Stimulus`,
  `Target`, `Squad`, `PerceptionRecord`, with their m2 class hashes and recovered defaults exported as
  consts (`AIBEHAVIOR_HASH = 0xdecd8889`, `AISKILL_HASH = 0xeba09b1a`, `PERCEPTION_HASH = 0x3f6ab8f0`,
  `STIMULUS_HASH = 0x06408d71`, `TARGET_HASH = 0xaff6b246`, `SQUAD_HASH = 0x9788c501`).

`AiWorld` bundles the two world-global pieces (bus + relations) that the `Ai.*` Lua surface drives
directly; per-entity AI state lives on ECS components in a `mercs2_core::World`. `mercs2_engine`
re-exports this crate as `mercs2_engine::ai`, holds an `AiWorld` on its runtime, ticks it each fixed
step, and forwards `Ai.Goal` / `Ai.SetRelation` / the order verbs into it from the script host.

## Where it comes from

Provenance as the crate's own sources state it:

| Piece | Oracle |
| --- | --- |
| Action bus | AI code map §2.2 — the local-enqueue primitive `FUN_00423d10` (`EnterCriticalSection(&DAT_0124aef8)`; `if (DAT_012476a8 < 0x400)`; 0xc-byte `{guid,hash,0}` entry at `&DAT_012476f0 + count*0xc`), posted by `DirectAction` `FUN_0056aa70`. The famous **"Ai 1024" is this ring**, not a per-entity component pool (an explicit §2.2 correction). MP replication to clients (`FUN_006bb960`) is the host-gated next stage and is not implemented here. |
| Perception update | AI code map §2.4 — the per-entity perception-record maintenance (`perception.rs` cites `FUN_00600240`; the 0x64-byte record layout with TotalObservers `[0x13]`, TotalAware `+0x4e`, HostileObservers `[0x14]`, HostileAware `+0x52`, Attackers `[0x15]` is cited in `components.rs` against `FUN_0058d520`). |
| Components + defaults | AI code map §3/§4 — the component census (m2 hash, stride, pool) and its "Headline tunables": AiSkill 10, Perception range 120, Stimulus strength/radius 100 + falloff 40, Target default True, Squad max 50, all `AiBehavior` restriction toggles default false. |
| Relation matrix | AI code map §5 + `docs/reverse_engineer/faction_reputation_code_map.md` — the directed `[-100,100]` attitude matrix the combat→faction loop reads for price scaling / pursuit / HUD colour. |
| Vehicle-AI actuation | `docs/reverse_engineer/road_graph_ai_driving_code_map.md` (referenced by the crate; the actuation itself is not in this crate). |

Code map: `docs/reverse_engineer/ai_code_map.md`. Silo definition:
`docs/modernization/reimplementation_parallelization_plan.md` §3. Owned Lua namespace: `Ai`.

## Usage

```rust
use mercs2_ai::{
    goal_action_hash, AiFaction, AiWorld, Perception, PerceptionRecord, Stimulus, Target,
};
use mercs2_core::glam::Vec3;
use mercs2_core::{Transform, World};

let mut world = World::new();

// An observer (faction 1) and a targetable actor (faction 2) 40 units away.
world.spawn((Perception::default(), Transform::from_translation(Vec3::ZERO), AiFaction(1)));
let victim = world.spawn((
    PerceptionRecord::default(),
    Target::default(),
    Stimulus::default(),
    Transform::from_translation(Vec3::new(40.0, 0.0, 0.0)),
    AiFaction(2),
));

let mut ai = AiWorld::new();
ai.set_relation(1, 2, -80);              // Ai.SetRelation — faction 1 hates faction 2
assert!(ai.goal(0x1000, "Attack"));      // Ai.Goal — posts to the 1024-slot DirectAction ring
assert_eq!(ai.bus.drain()[0].hash, goal_action_hash("attack"));

ai.tick(&mut world);                     // per-fixed-step perception update
assert_eq!(world.get::<&PerceptionRecord>(victim).unwrap().hostile_aware, 1);
```

## Modules

| Module | Owns |
| --- | --- |
| `bus` | `AiActionBus` / `AiAction` / `RING_CAP` / `goal_action_hash` — the 1024-slot `DirectAction` ring and verb hashing. |
| `relation` | `RelationMatrix` / `RELATION_MIN` / `RELATION_MAX` — the directed `[-100,100]` attitude matrix. |
| `perception` | `update_perception` — recomputes every `PerceptionRecord` from observers + relations. |
| `components` | The `Ai*` / `Perception` / `Stimulus` / `Target` / `Squad` / `PerceptionRecord` structs, their class hashes and recovered defaults, plus `AiFaction` and `dist_sq`. |

Crate root re-exports all of the above and adds `AiWorld` (bus + relations + `tick`).

## Notes / gotchas

* **No planner here, by design.** Goal selection, cover and squad tactics are authored content
  (data/Lua); this crate only carries orders and the state the brain reads. `AiWorld::order()` posts
  *any* order verb (`Role`/`Anchor`/`Squad`/`Deploy`/`SetHaste`/`RemoveGoal`/…) through the same
  hash-addressed ring, because there is no compiled per-verb body to reimplement (code map §5/§8).
* **The ring drops, it does not wrap.** Once 1024 actions are queued, further posts return `false`.
  The exe drops silently; `AiActionBus::dropped()` counts them so a caller can observe the budget
  being hit. The consumer is expected to `drain()` each frame.
* **Perception model.** An observer *sees* a target within `Perception::range × unit_mult[0]`; it is
  *aware* when the target is also inside the target's own emitted `Stimulus::radius`. The constants
  are recovered; **the exact falloff curve is data** and is not modelled. `PerceptionRecord::attackers`
  is fed by the combat/action-bus coupling and stays `0` in this crate.
* An entity participates as an **observer** only with `Perception + Transform + AiFaction`, and as a
  **target** only with `PerceptionRecord + Target + Stimulus + Transform + AiFaction`. A `Target(false)`
  entity records no observers at all. Entities never observe themselves.
* `AiFaction` is a reimpl convenience, not a shipped reflection component — identity lives elsewhere in
  the engine, but the perception pass needs a faction key to consult the relation matrix.
* `AiBehavior::set_state(name, on)` is the `Ai.SetState` entry point; it is case-insensitive and
  returns `false` for an unknown flag name rather than silently no-opping.
