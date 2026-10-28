# ucfx_byteswap

[![Crates.io](https://img.shields.io/crates/v/ucfx_byteswap.svg)](https://crates.io/crates/ucfx_byteswap)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Converts a decompressed Mercenaries 2 UCFX block from the Xbox 360 (big-endian) WAD format to the PC (little-endian) format — library plus a thin CLI.

## What it is

A big-endian → little-endian converter for one *decompressed* UCFX block at a time. It is not a blind `swap_bytes` sweep: several embedded payloads inside a block change *shape*, not just byte order, so the converter is structure-aware.

Per block it:

1. Reads the BE entry table (`entry_count` u32, then N × 16-byte rows: name/type/size/field_c) and re-emits it LE.
2. Parses each entry's UCFX descriptor table (tag + offset/size/fields) and dispatches on the chunk tag.
3. Converts the bodies:
   - **ECS bodies** (`Layer` 0xE6B81A54, `WorldEntityData` 0x5647C35D, `GuidMap` 0x140E8728): locates the COMP `info`/`schm`/`data` triplets and swaps each field at its *schema-declared* width. Components with no `schm` fall back to a hardcoded handler or a u32 sweep — and the fallback is reported, not silent.
   - **Generic containers** (Texture, Mesh, Animation, Script, …): tag-aware dispatch.
4. Runs the special-case re-encodes (below).
5. Recomputes container sizes, reframes offsets, and rewrites the `CSUM` trailers.

Special cases that resize or rewrite a body rather than swap it:

| Payload | What actually happens |
| --- | --- |
| Havok packfile (`ANIM`, `PHY2`) | Section-aware convert. A wholesale u32 swap scrambles the ASCII `__classnames__` strings, the loader then fails to find the class by name (`STATUS_OBJECT_NAME_NOT_FOUND` → AV). `havok.rs` swaps the header / section headers / classname signatures as u32, leaves the classname strings and `__types__` raw, and converts `__data__` with per-class field widths (u32 pointers/floats/enums, u16 refcounts/indices, u8 QuantizationFormat bytes and compressed bitstreams). |
| Texture `BODY` | GPU-tiled DXT → linear untile, plus a rebuilt 34-byte PC `INFO`. Shrinks the body, so the container is reframed. |
| Wavebank | Codec transcode. Each embedded clip is decoded to interleaved LE PCM16 and written as PC format 0x02: PCM (0x00) is a BE→LE byte swap; XMA (0x01), XMA2 (0x05, Xbox) and MP3 (0x0C, PS3) are decoded via `ffmpeg`. Streamed records carry their `.pws` offset forward. |
| Lua bytecode (`BINN`) | Not a swap. unluac disassemble → flip `.endianness BIG`→`LITTLE` → reassemble. May resize. |
| Mesh vertex declaration | Xbox 12-byte element records → the 8-byte PC `D3DVERTEXELEMENT9` array. |
| Terrainmesh (0x7C569307) / low-res terrain (0x1602815C) | A genuine re-encode: `STRM` vertices widen, indices are de-stripped, the whole data area is rebuilt and reframed. |

`validate.rs` then checks the output: entry-table integrity, `CSUM`, descriptor bounds, float sanity (NaN/Inf in `STRM`/`BNDS`), world-envelope on `BNDS`, `IBUF` index bounds, plus `DEPS`/`FXDICT`/`WATR` payload checks from `mercs2_formats::chunk_validate`.

## Where it comes from

The Xbox 360 disc and the PC disc ship the *same* assets baked for opposite endianness, so the console WAD is the oracle for anything the PC bake dropped. This crate is the Rust port of the Python pipeline that first proved that conversion; the modules live in `mercs2_formats::be_to_le` (re-exported by `lib.rs`), and the module headers name their sources:

- `convert.rs` — port of `tools/ucfx_be_to_le.py`; terrainmesh path is documented in `docs/terrainmesh_reencode_implementation.md`.
- `havok.rs` — port of `tools/ucfx_be_to_le.py::_convert_havok_be_to_le` + `tools/hk_class_layouts.py`. Targets Havok 5.5.0-r1, 32-bit (HK550). The class-layout registry covers the **animation** classes only; physics classes are unregistered, so a `PHY2` collision packfile's `__data__` degenerates to a u32 sweep + embedded-`layoutRules` repair — which is exactly what the Python did.
- `audio.rs` — port of `tools/pws_xbox_to_pc.py` + `ucfx_be_to_le.py::_convert_wavebank_data`. The deterministic ADPCM/IMA paths (`transcode_pws_xbox_to_pc`, which the wavebank path does not call) are byte-for-byte identical to the Python (checked in `audio::tests`).
- `aset.rs` — port of `tools/dlc_port.py` (`_strip_xbox_sub_entry`, `_recompute_aset_sub_entries`) and the final compose in `tools/ffcs_patch_wad.py`.
- `tests/integration_test.rs` — byte-for-byte round-trip against real shipped BE/LE fixture pairs (`anim_ks750`, `phy2_resident2`): convert the BE fixture, assert it equals the retail LE fixture.

The main in-process consumer is the `dlc_port` driver (Xbox DLC → PC WAD), which calls `convert::convert_block` directly instead of spawning the CLI.

## Usage

### Library

```rust
use ucfx_byteswap::{convert, validate};

let be_block: Vec<u8> = std::fs::read("block_be.bin")?;

// dry_run = false, no coverage report, no Lua tools (a block holding BINN errors)
let le_block = convert::convert_block(&be_block, false, None, None)?;

for err in validate::validate_converted_block(&le_block) {
    eprintln!("{:?}", err);
}
std::fs::write("block_le.bin", &le_block)?;
```

With a schema coverage report (printed to stderr):

```rust
use ucfx_byteswap::{convert, report::SchemaCoverageReport};

let mut rpt = SchemaCoverageReport::default();
let le_block = convert::convert_block(&be_block, false, Some(&mut rpt), None)?;
rpt.print_report();
```

ASET sub-entry recompute:

```rust
use ucfx_byteswap::aset::{AsetEntry, recompute_block_aset_subs};

let mut entries = vec![
    AsetEntry { asset_hash: 0x1234_5678, u32_2: 0, primary: false, in_base: true },
];
let counts = recompute_block_aset_subs(&le_block, &mut entries);
println!("{} preserved, {} resolved, {} unresolved",
         counts.preserved, counts.resolved, counts.unresolved);
```

Batch callers should set `convert::QUIET` to silence the per-entry diagnostics:

```rust
ucfx_byteswap::convert::QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
```

### CLI

```bash
# convert one decompressed BE block (--java/--unluac are required when it holds Lua bytecode)
ucfx_byteswap block_be.bin --output block_le.bin --java /path/to/java --unluac /path/to/unluac.jar

# convert one decompressed BE block that holds no Lua bytecode
ucfx_byteswap block_be.bin --output block_le.bin

# pipe mode
cat block_be.bin | ucfx_byteswap --stdin --stdout > block_le.bin

# parse + report, write nothing
ucfx_byteswap block_be.bin --dry-run

# validate an already-LE block (stage-2 / retail blobs); no conversion
ucfx_byteswap block_le.bin --validate-only

# schema field coverage report (stderr)
ucfx_byteswap block_be.bin --output out.bin --report-schema-coverage

# validation errors are fatal (exit 2, nothing written)
ucfx_byteswap block_be.bin --output out.bin --strict

# skip the post-conversion validation pass
ucfx_byteswap block_be.bin --output out.bin --no-validate

# untile a raw tiled Xbox DXT BODY into a PC-linear mip chain
ucfx_byteswap tiled_body.bin --untile-tex \
    --tex-w 512 --tex-h 512 --tex-fourcc DXT5 --tex-mips 10 --output linear.bin

# ASET sub-entry recompute (stdin -> stdout, binary protocol; see below)
ucfx_byteswap --stdin --aset-recompute < aset_protocol.bin > updated_u32_2.bin
```

## Modules

| Module | Owns |
| --- | --- |
| `convert` | Block-level BE→LE conversion. `convert_block` is the public entry point; also exposes `untile_tiled_dxt_body` and the `QUIET` diagnostics flag. |
| `validate` | Post-conversion checks on an LE block; returns `Vec<ValidationError>`. |
| `havok` | Havok 5.5.0-r1 (HK550) packfile conversion: `convert_havok_be_to_le`, `convert_phy2_be_to_le`, `validate_phy2`. |
| `audio` | Console wavebank → PC PCM16 wavebank: `convert_wavebank_data`, `normalize_embedded_wavebank_clip`, `transcode_xma_to_pcm16`, `transcode_xma2_raw_to_pcm16`, `transcode_mp3_raw_to_pcm16`, `find_ffmpeg`; plus the Xbox-ADPCM → PC IMA `transcode_pws_xbox_to_pc`. |
| `lua` | Lua 5.1 `BINN` bytecode BE→LE via the unluac disassemble/reassemble round-trip: `convert_binn_be_to_le`, run with the caller's `LuaTools` (java + unluac paths). |
| `aset` | ASET `packed_block_ref` sub-entry computation: `recompute_block_aset_subs`, `strip_xbox_sub_entry`, `AsetEntry`, `RecomputeCounts`. |
| `report` | `SchemaCoverageReport` — per-component schema field coverage, unknown type codes, fallback/`needs-investigation` tags. |

## Notes / gotchas

- **Input must already be decompressed.** The crate takes raw UCFX block bytes; WAD-level decompression is somebody else's job.
- **External toolchain.** `lua.rs` runs the `java` executable and unluac jar named by the caller's `LuaTools` (the CLI's `--java` / `--unluac`); `convert_block` errors on a block holding `BINN` when it gets no `LuaTools`. `audio.rs` needs `ffmpeg` for the XMA / XMA2 / MP3 clips only (`find_ffmpeg`: `MERCS2_FFMPEG`, then a `tools/ffmpeg/bin/` bundle found walking up from the exe or the cwd, then `PATH`; errors if none resolves); the PCM and ADPCM/IMA paths are pure Rust.
- **Decompile-to-source is deliberately not used for Lua.** unluac emits Lua 5.2+ `goto`/labels on complex control flow, which the 5.1 compiler rejects. The bytecode-level disassemble/assemble avoids that.
- **ASET field order is inverted between platforms.** PC/LE packs `{ block_index : hi16, sub : lo16 }`; the Xbox/BE source is `{ sub : hi16, block : lo16 }`. The Python pipeline mislabels these, which is the root of the recurring sub-field bugs — `aset.rs` keeps the distinction explicit.
- **`aset.rs` fixes a Python bug.** In the Python the faithful BE sub is not preserved — primaries got the physical index, then body-less stubs were forced to `0xFFFF`, clobbering real sub-offsets. `recompute_block_aset_subs` keeps primaries and in-base hashes at `0xFFFF` (resolve by hash), keeps a DLC-new stub's BE sub, and gives a DLC-new entry with a real body its physical sub-entry index.
- **The coverage report is the honest-signal channel.** `--report-schema-coverage` lists unknown schema type codes, `schm` parse failures, components with no schema, non-ECS bodies that hit the catch-all u32 sweep, and registered-but-unvalidated tags flagged "REQUIRES DEEPER INVESTIGATION". A body that took a fallback path may well be converted *wrong*; it is surfaced loudly rather than swapped silently.
- **`--validate-only`** cannot be combined with `--dry-run`, `--report-schema-coverage`, or `--stdout`.
- **`--aset-recompute` protocol** (stdin): `[u32 n_entries]`, then n × `[u32 asset_hash][u32 u32_2][u8 primary][u8 in_base]` (10 bytes/row), then the decompressed LE block bytes. Output: n × `[u32 updated_u32_2]`.

## Dependencies

- `mercs2_formats` — chunk tags, type hashes, schema parsing, Mercs2 CRC32, texture size math.
- `clap 4.5` — CLI parsing.
- Runtime (external, not cargo deps): `ffmpeg` (XMA / XMA2 / MP3 wavebank clips only), a JRE + unluac (Lua `BINN` only).

## License

MIT. See [LICENSE](LICENSE).

## Related

- `mercs2_formats` — type/tag/schema definitions this crate builds on.
- `dlc_port` — Xbox DLC → PC WAD pipeline; the in-process consumer of `convert_block`.
- `wad_simulator` — WAD container tooling.
