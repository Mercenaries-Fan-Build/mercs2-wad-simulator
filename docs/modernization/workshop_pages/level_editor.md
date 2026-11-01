# Level editor — behaviour contract (Level · Depot world edit · New level)

The design is the "Level editor" row of the Workshop Polish Pass canvas: `LevelMap`, `LevelLayers`,
`LevelState`, `LevelCompare`, `LevelChecks`, `LevelDraft`, `LevelNew`, and the rail `RailLevel`. Layers
leave Inspect and become one rail page, **Level**, which holds the world map, the outliner, the
viewport, world states, comparison, checks and budgets.

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
what is missing, and what proves it. Nothing is drawn without its row being true. Inspect stays
read-only base game; edits live in a Depot item; a Shipment packages Ready items; the workshop
validates and hands to Modkit, and never installs. Novel content is additive through `vz-patch.wad`.

Grades: PROVEN (measured this pass or cited to code), INFERRED, UNPROVEN. Numbers come from
`mercs2_probe layer --catalog`, `layer <name>` over all 925 layers, `save-dump`, `overlays`,
`dump-containers`, `dump-hibernation`, `dump-pool` and the decompiled Lua in `docs/mercs2-luacd/src`.
Line numbers in Lua are decompiled lines.

## Facts the design stands on

| Fact | Evidence | Grade |
|---|---|---|
| A world state is a set of layers. A save stores the dynamic part as `tLayerData` | `mrxlayermanager.lua` `SaveSingleton`, `LoadSingleton`; `mercs2_formats::save` | PROVEN |
| New game = block 29 (173 layers), `vz_mar_roads`, `vz_base`, and 246 default dynamic layers: 421 layers, 75,263 entities | `vz/xQ!L.lua` `_tStaticLayers` (170), `_tDefaultDynamicLayers` (246) | PROVEN |
| Save act1end: 235 dynamic layers, 410 in all, 75,829 entities. Save 6A0E523C: 238, 413, 76,040. The difference is `vz_state_car_city_act2chi` (7), `_act2chi_staging` (193), `vz_state_cumana_act2chi` (11) | `save-dump`, joined to `layer` | PROVEN |
| `mercs2_probe overlays` counts 62,624 + overlays and leaves out `vz_mar_roads` (618) and `vz_base` (58), hence its 75,153 against 75,829 here | `main.rs` `overlays_report` | PROVEN |
| The layers of block 29 that no Lua requests are live: keys of `vz_angel_falls_base`, `vz_angel_falls_scrub` and `vz_chi_job005_b` are in the SceneObject container of both captures read (`0x000C9901` at −3001.48, 130.09, −3142.25, its Transform). Keys of `vz_hidden` and `vz_vehiclehandlingtest` are absent | `dump-pool <capture> SceneObject --key`, clean-load and mid-mission | PROVEN |
| Why they are live is not known. INFERRED: a load of one layer of block 29 brings the whole block | `layer_load_code_map.md` §1 says "never requested"; both hold | INFERRED |
| Switching: `Add` loads at once; `MarkForRemoval` / `MarkForAddition` queue, `ProcessMarkedLayers` applies | `mrxlayermanager.lua` | PROVEN |
| 16 missions switch the world on completion; PmcCon003 alone removes 5 and adds 65 | `vz/wifmissionflow.lua` `fConseq` | PROVEN |
| A job's layers come from slots in its script: `sPristineLayer`, `sStagingLayer`, `sDefenseLayer`, `sDestroyedLayer`, `sCapturedLayer`, `sVerifiedLayer`. 378 layer names sit in those slots, over 124 subjects | 24 Lua files; `mrxtaskjob.lua:15-28`, `mrxtaskjobdestroyset.lua:49-59`, `mrxtaskcontractoutpost.lua:10-20,153-160` | PROVEN |
| `_ruined` and `_destroyed` are one role: `oiljob008.lua:90-111` puts `_Ruined` layers in `sDestroyedLayer`, and 2 `_Captured` layers too (`:76`, `:83`). Ruins sit on the pristine positions: `oiljob008_d` 3 of 3 | Lua, `layer` | PROVEN |
| Swap pairs: states replace entities in place. Over 42 pristine/destroyed pairs, 264 of 393 destroyed entities sit within 5 cm of a pristine entity; ChiJob006_C is 11 of 11. Over 19 staging/captured pairs, 403 of 1,010; AllJob001_03 is 51 of 76, e.g. 28 `_vzoutpost_walla` → `_vzoutpost_walla_AL`. Nothing in the data links them | position join over `layer` output | PROVEN (join), INFERRED (intent) |
| Linked positions: a child named by a placed `PhysicalLink` sits at parent × hard point × offset; its Transform is not its position | `docs/placement_data_format.md` §2.5 | PROVEN |
| A cdbsizes pool is a presize. Capture mid-mission: `Name` 82,666 against 6,912, `Label` 7,498 against 6,400 | `dump-containers`; memory `ecs-container-descriptor-and-pool-sizing` | PROVEN |
| The hibernation grid is the pool the veg work hit. Both use the 12-bit `+0x18` field, the `0x1F0` block, chain at `+0x1E8`, quantiser `FUN_005168B0`. Mid-mission capture: 78,163 rows in 2,919 blocks, none over 32 rows; 789 block ids below 1024 (the 32 × 32 cells), 2,130 at or above it, highest 3,161 | `hibernation_grid.md` §1–§2; memory `veg-density-pool-12bit-ceiling`; `dump-pool SceneObject --bytes 28`, `u16 @+0x18 & 0xFFF` | PROVEN |
| Cell occupancy predicts chained blocks: the capture's own positions give Σ(⌈n / 32⌉ − 1) = 2,137 against 2,130 live | same dump | PROVEN for this capture |
| The pool is 4,000 blocks with no bounds check; the game livelocked at cursor 4,003 (`0x4F217F`, `FUN_005161E0`) | memory `veg-density-pool-12bit-ceiling` | PROVEN |
| The 1,024 cell blocks count against the 4,000: chained ids start at 1024 in the same 12-bit field | capture ids | INFERRED. Read as such, act1end needs 1,024 + 2,057 = 3,081 of 4,000. Counted as chains only it is 2,057. U5 settles it |
| Other ceilings: stand-in slot lists 1,400 (`0x0050F1BE`, M0246), retail 1,208; 192 objects per stand-in (M0240) | `manifest_format.md` | PROVEN |
| Entity draws: 150 of one model clean, 462 froze in `FUN_00856760` | memory `veg-density-pool-12bit-ceiling` | PROVEN for those two runs; the ceiling between is unknown |
| Key ranges: placed keys run to `0x00144F24`, then 242 at `0x10000000`–`0x10001F5B`; DLC01 stems show `0x0014EEC5`–`0x00156ABB`; runtime entities in the capture are `0x40000010`–`0x4004B45E` (25,551) | `layer` over all layers, `dump-pool`, `dlc_blocks` | PROVEN |
| Map image: `vz_lrterrain`, entry `0xA007B5B9` of block 3121, 2048 × 2048 DXT1, covers 8,000 × 8,000 m | `format_reference.md` §13.4 | PROVEN |
| Map mapping: `px = (4000 − x) / 8000 · W`, `py = (4000 − z) / 8000 · H`. All 42 POI points of `vz_poi_lineregions` land on their places | plotted once. Conflicts with §13.4.1 `u = (x + 4000) / 8000` | PROVEN |
| Script names: 569 distinct literals in `GetGuidByName("…")` (567 folding case), 492 of them name a placed entity with or without its key suffix. The brief's 535 came from another count | regex over the Lua | PROVEN (this count) |
| 44 literal layer names in Lua have no layer; `_AddRequest` culls them by `Pg.AssetExists`. 30 are `_tg` names | Lua against the catalog; `mrxlayermanager.lua` | PROVEN |

