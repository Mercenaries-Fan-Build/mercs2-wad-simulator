# Mercenaries 2 (PC) — binary & metadata format reference

This document records what the repo’s **tools** actually parse, which fields are still **unknown**, and where deeper layout lives in code. For command-line usage see [`tools/README.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/README.md).

---

## 1. Save profile (`.profile`)

**Tool:** [`tools/savefile_parser.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/savefile_parser.py)

### 1.1 Binary header (bytes before zlib)

| Offset | Size | JSON field | Notes |
|--------|------|------------|--------|
| `0x00` | 4 | `checksum_hex` | u32, printed as `0x%08X` |
| `0x04` | 4 | `version` | u32 |
| `0x08` | 4 | `data_size_field` | u32 |
| `0x0C` | 4 | `unknown_0x0C` | **Unknown** |
| `0x10` | 4 | `unknown_0x10` | **Unknown** |
| `0x14` | 4 | `n_time_elapsed_seconds` | Interpreted as play time |
| `0x18` | 4 | `n_cash` | |
| `0x1C` | 4 | `n_fuel` | |
| `0x20` | 4 | `unknown_0x20` | **Unknown** |
| `0x24` | 4 | `unix_timestamp` | u32 |
| `0x2C` | 16 | `s_last_mission_name_ascii` | NUL-padded ASCII, trimmed |
| `0x3C`–`0x4B` | 16 | — | **Not structured** in tooling |
| `0x4C` | 4 | `flags_0x4C_hex` | u32 as hex string |
| `0x50`–`0x209` | — | — | **Not structured** (gap before costume / name) |
| `0x24A` | 1 | `character_costume_index` | Single byte |
| `0x20A` | — | `reference_name_utf16` | UTF-16 LE, NUL-terminated (max 64 chars read) |

Default **zlib** payload start: **`0x468` (1128)** — `ZLIB_OFFSET_DEFAULT`. Override with `--zlib-offset` if needed.

### 1.2 Decompressed Lua (regex harvest only)

Tables are **not** parsed as Lua; strings are matched with regex. Output keys include `mission_ids`, `vehicle_tokens`, `support_tokens`, `localization_key_like`, `sys_string_to_guid_hex`, `vz_layer_strings`, `faction_prefixes_from_missions`, each with a matching `*_count` where applicable.

---

## 2. FFCS `.wad` container

**Tools:** [`tools/ffcs_wad.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ffcs_wad.py), [`tools/mercs2_ffcs_extract.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mercs2_ffcs_extract.py)

| Field | Notes |
|--------|--------|
| Magic | `FFCS` |
| Version | u32 after magic |
| Declared chunk count | u32; **retail files may declare 7** while only **five** 12-byte chunk rows exist before the `0x48` region |
| Chunk row | 4-byte ASCII tag + **offset** u32 + **meta** u32 (12 bytes per row). **`meta` is not decoded** semantically |
| `CSUM` | CSUM chunk row `offset` is **NOT a file offset** — it is a hash/identifier (exceeds file size for demo vz.wad, shell.wad, Loading.wad). `meta` (file offset `0x2C`) is the largest entry count of any block in the WAD: vz.wad 7,018 (`VZ\resident_P000_Q3`), shell.wad 416 (`Shell\scaleform_shell_P000_Q3`), English.wad 55 (`English\vo_resident_P000_Q3`), Loading.wad 3 (`Loading\resident_P000_Q3`). The PC engine sizes its block-entry install ring from it: `FUN_00875140` (v1.0) copies the 0x48-byte header to the WAD object +0x404 and passes header+0x2C to `FUN_00872c50`, which reallocates the ring at `[0x0117662C]` (capacity +0x18, used +0x1C, 16-byte records) only when the value exceeds the current capacity — the capacity is the largest `meta` of any WAD mounted so far. `FUN_00875cc0` returns 3 (retry next tick) while a streamed block's entry count exceeds the ring's free slots, so a block with more entries than every mounted WAD's `meta` is never installed and its level never starts (a vz-patch.wad resident of 7,019 entries under vz.wad's 7,018 stalls shell→world; the same WAD with `meta` 7,019 loads). Readers of the row (v1.0 and v1.1, `FUN_00875140` / v1.1 `FUN_00875390` identical): the engine never checks the `FFCS` or `CSUM` tags; `meta` is read only by `FUN_00875140` (`0x008751B9 mov eax,[ebx+0x430]`) for the ring; the `offset` value (header+0x28, WAD object +0x42C) is returned by the WAD class's vtable getter `0x00874D20` (`mov eax,[ecx+0x42c]; ret`, vtable `0x00BEA8F8` slot +0x44), whose callers are not traced. Per-UCFX integrity uses CRC-32(init=0) trailers embedded in decompressed block data (see §4.0) |
| `INDX` | Block index; 12-byte entries: `(page_index, packed_field, flags_and_page_count)`. `page_index × 0x8000` = WAD file offset of the sges block. `flags_and_page_count & 0xFFFF` = number of 32KB pages the block spans. `packed_field`: bits 0–23 = decompressed pages (the engine allocates `pages << 15`); bits 24–27 = the block's **tier**, the `N` of its `_P00N_Q(3-N)` path (retail PC: tier 0/1/2/3 = 3,581/1,762/3,001/3,026 blocks, every one matching its path); bits 28–31 = 0 on disk, the stream status at runtime. The engine loads this table as-is (`23fe7f38/pmc` capture: identical to disk on all 11,370 rows except the status nibble) and copies the tier to the stream node, where it is the eviction class ([`reverse_engineer/render_core_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/render_core_code_map.md) §8) |
| `DATA`, `ASET`, `PTHS` | Spatial chunks; **inferred byte length** by sorting by offset; `DATA` extends to EOF |

**Xbox 360 and PS3:** the same container big-endian — magic `SCFF`, chunk tags byte-reversed
(`XDNI`, `ATAD`, …), blocks `segs`. The PS3 disc `VZ.WAD` is a plain `SCFF` file from offset 0
(`game-files/ps3-VZ.WAD`, sha256 `933527f8…` = the EU/DE disc `VZ.WAD` of
[`tools/wad_simulator/crates/mercs2_exemap/data/wads.tsv`](../crates/mercs2_exemap/data/wads.tsv)). It has no encrypted envelope: the
1 GiB sample once read as one (`analysis/cross_platform/wads/ps3/VZ.WAD`, sha256 `e8406d64…`,
first bytes `9FED8BC6`) is a slice from inside a `VZ.WAD` — its first 4 KiB equal
`ps3-VZ.WAD` at offset `0x3FFFF800`, and 522,946 of its 524,287 2-KiB sectors equal
`ps3-VZ.WAD[0x3FFFF800..]` (compared 2026-10-22).

`PTHS` yields `paths.txt` path-like strings (see `dump_paths_from_pths`). **After all path strings, a mandatory 258-byte null-terminated ASCII trailer marker** (`xa37dd45ff...d4ex`) must be present — identical across all tested WADs. Omitting it causes the engine to reject the WAD (black-screen hang). See [`docs/patch_wad_format.md`](patch_wad_format.md) §4 for full details.

**ASET (asset set / streaming graph hints):** [`docs/aset_format.md`](aset_format.md) — 16-byte rows; [`tools/aset_decoder.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/aset_decoder.py) emits `output/block_dependency_graph.json` (retail: `u32_0` hits `texture_index.json` keys for a large subset of rows). Hash algorithm confirmed as **FNV-1a with `|0x20` case suppression and `^0x2A * prime` finalization** — identical between Mercenaries 2 and The Saboteur (see [`docs/modding_deep_dive.md` §4.3.1](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/modding_deep_dive.md) and [`tools/pandemic_hash.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/pandemic_hash.py)).

**ECS `COMP` harvest:** [`docs/ecs_components.md`](ecs_components.md) — [`tools/ucfx_ecs_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_ecs_codec.py) + [`tools/ecs_metadata_extract.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ecs_metadata_extract.py) → `output/placements/ecs_components.json` and merged `ecs` keys on placement JSON when `make extract-placements` runs.

---

## 3. `sges` compressed block

