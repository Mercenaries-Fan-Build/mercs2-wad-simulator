# schm Field Type Code Mapping

Reverse-engineered from DLC block 18 (`dlc01_dlccon004_roads`) ECS_NODE COMP groups.
Cross-referenced against documented component field layouts.

## Type Code → Width Mapping

The engine's per-field stream readers in `mercs2_unpacked.exe` dispatch on the code: `FUN_00656210`
(int) and `FUN_00656320` (float) read codes 1 and 2 as one byte, 3 and 4 as two, 5, 6 and 9 as a
4-byte `int`, and 7 as an `f32`; `FUN_00656720` (enum) resolves code 9 through `FUN_00655EA0`;
`FUN_00656610` reads code 10 as three dwords and `FUN_0065644A` code 11 as eight. Retail
`worldentity` check: `Health`'s code-7 field holds `100.0f`. See
[`worldentity_container_format.md`](worldentity_container_format.md) §3.2.

| Type Code | Width (bytes) | Swap Unit | Read as | `SchemaFieldType` |
|-----------|---------------|-----------|---------|-------------------|
| 1 | 1 | none | `char` | `Byte` |
| 2 | 1 | none | `char` | `U8` |
| 3 | 2 | u16 | `short` | `Short` |
| 4 | 2 | u16 | `short` | `U16` |
| 5 | 4 | u32 | `int` | `Int` |
| 6 | 4 | u32 | `int` (name hash or raw value) | `Hash` |
| 7 | 4 | f32 | `f32` | `F32` |
| 8 | inline string | none | `Name`'s string | `StringRef` |
| 9 | 4 | u32 | `int`, resolved as an enum-value hash by the enum reader | `Enum` |
| 10 | 12 | 3×f32 | three `f32` | `Vec3` |
| 11 | 32 | 8×u32 | eight dwords | `Blob32` |

What separates codes 1 from 2 and 3 from 4 is not established; the engine reads each pair the same.

## Offset Field Encoding

The `field_offset` u32 in schm entries is packed, and **which half holds the byte offset depends on
the container's endianness** (the word is byte-swapped between builds):
- **Retail PC (little-endian schm):** `byte_offset = offset_word & 0xFFFF` (**LOW** 16 bits);
  byte +14 is a bit field's start bit and byte +15 its width in bits inside the storage unit at
  `byte_offset` (0 = the field is the whole unit). Retail bit fields are on codes 1, 4 and 5.
- **Xbox / BE-converted (big-endian schm):** `byte_offset = offset_word >> 16` (**HIGH** 16 bits).

> ⚠ **CORRECTION (2026-07, Wave-0 E1).** Earlier revisions stated `>> 16` unconditionally. That was
> derived from **converted DLC data carrying a converter bug** (byte-offset left in the high 16).
> Verified against **real retail vz.wad LE bytes**, the retail rule is the LOW 16 bits — Transform
> 32,36,38…50 · HibernationControl 0,2,3,4,5,5 · Road 0,4,8,12,16,28 · RoadIntersection 0,4…120.
> Authoritative endian-aware implementation: `mercs2_formats::schema::from_schm_body`. See also
> `spatial_hash_crash_analysis.md`.

## Byte-Swap Rules (derived)

For the converter, swap decisions are:
- **Types 1, 2**: NO swap (1 byte)
- **Types 3, 4**: swap 2 bytes (u16)
- **Types 5, 6, 7, 8, 9**: swap 4 bytes (u32/f32)
- **Type 10**: swap 4 bytes × 3 (Vec3 = three f32s)
- **Type 11**: swap 4 bytes × 8 (8 f32s — pos+pad+quat blob)

## Validation

| Component | Stride | Type codes | Sum | Match? |
|-----------|--------|-----------|-----|--------|
| ModelName | 4 | 1×type6(4) | 4 | ✓ |
| DestructionLink | 16 | 4×type6/7/9(4) | 16 | ✓ |
| Road | 40 | 4×type6(4) + 2×type10(12) | 16+24=40 | ✓ |
| RoadIntersection | 124 | 7×type6(4) + 6×type10(12) + 6×type6(4) | 28+72+24=124 | ✓ |
| HibernationControl | 6 | type4(2) + 3×type2(1) + two 1-bit fields of one type1 byte | 6 | ✓ |
| Transform | 52 (schm) | type11(32) + type5(4) + 8×type4(2) | 32+4+16=52 | ✓ (schm stride; runtime is 38) |

## DLC Block schm Presence

**All 7 COMP groups in DLC block 18 have schm.** The "DLC blocks might lack schm" concern is NOT confirmed — DLC blocks DO have full schema data.

## Xbox 360 sges Format

- Magic: "segs" (reversed "sges")
- **Header: 32 bytes** (vs PC's 16 bytes)
- Decompressed size: BE u32 at +8
- Compressed data: starts at offset +32 (raw deflate, same algorithm as PC)
- FFCS header: same structure as PC but with BE u32s and reversed tags (SCFF, XDNI, ATAD, etc.)
- INDX entries: 12 bytes each (same as PC), all fields BE

## Pandemic Lua Flag

Not yet confirmed from data — no Lua bytecode found in first 20 Xbox blocks scanned.
Will investigate during Phase 3 script converter implementation using DLC-specific blocks.
