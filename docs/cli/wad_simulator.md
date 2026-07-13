# wad_simulator

Engine-accurate WAD consumption simulator for Mercenaries 2. It walks the same load
path the game engine does over a WAD (optionally overlaid on a base WAD): validates ASET
hash ownership, decompresses and parses every referenced SGES block, dispatches each asset
to a consumer that mirrors what the engine's handler for that type actually reads, and
aggregates the findings into a console report and an optional JSON export. It is the tool a
modder runs to answer "will the game fault or livelock loading my patch WAD?" before shipping.

## Synopsis

```
wad_simulator [OPTIONS]

# Analyze a single WAD in isolation (defaults --wad output/data/vz-patch.wad):
wad_simulator --wad my_patch.wad

# Full overlay simulation of a patch on top of the base game, with cross-ref resolution:
wad_simulator --wad my_patch.wad --base-wad vz.wad --base-wad-dir game/data/ \
              --rainbow-table tools/rainbow_table.json --json-output report.json
```

There are no positional arguments; every input is a named flag.

## Options

All flags are single-use (none are repeatable). None are strictly required — `--wad` has a
default. Value types are as clap parses them.

| Flag | Value | Default | What it does |
|------|-------|---------|--------------|
| `--wad <WAD>` | path | `output/data/vz-patch.wad` | The primary WAD: the patch in overlay mode, or the sole WAD in single-WAD analysis. Its ASET is validated by the hash-ownership pass, and in overlay its entries override the base (patch-wins). This is the file whose entries carry `AsetSource::Patch`. |
| `--base-wad <BASE_WAD>` | path | none (single-WAD mode) | Base game WAD (`vz.wad`) to overlay `--wad` on top of. Presence flips the run into overlay mode: base ASET is loaded first, patch entries override by hash. **Setting this also makes unresolved cross-references FATAL** (see below). |
| `--audios-dir <AUDIOS_DIR>` | path | none | External PC streaming-audio directory (`Data/Audios`). When set, its `.pws` files are audited (found/validated counts) and the directory is passed to the wavebank consumer so streaming clips can be matched to their `.pws` payloads. |
| `--oob-only` | flag | off | **NO-OP, retained for compatibility.** It drove the old ASET out-of-bounds validator, which was removed (it read the packed-ref low-16 as an entry-table index and false-flagged ~10,788 of 10,798 valid retail rows). The value is parsed but never read. |
| `--limit <LIMIT>` | integer | `0` (all) | Max ASET rows to check in the **hash-ownership pass only**. `0` = all rows. Does not affect asset consumption. |
| `--skip-aset` | flag | off | Skip the ASET Hash Ownership Validation stage entirely. |
| `--skip-audio` | flag | off | Skip Pass 2 (wavebank + soundbank consumption). Audio blocks are also excluded from the parallel prefetch. |
| `--audio-only` | flag | off | Run only audio + PWS validation; skip Pass 1 (model/texture/layer/script/material/etc. consumption) and the texture-buffer sweep. Non-audio blocks are not prefetched. |
| `--asset-limit <ASSET_LIMIT>` | integer | `0` (all) | Max **non-audio** assets to consume in Pass 1. `0` = all. Also caps which non-audio blocks are prefetched. Does not limit audio (Pass 2) or the ASET pass. |
| `--progress-interval <PROGRESS_INTERVAL>` | integer | `100` | Log a progress line every N asset/block steps. Coerced to at least 1 internally. Pass 2 uses `min(interval, 10)`. Cosmetic only — does not change results. |
| `--jobs <JOBS>` | integer | `0` (auto) | Parallel worker threads (rayon) for block prefetch and parse. `0` = rayon default (auto). Performance only — does not change results. |
| `--skip-assets` | flag | off | Skip the entire Engine Asset Consumption stage (all passes, prefetch, texture sweep, xref). Leaves only the ASET hash pass — "ASET-only mode". |
| `--json-output <JSON_OUTPUT>` | path | none | Write the full `SimulateReport` as pretty JSON to this path. Only produced when the asset-consumption stage runs (i.e. not with `--skip-assets`). |
| `--audio-manifest <AUDIO_MANIFEST>` | path | `output/analysis/dlc_audio_manifest.json` (implicit) | `dlc_audio_manifest.json` mapping streaming clip hashes to `.pws` filenames. If omitted, the tool still tries the default path; if that file is absent the map is simply empty (no error). |
| `--rainbow-table <RAINBOW_TABLE>` | path | none | `rainbow_table.json` (hash → name). When loaded, unresolved hashes, texture-buffer findings, and DLC texture-provenance lines are annotated with human-readable asset names. A load failure prints a warning and continues unannotated. |
| `--base-wad-dir <BASE_WAD_DIR>` | path | none | The install `data/` dir. Every `*.wad` in it — except the patch (`--wad`), the primary base (`--base-wad`), and any filename containing "patch" — has its ASET hash set loaded (auxiliary WADs) so cross-references into sibling WADs (English/shell/Loading/vz) don't false-report as unresolved. Aux assets are **not** consumed, only their hash sets loaded. |
| `-h`, `--help` | flag | — | Print help and exit. |

