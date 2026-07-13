# wad_builder

Turns raw/edited assets into engine-format blocks and patches them into a `vz-patch.wad`, with a
byte-exact identity oracle proving the parse/serialize model is correct before any edit is applied.

## What it is

A CLI (`wad_builder`, `src/main.rs`) for the *build* side of Mercenaries 2 asset injection: it opens a
decompressed engine block or a patch WAD, performs a surgical edit, re-serializes, and re-verifies.
Every write path self-checks — re-parse the output, re-verify all container CSUMs — before the file
hits disk.

Three families of subcommands:

**Block-level edits** (operate on a *decompressed* block `.bin`):

| Command | What it does |
| --- | --- |
| `identity-test` | The correctness oracle: parse a block, verify every container CSUM, re-serialize and assert byte-for-byte equality with the input, then dump one entry's container layout. |
| `extract-lua` / `replace-lua` | Pull out / swap one script's LuaQ bytecode inside a multi-entry `scripts_vz` block (fixes BINN `body_size`, the BINN metadata `bytecode_size`, the CSUM and the entry's `chunk_size`). |
| `fix-model-mtrl` | Un-transpose the `[u16 flags][u16 count]` pair in MTRL material records `1..n` left swapped by `ucfx_byteswap::convert_mtrl`. |
| `fix-model-vertices` | Un-transpose FLOAT16 vertex positions in mixed-decl STRM streams that `apply_strm_vertex_fix` skips. |
| `unwrap-mesh` | Strip `AREA` chunks from static-`MESH` groups inside a skinned model; `--drop-slots` removes the MESH slots entirely (drop the GEOM count, trim `INDX`). |
| `reskin-eyes` | Re-rig static `MESH` eye slots into `SKIN` groups: decl 24→32 (insert BLENDINDICES + BLENDWEIGHT), INFO(60 PgMesh)→INFO(56 PgSkin), drop `AREA`, retag, fix sibling spans. One `--bone` per slot. |
| `set-tex-specular` | Set the specular flag `INFO byte[8] = 0x20` on a texture block and recompute CSUM. |
| `rebuild-resident-tex` | Fill a streamed texture's empty resident body by concatenating its P-tier streaming page bodies in descending-mip order (P003, P002, P001) and truncating to the resident body size. |
| `repoint-tex` | Rewrite MTRL texture hashes per repeatable `--remap OLD:NEW` (8-hex) pairs. |
| `build-atlas` | Pack N decompressed single-entry texture blocks into one multi-entry atlas block (count + entry table + UCFX bodies). |

**Patch-WAD edits** (operate on a `vz-patch*.wad`, via `mercs2_formats::patch_wad`):
`list-blocks`, `dump-block` (decompress one block out), `replace-block` (sges-recompress new
decompressed bytes in), `filter-keep`, `drop-blocks`, `merge-blocks` (copy blocks from a source patch
WAD into a target, preserving compressed data + ASET entries).

**End-to-end:** `build-skin` — open a patch WAD, decompress its `scripts_vz` block, replace one script's
LuaQ, re-verify CSUMs, sges-recompress (with a decompress round-trip check) and rebuild the WAD,
then re-read the output and assert the edit is present.

## Where it comes from

Provenance as recorded in the source itself:

* Block/container layout and the CSUM model: `docs/modding_deep_dive.md` §4.6 / §5.2 / §5.3. Each
  UCFX container ends with an 8-byte `CSUM` trailer = `"CSUM"` + CRC-32/JAMCRC
  (`mercs2_formats::crc32::crc32_mercs2`) over `[UCFX .. pre-CSUM]`. Names are matched by
  `pandemic_hash_m2`.
* `packed_field` (the INDX page count) sizes the engine's decompression destination buffer as
  `page_count << 15` (32 KB pages) — engine `FUN_00875b00`. `replace-block` and `merge-blocks`
  recompute it from the decompressed size; a stale/undersized value overruns the dest buffer into
  adjacent heap (observed as the `0x6B6FDA` render-singleton vtable crash).
