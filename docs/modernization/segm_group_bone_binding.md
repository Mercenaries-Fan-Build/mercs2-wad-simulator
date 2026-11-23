# Drawing-group → attachment-bone binding (SEGM)

Scope: how the native renderer places a model **drawing group** (PRMG sub-mesh) whose
vertices are authored in **bone-local space** (rigid accessories with no per-vertex
BLENDINDICES). The binding lives in the **`SEGM` chunk**, keyed by the group's **`PRMT`
segment id**. Verified against `pmc_hum_mattias_v3` (`0xA3C1FABC`) and a destructible
prop (`0xEEB8C3A8`).

This corrects/extends `skinning_animation_spec.md` §1.4, which read SEGM records as
`00 00 seg_id lod` and concluded SEGM carries no bone index. That "`00 00`" is only true
for the **self-skinning body** segments (their bone field is legitimately 0). The
**rigid** segments carry a **non-zero bone index** in the first field — that is the
attachment bone. See "Reconciliation" below.

---

## 1. Exact chunk + byte layout

### 1.1 `SEGM` — segment→bone table
A flat array of **4-byte records** (leaf chunk `SEGM`, resolved via the UCFX descriptor:
`abs = data_area_off + row.u0`). One record per **segment**.

```
struct SegmRecord {          // 4 bytes, little-endian on PC (vz.wad)
  u16 bone;                  // @0  HIER node index this segment attaches to (GLOBAL, not a palette slot)
  u8  seg_id;                // @2  segment id — the value PRMT.field0 references (0..N-1, dense)
  u8  state_mask;            // @3  LOD/state bitmask: 1,2,4,8 (per-LOD) or 0x0F (all/rigid); grouping only
}
```

- `bone` is a **global HIER node index** (same index space as BLENDINDICES and
  `Skeleton::bones[i]`), NOT a per-group palette index. It is `0` (root / `GlobalSRT`)
  for self-skinning body segments and a **real bone** for rigid segments.
- On the Xbox/PS3 source the `bone` u16 is **big-endian**; the LE converter swaps only
  those 2 bytes per record (`ucfx_byteswap::convert.rs`, `ChunkTag SEGM` arm:
  `body.swap(o, o+1)` every 4 bytes). The two `u8`s are not swapped. On PC vz.wad the
  records are already LE, so read them directly.

### 1.2 `PRMT` — the group's segment reference

