# Depot — an editor and a preview for every kind

Every kind `qm` reads gets, on its Depot item:

- **its required parts**, each filled by a picker that lists what the game ships (an asset of the right type,
  a hook, a category, a layer) or by a file chooser filtered to the right extensions;
- **every optional part qm supports**, offered as an `+ Add <part>` control that adds it to the
  contribution, and an `✕` that takes it back out;
- **the contents of the files the contribution points at**, edited as structured rows rather than text:
  entity records, string keys, text pairs, effect fields, state scripts, shader classes;
- **a preview**, laid out like the `ShipmentFallback` and `ShipmentRawKind` boards: the shipped thing and
  yours, in the centre, with the facts and the findings in the inspector.

No kind falls through to "edit the manifest.yaml directly", and no part is a text box of YAML.

## Where it stands

`contribution_form` (`crates/mercs2_workshop/src/quartermaster.rs:2220`) is the field editor the Depot uses.
Across the 39 kinds of `Contribution::ALL_KINDS` (40 with `load_sound`, which joins the list) there are
132 top-level fields: 75 editable, 5 partly, 52
not at all.

| | kinds |
|---|---|
| A note only | `add_shop_item` (13 fields) |
| Fields missing | `add_outfit.wearer` (its pills are in the Shipment page's Wardrobe section, which the Depot does not draw), `add_model.retarget`, `add_model.collision`, `level_asset.templates`, `level_asset.edits`, `native_hook.signature_guard` |
| Fields partly editable | `add_outfit.textures` (hidden while `single_group` is on), `add_outfit.retarget` and the inspector's read-only `retarget_summary`, `add_model.group` (cannot go back to absent), `add_level.streams` and `level_asset.rungs` (no add/remove; a rung's `block` is not shown) |

The per-kind tables below mark what the Depot shows ("built"). Most other viewers exist elsewhere in the
workshop but take a hash on the open game stack, not the author's file, and each holds one app-wide slot that
Inspect also uses.

## What makes coverage hold

- **One arm per kind, every field named.** `contribution_form` loses its `other =>` arm and every arm
  destructures its variant without `..` or `field: _`. A kind or field qm adds then fails the workshop's
  compile until it has an editor.
- **Tests over every kind in `ALL_KINDS`** (`quartermaster::form_tests`): its form draws, bare and with
  every optional part present, without changing the contribution; and the contribution with every part
  present reads back from YAML as written. The add-menu test holds `KINDS` and `stub()` to `ALL_KINDS`.
- **Pickers read the game stack and the names table**, never a list typed into the workshop: models,
  textures, animations (Havok only for `replace_animation`), layers, string tables, sound banks/cues/
  wavebanks, shader stems (`shader3.bin`), effects and templates, terrain cells, scripts (the decompiled
  corpus), chatter sets (`data/chatter_sets.tsv`), hooks (`data/hooks.tsv`), sound categories
  (`RETAIL_CATEGORY_NAMES`). A typed value that is not on the list is still accepted where qm accepts a bare
  hash, and shows its lint finding.
- **Enums are pills or combos** showing qm's own value names.

## Shared pieces to build first

1. **Picker** — search-as-you-type over a list from the game stack, with the hash shown beside each name.
2. **Optional part** — `+ Add` / `✕`, for `Option` fields, `#[serde(default)]` lists and maps, and nested
   optional structs.
