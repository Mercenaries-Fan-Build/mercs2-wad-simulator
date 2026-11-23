# Mercenaries 2 (PC) — Native ECS / Reflection Component Registry

Reverse-engineered map of the engine's **native component classes** — the reflection types the
engine registers at startup and deserializes from the level/WAD stream. This is the *native* side
that complements [`../ecs_components.md`](../ecs_components.md) (which documents the *WAD placement*
`COMP` record format). Source: the Ghidra decompilation of the game EXE (uncracked v1.1,
ImageBase `0x400000`); hashes via `tools/pandemic_hash.py --m2 <Class>`.

## How the reflection system works

At static init the engine registers ~220 component classes. For each class a **descriptor builder**
function writes `<global> = "<ClassName>"` and fills a **0x50-byte descriptor**:

- dword 0 = a **CopyFromStream** deserializer vtable pointer
- the class **name-hash** (`pandemic_hash_m2(name)`), a **stride** (record size), and the
  golden-ratio seed `0x9e3779b9`
- descriptor base ≈ `name_string_field − 0x3c`; stride at `base+0x24`

Two **dialects** of class exist — this distinction drives what's moddable:

1. **Authored / schema classes** — a per-class template declares an *ordered* field list by calling
   `FUN_00656210(intDefault)` (int), `FUN_00656320(floatDefault)` (float), `FUN_00656720(enumTable,
   default)` (enum), `FUN_00656610(rgb)`, `FUN_00656890(hash)`, then `FUN_0064a600(...)` to finalize.
   The order = the byte layout in the stream; the literal args are the **defaults** used when a field
   is absent. **Worked example:** `WeaponProjectileBase` (`FUN_0065ca70`) → enum(FireType=Automatic),
   enum(SpecialCase), int×8 (`…,30,60,…` → clip 30 / reserve 60), float, BoolEnum, int×3, float, float.
   These are where editable default values live.
2. **`Runtime*` / `Rt*` instance classes** — no authored schema; their "deserializer" either raw-copies
   a fixed-size block or *computes* the record at world-load from the live actor/asset (health, LODs,
   road children, animation deltas). No editable disk defaults — only a stride.

A third group (Controllers, Physics) reads an **opaque fixed block** decoded entirely in C++
(`CopyFromStream` target) — no reflected field names; layouts require disassembling each deserializer.

## Family index

| Doc | Components | Headline findings |
|---|---|---|
| [01_combat_weapons_projectiles.md](01_combat_weapons_projectiles.md) | 34 | `WeaponProjectileBase` core gun stats — **iClipSize=30, MaxAmmoReserve=60, RateOfFire=120, iBulletsPerShot=1**; `WeaponScatter` = 7 spread floats (ScatterMin/Max=1.5, LowSkillScatter=10); full Explosive/Homing/Thrown/Scope schemas |
| [02_ai_perception_population.md](02_ai_perception_population.md) | 25 | AI enums (PatrolMode, Priority, TrafficControl, Hint…); **AiSkill=10, Squad max=50, Perception range=120, Stimulus strength=100/falloff=40** |
| [03_controllers_physics.md](03_controllers_physics.md) | 31 | **Opaque blocks — no reflected fields**; vehicle Controllers = 4-byte handles; physics defaults (mass/friction) come from Havok PHY2 + level data, not the reflection tables |
| [04_player_vehicle_human.md](04_player_vehicle_human.md) | 23 | **No magazine/ammo-capacity field anywhere**; only slot taxonomy is `EquipmentTypeEnum {Primary=0, Secondary=1}`; `HumanInventory`=3 unnamed ints; `RuntimeVehicleInventory`=2-byte category bitmask |
| [05_presentation_audio_fx.md](05_presentation_audio_fx.md) | 34 | `SoundEffect` volume=50/pitch=1; `BlobShadow`/`LightObject` color defaults; animation curve params default 0 (authored per-instance); `Status` is a script object, not a component |
| [06_world_terrain_roads_streaming.md](06_world_terrain_roads_streaming.md) | 30 | Road graph: `RoadIntersection` (7 ints+6 vec3+6 links), `LaneData` (4 lanes); `RtGenericLOD` per-tier dist²; `Model`=`0x5b724250` (matches MESH CHDR); schema vs engine-computed split |
| [07_gameplay_state_health_mission.md](07_gameplay_state_health_mission.md) | 35 | **`Health`=1 float+3 bools; `RuntimeHealth`={max,cur} runtime-produced** (order corrected 2026-07-26) (→ why `healthpickup` Lua is empty: health is native); **`ObjectScript`=2 int32 name-hashes** (`script_hash_0`=field 0); `StateMachine`=4 name-hash int32 |
| [08_misc_uncategorized.md](08_misc_uncategorized.md) | 7 | Raw-blob components (Disable*Decals, RuntimeAnimationParams, TickDamage, TriggerOnTimer…); `Update` is a method-name string, not a component |
| [09_render_asset_pipeline.md](09_render_asset_pipeline.md) | 12 | **Not gameplay ECS:** 8 D3D9 precache resource types (vshader/pshader/vertex/vertdecl/surface/texture/index/display); `WpMeshShape` (real weapon collision-shape class, stride 0x30); + `failresolve`/`finalize`/`potential` = asset-resolution **phase strings** sharing one global (`FUN_00581fd0`), not classes |

Per-family docs carry the full registry table (Class · m2 hash · descriptor addr · stride · purpose)
plus field schemas for the authored classes. Internal data: `_registry_raw.tsv` (all 232 raw
registrations), `_manifests/` (family partitions).

## Answers this unlocked

- **Health is engine-side**, not script — the empty `healthpickup` Lua stub has no body because HP
  lives in the native `Health`/`RuntimeHealth` components (doc 07).
- **`ObjectScript` binding** (long-standing open question): the entity→script link is stored as two
  raw `int32` name-hashes; `script_hash_0` is field 0, resolved through the engine hash table. A
  chunk map can be rebuilt by hashing candidate chunk names with `pandemic_hash --m2` (doc 07).
- **Weapon magazine capacity** (clip=30 default) lives in `WeaponProjectileBase` reflection, **not**
  in any inventory component (docs 01 + 04) — consistent with the finding that capacity is an
  EXE/reflection default, not a `wpn_` WAD block value.

## Caveats

- Authored field *names* come from the EXE rodata property-name table; positions are matched to names
  by their defaults where recoverable — a few are flagged uncertain in the family docs.
- `Runtime*`/`Rt*`/Controller/Physics classes have no reflected field schema; getting their exact byte
  layout requires disassembling the per-class `CopyFromStream` target (next RE step).
- A handful of decompiled "float" defaults are Ghidra artifacts (a rodata/template pointer passed where
  a default is expected); these are flagged `(ptr)`/`[unresolved const]` rather than trusted.
