# Surface B — `lua_trace.asi` Lua→Engine Binding-Call Oracle

**Status: FULLY WORKING, lossless, live-validated (2026-06-30 / 2026-07-01).** Source of
record: memory `lua-trace-asi-surface-b-oracle`. Pairs with `surface_a_oracle_design.md`
(Surface A = engine bone-transform oracle); this is **Surface B** of the modernization
regression harness — it proves a reimplemented / newer-Lua VM is **trace-equivalent** to
the original by capturing the ordered Lua→engine C-binding call stream (name + args).

## Design

[`mods/lua_trace_asi/`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/tree/main/mods/lua_trace_asi) — a standalone ASI:

1. **Runtime PE-scan** of `.rdata` for `luaL_Reg` tables (no hardcoded addresses; derefs
   bounded to the module image). Skips Lua-stdlib / Scaleform tables by name marker
   (`LUA_TRACE_STDLIB=1` to include them).
2. Per binding, install a **16-byte index thunk** (`mov eax, i; jmp SharedDetour`) and
   **MinHook** each cfunc (inline hook = timing-independent, unlike dlc_enable's
   pre-registration pointer swap).
3. `SharedDetour` reads `idx` (EAX) + `L` (cdecl arg); `Record` reads `argc` + ≤4 args off
   the Lua stack into a **lock-free ring**; a watcher thread writes `lua_trace.ndjson`.

Built on the `resprobe.c` pattern + vendored `submodules/minhook`.

**Surface:** ~1216 game bindings / 53 tables (Player 107, Hud 114, Object 87, World 80,
Ai 66, …). Host reference: [`mods/lua_trace_asi/reference/binding_map.json`](https://github.com/Mercenaries-Fan-Build/notes-on-the-released-game/blob/main/mods/lua_trace_asi/reference/binding_map.json).

**Lua 5.1 offsets** (env-overridable for x32dbg tuning): `top @ L+8` (confirmed via
dlc_enable), `base @ L+0xC`, `TString` data `@ +0x10`. `lua_Number = float`, so numeric
args are float bits.

**Build:** `make` with i686 MinGW (`C:\Users\Shadow\mingw32\bin`) + GnuWin32 make →
`lua_trace.asi` (~108 KB PE32 i386). Deploy to the game's `scripts/`; the loader
auto-loads all `.asi` there.

## Validation

- Live run reached 95% / phase-19, hooked **1087 bindings / 49 tables**, captured 220K
  records / 24 MB. Offsets `top@8 / base@0xC / tstr@0x10` CONFIRMED — args decode
  correctly (`Printf("All movies complete")`, `Post("mpPlayerJoin", <table>)`,
  `GetCash`/`GetFuel` argc=0, `CreatePersistent(25, …)` argc=5).
- **Lossless ring (sha `4d1545a0`):** circular MPSC (atomic-claim producers, wrap-safe
  overrun drop, per-slot commit marker, MemoryBarrier before commit) + batched watcher
  (1 MB buffer, one WriteFile per 100 ms). Validated: 226,687 records, seq contiguous
  0→226686, **0 overflow warns = fully lossless.**

## Bugs found + fixed (via loadprobe crash forensics)

1. Candidate-ptr validation used the whole `SizeOfImage` → hit SecuROM NO_ACCESS pages →
   AV. **Fix:** confine scan + derefs to `.rdata` only (all binding tables live there).
2. Bounds check `va+48 >= g_rdHi` **32-bit-overflowed** on a `0xFFFFFFFF` name sentinel.
   **Fix:** `va >= g_rdHi - 48`.
3. String-read in `Record` walked off a page (a value mis-tagged `tt==4`). **Fix:**
   VirtualQuery-guard the string deref, cap to the committed region.
4. `EmitRec` used `%lld` (unsupported by `wsprintfA`) → invalid JSON. **Fix:** `%I64d`.
5. A `FormatRec` refactor made `sizeof(line)` = `sizeof(ptr)` = 4, truncating multi-arg
   calls to arg[0]. **Fix:** `#define LINE_CAP 1024`.
6. **[Jen-specific] Unguarded `L->top`/`L->base` read** (AV, was Record+0xA1): a garbage
   `lua_State*` passed the loose range test, and reading `L+8` faulted. **Key repro:** the
   AV was consistent when the user selected **Jen** (Jennifer Mui), never Mattias — Jen's
   selection/spawn Lua (`mrxguishell.lua`) calls a binding whose arg slot is an unmapped
   pointer. **Fix (sha `0E23904A`):** replace the range test with a `CommittedRange()`
   VirtualQuery-gated check (2-slot region cache keeps the hot path VQ-free) on the top/
   base reads AND the arg span; a garbage `L` now yields a benign record. Confirmed live:
   Jen runs, no crash. To NAME the culprit binding, an unmapped `L` is stamped `argc:-2`
   (sha `B8C99B6B`) → repro Jen, then `grep '"argc":-2' lua_trace.ndjson`.

**`argc:-1` explained:** ~56% of records (mostly `Printf` ×123k) are `argc:-1` because
`pmc_bb.dll` also hooks `Debug.Printf @0x6D5640` (double-hook) → our `[esp+4]` isn't a
clean `lua_State*` in the chained path. Benign — name + order are still captured; could
skip co-hooked fns later.

## Where it fits

This is the **oracle half of Phase 0** of the modernization program. Per the locked order:
Engine Skeleton (done — see `ecs_core_spine.md` / `world_terrain_loader.md`) → **Surface A
oracle dump** → Lua 5.4 migration spike. Relates to `modernization/00_charter.md`.
