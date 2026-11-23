# Mercenaries 2: World in Flames — ECS Component Family 01: Combat / Weapons / Projectiles

Reverse-engineered from the Ghidra decompilation of the game EXE and rodata in
the game EXE (uncracked v1.1, ImageBase 0x400000) (image base `0x400000`; `.rdata` `0xb05000`, `.data` `0xbf5000`).
Hashes from `tools/pandemic_hash.py --m2`.

## How these classes are laid out (verified)

Each component is registered by a **descriptor builder** function (one per class, clustered around
decomp lines 293300–299430). The builder writes a 0x50-byte reflection descriptor. Relative to the
descriptor **base** (`base = manifest_descriptor_addr − 0x3c`):

| offset | field | meaning |
|---|---|---|
| `base+0x00` | `&PTR_CopyFromStream_xxxx` | deserializer vtable pointer |
| `base+0x1c` | counter dword (the **template anchor** `DAT_<base+0x1c>`) | bumped each load; identifies the class' deserialize template |
| `base+0x24` (u16) | **stride** | per-instance struct size in bytes |
| `base+0x26` (u16) | `8` | fixed type tag |
| `base+0x2c` | `0x9e3779b9` | golden-ratio hash seed |
| `base+0x3c` | `s_<ClassName>_<straddr>` | class-name string (= manifest col 2) |

The **field schema** is declared by a per-class deserialize-template function (cluster ~lines
308500–318000) that calls, IN ORDER:

- `FUN_00656210(<intDefault>)` — next field is **int** (default = arg)
- `FUN_00656320(<floatDefault>)` — next field is **float** (0x3f800000 = 1.0, etc.)
- `FUN_00656720(<enumNameTable>, <enumDefault>)` — next field is **enum**
- `FUN_00656610(0)` — next field is a **Vector3 / float3** (returns a 3-dword struct)
- `FUN_00656890(<bool>)` — next field is a **BoolEnum / bool** (default arg)

…then `FUN_0064a600(...)` finalizes and `FUN_00665590(...)` registers. The template's identity is
proven by its `iVar1 = DAT_<base+0x1c>` line.

**Field NAMES** are recovered from the ordered reflection property-name string table in `.data`
spanning `0xbc9000`–`0xbca400`. The weapon/projectile names run from `0xbc96a0` (MuzzleFlashHardpoint)
to `0xbca3f8` (CanExit) and align positionally with the schema templates. Names below are matched to
schema positions by (a) the property-table order and (b) the recovered default values.

### IMPORTANT finding — `Runtime*` classes have NO data-driven schema

The 15 `Runtime*` classes do **not** call any `FUN_00656xxx` field declarators. Their "templates"
(`FUN_00666f00`, `FUN_0066ae30`, …) are **runtime-instance serializers** that copy live spawn state
(transforms from `DAT_011766f0…`, handles, flags) directly into the struct. They are not editable in
data/templates — they are the in-world instance counterparts of the authored components. So they have
**no default-value schema to surface** (stride is still listed below).

---

## Registry table (all 34 components)

