---
status: current
evidence: proven
verified_on: 2026-07-21
witness: |
  Re-measured end-to-end against retail game-files/vz.wad with a throwaway Rust probe,
  tools/wad_simulator/crates/mercs2_probe/src/bin/docaudit.rs:
    cargo run --release -p mercs2_probe --bin docaudit -- layers      (§2: 173 sub-blocks, 722 COMPs,
        stride 42 divides all 166 Transform blobs -> 62,624 records, quat@+0x14 unit-norm 62,624/62,624,
        pad@+0x10 nonzero 0/62,624, pos X -3887.5..3800.0 Y -102.8..392.6 Z -3868.9..3800.0)
    cargo run --release -p mercs2_probe --bin docaudit -- vzstate     (§3: 746 blocks, 744 carry a
        Transform COMP, 37,867 records; pmccon004 = 361, not 53)
    cargo run --release -p mercs2_probe --bin docaudit -- part2 <wad> vz_state   (§3.3 heuristic lands
        at Transform-blob offset === 28 mod 42 in 3/3 blocks -> the "flgs record" is a shifted window)
    cargo run --release -p mercs2_probe --bin docaudit -- part5       (§2.9: 398/400 lrterrain records
        match lrterrain_r{i/20}_c{i%20} by name; vz_state sizes 11,012..119,340 B)
    cargo run --release -p mercs2_probe --bin docaudit -- lrterrain   (400/400 mesh_hash in TOC)
    cargo run --release -p mercs2_probe --bin docaudit -- part3       (c3: 9,467 blocks, 0/48 sampled
        contain COMP/flgs/enum; 0 layer-type entries in any c3 block)
  §2 (layers_static) is CONFIRMED field-by-field. §3 (vz_state) was WRONG and is corrected in place:
  vz_state placements are the same Transform COMP as layers_static, not a bespoke flgs record.
  §5.1 UE/glTF transform rows and the "units are metres" landmark check are NOT re-measurable from
  WAD data and remain unverified.
---

# Mercenaries 2: World Placement Data Format — Complete Specification

**Date:** 2026-05-15 · **Audited:** 2026-07-21  
~~**Status:** VERIFIED from binary inspection. All findings based on actual byte-level analysis.~~

> **AUDIT (2026-07-21).** The blanket "VERIFIED" above was not earned. §2 (layers_static) survives
> re-measurement intact — every offset, the 42-byte stride, the quaternion and the position ranges
> are now confirmed by a re-runnable probe. **§3 (vz_state) did not.** Its "flgs placement record"
> is a *misaligned window over the very same 42-byte Transform COMP records* that §2 describes,
> shifted by +28 bytes; consequently §3.1's "version", §3.2's "ABSOLUTE offsets", §3.3's whole field
> table, §3.6's counts, §6.2 and half of §7 were wrong. Corrections are marked inline and dated;
> nothing is deleted. **Decoder of record is now Rust** — `mercs2_formats::placement`
> (`load_placements`, `comp_inventory`), which handles layers_static *and* vz_state with one code
> path. [`tools/placement_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/placement_extractor.py) is legacy and carries the §3 bug.

**Decoder:** ~~[`tools/placement_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/placement_extractor.py)~~ → [`tools/wad_simulator/crates/mercs2_formats/src/placement.rs`](../crates/mercs2_formats/src/placement.rs)

---

## 1. Architecture Overview

Mercenaries 2 stores world placement data in two complementary systems extracted from `vz.wad`:

| Layer | File | Size | Purpose | Placements |
|-------|------|------|---------|------------|
| **layers_static** | `00029_blocks__VZ__layers_static_P000_Q3.block.bin` | 7.97 MB | Base world layer — always loaded | ~~62,458~~ → **62,624** |
| **vz_state** | 746 files (`*_vz_state_*_P000_Q3.block.bin`) | 11 KB–120 KB each | Conditional overlays — mission/faction states | ~~~3,620 total~~ → **37,867** |

**CORRECTION (2026-07-21).** Two of the four numbers in this table were wrong.

- layers_static holds **62,624** Transform records, not 62,458 (§2.10 and §10 of this same document
  already said 62,624 — the table was the stale copy). Measured: 166 Transform COMP `data` blobs
  totalling 2,630,208 bytes; 42 is the largest stride that divides *every* blob;
  2,630,208 / 42 = 62,624, and all 62,624 entity keys are distinct.
- vz_state holds **37,867** placement records across **744 of 746** blocks, not "~3,620". The old
  figure came from the broken flgs heuristic in §3.3 (see the correction there), which only
  recovered a fraction of each file's records.
- Confirmed unchanged: block is 7,967,868 B decompressed (7.97 MB decimal); 746 vz_state blocks;
  their decompressed sizes span 11,012–119,340 B (i.e. "11 KB–120 KB"); 9,467 c3 blocks.

Repro: `cargo run --release -p mercs2_probe --bin docaudit -- layers` and `-- vzstate` and `-- part5`.

A third block type, **c3XXXX cells** (9,467 blocks), contains geometry/terrain data and does **NOT**
contain placement data. *(Confirmed 2026-07-21: of 55,429 UCFX entries WAD-wide, **zero** layer-type
`0xE6B81A54` entries live in a c3 block, and 0 of 48 sampled c3 blocks contain a `COMP`, `flgs` or
`enum` tag anywhere in their decompressed bytes.)*

---

## 2. layers_static Format

### 2.1 File Structure

The layers_static file is a **composite container** of 173 UCFX sub-blocks:

```
[16-byte TOC] × 173+1 entries
[Sub-block 0: UCFX → CHDR → COMP chunks]
[Sub-block 1: UCFX → CHDR → COMP chunks]
...
[Sub-block 172: UCFX → CHDR → COMP chunks]
```

### 2.2 Table of Contents (TOC)

Each TOC entry is 16 bytes:

~~```
Offset  Size  Type    Field
------  ----  ------  -----
+0x00   4     uint32  payload_size (entry 0 = sub-block count = 173)
+0x04   4     uint32  name_hash
+0x08   4     uint32  format_hash (constant: 0xe6b81a54)
+0x0C   4     uint32  zero
```~~

**CORRECTION (2026-07-21) — field names shifted by 4 bytes.** This is not a bespoke "TOC"; it is the
*standard decompressed-block entry table* every Mercs 2 block uses (`u32 count`, then `count` × 16 B
rows). The table above reads the same bytes with the whole array slid 4 forward, which is why it had
to invent a phantom "entry 0" and an off-by-one ("entry [N+1] holds the size of sub-block N"). Actual
layout:

