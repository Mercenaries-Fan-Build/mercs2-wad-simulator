# mercs2_probe

Headless WAD diagnostic and export tool for the Mercenaries 2 modernization — it runs the
diagnostics carved out of `mercs2_engine::diag` as subcommands, and never opens a window.

## What it is

A binary-only crate. It ships two kinds of executable:

**1. `mercs2_probe` itself** (`src/main.rs`) — one subcommand per diagnostic. Most delegate straight
to a `mercs2_engine::diag::*` entry point; a handful are implemented in `main.rs` itself. The full
set, exactly as matched in `main()`:

| subcommand | what it does |
|---|---|
| `c3-meta <out.ndjson>` | `diag::c3_meta` |
| `placement-hashes <out.json>` | `diag::placement_hashes` |
| `export-c3-obj <outdir>` | `diag::export_c3_obj` |
| `terrainmesh-probe [block]` | `diag::terrainmesh_probe` |
| `terrain-probe` / `terrain-consumer` | `diag::terrain_probe` / `diag::terrain_consumer_scan` |
| `wad-list` | `diag::wad_list` |
| `wad-meshes [--model H]` | `diag::wad_meshes` |
| `placement-probe` / `placement-names` | `diag::placement_probe` / `diag::placement_names` |
| `world-index` / `stream-probe` | `diag::world_index_probe` / `diag::stream_probe` |
| `animdiag` / `animcheck` / `skincheck` `[--model H] [--index I]` | the animation diagnostics |
| `trackmap [--model H] [--index I] [--clip H]` | `diag::trackmap` |
| `entity-find [0xKEY ...]` | `diag::entity_find` |
| `comp-probe` / `comp-dump [Name]` | reflection-component census / dump (`comp-dump` defaults to `HibernationControl`) |
| `block-grep <needle>` | `diag::block_grep` |
| `scan-hash <0xH ...>` / `find-ref <0xH ...>` | `diag::scan_hash` / `diag::find_ref` |
| `block-probe <index>` | `diag::block_probe` |
| `veg-census <out.json>` | `diag::veg_census` — understory coverage; writes the vegetation tag-set JSON to `out.json` |
| `tree-census <out.json>` | `diag::tree_census` — c3-cell canopy trees; writes the per-instance list JSON to `out.json` |
| `gfx-extract <outdir>` | `diag::gfx_extract` — writes every `cfx_pack` movie of `--wad` (the container's `data` body, verbatim) |
| `gfx-golden <movies_dir> --md <report.md> --json <index.json>` | inventories extracted movies into the GFx golden reference set; reads files, needs no WAD |
| `extract --model 0xHASH [out.bin]` | `wad::extract_container` → file |
| `dump-block <index> [out.bin]` | `wad::decompress_block_index` → file |
| `find-placement <name-substring>` | scans blocks `0..6000`, prints every placement whose name matches |
| `hier --model 0xHASH [names.txt] [--csv path]` | HIER bone tree with name resolution (see below) |
| `overlays <profile-path>` | resolves a save's active `vz_state` layers and folds them through the real streaming catalog |
| `save-dump <profile-path> [grep-needle]` | parsed `.profile` header + the decompressed SaveSingleton Lua payload |
| `anim-table <name\|0xH> [--handle 0xH] [--list <column>]` | an `animationtable` as the loader holds it: columns, defaults row, data rows, and each row's `ASTO` list |
| `we-template <name\|0xKEY>...` · `--class <Class>` · `--uses <0xU32>` · `--enums` | the retail `worldentity`: a template's component records, a class's schema (field names) and records, every record holding a value, or the enum tables |
| `clip-pose --model H --clip H --bones 0xA,0xB.. --ref 0xR [--frames N]` | a clip posed on a model as the engine poses it: the bones' model-space positions per frame and their positions in the reference bone's frame |
| `dump-read <capture-dir> <0xVA> [len]` | retail memory from an x32dbg capture (`game-files/memory-dumps/<build>/<state>`); no WAD |
| `dump-chain <capture-dir> <0xVA> <0xOFF>...` | follows `[[VA]+o0]+o1…` (the last offset is added, not read) |
| `dump-find <capture-dir> <0xU32>...` | every 4-aligned captured address holding one of the values |
| `dump-find-vec3 <capture> <x> <y> <z> [--tol T]` | every 4-aligned address where three consecutive f32 lie within T (default 0.0005) |
| `dump-containers <capture-dir>` | all 334 live component containers: name, class, type id, live count, stride |
| `dump-pool <capture-dir> <name\|0xVA> [--key 0xK] [--bytes N] [--limit N]` | one container's live records: key, address, bytes |
| `dump-hibernation <capture>` | the hibernation grid (`0x0179C7B0`): viewers, every row (via each SceneObject's block index), awake count; its retail test checks `mercs2_core::hibernation` against every row |
| `dump-spawnlists <capture>` | the runtime unit lists the game built from `PopulationList` data (table `0x016E51F0`): units and weights in stored order |

`<capture>` is either a capture folder (`<build>/<state>/` with `process.dmp`, `SHA256SUMS.txt`,
`capture.log`) or a `<yyyymmdd>_<label>.dmp` listed in a sibling `MANIFEST.md`. The `dump-*` commands
refuse a dump whose bytes do not match the recorded sha256, a folder capture whose exe (`capture.log`
`exe_sha256=`) is not a v1.0 build in `mercs2_exemap`'s `builds.tsv`, and any dump whose own image
fails the three-address v1.0 code-byte test (`pc_exe_build_fingerprints.md`) — only v1.0 shares the
Ghidra decomp's addresses. An address the capture did not hold is an error, never
zeros. Their tests are game-gated (`--features retail`) and read `memory_dumps_dir` from
`.mercs2-local.toml`.

**2. ~100 single-purpose dev bins** under `src/bin/` — each is a standalone investigation, run with
`cargo run -p mercs2_probe --bin <name>`. They cluster into:

* **Model assembly / LOD**: `lod_chunks`, `lodchain_probe`, `model_blocks`, `segm_join`,
  `assembly_probe`, `assembly_check`, `overlap_probe`, `stray_probe`, `viewstate_probe`,
  `header_diff`, `rigid_probe`.
* **Node binding** (which HIER node places which mesh): `indx_dump`, `segfix_probe`, `segm_probe2`,
  `node_witness`, `place_probe`, `hier_dump`, `hier_chain`.
* **Destruction / draw gate**: `sm_dump`, `gate_probe`, `govern_probe`, `health_draws` (drawn/total
  draw groups at full / half / zero health through the full LOD chain), `mesh_probe`.

  > `health_probe` was **removed** (2026-07-26). It printed drawn/total draw groups per health band,
  > but assembled geometry from `extract_container` + `build_indexed_all` on the **resident block
  > alone** — which for a vehicle is the far-LOD proxy (`game_world.rs`: "renders 371-triangle
  > tanks"). Its counts described the coarse stand-in, not what the engine draws, and read as a
  > destruction bug: `ch_veh_apc_wz551` showed 3/13 groups pristine and 0/13 destroyed. Measured
  > through the full LOD chain (`Model::load`, as `govern_probe` does) the same model gives 39/78 →
  > 49/78 → 9/78, which is the expected shape. `health_draws` replaces it, through the LOD chain.
* **Animation**: `action_table_probe`, `charanim`, `clipbind`, `swim_clip_probe`.
* **Textures / materials / FX / lights**: `tex_dump`, `texcmp`, `mtrl_probe`, `fx_probe`,
  `light_probe`, `tex_users` (the reverse of `mtrl_probe`: every model container, in every block,
  whose `MTRL` cites a given texture hash; exit 1 if any block or MTRL could not be read;
  `--unnamed-models` lists every model with no known name with the named textures its own `MTRL`
  cites, `--all` including named models), `tex_orphans` (every texture chunk no `MTRL` of any loader
  cites — model, terrainmesh, font, lowresterrain, scrub — grouped by block, with any other chunk
  that holds the hash as a raw u32). Both read names from `docs/data/aset_names.csv` under the
  `notes_docs_dir` key of `.mercs2-local.toml`.
* **Archive / naming**: `aset_probe` (every ASET row a name-hash owns + a multi-block striping
  census), `model_namer` (derives a model's name from the textures it uses, and can emit a rainbow-
  table name fragment), `kit_templates` (the resident worldentity's spawn templates that wear a given
  model, by their `Name` — the in-game identity of an unnamed model).
* **Water**: `watermap_probe` — resolves the `watr` singleton through `AssetSource::extract_singleton`
  and reports grid, world extent, wet/dry census, waterline range and the Layer 0 × Layer 1 cross-tab.
  The cross-tab is the regression guard on the 44-byte header: a wet cell carrying the dry sentinel
  (or vice versa) means the height field has slipped against the mask, which on retail is silent and
  costs 4,681 coastline cells. It must read `0` on both misalignment rows.

## Where it comes from

`vz.wad` is the oracle. Every probe reads the retail archive and reports what is actually on disk —
nothing here reimplements exe code, it *interrogates the data the exe consumes*, via
`mercs2_engine::{wad, mesh, worldutil, diag}` and `mercs2_formats::{orchestrator, placement, save,
hash}`.

Provenance the source itself records:

* `mercs2_engine::diag` states these subcommands were carved out of the engine's `main.rs` and are
  render-agnostic: they read the WAD and print/emit analysis, never opening a window.
* `sm_dump` carries the cracked destruction state vocabulary (`InitState 0x0ACE072A`,
  `PristineState 0xACB51200`, `DamagedState 0x1D5575A1`, `InitDestroyedState 0x5D308F4F`,
  `DestroyedState 0x7687DF41`, `StartDestroyedState 0x92791EBB`, `GoneState 0xCA261E5B`,
  `DamageMsg 0xC6507EE1`, `DestroyMsg 0x1ED7AD78`, `DestroyMsg2 0x3D0D4C99`).
* `gate_probe` / `viewstate_probe` / `header_diff` are written against the engine's draw gate:
  clause 3 indexes `OBJ+0x2a0` by the SEGM record's signed `node`, and the LOD rung is clamped to
  `[minLOD (M+0x80), maxLOD-1 (M+0x7c)]` — `header_diff` exists precisely because `minLOD`'s
  on-disk source was never located.
* `action_table_probe` documents the container layout it verified in `ucfx_byteswap::convert` +
  `action_table.rs`, and targets the base-game ActionTable (type `animationtable 0x207359C7`,
  asset `0x6802C321`).
* `mesh_probe` cross-references `docs/ucfx_tag_registry.md` §3/§6; `indx_dump` exists to falsify a
  claim in `accessory_bone_binding_B.md`.
* `swim_clip_probe` relies on state names being `pandemic_hash_m2(name)` — verified with
  `m2("Upright") = 0x12C07B18`, `m2("Fidget") = 0x0C0A7FA6`.
* `save-dump` prints the `.profile` header fields this project pinned: character index `0x4D`,
  upgrade index `0x4F`, unlocked costumes `0x24A`, `0x24B`, flags `0x4C`, plus the raw
  `header[0x240..0x260]` window and the zlib-decompressed Lua payload.

## Usage

`vz.wad` auto-discovers from `MERCS2_GAME_DIR` / `VZ_WAD`, then the EA Games registry key; override
it with `--wad <path>`:

```
HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames\Install Dir
```

```bash
# archive census
cargo run -p mercs2_probe -- wad-list
cargo run -p mercs2_probe -- comp-probe
cargo run -p mercs2_probe -- comp-dump HibernationControl

# a model's bone tree, with names resolved and a CSV alongside
cargo run -p mercs2_probe -- hier --model 0x39AF17DC --csv hier.csv

# pull one container / one block out to disk
cargo run -p mercs2_probe -- extract --model 0x39AF17DC model.bin
cargo run -p mercs2_probe -- dump-block 667 block667.bin

# hunt a hash / a string / a placement across every block
cargo run -p mercs2_probe -- scan-hash 0xA3CD72A7
cargo run -p mercs2_probe -- find-placement villa

# save files
cargo run -p mercs2_probe -- save-dump "C:/.../save0.profile" cash
cargo run -p mercs2_probe -- overlays "C:/.../save0.profile"

# Scaleform movies out of the WADs, laid out as the golden set expects (vz at the root)
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies         --wad <game>/data/vz.wad
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies/shell   --wad <game>/data/shell.wad
cargo run -p mercs2_probe -- gfx-extract output/gfx_movies/loading --wad <game>/data/Loading.wad
cargo run -p mercs2_probe -- gfx-golden output/gfx_movies \
    --md docs/reverse_engineer/gfx_golden_set.md --json docs/data/gfx_golden_set.json

# explicit WAD
cargo run -p mercs2_probe -- --wad D:/game/data/vz.wad comp-probe
```

The dev bins take a **model name** (hashed with `pandemic_hash_m2`) or a `0xHASH`, positionally:

```bash
cargo run -p mercs2_probe --bin sm_dump        -- ch_veh_tank_ztz98
cargo run -p mercs2_probe --bin lod_chunks     -- ch_veh_tank_ztz98
cargo run -p mercs2_probe --bin node_witness   -- ch_veh_tank_ztz98
cargo run -p mercs2_probe --bin rigid_probe    -- vz_veh_tank_amx30_elite
cargo run -p mercs2_probe --bin gate_probe     -- oc_veh_helicopter_md500
cargo run -p mercs2_probe --bin charanim                             # defaults to pmc_hum_mattias_v3
cargo run -p mercs2_probe --bin aset_probe     -- 0x9FCAE910 0x89D8DE72
cargo run -p mercs2_probe --bin model_namer    -- --emit fragment.json
```

`mercs2_probe` with no subcommand prints the usage summary and exits 2. `-h` / `--help` / `help` do
the same once a `vz.wad` resolves; with none, they print the "no vz.wad found" message and exit 1.
A failing diagnostic exits 1.

## Notes / gotchas

* **`--wad` is a main-binary flag** (a few dev bins parse their own), and 42 of the `src/bin/*` dev
  bins resolve `vz.wad` through `wad::resolve_vz_wad(None)`, so `MERCS2_GAME_DIR` / `VZ_WAD` override
  those; the rest take a WAD path argument or read `game_paths`' local config. Either variable takes
  the install root, its `data` folder, or the `vz.wad` file itself. The dev bins previously went straight to the EA Games
  registry key and panicked without it, which made them unrunnable off Windows and after a copied-off
  install. The main binary's `--wad` accepts the same folder-or-file forms.
* Flag values are consumed positionally: `flag_val` reads the token *after* a flag such as `--wad` /
  `--model` / `--index` / `--clip` / `--csv`, and `first_positional` skips `--wad`, `--wads`,
  `--model`, `--index`, `--clip`, `--csv`, `--skeleton-csv`, `--md` and `--json` with their values
  (any other `--` token is skipped alone). A flag value that itself starts with `--` is rejected.
* `hier`'s name resolution is rainbow-table first, then an optional plain-text candidate file (one
  string per line, hashed with `pandemic_hash_m2` and accepted only if the hash is actually present
  in this model's HIER) — unresolved nodes print as `?`.
* `model_namer` deliberately does **not** ask "is this candidate's hash anywhere in ASET" — 32-bit
  `pandemic_hash_m2` would fabricate names. It only asks whether a candidate generated from *this
  model's own materials* hashes to *this model's* hash (~50-1000 candidates against one target).
* `aset_probe` exists because `wad::extract_container` resolves **one** ASET row (primary, else any)
  and slices one span out of one block — if an asset's rows span several blocks, that call returns a
  fragment, not the asset.
* `find-placement` brute-scans blocks `0..6000` and stops after 96 consecutive decompress failures.
* `tex_dump` writes uncompressed BMP on purpose (no extra deps) and lives in a separate binary so it
  isn't locked by a running game.