> ### ⛔ SUPERSEDED (2026-07-30) — field 0 is `material_index`, NOT `seg_id`
>
> The record layout below is **disproved**. [`docs/modernization/texture_extraction_notes.md`](texture_extraction_notes.md)
> §2 tested exactly this guess against the bytes and states: *"The prompt's guessed field
> roles (`{seg_id, index_start, index_count, packed}`) did **not** match the bytes; the
> verified layout is `u32 material_index @0`"* — CONFIRMED double-blind, and corroborated by
> [`docs/reverse_engineer/valid_model_structure_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/valid_model_structure_map.md)'s PRMT section
> (`[material_index:u32][start_index:u32][index_count:u16][base_vertex:u16][max_vertex:u16][vertex_count:u16]`,
> with `material_index` MUST be `< MTRL material count`).
>
> **The group→segment join is therefore NOT via PRMT field 0.** Everything in §2 onward that
> reads `PRMT[0]` as a `seg_id` needs re-deriving; the SEGM record layout in §1.1 and the
> anatomical placement evidence are unaffected, because those were measured from SEGM itself.
>
> **Why this marker exists:** the layout below was left stated as plain fact with no caveat,
> and a later investigation read this doc, took it at face value, and budgeted an x32dbg
> session to resolve a question the project had already answered. If you are reading a bare
> assertion in this corpus, `corpus_search` the field name before you act on it — a
> `CONFIRMED (double-blind)` hit outranks an unmarked one.

Each drawing group is one `PRMG` container marker; inside it a `PRMT` leaf holds one or
more **16-byte primitive records**:

```
struct PrmtRecord {          // 16 bytes, LE   ⛔ SUPERSEDED — see the note above
  u32 seg_id;                // @0  DISPROVED: this field is `material_index`
  u32 index_start;           // @4  first index into this group's IBUF
  u32 index_count;           // @8  index count (matches IBUF/AREA info count)
  u32 packed;                // @12 packed prim/material info
}
```

`PRMT.seg_id` (field 0) is the join key into `SEGM`. A group may have several PRMT
records (multi-material / LOD split); each names its own segment.
⛔ **Both sentences above are superseded** — field 0 is the material index, so how a group
names its segment is an OPEN question. See §5 open question 4.

---

## 2. Algorithm: group_index → bone_index

```
1. Walk the UCFX descriptor rows. Collect, in order, every `PRMG` (u0==0xFFFFFFFF)
   marker — group i is the i-th PRMG.  Locate the `SEGM` leaf and the `HIER` leaf.
2. Parse SEGM into records; build seg_id -> bone:
      seg_to_bone[rec.seg_id] = rec.bone        // for the chosen state_mask (see note)
3. Parse HIER via Skeleton::from_block -> world-rest matrix per bone (skeleton.rs).
4. For drawing group i:
      seg   = PRMT[0].seg_id                     // first PRMT record of that PRMG
      bone  = seg_to_bone[seg]                   // GLOBAL HIER index
      place = Skeleton.world_bind(bone)          // 4x4 world-rest (row-major, row-vector)
      // rigid accessory has NO BLENDINDICES: transform every vertex by `place`
      p_world = [p_local, 1] @ place
5. Skinned groups (decl has BLENDINDICES) resolve `bone == 0` (root) and are skinned
   per-vertex instead — do NOT also multiply them by the segment transform.
```

Practical decision rule for the renderer, per group:
- **decl has BLENDINDICES** (mattias: stride ≥ 32, decl_elems ≥ 7 with a
  BLENDINDICES/BLENDWEIGHT element) → skin per-vertex; ignore SEGM bone (it is 0).
- **decl has no BLENDINDICES** (rigid accessory: decl_elems 5–6) → look up
  `seg_to_bone[PRMT[0].seg_id]` and multiply the whole sub-mesh by that bone's
  world-rest matrix.

Note on `state_mask`: several SEGM records can share a `bone` across LOD masks
(`0x0F` = the rigid/all case used by accessories). When more than one record matches a
`seg_id`, the rigid/all (`0x0F`) or highest-LOD record is the one to use; the bone is the
same across a piece's LOD variants in the models inspected.

---

## 3. Verification evidence — `0xA3C1FABC` (pmc_hum_mattias_v3)

100-bone HIER, 24 SEGM records, 29 PRMG groups. Bone name-hashes resolve via
`pandemic_hash_m2` in the rainbow table.

SEGM records (raw `bone seg_id mask`):
```
 seg 0..16 : bone 0   (GlobalSRT root, pos 0,0,0)          masks 1/2/4/8  -> self-skinning body LODs
 seg 17    : bone 8   pos [-0.14, 1.08,-0.04]  mask 0x0F   (neck/collar ring, parent bone 4)
 seg 18    : bone 7   pos [-0.12, 1.07, 0.09]  mask 0x0F
 seg 19    : bone 6   pos [ 0.00, 1.08,-0.11]  mask 0x0F
 seg 20    : bone 5   pos [ 0.13, 1.08,-0.07]  mask 0x0F
 seg 21    : bone 31  Bone_Head           pos [ 0.00, 1.66,-0.04]  mask 0x0F
 seg 22    : bone 42  bone_eyeball_right  pos [-0.03, 1.71, 0.04]  mask 0x0F
 seg 23    : bone 41  bone_eyeball_left   pos [ 0.03, 1.71, 0.04]  mask 0x0F
```
Bone names confirmed: `0xCBC1EB51=GlobalSRT`, `0x705C4508=Bone_Head`,
`0xB98D69C9=bone_eyeball_right`, `0xC65682D2=bone_eyeball_left`.

Drawing-group classification (`--meshes` bbox + decl):
- Groups 0–18: skinned body parts — full-body bboxes (y up to ~1.84), decl 7–14 with
  BLENDINDICES, each has a `SKIN`/`MESH` marker. PRMT.seg_id ∈ 0–13 → SEGM bone 0
  (correct: they self-skin).
- Groups 19–28: **rigid accessories** — origin-clustered bboxes (|y| ≤ 0.22, |x,z| ≤ 0.2),
  decl 5–6 (no BLENDINDICES), each uniquely has an `AREA` chunk. PRMT.seg_id ∈ 14–19.

Accessory group → segment → bone → world placement:
```
 G19 seg14 bone0  (root)        26v   floats at origin/feet cluster
 G20 seg14 bone0  (root)        26v
 G21 seg14 bone0  (root)       185v
 G22 seg14 bone0  (root)        26v
 G23 seg15 bone0  (root)       614v   (x[-0.03,0.20] asymmetric -> foot/boot-like)
 G24 seg16 bone0 / seg17 bone8 234v   two segments (root body + neck-ring piece)
 G25 seg18 bone7  y=1.07       102v -> lands at neck/collar ring (1.05..1.08 world)
 G26 seg19 bone6  y=1.08        85v -> neck/collar ring
 G27 seg18 bone7  y=1.07       102v
 G28 seg19 bone6  y=1.08        85v
```
The rigid groups that reference the non-zero-bone segments (18,19 → bones 6/7)
**place plausibly** at the neck/collar ring (world y ≈ 1.07). The head/eye bones
(segs 21–23) are present in SEGM but not referenced by any PRMT in this asset — the
eyes/teeth here are skinned into the head body-mesh (group 3, y 1.50–1.79), not shipped
as rigid accessories, so those attach slots are unused in this model.

**Second model `0xEEB8C3A8`** (destructible prop, bone `bone_massive_2x1_b`): every SEGM
record has a **non-zero** `bone` (10,11,9,8,7,6,5,4,3) at spread-out positions
(±2–3 units), 3 records per piece (masks 3/4/7/8/15 = LOD/state), and PRMT.seg_id
selects the piece. This independently proves `u16@0` is the attach-bone field
(it is simply 0 for a humanoid's self-skinning body segments) and matches the
`orchestrator.rs` destruction reading `{u8 node, u8 0, u8 seg, u8 type}` (the low byte of
the u16 = node for small indices).

Reproduce: `cargo run -p mercs2_formats --example segm_probe -- <vz.wad> 0xA3C1FABC`
(the probe dumps SEGM records, HIER bones, and per-group PRMT/INFO/AREA;
`--list` enumerates all models with a SEGM chunk).

---

## 4. Reconciliation with `skinning_animation_spec.md` §1.4

⚠️ **CORRECTION (superseded):** the earlier claim that *"skinned vertices use global
BLENDINDICES with no palette remap"* is **DISPROVEN**. Skinned vertices are **per-group
PALETTE-RELATIVE**: they index the concatenation of the group's `INFO(56)` range table
(`+20 u32 range_count`, then `{u16 hier_base, u16 count}×rc` from `+24`), which the engine
expands to global HIER at upload (oracle `FUN_00479d90`). Proven + screenshot-verified — see
`blendindices-per-group-palette`, the reader `mercs2_formats::model_cubeize`, and the faithful
WRITER `mercs2_formats::char_skin` (+ `inject_character_into_donor_block`). Do NOT author
skinned characters with global indices. The SEGM finding below is independent and stands.

Its side-claim "SEGM is `00 00 seg_id lod`, carries no bone" is a
**partial reading**: it only sampled the body segments, whose bone field is genuinely 0.
The bone field is non-zero for rigid segments (proven above). Update the two specs to
agree: `SEGM.record = {u16 bone, u8 seg_id, u8 state_mask}`; bone 0 ⇔ self-skinning,
bone ≠ 0 ⇔ rigid attach.

---

## 5. Open questions

1. **State-mask semantics.** `state_mask` values 1/2/4/8 look like per-LOD bits and
   `0x0F` = all/rigid. Not yet confirmed whether the engine filters draw calls by an
   active-LOD bit at runtime, or whether the mask also encodes damage-state (destructible
   pieces show 3/4/7/8). Does not affect static bind-pose placement.
2. **Multi-segment PRMT groups.** G24 references two segments (root + a neck-ring bone).
   Assumed each PRMT record's index range is transformed by its own segment's bone. Not
   yet visually verified that a single group can mix a root-space and a bone-local range.
3. **Unused attach slots.** Segs 21–23 (head/eyes) exist but no PRMT references them here.
   Confirm on a model that *does* ship rigid eyes/teeth as accessories (e.g. one of the
   higher-record humanoids from `--list`) that PRMT then references those seg_ids.
4. **Engine confirmation.** The literal `SEGM` tag constant does not appear in the
   on-disk PC decomp (the model chunk dispatcher `FUN_00478120` handles `PRMG`/`INFO`
   directly; SEGM consumption is likely in a SecuROM-packed thunk). The binding here is
   reverse-engineered from asset structure + two converters + anatomical placement, not
   yet from decompiled engine code. An x32dbg trace on a live model with a rigid accessory
   would close this.
```
