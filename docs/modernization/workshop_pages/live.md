# Live — behaviour contract (Live World, read-only free-cam)

The design is the `Live` board of the Workshop Polish Pass canvas (`Live.dc.html`, "Live · read-only
free-cam"). The page mirrors a running game in the workshop window: it follows the player through the
world on the workshop's own renderer, with its own camera, and it never changes the game.

Every row is an element: what the user does, what happens, where the data comes from, what it writes,
what is missing, and what proves it. Nothing is drawn without its row being true. No value is copied
from the mockup. A row that names a decision (D1…D8) follows the decision recorded below. The board's
Culled bindings and Fog of war overlays are not built (D6).

Grades: PROVEN (read in code, shipped Lua or a deployed log, or measured live), INFERRED, UNPROVEN. Live
runs were against v1.1 (`7a348847…`) under Wine, 2026-11-01.

## Measured on the running game

| Fact | Evidence | Grade |
|---|---|---|
| Every chunk's reply parses: pose, entities, world, mission (free roam), statics | Live page against the game, no fault shown | PROVEN |
| Hero `2652.3, -13.7, -995.4`, then `2856.7, -24.7, -1214.1` as it drove: the mirror followed, world and vehicle drawn at the reported place | capture | PROVEN |
| `M2Live.CameraEye`: eye 2.12 m above and 2.16 m behind the hero on foot, the on-foot boom of memory `camera-live-capture-and-mode-system`, on v1.1 | `eye 3791.84 452.18 -3888.77`, `hero 3791.89 450.06 -3886.61` | PROVEN |
| `M2Live.Residency`: 10,708 records, 820 blocks. Manager pointer `0x1175630` | `m2-sdk.log` | PROVEN |
| `M2Live.AssetStatus`: `pmc_hum_mattias_v4` `2:3` (resident, 3 refs); an unknown hash `-` | REPL | PROVEN |
| A chunk grows the Lua stack often: 8 moves in 80 s, each survived; the stack is reallocated smaller between them by GC | `m2-sdk.log` | PROVEN |
| The bridge's pump only ticks when the game's Lua reaches a detour, and the log stub's non-Lua callers tick it with a null `L`, which `slot_of` matched against an empty slot. While polling, chunks stopped draining for 10–20 s and then resumed; one run stayed stopped for minutes | watchdog `queue-not-draining`, chunks resuming after it; `m2-sdk.log` `lua_State 0x0 rejected` | PROVEN (the gaps, the null ticks). The cause of a gap, idle Lua or the polling itself, is UNPROVEN |
| `lua_bridge.log` records every result: 181 KB in 80 s, about 8 MB an hour | file size over the run | PROVEN |
| The game log holds no Lua lines without `PMC_VERBOSE_LOG=1`, so the ladder is read from `Loader.GetLoadPhase` | `pmc_blackbox.log` stops at `[Build identity]` | PROVEN |
| `Ai.GetRelation` writes relation caches and links a node (`FUN_005880D0`). `Ai.GetState` takes a table and calls a virtual. Neither can be on a read-only page | decomp | PROVEN (v1.0) |
| `Object.IsValid` checks only the argument's tag, so it cannot vouch for a stale guid | decomp `0x5CDF60` | PROVEN (v1.0) |
| `Object.GetPosition` on an unregistered key returns nil and does not dereference. `Player.GetCamera` returns a pointer, not a guid | decomp; `[ok] nil` live | PROVEN |

## Facts the design stands on

| Fact | Evidence | Grade |
|---|---|---|
| The deployed bridge is `lua_bridge` 1.0.0, the Rust rewrite on m2-sdk. `lua_bridge_DEV.c` is an older C build whose limits and threading differ; it is not the reference | `scripts/lua_bridge.log:2,7`; `shipments/mercs2-lua-bridge/README.md:8` | PROVEN |
| The game last ran on this Mac from `game-files/` under Wine: `Z:\Volumes\Projects\…`, `MinHook @0x006AEF90` (the v1.1 stub) | `pmc_blackbox.log:9,12` (2026-10-27) | PROVEN |
| The bridge address is configuration: `[repl] host, port` in `<install>/scripts/lua_bridge.ini`, written with defaults when missing | `lua_bridge.ini`; bridge README §Configuration | PROVEN |
| One raw REPL session at a time; a second raw client waits for the first to disconnect. Each connection has its own thread, so a silent client blocks nobody, and it is closed after 5 s | `shipments/mercs2-lua-bridge/client/src/repl.rs:17-21,42,143` | PROVEN |
| Every raw session writes an attach line and an end line to `lua_bridge.log` | `repl.rs:145,149` | PROVEN |
| A chunk's `return` values come back on raw TCP as `[ok]\t<v1>\t<v2>…`, at most 16 values. Numbers are `%g` (6 digits). Strings are quoted, unescaped. Guids print `<tt=2 val=…>`, tables `<table>` | `mercs2_formats::lua_exec` `MAX_RESULTS`, `fmt_g`, `format_value` | PROVEN |
| A result is cut at 16 KiB with no marker: `m2_lua_exec` copies `min(len, cap-1)` | `lua_host.rs` `m2_lua_exec`; `win.rs:56` `RESULT_CAP` | PROVEN |
| A chunk runs on the game thread on the next frame and nothing can abort it. `Sys.MainTimeStamp` allocates, `Loader.PopKeyEvents` drains a ring, and `Debug.Printf` paths reach the log | bridge README; agent read of bindings | PROVEN / INFERRED for the side effects |
| Bare module names are nil from the REPL; `_MODULES.<lowercase>` works; `import()` from a chunk is unsafe | `object_script_modules.md:87-94`; `fx_live_gate.md:293` | PROVEN |
| Hero: `Player.GetLocalCharacter()`, `Object.GetPosition`, `Object.GetYaw`, `Object.GetHealth`, `Object.GetMaxHealth` | `mrxplayer.lua:569-570`, `hero.lua:126-127` | PROVEN |
| Template and model come back as hashes: `Sys.GuidToString(Object.GetName(Object.GetParent(c)))`, `Object.GetModelName(c)`. The workshop's name table resolves them | `factionzone.lua:6-25`; `0x5ccc20` | PROVEN; the model binding has no shipped caller, INFERRED |
| Vehicle: `Vehicle.GetFromRider(c)`, `Vehicle.GetSeatFromRider(c)` | `mrxplayer.lua:486`, `hero.lua:311` | PROVEN |
| Camera: `Player.GetCamera(p)`, `Camera.GetYaw/GetPitch/GetFOV(u)`. **There is no position getter.** `Pg.FindPointFromCamera(0, 0)` is a ground point, not the eye | `mrxguipda.lua:434-436`; binding table (14 entries); `mrxguisatellite.lua:225` | PROVEN |
| Enumeration: `Pg.FastCollect{Humans,Cars,Tanks,Helicopters,Jets,Boats,…}(x,y,z,r,filter)`. `Pg.GetObjectsInArea` is capped at 256 and excludes humans | `oilcon002.lua:1198`; `0x100` on X360 | PROVEN (shipped use), INFERRED (cap on PC) |
| A placed entity's guid is its layer key, so a live guid joins to `PlacedEntity` | `level_editor.md` key ranges; `dump-pool` | PROVEN |
| Dynamic layers: `_MODULES.mrxlayermanager._tLoadedLayers`. Static layers are not in it | `mrxlayermanager.lua:28,250` | PROVEN |
| Blocking sequences: `_MODULES.mrxmissionflow._nBlockingSequences`, a module global (not `local`) | `mrxmissionflow.lua:49,812-821` | PROVEN |
| Mission: `M.mrxplaystate.GetCurrentMission()`, `:GetMissionId()`, task tree `:GetChildren()`, `._nState`, timer `_oTimer:GetTime()`, race `.iCurLoc`, `_tCourseLocs` | `mrxplayer.lua:206`, `mrxtask.lua`, `mrxtaskrace.lua:101,182,277` | PROVEN |
| Streaming state: `Sys.IsLoadingOrStreaming()`; load ladder: `Loader.GetLoadPhase()` | `wifvzatmosphere.lua:26`; bridge README | PROVEN |
| No Lua read for: camera world position, resident chunks, per-asset residency, save or profile name, an act name, a frame counter | binding table; agent sweep | PROVEN for the bindings searched |
| The workshop draws 3D behind egui on the whole surface; the camera is loose locals shared across pages; a `CentralPanel` takes the pointer | `scene.rs:2285,2296`; `app.rs:2138,8842`; `gui.rs:215` | PROVEN |
| `StreamingWorld::step(scene, world, viewer, exempt)` streams around any point; `viewer` is ours to supply. `load_streaming_world_data` opens one WAD with no overlays | `game_world.rs:1197,1884` | PROVEN |
| The map image is `vz_lrterrain` (`0xA007B5B9`, block 3121), 2048 × 2048 DXT1, 8,000 × 8,000 m, `px = (4000 − x) / 8000 · W`, `py = (4000 − z) / 8000 · H` | `level_editor.md` facts | PROVEN |
| The reimpl (`mercs2_game`) also answers the bridge, but replies `tostring(value)` with no `[ok]`, and `Player.GetCamera` returns nil | `world.rs:1806-1815`; `player.rs:972-974` | PROVEN |

## The link: one owner of the bridge

| Element | Behaviour | Data / code | Missing | Proof |
|---|---|---|---|---|
| Worker | One thread owns the connection. The rail dot and the page read one state, so they cannot disagree. Replaces `live::Probe` | `mercs2_bridge::Bridge` | `live::Link` | unit test with `Server` |
| Address | Host and port from `<install>/scripts/lua_bridge.ini`, as the ASI resolves them; `DEFAULT_ADDR` only where the ASI itself would use it. `install` is the folder above the active `data/vz.wad` | `settings.wad_path` | ini read | test over a temp install |
| Page closed | Connect-and-close reachability every 3 s, as today | `mercs2_bridge::reachable` | — | existing test |
| Page open | Holds a session. It closes and reopens every 5 s, so a REPL client gets its turn and `lua_bridge.log` gains under one line a second. Connecting per tick would write 20 lines a second | `repl.rs:145,149` | — | test: a second client gets a turn within one rotation |
| Tiers | Pose every 100 ms. Entities every 500 ms. World (layers, mission, streaming) every 2 s. Serial: the next chunk waits for the last reply, so a slow game slows the poll and never queues | `Bridge::eval` (60 ms floor) | tier scheduler | test with a fake clock |
| Reply | Each chunk returns one string built with `string.format`, ending in a sentinel. No sentinel, a bare `[ok]`, or `[runtime]` / `[compile]` / `[bridge]` is a hard, shown error, never empty data | `ess_live.rs` `Reply` parser | Move the parser to `mercs2_bridge`; `ess_live` calls it | tests: truncated reply, each error tag |
| Read-only | Chunks are constants in one module. Every call in them is on an allowlist. A test scans them and fails on anything else, and names `Debug.Printf`, `Sys.MainTimeStamp`, `Loader.PopKeyEvents`, `import` as banned | new `live_chunks.rs` | allowlist test | the test |
| Guard | Every chunk first checks `_MODULES ~= nil and _MODULES.mrxplaystate ~= nil`; false reads "no world", not an error | — | — | test |
| Stale guids | A guid is never stored across chunks. The selection and lock-on are held as the `Sys.GuidToString` text and found again inside the enumerating chunk, because a bad guid in a native read kills the game | — | — | test: chunks take no guid parameter |
| Link state | Off · Connecting · No world (menu or loading) · Streaming · Not answering. Each is a distinct string in the header and the footer | state machine | — | fake-host test per state |

## Header

| Element | Action → effect | Data / code | Writes | Missing | Proof |
|---|---|---|---|---|---|
| Link pill | Shows link state and measured round-trip: "streaming · N ms lag" | pose-tier RTT | — | — | fake host with a set delay |
| Read-only chip | Tooltip states what the page cannot do. True because of the allowlist | allowlist test | — | — | the test |
| Session pill | Host, hero, level, last completed contract. Host is `retail` when `Loader` exists, else `reimpl`. Save name per **D4** | `Sys.GetLevelName()`, `M.mrxmissionflow.GetLastCompletedContractName()`, `Object.HasLabel` | — | **D4** | capture |
| Camera mode | Free orbit · Chase hero · Top-down · Mirror in-game. Local only | see Camera | settings: last mode | Mirror needs **D2** | test per mode |
| Snapshot | One click writes a JSON of the last samples, overlay states and the camera to the Workshop scratch folder; the status line names the path | `Link` state | `<scratch>/live-snapshot-<time>.json` | — | test: fields round-trip |
| Record | Toggles; the button shows elapsed time while on. Appends pose and entity samples and new log lines to a `.replay`. Format and playback per **D8** | tiers | `<scratch>/<time>.replay` | **D8** | test |

The board's 9 and 10 px labels are set at the 11 px scale minimum. The mockup's blue-steel palette is
kept for this page as `theme` tokens, not literals.

## Connection panel

| Row | Shows | Data | Missing | Proof |
|---|---|---|---|---|
| lua_bridge | Link state, `:port` from the ini | `Link` | — | fake host |
| pmc_bb | "log tail" while `<install>/pmc_blackbox.log` is growing; "stale" past 5 s; "no log" when absent | file length and mtime | `live::LogTail` | test over a growing file |
| x32dbg | Per **D5** | none today | **D5** | — |
| wgpu mirror | Frames per second of the mirror | measured frame time | — | capture |

## Our render vs retail

| Row | Shows | Data | Missing | Proof |
|---|---|---|---|---|
| Hero pose | Per **D7** | pose tier against mirror entity | **D7** | fixture |
| Vehicle poses | `n / m` per **D7** | entity tier | **D7** | fixture |
| NPC poses | `n / m` per **D7** | entity tier | **D7** | fixture |
| Resident chunks | Per **D3**. No Lua read | none today | **D3** | — |
| Active layers | Count of `_tLoadedLayers` entries that are not marked for removal, plus the static list. Static list is found by scanning `_MODULES` for `_tStaticLayers` with `rawget`; its module key is unknown | world tier | — | fixture; `level_editor.md` 410 / 413 layers against a save |
| Blocking seq | `_nBlockingSequences`; nil reads 0 (idle), as the module's own reset leaves it | world tier | — | fixture |
| Surface-A divergence | Per **D5**. `surface_a.asi` is not deployed in the install | none today | **D5** | — |

## Overlays and view pills

The pills and the overlay list share one state: a pill with a matching overlay is that overlay.

| Element | Effect | Data | Missing | Proof |
|---|---|---|---|---|
| Terrain mesh / Terrain pill | Terrain cells on or off | `StreamingWorld` | Scene draw flag | capture on and off |
| Collision (MOPP) / Collision pill | Collision triangles of resident cells, drawn as lines | `collision_tris` | line pass or egui projection | capture |
| Patrol paths | `Path` entities of the active layers | `layerview`, `placement_data_format.md` §2.6 | — | capture |
| Trigger volumes | `LineRegion` polygons | same §2.6 | — | capture |
| Spawn radii | Population spawners with their radius | `population-unit-list-draw` data | decode check | capture |
| Audio emitters / Audio pill | Sound entities of the active layers, from the component classes `layerview` already marks | `layerview` | — | capture |
| Wire pill | Wireframe world | Scene | `POLYGON_MODE_LINE` pipeline variant | capture |
| AI debug pill | Labels each nearby NPC with its AI state and its relation to the hero | `Ai.GetState`, `Ai.GetRelation`, once read to be pure | decomp read | fixture |
| Cinematic pill | Follows the camera when the game is in a cutscene. Tied to Mirror | **D2** | **D2** | — |

## Viewport and camera — following the player

The mirror is the workshop's `Scene` drawing Live's own `hecs::World`, so Inspect's content and camera
are untouched. The world around the player is real: the same terrain, layers and models the game loads,
at the position the game reports.

| Element | Action → effect | Data / code | Missing | Proof |
|---|---|---|---|---|
| Mirror world | Starts on the first pose sample. `StreamingWorld::step` is driven with viewer = the hero. Requested layers come from retail's own lists, so the mirror shows the state the game is in, and a mission's layer swap appears in the mirror | `game_world.rs:1884`; world tier | Overlay stack on `load_streaming_world_data`; without it a modded game diverges | capture at two positions |
| Page hosting | Live registers no `CentralPanel`, so the camera keeps its input. It keeps its own camera | `app.rs:1078,3562,4241,6135,8842` | guards lifted | input test |
| Visible centre | The side columns cover the 3D, so the projection is shifted so the hero sits at the centre of the visible rect | `scene.rs:2301` | off-axis term on `set_view` | test: projected hero at rect centre |
| Default | The page opens in **Free orbit locked on the hero**, as drawn, so it follows from the first sample with no click | — | — | capture |
| Free orbit | Drag orbits, wheel zooms, around the lock target. Shift-drag pans and clears the lock | `app.rs:2200-2231` | Live input branch | test |
| Chase hero | Sits behind the hero on the hero's yaw, or the vehicle's when `Vehicle.GetFromRider` is set, and swings with it; drag offsets and it settles back | pose tier | — | fixture path, sampled |
| Top-down | Straight down on the hero, north up; wheel changes height | pose tier | — | test |
| Mirror in-game | Yaw, pitch and FOV from the game, eye per **D2** | pose tier | **D2** | — |
| Interpolation | The camera and markers draw one pose interval behind the newest sample and interpolate, so 10 Hz data moves smoothly. Retail values are shown unsmoothed in every readout | pose tier | — | test: readouts equal the sample |
| Hero marker | Name, player index, model, health, floating at the projected hero, with a facing arc from yaw | pose tier, name table, stringdb | — | capture |
| Hero model | The hero's model at the retail pose. Bind pose: no bridge read gives animation | `Object.GetModelName`, model loader | — | capture |
| Entities | Humans and vehicles from the entity tier, nearest 128 within 250 m (one reply stays under 16 KiB). A guid that matches a placed entity moves that entity; others spawn in the mirror and leave when the game stops reporting them | entity tier, `GuidMap` | — | fixture |
| Vehicle card | Shown for the hero's vehicle: name, template, model, distance | pose tier | — | capture |
| Pins | Race checkpoints from `_tCourseLocs` (done, active, upcoming, each with its distance), and allies and enemies among nearby entities, coloured by faction | world, entity tiers | — | fixture |
| Zoom | `−` / `+` and the wheel scale the camera distance; 100% is the mode's default | camera | — | test |
| Cam readout | "N m away · yaw N° · retail x, y, z". Yaw is the mirror camera's, position is retail's | camera, pose tier | — | test |

The drawn board shows "Guerrilla patrol · 4" and a layer row among nearby entities. Neither is built:
no data groups a patrol, and clustering by distance would be a guess; a layer has no position.

## Mission HUD, mini-map

| Element | Shows | Data | Missing | Proof |
|---|---|---|---|---|
| Mission title | Current mission id, kind, `CP n / m` when the task is a race | `GetCurrentMission`, `.iCurLoc`, `_tCourseLocs` | — | fixture |
| Timer | Elapsed, total, and a bar | `_oTimer:GetTime()`, `nStartTime` | — | fixture |
| Contract rows | Active tasks of the tree with their state | `:GetChildren()`, `._nState` | — | fixture |
| Available jobs | Count of `_tActiveMissions` where `IsJob` is true. The `+Oil / +Gur` suffix comes from those jobs' factions | `mrxmissionflow.lua:392,591` | `mrxtaskjob` override to read | fixture |
| No mission | The card reads "Free roam" (`mrxplaystate.Get() == 0`) | `mrxplaystate.lua:7-8,54` | — | fixture |
| Mini-map | `vz_lrterrain`, hero dot with heading, checkpoints, nearby entities, and a rectangle for the streamed area. One shared module, so Level's map uses it | `level_editor.md` mapping | `worldmap` module | test: the mapping; capture |
| Mini-map title | Level and its real extent. The board's "7.8 km²" disagrees with an 8 × 8 km map and is not copied | index | — | capture |
| Position, yaw | Retail values | pose tier | — | test |

## Entity inspector

| Element | Action → effect | Data | Missing | Proof |
|---|---|---|---|---|
| Header | The selection, the hero by default: display name, kind, player index | stringdb on `Object.GetLocalizedName`, never a hardcoded table | — | fixture |
| Fields | template, model, faction, health, position, yaw. Live, with the unresolved hash shown when a name is unknown | pose or entity tier | — | fixture |
| Nearby list | Sorted by distance, count in the title. Click selects | entity tier | — | test |
| Viewport click | Selects the entity under the pointer | projected entities | — | test |
| Live log tail | Last lines of `pmc_blackbox.log`, parsed, coloured by source. All sources are shown; the tag set is the log's, not the board's. Follows new lines until the user scrolls up. A new run truncates the file and restarts the view | `loadprobe::parse` | `loadprobe` dependency; `LogTail` | test over a growing and a truncated file |
| Jump cam to | Moves the free-cam to the selection once | camera | — | test |
| Lock-on | Toggles: the camera keeps the selection centred as it moves | selection text | — | fixture |
| Open in Shipment | Starts a Shipment seeded with the entity's template or model, by the Inspect route | `Act::StartShipment`, `app.rs:910,8064` | — | test |
| Selection gone | The entity leaves the reported set: the inspector says so and keeps its last values, marked old | entity tier | — | fixture |

## Footer

| Element | Shows | Data | Proof |
|---|---|---|---|
| Sync | Link state in words | `Link` | fake host |
| loadprobe | Furthest rung and its real ladder name. The board's "phase 10 world-booted" is not a ladder name; rung 10 is `WORLD LOAD START` | `loadprobe::phases::LADDER` over the tail | fixture logs in `crates/loadprobe/tests/fixtures` |
| Timings | bridge RTT, pmc_bb age of the last write, frame time | measured | capture |
| Camera | Mode and zoom. The board's "Spectate" is not a mode | camera | capture |

## Proof

The proof stands on one recording, because no other source can show a real game's replies:

1. Unit tests: reply parsing, read-only allowlist, tier order, camera modes, mapping, log tail.
2. A fake host on `mercs2_bridge::Server` replays recorded replies through the real `Link`.
3. `ui_states/live.json` drives a window capture from that replay, with no game running.
4. The recording itself comes from one run against the real game, and is committed as a fixture. Until
   then the connected rows are proven only against hand-written replies, and this file says so.

The reimpl can stand in as a real second host once `bridge_host` answers `[ok]\t…`, which is one change
to `world.rs:1806-1815`. It proves transport and following; `Camera`, and whatever else the reimpl
stubs, read as absent.

## Decisions

Decided with the user, 2026-11-01.

| # | Question | Decision |
|---|---|---|
| D1 | Who runs the game for the proof | The session launches it: the modkit's `wine64` and prefix against `game-files/`. Memory `run-game-on-mac-modkit-wine` |
| D2 | Mirror in-game needs the eye position, which Lua cannot read | `Object.GetPosition(uCamera)` returned nil on the real game, so `M2Live.CameraEye` in m2-sdk reads `*(P+0x1E0)+0x44`. Measured on v1.1 |
| D3 | Resident chunks and the Shipment residency box | Built, in `M2Live`: `Residency` sums the registry's own counts, `AssetStatus` reads the tables. The box is per overlay file: `placement.json` lists placed files, and nothing deployed says which Shipment gave which asset. Hash collisions are not shown |
| D4 | The save name has no read | Omitted from the session pill |
| D5 | x32dbg row and Surface-A row | x32dbg: process-table check plus bridge silence, reads "stalled", INFERRED. Surface-A's replacement, a live count of placed static entities off their decoded position, is not built |
| D6 | Culled bindings, Fog of war, AI debug | Culled bindings and Fog of war are dropped. AI debug labels nearby NPCs with faction and health: `Ai.GetRelation` writes and `Ai.GetState` is no label read, so neither is on the page |
| D7 | What does a pose "match" | `n / m` = reported entities the mirror resolved to a model and drew at the retail pose |
| D8 | Does Record have a player | `.replay` is NDJSON of the raw replies, played back on Live through the same `Link`, with an Open replay control that is not on the board |

## Order of work

1. `mercs2_bridge` reply parser; `live::Link`, tiers, chunks, allowlist test.
2. Panels, header, footer, log tail, mission card, inspector, all over the link.
3. Camera modes and the mirror world, then overlay-stack loading.
4. Mini-map as a shared module; overlays and pills.
5. Snapshot, Record and replay; fixture; `ui_states/live.json`; capture.
6. Decision rows that were chosen to be built, including any bridge read.
