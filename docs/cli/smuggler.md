# smuggler

`smuggler` (crate `mercs2_smuggler`) builds a `vz-patch.wad` overlay that overrides Mercenaries 2
model / texture / script assets **by hash** (last-opened-wins) — it never modifies `vz.wad` itself.
It sources each donor model from the block its ASET entry actually points to (so the HIER/structure
the engine instantiates is preserved), rebuilds the block, sges-compresses it, and emits a patch WAD
carrying only the overridden/added blocks plus their ASET entries. Overriding by hash sidesteps the
unsolved ASET name-hash problem.

The same binary also has read-only inspect/extract modes (`--list`, `--dump-container`,
`--dump-block`) for pulling donor containers out of a WAD.

## Synopsis

```
smuggler --source-wad <SOURCE_WAD> [--output <OUT.wad>] [build/inject options]

# Inspect / extract (no --output needed; each exits after running):
smuggler --source-wad vz.wad --list [--target-name a,b]
smuggler --source-wad vz.wad --dump-container donor.ucfx [--block-index N | --target-name s] [--exact-block]
smuggler --source-wad vz.wad --dump-block raw.bin      --block-index N

# Build a patch WAD:
smuggler --source-wad vz.wad -o data/vz-patch.wad                       # cube-ize default crate targets
smuggler --source-wad vz.wad -o data/vz-patch.wad --inject-container m.ucfx --block-index N
smuggler --source-wad vz.wad -o data/vz-patch.wad --extra-only --inject-extra 0xHASH:27:tex.bin
```

`--source-wad` is the only always-required flag. `--output` is required for every build mode but is
**not** needed by `--list`, `--dump-container`, or `--dump-block`.

## Options

| Flag | Value | Default | Req'd | Repeat | Effect |
|------|-------|---------|-------|--------|--------|
| `--source-wad <PATH>` | path | — | **yes** | no | Source `vz.wad` to read the target block(s) from. Opened and parsed as an FFCS archive up front. |
| `-o, --output <PATH>` | path | — | build modes only | no | Output patch WAD path (typically `<game>/data/vz-patch.wad`). Parent dirs are created. Required for all build modes; ignored by `--list`/`--dump-container`/`--dump-block`. |
| `--dump-container <PATH>` | path | — | no | no | **Inspect mode.** Write the donor model's raw UCFX container bytes to `<PATH>` and exit. Reads the model from the first targeted block, then (unless `--exact-block`) resolves the model's ASET-primary source block so HIER/MESH layout is preserved. This is the structural donor a mesh converter needs. |
| `--dump-block <PATH>` | path | — | no | no | **Inspect mode.** Write the whole RAW DECOMPRESSED block (every entry, not just a model container) for the first targeted block to `<PATH>` and exit. Needed for LOD rungs that are sub-entry models with no model ASET row, which `--dump-container` cannot reach. Pair with `--inject-block` to ship an edited copy. |
| `--block-index <N>` | usize | — | no | **yes** | Explicit block index(es) to target. When present, **overrides** `--target-name` for selecting which block(s) to operate on. Repeatable; multiple `--block-index N` build/override multiple blocks in one run. |
| `--exact-block` | flag | off | no | — | Honour `--block-index` VERBATIM for read/write: use exactly that block's model container with NO redirect to the ASET-primary block. The only way to reach the finer LOD rungs of a vehicle (`_P000_Q3` resident → `_P001_` → `_P002_`) and models with no primary ASET row. Affects `--dump-container` and `--inject-container`/cube/`--no-cubeize` source resolution. |
| `--target-name <CSV>` | comma-sep substrings | `deliverycrate,crateaid` | no | no (CSV) | Path substrings (case-insensitive) to auto-select target blocks when `--block-index` is absent. Ignored when `--block-index` is given (except that `--list` always uses `--target-name`). |
| `--list` | flag | off | no | — | List blocks whose path matches `--target-name` **and** contain a model, then exit. Read-only. |
| `--no-cubeize` | flag | off | no | — | Build mode: identity passthrough — copy the donor model container unchanged (no cube-ize). Isolates geometry vs plumbing issues. Overridden by `--inject-container`. |
| `--shape <corner\|clamp>` | string | `corner` | no | — | Cube shape for the default cube-ize mode: `corner` (sharp 8-corner cube) or `clamp`. Validated on every run (invalid value errors), but only has effect when actually cube-izing (i.e. not `--inject-container`, not `--no-cubeize`). |
| `--inject-container <PATH>` | path (UCFX) | — | no | no | Build mode: inject a pre-built model UCFX container in place of cube-izing. The file must start with `UCFX` and be ≥20 bytes. Applied to **every** targeted block (constrain with a single `--block-index`). Overrides `--no-cubeize` and `--shape`. Read at startup — a bad path errors before anything else runs. |
| `--inject-extra <SPEC>` | `0xHASH:TYPEID:path` | — | no | **yes** | Mint an extra single-asset PRIMARY override block from a raw UCFX container. `TYPEID` = `19` model / `27` texture / `35` script. The container file must start with `UCFX` and be ≥20 bytes. E.g. `0x21A2AFD1:27:heart.bin`. Appended after any donor-override blocks. |
| `--extra-only` | flag | off | no | — | Build ONLY the `--inject-extra` blocks; do NOT touch any donor block. For from-scratch assets that override nothing existing. Skips block-selection emptiness errors. |
| `--inject-block <SPEC>` | `<path_substr>:<file>` | — | no | **yes** | Ship a raw DECOMPRESSED block override. Looks the source block up by path substring, carries its existing ASET entries + path forward, sges-compresses `<file>`, and overlays it. For content-additive overrides (augmented `layers_static` placements, edited resident-script blocks). Appended after donor + extra blocks. Composable with `--extra-only`. |
| `-v, --verbose` | flag | off | no | — | Print per-model cube-ize stats (mesh count, verts reshaped) during cube-ize builds. |
| `-h, --help` | flag | — | — | — | Print help. |

