//! `ess-menu-check` — run the Ess Lua framework and menu scripts built on it under the game's EXACT
//! Lua 5.1 (`mercs2_luac`, `lua_Number = float`), headless, with the engine stubbed.
//!
//! ```text
//! mercs2_probe ess-menu-check --ess <Ess.lua> <menu.lua> [<menu.lua> ...]
//! ```
//!
//! What it proves, per menu file:
//!
//! 1. `Ess.lua` loads the way the lua-bridge OnLoad loader runs it (one chunk, named after the file),
//!    and leaves `_G.Ess` and `Ess.UI.Menu` behind.
//! 2. The menu file runs the way the OnKey loader runs it (the whole file, every key press), and
//!    every `Ess.UI.Menu` it builds is captured by wrapping `Ess.UI.Menu` (Ess.lua:8075).
//! 3. Every entry is activated through Ess's OWN path: a key byte queued in
//!    `Loader.PopKeyEvents`, the Ess.UI heartbeat (an `Event.TimerRelative` loop, Ess.lua:4664 /
//!    7282) polling it (Ess.lua:4750), the focused list's `_keyvk` (Ess.lua:7435), its `onChoose`
//!    (Ess.lua:7449) into `Menu:_choose` (Ess.lua:7995), which builds the real ctx
//!    (`menu_ctx`, Ess.lua:7908) and `pcall`s the entry (Ess.lua:8002).
//! 4. The file is run again to exercise `Menu:toggle` (Ess.lua:8069) in both directions.
//!
//! Strictness: `_G` and every stub namespace carry an `__index` that RECORDS the access and raises.
//! The record is kept on the Rust side, so an access swallowed by one of Ess's own `pcall`s is still
//! a failure. Nothing is auto-stubbed; the list of names that tripped is the finding.
//!
//! Template names handed to `Pg.Spawn` are recorded, never judged.
//!
//! Chunk names: files run here as `@<basename>`, so errors read `Ess.lua:N:`. In the game the
//! lua-bridge runs files through the SDK's `luaB_loadstring` with no chunk name, so the same errors
//! read `[string "<source start>..."]:N:` there.
//!
//! The whole sequence runs twice, once per FlashWidget load model ([`SwfLoad`]): with the
//! `SetSwfFile` load callback fired on the next timer tick, and with it never fired so Ess's 3 s
//! timeout fallback (Ess.lua:6991-6999) has to bring the UI runtime up. Both must pass.
//!
//! Errors raised by the menu file's own entries are listed in their own section and fail the run
//! unless `--entry-errors-pass` is given. Failures in Ess or the harness (load errors, unstubbed
//! access, stub violations, toggle not working, timer callbacks raising) always fail.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::os::raw::c_void;
use std::path::Path;
use std::rc::Rc;

use mercs2_luac::rt::{Error, Function, LightUserData, Lua, MultiValue, Table, Value};

type LResult<T> = std::result::Result<T, Error>;

/// One Lua source: the name its loader would give it and its bytes.
pub struct Source {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl Source {
    fn read(path: &str) -> Result<Source, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| format!("{path}: no file name"))?;
        Ok(Source { name, bytes })
    }

    /// Lua's `@name` convention: errors read `Ess.lua:123:` rather than `[string "..."]:123:`.
    fn chunk_name(&self) -> String {
        format!("@{}", self.name)
    }
}

const USAGE: &str =
    "ess-menu-check: usage: mercs2_probe ess-menu-check --ess <Ess.lua> [--entry-errors-pass] <menu.lua> [<menu.lua> ...]";

/// How the engine's asynchronous FlashWidget load is modelled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwfLoad {
    /// `SetSwfFile`'s load callback fires on the next timer tick (Ess.lua:6941-6943 path).
    CallbackNextTick,
    /// The callback never fires; Ess's 3 s timeout fallback (Ess.lua:6991-6999) must take over.
    CallbackNeverFires,
}

impl SwfLoad {
    pub const ALL: [SwfLoad; 2] = [SwfLoad::CallbackNextTick, SwfLoad::CallbackNeverFires];

    fn label(self) -> &'static str {
        match self {
            SwfLoad::CallbackNextTick => "SetSwfFile load callback fired on the next tick",
            SwfLoad::CallbackNeverFires => "SetSwfFile load callback never fired (Ess.lua:6991-6999 timeout fallback)",
        }
    }
}

/// CLI entry. Returns the process exit code: 0 all good, 1 a check failed, 2 could not run.
pub fn main(args: &[String]) -> i32 {
    let mut ess: Option<String> = None;
    let mut menus: Vec<String> = Vec::new();
    let mut entry_errors_pass = false;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a == "--ess" {
            match args.get(i + 1) {
                Some(v) if !v.starts_with("--") => ess = Some(v.clone()),
                _ => {
                    eprintln!("{USAGE}\n--ess needs a path");
                    return 2;
                }
            }
            i += 2;
            continue;
        }
        if a == "--entry-errors-pass" {
            entry_errors_pass = true;
            i += 1;
            continue;
        }
        if a.starts_with("--") {
            eprintln!("{USAGE}\nunknown flag '{a}'");
            return 2;
        }
        menus.push(a.clone());
        i += 1;
    }
    let Some(ess) = ess else {
        eprintln!("{USAGE}\n--ess is required");
        return 2;
    };
    if menus.is_empty() {
        eprintln!("{USAGE}\nat least one menu file is required");
        return 2;
    }
    let ess = match Source::read(&ess) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ess-menu-check: {e}");
            return 2;
        }
    };
    let mut sources = Vec::with_capacity(menus.len());
    for m in &menus {
        match Source::read(m) {
            Ok(s) => sources.push(s),
            Err(e) => {
                eprintln!("ess-menu-check: {e}");
                return 2;
            }
        }
    }
    let mut all_ok = true;
    for mode in SwfLoad::ALL {
        match check(&ess, &sources, mode, entry_errors_pass) {
            Ok(report) => {
                print!("{}", report.render());
                all_ok &= report.ok();
            }
            Err(e) => {
                eprintln!("ess-menu-check: could not run ({}): {e}", mode.label());
                return 2;
            }
        }
    }
    println!(
        "\n[ess-menu-check] overall: {} (both load models must pass{})",
        if all_ok { "PASS" } else { "FAIL" },
        if entry_errors_pass { "; --entry-errors-pass: entry errors listed, not failing" } else { "" }
    );
    if all_ok {
        0
    } else {
        1
    }
}

// ─── recorded state ──────────────────────────────────────────────────────────────────────────────

/// What a stub stands for, so the per-entry log can show the gameplay calls an entry made without
/// listing every draw call the menu repaint issued.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// World-affecting or world-reading engine calls (`Pg.Spawn`, `Object.*`, `Player.*`).
    Game,
    /// The FlashWidget / HUD surface Ess.UI draws through.
    Ui,
    /// `Event.TimerRelative` and the `Sys.*TimeStamp` clock.
    Clock,
    /// The lua-bridge `Loader.*` key buffer.
    Input,
    /// `Loader.Printf` and `import`.
    Host,
}

struct Timer {
    due: f64,
    seq: u64,
    f: Function,
    what: String,
}

#[derive(Default)]
struct State {
    now: f64,
    seq: u64,
    timers: Vec<Timer>,
    keys: Vec<u8>,
    stamps: HashMap<usize, f64>,
    handles: HashMap<usize, String>,
    next_handle: usize,
    spawned: usize,
    widgets: usize,
    timer_relative: Option<LightUserData>,
    swf_load: Option<SwfLoad>,
    withheld_swf_callbacks: usize,
    phase: String,
    calls: BTreeMap<String, u64>,
    /// `Some` while an entry is being activated: the Game-kind calls it made.
    entry_calls: Option<Vec<String>>,
    printf: Vec<String>,
    imports: BTreeMap<String, u64>,
    unstubbed: BTreeMap<String, (u64, String)>,
    declared_nil: BTreeMap<String, u64>,
    violations: Vec<String>,
    timer_errors: Vec<String>,
    flash_handlers: BTreeMap<String, u64>,
    script_events: BTreeMap<String, u64>,
    traceback: Option<Function>,
    captured: Vec<Table>,
    activations: Vec<(String, Option<String>)>,
}

type Shared = Rc<RefCell<State>>;

impl State {
    fn handle(&mut self, label: String) -> Value {
        self.next_handle += 1;
        // Distinct, never-dereferenced addresses: a GUID is an opaque pointer-sized handle on this
        // engine (mercs2_luac::rt::value module docs), so a fake one is too.
        let addr = 0x1000_0000usize + self.next_handle * 0x10;
        self.handles.insert(addr, label);
        Value::LightUserData(LightUserData(addr as *mut c_void))
    }

    fn label_of(&self, v: &Value) -> Option<&str> {
        match v {
            Value::LightUserData(p) => self.handles.get(&(p.0 as usize)).map(|s| s.as_str()),
            _ => None,
        }
    }

