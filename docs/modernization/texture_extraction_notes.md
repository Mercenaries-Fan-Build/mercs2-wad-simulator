# Texture / Material Extraction — Findings & Verification

**Status:** Research-derived + empirically verified against retail `vz.wad`.
**Date:** 2026-07-01
**Module:** [`tools/wad_simulator/crates/mercs2_formats/src/texture.rs`](../../crates/mercs2_formats/src/texture.rs)
**Probe:** `cargo run -p mercs2_formats --example texture_probe -- <vz.wad> 0xA3C1FABC`

This module resolves, for each drawing group of a model container, its diffuse
texture's **raw DXT/BC body** ready for direct `wgpu` upload (BC1/BC3 upload
natively — no CPU decode). Companion to `material_shader_spec.md`, which covers
the shader/BRDF side; this doc records the on-disk layouts and the empirical
verification behind them.

All verification uses **`pmc_hum_mattias_v3` = `0xA3C1FABC`** (per
[`docs/modernization/model_names.tsv`](model_names.tsv)), cross-checked on **`pmc_hum_mattias`
base = `0x0BBA3066`**.

---

## 1. MTRL chunk layout — CONFIRMED

Per-record layout (into the on-disk MTRL chunk body), decompile-verified from
`Mtrl_Parse` = `FUN_00858790` and re-confirmed against the live mattias_v3 bytes:

```
offset  size          field
 0      104            float preamble (color/emissive/specular params)
104     u16            flags
106     u16            tex_count        (1..=10; record-boundary signature)
108     tex_count×u32  texture asset hashes (slot 0=diffuse, 1=specular, 2=normal)
...     8              trailing floats
```

Inter-record stride = `116 + tex_count*4`. The walk stops when `tex_count` leaves
`1..=10` (a rare trailing-float tail), rather than mis-reading floats as hashes.

**Empirical (mattias_v3):** one MTRL leaf (desc[2], 2504 bytes) → **20 material
records**, tex_counts 1 and 3, flags in `0x0000..0x0098`. Every 3-slot record's
slot 0 resolves to a plausible diffuse (see §4). `parse_mtrl` reproduces this
exactly.

---

## 2. Group → material binding — CONFIRMED (double-blind)

Each `PRMG` drawing-group marker is followed by leaves including a `PRMT` chunk
of **16-byte records**. The prompt's guessed field roles
(`{seg_id, index_start, index_count, packed}`) did **not** match the bytes; the
verified layout is:

```
PRMT record (16 B):  u32 material_index @0,  u32 @4,  u32 @8,  u32 @12
```

`PRMG-INFO+0` = PRMT record count for the group. The **first word of each PRMT
record is the material index** into the MTRL array (§1).

### Method A — decompile (independent of the bytes)

`FUN_00478270` (the PRMG builder, dispatch VAs `0x0047817e`/`0x004783a5`) reads
`PRMG-INFO` (0x3c = 60 B): `local_3c` @0 = PRMT record count, allocates
`count*0x10` (16-byte records) at `param_1[0x6e]`. The PRMG struct is 0x1c4 bytes
(`FUN_00478120` strides by `0x1c4`). This establishes **record size = 16 B** and
**count source = INFO+0**, but not which field is the material index.

### Method B — semantic plausibility (independent of the decomp)

Taking PRMT[.0] as a material index and resolving
`group → material → material.diffuse → texture NAME` for **all 29 groups** of
mattias_v3 yields **body-part-correct names**:

| Group(s) | mat | diffuse NAME |
|---|---|---|
| G0, G10 | 0, 11 | `pmc_hum_mattias_v3_hair` |
| G1, G7 | 1 | `pmc_hum_mattias_v3_chain` |
| G2, G9, G13, G18 | 2, 10 | `pmc_hum_mattias_v3_lb` (lower body) |
| G3, G4, G5, G8, G12, G16 | 3,5,8,9 | `pmc_hum_mattias_head` |
| G6, G11, G15, G17 | 6, 12 | `pmc_hum_mattias_v3_ub` (upper body) |
| G23 | 15 | `pmc_hum_mattias_v3_hat` |
| G24 | 16, 17 | `pmc_hum_mattias_v3_glasses` |
| G26, G28 | 19 | `reflection` (env map) |
| G3, G6, G11, G15 (2nd rec) | 4,7,13 | `pmc_hum_strap` |
| G19–22 | 14 | `player_irish_default_body` (shared) |
| G25, G27 | 18 | `pmc_hum_fiona_eyes` (shared) |

