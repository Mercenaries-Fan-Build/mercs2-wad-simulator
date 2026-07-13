# mercs2_workshop

Native asset workshop for the Mercenaries 2 modernization — browse, inspect and remix the game's
assets on the `mercs2_engine` renderer, without booting the game.

## What it is

A binary crate (`cargo run -p mercs2_workshop`) that opens a winit window driving the engine's
`Scene` — the SAME renderer `mercs2_game` boots, so materials, skinning, lighting and shadows are
the faithful path rather than an export approximation. The window is organized as workbenches; the
one implemented today is the **asset workbench**:

- browse / search every model and texture in the open WAD, with names resolved from the registry
  dump (`index::AssetIndex`)
- preview a model with its real materials, textures, orbit camera and animation clips
- inspect layers: the HIER bone tree, per-material draw groups (isolate / hide individual
  sub-strips), full-screen texture plates
- an editable sandbox: place instances, move / rotate / scale them, save and reload the arrangement
  to `workshop_scene.json`
- read-only Lua source viewer with a hand-rolled highlighter, opened from corpus hits
- import a foreign mesh (`.obj` / `.gltf` / `.glb`) and preview it on the engine renderer
- publish a NOVEL (new-hash) model into a patch WAD

Every GUI action also has a headless flag, so the same code paths are scriptable from the command
line (catalog dumps, model load checks, texture PNG dumps, lossless export bundles, mod publish).

The GUI is egui painted through the engine's `Scene` overlay hook (`render_with` /
`render_menu_with`); the winit-0.29 → egui event bridge is hand-rolled in `gui.rs` because
`egui-winit` 0.28 targets winit 0.30 while the engine is still on 0.29.

## Where it comes from

The workshop is a consumer of the reverse-engineering corpora rather than a reimplementation of one
exe subsystem. What its source explicitly claims as provenance:

- **Overlay / patch stack** — `--overlay` and the auto-loaded `vz-patch.wad` sitting next to the
  base WAD reproduce the retail exe's own patch-WAD lookup (last-opened wins). That is how
  DLC-ported content reaches the workbench.
- **Names** — `docs/data/live_registry_hashes.csv`, ~82k names captured from the running game's
  name-hash table (memory note `name-registry-spawn-by-hash`). A curated ASET dictionary
  (`data/aset_names.tsv`, 18k rows generated from `docs/data/aset_names.csv`) is compiled into the
  binary so it resolves names with no external files.
- **Destruction state machines** — `--states` dumps the engine's own format, per
  `docs/destruction_orchestrator_format.md`, via `mercs2_formats::orchestrator`.
- **Model injection** — publishing builds containers with
  `model_inject::inject_into_donor_block`, the CJ donor recipe (take a real container the engine
  already accepts, rebuild the geometry, re-stamp the name, recompute CSUM). Never a from-scratch
  UCFX — that is the "sarah-hang". The single-entry-block + ASET-row shape is the cube_mod-proven
  one; no retail-block surgery. Milestone M3 of
  `docs/modernization/workshop_publish_pipeline.md`.
- **Roadmap** — `docs/modernization/workshop_charter.md` (mission design, model import/fix, AV
  replacement, unlock auditing are the planned workbenches).

## Usage

Open the window against the registry-discovered `vz.wad`:

```
cargo run -p mercs2_workshop
```

Headless flags (each runs and exits). `<name|0xHASH>` accepts either a raw m2 hash or an asset name
that gets hashed with `pandemic_hash_m2`:

```
# catalog dump, optionally substring-filtered
cargo run -p mercs2_workshop -- --list heli

# models grouped by vehicle class (helicopter, tank, car, boat, ...); optional class filter
cargo run -p mercs2_workshop -- --inventory helicopter

# load one model end-to-end (geometry + textures + clips + bones + LOD tiers) and print its stats
cargo run -p mercs2_workshop -- --check civ_hum_beachfemale_a

# dump a model's destruction state machine with names resolved
cargo run -p mercs2_workshop -- --states 0xA3C1FABC

# decode a texture to PNG (full mip chain, via extract_texture_hires)
cargo run -p mercs2_workshop -- --tex-png some_texture_dm out.png

# OBJ + MTL + PNG export -> workshop_export/<name>/
cargo run -p mercs2_workshop -- --export mattias_v3

# LOSSLESS bundle: glTF + PNG skins + every LOD rung's ORIGINAL container bytes + manifest
cargo run -p mercs2_workshop -- --export-bundle class:helicopter --out workshop_export

# publish a novel new-hash model into a patch WAD (self-tests by reopening + engine-loading it)
cargo run -p mercs2_workshop -- --mod-new my_tank tank_donor_name mesh.glb --mod-out vz-mod.wad
```

