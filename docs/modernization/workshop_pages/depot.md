# Depot — behaviour contract

The design is the "Depot" row of the Workshop Polish Pass canvas: `DepotBrowser` (All items),
`DepotCopy` (Copy to Shipment), `ShipmentCompose` (New item), `DepotLua` (Lua edit · spawn lists),
`LevelDraft` (World edit draft), `ShipmentEdit`, `Bench`, `BenchFit`, `ShipmentReskin`,
`ShipmentRawKind`, `ShipmentDestruction`, `ShipmentFallback` (the item editors), `SettingsDepot`
(Settings › Depot workspace), the shared `DepotNav`, and the rail `RailLevel`.

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
what is missing, and what proves it. Grades: PROVEN (cited to code or measured), INFERRED, UNPROVEN.

## Decisions the page stands on

| # | Decision | By |
|---|---|---|
| D1 | **Inspect** is the base game, read-only. **Depot** is the modder's own items. **Shipments** are packages made of copies of Depot items | user, 2026-10-21 |
| D2 | An item holds what a Shipment can hold: any kind `qm` knows (`Contribution::ALL_KINDS`). Nothing else is drawn or offered | user, 2026-10-21, 2026-11-04 |
| D3 | An item is **Draft** or **Ready** | user, 2026-10-21 |
| D4 | Every item lives in **one workspace folder**, set in Settings | user, 2026-10-21 |
| D5 | **Any state** can be copied into **any Shipment**: the mechanism is "copy to shipment". The copy goes into that Shipment's `src/` and adds the item's contributions to its manifest, so the manifest spec is untouched | user, 2026-11-03 |
| D6 | The per-kind editors and the Benches are modes of one Depot item, not rail places | user, 2026-10-21, 2026-11-03 |
| D7 | Creation is right-click **New ▸**, mirrored by a visible button | user, 2026-10-21 |
| D8 | The workshop validates and hands to Modkit; it never installs | user, 2026-10-21 |
| D9 | The rail reads INSPECT, LEVEL, SAVES, DRIVING, MISSIONS, SYSTEMS, DEPOT, SHIPMENT, SETTINGS | user, 2026-11-01 (`level_editor.md`) |
| D10 | Main use cases: Lua script edits, placement-layer adjustments, state-machine changes, spawn-table tweaks. A spawn table is a `patch_lua` on a mission script's `Ai.TweakAttachedSpawners*` calls | user, 2026-11-03 |

## On disk

```
<workspace>/                       Settings › Depot workspace
  <folder>/…                       folders the modder makes (characters, missions, …)
    <item>/                        an item: any folder holding item.yaml
      item.yaml
      src/…                        the item's own files
```

`item.yaml`:

```yaml
state: draft            # draft | ready
contributions:          # the manifest's own form; paths are relative to the item folder
  - kind: set_spawn_lists
    target: gurcon002
    lists:
      - function: SetupDangerousObjBuildings
        building: TallCommBuild
        group: Rooftop
        list: Spawnlist (VZ Elite)
copies:                 # written by Copy to Shipment, one row per copy
  - shipment: /path/to/my-first-mod
    at: 2026-11-15T09:41:00Z
    sha256: <the item's content hash at the copy>
```

- An item's **name** is its folder name. Two items with the same name anywhere in the workspace are an
  error naming both paths: their `src/` files would collide in any Shipment that takes both.
- A folder without `item.yaml` is a plain folder. A folder inside an item is part of the item.
- An `item.yaml` that does not parse is listed as a broken item with the parse error, never skipped.
- Every write of an existing file the Depot edits (`item.yaml`, a world, effect or `states:` file) and
  of a Shipment's manifest rewrites only what the edit changed, in the file's own format
  (`mercs2_quartermaster::lossless::rewrite`): the author's comments, spacing and spellings stay, a
  comment directly above a list item moves and goes with it, and an added item is set off by a blank
  line when the list's items are. The result must read back as the edited value or nothing is
  written. In JSON, a list or object that gains, loses or reorders members keeps the file's layout:
  its line breaks and indent step, or all on one line.
- `contributions` reuses `mercs2_quartermaster::manifest::Contribution`, so an item is checked by the
  same `qm lint` a Shipment is (below).
- The **content hash** is sha256 over `item.yaml`'s `state` + `contributions` and every file under
  the item's `src/`, in path order. It decides whether a copy is current.