3. **List editor** — add, remove, reorder rows of a nested struct (cues, shader classes, rungs, entities).
4. **Depot-owned preview slots** — the Depot gets its own model preview, layer view, VFX view, texture plate
   and audio player, so opening an item never replaces what Inspect is showing. The 3D ones live in the
   Depot's own `World` (`depot_world` in `app.rs`) with its own orbit, drawn through the one `Scene` under
   the Depot's `WorldScope`: the scene keeps each entity's GPU buffers and render state by
   `(WorldScope, Entity)`, since every `World` numbers its entities from the same start. Uploads are kept
   by world too: models, the textures terrain cells share, per-model hidden draws, ground cover and trees
   by `(WorldScope, hash)`, so the Depot can hold an author's asset under the hash of the shipped one
   Inspect draws, and one world's unload never takes another's. `Scene::forget_scope` drops everything a
   discarded world uploaded (the Live mirror's reload, an archive change).
5. **Byte-input viewers** — each viewer gains a constructor from the author's bytes beside the one from a
   stack hash.

## Per kind

`R` = required, `O` = optional. "Exists" = a decoder or viewer in the workspace, not yet called from the
Depot; "built" = the Depot shows it today.

### Models and characters

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_outfit` | R `name` (model picker when no file), `slug`, `display`, `wearer` (hero pills); O `chatter` (set picker), `levels` (level picker list), `briefing_safe`, `model` (glb/gltf/obj), `donor` (model picker; empty is valid), `textures` diffuse/normal/specular (png), `retarget` (`from` combo + bone map table: source bone → target bone or dropped), `single_group` | — | yours on the donor rig, posed, with a donor clip playing; shipped donor beside it (exists: `import_model`, `build_preview`, `clip_player_compact`) |
| `add_model` | R `name`, `model`; O `donor` (required by build — shown as required), `group` (0..63, removable), `textures`, `retarget`, `collision` (donor / follow_geometry) | — | yours in the donor's place, with the collision `collision:` will ship drawn over it (exists: `import_model`, `phy2_build`, the collision overlay) |
| `replace_phy2` | R `target` (model picker), `phy2` | — | built |
| `edit_state_machine` | R `target` (destructible picker), `states` | the machine: nodes, states, Enter/Exit script lines, switch slots (`statemachine.rs`, built, no UI) | the model run through the edited machine, state chips, health; findings in the inspector |
| `add_tiny_geometry` | R `layer` (layer picker), `cell` row/col (or picked from the map), `key`, `objects` (placement pickers), `model` | — | built: the stand-in `qm build` makes (`tiny::build_stand_in`), its intact or its ruined part at its cell's centre on its materials' textures, each object's part tintable by its slot; the objects it stands in for, marked in the layers that place them (`tiny::World::placed`: an object may be another layer's, as retail's `vz_*_tiny` stand-ins draw); the stand-in rules' findings (`tiny::hermetic_problems`, `game_problems`) |

### Textures and sprites

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_texture` | R `name`, `image` (png); O `normal_map` | — | built: the PNG and the texture the build ships, each level, the encode error between them |
| `replace_texture` | R `target` (texture picker), `image` | — | built: the game's texture at full resolution, the PNG and the texture the build ships at the game's size and format, the encode error; beside it, the engine's still render of a model that wears it, with the built texture (or, while Shipped is chosen, the game's); the models of `vz.wad` whose materials cite it, one chosen for the render |
| `add_fx_sprite` | R `name`, `image` | — | built: the game's `vfx` atlas, the PNG, and the atlas the fx merge draws with each sprite marked where it went |

The viewer shows one image at a time, as Unreal's texture editor does: Shipped, Your PNG or As built, with
R, G, B and A toggles (alpha alone shows as grey over a checkerboard) and every mip level the texture
holds. "As built" is decoded from the block `qm build` ships (`build::texture_block`, which the lowering
calls); a sprite from the atlas `fx::merge_shipment` draws (the merge the lint's fx checks run). "Shipped"
is read at full streamed resolution (`wad::extract_texture_hires`). The encode error compares the PNG, as
the build swizzles a normal map, with the built texture's finest level. The models that wear a texture are
read from every model container of `vz.wad` once per game, as `mercs2_probe`'s `tex_users` reads them.

The render is the engine's own frame (`Scene::render_still`: every pass `render_with` runs, into an
offscreen texture shaped like the window, read back; nothing presented, no particle step, no 2D UI). The
model sits in a world and scope of its own (`shot_scope` in `app.rs`), so its upload with the built texture
never meets the Depot's or Inspect's. It draws at the state a freshly placed instance has
(`default_render_state`), a character kit at the first outfit of its templates that shows a part wearing
the texture (`outfit_render_state`), framed on its rest-pose bounds (`Scene::model_bounds`) from
three-quarters above; the centred square of the frame is shown. A spinner holds its place while the wearers
are read and while it renders.

