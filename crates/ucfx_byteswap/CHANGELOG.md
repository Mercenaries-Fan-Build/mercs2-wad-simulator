# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-06-18

### Added
- **Initial stable release** with 90%+ test coverage
- Core library API: `convert::convert_block()` for in-process block conversion
- CLI binary with comprehensive options (stdin/stdout, dry-run, validation, schema reporting)
- Full schema-driven ECS component conversion (Layer, WorldEntity, GuidMap)
- Tag-aware generic container handling (Texture, Mesh, Animation, Script, Audio, etc.)
- **Embedded type conversions**:
  - Havok 5.5 packfile header repairs (layoutRules byte restoration)
  - Texture GPU-tiled DXT untiling (tiled → linear with size reframing)
  - Wavebank audio codec transcoding (Xbox-ADPCM/XMA → PC IMA via ffmpeg)
  - Mesh vertex element format translation (Xbox 12-byte → PC D3DVERTEXELEMENT9)
  - Lua bytecode conversion (unluac round-trip: disassemble → flip-endianness → reassemble)
  - Terrainmesh re-encoding (vertex expansion + index destrip)
- Comprehensive validation: CSUM checksums, float sanity (NaN/Inf), index bounds, world envelope
- Optional schema coverage reporting per ECS component
- ASET sub-entry recomputation (ported from Python dlc_port)
- Integration test suite with real fixture files (anim_ks750, phy2_resident2)
- Unit tests for all public modules with comprehensive branch coverage
- Full API documentation with examples
- README with library and CLI usage examples
- License (MIT)

### Module Documentation
- `convert`: Block-level conversion (main API entry point)
- `aset`: ASET packed_block_ref sub-entry computation
- `audio`: Xbox/XMA audio → PC IMA ADPCM transcoding
- `havok`: Havok packfile header repairs
- `lua`: Lua bytecode BE→LE conversion
- `validate`: Post-conversion block validation
- `report`: Schema field coverage tracking

### Dependencies
- `mercs2_formats 0.1`: Type hashes, schemas, CRC32, chunk tags
- `clap 4.5`: CLI argument parsing

### Known Limitations
- XMA transcoding requires ffmpeg on PATH
- Dry-run mode does not write output files
- Validation does not catch all semantic errors (structural checks only)

## [0.1.0] - (Not released)

Initial development version with basic byte-swap functionality.
