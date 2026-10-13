//! `ess-live` — check, against the RUNNING game through lua-bridge's REPL, that the Ess Lua framework
//! and a menu script built on it work on the bridge it is loaded under.
//!
//! Every check is one Lua chunk sent with [`Bridge::eval`]; its reply (`[ok]\t<v1>\t<v2>…`,
//! `[runtime] …`, `[compile] …`, `[bridge] …`, rendered by `mercs2_formats::lua_exec`) is parsed into
//! typed values and judged against the shape the check expects. Nothing is read from a log file.
//!
//! Each chunk's first line is `-- ess-live:<check>`, so a chunk is identifiable in `lua_bridge.log`.
//!
//! The checks, and the Ess call site each one mirrors (Ess `src/` file:line):
//! - `bridge`: `return 1+1` answers `[ok]\t2`.
//! - `ess_loaded`: `_G.Ess` is a table with a string `VERSION` (`00_core.lua:35-37`).
//! - `loader_printf`: `Loader.Printf(<one string>)` returns nothing, and `Ess.Log` runs (`00_core.lua:77-81`).
//! - `loader_pop_key_events`: `Loader.PopKeyEvents()` returns one string; `Ess.Input.poll()` returns
//!   `{pressed = table, down = function}` and `down(vk)` a boolean (`21_input.lua:34-46`, `:202`).
//! - `loader_get_keyboard_state`: one 256-byte string of only `0x00`/`0x80` bytes; `Ess.Input.held`
//!   returns a boolean (`21_input.lua:40`, `:54-58`).
//! - `loader_is_key_down`: `pcall(Loader.IsKeyDown, 0x10)` gives `true, <boolean>`, one result; a string
//!   argument raises (`21_input.lua:203`).
//! - `loader_clear_key_events`: `Loader.ClearKeyEvents()` returns nothing, `pcall` of it succeeds, the
//!   ring is empty after it, and `Ess.Input.clear()` returns nothing (`21_input.lua:65-67`, `:247`,
//!   `42_ui_engine.lua:263`).
//! - `loader_save_load_var`: raw `SaveVar`/`LoadVar` round trip on a probe-only key, `nil` deletes, a
//!   table value raises (`22_state.lua:49`, `:55`).
//! - `loader_get_load_phase`: two results, a number and a string. Ess itself never calls it (no
//!   `GetLoadPhase` anywhere in Ess `src/`); it is checked as part of the bridge contract.
//! - `ess_savevar_roundtrip`: `Ess.SaveVar.ns("ess_live_probe")` set/get of a number, a string and a
//!   boolean, `:flag`, then `set(k, nil)` so `get(k, default)` returns the default (`22_state.lua:44-65`).
//! - `menu_precondition` / `menu_open` / `menu_close` (with `--menu`): `Loader.LoadFile(<menu>)` twice;
//!   the open state is Ess's own `Ess.UI._S.openId` + `Ess.UI._S.menus[id].open`
//!   (`49_ui_menu.lua:127-132`, `:206-207`, `:216-217`, `42_ui_engine.lua:14`). An Ess menu already
//!   open is first closed through Ess's own `Menu:close` (`49_ui_menu.lua:211-220`) on the object its
//!   runtime state keeps (`rt.menu`, `49_ui_menu.lua:202`), and the evidence says so; the precondition
//!   fails only if that close does not take effect or there is no local player/character
//!   (`49_ui_menu.lua:179`).
//! - `swf_callback` (with `--menu`): PRESENT — the `MrxGuiBase.FlashWidget` members Ess's SWF path calls
//!   (`40_gfx.lua:52-63`, `:79`, `:91`); RUNS — opening the menu loads Ess's runtime movie through
//!   `SetSwfFile(file, onLoad, {})` when the runtime does not exist yet (`42_ui_engine.lua:329-341`), and
//!   the probe wraps that onLoad (via `Ess.Gfx.widget`, restored right after the open) to observe
//!   whether it fires and whether Ess had already taken the 3 s timeout path (`42_ui_engine.lua:380-391`).
//! - `spawn` / `spawn_remove` / `spawn_gone` / `spawn_cleanup` (with `--spawn`): the menu ctx's own
//!   `ctx:spawn(template, dist)` (`49_ui_menu.lua:78-95`), then `Ess.Object.remove` and
//!   `Ess.Object.alive` polled until false (`11_object.lua:224-239`: removal is deferred ~0.5 s).

use mercs2_bridge::{Bridge, BridgeError};
use std::time::{Duration, Instant};

/// The prefix every saved key this probe writes starts with; no real mod uses it.
const SAVE_PREFIX: &str = "ess_live_probe";
/// The `_G` slot the menu check keeps the opened menu's id in between its two chunks.
const MENU_ID_GLOBAL: &str = "__ess_live_probe_menu";
/// The `_G` slot the spawn check keeps the spawned object's guid in between its chunks.
const SPAWN_GLOBAL: &str = "__ess_live_probe_spawn";
/// The id of the throwaway `Ess.UI.Menu` the spawn check builds to obtain a real menu ctx.
const SPAWN_MENU_ID: &str = "__ess_live_probe_spawn_menu";

/// What to run and where.
#[derive(Debug, Clone)]
pub struct Options {
    /// The REPL address (`host:port`).
    pub addr: String,
    /// The menu script, relative to the game's `scripts/` folder; `None` = menu checks not requested.
    pub menu: Option<String>,
    /// The spawn template; `None` = spawn checks not requested.
    pub spawn: Option<String>,
    /// How far ahead of the player `ctx:spawn` places the object, in metres.
    pub spawn_dist: f64,
    /// How long to wait for a removed object to stop being alive.
    pub remove_deadline: Duration,
    /// The interval between polls (the removed object's `alive`, the SWF load callback).
    pub poll_interval: Duration,
    /// How long after the menu opens to wait for the SWF load callback.
    pub swf_deadline: Duration,
}

/// The SWF-callback deadline: twice Ess's own `RT_LOAD_TIMEOUT = 3.0` (42_ui_engine.lua:288), so a
/// callback that arrives after Ess's fallback is still seen and reported as late.
pub const DEFAULT_SWF_DEADLINE: Duration = Duration::from_secs(6);

/// The removal deadline: `11_object.lua:224-227` measured `alive` flipping ~0.5 s after `remove`.
pub const DEFAULT_REMOVE_DEADLINE: Duration = Duration::from_secs(5);
/// The poll interval while waiting for the removal.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// The default spawn distance: the menu's own distance for a car (`AllInOneSpawnMenu.lua:19`).
pub const DEFAULT_SPAWN_DIST: f64 = 10.0;

/// One value of an `[ok]` reply, as `mercs2_formats::lua_exec::format_value` renders it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Num(f64),
    Str(String),
    Table,
    Function,
    /// Anything else (`<tt=… val=…>`, unreadable strings, or a token a tab inside a string split).
    Other(String),
}

