# Production name lookup

`production_names.json` is the **official, committed source of node/asset name lookups** for the
Mercenaries 2 workshop — a curated, hash-verified `pandemic_hash_m2 -> name` map.

- **Bundled here on purpose.** It lives in this repo so `mercs2_workshop --pack-data` can build the
  redistributable `names.bin` with no game WAD and no 32 MB parent-repo `rainbow_table.json`.
- **Every entry is verified.** `m2(name) == hash`; brute-force `_gen_` handles, hex literals, and
  collision debris are excluded. Do NOT hand-edit — regenerate it.

## Regenerate (after new names land in the research corpus)
1. In the parent repo, fold verified names into `tools/rainbow_table.json`.
2. `mercs2_workshop --pack-data <dir>` once WITHOUT this file present (falls back to raw corpora +
   WAD-trim) to refresh `names.bin`, then
3. `python tools/build_production_names.py` to re-export this file from that `names.bin`.
4. Commit `data/production_names.json`. Subsequent `--pack-data` runs build straight from it.
