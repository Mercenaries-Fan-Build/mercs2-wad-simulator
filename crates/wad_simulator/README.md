# wad_simulator

Engine-accurate WAD consumption simulator for Mercenaries 2. This tool validates and simulates how the Mercenaries 2 game engine loads and consumes WAD (World Asset Database) files, providing diagnostic analysis for modders and developers.

## Features

- **ASET Validation & OOB Detection**: Identify and analyze out-of-bounds asset entries in WAD ASET (Asset Set) sections that would cause heap violations during engine load
- **Full Asset Consumption Path**: Simulate the complete engine asset loading pipeline, validating all asset types and their cross-references
- **WAD Overlay Simulation**: Model patch WAD overlay behavior where patch entries override base WAD entries (last-opened-file-wins semantics)
- **Parallel Block Processing**: Leverages multi-threaded decompression and UCFX container parsing for fast analysis
- **Detailed Diagnostic Reports**: JSON-exportable reports with type statistics, structural violations, and cross-reference resolution
- **Cross-WAD Resolution**: Resolve asset references across auxiliary base WADs (English, shell, Loading, etc.) to prevent false-positive unresolved-reference reports
- **Audio & Streaming Validation**: Validate wavebanks, soundbanks, and streaming audio .pws file mappings
- **ECS Corruption Detection**: Catch NaN/Inf and out-of-bounds issues in schema-driven float fields

## WAD Simulation Fundamentals

Mercenaries 2 game WADs are compound asset containers organized as:

1. **FFCS Archive**: File index + compression directory
2. **SGES Blocks**: Compressed asset blocks
3. **UCFX Containers**: Asset format wrappers with sub-entries
4. **Assets**: Type-specific payloads (Models, Textures, Animations, etc.)

The simulator:
- Decompresses SGES blocks in parallel
- Parses UCFX container structure
- Dispatches to type-specific consumers
- Collects structural and data-integrity issues
- Builds cross-reference graphs for resolution

## ASET Validation and OOB Checking

ASET (Asset Set) sections are sparse arrays of game asset metadata. Entries include:

- `asset_hash`: Unique asset identifier
- `block_index`: Which SGES block contains the asset
- `sub_entry`: Offset within block's entry table
- `type_id`: Asset type classifier

**OOB (Out-of-Bounds) Detection**: When an ASET entry's sub_entry offset exceeds the actual entry count in the decompressed block, it references garbage memory. The engine will try to load this garbage as an asset, causing a heap crash or data corruption.

Run with `--oob-only` to surface only OOB violations.

## CLI Usage Examples

### Basic WAD Analysis (patch with base overlay)

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad
```

### ASET-Only Validation (no asset consumption)

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad \
  --skip-assets
```

### Out-of-Bounds Entry Report

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad \
  --oob-only
```

### Audio & Streaming Validation

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --audios-dir "Data/Audios" \
  --audio-manifest dlc_audio_manifest.json \
  --json-output report.json
```

### Cross-WAD Asset Resolution

Use `--base-wad-dir` to scan a game `data/` directory for auxiliary WADs (English, shell, Loading), enabling asset references into sibling WADs to resolve correctly:

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad \
  --base-wad-dir output/data
```

### Parallel Prefetch with Custom Thread Pool

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --jobs 8 \
  --json-output report.json
```

### Asset Limit & Progress Logging

Consume up to 500 non-audio assets with progress every 50 steps:

```bash
wad_simulator \
  --wad output/data/vz-patch.wad \
  --asset-limit 500 \
  --progress-interval 50
```

## Building from Source

### Prerequisites

- Rust 1.70+
- Cargo

### Build

```bash
cargo build --release
```

### Run Tests

Tests are managed separately in Phase 3 of the release cycle.

## Integration with Modding Workflow

The simulator is designed to be invoked in modding pipelines:

1. **Post-Cook Analysis**: After packaging a WAD patch, run the simulator to validate before distribution
2. **CI/CD Integration**: Export JSON reports (`--json-output`) for automated asset quality gates
3. **Debug Iteration**: Use `--oob-only`, `--audio-only`, and asset/block limits to rapidly iterate on specific subsystems
4. **Cross-Reference Auditing**: Leverage `--base-wad-dir` to ensure all asset refs remain valid in the final distribution

Example pipeline:

```bash
# Bake the patch WAD
./cook_wad.sh output/data/vz-patch.wad

# Validate before shipping
wad_simulator \
  --wad output/data/vz-patch.wad \
  --base-wad output/data/vz.wad \
  --base-wad-dir output/data \
  --audio-manifest dlc_audio_manifest.json \
  --json-output build/validation.json

# Fail CI if structural issues found
if grep -q '"structural_violations": [1-9]' build/validation.json; then
  echo "WAD validation failed: structural violations detected"
  exit 1
fi
```

## Common Options

| Option | Purpose |
|--------|---------|
| `--wad` | Primary WAD file (patch or single-WAD analysis) |
| `--base-wad` | Base game WAD for overlay simulation |
| `--base-wad-dir` | Directory of auxiliary base WADs for cross-ref resolution |
| `--oob-only` | Only report out-of-bounds ASET entries |
| `--skip-assets` | ASET-only mode (skip full asset consumption) |
| `--skip-audio` | Skip audio/wavebank validation |
| `--audio-only` | Only validate audio + PWS (skip mesh/texture/layer scan) |
| `--asset-limit` | Max non-audio assets to consume (0 = all) |
| `--jobs` | Parallel worker threads for block prefetch (0 = auto) |
| `--json-output` | Export simulation report as JSON |
| `--audio-manifest` | Path to dlc_audio_manifest.json for .pws mapping |
| `--rainbow-table` | Path to rainbow_table.json for annotating unresolved hashes |
| `--progress-interval` | Log progress every N steps (default 100) |

## License

MIT License. See [LICENSE.md](LICENSE.md) for details.