```
+0x00   4     uint32  entry_count            (= 173 for layers_static; = 1 for every vz_state block)
then entry_count × 16 bytes:
  +0x00 4     uint32  name_hash              (sub-block asset hash)
  +0x04 4     uint32  type_hash              (constant 0xE6B81A54 = pandemic_hash_m2("layer"))
  +0x08 4     uint32  field_c                (0 in all 173 rows)
  +0x0C 4     uint32  chunk_size             (byte size of THIS sub-block's UCFX container)
```

Raw head of the block: `ad 00 00 00 | 10 c7 1f b4 | 54 1a b8 e6 | 00 00 00 00 | 72 62 00 00`
= count 173, name 0xB41FC710, type 0xE6B81A54, 0, size 25202.

**The substantive claim survives:** UCFX spacing exactly matches the declared sizes — walking
`4 + 173*16 = 2772` then adding each `chunk_size` lands on all 173 `UCFX` magics with **0 / 173
mismatches**. Repro: `cargo run --release -p mercs2_probe --bin docaudit -- layers`.

### 2.3 Sub-block Internal Structure

Each sub-block begins with UCFX and contains a CHDR chunk table:

```
UCFX header (20 bytes)
CHDR header (20 bytes)
Chunk table entries:
  - enum (0 children) — shared enum string table
  - COMP (3 children: info, schm, data) × N — component definitions
  - flgt (0 children) — float type marker
  - flgs (0 children) — placement state data
```

**CRITICAL: In layers_static, COMP child offsets are RELATIVE to the end of the chunk descriptor table (`data_area_start`).**

*Confirmed 2026-07-21, and it is true of **vz_state too** (see the §3.2 correction). Header sizes
confirmed: the UCFX header is 20 B (`"UCFX"` + 4×u32) with `CHDR` immediately at +20; the CHDR header
is 20 B (`"CHDR"` + 4×u32) with the chunk rows at +20 and the row count at CHDR+12. Sub-block 0 of
layers_static declares 5 rows = `enum`, `flgt`, `flgs`, and 2 × `COMP` (Name + Transform).*

*One caveat on the row list above: `flgs` is described as "placement state data", but in layers_static
sub-block 0 the `flgs` row's payload is **4 bytes**, and across vz_state it ranges 4–3,496 B. It is
not, and never was, where placements live.*

### 2.4 COMP Component Types (VERIFIED)

Each COMP block has an `info` child containing the component type name. Key types for placement:

| Component Name | Purpose |
|---------------|---------|
| **Transform** | Contains the 42-byte placement records with XYZ + rotation |
| **Name** | Entity name strings with hex IDs (`"EntityName 0xHEXID"`) |
| **DestructionLink** | Links between destroyable entity groups |
| **HibernationControl** | LOD/streaming visibility parameters |
| **Anchor** | Attachment points |
| **LightObject** | Light source definitions |
| **IntersectionToIntersection** | Road network connections |
| **RoadIntersection** | Road junction definitions |
| **DangerousBuilding** | Buildings that can collapse |
| **LowResTerrainObject** | Maps each `lrterrain_rXX_cYY` entity to its mesh asset hash (see §2.9) |

### 2.5 Transform Record Format (42 bytes)

VERIFIED: Records start at **byte offset 0** (no header) within the Transform COMP data section. Each record is exactly 42 bytes. The first 4 bytes are the **u32 entity key** — the same key used in all other COMPs for that entity.

```
Offset  Size  Type     Field            Description
------  ----  -------  ---------------  --------------------------------
+0x00   4     uint32   entity_key       Entity key (matches Name COMP keys)
+0x04   4     float32  position_x       World X coordinate
+0x08   4     float32  position_y       World Y coordinate (elevation)
+0x0C   4     float32  position_z       World Z coordinate
+0x10   4     float32  zero_pad         Always 0.0
+0x14   4     float32  quat_x           Quaternion X (pitch/roll component)
+0x18   4     float32  quat_y           Quaternion Y = sin(yaw/2)
+0x1C   4     float32  quat_z           Quaternion Z (pitch/roll component)
+0x20   4     float32  quat_w           Quaternion W = cos(yaw/2)
+0x24   4     uint32   parent           Template key (field `Parent`, 0x79450675); 0 = none
+0x28   2     bytes    flags            Read as flag bits by FUN_0063d7c0 (0x63d886) when [0x1176078] >= 0x2a
```

**`Parent`** (schm field 2, `0x79450675`, payload `+0x20`) is the entity's worldentity template. The
Transform reader `FUN_0063d7c0` copies it unchanged into the runtime SceneObject `+0x14`
(`0x63d84b` / `0x63d880`); component lookups fall back to it (`FUN_00649a80` `0x649afc–0x649b1b`)
and level load instantiates the template's children from it (`FUN_00654940` `0x655705` →
`FUN_00673070`). Verified 2026-10-21: across 63,312 records (layers_static + 25 other blocks) it is
0 (234) or a worldentity template key (63,078), nothing else; 54,502 placed Names equal their
template's Name; all 62,773 live SceneObjects of the mid-mission capture that share a key with those
blocks hold the same value at `+0x14`.

**The position is a world position, except on a link child** (2026-10-31; `mercs2_probe layer --frames
<capture>`, all four `updated-chris-exports` captures, ~76,000 of the 101,299 placed entities held in
each). An entity is a link child when a placed `PhysicalLink` of its layer names its key as `child`;
nothing in the record says so. `Parent` is the template only, and no flag bit marks the relation.

- Every other entity sits at its `Transform`: all 75,427 and 75,542 of them within 1 cm in the two
  captures where nothing had disturbed the world; the mid-mission two add 46 and 54 movable props
  (trash cans, dumpsters) that physics had shifted.
- A link child's `Transform` is not where it ends up. Its live position is parent × hard point ×
  offset up the chain of links, every one of the 679 to 882 children held in each capture within
  1 cm, positions only (rotation is untested). INFERRED: the solve overwrites the value the Transform
  reader stores, since the writer's body is a VM stub and no edited child has been run. The stored
  value is up to 2.8 km away and for some children is the hard point's local position (light
  `0x0010D532` stores (0, 5.97, 0); it sits on telephone pole `0x00102868`).
- The hard point counts once the parent has been awake. A child of a parent asleep since load sits at
  parent × offset. In the mid-mission captures 100 children of parents no longer awake still sit at
  the hard point's position (INFERRED: the node applied while the parent was awake and stays).
- 7 links name a child another link in the layer already named; the last record wins.

The child's own link is a `RuntimePhysicalLink` row (`0x6732C8`, `0x673389`), which also takes the
child out of the hibernation grid. The solve runs when the root's SceneObject changes
(`0x4FD400`, `0x4FD540`, job `0x4FD190`); the body it calls, `0x4FC470`, is a SecuROM VM stub.

