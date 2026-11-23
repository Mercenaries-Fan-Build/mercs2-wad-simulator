# Terrain splat + texture-LOD — ground-truth spec (RCA 2026-07-02)

> **Superseded for hi-res terrain.** How a cell's draws bind materials and how its ground is shaded is
> in [`terrain_cell_format.md`](../terrain_cell_format.md) §4.4 and §5.5, read from `FUN_004AAAB0` and the
> `PgTerrainMesh*` shader records. A draw's material is `PRMT` word 1; the `PRMG INFO +8` word this
> project once read as a material index is a pass group's draw count.
>
> **RCA CORRECTION — 2026-07-02, headless-verified via `--terrain-probe` (`mercs2_engine`).**
> Two load-bearing hypotheses below are **DISPROVEN by the data**. Do not build the low-res splat
> on them.
> 1. **The per-tile `MTRL` does NOT reference the `terraintextures` materials.** All 400 tiles'
>    MTRL parse cleanly, and every one references **exactly one** texture: `0xA007B5B9` =
>    `pandemic_hash_m2("vz_lrterrain")`, the **baked composite atlas** — the same thing
>    `terrain.rs` already binds. **0 of 400** references hit any of the 30 `terraintextures` hashes.
>    A raw-byte scan finds **0/30** of those hashes anywhere in `low_res_terrain` (3121) OR
>    `layers_static` (29). The low-res terrain is fully self-contained (geometry + baked atlas); it
>    carries **no splat authoring** over the terraintextures set.
> 2. **The `@12` f16 is NOT a blend weight — it is `normal.z`.** The `@8-13` lanes are a per-vertex
>    **unit normal** (`normal.xyz`, 3× f16): **203514/203514 (100%)** of vertices have
>    `len(f16@8,@10,@12)` within 0.03 of 1.0. `@12` ranges `[-0.9995, 0.9839]` (signed), only 55% in
>    `[0,1]`. Lanes `@6` and `@14` are `1.0` for all 400/400 tiles (the two half4 `w` components).
>    Corrected vertex layout: `pos.xyz f16 @0-5, w=1.0 @6, normal.xyz f16 @8-13, 1.0 @14`. (The real
>    render UV is derived from world XZ in `terrain_to_vertices`, which is why the atlas already maps.)
>
> **Consequence:** the low-res tiles cannot drive a terraintextures splat — the inputs aren't there.
> The `terraintextures` P000/P001/P002 mip rungs + P003 detail materials (mountain=`0xB6EA5F50`,
> rock=`0x121F00F7`, grass=`0x23F5B0C4`, all confirmed members of the 30-set) are consumed by a
> **hi-res terrain material path that is NOT block 3121 or 29** — location still unlocated (candidates:
> per-cell c3/c2/c1 world blocks, or a `TerrainKey`-driven runtime system; `FUN_004a88a0`). Find that
> consumer before attempting the splat. The sections below are retained as the original hypotheses.


The engine's terrain hi/low-res LOD (`TerrainGuidMappingHighResToLowRes`, runtime-built
`FUN_004a88a0`) is **NOT a geometry mesh swap** — there is no hi-res terrain mesh. It is a
**terrain-material texture pipeline**: a set of source materials blended per terrain location
(splat), each streamed at higher mip resolution near the camera. This spec codifies what `--block-grep`
/ `--block-probe` / `--lod-probe` / `MERCS2_TERRAIN_DBG` proved, so the build is grounded, not guessed.

## What exists in the WAD (verified via `--block-grep terrain`, `--block-probe <n>`)

Exactly **7 terrain blocks**:
- `low_res_terrain_P000_Q3` (block 3121) — the ONLY terrain GEOMETRY: 400 tile meshes (type
  `0x1602815C`, `INFO+MTRL+GEOM`) + a baked composite atlas `vz_lrterrain` (2048² Bc1). This is what
  `terrain.rs` loads today (tiles → shared atlas via UV). Self-contained low-res.
