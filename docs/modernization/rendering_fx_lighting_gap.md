# Rendering / FX / Lighting — faithful-implementation gap map

**Purpose:** the honest, exhaustive catalog of what the Pangea (Mercs2) presentation stack does vs what
`mercs2_engine` implements today, so we implement *everything* per the reversed description instead of
discovering the gap piecemeal. Cross-referenced to `docs/ucfx_tag_registry.md` (chunk formats),
[`docs/mercs2-pdb-analysis/rendering-shaders.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/rendering-shaders.md) (pipeline), [`docs/mercs2-ecs/05_presentation_audio_fx.md`](../../crates/mercs2_workshop/data/mercs2-ecs/05_presentation_audio_fx.md)
(components), and [`docs/ghidra_knowledge_inventory.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ghidra_knowledge_inventory.md) (handlers). Compiled 2026-07-04.

Status legend: ✅ done · 🟡 partial · ❌ not started.

## Where we are today (updated 2026-07-04 after the parallel fan-out)
A 4-lane worktree fan-out landed (commits ede029f/b47fdff/701ed96/314e36d), all merged + game green:
- **Dynamic lighting + specular** 🟡: `LightObject` (0x97e8ee92) parser + forward up-to-32 point lights
  (radius attenuation) + preserved sun/ambient + Blinn-Phong specular from MTRL slot 1 (`_sm`). SPEC:
  light_type enum + spot-cone floats not decoded (all treated as point); per-material gloss not threaded.
- **Particles/FX** 🟡: `fxdict` + EMTR/EFCT/COLR/FRCE/POFF/TRFM/PTYP/TEXT parser (18 tests) + a wgpu
  billboard CPU-sim (`Scene::fx_start`/`fx_stop`, mirrors Lua StartEmitter). SPEC: EffectTemplate→
  EmitterDesc auto-map (EMIT/FRCE/COLR float roles unpinned in decomp) — game feeds explicit descriptors.
- **Sky/atmosphere + HDR/bloom** 🟡: `atmosphere` model + scattering sky + HDR Rgba16Float target →
  bright-pass/blur/ACES-Reinhard tone-map + bloom; Option fallback to direct present. SPEC: auto-exposure
  approximated (not the real adaptive-luminance feedback loop).
- **Prop anim clips** 🟡: clip-selection + populate `LoadedModel.clips`. NOTE: retail vz.wad props ship no
  Havok clips (RE-verified), so nothing animates on retail data — the layer is correct for when clips exist.
- render() reconciled across lanes: `draw_geometry` binds group-3 lights; HDR-vs-direct path chosen by
  `sky_enabled`+post availability; particles draw in a separate swapchain pass over the final image.