    fn summarize(&self, v: &Value) -> String {
        match v {
            Value::Nil => "nil".into(),
            Value::Boolean(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => format!("{:?}", s.to_string_lossy()),
            Value::Table(_) => "table".into(),
            Value::Function(_) => "function".into(),
            Value::LightUserData(p) => match self.label_of(v) {
                Some(l) => l.to_string(),
                None => format!("userdata:{:p}", p.0),
            },
            Value::Other(_, t) => (*t).to_string(),
        }
    }

    fn record(&mut self, name: &str, kind: Kind, args: &[Value]) {
        *self.calls.entry(name.to_string()).or_default() += 1;
        if kind == Kind::Game {
            let shown: Vec<String> = args.iter().map(|a| self.summarize(a)).collect();
            let line = format!("{name}({})", shown.join(", "));
            if let Some(log) = self.entry_calls.as_mut() {
                log.push(line);
            }
        }
    }

    fn violation(&mut self, msg: String) -> Error {
        let full = format!("{msg} (during {})", self.phase);
        self.violations.push(full.clone());
        Error::RuntimeError(format!("ess-menu-check: {full}"))
    }

    fn unstubbed(&mut self, name: String, traceback: String) -> Error {
        let phase = self.phase.clone();
        let e = self.unstubbed.entry(name.clone()).or_insert((0, format!("{phase}\n{traceback}")));
        e.0 += 1;
        Error::RuntimeError(format!("ess-menu-check: engine global `{name}` is not stubbed (during {phase})"))
    }
}

/// The Lua call stack at the current point, from the `debug.traceback` captured before `debug`
/// was removed from `_G`. Called with no `State` borrow held.
fn traceback(st: &Shared) -> String {
    let f = st.borrow().traceback.clone();
    match f {
        Some(f) => f.call::<String>(("", 2.0f64)).unwrap_or_else(|e| format!("(traceback failed: {e})")),
        None => "(no traceback function)".into(),
    }
}

/// `__index` for a strict table: record `prefix<key>` with its call stack, then raise. Keys in
/// `declared_nil` (each with its reason) read as nil and are counted instead.
fn strict_index(
    st: Shared,
    prefix: String,
    declared_nil: &'static [(&'static str, &'static str)],
) -> impl Fn(&Lua, (Value, Value)) -> LResult<Value> + 'static {
    move |_, (_t, k): (Value, Value)| -> LResult<Value> {
        let key = match &k {
            Value::String(s) => s.to_string_lossy(),
            other => st.borrow().summarize(other),
        };
        if declared_nil.iter().any(|(n, _)| *n == key) {
            *st.borrow_mut().declared_nil.entry(format!("{prefix}{key}")).or_default() += 1;
            return Ok(Value::Nil);
        }
        let tb = traceback(&st);
        Err(st.borrow_mut().unstubbed(format!("{prefix}{key}"), tb))
    }
}

// ─── the host ────────────────────────────────────────────────────────────────────────────────────

/// Globals read on a fresh state that are legitimately nil in the real game at that point. Each is
/// declared, with its reason, rather than inferred: a read of anything else that is absent fails.
const DECLARED_NIL_GLOBALS: &[(&str, &str)] = &[
    // Ess.lua:40 `_G.Ess = _G.Ess or {}` — the first OnLoad run finds no Ess yet.
    ("Ess", "Ess.lua:40 first-load guard"),
    // Ess.lua:159-163: absent on this engine (removed from the state in Host::install).
    ("debug", "Ess.lua:159 no debug library in the game"),
];

/// The fake local character's pose returned by `Object.GetPosition` / `Object.GetYaw`.
const PLAYER_POS: (f32, f32, f32) = (-1520.25, 12.5, 830.75);
const PLAYER_YAW: f32 = 0.785;

/// Upper bound on heartbeat timer fires spent delivering one key press. A press the heartbeat never
/// drains in this many fires means nothing is polling input — reported, never waited out.
const MAX_FIRES_PER_PRESS: usize = 10_000;

/// Simulated seconds allowed for timers to run down after the last toggle.
const SETTLE_SECONDS: f64 = 30.0;

const HELPERS: &str = r#"
local H = {}
-- exactly Ess.UI.Menu's resolveLabel (Ess.lua:7868-7871)
function H.label(node)
    local l = node.label
    if type(l) == "function" then local ok, v = pcall(l); l = ok and v or "?" end
    return tostring(l or "?")
end
-- capture every menu built, without changing what Ess.UI.Menu returns (Ess.lua:8075-8089)
function H.install_capture(capture)
    local orig = Ess.UI.Menu
    Ess.UI.Menu = function(...)
        local m = orig(...)
        capture(m)
        return m
    end
end
-- observe an entry's outcome; the error is re-raised so Ess's own pcall (Ess.lua:8002) still sees it
function H.wrap(node, path, report)
    local orig = node.action
    node.action = function(ctx)
        local ok, err = pcall(orig, ctx)
        if ok then report(path, true, nil) else report(path, false, tostring(err)) end
        if not ok then error(err, 0) end
    end
end
-- runtime state kept per menu id across OnKey re-runs (Ess.lua:7974-7979, 8049-8054)
function H.state(id)
    local menus = Ess.UI._S.menus
    local rt = menus and menus[id]
    if not rt then return false, 0, 0, 0, false end
    local depth = (rt.menu and rt.menu._stack) and #rt.menu._stack or 0
    local list = rt.list
    local sel = list and list._sel or 0
    local n = list and #list._items or 0
    return rt.open == true, depth, sel, n, (list ~= nil and Ess.UI._S.focus == list)
end
-- the nav keys Ess.UI listens for (Ess.lua:6854)
function H.keys()
    local k = Ess.UI.KEYS
    return k.down, k.enter, k.left
end
-- whether the shared UI runtime came up (Ess.lua:6900-6902)
function H.rt_ready()
    if type(Ess.UI._rtReady) ~= "function" then error("Ess.UI._rtReady is not a function") end
    return Ess.UI._rtReady() == true
end
function H.version() return tostring(Ess.VERSION) end
function H.has_menu() return type(_G.Ess) == "table" and type(Ess.UI) == "table" and type(Ess.UI.Menu) == "function" end
return H
"#;

struct Host {
    lua: Lua,
    st: Shared,
    stubbed: Vec<String>,
}

impl Host {
    fn new() -> LResult<Host> {
        let lua = Lua::new()?;
        let st: Shared = Rc::new(RefCell::new(State::default()));
        let mut host = Host { lua, st, stubbed: Vec::new() };
        host.install()?;
        Ok(host)
    }

    fn set_phase(&self, p: String) {
        self.st.borrow_mut().phase = p;
    }

    /// A stub that records itself, then runs `f`. `method` stubs are called with `:` and their
    /// first argument (`self`) is left out of the recorded summary.
    fn stub<F>(&self, name: &str, kind: Kind, method: bool, f: F) -> LResult<Function>
    where
        F: Fn(&Lua, &[Value], &Shared) -> LResult<MultiValue> + 'static,
    {
        let st = self.st.clone();
        let name = name.to_string();
        self.lua.create_function(move |lua, args: MultiValue| {
            let v = args.into_vec();
            {
                let shown = if method { v.get(1..).unwrap_or(&[]) } else { &v[..] };
                st.borrow_mut().record(&name, kind, shown);
            }
            f(lua, &v, &st)
        })
    }

    /// A table whose missing keys are recorded and raise, naming `prefix<key>`.
    fn strict(&self, prefix: &str, declared_nil: &'static [(&'static str, &'static str)]) -> LResult<Table> {
        let t = self.lua.create_table()?;
        let mt = self.lua.create_table()?;
        let index = self.lua.create_function(strict_index(self.st.clone(), prefix.to_string(), declared_nil))?;
        mt.raw_set("__index", index)?;
        t.set_metatable(Some(mt))?;
        Ok(t)
    }

    fn namespace(&mut self, name: &str) -> LResult<Table> {
        self.namespace_with(name, &[])
    }

    fn namespace_with(&mut self, name: &str, declared_nil: &'static [(&'static str, &'static str)]) -> LResult<Table> {
        let t = self.strict(&format!("{name}."), declared_nil)?;
        self.lua.globals().raw_set(name, t.clone())?;
        Ok(t)
    }

    fn put(&mut self, ns: &Table, ns_name: &str, field: &str, f: Function) -> LResult<()> {
        self.stubbed.push(format!("{ns_name}.{field}"));
        ns.raw_set(field, f)
    }

