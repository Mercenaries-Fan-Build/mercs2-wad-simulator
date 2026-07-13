# dlc_port

All-Rust porter that turns an Xbox 360 Mercenaries 2 DLC package (RAR or STFS/DOH) into a PC `vz-patch.wad`.

## What it is

A single binary (`dlc_port`). It takes the Xbox 360 DLC container, walks its big-endian FFCS
index, converts every block to PC little-endian form, and re-assembles the result as an FFCS
patch WAD the PC game can load as an overlay.

The pipeline, in the order `src/main.rs` runs it:

1. **Load DOH bytes** — either `extract_stfs_from_rar` (RAR → STFS → `DLC01.doh`) or
   `load_stfs_or_doh` (an STFS container, or a raw `.doh` passed straight through).
2. **Parse the BE FFCS header** — `parse_be_ffcs` yields the chunk rows; `INDX`, `ASET`, `PTHS`
   and `CSUM` are pulled out by tag. `--list-blocks` stops here and prints page count, packed
   field and path per block.
3. **Per-block convert** — each block is either `segs`-decompressed (`decompress_be_sges`) or
   passed through when it is a bare `XFCU` entry table (trailing zeros trimmed, length rounded up
   to 4). The decompressed bytes go through `ucfx_byteswap::convert::convert_block` (BE→LE), are
   recompressed with `compress_sges`, and the result is immediately decompressed again and
   compared — a round-trip mismatch is a hard error. Blocks that fail to decompress or convert are
   counted as skipped, not fatal.
4. **Rebuild `packed_field`** — `(xbox_tier << 24) | ceil(size / 0x8000)`, keeping the source
   block's tier byte but recomputing the page count for the converted (LE) payload.
5. **Route ASET by content** — see the gotcha below. Each row's `type_id` is refined from the
   owning entry's `type_hash` via `type_id_for_type_hash` rather than trusting the Xbox row.
6. **Synthesise missing ASET rows** — any `SCRIPT_TYPE_HASH` or `STRINGDB_TYPE_HASH` entry in a
   block's table that has no ASET row gets one (`u1 = 0xFFFFFFFF`, `u2 = 0xFFFF`).
7. **Assemble** — `build_patch_wad_multi` with the source `CSUM` value/meta and `FFCS_CERT_BLOB`.

## Where it comes from

The crate doc states its own provenance: it reimplements the core of
`tools/dlc_port.py::port_x360_dlc` on the Rust pipeline. All the format work is borrowed rather
than re-derived — `mercs2_formats` supplies the STFS reader, the BE FFCS/INDX/ASET/PTHS parsers,
the sges codec, the UCFX block entry table and the patch-WAD assembler; `ucfx_byteswap` supplies
the BE→LE block conversion.

## Usage

From an Xbox 360 DLC RAR:

```
cargo run -p dlc_port -- --x360-rar DLC01.rar --output vz-patch.wad
```

From an STFS container or a raw `DLC01.doh`:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh --output vz-patch.wad
```

Inspect the block table without converting anything:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh --list-blocks
```

Convert a slice of the block range while iterating:

```
cargo run -p dlc_port -- --x360-stfs DLC01.doh --start-block 100 --max-blocks 20 -o test.wad
```

Flags: `--x360-rar`, `--x360-stfs` (one is required), `-o/--output` (required unless
`--list-blocks`), `--list-blocks`, `--max-blocks N`, `--start-block N`, `-v/--verbose`.

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
