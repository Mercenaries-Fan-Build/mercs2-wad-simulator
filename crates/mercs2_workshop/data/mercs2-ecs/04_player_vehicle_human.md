# Mercenaries 2: World in Flames — ECS / Reflection Components
## Family 04: Player / Vehicle / Human Systems

Reverse-engineered from the Ghidra decompilation of the game EXE (PC x86 retail).
Hashes from `tools/pandemic_hash.py --m2`.

---

## How these classes are wired (verified)

Each reflection class is set up by a **descriptor builder** function (one per class,
e.g. `FUN_00642740` for HumanInventory). The builder writes a 0x50-byte descriptor and
ends with `_DAT_<name_addr> = s_<ClassName>_<straddr>;`.

Descriptor layout, relative to the **class name-string slot** (`name_addr`, the manifest
col-3 address). The **descriptor base** = `name_addr - 0x3c`:

| offset (from base) | field | example (HumanInventory, base 0x017bde48) |
|---|---|---|
| +0x00 | `CopyFromStream` vtable ptr (`&PTR_CopyFromStream_*`) | 0x00bc0780 |
| +0x04 | name-1 anchor (`NAME_M1`, used by template guard) | `DAT_017bde4c` |
| +0x1c | field/version anchor (`ANCHOR`, template change-guard) | `DAT_017bde64` |
| +0x24 | **struct stride / serialized size** (u16) | 0x1c |
| +0x28 | type tag (u16) = 8 | 8 |
| +0x2c | seed `0x9e3779b9` | — |
| +0x3c | name string `s_<ClassName>_*` (= `name_addr`) | s_HumanInventory_00bc52d8 |

The actual **field schema** lives in a separate **deserialize template** function
(empty `callers=[]`, in the `0x0065adXX`–`0x0066exXX` region). A template registers fields
**in order**:

- `FUN_00656210(intDefault)`  → **INT** field
- `FUN_00656320(floatHex)`    → **FLOAT** field (IEEE754 hex; `0x3f800000`=1.0, `0`=0.0; or a `DAT_` float const)
- `FUN_00656720(enumTable, enumDefault)` → **ENUM** field (table + default member string)
- `FUN_00656890(default)`     → **SHORT/BOOL** field
- `FUN_00656610(0)`           → **VECTOR3** field (3-float bundle)
- ends with `FUN_0064a600(param_1, …)` + change guard against the descriptor's ANCHOR.

The template is matched to its class by its `iVar1 = DAT_<ANCHOR>;` line equalling the
class descriptor's `base+0x1c`. **Two dialects exist:**

1. **`FUN_006562xx` field dialect** — full ordered scalar schema recoverable (Equipment,
   Camera*, Human*, GrappleParameters, HumanInventory, ModelMixerProfile, …).
2. **`CopyFromStream`-vtable dialect** — the `Runtime*` classes plus `Usable`. Their per-field
   decoder is a *data* `PTR_CopyFromStream_*` not decompiled in this dump; the ANCHOR appears
   only in the runtime *apply/commit* pass. For these we report the verified **stride** and the
   runtime semantics recoverable from the apply function (flags/bitmasks), and flag the field
   list as not enumerable here.

---

## Registry table (ALL 23 classes)