    fn install(&mut self) -> LResult<()> {
        let g = self.lua.globals();

        // The game has no debug library: Ess.lua:159-163 records `type(_G.debug)` as nil, confirmed
        // live. luaL_openlibs opened one here, so it is removed — after keeping its traceback for
        // this harness's own diagnostics.
        let debug: Table = g.raw_get("debug")?;
        self.st.borrow_mut().traceback = Some(debug.raw_get("traceback")?);
        g.raw_set("debug", Value::Nil)?;

        // _G: unknown globals are recorded and raise; declared-nil ones read as nil and are counted.
        {
            let mt = self.lua.create_table()?;
            let st = self.st.clone();
            let index = self.lua.create_function(move |_, (_t, k): (Value, Value)| -> LResult<Value> {
                let key = match &k {
                    Value::String(s) => s.to_string_lossy(),
                    other => st.borrow().summarize(other),
                };
                if DECLARED_NIL_GLOBALS.iter().any(|(n, _)| *n == key) {
                    *st.borrow_mut().declared_nil.entry(key).or_default() += 1;
                    return Ok(Value::Nil);
                }
                let tb = traceback(&st);
                Err(st.borrow_mut().unstubbed(key, tb))
            })?;
            mt.raw_set("__index", index)?;
            g.set_metatable(Some(mt))?;
        }

        // import(name) — the engine's module loader. Ess.lua imports game modules at load time
        // (e.g. Ess.lua:1788, 4734, 6183). Recorded only: a module global used later must be stubbed
        // below, or the access fails.
        let import = self.stub("import", Kind::Host, false, |_, a, st| {
            let mut s = st.borrow_mut();
            match a.first() {
                Some(Value::String(n)) => {
                    *s.imports.entry(n.to_string_lossy()).or_default() += 1;
                    Ok(MultiValue::new())
                }
                other => {
                    let shown = other.map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                    Err(s.violation(format!("import() expects a module name string, got {shown}")))
                }
            }
        })?;
        self.stubbed.push("import".into());
        g.raw_set("import", import)?;

        // Loader — the lua-bridge host surface.
        let loader = self.namespace("Loader")?;
        // Ess.Log -> Loader.Printf (Ess.lua:82-86)
        let f = self.stub("Loader.Printf", Kind::Host, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let line = match a.first() {
                Some(Value::String(t)) => t.to_string_lossy(),
                Some(v) => s.summarize(v),
                None => String::new(),
            };
            let phase = s.phase.clone();
            s.printf.push(format!("[{phase}] {line}"));
            Ok(MultiValue::new())
        })?;
        self.put(&loader, "Loader", "Printf", f)?;
        // Ess.Input.poll: the edge-triggered key ring (Ess.lua:4752)
        let f = self.stub("Loader.PopKeyEvents", Kind::Input, false, |lua, _, st| {
            let keys: Vec<u8> = std::mem::take(&mut st.borrow_mut().keys);
            Ok(MultiValue::from_vec(vec![Value::String(lua.create_string(keys)?)]))
        })?;
        self.put(&loader, "Loader", "PopKeyEvents", f)?;
        // Ess.Input.poll: the 256-byte held-key snapshot (Ess.lua:4756-4760). Nothing is ever held.
        let f = self.stub("Loader.GetKeyboardState", Kind::Input, false, |lua, _, _| {
            Ok(MultiValue::from_vec(vec![Value::String(lua.create_string([0u8; 256])?)]))
        })?;
        self.put(&loader, "Loader", "GetKeyboardState", f)?;
        // ui_focus drains the ring on focus (Ess.lua:6871)
        let f = self.stub("Loader.ClearKeyEvents", Kind::Input, false, |_, _, st| {
            st.borrow_mut().keys.clear();
            Ok(MultiValue::new())
        })?;
        self.put(&loader, "Loader", "ClearKeyEvents", f)?;

