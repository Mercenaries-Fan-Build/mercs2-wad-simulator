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
  game Lua does not call directly (Flash calls them). The harness scopes to the **37 engine namespaces /
  1092 cfuncs** the game Lua binds against. If a system ever needs the AS2 surface it should live under a
  distinct `bindings_gfx` tree, not here.

- **`Debug.Printf` is intentionally a real body, not the retail stub.** `[faithful-blocker: no]` On
  retail every `Debug.*` routes to the `0x006D5640` return-0 stub. The reimpl backs `Printf` with a real
  log sink because the `[lua]` stream is load-bearing for bring-up; the other five `Debug.*` are faithful
  no-ops. This is a deliberate, bring-up-only divergence (pre-existing).

- **`Event.ObjectHibernation`'s model-detail gate is not modelled.** `[faithful-blocker: no]` The
  condition's optional fourth argument (`+0x2C`, awake form only) makes retail's entity test also
  require the entity's model resource loaded at that detail level (`0x0246F220`,
  `docs/reverse_engineer/hibernation_grid.md` §6a). No shipped script passes one — the only site that
  forwards a detail, `MrxUtil.SpawnActor`'s `nDetail`, passes nil — so `EngineHost::object_hibernation_passes`
  makes a detail a hard error instead.

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

- **`Sys.RealTime`/`RealTimeStamp`/`Time`/`Date` report game time, not wall time.** `[faithful-blocker: no]`
  All `Sys` time bindings read the VM's game clock (`bindings::ScriptClock`, advanced only by
  `Event.__pump(dt)`), so scripted behaviour is reproducible from the same ticks. Retail's `RealTime*`
  presumably keeps running through pauses and time scaling while `MainTime*` does not (INFERRED from the
  names; the cfunc bodies are not read), and `Time`/`Date` presumably read the system clock. In the
  shipped corpus `RealTimeStamp` has one call site and `Time`/`Date` have none, and nothing in this host
  pauses or scales the clock yet, so no script can observe the difference today. UNBLOCK: read the
  `Sys.RealTime`/`Sys.Time` cfuncs (table VA in `sys.rs`) and, if they are wall-clock, give the clock a
  second, real-time track that a host may drive from the wall clock and a harness from its virtual one.
