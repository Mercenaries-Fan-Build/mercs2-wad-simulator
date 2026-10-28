# mercs2_workshop

Native asset workshop for the Mercenaries 2 modernization — browse, inspect and remix the game's
assets on the `mercs2_engine` renderer, without booting the game.

## What it is

A binary crate (`cargo run -p mercs2_workshop`) that opens a winit window driving the engine's
`Scene` — the SAME renderer `mercs2_game` boots, so materials, skinning, lighting and shadows are
the faithful path rather than an export approximation. The window is a rail of pages — **Inspect ·
Saves · Driving · Missions · Systems · Shipment · Settings** — with **Live** and **Log** pinned at
its foot:

- **Inspect** browses / searches every model, texture, layer and audio clip in the open WAD, with
  names resolved from the reference bundle's name pack (`index::AssetIndex`). The header reads
  `INSPECTING  Tab › Lens › model`. On the Models tab a Lens row narrows the list to World,
  Characters or Weapons and groups it by faction: a spawn template's row by its own `FactionMarker`
  (so a defector sits with the faction it serves), any other row by its name's prefix; each marker
  is labelled with the prefix it pairs with across the world's templates. The inspector's Lens card
  starts a contribution of that domain's kinds.
- **Audio** (Inspect's AUDIO tab, `audiopage`) lists the 12,988 voice-over lines by the banks whose
  cues play them, read from `English.wad` (`audioroute`, through `sound::wavebank_players`): the 27
  labelled `vo_*` banks, 19 per-scene banks known by hash, and the 34 lines no cue plays. Search
  every clip by bank, speaker, caption or file name; select with the checkboxes or Ctrl/Cmd+A; ▶ or
  Space plays the current line from `vo_stream.english.pws`, again pauses and resumes it, with its
  position in the now-playing card. **Add to Shipment** offers *Replace this line*
  (`replace_sound_wave`), *Replace cue …* per cue that plays it (`replace_sound_cue`, seeded with the
  retail cue's values, by name or guid) and *Replace its bank*; the EXPORT card (F10) writes the
  selection, the bank or the search hits as 16-bit WAVs plus `captions.csv`.
- **Anims** plays a clip on the **hero**, the one model you choose for clips (kept in
  `settings.json`, shared with FaceFX). With no hero the picker opens, ranking every model by how
  many of the clip's bones its skeleton holds (`heroidx`, read on a worker). The clip card gives
  the codec (wavelet, interleaved and delta-compressed all decode), length, tracks and the bones
  bound on the hero. The 29 MANM material animations open a card of their tracks instead: each
  property's value at the playhead and its curve, sampled as the game samples them (`manm`).
- **Export…** (the model menu, F10, the verb bar) opens a dialog: a name, the format — the lossless
  bundle or OBJ + MTL + PNG (an imported mesh has no LOD rungs to bundle) — and the folder
  (`workshop_export/` unless chosen); it writes `<folder>/<name>/`.
- **Start a Shipment from this…** scaffolds a new Shipment in a picked folder holding the asset's
  first route — an outfit for a character, a new model for any other model, a texture replacement for
  a texture — and opens it on the Shipment page.
- preview a model with its real materials, textures, orbit camera and animation clips
- inspect layers: the HIER bone tree, per-material draw groups (isolate / hide individual
  sub-strips), full-screen texture plates
- right-click a model to load or export it, or to use it as a donor: **Retarget a rig onto this**
  and **Fit an import to this** open their bench with the model as donor, lit under Shipment with
  a `SHIPMENTS › shipment › contribution › bench` breadcrumb. The work happens in the bench and
  nothing is written until it saves; the donor is never written.
- **Live** is a blank page; its rail dot is lit while the game's lua-bridge (`127.0.0.1:27050`)
  accepts a connection.
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
- **The game's other WADs** — `shell.wad`, `Loading.wad` and each language's `<token>.wad` (the
  engine's language table, `mercs2_quartermaster::manifest::Language`) are opened read-only beside
  the stack. The game loads each in its own slot, so they never shadow it: Inspect lists what only
  they ship (its WAD named after the row) and, for an asset several WADs hold, every copy.
- **Names** — the reference bundle's `names.bin`, which `--pack-data` builds from this crate's
  `data/production_names.json`. An ASET dictionary (`data/aset_names.tsv`, written by
  `aset_export --embed-tsv` from the rainbow table and this crate's name fragments) is compiled into
  the binary and overrides the pack. Every name file lives in `data/` (see `data/README.md`).
- **Destruction state machines** — `--states` dumps the engine's own format, per
  `docs/destruction_orchestrator_format.md`, via `mercs2_formats::orchestrator`.
- **Model injection** — publishing builds containers with
  `model_inject::inject_into_donor_block`, the CJ donor recipe (take a real container the engine
  already accepts, rebuild the geometry, re-stamp the name, recompute CSUM). Never a from-scratch
  UCFX — that is the "sarah-hang". Each asset ships in a single-entry block with its own ASET row,
  the shape `mercs2_smuggler` builds; no retail-block surgery. Milestone M3 of
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

# voice-over clips as the Audio page exports them: a bank, or every clip a search finds
cargo run -p mercs2_workshop -- --audio-export vo_Jen out_dir
cargo run -p mercs2_workshop -- --audio-export "the cia" out_dir

# publish a novel new-hash model into a patch WAD (self-tests by reopening + engine-loading it)
cargo run -p mercs2_workshop -- --mod-new my_tank tank_donor_name mesh.glb --mod-out vz-mod.wad
```

Global options: `--wad <path>` (the first step of the game-archive chain, below), `--overlay <path>`
(repeatable, stacks patch WADs on top of the base), `--no-auto-patch` (suppress the automatic
`vz-patch.wad` pickup).

## Where the game and the reference data come from

Two chains, first match wins (`chains.rs`):

- **Game archive**: `--wad` → this run's choice (Settings) → the saved setting → an install the
  workshop sits inside → `MERCS2_GAME_DIR` / `VZ_WAD` → the Windows registry key.
- **Reference data**: this run's choice (Settings) → `MERCS2_WORKSHOP_DATA` → the saved setting →
  `workshop_data/` next to the exe → `workshop_data/` in the working directory or a parent → the
  downloaded copy.

A source someone set that is broken stops its chain; it is never passed over for the next one. The
window always opens first. When a chain has no answer it opens on Settings with that chain in red
and every other page locked. Settings shows each chain step by step, with **Choose…** (saved),
**Use for this run only…** (not saved), **Clear saved**, **Reveal folder**, and for reference data
with nothing set, **Download**. A change there reloads the archive or the bundle in place, with no
relaunch. A headless mode with the game chain unresolved exits 2, printing every step.

Capture a page in a given state (a diagnostic for comparing a page with its design artboard):

```
cargo run -p mercs2_workshop -- --ui-state crates/mercs2_workshop/ui_states/inspect.json
```

The JSON names the state by the words the window shows — `page` (a rail label or `Live`),
`settings_section`, `tab`, `lens`, `filter`, `load` (a model name, or on another tab that tab's row by its label), `hero` (a model name, set before `load`), `shipment` + `contribution`,
`bench` + `donor`, and on the AUDIO tab `bank`, `clip`, `search` and `select` (record indices),
applied once the tab's routing is read (`ui_states/audio.json`), `run_wad` / `run_data` (a path
used for this run as Settings' "Use for this run only…" does, after the first load; the capture
waits for the reload it starts, and a path that starts none exits 2) — plus `capture` (the PNG to write,
relative to the working directory) and `settle_frames` (default 8). The window opens, applies the state once the reference data is in
(or at once when it opens on Settings for a chain with no answer: `ui_states/settings_unset.json`),
captures the presented frame (3D and GUI) and exits 0. A name that does not resolve, a field it does
not know, or a surface that cannot be read back exits 2 with the reason.

Other headless flags: `--pack-data [dir]` (assemble the redistributable `workshop_data/` reference
bundle; see [Reference bundle](#reference-bundle)), `--hash <names...>` and `--hash-file <file>` (m2-hash names for hash-hunting),
`--block-strings <block>` (printable ASCII from a decompressed block), `--tex-check`, `--tex-scan`,
`--tex-scan-blocks`, `--tex-png-block <block> <0xHASH> <out.png>`, `--import-check <file>`.

## Reference bundle

Everything the workshop consults that is not a game file — `names.bin`, the registry rows, the
spawnable templates, the ECS schemas, the decompiled-Lua corpora and the captioned audio inventory
(`audio_manifest.json`) — comes from one
`workshop_data/` folder. A folder counts only when it holds the `manifest.json` that `--pack-data` writes last; a
repo checkout's committed, partial `workshop_data/` (no `names.bin`, no manifest) never does. The
folder is found in this order (`index::data_home`, over `chains::data`):

1. this run's choice (Settings, "Use for this run only…")
2. `MERCS2_WORKSHOP_DATA`
3. the Settings data folder (`data_dir` in the settings file)
4. `workshop_data/` next to the executable
5. `workshop_data/` in the working directory or any parent
6. the copy downloaded into the user's cache directory (`fetch::installed_dir`)

Steps 1–3 are set by a person: one that is not a complete bundle is an error naming it, never passed
over for a later step. When nothing is set and nothing is found, the workshop downloads the pinned
`mercs2-workshop-data.zip` and installs it as step 6 (`index::require_data_home`); the Settings
page's Download button runs the same install. The
pin is `fetch::PINNED`, read at compile time from the `BUNDLE_URL=` and `BUNDLE_SHA256=` lines of
`data/workshop_data_pin.env`. `.github/workflows/ci.yml` sources the same file to download the
bundle for the corpus gates, so moving the pin is an edit to those two lines alone. A pin file
without exactly one of each line, a URL other than
`https://github.com/Mercenaries-Fan-Build/mercs2-wad-simulator/releases/download/<tag>/mercs2-workshop-data.zip`,
or a digest other than 64 lowercase hex digits stops the build. The install downloads that URL and
nothing else, and checks the archive's sha256 against `BUNDLE_SHA256` before unpacking; a mismatch
stops the install with both digests, unpacks nothing, and leaves the installed copy as it was. A
staging folder or installed copy the install cannot remove stops it with an error naming the folder
and the removal. The installed copy is renamed aside to `workshop_data.previous` before the new one
is moved in, and removed only after: when the move fails, the previous copy is renamed back and the
error says it was kept, or names both failures and where the previous copy is. When
the download fails, the run stops with an error giving the URL, its sha256 and the folder to unpack
the zip into. A complete bundle missing `audio_manifest.json`, or holding a malformed one, stops the
boot with an error naming the file.

The corpus gates in `lua_source`, `blueprint` and `missions` run over the bundle's Lua and are
pinned to release v0.17.0's bundle (`index::PINNED_CORPUS`: 675 scripts in `lua/`, 39 in
`lua_dlc/`), the one CI downloads and `fetch::PINNED` names. A different bundle fails them on the
count: re-download the bundle from Settings, or point `MERCS2_WORKSHOP_DATA` at an unpacked v0.17.0
`workshop_data/`.

`--pack-data [dir]` builds the bundle (default `workshop_data`). Its in-repo sources are read from
the checkout the binary was built from: names come from this crate's
`data/production_names.json`, and `audio_manifest.json` is copied from the repo's committed
`workshop_data/`. Packing into that same `workshop_data/` leaves it in place. The rest is copied from the notes
repo's `docs/` folder named by `notes_docs_dir` in `.mercs2-local.toml`. With no
`production_names.json`, the names are merged from the raw corpora: this crate's name fragments in
`data/`, and the `rainbow_table` and `jul08_strings` keys. A missing key or source file stops the
pack with an error naming it.

## Modules

Binary crate — these are private modules, listed for orientation:

- `app` — the workshop window: winit loop over the engine `Scene`, the rail and its pages, the
  Retarget / Fit benches, the OBJ/PNG exporter.
- `bundle` — the lossless export bundle (`raw/*.ucfx` verbatim rungs + `model.gltf` + `textures/`
  + `manifest.json` reassembly map).
- `gui` — egui host: hand-rolled winit-0.29 → egui bridge + the egui-wgpu paint path.
- `import` — foreign model import (`.obj` / `.gltf` / `.glb`) into the engine's in-memory
  `ModelData`.
- `index` — asset catalog, reference-bundle resolution, and name resolution
  (`workshop_data/names.bin` overlaid by the embedded ASET dictionary).
- `fetch` — downloads and installs the release's reference bundle.
- `chains` — where the game archive and the reference bundle come from: each chain's steps, first
  match wins, a broken set source stopping it; the rows Settings draws and headless runs print.
- `settingspage` — Settings' navigator (with each chain's badge) and its Game archive / Reference
  data sections: the chain rows, the "saved path not found" banner, what it opened, the buttons.
- `live` — the Live page and the probe behind its rail dot (`mercs2_bridge::reachable`, polled off
  the frame loop).
- `textpage` — Inspect's Text tab: every string table of every WAD side by side by key (read on a
  worker, keys named from `names.bin` and the Lua corpus), and a font's pages with every glyph's box
  (`mercs2_formats::font`).
- `saves` — the Saves page: save browser, hero viewport, PROGRESSION grid with the contract menu,
  inspector, Write.
- `save_tables` / `save_inspect` — the readable views of every save table, and the Inspect window.
- `game_rules` — the shipped mission, reward and flow scripts, run from the WAD stack on the game's
  Lua VM; every other module is a recorder, so a flow binding's consequence comes back as the calls
  the game would make.
- `save_flow` — contract state changes on a save (complete, undo, offer, start, lock), as the game
  makes them. What it writes where: `docs/save_contract_state.md`.
- `missions` — the Missions page: a Blueprint editor over a mission script's Lua (script list,
  palette from the corpus, zoomable canvas, inspector, Compile, Add to Shipment as `replace_lua`).
  A function's statements chain left to right; a chain wider than the canvas at 1:1 wraps into rows
  sized to the canvas's shape, each row's blocks hanging under it before the next row starts.