        // Event — Ess.Loop's only scheduler (Ess.lua:4658, 4664).
        let event = self.namespace("Event")?;
        let tr = self.st.borrow_mut().handle("Event.TimerRelative".into());
        if let Value::LightUserData(p) = &tr {
            self.st.borrow_mut().timer_relative = Some(*p);
        }
        self.stubbed.push("Event.TimerRelative".into());
        event.raw_set("TimerRelative", tr)?;
        let f = self.stub("Event.Create", Kind::Clock, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let kind_ok = matches!((a.first(), s.timer_relative), (Some(Value::LightUserData(p)), Some(t)) if *p == t);
            if !kind_ok {
                let shown = a.first().map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                return Err(s.violation(format!(
                    "Event.Create kind {shown} is not handled (only Event.TimerRelative is stubbed)"
                )));
            }
            let interval = match a.get(1) {
                Some(Value::Table(t)) => match t.raw_get::<Value>(1.0f64)? {
                    Value::Number(n) => n as f64,
                    other => {
                        let shown = s.summarize(&other);
                        return Err(s.violation(format!("Event.Create TimerRelative params[1] must be a number, got {shown}")));
                    }
                },
                other => {
                    let shown = other.map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                    return Err(s.violation(format!("Event.Create TimerRelative params must be a table, got {shown}")));
                }
            };
            let f = match a.get(2) {
                Some(Value::Function(f)) => f.clone(),
                other => {
                    let shown = other.map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                    return Err(s.violation(format!("Event.Create callback must be a function, got {shown}")));
                }
            };
            s.seq += 1;
            let (due, seq) = (s.now + interval, s.seq);
            let what = format!("TimerRelative {interval}s scheduled during {}", s.phase);
            s.timers.push(Timer { due, seq, f, what });
            let h = s.handle(format!("event#{seq}"));
            Ok(MultiValue::from_vec(vec![h]))
        })?;
        self.put(&event, "Event", "Create", f)?;
        // Ess.Gps arms two script-event listeners at load (Ess.lua:10249-10264) through Ess.On.script:
        // Event.CreatePersistent(Event.ScriptEvent, { sName, validator }, cb, {}) (Ess.lua:6153-6155).
        // Registered and recorded; nothing posts script events headless.
        let se = self.st.borrow_mut().handle("Event.ScriptEvent".into());
        self.stubbed.push("Event.ScriptEvent".into());
        event.raw_set("ScriptEvent", se.clone())?;
        let f = self.stub("Event.CreatePersistent", Kind::Clock, false, move |_, a, st| {
            let mut s = st.borrow_mut();
            if !same_handle(a.first(), &se) {
                let shown = a.first().map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                return Err(s.violation(format!(
                    "Event.CreatePersistent kind {shown} is not handled (only Event.ScriptEvent is stubbed)"
                )));
            }
            let name = match a.get(1) {
                Some(Value::Table(t)) => match (t.raw_get::<Value>(1.0f64)?, t.raw_get::<Value>(2.0f64)?) {
                    (Value::String(n), Value::Function(_)) => n.to_string_lossy(),
                    _ => return Err(s.violation("Event.CreatePersistent ScriptEvent params must be { name, validator }".into())),
                },
                _ => return Err(s.violation("Event.CreatePersistent ScriptEvent params must be a table".into())),
            };
            if !matches!(a.get(2), Some(Value::Function(_))) {
                return Err(s.violation(format!("Event.CreatePersistent ScriptEvent '{name}' needs a callback function")));
            }
            *s.script_events.entry(name.clone()).or_default() += 1;
            let h = s.handle(format!("script-event:{name}"));
            Ok(MultiValue::from_vec(vec![h]))
        })?;
        self.put(&event, "Event", "CreatePersistent", f)?;

        // Sys — the stamp clock behind Ess.Time (Ess.lua:5098-5144).
        let sys = self.namespace("Sys")?;
        let f = self.stub("Sys.RealTimeStamp", Kind::Clock, false, |_, _, st| {
            let mut s = st.borrow_mut();
            let n = s.stamps.len() + 1;
            let h = s.handle(format!("stamp#{n}"));
            if let Value::LightUserData(p) = &h {
                let now = s.now;
                s.stamps.insert(p.0 as usize, now);
            }
            Ok(MultiValue::from_vec(vec![h]))
        })?;
        self.put(&sys, "Sys", "RealTimeStamp", f)?;
        let f = self.stub("Sys.TimeStampGetElapsed", Kind::Clock, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let mark = match a.first() {
                Some(Value::LightUserData(p)) => s.stamps.get(&(p.0 as usize)).copied(),
                _ => None,
            };
            match mark {
                Some(m) => Ok(MultiValue::from_vec(vec![Value::Number((s.now - m) as f32)])),
                None => {
                    let shown = a.first().map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                    Err(s.violation(format!("Sys.TimeStampGetElapsed on {shown}, which is not a stamp")))
                }
            }
        })?;
        self.put(&sys, "Sys", "TimeStampGetElapsed", f)?;
        let f = self.stub("Sys.TimeStampMark", Kind::Clock, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let key = match a.first() {
                Some(Value::LightUserData(p)) if s.stamps.contains_key(&(p.0 as usize)) => p.0 as usize,
                other => {
                    let shown = other.map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                    return Err(s.violation(format!("Sys.TimeStampMark on {shown}, which is not a stamp")));
                }
            };
            let now = s.now;
            s.stamps.insert(key, now);
            Ok(MultiValue::new())
        })?;
        self.put(&sys, "Sys", "TimeStampMark", f)?;
        // Ess.RNG.new seeds from the wall clock at load (Ess.lua:8730, reached from Ess.lua:11080).
        // Returns the simulated clock in seconds.
        let f = self.stub("Sys.RealTime", Kind::Clock, false, |_, _, st| {
            Ok(MultiValue::from_vec(vec![Value::Number(st.borrow().now as f32)]))
        })?;
        self.put(&sys, "Sys", "RealTime", f)?;

        // Player / Object — the pose menu_ctx reads (Ess.lua:7909 -> Ess.Player.pose, Ess.lua:1863-1873)
        // and Menu:open's player gate (Ess.lua:8026).
        let player_h = self.st.borrow_mut().handle("player".into());
        let char_h = self.st.borrow_mut().handle("char".into());
        let player = self.namespace("Player")?;
        let ph = player_h.clone();
        let f = self.stub("Player.GetLocalPlayer", Kind::Game, false, move |_, _, _| Ok(MultiValue::from_vec(vec![ph.clone()])))?;
        self.put(&player, "Player", "GetLocalPlayer", f)?;
        let ch = char_h.clone();
        let f = self.stub("Player.GetLocalCharacter", Kind::Game, false, move |_, _, _| Ok(MultiValue::from_vec(vec![ch.clone()])))?;
        self.put(&player, "Player", "GetLocalCharacter", f)?;

        let object = self.namespace("Object")?;
        let ch = char_h.clone();
        let f = self.stub("Object.GetPosition", Kind::Game, false, move |_, a, st| {
            if !same_handle(a.first(), &ch) {
                let mut s = st.borrow_mut();
                let shown = a.first().map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                return Err(s.violation(format!("Object.GetPosition on {shown}; only the local character has a stubbed position")));
            }
            let (x, y, z) = PLAYER_POS;
            Ok(MultiValue::from_vec(vec![Value::Number(x), Value::Number(y), Value::Number(z)]))
        })?;
        self.put(&object, "Object", "GetPosition", f)?;
        let ch = char_h.clone();
        let f = self.stub("Object.GetYaw", Kind::Game, false, move |_, a, st| {
            if !same_handle(a.first(), &ch) {
                let mut s = st.borrow_mut();
                let shown = a.first().map(|v| s.summarize(v)).unwrap_or_else(|| "nothing".into());
                return Err(s.violation(format!("Object.GetYaw on {shown}; only the local character has a stubbed yaw")));
            }
            Ok(MultiValue::from_vec(vec![Value::Number(PLAYER_YAW)]))
        })?;
        self.put(&object, "Object", "GetYaw", f)?;
        // ctx:spawn faces the spawn (Ess.lua:7939)
        let f = self.stub("Object.SetYaw", Kind::Game, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let is_spawn = s.label_of(a.first().unwrap_or(&Value::Nil)).is_some_and(|l| l.starts_with("spawn#"));
            if !is_spawn || !matches!(a.get(1), Some(Value::Number(_))) {
                let shown: Vec<String> = a.iter().map(|v| s.summarize(v)).collect();
                return Err(s.violation(format!("Object.SetYaw({}) expects (spawned handle, number)", shown.join(", "))));
            }
            Ok(MultiValue::new())
        })?;
        self.put(&object, "Object", "SetYaw", f)?;

        // Pg.Spawn — ctx:spawn (Ess.lua:7938). Returns a distinct handle for every call; the
        // template is recorded, not judged.
        let pg = self.namespace("Pg")?;
        let f = self.stub("Pg.Spawn", Kind::Game, false, |_, a, st| {
            let mut s = st.borrow_mut();
            let shape_ok = matches!(a.first(), Some(Value::String(_)))
                && a.len() == 4
                && a[1..].iter().all(|v| matches!(v, Value::Number(_)));
            if !shape_ok {
                let shown: Vec<String> = a.iter().map(|v| s.summarize(v)).collect();
                return Err(s.violation(format!("Pg.Spawn({}) expects (template string, x, y, z)", shown.join(", "))));
            }
            s.spawned += 1;
            let n = s.spawned;
            let h = s.handle(format!("spawn#{n}"));
            Ok(MultiValue::from_vec(vec![h]))
        })?;
        self.put(&pg, "Pg", "Spawn", f)?;

        // Graphics.GetScreenRatio — Ess.UI.isWide (Ess.lua:6733). Ess.lua:6675-6677 documents the
        // return as a category string, "WIDESCREEN" or otherwise.
        let graphics = self.namespace("Graphics")?;
        let f = self.stub("Graphics.GetScreenRatio", Kind::Ui, false, |lua, _, _| {
            Ok(MultiValue::from_vec(vec![Value::String(lua.create_string("WIDESCREEN")?)]))
        })?;
        self.put(&graphics, "Graphics", "GetScreenRatio", f)?;

        // MrxGuiBase.FlashWidget — Ess.Gfx.widget/call/onEvent/setVisible (Ess.lua:6215-6276).
        let gui = self.namespace("MrxGuiBase")?;
        let class = self.strict("MrxGuiBase.FlashWidget.", &[])?;
        self.stubbed.push("MrxGuiBase.FlashWidget".into());
        let methods = self.widget_methods()?;
        let new = self.stub("MrxGuiBase.FlashWidget:new", Kind::Ui, true, move |lua, _, st| {
            let obj = lua.create_table()?;
            for (k, f) in &methods {
                obj.raw_set(k.as_str(), f.clone())?;
            }
            let mt = lua.create_table()?;
            let index = lua.create_function(strict_index(st.clone(), "FlashWidget:".into(), &[]))?;
            mt.raw_set("__index", index)?;
            obj.set_metatable(Some(mt))?;
            st.borrow_mut().widgets += 1;
            Ok(MultiValue::from_vec(vec![Value::Table(obj)]))
        })?;
        self.stubbed.push("MrxGuiBase.FlashWidget:new".into());
        class.raw_set("new", new)?;
        gui.raw_set("FlashWidget", class)?;
        let f = self.stub("MrxGuiBase.AddWidget", Kind::Ui, false, |_, a, st| {
            if !matches!(a.first(), Some(Value::Table(_))) {
                let mut s = st.borrow_mut();
                return Err(s.violation("MrxGuiBase.AddWidget expects a widget".into()));
            }
            Ok(MultiValue::new())
        })?;
        self.put(&gui, "MrxGuiBase", "AddWidget", f)?;

        let mgr = self.namespace("MrxGuiManager")?;
        let ph = player_h.clone();
        let f = self.stub("MrxGuiManager.AddWidgetToHud", Kind::Ui, false, move |_, a, st| {
            if !same_handle(a.first(), &ph) || !matches!(a.get(1), Some(Value::Table(_))) {
                let mut s = st.borrow_mut();
                return Err(s.violation("MrxGuiManager.AddWidgetToHud expects (local player, widget)".into()));
            }
            Ok(MultiValue::new())
        })?;
        self.put(&mgr, "MrxGuiManager", "AddWidgetToHud", f)?;

        // MrxFactionManager.NetEventCallback — Ess.Net hijacks it at load (Ess.lua:11872-11875) through
        // Ess.Override.wrap, which first reads the module's `_essWrapped` marker (Ess.lua:11517; nil on
        // a module nobody has wrapped yet). The callback only runs on co-op traffic; never headless.
        const FACTION_NIL: &[(&str, &str)] = &[("_essWrapped", "Ess.lua:11517 unwrapped-module marker")];
        let fm = self.namespace_with("MrxFactionManager", FACTION_NIL)?;
        let f = self.stub("MrxFactionManager.NetEventCallback", Kind::Host, false, |_, _, st| {
            let mut s = st.borrow_mut();
            Err(s.violation("MrxFactionManager.NetEventCallback (the game's original) is not modelled; nothing should post co-op traffic headless".into()))
        })?;
        self.put(&fm, "MrxFactionManager", "NetEventCallback", f)?;

        // Net.IsMultiplayer — Ess.Net's identity checks at load (Ess.lua:11770, 11884) and its 2 s
        // heartbeat (Ess.lua:11850-11851). Single-player: false.
        let net = self.namespace("Net")?;
        let f = self.stub("Net.IsMultiplayer", Kind::Game, false, |_, _, _| Ok(MultiValue::from_vec(vec![Value::Boolean(false)])))?;
        self.put(&net, "Net", "IsMultiplayer", f)?;
        Ok(())
    }

    /// The FlashWidget instance methods Ess.Gfx calls, each with `:` syntax.
    fn widget_methods(&mut self) -> LResult<Vec<(String, Function)>> {
        let mut out = Vec::new();
        for name in ["SetOwner", "SetLocation", "SetVisible"] {
            let f = self.stub(&format!("FlashWidget:{name}"), Kind::Ui, true, |_, _, _| Ok(MultiValue::new()))?;
            out.push((name.to_string(), f));
        }
        // SetSwfFile(file, onLoad, tData): the engine loads asynchronously and then calls
        // onLoad(unpack(tData)) (Ess.lua:6202-6209). Modelled as a callback on the next timer fire.
        let f = self.stub("FlashWidget:SetSwfFile", Kind::Ui, true, |_, a, st| {
            let mut s = st.borrow_mut();
            if !matches!(a.get(1), Some(Value::String(_))) {
                return Err(s.violation("FlashWidget:SetSwfFile expects a file name string".into()));
            }
            match (a.get(2), a.get(3)) {
                (Some(Value::Function(f)), Some(Value::Table(t))) => {
                    if !t.is_empty() {
                        return Err(s.violation("FlashWidget:SetSwfFile tData with values is not stubbed (only {} is)".into()));
                    }
                    match s.swf_load {
                        Some(SwfLoad::CallbackNextTick) => {
                            s.seq += 1;
                            let (due, seq) = (s.now, s.seq);
                            let what = format!("SetSwfFile load callback registered during {}", s.phase);
                            s.timers.push(Timer { due, seq, f: f.clone(), what });
                        }
                        Some(SwfLoad::CallbackNeverFires) => s.withheld_swf_callbacks += 1,
                        None => return Err(s.violation("FlashWidget:SetSwfFile called with no load model set".into())),
                    }
                }
                (None | Some(Value::Nil), None | Some(Value::Nil)) => {}
                _ => return Err(s.violation("FlashWidget:SetSwfFile expects (file, fn, {}) or (file, nil, nil)".into())),
            }
            Ok(MultiValue::new())
        })?;
        out.push(("SetSwfFile".into(), f));
        let f = self.stub("FlashWidget:CallActionScriptCallback", Kind::Ui, true, |_, a, st| {
            if !matches!(a.get(1), Some(Value::String(_))) || !matches!(a.get(2), Some(Value::Table(_))) {
                let mut s = st.borrow_mut();
                return Err(s.violation("FlashWidget:CallActionScriptCallback expects (name, args table)".into()));
            }
            Ok(MultiValue::new())
        })?;
        out.push(("CallActionScriptCallback".into(), f));
        // Movie -> Lua events never arrive headless; the registration is recorded.
        let f = self.stub("FlashWidget:SetFlashEventHandler", Kind::Ui, true, |_, a, st| {
            let mut s = st.borrow_mut();
            match (a.get(1), a.get(2)) {
                (Some(Value::String(n)), Some(Value::Function(_))) => {
                    *s.flash_handlers.entry(n.to_string_lossy()).or_default() += 1;
                    Ok(MultiValue::new())
                }
                _ => Err(s.violation("FlashWidget:SetFlashEventHandler expects (name, fn, {})".into())),
            }
        })?;
        out.push(("SetFlashEventHandler".into(), f));
        for (n, _) in &out {
            self.stubbed.push(format!("FlashWidget:{n}"));
        }
        Ok(out)
    }

    /// Fire the earliest pending timer. `Ok(false)` when none is pending.
    fn fire_next(&self) -> LResult<bool> {
        let timer = {
            let mut s = self.st.borrow_mut();
            let Some(i) = (0..s.timers.len()).min_by(|&a, &b| {
                let (ta, tb) = (&s.timers[a], &s.timers[b]);
                ta.due.total_cmp(&tb.due).then(ta.seq.cmp(&tb.seq))
            }) else {
                return Ok(false);
            };
            let t = s.timers.swap_remove(i);
            if t.due > s.now {
                s.now = t.due;
            }
            t
        };
        if let Err(e) = timer.f.call::<()>(()) {
            let mut s = self.st.borrow_mut();
            let msg = format!("{} raised: {e}", timer.what);
            s.timer_errors.push(msg);
        }
        Ok(true)
    }

    /// Queue one key byte and run timers until the heartbeat has drained it.
    fn press(&self, vk: u8) -> Result<(), String> {
        self.st.borrow_mut().keys.push(vk);
        for _ in 0..MAX_FIRES_PER_PRESS {
            if self.st.borrow().keys.is_empty() {
                return Ok(());
            }
            if !self.fire_next().map_err(|e| e.to_string())? {
                return Err(format!("key 0x{vk:02X} was queued but no timer is pending to poll it (heartbeat not running)"));
            }
        }
        Err(format!("key 0x{vk:02X} still queued after {MAX_FIRES_PER_PRESS} timer fires (nothing drains Loader.PopKeyEvents)"))
    }

    fn settle(&self) -> LResult<()> {
        let end = self.st.borrow().now + SETTLE_SECONDS;
        loop {
            let next_due = self.st.borrow().timers.iter().map(|t| t.due).fold(f64::INFINITY, f64::min);
            if !next_due.is_finite() || next_due > end {
                return Ok(());
            }
            self.fire_next()?;
        }
    }
}

