# Mercenaries 2: World in Flames — ECS Family 07: Gameplay State / Health / Mission

Reverse-engineered from the Ghidra decompilation of the game EXE (PC retail x86). Hashes from
`tools/pandemic_hash.py --m2`. All addresses are in the game's image (0x00400000-based).

## How these classes are wired (deserialization model)

Each reflection component has a **builder** function (called once at startup from the `0x00a7xxxx`
init table) that fills a **0x50-byte descriptor** at a fixed `_DAT_<base>` address:

| descriptor offset | meaning |
|---|---|
| `base + 0x00` | `&PTR_CopyFromStream_<addr>` — the vtable used to read the component from the stream |
| `base + 0x04` | name-hash/id slot (`DAT_<base+4>`) — also the arg passed to `FUN_00665590` in the schema template (the per-class "register fields" tail) |
| `base + 0x0c` | `3` (type tag) |
| `base + 0x24` | **stride** = serialized size in bytes (the value reported in the registry below) |
| `base + 0x26` | `8` (alignment) |
| `base + 0x28` | `0x100` |
| `base + 0x30` | seed `0x9e3779b9` |
| `base + 0x3c` | `s_<ClassName>_<straddr>` (the name string, written last) |

Two deserialization styles are used:

1. **Schema template** (load-from-disk POD): a small `FUN_006xxxxx(param_1)` whose body is an ordered
   list of field-reader calls, then `(**(code**)(*DAT_00edc6d8+0x1c))(...)`, then
   `FUN_0064a600(param_1, …)` and a conditional `FUN_00665590(param_1,(int)DAT_<base+4>)`. The field
   readers and their meaning:
   - `FUN_00656210(default)` → **int32** field (default value inline)
   - `FUN_00656320(default)` → **float** field (default is raw IEEE bits; `0x3f800000`=1.0f; `DAT_…`=named const)
   - `FUN_00656720(enumTableStr [, defaultEntryStr])` → **enum** field
   - `FUN_00656890(default)` → **bool / u8** field (single byte; `_DAT_02455ab0` dispatch)
   - `FUN_00656610` → **vec3 / {x,y,z}** triple
   - `FUN_00656440` → **vec3/quat block** (8 dwords, type tag 0xb)
   - an argument of `FUN_00824270()` = a **runtime name-hash** default (string hashed at init)
2. **Raw-copy** (`(**(code**)(*param_2+0x14))(dst, N, 0)` with no `FUN_00656xxx` calls): the component is
   a fixed `N`-byte blob copied verbatim, OR it is **runtime-only** — never read from the level stream;
   the only writer is a gameplay producer that fills the component live (these are noted per-entry).

`FUN_00656210/320/720/890` are shared stream readers keyed by field-name hash (`FUN_00824270`), reading
1/2/4 bytes by the reflected field's type code (case 1/2=byte, 3/4=short, 5/6/9=dword, 7=float) — see
`@0x00656210` ll.307380-307445.

---

## Registry (all 35)