## Page and navigation

Decided with the user (2026-11-01), for now: the rail reads LEVEL at 02 and DEPOT at 07.

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Rail (`RailLevel`) | Adds **LEVEL** at 02 and **DEPOT** at 07; every label 11 px. The later numbers move up by one or two | `app.rs` `Workbench::ALL`, `gui::theme::RailIcon` | — | `Workbench::Level`, `Workbench::Depot`. "depot" has no hit in `crates/mercs2_workshop/src` | the rail tests in `app.rs`; capture |
| Inspect tab strip | `LAYERS` leaves. Its row menu routes (`quartermaster.rs:1057`) move to the outliner's row menu | `index::Kind::ALL`, `app.rs:4320`, `app.rs:5949`, `app.rs:7604` | — | `Kind::Layer` stays an index kind; only the tab goes | test: Inspect tabs omit Layers; seeded routes unchanged |
| `--ui-state` | `tab: "WORLD LAYERS"` becomes `page: "Level"`, and `load` folds case. Today the row match is exact, so `vz_lowresterrain` fails where `vz_LowResTerrain` is the row's label | `ui_state.rs`, `app.rs:3025` | — | case-folded match | test: both spellings open one row |
| Level picker (header) | Lists levels by name: `vz`, each other level WAD found, the Depot's new levels. Choosing one reloads the page's catalog | `index`, `game_paths` | `settings.json` | a level is a page argument, not a constant | test: catalog per level |
| World state picker (header) | Lists New game, each profile in the Saves folder, each saved state, the current scrub. Choosing one sets which layers count as on, everywhere on the page | see world-state model | `settings.json` last state | — | test: 421, 410, 413 layers; 75,263, 75,829, 76,040 entities |
| Panel widths | Left and right panels use `gui::held_side_panel`; only a drag sets a width; rows cut text | `gui.rs` | `settings.json` | — | capture at the narrowest width |
| Type | Every size from `theme::type_scale()`: 11, 13, 15, 18 | `gui.rs` | — | — | `no_hard_coded_font_sizes_in_the_workshop_sources` |

