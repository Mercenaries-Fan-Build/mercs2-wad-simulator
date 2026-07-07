# mercs2_engine — deferred render-graph work

Wave-0 silo **E2 (render-graph carve)** established `render_graph::SCENE_ORDER` — the recovered
`FUN_00466d40` per-viewport pass order (render_core_code_map.md §5/§11) — and moved the engine's
existing passes (shadow-depth, color/HDR, transparent-FX, UI) into ordered nodes with **zero
behaviour change**. The following canonical passes are registered as **no-op seams** in the correct
order and are left for the Band-A silos to implement next wave. Each is a real faithful gap, but by
design out of E2's carve scope.

| Seam node (`render_graph::PassId`) | Anchor | Owner |
|---|---|---|
| `WakeMap` / `Occlusion` / `Reflection` / `WaterSurface` | `FUN_00486390` / `FUN_00482fa0` / `FUN_00486fa0`→`FUN_004677d0` / `FUN_00487540`+`FUN_00487dd0` (water_code_map.md §2) | Band-A water/reflection silo |
| `ZOpaque` | `FUN_00468e40` vtbl+0x1c (Render/RenderZPass) | Band-A z-prepass silo |
| `FadingTrees` | `FUN_00468bb0` (RenderFadingTrees, Xbox 0x16a2c) | Band-A vegetation silo |
| `Mirror` | `PTR_PTR_01175a10` iterate, obj vtbl+0x40/+0x14 | Band-A mirror/sub-scene silo |
| `Blob` | `FUN_00853710` (BlobShadow fallback) | Band-A shadow silo |
| `Particles` (canonical PgFX pass) | particle_fx_code_map.md | Band-A particles silo |

Plug-in seam: implement `render_graph::RenderNode` against the node's `PassId` slot and record into
`render_graph::PassCtx`.

## Improvements observed but NOT implemented in E2

- **4× shadow cascades.** The exe emits four cascades into a 1024×4096 atlas (`while(i<4)` around
  `FUN_00468ca0`, shadow_code_map.md §4); the engine realizes a single directional cascade
  (`PassId::ShadowCascade`). Multi-cascade is a shadow-silo task. `[faithful-blocker: no]`
- **Sky-as-a-pass.** The engine draws the sky as the first fullscreen triangle inside the color pass
  rather than as the canonical standalone sky/atmosphere pass. Splitting it out belongs with the
  sky/HDR silo. `[faithful-blocker: no]`
- **`PassCtx` surface.** Intentionally minimal (device/queue/encoder/color/depth/size). Band-A will
  extend it with the shared camera/lights bind groups + collected renderable list as the water and
  reflection passes need them. `[faithful-blocker: no]`

## Wave-1 seam A/B (schema loader + region cache) — confirm-live follow-ups

The E1 `schm` deserializer is now wired into the world loader (`worldutil::load_schema_components`)
and the S5 RegionCache is populated from authored `PopulationDensity` anchors
(`worldutil::register_population_regions`, driven each tick in `game_world`). Grounded facts and the
items left for a live/x32dbg read:

- **Region extent is a POINT, not a real rect.** The placed region COMPs do not author an extent:
  `PopulationDensity` = density params + 2 name refs + flags (no min/max), `LineRegion` = a single
  ref (points live in a separate `PgLineRegion`), and `SphereRegion`/`CircleRegion` (the only region
  types that author a radius float) have ZERO placed instances joined to a Transform in retail vz.wad
  (registry-block prototypes + runtime `World.CreateRegion` only). The engine's real per-region rect
  is built at load in `FUN_004d60e0`/`PgSysPopulation::Update` (+0x10..+0x1c). UNBLOCK = x32dbg-read
  that rect (and the priority `+0x38`) for a placed region and replace the point-anchor +
  `POPULATION_REGION_CACHE_IN/OUT` tunables with the authored rect. `[faithful-blocker: no]`
- **Region cache radii are tunables.** `POPULATION_REGION_CACHE_IN=250` / `_OUT=400` (metres) are
  streaming tunables analogous to `StreamingConfig::tier_stream_out`, pending the live rect above.
  `[faithful-blocker: no]`
- **PopulationDensity field semantics.** 7 fields decoded (`0x263c1369`/`0xc63c9abc` f32-encoded
  small ints = density levels; `0x77e838e4`/`0xafb45fd9` = name/template refs; `0x87519019` f32-int;
  `0xa603f273` flags; `0x431c37e3` ref). Region `priority` is set to 0 pending a live confirm of
  which field drives the density-selection priority gate. `[faithful-blocker: no]`
- **The population-lump executor.** `update_regions` decisions (CacheIn/CacheOut) are computed each
  tick but not yet acted on (no spawn/despawn of an ambient-population lump). That executor is the
  population silo's job; the decision layer is now live and correct. `[faithful-blocker: no]`