The two methods **converge** with no shared assumption (A never looks at the
data; B never looks at the decomp). **CONFIRMED.** The layout also generalises to
the base model `0x0BBA3066` (28 groups, same PRMT[.0]-as-index shape, including
multi-material groups G3/G4).

**Multi-material groups.** A group may carry several distinct PRMT records
(e.g. mattias_v3 G6 = `[6,7,6]` → materials {6,7}); `group_prmt_material_indices`
returns the full de-duplicated list, `group_material_indices` returns the first.

**Duplication note (minor, OPEN).** Single-material groups store their one PRMT
record **twice** (`n=2` identical); `PRMG-INFO+0` is `1` for those. So INFO+0
counts *distinct sub-meshes/materials*, while the physical PRMT record count is
`INFO+0 + 1` (an extra terminator/degenerate record — e.g. G3 INFO+0=4, 5 PRMT
records). We collapse duplicates rather than rely on either count. This does not
affect material selection.

---

## 3. Texture container layout — CONFIRMED (double-blind)

A texture asset is a UCFX container with `NAME` / `INFO` / `BODY` leaves.

```
INFO (>=18 B):  u16 width @0,  u16 height @2,  u16 @4 (=1),
                u16 mip_count @6,  ...,  fourcc @14 ("DXT1"/"DXT5")
BODY:           contiguous linear DXT mip chain (no framing / no tiling)
```

### Method A — dimension bytes

mattias_v3 `..._hat` INFO → width@0 = 256, height@2 = 256, mip_count@6 = 7,
fourcc@14 = `DXT1`. `..._lb` → 512×512, mips 8, `DXT1`.

### Method B — BODY size == mip-chain math

`linear_mip_chain_size(w, h, fourcc, dxt_mip_count(w, h))` equals the BODY length
**exactly** (zero slack) for every fully-resident texture:

| texture | dims | fmt | mip_count@6 | BODY | chain(dxt_mip_count) |
|---|---|---|---|---|---|
| `_hat` | 256×256 | DXT1 | 7 | 43688 | 43688 |
| `_lb`/`_head`/`_ub` | 512×512 | DXT1 | 8 | 174760 | 174760 |
| `_hair` | 256×256 | DXT5 | 7 | 87376 | 87376 |
| `_glasses` | 128×128 | DXT1 | 6 | 10920 | 10920 |
| `reflection` | 64×64 | DXT5 | 5 | 5456 | 5456 |

Width/height read at @0/@2 and the size arithmetic corroborate each other
independently → **CONFIRMED**. `INFO@6` also equals `dxt_mip_count` in every
fully-resident case.

The BODY is the DXT body verbatim (e.g. head `01000000 55555555 …` = DXT1 blocks:
`0100`=color0, `0000`=color1, `55555555`=indices). Upload directly: DXT1 →
`Bc1RgbaUnorm(Srgb)`, DXT5 → `Bc3RgbaUnorm(Srgb)`; diffuse sRGB, normal/spec
linear (`material_shader_spec.md` §3b).

---

## 4. Texture resolution — primary + sub-entry fallback (CONFIRMED)

`extract_texture` resolves a texture hash via the ASET registry:

1. the **primary** ASET row (`sub_entry == 0xFFFF`, `type_id == 27`) → its block;
2. failing that, **any** `type_id 27` row for the hash — the texture is a
   shared/aliased asset carried as a **sub-entry** in another asset's block.

Four mattias_v3 diffuses have **no primary row**, only sub-entries — and are all
shared textures that resolve correctly via fallback:

| hash | NAME | block | note |
|---|---|---|---|
| `0x6D74F10B` | `pmc_hum_strap` | 2583 | shared strap |
| `0xE27C6F51` | `teeth` | — | shared |
| `0x2D237115` | `pmc_hum_fiona_eyes` | — | shared eyes |
| `0x5321E155` | `player_irish_default_body` | — | shared body |

With the fallback, **all 12 distinct diffuse textures of mattias_v3 resolve.**
Without it they report `no primary ASET` (the pre-fix behaviour).

---

## 5. Per-model verification (mattias_v3, full)

`texture_probe` output (abridged) — 20 materials, 29 groups, 12 distinct
diffuses, every group bound to a named body-part texture with dims/format:

```
mat[ 0] diffuse=0x9FB7C103 (pmc_hum_mattias_v3_hair)   256x256 Bc3
mat[ 2] diffuse=0x19205095 (pmc_hum_mattias_v3_lb)     512x512 Bc1
mat[ 3] diffuse=0xB86A929B (pmc_hum_mattias_head)      512x512 Bc1
mat[ 6] diffuse=0xEE6ACC8E (pmc_hum_mattias_v3_ub)     512x512 Bc1
mat[15] diffuse=0x0E756742 (pmc_hum_mattias_v3_hat)    256x256 Bc1
mat[16] diffuse=0xB7DCAD5D (pmc_hum_mattias_v3_glasses) 128x128 Bc1
...
G 2: mat2 -> pmc_hum_mattias_v3_lb   [512x512 Bc1]
G 6: mat6 -> pmc_hum_mattias_v3_ub   [512x512 Bc1] | mat7 -> pmc_hum_strap
G23: mat15-> pmc_hum_mattias_v3_hat  [256x256 Bc1]
G24: mat16-> pmc_hum_mattias_v3_glasses [128x128 Bc1]
```

---

## 6. API

```rust
pub struct MtrlMaterial { pub textures: Vec<u32> }       // slot 0=diffuse
pub fn parse_mtrl(container: &[u8]) -> Vec<MtrlMaterial>;
pub fn group_material_indices(container: &[u8]) -> Vec<usize>;         // group i -> material idx (first)
pub fn group_prmt_material_indices(container: &[u8]) -> Vec<Vec<usize>>; // group i -> all material idxs
pub enum TexFormat { Bc1, Bc3 }
pub struct TextureData { pub width, height, mip_count: u32,
                         pub format: TexFormat,
                         pub mip0: Vec<u8>, pub all_mips: Vec<u8> }
pub fn extract_texture(file, archive, name_hash) -> Result<TextureData, String>;
pub fn extract_model(file, archive, name_hash)   -> Result<Vec<u8>, String>;
pub fn extract_texture_name(file, archive, name_hash) -> Option<String>;
```

`all_mips` is the raw DXT/BC body (upload with the full chain); `mip0` is the
level-0 surface slice.

---

## 7. OPEN questions (for manual review)

1. **Streamed (partial-resident) textures.** Some shared diffuses have a BODY far
   shorter than their full dimension-derived chain — a *resident tail*, with the
   rest paged from the streaming store (`texsize::info_is_fully_resident`,
   `INFO[26:32]`). Examples: `player_irish_default_body` 512×512 body=2728 (not
   174760), `teeth` body=1360, `pmc_hum_fiona_eyes` body=2728. For these,
   `TextureData.all_mips`/`mip0` currently hold only the inline resident tail, and
   `mip_count` falls back to `INFO@6`. **The renderer must either (a) treat these
   as fully streamed and page the full chain from the streaming store, or (b) use
   only the resident tail's smallest mips.** The current module returns the
   resident bytes as-is; a `TextureData.fully_resident: bool` flag +
   streaming-store reader is the clean follow-up. *(Does not affect the
   fully-resident character body/head/ub/lb/hat/glasses maps, which are complete.)*
2. **PRMT fields @4/@8/@12.** Only field @0 (material index) is needed for
   texturing and is confirmed. The remaining three words (index_start-like @4,
   plus two large values) are the vertex/index-range binding into the STRM/IBUF;
   they are **not yet fully decoded** and are out of scope for texture
   extraction. Decode them when wiring per-sub-mesh draw ranges.
3. **PRMT record duplication / INFO+0 vs physical count** (see §2) — cosmetic;
   worth confirming the extra record is a strip-restart terminator when the draw
   path is reversed.
4. **flags & 0x200 env slot.** `Mtrl_Parse` binds an extra env/cube texture from a
   global registry when `flags & 0x200` (`material_shader_spec.md` §1b). None of
   mattias_v3's materials set it (max flag 0x0098); the `reflection` material uses
   an ordinary 3-slot record. Handle the env slot when adding cube reflections.

---

## Cross-references

- [`tools/wad_simulator/crates/mercs2_formats/src/texture.rs`](../../crates/mercs2_formats/src/texture.rs) — this module.
- [`tools/wad_simulator/crates/mercs2_formats/examples/texture_probe.rs`](../../crates/mercs2_formats/examples/texture_probe.rs) — probe.
- [`docs/modernization/material_shader_spec.md`](material_shader_spec.md) — shader/BRDF + MTRL semantics.
- [`tools/wad_simulator/crates/mercs2_formats/src/texsize.rs`](../../crates/mercs2_formats/src/texsize.rs) — mip-chain sizing.
- `output/_ghidra/all_functions_decomp.txt` — `FUN_00478270` (PRMG builder),
  `FUN_00858790` (`Mtrl_Parse`).
- [`docs/modernization/model_names.tsv`](model_names.tsv) — model hash → name.
