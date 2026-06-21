# ucfx_byteswap

[![Crates.io](https://img.shields.io/crates/v/ucfx_byteswap.svg)](https://crates.io/crates/ucfx_byteswap)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust 1.70+](https://img.shields.io/badge/Rust-1.70+-orange.svg)](https://www.rust-lang.org)

Xbox 360 big-endian → PC little-endian UCFX block converter. Converts decompressed container blocks from Mercenaries 2 Xbox 360 (BE) WAD format to PC (LE) format, including embedded format conversions (Havok, texture untiling, audio transcoding, Lua bytecode, mesh vertex format translation, and terrainmesh re-encoding).

## Features

- **Block conversion**: Entry table + descriptor tables + data area (BE → LE)
- **ECS components**: Schema-driven field swaps for Layer/WorldEntity/GuidMap
- **Generic containers**: Tag-aware dispatch for Texture, Mesh, Animation, Script, etc.
- **Embedded conversions**:
  - Havok 5.5 packfile headers (layoutRules restoration)
  - Texture untiling (GPU-tiled DXT → linear)
  - Wavebank audio transcode (Xbox-ADPCM/XMA → PC IMA)
  - Mesh vertex format translation (Xbox 12-byte → PC D3DVERTEXELEMENT9)
  - Lua bytecode (unluac disassemble → flip-endianness → reassemble)
  - Terrainmesh re-encoding (vertex expansion + index destrip)
- **Validation**: CSUM checksums, float sanity (NaN/Inf), index bounds, world envelope
- **Schema coverage reporting**: Optional per-component stats
- **Dry-run mode**: Parse and report without writing

## Library API

### Basic conversion

```rust
use ucfx_byteswap::convert;

let be_block: Vec<u8> = /* read Xbox 360 BE UCFX bytes */;
let le_block = convert::convert_block(&be_block, false, None)?;
```

### With schema coverage report

```rust
use ucfx_byteswap::convert;
use ucfx_byteswap::report::SchemaCoverageReport;

let mut report = SchemaCoverageReport::default();
let le_block = convert::convert_block(&be_block, false, Some(&mut report))?;
println!("{}", report);
```

### Validation

```rust
use ucfx_byteswap::validate;

let errors = validate::validate_converted_block(&le_block);
if errors.is_empty() {
    println!("Valid!");
} else {
    for err in errors {
        eprintln!("{}", err);
    }
}
```

### ASET sub-entry recomputation

```rust
use ucfx_byteswap::aset::{AsetEntry, recompute_block_aset_subs};

let mut entries = vec![
    AsetEntry { asset_hash: 0x1234, u32_2: 0, primary: false, in_base: true },
];
recompute_block_aset_subs(&block_bytes, &mut entries);
```

## CLI Usage

Convert a single block:

```bash
ucfx_byteswap input_be.bin --output output_le.bin
```

Stdin/stdout:

```bash
cat input_be.bin | ucfx_byteswap --stdin --stdout > output_le.bin
```

Dry-run (parse and report without writing):

```bash
ucfx_byteswap input_be.bin --dry-run
```

Validation only (existing LE block):

```bash
ucfx_byteswap my_le_block.bin --validate-only
```

Schema coverage report:

```bash
ucfx_byteswap input_be.bin --output out.bin --report-schema-coverage
```

Strict mode (validation errors are fatal):

```bash
ucfx_byteswap input_be.bin --output out.bin --strict
```

Skip validation:

```bash
ucfx_byteswap input_be.bin --output out.bin --no-validate
```

ASET sub-entry recomputation:

```bash
cat aset_protocol | ucfx_byteswap --aset-recompute > updated_u32_2.bin
```

See `ucfx_byteswap --help` for all options.

## Design

### Hybrid lib + binary

The crate is both a **library** (exposing `convert::convert_block` for in-process use by the `dlc_port` driver) and a **CLI binary** (for single-block testing and batch scripts). The binary is thin (main.rs, ~200 lines) and re-uses all library logic.

### Conversion flow

1. **Parse BE entry table**: name/type/size/field_c hashes (4 fields × u32 per entry)
2. **Parse descriptors**: tag + offset/size/fields per UCFX container
3. **Convert containers**:
   - ECS (Layer/WorldEntity/GuidMap): identify COMP triplets (info/schm/data) and apply schema-driven swaps
   - Generic: tag-aware dispatch (Texture untiling, wavebank transcode, etc.)
4. **Recompute sizes**: CSUM trailers + entry table offsets
5. **Write LE output**: entry table + containers + CSUM trailers

### Special-case conversions

- **Texture**: GPU-tiled DXT untiling (shrinks BODY, reframes container)
- **Wavebank**: Audio codec transcode (Xbox-ADPCM/XMA → PC IMA, may resize)
- **Lua bytecode (BINN)**: Unluac disassemble → flip endianness → reassemble (may resize)
- **Havok**: Fix embedded layoutRules bytes (4 u8 fields reversed by generic u32 swap)
- **Mesh vertex decl**: Translate Xbox 12-byte element format → PC 8-byte D3DVERTEXELEMENT9
- **Terrainmesh**: Full re-encode (index destrip, vertex stride expansion)

## Dependencies

- `mercs2_formats ^0.1`: Schema, type hashes, CRC32, chunk tags, texture utilities
- `clap 4.5`: CLI argument parsing
- (Optional runtime) `ffmpeg`: For XMA audio transcoding

## License

MIT License. See [LICENSE](LICENSE) for details.

## Related

- **mercs2_formats**: Type definitions, schema parsing, CRC32
- **dlc_port**: DLC porting pipeline (uses ucfx_byteswap as in-process converter)
- **wad_simulator**: WAD container tools