| Class | m2 hash | descriptor base | name-str addr | stride | CopyFromStream | builder FUN | template FUN | purpose |
|---|---|---|---|---|---|---|---|---|
| HumanAnimationSystem | 0x27a3c8a9 | 0x017bccc8 | 0x017bcd04 | 0x34 | 0x00bbea58 | FUN_0063fee0 | FUN_0065ade0 | human anim driver: 3 int handles + 10 float tuning params |
| HumanAnimationSet | 0xe8f41716 | 0x017bcd18 | 0x017bcd54 | 0x08 | 0x00bbeaa8 | FUN_0063ff90 | FUN_0065af90 | 2 int handles → an animation set |
| VehicleAnimationSet | 0x35e09a35 | 0x017bcd68 | 0x017bcda4 | 0x08 | 0x00bbeaf8 | FUN_00640040 | FUN_0065b030 | 2 int handles → a vehicle anim set |
| Equipment | 0xdab653e7 | 0x017bcdb8 | 0x017bcdf4 | 0x20 | 0x00bbeb48 | FUN_006400f0 | FUN_0065b0d0 | **loadout slot**: EquipmentTypeEnum + 6 int + 1 bool |
| CameraCarPreset | 0x5d1f87ef | 0x017bce08 | 0x017bce44 | 0x50 | 0x00bbed00 | FUN_006401b0 | FUN_0065e1d0 | car-camera preset: scalar gains + 3 vector3 offsets |
| CameraShake | 0x412d1576 | 0x017bce58 | 0x017bce94 | 0x10 | 0x00bbef30 | FUN_006402b0 | FUN_0065ea10 | shake type enum + amplitude/freq floats + int |
| HumanCameraModifier | 0x212ffcb2 | 0x017bcea8 | 0x017bcee4 | 0x38 | 0x00bbef80 | FUN_00640360 | FUN_0065eaf0 | 12 float camera weights + 2 behavior-category enums |
| VehicleDisguiseScale | 0x8b3a2b88 | 0x017bd5d8 | 0x017bd614 | 0x0c | 0x00bbf9f0 | FUN_006413f0 | FUN_0065c330 | 3 float disguise scale factors |
| SeatParameters | 0xa2d3ae72 | 0x017bd808 | 0x017bd844 | 0x14 | 0x00bbfd88 | FUN_006418f0 | *(none in dump)* | seat config (vtable-dialect; 20 B) |
| EntranceParameters | 0x70d05913 | 0x017bd858 | 0x017bd894 | 0x1c | 0x00bbfe50 | FUN_006419b0 | *(none in dump)* | entrance config (vtable-dialect; 28 B) |
| HumanInventory | 0xe672296c | 0x017bde48 | 0x017bde84 | 0x1c | 0x00bc0780 | FUN_00642740 | FUN_00661260 | **3 INT** (see below) |
| ModelMixerProfile | 0x1611c502 | 0x017be708 | 0x017be744 | 0x04 | 0x00bc1748 | FUN_00643a40 | FUN_00663700 | 1 INT (profile/blend index) |
| Carryable | 0x712af756 | 0x017be758 | 0x017be794 | 0x04 | 0x00bc1798 | FUN_00643af0 | FUN_00663810 | marker/tag (no scalar fields) |
| GrappleParameters | 0x6ac5ee26 | 0x017be848 | 0x017be884 | 0x1c | 0x00bc1a68 | FUN_00643d50 | FUN_00664ca0 | 1 BoolEnum + 6 float tuning |
| RuntimeHeadLookAt | 0x6b1666df | 0x017bf338 | 0x017bf374 | 0x2c | 0x00bc2980 | FUN_00645570 | *(none in dump)* | runtime head-look state (44 B) |
| RuntimeInventory | 0xa364fc7d | 0x017bf3d8 | 0x017bf414 | 0x30 | 0x00bc2b88 | FUN_00645720 | FUN_00667210 † | runtime inventory rebuild pass (no scalar schema) |
| RuntimeRiderDiveEnter | 0x8a15415f | 0x017bf4c8 | 0x017bf504 | 0x44 | 0x00bc2c78 | FUN_00645960 | *(vtable-dialect)* | rider "dive-enter" transition (68 B) |
| RuntimeRiderCrawlExit | 0xa7d4d8ca | 0x017bf518 | 0x017bf554 | 0x34 | 0x00bc2cc8 | FUN_00645a20 | FUN_0053b4b0 ‡ | rider crawl-out pose/transform (52 B) |
| RuntimeVehicleCrawlExits | 0x1fa43615 | 0x017bf568 | 0x017bf5a4 | 0x6c | 0x00bc2d18 | FUN_00645b00 | *(vtable-dialect)* | per-vehicle crawl-exit slot table (108 B) |
| RuntimeVehicleInventory | 0x9a6db283 | 0x017bf7e8 | 0x017bf824 | **0x02** | 0x00bc3010 | FUN_00646150 | FUN_0066ebf0 ‡ | **2-byte weapon-category bitmask** (see below) |
| SeatParameters → see above | | | | | | | | |
| Usable | 0xb3af2a59 | 0x017c01e8 | 0x017c0224 | 0x08 | 0x00bc3ea8 | FUN_00647a10 | *(vtable-dialect)* | generic "is-usable" interaction (8 B) |
| RuntimeEntrance | 0x55d8d2b1 | 0x017c0418 | 0x017c0454 | 0x18 | 0x00bc4378 | FUN_00647ef0 | FUN_005382xx ‡ | per-entity entrance-available flag (24 B) |
| RuntimeSeatPlayerUsable | 0xe5fb2b37 | 0x017c04b8 | 0x017c04f4 | **0x01** | 0x00bc4418 | FUN_00648070 | FUN_0066a9d0 ‡ | 1-byte bool: seat usable by player (bit-12 test) |

† `FUN_00667210` is a runtime rebuild/registration pass, **not** a scalar field template.
‡ a runtime *apply/commit* function, not a `FUN_006562xx` field template (vtable-dialect).
All descriptors share seed `0x9e3779b9` and `&PTR_FUN_00bc5ff8`.