/// A whole REPL reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Ok(Vec<Value>),
    Runtime(String),
    Compile(String),
    Bridge(String),
    Unrecognized(String),
}

fn is_number_text(t: &str) -> bool {
    t.bytes().any(|b| b.is_ascii_digit()) && t.bytes().all(|b| b.is_ascii_digit() || b"+-.e".contains(&b))
}

/// One tab-separated token of an `[ok]` reply.
pub fn parse_value(tok: &str) -> Value {
    match tok {
        "nil" => Value::Nil,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        "<table>" => Value::Table,
        "<function>" => Value::Function,
        "1.#INF" => Value::Num(f64::INFINITY),
        "-1.#INF" => Value::Num(f64::NEG_INFINITY),
        "1.#QNAN" | "-1.#IND" => Value::Num(f64::NAN),
        _ if tok.len() >= 2 && tok.starts_with('"') && tok.ends_with('"') => {
            Value::Str(tok[1..tok.len() - 1].to_string())
        }
        _ if is_number_text(tok) => tok.parse::<f64>().map(Value::Num).unwrap_or_else(|_| Value::Other(tok.to_string())),
        _ => Value::Other(tok.to_string()),
    }
}

/// Parse the text [`Bridge::eval`] returned.
pub fn parse_reply(text: &str) -> Reply {
    if text == "[ok]" {
        Reply::Ok(Vec::new())
    } else if let Some(rest) = text.strip_prefix("[ok]\t") {
        Reply::Ok(rest.split('\t').map(parse_value).collect())
    } else if let Some(rest) = text.strip_prefix("[runtime]") {
        Reply::Runtime(rest.trim_start().to_string())
    } else if let Some(rest) = text.strip_prefix("[compile]") {
        Reply::Compile(rest.trim_start().to_string())
    } else if let Some(rest) = text.strip_prefix("[bridge]") {
        Reply::Bridge(rest.trim_start().to_string())
    } else {
        Reply::Unrecognized(text.to_string())
    }
}

/// The shape one returned value must have.
#[derive(Debug, Clone)]
pub enum Expect {
    Bool(bool),
    AnyBool,
    Num(f64),
    AnyNum,
    Str(&'static str),
    AnyStr,
}

fn value_matches(e: &Expect, v: &Value) -> bool {
    match (e, v) {
        (Expect::Bool(want), Value::Bool(got)) => want == got,
        (Expect::AnyBool, Value::Bool(_)) => true,
        (Expect::Num(want), Value::Num(got)) => want == got,
        (Expect::AnyNum, Value::Num(got)) => !got.is_nan(),
        (Expect::Str(want), Value::Str(got)) => want == got,
        (Expect::AnyStr, Value::Str(_)) => true,
        _ => false,
    }
}

/// Judge a reply against the exact list of values it must return.
pub fn judge(reply: &Reply, expect: &[Expect]) -> Result<Vec<Value>, String> {
    let values = match reply {
        Reply::Ok(v) => v,
        other => return Err(format!("not an [ok] reply: {other:?}")),
    };
    if values.len() != expect.len() {
        return Err(format!("expected {} values {expect:?}, got {}: {values:?}", expect.len(), values.len()));
    }
    for (i, (e, v)) in expect.iter().zip(values).enumerate() {
        if !value_matches(e, v) {
            return Err(format!("value {} is {v:?}, expected {e:?} (all: {values:?})", i + 1));
        }
    }
    Ok(values.clone())
}

/// A string as a Lua literal. Only printable ASCII is accepted: the game's `"\ddd"` escapes are not
/// stock Lua's (lua-bridge README, "Text encoding"), so anything that would need one is refused.
pub fn lua_str(s: &str) -> Result<String, String> {
    if let Some(c) = s.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(format!("{s:?} holds {c:?}: only printable ASCII can be passed into a chunk"));
    }
    Ok(format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
}

/// The outcome of one check.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: &'static str,
    pub pass: bool,
    pub evidence: String,
}

impl std::fmt::Display for CheckResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}: {}", if self.pass { "PASS" } else { "FAIL" }, self.name, self.evidence)
    }
}

/// Why no check could run.
#[derive(Debug)]
pub enum Unreachable {
    /// The socket could not be opened.
    Connect(BridgeError),
    /// The socket opened but the first chunk got no answer.
    NoAnswer(BridgeError),
}

impl std::fmt::Display for Unreachable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreachable::Connect(e) => write!(f, "{e}"),
            Unreachable::NoAnswer(e) => write!(f, "connected, but the first chunk got no answer: {e}"),
        }
    }
}

/// 0 when every check passed, 1 otherwise.
pub fn exit_code(results: &[CheckResult]) -> i32 {
    if !results.is_empty() && results.iter().all(|r| r.pass) {
        0
    } else {
        1
    }
}

struct Runner {
    bridge: Bridge,
    results: Vec<CheckResult>,
}

impl Runner {
    fn record(&mut self, name: &'static str, pass: bool, evidence: String) {
        self.results.push(CheckResult { name, pass, evidence });
    }

    fn not_run(&mut self, name: &'static str, because: &str) {
        self.record(name, false, format!("not run: {because}"));
    }

    /// Send `chunk` (marked with `name`), returning the raw reply text, or recording a FAIL.
    fn send(&mut self, name: &'static str, body: &str) -> Option<String> {
        match self.bridge.eval(&format!("-- ess-live:{name}\n{body}")) {
            Ok(text) => Some(text),
            Err(e) => {
                self.record(name, false, format!("bridge error: {e}"));
                None
            }
        }
    }

    /// Send, parse and judge one check. `Some(values)` iff it passed.
    fn check(&mut self, name: &'static str, body: &str, expect: &[Expect]) -> Option<Vec<Value>> {
        let text = self.send(name, body)?;
        match judge(&parse_reply(&text), expect) {
            Ok(values) => {
                self.record(name, true, format!("{text:?}"));
                Some(values)
            }
            Err(why) => {
                self.record(name, false, format!("{text:?} — {why}"));
                None
            }
        }
    }
}

/// `n(...)`: the count of a call's results, then the results — `select('#', ...)` keeps trailing nils.
const ARITY: &str = "local function n(...) return select('#', ...), ... end\n";

