# Rigid-accessory drawing-group → attachment-bone binding (B investigation)

> ## ★ 2026-07-12 — INCOMPLETE. This rule is a SPECIAL CASE. Read this box first.
>
> The rule below — *"the i-th top-level GEOM child attaches to `SEGM` record i"* — **omits the `INDX`
> indirection** and holds only where `INDX` is the **identity map**. It is:
>
> ```
> seg_id = INDX[sub_object]        <- the missing step
> SEGM[seg_id] = { bone, seg_id, state_mask }
> ```
>
> It was verified on `pmc_hum_mattias_v3`, whose `INDX` **is** identity (`[0,1,2,3,…]`) — so
> `SEGM[i]` and `SEGM[INDX[i]]` coincide and the shortcut looked correct. **No vehicle is identity:**
>
> ```
> pmc_hum_mattias_v3   INDX = [0,1,2,3,4,…]                    identity  -> shortcut works
> ch_veh_tank_ztz98    INDX = [0,1,2,3,4,5,6,17,20,23,73,80]   NOT       -> shortcut is WRONG
> vz_veh_tank_amx30_elite (P002)  = [2,5,8,10,12,14,16,19,…]   NOT       -> shortcut is WRONG
> ```
>
> Using the shortcut on a vehicle hands meshes **someone else's bone and someone else's LOD tier** —
> it is what threw the `amx30_elite`'s treads into the air. Everything else in this document
> (the GEOM-child walk, `SKIN` vs `MESH` rigidity, the world-rest placement) is **correct and still
> load-bearing** — only the join key needs the extra hop.
>
> **Authoritative:** [`vehicle_model_spec.md`](vehicle_model_spec.md) §2.

**Status: CONFIRMED (two independent methods) — but see the correction box above.** Verified against
`pmc_hum_mattias_v3` (`0xA3C1FABC`) and a destructible prop (`0xEEB8C3A8`), both of which have an
identity `INDX`, which is why the missing hop went unnoticed.

**This corrects `segm_group_bone_binding.md` (the "A" investigation), whose join
key — `PRMT[0].field0` → SEGM.seg_id — is WRONG.** `PRMT[0].field0` is the
**material index** (proven below by texture names), and the A path resolves the
eyes/hat/glasses accessories to **bone 0 (root)** instead of the eyeball/head
bones. The A doc's own §5.3 admits "segs 21–23 (head/eyes) … not referenced by
any PRMT" — that is the symptom of the wrong key. The correct key is **positional**.

---

## 1. The binding: positional GEOM-child order == SEGM record index

A model's geometry lives under a single `GEOM` marker (a UCFX descriptor row with
`u0 == 0xFFFFFFFF`). Its **direct children**, walked in descriptor pre-order, are the
top-level **drawing groups** — each one a `SKIN` marker (self-skinning body part) or a
`MESH` marker (rigid accessory). There are exactly as many top-level drawing groups as
there are `SEGM` records, and:

> **The i-th top-level GEOM child (0-based, in tree order) attaches to `SEGM` record i;
> its bone is that record's `u16@0` (a global HIER node index).**
>
> ★**CORRECTED:** the i-th top-level GEOM child attaches to `SEGM[INDX[i]]`, not `SEGM[i]`. The two
> coincide only when `INDX` is the identity map (true for `pmc_hum_mattias_v3`, false for every
> vehicle). The record's `u8@3` is additionally the **LOD-tier mask**, which this doc never noticed.

No pointer, hash, or index inside the group's own chunks names the bone. The join is the
**ordinal position** of the group among GEOM's children, matched 1:1 against the flat
`SEGM` array. `SKIN` groups always land on `SEGM` records whose bone is 0 (root) and are
skinned per-vertex; `MESH` groups land on the records carrying a real bone and are placed
rigidly by that bone's world-rest transform.

### 1.1 Exact byte walk (matches engine iterator `FUN_00478120`)

UCFX header: 20 bytes; then `ndesc` descriptor rows of **20 bytes** each
(`data_area_off = u32@4`, `ndesc = u32@16`). Descriptor row layout used here:

```
+0  char[4] tag
+4  u32     u0          0xFFFFFFFF => marker (container); else leaf, abs = data_area_off + u0
+8  u32     size        leaf payload size
+12 u32     x2          (sibling bookkeeping; NOT the bone — see §3)
+16 u32     x3          for a marker: number of descendant rows in its subtree
```

Sibling iteration (verbatim from `FUN_00478120` @0x00478120, the on-disk PRMG walker):

```c
// advance to next sibling of row iVar6:
next = *(int*)(descbase + iVar6*0x14 + 0x10)   // x3 (subtree descendant count)
     + 1 + iVar6;                               // + self
```

So to enumerate GEOM's children:

```
find the GEOM marker row g (tag "GEOM", u0==0xFFFFFFFF); let D = x3[g].
child = g + 1;  end = g + 1 + D;  i = 0
while child < end:
    if tag[child] in {"SKIN","MESH"} and u0[child]==0xFFFFFFFF:
        bone = segm[i].u16@0            # <-- the attachment bone
        i += 1
    child = child + 1 + x3[child]       # skip this child's whole subtree
```

`SEGM` is a sibling leaf chunk (24 records × 4 bytes here):
`{ u16 bone@0, u8 seg_id@2, u8 state_mask@3 }`, already LE in vz.wad. Record order **is**
the seg index; `seg_id@2` is a redundant dense 0..N-1 counter (not needed for the join).

### 1.2 Placement (unchanged from A)

```
skel  = Skeleton::from_block(block)          # skeleton.rs; per-bone world-rest 4x4
place = skel.world_bind(bone)                # row-major / row-vector
p_world = [p_local, 1] @ place               # transform every accessory vertex
```
Rigid accessory (`MESH`, no BLENDINDICES): multiply the whole sub-mesh by `place`.
`SKIN` group (has BLENDINDICES, bone==0): skin per-vertex; do **not** also apply `place`.

---

## 2. Verification — `0xA3C1FABC` (pmc_hum_mattias_v3)

100-bone HIER, 24 `SEGM` records, `GEOM` has 24 top-level children (17 `SKIN` + 7 `MESH`).
Reproduce: `cargo run -p mercs2_formats --example segm_probe -- <vz.wad> 0xA3C1FABC --tree`.

| grp# | marker | SEGM[i].bone | bone world-pos | material(s) (via rainbow table) | anatomy |
|------|--------|-------------|----------------|--------------------------------|---------|
| 0–16 | SKIN   | 0 (root)    | 0,0,0          | body/head/clothing (skinned)   | self-skin ✓ |
| 17   | MESH   | 8           | -0.14,1.08,-0.04 | m14 `player_irish_default_body` | neck/collar ring ✓ |
| 18   | MESH   | 7           | -0.12,1.07, 0.09 | m14 `player_irish_default_body` | neck/collar ring ✓ |
| 19   | MESH   | 6           |  0.00,1.08,-0.11 | m14 `player_irish_default_body` | neck/collar ring ✓ |
| 20   | MESH   | 5           |  0.13,1.08,-0.07 | m14 `player_irish_default_body` | neck/collar ring ✓ |
| **21** | MESH | **31 Bone_Head** | -0.00,**1.66**,-0.04 | m15 `..._hat`, m16/m17 `..._glasses` | **hat+glasses → HEAD** ✓ |
| **22** | MESH | **42 eyeball_right** | -0.03,**1.71**,0.04 | m18 `pmc_hum_fiona_eyes`, m19 `Reflection` | **eye+reflection → EYEBALL** ✓ |
| **23** | MESH | **41 eyeball_left**  |  0.03,**1.71**,0.04 | m18 `pmc_hum_fiona_eyes`, m19 `Reflection` | **eye+reflection → EYEBALL** ✓ |

**Both independent checks agree** on ≥2 accessories:
1. **Byte-path** (positional GEOM-child order → SEGM record → bone position) lands the
   eyes at y≈1.71 (eyeball bones), the hat/glasses at y≈1.66 (head bone), the ring at
   y≈1.08 (neck bones).
