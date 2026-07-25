# Workshop "Mods" rebuild — Plan 03: the live bridge

**Status:** DESIGN (2026-07-24). Wally's repo survey COMPLETE — facts below are from reading his source.
Account: `github.com/loganw234`, ~23 M2 repos, self-branded ecosystem "mercs2.tools", mostly MIT,
explicitly ports format work from our project lineage.
**Siblings:** `-01-mod-model.md`, `-02-navigation.md`, `-04-manifest-format.md`

## Why this matters most (the loop, not a capability)

Today's authoring loop: author → build WAD → boot game → navigate → look. Minutes per iteration, and
behavior work ("does the gate lower?") is nearly untestable. The live bridge attacks the LOOP. It is
the highest-leverage item on the roadmap because it multiplies every other capability's iteration speed.

The pieces already exist: the game hosts a Lua 5.1 VM; we have the decompiled Lua corpus; spawn-by-hash
works; `pmc_bb` gives native logging; we have full symbol coverage of the exe.

## Ownership (settled with the user)

We are NOT merely consuming an external bridge. **Wally (loganw234) gave permission to adapt his Lua
bridge code into Workshop/Modkit**, and the user wants to ENRICH it with our RE knowledge, not just
lift it. He has been building the Lua-bridge side (plus a webmap, a Lua web IDE, a stopgap skinner, and
a Lua framework "essentials"/"ess") while leaving the bigger engine features to the user. So:

- **The bridge becomes a shared crate `mercs2_bridge`** — a third pillar alongside `mercs2_quartermaster`
  (Plan 01). Both Workshop AND Modkit benefit ("deploy → see it live" is valuable at every tier).
- **His layer = the wire. Our layer = the meaning.** His transport + injection is the foundation; our
  enrichment is the VOCABULARY riding on it. We terminate his text pipe into our knowledge base.

## The enrichment thesis — REPL → remote inspector

A raw `dostring` + log stream is a text pipe. What our RE maps add on top (pure enrichment, his
transport unchanged):
1. **Names on the wire.** Bridge speaks hashes; we have the 939k-name pack + ECS component registry +
   state-machine vocab. `0x8B7DE1F5` ⇄ `my_custom_helipad` on both ends.
