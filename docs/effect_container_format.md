# Mercenaries 2 effect container format

**Status:** specification, measured against all 314 retail effects in PC `vz.wad`: every one
parses into the model below and re-encodes to the identical bytes, and the computed `EFCT` header
equals the stored one in 314/314 (`mercs2_formats/tests/effect_retail_roundtrip.rs`).
**Implementation:** `mercs2_formats::fxdict::{parse_effect_container, write_effect_container,
EffectContainer}`.
**Loader:** `mercs2_unpacked.exe` — effect driver `FUN_00491920`, PTYP-child reader `FUN_00492af0`,
EMIT walker `FUN_0048cc30`, TRFM-channel reader `FUN_00493150`. **Runtime:** spawner
`FUN_00488d70`, per-frame spawn count `FUN_0048f4f0`, particle spawn `FUN_0048f900` →
`FUN_0048ae80` → `FUN_00488770` (§2.1). What each field means at run time, in units, with the
integrator `FUN_00876840`: [`reverse_engineer/particle_sim_units.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/particle_sim_units.md),
ported in `mercs2_engine::fx_sim`.

Confidence labels: **PROVEN** = checked against retail bytes or a decomp read this pass;
**INFERRED** = reasoned from the decomp or the data, not exercised; **UNKNOWN** = carried verbatim,
meaning not established.

---

## 1. The asset

| Field | Value |
|---|---|
| Type hash | `0x5608BD5A` |
| ASET type id | 29 |
| Retail count | 314, all in `blocks\VZ\effects_P000_Q3` |
| Asset name | `pandemic_hash_m2(<effect name>)`, e.g. `global_explosion_c4` = `0x41B4326E` |
| Entry `field_c` | 0 in all 314 |

### 1.1 The effects block and its rows

`blocks\VZ\effects_P000_Q3.block` (block 3459) holds the 314 effects and 46 models (type
`0x5B724250`, ASET type id 19), 360 entries. Every one of its 360 ASET rows is a sentinel by-hash
row: `secondary_ref = 0xFFFFFFFF`, `packed_block_ref = 0x0D83FFFF` (block 3459, low 16 bits
`0xFFFF`). PROVEN by the retail ASET table.

**Nine of the 46 models share their asset hash with a texture** (type id 27) in the resident block
(block 3185), each hash with one row per type:

`0x196896DC`, `0xB1EC0672`, `0x663C3B0C`, `0xF84AEB3C`, `0x5B9DE759`, `0xD8382CD5`, `0x40FF196A`,
`0x3630720A`, `0x2EB74539`.

PROVEN; to list a hash's rows:

```text
cargo run -p mercs2_probe --bin aset_decode -- --wad <vz.wad> 0x196896DC
```

The engine keys an asset by type and hash, so the pairs do not collide in the game. A tool that
claims assets by hash alone sees the effects block and the resident block claim the same nine.

**The block ships whole.** In the live test (2026-10-04, the game under Wine on macOS) a patch block
carrying the whole effects block — 360 entries, the C4 effect recoloured — played the recoloured C4
explosion, both from a placed charge and from `Pg.Spawn("global_particle_explosion_c4")`. A patch
block carrying the one recoloured effect alone left the C4 explosion's look retail and spawned
effects played their sound with no visual. That the lone-effect block is what broke the visuals is
SPECULATIVE: the cause was not traced. `qm` ships effects only as the whole block at its own path,
the game's entries first and added effects after them, each added effect with a sentinel by-hash
row of type id 29 (`mercs2_quartermaster::fx`).

A world template names its effect with the placement / template name minus `particle_`:
`global_particle_env_godray2` → `global_env_godray2` (`0xDB331999`),
`global_particle_explosion_c4` → `global_explosion_c4` (`0x41B4326E`). **PROVEN** by hash match
against the 314. For the C4 template the world-entity data agrees: the `RedEffectComponent` record
of entity `0x80008028` carries `0x41B4326E` in its `name` field (`0x1DE5C824`).

**The field that carries a template's effect** is `RedEffectComponent`'s `name` (`0x1DE5C824`, schm
code 6, offset 0): in 524 of the 538 retail `RedEffectComponent` records it is the name hash of one
of the 314 effects (PROVEN by the data, [`worldentity_container_format.md`](worldentity_container_format.md)
§4). `ObjectState.StartEmitter` passes a **template** name hash, not an effect hash:
`FUN_004D28C0` resolves it through the template lookup `FUN_00672F60` and spawns that template with
`FUN_006746D0`. That the spawned template's `RedEffectComponent` `name` is what starts the effect
is INFERRED; the code from it to PgFX was not read.

The container is a UCFX tree — see [`ucfx_tree_container.md`](ucfx_tree_container.md) for rows,
`x2`/`x3`, marker rows, contiguous bodies and `CSUM`.

---

## 2. Tree

```text
EFCT                          18 B    nine u16, computed (§3)
├─ EMTR                        2 B    u16 = number of GEOM children
│  └─ GEOM × n                        u32 k, then k × 13 f32         (emitter shapes)
├─ EMIT  (marker)                     ┐
│  ├─ TRFM                    64 B    │ 4×4 f32
│  │  └─ ATRB × 9                     │ the nine channels (§5.1)
│  └─ GEOM (optional)          4 B    │ u16 shape index, u16 count    } one pair per emitter
├─ PTYP                        4 B    │ u32 flags
│  ├─ ATRB × 19                       │ (§5.2, fixed order)
│  ├─ COLR                   800 B    │ (§6)
│  ├─ ATRB × 13                       │ (§5.2, fixed order)
│  └─ TEXT                            ┘ u32 n, n × u32 frame (fxdict key)
└─ FRCE × k                           u32 kind hash + kind parameters (§7)
   └─ ATRB × (7 + kind extras)
