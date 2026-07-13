# wad_simulator

An engine-accurate simulator of how the Mercenaries 2 engine consumes a WAD: it walks the same load path the game does and reports what would break.

## What it is

`wad_simulator` opens a WAD (optionally overlaid on a base WAD), decompresses every referenced
block, parses the containers, and hands each asset to a consumer that mirrors what the engine's
handler for that asset type actually reads. Anything the engine would choke on comes back as a
diagnostic instead of a crash on the target machine.

Two passes run by default:

1. **ASET hash-ownership validation** — confirms every ASET row's `asset_hash` really exists in the
   block that row claims. Rows split into *verified*, *misrouted* (the hash lives in a different
   block — remappable) and *true ghost* (the hash exists in no block at all).
2. **Engine asset consumption** — prefetches and decompresses the referenced SGES blocks in
   parallel (Rayon), parses each UCFX container, dispatches per type (model, texture, animation,
   material, script, layer/placement, action table, wavebank, soundbank, resident singletons) and
   aggregates the findings into a `SimulateReport`, exportable as JSON.

The report separates **fatal** findings from **advisory** ones. Only fatal findings set the exit
code to 1: access violations, decode errors, `texture_buffer_too_small`, position/vertex/bounds
/structural violations, unresolved cross-references (only when a `--base-wad` was supplied), and
UCFX issues naming codec `0x05`, codec `0x01`, XMA or a streaming clip. Heuristic checks
(`*_advisory`, `needs_investigation`, `dlc_texture_provenance`) are reported but excluded from the
verdict.

## Where it comes from

The WAD structure itself is parsed through `mercs2_formats` (FFCS archive index → SGES compressed
blocks → UCFX containers). The validation rules on top of it were derived from the retail game:

* **ASET row layout** (`src/aset_validate.rs`): 16 bytes, `{ asset_hash, secondary_ref, packed_ref,
  type_id }`, `packed_ref = { block_index:hi16, sub_offset:lo16 }` on PC/LE. `sub_offset == 0xFFFF`
  marks a primary (resolve-by-hash) entry; otherwise `sub_offset` is the **byte offset** of the
  asset's sub-resource descriptor inside the decompressed block. Established against retail
  `game-files/vz.wad`, where all 10,798 non-primary entries resolve by hash in their claimed block.
  See `docs/aset_format.md`.
* **Chunk invariants** (`src/chunk_invariants.rs`): each rule was derived by disassembling that
  chunk's handler in `output/patched/Mercenaries2.exe` (image base `0x00400000`) — e.g. the
  renderable consumer at `0x004a4c40`, which reads each array chunk as `count * record` bytes with
  `count` taken from the 0x10-byte renderable INFO. Tag registry: `docs/ucfx_tag_registry.md`.
* **Action tables** (`src/action_table.rs`): the engine processes type `0x207359C7` (type_id 11) in
  `FUN_0067cfb0`, building a fixed 1024-slot per-row hash table (open addressing, mask `0x3FF`). A
  table with more than 1024 rows fills it and the next linear probe at `0x0067D130` spins forever —
  the deterministic world-load livelock this consumer exists to catch.
* **`.pws` streaming audio** (`src/pws.rs`): a PC `.pws` is headerless blob storage with no
  self-describing layout — verified on retail `music.pws`, `ambience.pws` and
  `vo_stream.english.pws`, none of which carry `RIFF`/`OggS`/IMA markers. Format lives in the
  wavebank clip record (codec `0x04` = streamed), so the audit only confirms presence and size.
* **Asset names** are not stored in the PC WAD — only `pandemic_hash_m2(name)`. Names come back via
  a rainbow table (`--rainbow-table`, see `src/names.rs`) and via the side tools below, which mine
  preimages out of the WAD payloads and out of the **console** WADs (the PS3/360 bakes ship an
  uncompressed block-path/name table the PC bake strips).

## Usage

Validate a patch WAD against the base game, resolving cross-references into the sibling WADs, and
write a JSON report:

```bash
cargo run --release -p wad_simulator -- \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad \
  --base-wad-dir output/data \
  --json-output build/validation.json
```

`--base-wad-dir` scans a game `data/` directory and loads the ASET of every non-patch WAD it finds
(English, shell, Loading, vz), so references into a sibling WAD do not false-report as unresolved.
The patch (`--wad`) and the primary base (`--base-wad`) are skipped rather than reloaded.

ASET hash-ownership only, no asset consumption:

```bash
cargo run --release -p wad_simulator -- \
  --wad output/data/vz-patch.wad --skip-assets
```

Audio and `.pws` only, against the PC streaming audio directory:

```bash
cargo run --release -p wad_simulator -- \
  --wad output/data/vz-patch.wad \
  --audios-dir "Data/Audios" \
  --audio-only
```

Exit code is 0 when no fatal finding was recorded, 1 otherwise — so the command can gate a build.

### Options

