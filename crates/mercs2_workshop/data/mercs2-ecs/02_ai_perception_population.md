# Mercenaries 2 ECS — AI / Perception / Population components

Family: **AI / perception / population** reflection components (PC, x86; the game EXE (uncracked v1.1), ImageBase `0x400000`).

All addresses below are virtual addresses in the retail PC exe. Decomp citations are line numbers in
the Ghidra decompilation of the game EXE. Hashes from `tools/pandemic_hash.py --m2 <Class>`.

## How these classes are wired (verified)

Each component has three pieces:

1. **Descriptor builder** (`FUN_0063f4xx` / `FUN_0064xxxx` / `FUN_0064xxxx`), e.g. AiBehavior = `FUN_00640b90`
   @`0x00640b90` (decomp 295560). It zero-inits a **0x50-byte descriptor** whose base = `name_string_addr - 0x3c`,
   then writes:
   - `desc[+0x00]` = `&PTR_CopyFromStream_<addr>`  — the binary deserializer vtable for the class.
   - `desc[+0x0c]` = `3`, `desc[+0x10]/[+0x14]` = `0x100`.
   - `desc[+0x24]` = **stride** (byte size of the streamed record / per-field cursor width — value differs per class).
   - `desc[+0x26]` = `8`, `desc[+0x28]` = `0x100`.
   - `desc[+0x2c]` = seed **`0x9e3779b9`** (golden-ratio hash seed, present in every class).
   - `desc[+0x18]` = `&PTR_FUN_00bc5ff8` (shared vtable), then `FUN_0064a770()` registers it.
   - `desc[+0x3c]` = `s_<ClassName>_<straddr>` (the reflection class name, matched by hash at data-load).

2. **Field-schema template** (`FUN_0065xxxx` / `FUN_00660xxx`). Declares fields **in order** with defaults:
   - `FUN_00656210(intDefault)` → **int32** field
   - `FUN_00656320(floatDefault)` → **float** field (default is a `DAT_*` float constant or immediate)
   - `FUN_00656720(enumTableName, enumDefault)` → **enum** field
   - `FUN_00656890(0)` → **bool/bit** field (packed)
   - `FUN_00656610(0)` → **vec3** field (returns 3 dwords)
   - terminates with `FUN_0064a600(param_1, …)` and a `FUN_00665590` change-notify.
   The template is **matched to its class** by the tail line `iVar1 = DAT_<base+0x1c>` (== descriptor base + 0x1c)
   and `FUN_00665590(param_1,(int)DAT_<base+4>)`.

3. **Binary CopyFromStream reader** (the `289xxx–293xxx` cluster), one per class, calls
   `(**(code**)(*param_2+0x14))(localbuf, STRIDE, 0)` — the second arg is the same per-class **stride**.

Enum **member name** strings live contiguously just before/around the enum-type-name string in `.rdata`
(recovered directly from the exe). Member **counts/values** come from the enum-registration block
(`FUN_0049xxxx`, decomp ~302300–303700): each `_DAT_00edc6cc = N` gives the member count, `DAT_00edc6d4[2k+1]`
the integer values.

Runtime/networked components (**AiUnUsable, Suspect, Rt\***, **RuntimeTravelGroup**) do **not** use a static
`FUN_0065xxxx` schema template — their record is filled at runtime (server/living-world), so only the descriptor
(stride, hash, deserializer) is documented; field lists are flagged where unknown.

---

## Registry (all 25 manifest components)

