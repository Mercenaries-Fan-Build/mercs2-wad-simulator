# Name lookups

The workshop's names, committed in the crate that uses them. Every file maps `pandemic_hash_m2` to a
name, and every entry satisfies `m2(name) == hash`.

| File | What it is | Made by |
|---|---|---|
| `production_names.json` | The **official, committed source of node/asset name lookups**: a curated, hash-verified map that `mercs2_workshop --pack-data` builds the redistributable `names.bin` from, and that `qm` (M0130), `mercs2_probe --bin skin_census` and the release workflow read. Brute-force `_gen_` handles, hex literals and collision debris are excluded. Do NOT hand-edit — regenerate it | `tools/build_production_names.py` |
| `aset_discovered_names.json` | Names cracked from the WAD itself (texture-base → model convention, audio names, character kits …). The home for newly cracked names: append, matching its `indent=2` | `asset_gap_probe --emit`, the crackers, by hand after a hash check |
| `aset_expanded_names.json` | Slot-grammar expansion of seed names | `wad_simulator --bin name_expand` |
| `aset_external_names.json` | Block-path stems mined from the console WADs | `wad_simulator --bin aset_external_mine` |
| `aset_roster_names.json` | Model roots from the vehicle/weapon roster × the `<faction>_veh_<class>_<name>` grammar | `aset_external_mine --roster` |
| `aset_model_names.json` | Model names derived from the textures each model uses | `mercs2_probe --bin model_namer` |
| `aset_variant_names.json` | Decorated variants of named models (`vz_veh_tank_amx30_base`) | per-model search against its own hash |
| `rejected_names.json` | Names harvested from block bytes that are collision debris: those with an identifier's shape (`wxfm0u`, `i4e2`, `csumoe9mucfxp`), and `j-cfe`, which `production_names.json` had taken in. Every reader of the harvest, and `--pack-data`'s read of `production_names.json`, drops these pairs | by hand, each hash checked |
| `aset_names.tsv` | The ASET dictionary compiled into the binary (`0xHASH\tname`), applied over `names.bin` | `wad_simulator --bin aset_export --embed-tsv` |

Candidate sources stay in the notes repo's `docs/data/` (named by `notes_docs_dir` in
`.mercs2-local.toml`): `aset_block_strings.json` (identifiers harvested out of block payloads, read
only through `mercs2_formats::hash::is_identifier_shaped` and `rejected_names.json`: a run that hashes
to an asset can be collision debris), `bone_name_candidates.txt`, `live_registry_hashes.csv`; and the
parent repo's `tools/rainbow_table.json`.

## Regenerate `production_names.json` (after new names land)
1. Fold verified names into `aset_discovered_names.json` here (and `tools/rainbow_table.json` in the
   parent repo).
2. `mercs2_workshop --pack-data <dir>` once WITHOUT `production_names.json` present. It then builds
   `names.bin` from the raw corpora: the fragments in this folder, and the `rainbow_table`,
   `jul08_strings` and `notes_docs_dir` keys of `.mercs2-local.toml` (trimmed to the hashes the WAD
   references when a WAD opens); then
3. `python tools/build_production_names.py` to re-export `production_names.json` from that `names.bin`.
4. Commit `crates/mercs2_workshop/data/production_names.json`. Subsequent `--pack-data` runs build
   straight from it.
