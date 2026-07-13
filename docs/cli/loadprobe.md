# loadprobe

Scores how far a Mercenaries 2 world-load got by grading `pmc_blackbox.log` against a
20-step milestone ladder, then classifies the end-state (REACHED-WORLD / CRASH / HANG /
TRUNCATED) and dumps a forensic report — phase timeline, streaming cycles, texture-pool
health, crash decode, high-signal Lua markers, and a build/run identity fingerprint. The
verdict is also the process exit code, so scripts can branch on how the load ended without
re-parsing the output.

## Synopsis

```
loadprobe [OPTIONS] [LOG]
loadprobe --milestones
```

`LOG` is the log file to analyze. When omitted it defaults to the deployed game's log at
`C:/Users/Shadow/Desktop/Mercenaries 2 World in Flames/pmc_blackbox.log`.

## Options

| Flag | Value | Default | Required | Repeatable | Effect |
| --- | --- | --- | --- | --- | --- |
| `[LOG]` | path (positional) | the deployed game's `pmc_blackbox.log` (see Synopsis) | no | no | The log file to analyze. If it cannot be read, loadprobe exits `2`. |
| `--routine` | comma-separated source tags | `lua,pool` | no | no (comma-list, not multi-flag) | Source tags treated as routine and **suppressed** from the FLAGGED SOURCES dump. The `raw` (continuation) source is always skipped regardless. |
| `--hang-secs` | integer (u64, seconds) | `10` | no | no | Minimum steady-with-no-progress duration to classify a HANG. A load is only a HANG if the gap between the last `[lua]`/`[world]` line and the last log line is ≥ this many seconds **and** the tail is pool-dominated. |
| `--top-gaps` | integer (usize) | `5` | no | no | Number of largest inter-line time gaps to report in the LARGEST TIME GAPS section. |
| `--signals` | comma-separated prefixes | `###!,###,!!!,##@,@@@,***,=-=` | no | no (comma-list, not multi-flag) | Message prefixes that mark a `[lua]`/`[world]` line as a high-signal marker. Only these prefixes populate the HIGH-SIGNAL LUA MARKERS section. |
| `--json` | flag | off (text dump) | no | no | Emit the full `Report` as pretty JSON instead of the colored text dump. |
| `--no-color` | flag | off (colored) | no | no | Disable ANSI colors in the text dump. No effect on `--json` (JSON is never colored). |
| `--milestones` | flag | off | no | no | Print the milestone ladder (index / name / match substrings) and the reached-world index, then exit `0`. The log is never read. |
| `-S`, `--symbolize` | flag | off | no | no | Rewrite `module+0xOFFSET` tokens in the `[crash]` block into `= function+0xN`, using the exe VA→name map plus the un-stripped COFF symbols of any `.asi`/`.dll` found near the log. Only touches the crash block; a run with no crash is unaffected. |
| `--exe-symbols` | path | `C:/Users/Shadow/Desktop/notes-on-the-released-game/scripts/mercs2_annotations.json` | no | no | The curated `Mercenaries2.exe` VA→name JSON consulted by `--symbolize` for `.exe` frames. Only read when `--symbolize` is set. |
| `--module-dir` | path | none | no | **yes** (pass multiple times) | Extra directory to search for `.asi`/`.dll` module files during `--symbolize`. The log's own directory and its `scripts/` subdir are always searched in addition. Only used when `--symbolize` is set. |
| `-h`, `--help` | flag | — | no | no | Print help and exit. |

### Verdict → exit code

The verdict doubles as the process exit code:

| Exit | Verdict | Meaning |
| --- | --- | --- |
| `0` | REACHED-WORLD | `GlobalExit - Complete` fired; the world fully loaded. A post-load crash (including a hard-close teardown) is noted but does not demote this. |
| `10` | CRASH | A real (non-teardown) terminal crash before the world finished loading. |
| `11` | HANG | Load wedged: no `[lua]`/`[world]` progress for ≥ `--hang-secs`, tail pool-dominated. |
| `12` | TRUNCATED | Log ends mid-load with no crash/hang signature (or a mid-load hard-close). |
| `2` | — | Could not read the log file, or a JSON serialization error. |

## How the options combine

**`--milestones` short-circuits everything.** It is handled before the log is even read: it
prints the ladder and returns `0`. Combined with any other flag (including a `LOG` argument
or `--json`), those other flags are ignored. Note it runs *before* `--no-color` is applied,
but its output contains no color anyway.

**`--json` and the text dump are the two mutually exclusive output forms.** `--json`
serializes the entire `Report` (every field the text dump summarizes, plus fields the text
dump caps or omits — e.g. all signal markers, all build artifacts, full pool caller lists).
`--no-color` only styles the text dump, so with `--json` it is a no-op. The verdict and thus
the exit code are identical whichever form you choose — the output format never changes the
classification.

**`--symbolize` gates `--exe-symbols` and `--module-dir`.** The two path options are only
consulted when `--symbolize` is set; on their own they do nothing. Symbolization runs
*after* analysis and *only* rewrites the lines inside the detected `[crash]` block, so:

* If the run has no `[crash VEH EXCEPTION]` line, `--symbolize` (and its path options) change
  nothing.
