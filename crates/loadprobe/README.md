# loadprobe

World-load progress and forensic analyzer for Mercenaries 2's `pmc_blackbox.log` — it scores how far a
run's world-load actually got and classifies how it ended.

## What it is

A single binary (`loadprobe`). Point it at a `pmc_blackbox.log` and it:

* **Scores the load against a milestone ladder** — 21 ordered phases (0 `Process init` → 20
  `World fully loaded (GlobalExit)`), each matched by substrings in the log's message text. The
  furthest phase reached becomes a `LOADED N%` headline. `--milestones` prints the whole ladder.
* **Classifies the end-state** into exactly one verdict, which is also the process exit code:
  * `ReachedWorld` — exit **0** (GlobalExit complete)
  * `Crash` — exit **10**
  * `Hang` — exit **11**
  * `Truncated` — exit **12**
* **Knows which crashes are not crashes.** `phases::KNOWN_EIPS` maps faulting EIPs to a subsystem
  label, and flags some as `teardown` — e.g. `0x00874E7D` (texture-streaming worker faulting on
  process teardown) is a hard-close artifact, so it never becomes a `CRASH` headline. A crash after
  phase 20 is reported as a *post-load* crash and the run still counts as REACHED-WORLD. An EIP that
  isn't in the table is loudly called out as an `UNRECOGNIZED` (candidate new) crash site.
* **Dumps forensics**: the phase timeline with inter-phase deltas, WAITFORSTREAMING enter/exit cycles
  + max refcount, progression (Staging Act / `CreatePlayerCharacter` / portal enables / faction
  job+contract module imports), texture-component pool health from the `[cc]`/`[pool]` sources
  (distinct hashes vs pool cap, insert callers, GARBAGE keys, free min/final, BURST/REFILL),
  `[mtrl] OVERCOUNT` lines, `[stall]` dumps, high-signal Lua markers, the largest inter-line time
  gaps, and the end-of-log tail (with steady `[pool] free=` polls collapsed).
* **Binds a run to its bytes.** It SHA-256s the log it read, and parses pmc_bb's
  `[blackbox] BUILD <kind>=<name> sha256|qsha256=<hex> size=<n>` lines into a BUILD/RUN IDENTITY
  block (exe / asi / pmc_bb.dll / WAD). `qsha256` is the head+tail+size quick hash used for files
  over 1 GiB. No BUILD lines ⇒ it warns that the run is not self-attributing.
* **Refuses to silently pass unknown content**: any source tag not in `phases::KNOWN_SOURCES` is
  surfaced as `UNKNOWN SOURCE`, and lines that don't match `[ts] [source] msg` are counted as
  unparsed raw continuations.
* **Optionally symbolizes the crash block** (`--symbolize`): rewrites `module+0xOFFSET` tokens into
  `= function+0xN`.

## Where it comes from

The log format is the one emitted by `tools/pmc_blackbox/pmc_blackbox.c` (`pmc_log`):
`[HH:MM:SS.mmm] [source] message`. Lua lines come from `tools/pmc_blackbox/lua_log_hook.c` and carry a
trailing `  @script:line`; `[world]` echoes are prefixed `>>> `. The crash block is what
`tools/pmc_blackbox/crash_handler.c` writes, which reports every frame as `module.dll+0xOFFSET`.

The milestone ladder's order is the observed chronology of a deep load, taken from the real capture
`storage/pmc_blackbox-vanilla-boot-into-game.log` (the GlobalEnter entity-construction burst fires
before mission-flow + the WAITFORSTREAMING layer cycles, which precede the portal enables and
GlobalExit).

Symbolization resolves against two sources, offline and never in the crash path (the game's handler
stays allocation-free):
* our own modules (`lua_trace.asi` and other un-stripped `.asi`/`.dll`) still carry a COFF symbol
  table, parsed directly by `symbolize.rs` — no `nm` and no external crate;
* `Mercenaries2.exe` frames resolve against the curated VA→name map in
  `scripts/mercs2_annotations.json` (image base `0x00400000`). That map is sparse, so a "nearest"
  match further than 0x4000 bytes away is dropped rather than mislabelled.