2. **Typed reads, not eval.** ECS component registry (220 classes) + symbol map ⇒ "read entity N"
   returns NAMED, TYPED fields instead of a Lua table dump. This is the REPL→inspector jump
   (like Godot's remote scene tree).
3. **State machine as a live control surface.** `FUN_004cf340` Enter/Exit/`SetStateOnMsg` is decoded ⇒
   "force this door to its Open state and watch it" — the landing-craft-gate loop made real.

## Capability ladder (cheapest → costliest)
1. Log stream + `dostring` eval  → a Lua console in the Workshop wired to the running game. Biggest
   payoff per unit cost. (This is likely most of what Wally already has.)
2. Read-only entity/ECS tree     → remote inspector (needs enrichment #2).
3. Writes / pokes / spawn-by-hash → live manipulation.
4. Hot asset reload               → swap a texture/model in a live process.

## Two targets, one protocol
- **retail exe** — REPL FIRST (Wally's already there; adopt it). Age is fighting us here, so writes and
  hot-reload are the expensive end.
- **reimpl engine** — the EXPENSIVE stuff first (hot-reload, entity writes are near-free there).
- Define the message schema so BOTH engines can serve it, one client speaks it. Community story:
  prototype fast against the reimpl, verify against retail over the same wire.

## Performance discipline (a REAL constraint, user-flagged)

The retail game is old and frame-sensitive. The bridge MUST NOT stall the frame. Requirements to verify
against Wally's implementation: work off the game's critical path (own thread / debug thread), rate-
limited, poll rather than block. This is the ONE place to read Wally's actual source before committing
— see below. (Also respect the mandate `no-debug-probes-in-game-exe`: retail-side probes are their own
subcommands; the bridge injection must be an ADDITIVE host, not new game-exe flags.)

## Wally's bridge — as-built facts (from source)

**Repo `mercs2-lua-mods`** (C / MinGW ASI). Core = single TU `mods/lua-bridge/lua_bridge.c` on a shared
`sdk/m2` stdlib (`m2_hook`, `m2_luastack`, `m2_loadtrigger`, `load_ladder.gen.h`) + vendored MinHook.

- **TRANSPORT:** raw TCP (WinSock2), binds `127.0.0.1:27050`, loopback-enforced. Line protocol: client
  sends a Lua chunk terminated by `<<<RUN>>>`; server replies tab-delimited result lines then `<<<END>>>`.
  NO HTTP/WebSocket/upgrade in the C bridge (verified — no `Sec-WebSocket`, no frame masking).
- **INJECTION:** MinHook `.text` detours (SecuROM-safe) via per-binary RVA tables. Three hooks, each
  captures the Lua state then calls `GatedPump(L)`: `DetourNoopStub` (0x002AEF90 __cdecl, shared by ~60
  engine names), `DetourLuaType`→`luaB_type` (0x00460E90), `DetourCreateTextWidget` (0x001B7D30
  __fastcall). **No dedicated frame/tick hook** — execution is OPPORTUNISTIC: whichever hot Lua C-fn
  fires next drives the pump.
- **LUA VM:** Lua 5.1 FLOAT build. `LuaDoString` hand-crafts a `FixedTString` on the stack and calls the
  GAME's own `luaB_loadstring` + `luaB_pcall` (no linked liblua). `lua_State` acquisition is robust:
  `LooksLikeLuaState()` (tt-byte at +4 == LUA_TTHREAD) + `g_seenL[8]` cache + `CaptureL()`.
- **SCHEMA:** no command vocabulary — the payload IS arbitrary Lua source. In/out queues, mutex-guarded.
- **CAPABILITIES (C, `luaL_register`):** `Tcp.Send(host,port,msg)` (loopback only), `Loader.*`
  (Printf, key state, `IsKeyDown`, `PopKeyEvents`, persistence `SaveVar`/`LoadVar`), `math.*` parity,
  script loader (`OnBoot/`/`OnLoad/`(world-load-triggered)/`OnKey/`). **NO entity/ECS/spawn/hot-reload
  in the C bridge** — those live in Lua, in Ess (below). The bridge is deliberately a thin
  dostring+logging+input+persistence surface.
- **PERF/THREADING:** game thread runs only `GatedPump→PumpQueue→LuaDoString` + `CaptureL`; reentrancy
  guard `t_inBridgeExec`; `g_hotWork` gate so idle frames do nothing. Background: `WatchdogThread`
  (self-healing, resets a stuck pump after 8000 ms), key-event ring (~60 Hz), hotkey poll (30 Hz), disk
  I/O off-thread. Documented cost: Printf ≈ 5 ms (Defender), Tcp.Send ≈ 15 ms; explicit warning against
  per-frame logging. NOTE vs our `x32dbg-mcp-pitfalls` memory (a conditional bp on a hot per-frame fn
  kills the process): his is a DETOUR + queue + gate, not a breakpoint, so it's safe where a bp isn't.
- **WORKING CLIENT:** `lua_console.py` (raw TCP, `<<<RUN>>>`/`<<<END>>>`, 120 s timeout). This is the
  only client that speaks the shipped protocol today.

## ★ THE INTEGRATION GAP (load-bearing — the first task)

His BROWSER clients (`tools-shared/js/bridge-client.js` = `EssBridge`) open a **WebSocket** to
`ws://127.0.0.1:27050` and speak a DIFFERENT protocol (`<<<WSR:id>>>`, JSON, `OK\t`/`ERR\t`). A browser
`WebSocket` CANNOT connect to the raw-TCP listener, and the two protocols don't match. His own note:
the client is "not yet wired into any consumer tools; consolidation only." So his entire web fleet
(webmap, Lua IDE, 3d-editor, world-enhancement) has a live path that is NOT wired to the current bridge.

**⇒ The highest-value first move when we absorb this: give the bridge a WebSocket (or HTTP) listener, or
ship a TCP↔WS shim, speaking the `EssBridge` protocol.** That single piece unblocks ALL his web tools at
once AND becomes the transport our own clients (egui Workshop, Tauri Modkit) speak. The `EssBridge`
protocol is the target contract; the raw-TCP `<<<RUN>>>` protocol is what exists.

## ★ Ess = where "meaning" already lives (extend HERE, don't fatten the C bridge)

**Repo `mercs2-lua-essentials`** ("Ess") — the in-game Lua stdlib that turns the thin bridge into a real
API. Namespace files ordered by numeric prefix = load order; `merge.py` concatenates to `dist/`. Real
identifiers: `Ess.Player.pose/teleport/character`, `Ess.Object.spawn/setPos/health/kill`,
`Ess.Vehicle.flyTo/orbitFlight`, `Ess.Probe.nearby/nearest`, `Ess.AIOrders.command`, `Ess.Relations`,
`Ess.Triggers.arm`, `Ess.Objective/Quest/Contract` (mission engine), `Ess.Net` (co-op), `Ess.UI` (9
widgets), `Ess.Camera`, `Ess.Cinematic`, in-game `Ess.Easy.Console`. v0.3.0: encounter features
live-tested (7/8 `Ess.On` hooks fired live).

**★ Ess's three-tier API `Raw` → Core → `Easy` maps 1:1 onto our Tier 3/2/1** (Raw=composable
primitives=Workshop power user; Core=named-param; Easy=guard-railed presets=Modkit/beginner). Adopt this
idiom for our `mercs2_quartermaster` recipe API too (see Plan 01). Our RE enrichment (names on wire, typed ECS
reads via the 220-class registry, state-machine control) plugs in cleanly at the LUA level by extending
Ess — cheaper and more in keeping with his architecture than fattening the C bridge. Typed ECS reads may
still want a native primitive; decide per capability.

## Reconcile / adopt list (from the survey)
- (Resolved 2026-07-24) The untracked `tools/mercs2-skinner/` and `tools/mercs2-mesher/` local copies
  were DELETED from disk by the user — no reconcile needed. If we later want his JS tools, pull fresh
  from `github.com/loganw234`, don't resurrect stale local copies.
- `mercs2-mesher` retargets glTF by BONE NAME (not spatial NN) with FIVE independent validators (bone
  distance / triangle area / limb direction / bind-height / character height). This CONVERGES with our
  own memory (`cj-foreign-model-import` spatial-NN-is-a-trap; `xfer_selftest`). Convergent evolution =
  confidence; his 5-check framework is worth adopting into our retarget QA.
- `mercs2-skinner` `src/repoint.js` = additive "new coexisting asset" route — already respects our
  `no-destructive-replacements` mandate. Its stopgap texture path supersedes once Modkit's texture bug
  ships fixed.
- `wad-simulator-js` = faithful JS port of our Rust `wad_simulator` — useful as a cross-check oracle.
- `mercs2-lua-web-ide-ai` advertises an "11-tool agent" driving the bridge — look directly if we want the
  agent-drives-the-live-game pattern (provider/tools unconfirmed from web).

## Phasing
1. **Close the transport gap** — add a WebSocket/HTTP listener (or TCP↔WS shim) to his C bridge speaking
   the `EssBridge` protocol. Unblocks his whole web fleet AND becomes our clients' transport. This is the
   single highest-value first move.
2. **Adopt into `mercs2_bridge`** — wrap/keep his wire; our clients (egui Workshop, Tauri Modkit) speak
   `EssBridge`. Reconcile the untracked `tools/mercs2-{skinner,mesher}` copies.
3. **Ship the Lua console** in the Workshop (capability ladder #1) against retail via the bridge.
4. **Enrich in Ess** — names-on-wire + typed ECS reads (220-class registry) + state-machine control,
   added at the Lua level (extend Ess), native primitive only where a typed read demands it.
5. **Serve the same protocol from the reimpl engine**; do writes/hot-reload (#3/#4) there where cheap.

## Grounding pointers
- memory: `pmc-bb-native-lua-logging`, `decompiled-lua-corpus`, `ecs-component-registry-corpus`,
  `name-registry-spawn-by-hash`, `mercs2-workshop-devtool` (destruction orchestrator `FUN_004cf340`).
- `no-debug-probes-in-game-exe` mandate (bridge must be an additive host, not game-exe flags).
- The reimpl engine (`mercs2_engine` + `mercs2_script` Lua host) is the low-cost target for writes.
