# ps3_dlc_crypt

The PS3 "Blow It Up Again" DLC decrypt chain, as a CLI. This is the Rust
promotion of four reference Python scripts (`ps3_pkg_unpack.py`,
`edat_decrypt.py`, `unself_decrypt.py`, `klic_scan_{cpu,gpu}.py`); those remain
only as oracles. All logic lives in `mercs2_formats::ps3_*` — this crate is a
thin clap front end.

Unlike `mercs2_probe`, it operates on external `.pkg`/`.edat`/EBOOT files and so
does **not** require a `vz.wad`.

## The chain

```
PSN .pkg ──pkg-unpack──▶ DLC01.EDAT ──edat-decrypt──▶ inner WAD (big-endian SCFF)
   │                                        ▲
   │ (v1.03 update .pkg)                    │ title klicensee
   ▼                                        │
patch EBOOT.BIN ──unself --npdrm──▶ ELF ──klic-scan──▶ 0x103f498 = klicensee
```

The decrypted inner WAD is a big-endian `SCFF` FFCS archive — the same format
the Xbox 360 route produces — so it feeds straight into the existing
`dlc_input` + `ucfx_byteswap` + `patch_wad` pipeline that `dlc_port` uses.

## Subcommands

| Command | Ports | Does |
|---------|-------|------|
| `pkg-unpack --pkg <f> [--out <dir>]` | `ps3_pkg_unpack.py` | Parse + AES-128-CTR-decrypt a retail PKG; list or extract entries |
| `edat-decrypt --edat <f> --out <wad> [--klic <hex>]` | `edat_decrypt.py` | Decrypt an NPDRM EDAT to its inner WAD (default klic = recovered DLC01) |
| `unself --self <f> --out <elf> [--npdrm]` | `unself_decrypt.py` | Decrypt a SCE SELF (disc APP, or `--npdrm` patch) to a PPC64 ELF |
| `klic-scan --edat <f> (--bin <f> \| --check <hex>)` | `klic_scan_*.py` | Slide the AES-CMAC klicensee oracle over a binary, or validate one candidate |

## Examples

```bash
# List the DLC package contents
cargo run -p ps3_dlc_crypt -- pkg-unpack --pkg MERCS2WIFDLC01NA.pkg

# Full chain to a PC-portable big-endian WAD
cargo run -p ps3_dlc_crypt -- pkg-unpack   --pkg DLC.pkg --out ./out
cargo run -p ps3_dlc_crypt -- edat-decrypt --edat ./out/USRDIR/DLC/DLC01/DLC01.EDAT --out dlc01.wad

# Reproduce the klicensee recovery from the v1.03 patch EBOOT
cargo run -p ps3_dlc_crypt -- unself    --self patch/USRDIR/EBOOT.BIN --out eboot.elf --npdrm
cargo run -p ps3_dlc_crypt -- klic-scan --edat DLC01.EDAT --bin eboot.elf
# -> FOUND off=0x103f498 klic=1896170d86be49b983b7135c96d6fb79
```

## Verification

The AES/CMAC primitives (`mercs2_formats::ps3_crypto`) carry FIPS-197,
NIST SP 800-38A and RFC 4493 known-answer tests (`cargo test -p mercs2_formats
--lib ps3`). The full chain was validated against the retail package and the
Python oracle outputs:

- `pkg-unpack` on the retail 400 MB PKG → `content_id=UP0006-BLUS30056_00-MERCS2WIFDLC01NA`, correct 16-entry table.
- `edat-decrypt` → 271,089,664-byte `SCFF` WAD (matches the recorded inner-WAD size).
- `unself` APP **and** NPDRM outputs are sha256-identical to the Python oracle ELFs.
- `klic-scan` reproduces `off=0x103f498 klic=1896170d86be49b983b7135c96d6fb79`.

## Keys

Public appldr / NPDRM / EDAT constants are the RPCS3 values (`ps3_keys.rs`,
provenance noted there). The one project-specific fact is the recovered
title-specific DLC01 klicensee `1896170d…fb79`.