Load cost: the page opens from a catalog built once per archive, not from layer blocks; only opened
layers read their entities. `mercs2_probe layer --catalog` is the logic, in a probe crate; it moves to
`mercs2_formats` so the workshop can call it and the probe stays a thin caller.

## Layer taxonomy

Three independent facets. **Role** is what a layer holds, **Lifecycle** is who switches it, **Place**
is where it is. The outliner groups by any one (Place, Role, Mission, Lifecycle), and chips show the
others.

Role comes from the Lua slot first and the name second. A name is never authoritative: slot and suffix
disagree on 6 layers, the four `_ruined` of OilJob008 and `vz_state_pirjob002_01_captured`,
`vz_state_gurjob005_captured`. Name case never matters: the hash folds it and `mrxlayermanager.lua`
lowers every name.

| Role | Layers | Entities | Rule | What it holds (examples measured) |
|---|---:|---:|---|---|
| Terrain | 2 | 800 | exact names | `vz_terrain` hi-res cells, `vz_lowresterrain` low-res tiles, 400 each |
| Level root | 3 | 58 | `<level>_base` | `vz_base`; `english_base`, `loading_base` are empty |
| Area | 24 | 41,709 | remaining base layers | `vz_car_city` 5,848, the largest layer |
| Job sites | 82 | 10,978 | `vz_<fac>_job/con…` | always-loaded layers named for a job, `vz_chi_job006_c` 403 |
| Faction sites | 6 | 2,107 | exact names | `vz_pir_hq`, `vz_gua_vzbase`, `vz_sol_base` |
| Roads | 15 | 4,923 | `_road`, `_roads` | Road, RoadIntersection, LaneData |
| Scrub | 9 | 1,123 | `_scrub` | ScrubObject placements |
| Foliage | 9 | 145 | `_foilage` | 6 of 9 are empty; the game spells it so |
| Far stand-ins | 10 | 438 | `_tiny` | TinyGeometryObject |
| Billboards | 7 | 414 | `_billboard` | MassiveComponent props |
| Regions | 4 | 178 | exact names | `vz_atmosphere_regions`, `vz_boundary_regions`, `vz_poi_lineregions`, `vz_sfx_ambience` |
| Locations | 4 | 239 | exact names | `vz_commonlocations` 141, `vz_landingzone` 71, `vz_teleporter` 17, `vz_start_location` 10 |
| Sound | 1 | 187 | exact name | `vz_watersounds` |
| Test | 3 | 133 | exact names | `vz_rover_camera_test`, `vz_vehiclehandlingtest`, `vz_hidden` |
| Pristine | 132 | 4,300 | `_pristine` | the buildings a job can remove, with their stand-ins; `chijob006_c` 8 buildings, 3 stand-ins |
| Staging | 127 | 9,468 | `_staging`, `_staging2`, `staging_` | site dressing, pickups, gate props; on from New game, not only during a mission |
| Defenses | 84 | 3,635 | `_defenses` | defenders, flags, emplacements. None is on at New game; a job adds it |
| Destroyed | 67 | 562 | slot, or `_destroyed`, `_ruined` | ruins at the pristine positions; `vz_state_mar_city_ruined` adds 68 fire effects |
| Captured | 19 | 1,010 | slot, or `_captured` | the site re-flagged for its new owner |
| Hostiles | 3 | 67 | `_hostiles` | ChiCon002 attackers |
| Act | 37 | 1,600 | `_act1..3` | story-wide state of an area; many hold only LineRegion and Path |
| Aftermath | 7 | 571 | `_post`, `nuked`, `_epilogue`, `_crash` | after a mission |
| Free roam | 18 | 4,496 | fuel, HQ structures, `cashpickups`, `munitions_freeplay`, `big_lineregion`, life missions | not tied to one mission |
| Mission | 65 | 6,505 | `vz_state_<missionid>` | the default `tLayers` of a mission, `mrxmissionflow.lua:249` |
| Mission part | 147 | 3,592 | `<missionid>_…` | a catch-all: the mission is known, the role is not |
| Difficulty | 16 | 365 | `_easy`, `_med`, `_hard`, `_basicN`, `_interN`, `_advN` | variants; the rule is the name only |
| Co-op split | 7 | 107 | `_mp`, `_sp`, `_coop`, `_mpdeliverables` | |
| Interior | 5 | 182 | `pmcinterior` | |
| Cinematic | 1 | 600 | exact name | `vz_state_cinematic_vz` |
| Stand-in pair | 3 | 7 | `_tg` | three `_pristine_tg` layers |
| Residue | 8 | 800 | `recyclebin`, `old`, `temp`, `playtest`, `spawnerspam` | author leftovers; `vz_state_pmccon001old` 532 |