| Class | m2 hash | descriptor base (name@) | stride (desc+0x24) | schema template / reader | one-line purpose |
|---|---|---|---|---|---|
| AiBehavior | `0xdecd8889` | `0x017bd218` (name@`0x017bd254`) | `0x30` (48) | `FUN_0065b5e0` / rdr 290815 | 12 boolean AI behavior toggles |
| AiDriving | `0x67ab955c` | `0x017bc908` (name@`0x017bc944`) | `0x08` (8) | `FUN_0065d090` / rdr 289924 | vehicle-AI driving tuning (2 floats) |
| AiHelicopter | `0x78eb1adc` | `0x017bc8b8` (name@`0x017bc8f4`) | `0x24` (36) | `FUN_0065cf30` / rdr 289903 | helicopter-AI flight tuning (9 floats) |
| AiPatrol | `0xb0ca290d` | `0x017bc868` (name@`0x017bc8a4`) | `0x18` (24) | `FUN_0065ce20` / rdr 289882 | patrol route mode + priority |
| AiSkill | `0xeba09b1a` | `0x017bc818` (name@`0x017bc854`) | `0x04` (4) | `FUN_0065cd80` / rdr 289861 | single AI skill/accuracy float (10.0) |
| AiUnUsable | `0x4a548962` | `0x017c0558` (name@`0x017c0594`) | `0x01` (1) | runtime (`PTR_CopyFromStream_00bc44b8`) | marks entity AI-unusable (1-byte flag) |
| AiWaterZone | `0xdf6533de` | `0x017bd678` (name@`0x017bd6b4`) | `0x04` (4) | `FUN_0065c520` / rdr 291189 | water-zone AI type enum |
| ChatterSet | `0x949a1e44` | `0x017bd498` (name@`0x017bd4d4`) | `0x04` (4) | `FUN_0065bdc0` / rdr 290983 | radio-chatter set id (1 int) |
| MeleeCombatant | `0xbf438e92` | `0x017be168` (name@`0x017be1a4`) | `0x28` (40) | `FUN_00661d80` / rdr 292179 | melee combat tuning (10 floats) |
| Perception | `0x3f6ab8f0` | `0x017bd2b8` (name@`0x017bd2f4`) | `0x14` (20) | `FUN_0065b8f0` / rdr 290857 | sight/awareness ranges (3 floats + 1 + int) |
| PopulationDensity | `0x6fa2f9d4` | `0x017bdad8` (name@`0x017bdb14`) | `0x1c` (28) | `FUN_00660980` / rdr 291618 | crowd/traffic density + traffic-control |
| PopulationDynamicRoad | `0xffc5baa5` | `0x017bdb78` (name@`0x017bdbb4`) | `0x0c` (12) | `FUN_00660b20` / rdr 291681 | dynamic road type (overpass/wall) |
| PopulationFlow | `0x322750ec` | `0x017bdbc8` (name@`0x017bdc04`) | `0x0c` (12) | `FUN_00660eb0` / rdr 291722 | traffic-flow control (stop/light) |
| RtLivingWorld | `0x115b2b5c` | `0x017bfe78` (name@`0x017bfeb4`) | `0x10` (16) | runtime (`PTR_CopyFromStream_…`) | living-world runtime state (server) |
| RtPopHint | `0x036dc9cb` | `0x017bfa18` (name@`0x017bfa54`) | `0x01` (1) | runtime | runtime population hint flag |
| RtPopMembership | `0x8c8e5490` | `0x017bfa68` (name@`0x017bfaa4`) | `0x14` (20) | runtime (inline @132018) | runtime population-group membership |
| RuntimeTravelGroup | `0x5f187fa4` | `0x017bf658` (name@`0x017bf694`) | `0x08` (8) | runtime (`PTR_CopyFromStream_00bc2e80`) | runtime traveling-NPC group |
| SkirmishSpawnList | `0xafba5846` | `0x017bd538` (name@`0x017bd574`) | `0x18` (24) | `FUN_0065bf00` / rdr 291025 | skirmish spawn-list (6 ints) |
| SkirmishZone | `0xfc5923af` | `0x017bd4e8` (name@`0x017bd524`) | `0x08` (8) | `FUN_0065be50` / rdr 291004 | skirmish zone (1 float + 1 int) |
| SocialUse | `0x7e6bf93d` | `0x017bd448` (name@`0x017bd484`) | `0x10` (16) | `FUN_0065bce0` / rdr 290962 | social-prop "need" use point |
| Squad | `0x9788c501` | `0x017bc958` (name@`0x017bc994`) | `0x04` (4) | `FUN_0065d140` / rdr 289945 | squad max-size (1 int = 50) |
| Stimulus | `0x06408d71` | `0x017bd308` (name@`0x017bd344`) | `0x0c` (12) | `FUN_0065b9e0` / rdr 290878 | perception stimulus (2 floats) |
| StimulusModifier | `0xb9388f0a` | `0x017bd358` (name@`0x017bd394`) | `0x18` (24) | `FUN_0065ba90` / rdr 290899 | stimulus range/falloff modifier (6 floats) |
| Suspect | `0x1afc276c` | `0x017c05a8` (name@`0x017c05e4`) | `0x20` (32) | runtime (inline @197148, `PTR_CopyFromStream_00bc4508`) | per-faction suspicion state (8 dwords) |
| Target | `0xaff6b246` | `0x017bd3a8` (name@`0x017bd3e4`) | `0x04` (4) | `FUN_0065bba0` / rdr 290920 | targetable flag (1 bool = True) |