2. **Material identity** (PRMT[0].field0 → MTRL record → texture name via rainbow table)
   names those same groups `pmc_hum_fiona_eyes`/`Reflection`, `..._hat`/`..._glasses`.
   Eyes-material lands on eyeball bones; hat/glasses-material lands on the head bone —
   exactly the required ground truth.

This also proves **PRMT[0].field0 is the MATERIAL index, not seg_id**: group 21 has
`{15,16,17}`, group 22/23 have `{18,19}`, and those select MTRL[15..19] whose textures
are the hat/glasses/eyes/reflection maps. It is NOT a SEGM key.

### 2.1 Second model `0xEEB8C3A8` (destructible prop) — independent generalization
24 top-level `MESH` children, 24 `SEGM` records; every record carries a non-zero bone
(3, 5..11) at spread-out positions (±2–3 units). Positional mapping produces coherent
geometry: e.g. groups 4/5/6 all → bone 9 @ [1.43,3.21,1.21] (three LOD/damage sub-pieces
sharing one physical chunk, mirroring SEGM records 4/5/6 with masks 3/4/7). The rule holds
with no per-group bone pointer anywhere in the group chunks.

---

## 3. What is NOT the key (candidates falsified empirically)

- **`PRMT[0]` field0/8/12** — field0 = material index; field8 = index_count; not a bone.
- **`SKIN.INFO` / `MESH.INFO`** (a single u32) — value ∈ {1,2,3}; too few distinct values
  to key 7 distinct bones; unrelated to bone.
- **PRMG.INFO fields** — 56 B (skinned) vs 60 B (rigid). The rigid 60 B tail decodes as a
  **local-space bbox** (f32 corners/extents ≈0.02–0.28, origin-clustered) + material/decl
  hashes at u32[3],[4]. No bone index.
- **`AREA` chunk** — `AREA.info` = vertex count; `AREA.data` = per-vertex payload. No bone.
- **Descriptor `x2`** — on SKIN/MESH markers it counts DOWN 23→0 (remaining younger
  siblings). It equals `23 - grp#` here, so using it directly as a seg index would REVERSE
  the mapping (grp0→SEGM[23]) and break anatomy. It is tree bookkeeping, not the key.

---

## 4. Confidence / OPEN items

- **CONFIRMED**: positional GEOM-child-order → SEGM-record → `u16@0` bone, on two models,
  by byte-path AND material-name anatomy for ≥3 accessories (hat/glasses→head, both
  eyes→eyeballs, neck ring→bones 5–8).
- **CONFIRMED (partial engine)**: the child-iteration order and subtree-skip
  (`next = x3 + 1 + i`) is the engine's own (`FUN_00478120`). The GEOM marker handler
  (`0x4d4f4547` @ ~0x48d0..) reads two shorts from a stream into a per-object struct
  (count/index), consistent with an ordered geometry-list build.
- **OPEN (engine literal)**: the `SEGM` tag (`0x4d474553`) has **zero** references in the
  unpacked on-disk decomp — the record-array→group pairing executes in SecuROM-packed
  code not present on disk. The positional pairing is therefore proven from asset
  structure + the on-disk ordered iterator + anatomy, not yet from a decompiled SEGM
  consumer. An x32dbg trace on a model with rigid eyes would close this to fully-CONFIRMED.
- **OPEN (multi-record MESH state)**: prop groups sharing a bone across 3 SEGM records
  (masks 3/4/7/8/0x0F) are assumed LOD/damage-state variants of one attach; each still
  maps by its own ordinal to its own SEGM record with the same bone. Bind-pose placement
  is unaffected.
- **Note vs A doc**: `segm_group_bone_binding.md` §2 (PRMT.seg_id key) and its §3 table
  (eyes/hat → root) are **superseded** by this file. Its SEGM record layout (§1.1) and
  placement math (§1.2 step 3–5) remain correct.

Probe modes used: `--tree` (group→bone), `--desc` (descriptor table), `--mtrl`
(material→texture hashes); `crates/mercs2_formats/examples/segm_probe.rs`.
