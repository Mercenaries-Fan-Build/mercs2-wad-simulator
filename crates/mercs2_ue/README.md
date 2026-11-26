# mercs2_ue

Mercenaries 2 game data for Unreal Engine: a static library with a C interface that opens the game's
archives and hands out models, terrain cells, textures and layer placements already in Unreal's space.

## What it is

An Unreal plugin links `libmercs2_ue.a` (`mercs2_ue.lib` on Windows) and includes
[`include/mercs2_ue.h`](include/mercs2_ue.h). The plugin parses no game format and converts no axis:

- every format is read by the decoders the engine itself uses (`mercs2_engine`, `mercs2_formats`);
- every position, direction, rotation, length and triangle leaves the library converted by one module,
  [`src/space.rs`](src/space.rs).

| Call | Returns | Read through |
|---|---|---|
| `m2ue_open` | the game: the WAD stack around a level archive, and the pixel shaders' samplers from the `shader3.bin` beside it | `mercs2_engine::asset::AssetSource::discover`, `mercs2_engine::render::pixel_samplers` |
| `m2ue_model_load` | a model, every LOD block of its chain, posed at rest | `mercs2_engine::model::Model::load` |
| `m2ue_terrain_cell_load` | a hi-res terrain cell, cell-local | `mercs2_engine::mesh::build_terrain_cell` |
| `m2ue_texture_load` | a texture's whole mip chain, block-compressed as shipped, and its largest mip decoded to RGBA | `AssetSource::extract_texture_hires`, `texture_encode::decode_dxt_surface` |
| `m2ue_position` | a point from the game's scripts, converted | `space::position` |
| `m2ue_layers` | every layer: its name hash, whether a `layers_static` or a `vz_state_*` block ships it, and that block's name | the archives' `ASET` rows of type 9, `mercs2_formats::world_index::WorldIndex` |
| `m2ue_stand_in_load` | a far-distance stand-in (`TinyGeometryObject`): one section per world object, state and material | `mercs2_formats::tiny_model::TinyModel` |
| `m2ue_mesh_lod_switch_distance` | the distance from the main camera at which a model's tier takes over from the one before it | the model header's reference distance through `mercs2_engine::render_state::lod_rung`'s rule |
| `m2ue_layer_load` | a layer's placed entities: model, terrain cell, tile, ground cover, light, far-distance stand-in; each with its wake radius and whether it is a prop, a vehicle or a character | `mercs2_engine::worldutil::PlacedCatalog`, `mercs2_core::hibernation::HibernationScale` |
| `m2ue_save_layers` | the names of the layers a save file has loaded (its `tLayerData`) | `mercs2_formats::save` |
| `m2ue_atmosphere_at` | the light of the atmosphere setting that applies at a place: sun direction, light colour, rim colour, six ambient faces, Mie phase constant | the `AtmosphereBase` records and `LineRegion` polygons of the static world's layers, and the `worldentity`'s (`mercs2_formats::schema`, `mercs2_formats::worldentity`) |
| `m2ue_model_exists` | whether the level archive ships a model | `mercs2_engine::wad::has_model` |
| `m2ue_length` | a length in metres, converted | `space::length` |
| `m2ue_hash` | `pandemic_hash_m2` of a name | `mercs2_formats::hash` |

A mesh is one vertex buffer, one triangle list and its sections. A section carries what the game's
draw gate reads, so the caller can build Unreal LODs from the game's own tiers:

- `lod_mask` — the tiers the draw is drawn at (bit `n` = tier `n`), after a finer LOD block has taken
  over the tiers it re-authors;
- `node_enabled` — whether the draw shows on an entity with no destruction state (0 for wreckage);
- the material: shader key, flags, whether it is alpha-tested or blended, texture hashes by slot, pixel
  constants; for a terrain ground draw, the global map, detail map, four layers and their planar
  projections;
- what the material's pixel shader samples (`diffuse_map`, `slot1_map` with `slot1_role`,
  `normal_map`): a slot means what the shader's sampler at that stage is named in the game's shader
  store (`shader3.bin`, beside the level archive), read by `mercs2_engine::render::material_slots`.
  Stage 1 is a specular mask or an emissive map depending on the shader;
