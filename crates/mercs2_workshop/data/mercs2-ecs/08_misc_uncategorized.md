# ECS Family 08 — Miscellaneous / Uncategorized Components

Mercenaries 2: World in Flames (PC, x86). Reverse-engineered from
the Ghidra decompilation of the game EXE. Hashes from `tools/pandemic_hash.py --m2`.

## How this family deserializes (important)

Unlike the "rich" reflection classes (e.g. WeaponProjectileBase, whose template
`FUN_0065ca70` enumerates fields one-by-one with `FUN_00656210/320/720`), **every
real component in this family uses the flat raw-blob deserializer pattern**:

```c
undefined4 deserialize(param_1, int *param_2 /* stream */) {
    undefined1 local_buf[<stride>];
    (**(code **)(*param_2 + 0x14))(local_buf, <stride>, 0);  // stream->Read(buf, stride, 0)
    iVar1 = DAT_<descriptor_diff_counter>;
    FUN_0064a600(param_1, &stack...);                        // commit/diff against descriptor
    if (iVar1 != DAT_<counter> && ...) FUN_00665590(param_1, DAT_<head>);  // notify dirty
    return 1;
}
```

So the on-disk/stream layout for these is just `<stride>` opaque bytes — there is
**no per-field int/float/enum template** to recover. The field semantics live in the
*consumer* code that reads the committed blob, not in a reflection schema. This is
expected for small "tag"/state components.

Common descriptor constants every builder writes (verified across all 7):
seed `0x9e3779b9`, table-size `0x100`, flag `3`, vtable slot `&PTR_FUN_00bc5ff8`
(shared generic component-interface vtable — NOT class-specific), and a class
deserializer vtable at descriptor base (`&PTR_CopyFromStream_<addr>`, whose dword0
is the deserializer above).

`Update` (manifest row 8) is **not an ECS component** — see its section.

---

## Registry table

| Class | m2 hash | name-str global (col 3) | name str addr | descriptor CopyFromStream vtable | deserializer fn | stride | one-line purpose | suggested family |
|---|---|---|---|---|---|---|---|---|
| Disable3DDecals | `0x69a0e0e4` | `0x017be834` | `0x00bc5598` | `PTR_CopyFromStream_00bc1928` | `FUN_0063d0d0` | 0x04 | Tag: suppress 3D/projected decals on this entity | rendering/decals |
| DisableDecals | `0xff4533e5` | `0x017be7e4` | `0x00bc5588` | `PTR_CopyFromStream_00bc18d8` | `FUN_0063d060` | 0x04 | Tag: suppress decals on this entity | rendering/decals |
| RuntimeAnimationParams | `0x9606e589` | `0x017bf464` | `0x00bc5988` | `PTR_CopyFromStream_00bc2bd8` | (raw 0x28; producers FUN_00535590 / FUN_004?? @147338) | 0x28 | Runtime per-entity animation params (40-byte/5-qword blob, computed each tick) | animation |
| TickDamage | `0x8def82ad` | `0x017be1f4` | `0x00bc53b4` | `PTR_CopyFromStream_00bc0d48` | `FUN_0063c4f0` | 0x10 | Periodic ("tick") damage-over-time state on an entity | combat/damage |
| TimerResponse | `0xc122d3ed` | `0x017bccb4` | `0x00bc4dec` | `PTR_CopyFromStream_00bbe990` | `FUN_00639fe0` | 0x0c | Action/response fired when an associated timer fires | logic/triggers |
| TinyGeometryObject | `0x06468e56` | `0x017bc494` | `0x00bc4c14` | `PTR_CopyFromStream_00bbddb0` | `FUN_00639270` | 0x04 | Lightweight geometry/proxy object (single 4-byte handle/id) | world/geometry |
| TriggerOnTimer | `0xfb35cd6f` | `0x017bcc64` | `0x00bc4dcc` | `PTR_CopyFromStream_00bbe8c8` | `FUN_00639f30` | 0x08 | Fires a trigger/event after a timed interval | logic/triggers |
| Update | `0x6e868ffa` | `0x00cde784` | `0x00b61018` | — (method-name string, not a descriptor) | — | — | **Not a component** — RPC/state-machine method name | (n/a — task/RPC iface) |