/// Run every requested check. `Err` only when the bridge cannot be reached at all.
pub fn run(opts: &Options) -> Result<Vec<CheckResult>, Unreachable> {
    let bridge = Bridge::connect_at(opts.addr.as_str()).map_err(Unreachable::Connect)?;
    let mut r = Runner { bridge, results: Vec::new() };

    // 1. The bridge answers. A transport failure on the very first chunk means nothing is answering.
    let first = r.bridge.eval("-- ess-live:bridge\nreturn 1+1").map_err(Unreachable::NoAnswer)?;
    match judge(&parse_reply(&first), &[Expect::Num(2.0)]) {
        Ok(_) => r.record("bridge", true, format!("{first:?}")),
        Err(why) => r.record("bridge", false, format!("{first:?} — {why}")),
    }

    // 2. Ess is loaded (00_core.lua:35-37).
    if let Some(v) = r.check(
        "ess_loaded",
        "local E = rawget(_G, 'Ess')\n\
         if type(E) ~= 'table' then return type(E) end\n\
         return type(E), type(E.VERSION), tostring(E.VERSION)",
        &[Expect::Str("table"), Expect::Str("string"), Expect::AnyStr],
    ) {
        if let (Some(last), Some(Value::Str(ver))) = (r.results.last_mut(), v.get(2)) {
            last.evidence = format!("Ess.VERSION = {ver}; {}", last.evidence);
        }
    }

    // 3. Each Loader call Ess makes, the way Ess makes it.
    r.check(
        "loader_printf",
        "local c = select('#', Loader.Printf('[ess-live] Loader.Printf probe'))\n\
         Ess.Log('ess-live: Ess.Log probe')\n\
         return c",
        &[Expect::Num(0.0)],
    );
    r.check(
        "loader_pop_key_events",
        &format!(
            "{ARITY}local c, e = n(Loader.PopKeyEvents())\n\
             local p = Ess.Input.poll()\n\
             return c, type(e), type(p.pressed), type(p.down), type(p.down(0x10))"
        ),
        &[Expect::Num(1.0), Expect::Str("string"), Expect::Str("table"), Expect::Str("function"), Expect::Str("boolean")],
    );
    r.check(
        "loader_get_keyboard_state",
        &format!(
            "{ARITY}local c, ks = n(Loader.GetKeyboardState())\n\
             local len, bad = -1, -1\n\
             if type(ks) == 'string' then\n\
             \x20 len, bad = #ks, 0\n\
             \x20 for i = 1, #ks do local b = string.byte(ks, i); if b ~= 0 and b ~= 128 then bad = bad + 1 end end\n\
             end\n\
             return c, type(ks), len, bad, type(Ess.Input.held(0x10))"
        ),
        &[Expect::Num(1.0), Expect::Str("string"), Expect::Num(256.0), Expect::Num(0.0), Expect::Str("boolean")],
    );
    r.check(
        "loader_is_key_down",
        "local ok, held = pcall(Loader.IsKeyDown, 0x10)\n\
         local c = select('#', Loader.IsKeyDown(0x10))\n\
         local badOk = pcall(Loader.IsKeyDown, '16')\n\
         return ok, type(held), c, badOk",
        &[Expect::Bool(true), Expect::Str("boolean"), Expect::Num(1.0), Expect::Bool(false)],
    );
    r.check(
        "loader_clear_key_events",
        "local c = select('#', Loader.ClearKeyEvents())\n\
         local ok = pcall(Loader.ClearKeyEvents)\n\
         local after = Loader.PopKeyEvents()\n\
         local ce = select('#', Ess.Input.clear())\n\
         return c, ok, #after, ce",
        &[Expect::Num(0.0), Expect::Bool(true), Expect::Num(0.0), Expect::Num(0.0)],
    );
    r.check(
        "loader_save_load_var",
        &format!(
            "{ARITY}local K = '{SAVE_PREFIX}_raw'\n\
             local r = {{ pcall(function()\n\
             \x20 local cs = select('#', Loader.SaveVar(K, 7))\n\
             \x20 local cl, v = n(Loader.LoadVar(K))\n\
             \x20 local cd = select('#', Loader.SaveVar(K, nil))\n\
             \x20 local cg, g = n(Loader.LoadVar(K))\n\
             \x20 local badOk = pcall(Loader.SaveVar, K, {{}})\n\
             \x20 return cs, cl, type(v), v, cd, cg, type(g), badOk\n\
             end) }}\n\
             pcall(Loader.SaveVar, K, nil)\n\
             if not r[1] then error(r[2], 0) end\n\
             return r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]"
        ),
        &[
            Expect::Num(0.0),
            Expect::Num(1.0),
            Expect::Str("number"),
            Expect::Num(7.0),
            Expect::Num(0.0),
            Expect::Num(1.0),
            Expect::Str("nil"),
            Expect::Bool(false),
        ],
    );
    r.check(
        "loader_get_load_phase",
        &format!("{ARITY}local c, i, nm = n(Loader.GetLoadPhase())\nreturn c, type(i), type(nm), i, nm"),
        &[Expect::Num(2.0), Expect::Str("number"), Expect::Str("string"), Expect::AnyNum, Expect::AnyStr],
    );

    // 4. Ess.SaveVar round trip through Ess's own API (22_state.lua:44-65), always cleaned up.
    r.check(
        "ess_savevar_roundtrip",
        &format!(
            "local sv = Ess.SaveVar.ns('{SAVE_PREFIX}')\n\
             local r = {{ pcall(function()\n\
             \x20 sv:set('num', 42.5); sv:set('str', 'ess-live'); sv:set('bool', true)\n\
             \x20 local a, b, c, f = sv:get('num'), sv:get('str'), sv:get('bool'), sv:flag('bool')\n\
             \x20 sv:set('num', nil); sv:set('str', nil); sv:set('bool', nil)\n\
             \x20 return a, b, c, f, sv:get('num', 'default'), sv:get('str', 'default'), sv:get('bool', 'default'), sv:flag('bool')\n\
             end) }}\n\
             for _, k in ipairs({{ 'num', 'str', 'bool' }}) do pcall(Loader.SaveVar, '{SAVE_PREFIX}_' .. k, nil) end\n\
             if not r[1] then error(r[2], 0) end\n\
             return r[2], r[3], r[4], r[5], r[6], r[7], r[8], r[9]"
        ),
        &[
            Expect::Num(42.5),
            Expect::Str("ess-live"),
            Expect::Bool(true),
            Expect::Bool(true),
            Expect::Str("default"),
            Expect::Str("default"),
            Expect::Str("default"),
            Expect::Bool(false),
        ],
    );

    // 5. The menu, toggled the way one hotkey press runs it.
    if let Some(menu) = &opts.menu {
        menu_checks(&mut r, menu, opts);
    }

    // 6. The spawn path ctx:spawn takes.
    if let Some(template) = &opts.spawn {
        spawn_checks(&mut r, template, opts);
    }

    Ok(r.results)
}

/// The menu precondition: close an already-open Ess menu through `Menu:close`, then report
/// `type(S), wasOpen, closedId, closedOk, stillOpen, hasPlayer, hasCharacter`.
const MENU_PRECONDITION: &str = "local S = Ess.UI._S\n\
     local id = S.openId\n\
     local wasOpen = false\n\
     if id ~= nil and type(S.menus) == 'table' and type(S.menus[id]) == 'table' then wasOpen = S.menus[id].open == true end\n\
     local closedId = 'none'\n\
     local closedOk = true\n\
     if wasOpen then\n\
     \x20 local rt = S.menus[id]\n\
     \x20 if type(rt.menu) ~= 'table' then return 'the open menu keeps no menu object to close', tostring(id) end\n\
     \x20 rt.menu:close()\n\
     \x20 closedId = tostring(id)\n\
     \x20 closedOk = rt.open == false\n\
     end\n\
     local stillOpen = false\n\
     if S.openId ~= nil and type(S.menus[S.openId]) == 'table' then stillOpen = S.menus[S.openId].open == true end\n\
     return type(S), wasOpen, closedId, closedOk, stillOpen, (Player.GetLocalPlayer() and true or false), (Player.GetLocalCharacter() and true or false)";