> Note: descriptor base = `name_addr − 0x3c` for the static-schema classes; the runtime classes share the same
> 0x50 layout but with `desc[+0x08]` pre-set (e.g. AiUnUsable `0x28`, Suspect `0x18`).

---

## Enum tables (recovered from exe `.rdata`)

Member **counts** are exact (from enum-registration block). Member **order/values** are as laid out in `.rdata`;
where the count exceeds the contiguous strings recovered, the extra members are scattered in the string pool and are
flagged. Enum-type-name string addresses are the `s_*Enum_*` labels referenced by `FUN_00656720`.

| Enum | count | members (value order, recovered) | used by |
|---|---|---|---|
| `AiPatrolModeEnum` @`0xbc6834` | 3 | `Loop`(0), `Bounce`(1), [+1 scattered] | AiPatrol |
| `AiPriorityEnum` @`0xbc6854` | 5 | `Low`, `High`, [+3 scattered] | AiPatrol |
| `AiWaterZoneEnum` @`0xbc61e4` | 2 | (members scattered; type @`0xbc61e4`) | AiWaterZone |
| `AiHintEnum` @`0xbc61d8` | 4 | `Movement`, `MovementPortal`, `FirePoint`, `CowerPoint` | (AI hint nodes; see FUN_0065c1xx) |
| `NeedTypeEnum` @`0xbc6118` | 5 | `…`, `SHADE`, `CONTACT`, `ACTIVITY`, `TRASH`, `EXIT` (5 of these) | SocialUse |
| `TrafficControlEnum` @`0xbc75fc` | (6) | `Default`, `NoTraffic`, `NoVehicles`, `NoPeds`, `BanFaction`, `SingleFaction` | PopulationDensity |
| `DynamicRoadTypeEnum` @`0xbc675c` | (2) | `Overpass`, `Wall` | PopulationDynamicRoad |
| `FlowControlTypeEnum` @`0xbc6734` | (2) | `StopSign`, `TrafficLight` | PopulationFlow |

(Cover*/Spawner* enums are documented in the WeaponProjectile / spawner families; included here only where an
AI/population class references them.)

---

## Per-component field schemas

Defaults: float `DAT_*` constants resolved from the exe are shown as their float value. A handful of frequently
used `DAT_*` floats (`DAT_00bbb99c`, `DAT_00d2d840`, `DAT_00d2d898`, `DAT_00d2d820`, `DAT_00bad270`) read back as
non-sensible bit patterns in the static image (`.rdata`/`.data`) and are flagged **[unresolved const]**; they are
populated/relocated at runtime and the exact default could not be proven statically.

### AiBehavior  (`FUN_0065b5e0`, decomp 309750; reader 290814; stride 0x30)
12 enum fields, **all `BoolEnum`, default `False`** (0). Twelve independent AI behavior toggles
(e.g. can-flee / can-take-cover / can-call-reinforcements style flags; individual labels not separately named —
they share the generic `BoolEnum`). This is the master "what is this AI allowed to do" flag block.

```
field 0..11 : enum BoolEnum = False   (×12)
```

### AiPatrol  (`FUN_0065ce20`, decomp 310513; reader 289881; stride 0x18)
```
field 0 : int    = 0
field 1 : bool   = 0                       (FUN_00656890)
field 2 : enum   AiPatrolModeEnum = Loop?  (default &DAT_00bc6834; members Loop/Bounce/+1)
field 3 : bool   = 0                       (FUN_00656890)
field 4 : float  = [unresolved const DAT_00bbb99c]
field 5 : enum   AiPriorityEnum = Low?     (default &DAT_00bc6858; members Low/High/+3)
```
Patrol = route traversal **mode** (Loop vs Bounce) + an AI scheduling **priority** (Low..High).

### AiSkill  (`FUN_0065cd80`, decomp 310491; reader 289861; stride 0x04)
```
field 0 : float = 10.0   (DAT_00b9c174)
```
Single skill/competence scalar (default **10.0**). Smallest AI tuning component.