## How the options combine

This is the section that governs what the tool actually produces. The run has two top-level
stages, each independently skippable, and the consumption stage has three passes plus a sweep.

**Two stage gates.**
- `--skip-aset` turns off the ASET Hash Ownership stage.
- `--skip-assets` turns off the entire Asset Consumption stage.
- Setting **both** leaves the tool doing essentially nothing but printing the banner. They are
  independent, not mutually exclusive; each simply drops its stage.

**`--limit` vs `--asset-limit` are different knobs for different stages.** `--limit` bounds only
the ASET hash-ownership pass (how many ASET rows are checked for block ownership). `--asset-limit`
bounds only Pass 1 non-audio consumption (and the matching prefetch set). They never affect each
other, and neither touches audio.

**`--audio-only` vs `--skip-audio` are opposite halves and can cancel out.**
- `--audio-only` runs Pass 2 (audio) + PWS audit and skips Pass 1, the texture-buffer sweep, and
  non-audio prefetch.
- `--skip-audio` runs Pass 1 (+ sweep) and skips Pass 2 and audio prefetch.
- Passing **both** together means Pass 1 is skipped (audio-only) AND Pass 2 is skipped
  (skip-audio): the consumption stage prefetches nothing, consumes nothing, and only cross-ref
  Pass 3 could run — but with no Pass 1 there are no xref sources either, so the report is empty.
  This combination is legal but pointless.

**`--base-wad` changes the *verdict*, not just the inputs.** Without it, the tool runs in
single-WAD mode: unresolved cross-reference hashes are reported but downgraded to a yellow
advisory ("no --base-wad; these likely resolve in vz.wad") and do **not** set the failure exit
code. With `--base-wad` set (`has_base_wad = true`), any unresolved cross-ref becomes FATAL and
forces exit code 1. So the same patch WAD can pass alone and fail under overlay — that is by
design: overlay is the honest test of whether the patch's refs actually resolve. `--base-wad`
also enables the DLC texture-provenance advisory (a patch-origin model whose material diffuse
resolves in the base ASET but is not shipped by the patch → fallback-render risk); that check is
advisory and never fatal.

**`--base-wad-dir` only suppresses false unresolved reports; it cannot create them.** It loads
sibling WADs' ASET hash sets so a ref into `English.wad`/`shell.wad`/etc. counts as resolved. It
does not consume those assets and cannot itself add findings. Its exclusion rules skip the patch,
the primary base, and any filename containing "patch". If the dir is unreadable the tool prints a
warning and proceeds with no aux hashes (refs into siblings may then read as unresolved).

**Cross-ref resolution order (Pass 3).** A referenced hash is considered resolved if it is a
top-level ASET asset in the overlay (base or patch), OR in any aux WAD's ASET (`--base-wad-dir`),
OR the name-hash of any parsed block's internal entry table (embedded sub-resources). Only hashes
failing all three are "unresolved". Because Pass 3's xref sources come exclusively from Pass 1
consumption, `--audio-only` and `--skip-assets` both eliminate xref checking, and `--asset-limit`
shrinks it (fewer models consumed → fewer refs examined).

**The texture-buffer sweep is tied to Pass 1.** After Pass 1, the tool sweeps *every* parsed
block's texture sub-resources (including ones with no ASET row) for the engine's
`BUFFER_TOO_SMALL` mip-chain over-read — the world-load streaming livelock signal, and always
fatal. This sweep only runs when Pass 1 runs, so `--audio-only` and `--skip-assets` disable it.
`--asset-limit` also shrinks the set of blocks that get prefetched/parsed, so a low limit can hide
buffer-too-small textures that a full run would catch.

**Rainbow table is annotation-only.** `--rainbow-table` never changes counts, findings, or exit
code; it only substitutes names into printed/JSON strings for unresolved hashes, buffer-too-small
lines, and provenance lines. A failed load degrades gracefully to unannotated output.

