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

### `exemap`
Identifies a `Mercenaries2.exe` build by sha256 and checks it against its code layout: every byte
that differs from the layout's reference build must be a catalogued overlay
(`crates/mercs2_exemap/data/*.tsv`). Exit status 0 on a member, 1 on an unknown build or an
unlisted difference, 2 when a file cannot be read.

The two code layouts are the two code versions of the game. `U` (PE timestamp `0x48a370c7`) is
v1.0: the retail `v10-unsigned` build and every cracked, DRM-free and SecuROM-unwrapped build
in the catalogue. `E` (PE timestamp `0x48cae190`) is v1.1: `v11-official-securom`, which the
official v1.1 patcher (sha256 `b27fedc2…`, one file for all four regions) writes from
`v10-unsigned`, and `v11-official-unwrapped`, the same code with the SecuROM wrapper removed.
`builds.tsv` records each build's `code_version`; `aliases.tsv` maps other ids for a build,
including the ones Modkit uses, to its canonical id, and `identify` prints the canonical id.

```bash
exemap identify Mercenaries2.exe
exemap check Mercenaries2.exe --reference <layout-reference.exe>
```

It also takes in the console executables for disassembly:

- `exemap xex <xex> --key retail|devkit --out <pe>` recovers the PE image from an Xbox 360 XEX2
  file: it unwraps the session key with the named key-encryption key, decrypts the basefile and
  undoes its compression (LZX through `mercs2_formats::lzx`). A key that does not fit the file is
  an error naming the key.
- `exemap ghidra-pe <pe> --out <pe>` rewrites an Xbox 360 image's section headers to its RVA
  layout, which Ghidra's PE loader needs.
- `exemap starts <image> --out <list>` writes one hex VA per line: the `.pdata` starts of an Xbox
  360 PE (every executable section, `.text` and `BINK`), or the `.opd` descriptor code addresses of
  a PS3 PPU ELF (found from the entry descriptor). The list seeds Ghidra's `CreateFunctions.java`.

```bash
exemap xex mercs2_xenon_p_EN_FR.xex --key devkit --out profile.pe
exemap ghidra-pe profile.pe --out profile.pe_ghidra
exemap starts profile.pe_ghidra --out profile.starts
```

`crates/mercs2_exemap/data/images.tsv` lists the images the function map spans on every platform,
each with the Ghidra language and load base it is analysed with. The PC rows are the two layout
references' SecuROM-unwrapped dumps, named by build id. Each console row carries the chain that
rebuilds it from its original container (a file from the Xbox disc, `xex`, `ghidra-pe`; a PSN
package entry, `unself`), and `exemap derive` replays it, checking every sha256 on the way. A
regional build whose executable sections and function starts match another image byte for byte
names that image as its `code_image` (the PS3 v1.03 EU and DE EBOOTs share the NA code and differ
only in their embedded region config).

`exemap rtti <image> --out <tsv>` recovers vtables from an image's own C++ type information and
writes one row per slot (ABI, decorated name, `Foo::Bar` class name, vtable address, subobject
offset, slot index, function address): MSVC locators in the PC and Xbox 360 PEs (both byte orders),
GCC/Itanium typeinfo in the PS3 ELFs, where each slot is an `.opd` descriptor resolved to its code.
The PC and Xbox images carry type information for about 270 classes (mostly Havok); the PS3 images
for about 1,290, game classes included. Xbox 360 `.pdata` omits leaf functions, so about half of
the Xbox slots point at functions that have no `.pdata` entry.

```bash
exemap derive x360-profile --from "Mercenaries 2 Preview X360 (Jul 11 2008).iso" --out profile.pe_ghidra
exemap derive ps3-v103-na --from "UP0006-BLUS30056_00-MERCS2PATCHNA103-A0103-V0100-PE.pkg" --out eboot103.elf
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
- `exemap` / `exemap.exe`

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
| `exe_corpus` | a directory of retail `Mercenaries2.exe` builds (`game-files/exe_sha256`) | the `mercs2_exemap` corpus tests |
| `exe_dumps` | a directory of derived executables and process-image dumps (`output/_ghidra/securom_dump`) | the `mercs2_exemap` corpus tests |
| `drmfree_exe` | the DRM-free executable (`mercs2_nodrm_v3.exe`, v1.0 code) | the `mercs2_exemap` corpus tests |
| `v11_patcher_exe` | the official v1.1 patcher (`Mercs2 WiF v1.1 Patch/mercenaries2_patch1_na.exe`) | the `mercs2_exemap` official-patcher test |
| `wine_binary` | the `wine64` binary that runs the patcher (Modkit's `…/Wine-Crossover-23.7.1-1/Contents/Resources/wine/bin/wine64`) | the `mercs2_exemap` official-patcher test |
| `x360_devkit_iso` | the Jul 11 2008 Xbox 360 preview disc image (`Mercenaries 2 Preview X360 (Jul 11 2008).iso`) | the `mercs2_exemap` console-image test |
| `ps3_eboot_bin` | the PS3 disc's `EBOOT.BIN` (BLUS30056) | the `mercs2_exemap` console-image test |
| `ps3_update_pkgs` | a directory of the PS3 v1.03 update `.pkg` files (NA, EU, DE) | the `mercs2_exemap` console-image test |

`scripts/find-vz-wad.sh --write` writes `vz_wad` only, and rewrites the whole file, so
the console keys, `unpacked_exe`, the three executable keys, the patcher keys and the console-image keys are added by hand after it runs. When the file, a key a test needs,
or the file that key names is missing, the test fails with a message naming the file
and the key.
Two shapes exist: integration test targets declared
`required-features = ["retail"]`, and unit tests that need a crate's private items,
kept in src/ inside a `#[cfg(feature = "retail")] mod retail`. `cargo xtask
retail-test` selects both from `cargo metadata`; extra arguments go to
`cargo nextest run` (e.g. `cargo xtask retail-test --no-capture`).

The official-patcher test reproduces the `v11-official-securom` recipe: it copies
`v10-unsigned` from `exe_corpus` into a temporary install folder, creates a throwaway
`WINEPREFIX` in the system temp dir, imports the one registry value the patcher reads
(`HKLM\SOFTWARE\WOW6432Node\EA Games\Mercenaries 2 World in Flames\Install Dir` = the
install folder) with `wine64 reg import`, runs the patcher with the install folder as its
working directory, and requires the patched `Mercenaries2.exe` to be `v11-official-securom`
and `BACKUP\Mercenaries2.A###.exe` to be the input. It stops the prefix's `wineserver` and
removes the folder and prefix afterwards.

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
- **`mercs2_exemap`** — The cross-build exe map: the catalogue of known `Mercenaries2.exe` builds and dumps, their two code layouts, the byte overlays that separate each build from its layout reference, and the `exemap` binary.

## See also

- [mercs2-pmc-blackbox](https://github.com/Mercenaries-Fan-Build/pmc-blackbox) — Game startup DLL (SecuROM spoof, ASI loader).
- [mercs2-crack-game](https://github.com/Mercenaries-Fan-Build/mercs2-securom-bypass) — EXE patcher (applies cracks and injects pmc_bb.dll).