### AiDriving  (`FUN_0065d090`, decomp 310570; reader 289924; stride 0x08)
```
field 0 : float = 0.0
field 1 : float = [unresolved const DAT_00bbb99c]
```

### AiHelicopter  (`FUN_0065cf30`, decomp 310540; reader 289903; stride 0x24)
9 floats — helicopter flight envelope tuning:
```
f0 = 10.0  (DAT_00b9c174)
f1 = 80.0  (DAT_00b9b714)
f2 = 20.0  (DAT_00b9b980)
f3 = 30.0  (DAT_00b977d0)
f4 = 20.0  (DAT_00b9b980)
f5 = 5.0   (DAT_00b9b700)
f6 = 1.0   (0x3f800000)
f7 = 0.0   (DAT_00dfdb5c)
f8 = 20.0  (DAT_00b9b980)
```

### AiWaterZone  (`FUN_0065c520`, decomp 310229; reader 291188; stride 0x04)
```
field 0 : enum AiWaterZoneEnum = 0
```

### Perception  (`FUN_0065b8f0`, decomp 309823; reader 290856; stride 0x14)
```
f0 : float = 1.0  (0x3f800000)
f1 : float = 1.0
f2 : float = 1.0
f3 : float = 120.0 (DAT_00b9851c)     <-- looks like a perception RANGE/distance (120 units)
i4 : int   = 0
```
Three unit-scale multipliers + a **120.0** range + a mode int. Drives entity sight/awareness.

### Stimulus  (`FUN_0065b9e0`, decomp 309849; reader 290877; stride 0x0c)
```
f0 : float = 100.0 (DAT_00b92870)     <-- stimulus strength/radius
f1 : float = 40.0  (DAT_00b9b724)
```

### StimulusModifier  (`FUN_0065ba90`, decomp 309872; reader 290898; stride 0x18)
```
f0 : float = 2.0   (DAT_00b92874)
f1 : float = 0.0
f2 : float = 0.30  (DAT_00b9b688)
f3 : float = 1.20  (DAT_00baab38)
f4 : float = 0.80  (DAT_00b9852c)
f5 : float = [unresolved const DAT_00d2d840]
```
Scales/falls-off an incoming Stimulus (multiplier 2.0, falloffs 0.3 / 1.2 / 0.8).

### Target  (`FUN_0065bba0`, decomp 309899; reader 290919; stride 0x04)
```
field 0 : enum BoolEnum = True   (default &DAT_00bc607c)
```
Marks an entity as a valid AI target (default **on**).

### SocialUse  (`FUN_0065bce0`, decomp 309943; reader 290961; stride 0x10)
```
field 0 : enum  NeedTypeEnum = 0   (SHADE/CONTACT/ACTIVITY/TRASH/EXIT — what social "need" this point satisfies)
f1     : float = 5.0  (DAT_00b9b700)
f2     : float = 30.0 (DAT_00b977d0)
f3     : float = 5.0  (DAT_00b9b700)
```
A prop/anchor that civilians use to satisfy a "need"; range/duration floats 5 / 30 / 5.

### ChatterSet  (`FUN_0065bdc0`, decomp 309968; reader 290982; stride 0x04)
```
field 0 : int = 0
```
Selects a radio-chatter/voice set id.

### Squad  (`FUN_0065d140`, decomp 310593; reader 289944; stride 0x04)
```
field 0 : int = 0x32 (50)
```
Squad capacity / max members, default **50**.

### MeleeCombatant  (`FUN_00661d80`, decomp 313047; reader 292178; stride 0x28)
10 floats — melee fight tuning:
```
f0 = 1.0   (0x3f800000)
f1 = 0.0   (DAT_00dfdb60)
f2 = [unresolved const DAT_00d2d898]
f3 = [unresolved const _DAT_00bad270]
f4 = 0.0
f5 = 2.0   (DAT_00b92874)
f6 = 1.5   (DAT_00b9c650)
f7 = [unresolved const DAT_00dfddc8 = 0.0]
f8 = 2.0   (DAT_00b92874)
f9 = 1.5   (DAT_00b9c650)
```
(Likely: reach, windup, cooldown, damage, range pairs — strength 1.0, multipliers 2.0/1.5.)

### SkirmishZone  (`FUN_0065be50`, decomp 309990; reader 291003; stride 0x08)
```
f0 : float = 0.0
i1 : int   = 0
```

