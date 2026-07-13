# mercs2_formats

The asset-format layer of the Mercenaries 2 modernization project: readers, writers and authoring
tools for the game's WAD/sges/UCFX container stack and everything inside it — meshes, textures,
terrain, world placements, Havok collision and animation, saves and Scaleform movies.

## What it is

`mercs2_formats` is the crate every other crate in the workspace parses bytes with. It contains no
rendering, no simulation and no I/O policy — just format code:

* **Archive layer.** `ffcs` reads the 256-byte FFCS WAD header and its `INDX` / `ASET` / `PTHS`
  tables (little-endian on the PC bake, big-endian on the console bakes). `sges` inflates a block
  (segmented deflate) and also re-compresses one. `ucfx` walks the UCFX descriptor tree inside a
  decompressed block, resolves leaf chunks and verifies the `CSUM` trailer.
* **Asset decoders.** Model geometry and materials (`texture`, `schema`, `skeleton`), textures
  incl. the high-mip chain (`texture`, `texsize`), low-res world terrain (`terrain`), world
  placements (`placement`), the world block index used by the streaming engine (`world_index`),
  FX dictionaries (`fxdict`), sky/HDR parameters (`atmosphere`), destruction state machines
  (`orchestrator`), Havok 5.5 collision (`havok`) and animation clips (`anim`, `animgroup`,
  `anim_select`), `.profile` saves (`save`, `save_write`), and Scaleform `.gfx` movies (`gfx`).
* **Authoring / write side.** `model_build` (author a static model container from scratch),
  `model_inject` and `model_cubeize` (conform novel geometry into a real donor container),
  `placement_build` (append a new world placement), `patch_wad` (serialize a PC `vz-patch.wad`),
  `save_write`, and the DLC-port readers `dlc_input` / `dlc_stfs`.