## Rail and page

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Rail **DEPOT** | Opens the Depot. Glyph from `RailLevel` | `app.rs` `Workbench::ALL`, `gui::theme::RailIcon` | — | `Workbench::Depot`, `RailIcon::Depot` | rail-order test in `app.rs` |
| Rail **LEVEL** | Comes with the Level page (`level_editor.md` step 2). Until then DEPOT is the sixth entry | — | — | `Workbench::Level` | same test |
| Page with no workspace set | Centre says the Depot has no workspace and offers **Choose folder…** and **Open Settings** | `Settings.depot_dir` | `settings.json` | — | test |
| Workspace set but missing or not a folder | Same, red, with the path and the OS error | `settings::check_depot_dir` | — | — | test |

## Navigator (`DepotNav`)

The left panel of every Depot board.

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Header | **DEPOT**; inside an item, a back arrow to All items | — | — | — | — |
| New ▸ | The New item dialog (below) | — | — | — | — |
| Filter | Narrows rows by name, kind or target, case-folded | items | — | — | test |
| All · Draft · Ready | One chip on at a time, with counts | items | — | — | test |
| Rows | Grouped DRAFT then READY. A row: the kind's title (`quartermaster::kind_title`), the kind, the name, and one note line: the worst check finding (code and title), else where it was copied, else "in no Shipment". Left stripe: red error, amber held or warning, green clean | items, checks | — | — | test |
| Row click | Opens the item | — | — | — | — |
| Footer | The workspace path | settings | — | — | — |

## All items (`DepotBrowser`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Folders | All folders, then each top-level folder with its item count. Click narrows | scan | — | — | test |
| State | Draft, Ready checkboxes with counts | items | — | — | test |
| Kind | One checkbox per kind present, with counts. **Every kind qm knows (n)** lists `ALL_KINDS` | items, `ALL_KINDS` | — | — | test |
| In a Shipment | In none, then each Shipment copied into, with counts. A copy is counted only while that Shipment still holds it (see Copy) | `copies` | — | — | test |
| Search | Name, kind or target | items | — | — | test |
| Re-check all | Re-runs every item's checks | checks | — | — | — |
| Sort | Modified (newest first) or Name | folder mtimes | — | — | test |
| Tiles · List | Two views of the same rows | — | `settings.json` | — | — |
| Tile | Thumbnail, Draft/Ready chip, check dot, name, kind, folder and where it is | items | — | thumbnails (below) | capture |
| Thumbnail | A model kind renders its own model; a kind that targets a shipped model renders that model; an animation plays on its rig; other kinds draw the kind's glyph | `shot` renderer, shared, one device | — | a shared offscreen renderer; `shot::HeroRenderer` owns a device per instance | capture |
| Selected item | Kind title and kind, state chip, name, folder and modified time; **Target**; **Writes into manifest.yaml** (the contributions as YAML) and the files; **Checks**; **In Shipments** | item | — | — | test |
| Open | Opens the item's editor (below) | — | — | — | — |
| Mark Ready | Draft → Ready. Disabled while a check has an error | checks | `item.yaml` | — | test |
| Back to Draft | Ready → Draft | — | `item.yaml` | — | test |
| Copy to Shipment ▸ | The Copy dialog | — | — | — | — |
| Context menu | New ▸, Open, Reveal folder, Duplicate, Rename, Delete (to the OS trash) | — | item folder | — | test |
| Footer | workspace · items · draft · ready · with errors | items | — | — | test |

## Checks

| Check | What runs | Shown | Proof |
|---|---|---|---|
| qm lint | `mercs2_quartermaster::lint::lint` over a manifest holding the item's contributions, with the item folder as root, so `src/` checks (M0110, M0111, M0112) read the item's own files | count and worst finding; each finding with its code, linked to the rule's page | test with a missing source |
| Lua compile | `mercs2_luac::compile` of every Lua file a `patch_lua`, `replace_lua` or `add_script` names | pass, or the compiler's message | test with a syntax error |
| qm link | Only in a Shipment: "in a Shipment" until the item is copied | — | — |

An item with an error cannot be marked Ready. A Ready item that gains an error stays Ready and shows the
error; it is never silently moved back.

