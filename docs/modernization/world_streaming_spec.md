# World Streaming Engine — Ground Truth Spec

**Status:** Authoritative build reference. Every fact below is VERIFIED against retail `vz.wad` +
the decompiled exe/Lua unless marked `UNVERIFIED`/`TODO`. This document exists so the builder
**implements our proven research** rather than re-deriving it. When something says PROVEN, trust it
and wire it; do not re-investigate.

**Principle (from `00_charter.md`):** behavior gated, implementation free. Mirror the *original
engine's architecture*; the exe is the oracle. STOP building one-off loaders (`--interior`,
`--props`, `--cells` are bespoke staples to be SUBSUMED). The real engine has ONE system: index every
block, know each block's location + contents + settings, and stream the right ones in/out by player
proximity and configured rules. Terrain, c3 cells, placements, interiors are all *outputs* of that
one system.

**Coordinate space:** LEFT-HANDED, +Y up, +Z north, +X east. Game space == our engine space; the
asset-load transform is the IDENTITY. NO coordinate flips anywhere (the Y↔Z swaps in old docs were
UE5/glTF export only). World bounds ≈ ±5000 (`mercs2_formats::world` constants); real placement
extent X[-3888,3800] Y[-103,393] Z[-3869,3800] (interiors are OFF-MAP, see §7).

---

## 1. The WAD / block model (what exists on disk)

`vz.wad` = an **FFCS archive** (`mercs2_formats::ffcs`). Access via `mercs2_engine::wad`:
- `wad::open(path) -> Wad` (auto-discovered from the EA Games registry key).
- `wad::model_list(&Wad) -> Vec<(u32 asset_hash, u16 block_index)>` — all primary model ASETs
  (ASET `type_id==19`). **1,771** in retail.
- `wad::block_paths(&Wad)` — the PTHS table: `block_index -> "blocks\\VZ\\<name>_P###_Q#.block"`.
- `wad::decompress_block_index(&mut Wad, u16) -> Result<Vec<u8>>` — SGES-decompress a block.
- `wad::extract_container(&mut Wad, u32 hash) -> Result<Vec<u8>>` — the UCFX model container for a
  model asset hash (finds its block via the ASET table, decompresses, slices the `model`
  (`0x5B724250`) chunk).
- `wad::extract_texture(&mut Wad, u32 hash) -> Result<TextureData>`.
- `wad::animgroup_blocks(&Wad)` — blocks holding animation ASETs (`type_id == 16`).

**ASET type ids / chunk type_hashes seen:** model `0x5B724250`, texture `0xF011157A` (ASET type 27),
animation `0x18166555`, facefx `0x1CF649BB`/`0x665EF13E`, scaleform `0xFE0E8320`, placement-composite
format_hash `0xE6B81A54`.

**Block classes (by name / content):**
| Class | Name pattern | Contents | Resolve path |
|---|---|---|---|
| **Model** | `*_bld_*`, props, `pmcoutpost_interior_*`, etc. | UCFX: HIER/MTRL/SEGM/GEOM/MESH/INDX (+ Havok) | `extract_container`+`build_indexed_from_container` |
| **c3 cell** | `c3####[-c2###-c1###-c0###]_P#_Q#` | baked cell geometry (buildings/roads/props) OR streamed texture/BODY | §3, §4 |
| **layers_static** | `00029_…layers_static` (**block 29**) | 173 UCFX sub-blocks of COMP records (Transform/Name/ModelName/…) — the always-loaded base placement layer | §2 |
| **vz_state overlay** | `*_vz_state_*` (746 files) | conditional COMP overlays (mission/faction/destruction state, interiors) | §2, §5, §7 |
| **low_res_terrain** | `03121_…low_res_terrain` (**block 3121**) | 401-entry TOC of 400 tile meshes + `vz_lrterrain` atlas | §3 |
| **Texture** | `*_dm/_nm/_sm`, atlases | BC1/BC3, STREAMED (resident low-res mip tail only) | §6 |

---

## 2. Content resolution — the TWO PROVEN paths (the keystone)

**This is the crux and it is SOLVED.** World content resolves two ways, no unknowns:

### 2A. Placed entities (props, furniture, lights, gameplay objects) → `ModelName` recipe
Placement blocks (layers_static block 29; vz_state overlays) are UCFX→CHDR→**COMP** composites.
Each sub-block holds COMP records keyed by a `u32 entity_key`. **43 COMP types** exist; the ones that
matter:
- **`Transform` COMP** — 42-byte records: `{+0 u32 entity_key, +4 f32 x, +8 f32 y, +12 f32 z, +16 f32
  pad, +20 f32 qx, +24 f32 qy, +28 f32 qz, +32 f32 qw, +36 6-byte tail}`. Unit quaternion (LH game
  space). Yaw = `2*atan2(qy,qw)`; ~16% have pitch/roll — USE THE FULL QUAT (`Quat::from_xyzw`).
- **`Name` COMP** — `{u32 key}["EntityName 0xHEXID\0"]` → gameplay name.
- **`ModelName` COMP** — **`{u32 entity_key, u32 model_hash}`** (8-byte stride). `model_hash` ==
  `pandemic_hash_m2(asset_name)` == the model ASET `asset_hash` → feed DIRECTLY to
  `wad::extract_container`. **455/465 resolve.** COMP child offsets are RELATIVE to data_area_start
  in layers_static, ABSOLUTE in vz_state (the walker in `placement.rs` handles both).

**The recipe (per block):** build `key→(pos,quat)` from Transform and `key→name` from Name; for each
`ModelName{key,hash}` place mesh `hash` at that key's Transform. When an entity has a Name but no
ModelName, its asset name is the entity name **minus the leading `_` and the trailing ` 0xKEY` hex
suffix**, hashed with `pandemic_hash_m2` (e.g. `_pmcoutpost_interior_recruitjet 0x000c740d` →
`pmcoutpost_interior_recruitjet` → `0x86D7CF92` → real mesh). Already implemented:
`mercs2_formats::placement::{load_placements, load_model_placements(→ModelPlacement{key,model_hash,
pos,quat,name}), comp_inventory}`.

### 2B. Static world / buildings → **c3 cell geometry (baked)**
Exterior BUILDINGS are NOT placed per-entity — they are **baked into c3 cell geometry** (tier B).
PROVEN: 0 `_bld_` entries in ModelName; a test building resolves to its covering c3 cell. So "render
the city" = load the c3 cells covering the area, NOT resolve building props. c3 cells are standard
model-format (`0x5B724250`) containers → `build_indexed_from_container`. **~1,849 c3 blocks carry
model-format geometry** (far more than an early ~60 estimate). Grid formula = §4.

### 2C. Terrain → `low_res_terrain` + `LowResTerrainObject`
Already implemented: `mercs2_formats::terrain::load_terrain(low_res_block, layers_static_block) ->
TerrainMesh`. Block 3121 TOC (401 entries; entry-0 dual-purpose, entry-224 dummy) → 400 tile meshes
(flat STRM f16 verts + IBUF strips). Block 29 sub-block `LowResTerrainObject` COMP = 400 records
`{key, mesh_hash, scene_object_id}` row-major → cell→mesh. World placement = tile center
`(-3800 + col*400, 0, -3800 + row*400)`; verts f16, offset to world. This is the BOTTOM LOD rung.

---

## 3. Meshes & materials (how a container becomes drawable)

`mesh::build_indexed_from_container(&container) -> (Vec<Vertex>, Vec<u32> indices, Vec<DrawGroup>,
SkinData)`. Critical VERIFIED rules the builder must not regress:
- **BLENDINDICES are PER-DRAW-GROUP palette-relative, NOT global HIER.** Each PRMG group's INFO(56)
  leaf carries a bone-range table (`+20 u32 range_count`, then `{u16 hier_base, u16 count}×rc` from
  +24); vertex joints index the CONCATENATION of those ranges. Fixed in `model_cubeize.rs`; decomp
  `FUN_00479d90` confirms. (Getting this wrong = claw-hands / exploded faces.)
- **Rigged static props need an identity SkinPalette sized to the mesh's bone count**
  (`vec![IDENTITY; skin.bones.len().max(1)]`). A 1-bone palette makes verts weighted to bone≥1 read a
  zero matrix and collapse to the origin.