### Sound

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_sound` | R `bank`, `category` (combo), `cues` (list editor, 16 fields each), `load_in` | — | built: each cue of the bank as built, its wave playable, and the loader lines that load it |
| `load_sound` | R `in` (session pills); O `soundbank`, `wavebank`, `stream` (each a bank picker; at least one) | — | built: each named soundbank's cues and each named wavebank's lines, playable, from the game or the item's `level_asset` rows (a stream: the level wavebank it is named for); the loader lines the build writes |
| `replace_sound_bank` | R `bank` (bank picker), `category`, `cues`; O `language` | — | built: per WAD that carries the bank, each cue shipped beside yours, replaced, new or dropped |
| `replace_sound_cue` | R `bank`, `category`, `cue` (a shipped cue's "Replace this cue"); O `language` | — | built: per WAD that carries the bank, the cue shipped beside yours with their values, and the bank's other cues, unchanged |
| `replace_sound_wave` | R `wavebank`, `line` (the Audio page's "Replace this line"), `wave`; O `language` | — | built: the line shipped beside yours, and each cue that plays it, shipped beside the build's |

"Yours" is read back from the blocks `qm build` ships (`sound::lower_shipment_sound`), so a wave plays as the
encoder wrote it; "shipped" is each game WAD's copy of the bank (`sound::carried_banks`), with every
wavebank its groups play, another bank's included. A streamed wave reads its range of the `.pws` in the
game's `Audios` folder: the wavebank names the alias `<name>.pws`, and the file is the localized name,
`vo_stream.english.pws` (`mrxsoundbanks.lua:80-108`). A cue that picks among several waves lists each; the
waves play as recorded, the cue's gain and pitch shown as values. The inspector's Build card lists each
block the build ships and each session's loader lines (`link::session_sound_subscriptions`, the generator
both loaders write theirs with); the build runs under the Shipment name `<shipment>`, which a Shipment's
override wavebank and loader lines carry. The Depot plays on a player of its own (`depot_player`), which
stops when the Depot, the item or the game changes.

### Animation

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_animation` | R `name`, `clip` (hkx), `trnm`; O `events` | the events as rows (time, name, category) | built: the clip on a hero you pick, with scrub, loop and event marks, decode facts and the bone binding |
| `replace_animation` | R `target` (Havok clip picker), `clip`, `trnm`; O `events` | the events as rows; **Start from the shipped clip** | built: the shipped clip beside yours on the same hero, one clock |

### Lua

| kind | parts | file contents | preview |
|---|---|---|---|
| `patch_lua` | R `target` (script picker), `append` | the append (built) | built: the shipped script beside the append |
| `set_spawn_lists` | R `target` (script picker), `lists` (rows: `function`, `building`, `list`; O `group`) | — (qm writes the Lua) | built: the spawn table over the target's calls, the shipped function beside the Lua qm writes |
| `add_script` | R `name`, `source`; O `exports` (identifier list), `load` (`when` hook picker + `in` session pills) | the module source | the source with its compile result |
| `replace_lua` | R `target` (script picker), `source` | the replacement source | shipped script beside yours, with a diff |