- `blueprint` — a Lua function body as nodes and wires: lifted from the AST, edited, and emitted back
  through it, plus the corpus index (signatures, inherit chains) and the palette. Its gates lift and
  emit every corpus script byte-identical and compile each with the game's Lua 5.1.
- `lua_source` — the lossless Lua model the Blueprint stands on (full_moon); its gate parses and
  prints every corpus script byte-identical.
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
  and asserts they agree. It needs the retail WAD: game-gated, built only by the `retail` feature, it
  reads the `vz.wad` named by the repo-root `.mercs2-local.toml` (`scripts/find-vz-wad.sh --write`)
  and fails when it is missing — `cargo xtask retail-test`.
- **Loading a model always walks the LOD-block chain.** `build_indexed_from_container` reads only
  the resident block, and for any model whose near geometry lives in a finer rung it finds nothing
  and reports a false "no placed drawing groups". `civ_hum_beachfemale_a` is the case in point: its
  resident container holds only mask-0x08 (far) geometry. Use `app::load_model_data` (what
  `--check`, the preview and the exporters all use).
- **Skinning is not imported.** Foreign meshes preview as static geometry; imported textures get
  BC-compressed on the fly under synthetic `m2(<file>#<n>)` hashes.
- **`--tex-png-block` prints decoded dims, not declared dims.** One block holds one mip level, so
  the surface is usually coarser than the texture's full size.
- **Names load from the pack only.** `workshop_data/names.bin` loads in under a second; a missing or
  unreadable pack is an error.