**Tool:** [`tools/sges_decompress.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/sges_decompress.py)

| Offset | Field |
|--------|--------|
| `0` | Magic `sges` |
| `4` | `major`, `minor` (u16 LE) |
| `8` | `total_u` uncompressed size (u32) |
| `12` | `total_c` compressed hint (u32) |
| Header | Segment table (`minor` × 8 bytes), then **16-byte-aligned** start of payload |
| Payload | Repeated **raw deflate** (`zlib` windowBits `-15`) segments separated by **zero padding** |

Each segment decompresses independently. A **64 KB sentinel** (`0x00010000` LE) appears in the segment table for the final chunk when the last segment is exactly 64 KB uncompressed.

**Engine lineage:** The `sges` compression format is **identical** between Mercenaries 2 and The Saboteur — same header layout, chunk format, raw deflate, and 64 KB sentinel. This confirms shared Zero Engine heritage.

The **n-th** `sges` in `data.bin` corresponds to the **n-th** line in `paths.txt` (same indexing as batch scripts).

---

## 4. UCFX (decompressed blob)

**Shallow scan:** [`tools/ucfx_parser.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_parser.py) — tag occurrence counts, Havok keyword hits, ASCII `strings_sample` (capped in output), `geom_chunk_trees` preview after each `GEOM`.

**Deep mesh / texture layout:** [`tools/ucfx_mesh_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py), [`tools/mesh_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mesh_extractor.py), [`tools/texture_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/texture_extractor.py)

- **UCFX** root: magic + four u32 header fields (`u0`–`u3`); `data_base = ucfx_off + u0` for child chunks.
- **Chunk table:** 20-byte rows: 4-byte tag + four u32s (`read_chunk_header`): `{tag@0, row_off@4, size@8, w3@0xc, w4@0x10}`. `row_off = 0xFFFFFFFF` ⇒ a container marker (no body); otherwise the body is at `data_base + row_off` for `size` bytes. Tags parsed in mesh path include **GEOM**, **MESH**, **PRMG**, **STRM**, **IBUF**, **INFO**, **MTRL**, **PRMT**, **HIER**, **SWIT**, **NAME**, **BODY** (texture). **SKIN** containers are markers (`u0 = 0xFFFFFFFF`); bone indices and weights live in the paired **STRM** vertex buffer (`BLENDINDICES` / `BLENDWEIGHT` at decl offsets +16/+20) — see [`docs/skeleton_status.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/skeleton_status.md) and [`tools/export_skinned_mesh.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/export_skinned_mesh.py). Others (**CHDR**, **STAT**, **CEXE**, **enum**, **flgt**, **flgs**, **INDX**, …) may appear in **`tag_occurrences`** without a dedicated decoder.
  - **Nav fields `w3`(+0xc) / `w4`(+0x10) — worldentity/ECS containers** (decoded 2026-08 from loader `FUN_00654940`, block 3185 `0x5647C35D`; used when authoring an additive block, e.g. `mercs2_probe --bin worldentity_template_block`): **`w4` = child count** — `0` for leaf/top-level rows (CHDR/enum/UNIQ/flgs/info/schm/data), **`3` for a COMP** grouping info+schm+data, **`2` for a Name COMP** (info+data). The loader reads `chunk+0x10` (w4) as a **descend gate**: `w4 == 0` ⇒ the group is not entered, so an authored COMP row MUST carry its child count. **`w3` = a countdown**: top-level rows = `T − index` (`T` = total top-level rows; base worldentity `T = 199`, so CHDR = 199), and a group child = remaining-children-after-it (info=2, schm=1, data=0). A COMP group = `COMP · info(classname) · schm(field schema) · data([u32 N][N u32 keys][payload]…)`; `flgs = [u32 count]{u32 handle, 32-byte signature}…` and is READ by the loader (inserted into the signature registry `0xDF6D08`).
- **CONTAINER_SENTINEL** `0xFFFFFFFF` on chunk row `u0` marks nested-container boundaries in some walks.
- **Stringdb chunks** (**SYEK**, **SRTS**): localized string database. On **PC** the bodies are **little-endian** (UTF-16LE text), ★corrected 2026-07-22 — the previous "natively big-endian on all platforms" claim was measured false. Descriptor fields (offset, size) follow normal UCFX LE convention. See §4.1.

### 4.0 CSUM trailer (per-chunk integrity)

Each UCFX chunk inside a decompressed block file is followed by an 8-byte **CSUM** trailer:

| Offset | Size | Field |
|--------|------|-------|
| `+0` | 4 | ASCII tag `CSUM` |
| `+4` | 4 | u32 LE checksum value |

**Algorithm — CRC-32 (init=0, no final XOR)** (verified against **53,765 chunks across 10,099 block files**):

| Parameter | Value |
|-----------|-------|
| Polynomial | `0x04C11DB7` (normal) / `0xEDB88320` (reflected) |
| Init value | `0x00000000` |
| Final XOR | `0x00000000` |
| Reflect in | Yes |
| Reflect out | Yes |

This uses the standard CRC-32 polynomial (ISO 3309 / ITU-T V.42) but with **init=0** and **no final inversion**. Note: this is neither standard CRC-32 (init=0xFFFFFFFF, xorout=0xFFFFFFFF) nor CRC-32/JAMCRC (init=0xFFFFFFFF, xorout=0x00000000) — it is a custom variant with init=0.

**Input range:** The checksum covers the entire byte range from the start of the `UCFX` tag (inclusive) to the byte immediately before the `CSUM` tag. For multi-chunk block files, each UCFX chunk has its own trailing CSUM that covers only that chunk.

**Block file structure:** `count(4)` + `count × entry(16)` + concatenated chunks. Each header entry is `(name_hash, type_hash, field_c, chunk_size)` as u32 LE. Each chunk is `UCFX_header(8) + UCFX_body(variable) + "CSUM"(4) + checksum(4)`. The `chunk_size` field in the header entry gives the total chunk length including the CSUM trailer.

**Python implementation:**

```python
import zlib

def crc32_mercs2(data: bytes) -> int:
    """CRC-32 with init=0, no final XOR (Mercenaries 2 CSUM algorithm)."""
    # Python zlib internally does: init' = init ^ 0xFFFFFFFF, ..., result ^ 0xFFFFFFFF
    # So zlib.crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF gives table-CRC with init=0, no final XOR
    return (zlib.crc32(data, 0xFFFFFFFF) ^ 0xFFFFFFFF) & 0xFFFFFFFF
```

Equivalent explicit table implementation:

```python
def crc32_mercs2_explicit(data: bytes) -> int:
    table = [0] * 256
    for i in range(256):
        c = i
        for _ in range(8):
            c = (c >> 1) ^ 0xEDB88320 if c & 1 else c >> 1
        table[i] = c
    crc = 0
    for b in data:
        crc = table[(crc ^ b) & 0xFF] ^ (crc >> 8)
    return crc
```

### 4.1 Stringdb chunks (SYEK / SRTS)

**ASET type_id:** 7 (`type_hash = 0x39E5E978`, `pandemic_hash_m2("stringdb")`)

Localized string database blocks (e.g., `english_P000_Q3.block`, `english_dlc01_P000_Q3.block`). Contains key→value lookup for bracket-key strings like `[DlcCon001.Title]`.

> **★CORRECTED 2026-07-22 by measurement — this section previously claimed the bodies are
> "natively big-endian on all platforms" and that the SRTS header is a byte count. BOTH are wrong
> for the retail PC build.** Measured over all six language blocks in `shell.wad` and the
> `english_P000_Q3` block in `vz.wad`:
>
> - SYEK entry table and the UTF-16 text heap are both **little-endian**.
> - The SRTS header is a **u16 code-unit count, not a byte count**: `heap_bytes == 2 × header`,
>   exact in every language (english `1229064 == 2 × 614532`, german `1326564 == 2 × 663282`,
>   spanish `1326110 == 2 × 663055`, french `1313846 == 2 × 656923`, italian `1295504 == 2 × 647752`,
>   russian `1229126 == 2 × 614563`).
>
> - The chunk tags are literally **`KEYS`** and **`STRS`** in the PC containers, *not* `SYEK`/`SRTS`.
>   The reversed spellings in this document's headings are the big-endian (Xbox) byte order read as
>   ASCII. Code that looks up `SYEK`/`SRTS` on a PC WAD finds **nothing and silently skips the
>   table** — which is exactly how the first version of the round-trip checker reported "0
>   containers" against a WAD that plainly has six.
>
> [`tools/build_shell_string_patch.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/build_shell_string_patch.py)'s UTF-16**LE** assumption was the correct one all along.
> Reproduce: `cargo run -p mercs2_probe --bin stringdb_dump -- --wad <wad>` — it detects the
> endianness per chunk rather than trusting either reading, and prints what it found.
>
> The original BE claim is retained below only where it may still hold for **Xbox** blocks, which
> were not re-measured. Do not assume it for PC.
>
> On the consoles (Xbox and PS3 DLC blocks) `STRS` is a big-endian u32 code-unit count followed by a
> UTF-16**BE** heap, `heap_bytes == 2 × count` in all three Xbox retail tables. Converting to PC swaps the
> count as a u32 and each code unit as a u16 (`be_to_le::convert::convert_strs_inplace`); left as it was,
> the PC game read the DLC table and crashed (`0x04888CFB`).

