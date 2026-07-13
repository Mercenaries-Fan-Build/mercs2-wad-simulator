# securom_unwrap

Turns a SecuROM-cracked-**but-already-decrypted** Mercenaries 2 PC executable into a SecuROM-free one by repointing the PE entry to the real original entry point (OEP) and rebuilding a clean import directory, so none of the protection trigger code ever runs. Every section is preserved verbatim — including the SecuROM-named sections that hold relocated ("code-spliced") game code — so the output is byte-for-byte identical to the input apart from a handful of header/data-directory edits.

This crate does **not** decrypt SecuROM sections; the input must already have decrypted code on disk (e.g. a RELOADED-unpacked build).

## Synopsis

```
securom_unwrap [OPTIONS] <INPUT> <OUTPUT>
```

## Options

| Flag / arg | Value type | Default | Required | Repeatable | Effect |
|---|---|---|---|---|---|
| `<INPUT>` | path | — | yes | no | The decrypted PE to read (positional #1). A 32-bit PE32 image. |
| `<OUTPUT>` | path | — | yes | no | Path the SecuROM-free PE is written to (positional #2). Overwrites if it exists. |
| `--oep <OEP>` | hex RVA (`u32`) | derived from the entry stub | no | no (last wins) | Override the original entry point, as a hex RVA. `0x`/`0X` prefix is optional (e.g. `--oep 0x5ee71c` or `--oep 5ee71c`). When set, the built-in OEP walker is skipped entirely. |
| `--securom-section <NAME>` | string (section name) | see below | no | yes | Add an extra section name to the set treated as SecuROM's. **Extends**, does not replace, the built-in list. Matching is case-insensitive. |
| `-h`, `--help` | — | — | no | no | Print help and exit. |
| `-V`, `--version` | — | — | no | no | Print version and exit. |

### Default SecuROM section set

An import descriptor whose IAT (`FirstThunk`) array lives in one of these sections is classed as SecuROM's and dropped from the rebuilt import table. The sections themselves are still kept in the output. Matching lowercases both sides.

```
stext  sitext  srdata  sdata  sidata  .securom  reloaded
```

`--securom-section` names are appended to this list; there is no flag to remove a default.

## How the options combine

The two positional paths are independent I/O. The two real levers — `--oep` and `--securom-section` — steer the two halves of the transform (entry-point fix and import rebuild) separately, and each changes what ends up in the output header.

**`--oep` short-circuits OEP derivation.**
Without it, the tool walks the entry stub with a tiny opcode stepper (up to 64 instructions) looking for the `call;jmp` `WinMainCRTStartup` signature. If the entry already points at that signature, the entry is used unchanged. Supplying `--oep` replaces this entirely: no walk is performed, so the `OepNotFound` failure can never occur when `--oep` is given, and a wrong value is accepted blindly (the walker's validation is bypassed). The chosen OEP is written to `AddressOfEntryPoint`; the report prints both the new and the original entry.

**`--securom-section` decides the kept/dropped import partition, and everything the IAT directory reports flows from it.**
Each import descriptor is tagged SecuROM or not by which section its `FirstThunk` RVA falls in. The kept imports must form a *leading prefix* of the descriptor array and all SecuROM descriptors must follow as a suffix. Consequences of adding section names:

- More descriptors can move from *kept* to *dropped*, shrinking the rebuilt import directory. The import-directory **size** written to the header is `(kept_count + 1) * 20` bytes (kept descriptors plus one null terminator), and the descriptor slot immediately after the kept run is zeroed to terminate the array.
- The **IAT data directory** (`iat_rva`/`iat_size`, data directory 12) is recomputed to span only the *kept* descriptors' thunk arrays (min start RVA to max end RVA). Change the partition and this span changes.
- If a newly-matched section splits the game's imports — i.e. a SecuROM descriptor appears *before* a game one — the prefix invariant breaks and the run aborts with `ImportsNotPrefix` rather than silently dropping a real import. So `--securom-section` can *cause* that failure if pointed at a section the game legitimately imports through.

`--oep` and `--securom-section` never interact with each other; they touch disjoint parts of the header. Regardless of options, the output keeps the input's section layout and file size unchanged — the only writes are: `AddressOfEntryPoint` → OEP, `CheckSum` → 0, import-directory size, IAT directory RVA+size, and the one zeroed terminator descriptor.

## Examples

Basic run — derive the OEP automatically, use the default SecuROM section set:

```
securom_unwrap mercs2_reloaded.exe mercs2_clean.exe
```
Produces `mercs2_clean.exe` with the entry repointed to the derived OEP and the SecuROM loader's import descriptors removed. Prints the OEP (old and new), the IAT directory range, the kept imports (one line per DLL with thunk count), and the dropped SecuROM DLLs.

Force a known OEP (skips the stub walker — useful when the entry stub doesn't match the built-in opcode set):

```
securom_unwrap --oep 0x5ee71c mercs2_reloaded.exe mercs2_clean.exe
```
Same output, but `AddressOfEntryPoint` is set to RVA `0x5ee71c` without any derivation. The leading `0x` is optional.

Add a non-standard SecuROM section name seen in a particular build:

```
securom_unwrap --securom-section .cms --securom-section pelock \
  mercs2_reloaded.exe mercs2_clean.exe
```
Descriptors whose IAT lives in `.cms` or `pelock` (in addition to the seven defaults) are dropped. The repeated flag accumulates.

## Failure modes

All errors are printed to stderr and the process exits non-zero.

- `error: cannot read <path>: <io error>` — the `<INPUT>` file could not be read.
- `error: cannot write <path>: <io error>` — the `<OUTPUT>` path could not be written (the transform succeeded; only the write failed).
- `error: not a 32-bit PE image` — missing `MZ`/`PE\0\0`, or the optional-header magic is not `0x10b` (PE32). This tool is 32-bit-only; a 64-bit (`0x20b`) image trips this.
- `error: image truncated while reading <what>` — a structure (section header, name, thunk array, u16/u32 field) ran past the end of the buffer; the file is truncated or malformed.
- `error: image has no import directory` — data directory 1 is empty, or its RVA does not map to a file offset.
- `error: could not derive original entry point (pass --oep)` — the entry-stub walker did not reach a `WinMainCRTStartup` signature within 64 instructions (unrecognized stub, or the walk left mapped memory). Re-run with an explicit `--oep`.
- `error: SecuROM imports interleaved with game imports (needs full rebuild)` — a SecuROM-tagged descriptor sits before a game one, so simple truncation would drop a real import. Check whether a `--securom-section` value is matching a section the game genuinely imports through; a true interleave needs a full import-table rebuild this tool does not perform.
- `invalid hex '<v>': <parse error>` — the `--oep` value is not valid hexadecimal (clap argument-parse error, before any work is done).
