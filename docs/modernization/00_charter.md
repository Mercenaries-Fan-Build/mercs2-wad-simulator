# Mercenaries 2 — Modernization Program Charter

**Status:** Founding charter (living document)
**Date:** 2026-06-30

## Vision

Rebuild Mercenaries 2 as a **native 64-bit, maintainable, faithful, faster** open engine that
consumes the **original game's data** (WAD/script/model assets), reimplementing systems one at a
time. The original 32-bit retail exe + our full decomp + the x32dbg live bridge become the
**reference oracle and specification** — not the shipping artifact.

This is a multi-year program. Each phase ships standalone value and is gated on provable parity
with the original.

## Governing principle

> **Implementation is free; behavior is gated.** Nothing is off the table — newer Lua, a modern
> renderer, different data structures, 64-bit everything — *as long as it passes its oracle gate*.
> The constraint is **provable equivalence of behavior**, not fidelity of implementation.

## Locked decisions

| # | Decision | Rationale |
|---|----------|-----------|
| D1 | **Reimplement-from-spec**, not auto-lift the binary | Static binary translators (McSema/remill, RetDec) produce a faster *opaque blob* — fails the maintainability goal, chokes on C++ vtables/exceptions/SSE and SecuROM-packed regions. |
| D2 | **Rust** engine, built on the existing `wad_simulator` workspace | Aligns with the standing "Rust not Python" tooling mandate; `wad_simulator` is already the asset layer. |
| D3 | Renderer = **`wgpu`** (DX12 / Vulkan / Metal) | Exceeds the "DX11" ask, portable, modern. |
| D4 | **No DXVK bridge.** Original exe kept purely as oracle | User chose engine-only; do not spend effort wrapping the 32-bit renderer. |
| D5 | **Lua 5.4** via `mlua` (5.1 = conservative fallback) | Native 64-bit integers → money/economy is `i64` for free; migration surface is small (see below). |
| D6 | Original exe + Ghidra decomp + x32dbg = **oracle/spec** | We have spent months writing this spec already (see Assets). |

## Why 64-bit is the point (and why the old money question dissolves)

The original currency is **signed int32** (cash @ `singleton+0x2C`, fuel @ `+0x30`), and the 1-billion
cap is a *Lua soft-clamp* (`knBillion` in `mrxpmc.lua`), not a type limit. On the old binary, making
money truly bigger was hard only because of artifacts — the float `lua_Number`, the packed struct, the
save format. On a 64-bit foundation with Lua 5.4 integers, money is just `i64`. The hard problem
disappears by construction. See memory `money-fuel-datatype-and-cap`.

## Head start — spec assets already on disk

- Full named Ghidra decomp + **engine load-path map** ([`docs/engine_load_path_map.md`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/docs/engine_load_path_map.md), 64 named fns)
- **ECS component registry** — 220 native classes w/ schemas, defaults, hashes ([`docs/mercs2-ecs/`](../../crates/mercs2_workshop/data/mercs2-ecs/))
- **Entire base-game Lua**, decompiled + readable ([`docs/mercs2-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-luacd)); DLC Lua ([`docs/mercs2-dlc-luacd/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/docs/mercs2-dlc-luacd))
- **`wad_simulator`** — Rust WAD/asset parser (the new engine's asset crate)
- **`loadprobe`** + **x32dbg MCP bridge** — behavioral oracle, already in use
- **`pmc_bb.dll`** — already hooks the Lua log stream (extend it to a binding-call tracer; see harness)

## Regression harness (the foundation — built before/with the engine)

Everything reduces to **same inputs at a boundary → same outputs at that boundary.** Two oracle surfaces:

- **Surface A — asset → struct.** Pure/deterministic. Dump original's parsed structs via x32dbg at
  known load points; assert Rust reimpl is byte-identical. (Already practiced: converter-vs-retail.)
- **Surface B — script → engine bindings.** Capture the **ordered sequence of C-binding calls + args**
  the original scripts emit (extend `pmc_bb.dll` to a binding tracer → golden trace). Replay the same
  scenario in the new VM; assert trace-equivalence. **This is what makes upgrading Lua safe** — we
  don't care *how* 5.4 computes, only that it emits the same engine calls.

Layered gates:
- **Render golden tests** — render-target/frame hashes + perceptual diff w/ tolerance (extends the
  `base_xbox→converter` golden method).
- **Behavioral milestone gates** — `loadprobe`-style scoring of world-load phases / entity spawns.
- **Record/replay determinism** — captured input timeline + identical RNG seed → assert same state
  trajectory (gold standard; requires identical RNG seeding from day one).

**Rule:** nothing ships in the new engine until it passes its oracle gate.

## Lua 5.1 → 5.4 migration surface (measured across 409 corpus files)

| Construct | Files | Fix |
|-----------|-------|-----|
| `setfenv` / `module` / `loadstring` / `table.setn` / `math.mod` / `string.gfind` | 0 | clean |
| `unpack(` | 40 | compat alias `unpack = table.unpack` |
| `table.getn` | 54 | compat alias `#t` |
| `arg` (vararg table) | 4 | `...` / `table.pack` |
| `%d` format | 16 | ensure integer subtype (free once money is `i64`) |
| `getfenv` | 19 | **hand-review** — `_ENV` rework |

~73 file-touches absorbed by a tiny compat prelude (no codemod); 19 `getfenv` sites reviewed by hand.

## Phased roadmap (each phase ships value; each gated on parity)

0. **Harness + skeleton** — stand up the oracle harness (binding tracer, struct-dump capture,
   render-golden scaffold) and the Rust engine shell (window + `wgpu` device + `wad_simulator` asset crate).
1. **First triangle from real data** — load one level's geometry+textures from original WADs, render
   in 64-bit. No gameplay. Validated by render-golden.
2. **Deterministic core, oracle-diffed** — reimplement the streaming/load path (we have the map +
   `loadprobe`), function-by-function vs x32dbg (Surface A).
3. **ECS + scripting** — Rust ECS matching the 220 component schemas; embed Lua 5.4; run migrated
   scripts validated by binding-trace equivalence (Surface B). Money is `i64` here.
4. **Gameplay → AI → physics → audio → net** — fold in systems; online stack already restored.

Throughout: original exe = oracle, decomp = spec, x32dbg = judge of "faithful."

## Open questions / TODO

- Physics: reimplement Havok-equivalent vs adopt a modern physics lib (parity-gate either way).
- Save format: new engine's save vs original `SaveSingleton` layout — migration/compat policy.
- Bink video (`binkw32`): replace with a modern codec path.
- Audio: DirectSound → modern mixer (we already ported wavebank/XMA tooling to Rust).
