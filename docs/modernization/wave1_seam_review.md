# Wave-1 Tier-2 seam review

**Scope.** The inter-silo boundaries of the Wave-1 work merged on `main`: the seam sub-wave (W1-A
schema→loader, W1-C `PhysicsQuery`, W1-D `PassCtx`) and the six-silo fleet (physics 7, animation 8,
vehicles 25, weapons/combat 26, audio 14, lighting/shadows 2). Reviewed against the actual code, every
claim grep-verified (not taken from the agents' self-reports).

**Verdict: no correctness defects — the fleet composes (build + full test suite green, merged
conflict-free). But every one of the six subsystems is DANGLING at the engine-loop boundary.** That is
*by design* — Wave 1 was the "build the subsystems" wave — but it must be stated plainly so "6 crates,
green" is not mistaken for "6 subsystems running." **Nothing in the running engine instantiates or
drives any of them yet.** Grep proof:

- `crates/mercs2_engine/src/**` references **none** of `mercs2_physics`/`mercs2_anim`/`mercs2_vehicle`/
  `mercs2_combat`/`mercs2_audio` and calls none of `animation_system`/`drive_step_system`/
  `WeaponSystem`/`CharacterController`/`AudioEngine`. `game_world.rs` drives only `mgr.update()`
  (streaming) — no gameplay system on the fixed tick.
- `crates/mercs2_game/src/**` references none of the fleet crates either.
- `crates/mercs2_script/**` references none of the leaf crates — so the real `Sound`/`Vehicle`/
  `Weapon`/`Airstrike` bodies the silos wrote (in each crate's `lua_surface.rs`/`engine.rs`) are
  **not installed**: `bindings/*.rs` still `install` nothing, and the zero-stub counter is unmoved.

So the whole Tier-2 story is **one connection layer** — the Wave-2 analog of Wave-1's A/C/D seam
sub-wave. Three tasks make the fleet *run*; a few smaller seams round it out.

---

## Seam table

| # | Seam | Status | Finding (grep-verified) | Wave-2 action |
|---|------|--------|-------------------------|---------------|
| **1** | **Physics world → sim systems** | ⚠ DANGLING | `StaticSoupPhysics`/`CharacterController` exist; nothing builds a physics world from the streamed `collision_tris` (which `world.rs` already collects) or hands `&dyn PhysicsQuery` to the vehicle/combat/anim/character systems. | Build a physics world per streamed region from `collision_tris` + heightmap; inject `&dyn PhysicsQuery` into every sim system's tick. |
| **2** | **Layer-4 system registration** | ⚠ DANGLING | `game_world.rs` runs no gameplay system on the tick. The five new systems (`CharacterController::step`, `animation_system`, `drive_step_system`, `WeaponSystem::update`, `AudioEngine::tick`) are never called. This is the still-open "layer-4 registered list" seam from the scheduler work + carve. | Register the systems into the fixed-tick schedule in the recovered `FUN_004c9740` order (population/vehicle/weapons mid-list, etc.); drive them from the streaming loop's master update. |
| **3** | **`EngineHost` → leaf-crate Lua bodies** | ⚠ DANGLING (the zero-stub unlock) | `EngineHost` trait is in `mercs2_script` (impl `mercs2_game::script_host`). Audio/vehicle/combat wrote real bodies in their crates + named each method to match its Lua binding, but nothing forwards: `bindings::{sound,vo,vehicle,camera,weapon,airstrike}` install nothing. The **1075-stub baseline hasn't moved** despite the bodies existing. | Extend the `EngineHost` trait + its `mercs2_game` impl with forwarding to the leaf-crate bodies; fill each `bindings/*.rs::install()` via `b.real(...)` (Lua `uGuid`↔`Entity` conversion). This is what actually drives the zero-stub counter down. |
| **4** | **Render group-3 BGL contract** | 🔧 API-GAP | Lighting changed group 3: **added binding 4 (`SpotLightGpu`)** and made the shadow texture **1024×4096** (4 tiles), and `set_shadow`'s `half_extent` now means cascade 0 (coverage ×15). Any future Band-A `RenderNode` binding group 3 must match, and must sample the atlas with tile UVs. | Document the group-3 BGL as a contract in `render_graph`; when a Band-A silo adds a pass, hold it to that layout. (No action needed until a Band-A render silo runs.) |
| **5** | **`combat::Health`/`Inventory` → canonical types** | 🔧 FUTURE-DUP | `Health` + `Inventory` are defined **only** in `mercs2_combat` today (no duplication yet), as documented stand-ins. When destruction (silo 13) lands `RuntimeHealth` and player (silo 17) lands the canonical loadout `Inventory`, two crates will define the same concept → the `World` would carry two distinct component types. | When 13/17 land, hoist the canonical `Health`/`RuntimeHealth` + `Inventory` to a shared home (destruction crate or `mercs2_core`) and retarget combat's applier (combat already flags this in its `DEFERRED.md`). |
| **6** | **Spot/LightAnimation harvest** | ⚠ DANGLING (cross-silo) | Lighting's `_sl` spot path + `Rt*Animation` tween are wired and unit-tested, but `placement::light_inventory` only emits omni point lights — the `LightObject` type field + cone floats + `LightAnimation` sub-records aren't decoded. On retail data the spot/anim sets are **empty**. | Decode `LightObject` type + cone (`FUN_006622e0`) and `LightAnimation` (`FUN_00646b60`) in the world harvest → route to `set_spot_lights`/`set_light_animations`. A world-content task (placement decode), not a render task. |
| **7** | **Anim stance-binder** | 🔧 SEAM | Animation is data-table-driven (no `Human.*` clip-call bodies — a faithful finding). Gameplay Lua sets stance/action, which must map onto `HumanAnimationSet::state` (a `StateKey`). No binder exists. | Add the stance/action → `StateKey` binder in the Lua-host/gameplay layer (rides seam 3). |

---

## The dominant seam, unpacked: the Wave-2 connection sub-wave

Seams 1–3 are one coherent unlock — make the fleet *run* — and are exactly parallelizable the way
W1-A/C/D were (each a bounded, mostly-disjoint edit to the engine/game boot layer, sequenced by me):

- **W2-phys — physics-world injection.** Per streamed region, build a `StaticSoupPhysics` (+ char
  controllers for actors) from `collision_tris`/heightmap; expose `&dyn PhysicsQuery` to the tick.
  Touches `mercs2_engine::game_world`/`worldutil` + a small physics-world holder.
- **W2-sched — layer-4 registered system list.** Stand up the ordered gameplay-system list
  (`FUN_004c9740` order) inside the streaming loop's master update, calling the five fleet systems at
  fixed timestep on the shared `Time`. Closes the last open scheduler item. Touches `game_world`.
- **W2-lua — `EngineHost` binding wiring.** Extend `EngineHost` + `script_host` + fill the
  `bindings/*.rs::install()`s to forward to audio/vehicle/combat/anim bodies. Touches `mercs2_script`
  + `mercs2_game::script_host`. **This is the one that moves the zero-stub Lua counter** — measure it
  before/after via E3's `binding_coverage.json` gate.

W2-phys and W2-sched both touch `game_world` (coordinate or sequence them); W2-lua is disjoint
(`mercs2_script`/`script_host`). A collision-safe split: run **W2-lua** concurrently with a combined
**W2-phys+sched** owner (they're the same file region — one owner avoids the conflict).

---

## Bottom line

The fleet is six correct, tested, isolated subsystems — and the honest state is that **not one of them
runs yet**. That's the right shape for the wave that built them; the next move is the connection layer
(seams 1–3), after which the engine actually *simulates* (characters move on real physics, vehicles
drive, weapons fire, audio plays, the tick order matches the oracle) and the zero-stub Lua counter
finally drops. Seams 4–7 are smaller and land with their partner silos. No defects, no rework — pure
connection.