Every layer lands in a row, but two rows are weak. Mission part (147) says which mission and not what
the layer does, so its role stays unknown. `vz_state_temp_staging_gurcon002` (39) is in Residue on its
`temp` token alone. Free roam (18) is a list of names, not a rule. The classifier is data: a rule table
in `crates/mercs2_workshop/data/`, with a per-layer override column the user can edit.

**Lifecycle**, from `xQ!L.lua`, `wifmissionflow.lua` and every Lua literal:

| Lifecycle | Layers | Entities |
|---|---:|---:|
| Static list | 170 | 60,737 |
| Level root | 1 | 58 |
| New game (default dynamic) | 246 | 11,963 |
| Story switch | 96 | 9,044 |
| Mission script, built name, or `Vz_State_<id>` default | 304 | 13,965 |
| Block 29, no Lua request found | 4 | 2,505 |
| No Lua request found | 104 | 3,027 |

"No Lua request found" means no literal or built name in the Lua. It does not mean the layer never
loads; the block-29 row proves that. None of the 104 is in either save. The label stays that literal.

**Place** is by name token, else the nearest of 12 POI anchors to the layer's centroid. It is a
grouping aid, not game data: 925 layers fall into 14 places and "No entities" (5).

## World-state model

A **world state** is a named set of layers, nothing more: `{level, on: [layer…]}`. Sources:

1. **New game**: `_tStaticLayers`, `<level>_base`, `_tDefaultDynamicLayers`, parsed from the level's master script.
2. **A save**: `SaveState.layers` plus the static set.
3. **A step of a site arc**: the slots of one subject.
4. **A story switch**: the `fConseq` deltas of `wifmissionflow.lua`, applied in order to a start state.
5. **A saved state**: the user's own, stored in the workspace. It is a workshop file, never a contribution.

The order of story switches is not linear: AllCon and ChiCon are branches, as the two saves show.
A scrub applies one chosen path and says so. Applying a state in the viewport loads no game code; it
loads the same block readers `LayerView` uses, one view per layer, into one scene.

Precedence between states follows retail: `RuntimeLayerId` is set-or-overwrite, so a key two dynamic
layers place belongs to the last loaded. No retail key is in two layers (101,299 distinct); a Shipment
key that collides is M0198.

## Boards

### World map (`LevelMap`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Map | The game's `vz_lrterrain` texture, north up, with the mapping above. Drag pans, wheel zooms to the 256 m cell. Hover reads cell, x, z and the count there | block 3121 entry `0xA007B5B9`, `texture` decode | — | a world-space 2D canvas in `gui`; a level with another size reads its own `<level>_lowresterrain` extent | test: all 42 POI inside their `LineRegion`s on the map; capture |
| Outliner | Same tree on every Level board. Row click selects; eye shows or hides the layer in the view, without changing the state; a row off in this state reads dim | catalog | — | catalog in the workshop | test: 925 rows, every layer in one group per grouping |
| Group by | Place, Role, Mission, Lifecycle | taxonomy | `settings.json` | — | test: taxonomy counts above |
| New layer | Starts an `add_layer` in the open Depot draft, asking name and template | `quartermaster.rs:1206` | Depot item | Depot | test: seeded contribution |
| Layer bounds | Rectangles of the selection, from resolved positions. The board draws 2–98 percentile stored positions as a stand-in | `layerview` positions | — | catalog `bounds_xz` uses stored Transforms, so a layer with link children reads wrong: `vz_mar_city` −2578..3060 | test: `vz_state_mar_city_pristine` holds no origin point |
| Density lens | Entities per 256 m cell in this state. Amber over 32 (a chained block), red over 320 | cell = `(x + 4096) / 256`, `hibernation_grid.md` §2 | — | act1end: 791 cells used, 448 over 32, 51 over 320, fullest 732 at (4, 18) Cumana. These counts hold link children at their stored Transforms; the tool resolves them and leaves attached children out of the grid | test: Σ against a capture's 2,130 chained blocks |
| Awake lens | Pick a viewer point; each entity shows awake, or asleep with its radius | `hibernation_grid.md` §2–§4, `mercs2_formats::hibernation_control` | — | offline compute of radius | test: on the capture's viewer, 2,214 awake of 78,163 rows |
| Script markers lens | Marks entities Lua names, with the mission | literals; `mercs2_autoplay` NDJSON for names built at run time; waypoints come from Lua (`MrxTaskObjective` line 428), not layers | — | the join table | test: 492 of 567 literals place |
| Grids | Toggles the 400 m terrain, 256 m hibernation and 200 m stand-in grids | `docs/terrain_cell_format.md`, manifest `add_tiny_geometry` | `settings.json` | — | capture |
| Place card | Layers, base and overlay counts, on in this state, every state, role chips. A chip filters the outliner. **Open layers** goes to Role grouping with the place filtered; **Show in 3D** switches mode | catalog, state set | — | — | test: Maracaibo 134, 58 on at act1end |
| Fullest cells | Sorted list; click frames the cell | density | — | — | test |
| Map · 3D · Plan | Same selection and state in three views | `layerview` | — | Plan is new: entities as marks in x, z | capture of each |