- on a stand-in's section, `object` — the entity it stands in for — and `ruined`, which of that
  entity's two states the section is.

## When the game draws what

Four rules decide whether and how an entity is drawn, and the library hands each one over as a
distance in Unreal units or a set of names:

- **Wake radius** (`M2uePlacement::wake_radius`). The game draws an entity only while a viewer is
  within `trunc(1.2 × Distance)` metres of it on the ground plane, `Distance` being its template's
  hibernation distance (`HibernationScale::single_player`).
- **Stand-ins** (`M2UE_PLACEMENT_TINY_STAND_IN`). One model per 200 m cell holds a part for each world
  object of the cell; the game draws an object's part while the object itself is not drawn, that is,
  from the object's wake radius outward.
- **Model tiers** (`m2ue_mesh_lod_switch_distance`). The selector (`FUN_00470740`) takes
  `x = distance / (reference × factor)` and draws tier `floor(log2 x) + 1`, held to the model's tiers:
  tier `n` starts at `reference × factor × 2^(n-1)`. `reference` is the model header's `+0x38`;
  `factor` is the main camera view's (`data::MAIN_VIEW_LOD_FACTOR`, read from retail memory).
- **Layers** (`m2ue_layers`, `m2ue_save_layers`). The layers of the `layers_static` block are the static
  world; the game's scripts add and remove the others as a save progresses, and a save names the ones
  it has loaded.

## Space

The game's world is Y-up and in metres, Unreal's is Z-up and in centimetres:

```text
(x, y, z) m  →  (100·x, 100·z, 100·y) cm
```

Exchanging two axes is a reflection. Retail's projection carries a mirror (`m00` = -1.92 at viewport
`+0xB30` with the camera's right vector at +X, the `23fe7f38` PMC capture), so looking along +Z, +X is
on the left of the screen; Unreal's axes are X forward, Y right, Z up with no mirror. The reflection
puts the game's screen-right on Unreal's for every view direction. From it:

| | Game | Unreal |
|---|---|---|
| direction | `(x, y, z)` | `(x, z, y)` |
| rotation quaternion | `(x, y, z, w)` | `(-x, -z, -y, w)` |
| triangle `(a, b, c)` | `cross(b - a, c - a)` along the normal | same order; the cross product opposes the normal, Unreal's front face |
| texture coordinate | V = 0 at the top row | unchanged |

`src/space.rs` tests each row; `tests/retail.rs` measures the triangle row on shipped models.

## Building and linking

```bash
cargo build -p mercs2_ue --release      # target/release/libmercs2_ue.a
```

The archive carries the engine crate's dependencies, so the program that links it also links their
system libraries. The list for a target is printed by:

```bash
cargo rustc -p mercs2_ue --release --lib --crate-type staticlib -- --print native-static-libs
```

On `aarch64-apple-darwin` that is the frameworks AudioUnit, CoreAudio, IOKit, QuartzCore, CoreGraphics,
Metal, Foundation, CoreFoundation, ApplicationServices, CoreVideo, Carbon and AppKit, and the libraries
`objc`, `iconv` and `m`.

The archive also contains the vendored Lua 5.1.5 objects (`lapi.o` … `lzio.o`, from `mercs2_luac`); a
program that links another Lua links two sets of `lua_*` symbols.

Those C objects are compiled for the SDK's own macOS version unless `MACOSX_DEPLOYMENT_TARGET` says
otherwise, and a linker targeting an older version warns about each one. For Unreal, build with the
version UnrealBuildTool prints as "minimum Target OS version" (14.0 for UE 5.9):

```bash
MACOSX_DEPLOYMENT_TARGET=14.0 cargo build -p mercs2_ue --release
```

UnrealBuildTool does not treat the archive as an input of the module that links it: after replacing
`libmercs2_ue.a`, delete the plugin's built `.dylib` so the next build links again.

## Calling it

