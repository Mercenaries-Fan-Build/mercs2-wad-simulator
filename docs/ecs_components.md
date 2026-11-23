# ECS `COMP` components — verified record format

**Date:** 2026-05-15 (updated)  
**Tools:** [`tools/ucfx_ecs_codec.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ucfx_ecs_codec.py), [`tools/ecs_metadata_extract.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/ecs_metadata_extract.py)  
**Output:** `output/placements/ecs_components.json`

## Record format (layers_static)

Every COMP `data` blob in a `layers_static` sub-UCFX is an array of
fixed-stride records.  Each record is:

```
[ u32_le entity_key ][ payload bytes … ]
```

The **entity key** is the same u32 that appears in the Name COMP and the
Transform COMP for the same entity.  All COMPs in a sub-block share the
same key space.

### CHDR header `{ u16; u16; u32 }` and the Transform stride gate

Each sub-UCFX opens with a **CHDR** chunk. The engine chunk dispatcher
`0x654940` reads the CHDR body as:

```
struct CHDR { u16 fieldA @ +0;  u16 stride @ +2;  u32 flags @ +4; }
```

Retail `layers_static` block 29 stores `00 00 38 00 02 00 00 00` in **all 173**
sub-blocks → `u16@+0 = 0`, `u16@+2 = 0x0038 (56)`, `flags = 2`. The `u16@+2`
value is written to process-global `[0x01176078]`; the Transform record builder
`0x0063D7C0` strides **42** only when that gate is `>= 0x2A`, otherwise **40**.

**Byte-swap rule (DLC port):** swap the CHDR header per-field — `u16@+0`,
`u16@+2`, then `u32@+4` — **not** as two `u32`s. A whole-`u32` swap transposes
the two `u16` fields, zeroing the stride gate (`u16@+2 = 0`), which makes the
engine stride 40, drifts every Transform record by 2 bytes, and crashes the
spatial hash on save-load (AV `0x0248BBE2`). Implemented in
`ucfx_be_to_le.py::_swap_chdr_header` and `convert.rs::swap_chdr_header_inplace`;
see [`docs/spatial_hash_crash_analysis.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/spatial_hash_crash_analysis.md) §2026-06-01b.

### Stride determination

The `schm` child of each COMP carries a binary schema descriptor:

| Offset | Type  | Field          |
|--------|-------|----------------|
| 0      | u32   | n_fields       |
| 4      | u32   | payload_stride |
| 8+     | 16×N  | field entries (type, name_hash, unk, field_offset) |

**Total record stride = 4 (entity key) + payload_stride.**

Exception: `Transform` — the schm reports payload_stride=52 but the
verified payload is 38 (stride 42).  All other COMPs match schm exactly
(validated across all 173 sub-blocks).

### Verified COMP types and strides

| COMP name | Payload stride | Fields | Notes |
|-----------|---------------|--------|-------|
| Name | variable | entity name string | `u32 key + null-terminated "name 0xKEY" + \x00 pad` |
| Transform | 38 (stride 42) | position XYZ + rotation | `u32 key + 3×f32 pos + rotation data` |
| ModelName | 4 (stride 8) | u32 model hash | Consistent across 13 sub-blocks |
| HibernationControl | 6 (stride 10) | u8+u8+u16+u16 | 48 sub-blocks, 1797 entities |
| ObjectScript | 8 (stride 12) | u32 script hash + u32 | 7 sub-blocks |
| DestructionLink | 16 (stride 20) | u32 ref key + u32 flags + more | 39 sub-blocks |
| LightObject | 52 (stride 56) | u32 + RGB floats + intensity + radius + more | 22 sub-blocks |
| Road | 40 (stride 44) | lane/intersection data | 29 sub-blocks |
| RoadIntersection | 124 (stride 128) | intersection geometry | 38 sub-blocks |

### Road payload (40 bytes, stride 44)

Schm-validated layout ([`docs/schm_type_codes.md`](schm_type_codes.md), DLC `dlc01_dlccon004_roads`):

| Offset | Size | Type | Field (tool name) |
|--------|------|------|-------------------|
| +0x00 | 4 | u32 | `road_ref_key_0` — likely intersection entity key at segment start |
| +0x04 | 4 | u32 | `road_ref_key_1` — likely intersection entity key at segment end |
| +0x08 | 4 | u32 | `road_lane_hash_0` |
| +0x0C | 4 | u32 | `road_lane_hash_1` |
| +0x10 | 12 | Vec3 | `road_endpoint_a` (world-space lane endpoint) |
| +0x1C | 12 | Vec3 | `road_endpoint_b` |

Decoded by `tools/ucfx_ecs_codec.decode_road_payload` and graphed by [`tools/road_graph_extractor.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/road_graph_extractor.py).

### RoadIntersection payload (124 bytes, stride 128)

| Offset | Size | Type | Field (tool name) |
|--------|------|------|-------------------|
| +0x00 | 28 | 7×u32 | `intersection_ref_keys` — connectivity / lane refs (semantics TBD) |
| +0x1C | 72 | 6×Vec3 | `intersection_vec3s` — approach or lane anchor points |
| +0x64 | 24 | 6×u32 | `intersection_tail_u32` — trailing hashes/flags (semantics TBD) |

Decoded by `decode_road_intersection_payload`. Graph **nodes** use placement Transform position;
Vec3 fields are exported as `lane_hints` only.

