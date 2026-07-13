# model_forge

Authors a complete static (or rigid-skinned) UCFX **model** container from scratch out of a raw
sized `.mesh` blob — no donor container, nothing overridden. It is the "author a model UCFX from
scratch" endpoint of the injection pipeline: `fbx_preprocess.py --mesh` produces the raw MESH blob,
`model_forge` turns it into a `type_id=19` UCFX container, and `smuggler --inject-extra` smuggles
that container into a WAD as a brand-new asset. Unlike `model_inject`/`inject_parts` (which edit an
existing donor's chunks), this mints a fresh asset hash and emits the full descriptor tree from
first principles via `mercs2_formats::model_build`.

## Synopsis

```
model_forge <mesh.bin> <out.bin> --name <asset_name> [--diffuse 0x<hash>] [--skinned]
```

Positionals are order-sensitive (`mesh.bin` first, `out.bin` second); flags may appear anywhere.

## Arguments

### Positionals (exactly two, in order)

1. **`<mesh.bin>`** — required. Path to the raw sized MESH blob (little-endian; see *Input / output
   formats*). Read and parsed for vertex/triangle data.
2. **`<out.bin>`** — required. Path the forged UCFX container is written to. Overwritten if it
   exists.

The parser collects every non-flag token into a positional list and requires **exactly 2**. Passing
fewer or more (including a stray `--help`, which is not a recognized flag and lands in the positional
list) prints the usage line and exits `2`.

### Flags

- **`--name <asset_name>`** — **required**. The asset name string. The tool computes the new model
  asset hash as `pandemic_hash_m2(name)`; this same hash is used as the root HIER node hash, the
  PRMG-INFO group hash, and (skinned) the skin-palette group hash. This is the hash you inject
  under. If omitted, prints `--name is required` and exits `2`. Takes the **next** argument as its
  value.
- **`--diffuse 0x<hash>`** — optional. The 32-bit texture hash the single material samples for its
  diffuse/specular/normal slots. Value is parsed as hexadecimal; a leading `0x` is stripped
  (`u32::from_str_radix(.., 16)`). Default when omitted: `pandemic_hash_m2("global_defaultdiffuse")`
  — a global-resident base texture so the model binds even before its own texture ships. Takes the
  **next** argument as its value. Note: a value that fails hex parsing silently leaves `diffuse`
  unset and thus falls back to the default (the `.and_then(...ok())` swallows the error).
- **`--skinned`** — optional boolean flag (no value). When present, builds a rigid bone-0 A-pose
  **skinned** container (`build_skinned_model`) instead of the default **static** container
  (`build_static_model`). Default: static.

No flag is repeatable in any meaningful way — a repeated `--name`/`--diffuse` simply overwrites the
prior value with the last occurrence; a repeated `--skinned` stays `true`.

## How the arguments combine

- `--name` is the spine: its `pandemic_hash_m2` result becomes `model_hash`, which is written as the
  root HIER node hash and the group hash(es). The name you pass here MUST equal the hash you later
  inject under (`smuggler --inject-extra 0x<model_hash>:19:<out.bin>`).
- `--diffuse` only affects the MTRL chunk. The chosen hash is patched into **all three** texture
  slots of the single material record (diffuse, specular, normal) — the template's own spec/normal
  hashes are base-resident defaults but are overwritten by the caller's hash so every slot resolves.
- `--skinned` selects the entire tree shape and vertex format:
  - **static** (default): stride-20 `DECL20` vertices (pos f16x4 / uv f16x2 / normal f16x4), a
    `GEOM→MESH→PRMG` tree carrying `STRM`/`AREA`/`IBUF`/`PRMT`, the static/building shader
    (`0x0a164785`) MTRL, and a per-triangle **AREA** chunk.
  - **skinned**: stride-40 `DECL40` vertices (adds COLOR/BLENDINDICES/BLENDWEIGHT/TANGENT, all
    weighted 1.0 to bone 0), a `GEOM→SKIN→PRMG` tree with a 56-byte skin-palette INFO, the
    human-skin shader (`0x406b230e`) MTRL, and **no** AREA chunk.
- Both paths omit every destruction chunk (SEGM/PHY2/STAM/SWIT/NODE/STAT/CHDR/CEXE) — the output is
  a non-destructible static prop / rigid character, so there is no twin-PRMT state pair.

## Input / output formats

**Input `<mesh.bin>`** — a raw little-endian MESH blob (as emitted by `tools/fbx_preprocess.py
--mesh`). There is **no** template/donor `.ucfx` input; the container is built entirely from
constants plus this geometry. Layout:

```
"MESH"                 (4 bytes magic)
u32 nverts
u32 ntris
f32 pos[3*nverts]      positions (engine space: Y-up, metres)
f32 nrm[3*nverts]      normals
f32 uv [2*nverts]      texcoords
u32 tris[3*ntris]      triangle vertex indices
```

Must be at least 12 bytes and begin with `MESH`. Positions/normals/uvs are parallel arrays; `tris`
indexes them. Constraints (enforced in `model_build`): mesh must be non-empty; vertex count ≤ 65534
(u16); the generated triangle-strip length ≤ 65534.

**Output `<out.bin>`** — a self-contained UCFX container: `"UCFX"` magic, a `data_off` header
(`20 + ndesc*20`), an 8-byte reserved field, a `u32 ndesc`, then `ndesc` 20-byte descriptor rows
(`tag`, `u0` body-offset, `size`, `u2` #siblings-after, `u3` #children), the 16-byte-aligned chunk
bodies, and a trailing `CSUM` + `crc32_mercs2` checksum over everything preceding. This is the
**`type_id = 19` (model)** asset payload consumed by the WAD loader. Ship it with:

```
smuggler --inject-extra "0x<model_hash>:19:<out.bin>"
```

where `<model_hash>` = `pandemic_hash_m2(<asset_name>)`. On success the tool prints the forged name,
model hash, diffuse hash, vert/tri counts, output path, and byte size.

## Examples

Forge a static prop, letting the diffuse default to the global base texture:

```
model_forge crate.mesh crate.bin --name props/mod/crate
```
Produces `crate.bin`, a static UCFX model whose asset hash is `pandemic_hash_m2("props/mod/crate")`
and whose material samples `global_defaultdiffuse`.

Forge a static prop bound to a specific shipped texture:

```
model_forge barrel.mesh barrel.bin --name props/mod/barrel --diffuse 0x1a2b3c4d
```
Produces `barrel.bin`; the single material's diffuse/specular/normal all reference `0x1a2b3c4d`.

Forge a rigid-skinned character body (bone-0 A-pose, human-skin shader):

```
model_forge body.mesh body.bin --name chars/mod/newguy --diffuse 0xdeadbeef --skinned
```
Produces `body.bin` with the DECL40 skinned vertex stream and skin-palette INFO.

Smuggle any of the above into a WAD as a new asset:

```
smuggler --inject-extra "0x$(printf %08X <model_hash>):19:barrel.bin"
```
Registers the container under its own model hash at `type_id 19`, overriding nothing.

## Failure modes

All are real early-return / error paths in the source:

- **Wrong positional count** → `usage: model_forge <mesh.bin> <out.bin> --name <asset_name>
  --diffuse 0x<hash>` on stderr, exit `2`. (Also the path taken by `--help`, which is not a
  recognized flag.)
- **Missing `--name`** → `--name is required` on stderr, exit `2`.
- **Mesh file unreadable** → `read <mesh_path>: <io error>` on stderr, exit `1`.
- **Bad magic / too short** (`< 12` bytes or first 4 bytes ≠ `MESH`) → `not a MESH blob` on stderr,
  exit `1`.
- **Builder rejects the mesh** (`build_static_model`/`build_skinned_model` returns `Err`: empty
  mesh, `vertex count N exceeds u16`, or `strip length N exceeds u16`) → `build_static_model:
  <msg>` on stderr, exit `1`. (Message prefix is literally `build_static_model:` on both static and
  skinned paths.)
- **Output not writable** → `write <out_path>: <io error>` on stderr, exit `1`.
- **Malformed MESH body** — the parser trusts the `nverts`/`ntris` header and reads fixed strides
  without bounds-checking against the file length; a header that overstates the counts will
  index past the buffer and **panic** (index-out-of-bounds) rather than return a clean error.

Success exits `0`.