| Class | m2 hash | descriptor base | name-str addr | stride | builder | schema/producer fn | purpose |
|---|---|---|---|---|---|---|---|
| Alarm | 0xbe65fdd0 | 0x017bd3f8 | 0x00bc5018 | 4 | FUN_? | FUN_0065bc40 | Alert/alarm state: one float (cooldown/level) |
| BuildingDestruction | 0x17a5555b | 0x017bdda8 | 0x00bc52b4 | 0x18 | FUN_? | FUN_00661090 | Destructible building: 5 floats + 1 int (damage thresholds/state) |
| CashValue | 0x564990c3 | 0x017bd718 | 0x00bc50dc | 4 | FUN_? | FUN_0065c6b0 | Cash/money pickup amount: 1 int32 |
| ContextAction | 0xf957a94c | 0x017bff18 | 0x00baa60c | 0xa0 | FUN_? | raw-copy (0xa0 B) | Contextual-action binding blob (160 B) |
| ControlBinding | 0x3486768b | 0x017c00a8 | 0x00bc5d30 | 4 | FUN_? | raw-copy (4 B) | Control/input binding id (runtime) |
| DamageKey | 0xef41976f | 0x017bdf38 | 0x00bc5348 | 4 | FUN_? | FUN_006616c0 | Damage classification: 1 enum `DamageKeyEnum` |
| DangerousBuilding | 0x543977f7 | 0x017bdb28 | 0x00bc5208 | 4 | FUN_? | FUN_00660a90 | "Dangerous" flag/id: 1 int32 |
| FactionMarker | 0x9b98cb09 | 0x017bd588 | 0x00bc5078 | 4 | FUN_? | FUN_0065c0f0 | Faction id tag on an entity: 1 int32 |
| FactionValue | 0x8bfc69d6 | 0x017bd7f4 | 0x00bc50f8 | 4 | FUN_? | FUN_0065c7d0 | Per-faction scalar (rep/value): 1 float |
| FactionZone | 0x67267cc1 | 0x017bd628 | 0x00bb98c0 | 4 | FUN_? | FUN_0065c490 | Faction-owned zone id: 1 int32 |
| FlightNoise | 0x10ed85af | 0x017bcbd8 | 0x00bc4dc0 | 0x18 | FUN_? | FUN_0065dd50 | Aircraft noise emitter: 5 floats + 1 int |
| **Health** | 0x06be1abf | 0x017bbf58 | 0x00bc49c0 | 8 | FUN_0063e090 | FUN_00656db0 | **HP component: 1 float + 3 bool** (see below) |
| HibernationControl | 0xe18afd65 | 0x017bd178 | 0x00bc4fcc | 6 | FUN_? | FUN_0065f380 | LOD/sleep distances: 4 int + 2 bool |
| LandingZone | 0x2a20b640 | 0x017bd6c8 | 0x00bc50d0 | 8 | FUN_? | FUN_0065c610 | Heli/LZ marker: 2 int32 (type=1, id=-1) |
| NetCategoryInfo | 0x99cdca52 | 0x017bebb8 | 0x00bc56a4 | 2 | FUN_? | FUN_006652a0 | Network replication flags: 8 bool/u8 |
| **ObjectScript** | 0xd81512a1 | 0x017bdd58 | 0x00bc52a4 | 8 | FUN_006424e0 | FUN_00660ff0 | **Binds entity→Lua: 2 int32 (script hashes)** (see below) |
| RtAlarm | 0x7a3425ce | 0x017c0198 | 0x00bc5d80 | 1 | FUN_? | raw-copy (1 B) | Runtime alarm state byte |
| RtDamageFlags | 0x93621235 | 0x017c0238 | 0x00bc5d88 | 4 | FUN_? | raw-copy (4 B) | Runtime damage bitflags |
| RtFactionZone | 0xa67114c7 | 0x017c05f8 | 0x00bc5ed8 | 0x1c | FUN_? | raw-copy (0x1c B) | Runtime faction-zone state (28 B) |
| RuntimeAssetRef | 0xd2435030 | 0x017beed8 | 0x00bc57c0 | 4 | FUN_? | raw-copy (4 B) | Runtime asset handle |
| RuntimeClaim | 0x5d5cb7bd | 0x017bf5b8 | 0x00bc59fc | 0xc | FUN_? | FUN_00538b50 (producer) | Runtime area claim: {claimVal float, claimant} |
| RuntimeFlightNoise | 0xebf6d595 | 0x017bede8 | 0x00bc5784 | 0x20 | FUN_? | producer @0x53xxxx | Runtime noise vec3 + state (32 B) |
| **RuntimeHealth** | 0xf9b9b2a5 | 0x017bef78 | 0x00bc57e4 | 0xc | FUN_00644c10 | FUN_004cfed0 (producer) | **Live HP: {max, cur} floats** — order corrected 2026-07-26 (see below) |
| RuntimeHijackState | 0xd5f2b17a | 0x017c0468 | 0x00bc5e84 | 0x14 | FUN_? | raw-copy (0x14 B) | Runtime vehicle-hijack state (20 B) |
| RuntimeLastDamageApplied | 0x9cbd437b | 0x017bf608 | 0x00bc5a20 | 0x1c | FUN_? | raw-copy (0x1c B) | Last damage event record (28 B) |
| **RuntimeNodeHealth** | 0x76927bf5 | 0x017befc8 | 0x00bc57f4 | 4 | FUN_00644cd0 | FUN_004cfed0 (producer) | **Per-node HP: 1 float** (see below) |
| RuntimeObjectiveMarker | 0x2a77b292 | 0x017bf248 | 0x00bc58a8 | 0x70 | FUN_? | raw-copy (0x70 B) | Live objective HUD marker (112 B) |
| RuntimeOwnerGuid | 0xaff006a7 | 0x017bf158 | 0x00bc585c | 4 | FUN_? | producer @0x532xxx | Runtime owning-entity GUID |
| RuntimeRope | 0xa9c2a15b | 0x017c0008 | 0x00bc5ccc | 4 | FUN_? | raw-copy (4 B) | Runtime rope/tow handle |
| RuntimeScriptCallback | 0x3b105827 | 0x017bf298 | 0x00bc58c0 | 8 | FUN_? | raw-copy (8 B) | Live script callback {hash, arg} (see below) |
| RuntimeScrub | 0x7da4bd48 | 0x017bf2e8 | 0x00bc58d8 | 8 | FUN_? | raw-copy (8 B) | Runtime "scrub"/cleanup record |
| RuntimeTerrainBound | 0x745c6d6a | 0x017bfec8 | 0x00bc5c9c | 0x1c | FUN_? | producer @0x66cxxx | Runtime terrain AABB (7 floats) |
| RuntimeTimer | 0x38437a4e | 0x017bee88 | 0x00bc57b0 | 0x10 | FUN_? | FUN_0066ad60 (producer) | Live timer {value u32, active bool} (see below) |
| ScrubObject | 0xab92c697 | 0x017bea28 | 0x00bc565c | 4 | FUN_? | FUN_00662120 | Scrub/despawn object id: 1 int32 |
| **StateMachine** | 0x98a3661f | 0x017bd8a8 | 0x00bc5170 | 0x10 | FUN_00641aa0 | FUN_0065fcb0 | **4 name-hash int32** (see below) |

