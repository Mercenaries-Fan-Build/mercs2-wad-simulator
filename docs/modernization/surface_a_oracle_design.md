# Surface A Oracle — asset → struct capture (design)

**Status:** design + first-capture scaffold
**Date:** 2026-06-30
**Charter:** [`00_charter.md`](00_charter.md) — "Surface A — asset → struct"
**Sibling (done):** Surface B binding-call oracle — [`../../mods/lua_trace_asi/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/lua_trace_asi/README.md)

## What Surface A is

Surface A captures the **original game's PARSED structs at known load points**, so the Rust
reimplementation's parsers can be diffed **byte-for-byte** against ground truth.

> Same input at a boundary → same output at that boundary.

For a parser this is the cleanest possible gate: the boundary is a pure, deterministic function
`parse(asset_bytes) -> struct`. If the Rust parser, fed the identical input span the original
consumed, produces the identical output-struct bytes, the reimplementation is **provably equivalent
by construction** for that function — no rendering, no scenario, no timing. This is the same
methodology already practiced with the `base_xbox → converter` vs `base_pc` oracle, generalized to
capture the *engine's own* parse output at runtime instead of a re-derived file.

Surface A and Surface B are complementary: B gates the *script→engine* boundary (upgrading Lua),
A gates the *asset→struct* boundary (rewriting parsers/streaming). Phase 1 (first triangle) and
Phase 2 (deterministic core, oracle-diffed) of the roadmap both consume Surface A.

## Capture targets (first three, ranked)

All three are deterministic `asset in → struct out` boundaries drawn from
[`../engine_load_path_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/engine_load_path_map.md) and
[`../../scripts/mercs2_annotations.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/scripts/mercs2_annotations.json).

| # | Function | VA | Boundary | Why |
|---|----------|----|----------|-----|
| **A1 (first)** | **`Mtrl_Parse`** | `0x00858790` | MTRL chunk → 10-slot material struct | Small, fully self-contained, hot, already the most-studied parser in the repo (multiple RCA notes). Fixed input reader + fixed output struct. `__stdcall`, `ret 0x8`. |
| A2 | `Tex_ConsumeChunk` | `0x00750a30` | INFO(0x3c hdr) → texture header (w/h/mip/fmt/surface-array) | The DXT mip-count field that caused the livelock lives here; high diffing value for the streaming rewrite. |
| A3 | `Chunk_GetEntryReader` | `0x00464780` | WAD block entry-table index → 0x14-stride reader | The universal chunk-reader every consumer calls; validates the block-entry-table parse itself. |

**First target = A1 `Mtrl_Parse`.** It is the canonical small deterministic parser: one input
stream, one output struct, no cross-object state, and we already have its decomp, its callers, and
its failure modes fully mapped.

### A1 boundary shape (decomp-verified, `FUN_00858790`)

```c
void __stdcall Mtrl_Parse(void *out_material /*param_1*/, ChunkReader *reader /*param_2*/);
```

- **Input** = the chunk `reader` (`param_2`). Byte cursor is `*(reader + 0x10)`; the chunk base is
  `*(reader + 0x18)`; the running "bytes over" counter is `*(reader + 0x14)`. The absolute read
  pointer is `base + cursor`. The parser advances `cursor` field-by-field, so the **input asset
  span it consumed** is `[base + cursor_at_entry, base + cursor_at_exit)`.
- **Output** = the `out_material` struct (`param_1`), written in place. Fields observed in the
  decomp: transform/UV floats from `+0x00`; type-hash-ish `+0x64`; flags u16 `+0x50`; **u16
  texture-count `+0xa2`**; the **10-slot `{hash, 0xF011157A, 0}` texture array at `+0xac`** (stride
  0x0c); a parallel handle array at `+0x144`; final derived floats up to `+0x9c..+0x182`. A capture
  window of **0x1C0 (448) bytes** from `param_1` covers the whole struct with headroom.

Because the parser both reads the input and fully writes the output, the record is taken **at
function exit**: capture `cursor_at_entry` on the way in, hash `[base+entry, base+exit)` and dump
`out_material[0..0x1C0]` on the way out.

## Mechanism — recommendation

Two options were on the table:

- **(a) ASI probe** — hook the parse function in-process, dump `{input, output}` records to a file.
  Modeled on [`../../tools/resprobe/resprobe.c`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/tools/resprobe/resprobe.c) and
  [`../../mods/lua_trace_asi/src/lua_trace_asi.c`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/lua_trace_asi/src/lua_trace_asi.c):
  MinHook, naked detours that preserve live registers, a lock-free ring on the hot path, and a
  watcher thread that does all I/O.
- **(b) x32dbg MCP scripting** — set a breakpoint, read memory read-only when paused, user drives
  execution.

**Recommendation: (a) ASI probe.** Reasons:

1. **Automatable / unattended.** Surface A wants *thousands* of parse instances captured in one
   world-load run to build a corpus; x32dbg is a manual single-shot inspector (and the standing
   mandate is *never resume* the x32dbg process from the bridge, so it cannot pump a whole load).
2. **Proven in this repo.** `resprobe` and `lua_trace` already hook the exact hot load-path with
   this pattern without destabilizing the game.
3. **Hot-path safe.** The detours do zero I/O; they push fixed-size records into a lock-free ring
   and a watcher thread flushes. Memory reads are bounds-guarded, matching `lua_trace`'s
   `VirtualQuery` discipline, so a bad pointer degrades to a blanked field, never an AV.
4. **x32dbg stays the *tuning* tool.** Use the bridge (read-only, paused) to confirm the reader/
   struct offsets live before/if they ever drift — the ASI takes the offsets as compile-time
   constants derived from the decomp and is exe-size-guarded like `resprobe`.

## Record format (`surface_a.bin`, length-delimited binary)

A binary stream of length-delimited records (not NDJSON — struct bytes are the payload and must be
captured verbatim, not JSON-escaped). One 8-byte file header, then N records:

```
File header (8 bytes):
  char   magic[4]   = "SA1\0"
  u32    version    = 1

Record (variable):
  u32    rec_len          # bytes following this field (for skip-scan)
  u32    target_va        # e.g. 0x00858790 — which parser produced this
  u32    seq              # monotonic capture order
  u32    input_len        # bytes of consumed input span
  u32    input_crc32      # CRC32 of the input span (fast identity)
  u32    output_len       # bytes of dumped output struct (A1: 0x1C0)
  u8     input[input_len] # the exact asset bytes the parser consumed
  u8     output[output_len]# the parsed struct, verbatim
```

Rationale:
- **Full input bytes + CRC.** The CRC is the fast dedupe/identity key; the raw input bytes are what
  the Rust parser is *fed* so the diff is apples-to-apples (no re-deriving the span from the WAD).
- **Full output bytes.** The whole point — the ground-truth struct to diff against.
- **`target_va`** lets one file hold captures from A1/A2/A3 once the probe is extended; the Rust
  side dispatches on it.
- **Dedupe** by `(target_va, input_crc32)` in the ring, exactly like `resprobe`'s `Seen()` — a WAD
  ships each material once but it may be parsed many times; we want one golden record per distinct
  input.

A companion `surface_a.log` carries human-readable diagnostics (hook-armed, exe-ok, ring overflow),
matching `resprobe.log`.

## How the Rust side diffs

The `wad_simulator` workspace gains an oracle test (Phase 2 harness):

1. **Ingest** `surface_a.bin`: for each record, `(target_va, input[], output[])`.
2. **Feed** `input[]` to the Rust parser for that `target_va`
   (`target_va == 0x858790 → wad_simulator::mtrl::parse(input) -> Material`).
3. **Serialize** the Rust `Material` to the *same in-memory layout* the engine uses (the struct is
   POD: floats + u16 count + 10× `{u32 hash, u32 0xF011157A, u32 0}` + tail). Emit `output_len`
   bytes.
4. **Assert byte-identical** to the record's `output[]`. First differing offset localizes the bug to
   an exact field (e.g. the historical u16-tex-count-at-0xa2 swap, or the tangent DEC3N layout).
5. Fields that are legitimately non-deterministic (live pointers/handles — e.g. the `+0x144` handle
   array is filled from a runtime hash table, and the `0xF011157A` slots hold a resolved handle in
   some paths) are **masked** by a per-target field-mask before comparison. For A1 the mask covers
   the resolved-handle dwords; the `hash` and `0xF011157A` sentinel and all float/flag fields are
   compared exactly. The mask lives beside the Rust parser so it is reviewed as part of the gate.

This makes the parser rewrite a **gated, function-by-function** effort: a parser ships only when it
reproduces every golden record for its target byte-for-byte (mod the documented mask).

## Files

- [`docs/modernization/surface_a_oracle_design.md`](surface_a_oracle_design.md) — this document.
- [`mods/surface_a_probe/src/surface_a_asi.c`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/surface_a_probe/src/surface_a_asi.c) — the skeleton probe (A1 = `Mtrl_Parse`).
- [`mods/surface_a_probe/Makefile`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/surface_a_probe/Makefile) — i686 MinGW build (`make`), vendored MinHook.
- [`mods/surface_a_probe/README.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/surface_a_probe/README.md) — build/deploy/output.

## Open items (v2)

- Extend the probe to A2/A3 (one more entry/exit hook pair each; `target_va` already in the record).
- Confirm the A1 output-struct high-water mark (0x1C0) and the runtime-handle field-mask live via
  the x32dbg bridge (read-only, paused) before locking the Rust comparison mask.
- Optional: capture the `Chunk_Alloc` size the engine chose per material, to also gate the allocator
  size-class decision (Surface A extended to the streaming buffer-size chain — the `FUN_00875b00`
  page-count boundary from `engine-streaming-buffer-sizing-chain`).
