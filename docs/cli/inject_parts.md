# inject_parts

Conforms a **multi-part** novel model (body + turret + barrel + tracks + …, each its own mesh) into a
real vehicle **template** UCFX container, so it inherits the donor's HIER rig, SEGM/INDX bindings, MTRL
records, shaders and PHY2 collision hulls. Where `inject_static` hosts one rigid mesh in one drawing
group, `inject_parts` binds each part to its own PRMG group, its own HIER node (moving parts spin; static
parts ride the intact-body slot), and its own material — the per-group / per-node / per-material machinery
a vehicle needs. It sits between mesh authoring and packaging: it takes MESH blobs + a donor `.ucfx` and
emits a bare UCFX model container that `smuggler --inject-extra "0xHASH:19:out.bin"` mints as a new asset.
See `docs/modernization/vehicle_model_spec.md` §2/§4 for the binding chain this implements.

## Synopsis

```
inject_parts <template.ucfx> <out.ucfx> --name-hash 0xH \
    --part <mesh>:<group>:<node>:<mtrl_idx>[:spin] [--part ...] \
    [--repoint 0xFROM:0xTO] [--scale S] [--y-offset Y] [--fit-percentile P] [--no-flip] \
    [--node-at <node>:<x>,<y>,<z>] \
    [--set-mtrl <mtrl>:<0xTEX>] [--set-tex <mtrl>:<slot>:<0xTEX>] \
    [--add-mtrl <src>:<0xTEX>] [--replace-mtrl <dst>:<src>:<0xTEX>]
```

## Arguments

### Positionals (exactly two, in order)

Anything not consumed as a flag or a flag-value is collected as a positional. Order is by appearance.

1. `<template.ucfx>` — **required**. Path to the donor vehicle UCFX container (read as raw bytes; must
   begin with `UCFX`). This is the real block whose rig, materials, shaders, SEGM/INDX tables and PHY2
   hulls the novel model inherits.
2. `<out.ucfx>` — **required**. Output path. The conformed UCFX container is written here.

If `pos.len() != 2`, the tool prints the usage line and exits `2`.

### `--part <mesh>:<group>:<node>:<mtrl_idx>[:spin]`  (repeatable, at least one **required**)

Colon-delimited, 4 or 5 fields. `split(':')`; fewer than 4 fields → error + exit `2`.