Stride = value of `DAT_<base+0x24>`. For pure-bool schemas (NetCategoryInfo) the stored stride (2) is a
packing/alignment hint and is smaller than the field count; the schema field list is authoritative.
"producer" = no disk schema template; the listed `FUN_` is the gameplay function that fills the component
at runtime (so the stride is the in-memory size, not a stream record).

---

## Priority components

### Health  (hash 0x06be1abf, descriptor 0x017bbf58, stride 8)

Builder `FUN_0063e090` (ll.293604-293629) sets `CopyFromStream = PTR_CopyFromStream_00bbd1a8`, stride
field `DAT_017bbf7c = 8`.

Schema template **`FUN_00656db0`** (ll.307929-307946):

| # | reader | type | default | inferred field |
|---|---|---|---|---|
| 0 | `FUN_00656320(0)` | float | 0.0 | **HP / current health** (or health fraction) |
| 1 | `FUN_00656890(0)` | bool | 0 | flag (e.g. invulnerable / regen) |
| 2 | `FUN_00656890(0)` | bool | 0 | flag |
| 3 | `FUN_00656890(0)` | bool | 0 | flag |

Layout: 1 float (4 B) + 3 bytes = 7 → padded to the 8-byte stride. There is also a trivial CopyFromStream
`FUN_00638760` (ll.288883-288895) that just `(*vtbl+0x14)(dst,8,0)` raw-copies the 8 bytes when no
field-name remap is needed — i.e. Health serializes as a small flat record, not a string.

**Finding for the "healthpickup" question:** Health itself carries *no HP number for pickups* — it is the
generic 1-float + 3-flag HP record. The actual current/max numbers live in **RuntimeHealth** (below),
which is produced from the live actor, not read from disk. The empty `healthpickup` Lua stub is therefore
expected: health-pickup logic is native (Health float + CashValue/DamageKey siblings), not scripted.

### RuntimeHealth (0xf9b9b2a5, desc 0x017bef78, stride 0xc) & RuntimeNodeHealth (0x76927bf5, desc 0x017befc8, stride 4)

Builders `FUN_00644c10` (RuntimeHealth, `CopyFromStream_00bc24d8`, stride 0xc) and `FUN_00644cd0`
(RuntimeNodeHealth, `CopyFromStream_00bc2528`, stride 4).

Neither has a `FUN_00656xxx` disk schema — both are **runtime-produced**. The producer is
**`FUN_004cfed0`** (ll.103755-103882, called from `FUN_006696a0`):

- It reads the live actor's health via `FUN_005857e0()` (returns `{value, …}`), then computes
  **`{max, cur}`** (two clamped floats) and writes them via `FUN_0064a600(param_1, &piStack_284)` for
  **RuntimeHealth** (gate `DAT_017bef94`, ll.103825-103829). `piStack_284`/`piStack_280` are clamped
  current/max (negatives → 0; `fStack_27c` = a fraction). 0xc stride = `{max f32, cur f32, +1 dword}`.
  **⚠ Field order corrected 2026-07-26** (was `{cur, max}` here and in four sibling docs): the
  accessors settle it — `Object.GetMaxHealth` (`0x005CC030`) loads `[rec+0x00]` and
  `Object.GetHealth` (`0x005CBDB0`) loads `[rec+0x04]`, and `Object.SetHealth` (`0x005CBEE0`)
  clamps against `[esi]` then stores `[esi+4]`. The third dword remains unidentified.
