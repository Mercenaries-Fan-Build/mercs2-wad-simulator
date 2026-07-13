# mercs2_reassemble

Reassembles the decompiled Mercenaries 2 engine out of the 27k-function Ghidra corpus by joining
every attribution source we have onto the master function list, then flags the unmapped residue for
manual review.

## What it is

A single binary (`mercs2_reassemble`). It parses the flat Ghidra export
`output/_ghidra/all_functions_decomp.txt` into a master function list keyed by virtual address, then
merges every attribution source in the repo onto that key. The join key is the VA normalised to
`0x{:08x}`, so `FUN_00478120`, `0x478120` and `0x00478120` all collapse to one identity.

Each address ends up with a *primary* attribution (best hit wins: lower tier, then higher
confidence, then `.json` over `.md`) and a tier:

| Tier | Meaning |
|------|---------|
| T1 | identified — assigned to a subsystem, emitted into a module |
| T2 | referenced only — the address is mentioned somewhere in the corpus but never attributed |
| T3 | unmapped — nothing known |

**Attribution sources** (all optional except the master list; missing ones are skipped with a log line):

| Source | What it contributes |
|--------|---------------------|
| `output/_ghidra/all_functions_decomp.txt` | master list: name, addr, size, callers, decompiled body |
| `docs/data/*code_map*.json`, `*function_map*.json` | structured subsystem code maps (T1) |
| `docs/reverse_engineer/*_code_map.md`, `*_class_map.md`, `*_function_map.md` | prose code maps — every address cited in the file is attributed to the file's subsystem, first mention wins the role snippet (T1, med) |
| `scripts/mercs2_annotations.json` | the load-path annotation map → `asset_load` |
| `output/_ghidra/func_class_map.csv` (from `ExportFuncClass.java`) | Ghidra RTTI class → subsystem (`hkp*`→physics, `hka*`/`hkb*`→animation, `GFx*`→scaleform, D3D/DSOUND/DINPUT/WS2_32→render/audio/input/networking, …) |
| `output/_ghidra/fid_matches.csv` (from `FidApplyExport.java`) | FID byte-signature library matches → real names + CRT/C++-runtime/networking bucket |
| `mods/lua_trace_asi/reference/binding_map.json` | `luaL_Reg` entries → real Lua cfunc names, `scripting_host_binding` (cfunc value may be RVA or VA; both are tried against the master set) |
| recovered symbol names | a function literally named `strlen` *is* the CRT — identity, not inference. Buckets: `crt`, `cpp_runtime`, `havok`, `scaleform_gfx` |
| in-body binary signals | string-labels / imports / class-refs in the decompiled body vote for a subsystem. A decisive vote (top ≥3 hits and ≥2× the runner-up) is promoted to T1/med. One `FESL` reference alone ⇒ `networking` |
| everything else under `docs/`, `mods/`, and `--extra-refs` | T2 "merely referenced" scrape |

Whatever is still unlabeled gets a *probabilistic* classification, kept strictly separate from
evidence: address-locality (an unlabeled function inherits the subsystem of the evidence anchors
bracketing it — both sides agreeing within 0x8000 = `locality-strong`, nearest anchor within 0x2000
= `locality-weak`), then call-graph majority propagation (≥2 neighbour votes, iterated up to 4
passes), then a `securom_drm` region label for anything at/above `0x01000000` with no other signal.
Ghidra bad-disassembly artifacts (`halt_baddata` / `Bad instruction` in the body) are never inferred.

**Outputs** (default `output/engine_reassembled/`):

* `<subsystem>.c` — one regrouped decompiled-C module per subsystem, functions sorted by address,
  each with a header comment carrying system / tier / confidence / source / role / evidence.
* `_unclassified/seg_XXXX.c` — the T2+T3 residue, sharded by the top 16 bits of the address.
* `inferred/<subsystem>.c` — the probabilistic assignments, never mixed into the evidence modules;
  each function's header records the inference method and its basis.