const MENU_PRECONDITION_EXPECT: [Expect; 7] = [
    Expect::Str("table"),
    Expect::AnyBool,
    Expect::AnyStr,
    Expect::Bool(true),
    Expect::Bool(false),
    Expect::Bool(true),
    Expect::Bool(true),
];

/// swf_callback, part 1 (sent before the menu is opened).
///
/// PRESENT: the engine members Ess's SWF path calls — `MrxGuiBase.FlashWidget:new`, `:SetLocation`,
/// `:SetSwfFile(file, onLoad, {})` whose 2nd argument IS the load callback, `MrxGuiBase.AddWidget`
/// (40_gfx.lua:52-63), `:SetFlashEventHandler` (40_gfx.lua:91) and `:CallActionScriptCallback`
/// (40_gfx.lua:79).
///
/// ARM: the movie is loaded only when Ess's shared runtime does not exist yet (`rtEnsure`,
/// 42_ui_engine.lua:329-341, rebuilt after each world load by 42_ui_engine.lua:686-691), through
/// `Ess.Gfx.widget(Ess.UI.FILES.runtime, ..., onLoad)` looked up at call time (42_ui_engine.lua:333).
/// Ess records which path made the runtime ready only in a log line (42_ui_engine.lua:323), so the
/// probe wraps `Ess.Gfx.widget` to wrap that one onLoad with a recorder, and forwards everything else
/// unchanged. The recorder notes whether Ess was already ready (the 3 s timeout fallback,
/// 42_ui_engine.lua:380-391) when the callback fired.
///
/// Returns: 6 member types, AddWidget's type, whether the runtime already exists, whether armed.
const SWF_ARM: &str = "local function ty(t, k) if type(t) ~= 'table' then return 'nil' end return type(t[k]) end\n\
     local FW = type(MrxGuiBase) == 'table' and MrxGuiBase.FlashWidget or nil\n\
     local rtExists = Ess.UI._S.rt ~= nil\n\
     local armed = false\n\
     if type(FW) == 'table' and type(FW.SetSwfFile) == 'function' and not rtExists then\n\
     \x20 local orig = Ess.Gfx.widget\n\
     \x20 _G.__ess_live_probe_swf = { orig = orig, built = false, fired = false, readyAtFire = false }\n\
     \x20 Ess.Gfx.widget = function(file, x, y, w, h, onLoad)\n\
     \x20   local rec = _G.__ess_live_probe_swf\n\
     \x20   if rec and not rec.built and file == Ess.UI.FILES.runtime and onLoad ~= nil then\n\
     \x20     rec.built = true\n\
     \x20     local inner = onLoad\n\
     \x20     onLoad = function(...)\n\
     \x20       local r2 = _G.__ess_live_probe_swf\n\
     \x20       if r2 and not r2.fired then r2.fired = true; r2.readyAtFire = Ess.UI._rtReady() end\n\
     \x20       return inner(...)\n\
     \x20     end\n\
     \x20   end\n\
     \x20   return orig(file, x, y, w, h, onLoad)\n\
     \x20 end\n\
     \x20 armed = true\n\
     end\n\
     return ty(MrxGuiBase, 'FlashWidget'), ty(FW, 'new'), ty(FW, 'SetLocation'), ty(FW, 'SetSwfFile'), ty(FW, 'SetFlashEventHandler'), ty(FW, 'CallActionScriptCallback'), ty(MrxGuiBase, 'AddWidget'), rtExists, armed";

/// swf_callback, part 2 (sent right after the menu-open chunk): put `Ess.Gfx.widget` back.
/// Returns: whether the runtime widget was built through it, whether restored, whether rt exists.
const SWF_RESTORE: &str = "local rec = _G.__ess_live_probe_swf\n\
     if rec == nil then return 'not armed' end\n\
     local restored = false\n\
     if rec.orig ~= nil then Ess.Gfx.widget = rec.orig; rec.orig = nil; restored = true end\n\
     return rec.built, restored, Ess.UI._S.rt ~= nil";

/// swf_callback, part 3 (polled): fired, Ess-was-already-ready-when-it-fired, Ess ready now.
const SWF_POLL: &str = "local rec = _G.__ess_live_probe_swf\n\
     if rec == nil then return 'not armed' end\n\
     return rec.fired, rec.readyAtFire == true, Ess.UI._rtReady()";

/// swf_callback, part 4: drop the record. The recorder left inside the engine's pending callback
/// reads the global each time, so it becomes a pure pass-through.
const SWF_CLEANUP: &str = "_G.__ess_live_probe_swf = nil\nreturn true";

const SWF_ARM_EXPECT: [Expect; 9] = [
    Expect::Str("table"),
    Expect::Str("function"),
    Expect::Str("function"),
    Expect::Str("function"),
    Expect::Str("function"),
    Expect::Str("function"),
    Expect::Str("function"),
    Expect::AnyBool,
    Expect::AnyBool,
];

/// Send one swf_callback chunk; the transport error becomes swf_callback evidence.
fn swf_eval(r: &mut Runner, part: &str, body: &str) -> Result<(String, Reply), String> {
    r.bridge
        .eval(&format!("-- ess-live:{part}\n{body}"))
        .map(|t| {
            let p = parse_reply(&t);
            (t, p)
        })
        .map_err(|e| format!("{part}: bridge error: {e}"))
}

/// Arm swf_callback. `Ok(evidence)` when armed; `Err` = swf_callback FAIL evidence (nothing armed).
fn swf_arm(r: &mut Runner) -> Result<String, String> {
    let (text, reply) = swf_eval(r, "swf_arm", SWF_ARM)?;
    let v = judge(&reply, &SWF_ARM_EXPECT).map_err(|why| format!("PRESENT check failed: {text:?} — {why}"))?;
    match (&v[7], &v[8]) {
        (Value::Bool(false), Value::Bool(true)) => Ok(format!("PRESENT {text:?}")),
        (Value::Bool(true), _) => Err(format!(
            "PRESENT, but no SWF load to observe: Ess's shared runtime (Ess.UI._S.rt) already exists this world, \
             and the movie loads only when rtEnsure builds it (42_ui_engine.lua:329-341; reset on world load, \
             42_ui_engine.lua:686-691). Reload the world and run ess-live before any Ess UI is shown. {text:?}"
        )),
        _ => Err(format!("not armed: {text:?}")),
    }
}