Builder functions (each `callers=[<one>]`, write descriptor then publish name string):
Disable3DDecals `FUN_00643c80`; DisableDecals `FUN_00643bd0`; RuntimeAnimationParams
`FUN_006457e0`; TickDamage `FUN_00642ef0`; TimerResponse `FUN_0063fe00`;
TinyGeometryObject `FUN_0063ed20`; TriggerOnTimer `FUN_0063fd60`.

---

## Per-component detail

### Disable3DDecals — `0x69a0e0e4`
- Builder `FUN_00643c80` (line 297855). Descriptor head `_DAT_017be7fc`, diff-counter
  `_DAT_017be814`, CopyFromStream vtable `PTR_CopyFromStream_00bc1928`, stride `4`,
  notify-list dirty bit via `FUN_00665590(.., DAT_017be7fc)`.
- Deserializer `FUN_0063d0d0` (line 292827): `stream->Read(buf, 4, 0)` then commit.
- **Schema:** 4 opaque bytes — almost certainly a marker/flag (bool or small handle).
  No per-field template exists; treat as a tag component.
- **Purpose:** companion to DisableDecals; disables the projected/"3D" decal pass on
  the owning entity. Rendering family.

### DisableDecals — `0xff4533e5`
- Builder `FUN_00643bd0` (line 297822). Head `_DAT_017be7ac`, counter `_DAT_017be7c4`,
  CopyFromStream `PTR_CopyFromStream_00bc18d8`, stride `4`.
- Deserializer `FUN_0063d060` (line 292827): `stream->Read(buf, 4, 0)`.
- **Schema:** 4 opaque bytes (tag/flag).
- **Secondary usage (note):** the *name hash* `0xff4533e5` is also consumed as a
  config/command token at line 99623 — `FUN_00826820(0xff4533e5,0)` then
  `FUN_00826990()` sets `DAT_01175c37` (a global bool). This is a parallel
  command-line/config switch keyed on the same string, not the ECS component
  instance. Worth flagging but does not change the component schema.
- **Purpose:** tag to suppress decal rendering on the entity. Rendering family.

### RuntimeAnimationParams — `0x9606e589`
- Builder `FUN_006457e0` (line 299044). Head `_DAT_017bf42c`, counter `_DAT_017bf444`,
  CopyFromStream `PTR_CopyFromStream_00bc2bd8`, stride `0x28` (40 bytes). Note this
  builder also sets `_DAT_017bf430 = 0x18` (an extra size/count field, distinct from
  the usual 4/8/0xc/0x10 minor strides).
- **No standalone stream-read wrapper** found; instead the 40-byte blob is produced
  at runtime and committed directly via `FUN_0064a600` against `DAT_017bf444`:
  - `FUN_00535590` (line 158015): computes a clamped time/phase value `fVar4`
    (window math `DAT_00df57cc*DAT_00d1e430`, clamp, normalize) and writes a
    5-qword struct (`local_2c..local_c`) seeded from getter `FUN_005857e0()`.
  - Producer at line 147338 (same struct shape, getter `FUN_005857e0()`, plus an
    extra int from `FUN_0068b5b0()`).
  - Another committer at line 160542 (same pattern).
- **Schema (inferred from the 5-qword blob):** 40 bytes = 5 × 8-byte fields. The
  fields are populated from the per-entity animation-state getter `FUN_005857e0()`
  (returns a 40-byte record copied verbatim) with one or two fields overwritten by
  the computed phase/time value. Exact field meanings beyond "current anim params +
  computed phase" are **not recoverable from the schema alone** — flagged unknown.
- **Purpose:** holds the live (per-tick) animation parameters for an entity (phase/
  time-into-window plus the snapshotted anim record). Animation family.

### TickDamage — `0x8def82ad`
- Builder `FUN_00642ef0` (line 297228). Head `_DAT_017be1bc`, counter `_DAT_017be1d4`,
  CopyFromStream `PTR_CopyFromStream_00bc0d48`, stride `0x10` (16 bytes).
- Deserializer `FUN_0063c4f0` (line 292193): `stream->Read(buf, 0x10, 0)` then commit
  against `DAT_017be1d4`, dirty-notify `FUN_00665590(.., DAT_017be1bc)`.
- **Schema:** 16 opaque bytes. By name + size this is a damage-over-time descriptor —
  plausibly {damage-per-tick (float), interval (float), remaining time/count (float),
  damage-type/source (int)} but the layout is **not proven** by a field template;
  flagged. Confirm against the DOT applier that reads `DAT_017be1bc`.
