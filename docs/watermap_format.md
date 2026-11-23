# Watermap (`0x4D7D30C4`) / `watr` Chunk

**Verified:** retail PC `blocks\VZ\resident_P000_Q3.block` (2026-05-30)  
**Tools:** [`tools/watermap_probe.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/watermap_probe.py), [`tools/watermap_decode.py`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/watermap_decode.py)

---

## Location

| Field | Value |
|-------|-------|
| type_hash | `0x4D7D30C4` (`pandemic_hash_m2("watermap")`) |
| ASET entries | 1 (singleton) |
| Block | `resident_P000_Q3` |
| UCFX | Single chunk tag `watr`, ~495,669 bytes |

---

## `watr` layout

The header is 11 words, read in order by the loader (PC v1.1 `FUN_00481290`, v1.0 `FUN_004811C0`, Xbox
`FUN_821D20D8`, PS3 v1.03 `FUN_00244520`); the offsets in the object are the loader's stores.

| Offset | Size | Type | Field | Retail | Object |
|--------|------|------|-------|--------|--------|
| +0 | 4 | u32 | format version: below 5, the loader takes the old path (coarse grid derived, no coarse header words) | 5 | — |
| +4 | 4 | u32 | width | 257 | +0x18 |
| +8 | 4 | u32 | height | 257 | +0x1C |
| +12 | 4 | f32 | cell size, m | 32.0 | +0x28 |
| +16 | 4 | f32 | lowest height | −50.0 | +0x58 |
| +20 | 4 | f32 | highest height | ~325.26 | +0x5C |
| +24 | 4 | f32 | coarse cell size, m | 64.0 | +0x2C |
| +28 | 4 | u32 | coarse width | 129 | +0x20 |
| +32 | 4 | u32 | coarse height | 129 | +0x24 |
| +36 | 4 | f32 | coarse bias, m | −49.8317 | +0x50 |
| +40 | 4 | f32 | coarse scale, m per step | 0.0056970 | +0x54 |
| +44 | 264,196 | f32[width·height] | **Layer 0 — height (m)** | | +0x3C |
| +264,240 | 66,049 | u8[width·height] | **Layer 1 — wet mask** | | +0x40 |
| +330,289 | 66,049 | u8[width·height] | **Layer 2** | | +0x44 |
| +396,338 | 66,049 | u8[width·height] | **Layer 3** | | +0x48 |
| +462,387 | 33,282 | u16[coarse w·h] | **coarse heights**: `value · scale + bias` m | | +0x4C |

**Size check:** `44 + 66049×4 + 66049×3 + 129×129×2 = 495,669` bytes.

The old path (version below 5) sets the coarse cell to twice the cell, the coarse grid to `(n >> 1) + 1` a
side, the bias to −36.0 and the scale to 0. An earlier revision of this table read the format version as a
layer count, the coarse width and height as unknown words (they print as 0 when read as floats), the bias and
scale as the first two heights, and the coarse grid as an unknown footer.

The console (Xbox, PS3) body is the same layout big-endian. Swapping the header words and the heights as
u32, keeping the masks' bytes and swapping the coarse grid as u16, gives the retail PC body byte for byte
(`be_to_le::watermap`, `chunk-compare --only watermap`: 1 of 1 equal). PS3 v1.03 then folds the wet mask into
each height's lowest bit after loading and frees layer 1; its sampler reads wetness from those bits.

---

## Layer semantics

### Layer 0 — height (f32)

- Metres in game **Y-up** space.
- **Dry cells:** often exactly **-50.0** (matches `height_min_m`).
- **Wet cells:** plateau near **-36.0** m (open water surface in the watermap).
- Inland water and bathymetry use values between these extremes (max ~325 m on retail).

### Layer 1 — wet mask (u8)

| Value | Meaning | Status |
|-------|---------|--------|
| `0` | Dry / land | confirmed |
| `255` | Wet / water column | confirmed |

Cell counts on retail: ~27,973 dry, ~38,070 wet.

### Layers 2–3

Mostly `255` with sparse other bytes at shores and isolated cells. Likely coastal/shore typing and per-cell overrides — **not confirmed** in EXE.

### Coarse heights

A 129 × 129 grid of 64 m cells on the same origin, `u16 · scale + bias` metres. The reflection plane is
`y = h + 1` with `h` the coarse height under the camera (PC v1.1 `FUN_00487070`).

### What the engine samples and draws

- **Height query** (PC v1.1 `FUN_004817E0`): `fx = (x − origin_x) / cell`, `fz` likewise, floored and clamped;
  the height is bilinear over the four corners; the point is water unless all four corners' layer 1 is 0.
  The underwater test (`FUN_00480470`) subtracts a time-based wave sum.
- **Drawn band** (PC v1.1 `FUN_00486460`): the lowest and highest height of every cell in the view frustum's
  XZ rectangle, wet or dry, widened by the wave amplitude; the projected water grid is clipped to it
  (`FUN_00480640`), and the water is hidden when nothing intersects.
- **Height and mask textures** (Xbox `WaterHeightMap`, `0x821D4278`): a 256 × 256 16-bit texture of the
  heights normalised to the header's lowest/highest ± the wave amplitude, and a 4444 texture of the three
  mask layers. PC builds them in SecuROM-virtualized code (INFERRED the same).

No build reads a level-specific sea level: the wet mask and the heights are the whole description.

---

## World extent mapping

| Quantity | Value | Status |
|----------|-------|--------|
| Sample grid | 257 × 257 | confirmed |
| Cell size | 32 m | confirmed (header) |
| Span (256 intervals) | 8192 m × 8192 m on X/Z | confirmed |
| Game coord coverage | ±4096 m, centred | confirmed (loader) |
| Playable bbox (AGENTS.md) | X/Z ≈ ±3900 m | reference |

**Index → game XZ.** The loader computes the origin from the header: `origin = −(n − 1) · cell / 2` on each
axis (PC v1.1 `FUN_00481290`, constants −0.0 and 0.5); −4096 for 257 cells of 32 m.

```
x = origin_x + ix * cell_size_m
z = origin_z + iz * cell_size_m
y = height[iz * grid_width + ix]
```

**Retail sea level:** wet-surface height ≈ **-36 m** (not terrain Y=0 open-water tiles).

---

## UE5 (`setup_water.py`) constants

| Constant | Current value | `watr` tie-in |
|----------|---------------|---------------|
| `OCEAN_HALF_M` | 5000 | OK margin over 4096 m half-span |
| `WORLD_BBOX_HALF_M` | 4000 | Matches playable ±3900 m |
| `SEA_LEVEL_UE` | -2500 (empirical) | **Not calibrated** to -36 m game height |

**Next step for raster-driven water:** sample layer 0 + mask 1 → spawn `WaterBodyOcean` / custom mesh; map game Y to UE Z via `game_to_ue` (100×, swap Y↔Z). Calibrate ocean Z against a known coastal placement + wet-cell height.

---

## Extraction

```bash
.venv/Scripts/python tools/extract_single_block.py \
  --wad game-files/pc-game-vz.wad \
  --path "blocks\VZ\resident_P000_Q3.block"

.venv/Scripts/python tools/watermap_decode.py --wad game-files/pc-game-vz.wad \
  --out output/watermap_decode.json --png
```

PNG exports (16-bit): `output/_scratch/watermap/layer00_height_u16.png`, `layer01..03_mask_u8_stretched.png`.