/// Restore, then poll until the callback fires or the deadline passes, and record swf_callback.
fn swf_finish(r: &mut Runner, armed_evidence: String, restore: Result<(String, Reply), String>, opened_at: Instant, opts: &Options) {
    let restore = restore.and_then(|(text, reply)| {
        judge(&reply, &[Expect::AnyBool, Expect::Bool(true), Expect::AnyBool])
            .map(|v| (text.clone(), v))
            .map_err(|why| format!("restore of Ess.Gfx.widget failed: {text:?} — {why}"))
    });
    let (restore_text, v) = match restore {
        Ok(x) => x,
        Err(e) => {
            r.record("swf_callback", false, format!("{e}; {armed_evidence}"));
            return;
        }
    };
    if v[0] != Value::Bool(true) {
        r.record(
            "swf_callback",
            false,
            format!(
                "opening the menu did not build Ess's runtime widget through Ess.Gfx.widget \
                 (42_ui_engine.lua:333): {restore_text:?}; {armed_evidence}"
            ),
        );
        return;
    }
    let mut polls = 0u32;
    loop {
        let (text, reply) = match swf_eval(r, "swf_poll", SWF_POLL) {
            Ok(x) => x,
            Err(e) => {
                r.record("swf_callback", false, format!("{e}; {armed_evidence}"));
                return;
            }
        };
        polls += 1;
        let ms = opened_at.elapsed().as_millis();
        let v = match judge(&reply, &[Expect::AnyBool, Expect::AnyBool, Expect::AnyBool]) {
            Ok(v) => v,
            Err(why) => {
                r.record("swf_callback", false, format!("{text:?} — {why}; {armed_evidence}"));
                return;
            }
        };
        if v[0] == Value::Bool(true) {
            let path = if v[1] == Value::Bool(true) {
                "Ess had already gone ready by the 3 s timeout fallback (42_ui_engine.lua:386-388) before the callback fired late"
            } else {
                "Ess went ready through the load callback (42_ui_engine.lua:333-335), before the timeout fallback"
            };
            r.record(
                "swf_callback",
                true,
                format!("RUNS: the SetSwfFile load callback fired, observed by poll {polls} at {ms} ms after the menu-open reply; path: {path}. {text:?}; {armed_evidence}"),
            );
            return;
        }
        if opened_at.elapsed() >= opts.swf_deadline {
            let path = if v[2] == Value::Bool(true) {
                "Ess went ready through the 3 s timeout fallback (42_ui_engine.lua:386-388)"
            } else {
                "Ess is not ready either"
            };
            r.record(
                "swf_callback",
                false,
                format!("the SetSwfFile load callback did not fire within {ms} ms ({polls} polls); {path}. {text:?}; {armed_evidence}"),
            );
            return;
        }
        std::thread::sleep(opts.poll_interval);
    }
}

fn menu_checks(r: &mut Runner, menu: &str, opts: &Options) {
    let path = match lua_str(menu) {
        Ok(p) => p,
        Err(e) => {
            r.record("menu_precondition", false, format!("--menu: {e}"));
            r.not_run("menu_open", "menu_precondition failed");
            r.not_run("menu_close", "menu_precondition failed");
            r.not_run("swf_callback", "menu_precondition failed");
            return;
        }
    };
    // Menu:open refuses without a local player and character (49_ui_menu.lua:179-182). An Ess menu that
    // is already open is closed first through Ess's own Menu:close (49_ui_menu.lua:211-220), on the
    // menu object its runtime state keeps (`rt.menu = self`, 49_ui_menu.lua:202); the check fails only
    // when that close does not take effect.
    let pre = r.check("menu_precondition", MENU_PRECONDITION, &MENU_PRECONDITION_EXPECT);
    if let (Some(v), Some(last)) = (&pre, r.results.last_mut()) {
        if let (Value::Bool(true), Value::Str(id)) = (&v[1], &v[2]) {
            last.evidence = format!(
                "an Ess menu was already open: closed {id:?} first via Menu:close (49_ui_menu.lua:211-220); {}",
                last.evidence
            );
        }
    }
    if pre.is_none() {
        r.not_run("menu_open", "menu_precondition failed");
        r.not_run("menu_close", "menu_precondition failed");
        r.not_run("swf_callback", "menu_precondition failed");
        return;
    }
    let armed = swf_arm(r);
    let opened = r.check(
        "menu_open",
        &format!(
            "local ran = Loader.LoadFile({path})\n\
             local S = Ess.UI._S\n\
             local id = S.openId\n\
             local isOpen = false\n\
             if id ~= nil and type(S.menus) == 'table' and type(S.menus[id]) == 'table' then isOpen = S.menus[id].open == true end\n\
             _G.{MENU_ID_GLOBAL} = id\n\
             return ran, type(id), tostring(id), isOpen"
        ),
        &[Expect::Bool(true), Expect::Str("string"), Expect::AnyStr, Expect::Bool(true)],
    );
    let opened_at = Instant::now();
    // Restore Ess.Gfx.widget straight after the open, whatever the open's outcome.
    let swf = armed.map(|ev| (ev, swf_eval(r, "swf_restore", SWF_RESTORE)));
    if opened.is_none() {
        r.not_run("menu_close", "menu_open failed");
    } else {
        menu_close(r, &path);
    }
    match swf {
        Ok((ev, restore)) => {
            swf_finish(r, ev, restore, opened_at, opts);
            if let Err(e) = swf_eval(r, "swf_cleanup", SWF_CLEANUP) {
                r.record("swf_cleanup", false, e);
            }
        }
        Err(e) => r.record("swf_callback", false, e),
    }
}

fn menu_close(r: &mut Runner, path: &str) {
    r.check(
        "menu_close",
        &format!(
            "local id = _G.{MENU_ID_GLOBAL}\n\
             _G.{MENU_ID_GLOBAL} = nil\n\
             local ran = Loader.LoadFile({path})\n\
             local S = Ess.UI._S\n\
             local closed = false\n\
             if id ~= nil and type(S.menus) == 'table' and type(S.menus[id]) == 'table' then closed = S.menus[id].open == false end\n\
             return ran, tostring(id), closed, S.openId == nil"
        ),
        &[Expect::Bool(true), Expect::AnyStr, Expect::Bool(true), Expect::Bool(true)],
    );
}