### Layers and the viewport (`LevelLayers`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Outliner by Role | Group row shows layers · entities and expands | catalog | — | — | test: counts |
| Viewport | One or many layers in the world state, using today's view: models with link children, markers by kind, kind labels | `layerview::{LayerView, overlay, pick}` | — | several `LayerView`s in one scene; a shared budget for `step` | `every_layer_reads_and_every_link_is_a_rotation`; capture of act1end Maracaibo |
| Move, Rotate, Select | Q W E. Gizmo in world axes. On a world entity it edits Transform; on a link child it edits the link offset; the chip says which | `layerview::LayerEntity::link` | Depot draft | gizmo; undo stack in the draft | test: frame chosen by `link` |
| Drop to ground | Sets Y from terrain under the entity | `mercs2_formats::terrainmesh` | Depot draft | ray against the cell; retail entities do not follow terrain | test on one cell |
| Parent awake | Toggles the two positions of a link child | `entity_world`, `resting_world` | — | — | `linked_entities_sit_where_retail_puts_them` |
| Show flags | Models, Markers, Link lines, Regions, Roads, Terrain | `MARKER_KINDS`; regions in local XZ plus the entity's X, Z, no rotation; paths through the transform | — | region and path drawing | test: one region, one path against §2.6 |
| Entity table | Filter by kind, Linked; click selects and frames | `layerview::inspector` | — | moves from inspector to a bottom panel, height by drag | `pick` test; `--ui-state` `entity` capture |
| Layer card | Role, lifecycle, block and how many layers share it, who loads and removes it with file:line | catalog, Lua literals | — | — | test: `vz_state_mar_city_pristine` cites `xQ!L.lua:175`, `wifmissionflow.lua:338`, `chicon002.lua:13` |
| Entity card | Name, frame, parent (click selects it), hard point | `EntityLink`, `physical_link` | — | hard point names from `index.names` | capture |
| Link offset fields | x, y, z of the `Position` blob, and quaternion. Light `0x0010D532`: (1, −4, 0), hard point `hp_fx_light` `0x125C0227`, on pole `0x00102868`. Stored Transform shows read-only | `physical_link::placed_links_in_order` | Depot draft | P1. Until it is approved the change is held and the item cannot be Ready | test: fields read block 594's record |
| Why asleep or awake | In the grid or following a parent; `Distance` own or template; wake radius `1.2 × Distance`, sleep at radius + 7 m; viewer distance. Pole: 120 → 144 m, 151 m; viewer 435.4 m | `hibernation_control`, capture viewer (2350.1, 1491.8) | — | — | test: radius rule on capture rows |
| Used by | Count of script literals naming the entity; its swap partner | literals, swap join | — | an index in `lua_source.rs` | test: `ChiJob006_C_Objective0` found in `alljob020.lua` |
| A name Lua uses | Renaming or removing it is refused in the draft with the file:line | literals | — | — | test |

### World state (`LevelState`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| World states list | New game and every save, with layers · entities. Click applies it to the page | `save::SaveState` | — | — | test: counts above |
| Site arcs | 124 subjects from the Lua slots. Click opens the arc | slot extraction from `lua_source` | — | a slot index; 24 files today | test: 378 slot names |
| Step bar | Four steps. New game, job or contract running, target down or taken (marks queued, nothing moved), marks applied | see each step's cited lines | — | the call that applies the marks for a given job is INFERRED: `wifmissionflow.lua:1448` among 7 callers | U6 |
| Plan view | Entities of the layers on, in plan, by role | `layer` positions | — | — | test: entity counts per step |
| This step | Each layer with on, marked for removal, marked for addition | slots | — | — | test against `mrxtaskjobdestroyset.lua:49-59` |
| Both saves | Whether a save's list equals this step: ChiJob006_C and AllJob001_03 both equal step 4 in both saves | `SaveState.layers` | — | — | test on both profiles |
| Story switches | 16 missions with removed, added counts. Click lists the names and applies them in order | `wifmissionflow.lua` | — | branch handling | test: PmcCon003 −5, +65 |
| Save state as… | Dialog: name; stores the current set | workspace | `states/<name>.json` | — | test: round trip |
| Compare steps | Opens Compare with these two sets | — | — | — | capture |

ChiJob006_C reads New game = Pristine 11 + Staging 98, running adds Defenses 172, after = Destroyed 11.
AllJob001_03 keeps Pristine after capture, because `mrxtaskcontractoutpost.lua:153-160` removes only
Staging and Defenses. The two arcs differ, so the arc is read from the script's class, not assumed.