- Load placed meshes with **identity fit** (`skin.center=[0;3]; skin.scale=1`) so world placement is
  the authored Transform (`load_model_by_hash` in main.rs already does this + extracts textures).
- Vertex decl Xbox→PC is a format translation; DEC3N tangent is 10-10-10-2 (not 11-11-10); PHY2 is a
  Havok packfile; CHDR is `{u16,u16,u32}` in placement but `{u32,u32}` in mesh. (All fixed already.)

---

## 4. Spatial index (proximity) — the piece we NEVER built

The streaming manager needs "which blocks are near (x,z)". Each block's spatial extent comes from:
- **c3 cells** — `mercs2_c3_grid.py` v3 (ported in `load_c3_cells`): IDs map to a **100×100 grid**,
  cell **77.5 m**, X/Z ∈ **[-3900, 3850]**; verified anchor `c30123 → (-2156.25, -3783.75)`. The
  `c3####-c2###-c1###-c0###` chain encodes the LOD tiers of the same cell (see §5). Cell meshes are
  **cell-local** (origin-centred) → offset to the cell's grid-centre world position.
- **lrterrain tiles** — 20×20, 400 m, center `(-3800+col*400, 0, -3800+row*400)`.
- **Placement sub-blocks / vz_state** — spatial extent = the AABB of the block's Transform positions
  (compute at index time). vz_state overlays attach to a region/mission/faction state.
- **Interiors** — OFF-MAP or high-Y; keyed by portal/zone not proximity (§7).

The index must therefore, per block: name (PTHS), class/type (ASET), LOD tier + variant (name suffix
parse), and spatial extent (grid formula for c3/lrterrain, Transform-AABB for placement blocks).

---

## 5. LOD × destruction matrix (the "configured settings" the game honors)

World detail is a 2-D matrix: **LOD tier × destruction state** — NOT one mesh.
- **LOD tier** — CORRECTED 2026-07-02 (Layer-1 build, verified vs WAD): every c3 block name **LEADS
  with a `c3####` token** (all 9,467 do — there is NO separate leading `c2/c1/c0` prefix). Finer LOD
  is encoded by **how deep the hyphen-chain goes**: a bare `c30001` = the COARSE (c3) rep of a
  quadtree cell; `c39998-c20482-c10053-c00008` = the FINE (c0) rep of that SAME cell (shared leading
  `c3####` anchor). So: `tier` = the FINEST token the chain reaches (last `cN` = tier N, 0=finest);
  `base_cell_id` = the leading `c3####`; group a cell's LODs by that anchor (7,195 of 9,467 are
  chain-named; 2,272 are bare-c3 coarse-only). The `_P###_Q#` suffix is variant/quality and CORRELATES
  with tier (fine=P3/Q0 … coarse=P0/Q3 — verified on cell 39998). Implemented in
  `mercs2_formats::world_index::LodInfo` (`tier`, `base_cell_id`) + `WorldIndex::lod_chain(base_cell)`.
  The streamer picks a tier by camera distance via `lod_chain`. `HibernationControl` COMP carries
  per-entity LOD/visibility state. (vz_state faction tokens also include `jet`/`mec` beyond
  `chi/pir/gur/oil/all/pmc`.)
- **Destruction state** — a SEPARATE overlay axis: `vz_state` files suffixed `_pristine`/`_destroyed`
  /`_captured`/`_staging`/`_defenses` + faction (`chi/pir/gur/oil/all/pmc`); `DestructionLink` and
  `DangerousBuilding` COMPs. The active state selects which overlay blocks load on top of the base.

---

## 6. Texture streaming (PROVEN, and already partly built)

Textures **stream**: a model block ships only the **resident low-res mip TAIL**; the high mips
(mip 0–2/3) stream in via the GLOBAL texture system. PROOF: a 512² Bc1 needs 131 KB for mip 0 but the
resident `all_mips` = **2,728 B = exactly sum(mips 3..7)**; 512² Bc3 = 1,360 B = sum(mips 4..7).
- **Decode fix (DONE in `make_bc_view`):** when full mip 0 is absent, find the largest RESIDENT mip
  level L where `sum(mip_bytes(L..mip_count)) == all_mips.len()`, and build the texture at
  `(w>>L, h>>L)`. Result: low-res but textured (not white). This un-whites ALL streamed props/cells.