```

Every `ATRB` may own one curve:

```text
ATRB                          12 B    { u32 hash, u32 flags, u32|f32 value }
└─ ANIM                        4 B    u32 = number of AKEY children
   └─ AKEY × n                 8 B    { f32 time, f32 value }
```

Order is fixed: `EMTR` first, then (`EMIT`, `PTYP`) pairs, then the `FRCE`s. **PROVEN** (314/314).
There is no `POFF` in any retail effect.

| Node | Rule | Evidence |
|---|---|---|
| `EMTR` | u16 equals its GEOM child count; every retail EMTR has ≥ 1 | PROVEN; loader allocates `count × 4` |
| `EMTR/GEOM` | `4 + 52·k` bytes; each record is 13 f32, a triangle particles spawn on (§2.1) | PROVEN; loader allocates `k × 0x34`, copies 13 words |
| `EMIT` | marker; children `TRFM` then optional `GEOM` (811 of 820 have one; §2.1 says when an emitter may go without) | PROVEN |
| `EMIT/GEOM` | u16 shape index into the EMTR table, then u16 count: the number of the shape's records the emitter samples (§2.1) | PROVEN, decomp `FUN_0048cc30`, disassembly `FUN_0048ae80` |
| `PTYP` | u32 flags: bit 0 → emitter `+0x205` (sprite turns to the screen-projected velocity), bit 1 → `+0x206` (flipbook); no other bit is read; retail uses 0–3 | decomp `FUN_00491920`, `0x00494802`, `0x00500F0A` |
| `TEXT` | u32 n, then n frames; `4 + 4n` bytes; n ≥ 1 (the loader reads one frame when n ≤ 1) | PROVEN sizes; decomp `FUN_00492af0` |

**A frame is the key of an `fxdict` record, and the record is the frame's rectangle of the `vfx`
atlas.** `FUN_00492af0` hands the frames to `FUN_004911a0`, which looks each one up with
`FUN_00491510` — a binary search over the record keys of the `fxdict` (`0x86BF6C5B`, type
`0xFA46D8A8`, in the resident block) — and packs the record's four values, `(u, 1 − v − h, w, h)`, as
binary16 into the stream table. The values are the frame's rectangle in the one texture every
particle samples, the `vfx` atlas `0x89E211AF` (2048² DXT5 in the resident block): the vertex shader
`PgFXVP` computes the texel coordinate as `rect.xy + corner × rect.zw`. A key the search misses gets
the static record `(0, 0, 1, 1)`, the whole atlas. PROVEN by the decomp, the shader and the retail
atlas ([`fxdict_format.md`](fxdict_format.md) §3.2, §3.4). Of the 566 distinct frames in the 314
retail effects, 546 are records of the 630-record `fxdict` and none is a texture asset
(`mercs2_quartermaster/tests/fx_retail.rs`).

**Several frames.** With `PTYP` bit 1 clear, `FUN_004911a0` writes 100 rectangles: the frame index
starts at 0 and advances by `n × 0.01` (`DAT_00B97EEC`) per rectangle, wrapping to 0 at `n`, so each
of the `n` frames fills about `100 / n` consecutive slots of the 100. With bit 1 set, it writes the `n`
rectangles once, in order (and the loader reserves `2·n` stream words, §3). PROVEN by the decomp.
Bit 1 clear, a particle draws slot `trunc(99 · age01)`; bit 1 set, rectangle `counter / scale`, the
counter advancing one per sim update (`particle_sim_units.md` §10).

### 2.1 Emitter shapes: where particles spawn

**The `GEOM` of an emitter.** `FUN_0048cc30`, the EMIT walker, reads the `GEOM` body as two u16:
the first indexes the EMTR shape table and the shape's record pointer goes to `+0x04` of the
emitter's 0x210-byte runtime record; the second, sign-extended (`(int)(short)`), goes to `+0x00`.
PROVEN (decomp `FUN_0048cc30`, the `GEOM` arm). An emitter without `GEOM` has 0 at both: PROVEN by
the live dump below; the code that zeroes them was not read.

**The spawn.** The spawner `FUN_00488d70` zeroes the effect instance's mode word `+0x770` (`mov
[ebp+0x770], edi` with `edi = 0` at `0x00488F7E`), so a spawned effect runs in mode 0
(`FUN_0048b470` sets modes 1, 4 and 5 for attached effects). For an effect instance,
`FUN_0048f900` calls `FUN_0048ae80` once per emitter with the emitter's runtime record as its first
argument (the call at `0x0048FE08`). In mode 0,
for each particle to spawn, `FUN_0048ae80` takes the count at `+0x00` and picks a record as
`random % count`, then reads it at `+0x04 + 52 × index`:

```text
0x0048AFC7  mov edi,[esp+0x64]        ; the emitter's runtime record
0x0048AFD1  mov esi,[edi]             ; +0x00: the GEOM count
            ...                       ; the LCG at 0x00DFCBAC