**Rotation encoding:** The four floats at +0x14..+0x20 form a **unit quaternion** `(qx, qy, qz, qw)`.  VERIFIED: `qx² + qy² + qz² + qw² ≈ 1.0` across all 62k records.  For pure Y-axis rotation, `qy = sin(yaw/2)` and `qw = cos(yaw/2)`, so true yaw = `2 * atan2(qy, qw)`.  ~16% of entities have non-trivial qx/qz (tilted objects with pitch/roll).  The decoder exports all four components as `rotation_quat_x/y/z/w` and pre-computes `rotation_y_rad` (the full yaw angle).

**RE-VERIFIED 2026-07-21 — this section is correct, field for field.** Measured on
`game-files/vz.wad` block 29:

| Claim | Measurement |
|---|---|
| 42-byte stride, no header | 42 is the largest stride dividing all 166 Transform blobs (2,630,208 B → 62,624 records exactly) |
| `entity_key` @ +0x00 | 62,624 records → 62,624 **distinct** keys; each matches a `Name` record whose string ends in that same key as `0xHEX` |
| `zero_pad` @ +0x10 always 0.0 | **0** records out of 62,624 have a nonzero float there |
| unit quaternion @ +0x14 | **62,624 / 62,624** satisfy \|q\|² = 1 ± 1e-3. Reading the quaternion at +0x10 instead gives only 2,676 / 62,624 (4.3%), and a 56-byte stride (what `schm` declares, see §2.6) gives 33% at best — the layout above is the only one that fits |
| ~16% tilted | 9,963 / 62,624 = **15.9%** have \|qx\|>0.01 or \|qz\|>0.01 |

First record, raw: `85 6e 08 00 | 53 10 30 45 | fb 7b 39 41 | 58 56 ae 43 | 00 00 00 00 | 95 ef 12 b6
| bd 64 7a 3f | ac a4 a1 35 | ee 21 55 3e | a5 4d 00 80 16 04` — key `0x00086e85`
(= `_outskirt_wallchurchshort 0x00086e85` in the Name COMP), pos (2817.01, 11.593, 348.67), pad 0.0,
quat (−2.24e-4, 0.978, 1.2e-6, 0.208), 6-byte tail.

Repro: `cargo run --release -p mercs2_probe --bin docaudit -- layers`.

### 2.6 COMP Record Format (generic)

All COMP `data` blobs in `layers_static` follow the same pattern: fixed-stride records starting with a u32 entity key. The stride is determined from the `schm` child:

- `schm[0:4]` u32 = n_fields
- `schm[4:8]` u32 = **payload_stride** (bytes of payload after the entity key)
- Total record stride = **4 + payload_stride**

The `schm` payload_stride is verified to match across all non-Transform COMPs. Transform is an exception: its schm reports 52 but the actual payload is 38.

**RE-VERIFIED 2026-07-21, with one addition: `Name` is a second exception.** Testing *exact*
divisibility (`data_size % (4 + schm_stride) == 0`) over all 722 COMPs in layers_static, every blob
that is not a `Transform` or a `Name` divides exactly; the only failures are those two types.
`Transform` declares 52 (→ 56) but the real stride is 42 (payload 38, §2.5). `Name` declares 5 (→ 9)
but its records are **variable-length** C-strings (§2.7), so no fixed stride applies at all — the
declared 5 is a schema artefact, not a size. Spot checks of the rule where it does hold:
`HibernationControl` 26,250 B / (4+6) = 2,625 exactly; `LowResTerrainObject` 4,800 / (4+8) = 400
exactly; `TerrainObject` 3,200 / (4+4) = 400 exactly.

Repro: `cargo run --release -p mercs2_probe --bin docaudit -- part2 <vz.wad> layers`.

**Over every layer of `vz.wad` and `vz-patch.wad` there is a third exception, `PointLocation`**
(2026-10-31): `[u32 key][32 bytes][cstring]`, the worldentity layout (native reader `FUN_00656AD0`), in
18 layers. Its `schm` declares 36, and a blob's size says nothing about its records.

**`PhysicalLink`** (2026-10-31; `mercs2_formats::physical_link`): `[u32 key]` then 44 bytes, `+0` child,
`+4` `HardPoint`, `+8` `NAME`, `+0x0C` the 32-byte `Position` blob: x, y, z, a padding word, quaternion
xyzw. The template records' quaternions are all unit, which pins the layout. Of 1,687 template links
with a hard point on a parent with a model, 1,670 name a HIER node of that model; the other 17 are
unexplained. The 901 placed records resolve with no remainder: 891 children are entities of the same
layer, 10 are templates (`mercs2_probe layer --all`). A placed child's `Transform` is not its position
(§2.5). The `Position` padding word is a version counter (`max(a, b) + 1`, `0x673282`, `0x665AF0`).

**Positions held outside `Transform`:**

| Class | Stored | Frame | Grade |
|---|---|---|---|
| `PhysicalLink` | `Position` | parent × hard point | PROVEN (captures) |
| `LineRegion` | hash of a `lineregion` asset: `u16 2, u16 n, u32 0, n × (x, z)` | local XZ; `FUN_0045E970` adds the entity's X and Z once, with no rotation; 168 of 171 polygon centres in block 29 sit within 25 m of the origin | PROVEN (decomp + census) |
| `Path` | hash of a `path` asset: `u16 2, u16 1, u32 n, n × vec3` | through the path entity's transform (`FUN_00824D90`, `0x5A641A`, `0x5A658D`), so it rotates with the entity, unlike `LineRegion` | PROVEN (decomp) |
| `SphereRegion`, `CircleRegion` | one float, the radius | centred on the entity; both are tested in 3D (`Event.Boundary`, `0x5F3760`) | PROVEN (decomp) |
| `PointLocation` | `Position` | identity in 27 of 27 sampled; consumer unread | INFERRED |
| `PopulationSimpleSpawner` | `Position`, `HardPoint` | identity in 29 of 29 sampled; consumer unread | INFERRED |
| `RoadIntersection`, `Road` | `hpPosition0..5`; `MinExtent`, `MaxExtent` | small values on entities thousands of metres out; consumer unread | INFERRED |

### 2.7 Name ↔ Transform pairing (layers_static)

Within each UCFX sub-block, the Name COMP and Transform COMP both use the same u32 entity keys. Entities are matched **by key** — Name records contain `[u32_le key][entity_name_string 0xKEY\x00][\x00 pad]` and Transform records contain `[u32_le key][38 bytes position/rotation]`. The decoder emits both `entity_id` (as `0xKEY` hex string) and `entity_name` (the bare name portion) on each placement record.