fn same_handle(v: Option<&Value>, h: &Value) -> bool {
    matches!((v, h), (Some(Value::LightUserData(a)), Value::LightUserData(b)) if a == b)
}

// ─── the menu tree ───────────────────────────────────────────────────────────────────────────────

struct EntryRef {
    path: Vec<usize>,
    label: String,
    node: Table,
}

#[derive(Default)]
struct Tree {
    categories: usize,
    headers: usize,
    entries: Vec<EntryRef>,
}

/// Walk a menu built by Ess.UI.Menu: `_root.children` (Ess.lua:8080-8082), where a node is a header
/// (`header`, Ess.lua:7885), a category (`children`, Ess.lua:7898) or an entry (`action`, Ess.lua:7881).
fn collect_tree(h: &Table, menu: &Table) -> Result<Tree, String> {
    fn walk(h: &Table, node: &Table, path: &mut Vec<usize>, crumb: &str, tree: &mut Tree) -> Result<(), String> {
        let children: Table = node.raw_get("children").map_err(|e| format!("{crumb}: children: {e}"))?;
        let kids: Vec<Table> = children
            .sequence_values::<Table>()
            .collect::<LResult<Vec<Table>>>()
            .map_err(|e| format!("{crumb}: a child is not a table: {e}"))?;
        for (i, kid) in kids.iter().enumerate() {
            path.push(i + 1);
            let label_fn: Function = h.get("label").map_err(|e| e.to_string())?;
            let label: String = label_fn.call(kid.clone()).map_err(|e| format!("{crumb}: label: {e}"))?;
            let here = if crumb.is_empty() { label.clone() } else { format!("{crumb} > {label}") };
            let header: Value = kid.raw_get("header").map_err(|e| e.to_string())?;
            let sub: Value = kid.raw_get("children").map_err(|e| e.to_string())?;
            let action: Value = kid.raw_get("action").map_err(|e| e.to_string())?;
            if !matches!(header, Value::Nil) {
                tree.headers += 1;
            } else if matches!(sub, Value::Table(_)) {
                tree.categories += 1;
                walk(h, kid, path, &here, tree)?;
            } else if matches!(action, Value::Function(_)) {
                tree.entries.push(EntryRef { path: path.clone(), label: here, node: kid.clone() });
            } else {
                return Err(format!("{here}: node is neither a header, a category nor an entry"));
            }
            path.pop();
        }
        Ok(())
    }
    let root: Table = menu.raw_get("_root").map_err(|e| format!("menu has no _root table: {e}"))?;
    let mut tree = Tree::default();
    walk(h, &root, &mut Vec::new(), "", &mut tree)?;
    Ok(tree)
}

// ─── the report ──────────────────────────────────────────────────────────────────────────────────

pub struct EntryOutcome {
    pub label: String,
    pub error: Option<String>,
    pub game_calls: Vec<String>,
}

pub struct MenuReport {
    pub title: String,
    pub categories: usize,
    pub headers: usize,
    pub entries: usize,
    pub outcomes: Vec<EntryOutcome>,
    pub reopen_presses: usize,
}

pub struct MenuFileReport {
    pub name: String,
    pub presses: Vec<String>,
    pub menus: Vec<MenuReport>,
    pub opened_by_toggle: bool,
    pub closed_by_toggle: bool,
    pub failures: Vec<String>,
}

pub struct Report {
    pub mode: SwfLoad,
    pub entry_errors_pass: bool,
    /// `Ess.UI._rtReady()` after the run: the shared UI runtime came up under this load model.
    pub rt_ready: Result<bool, String>,
    pub withheld_swf_callbacks: usize,
    pub ess_name: String,
    pub ess_version: Option<String>,
    pub ess_failure: Option<String>,
    pub files: Vec<MenuFileReport>,
    pub calls: BTreeMap<String, u64>,
    pub unstubbed: BTreeMap<String, (u64, String)>,
    pub declared_nil: BTreeMap<String, u64>,
    pub violations: Vec<String>,
    pub timer_errors: Vec<String>,
    pub printf: Vec<String>,
    pub imports: BTreeMap<String, u64>,
    pub flash_handlers: BTreeMap<String, u64>,
    pub script_events: BTreeMap<String, u64>,
    pub stubbed: Vec<String>,
    pub pending_timers: Vec<String>,
}

impl Report {
    /// Ess and the harness: everything except errors raised by the menu file's own entries.
    pub fn harness_ok(&self) -> bool {
        self.ess_failure.is_none()
            && self.unstubbed.is_empty()
            && self.violations.is_empty()
            && self.timer_errors.is_empty()
            && matches!(self.rt_ready, Ok(true))
            && !self.files.is_empty()
            && self.files.iter().all(|f| {
                f.failures.is_empty()
                    && !f.menus.is_empty()
                    && f.opened_by_toggle
                    && f.closed_by_toggle
                    && f.menus.iter().all(|m| m.outcomes.len() == m.entries)
            })
    }

    /// `(menu file, entry path, error)` for every entry whose own code raised.
    pub fn entry_errors(&self) -> Vec<(&str, &str, &str)> {
        let mut out = Vec::new();
        for f in &self.files {
            for m in &f.menus {
                for x in &m.outcomes {
                    if let Some(e) = &x.error {
                        out.push((f.name.as_str(), x.label.as_str(), e.as_str()));
                    }
                }
            }
        }
        out
    }