- **Purpose:** periodic ("tick") damage applied to an entity over time (burning, gas,
  etc.). Combat/damage family.

### TimerResponse — `0xc122d3ed`
- Builder `FUN_0063fe00` (line 294952). Head `_DAT_017bcc7c`, counter `_DAT_017bcc94`,
  CopyFromStream `PTR_CopyFromStream_00bbe990`, stride `0x0c` (12 bytes).
- Deserializer `FUN_00639fe0` (line 290202): `stream->Read(buf, 0xc, 0)`.
- **Schema:** 12 opaque bytes (e.g. {response-id/event hash (u32), param (u32),
  param2/flags (u32)} — **inferred, unproven**).
- **Purpose:** the action/response that gets executed when an associated timer
  elapses (pairs with TriggerOnTimer). Logic/trigger family.

### TinyGeometryObject — `0x06468e56`
- Builder `FUN_0063ed20` (line 294132). Head `_DAT_017bc45c`, counter `_DAT_017bc474`,
  CopyFromStream `PTR_CopyFromStream_00bbddb0`, stride `4`.
- Deserializer `FUN_00639270` (line 289508): `stream->Read(buf, 4, 0)`.
- **Schema:** single 4-byte field (a handle/id or flag).
- **Registry usage:** the descriptor's component table (`DAT_017bc478` count,
  `DAT_017bc48c`) is iterated by the reflection-system walker around line 307221
  (`FUN_0065...`), i.e. these instances are enumerated as a set during a system pass.
- **Purpose:** a lightweight ("tiny") geometry/proxy world object — minimal data,
  likely just references a shared geometry/placement by id. World/geometry family.

### TriggerOnTimer — `0xfb35cd6f`
- Builder `FUN_0063fd60` (line 294919). Head `_DAT_017bcc2c`, counter `_DAT_017bcc44`,
  CopyFromStream `PTR_CopyFromStream_00bbe8c8`, stride `8`.
- Deserializer `FUN_00639f30` (line 290161): `stream->Read(buf, 8, 0)`.
- **Schema:** 8 opaque bytes (e.g. {interval (float), flags/target (u32)} — inferred,
  unproven).
- **Purpose:** fires a trigger/event after a timed interval; the "what to fire" half
  is most likely TimerResponse. Logic/trigger family.

### Update — `0x6e868ffa`  (NOT a component)
- Source: `FUN_00983c60` (line 829810). `s_Update_00b61018` is stored into
  `_DAT_00cde784` **alongside** `s_Start`, `s_Cancel`, `s_GetStatus`, `s_Status` into
  a contiguous `PTR_DAT_00cde768..` table, paired with `PTR_DAT_00cdde68` (a shared
  vtable). These entries are then bound via `(**(code**)(*piVar1 + 0x1c))(&table, &LAB_, param_1)`
  — i.e. **method/RPC-name registration for a task/state-machine interface**, not a
  reflection descriptor (no 0x50-byte descriptor, no `0x9e3779b9` seed, no stride,
  no `PTR_FUN_00bc5ff8`).
- **Correct family:** this belongs to the **task / RPC interface** group (the
  Start/Cancel/Update/GetStatus/Status lifecycle), NOT ECS components. Recommend
  re-filing `Update` (and its siblings if they appear in other manifests) there.

---

## Cross-family / re-filing notes
- `Update` → task/RPC interface (lifecycle method name), not ECS. See above.
- `TickDamage` → combat/damage family.
- `Disable3DDecals`, `DisableDecals` → rendering/decals family.
- `RuntimeAnimationParams` → animation family.
- `TriggerOnTimer`, `TimerResponse` → logic/triggers family (and they pair with each other).
- `TinyGeometryObject` → world/geometry family.

## Unknowns flagged
- No per-field templates exist for any real component here (all use the flat
  raw-blob deserializer), so exact field layouts for TickDamage (0x10), TimerResponse
  (0x0c), TriggerOnTimer (0x08), and RuntimeAnimationParams (0x28) are **inferred from
  size + name + consumer code**, not proven from a reflection schema.
- RuntimeAnimationParams' 5-qword field meanings beyond "snapshot of FUN_005857e0()
  record + computed phase/time" are unresolved.