Sixteen binaries and five examples in the crate drive those paths from the command line (see
[Usage](#usage)).

The crate has exactly one dependency: `flate2`.

## Where it comes from

Everything here is reverse-engineered from the retail PC build and its console siblings. The
provenance the source itself records:

* **Ports of the project's earlier Python tools**, kept faithful rather than "improved":
  `tools/placement_extractor.py` (→ `placement`), `tools/terrain_extractor.py` +
  `tools/ucfx_mesh_codec.py` (→ `terrain`), `tools/x360_dlc_io.py` (→ `dlc_input` + `dlc_stfs`),
  `tools/ffcs_patch_wad.py` (→ `patch_wad`), `tools/pandemic_hash.py` (→ `hash`),
  `tools/aset_type_ids.py` (→ `aset_type_ids`), `tools/ucfx_be_to_le.py` (→ `crc32`).
* **The decompiled exe.** `tag_registry` is seeded from a scan of `cmp eax, <imm32>` tag
  comparisons in the plaintext image `output/patched/Mercenaries2.exe` (base `0x00400000`) —
  232 distinct FourCCs, each with the dispatch address it was found at. `orchestrator`'s state
  machine mirrors `FUN_004cf340`; `fxdict` mirrors the container loader `FUN_00491320` and the
  effect loader `0x492AF0`. The wavelet animation decode is a port of
  `FUN_009f5b90 → FUN_009f54f0 → FUN_009ff120/FUN_009fdd50/FUN_009fe5b0` plus `StRecomposeW`
  (`FUN_009fb870`).
* **Live x32dbg captures.** The wavelet decoder is gated by two integration tests
  (`tests/wavelet_decompress.rs`, `tests/wavelet_recompose.rs`) that replay a captured frame from
  `tests/fixtures/wavelet_capture_2p567s/` and must reproduce the engine's own coefficients.
* **Format docs in this repo**, cited per-module: `docs/format_reference.md`,
  `docs/ucfx_tag_registry.md`, `docs/fxdict_format.md`, `docs/placement_data_format.md`,
  `docs/schm_type_codes.md`, `docs/type_hash_registry.md`, `docs/coordinate_systems.md`,
  `docs/modernization/world_streaming_spec.md`,
  `docs/reverse_engineer/valid_model_structure_map.md`. Two format write-ups live in the crate:
  [`SAVE_FORMAT.md`](SAVE_FORMAT.md) and the deferred-work list [`DEFERRED.md`](DEFERRED.md).
* **Havok.** `havok` / `anim` read Havok **5.5.0-r1** little-endian packfiles as shipped in `PHY2`
  (collision) and the animation blocks.

## Usage

As a library — open `vz.wad`, resolve an asset by name hash, and pull a texture ready for a
`wgpu` BC upload:

```rust
use std::fs::File;
use mercs2_formats::{ffcs, hash, sges, texture, types, ucfx};

let mut f = File::open("vz.wad")?;
let size = f.metadata()?.len();
let archive = ffcs::load_ffcs_archive(&mut f, size)?;   // INDX + ASET + PTHS + endian

// High-level: ASET lookup -> block decompress -> UCFX walk -> INFO/BODY parse.
let name = hash::pandemic_hash_m2("pmc_hum_mattias_v2_dm");
let tex = texture::extract_texture(&mut f, &archive, name)?;
println!("{}x{} {:?}, {} mips, {} bytes",
         tex.width, tex.height, tex.format, tex.mip_count, tex.all_mips.len());

// Low-level: do the same steps by hand for any block.
let entry = archive.aset.iter()
    .find(|a| a.asset_hash == name && a.type_id == types::TYPE_ID_TEXTURE)
    .expect("not in ASET");
let block = sges::decompress_block(&mut f, &archive.indx, entry.block_index())?;
let (parsed, issues) = ucfx::walk_decompressed_block(&block, "vz.wad");
let container = ucfx::get_container_by_type_hash(
    &parsed, types::TYPE_HASH_TEXTURE, Some(name),
).expect("no texture container");
assert!(issues.is_empty());
let _ = texture::parse_texture_container(&container)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

As tools — the crate's binaries are auto-discovered from `src/bin/`:

```sh
# dump the collision hulls in a model container's PHY2 chunk
cargo run -p mercs2_formats --bin phy2_probe -- <model_block.bin>

# inventory the extracted retail Scaleform movies into a golden reference set
cargo run -p mercs2_formats --bin gfx_golden

# conform a novel rigid mesh into a real vehicle/static donor container
cargo run -p mercs2_formats --bin inject_static -- --help
```

## Modules

| Module | Owns |
| --- | --- |
| `ffcs` | FFCS WAD header + `INDX` / `ASET` / `PTHS` tables; `load_ffcs_archive`. |
| `sges` | `sges` segmented-deflate block decompression/compression; whole-block and head-only reads. |
| `ucfx` | UCFX descriptor-tree walk, chunk-body extraction, container `CSUM` verification. |
| `chunk_validate` | Validators for the documented UCFX chunk layouts (retail PC). |
| `tags` | `ChunkTag` enum for every known UCFX descriptor tag. |
| `tag_registry` | Every FourCC the engine dispatches on (232), with dispatch address, subsystem and verification status. |
| `types` | ASET `type_id` and UCFX `type_hash` constants for the retail `vz.wad`. |
| `aset_type_ids` | `type_hash` → `type_id` map. |
| `hash` | Pandemic FNV-1a hashing: `pandemic_hash` (Mercs 1) and `pandemic_hash_m2` (Mercs 2). |
| `crc32` | Mercs 2 `CSUM`: CRC-32, init 0, no final XOR. |
| `schema` | ECS `COMP` reflection: `schm` field-type codes, component schemas, `parse_comp_groups`. |
| `safe_slice` | Bounds-checked byte buffer that models engine pointer dereferences. |
| `texture` | `MTRL` material parse, per-group/per-`PRMT` material binding, texture container → raw DXT/BC (incl. hi-res mip assembly). |
| `texsize` | DXT mip-chain surface sizing — the single source of truth shared by converter and validator. |
| `skeleton` | `HIER` bone hierarchy → per-bone world rest transforms. |
| `havok` | Little-endian Havok 5.5 packfile reader (the `PHY2` collision path). |
| `anim` | Havok `hkaAnimation` clip decoder → sampleable per-bone local pose. |
| `animgroup` | The `animation` WAD block (`0x18166555`): clips + track→bone mapping for GPU skinning. |
| `anim_select` | The engine's data-driven clip picker (`animationtable` assets, `0x207359C7`). |
| `mannequin` | Procedural humanoid mesh fitted and auto-weighted to a real skeleton. |
| `retarget` | Foreign-rig retarget driver (uses the source glb's own `JOINTS_0`/`WEIGHTS_0`). |
| `model_build` | Author a static UCFX model container from scratch (no donor). |
| `model_cubeize` | Rewrite a real model container's geometry to a cube, in place. |
| `model_inject` | Inject external mesh geometry into a real model donor container. |
| `orchestrator` | Destruction: the per-model state machine (`SWIT`/`NODE`/`STAT`/`CHDR`/`CEXE`). |
| `placement` | `layers_static` world-placement loader (WAD block 29). |
| `placement_build` | Append a new `SceneObject` placement without overriding an existing entity. |
| `world_index` | Layer-1 world block index: class, LOD tier/variant, state overlay, spatial extent of every block. |
| `world` | World spatial constants used for validation. |
| `terrain` | Low-resolution world terrain loader. |
| `atmosphere` | `Graphics.Atmosphere.*` sky / HDR tone-map / bloom parameter model. |
| `fxdict` | FX cluster: `fxdict` `DICT` + effect-template key chunks. |
| `gfx` | Scaleform GFx / SWF tag-stream parser and feature inventory. |
| `save` | PC `.profile` save parser (13,404 bytes; zlib Lua payload at `0x468`). |
| `save_write` | The inverse: rebuild the container and stamp a correct `ProfileHash`. |
| `patch_wad` | FFCS patch-WAD assembly — the canonical serializer for a PC `vz-patch.wad`. |
| `dlc_input` | Big-endian Xbox 360 DLC readers: BE FFCS/INDX/ASET/PTHS and the BE `sges` decompressor. |
| `dlc_stfs` | STFS (Xbox 360 secure container) reader + RAR extraction. |

## Notes / gotchas

* **No coordinate conversion is applied.** `anim` returns Havok values verbatim: right-handed,
  +Y up, metres, quaternions `(x,y,z,w)`. Mercenaries 2 game space is **left-handed, +Y up**
  (`docs/coordinate_systems.md`) — the RH→LH conversion is the integrator's job, not this crate's.
* **One Havok packfile walker.** `anim` reuses `havok::parse_packfile_raw` for the
  section-header / classname / fixup pass and only adds the animation classes on top. Do not
  re-implement the walker. (`ucfx_byteswap::havok` is a different thing: a BE→LE *rewriter* for
  PS3 packfiles.)
* **Authoring from scratch is the hard path.** The engine rejects a hand-built model container's
  decl/material/shader bindings (`0x004CC064`), which is why `model_inject` / `model_cubeize`
  conform novel geometry into a *real donor* container the engine already accepts, touching only
  geometry, material hashes and the `CSUM` — every byte not rewritten stays valid.
* **`decompress_block` handles all three block forms**: `sges` (deflate segments), a raw `UCFX`
  block, and a raw block — dispatching on the leading magic and truncating to the INDX
  decompressed page count.
* **`dlc_stfs` shells out** to `UnRAR.exe` for RAR extraction, mirroring the subprocess pattern
  used elsewhere in this workspace.
* **`atmosphere` is not a WAD chunk.** The world's Lua assembles the sky/HDR look through the
  named `Graphics.Atmosphere.*` key/value API bracketed by `Begin()`/`End()`; the module models
  that runtime namespace, not a file layout.
* Known non-blocking gaps are tracked in [`DEFERRED.md`](DEFERRED.md) (e.g. `save_write` still
  needs a real `.profile` as a template to supply a handful of unexplained constant bytes).