    pub fn ok(&self) -> bool {
        self.harness_ok() && (self.entry_errors_pass || self.entry_errors().is_empty())
    }

    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut o = String::new();
        let _ = writeln!(o, "\n================ [load model] {} ================", self.mode.label());
        let _ = writeln!(o, "[ess-menu-check] Lua: mercs2_luac exact 5.1 (lua_Number = f32)");
        match &self.ess_failure {
            None => {
                let _ = writeln!(o, "[ess-menu-check] {}: loaded, Ess.VERSION {}", self.ess_name, self.ess_version.as_deref().unwrap_or("?"));
            }
            Some(e) => {
                let _ = writeln!(o, "[ess-menu-check] {}: FAILED: {e}", self.ess_name);
            }
        }
        for f in &self.files {
            let _ = writeln!(o, "\n[menu file] {}", f.name);
            for p in &f.presses {
                let _ = writeln!(o, "  {p}");
            }
            for m in &f.menus {
                let ok = m.outcomes.iter().filter(|x| x.error.is_none()).count();
                let raised = m.outcomes.len() - ok;
                let _ = writeln!(
                    o,
                    "  menu '{}': {} categories, {} entries, {} headers | activated {} of {}: {ok} ok, {raised} raised | reopen presses during the walk: {}",
                    m.title, m.categories, m.entries, m.headers, m.outcomes.len(), m.entries, m.reopen_presses
                );
                for x in &m.outcomes {
                    let status = if x.error.is_some() { "RAISED" } else { "ok" };
                    let _ = writeln!(o, "    {status:<6} {} :: {}", x.label, x.game_calls.join("; "));
                    if let Some(e) = &x.error {
                        let _ = writeln!(o, "           error: {e}");
                    }
                }
            }
            let _ = writeln!(o, "  toggle: opened by a press = {}, closed by a press = {}", f.opened_by_toggle, f.closed_by_toggle);
            for e in &f.failures {
                let _ = writeln!(o, "  FAILURE: {e}");
            }
        }
        let _ = writeln!(o, "\n[engine calls] (per function, whole run)");
        for (k, n) in &self.calls {
            let _ = writeln!(o, "  {n:>8}  {k}");
        }
        let _ = writeln!(o, "[imports] {}", fmt_counts(&self.imports));
        let _ = writeln!(o, "[flash event handlers registered] {}", fmt_counts(&self.flash_handlers));
        let _ = writeln!(o, "[script events listened for] {}", fmt_counts(&self.script_events));
        let _ = writeln!(o, "[declared-nil global reads] {}", fmt_counts(&self.declared_nil));
        let _ = writeln!(o, "[stubbed engine globals] {}", self.stubbed.join(", "));
        let _ = writeln!(o, "[timers still pending after settle] {}", self.pending_timers.len());
        for t in &self.pending_timers {
            let _ = writeln!(o, "  {t}");
        }
        let _ = writeln!(o, "[Loader.Printf] {} lines", self.printf.len());
        for l in &self.printf {
            let _ = writeln!(o, "  {l}");
        }
        let _ = writeln!(o, "[unstubbed engine accesses] {}", self.unstubbed.len());
        for (k, (n, phase)) in &self.unstubbed {
            let _ = writeln!(o, "  {k}  x{n}  (first during {phase})");
        }
        let _ = writeln!(o, "[stub contract violations] {}", self.violations.len());
        for v in &self.violations {
            let _ = writeln!(o, "  {v}");
        }
        let _ = writeln!(o, "[timer callbacks that raised] {}", self.timer_errors.len());
        for v in &self.timer_errors {
            let _ = writeln!(o, "  {v}");
        }
        let _ = writeln!(o, "[SetSwfFile load callbacks withheld] {}", self.withheld_swf_callbacks);
        match &self.rt_ready {
            Ok(r) => {
                let _ = writeln!(o, "[UI runtime ready (Ess.UI._rtReady)] {r}");
            }
            Err(e) => {
                let _ = writeln!(o, "[UI runtime ready (Ess.UI._rtReady)] could not read: {e}");
            }
        }
        let errs = self.entry_errors();
        let _ = writeln!(
            o,
            "[errors raised by the menu file's own entries] {}{}",
            errs.len(),
            if self.entry_errors_pass { " (--entry-errors-pass: listed, not failing)" } else { " (failing)" }
        );
        for (file, label, e) in &errs {
            let _ = writeln!(o, "  {file}: {label}\n      {e}");
        }
        let _ = writeln!(
            o,
            "[ess-menu-check] {} -- Ess/harness {}, entry errors {}",
            self.mode.label(),
            if self.harness_ok() { "PASS" } else { "FAIL" },
            if errs.is_empty() { "none" } else if self.entry_errors_pass { "present (allowed)" } else { "present (FAIL)" }
        );
        let _ = writeln!(o, "[ess-menu-check] {}: {}", self.mode.label(), if self.ok() { "PASS" } else { "FAIL" });
        o
    }
}

fn fmt_counts(m: &BTreeMap<String, u64>) -> String {
    if m.is_empty() {
        return "(none)".into();
    }
    m.iter().map(|(k, n)| format!("{k} x{n}")).collect::<Vec<_>>().join(", ")
}

// ─── the run ─────────────────────────────────────────────────────────────────────────────────────

struct Keys {
    down: u8,
    enter: u8,
    left: u8,
}

struct Runner<'a> {
    host: &'a Host,
    h: Table,
    keys: Keys,
    report_fn: Function,
    file: &'a Source,
    press_no: usize,
}

impl Runner<'_> {
    fn helper(&self, name: &str) -> Result<Function, String> {
        self.h.get(name).map_err(|e| format!("harness helper {name}: {e}"))
    }

    /// One OnKey press: run the whole menu file. Returns the menus it built, each wrapped.
    fn press_file(&mut self, purpose: &str, log: &mut Vec<String>) -> Result<Vec<(Table, Tree)>, String> {
        self.press_no += 1;
        let n = self.press_no;
        self.host.set_phase(format!("{} press {n} ({purpose})", self.file.name));
        self.host.st.borrow_mut().captured.clear();
        let run = self.host.lua.load(&self.file.bytes).set_name(self.file.chunk_name()).exec();
        let captured = std::mem::take(&mut self.host.st.borrow_mut().captured);
        if let Err(e) = run {
            log.push(format!("press {n} ({purpose}): FAILED: {e}"));
            return Err(format!("press {n} ({purpose}) raised: {e}"));
        }
        let mut out = Vec::new();
        let wrap = self.helper("wrap")?;
        for m in captured {
            let tree = collect_tree(&self.h, &m)?;
            for e in &tree.entries {
                wrap.call::<()>((e.node.clone(), path_key(&e.path), self.report_fn.clone()))
                    .map_err(|err| format!("wrap {}: {err}", e.label))?;
            }
            out.push((m, tree));
        }
        log.push(format!("press {n} ({purpose}): ran, built {} menu(s)", out.len()));
        Ok(out)
    }

    fn state(&self, id: &Value) -> Result<(bool, usize, usize, usize, bool), String> {
        self.helper("state")?.call(id.clone()).map_err(|e| format!("state: {e}"))
    }

    fn key(&self, vk: u8, why: &str) -> Result<(), String> {
        self.host.press(vk).map_err(|e| format!("{why}: {e}"))
    }

    fn select_row(&self, id: &Value, row: usize) -> Result<(), String> {
        let (_, _, _, n, _) = self.state(id)?;
        for _ in 0..=n {
            let (open, _, sel, _, focused) = self.state(id)?;
            if !open || !focused {
                return Err(format!("menu closed or lost focus while moving to row {row}"));
            }
            if sel == row {
                return Ok(());
            }
            self.key(self.keys.down, &format!("moving to row {row}"))?;
        }
        Err(format!("row {row} never became the selection after {} DOWN presses", n + 1))
    }
}

fn path_key(path: &[usize]) -> String {
    path.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(".")
}

/// Load `ess`, then run and drive every menu file. `Err` only when the harness itself could not
/// be built; every check outcome is in the [`Report`].
pub fn check(ess: &Source, menus: &[Source], mode: SwfLoad, entry_errors_pass: bool) -> Result<Report, String> {
    let host = Host::new().map_err(|e| format!("Lua state: {e}"))?;
    host.st.borrow_mut().swf_load = Some(mode);
    let mut report = Report {
        mode,
        entry_errors_pass,
        rt_ready: Err("Ess did not load".into()),
        withheld_swf_callbacks: 0,
        ess_name: ess.name.clone(),
        ess_version: None,
        ess_failure: None,
        files: Vec::new(),
        calls: BTreeMap::new(),
        unstubbed: BTreeMap::new(),
        declared_nil: BTreeMap::new(),
        violations: Vec::new(),
        timer_errors: Vec::new(),
        printf: Vec::new(),
        imports: BTreeMap::new(),
        flash_handlers: BTreeMap::new(),
        script_events: BTreeMap::new(),
        stubbed: host.stubbed.clone(),
        pending_timers: Vec::new(),
    };

    host.set_phase(format!("OnLoad {}", ess.name));
    let loaded = host.lua.load(&ess.bytes).set_name(ess.chunk_name()).exec();
    let h: Option<Table> = match loaded {
        Err(e) => {
            report.ess_failure = Some(format!("chunk raised: {e}"));
            None
        }
        Ok(()) => {
            let h: Table = host
                .lua
                .load(HELPERS)
                .set_name("=ess-menu-check helpers")
                .eval()
                .map_err(|e| format!("harness helpers: {e}"))?;
            let has: bool = h
                .get::<Function>("has_menu")
                .and_then(|f| f.call(()))
                .map_err(|e| format!("has_menu: {e}"))?;
            if has {
                report.ess_version = Some(
                    h.get::<Function>("version")
                        .and_then(|f| f.call(()))
                        .map_err(|e| format!("reading Ess.VERSION: {e}"))?,
                );
                Some(h)
            } else {
                report.ess_failure = Some("after loading, _G.Ess.UI.Menu is not a function".into());
                None
            }
        }
    };

    if let Some(h) = h {
        let capture = {
            let st = host.st.clone();
            host.lua
                .create_function(move |_, m: Table| {
                    st.borrow_mut().captured.push(m);
                    Ok(())
                })
                .map_err(|e| e.to_string())?
        };
        h.get::<Function>("install_capture")
            .and_then(|f| f.call::<()>(capture))
            .map_err(|e| format!("install_capture: {e}"))?;
        let report_fn = {
            let st = host.st.clone();
            host.lua
                .create_function(move |_, (path, ok, err): (String, bool, Option<String>)| {
                    st.borrow_mut().activations.push((path, if ok { None } else { Some(err.unwrap_or_default()) }));
                    Ok(())
                })
                .map_err(|e| e.to_string())?
        };
        let (down, enter, left): (u8, u8, u8) = h
            .get::<Function>("keys")
            .and_then(|f| f.call(()))
            .map_err(|e| format!("Ess.UI.KEYS: {e}"))?;
        for file in menus {
            let mut runner = Runner {
                host: &host,
                h: h.clone(),
                keys: Keys { down, enter, left },
                report_fn: report_fn.clone(),
                file,
                press_no: 0,
            };
            report.files.push(run_file(&mut runner));
        }
        host.set_phase("settle".into());
        host.settle().map_err(|e| format!("settle: {e}"))?;
        report.rt_ready = h
            .get::<Function>("rt_ready")
            .and_then(|f| f.call::<bool>(()))
            .map_err(|e| e.to_string());
        finish(&host, &mut report);
        // Break the Rc cycle (timers hold functions that hold the state's Lua).
        host.st.borrow_mut().timers.clear();
    } else {
        finish(&host, &mut report);
    }
    Ok(report)
}

