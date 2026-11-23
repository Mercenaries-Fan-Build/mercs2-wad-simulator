# K2 — streaming unification plan (route the playable boot through StreamingManager)

**Keystone K2** (`engine_support_inventory.md` §6.1). Chosen approach: **full unification** (not the
re-center stepping stone). This is the plan of record; it stages a large cross-crate refactor so each
step is safe, compiles, and keeps the one working boot path alive.

## The problem, precisely

Two disconnected world-boot paths exist:

| | `run_game_world` (`mercs2_engine::game_world`) | `run_scene_world_loading` (`mercs2_game::world`) |
|---|---|---|
| Reached by | `mercs2_game --stream` | default menu boot + direct `.profile` boot (**the playable path**) |
| World load | `load_streaming_world_data` → `StreamingWorldData` (block index + `StreamingManager` + wake recipes + WAD handle) | one-shot `load_world_data` → `WorldData` (fixed 400 m bubble, cap 200) |
| Per frame | drives `StreamingManager::update(cam)` → `StreamDiff` → a ~200-line executor (LOAD/WAKE/UNLOAD/HIBERNATE meshes) | **nothing** — static after boot |
| Has | free-fly cam, no player/collision/gameplay | player, TPS camera, collision, `GameRuntime` fleet tick |

The player runs the path **without streaming** → walk/drive past the 400 m bubble and the world is empty.
The streaming decision core + executor already exist and are tested — but in the *other* crate, private,
and welded to `run_game_world`'s local closure state (`prop_ents`/`block_ents`/`model_refs`/`wake_failed`/
`anim_store`/`terrain_tiles`/`scene`/`wad`).

## Target architecture

Extract the streaming runtime into a **reusable, public `StreamingWorld` component** in `mercs2_engine`
that owns the manager + all executor bookkeeping and exposes a single per-frame entry point:

```rust
// mercs2_engine::streaming_world  (new)
pub struct StreamingWorld { /* wad, manager, props, terrain_tiles, prop_ents, block_ents,
                               model_refs, wake_failed, anim_store, lowres_draw_by_cell, ... */ }
impl StreamingWorld {
    pub fn from_loaded(data: StreamingWorldData, scene: &mut Scene) -> Self; // base terrain/lights upload
    /// Per fixed step: run the manager diff and perform the GPU LOAD/WAKE/UNLOAD/HIBERNATE work over
    /// `scene`+`world`, returning the woken/unloaded geometry so the caller can rebuild collision.
    pub fn step(&mut self, scene: &mut Scene, world: &mut World, cam: [f32;3]) -> StreamStep;
    pub fn collision_delta(&self) -> &StreamCollisionDelta; // tris added/removed this step
}
pub struct StreamStep { pub woke: Vec<u16>, pub unloaded: Vec<u16>, /* … */ }
```

Both loops then reduce to: build `StreamingWorld`, and each fixed step call `sw.step(scene, world, pos)`.
`run_game_world` layers a free-fly cam on top; `run_scene_world_loading` layers the player + TPS camera +
`GameRuntime` + collision on top.

## Stages (each compiles + keeps `--stream` working; verify build after every stage)

- **S0 — surface the API (safe, no behavior change).** Make `load_streaming_world_data`,
  `StreamingWorldData`, `PropSpawn`, and the wake mesh-resolve helper `pub` in `mercs2_engine`, so
  `mercs2_game` can reach them. Pure visibility change. ✅ *this commit.*
- **S1 — extract `StreamingWorld`.** Move the executor bookkeeping + the ~200-line diff→wake/unload
  body out of `run_game_world`'s closure into `StreamingWorld::{from_loaded, step}`. Refactor
  `run_game_world` to call it. **Behavior-preserving** — the free-fly path must render identically
  (GUI spot-check + build). This is the largest, most delicate stage; do it alone.
- **S2 — collision delta.** Have `step` accumulate the woken/hibernated block+prop triangles into a
  `StreamCollisionDelta` (add/remove), so a consumer can maintain a live collision soup incrementally
  instead of the one-shot boot set.
- **S3 — drive it from the TPS boot.** In `run_scene_world_loading`, replace the `load_world_data`
  bubble with `StreamingWorld` (via `load_streaming_world_data`), calling `sw.step(scene, world,
  player.pos)` each fixed step; feed `collision_delta` into `runtime.set_collision`/the camera/player,
  and pass the terrain heightmap to the fleet physics (`StaticSoupPhysics::set_heightmap`, the row-4
  gap in §6.2). Keep the interior-assembly + player-avatar path.
- **S4 — placements as gameplay entities.** On WAKE, route placements through the `SpawnResolver`
  (Prop/Vehicle/Character per the reflection registry) instead of a bare `Transform+ModelRef`, so
  streamed content is interactive (closes the world-cluster "placements are render-only" gap and
  consumes the K3 `Character` archetype).

## Risks & mitigations

- **S1 is the risk.** The executor is woven into `run_game_world`'s state. Mitigation: extract
  mechanically (move fields into the struct verbatim, keep identical logic), verify the free-fly boot is
  visually unchanged before proceeding.
- **Two loaders during transition.** Until S3, `run_scene_world_loading` keeps `load_world_data`; the
  streaming path is only exercised via `--stream`. No double-load in the playable path until S3 flips it.
- **Collision reps.** S2/S3 must converge the player/camera raw-`Vec` collision and the fleet
  `StaticSoupPhysics` onto one source (the §6.2 "two parallel collision reps" note).

## Definition of done

The default `mercs2_game` boot streams the world around the player (walk/drive out → world loads/unloads,
not empty), collision + terrain track streaming, and streamed placements are interactive — the free-fly
`--stream` path still works (or is retired as redundant).
