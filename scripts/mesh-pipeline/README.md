# Mesh & texture pipeline scripts

The front-end for bringing a **novel model** (or texture) into Mercenaries 2. These convert a
source asset (FBX / glTF / PNG) into the raw blobs the Rust tools (`model_forge`, `inject_parts`,
`inject_static`, `smuggler`) turn into a shippable `vz-patch.wad`.

They are bundled here so you don't have to hunt for them — but they are a *convenience front-end*,
not the engine tooling. The authoritative, cross-platform tools are the Rust binaries on the release
(`model_forge`, `smuggler`, `wad_builder`, `wad_simulator`, …); these scripts only prepare their inputs.

## Requirements

- **Python 3** with **numpy** (`pip install numpy`) — for every script except the two Blender ones.
- **Blender** (any recent 3.x/4.x) — only for `fbx_preprocess.py` and `gltf_parts_preprocess.py`,
  which run *inside* Blender (`blender -b -P script.py -- …`) to use its importers and decimator.

## The scripts

| Script | Needs | Does |
|--------|-------|------|
| `fbx_preprocess.py` | Blender + numpy | FBX → universal mesh `.npz` (pos/nrm/uv/tris). Decimates to a triangle budget, unwelds to per-corner verts. |
| `gltf_parts_preprocess.py` | Blender | glTF → one engine mesh **per material**, each decimated. For multi-part / multi-material models. |
| `npz_to_mesh.py` | numpy | `.npz` → the raw **`.mesh`** blob `model_forge` reads. Sizes the model (`--target-height`, `--ground`). |
| `gltf_to_ucfx_model.py` | numpy | Conform a universal mesh `.npz` **into a reference/donor UCFX container** → a new UCFX model container. Args: `<donor_container> <model.npz> <out.bin>`. The Python analogue of the Rust `inject_static` (donor-based), distinct from `model_forge` (from-scratch). |
| `png_to_dds.py` | numpy | PNG/JPEG → uncompressed RGBA `.dds`, resized to a game-sane power-of-two. |
| `dds_to_ucfx_texture.py` | numpy | RGBA `.dds` → UCFX **texture container** (DXT1, fully resident). |
| `nm_to_ucfx_dxt5nm.py` | numpy | Normal map → UCFX texture container (DXT5nm, fully resident). |

Run any of them with no arguments (or `--help` where supported) to print its exact usage.

## The pipeline

**Model — FBX path:**

```bash
# 1. FBX -> universal mesh (decimated to a triangle budget)
blender -b -P scripts/mesh-pipeline/fbx_preprocess.py -- model.fbx model.npz --tris 9000

# 2. universal mesh -> raw .mesh blob (sized, ground-aligned)
python scripts/mesh-pipeline/npz_to_mesh.py model.npz model.mesh --target-height 1.83 --ground

# 3. .mesh -> UCFX model container (--skinned for a wardrobe character)   [Rust]
model_forge model.mesh model.bin --name rsg_thing --diffuse 0x68E14661

# 4. ship it as a NEW asset, overriding nothing                          [Rust]
smuggler --source-wad vz.wad --extra-only --inject-extra 0xHASH:19:model.bin --output vz-patch.wad

# 5. validate                                                            [Rust]
wad_simulator --wad vz-patch.wad --base-wad vz.wad --skip-audio
```

**Texture (for the `--diffuse` hash in step 3, or an override block):**

```bash
python scripts/mesh-pipeline/png_to_dds.py skin.png skin.dds
python scripts/mesh-pipeline/dds_to_ucfx_texture.py skin.dds my_skin skin_tex.bin
smuggler --source-wad vz.wad --extra-only --inject-extra 0xTEXHASH:27:skin_tex.bin --output vz-patch.wad
# type_id 27 = texture, 19 = model
```

## Note on decimation

`fbx_preprocess.py` / `gltf_parts_preprocess.py` use **Blender's decimate modifier** (quadric
edge-collapse) — the one step in this pipeline that still needs Blender. If your source model is
already at a sane triangle count you can skip the Blender step entirely. A Rust-native decimator that
would remove the Blender dependency is a known follow-up.
