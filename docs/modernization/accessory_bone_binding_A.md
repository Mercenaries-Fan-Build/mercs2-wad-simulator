# Rigid-accessory → attachment-bone binding (Mercenaries 2 model UCFX)

**Status: CONFIRMED** (verified two independent ways — byte path *and* anatomical
placement of ≥2 accessory groups on their expected bones, across 3 models).

Derivation model: `pmc_hum_mattias_v3` (`0xA3C1FABC`) in
`data/vz.wad`. Cross-checked on `0x23A1501B` (22 segs) and `0x89A8AC72` (32 segs).

---

## TL;DR — the algorithm

A rigid accessory drawing group finds its bone through the model's **top-level
sub-object marker tree**, *not* through any field inside the group's own
`PRMG`/`PRMT`/`INFO`/`AREA`/`MESH` chunks:

```
group (PRMG)
  └─ parent top-level sub-object marker  (SKIN* or MESH*, a direct child of GEOM*)
        └─ k = ordinal of that marker among GEOM's direct children (0-based, tree order)
              └─ SEGM record[k]              (records are in seg_id order: u8@2 == k)
                    └─ bone = SEGM record[k].u16@0
                          └─ Skeleton::from_block world-rest 4x4 of that bone
```

So the join key is **positional**: *the k-th top-level sub-object under `GEOM`
binds to `SEGM` record `k`, whose `u16@0` is the bone index.* Rigid accessories
are the sub-objects whose marker tag is `MESH` (vs `SKIN` for skinned body
parts); their vertices are authored in that bone's local frame and must be
placed with the bone's world-rest transform.

**Do NOT use** `PRMT[0]` (that is the material index), the descriptor `+12`
field (`x2`, a reverse ordinal), or any `INFO`/`AREA`/`MESH-INFO` value as the
bone/seg key — none of them carry it.

---

## Container layout used (all offsets verified)

UCFX container: 20-byte header, then flat **20-byte descriptor rows**.
Header: `data_off = u32@4`, `n_desc = u32@16`.
Row `i` at `ro = 20 + i*20`:

| off | field                                                        |
|-----|--------------------------------------------------------------|
| +0  | 4-char tag                                                   |
| +4  | `u0`: `0xFFFFFFFF` ⇒ **marker/container** row; else data offset `= data_off + u0` |
| +8  | chunk byte size (for markers: subtree byte accounting)       |
| +12 | marker field `x2` — a **reverse ordinal** (count-1 … 0). NOT the seg. |
| +16 | marker field `x3` — **subtree descendant count** (rows consumed by this marker) |

Marker tags seen: `GEOM*` (the geometry root), and its direct children
`SKIN*` (skinned) / `MESH*` (rigid). A `MESH`/rigid sub-object additionally
carries an `AREA*` chunk; skinned ones carry per-vertex `BLENDINDICES` (a wider
`decl`). Child `PRMG*` groups nest **under** these markers.

### SEGM chunk

Located by scanning descriptor rows for tag `SEGM` with `u0 != 0xFFFFFFFF`;
bytes at `data_off + u0`, `size` bytes, **4 bytes per record**:

| off | type | meaning                                             |
|-----|------|-----------------------------------------------------|
| +0  | u16  | **bone index** (into HIER)                           |
| +2  | u8   | `seg_id` — always equals the record index (0,1,2,…) |
| +3  | u8   | `state_mask` — LOD/damage-state bitmask; `0x0F` = all |

`seg_id == record_index` verified on all 3 models (`0..N-1` sequential), so
"SEGM record k" and "seg_id k" are interchangeable.

### Walk (tree order)

Start at the `GEOM` marker; iterate its direct children. Each child consumes
`1 + x3` rows. The k-th child that is a `SKIN`/`MESH` marker is sub-object k →
SEGM record k. (`x2` counts down as k counts up, i.e. `x2 = (num_subobjects-1) - k`,
confirming these are the ordered sub-objects but that `x2` is not itself the seg.)

---

## Per-accessory verification — mattias_v3 (ANATOMICAL GROUND TRUTH)

SEGM (24 records). Body segs 0–16 → bone 0 (root, skinned). Accessory segs 17–23:

| seg | bone | bone hash    | world pos (from `Skeleton::from_block`) | anatomy |
|-----|------|--------------|-----------------------------------------|---------|
| 17  | 8    | 0x5663559D   | [-0.14, 1.08, -0.04]                    | neck ring |
| 18  | 7    | 0xD8B06174   | [-0.12, 1.07,  0.09]                    | neck ring |
| 19  | 6    | 0x1A8CEAD9   | [ 0.00, 1.08, -0.11]                    | neck ring |
| 20  | 5    | 0x44DAC356   | [ 0.13, 1.08, -0.07]                    | neck ring |
| 21  | 31   | 0x705C4508   | [-0.00, 1.66, -0.04]                    | **Head** |
| 22  | 42   | 0xB98D69C9   | [-0.03, 1.71,  0.04]                    | **eyeball L** |
| 23  | 41   | 0xC65682D2   | [ 0.03, 1.71,  0.04]                    | **eyeball R** |

