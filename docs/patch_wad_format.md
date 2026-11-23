# Patch WAD System — Mercenaries 2 (PC)

> Reverse-engineered analysis of the engine's built-in patch WAD overlay mechanism.
>
> **Date:** 2026-05-18
> **Status:** Binary analysis complete. Patch WAD system verified end-to-end — FFCS structure, ASET overlay, and sges round-trip are correct. Initial test crashes were traced to UCFX header bugs in the block data, not the WAD format itself.

---

## Table of Contents

1. [Discovery](#1-discovery)
2. [How Patch WAD Loading Works](#2-how-patch-wad-loading-works)
3. [FFCS Structure Requirements](#3-ffcs-structure-requirements)
4. [Block Overlay / Replacement Mechanism](#4-block-overlay--replacement-mechanism)
5. [The `loading-patch.wad` Special Case](#5-the-loading-patchwad-special-case)
6. [Patch WAD vs Full WAD Patching](#6-patch-wad-vs-full-wad-patching)
7. [Creating a Minimal Patch WAD](#7-creating-a-minimal-patch-wad)
8. [SecuROM and DRM Implications](#8-securom-and-drm-implications)
9. [PATCHES.CAB Analysis](#9-patchescab-analysis)
10. [Open Questions](#10-open-questions)
11. [Practical Modding Guide](#11-practical-modding-guide)

---

## 1. Discovery

The demo executable `Merc2-Demo.exe` (17.1 MB) contains three format strings in its `.rdata` section that reveal a built-in patch WAD overlay system:

| File offset | VA | String |
|------------|-----|--------|
| `0x7AFEA8` | `0x00BAFEA8` | `%s\%s.wad` |
| `0x7B015C` | `0x00BB015C` | `%s\%s-patch.wad` |
| `0x7B016C` | `0x00BB016C` | `%s\loading.wad` |
| `0x7B017C` | `0x00BB017C` | `%s\loading-patch.wad` |
| `0x7BD400` | `0x00BBD400` | `%s\vz.wad` |

The strings are laid out sequentially in the read-only data section. The first `%s` is the data directory path (e.g., `data`), and the second `%s` is the WAD name stem (e.g., `vz`, `English`, `shell`).

### Surrounding context

The patch WAD strings sit in a cluster with related strings:

```
0x7B0100: "...the missing file: %s."        (error message)
0x7B0134: "Mercenaries 2: World in Flames Error"
0x7B015C: "%s\%s-patch.wad"
0x7B016C: "%s\loading.wad"
0x7B017C: "%s\loading-patch.wad"
0x7B0194: "IsLocked"
0x7B01A0: "path"
0x7B01A8: "texture"
0x7B01B0: "d:\first.ini"
```

The "missing file" error and "IsLocked" strings suggest the patch WAD code path handles file-not-found gracefully (the patch WAD is optional).

### Cross-reference limitations

Direct code cross-references to these VA addresses could not be found in the `.text` section because SecuROM encrypts the code. The `.securom` PE section (1.3 MB at file offset `0xF03000`) contains encrypted stubs that are decrypted at runtime. One reference to the `%s\vz.wad` string's file offset was found at `0x100A341` within the SecuROM section, confirming the WAD loading code passes through the SecuROM decryption layer.

---

## 2. How Patch WAD Loading Works

### Engine architecture (from Mercs 1 source code)

The Mercenaries engine uses `RedVirtualDisk` as its asset loading layer. Key design:

```cpp
class RedVirtualDisk {
    static DiskFile _Library[MAX_NUM_ASSET_FILES];  // up to 64 archive files
    static int32 _iNumLibrariesLoaded;

    static bool Open(const char* pFileName, ...);
    static RedAssetRequest* RequestAsset(uint32 uiName, uint32 uiType);
};
```

**Multiple archive files are opened into a library array.** When requesting an asset, the engine searches the library array **in reverse order** (newest-first):

```cpp
RedAssetRequest* RedVirtualDisk::RequestAsset(uint32 uiName, uint32 uiType)
{
    RedAssetRequest *rVal = NULL;
    for(int32 i=_iNumLibrariesLoaded-1; rVal==NULL && i>=0; --i)
    {
        rVal = _Library[i].RequestAsset(uiName, uiType);
    }
    return rVal;
}
```

This **last-opened-wins** lookup is the core overlay mechanism. If a patch WAD contains an asset with the same `(name_hash, type_hash)` pair as the original WAD, the patch version is found first and the original is never loaded.

### Mercs 2 WAD loading sequence (inferred)

Based on the format strings and the Mercs 1 loading pattern:

```
1. Open "%s\loading.wad"           (Loading.wad)
2. Open "%s\loading-patch.wad"     (loading-patch.wad — if it exists)
3. Open "%s\%s.wad"                (vz.wad, shell.wad, English.wad)
4. Open "%s\%s-patch.wad"          (vz-patch.wad, shell-patch.wad, etc. — if they exist)
```

The patch WADs are opened **after** the corresponding base WADs, placing them later in the `_Library[]` array. Due to the reverse-order search, assets in patch WADs override those in base WADs.

### File existence check

The Mercs 1 `RedVirtualDisk::Open()` returns `false` if the file doesn't exist:

```cpp
FileExists = PblDiscFile::Exists(filename);
if (!FileExists) return false;
```

The error message strings ("unable to continue due to the missing file") are for **required** WADs, not patch WADs. The patch WAD path almost certainly has a soft existence check — if the file doesn't exist, loading continues normally without it.

---

## 3. FFCS Structure Requirements

### Header layout

Every WAD file uses the identical FFCS header structure. A patch WAD must follow this exactly:

```
Offset  Size  Field
0x00    4     Magic "FFCS" (0x46464353)
0x04    4     Version (u32 = 2)
0x08    4     Declared chunk count (u32 = 7)
0x0C    60    5 chunk rows × 12 bytes each
0x48    144   Static build certificate blob (byte-for-byte identical in all WADs)
0xD8    40    Zero padding to 0x100
```

### The 0x48 blob

The 144-byte blob at offset 0x48 is **byte-for-byte identical** across all 8 tested WADs (4 demo + 4 retail). It is NOT derived from WAD content. A patch WAD must include this exact blob:

```
a8 d8 46 fa 28 87 0e 14  9a d3 31 71 e2 54 0a 8f
f8 ab 0a 3b 3e f1 5e 66  d0 f6 53 f7 78 e9 e5 39
5a 54 22 c1 54 1a b8 e6  87 4d df e8 c7 59 73 20
4e 90 0b 60 14 3c 27 e5  61 2d 98 de ce 7a e7 99
55 65 16 18 5d c3 47 56  bc 8d 0b fa 50 42 72 5b
86 2f 61 34 10 ca 8b 9f  5c 81 02 16 20 83 0e fe
f2 47 ce ac c4 30 7d 4d  d5 29 48 ea 7a 15 11 f0
14 63 fe bc 5a bd 08 56  7f 80 10 63 6a df b9 59
07 93 56 7c 71 03 e7 ec  bb 49 f6 1c 80 86 49 42
```

### Chunk row layout

Each chunk row is 12 bytes: `tag(4) + offset_or_value(4) + meta(4)`.

| Row | Tag | `offset` meaning | `meta` meaning |
|-----|------|-------------------|----------------|
| 0 | `INDX` | File offset to INDX data | Number of block entries |
| 1 | `DATA` | File offset to block data | Always 36 (0x24) |
| 2 | `CSUM` | Hash/identifier (NOT a file offset) | Largest entry count of any block in this WAD — sizes the engine's block-entry install ring; must be ≥ every block's entry count (see `format_reference.md` §CSUM) |
| 3 | `ASET` | File offset to ASET data | Number of ASET rows |
| 4 | `PTHS` | File offset to PTHS data | Number of path strings |

### DATA offset convention

All observed WADs use `DATA.offset = 0x00208000` (2,129,920 bytes = page 0x41 × 0x8000). The INDX page indices start at `0x41`, meaning the first block's `sges` data begins at file offset `0x208000`. The 2 MB gap between 0x100 and 0x208000 accommodates the INDX, ASET, and PTHS chunks. For a small patch WAD these chunks would be much smaller, but maintaining this offset convention is safest.

---

## 4. Block Overlay / Replacement Mechanism

### How assets are identified

The engine does NOT match blocks by INDX position or PTHS path name. Instead, it uses **asset hash matching** via the ASET chunk and the per-block header table.

Each decompressed `.block.bin` file begins with an asset index table:

```
Offset  Size  Field
+0      4     u32 record_count (N)
+4      N*16  Asset records (16 bytes each):
              +0  u32 asset_hash
              +4  u32 constant (0x42498680)
              +8  u32 zero
              +12 u32 chunk_size
```

Each `asset_hash` identifies one UCFX container within the block. These hashes appear in the WAD-level ASET chunk, forming a global asset registry.

### ASET row structure (16 bytes)

| Offset | Field | Notes |
|--------|-------|-------|
| +0 | `u32_0` | **Asset hash** — primary lookup key |
| +4 | `u32_1` | Dependency/mip block reference (0xFFFFFFFF = none) |
| +8 | `u32_2` | Packed field: `(block_index << 16) | secondary_ref` |
| +12 | `u32_3` | Small integer (type discriminator / LOD hint) |

The `u32_2` high 16 bits encode the **INDX block index** that contains this asset. This is how the engine resolves `asset_hash → block → file offset`.

### Overlay logic

When a patch WAD is opened after the base WAD, both WADs contribute to the engine's asset registry. Because `RequestAsset()` searches libraries in reverse order:

1. Engine receives request for asset `(name_hash, type_hash)`
2. Searches patch WAD's directory first
3. If found → loads from patch WAD (OVERRIDE)
4. If not found → falls through to base WAD (FALLBACK)

This means a patch WAD only needs to contain the **blocks that have changed**. Unchanged blocks are served from the original WAD automatically.

### PTHS structure

PTHS contains **null-terminated** path strings (one per INDX entry), concatenated sequentially. Each path is the block's source path (e.g., `blocks\VZ\scripts_vz_P000_Q3.block`). The N-th null-terminated string corresponds to the N-th INDX entry. PTHS is used for debugging/logging; the runtime asset lookup uses hash-based ASET, not path names.

### PTHS trailer marker (CRITICAL)

After all path strings, every WAD includes a mandatory **258-byte null-terminated ASCII marker string** (identical across all 8 tested WADs). The engine validates its presence — omitting it causes a black-screen hang during WAD loading.

Verify on built patches:

```bash
make verify-patch-wad-structure OUTPUT=./output WAD_VARIANT=full WAD_EXPECT_BLOCKS=2197
# or: .venv/bin/python3 tools/verify_patch_wad_structure.py --wad Data/vz-patch.wad
```

```
xa37dd45ffe100bfffcc9753aabac325f07cb3fa231144fe2e33ae4783feead2
b8a73ff021fac326df0ef9753ab9cdf6573ddff0312fab0b0ff39779eaff312
a4f5de65892ffee33a44569bebf21f66d22e54a22347efd375981188743afd9
9baacc342d88a99321235798725fedcbf43252669dade32415fee89da543bf23
d4ex
```

This marker is NOT counted in the PTHS `meta` field (which only counts block path strings). It appears as an extra null-terminated string immediately following the last path string's null terminator. The `PTHS.meta` value equals the number of INDX entries; the marker is always the `meta + 1`-th string in the PTHS data region.

The marker content looks like a hex-encoded build certificate or hash, bookended by `x` characters. Its exact purpose is unknown but it may be a Pandemic build pipeline artifact that the engine's `RedVirtualDisk::Open()` validates before accepting a WAD.

---

## 5. The `loading-patch.wad` Special Case

The `loading-patch.wad` string is separate from the generic `%s-patch.wad` pattern, suggesting the Loading WAD has special handling:

- Loading WAD provides assets needed during level transitions (loading screens, fonts, skulls)
- It is likely loaded first, before the main game WADs
- `loading-patch.wad` would override loading screen assets specifically

This confirms that the patch system applies to **all** WAD types:

| Original WAD | Patch WAD | Contents |
|-------------|-----------|----------|
| `vz.wad` | `vz-patch.wad` | World blocks, scripts, placements, meshes, textures |
| `English.wad` | `English-patch.wad` | Localized strings, voice-over references |
| `shell.wad` | `shell-patch.wad` | Menu/UI assets |
| `Loading.wad` | `loading-patch.wad` | Loading screen assets |

---

## 6. Patch WAD vs Full WAD Patching

### Advantages of patch WADs

| Aspect | Patch WAD | Full WAD replacement |
|--------|-----------|---------------------|
| File size | Tiny (only changed blocks) | Full WAD size (up to 2.4 GB) |
| Original files | **Untouched** | Must backup/modify original |
| SecuROM risk | **None** — original WAD intact | Could trigger integrity checks |
| Multiple mods | Stack multiple patches | Only one version of WAD |
| Uninstall | Delete patch file | Restore from backup |
| FFCS-level CSUM | Only for patch blocks | Must recompute for all blocks |

### Disadvantages

| Aspect | Notes |
|--------|-------|
| Cannot remove blocks | Only add or replace; cannot delete original blocks |
| Must match ASET hashes | Replaced blocks must contain assets with the same hashes |
| CSUM meta must cover every block | The CSUM row's `meta` sizes the engine's block-entry install ring (the largest `meta` of every mounted WAD); a block with more entries than that is retried forever and its level never starts. Write the largest entry count of any block in the patch (`patch_wad::csum_meta_for`) |
| Re-emitting a base block keeps its rungs only if they ship | A re-emitted block's rows name other base blocks as `_P001`..`_P003` rungs (vz resident's PMC-courtyard textures name `resident-pmcoutpost_fountain_P001`..`_P003`). Ship those blocks too, with `source_block_index`, so the writer re-points the rungs; otherwise it sentinels them and the assets render at their coarsest tier |

---

## 7. Creating a Minimal Patch WAD

### Requirements

A valid patch WAD needs:

1. **FFCS header** (256 bytes) with correct magic, version, chunk count, blob, and chunk rows
2. **INDX chunk** — 12-byte entries for each block in the patch
3. **ASET chunk** — 16-byte rows for each asset in the patch blocks
4. **PTHS chunk** — null-terminated path strings for each block
5. **DATA region** — `sges`-compressed block data at the offsets specified by INDX

### Step-by-step process

#### 1. Prepare the modified block

```bash
# Decompress the target block from the original WAD
.venv/bin/python3 tools/sges_decompress.py \
  --input output/extracted/ffcs_vz/blocks/scripts_vz_P000_Q3.sges \
  --output /tmp/scripts_vz.block.bin

# Modify the decompressed block (e.g., replace Lua bytecode)
# ... your modification tool here ...

# Recompress
.venv/bin/python3 tools/sges_compress.py \
  --input /tmp/scripts_vz_modified.block.bin \
  --output /tmp/scripts_vz_modified.sges
```

#### 2. Build the patch WAD

The patch WAD builder needs to:

1. Copy the original block's ASET entries (preserving all asset hashes)
2. Copy the original block's PTHS entry
3. Create a new INDX entry pointing to the compressed data offset
4. Assemble the FFCS header with the 0x48 blob
5. Write compressed block data at the DATA offset

#### 3. Deploy

```bash
# Place the patch WAD alongside the original
cp vz-patch.wad "Mercenaries 2 World in Flames/data/vz-patch.wad"
```

### Minimum viable WAD structure

For a patch containing a single block (e.g., `scripts_vz`):

```
[0x000-0x0FF]   FFCS header (256 bytes)
                  Magic "FFCS", version 2, chunk_count 7
                  INDX row: offset=0x8000, meta=1
                  DATA row: offset=0x208000, meta=36
                  CSUM row: offset=the base WAD's value, meta=largest block entry count
                  ASET row: offset=0x800C, meta=N (assets in this block)
                  PTHS row: offset=after_aset, meta=1
                  0x48 blob (144 bytes, exact copy)
                  Zero padding

[0x8000]        INDX data: 1 entry × 12 bytes
                  page_index=0x41, packed=segment_count, flags_and_pages

[0x800C]        ASET data: N entries × 16 bytes
                  (copied from original WAD's ASET for this block's assets)

[ASET_end]      PTHS data: 1 null-terminated path string
                  "blocks\VZ\scripts_vz_P000_Q3.block\0"

[0x208000]      DATA: sges-compressed block data
                  (output of sges_compress.py)

[DATA_end]      End of file (pad to 0x8000 boundary if needed)
```

### Important: ASET hash preservation

When replacing a block, all `asset_hash` values in the block's header table must remain identical to the originals. The engine locates assets by hash — changing hashes would make assets unfindable.

For blocks with a single UCFX container (like many vz_state blocks), there is exactly one asset hash. For multi-asset blocks (like `scripts_vz` with 114 Lua chunks), all 114 asset hashes must be preserved in both the block header table and the WAD-level ASET chunk.

---

## 8. SecuROM and DRM Implications

### SecuROM protects the EXE, not data files

SecuROM wraps the executable (`.cms_t` and `.cms_d` PE sections) but does not encrypt or checksum game data files. The `.wad` archives use Pandemic's proprietary FFCS format with independent integrity mechanisms.

### Adding a patch WAD does NOT modify any original file

This is the key advantage: the original `vz.wad` remains byte-for-byte identical. SecuROM cannot detect the presence of a new file alongside the originals. There is no file enumeration or data directory integrity check in SecuROM — it only validates the protected executable.

### The 0x48 blob and `vz.bin`

Both are static build certificates that are:
- **Identical across all WADs** (demo and retail)
- **Not derived from WAD content**
- **Not DRM artifacts** — they originate from Pandemic's build pipeline

A patch WAD should include the same 0x48 blob. The game may validate its presence but does not compute it from content.

---

## 9. PATCHES.CAB Analysis

The retail game's install directory may contain a `PATCHES.CAB` file. In the demo installation:

- **No `PATCHES.CAB` was found** in the demo directory
- The `webhelp.cab` in `Support/EA Help/` is an unrelated help file cabinet

The retail `PATCHES.CAB` (reported as 36 bytes) is an **empty MSCF (Microsoft Cabinet) container** — it has the CAB header but no files inside. This is likely a placeholder for EA's update system, suggesting the retail game was designed to receive content patches but none were ever shipped for the PC version.

This further supports the patch WAD system: the engine has built-in support for overlay files, and EA's distribution infrastructure (PATCHES.CAB) was prepared to deliver them.

---

## 10. Open Questions

### Critical (must resolve before shipping a patch WAD)

1. **Does the game validate the FFCS-level CSUM?** The CSUM "offset" field appears to be a hash rather than a file offset. For a patch WAD, what value should go here? Options:
   - `0x00000000` (null)
   - A computed hash of the patch's compressed block data
   - A copy of the original WAD's CSUM value

   The row's `meta` is answered: the largest entry count of any block (see the CSUM row in
   `format_reference.md`). The value is returned by the WAD class's vtable getter `0x00874D20`, whose
   callers are not traced; patch WADs carrying 0 (the Modkit's merge) and the base's value (qm) both
   load.

2. **Is the DATA `meta` field (always 36 / 0x24) validated?** Every WAD uses the value 36. This may be a fixed constant or may encode something about the DATA region.

3. **Does the INDX `packed` field need specific values?** The `packed` field (second u32 in each INDX entry) varies between 1-39 in observed data. It may encode segment count or compressed-size hints. Testing with value 1 is the safest starting point.

4. **Does the engine require ASET entries to be sorted?** If the runtime does a binary search on ASET `u32_0`, the entries must be in ascending hash order.

### Important (affects robustness)

5. **What is the INDX `flags` high 16-bit?** Values like `0x8000`, `0x8001`, `0x8040`, `0x80F0`, `0x8130` appear. The `0x8000` bit is always set. Lower bits may encode compression flags, LOD tiers, or resource priorities.

6. **Can a patch WAD have different block indices than the base WAD?** The ASET `u2_hi` field encodes block index. In a patch WAD with only 1 block, should this be index 0, or should it mirror the original WAD's block index?

7. **Does the Precache system interfere?** The `Precache/` directory contains pre-baked GPU data. If a patch replaces mesh or texture blocks, stale precache may cause rendering issues. Testing whether deleting precache forces regeneration is needed.

### Nice to know

8. **Can patch WADs be chained?** With `MAX_NUM_ASSET_FILES = 64`, theoretically up to 62 patch WADs could be loaded alongside 2 base WADs. The reverse-search means later patches override earlier ones.

9. **Does the loading order matter for cross-WAD dependencies?** If a patch WAD for `vz.wad` references textures from `shell.wad`, the dependency resolution may require `shell.wad` to be loaded first.

---

## 11. Practical Modding Guide

### Phase 1: Prove the concept (file presence test)

Create a minimal empty patch WAD and verify the game doesn't crash:

```bash
# Build a patch WAD with a single modified block
.venv/bin/python3 tools/build_patch_wad.py \
  --source-wad "path/to/vz.wad" \
  --block-index 1257 \
  --modified-block /tmp/scripts_vz_modified.sges \
  --output "data/vz-patch.wad"

# Launch game and verify normal operation
```

### Phase 2: Single block replacement

Replace the `scripts_vz` block with a modified version:

1. Extract and decompress `scripts_vz` from original `vz.wad`
2. Modify one Lua chunk (e.g., change a cash value in `wifpmcinterior`)
3. Recompute per-UCFX CSUM trailers (CRC-32/JAMCRC, init=0)
4. Update the block header table's `chunk_size` fields if sizes changed
5. Recompress with `sges_compress.py`
6. Build patch WAD containing only this block
7. Place as `data/vz-patch.wad` and test

### Phase 3: Multiple block replacement

Once single-block works:

1. Add texture blocks (DDS replacement)
2. Add placement blocks (entity position edits)
3. Test cross-block references (meshes referencing textures in other blocks)

### Tool requirements

| Tool | Status | Purpose |
|------|--------|---------|
| `sges_compress.py` | **Built** | Recompress modified blocks (raw deflate, multi-segment) |
| `sges_decompress.py` | **Built** | Decompress sges blocks from WAD data |
| `wad_patcher.py` | **Built** | In-place WAD modification (single block swap) |
| `build_patch_wad.py` | **Built** | Build standalone patch WADs. Supports single-block, raw/pre-compressed input, and all-in-one `--build-string-mod-patch` mode |
| `ffcs_patch_wad.py` | **Built** | Canonical FFCS layout module — shared by `build_patch_wad.py` and `dlc_port.py`. Provides `build_patch_wad_single()`, `build_patch_wad_multi()`, `merge_patch_wads()`, and `read_patch_wad()` |
| `aset_decoder.py` | **Built** | Decode ASET chunk → `block_dependency_graph.json` (asset hash → block index mapping) |
| `dlc_port.py` | **Built** | Xbox 360 DLC → PC `vz-patch.wad` conversion (multi-block, 2,197 blocks) |
| `verify_patch_wad_structure.py` | **Built** | Validate PTHS trailer marker + FFCS structural integrity |
| `verify_patch_wad_integrity.py` | **Built** | Verify per-block CSUM trailers and sges round-trip correctness |
| `validate_patch_wad.py` | **Built** | End-to-end patch WAD validation (ASET overlay, block hashes) |
| `diagnose_patch_wad.py` | **Built** | Diagnostic tool for patch WAD debugging (header dump, block inspection) |
| `trim_patch_wad.py` | **Built** | Remove/trim blocks from an existing patch WAD |

---

## Related Documentation

- [`docs/format_reference.md`](format_reference.md) — FFCS, sges, UCFX format specs
- [`docs/modding_deep_dive.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/modding_deep_dive.md) — DRM analysis, hash systems, Lua format
- [`docs/aset_format.md`](aset_format.md) — ASET chunk row layout
- [`docs/game_data_analysis.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/game_data_analysis.md) — Game data directory structure