Global options: `--wad <path>` (override registry discovery), `--overlay <path>` (repeatable,
stacks patch WADs on top of the base), `--no-auto-patch` (suppress the automatic `vz-patch.wad`
pickup), `--names <csv>` (override the name corpus).

Other headless flags: `--pack-data [dir]` (assemble the redistributable `workshop_data/` reference
bundle), `--hash <names...>` and `--hash-file <file>` (m2-hash names for hash-hunting),
`--block-strings <block>` (printable ASCII from a decompressed block), `--tex-check`, `--tex-scan`,
`--tex-scan-blocks`, `--tex-png-block <block> <0xHASH> <out.png>`, `--import-check <file>`.

## Modules

Binary crate — these are private modules, listed for orientation:

- `app` — the workshop window: winit loop over the engine `Scene`, the asset workbench, the
  editable sandbox, the OBJ/PNG exporter.
- `bundle` — the lossless export bundle (`raw/*.ucfx` verbatim rungs + `model.gltf` + `textures/`
  + `manifest.json` reassembly map).
- `gui` — egui host: hand-rolled winit-0.29 → egui bridge + the egui-wgpu paint path.
- `import` — foreign model import (`.obj` / `.gltf` / `.glb`) into the engine's in-memory
  `ModelData`.
- `index` — asset catalog + the name-resolution stack (`workshop_data/names.bin`, the embedded
  ASET dictionary, the repo corpora fallback).
- `luaview` — read-only Lua source viewer with a hand-rolled Lua lexer/highlighter.
- `publish` — background-threaded mod publishing into a patch WAD, with SHA-256 + a load self-test.
- `texenc` — CPU BC1/BC3 encode (min/max endpoint, no search) for imported images.
- `texpng` — CPU BC1/BC3 decode + PNG write for the headless texture dumps.

## Notes / gotchas

- **Nothing is discarded on export.** A vehicle container holds chunks we have not fully reversed
  (Havok `PHY2` hulls, the `CHDR`/`CEXE` destruction scripts, `SWIT`/`STAT`/`NODE`, `MTRL` shader
  params). The bundle therefore keeps every LOD rung's ORIGINAL bytes in `raw/*.ucfx` alongside the
  editable glTF, so a bundle can always be reassembled into a working asset.
- **glTF geometry is node-local.** `build_indexed_all` hands back world-space verts; the bundle
  divides by each node's world-rest so the rig is real (moving the rotor node in Blender moves the
  rotor).
- **Convention boundary is load-bearing.** The engine is row-major / row-vector
  (`world = local · world_parent`); glTF is column-major / column-vector
  (`world = world_parent · local`). A wrongly-transposed matrix still *looks* like a plausible
  skeleton, so `tests/anim_export_parity.rs` recomputes each bone's model-space position both ways
  and asserts they agree. It needs the retail WAD, so it is `#[ignore]`d — run with
  `cargo test -p mercs2_workshop -- --ignored`.
- **Loading a model always walks the LOD-block chain.** `build_indexed_from_container` reads only
  the resident block, and for any model whose near geometry lives in a finer rung it finds nothing
  and reports a false "no placed drawing groups". `civ_hum_beachfemale_a` is the case in point: its
  resident container holds only mask-0x08 (far) geometry. Use `app::load_model_data` (what
  `--check`, the preview and the exporters all use).
- **Skinning is not imported.** Foreign meshes preview as static geometry; imported textures get
  BC-compressed on the fly under synthetic `m2(<file>#<n>)` hashes.
- **`--tex-png-block` prints decoded dims, not declared dims.** One block holds one mip level, so
  the surface is usually coarser than the texture's full size.
- **Name loading has two speeds.** `workshop_data/names.bin` (packed, sub-second) is preferred; the
  raw repo-corpora fallback takes ~8 s. `--pack-data` builds the packed form.