## New item (`ShipmentCompose`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Search kinds | Filters by kind, title or description | `quartermaster::KINDS` | — | — | test |
| Chips | All, World, Characters, Weapons, Driving, Audio, Missions, Systems | `domain::Domain` | — | which kinds each domain owns | test |
| Kind list | Grouped by layer (Data, Script, Code, Any), `KINDS` order | `KINDS` | — | — | test: every `ALL_KINDS` kind listed |
| Detail | Title, description, **You'll be asked for** (the fields with the rule each one satisfies), **Writes into manifest.yaml** (the stub) | `quartermaster::stub` | — | the per-kind "asked for" table | test |
| Name | The item's folder name; refused when an item of that name exists | scan | — | — | test |
| Create | Makes `<folder>/<name>/item.yaml` (Draft, the stub) and `src/`, then opens it | — | item folder | — | test: round trip |

## Copy to Shipment (`DepotCopy`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Shipments list | The modder's Shipments, with versions | see Q1 | — | Q1 | — |
| New Shipment… | Scaffolds one and selects it | `quartermaster::Panel::scaffold` | Shipment folder | — | test |
| Copies into `<shipment>/src/` | Each file of the item's `src/`, marked new, identical (already there, same bytes) or renamed (a different file of that name is there) | the item's files | — | — | test |
| Adds to manifest.yaml | The contributions, with any renamed path rewritten | `mercs2_quartermaster::discover` | — | a `sources_mut` beside `Contribution::sources` to rewrite paths | test |
| Checked against the Shipment | No other contribution writes the same target; no `src/` name clash left; `qm lint` of the Shipment with the copy: 0 errors | `lint::lint` | — | — | test |
| Copy | Writes the files and the manifest, then records the copy in `item.yaml` | the single `src/` writer | Shipment, `item.yaml` | the single writer (below) | test: round trip |

"The item is copied as it is now, Draft or Ready. Later edits in the Depot stay in the Depot until you
copy again." A copy is **current** while the item's content hash equals the one recorded; **out of
date** when it differs; and it is no longer counted once the Shipment no longer has the contribution.

**One writer into `src/`.** Today three writers put files into a Shipment's `src/` and each treats a
name clash differently: `Panel::import_source` suffixes `_1`, `gui::copy_into_src` suffixes `-2` and
falls back to the absolute path when the copy fails, and the Missions "Add to Shipment" overwrites.
The Depot's copy is the one writer: same bytes reuse the file, different bytes get a `_N` suffix, a
failed copy is an error. The other three call it.

## Settings › Depot workspace (`SettingsDepot`)

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Navigator entry | **Depot workspace** · "Where your drafts live", between Shipments library and Shortcuts | `SettingsSection` | — | the variant | test |
| Folder | The path, found or not, with **Choose folder…** and **Reveal folder** | `Settings.depot_dir`, `settings::pick_depot_dir`, `check_depot_dir` | `settings.json` | — | test |
| Summary | items · draft · ready, and the top-level folders | scan | — | — | test |
| One item on disk | The folder layout and `item.yaml` above | — | — | — | — |

Choosing a folder that does not exist is refused; an empty folder is a valid workspace with no items.

## Item editors

Opening an item shows the editor for its kind, with the Depot navigator on the left, the item's name
and state in the header, and **New ▸ · Re-check · Reveal folder · Copy to Shipment ▸ · Mark Ready**
(or **Back to Draft**) in the toolbar. An edit writes the item, never a Shipment.