**CONFIRMED 2026-07-21, with one refinement.** Raw head of the first Name blob:
`85 6e 08 00` + `"_outskirt_wallchurchshort 0x00086e85"` + `00 00` — i.e. the u32 key and the hex id
embedded in the string are the *same value*, so the pairing rule is self-checking. Refinement: match
keys **globally across the whole block, not per sub-block** — a locator's `Name` and `Transform` can
sit in different UCFX sub-blocks (contract start locators like `PmcCon001_Start1` split this way).
Also, the trailing pad is a per-record flag byte that is `0x00` in most Name COMPs but `0x01` in the
locator/starter-name COMP (sub-block 33); consuming only a single NUL slips every later key by one
byte there. Both fixes are in `mercs2_formats::placement::parse_name_records`. With them, **62,143 of
62,624** placements resolve a name (481 unnamed) — see the §2.10 correction.

**Correction 2026-10-27: the flag is exactly ONE byte.** A record is `[u32 key][string][NUL][u8
flag]`, flag `0x00` or `0x01`, and the next key starts right after it. The parser had skipped every
`0x00`/`0x01` byte after a name, on the assumption that a key's low byte is never `0x00`/`0x01`;
retail disproves it — `vz_state_pmccon004` places key `0x00105200` right after a `0x00` flag, and
landing pad `12_chi_hq_lz_playerone` has key `0x000D7901` (the key [`data/live_registry_hashes.csv`](../crates/mercs2_workshop/data/live_registry_hashes.csv)
records for it live). The skip ate those low bytes, read wrong keys and dropped the names. The
one-byte layout tiles every Name COMP of `vz.wad`'s 923 layers exactly
(`mercs2_formats/tests/placement_keys_retail.rs`, which also pins that none of the 101,299 keys of
those layers is in two of them). The 62,143-of-62,624 count above predates this correction.

### 2.9 LowResTerrainObject COMP — tile→mesh mapping

VERIFIED 2026-05-16 from `00029_blocks__VZ__layers_static_P000_Q3.block.bin`.

Sub-block 13 of `layers_static` contains the placement metadata for the 400
`lrterrain_rXX_cYY` entities that form Maracaibo's 20×20 low-resolution
terrain grid. Alongside the standard `Name` and `Transform` COMPs, it carries
a `LowResTerrainObject` COMP whose `data` payload is exactly **400 records of
12 bytes** in row-major grid order:

```
Offset  Size  Type     Field             Description
------  ----  -------  ----------------  -----------------------------------
+0x00   4     uint32   entity_key        Same key used by Name/Transform COMPs
+0x04   4     uint32   mesh_hash         Asset hash; matches TOC.hash1 in
                                          the low_res_terrain_P000_Q3 block
+0x08   4     uint32   scene_object_id   Sequential per-record id (opaque)
```

The records are authored in lrterrain row-major order: record `i` is the
mesh assignment for cell `(row=i//20, col=i%20)`. The `entity_key` numeric
range overlaps with other entities in the same sub-block (other COMPs use
intermediate keys), so it must NOT be used as a positional offset — the
list index of the record IS the (row, col).

The `mesh_hash` is the unique mesh asset hash and matches the per-tile
`hash1` field in the TOC of `03121_blocks__VZ__low_res_terrain_P000_Q3.block.bin`
(see `format_reference.md`). ~~399 of 400 hashes match exactly; the remaining
record corresponds to the one unused TOC slot (one tile in the world is
referenced by a different mechanism — the missing cell can be filled by
the unique unused file index as a deterministic fallback).~~

**CORRECTION (2026-07-21): it is 400 of 400, and no fallback is needed.** The
`low_res_terrain_P000_Q3` block (block index 3121) declares **401** entries with 401 distinct
`name_hash` values; **every one of the 400** `LowResTerrainObject.mesh_hash` values is present in
that set — 0 misses. The asymmetry is on the *other* side: exactly one of the block's 401 entries is
never referenced by a placement record. Repro:
`cargo run --release -p mercs2_probe --bin docaudit -- lrterrain`.

**The row-major claim is now proven independently, by name.** Joining each record's `entity_key`
through the `Name` COMP: record `i` is named `lrterrain_r{i/20:02}_c{i%20:02}` for **398 of 400**
records (the other 2 have no Name record at all; 0 contradictions). The transposed reading
(`r=i%20, c=i/20`) matches only 19/400 — the diagonal. Samples: `i=0 → lrterrain_r00_c00`,
`i=19 → lrterrain_r00_c19`, `i=20 → lrterrain_r01_c00`, `i=22 → lrterrain_r01_c02`.

Minor amendment to the field table above: `scene_object_id` is **strictly increasing but not
contiguous** — 398 of 399 consecutive deltas are `+1`, with one large jump; the values run
261,331 → 476,981. "Sequential" is right locally, not as a dense run.
Repro: `cargo run --release -p mercs2_probe --bin docaudit -- part5`.

Decoded by [`tools/terrain_extractor.py:_read_lrterrain_object_records`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/terrain_extractor.py) and
used to drive the `lrterrain_rXX_cYY` → mesh-file-index mapping for terrain
extraction. Recovers the correct world layout for all 400 tiles without
seam-matching. Note that 11% of inter-tile edges align to <5m under this
mapping (with several at <0.05m, confirming exact correctness on those
pairs); the remainder show up to ~400m residual mismatch because each tile
is authored in its own bbox-centred local frame and the raw mesh boundaries
were not designed to be C0-continuous (the original engine likely stitches
them at runtime via skirts or material blending).

### 2.10 Verified Statistics

*Re-measured 2026-07-21 — `docaudit -- layers` / `-- part2`. Struck figures did not reproduce.*

| Statistic | Doc (2026-05) | Measured 2026-07-21 | Verdict |
|---|---|---|---|
| Transform placement records | 62,624 | **62,624** | confirmed |
| Entity names matched by key | ~~60,136~~ | **62,143** (481 unnamed) | superseded — the two §2.7 parser fixes (global key match + flag-byte skip) recover 2,007 more |
| UCFX sub-blocks | 173 | **173** | confirmed |
| COMPs per sub-block | ~~2–43~~ | **2–14**, and only **166 of 173** sub-blocks carry any COMP at all | WRONG — "43" is the number of distinct COMP *type names*, not a per-sub-block count |
| Distinct COMP type names | 43 | **43** | confirmed |
| ECS records (non-Transform/Name) | ~~10,030~~ | **14,335** | superseded |
| Placements with ≥1 ECS component | ~~7,984 (12.7%)~~ | **10,421 (16.6%)** | superseded |
| Position range | X −3888..3800, Y −103..393, Z −3869..3800 | X **−3887.5..3800.0**, Y **−102.8..392.6**, Z **−3868.9..3800.0** | confirmed |
| Total COMPs in the block | — | 722 | new |

The three superseded record counts all move the *same direction* (more records found), consistent
with the older Python decoder dropping COMPs it could not stride-resolve.

---

## 3. vz_state Format