fn finish(host: &Host, report: &mut Report) {
    let mut s = host.st.borrow_mut();
    report.calls = std::mem::take(&mut s.calls);
    report.unstubbed = std::mem::take(&mut s.unstubbed);
    report.declared_nil = std::mem::take(&mut s.declared_nil);
    report.violations = std::mem::take(&mut s.violations);
    report.timer_errors = std::mem::take(&mut s.timer_errors);
    report.printf = std::mem::take(&mut s.printf);
    report.imports = std::mem::take(&mut s.imports);
    report.flash_handlers = std::mem::take(&mut s.flash_handlers);
    report.script_events = std::mem::take(&mut s.script_events);
    report.withheld_swf_callbacks = s.withheld_swf_callbacks;
    report.pending_timers = s.timers.iter().map(|t| format!("due {:.2}s: {}", t.due, t.what)).collect();
}

fn run_file(r: &mut Runner<'_>) -> MenuFileReport {
    let mut fr = MenuFileReport {
        name: r.file.name.clone(),
        presses: Vec::new(),
        menus: Vec::new(),
        opened_by_toggle: false,
        closed_by_toggle: false,
        failures: Vec::new(),
    };
    if let Err(e) = drive(r, &mut fr) {
        fr.failures.push(e);
    }
    fr
}