Related COMP types not yet harvested from `layers_static`: `IntersectionToIntersection` (stride 8),
`LaneData`, `LaneZeroDirection` — see [`docs/gameplay_data_ue5_mapping.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/gameplay_data_ue5_mapping.md) §6.
| ModifierKey | 8 (stride 12) | modifier reference | 23 sub-blocks |
| ScrubObject | 4 (stride 8) | scrub hash | 12 sub-blocks |
| LineRegion | 4 (stride 8) | region ref | 16 sub-blocks |
| MaterialMapping | 8 (stride 12) | material refs | 3 sub-blocks |
| PhysicalLink | 44 (stride 48) | physics link data | 16 sub-blocks |
| LandingZone | 272 (stride 276) | zone definition | 1 sub-block |
| Label | 4 (stride 8) | label hash | 22 sub-blocks |
| Anchor | 16 (stride 20) | anchor data | 3 sub-blocks |

### Extraction results (layers_static)

- **62,624** total placements with entity_id
- **10,030** ECS records extracted across 43 unique COMP types
- **7,984** placements (12.7%) received at least one ECS component

### COMP distribution

| COMP | Entities with it |
|------|-----------------|
| Road | 2,441 |
| HibernationControl | 1,797 |
| LightObject | 1,197 |
| ScrubObject | 1,033 |
| RoadIntersection | 883 |
| DestructionLink | 672 |
| ModifierKey | 537 |
| PhysicalLink | 465 |
| ModelName | 464 |
| ObjectScript | 103 |
| Label | 91 |
| LineRegion | 77 |
| MaterialMapping | 18 |
| DangerousBuilding | 14 |
| Anchor | 8 |
| StateMachine | 4 |
| PointLocation | 2 |
| LandingZone | 2 |
| BuildingDestruction | 1 |

## vz_state — different structure

vz_state CHDR COMP entries do **not** follow the per-component-type keyed
record pattern.  Instead, each COMP entry contains:

- **info**: a packed blob with component names, field name hashes, and
  metadata — the offset frequently points into the middle of a string
- **schm**: a string table with entity names and references
- **data**: mixed data (entity names, enum value tables, config data)

Placement data for vz_state entities comes from the `flgs` section, which
is already extracted by `placement_extractor.py`.  The COMP entries in
vz_state carry component *definitions* (VehicleWeakPoint, TreeLeaves, etc.)
and enum value tables, not per-entity keyed records.

Cross-referencing between vz_state placements and layers_static is done via
the shared `entity_id` field.

## Merge rules

`ucfx_ecs_codec.merge_ecs_into_placements`:

- **layers_static:** match by u32 entity key (from `entity_id` hex string on
  placements → `entity_key` int on ECS records).
- **vz_state:** no ECS records are currently extracted from vz_state COMP
  blobs (fundamentally different structure).

## RuntimeLayerId

Not found as an explicit ASCII string in `layers_static` or `vz_base` blobs.
May be hash-identified in `schm` tables without a plaintext `info` name.

## resident / worldentity META — compact `info` + "keyed-group" data

**Date:** 2026-06-01 · **Source:** DLC `blocks\dlc01\resident_P000_Q3.block`
(Xbox 360 BE) · **Tools:** `tools/ucfx_be_to_le.py`,
`tools/wad_simulator/crates/ucfx_byteswap/src/convert.rs`

The `resident` singleton (type_hash `0xE6B81A54` ECS_NODE, also `0x5647C35D`
worldentity / `0x140E8728` guidmap) packs hundreds of components in **compact**
`info` form. Two details broke the BE→LE port and are now handled:

### 1. Compact `info` discriminator

A compact `info` body is `[u32 comp_hash (BE)][12 bytes metadata]` (16 bytes).
The component hash can be **coincidentally printable**, so "first bytes printable
⇒ ASCII name" is wrong:

| comp_hash (BE) | printable bytes | actual meaning |
|----------------|-----------------|----------------|
| `0x4E2B6C54` | `N+lT` | compact hash (unknown component) |
| `0x69567E62` | `iV~b` | compact hash (unknown component) |

Resolve in order: **(a)** recognized hash → named; **(b)** candidate is a valid
C++-style identifier (`[A-Za-z_][A-Za-z0-9_]*`, len ≥ 2) → full-format name;
**(c)** otherwise → compact `__hash_0x…`. Real names (`Transform`, `ModelName`,
`PointLocation`, …) pass (b); hashes with `+`/`~` correctly fall to (c).

### 2. "Keyed-group" data layout (mixed u8/u32)

Some components store their `data` as a sequence of groups:

```
[ u32 count ][ count × record ][ u8 flag ]   (repeated until body end)
```

The per-group trailing **`u8` flag** is why these bodies are not u32-aligned; a
blanket u32 sweep corrupts everything past the first flag. The converter
(`_convert_keyed_group_records`) byte-reverses `count` and every 4-byte record
field, copies the `u8` flag verbatim, and **requires exact consumption** (raises
rather than emit a corrupt buffer).

| comp_hash | name | record size | notes |
|-----------|------|-------------|-------|
| `0x60B7ABE0` | **PointLocation** | 36 B (key + 8×u32/f32) | docs list 2 entities in `layers_static`; resident sample: 1 group, flag=0 |
| `0x2E2659F0` | *(name not in rainbow table)* | 4 B (entity-ref key) | resident sample: 26 groups, flag=1 — an entity-reference list |

### 3. ModelName variable records

Besides the fixed `(key, hash)` pairs (stride 8), `ModelName` also appears as
variable records `[u32 count][count × u32 keys][u32 model_hash]` (u32- but not
8-aligned). Every field is a u32, so the converter swaps the whole body as a
u32 array (alignment requirement relaxed from `% 8` to `% 4`).

With these three handlers the DLC resident block converts cleanly (933 UCFX
entries, **0 errors, 0 blind u32 fallbacks**). Verified by
`tools/test_ecs_comp_byteswap.py` (Python) and `convert::tests` (Rust).