### Compare (`LevelCompare`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| A, B pickers | Any layer, world state or save | catalog | — | — | test |
| Suggested | Pairs from arcs and saves | swap join | — | — | test: 42 and 19 pairs |
| Plan | Swap, only A, only B | position join within 5 cm; XZY distance | — | layer-level comparison lists layers instead when A and B are states | test: 11 of 11, 51 of 76, 130, 3 of 3 |
| Swaps card | From → to template with counts | join | — | — | test: `_vzoutpost_walla` → `_AL` 28 |
| Only in B | Top templates; Maracaibo's ruined adds 31, 22 and 15 fire effects | join | — | — | test |
| Move swap partners together | On by default. A move of one entity moves its partners in the same draft | join | Depot draft | INFERRED intent: the pairing is by position only, so the partner list is shown and can be cut | test: a draft holds both edits |
| Copy list | Tab-separated rows | — | clipboard | — | test |
| Open A and B | Both layers on in the view | — | — | — | capture |

### Checks and budgets (`LevelChecks`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Re-check | Runs every check for the state and draft | below | — | — | test |
| My draft checks | Claims (layers, blocks, block neighbours), swap partners, `aset_refcheck`, M0198, M0194, M0246, keys in range, name in use | `qm lint`, `qm preflight` | — | claim footprint from block index | fixtures per rule |
| Names with no layer | 44, with the file naming each | Lua literals, catalog | — | — | test: 44, 30 `_tg` |
| Layers with no Lua request | 104; click filters the outliner | Lifecycle | — | — | test |
| Slot and suffix disagree | 6 | slots | — | — | test |
| Template models in no archive | 293 of 2,027 | `load_model`, `wad::aset_types` | — | run over all templates | test |
| Links on a child twice | 7; last wins | `placed_links_in_order` | — | — | `layer --frames` |
| Grid blocks | 3,081 of 4,000 at act1end: 1,024 cells + Σ(⌈n / 32⌉ − 1). The bar stays INFERRED, and the card shows the chain-only reading, 2,057 | grid | — | U5 | Σ against the capture, 2,137 and 2,130 |
| Far stand-in slot lists | 1,208 of 1,400 | M0246 | — | — | M0246 fixtures |
| Same-model draws | Two proven runs, no ceiling drawn past them | memory | — | — | — |
| Pools | Bar is the capture's count; the tick is the presize, and a bar may pass it. Columns State, Capture, Presize. State shows own placed records; the pool also holds template and runtime rows | `docs/game_config/cdbsizes.ini`, `dump-containers` | — | captures bundled as numbers in the release: `index::data_home()` | test: `Name` passes its tick |
| Add my draft | Adds the draft's entities to State and to cell density | draft | — | — | test |

Cdbsizes rows are never drawn as limits. A modder who sees `Name` at 12 × presize learns it grows;
what ends a run is the grid pool, the stand-in lists and draw load.

### Depot draft (`LevelDraft`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Depot list | Draft and Ready items in the one workspace folder | Depot, Settings | — | all of it; `workshop-depot-ia-decisions` | Depot's own contract |
| World edit item | Every edit on Level lands in the open item. A viewport edit with no item open asks for a name once | — | item folder | item kind "World edit" | test |
| Change table | One row per change, grouped by entity. Click opens it in Level, framed | draft | — | — | test |
| Undo, Redo | Draft history | — | item folder | — | test |
| Lowering | Moves → `edit_world` per layer; adds to a layer → `add_placement`; a new layer → `add_layer` + `activate_layer`; a stand-in → `add_tiny_geometry`. Layer named exactly | `quartermaster.rs:1130-1206`; `layer_block_for_edit` gives way to exact lookup, user-approved | `manifest.yaml`, `src/*.yaml` of the item | `edit_world` extract-then-edit stays the base: `qm extract-world` | `edit_world_builds_an_overlay_that_moves_an_entity` |
| Manifest preview | The YAML written | — | — | — | test: `qm lint` reads it |
| Check chain | `qm lint`, `qm build`, `aset_refcheck`, `qm preflight`, then `qm link` in a Shipment | `quartermaster.rs`, `qm` | `_build/` | — | exit codes, sha256 of the overlay |
| Claims | Block, layer, why. A block-29 layer shows 173 layers and 7.97 MB, and offers **Put it in a new layer** | block index | — | whether M0207 fires for two different layers of block 29 is unverified | U9 |
| Switch warning | `activate_layer` runs at PMC entry each session. **Switch with the job…** writes a `patch_lua` append for the mission | `manifest_format.md` `activate_layer`, `patch_lua` | item | — | test |
| Mark Ready | Enabled when the chain has no error and no change is held | state machine | item state | — | test |
| Copy to Shipment | Copies the item into that Shipment's `src/` and merges contributions | IA decision | Shipment | — | test |

New entity keys are ids, not hashes. An item takes a range `0x0F000000 + (item number << 12)`, so
`0x0F00A001` is item 10's first entity, the shape the manifest documentation already uses. That range
sits above DLC01's `0x00156ABB` and below retail's `0x10000000` run, clear of runtime `0x4…` keys.
M0198 checks only the target block, so the Depot also checks across every layer and its own items.
Layer, model and template names are real names hashed by `pandemic_hash_m2`.

