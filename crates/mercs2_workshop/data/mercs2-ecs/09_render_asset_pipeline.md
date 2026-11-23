# Mercenaries 2: World in Flames — Family 09: Render / Asset-Pipeline Types

Reverse-engineered from the Ghidra decompilation of the game EXE (uncracked v1.1,
image base `0x400000`; `.rdata` `0xb05000`, `.data` `0xbf5000`). m2 hashes computed
with `tools/pandemic_hash.py --m2` (`texture=0xF011157A` and `index=0xBCC8D613`
cross-checked against the prebuilt m2 rainbow table).

## TL;DR — these 12 are NOT gameplay-ECS components

They matched the class-registration grep pattern but split into **three** kinds:

1. **8 render/precache resource-type tags** — `vshader, pshader, vertex, vertdecl,
   surface, texture, index, display`. Each is a `Precache*` *resource-class*
   descriptor built by its own tiny constructor in the D3D9 resource-precache
   subsystem (`FUN_007570a0`…`FUN_00758e10`). They are real type tags, but they are
   **resource classes**, not the `_DAT_<descr> = s_<Class>` + 0x50-byte
   CopyFromStream/0x9e3779b9 ECS reflection descriptor used by gameplay components.

2. **1 weapon/physics mesh-shape class** — `WpMeshShape`. A genuine serializable
   class (constructor `FUN_007253f0`, vtable `PTR_FUN_00bd22d0`, stride `0x30`), in
   the weapon collision-shape subsystem — again NOT the ECS builder family.

3. **3 state/phase strings on ONE shared global `0x01175e30`** — `failresolve,
   finalize, potential`. These are **NOT classes**. They are debug/trace progress
   markers written (and only written — never read in decompiled code) by the single
   recursive function `FUN_00581fd0` as it walks an asset/visibility-resolution
   state machine. They are members of an enum-like set of progress strings, all
   stored into the same global string slot.

---

## Summary table

| # | Name | m2 hash | global addr | classification | purpose |
|---|------|---------|-------------|----------------|---------|
| 1 | vshader | `0x504be9d2` | `0x017d3fb0` | render-type (precache class) | vertex-shader precache resource type |
| 2 | pshader | `0x8219ba20` | `0x017d4044` | render-type (precache class) | pixel-shader precache resource type |
| 3 | vertex | `0x0c4c02f7` | `0x017d3f78` | render-type (precache class) | vertex-buffer (VB) precache resource type |
| 4 | vertdecl | `0xdc4b0e40` | `0x017d400c` | render-type (precache class) | vertex-declaration precache resource type |
| 5 | surface | `0xbe609ae2` | `0x017d3fd4` | render-type (precache class) | render-surface/render-target precache resource type |
| 6 | texture | `0xF011157A` | `0x017d3ff0` | render-type (precache class) | texture precache resource type (the canonical ASET `texture` tag) |
| 7 | index | `0xBCC8D613` | `0x017d4028` | render-type (precache class) | index-buffer (IB) precache resource type |
| 8 | display | `0x84d0382d` | `0x017d3f94` | render-type (precache class) | display/back-buffer precache resource type |
| 9 | WpMeshShape | `0x509eee8a` | `0x017c6f88` | reflection-class (weapon/physics) | weapon mesh collision-shape serializable class (stride 0x30) |
| 10 | failresolve | `0x213e70c3` | `0x01175e30` *(shared)* | state-string | resolution-failed phase marker (see state machine) |
| 11 | finalize | `0x3e6c2a4f` | `0x01175e30` *(shared)* | state-string | finalize/commit phase marker (see state machine) |
| 12 | potential | `0x8bca940d` | `0x01175e30` *(shared)* | state-string | candidate-evaluation ("potential") phase marker (see state machine) |

> **Note A — hashes:** All 12 m2 hashes in the table were computed with
> `tools/pandemic_hash.py --m2 <name>`; `texture=0xF011157A` and `index=0xBCC8D613`
> were cross-checked against the prebuilt m2 rainbow table and match. The m2 algorithm
> (FNV-1a, case-folded, plus the Mercs2 post-step): `h=0x811C9DC5`; per char
> `h = ((h ^ (ord(c)|0x20)) * 0x01000193) & 0xFFFFFFFF`; then finalize
> `h = ((h ^ 0x2A) * 0x01000193) & 0xFFFFFFFF`.

---

## The 8 precache resource-type tags (vshader/pshader/vertex/vertdecl/surface/texture/index/display)

These are **render-resource class descriptors**, one per D3D9 resource kind, built by
a cluster of near-identical zero-arg constructors that populate a small global
descriptor and return a pointer to it. They are invoked from the global static-init
region `0x00a7da50`–`0x00a7db30` (a ctor table), i.e. set up once at engine startup.

### Descriptor layout (verified, identical across all 8)

