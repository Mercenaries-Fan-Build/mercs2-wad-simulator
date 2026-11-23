# fxdict and effect binary formats

**Date:** 2026-05-30; the fxdict record and the `vfx` atlas 2026-10-05  
**Status:** Decoded from retail PC `vz.wad` (resident + effects blocks) and the loader  
**Tools:** [`tools/fxdict_parser.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/fxdict_parser.py), [`tools/effect_block_probe.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/effect_block_probe.py), [`tools/fxdict_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/fxdict_codec.py)

---

## 1. Asset locations (ASET-verified)

| Asset | type_id | type_hash | ASET count | Block file | Block entry table |
|-------|---------|-----------|------------|------------|-------------------|
| **fxdict** | 0 | `0xFA46D8A8` | 1 | `blocks\VZ\resident_P000_Q3.block` | 1× fxdict in resident |
| **effect** | 29 | `0x5608BD5A` | 314 | `blocks\VZ\effects_P000_Q3.block` | 314× effect + 46× mesh (`0x5B724250`) |

**Correction:** Earlier notes placed fxdict inside the effects block file. ASET points to the **resident** singleton. The effects block holds all particle definitions; they share the global dictionary loaded from resident.

- fxdict asset hash = `pandemic_hash_m2("fx")` = **`0x86BF6C5B`**
- Retail probe: resident entry index **5889**, body **12 672** bytes (2026-05-30)
- ASET type id **0**: the WAD's type table puts `0xFA46D8A8` at 0, and the fxdict's ASET row carries
  0. PROVEN by `mercs2_formats/tests/type_ids_match_the_wad.rs`.
- The resident block also carries a second asset under `0x86BF6C5B`, of type `0x42498680` (entry
  816); it is not the fxdict.

Extraction (single-block policy):

```bash
.venv/Scripts/python.exe tools/extract_single_block.py \
  --wad game-files/pc-game-vz.wad \
  --path "blocks\VZ\resident_P000_Q3" \
  --keep --scratch-root output/_scratch/fx_probe

.venv/Scripts/python.exe tools/extract_single_block.py \
  --wad game-files/pc-game-vz.wad \
  --path "blocks\VZ\effects_P000_Q3" \
  --keep --scratch-root output/_scratch/fx_probe
```

---

## 2. Block file envelope

Same as other VZ blocks ([`docs/format_reference.md`](format_reference.md)):

```
u32 entry_count
entry_count × 16 bytes: asset_hash, type_hash, reserved, payload_size
payloads packed sequentially (each payload is usually UCFX + CSUM in archive;
decompressed .block.bin strips CSUM wrapper per entry)
```

---

## 3. fxdict UCFX container

### 3.1 Chunk table

Standard UCFX header (`dao` + chunk count) with **two** leaf chunks on retail:

| Chunk | rel_off | Typical size | Role |
|-------|---------|--------------|------|
| **INFO** | 0 | 4 | `u32 entry_count` |
| **DICT** | 4 | `entry_count × 20` | Parameter records |

Verified retail: **630** entries, **12 600** DICT bytes, zero trailing slack.

### 3.2 DICT record (20 bytes): a sprite rectangle

Each record is the rectangle of one sprite frame in the `vfx` atlas (`0x89E211AF`, §3.4), in
atlas-normalised units. **PROVEN** by the loader and by the retail atlas
(`mercs2_formats/tests/fxdict_atlas_retail.rs`).

| Offset | Type | Field | Meaning |
|--------|------|-------|---------|
| +0x00 | `u32` | `key` | The frame key: the hash an effect's `TEXT` names |
| +0x04 | `f32` | `u` | Left edge |
| +0x08 | `f32` | `v` | Bottom edge, measured **up from the atlas's bottom** |
| +0x0C | `f32` | `w` | Width |
| +0x10 | `f32` | `h` | Height |

- **The loader** `FUN_00491320` reads `INFO` (`u32 count`, allocating `count × 0x20`) and then each
  20-byte `DICT` record, and stores a 32-byte runtime record: `+0x00 key`, `+0x10 u`,
  `+0x14 1 − v − h` (the top edge, measured down from the atlas's top; the `1.0` is
  `DAT_00B9B664`), `+0x18 w`, `+0x1C h`.
- **The lookup** `FUN_00491510` is a binary search over the keys with a **signed `i32`** compare,
  and returns the record's `+0x10`. The records are therefore sorted by `key as i32`, and a key
  appears once: the 630 retail records are, and no key repeats.
- **A key the search misses** gets a static record `(0, 0, 1, 1)`: the whole atlas.
- `FUN_004911a0` converts the four values to binary16 for the effect's stream table
  (`FUN_004A4530`). Every retail field is a whole number of atlas pixels, to the 0.001 pixel its six
  written decimals allow, and binary16 holds every multiple of 1/2048 in [0, 1] exactly.
- **The retail records** are whole-pixel, inside the 2048² atlas, and no two overlap. Read with `v`
  from the bottom, they cover the atlas's alpha: one record (`0xDC5C5324`, 64² at (832, 512)) lies
  over texels of alpha 0, and eight texels outside every record carry alpha 1. Read with `v` from
  the top, 46 records cover no alpha and 219,200 texels with alpha fall outside every record.

### 3.3 JSON output

[`tools/fxdict_parser.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/fxdict_parser.py) writes:

- `entry_count`, full `parameters[]` with hashes and floats
- `top10_by_default_magnitude` with Niagara mapping **hypotheses**
- Per-row `resolved_names` from rainbow table when present

Default path: `output/_scratch/fx_probe/fxdict.json`

### 3.4 The `vfx` atlas

| Field | Value |
|---|---|
| Asset | `vfx`, `pandemic_hash_m2("vfx")` = `0x89E211AF`, texture type `0xF011157A` |
| Block | `blocks\VZ\resident_P000_Q3.block` (block 3185), entry 5062 |
| Format | 2048 × 2048 DXT5, 10 mips (to 4 × 4), body 5,592,400 bytes, fully resident |

- PgFX requests it at init (`FUN_0048A170`), with `particle_mask` `0x07B71436` and the fxdict.
- It is the only texture particles sample: the effect format has no per-effect texture field, and
  no `MTRL` in `vz.wad` names `0x89E211AF`
  ([`reverse_engineer/particle_fx_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/particle_fx_code_map.md) §4).
- **The free square.** The largest square of the atlas, aligned to its own side, that no record
  lies over and whose every mip-0 texel has alpha 0 is the 512² at (1536, 0). Its texels at mips
  0–5 and 9 have alpha 0; at mips 6, 7 and 8, four, one and four of them carry alpha 1 or 2. PROVEN
  (`fxdict_atlas_retail.rs`; `mercs2_quartermaster/tests/fx_retail.rs` for the search). `qm` draws
  `add_fx_sprite` images there ([`modding/manifest_format.md`](modding/manifest_format.md#add_fx_sprite)).

---

## 4. effect UCFX container

**Specified in [`effect_container_format.md`](effect_container_format.md)**, measured against all
314 retail effects (each re-encodes byte for byte; the computed `EFCT` equals the stored one in
314/314). The container flattening is in [`ucfx_tree_container.md`](ucfx_tree_container.md).
Summary:

- One tree rooted at `EFCT` (18 B, nine u16, computed). Children: `EMTR` (u16 = GEOM child count;
  each `GEOM` = u32 k + k × 13 f32, k triangles particles spawn on), then one (`EMIT` marker,
  `PTYP`) pair per emitter, then the `FRCE`s.
- `EMIT` → `TRFM` (64 B 4×4) with the nine channel `ATRB`s (`posx`…`sclz`), and an optional
  `GEOM` (u16 shape index, u16 count of the shape's records sampled). **The engine requires a
  sampleable shape:** a spawned effect picks each particle's spawn record as `random % count`
  (`FUN_0048ae80`, `div` at `0x0048AFF6`), so a `GEOM` names a shape with records and a count from 1
  to that shape's record count (all 811 retail `GEOM`s give exactly the record count), and an emitter
  without `GEOM` (count 0) must spawn no particle: a constant `rate` at or below 0, a `ratevar` of 0,
  and every template that starts it with `RedEffectComponent` `0x62C7746E` = 0, as the 9 retail
  emitters without `GEOM` are. An emitter that breaks this divides by zero when it spawns, observed
  live 2026-10-06 ([`effect_container_format.md`](effect_container_format.md) §2.1).
- `PTYP` (u32 flags; bits 0/1 read) → 19 `ATRB`, `COLR`, 13 `ATRB`, `TEXT`, in a fixed order.
- `ATRB` (12 B `{u32 hash, u32 flags, u32|f32 value}`) may own `ANIM` (u32 key count) → `AKEY`
  × n (8 B `{f32 time, f32 value}`). `ANIM` and `AKEY` exist; 1,880 retail curves.
- `COLR` is **800 bytes** = 100 × `{u8 blue, u8 green, u8 red, u8 alpha, binary16, u16 0}`; the
  colour order is PROVEN live, 2026-10-07 ([`effect_container_format.md`](effect_container_format.md) §6). (The 200 the loader stores is
  the stream-word reservation, not the byte size.)
- `TEXT` = u32 n + n frame keys, each an fxdict record (§3.2) (`4 + 4n` bytes in every retail TEXT).
- `FRCE` = u32 kind (`gravity`, `drag`, `wind`, `attractor`, `vortex` — all `pandemic_hash_m2`
  of the name) + 16 / 4 / 16 / 28 / 56 bytes of parameters, then the force's `ATRB`s.
- No retail effect contains `POFF`.

## 5. UE5 mapping (downstream)

| Game | UE5 target |
|------|------------|
| fxdict 630 sprite rectangles | Sub-UV rectangles of one atlas texture |
| effect 314 defs | `UNiagaraSystem` per `asset_hash` |
| TEXT frames | The fxdict rectangles of the `vfx` atlas, one texture for every effect |
| placements `particle` / `fx_` | `ANiagaraActor` via future `populate_effects.py` |

See [`audio_ue5_path.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/audio_ue5_path.md) §2.

---

## 6. Confidence summary

| Area | Confidence |
|------|------------|
| ASET block locations (resident vs effects) | **High** |
| fxdict INFO `u32` count + DICT `20×count` | **PROVEN** |
| DICT record `{key, u, v, w, h}`, `v` from the bottom, a rectangle of `vfx` | **PROVEN** (loader + retail atlas coverage) |
| Record order `key as i32`, unique; a miss draws `(0, 0, 1, 1)` | **PROVEN** (lookup `FUN_00491510`) |
| Frame key string names | hash-only |
| effect tree, sizes and EFCT rule | **High** — 314/314 byte-identical re-encode ([`effect_container_format.md`](effect_container_format.md)) |
| Emitter `GEOM` = shape index + sampled record count; count 0 or a missing `GEOM` divides by zero on spawn | **PROVEN** (disassembly of `FUN_0048ae80`, decomp `FUN_0048cc30`, 811/811 retail counts, live crash 2026-10-06) |
| Shape record = `\|A × B\|`, unit `±(A × B)`, vertex `P`, edges `A`, `B`; spawn at `P + u·A + v·B` | **PROVEN** for `P`, `A`, `B` (`FUN_00488770`) and the retail layout (13,148/13,148); float 0's reader **unknown** |
| COLR key structure (100 × 8 B) | **High** for the layout; colour stored blue, green, red, alpha **PROVEN** live 2026-10-07 for blue, green, red (`qm_fx_cyan_burst` stored `00 FF FF` drew gold, the car-hood fire stored `FF FF 00` drew cyan), alpha **INFERRED**; the binary16's role unknown |

---

## 7. Encoder (Rust, native)

[`mercs2_formats::fxdict`](../crates/mercs2_formats/src/fxdict.rs) reads and
writes both containers.

**fxdict:** `parse_fxdict` / `parse_fxdict_container` and `write_fxrect` / `write_fxdict_dict` /
`write_fxdict_info` / `write_fxdict_container`, over `FxRect { key, u, v, w, h }`; `sort_fxdict`
orders records by `key as i32` and refuses a repeated key. The container is two top-level leaves,
`INFO` (`x2 = 1`) then `DICT` (`x2 = 0`). The retail container re-encodes byte for byte
(`tests/effect_retail_roundtrip.rs`, `tests/fxdict_atlas_retail.rs`).

**effect:** `parse_effect_container(&[u8]) -> Result<EffectContainer, String>` and
`write_effect_container(&EffectContainer) -> Result<Vec<u8>, String>`, over a typed model:

| Type | Holds |
|---|---|
| `EffectContainer` | `shapes` (EMTR GEOMs), `emitters`, `forces` |
| `Emitter` | TRFM matrix, 9 channel `Atrb`s, optional `EmitterGeom`, `ParticleType` |
| `ParticleType` | PTYP flags, 32 `Atrb`s, `Colr`, `Text` |
| `Atrb` | `hash`, `flags`, `value` (`AtrbValue::F32`/`U32`), `curve` (`Option<Vec<AnimKey>>`) |
| `Colr` | 100 `ColrKey { rgba: [u8; 4], half_bits: u16 }`: red, green, blue, alpha, which `Colr::to_bytes` / `Colr::from_bytes` write and read blue, green, red, alpha |
| `Force` | `ForceKind` (typed per kind) + its `Atrb`s |

- `EFCT` is computed (`EffectContainer::efct_words`), never stored in the model.
- `ATRB` flag bits 0 (f32 value) and 10 (has curve) are derived; an authored flag word that
  disagrees is an error.
- Attribute runs are checked against the position tables (`TRFM_CHANNELS`,
  `PTYP_ATTRIBUTES_BEFORE_COLR` / `_AFTER_COLR`, `FRCE_*_ATTRIBUTES`), which also carry the
  recovered names.
- The container is flattened by `ucfx::write_ucfx_tree` (computed `x2`/`x3`, marker rows,
  contiguous bodies, `CSUM`) and read by the strict `ucfx::parse_ucfx_tree`.

Tests: `cargo test -p mercs2_formats fxdict ucfx` (hermetic: every FRCE kind's size, COLR 800 B,
CSUM, flag rejection, an authored one-emitter `magenta_burst` effect that re-parses and satisfies
the tree rules) and `tests/effect_retail_roundtrip.rs` (game-gated: 314/314 byte-identical,
computed `EFCT` 314/314, fxdict container byte-identical, C4 effect name resolution).

The previous encoder wrote a 16-byte `EFCT`, `EMTR` as count + refs, a float `EMIT` body, a `POFF`
chunk, a 1-byte `PTYP`, a 200-byte `COLR` and FRCE params clamped to four, and padded bodies to
4 bytes with `x2 = x3 = 0`; none of that is what retail contains.

## 8. Related docs

- [`type_hash_registry.md`](type_hash_registry.md) — type hashes
- [`format_reference.md`](format_reference.md) — UCFX chunk header layout
- [`audio_ue5_path.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/audio_ue5_path.md) §2 — Niagara conversion plan