0x0048AFF4  xor edx,edx
0x0048AFF6  div esi                   ; unsigned: random % count
0x0048AFFC  mov eax,edx
0x0048AFFE  imul eax,eax,0x34
0x0048B001  add eax,[edi+4]           ; +0x04: the shape's records
0x0048B008  call 0x488770
```

PROVEN (disassembly of `mercs2_unpacked.exe`). So the engine runs an emitter's shape only when:

| Rule | What breaks otherwise | Evidence |
|---|---|---|
| A `GEOM` names a shape of the EMTR table | the record pointer is read from past the table | PROVEN, `FUN_0048cc30` |
| The count is ≥ 1 | `div` by 0: `INT_DIVIDE_BY_ZERO` | PROVEN, disassembly; observed live (below) |
| The count is ≤ the shape's record count | the index reaches past the shape's records | PROVEN, disassembly |
| The count is ≤ 32,767 | sign-extended, it is a negative count, and the unsigned `div` yields an index past the records | PROVEN, decomp `(int)(short)` |
| An emitter without `GEOM` spawns no particle | its count is 0: `div` by 0 on its first particle | PROVEN, disassembly; observed live |

In all 811 retail `GEOM`s the count equals the named shape's record count; no retail shape is
empty. PROVEN (`mercs2_quartermaster/tests/fx_retail.rs`).

**Observed, 2026-10-06.** An authored effect whose one emitter had no `GEOM` (rate 30) crashed the
game under Wine on `Pg.Spawn` of its template: `pmc_blackbox.log` recorded `VEH EXCEPTION C0000094
INT_DIVIDE_BY_ZERO @ EIP=0048AFF6`, `ESI=00000000`, `EDI=1F5C63F0`, and the 16 bytes at `EDI` were
zero (count 0, record pointer 0); the return address on the stack was `0x0048FE0D`, the call in
`FUN_0048f900`. The fixture is [`modding/fx_live_gate.md`](modding/fx_live_gate.md) §3.

**How many particles an emitter spawns.** `FUN_0048f4f0` adds to each emitter's spawn accumulator
(the instance's `+0x670 + 4·i`), on each update:

- `max(rate + (1 − 2u)·ratevar, 0)` times the instance's `+0x7F0` (1.0 from the spawner), where
  `rate` is the attribute `0x062F0D37` (`FUN_00492af0` stores it in the emitter slot `+0xC0`, and
  `0x70653182`, `ratevar`, in that slot's `+0xC`), evaluated by `FUN_00490960` (`u` uniform in
  `[0, 1)`, the constant `2.0` at `0x00B92874`); PROVEN, disassembly;
- the instance's displacement since the last update (`FUN_00401740` of `Δx² + Δy² + Δz²`, a
  square root — INFERRED — capped) times the
  instance's `+0x7F4`, which the spawner copies from `+0x08` of the template's
  `RedEffectComponent` (`ecx = 0x017BE398`, that class's descriptor, at `0x00488DC8`). PROVEN by
  the disassembly; that `+0x08` of the runtime component is the schm field at offset 8,
  `0x62C7746E`, is INFERRED (the schm offsets are the serialized layout, and the class stride is
  `0x38` in both).

In mode 0, `FUN_0048ac80` makes the integer part of the accumulator, capped at 2,000, the update's
count; `FUN_0048ae80` spawns nothing when it is below 1. PROVEN, disassembly.

So an emitter without `GEOM` runs only when both terms stay 0: `rate` a constant (no curve) at or
below 0, `ratevar` 0, and every template that starts the effect with a `0x62C7746E` of 0. The 9
retail emitters without `GEOM` (in 9 effects) all have `rate` 0.0 with no curve and `ratevar` 0.0,
and all 20 retail `RedEffectComponent` records that name those effects have `0x62C7746E` = 0.0
(of the 538 retail records, 71 have a non-zero value). PROVEN, `fx_retail.rs`.

**The record.** `FUN_00488770` reads a record as a triangle: the vertex `P` at floats 4–6, the edges
`A` at 7–9 and `B` at 10–12. The particle starts at `P + u·A + v·B`, `u` uniform in `[0, 1)` and
`v` uniform in `[0, 1 − u)`, and floats 1–3 are copied out beside it. PROVEN, decomp
`FUN_00488770`. `FUN_0048ae80` passes the copied vector through `D3DXVec3TransformNormal` (PROVEN,
disassembly at `0x0048B0D2`) and then `FUN_00401630`; that the result is the particle's emission
direction, perturbed by `spread`, is INFERRED. Float 0 is not read by `FUN_00488770`; where else it
is read is UNKNOWN.

| Floats | Retail (13,148 records) | Read by |
|---|---|---|
| 0 | `\|A × B\|`, in all 13,148 | not `FUN_00488770` |
| 1–3 | a unit vector along `±(A × B)` in all 13,148 (`+` in 8,766) | copied out by `FUN_00488770` |
| 4–6 | `P` | `FUN_00488770` |
| 7–9 | `A` | `FUN_00488770` |
| 10–12 | `B` | `FUN_00488770` |

---

## 3. `EFCT` — computed header

Nine u16. The loader reads them as a count pair and three `(entries, words)` table reservations
(`FUN_00491920`). A writer computes every word:

| Word | Value |
|---|---|
| 0 | emitters (PTYP count) |
| 1 | `0x0226` (read and skipped) |
| 2 | forces (FRCE count) |
| 3 | linear-table entries: TRFM channel curves + PTYP *linear* curves + FRCE curves |
| 4 | linear-table words: `2 × keys` per curve; a TRFM curve with flag bit 8 reserves 100 |
| 5, 6 | 0 (a third table, empty in all of retail) |
| 7 | stream-table entries: `COLR` + `TEXT` per emitter + PTYP *resampled* curves |
| 8 | stream-table words: 200 per `COLR`, 200 per `TEXT`, 100 per resampled curve |

Whether a PTYP curve is *linear*, *resampled* or *not counted* is decided by the handler its
attribute hash dispatches to in `FUN_00492af0`, **not** by the flag bits:

| Handler (thunk) | Curve class | Hashes |
|---|---|---|
| `0x024E2380` | resampled (100 words, stream table) | `size` `0x8C3654C2`, `0x497A1895`, `0x35ACBAB7` |
| `0x024EEC10` | linear (`2 × keys`, linear table) | `rate`, `speed`, `spread`, `inheritvel`, `life` |
| `0x024EEBE0` | not counted | `speedvar`, `spreadvar`, `lifevar`, `inheritvelvar` |

- The rule is **PROVEN** by 314/314. The handler table is the dispatch in `FUN_00492af0`; the
  handler bodies are SecuROM-relocated and not decompiled, so the *class* names are INFERRED from
  what each handler's hashes do to `EFCT`.
- `size` curves count as resampled with flags `0x401` as well as `0x701`: bit 8 does not decide it.
- Retail carries two `speedvar` curves (effect `0xBB86EA9B`). They are in the file and not in
  `EFCT` — consistent with their handler not consuming a curve.
- FRCE curves (retail puts them on `ampl`, `posx`, `posy`, `posz`) count as linear. PROVEN by the
  data; the FRCE-child dispatch is behind an unresolved jump table.
- TRFM curves go through `FUN_00493150`: flag bit 8 clear → `2 × keys` words; bit 8 set → 100
  resampled words. Both branches write through the same table pointer, so a bit-8 TRFM curve
  counts in words 3/4 with 100 words. INFERRED — no retail TRFM curve has bit 8.
- `TEXT` counts 200 words in every retail effect, including the 80 PTYPs with bit 1 set. With bit 1
  the loader actually reserves `2 × n` words for the frames (`FUN_00492af0`), so bit 1 with more
  than 100 frames would overrun the reservation. INFERRED; the reference writer refuses it.

The byte-swapping note for the Xbox path (EFCT read as u16 vs byteswapped as u32 → NULL `[+0x60]`
→ fault at `0x00493102`) is in [`spatial_hash_crash_analysis.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/spatial_hash_crash_analysis.md).

