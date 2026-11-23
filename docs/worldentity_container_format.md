# The `worldentity` container

**Status:** specification. Every byte rule below is checked against the retail container in PC
`vz.wad`: it parses into the model described here and re-writes to the identical 1,819,324 bytes,
and every one of its 37,944 records decodes into typed field values and re-encodes from them
(`mercs2_formats/tests/worldentity_retail.rs`). All 6,126 templates re-express through the template
author form and re-encode their 131,955 records byte for byte
(`mercs2_quartermaster/tests/template_retail.rs`).
**Implementation:** `mercs2_formats::worldentity` (codec) and `mercs2_quartermaster::template`
(author form).
**Loader:** `mercs2_unpacked.exe` — CHDR dispatcher `FUN_00654940`, reached from the block dispatch
`FUN_004646B0`.

Confidence labels: **PROVEN** = checked against retail bytes or read in the disassembly this pass;
**INFERRED** = reasoned from the code or the data, not exercised; **UNKNOWN** = carried verbatim,
meaning not established.

---

## 1. The asset

| Field | Value |
|---|---|
| Type hash | `0x5647C35D` (`pandemic_hash_m2("worldentity")`) |
| ASET type id | 17 |
| Retail | one, `0x50075B3B`, in block 3185 `blocks\VZ\resident_P000_Q3.block`, 1,819,324 bytes |

The container is the game's template library: every name `Pg.Spawn` and
`ObjectState.StartEmitter` resolve is a `Name` record here, and a template is the set of component
records under its key.

---

## 2. Tree

A UCFX tree ([`ucfx_tree_container.md`](ucfx_tree_container.md)); `ucfx::parse_ucfx_tree` reads it
strictly and `write_ucfx_tree` re-writes it identically (PROVEN — no deviation from the tree
rules). Top level, in this order, 200 rows in retail:

```text
CHDR     8 B
enum     enum tables
UNIQ     [u32 n][n × u32 key]
COMP ×N  marker; children info, schm, data       (195 in retail)
flgt     [u32 n][n × (u32 hash, cstring class)][u32 trailer]
flgs     [u32 n][n × (u32 key, 32-byte bitset)]
```

### 2.1 `CHDR`

```text
+0  i16  → [0x0117607C]
+2  i16  → [0x01176078]   the Transform record-stride gate
+4  u32  flags
```

Retail: `0, 0x33, 1`. `FUN_00654940` (CHDR arm, PROVEN):

| Flag bit | Effect |
|---|---|
| 0 | `data` chunks are grouped records (§3.3); after the container, every `UNIQ` key without a `HibernationControl` record goes to `thunk_FUN_024ECAB0` |
| 2 | `DAT_01176053`: the compact form of §7, in which class names and `Name` strings are stored as hashes. Clear on PC, set on console |

The codec refuses any other bit (no meaning established).

### 2.2 `enum`

```text
u32 n
n × { cstring name, u32 pandemic_hash_m2(name), u32 k,
      k × { cstring value name, u32 pandemic_hash_m2(value name), u32 value } }
```

72 tables in retail; every stored hash equals the hash of its name (PROVEN). `FUN_00654940` passes
the chunk to `thunk_FUN_028CB000`.

### 2.3 `UNIQ`

`[u32 n][n × u32 key]`: every template key, strictly ascending (PROVEN, 6,126 keys). The CHDR
bit-0 pass walks it (§2.1).

### 2.4 `flgt` and `flgs`

`flgt` lists component classes: `[u32 n][n × (u32 pandemic_hash_m2(class), cstring class)]`, then a
u32 trailer (`0xE9DABB4A` in retail; meaning UNKNOWN — it is not the CRC-32 of the table, and it is
carried verbatim). 215 classes in retail. `FUN_00654940` reads only the count.