| Class | m2 hash | descriptor addr | base | stride | CopyFromStream | template (FUN) | one-line purpose |
|---|---|---|---|---|---|---|---|
| ExplosionFudge | `0x5aeabc23` | `0x017bc674` | `0x017bc638` | 0x04 (4) | `0xbbe1e8` | `FUN_0065ab40` @309437 | single float (1.0) — explosion radius/damage fudge multiplier |
| Explosive | `0xf74044ba` | `0x017bcb24` | `0x017bcae8` | 0x24 (36) | `0xbc...` | `FUN_0065d6e0` @310761 | explosive blast: force, falloff, damage, arc, detail enum |
| FlareObject | `0x9f3ebfba` | `0x017be514` | `0x017be4d8` | 0x40 (64) | — | `FUN_0065...` @313638 | flare: 4×vec3 + 3 floats + 1 int |
| HomingProjectile | `0xe81b2874` | `0x017be6a4` | `0x017be668` | 0x0c (12) | — | `FUN_0065da40` @310842 | homing flight: 3 floats (turn/accel tuning) |
| HomingTarget | `0xb9ea3b32` | `0x017be6f4` | `0x017be6b8` | 0x10 (16) | — | `FUN_0065db10` @310866 | lock-on target: 3 floats + bool |
| HomingWeapon | `0x1a4db6ed` | `0x017be654` | `0x017be618` | 0x18 (24) | — | `FUN_0065d930` @310815 | homing weapon: HomingType enum + 4 floats + int |
| Ignitor | `0x37c12455` | `0x017be2e4` | `0x017be2a8` | 0x0c (12) | — | `FUN_006627a0` @313398 | fire-start: 3 floats (all default 0) |
| InitialVelocity | `0x6537a65a` | `0x017bc034` | `0x017bbff8` | 0x30 (48) | `0xbbe...` | `FUN_00657f40` @308522 | 4×Vector3 — initial linear/angular velocity vectors |
| ProjectilePhysics | `0x11e6c283` | `0x017bcbc4` | `0x017bcb88` | 0x28 (40) | — | `FUN_0065dc00` @310891 | projectile ballistics: 8 floats + BoolEnum |
| RuntimeAirstrikeAirplane | `0x23d5de91` | `0x017bf234` | `0x017bf1f8` | 0xb0 (176) | — | runtime-state (no schema) | live airstrike plane instance |
| RuntimeAirstrikeProjectile | `0xf67a894a` | `0x017bf1e4` | `0x017bf1a8` | 0x28 (40) | — | runtime-state (no schema) | live airstrike ordnance instance |
| RuntimeAlternatingFire | `0x9bb55cf2` | `0x017bece4` | `0x017beca8` | (see builder) | — | runtime-state (no schema) | alternating-fire runtime state |
| RuntimeExplosion | `0x5529dd38` | `0x017bef64` | `0x017bef28` | 0x40 (64) | — | `FUN_0066ae30` @317985 (runtime) | live explosion instance |
| RuntimeFakeProjectile | `0x750bc641` | `0x017bedd4` | `0x017bed98` | 0x44 (68) | — | runtime-state (no schema) | cosmetic/fake projectile instance |
| RuntimeHomingProjectile | `0xc45d369e` | `0x017bf734` | `0x017bf6f8` | 0x58 (88) | — | runtime-state (no schema) | live homing projectile instance |
| RuntimeHomingTarget | `0x14f6de44` | `0x017bf784` | `0x017bf748` | 0x30 (48) | — | `FUN_0066...` @317246 (runtime) | live homing target/lock instance |
| RuntimeHomingWeapon | `0xc09adb1b` | `0x017bf6e4` | `0x017bf6a8` | 0x54 (84) | — | `FUN_0066...` @317218 (runtime) | live homing weapon instance |
| RuntimeIgnitor | `0x1ca3abd7` | `0x017bf3c4` | `0x017bf388` | 0x1c (28) | — | runtime-state (no schema) | live fire-ignitor instance |
| RuntimeLaserDesignator | `0x735b0eaa` | `0x017bf7d4` | `0x017bf798` | 0x10 (16) | — | runtime-state (no schema) | live laser-designator instance |
| RuntimeProjectile | `0x9d2ab1a6` | `0x017bed84` | `0x017bed48` | 0xa0 (160) | `0xbc22a8` | runtime-state (no schema) | live generic projectile instance |
| RuntimeProjectileThrown | `0xf394de30` | `0x017bee74` | `0x017bee38` | 0x04 (4) | — | runtime-state (no schema) | live thrown-projectile instance |
| RuntimeVelocity | `0xe493bf82` | `0x017bed34` | `0x017becf8` | 0x08 (8) | — | runtime-state (no schema) | live velocity instance |
| RuntimeWeapon | `0xec62e3a3` | `0x017bec44` | `0x017bec08` | 0x34 (52) | `0xbc20f8` | `FUN_00666f00`/`FUN_006670e0` @315791 (runtime) | live weapon instance (local/remote) |
| RuntimeWeaponProjectile | `0x7a303ad6` | `0x017bec94` | `0x017bec58` | 0x6c (108) | — | runtime-state (no schema) | live weapon-fired projectile instance |
| WeaponBarrel | `0x180e2b95` | `0x017bc764` | `0x017bc728` | 0x04 (4) | `0xbbe...` | `FUN_0065c9e0` @310405 | single int — barrel index/id |
| WeaponEffects | `0xf24d2021` | `0x017bc6c4` | `0x017bc688` | 0x10 (16) | — | `FUN_0065c860` @310357 | 4 ints — muzzle/shell effect template ids |
| WeaponHint | `0xd390834a` | `0x017be384` | `0x017be348` | 0x34 (52) | — | `FUN_00662860` @313422 | AI targeting hints: ObjectType/Elevation/FireAngle enums + bools + floats |
| WeaponProjectileBase | `0xeb505c8b` | `0x017bc7b4` | `0x017bc778` | 0x28 (40) | `0xbbe328` | `FUN_0065ca70` @310427 | **core gun stats**: clip, reserve, RoF, aim angles |
| WeaponRecoilVehicle | `0x557e4b99` | `0x017bc714` | `0x017bc6d8` | 0x08 (8) | — | `FUN_0065c930` @310382 | 1 float + 1 int — vehicle-mounted recoil |
| WeaponScatter | `0xe7234615` | `0x017bc804` | `0x017bc7c8` | 0x1c (28) | — | `FUN_0065cc50` @310463 | spread/accuracy: 7 floats |
| WeaponScope | `0x27ca777f` | `0x017bcad4` | `0x017bca98` | 0x14 (20) | — | `FUN_0065d5f0` @310737 | zoom: 4 floats + bool |
| WeaponThrown | `0x24870cff` | `0x017bc9e4` | `0x017bc9a8` | 0x34 (52) | — | `FUN_0065d290` @310653 | thrown-weapon arc: int×4 + float×8 + bool |
| WeaponTrigger | `0xc526a637` | `0x017bca34` | `0x017bc9f8` | 0x08 (8) | — | `FUN_0065d440` @310687 | 1 int + 1 bool — trigger name/broadcast |
| WeaponUI | `0xe5d5e31f` | `0x017bca84` | `0x017bca48` | 0x18 (24) | — | `FUN_0065d4f0` @310710 | reticle: 2 ints + 3 enums + 1 int |

