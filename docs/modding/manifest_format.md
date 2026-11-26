# Shipment manifest format

The reference for `manifest.yaml` — the file that describes a Mercenaries 2 mod.

A **Shipment** is a mod package. The **Quartermaster** (`qm`) reads one, checks it, and builds it
into an overlay WAD. This document is the contract between the two; `qm rules` lists every check and
links back here.

Start from the [template repo][template] rather than from a blank file.

Format version: **2**, the only format. A manifest declaring any other `format` — older or newer,
including `1` — is rejected loudly (M0100) rather than parsed optimistically: a field this build does
not understand is a field it would silently drop.

## The file

`manifest.yaml` at the Shipment root. `.yml`, `.json` and `.toml` are also accepted and parse
through the same model, so nothing is expressible in one and not the others. **Exactly one** — two
manifests is an error rather than a precedence puzzle.

YAML is preferred, and it is what `qm` writes a new manifest in, because this file is mostly
prose-adjacent metadata that people read and review. An edit to an existing manifest (the Workshop's
Shipment page) is written in that manifest's own format and rewrites only what the edit changed
(`mercs2_quartermaster::lossless`), so comments, key order and layout stay.

```yaml
format: 2

shipment:
  name: my-shipment          # lowercase, dashes; becomes _build/<name>.wad
  version: 1.0.0             # semver, MAJOR.MINOR.PATCH
  target: retail             # retail | reimpl
  authors: [your-name]
  description: One line about what this does.

load:
  requires:
    - { shipment: lua-bridge, version: "^1.0.0" }

contributions:
  - kind: replace_texture
    target: pmc_hum_mattias_v3_ub
    image: src/skin.png
```

`target` picks the engine: `retail` is the shipped game, `reimpl` the fan-build engine. There is no
value meaning "both" — a Shipment that claims to target both has almost certainly been tested
against neither, and the layers available differ (ASI plugins exist only on retail).

`shipment.name` is a slug and must not be a **reserved name** (M0211): the stem of a DLL no
Shipment may ship — `pmc_bb`, `cruise`, `dxwrapper`, `binkw32` — compared case-insensitively.
`shipment.version` must be semver (M0100).

### `supersedes`

A top-level list of legacy files this Shipment replaces — typically a mod's old loose-file install:

```yaml
supersedes:
  - { dest: on_load, file: 1_Ess.lua }
```

`dest` is one of the `place_file` destination names and `file` a single filename. While any of them
is still in the game folder (matched case-insensitively), `qm build` refuses and `qm preflight` /
`qm link` report **M0208**. qm never deletes the file; remove it (Modkit offers to, with undo) and
build again.

## Names and hashes

Anywhere an existing asset is referenced you may write **either** a name or a bare `0xHHHHHHHH`
hash. A `0x…` reference *is* that hash; anything else is treated as a name and hashed for you.

```yaml
target: pmc_hum_mattias_v3_ub    # preferred
target: "0x6F84F6A3"             # equally valid
```

**A name is preferred, not required.** The engine's hash is one-way, so a hash cannot be turned back
into a name, which makes a manifest full of them hard to read, review or diff. `qm` warns (M0130) and
offers the name when it can reverse one — but the hash works, and the warning never blocks a build.

Requiring names would be a rule the game itself does not follow: the base data ships hashes, and our
name table does not cover every asset. If you are modding something unnamed, the hash is the only
thing you *can* write.

What the linter is actually guarding against is a **wrong** pairing. An earlier draft of this
document paired `ch_veh_boat_destroyer` with `0xE54047D5` — a hash that belongs to
`al_veh_boat_destroyer`. Nothing about it looked wrong. Writing the name, where you have one, is what
makes that class of drift impossible.

## Folder layout

```
my-shipment/
  manifest.yaml     this file
  src/              your .glb / .png / .lua / raw payloads
  _build/           qm output: <name>.wad + .sha256 + build.log + placement.json   (gitignore this)
  README.md         your own description
```

Every path in the manifest is relative to the Shipment root and must resolve **under `src/`**. A path
that escapes the root — through `..` or a symlink — is refused (M0111), because a Shipment has to
mean the same thing on someone else's machine as on yours.

### `placement.json`

Every `qm build` and `qm link` output directory holds one `placement.json`: what each output is,
its size and sha256, and where a deploy step puts it. An output that produced nothing writes
`"placements": []`.

```json
{
  "format": 2,
  "placements": [
    { "name": "my-shipment.wad", "bytes": 81920, "sha256": "…", "destination": { "kind": "overlay" } }
  ]
}
```

`format` is 2. `destination.kind` is one of:

| kind | fields | what the deploy step does |
|---|---|---|
| `overlay` | — | mounts the WAD as a patch overlay on `vz.wad` |
| `game_folder` | `relative` | copies the file to that path under the game folder (the Code layer) |
| `data_wad` | `relative`, `display` | places a new WAD at `relative`, `data/<name>.wad`, for [`add_language`](#add_language); `display` is the label a selector shows |
| `language_patch` | `language`, `relative` | merges the WAD's blocks, with every installed Shipment's for that language, into `data/<language>-patch.wad`, which the engine mounts directly above `data/<language>.wad` (`FUN_004BFEF0`, `"%s\%s-patch.wad"` with the language table's entry) |
| `shell_patch` | — | merges the WAD's blocks, with every installed Shipment's, into `data/shell-patch.wad`, which the engine mounts above `shell.wad` in the front end (`FUN_004BFDA0` with the name `shell`, which `FUN_004C1280` writes at `0x004C12DD`) |
| `level_patch` | `level`, `relative`, `display` | the level's patch WAD, at `data/<level>-patch.wad`. The engine mounts no `vz-patch.wad` in a level, so the deploy step merges every installed Shipment's overlay with `qm link`'s level patch last. `qm link`'s supersedes `qm build`'s, which says only that the level exists |
| `stream_copy` | `from`, `to` | copies the game file `from` to `to`, both relative to the game folder; `bytes` and `sha256` describe `from` as the build read it, and no bytes are in the output directory |
| `data_file` | `relative`, `base_sha256` | replaces the game data file at `relative` with the output file at the same relative path. `relative` is one of `data/shader3.bin` and `data/shader3Low.bin`. `base_sha256` is the sha256 of the original the output was built from (`--original-data`), and `sha256` / `bytes` describe the output file ([Shader stores](#shader-stores-and---original-data)) |

A link output's patch WAD merges last. The kinds are `Destination` in `mercs2_quartermaster`'s
`build.rs`.

## Contribution kinds

| kind | layer | required fields |
|---|---|---|
| `replace_texture` | Data | `target`, `image` |
| `add_texture` | Data | `name`, `image` (`normal_map:` optional) |
| `add_model` | Data | `name`, `model`, `donor` (optional in the schema, required by `qm build`) (`group`, `textures`, `retarget` optional) |
| `add_outfit` | Data + Script (Script only without `model`) | `name`, `slug`, `display`, `wearer` (one hero or a list); `chatter`, `levels` and `briefing_safe` optional ([below](#outfits-are-generated-by-the-link)); `model` optional — omit it to wear a model the game ships, named by `name`; `donor`, `textures`, `retarget`, `single_group` optional, with `model` only |
| `add_sound` | Data + Script | `bank`, `category`, `load_in`, `cues` ([sound cue fields](#sound-cue-fields)) |
| `load_sound` | Script | `in`; any of `soundbank`, `wavebank`, `stream` ([`load_sound`](#load_sound)) |
| `replace_sound_bank` | Data + Script | `bank`, `category`, `cues` (`language` on a `vo_*` bank) |
| `replace_sound_cue` | Data + Script | `bank`, `category`, `cue` (`language` on a `vo_*` bank) |
| `replace_sound_wave` | Data + Script | `wavebank`, `line`, `wave` (`language` on a `vo_*` wavebank) |
| `add_animation` | Data | `name`, `clip`, `trnm` (`events` optional) |
| `add_shader` | Data + Code | `family`, `classes` ([`add_shader`](#add_shader)) |
| `replace_shader` | Data | `target`, `shader` (`shader_low` when the stem has a low record) |
| `replace_animation` | Data | `target`, `clip`, `trnm` (`events` optional) |
| `add_fx` | Data | `name`, `effect`, `template` ([`add_fx`](#add_fx)) |
| `replace_fx` | Data | `target`, `edits` ([`replace_fx`](#replace_fx)) |
| `add_fx_sprite` | Data | `name`, `image` ([`add_fx_sprite`](#add_fx_sprite)) |
| `add_movie` | Data | `name`, `movie` |
| `add_ui` | Data + Script | `name`, `movie` |
| `patch_lua` | Script | `target`, `append` |
| `set_spawn_lists` | Script | `target`, `lists` (each `function`, `building`, `list`; `group` optional) ([`set_spawn_lists`](#set_spawn_lists)) |
| `add_script` | Script | `name`, `source` (`exports` optional) ([`add_script`](#add_script)) |
| `edit_stringdb` | Data | `target`, `strings` |
| `add_language` | Data (new base WAD `data/<name>.wad`) | `name`, `display`, `strings` (`base` optional) |
| `add_level` | Data (`data/<name>-patch.wad` above a copy of `vz.wad`) | `name`, `display` (`streams` or `from`, and `mission`, `details`, `date` optional) ([`add_level`](#add_level)) |
| `add_menu_item` | Script (composed by `qm link`) | `menu`, `label`, `after`, `action` (`help` optional) ([`add_menu_item`](#add_menu_item)) |
| `add_shop_item` | Script | `id`, `name`, `icon`, `shops`; `behaviour` (support) or `equipment_type` (equipment); `levels` optional, default `[vz]` ([`add_shop_item`](#add_shop_item)) |
| `level_asset` | Data (its level's patch; for `level: vz` the overlay, or templates appended to the worldentity) | `level`, `type`, `name`, `rungs` or `from`, `templates`, `edits` ([`level_asset`](#level_asset)) |
| `fetch_package` | Data (downloaded and converted on the player's machine at build; never shipped) | `id`, `format`, `url`, `sha256`, `size` ([`fetch_package`](#fetch_package)) |
| `edit_state_machine` | Data | `target`, `states` |
| `edit_world` | Data | `layer`, `edits` |
| `add_placement` | Data | `layer`, `entity` ([`add_placement` and `add_layer`](#add_placement-and-add_layer)) |
| `add_layer` | Data | `name`, `template`, `entities` ([`add_placement` and `add_layer`](#add_placement-and-add_layer)) |
| `add_tiny_geometry` | Data | `layer`, `cell`, `key`, `objects`, `model` |
| `activate_layer` | Script | `layer` (`replaces:` optional) |
| `native_hook` | Code | `target`, plus a `plugin` or a symbol/detour descriptor, plus `touches` |
| `place_file` | Code | `file`, `dest` |
| `add_runtime_dll` | Code | `dll` |
| `raw` | any | `payload`, `target_layer`, `touches` |

This table covers the kinds documented on this page. **`qm kinds` is the authoritative list** of every
kind the installed `qm` reads (`qm kinds --json` prints `{"format":2,"kinds":[...]}`); a client that
offers kinds should read it rather than keep a copy.

**Removed kinds.** `add_ai_squad_template` and `add_schema` were removed in qm 3.1.0. A manifest that
still uses one fails to parse, with a message naming the kind as removed. `add_ai_squad_template`
shipped bytes under an author-supplied type id and type hash for an asset nobody has
reverse-engineered, so nothing about it could be checked — ship such bytes with `raw` and a declared
`touches`. `add_schema` had nothing to load it: a schema is the column layout of a component table
inside a placement layer, not a standalone asset.

`donor` is **optional on `add_outfit`**: omit it and the build hosts the outfit on the wearer's own
hero model (`pmc_hum_mattias` / `pmc_hum_chris` / `pmc_hum_jen`), validated against the game stack
(Plan 04, resolved Q2); for a list of wearers, on the first listed hero's. The game ships one human
skeleton, identical across its humanoid models, so a model hosted on any hero animates on every hero;
the hosts differ in bone count and materials. Set `donor:` only for a variant host such as
`pmc_hum_mattias_v3`.

On `add_model` `donor:` is optional in the schema and **required by `qm build`** — a prop has no wearer to name its host, and its rig
must match the geometry class, which is not safe to guess (the weight-transfer history shows a wrong
host produces a model rigged to nothing). Supply it explicitly.

**Write a bare hash QUOTED in YAML.** A name is preferred and a bare `0xHHHHHHHH` is legal
anywhere an asset is referenced — but unquoted, YAML reads `0xEB6F1B2D` as the integer
`3949927213` and the manifest fails to load with *"invalid type: integer, expected a string"*.
So `target: "0x6F84F6A3"`, `touches: ["0xE54047D5"]`.

### `edit_state_machine`

Rewrites a destructible's destruction **state machine** — the `SWIT`/`NODE`/`STAT`/`CHDR`/`CEXE`
family that decides which body shows in which damage state, and the Enter/Exit command scripts
(`SHOW`/`HIDE`/`SetState`) each state runs. `target` is the model; `states` is a YAML file.

**Extract, then edit.** Authoring the machine by hand is punishing, so pull the baseline the model
already carries and change that:

```sh
qm extract-states al_veh_boat_destroyer --game <dir> > src/states.yaml
```

The dump reads in names where a hash reverses (states, HIER nodes, commands) and keeps the script
opcodes as plain integers; edit it and point `states:` at it. The whole family is **regenerated** from
your file every build, so you can **add or remove** nodes and states, not only rewrite them — the
writer rebuilds the descriptor table, re-bases the container, and recomputes the CSUM. Re-serialising an
unedited machine (`serialize_state_machine`) reproduces the container byte-for-byte for all 1,311 retail
destructibles.

⚠ **State names are the engine's global vocabulary, not labels you own.** `SetState` /
`SetStateOnMsg` key on a shared set of hashes — `PristineState`, `DamagedState`, `DestroyedState`,
`GoneState`, `InitState`, … — so a state you rename to a novel name (or add with one) is **never
reached by the damage system**: it ships but is dead. **M0193** warns when an edited state is outside
that vocabulary and was not already in the model. The edits that matter are a state's **Enter/Exit
command scripts** (which HIER subtrees to `SHOW`/`HIDE`, which emitters to start) and its switch
slots — change what a state *does*, using state identities the model and the vocabulary already
carry.

The overlay carries the model's **whole LOD chain** and nothing else — no block-mate is shadowed. The
machine lives in the `_P000` container (1,059 of the 1,311 destructibles also have finer `_P001`/`_P002`
rungs holding the near geometry), so the build emits one single-entry block per rung the model's ASET row
names: `_P000` with the edited container, each finer rung with its retail container byte-for-byte. Each
block keeps its source block's INDX tier byte, each entry's third word is restated as the byte sum of the
rungs before it, and the row names the new blocks
([`docs/aset_format.md`](../aset_format.md) §The LOD chain lives in the ASET row). The build fails, naming
the rung, when the stack's chain for the model is shorter than base `vz.wad`'s or a rung block does not
hold the model exactly once. `replace_phy2` carries the chain the same way.

The **permanent, world-scale** counterpart — making a building ruined for the rest of a mission — is
the `vz_state` overlay, a separate mechanism scoped in
[`vz_state_world_overlay_scope.md`](vz_state_world_overlay_scope.md).

### `edit_world`

Moves, rotates or re-models the **placed entities** of a world layer — the permanent, world-scale
counterpart to `edit_state_machine`'s per-model destruction edits. Where `edit_state_machine` changes
what a destructible *becomes*, `edit_world` changes where a thing *is*: the `vz_state` /
`layers_static` placement layers are the `UCFX`→`CHDR`→`COMP` tree of 42-byte `Transform` + `Name` +
`ModelName` records that the world load reads to spawn the level.

`layer` is a **needle** matched against the layer blocks in the game stack (`vz_state_pmccon004`,
`vz_state_pmc`, or just `vz_state` for the first match). `edits` is a `src/`-relative YAML file.

**Extract, then edit.** Author against the baseline the layer already carries:

```sh
qm extract-world vz_state_pmccon004 --game <dir> --names production_names.json > src/world.yaml
```

Each edit names one entity by its **key** (the placement hash, quoted `"0xHHHHHHHH"`) or by its
`Name` where one reverses, and supplies any of `pos: [x,y,z]`, `quat: [x,y,z,w]`, `model: <name>`:

```yaml
edits:
  - entity: "0x8B7DE1F5"   # my_custom_helipad
    pos: [1240.5, 12.0, -880.25]
    quat: [0, 0, 0, 1]
  - entity: crate_stack_03
    model: global_barrela
```

The overlay is an **in-place patch**, not a regeneration: the layer block is shadowed with the base
bytes and only the named records' transform/model words are overwritten, because the `Placement`
parse drops the record's `+16` pad and `+36` tail and cannot round-trip a full rebuild. Each container
an edit writes into gets its `CSUM` trailer recomputed (`placement::reseal`); every other byte is the
base's. An edit that resolves to no change is a hard error — a no-op overlay would ship an identical
block for nothing.

`qm extract-world`'s text is what `world::write_world` writes, so a file it extracted reads and writes
back byte for byte.
Two Shipments editing one layer conflict (M0207): each overlay shadows the whole layer, so one
Shipment's edits would be silently absent. An `add_placement` on the same layer conflicts with it
too.

Scope and the step-0/1/2 proof are in
[`vz_state_world_overlay_scope.md`](vz_state_world_overlay_scope.md).

### `add_placement` and `add_layer`

`add_placement` adds entities to a layer the game ships; `add_layer` mints a new layer in the block
of a `template` layer:

```yaml
  - kind: add_placement
    layer: vz_state_pmccon004       # one layer, by name
    entity: src/barrel.yaml
  - kind: add_layer
    name: vz_state_my_mission       # the new layer's name, hashed to its ASET key
    template: vz_state_pmccon040    # a needle, matched like edit_world's: the block the layer joins
    entities: src/my_mission.yaml
```

`add_placement`'s `layer` names one layer, as `add_tiny_geometry`'s and `activate_layer`'s do; a
block path such as `layers_static` names none and is a build error. The entities go into that layer
itself, each with a record in its `Name`, `ModelName` and `Transform` COMPs and in its `flgs`; every
other record keeps its bytes. A layer without one of those COMPs is given it: one with no
`ModelName`, whose entities take their models from templates (`vz_state_pmccon004`), and one with
no COMP at all (nine retail layers). The COMPs are built from the game's own layers, which all carry
the same `info` and `schm` for each (`mercs2_probe layer-scaffold`).

`add_layer` builds its layer on the shared rows (`CHDR`, `enum`, `flgt`) of the first layer of the
template's block and appends it to that block as its last entry. A block that already holds a layer
of that name is a build error.

The entity file is one record, or a list of records:

```yaml
- key: "0x0F00A001"                 # the entity key, quoted hex
  model: global_barrela
  pos: [1240.5, 12.0, -880.25]
  yaw: 90                           # degrees about Y; or quat: [x, y, z, w]
  name: my_barrel                   # optional; defaults to modent_<key, 8 hex digits>
```

`key`, `model` and `pos` are required. `key` is quoted hex, `"0x"` and up to 8 hex digits: a
number is refused, with the quoted spelling of its value, since an unquoted `0x0F00A001` is a YAML
integer. A model named by its hash is quoted too (`model: "0x8FAE19F4"`). A record gives `quat` or
`yaw`, not both, and with neither it is unrotated. Any other field is refused by name. A file that
does not read is a build error naming the file, as an `edit_world` edits file is.

Both kinds ship the whole block the layer (or the template) lives in, so each claims that layer:
two Shipments adding to one layer, or minting from one template, conflict (M0207), and so do an
`add_placement`, an `add_layer` and an `edit_world` naming one layer. Within one Shipment, every
write to one block (its `edit_world`, `add_placement` and `add_layer` contributions and its
`add_tiny_geometry` placements) is applied to that block in contribution order, and the block ships
once.

Retail places every entity key once: none of the 101,299 keys of `vz.wad`'s 923 layers is in two
layers or twice in one (measured 2026-10-27 over every `type_id 9` layer entry's `Transform`,
`ModelName` and `Name` records). A key any layer of the game places, a key another `add_placement`,
`add_layer` or `add_tiny_geometry` of the Shipment adds, and a key two records of one entity file
share, are **M0198**.

### `add_tiny_geometry`

Adds a **TINY stand-in**: the far-distance model the game draws, from far away, in place of the
world objects of one 200 m grid cell of a layer, and the `TinyGeometryObject` placement that loads
it. Each object's part of the stand-in is drawn while the object is intact, or while it is ruined,
so a destroyed building shows its ruin from a distance too.

```yaml
  - kind: add_tiny_geometry
    layer: vz_state_mar_city_pristine
    cell: { row: 28, col: 32 }
    key: "0x00143FFF"
    objects: ["0x00097FF3", mar_city_tower]
    model: src/tiny/tgr28_tgc32.glb
```

| field | what it is |
|---|---|
| `layer` | the layer the placement goes into, by name: its sub-block entry is `m2(layer)` |
| `cell` | the cell, `row` from z and `col` from x, each 0 to 39: `col = floor((x + 4000) / 200)`, `row = floor((z + 4000) / 200)` |
| `key` | the placement's entity key (GUID), quoted hex as an entity file's is; no layer of the game may already use it (M0250) |
| `objects` | the world objects the stand-in draws, each a bare `"0xGUID"` or the name of a placement in `layer` |
| `model` | the stand-in's `.glb` / `.gltf` |

**Extract, then edit.** Any retail stand-in takes apart into this contribution and its model:

```sh
qm extract-tiny vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece --out <shipment> --game <dir>
```

It writes `src/<model>.glb` under `--out` and prints the contribution. Each retail stand-in, taken
apart and built back, is its own container but for its bounds and, where its slot list holds a slot
`≡ 3 (mod 4)`, its slots (both below); its layer is its own layer but for the placement's
`Transform` tail.

**The model.** Every primitive declares `extras.tiny_role`: `intact` (drawn by `PgMeshTinyVP` while
the object is intact) or `ruined` (`PgMeshTinyVP_Ruin`, while it is ruined); a primitive without one
is M0244. Every vertex carries `_TINY_SLOT` (an unsigned integer scalar), the index of its object
in `objects`; a slot past the list is M0242, and a triangle whose vertices name two slots M0243.
Every primitive has a `NORMAL`, a `TEXCOORD_0` and a material, and is `TRIANGLES` or
`TRIANGLE_STRIP` (a strip is kept as it is; triangles are stitched into one). A node transform must
be a rotation and a translation: positions move with it and normals turn with it, unscaled.

Every material declares its diffuse texture in `extras.texture` (a name, or a bare `0xHASH`; it is
in the game or is an `add_texture` of the Shipment) and its pixel shader in `extras.pixel_shader`:
retail uses several pixel shaders for a material with one texture ([the shader
import](#the-shader-import)), and `PgDiffFP` on every stand-in. `alphaMode` is `OPAQUE` or `MASK`
(alpha-tested); `BLEND` has no stand-in material. Every material is drawn by a primitive. A model
that does not read this way is M0251.

**What the build writes.** The model is `<layer>_tinygeometry_tgr<row>_tgc<col>_0x<key>`, two digits
for the row and column and eight hex digits for the key. Its container is the shape every one of
retail's 1,208 stand-ins has (`mercs2_formats::tiny_model`):

- one sub-object per role present, intact first, each with a `HIER` node named `m2("pristine")` or
  `m2("ruin")` and a `SEGM` record `{k, k, 1}`;
- one primitive group per primitive, in the file's order: the role's vertex shader, the shadow
  shader the role and the material's alpha flag pick (`PgMeshTinyShadowVP`, `…TexVP`, `…VP_Ruin`,
  `…TexVP_Ruin`), and two identical `PRMT` records drawing the whole strip;
- each vertex as half floats: position, texture coordinate, normal, `POSITION.w` its object's slot
  and `NORMAL.w` 1;
- each material named `m2("tinygeometry_tgr<row>_tgc<col>_opaque")` or `…_alphatest`, flags `0x80`
  or `0x88`, one texture, the pixel shader, and `m2("ANY")` after it;
- the bounds of each group, node and the model computed from the half-float positions. Retail's come
  from the positions before they were halved, within half an f16 step of these.

**The slots.** The container's top-level `TINY` chunk lists the objects' GUIDs ascending: a
vertex's slot indexes that list and `ObjectIDScaleArray`, whose entry is its object's state (1
intact and drawn, 2 intact and hidden, 3 ruined and drawn, 4 ruined and hidden). The TINY vertex
shaders read slot `w` as component `w mod 4` of register `w / 4`, and compute component 3 as
`2·.w − .y`: a vertex at a slot `≡ 3 (mod 4)` shows only while its object's state and that of the
slot two below agree. So no object takes such a slot. Each skipped slot holds a filler GUID between
its neighbours that no object of the stand-in has (the first that no placement of the game has,
where there is one). The engine finds an object's slot by binary search over the list, so the list
stays ascending: a run of consecutive GUIDs starts after enough fillers to keep the skipped slots out
of it, and where a run cannot (one of more than three consecutive GUIDs), a filler repeats the
neighbour the search reaches first, so the search still finds every object at its own slot. The list
holds at most 255 GUIDs, so a stand-in draws at most 192 objects (M0240). Of retail's stand-ins, 436
hold an object at a slot `≡ 3`; built back, those objects move, and 122 of those lists repeat a GUID.

**The objects.** A name resolves among the placements of `layer`; a bare GUID among every layer's.
Each object is placed in the stand-in's cell (M0245): the engine finds the stand-ins of an object
whose state changes through the cell of the object's position (`FUN_0050F730`, `FUN_0050F7E0`). An
object of another layer counts by its own placement there; retail's `vz_*_tiny` stand-ins draw the
objects of other layers.

**The placement** is added to the layer's own sub-block, at its key's place in the ascending key
order every retail layer keeps: a `Name` record `tinygeometry_tgr<row>_tgc<col> 0x<key>`, a
`ModelName` record naming the model, a `Transform` at the cell's centre
`(−3900 + 200·col, 0, −3900 + 200·row)`, unrotated, and the `flgs` state every retail placement of a
TINY model carries, bit 111 alone. The `Transform` record's 6-byte tail is written as zero; retail's
TINY placements carry a small value there whose meaning is open. The overlay carries the whole
layer block; a Shipment's stand-ins in one layer go into one overlay of it.

**The key** is the author's. Retail's stand-in keys are the world editor's: one pass allocated the
1,208 stand-ins GUIDs from its counter, layer by layer (`0x0014399B` to `0x00144F24`, above every
other placement key below `0x10000000`), and nothing in a layer or a model derives them.
`extract-tiny` keeps the retail key.

**Limits.** The engine registers at most 1,400 slot lists (`0x0050F1BE`), and drops one past that
(`0x0050F26C`); M0246 counts every stand-in the game places and the Shipment's. A layer has one
stand-in per cell (M0247).

### `activate_layer`

Turns a normally-hidden world-state layer **on** — the permanent, whole-mission counterpart to
`edit_world`. Where `edit_world` moves things inside a layer, `activate_layer` decides which layers
the world streams in at all.

A `vz_state` overlay is switched at runtime by the game's own layer manager:

```lua
MrxLayerManager.MarkForAddition("vz_state_pmccon004_destroyed")
MrxLayerManager.MarkForRemoval("vz_state_pmccon004_pristine")
```

These are the exact calls a vanilla contract makes (`OilCon001.Activated` adds `_act1` and removes
`_pristine`; an outpost capture removes its defense layer and adds its captured one). `layer` is the
name to add; `replaces:` is an optional list to remove first (the pristine or prior overlay this one
supersedes).

```yaml
  - kind: activate_layer
    layer: vz_state_pmccon004_destroyed
    replaces:
      - vz_state_pmccon004_pristine
```

**No `src/` file and no Data half.** `layer` / `replaces` are layer **names**, not assets you ship —
the layer must already exist in the WAD (retail carries 900-odd of them, or a companion `edit_world` /
`raw` ships one). The whole contribution is a Lua registration, baked into the Quartermaster-owned
**`qm_modloader`** — the same expandable load space `add_ui` uses (see [`add_ui`](#add_ui) for how
that works). So N `activate_layer` and `add_ui` mods share one loader and merge cleanly.

⚠ **Layer names are CASE-SENSITIVE**, and a wrong one is silent — `MarkForAddition` on a name no
layer carries reaches nothing at runtime. **M0194** (advisory, needs the game stack) warns when the
`layer` or any `replaces` name has no layer-typed ASET row, unless a companion contribution in the
same install ships it.

The trigger is qm's `world.begin` hook, when player-visible gameplay first starts in a world, once per
Lua VM. For a mission-timed switch (a layer that should flip at a specific objective) you still want a
bespoke `patch_lua` in the contract; `activate_layer` is the turnkey "turn this overlay on" face.

### `add_movie`

Adds a Scaleform GFx movie as a `cfx_pack` asset. The `.gfx`/`.cfx` bytes ship verbatim (retail
carries both, so neither is normalised to the other); `name` is the identity a Lua caller passes to
`SetSwfFile` / `GetShellGfxFilename`, so it is a bare name like `topbar` or `MINIMAP`, no extension.

The engine references movies by **fixed name**, so how a movie reaches the screen decides what
`add_movie` can do:

- **Replace** a shipped movie by using its exact name (`SHELL`, `pause_menu`, `MINIMAP`, a
  `*_briefing`, …). Same name → same hash → last-wins, and every UI site that already names it now
  serves yours.
- **Add a new** movie under a novel name and load it from Lua. The engine binds a movie to a
  `FlashWidget` by NAME — the shipped `loadingscreen_standalone` movie is loaded exactly this way
  (`mrxgui.lua`): `w = FlashWidget:new(); w:SetSwfFile("<name>"); w:Play(); w:SetVisible(true)`.
  A `patch_lua` carrying that boilerplate, plus the movie, is a complete working addition — this is
  a PROVEN capability, not a hypothesis. What stays author-specific is only *where* the widget
  attaches and *when* it shows (a boot overlay, a hotkey toggle, a HUD element parent). Use
  **`add_ui`** below to have the Quartermaster bake that boilerplate for you; reach for a bare
  `add_movie` + `patch_lua` only when you need full control of the trigger.

The block goes where the front end or gameplay can load it, from the WAD retail ships the movie in
(qm's `data/retail_movies.tsv`):

| Retail ships `name` in | The block goes to |
|---|---|
| `shell.wad` (`SHELL`, `LTI_precache`) | `shell-patch.wad`, which the front end mounts |
| `vz.wad` (`pause_menu`, `MINIMAP`, `*_briefing`) | the overlay, which gameplay mounts |
| both (`loadingscreen`, `GFxFontLib`) | both |
| neither (a new movie) | the overlay |
| only `Loading.wad` (`loadingscreen_standalone`, `fonts_*`) | refused: qm writes no patch for it |

The shell patch carries `shell.wad`'s CSUM row, so building a `shell.wad` movie needs the game. When the
installed set has a level, `qm link` ships its own edit of `SHELL` ([`add_level`](#add_level)) and refuses an
`add_movie` or `add_ui` of `SHELL`.

**M0192** (advisory, needs the game stack) fires when `name` matches no shipped movie, to catch the
novel-name case above before it ships something the game never shows. It does **not** fire on
`add_ui`, which references its own movie by construction.

### `add_ui`

The turnkey face over `add_movie` for a movie that should simply **appear on screen**. Same two
fields — `name`, `movie` — and the Data half is byte-for-byte an `add_movie` (the same `cfx_pack`,
the same primary ASET row). The difference is the Script half: `add_ui` also enrols a `FlashWidget`
that plays the movie, so it shows without any hand-written Lua.

How that Script half works is the part worth understanding, because it is how **every** future
composed addition reaches the game without mods fighting over one script:

- The Quartermaster mints a script it owns, **`qm_modloader`**, as a genuinely new `scripts_vz`
  asset (a new block entry *and* its primary type-35 ASET row — the two halves the DLC's own loader
  ships for a new importable module). Every UI mod's `FlashWidget` registration is baked into this
  one script. It is the *expandable load space*: it grows as mods are added.
- No game script is recompiled for it. qm's world entry, spliced onto the master script
  ([qm's hooks](#qms-hooks)), loads `qm_modloader` with `dynamic_import`, and each registration runs
  at qm's `world.begin` hook, once per Lua VM. That code never changes however many UI mods are
  installed.
- Installing several UI mods together merges cleanly: their registrations concatenate into one
  `qm_modloader`, in load order (the order `requires` and the installer's list give; see
  [Composition](#composition)), so the same set always produces the same bytes. Each registration
  runs under its own Shipment's name, so one that fails stops the game naming that Shipment (the
  [error policy](#qms-hooks)).

`add_ui` makes the movie *appear* (created, played, visible, and parked in `_QM.ui[name]` so you can
reach it from Lua). Its on-screen **rect and hide trigger** are still yours — add a `patch_lua` that
manipulates `_QM.ui["<name>"]` if you need to place or toggle it. For a movie that *replaces* a
shipped one, use `add_movie` with the exact shipped name instead; a replacement needs no widget of
its own.

The movie is checked as an `add_movie`'s is: one carrying ActionScript 3 is **M0190**, which the
game's GFx 2.0.48 runtime cannot execute.

### `add_script`

Adds a new Lua module the game can `import`:

```yaml
  - kind: add_script
    name: ess                       # what Lua passes to import("ess")
    source: src/lua/ess.lua         # compiled to the game's LuaQ at build time
    exports: [Ess, OnStateChange]   # optional: the globals this module writes on purpose
    load: { when: world.scripts_loaded, in: [gameplay] }   # optional: a library qm loads
```

`name` is the module name the engine hashes to find the script: bare, no extension, and not a name a
script of the game, of `qm`'s own loaders or of the Shipment's novel shop behaviours already has
([M0197](#m0197)). `source` is the `.lua` file, compiled when the Shipment is built.

**When the module runs.** The engine's `import("<name>")` does not run a module: it only binds one
whose body has already run, and binds nothing for one that has not. A module's body runs when an
executed script lists it in its compiled imports (its DEPS), or when something `dynamic_import`s it.
A **library** that nothing in the game imports therefore declares `load:`.

**`load: { when, in }`.** At the lifecycle hook `when`, in each session of `in` (`gameplay` or
`front_end`), qm's loader `dynamic_import`s the module. The engine runs its body and hands back its
environment, and the loader records it ready. `when` is a hook id of Quartermaster's catalogue
(`crates/mercs2_quartermaster/data/hooks.tsv`). The loaders' entry points are `world.scripts_loaded`
(gameplay, when the master script's chunk runs: `vz`, or a custom level's own master) and
`shell.boot` (front end). `in` must list only sessions that hook fires in ([M0312](#m0312)). A
library the engine cannot load, or one requested before its session's settle hook (`world.begin`,
`shell.open`) and not ready when that hook fires, stops the game with a message naming the Shipment
and the module. Libraries of one hook load in the set's load order, so a library loads after the
libraries its Shipment `requires`.

#### qm's hooks

Every `qm link` splices qm's own Lua onto the end of retail scripts, mods or no mods: a hook
dispatcher, the loaders' entry points, and one piece of code per hook of the catalogue
(`crates/mercs2_quartermaster/data/hooks.tsv`) the link generates. Nothing is recompiled from the
decompiled corpus; the code is compiled on its own and runs at the end of the retail chunk. Each hook
prints `[qm] hook <id>: enter` every time it fires, which `pmc_blackbox.log` records.

The link generates every hook of the catalogue, in the session it fires in, so any library can load
at, or subscribe to, any of them. qm's own loaders use `world.scripts_loaded`, `world.begin` and
`world.sound_exit` in gameplay, and `shell.boot`, `shell.open` and `shell.sound_exit` in the front end.

The front end has two opening hooks. `shell.open` fires when the front-end shell opens, at its "Press
Start" screen; `menu.open` fires once per shell, when the start press brings up the main menu (qm
reads retail's own condition for that press, `mrxguishell.lua:318-320`). `shell.play` fires for New
Game and for Continue alike: retail starts both through one function. The online hooks fire on the
shell's lobby and join choices: `shell.lobby.enter` / `.exit` (LAN), `shell.friends_lobby.enter` /
`.exit`, `shell.online`, and `shell.join.lan`, `.friends`, `.quickmatch`, `.optimatch`.

`dynamic_import` answers asynchronously, so a library loaded at a hook is not ready during the hooks
that follow at once. A library loaded at `world.scripts_loaded` is ready by `world.begin` (the settle
check makes sure), but not by `world.load`: in a run of 2026-11-07, `world.load` fired 92 ms after the
entry and 18 ms before the entry's library was ready. A game that loads into an HQ fires `hq.enter`
just before `world.begin` (9 ms in the same run).
Retail code reaches the dispatcher through the global `_G._QM`, never through `import`.

**The error policy.** A hook handler or a library that fails stops the game; it never continues. The
failure prints `[qm] FATAL <owner> at <hook>: <error>`, naming the Shipment (or `Quartermaster` for
qm's own code). When retail's dialog can be shown (`MrxGuiDialogBox` has loaded and there is a local
player), the game pauses under the dialog retail's own in-game prompts use (the one the death
dialog opens), which takes keyboard, pad and mouse input, and choosing **Exit** exits the game. Otherwise the failure also prints
`[qm] FATAL-NODIALOG <owner> at <hook>: <error>`. pmc_bb's log variants watch for that line on
every run, with or without `PMC_VERBOSE_LOG`: they write it to `pmc_blackbox.log` (source `qm`),
show it in a native message box, and end the game when the box is closed (exit code 3). pmc_bb's
quiet variants (`crack_only`, `crack_asi`) cannot see it, so right after the print qm requests the
`Exiting` game state itself.

A host script must define each function a hook wraps, and must not `return` from its top level (a
return there skips the code spliced after it). The link checks both against the bytecode it splices
onto and fails, naming the hook and the script, when either does not hold. This applies to a custom
level's master too ([`add_level`](#add_level)).

**The load plan's libraries.** `qm link` records every module it adds in its `load-plan.json`
(format 3), under `libraries`, from the list it splices: one row per module and session, in load
order, the libraries first and then the modules only an `import` loads. A row holds `module`, `item`
(the Shipment's request id; `null` for qm's own `qm_modloader` and `qm_shell_modloader`), `session`
(`gameplay` or `front_end`), `when` (the hook qm's loader loads it at; `null` for a module only an
`import` loads) and `settles` (whether the session's settle hook checks it is ready: the entry's
libraries). A custom level loads the same set, so the one list covers it. Every link also places the
plan in the game folder at `scripts/load-plan.json`, beside the Lua bridge, which reads it at startup:
a `game_folder` placement, written with an empty list when the set has no library. `qm preflight`,
which links nothing, and a plan that refuses the link write `libraries: null`. The plan's types are
`mercs2_formats::load_plan`, which every reader parses.

**`exports`.** A module that writes into `_G` competes with the engine's own writes to it (M0302,
[Lua engine seams](lua_engine_seam_hardening.md)), so a write to `_G` or `_MODULES` is an error unless
the module declares it. `exports` lists the globals the module writes deliberately — a library's public
table, an engine callback it chains — each as a bare Lua identifier. A write to `_G.<name>` or a field
under it (`_G.<name>.<field>`) passes for a listed name; any other `_G` write, every `_MODULES` write,
and an entry that is not a Lua identifier stay errors. Writes inside comments and strings are not
writes.

### `set_spawn_lists`

Sets spawn-list calls of a shipped mission script to other lists.

```yaml
- kind: set_spawn_lists
  target: gurcon002
  lists:
    - function: SetupDangerousObjBuildings
      building: TallCommBuild
      group: Rooftop          # leave out for a call that sets every group
      list: Spawnlist (VZ Elite)
```

Each row names one call the target makes inside its top-level function `function`:
`Ai.TweakAttachedSpawnersInGroup(building, "group", {SpawnList = "…"})`, or, with no `group`,
`Ai.TweakAttachedSpawners(building, {SpawnList = "…"})`. qm writes the Lua: for each function, a
wrapper that runs the shipped function and then makes each row's call again with the row's `list`.
It is linked onto `target` as a `patch_lua` append is, so it merges with other Shipments' Lua. A
`lists: []` writes no Lua and claims nothing.

The wrapper reads the building by its name, so a row can name only a building that is a global: not
a parameter of the function, and not a `local`, a `for` variable or a parameter of a function inside
it. The rows are checked by [M0317](#m0317).

### `edit_stringdb`

Corrects or localises UI text. The Shipment's own overlay carries one edited copy of the target
string table, with all of the Shipment's string contributions to that table applied in order;
installed beside other Shipments editing the same table, `qm link` merges all their edits into one
table, in load order (see [String tables are merged](#string-tables-are-merged)).
Arbitrary-length
edits are supported — the codec (`mercs2_formats::stringdb`, proven byte-identical against all six
retail language tables) rebuilds the heap and re-points the offsets.

`strings:` is a `src/`-relative text file, **one edit per line**:

```
[Menu.Play] = New text
0x0000B29D = an edit by hash, for a key whose bracket name you do not have
# a comment
```

Line-oriented rather than YAML on purpose: UI text carries `:`, `%s`, quotes and colons that a YAML
value would demand escaping. A key not present in the table is a hard error, named — a
silently-dropped correction is worse than a failed build.

**Both copies of a table are written.** The language tables are served from `shell.wad` in the front
end and from `vz.wad` in gameplay ([`docs/fixpack/wad_duplicate_inventory.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/fixpack/wad_duplicate_inventory.md) §C), and the two never
mount together. So every string write goes to each copy of its table that exists: `vz.wad`'s
(`english`; the overlay carries it) and `shell.wad`'s (`english`, `french`, `german`, `italian`,
`spanish`, `russian`; the shell patch carries it). A table only `shell.wad` has, such as `french`, is a
valid target and reaches the front end only. A table in neither is an error.

### `add_language`

Adds a **new language** the game never shipped. It is the one kind that places a new **base WAD**,
`data/<name>.wad`, rather than an overlay: the engine builds both the mounted file name
(`.\Data\<name>.wad`) and the language's string-table key from the same name, and it exits if that
base WAD is missing.

```yaml
  - kind: add_language
    name: klingon          # the WAD file name and the string table's name
    display: Klingon       # the label a selector (Modkit) shows; not written into any WAD
    strings: src/klingon.txt
    base: english          # the shipped table to start from; omit for english
```

- `strings:` has the same format as [`edit_stringdb`](#edit_stringdb)'s. The build copies the `base`
  table, applies these edits (keys you leave out keep the base text; a key the base does not have is
  a hard error), and re-keys the copy under the new language's name. A file with no strings at all is
  refused.

`data/<name>.wad` is the whole of the language's data; the Shipment overlay carries nothing for it.
The build needs the game stack, and opens `English.wad` beside `vz.wad` as well as the stack. The
WAD holds:

| what | from | as |
|---|---|---|
| the string table | the `base` table, translated | `m2(name)`, type `0x39E5E978` |
| fonts `<name>_18`, `<name>_20` | `<base>_18`, `<base>_20` | the font's one `MTRL` reference to `<base>_18_main` / `<base>_20_main` repointed to `<name>_18_main` / `<name>_20_main`; every other byte as the base has it |
| atlases `<name>_18_main`, `<name>_20_main` | `<base>_18_main`, `<base>_20_main` | the base atlas under the new name hash; the texture's `NAME` chunk unchanged |
| voice-over tables | every soundbank, sounddb and wavebank of `English.wad`, streamed and embedded | re-keyed from `m2("<bank>.english")` to `m2("<bank>.<name>")`, computed from the bank hash each table carries; the table bytes unchanged |

A font reaches its atlas only through the name hash in its `MTRL` chunk, and the engine reads a
texture's `NAME` chunk into a buffer nothing uses, so a re-keyed atlas needs no other edit
([`eighth_language_wiring.md` §10](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/eighth_language_wiring.md#10-fonts-atlases-and-the-engine-language-table)).
The fonts and atlases of `base` are read from `vz.wad`'s stack and from `shell.wad`; a copy in both
must match byte for byte (M0219 when `base` has none). In the retail install only `english` has
them (`shell.wad` block `blocks\Shell\english_P000_Q3.block`), so `base` is `english` in practice.

Retail Lua loads a `vo_*` bank as `<bank>.<language>` (`_GetLocalizedName`,
`mrxsoundbanks.lua:80-87`), so the re-keyed tables are the ones the new language's voice-over
loads. The streamed `vo_stream` wavebank's waves play from `Audios\vo_stream.<language>.pws`
(`_OpenFile`, `mrxsoundbanks.lua:96-107`), so the build records a `stream_copy` placement from
`data/Audios/vo_stream.english.pws` to `data/Audios/vo_stream.<name>.pws`, with the source's size
and sha256, and the deploy step copies the file. The build writes two placements for the language:

- `data_wad { relative: data/<name>.wad, display }`;
- `stream_copy { from: data/Audios/vo_stream.english.pws, to: data/Audios/vo_stream.<name>.pws }`.

`vo_stream` is the one streamed wavebank of `English.wad`. Its other 42 wavebanks are
**embedded**: each carries its waves' audio inside the table (1,498 waves in all), so the re-keyed
copy in `data/<name>.wad` carries that audio to the new language and the 1,492 English cues that
play those waves play them in the new language too. They are the 13 `vo_*Con*` banks
(`vo_allCon001`, `vo_oilCon021`, `vo_pmcCon003`, …), the 25 `vo_job_*` banks (`vo_job_all_Conrad`,
`vo_job_heros`, `vo_job_pmc`, …), `vo_helirec001`, `vo_jetRec001`, `vo_mechRec001`, and
`vo_solanoahj`, whose waves the `vo_Chris`, `vo_Jen`, `vo_mattias` and `vo_Misc` soundbanks play.
Retail's `vo_job_all_gonzalez` wavebank holds 26 waves of all-zero samples, and its copy is the
same. The census is the retail test `english_wad_embedded_voice_over_wavebank_census`
(`mercs2_quartermaster/tests/build_retail.rs`).

- **M0200** refuses a `name` that is not a lowercase `[a-z0-9_]` token (it becomes a file name) or
  that is a WAD the game already ships (`vz`, `shell`, `loading`, `english`, `french`, `german`,
  `italian`, `spanish`, `japanese`, `russian`). So `add_language` can only add a WAD, never shadow
  one.
- Switching the game into the new language is **not** this kind's job, and a Shipment does not
  carry anything for it. PC has no in-game language selector — the language is picked at boot from
  the OS locale — and selecting an installed language is handled by Modkit. The engine side is in
  [`language_asi_hook_contract.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/language_asi_hook_contract.md).
- Two Shipments adding the same language name conflict (M0207). The build needs the game stack, to
  read the `base` table.

### `add_level`

Adds a **level the game boots by name**, compiled from the Shipment's [`level_asset`](#level_asset) rows.

```yaml
  - kind: add_level
    name: dlc01              # the name it boots by: its master script and <name>_base layer
    display: Blow It Up Again  # the map name on the Extras page
    mission: Blow It Up Again Pack                          # optional
    details: Blow it up again with four new explosive missions!  # optional
    date: 2008-11-25                                        # optional, YYYY-MM-DD
    streams:                 # one per streamed wavebank of the level, named for its entry
      - src/level/Audios/dlctest_streaming.pws
```

**Extras ▸ Downloadable Content lists every installed level**; picking one, then a hero, starts it. `qm link`
does this for the set; a Shipment writes nothing for it.

- The page shows `display`, `date` as `month/day/year`, `mission` and `details`, as literal text. Without
  `date` it shows `//`; without `mission` or `details`, an empty line. The page sorts the list itself (SHELL's
  `sortDLCbyDate` / `sortDLCbyName`), four entries to a screen.
- `qm link` edits the game's own SHELL movie, read from `shell.wad`, so the page is reachable on PC and its
  hero pick starts the level. The edit is qm's `data/shell_extras.tsv`; a SHELL that is not retail's fails the
  link. The edited SHELL ships in the shell patch, so a Shipment replacing SHELL with `add_movie`/`add_ui`
  beside a level is refused.
- qm's front-end loader adds the entries when the page opens and starts the level chosen. A name it did not
  list fails, naming it.
- **M0311** refuses a `date` that is not a `YYYY-MM-DD` calendar date.

For a level `L` the engine opens `.\Data\L.wad`, which must exist, then `.\Data\L-patch.wad` above it,
which may not, and no other level WAD (`FUN_004BFCE0`, `FUN_004BFDA0`). A level does not run alone,
because its scripts and models use the base game's, so the build writes three placements:

- `level_patch { level, relative: data/<name>-patch.wad, display }`: the compiled blocks, stamped with
  `vz.wad`'s `CSUM` row as every overlay is;
- `stream_copy { from: data/vz.wad, to: data/<name>.wad }`, with the size and sha256 of the file read,
  so the level's assets win and everything else resolves from the copy. This takes as much disk as
  `vz.wad`, about 2.4 GB;
- `game_folder { relative: DLC/<name>/Data/Audios/<file> }` for each stream: the level's own folder, never
  the game's `data/Audios`. The level's sound bootstrap opens a stream at
  `Net.DLCGetMountPoint() .. "\" .. Sound.GetAudioDir() .. "\" .. <NAME>.PWS`, and on PC the Shipment
  that ships the level registers `Net.DLCGetMountPoint` to return `DLC\<name>`, as the consoles return
  the DLC's install folder.

`vz.wad` itself is never changed. The level slot holds only these two WADs, so `qm link` compiles each
level's whole patch for the installed set, and the deploy step merges every Shipment's overlay under it:

- `vz.wad`'s scripts blocks are read with the level's assets in place of their twins, and the set's
  scripts, effects and templates are linked into those.
- A `patch_lua` on a script the level overrides reaches the level's script. Its source may not compile
  (unluac writes `goto` for a stripped script's control flow), so the level's bytecode stays as it is and
  the appended Lua runs at the end of its main chunk, in the same module. It reaches the module's globals,
  not its locals.
- A block the link touched ships whole, so a level asset in it leaves the level's own blocks.
- The level runs its own master `L`, never `vz`'s chunk, so qm's hooks that the catalogue hosts on
  `vz` ([qm's hooks](#qms-hooks)) are spliced onto `L` instead, on every link. `L` must define the
  functions those hooks wrap, as both retail masters do (`vz/xQ!L.lua`, `dlc01.lua`): `LoadSingleton`,
  `SaveSingleton`, `SaveComplete` and `ResetSingleton`, which the engine calls by name on the master, and
  `_StartPlayerVisibleGameplay`. It must not `return` from its top level. The link fails, naming the hook,
  when either does not hold. The same holds for a script the level overrides that hosts a hook: it keeps
  the functions the hook wraps (Blow It Up Again's 12 overrides do).

The build needs the game stack, and fails, listing each one, on:

- `level_asset` rows that hold no script `<name>` and layer `<name>_base`;
- a script a script's `DEPS` names, or a texture a model's `MTRL` names, that neither the rows nor the
  game stack holds;
- a pixel shader a material names, or a vertex shader a model's group names, that retail does not
  register;
- a streamed wavebank with no file in `streams`, or a file whose stem is no streamed bank's entry.

What Lua asks for by a name it computes is not visible to a build.

- **M0311** refuses what could only overwrite.
- Two Shipments adding the same level conflict (M0207).

`from: <id>` instead of `streams` takes the whole level from a [`fetch_package`](#fetch_package): at build
every asset the package converts to becomes a `level_asset` row of this level, and its streams the level's
streams. With `from`, `streams` is empty and no `level_asset` row of this level is written by hand
(**M0316**).

```yaml
  - kind: add_level
    name: dlc01
    display: Blow It Up Again
    from: biua_ps3
```

### `add_menu_item`

Adds an **item to a page of a retail menu**. Its label is set in the menu's own stencil lettering;
choosing it runs a Lua function of yours in the front end.

```yaml
  - kind: add_script            # the library the item's action is in
    name: modsmenu              # script names are lowercase
    source: src/modsmenu.lua    # defines function Open()
    load: { when: shell.boot, in: [front_end] }
  - kind: add_stringdb_keys     # the label and help text, once per installed language
    target: english
    strings: src/english.txt    # [Mods.Menu] = MODS
                                # [Mods.MenuHelp] = Browse the installed mods.
  - kind: add_menu_item
    menu: shell.main            # the main menu
    label: Mods.Menu            # the label's string key
    after: SHELL.MainMenu.Options
    help: Mods.MenuHelp         # optional: the help bar's line while it is selected
    action: modsmenu.Open       # Module.Function, run with no arguments
```

- `menu` is a page of qm's menu catalogue. The catalogue has `shell.main`, the main menu. A page
  takes as many added items as it has spare bullets below its last one: **two** on the main menu,
  across every installed Shipment. A set adding more fails the link, naming each item and its Shipment.
- `after` is the item it goes after: a retail item's key (`SHELL.MainMenu.Continue`, `NewGame`,
  `JoinGame`, `Options`; EXTRAS is `0xaba15838` and EXIT `0xb618aecb`, whose names are not known) or
  another `add_menu_item`'s `label`. The items below move down one place.
- `label` and `help` are string keys. The label is set, in capitals, from the key's text in **every
  installed language's front-end table** (`english`, `french`, `german`, `italian`, `spanish`: write
  each with `add_stringdb_keys`, [both copies](#edit_stringdb)). A key with no text in an installed language
  fails the link, naming each one. The lettering has the letters A–Z, the accented capitals of those
  languages and Œ; a character it has no letter for (a digit, punctuation) fails the link, naming it,
  and so does a label wider than the button.
- `action` is `Module.Function`. The module is an `add_script` of the installed set that loads in the
  front end (`load: { in: [front_end] }`), or the link fails naming it.

`qm link` composes every installed Shipment's items, in load order and then contribution order, into
one edit of the player's own `SHELL` (after qm's Extras edit when the set has a level), shipped in the
link's shell patch. Each item gets a new button whose text clips read its slot of a new texture,
`shell_MAINMENU_qm_text`, set in each installed language and shipped in that language's patch
(`data/<language>-patch.wad`), as retail ships its localized label art. Its Enter, or a click, sends
FSCommand `qmMenuItem` with the label key; `qm_shell_modloader` runs the item's function under its
Shipment. A function that errors, or is missing when the item is chosen, stops the game naming the
Shipment.

- **M0320** refuses a `menu` that is no catalogue page, **M0321** an `action` that is not
  `Module.Function`, **M0322** an empty key.
- Two Shipments adding one label to one page conflict (M0207); one Shipment adding it twice is M0120.
- No Shipment may replace `SHELL` (`add_movie` / `add_ui`) in a set with a menu item or a level.

### `level_asset`

One asset of an [`add_level`](#add_level) of this Shipment, or of the main campaign (`level: vz`), in the
PC container form of its type. It carries every byte of the asset, so any type the engine loads can be
carried and nothing is re-encoded. A Shipment holds no WAD: the build compiles these rows, because a WAD
shipped whole would shut every other Shipment out of the level.

```yaml
  - kind: level_asset
    level: dlc01             # the add_level it belongs to
    type: model              # as the WAD's type table names it
    name: "0x86EDD514"       # a name, or a bare hash
    rungs:                   # its LOD chain, coarsest first
      - { block: dlc_al_veh_tank_m1a3_P000_Q3, file: src/level/model/0x86EDD514.0.ucfx }
      - { block: dlc_al_veh_tank_m1a3_P001_Q2, file: src/level/model/0x86EDD514.1.ucfx }
```

- Every asset naming one `block` ships in that block, and the block's `_P00N` is the rung. A chain
  starts at `_P000` and runs without a gap. The build writes each rung's chunk table word, the bytes of the
  rungs before it.
- A row of an `add_level` reaches only its level, so it may hold a name the base game also uses, and
  Venezuela keeps its own.
- **M0311** refuses a row whose `level` this Shipment does not add (and is not `vz`), whose `type` the type
  table does not name, or whose rungs do not fit the rules above.
- `dlc_port --shipment` writes these rows from a console package.
- `from: <id>` instead of `rungs` takes the asset's rungs from a [`fetch_package`](#fetch_package)'s
  conversion, found by `type` and `name` (a name is hashed; a bare hash is used as it is). The build fails,
  naming the row, when the package converts no such asset. A row has exactly one of `rungs` and `from`
  (**M0311** for neither, **M0316** for both). `templates` and `edits` work as with `rungs`.

  ```yaml
  - kind: level_asset
    level: vz
    type: model
    name: pmc_hum_obama
    from: biua_ps3
  ```

**`level: vz`: the main campaign.** A row of level `vz` needs no `add_level`. Its asset ships in the
Shipment's overlay, in `blocks\VZ\<block>.block`, with its own ASET row; its rungs are numbered after the
overlay's other blocks. It only adds: the build refuses, listing each one, an asset whose name and type
`vz.wad` holds, and a script a script's `DEPS` names, a texture a model's `MTRL` names or a shader that
neither the Shipment's `level: vz` rows nor the game holds. Two Shipments adding one asset conflict
(M0207). A streamed wavebank plays only from its stream, and the main campaign opens none of a
Shipment's on its own, so the build also refuses one no [`load_sound`](#load_sound) of the Shipment opens
a stream for.

A row whose block `vz.wad` already has (a `chatter` container in `resident_P000_Q3`, the always-loaded
block) cannot ship in a block of its own: an overlay block at that path would replace `vz.wad`'s whole.
`qm link` adds it to the copy of that block it re-emits anyway (the scripts blocks), with an ASET row
shaped like that block's own rows of its type; the block must be one the link re-emits, and the asset
must have one rung. A standalone `qm build` logs that the row ships only through `qm link`. A bank's
tables of one name (a `soundbank` and its `sounddb`) are one asset, not two claims of one.

The worldentity is the one row that ships nothing: `vz.wad`'s worldentity holds every template of the
game, so a row of `type: world_entity_data` lists the templates of its container to append to it.

```yaml
  - kind: level_asset
    level: vz
    type: world_entity_data
    name: "0x50075B3B"
    rungs: [{ block: resident_P000_Q3, file: src/level/world_entity_data/0x50075B3B.0.ucfx }]
    templates:               # every template, by name; none is added that is not listed
      - DLC_M1A3
      - Vehicle Seat (DLC M1A3) (Driver)
      - "Explosion (Super TOW) "
```

- `edits` (optional) sets fields of listed templates before they are appended, by template name, then
  class, then field — a name or `"0xHHHHHHHH"`, quoted, since YAML reads a bare one as a number — as the
  [template form](#add_fx) writes values. A class
  the template holds more than one record of, a field the record lacks, or a value of the wrong type is
  refused. Blow It Up Again lets the Hammerhead's driver leave, which the DLC forbade:

  ```yaml
      edits:
        Vehicle Seat (2 Seat) (seat_front_left) DLC (armoredmonster):
          EntranceToSeat: { CanExit: true }
  ```
- The container may be a console level's compact one: it is read by the class layouts of `vz.wad`.
- Each template is appended under the key it has in the container, so the links between the listed
  templates (a vehicle's seat, its weapons, their projectiles) stay as the level wrote them. Its name gets
  the `Name` flag 0.
- The build and the link refuse, listing every one: a name the container has no template of, or more
  than one; a key the game's worldentity, or a template another Shipment appended, already uses; and a
  template key of the container that a record of a listed template holds, that is not listed, and that the
  game does not hold under the same name. That last check reads each 4-byte word of the records'
  fixed-size payloads (names, point locations and packed 16-bit values are not read), so one
  run lists every template the listed ones name; a template it adds can name more, which the next run
  lists (Blow It Up Again's five vehicles took four runs, to 31 templates).
- Two Shipments appending one template name conflict (M0207).
- **M0311** refuses a `level: vz` worldentity row without `templates`, `templates` on any other row, a
  worldentity row of more than one rung, a template named twice, and `edits` of a template `templates`
  does not list.

### `add_shop_item`

An item for sale at the `shops` vendors. `qm link` appends its catalog row to `mrxsupportdata`
(`behaviour`, support catalog) or `wifequipmentdata` (`equipment_type`), and a reward row per vendor
to `mrxrewarddata`.

```yaml
- kind: add_shop_item
  id: dlcm1a3
  name: "[vehicle.dlc_m1a3]"
  icon: vehicles_tank_m1a2
  shops: [pmc]
  type: heavy
  cash_cost: 250000
  unlocked: true
  behaviour: { module: mrxcratedelivery, cargo: DLC_M1A3, delivery_vehicle: "Mi26 (PMC) (Driver)" }
  levels: [vz, mylevel]
```

- `levels`: where the item is sold. `vz` is the main campaign; any other entry names an installed
  Shipment's `add_level`. Default `[vz]`.
- Each level runs its own worldentity. `behaviour.cargo` and `behaviour.delivery_vehicle` must name a
  template of the worldentity of every listed level: the campaign's, with every `level: vz`
  template, or the custom level's own. A template added with a `level: vz` row is in the campaign
  only.
- `qm link` refuses the set, naming the item, the level and the template, when a listed level's
  worldentity has no such template, when an entry names no installed level, or when `levels` is empty.
- A novel behaviour (`behaviour.script`) mints its module in every level; its row goes into the
  loader of each listed level.

### `fetch_package`

Content the **player's machine downloads and converts at build**, which the Shipment never ships. A
console DLC's assets belong to their publisher: the Shipment carries only where to get the package and
how to check it, and `qm build` makes the components on the player's machine.

```yaml
  - kind: fetch_package
    id: biua_ps3             # what `from:` names; a lowercase [a-z0-9_] token, unique in the Shipment
    format: ps3_dlc_pkg      # the converter: a PS3 PSN .pkg of a Mercenaries 2 DLC (dlc_port)
    url: http://zeus.dl.playstation.net/cdn/UP0006/BLUS30056_00/1ntHdj11R8gYK1qjpRcpRGHI3bEQBhL6i616GVXFV8bh78m4U6iSVyAiYJ4q9RNv6JnuURsk8CN8WWTLSdJU9cT631f4tHSHg7XBs.pkg
    sha256: d62081de1b9efbc484766fb586adba04b03edd5f8571ed03cf6e722bd70263e4
    size: 407578320
```

- **`qm lint`** checks the fields and never touches the network, so a Shipment without the package's
  components lints clean.
- **`qm build` and `qm link`** first check every `fetch_package` row by the same rules as M0316, before
  touching the cache or the network. They keep the file in qm's cache as `packages/<sha256>.pkg` and
  download it there when a conversion needs it and it is absent. The download goes into a `.part` file
  of its own, takes at most `size` bytes, and is renamed into place only when its size and sha256 match
  the row; otherwise it is deleted and the build fails, naming the URL and both values. A
  `Content-Length` other than `size`, a body encoded other than plainly or with gzip, a redirect to
  anything but an `http://` or `https://` URL, and more than 5 redirects fail the download; each
  redirect followed is printed. The whole download, redirects included, must finish within 300 s plus
  `size` at 64 KiB/s (BIUA's package: about 1 h 49 min). A cached file that does not match is an error
  naming it; delete it and the next build downloads it again.
- `--package <id>=<path>` uses a local copy instead: a regular file, checked the same way, on every
  build. Each `<id>` must name a `fetch_package` of the Shipment, or of one of the Shipments `qm link`
  is given, and may be given once.
- The file must start with its `format`'s magic (`ps3_dlc_pkg`: a PS3 package's `7F 50 4B 47`) before
  anything reads it whole.
- The cache is `QM_CACHE_DIR` when set, else `%LOCALAPPDATA%\mercs2_quartermaster` on Windows,
  `~/Library/Caches/mercs2_quartermaster` on macOS, and `$XDG_CACHE_HOME/mercs2_quartermaster` (or
  `~/.cache/mercs2_quartermaster`) elsewhere.
- The conversion reads the player's `vz.wad` (`--game`): its class layouts lay out the package's compact
  containers, and an asset `vz.wad` already holds is left to it. The converted components are kept in
  the cache as `converted/<key>/`, keyed by qm's version, the package's sha256 and `vz.wad`'s sha256
  only, so editing the manifest or `src/` converts nothing again; a conversion appears there only once
  it is complete. A stream name in the package that is not a plain file name fails the conversion.
- Each build makes a fresh staging copy of the Shipment in the cache, `staging/<shipment>/<run>/`: the
  Shipment's `src/` (minus `src/level`) copied, and the converted components hard-linked in, so the
  cache must be on a filesystem with hard links. A symlink, or anything but a folder or a regular file,
  under `src/` fails the build, naming it. Every `from:` row is resolved against the staging copy before
  the build reads the rows, and the build deletes the Shipment's older staging copies that no other qm
  is using. `vz.wad` is hashed once per run, however many Shipments `qm link` is given. The build's
  output still goes to `--out` (default `<shipment>/_build`).
- `qm preflight` reads no component, so it needs neither the package nor the conversion.
- A `load_sound` stream installed by an `add_level` with `from:` is checked against the converted
  streams at build ([M0222](#m0222)).
- **M0316** refuses a malformed field, a package no `from:` names, and a `from:` that names no package.

### `add_model`

Adds a new model by injecting your geometry into a **donor** — a shipped model whose rig, materials
and state machine it borrows, read-only.

```yaml
  - kind: add_model
    name: my_crate               # the new asset's name
    model: src/crate.glb
    donor: oc_veh_helicopter_md500
    group: 3                     # optional: the donor draw group that hosts the geometry
    collision: follow_geometry   # optional, rigid only: donor (default) | follow_geometry
    textures:                    # optional: the model's own maps
      diffuse: src/crate_dm.png
```

Without `retarget:` the model is lowered **rigidly**: the geometry goes into one donor draw group
(`group:`, default 0) and the rest are neutralised. With `retarget:` it takes the skinned path
([below](#retarget--the-skinned-path-add_model-add_outfit)), which splits the mesh one donor draw
group per source part; there `group:` is the first of those groups: a mesh the split keeps in one
group goes into it whole, and one split across several starts in it and continues in the donor's
largest other groups. Without `group:` the split takes the largest groups. A `group:` the donor does
not draw is a build error naming the groups it does.

`collision:` picks the prop's static collision: `donor` keeps the donor's `PHY2` as is;
`follow_geometry` regenerates it from your own mesh. It is a rigid-path option — the skinned path's
collision is ragdoll/capsule and is never re-authored — so **M0202** refuses
`collision: follow_geometry` together with `retarget:` rather than ignore it.

`textures:` gives the model its own skin. Each map (`diffuse`, `specular`, `normal`) ships as its own
resident texture, `<name>_dm` / `_sm` / `_nm`, and the donor's MTRL is repointed onto it at that
slot:

- **rigid** — the hashes the **host group's** materials name at that slot. Each of those materials
  must carry the textured flag `0x0080`: a material with flags `0x0000` is flat-shaded and ignores
  whatever texture is bound
  ([`field_guide.md` Trap 2](field_guide.md#trap-2--the-model-loads-fine-then-the-game-crashes-when-you-look-at-it)).
  The build refuses a host group with such a material rather than ship a map nothing draws — pick a
  `group:` whose materials are textured.
- **skinned** (`retarget:`) — every donor material's hash at that slot.

Either way, a map with nothing to repoint onto, or a repoint that matches nothing, fails the build.
PNG only, 8-bit, dimensions a multiple of 4; normals are BC3/DXT5nm, diffuse and specular BC1 unless
the source carries real alpha.

#### The shader import

The host group names two vertex shaders in its primitive-group `INFO` — the main one at `+0x0C` and
the shadow one at `+0x10`, each `m2` of a registration name — and each of its materials names a
pixel shader in the `MTRL` word after its texture hashes. `add_model` writes all three (on the
skinned path, for every host group), resolved by the convention retail follows:

| shader | the input it follows | example |
|---|---|---|
| pixel | the material's texture count and which slots are non-zero | `3:011` → `PgDiffRefractNormFP` |
| vertex | the group's sub-object kind (`MESH`, `SKIN`, `TINY`), its vertex declaration's `usage.index` elements in order, and its materials' one pixel shader | `MESH`, `0.0+5.0+3.0+6.0`, `PgDiffSpecNormFP` → `PgMeshNoColorVP` |
| shadow | the kind, the main vertex shader, and whether any of the group's materials has an on-disk flag in `0x0B` (`alpha`) or none has (`opaque`) | `MESH`, `PgMeshNoColorVP`, alpha → `PgMeshTexShadowVP` |

The convention is the census of every retail model (`crates/mercs2_quartermaster/data/shader_import_rules.tsv`,
recomputed from `vz.wad` by the game-gated test `shader_import_census`): 4,407 model containers,
the 1,400 finer LOD rungs among them read against their resident container's materials, giving
125,841 observations. An input retail gives one shader resolves to it. Where retail uses several
shaders for one input, nothing in the model says which, and `qm` stops and names the choices:

- **pixel** — only `3:011` has one choice. A material with a diffuse map alone has six
  (`PgDiffFP`, `PgFastFP`, `PgDiffAmbOccFP`, …), one with diffuse and a second map eleven, and one
  with diffuse, specular and normal maps fifteen (the `Refl`, `Metal`, `SSS`, `AmbOcc` and `Rim`
  variants of `PgDiffSpecNormFP`). The rest of the model narrows them without deciding: `AmbOcc`
  tracks the group's vertex colour (usage 10.0) for every non-`Fast` shader (7,224 of 7,224
  groups), and the best deterministic rule over the texture set, the vertex colour, the flags and the
  whole float preamble still names the wrong shader for 598 of 4,907 `1:1` materials (1,457 by the
  texture set alone), 4 of 500 `2:11` (81) and 129 of 2,377 `3:111` (1,113). What stays open is Rim
  or not, `Fast` or `Diff`, and `Metal` or `Refl`. So a material whose input has several choices
  **must** declare its pixel shader; the build error names the host material, its input and the
  candidates. The host's own key is never kept.
- **vertex** — 48 of 58 inputs have one choice. The rest differ by `AmbientWind` (`MESH` / `SKIN`
  with vertex colour) or `_Ruin` (`TINY`); both are kept from the host (below).
- **shadow** — 42 of 42 inputs have one choice: a flag in `0x0B` on any of the group's materials
  selects the `Tex` shadow shader (57,083 of 57,083 groups).

Declare a shader in the glTF's `extras`, by registration name: `pixel_shader` on a material,
`vertex_shader` and `shadow_vertex_shader` on a mesh or a primitive. One declaration applies to
every host material or group; two different declarations of one key are an error. A declaration
takes precedence over the convention.

```json
"materials": [{ "extras": { "pixel_shader": "PgDiffSpecNormFP" } }],
"meshes": [{ "extras": { "shadow_vertex_shader": "PgMeshShadowVP" }, "primitives": [...] }]
```

A declared or resolved name must be registered in every configuration: a retail registration, or
an [`add_shader`](#add_shader) of this Shipment (at `qm link`, also of a Shipment it requires). A
pixel name that is not is **M0235**, a vertex name **M0236**. The main vertex shader must read only
inputs the group's vertex declaration supplies (**M0238**): each `dcl` input's usage and index, in
every store that holds the shader, must be an element of the declaration. The host's vertex
declaration is kept, so a vertex shader that reads vertex colour (usage 10) cannot draw a group
without one.

##### `POSITION.w`: AmbientWind and TINY hosts

Two kinds of vertex shader read `POSITION.w`, and `add_model` writes it for every vertex it injects:

| host group's vertex shader | what `POSITION.w` holds | where it comes from |
|---|---|---|
| **AmbientWind** — its `CTAB` declares `WindMatrix` (`PgMesh*AmbientWind*`, `PgSkin*AmbientWind*`, `PgMeshVPFastAmbientWind`, …) | the vertex's sway weight: the shader scales its `WindMatrix` sway by it (`mad r0, v0.w, r0, r2` in `PgMeshVPAmbientWind_3.sho`); 0 is still, 1 is full sway | the glTF vertex attribute `_SWAY_WEIGHT` |
| **TINY** (`PgMeshTinyVP` intact, `PgMeshTinyVP_Ruin` ruined) | the slot of the world object the vertex belongs to: an index into the container's top-level `TINY` id list (`u32 N`, then `N` GUIDs) and into `ObjectIDScaleArray`, whose entry is the object's state | the glTF vertex attribute `_TINY_SLOT` |
| any other | `1` | — |

- **AmbientWind.** A host group drawn by an AmbientWind shader keeps it (the census cannot choose
  between the wind and plain shader). Every primitive must carry `_SWAY_WEIGHT`; a mesh without it
  under an AmbientWind shader is **M0238**, naming the attribute. A declared non-wind
  `vertex_shader` on such a host draws it with that shader and writes `POSITION.w = 1`. The wind
  `.sho` loads when caps bit 2 (the vertex-texture capability) is set; otherwise the same name loads
  a plain `.sho`, which ignores `POSITION.w`.
- **TINY.** A `TINY` far-LOD host group keeps its vertex shader, so it keeps its role: the intact
  shader draws a vertex while its object's state is 1 (intact, drawn), the `_Ruin` shader while it is
  3 (ruined, drawn). A declared `vertex_shader` other than the host's is **M0236**. Every primitive
  must carry `_TINY_SLOT`, each value a slot of the host container's id list (`0` to `N−1`) and the
  same on a triangle's three vertices (retail holds this on every triangle); a mesh without it is
  **M0238**, naming the attribute and the host's slots. A slot `≡ 3 (mod 4)` is **M0248**: the
  shaders read it as `2·.w − .y` of its register ([`add_tiny_geometry`](#add_tiny_geometry)).

The attributes follow glTF's rule for application-specific attributes (a leading underscore):

| attribute | accessor | values |
|---|---|---|
| `_SWAY_WEIGHT` | `SCALAR`, `FLOAT` | 0 to 1 |
| `_TINY_SLOT` | `SCALAR`, `UNSIGNED_BYTE`, `UNSIGNED_SHORT` or `UNSIGNED_INT`, not normalized | a slot of the host's `TINY` id list |

Every triangle primitive carries an attribute or none does. When a dense rigid mesh is decimated to
fit the u16 strip, a merged vertex takes the mean `_SWAY_WEIGHT` of the vertices it merges, and only
vertices sharing a `_TINY_SLOT` merge.

### `retarget:` — the SKINNED path (`add_model`, `add_outfit`)

Without `retarget:` a model is lowered RIGIDLY: hosted on the donor with empty joints. Correct for
a prop, wrong for anything that animates. `retarget:` selects the skinned lowering, which re-poses
the source rig onto the donor's and writes palette-relative `BLENDINDICES` plus the matching
`INFO(56)` range table — the shipped skinning format. The mesh is split one donor draw group per
source part, the largest groups first; an `add_model`'s `group:` puts its first part in that group
instead ([`add_model`](#add_model)).

```yaml
    retarget:
      from: valve          # cod | valve | mixamo | unreal | pandemic | generic
      bones:               # optional; the RESOLVED map
        ValveBiped.Bip01_Spine2: Bone_Chest
        ValveBiped.Bip01_L_Calf: Bone_LShin
        SomeHelperBone: ~   # `~` drops the bone
```

`from:` is documentation and a sanity check, not the instruction — detection runs from the bone
names in the file, and a mismatch is reported.

`bones:` is the reproducibility record. Omit it and the build derives the map itself from the
convention tables, which is the same map the Workshop previews; supply it and a rebuild by someone
else reproduces exactly what the author approved, including any hand adjustment. A stale entry is
an error, not something to skip.

**Do not hand-write it.** `mercs2_workshop --export-shipment <model.glb> --rebind-target <donor>`
writes the whole Shipment — manifest plus `src/` — with the map it actually used.

### `textures:` (`add_outfit`)

```yaml
    textures:
      diffuse: src/skin_dm.png
      specular: src/skin_sm.png
      normal: src/skin_nm.png
```

Each map becomes its own resident texture block, and every donor hash at that MTRL slot is
repointed onto it. **Needs `retarget:`** — `add_outfit`'s rigid path performs no repoint, so an
outfit without it wears the donor's skin whatever is listed here, and the build refuses rather than
substituting. (`add_model`'s rigid path does repoint; see [`add_model`](#add_model).)
PNG only, 8-bit, dimensions a multiple of 4.

Slot order is `0 = diffuse, 1 = SPECULAR, 2 = NORMAL` — not the intuitive d/n/s. Normals are
BC3/DXT5nm; diffuse and specular are BC1 unless the source carries real alpha. A repoint that
matches nothing fails the build: the texture would ship with nothing referencing it.

### `single_group:` (`add_outfit`)

```yaml
    single_group: true
```

Forces the whole mesh into ONE donor draw group, wearing the source's OWN retargeted weights.
Use it for a **dense foreign-rig** import — one whose weights reference more than ~48 distinct
bones (a `donor_transfer` resample of the retail rig easily does). Above that ceiling an outfit is
otherwise split across several host draw groups (the multi-group balanced split), where the injector
fills a few and neuters the donor's others — a donor-structure-dependent layout a foreign-rig
character has been observed to render unstably on (it culls / teleports as the camera rotates).
`single_group` takes the proven single-host path instead: it skips donor-weight resampling (the
thing that inflates the bone count), so the conform's own weights — limbs mapped 1:1 by the
convention table, fingers folded to the hand — fit one group.

What it does with textures (no manual `textures:` needed): the build reads the GLB's OWN embedded
per-material diffuse maps, bakes them into a single atlas, remaps each part's UVs into its atlas
cell, and repoints the donor's diffuse slot onto it — plus a flat matte specular and a flat normal
so the donor's maps don't mis-light the atlased UVs. One draw group carries one material, so this is
the price of `single_group`: no per-material maps and no donor-resampled limb polish. The trade buys
placement stability, which is what a foreign-rig outfit needs first. A manual `textures:` block, if
present, overrides the auto-atlas.

### `replace_texture`

Same hash, fully resident, so your image must match the target's dimensions — a mismatch is a hard
error rather than something the builder can quietly rescale.

Two warnings are worth understanding before you pick a target:

- **M0007** — the texture *streams*. Its row names finer LOD rungs held in other blocks, and a
  fully-resident replacement stops those from being named. Hero textures (`pmc_hum_*`) are already
  single-block and are unaffected; most world textures are not.
- **M0009** — the texture has no primary ASET row of its own; retail carries it as a shared
  sub-entry. Replacing it mints a primary row, which is what makes your change take effect — and
  also means every asset sharing that texture now gets your version.

Neither blocks a build. Both change what your mod affects.

**A `target` of `vfx`** (`0x89E211AF`), the atlas every particle samples, repaints it: the repaint is
the base the set's `add_fx_sprite` images are drawn on, written into the resident block with them,
and one Shipment of a set repaints it ([`add_fx_sprite`](#add_fx_sprite)).

### `add_animation`

Adds a new Havok animation clip, as an `animation` asset (ASET type 16) under `name`.
`replace_animation` takes the same sources and swaps a shipped clip in place, same hash (`target`
instead of `name`).

```yaml
  - kind: add_animation
    name: my_wave
    clip: src/anim/my_wave.hkx      # Havok 5.5 packfile, one hka*Animation
    trnm: src/anim/my_wave.trnm     # the track → bone binding
    events: src/anim/my_wave.evnt   # optional: timed events
```

The build assembles the container every retail clip uses — `info` (`01 00`), `data` (the `clip`
bytes), `trnm`, and `evnt` when `events` is given, packed with its CSUM — and ships each source
verbatim. The layout is in [`anim_clip_format.md`](../anim_clip_format.md), including the `evnt`
format:

```text
[u32 count]  then per event:  [f32 seconds] [name, NUL-terminated ASCII] [category, NUL-terminated ASCII]
```

Retail's events are sound cues, voice lines and gameplay markers (`opendoor`), with categories such
as `sound`, `sound_surface`, `vo` and `camera`, or none.

- `clip` is a Havok 5.5 packfile (magic `57 E0 E0 57 10 C0 C0 10`) as the Havok content tools write
  it.
- `trnm` is `[u16 count][u16 flags][u32 lead][count × u32 bone name-hash]`, the hashes being HIER
  bone name-hashes of the rig the clip drives.

**M0213** checks that the three belong together before anything is built, and the build checks it
again: the clip must be a packfile with a readable `hkaAnimation`; the `trnm` must be exactly
`8 + 4·count` bytes with `count` equal to the clip's `numTransformTracks`; the events must parse,
with finite, non-negative times in non-decreasing order. An event may fire after the clip's
duration — retail does.

`replace_animation` needs the game stack: its target must be a shipped **Havok clip**. 29 retail
`animation` assets are `MANM` keyframe animations instead, and a replace naming one is refused —
these sources cannot express one. Omitting `events` on a replace ships the clip with no `evnt`,
whatever the clip it replaces had.

### `add_shader`

Registers new shaders in the engine's shader registry and adds their bytecode to the shader stores.

```yaml
load:
  requires:
    - capability: shader-registry        # the m2-sdk registers them at runtime (M0231)

contributions:
  - kind: add_shader
    family: pixel                        # the record family the engine constructs
    classes:                             # a pixel family: base, _pl, _sl, _pl_sl, in that order
      - { name: GlowFP,       stem: GlowFP,       shader: {asm: src/glow.asm},       shader_low: {asm: src/glow_low.asm} }
      - { name: GlowFP_pl,    stem: GlowFP_pl,    shader: {asm: src/glow_pl.asm},    shader_low: {asm: src/glow_pl_low.asm} }
      - { name: GlowFP_sl,    stem: GlowFP_sl,    shader: {asm: src/glow_sl.asm},    shader_low: {asm: src/glow_sl_low.asm} }
      - { name: GlowFP_pl_sl, stem: GlowFP_pl_sl, shader: {asm: src/glow_pl_sl.asm}, shader_low: {asm: src/glow_pl_sl_low.asm} }
  - kind: add_shader
    family: vertex
    classes:                             # a vertex family: exactly one
      - { name: GlowVP, stem: GlowVP, shader: {blob: src/glow_vp.vso}, shader_low: {blob: src/glow_vp_low.vso} }
```

- **`family`** is the record family the engine constructs for the shader: one of the 45 record
  vtables of the retail exe, which decides the registry (vertex or pixel), the record's size, and the
  constants the engine binds by name. The names and each family's constants are
  `crates/mercs2_quartermaster/data/shader_families.tsv`: `pixel` and `vertex` are the base
  families most shaders use; the others (`blur_pixel`, `water_vertex`, `scaleform_strip_pixel`, …)
  are the subclasses the engine's own subsystems construct
  ([`shader_store_format.md` §9](../shader_store_format.md#9-registration)).
- **`classes`**: exactly 4 for a pixel family, exactly 1 for a vertex family (**M0234**). A material
  names its pixel shader once and the draw adds the light class — 0 base, 1 `_pl` (point light),
  2 `_sl` (spot light), 3 `_pl_sl` — to its registry index, so the four classes register one after
  the other. With the ShaderLevel setting off the engine registers class 0 alone and draws every
  material with it.
- **`name`** is the name a material or primitive group keys the shader by (`m2` of it). It must be
  new: a retail registration's key, or another added one, is **M0233** (the registry keeps the first).
- **`stem`** is the store record the registration loads: `<stem>_3.sho` in `shader3.bin`,
  `<stem>_3l.sho` in `shader3Low.bin`. Classes may share a stem; then their sources must assemble to
  the same bytes (**M0230**) and one record is added.
- **`shader`** is the `shader3.bin` bytecode and **`shader_low`** the `shader3Low.bin` bytecode, which
  the engine loads with ShaderLevel off. Both are required.

A source is `{asm: <path>}`, SM3 assembly in `sm3asm`'s syntax, or `{blob: <path>}`, a compiled
`vs_3_0` / `ps_3_0` blob whose disassembly must assemble back to the same bytes. Either way the
bytecode must be a whole token stream of at most 0x8000 bytes with a `CTAB`, and its version token
must be the family's stage (**M0230**). Every constant its `CTAB` names must be one the family's
binder resolves, or nothing sets it (**M0237**).

The shaders register at runtime, not from the WAD: `qm build` writes `_build/<shipment>.shaders.h`,
one `m2_shader_class` table and family enumerator per `add_shader`, and the Shipment's own ASI queues
them from `DllMain` with the m2-sdk's `m2_shader_add_pixel` / `m2_shader_add_vertex`. The m2-sdk
registers them right after the game's own shaders. Because nothing registers them without it, an
`add_shader` Shipment requires the `shader-registry` capability (**M0231**), which the m2-sdk
Shipment provides.

### `replace_shader`

Replaces the bytecode of a shipped shader's store records, in place.

```yaml
  - kind: replace_shader
    target: PgMeshVP                     # the registered .sho stem (PgMeshVP.sho)
    shader: {asm: src/mesh_vp.asm}       # its shader3.bin record, PgMeshVP_3.sho
    shader_low: {asm: src/mesh_vp_low.asm}  # its shader3Low.bin record, PgMeshVP_3l.sho
```

`target` is a stem a retail registration loads (the `.sho` name without `.sho`; the logical name can
differ — `PgMeshNoTangentVP` loads `PgMeshVPNoTangent.sho`). `shader` replaces its `shader3.bin`
record. `shader_low` replaces its `shader3Low.bin` record and is required exactly when the stem has
one there. Each source's stage must be its record's (**M0232**). The record keeps its id and position.

### Shader stores and `--original-data`

The shader kinds edit the game's stores, `data/shader3.bin` and `data/shader3Low.bin`. `qm build`,
`qm link` and `qm lint --with-game` read the originals only from the directory `--original-data`
names, never from the game folder, whose copy is whatever the last deploy left there: a Shipment
with a shader kind and no `--original-data` is an error. The game's `shaderVT*.bin` and
`shaderR2VB*.bin` pairs are read from its `data` folder for one purpose: an added record's id must
be free in every set of stores the engine loads together — both stores plus the VT pair, or both
plus the R2VB pair — and each set must stay below the engine's 0x1200-slot id table (**M0233**).

- `qm build` applies the Shipment's edits in contribution order and writes `_build/data/shader3.bin`
  and `_build/data/shader3Low.bin`, each a `data_file` placement whose `base_sha256` is the sha256 of
  the original.
- `qm link` applies every installed Shipment's edits in load order, one store per file, emitted the
  same way. The load plan's `link_file_paths` lists both files when any Shipment in the set has a
  shader kind, and each item's `data_files` lists the files that Shipment edits: a deploy step drops
  the per-Shipment `data_file` placements of the files `link_file_paths` lists and deploys link's.

### `add_fx`

Adds a new particle effect and the world template that starts it.

```yaml
  - kind: add_fx
    name: qm_fx_cyan_burst               # the effect's name; its asset hash is m2 of it
    effect: src/qm_fx_cyan_burst.yaml    # the effect form (YAML, JSON or TOML)
    template:                            # the template form, inline
      name: qm_cyan_burst                # what Pg.Spawn and ObjectState.StartEmitter look up
      name_flag: 1
      components:
        EffectTemplate:     { "0xB9D95A23": "0x00000000" }
        RedEffectComponent: { name: qm_fx_cyan_burst, "0x4D7D459B": 1.0, ... }
        ...
```

- **The effect** is declared whole in the effect form (`mercs2_quartermaster::effect`): the `EMTR`
  shape tables, every emitter (its `TRFM` 4×4, all nine channels, its `GEOM` or `none`, the `PTYP`
  flags, all 32 attributes, exactly 100 colour keys, each `{rgba, half}` with `rgba` red, green,
  blue, alpha, and at least one frame) and every force (its kind and parameters, and its seven
  common attributes plus the kind's own). Every attribute is given by name or `0xHHHHHHHH`, each
  with `value`, `curve` (`none` or `[time, value]` keys) and `options` (a list of `bit7`,
  `resample`, `bit9`). Nothing has a default. The writer's rules are
  [`effect_container_format.md`](../effect_container_format.md) §8.
- **An emitter spawns on a shape.** Its `geom` is `{shape, word}`: `shape` names one of the
  effect's shape tables, which has at least one record, and `word` is how many of its records the
  engine samples, from 1 to the table's record count. Each record is a triangle a particle starts
  on: `|A × B|`, a unit vector along `±(A × B)`, the vertex `P`, the edges `A` and `B` (13 floats)
  ([`effect_container_format.md`](../effect_container_format.md) §2.1). An emitter with `geom: none`
  has a constant `rate` at or below 0 and a `ratevar` of 0, so it spawns nothing; any other is
  **M0309**.
- **A frame** is the key of a record in the game's `fxdict`, or a sprite of this Shipment or of one
  it requires ([`add_fx_sprite`](#add_fx_sprite)): the loader looks each one up in the `fxdict` and
  draws the record's rectangle of the `vfx` atlas
  ([`effect_container_format.md`](../effect_container_format.md) §2). Any other frame is **M0259**.
- **The template** is declared field by field in the template form
  ([`worldentity_container_format.md`](../worldentity_container_format.md) §6): every component the
  template carries, each with every field of its class, and no `Name` (that is `name` and
  `name_flag`). It has exactly one `RedEffectComponent` (**M0255**), whose `name` field names the
  effect it starts: this Shipment's or one the game ships (**M0259**).

The effect is appended to the game's effects block under `m2(name)`; a name the game already has is
**M0258**. The template is appended to the game's `worldentity` under the key its name derives —
`0x8` in the top nibble, the low 28 bits of `m2(template.name)` below it — and a name or key the
game or another added template already has is **M0257**. Both blocks are emitted at the game's own
paths and merged across the installed set by `qm link` ([below](#effects-and-templates-are-merged)).

### `replace_fx`

Edits an effect the game ships, in place.

```yaml
  - kind: replace_fx
    target: { template: global_particle_fire_carhood }   # or { effect: global_explosion_c4 }
    edits: src/carhood.edits.yaml
```

```yaml
# src/carhood.edits.yaml
edits:
  - { op: colour_rgb, emitter: 0, rgb: [255, 255, 0] }
  - { op: attribute, emitter: 0, attribute: size, value: 2.0, curve: none }
```

- **`target`** is `{effect: <name or 0xHHHHHHHH>}`, or `{template: <name>}` for the effect a template
  starts: the template's one `RedEffectComponent` names it. A target that resolves to no effect is
  **M0260**: no such effect or template, or a template with no `RedEffectComponent` or more than one.
- **An effect or template another Shipment's `add_fx` adds** is a target only when this Shipment
  lists that Shipment in `load.requires`; it is applied after that Shipment. Anything else is
  **M0260**. Built on its own, a Shipment that requires another leaves a target it cannot find in the
  game to `qm link`, which resolves it against the Shipments it requires.
- **`edits`** is an ordered list of operations, each tagged by `op`:

| `op` | fields | what it does |
|---|---|---|
| `attribute` | `emitter`, `attribute`, any of `value`, `curve`, `options` | edits one `PTYP` attribute |
| `channel` | `emitter`, `channel`, any of `value`, `curve`, `options` | edits one `TRFM` channel |
| `colour_rgb` | `emitter`, `rgb` (red, green, blue) | sets the red, green and blue of all 100 colour keys; each key keeps its alpha and its `half` |
| `colour_keys` | `emitter`, `keys` (100 `{rgba, half}`, `rgba` red, green, blue, alpha) | replaces the colour keys |
| `frames` | `emitter`, `frames` | replaces the frames |
| `transform` | `emitter`, `transform` | replaces the `TRFM` 4×4 |
| `flags` | `emitter`, `flags` | replaces the `PTYP` flags |
| `geom` | `emitter`, `geom` (`{shape, word}` or `none`) | replaces the emitter's `GEOM` |
| `force_params` | `force`, `params` | replaces a force's parameters; the kind keeps its attributes |
| `force_attribute` | `force`, `attribute`, any of `value`, `curve`, `options` | edits one `FRCE` attribute |
| `shape` | `shape`, `records` | replaces one shape table |
| `add_emitter` | `at`, `emitter` (as the effect form writes one) | inserts an emitter before index `at`; `at` equal to the count appends |
| `remove_emitter` | `emitter` | removes an emitter |
| `add_force` | `at`, `force` | inserts a force |
| `remove_force` | `force` | removes a force |
| `add_shape` | `at`, `records` | inserts a shape table; every `GEOM` naming a shape at or after `at` moves up one |
| `remove_shape` | `shape` | removes a shape table no `GEOM` names; every `GEOM` naming a later shape moves down one |

In an attribute edit an absent `value`, `curve` or `options` keeps the attribute's own, and `curve:
none` removes its curve. An edit that names a node the effect does not have, or that leaves the
effect breaking a rule of the effect writer, is **M0261**. An empty list is **M0254**.

### `add_fx_sprite`

Adds a sprite frame effects draw: an image drawn into the `vfx` atlas, and the `fxdict` record that
names its rectangle.

```yaml
  - kind: add_fx_sprite
    name: qm_fx_a_ring                   # the frame key is m2(name); an effect's frames name it
    image: src/qm_fx_a_ring.png          # a PNG; width and height each a power of two, 4 to 512
```

- Every particle samples one texture, the `vfx` atlas `0x89E211AF` (2048² DXT5, 10 mips, in the
  resident block), inside the rectangle the `fxdict` record of its frame gives
  ([`fxdict_format.md`](../fxdict_format.md) §3.2, §3.4). A sprite is texels in the atlas and a
  record.
- **The image** is a PNG with straight (unpremultiplied) alpha: the effect pixel shader multiplies
  the texel by the particle's colour and writes `rgb × a`. Its width and height are each a power of
  two from 4 to 512 (**M0304**).
- **`name`** is hashed into the frame key, and an effect's `frames` name the sprite by it. A name that
  is empty or written as `0xHHHHHHHH` is **M0305**; a key the game's `fxdict` already has, or another
  Shipment's sprite has, is **M0306**.
- **Who may draw it.** An effect draws its own Shipment's sprites and those of the Shipments it lists
  in `load.requires`; another Shipment's sprite is **M0259**. Built on its own, a Shipment that
  requires another leaves a frame it cannot find to `qm link`, which has the Shipments it requires.

**Where a sprite goes.** The sprites are drawn into the free square of the base atlas: the largest
square, aligned to its own side, that no `fxdict` record lies over and whose every texel has alpha 0;
the first in row-major order among squares of that side. On the game's atlas it is the 512² square at
(1536, 0). Each sprite is allocated the square of its larger side, and the set's sprites are placed
largest allocation first, then by key, each into the smallest free quarter-split cell that holds it,
at the cell's top-left: a sprite's x is a multiple of its width and its y of its height. The
placement depends on the set alone, not on its load order. Sprites that need more than the square
holds are **M0307**, which lists every sprite and the space left. A sprite's record is
`u = x / 2048`, `v = 1 − (y + h) / 2048` (`v` is measured from the atlas's bottom), `w / 2048`,
`h / 2048`.

**Repainting the atlas.** A `replace_texture` of `vfx` (or `"0x89E211AF"`) repaints the whole atlas,
so its image is 2048²; the repaint is the base the set's sprites are drawn on. The free square is
found on the repaint, so a repaint that leaves no transparent square free of records leaves every
sprite **M0307**. One Shipment of a set repaints the atlas: two are **M0207** (within one Shipment,
**M0120**). The repaint is written into the resident block with the sprites
([below](#sprites-and-the-vfx-atlas-are-merged)), not as a block of its own.

### `add_sound`

Adds a new sound bank. `Sound.CueSound("<cue name>")` plays one of its cues once the bank is
loaded.

```yaml
  - kind: add_sound
    bank: mymod_sounds         # the bank name; every table of the bank carries m2(bank)
    category: ui               # the category of every cue's group (M0216)
    load_in: [gameplay, front_end]   # where the bank loads: one or both, each once (M0221)
    cues:
      - name: mymod_click      # Sound.CueSound("mymod_click")
        wave: src/click.wav
        group_gain_db: -4
        cue_gain_db: -6
        pitch_semitones: 0
        positional: false
        min_distance: 10
        max_distance: 1000
        distance_exponent: 1
        doppler_scale: 1
        start_limit: 0
        sound_id: 0x1C2B3A49
        priority: 0.95
        group_20: 1
        cue_16: 0
        clip_hash: 0x1C2B3A49
```

The build encodes the bank's three tables — soundbank (ASET type 21, type hash `0x9F8BCA10`),
sounddb (13, `0xE5273C14`) and wavebank (6, `0xF753F6D0`) — as three entries of one block under
`m2(bank)`, at `blocks\VZ\mod_<hash>.block`, each with one primary ASET row, the shape of every
retail bank ([`audio_code_map.md` §11.1](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#111-conventions-and-packaging)).
Each cue becomes one embedded wave, one single-wave group and one single-track cue at the cue's
index, and one sounddb entry routing `m2(name)` to it (§11.6).

The bank plays only once it is loaded, and retail Lua loads banks by name. `load_in` names the
sessions whose loader loads it, and the block ships to each one's WAD:

| `load_in` | the loader | the block ships in |
|---|---|---|
| `gameplay` | `qm_modloader`, loaded at qm's `world.begin` hook (player-visible gameplay starting, once per world) and unloaded at `world.sound_exit`, after `MrxSoundBootstrap.ExitGame` | the Shipment overlay |
| `front_end` | `qm_shell_modloader`, loaded at qm's `shell.open` hook, after `MrxSound.EnterShellState` (the front-end shell opening, at its "Press Start" screen, before the main menu), and unloaded at `shell.sound_exit`, after `ExitShellState` ([The front-end loader](#the-front-end-loader)) | the shell patch |

`load_in` is required: it lists at least one session, each once (**M0221**). Each loader runs
`MrxSoundBanks.LoadWaveBank(bank)` then `MrxSoundBanks.LoadSoundBank(bank)`, the calls
`mrxsoundbootstrap.lua` makes for the game's own banks. Loading a soundbank also requests the
sounddb of the same name (`0x00602768`–`0x006027A1`), which is what makes FindCue see the cues. A
`qm build` of a Shipment with an `add_sound` links the loaders, so it needs the game stack and the
Lua corpus.

A cue named like a cue the game already has never plays: FindCue answers from the first loaded table
that has the guid, which is the game's (**M0220**). To change a game cue, use
[`replace_sound_cue`](#replace_sound_cue). A bank named `vo_*` is refused (**M0215**): retail Lua
appends the language to such a name before loading it. So is a bank or cue written as a bare hash:
a new identity's hash is the hash of its name.

### `load_sound`

Has qm's loader load sound that the game or this Shipment already ships, in a session that does not load
it: a DLC level's voice banks in the main campaign, for example.

```yaml
- kind: load_sound
  soundbank: vo_resident_DLCTest
  wavebank: vo_stream_DLCTest
  stream: vo_stream_DLCTest
  in: [gameplay]
```

- `soundbank`, `wavebank`: names as `MrxSoundBanks` takes them. A `vo_` name is localized as retail
  localizes it, `<name>.<language>` (`mrxsoundbanks.lua:80-87`), so the bank must exist for every
  language whose WAD is installed. Each must be held by the game, the language's WAD, or the Shipment's
  own `level: vz` rows (**M0223**).
- `stream`: one of this Shipment's [`add_level`](#add_level) streams, without `.pws` (for a `vo_` stream,
  without `.<language>.pws`). It is opened from where that level installs it, `DLC\<level>\Data\Audios`,
  by the path the DLC opens its own streams by (`dlctest01_soundbootstrap.lua:186-193`), under the alias
  `<stream>.pws`, before any bank of the session loads, and closed after they unload.
- `in`: the sessions whose loader loads them, `gameplay` and/or `front_end`, each once.

They load at the session's settle hook (`world.begin` in gameplay, `shell.open` in the front end) and
unload at its sound exit, as an `add_sound` bank does. A name written as a hash, a blank name, no name
at all, an empty or repeating `in`, and a stream no `add_level` of the Shipment installs are **M0222**.

### Sound cue fields

Every field is required; none has a default. A cue is one PCM16 wave played by one single-wave group
through one single-track cue — the shape of retail `ui_PDA_Open_01_st` (cue 57 → group 70 of
`ui_hud`). Offsets are into the group, the cue or the wave record of
[`audio_code_map.md` §11.4–§11.5](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#114-soundbank). In YAML a
`u32` field takes a decimal or an unquoted `0x` hex integer.

| field | type | written to | what the engine does with it | evidence |
|---|---|---|---|---|
| `name` | string | cue `+0x00` guid = `m2(name)`, or the bare hash itself in an override ([Sound overrides by name or hash](#sound-overrides-by-name-or-hash)); the sounddb entry's guid | `Sound.CueSound(name)` looks up `m2(name)` | FindCue `FUN_00835a70` (PROVEN) |
| `wave` | `src/` path | wave record and its embedded PCM16 data | the samples the wave plays | the strict reader `mercs2_audio::wav::read_pcm16_wav`: RIFF/WAVE, format 1, 16 bits, 1 or 2 channels, rate above 0, whole frames (M0214) |
| `group_gain_db` | number, dB | group `+0x2C`, as the f32 `10^(dB/20)` | base volume of the sound instance | `FUN_0083d770` (PROVEN); −4 dB gives `0x3F21866C` and −6 dB `0x3F004DCE`, retail `ui_PDA_Open_01_st`'s bits |
| `cue_gain_db` | number, dB | cue `+0x08`, as the f32 `10^(dB/20)` | multiplies the cue's volume each frame | `FUN_00835060` (PROVEN) |
| `pitch_semitones` | f32 | group `+0x30` | base pitch of the sound instance, in semitones | `FUN_0083d700` (PROVEN) |
| `positional` | bool | group `+0x14` (1 / 0) | `true`: with an emitter, the instance plays from the emitter's own source, positioned; `false`: from the shared 2D source | `FUN_00837830`, `0x008378A9` (PROVEN) |
| `min_distance` | f32 | group `+0x18` | full volume up to this distance from the listener | `FUN_0083d3a0` (PROVEN) |
| `max_distance` | f32 | group `+0x1C` | silent from this distance | `FUN_0083d3a0` (PROVEN) |
| `distance_exponent` | f32 | group `+0x24` | exponent of the fall-off between the two distances | `FUN_0083d3a0` (PROVEN) |
| `doppler_scale` | f32 | group `+0x28` | how much of the Doppler shift applies | `FUN_0083b120` (PROVEN) |
| `start_limit` | u8 | cue `+0x06` | the cue starts only while fewer than this many instances of it are playing; 0 starts it every time | `FUN_00834ad0` (decomp 627053–627066) compares it with a counter in the cue's runtime record that `FUN_008354e0` raises when an instance plays and `FUN_00835850` lowers when one finishes (PROVEN) |
| `sound_id` | u32 | group `+0x00` | one reader: for the ids `0xEA1343AA`, `0xC05D8686` and `0xBB8AE67D` nothing plays unless the game runs in English; any other value has no known effect | `FUN_008369e0`, `0x00836A27`–`0x00836A45` (PROVEN); no other reader in the Pal code `0x0082A000`–`0x00842000` (INFERRED, bounded search) |
| `priority` | f32 | group `+0x10` | voice-stealing priority: with every voice busy, a new instance takes the voice of the lowest-priority wave only when its own priority (this value times its distance volume) is higher; otherwise no wave is created for it | `GetWavePriority` `FUN_00837e10` (`0x00837EDF`), `FUN_00837830` (`0x00837A0C`) (PROVEN) |
| `group_20` | f32 | group `+0x20` | no reader known; carried as written | copied into the wave at `+0x68` (`0x00838F70`); the only reader of wave `+0x68` is the getter `0x00838F30` at vtable `+0x44` of both wave vtables, and it has no call site in `0x00828000`–`0x00842000` (INFERRED) |
| `cue_16` | u16 | single-track cue `+0x16` | no reader known; carried as written | the cue's `{bank, group}` reference is read at `+0x10` and `+0x14` only (`FUN_0082e7d0`, `FUN_0083d410`); the instance field that holds the reference (`+0x28`, written at `0x00836A05`) is otherwise only cleared (`0x00836BEE`) (INFERRED) |
| `clip_hash` | u32 | wave record `+0x00` | no reader known; carried as written | `FUN_00837830` reads the record at `+0x05`, `+0x06`, `+0x08`, `+0x0C`, `+0x14`, `+0x18`, `+0x1C`, `+0x20` and not `+0x00` (`0x00837883`–`0x0083789F`, `0x00837AB7`–`0x00837AF4`); no wave method reads the record pointer it stores as data (INFERRED) |

The retail values of each raw field are counted by the census tests in `mercs2_audio`'s
`tests/retail_fields.rs` (`group_sound_id_against_the_playing_cue_guids`, `group_priority_values`,
`group_word_20_values`, `cue_start_limit_values`, `single_track_cue_word_16_values`,
`wave_clip_hash_against_sound_id_and_cue_guid`) and listed in
[`audio_code_map.md` §11.4](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#114-soundbank).

The build derives the rest of each table:

- group `+0x04` = `m2(category)`, `+0x08` = 0, form 0 (single-wave), and its wave
  `{wavebank hash, wave index, weight 1.0}` — the engine does not read a single-wave group's weight
  (`FUN_0083d410` returns the wave at `+0x34` directly, decomp 633049–633051, PROVEN), and it is 1.0
  in every retail single-wave group (`single_wave_group_weight_is_one`);
- cue form 0 (single-track), its `{bank hash, group index}`, and its length `+0x0C` by §11.4's rule;
- the wave record's channels, format, rate, size, frame count and data offset, from the WAV;
- the sounddb, sorted by guid.

### `replace_sound_bank`

Replaces a bank the game ships, by the name the game's Lua loads it under.

```yaml
  - kind: replace_sound_bank
    bank: vo_mattias
    language: german           # vo_* banks only (M0217)
    category: vo
    cues:
      - name: MATTIAS_LINE_001
        wave: src/mattias_001.wav
        # … every sound cue field
```

The build encodes the bank's soundbank and sounddb from `cues` and ships both under the bank's entry
name — `bank`, or `<bank>.<language>` for a `vo_*` bank — so the game's own load of the bank reads
them. A cue of the game's bank that `cues` does not declare is gone from the bank; nothing checks
for it.

For a bank retail Lua loads, the waves go in a wavebank of their own, `qm_<shipment>_<entry>` (for
example `qm_my-mod_vo_mattias.german`), which the loader of each session that loads the bank loads
(`LoadWaveBank` only; [below](#where-a-sound-override-ships)). A retail wavebank can be shared between banks
([`audio_code_map.md` §11.4](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#114-soundbank)), and the `vo_*`
banks have none of their own (their waves stream from `vo_stream`, `mrxsoundbootstrap.lua:218-245`),
so the replacement's waves are not written into a game wavebank. The override wavebank's name does
not start with `vo_`, so Lua loads it under exactly that name.

For a bank the engine loads ([below](#banks-the-engine-loads)), the waves go in the bank's own
wavebank, after its retail waves, which stay at their indices because other banks may play them.

Where the tables ship is [below](#where-a-sound-override-ships). Two Shipments replacing one bank
conflict (M0207).

### `replace_sound_cue`

Replaces one cue of a bank the game ships, leaving every other cue of the bank as the game has it.

```yaml
  - kind: replace_sound_cue
    bank: ui_hud
    category: ui
    cue:
      name: ui_PDA_Open_01_st  # the cue it replaces
      wave: src/pda_open.wav
      # … every other sound cue field
```

The build forks the game's soundbank — or the replacement, when a `replace_sound_bank` of the same
entry is in the installed set — appends one single-wave group, and rewrites the cue as a
single-track cue playing it. The cue keeps its index, so the bank's own sounddb still routes to it
and is not shipped; every other cue and group is byte-identical. The wave goes where a
`replace_sound_bank`'s waves go: the override wavebank `qm_<shipment>_<entry>` for a bank retail Lua
loads, the bank's own wavebank after its retail waves for a bank the engine loads (which then ships
the game's sounddb beside it). The cue must be in the bank (**M0218**).

Two Shipments replacing different cues of one bank compose: `qm link` merges every
`replace_sound_cue` of the set into one soundbank per bank ([Sound banks are
merged](#sound-banks-are-merged)). One cue replaced by two Shipments conflicts (M0207).

A cue that picks among several waves (1,431 of the `vo_*` cues pick among 2–14 takes of a line)
becomes a cue of one wave. To replace one take and keep the others, use
[`replace_sound_wave`](#replace_sound_wave).

### `replace_sound_wave`

Replaces one line — one wave record of a wavebank the game ships — wherever it plays.

```yaml
  - kind: replace_sound_wave
    wavebank: vo_stream            # as the groups name it, or its entry's hash
    language: english              # vo_* wavebanks only (M0217)
    line: "0x96FFDF9F"             # the record's clip hash, or the name it is the hash of
    wave: src/new_line.wav
```

The build finds the wavebank's entry (`<wavebank>.<language>` for a `vo_*` one) in each WAD that
carries it, and the one record whose `+0x00` clip hash is `line`. In every soundbank of those WADs,
each group's reference to that record is pointed at the new wave, keeping its weight and every other
wave of the group, and every cue that reaches a moved reference gets its length `+0x0C` recomputed
by [the length rule](#sound-cue-fields). The forked soundbanks ship as a `replace_sound_cue`'s do;
the new wave goes where an override's waves go (the override wavebank `qm_<shipment>_<entry>` of each
bank retail Lua loads, the bank's own wavebank for a bank the engine loads), with the record's clip
hash.

The record cannot change in place: a wavebank streams or embeds by its own `+0x0A` kind, not per
record (`FUN_0082e8b0` returns that byte, and `FUN_0082e8f0` gives the `.pws` name only when it is
set), so a record of the streamed `vo_stream` cannot become embedded PCM.

Refused (**M0218**): a wavebank no WAD carries; no record, or several, with the line's clip hash; a
line no group plays (replacing it would change nothing — 34 of `vo_stream`'s 12,988 records); and a
cue that plays the line through a group of another soundbank, whose length its own bank's fork
could not recompute.

Which banks play a line is the game's answer, so the [load plan](#placementjson) of a set with a
`replace_sound_wave` reads them from the game folder's WADs (`--game`). Two
Shipments replacing one line conflict (M0207); `qm link` merges a line's banks with the set's other
cue and line overrides ([Sound banks are merged](#sound-banks-are-merged)).

### Sound overrides by name or hash

`replace_sound_bank`, `replace_sound_cue` and `replace_sound_wave` name a bank, cue, wavebank or
line the game ships, so each takes a name or a bare hash, quoted in YAML, like every other asset
reference ([Names and hashes](#names-and-hashes)). Most shipped audio has no known name — 41 of
`English.wad`'s 68 soundbank entries, and the only `vo_*` cue of 9,850 voice-over lines.

- A bank or wavebank written as a hash is its **entry's** hash: for a `vo_*` one, the hash of
  `<bank>.<language>`. Whether it is a language's copy follows from where the game carries it: with
  a `language`, it is looked up in that language's WAD only; without one, in `vz.wad` and
  `shell.wad`. A hash the declared WAD does not carry is **M0218**.
- A cue or line written as a hash is its guid or clip hash itself.
- `add_sound` mints its bank and cues, and a new identity's hash is the hash of its name, so there a
  bare hash is **M0215**.

A hash with a known name gets **M0130**'s suggestion; for a localized entry the suggested name drops
the `.<language>` suffix.

### Where a sound override ships

An override's tables and its wavebank ship to every level that both carries the bank and loads it —
each level WAD runs its own Lua, and `shell.wad` and `vz.wad` are never mounted together. For a
bank retail Lua loads, that level's loader loads the wavebank; the engine loads the others itself:

| bank | read from | loaded in | the tables and the wavebank ship to |
|---|---|---|---|
| `vo_*` | the declared `language`'s WAD (`English.wad`, …) | gameplay | the tables to that language's patch (`language_patch/<language>.wad`, merged into `data/<language>-patch.wad`); the wavebank to the Shipment overlay |
| `ui_hud`, `music` | `vz.wad` and `shell.wad` | gameplay and the front end | both to the Shipment overlay and to the shell patch (`<shipment>.shell-patch.wad`, merged into `data/shell-patch.wad`) |
| `ui_shell` | `shell.wad` | the front end | both to the shell patch |
| any other bank `vz.wad` carries (the engine loads it) | `vz.wad` | gameplay | one block of the Shipment overlay, under the retail name ([below](#banks-the-engine-loads)) |

Where retail loads each bank
([`audio_code_map.md` §11.10](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#1110-where-banks-are-loaded)):

- **The front end** loads `ui_shell`, `ui_hud` and `music` in `MrxSound.EnterShellState`
  (`shell/mrxsound.lua:9-14`, called from `mrxguishell.lua:505`) — the only bank-load call site among
  `shell.wad`'s 28 scripts — and `shell.wad` carries exactly those three soundbanks.
- **Gameplay** loads 11 banks by name in `MrxSoundBootstrap.LoadBanks`
  (`resident/mrxsoundbootstrap.lua:195-245`, `ui_hud` and `music` among them) and the `vo_*` banks
  per language. `vz.wad` carries 76 soundbanks: those 11, `ui_shell`, and 64 that no Lua in the
  corpus loads by a literal name, which the engine loads in gameplay
  ([`audio_code_map.md` §11.11](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#1111-the-banks-the-engine-loads-soundeffect)).
- `vz.wad` also carries the front end's scripts and `ui_shell`, but its copy of
  `EnterShellState` runs only from `GameBootstrap.Start`, which returns at once once the main menu
  has handed over to the game (`Sys.FinishedShell()`, `gamebootstrap.lua:43-46`). So `ui_shell`
  loads in the front end only, and a `ui_shell` override ships to the shell patch alone.

`ui_hud`, `ui_shell` and `music` are byte-identical in both archives (`cue_guids_across_sounddbs`).
A bank in no carrier, or one no carrier loads (a `vz.wad`-only `ui_shell`), is **M0218**.

A shell patch is stamped with `shell.wad`'s CSUM row, the WAD it mounts above.

### Banks the engine loads

A bank is Lua-loaded when a retail load site names it (the front end's `EnterShellState`, gameplay's
`LoadBanks`) or its name starts with `vo_`; the sites are the corpus's literal `LoadSoundBank` calls
(`the_sound_load_sites_are_the_corpus_calls`). Every other bank `vz.wad` carries — the 38 `veh_*` and 26 `wpn_*` banks — is
loaded by the engine by its retail name: the `SoundEffect` component of a vehicle or weapon template
names the bank, and the engine requests its soundbank and wavebank
([`audio_code_map.md` §11.11](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#1111-the-banks-the-engine-loads-soundeffect)).
`wpn_grapplegun` is named by no template and ships the same way.

An override of such a bank ships its three tables in one block of the Shipment overlay, the shape
of the retail block,
`blocks\VZ\mod_<entry hash>.block`, each under `m2(<bank>)`:

- the soundbank — the game's with the cue overrides applied, or the replacement's;
- the sounddb — the game's for `replace_sound_cue`, the replacement's for `replace_sound_bank`;
- the wavebank — the game's, every retail wave byte-identical at its own index, then the override
  waves. Every cue of any bank that plays a retail wave still finds it: `veh_largedieselold` and
  `veh_largedieselnew` play waves of `veh_largegasold`'s wavebank.

The engine's request loads the soundbank and the wavebank; it does not request the sounddb. No
loader loads anything for the bank, and it claims no loader script. The build checks that every such
bank's block carries all three rows under the retail name; a missing row is a build error naming the
bank. What the override changes is what the engine loads for the local player's own weapon or
vehicle (INFERRED from the callers, which run on the local player's equipment and seat changes); AI
units are INFERRED to be unaffected.

### The front-end loader

The front end runs `shell.wad`'s own Lua VM, so the gameplay loader is not there. When any
registration loads a bank in the front end, the build links a second loader into `shell.wad`'s
scripts block (`blocks\Shell\resident_P000_Q3.block`, 28 scripts) and ships the block in the shell
patch, with every ASET row copied from `shell.wad`:

- **`qm_shell_modloader`** — a new script the Quartermaster mints into that block, which qm's
  front-end entry `shell.boot` loads with `dynamic_import`. Its body subscribes each bank to qm's
  `shell.open` hook, after retail's `EnterShellState`, and to `shell.sound_exit`, after retail's
  `ExitShellState` ([qm's hooks](#qms-hooks)), generated by the same code as the gameplay loader's:
  each bank's `LoadWaveBank` (and, for an `add_sound` bank, `LoadSoundBank`), in load order and then
  by bank name, each bank loaded once until it is unloaded (`_QM.sounds`). No callback is passed, so
  the batch callback retail's `EnterShellState` sets (`_StartShellMusic`) stands. The front end calls
  `EnterShellState` when the front-end shell opens (`mrxguishell.lua:505`): its first page is the
  "Press Start" screen, and the main menu follows inside the same Flash movie with no Lua call of its
  own (seen live 2026-11-10). It calls `ExitShellState` when the shell closes (`:589`) and when the
  shell exits (`mrxsoundshellbootstrap.lua:99`).
- **qm's front-end hooks**, spliced onto the retail bytecode of `shellbootstrap` (the entry) and
  `mrxsound` (the shell's two sound-state hooks).

A script minted into the shell patch's copy of the block is found like the shell's own: the engine
finds a script by the hash of its name and the script type through its typed asset lookup, with no
block or WAD in the key (`_SYS._IMPORT` = `0x005AE2D0`).

The gameplay loader works the same way in `vz.wad`: qm's world entry loads `qm_modloader`, which
loads each bank at `world.begin` (each bank once per session, `_QM.sounds`) and unloads it at
`world.sound_exit`, after `MrxSoundBootstrap.ExitGame` runs retail's `UnloadBanks`. The sound calls
run with no `pcall` and no existence check.

`MrxSoundBanks` reports no failure: its completion callback takes no argument
(`mrxsoundbanks.lua:141-152`) and the engine's carries no flag, and a `LoadWaveBank` of a name no
mounted WAD holds is released after a timeout with no error
([`audio_code_map.md` §11.2](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#112-resolving-a-cue)). So the build
checks it: every bank a loader loads must have its wavebank among the blocks shipped to that
loader's level, in `qm build` and in `qm link` (for the link, the blocks each Shipment's own build
ships); a bank without one is a build error naming the bank and the level.

### The `language` field

`language` names which language's copy of a `vo_*` bank a sound override replaces. Its values are
the engine's language table (`0x00CF281C`) without `english_uk` and `allcaps`, which the game selects
only from the command line and whose Lua `GetLanguage` reports English:

`english` · `spanish` · `italian` · `french` · `german` · `japanese` · `russian`

The table entry is the base name of the language's WAD (`.\Data\<entry>.wad`) and the suffix retail
Lua appends to a `vo_*` bank's name ([`eighth_language_wiring.md`
§10](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/eighth_language_wiring.md#10-fonts-atlases-and-the-engine-language-table)).
A bank that is not `vo_*` has one copy for every language and takes no `language` (**M0217**).

When a contribution declares a language, `qm` and the Workshop open `vz.wad` and then that
language's WAD from the same folder (every declared language, in table order); `add_language` adds
`English.wad`. A declared language whose WAD is not there is an error — the retail install ships no
`japanese.wad`.

## Composition

Two Shipments that touch the same thing must not silently produce one winner and one no-op. This is
the part of the format that exists for that.

### The engine gives no single answer

Four subsystems, four different rules, running at once:

| subsystem | resolution |
|---|---|
| WAD stack | last mounted wins |
| runtime chunk registry | **first** writer wins |
| string databases | last registered wins, capped at 8 |
| ASI plugins | no arbitration at all |

So "load order" is not a universal answer, and for ASI plugins there is no order that resolves a
collision at all.

### Merge classes

Every claim a Shipment makes carries a class:

- **`Exclusive`** — one claimant; a second is a hard error. Raw blocks, function redefinitions, HQ
  starters, anything opaque.
- **`KeyedSet { key }`** — union by key; a duplicate key is an error. New names key on the name.
- **`OrderedList`** — append-only, with companion values the Quartermaster derives rather than
  trusting.
- **`LastWins`** — later wins and load order is genuinely the answer. Texture replacement.

**Claimants of one target that disagree about its class take the strictest** — `Exclusive`, then
`KeyedSet`, then `LastWins`, then `OrderedList` — whatever order they come in.

**Unrecognized targets are `Exclusive`.** The Quartermaster knows the wardrobe list is append-only
because somebody reversed it and wrote it down; it cannot infer that for something nobody has
studied. Failing closed keeps an unknown edit *expressible* — it just cannot silently co-install.

### Conflict classes by kind

Two Shipments in one installed set conflict (**M0207**, a hard error before anything is built) when
they claim one target in a class that cannot be shared:

| kind(s) | what two Shipments on one target do |
|---|---|
| `replace_texture` | `LastWins` — load order picks; never a conflict. Of `vfx`: the atlas merges with every sprite, and the repaint is `Exclusive` — two Shipments repainting it conflict |
| `replace_animation`, `replace_phy2`, `replace_terrain_cell`, `edit_state_machine`, `edit_world` | `Exclusive` — conflict. An `edit_world` and an `add_placement` on one layer conflict too |
| `add_fx` | `KeyedSet` on the effect name, on the template name and on the template's derived key; the worldentity they are appended to merges ([below](#effects-and-templates-are-merged)). A `raw` claiming the worldentity `0x50075B3B` conflicts |
| `add_fx_sprite` | `KeyedSet` on the frame key (`m2(name)`); the `fxdict` and the `vfx` atlas merge ([below](#sprites-and-the-vfx-atlas-are-merged)). A `raw` claiming either conflicts, and so does an `add_texture` named `vfx` |
| `replace_fx` | `Exclusive` on the effect it resolves to, whether named or reached through a template: two Shipments editing one effect conflict. A template target is resolved by `qm link` against the game's worldentity and the set's additions, and a collision refuses the link as **M0207**. A `replace_fx` of an effect another Shipment adds composes with that `add_fx` when it requires that Shipment |
| `replace_shader`, `add_shader` | `Exclusive` on each store stem (`m2` of the stem, so case does not matter) and on each registration name: a replace and an add of one stem, two adds of one stem, and two adds of one name all conflict. The classes of one `add_shader` may share a stem |
| `patch_lua` (and the rows `add_ui`, `activate_layer`, `add_shop_item` append) | compose, on **any** script — see below |
| `add_outfit` | compose — `qm link` generates the wardrobe lists from every installed outfit ([below](#outfits-are-generated-by-the-link)) |
| `replace_lua` | `Exclusive` — conflicts with another `replace_lua` **and** with a `patch_lua` of the same script |
| `edit_stringdb`, `add_stringdb_keys`, `replace_stringdb_text` | compose — `qm link` merges every Shipment's writes to one table (below), in this Shipment and others |
| `add_*` minting a name (`add_model`, `add_movie`, `add_script`, …) | `KeyedSet` — the same new name twice is a conflict |
| `add_shop_item` with `behaviour.script` | `KeyedSet` on `behaviour.module`, the script it mints: two Shipments minting one module, or one minting it and another an `add_script` of that name, conflict. Items of one Shipment naming one module and one `script` mint it once |
| `add_placement`, `add_layer` | `KeyedSet` on the layer (`add_layer`: its `template`), whose whole block each ships — another Shipment's `add_placement`, `add_layer` or `edit_world` on it conflicts |
| `add_sound` | `KeyedSet` on the bank name and on each cue — the same bank or cue name added twice is a conflict |
| `load_sound` | compose — it ships nothing, and claims only the scripts of its sessions' loaders |
| `replace_sound_bank` | `Exclusive` on the bank's entry (`<bank>` or `<bank>.<language>`) and on every cue it declares |
| `replace_sound_cue` | `Exclusive` on the cue; replacements of different cues of one bank compose ([below](#sound-banks-are-merged)) |
| `replace_sound_wave` | `Exclusive` on the line; replacements of different lines, and of cues, in one bank compose ([below](#sound-banks-are-merged)) |
| `native_hook`, `place_file`, `add_runtime_dll` file names | `Exclusive` per game-folder path, **compared lowercased** (Windows file names are case-insensitive) |
| `native_hook` `touches` | `Exclusive` per hooked address or symbol, exactly as spelled (see the Code layer) |
| `add_tiny_geometry` | `KeyedSet` on the model; `Exclusive` on the layer's cell; `KeyedSet` on the layer, claimed by the first stand-in of each layer in a Shipment — another Shipment's stand-in, `add_placement` or `edit_world` on the layer conflicts, and one Shipment's stand-ins in several cells of a layer compose |
| `raw` | `Exclusive` on every declared target |

Within **one** Shipment, two contributions claiming one target in any class but an append are
**M0120**: only one of them can take effect.

A Shipment can also declare conflicts outright: `load.conflicts` names a Shipment, optionally within
a version range (`- { shipment: other-mod, version: "<2" }`). An installed match is **M0206**.

### String tables are merged

A Shipment's own build applies all of its `edit_stringdb` / `add_stringdb_keys` /
`replace_stringdb_text` contributions to one table **in contribution order**, each against the table
as edited so far, and ships the result as one copy of the table: an edit of a key the table (so far)
does not have, or an addition of one it has, is an error. So a Shipment can replace, by text, what its
own earlier `edit_stringdb` wrote.

Each Shipment's overlay carries a whole edited copy of its table, so installed together the last
mounted would silently drop the others' edits. `qm link` therefore merges every installed Shipment's
writes to one table into a single link-owned table — with the same code — applied in load order with
the later write winning:

- `edit_stringdb` / `add_stringdb_keys` go by key hash — a key that exists is overwritten, one that
  does not is added;
- `replace_stringdb_text` matches text in the table **as merged so far**, so it sees every earlier
  write. A pair that matches nothing there is an error naming the Shipment, the table and the text —
  in `qm link`, and in the Shipment's own build.

`replace_stringdb_text`'s `pairs:` file has one `old<TAB>new` pair per line; a line starting with `#`
is a comment and blank lines are skipped. The text is taken exactly as written (nothing is trimmed or
unescaped), so it can contain spaces, `:` and `=`. A line with no tab, more than one tab, or an empty
old text is an error naming the file and line.

One Shipment may fix a table by key and by text. Each copy of the table is merged on its own
([both copies](#edit_stringdb)): the link WAD carries the merged `vz.wad` copy and the link's shell
patch the merged `shell.wad` copy, at the same block path, and the load plan's `link_block_paths` names
that path once so a deploy step drops the per-Shipment copies of both.

### Sound cues are claimed by guid and language

A sound contribution claims each cue as `SoundCue { guid, language }`: `guid` is `m2` of the cue's
name (or the bare hash it is written as), and `language` is the declared language of a `vo_*`
override (none otherwise). Each language's `vo_*` banks are separate entries (`<bank>.<language>`),
so claims in different languages never conflict. A replaced line is claimed as
`SoundLine { wavebank, clip, language }`: the wavebank's entry hash and the record's clip hash.

| kind | claims | intent | class |
|---|---|---|---|
| `add_sound` | the bank (an asset) and each cue | additive | `KeyedSet` |
| `replace_sound_bank` | the bank's entry (an asset) and every declared cue | replace | `Exclusive` |
| `replace_sound_cue` | the cue | replace | `Exclusive` |
| `replace_sound_wave` | the line | replace | `Exclusive` |

An added cue name is a key across the set because FindCue answers from the first loaded table that
has the guid (`FUN_00835a70`): of two banks adding one cue name, one is never heard. A replaced cue
has one winner in the table the game loads.

Each sound kind also claims the scripts its loaders live in, additive, for each session it loads in
(an `add_sound`'s `load_in`; for an override of a bank retail Lua loads, the sessions retail loads
its bank in; an override of a bank the engine loads claims none): gameplay claims `qm_modloader`,
`vz` and `mrxsoundbootstrap` in `vz.wad`; the front end claims `qm_shell_modloader`,
`shellbootstrap` and `mrxsound` in `shell.wad` — the loader and the hosts of the hooks it loads at
and runs on. `add_ui` and `activate_layer` claim gameplay's. A script claim names its level, so the
front end's `mrxsound` is not `vz.wad`'s. Any number of sound Shipments share the loaders; a
`replace_lua` of a host a sound Shipment loads through (`replace_lua mrxsoundbootstrap` beside a
`ui_hud` override) is a conflict.

### Sound banks are merged

Each Shipment's build ships a whole soundbank per bank it overrides, and of several copies of one
bank the last mounted is the one the game reads. `qm link` merges every bank a
`replace_sound_cue` in the set targets, and every bank whose groups play a line a
`replace_sound_wave` replaces: per carrier (the overlay, the shell patch, each language patch), one
soundbank at `blocks\VZ\mod_<entry hash>.block`, starting from the set's `replace_sound_bank` of
that entry when there is one and the game's bank otherwise, with every `replace_sound_cue` and then
every replaced line applied in load order. The load plan's `link_block_paths` lists these blocks,
so a deploy step drops the per-Shipment copies; the banks a line reaches are read from the game. The link writes them to
`zz-quartermaster-link.wad` (overlay), `zz-quartermaster-link.shell-patch.wad` (shell patch) and
`language_patch/<language>.wad` (language patch), each recorded in `placement.json`.

The override wavebanks are not merged: each is named for its Shipment, so they never collide. A
bank the engine loads has no override wavebank: its merged block carries the game's sounddb (or the
set's replacement's) and one wavebank, the retail waves at their indices followed by each
Shipment's override waves in load order, each cue playing its own Shipment's wave.

The link also bakes one loader per session over the set: `qm_modloader` in `scripts_vz`, and
`qm_shell_modloader` in `shell.wad`'s scripts block. Every link splices qm's hooks onto both
sessions' hosts ([qm's hooks](#qms-hooks)), so it always writes `zz-quartermaster-link.wad` and
`zz-quartermaster-link.shell-patch.wad` (the front end's scripts block, with the merged banks when
there are any), and places its load plan at `scripts/load-plan.json`. `link_block_paths` always lists `blocks\Shell\resident_P000_Q3.block` after the
`vz.wad` scripts blocks, so a deploy step drops each Shipment's own copy of the front end's scripts
block and keeps the link's. After those it lists the blocks the scripts blocks carry as LOD rungs
(the resident block's rows name `resident-pmcoutpost_fountain_P001`..`_P003`), which ship wherever a
scripts block is re-emitted; they are read from the game's `vz.wad` and `shell.wad`, so every plan
reads the game folder (`--game`).

### Effects and templates are merged

The game ships every effect in one block, `blocks\VZ\effects_P000_Q3.block` (314 effects and the
46 models they draw), and every template in one container, the `worldentity` `0x50075B3B` in
`blocks\VZ\resident_P000_Q3.block`. A Shipment's own build applies its `replace_fx` and `add_fx` to
the game's copies and emits both blocks at those paths, every row copied from the game:

1. the effects block: the game's entries in block order, each `replace_fx` applied to its effect in
   place in contribution order, then each `add_fx` effect appended with a primary row of type id 29
   and sentinel rungs;
2. the resident block, when the Shipment has an `add_fx`: the game's, with each template appended to
   the `worldentity`, in the same block the Shipment's linked scripts go in.

Installed together, the last mounted copy of each block would drop the others' effects and
templates. `qm link` runs the same merge over every Shipment in load order — a Shipment's
replacements, then its additions, Shipment by Shipment — and emits one effects block and one
resident block, the latter carrying every linked script too. The load plan's `link_block_paths`
lists the effects block whenever a Shipment in the set has an `add_fx` or a `replace_fx`, and always
lists the resident block, so a deploy step drops each Shipment's copies and keeps link's. The two
blocks share nine asset hashes (models in the effects block, textures in the resident block); both
are link's, so they are one claim group.

A `raw` payload that carries an effect or the worldentity is **M0262**.

### Sprites and the `vfx` atlas are merged

The `fxdict` `0x86BF6C5B` and the `vfx` atlas `0x89E211AF` are both in
`blocks\VZ\resident_P000_Q3.block`. A Shipment's own build draws its sprites into its base atlas —
its repaint, else the game's — and writes the `fxdict`, with the game's records and its sprites', and
the atlas into the resident block, the same block its scripts and templates go in, every row copied
from the game.

Installed together, the last mounted resident block would drop the others' sprites. `qm link` packs
every installed Shipment's sprites together on the set's base atlas, by the rule in
[`add_fx_sprite`](#add_fx_sprite) — the same bytes in any load order — and writes one `fxdict` and
one atlas into its one resident block:

- the free square's mips are box-filtered from the square alone, BC3-encoded and written over the
  base at the square's blocks; where the square at a mip is narrower than a 4×4 block, the block is
  decoded, the square's texels written, and the block encoded again; where it is narrower than a
  texel, that texel is the mean of the four texels above it;
- the records are the base's and the sprites', sorted by key as a signed 32-bit integer, the order
  the lookup searches.

A set with no sprite and no repaint leaves both untouched. A set whose only fx contributions are
sprites or a repaint lists no effects block in `link_block_paths`; the resident block is always
listed. A `raw` payload that carries the `fxdict` or the atlas is **M0308**.

### Write-sets and read-sets

A claim is not only a write. Shipment A can *read* something Shipment B provides; uninstall B and A's
contribution evaporates with no error anywhere. So a blast radius records both, and a read with no
writer is a finding at deploy rather than a mystery later.

`donor:` is a read — it is borrowed, never written.

### Why `patch_lua` takes an append, not a script

Scripts do not load individually. All 114 live in one block, so editing one means re-emitting all of
them — and two Shipments that each ship their own copy of that block cannot both win. The last one
installed erases the other, silently.

You therefore declare an **append**, and `qm link` composes every installed Shipment's appends onto
the base script, compiles once, and emits a single WAD mounted last. This works for **any** script:
there is no list of scripts that may be patched. A wholesale `replace_lua` cannot compose — an append
to it would land on source that is no longer there — so it conflicts with any other writer to that
script.

The order the appends are concatenated in is the **load order**: a Shipment always comes after the
Shipments it `requires`, and the order the installer lists them in breaks ties. The same order
decides everything else ordered in a link — which of two string edits wins, the order `add_script`
modules are minted, the order of added outfits in the wardrobe. A stable order matters more than it
looks: the game saves a costume as a *position* in the outfit list, and a co-op partner and a save
qm did not write restore from that position alone
([below](#outfits-are-generated-by-the-link)).

`add_script` modules and `import`: once a link has minted everything, it checks the module names
every piece of Shipment Lua depends on, read from that source's **compiled** DEPS: what the engine
loads, one entry per `import` or `inherit` call with a literal name, in any call form
(`import("x")`, `import "x"`), inside functions too. In the front end the check runs against
`shell.wad`'s scripts, so a front-end library that imports a gameplay-only module is caught.
- [M0209](#m0209): a dependency nothing in the link's blocks provides.
- [M0313](#m0313): an `add_script` with no `load:` that nothing that runs reaches.

`qm link` refuses the set on either, with the plan written and `libraries: null`. A standalone
`qm build` logs them as warnings, since another Shipment of the set may provide the module. The
exception is an import of one of the Shipment's own modules that is minted only in the other
session: no other Shipment may mint that name, so the build fails. `dynamic_import(<expression>)`,
`import(<expression>)` and a concatenated name (`import("a" .. b)`) are not dependencies: they
resolve at runtime, where the linker cannot see them.

### Outfits are generated by the link

The wardrobe saves the picked row `i` as the costume byte `i - 1`, and the join puts the byte back on
through the player script's character map, whose entry `k` is the model of row `k + 1`. Both are lists
by position, and the map is a local of the player script, so an outfit appended by hand restores as
whichever model the map holds at that position. `add_outfit` therefore appends nothing: `qm link`
reads the wardrobe and the character map from the bytecode it is linking, and generates them for the
installed set.

- **One list per hero.** The retail rows in retail order, then each installed outfit offered to that
  hero, in load order (a Shipment's own outfits in manifest order). The character map is regenerated
  to match, with the retail map's models that no wardrobe row offers after the added ones. The same
  set gives the same lists on every machine.
- **`wearer`.** One hero (`wearer: mattias`) or a list (`wearer: [chris, jen, mattias]`); `jen` and
  `jennifer` are one hero. A listed outfit has its own place in each hero's list and is saved by the
  same name whichever hero wears it. A list names each hero, so a hero added later is not offered it
  ([M0140](#m0140), [M0141](#m0141)).
- **`levels`.** The levels whose wardrobes offer the outfit, e.g. `[vz, dlc01]`. Omitted, the main
  campaign (`vz`) alone: a custom level runs its own wardrobe and is left as it ships unless an outfit
  names it. A custom level saves no outfit name, so there it restores by the costume byte alone
  ([M0143](#m0143), [M0263](#m0263)).
- **`chatter`.** The chatter set the hero speaks with while wearing the outfit, e.g. `Obama_Obama01`:
  set on the join, on a wardrobe change and on a co-op partner's change. Changing to an outfit
  without one gives the hero its own set back (the `ChatterSet` of its templates, named by qm's
  `data/chatter_sets.tsv`). The name must be one qm knows the game holds ([M0142](#m0142)), and a
  Shipment of the set must add `Human.SetChatterSet`, which the game does not register
  ([M0264](#m0264)). The set's lines play only if their bank is loaded: a DLC set's are in the DLC's
  voice banks, which [`load_sound`](#load_sound) loads.
- **`briefing_safe: false`.** A briefing cutscene shows the hero in its own model while it plays, and
  the outfit again after, as the DLC does for its own costumes (`bNoBriefing`,
  `dlc01_briefing.lua:840-861`, `:2040-2049`); a co-op partner is told of both changes. Omitted, the
  outfit is shown in briefings, as every retail outfit is. In a level whose briefing already honours
  `bNoBriefing` (dlc01's) the row carries the flag and the level's own guard acts on it.
- **Always offered.** An added outfit is listed whatever the unlock count, and a hero with one opens
  the wardrobe even before the first retail unlock (where the game otherwise shows its custom-outfit
  tip). Retail outfits keep their unlock order.
- **Saves remember which outfit.** A save also records the worn outfit by name (the retail `Name`, or
  the Shipment and `slug`). Loading it puts the costume byte back where that outfit now is, so a set
  that changed between save and load still restores the right outfit; an outfit no longer installed
  restores as the hero's own model. A save without that record (a retail save) keeps its byte, and a
  co-op client restores from the byte the host sends.
- **Refusals.** `qm link` refuses a `slug` equal to a retail outfit's `Name` for a hero it is offered
  to, the same `slug` twice from one Shipment for one hero, and a `patch_lua` that appends its own
  `_tOutfits` row while the set has outfits (the map would not restore it).

## The Code layer

Three kinds place native-code files in the game folder, and none takes a path: the destination is a
closed set of names, and the file name comes from the source file.

- **`native_hook`** places one prebuilt `.asi` plugin in `scripts/`, where `pmc_bb.dll` loads it on
  retail. M0160 rejects attaching one to a `reimpl` target, where the loader does not exist, and
  M0161 a hook with neither a plugin nor a symbol.
- **`place_file`** places a companion file (an `.ini` beside a plugin, a Lua framework's `.lua`
  files) under a destination name: `game_root`, `scripts`, `plugins`, `update`, `on_boot`, `on_load`,
  `on_key`. It refuses `.asi`, `.wad`, `.exe` and `.dll`.
- **`add_runtime_dll`** places one runtime DLL — a library other plugins import by name — in the game
  root, the directory Windows searches first:

  ```yaml
  - kind: add_runtime_dll
    dll: src/my-runtime.dll
  ```

  The file must be named **`<shipment.name>.dll`** (compared case-insensitively), so a runtime
  Shipment ships exactly one DLL, named after itself, and two Shipments can never ship one name. Its
  stem must not be on the deny list — `pmc_bb` (the loader), `cruise` (its sidecar), `dxwrapper`,
  `binkw32` — which is also why those are reserved Shipment names (M0211).

**M0162** refuses a file name no Shipment may write (any of the rules above). **M0178** refuses a
plugin or runtime DLL the game could not load: not an i386 PE image with the DLL flag, since
`Mercenaries2.exe` is a 32-bit process and `LoadLibrary` refuses anything else.

**A prebuilt ASI or DLL is arbitrary native code.** The Quartermaster does not compile it and does not
read its import table. What it records is *which bytes* were placed: every placement's sha256 is in
`placement.json` and the load plan. That is **integrity, not authenticity** — it proves the file is
the one that was built, not that it is safe. Treat installing one the way you would treat running
any downloaded executable. A plugin that needs another Shipment's DLL must say so in
`load.requires`; nothing checks its imports for it, and a missing DLL is a runtime failure.

### `native_hook`

```yaml
  - kind: native_hook
    target: retail
    plugin: src/my_hook.asi
    touches: ["0x004CF340", "0x004CF400"]
    signature_guard:
      "0x004CF340": "55 8B EC"
      "0x004CF400": "53 56 57"
```

`plugin` is the `.asi`, placed in `scripts/`; `symbol` names a detour instead of (or as well as) a
plugin, and one of the two is required (M0161). `touches` is the list of hooks the plugin installs,
claimed `Exclusive` ([spelling below](#touches-how-hooks-are-spelled)).

`signature_guard` records, per touched address, the prologue bytes the plugin expects to find there
— the check a well-behaved plugin makes at load time before patching, so it leaves an exe that has
shifted under it alone. **M0199** checks the guards: a guard for an address the hook does not
`touch`, or a value that is not space-separated hex bytes, is an error; once some addresses are
guarded, a touched address left unguarded is a warning; and with the game in hand
(`qm lint --with-game`, and during `qm build`) a guard whose bytes do not match `Mercenaries2.exe`
at that address is a warning — the local exe may be a different build than the hook targets.
Declaring no guards at all is allowed.

### `touches`: how hooks are spelled

`native_hook` `touches` are claimed `Exclusive`, and a collision is a hard error — no load order
fixes two plugins hooking one function. The claim is on the exact string, so **two plugins collide
only if they spell the same hook the same way**. Spell each hook kind one way:

| hook kind | spelling | example |
|---|---|---|
| game code, at a fixed address | `0xVVVVVVVV`, the virtual address in that EXE build | `0x004CF340` |
| an exported Windows API function | `module!function`, the module lowercased and without `.dll` | `ws2_32!connect` |
| a COM method | `Interface::Method` | `IDirect3DDevice9::EndScene` |
| a Lua binding's C function | `Table.Function`, declared next to its `0xVVVVVVVV` | `Player.SetCash` |

Declare each hook **by its symbol name** (`luaB_type`, `ws2_32!connect`,
`IDirect3DDevice9::EndScene`) **and**, where it has fixed addresses, **by the VA for every EXE build
the plugin supports** — so a plugin that picks its addresses per build declares each of them.

### Dependencies

`load.requires` declares what must be installed for a Shipment to work. **Every dependency is a
Shipment**, runtimes included; there is no other kind. Three forms:

```yaml
load:
  requires:
    - lua-bridge                                     # that Shipment, any version
    - { shipment: lua-bridge, version: "^1.0.0" }    # that Shipment, within a semver range
    - { capability: widescreen }                     # any Shipment that `provides: [widescreen]`
```

Before a build, `qm preflight` (and `qm link`) checks the whole installed set: a requirement with no
provider, or a provider outside the range, is **M0204**; two Shipments with one name are **M0203**.
A requirement also orders the load: a Shipment loads after the Shipments it requires (a cycle is
**M0174**).

Ranges are semver ranges with Cargo's grammar (**M0172** rejects one that does not parse). Note the
caret on a `0.x` version: **`^0.6` means `>=0.6.0, <0.7.0`**, so it excludes `0.7.0`. Write
`">=0.6, <1"` when you mean "any 0.x from 0.6". A Shipment naming itself in `requires` or `conflicts`
is **M0173**. `shipment.quartermaster`, when present, is a range the running qm must satisfy
(**M0210**).

**Removed forms.** `{ name, version }` is not a requirement form; validation says to write
`{ shipment: <name>, version: <range> }`. The external pin `{ url, sha256 }` is gone and does not
parse, and with it M0170 and M0171: a Shipment never installs a file from a URL — a plugin you depend
on is required as the Shipment that ships it. `load.after` / `load.before` are gone too; the load
order comes from `requires`.

## Limits

- `shipment.name` — at most 64 characters, `^[a-z0-9]+(-[a-z0-9]+)*$`. It becomes a filename.
- A patch WAD's header region — the block index, asset table and path list — shares the 2 MB below
  the payload region at `0x208000`. Overflowing it writes the path list into the payload (M0181).
- Every block must declare a decompressed size at least as large as it actually inflates to. The
  engine sizes its buffer from that number, so under-declaring overruns the heap (M0002).

The last two are properties of the built WAD rather than of your manifest, and `qm` checks them
against the artifact before writing it to disk.

## What the checks mean

```
[M0007] warning: pmc_hum_example is a STREAMED texture … — see https://…
```

| severity | effect |
|---|---|
| `info` / `warning` | printed; the build proceeds |
| `error` | the build fails |
| `HANG` | the build fails — this class freezes the game with no message at all |

Builds are gated on the **exit code**, never on a printed count, so a script that discards output
still cannot ship a broken Shipment.

`qm rules` lists every rule, including the ones that are known but **not yet implemented**. Those are
listed on purpose: a linter that silently omits its most dangerous checks reads as a clean bill of
health.

The fx rules M0252–M0262 and M0309 and the sprite rules M0304–M0308 are errors. M0252–M0255,
M0262, M0304, M0305 and M0308 need no game; M0256–M0261, M0306 and M0307 need the game stack and run
in `qm lint --with-game` and in `qm build`, and `qm link` applies the same rules to the installed
set. M0309 needs no game for an `add_fx` effect form and the game stack for a `replace_fx` and for
the templates that start an effect.

The sound and language rules below are errors. M0214–M0217, M0221 and M0222 need no game;
M0218–M0220 and M0223 need the game stack and run in `qm lint --with-game` and in `qm build`.

The outfit rules M0140–M0143 are errors and need no game; M0263 and M0264 are errors of the load plan.

### M0140

**A `wearer` is not a wardrobe hero.** The wardrobe has one list per hero, keyed `mattias`, `chris`
and `jennifer` (written `jen` or `jennifer`), so an outfit for any other name is offered to no one and
the game reports nothing. The fix suggests the nearest hero.

### M0141

**A `wearer` list is empty or names a hero twice.** An empty list offers the outfit to no one; `jen`
and `jennifer` are one hero.

### M0142

**A `chatter` set is not one qm knows the game holds.** `Human.SetChatterSet` takes any name, and an
unknown one does nothing, with no message. The known sets are qm's `data/chatter_sets.tsv`, each with
its evidence (a template's `ChatterSet` value or a retail script that sets it); case does not matter.
Fix: use a listed set, or add one the game holds to that file with its evidence.

### M0143

**A `levels` list is empty or names a level twice.** An empty list offers the outfit in no wardrobe;
omit `levels` for the main campaign alone.

M0197 and M0198 are errors; M0310 is a warning. M0197 checks `qm`'s own names and the Shipment's
with no game, and the game's script names in `qm lint --with-game` and in `qm build`. M0198 needs
the game stack and runs in `qm lint --with-game` and in `qm build`. M0310 needs no game.

### M0197

**An `add_script` name, or the `module` of a novel shop behaviour, is already a script's name.** The
engine finds a script by its name hash and the script type, and the registry keeps the first one
registered, so one of the two is silently not the one that loads. Names are compared by
`pandemic_hash_m2`, so case does not matter.

- With no game: an `add_script` named `qm_modloader` or `qm_shell_modloader` (the loaders `qm link`
  mints), or named like a `behaviour.module` with a `behaviour.script` of the same Shipment.
- With the game: a name with a script row (`type_id 35`) in one of the game's own WADs — `vz.wad`,
  whose `scripts_vz` and `resident` blocks both carry scripts, and each of these present beside it:
  `shell.wad`, `Loading.wad` and each language WAD (`english.wad`, `spanish.wad`, `italian.wad`,
  `french.wad`, `german.wad`, `japanese.wad`, `russian.wad`), matched case-insensitively. A present
  WAD that does not read is an M0197 error; an absent one is not checked. No `-patch.wad` overlay
  is read, even when the build's stack holds one: it is mod output, the Shipment's own installed
  build among it, and a name another Shipment mints is **M0207**.

`qm link` refuses the same name when it mints a script into `vz.wad`'s scripts blocks: an
`add_script`, a novel shop behaviour or `qm_modloader` whose name a script of either block has.

Fix: rename the module.

### M0198

**An entity key is already placed.** An `add_placement` or `add_layer` entity file lists one key in
two records; adds a key a `Transform`, `ModelName` or `Name` record of any layer of the game
places; or adds a key an earlier `add_placement`, `add_layer` or `add_tiny_geometry` of the
Shipment adds (a stand-in that repeats an earlier entity file's key is reported at the stand-in).
Retail places every key once ([`add_placement` and `add_layer`](#add_placement-and-add_layer)).

Fix: a key no layer of the game and no other placement of the Shipment uses, once per file.

### M0214

**A sound cue's WAV is not uncompressed 16-bit mono or stereo PCM.** Fires when the strict reader
(`mercs2_audio::wav::read_pcm16_wav`) refuses a cue's or replaced line's `wave`: the file cannot be read, is not
RIFF/WAVE, its format is not 1 (PCM), its bits per sample are not 16, its channels are not 1 or 2,
its rate is 0, its block align is not `channels × 2`, its `data` chunk is empty or ends inside a
frame, it lacks or repeats a `fmt ` or `data` chunk, `data` comes before `fmt `, or its RIFF size
and chunks do not add up to the file. The build reads the file through the same reader.

Fix: export the sound as uncompressed 16-bit PCM WAV, mono or stereo.

### M0215

**A sound bank or cue name the engine cannot reach.** Fires on:

- two cues of one bank whose names hash to one guid — `m2` folds case, so `Click` and `click`
  collide;
- a bank, cue, wavebank or line name that is empty or has surrounding whitespace (the whitespace is
  hashed with the name);
- an `add_sound` bank or cue written as a bare `0xHHHHHHHH` hash: it mints a new identity, whose
  hash is the hash of its name. An override may write a hash ([Sound overrides by name or
  hash](#sound-overrides-by-name-or-hash)), and a cue declared once by name and once by its hash is
  two cues of one guid;
- a bank with no cues;
- an `add_sound` bank whose name starts with `vo_`: retail Lua appends the language to such a name
  before loading it (`_GetLocalizedName`, `mrxsoundbanks.lua:80-87`), so the loader would ask for
  `<bank>.<language>` and find nothing.

Fix: give each cue a distinct name, write names as plain trimmed text, name what `add_sound` adds,
declare at least one cue, and name an added bank without the `vo_` prefix.

### M0216

**A sound bank's `category` is not a category of the game's tree.** A group's category hash must be
one of the 19 categories of the global sounddb's tree
([`audio_code_map.md` §11.3](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/reverse_engineer/audio_code_map.md#113-sounddb)). 14 have known
names: `ambience`, `chatter`, `collision`, `explosion`, `foley`, `music`, `Non_Action_Hijack`,
`non_ui`, `sfx`, `source`, `ui`, `vehicle`, `vo`, `weapon`; the other five have no known name, so a
manifest cannot name them.

Fix: use one of the 14 names; the diagnostic suggests the nearest.

### M0217

**A sound override's `language` does not match its bank.** Fires on `language` on a
`replace_sound_bank` / `replace_sound_cue` / `replace_sound_wave` whose bank or wavebank is not
`vo_*` (the game has one copy of it for every language), or on no `language` on one whose bank or
wavebank is `vo_*` (each language has its own copy, `<bank>.<language>`). See [The `language`
field](#the-language-field). A bank or wavebank written as a hash names its entry; whether the
declared language's WAD carries it is M0218's.

Fix: remove `language` from a non-`vo_*` override; name the language on a `vo_*` one.

### M0218

**A sound override's bank or cue is not in the game, or no level that carries the bank loads it.**
Fires when no carrier holds a soundbank under the override's entry name — the carriers being the
game stack (`vz.wad` and the declared languages' WADs) and `shell.wad` beside it — when no carrier
that holds it loads it ([Where a sound override ships](#where-a-sound-override-ships)): `shell.wad`
loads only the banks its front end's Lua loads, and `vz.wad` loads every bank it carries but
`ui_shell`, which retail Lua loads only in the front end (the engine loads the banks no Lua names,
[Banks the engine loads](#banks-the-engine-loads)); or when a
`replace_sound_cue`'s cue is not in that bank (a cue the same Shipment's `replace_sound_bank`
declares for the bank counts as in it). For a `replace_sound_wave`, when no carrier holds the
wavebank, when no record or several carry the line's clip hash, when no group plays the line, or
when a cue plays it through another soundbank's group ([`replace_sound_wave`](#replace_sound_wave)).

Fix: write the bank name exactly as the game's Lua loads it (`ui_hud`, `vo_mattias`), or its entry's
hash, and, for a `vo_*` bank, the right `language`. To add a cue the bank does not have, use
[`add_sound`](#add_sound).

### M0219

**An `add_language` base has no fonts or font atlases to fork.** Fires when the `base` language
(default `english`) has no fonts `<base>_18` / `<base>_20` or atlases `<base>_18_main` /
`<base>_20_main` in `vz.wad`'s stack or in `shell.wad`, or when the two hold different bytes for
one of them. In the retail install only `english` has them.

Fix: omit `base`, or set it to `english`.

### M0220

**An `add_sound` cue has the name of a cue the game already has.** Fires when an added cue's guid is
routed by a sounddb in the game stack or in any language WAD installed beside `vz.wad` (the running
language's `vo_*` banks load at boot, `mrxsoundbootstrap.lua:219-245`, before the mod loader runs).
FindCue walks the loaded sound tables from the first loaded and answers with the first that has the
guid (`FUN_00835a70`), so the game's cue plays and the added one never does.

Fix: to change the game's cue, use [`replace_sound_cue`](#replace_sound_cue); to add a cue, give it
a new name.

### M0221

**An `add_sound`'s `load_in` is empty or lists a session twice.** `load_in` names the sessions whose
loader loads the bank — `gameplay`, `front_end` — and the bank's block ships to each one's WAD
([`add_sound`](#add_sound)). An empty list loads the bank nowhere.

Fix: list where the bank plays, each session once: `[gameplay]`, `[front_end]` or
`[gameplay, front_end]`.

### M0222

**A `load_sound` cannot be loaded as written.** It names no soundbank, wavebank or stream; a name is
blank, has surrounding whitespace, or is a hash (the loader loads sound by name); `in` is empty or lists
a session twice; or its stream is installed by no `add_level` of the Shipment, or by several.

Fix: name what to load by its name, list each session once, and name a stream of one of the Shipment's
own levels.

### M0223

**A `load_sound` soundbank or wavebank is held by nothing.** A load of a bank no mounted WAD holds times
out with no error. The name must be held, under its type, by the game, by the language's WAD for a `vo_`
name (in every installed language), or by the Shipment's own `level: vz` rows.

Fix: correct the name, or ship the bank's tables as `level: vz` rows.

The shader rules below cover [`add_shader`](#add_shader), [`replace_shader`](#replace_shader) and
[the shader import](#the-shader-import). M0230, M0231 and M0234 need no game (M0230 needs the
Shipment's files); M0232, M0233, M0237 and M0239 need the game stack and `--original-data`, and run
in `qm lint --with-game` and `qm build`; M0235 and M0236 check the built WAD and `qm link`'s set;
M0238 runs in the `add_model` lowering.

### M0230

**A shader source does not load.** Fires when an `asm` source does not assemble; when a `blob`
does not disassemble, or its disassembly assembles to different bytes; when the bytecode is over
0x8000 bytes, is not a whole token stream, lacks the end token or a `CTAB`; when its version token is
not the family's stage (`add_shader`); or when classes sharing a stem assemble to different bytes.

Fix: assemble the source with `shaderforge asm` and fix what it reports; give a pixel family
`ps_3_0` sources and a vertex family `vs_3_0` ones; give classes of one stem one source.

### M0231

**`add_shader` without the `shader-registry` capability.** The shaders register at runtime through
the m2-sdk, so the Shipment must `load.requires: [{capability: shader-registry}]`.

### M0232

**A `replace_shader` target is not a registered store record of that stage.** Fires when the stem
has no record in `shader3.bin` (or, for `shader_low`, in `shader3Low.bin`); when a record's stage is
not the source's; when no retail registration loads the stem (the engine never reads the record);
or when `shader_low` is missing for a stem `shader3Low.bin` has, or given for one it does not.

### M0233

**A shader store id or registration name collides, or the stores fill the id table.** Fires when an
added stem's id is held by a store the engine loads with it; when an added name's key is a retail
registration's or another added one's (the registry keeps the first); or when a set of stores loaded
together reaches 0x1200 records, where the engine's id table insert never returns.

### M0234

**An `add_shader`'s classes are malformed for its family.** Fires when a pixel family has other than
4 classes or a vertex family other than 1, when a name is empty, has surrounding whitespace or shares
a key with another class (`m2` folds case), or when a stem is empty, ends in `.sho`, has a path
separator or a non-printable character, or makes `<stem>.sho` longer than 128 characters.

### M0235

**A material's pixel-shader key is not registered in every configuration** (crash). `Mtrl_Parse`
(`FUN_00858790`) looks the key up in the pixel registry and, on a miss, takes the registry's null
entry (`DAT_01977a3c`, 0); the read through it at `0x00858DB8` is an access violation (crash at
`0x00858DB8`, a null registry entry read at `+8`;
[`shader_store_format.md` §7](../shader_store_format.md#7-the-0x00858db8-crash)). The key must be a
retail pixel registration made in every configuration, or light class 0 of an `add_shader` of the
Shipment (at `qm link`, of it or a Shipment it requires).

### M0236

**A primitive group's vertex-shader key is not a registered vertex shader** (crash when drawn).
The group loaders (`FUN_00478270` for `MESH` and `TINY`, `FUN_004796f0` for `SKIN`) look the `INFO`
words `+0x0C` and `+0x10` up in the vertex registry and, on a miss, store its null entry
(`DAT_0197da44`, 0) as the group's main (`+0`) or shadow (`+4`) vertex-shader record. Every pass that
draws the group reads the record with no null test, an access violation:

| pass | `MESH` / `TINY` mesh path | `SKIN` | `TINY` instance path |
|---|---|---|---|
| main (`+0`) | `0x00478906` (`0x004788b4` with blend shapes) | `0x00479a14` | `0x0047a6fb` / `0x0047a764` |
| shadow (`+4`; the mesh path reads `+0` first, at `0x00478d48`) | `0x00478dd3` (`0x00478cf5` with blend shapes, `0x00478d6a` on another branch) | `0x00479dc5` | `0x0047a89c` / `0x0047a902` |
| Z (`+4`) | `0x004790a8` (`0x00479055` with blend shapes) | `0x0047a178` | `0x0047aa5c` / `0x0047aac6` |

A pass reaches the read only when it draws the group (the LOD and node gates, the mesh main pass's
cull, the Z pass's material flag test, `EnableShadows` for the shadow pass), so a model that is
loaded and never drawn does not crash. Every dereference is **PROVEN** by disassembly; that the
access violation ends the process, with no handler absorbing it, is **INFERRED**.

### M0237

**A shader constant its family never binds.** A family's binder (the record's vtable `+0x10`)
resolves a fixed list of constant names, and the engine sets a shader's constants through those. A
`CTAB` constant outside the list is set only by code that addresses it directly: three retail
shaders declare one (`PgColorFPConst` `color`, `PgLtiDebugZPassFP` `depthRange`,
`PgLtiTerrainShadowVP` `PositionOffset`), and no engine code addresses a new shader's. So an
`add_shader` source may declare only its family's constants, and a `replace_shader` source only its
family's plus those its retail record declares. Samplers are bound by texture stage and are not
checked.

### M0238

**A vertex shader reads an input the group's vertex declaration does not supply.** The resolved main
vertex shader's `dcl` inputs, in every store holding it, must all be elements of the group's
declaration. It also fires when the shader reads `POSITION.w` and the glTF does not supply it: an
AmbientWind shader without `_SWAY_WEIGHT`, a `TINY` group without `_TINY_SLOT` (or with a slot the
host's id list lacks, or a triangle spanning two slots), or a `POSITION` element with no `w`
([`POSITION.w`](#positionw-ambientwind-and-tiny-hosts)).

### M0239

**The shader registry's capacity is exceeded** (HANG). A configuration registers more than 0x800
pixel names or 0x100 vertex names, counting retail's (243 pixel, `PgCompositeFP` included, and 129
vertex with ShaderLevel on).
The registry inserts (`FUN_0085b7c0`, `FUN_00632250`) never give up on a full table.


The stand-in rules below cover [`add_tiny_geometry`](#add_tiny_geometry) and a TINY host of
[the shader import](#positionw-ambientwind-and-tiny-hosts). They are errors. M0240, M0241, M0249 need
no game, and M0242–M0244 and M0251 need the Shipment's files; M0241 (for two spellings of one object),
M0245–M0247 and M0250 need the game stack and run in `qm lint --with-game` and `qm build`; M0248 runs
in the `add_model` lowering.

### M0240

**An `add_tiny_geometry` lists more than 192 objects, or none.** The slot list's count is a byte
(`0x0050F42B`), so it holds 255 GUIDs, and no object takes a slot `≡ 3 (mod 4)`: 192 slots are left.

Fix: split the cell's objects between two layers' stand-ins, or draw fewer.

### M0241

**An `add_tiny_geometry` names one object twice**, as one spelling twice or as a name and the GUID it
resolves to. The list is searched by GUID, so a second slot for one object is never reached.

Fix: name each object once.

### M0242

**A stand-in vertex has no `_TINY_SLOT`, or a slot past `objects`.** The shader reads the state of a
slot no object of the stand-in sets.

Fix: give every vertex the index of its object in `objects`.

### M0243

**A stand-in triangle spans two slots.** The shader keeps or drops each vertex by its own object's
state, so the triangle tears when the two states differ.

Fix: give a triangle's three vertices one slot; split shared vertices.

### M0244

**A stand-in primitive declares no `extras.tiny_role`**, or one that is not `intact` or `ruined`.
Nothing then says which shader draws it.

Fix: set `extras.tiny_role` on the primitive.

### M0245

**A stand-in object does not resolve, or is not placed in the stand-in's cell.** A name must be one
placement of the layer, a GUID a placement of some layer of the game, and the object's position (in
the layer, or else where it is placed) in the cell. The engine finds the stand-ins of an object whose
state changes through the cell of the object's position (`FUN_0050F730`, `FUN_0050F7E0`), so an
object elsewhere never updates its slot. Also fires for a `layer` the game lacks.

Fix: list the objects of the cell; name an object by its GUID when its name is shared.

### M0246

**The stand-ins exceed the 1,400 slot lists the registry holds.** The registry takes 1,400
(`0x0050F1BE`) and drops a list past that (`0x0050F26C`): the stand-in's renderable then finds no
list, uploads no states, and its shaders read whatever the previous draw left in the registers. The
count is every stand-in the game places (1,208) and the Shipment's.

Fix: add fewer stand-ins.

### M0247

**The layer already has a stand-in for the cell.** Two would draw the cell's objects twice.

Fix: pick the cell's stand-in in another layer, or another cell.

### M0248

**A TINY vertex slot is `≡ 3 (mod 4)`.** Fires in `add_model` on a TINY host. The TINY vertex shaders
(all six, in both stores) read slot `w` as component `w mod 4` of `ObjectIDScaleArray[w / 4]` and
compute component 3 as `2·.w − .y`, so the vertex shows only while its object's state equals that of
the slot two below: an intact object beside a hidden one disappears.

Fix: host the geometry on another slot, or build the stand-in with
[`add_tiny_geometry`](#add_tiny_geometry), which keeps every object off those slots.

### M0249

**An `add_tiny_geometry` cell is outside the 40 × 40 grid.** `row` and `col` are 0 to 39.

Fix: pick a cell of the grid.

### M0250

**An `add_tiny_geometry` key is already a placement**, of the game or of another stand-in of the
Shipment. Placements are found by key.

Fix: pick a key no layer uses.

### M0251

**A stand-in's model does not read as one.** A primitive without a material, `NORMAL` or
`TEXCOORD_0`, or with more than 65,535 vertices; a material no primitive draws, one with
`alphaMode: BLEND`, one without `extras.texture`, or one whose pixel shader the convention cannot
name and the file does not declare; or a file that does not read (a mode other than triangles or
strips, a scaled node).

Fix: what the message names.

### M0252

**An `add_fx` effect form does not read as an effect.** The file is missing, its extension names no
form format, it does not parse, or it does not lower: an attribute not declared or declared twice,
an unknown key, a value its position cannot take, a curve at a position the loader takes none at,
or another rule of the effect writer ([`effect_container_format.md`](../effect_container_format.md)
§8). An emitter's shape is **M0309**.

Fix: what the message names.

### M0253

**An `add_fx` template name is empty, longer than 0x7F bytes, or carries a NUL.** The engine reads a
name into a 0x80-byte buffer, and a NUL ends it.

Fix: a non-empty name of at most 127 bytes.

### M0254

**A `replace_fx` edits form does not read, or is empty.** The file does not parse (an unknown `op`,
an unknown field, a missing one), or `edits` is empty and the replacement would change nothing.

Fix: what the message names.

### M0255

**An `add_fx` template does not declare exactly one `RedEffectComponent`.** Its `name` field names the
effect the template starts.

Fix: declare one `RedEffectComponent`.

### M0256

**An `add_fx` template does not lower against the game's component schemas.** A class the
`worldentity` has no component group for, a field of the class not declared, a key that is not one of
its fields, or a value its field cannot hold.

Fix: what the message names; `TemplateForm::express` writes any retail template as a form to start
from.

### M0257

**An `add_fx` template's name or derived key is already taken**: by a template the game has, or by
another added template, which the message names with its Shipment. The registry answers a name with
one key, and two templates under one key overwrite each other's records.

Fix: rename the template.

### M0258

**An `add_fx` name is an effect the game already has**, or one another `add_fx` adds.

Fix: give the effect a new name; to change the game's effect, use `replace_fx`.

### M0259

**An effect's frame or a template's effect names nothing.** A `TEXT` frame the Shipment gives an
effect that is neither a record of the game's `fxdict`, where the loader looks each frame up (a frame
it does not find draws the whole `vfx` atlas), nor an `add_fx_sprite` of the Shipment or of a
Shipment it requires; or an `add_fx` template whose `RedEffectComponent` names an effect that neither
the game ships nor the Shipment adds. Built on its own, a Shipment that requires another leaves a
frame it finds in none of these to `qm link`, which checks it against the Shipments it requires.

Fix: name `fxdict` records or sprites as frames — to draw another Shipment's sprite, require that
Shipment — and the effect the Shipment adds in the template.

### M0260

**A `replace_fx` target resolves to no effect**: no effect or template of that name, a template with
no `RedEffectComponent` or more than one, or an effect or template another Shipment adds when this
Shipment does not require that Shipment.

Fix: name an effect, or a template that starts one; to edit another Shipment's addition, require it.

### M0261

**A `replace_fx` edit addresses a node the effect does not have, or breaks a writer rule**: an
emitter, force or shape index past the effect's, an attribute the node does not have, a value its
position cannot take, a shape removed while a `GEOM` names it, or an effect left breaking a rule of
the effect writer. An emitter left without a shape the engine can sample is **M0309**.

Fix: what the message names; the effect's own form (`EffectForm::express`) shows its nodes.

### M0262

**A `raw` payload carries an effect or the worldentity.** Effects and templates ship through
`add_fx` and `replace_fx`, which `qm link` merges into the game's effects block and worldentity.

Fix: use `add_fx` or `replace_fx`.

### M0263

**An outfit is offered in a level the set does not have.** A `levels` entry is neither `vz` nor a
level an `add_level` of the installed set adds, so no wardrobe would offer the outfit there. `qm link`
and `qm preflight` refuse the set; a standalone `qm build` does not check it, since another Shipment
of the set may add the level.

Fix: install the Shipment that adds the level, or drop it from `levels`.

### M0264

**An outfit's `chatter` set needs a binding the game does not register.** qm applies a chatter set with
`Human.SetChatterSet`, which the PC executable does not register; a plugin must add it (the Blow It Up
Again Shipment's does). A Shipment of the set must declare `Human.SetChatterSet` in a `native_hook`'s
`touches`, or the first join that applies the set fails.

Fix: install the Shipment that adds the binding, or drop `chatter`.

### M0304

**An `add_fx_sprite` image does not read as a sprite.** The file is not a PNG, does not decode, or
its width or height is not a power of two from 4 to 512.

Fix: a PNG of 4, 8, 16, 32, 64, 128, 256 or 512 texels on each side.

### M0305

**An `add_fx_sprite` name is empty or written as `0xHHHHHHHH`.** The frame key is the name's hash,
and an effect frame written as a hash names that hash itself.

Fix: give the sprite a name.

### M0306

**An `add_fx_sprite` key is already an `fxdict` record**: one the game's `fxdict` has, or another
Shipment's sprite, which the message names. The lookup finds one record per key.

Fix: rename the sprite.

### M0307

**The sprites do not fit the free square of the base atlas**: together they need more texels than
the square holds — the message lists every sprite, its allocation and the space left — or the base
atlas has no free square, every square of it lying under a record or carrying alpha. The base is the
set's repaint of `vfx`, else the game's atlas, whose free square is 512² at (1536, 0).

Fix: smaller or fewer sprites; a repaint that leaves a transparent square free of records.

### M0308

**A `raw` payload carries the `fxdict` or the `vfx` atlas.** Sprites ship through `add_fx_sprite`
and the atlas through `replace_texture` of `vfx`, which `qm link` merges into the game's `fxdict` and
atlas.

Fix: use `add_fx_sprite` or `replace_texture`.

### M0309

**An effect emitter has no shape table the engine can sample.** A spawned effect picks each new
particle's spawn record from its emitter's `GEOM` as `random % count`
([`effect_container_format.md`](../effect_container_format.md) §2.1), so it divides by zero or reads
past the shape when:

- a `GEOM` names a shape the effect does not have, or a shape without records;
- a `GEOM`'s `word` (the count) is 0, more than the named shape's record count, or more than 32,767;
- an emitter without `GEOM` can spawn a particle: its `rate` has a curve or is above 0, or its
  `ratevar` is not 0;
- with the game: a template whose `RedEffectComponent` field `0x62C7746E` (its per-distance factor)
  is not 0 starts an effect with an emitter without `GEOM`, the factor spawning particles as the
  effect moves. An `add_fx` template that does is refused, and so is a `replace_fx` that leaves an
  emitter without `GEOM` in an effect such a template starts; the message names the templates.

Observed 2026-10-06: an emitter without `GEOM` crashed the game with `INT_DIVIDE_BY_ZERO` at
`0x0048AFF6` on `Pg.Spawn` of its template ([`fx_live_gate.md`](fx_live_gate.md) §3).

Fix: give the emitter a `GEOM` naming a shape of the effect, with `word` the shape's record count;
or, for an emitter that spawns nothing, a constant `rate` of 0 and a `ratevar` of 0, started only by
templates whose `0x62C7746E` is 0.

### M0310

**A novel shop behaviour's rows exist only from `world.begin`** (warning). An `add_shop_item` whose
`behaviour` carries a `script` mints a new module, whose `Create` cannot run while the resident scripts
load, so `qm_modloader` inserts the item's catalog and reward rows at qm's `world.begin` hook, when
player-visible gameplay first starts in a world. From then the item is at every vendor in `shops`,
because the loader also clears `MrxRewardData.gtAllSupport`, the per-faction list cache
`GetAllPotentialShopItems` fills (`mrxrewarddata.lua:1489-1528`).

A save is restored earlier, during the world load: `MrxSupportData.LoadSingleton` restores the rows
that exist (`xQ!L.lua:795`, `mrxsupportdata.lua:2445-2457`), and the save table is cleared before
`world.begin` (`xQ!L.lua:865`). qm keeps that table as it reaches `LoadSingleton` (its `world.load`
hook) and applies the item's saved unlock status and stock when it inserts the row, as
`MrxSupportData.LoadSingleton` would have. What stays true is the warning: Lua that reads
`MrxSupportData.tSupportData` or `MrxRewardData` during the world load does not see the item.

Fix: none — the warning says when the item exists.

### M0311

**An `add_level` could only overwrite, or a `level_asset` cannot compile.** An `add_level`'s `name` is not
a lowercase `[a-z0-9_]` token, or is a WAD the game ships (`vz`, `shell`, `loading`, `english`, `french`,
`german`, `italian`, `spanish`, `japanese`, `russian`), or a stream is no `.pws` or is one the game ships:
`music`, `ambience`, or a `vo_stream` language. A `level_asset` names a level this Shipment does not add
(other than `vz`) or a type the type table does not name, has no rungs, or has a rung whose block is not one
path component or does not name that rung. The build refuses the same names. A `level: vz` row would
replace what `vz.wad` holds: a worldentity row without `templates`, `templates` on another row, a
worldentity of more than one rung or a template named twice; with the game, an asset or block path
`vz.wad` has, a reference nothing resolves, or a template that cannot be appended
([`level: vz`](#level_asset)).

Fix: pick a name the game does not use.

### M0312

**An `add_script` `load:` that qm's loader cannot honour** (error). `when` names no hook of
Quartermaster's catalogue (`data/hooks.tsv`); `in` is empty or lists a session twice; or `in` lists a
session the hook does not fire in (`world.scripts_loaded` fires only in gameplay, `shell.boot` only in
the front end). The library would never load there, and a consumer's `import` of it would bind
nothing. The build refuses the same `load:`.

Fix: name a catalogue hook, and list in `in` only the sessions it fires in.

### M0209

**A dependency nothing provides** (error at `qm link`; a warning in a standalone `qm build`). The
compiled DEPS of a piece of Shipment Lua (a `patch_lua` append, an `add_script` module, a
`replace_lua` source or a support module) names a module that no script of the link's blocks
carries. That covers retail's scripts, the set's `add_script` modules and the modules qm mints;
`qm_modloader` counts only when the link mints it. The import may be `import("x")`,
`import "x"` or `inherit("x")`, at the top level or inside a function. The engine never runs a script
whose dependency is missing, and an `import` of a name no loaded WAD holds crashes the game
(`0x5AE362` on v1.1). The front end checks against `shell.wad`'s scripts only.

Fix: ship the module (an `add_script` in this Shipment or one it requires), or correct the name.

### M0313

**An `add_script` nothing runs** (error at `qm link`; a warning in a standalone `qm build`). The
module has no `load:`, and nothing that runs reaches it. `import` only binds a module whose body
has already run; a body runs when something queues it. The link counts these as reaching a module:
- a DEPS edge (`import`/`inherit`) from a retail script, a library, qm's loader, or a module
  reached that way;
- a literal by-name route in one of those: `sModuleName = "x"` (the task system `dynamic_import`s a
  row's module when it activates), `dynamic_import("x", …)`, or `Sys.RequiredAsset("x", "script", …)`.

A name built at runtime is not seen. A module only another unreached module imports is unreached too.

Fix: give the module a `load:`, or import it from code that runs.

### M0314

**A module read before its body runs** (error at `qm link` and in a standalone `qm build`), or **a
link that changes retail's load order** (warning). `import` binds the module's own table (its env).
The engine's loader builds that table before it runs the module's body, and the body fills it. In a
DEPS cycle the loader runs one module's body while a module it imports has only its table: whatever
that body reads from the module is not there yet. The table fills when the module's body runs, so
code called later sees it: the engine calls a module's `Init` after the body of every module it
imports. qm models the loader (pc-v10 queue `0x59CD30`, step `0x59CD70`, pump `0x5A2BC0`) over the
link's scripts, before and after the link, and reports two cases:

- **error**: an import the link adds, where the engine runs the importer's body before the body of
  the module it imports, and Shipment code of the importer uses the module at its top level
  (outside every function): indexes it, calls a method on it, stores into it, takes its length or
  calls it, directly or through a local it copied the module into. The message names the line.
  Keeping the table in a local, or putting it in a table, is not a use; the table fills later.
- **warning**: a link that changes the order the engine loads the scripts that were already
  loading. Where an append's import lands in a retail script's DEPS is not the author's to choose,
  and no reorder has been seen to break retail, so this reports what moved and refuses nothing.

The error is not seen when the top-level code passes the module to a call that reads it (such as
`pairs(m)`), or calls a function of its own that reads it; nor for a bare name the script reaches
through a base it `inherit`s (its env's `__index` reads the base's table).

The model loads in passes. The world loads `gamebootstrap`, then the level's master (`vz`, or a
custom level's own); the front end loads `shellbootstrap`. Each pass finishes before the next is
queued. Every script those passes leave unloaded loads in a pass of its own, from where they left
off. That covers a script nothing else depends on, a library's `load:`, and a literal by-name route
([M0313](#m0313)). The engine loads those by name at times the model cannot order, so each is
checked alone.

Run against the game, the model reproduces the order in which retail's 84-script DEPS cycle ran its
bodies, and the order in which a library's own two-module cycle ran (the cycle gate,
`tests/fixtures/live_gate/qm-cycle-gate`). Retail's own cycles never fire, because only what the
link changes counts. An import of a module whose body has already run is fine, inside a cycle or
not.

Fix (the error): use the module inside a function that runs later, such as your `add_script`
module's `Init`, rather than at the top level.

### M0315

**An `import` inside `pcall`** (warning, `qm lint`). The forms checked are
`pcall(function() import("x") end)` (an import anywhere in the protected function's body, outside a
function nested in it), `pcall(import, "x")`, and the same with `xpcall`. An import of a module no
loaded WAD holds crashes the game natively (`0x5AE362` on v1.1), not with a Lua error, so the `pcall`
never sees it.

Fix: import the module plainly, and make sure something provides it ([M0209](#m0209)).

### M0316

**A `fetch_package` or a `from:` the build cannot resolve** (error, `qm lint`). Each of these is refused,
without the network:

- a `fetch_package` whose `id` is not a lowercase `[a-z0-9_]` token or is repeated, whose `url` is not
  an `http://` or `https://` URL with a host, whose `sha256` is not 64 lowercase hex digits, or whose
  `size` is 0 (`qm build` and `qm link` refuse these too, before they touch the cache);
- a `fetch_package` no `add_level` or `level_asset` names with `from:`;
- a `from:` that names no `fetch_package` of the Shipment;
- a `level_asset` with both `rungs` and `from`;
- an `add_level` with `from:` that also lists `streams`, or whose level has `level_asset` rows written by
  hand.

Fix: give each package one `id`, name it from the rows it supplies, and let `from:` replace the rows and
streams it would write.

### M0317

**A `set_spawn_lists` row the wrapper cannot set** (error). `qm lint` checks the rows alone: a
`function` that is not a function name, a `building` that is not a plain name, an empty `group` or
`list`, two rows naming the same call. With the decompiled corpus (`qm lint --with-game` with
`--corpus` or `--workshop-data`, `qm build`, `qm link`), each row is checked against the shipped
scripts: `target`'s `function` makes no such call, the function holds the building as a parameter or
local, or no `vz.wad` script sets the `list`.

Fix: choose the row in the Depot's spawn table, which lists every call the target makes and the lists
the scripts set.

[template]: https://github.com/Mercenaries-Fan-Build/mercs2-shipment-template