- A second write (ll.103878-103881, gate `DAT_017befe4`) fills **RuntimeNodeHealth** (stride 4 = one
  per-node float), via `FUN_004d5a10` / spatial-hash walk over body nodes (ll.103843-103867) — i.e.
  per-destructible-node health derived from the parent body.

Default HP values are NOT literals here — they come from the actor's health asset (`FUN_005857e0`),
compared against `DAT_00b97eec` (a low-health threshold). So "default HP" is data-driven per actor, not a
constant in these components.

### ObjectScript  (hash 0xd81512a1, descriptor 0x017bdd58, stride 8)  — *script_hash_0 → chunk mapping*

Builder `FUN_006424e0` (ll.296748-296772): `CopyFromStream = PTR_CopyFromStream_00bc0690`, stride
`DAT_017bdd7c = 8`.

Schema template **`FUN_00660ff0`** (ll.312585-312600):

| # | reader | type | default | inferred field |
|---|---|---|---|---|
| 0 | `FUN_00656210(0)` | int32 | 0 | **script_hash_0** (Lua chunk name-hash) |
| 1 | `FUN_00656210(0)` | int32 | 0 | second hash — script *param* or instance/secondary chunk id |

**Answer to the open project question:** `ObjectScript` stores the binding as **two raw 32-bit name
hashes**, read in order, *no string* — exactly the 8-byte stride. `script_hash_0` is field 0. The engine
later resolves the hash to a Lua chunk through the same `FUN_00824270` name-hash table the readers use;
there is no embedded chunk name, so a `script_hash_0`→chunk map must be reconstructed from the hash table
(hash the candidate Lua chunk names with `pandemic_hash --m2` and match field 0). The second int is most
likely the script argument/owner id rather than a separate chunk (it defaults to 0 and is adjacent in the
same 8-byte record). *Flagged unknown:* exact role of field 1 (param vs. secondary hash) is not proven
from the schema alone.

### StateMachine  (hash 0x98a3661f, descriptor 0x017bd8a8, stride 0x10)

Builder `FUN_00641aa0` (ll.296253-296277): `CopyFromStream = PTR_CopyFromStream_00bc0078`, stride
`DAT_017bd8cc = 0x10`.

Schema template **`FUN_0065fcb0`** (ll.311967-311988):

| # | reader | type | default | inferred field |
|---|---|---|---|---|
| 0 | `FUN_00656210(FUN_00824270())` | int32 | namehash | state-machine **definition** name-hash |
| 1 | `FUN_00656210(FUN_00824270())` | int32 | namehash | **initial state** name-hash |
| 2 | `FUN_00656210(FUN_00824270())` | int32 | namehash | state/sub-state name-hash |
| 3 | `FUN_00656210(FUN_00824270())` | int32 | namehash | param / event name-hash |

Four int32 name-hash fields → 0x10 stride. Each default is a *runtime-hashed string token*
(`FUN_00824270()`), confirming these are symbolic state names hashed at load, not numeric enums. This is
the component contracts use for their 768 state-machine pool slots (one record per contract objective FSM).

### Faction\* family

- **FactionMarker** (0x9b98cb09, FUN_0065c0f0): 1 int32 default 0 — the **faction id** tagging an entity.
  In retail `worldentity` the id is `pandemic_hash_m2` of the faction's HUD icon name (verified
  2026-10-21 by hashing each name against the stored values): `HUD_faction_AN` 0x6AD45C01, `_CH`
  0x1A21DCED, `_CV` 0xF1FF0BB3, `_GR` 0x5D30A81B, `_OC` 0x41ED2636, `_PR` 0xC9CE166E, `_VZ` 0x443432FC,
  `_PMC` 0x9FADA550, and `global_pickupmarker` 0x5B9DE759 on the 14 police/taxi templates. It is a
  property of the template, not of its model: one kit model is worn by templates of different
  factions (e.g. "VZ Oil Company Defector" wears `oc_hum_super_executive`). Dump: `mercs2_probe --bin
  kit_templates -- --factions`.
