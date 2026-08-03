# Workshop "Mods" rebuild — Plan 03: the live bridge

**Status:** ★ WRAPPED (2026-08-03) — bridge built from Wally's C source, and the enrichment thesis shipped as
three live-validated Ess PRs (`Ess.Names` MERGED, `Ess.Machine` + `Ess.Inspect` OPEN). See the WRAPPED block
below (it also corrects a Phase-4 binding error). All phases 1–5 built; only a generic ECS-component read (a
native, Wally's side) is left.
**Siblings:** `-01-mod-model.md`, `-02-navigation.md`, `-04-manifest-format.md`

> ## ★ WRAPPED (2026-08-03) — the bridge was BUILT and the enrichment thesis SHIPPED, live-validated
>
> The "last unbuilt pillar" is built. Wally's lua-bridge was compiled from
> `Desktop/Merc2-Mods-Exp/mods/lua-bridge-DEV` (i686 MinGW), deployed into the retail install, and driven
> live over `mercs2_bridge` / `tools/lua_repl.py` — used to VALIDATE everything below in a running game, not
> just compose it. (Repro + gotchas: memory [[ess-names-pr-contribution]].)
>
> **The enrichment thesis (§"REPL → remote inspector") is delivered as three PRs to Wally's Ess library —
> "his layer = the wire, our layer = the meaning" — all live-validated over that bridge:**
> - **Names on the wire → `Ess.Names`** (hash→name, the committed 23k table). **MERGED** (loganw234 PR #1).
> - **State machine as a live control surface → `Ess.Machine`** (`set`/`onChange`/`link`/`print` over the real
>   `ObjectState.*`). Live: forced a real building's 8 nodes to `DestroyedState`, engine fired `OnStateChange`
>   back. **OPEN** (PR #2).
> - **Typed reads, not eval → `Ess.Inspect`** (a typed entity record; recovers the readable name/model from the
>   opaque `Object.GetName`/`GetModelName` handles via `Sys.GuidToString` + `Ess.Names`). Live: Veyron →
>   `civ_veh_car_veyron`. **OPEN** (PR #3).
>
> **★ CORRECTION to Phase 4 below.** "read_state … via the real `GetState`/`GetHealth` natives" is WRONG:
> there is **no `Object.GetState`/`ObjectState.GetState`** — verified against the live-captured `api/natives.json`.
> `GetHealth` is real; object *state* has no getter (inspect via `ObjectState.PrintStateMachine` or the
> `OnStateChange` watcher). `mercs2_destruction/live.rs` was emitting three non-existent bindings
> (`Object.SetState` 2-arg, `Object.GetState`, `Object.GetStateName`) and is now **fixed** (`ObjectState.SetState`
> 3-arg node-keyed; `print_machine_lua`; `OnStateChange` + `Sys.GuidToString`); the Workshop destruction panel
> gained a `node` field and "Read state" → "Dump machine".
>
> **The generic ECS read — split and half-shipped.** Its NAMING half is delivered: **`Ess.Ecs`** (PR #4) ships
> the 232-class component registry as a typed vocabulary (name / family / `pandemic_hash_m2` hash — the key the
> resolver takes). `Ess.Inspect` covers the CURATED read (components the engine exposes via getters). **The only
> genuinely-open residual is the RAW per-entity read** — dump an arbitrary component's fields off an arbitrary
> entity — which needs a native memory-read verb the bridge doesn't expose. The path is fully reversed
> (object→component resolver `FUN_005857e0`; entity 256-slot component table at `+8`; `0x9e3779b9` pools) and
> `Ess.Ecs`'s hashes are its keys, so a future native (a PR to a fork of the C bridge `Merc2-Mods-Exp`) plugs
> straight in.

> ## What landed (2026-08-01)
>
> - **Phase 1 (TCP↔WS shim) — OBVIATED.** Wally's v0.5.0+ ASI already serves both raw-TCP and
>   WebSocket on the one loopback port 27050, auto-detecting the transport, so browsers reach it
>   directly and a native tool opens a socket — nothing to bridge. (Detail below.)
> - **Phase 2 (`mercs2_bridge`) — built.** A std-only, rate-limited, timeout-bounded raw-TCP client
>   for the `<<<RUN>>>`/`<<<END>>>` protocol, driven from a worker thread. Protocol proven against a
>   mock REPL (no game needed).
> - **Phase 3 (Lua console) — built.** `Craft ▸ Console`: a chunk runs in the live game, the result
>   returns, `0xHASH` is enriched to `0xHASH (name)` from the Workshop's name pack. Worker thread +
>   mpsc so the frame loop never blocks.
> - **Phase 4 (typed reads + ECS registry) — built (2026-08-01).** `mercs2_destruction::live` gained
>   `read_state_lua` / `read_health_lua` (the current destruction state, resolved to the cracked
>   vocabulary, and health — via the real `GetState`/`GetHealth` natives). The **220-class native ECS
>   component registry** (`mercs2_workshop::ecsreg`, 232 classes / 9 families, bundled from the
>   vendored `docs/mercs2-ecs/`) is the typed vocabulary; the console reference searches it alongside
>   the Ess callables, and the destruction poke gained Read-state / Read-health buttons. Open sliver: a
>   GENERIC per-component read (arbitrary ECS class of an arbitrary entity) waits on Ess growing a
>   component-read verb — Wally's repo.
> - **Phase 5 (reimpl serves the protocol) — built (2026-08-01).** `mercs2_bridge::Server` is the
>   transport counterpart to `Bridge` (client↔server roundtrip tested). `mercs2_game` hosts it: a
>   worker thread accepts connections and hands each chunk to `Mercs2Game::update`, which evaluates it
>   on the main thread's Lua VM and answers — the ASI's own "queues to the next frame" model. The same
>   console drives retail or the reimpl over one socket. Open sliver: full `Object.*`/`Ess.*` binding
>   parity in the reimpl VM, which widens as the engine's bindings do.

> ## Standing (2026-08-01) — ★ CORRECTED against Wally's v0.5.0+ ASI
>
> This is the last unbuilt pillar, and reading Wally's current C source
> (`loganw234/Merc2-Mods-Exp/mods/lua-bridge-DEV`, permission to build on it granted) **obsoletes
> phase 1**:
>
> - **G1 (the TCP↔WS shim) is OBVIATED.** Plan 03 called it "the single highest-value first move"
>   because his ASI was raw-TCP only and his browser clients speak WebSocket. His **v0.5.0+ ASI now
>   serves BOTH transports on the one port `127.0.0.1:27050`, auto-detecting** raw-TCP vs a full
>   WebSocket handshake (bcrypt SHA-1 + base64, ≤16 WS clients). Browsers reach it directly; nothing
>   bridges TCP↔WS anymore.
> - **Protocol, confirmed from source:** raw-TCP is clean request/response — send `<lua chunk>` +
>   `<<<RUN>>>`, chunk queues to the next engine frame, result returns + `<<<END>>>`. The WS side is a
>   `type`-tagged *broadcast* line channel (`ws_broadcast_typed_line`), not per-request correlated.
>   So a console (request→response) uses **raw-TCP**, and our native egui Workshop opens a TCP socket
>   directly — **we need no WebSocket on our end at all.**
> - **Script lifecycle maps onto our tooling:** his loader runs `scripts/OnBoot` / `OnLoad` / `OnKey`,
>   which is exactly `place_file`'s `on_boot` / `on_load` / `on_key` destinations — a Shipment can
>   drop Lua into his loader.
> - **Unblocked:** the old mlua/`mercs2_luac` symbol clash is resolved (`mercs2_script` v2.0.0 runs
>   one VM), so a compile-and-talk-to-a-live-VM Workshop is fine.
>
> **So the build is: G2 `mercs2_bridge`** — a raw-TCP client speaking `<<<RUN>>>`/`<<<END>>>`,
> rate-limited and poll-not-block (his measured costs: `Tcp.Send` ~15 ms, `Loader.Printf` ~5 ms; the
> retail game is frame-sensitive), consumed by the Workshop and Modkit — **then G3 the Lua console**
> as a Craft mode with `0xHASH ⇄ name` enrichment (we hold the name pack). Phases 4–5 (enrich his Ess
> Lua; serve the protocol from the reimpl engine) depend on external work and stay open here.

## Why this matters most (the loop, not a capability)

Today's authoring loop: author → build WAD → boot game → navigate → look. Minutes per iteration, and
behavior work ("does the gate lower?") is nearly untestable. The live bridge attacks the LOOP. It is
the highest-leverage item on the roadmap because it multiplies every other capability's iteration speed.

The pieces already exist: the game hosts a Lua 5.1 VM; we have the decompiled Lua corpus; spawn-by-hash
works; `pmc_bb` gives native logging; we have full symbol coverage of the exe.

## Ownership (settled)

We are NOT merely consuming an external bridge. **Wally (loganw234) gave permission to adapt his Lua
bridge code into Workshop/Modkit**, and the intent is to ENRICH it with our RE knowledge, not just
lift it. He has been building the Lua-bridge side (plus a webmap, a Lua web IDE, a stopgap skinner, and
a Lua framework "essentials"/"ess") while leaving the bigger engine features to us. So:

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

## Performance discipline (a REAL constraint)

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
  were DELETED from disk — no reconcile needed. If we later want his JS tools, pull fresh
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
