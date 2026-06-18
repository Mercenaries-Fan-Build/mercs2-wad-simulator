# mercs2-wad-simulator

A Rust workspace for Mercenaries 2 WAD analysis, asset extraction, and Xbox-to-PC conversion.

## Binaries

### `wad_simulator`
Engine-accurate consumption simulator for WAD archives. Validates WAD structure, detects out-of-bounds ASET entries, and runs every asset through a consumption pipeline (meshes, textures, sounds, scripts, etc.). Useful for validating WAD conversions and catching data corruption before runtime.

```bash
wad_simulator --wad output/data/vz-patch.wad --rainbow-table tools/rainbow_table.json
```

### `ucfx_byteswap`
Converts Xbox 360 (big-endian) UCFX blocks to PC (little-endian) format. Used in the DLC Xbox-to-PC porting pipeline. Handles entire asset blocks with full struct-aware byte-swapping.

```bash
ucfx_byteswap /path/to/xbox_block.bin --output /path/to/pc_block.bin
```

### `loadprobe`
Analyzes `pmc_blackbox.log` from game runs to quantify world-load progress and classify end-state (crashed, hung, or fully loaded). Scores against a 21-phase milestone ladder and surfaces diagnostics.

```bash
loadprobe /path/to/pmc_blackbox.log
```

### `dlc_port`
Incomplete Rust reimplementation of the Python DLC porter. Converts Xbox 360 DLC (RAR/STFS) to PC `vz-patch.wad`. Currently a work-in-progress; the Python version is still authoritative.

```bash
dlc_port --x360-rar Mercenaries.2.DLC.rar --source-wad vz.wad --output vz-patch.wad
```

## Building

### Prerequisites
- Rust 1.70+ ([rustup](https://rustup.rs/))

### Build all binaries
```bash
cargo build --release
```

Binaries land at `target/release/`:
- `wad_simulator` / `wad_simulator.exe`
- `ucfx_byteswap` / `ucfx_byteswap.exe`
- `loadprobe` / `loadprobe.exe`
- `dlc_port` / `dlc_port.exe`

### Build a single binary
```bash
cargo build --release -p ucfx_byteswap
```

## Crates

- **`mercs2_formats`** — Shared file-format parsing library (WAD, FFCS, ASET, PTHS, UCFX, etc.). Used by all other crates.
- **`wad_simulator`** — WAD consumption simulator binary.
- **`ucfx_byteswap`** — Xbox BE→PC LE converter (binary + library).
- **`dlc_port`** — DLC porter binary (work-in-progress).
- **`loadprobe`** — Log analyzer binary.

## See also

- [mercs2-pmc-blackbox](https://github.com/austinkregel/mercs2-pmc-blackbox) — Game startup DLL (SecuROM spoof, ASI loader).
- [mercs2-crack-game](https://github.com/austinkregel/mercs2-crack-game) — EXE patcher (applies cracks and injects pmc_bb.dll).
- [mercenaries-game](https://github.com/austinkregel/mercenaries-game) — Full reverse-engineering toolkit.