| Board | Kind | Today | Missing |
|---|---|---|---|
| `DepotLua` Lua edit · spawn lists | `set_spawn_lists` | `mercs2_quartermaster::spawn_lists` reads every call of the target that sets a literal list: `Ai.TweakAttachedSpawnersInGroup(guid, group, {…})` and `Ai.TweakAttachedSpawners(guid, {…})` (all groups). gurcon002 has 15: the 9 `InGroup` calls at lines 530–561 and 6 all-groups calls at 581–614. A row's choices are the lists the `vz.wad` scripts set (6). A choice is an edit of the item's `lists:` rows, written into `item.yaml` in place; choosing the shipped list again takes the row out. A building that is a parameter of the function, or a `local`, a `for` variable or a parameter of a function inside it, is read-only, with the reason. Rows qm refuses (M0317) are listed above the table. The shipped function beside the Lua qm writes for the rows. A `patch_lua` item shows its append, edited by hand | **Preview run ▸** (`missions.md` D4); whether the game runs the wrapper is UNPROVEN until a run |
| `LevelDraft` World edit draft | `edit_world`, `add_placement`, `add_layer`, `activate_layer` | path fields only | the Level page (`level_editor.md` steps 2–9) |
| `ShipmentEdit` Wearable outfit, `ShipmentReskin` | `add_outfit` | form exists | moved onto the item |
| `Bench` Retarget rig | `add_outfit`, `add_model` | writes into the open Shipment and replaces the whole entry with wearer `"mattias"` (`app.rs:7636`) | writes the item; keeps every field the bench does not own |
| `BenchFit` Fit to donor | `add_model` | publishes `vz-mod.wad` beside the base WAD (`app.rs` `mod_out_beside`) and records no contribution | writes an `add_model` item; writes nothing into the game folder (D8) |
| `ShipmentRawKind` Collision swap | `replace_phy2` | Opening the item loads the target model into the viewport. `collision.rs` reads the shipped `PHY2` through the game stack as `qm build` does (`container_for_asset`, `phy2_span_in_container`) and decodes it with `havok::parse_phy2_body`: the destroyer reads 80,212 B, Havok 5.5.0-r1 (56,832 B), 7 `WpMeshShape16` (1,078 tris), 66 hulls, 7 MOPP, 27.6 × 46.2 × 155.8 m. The viewport draws the item's file, or the shipped one, over the model: mesh edges, and each convex shape's projected hull. **Save shipped as .phy2** writes the shipped body into the item and points it there. The form gains Target and PHY2 file | Render mesh off; making a `.phy2` from a model (`phy2_build::build_phy2_multi`) |
| `ShipmentDestruction` Destruction states | `edit_state_machine` | path to `states.yaml` only | the visual editor |
| `ShipmentFallback` New animation · clip preview | `add_animation`, `replace_animation` | `depotanim.rs`: the clip's class, duration, frames and tracks, and M0213's findings; for a replace the shipped clip's too, read from the target's own container as qm's replace reads it (`container_for_asset`; a MANM keyframe animation is refused), and **Start from the shipped clip** writes its packfile, `trnm` and `evnt` into `src/` and points the item at them. **Hero**: every model of the game, ranked by how many of the `trnm`'s bones its skeleton holds (`heroidx`), the best by default, the choice kept per contribution. The viewport plays the clip on the hero through the engine (`ClipAnim::from_sources`, posed by the same `clip_palette` Inspect uses), with Play/Pause, Restart, Loop and a scrub marking each event; a replace's shipped clip plays on a copy of the hero beside it, on the same clock. **Bone binding**: each track's bone and whether the hero holds it. **Events**: the `evnt` file as rows (time, name, category), written with `build_evnt`, refused when the file changed on disk; **+ Add events** creates `<clip>.evnt` and names it | tile thumbnail (no kind has one yet) |

Every other kind opens `contribution_form`, as the Shipment page does today.

## Routes into the Depot

Today seven routes write into the single open Shipment: the asset menus' **Add to Shipment**, Lens
cards, the Text page, the Audio page, the Missions page, the Retarget bench and `--export-shipment`.
Each becomes "new Depot item from this" (or "update the open item"), so every edit starts in the Depot
and a Shipment only receives copies.

## Open

| # | Question | Blocks |
|---|---|---|
| Q1 | `DepotCopy` lists Shipments from "Settings › Shipments library". That folder is Modkit's drop folder (`quartermaster::shipments_library`, not configurable): copying into it edits Modkit's copy, not the modder's source. Where do the modder's own Shipments come from? | the Copy dialog's list |
| Q2 | What "layout" means in D10: placement layout (World edit) or UI layout (`add_ui`) | nothing built yet |

## Order of work

Each step is one-file commits, with tests, proven before the next starts.

1. This contract.
2. `Settings.depot_dir`, the Settings section, and `depot.rs`: scan, `item.yaml` read and write, checks.
3. Rail entry and page shell: `Workbench::Depot`, `RailIcon::Depot`, navigator, empty states.
4. All items: filters, tiles and list, the selected item, Mark Ready, the context menu.
5. New item.
6. Copy to Shipment: the single `src/` writer, `sources_mut`, the dialog, the copy record (after Q1).
7. Routes into the Depot.
8. Item editors: move the existing ones, then build Lua, collision, destruction and animation.
9. Thumbnails through one shared renderer.