* `reskin-eyes` / `unwrap-mesh` were derived from a live x32dbg session (2026-06-29) on Sarah's model:
  static `MESH` eye slots inside a skinned character are stepped over by the skinned-mesh consumer
  `MeshSkin_ConsumeChunk` (`FUN_004796f0`), so their vertex buffer is built zero-size, D3D
  `CreateVertexBuffer` returns null, and the game dies at `0x0085C8D0`. The working all-skinned
  `pmc_hum_obama` model supplied the INFO(56) PgSkin byte template (`OBAMA_EYE_INFO56` in `main.rs`,
  shaders `PgSkinNoTangentVP` 0x4e8f3ae1 / `PgSkinShadowVP` 0x0f8fae93).
* `fix-model-mtrl`: the engine writes `count` 12-byte `{hash, 0xF011157A, 0}` records into a FIXED
  10-slot array; a transposed count overruns it and faults in `Mtrl_Parse`.

## Usage

Run from the `tools/wad_simulator` workspace root.

```sh
# 1. Prove the container model round-trips the real block byte-for-byte.
cargo run -p wad_builder --release -- identity-test --block scripts_vz.bin

# 2. What's in a patch WAD?
cargo run -p wad_builder --release -- list-blocks --patch-wad output/data/vz-patch.wad

# 3. Swap a Lua script inside the patch WAD's scripts_vz block and rebuild the WAD.
cargo run -p wad_builder --release -- build-skin \
    --patch-wad vz-patch-human.wad \
    --script wifpmcinterior \
    --luac wifpmcinterior.luac \
    --out vz-patch.wad

# 4. Pull a block out, edit it, put it back (page_count is recomputed for you).
cargo run -p wad_builder --release -- dump-block --patch-wad vz-patch.wad --path pmc_hum_sarah --out sarah.bin
cargo run -p wad_builder --release -- fix-model-mtrl --block sarah.bin --out sarah_fixed.bin
cargo run -p wad_builder --release -- replace-block --patch-wad vz-patch.wad \
    --path pmc_hum_sarah --data sarah_fixed.bin --out vz-patch-new.wad

# 5. Copy blocks from one patch WAD into another (target = --patch-wad, source = --from).
cargo run -p wad_builder --release -- merge-blocks --patch-wad output/data/vz-patch.wad \
    --from output/parts/tank.wad --block pmc_veh_tank --out output/data/vz-patch.wad
```

## Modules

Binary crate — no public library API. Internal modules:

* `scripts_block` — the `ScriptsBlock` / `Entry` container model: parse, `serialize`, `verify_csums`,
  `find_by_name` (by `pandemic_hash_m2`), `extract_lua`, `replace_lua`, plus `parse_container` for the
  UCFX/BINN/LuaQ/CSUM layout of one entry. Every model-edit subcommand reuses it as the block
  parse/re-emit spine.
* `model_mtrl` — MTRL material-array walker: `fix_container_mtrl` (count transposition),
  `repoint_container_textures` (hash remap).
* `model_vertex` — `fix_container_vertices`: un-transpose FLOAT16 positions in STRM vertex buffers.
* `model_unwrap` — `unwrap_container_mesh` (strip `AREA`) and `drop_container_mesh_slots` (remove the
  whole static-MESH slot, fix GEOM count + `INDX`).
* `model_reskin` — `reskin_container_eyes`: static `MESH` → skinned `SKIN` group conversion.

## Notes / gotchas

* Block subcommands take a **decompressed** block. Use `dump-block` to get one out of a WAD and
  `replace-block` to put it back (that path sges-compresses and fixes `packed_field`).
* Path matching (`--path`, `--keep`, `--drop`, `--block`) is a case-insensitive **substring** match on
  the block's path string, not an exact name.
* `replace-lua` refuses a `--luac` that does not start with the `\x1bLua` LuaQ signature. If the new
  bytecode is byte-identical to the old, it additionally asserts the rebuilt block reproduces the
  input exactly.
* `rebuild-resident-tex` requires the `--page` arguments in **descending** mip order (P003 first) and
  errors out if the concatenated page bodies are smaller than the resident body (a missing tier).
* `unwrap-mesh` without `--drop-slots` only strips `AREA` — the source records that this is
  insufficient on its own (the MESH wrapper still hides the subtree from the skinned walker); it is
  kept as the AREA-removal primitive. `reskin-eyes` is the fix that keeps all slots.
* `model_mtrl`'s fix is deliberately applied here rather than in the shared `ucfx_byteswap` converter:
  the converter-side full-array fix changed world-load behaviour and is parked. On-demand assets
  (skins via `SetOutfit`) have no world-load, so the post-convert correction is safe.