> ### ⚠ RETRACTED IN BULK — CORRECTION (2026-07-21)
>
> **vz_state is not a different format. It is layers_static.** Every vz_state block is one UCFX
> sub-block carrying the same `CHDR` → `COMP{info,schm,data}` tree, with the same **relative** child
> offsets, and its placements live in a `Transform` COMP holding the same **42-byte records with the
> same field layout as §2.5** — u32 key, XYZ, zero pad, unit quaternion, 6-byte tail.
>
> `mercs2_formats::placement::load_placements` parses layers_static and vz_state with one unmodified
> code path; on `vz_state_pmccon004` it returns 361 named, positioned entities
> (`location`, `M113 (VZ)`, `VZ Soldier`, `Health Pickup`, …).
>
> §3.1, §3.2, §3.3, §3.6, §6.2 and the vz_state column of §7 are wrong and are struck below.
> The original text is preserved — the misreading is itself the finding.
>
> Repro: `cargo run --release -p mercs2_probe --bin docaudit -- vzstate`
> and `-- part2 <vz.wad> vz_state`.

### 3.1 ~~File Header (20 bytes)~~ → the standard block entry table

~~```
Offset  Size  Type      Field              Verified Values
------  ----  --------  -----------------  --------------------------------
0x00    4     uint32    version            Always 1
0x04    4     uint32    asset_hash         Varies per file
0x08    4     uint32    format_hash        Always 0xe6b81a54
0x0C    4     uint32    reserved           Always 0
0x10    4     uint32    total_payload_size File size minus 20
```~~

**CORRECTION (2026-07-21).** There is no vz_state-specific header. These 20 bytes are the ordinary
block entry table (see the §2.2 correction): `u32 entry_count`, then one 16-byte row. The field
labels are wrong even though the *values* looked right:

| Doc label | Reality |
|---|---|
| `version` "Always 1" | `entry_count`. It is 1 only because a vz_state block holds exactly one UCFX container; the *same* field is **173** in layers_static |
| `asset_hash` | `name_hash` of that single entry — correct in substance |
| `format_hash` 0xe6b81a54 | `type_hash` = `pandemic_hash_m2("layer")` — correct in substance |
| `reserved` | `field_c`, 0 in every layer block observed |
| `total_payload_size` "file size minus 20" | the entry's `chunk_size`. Numerically it *is* size−20 (pmccon004: 49,694 = 49,714 − 20), because 20 B is exactly `4 + 1*16` |

Head of `vz_state_pmccon004_P000_Q3`: `01 00 00 00 | 42 e6 55 00 | 54 1a b8 e6 | 00 00 00 00 |
1e c2 00 00 | 55 43 46 58 ("UCFX")`.

### 3.2 Chunk Table Structure

Same UCFX/CHDR/COMP hierarchy as layers_static, but:

~~**CRITICAL: In vz_state, COMP child offsets are ABSOLUTE file offsets (not relative).**~~

**CORRECTION (2026-07-21): they are RELATIVE, exactly as in layers_static.** Measured by parsing all
746 vz_state blocks with the *relative* decoder (`comp_inventory`, which adds `data_area_start`): it
recovers clean ASCII `info` type names (`PopulationSimpleSpawner`, `AiPatrol`, `LineRegion`, …) and
`data` blobs whose sizes divide exactly by `4 + schm_stride`. The two readings are not confusable —
`data_area_start` in pmccon004 is ~1.3 KB, so an absolute reading would land the `info` children
1.3 KB early and produce garbage. There is no ABSOLUTE/RELATIVE split between the two file kinds.

The `flgs` and `flgt` chunk table entries use a non-COMP format:

```
Offset  Size  Type    Field
------  ----  ------  -----
+0x00   4     char[4] "flgs"
+0x04   4     uint32  data_offset (RELATIVE to data_area_start, same as COMP children)
+0x08   4     uint32  data_size
+0x0C   4     uint32  zero
+0x10   4     uint32  zero
```

*Row shape confirmed 2026-07-21 (`flgt` payload is 4 B in every block sampled; `flgs` payload is
4–3,496 B). `data_offset` corrected from ABSOLUTE to RELATIVE. Note `+0x0C`/`+0x10` are **not** both
zero — `+0x10` is the child count (0 for `flgs`/`flgt`/`enum`, 3 for `COMP`), which is what makes the
CHDR row walk terminate correctly.*

### 3.3 ~~flgs Placement Record Format (42 bytes)~~ — a 28-byte-shifted window over §2.5

~~Records begin after a variable-length header in the flgs data section. The header end is located by finding the first `0x3f800000` (1.0f) and subtracting 4 bytes.~~

~~```
Offset  Size  Type      Field            Description
------  ----  --------  ---------------  --------------------------------
+0x00   4     uint32    state_flags      Usually 0x00000000
+0x04   4     float32   boot_float       **UNVERIFIED as scale** — also interpretable as uint32 (`boot_u32`); do not use as mesh uniform scale in UE until re-validated
+0x08   4     uint32    type_hash        Component type hash
+0x0C   2     uint16    extra_flags      Varies
+0x0E   4     uint32    entity_id        Maps to named entities in COMP data
+0x12   4     float32   position_x       World X coordinate
+0x16   4     float32   position_y       World Y coordinate (elevation)
+0x1A   4     float32   position_z       World Z coordinate
+0x1E   4     float32   rotation_0       Rotation component (usually 0)
+0x22   4     float32   rotation_1       Often -0.0 or 0.0
+0x26   4     float32   rotation_y       sin(yaw) for Y-axis rotation
```~~

~~Total: 42 bytes (0x2A). **Stride VERIFIED** via recurring 1.0f pattern at offset +4.~~

**CORRECTION (2026-07-21) — this whole table is one record boundary out of phase.**

The records were never in `flgs`. They are the `Transform` COMP's 42-byte records (§2.5). The
"first 1.0f minus 4" heuristic lands at a Transform-blob offset of **exactly 28 mod 42** in every
block tested — measured 3/3 (raw offsets 70, 28, 28 in `chijob005_a_staging`, `pmccon004`,
`mar_altagracia_act1`). The recurring 1.0f the heuristic locks onto is `quat_w = cos(yaw/2)` of the
*previous* record, which is 1.0 for every unrotated entity. Field by field, doc offset → real offset
`(28 + doc) mod 42`:

| Doc field | Real field |
|---|---|
| `state_flags` +0x00 | `quat_z` of the **previous** record |
| `boot_float` +0x04 | `quat_w` of the previous record — this is the "recurring 1.0f", and it is **not** a scale |
| `type_hash` +0x08, `extra_flags` +0x0C | the previous record's 6-byte tail |
| `entity_id` +0x0E | ✅ `entity_key` +0x00 — the one field that was right, by accident |
| `position_x/y/z` +0x12/+0x16/+0x1A | ✅ `position_x/y/z` +0x04/+0x08/+0x0C — right, which is why the extracted coordinates looked sane |
| `rotation_0` +0x1E | the always-zero pad at +0x10 (hence "usually 0") |
| `rotation_1` +0x22 | `quat_x` (hence "often ±0.0") |
| `rotation_y` "sin(yaw)" +0x26 | `quat_y` = **sin(yaw/2)**, not sin(yaw) — every yaw recovered from this field was wrong by the half-angle |