`flgs` is `[u32 n][n × (u32 key, 32 bytes)]`. **Bit `i` of a key's 32 bytes is set exactly when the
key has a record in the class `flgt[i]`** (PROVEN: all 6,057 retail `flgs` records equal the bits
their keys' classes give). A key whose classes give no bit has no `flgs` record (69 retail keys, all holding only
`Name`). `Name` and `NetCategoryInfo` are not in `flgt`. Records are ascending by key.

The loader (`flgs` arm, PROVEN) puts each 32-byte set into the `Flags` component container
`0x00DF6D08` (stride 32) through `FUN_00649180`, ORing it with the set of the entity's source when
`SceneObject` names one.

The bitset is four u64 words: a console bake stores each big-endian (§7).

---

## 3. `COMP`

A marker with three children, `info`, `schm`, `data`.

### 3.1 `info`

```text
cstring class name
u32     pandemic_hash_m2(class name)
u32     version
u32     record count     (the number of grouped records in data)
u32     flags            (0 or 1 in retail)
```

All PROVEN against retail; `FUN_00654940` (`info` arm and the data loop at `0x00654CEC`–`0x00654EC5`):

* The engine looks the class name up in three tables a registration function fills
  (`FUN_0064EE60` and siblings): `0x017C0B58` native deserializer, `0x017C0B80` version,
  `0x017C0B6C` schm-driven deserializer. **When the `info` version equals the registered version the
  native deserializer reads the record; otherwise the schm-driven one does.** A class with no
  deserializer is skipped.
* Flags bit 0: before each record the loader reads a u32 that it passes as the container's
  secondary key. The read happens after the record start is taken, so that u32 is also the
  record's first field (`EntranceLink`, `EquipmentLink`, `PhysicalLink`, `Relationship`,
  `RiderLink`, `SeatLink`, all with field `0x54704554` at offset 0).
* The record count is the number of grouped records (PROVEN on all 195 groups).

Retail holds 195 groups of 194 classes; `LightObject` has two groups with the same schema and
version. The loader reads every group through the deserializer its class name selects, so a record
reads the same in either.

### 3.2 `schm`

`[u32 n_fields][u32 payload_stride][n × 16-byte field]`, each field:

```text
+0   u32  type code
+4   u32  pandemic_hash_m2(field name)
+8   u32  0 in all retail
+12  u16  byte offset of the field's storage unit in the payload
+14  u8   bit start
+15  u8   bit width   (0 = the whole unit)
```

Type codes, from the engine's per-field readers (`FUN_00656210` int, `FUN_00656320` float,
`FUN_00656720` enum, `FUN_00656610` vec3, `FUN_0065644A` transform; PROVEN):

