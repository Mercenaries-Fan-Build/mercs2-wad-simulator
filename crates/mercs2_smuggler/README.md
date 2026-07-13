# mercs2_smuggler

Asset-injection / override patch builder for Mercenaries 2: smuggles new or replacement
model/texture/script assets into the game by building a `vz-patch.wad` overlay that overrides
assets **by hash**.

## What it is

A single CLI binary (`smuggler`). It opens a source `vz.wad` (FFCS archive), pulls out the block(s)
you target, rebuilds their contents, sges-compresses the result and emits a small patch WAD
containing only the overridden/added blocks plus their ASET entries. The game loads that patch WAD
after the base WAD, and the by-hash override is **last-opened-wins** — no block injection into
`vz.wad` itself.

Four things it can build, composable in one invocation:

* **Model override** — `--inject-container <file.ucfx>` replaces the model container in a donor
  block with a pre-built UCFX container (e.g. the output of `tools/gltf_to_ucfx_model.py`).
* **New asset** — `--inject-extra "0xHASH:TYPEID:file"` (repeatable) mints an extra single-asset
  override block from a raw UCFX container. Supported `TYPEID`s are the ones it can map to a UCFX
  type hash: `19` = model (`0x5B724250`), `27` = texture (`0xF011157A`), `35` = script
  (`0x42498680`). Pair with `--extra-only` to build *only* these and touch no donor block.
* **Raw block override** — `--inject-block "<path_substr>:<file>"` (repeatable) looks a block up by
  path substring, carries its existing ASET entries and path forward, compresses your raw
  decompressed block file and overlays it. Used for content-additive overrides (augmented
  `layers_static` placements, edited resident-script blocks).
* **Cube-ize** — the original proof-of-concept mode, still the default when no
  `--inject-container` is given: reshapes the donor model's vertices into a cube
  (`--shape corner|clamp`). Kept for plumbing bisection — it proves the WAD/ASET/sges path works
  independently of any new geometry. `--no-cubeize` is the identity passthrough.

It also reads: `--list` (list model-bearing blocks matching `--target-name`),
`--dump-container` (write the donor model's raw UCFX bytes out, the input a mesh converter needs as
its structural donor) and `--dump-block` (write the whole raw decompressed block out).

## Where it comes from

Everything is derived from the retail PC WAD format work in `mercs2_formats`: the FFCS archive +
INDX block index, the sges block codec, the UCFX container entry table (`[u32 count][16-byte
entries][containers]`), and the ASET asset table. `mercs2_smuggler` is only the *composition* layer
on top of those parsers.

The by-hash override strategy exists specifically to sidestep the unsolved ASET name-hash problem
(modding deep-dive Open Q#6) — you never need to know an asset's *name*, only its hash.

The block-resolution rules encode hard-won facts about how the engine assembles a model, documented
in `docs/modernization/vehicle_model_spec.md` §1 and used end-to-end in
`docs/asset_injection_playbook.md`:

* By default the tool resolves a model's **ASET-primary block** rather than the block you named, so
  the HIER/structure the engine actually instantiates is the one you edit.
* A vehicle's geometry is spread across an LOD chain (`_P000_Q3` resident → `_P001_` → `_P002_`),
  and finer rungs bounce back to the resident under the default rule. `--exact-block` honours
  `--block-index` verbatim so those rungs (a tank's animated tracks, its full-res materials) can be
  reached.
* Some rungs are **sub-entry** models with no model ASET row at all (the ztz98's `_P003_Q0` rung and
  its separate `resident2-..._tracks_*` chain; the Mi-26). `--dump-container` cannot reach them —
  that is what `--dump-block` + `--inject-block` are for.

## Usage

List the candidate donor blocks, dump a donor container, then override it:

```bash
# what can I target?
smuggler --source-wad vz.wad --target-name deliverycrate,crateaid --list

# pull the donor model container out (structural donor for the mesh converter)
smuggler --source-wad vz.wad --block-index 3565 --dump-container donor.ucfx

# override that model with a converted container -> a part WAD
smuggler --source-wad vz.wad --block-index 3565 \
  --inject-container custom.ucfx --output output/parts/tank.wad
```

A full vehicle swap — an exact LOD rung, a bytecode-redirect script block and a skin texture, in one
patch WAD (from `docs/asset_injection_playbook.md` §5.1):

```bash
smuggler --source-wad vz.wad --exact-block --block-index 3565 \
  --inject-container ct.ucfx \
  --inject-block "scripts_vz:scripts_vz_ztz.bin" \
  --inject-extra "0x21A2AFD1:27:skin.bin" \
  --output output/parts/ztz98.wad
```

A from-scratch asset that overrides nothing existing:

```bash
smuggler --source-wad vz.wad --extra-only \
  --inject-extra "0xDEADBEEF:19:model.bin" --output output/parts/new_model.wad
```

Defaults worth knowing: `--target-name` is `deliverycrate,crateaid`, `--shape` is `corner`.
`--output` is required unless you are dumping.

## Notes / gotchas

* Every `--inject-container` / `--inject-extra` input must be a real UCFX container — the tool
  checks for the `UCFX` magic and a ≥20-byte length and refuses otherwise.
* Cube-ize is strict: it fails if it changed the container length, or if the donor model has no
  vertex meshes.
* Overriding is **per model hash** — duplicate targets that resolve to the same model hash are
  emitted once.
* The patch WAD carries the source WAD's `CSUM` chunk value/meta and a canned `FFCS_CERT_BLOB`;
  that plumbing lives in `mercs2_formats::patch_wad`.
* `--inject-block` refuses a block that has no ASET entries — the overlay would be unreachable.
* A patch WAD's **block set** is the contract, not its block count: a vehicle swap is never just the
  model (the spawn-redirect script block and the model are two independent halves of one feature).
  Diff the block *paths* after a rebuild.
* Per the project workflow, build each asset as its own part WAD and merge with `wad_builder
  merge-blocks`; validate the combined WAD with `wad_simulator` and verify by sha256 before
  deploying.