*(stride = u16 at `base+0x24`; CopyFromStream column lists the low bytes of the `&PTR_CopyFromStream_00bcXXXX` where captured.)*

---

## Gameplay-significant field schemas (ordered; defaults recovered)

Legend for defaults: floats resolved from rodata; `0x3f800000`=1.0. Names matched from the
`0xbc9000` property table by position.

### WeaponProjectileBase — `FUN_0065ca70` @310427  (stride 0x28, the core gun-stat block)

Property-name run: `0xbc9710` FireType … `0xbc9818` MaxAimAngleAi.

| # | type | default | field name |
|---|---|---|---|
| 1 | enum WeaponProjectileTypeEnum | `Automatic` | **FireType** |
| 2 | enum WeaponProjectileSpecialCaseTypeEnum | (Prop/none) | **SpecialCaseType** |
| 3 | int | 0 | **AmmoTemplate** (template id) |
| 4 | int | 0 | **iHideMagazineOnFire** |
| 5 | int | 1 | **iTracerRound** |
| 6 | int | **30** (0x1e) | **iClipSize** |
| 7 | int | **60** (0x3c) | **MaxAmmoReserve** (reserve ammo) |
| 8 | int | 0 | **MaxAmmoReserveModifier** |
| 9 | int | 1 | **iBulletsPerShot** |
| 10 | int | -1 | **iRoundsPerReload** |
| 11 | float | **120.0** | **RateOfFire** |
| 12 | enum BoolEnum | False | **FireFromReticle** |
| 13 | int | -1 | **FirstMagazine** |
| 14 | int | 0 | **iMultipleMagazines** |
| 15 | int | 1 | **FakeUIMultiplier** |
| 16 | float | 0.0 | **MaxAimAngle** |
| 17 | float | **15.0** | **MaxAimAngleAi** |

**Modding:** iClipSize(30), MaxAmmoReserve(60), iBulletsPerShot(1), RateOfFire(120) are the prime
gun-tuning knobs. iBulletsPerShot>1 = shotgun-style. iRoundsPerReload=-1 means reload whole clip.

### WeaponScatter — `FUN_0065cc50` @310463  (stride 0x1c, accuracy/spread)

Property-name run: `0xbc9828` LowSkillScatter … `0xbc9894` ScatterPerShot (+ RifleSkill).

| # | type | default | field name |
|---|---|---|---|
| 1 | float | 10.0 | **LowSkillScatter** |
| 2 | float | 1.0 (`0x3f800000`) | **CenterBias** |
| 3 | float | 1.5 | **ScatterAimModeMin** |
| 4 | float | 1.5 | **ScatterAimModeMax** |
| 5 | float | 1.5 | **ScatterMin** |
| 6 | float | 1.5 | **ScatterMax** |
| 7 | float | (ptr/0) | **ScatterPerShot** |