| Code | Unit | Read as |
|---|---|---|
| 1, 2 | 1 byte | `char` |
| 3, 4 | 2 bytes | `short` |
| 5 | 4 bytes | `int` |
| 6 | 4 bytes | `int`; holds a `pandemic_hash_m2` name hash or a raw value |
| 7 | 4 bytes | `f32` (`Health`'s first field holds `100.0f`) |
| 8 | inline string | `Name` only (§3.4) |
| 9 | 4 bytes | `int`; the enum reader resolves it as the hash of an enum value name through `FUN_00655EA0`. 7,516 of 21,055 retail code-9 values are not a hash in the `enum` chunk |
| 10 | 12 bytes | three `f32` |
| 11 | 32 bytes | eight dwords |

The engine reads codes 1 and 2 identically, and 3 and 4 identically; what separates each pair is
UNKNOWN. Codes 1, 4 and 5 carry bit fields in retail: `HibernationControl` two 1-bit fields in byte
5, `Health` three 1-bit fields in the u32 at 4, `HumanInventory` fields of 1/7/8/8/8 bits in the u32
at 20. In every retail record, no bit outside the declared fields is set, and every byte no field
covers is 0 (PROVEN).

### 3.3 `data` — grouped records

With CHDR bit 0 set (retail), `data` is `record count` groups:

```text
u32 n            (≥ 1)
n × u32 key      the keys sharing this payload
payload
```

**Shared payloads.** Keys with identical payloads share one record: retail stores 138,081 keyed
entries in 37,944 records. A key can also have several records in one class (`Label`: 7,086 entries
over 2,626 keys).

The payload layout is the native deserializer's. For 192 groups it is the schm's: `payload_stride`
bytes with each field at its offset (PROVEN by exact consumption of every group). Three classes'
native readers read something else (PROVEN by disassembly and exact consumption):

| Class | Version | Native reader | Payload |
|---|---|---|---|
| `Name` | 1 | `FUN_006569B0` | `[cstring name][u8 flag]` (§3.4) |
| `PointLocation` | `0x50` | `FUN_00656AD0` | `[32 bytes][cstring]` — reads 0x20 bytes, then a string of at most 0x7F; the schm's code-6 field at 32 does not describe the native layout |
| `NetCategoryInfo` | 1 | `FUN_0063D750` | one u16 — reads 2 bytes; the eight schm fields are bit fields of it at their bit starts (0+4, then seven 1-bit fields at 4–10); their schm offsets 0, 2, …, 14 are not used |

Strings are read by `FUN_00825DC0` into a 0x80-byte buffer: at most 0x7F bytes.

### 3.4 `Name`

Each record names its keys. `FUN_006569B0` reads `[cstring][u8 flag]` and adds a record to the `Name`
container `0x00DF6B88` through `FUN_00649180(container, key, h, h, …)` with
`h = FUN_00824270(name)`, when the flag is non-zero, the key is negative as an
`i32`, or `DAT_00CFB589` is set. So a template's name is registered whatever its flag (PROVEN). That `FUN_00824270` is
`pandemic_hash_m2` is INFERRED: the same function hashes the class names the deserializer tables are
keyed by, and every retail class hash is `pandemic_hash_m2` of its name.

With CHDR bit 2 set the payload is `[u32 hash]`, with no flag byte (`0x6569BC`–`0x656A0B`). The registry
call is `FUN_00649180(0x00DF6B88, key, h, h, "NO_NAME")`, where `h` is the stored hash only when bit 1
of `DAT_00DF6B90` is set, else 0 (PROVEN). What sets that bit on PC is unread.

---

## 4. Templates and the name lookup

**The name registry is the `Name` container's second hash index** (PROVEN). `FUN_00672F70` walks the
index at `0x00DF6BD8` (`0x00DF6B88 + 0x50`) for a name hash and returns the first owner key (`+0x44`
array) that is negative as an `i32`; `FUN_00672F60` is its wrapper, which `ObjectState.StartEmitter`'s
bridge `FUN_004D28C0` calls, among other call sites. A key is a template when bit 31 is set.

**Key ranges** (PROVEN counts; meaning INFERRED):

| Top nibble | Keys | Range | Content |
|---|---|---|---|
| `0x8` | 5,993 | `0x80000002`–`0x8000B3C4`, sparse | the level's own templates |
| `0x9` | 133 | `0x90000065`–`0x900001F7`, sparse | shared library templates: `global_particle_*` effects, `SoundMaterial (…)`, `Hibernation Control (…)` presets, building prototypes named `_…`, debris templates |

No engine code read this pass branches on bit 28; both nibbles satisfy the template test (bit 31).
The codec gives a new template `0x8` in the top nibble and the low 28 bits of
`pandemic_hash_m2(name)` (`worldentity::derived_template_key`), and refuses a key or a name hash
already present.

**A derived `0x8` key is spawnable** (live, 2026-10-04, the game under Wine on macOS). With the
container re-shipped in a patch block and `qm_gate_c4` appended under its derived key `0x8D9E11CB`,
`Pg.GetGuidByName("qm_gate_c4")` returned `8D9E11CB`, `Pg.Spawn("qm_gate_c4")` played the C4
explosion its `RedEffectComponent` names, and `ObjectState.StartEmitter` on a Monster Truck's
`hp_fx_exhaust_a` with `qm_gate_c4` fired it. The retail name resolved as before
(`global_particle_explosion_c4` → `80008028`) and a name no template has returned `nil`.

**The C4 template** `global_particle_explosion_c4` is `0x80008028` (PROVEN): `EffectTemplate`
(shared record of 438 keys, value 0), `HibernationControl` (shared, 293 keys), `RedEffectComponent`
(own record, `name` field `0x1DE5C824` = `0x41B4326E` = `global_explosion_c4`), `SoundEffect` (own
record). `fx_Explosion_HugeOil_RigOnly` is `0x80008756` (`CameraShake`, `DamageKey`,
`EffectTemplate`, `Explosive`, two `GenericLOD`, `HibernationControl`, `ObjectScript`,
`PhysicalLink`, `SoundEffect`, `Stimulus`; no `RedEffectComponent`).
`global_particle_fire_carhood` is `0x80008C06` (`EffectTemplate`, `HibernationControl`,
`PhysicalLink`, `RedEffectComponent`, `SoundEffect`).

**The effect a template starts** is its `RedEffectComponent`'s `name` field `0x1DE5C824`: in 524 of
the 538 retail `RedEffectComponent` records it is the name hash of one of the 314 effect assets
(PROVEN by the data). That the runtime starts the effect through this field is INFERRED: the code
from the spawned `RedEffectComponent` to PgFX was not read this pass.

---

## 5. Loading twice

Every per-record insert is an upsert (PROVEN):

* `FUN_0064A600` (the pool insert native deserializers end in) probes the key and, when present,
  copies the new record over the old one; only a new key grows the pool.
* `FUN_00649180` (the `Name` and `Flags` insert) overwrites in place on a hit.

So processing a `worldentity` a second time rewrites the records of keys it shares with the first
and adds the keys it alone carries.

**A patch copy is processed** (live, 2026-10-04): a patch block carrying the container with one
template appended made that template's name resolve and spawn (§4).

**guidmap** (`0x385EA82C`, type `0x140E8728`, same block): its keys are the 6,126 template keys plus
one zero slot. Neither the template lookup (`FUN_00672F70`) nor `FUN_00654940` reads it, so a new
template needs no guidmap row (INFERRED).

One `CHDR` row holds the whole map:

```text
+0   u16  version            8
+2   u16
+4   u32  a                  6,127
+8   u32  b                  59,686
+12  u8, u8                  0x0D, 0x10
+14  u16
+16  u32                     read at version 7 and above
+20  a × u32  keys, the zero slot first
     a × u8
     b × u32
     b × u16
```

The field order is `PgGuidMap`'s reader on PS3 (`FUN_001EDDB0`, its class's vtable slot 5). The PC
reader is not located: `0x004CBC90` returns the type hash, and its class's vtable is unread. Sizes fit
exactly on both bakes (PC 20 + 5 × 6,127 + 6 × 59,686 = 388,771; Xbox, b = 59,684, 388,759), and the
converted PS3 guidmap equals PC's byte for byte (PROVEN). What `+16` and the `b` arrays hold is unread.