**Audio manifest / audios-dir interplay.** `--audio-manifest` supplies the clip→`.pws` map used
by the wavebank consumer; `--audios-dir` supplies the actual `.pws` files and triggers the PWS
audit. They are most meaningful together (manifest names the clips, dir provides the payloads),
but each works independently: with `--skip-audio` or `--audio-only`+`--skip-audio` neither affects
output because Pass 2 doesn't run.

**Exit code.** `0` unless a fatal finding exists. Fatal = any access violation, decode error,
`texture_buffer_too_small > 0`, position/vertex/bounds/structural violation, a fatal UCFX codec
issue (codec 0x01/0x05, XMA, streaming clip), OR (with `--base-wad`) any unresolved hash. If the
ASET pass reports misrouted or true-ghost hashes, exit is also 1. Advisory findings (ECS-float,
heuristic vertex/bounds/structural advisory, needs-investigation tags, DLC provenance) are
reported but excluded from the verdict.

## Examples

```
# 1. Quick single-WAD sanity check on the default patch WAD.
wad_simulator
# → ASET hash-ownership summary + full consumption report for output/data/vz-patch.wad;
#   unresolved refs shown as advisory (no base), exit 0 unless a hard fault is found.

# 2. Honest overlay test of a DLC patch against the base game, names + JSON.
wad_simulator --wad dlc_mattias.wad --base-wad vz.wad \
              --base-wad-dir "C:/Mercs2/data" \
              --rainbow-table tools/rainbow_table.json \
              --json-output out/mattias_report.json
# → base+patch overlay; sibling WADs resolve cross-refs; unresolved refs are now FATAL;
#   findings annotated with asset names; full report written to out/mattias_report.json.

# 3. ASET-only mode: just verify every ASET row lives in the block it claims.
wad_simulator --wad my_patch.wad --skip-assets
# → only the hash-ownership pass runs (verified / misrouted / true-ghost counts).

# 4. Audio-only pass with the external streaming dir and manifest.
wad_simulator --wad vz.wad --audio-only \
              --audios-dir "C:/Mercs2/Data/Audios" \
              --audio-manifest output/analysis/dlc_audio_manifest.json
# → skips all mesh/texture work; loads wavebanks+soundbanks, audits .pws files,
#   reports streaming-clip resolution.

# 5. Fast smoke test on a big WAD: cap non-audio consumption and skip audio.
wad_simulator --wad vz.wad --skip-audio --asset-limit 500 --progress-interval 50 --jobs 8
# → consumes the first 500 non-audio assets across 8 prefetch threads, logging every 50;
#   NOTE a limit can hide buffer-too-small textures a full run would catch.
```

## Failure modes

- **`--wad` (or `--base-wad`) not found / not a valid FFCS archive.** `VirtualDisk::load` and
  `run_aset_hash_validation` open the file directly; a missing file or a bad archive header
  surfaces as `ASET hash validation failed: <io/parse error>` and/or `Simulation failed: <error>`
  on stderr, with exit code 1. The banner still prints first.
- **`--base-wad-dir` unreadable.** Prints `WARNING: --base-wad-dir <path> not readable` and
  continues with no auxiliary hashes — cross-refs into sibling WADs may then be reported as
  unresolved (fatal if `--base-wad` is also set).
- **`--rainbow-table` load failure.** Prints `WARNING: failed to load rainbow table: <error>` and
  continues; output is unannotated but otherwise complete. Not fatal.
- **`--audio-manifest` missing / unparseable.** No error — the clip→pws map is silently empty
  (`load_clip_pws_map` returns `None`), so streaming clips just won't be matched to `.pws` names.
- **A block fails to decompress.** Recorded per-block as an access violation
  (`block <n> decompress: <error>`), which is fatal (exit 1). In the ASET pass a block that won't
  decompress is counted as a `decompression_failure` instead.
- **`--json-output` path unwritable.** `File::create` / `write_all` errors propagate out of
  `main` and abort with a non-zero exit after the report has already printed to the console.
- **Fatal engine-accuracy findings.** Any access violation, decode error, texture
  BUFFER_TOO_SMALL, position/vertex/bounds/structural violation, fatal UCFX codec, or (with
  `--base-wad`) unresolved hash prints under a red `VERDICT:` line and yields exit code 1 — the
  signal that the game would fault or livelock loading the WAD.
