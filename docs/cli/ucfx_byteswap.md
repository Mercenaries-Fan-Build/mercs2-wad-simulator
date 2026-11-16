# ucfx_byteswap

Converts a single **already-decompressed** Mercenaries 2 UCFX block from the Xbox 360 big-endian WAD format to the PC little-endian format. It is not a blind `u32` sweep: the entry/descriptor tables are re-emitted LE, ECS component bodies are swapped at their schema-declared field widths, and several embedded payloads (Havok packfiles, GPU-tiled DXT textures, wavebanks, Lua `BINN` bytecode, mesh vertex declarations, terrainmesh) are re-encoded rather than swapped, forcing container reframes and recomputed `CSUM` trailers.

The binary is a thin CLI over the `ucfx_byteswap` library; the in-process consumer (`dlc_port`) calls `convert::convert_block` directly instead of spawning this exe.

## Synopsis

```
ucfx_byteswap [OPTIONS] [INPUT]

# Convert a BE block file to an LE block file
ucfx_byteswap block_be.bin --output block_le.bin

# Pipe mode (stdin -> stdout)
cat block_be.bin | ucfx_byteswap --stdin --stdout > block_le.bin

# Parse + report only, write nothing
ucfx_byteswap block_be.bin --dry-run

# Validate an already-LE block; no conversion
ucfx_byteswap block_le.bin --validate-only

# Untile a raw tiled Xbox DXT BODY into a PC-linear mip chain
ucfx_byteswap tiled_body.bin --untile-tex --tex-w 512 --tex-h 512 --tex-fourcc DXT5 --tex-mips 10 --output linear.bin

# ASET sub-entry recompute (binary protocol in, u32 array out)
ucfx_byteswap --stdin --aset-recompute < aset_protocol.bin > updated_u32_2.bin
```

## Options

| Flag | Value | Default | Required | Repeatable | Description |
| --- | --- | --- | --- | --- | --- |
| `[INPUT]` | path (positional) | — | Required **unless** `--stdin` | No | The input block file. In default/`--validate-only`/`--untile-tex`/`--aset-recompute` modes this is the source bytes. For default and `--validate-only` it is a decompressed BE (or, for `--validate-only`, LE) UCFX block; for `--untile-tex` it is a raw tiled DXT BODY; for `--aset-recompute` it is the binary protocol buffer. |
| `-o`, `--output <OUTPUT>` | path | none | Conditional | No | Destination file for the converted LE block (default mode) or the untiled body (`--untile-tex`). In default mode, one of `--output`, `--stdout`, or `--dry-run` must be present or the run errors. In `--untile-tex` it is mandatory (its absence panics). Ignored by `--validate-only`, `--aset-recompute`, and (for writing) `--dry-run`. |
| `--stdin` | flag | off | See INPUT | No | Read the input bytes from stdin instead of a file. Satisfies the "input required" check when no positional `INPUT` is given. |
| `--stdout` | flag | off | No | No | Write the converted LE block to stdout instead of a file (default mode only). Overrides `--output` when both are set. Rejected by `--validate-only`. Ignored by `--untile-tex` (which always writes to `--output`) and `--aset-recompute` (which always writes to stdout). |
| `--dry-run` | flag | off | No | No | Parse and convert in memory, print diagnostics/report, then return without validating or writing. Default mode only. |
| `--no-validate` | flag | off | No | No | Skip the post-conversion validation pass in default mode. Also makes `--strict` inert (nothing runs to fail). |
| `--strict` | flag | off | No | No | Treat validation issues as fatal: exit non-zero and write no output. In default mode this means exit code 2 when validation reports issues; in `--validate-only` mode it exits 2 from inside the validator. Requires validation to actually run (no effect with `--no-validate` or `--dry-run`). |
| `--report-schema-coverage` | flag | off | No | No | After conversion, print a schema field coverage report to **stderr** (unknown type codes, `schm` parse failures, schema-less components, catch-all `u32` fallbacks, "REQUIRES DEEPER INVESTIGATION" tags). Default mode only. Rejected by `--validate-only`. |
| `--validate-only` | flag | off | No | No | Validate an existing PC LE block and exit; performs **no** BE→LE conversion. For stage-2 / retail blobs. Cannot be combined with `--dry-run`, `--report-schema-coverage`, or `--stdout`. |
| `--aset-recompute` | flag | off | No | No | ASET sub-entry recompute mode. Reads the binary protocol from the input (`--stdin` or file), runs the ported `_recompute_aset_sub_entries`, and writes the updated `u32_2` array to stdout. Takes precedence over every mode below it. |
| `--untile-tex` | flag | off | No | No | Untile a raw tiled Xbox DXT BODY into the PC-linear mip chain. Requires `--tex-w`, `--tex-h`, `--tex-fourcc`, `--tex-mips`, and `--output`. |
| `--tex-w <TEX_W>` | usize | none | With `--untile-tex` | No | Texture width in pixels. |
| `--tex-h <TEX_H>` | usize | none | With `--untile-tex` | No | Texture height in pixels. |
| `--tex-fourcc <TEX_FOURCC>` | string | none | With `--untile-tex` | No | DXT FourCC, exactly 4 characters, e.g. `DXT1` or `DXT5`. |
| `--tex-mips <TEX_MIPS>` | usize | none | With `--untile-tex` | No | Number of mip levels in the body. |
| `-h`, `--help` | flag | — | No | No | Print help and exit. |

