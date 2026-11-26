---
status: current
evidence: proven
verified_on: 2026-07-21
witness: whole-WAD sweep (mercs2_probe --bin aset_decode) over 16,347 model+texture rows, cross-checked against independent geometry scans (model_blocks) for civ_hum_beachfemale_a and ch_veh_tank_ztz98
supersedes: [docs/aset_format.md@2026-05-18]
---

# ASET chunk — row layout (Mercenaries 2 `vz.wad`)

**Date:** 2026-05-18 · **field decode corrected 2026-07-21**  
**Status:** **Fully verified** — all four fields decoded. `u32_0` = asset name hash, `u32_2 >> 16` = block index confirmed by decompressing blocks and matching sub-entry headers.  

> **⚠ CORRECTION (2026-07-21).** The `+4` and `+8` low-half descriptions below were **wrong** for
> two months and were marked "Verified: Yes" the whole time, which is worse than having been
> absent. `secondary_ref` does not hold *dependency hashes*, and `packed_block_ref` low16 is not a
> *sub-entry offset*. **Both words hold BLOCK INDICES, and together they encode the asset's entire
> LOD chain.** See "LOD chain" below. Reproduce with
> `cargo run -p mercs2_probe --bin aset_decode`.

**Decoder:** [`tools/aset_decoder.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/aset_decoder.py) → `output/block_dependency_graph.json`  
**Tracer:** [`tools/aset_prop_tracer.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/aset_prop_tracer.py) — traces asset hashes through ASET→PTHS

## Manifest facts (retail)

From `output/extracted/ffcs_vz/manifest.json`:

| Chunk | `meta` (row count) | `size` (bytes) | Bytes / row |
|-------|-------------------|----------------|-------------|
| ASET | 30,645 | 490,320 | **16** |

Demo `vz.wad` uses the same **16 bytes/row** (see [`docs/demo_corpus.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/demo_corpus.md)).

## Row layout (little-endian)

Each row is four `uint32`:

| Offset | Field | Verified? | Notes |
|--------|-------|-------------|-------|
| +0 | `asset_hash` | **Yes** | FNV-1a hash (with `\|0x20` case suppression and `^0x2A * prime` finalization) of the asset's internal name. Matches `name_hash` field in decompressed block sub-entry headers. **30,006 unique hashes** across 30,645 rows. Confirmed identical between Mercenaries 2 and The Saboteur. |
| +4 | `secondary_ref` | **Yes** (2026-07-21) | **Two block indices, not hashes.** `0xFFFFFFFF` = neither rung present. **High 16** = the `_P002` block index; **low 16** = the `_P003` block index (`0xFFFF` = absent). Measured: of model+texture rows, 8,449 name a `_P002` block and 5,537 name a `_P003` block — **100%** in the primary's own cell subtree, **0** out of range. |
| +8 | `packed_block_ref` | **Yes** | **High 16 bits** = block index into PTHS/INDX (verified: 3,581 unique blocks referenced) — the `_P000` resident rung. **Low 16 bits** = `0xFFFF` when there is no finer rung (19,847 rows), otherwise **the `_P001` block index** — *not* a sub-entry offset. Measured: 10,798 non-sentinel rows, **100%** naming a `_P001` block in the same cell subtree, exactly one LOD level finer, **0** out of range. |
| +12 | `type_id` | **Yes** | Type discriminator (0–35). Maps 1:1 to the `type_hash` field in decompressed block sub-entry headers. See type table below. |

## The LOD chain lives in the ASET row

A row does not name one block. It names **up to four**, one per LOD rung, packed into the two
u32s as `[hi16][lo16]` pairs:

| rung | field | half | sentinel |
|---|---|---|---|
| `_P000` (resident, coarsest) | `packed_block_ref` | hi16 | — always present |
| `_P001` | `packed_block_ref` | lo16 | `0xFFFF` |
| `_P002` | `secondary_ref` | hi16 | `0xFFFF` (whole word `0xFFFFFFFF` = neither) |
| `_P003` (finest) | `secondary_ref` | lo16 | `0xFFFF` |

Worked example — `ch_veh_tank_ztz98` (`0xF88147A1`):

```
packed_block_ref = 0x0DED14D7   ->  hi16 3565 = ch_veh_tank_ztz98_P000_Q3   (4,435 tri)
                                    lo16 5335 = ch_veh_tank_ztz98_P001_Q2  (18,333 tri)
secondary_ref    = 0x2093FFFF   ->  hi16 8339 = ch_veh_tank_ztz98_P002_Q1  (28,620 tri)
                                    lo16 0xFFFF = no _P003 rung
```

An independent geometry scan (`mercs2_probe --bin model_blocks`) finds chunks for that hash in
exactly blocks 3565, 5335 and 8339 — the three the row names, and no others.

**Why this matters for authoring.** "Is this asset safe to clone?" is **not** answered by the
primary block alone, and not by a single `sub` field either. An asset is genuinely single-block
only when **both** halves are sentinel:

```rust
let single_block = entry.packed_block_ref & 0xFFFF == 0xFFFF
                && entry.secondary_ref == 0xFFFF_FFFF;
```

Every block a chain names holds the asset exactly once: 35,582 of 35,582 rungs in retail `vz.wad`. The
engine installs rungs coarsest first and re-queues one that arrives early
([`reverse_engineer/render_core_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_core_code_map.md) §8), so `aset_refcheck` enforces it.

