# Inspect tabs — behaviour contract (Anims · VFX · UI · FaceFX · Text · Layers)

The Workshop Polish Pass design draws Inspect only on the Models tab (`Inspect.dc.html`); the other tabs
follow that page's frame: the list at left, the viewport in the middle, the inspector at right, the
verb bar above the viewport, the footer status line. Models and Textures are covered by
[`inspect.md`](inspect.md); Audio by [`audio.md`](audio.md).

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
and what proves it. Nothing is drawn without its row being true. Where a row needs a format that is not
decoded yet, the decode is part of this pass and its proof is named in the row.

Decisions taken with the user (2026-10-27): one shared **hero** model that Anims and FaceFX play on,
compatible models listed first; VFX plays in a sim wired to every decoded field, its units pinned from
the memory dumps in `game-files/memory-dumps`; FaceFX goes all the way to a moving face; Layers draw as
3D instances in the Inspect viewport; Inspect reads **every original game WAD that is present**;
the UI tab renders the movie; names whose only source is block-string harvesting are dropped.

## Shared

### Sources: every present game WAD

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Source WADs | at boot, beside `vz.wad` + `vz-patch.wad` (+ `--overlay`), every present `shell.wad`, `Loading.wad` and language WAD (`English`, `French`, `German`, `Italian`, `Spanish`) in the archive's folder is opened read-only and indexed. Opening the archive never fails because one of these is missing; each feature that needs a specific WAD (the FaceFX voice and the Audio page need `English.wad`, a string table's language column needs that language's WAD) says which WAD is missing, where the user is looking | `app::open_archive`, `index::AssetIndex::build` | — | test: index over the retail folder holds `SHELL.gfx` and the French stringdb |
| Row source tag | a row from the vz stack shows as today (`+ ` = an overlay); a hash found only in another WAD shows that WAD's name as a dim suffix (`english_18 · shell`). A hash in the vz stack and another WAD lists once and previews the vz-stack copy (the copy the game's world loads); the inspector names every WAD that holds it and switches the preview to another WAD's copy, with "differs from vz" when its bytes differ (`docs/fixpack/wad_duplicate_inventory.md`: 112 duplicate keys, not all identical) | `AssetRow::src` | — | test: tag per source; one row per hash; the switch previews the other copy |
| Settings › What it opened | lists each extra WAD found, or "not present" | `settingspage` | — | capture |
| List follows the selection (bug: arrow keys, Page Up/Down, F4 and a `--ui-state` load moved the selection off-screen) | a selection that changed since the last frame is scrolled just into view; a click lands on a row already in view and scrolls nothing | `app.rs` `list_follow` | — | capture: `anims_material` shows its row selected at the top of a scrolled list |
| Sliders (bug: the rail was drawn in the card colour, so only the knob showed) | every slider (clip transport, Health, the material-animation playhead) draws its rail in `LINE` and fills it to the knob | `theme::scrub` | — | capture |

### The hero

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Hero card (Anims and FaceFX tabs, top of the inspector) | shows the hero model's name, or "No hero — pick the model clips play on". **Change…** opens the hero picker | `Settings::hero` | — | capture |
| Hero picker | a filterable list of every model. Anims: models whose HIER covers the selected clip's bone hashes sort first, each with its coverage (`60/60 bones`). FaceFX: models whose HIER holds the actor's face bones sort first, the actor's own model (`actor name = model name`) at the top. Picking a model with 0 coverage is refused, with the reason in the status line. The pick loads the hero into the viewport | `AnimBinding::resolve_to_hier`, actor bone list (below) | `settings.json` `hero` | test: ordering and refusal for a human clip and a 1-track md500 clip |
| Hero persistence | one hero for both tabs, kept across tabs and restarts; a hero no longer in the archive is cleared with a status-line note | `settings.rs` | `settings.json` | test |

### Row menu, by kind (bug: every non-Layer row offers the model menu today)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Right-click menu | **Preview** (= click / Enter for that kind), then the kind's **Add to Shipment ▸** routes from `quartermaster::routes_for` (none for a kind with no route: the item is absent), then Copy name / hash / manifest reference. Model actions (Load in viewport, Export, Retarget, Fit) appear only on Model rows. Today a Text row's "Load in viewport" fails with `no model ASET for 0x093F42E5` (`english_18`, a font) | `asset_menu` | clipboard / `manifest.yaml` | test: menu items per kind |
| Hover reason on a tab | replaced: each kind's preview is real, so the `Preview::None` reasons go; `Preview` names what the click does | `index::Kind::preview` | — | test |

