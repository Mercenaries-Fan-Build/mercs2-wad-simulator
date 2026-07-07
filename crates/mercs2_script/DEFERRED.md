# mercs2_script — deferred items

Non-blocking improvements and known-imperfect calls, deferred out of the current change. The exe is
the oracle; only `[faithful-blocker: yes]` items can change observable game behavior.

## Wave-0 E3 (Lua binding harness)

- **Namespace global-name uncertainty for low-usage tables.** `[faithful-blocker: no]` The per-table
  `GLOBAL` in `src/bindings/*.rs` was data-derived from the dominant `Global.Func(` prefix in the
  decompiled corpus (`docs/mercs2-luacd`). For tables the game barely calls, the corpus signal is weak,
  so a few globals are best-effort labels pending a confirm-live read of the `luaL_register` registrar
  (the open item in `scripting_host_binding_code_map.md` §1.2/§6): `ObjectFilter` (doc "Filter"),
  `Report` (doc "Infraction"; entries `Init/GetInfractions/Completed/Failed/SetDelay`), `Timer`,
  `Lti` (corpus prefix was the module-local `LTILibName`), `Fade`, `Bloom`, `CameraFx`. The coverage
  `NAMESPACE` keys and `TABLE_VA`s are exact regardless; only the installed Lua global name is at issue,
  and it only matters once a silo wires real bodies. Confirm-live via a read bp on the table VA during
  init to pin the true global.

- **`Pg` and `Camera` each span two luaL_Reg tables.** `[faithful-blocker: no]` `Pg` = table
  `0x00b99328` (`pg.rs`, has `Spawn`/`GetGuidByName`) **and** `0x00b99e28` (`pg_world.rs`, spawn/asset
  dump). `Camera` = `0x00b9a530` (`camera.rs`) **and** `0x00b9a7d8` (`camera_fx.rs`). They are kept as
  separate coverage keys (one file per table) but install into the same Lua global. When a silo fills
  the second table of a pair it must merge into the existing global table rather than overwrite it.

- **Scaleform AS2 method tables excluded from the harness.** `[faithful-blocker: no]` The live trace
  flags 53 `.rdata` tables "game", but ~18 are the GFx 2.0.48 ActionScript runtime (MovieClip, TextField,
  Array, XML, geom.*, ColorTransform, Selection, Stage, Mouse, Key, …) — a *separate* scripting VM the
  game Lua does not call directly (Flash calls them). The harness scopes to the **35 engine namespaces /
  1086 cfuncs** the game Lua binds against. If a silo ever needs the AS2 surface it should live under a
  distinct `bindings_gfx` tree, not here.

- **`Debug.Printf` is intentionally a real body, not the retail stub.** `[faithful-blocker: no]` On
  retail every `Debug.*` routes to the `0x006D5640` return-0 stub. The reimpl backs `Printf` with a real
  log sink because the `[lua]` stream is load-bearing for bring-up; the other five `Debug.*` are faithful
  no-ops. This is a deliberate, bring-up-only divergence (pre-existing).