The 42-byte stride was correct for the right reason after all: it is the real Transform stride. Use
§2.5 verbatim for vz_state.

### 3.4 Entity Name Cross-Reference

Entity IDs in flgs records map to named entities defined in COMP `data` sections. Names follow the format `"EntityName 0xHEXID"`.

VERIFIED examples from pmccon004:

| Entity ID | Name |
|-----------|------|
| 0x0012b37b | Light_large_whiteblue_cumana |
| 0x0012d4f1 | VZ Soldier |
| 0x001313f9 | _global_fencechainlong |
| 0x00131407 | _global_flag_smallVZ |
| 0x0013140d | _global_fencechaingate |

**CONFIRMED 2026-07-21 — all five, name for name, in `vz_state_pmccon004_P000_Q3`.** Resolved by
key-joining `Transform` → `Name` with the Rust loader; every id lands in that block with exactly the
listed name. This is also independent corroboration of the §3.3 phase analysis: the old decoder's
`entity_id` field really was reading the record's `entity_key`, just from 14 bytes into a window that
started 28 bytes early. Repro: `cargo run --release -p mercs2_probe --bin docaudit -- part6`.

### 3.5 State Overlay System

vz_state filenames encode game state:

| Suffix | Meaning |
|--------|---------|
| `_pristine` | Original/undamaged state |
| `_destroyed` | Post-destruction |
| `_staging` | Pre-mission enemy placement |
| `_defenses` | Defensive fortifications |
| `_captured` | Player-captured location |
| Faction: `chi`, `pir`, `gur`, `oil`, `all`, `pmc` | Chinese, Pirate, Guerrilla, UP, Allied, PMC |

### 3.6 Verified Statistics

*Re-measured 2026-07-21 — `docaudit -- vzstate` decompresses all 746 blocks and counts.*

| Statistic | Doc (2026-05) | Measured | Verdict |
|---|---|---|---|
| vz_state files | 746 | **746** | confirmed |
| Files with XYZ placements | ~~320 (43%)~~ | **744 (99.7%)** carry a `Transform` COMP | WRONG |
| Total entity placements | ~~~3,620~~ | **37,867** | WRONG — off by >10× |
| Placements in pmccon004 | ~~53~~ | **361** | WRONG |
| Decompressed size range | 11 KB–120 KB | **11,012–119,340 B** (11.99 MB total) | confirmed |
| Distinct COMP types across vz_state | — | 40+, led by `Name` 744 / `Transform` 744 / `ModelName` 333 / `HibernationControl` 278 | new |

Every shortfall traces to §3.3: the 1.0f heuristic scanned a `flgs` chunk that holds no placements,
then stopped at the first record that failed its coordinate-range filter.

*§3.5's overlay taxonomy is corroborated: across the 746 filenames, `_pristine` 135, `_staging` 128,
`_defenses` 84, `_destroyed` 56, `_captured` 19; faction tokens `chi` 147, `oil` 130, `gur` 124,
`all` 119, `pir` 100, `pmc` 63, `vza` 9.*

---

## 4. c3XXXX Cell Blocks — NOT Placement Data

### 4.1 Summary

9,467 c3XXXX blocks were analyzed. They contain **NO placement data**.

### 4.2 Three Sub-types

| Format Hash | Count | Content | Contains Placement? |
|------------|-------|---------|-------------------|
| `0xf011157a` | ~~~9,400~~ | Compressed texture/terrain data (BODY chunks) | **NO** |
| `0x5b724250` | ~~~60~~ | Mesh geometry (HIER, MTRL, SEGM, GEOM, MESH, INDX + Havok physics) | **NO** |
| `0x600b904e` | ~~~7~~ | ~~Tiny checksum wrappers (48 bytes, CSUM only)~~ | **NO** |

**CORRECTION (2026-07-21) — the counts and the third description are wrong; the "NO" column is
right.** c3 blocks are *mixed*, so a three-way partition by "format hash" was never going to work.
Measured by classifying each of the 9,467 c3 blocks by the **set** of `type_hash` values in its entry
table:

| Entry-type set | Blocks |
|---|---|
| texture only | 7,209 |
| model + texture | 1,138 |
| model only | 691 |
| 0x600B904E + terrainmesh + texture | 245 |
| terrainmesh + texture | 146 |
| all others (12 further combinations) | 38 |

So **1,849** c3 blocks contain model geometry (not ~60) and **261** contain `0x600B904E` (not ~7).
`0x600B904E` is also not a "tiny checksum wrapper": it is a real resource type with **1,026** entries
WAD-wide (shader/material resources — see [`docs/type_hash_registry.md`](type_hash_registry.md)). By entry count inside c3
blocks: texture 31,126, model 3,877, 0x600B904E 1,026, terrainmesh 400, animation 13.

### 4.3 Evidence

- No COMP, flgs, flgt, or enum tags in any c3 block — **re-tested 2026-07-21**: 48 c3 blocks sampled
  at stride 200, fully decompressed, byte-scanned for `COMP`/`flgs`/`enum` → **0 hits**. Independently,
  of 55,429 UCFX entries WAD-wide, **0** layer-type (`0xE6B81A54`) entries live in a c3 block.
- No (0,0,0,1) float patterns in BODY-type blocks *(not re-tested)*
- Mesh blocks contain vertex/face/material data but no world transforms *(not re-tested)*
- The spatial grid naming (c3XXXX) refers to streaming cells for geometry LOD, not entity placement

Repro: `cargo run --release -p mercs2_probe --bin docaudit -- part3` and `-- census`.

---

## 5. Coordinate System

VERIFIED from extracted positions across both layers_static and vz_state:

```
X axis:  -3900 to +3800  (East-West)
Y axis:  -103  to +393   (Elevation — negative = underwater)
Z axis:  -3870 to +3800  (North-South)
```

Units are **meters** (verified: Parque Central towers = 220 game units ≈ 225 m real-world height).

*Audit note (2026-07-21): the numeric extents above are **confirmed** — measured over all 62,624
layers_static Transform records, X −3887.5..3800.0, Y −102.8..392.6, Z −3868.9..3800.0. The
**metres** claim is **unverifiable from WAD data**: it rests on an external real-world building
height, which no probe here can check. Treat the unit as inferred, not measured.*

### 5.1 Handedness and transforms (positions + rotations)

> *Audit note (2026-07-21): everything in §5.1 concerns the game→glTF→UE export pipeline and the
> landmark-matching that validated it. None of it is checkable from `vz.wad` alone, so it is left
> **unverified** by this pass — neither confirmed nor disputed. The raw-data facts it builds on (the
> §2.5 quaternion layout and the position extents) are confirmed above.*

