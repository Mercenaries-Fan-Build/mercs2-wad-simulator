# destruction_extract

Stage-2 extractor that reads a decompressed WAD block and writes a `destruction.json` describing each model's HIER/SWIT destruction states.

## What it is

A single binary. Given one decompressed block blob, it walks the block's containers
(`mercs2_formats::ucfx::walk_decompressed_block`) and, for every container that looks like a model
— one carrying an `INDX` and/or a `SWIT` switch list — emits a record into `<out-dir>/destruction.json`:

* `model_hash` — the block-table entry's name hash for that container.
* `nodes` — one entry per HIER node: `hier_node`, `parent`, `hash`, `destruction_state`
  (`intact` / `break_piece` / `static`), `switch_group`. Only populated for containers that have a
  `SWIT` (i.e. the *orchestrator* container); geometry containers have `INDX` only and get an empty
  `nodes` list.
* `indx` — the container's `INDX` table (the orchestrator's own if it has one, else a standalone parse).
* `hulls` — grounded PHY2 convex hulls (`{node, vertices}`), placed in model space via the owning
  HIER node's world transform, hull→node taken from `SEGM`. For the viewer overlay.
* `switch_group_count`, `hull_count`, `warnings`.

The top-level manifest is `{"schema": "mercs2_destruction/1", "extractor": "mercs2_formats::orchestrator",
"orchestrated": <bool>, "orchestrated_models": [...]}`. `orchestrated` is true only if some container
produced classified nodes; a block with no switching models still writes the manifest, with
`orchestrated: false`.

All the parsing lives in `mercs2_formats::orchestrator`; this crate is the CLI wrapper plus the JSON
schema.

## Where it comes from

The destructible-model chunks of the game's own model containers: `HIER` (node tree), `SWIT` (switch
list), `SEGM` / `INDX` (sub-object → segment → node), and PHY2 collision hulls. Provenance is recorded
in `mercs2_formats::orchestrator`, which mirrors the engine state machine at `FUN_004cf340`
(`SWIT` + `NODE`/`STAT`/`CHDR`/`CEXE`) and in `docs/modernization/vehicle_model_spec.md` §5 and
`docs/destruction_orchestrator_format.md`.

Downstream: `tools/destruction_join.py` maps a stripped geometry block's submeshes onto its
orchestrator's `destruction.json` by model hash + HIER node, so the workbench can show one destruction
state at a time.

## Usage

```
destruction_extract <blob> --out-dir <dir>
```

e.g. as the asset pipeline drives it (`scripts/stage2_parallel.sh`):

```sh
cargo build --release -p destruction_extract     # or: make build-destruction-extract
./target/release/destruction_extract path/to/block_1234.bin --out-dir output/review/1234
# -> output/review/1234/destruction.json
```

Argument parsing is positional-plus-`--out-dir`; both are required (missing either prints the usage
line and exits 2). Read errors exit 1, success exits 0 and prints the destructible-model count.

## Notes / gotchas

* **The `intact` / `break_piece` / `static` classification is a heuristic and is superseded.** This
  binary calls `orchestrator::classify`, which infers the states from `SWIT` sibling structure (break
  root = the switch-group root with the most descendants also in `SWIT`). `mercs2_formats::orchestrator`
  now also has the real recovered state machine (`parse_state_machine` + `machine_node_enable`,
  `PristineState` `0xACB51200` / `DestroyedState` `0x7687DF41`), which knows about health, states and
  messages; the heuristic does not, and its "static = always rendered" category is misleading. Treat
  `destruction_state` in the JSON as a label, not ground truth.
* `INDX` is **not** a mesh→node map: it is keyed by sub-object ordinal and yields a `seg_id` into
  `SEGM`, whose `bone` is the node (see `vehicle_model_spec.md` §2).
* PHY2 hulls are emitted for display; they are not used to assign per-node states (hull→piece bbox
  containment is ambiguous).