* Because it mutates the crash block in the `Report` before output, `--symbolize` affects
  **both** the text dump and `--json` — the resolved `= function+0xN` suffixes appear in the
  crash-block lines of either form.
* Resolution uses two independent sources: `.exe` frames resolve against `--exe-symbols`
  (queried as image-base `0x00400000` + offset, dropped if the nearest name is farther than
  `0x4000`); `.asi`/`.dll` frames resolve against that module's own COFF symbol table found
  in a search directory (dropped if farther than `0x8000`). The search directories are the
  log's parent dir, that dir's `scripts/` subdir, and every `--module-dir` — in that order.
* If neither source exists (missing exe map and no `.asi`/`.dll` next to the log),
  `--symbolize` prints a warning to stderr and leaves the crash block untouched.

**`--hang-secs` is the only option that can change the verdict (and exit code).** A larger
value makes HANG harder to declare: a wedge shorter than the threshold falls through to
TRUNCATED instead. It has no effect once the world fully loaded (REACHED-WORLD wins) or when
a blocking crash is present (CRASH wins) — the classifier checks blocking-crash, then
reached-world, then hang, then truncated in that order.

**`--routine` and `--signals` only reshape two report sections; they never touch the
verdict.** `--routine` controls which sources are hidden from FLAGGED SOURCES (default hides
the high-volume `lua` and `pool` streams so instrumentation like `[crash]`/`[mtrl]`/`[stall]`
stands out). Clearing it (`--routine ""`) surfaces every source including the `lua` flood.
`--signals` controls which Lua-line prefixes are collected as high-signal markers; an empty
value yields an empty markers section. Neither changes phase scoring, crash detection, or
pool health — those key on message content, not on these lists.

**`--top-gaps` only sizes the LARGEST TIME GAPS list**; `0` produces an empty section.

## Examples

Analyze the deployed game's log with the default report:

```
loadprobe
```
Reads the default log path, prints the colored `LOADED N%` banner, verdict, phase timeline,
pool health, and tail. Exit code encodes the verdict.

Analyze a captured log and decode any crash frames:

```
loadprobe --symbolize "storage/pmc_blackbox-vanilla-boot-into-game.log"
```
Same report for the named log, with each `module+0xOFFSET` line in the `[crash]` block
annotated `= function+0xN` where a symbol source resolves it.

Symbolize using an extra module directory and a specific exe map:

```
loadprobe -S --module-dir mods/lua_trace_asi --module-dir build/out \
  --exe-symbols scripts/mercs2_annotations.json crash_run.log
```
Resolves `.asi`/`.dll` frames against COFF symbols found in `mods/lua_trace_asi/`,
`build/out/`, and next to `crash_run.log`; resolves `.exe` frames against the given map.

Machine-readable output for a CI gate:

```
loadprobe --json run.log > report.json ; echo $?
```
Emits the full `Report` as JSON and exits `0`/`10`/`11`/`12` so a script can branch on the
verdict. (`--no-color` is unnecessary here — JSON is never colored.)

Treat a shorter stall as a hang and widen the gap list:

```
loadprobe --hang-secs 5 --top-gaps 10 run.log
```
Classifies a ≥5s no-progress pool-dominated tail as HANG (exit `11`) and reports the ten
largest time gaps.

Print the milestone ladder without analyzing anything:

```
loadprobe --milestones
```
Lists all 21 ladder phases (0–20) with their match substrings and the reached-world index,
then exits `0`.

Surface every source (including the routine `lua`/`pool` flood):

```
loadprobe --routine "" run.log
```
The FLAGGED SOURCES section now includes `lua` and `pool` instead of hiding them.

## Failure modes

* **`loadprobe: cannot read <path>: <err>`** — the `LOG` path (or the default deployed-game
  path) could not be read. Exit code `2`. Check the path, or pass an explicit `LOG`.
* **`loadprobe: --symbolize found no symbol sources (exe map <path> and no .asi/.dll next to
  the log)`** — `--symbolize` was requested but neither the `--exe-symbols` JSON nor any
  `.asi`/`.dll` in the search directories exists. The crash block is left as raw
  `module+0xOFFSET`. Not fatal (exit code still reflects the verdict); fix by pointing
  `--exe-symbols` at a real map or adding a `--module-dir` that contains the module.
* **`loadprobe: json error: <err>`** — `--json` was set but the report failed to serialize.
  Exit code `2`. This is not expected in normal use.
* **`UNKNOWN SOURCE [x] ×N` / `N unparsed line(s)` in the COVERAGE / UNDETECTED section** —
  not an error and not a nonzero exit: loadprobe found a `[source]` tag it does not recognize
  (likely new instrumentation to teach `phases::KNOWN_SOURCES`) or lines that didn't match
  `[ts] [source] msg` (kept as `raw` continuation). It surfaces these so new content never
  passes silently.
* **`UNRECOGNIZED EIP` in a CRASH verdict** — a terminal crash whose EIP is not in the
  known-crash map. The verdict is still CRASH (exit `10`); the report flags it as a candidate
  new crash site to add to `phases::KNOWN_EIPS`.