Top-level `MESH` sub-objects (17–23) and the child PRMG materials they contain:

| sub-obj / seg | bone (world y)  | child PRMG materials                              | expected? |
|---------------|-----------------|---------------------------------------------------|-----------|
| 17 → bone 8   | neck 1.08       | m14 `player_irish_default_body` (neck-ring piece) | ✅ neck ring 5–8 |
| 18 → bone 7   | neck 1.07       | m14                                               | ✅ |
| 19 → bone 6   | neck 1.08       | m14                                               | ✅ |
| 20 → bone 5   | neck 1.08       | m14                                               | ✅ |
| 21 → bone 31  | **Head 1.66**   | m15 `..._hat`  **+**  m16/m17 `..._glasses`       | ✅ **hat + glasses → Head** |
| 22 → bone 42  | **eyeL 1.71**   | m18 `pmc_hum_fiona_eyes` **+** m19 `reflection`   | ✅ **eye/reflection → eyeball** |
| 23 → bone 41  | **eyeR 1.71**   | m18 `pmc_hum_fiona_eyes` **+** m19 `reflection`   | ✅ **eye/reflection → eyeball** |

Every accessory lands on its anatomically-correct bone. This also explains the
group/seg count mismatch that broke the naive "one seg per PRMG group" idea:
the 10 accessory PRMG groups (G19–G28) are **children of only 7 top-level MESH
sub-objects**. Hat (m15) and glasses (m16/17) share the Head sub-object; each
eye's `fiona_eyes` (m18) and `reflection` (m19) share that eye's sub-object.

Independent model #2 `0x89A8AC72` (32 segs): sub-objects cluster in triplets
(base / m3-detail / m5-trim) all binding to the same limb bone at distinct
world positions (b13,b13,b13 / b12… / b11…), each triplet a different joint —
a fully rigid model, per-sub-object per-bone, consistent with the rule.

---

## Decomp corroboration (runtime side)

`SEGM` is not present in `output/_ghidra` (SecuROM-packed), so the *on-disk*
group→seg parser could not be read directly; the byte rule above is derived from
asset structure + count invariants + anatomy. The **runtime consumer** *is*
decompiled and matches exactly:

- `FUN_00477e20` (draw setup): `DAT_01164754 = *(u16*)(model + 0x1c4 + (*(u16*)(model + 0x1c2)) * 4);`
  — a stride-4 array of `{u16 bone,…}` (the SEGM records live at model+0x1c4),
  indexed by a per-object seg index at model+0x1c2, storing the bone into the
  "current bone" global `DAT_01164754`.
- Draw loop (`~line 56990`, and the clean form at `~line 56806`):
  `record = *(model+0x1e0)+0x50 + k*4;` then it indexes a **per-seg pointer
  array** `*(model+0x1e0)+0x58` by `record.byte@+2` (= seg_id) to fetch that
  segment's draw sub-object, and gates drawing by `record.byte@+3` (state_mask)
  against the current state byte `iVar1+0x352`. This is `{u16 bone@0, u8
  seg_id@2, u8 state_mask@3}` byte-for-byte, and realizes `seg_id → sub-object`
  as an array lookup — the runtime image of the positional load-time binding.

`FUN_00478270` (per-`PRMG` parser, ret via `FUN_00478120`) reads `INFO` as
`0x3c`=60 bytes into the group struct's local-bbox fields and never touches a
bone/seg — confirming the group chunks carry no bone key.

---

## Implementation recipe (for the placement/repose code)

1. Parse `SEGM` → `seg_bone[k] = u16@(base + k*4)` for k in 0..count.
2. Walk `GEOM`'s direct children in tree order; for the k-th `SKIN`/`MESH`
   marker, all `PRMG` groups inside its subtree bind to `seg_bone[k]`.
3. A `MESH` marker (rigid; also has `AREA`, no `BLENDINDICES`) ⇒ apply
   `Skeleton::from_block` world-rest 4x4 of `seg_bone[k]` to the group's
   bone-local vertices. A `SKIN` marker ⇒ normal skinning (bone 0 root here).

## Confidence / OPEN

- **CONFIRMED**: positional map (marker ordinal = SEGM record index = seg_id),
  `SEGM` record layout, `u16@0` = bone, anatomical fit for all 7 mattias
  accessory sub-objects, and the count-invariant + runtime-decomp agreement on
  2 further models.
- **OPEN (minor)**: the meaning of descriptor `+12` (`x2`, a reverse ordinal)
  and whether any model ever emits top-level markers out of seg order (all
  observed models have `seg_id == record_index`, sequential). If a future model
  violates `u8@2 == index`, key off `record.u8@2` explicitly rather than raw
  position. The on-disk SEGM *writer/parser* opcode itself remains in packed
  code and was not read (not required — the rule is fully determined without it).
