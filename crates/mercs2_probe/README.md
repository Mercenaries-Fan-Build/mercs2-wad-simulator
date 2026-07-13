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
| `export-c3-obj [outdir]` | `diag::export_c3_obj` |
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
| `gfx-extract [outdir]` | `diag::gfx_extract` — dumps the Scaleform movies (default `output/gfx_movies`) |
| `extract --model 0xHASH [out.bin]` | `wad::extract_container` → file |
| `dump-block <index> [out.bin]` | `wad::decompress_block_index` → file |
| `find-placement <name-substring>` | scans blocks `0..6000`, prints every placement whose name matches |
| `hier --model 0xHASH [names.txt] [--csv path]` | HIER bone tree with name resolution (see below) |
| `overlays <profile-path>` | resolves a save's active `vz_state` layers and folds them through the real streaming catalog |
| `save-dump <profile-path> [grep-needle]` | parsed `.profile` header + the decompressed SaveSingleton Lua payload |

**2. ~34 single-purpose dev bins** under `src/bin/` — each is a standalone investigation, run with
`cargo run -p mercs2_probe --bin <name>`. They cluster into:

* **Model assembly / LOD**: `lod_chunks`, `lodchain_probe`, `model_blocks`, `segm_join`,
  `assembly_probe`, `assembly_check`, `overlap_probe`, `stray_probe`, `viewstate_probe`,
  `header_diff`, `rigid_probe`.
* **Node binding** (which HIER node places which mesh): `indx_dump`, `segfix_probe`, `segm_probe2`,
  `node_witness`, `place_probe`, `hier_dump`, `hier_chain`.
* **Destruction / draw gate**: `sm_dump`, `gate_probe`, `govern_probe`, `health_probe`, `mesh_probe`.
* **Animation**: `action_table_probe`, `charanim`, `clipbind`, `swim_clip_probe`.
* **Textures / materials / FX / lights**: `tex_dump`, `texcmp`, `mtrl_probe`, `fx_probe`,
  `light_probe`.
* **Archive / naming**: `aset_probe` (every ASET row a name-hash owns + a multi-block striping
  census), `model_namer` (derives a model's name from the textures it uses, and can emit a rainbow-
  table name fragment).

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

`vz.wad` auto-discovers from the EA Games registry key; override it with `--wad <path>`:

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

# Scaleform movies out of the WAD
cargo run -p mercs2_probe -- gfx-extract

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

`mercs2_probe` with no subcommand (or `-h` / `--help` / `help`) prints the usage summary and exits 2;
a failing diagnostic exits 1.

## Notes / gotchas

* **`--wad` is a main-binary flag only.** Every `src/bin/*` dev bin resolves `vz.wad` through
  `wad::registry_vz_wad()` and panics if the registry key is absent — they have no override.
* Flag values are consumed positionally: `flag_val` reads the token *after* `--wad` / `--model` /
  `--index` / `--clip` / `--csv`, and `first_positional` skips exactly those five flags and their
  values. A flag value that itself starts with `--` is rejected.
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