* `MANIFEST.csv` / `MANIFEST.json` — addr → {name, recovered_name, system, raw_system, tier,
  confidence, size, callers, caller_span, in_image, n_sources, source}.
* `REVIEW_QUEUE.md` / `.csv` — the un-attributed functions ranked by `size × (1 + callers)`.
* `COVERAGE.md` — T1/T2/T3 headline + per-subsystem function and code-byte counts.
* `CLASSIFICATION.md` / `.csv` — the evidence / signal / inferred / artifact / unclassified split,
  plus a corroboration table (does the independent body signal agree with the assigned label?).
* `UTILITY_REPORT.md` — how much of the residue is shared library code, and the un-named `FUN_`
  helpers whose callers span ≥3 distinct gameplay subsystems (naming one explains many call sites).

## Where it comes from

Nothing here is a reimplementation of game code — it is a join over the RE corpus of this repo. The
oracles are the Ghidra decompilation of the retail PC executable
(`output/_ghidra/all_functions_decomp.txt`, plus the `ExportFuncClass.java` RTTI export and the
`FidApplyExport.java` FID export), the subsystem code maps under `docs/reverse_engineer/` and
`docs/data/`, the load-path annotations in `scripts/mercs2_annotations.json`, and the Lua binding
table dumped by `mods/lua_trace_asi`.

The subsystem names are the ones those code maps already use; `canon_system` only folds synonym
spellings onto one module (`vehicle`→`vehicles`, `pimp`→`pimp_job_system`, `fx`→`particle_fx`,
`spawner`/`death`/`population_update`→`population_spawner`, …). The raw system string is preserved
in the manifest.

## Usage

Run it from anywhere inside the repo — the repo root is auto-detected by walking up for
`output/_ghidra/all_functions_decomp.txt`:

```
cargo run -p mercs2_reassemble
```

With explicit paths, and scraping an extra directory (e.g. the memory dir) for T2 references:

```
cargo run -p mercs2_reassemble -- \
  --repo-root C:/Users/Shadow/Desktop/notes-on-the-released-game \
  --out C:/Users/Shadow/Desktop/notes-on-the-released-game/output/engine_reassembled \
  --extra-refs C:/Users/Shadow/.claude/projects/c--Users-Shadow-Desktop-notes-on-the-released-game/memory
```

`--extra-refs` may be repeated. Progress (per-source hit counts, inference counts, final
tier1/tier2/tier3 headline) is written to stderr.

## Notes / gotchas

* **The output directory is deleted and recreated on every run** (`remove_dir_all` then
  `create_dir_all`). Do not keep hand-edits in `output/engine_reassembled/`.
* The master export is mandatory: without it, repo-root auto-detection panics
  (`could not locate repo root — pass --repo-root`).
* The caller list in the master export's header is capped at 12, so call-graph propagation does
  **not** use it — callee edges are re-derived by scanning `FUN_` tokens in the decompiled bodies, and
  callers are the inverse of that.
* Bare `0x…` tokens are only accepted as functions when they are 6–8 hex digits *and* present in the
  master set; this is what stops `DAT_`/global/hash addresses in the prose docs from fabricating
  functions. `FUN_`-prefixed tokens are always taken as functions.
* Attributed addresses that are not in the static image (SecuROM/`.securom` thunk targets etc.) are
  still emitted to the manifest with `in_image=0`, and counted in `COVERAGE.md`.
* `inferred/` is probability, not evidence. `CLASSIFICATION.csv`'s `method` column
  (`locality-strong` / `locality-weak` / `callgraph` / `securom-region`) and `corrob` column
  (`corroborated` / `conflict`) are how you filter it; conflicts are the first thing to review.
* Body-signal labels are binary-derived but heuristic — they land in `CLASSIFICATION.md` as their
  own `signal` status, not as verified evidence.
