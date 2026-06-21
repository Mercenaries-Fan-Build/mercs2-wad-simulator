# Changelog

All notable changes to wad_simulator are documented in this file.

## [1.0.0] - 2025-06-18

### Initial Release

**wad_simulator v1.0.0** is the first stable release of the engine-accurate WAD consumption simulator for Mercenaries 2. This release provides production-ready tools for WAD validation, asset consumption simulation, and diagnostic analysis for the modding community.

#### Core Features

- **ASET Validation & OOB Detection**
  - Validates Asset Set entries across all WAD sections
  - Detects out-of-bounds references that would cause heap violations
  - Detailed per-entry diagnostics with garbage memory inspection
  - Configurable OOB-only reporting mode

- **Full Asset Consumption Path**
  - End-to-end simulation of game engine asset loading
  - Support for all major asset types: Models, Textures, Animations, Scripts, Layers, Terrains, Materials, Action Tables, FX Dictionaries, Watermaps
  - Schema-driven ECS validation with NaN/Inf detection
  - Structural integrity checking across all asset families

- **WAD Overlay Simulation**
  - Patch WAD overlay behavior (last-opened-file-wins)
  - Virtual disk resolution for base + patch WAD pairs
  - ASET shadowing and csum mismatch detection

- **Parallel Block Processing**
  - Multi-threaded SGES block decompression
  - Parallel UCFX container parsing
  - Configurable thread pool (auto-detect or manual)
  - Progressive prefetch for memory-efficient processing

- **Comprehensive Diagnostics**
  - Per-type asset statistics
  - Cross-reference resolution with rainbow table support
  - Structural violation categorization (vertex bounds, index buffer mismatches, type mismatches)
  - Advisory heuristic checks for decl-stride and position interpretation
  - JSON-exportable simulation reports

- **Audio & Streaming Validation**
  - Wavebank and soundbank consumption
  - Streaming audio .pws file mapping via dlc_audio_manifest.json
  - Audio directory structure validation

- **Cross-WAD Asset Resolution**
  - Auxiliary base WAD discovery and loading (English, shell, Loading, etc.)
  - Asset reference resolution across sibling WADs
  - Prevention of false-positive unresolved-reference reports

#### CLI Capabilities

- Modular flag system for precise control:
  - `--oob-only`: Surface only out-of-bounds violations
  - `--skip-assets`: ASET validation without asset consumption
  - `--skip-audio`: Exclude audio validation
  - `--audio-only`: Audio and PWS validation only
  - `--asset-limit`: Consume up to N non-audio assets
  - `--progress-interval`: Control logging verbosity
  - `--jobs`: Override thread pool size
  - `--json-output`: Export diagnostic reports
  - `--base-wad-dir`: Enable auxiliary WAD resolution

#### Documentation

- Comprehensive README with feature overview, fundamental concepts, and workflow integration
- Detailed CLI usage examples covering all major scenarios
- Module-level documentation in all source files
- Public API documentation for all types and functions

#### Technical Foundation

- Parallel asset consumption with Rayon
- Compressed SGES block handling via mercs2_formats
- UCFX container parsing and type dispatch
- Safe slice abstractions for memory safety
- Colored terminal output for diagnostic clarity
- JSON serialization for report automation

#### Known Limitations (by design)

- Schema-driven field interpretation relies on heuristic offset/stride guessing for some asset types (STRM vertex decls, HIER node bounds, PRMG info bbox, flgs placement records). These fire as "advisory" counters, not verdict-affecting violations.
- Audio validation requires external manifest and directory support (not self-contained in WAD)
- Cross-WAD resolution disabled by default; use `--base-wad-dir` to enable

### Initial Commit

This release marks the culmination of the wad_simulator project from its genesis through production-ready status. The codebase includes:

- 18 source modules covering all asset types and simulation stages
- ~5000 lines of production Rust code
- Full integration with mercs2_formats library
- Tested on representative Mercenaries 2 patch and base WADs

### Repository

Source code and updates: https://github.com/aussieurban/mercenaries-game

### License

MIT License. See LICENSE.md for full text.

---

**Note**: This is the initial 1.0.0 release. Future releases will follow semantic versioning with changes documented in this file.
