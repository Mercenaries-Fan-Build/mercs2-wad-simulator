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

- **4× shadow cascades.** ~~The exe emits four cascades into a 1024×4096 atlas (`while(i<4)` around
  `FUN_00468ca0`, shadow_code_map.md §4); the engine realizes a single directional cascade.~~ **DONE
  (Wave-1 silo 2 — lighting/shadows).** `PassId::ShadowCascade` now renders the DYNAMIC scene into a
  **1024×4096 depth atlas** (four stacked 1024² tiles, faithful to `FUN_00755d90` §1), one viewport +
  light-VP per cascade via a dynamic-offset uniform; casters are distance-LOD gated into only the
  cascade boxes that contain them (`FUN_00858150` analog). The color pass (`shader.wgsl` `shadow_factor`)
  selects the tightest containing cascade per fragment and PCF-samples its tile. `[faithful-blocker: no]`
- **Sky-as-a-pass.** The engine draws the sky as the first fullscreen triangle inside the color pass
  rather than as the canonical standalone sky/atmosphere pass. Splitting it out belongs with the
  sky/HDR silo. `[faithful-blocker: no]`
- **`PassCtx` surface.** ~~Intentionally minimal (device/queue/encoder/color/depth/size).~~ **DONE
  (Wave-1 seam D).** Extended with the camera (`view_proj` / `view` / `cam_pos`), the per-frame
  `lights_bind` (group-3 dynamic lights + folded shadow map), `surface_format` (for transient RTs),
  and `items` — the collected renderable list (`PassId::Collect` output, [`RenderItem`]). `Scene`
  populates a `PassCtx` per registered node in `dispatch_nodes`; register via `Scene::add_render_node`.
  Each field cites the pass(es) that consume it (render_core §5 / water §2). `[faithful-blocker: no]`
- **`Collect` list is populated-only, not yet consumed by `Color`.** The `items` list is exposed on
  `PassCtx` and IS the same `Vec` the engine's existing `Color`/`ShadowCascade` passes draw from
  (zero-copy alias), but those passes still index it via their own private path rather than reading it
  back through `PassCtx`. Re-driving `Color` from `ctx.items` is a mechanical follow-up left untouched
  here to keep the carve byte-identical (fidelity bar). A newly-registered Band-A node already reads
  the fully-populated `items`. `[faithful-blocker: no]`

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

## Wave-1 silo 2 (lighting + shadows) — confirm-live follow-ups

The 4-cascade directional atlas, the `_pl`/`_sl`/`_pl_sl` per-pixel light-class permutation (ShaderLevel
gate), the BlobShadow fallback, and the `Rt*Animation` light-tween update are now wired into `scene.rs`
+ `shader.wgsl` + `blob.wgsl`. Grounded facts and the items left for a live/x32dbg read:

- **Per-pixel light-class shader math is a faithful reconstruction, not the exe's exact curve.** The
  `_pl`/`_sl`/`_pl_sl` fragment math (point windowed falloff, spot cone smoothstep, Blinn-Phong spec) is
  **VMX128 in the exe and does not decode from the PPC dump** (rendering-shaders.md §Corrections). We
  realize the four compiled permutations as a runtime branch on `lights.count.y` (the `DAT_00dfc345`
  ShaderLevel analog, default 3 = `_pl_sl`) — same visible result, one pipeline. **CONFIRM-LIVE:** break
  the `_pl`/`_sl` fragment shaders (or `RenderLights` `0x0026b28`) in x32dbg to recover the exact
  attenuation curve, cone falloff, and specular model, then match the WGSL. `[faithful-blocker: no]`
- **Spot lights are not yet harvested from retail data.** `set_spot_lights` + the `SpotLightGpu` record
  (pos/range/color/intensity/**cone axis + inner/outer cos**) + the `_sl` shader path are all wired, but
  `placement::light_inventory` only emits omni `render::GpuLight` point records today — the `LightObject`
  stride-0x34 `int` type field (point vs spot) and the cone-angle floats are not decoded into spot
  records. On retail data the spot set is empty (point lights only). **CONFIRM-LIVE / cross-silo:** decode
  the `LightObject` type + cone floats in the harvest (`FUN_006622e0`, presentation-ECS §LightObject) and
  route spots to `set_spot_lights`. `[faithful-blocker: no]`
- **`Rt{Light,Color,Scale,Alpha}Animation` descriptors are not decoded from the COMP stream.** The tween
  runtime (`set_light_animations` + `animated_lights`, the `FUN_00675e50` master-update analog) is wired
  and unit-tested (pulse/flicker), but the retail `LightAnimation` sub-records (`FUN_00646b60` descriptor,
  the `0x9e3779b9`-seeded keys) are not parsed by the world harvest, so on retail data the anim set is
  empty unless a caller supplies it. The pulse/flicker MATH is an engine approximation. **CONFIRM-LIVE:**
  read the `RtLightAnimation::Update` (`0x0017758`) tween keys live and decode them into `LightAnim`.
  `[faithful-blocker: no]`
- **Blob geometry/darkness/projection are approximations.** `record_blob` grounds a dark radial disc in
  the world XZ plane at the caster's origin for casters outside every cascade (sun-on/outdoor only, so
  interiors/default paths are unchanged). Radius (1.3 m) + `darkness` (0.45 = the `ShadowK` analog) are
  fixed knobs, and the disc sits at the entity origin rather than a real ground-raycast contact point.
  The exe's blob render is vtable-reached (`FUN_00853710`, `BlobShadowFrontVB`, `ShadowK` — no static
  token, shadow_code_map.md §5). **CONFIRM-LIVE:** break the `PgBlobShadowVP/FP` set to recover the real
  `ShadowK`, per-caster bounds, and ground projection. The sun-off (interior overhead) blob path is left
  disabled to avoid net-new dark discs under every indoor prop. `[faithful-blocker: no]`
- **`SetShadowBaseDistance` threshold not bound.** The distance-LOD caster gate uses the cascade-box
  containment test as its near/far split ([`CASCADE_SPLIT_FACTORS`] = the nested extents), not the exe's
  authored `SetShadowBaseDistance` float (near `DAT_01176288`, unbound — shadow_code_map.md §4). **CONFIRM-LIVE:**
  read that float and drive the cascade extents from it. `[faithful-blocker: no]`
