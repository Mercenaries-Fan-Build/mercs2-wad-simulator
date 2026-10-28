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
| `--wad <path>` | path (the install root, its `data` folder, or the file) | the game-archive chain's answer | See note | No | The base WAD every mode reads: the first step of the game-archive chain (`--wad` → the saved setting → an install the workshop sits inside → `MERCS2_GAME_DIR` / `VZ_WAD` → the Windows registry key; first match wins). **A source that is set but broken (here, a `--wad` with no `vz.wad`) stops the chain: it is never passed over for the next source.** A headless mode with the chain unresolved exits 2 printing every step and why it was not used; the window opens on Settings instead. |
| `--overlay <path>` | path | none | No | **Yes** | Opens a patch WAD *on top of* the base (the retail exe's `vz-patch.wad` last-opened-wins mechanism). Used to layer DLC-ported content (obama, sarah, DLC world blocks) into the workbench. Applied in argument order. Only consulted by the overlay-aware modes (see combining section). |
| `--no-auto-patch` | flag | off (auto-patch on) | No | n/a | Disables the automatic loading of a `vz-patch.wad` found next to the base WAD. By default that sibling patch is auto-opened as the first overlay, exactly as the retail exe does. |

### Modes (choose one; each runs headless and exits)

| Flag | Args | Repeatable | Effect |
|------|------|------------|--------|
| `--list [filter]` | optional substring | No | Dumps the whole catalog — every model then every texture — as `KIND<tag>\t0xHASH\tlabel`. Optional `filter` keeps only rows whose label contains it (case-insensitive). Overlay-aware. |
| `--inventory [class]` | optional class | No | Lists models grouped by inferred vehicle class (helicopter, tank, car, boat, …), derived from the `_veh_<token>_` name segment. Optional `class` filters to one class. Overlay-aware. |
| `--check <name\|0xHASH>` | one asset | No | Loads a model end-to-end through the full LOD-block chain — geometry, textures, bones, clip count, LOD tiers, bbox, destruction state machine, and (when the name maps a character) the AnimationLookup/ActionTable coverage. Also prints asset-layer residency stats. Overlay-aware. |
| `--states <name\|0xHASH>` | one asset | No | Dumps the model's destruction state machine (NODE/STAT family) with names resolved, plus HIER node count and SEGM state tiers. Prints `no state machine` when absent. Overlay-aware. |
| `--audio-export <bank\|query> <dir>` | 2 positional | No | Exports voice-over clips as the Audio page does: the bank of that name as the page lists it (routed from `English.wad` beside the base WAD), or else every clip whose bank, speaker, caption or file name holds every word of the query — each as a 16-bit PCM WAV under its shipped file name, plus `captions.csv` (`file,wave,bank,speaker,caption,length_s`), into `<dir>/<bank or search_query>/`. Reads the samples from `Audios/vo_stream.english.pws`. Exit 0; 2 with the reason when the arguments are missing, nothing matches, or a file cannot be read or written. |
| `--ui-state <file.json>` | one file | No | Not a mode: opens the window in the state the file names and captures the presented frame to its `capture` PNG, then exits 0 (2 with the reason for a name that does not resolve or a frame that cannot be read back). Keys: `page`, `settings_section`, `tab`, `lens`, `filter`, `load`, `shipment` + `contribution`, `bench` + `donor`, and on `"tab": "AUDIO"` `bank`, `clip`, `search` and `select` (applied once the tab's routing is read); `run_wad` / `run_data` (used for this run as Settings' "Use for this run only…" does, after the first load; the capture waits for the reload, and a path that starts no reload exits 2); `capture`; `settle_frames` (default 8). The state applies once the reference data is in, or at once when the window opens on Settings for a chain with no answer. |
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
| `--pack-data [dir]` | optional dir | No | Builds the redistributable `workshop_data/` reference bundle (packed `names.bin`, registry CSVs, ECS schemas, decompiled Lua, and `audio_manifest.json`, committed in the repo's `workshop_data/`) and writes its `manifest.json` last. `dir` defaults to `workshop_data`; a following token starting with `--` is not consumed as the dir. In-repo sources are read from the checkout the binary was built from: names come from the committed `crates/mercs2_workshop/data/production_names.json` (with no such file, from the raw corpora, including the name fragments committed beside it), and `audio_manifest.json` from the committed `workshop_data/` (packing into that folder leaves it in place); a missing one fails before anything is written. Every other source is named by the `notes_docs_dir` key of `.mercs2-local.toml` (and, when `production_names.json` is absent, the `rainbow_table` and `jul08_strings` keys). A missing key or source file fails before anything is written. Does not read the WAD. |
| *(no mode flag)* | — | — | Opens the GUI window (`app::run`). When the game archive or the reference data has no answer, the window opens on Settings with that chain in red and every other page locked; fixing it there loads the game without a restart. |

### Environment variables

| Var | Effect |
|-----|--------|
| `MERCS2_WORKSHOP_DATA` | The second step of the reference-data chain (this run's choice from Settings → `MERCS2_WORKSHOP_DATA` → the Settings data folder → `workshop_data/` next to the exe → a `workshop_data/` found walking up from the CWD → the downloaded copy in the cache; first match wins). It must name a complete reference bundle (a directory holding `manifest.json`): one that does not is an error naming it, never passed over for a later step. With nothing set and nothing found, the tool downloads the release bundle into the cache; if that fails it exits with the download link and the folder to unpack it into. |
| `MERCS2_GAME_DIR`, `VZ_WAD` | A game-archive chain step (see `--wad`): the install root, its `data` folder, or the file. The first one set is the source; set but holding no `vz.wad`, it stops the chain. |

## How the options combine

**One mode runs, chosen by a fixed priority order — not by argument position.** `main` tests the
mode flags in a fixed sequence and the FIRST one present wins; all later ones are ignored. The order
is:

```
--pack-data → --list → --inventory → --mod-new → --audio-export → --export → --export-bundle →
--hash → --block-strings → --hash-file → --states → --import-check →
--tex-check → --tex-scan → --tex-scan-blocks → --tex-png-block → --tex-png → --check
```

So `mercs2_workshop --check foo --list` runs `--list` (it is earlier in the order), and
`--hash x --check y` runs `--hash` and treats `--check` and `y` as more hash inputs. If no mode flag
is present at all, the GUI opens. There is no diagnostic for "you passed two modes"; the loser is
just silently dropped.

**WAD selection is global and evaluated first, before any mode.** `--wad`, `--overlay`,
`--no-auto-patch` are read up front, and the game-archive chain is resolved up front. `--pack-data`
runs without an archive (it only trims names against one) unless a set source is broken, which exits
2. Every other mode — `--hash` and `--hash-file` included, though they never read the WAD — exits 2
when the chain has no answer, printing each step and why it was not used.

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

**Names change labels, not resolution of your input.** The reference bundle's `names.bin` (plus the
embedded curated ASET dictionary) turns hashes into names in the *output* and labels the catalog.
Your `<name|0xHASH>` argument is hashed independently with the built-in m2 hash, so a name you type
resolves the same whether or not the bundle knows it — only how results are *displayed* changes.

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

- **`workshop: the game archive is not set and was not found`** /
  **`workshop: the game archive is not usable: <step> — <why>`**, followed by one line per chain step
  — the game-archive chain has no answer (exit 2). Emitted up front, so it blocks even non-WAD modes
  (`--hash`, `--hash-file`); fix or clear the step named, or give a valid `--wad`.
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
- **`--pack-data: <err>`** — a `.mercs2-local.toml` key the pack needs (`notes_docs_dir`, and
  `rainbow_table` / `jul08_strings` when `crates/mercs2_workshop/data/production_names.json` is absent) is missing, names a
  path that does not exist, or one of the source files under it is missing or unreadable; or
  `audio_manifest.json`, committed in the repo's `workshop_data/`, is missing from the checkout the
  binary was built from. The error names the key or the file; nothing is written.
- **No reference bundle** — no complete `workshop_data/` was found and the download failed. The error
  gives the release page to download `mercs2-workshop-data.zip` from and the folder to unpack it into.
- **`workshop: <path>/<file>: <err>`** at boot — the complete bundle in use is missing, or holds a
  malformed, `audio_manifest.json`. Rebuild it with `--pack-data` or download the release bundle again.
```