## How the options combine

**Mode selection is a fixed priority ladder.** After the input bytes are loaded (from `--stdin` or the positional file), exactly one mode runs, chosen in this order — an earlier flag wins and later flags/options are ignored:

1. `--aset-recompute` — highest priority. If set, the tool interprets the input as the ASET protocol buffer and exits after writing the `u32_2` array to stdout. `--output`, `--stdout`, `--dry-run`, `--strict`, validation, and the `--untile-*`/`--tex-*` flags are all ignored.
2. `--untile-tex` — if set (and `--aset-recompute` is not), the input is treated as a raw tiled DXT body. `--tex-w`, `--tex-h`, `--tex-fourcc`, `--tex-mips`, and `--output` are all consumed here; `--tex-fourcc` must be exactly 4 bytes. Missing `--tex-*` or `--output` values **panic** (unwrap), unlike the clean error paths elsewhere. Result is always written to the `--output` file (never stdout), followed by an `untile-tex: wrote N bytes` line on stdout.
3. `--validate-only` — if set (and neither above), the input is validated as an existing LE block; no conversion happens. This mode **rejects** `--dry-run`, `--report-schema-coverage`, and `--stdout` (combining any of them is a hard error, exit 1). It does honour `--strict` (exit 2 on issues) and `--stdin`.
4. Default conversion — when none of the above flags are set.

**Input requirement.** If `--stdin` is absent and no positional `INPUT` is given, the tool errors immediately (`provide an input file or use --stdin`, exit 1) — before any mode runs. `--stdin` and a positional file are mutually exclusive sources; `--stdin` wins the "is input satisfied" check, and if both were somehow provided the stdin bytes are read.

**Pipe mode quiets the informational chatter.** Setting `--stdin` or `--stdout` puts the tool in "pipe mode", which suppresses the human-readable progress lines (`processing (N bytes)`, `Wrote N bytes...`, `Validation: OK`, `Dry run complete.`) so stdout stays clean for the block bytes. Warnings, errors, and the coverage report (all on stderr) are unaffected.

**Output destination precedence (default mode).** `--stdout` overrides `--output`: if both are given, the block goes to stdout and the file is not written. If neither is given and it is not a `--dry-run`, the tool errors with `No output path specified` (exit 1). `--dry-run` short-circuits before any write, so `--output`/`--stdout` are silently unused under it.

**Validation and `--strict` only matter together, and only in the write path.** In default mode, validation runs after conversion unless `--no-validate` is set. Validation alone is non-fatal (warnings to stderr, output still written). `--strict` upgrades a failing validation to a fatal exit 2 with nothing written — but it needs validation to run, so `--no-validate` and `--dry-run` both make `--strict` a no-op. In `--validate-only` mode, `--strict` similarly turns any reported issue into exit 2 (otherwise exit 1 on issues, exit 0 on clean).

**`--report-schema-coverage` is conversion-only.** It attaches a coverage collector to `convert_block` and dumps it to stderr afterward, so it works in default mode (including with `--dry-run`, where the report still prints before the early return). It is rejected outright by `--validate-only`.

**`--tex-*` flags are meaningful only under `--untile-tex`.** Outside that mode they are parsed but never read.