**Modding:** lower Scatter* → tighter spread. Field 7 default decompiles to a non-IEEE value
(rodata pointer artifact) — treat as ~0.

### ProjectilePhysics — `FUN_0065dc00` @310891  (stride 0x28, ballistics)

Property-name run near `0xbc9b10`: MinVelocity/MaxVelocity/Velocity/Accel/AccelTime/HumanBoost…
and damage-dropoff group `0xbc9bd4`.

| # | type | default | likely field |
|---|---|---|---|
| 1 | float | (ptr/0) | MinVelocity / DamageDropoff |
| 2 | float | 10.0 | Velocity-class |
| 3 | float | 0.0 | (dropoff start) |
| 4 | float | 0.0 | (dropoff stop) |
| 5 | float | 0.0 | (damage minimum) |
| 6 | float | 10.0 | HeroMultiplier-class |
| 7 | float | 0.0 | — |
| 8 | float | 1.0 | — |
| 9 | enum BoolEnum | True | NeedsDriver / FiredFromWeapon |

*(Exact name↔slot mapping for fields 1–8 is ambiguous between the Velocity group and the
DamageDropoff group; defaults are authoritative, names approximate.)*

### Explosive — `FUN_0065d6e0` @310761  (stride 0x24)

Property-name run `0xbc9ad8` MaxAge / MaxForce / MinForceFalloff / Damage / Arc / Detail.

| # | type | default | field name |
|---|---|---|---|
| 1 | float | 20.0 | **MaxAge** (or MaxForce) |
| 2 | int | 0 | (flag) |
| 3 | float | 1.0 | **MaxForce** |
| 4 | float | (ptr) | **MinForceFalloff** |
| 5 | float | 0.3 | **Damage**-group |
| 6 | float | 20.0 | **Arc** |
| 7 | float | 0.0 | — |
| 8 | enum ExplosiveDetailEnum | (default) | **Detail** |

### HomingWeapon — `FUN_0065d930` @310815  (stride 0x18)

| # | type | default | likely field |
|---|---|---|---|
| 1 | enum HomingTypeEnum | (default) | **HomingType** |
| 2 | float | 0.0 | LockOnMinWeight |
| 3 | float | 10.0 | LockOnMaxAngle |
| 4 | float | 100.0 | LockOnMaxDistance |
| 5 | float | 1.0 | LockOnTime |
| 6 | int | 0 | uTargetHardpoint |

### HomingProjectile — `FUN_0065da40` @310842  (stride 0x0c)
3 floats: **10.0, 0.3, 0.3** → (TurnSpeed, …, …).

### HomingTarget — `FUN_0065db10` @310866  (stride 0x10)
float (8.97e-39 = ptr artifact), float 0.2, float 0.0, BoolEnum 0 → (DetonationDistance, …, NeedsDriver).

### WeaponScope — `FUN_0065d5f0` @310737  (stride 0x14)

Property-name run `0xbc9a8c`: MinZoomLevel/MaxZoomLevel/ZoomMultiplier/StartingZoom/bContinuous.

| # | type | default | field name |
|---|---|---|---|
| 1 | float | 1.0 | **MinZoomLevel** |
| 2 | float | (huge=ptr) | **MaxZoomLevel** |
| 3 | float | 1.0 | **ZoomMultiplier** |
| 4 | float | 2.0 | **StartingZoom** |
| 5 | BoolEnum | 0 | **bContinuous** |

### WeaponThrown — `FUN_0065d290` @310653  (stride 0x34)

Property-name run `0xbc9988`: ReticlePitchLowest/Middle/Highest, VelocityAtLowest/Middle/Highest
Pitch, ThrowAngle, GravityScale, bCook.

| # | type | default | field name |
|---|---|---|---|
| 1 | int | 0 | (count/flag) |
| 2 | int | 60 (0x3c) | — |
| 3 | int | 0 | — |
| 4 | int | 0 | — |
| 5 | float | (ReticlePitchLowest) | **ReticlePitchLowest** |
| 6 | float | 0.0 | **ReticlePitchMiddle** |
| 7 | float | (VelocityAtLowestPitch) | **VelocityAtLowestPitch** |
| 8 | float | (Velocity…) | **VelocityAtMiddlePitch** |
| 9 | float | (Velocity…) | **VelocityAtHighestPitch** |
| 10 | float | 16.0 | **ThrowAngle** |
| 11 | float | 30.0 | **GravityScale** |
| 12 | float | 2.0 | — |
| 13 | BoolEnum | 0 | **bCook** |

