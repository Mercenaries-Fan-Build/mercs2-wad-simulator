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
| `sfx_namecrack` | Brute-force SFX cue names from the `<bank>_<action>` grammar; recovered names go into `tools/rainbow_table.json` |
| `sfx_route_probe` | Dump one block's full cue → track → sound-group → wave routing |

## SFX banks

Unlike VO — codec-`0x04` wavebank records that only *index* `vo_stream.<lang>.pws`, so reading them
needs the WAD **and** the stream file — every weapon/vehicle/ambience bank ships its samples
**inside its own block** as IMA-ADPCM or PCM16, nothing external. 95 banks / 1316 clips in
`vz.wad`; the 26 `wpn_*` banks alone hold 211 waves / 237.9 s.

```bash
cargo run --release -p wad_simulator --bin sfx_route_probe -- --block wpn_pistol
cargo run --release -p wad_simulator --bin sfx_namecrack -- --wad game-files/vz.wad --filter wpn_
```

### The routing chain (why `sounddb` alone is not enough)

`sounddb` looks like the cue → wave table but is a **red herring for SFX**: `wpn_pistol` has ten
waves and exactly **one** `sounddb` cue, yet all ten play. The real chain lives in the `soundbank`
(`0x9F8BCA10`), matching the engine's own `PgSoundDb` dump (`Sound Groups (%d)`, `Track %d — …
Sounds: %d`):

> **cue → tracks → sound GROUP → N waves with selection weights**

One `wpn_pistol_fire` cue fans out to five groups covering all ten waves (layers, plus weighted
random takes at 0.5/0.5 and 0.333×3). Reading `sounddb` alone names 53 of 211 waves; adding the
group table takes it to **210 of 211**.

```text
+0x04 self_hash  +0x08 group_count(u16)  +0x0A cue_count(u16)
+0x10 data_start  +0x14 sec_b  +0x18 sec_c  +0x1C sec_end
  A [data_start..sec_b)  group records, VARIABLE length
  B [sec_b..sec_c)       group_count u32 offsets (rel. data_start)  -> locates A
  C [sec_c..sec_end)     cue records, VARIABLE length
  tail [sec_end..]       cue_count u32 offsets (rel. sec_c)         -> locates C
```

Three things that will bite a reimplementation:

* **Section A holds two record kinds** — a 104+12n multi-take record and a 64-byte single-wave one
  — so a fixed field offset or a count byte reads garbage on one of them (observed counts of 232
  and 179). Scan each record for `{self_hash, wave_index, weight}` triples instead; that covers
  every wave index exactly once on every shipped weapon bank.
* **Section C records are variable-length too**, so `(sec_end - sec_c) / cue_count` is wrong — on a
  4-cue bank it divides to 211, not even 4-byte aligned, and misreads three of the four cue guids
  as zero. Use the tail offset table, exactly as B locates A.
* **Groups are claimed by several cues** (a `_fire` cue sweeps nearly all of them while
  `_dryfire` / `_reload` take one each), so apply the **narrowest** cue first or the catch-all wins
  every wave.

### Naming

A wavebank record carries only `pandemic_hash_m2(name)`, and a wave has no name of its own — it is
named through whichever cue reaches it. A whole-WAD string sweep yields just **two** cue names
(`wpn_covertpistol_fire_npc`, `wpn_tankgun_fire_npc`, plus `wpn_bomb_timer_01_armed`), but they
expose the grammar `<bank>_<action>[_nn][_npc]`, which `sfx_namecrack` uses to recover 33 of 48
unresolved weapon cue guids (26 distinct cue names). Verified: `wpn_pistol_fire` == `0x8BC8ABF3`.
**Recovered names belong in `tools/rainbow_table.json`** — they are validated preimages, so once
merged every tool that loads the table resolves them without re-running the cracker.

Do **not** point the cracker at wave hashes: 11.8M candidates across perspective/distance/layer
variants recovered exactly one extra name. Wave hashes are not `<bank>_<action>`-shaped — a wave
is named *through its cue*, which is why the group table above is what mattered.

## Notes / gotchas

* **ASET rows key on a small `type_id`, not the type hash.** Filtering `arch.aset` by
  `0xE5273C14` matches *nothing, silently* — `sounddb` is `type_id == 13`, `wavebank` is `6`
  (the hashes are what appear on the UCFX *entries* inside a block). This read a 1198-route cue
  catalog as zero routes with no error anywhere.
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
