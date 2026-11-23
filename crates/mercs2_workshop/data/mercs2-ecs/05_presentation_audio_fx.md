# Mercenaries 2 — ECS Reflection Components: Presentation / Audio / FX (family 05)

Reverse-engineered from the Ghidra decompilation of the game EXE (function decomp) and the
shipping PE the game EXE (uncracked v1.1, ImageBase 0x400000) (.rdata/.data for float defaults).
Hashes via `tools/pandemic_hash.py --m2`.

## How these classes are wired (verified)

Each authored component is registered by a small **builder** in the `0x0064xxxx` range that
zero-inits and then fills a **0x50-byte descriptor** in `.data`, e.g. SoundEffect builder
`FUN_00642b00` (`@0x297030` in decomp):

```
_DAT_<descbase>      = &PTR_CopyFromStream_xxxx   ; dword0  = generic CopyFromStream vtable
DAT_<descbase+0x24>  = <stride>                   ; per-instance record size (bytes)
DAT_<descbase+0x28>  = 0x9e3779b9                 ; golden-ratio hash seed (all classes)
_DAT_<descbase+0x3c> = s_<ClassName>_<straddr>    ; class-name string ptr
FUN_0064a770();                                   ; common registration tail
```

The **field schema** is a *separate* per-class **deserialize template** that calls the field
registrars **in order** then finalizes with `FUN_0064a600(record,&stack)`. The template is
linked to its descriptor by its back-reference call
`FUN_00665590(record,(int)DAT_<descbase+0x04>)` — i.e. **template ⇔ descriptor via
`descbase+0x04`** (verified: SoundEffect descbase `0x17bdfd8` → glob `DAT_017bdfdc` →
template `FUN_006617e0`, 7 fields × 4 = `0x1c` = stride ✓; this stride==Σfield-size check holds
for every authored class below).

### Field registrars (size each contributes to stride)

| Fn | Type | Bytes | Notes |
|----|------|-------|-------|
| `FUN_00656210(intDefault)` | int / bool-as-int / handle | 4 | default is literal |
| `FUN_00656320(floatDefault)` | float | 4 | default is a float-DAT global or hex float |
| `FUN_00656610(&vec3OrColor)` | rgb / vec3 (returns 3 dwords) | 12 | default white = `DAT_00b9b664`(=1.0) triplet |
| `FUN_00656720(enumTable,enumDefault)` | enum | 4 | stored as int index into the named table |
| `FUN_00656890(default)` | name/hash ref | 4 | string-hash or asset handle |

### Two builder families & the marker/runtime split (important finding)

Every authored class also has a **stub** twin in `0x0063xxxx` (n=0 fields) — the bare/default
constructor. The *real* schema lives in `0x0065c000–0x0066ffff`.

The **`Rt*` and `Runtime*` components are NOT authored/streamed**: they either have **no schema
template at all** (pure markers) or a `0x66xxxx` template that *computes* values at runtime
(interpolation deltas, RNG, timers) instead of reading a stream. They are runtime-instance
shadows populated from their authored counterparts (e.g. `RtColorAnimation` ← `ColorAnimation`).
This is consistent across the whole family and explains every stride/field "mismatch".

---

## Registry table (all 34)

m2 hash via `pandemic_hash.py --m2`. "descbase" = descriptor base (`name_str_global − 0x3c`).
"schema" = ordered-field deserialize template (`—` = marker / runtime-computed, no authored schema).