## Examples

```bash
# 1. Standard conversion: BE block file -> LE block file, with the default
#    post-conversion validation pass (warnings printed, output still written).
ucfx_byteswap block_be.bin --output block_le.bin
# -> writes block_le.bin; prints "Wrote N bytes to block_le.bin".

# 2. Pipe mode for a build script: no chatter on stdout, just the LE bytes.
cat block_be.bin | ucfx_byteswap --stdin --stdout > block_le.bin
# -> block_le.bin contains the converted block; progress lines suppressed.

# 3. Gate a batch: fail hard if the converted block does not validate.
ucfx_byteswap block_be.bin --output out.bin --strict
# -> on clean validation: writes out.bin. On issues: exit 2, out.bin NOT written.

# 4. Investigate schema coverage without trusting the output.
ucfx_byteswap block_be.bin --dry-run --report-schema-coverage
# -> converts in memory, prints the coverage report to stderr, writes nothing.

# 5. Skip validation (e.g. a block you know trips a false-positive check).
ucfx_byteswap block_be.bin --output out.bin --no-validate
# -> writes out.bin unconditionally; --strict would be inert here.

# 6. Validate a retail / stage-2 LE blob (no conversion).
ucfx_byteswap block_le.bin --validate-only
# -> exit 0 if clean, exit 1 if issues (exit 2 if you add --strict).

# 7. Untile a tiled DXT5 512x512 body with 10 mips into a linear mip chain.
ucfx_byteswap tiled_body.bin --untile-tex \
    --tex-w 512 --tex-h 512 --tex-fourcc DXT5 --tex-mips 10 --output linear.bin
# -> writes linear.bin; prints "untile-tex: wrote N bytes to linear.bin".

# 8. ASET sub-entry recompute (binary protocol in, u32 array out).
ucfx_byteswap --stdin --aset-recompute < aset_protocol.bin > updated_u32_2.bin
# -> stdout is n x [u32 updated_u32_2] little-endian.
```

### `--aset-recompute` protocol

The input buffer (from `--stdin` or a file) is laid out as:

```
[u32 n_entries]                                   (little-endian)
n x [u32 asset_hash][u32 u32_2][u8 primary][u8 in_base]   (10 bytes/row)
<decompressed LE block bytes>                     (remainder)
```

Output on stdout: `n x [u32 updated_u32_2]` (little-endian), one per input entry, in order.

## Failure modes

| Symptom | Cause | Exit |
| --- | --- | --- |
| `Error: provide an input file or use --stdin` | No positional `INPUT` and no `--stdin`. | 1 |
| `Error reading <path>: ...` | The positional input file could not be read. | 1 |
| `Error reading stdin: ...` | `--stdin` set but stdin read failed. | 1 |
| `Conversion error: ...` | `convert::convert_block` returned an error (malformed/unexpected block structure). | 1 |
| `No output path specified (use --output, --stdout, or --dry-run)` | Default mode with none of `--output`, `--stdout`, `--dry-run`. | 1 |
| `Error writing <path>: ...` / `Error writing to stdout: ...` | The output write failed. | 1 |
| `Strict mode: aborting due to validation errors` | `--strict` and validation reported issues. | 2 |
| `Error: --validate-only cannot be combined with --dry-run, --report-schema-coverage, or --stdout` | Illegal flag combination with `--validate-only`. | 1 |
| `aset-recompute: short input` / `truncated entry table` / `stdout write failed: ...` | `--aset-recompute` buffer smaller than the header, smaller than the declared entry table, or a stdout write error. | 1 |
| `--tex-fourcc must be 4 chars` | `--untile-tex` with a FourCC that is not exactly 4 bytes. | 1 |
| `untile-tex: conversion failed (body too short?)` | `--untile-tex` untile returned `None` (body shorter than the declared mip chain). | 1 |
| `Error writing <path>: ...` (untile) | `--untile-tex` output write failed. | 1 |
| **panic** `--tex-w required` / `--tex-h required` / `--tex-fourcc required` / `--tex-mips required` / `--output required` | `--untile-tex` missing one of its mandatory parameters — these are `.expect()` unwraps, so the process panics rather than exiting cleanly. | panic |

Note: validation issues without `--strict` are **not** failures — they print `WARN:` lines to stderr and the tool still writes its output and exits 0.