**Position transforms:**

| Path | Transform | Notes |
|------|-----------|-------|
| Mesh export (game → glTF) | `(x, y, z)` written directly | No Z-negate, no winding flip; only UV V-flip |
| UE Interchange importer (glTF → UE) | Y-up → Z-up (swaps Y↔Z) | Automatic |
| Placement (game → UE) | `(x, y, z)` → `(100·x, 100·z, 100·y)` | Same result as mesh import path |

- Game geometry is **left-handed Y-up** (D3D9).
- [`tools/gltf_exporter.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/gltf_exporter.py) writes game coordinates directly into the glTF buffer.  The only transform applied is the **UV V-flip** (`v = 1 - v`, D3D9 V=0-top → glTF V=0-bottom) via `convert_uvs_d3d_to_gltf`.
- UE Interchange's Y-up→Z-up swap produces UE coordinates equivalent to `game_to_ue`.
- Placement JSON and `game_to_ue` in populate scripts stay in **game LH** metres with `(x, y, z) → UE (100·x, 100·z, 100·y)`.

**Rotation transforms:**

The 42-byte Transform record stores a **unit quaternion** `(qx, qy, qz, qw)` at offsets +0x14..+0x20.  For pure Y-axis rotation: `qy = sin(yaw/2)`, `qw = cos(yaw/2)`.  True yaw = `2 * atan2(qy, qw)`.

VERIFIED: `qx² + qy² + qz² + qw² ≈ 1.0` across all 62k records.  ~16% of records have non-trivial pitch/roll (|qx| > 0.01 or |qz| > 0.01).

| Source | Transform | Destination |
|--------|-----------|-------------|
| Game quaternion (qx,qy,qz,qw) | `game_quat_to_ue_rotator_deg(qx,qy,qz,qw)` | UE Rotator (pitch, -yaw, roll) |
| Game yaw (rad, from `2*atan2(qy,qw)`) | `game_yaw_to_ue_yaw_deg(rad)` → -degrees | UE yaw (around +Z) |

- Both game (LH Y-up) and UE (LH Z-up) are left-handed. The basis swap `(x,y,z)→(x,z,y)` preserves handedness.
- **Yaw is NEGATED**: game positive yaw (CW looking down +Y) is opposite to UE positive yaw.  UE yaw = -game_yaw.  Empirically verified against PMC HQ, pool, and estate wall landmarks.
- The quaternion basis swap is: game `(qx, qy, qz, qw)` → UE `(qx, qz, qy, qw)` (swap Y/Z components).
- Full pitch/roll is preserved in the quaternion path for tilted entities (tires, rocks, poles).
- Implementation: [`tools/mercs2_coords.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/mercs2_coords.py) → `game_yaw_to_ue_yaw_deg()`, `game_quat_to_ue_rotator_deg()`.

### 5.2 vz_state Data Layers

Overlay taxonomy and UE Data Layer labels: [`game-scripts/mercs2_vz_taxonomy.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/game-scripts/mercs2_vz_taxonomy.py).  
Manifest: `output/placements/vz_act_layer_manifest.json` (from [`tools/build_vz_act_manifest.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/build_vz_act_manifest.py)).

---

## 6. Rotation Encoding

### 6.1 layers_static

Stores a **unit quaternion** `(qx, qy, qz, qw)` at offsets +0x14, +0x18, +0x1C, +0x20:
- For pure Y-axis rotation: `qy = sin(yaw/2)`, `qw = cos(yaw/2)`
- True yaw = `2 * atan2(qy, qw)`
- Identity rotation: qx=0, qy=0, qz=0, qw=1

VERIFIED: `qx² + qy² + qz² + qw² ≈ 1.0` across all 62,624 extracted records.
~16% of records have non-trivial pitch/roll (tilted objects: tires, rocks, telephone poles).

### 6.2 vz_state

~~Uses a **single sin(yaw) value** at record offset +0x26:~~
- ~~Values range from -1.0 to 1.0~~
- ~~Full quaternion reconstruction requires `cos(yaw) = sqrt(1 - sin²(yaw))` (sign ambiguous)~~
- ~~Most records have (0, 0, 0) rotation = identity~~

**CORRECTION (2026-07-21): vz_state stores the SAME full unit quaternion as §6.1.** There is no
sign ambiguity and no reconstruction step — vz_state placements are `Transform` COMP records
identical in layout to layers_static (see the §3.3 correction). The float the old decoder called
"sin(yaw)" was `quat_y` = **sin(yaw/2)**; every yaw derived from it was wrong by the half-angle,
and its neighbouring `quat_w` was being read 28 bytes earlier as "boot_float". Use
`yaw = 2*atan2(qy, qw)`, as in §6.1.

---

## 7. Differences: layers_static vs vz_state

**CORRECTION (2026-07-21): four of these seven rows were wrong.** Corrected table:

| Property | layers_static | vz_state | changed? |
|----------|--------------|----------|---|
| File count | 1 composite file | 746 individual files | — |
| COMP child offsets | **Relative** to data_area_start | **Relative** ~~Absolute file offsets~~ | ✔ fixed |
| Transform record start | Offset 0 (u32 entity key) | **Offset 0** ~~Variable (1.0f search heuristic)~~ | ✔ fixed |
| Rotation encoding | Unit quaternion (4 floats: qx, qy, qz, qw) | **Unit quaternion** ~~sin(yaw) only~~ | ✔ fixed |
| Record layout | u32 key + XYZ + pad + quat(4) + tail(6) | **identical** ~~uint32×2 + uint32 + uint16 + …~~ | ✔ fixed |
| Entity ID in record | u32 at offset 0 of each Transform record | **offset 0** ~~offset +14~~ | ✔ fixed |
| Purpose | Always-loaded base world | Conditional state overlays | — |
| Block entry count | 173 (one per UCFX sub-block) | 1 | new |

The only real differences are the entry count and the purpose. One parser serves both.

~~Original (wrong) table:~~

| Property | layers_static | vz_state |
|----------|--------------|----------|
| File count | 1 composite file | 746 individual files |
| COMP child offsets | **Relative** to data_area_start | ~~**Absolute** file offsets~~ |
| Transform record start | Offset 0 (u32 entity key) | ~~Variable (1.0f search heuristic)~~ |
| Rotation encoding | Unit quaternion (4 floats: qx, qy, qz, qw) | ~~sin(yaw) only (1 float)~~ |
| Record layout | u32 key + XYZ + pad + quat(4) + tail(6) | ~~uint32×2 + uint32 + uint16 + uint32 + XYZ + rot(3)~~ |
| Entity ID in record | u32 at offset 0 of each Transform record | ~~At record offset +14~~ |
| Purpose | Always-loaded base world | Conditional state overlays |

