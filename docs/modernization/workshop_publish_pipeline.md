# Workshop publish pipeline — adding & repackaging novel content

Status: M3 SPINE BUILT + VALIDATED (2026-07-05) — user chose novel new-hash assets first.
`mercs2_workshop/src/publish.rs` + the GUI "Mod project" section + headless `--mod-new`.
Proven e2e: `--mod-new my_custom_helipad 0xE6A22510 <obj>` → 0x8B7DE1F5 published (sha256
printed), self-test engine-loads the new hash from the written wad, `--list`/`--check` see it
via overlay (donor's textures/HIER/state machine intact). Charter workbench: "Import and fix
models" + "Mission designer" (docs/modernization/workshop_charter.md). Remaining below: M1
texture path (texture_build.rs), M2 GUI donor recipe depth, in-game reference wiring (Lua
spawn), project persistence.

## Goal

The workshop can already **read** everything (browse/preview/inspect across base + overlay WADs)
and **import** foreign content into the previewer (OBJ/glTF, BC-encoded textures). The missing
half is **write**: turning session edits into a `vz-patch.wad` the retail engine (and our own
engine) consumes — replacing existing assets and adding genuinely new ones, natively, no Python
side-trips.

## What already exists (all Rust, all proven in-game)

| Piece | Where | State |
|---|---|---|
| Patch WAD serializer (INDX/DATA/CSUM/ASET/PTHS, cert blob, paging) | `mercs2_formats::patch_wad::build_patch_wad_multi` | ✅ canonical, used by dlc_port + cube_mod |
| sges block compression | `mercs2_formats::sges::compress_sges` | ✅ |
| Single-asset override block format `[count=1][16B entry][UCFX]` + ASET row | `cube_mod::build_block` / `build_extra` | ✅ proven in-game (cube mod, CJ, heli) |
| Donor-based mesh injection (STRM/IBUF/PRMT/decl rebuild, MTRL repoint, CSUM) | `mercs2_formats::model_inject` (DECL64 w/ tangent) | ✅ proven (CJ recipe) |
| In-place geometry rewrite | `mercs2_formats::model_cubeize` | ✅ |
| OBJ/glTF import → engine preview | workshop `import.rs` | ✅ |
| BC1/BC3 encoder | workshop `texenc.rs` | ✅ (naive min/max, single level) |
| Overlay consumption (last-wins by hash) | workshop `WadStack` | ✅ — the self-test loop |
| ASET type ids | 19 = model, 27 = texture, 35 = script | ✅ |

**Key structural insight (cube_mod):** an override/new asset does NOT require splicing into a
retail block. Each asset ships as its own tiny single-entry block; the patch ASET repoints the
hash at it. No entry-table surgery, no offsets to fix in retail data.

## What's missing

1. **Texture container BUILDER** (Rust). `texture.rs` only parses. `dds_to_ucfx_texture.py` is
   the Python original. Plan: donor-based, like models — take the asset's own retail container,
   replace the `BODY` leaf (new BC payload, full mip chain), patch `INFO` (dims/mips/format) and
   recompute `CSUM`. Reuses the container the engine already accepts; no from-scratch authoring.
2. **Mip chain generation** — box-filter downsample before texenc per level.
3. **Workshop "Mod project" state + UI** — the session ledger of edits and the Publish action.
4. **New-hash asset path in the GUI** — name → `pandemic_hash_m2`, ASET row with
   `secondary_ref=0xFFFFFFFF` (cube_mod `build_extra` shape), plus something that *references*
   the new asset (see risks).

## Architecture

New workshop module `publish.rs` + `mercs2_formats::texture_build`:

```
ModProject (persisted mod_project.json next to the output wad)
  items: Vec<ModItem>
    ReplaceTexture { hash, source_png: PathBuf }          // donor = retail container of `hash`
    ReplaceModel   { hash, source_mesh: PathBuf, opts }   // donor = retail container of `hash`
    AddModel       { name: String, donor_hash, source_mesh, opts }   // new m2(name) hash
    AddTexture     { name: String, source_png }
  output: PathBuf   // the mod wad (NOT the DLC-port vz-patch.wad — see risks)

Publish:
  for each item -> UCFX container (model_inject / texture_build)
              -> single-entry block  -> compress_sges
              -> PatchBlock { path "blocks\\VZ\\mod_<hash>.block", AsetEntry(hash, sec, 0xFFFF, type_id) }
  build_patch_wad_multi -> write -> print SHA-256 (mandate)
  -> WadStack: reload as overlay -> browser/preview now shows the published result (self-test)
```

GUI surfaces (extend existing panels, no new windows):
- Texture plate view / material rows: **"Replace from PNG…"** → adds `ReplaceTexture`.
- Imported model toolbar: **"Package as override of <selected>"** / **"Package as new asset…"**.
- New **Mod project** section (Details panel): item list, remove, output path, **Publish** button,
  spinner via the existing background-loader pattern (packaging must not freeze the frame loop).

## Risk ladder → milestones

**M1 — Texture override, end to end.** Lowest risk: donor container + payload swap; engine-side
format risk near zero. Deliverable: right-click any texture → replace → Publish → workshop
auto-overlay shows it; same wad drops into the game.
Risk to test: replacing a RESIDENT texture whose hires mips stream from c3 subtree blocks
(texture-high-mip-streaming): the engine may still stream retail hires over our override at close
range. Mitigation: test with a UI/shell texture first (no hires subtree), then a world texture;
if hires wins, override the subtree BODY blocks too (we know where they live).

**M2 — Model override, native.** `model_inject` donor recipe driven from the GUI (donor = the
overridden asset's own container, preserving HIER/materials the engine already instantiates).
Skinned transfer stays out of scope here (CJ weight-transfer is its own workstream); rigid +
bone-0 bind first, exactly like the proven recipe.

**M3 — Novel assets (new hashes).** `AddTexture`/`AddModel` with `m2(name)` + ASET row. The hard
part is not packaging but REFERENCE: nothing in retail data points at a new hash. Options, in
increasing ambition: (a) MTRL repoint in an overridden model → new texture (works today);
(b) Lua spawn (`Pg.Spawn` takes a raw m2 hash natively — name-registry memory) via a script
patch; (c) placement/blueprint edits (M4 territory). Script lesson from the heli experiment
applies to script-type assets: new resident scripts need DEPS wiring + ASET or the loader wedges.

**M4 — Scripts & blueprints.** Port `build_mod_patch.py`'s append-only scripts_vz path (UCFX
header VERBATIM rule; CSUM per container) — converges with the blueprint editor pipeline
(charter §Mission designer: full_moon → emit → LuaQ → patch).

## Hygiene rules (hard, from memory/mandates)

- Never overwrite the DLC-port `vz-patch.wad` build; the mod wad is a separate file, merged via
  `read_patch_wad`/append when the user wants one file (builder supports multi-block).
- SHA-256 print on every wad written; verify after copy.
- Ship CONVERTED/donor-based containers only — never raw foreign layouts (sarah lesson).
- All packaging on a background thread with the status-bar spinner (no frame-loop stalls).
- Workshop self-test after publish: auto-reload overlay, load the touched hashes, report
  per-asset load OK/FAIL in the status bar before the user ever boots the game.