### New level (`LevelNew`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Levels list | Each level with its layer or block count. `dlc01` and `dlctest01` read as block lists until a package is opened | `dlc_port --list-blocks` | — | a workshop reader for a level WAD | test: 1,467 and 178 |
| Name | Derives `data/<name>.wad`, script `<name>`, `<name>_base`. Refuses a clash with any known level | `levelbootstrap.lua:14-15` | Depot item | — | test |
| Start from | Empty 20 × 20, or cut a rectangle of `vz` into layers by cell | terrain grid | Depot item | the cut is a copy of cells and entities; collision reuse is UNPROVEN | U7 |
| Slot table | vz's layer, the DLC's stems and the new name for each slot. Rows for a state add `_atmofx`, `_pathfinding`, `_spawns`, stems the DLC uses | `dlc_blocks`, `dlc01.lua` | — | DLC layer contents are unread | test against the stems |
| Write master script | Writes `<name>.lua` with `_tStaticLayers` and `_tDefaultDynamicLayers`, from the dlc01 shape | `add_script` | item | — | `qm lint`, exact Lua compile |
| Make Depot item | Stores the level as an item | Depot | item folder | Depot | test |
| Needs list | Nine rows with grades, in the order they fail | below | — | — | each row's test |

| Need | Grade | Test |
|---|---|---|
| Boot by name, `Sys.StartSingleplayer(level, master)` | PROVEN live for dlc01 | — |
| `<level>_base` layer, master script asset | PROVEN | — |
| Static and default lists | PROVEN, `dlc01.lua` 11 and 3 | — |
| Self-contained WAD | wedge seen: `dlc01.wad` alone spins on "1 outstanding asset" | merge base globals into the level WAD, boot |
| Terrain cells load | UNPROVEN: no edited cell has been run | U7 |
| Collider loads | UNPROVEN | U7 |
| Road graph | built at run time from Road, RoadIntersection, LaneData, `path` | one road on a test level, a traffic spawner |
| AI ground navigation | undecoded; no nav asset; DLC ships `_pathfinding` stems | read `dlc01_state_dlccon003_pathfinding` |
| Delivery | needs P5 | — |

## How a modder builds

- **A change to a site**: Level › Role › Staging › open a layer › move. The swap partner moves along.
  Depot draft lowers to `edit_world`. Scrub the arc to see it at each step.
- **A new outpost**: Level › New layer, template a small `vz_state_*` layer, not block 29 › place
  models. Add Defenses and Captured as sibling new layers and bind them to a mission's slots in a
  `patch_lua`, or switch with `activate_layer`. Within the current kinds this covers model-only props;
  a soldier or spawner needs P3.
- **A new mission's world**: Missions blueprint and a Level draft share layer names; a layer name in a
  blueprint slot reads as a link to Level.
- **A far stand-in**: select a cell in a layer, `add_tiny_geometry` from `qm extract-tiny`.
- **A new level**: `LevelNew`. Blocked at the rows graded above; each row names its test.

## Borrowed and new

