# Scope — the `vz_state` world overlay as a contribution kind

Companion to `edit_state_machine`. Where that edits the **in-model** destruction machine (per-object,
real-time, reversible-in-data — which body shows in which damage state), this edits the **world-state
overlay**: the authored, *permanent* half of the same system. A mission that "destroys building X for
the rest of the act" runs the in-model machine to `DestroyedState` for the live show **and** flips a
`vz_state` overlay so the ruin survives a stream-out/stream-in
([`docs/reverse_engineer/state_machine_destruction_code_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/state_machine_destruction_code_map.md) §6). This doc scopes the second half;
it is grounded entirely in the reverse-engineering already in the corpus — nothing here is re-derived.

## What `vz_state` actually is (proven — do not re-derive)

`vz_state` is **not a distinct format. It is `layers_static`** ([`docs/placement_data_format.md`](../placement_data_format.md) §3,
RETRACTED-IN-BULK correction 2026-07-21, evidence: proven). A layer block is a UCFX → `CHDR` →
`COMP{info,schm,data}` tree of **entity placements**, one code path for both:

- `layers_static` — the always-visible base world (block 29, 7.6 MB, 173 sub-UCFX entity groups).
- `vz_state_*` — 746 conditional overlays, 3,501 placements, toggled by game state
  ([`docs/gameplay_data_ue5_mapping.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/gameplay_data_ue5_mapping.md), [`docs/game_data_analysis.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/game_data_analysis.md) §5).

The visibility model ([`AGENTS.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/AGENTS.md), [`docs/ue_game_bindings.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/ue_game_bindings.md) §5): `layers_static` always on;
`*_pristine` (~430) on by default; `*_ruined`/`*_destroyed` (~27), `*_staging`/`*_combat`/`*_defenses`
(~890), `*_act1..3` (~270) all **hidden** until a script turns them on.

### The records (from [`docs/modernization/world_streaming_spec.md`](../modernization/world_streaming_spec.md) §2A)

43 COMP types exist; three matter, all keyed by a `u32 entity_key`:

| COMP | Bytes | Layout |
|---|---|---|
| `Transform` | 42 | `+0 u32 key · +4 f32 x · +8 f32 y · +12 f32 z · +16 f32 pad · +20 qx · +24 qy · +28 qz · +32 qw · +36 6-byte tail` — unit quat, LH game space; ~16% carry pitch/roll, use the full quat |
| `Name` | var | `{u32 key}["EntityName 0xHEXID\0"]` |
| `ModelName` | 8 | `{u32 key, u32 model_hash}` — `model_hash == pandemic_hash_m2(asset_name) ==` the model ASET hash |

⚠ **COMP child offsets are RELATIVE to `data_area_start` in `layers_static` but ABSOLUTE in
`vz_state`.** The walker in `mercs2_formats::placement` already handles both.

### What exists, what does not

- **Parser: yes.** `mercs2_formats::placement::{load_placements, load_model_placements, comp_inventory}`
  reads all of the above; `placement_extractor.py` is the Python reference.
- **Writer: no.** There is no placement serializer, and `placement::Placement` keeps only
  `{key, name, pos, quat, sub_block}` — it **drops the record's `+16` pad and `+36` 6-byte tail**. So
  byte-identical re-emission needs an **in-place field patch** (rewrite only `x/y/z/qx..qw` and the
  `ModelName.model_hash`, leave pad/tail untouched), exactly the lesson `edit_state_machine` learned
  before its full regenerator — not a parse-then-regenerate.

## Proposed kinds

Two tractable now, one harder — mirroring `edit_state_machine`'s "edit is easy, add re-bases the tree":

1. **`move_entity` / `reskin_entity`** (in-place edit — LOW risk). Target a layer + `entity_key`;
   set a new position/rotation (patch the 42-byte `Transform` in place) and/or a new model (patch the
   8-byte `ModelName.model_hash`). Nothing resizes, so no COMP-tree splice: emit the layer block as an
   overlay the same way `edit_state_machine` emits the model block (single edited block, ASET row
   copied, `source_block_index` set so `build_patch_wad_multi` recomputes refs).

2. **`activate_layer`** (Script — LOW risk, reuses `add_ui`'s loader). Turn a normally-hidden overlay
   (`*_ruined`, `*_staging`, an act layer) **on** permanently by emitting one
   `MrxLayerManager.MarkForAddition("<Vz_State_Name>")` registration into `qm_modloader` — the exact
   expandable-load-space + trampoline mechanism `add_ui` already ships. This is how retail missions
   do it (`vz/oiljob008.lua`: `sDestroyedLayer = "Vz_State_..._Ruined"`), so it is the proven path.

3. **`add_entity`** (COMP-tree splice — HIGHER, deferred with a survey). Adding a new
   Transform+Name+ModelName triple grows the `CHDR`/`COMP` tree and re-bases child offsets (relative
   in `layers_static`, absolute in `vz_state`). This is the placement analog of adding a state to the
   machine; land it after the survey below proves the splice, not before.

## Build order (measure-first, the established methodology)

0. **Round-trip survey** (the `scripts_block`/`state_machine` pattern): across all 746 `vz_state`
   blocks + `layers_static`, does an in-place Transform/ModelName patch re-emit the block
   byte-identically (pad + tail preserved, offsets intact)? Pass ⇒ the in-place writer is bounded;
   fail ⇒ the survey names the dropped field. This runs **before** any lowering.

   > ### ✅ DONE (2026-08-01) — the in-place writer is bounded
   > `mercs2_formats/tests/placement_roundtrip_survey.rs`, over retail (747 blocks: `layers_static` +
   > 746 `vz_state`):
   > * **100,491 `Transform` records tile at a fixed 42 bytes**, zero non-tiling. So an edit is
   >   arithmetic: `data_off + i*42 + field`.
   > * **Layout confirmed**: all 100,491 records' pos/quat read at `+4/+8/+12` and `+20..+32` MATCH
   >   `load_placements` — **0 mismatches**.
   > * **`+16` pad is always 0**; the **`+36` 6-byte tail varies** (2,198 distinct) — so the writer
   >   MUST patch in place (rewrite only pos/quat/model, preserve pad+tail), never parse-then-regen.
   > * **A no-op in-place re-emit reproduces every one of the 747 blocks byte-for-byte.**
   > * ⚠ One trap the survey caught: the `schm` payload-stride word reads **52**, not the operative
   >   42 — trust the tiling + parser agreement, not the schema word.
   >
   > So `move_entity` / `reskin_entity` (in-place Transform / ModelName patch) is a **bounded job**.
   > `add_entity` (the COMP-tree splice) still wants its own survey.
1. ✅ **DONE (2026-08-01)** — `placement::patch_transform` / `patch_model` (in-place, byte-preserving)
   + `TRANSFORM_STRIDE`/`MODELNAME_STRIDE`. `tests/placement_writer.rs`: moving an entity reads back
   moved with its quat untouched, no sibling moves, revert is byte-identical; reskin repoints one
   `ModelName` and reverts; an unknown key patches nothing.
2. ✅ **DONE (2026-08-01)** — the block emission. `GameStack::layer_block_for_edit` hands back the
   whole layer (block, PTHS path, index, ASET rows); `build::emit_edited_layer` re-emits it as an
   overlay shadowing the base path with its rows restated. Proven end to end: an edited vz_state layer
   emits an overlay that decodes to exactly the edit and reads the entity back moved; a no-op
   reproduces the decoded bytes. So a placement edit is `layer_block_for_edit → patch_* → emit`.
3. `activate_layer` via the `qm_modloader` registration (reuse `add_ui`).

**The manifest-kind surface — ✅ DONE (2026-08-01):** the `edit_world` kind landed — an
extract-then-edit `world:` schema (`crates/mercs2_quartermaster/src/world.rs`: entity by key/name →
`pos`/`quat`/`model`), the `lower` arm calling the engine above, `qm extract-world` to dump the
baseline, and the routine `blast`/`discover`/workshop/conformance surface every kind carries. Proven
end to end against retail `vz.wad` (`tests/build.rs::edit_world_builds_an_overlay_that_moves_an_entity`):
the moved entity reads back at its new position and every other entity is byte-unchanged. Documented
in `manifest_format.md`. Same "extract, don't author" rule as `states:` — the author edits a dumped
baseline rather than hand-writing keys.

**`activate_layer` — ✅ DONE (2026-08-01):** turn a normally-hidden overlay on from Lua. It reuses
`add_ui`'s loader exactly — a `LayerRegistration` baked into `qm_modloader` as
`MrxLayerManager.MarkForAddition("<layer>")` / `MarkForRemoval` (the vanilla-contract calls, confirmed
in the decompiled corpus), reached by the one shared `wifpmcinterior` trampoline. No Data half, no
`src/` file. Wired through the whole kind surface with M0194 (a mistyped case-sensitive layer name is
a silent no-op; firing + quiet fixtures) and proven end to end against retail (the build mints
`qm_modloader` carrying the marks and layer names in its bytecode). The vz_state overlay mechanism is
now complete: **edit placements in place (`edit_world`)** and **switch whole overlays on
(`activate_layer`)**.

## Notes carried from the standing constraints

- **Bundle the data.** Any reference table the kind needs at runtime — the `vz_state` layer-name list,
  the COMP schema from `cdbsizes.ini` — is copied into the workshop's packed data dir, never linked
  from outside the repo, so a released build carries it.
- **The reference `vz.wad`** is the user's game stack (provided at build time), not bundled — same as
  every other game-aware kind.
- This is a **World-domain** contribution (domain spine §C): its subjects are placements/layers, and
  its governing scripts are the `MrxLayer*` / mission Lua the domain already lists.
