# mercs2_water

Water (scoreboard row 7) for the Mercenaries 2 reimplementation: the static watermap query, the
third-person swim-state FSM, and the buoyancy / water-drag force math, plus the `AiWaterZone` tag.

## What it is

The engine-owned water *mechanism* — the compiled-logic half of the water system, with the render pass
kept out of scope (it lives on the other side of a seam, in `mercs2_engine`).

Concretely the crate provides:

- **`Watermap`** — the static water asset (type hash `0x4D7D30C4`, `watr` UCFX chunk): a Layer-0 f32
  surface-height field + a Layer-1 `u8` wet mask over a 257×257 grid of 32 m cells. Parses raw `watr`
  bytes, maps world XZ → cell (nearest-cell, clamped), and answers the waterline query as a
  `WaterSample { is_water, surface_height }`. Also emits a CPU surface mesh (one flat quad per wet cell
  at that cell's height) for a translucent water pass.
- **`Swimmer` / `SwimState` / `SwimConfig`** — the TPS swim FSM (`OnLand → Wading → Swimming →
  Submerged`), classified from feet-depth below the sampled surface, with a hysteresis band so a
  character bobbing at a boundary does not chatter. `update_swim_state` is the per-fixed-step ECS
  system.
- **`Buoyancy` / `WaterDragTunables` / `submersion_fraction`** — flotation as pure force math: a
  spring-damper up-force against the waterline, per-axis body-frame water drag scaled by submersion,
  the out-of-water gravity factor, and the corner-sampling submersion fraction.
- **`AiWaterZone`** — the AI water-type reflection tag (hash `0xdf6533de`, stride `0x04`), carried as a
  raw enum column.
- **`WaterWorld`** — the world-global bundle (loaded watermap + `SwimConfig`) the sim holds; `tick()`
  drives every `Swimmer` in the ECS `World` and is a no-op until a watermap is streamed in.

## Where it comes from

Provenance as the source states it:

- `docs/reverse_engineer/water_code_map.md` (the sky/decal/water PC code maps) — scoreboard row 7.
- `docs/watermap_format.md` — the recovered `watr` layout: header (`layer_count`, `grid_width` 257,
  `grid_height` 257, `cell_size_m` 32.0, `height_min_m` −50.0, `height_max_m` ≈325.26, 3 unknown
  trailing fields), Layer 0 heights, Layer 1 wet mask. The type hash is asserted in-crate to equal
  `pandemic_hash_m2("watermap")`.
- `FUN_00480440` — the exe's waterline query. Its *exact* return packing (height vs boolean) is a
  SecuROM-island confirm-live item (code map §5), so the crate returns **both** facts and lets the
  caller pick.
- `docs/reverse_engineer/vehicle_code_map.md` §3/§5 — the buoyancy `hkpUnaryAction` `FUN_00458ac0`
  (8 AABB-corner sample points, waterline query per frame, buoyant impulses every other frame, a "sunk"
  latch) and the boat driver `FUN_00447260` which applies buoyancy + `WaterDrag{Fwd,Side,Up}`. The
  `Buoyancy` component: hash `0xb9659f7b`, builder `FUN_006395e0`, stride `0x14` (5 floats).
- `docs/reverse_engineer/ai_code_map.md` §3/§4 — `AiWaterZone` hash `0xdf6533de`, builders
  `FUN_0065c520` / `FUN_00641560`, stride `0x04`.

## Usage

```rust
use mercs2_core::glam::Vec3;
use mercs2_core::{Transform, World};
use mercs2_water::{
    Swimmer, SwimState, WaterWorld, Watermap, CELL_SIZE_M, GRID_DIM, OPEN_WATER_SURFACE_M,
};

let mut world = World::new();
let swimmer = world.spawn((
    Swimmer::new(),
    Transform::from_translation(Vec3::new(0.0, -50.0, 0.0)),
));

let mut water = WaterWorld::new();
// Real asset: Watermap::from_watr_bytes(&watr_payload)?  — here, a uniform stand-in.
water.set_watermap(Watermap::uniform(GRID_DIM, CELL_SIZE_M, OPEN_WATER_SURFACE_M, true));

// The waterline query (the engine-owned half of FUN_00480440).
assert!(water.is_water(0.0, 0.0));
assert_eq!(water.water_surface_height(0.0, 0.0), Some(OPEN_WATER_SURFACE_M));

// Per-fixed-step: advance every Swimmer's FSM against the watermap.
water.tick(&mut world);
assert_eq!(world.get::<&Swimmer>(swimmer).unwrap().state, SwimState::Submerged);

// A renderable water surface: one quad per wet cell, in world space (game Y-up).
let (positions, indices) = water.watermap.as_ref().unwrap().surface_mesh();
```

Buoyancy and water drag are **pure math**, applied by the physics silo rather than by a system here:

```rust
use mercs2_core::glam::Vec3;
use mercs2_water::{submersion_fraction, Buoyancy, WaterDragTunables};

let hull_corner_ys = [-1.0f32, -1.0, -1.0, -1.0, 1.0, 1.0, 1.0, 1.0]; // 8 AABB corners
let submersion = submersion_fraction(&hull_corner_ys, 0.0); // 0.5

let b = Buoyancy::default();
let up = b.vertical_force(/* body_y */ -1.0, /* vel_y */ 0.0, /* surface_y */ 0.0);

let t = WaterDragTunables::default(); // neutral placeholders — see gotchas
let drag = t.water_drag(Vec3::new(1.0, 0.0, 8.0), submersion); // body frame: x=side, y=up, z=fwd
let g = t.out_of_water_gravity_factor(/* vel_y */ -3.0);
```

## Modules

- `watermap` — the static `watr` asset: `watr` parse, grid→world mapping, the wet/height query
  (`WaterSample`), and `surface_mesh()`.
- `swim` — the TPS swim-state FSM (`SwimState`, `SwimConfig`, `Swimmer`, `update_swim_state`).
- `buoyancy` — the `Buoyancy` reflection component, the boat `WaterDragTunables`, and
  `submersion_fraction`, as pure force math.
- `zone` — the `AiWaterZone` reflection component (raw enum column).

The crate root additionally owns `WaterWorld`, the world-global water state (watermap + swim config)
with the per-fixed-step `tick`.

## Notes / gotchas

- **Sea level is not Y=0.** In the retail Maracaibo watermap the open-water plateau sits near
  **−36.0 m** (`OPEN_WATER_SURFACE_M`); dry cells store exactly the header's `height_min_m` = **−50.0**
  (`HEIGHT_MIN_M`) as a sentinel. Always gate on `is_water` before trusting `surface_height`.
- **Nearest-cell, never bilinear.** Layer 1 is a categorical mask and Layer 0 mixes the −50 dry
  sentinel with real wet heights, so interpolating across a shoreline would smear both. The engine
  samples the discrete field; so does this crate.
- **Only Layers 0 and 1 are modelled.** Layers 2–3 (coastal-variant / sparse-override, a hypothesis)
  and a 33,290-byte footer are left unread — the format doc marks them unconfirmed.
- **The grid origin is a hypothesis.** The centred mapping (index 0 → `-(dim-1)/2 * cell`; −4096 m for
  257 @ 32 m) is what the format doc proposes and what `from_watr_bytes` uses, but it is stored as the
  public `origin_x` / `origin_z` fields so an exe-confirmed origin can override it.
- **Swim thresholds are gameplay-derived, not exe-recovered.** `SwimConfig`'s `wade_depth` (1.0 m),
  `swim_depth` (1.7 m) and `hysteresis_m` (0.1 m) come from a ~1.8 m human, not from the code map. The
  state vocabulary and the depth-drives-state shape are faithful; the numbers are tunable config.
- **`Buoyancy` / `WaterDragTunables` numeric defaults are neutral placeholders, not exe constants.**
  The tuning field *names* are stripped on the PC build and extracting the authored values is the
  vehicle-map §5 open item. The field *set* is faithful; the defaults are deliberately inert (zero
  drag, normal gravity) so an unconfigured boat is inert rather than silently wrong.
- **`AiWaterZoneEnum` member names are not recovered** — the code map lists the table but does not
  itemise it, so the zone value is carried raw.
- **Deliberately out of scope here:** the water *render* pass (wake → occlusion → reflection → surface,
  the ping-pong `pHeightS`/`pNormalS`/`pFoamMas` sim RTs, the reflection mirror-matrix, `PgWater*`
  shaders, `OWater::LOD` tessellation banding — code map §1–§3). The **dynamic wave displacement** lives
  there, so this crate models only the *static* waterline. Motion blur is **absent on PC** per the code
  map, and nothing here fabricates it.