| Class | m2 hash | descbase | CopyFromStream | stride | schema fn | purpose |
|-------|---------|----------|----------------|--------|-----------|---------|
| SoundEffect | 0xb40954f5 | 0x017bdfd8 | PTR_00bc0b68 | 0x1c | FUN_006617e0 | one-shot/positional SFX emitter (5 ids + volume + pitch) |
| SoundAmbience | 0x514cad3a | 0x017be028 | PTR_00bc0bb8 | 0x14 | FUN_006618f0 | looping ambient bed (5 bank/key ids) |
| SoundInterior | 0x05d1d9ba | 0x017be078 | PTR_00bc0c08 | 0x0c | FUN_006619d0 | interior/room reverb-zone sound set (3 ids) |
| SoundRuinKey | 0xbc1f685d | 0x017bdee8 | PTR_00bc0a00 | 0x04 | FUN_006615b0 | sound key tied to ruins/destruction (enum) |
| MusicSource | 0xb52a3a81 | 0x017be0c8 | PTR_00bc0c58 | 0x08 | FUN_00661a90 | music emitter (id + radius/weight float) |
| MusicRegion | 0x79dcbe56 | 0x017be118 | PTR_00bc0ca8 | 0x04 | FUN_00661b40 | music-trigger volume (1 id) |
| LocalizedName | 0xa49afec1 | 0x017bd768 | PTR_00bbfc70 | 0x04 | FUN_0065c740 | localized string-table key (1 id) |
| ObjectHint | 0x2a390a27 | 0x017be2f8 | PTR_00bc0ff0 | 0x0c | FUN_00662a60 | UI/AI object hints (3 enums) |
| Flammable | 0xd930020e | 0x017be294 | PTR_00bc0f50 | 0x04 | FUN_00662720* | flammability marker (1 opaque dword, no stream fields) |
| ParticleEmitter | 0xe595ab2f | 0x017bddf8 | PTR_00bc0730 | 0x10 | FUN_00661190 | particle system spawner (3 ids + 1 float) |
| RedEffectComponent | 0x60a13e3e | 0x017be398 | PTR_00bc1090 | 0x38 | FUN_00662b20 | "RedEffect" VFX/emitter authoring (14 fields, 2 enums) |
| EffectTemplate | 0xabaa1f3c | 0x017be3e8 | PTR_00bc10e0 | 0x04 | FUN_00662d40* | named effect-template ref (1 opaque dword) |
| EffectAiOccluder | 0x20e89c9d | 0x017be438 | PTR_00bc11a8 | 0x04 | FUN_00662e50 | marks effect as AI line-of-sight occluder (1 int) |
| BlobShadow | 0x40349618 | 0x017be208 | PTR_00bc0e10 | 0x24 | FUN_00661fe0 | projected blob shadow (id + 2 colors + flag + size) |
| LightObject | 0x97e8ee92 | 0x017beb18 | PTR_00bc1ea0 | 0x34 | FUN_006622e0 | placed dynamic light (id + color + 9 floats) |
| LightAnimation | 0xbd5349f7 | 0x017be488 | PTR_00bc1270 | 0x2c | FUN_00662ee0 | light param animation curve (10 floats + 1 int) |
| ColorAnimation | 0x2c9fb394 | 0x017be528 | PTR_00bc1310 | 0x0c | FUN_006631f0 | color/tint animation (3 floats) |
| ScaleAnimation | 0x60e3d029 | 0x017be578 | PTR_00bc1360 | 0x10 | FUN_006632b0 | scale animation (3 floats + bool) |
| Ribbon | 0x059b95b9 | 0x017be898 | PTR_00bc1ab8 | 0x2c | FUN_00664dd0 | trail/ribbon renderer (4 ints + 6 floats + bool) |
| TerrainFade | 0x26ae8736 | 0x017beac8 | PTR_00bc1e50 | 0x14 | FUN_006624f0 | terrain dither/fade params (5 floats) |
| Status | 0x09cd8b1f | (0x00cde768) | n/a (script obj) | n/a | FUN_00983c60 | **script object**, not ECS: verbs Start/Cancel/Update/GetStatus |
| AnimationController | 0xf1d5add9 | 0x017c0058 | PTR_00bc3ca8 | 0x04 | — | marker; drives the Rt* animations |
| RtVFX | 0x757b2069 | 0x017bfba8 | PTR_00bc3538 | 0x10 | — | runtime VFX instance (no authored schema) |
| RtRedEffect | 0x9b2daf6f | 0x017bfb58 | PTR_00bc34e8 | 0x20 | — | runtime RedEffect instance |
| RtRibbon | 0x9ab86eb3 | 0x017c0648 | PTR_00bc45a8 | 0x20 | — | runtime ribbon instance |
| RtLightAnimation | 0x8aa117bd | 0x017bfc48 | PTR_00bc35d8 | 0x2c | FUN_0066d860† | runtime light-anim instance (computed) |
| RtColorAnimation | 0x52da71de | 0x017bfc98 | PTR_00bc3628 | 0x10 | FUN_0066e580† | runtime color-anim instance (computed) |
| RtScaleAnimation | 0xc1edc09b | 0x017bfce8 | PTR_00bc3678 | 0x14 | FUN_0066e4a0† | runtime scale-anim instance (computed) |
| RtAlphaAnimation | 0xa7b2f925 | 0x017bfd38 | PTR_00bc36c8 | 0x14 | — | runtime alpha-fade instance |
| RtCoverHint | 0x4350b887 | 0x017c0148 | PTR_00bc3e08 | 0x01 | FUN_0066baa0† | runtime AI cover-hint instance (computed) |
| RuntimeSoundEffect | 0x0e83bcb7 | 0x017bf068 | PTR_00bc25c8 | 0x1c | FUN_0066daf0† | runtime SoundEffect playback (RNG/timer) |
| RuntimeSoundAmbience | 0x5fe773cc | 0x017bf0b8 | PTR_00bc2618 | 0x01 | — | runtime ambience playback |
| RuntimeSoundRuinKey | 0x25e2def3 | 0x017bf018 | PTR_00bc2578 | 0x04 | — | runtime ruin-key playback |
| RuntimeMusicRegion | 0xaa6964e8 | 0x017bf108 | PTR_00bc2668 | 0x01 | — | runtime music-region instance |