#### SYEK (keys)

| Offset | Size | Endian | Field |
|--------|------|--------|-------|
| +0 | 4 | **LE** (PC) | `key_count` — number of entries |
| +4 | 8 × N | **LE** (PC) | entries: `pandemic_hash(key_string)` (u32) + `byte_offset_into_SRTS` (u32) |

Key strings use the standard `pandemic_hash_m2()` algorithm (FNV-1a + `|0x20` + `^0x2A * prime`). Offsets point into the SRTS string data area (after the SRTS 4-byte size header).

#### SRTS (strings)

| Offset | Size | Endian | Field |
|--------|------|--------|-------|
| +0 | 4 | **LE** (PC) | `total_code_units` — **u16 count, not bytes**. Heap byte length = `2 ×` this |
| +4 | variable | **LE** (PC) | Concatenated NUL-terminated UTF-16**LE** strings |

Each string is NUL-terminated (u16 `0x0000`). SYEK entry offsets are **byte** offsets (not code-unit
offsets) from the start of the string data area — i.e. from SRTS body + 4. Verified: decoding at those
offsets as UTF-16LE yields well-formed text for all 18,299 English keys.

#### Loading via Lua

```lua
Sys.AddStringDb("patch01", "dlc01")
```

Resolves asset name `<current_language>_<prefix>` → `english_dlc01`, hashes to `pandemic_hash_m2("english_dlc01") = 0x9FE1E7D8`, loads via ASET lookup. The first argument `"patch01"` is an internal DB namespace identifier.

#### Lookup semantics (runtime) — ★NEW 2026-07-25, read from the decomp

Everything above describes the bytes on disk. This describes what the engine does with them, which is
what actually decides whether a modded string appears.

**Source:** `output/_ghidra/mercs2_unpacked.exe_decomp.txt` throughout.
⚠ Do **not** mix addresses with `genuine_patched_unpacked.exe_decomp.txt` — its data segment is
rebased ~`+0x1000` (that dump's clear-all is `FUN_00464640` over `DAT_011749b8` / `DAT_00e89e58`;
the same logic here is `FUN_004645f0` over `DAT_011759b8` / `DAT_00e8ae58`). Function VAs match;
`DAT_` addresses do not.

##### The registry

| global | meaning |
|---|---|
| `DAT_011759b8` | count of registered DBs — **hard maximum 8** |
| `DAT_00e8ae18[i]` | `pandemic_hash_m2(prefix)` — the dedup key |
| `DAT_00e8ae38[i]` | the prefix string itself (heap copy) |
| `DAT_00e8ae58[i]` | `pandemic_hash_m2("<language>_<prefix>")` — the loaded asset |

- **`AddStringDb` = `FUN_00464540`** (tail of the Lua binding at `0x005E61D0`). Rejects a NULL prefix
  or a full table (`DAT_011759b8 < 8`), **dedups by prefix hash** (re-adding the same prefix is a
  silent no-op, not a reload), copies the prefix, then calls `FUN_004644b0`, which `_snprintf`s
  `"%s_%s"` from the current language (`PTR_s_english_00cf281c[*DAT_01176018]`) and requests the load.
- **`ClearStringDb` = `FUN_004645f0`** (`0x005E61E0`) — releases every slot and resets the count to 0.
- **Parser = `FUN_00463f10`**, walking `INFO` / `STRS` / `KEYS`. `INFO` carries a version that **must
  equal 5**; anything else stores the error string `Wrong SDB version` and abandons the load. Keys are
  inserted into an open-addressed hash table with linear probing (`(slot + 1) % size`).

##### Resolution order — a THIRD rule, distinct from the other two

[`docs/modding/field_guide.md`](modding/field_guide.md) documents two resolution rules running in opposite directions (WAD stack
= last-mounted wins; runtime chunk registry = first-writer wins). String lookup is a **third**:

**`FUN_0046423e`** (reached via `FUN_00464230` → `FUN_004dd6f1`) takes a **key hash** and:

1. Walks the registered DBs by **descending slot index** — i.e. **last-registered wins**. The first
   slot with a hit returns immediately.
2. Falls back to the **base language DB** (`pandemic_hash_m2(<language>)`, no prefix).
3. **Returns 0 (NULL) on a total miss.** There is no literal-passthrough at this layer.

So a later `AddStringDb` shadows an earlier one for any key they share — the opposite direction from
the chunk registry, and worth stating out loud because the two are easy to transpose.

##### Keys are hashed WITHOUT brackets

This is the non-obvious one. Retail content refers to strings as `[SHELL.Misc.41]`, so it is natural
to assume the brackets are part of the key. They are not.

**Evidence** — the loading-tip picker `FUN_00609600` counts how many tips exist by generating keys and
probing until one misses:

```c
sprintf(&local_220, s_<Loading_Set1__03d_00bbb9d3 + 1, *(undefined4 *)(in_EAX + 0x3c));
uVar6 = FUN_00824270();          /* pandemic_hash_m2 of that string */
iVar7 = FUN_00464230(uVar6);
if (iVar7 == 0) break;           /* miss == end of the set */
```

The `+ 1` skips the leading `<` of the Ghidra-named literal, leaving the format `Loading_Set1_%03d` —
**no brackets** — and it resolves. `FUN_00824270` is the `pandemic_hash_m2` helper.

⇒ The brackets are stripped (or merely tested) by a **caller**, above the lookup. A bracket-free
string such as `Sean Devlin` is therefore a perfectly well-formed key that simply *misses*.

