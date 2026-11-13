# Missions — behaviour contract (Debug run)

The design is the `MissionsDebug` board of the Workshop Polish Pass canvas (`MissionsDebug.dc.html`,
"Missions · Debug run · gurcon002 Start()"), next to the `Missions` board (Blueprints · PmcCon016), and the
`MissionsLibrary` board (`MissionsLibrary.dc.html`, "Missions · Library script in a run · mrxutil
SpawnObject()") for a run that watches a library script beside a contract. The
Missions page already lifts a script into a node graph and edits it (`crates/mercs2_workshop/src/missions.rs`,
`blueprint.rs`). Debug mode lays a run of that script over the same graph: what ran, which branch was
taken, how often, with which arguments, and where a breakpoint stopped it.

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
what is missing, and what proves it. Nothing is drawn without its row being true. No value is copied
from the mockup. A row that names a decision (D1…D7) follows the decision recorded below.

Grades: PROVEN (read in code, shipped Lua or the decomp, or measured), INFERRED, UNPROVEN.

## Decisions

| | Decision | Source |
|---|---|---|
| D1 | The debugger records **whatever run is attached to the bridge**: the retail game (the `lua-bridge` Shipment on m2-sdk.dll), our engine (`mercs2_game`), or `mercs2_autoplay`. All three speak one debug protocol on the bridge port; the Workshop does not know or care which one answered beyond labelling it | user, 2026-11-05 |
| D2 | Breakpoints **pause the live game**, and only with a way for the player to resume that the player can see | user, 2026-11-05 |
| D3 | The graph's line space is the decompiled corpus's (the text the Blueprint lifts). A retail position is translated by the instruction-exact line map (§Line identity); a script with no map has no overlay on a retail run, and says so by name | design |
| D4 | **Preview run** of a Depot draft is either an autoplay run with the draft applied, which reaches the mission by playing, or a retail / `mercs2_game` run started from a save state autoplay discovered, with the draft installed as a preview overlay | user, 2026-11-05 |
| D5 | Autoplay never jumps to a mission. It plays through and discovers save states; that is what it is for | user, 2026-11-05 |
| D6 | A recording names, per module, the bank and the hash of what ran. The overlay refuses a recording whose stamp does not match the graph on screen | design |
| D7 | Step, step back and the playhead move through the recording. On a live paused run, Continue and Step are sent to the paused host; step back only moves the playhead, never the game | design |

## Facts the design stands on

| Fact | Evidence | Grade |
|---|---|---|
| The graph is lifted from decompiled source text with full_moon; nothing in the lift reads bytecode | `lua_source.rs:47-61`; `blueprint::lift` `blueprint.rs:503`; `missions::pick` `missions.rs:411-425` | PROVEN |
| `Node.line` is the 1-based start line of the node's statement in that text; there is no end line, and no module identity on `Node` or `Graph` | `blueprint.rs:133-159`, `line_of` `:2891-2893`, `Graph` `:112-116` | PROVEN |
| An if/elseif/else is one node with an exec output per arm; the AST of each arm is kept in the node's private `form` | `if_stmt` `blueprint.rs:875-902`; `Form::If` `:244-250` | PROVEN |
| Exec and data wires carry no line | `ExecWire` `blueprint.rs:120-123`, `DataWire` `:127-130` | PROVEN |
| The canvas is a custom egui painter: wires first, then nodes with a `state` colour | `missions.rs:1495` `canvas`; wires `:1657-1669`; node state `:1681-1687`; `paint_node` `:1874-1942` | PROVEN |
| `mercs2_game` and `mercs2_autoplay` run the decompiled corpus text, so their line numbers are the graph's | `script_host.rs:1838-1839` (`ScriptHost::new(corpus::roots())`); `world.rs:1107` | PROVEN |
| Retail reports the original author's lines, which differ from the decompiled lines (`mrxmissionflow:201` vs `.lua:132`) | `docs/reverse_engineer/mission_progression_graph.md` §5.1 | PROVEN |
| Our compile of `allcon001`'s decompiled text and retail's chunk differ only in line info | `mercs2_luac/tests/parity_retail.rs` `the_differences_from_retail_are_confined_to_line_number_info` | PROVEN for that script |
| The retail VM still runs the line/count hook: `luaV_execute` tests `L+0x36 & 0xC`, decrements `L+0x3C`, and calls the trace step; the trace step reads `lineinfo` and calls the hook; the hook caller reads the hook at `L+0x40` and `allowhook` at `L+0x37`. The `debug` library is absent | pc-v10 `FUN_00866910`, `FUN_00865ba0`, `FUN_00868290`; `luaD_precall` `FUN_00868610` tests `L+0x36 & 1` | PROVEN (fields); that an ASI can install a hook by writing them is INFERRED |
| Retail's `lua_State` puts `hookmask` at `+0x36`, two bytes before 5.1.5's layout (no `baseCcalls`); the vendored `lstate.h` offsets do not apply | vendored `lstate.h:114-120` vs the decomp above | PROVEN |
| Our host opens the `debug` library; the Rust FFI declares `lua_getinfo` and `lua_getstack` but not `lua_sethook` | `mercs2_luac/src/rt/mod.rs:132-142`; `sys.rs:251-252` | PROVEN |
| A state has one hook slot; a second `sethook` replaces the first | vendored `ldebug.c:56-65` | PROVEN |
| Hooks do not fire inside a hook (`allowhook = 0` while it runs), so chunks run while paused cannot re-enter the debugger | vendored `ldo.c:194`; retail `FUN_00868290` clears `L+0x37` around the call | PROVEN |
| No coroutine is used by the shipped scripts; everything runs on the main state | `grep coroutine.` over `mercs2_script/corpus`: no match | PROVEN |
| The bridge is ours: `shipments/mercs2-lua-bridge`, a raw REPL plus a WebSocket with a typed broadcast feed on `127.0.0.1:27050`. It hooks nothing; m2-sdk.dll owns the detours, the captured states, the game-thread tick and the chunk executor | its `README.md`, `PARITY.md`; `mercs2_mod_runtime/src/lua_host.rs:1-15` | PROVEN |
| Bridge chunks run on the game thread at the next pump, so a hook that blocks waiting for a chunk would wait forever | bridge README §The REPL; `pump.rs` | PROVEN |
| `mercs2_game` serves the raw REPL only (no WebSocket), and replies `tostring` of the first value | `mercs2_game/src/bridge_host.rs:47-65`; `world.rs:2025-2054` | PROVEN |
| `mercs2_autoplay` writes NDJSON (`engine`, `lua`, `stub`) through a call-only hook, drops calls inside one module, and serves no bridge | `mercs2_autoplay/src/trace.rs:136-169,154`; its `Cargo.toml` | PROVEN |
| A spliced `patch_lua` append keeps its own chunk's source name: the appended main function is copied whole as a new child of the base's | `mercs2_luac/src/splice.rs:6-17,178` | PROVEN |
| Retail calls the hook as cdecl `hook(L, &ar)`; `luaD_callhook` itself takes L in EAX, the event in EDX and the line on the stack. `ar.event` is at +0, `ar.currentline` at +0x14, `ar.i_ci` at +0x60 = `(ci - base_ci) / 24` (0 for event 4). Events: 0 call, 1 return, 2 line, 3 count, 4 tail return (numbers from the call sites; names from stock 5.1) | pc-v10 disassembly `0x868290`-`0x86835E`; call sites `0x865BCA`, `0x865C22`, `0x8687A3`, `0x8687FF`, `0x868830` | PROVEN |
| `luaV_execute` hands traceexec the pc one instruction past the one executing, and stores it as `L->savedpc` (+0x18) | `0x866943`-`0x86696D`, `0x865BB0` | PROVEN |
| The VM is Lua 5.1.2 | Xbox PDB paths `Lua-5.1.2\src\lundump.c` (`docs/mercs2-pdb-analysis/lua-scripting.md`) | PROVEN |
| Retail `Proto`: `k` +0x08, `code` +0x0C, `p` +0x10, `lineinfo` +0x14, `source` +0x20, `sizek` +0x28, `sizecode` +0x2C, `sizelineinfo` +0x30, `sizep` +0x34, `linedefined` +0x3C, `lastlinedefined` +0x40, `nups`/`numparams`/`is_vararg`/`maxstacksize` +0x48..+0x4B | LoadFunction `0x870F00`, LoadCode `0x8707F0`, LoadConstants `0x8708D0` | PROVEN |
| A running function's module name is its `Proto.source`, the chunk's embedded source name (`luaU_undump` `0x8711D0` passes `"=?"` only as the fallback) | `0x8711D0`; `lua_trace_asi` reads the same field | PROVEN |
| Retail's call hook for a Lua function is reached: `luaD_precall`'s Lua branch `jmp [0x244F688]` at `0x868653` goes to `0x4F1451`, SecuROM's relocated `lea edx,[eax*8]`, which jumps back to `0x86865A` | pc-v10 dump bytes and disassembly | PROVEN |
| `luaU_undump` `0x8711D0` is cdecl `(L, z, buff, name)` with a plain `ret`, returning the main `Proto*` in EAX; its only reference is the `mov eax, 0x8711D0` at `0x868BFD` | disassembly `0x8711D0`-`0x87123C`, `0x868BFA`-`0x868C28`; whole-dump scan | PROVEN |
| `luaD_poscall` is `0x868850`: L in ECX, the first result in EAX, plain `ret`; it calls the return-hook helper `0x8687F0` when `hookmask & 2`. Its callers: `0x86790E`, `0x8687D9` (C functions: first result = `L->top - n*8`), `0x86898C` | disassembly | PROVEN |
| Engine binding names: `mercs2_exemap luareg` on the shipped SecuROM-wrapped exe on disk gives output byte-identical to the unpacked v1.1 export (sha256 `209cae52…`); every reference build carries the same 62 tables (31 named, 1,375 entries) at its own addresses | run 2026-11-05; `lua_sites_exes` `every_reference_build_names_its_engine_bindings` | PROVEN |
| A C function is called through `closure+0x10`: `luaD_precall`'s C branch `mov eax,[edx+0x10]; call eax` (`0x8687B3`) | disassembly | PROVEN |
| Every shipped chunk embeds its bare container name as its source (`gurcon002`, `vz`, `mrxguimanager`: no `@`, no `.lua`), so that is a retail position's module | decoded from the gate's blocks, 2026-11-05 | PROVEN |
| 16,388 of the 106,468 corpus lines with code never begin a run of one original line, so retail's own line hook never stops on them (counting line changes only; jump targets reduce the true number) | measured over the gate's 369 scripts, 2026-11-05 | PROVEN |

## Line identity

| Element | Behaviour | Data / code | Missing | Proof |
|---|---|---|---|---|
| Line map | For one module: decode retail's chunk and our compile of the decompiled text and walk both function trees together. In a function with the same code, constants and shape, instruction *i* gives (retail line, corpus line). A function that differs is unmapped: a position in it is an error naming the function, both line ranges and the first difference, and the rest of the module still maps. A tree that cannot be paired (different nested-function counts) refuses the module | `mercs2_formats::luachunk::decode` | — | `mercs2_formats::lua_linemap` (`LineMap::build`), unit tests |
| Module pairing | A corpus file pairs with the retail script of its name; a file whose name no script has pairs with the one left-over script with identical code. Two candidates is an error naming both | — | — | `lua_linemap::pair_modules`, unit test |
| One file's script | The Workshop and the probe pair one corpus file alone (`lua_linemap::shipped_for`): the shipped script of its name, else the one script with its code that no corpus file of the bank is named after. The map names the module by the shipped chunk's source, the name a live hook reports: `resident/Init.lua` runs as `c4`, `vz/xQ!L.lua` as `vz` | the bank's shipped scripts (`shipped_scripts`), the bank's corpus file names | — | `one_corpus_file_finds_its_shipped_script_by_name_then_by_code`; the parity gate checks it gives the bank-wide pairing for every pair (all 369, 2026-11-06) |
| Function identity | A retail event names its function by (line defined, last line defined, instruction count) in the original line space, plus a 0-based instruction index. The map resolves that to one function, or errors when two functions share the key and their corpus lines differ | — | — | `LineMap::function`, test with two same-line closures |
| Breakpoint positions | A breakpoint on a corpus line becomes every (function, instruction) on that line; an unmapped function holds none | — | — | `LineMap::positions`, test |
| Parity gate | Every corpus file is paired with the block it was decompiled from (`vz.wad` `resident`, `scripts_vz`; `shell.wad` `resident`) and mapped; the test names every file that pairs with nothing and every function that does not map | retail blocks, corpus | the DLC blocks | `mercs2_luac/tests/line_map_retail.rs` against the real `vz.wad` |
| Stripped chunks | A debug-stripped retail chunk still carries line-defined and the code, so its positions map through our compile's lines | — | gate over the DLC blocks | `a_stripped_retail_chunk_still_maps` |

Gate result, 2026-11-05 (`vz.wad`/`shell.wad` beside `.mercs2-local.toml`'s `vz_wad`): **369 scripts, 6,396
functions map**; `resident/Init.lua` is retail `c4` and `vz/xQ!L.lua` is retail `vz`, both paired by identical
code. Three corpus files differ from what shipped, so the gate fails on them:

| Corpus file | Difference from retail | Grade |
|---|---|---|
| `resident/Multi.lua` (retail `fx` has the same functions) | function `[0]`, constant 2: the `Debug.Printf` long string starts with a newline retail's does not have | PROVEN (constant compared) |
| `resident/mrxguihudfactiongauge.lua:193-269` `SetValue` | instruction 78, a jump: the corpus's `if bInitialize then repeat … do return end do break end -- pseudo-goto …` puts the level-transition code after an unconditional `return`, where it can never run | PROVEN (instruction compared); what retail's control flow is, INFERRED |
| `vz/oilcon001.lua:2115-2124` `_RabbitStart` | 29 instructions where retail has 26, differing from instruction 5: the corpus's `local res = not uDriver or … or Ai.Goal{…}` is not what was compiled | PROVEN (instruction compared) |

The corpus is not edited to clear these (it changes only by regenerating it to drop decompiler tokens that
do nothing). On a retail run the three functions are unmapped: a position in one is refused with its reason,
and the rest of each module maps.

## Debug protocol

Defined once in `mercs2_formats::lua_debug`; every host and the Workshop use that module, not their own copy.
JSON text frames on the bridge's WebSocket, each with a `type` starting `debug.`, beside the REPL's `{id, code}`
requests (no `type`) and its `log`/`ws`/`ack` frames. The message list is the module's doc.

| Rule | Behaviour | Proof |
|---|---|---|
| Routing | A transport routes each client frame by `lua_debug::Inbound::classify`: a JSON object with a `type` is the debugger's, anything else the REPL's (a REPL `code` may carry raw CP1252 bytes, which are not JSON). A typed frame that is not a valid debug request is answered `debug.refused` when it has a string `id`, else only logged | `inbound_frames_route_by_type`; bridge `debug_frames_the_debugger_does_not_take_are_refused_here` |
| Retail transport | The lua-bridge hands each debug frame to `m2_lua_debug_request` with the WebSocket client's id, and m2-sdk.dll's sink queues every answer and event on that client's outbox alone (`WsHub::send_to`); a closed, unknown or over-backlog client makes the sink return 0. The bridge tells m2-sdk.dll when a client's connection ends, from the connection's thread, never from the sink | bridge `debug_frames_reach_the_debugger_and_its_answers_reach_only_their_client`, `the_debug_sink_drops_a_client_whose_backlog_is_full` |
| Frame size | Every debug frame is at most `lua_debug::MAX_FRAME_BYTES` (1,048,575), the bridge's WebSocket message limit (`ws::MESSAGE_BUFFER`); a larger message is a protocol error that closes the connection. Measured over the 370 corpus modules our compiler builds, one frame carrying a module's line tables is at most 98,542 bytes (`vz/wifbriefingdata`, 18,401 instructions; median 1,799), and one carrying every module's would be 1,764,394 bytes (272,874 instructions), so line tables travel in `debug.lines` frames packed under the limit by `lua_debug::lines_frames` (user, 2026-11-05) | measured 2026-11-05; `line_tables_pack_into_frames_under_the_limit_in_order` |
| Versions | `debug.hello` and `debug.attach` carry `protocol` (2: engine breakpoints, `debug.bindings`, `debug.watch`); a host refuses another version, and the client refuses a `debug.host` with another. Nothing is half-served | `another_protocol_version_is_refused` |
| Engine breakpoints | `debug.attach` and `debug.breakpoints` carry `engine`, binding names (`Pg.Spawn`). A call of one from any module (filtered or not) is reported as `debug.engine` with its arguments, then pauses (`debug.paused`, reason `engine`) at the calling instruction, before the binding runs; its results arrive as `debug.engine_return` after the resume. A name the host does not have refuses the request, naming it. An attach names at least one module or one engine breakpoint. Paused in a module the attach does not filter, a step is refused (no line event is raised there to stop on): Continue | `engine_breakpoints_name_bindings_the_host_has`, `an_engine_call_reports_from_a_filtered_module_and_pauses_from_any`, retail `an_engine_breakpoint_pauses_a_call_from_any_module_before_the_binding_runs`; live 2026-11-06, v1.1 exe under Wine, in the world: `mercs2_probe lua-debug --engine Player.GetControlledObject --then continue` attached with no module, paused before the call from `mrxguibase:3671` with its argument `0x40000014`, and after `debug.resumed` the call's `debug.engine_return` (`0x40005a77`) arrived, then a clean detach (exit 0) |
| Binding names (live) | live 2026-11-06: `debug.bindings` answered 1,003 names from the running v1.1 exe through lua-bridge 1.2.0 | `mercs2_probe lua-debug --bindings`, exit 0 |
| Library script in the Workshop (live) | live 2026-11-06 (user at the Workshop): a breakpoint in `mrxplayer` (a library script) `MedEvac`, reached by opening the main menu, paused the game at `mrxplayer:444` with the node lit on the canvas ("watching mrxplayer"); closing the Workshop released the pause | user report |
| Hook cost, a busy script watched | live 2026-11-06, in the world: watching `mrxguibase` (184 functions with line tables) the hook ran 174,085 times in 20 s for 10.0 s (57.6 µs each), about half the game thread; an engine-only attach ran it 24 times before its pause | `m2-sdk.log` "hook removed" |
| Binding names | `debug.bindings` is answered by `debug.names`: every binding the host can break on, sorted. On retail they are the namespaced `luaL_Reg` rows of the running executable (`mercs2_exemap::luareg`), read once on a transport thread; a host that cannot read them refuses with why | `engine_breakpoints_name_bindings_the_host_has` |
| Watch | `debug.watch` replaces the module filter of a running or paused attach: tables of modules it keeps stay, the client's staged tables are added (as an attach consumes them), and a module dropped and filtered again is reported again with its stamp. The host decides each hooked state's count hook again and reports each newly filtered module already loaded. Only the holder can watch | `watch_changes_the_filter_of_a_paused_run_and_keeps_the_tables_it_still_needs` |
| Requests | Every request carries an `id` and gets exactly one answer: `debug.host` (hello), `debug.ack` or `debug.refused` with the error | `every_request_round_trips` |
| One debugger | A host has at most one attached client. Another `debug.attach` is refused naming the holder's `client`; `debug.host.attached` names it to the run picker | host tests (steps 4–7) |
| Delivery | Events go only to the attached client, never through the bridge's broadcast. If that client's outbox overflows, the host sends `debug.detached` with the error and removes the hook; it never drops an event | host tests |
| No gaps | Every run event has a `seq` from 1 per attach, with no gaps; `lua_debug::Sequence` fails on the first missing one, and starts its count again after `debug.detached` (an attach's last event), so a client can attach again on one connection | `a_gap_in_the_events_is_an_error`, `the_count_starts_again_after_an_attachs_last_event`; `mercs2_bridge` `a_second_attach_on_the_connection_numbers_its_events_from_one` |
| Positions | `At` is the chunk name the host loaded the module under, the `FunctionKey` and instruction index, the host's line, and the frame's call depth, in the host's line space (`HostKind::line_space`: retail = original lines, `mercs2_game`/autoplay = corpus lines). The Workshop resolves a retail position through the `LineMap` of the shipped chunk and a corpus host's through `LineMap::build(ours, ours)`. The stack is cut to `depth` rather than counted from entries and returns: a tail call reuses its frame and an error caught by `pcall` unwinds frames with no return | `the_wire_text_is_pinned` |
| Line events | Before `debug.attach` the client stages a `LineTable` (corpus line per instruction) for every function of the filtered modules the line map holds, in as many `debug.lines` frames as they need; the attach consumes every table that client staged, whether it is accepted or refused, and a disconnect drops them. Every host reports each instruction a function with a table executes (on retail the count hook is switched on only while such a function runs, decided on every call and return), and `lua_debug::session::Debugger::instruction` raises `debug.line` by the VM's own rule on the corpus lines: the function's first instruction, a jump backwards, or a changed line. Retail's native line hook is not used: it fires on original lines and never reaches 15% of corpus lines. A function with no table (unmapped) reports entry and return only. Tables that do not fit their function or filter, or two that disagree, refuse the attach | `a_line_starts_where_the_corpus_line_changes_or_the_code_jumps_back`, `line_tables_must_fit_their_function_and_filter`, `a_function_without_a_table_raises_no_line`, `staged_tables_add_up_and_the_next_attach_consumes_them` |
| Shared session | The one-holder rule, request handling, filter, line events, breakpoints, steps and numbering are `lua_debug::session::Debugger`; each host only reads its VM and pauses | session tests |
| Retail memory | `mercs2_formats::lua_proto` reads a running `Proto` tree, its `FunctionKey`, its source and a state's Lua frames at the proven offsets; the stamp of a tree laid out as the loader does equals the stamp of the decoded chunk | `a_tree_laid_out_as_the_loader_does_reads_back_with_its_stamp`, `the_frames_of_a_state_name_their_function_and_instruction` |
| Attach state | After `debug.attach`, a `debug.module` for each filtered module already loaded and one `debug.stack` of the Lua frames on the stack | host tests |
| Stamps (D6) | `lua_debug::stamp` hashes, per function in pre-order, exactly what the line map compares: upvalue/parameter counts, vararg flag, stack size, code, constants, child count. Line info, names and source do not count. A retail run's stamp is compared with the shipped chunk the line map was built from; a corpus host's with our compile of the graph's text | `the_stamp_covers_what_the_line_map_compares_and_nothing_else` |
| Arguments | `lua_debug::Arg` is `mercs2_autoplay`'s trace encoding (`nil`, `bool`, `num`, `str`, `tt2`, `tt5`, `tt6`, `userdata`, `thread`). The retail `lua_trace_asi` file format (`ttN` with `p`) is a different file and is unchanged | `arguments_keep_autoplays_encoding` |
| Text | Game text (module, binding and argument strings) is CP1252, carried as the Unicode text of those bytes, the rule of the bridge's `log` frames; `game_bytes` gives the bytes back | `game_text_is_cp1252_both_ways` |
| Work while paused | An ordinary REPL `{id, code}` sent while paused runs inside the hook; no debug message is needed for it | bridge `a_paused_tick_drains_the_queue_and_leaves_onload_for_the_next_tick` |
| Steps | `into`: next line at any depth; `over`: next line at the paused depth or shallower; `out`: next line shallower than the paused depth. Continue and step are refused unless paused | host tests |

## Run source and attach

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Edit / Debug switch | Debug lays the open run over the open graph; Edit hides it. The graph is not editable in Debug | `missions.rs` header | — | the mode | UI test: no edit act is accepted in Debug |
| Run picker | Lists the live host on the bridge port, labelled by what it says it is (retail with its exe build, `mercs2_game`, autoplay), and the recordings in the workspace. Picking a live host attaches; picking a recording opens it | `debug.hello` → `debug.host` (D1) | — | hosts, picker | fake-host test per host kind |
| Attach | Sends the filtered modules' line tables (`debug.lines`), then the module filter and the breakpoints. The filter is the open script, every script with a breakpoint (a library script as much as a contract) and, for a Depot preview, the draft's chunk. A watched script with no line map refuses the attach in the Workshop, naming it and why, before anything is sent. Nothing is hooked until a client attaches, and detaching removes the hook | protocol | — | Depot preview | `missions_debug` `an_attach_watches_the_open_script_and_every_script_with_a_breakpoint`, `a_script_with_no_map_refuses_the_attach_by_name`; host test: hook absent before attach and after detach |
| Adding a script to a run | A breakpoint in a script outside the run, or Add to the run on the open one, sends that script's line tables and a `debug.watch` naming every watched script, then the breakpoints; running or paused (the pause holds), and the run keeps its counts. A refusal leaves the run watching what it did | `debug.watch` | — | — | `a_breakpoint_in_a_new_script_joins_a_paused_run_through_a_watch` |
| Engine breakpoints (page) | The Breakpoints list holds engine breakpoints beside the line ones; the picker below it lists the bindings the game named after hello (`debug.bindings`) that contain what is typed, eight at a time. An engine breakpoint can be set before attaching, and an attach with only engine breakpoints watches no script. Paused before an engine call, the bar names the binding and its caller and shows the call with its arguments; Step is disabled when the caller is not watched | `debug.names`, `debug.engine` | — | — | `engine_breakpoints_name_the_hosts_bindings_and_a_pause_outside_the_run_only_continues` |
| Refusals | A refused request is shown with its id and reason, and the link stays up; the rest of that command's requests are not sent | — | — | — | `a_refused_attach_leaves_the_run_as_it_was` |
| Recording | Every event received while attached is written to a recording in the Depot workspace, with the host label, build, and the per-module stamps (D6) | — | recording file | recorder | round-trip test |
| Stamp check | Opening a run whose module stamp differs from the graph on screen is refused with the module, both hashes and the bank: "recorded against a different gurcon002 (bank vz, …); reopen the graph it ran" | D6 | — | — | test: edited-then-compiled graph refuses its old recording |
| Preview run (Depot) | From a Depot Lua draft: choose autoplay with the draft applied, or a host started from an autoplay-discovered save with the draft installed as a preview overlay (D4). The run opens here in Debug | Depot item, Saves | preview overlay (never `vz.wad`) | all | end-to-end on autoplay |

## Graph overlay

| Element | Behaviour | Data / code | Missing | Proof |
|---|---|---|---|---|
| Node: ran | A node any of whose lines executed is drawn lit, with its run count (×N) | line events | `paint_node` state | test over a recorded run |
| Node: not taken | A node in an arm that was never entered is dimmed with ×0 | arm spans | arm line spans from `Form` | test |
| Node: not reached yet | Lit nodes stop at the playhead; nodes after it on a taken path are outlined, not lit | playhead | — | test |
| Wires | An exec wire is lit when both its ends ran in order; a wire into a not-taken arm is dashed and dimmed | — | wire stroke at `missions.rs:1667` | test |
| Branch outcome | An if/elseif/else node shows each condition's result and the arm taken, decided by which arm's line span executed. An arm on the same line as its condition (`if c then x() end`) cannot be told apart by lines; the node says "arm not resolvable by line" for it rather than guessing | line events, arm spans | span accessor | test with a one-line `if` |
| Depot frame | Functions from a Depot draft's chunk are drawn in a dashed amber frame named for the draft, joined to the shipped function they wrap | append chunk name (splice keeps it) | lift of the draft beside the base | test with a spliced chunk |
| Breakpoint marker | A red dot on a node with a breakpoint; the node the run is paused on is outlined in amber | — | — | test |

## Runs of calls

Decided by the user 2026-11-06: any run of two or more consecutive calls to one function folds into one
node, and a call with a wired argument breaks the run. Measured over the vendored corpus: 705 such runs
hold 2,985 statements, `import` 1,001 of them (runs up to 37); folding removes 2,223 exec wires from the
corpus's canvases.

| Element | Behaviour | Proof |
|---|---|---|
| Fold | Consecutive statement calls to one global's function (no kept value, no callback, every argument a literal, a name or a literal list) lift as one run node titled `<callee> ×n`, each call's arguments its own group of pins. A comment between two calls ends the run | `repeated_calls_fold_into_a_run_that_emits_every_call` |
| Emit | Each call is emitted back as its own statement with its own tokens, line and semicolon, so the corpus still lifts and emits byte-identical | `every_corpus_script_lifts_and_emits_byte_identical` |
| Cards | A run draws as a stack of cards: the edges of two cards behind the node's left side, and a numbered card around each call's pins; when every call takes one argument the card shows the value alone | — |
| Data pins | On every node, an input pin sits inside the node, left-aligned with the header's icon, and an output the same distance inside the right edge (user, 2026-11-06); exec pins stay on the edges | `missions::pin_at` |
| Edit | The node's menu: Unfold calls (one node per call, chained where the run stood), Add a call (a copy of the last), Remove a call; a run of one is a call again, and a plain call can Repeat this call into a run | `a_run_adds_removes_and_unfolds_its_calls` |
| Debug | Each call keeps its line: the node's count is the sum of its calls', it is lit as paused on any of them, and its menu has a breakpoint per call | — |

## Members

Decided by the user 2026-11-06, after a reading of Unreal's Blueprint editor (members are declared in its
My Blueprint panel with their defaults in Details; the graph holds behaviour): a script's top-level
declarations are listed in the navigator, not drawn on the main chunk's exec chain. How a function is
declared says nothing (Lua 5.1 compiles `function X()` and `X = function()` alike, and the corpus is
decompiled); how it is reached is what the list sorts by.

| Element | Behaviour | Data / code | Proof |
|---|---|---|---|
| Members | A top-level statement of the main chunk is a member (`blueprint::Member`): a `function` statement (any form); globals given literals, global paths or `nil` (a variable); globals given tables of those (data); an `import`/`inherit` call or run of them. Any other statement is behaviour — among them an assignment to a local, and a value read from a local (`Activated = L0_1`) | `Lifter::block`, `data_member`, `lua_source::MODULE_CALLS` | `mrxbriefing_s_functions_are_reached_as_the_exe_and_the_corpus_show`, `member_rows_list_what_a_script_declares_and_not_what_its_code_does` |
| Drawn chunk | The main chunk draws its behaviour only: a member is left out unless an output of it feeds a drawn node, with the data nodes that only feed left-out nodes; an exec wire runs on through left-out nodes to the next drawn one. Every node keeps its id, so the script emits as before | `Graph::hidden`, `Graph::shown` | `the_drawn_chunk_runs_its_wire_through_members_and_an_edit_on_it_keeps_them` |
| Edits through members | Breaking, connecting or adding on a wire that runs through left-out members edits it at the last of them, so they stay on the chain; select-all and the selection rectangle take drawn nodes only | `Graph::through`, `missions::through_hidden` | same test |
| Reach | Each top-level function carries how it is reached (`blueprint::Reach`): the game's executables call it by name (in this module, in a world object's script, or in a module chosen at run time); it overrides a base class's function; it is passed to a call (an engine binding, Lua of the corpus, or neither) or stored; it is called on a line of the script; other scripts name it as `Module.name`. Calls through `self:` and through a module held in a variable (`mModule.Start()`) name no function and are not counted | `blueprint::reach`, `engine_calls`, `CorpusIndex` | same test (mrxbriefing: 8 named by the exe in its module, `NetEventCallback` at run time, `Event.Create*` handlers, Lua callbacks, 4 with none found) |
| Engine calls | `crates/mercs2_workshop/data/engine_lua_calls.tsv`, written by `exemap lua-calls` over every catalogued image (the by-name callers are `mercs2_exemap`'s `data/lua_callers.tsv`: the by-name caller and the object dispatcher of each); a call whose module was not read counts for no module. Engine bindings are the pc-v10 image's luareg export, `data/pc-v10.luareg.tsv`. Not read: the PC paths that pass `Init` to `0x5a0db0`, `Start` to `0x59e480` and `Investigate`/`Reverse` to `0x59e190` (each a jump into the SecuROM VM); `Deinit`, which no code of any image references by address; and per PS3 image 23 call sites in no Ghidra function. So "none found" is what a function row says, never "no caller" | `engine_calls` | `the_compiled_in_data_reads_and_names_what_the_briefing_proves`, `a_malformed_row_is_an_error_naming_its_line` |
| Engine arguments | A call of an engine binding labels its argument pins with the name `data/binding_names.tsv` gives it, else `arg N · type` (the type the binding reads it as), else `arg N`; a pin's hover shows the name's confidence, review status and evidence, and the type. Types and positions are `data/binding_args.tsv`, written by `exemap binding-args x360-retail` through `mercs2_exemap`'s `data/lua_readers.tsv`: the positions of the call that gives every argument. The names were traced from each binding's decomp and the scripts' calls (Graphics.Atmosphere by hand, the rest one agent per namespace with a second checking each name), and every row is `proposed` until reviewed. The types come from the x360 image alone, so the 61 namespaced bindings only the PC image registers have none | `engine_calls::EngineData::params`, `param_note` | `a_binding_s_pins_take_their_names_else_the_type_its_reader_reads`, `the_names_file_refuses_an_unknown_confidence_or_status_and_a_second_name` |
| Canvas header | Edit / Debug and the breadcrumb (file › function › …) on the left; View, Script, Add Node, Undo, Redo, Revert and Compile on the right, laid out first so nothing runs under them. The breadcrumb takes the width left over: the file name shrinks first, then the functions, each truncated with its full text on hover | `missions::header` | none (seen in a `--ui-state` capture) |
| This script | The navigator's lower pane shows one of This script, Palette, Breakpoints (Debug mode) and Saves, picked by its tab. This script: one filter over every section; the main chunk (its drawn statement count); then sections — each a card whose header is a row of the rows' own height and padding (collapse arrow, title, count, `+`), reaching 4 px past the column so its rows line up with the script list's; a closed or empty section is its header, and the Palette tab's groups are the same cards (each group's note on its title's hover) — Events (the engine or the base class calls it by name), Handlers (passed to an engine binding), Callbacks (passed to Lua or to something unresolved, or stored), Functions, Variables, Data, Imports (collapsed at first), Objectives, Globals set. A section that only lists what the script holds is left out when it is empty; Functions, Variables and Data have `+`, which adds `NewFunction`, `NewValue = nil` or `tNewData = {}` (the next free name) at the end of the main chunk. A function row opens its canvas, says who reaches it, and lists every way it is reached on hover; its menu has Break on entry (Debug) or Delete (Edit). Any other row opens the main chunk with that member selected, for the inspector; in Debug mode it keeps what its node had on the canvas — its run count (×N), its breakpoint dot and an Add/Remove breakpoint on its line, lit when the run is paused on it — and a pause on a member's line opens This script on its row. An import run lists one row per call, named by the module its first argument names | `missions::members`, `member_rows` | `member_rows_list_what_a_script_declares_and_not_what_its_code_does` |
| Decompiled DLC | unluac's output for a debug-stripped chunk defines a function into a register local (`L0_1 = function … end`) and stores it in its global afterwards (`Activated = L0_1`), and builds tables one field at a time: those functions are listed under the register's name with none found, and the stores stay on the drawn chunk as the code they are | — | — |

## Canvas layout

Where a node goes when it has no position yet (`crates/mercs2_workshop/src/missions_layout.rs`; every rule
is a field of `missions_layout::Rules`). Decided by the user 2026-11-06: no wrapping (a long function is
wide, and the canvas pans), the branch shape of their hand-arranged `mrxplayer` `MedEvac` as the default,
and a hand-written layout with ELK Layered kept as a test-only yardstick.

| Rule | Behaviour | Proof |
|---|---|---|
| Feeders | A data node (no exec pins) sits left of the node it feeds, its output pin level with the input it feeds (`feeder_drop` below it, 0 by default), its own feeders further left, one node's feeders stacked down in pin order. The node they feed is pushed right to make room | `a_branch_puts_then_above_else_below_and_the_rest_below_both` |
| Exec chain | Statements run left to right, each exec input level with the exec output it continues from; the entry's body continues the entry's row | `a_long_chain_stays_one_row` |
| Blocks | A statement with blocks (a branch's arms, a loop's body, a callback, including one held by its feeder) ends its row: the first block rises `first_arm_rise` above the output it hangs from, the others stack below in port order, and the code after the statement continues on a row below them all, right of the statement. Each block is laid out whole and placed as a box | `a_branch_puts_then_above_else_below_and_the_rest_below_both` |
| Unreached | Statements no chain reaches (unwired by an edit) and loose values go in one row below everything | — |
| Gate | Over every canvas of the corpus, each main chunk as drawn (without its members): no two nodes overlap and no wire runs backward. Measured 2026-11-06 before members left the chunk: the previous layout had 77,379 backward wires, 529,316 crossings and 879 units of vertical drift per wire; this one 0 backward. Measured 2026-11-06 with members listed and pins inset (7,136 canvases, 178,048 wires): 0 backward, 0 overlaps, 250,409 crossings (counted on straight pin-to-pin segments; the wires are drawn as curves), 219.9 drift per wire; by kind exec wires 63.9, feeder wires 96.5, local and parameter wires 407.8. Without wrapping, `mrxsupportdata` `Init` (790 nodes) is 295,887 units wide, and 105 canvases are larger than fit-to-view frames at its farthest zoom (0.1: 15,040 × 8,040 units in a 1600 × 900 pane) | `every_corpus_canvas_lays_out_with_no_overlap_and_no_backward_wire` (prints the totals and the widest and longest-wired canvases) |
| Yardstick | ELK Layered (`elk-layout`, a dev-dependency) with fixed pin positions, spline routing, straight edges favoured and model order kept, over the 7,110 canvases of up to 200 nodes (2026-11-06, chunks as drawn): ELK 308.5 drift per wire, 182,291 crossings, 208M length; this layout 178.2, 158,340, 230M; both 0 backward and 0 overlaps | `elk_lays_out_the_same_canvases_as_a_yardstick` |

## Left column

| Element | Behaviour | Data / code | Missing | Proof |
|---|---|---|---|---|
| Call stack | Rebuilt from entry and return events at the playhead; each frame names the function and its current line, an engine binding shows `engine`. Clicking a frame opens its canvas | entry/return events | — | test |
| Functions this run | Every function of the open module with its entry count at the playhead; never-entered functions are grey ×0 | entry events | — | test |
| Script list | Missions (inherit chain reaches the base task) or Library (every other corpus script), switched in the Script heading with each count; a script with breakpoints carries a red dot. A library script opens on the canvas like a contract | corpus index | — | `debug_mode_maps_a_real_contract_and_a_library_script_onto_their_shipped_chunks` |
| Breakpoints | In Debug mode the navigator's Breakpoints tab (its count on the tab) lists every script's breakpoints, each with its script, the function it is in (or `entry`) and its line; clicking one opens its script on that line, × removes it. Add from a node's context menu, or from a function row's in This script ("Break on entry": its first instruction's corpus line; a function that does not map, or that the host cannot tell from another, refuses it with why). On a live run the set is sent at attach and on every change. Opening another script keeps every script's breakpoints | — | the source view | `a_function_breaks_on_entry_at_its_first_instructions_line`, the retail page test (mrxutil `SpawnObject() entry`); host test: a breakpoint added while running stops the next pass |
| Following a pause | When the run pauses in a watched script, that script opens on the canvas of the paused line with its node selected, whichever script was open | pause event, the script's map | — | live |

## Transport

| Element | Action → effect | Missing | Proof |
|---|---|---|---|
| Continue / Step over / Step into / Step out | On a live paused run: sent to the host, which resumes until the next breakpoint or step target. On a recording: moves the playhead to the next matching event | hosts | host tests per command |
| Step back | Moves the playhead back one event; never touches the game (D7) | — | test |
| Paused status | "Paused at breakpoint" with the host and location; "Running", "Detached", or the host's error otherwise | — | fake-host test |

## Execution trace

| Element | Behaviour | Missing | Proof |
|---|---|---|---|
| Rows | Every event in run order: sequence, kind (`lua`, `engine`, `line`), what (function or binding and arguments), and where (module:line in the graph's line space) | — | test |
| Filters | All · Engine calls · Branches | — | test |
| Row click | Moves the playhead to that event; the graph, stack and inspector follow | — | test |

## Inspector

| Element | Behaviour | Missing | Proof |
|---|---|---|---|
| Call header | The selected engine call or Lua call, whether the run is paused on it, and whether it comes from a Depot draft | — | test |
| Arguments, as recorded | Each argument as the host recorded it. A guid that an earlier engine call returned names that call and its line | return values of engine calls (return event) | test |
| Same calls earlier in this run | Earlier calls of the same binding with the same first argument, with their values and lines | — | test |
| Source | The function's source around the selected line, from the draft for a Depot frame or the corpus otherwise | `luaview` | test |
| Jump to source / Open Depot item | Opens the line in the Lua view (`luaview::scroll_to_line`), or the Depot item the draft belongs to | — | UI test |

## Pausing the live game (D2)

| Element | Behaviour | Host | Missing | Proof |
|---|---|---|---|---|
| Pause | The hook blocks the game thread on a condition the debug channel signals. The bridge's socket threads keep running, so Continue arrives; requests are queued by `m2_lua_debug_request` and handled by the pause loop. Closing the holder's connection while paused frees the debugger and resumes the game | all | — | host test; live 2026-11-05, v1.1 exe under Wine: `mercs2_probe lua-debug --module mrxguibase --bank shell --line 157` paused at `mrxguibase` pc 0, `--then continue` gave `debug.resumed` and a clean detach, `--then disconnect` left the debugger free and the game running |
| Work while paused | The pause loop calls the transport's paused callbacks (`m2_lua_debug_on_paused`) on the game thread, inside the hook; the bridge's runs its queued chunks there (`Pump::paused_tick`). OnLoad's scripts, if they come due during a pause, wait for the first tick after it. The hook cannot fire inside itself, and no event is raised while an executor chunk runs (`m2_lua_exec`), there or at any other time | all | — | bridge `a_paused_tick_drains_the_queue_and_leaves_onload_for_the_next_tick`; live 2026-11-05 (`--repl`): a WebSocket chunk's `Loader.WsSend` line and a raw-TCP chunk's result (`return 6 * 7` → `42`) both arrived before `debug.resumed` |
| Hook cost (retail) | m2-sdk.dll logs the hook's calls and time at detach. Measured live 2026-11-05 under Wine on the front end: 60–85 µs per call, about 4,800 calls a second with a front-end module filtered, so about 30% of the game thread while attached. Each event's frame read checks every pointer with `VirtualQuery` (`lua_proto` `need`), at least four a call | retail | — | `m2-sdk.log` "hook removed" lines |
| Player notice (retail) | The game cannot draw while its thread is blocked, so m2-sdk.dll shows a topmost window that does not take focus, from its own thread: "The game is paused by the Workshop debugger at `<module>:<line>`. Press Continue or F8 to continue, or click Continue in the Workshop.", with a Continue button. Continue or F8 resumes; however the pause ends, the window closes itself. The game window's title (found among the pausing thread's windows) says the same until resumed | retail | the title: under Wine the thread that runs the game's Lua owns no visible window (`m2-sdk.log`, 2026-11-05) | live 2026-11-06, windowed under Wine (user at the game): the notice showed over the menu and over the world; fn+F8 resumed at the menu and the notice's Continue resumed in the world (`m2-sdk.log` "the player resumed", `debug.resumed`). UNPROVEN over a full-screen exclusive device |
| Paused on every page (Workshop) | While the debugger holds the game, the rail's Live cell reads PAUSED with an amber dot (`theme::live::AMBER`) whatever page is open; the session is the app's (`missions_debug::Debug`, owned by `app.rs`), because the host takes one debug client and a page reset must never drop the link and resume the game. Live's own pieces (a debugger row in Connection, the paused strip, pins for the run's guids, one recording for replies and run) are drawn on the canvas board `LiveDebug.dc.html` and not built | Workshop | Live's pieces | `missions_debug` `the_session_follows_the_host` |
| Player notice (`mercs2_game`) | The engine keeps presenting the last frame with an in-game banner carrying the same text; F8 resumes | ours | banner | live |
| Watchdog | The bridge's watchdog reads the timestamps `paused_tick` keeps as a tick does, so a pause in which queued work drains is not reported, and one in which it waits is reported as not draining. A pause cannot start inside a bridge chunk (no events while an executor chunk runs), so the stuck-chunk path never sees one | retail | — | bridge `the_watchdog_sees_a_paused_bridge_that_drains_and_one_that_does_not` |
| Autoplay | Headless: waits for Continue | autoplay | — | host test |