---

## 4. `ATRB`

```text
+0  u32 hash      pandemic_hash_m2(attribute name)
+4  u32 flags
+8  u32 value     f32 when flags bit 0 is set, else u32
```

| Bit | Name | Rule |
|---|---|---|
| 0 | float | set ⇔ the value is an f32. Derived. |
| 7 | option | authored; meaning UNKNOWN |
| 8 | resample | authored; `FUN_00493150` takes the 100-sample branch for a TRFM curve |
| 9 | option | authored; meaning UNKNOWN |
| 10 | curve | set ⇔ the ATRB has an `ANIM` child. Derived. PROVEN on all 1,880 retail curves. |

`FUN_00493150` packs bits 7–10 into a nibble as `b7<<3 | b10<<2 | b8<<1 | b9`. No retail ATRB sets
any other bit, and no u32-valued ATRB sets any bit. A writer derives bits 0 and 10 and refuses an
authored flag word that disagrees.

`ANIM` = u32 key count, equal to its `AKEY` child count (PROVEN). `AKEY` = `{f32 time, f32 value}`;
retail times run 0–100.

---

## 5. Attribute positions

### 5.1 TRFM channels (9, in order)

`posx` `0x7D1117EB`, `posy` `0x9B0F088E`, `posz` `0xFD165E99`, `rotx` `0xF6B55F38`,
`roty` `0x20B7DFED`, `rotz` `0x9EB05782`, `sclx` `0xC5574C5F`, `scly` `0xE3553D02`,
`sclz` `0x655CC56D`. All f32; all may carry a curve (`FUN_0048cc30` → `FUN_00493150`).

