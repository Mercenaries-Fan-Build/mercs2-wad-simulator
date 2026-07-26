# mercs2_destruction

Entity destruction for the Mercenaries 2 reimplementation: the **health → damage messages → per-node
state machine → node-enable** pipeline, plus the debris/emitter side effects the entered states ask
for.

## What it models

```text
Health ↓  →  DamageMsg (0xC6507EE1) / DestroyMsg (0x1ED7AD78)
          →  per-switch-node state machine  (SetStateOnMsg)
          →  SHOW / HIDE over HIER SUBTREES
          →  the node-enable table (OBJ+0x2a0) = draw-gate clause 3
          +  CreateObject (debris) / StartEmitter (fire)
```

States are `Pristine → Damaged → StartDestroyed → Destroyed`, plus **`DetachState`** for break-parts
— the mechanism behind a car shedding a hood or a tank losing its turret as health falls. `SHOW`/
`HIDE` act on whole subtrees, so a governed parent takes its children with it.

Code map: [`docs/reverse_engineer/state_machine_destruction_code_map.md`]; vehicle reading in
[`docs/modernization/vehicle_model_spec.md`] §5.

## Where the pieces live

| Piece | Home |
|---|---|
| `Destructible` component (delivered set, chosen states, node-enable) | `mercs2_core` |
| Machine format + pure replay (`node_states_for_delivered`, `machine_node_enable`) | `mercs2_formats::orchestrator` |
| The runtime system + side-effect intents | **here** |
| Consuming `node_enable` to gate draws | `mercs2_engine` |
| Giving debris rigid bodies | `mercs2_physics` |

Dependencies are exactly `mercs2_core` + `mercs2_formats`, satisfying the workspace carve rule.

## Why a crate, and not part of `mercs2_physics`

Destruction contains essentially no physics math — it is a state machine over model nodes. Physics is
a *consumer* of its output (debris get rigid bodies), not its host. Hosting it there would force
`mercs2_physics` to depend on the orchestrator parser and to write render state.

## The invariant worth knowing

**`Destructible::delivered` is monotonic.** `mercs2_formats` also exposes `node_states_for_health`,
which derives the machine's position from the *current* health fraction — convenient, but it walks
the machine **backwards** when health is restored, so a shed door would reattach. Retail delivers a
message once and only moves forward. This crate keeps the delivered set per entity and recomputes
only when a genuinely new message lands. `the_machine_never_walks_backwards_when_health_is_restored`
is the test that pins it.

## Usage

```rust
let intents = destruction_system(&mut world, &assets, DamageBands::default());
// engine drains: CreateObject -> spawn debris, StartEmitter -> start fire FX
```

`DestructionAssets` is the asset seam (mirroring `mercs2_anim`'s `AnimAssets`): the store lives in
the engine, the system takes a `&dyn`, and no leaf→leaf edge appears.

## Faithfulness caveats — read before trusting output

- **The node-enable seed is not ground truth.** `NodeSeed`'s variants are a choice validated against
  real models; the engine constructor's `memset` sits behind a register alias in the decomp
  (`model_render_gate_spec.md` §6). Everything here inherits that caveat.
- **The health→message band thresholds are ours**, not the engine's. Retail posts real damage
  messages with live HP math we have not recovered. `DamageBands` makes that approximation explicit
  and tunable rather than burying a magic number.
- Intents are **recorded, not performed** — nothing spawns debris or starts a fire until the engine
  drains them. A caller that ignores the return value gets correct geometry and no effects.
