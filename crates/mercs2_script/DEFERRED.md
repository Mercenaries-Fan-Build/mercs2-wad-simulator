# mercs2_script — deferred items

Non-blocking improvements and known-imperfect calls, deferred out of the current change. The exe is
the oracle; only `[faithful-blocker: yes]` items can change observable game behavior.

## Lua binding harness

- **Namespace global-name uncertainty for low-usage tables.** `[faithful-blocker: no]` The per-table
  `GLOBAL` in `src/bindings/*.rs` was data-derived from the dominant `Global.Func(` prefix in the
  decompiled corpus (`docs/mercs2-luacd`). For tables the game barely calls, the corpus signal is weak,
  so a few globals are best-effort labels pending a confirm-live read of the `luaL_register` registrar
  (the open item in `scripting_host_binding_code_map.md` §1.2/§6): `ObjectFilter` (doc "Filter"),
  `Report` (doc "Infraction"; entries `Init/GetInfractions/Completed/Failed/SetDelay`), `Timer`,
  `Lti` (corpus prefix was the module-local `LTILibName`), `Fade`, `Bloom`, `CameraFx`. The coverage
  `NAMESPACE` keys and `TABLE_VA`s are exact regardless; only the installed Lua global name is at issue,
  and it only matters once a system wires real bodies. Confirm-live via a read bp on the table VA during
  init to pin the true global.

- **`Pg` and `Camera` each span two luaL_Reg tables.** `[faithful-blocker: no]` `Pg` = table
  `0x00b99328` (`pg.rs`, has `Spawn`/`GetGuidByName`) **and** `0x00b99e28` (`pg_world.rs`, spawn/asset
  dump). `Camera` = `0x00b9a530` (`camera.rs`) **and** `0x00b9a7d8` (`camera_fx.rs`). They are kept as
  separate coverage keys (one file per table) but install into the same Lua global. When a system fills
  the second table of a pair it must merge into the existing global table rather than overwrite it.

- **Scaleform AS2 method tables excluded from the harness.** `[faithful-blocker: no]` The live trace
  flags 53 `.rdata` tables "game", but ~18 are the GFx 2.0.48 ActionScript runtime (MovieClip, TextField,
  Array, XML, geom.*, ColorTransform, Selection, Stage, Mouse, Key, …) — a *separate* scripting VM the
  game Lua does not call directly (Flash calls them). The harness scopes to the **35 engine namespaces /
  1086 cfuncs** the game Lua binds against. If a system ever needs the AS2 surface it should live under a
  distinct `bindings_gfx` tree, not here.

- **`Debug.Printf` is intentionally a real body, not the retail stub.** `[faithful-blocker: no]` On
  retail every `Debug.*` routes to the `0x006D5640` return-0 stub. The reimpl backs `Printf` with a real
  log sink because the `[lua]` stream is load-bearing for bring-up; the other five `Debug.*` are faithful
  no-ops. This is a deliberate, bring-up-only divergence (pre-existing).

- **`ObjectHibernation` phase matching folds five spellings to two phases; retail's predicate is
  unrecovered.** `[faithful-blocker: no]` The corpus spells the phase five ways across 109
  registrations — `"awake"`×80, `"a"`×4, `"hibernated"`×19, `"s"`×5, `"asleep"`×1. That these are
  exactly **two** phases is settled by what the handlers do (`_OnAsleep` `wifpmcgarage.lua:364`,
  `_OnHqHibernation` `mrxhqmanager.lua:185`, `Object.Remove` on stream-out `oilcon001.lua:1727`, versus
  the wake-gates), so `bindings::event::canon_phase` folds by author intent. Retail's actual comparison
  is **not** recovered: `event_bus_code_map.md:36` reaches "installs match predicate via
  `vt[0](filter_args)`" and stops before the per-type predicate. What is certain is that retail cannot
  be doing an exact match (that would strand 28 shipped registrations, including `vzacon001.lua:120`,
  the gate the world-load machine waits on) nor a first-character match (`"awake"` and `"asleep"` share
  `'a'` and mean opposites).

  **The one site that could differ:** `gurcon002.lua:194` is the lone `"asleep"`. If retail's predicate
  happens to key on `'a'`, that registration fires on *wake* and is a shipped bug — in which case we are
  fixing a bug rather than reproducing it, contrary to the standing rule. UNBLOCK: a live read of the
  `KIND_OBJECT_HIBERNATION` match predicate installed by `FUN_005eb480`'s per-type factory
  (`PTR_LAB_00d12274[type*3]`). One site; do not raise `canon_phase`'s confidence without that read.

- **Layer objects wake on layer-load completion, not on a per-object streaming tick.**
  `[faithful-blocker: no]` `worldutil::layer_index` + the pending-wake drain in `pump_resident` fire
  `ObjectHibernation`/`"awake"` for every named object in a layer, one pump after that layer's load
  completes. Retail wakes each object as its own `HibernationControl` distance is crossed
  (`world_streaming_code_map.md` §4), so retail's wakes are spread over time and gated on player
  proximity while ours arrive together. The event set is the same; the timing and ordering within a
  layer are not. The per-entity distance path already exists in the engine (`StreamDiff.wake` from
  `mercs2_core::streaming`) for `layers_static` props — joining the two so layer objects also enter the
  distance-driven manager is the follow-up.

- **`Vehicle.Enter`/`Exit` are instantaneous; retail plays a mount/dismount animation.**
  `[faithful-blocker: no]` `GameScriptHost::vehicle_enter` records occupancy and queues the
  `ObjectInSeat` event on the next pump. Retail routes the same transition through the animated
  mount/dismount set (`FUN_00540690`/`FUN_00538fe0`/`FUN_0053a9f0`/`FUN_00540990`, dispatched by
  `FUN_0053f110` on `rec+0xc` ∈ {0,4,8,0xc,0x10} — `vehicle_code_map.md` §1), which takes real time and
  can be interrupted (hijack abort, seat blocked). Scripts that *observe* seating see the same event in
  the same order; scripts that depend on the in-between animation state, or on a seat entry failing
  partway, will not. The seat-blocked path (`Vehicle.IsSeatBlocked`) is likewise not consulted.

- **`Event.ObjectInSeat`'s `"Hero"` occupant degrades to any-occupant.** `[faithful-blocker: no]`
  `wifpmcgarage.lua:410` registers `{"Hero", _uFionaCar, "a", "e"}` — a string where every other site
  passes a guid. `bindings::event` cannot resolve "Hero" without the host, so it reads as `Guid::NONE`
  and matches any occupant. With one local player "the hero" and "any character" select the same
  object, so single-player behaviour is identical; in split-screen co-op the filter would fire for the
  second player too. UNBLOCK: resolve the string against the local player's character at registration
  (the `Event` installer already receives the `SharedHost`).

- **Seat code `"a"` is read as an ObjectInSeat filter wildcard.** `[faithful-blocker: no]` INFERRED from
  distribution, not from the exe: `"a"`/`"A"` appears 27 times in the `ObjectInSeat` seat field and
  *never* as a real seat — `Vehicle.Enter` and `Vehicle.GetSeatByType` only ever pass `"d"`/`"p"`. If
  retail instead has an actual seat type `a` (e.g. "any available"), a filter written for it would over-
  match here. UNBLOCK: the seat-type table behind `Vehicle.GetSeatByType` (`vehicle_code_map.md` §1
  applier `FUN_0053f110`).