### `--inject-extra` TYPEID values

Only these ASET `type_id`s resolve to a UCFX type-hash and are accepted:

| TYPEID | Asset | UCFX type_hash |
|--------|-------|----------------|
| 19 | model | `0x5B724250` |
| 27 | texture | `0xF011157A` |
| 35 | script | `0x42498680` |

Any other `TYPEID` is rejected. Note: the crate's error text names only `19 model / 27 texture`, but
`35 script` is fully supported by the code.

## How the options combine

This is the section that matters. `smuggler` runs a fixed pipeline; understanding the order explains
every interaction.

**1. Startup validation happens before the WAD is even opened.** `--shape` is parsed first (invalid
value → error), then `--inject-container` is read from disk. This means a bad `--shape` or a missing
`--inject-container` file will error out **even in `--list`/`--dump-*` modes**, before any listing or
dumping occurs.

**2. Mode precedence — only one of these runs, and each early-exits:**
   1. `--list` (if set) — lists and exits. It **always** filters by `--target-name` and ignores
      `--block-index`.
   2. `--dump-block` (if set) — dumps the first targeted block's raw bytes and exits.
   3. `--dump-container` (if set) — dumps the first targeted donor container and exits.
   4. Otherwise: **build mode**, which requires `--output`.

   If both `--dump-block` and `--dump-container` are given, `--dump-block` wins (it is checked first).
   The dump modes use `indices[0]` — the first targeted block.

**3. Target selection: `--block-index` overrides `--target-name`.** If any `--block-index` is given,
that exact list of indices is used and `--target-name` is ignored for block selection. Otherwise the
tool scans all block paths for any `--target-name` substring (case-insensitive) and targets every
match. Exception: `--list` always uses `--target-name` regardless of `--block-index`.

