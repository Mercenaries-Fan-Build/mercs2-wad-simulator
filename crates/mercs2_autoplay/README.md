# mercs2_autoplay

Runs the shipped Mercenaries 2 game Lua headless, the way a player's session runs it, and records every
call it makes.

## What it is

A library + CLI over the engine's own script host (`mercs2_engine::script_host`). It boots a new game
through the real `vz` master script against the real world data from `vz.wad` — the same sequence
`mercs2_game` runs at world setup, minus everything that draws — and records the session as NDJSON.

* `boot` — `boot_new_game(vz_wad, hero, trace_out)`: reads the named markers, landing pads and layer
  index from `vz.wad`, realizes them in the order the game does, installs the recorder, runs
  `run_boot_flow`, and returns a `Session` only once the world load reached `GlobalExit`. Missing world
  data or an unfinished load is an error; there is no world-less mode.
* `tasks` — `snapshot(&session)`: the live `MrxTask` tree (every mission / briefing / objective node and
  its state), read from `MrxMissionFlow._oParent` with the task framework's read-only accessors.
* `trace` — the recorder. Three record kinds in one stream:
  * `engine` — every engine binding call (`ns`, `fn`, `caller_src` `@module:line`, `args`), in the field
    shape of `mods/lua_trace_asi`'s retail `lua_trace.ndjson` so the two can be diffed;
  * `lua` — every Lua call that crosses a module boundary (`caller_src`, `callee`, `fn` defined-as name,
    `called_as`, `via` C frames, `args`); an empty `caller_src` is a callback the engine fired;
  * `stub` — every touch of an engine name with no binding at all (the auto-stub layer).

  Arguments are encoded in full: tables one level deep with sorted keys (32 entries kept, `n` = full
  count), functions as their definition site, guids as hex, and Lua `tostring` addresses masked to
  `<addr>`.

## Usage

```
cargo run -p mercs2_autoplay -- boot --trace out.ndjson [--hero mattias|jennifer|chris] [--vz-wad <path>]
```

Without `--vz-wad` the archive comes from `MERCS2_GAME_DIR`, then `VZ_WAD` (each the install root, its
`data` folder, or the archive itself), then the first `.mercs2-local.toml` at or above the working
directory whose `vz_wad` key names an existing file (write it with `scripts/find-vz-wad.sh --write`, run
from `tools/wad_simulator`).
`MERCS2_VZ_WAD` is not read here. Exit codes: 0 booted, 1 the boot ran but did not finish, 2 could not
run.

The game-gated tests (`retail` feature, `cargo xtask retail-test`) pin the boot to the first objective,
pin each record kind to a shipped source line, and require two boots to write byte-identical traces.

## Notes / gotchas

* **Determinism is load-bearing.** The explorer this crate is built for replays and forks sessions, so
  the same inputs must give the same calls. Two engine sources of run-to-run drift were fixed for it
  (named markers spawned in `HashMap` order; HUD widget callbacks dispatched in `HashMap` order), and
  `mercs2_script`'s clock and RNG are per-VM and tick-driven. A new determinism failure means a new
  unordered container or wall-clock read somewhere under the boot — find it, don't mask it.
* The Lua→Lua half is a `debug.sethook` call hook, which is per Lua thread. The shipped corpus uses no
  coroutines, so the main thread sees every call.
* Recording costs ~3.5 s and ~13 MB per new-game boot. Encoding a second table level multiplied both by
  ten, which is why tables stop at one level.