`*` = template registers no stream fields; the single stride-4 payload is written inline by the
record copy (a flag / template handle), not field-by-field.
`†` = `0x66xxxx` template **computes** its record at runtime (no `FUN_00656xxx` field reads) — a
runtime instance, not an authored schema. CopyFromStream PTR is the generic vtable
(`[0]=FUN_00644ec0 …`) shared by all classes; per-class behavior is the schema template.

---

## Per-component field schemas (ordered, with defaults)

Indices are stream order. Float defaults resolved from .rdata. `enum<Table>=N` = int index `N`
into the named enum table. Unlabeled int fields are usually ids/handles (default 0) or bool-as-int.

### SoundEffect — `FUN_006617e0`, stride 0x1c
1. `i` int = 0   — (sound/bank id A)
2. `i` int = 0   — (id B)
3. `i` int = 0   — (id C)
4. `i` int = 0   — (id D)
5. `i` int = 0   — (id E)
6. `f` float = **50.0**  — volume / max-distance
7. `f` float = **1.0**   — pitch / falloff scale

**On disk (retail `vz.wad`, PROVEN by the census `sound_effect_names_the_engine_loaded_banks`,
`mercs2_quartermaster`, `retail` feature).** The `COMP` (type hash `0xB40954F5`) sits in the
world-entity container (type `0x5647C35D`, ASET type 17). Its `schm` gives a 0x1C-byte payload of
seven fields, in stream order:

| word | byte | field hash | schm type | meaning |
|---|---|---|---|---|
| 0 | 0x00 | `0x2EB62242` | U32 | *unknown*: 81 of its 1,066 non-zero values are cue guids of `vz.wad` soundbanks, never of the word-4 bank |
| 1 | 0x04 | `0xC43322C3` | F32 | *unknown* |
| 2 | 0x08 | `0x29F15442` | F32 | *unknown* |
| 3 | 0x0C | `0x4E97DE03` | Ref | *unknown* |
| 4 | 0x10 | `0x14A67FA6` | U32 | the soundbank name hash, `m2(<bank>)`; 0 for none |
| 5 | 0x14 | `0xD932985B` | F32 | *unknown* |
| 6 | 0x18 | `0x11957817` | Ref | *unknown* |