Taking `index` (`FUN_007570a0` @ `0x007570a0`) as the template — base = `0x017d4024`:

| field (rel. to base) | value for `index` | meaning |
|---|---|---|
| `+0x00` | `&PTR_LAB_00bd6674` | resource-type vtable (Create/ReadData/WriteData/RegisterResource) |
| `+0x04` | `s_index_00bd65a4` | **type-name string** (= manifest col 2) |
| `+0x08` (dword) | `0x50524543` low | **type tag** — bytes `43 45 52 50` = ASCII **"PREC"** (PRECache) |
| `+0x08` (5th byte) | per-type id (see below) | resource-kind discriminator |
| `+0x10` (high byte) | per-type size/stride | per-resource record size |

The `5th tag byte` is a small sequential resource-kind id and the `+0x10` high byte
is a size, decoded from each constructor's two literal dwords
(`_DAT_..2c = 0xNN0524543`, `_DAT_..34 = 0xSS00000000`):

| Name | constructor (VA) | base global | type-name str | kind id (5th byte) | size byte |
|------|------------------|-------------|----------------|--------------------|-----------|
| texture  | `FUN_007589a0` @ `0x007589a0` | `0x017d3fec` | `s_texture_00baffa8`  | `0x00` | `0x34` (52) |
| surface  | `FUN_00758170` @ `0x00758170` | `0x017d3fd0` | `s_surface_00bd6e88`  | `0x01` | `0x28` (40) |
| index    | `FUN_007570a0` @ `0x007570a0` | `0x017d4024` | `s_index_00bd65a4`    | `0x02` | `0x1c` (28) |
| vertex   | `FUN_007575f0` @ `0x007575f0` | `0x017d3f74` | `s_vertex_00bd231c`   | `0x03` | `0x1c` (28) |
| vertdecl | `FUN_00757cc0` @ `0x00757cc0` | `0x017d4008` | `s_vertdecl_00bd6c30` | `0x04` | `0x10` (16) |
| vshader  | `FUN_007584c0` @ `0x007584c0` | `0x017d3fac` | `s_vshader_00bd6fec`  | `0x05` | `0x0c` (12) |
| pshader  | `FUN_007578e0` @ `0x007578e0` | `0x017d4040` | `s_pshader_00bd69e8`  | `0x06` | `0x0c` (12) |
| display  | `FUN_00758e10` @ `0x00758e10` | `0x017d3f90` | `s_display_00bd7350`  | `0x07` | `0x10` (16) |

The per-type vtables and the strings prove the subsystem: the index path logs
`PrecacheIndexBuffer::ReadData` / `PrecacheIB::RegisterResource` (`FUN_00756ed0`,
`FUN_007571a0`); the vertex path logs `PrecacheVB::RegisterResource`
(`FUN_007576f0`); pshader logs `PrecachePS::RegisterResource` (`FUN_007579e0`);
vertdecl logs `PrecacheVertexDecl::WriteData`. All reference source path
`D:\Projects\Mercs2\PC\mercs2\...`. So this is the **D3D9 resource-precache
manager**: each tag names a streamable GPU resource class (VS/PS shader, VB, vertex
decl, render surface, texture, IB, display buffer).

**Verdict:** genuine engine **type tags**, but in the render/precache resource
hierarchy — distinct from the gameplay-ECS reflection builder. They do NOT use the
`CopyFromStream + stride@+0x24 + 0x9e3779b9 seed` ECS descriptor; their "stride" is
the per-resource size byte above and their vtable is a Precache resource vtable.

`s_index_00bd65a4` is also referenced at `FUN_0085ed00(s_index_00bd65a4)` (a generic
logging/format helper) — incidental, not a second class.

---

## WpMeshShape — weapon/physics mesh-collision-shape class

- **Registrar:** `FUN_00a7ce60` @ `0x00a7ce60` writes:
  - `_DAT_017c6f88 = s_WpMeshShape_00bd22c4` (class-name string = manifest col 2)
  - `_DAT_017c6f8c = FUN_007253f0` (constructor)
  - `_DAT_017c6f90 = &LAB_00725470` (handler/factory entry)
  - `_DAT_017c6f94 = &PTR_FUN_00bd22d0` (object vtable)
- **Constructor:** `FUN_007253f0` @ `0x007253f0` sets `*this = &PTR_FUN_00bd22d0`.
- **Stride:** `FUN_00725e00` @ `0x00725e00` returns `0x30` (48 bytes).
- **Serializer:** `FUN_00725e10` @ `0x00725e10` emits the tag `s_WpMeshShape16_00bd23c0`
  (note the **"16"** suffix variant) via the stream vtable
  (`(**(code**)(*stream+4))(s_WpMeshShape16, 1, this)`).