### World

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_placement` | R `layer` (layer picker), `entity` | built: the entity records — key, model (picker), position, orientation (none, yaw, or yaw/pitch/roll), name | built: the layer entry the build appends, its entities ringed |
| `add_layer` | R `name`, `template` (layer picker), `entities` | built: as above | built: the new layer on its own |
| `edit_world` | R `layer`, `edits` | built: per entity a move, a turn (yaw/pitch/roll) and a re-model, each added or removed; entities picked from the layer's own | built: the layer entries holding what the build changes, after the build or as shipped |
| `activate_layer` | R `layer`; O `replaces` (layer picker list) | — | built: each layer it names as the game stack carries it (`GameStack::layer_by_name`), the one it adds or one it removes drawn |
| `replace_terrain_cell` | R `target` (terrain cell picker), `cell` | — | built: yours or the shipped cell, each built from its bytes as the game builds a cell (`game_world::terrainmesh_from_container`, `mesh::build_terrain_cell`) at the `TerrainObject` placement that names the target, with its template's fade; both cells' patches, groups, vertices and materials side by side |

The world files are read and written with qm's own readers and writers (`world::parse`/`write_world`,
`world::read_entities`/`write_entities`). Any file that reads is edited, and a write rewrites only what
the edit changed (`mercs2_quartermaster::lossless`), so a comment or a number spelled another way stays.
`qm extract-world`'s output is the writer's output.

What the viewport draws is what `qm build` builds: `build::world_block`, the function the lowering calls,
with `lint::placement_game_checks` for M0198, on a worker (`depotworld.rs`). The drawn block is the built
block's entries that hold a changed or new entity (`depotworld::sub_block`), read by `LayerView` in the
Depot's world. A refusal is shown over the viewport and in the inspector. A click on an entity selects
the record that places or edits it; for an `edit_world` entity no edit names, it filters the layer's
entity list to it. A layer entry in the compact form a ported level ships is refused: the placement
readers take the named form, which Inspect inflates it to (`WadStack::named_ecs`).

A new record's key is the one after the highest key any layer of the game stack places, below the runtime
spawn keys (`0x4xxxxxxx`, `Object.Remove` at `0x5CDCB6`); bit 31 is the template key space. The key field
names M0198's cases (another record's key; for `add_placement`, a key the target block places) and, as a
warning, a key another layer places or one in either reserved space.

### Effects

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_fx` | R `name`, `effect`, `template` (`name`, `name_flag`, components from the template schema) | built: the effect node by node (each emitter, force and shape table), started from a shipped effect; the template's components as typed fields of their classes, started from a shipped template | built: the effect as built, running in the engine under its template, with the transport |
| `replace_fx` | R `target` (effect or template picker), `edits` | built: the shipped effect after the edits, node by node; each field an edit sets marked, with the shipped value and Use shipped | built: Shipped or As built, running in the engine under the templates that start it |