**4. Block-source resolution: `--exact-block` changes where the model container is read from.** By
default, for a targeted block the tool finds the model's name-hash, then follows that model's
ASET-primary entry to the block the engine actually instantiates and reads the container from there
(correct for a vehicle's resident rung). `--exact-block` disables that redirect and reads/writes the
container in exactly the block being processed (whether it was named by `--block-index` or matched by
`--target-name`) — the only way to reach finer LOD rungs (`_P001_`, `_P002_`) and sub-entry-only
models. This applies to both `--dump-container` and the build modes.

**5. Per-donor build mode: `--inject-container` > `--no-cubeize` > cube-ize (default).** For each
targeted donor block:
   - If `--inject-container` is set, its bytes replace the container (the same file is injected into
     **every** targeted block — use a single `--block-index` to constrain it). `--shape` and
     `--no-cubeize` are then irrelevant.
   - Else if `--no-cubeize`, the donor container is copied unchanged.
   - Else the donor model is cube-ized using `--shape` (`--verbose` prints mesh/vert stats).

   Donor-override blocks are de-duplicated by model name-hash: if two targeted blocks resolve to the
   same model, only the first is emitted.

**6. `--extra-only` suppresses all donor overrides.** With `--extra-only`, block selection is skipped
entirely (no "no blocks matched" error) and only `--inject-extra` / `--inject-block` blocks are
built. Without it, `--inject-extra` and `--inject-block` blocks are simply **appended** to the donor
overrides — the three build sources compose in one WAD.

**7. Output-block ordering** in the final WAD is: donor overrides (cube/inject-container/identity)
first, then `--inject-extra` blocks, then `--inject-block` blocks. The build fails if, after all of
this, zero blocks were produced.

**Meaningful-only-together / mutually-exclusive summary:**
- `--shape` matters only in cube-ize mode (no `--inject-container`, no `--no-cubeize`); still always
  validated.
- `--exact-block` suppresses the ASET-primary redirect for **whichever** blocks are being processed —
  those selected by `--block-index` *and* those matched by `--target-name`. It is most useful with an
  explicit `--block-index` (that is how you name a specific finer rung), but it is not a no-op with
  `--target-name`.
- `--inject-container` overrides `--no-cubeize` and `--shape`.
- `--output` is meaningless (unused) in `--list`/`--dump-*` modes and required in every build mode.
- `--extra-only` makes donor-selection flags (`--target-name`, `--block-index`, `--no-cubeize`,
  `--shape`, `--inject-container`) inert.
- `--list` uses only `--source-wad` + `--target-name`.

## Examples

List every crate block that contains a model:
```
smuggler --source-wad vz.wad --list
```
Prints `[index] path` for each block whose path contains `deliverycrate` or `crateaid`.

List blocks for a specific vehicle:
```
smuggler --source-wad vz.wad --list --target-name ztz98
```

Extract a donor model container (ASET-primary resolved) to feed a mesh converter:
```
smuggler --source-wad vz.wad --dump-container ztz98_resident.ucfx --target-name ztz98
```
Writes the resident rung's raw UCFX container; nothing else happens.

Extract a specific finer LOD rung verbatim:
```
smuggler --source-wad vz.wad --dump-container ztz98_p001.ucfx --block-index 12345 --exact-block
```
Reads block 12345 exactly (no ASET redirect) — reaches the `_P001_` geometry rung.

Dump a whole raw block (including sub-entry models with no ASET row):
```
smuggler --source-wad vz.wad --dump-block ztz98_tracks.bin --block-index 12346
```
Writes the fully decompressed block bytes; edit and re-ship with `--inject-block`.

Build the default cube-ized crate patch (plumbing bisection PoC):
```
smuggler --source-wad vz.wad -o data/vz-patch.wad
```
Overrides every delivery-/aid-crate model with an 8-corner cube.

Inject a custom model into one block:
```
smuggler --source-wad vz.wad -o data/vz-patch.wad --inject-container custom_tank.ucfx --block-index 12345 --exact-block
```
Replaces exactly block 12345's container with `custom_tank.ucfx`.

Ship a from-scratch texture that overrides nothing existing:
```
smuggler --source-wad vz.wad -o data/vz-patch.wad --extra-only --inject-extra 0x21A2AFD1:27:heart.bin
```
Builds a patch WAD containing only the single texture override block for hash `0x21A2AFD1`.

Compose several sources in one WAD:
```
smuggler --source-wad vz.wad -o data/vz-patch.wad \
  --block-index 12345 --exact-block --inject-container tank.ucfx \
  --inject-extra 0xF00D:27:tank_dm.bin \
  --inject-block layers_static_maracaibo:edited_layer.bin
```
Emits a donor-override block (block 12345), plus a texture override, plus a raw edited layer block.

Ship an edited raw block override:
```
smuggler --source-wad vz.wad -o data/vz-patch.wad --extra-only --inject-block ztz98_tracks:ztz98_tracks_edited.bin
```
Looks up the first block whose path contains `ztz98_tracks`, carries its ASET entries forward, and
overlays the edited decompressed bytes.

## Failure modes

All errors print `smuggler error: <msg>` to stderr and exit non-zero. Real paths in the code:

| Message (substring) | Cause |
|---|---|
| `unknown --shape '<x>' (use corner\|clamp)` | `--shape` was neither `corner` nor `clamp`. Checked before the WAD is opened. |
| `read <path>: ...` | `--inject-container` file could not be read (checked at startup, even in inspect modes). |
| `open <path>: ...` | `--source-wad` could not be opened. |
| `FFCS: ...` | `--source-wad` failed to parse as an FFCS archive. |
| `no blocks matched [...] (try --list)` | `--target-name` matched nothing and no `--block-index` was given (and not `--extra-only`). |
| `block_index N >= INDX count M` | A `--block-index` (or dump target) is out of range for the archive. |
| `block N contains no model container` | `--dump-container` target block has no model entry. |
| `model 0x... not in source block N` | The resolved ASET-primary (or exact) block does not actually contain the expected model container. |
| `--inject-container is not a UCFX container` | The inject file is <20 bytes or does not start with `UCFX`. |
| `cube-ize changed container length (unexpected)` | Internal invariant: cube-ize must preserve container byte length. |
| `block N: model has no vertex meshes` | Cube-ize found no vertex streams to reshape in the donor model. |
| `--inject-extra '<spec>' must be HASH:TYPEID:path` | `--inject-extra` did not split into three colon-separated fields. |
| `bad hash in '<spec>': ...` / `bad type_id in '<spec>': ...` | Non-hex hash or non-numeric type_id in an `--inject-extra` spec. |
| `unsupported type_id N (need 19 model / 27 texture)` | `--inject-extra` type_id was not 19, 27, or 35. (Text lists only model/texture; 35 script is also accepted.) |
| `<path> is not a UCFX container` | An `--inject-extra` container file is <20 bytes or lacks the `UCFX` magic. |
| `--inject-block '<spec>' must be <path_substr>:<file>` | `--inject-block` had no `:` separator. |
| `no block path contains '<needle>'` | `--inject-block` path substring matched no block in the source WAD. |
| `block N (<path>) has no ASET entries` | The `--inject-block` source block carries no ASET entries to forward. |
| `--output is required (unless --dump-container)` | A build mode was requested without `-o/--output`. |
| `no model-bearing blocks among the targets` | After building, zero blocks were produced (e.g. all targets lacked models and no extras). |

**Note on paths with colons.** `--inject-extra` splits on the first two colons only, so a Windows
absolute container path (`0xHASH:27:C:\assets\heart.bin`) works. `--inject-block` splits on the first
colon only (`<path_substr>:<file>`), so its `<file>` may also contain a drive-letter colon; keep the
path-substring needle colon-free.