fn spawn_checks(r: &mut Runner, template: &str, opts: &Options) {
    let names = ["spawn", "spawn_remove", "spawn_gone", "spawn_cleanup"];
    let tmpl = match lua_str(template) {
        Ok(t) => t,
        Err(e) => {
            r.record("spawn", false, format!("--spawn: {e}"));
            for &n in &names[1..] {
                r.not_run(n, "spawn failed");
            }
            return;
        }
    };
    if !opts.spawn_dist.is_finite() || opts.spawn_dist < 0.0 {
        r.record("spawn", false, format!("--spawn-dist {} is not a finite distance >= 0", opts.spawn_dist));
        for &n in &names[1..] {
            r.not_run(n, "spawn failed");
        }
        return;
    }
    // menu_ctx is a file-local (49_ui_menu.lua:61), so the only way to the real ctx:spawn is a menu's
    // own action dispatch: Menu:_choose calls node.action(menu_ctx(self)) (49_ui_menu.lua:148-155).
    let spawned = r.send(
        "spawn",
        &format!(
            "local m = Ess.UI.Menu{{ title = '{SPAWN_MENU_ID}', id = '{SPAWN_MENU_ID}' }}\n\
             local cap\n\
             m:entry('probe', function(ctx) cap = ctx end)\n\
             m:_choose({{ _node = m._children[1] }})\n\
             Ess.UI._S.menus['{SPAWN_MENU_ID}'] = nil\n\
             if cap == nil then return 'the menu action never ran: no ctx' end\n\
             local u = cap:spawn({tmpl}, {dist})\n\
             _G.{SPAWN_GLOBAL} = u\n\
             return type(u), Ess.Object.valid(u), Ess.Object.alive(u)",
            dist = opts.spawn_dist
        ),
    );
    let Some(spawned) = spawned else {
        for &n in &names[1..] {
            r.not_run(n, "spawn got no reply");
        }
        return;
    };
    match judge(&parse_reply(&spawned), &[Expect::Str("userdata"), Expect::Bool(true), Expect::Bool(true)]) {
        Ok(_) => r.record("spawn", true, format!("{spawned:?}")),
        Err(why) => r.record("spawn", false, format!("{spawned:?} — {why}")),
    }

    // Always try to remove whatever was spawned, even when the spawn check failed.
    let removed = r.check(
        "spawn_remove",
        &format!(
            "local u = _G.{SPAWN_GLOBAL}\n\
             if u == nil then return 'nothing spawned' end\n\
             Ess.Object.remove(u)\n\
             return true"
        ),
        &[Expect::Bool(true)],
    );
    if removed.is_some() {
        spawn_gone(r, opts);
    } else {
        r.not_run("spawn_gone", "spawn_remove failed");
    }
    r.check("spawn_cleanup", &format!("_G.{SPAWN_GLOBAL} = nil\nreturn true"), &[Expect::Bool(true)]);
}

