# securom_unwrap

Turn a SecuROM-cracked-but-**decrypted** Mercenaries 2 PC executable into a SecuROM-free
one by restoring the original entry point and a clean import table — keeping every section,
including the SecuROM-named sections that hold relocated game code, byte-for-byte.

## What it is

A static PE rewriter (library + CLI) for game preservation on a legally-owned copy. It does
**not** decrypt anything and does not touch the game's code bytes. It performs the standard
"run-to-OEP" endgame *statically* on an image whose code is already decrypted on disk:

1. **Repoint the entry point** from the SecuROM/loader stub to the real original entry point
   (`WinMainCRTStartup`), so none of the protection trigger code ever runs.
2. **Rebuild a clean import directory** listing only the game's own imports, dropping the
   SecuROM loader's duplicate import descriptors.

Every section is preserved verbatim, so the relocated game code that SecuROM splices into its
own sections keeps working. The result needs no disc, activation, online check, or crack loader.

## Where it comes from

SecuROM 7.x on Mercenaries 2 (PC) uses *code-splicing*: chunks of the game's own code are
relocated into SecuROM-named sections (`Stext`, `.securom`, …) and reached by `jmp`/`call` from
`.text`. A double-blind reverse-engineering study on this project established that the protection
runtime **cannot** be removed by simply dropping those sections without devirtualizing ~700
spliced macros — proven not worth it (weeks of work, zero functional gain). The supported,
robust transform is the run-to-OEP rewrite this crate implements. See the licensing/SecuROM
code map in the project docs for the full analysis.

The input image must already have decrypted code on disk (for example a RELOADED-unpacked
build); this crate deliberately does **not** decrypt SecuROM sections.

## Usage

CLI:

```bash
securom_unwrap <input.exe> <output.exe>

# Override the auto-derived OEP with a hex RVA:
securom_unwrap in.exe out.exe --oep 0x5ee71c

# Treat extra section name(s) as SecuROM's (repeatable):
securom_unwrap in.exe out.exe --securom-section .myprot
```

Library:

```rust
use securom_unwrap::{unwrap, Options};

let input = std::fs::read("mercs2-decrypted.exe")?;
let (out_bytes, report) = unwrap(&input, &Options::default())?;
std::fs::write("mercs2-securom-free.exe", &out_bytes)?;
println!(
    "OEP 0x{:08X} -> 0x{:08X}; kept {} imports, dropped {}",
    report.original_entry, report.oep, report.kept.len(), report.dropped.len()
);
```

## Notes

- `publish = false` — this crate is intentionally excluded from the crates.io release set; it
  ships only as a binary on the GitHub release.
- The OEP is derived automatically (`derive_oep`) but can be overridden with `--oep` when the
  heuristic does not match your build.
- Read-in / read-out only: the tool never runs the executable and never contacts a network.
