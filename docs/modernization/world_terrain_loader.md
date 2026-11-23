# Native World Terrain Loader + `--world` Render

**Status: low-res world loads + renders; TPS locomotion wired; screenshot-verified
(2026-07-01 / 2026-07-02).** Source of record: memory `world-terrain-loader`.

The first real WORLD in the native engine: the low-res Maracaibo terrain loads and
renders as a continuous, recognizable landmass. Per user directive, "terrain surface
first," and coords stay in native game space (LH +Y up) with **no UE flips** (see
`coordinate_systems.md`).

## Rust loader — `mercs2_formats/src/terrain.rs`

`load_terrain(low_res_block, layers_static_block) -> TerrainMesh`. Ports the *logic*
(no coord math) from [`tools/terrain_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) + `ucfx_mesh_codec.py`:

- **Low-res terrain block = WAD block index 3121.** 401-entry TOC (`u32[0] == 401`); each
  tile is a UCFX container `INFO + MTRL + GEOM(BNDS + PRMT + STRM + IBUF)`, no PRMG.
  `STRM` = flat verts, stride 16 = pos f16×3 + uv f16×2 + 2 pad; `IBUF` = triangle
  **strips**. Handles the entry-0 dual-purpose case and the entry-224 dummy (`u3 < 10`
  skip).
- **Layers-static block = index 29**, sub-block `LowResTerrainObject` COMP = 400 records
  `(key, mesh_hash, scene)` row-major; record index `i` ⇒ cell `(row = i//20, col =
  i%20)`. `mesh_hash → TOC hash1 → UCFX iter index` selects the tile mesh per cell.
- **Placement formula:** tile center = `(-3800 + col·400, 0, -3800 + row·400)`; verts
  offset to world.
- Also extracts the shared `vz_lrterrain` 2048² DXT1 atlas.

## Engine render — `--world` / `--world-probe`

Merges all 400 tiles into **one** world-space mesh loaded as a single static entity in
the `Scene` (see `ecs_core_spine.md`). Terrain verts map into the engine `Vertex`
(normal `[0,1,0]`, white color, identity bone-0 weight); `terrain_to_vertices` does
planar-XZ UV synthesis + retail V-flip.

**Verified numbers** (probe re-run + screenshot captured): TOC 401 / 400 decoded /
**400 placed**, 203,514 verts, 271,150 tris, X&Z ∈ [-4000, 4000], Y ∈ [-167.75, 435.75],
atlas 2048² BC1. Render shows a continuous Maracaibo (city grid, roads, refinery,
mountains continuous across tiles).

**Caveats:** low-res baked-atlas terrain only (streaming hi-detail c3 cells are separate —
behind `--cells`); lighting is FAKE (all normals up = unlit); possible minor tile seams
(spec: tiles not C0-continuous, up to ~400 m edge residual).

Run: `cargo run -p mercs2_engine -- --world` (or `--world-probe` headless).

## Sky + fog

User-approved approach: *similar look, free tech* — NOT a PgSky port. `sky.wgsl` =
fullscreen-triangle gradient dome at far depth (inverse view-proj ray, horizon→zenith
smoothstep, pow-512 sun glow, `sun_dir [0.3, 0.35, 0.6]`); exponential distance fog in
`shader.wgsl` (`1 - exp(-density · max(d - start, 0))`, `d = clip.w`). The group0 uniform
grew 64 → 96 B (mvp + fog vec4×2) in **both** `Scene` and the legacy `Renderer`, enabled
only via `Scene::set_fog(color, density, start)`. The real game's PgSkyFP/PgSunFP/
PgMoonFP/PgCloudGen stack is the oracle for LOOK tuning (see
[`docs/mercs2-pdb-analysis/rendering-shaders.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/rendering-shaders.md)) — per user, tune against game
screenshots. Current tunables: color `[0.55, 0.62, 0.70]`, density `0.00016`, start `60`.

## Cameras

Mercs2 gameplay = third-person over-the-shoulder; free cam is for engine/dev.
`Scene::set_view(view, near, far)` is caller-driven (the old auto-rotating BirdEye was
removed). `run_scene_world` has two modes, **Tab toggles**:
- **FREE** — WASD + E/Q up/down + arrows look + Shift boost; starts elevated.
- **THIRD-PERSON** — WASD moves camera-relative, player yaws to face motion, arrows orbit;
  camera = `focus(+2.2Y) − dir·6 + right·1.2` shoulder offset. `--world --tps` starts here.

## TPS locomotion (user-confirmed good, 2026-07-02)

- Multi-clip player (HashMap by name-hash): idle `0x24F8C8E6` + walk `0x53682784` + run
  `0x867B166D` (Shift). See `locomotion_clips.md` for clip identification.
- **Root-motion strip** = `pose::havok_palette_in_place` (root bones keep BIND
  translation; only the world scene uses it — `--animate`/`--ecs` untouched).
- **Crossfade blending** (0.25 s Havok `blendPoses`: lerp T/S + hemisphere-nlerp R on
  local QsTransforms), walk↔run phase sync, exponential yaw damp (12 rad/s), speed easing,
  FOOT_SYNC playback scale.
- **Human-scale speeds:** WALK 2.2 / RUN 6.5 m/s, ACCEL 12 / DECEL 16 (the earlier 14/60
  were vehicle speeds — user-confirmed mismatch).
- **Ground follow:** exact triangle spatial hash (250² 32 m cells, barycentric, f64;
  `height_at_near(y_hint)` for overhangs); the old coarse grid was 13.4 m wrong at spawn,
  kept only as a hole fallback. Foot offset from model minY.
- Mouse = dual-source auto-detect (see memory `shadow-pc-absolute-mouse-input`).
- Skinning deformations all FIXED via the per-group BLENDINDICES correction.

## Hi-res c3 cells (`--cells`)

1,849 mesh blocks found; 16 nearest loaded, cell-local; grid formula ported from
`mercs2_c3_grid.py` v3 (base 30001, 100×100, 77.5 m, X/Z ∈ [-3900, 3850]). Not yet
visually confirmed. See `world-lod-and-destruction-scope` and `world_placements.md`.

## Loading screen (done, 2026-07-02)

Window-first + threaded load (20 s → 9 s via a single cached animgroup scan
`load_clips_for_rig`). Real plate from `shell.wad` = `lti_precache1` `0x7329D083`
(2048×1024 BC1, cropped to authored 1280×720; found via `mrxguiloadscreen.lua` — it is a
Scaleform swf composite, the plate is the precache art). Staged progress bar fills the
plate's own frame (`LoadProgress` atomic `step(name)`, 6 stages).

## Known gaps

- No collision.
- Player spawns at the PMC HQ compound location (game `2647, −951`, terrain-snapped) but
  the PMC base geometry is not rendered yet — it is "at the PMC's location on bare
  ground," not "in the compound." PMC base has a dedicated placement set
  (`populate_pmc_base.py` / `import_pmc_base.py`). See memory `pmc-teleport-coords-and-interior`.
- c3 cells not visually confirmed; placements (62,624) + prop meshes are the next render
  brick (see `world_placements.md`); LOD/destruction matrix; water + swim.
