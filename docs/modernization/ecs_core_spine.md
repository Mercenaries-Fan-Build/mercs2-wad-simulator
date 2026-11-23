# `mercs2_core` — ECS Simulation Spine + Multi-Entity Scene

**Status: built + plumbing-verified (2026-07-01).** Source of record: memories
`mercs2-core-ecs-spine`, `mercs2-engine-skeleton`.

[`tools/wad_simulator/crates/mercs2_core`](../../crates/mercs2_core) is the renderer/asset-agnostic **simulation
spine** for the 64-bit native reimplementation (see
[`docs/modernization/00_charter.md`](00_charter.md)). The design decision was to **mirror the original
engine's architecture**, with an ECS crate as the substrate.

> Note: this documents the *modernization Rust reimplementation's* ECS. It is distinct
> from [`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/), which documents the *original game's* native reflection-based
> component system.

## Crate contents (`mercs2_core`)

- **`hecs`** re-exported as the ECS storage substrate (`pub use hecs; World =
  hecs::World`). Chosen over `bevy_ecs` so **we own the explicit system order** (mirror
  the exe's tick), with no auto-scheduler reorder and no version churn.
- **`Time`** — fixed-timestep clock: `Time::new(hz)`, `fixed_dt/dt/elapsed/tick`,
  accumulator, `max_steps = 8` spiral-of-death clamp.
- **`Schedule`** — ordered named systems: `add_system(name, FnMut(&mut World, &Time))`,
  `run_fixed()` drains the accumulator. **Registration order = execution order.**
- **Components** (`components.rs`), all in canonical game space (LH +Y up, see
  `coordinate_systems.md`):
  - `Transform { translation, rotation, scale }` (glam) with `.matrix()`
  - `ModelRef { model: u32 }`
  - `AnimState { clip, time, speed, playing }` (+ `prev_clip/prev_time/blend` for
    crossfades, added with the locomotion work)
  - `SkinPalette { mats: Vec<[[f32;4];4]> }` — the **sim → render hand-off**
- 2 unit tests pass (fixed-timestep drain; animation system writes the palette).

## Engine wiring

- **`--ecs`** flag → `run_render_ecs()`: builds a `World`, spawns entities with
  `{Transform, ModelRef, AnimState, SkinPalette}`, registers an `animation` system that
  advances `AnimState.time` and samples the clip via `pose::havok_palette` into
  `SkinPalette` each fixed tick.
- **Multi-entity Scene + asset store** (`mercs2_engine/src/scene.rs`): `Scene` owns
  shared GPU state once + a per-model GPU store (`ModelGpu`, keyed by hash) + per-entity
  GPU resources (`EntityGpu`: own MVP uniform group0 + bone-palette group2).
  `AssetStore` / `ModelAnim` is the CPU rig+clip store shared to the animation system via
  `Rc`. `Scene::load_model(hash, …)` uploads once (idempotent per hash);
  `Scene::render(&World)` snapshots drawable `(Transform, ModelRef, SkinPalette)` entities
  and draws each model's groups with `view_proj · (entity_model · fit)`.
- Verified: `--ecs` spawns two Mattias instances from **one** shared asset at x=±0.6 with
  a half-duration animation phase offset (2 entities / 1 asset / independent phase).
  `--model2 <hash>` adds a distinct second model. The legacy single-model Renderer
  `--animate` path is untouched.

**Scope of verification:** the ECS *mechanics* (entity → system → component → renderer,
multi-entity, independent phase) are verified. Render correctness rode on the skinning
fixes — the hand deformation present at the time this crate landed was later fixed via
the per-group BLENDINDICES correction (`skinning_animation_spec.md` §1.4).

## Next bricks

`mercs2_script` (Lua 5.4 behavior bound to components by type-hash), then physics / world
streaming. Relates to `world_terrain_loader.md` (the first `World` consumer) and
`modernization/00_charter.md`.
