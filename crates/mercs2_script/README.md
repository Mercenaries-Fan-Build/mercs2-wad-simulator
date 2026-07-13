# mercs2_script

The engine's Lua script host: the VM, the `import`/`inherit` module system, and the engine binding
tables (`Sys.*`, `Pg.*`, `Object.*`, …) that the game's Mercenaries Lua runs on top of.

## What it is

A library crate with three parts:

* **The VM.** `ScriptHost` embeds Lua 5.4 (`mlua`, `vendored` — Lua is built from source by the same
  `cc` toolchain wgpu uses, so no system Lua is required) and loads a **5.1 → 5.4 compat prelude**:
  `unpack`, `loadstring`, `table.getn`, `math.mod`, `string.gfind`, plus `getfenv`/`setfenv` shims
  implemented over 5.4's `_ENV`-as-upvalue model via `debug.getupvalue`/`setupvalue`. The prelude also
  supplies the engine's own math extension `math.randi(n)` / `math.randi(a,b)` and aliases the engine's
  capitalized `Math` namespace onto it.
* **The module system.** `import(name)` / `dynamic_import(name)` resolve a module name to a `.lua` file
  under the configured roots (index is by **lowercased file stem**; first root wins), execute it in its
  own environment table whose metatable `__index` chains to `_G`, cache it in `_MODULES`, and bind it as
  a global. `inherit(base)` loads `base` and points the currently-defining module's `__index` at it.
  A module's parameterless `Init()` is auto-invoked by the loader, **deferred**: all modules load first,
  then the queued `Init`s run in load order (running them eagerly fires a module's `Init` while a
  dependency is still half-loaded).
* **The binding surface.** 35 engine namespaces — one `src/bindings/<ns>.rs` per `luaL_Reg` table —
  covering **1086 required cfuncs**. Each file declares `NAMESPACE`, `GLOBAL`, `TABLE_VA`, the full
  `REQUIRED` cfunc list (with per-name corpus call counts), and an `install()` that wires this build's
  bodies. `install_all` returns per-namespace `NsCoverage`, from which `coverage_json` writes the
  machine-readable `binding_coverage.json`. Current baseline (asserted by the `coverage_report` test):
  **1086 required / 1058 real / 28 stub / 0 missing**, i.e. 28 "stubs remaining".

The crate is **asset-agnostic** — it knows nothing about Mercenaries. The bindings never call the
engine directly: the engine implements the `EngineHost` trait and hands it in via
`ScriptHost::register_engine`, so the dependency points *into* this crate — the same shape as the
original `Sys.*` C-binding table calling into the native engine. The game's `Mrx*` / mission / contract
modules are *game* Lua and arrive from the decompiled corpus through `import`, not from here.

## Where it comes from

* The **cfunc surface** (`REQUIRED`, per-namespace) is seeded from the live Surface-B binding trace
  `mods/lua_trace_asi/reference/binding_map.json`; the human index is
  `docs/reverse_engineer/scripting_host_binding_code_map.md` §3. Each namespace records the `luaL_Reg`
  table's VA in the unpacked image (e.g. `Pg` = `0x00b99328` — the trace corrected the doc's label).
  `corpus_calls` per name = call sites counted in the decompiled game Lua (`docs/mercs2-luacd`).
  The list is **never trimmed**: a name only leaves the "stubs remaining" tally by gaining a real body.
* The compat prelude is exactly the 5.1 constructs measured across the 409-file corpus (the heavy ones —
  `setfenv`/`module`/`loadstring`/`table.setn` — are 0 files, so it stays small).
* `stub` bodies model the retail engine's deliberate no-ops (on retail every `Debug.*` routes to the
  `0x006D5640` return-0 stub) *and* mark bindings whose engine system isn't built yet; the burn-down is
  tracked in `docs/modernization/binding_burndown.md`.
* Realizes charter **Phase 3** (embed Lua 5.4, run migrated scripts validated by Surface B) and the
  engine/game split in `docs/modernization/pangea_engine_alignment.md`.

## Usage

```rust
use std::cell::RefCell;
use std::rc::Rc;
use mercs2_script::{EngineHost, ScriptHost};

/// Minimal host: only these nine `EngineHost` methods have no default body.
struct MyHost;
impl EngineHost for MyHost {
    fn log(&mut self, _source: &str, msg: &str) { println!("[lua] {msg}"); }
    fn get_level_name(&self) -> String { "vz".into() }
    fn guid_by_name(&mut self, _name: &str) -> u64 { 0 }        // 0 → Lua nil
    fn pg_spawn(&mut self, _t: &str, _p: [f32; 3], _yaw: f32, _hi: bool) -> u64 { 1 }
    fn object_set_name(&mut self, _guid: u64, _name: &str) {}
    fn object_set_position(&mut self, _guid: u64, _pos: [f32; 3]) {}
    fn object_set_yaw(&mut self, _guid: u64, _yaw: f32) {}
    fn teleport_hero(&mut self, _pos: [f32; 3]) {}
    fn add_layers(&mut self, _layers: &[String]) {}
}

// Roots are the corpus dirs `import`/`inherit` resolve against (e.g. docs/mercs2-luacd/src).
// `ScriptHost::bare()` = no roots, bindings only.
let sh = ScriptHost::new(vec!["docs/mercs2-luacd/src".into()])?;
sh.register_engine(Rc::new(RefCell::new(MyHost)))?;

// The real MrxUtil.SpawnActor recipe against the real Pg.* / Object.* bindings.
let guid: i64 = sh.eval(r#"
    local uGuid = Pg.GetGuidByName("HqInterior")
    if not uGuid then uGuid = Pg.Spawn("PmcHqInterior", 0, 0, 0, 0, false, true) end
    Object.SetName(uGuid, "HqInterior")
    Object.SetPosition(uGuid, 3750, 450, -3840)
    return uGuid
"#)?;

// Advance the world-load state machine's Lua side.
sh.fire_state_change("WaitForStreaming", "enter")?;
# Ok::<(), mlua::Error>(())
```