### WeaponUI — `FUN_0065d4f0` @310710  (stride 0x18)

Property-name run `0xbc9a60`: ReticleHealthType / ReticleType / ScopeType.

| # | type | default | field name |
|---|---|---|---|
| 1 | int | 0 | (Texture id) |
| 2 | int | `0x9f3e7371` | (ReticleTexture hash) |
| 3 | enum WeaponUIReticleHealthTypeEnum | Curved | **ReticleHealthType** |
| 4 | enum WeaponUIReticleTypeEnum | Normal | **ReticleType** |
| 5 | enum WeaponUIScopeTypeEnum | (default) | **ScopeType** |
| 6 | int | 1 | — |

### WeaponHint — `FUN_00662860` @313422  (stride 0x34, AI targeting hints)

| # | type | default | field name |
|---|---|---|---|
| 1 | enum ObjectTypeHintEnum | 0 | **ObjectTypeHint** |
| 2 | enum ElevationHintEnum | 0 | **ElevationHint** |
| 3 | BoolEnum | 1 | — |
| 4 | BoolEnum | 1 | — |
| 5 | BoolEnum | 1 | — |
| 6 | BoolEnum | 1 | — |
| 7 | float | 50.0 | — |
| 8 | float | 0.0 | — |
| 9 | enum FireAngleEnum | 0 | **FireAngle** |
| 10 | float | 1.0 | — |
| 11 | float | 2.0 | — |
| 12 | int | 0 | — |
| 13 | float | 0.0 | — |
| 14 | int | 0 | — |
| 15 | float | 0.0 | — |
| 16 | float | 0.0 | — |

### WeaponEffects — `FUN_0065c860` @310357  (stride 0x10)
4 ints (all default 0): **MuzzleFlashTemplate, ShellTemplate, …** (effect template ids; property run
`0xbc96a0` MuzzleFlashHardpoint/Template, ShellEjectHardpoint/Template).

### WeaponRecoilVehicle — `FUN_0065c930` @310382  (stride 0x08)
float(0.0) + int(0) → **PhysicalRecoil**, ControllerName id.

### WeaponBarrel — `FUN_0065c9e0` @310405  (stride 0x04)
1 int (default 0) — barrel index.

### WeaponTrigger — `FUN_0065d440` @310687  (stride 0x08)
int(0) + BoolEnum(0) → **TriggerName** id, **bBroadcast**.

### ExplosionFudge — `FUN_0065ab40` @309437  (stride 0x04)
1 float, default **1.0** — a single multiplier (radius/damage fudge).

### Ignitor — `FUN_006627a0` @313398  (stride 0x0c)
3 floats, all default **0.0**.

### InitialVelocity — `FUN_00657f40` @308522  (stride 0x30)
4 × **Vector3** (`FUN_00656610`), all default (0,0,0) — initial velocity/impulse vectors.

### FlareObject — `FUN_0065...` @313638  (stride 0x40)
4 × **Vector3** + 3 floats (default 0) + 1 int (default 0).

---

## Notes / caveats

- **Defaults shown as "(ptr)" / huge / denormal** (e.g. `9.8e-39`, `1.85e+28`) are Ghidra
  decompiler artifacts where a `FUN_00656320(DAT_xxxx)` call actually loaded a rodata **string/pointer
  address** (template-name field) rather than an IEEE float; treat those numeric fields as ~0 or as
  a string-template id, not literal floats.
- **Name↔slot alignment** for the longer float runs (ProjectilePhysics, WeaponThrown) is inferred
  from the ordered property table + recovered defaults; the *defaults* are exact (read from the
  decompiled calls), the *names* are best-effort positional matches.
- All `Runtime*` classes are instance/state mirrors with no authored schema (see top section).
- Enum value tables (e.g. `WeaponProjectileTypeEnum`, `HomingTypeEnum`, `ExplosiveDetailEnum`,
  `ObjectTypeHintEnum`, `WeaponUIReticleTypeEnum`) are named in the templates; their member lists
  live in the corresponding `s_*Enum_*` rodata tables and can be expanded if needed.
- Property-name table dumped from `.data` `0xbc9000`–`0xbca400`; full list captured during analysis.