### 5.2 PTYP (32, fixed order, 820/820 retail)

Before `COLR`:

| # | Hash | Name | Value | Curve |
|---|---|---|---|---|
| 0 | `0x1DE5C824` | name | u32 | — |
| 1 | `0x8C3654C2` | size | f32 | resampled |
| 2 | `0xC1008E25` | sizevar | f32 | — |
| 3 | `0xC3AEB321` | mass | f32 | — |
| 4 | `0x47035D90` | massvar | f32 | — |
| 5 | `0xD3AE67AF` | life | f32 | linear |
| 6 | `0xC7CFE6AA` | lifevar | f32 | not counted |
| 7 | `0x497A1895` | *unresolved*: spin, degrees a second | f32 | resampled |
| 8 | `0x1792B524` | *unresolved*: its variance | f32 | — |
| 9 | `0x10831673` | *unresolved*: factor on attractors | f32 | — |
| 10 | `0xB6197EFE` | *unresolved*: its variance | f32 | — |
| 11 | `0x6CF0BE14` | forcedrag | f32 | — |
| 12 | `0xBE968D3B` | forcedragvar | f32 | — |
| 13 | `0xC558C9D8` | forcegravity | f32 | — |
| 14 | `0xD80DF37F` | forcegravityvar | f32 | — |
| 15 | `0x720F22F8` | *unresolved*: factor on vortices | f32 | — |
| 16 | `0x4712719F` | *unresolved*: its variance | f32 | — |
| 17 | `0xB3DBB6C0` | forcewind | f32 | — |
| 18 | `0x4C49B137` | forcewindvar | f32 | — |