- **field 0 `<mesh>`** — path to a MESH blob (see Input formats). Loaded immediately; a load error exits
  `1`. The part's `label` in the output report is the mesh's basename (last `/`- or `\`-separated segment).
- **field 1 `<group>`** — `usize` PRMG drawing-group ordinal this part draws into. Parses via
  `.unwrap_or(0)` (a non-numeric value silently becomes `0`).
- **field 2 `<node>`** — `i32` HIER node index that carries this part. `.unwrap_or(-1)`.
  A **real, enabled, non-animated node** is required for a rigid MESH: `-1` indexes the node-matrix array
  out of bounds → garbage transform (source docs the intact-body slot `0x255EAB53` as the correct static
  host; the conform pre-multiplies by `inverse(node.world)` so the part still lands in model space).
- **field 3 `<mtrl_idx>`** — `u32` MTRL record index this group draws with (PRMT word 0). `.unwrap_or(0)`.
- **field 4 `spin`** (optional) — the literal string `spin`. Sets `recenter_xz = true`: re-centres the
  part's X/Z bbox onto the node origin so a part on a **rotating** node spins in place instead of orbiting.
  Any other value (or absent) → `false`. Only use on parts bound to a moving node.

### `--name-hash 0xH`  (**required**)

The new asset's m2 name hash, hex (leading `0x`/`0X` optional). Parsed by `parse_hash`; unparseable → `0`.
The value `0` is treated as absent — if `name_hash == 0` after parsing, the tool prints usage and exits `2`.
Stamped into the output container as its identity.

### `--scale S`  (optional, default `1.0`)

`f32` uniform multiplier applied on top of the automatic template-fit scale. Unparseable → `1.0`.
Also drives PHY2 collision-hull rescaling (reported as `PHY2: rescaled N hull(s) by Sx`).

### `--y-offset Y`  (optional, default `0.0`)

`f32` vertical shift in model space applied after fit. Unparseable → `0.0`.

### `--fit-percentile P`  (optional, default `100.0`)

`f32` percentile (clamped `50.0..100.0` internally) used to measure the model for the **fit scale only**.
`100` = raw bbox. Use `<100` when a thin outlier (antenna/mast) inflates one axis and squashes the whole
model — measuring at e.g. the 99.5th percentile ignores the spike. Placement still uses the true extents,
so the outlier is still drawn; it just doesn't dictate scale. Unparseable → `100.0`.

### `--no-flip`  (optional flag, no value)

Clears the default `flip = true` triangle-winding flip. By default winding is flipped (handedness
conversion); pass `--no-flip` to keep source winding.

### `--node-at <node>:<x>,<y>,<z>`  (repeatable, optional)

Retarget a HIER node to a **model-space point** (post-fit), moving the whole subtree with it — the correct
way to re-rig a novel model's turret ring / gun trunnion onto our tank's real axes instead of inheriting
the donor's. Format: `<node>:<x>,<y>,<z>`. First `:`-field is the `usize` node; the remainder is split on
`,` into exactly 3 `f32`. Anything else → `eprintln!("--node-at needs <node>:<x>,<y>,<z>")` and exit `2`.

### `--repoint 0xFROM:0xTO`  (repeatable, optional)

Rewrite MTRL name-hash references from `FROM` to `TO` across the container. Two `:`-separated hashes, both
via `parse_hash`; if either fails to parse the entry is silently skipped.

### `--set-mtrl <mtrl>:<0xTEX>`  (repeatable, optional)

Set the **diffuse** (texture slot 0) of one MTRL record. `<mtrl>` = `usize` index, `<0xTEX>` = texture
hash. Recorded as `(mtrl, 0, tex)`. Non-parsing fields silently skip the entry.

### `--set-tex <mtrl>:<slot>:<0xTEX>`  (repeatable, optional)

Set **any** texture slot of one MTRL record. `slot 0` = diffuse, `1` = NORMAL, `2` = specular. Three
`:`-fields, all required (`<mtrl>` usize, `<slot>` usize, `<0xTEX>` hash). Malformed →
`eprintln!("--set-tex needs <mtrl>:<slot>:<0xTEX>")` and exit `2`. (Repointing slot 1 as well as slot 0 is
what makes a donor's normal map stop reading as crumpled foil on the novel UVs.)

### `--add-mtrl <src>:<0xTEX>`  (repeatable, optional)

**Append** a material: clone record `<src>` (usize) and give the copy a new diffuse `<0xTEX>`. The new
record's index is the old material count. Non-parsing fields silently skip. **Caution** (from source):
growing the material set past the donor's original count can leave a material with no shader-registry slot,
faulting the renderer on a NULL shader at `0x00855691` when the model is drawn — prefer `--replace-mtrl`.

### `--replace-mtrl <dst>:<src>:<0xTEX>`  (repeatable, optional)

**Replace** material `<dst>` (usize) in place with a clone of `<src>` (usize) + new diffuse `<0xTEX>`,
keeping `dst`'s own name hash and the record **count**. This is the count-preserving alternative to
`--add-mtrl`. Non-parsing fields silently skip. Point `dst` at an unused/untextured record.

## How the arguments combine

- **Required set**: two positionals + `--name-hash` (non-zero) + at least one `--part`. Missing any →
  usage + exit `2`.
- **Global fit**: all parts share ONE transform. The union bbox of every `--part` mesh is fit uniformly to
  the template's model-space AABB, then `--scale` multiplies that, `--fit-percentile` trims outliers from
  the *scale* measurement (not placement), `--y-offset` shifts vertically, and `--no-flip` toggles winding.
  Parts must share the transform or they separate relative to each other; the report prints the final
  `fit: scale …, model bbox …`.
- **Per-part binding**: each `--part` writes geometry into its `group`, binds it to `node` (moving vs.
  `-1`/static), draws with `material_index`, and optionally `spin`-recentres for rotating nodes. The report
  lists `grp / node / mtrl / seg_id / verts / tris` and tags each `[node-local: spins with its node]` or
  `[model space, all tiers]`.
- **Rig retargeting**: `--node-at` moves donor nodes (and subtrees) onto the novel model's real axes,
  applied post-fit; reported as `node N RETARGETED -> (x,y,z)`.
- **Material edits** are applied in this fixed pass order (it matters, because `--add-mtrl` grows the
  record count, so index-based edits that follow it see the appended records): **1.** `--replace-mtrl`
  (in-place clone of an existing record, no count change), **2.** `--add-mtrl` (append a new record),
  **3.** `--set-mtrl` / `--set-tex` (edit a slot on an existing record by index), **4.** `--repoint`
  (raw diffuse-hash value rewrite, reported as `MTRL repoints: N`). Groups not fed by any part are
  neutralised (reported as `neutralised N group(s)`).
- **Collision**: `--scale` also rescales PHY2 convex hulls so the collision shape tracks the visual scale.

## Input / output formats

**Template (`<template.ucfx>`)** — a raw UCFX container (first 4 bytes `UCFX`, ≥20 bytes). A real vehicle
donor block carrying HIER/SEGM/INDX/MTRL/PRMG + PHY2. Not a WAD-wrapped block; the bare container.

**Part meshes (`--part` field 0)** — a hand-rolled little-endian **MESH** blob:

| offset | type | meaning |
|--------|------|---------|
| 0 | `4 bytes` | magic `"MESH"` (else → error) |
| 4 | `u32` | vertex count `nv` |
| 8 | `u32` | triangle count `nt` |
| 12 | `f32[nv][3]` | positions (x,y,z) |
| … | `f32[nv][3]` | normals (x,y,z) |
| … | `f32[nv][2]` | UVs (u,v) |
| … | `u32[nt][3]` | triangle indices |

(joints/weights are left empty — these are rigid parts bound whole to HIER nodes, not skinned.)

**Output (`<out.ucfx>`)** — a **bare UCFX model container** (contiguous bodies, recomputed offsets, CSUM),
stamped with `--name-hash`. This is exactly what `smuggler --inject-extra` expects for a new single-asset
block; the model **type_id is 19**. Feed it via `smuggler --inject-extra "0xHASH:19:out.bin"` (HASH must
match `--name-hash`) to mint the asset into a patch WAD.

## Examples

Single static part (a novel body) hosted on the intact-body slot, one skin:

```
inject_parts tank_donor.ucfx custom_tank.bin --name-hash 0xC0FFEE01 \
    --part meshes/body.mesh:0:626961235:0
```
Produces `custom_tank.bin` — the donor rig with `body.mesh` drawing in group 0 on node `0x255EAB53`
(626961235), material 0, in model space (all LOD tiers).

Multi-part tank: static hull + spinning turret + a barrel, each its own group/material, turret re-rigged:

```
inject_parts tank_donor.ucfx custom_tank.bin --name-hash 0xC0FFEE01 \
    --part meshes/hull.mesh:0:626961235:0 \
    --part meshes/turret.mesh:1:42:1:spin \
    --part meshes/barrel.mesh:2:57:2 \
    --node-at 42:0.0,1.8,0.0 \
    --node-at 57:0.0,1.8,2.4 \
    --set-tex 0:0:0xAABBCCDD --set-tex 0:1:0x11223344 \
    --fit-percentile 99.5 --scale 1.0
```
Hull in group 0 (static), turret in group 1 on node 42 with `spin` (X/Z re-centred so it rotates in place),
barrel in group 2 on node 57; nodes 42/57 retargeted onto our turret ring / trunnion; material 0 gets a new
diffuse + normal; scale measured at the 99.5th percentile so an antenna doesn't squash the model.

Package the result:

```
smuggler --inject-extra "0xC0FFEE01:19:custom_tank.bin" ...
```
Mints the conformed container as a new type-19 (model) asset under hash `0xC0FFEE01`.

## Failure modes

- **Wrong positional count** — `pos.len() != 2` → usage line, exit `2`.
- **Missing/zero name-hash** — `name_hash == 0` (unparsed or literally 0) → usage line, exit `2`.
- **No parts** — `parts.is_empty()` → usage line, exit `2`.
- **Malformed `--part`** — fewer than 4 `:`-fields →
  `--part needs <mesh>:<group>:<node>:<mtrl_idx>[:spin], got '<v>'`, exit `2`.
- **Mesh load error** — file unreadable, `<12` bytes, or first 4 bytes ≠ `MESH` →
  `read <path>: <e>` or `<path> is not a MESH blob`, exit `1`.
- **Malformed `--node-at`** — not `<node>:<x>,<y>,<z>` with 3 floats → `--node-at needs <node>:<x>,<y>,<z>`,
  exit `2`.
- **Malformed `--set-tex`** — not `<mtrl>:<slot>:<0xTEX>` → `--set-tex needs <mtrl>:<slot>:<0xTEX>`, exit `2`.
- **Template not readable** — `read <path>: <e>`, exit `1`.
- **Not a UCFX / too small** — `inject_parts_into_template` returns `template is not a UCFX container`
  (len `<20` or magic ≠ `UCFX`), surfaced as `inject_parts: <e>`, exit `1`.
- **Parts have no geometry** — empty union bbox → `inject_parts: parts have no geometry`, exit `1`.
- **Any other conform error** — surfaced as `inject_parts: <e>`, exit `1`.
- **Output write error** — `write <path>: <e>`, exit `1`.

Silent (non-fatal) skips: unparseable numeric/hash fields in `--scale`, `--y-offset`, `--fit-percentile`,
`--name-hash`, `--repoint`, `--set-mtrl`, `--add-mtrl`, `--replace-mtrl`, and `--part` fields 1–3 fall back
to their defaults (`0`/`-1`/`1.0`/etc.) rather than erroring.