What the viewport runs is what `qm build` ships: the item's fx merged over the game's
(`fx::merge_shipment`, the merge the build and the lint's fx checks run), the effect read back from the
merged effects block, under the templates that start it in the merged `worldentity` (a `replace_fx`
through a template runs under that template first), drawn from the atlas and `fxdict` the merge writes
when the item adds or repaints sprites, otherwise the game's. It runs in the retail sim
(`mercs2_engine::fx_sim`) in the Depot's world: the scene keeps each world's atlas, effects, emitters and
glow cards apart (`particles::ParticleSystem`), and a frame of one world steps and draws that world's
alone, so the Depot's effect never draws in Inspect's viewport or its atlas in Inspect's effects. Restart,
play, scrub, speed, travel and the starting template are Inspect's VFX transport (`vfxview`).

The editor edits the effect as qm's `replace_fx` operations (`fx::Edit`): a value, a curve's keys, an
option bit; a frame added or removed; the colour of one key or a tint of every key; the flags; the shape
an emitter spawns on and how many of its triangles it samples; a force's parameters; a shape's
triangles (P, A and B; the area and normal follow from A × B, on the side the normal was); and an
emitter, force or shape duplicated or removed. An `add_fx` applies the operation to its form and writes
the form back (`EffectForm::express`); a `replace_fx` folds it into its edits file: an edit of a field an
earlier edit sets replaces that edit unless an operation that adds or removes a node comes between
(which renumbers what it names), and an edit that leaves the effect as it was without it is taken out.
An edit the writer refuses is not written; the refusal shows on the status line. Any file that reads is
edited, and a write rewrites only what the edit changed, so what the author wrote by hand stays. A
new `add_fx` starts its form from an effect the game ships.

An `add_fx` template's components are edited against the class schemas of the game's `worldentity`: each
field typed as `template.rs` types it (a flag, a count, an integer, a hash with the name it resolves to,
a number, three or eight numbers, a string), a component added from a shipped template's record of its
class, the whole set started from a shipped template with its `RedEffectComponent` naming the item's
effect. The Shipment page, which reads no game classes, keeps the class-and-field rows.

### Shaders

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_shader` | R `family` (45-value combo), `classes` — created from the family: 4 slots for a pixel family (base, `_pl`, `_sl`, `_pl_sl`), 1 for vertex; each `name`, `stem`, `shader` and `shader_low` as asm or blob | — | each class's assembly and the bytecode it assembles to, the constant table, unbound constants (exists: `sm3asm`, `qm::shader`) |
| `replace_shader` | R `target` (stem picker from `shader3.bin`), `shader`; O `shader_low` (shown as required when the stem has a low record) | — | shipped disassembly beside yours, with a diff |

No pixels: the engine draws through WGSL and nothing in the workspace runs SM3 bytecode.

### Text

| kind | parts | file contents | preview |
|---|---|---|---|
| `edit_stringdb` | R `target` (table picker), `strings` | rows of key → new text, with the shipped text beside; a key the table lacks is flagged | the shipped table with the text the build writes beside each key it changes, and qm's refusal when it refuses (`build::shipment_string_tables`) |
| `add_stringdb_keys` | R `target`, `strings` | rows of new key → text; a key the table has is flagged | as `edit_stringdb` |
| `replace_stringdb_text` | R `target`, `pairs` | rows of old text → new text, with every key each one hits; an old text no key carries is flagged | as `edit_stringdb` |
| `add_language` | R `name`, `display`, `strings`; O `base` (table picker) | the translation rows against the base table | the base table with the translation beside each key (`build::language_table`) |

The rows are the file's lines, read and written with qm's line grammar (`build::string_edit_line` /
`string_edit_text`, `build::text_pair_line` / `text_pair_text`): an edit rewrites only its own line, and
comments, blank lines and the other lines stay byte for byte. A row the table lists is added from the table
below the rows. A strings line cannot hold a line break or text that starts or ends with whitespace (qm trims
it), and a pairs line cannot hold a tab or a line break; such text is refused with that reason. In retail
`english`, 13 of 18,299 keys have such text.

### Levels

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_level` | R `name`, `display`; O `streams` (list) | — | the level's streams and what the patch will carry |
| `level_asset` | R `level`, `type`, `name`, `rungs` (list: `block`, `file`); O `templates`, `edits` (template → class → field) | — | by `type`, the matching viewer on the author's chunk (needs the byte-input viewers) |

### Game folder, code, shop, raw

| kind | parts | file contents | preview |
|---|---|---|---|
| `native_hook` | R `target` (retail/reimpl); O `plugin`, `symbol`, `touches`, `signature_guard` (per touch: prologue bytes) | — | the plugin's PE facts and the shipped bytes at each touched address (`qm::pe`) |
| `place_file` | R `file`, `dest` | — | the file's facts; text shown as text |
| `add_runtime_dll` | R `dll` | — | PE facts: machine, exports, the name it is placed under |
| `add_shop_item` | R `id`, `name`, `icon`, `shops`; O `description`, `catalog`, `type`, `cash_cost`, `fuel_cost`, `max_stock`, `unlocked`, `behaviour` (`module` + optional `cargo`, `delivery_vehicle`, `script`), `equipment_type` | — | the catalog row the build appends, and the icon from its atlas key |
| `raw` | R `payload`, `target_layer`, `touches`; O `description` | — | the payload's block table and the assets it names against `touches` |

### Movies

| kind | parts | file contents | preview |
|---|---|---|---|
| `add_movie` | R `name`, `movie` | — | the movie played headless, a frame scrubber and its labels; its images from the game stack (exists: `GfxMovie::parse` → `MovieDef::decode` → `mercs2_gfx` stage and AVM → `gfx_pass::render_to_rgba`, the `mercs2_probe gfx-render` path) |
| `add_ui` | R `name`, `movie` | — | as `add_movie` |

Host state a movie reads (owner-Lua callbacks, `_level0`, FSCommand replies) is supplied by the Workshop,
as the Inspect ▸ UI tab supplies it (`.claude/plans/workshop-ui-tab.md`); input is faked, never captured.