---

## 6. Writing

`WorldEntity::write` lays the tree out as in §2 with every derived word computed: the `info` hash
and record count, the `schm` bytes, the `enum` and `flgt` hashes, the `UNIQ` and `flgs` counts.
`WorldEntity::append_template` adds one template whose every component and field the caller
declares (no record is copied from another template):

1. The key carries bit 31 and is unused; the name's hash is not an existing name's.
2. Each declared record is encoded from typed values against its class's schema and appended to
   the end of the class's first group.
3. A `Name` record `[name][flag]` under the key.
4. The key is inserted into `UNIQ` in order.
5. A `flgs` record with the bits of the template's classes listed in `flgt`, inserted in order,
   when there is at least one.

The template author form is specified in `mercs2_quartermaster::template`.

### 6.1 The set's templates

`add_fx` templates ship in the container of the resident block, at the block's own path. A Shipment's build writes the game's container with its own templates appended,
in contribution order; `qm link` writes the game's container with every installed Shipment's
templates appended, Shipment by Shipment in load order, into the one resident block that also
carries the linked scripts (`mercs2_quartermaster::fx::merge`). The merge refuses:

1. a template whose name hash or derived key the game's container already holds;
2. a template whose name hash or derived key another added template holds, naming both Shipments;
3. a template that does not lower against the container's schemas, or that declares other than one
   `RedEffectComponent`.