> **This corrects [`exe_analysis_agent_a.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/exe_analysis_agent_a.md) §"String resolution likely uses
> the `[contract` prefix pattern".** That guess is not wrong about the engine, but it is about a
> different function: no bracket ever reaches `FUN_0046423e`.

##### Open — what a caller does with NULL

**Unresolved:** whether a UI text setter handed a NULL (i.e. an unlocalized literal like a modded
`PlayerVisibleName`) draws the original string or draws nothing. That decision lives in the
Scaleform/menu path *above* `FUN_0046423e` and has not been read. `FUN_005cb9b0` and `FUN_00701b20`
both test for `'['` but are unrelated parsers — checked, dead ends.

**Consequences for modding.** Until that is settled, treat a raw (non-token) display string as
unproven. And note the **8-DB cap** is a shared, exhaustible resource: if several mods each register
their own string DB, the ninth silently gets nothing — with no error, because `AddStringDb` simply
returns.

### 4.2 INDX chunk (sub-object → **seg_id**) — ★CORRECTED 2026-07-12

> **⚠ This section previously said "MESH group → HIER node index". That is wrong on BOTH counts.**
> `INDX` is keyed by **sub-object ordinal**, not by drawing group, and its value is a **`seg_id` — an
> index into `SEGM`** — not a HIER node index. The node comes from `SEGM[seg_id].bone`.
> Authoritative: [`docs/modernization/vehicle_model_spec.md`](modernization/vehicle_model_spec.md) §2.

```
PRMG drawing group → its parent MESH/SKIN sub-object under GEOM  → ordinal k
                     seg_id = INDX[k]
                     SEGM[seg_id] = { bone: u16, seg_id: u8, state_mask: u8 }
                                      ^ HIER node        ^ LOD tier bitmask
```

`INDX.len()` equals the **`MESH` + `SKIN` sub-object count**, never the PRMG-group count (which is
larger — one sub-object can own several groups). Verified on every container: Mattias 24 = 7+17
(29 PRMG); car van 65 = 65 (77 PRMG); ztz98 P002 62 = 62 (63 PRMG).

**Tool:** [`tools/ucfx_mesh_codec.py :: parse_indx_chunk`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_mesh_codec.py) *(still carries
the old interpretation — see the Rust `model_cubeize::read_model_meshes_segm` for the correct one).*

Located inside the **GEOM** container as a direct child chunk.

| Field | Type | Description |
|-------|------|-------------|
| Header tag | 4 bytes | ASCII `INDX` |
| `u0` (offset) | u32 | Byte offset relative to `data_base` |
| `u1` (length) | u32 | Byte length of the payload (= N × 2) |
| `u2` (count) | u32 | Number of entries (= N = number of MESH groups in this GEOM) |
| `u3` | u32 | Reserved (0) |

**Payload:** N × u16 little-endian values, N = the sub-object count. Entry `i` gives the **`seg_id`**
(SEGM row) for sub-object `i`.

**★The old example below misread seg_ids as node indices.** The values `[0,1,2,3,4,5,6,51,64]` are
seg_ids: the low run happens to look like node numbers, and the outliers (51, 64) gave it away. The
retail `ch_veh_tank_ztz98` reads `INDX = [0,1,2,3,4,5,6,17,20,23,73,80]` — same shape. Note also that
each LOD rung's INDX claims a **disjoint subset** of the shared SEGM table (P000 → 0–6…, P001 → 9–16…,
P002 → 7,8,18,21,24…), which only makes sense if these are SEGM rows, not nodes.

<details><summary>Original (incorrect) example, kept for reference</summary>

```
INDX values: [0, 1, 2, 3, 4, 5, 6, 51, 64]
  MESH group 0 → HIER node  0  (hull, at origin)
  MESH group 1 → HIER node  1  (tread L, at origin)
  MESH group 2 → HIER node  2  (tread R, at origin)
  MESH group 3 → HIER node  3  (antenna, at 0.49, 2.55, 0.47)
  MESH group 4 → HIER node  4  (hatch, at 0, 1.89, 4.29)
  MESH group 5 → HIER node  5  (barrel, at 0, 1.98, 1.30)
  MESH group 6 → HIER node  6  (destroyed hull variant, at origin)
  MESH group 7 → HIER node 51  (turret base, at 0, 1.63, 0.23)
  MESH group 8 → HIER node 64  (headlight L, at -0.77, 1.51, 3.60)
```
</details>

The mapping above is coincidentally *plausible* — which is what made it survive. It reads as node
indices only because a mesh's seg_id and its bone often correlate for the first few sub-objects. Route
through SEGM and the outliers land somewhere else entirely.

When INDX is present, `decode_submesh` uses it to place the mesh (via `SEGM[INDX[k]].bone` — the Python
path still shortcuts straight to a node and inherits the old error). Analysis shows that **every block with a HIER chunk also has an INDX chunk** (1,147 out of 3,581 P000_Q3 blocks have both; the remaining 2,434 have neither HIER nor INDX). The bbox fallback path exists for safety but is not exercised by any known block in the archive.

> **Note:** a block can carry `INDX` **without** `HIER`/`SEGM` — that is exactly what a streamed LOD
> rung (`_P001_`/`_P002_`) is. Its INDX rows index the **resident** block's SEGM table. See
> [`vehicle_model_spec.md`](modernization/vehicle_model_spec.md) §1.

INDX + HIER are found in multi-part assets: vehicles, characters, buildings with sub-components, and complex props where multiple MESH groups must be positioned relative to a hierarchy. Single-part meshes (simple static props, vegetation, terrain tiles) have no HIER and produce one mesh group at origin.

---

### 4.3 UCFX texture INFO and rungs

**Tools:** `mercs2_formats::be_to_le::texture`, `mercs2_probe tex-rungs`, `mercs2_probe tex-compare`.

A texture lives in up to four blocks. Rung `_P000` holds `NAME`, `INFO` and `BODY`; each finer rung
`_P001`..`_P003` holds a lone `BODY`. The ASET row names the blocks (`aset_format.md`).

PC `INFO` is 34 bytes, read by `Tex_ConsumeChunk FUN_00750a30` (object field in brackets, the object
being resource `+0x1c`):

| Offset | Field |
|---|---|
| `+0`, `+2` | width, height, u16 (`+4`, `+6`) |
| `+4` | u16, stored at `+8`, 1 on every texture read |
| `+6` | mip count, u16 (`+0xa`) |
| `+8` | flags, u16 (`+0x28`). The only PC reader found is bit 0, in the sampler setter `FUN_00751030` |
| `+10` | kind, low byte only (`+0x2a`): 1 plain, 2 cube. Any other value drops `BODY` |
| `+12` | u16, skipped by the reader |
| `+14` | D3DFORMAT or FourCC, u32 (`+0x24`) |
| `+18` | f32 (`+0x20`), meaning not read |
| `+22` | linear size of the whole chain, u32, skipped by the reader |
| `+26`, `+28`, `+30`, `+32` | mip masks of `_P003`, `_P002`, `_P001`, `_P000`, u16 (`+0x2c`..`+0x32`) |

**A rung's `BODY` is exactly its masked mips, largest first.** 36,722 of 36,723 PC bodies equal the sum of
their masked linear mip sizes; the other is the cube, whose `_P000` mask of `0xFFFF` means every mip of
all six faces, stored face by face. PC's `_P000` mask is never 0.

The rung comes from the ASET slot, not the path or a mip count: the slot travels through the stream
node (`node+0x5a`) into `FUN_004b1000` and `FUN_00750a30`, which indexes the masks with it. Nothing
compares `BODY`'s size to its mask: the reader is a bare `memcpy`. So on PC:

- A rung with a non-zero mask and a size-0 `BODY` faults (reader is NULL, `0x750bd9`).
- A rung with a zero mask whose block holds this hash corrupts the texture: `first` stays -1, so the
  surface is 1×1 with `mips + 1` levels, and `FUN_0085cbd0` discards `CreateTexture`'s HRESULT.
- A mask with a gap lands its mips consecutively, so they shift silently.
- A mask with no block never arrives: the texture stays coarser.

None of `INFO` or `BODY` reaches stream-node state: `FUN_004b1000` ignores the reader's result. What can
stall is the rung order. Rungs install coarsest first: the ring consumer `0x4b1160` takes an item only
when `((res+0x16 >> 6) & 7) - 1 == node+0x5a`, and otherwise re-queues it and loops, so a rung that
arrives before the one above it spins. Every block a chain names must therefore hold the asset exactly
once, which retail does on 35,582 of 35,582 rungs and `aset_refcheck` enforces.

**Xbox** `INFO` is the same fields big-endian, 34 or 42 bytes, with the D3DFORMAT word at `+14` (low byte
`0x52` DXT1, `0x53` DXT3, `0x54` DXT5, `0x86` A8R8G8B8) and the tiled size at `+22`. Bodies are tiled:

- Each mip takes `(w, h)` in blocks rounded up to 32, in a 32-block macrotile.
- Mips under 32 texels on a side share one surface, the tail, sized from the first of them. In blocks,
  the n-th tail mip sits `(16 >> n) / 4` along the short axis, along X when the texture is not wider.
- A texture of one mip sits at the origin of its surface.
- A cube stores every face of a mip before the next mip, then the faces' tails.
- Words are big-endian: 16-bit in a DXT block, 32-bit in A8R8G8B8.

Proven against PC twins over 88,448 mips: all 5,218 resident tail mips of unflagged DXT5 pairs are
byte-identical, 3 of their 4,753 full mips differ in alpha, and DXT1 pairs differ by codec noise (median 3
of 255). PC clears the colour under fully transparent texels, and PC and each console compressed their
own blocks, so most bodies differ in bytes. `mercs2_probe tex-compare` lists every mip.

A 42-byte `INFO` is a map flagged `0x10` or `0x20` (all 5,808): the runtime has a flat stand-in for it
(`FUN_822047f0`: `(0.5, 0.5, 1)` for `0x10`, black for `0x20`), so the bake empties `_P000`, folds its
mips into `_P001` and sets the first mask table to match. The second table at `+34` keeps the split:
`_P003` and `_P002` are equal in both tables and first `_P001` = second `_P001 | _P000` on all 5,808.
Nothing in the Xbox reader touches the second table. PC cannot read this form, for the reasons above,
so conversion moves the folded mips back to `_P000`. 90 of them have no `_P001` and so no pixels at all
on Xbox; PC retail has all 90, and the 7 in the DLC are all in the base game.

Xbox flag `0x10` on DXT1 is a normal map with X and Y in red and green. All 2,770 are DXT5 on PC, with Y
in the colour block and X in alpha. Bit `0x10` is clear on 2,872 of 2,901 PC twins and PC has no reader of it.

**PS3** `INFO` is 25 bytes, big-endian, with no `NAME` row: width, height, depth and mips as PC, `+8` kind
(2 plain, 4 cube), `+10` format (`0x86` DXT1, `0x88` DXT5, `0x85` A8R8G8B8), `+12` a flags byte, `+13` the
f32, masks at `+17`. The flags byte is PC's `+8` less bit `0x20` on all 13,253 retail twins, and PC reads
only bit 0. Bodies are PC's split, linear, in PC's byte order. A DXT1 with `0x10` set is a normal map; PC
has it in DXT5 with `0x10` kept.
Bodies are linear, in the same byte order as PC, and the masks equal PC's on all 13,253 shared textures.

### 4.4 Texture streaming index

**Tool:** [`tools/texture_streaming_index.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/texture_streaming_index.py)

Per-entry: `asset_hash`, `type_hash`, reserved, `size`. Build filters **texture** type hash; used for cross-block mip assembly (`texture_index.json`).

---

## 5. Havok slices (blob carve-out)

**Tool:** [`tools/havok_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/havok_extractor.py)

- Searches for `hkxp`, `<hkpackfile`, `Havok`.
- Writes **raw binary slices** (default max 256 KiB each) and optional **heuristic** `convex_hull_*.obj` from longest run of plausible `f32` triplets after `hkpConvexVerticesShape`.
- **No** Havok class table, version string, or rigid-body graph parsing.

`havok/manifest.json` (stage 2): `havok_slices` list with `offset`, `size_written`, `preview` (short ASCII snippet).

---

## 6. CERP precache

**Tool:** [`tools/cerp_precache.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/cerp_precache.py)

| Offset | Field |
|--------|--------|
| `0` | Magic `CERP` |
| `4` | Version u32 |

Remainder of file is **not** structurally decoded in tooling; use `--json` / `--out` for a hex preview blob, or hex dump in default CLI mode.

---

## 7. Audio (PWS streams + embedded wavebanks)

### 7.1 PWS / RIFF / Ogg in stream roots

**Tool:** [`tools/pws_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/pws_extractor.py)

- **RIFF**: total size from RIFF length dword; nested **`fmt `** chunk parsed when present (**WAVE**): JSON fields `wave_audio_format`, `wave_sample_rate`, `wave_channels`, `wave_bits_per_sample`, `wave_byte_rate`, `wave_block_align`, and **`wave_duration_seconds`** when a `data` chunk exists and `wave_byte_rate > 0`.
- **OggS**: heuristic slice (up to 512 KiB); **no** full stream parse or duration.

Outputs `pws_manifest.json` per source directory and optional `pws_summary.json` for multi-input runs.

### 7.2 Embedded wavebank / soundbank (UCFX in WAD blocks)

| type_hash | Tool | Output |
|-----------|------|--------|
| `0xF753F6D0` wavebank | [`wavebank_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/wavebank_extractor.py) + [`ima_adpcm.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ima_adpcm.py) | `output/extracted/audio/wavebanks/{bank_hash}/clip_{clip_hash}.wav` |
| `0x9F8BCA10` soundbank | (structural probe in manifest) | Event → clip hashes in [`audio_ue5_manifest.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/audio_ue5_manifest.py) |
| `0xE5273C14` sounddb | — | Cue routing table: cue guid → (soundbank, soundbank cue index); the global one holds the category tree |

All three tables start with the `u32` version `0x1D` and the bank hash, and a bank's three tables sit
in one block under one name hash. A wavebank record's data offset (`+0x20`) is relative to the record's
own start, and embedded clips are **PCM16** (the format byte `0x02` is bytes per sample, not an IMA
codec id). The byte-level specification of all three — and how a cue name resolves through sounddb →
soundbank cue → group → wavebank wave — is
[`reverse_engineer/audio_code_map.md` §11](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md); the reference
reader/writer (`mercs2_audio`) re-encodes every retail table in `vz.wad` byte-identically.

UE5 index: [`audio_ue5_manifest.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/audio_ue5_manifest.py) → `ue5_import/metadata/audio_ue5_manifest.json` (`scan_decoded_wavebanks()` when WAV tree exists). Design: [`docs/pandemic_audio_system_design.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/pandemic_audio_system_design.md), plan: [`docs/audio_ue5_path.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/audio_ue5_path.md).

---

## 8. ECS manifest (INI + EXE strings)

**Tool:** [`tools/mercs2_ecs_manifest.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mercs2_ecs_manifest.py)

- **`cdbsizes.ini`** `[presize]`: component name → list of integer sizes.
- **`Mercenaries2.exe`**: `strings -n 8` + regex for API-like symbols (`Add|Set|Get|…`).

Output `output/ecs_manifest.json`: `symbols` array is **capped at 8000 entries**; see `symbol_count` for full distinct count (`symbols_max_output` in JSON).

---

## 9. Stage 2 JSON sidecars (review tree)

Typical path: `<pipeline>/extracted/review/<batch_pack>/<stem>/`

| File | Producer | Purpose (short) |
|------|----------|-------------------|
| `ucfx.json` | `ucfx_parser.py` | Tag hits, DXT/Havok strings, `geom_chunk_trees` preview |
| `mesh.meta.json` | `mesh_extractor.py` | Topology, LOD, counts, material indices, `extract` debug |
| `mesh.obj` / `mesh.gltf` | `mesh_extractor.py` | Geometry |
| `mesh_scene.gltf` (+ `.bin`) | `gltf_exporter.py` (when enabled) | Scene glTF |
| `submeshes/index.json` | `mesh_extractor.py --per-submesh-obj` | Per-part bbox, transforms, counts |
| `shared_textures.json` | `mesh_extractor.py` | Cross-block texture refs when `--texture-index` set |
| `textures/manifest.json` | `texture_extractor.py` | Texture list / paths |
| `textures/texture_manifest.json` | `texture_extractor.py` | Richer manifest (source blob, PNG, entries) |
| `havok/manifest.json` | `havok_extractor.py` | Slice list + optional convex heuristic |
| `dialog_fragments.json` | `dialog_extractor.py` | Bracket / Generic / UTF-16 key harvest |
| `level_hints.json` | `level_extractor.py` (optional) | `matrix_candidates`, optional `precache_files` |

Pipeline (optional): **`<pipeline-root>/animations_work/<slug>/mesh_skin.json`** — bone-name harvest + empty skin placeholder from [`tools/hk_skeleton.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_skeleton.py) during [`tools/mercs2_anim_pipeline.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mercs2_anim_pipeline.py).

Global: **`texture_index.json`** (repo-relative default under `extracted/`) from `texture_streaming_index.py` — drives `--texture-index` in stage 2 when **`TEXTURE_INDEX`** is set.

---

## 10. Related docs

- [`tools/README.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/README.md) — commands, pipeline, artifact index, env vars.
- [`docs/gameplay_data_ue5_mapping.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/gameplay_data_ue5_mapping.md) — non-mesh data → UE5 systems inventory.
- [`docs/skeleton_status.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/skeleton_status.md), [`docs/watermap_format.md`](watermap_format.md), [`docs/fxdict_format.md`](fxdict_format.md), [`docs/audio_ue5_path.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/audio_ue5_path.md), [`docs/glue_gap_closeout.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/glue_gap_closeout.md) — recent decode tracks + integration gaps.
- [`docs/shader_store_format.md`](shader_store_format.md) — PC shader stores: layout, record ids, loader limits, material → pixel-shader binding.
- [`tools/wad_simulator/README.md`](../README.md) — Rust consumption / byte-swap validation.
- [`docs/game_extractor_notes.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/game_extractor_notes.md), [`docs/quickbms_notes.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/quickbms_notes.md) — external tooling workflows.
- [`docs/modding_deep_dive.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/modding_deep_dive.md) — DRM analysis, hash system identification, Lua bytecode format, `vz.bin` decode, and modding feasibility roadmap.
- `UnrealEngineGame/README.md` — Maracaibo demo list (`maracaibo_asset_list.json`).

---

## 11. Anim group block + Havok 5.5 packfile (skeletal pipeline)

**Carving:** [`tools/animgroup_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/animgroup_extractor.py) — each ``output/extracted/batch_*/blocks/*animgroup*.block.bin`` begins with a **record table** (16 bytes per record: ``u32 checksum``, ``u32 magic`` = ``0x18166555`` = `pandemic_hash_m2("animation")`, ``u32 reserved``, ``u32 record_size``), followed by concatenated **UCFX** wrappers. Inside each wrapper the Havok slice starts at the ASCII version token ``Havok-5.5.0-r1``; the tool writes standalone ``record_NNNN.hkx`` bytes plus ``records.json``.

**Packfile:** [`tools/hk_packfile.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_packfile.py) parses the three 48-byte section headers (``__classnames__``, ``__types__``, ``__data__``), the hashed classname table, and the **four chained fixup streams** inside ``__data__`` (local → global → virtual → finish, each terminated by ``0xFFFFFFFF`` dword pairs). **Local** fixups are applied to build ``data_patched.bin``. **Global** fixups are optional (``--apply-global``) while pointer semantics are validated against external dumps (hkxcmd). ``packfile.json`` includes ``types_preview`` (hex head of ``__types__``), fixup counts, and a coarse ``data_class_hits_sample`` (u32 hits against classname hashes).

---

## 12. Havok 5.5 skeletal animation compression (Mercenaries 2)

| Class | Module | Status in repo |
|--------|--------|----------------|
| ``hkaInterleavedUncompressedAnimation`` | [`tools/hk_anim/interleaved.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/interleaved.py) | **Implemented:** ``numTransformTracks`` × ``numFrames`` × 40-byte ``hkQsTransform`` + optional float tracks. |
| ``hkaDeltaCompressedSkeletalAnimation`` | [`tools/hk_anim/delta.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/delta.py) | **Header/meta only:** reads duration, transform track count, frame count, quantization format byte, and block size hint from the HK 5.5 struct. Emits **identity TRS** samples over the declared duration so downstream glTF/UE5 plumbing stays testable. Full ``staticMask`` + ``quantizedData`` bitstream walk is **not implemented** (see HKLib ``HKAnimationData.Delta``). |
| ``hkaWaveletSkeletalAnimation`` | [`tools/hk_anim/wavelet.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/wavelet.py) | **Fully implemented:** locates wavelet struct via ``type=3`` scan, parses HK550 32-bit header (96-byte struct), reads ``StaticMask`` per track (identity/static/dynamic classification + per-axis sub-track bits), reads ``QuantizationFormat`` (offset/scale/bitWidth arrays), builds static DOF rest-pose values, maps dynamic DOFs → ``(track, component)``, decodes per-block quantized wavelet coefficients (preserved f32 values + packed bitstream via ``dequantize_bitstream``), applies inverse Haar wavelet (lifting) transform, and assembles per-frame TRS. Also provides ``peek_wavelet_header()`` for read-only header introspection. Debug hook stub at [`tools/hk_anim/_wavelet_debug.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/_wavelet_debug.py). |

**Shared internals:** [`tools/hk_anim/_decompress_common.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_anim/_decompress_common.py) — ``StaticMask``, ``MaskBit``, ``TrackType`` enums, ``read_static_masks()``, ``dequantize_bitstream()``, ``dequantize_values()``, ``fix_quat_w_sentinel()``. Used by both wavelet and (future) delta decoders.

**Emit / import:** [`tools/mercs2_anim_pipeline.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mercs2_anim_pipeline.py) → [`tools/anim_gltf_export.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/anim_gltf_export.py) (``pygltflib`` skeletal **.glb**). UE5 bundle copies into ``ue5_import/animations/`` and lists clips in ``metadata/manifest.json`` ([``tools/ue5_export.py``](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ue5_export.py)).

---

## 13. low_res_terrain block + lrterrain tile lookup

**Tool:** [`tools/terrain_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py)

`03121_blocks__VZ__low_res_terrain_P000_Q3.block.bin` (≈7 MB) packs the
20×20 Maracaibo low-resolution terrain grid as **401 UCFX containers**
indexed by a leading 16-byte-per-entry TOC:

| Entry | Field | Notes |
|-------|-------|-------|
| `[0]` | `count` u32 | Total TOC entry count (= 401) |
| `[1..400]` | `(size, hash1, hash2, 0)` u32×4 | Per-tile UCFX byte-size (`size`), per-tile **mesh asset hash** (`hash1`), constant `hash2 = 0x1602815c` |

The 401 UCFX containers are laid out back-to-back after the TOC:
- 400 are terrain tile meshes (each `INFO + MTRL + GEOM(BNDS+PRMT+STRM+IBUF)`,
  no `PRMG`); their TOC byte-deltas match the inter-UCFX spacing exactly.
- 1 is a special 3-chunk container (`NAME="vz_lrterrain" + INFO + BODY`)
  carrying the **shared DXT1 terrain texture** (`vz_lrterrain.dds`), not a
  tile lookup table.

**Tile → (row, col):** the TOC has no positional metadata. The mapping is
in `layers_static` sub-block 13 via the `LowResTerrainObject` COMP — see
[`docs/placement_data_format.md` §2.9](placement_data_format.md). 400 records
of `(entity_key, mesh_hash, scene_object_id)` in row-major order; the
record's **list index** is the (row, col) and `mesh_hash` matches the TOC's
per-tile `hash1`. [`tools/terrain_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) uses this mapping
(`grid_source = "metadata_lookup"` in `mesh.meta.json`) and falls back to
seam-matching only if the `layers_static` blob is unavailable.

**Vertex format per tile:** STRM `data` is a flat vertex buffer (no PRMG row);
stride is `vb_len // n_verts` (typically 16 B = pos f16×3 + UV f16×2 + 2 B
pad). Positions are decoded as **f16 vec3** in tile-local space (≈±200 m on
X/Z, ymin/ymax per the tile's BNDS chunk). Each tile's BNDS is 40 B holding
`bbox_center.xyz, radius, bbox_min.xyz, bbox_max.xyz`. After local decode,
vertices are offset to the world placement of the matching `lrterrain_rXX_cYY`
(centers at `(-3800 + col*400, 0, -3800 + row*400)`).

### 13.1 TOC entry 0 is a real tile slot, entry 224 is a dummy

Two structural quirks of the retail PC `low_res_terrain` TOC that matter for
correct tile routing:

1. **TOC entry 0 is dual-purpose.** Its first u32 stores the count (`= 401`)
   but its second u32 is a valid mesh `hash1` for a real tile (cell
   `(17, 9)` in the retail build). Earlier extractor code that iterated
   `range(1, n_entries)` skipped entry 0's hash and routed `(17, 9)` to the
   unused TOC entry's mesh via a "unique-unused-index" fallback.
2. **TOC entry 224 is a dummy.** Its `u2` is `0xf011157a` (every other tile
   entry has `0x1602815c`) and its `hash1` does not appear in any
   `LowResTerrainObject` record. The UCFX container at the entry's expected
   offset has `u3 = 3` (vs the normal `u3 = 12`), so
   `iter_ucfx_containers()` filters it out via its `u3 < 10` threshold. The
   400 "real" UCFX iter indices therefore correspond to TOC entries
   `{0, 1..223, 225..400}`, with index shift `−1` past the dummy.

[`tools/terrain_extractor.py::_read_low_res_terrain_toc()`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) implements the
correct mapping by walking the cumulative TOC offsets, skipping any entry
whose UCFX is filtered out, and emitting `hash1 → iter_index` for the rest.

### 13.2 Tile-local axis convention

Each tile mesh is authored centered at the origin with X/Z ∈ `[−200, 200]`.
**Row index increases with world Z** (placement
`center_z = −3800 + row × 400`). The tile-local face naming used by
`_edge_samples` is unusual: `"N"` is the `z = +TILE_HALF` face and `"S"` is
the `z = −TILE_HALF` face, i.e. the labels are **inverted relative to the
row convention**. For the seam between tiles `(r, c)` and `(r + 1, c)`, the
correct pairing is:

- Upper tile (smaller r, smaller world Z) world-south face = local
  `z = +TILE_HALF` = code label `"N"`.
- Lower tile (larger r, larger world Z) world-north face = local
  `z = −TILE_HALF` = code label `"S"`.

So adjacency pairs are `(a."E", b."W")` for column adjacency and
`(a."N", b."S")` for row adjacency. Comparing `a."S"` to `b."N"` instead
(as earlier versions of `terrain_extractor.py`, `probe_terrain_offsets.py`
and `solve_terrain_offsets.py` did) compares two non-adjacent faces 800 m
apart in world Z and produces nonsense residuals (the metric appeared as
"92 m mean seam" before the fix; the true value is ≈ 2 mm).

### 13.3 Seam continuity verdict

With both bugs above resolved, the source tile meshes are **already
seam-continuous**: mean per-edge mismatch ≈ 0.002 m, max ≈ 0.68 m, no
per-tile offset solve required. `mesh.meta.json` records this with
`transform_source: "none_source_seam_continuous"`.

Twenty cells are placeholder "open-water" tiles authored as a single quad
(`vertex_count = 4`, `triangle_count = 2`, `dy = 0`). They sit on the outer
map rim (col 0, col 19, row 19, plus a few coastal-cutout interior cells —
e.g. `(9, 15)`). Because their edges only contribute the two corner
along-coord buckets, they fail the `≥ 3 matched buckets` threshold most
solvers use and appear "disconnected" in seam graphs. They are listed in
`mesh.meta.json::ocean_tile_iter_indices`; downstream importers should
keep them at world Y = 0 (sea level) without further transform.

### 13.4 Terrain texture pipeline

The `low_res_terrain` block ships exactly **one** texture: `vz_lrterrain`,
a 2048×2048 DXT1 atlas covering the entire 8 000 × 8 000 m continent. It
is referenced by every merged tile via a planar XZ projection synthesized
at merge time (the source vertex stream carries no UVs).

#### 13.4.1 UV convention (Case 4 — generated)

Per-vertex UV layout in the source tile mesh: **none.** The 16-byte
vertex stride decomposes as `f16 pos.xyz (6) + f16 w (2) + f16 normal.xyz
(6) + f16 pad (2)`; values at byte offset 8 are unit-length normals, not
UVs. This rules out all three documented "case 1/2/3" UV conventions —
there is nothing to remap from.

[`tools/terrain_extractor.py::_world_xz_to_uv`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) synthesizes UVs from world
position (planar XZ over the 8 000 m continent):

```
u = (world_x + 4000) / 8000
v_raw = (world_z + 4000) / 8000
v = 1 - v_raw   when _TEXTURE_V_FLIP is True (retail default)
```

The `vz_lrterrain` atlas is authored D3D9-style (V=0 at image top = game
north / high Z). Raw planar projection maps south (low Z) to low V (image
bottom), so synthesis applies `v = 1 - v` via `_TEXTURE_V_FLIP = True`.
The PNG is **not** rotated at load time.

`make extract-terrain` writes `mesh_scene.glb` via
`_build_terrain_glb` (pygltflib), **not** via `gltf_exporter`. That path
embeds the synthesized UVs as-is (no second `convert_uvs_d3d_to_gltf`) and
writes game LH coordinates directly (no Z-negate, no winding flip) — the
same convention as all other meshes. UE Interchange's Y-up→Z-up swap
produces the correct world alignment with `game_to_ue` placements at the
origin actor.

Regular meshes with UVs baked in the vertex buffer use `gltf_exporter`'s
`convert_uvs_d3d_to_gltf` for the D3D9 V-flip.

#### 13.4.2 Aux texture entries in the TOC

The TOC has 401 entries:

- entry 0 stores the count in its first u32 *and* a real mesh hash in its
  second u32 (see §13.1) — it iterates as a normal mesh tile;
- entries 1..400 except entry 224 are mesh tiles (`u3 = 12`, chunks
  `INFO/GEOM/STRM/IBUF/MTRL`); 400 mesh tiles total;
- **entry 224 is the texture entry** (`u3 = 3`, chunks `INFO/NAME/DXT1`,
  payload contains the literal name `vz_lrterrain` followed by the DXT1
  pixel data). It is filtered out of the mesh iteration by
  `iter_ucfx_containers` because of the `u3 < 10` guard.

The aux-texture walk ([`tools/terrain_extractor.py::_walk_terrain_toc_aux_textures`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py))
runs a file-wide DXT/BC signature scan and a per-entry NAME-chunk probe.
For the retail PC build the result is:

```
mesh_count        = 399  (mesh-shaped entries minus entry 224 itself
                          and entry 0 which the codec does iterate)
named_entries     = 2    (both reference vz_lrterrain — entry 224 and
                          its 12 124-byte stub at TOC index 223 whose
                          payload bleeds into entry 224's NAME chunk
                          due to the way the size field is laid out)
aux_entries       = 0    (no NAME other than vz_lrterrain)
signature_scan    = {DXT1: 1, DXT3: 0, DXT5: 0, DX10: 0, BC4: 0, BC5: 0}
```

**Conclusion**: the low-LOD merged terrain has only a diffuse atlas — no
embedded normal, specular, gloss, AO, or height map. Higher-fidelity
per-cell terrain ships separately as `vz_terrainglobal_r##_c##` textures
(174 918 B each, in `c30NNN` cell blocks) and is out of scope for the
merged low-LOD bake.

#### 13.4.3 Why the GLB embeds the texture

[`tools/terrain_extractor.py::_build_terrain_glb`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) emits the master texture
into a GLB `bufferView` (MIME `image/png`) rather than as a sibling URI
(the debug OBJ path still references `submeshes/index.json` for tooling):

- UE 5.7's Interchange importer reliably picks up embedded PNGs and
  binds them to the material slot during import. URI references require
  the file to be co-located at import time and tend to break when the
  GLB is moved.
- A self-contained GLB is the only artifact the rest of the pipeline
  needs to ship to UE — there is no sibling-PNG copy step in
  `regen_maracaibo_glbs` or `populate_world.py`.
- DXT1 is transcoded to PNG before embedding because UE's importer
  treats embedded DDS as opaque binary and yields a white material.

The terrain material asks for `metallicFactor = 0`, `roughnessFactor = 1`,
`baseColorFactor = [1, 1, 1, 1]`, `doubleSided = false` — wired through
the per-entry overrides supported by [`tools/gltf_exporter.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/gltf_exporter.py) (read from
`submeshes/index.json` keys `metallic_factor` / `roughness_factor` /
`base_color_factor` / `double_sided`).

#### 13.4.4 Multi-channel material support (forward-compat)

[`tools/gltf_exporter.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/gltf_exporter.py) accepts the full glTF PBR channel set even
though lrterrain only populates `texture_diffuse`. The `index.json`
keys, in order of resolution priority, are:

| index.json key | glTF slot | Notes |
|----------------|-----------|-------|
| `texture_diffuse` | `pbr.baseColorTexture` | sRGB 4-channel |
| `texture_normal` | `material.normalTexture` | tangent-space, 2-channel reconstruction supported by UE |
| `texture_metallic_roughness` | `pbr.metallicRoughnessTexture` | glTF-spec packing: G = roughness, B = metallic, R = AO (when shared) |
| `texture_occlusion` | `material.occlusionTexture` | R-channel AO |
| `texture_emissive` | `material.emissiveTexture` | sRGB |
| `texture_specular` | `material.extras.mercs2_specularTextureIndex` | retained for legacy Mercenaries 2 specular workflow |

When a 2008-era specular map needs to be converted to the glTF MR
packing, the recommended remap is `roughness = 1.0 − specular_intensity`
written into the G channel of a freshly-generated 8-bit RGBA PNG, with
R = 255 (or AO if available) and B = 0 (terrain non-metallic). The
build then sets `texture_metallic_roughness` to the converted PNG name.
This conversion is **not** performed for lrterrain because no specular
source exists in this block; the implementation lives in the exporter
ready to consume it once a source is identified for other materials.

---

## 14. Resident singletons (`watermap`, `lineregion`)

Always-loaded `resident_P000_Q3` block. Type hashes: see [`docs/type_hash_registry.md`](type_hash_registry.md).

| type_hash | Tag / chunk | Doc |
|-----------|-------------|-----|
| `0x4D7D30C4` | UCFX → `watr` | [`docs/watermap_format.md`](watermap_format.md) — 257² grid, 5 logical layers, sea level ≈ **-36 m** |
| `0x6310807F` | UCFX → `data` | Polygon vertices (game XZ); [`tools/lineregion_probe.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/lineregion_probe.py) |
| `0xFA46D8A8` | UCFX → INFO+DICT | [`docs/fxdict_format.md`](fxdict_format.md) — global FX params (`pandemic_hash_m2("fx")`); **resident** only |

Extraction: [`tools/extract_single_block.py --path "blocks\\VZ\\resident_P000_Q3.block"`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/extract_single_block.py).

### 14.1 `lineregion` `data`

A region polygon, read by slot 4 of the lineregion vtable `0x00BAA8B0`, `0x0045E8A0` (pc-v11), into an
object whose count is at `+0x1C` and points at `+0x20`:

| Offset | Type | Field |
|---|---|---|
| +0 | u16 | kind: 2 on all 625 retail PC regions and all 31 DLC regions; the loader skips it |
| +2 | u16 | point count; the body is `8 + 8·count` bytes |
| +4 | s16 | stored at `+0x18`; 0 on all 625 PC regions; its reader was not read |
| +6 | u16 | skipped by the loader |
| +8 | `count × {f32 x, f32 z}` | points, offset by the owning entity (below) |

A console body is the same fields big-endian, so its header is four u16 swaps, not a u32 swap; a word
swap trades kind and count, and the loader reads every region as 2 points. `be_to_le` converts all 625
Xbox retail regions to their PC twins byte for byte (`mercs2_probe chunk-compare … --only lineregion`).

An entity names its region with a `LineRegion` component (descriptor `0x017BCC68`, built by
`FUN_00642260`), one u32: the region's asset hash, looked up as `(hash, 0x6310807F)`. `FUN_0045E9E0`
finds the entity whose `LineRegion` names the region, adds floats 0 and 2 of the record
`FUN_00665A60` copies for that entity (`[ESP+0x38]`, `[ESP+0x40]` after the call at `0x0045EA9F`) to
every point, and stores the bounds at `+0x34..+0x40` and the centre and radius at `+0x24..+0x30`.
`FUN_0045EC20` is the containment test: the bounds, then an even-odd crossing count over the points.
`Player.AddBoundary` reaches the region the same way (PS3 `0x00559D98` → `FUN_001CFCF8`); on PC that
call goes through a SecuROM thunk (`0x006A4870`).

---

## 15. Xbox 360 → PC mesh conversion (`decl` translation + `PHY2` Havok collision)

Two mesh chunks need **format-aware** conversion that a numeric byte-swap gets
wrong. Both are implemented twice — Python (`tools/ucfx_be_to_le.py`,
the reference) and Rust ([`tools/wad_simulator/crates/ucfx_byteswap/`](../crates/ucfx_byteswap/),
the default build backend) — and the Rust output is held **byte-for-byte
identical** to Python by parity tests (fixtures under
`crates/ucfx_byteswap/tests/fixtures/`).

### 15.1 Vertex declaration (`decl`) — a *format translation*, not a swap

The Xbox `decl` chunk is an array of **12-byte** big-endian elements; the PC
`decl` is an array of **8-byte** `D3DVERTEXELEMENT9` records. A u16/u32 swap
produces an out-of-range `Type` and the engine raises *"Unsupported data type"*
(AV). Translate element-by-element instead (`_convert_decl`):

- **Xbox element** (3 × BE u32 `a,b,c`): `a` low-16 = stream offset slot
  (`index*4 + 8`); `b` byte `(b >> 8) & 0xFF` = **format**; `c` high-byte =
  `D3DDECLUSAGE`, `c` low-byte = usage index.
- **PC element** `D3DVERTEXELEMENT9` = `{ u16 Stream, u16 Offset, u8 Type,
  u8 Method, u8 Usage, u8 UsageIndex }`.

**Format → `D3DDECLTYPE` table** (verified ~99.86% byte-exact against retail
`vz.wad` oracle pairs):

| Xbox format | PC `Type` | `D3DDECLTYPE` | Size (B) |
|-------------|-----------|----------------|----------|
| `0x23` | `15` | `FLOAT16_2` | 4 |
| `0x21` | `16` | `FLOAT16_4` | 8 |
| `0x28` | `4`  | `D3DCOLOR`  | 4 |
| `0x22` | `5`  | `UBYTE4`    | 4 |
| `0x20` | `8`  | `UBYTE4N`   | 4 |

The PC `Offset` is **cumulative by `D3DDECLTYPE` size** (4/8 above), *not* the
Xbox slot value. The stream terminates with `D3DDECL_END`: PC `Stream=0x00FF,
Type=17` → bytes `ff 00 00 00 11 00 00 00`.

**Empty decls = reskins.** A sub-mesh that reuses another mesh's vertex layout
ships an **END-only** Xbox decl (12 B `00ff0000 ffffffff 00000000`, zero
elements). This is normal — base-game Mattias has 15 of them. Emit the bare
**8-byte PC `D3DDECL_END`** (`ff00000011000000`), do **not** treat it as a
truncation. A genuine truncation (no END marker found) must still **fail loud**
(`UnhandledByteSwapError`) — END present = intentionally empty; END absent =
lost geometry. Without this, the 4 reskin DLC characters (mattias_v5, obama,
sarah, barman) are silently dropped.

### 15.2 `PHY2` collision chunk = `[u32 header][Havok packfile][trailing]`

`PHY2` is **not** a u32 array (it is listed under "verified u32-only layouts" in
some older notes — that is wrong). It is a three-part structure and a blanket
u32 sweep corrupts it in two different ways, each a distinct crash. Conversion
is `_convert_phy2_body` (Python) / `convert_phy2_be_to_le` (Rust):

1. **u32 header** (~48 B before the Havok magic): `[count][asset-hash][flags…]
   [packfile-size]` — genuine u32 fields, u32-swap.
2. **Havok 5.5 packfile** (from the 8-byte magic `57 E0 E0 57 10 C0 C0 10`,
   which is word-palindromic so it survives a swap): convert **section-aware**
   (`convert_havok_be_to_le`) — swap the header/section-headers/`__classnames__`
   *signatures* as u32, but leave the `__classnames__` **ASCII strings** and
   `__types__` raw, and convert `__data__` with per-class field widths from the
   HK550 registry ([`tools/hk_class_layouts.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/hk_class_layouts.py)).
   The registry covers the **animation** `hka*` classes; **physics** classes
   (`hkpConvexVerticesShape`, …) are unregistered, so a physics `__data__`
   degenerates to a u32 sweep + embedded-`layoutRules` repair.
3. **Trailing engine collision-wrapper** (after the packfile): a quaternion +
   **intra-chunk u32 self-offsets** + `0xAAAAAAAA`/`0xBBBBBBBB` fills. **All 110
   DLC `PHY2` chunks carry this.** It is **u32-swapped**; the packfile extent is
   bounded by `havok_packfile_size` (max section end) so it is not mistaken for
   packfile body. The exception is each vertex pool: `u16×3` vertices, named by an
   `0xEEEEEEEE` block (count at +68, body offset at +76, 0 on a convex shape, whose
   vertices are in the packfile), which swap as u16. A u32 sweep exchanges each pair.
   An odd vertex count leaves 2 bytes of padding after the pool, and they differ
   between the platforms.

PS3's `layoutRules` are `4, 0, 1, 1` where Xbox's are `4, 0, 0, 1`, and its
`hkpMoppCode` is chunked with an opcode PC does not have, so a PS3 packfile takes PC's
rules and its MOPP is recomputed ([`mopp_bytecode_format.md`](reverse_engineer/mopp_bytecode_format.md)).

**Two crashes this fixed** (both inside the exe's VM-based protection layer, so
faulting addresses are reused obfuscation gadgets):

| Crash | Cause | Fix |
|-------|-------|-----|
| `0x00414B4C` (`mov [eax],edi`, `eax` = ASCII e.g. `"rrAp"`; `lastStatus = STATUS_OBJECT_NAME_NOT_FOUND`) | Blanket u32 swap **scrambled the `__classnames__` strings** → Havok class-by-name lookup fails → derefs scrambled pointer | §15.2 step 2 (section-aware) |
| `0x0248C13E` (`add [ecx+4],edi`, `ecx` unmapped) | **Trailing left big-endian** → a self-offset `0x000004D8` reads as `0xD8040000` → engine relocator computes `base + 0xD8040000` → AV | §15.2 step 3 (u32-swap trailing) |

**Animation `data` (type `0x18166555`)** is the same Havok 5.5 packfile (magic at
+0, no header). Both converters route it through `convert_havok_be_to_le` — the
registry's `hka*` classes give correct per-field widths (u16 bone-indices, u8
compressed bitstream buffers). A blanket u32 sweep instead scrambles the
classname strings AND half-swaps those u16/u8 fields → a mis-swapped u16 count
over-allocates → **heap corruption** surfacing later at an innocent allocation
(observed: allocator free-list AV `0x0084DD5B`). The Rust backend was missing
this routing (Python had it) until it was wired — all 114 DLC animgroup blocks
were scrambled in the build before the fix.

> **Remaining gap:** **physics** `__data__` (PHY2) is still a blind u32 sweep
> (no `hkp*` classes in the registry), so any `u16`/`u8` fields inside `hkp*`
> objects are mis-swapped. Animation is now class-aware in both converters;
> adding HK550 physics layouts to `hk_class_layouts.py` is the next step if a
> collision-mesh crash recurs.
