# mercs2_net

The game-side `Net*` replication layer of Mercenaries 2, reimplemented: replicated event/RPC messages, the two-player session model, the host replicate gate, and the join-time Lua module pull.

## What it is

A library crate. It supplies the deterministic **Layer-1** networking mechanism the original engine
owns — the part that decides *what leaves the host* and *what a joining client must pull first* —
and nothing else:

- **Message record.** A replicated event is a 32-bit name-hash + a category nibble + a stream of at
  most 7 typed args (`Str`/`Int`/`Float`/`Handle`, the four recovered per-arg type tags). `NetMessage`
  marshals and unmarshals that record.
- **Session.** `Session` is the host/client role plus the **max-2** player-slot table. Co-op is a
  two-player title; the slot table refuses a third peer.
- **Host gate.** `should_replicate(is_host, player_count)` is the local-vs-wire predicate: a message
  goes to the wire **only** when this peer is the host *and* a client occupies the second slot. A solo
  host stays local. A client never replicates.
- **Replicate routing.** `route()` mirrors the engine's marshal core: local-post always happens; the
  wire marshal is the host-gated addendum. `deliver()` is the receive-side decode.
- **Categories.** `NetCategory` names the recovered per-object property-sync set: one primary class
  plus eight `NetSubCat*` sub-streams (health, node health, inventory, seat link, powered gate,
  friend-or-foe, is-important), each independently serialized.
- **Module pull.** `ModulePullState` gates inbound events: an event for a Lua module this peer has not
  synced yet produces a **pull request** to the host before the event is allowed to deliver.

Layers 2 and 3 — the Winsock peer mesh, the FESL/EA services, the OpenSSL-0.9.8 TLS — are dead
transports that the code map marks *replace-don't-port*. This crate does **not** contain or depend on
them; the online-restore mod stands in for that surface.

## Where it comes from

Derived from `docs/reverse_engineer/networking_code_map.md`, with `event_bus_code_map.md` §4 (the
shared bus the wire branch rides) and `ai_code_map.md` §2.2 (the `DirectAction` replicate gate).
Silo 16 / scoreboard row 28 of `docs/modernization/reimplementation_parallelization_plan.md`.

What is recovered first-hand and modeled here:

| Piece | Oracle |
|---|---|
| Wire record layout (name-hash, header nibble, type-tag switch, argc ≤ 7) | Xbox receive decoder `NetEventCallback` @`825d3ce8` (§2.1) |
| Frame slot budget `0x801` (`(end-cursor)>>3`) | PC marshal core `FUN_005a0cc0`, byte-identical to Xbox `FUN_82878c50` (§2.2) |
| Max-2 player slots | `FUN_006cdac0` / Xbox `FUN_82590e28` (§5, AI map §2.2) |
| `NetCategoryInfo` descriptor: pool `0x100`, seed `0x9e3779b9`, `CopyFromStream` vtable | `FUN_00644510` (§3) |
| Category / sub-cat symbol names | Xbox `.rdata` (§3) |
| Join-time module pull, registry hash `0x762c8f61` | Xbox `SynchNetImportModule` @`825ce918` (§4) |

The **exact PC on-wire bytes are not recovered.** The routing predicate and the encode/emit steps of
`FUN_005a0cc0` are SecuROM-virtualized VM residue (`thunk_FUN_02ee0000` / `_02935000` / `_024f28e0`),
readable only live. So `NetMessage::marshal` emits a *marshal boundary* — a self-consistent,
round-trippable byte form that preserves every recovered field — rather than a fabricated claim about
the virtualized wire encoding.

## Usage

```rust,ignore
use mercs2_net::{Dispatch, NetArg, NetWorld, Received};
use mercs2_formats::hash::pandemic_hash_m2;

// Host side: Net.StartServer, then a client joins.
let mut host = NetWorld::host(0x1000);
host.session.join(0x2000);

// Net.SendCustomEvent("MrxFactionManager", 42, { handle }) — the NETEVENT_* id
// rides as the first Int arg, the channel name is the event name-hash.
let bytes = match host.send_custom_event("MrxFactionManager", 42, &[NetArg::Handle(0xBEEF)])? {
    Dispatch::Wire(bytes) => bytes,   // host + client → replicated
    Dispatch::LocalOnly => return Ok(()), // solo host → stays on the local bus
};

// Client side: the module-pull gate runs before delivery.
let mut client = NetWorld::client(0x1000, 0x2000);
match client.receive(&bytes)? {
    Received::PullFirst { pull, deferred } => {
        // your transport emits `pull.marshal()?` to the host; once the host's module
        // snapshot is applied, mark it synced and re-deliver `deferred`.
        let _pull_bytes = pull.marshal()?;
        client.mark_module_synced(pandemic_hash_m2("MrxFactionManager"));
        let _ = deferred;
    }
    Received::Deliver(msg) => {
        // re-drive `msg` onto the local event bus, exactly as a local Event.Post would
        let _ = msg;
    }
}
```

The host gate on its own, without a `NetWorld`:

```rust
use mercs2_net::{should_replicate, Session};

let mut s = Session::host(1);
assert!(!should_replicate(s.is_host(), s.player_count())); // solo host → local only
s.join(2);
assert!(should_replicate(s.is_host(), s.player_count()));  // host + client → wire
```

## Modules

- `message` — `NetMessage` / `NetArg` / `WireError`: the recovered wire record and its marshal
  boundary. `MAX_ARGS` (7), `FRAME_SLOT_CAP` (`0x801`).
- `session` — `Session` / `Role` / `MAX_PLAYERS` (2) and `should_replicate`, the host gate.
- `replicate` — `route` / `deliver` / `Dispatch` / `frame_has_room`: the local-vs-wire branch and the
  recovered frame capacity check.
- `category` — `NetCategory` (primary + 8 sub-cats) / `NetChannel` (`NetCommand`, `NetNotify`) and the
  `NetCategoryInfo` registrar constants.
- `module_pull` — `ModulePullState` / `pull_request` / `IMPORT_MODULE_REGISTRY_HASH`: the join-time
  module-sync gate.

The crate root adds `NetWorld` (session + module state, the world-global net spine the `Net.*` Lua
surface drives) and `Received`.

## Notes / gotchas

- **Argc is hard-capped at 7.** The header packs argc in `>>1 & 7`; an 8th arg is unrepresentable and
  `marshal` returns `WireError::TooManyArgs`.
- **The category nibble → `NetSubCat*` mapping is NOT recovered.** The Xbox `.rdata` gives the sub-cat
  names; the data table behind `FUN_00644510` that assigns their numeric nibbles is not. `NetMessage`
  therefore carries the raw 4-bit `category` field and `NetCategory` deliberately exposes no nibble.
- **Per-object replicated properties do not live here.** Health / inventory / node-health ride on ECS
  components in the `World`; this crate is the session-level spine only.
- **Only the host replicates.** `route` on a client session always returns `Dispatch::LocalOnly` — the
  client is never authoritative.
- The crate root's `NetWorld::receive` runs the module-pull gate *before* decode delivery, matching
  `NetEventCallback`, which calls `SynchNetImportModule` first.
