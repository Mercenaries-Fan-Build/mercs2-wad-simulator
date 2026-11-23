# Authoring a UCFX model container FROM SCRATCH (static prop)

> The modding deep-dive rated "author a model UCFX from scratch" as the hardest, unsolved tier
> (the engine rejects hand-authored decl/material bindings). This is that spec — every chunk of a
> **minimal static (non-destructible) model** reverse-engineered from real containers
> (2026-07-08, ground truth = `pmcoutpost_bld_hqsuites` 0xD5D65249). No donor is copied: a builder
> emits all chunks from these layouts, mints a NEW asset hash, and nothing existing is replaced.
>
> A static prop **omits all destruction chunks** (`SEGM/PHY2/STAM/SWIT/NODE/STAT/CHDR/CEXE`) — those
> are only for breakables and are the source of the `0x00478E43` twin-PRMT crash. Clean static =
> `INFO / HIER / MTRL / GEOM{ INFO, INDX, MESH{ INFO, PRMG{ INFO, STRM, AREA, IBUF, PRMT } } }`.

## Container envelope
```
"UCFX"            4 bytes  magic
data_off          u32      byte offset where the data area begins (= 20 + ndesc*20)
reserved          8 bytes  two u32 = 0,0  (donor observed all-zero)
ndesc             u32      number of 20-byte descriptor rows
descriptor rows   ndesc × 20 bytes   (the flattened chunk tree — see below)
data area         chunk bodies, each placed at row.u0, 16-byte aligned
"CSUM"            4 bytes
crc               u32      = crc32_mercs2(everything before this u32)  (zlib crc32, init/xor 0xFFFFFFFF)
```

## Descriptor row (20 bytes) — the chunk tree
`[tag:4][u0:u32][size:u32][u2:u32][u3:u32]`
- **tag** — 4-char chunk id (`INFO`,`HIER`,`MTRL`,`GEOM`,`MESH`,`PRMG`,`STRM`,`decl`,`data`,`info`,`AREA`,`IBUF`,`PRMT`,`INDX`).
- **u0** — body offset into the data area, or **`0xFFFFFFFF`** = this row is a **container** (no body; its children are the following rows).
- **size** — body byte length (0 for containers).
- **u2** — **number of sibling rows after this one at the same nesting level** (a reverse ordinal).
- **u3** — for a container, the **number of child rows** that follow; 0 for a leaf.

The tree is stored depth-first. Example (static model), with (u2,u3):
```
INFO   (3,0)        GEOM  (0,4) CONT
HIER   (2,0)          INFO (2,0)
MTRL   (1,0)          INDX (1,0)
GEOM   (0,4) CONT     MESH (0,2) CONT
                        INFO (1,0)
                        PRMG (0,4) CONT
                          INFO (3,0)   60-byte
                          STRM (2,3) CONT → info, decl, data
                          AREA (1,2) CONT → info, data
                          IBUF (0,2) CONT → info, data
                          PRMT (0,0)   (append 1 record per… see PRMT note)
```
(Top-level order in the donor is `INFO,HIER,MTRL,…,GEOM`; keep that order.)

## Chunk bodies

### INFO (top level, 72 B) — model info + bbox
`u32 flags(0x39 obs); f32 bbox_min[3]; f32 bbox_max[3]; u32 (0x2b0 obs); u32(4); u32(0x12); u32(4); u32(4); u32(0); u32(3); f32(100.0 LOD?); f32(5.0); u32(0); u32(0x00040003)`.
Author: set the two bbox vec3 to the model's min/max; the trailing constants can be copied verbatim (LOD/param defaults). 72 bytes.

### HIER (88 B per node; a static prop needs ONE root node)
`u32 node_hash (= the model's own asset hash for the root); u32 flags(0x00010001); u32 parent(0xFFFFFFFF for root); then a transform block` — identity observed:
`0, 1.0, 0, -0.0, 0,0,0,1.0, 0,0,0,1.0, 0,0,0,1.0, 1.0, 0` (row-major-ish 4×4 + 2 tail floats). 88 bytes total. Root node hash = model hash.

### MTRL (104 B preamble + N × record) — materials
- **Preamble (104 B):** `u32 shader_hash` then 100 B of default color/emissive/specular floats (mostly `1.0`). Copy a sane preamble verbatim.
- **Record (per material, PRMT `material_index` indexes these in order):**
  `u32 flag = (tex_count<<16) | flags(0x80)` — `tex_count` ∈ {1,2,3};
  then `tex_count × u32` texture hashes in **diffuse, specular, normal** order;
  then float properties (spec_power @ prop[16], spec_intensity @ prop[17]). Donor record stride = 128 B (3-tex). A minimal 1-texture material = `flag=0x00010080` + `diffuse_hash` + prop floats.
  Bind `diffuse_hash` to a shipped/global resident texture.

### GEOM children
- **INFO (4 B):** `u32` = mesh-group count.
- **INDX (2 B × groups):** one `u16` per mesh group = the HIER node it attaches to (donor: `01 00 03 00` → groups→nodes 1,3). For a single-group static prop → one `u16` = 0 (root).

### MESH child
- **INFO (4 B):** `u32` per-mesh id/index (1 obs).

### PRMG (per mesh group)
- **INFO (60 B):** `u32(1); u32(1); u32(0); u32 hash; f32 …` then at **offset 20**: `center[3], radius, bbox_min[3], bbox_max[3]` (10 floats). Author bounds from the group's verts.
- **STRM** container → `info`, `decl`, `data`:
  - **info (12 B):** `u32 flag(=4); u32 stride(=20); u32 vertex_count`.
  - **decl (32 B, stride-20 static):** verbatim —
    `00 00 00 00 10 00 00 00` POSITION FLOAT16_4 @0 ·
    `00 00 08 00 0f 00 05 00` TEXCOORD FLOAT16_2 @8 ·
    `00 00 0c 00 10 00 03 00` NORMAL FLOAT16_4 @12 ·
    `ff 00 00 00 11 00 00 00` END. (element = `[u16 stream][u16 offset][u16 type][u16 usage]`; type 0x10=FLOAT16_4, 0x0f=FLOAT16_2; usage 0=POS,5=TEXCOORD,3=NORMAL.)
  - **data:** the vertex buffer, stride 20: `f16 pos[3] + 2 pad | f16 uv[2] | f16 nrm[3] + 2 pad`.
- **AREA** container → `info` (`u32` = triangle count = strip_len−2), `data` (`f16` surface area per strip triangle; 0 for degenerate joiners).
- **IBUF** container → `info` (`u32` = strip index count), `data` (`u16` triangle-strip, degenerate-joined).
- **PRMT (16 B per record):** `u32 material_index; u32 start_index; u16 index_count; u16 base_vertex; u16 max_vertex_index; u16 vertex_count`. One record = whole strip: `{mat, 0, strip_len, 0, vcount-1, vcount}`.
  **Static prop: one PRMT record is fine** (the twin-record requirement is a *destructible* state-pair; a clean static model has none — so no twin, no `0x00478E43` crash).

## Build & verify
Emit the tree, place bodies 16-byte-aligned, backfill `u0`/`size`, append `CSUM`+crc. Then:
`smuggler --inject-extra "0x<newhash>:19:model.bin"` mints it as a NEW model asset (new ASET entry,
sub `0xFFFF`, type 19). Validate with `wad_simulator` (ASET verified, model consumed, 0 violations).
Place it with a new `SceneObject` (Transform+Name+ModelName→newhash) via the COMP-block writer.

This is implemented in Rust as `mercs2_formats::model_build` (see that module) — no Python, no donor.
