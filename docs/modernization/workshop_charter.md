# Workshop charter — the engine's developer tool platform

**Crate:** [`tools/wad_simulator/crates/mercs2_workshop`](../../crates/mercs2_workshop) · **Run:** `cargo run -p mercs2_workshop`
**Status:** v1 (asset workbench) BUILT — 2026-07-04

The workshop is the first proper developer tool for the modernization engine: a native window on
the **same `mercs2_engine` renderer the game boots** (not three.js, not an export approximation).
Everything it shows is the faithful pipeline — UCFX mesh build, MTRL multi-material sub-strips,
BC textures with streamed hi-mips, LBS skinning, wavelet clips, the forward+shadow+HDR passes.
Its purpose is to let users experiment with aspects of the game, and with novel features, in
isolation — no game boot, no WAD rebuild round-trip.

## Architecture

- One winit loop over `mercs2_engine::scene::Scene` + a `mercs2_core::World` (hecs). The app is
  the *game-layer*: the engine stays asset-agnostic (pangea_engine_alignment §6).
- `index.rs` — the asset catalog: WAD ASET table × name resolution (rainbow table 733k preimages,
  overlaid by the 82k live-registry dump; ~82% of vz.wad ASETs resolve to names).
- `app.rs` — the workbench itself. A rail switches pages: Inspect · Saves · Driving · Missions ·
  Systems · Shipment · Settings, with Live (dot = the game's lua-bridge answers) and Log at its
  foot. World, Characters and Weapons are Lenses on Inspect's Models tab, not pages; the Retarget
  and Fit benches open from a model's "use as a donor" menu or a Shipment contribution, light
  Shipment, and write nothing until they save. UI is a mouse-driven INSPECTOR (egui, `gui.rs`): left browser (search + tabs), right
  Unreal-style Details panel (Info / States-LODs / Materials with per-group visibility /
  Animation with a scrubbable timeline / Skeleton), toolbar for import-export ops. egui paints through the engine's
  `Scene::render_with` overlay hook — the engine stays GUI-agnostic, and the game's authentic
  `ui_rect`/`ui_text` pass (which the boot screen still uses) is untouched. Every mutation is an
  `Act` queued by widgets and keyboard shortcuts alike, executed once per frame.
- Headless modes for scripting/CI: `--list [filter]` (catalog dump), `--check <name|0xHASH>`
  (end-to-end load: geometry, textures, clips, bbox).

## v1 — ASSET workbench (built)

- Browse/search all models + textures (typed filter, registry names, hash fallback).
- Preview with real materials/skinning; orbit/pan/zoom camera (absolute-delta mouse — works with
  Shadow streaming input); animation clip cycling (F4) via the animgroup→HIER binding path.
- Inspect layers: HIER bone tree (F1, names resolved), per-material draw groups (F2) with
  isolate/hide per sub-strip (`set_draw_hidden`), full-screen texture plates (F3 / Enter on a
  texture row) at streamed full resolution.

## Roadmap — future workbenches (user-set vision, 2026-07-04)

Each mounts into the same loop as a mode; the asset workbench is the pattern.

1. **Mission designer — the Lua BLUEPRINT editor** (user-set direction, 2026-07-05): a
   node-graph ("blueprint") view over the contracting system's Lua, both for understanding
   existing contracts and authoring new ones. Pipeline, every stage chosen so it can be
   validated against the 370-script corpus before any editing is trusted:
   - **Parse**: `full_moon` (pure-Rust, Lua 5.1 — matches the LuaQ corpus), full-fidelity AST
     that round-trips source byte-identically. Quality gate: parse→re-emit the WHOLE corpus and
     diff — the parser earns trust mechanically.
   - **Lift**: an idiom recognizer over the AST for the contract vocabulary (`import`,
     `OnActivate`, `Event.Create(Event.X, …)`, `Objective.*`, `Pg.Spawn`, `Airstrike.*`,
     `MrxUtil.*`) → Blueprint IR: event/condition/action nodes, control+data edges. Unrecognized
     Lua stays as opaque "script nodes" (never lossy).
   - **Edit**: egui node-graph (egui-snarl) inside the workshop, alongside the 3D viewport so
     spawns/objectives can be placed spatially.
   - **Emit**: IR → Lua source (via the same AST library) → LuaQ compile → ship through the
     PROVEN patch path (`wad_builder build-skin` / vz-patch overlay — the wardrobe scripts
     already went through it).
   - Reference data for pickers (templates, support types, factions) comes from the bundled
     `workshop_data/` (see "Reference bundle" below).
2. **Model import/fix** — bring foreign models in (CJ/GTA-SA pipeline), diagnose and repair:
   vertex-decl translation, BLENDINDICES palette mapping, weight transfer, MTRL binding — with
   the oracle renderer right there to verify.
   **v1 SHIPPED (2026-07-05):** drag-drop `.obj`/`.gltf`/`.glb` → engine-rendered preview
   (glTF base-color textures BC-compressed on the fly, `texenc.rs`); **F10** exports
   any preview (game asset / import) to `workshop_export/<label>/` OBJ+MTL+PNGs —
   round-trip verified geometry-exact on mattias_v3. Headless: `--import-check`, `--export`.
   Remaining for "ship in-game": native UCFX-container export (today the OBJ hand-off goes
   through `gltf_to_ucfx_model.py` + `cube_mod`), skinning import/weight transfer.
3. **AV / cutscenes** — list Bink movies + audio streams; convert/replace specific videos or
   audio (PS3 movies drop in; `.bik` via RAD tools; pws audio tooling exists in Rust).
4. **Unlock / gating auditor** — identify act- or DLC-gated content (AddSupportData DLC gates,
   costume/tier unlocks, spawnable templates) and toggle them in a controlled way.
5. **Debug interaction environment** — spawn actors with the human-animation SELECTION chain
   (ActionTable→AnimationLookup→ASTO), drive states, watch real interactions in isolation —
   the controlled debug environment for behaviour work.

## UI direction (user-set)

Lean on the game's own art for the tool's look: the shell.wad loading plate already backs the
browse screen and the texture viewer, and the panel palette follows the shell's gold-on-dark.
Next steps on this axis: pull HUD/menu textures out of shell.wad / Loading.wad (the `Gui.LoadTexture`
set + scaleformgfx containers) for panel chrome, and extract the game's fonts to replace
the system-TTF glyph atlas in `ui.rs` — both slot into the existing `UiPass` without call-site
changes.

**Loading.wad inventory (2026-07-04, via `--tex-png`):** 4 textures — `global_loading_skull`
(0x443F889B, 64x64 gold logo, the loading icon), 0x94BD9390 (256x128 warm green→gold gradient,
the animated loading wash), and **0x30705573 (512x256) + 0x44142C14 (512x512) = the game's OWN
UI FONT atlases** (Larabie typeface, white glyphs in ALPHA; extended-charset pages — the base
ASCII page presumably lives in shell.wad). Those atlases are the direct path to the authentic
`ui.rs` glyph source. The boot loading screen (loading.wgsl mode 1, `Scene::render_boot`)
already uses the skull + the gradient's palette: black background, pulsing gold skull, gold→green
sheen, arc spinner, gold progress bar.

## Reference bundle (`workshop_data/`) — user-set mandate, 2026-07-05

Everything the workshop consults that is NOT a game-distributed file ships WITH the workshop, in
formats it can query directly — the tool must run self-contained and its insight features must
not depend on a repo checkout. `mercs2_workshop --pack-data <dir>` builds the bundle; resolution
at runtime is `MERCS2_WORKSHOP_DATA` env → `workshop_data/` next to the exe → CWD walk-up
(`index::data_home`), with the raw repo corpora as the dev-mode fallback.

| Content | Format | Feeds |
|---|---|---|
| `names.bin` | binary hash→name pack (939k merged: bones + rainbow + registry; 0.7 s load vs ~8 s raw JSON) | every label everywhere |
| `live_registry_hashes.csv` | full registry rows (template handles, is_vehicle) | Identity panel, spawn tooling |
| `spawnable_templates.csv` | template dump | blueprint pickers |
| `ecs_schemas/` | 220-class COMP schema docs | COMP inspector |
| `lua/`, `lua_dlc/` | decompiled Lua corpora (370 + 39 scripts) | Lua references, blueprint lifter + its round-trip validation |
| `manifest.json` | provenance + counts | diagnostics |

## Conventions

- New inspect/EDIT features go in the workshop, not as `mercs2_game` flags (memory:
  `no-debug-probes-in-game-exe` applies to the retail exe; the workshop is the sanctioned home
  for interactive dev tooling on the reimpl side).
- The workshop must keep working from a plain `cargo run -p mercs2_workshop` with only the
  registry-discovered install; extra data (names, rainbow table) degrade gracefully to hashes.