| Option | Purpose |
|--------|---------|
| `--wad` | Primary WAD; patch, or the single WAD to analyse. Default `output/data/vz-patch.wad` |
| `--base-wad` | Base game WAD (`vz.wad`) for overlay simulation |
| `--base-wad-dir` | Game `data/` dir; every non-patch WAD there has its ASET loaded for cross-ref resolution |
| `--audios-dir` | External streaming audio dir (PC `Data/Audios`) |
| `--audio-manifest` | `dlc_audio_manifest.json` for streaming-clip → `.pws` mapping. Defaults to `output/analysis/dlc_audio_manifest.json` |
| `--rainbow-table` | `rainbow_table.json`, to annotate unresolved hashes with asset names |
| `--json-output` | Write the `SimulateReport` as JSON |
| `--skip-aset` | Skip the ASET hash-ownership pass |
| `--skip-assets` | Skip asset consumption (ASET-only mode) |
| `--skip-audio` | Skip wavebank/soundbank consumption |
| `--audio-only` | Only audio + PWS (skip mesh/texture/layer consumption) |
| `--limit` | Max ASET rows to validate in the hash-ownership pass (0 = all) |
| `--asset-limit` | Max non-audio assets to consume (0 = all) |
| `--jobs` | Parallel worker threads for block prefetch (0 = auto) |
| `--progress-interval` | Log progress every N assets (default 100) |

## Modules

| Module | Owns |
|--------|------|
| `aset_validate` | ASET hash-ownership validation (verified / misrouted / true ghost) |
| `overlay` | Virtual disk: patch ASET wins over base (last-opened-file-wins) |
| `blocks` | Parallel SGES decompression + per-block UCFX container parse cache |
| `simulate` | Orchestrates the pipeline; builds and prints `SimulateReport` |
| `consume` | Per-asset-type consumer trait and result aggregation |
| `chunk_invariants` | Exe-derived structural invariants applied to every UCFX chunk |
| `model` | Model/mesh consumption (GEOM, STRM, IBUF, BNDS, HIER, PRMG) |
| `texture` | Texture consumption (INFO + BODY/DDS), incl. the DXT mip-chain buffer check |
| `material` | `material_params` / MTRL / PRMT structural checks |
| `animation` | Animation / Havok packfile structural validation |
| `script` | Script consumption (LuaQ / BINN) |
| `placement` | Layer/ECS_NODE Transform validation + `flgs` vz_state placement records |
| `action_table` | ActionTable 1024-slot overflow check (the world-load livelock) |
| `resident` | Resident singletons (watermap, fxdict) |
| `audio` | Wavebank + soundbank consumption, IMA ADPCM decode |
| `pws` | External `.pws` streaming audio audit |
| `names` | Rainbow-table hash → name resolver (`pandemic_hash_m2`) |
| `progress` | Progress lines, always flushed to stderr |

## Binaries

Besides `wad_simulator` itself, the crate ships focused RE tools (`cargo run -p wad_simulator --bin <name>`):

| Binary | Purpose |
|--------|---------|
| `aset_export` | Export every ASET row of one or more WADs with rainbow-table name candidates |
| `aset_external_mine` | Mine asset-name preimages from sources outside the PC WAD (the console name table) |
| `aset_namehunt` | Brute-force the build-generated ASET hashes the rainbow table cannot resolve |
| `aset_target_crack` | Crack one unnamed asset hash at a time from the corpus vocabulary |
| `asset_gap_probe` | Find assets whose textures shipped but whose model did not; exploit the `X` / `X_dm` / `X_nm` / `X_sm` naming convention |
| `block_string_harvest` | Harvest plaintext identifiers from decompressed block payloads, hash them against unresolved ASET hashes |
| `name_expand` | Expand known asset names into unknown siblings via the generated-name grammars |
| `registry_hash_dump` | Decode an x32dbg dump of the engine's global name-hash registry (`0x00DF6B88`) to CSV |
| `vo_extract` | Extract spoken VO from the PC build to named `.wav` |
| `vo_console` | Extract per-line VO for every language from the console (big-endian) build |
| `vo_stream_extract` | Extract the streamed VO out of `vo_stream.<lang>.pws` |
| `cue_probe` | Which wavebank the VO cues route to, and how many waves they expect |
| `soundbank_probe` | Parse a VO character soundbank and find its wave table into `vo_stream.<lang>.pws` |
| `wavebank_scan` | Find every wavebank container in a WAD by walking blocks rather than trusting ASET |
| `wavebank_layout_probe` | Decide the wavebank clip-record layout against every shipped bank |

## Notes / gotchas

* **`--oob-only` is vestigial.** It is still accepted on the command line but nothing reads it. The
  OOB validator it drove (`run_aset_oob`) has been removed: it treated `packed_ref`'s low 16 bits as
  an index into the 16-byte entry table and flagged `sub >= entry_count` as heap corruption. On
  retail `vz.wad` that model held for 10 of 10,798 non-primary entries — it false-flagged ~10,788
  perfectly good rows. The low 16 bits are a **byte offset**, and the authoritative check is
  hash-ownership, which is what runs now.
* 92 retail sub-entries have a `sub_offset` past the end of their decompressed block. All are
  streaming textures (type 27): the in-WAD block is a small descriptor and `sub_offset` indexes the
  *external* texture stream, so it is not bounded by the block (the texture analogue of codec-`0x04`
  audio → `.pws`). Counted as informational, not a defect.
* Unresolved cross-references are only fatal when a `--base-wad` was given; a patch analysed alone
  is expected to reference hashes it does not ship.
* `mercs2_audio` is pulled in with `default-features = false` (decode side only). These CLIs never
  open an output device, and linking `cpal` → `alsa-sys` breaks the 32-bit cross-build outright and
  would give the shipped CLI a runtime ALSA dependency it never calls.

## License

MIT License. See [LICENSE.md](LICENSE.md) for details.