### SkirmishSpawnList  (`FUN_0065bf00`, decomp 310013; reader 291024; stride 0x18)
```
i0..i5 : int = 0   (×6)
```
Six spawn-list slot ints (faction/unit/count indices).

### PopulationDensity  (`FUN_00660980`, decomp 312395; reader 291617; stride 0x1c)
```
i0..i4 : int = 0                              (×5 density/cap counters)
field5 : enum TrafficControlEnum = Default    (Default/NoTraffic/NoVehicles/NoPeds/BanFaction/SingleFaction)
i6     : int = 0
```
Controls crowd/vehicle density and traffic rules in a population zone.

### PopulationDynamicRoad  (`FUN_00660b20`, decomp 312445; reader 291680; stride 0x0c)
```
field 0 : enum DynamicRoadTypeEnum = Overpass   (Overpass/Wall)
i1      : int = 0
i2      : int = 0
```

### PopulationFlow  (`FUN_00660eb0`, decomp 312541; reader 291721; stride 0x0c)
```
field 0 : enum FlowControlTypeEnum = 0   (StopSign/TrafficLight)
i1      : int = 0
i2      : int = 0
```

---

## Runtime / networked components (no static schema template)

These share the descriptor layout but their records are produced at runtime (server replication / living-world
manager), so there is no `FUN_0065xxxx` default-field template.

### Suspect  (descriptor `FUN_006482b0` @300834; deserializer `PTR_CopyFromStream_00bc4508`; stride 0x20)
Runtime fill at decomp **197148** (`FUN_00589573`) and **102540**: an array of **8 dwords** copied from
`&DAT_00b9b694` (per-index seed), indexed by `unaff_ESI`. This is **per-faction suspicion / wanted state**
(8 factions × 1 dword each). `DAT_017c05c8` is the live entry count; `DAT_017c05dc` the entry array base.

### AiUnUsable  (descriptor `FUN_006481f0` @300801; `PTR_CopyFromStream_00bc44b8`; stride 0x01)
A 1-byte marker component flagging an entity as currently not usable by AI (e.g. dead/disabled body).
`desc[+0x08]=0x28` pre-set.

### RtPopHint  (descriptor @299650 region; stride 0x01) / RtPopMembership (@299683; stride 0x14)
Runtime population components. RtPopMembership runtime fill is inlined at decomp **132018**
(`iVar1 = DAT_017bfa84`). RtPopHint = a 1-byte runtime hint; RtPopMembership = which runtime population group an
NPC belongs to (20-byte record).

### RtLivingWorld  (descriptor @300122; stride 0x10) / RuntimeTravelGroup (descriptor `FUN_00645d70` @299271; `PTR_CopyFromStream_00bc2e80`; stride 0x08)
Living-world / traveling-NPC-group runtime state, managed by the living-world subsystem (RtLivingWorld inline
fill near decomp **318428**, `DAT_017bfe94`). No authored defaults.

---

## Cross-references / usage notes

- The class **name strings** (`s_AiBehavior_*`, `s_Perception_*`, …) are referenced **only** by their descriptor
  builders. This reflection system resolves components by **m2 hash**, not by C++ symbol xref, so there are no
  direct call-site references to chase — the hash (see registry) is the lookup key used by the data loader.
- `Target` additionally gets a script/Lua binding: `FUN_0072e740(s_Target_00bb4d24)` at decomp **432265**.
- The shared deserialize plumbing: `FUN_00656210` (int), `FUN_00656320` (float), `FUN_00656720` (enum),
  `FUN_00656890` (bool/bit), `FUN_00656610` (vec3), terminator `FUN_0064a600`, change-notify `FUN_00665590`.
- Enum registration block: decomp ~**302300–303700** (`thunk_FUN_004935d1(s_<Enum>)` after each `_DAT_00edc6cc = N`
  member-count assignment).

### Headline AI tuning defaults
- **AiSkill** = 10.0 (single competence scalar) · **Squad** = 50 (max members) · **Perception** range = 120.0 ·
  **Stimulus** strength/radius = 100.0 (falloff 40.0) · **StimulusModifier** = ×2.0 with 0.3/1.2/0.8 falloffs ·
  **AiBehavior** = 12× boolean toggles (all default False) · **AiPatrol** = mode {Loop,Bounce} + priority {Low,High}.
