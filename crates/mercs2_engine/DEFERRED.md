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