Clone a multi-rung asset by copying only its `_P000` block and you ship the coarsest tier with no
finer rungs behind it. Use `AsetEntry::lod_chain()` (`mercs2_formats::ffcs`) rather than
re-deriving the packing.

**How the engine reads the row.** `FUN_00875760` reads the row's `+4..+0xB` as four u16 block indices,
`[_P003, _P002, _P001, _P000]`, and indexes them by **slot `3-N`** for rung `_P00N`; the block index it
takes is the one it streams. Nothing on that path reads a block's PTHS path. The block's INDX tier byte
(`format_reference.md` §2, `INDX`) rides along as the stream node's eviction class
([`reverse_engineer/render_core_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_core_code_map.md) §8).

**What a model's rungs hold** (`mercs2_probe model-chains <vz.wad> <out.tsv>`, retail PC, exits 0 when all
of it holds). 3,007 model rows name 4,407 rungs — 3,007 `_P000`, 1,236 `_P001`, 164 `_P002`, 0 `_P003`:

- each rung block holds the model's `(hash, model)` entry exactly once, and no chain has a gap;
- each rung block's INDX tier is `N` and its path ends `_P00N_Q(3-N)`;
- the destruction machine (`SWIT`/`NODE`/`STAT`/`CHDR`/`CEXE`) is in `_P000` only — 1,311 models, 1,059 of
  them with finer rungs;
- each entry's third word (`field_c`) is the byte sum of the model's containers in the rungs before it:
  0 in `_P000`, the `_P000` container's length in `_P001`, and so on. The rung install loop
  (`FUN_00875d80`) does not read it; other readers are unchecked. Re-emitting a chain whose `_P000`
  changed size therefore restates every finer rung's word;
- 686 of the finer-rung blocks also hold other assets (up to 73).

### ⚠ The Xbox layout is MIRRORED — do not apply the PC decode to a 360 WAD

Everything above is measured on **PC** `vz.wad`. The 360 bake packs the same two block indices in
the **opposite half order**, so reasoning about an Xbox-side tool using the PC rule is invalid.
Same asset, `civ_hum_beachfemale_a` (`0xFA572E52`):

| | `packed_block_ref` | hi16 | lo16 |
|---|---|---|---|
| PC | `0x083E11EB` | 2110 → `c32143_P000_Q3` (**primary**) | 4587 → `c32143-c21152_P001_Q2` (`_P001`) |
| Xbox | `0x11E2083E` | 4578 → `c32143-c21152_P001_Q2` (`_P001`) | 2110 → `c32143_P000_Q3` (**primary**) |

Measured across the whole 360 WAD (16,260 model+texture rows), the mirror is exact — every `u32`
has its two `u16` halves swapped relative to PC:

| word | PC | Xbox |
|---|---|---|
| `packed_block_ref` | `[hi16 _P000][lo16 _P001]` | `[hi16 _P001][lo16 _P000]` |
| `secondary_ref` | `[hi16 _P002][lo16 _P003]` | `[hi16 _P003][lo16 _P002]` |

Xbox `packed_block_ref` low16: 16,260 non-sentinel rows, **100%** naming a `_P000` block.
Xbox `secondary_ref` low16: 9,365 rows, **100%** `_P002`, **100%** in the primary's own cell subtree.
That half-swap is exactly a u16-granular byteswap of each word, which is what the Xbox→PC
conversion performs.

Two further traps in the 360 file:

- **The ASET chunk is mixed-endian.** The container detects as `Big` and each `u16` block index
  reads correctly big-endian, but `type_id` comes back as `0x13000000` rather than `19` — it is
  stored little-endian. `ffcs::parse_aset_entries` applies one endianness to the whole row, so
  **30,552 of 30,553 Xbox rows decode with a nonsense `type_id`**. Any filter on `type_id == 19`
  silently matches nothing on a 360 WAD. (Consistent with the known "BE container is a MIX" rule.)
- Consequently, `mercs2_probe --bin aset_decode` prints a loud warning and refuses to interpret its
  own numbers when more than half the rows show `type_id > 64`. A measurement that silently returns
  zeros is worse than one that fails.

**Practical consequence.** `ucfx_byteswap::recompute_block_aset_subs` writes a physical entry-table
ordinal into the low half. Under the PC rule that would be plainly wrong — but it operates on
**Xbox** input, where that half is the primary block index and the converter's job is precisely to
re-lay the pair for PC. It was reported as a latent shipping bug; **that report is not supported by
evidence** and the function is unchanged pending a measurement of the Xbox semantics in their own
terms.

### What this corrects

- `secondary_ref` was documented as *"secondary block reference **hashes** for streaming
  dependencies"*. They are **block indices**, and they are this asset's own finer LOD rungs.
- `packed_block_ref` low16 was documented as *"a sub-entry offset within the block"*. It is a
  **block index**. Anything treating it as an intra-block ordinal is wrong — including
  `ucfx_byteswap::recompute_block_aset_subs`, which writes a physical entry ordinal into it, and
  the `max_sub >= entry_count` check in [`tools/diagnose_patch_wad.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/diagnose_patch_wad.py), which validates against the
  wrong quantity entirely.
- The claim that `_P002`/`_P003` blocks are *never referenced from ASET* is false: 8,449 and 5,537
  rows name them respectively. They are simply named by the **other** word.

## Type discriminator table

See also: [`docs/type_hash_registry.md`](type_hash_registry.md) for the complete 35-type registry with block distribution analysis.

**35 unique `type_hash` values** exist across vz.wad (55,425 UCFX entries), all 35 resolved to names (see [`type_hash_registry.md`](type_hash_registry.md) for the complete registry):

| `type_id` | `type_hash` | Resolved Name | Count | Description |
|-----------|-------------|---------------|-------|-------------|
| 27 | `0xf011157a` | **texture** | 13,340 | Texture (DDS body) — same as `FORMAT_HASH_C3_BODY` |
| 28 | `0xbcfe6314` | **path** | 5,194 | Registry / configuration data (small 52–116 byte entries) |
| 16 | `0x18166555` | **animation** | 4,261 | `pandemic_hash_m2("animation") == 0x18166555` — Havok 5.5 animation / skeleton data. This hash is the magic constant in animgroup record headers. |
| 19 | `0x5b724250` | **model** | 3,007 | **Mesh / geometry** — same as `FORMAT_HASH_C3_MESH` |
| 12 | `0x600b904e` | **scrub** | 1,026 | Shader resource binary (SCRB+MTRL+STRM+INFO chunks) |
| 9 | `0xe6b81a54` | **layer** | 923 | Placement/entity layer data (vz_state + layers_static) |
| 35 | `0x42498680` | **script** | 645 | Lua bytecode (BINN/LuaQ) |
| 30 | `0x6310807f` | **lineregion** | 625 | Spatial line/region definitions (all in resident block) |
| 32 | `0x7c569307` | **terrainmesh** | 400 | Terrain mesh geometry (one per c3 cell) |
| 22 | `0x1602815c` | **lowresterrain** | 400 | Low-res terrain tiles (20×20 grid) |
| 29 | `0x5608bd5a` | **effect** | 314 | Particle/VFX definitions (all in effects block) |
| 6 | `0xf753f6d0` | **wavebank** | 95 | Audio wave bank data |
| 5 | `0x665ef13e` | **facefxanimationset** | 86 | FaceFX facial animation sets (contract/briefing blocks) |
| 13 | `0xe5273c14` | **sounddb** | 77 | Sound database metadata (vehicle/weapon blocks) |
| 21 | `0x9f8bca10` | **soundbank** | 76 | Sound bank data |
| 23 | `0xfe0e8320` | **scaleformgfx** | 60 | Scaleform GFX UI assets (compressed CFX payload) |
| 34 | `0x1cf649bb` | **facefxactor** | 31 | FaceFX actor definitions (starter/misc blocks) |
| 18 | `0xfa0b8dbc` | **chatter** | 22 | NPC chatter/ambient dialogue config (resident) |
| 11 | `0x207359c7` | **animationtable** | 15 | Animation lookup/mapping tables (resident) |
| 3 | `0x8f0a54e2` | **binary** | 14 | Raw binary data (ps3saveassets) |
| 15 | `0x99e77ace` | **font** | 9 | Font data |
| 14 | `0xde982d61` | **materialparam** | 6 | Material/shader parameter tables (resident) |
| 7 | `0x39e5e978` | **stringdb** | 3 | Localized string database |
| 20 | `0xea4829d5` | **level** | 1 | Singleton in resident |
| + 12 more singleton types | — | — | 1 each | See [type_hash_registry.md](type_hash_registry.md) |

## Block distribution

ASET entries point to blocks by category:

| Category | ASET rows | % |
|----------|-----------|---|
| c3 cell (world geometry) | 15,324 | 50.0% |
| resident (always-loaded pack) | 7,018 | 22.9% |
| Vehicle blocks | 3,408 | 11.1% |
| Building / outpost blocks | 642 | 2.1% |
| Character blocks | 544 | 1.8% |
| vz_state overlays | 746 | 2.4% |
| resident2 (secondary pack) | 333 | 1.1% |
| Other | 2,630 | 8.6% |

### Where are the prop meshes?

Of the 3,007 mesh-type (type 19) ASET entries:
- **2,693 (89.6%)** are in c3 cell blocks
- **99** in resident2
- **82** in vehicle blocks
- **48** in building blocks
- **7** in the resident pack

Props like trees, rocks, fences, and street furniture are **embedded in c3 cell blocks**, not in standalone blocks. Each c3 cell contains all meshes + textures needed for that spatial region.

## Asset name hash opacity

The `asset_hash` values are FNV-1a hashes (with `|0x20` case suppression and `^0x2A * prime` finalization) of internal asset names. These names are **not stored** anywhere in the WAD — only the hash survives. The original asset names were defined during Pandemic's build pipeline. Entity names from placement data (e.g., `_global_env_rocksbeach01`) are **instance names**, not asset names, and do not hash to ASET entries.

The hash algorithm is identical to The Saboteur's, confirming shared Zero Engine lineage (Mercenaries 1 → Mercenaries 2 → The Saboteur). Implementation: [`tools/pandemic_hash.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/pandemic_hash.py).

To reverse-map hashes → names would require the game's internal registry tables or brute-force hash collision with a comprehensive wordlist.

## Verification method

Confirmed by decompressing the `resident_P000_Q3.block` (demo WAD, block index 1250):
1. Block header contains `count(4) + count × entry(16)` with `(name_hash, type_hash, field_c, chunk_size)` per entry
2. All 2,331 `name_hash` values from the block appear in ASET `u32_0`
3. All corresponding ASET rows have `u32_2 >> 16 = 1250` (correct block index)
4. `type_hash` in block headers maps 1:1 to ASET `u32_3` type discriminator