Other public entry points: `ScriptHost::import_module(name)` (load + cache a corpus module),
`ScriptHost::exec(src, name)`, `ScriptHost::lua()` (the raw `mlua::Lua`),
`ScriptHost::register_engine_reported(host)` (same as `register_engine` but returns
`Vec<NsCoverage>`), and `ScriptHost::enable_autostub(sink)` — a bring-up layer that installs a `_G`
metatable so a read of an as-yet-unwired **Capitalized** global resolves to a logged, indexable+callable
no-op stub instead of erroring; every stubbed name lands in `sink` (a reimpl-side Surface-B trace of what
the real scripts touch). Call it *after* `register_engine` so real bindings win.

Regenerate the reports:

```
cargo test -p mercs2_script coverage_report      # rewrites binding_coverage.json
cargo test -p mercs2_script --test game_hooks    # rewrites hook_coverage.json
```

`tests/binding_smoke.rs` asserts every one of the ~1086 `Required` cfuncs resolves to a callable Lua
function after `register_engine`; `tests/game_hooks.rs` runs real corpus hook patterns against the
bindings backed by a `HarnessHost` (which uses `mercs2_ai` so the `Ai.*` hooks hit the real action ring).

## Modules

* `bindings` — the binding harness (`Required`, `Installed`, `NsBuilder`, `NsCoverage`, `install_all`,
  `totals`, `coverage_json`) plus one private module per engine namespace: `ai`, `airstrike`,
  `atmosphere`, `bloom`, `camera`, `camera_fx`, `debug`, `event`, `face`, `fade`, `fire`, `graphics`,
  `gui`, `hud`, `human`, `inventory`, `lti`, `math_ns` (`Math`), `net`, `object`, `object_filter`,
  `object_state`, `pg`, `pg_world`, `player`, `report`, `socket`, `sound`, `string_ext` (`String`),
  `sys`, `sys_module` (`_SYS`), `timer`, `vehicle`, `vo`, `weapon`.

The crate root exports `ScriptHost`, `EngineHost`, `SharedHost`, and re-exports `install_all`,
`coverage_json`, `totals`, `NsCoverage`, `Totals`.

## Notes / gotchas

* **Single-threaded by design.** `SharedHost = Rc<RefCell<dyn EngineHost>>` — the VM and the engine live
  on the same thread (the render loop is `pollster::block_on` on main), so nothing here is `Send`.
* The VM is created with `Lua::unsafe_new_with(StdLib::ALL, …)`: the `debug` library is required by the
  `getfenv`/`setfenv` shims. This host runs **trusted** decompiled game Lua.
* **`Pg` and `Camera` each span two `luaL_Reg` tables** (`pg` `0x00b99328` + `pg_world` `0x00b99e28`;
  `camera` `0x00b9a530` + `camera_fx` `0x00b9a7d8`). They are separate coverage keys but install into the
  same Lua global — a later fill must **merge** into the existing table, not overwrite it. A `pg_world`
  install clobbering `Pg.GetGuidByName`/`Spawn` actually shipped once; `tests/binding_smoke.rs` exists to
  name that failure immediately.
* **`stub` is not "done"** — it means the engine system behind the binding isn't built yet (or the retail
  cfunc is genuinely stripped). The `coverage_report` test asserts exact `EXPECTED_REAL` /
  `EXPECTED_STUB` baselines, so landing bodies requires bumping them (they must move in opposite
  directions; `remaining` only ever goes down).
* A few installed Lua **global names** are best-effort labels pending a confirm-live read of the
  `luaL_register` registrar (`ObjectFilter`, `Report`, `Timer`, `Lti`, `Fade`, `Bloom`, `CameraFx`) — see
  `DEFERRED.md`. The `NAMESPACE` keys and `TABLE_VA`s are exact regardless.
* The ~18 Scaleform/GFx 2.0.48 ActionScript-2 method tables in the live trace are **out of scope** here:
  that is a separate VM the game Lua does not call directly. The harness scopes to the 35 engine
  namespaces the game Lua binds against.
* `Debug.Printf` is a deliberate bring-up divergence: retail routes it to the return-0 stub, but the
  `[lua]` log stream is load-bearing, so it gets a real body. The other five `Debug.*` are faithful
  no-ops.
* Bindings map a `0` GUID to Lua `nil`, so the game's `if not uGuid` control flow stays authentic.