### Names

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Debris names | a name harvested from block bytes (`docs/data/aset_block_strings.json`) is kept only when it starts with a letter and holds only `[A-Za-z0-9_]`, and is not on `crates/mercs2_workshop/data/rejected_names.json` (`wxfm0u`, `i4e2`, `csumoe9mucfxp`). Over the 281 names only the harvest supplies, that drops the 18 debris (`6xht`, `-ufzc9`, `.j0x.`, `2v3tx`, `1uud` …) and keeps the 263 real ones (the hijack clips the Lua spells, `pmc04_solano_actionhijack_*`, `telephone`, `oilrig_catwalka` …). Every reader of the harvest applies it (`--pack-data`, `aset_export`, the audio bins); those rows show their hash. The comment calling every entry "a verified preimage" (`index.rs:536`) is corrected | `mercs2_formats::hash::is_identifier_shaped`, `RainbowTable::load_harvested`, `index::load_harvested_names` | `aset_names.tsv` regenerated | test: debris dropped, real kept; the regenerated TSV loses exactly the 18 |
| Name sources move into the crate | the verified fragments `aset_discovered_names`, `aset_expanded_names`, `aset_model_names`, `aset_external_names`, `aset_roster_names`, `aset_variant_names` (`.json`) move from the notes repo's `docs/data/` into `crates/mercs2_workshop/data/`, and so does the repo root's `data/production_names.json` (the package `names.bin` is built from; `qm`, `skin_census` and the release workflow read it there too). `--pack-data` and the `wad_simulator` bins that read them (`aset_export`, `name_expand`, `aset_external_mine`, `vo_extract`, `vo_stream_extract`, `wavebank_scan`, `sfx_extract`, `sfx_namecrack`) read them there. `aset_block_strings.json`, the rainbow table, `bone_name_candidates.txt` and `live_registry_hashes.csv` stay in the notes repo as candidate sources | `index::{ASET_NAME_FRAGMENTS, crate_data_file}`, `qm` `names::PRODUCTION_NAMES` | the moved files | test: each fragment loads from the crate and none is `aset_block_strings`; `qm`'s walk-up finds the table (no longer a silent skip) |
| Proven names | names this pass proves go to `crates/mercs2_workshop/data/aset_discovered_names.json` (matching its indent): the 77 FaceFX set names, the 4 `PMC04_Solano_ActionHijack_*` chunks, the 1,116 cue names that hash to English clips, the hijack clip names from `hijack_*.lua` | `pandemic_hash_m2` check per name | that file | each name's hash re-checked by the writer |

