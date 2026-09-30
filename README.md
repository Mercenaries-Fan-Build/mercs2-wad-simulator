# mercs2-wad-simulator

A Rust workspace for Mercenaries 2 WAD analysis, asset extraction, and Xbox-to-PC conversion.

## Binaries

### `qm` — the Quartermaster
Lints and builds **Shipments**: the mod package format (a `manifest.yaml` plus a `src/` directory).
This is the tool a *mod author* uses; everything else here is for analysing and converting game data.

```bash
qm lint  ./my-shipment                  # check it — no game install needed
qm build ./my-shipment --out build      # produce the overlay WAD
qm link  ./mod-a ./mod-b --out deploy   # compose several mods' Lua into one WAD
qm rules                                # every check, and the ones not implemented yet
```

`qm lint` is hermetic — manifest text plus the Shipment directory, no game and no network — which is
what lets it run in a public CI job. `qm build` and `qm link` need the retail WADs.

Gated on the **exit code**: `0` clean, `1` findings at error or above, `2` could not run at all.

Start from the [Shipment template](https://github.com/Mercenaries-Fan-Build/mercs2-shipment-template),
which has a runnable example for every contribution kind.

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
- `qm` / `qm.exe`
- `wad_simulator` / `wad_simulator.exe`
- `ucfx_byteswap` / `ucfx_byteswap.exe`
- `loadprobe` / `loadprobe.exe`
- `dlc_port` / `dlc_port.exe`

### Build a single binary
```bash
cargo build --release -p ucfx_byteswap
```

## Testing

Tests run across the whole workspace with [cargo-nextest](https://nexte.st/):

```bash
cargo install cargo-nextest --locked   # once
cargo nextest run --workspace          # all crates
cargo test --workspace --doc           # doctests (nextest does not run these)
```

### Game-gated tests

Tests that read the retail game are **not** part of the run above. The game cannot be
committed, so they are built only by each crate's `retail` cargo feature, and one
command runs all of them:

```bash
scripts/find-vz-wad.sh --write   # once: writes the git-ignored .mercs2-local.toml
cargo xtask retail-test          # every game-gated test in the workspace
```

They find the game **only** through the repo-root `.mercs2-local.toml`; no
environment variable is consulted. The file holds one `key = "path"` per line:

| Key | Names | Read by |
|---|---|---|
| `vz_wad` | the PC base archive | every game-gated test |
| `xbox_vz_wad` | an Xbox 360 bake (`SCFF` magic) | the qm console-bake tests |
| `ps3_vz_wad` | a PS3 bake (`SCFF` magic) | the qm console-bake tests |
| `unpacked_exe` | the SecuROM-unpacked executable (`mercs2_unpacked.exe`) | the qm shader-registry disassembly test |

`scripts/find-vz-wad.sh --write` writes `vz_wad` only, and rewrites the whole file, so
the console keys and `unpacked_exe` are added by hand after it runs. When the file, a key a test needs,
or the file that key names is missing, the test fails with a message naming the file
and the key.
Two shapes exist: integration test targets declared
`required-features = ["retail"]`, and unit tests that need a crate's private items,
kept in src/ inside a `#[cfg(feature = "retail")] mod retail`. `cargo xtask
retail-test` selects both from `cargo metadata`; extra arguments go to
`cargo nextest run` (e.g. `cargo xtask retail-test --no-capture`).

CI runs on every pull request ([.github/workflows/ci.yml](.github/workflows/ci.yml)):
it runs the full workspace under nextest plus doctests (without the `retail` feature,
so no game-gated test is built there), and rolls the results up
into a per-crate pass/fail/skip table in the run's summary so a failure is
attributable to a specific crate.

## Crates

- **`mercs2_formats`** — Shared file-format parsing library (WAD, FFCS, ASET, PTHS, UCFX, etc.). Used by all other crates.
- **`wad_simulator`** — WAD consumption simulator binary.
- **`ucfx_byteswap`** — Xbox BE→PC LE converter (binary + library).
- **`dlc_port`** — DLC porter binary (work-in-progress).
- **`loadprobe`** — Log analyzer binary.

## See also

- [mercs2-pmc-blackbox](https://github.com/Mercenaries-Fan-Build/pmc-blackbox) — Game startup DLL (SecuROM spoof, ASI loader).
- [mercs2-crack-game](https://github.com/Mercenaries-Fan-Build/mercs2-securom-bypass) — EXE patcher (applies cracks and injects pmc_bb.dll).