- **High-mip streaming — BUILT 2026-07-03** (`wad::extract_texture_hires` + `texture::assemble_hires`).
  MECHANISM (proven via `--tex-audit`/`--tex-locate`): the higher mips are NOT a separate global
  store — they live in the FINER c3-cell LOD blocks of the texture's OWN cell subtree. The resident
  ASET block is `cXXXXX_P000_Q3` (coarse tail); each finer block `cXXXXX-…_P00N_Q(3-N)` carries a
  **lone `BODY` UCFX chunk = exactly ONE finer mip level** (no INFO/NAME). Example: hall texture
  0x6523B88D → 4 blocks, BODY sizes 2885/8240/32816/131120 B = resident tail(mips3-7) / mip2 / mip1 /
  mip0. Assembly: gather every BODY the hash carries across its subtree (blocks whose cell prefix ==
  or starts-with the resident prefix), order by size DESCENDING, concatenate → full mip0..N chain (the
  geometric 4× ratio guarantees size-order == mip-order). `make_bc_view` uploads the full chain;
  sampler `mipmap_filter=Linear`. The game's global registry "last write wins on hash collision" =
  the finest streamed-in copy overwriting the coarse resident one (user's insight). Currently wired
  into the interior SHELL loader (always-visible); extend to near/visible world cells with a
  view-dependent trigger (only upgrade what the camera can see). Formats: BC1/BC3.

---

## 7. Interiors & portals (case study; the SpawnActor pattern)

Interiors are RUNTIME actors, not proximity-streamed placements:
- `mrxhq.lua:657` `MrxUtil.SpawnActor(self.tInterior.sTemplate, "HqInterior", vPosition={3750,450,
  -3840}, self.tInterior.sAnchorHardpoint, self.nRotation=0, …)`. Template =
  `WifHqData.GetHqConfigFromId(sHqName).tInterior.sTemplate` ([`docs/mercs2-luacd/src/vz/wifhqdata.lua`](../../crates/mercs2_script/corpus/mercs2-luacd/src/vz/wifhqdata.lua)
  — faction rooms `AllHq_Interior`/…; PMC starters `_proutpost_interior_job`,
  `_merida_bld_pmcautoshop_interior`). `nRotation` defaults 0, `sAnchorHardpoint` defaults nil.
- **Player teleport** = `MrxUtil._TeleportHero` (mrxutil.lua:490). Verified live coords: PMC INTERIOR
  spawn `(3794.0427, 450.7505, -3911.0322)` (Y ABOVE the ~393 terrain cap → off-map, SE corner);
  exterior/pool `(2560.2646, -13.1779, -926.2511)`. The interior door toggles between them.
- Player is placed at the hardpoint `hp_playerA_enter` on the HqInterior actor
  (`mrxbriefing.lua:272`) → local `(±44, 0, ∓71)` from the actor origin.
- Interior CONTENT = `vz_state_pmcinterior` blocks (**667** base + `_hel` 703 / `_jet` 711 / `_mec`
  461 / `_mecabsent` 291 starter variants) — resolve every entity via the §2A recipe (VERIFIED: 69
  instances / 42k verts = bunks, racks, crates, barrels, + the recruit-bay meshes recruitjet
  `0x86D7CF92` 8,970v / recruitmechanic `0xE8EB75D7` 19,197v placed at the actor origin). Block 3490
  `pmc_interior` is FaceFX/cutscene, NOT geometry.
- **OPEN:** the enclosing hall/floor SHELL mesh (the `HqInterior` template itself) is NOT
  name-resolvable (its name is among the ~790 unreversed model hashes). A geometric ID is possible:
  a large mesh whose local bbox contains the hardpoint `(±44,0,∓71)`. The streamer's actor/template
  layer should resolve this by template→asset like the game does (via an actor-template table if we
  can locate it) OR by the geometric scan.

So the streamer needs a **zone/portal layer** on top of proximity streaming: named zones (interiors)
loaded on trigger (door/`Enter`), with an actor-spawn + hardpoint-teleport mechanic, atmosphere swap
(`rgn_atmo_interior`), and terrain suppressed while inside.

---

## 8. RE'd streaming machinery (the oracle for HOW the engine streams)

We decompiled the streaming path — USE IT as the behavior oracle:
- [`docs/engine_load_path_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/engine_load_path_map.md) + [`scripts/mercs2_annotations.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/scripts/mercs2_annotations.json) — 64 named streaming functions.
- **Buffer-sizing chain (decompile-verified):** dest buffer = `page_count(u24 @ page_record+4) << 15`;
  `FUN_00875b00`. Emit/expect `page_count ≥ ceil(chain_bytes / 0x8000)`.
- `PgSysPopulation @0x823641f0` — registers the population system into the world-update list (the
  per-tick "what should exist near the player" driver).
- `HibernationControl` COMP — per-entity LOD/streaming visibility (hibernate far entities).
  **On-disk layout REVERSED + PARSED (2026-07-02, `placement::load_hibernation`):** 10-byte record
  `{key:u32, dist0:u16, dist1:u8, dist2:u8, dist3:u8, flag:u8}` (the `schm` stride 6 = the 6 payload
  bytes after the key; NOT the in-memory footprint — cf. Transform disk-42 vs schm-52). `dist0` =
  the per-object hibernation (stream-out) distance; `dist1..3` = LOD-tier distances. Verified across
  retail `layers_static`: **2,625 entities carry a directive**; `dist0` min **8** / median **231** /
  max **5037**, with **222 objects > 400** (the "may fall through terrain" band); `dist1..3` are the
  class defaults **160 / 60 / 20** in EVERY record (per-object override lands on `dist0` only here);
  `flag` always 0. Only **20 of 464 ModelName props** carry one — the other ~2,600 directives sit on
  NON-prop entities (c3-baked buildings, destructibles, lights), so the streaming manager reads
  hibernation **per entity-key regardless of how the entity renders**, and props without a directive
  fall back to class defaults (100 / 160 / 60 / 20). `ModelPlacement.hibernation: Option<Hibernation>`
  now carries it; inspect via `mercs2_engine --comp-dump HibernationControl`.
- World-load path debugging corpus: `loadprobe` tool + [`docs/engine_load_path_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/engine_load_path_map.md) +
  [`docs/mercs2-pdb-analysis/world-streaming.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/world-streaming.md).

---

## 9. What already exists to BUILD ON (do not rewrite)

- **`mercs2_core`** — ECS spine: `World` (hecs), `Time` (fixed tick), `Schedule` (ordered systems),
  components `Transform`/`ModelRef`/`AnimState`/`SkinPalette`. The streaming manager is a new SYSTEM
  in this schedule.
- **`mercs2_engine`** — `scene.rs` `Scene` (multi-model renderer: `load_model` idempotent by hash,
  per-entity MVP+palette, `set_view`, sky+fog); `main.rs`: `load_model_by_hash`, `HeightMap` (exact
  triangle ground), `load_c3_cells`, `load_model_props`, `load_pmc_interior`, `make_bc_view`
  (streamed-mip fix), the loading-screen + camera + movement systems. **These bespoke loaders are the
  reference behavior to fold INTO the streaming manager, then retire the ad-hoc flags.**
- **`mercs2_formats`** — `placement` (`load_placements`, `load_model_placements`, `comp_inventory`,
  `Placement`, `ModelPlacement`, COMP walker for both offset conventions); `terrain` (`load_terrain`,
  `TerrainMesh`); `hash::pandemic_hash_m2`; `texture` (`extract_texture`, `TextureData`, `TexFormat`);
  `mesh::build_indexed_from_container`; `ffcs`, `sges`, `ucfx`.

---

## 10. Requirements — what the streaming system MUST do

Build three layers (implementation free; this is the required behavior):

**Layer 1 — World Block Index (build once at startup).** Catalog EVERY block: `block_index`, name
(PTHS), class/type (from its ASET entries), LOD tier + variant (name-suffix parse per §5), and
spatial extent (c3/lrterrain grid formula, or the AABB of a placement block's Transforms). Output a
queryable index: "blocks overlapping (x,z) within radius R, at tier T". Persist/cache if the full
scan is slow.

**Layer 2 == Layer 3 — the streaming decision IS driven by the per-entity control components** (do
NOT split them; CORRECTED 2026-07-02 — the settings are the control INPUT to streaming, not a
downstream pass). The real engine streams PER-ENTITY by each object's own **`HibernationControl`**
(0xe18afd65, schema = 4 int LOD/hibernation distances + 2 bools; class-DEFAULT distances **100 / 160
/ 60 / 20** units, per-object override rare). Objects beyond their hibernation distance are cached
out (`SetHibernationDistance`/`IsHibernated`/`Del::Hibernate`); `NoDelOnHibernateIfPreplaced` exempts
pre-placed objects from deletion; a live warning fires if hibernation distance > 400 ("may fall
through terrain"). `PropPhysics::Activate/Deactivate` streams a prop's physics on/off by proximity
(mirrors `TerrainObject::Activate/Deactivate`). RE: [`docs/mercs2-pdb-analysis/world-streaming.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/world-streaming.md)
§Hibernation; `docs/mercs2-ecs/07_…` §HibernationControl; `PgSysPopulation @0x823641f0` drives it.

The Streaming Runtime (a `mercs2_core` system, per tick) is therefore:
1. **Block residency (coarse, the LOAD unit):** ensure blocks whose extent is within a max working
   radius of the player are decompressed + their entities INSTANTIATED (via the §2 recipes). This is
   the I/O layer — `WorldIndex::blocks_near`. Budget/throttle loads per frame; UNLOAD (despawn +
   FREE GPU — net-new, nothing in the codebase unloads today) blocks that leave the working radius.
2. **Per-entity hibernate/wake (fine, driven by the entity's own control data):** each loaded
   entity carries (from its COMPs, or class defaults) its hibernation distance (default ~160 far,
   ~20 near) + LOD-tier distances (100/60/20). Per tick, by player distance: wake/hibernate the
   entity. THIS is what "respects the configured settings on the model" means; it is not a separate
   layer.
2b. **Per-object geometry streaming — the c3 "tier" is a SIZE-keyed spatial index, NOT LOD (RCA
   CORRECTED 2026-07-02, `mercs2_core::streaming::BlockUnit.stream_out`).** An earlier reading treated
   a cell's coarse (`c3####`) vs fine (deeper-chain) blocks as LOD levels to swap; `--lod-probe` proved
   that WRONG. Real per-tier geometry AABBs: **c0 = 0 geometry blocks; c1 median 6 m / max 53 m /
   ~10.7k verts; c2 median 9 m / max 143 m / ~13.3k verts; c3 median 10 m / MAX 257 m but only ~1.2k
   verts.** The coarse tier holds BIG SIMPLE objects (ground, landmarks, up to 257 m); fine tiers hold
   small detailed props. A cell's blocks are DISTINCT objects at distinct positions (cell 39998: 39
   blocks, 1×1–10×10 m, clustered — 10 distinct real footprints of 16 sampled), not detail levels of a
   shared surface. So the c3/c2/c1 chain is a **loose quadtree keyed by object size** (a Havok/scene
   cull structure), and the correct model is **per-object distance streaming**: each geometry block
   loads/unloads independently, with a `stream_out` distance scaled by its tier (size proxy) —
   `tier_stream_out = [350, 350, 700, 1200]` m for c0..c3, so big landmarks stay visible far and small
   props cull up close. This is the same per-object-distance model as `HibernationControl`, applied to
   baked geometry. NOT done: scaling `stream_out` by each block's TRUE geometry AABB (would need to
   decompress all 1849 blocks at startup) — tier is the cheap proxy. Per-PROP alternate-LOD mesh swap
   is NOT in the data (`--lod-probe`: 766 sub-objects at `state_mask=0x01`, only 2 of 446 prop models
   have geometry needing a tier swap) — not built. Verified via `--stream-probe` (1849 blocks stream by
   per-object tier distance) and `--lod-probe`.
3. **State overlay selection:** the active destruction/mission/faction state picks which `vz_state`
   overlay blocks load on top of the base (`_pristine`/`_destroyed` + `DestructionLink`/
   `DangerousBuilding`).
4. **Texture LOD:** resident-mip now (§6); high-mip on-demand upgrade for near objects (TODO).

**Layer-2 build MUST therefore parse the control COMPs** (currently among the 39 unparsed of 43):
`HibernationControl` (per-entity distances — the streaming policy), `DestructionLink`/
`DangerousBuilding` (state), `LightObject` (lights). Use `tag_registry.rs` (the cataloged 232 engine
FourCCs, 67 Validated / 19 needs_investigation) as the dispatch source of truth rather than scattered
hardcoded tag literals. Prop-granularity caveat: `layers_static` (block 29) is ONE map-spanning block
of 62,624 props — it cannot stream "by block"; bucket its placements spatially (at index time) so
props hibernate/wake per-entity, not per-block.

**Plus a Zone/Portal layer (§7):** named interior zones loaded on trigger (door/enter), actor-spawn +
hardpoint teleport, atmosphere swap, terrain suppression while inside. The PMC interior is the first
instance (spawn at the teleport coord, load the vz_state_pmcinterior blocks + recruit bays).

**Verification the builder must produce:** the index's block/count/extent numbers (should match the
proven totals — 1,771 models, ~1,849 c3 mesh blocks, 62,624 placements, 400 terrain tiles); load/
unload working as the camera moves (log counts); and screenshots for the human to judge (streaming a
populated area; the PMC interior via the zone path). Keep `cargo test -p mercs2_formats` green; no
visual claims from the agent — the human judges shots.

---

## 11. Known-fixed bugs the builder MUST NOT regress
BLENDINDICES per-group palette (§3); bone-count identity palette for static rigged props (§3);
identity fit for placed meshes (§3); streamed-mip texture decode (§6); wavelet anim decode; full-quat
placement rotation; the vz_state ABSOLUTE vs layers_static RELATIVE COMP-offset handling.

## 12. Open / TODO (honest gaps)
- ~~Interior hall/floor SHELL mesh (unnamed hash) — geometric ID or actor-template table (§7).~~
  RESOLVED 2026-07-03: the `HqInterior` actor mesh = `pmcoutpost_interior_hq` → **0x39AF17DC** (a
  non-primary type-19 model, 96,784v/131,834t — the ornate columned hall). Same `pmcoutpost_interior_<room>`
  naming as the recruit bays. NOT `_pmcoutpost_bld_hq_livedin` (that's the EXTERIOR building; see
  wifpmcinterior.lua:563-567 uRealPmc vs uFakePmc). Placed at actor origin (3750,450,-3840).
- ~~High-mip texture streaming/upgrade (§6).~~ BUILT 2026-07-03 (`extract_texture_hires`); remaining:
  a view-dependent trigger so only near/visible world cells assemble hi-res (interior shells done).
- Actor-template registry (SpawnActor `sTemplate` → asset) if we can locate it in the data.
- ~790 of 1,771 model hashes are unreversed (rainbow-table gap) — spatial/geometric ID covers most.

## 13. Cross-references (the full corpus)
Charter [`docs/modernization/00_charter.md`](00_charter.md). Formats: [`docs/placement_data_format.md`](../placement_data_format.md),
[`docs/format_reference.md`](../format_reference.md) §13 (low_res_terrain), [`docs/coordinate_systems.md`](../coordinate_systems.md), `docs/watermap_format`.
Streaming RE: [`docs/engine_load_path_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/engine_load_path_map.md), [`docs/mercs2-pdb-analysis/world-streaming.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/mercs2-pdb-analysis/world-streaming.md),
[`scripts/mercs2_annotations.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/scripts/mercs2_annotations.json). ECS: [`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/) (220 component classes incl.
HibernationControl / DestructionLink / LowResTerrainObject / ModelName). Lua: [`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd)
(mrxhq, wifhqdata, wifpmcinterior, mrxutil, mrxstarter). Memory index summarises the live state of
every subsystem.
