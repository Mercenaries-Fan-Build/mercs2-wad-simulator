# Type Hash Registry — Mercenaries 2 ECS Component Types

Complete enumeration of `type_hash` values from every block in the retail `vz.wad`.

> **★ `type_id` provenance (corrected 2026-08-01).** The `ASET type_id` column is now taken from
> the **WAD's own type table**, not derived: a flat array of `type_hash` u32s indexed by `type_id`
> at file offset `0x48`, immediately after the 0x48-byte FFCS header, length = header dword 8 (36
> in retail), identical across all four shipped WADs.
>
> Nine rows here were wrong — `fxdict`/`watermap` transposed, `level` 20 for 26, `musicstatemap`
> 26 for 8, `worldentity` 8 for 17, `musiccue` 4 for 10, `decaltable` 1 for 4, `guidmap` 10 for 1,
> `sequencetable` 33 for 24, `animstatemachine` 31 for 33. [`docs/fixpack/wad_duplicate_inventory.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/fixpack/wad_duplicate_inventory.md)
> Appendix C reported this in July (139 hit / 0 miss) and it was never applied here.
>
> `type_id` is the byte the engine indexes its loader table with, so a wrong one dispatches an asset
> to the wrong loader. **Read the in-WAD table; do not re-derive this.**
> `mercs2_formats/tests/type_ids_match_the_wad.rs` pins the code against it at test time.
>
> Still inherited elsewhere: [`docs/data/aset_export.csv`](data/aset_export.csv)'s `type_name`/`type_hash` columns (its
> `type_id` and `asset_hash` columns are sound).

Type hashes are computed via `pandemic_hash_m2()` (FNV-1a with `|0x20` case suppression + `^0x2A * FNV_PRIME` finalization). Resolved names confirmed by hash collision.

## Summary

- **WAD file**: `game-files/vz.wad` (2,565,537,792 bytes)
- **Total blocks in WAD**: 11,370
- **Blocks processed**: 11,367 (3 failed to decompress)
- **Total UCFX entries**: 55,425
- **Unique type_hash values**: 35
- **Resolved by name**: 35 / 35
- **ASET entries**: 30,645 (each maps 1:1 to a block sub-entry via `type_id`)

---

## Master Type Table

Sorted by frequency (most common first). ASET `type_id` is the integer discriminator stored in the ASET chunk that maps 1:1 to the `type_hash`.

| # | type_hash | ASET type_id | Entries | Blocks | Resolved Name | Description |
|---|-----------|-------------|---------|--------|---------------|-------------|
| 1 | `0xF011157A` | 27 | 36,724 | 9,482 | **texture** | DDS texture data (compressed BODY chunks). Dominates c3 cell blocks |
| 2 | `0xBCFE6314` | 28 | 5,194 | 1 | **path** | Registry/configuration entries (52–116 bytes each). All in `resident` block |
| 3 | `0x5B724250` | 19 | 4,407 | 2,180 | **model** | Mesh geometry (HIER, MTRL, SEGM, GEOM, MESH, INDX + Havok physics) |
| 4 | `0x18166555` | 16 | 4,261 | 191 | **animation** | Havok 5.5 animation/skeleton packfile data |
| 5 | `0x600B904E` | 12 | 1,026 | 261 | **scrub** | Shader resource binary (SCRB+MTRL+STRM+INFO chunks). Compiled shader/material packages, up to ~4 per c3 cell |
| 6 | `0xE6B81A54` | 9 | 923 | 751 | **layer** | Placement/entity layer data. 81% in `vz_state`, 19% in `layers_static` |
| 7 | `0x42498680` | 35 | 645 | 107 | **script** | Lua bytecode (BINN chunk containing LuaQ compiled scripts) |
| 8 | `0x6310807F` | 30 | 625 | 1 | **lineregion** | Spatial line/region definitions. All 625 in `resident` block (zone boundaries, patrol paths, trigger volumes) |
| 9 | `0x7C569307` | 32 | 400 | 400 | **terrainmesh** | One per c3 cell block — terrain mesh geometry for that cell |
| 10 | `0x1602815C` | 22 | 400 | 1 | **lowresterrain** | All 400 tiles in `low_res_terrain` block (20×20 grid) |
| 11 | `0x5608BD5A` | 29 | 314 | 1 | **effect** | Particle/VFX definitions. All 314 in `effects` block |
| 12 | `0xF753F6D0` | 6 | 95 | 87 | **wavebank** | Audio wave bank data. In vehicle, weapon, ambient sound blocks |
| 13 | `0x665EF13E` | 5 | 86 | 38 | **facefxanimationset** | FaceFX facial animation sets (44–46 KB). In contract/briefing/mission blocks for dialogue lip-sync |
| 14 | `0xE5273C14` | 13 | 77 | 69 | **sounddb** | Cue routing table: cue guid → (soundbank, soundbank cue index). Beside the same-named soundbank in its block; the global one holds the category tree |
| 15 | `0x9F8BCA10` | 21 | 76 | 68 | **soundbank** | Sound bank data. Same blocks as wavebank entries |
| 16 | `0xFE0E8320` | 23 | 60 | 29 | **scaleformgfx** | Scaleform GFX UI assets (inner CFX + zlib-compressed payload). In c316XX and contract blocks |
| 17 | `0x1CF649BB` | 34 | 31 | 28 | **facefxactor** | FaceFX actor definitions (facial animation rigs). In starter blocks and misc c3 blocks |
| 18 | `0xFA0B8DBC` | 18 | 22 | 1 | **chatter** | NPC chatter/ambient dialogue configuration. All 22 in `resident` block |
| 19 | `0x207359C7` | 11 | 15 | 1 | **animationtable** | Animation lookup/mapping tables. All 15 in `resident` block |
| 20 | `0x8F0A54E2` | 3 | 14 | 4 | **binary** | Raw binary data. In `ps3saveassets` blocks |
| 21 | `0x99E77ACE` | 15 | 9 | 3 | **font** | Font data. In `japanese`, `resident` blocks |
| 22 | `0xDE982D61` | 14 | 6 | 1 | **materialparam** | Material/shader parameter tables (INFO+DATA). Float4 arrays — colors, specular, etc. 268B–95KB |
| 23 | `0x39E5E978` | 7 | 3 | 3 | **stringdb** | Localized string database. In `japanese`, `allcaps`, `english` blocks |
| 24 | `0x59B9DF6A` | — | 1 | 1 | **materialtable** | Singleton in `resident` block |
| 25 | `0x4D7D30C4` | — | 1 | 1 | **watermap** | Singleton in `resident` block |
| 26 | `0x34612F86` | — | 1 | 1 | **foliage** | Singleton in `resident` block |
| 27 | `0xACCE47F2` | 24 | 1 | 1 | **sequencetable** | Sequence/cinematic timeline table. Singleton in `c33364` with `sequ`/SINF/ITEM chunks |
| 28 | `0xC122545A` | 8 | 1 | 1 | **musicstatemap** | Music state map. Hash→float parameters (volume, crossfade). 48KB. ASET name_hash `0xB4420059`="VZ". In `musicdata` block |
| 29 | `0xE8DF4D87` | 10 | 1 | 1 | **musiccue** | Music cue table. Hash→index→float timing entries. 23KB. In `musicdata` block |
| 30 | `0xECE70371` | 33 | 1 | 1 | **animstatemachine** | Animation state machine (SINF/AINF/TRNS/stns/actn). Saboteur equivalent: SEQC/TRAN/EDGE/BANK. 561KB |
| 31 | `0xEA4829D5` | 26 | 1 | 1 | **level** | Singleton in `resident` block |
| 32 | `0x3B0AABF8` | 4 | 1 | 1 | **decaltable** | Decal definition table. Singleton in `resident` block |
| 33 | `0x5647C35D` | 17 | 1 | 1 | **worldentity** | World entity data — master ECS/placement container (CHDR/UNIQ/COMP/info/schm/data). Same format as `layers_static`. 1.8MB |
| 34 | `0x140E8728` | 1 | 1 | 1 | **guidmap** | GUID mapping table. Singleton in `resident` block |
| 35 | `0xFA46D8A8` | 0 | 1 | 1 | **fxdict** | Effect dictionary — FX parameter definitions (INFO+DICT). Particle sizes, rates, etc. 12KB. name="fx" |

---

## Resolved Names Summary

All 35 of 35 type hashes have been resolved to their original string via `pandemic_hash_m2()`. The final 11 were cracked via a 64k-entry rainbow table brute-force:

```
pandemic_hash_m2("texture")        = 0xF011157A   (36,724 entries)
pandemic_hash_m2("path")           = 0xBCFE6314   ( 5,194 entries)
pandemic_hash_m2("model")          = 0x5B724250   ( 4,407 entries)
pandemic_hash_m2("animation")      = 0x18166555   ( 4,261 entries)
pandemic_hash_m2("scrub")          = 0x600B904E   ( 1,026 entries)
pandemic_hash_m2("layer")          = 0xE6B81A54   (   923 entries)
pandemic_hash_m2("script")         = 0x42498680   (   645 entries)
pandemic_hash_m2("lineregion")     = 0x6310807F   (   625 entries)
pandemic_hash_m2("terrainmesh")    = 0x7C569307   (   400 entries)
pandemic_hash_m2("lowresterrain")  = 0x1602815C   (   400 entries)
pandemic_hash_m2("effect")         = 0x5608BD5A   (   314 entries)
pandemic_hash_m2("wavebank")       = 0xF753F6D0   (    95 entries)
pandemic_hash_m2("facefxanimationset") = 0x665EF13E (   86 entries)
pandemic_hash_m2("sounddb")        = 0xE5273C14   (    77 entries)
pandemic_hash_m2("soundbank")      = 0x9F8BCA10   (    76 entries)
pandemic_hash_m2("scaleformgfx")   = 0xFE0E8320   (    60 entries)
pandemic_hash_m2("facefxactor")    = 0x1CF649BB   (    31 entries)
pandemic_hash_m2("chatter")        = 0xFA0B8DBC   (    22 entries)
pandemic_hash_m2("animationtable") = 0x207359C7   (    15 entries)
pandemic_hash_m2("binary")         = 0x8F0A54E2   (    14 entries)
pandemic_hash_m2("font")           = 0x99E77ACE   (     9 entries)
pandemic_hash_m2("materialparam")  = 0xDE982D61   (     6 entries)
pandemic_hash_m2("stringdb")       = 0x39E5E978   (     3 entries)
pandemic_hash_m2("materialtable")  = 0x59B9DF6A   (     1 entry)
pandemic_hash_m2("watermap")       = 0x4D7D30C4   (     1 entry)
pandemic_hash_m2("foliage")        = 0x34612F86   (     1 entry)
pandemic_hash_m2("sequencetable")  = 0xACCE47F2   (     1 entry)
pandemic_hash_m2("level")          = 0xEA4829D5   (     1 entry)
pandemic_hash_m2("decaltable")     = 0x3B0AABF8   (     1 entry)
pandemic_hash_m2("guidmap")        = 0x140E8728   (     1 entry)
pandemic_hash_m2("musicstatemap")  = 0xC122545A   (     1 entry)
pandemic_hash_m2("musiccue")       = 0xE8DF4D87   (     1 entry)
pandemic_hash_m2("animstatemachine") = 0xECE70371 (     1 entry)
pandemic_hash_m2("worldentity")    = 0x5647C35D   (     1 entry)
pandemic_hash_m2("fxdict")         = 0xFA46D8A8   (     1 entry)
```

---

## Newly Resolved Type Hashes — Analysis

The final 11 type names were resolved via a 64,216-entry rainbow table brute-force against `pandemic_hash_m2()`. Several reveal middleware integrations (FaceFX, Scaleform) that were not obvious from UCFX chunk analysis alone.

### `0x600B904E` → **scrub** — ASET type_id 12 — 1,026 entries across 261 blocks

Shader resource binary packages. UCFX chunk data contains SCRB+MTRL+STRM+INFO sub-chunks — compiled shader programs with associated material definitions. Appears in c3 cell blocks alongside textures and models, up to ~4 per cell.

### `0x6310807F` → **lineregion** — ASET type_id 30 — 625 entries in `resident` block

Spatial line/region definitions. All 625 entries in the always-loaded `resident` block — zone boundaries, patrol paths, trigger volumes, and other spatial primitives used by the gameplay systems.

### `0x665EF13E` → **facefxanimationset** — ASET type_id 5 — 86 entries across 38 blocks

FaceFX facial animation sets for dialogue lip-sync. Found in contract blocks (`mec001_contract`), briefing blocks (`briefing_job_chris`), and mission-flow blocks. The large entry size (44–46 KB) reflects pre-baked FaceFX animation curves for full cutscene performances.

### `0xE5273C14` → **sounddb** — ASET type_id 13 — 77 entries across 69 blocks

The cue routing table. A per-bank sounddb (76 in `vz.wad`) sits in the same block as, and under the same name hash as, its bank's `soundbank`, and holds one 12-byte entry per soundbank cue: `{cue guid = m2(cue name), soundbank hash, cue index in that soundbank}`, sorted by guid. `Sound.CueSound(name)` finds the entry, then the soundbank cue, whose group names the `wavebank` wave. The global `mercs2globals` sounddb (188 bytes) has no cue entries; it holds the 19-entry sound category tree and two parameter hashes. Sizes run from 112 bytes to several KB. Byte layout: [`reverse_engineer/audio_code_map.md` §11.3](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md).

### `0xFE0E8320` → **scaleformgfx** — ASET type_id 23 — 60 entries across 29 blocks

Scaleform GFX UI middleware assets. Inner CFX container with zlib-compressed Flash/SWF-derived UI layouts. Found in c316XX and contract blocks — HUD elements, menu screens, and in-game UI overlays compiled through Scaleform's GFx pipeline.

### `0x1CF649BB` → **facefxactor** — ASET type_id 34 — 31 entries across 28 blocks

FaceFX actor definitions — the facial animation rig configurations that pair with `facefxanimationset` data. Found in starter blocks (`allstarter0`, `gurstarter0`) and character/mission c3 blocks.

### `0xFA0B8DBC` → **chatter** — ASET type_id 18 — 22 entries in `resident` block

NPC chatter/ambient dialogue configuration tables. All 22 in the always-loaded `resident` block — defines the rules for contextual NPC barks (combat callouts, idle chatter, faction greetings).

### `0x207359C7` → **animationtable** — ASET type_id 11 — 15 entries in `resident` block

Animation lookup/mapping tables. All 15 in `resident` — maps logical animation names to Havok animation assets, providing the indirection layer between gameplay state machines and raw animation clips.

### `0xACCE47F2` → **sequencetable** — ASET type_id 33 — 1 entry in `c33364`

Sequence/cinematic timeline table. UCFX data contains `sequ`/SINF/ITEM chunks — a timeline-based sequence editor format for scripted in-game cinematics.

### `0x3B0AABF8` → **decaltable** — ASET type_id 1 — 1 entry in `resident`

Global decal definition table. Singleton in `resident` — defines bullet holes, explosion marks, blood splatter, tire tracks, and other projected decal materials.

### `0x140E8728` → **guidmap** — ASET type_id 10 — 1 entry in `resident`

GUID mapping table. Singleton in `resident` — maps globally unique identifiers to internal asset/entity references, providing stable cross-reference identifiers for save/load and network synchronization.

---

## Block Category Distribution

How type hashes distribute across block categories:

| Category | Description | Dominant Types |
|----------|-------------|----------------|
| c3 cells (~9,400 blocks) | Spatial streaming cells | texture (×3–4), model (×1–2), terrainmesh (×1), scrub (×1–4) |
| vz_state (746 blocks) | Game state overlays | layer (×1 each) |
| layers_static (173 blocks) | Static world placement | layer (×1 each) |
| resident (1 block, ~6,500 entries) | Always-loaded global data | path (5,194), lineregion (625), chatter (22), animationtable (15), font, materialtable, watermap, foliage, level, decaltable, guidmap |
| vehicle/weapon (~87 blocks) | Entity definitions | wavebank, soundbank, sounddb, animation |
| effects (1 block, 314 entries) | Particle/VFX library | effect (314) |
| low_res_terrain (1 block, 400 entries) | Low-res terrain tiles | lowresterrain (400) |
| scripts (~107 blocks) | Lua game logic | script (×1–6 per block) |
| contract/briefing (~38 blocks) | Mission data | facefxanimationset (dialogue lip-sync) |
| animation (~191 blocks) | Character animations | animation (×1–22 per block) |
| language (3 blocks) | Localization | stringdb |
| musicdata (1 block) | Music definitions | musicstatemap, musiccue |
| ps3saveassets (4 blocks) | Platform save icons | binary |

---

## ASET type_id ↔ type_hash Complete Mapping

The ASET chunk uses small integer `type_id` values (0–35) that map 1:1 to `type_hash` values in decompressed block headers:

| type_id | type_hash | Resolved Name | ASET Count |
|---------|-----------|---------------|------------|
| 0 | `0xFA46D8A8` | fxdict | 1 |
| 1 | `0x3B0AABF8` | decaltable | 1 |
| 3 | `0x8F0A54E2` | binary | 14 |
| 4 | `0xE8DF4D87` | musiccue | 1 |
| 5 | `0x665EF13E` | facefxanimationset | 86 |
| 6 | `0xF753F6D0` | wavebank | 95 |
| 7 | `0x39E5E978` | stringdb | 3 |
| 8 | `0x5647C35D` | worldentity | 1 |
| 9 | `0xE6B81A54` | layer | 923 |
| 10 | `0x140E8728` | guidmap | 1 |
| 11 | `0x207359C7` | animationtable | 15 |
| 12 | `0x600B904E` | scrub | 1,026 |
| 13 | `0xE5273C14` | sounddb | 77 |
| 14 | `0xDE982D61` | materialparam | 6 |
| 15 | `0x99E77ACE` | font | 9 |
| 16 | `0x18166555` | animation | 4,261 |
| 17 | `0xECE70371` | animstatemachine | 1 |
| 18 | `0xFA0B8DBC` | chatter | 22 |
| 19 | `0x5B724250` | model | 3,007 |
| 20 | `0xEA4829D5` | level | 1 |
| 21 | `0x9F8BCA10` | soundbank | 76 |
| 22 | `0x1602815C` | lowresterrain | 400 |
| 23 | `0xFE0E8320` | scaleformgfx | 60 |
| 24 | `0xC122545A` | musicstatemap | 1 |
| 25 | `0xFA46D8A8` | fxdict | 1 |
| 26 | `0xC122545A` | musicstatemap | 1 |
| 27 | `0xF011157A` | texture | 13,340 |
| 28 | `0xBCFE6314` | path | 5,194 |
| 29 | `0x5608BD5A` | effect | 314 |
| 30 | `0x6310807F` | lineregion | 625 |
| 31 | `0xECE70371` | animstatemachine | 1 |
| 32 | `0x7C569307` | terrainmesh | 400 |
| 33 | `0xACCE47F2` | sequencetable | 1 |
| 34 | `0x1CF649BB` | facefxactor | 31 |
| 35 | `0x42498680` | script | 645 |

---

## Name Hash Samples

For each type hash, the first 10 unique `name_hash` values observed. These are `pandemic_hash_m2()` hashes of the original asset names (which are not stored in the WAD).

| type_hash | Name | Unique Names | Sample name_hashes |
|-----------|------|-------------|-------------------|
| `0xF011157A` | texture | 13,340 | `0x000C9002` `0x000FCC62` `0x00108D8E` `0x00140A1E` `0x001577A0` ... |
| `0xBCFE6314` | path | 5,194 | `0x00043D4F` `0x0007BF84` `0x000BF11E` `0x00109310` `0x0030C486` ... |
| `0x5B724250` | model | 3,007 | `0x001616F7` `0x0045B675` `0x00461E64` `0x006243A4` `0x0065B09D` ... |
| `0x18166555` | animation | 4,261 | `0x00312661` `0x004DAFC0` `0x005381C4` `0x006C8D0B` `0x006ECADD` ... |
| `0x600B904E` | scrub | 1,026 | `0x000EE46F` `0x007839B1` `0x0115F817` `0x011ACDCC` `0x02569157` ... |
| `0xE6B81A54` | layer | 923 | `0x00360A74` `0x0055E642` `0x0091BD04` `0x00A8865A` `0x00B15690` ... |
| `0x42498680` | script | 645 | `0x00755CE5` `0x00F56A80` `0x00FA985C` `0x01330EB2` `0x01A266D2` ... |
| `0x6310807F` | lineregion | 625 | `0x00529F1C` `0x0076650A` `0x00C136F9` `0x019024B5` `0x01C06833` ... |
| `0x7C569307` | terrainmesh | 400 | `0x0057041A` `0x0086E8BE` `0x012BFC96` `0x01DB0F57` `0x02C4975F` ... |
| `0x1602815C` | lowresterrain | 400 | `0x0035EC36` `0x00DFC11D` `0x02005FA2` `0x023AB4A7` `0x0241A69A` ... |

---

## Verification

Generated by [`tools/enumerate_type_hashes.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/enumerate_type_hashes.py) which:
1. Parses FFCS header without loading the full WAD into memory
2. Reads INDX entries to compute block offsets (`page_index * 0x8000`)
3. Uses mmap to decompress each block's header via `sges_decompress` (only the first `4 + count*16` bytes)
4. Collects `(type_hash, name_hash)` pairs from each block's entry table

The JSON output with full data is at `output/type_hash_registry.json`.

Type name resolution uses `pandemic_hash_m2()` from [`tools/pandemic_hash.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/pandemic_hash.py) — the same FNV-1a variant verified against the game executable (166+ call sites).