Borrowed, by name, from Unreal Engine: the **World Partition** minimap (world map), **Data Layers**
with **Initial Runtime State** (Lifecycle facet, New game as initial state; Unreal has Unloaded,
Loaded, Activated, this game has off and on, with a marked pending state), **One File Per Actor** (a
draft's per-layer edits, so two people touch one file only when they touch one layer), **Level
Snapshots** (saved states, of layer sets rather than properties), the **Reference Viewer** and
**Size Map** (Used by, Claims), and the **World Outliner**.

New here, and possible only because of how this game is built:

1. **Any save is a world state.** A player's own `.profile` opens in the editor as the exact layer set.
2. **Arcs read from the script.** The step bar is the game's own Lua slots, not an authored timeline.
3. **Swap pairs.** Editing a pristine building moves its ruin; the join finds a relation the data never states.
4. **Why it is awake.** Each entity answers with the rule retail runs: its radius, its viewer, or the parent it follows.
5. **Honest budgets.** Only proven ceilings end a bar. Presizes are ticks that a bar passes, and per-cell density is the budget that matters.
6. **Script-safe edits.** A name Lua uses cannot be renamed by accident, and 44 dead names are visible.
7. **Graded facts in the tool.** A value from a capture, a rule, or a guess says which.

## Manifest proposals

Approved by the user (2026-11-01): P1, P2, P3, P4a, P4b, P5. The order they land in is
[`../blow_it_up_again.md`](../blow_it_up_again.md).

| # | Change | Enables | Without it | Touches |
|---|---|---|---|---|
| P1 | `edit_world` edit (`world.rs` `EntityEdit`) gains `link: {pos, quat}` for a placed `PhysicalLink` record | Moving a link child at all. 679 to 882 children per capture | Child moves are held; `raw` can ship the block | `crates/mercs2_quartermaster/src/world.rs`, the `edit_world` arm of `build.rs`, a `patch_link` beside `placement::patch_transform` (the record is fixed-size, so the in-place overlay holds); a released `qm` for Modkit |
| P2 | `edit_world` edit gains `remove: true` | Deleting a placed entity | Swap the model, or move it out of play | Not an in-place patch, which is all `edit_world` does today: the overlay overwrites words and `Placement` cannot round-trip a rebuild. A COMP rebuild, unsurveyed |
| P3 | `add_placement` / `add_layer` record (`world.rs` `EntityRecord`) gains `template:`, and `model` becomes optional | Soldiers, spawners, vehicles, regions placed by template. Today `key`, `model`, `pos` only | Spawn by `patch_lua` with `Pg.Spawn`; props only by layer | `placement_build::NewEntity` has no parent and `transform_record` writes the `Parent` word as 0; `modelname_data` writes a record per entity |
| P4a | `edit_world` edit gains a component field: `HibernationControl.Distance` | Wake distance edited in place | `raw` with `touches` | `world.rs`, a per-class patch; an entity without its own record reads its template's, so the edit may have no record to patch |
| P4b | New same-hash kinds for the `lineregion` and `path` assets | Region polygons and patrol paths edited | `raw` with `touches` | These are assets an entity names by hash, not entity fields, so `edit_world` is the wrong home. `build.rs` `opaque_new_asset` is the shape `replace_terrain_cell` uses |
| P5 | `add_level` kind with destination `data_wad` | A new level as a Shipment | A hand-built WAD; `data_wad` exists for `add_language`, and `place_file` has no data destination (`manifest.rs` `PlaceIn`) | `build.rs`, Modkit |

Terrain sculpting needs no new kind: `replace_terrain_cell` (`manifest.rs`, in `qm kinds`, absent from
`manifest_format.md`'s kinds table) ships a pre-encoded cell, and `mercs2_formats::terrainmesh`
(`TerrainCell::displace`, `rebuild_collision`, `encode`) makes one. U7 stays the test.

What needs no manifest change: every read-only board; moving, rotating, re-modelling world entities;
adding model-only entities and new layers; stand-ins; switching layers; saved states.

## Unproven, and the test for each

| # | Question | Test |
|---|---|---|
| U1 | Does editing a child's Transform have any effect? | `edit_world` on `0x0010D532`, capture, `layer --frames` |
| U2 | Does an edited link offset move the child? | P1 or a `raw` block on the same entity, capture |
| U3 | Rotation of link children | `layer --frames` extended to quaternions |
| U4 | Do moved buildings confuse free-moving AI? | Move a building on a patrol route; what AI navigates on is undecoded |
| U5 | Do the 1,024 cell blocks count against the 4,000? | Read the free-list cursor (`FUN_005161E0`) with `dump-read` on a capture |
| U6 | Which call applies a job's marks | `lua_trace_asi` through one job |
| U7 | Does the game accept an edited terrain cell and collider? | The HQ pyramid cell in-game |
| U8 | Why are unrequested block-29 layers live? | Break on the block's load in a session the user drives |
| U9 | Does M0207 see two different layers of block 29? | `qm link` over two Shipments |
| U10 | Does `Pg.ReloadLayer` destroy old entities first? | Live, `layer_load_code_map.md` §3 |

## Conflicts found in the record

- `layer_load_code_map.md` §1: "the other 4 in block 29 are never requested" is true of the Lua, but three carry live entities.
- `format_reference.md` §13.4.1 gives the map's U unmirrored; the POI plot needs it mirrored.
- `manifest_format.md` M0194 calls layer names case-sensitive; `mrxlayermanager.lua` lowers them and the hash folds them.
- The workshop's row match is case-sensitive (`app.rs:3025`). The Level page folds case.
- The catalog's `bounds_xz` uses stored Transforms.

## Order of work

Each step is one-file commits, proven before the next starts.

1. **Catalog** in `mercs2_formats`; rule table; Lifecycle from master script and Lua; state sets. Test: 925 rows, 101,299 entities, the counts here.
2. **Rail and page**: `Workbench::Level`, Inspect tab out, `--ui-state` case fold, outliner with today's single-layer view.
3. **World map**: `vz_lrterrain`, grids, density, bounds from resolved positions.
4. **Many layers in one view** and the state picker. Test: act1end entity count.
5. **World state** board: arcs, step bar, story switches.
6. **Compare** and swap pairs.
7. **Checks and budgets**; U5 before the grid bar loses its grade.
8. **Depot** (its own contract), **World edit** item, exact `layer_by_name` lookup in place of the needle.
9. **Editing**: gizmo, drafts, lowering, Mark Ready. U1 and U2 before child editing is offered.
10. **Awake and script lenses.**
11. **Proposals**, one at a time, each with a released `qm`, in the order of
    [`../blow_it_up_again.md`](../blow_it_up_again.md).
12. **New level**, after the self-contained WAD boots and U7 has a result.