/// Poll `Ess.Object.alive` until it reads false or the deadline passes (11_object.lua:224-229).
fn spawn_gone(r: &mut Runner, opts: &Options) {
    let start = Instant::now();
    let mut polls = 0u32;
    loop {
        let Some(text) = r.send("spawn_gone", &format!("return Ess.Object.alive(_G.{SPAWN_GLOBAL})")) else {
            return;
        };
        polls += 1;
        match judge(&parse_reply(&text), &[Expect::AnyBool]) {
            Ok(v) if v[0] == Value::Bool(false) => {
                r.record(
                    "spawn_gone",
                    true,
                    format!("{text:?} after {polls} polls, {} ms", start.elapsed().as_millis()),
                );
                return;
            }
            Ok(_) => {
                if start.elapsed() >= opts.remove_deadline {
                    r.record(
                        "spawn_gone",
                        false,
                        format!("{text:?} — still alive after {polls} polls, {} ms", start.elapsed().as_millis()),
                    );
                    return;
                }
                std::thread::sleep(opts.poll_interval);
            }
            Err(why) => {
                r.record("spawn_gone", false, format!("{text:?} — {why}"));
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_bridge::Server;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// The check name a chunk was sent for, from its `-- ess-live:<name>` first line.
    fn marker(chunk: &str) -> &str {
        chunk.lines().next().and_then(|l| l.strip_prefix("-- ess-live:")).unwrap_or("")
    }

    /// The replies a healthy game gives, per check.
    fn healthy(name: &str) -> String {
        match name {
            "bridge" => "[ok]\t2",
            "ess_loaded" => "[ok]\t\"table\"\t\"string\"\t\"0.7.0\"",
            "loader_printf" => "[ok]\t0",
            "loader_pop_key_events" => "[ok]\t1\t\"string\"\t\"table\"\t\"function\"\t\"boolean\"",
            "loader_get_keyboard_state" => "[ok]\t1\t\"string\"\t256\t0\t\"boolean\"",
            "loader_is_key_down" => "[ok]\ttrue\t\"boolean\"\t1\tfalse",
            "loader_clear_key_events" => "[ok]\t0\ttrue\t0\t0",
            "loader_save_load_var" => "[ok]\t0\t1\t\"number\"\t7\t0\t1\t\"nil\"\tfalse",
            "loader_get_load_phase" => "[ok]\t2\t\"number\"\t\"string\"\t20\t\"World fully loaded (GlobalExit)\"",
            "ess_savevar_roundtrip" => {
                "[ok]\t42.5\t\"ess-live\"\ttrue\ttrue\t\"default\"\t\"default\"\t\"default\"\tfalse"
            }
            "menu_precondition" => "[ok]\t\"table\"\tfalse\t\"none\"\ttrue\tfalse\ttrue\ttrue",
            "menu_open" => "[ok]\ttrue\t\"string\"\t\"Ferd's AiO SpawnMenu v0.85\"\ttrue",
            "menu_close" => "[ok]\ttrue\t\"Ferd's AiO SpawnMenu v0.85\"\ttrue\ttrue",
            "swf_arm" => {
                "[ok]\t\"table\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\tfalse\ttrue"
            }
            "swf_restore" => "[ok]\ttrue\ttrue\ttrue",
            "swf_poll" => "[ok]\ttrue\tfalse\ttrue",
            "swf_cleanup" => "[ok]\ttrue",
            "spawn" => "[ok]\t\"userdata\"\ttrue\ttrue",
            "spawn_remove" => "[ok]\ttrue",
            "spawn_gone" => "[ok]\tfalse",
            "spawn_cleanup" => "[ok]\ttrue",
            other => panic!("a chunk with no healthy reply: {other:?}"),
        }
        .to_string()
    }

    /// A fake game on an ephemeral port answering with `handler`.
    fn serve(handler: impl FnMut(&str) -> String + Send + 'static) -> String {
        let server = Server::bind("127.0.0.1:0").expect("bind ephemeral");
        let addr = server.local_addr().expect("local addr").to_string();
        let mut handler = handler;
        std::thread::spawn(move || {
            let _ = server.serve(|chunk| handler(chunk));
        });
        addr
    }

    fn opts(addr: String) -> Options {
        Options {
            addr,
            menu: Some("OnKey/AllInOneSpawnMenu.lua".into()),
            spawn: Some("HMMWV (Softtop)".into()),
            spawn_dist: DEFAULT_SPAWN_DIST,
            remove_deadline: Duration::from_millis(1500),
            poll_interval: Duration::from_millis(10),
            swf_deadline: Duration::from_millis(1500),
        }
    }

    fn by_name<'a>(results: &'a [CheckResult], name: &str) -> &'a CheckResult {
        results.iter().find(|r| r.name == name).unwrap_or_else(|| panic!("no result for {name}: {results:#?}"))
    }

    const ALL: &[&str] = &[
        "bridge",
        "ess_loaded",
        "loader_printf",
        "loader_pop_key_events",
        "loader_get_keyboard_state",
        "loader_is_key_down",
        "loader_clear_key_events",
        "loader_save_load_var",
        "loader_get_load_phase",
        "ess_savevar_roundtrip",
        "menu_precondition",
        "menu_open",
        "menu_close",
        "swf_callback",
        "spawn",
        "spawn_remove",
        "spawn_gone",
        "spawn_cleanup",
    ];

    #[test]
    fn ess_live_all_good_passes_every_check_and_exits_0() {
        // The removed object stays alive for two polls, as the deferred remove does.
        let gone_polls = Arc::new(AtomicUsize::new(0));
        let polls = gone_polls.clone();
        let addr = serve(move |chunk| match marker(chunk) {
            "spawn_gone" if polls.fetch_add(1, Ordering::SeqCst) < 2 => "[ok]\ttrue".into(),
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        let names: Vec<&str> = results.iter().map(|r| r.name).collect();
        assert_eq!(names, ALL);
        assert!(results.iter().all(|r| r.pass), "{results:#?}");
        assert_eq!(exit_code(&results), 0);
        assert_eq!(gone_polls.load(Ordering::SeqCst), 3);
        assert!(by_name(&results, "ess_loaded").evidence.starts_with("Ess.VERSION = 0.7.0"));
    }

    #[test]
    fn ess_live_without_menu_or_spawn_runs_only_the_core_checks() {
        let addr = serve(|chunk| healthy(marker(chunk)));
        let results = run(&Options { menu: None, spawn: None, ..opts(addr) }).expect("reachable");
        let names: Vec<&str> = results.iter().map(|r| r.name).collect();
        assert_eq!(names, &ALL[..10]);
        assert_eq!(exit_code(&results), 0);
    }

    #[test]
    fn ess_live_a_wrong_type_fails_that_check_only() {
        // A 255-byte keyboard state, and a boolean returned as a number.
        let addr = serve(|chunk| match marker(chunk) {
            "loader_get_keyboard_state" => "[ok]\t1\t\"string\"\t255\t0\t\"boolean\"".into(),
            "loader_is_key_down" => "[ok]\ttrue\t\"number\"\t1\tfalse".into(),
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        let failed: Vec<&str> = results.iter().filter(|r| !r.pass).map(|r| r.name).collect();
        assert_eq!(failed, ["loader_get_keyboard_state", "loader_is_key_down"]);
        assert!(by_name(&results, "loader_get_keyboard_state").evidence.contains("Num(255.0)"));
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_a_runtime_error_fails_with_the_message_as_evidence() {
        let addr = serve(|chunk| match marker(chunk) {
            "ess_savevar_roundtrip" => "[runtime] \"attempt to index field 'SaveVar' (a nil value)\"".into(),
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        let r = by_name(&results, "ess_savevar_roundtrip");
        assert!(!r.pass);
        assert!(r.evidence.contains("Runtime(") && r.evidence.contains("attempt to index field 'SaveVar'"), "{r:?}");
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_a_menu_that_does_not_open_fails_and_close_is_not_run() {
        let addr = serve(|chunk| match marker(chunk) {
            "menu_open" => "[ok]\ttrue\t\"nil\"\t\"nil\"\tfalse".into(),
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        assert!(!by_name(&results, "menu_open").pass);
        let close = by_name(&results, "menu_close");
        assert!(!close.pass && close.evidence.starts_with("not run"), "{close:?}");
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_an_open_menu_is_closed_first_then_open_and_close_pass() {
        let addr = serve(|chunk| match marker(chunk) {
            // The precondition chunk itself closes the open menu through Menu:close.
            "menu_precondition" => {
                assert!(chunk.contains("rt.menu:close()"), "the close goes through Ess's Menu:close");
                "[ok]\t\"table\"\ttrue\t\"Other Menu\"\ttrue\tfalse\ttrue\ttrue".into()
            }
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        let pre = by_name(&results, "menu_precondition");
        assert!(pre.pass, "{pre:?}");
        assert!(pre.evidence.starts_with("an Ess menu was already open: closed \"Other Menu\" first"), "{pre:?}");
        assert!(by_name(&results, "menu_open").pass);
        assert!(by_name(&results, "menu_close").pass);
        assert_eq!(exit_code(&results), 0);
    }

    #[test]
    fn ess_live_an_open_menu_the_close_does_not_shut_fails_the_precondition() {
        for reply in [
            // closedOk false: the menu's own open flag did not drop.
            "[ok]\t\"table\"\ttrue\t\"Other Menu\"\tfalse\tfalse\ttrue\ttrue",
            // stillOpen true: a menu is still open after the close.
            "[ok]\t\"table\"\ttrue\t\"Other Menu\"\ttrue\ttrue\ttrue\ttrue",
            // No menu object to close.
            "[ok]\t\"the open menu keeps no menu object to close\"\t\"Other Menu\"",
        ] {
            let addr = serve(move |chunk| match marker(chunk) {
                "menu_precondition" => reply.into(),
                name => healthy(name),
            });
            let results = run(&opts(addr)).expect("reachable");
            assert!(!by_name(&results, "menu_precondition").pass, "{reply}");
            for n in ["menu_open", "menu_close", "swf_callback"] {
                let c = by_name(&results, n);
                assert!(!c.pass && c.evidence.starts_with("not run"), "{c:?}");
            }
            assert_eq!(exit_code(&results), 1);
        }
    }

    #[test]
    fn ess_live_no_local_player_fails_the_precondition() {
        let addr = serve(|chunk| match marker(chunk) {
            "menu_precondition" => "[ok]\t\"table\"\tfalse\t\"none\"\ttrue\tfalse\tfalse\tfalse".into(),
            name => healthy(name),
        });
        let results = run(&opts(addr)).expect("reachable");
        assert!(!by_name(&results, "menu_precondition").pass);
        assert_eq!(exit_code(&results), 1);
    }

    /// The SWF chunks the fake game saw, in order.
    fn swf_order(log: &std::sync::Mutex<Vec<String>>) -> Vec<String> {
        log.lock().unwrap().iter().filter(|n| n.starts_with("swf_") || n.starts_with("menu_")).cloned().collect()
    }

    #[test]
    fn ess_live_swf_callback_fired_passes_and_names_the_callback_path() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = log.clone();
        let polls = Arc::new(AtomicUsize::new(0));
        let p = polls.clone();
        let addr = serve(move |chunk| {
            let name = marker(chunk).to_string();
            seen.lock().unwrap().push(name.clone());
            match name.as_str() {
                // Not fired on the first two polls, then fired before Ess was ready.
                "swf_poll" if p.fetch_add(1, Ordering::SeqCst) < 2 => "[ok]\tfalse\tfalse\tfalse".into(),
                n => healthy(n),
            }
        });
        let results = run(&opts(addr)).expect("reachable");
        let swf = by_name(&results, "swf_callback");
        assert!(swf.pass, "{swf:?}");
        assert!(swf.evidence.contains("RUNS") && swf.evidence.contains("through the load callback"), "{swf:?}");
        assert!(swf.evidence.contains("PRESENT"), "{swf:?}");
        assert_eq!(polls.load(Ordering::SeqCst), 3);
        // Armed before the open, restored straight after it, cleaned up last.
        let order = swf_order(&log);
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap_or_else(|| panic!("{n} missing: {order:?}"));
        assert!(pos("swf_arm") < pos("menu_open") && pos("menu_open") < pos("swf_restore"));
        assert!(pos("swf_restore") < pos("menu_close") && pos("menu_close") < pos("swf_poll"));
        assert_eq!(order.last().map(String::as_str), Some("swf_cleanup"));
        assert_eq!(exit_code(&results), 0);
    }

    #[test]
    fn ess_live_swf_callback_fired_late_passes_and_names_the_timeout_path() {
        let addr = serve(|chunk| match marker(chunk) {
            "swf_poll" => "[ok]\ttrue\ttrue\ttrue".into(),
            n => healthy(n),
        });
        let results = run(&opts(addr)).expect("reachable");
        let swf = by_name(&results, "swf_callback");
        assert!(swf.pass && swf.evidence.contains("timeout fallback") && swf.evidence.contains("fired late"), "{swf:?}");
    }

    #[test]
    fn ess_live_swf_callback_not_fired_fails_at_the_deadline_naming_the_timeout_path() {
        let cleaned = Arc::new(AtomicUsize::new(0));
        let c = cleaned.clone();
        let addr = serve(move |chunk| match marker(chunk) {
            "swf_poll" => "[ok]\tfalse\tfalse\ttrue".into(),
            "swf_cleanup" => {
                c.fetch_add(1, Ordering::SeqCst);
                healthy("swf_cleanup")
            }
            n => healthy(n),
        });
        let mut o = opts(addr);
        o.swf_deadline = Duration::from_millis(100);
        let results = run(&o).expect("reachable");
        let swf = by_name(&results, "swf_callback");
        assert!(!swf.pass, "{swf:?}");
        assert!(swf.evidence.contains("did not fire") && swf.evidence.contains("3 s timeout fallback"), "{swf:?}");
        assert_eq!(cleaned.load(Ordering::SeqCst), 1, "cleanup still runs");
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_swf_callback_handler_missing_fails_present_and_arms_nothing() {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = log.clone();
        let addr = serve(move |chunk| {
            let name = marker(chunk).to_string();
            seen.lock().unwrap().push(name.clone());
            match name.as_str() {
                // FlashWidget has no SetSwfFile: the instrumentation is not installed.
                "swf_arm" => {
                    "[ok]\t\"table\"\t\"function\"\t\"function\"\t\"nil\"\t\"function\"\t\"function\"\t\"function\"\tfalse\tfalse".into()
                }
                n => healthy(n),
            }
        });
        let results = run(&opts(addr)).expect("reachable");
        let swf = by_name(&results, "swf_callback");
        assert!(!swf.pass && swf.evidence.contains("PRESENT check failed"), "{swf:?}");
        let order = swf_order(&log);
        assert!(!order.iter().any(|n| n == "swf_restore" || n == "swf_poll" || n == "swf_cleanup"), "{order:?}");
        assert!(by_name(&results, "menu_open").pass && by_name(&results, "menu_close").pass);
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_swf_callback_with_the_runtime_already_built_fails_with_how_to_observe_it() {
        let addr = serve(|chunk| match marker(chunk) {
            "swf_arm" => {
                "[ok]\t\"table\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\t\"function\"\ttrue\tfalse".into()
            }
            n => healthy(n),
        });
        let results = run(&opts(addr)).expect("reachable");
        let swf = by_name(&results, "swf_callback");
        assert!(!swf.pass && swf.evidence.contains("already exists"), "{swf:?}");
    }

    #[test]
    fn ess_live_an_object_that_never_goes_away_fails_after_the_deadline() {
        let addr = serve(|chunk| match marker(chunk) {
            "spawn_gone" => "[ok]\ttrue".into(),
            name => healthy(name),
        });
        let mut o = opts(addr);
        o.remove_deadline = Duration::from_millis(100);
        let results = run(&o).expect("reachable");
        let gone = by_name(&results, "spawn_gone");
        assert!(!gone.pass && gone.evidence.contains("still alive"), "{gone:?}");
        assert!(by_name(&results, "spawn_cleanup").pass, "cleanup still runs");
        assert_eq!(exit_code(&results), 1);
    }

    #[test]
    fn ess_live_a_bridge_that_is_down_is_unreachable() {
        // Take a free port, then close it: nothing listens there.
        let addr = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
            l.local_addr().expect("addr").to_string()
        };
        match run(&opts(addr)) {
            Err(Unreachable::Connect(_)) => {}
            other => panic!("expected an unreachable bridge, got {other:?}"),
        }
    }

    #[test]
    fn ess_live_replies_parse_into_typed_values() {
        assert_eq!(parse_reply("[ok]"), Reply::Ok(vec![]));
        assert_eq!(
            parse_reply("[ok]\tnil\ttrue\t2.5\t\"hi\"\t<table>\t<function>\t1e+006\t-1.#INF\t<tt=7 val=0000ABCD>"),
            Reply::Ok(vec![
                Value::Nil,
                Value::Bool(true),
                Value::Num(2.5),
                Value::Str("hi".into()),
                Value::Table,
                Value::Function,
                Value::Num(1_000_000.0),
                Value::Num(f64::NEG_INFINITY),
                Value::Other("<tt=7 val=0000ABCD>".into()),
            ])
        );
        assert_eq!(parse_reply("[runtime] \"boom\""), Reply::Runtime("\"boom\"".into()));
        assert_eq!(parse_reply("[compile] \"x\""), Reply::Compile("\"x\"".into()));
        assert_eq!(parse_reply("[bridge] too big"), Reply::Bridge("too big".into()));
        assert_eq!(parse_reply("2"), Reply::Unrecognized("2".into()));
        // The string "2" is not the number 2.
        assert!(judge(&parse_reply("[ok]\t\"2\""), &[Expect::Num(2.0)]).is_err());
        // A tab inside a string splits it into tokens that match nothing, so it can never pass by luck.
        assert!(judge(&parse_reply("[ok]\t\"a\tb\""), &[Expect::AnyStr]).is_err());
        // Arity is exact.
        assert!(judge(&parse_reply("[ok]\t2\t3"), &[Expect::Num(2.0)]).is_err());
    }

    #[test]
    fn ess_live_lua_literals_escape_and_refuse_non_ascii() {
        assert_eq!(lua_str("OnKey/a b.lua").unwrap(), "\"OnKey/a b.lua\"");
        assert_eq!(lua_str("a\"b\\c").unwrap(), "\"a\\\"b\\\\c\"");
        assert!(lua_str("M\u{f6}bius").is_err());
        assert!(lua_str("a\nb").is_err());
    }
}