- ⚠ NOT visually tested yet — needs a `cargo run` to confirm (esp. the HDR path; it's gated + falls back).
Still ❌: shadows, multi-pass z-prepass/reflection, decals, LOD imposters, ECS destruction, real exposure.

## The subsystems

### A. Geometry / mesh — 🟡 mostly done
- ✅ GEOM/PRMG/STRM/`decl`/`data`/`info`/IBUF/PRMT/POFF, HIER + SEGM (skeleton + bone bind), INDX.
  (`mesh.rs::build_indexed_state`, `model_cubeize`, `skeleton.rs`.)
- ❌ `MESH`/`TINY` LOD imposters (0x471900 / 0x471a01) — we always draw top LOD; no distance LOD swap.
- ❌ `PHY2` Havok collision (0x4a845f) — we built our own capsule controller instead (acceptable).
- ⚠ SEGM byte-3 semantics: we treat it as a `state_mask` bitmask; registry calls it `group`. Reconcile
  against the SEGM consumer decomp before trusting it for anything beyond the current heuristic.

### B. Materials / shading — 🟡 partial
- ✅ MTRL diffuse (slot 0) + normal (slot 2). Alpha-tested cutout. Hi-res texture streaming
  (`extract_texture_hires`) + NAME labelling.
- ❌ **Specular / gloss (slot 1, the `_sm` maps)** — dropped. Every interior material has one.
- ❌ Detail maps / slots 3-9 (MTRL is a fixed 10-slot array, reg line 354).
- ❌ Per-material **shader permutations** (`_pl`/`_sl`/`_pl_sl` = point-light / shadow-light variants;
  `AmbientWind`/`Ruin`/`Morph`/`Tiny` mesh-shader variants). We have one übershader.
- ❌ No real BRDF (fixed `ambient + 0.9·NdL`); no HDR-space lighting.

### C. Lighting — ❌ not started (1 fake directional)
- ❌ **`LightObject`** ECS (0x97e8ee92, `FUN_006622e0`; 0x34 bytes = id + color + 9 floats:
  color rgb, intensity, radius/attenuation, cone, …). Placed dynamic **point/spot lights** — the
  interior is lit by these (the villa's `global_portablelight` etc.). Read from `layers_static` /
  `vz_state` placements and the interior blocks. *"Low effort, high visual impact"* (per ue5 mapping).
- ❌ **`LightAnimation`** (0xbd5349f7 — 10 floats + int) flicker/pulse; **`ColorAnimation`**,
  **`ScaleAnimation`**.
- ❌ Sun/directional day-night (`PgSun`), ambient/GI approximation.
- ❌ Multi-light accumulation (forward+ or deferred) — the `_pl_sl` permutations imply per-light passes.
- ❌ `BlobShadow` ECS (0x40349618) cheap projected blob shadows under characters/props.

### D. Shadows — ❌ not started
- ❌ Depth `ShadowBuffer` + per-mesh-type shadow VPs (`PgMeshShadowVP`, `PgSkinShadowVP`,
  `PgRoadShadowVP`, `PgBillboardTreeShadowVP`); `ShadowBounds`/`GetShadowBaseDistance`.
- ❌ `BlobShadow` (cheap fallback).

### E. Particles / FX — 🟡 sim + render + real-data wire done; job-parallel/ribbons pending
Mercs2 is explosion/smoke/fire-heavy; the Lua drives emitters constantly (`ObjectState.StartEmitter/
StopEmitter`, `global_particle_*` templates). The CPU billboard sim + additive/alpha render exist
(`mercs2_engine::particles`), and authored effect data now drives them.
- 🟡 **`fxdict`/DICT** (`FUN_00491320`): parser lands the 20B effect-parameter records
  (`mercs2_formats::fxdict::parse_fxdict`). (reg §7.)
- ✅ **Effect template → runtime wire**: `EffectTemplate::from_chunks` parses the cluster (`EFCT`/`EMTR`/
  `EMIT`/`COLR`/`FRCE`/`PTYP`/`POFF`/`TRFM`/`TEXT`), `game_world::load_effect_template` reads it from the
  `effects` block by name-hash, and `EmitterDesc::from_effect_template` converts the **reliably-parsed**
  chunks (COLR gradient, FRCE gravity/drag/wind, PTYP blend) into the runtime emitter — replacing the
  `demo_*` name-heuristic (`world.rs` resolves each `global_particle_*` at load, WAD open). Informed by
  the WildStar `WSParticleEmitter/Transformer` recovery (`saboteur_mercs2_crossval_render_physics.md`).
  RESIDUAL: `EMIT` timing float order is **unpinned** (positional reflection) → lifetime/spawn_rate stay
  at the base preset until a live capture pins it; `FRCE` kind classification is the FRCE hypothesis;
  Vortex force + `ATRB` attributes not modelled by the billboard sim.
- ❌ **ECS spawners**: `ParticleEmitter` (0xe595ab2f, `FUN_00661190`), `RedEffectComponent`
  (0x60a13e3e), `EffectTemplate` (0xabaa1f3c), `EffectAiOccluder`.
- ❌ **Runtime**: hardpoint-attached emitters (`hp_fx_*` nodes), billboard/soft particles, additive
  blending, `Ribbon` (0x059b95b9) trails/tracers, `Render3DParticles-FX` pass.

### F. Sky / atmosphere / post — ❌ not started (fog placeholder)
- ❌ `PgSky`/`PgSun`/`PgCloud` sky+atmosphere; ❌ HDR tone-map + `PgBloomCombiner` bloom;
  ❌ reflection pass; ❌ water (scoped separately, see `water-and-swimming-scope`).

### G. Decals — ❌ not started
- ❌ Projected decals (bullet holes, scorch, `global_decal_super_concrete` seen in the hall MTRL).
  Job-parallel decal render per the pipeline doc.

### H. Animation playback — 🟡 partial
- ✅ Character skeletal (Mattias wavelet decode, 168/168 tests; hand/head solved).
- ❌ **Prop/object clips** — `TRCK`/`VALU`/`KEYS`/`MANM` parsed+validated but NOT loaded into
  `LoadedModel.clips`; interior props/doors/machines don't animate.
- ❌ `BoneCtrlLocalRotation` / continuous controllers (rotor spin etc.), `LightAnimation` curves.

### I. Render architecture — 🟡 single-pass
- Game: `PgScene::Render` per viewport, passes **z → color → shadow → reflection**, job-parallel;
  Renderable lifecycle (`AddRenderable`/`RenderableUpdate`/hibernation), `Model::Render`/`RenderZPass`/
  `RenderShadow`. We do one forward color pass. Multi-pass + z-prepass is the prerequisite for shadows,
  decals, and correct transparency ordering (particles/water).

### J. ECS behaviour graph — ❌ not started
- `STAT`/`SWIT`/`CHDR`/`CEXE`/`NODE` (`ECS_PlacementParse` 0x4cf340) = destruction/variant **state
  machine** (e.g. the villa's `CollapseState`). A runtime gameplay feature (buildings collapse), not a
  render filter — do NOT gate draw groups on it.

## Dependency graph — these are mostly PARALLEL, not a ladder
Almost every subsystem is independent; the only real couplings are noted. Do them in any order / concurrently.

**Independent workstreams (no hard deps):**
- **Specular (MTRL slot 1)** — shader/material only.
- **Dynamic lights (`LightObject`)** — forward multi-light accumulates in the *existing* single pass; no
  multi-pass required. (Pairs naturally with specular since both touch the shader.)
- **Particles/FX** — separate billboard/additive path rendered in/after the forward pass; `fxdict`
  parsing is standalone data work.
- **Prop anim clips (`TRCK/VALU/KEYS` → `LoadedModel.clips`)** — animation-data work; reuses the
  character skeletal system.
- **Sky/atmosphere + HDR/bloom** — a post pass over the color target.
- **Decals** — projected quads (depth-test + polygon-offset) work in forward.
- **LOD imposters (`MESH/TINY`)**, **ECS destruction state machine** — independent.

**Soft couplings (helpful-before, not blocking):**
- **Shadows** want a depth/z-prepass + a light to cast from → nicer *after* lights + a z-prepass exist,
  but can be prototyped against the current directional light.
- **Multi-pass (z→color→shadow→reflection)** is not a prerequisite for lights/particles/specular/decals;
  it's the clean home for shadows + correct transparency ordering once several of the above land.

**Practical caveat for concurrency:** these are independent in *design* but several touch the same render
files (`scene.rs`, `shader.wgsl`, pipeline/bind-group setup). Concurrent edits want either one hand
interleaving them or isolated branches/worktrees to avoid churn in those shared files. The *data* parsers
(`fxdict`, `LightObject` ECS, anim clips, decal defs) are cleanly separable and conflict-free.

Highest visual impact for least effort: **dynamic lights + specular** (villa/interiors read flat without
real lights). But pick freely — nothing blocks. This file is the checklist; update status as we go.