## Anims (4,261 `animation`, `0x18166555`)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Row click / Enter | plays the clip on the hero with the bones it covers; no hero → the hero picker opens with this clip's compatible models first | `game_world::clip_for_rig`, `Preview` transport | — | test: a 60-track human clip binds 60/60 on `pmc_hum_mattias_v3`; `0x9C94874B` binds 1/1 on `oc_veh_helicopter_md500` |
| Transport | the existing controls: Space play/pause, `\` stop to bind pose, scrub slider, time readout. On this tab **F4 / `.` / `,` step the tab's rows** (next/previous clip in the list) | `Act::{PlayPause, ClipStop, ClipNav}` | — | test: F4 on the Anims tab selects and plays the next row |
| Inspector: clip card | name (or hash), block, codec (wavelet / interleaved / delta / MANM), duration, frames, tracks, bound tracks on the hero (`52/60`), the unbound bone hashes listed by name where known | `animgroup`, `anim` | — | test |
| Delta clips (56) | decoded and play: `anim::decode_delta`, a transcription of the retail `LtSampleDelta` call tree (per-DOF chunk = preserved raw f32s + LSB-first codes, `code·2^-bw·scale + offset`, running sum per block, the wavelet path's recompose) | `mercs2_formats::anim` | — | test: every retail delta clip decodes to unit rotations (`game_world::delta_clip_retail_tests`). **Open:** bit-exactness against the game — a live read of `LtSampleDelta`'s output the user drives in x32dbg (or the HCT 5.5 `AssetCc2.exe` oracle, a Windows run) |
| MANM material animations (29) | decoded (`manm::parse_material_anim`: `MANM` 16 B, `MINF` `[u32 group][u16 n]`, `TRCK` `[u32 property][f32 divisor][u16 N]` + N×`[i16 time][i16 value]`). A row opens a **Material animation** card: groups, tracks, key-time span, the `MANM` f32 (29.97 / 59.94 / 15.0), a playhead slider, and per track its live value and its curve, sampled by `Track::sample` — the retail `FUN_0068E9A0` operation for operation (value × 1/divisor; the first key's value before it, the last after it; a run of equal times gives its last key). Clicking or dragging a curve moves the playhead. Nothing loads on the hero: these drive material values, not bones. **Open, so not played on a model:** the key-time unit (what advances the controller's time at `+0x20` — a live read during `Object.PlayMaterialAnimation`), and which material field each property hash writes (`FUN_0068ECA0`'s target object, reached through a SecuROM thunk) | `mercs2_formats::manm`, `app::clip_info`, `theme::curve` | — | test: all 29 retail containers decode, 378 tracks (`tests/anim_container_roundtrip.rs`); sampler unit tests; capture `anims_material` |
| Clips with 0 tracks | the 29 with no transform track are exactly the MANM material animations (`anim_codec_census`); they show the card above | `animgroup` | — | census |
| Comment `game_world.rs:574` | it said every clip is the 60–105-track character family. Replaced by the census: 4,103 wavelet, 73 interleaved, 56 delta, 29 MANM; by track count 3,606 × 60–105, 356 × 20–59, 126 × 2–19, 141 × 1, 3 × over 105, 29 × 0; of vz.wad's 2,414 models with two or more HIER nodes, 312 carry a node some clip binds | `mercs2_probe --bin anim_codec_census` | — | — |
| `clipbind` probe | panicked on any `trnm` under 24 bytes, printed the count word unmasked, and printed a fixed "50-track clips ship no trnm" line. Fixed: reads only the words present, prints the count as the reader takes it (low half; high half = flags), states what the tag count shows | `mercs2_probe` | — | run over the 1-track `0x9C94874B` |

## VFX (314 `effect`, `0x5608BD5A`, + the `fxdict`)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Row label | the effect's name when known; otherwise the `Name` of every template whose `RedEffectComponent.name` starts it ("started by VZ C4 Explosion"), then the hash | worldentity templates | — | test: label for a templated unnamed effect |
| Row click / Enter | plays the effect on the pedestal, looping; the sim is driven by every decoded field: rate, rate variance, life, speed, size and its curve, spread, emission direction, inherit velocity, the GEOM spawn triangles, the TRFM channels and their curves, every FRCE kind (gravity, wind, drag, attractor, vortex), COLR over life, TEXT atlas frames (particles textured from the `vfx` atlas through the `fxdict` rectangles), PTYP blend | `fxdict::parse_effect_container`, `particles::EmitterDesc::from_effect`, `particles.wgsl` | — | test: `from_effect` maps every field; capture |
| Units | each field's unit (per second or per update, degrees or radians, metres) is pinned from the decomp of the spawn path (`docs/effect_container_format.md` §2.1) and the memory dumps' `pmc` state (`game-files/memory-dumps`, captured 2026-10-13/14 at the menu and in the PMC; the PMC interior places `global_particle_*` effects, `fx_probe`) — a running emitter's fields read beside its effect's bytes. That the PMC dumps hold running emitters is INFERRED until read. A unit neither settles is a live x32dbg read the user drives | decomp `FUN_00491920` family, dumps | `docs/effect_container_format.md` | each unit cited to a decomp line or a dump offset |
| Transport | Space restarts / pauses, the scrub sets the sim time, a speed control (¼×–2×) | `Scene::fx_start_desc` | — | test |
| Inspector: effect card | emitters (each: COLR gradient strip, atlas frames as thumbnails, spawn shape, the attribute table with names from `fxdict::attribute_name`, curves plotted), forces with their kind and attributes; the 16 unnamed attribute hashes show as hashes | `fxdict` | — | capture |
| Footer status | the stale "parameters are still hash-only" line goes | `index.rs` | — | — |

## UI (60 `scaleformgfx`, `0xFE0E8320`, + those in `shell.wad`)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Row click / Enter | renders the movie's first frame in the viewport (2D, stage size from the header, fit to the viewport) | new `mercs2_formats::gfx` decode + a 2D render pass | — | captures of `pause_menu`, a briefing, `SHELL.gfx` |
| Renderer | shapes (edge + curve records tessellated), solid / linear / radial gradient / bitmap fills, line styles, the display list (PlaceObject / RemoveObject with matrix, colour transform, depth, clip layers), sprites with their own timelines, edit-text drawn with the movie's fonts, external images bound by name (`<name>.tga` → texture `m2(name)`) from any present WAD, including the 1004 sub-image variant (`FUN_007d6120`) | `gfx.rs` | — | test per record kind; captures |
| Timeline | play/pause, frame scrub, frame number / count, fps from the header; the frame-label list (top-level and inside sprites) jumps to a label | `gfx.rs` | — | test: labels of `pause_menu` (14) and `Database` (78) |
| ActionScript | **Decision pending (asked with this contract):** an AVM1 interpreter in this pass, or only the frame actions that move the playhead (`stop`, `play`, `gotoAndStop`, `gotoAndPlay`, `nextFrame`, `prevFrame`) honoured and every other action listed per frame. A timeline that ignores `stop()` runs every menu past its stop frames, so neither option leaves AVM1 out entirely | `gfx.rs` | — | test per honoured action |
| Inspector: movie card | stage size, fps, frames, tag/feature counts, the external-image list (click → texture plate), frame labels, exports/imports | `gfx::features` | — | capture |

## FaceFX (86 `facefxanimationset` type 5, 31 `facefxactor` type 34)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Decoder | new `mercs2_formats::facefx`: the UCFX wrapper, FxArchive header (`FACE`, version 0x640–0x64A), name table, FxAnimSet → FxAnim → FxAnimCurve (keys `{time, value, slopeIn, slopeOut}`), and the actor: master bone list (20 face bones), face graph with every node body (`FxBonePoseNode`, `FxCombinerNode`), links and link functions. Read side ports the PS3 serializers (`FUN_00894e10`, `FUN_008927a8`, `FUN_00890458`, `FUN_008937a0`, `FUN_0089cc68`); byte reads are delegated per the RE rule. The survey's two scratch probes (`facefx_dump`, `facefx_parse`, in a side worktree) become one `mercs2_probe` subcommand over the decoder | `facefx.rs`, `mercs2_probe` | — | test: all 86 sets and 31 actors consume exactly to their body size |
| Evaluator | per frame: curves evaluated at t (interp 0 decoded from the FxAnimCurve evaluate), face-graph nodes solved (input sum/product through the link function, op at +0x20: add / multiply / replace, clamp +0x14/+0x18), bone-pose nodes blended into per-bone deltas on the 20 face bones | port of `FUN_00686ce0` / PS3 `FUN_0089d3b0`, `FUN_008a0128` | — | test: node values against a live x32dbg read the user drives during a briefing (the memory dumps hold only menu and PMC states, no speech); the dumps' `pmc` state gives the `FxActorInstance` layout first |
| Row = a set | expands to its animations (the VO cue names); clicking an animation plays it on the hero: face bones driven, the English voice line playing in sync | `vostream`, English.wad banks | — | test: set → animations; capture |
| Voice | the clip `m2(<animation name>)` from English.wad: embedded banks (961) and `vo_stream.pws` (155) | `audioplay`, `vostream` | — | test: decoded duration within 0.15 s of the last key for an embedded and a streamed line |
| Stream record with rate 0 | today `vostream` decodes a record whose rate is 0 at 44,100 Hz (`vostream.rs:114`), a silent default. It becomes an error naming the record | `vostream` | — | test |
| Hero for FaceFX | the actor whose name is the hero model drives it; a hero with no actor of its name lists the actors whose bones its HIER holds, and refuses with the reason when none | `Settings::hero` | — | test |
| Inspector: animation card | duration, curve list with key counts, the curves plotted over time with a playhead, curves with no node in the actor flagged "no node in <actor>" | `facefx` | — | capture |
| Actor rows (`pmc_hum_mattias` …) | click loads the actor's model as hero and shows the graph: nodes, links, bones | `facefx` | — | capture |

## Text (fonts type 15, string tables `0x39E5E978`, from every present WAD)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| String table row | the viewport becomes a table: key (name where `names.bin` holds it or a Lua string literal hashes to it — a literal written as the hash names nothing — else the hash), then one column per table of every present WAD, the opened table first; columns are at least 110 px and the table scrolls sideways when they do not fit; a key the table carries with no text reads `empty`, one it lacks `—`; filter by key or any shown text; cell hover shows the full text | `textpage`, `stringdb::parse`, `ucfx::extract_chunk_body` | — | test: filter and columns; Lua naming; retail: every table reads, English has 18,299 keys, `french` is read from shell.wad and French.wad; capture `text_stringtable` |
| String table inspector | the table (keys, hash), every WAD that ships it with "differs" on a copy whose text differs from the first's — clicking one shows that copy in the column; the chosen key's text in every table | `textpage::inspector` | — | test: choosing a copy shows and filters on its text |
| String table verbs | **Edit this text** adds an `edit_stringdb` of the open table to the Shipment (scaffolding one); **Copy key** copies the chosen key's name (enabled with a key chosen) | `textpage::verbs` | `manifest.yaml`, clipboard | test: the route seeds the table |
| Font row | the viewport shows the font's pages one at a time (a strip names each page's texture, size and glyph count; 7 for `english_18`), at whole-pixel zoom, with every glyph's box drawn on it; hovering a box names its glyph, clicking it chooses it | `mercs2_formats::font` (the `CHAR` record: UNORM16 box edges, width, height, byte 10, page — `FUN_004ac8e0`); pages via `parse_mtrl(…, MtrlSource::Font)` | — | test: a glyph's box from its edges; retail: every font reads, every `english_18`/`english_20` box lies inside its page at its width × height; capture `text_font` |
| Sample text in the font | a line the user types, laid out and drawn the way the game draws text with this font. **Pending:** the code that lays text out has not been found, so the meaning of byte 10 and the advance rule are not established; built when they are | the text layout code (search under way) | — | — |
| Inspector: font card | glyph count, pages, widest / tallest glyph (`INFO u16 @8 / @10`), `INFO u32 @4` shown raw as "meaning not established", hash, the WAD read from; the chosen glyph (character, page, box size, edges, byte 10 raw); every glyph listed (character, code point, page, size), a click choosing it | `textpage::font_inspector` | — | capture |

## Layers (923 `layer`, `0xE6B81A54`)

| Element | Action → effect | Data / code | Writes | Proof |
|---|---|---|---|---|
| Row click / Enter | the viewport switches to the layer view: every entity of the layer placed in 3D at its position and rotation, camera framing the layer's bounds (orbit + pan + zoom) | `placement::load_placements`, `worldutil::layer_index` | — | capture of `vz_state_pmccon004` |
| What an entity draws | a ModelName entity: its model. A template entity: the models its worldentity template carries (`Transform.Parent` at +0x24 → template in block 3185 → its model components with their own transforms; the subgraph walk is part of this pass). An entity whose template carries no model (trigger, spawner, light, zone): a labelled marker of its kind | `placement`, `worldentity`, `AssetIndex::world_templates` | — | test: `vz_state_pmccon004` 361 entities, 336 with a Parent, each resolves or is a marker |
| `Placement` | gains `parent` (the template key; today dropped into the tail) | `mercs2_formats::placement` | — | test |
| Entity table (inspector) | every entity: name, template, kind, position; click selects it in the viewport and frames it; clicking an entity in the viewport selects its row | — | — | test: pick ↔ row |
| Inspector: layer card | entity count by kind, block(s), the models it uses | — | — | capture |
| Row menu | Add to Shipment ▸ `activate_layer` / `edit_world` as today; on a hash-only layer the routes are absent (neither the path search nor `MarkForAddition` can resolve a hash). Bug: today `activate_layer` seeds the placeholder `replaces: [vz_state_pmccon004_pristine]` for every layer (`quartermaster.rs:1010-1014`, `1108-1111`), which would remove an unrelated layer. Fixed: it seeds `replaces: []` | `quartermaster::seeded` | `manifest.yaml` | test: a seeded `activate_layer` replaces nothing |
| `layer_block_for_edit` | returns the first block whose path contains the text and can over-read a shared block; replaced by the exact `layer_by_name` lookup | `quartermaster` | — | test |

## Order of work

Each tab lands as its own set of one-file commits, proven before the next starts: shared (sources,
row menu, names, hero) → Text → Anims → Layers → VFX → FaceFX → UI.