---

## Per-component schemas

### Equipment — `FUN_0065b0d0` (stride 0x20)   ★ loadout slot type
Ordered fields:
0. **enum `EquipmentTypeEnum`** default = `Primary`  (`s_EquipmentTypeEnum_00bc67ac` / `s_Primary_00bc6798`)
1. int = 0
2. int = 0
3. int = 0
4. int = 0
5. int = 0
6. int = 0
7. short/bool = 0  (`FUN_00656890(0)`)

**EquipmentTypeEnum = { Primary = 0, Secondary = 1 }** (per project background; `Primary` is the
default member). This is the per-item slot-class tag — it distinguishes a primary vs secondary
weapon/equipment slot. The 6 ints are unlabeled equipment params (likely model/ammo-type/icon
handles); no float magazine value here.

### HumanInventory — `FUN_00661260` (stride 0x1c)   ★ SPECIAL INTEREST
Ordered fields (the *entire* persisted reflection schema):
0. **int = 0**
1. **int = 0**
2. **int = 0**

Exactly **three INT fields, all default 0**, then a nested sub-struct register
(`*DAT_00edc6d8+0x1c` vtable call) and the commit/guard against anchor `DAT_017bde64`.
No enum, no float, **no named magazine/ammo/capacity field**. The 3 ints are the only
plausible slot-count/capacity/current-count candidates, but the decomp carries **no string
labels** to disambiguate which is which — flagged UNKNOWN. Magazine capacity does **not**
appear as a labeled field here; if it exists it is one of these unlabeled ints or lives in
the nested sub-struct / in `WeaponProjectileBase` (FUN_0065ca70) rather than in HumanInventory.

### RuntimeInventory — `FUN_00667210` (stride 0x30)   ★ SPECIAL INTEREST
**Not a scalar field template.** It is the *runtime* inventory rebuild pass: zeroes an inline
11-dword struct, commits via `FUN_0064a600`, then iterates an entity/hash table
(`FUN_0052a1b0`/`FUN_0052a3b0`) under a critical section. **No** `FUN_006562xx` field calls →
**no weapon-slot/capacity/ammo scalar fields** are exposed here. Persisted slot schema lives in
`HumanInventory`; this is the live mirror that re-populates from entity components at runtime.

### RuntimeVehicleInventory — `FUN_0066ebf0` (stride **0x02**)   ★ SPECIAL INTEREST
The whole serialized state is a **2-byte bitmask** (`DAT_017bf80c = 2`). No int/short
capacity, ammo, or magazine fields. The runtime builder walks the vehicle's weapon/hardpoint
slots, reads each slot's **type code** (`puVar6[1]`, validated `< 4` for the named branch,
then a `switch`), and folds the category into two flag bytes:

| slot type code | byte | bit set |
|---|---|---|
| 1 | local_3c | 0x04 |
| 2 | local_3c | 0x10 |
| 3 | local_3c | 0x01 |
| 4 | local_3c | 0x80 |
| 5 | local_3b | 0x01 |
| 6 | local_3b | 0x30 |
| 7 | local_3b | 0x10 |
| 8 | local_3b | 0x20 |

`local_3b` is pre-masked `&0xC0` on entry; a post bit (0x40 of local_3c) is set if
`(local_3c & 0x15)!=0`. These are **weapon-category PRESENCE flags** (which of ~8 slot
categories a vehicle exposes), **NOT counts/capacities/magazine sizes**. Categories are raw
integer codes 1–8 — **no enum-table strings** are registered for this class.

### GrappleParameters — `FUN_00664ca0` (stride 0x1c)
0. **enum `BoolEnum`** default = `True`  (`s_BoolEnum_00bc6084`, default member `&DAT_00bc607c`="True"; `False`=`s_False_00bc6074`)
1. float = `DAT_00ba8990`
2. float = `DAT_00b984ac`
3. float = `DAT_00dfdb4c`
4. float = `DAT_00b9c174`
5. float = `DAT_00b92874`
6. float = `DAT_00b9c174` (same const as field 3)
→ 1 bool toggle + 6 tunable floats (grapple range/speed/force etc.). Defaults are `DAT_` globals.

### Camera* family
**CameraCarPreset — `FUN_0065e1d0` (stride 0x50):** 14 fields — scalar float gains interleaved
with **3 vector3 bundles** (`FUN_00656610(0)`): f0 `DAT_00d2d87c`, f1 `DAT_00dfdb5c`, **v2**,
f3 `DAT_00beb6b4`, f4 `DAT_00beac14`, f5 `DAT_00b92874`, f6 1.0, **v7**, f8 `DAT_00b92b58`,
**v9**, f10 `DAT_00b92b58`, f11 `DAT_00beb5ac`, f12 `DAT_00beb5b0`, f13 `DAT_00bcdb94`. The
vector3s are camera offset/target positions.