```c
#include "mercs2_ue.h"

M2ueGame *game = m2ue_open("/path/to/data/vz.wad");
if (!game) { /* m2ue_last_error() says why */ }

M2ueMesh *hall = m2ue_model_load(game, m2ue_hash("pmcoutpost_interior_hq"));
size_t vertex_count, index_count, section_count;
const M2ueVertex  *vertices = m2ue_mesh_vertices(hall, &vertex_count);
const uint32_t    *indices  = m2ue_mesh_indices(hall, &index_count);
const M2ueSection *sections = m2ue_mesh_sections(hall, &section_count);
/* ... build the mesh ... */
m2ue_mesh_free(hall);

M2ueLayer *layer = m2ue_layer_load(game, m2ue_hash("Vz_State_PmcInterior"));
size_t count;
const M2uePlacement *placed = m2ue_layer_placements(layer, &count);
/* placed[i].kinds says what each entity draws; position and rotation are Unreal's */
m2ue_layer_free(layer);

m2ue_close(game);
```

A call that fails returns `NULL` (or 0 where it returns a number) and leaves a message for
`m2ue_last_error()`, valid on that thread until its next call. A panic inside the library is caught at
the boundary and reported the same way. A game handle is used by one thread at a time.

## Modules

- `space` — game space to Unreal space.
- `atmosphere` — the atmosphere settings, their regions, and the light a setting gives.
- `data` — the decoders' output posed at rest, converted, and laid out as the header's records.
- `ffi` — the C interface.

## Notes

- **`m2ue_open` mounts what the game mounts.** A `vz-patch.wad` or language archive beside the level
  archive is part of the stack, so an installed mod's layers and assets are returned with the game's.
- **Models and terrain cells are read from the level archive**, as `Model::load` and the engine's
  terrain loader read them; a model only an overlay archive ships is not found.
- **Meshes are posed at rest and carry no skeleton.** Skinning weights, bones and animation are not
  part of this interface.
- **Light intensity is the game's own number** (`LightObject` `params[0]`), in no Unreal unit.
- **A character's model is a kit.** A placed person (`M2UE_ARCHETYPE_CHARACTER`) names a model whose
  visible parts an outfit selects on an animated skeleton; `m2ue_model_load` on it gives every part.
- **An atmosphere is one setting, settled.** The level ships `AtmosphereBase` records, each a named
  setting (`default`, `warzone`, `night`, `pmc`, …) on a region of the map or, for outside every
  region, on entity `0x900000CF`. The game blends its live atmosphere toward the setting of the region
  the viewer is in, over the setting's own blend time; `m2ue_atmosphere_at` gives the setting itself.
  - The region is the last one, in the records' order, whose polygon holds the point. Where regions
    overlap, the game's own choice also depends on the path the viewer took.
  - A region's setting is `default` until a mission script changes it; the call takes the name.
  - A region a save's layers add on top of the static world is not read.
  - The ambient faces, the rim colour and the Mie constant are the record's values. **The sun's
    direction and the light's colour are derived, and the exe's own producer of them is not found.**
    The direction is a fit to five retail captures (`(-cos θ · cos A, sin θ, -cos θ · sin A)`, θ = π ·
    `LightTime`, A = -150°, the live `LightAngle` in all five). The colour is `LightMultiplier ×
    LightIntensity`; the game's is that times a tint that reddens as the sun gets lower (1.03 to
    1.10 in red, 0.82 to 0.94 in blue in the captures).
  - `data/atmosphere_captures.tsv` holds the light block the game builds every frame
    (`FUN_00465630`, `0x018C2190`) as read from four retail memory captures; its header names each
    column's address and each row's capture. The retail test holds the records against it: the PMC
    capture's block is the `pmc` setting of the interior region, to the last digit in the ambient
    faces and the rim colour.
- **The tier distances are the main camera's.** The selector's factor is per view and no writer of it
  is known; the runtime's lowest tier (`M+0x80`) has no known source in the files and is taken as 0.
- **Record layouts are pinned on both sides**: `const` assertions in `src/ffi.rs` and
  `M2UE_STATIC_ASSERT` in the header hold the same sizes and offsets.

## Tests

`cargo test -p mercs2_ue` runs the conversion, posing and interface tests without the game.
`cargo test -p mercs2_ue --features retail --test retail` (or `cargo xtask retail-test`) reads the
retail `vz.wad` named by the repo-root `.mercs2-local.toml` through the C interface.