- **FactionValue** (0x8bfc69d6, FUN_0065c7d0): 1 float default 0 — a per-entity faction **scalar**
  (reputation / influence / contribution).
- **FactionZone** (0x67267cc1, FUN_0065c490): 1 int32 default 0 — id of a **faction-owned zone**.
- **RtFactionZone** (0xa67114c7): runtime 28-byte state blob (raw-copy), the live counterpart of FactionZone.

All three load-time Faction\* components are single-field, so faction state on an object is just (id, value,
zone) — the heavy logic is elsewhere (faction tables), these are only the per-entity hooks.

### CashValue  (0x564990c3, FUN_0065c6b0, stride 4)
Single **int32** (default 0) = cash amount granted by a pickup/objective. Pairs with Health for pickups.

### LandingZone  (0x2a20b640, FUN_0065c610, stride 8)
Two int32: field0 default **1** (LZ type/enabled), field1 default **0xffffffff** (LZ id / linked entity,
-1 = none). Heli landing-zone marker.

### DamageKey  (0xef41976f, FUN_006616c0, stride 4)
Single **enum** `DamageKeyEnum` (table `s_DamageKeyEnum_00bc7264`, default entry 0). Classifies what
damage table/key an object responds to.

### BuildingDestruction (0x17a5555b, FUN_00661090, stride 0x18) & FlightNoise (0x10ed85af, FUN_0065dd50, stride 0x18)
Identical schema shape: **5 floats + 1 int32** (24 B). BuildingDestruction = destruction thresholds /
rubble state; FlightNoise = noise emitter params (radius/falloff/intensity floats + a category int).

### HibernationControl (0xe18afd65, FUN_0065f380, stride 6)
**4 int32 + 2 bool** but stored in a 6-byte stride (the schema declares 4 ints whose *values* are LOD
distances 100 / 0xa0 / 0x3c / 0x14, then 2 bools). The 6-byte stride implies the persisted record packs
only the 2 bools + small fields; the int defaults (distances) are mostly compile-time and rarely
overridden. *Flagged:* stride (6) < declared field footprint — packing detail not fully resolved.

### RuntimeTimer (0x38437a4e, FUN_0066ad60, stride 0x10)
Runtime producer reads a live timer object (`FUN_005857e0` → `{value u32, secondWord}`) and writes
`{value, active = (word1 != 0)}`. Used for objective/mission countdowns; not read from disk.

### RuntimeScriptCallback (0x3b105827, stride 8, raw-copy)
8-byte blob = `{callbackHash u32, arg u32}` — the runtime counterpart of ObjectScript's static binding,
holding a pending script callback to fire (mirrors ObjectScript's 2-int layout). Populated by gameplay,
not the stream.

### RuntimeObjectiveMarker (0x2a77b292, stride 0x70, raw-copy)
Large 112-byte runtime record (HUD objective marker: world position, screen state, icon/label handles).
Raw-copied; no field schema — purely runtime/UI state.

### RuntimeHijackState (0xd5f2b17a, stride 0x14, raw-copy)
20-byte runtime blob tracking an in-progress vehicle hijack (target vehicle handle, progress, phase).

### RuntimeClaim (0x5d5cb7bd, stride 0xc) & RuntimeLastDamageApplied (0x9cbd437b, stride 0x1c)
RuntimeClaim: producer `FUN_00538b50` writes `{claimValue float, claimantId, +1}` when a unit claims an
area (12 B). RuntimeLastDamageApplied: 28-byte raw-copy record of the most recent damage event (amount,
source, type, position) — read by AI/feedback systems.

---

## Notes / unknowns

- Builder cells marked `FUN_?` were not individually captured for the non-priority group; each is the
  single function whose body matches the descriptor block at the listed base (locatable via
  `_DAT_<base+0x3c> = s_<Class>_…`). The descriptor base, CopyFromStream pointer, stride, and schema/
  producer functions are all confirmed.
- `RuntimeFlightNoise`, `RuntimeOwnerGuid`, `RuntimeTerrainBound`, `RuntimeClaim` schema-tail refs land
  inside large gameplay/render functions (≈0x53xxxx / 0x66xxxx) — they are **runtime producers**, so they
  have a memory stride but no level-stream field schema. Their exact field breakdowns beyond the inferred
  shapes are not fully decompiled here and are flagged unknown.
- The `stride` for bool-only schemas (NetCategoryInfo) and HibernationControl is a packed size and does
  not linearly equal field count; trust the ordered schema for field identity.