**CameraShake — `FUN_0065ea10` (stride 0x10):** enum `CameraShakeTypeEnum`
(`s_CameraShakeTypeEnum_00bc6288`, default null) ; float 1.0 ; float `DAT_00b9b700` ; int 0.

**HumanCameraModifier — `FUN_0065eaf0` (stride 0x38):** 12 floats (mostly 1.0 weights;
f7 `DAT_00bbb99c`, f8 `DAT_00d2d840`, f9 `DAT_00b9776c`) + 2 enums `HumanCameraBehaviorCategory`
(both default UNKNOWN).

### Human / Vehicle animation family
**HumanAnimationSystem — `FUN_0065ade0` (stride 0x34):** int×3 (default 0) + float×10
(`_DAT_00beb6d0`, `DAT_00beb6cc`, `_DAT_00beb6c8`, `_DAT_00beb6c4`, `_DAT_00bb3e80`,
`DAT_00b97744`, `DAT_00b92874`, `_DAT_00beb6c0`, `_DAT_00beb6bc`, `DAT_00bbc7ec`).
**HumanAnimationSet — `FUN_0065af90` (stride 8):** int 0, int 0.
**VehicleAnimationSet — `FUN_0065b030` (stride 8):** int 0, int 0.

### ModelMixerProfile — `FUN_00663700` (stride 4)
Single INT = 0 (profile / blend layer index).

### Carryable — `FUN_00663810` (stride 4)
No scalar fields (only the nested sub-struct register). Marker/tag component for
pick-up-and-carry objects; data lives in the nested registered struct.

### VehicleDisguiseScale — `FUN_0065c330` (stride 0x0c)
float `DAT_00b977d0`, float `DAT_00b9c174`, float `DAT_00b977d0` — 3 disguise scale factors.

### Seat / Entrance / Rider runtime family (vtable-dialect — schemas not enumerable here)
- **SeatParameters** (stride 0x14) & **EntranceParameters** (stride 0x1c): authored params; their
  ANCHOR appears only at init in this dump (no `FUN_006562xx` template present). Field list UNKNOWN
  from this dump — decode via `PTR_CopyFromStream_00bbfd88` / `_00bbfe50`.
- **RuntimeHeadLookAt** (stride 0x2c): runtime head-aim state; UNKNOWN field list (no template in dump).
- **RuntimeRiderDiveEnter** (stride 0x44) & **RuntimeVehicleCrawlExits** (stride 0x6c): vtable-dialect,
  UNKNOWN field list in dump.
- **RuntimeRiderCrawlExit** (stride 0x34): runtime-apply `FUN_0053b4b0` builds the 52 B exit
  pose/transform via seat-query helpers `FUN_00636e20`/`FUN_006373e0`; no scalar reflection fields.
- **RuntimeEntrance** (stride 0x18): runtime-apply loop (`FUN_005382xx`) toggles a single bool
  ("entrance available", bit 1) per entity via XOR-mask, commits a 0xc-stride record.
- **RuntimeSeatPlayerUsable** (stride **0x01**): runtime-apply `FUN_0066a9d0` writes one bool =
  `(*src >> 0xC) & 1` — whether a seat is player-usable (source bit 12).
- **Usable** (stride 8): generic interaction flag; vtable-dialect, field list UNKNOWN in dump.

---

## Capacity / slot / ammo summary (project question)

- **Magazine / ammo capacity is NOT a labeled field in any class in this family.**
- **Equipment** holds the only explicit slot taxonomy: **`EquipmentTypeEnum {Primary=0, Secondary=1}`**
  tags an item as a primary or secondary slot. No per-magazine count.
- **HumanInventory** = 3 unlabeled INTs (default 0) — the only numeric slot/capacity candidates in
  the player inventory, but unnamed/ambiguous. Likely slot counters; needs runtime confirmation.
- **RuntimeVehicleInventory** = a 2-byte **category-presence bitmask** (slot types 1–8), explicitly
  NOT counts or capacities.
- **RuntimeInventory** exposes no scalar schema (runtime rebuild pass only).
- Conclusion: magazine capacity almost certainly lives on the **weapon/projectile** side
  (`WeaponProjectileBase` = `FUN_0065ca70`, which has explicit int fields incl. `0x1e`/`0x3c`),
  **not** in these player/vehicle/human inventory components.