The `KNOWN_EIPS` labels are the accumulated findings of this project's crash work (MTRL 128-byte
record size, `Mtrl_Parse` = `FUN_00858790`, the shader-registry NULL slot in the draw loop
`FUN_00855420`, the null-DXT1 texture BODY deref, …) — each entry's comment in `src/phases.rs` states
its own evidence.

## Usage

```bash
# analyze the deployed game's log (the default path is baked in)
cargo run -p loadprobe

# a specific capture, machine-readable
cargo run -p loadprobe -- --json --no-color storage/pmc_blackbox-vanilla-boot-into-game.log

# print the milestone ladder and exit
cargo run -p loadprobe -- --milestones

# name the frames in the [crash] block
cargo run -p loadprobe -- --symbolize "C:/.../Mercenaries 2 World in Flames/pmc_blackbox.log"

# branch on the verdict in a script (0 = reached world, 10 crash, 11 hang, 12 truncated)
loadprobe --no-color mylog.log; echo "verdict exit = $?"
```

Flags (all optional):

| flag | default | meaning |
|---|---|---|
| `[LOG]` (positional) | the deployed game's `pmc_blackbox.log` | log file to analyze |
| `--routine` | `lua,pool` | comma-separated sources treated as routine (suppressed from the flagged-source dump) |
| `--hang-secs` | `10` | seconds of no progress (steady pool polls) before classifying a HANG |
| `--top-gaps` | `5` | how many largest inter-line time gaps to report |
| `--signals` | `###!,###,!!!,##@,@@@,***,=-=` | high-signal Lua message prefixes |
| `--json` | off | emit the whole `Report` as JSON instead of the text dump |
| `--no-color` | off | disable ANSI colors |
| `--milestones` | off | print the milestone ladder and exit |
| `--symbolize` / `-S` | off | resolve `module+0xOFFSET` in the `[crash]` block |
| `--exe-symbols` | `scripts/mercs2_annotations.json` | curated exe VA→name map for `--symbolize` |
| `--module-dir` | — | extra dir to search for `.asi`/`.dll` (repeatable); the log's dir and its `scripts/` are always searched |

## Modules

Binary-only crate; the modules are internal (declared in `src/main.rs`).

* `parse` — `pmc_blackbox.log` line parser → ordered `LogLine`s (timestamp with midnight-wrap
  correction, source tag, message, Lua `@script:line`, `>>> ` world echo, file line number).
* `phases` — the static tables and the single place to extend: the milestone `LADDER`,
  `KNOWN_SOURCES`, `INSTRUMENTATION` / `INIT_NOISE` source classes, `KNOWN_EIPS` (+ teardown flags),
  and the faction job/contract module-name test.
* `report` — the analysis (`analyze`) that turns lines into a `Report`, plus the colored text dump
  and the `Serialize` JSON form.
* `symbolize` — COFF symbol-table reader for un-stripped `.asi`/`.dll` + the exe annotation map;
  rewrites crash-block lines.
* `sha256` — dependency-free FIPS 180-4 SHA-256 used to fingerprint the analyzed log.

`tests/fixtures.rs` runs the built binary with `--json` against four real captures in `storage/` and
locks in their verdicts (reached-world / hang / hang / parses). Those fixtures live outside the crate;
the tests skip gracefully if they're absent.

## Notes / gotchas

* **Do not eyeball the log.** The classifier deliberately disagrees with a naive read: the trailing
  `0x874E7D` fault at the end of many logs is a *hard-close teardown artifact*, not a load bug.
* A HANG is only declared if progress stopped for `--hang-secs` **and** the tail after the last
  `[lua]`/`[world]` line is pool-dominated (more than half the remaining records are `[pool]`).
* The phase for "World entities online" is keyed on content (`Enabling …` + `portal`), not on a
  hardcoded ladder index, so reordering the ladder can't break it.
* `--symbolize` is post-analysis by design; if it finds no symbol source (no exe map, no `.asi`/`.dll`
  next to the log) it warns to stderr and leaves the crash block untouched.
* The default log path is a hardcoded absolute path to this developer's install
  (`C:/Users/Shadow/Desktop/Mercenaries 2 World in Flames/pmc_blackbox.log`); pass the log explicitly
  anywhere else.
