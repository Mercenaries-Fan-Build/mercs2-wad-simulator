# anim_patch

Exports a base-game player animation block out of `vz.wad` and repacks it into a `vz-patch.wad`
overlay, for controlled Mercenaries 2 animation modding.

## What it is

A command-line tool (single `[[bin]]`, no library surface). It:

1. Opens the game's `vz.wad` (passed with `--wad`, or discovered from the Windows registry key
   `HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames` → `Install Dir` + `data\vz.wad`).
2. Selects one animation block. Selector precedence is `--block-index` > `--clip` > `--anim-name`
   (default `characternameanimgroup_mattias`). A block only qualifies if it owns an ASET entry with
   `type_id == TYPE_ID_ANIMATION` (16).
3. Decompresses that block (`sges::decompress_block`) and parses it as an animgroup
   (`animgroup::parse_animgroup`) to enumerate its clips (`name_hash`, `havok_offset`).
4. Optionally perturbs the decompressed bytes in place (`--freeze`).
5. Recompresses with `compress_sges`, **verifies the recompressed data re-decompresses byte-exact**,
   wraps it in a `PatchBlock` (original `path_string` + *all* of the block's ASET entries + decompressed
   page count `(len + 0x7FFF) / 0x8000`), and emits a patch WAD via `patch_wad::build_patch_wad_multi`
   with the source archive's `CSUM` chunk value/meta and `FFCS_CERT_BLOB`.

Modes:

| flag | effect |
| --- | --- |
| `--roundtrip` | Identity pass: recompress the block **unmodified**, assert a byte-exact round-trip, then pack. Run this first — nothing else is trusted until it passes. |
| `--freeze` | Zero every clip's dynamic quantized wavelet coefficient stream (`quantData`), producing a static / near-bind pose, then pack. |
| `--list` | Print every clip in the selected block (`0xHASH @0xOFFSET`) and exit. |
| `--dump <path>` | Write the selected block's decompressed bytes to `<path>` and exit. |

`--roundtrip` and `--freeze` are mutually exclusive, and one of them (or `--list`/`--dump`) is required.
Output goes to `--out` (default `crates/anim_patch/out/vz-patch.wad`); the game's real
`data/vz-patch.wad` is only written when `--deploy` is passed.

## Where it comes from

* The WAD read/write path is not new: it reuses the shipped modding pipeline verbatim
  (`mercs2_formats::{ffcs, sges, animgroup, patch_wad}`) and follows the same block-build pattern as
  the cube_mod proof of concept (`docs/cube_mod_poc.md`) — source the target block via its ASET entry,
  copy its exact `path_string` and *all* of its ASET entries, `compress_sges`, wrap in a `PatchBlock`,
  `build_patch_wad_multi`.
* The only genuinely new logic is (a) locating the human-rig animgroup block that owns the player-idle
  clip `0x24F8C8E6`, and (b) the in-place clip perturbation in `perturb.rs`.
* `perturb.rs` works against the on-disk `hkaWaveletSkeletalAnimation` layout (Havok 5.5.0-r1, 32-bit LE).
  Its field offsets (`W_OFF_ANIM_TYPE` 8, `W_OFF_DURATION` 12, `W_OFF_NUM_TT` 16, `W_OFF_BLOCK_SIZE` 40,
  `W_OFF_QUANT_DATA_IDX` 80, `W_OFF_QUANT_DATA_SIZE` 84, struct size 96) intentionally mirror the private
  `W_OFF_*` constants in `mercs2_formats::anim`, and the struct finder mirrors `anim::find_wavelet_struct`.

## Usage

Report which block the player rig lives in, and list its clips:

```sh
cargo run -p anim_patch -- --list
```

Identity pass — prove a block survives decompress → recompress → pack byte-exact, into the default
scratch output:

```sh
cargo run -p anim_patch -- --roundtrip
```

Freeze every clip in a specific character's animgroup and write it into the live game:

```sh
cargo run -p anim_patch -- --anim-name characternameanimgroup_mattias --freeze --deploy
```

Explicit WAD, explicit block index (as printed by `--list`), dump the decompressed bytes for inspection:

```sh
cargo run -p anim_patch -- --wad "D:/Games/Mercs2/data/vz.wad" --block-index 1234 --dump out/block.bin
```

Select by clip hash instead of by name (legacy):

```sh
cargo run -p anim_patch -- --clip 0x24F8C8E6 --list
```

## Notes / gotchas

* **`--clip` is not a character selector.** Shared clip hashes such as the player-idle `0x24F8C8E6`
  exist in *every* character's rig, so `--clip` grabs whichever copy appears first in the archive
  (jennifer's, in practice). `--anim-name` is the intended selector because the player (Mattias) loads
  a *different* block than the one the shared clip hash first lands in.
* The `--anim-name` default is scoped to `characternameanimgroup_mattias` rather than a bare `mattias`
  because `mattias` alone also matches the chopper / briefing animgroups. An ambiguous `--anim-name`
  is an error, and the tool prints the matching candidate blocks so you can narrow it.
* `--freeze` bounds each clip's wavelet search to `[havok_offset, next clip's havok_offset)` so a miss
  can never spill into and corrupt an adjacent clip. Clips with no locatable wavelet `quantData`
  (interleaved/delta/spline, or `quantDataSize == 0`) are skipped; if *no* clip could be frozen the run
  fails rather than packing an unchanged block.
* Freezing zeroes only the dynamic per-frame coefficient blob. The header, static/dynamic masks, block
  index, static DOFs, and quantization descriptors are left intact — that is why the result is a static
  / near-bind pose rather than garbage.
* Every pack path re-decompresses the block it just compressed and asserts equality before writing;
  `sges` is lossless, so a mismatch means the pipeline is broken, not the data.
* Registry discovery of `vz.wad` / `vz-patch.wad` is Windows-only; elsewhere `--wad` is mandatory and
  `--deploy` requires an explicit `--out`.