The `data` chunk is grouped `[u32 n][n × u32 entity key][7-word payload]`: 1,080 groups over 2,754
entity keys, consuming its 11,394 words exactly. Word 4 names 63 of `vz.wad`'s 64 `veh_*` / `wpn_*`
soundbanks, and the engine loads the named bank by it on the local player's equipment and seat
changes ([`audio_code_map.md` §11.11](../reverse_engineer/audio_code_map.md#1111-the-banks-the-engine-loads-soundeffect)).
The schm order and types differ from the ordered list above, which is read from the deserialize
template's defaults.

### SoundAmbience — `FUN_006618f0`, stride 0x14
1–5. `i` int = 0  (five bank/key/zone ids — looping ambience set)

### SoundInterior — `FUN_006619d0`, stride 0x0c
1–3. `i` int = 0  (three ids — interior reverb/room sound set)

### SoundRuinKey — `FUN_006615b0`, stride 0x04
1. `enum<SoundKeyEnum_00bc71cc>` = 0  (first key) — destruction/ruin sound key

### MusicSource — `FUN_00661a90`, stride 0x08
1. `i` int = 0  — music cue/track id
2. `f` float = `DAT_00bd2154` (raw `0x00721c00`; reads as denormal — likely an int/handle Ghidra typed as float; **flag: treat as id/weight, not a real float**)

### MusicRegion — `FUN_00661b40`, stride 0x04
1. `i` int = 0  — music region/cue id

### LocalizedName — `FUN_0065c740`, stride 0x04
1. `i` int = 0  — localized string-table key/hash

### ObjectHint — `FUN_00662a60`, stride 0x0c
1. `enum<ObjectTypeHintEnum_00bc736c>` = 0
2. `enum<ElevationHintEnum_00bc73b4>` = 0
3. `enum<ArmorHintEnum_00bc73fc>`     = 0

### ParticleEmitter — `FUN_00661190`, stride 0x10
1. `i` int = 0  — particle-system/effect id
2. `i` int = 0  — (id/flags)
3. `i` int = 0  — (id/flags)
4. `f` float = 0 — (rate/scale)

### RedEffectComponent — `FUN_00662b20`, stride 0x38  (14 fields)
1. `i` int = 0
2–7. `f` float = 0  (six floats — emission rate / lifetime / size / etc., all 0 default)
8. `i` int = 0
9. `i` int = 0
10. `enum<EmissionShapeEnum_00bc74f4>`   = 0
11. `enum<EmissionControlEnum_00bc7514>` = 0
12. `f` float = 0
13. `f` float = 0
14. `i` int = 0

### Flammable — `FUN_00662720`, stride 0x04
Single opaque dword (no `FUN_00656xxx` field reads). Marker carrying one
flammability flag / material-key written inline.

### EffectTemplate — `FUN_00662d40`, stride 0x04
Single opaque dword — named effect-template reference (no stream fields).

### EffectAiOccluder — `FUN_00662e50`, stride 0x04
1. `i` int = 0  — occluder flag/id (marks an effect as blocking AI line-of-sight)

### BlobShadow — `FUN_00661fe0`, stride 0x24
1. `i` int = 0           — texture/shadow id
2. `rgb` color = (0,0,0) — color A (black default)
3. `rgb` color = (**1,1,1**) — color B (white; default `DAT_00b9b664`=1.0 triplet)
4. `i` int = **1**       — enabled / blend mode
5. `f` float = **1.0**   — size / opacity

### LightObject — `FUN_006622e0`, stride 0x34
1. `i` int = 0           — light type / id
2. `rgb` color = (0,0,0) — light color
3–11. `f` float = 0 (nine floats — intensity, range, atten, cone angles, etc., all 0 default)

### LightAnimation — `FUN_00662ee0`, stride 0x2c
1–10. `f` float = 0 (ten curve/timing floats — keyframe times & values, all 0 default)
11. `i` int = 0 (loop/mode flag)

### ColorAnimation — `FUN_006631f0`, stride 0x0c
1–3. `f` float = 0 (R/G/B delta or time/from/to — all 0 default)

### ScaleAnimation — `FUN_006632b0`, stride 0x10
1–3. `f` float = 0 (scale from/to/time)
4. `enum<BoolEnum_00bc6084>` = 0 (False — loop/relative flag)

### Ribbon — `FUN_00664dd0`, stride 0x2c
1–4. `i` int = 0 (texture/segment/material ids)
5–10. `f` float = 0 (width, lifetime, uv-scroll, etc.)
11. `enum<BoolEnum_00bc6084>` = False (0x6074) — enabled/world-space flag

### TerrainFade — `FUN_006624f0`, stride 0x14
1–5. `f` float = 0 (fade start/end distance, dither params — all 0 default)

### Status — `FUN_00983c60` (NOT an ECS component)
Registers a **script-callable object** named `Status` exposing the verbs
`Start` / `Cancel` / `Update` / `GetStatus` (strings `s_Start_00bad484`, `s_Cancel_00bba8ec`,
`s_Update_00b61018`, `s_GetStatus_00b6100c`). It binds two methods via vtable+0x1C
(`LAB_00983c50`, `LAB_00983c40`). Has no 0x50 reflection descriptor / stream schema — it is the
mission/objective Status command interface, included here only because the family manifest lists it.

---

## Runtime / marker components (no authored schema)

These carry no `FUN_00656xxx` field list. `—` markers have no template at all; `†` templates
compute their record at runtime (interpolation deltas / RNG / timers) and are driven by
`AnimationController` + their authored counterpart:

- **AnimationController** (FUN none) — orchestrates the Rt*/animation playback; pure marker, stride 4.
- **RtColorAnimation** `FUN_0066e580` — computes `(to-from)`, `1/duration`, etc. from a source curve.
- **RtScaleAnimation** `FUN_0066e4a0`, **RtLightAnimation** `FUN_0066d860`, **RtAlphaAnimation** (none) — same pattern.
- **RtVFX / RtRedEffect / RtRibbon** (none) — runtime instances of ParticleEmitter/RedEffectComponent/Ribbon.
- **RtCoverHint** `FUN_0066baa0` — runtime AI cover-hint, allocates via `FUN_006499f0`, links into a list under a critical section (`DAT_00edbaa4`). stride 1.
- **RuntimeSoundEffect** `FUN_0066daf0` — runtime SFX playback; uses the LCG `x*0x19660d+0x3c6ef35f` (`DAT_00dfcbac`) for random one-shot gating against field[3] probability.
- **RuntimeSoundAmbience / RuntimeSoundRuinKey / RuntimeMusicRegion** (none) — runtime audio-playback instances; data sourced from the authored Sound*/Music* components.

---

## Unknowns / flags

- Field *semantic names* are inferred from type, order, and class purpose; the binary keeps no
  member-name strings (only the enum-table names survive). Treated ints default-0 as "ids".
- `MusicSource` field[1] default (`DAT_00bd2154`) decodes as a float denormal → almost certainly an
  integer/handle mis-typed by Ghidra. Do not treat as a real float radius without a runtime check.
- Most authored Sound*/Music* fields default to **0** (ids resolved from the level stream); only
  `SoundEffect` exposes real tuned floats (volume 50, pitch 1.0).
- The CopyFromStream PTR per class (`PTR_CopyFromStream_*`) is a *generic* vtable shared by all
  reflected classes; per-class deserialize is the schema template, not the vtable entry.
- Enum default args shown as `0` select the first table entry; resolving full enum value lists
  (e.g. EmissionShapeEnum, SoundKeyEnum) was out of scope — table label addresses are cited so
  they can be dumped from .rdata if needed.