- `terraintextures_P000_Q3` (3434), `_P001_Q2` (5270), `_P002_Q1` (8287) — the SAME ~30 material
  textures at increasing mip residency (the LOD rungs). Verified: identical texture hashes across
  rungs; sizes grow `P000 ≈ 2.8 KB (resident low-mip tail) → P002 ≈ 32 KB (256²) / 163 KB (1024²)`.
  This IS the hi/low-res LOD = **mip streaming of the terrain materials** (coarse rung far, fine near).
- `terraintextures-tt_mountain01 / -tt_rock / -tt_pmcgrass02_P003_Q0` (10732/11025/11232) — full-res
  **detail materials** (512² Bc1, full mip0 resident), the finest per-surface-type detail rung.

## The per-tile material mapping (the splat authoring — the remaining RCA to nail)

- Each low-res tile is a `0x1602815C` container with a **`MTRL`** chunk. HYPOTHESIS (to verify): the
  tile MTRL lists which of the ~30 `terraintextures` materials that tile blends (1–N material hashes).
  `terrain.rs` currently IGNORES the tile MTRL and uses the composite atlas instead — the build must
  parse it. (Reuse `model_cubeize`/`mesh.rs` MTRL parsing patterns; MTRL hashes are the same texture
  hashes seen in the `terraintextures` blocks.)
- **Terrain vertex layout (verified, `MERCS2_TERRAIN_DBG`):** stride 16 = 8× f16 —
  `pos.xyz @0-5, w=1.0 @6, uv.xy @8-11, SCALAR @12 (varies), 1.0 @14`. The varying `@12` f16 is the
  prime candidate for a **per-vertex blend weight** (base↔detail, or between the tile's two materials).
  A single scalar can't select among 3 arbitrary materials, so 3-way mountain/rock/grass selection is
  NOT per-vertex — it is per-tile (via MTRL) and/or a control texture. VERIFY `@12`'s meaning before
  relying on it (dump vs known grass/rock tiles).
- `TerrainKey` COMP (`s_TerrainKeyEnum_00bc72c4`) = per-cell surface class (material/footstep/physics)
  — a coarse per-tile surface-type hint that may drive which detail material a tile uses.

## What to build (Full splat + LOD — user-approved scope)

1. **Parse the per-tile MTRL** → each tile's terrain-material hash(es) + the `@12` blend weight into
   the terrain vertex. Headless-verifiable (dump counts: tiles → materials, weight range). This is the
   foundational data step; do it first and prove it.
2. **Load the `terraintextures` materials** (choose a mip rung) + the mountain/rock/grass detail
   materials, as GPU textures (reuse `make_bc_view` streamed-mip decode; the coarse rung is a resident
   mip tail — same resident-mip handling as props).
3. **Terrain splat shader** — per tile/vertex, sample the tile's base material + blend the detail
   material(s) by the `@12` weight (and/or a control scheme if `@12` proves insufficient). Detail
   materials tile at a higher spatial frequency (separate detail-UV = world-space or scaled base UV).
4. **Texture-LOD streaming** — swap the terrain-material rung coarse→fine by camera distance
   (`terraintextures_P000 → _P002`), i.e. upgrade resident mips near the camera. This is the literal
   `TerrainGuidMappingHighResToLowRes` behavior.

## Constraints / guardrails
- Native game space (LH +Y up), no coordinate flips. Reuse `make_bc_view` (streamed-mip) and the
  existing terrain load path in `mercs2_formats/src/terrain.rs` + `mercs2_engine` `--world`.
- Do NOT overclaim visual correctness — the shader's look is USER-verified from screenshots. Report
  the headless-verifiable facts (tiles parsed, materials resolved, mip sizes, rung swaps) precisely.
- Keep the existing baked-atlas path working as a fallback (`--world` must still render).
- New headless RCA tooling already in place: `--block-grep <substr>`, `--block-probe <index>`,
  `MERCS2_TERRAIN_DBG=1`, `--lod-probe`.