After `COLR`:

| # | Hash | Name | Value | Curve |
|---|---|---|---|---|
| 19 | `0xC3592BB7` | *unresolved*: material index into `0xD2AE50` | u32 (2 in all retail) | — |
| 20 | `0xEEE1A341` | scale | u32 (loader clamps to ≥ 1) | — |
| 21 | `0x062F0D37` | rate | f32 | linear |
| 22 | `0x70653182` | ratevar | f32 | — |
| 23 | `0x437F66EC` | spread | f32 (retail 0–180) | linear |
| 24 | `0xB8A95DE3` | spreadvar | f32 | not counted |
| 25 | `0x15BA509E` | speed | f32 | linear |
| 26 | `0x9BE62E41` | speedvar | f32 | not counted |
| 27 | `0xB4247BF3` | inheritvel | f32 | linear |
| 28 | `0xB15ABB7E` | inheritvelvar | f32 | not counted |
| 29 | `0x35ACBAB7` | *unresolved*: start angle, degrees | f32 | resampled |
| 30 | `0x1B878602` | *unresolved*: its variance | f32 | — |
| 31 | `0x270C9E9D` | emissiondir | u32 (0, 1, 2 in retail) | — |

"—" = no retail effect has a curve there and the loader's handling of one is not decoded; the
reference writer refuses a curve at that position.

### 5.3 FRCE (7 common + kind extras)

Common, in order: `ampl` `0x3DC3D9DF`, `posx`, `posy`, `posz`, `rotx`, `roty`, `rotz`. Curves
appear on `ampl` and `pos*` (linear).

| Kind | Extra attributes |
|---|---|
| gravity, wind | none |
| drag | `0x2F68CD8F` (unresolved, f32: radius, metres) |
| attractor | `local` `0x201B5A86` (u32), `range` `0xE0686A68`, `decay` `0xC2783A55` |
| vortex | `local`, `range`, `decay`, `radial` `0xB46F1F0C` |

### 5.4 Names