---

## 8. Using the Decoder Tool

> **NOTE (2026-07-21): [`tools/placement_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/placement_extractor.py) carries the §3 vz_state bug** (the 28-byte
> misaligned flgs window) and undercounts vz_state placements ~10×. For correct results use the Rust
> loader `mercs2_formats::placement::load_placements`, which parses layers_static and vz_state with
> one code path. To reproduce every number in this document:
> `cargo run --release -p mercs2_probe --bin docaudit -- {layers|vzstate|part2|part3|part5|part6}`
> (source: [`tools/wad_simulator/crates/mercs2_probe/src/bin/docaudit.rs`](../crates/mercs2_probe/src/bin/docaudit.rs)).

```bash
# Single file extraction  (LEGACY — layers_static only is trustworthy; vz_state is wrong)
python3 tools/placement_extractor.py path/to/block.bin -o placements.json

# Stats only (no placement data in output)
python3 tools/placement_extractor.py path/to/block.bin --stats-only

# Batch all vz_state files
python3 tools/placement_extractor.py --batch path/to/blocks/ --filter vz_state -o all_vz.json

# Extract layers_static only
python3 tools/placement_extractor.py path/to/blocks/00029_blocks__VZ__layers_static_P000_Q3.block.bin -o static.json
```

---

## 9. Known Limitations

*Audit pass 2026-07-21: limitations 2–7 were all downstream of the §3.3 misalignment. Five of the
seven are now closed.*

1. **Entity name → Transform cross-referencing in layers_static**: **RESOLVED (2026-05-15).** All COMP data blobs use the same u32 entity key. Name and Transform records are matched by key (not by index). ~~60,136 of 62,624~~ **62,143 of 62,624** placements have both `entity_id` and `entity_name`. The remaining ~~~2,488~~ **481** have `entity_id` but no `entity_name`. *(Updated 2026-07-21: matching keys globally rather than per sub-block, plus skipping the per-record `0x01` flag byte in the locator Name COMP, recovers 2,007 additional names.)*

2. ~~**vz_state offset +4**: Previously mislabeled as `scale` in JSON output; exported as `boot_float` / `boot_u32`. **Do not** use as UE actor scale until the field semantics are verified.~~
   → **CLOSED (2026-07-21).** The field is `quat_w = cos(yaw/2)` of the preceding Transform record. It is not a scale, and the caution was correct for the wrong reason. Nothing in vz_state encodes a scale.

3. ~~**vz_state variable-length header**: The "first 1.0f minus 4" heuristic for finding record starts works for ~90% of files but may fail for zone/region format files.~~
   → **CLOSED (2026-07-21).** There is no variable-length header, and the heuristic is the bug: it lands 28 bytes into the preceding record, in **every** block tested. Records start at offset 0 of the `Transform` COMP `data` blob, always.

4. ~~**Records at flgs tail**: Some records at the end of vz_state flgs sections contain component-table data (not placements) that passes coordinate range filters. Currently filtered by excluding (0,0,0) positions.~~
   → **CLOSED (2026-07-21).** An artefact of scanning `flgs` for records that were never there. With the `Transform` COMP the record count is exact (`data_size / 42`) and no filtering is needed.

5. ~~**Rotation sign ambiguity**: vz_state records encode rotation as a single `sin(yaw)` value. Recovering the full angle requires sign disambiguation of `cos(yaw)`.~~
   → **CLOSED (2026-07-21).** vz_state carries the full unit quaternion; `yaw = 2*atan2(qy, qw)` is unambiguous. See §6.2.

6. ~~**UNVERIFIED: layers_static rec_type field**: The uint16 at record offset +0 (values 0x000E, 0x000F) may encode entity type or sub-block cross-reference.~~
   → **CLOSED (2026-07-21): it is the high half of the u32 entity key.** Keys are dense mid-range ids; a sub-block whose entities are numbered around `0x000Exxxx` shows `0x000E` in the u16 at record +2 (or +0 under the shifted §3.3 window). Not a type field. All 62,624 keys in layers_static are distinct, which a 2-value enum could not be.

7. **UNVERIFIED: type_hash mapping**: The `type_hash` field in vz_state records (values like `0x800084b3`, `0x80005422`) maps to COMP component indices, but the mapping table is file-specific and not fully decoded.
   → **Moot (2026-07-21):** there is no `type_hash` field in a placement record; doc offset +0x08 falls in the previous record's 6-byte tail. The tail is `Parent` (the template key, §2.5) plus 2 flag bytes.

---

## 10. Data Flow Summary

```
vz.wad (2.57 GB)
  └─ blocks/ (11,371 .bin files)
       ├─ layers_static (1 file, 7.97 MB)
       │    └─ 173 UCFX sub-blocks
       │         └─ COMP[Transform] → 42-byte keyed records → 62,624 placements
       │         └─ COMP[Name] → entity name ↔ key mappings → 62,143 matched  (was 60,136)
       │         └─ COMP[*other*] → per-entity ECS data → 14,335 records      (was 10,030)
       │
       ├─ vz_state (746 files, 11–120 KB each)
       │    └─ COMP[Name]      → entity name ↔ key mappings
       │    └─ COMP[Transform] → 42-byte records → 37,867 placements  (was wrongly "flgs → 3,501"; corrected 2026-07-21)
       │
       ├─ c3XXXX cells (9,467 files) — per-cell baked geometry (props/roads/buildings in-cell)
       │
       └─ Other (vz_base, vz_mar_roads, assets, etc.)
```

---

## 10. UE5 populate strategy (two-tier model)

[`game-scripts/populate_world.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/game-scripts/populate_world.py) uses three tiers — do **not** spawn one StaticMesh per
`layers_static` `entity_name` for world fill:

| Tier | Source | UE5 action |
|------|--------|------------|
| **B — c3 cells** | `c3####` / `c3####-c####-c####-__shared__` blocks with `mesh_scene.gltf` | One `StaticMeshActor` per imported cell at grid origin ([`tools/c3_cell_grid.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/c3_cell_grid.py)) |
| **A — hero blocks** | `*_veh_*`, `*_bld_*`, `pmcoutpost_*` GLBs | One actor per block at centroid of placement records whose names share the block prefix |
| **C — placements JSON** | `layers_static` + `vz_state` | Point lights (ECS `LightObject`), skip env props when Tier B is active, vz overlays on data layers |

Grid decode for Tier B: `c3####` IDs map linearly to a 100×100 grid over the world XZ
extent (see [`tools/c3_cell_grid.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/c3_cell_grid.py)). Cell meshes use `import_world.py` →
`/Game/Mercs2/Meshes/WorldCells` (accepts `mesh_scene.gltf` when `.glb` is absent).

PMC base uses `populate_pmc_base.py` + `import_pmc_base.py` for the outpost compound.
