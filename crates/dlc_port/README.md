# dlc_port

All-Rust porter that turns a Mercenaries 2 DLC package (PS3 `.pkg`, or Xbox 360 RAR or STFS/DOH) into PC
form. `--shipment <folder>` writes it into a Shipment as components: one file per asset rung under
`src/level`, the streams under `src/level/Audios`, and one generated `level_asset` row per asset after the
manifest's `contributions:` line. `qm build` compiles those, so `shipments/blow-it-up-again/` holds no WAD.

```
dlc_port --package <ps3.pkg> --vz-wad <game>/data/vz.wad --shipment <folder>
```

The PS3 package needs no other input: every one of its 7,770 chunks converts. `--xbox-doh-oracle <xbox package>`
names a further source, whose chain replaces any chunk PS3 cannot convert. A chunk is carried when the base lacks
its (name, type), or when its type is one a level overrides and its bytes are not PC's (`src/components.rs`).

## What it is

A single binary (`dlc_port`). It takes the Xbox 360 DLC container, walks its big-endian FFCS
index, converts every block to PC little-endian form, and re-assembles the result as an FFCS
patch WAD the PC game can load as an overlay.

The pipeline, in the order `src/main.rs` runs it:

1. **Load DOH bytes** — either `extract_stfs_from_rar` (RAR → STFS → `DLC01.doh`, unpacked by the
   `--unrar` or `--bsdtar` program named on the command line) or
   `load_stfs_or_doh_with_streams` (an STFS container and its `.pws` files, or a raw `.doh` passed
   straight through, which brings none).
2. **Parse the BE FFCS header** — `parse_be_ffcs` yields the chunk rows; `INDX`, `ASET`, `PTHS`
   and `CSUM` are pulled out by tag. `--list-blocks` stops here and prints page count, packed
   field and path per block.
3. **Per-block convert** — `TextureTable::from_doh` first reads every block, because a texture's
   `INFO` sits in one block and its rungs in others. `StreamTable::read` then re-encodes each `.pws`:
   XMA2 or MP3 decoded by ffmpeg, IMA ADPCM written the way a retail `.pws` lays it out, and the
   bank's records rewritten to match, so a streamed bank needs its file. A bank finds its file by
   hash: `vo_stream_dlctest.english.pws` is the entry `vo_stream_dlctest.english`. A streamed bank
   with no file, or a file no bank claims, fails the run. Then each block is either `segs`-decompressed
   (`decompress_be_sges`) or passed through when it is a bare `XFCU` entry table (trailing zeros
   trimmed, length rounded up to 4). The decompressed bytes go through
   `ucfx_byteswap::convert::convert_block` (BE→LE), are
   recompressed with `compress_sges`, and the result is immediately decompressed again and
   compared — a round-trip mismatch is a hard error. A block that fails to decompress or convert
   fails the run after the rest are tried, and the error lists every such block with its reason, so
   a WAD on disk holds every block in the range asked for.
4. **Rebuild `packed_field`** — `(xbox_tier << 24) | ceil(size / 0x8000)`, keeping the source
   block's tier byte but recomputing the page count for the converted (LE) payload.
5. **Route ASET by content** — see the gotcha below. Each row goes to the block of `_P000` holding
   its asset and names the blocks of `_P001`..`_P003` holding it, up to the first gap. The Xbox row
   is then an oracle: its type must be the entry's, and each of its block indices must name the same
   block throughout and be absent exactly where the content has no rung. A disagreement fails the run.
6. **Synthesise missing ASET rows** — any `SCRIPT_TYPE_HASH` or `STRINGDB_TYPE_HASH` entry in a
   block's table that has no ASET row gets one (`u1 = 0xFFFFFFFF`, `u2 = 0xFFFF`).
7. **Assemble** — `build_patch_wad_multi` with the source `CSUM` value/meta and `FFCS_CERT_BLOB`.
   Each stream is written to `Audios/<bank entry>.pws` beside the output, where the game keeps them.

## Where it comes from

The crate doc states its own provenance: it reimplements the core of
`tools/dlc_port.py::port_x360_dlc` on the Rust pipeline. All the format work is borrowed rather
than re-derived — `mercs2_formats` supplies the STFS reader, the BE FFCS/INDX/ASET/PTHS parsers,
the sges codec, the UCFX block entry table and the patch-WAD assembler; `ucfx_byteswap` supplies
the BE→LE block conversion.

## Usage

From an Xbox 360 DLC RAR:

```
cargo run -p dlc_port -- --x360-rar DLC01.rar --unrar /path/to/UnRAR.exe \
  --vz-wad /path/to/vz.wad --output vz-patch.wad
```

From an STFS container or a raw `DLC01.doh`:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh \
  --vz-wad /path/to/vz.wad --output vz-patch.wad
```

Inspect the block table without converting anything:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh \
  --vz-wad /path/to/vz.wad --list-blocks
```

Convert a slice of the block range while iterating:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh \
  --vz-wad /path/to/vz.wad --start-block 100 --max-blocks 20 -o test.wad
```

Flags: `--x360-rar`, `--x360-stfs` (one is required), `--unrar PATH` / `--bsdtar PATH` (exactly
one is required with `--x360-rar`), `--vz-wad PATH` (required: the
player's PC `vz.wad`, whose `schm` chunks lay out the DLC's compact ECS containers, see
`mercs2_formats::be_to_le::ecs`, and whose texture list decides which pixel-less DLC textures the
base game serves), `-o/--output` (required unless `--list-blocks`),
`--list-blocks`, `--max-blocks N`, `--start-block N`, `-v/--verbose`.

## Notes / gotchas

* **Xbox `block_index` cannot be rebased arithmetically.** The DLC's blocks are not a contiguous
  run in the source index space, so `idx - min(idx)` misroutes — the source records this as
  measured, with 1045 texture rows landing on the wrong block. Instead *every* ASET row (per-block
  rows and the `0xFFFF` "global" ones alike) is routed to whichever converted block actually
  contains its `asset_hash`.
* **Rows owned by no shipped block are dropped**, on purpose. Those are Xbox rows for base-game
  assets that live in the retail WAD; dropping them lets the base WAD resolve the asset instead of
  shadowing it with a dangling patch row.
* **The bootstrap / import-chain injection is intentionally omitted.** Those passes existed to
  graft the DLC contracts into the *vz* master script. The DLC ships its own master script
  (`dlc01`), so it is loaded as a level via `LevelBootstrap.LoadLevel("dlc01", "dlc01")` instead
  of being injected into Venezuela. The binary prints a note saying so on every run.
* The Xbox ASET sub-entry field is normalised on the way in: `(u2 & 0xFFFF_0000) | 0xFFFF`.
* `--verbose` is accepted but not currently read by the conversion path. Per-block diagnostics from
  `convert_block` are suppressed unconditionally (`QUIET` is set before the loop).
* `publish = false` — this crate is excluded from the crates.io release set.