Every name above is `pandemic_hash_m2(name) == hash`, PROVEN. Ten hashes (9 PTYP positions and the
drag extra) are unresolved and kept as hashes; what the code does with each is
[`particle_sim_units.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/particle_sim_units.md) §1.

---

## 6. `COLR` — 800 bytes

`FUN_00492af0` copies exactly 800 bytes and reserves 200 stream words for it. The body is 100 keys
of 8 bytes, spread over the particle's life (key `i` at age `i / 99`):

```text
+0  u8      blue
+1  u8      green
+2  u8      red
+3  u8      alpha
+4  u16     binary16 bit pattern
+6  u16     0
```

- The trailing u16 is 0 in all 82,000 retail keys. PROVEN.
- The colour is stored blue, green, red, alpha. PROVEN live, 2026-10-07 (the game under Wine,
  [`modding/fx_live_gate.md`](modding/fx_live_gate.md) §3.6), for bytes 0–2:
  - `qm_fx_cyan_burst` (fixture `qm-fx-a`), its keys stored `00 FF FF`, drew gold/yellow;
  - `global_particle_fire_carhood` recoloured with its keys stored `FF FF 00` drew cyan;
  - keys stored `00 FF 00` drew green and keys stored `FF 00 FF` (the C4) drew magenta, both the
    same under a red/blue swap.
- Byte 3 is alpha: it fades to 0 over the keys in retail (god-ray: `3f 3f 3f 29`). INFERRED; the
  live run held it fixed.
- Authors give red, green, blue, alpha. `mercs2_formats::fxdict::ColrKey::rgba` holds them in that
  order, and `Colr::to_bytes` / `Colr::from_bytes` are the one place the stored order is mapped, so
  retail keys still re-encode byte for byte.
- The binary16 multiplies the colour by its magnitude, and below 0 writes alpha 0, which adds (`PgFXVP`;
  the blend factors are INFERRED). It is taken from the floor key, not lerped. All of a body's keys
  share one sign: 638 bodies positive, 182 negative. PROVEN; `particle_sim_units.md` §9.

Earlier notes gave COLR as 200 bytes of 50 RGBA stops (the 200 is the stream-word reservation, not
the byte size) and as an 8-byte record `[0xBC, 0, 0, R, G, B, A, 0]` (that reading is the key
layout shifted by five bytes).

---

## 7. `FRCE` — forces

`u32 kind` (`pandemic_hash_m2` of the kind name, PROVEN) then fixed parameters
(`FUN_00491920`, FRCE arm). Field names that state a meaning are INFERRED; the rest are named by the
runtime offset the loader writes.

| Kind | Hash | Bytes | Mode | Parameters |
|---|---|---|---|---|
| wind | `0xC9F7A9D7` | 20 | 0 | f32 magnitude → `+0x12C`; vec3 direction → `+0x110`; sets effect `+0x94 \|= 2` |
| gravity | `0x14BD1BBD` | 20 | 1 | f32 magnitude → `+0x12C`; vec3 direction → `+0x110` |
| drag | `0xED791C4B` | 8 | 2 | f32 magnitude → `+0x12C` |
| attractor | `0xC235456B` | 32 | 3 | f32 → `+0x12C`; u32 (read `!= 0`) → `+0x13C`; f32 → `+0x130`; f32 → `+0x134`; vec3 → `+0x104` |
| vortex | `0xF4D85A49` | 60 | 4 | as attractor, plus f32 → `+0x138`; vec3s → `+0x104`, `+0x110`, `+0x11C` (a basis is built from the cross product of the last two) |

Retail: 225 gravity, 255 drag, 42 wind, 27 attractor, 110 vortex. No other kind hash occurs, and the
loader reads nothing for one.

---

## 8. Writing an effect

1. Build shapes (≥ 1), emitters (≥ 1; each with 9 TRFM channels and 32 PTYP attributes in the
   order above, a `COLR`, a `TEXT` with ≥ 1 frame, each an `fxdict` key), and forces.
2. Derive each ATRB's bits 0 and 10; set only bits 7/8/9 by hand.
3. Compute `EFCT` (§3).
4. Flatten per [`ucfx_tree_container.md`](ucfx_tree_container.md) and append `CSUM`.

The reference writer refuses: a flag word that disagrees with its value or curve; a bit outside
{0, 7, 8, 9, 10}; options or a curve on a u32 attribute; an empty curve; an attribute out of
position or of the wrong value type; a curve at a position marked "—"; PTYP flag bits other than 0
and 1; an empty `TEXT`; PTYP bit 1 with more than 100 frames (`EffectContainer::validate_nodes`).
It also refuses every emitter shape §2.1 says the engine cannot run
(`EffectContainer::check_emitter_shapes`): a `GEOM` shape index past the shape table; a `GEOM`
naming a shape without records; a count of 0, above the shape's record count, or above 32,767; and
an emitter without `GEOM` whose `rate` has a curve or is above 0, or whose `ratevar` is not 0. The
template side of §2.1 (`0x62C7746E` = 0 for every template that starts an effect with an emitter
without `GEOM`) is checked by `qm` where it merges templates (lint M0309).