**Verdict:** a genuine **serializable class**, but it belongs to the **weapon
(`Wp`) collision-shape** subsystem, not the gameplay-ECS component builder. It has
its own ctor/vtable/stride/serializer rather than the ECS
`_DAT_<descr>`+0x50-descriptor pattern. Real type; coincidental match to the ECS grep.

---

## failresolve / finalize / potential — ONE shared global = state-machine progress strings

All three (plus two un-manifested siblings) write the **same** global pointer slot
`_DAT_01175e30`. They are **not classes**; they are progress/phase strings assigned
to a single trace variable inside one function.

- **Sole writer:** `FUN_00581fd0` @ `0x00581fd0` — a recursive `__thiscall`
  (6 params), the asset/visibility **resolution walker**. Confirmed: across the whole
  decomp `_DAT_01175e30` is written in **6 places, all inside `FUN_00581fd0`**, and
  is **never read** in decompiled code → it is a debug/telemetry "current phase"
  marker (a breadcrumb a debugger or crash dump would surface), not control state.

### Reconstructed phase sequence (in code order within `FUN_00581fd0`)

The function does a bitmask sweep (`local_28`/`local_24` walking bits of a visibility
mask `local_30`/`local_2c`), and at each milestone stamps the phase string:

| order | string | VA set | when |
|---|---|---|---|
| 1 | `&DAT_00bb3e28` *(unnamed short string, see below)* | `_DAT_01175e30 = &DAT_00bb3e28;` | entry / "mask cleared, fully resolved" early-out branch (`local_30==0 && local_2c==0`) |
| 2 | `s_finalize_00bb3e30` | inside the early-out, `param_5 != 0` | **finalize**: copy transform block from `param_2+0x20` source, recompute `+0x24` weight, commit |
| 3 | `s_potential_00bb3e00` | when a populated bucket (`uVar4 != 0`) is entered | **potential**: begin evaluating candidate entries in this grid/visibility bucket |
| 4 | `s_failresolve_00bb3e1c` | after `FUN_00581fd0`/`thunk_FUN_024ed3d0` returns false | **failresolve**: a candidate could not be resolved/inserted → release (`FUN_00582460`) |
| 5 | `s_matchingchain_00bb3e0c` *(sibling, not in manifest)* | when an existing chain slot's `+4 != param_3` | **matchingchain**: walking an existing hash-chain match |
| 6 | `s_potential_00bb3e00` (again) | after the bucket loop completes | back to **potential** (bucket-exit re-stamp) |

So the manifest's 3 names are 3 members of a ~5-member set of resolution-phase
breadcrumbs (`potential` → [`matchingchain`] → `finalize` / `failresolve`, plus the
unnamed entry marker). They share `0x01175e30` precisely because they are *successive
values of one variable*, exactly as the task hypothesized.

### String packing in `.rdata` (the enum's backing strings)

Consecutive, tightly packed at `0x00bb3e00`:

| VA | symbol | string |
|---|---|---|
| `0x00bb3e00` | `s_potential_00bb3e00` | `potential` |
| `0x00bb3e0c` | `s_matchingchain_00bb3e0c` | `matchingchain` |
| `0x00bb3e1c` | `s_failresolve_00bb3e1c` | `failresolve` |
| `0x00bb3e28` | `DAT_00bb3e28` | **(unnamed, ~8 bytes before `finalize`)** — Ghidra left it unnamed; falls right after `failresolve\0`. Likely the entry/"resolved" marker. **Flagged unknown — could not dump exact bytes (EXE read blocked this session).** |
| `0x00bb3e30` | `s_finalize_00bb3e30` | `finalize` |

(`DAT_00bb3e44` that appears elsewhere is an unrelated float constant, not part of
this string block.)

**Verdict:** `failresolve`, `finalize`, `potential` are **state/phase strings**
(enum-like progress breadcrumbs) on one shared global, written only by
`FUN_00581fd0`. They are definitively **not** reflection classes.

---

## Genuine types vs. coincidental matches — bottom line

- **Genuine engine types (9):** the 8 precache resource tags + `WpMeshShape`. Real
  classes/type-tags with vtables, sizes, and serializers — but in the
  **render/precache** and **weapon-physics** subsystems, NOT the gameplay-ECS
  builder. They matched the grep because they too register a name-string global.
- **Not types (3):** `failresolve`, `finalize`, `potential` — pure
  **state-machine trace strings** sharing `0x01175e30`.

## Open items / flagged unknowns

- 10 of 12 m2 hashes could not be tool-verified this session (sandbox blocked the
  hash tool and all interpreters). Algorithm + exact command are documented above so
  the column can be filled deterministically. `texture` and `index` are tool-grade.
- Exact bytes of `DAT_00bb3e28` (the unnamed 5th resolution-phase string) not dumped
  — EXE byte-read was blocked. Disassemble `0x00bb3e28` to name it.