Every other record of the container is the game's, byte for byte, and every row of the resident
block is copied from the game's.

---

## 7. The compact form (console bakes)

Every `worldentity` and layer of the Xbox and PS3 bakes, and the DLC's `worldentity`, have CHDR bit 2
set. The Xbox and PS3 base `worldentity` are byte-identical, and so are the DLC's on Xbox, on PS3 and in
the Nov 25, 2008 prototype. The Nov 6, 2008 prototype and Pack B store the DLC's `worldentity` named,
with `schm`, `enum` and strings. The retail DLC keeps 15 Xbox layers named and 5 compact; on PS3, 4 and
16.

```text
CHDR
UNIQ                  worldentity only
COMP ×N               info, data
flgt, flgs
```

No `enum` row, no `schm`. `info` is `[u32 hash][u32 version][u32 count][u32 flags]`, with no class name
(`0x654B8C`–`0x654BD0`). The hash selects the exe's native table (`0x017C0B58`, `FUN_008242B0`
`0x654C0A`). A `Name` record holds a hash (§3.4). `flgt` keeps its strings, and in a layer is the single
trailer word. A layer's `data` is `[u32 key][payload]`, a `worldentity`'s grouped (§3.3). Most console
layers have no `Name` group; the PC bake has one in each.

**Layouts.** The PC bake's `schm` of the same class hash and version gives each record, 207 classes in
all (`mercs2_formats::be_to_le::ecs::ComponentSchemas`).

**Big-endian.** Every field swaps at its own width, and three things need more than that:

| What | Console | PC |
|---|---|---|
| A fixed record's unit of bit fields | the whole unit bit-reversed, so a `WeaponHint` 2-bit field holds 1 where PC holds 2 | |
| `NetCategoryInfo`'s u16 | a field at bit `s` of width `w` sits at bit `16 - s - w` and keeps its value | as `schm` declares |
| `flgs` bitset | four big-endian u64 | four little-endian u64 |

`schm` declares the same bit positions in a console prototype as on PC, so it does not show this.
PROVEN by the join below; which code writes or reads the console order is unread.

**Proof.** `mercs2_probe ecs-compare` joins two archives by container, class and key, and
`tests/ecs_console_retail.rs` gates the result:

| Compared | Records equal | `flgs`, `flgt`, `UNIQ` | Guidmap |
|---|---|---|---|
| PS3 retail, converted, against PC retail | 220,085 of 220,085 | all equal | byte-identical |
| Xbox retail, converted, against PC retail | 220,081 of 220,081 | `flgs` 923 of 924 | differs |
| Xbox DLC against PS3 DLC | 104,852 of 104,852 | all equal | differs |
| Xbox DLC against the Nov 6 prototype | 108,052 of 108,061 | `flgs` 17 of 21 | differs |

The Xbox bake lacks the two entities PS3 and PC hold (`Transform`, `Anchor`). The 9 prototype records
are 8 `Transform` and 1 `Road`. Each `Name` hash compares with its twin's string.

**Reading.** `WorldEntity::parse_compact` reads the form and writes back its bytes.
`ComponentSchemas::inflate` turns a compact container into the named form, with each `Name` string the
PC bake or a rainbow file knows for its hash, else `0xHHHHHHHH`. The game reads the compact bytes.
