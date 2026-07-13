# inject_static

Conforms a novel rigid (unskinned) mesh into a **real** static-prop / vehicle template UCFX container rather than authoring a model from scratch — the engine rejects hand-built UCFX models at `0x004CC064`. It replaces one drawing group's geometry with the mesh (re-encoded into the *template's own* vertex declaration), neutralises the other groups, and rewrites the host SEGM row to `{node:-1, lod_mask:0x7f}` so the mesh draws unconditionally, in model space, at every LOD tier. Decl / material / shader / chunk layout are preserved. Sits at the front of the model-injection pipeline: its output is a raw UCFX container that `smuggler` then smuggles into a WAD.

## Synopsis

```
inject_static <template.ucfx> <mesh.blob> <out.ucfx> --name-hash 0xHASH
    [--group N | --group largest | --raw-group N] [--raw-groups N,M,...]
    [--all-groups] [--keep-groups] [--neutralize-only]
    [--natural-scale] [--scale S] [--no-flip]
    [--diffuse-from 0xA --diffuse-to 0xB]
```

## Arguments

### Positionals (exactly 3, in order; all required)

Any argument that is not a recognised flag (and not a flag's value) is collected as a positional. The three are consumed strictly by position:

1. `<template.ucfx>` — the donor template. Must be a **raw UCFX container** (first 4 bytes `UCFX`), as produced by `smuggler --dump-container`. Read from disk; wrapped in a 20-byte WAD-block header in memory before injection.
2. `<mesh.blob>` — the novel geometry, a **`MESH` blob** (see *Input / output formats*).
3. `<out.ucfx>` — output path; the raw UCFX container is written here.

If fewer or more than 3 positionals are given, or `--name-hash` is missing/zero, the tool prints usage and exits `2`.

### Flags

| Flag | Value | Default | Req. | Repeat | Effect |
|------|-------|---------|------|--------|--------|
| `--name-hash` | hex u32 (`0x…` or bare hex) | `0` | **yes** | last wins | The new model's name hash. Written into the wrapper block header and passed to the injector as `new_name_hash`. `0` (or unparseable) is treated as unset → usage error, exit `2`. |
| `--group` | `N` (usize) or the literal `largest` | `0` | no | last wins | Selects the target drawing group. `N` = index into the **drawing** list (groups that actually carry geometry). `largest` = the drawing group with the most indices (`usize::MAX` sentinel). Unparseable `N` → `0`. |
| `--raw-group` | `N` (usize) | — | no | last wins | Selects target by **RAW** group ordinal (index into the container's PRMG groups, *not* the drawing list). Encoded as `0x1000_0000 + N`. Use to hit a specific state-machine rendered body group that the has-geometry filter can't isolate (e.g. UH1 group 14). Overrides `--group` (both write the same `group` slot; last one on the command line wins). |
| `--raw-groups` | `N,M,...` (comma list of usize) | empty | no | last wins | Inject the mesh into **exactly** these RAW group ordinals. Out-of-range ordinals (`>= group count`) are silently dropped. Non-empty value overrides `--group`/`--raw-group`/`--all-groups` for target selection. Whitespace around each number is trimmed; unparseable entries are skipped. |
| `--all-groups` | (none) | off | no | — | Inject the mesh into **every** drawing group (guarantees visibility regardless of which group the state machine renders). Ignored if `--raw-groups` is non-empty or `--neutralize-only` is set. |
| `--keep-groups` | (none) | off | no | — | Diagnostic: do **not** neutralise the non-target template groups. Passed through as `keep_groups`. |
| `--neutralize-only` | (none) | off | no | — | Host **no** geometry — empty every drawing group. Used on a vehicle's finer LOD rungs (`_P001_`/`_P002_`) so the template's near-tier geometry can't draw over the conformed mesh in the resident rung. Suppresses all geometry injection and the SEGM rewrite. |
| `--natural-scale` | (none) | off (fit **on**) | no | — | Disable auto-fit. By default the mesh is uniformly scaled + recentred into the template's model-header AABB; this flag injects the mesh at its natural coordinates. |
| `--scale` | `S` (f32) | `1.0` | no | last wins | Multiplies the auto-fit scale (`1.0` = exact fit to the header envelope). Only meaningful while auto-fit is on. Values `<= 0` are treated as `1.0` inside the fit. Unparseable → `1.0`. |
| `--no-flip` | (none) | off (flip **on**) | no | — | Disable triangle-winding reversal. By default each triangle is reversed (`[a,b,c]→[a,c,b]`) to convert Blender/FBX right-handed winding to the engine's left-handed convention; without it, faces are backface-culled → invisible. |
| `--diffuse-from` | hex u32 | none | no | last wins | Source diffuse-material name hash for a single MTRL repoint. |
| `--diffuse-to` | hex u32 | none | no | last wins | Destination diffuse-material name hash. |

A flag that expects a value but is last on the line (no following token) falls back to its default (`--name-hash`→`0`, `--group`→`0`, `--raw-group`→RAW ordinal 0 = `0x10000000`, `--scale`→`1.0`, `--diffuse-*`→`None`, `--raw-groups`→empty).

## How the arguments combine

**Target-group selection** (which group(s) receive the injected geometry) is resolved in this precedence:

1. `--neutralize-only` → target set is **empty**; every drawing group is emptied, no SEGM rewrite, no bbox update.
2. else `--raw-groups N,M,...` (non-empty) → exactly those RAW ordinals (in-range only).
3. else `--all-groups` → every drawing group.
4. else → the single group chosen by `--group`/`--raw-group`/`largest` (the `target_gi`).

`--group`, `--raw-group`, and `--group largest` all write the same internal `group` value, so the **last** of them on the command line wins. `target_gi` is still resolved for the stats line even when `--raw-groups`/`--all-groups` widen the actual injection set.

**Auto-fit** (`fit` = on unless `--natural-scale`) scales the mesh to the template's **model-header AABB** (descriptor row 0's INFO, min@+0x04 / max@+0x10 — the container's model-space bounds), then `--scale S` multiplies that scale. X/Z are centre-aligned to the envelope; Y is **bottom-aligned** (mesh min-Y → envelope min-Y) so skids/feet sit on the ground. With `--natural-scale`, the mesh is injected verbatim and `--scale` has no effect.

**Winding** flips by default; `--no-flip` disables it. Flip happens before strip-ification.

**MTRL repoint**: a repoint is emitted only when **both** `--diffuse-from` and `--diffuse-to` parse to hashes; otherwise no repoint. (The vec is single-element — only one from→to pair.)

**Per group written**, the injector re-encodes the mesh into that group's existing STRM decl/stride, rewrites STRM info (kept stride, new vertex count), index-buffer info + data (triangle strip, `u16`), and the PRMT record (`count = ic-2`). Non-target drawing groups are emptied unless `--keep-groups`. When injecting, the host SEGM row is rewritten to `{node:-1, lod_mask:0x7f}` (recorded as `unbound_seg`), and the injected-mesh bbox drives the PRMG group bounds + top INFO.

## Input / output formats

**Template (`<template.ucfx>`)** — raw UCFX container, magic `UCFX` at offset 0, `>= 8` bytes. Must contain at least one PRMG group with a stream decl carrying a position element (usage 0) and a stride in `8..=256`; groups failing those checks are skipped.

**Mesh blob (`<mesh.blob>`)** — a hand-rolled `MESH` binary, little-endian, `>= 12` bytes:

```
off 0   : "MESH"            (4 bytes magic)
off 4   : nv   u32          (vertex count)
off 8   : nt   u32          (triangle count)
off 12  : positions[nv]     each 3×f32  (x,y,z)          → 12·nv bytes
        : normals[nv]       each 3×f32  (x,y,z)          → 12·nv bytes
        : uvs[nv]           each 2×f32  (u,v)            →  8·nv bytes
        : tris[nt]          each 3×u32  (vertex indices) → 12·nt bytes
```

Tangents are synthesised; joints/weights are empty (rigid mesh only). Hard limits: injected vertex count `<= 65534` (else error), and triangle-strip length `<= 65534` (else error).

**Output (`<out.ucfx>`)** — a **raw UCFX container** (the injector's wrapper block, unwrapped back to just the UCFX payload). This is the same shape as the input template: a drop-in model container. It feeds the WAD-smuggling stage as a **model** — WAD `type_id 19` (`= pandemic_hash_m2("model")`, hash `0x5B724250`). In the pipeline that is:

```
smuggler --inject-extra "0xHASH:19:out.ucfx"
```

where `0xHASH` is the model name hash (match it to `--name-hash`). The tool's own doc-comment refers to this stage as `smuggler --inject-container`; both name the model-container smuggle step.

## Examples

Replace a static-prop template's body with a novel crate mesh, exact fit, default winding flip:

```
inject_static barrel_template.ucfx my_crate.blob crate_out.ucfx --name-hash 0xDEADBEEF
```
→ `crate_out.ucfx`: the mesh conformed into `barrel_template`'s **drawing group 0** (the default target — `drawing[0]`, *not* the largest; pass `--group largest` for that), other drawing groups emptied, SEGM unbound. Injects into the WAD via `smuggler --inject-extra "0xDEADBEEF:19:crate_out.ucfx"`.

Target the specific rendered body group of a helicopter template (raw ordinal 14), at 1.1× fit, remap the diffuse material:

```
inject_static uh1.ucfx tank_body.blob uh1_tank.ucfx --name-hash 0x1A2B3C4D \
    --raw-group 14 --scale 1.1 --diffuse-from 0x11112222 --diffuse-to 0x33334444
```
→ `uh1_tank.ucfx`: mesh in RAW group 14 only, scaled to 110% of the header envelope, one MTRL repoint `0x11112222→0x33334444`.

Guarantee visibility across the whole rendered set (both mask-state body groups):

```
inject_static uh1.ucfx tank_body.blob uh1_all.ucfx --name-hash 0x1A2B3C4D --raw-groups 12,14
```
→ mesh injected into RAW groups 12 and 14; out-of-range ordinals would be dropped.

Silence a finer LOD rung so it can't out-draw the resident conformed mesh:

```
inject_static uh1_P001.ucfx dummy.blob uh1_P001_muted.ucfx --name-hash 0x1A2B3C4D --neutralize-only
```
→ every drawing group emptied, no geometry hosted. Prints `neutralised N drawing group(s)`.

Inject at natural coordinates without winding flip (diagnostic / pre-transformed mesh):

```
inject_static prop.ucfx mesh.blob prop_out.ucfx --name-hash 0xAABBCCDD --natural-scale --no-flip
```

## Failure modes

Exit `2` (usage) — `eprintln!` of the usage string:
- Positional count `!= 3`.
- `--name-hash` missing, `0`, or unparseable.

Exit `1` (`eprintln!` error) — early returns:
- Template read error: `read <template>: <err>`.
- Template not UCFX (`< 8` bytes or magic `!= "UCFX"`): `<template> is not a raw UCFX container`.
- Mesh read error: `read <mesh>: <err>`.
- Mesh not a MESH blob (`< 12` bytes or magic `!= "MESH"`): `<mesh> is not a MESH blob`.
- Output write error: `write <out>: <err>`.
- Injector error, prefixed `inject_static: <e>` — includes: donor payload not UCFX; no PRMG groups; `raw group N out of range (0..count)`; `target group N out of range; drawing=[...]`; `vertex count N exceeds u16` (> 65534); `strip length N exceeds u16` (> 65534).

Exit `0` (success) — prints one of:
- `--neutralize-only`: `neutralised N drawing group(s) — no geometry hosted -> <out> (N UCFX bytes)`.
- normal: `injected V verts / T tris into group G (SEGM seg_id S unbound -> node=-1, lod_mask=0x7f; neutralised N groups); bbox=[...]; MTRL repoints=[...] -> <out> (N UCFX bytes)`.