fn drive(r: &mut Runner<'_>, fr: &mut MenuFileReport) -> Result<(), String> {
    let built = r.press_file("first press", &mut fr.presses)?;
    if built.is_empty() {
        return Err("the first press built no Ess.UI.Menu".into());
    }
    let first: Vec<(Value, Tree)> = built
        .into_iter()
        .map(|(m, t)| m.raw_get::<Value>("_id").map(|id| (id, t)).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;

    for (mi, (id, tree)) in first.iter().enumerate() {
        let title = r.host.st.borrow().summarize(id);
        let mut mr = MenuReport {
            title,
            categories: tree.categories,
            headers: tree.headers,
            entries: tree.entries.len(),
            outcomes: Vec::new(),
            reopen_presses: 0,
        };
        if mi == 0 {
            let (open, ..) = r.state(id)?;
            if !open {
                fr.menus.push(mr);
                return Err(format!("menu {} did not open on the first press", r.host.st.borrow().summarize(id)));
            }
            fr.opened_by_toggle = true;
        }
        let res = walk_menu(r, id, tree, &mut mr, fr);
        fr.menus.push(mr);
        res?;
    }

    // Toggle: the next press must flip the first menu's state; if that opened it, one more must
    // close it, so the open->closed toggle path always runs.
    let id = &first[0].0;
    let (before, ..) = r.state(id)?;
    r.press_file("toggle", &mut fr.presses)?;
    let (after, ..) = r.state(id)?;
    if after == before {
        return Err(format!("toggle press left the menu {} (expected it to flip)", if after { "open" } else { "closed" }));
    }
    if after {
        fr.opened_by_toggle = true;
        r.press_file("toggle off", &mut fr.presses)?;
        let (again, ..) = r.state(id)?;
        if again {
            return Err("second toggle press left the menu open".into());
        }
    }
    fr.closed_by_toggle = true;
    Ok(())
}

fn walk_menu(r: &mut Runner<'_>, id: &Value, tree: &Tree, mr: &mut MenuReport, fr: &mut MenuFileReport) -> Result<(), String> {
    let mut cur: Vec<usize> = Vec::new();
    for e in &tree.entries {
        let (open, ..) = r.state(id)?;
        if !open {
            // The user's way back into a closed menu is the OnKey press: re-run the file.
            let rebuilt = r.press_file(&format!("reopen for {}", e.label), &mut fr.presses)?;
            mr.reopen_presses += 1;
            let same = rebuilt.iter().any(|(m, t)| {
                m.raw_get::<Value>("_id").is_ok_and(|v| r.host.st.borrow().summarize(&v) == r.host.st.borrow().summarize(id))
                    && t.entries.len() == tree.entries.len()
            });
            if !same {
                return Err(format!("reopen press did not rebuild menu {} with the same {} entries", mr.title, tree.entries.len()));
            }
            let (open, ..) = r.state(id)?;
            if !open {
                return Err(format!("reopen press did not open menu {}", mr.title));
            }
            cur.clear();
        }
        let parent = &e.path[..e.path.len() - 1];
        while !parent.starts_with(&cur) {
            r.key(r.keys.left, &format!("backing out toward {}", e.label))?;
            cur.pop();
            let (_, depth, ..) = r.state(id)?;
            if depth != cur.len() + 1 {
                return Err(format!("after LEFT the menu stack is {depth} deep, expected {}", cur.len() + 1));
            }
        }
        for &row in &parent[cur.len()..] {
            r.select_row(id, row)?;
            r.key(r.keys.enter, &format!("entering category toward {}", e.label))?;
            cur.push(row);
            let (_, depth, ..) = r.state(id)?;
            if depth != cur.len() + 1 {
                return Err(format!("after ENTER on a category the menu stack is {depth} deep, expected {}", cur.len() + 1));
            }
        }
        let row = *e.path.last().expect("an entry path is never empty");
        r.select_row(id, row)?;

        r.host.set_phase(format!("{} activate {}", r.file.name, e.label));
        r.host.st.borrow_mut().entry_calls = Some(Vec::new());
        r.host.st.borrow_mut().activations.clear();
        let pressed = r.key(r.keys.enter, &format!("activating {}", e.label));
        let game_calls = r.host.st.borrow_mut().entry_calls.take().unwrap_or_default();
        pressed?;
        let acts = std::mem::take(&mut r.host.st.borrow_mut().activations);
        let key = path_key(&e.path);
        match acts.as_slice() {
            [(p, err)] if *p == key => mr.outcomes.push(EntryOutcome { label: e.label.clone(), error: err.clone(), game_calls }),
            _ => {
                return Err(format!(
                    "ENTER on {} produced {} activation(s) {:?}, expected exactly this entry ({key})",
                    e.label,
                    acts.len(),
                    acts.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>()
                ))
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny Ess-shaped framework honouring the contract the harness drives: `Ess.UI.Menu` builds
    /// `{_root, _id, _rt}`, runtime state lives at `Ess.UI._S.menus[id]` with `list._sel/_items`,
    /// keys arrive through a `Loader.PopKeyEvents` heartbeat on `Event.TimerRelative`.
    const FIXTURE_ESS: &str = r#"
_G.Ess = _G.Ess or {}
local Ess = _G.Ess
Ess.VERSION = "fixture"
Ess.UI = Ess.UI or {}
Ess.UI._S = Ess.UI._S or {}
Ess.UI.KEYS = { up = 0x26, down = 0x28, left = 0x25, right = 0x27, enter = 0x0D, esc = 0x1B }
local S = Ess.UI._S
local rtEnsure
local function tick()
    local f = S.focus
    if f then
        local ev = Loader.PopKeyEvents()
        for i = 1, #ev do f:_keyvk(string.byte(ev, i)) end
    end
    if S.focus then Event.Create(Event.TimerRelative, { 0.05 }, tick) else S.ticking = false end
end
local function ensureTick()
    if not S.ticking then S.ticking = true; Event.Create(Event.TimerRelative, { 0.05 }, tick) end
end
local function List()
    local o = { _items = {}, _sel = 1 }
    function o:items(t) o._items = t; o._sel = 1 end
    function o:_keyvk(vk)
        if vk == Ess.UI.KEYS.down then o._sel = o._sel % #o._items + 1
        elseif vk == Ess.UI.KEYS.enter then pcall(o.onChoose, o._items[o._sel])
        elseif vk == Ess.UI.KEYS.left then pcall(o.onBack) end
    end
    return o
end
local B = {}; B.__index = B
function B:entry(l, a) self._children[#self._children + 1] = { label = l, action = a }; return self end
function B:category(l, fn)
    local n = { label = l, children = {} }
    self._children[#self._children + 1] = n
    fn(setmetatable({ _children = n.children }, B))
end
local M = setmetatable({}, { __index = B }); M.__index = M
function M:_paint()
    local rows = {}
    for i, n in ipairs(self._stack[#self._stack].children) do rows[i] = { _node = n } end
    self._rt.list:items(rows)
end
function M:_choose(it)
    local n = it._node
    if n.children then self._stack[#self._stack + 1] = n; self:_paint(); return end
    local menu = self
    local ctx = {}
    function ctx:spawn(t, d) local u = Pg.Spawn(t, 1, 2, 3 + d); Object.SetYaw(u, 0.5); return u end
    function ctx:close() menu:close() end
    local ok, err = pcall(n.action, ctx)
    if not ok then Loader.Printf("action error: " .. tostring(err)) end
end
function M:_back()
    if #self._stack > 1 then self._stack[#self._stack] = nil; self:_paint() else self:close() end
end
function M:open()
    local rt = self._rt
    if not Player.GetLocalPlayer() then return end
    rtEnsure()
    rt.list = rt.list or List()
    rt.list.onChoose = function(it) self:_choose(it) end
    rt.list.onBack = function() self:_back() end
    rt.menu = self
    self._stack = { self._root }
    self:_paint()
    S.focus = rt.list
    rt.open = true
    ensureTick()
end
function M:close() self._rt.open = false; S.focus = nil end
-- an async-loaded runtime with a 3 s timeout fallback, shaped like Ess.lua:6937-7001
rtEnsure = function()
    if S.rt then return end
    S.rt = { ready = false }
    local w = MrxGuiBase.FlashWidget:new()
    w:SetSwfFile("fixture.gfx", function()
        if not S.rt.ready then S.rt.ready = true; Loader.Printf("rt ready via callback") end
    end, {})
    local waited = 0
    local function poll()
        waited = waited + 0.1
        if S.rt.ready then return end
        if waited >= 3.0 then S.rt.ready = true; Loader.Printf("rt ready via timeout"); return end
        Event.Create(Event.TimerRelative, { 0.1 }, poll)
    end
    Event.Create(Event.TimerRelative, { 0.1 }, poll)
end
function Ess.UI._rtReady() return S.rt ~= nil and S.rt.ready == true end
function M:toggle() if self._rt.open then self:close() else self:open() end end
function Ess.UI.Menu(o)
    local id = o.title
    S.menus = S.menus or {}
    S.menus[id] = S.menus[id] or { open = false }
    local root = { label = id, children = {} }
    return setmetatable({ _root = root, _children = root.children, _id = id, _rt = S.menus[id] }, M)
end
"#;

    fn src(name: &str, text: &str) -> Source {
        Source { name: name.into(), bytes: text.as_bytes().to_vec() }
    }

    fn run_with(menu: &str, mode: SwfLoad, entry_errors_pass: bool) -> Report {
        check(&src("FixtureEss.lua", FIXTURE_ESS), &[src("menu.lua", menu)], mode, entry_errors_pass)
            .expect("harness builds")
    }

    fn run(menu: &str) -> Report {
        run_with(menu, SwfLoad::CallbackNextTick, false)
    }

    const GOOD_MENU: &str = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:category("Vehicles", function(a)
    a:entry("Tank", function(ctx) ctx:spawn("Tank Template", 10) end)
    a:category("Air", function(b)
        b:entry("Heli", function(ctx) ctx:spawn("Heli Template", 15) end)
    end)
end)
menu:entry("Crate", function(ctx) ctx:spawn("Crate Template", 2) end)
menu:toggle()
"#;

    #[test]
    fn ess_menu_check_loads_and_activates_every_entry() {
        let r = run(GOOD_MENU);
        let text = r.render();
        assert_eq!(r.ok(), true, "{text}");
        assert_eq!(r.ess_version.as_deref(), Some("fixture"));
        let m = &r.files[0].menus[0];
        assert_eq!((m.categories, m.entries, m.outcomes.len()), (2, 3, 3), "{text}");
        assert!(m.outcomes.iter().all(|o| o.error.is_none()), "{text}");
        let heli = m.outcomes.iter().find(|o| o.label == "Vehicles > Air > Heli").expect("heli entry");
        assert_eq!(heli.game_calls[0], "Pg.Spawn(\"Heli Template\", 1, 2, 18)", "{text}");
        assert_eq!(r.calls.get("Pg.Spawn"), Some(&3), "{text}");
        assert!(r.unstubbed.is_empty() && r.violations.is_empty(), "{text}");
    }

    #[test]
    fn ess_menu_check_reports_an_entry_that_raises() {
        let r = run(RAISING_MENU);
        let text = r.render();
        assert_eq!(r.ok(), false, "{text}");
        let m = &r.files[0].menus[0];
        assert_eq!(m.outcomes.len(), 2, "every entry is still activated: {text}");
        let broken = m.outcomes.iter().find(|o| o.label == "Broken").expect("broken entry");
        assert!(broken.error.as_deref().is_some_and(|e| e.contains("boom from the menu")), "{text}");
        assert!(text.contains("RAISED"), "{text}");
    }

    #[test]
    fn ess_menu_check_fails_loudly_on_an_unstubbed_global() {
        // The fixture's own pcall swallows the error; the harness must still see it.
        let menu = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:entry("Uses Foo", function(ctx) Foo.Bar() end)
menu:toggle()
"#;
        let r = run(menu);
        let text = r.render();
        assert_eq!(r.ok(), false, "{text}");
        assert!(r.unstubbed.contains_key("Foo"), "{text}");
        let m = &r.files[0].menus[0];
        assert!(m.outcomes[0].error.as_deref().is_some_and(|e| e.contains("`Foo` is not stubbed")), "{text}");
    }

    #[test]
    fn ess_menu_check_fails_on_an_unstubbed_namespace_field() {
        let menu = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:entry("Uses Pg.Nope", function(ctx) Pg.Nope() end)
menu:toggle()
"#;
        let r = run(menu);
        assert!(!r.ok());
        assert!(r.unstubbed.contains_key("Pg.Nope"), "{}", r.render());
    }

    #[test]
    fn ess_menu_check_toggles_off_on_the_second_press() {
        let r = run(GOOD_MENU);
        let f = &r.files[0];
        assert!(f.opened_by_toggle && f.closed_by_toggle, "{}", r.render());
        // first press + one toggle press that closed it
        assert_eq!(f.presses.len(), 2, "{:?}", f.presses);
    }

    #[test]
    fn ess_menu_check_reopens_after_a_close_entry_then_toggles_both_ways() {
        let menu = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:entry("Close Menu", function(ctx) ctx:close() end)
menu:entry("After", function(ctx) ctx:spawn("B", 1) end)
menu:toggle()
"#;
        let r = run(menu);
        let text = r.render();
        assert_eq!(r.ok(), true, "{text}");
        let f = &r.files[0];
        assert_eq!(f.menus[0].reopen_presses, 1, "{text}");
        assert_eq!(f.menus[0].outcomes.len(), 2, "{text}");
        assert!(f.opened_by_toggle && f.closed_by_toggle, "{text}");
    }

    #[test]
    fn ess_menu_check_menu_file_that_raises_fails() {
        let r = run("error('menu file broke')");
        assert!(!r.ok());
        assert!(r.files[0].failures[0].contains("menu file broke"), "{}", r.render());
    }

    const RAISING_MENU: &str = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:entry("Fine", function(ctx) ctx:spawn("A", 1) end)
menu:entry("Broken", function(ctx) error("boom from the menu") end)
menu:toggle()
"#;

    #[test]
    fn ess_menu_check_both_load_models_pass() {
        let tick = run_with(GOOD_MENU, SwfLoad::CallbackNextTick, false);
        let t = tick.render();
        assert!(tick.ok(), "{t}");
        assert_eq!(tick.withheld_swf_callbacks, 0, "{t}");
        assert!(tick.printf.iter().any(|l| l.ends_with("rt ready via callback")), "{t}");

        let never = run_with(GOOD_MENU, SwfLoad::CallbackNeverFires, false);
        let n = never.render();
        assert!(never.ok(), "{n}");
        assert_eq!(never.withheld_swf_callbacks, 1, "{n}");
        assert!(never.printf.iter().any(|l| l.ends_with("rt ready via timeout")), "{n}");
        assert!(n.contains("timeout fallback"), "the run is labelled with its load model: {n}");
        assert!(matches!(never.rt_ready, Ok(true)), "{n}");
    }

    #[test]
    fn ess_menu_check_entry_errors_fail_by_default_and_pass_with_the_flag() {
        for mode in SwfLoad::ALL {
            let strict = run_with(RAISING_MENU, mode, false);
            assert!(strict.harness_ok(), "{}", strict.render());
            assert!(!strict.ok(), "{}", strict.render());
            assert_eq!(strict.entry_errors().len(), 1);

            let lenient = run_with(RAISING_MENU, mode, true);
            let text = lenient.render();
            assert!(lenient.ok(), "{text}");
            assert_eq!(lenient.entry_errors().len(), 1, "{text}");
            assert!(text.contains("[errors raised by the menu file's own entries] 1"), "{text}");
            assert!(text.contains("boom from the menu"), "{text}");
        }
    }

    #[test]
    fn ess_menu_check_flag_never_masks_harness_failures() {
        let menu = r#"
local menu = Ess.UI.Menu{ title = "T" }
menu:entry("Uses Foo", function(ctx) Foo.Bar() end)
menu:toggle()
"#;
        let r = run_with(menu, SwfLoad::CallbackNextTick, true);
        assert!(!r.ok(), "{}", r.render());
        assert!(r.unstubbed.contains_key("Foo"));
        let broken = run_with("error('menu file broke')", SwfLoad::CallbackNeverFires, true);
        assert!(!broken.ok(), "{}", broken.render());
    }

    #[test]
    fn ess_menu_check_bad_args_exit_2() {
        assert_eq!(main(&[]), 2);
        assert_eq!(main(&["menu.lua".into()]), 2);
        assert_eq!(main(&["--ess".into()]), 2);
    }
}
