# mercs2_workshop

A native developer workshop built on the same `mercs2_engine` renderer the game boots: browse,
faithfully preview, inspect, export and re-inject the models, textures, animation clips and
destruction state-machines packed in a Mercenaries 2 `vz.wad` — all without launching the game.
Run with no arguments it opens a windowed GUI; every workbench action also has a headless flag so
the same code paths are scriptable.

## Synopsis

```
mercs2_workshop [WAD SELECTION]                       # open the GUI window
mercs2_workshop [WAD SELECTION] <MODE> [mode args]    # run one headless mode and exit
```

`<MODE>` is exactly one of the flags in the "Modes" group below. There is **no `--help`** — the
tool hand-rolls its parsing over `std::env::args()`. If you pass several mode flags at once only one
runs (see [How the options combine](#how-the-options-combine)).

A `<name|0xHASH>` argument is either a raw 32-bit m2 hash written `0xDEADBEEF`, or an asset name that
is hashed with the Pandemic m2 hash. For most model/texture modes a name is first stripped of a
leading `_` before hashing (the `_dm`/`_nm`/`_sm` texture-suffix convention), so `mymodel` and
`_mymodel` resolve identically.

## Options

### WAD selection (global — parsed before any mode, apply to every mode that opens a WAD)

| Flag | Value | Default | Required | Repeatable | Effect |
|------|-------|---------|----------|------------|--------|
| `--wad <path>` | path | registry-discovered `vz.wad` | See note | No | The base WAD every mode reads. If omitted, the tool looks up the installed `vz.wad` in the Windows registry. **This resolution happens before any mode dispatch: if it fails and `--wad` was not given, the tool prints an error and exits — even for modes that never open a WAD (`--hash`, `--hash-file`, `--pack-data`).** |
| `--overlay <path>` | path | none | No | **Yes** | Opens a patch WAD *on top of* the base (the retail exe's `vz-patch.wad` last-opened-wins mechanism). Used to layer DLC-ported content (obama, sarah, DLC world blocks) into the workbench. Applied in argument order. Only consulted by the overlay-aware modes (see combining section). |
| `--no-auto-patch` | flag | off (auto-patch on) | No | n/a | Disables the automatic loading of a `vz-patch.wad` found next to the base WAD. By default that sibling patch is auto-opened as the first overlay, exactly as the retail exe does. |
| `--names <csv>` | path | see below | No | No | Overrides the name corpus used to resolve hashes to human names. Default is found by `default_names_csv`: env var `MERCS2_NAMES` if it points at a file, otherwise the first `docs/data/live_registry_hashes.csv` found by walking up from the CWD. Names from the CSV are merged with the embedded curated ASET dictionary and (in a repo checkout) speculative corpora. |

### Modes (choose one; each runs headless and exits)

| Flag | Args | Repeatable | Effect |
|------|------|------------|--------|
| `--list [filter]` | optional substring | No | Dumps the whole catalog — every model then every texture — as `KIND<tag>\t0xHASH\tlabel`. Optional `filter` keeps only rows whose label contains it (case-insensitive). Overlay-aware. |
| `--inventory [class]` | optional class | No | Lists models grouped by inferred vehicle class (helicopter, tank, car, boat, …), derived from the `_veh_<token>_` name segment. Optional `class` filters to one class. Overlay-aware. |
| `--check <name\|0xHASH>` | one asset | No | Loads a model end-to-end through the full LOD-block chain — geometry, textures, bones, clip count, LOD tiers, bbox, destruction state machine, and (when the name maps a character) the AnimationLookup/ActionTable coverage. Also prints asset-layer residency stats. Overlay-aware. |
| `--states <name\|0xHASH>` | one asset | No | Dumps the model's destruction state machine (NODE/STAT family) with names resolved, plus HIER node count and SEGM state tiers. Prints `no state machine` when absent. Overlay-aware. |
| `--export <name\|0xHASH>` | one asset | No | Writes OBJ + MTL + PNG textures to `workshop_export/<name>/`. Overlay-aware. |
| `--export-bundle <name\|0xHASH\|class:X>` | one asset or a class | No | Writes a **lossless** bundle per target: editable glTF + PNG skins + every LOD rung's original container bytes + a reassembly manifest. `class:helicopter` (etc.) bundles every model in that class. Output root set by `--out` (default `workshop_export`). Overlay-aware. |
| `--out <dir>` | path | No | Output root for `--export-bundle` only. Default `workshop_export`. Ignored by every other mode. |
| `--mod-new <name> <donor\|0xHASH> <mesh>` | 3 positional | No | Publishes a NOVEL new-hash model into a patch WAD: imports `mesh` (`.obj`/`.gltf`/`.glb`), grafts it onto the `donor` model's container, injects under `hash(name)`, compresses, SHA-256s and runs a load self-test. Overlay-aware (base + overlays form the source stack). |
| `--mod-group <N>` | usize | No | Target draw-group index for `--mod-new` only. Default `0`; an unparseable value silently falls back to `0`. |
| `--mod-out <path>` | path | No | Output patch-WAD path for `--mod-new` only. Default `vz-mod.wad` next to the base WAD. |
| `--import-check <file>` | one file | No | Parses a foreign `.obj`/`.gltf`/`.glb` and prints what *would* import (verts, tris, draw groups, textures, bbox). Does not write anything. Does not open the WAD. |
| `--tex-check <name\|0xHASH>` | one asset | No | Prints one texture's dims / format / mip count / byte size. **Base WAD only — ignores overlays.** |
| `--tex-png <name\|0xHASH> <out.png>` | asset + path | No | Decodes a texture to PNG, assembling the full mip chain from the finer LOD blocks (falls back to the resident tail). **Base WAD only.** |
| `--tex-png-block <blk> <0xHASH> <out.png>` | block + asset + path | No | Decodes ONE block's texture chunk (a single mip level) to PNG. `blk` is a block index; an unparseable value becomes `0`. **Base WAD only.** |
| `--tex-scan` | flag | n/a | Dims/format of every texture ASET in the base WAD, one pass. **Base WAD only.** |
| `--tex-scan-blocks` | flag | n/a | Walks every block's entry table and reports each texture CHUNK (block, hash, dims, offset, field_c). Ground truth when ASET resolution dedupes to the wrong chunk. **Base WAD only.** |
| `--block-strings <blk>` | block index | No | Prints printable ASCII runs (≥5 chars) from a decompressed block — pairs with `--hash-file` for hash-hunting names embedded in game data. Unparseable `blk` becomes `0`. Opens the overlay stack but decompresses **only the base WAD (`wads[0]`)**. |
| `--hash <names…>` | one or more | consumes rest | m2-hashes every following argument and prints `0xHASH  name`. Consumes ALL remaining args. Does not open the WAD (but WAD resolution must still succeed — see `--wad`). |
| `--hash-file <file>` | one file | No | m2-hashes every non-empty line of `file`. Same WAD-resolution caveat as `--hash`. |
| `--pack-data [dir]` | optional dir | No | Builds the redistributable `workshop_data/` reference bundle (packed `names.bin`, registry CSVs, ECS schemas, decompiled Lua). `dir` defaults to `workshop_data`; a following token starting with `--` is not consumed as the dir. Reads repo corpora, not the WAD. |
| *(no mode flag)* | — | — | Opens the GUI window (`app::run`). |

### Environment variables

| Var | Effect |
|-----|--------|
| `MERCS2_NAMES` | If it points at a file, used as the default name corpus (overridden by `--names`). |
| `MERCS2_WORKSHOP_DATA` | If it points at a directory, used as the `workshop_data` home (else the tool looks next to the exe). |
| `MERCS2_RAINBOW` | Overrides the path to `tools/rainbow_table.json` (the 733k-hash speculative preimage table). Read by `default_rainbow_json`; consumed when `--pack-data` rebuilds the name pack in a repo checkout. If unset, the table is found by walking up from the CWD. |

## How the options combine

**One mode runs, chosen by a fixed priority order — not by argument position.** `main` tests the
mode flags in a fixed sequence and the FIRST one present wins; all later ones are ignored. The order
is:

```
--pack-data → --list → --inventory → --mod-new → --export → --export-bundle →
--hash → --block-strings → --hash-file → --states → --import-check →
--tex-check → --tex-scan → --tex-scan-blocks → --tex-png-block → --tex-png → --check
```

So `mercs2_workshop --check foo --list` runs `--list` (it is earlier in the order), and
`--hash x --check y` runs `--hash` and treats `--check` and `y` as more hash inputs. If no mode flag
is present at all, the GUI opens. There is no diagnostic for "you passed two modes"; the loser is
just silently dropped.

**WAD selection is global and evaluated first, before any mode.** `--wad`, `--overlay`,
`--no-auto-patch` and `--names` are read up front. Crucially, WAD *resolution* also happens up
front: if no `--wad` is given and no registry `vz.wad` is found, the tool exits immediately —
including for `--hash`, `--hash-file` and `--pack-data`, which never actually read the WAD. Point
those modes at any WAD (or install one) to get past the guard.

**The overlay stack is built once and shared, but only some modes consult it.** The stack is: the
auto-loaded sibling `vz-patch.wad` (unless `--no-auto-patch`), then every `--overlay` in argument
order. Later overlays win on hash collisions (last-opened-wins).
- **Overlay-aware** (resolve through the full stack): `--list`, `--inventory`, `--check`,
  `--states`, `--export`, `--export-bundle`, `--mod-new`.
- **Base-WAD-only** (overlays are ignored entirely — they open `wad::open(--wad)` directly): all the
  texture modes — `--tex-check`, `--tex-png`, `--tex-png-block`, `--tex-scan`, `--tex-scan-blocks`.
  To inspect a texture that lives in a patch WAD, pass that patch as `--wad`, not `--overlay`.
- **Partial**: `--block-strings` opens the overlay stack (and logs each overlay) but then dumps
  strings from the base WAD only.
- **Neither**: `--hash`, `--hash-file`, `--import-check`, `--pack-data` do not read the WAD.

**`--no-auto-patch` only matters when a `vz-patch.wad` sits next to the base WAD.** With it, the
auto-patch overlay is dropped from the stack; explicit `--overlay` flags are unaffected. It changes
output only for overlay-aware modes, and only when that sibling file exists.

**Sub-flags are scoped to their mode and inert elsewhere.** `--mod-group` and `--mod-out` are read
only by `--mod-new`; `--out` only by `--export-bundle`. Passing them to any other mode has no effect.

**`--names` changes labels, not resolution of your input.** The corpus is used to turn hashes into
names in the *output* (and to enumerate/label the catalog). Your `<name|0xHASH>` argument is hashed
independently with the built-in m2 hash regardless of `--names`, so a name you type resolves the same
whether or not it is in the corpus — only how results are *displayed* changes.

**`class:` in `--export-bundle` fans out.** `--export-bundle class:tank` selects every model whose
inferred vehicle class matches and bundles each; a plain name/hash bundles exactly one. An empty
match set is a failure (see below). The class taxonomy is the same one `--inventory` prints.

## Examples

```
# Open the GUI on the auto-discovered install (auto-loads a sibling vz-patch.wad).
mercs2_workshop

# GUI on an explicit base WAD plus two patch overlays (obama+sarah), suppressing auto-patch.
mercs2_workshop --wad D:/mercs2/vz.wad --no-auto-patch --overlay obama.wad --overlay sarah.wad
```

```
# Every helicopter model in the catalog (base + auto-patch), with names resolved.
mercs2_workshop --inventory helicopter
#  -> "== helicopter (N) ==" then "0xHASH<tag>  label" per model

# Everything whose label contains "tank".
mercs2_workshop --list tank
```

```
# Full load report for one vehicle: geometry, bones, clips, LOD tiers, destruction SM, ActionTable.
mercs2_workshop --check us_veh_abrams
#  -> asset-layer residency line, then the model's stats + state-machine + character-clip lines

# Its destruction state machine, decoded and name-resolved.
mercs2_workshop --states us_veh_abrams
```

```
# Lossless editable bundle of one model (glTF + PNG skins + raw LOD-rung bytes + manifest).
mercs2_workshop --export-bundle 0x1A2B3C4D --out ./bundles
#  -> ./bundles/<label>/...   (multiple dirs if you use class:heli)

# Quick OBJ+MTL+PNG export.
mercs2_workshop --export civ_hum_beachfemale_a
#  -> workshop_export/civ_hum_beachfemale_a/
```

```
# Decode a texture's full mip chain to PNG (base WAD only — overlays are ignored here).
mercs2_workshop --tex-png us_veh_abrams_dm abrams.png

# Ground-truth every texture chunk per block when ASET dedup is suspect.
mercs2_workshop --tex-scan-blocks
```

```
# Inject a novel new-hash model built from a donor, into a patch WAD, with a load self-test.
mercs2_workshop --mod-new my_custom_tank us_veh_abrams custom-tank.glb --mod-out vz-mod.wad
#  -> "wrote vz-mod.wad (N bytes)", sha256, and a per-target self-test line

# See what a foreign mesh would import as, without touching the WAD.
mercs2_workshop --import-check custom-tank.glb
```

```
# Hash names for hash-hunting; then harvest embedded strings from a block.
mercs2_workshop --wad D:/mercs2/vz.wad --hash us_veh_abrams us_hum_mattias
mercs2_workshop --block-strings 3154

# Build the redistributable reference bundle shipped next to the released exe.
mercs2_workshop --wad D:/mercs2/vz.wad --pack-data workshop_data
```

## Failure modes

- **`workshop: no vz.wad found (install not in registry) — pass --wad <path>`** — WAD resolution
  failed and no `--wad` was given. Emitted up front, so it blocks even non-WAD modes (`--hash`,
  `--hash-file`, `--pack-data`); give any valid `--wad` to proceed.
- **`workshop: cannot open <wad>: <err>`** — the base (or, for the stack, a required) WAD could not
  be opened/parsed. A failing `--overlay` is *not* fatal — it prints `[workshop] overlay <p>: <err>
  (skipped)` and continues without it.
- **`--check '<arg>' (0x...): <err>` / `container parse FAILED`** — the hash resolved to no
  container, or the container/LOD chain would not parse. Check the name/hash and that the owning WAD
  (or overlay) is loaded.
- **`--states '<arg>' (0x...): <err>`** — container extraction failed. `no state machine (NODE/STAT
  family absent)` is *not* an error — that model simply has no destruction SM.
- **`--export ...` / `--export-bundle ...` errors** — per-target `[FAIL] <label> (0x...): <err>`
  lines; the run continues and prints an `exported N bundle(s), M failed` tally.
  `--export-bundle: nothing matched '<arg>'` means the name/hash/class selected zero models.
- **`--mod-new <name> <donor name|0xHASH> <mesh file>`** — fewer than three positional args were
  given. `--mod-new: import <mesh>: <err>` means the mesh file would not parse; `--mod-new: <err>` /
  `--mod-new: worker died` come from the background publish/self-test failing. A `self-test ...: FAIL
  <err>` line means the injected model did not load back cleanly.
- **`--import-check <file>: <err>`** — the foreign model would not parse.
- **`--tex-check '<arg>' (0x...): <err>` / `--tex-png '<arg>' (0x...): <err>`** — no texture at that
  hash in the **base** WAD (remember these modes ignore overlays). `--tex-png-block: no texture
  0x... in block <blk>` means that specific block carries no such chunk. `--tex-png[-block]: PNG
  write failed: <err>` is an output-path/IO error.
- **`--tex-png-block <block> <0xHASH> <out.png>` / `--tex-png <name|0xHASH> <out.png>`** — required
  positional args were missing.
- **`--block-strings <blk>: <err>` / `--hash-file <f>: <err>`** — block decompression failed, or the
  hash-file could not be read. Note an unparseable block index silently becomes block `0` rather than
  erroring.
- **`--pack-data: <err>` / `no name corpora found — run from the repo checkout`** — packing needs the
  repo corpora (found by walking up from the CWD); a bare released binary run outside the checkout
  cannot build the bundle. Missing individual reference files are warned-and-skipped, not fatal.
```
