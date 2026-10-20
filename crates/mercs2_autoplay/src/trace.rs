//! The call recorder: every call the game's Lua makes, with who made it and with what.
//!
//! Three record kinds go to one NDJSON stream, one object per line, in call order (`seq`):
//!
//! * **`engine`** — a call into an engine binding (`Pg.Spawn`, `Event.Create`, …). Every function in
//!   every [`NAMESPACES`] table (and the plain tables nested in them, e.g. `Hud.Cinematic`) is wrapped
//!   before any game module loads, so module-load-time captures (`local Spawn = Pg.Spawn`) capture the
//!   wrapper too. Same fields as `mods/lua_trace_asi`'s retail `lua_trace.ndjson` (`fn`, `argc`,
//!   `caller_src`, `args` with `t` = `num`/`str`/`bool`/`nil`/`tt<N>`), plus `ns`, so a harness trace
//!   and a retail capture of the same scenario can be diffed call for call.
//! * **`lua`** — a Lua function called from a *different* module than its own (calls inside one module
//!   are not recorded). `fn` is the name it is defined under in its module (`null` for local functions
//!   and closures); `called_as` is the name at the call site (`null` when no Lua instruction made the
//!   call). `via` lists the C frames between caller and callee: `["pcall"]`, or the binding that
//!   dispatched an engine callback (`["Post"]`). An empty `caller_src` means the engine called it with
//!   no Lua frame above it (a timer, an event fire, a state change).
//! * **`stub`** — a touch of an engine name that has no binding at all, answered by the host's auto-stub
//!   layer (`global:<Name>` on first read, `call:<path>` on each call). These are the places the trace
//!   *cannot* match retail yet.
//!
//! Every argument is encoded (not capped at 4 like the retail ASI): strings whole, tables one level deep
//! with keys sorted (table iteration order is not stable across runs) and the first 32 entries kept,
//! functions as the `@module:line` they were defined at, light userdata (engine guids) as hex. Inside
//! string arguments, Lua's `tostring` of a reference value (`function: 0x…`) has its address replaced
//! with `<addr>`; it is the one part of a script's output that differs between two identical runs.
//!
//! `caller_src` is `@<module>:<line>` with the module lowercased — the loader names each chunk after
//! the spelling its importer used (`import("MrxMissionFlow")`), which is not stable across importers.
//!
//! The Lua→Lua half is a `debug.sethook` call hook. Hooks are per Lua thread; the shipped corpus uses no
//! coroutines (no `coroutine.` anywhere in `mercs2_script/corpus`), so the main thread sees every call.

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::rc::Rc;

use mercs2_engine::script::bindings::NAMESPACES;
use mercs2_engine::script::mercs2_luac::rt::{Error as LuaError, Function, Result as LuaResult, Value, Variadic};
use mercs2_engine::script::ScriptHost;
use serde_json::{json, Map, Value as Json};

/// Chunk name of the recorder's own Lua; frames from it (the binding wrappers) are never recorded.
const SELF_CHUNK: &str = "@autoplay_trace";

/// Tables nested deeper than this inside an argument are written as a bare `{"t":"tt5"}`. One level
/// keeps an argument table's own fields (`{uBoat, "a"}`, a layer list, a config's scalars) and marks the
/// tables inside it; a second level mostly re-writes task objects reachable from `self`, which took
/// the new-game boot trace from 13 MB to 252 MB and its recording time from ~3.5 s to ~35 s.
const TABLE_DEPTH: u8 = 1;
/// Entries written per table (lowest keys first); `n` gives the table's full entry count.
const TABLE_ENTRIES: usize = 32;

const TRACE_LUA: &str = r#"
local rec_engine = __autoplay_rec_engine
local rec_lua = __autoplay_rec_lua
local rec_stub = __autoplay_rec_stub
local namespaces = __autoplay_namespaces
__autoplay_rec_engine, __autoplay_rec_lua, __autoplay_rec_stub, __autoplay_namespaces = nil, nil, nil, nil

local SELF = "@autoplay_trace"
local getinfo, getlocal, sethook = debug.getinfo, debug.getlocal, debug.sethook
local rawget, pairs, ipairs, type, error, unpack, getmetatable = rawget, pairs, ipairs, type, error, unpack, getmetatable
local sub, gmatch = string.sub, string.gmatch
local stdlib = {}
for _, t in ipairs({ _G, string, table, math, os, io, debug, coroutine, package }) do stdlib[t] = true end

-- Wrap every function of `t` (and of the plain tables nested in it) so the call is recorded first.
local wrapped = {}
local function wrap(path, t)
  if wrapped[t] then return end
  wrapped[t] = true
  local fns, subs = {}, {}
  for k, v in pairs(t) do
    if type(k) == "string" and sub(k, 1, 2) ~= "__" then
      if type(v) == "function" then fns[#fns + 1] = k
      elseif type(v) == "table" and not stdlib[v] and getmetatable(v) == nil then subs[#subs + 1] = k end
    end
  end
  for _, k in ipairs(fns) do
    local f = t[k]
    t[k] = function(...)
      local i = getinfo(2, "Sl")
      rec_engine(path, k, i and i.source or "", i and i.currentline or -1, ...)
      return f(...)
    end
  end
  for _, k in ipairs(subs) do wrap(path .. "." .. k, t[k]) end
end

for _, path in ipairs(namespaces) do
  local t = _G
  for part in gmatch(path, "[^%.]+") do
    t = rawget(t, part)
    if type(t) ~= "table" then error("binding namespace '" .. path .. "' is not installed") end
  end
  if stdlib[t] then error("binding namespace '" .. path .. "' is a standard-library table") end
  wrap(path, t)
end

-- Names with no binding at all, answered by the auto-stub layer. Level 3 is the code that touched the
-- name: 1 = this function, 2 = the stub's __index/__call metamethod.
local note_prev = rawget(_G, "__stub_note")
__stub_note = function(s)
  if sub(s, 1, 5) == "call:" or sub(s, 1, 7) == "global:" then
    local i = getinfo(3, "Sl")
    rec_stub(s, i and i.source or "", i and i.currentline or -1)
  end
  if note_prev then note_prev(s) end
end

-- The name a function is defined under: the key that holds it in its module's environment table
-- (`function UnlockMission(...)` in mrxmissionflow.lua -> "UnlockMission"). Local functions and
-- closures have none. Cached per function; weak keys so the cache never keeps a closure alive.
local getfenv, setmetatable = getfenv, setmetatable
local defined = setmetatable({}, { __mode = "k" })
local function defined_as(f)
  local n = defined[f]
  if n == nil then
    n = false
    local env = getfenv(f)
    if type(env) == "table" and env ~= _G then
      for k, v in pairs(env) do
        if v == f and type(k) == "string" then n = k break end
      end
    end
    defined[f] = n
  end
  return n or nil
end

-- Lua -> Lua calls that cross a module boundary. Only cheap getinfo options ("S", "l", "f") run on
-- every call; "n" makes Lua 5.1 re-scan the caller's bytecode (symbexec), so names are resolved only
-- for calls that are actually recorded.
sethook(function()
  local callee = getinfo(2, "Sf")
  if callee == nil or callee.what ~= "Lua" or callee.source == SELF then return end
  local via, level, caller = nil, 3, nil
  while true do
    local i = getinfo(level, "Sl")
    if i == nil then break end
    if i.what == "C" then
      via = via or {}
      via[#via + 1] = level
    elseif i.source ~= SELF then
      caller = i
      break
    end
    level = level + 1
  end
  local caller_src, caller_line = "", -1
  if caller then
    if caller.source == callee.source then return end
    caller_src, caller_line = caller.source, caller.currentline
  end
  if via then
    for j = 1, #via do via[j] = getinfo(via[j], "n").name or "?" end
  end
  local args, n = {}, 0
  while true do
    local name, val = getlocal(2, n + 1)
    if name == nil or sub(name, 1, 1) == "(" then break end
    n = n + 1
    args[n] = val
  end
  rec_lua(caller_src, caller_line, via, callee.source, callee.linedefined,
          defined_as(callee.func), getinfo(2, "n").name, n, unpack(args, 1, n))
end, "c")

return function(f)
  local i = getinfo(f, "S")
  return i.source, i.linedefined
end
"#;

/// Records written so far, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TraceCounts {
    pub engine: u64,
    pub lua: u64,
    pub stub: u64,
}

struct Sink {
    out: BufWriter<File>,
    seq: u64,
    counts: TraceCounts,
    /// Nesting depth of [`Tracer::suspended`]; records are dropped while non-zero.
    suspended: u32,
    /// `function(f) return source, linedefined end`, defined in [`TRACE_LUA`].
    fnsrc: Option<Function>,
}

/// The installed recorder. Records stream to the file as they happen; [`Tracer::finish`] flushes it.
pub struct Tracer {
    sink: Rc<RefCell<Sink>>,
}

impl Tracer {
    /// Install the recorder into `sh`, writing to `out` (created or truncated). Call it after the
    /// engine bindings are registered and before any game module is imported.
    pub fn install(sh: &ScriptHost, out: &Path) -> Result<Tracer, String> {
        let file = File::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
        let sink = Rc::new(RefCell::new(Sink {
            out: BufWriter::new(file),
            seq: 0,
            counts: TraceCounts::default(),
            suspended: 0,
            fnsrc: None,
        }));
        let lua = sh.lua();
        let g = lua.globals();
        let err = |e: LuaError| format!("installing the recorder: {e}");

        let s = sink.clone();
        let engine = lua
            .create_function(move |_, args: Variadic<Value>| record_engine(&s, args.0))
            .map_err(err)?;
        let s = sink.clone();
        let lua_rec = lua
            .create_function(move |_, args: Variadic<Value>| record_lua(&s, args.0))
            .map_err(err)?;
        let s = sink.clone();
        let stub = lua
            .create_function(move |_, (note, src, line): (String, String, i64)| record_stub(&s, &note, &src, line))
            .map_err(err)?;
        let namespaces = lua.create_table().map_err(err)?;
        for (i, ns) in NAMESPACES.iter().enumerate() {
            namespaces.set(i + 1, ns.global).map_err(err)?;
        }
        g.set("__autoplay_rec_engine", engine).map_err(err)?;
        g.set("__autoplay_rec_lua", lua_rec).map_err(err)?;
        g.set("__autoplay_rec_stub", stub).map_err(err)?;
        g.set("__autoplay_namespaces", namespaces).map_err(err)?;

        let fnsrc: Function = lua.load(TRACE_LUA).set_name(SELF_CHUNK).eval().map_err(err)?;
        sink.borrow_mut().fnsrc = Some(fnsrc);
        Ok(Tracer { sink })
    }

    /// Run `f` with recording off — for the harness's own reads of Lua state (e.g. the task-tree walk),
    /// which are not game calls.
    pub fn suspended<R>(&self, f: impl FnOnce() -> R) -> R {
        self.sink.borrow_mut().suspended += 1;
        let r = f();
        self.sink.borrow_mut().suspended -= 1;
        r
    }

    pub fn counts(&self) -> TraceCounts {
        self.sink.borrow().counts
    }

    /// Flush the stream. A write error here (or earlier, which raised in the Lua that triggered it)
    /// means the trace on disk is incomplete.
    pub fn finish(&self) -> Result<TraceCounts, String> {
        let mut s = self.sink.borrow_mut();
        s.out.flush().map_err(|e| format!("flushing the trace: {e}"))?;
        Ok(s.counts)
    }
}

fn rt_err(msg: String) -> LuaError {
    LuaError::RuntimeError(msg)
}

/// The recorder's function-describer, or `None` while suspended (the caller then drops the record).
fn active_fnsrc(sink: &Rc<RefCell<Sink>>) -> Option<Function> {
    let s = sink.borrow();
    if s.suspended > 0 {
        return None;
    }
    s.fnsrc.clone()
}

fn write_record(sink: &Rc<RefCell<Sink>>, kind: &str, mut rec: Map<String, Json>) -> LuaResult<()> {
    let mut s = sink.borrow_mut();
    s.seq += 1;
    let mut line = Map::new();
    line.insert("seq".into(), json!(s.seq));
    line.insert("kind".into(), json!(kind));
    line.append(&mut rec);
    match kind {
        "engine" => s.counts.engine += 1,
        "lua" => s.counts.lua += 1,
        _ => s.counts.stub += 1,
    }
    serde_json::to_writer(&mut s.out, &Json::Object(line)).map_err(|e| rt_err(format!("trace write: {e}")))?;
    s.out.write_all(b"\n").map_err(|e| rt_err(format!("trace write: {e}")))
}

/// `(ns, fn, caller_source, caller_line, args...)` from a binding wrapper.
fn record_engine(sink: &Rc<RefCell<Sink>>, mut v: Vec<Value>) -> LuaResult<()> {
    let Some(fnsrc) = active_fnsrc(sink) else { return Ok(()) };
    if v.len() < 4 {
        return Err(rt_err(format!("engine record with {} header values (want 4)", v.len())));
    }
    let args: Vec<Value> = v.split_off(4);
    let mut rec = Map::new();
    rec.insert("ns".into(), json!(lua_str(&v[0])));
    rec.insert("fn".into(), json!(lua_str(&v[1])));
    rec.insert("caller_src".into(), json!(src_line(&lua_str(&v[2]), lua_int(&v[3]))));
    rec.insert("argc".into(), json!(args.len()));
    rec.insert("args".into(), Json::Array(args.iter().map(|a| encode(a, 0, &fnsrc)).collect()));
    write_record(sink, "engine", rec)
}

/// `(caller_source, caller_line, via, callee_source, callee_line, defined_as, called_as, n, args...)`
/// from the hook.
fn record_lua(sink: &Rc<RefCell<Sink>>, mut v: Vec<Value>) -> LuaResult<()> {
    let Some(fnsrc) = active_fnsrc(sink) else { return Ok(()) };
    if v.len() < 8 {
        return Err(rt_err(format!("lua record with {} header values (want 8)", v.len())));
    }
    let args: Vec<Value> = v.split_off(8);
    let via: Vec<Json> = match &v[2] {
        Value::Table(t) => t
            .clone()
            .sequence_values::<String>()
            .map(|s| s.map(Json::String))
            .collect::<LuaResult<_>>()?,
        _ => Vec::new(),
    };
    let mut rec = Map::new();
    rec.insert("caller_src".into(), json!(src_line(&lua_str(&v[0]), lua_int(&v[1]))));
    rec.insert("via".into(), Json::Array(via));
    rec.insert("callee".into(), json!(src_line(&lua_str(&v[3]), lua_int(&v[4]))));
    rec.insert("fn".into(), opt_str(&v[5]));
    rec.insert("called_as".into(), opt_str(&v[6]));
    rec.insert("argc".into(), json!(lua_int(&v[7])));
    rec.insert("args".into(), Json::Array(args.iter().map(|a| encode(a, 0, &fnsrc)).collect()));
    write_record(sink, "lua", rec)
}

fn record_stub(sink: &Rc<RefCell<Sink>>, note: &str, src: &str, line: i64) -> LuaResult<()> {
    if active_fnsrc(sink).is_none() {
        return Ok(());
    }
    let mut rec = Map::new();
    rec.insert("path".into(), json!(note));
    rec.insert("caller_src".into(), json!(src_line(src, line)));
    write_record(sink, "stub", rec)
}

/// `@Module` + line → `@module:line`; an empty source (no Lua frame) stays empty.
fn src_line(src: &str, line: i64) -> String {
    if src.is_empty() {
        String::new()
    } else {
        format!("{}:{line}", src.to_ascii_lowercase())
    }
}

fn lua_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string_lossy(),
        _ => String::new(),
    }
}

/// A string as JSON, or `null` for a Lua `nil` (an unnamed function).
fn opt_str(v: &Value) -> Json {
    match v {
        Value::String(s) => json!(s.to_string_lossy()),
        _ => Json::Null,
    }
}

fn lua_int(v: &Value) -> i64 {
    match v {
        Value::Number(n) => *n as i64,
        _ => -1,
    }
}

/// A Lua number as JSON: integral values as integers, the rest via the float's shortest decimal form
/// (`lua_Number` is `f32` in this build, so widening the bits would print `0.10000000149011612`).
fn number(n: f32) -> Json {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1.0e15 {
        json!(n as i64)
    } else {
        format!("{n}")
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Json::Number)
            .unwrap_or_else(|| json!(format!("{n}")))
    }
}

fn encode(v: &Value, depth: u8, fnsrc: &Function) -> Json {
    match v {
        Value::Nil => json!({"t": "nil"}),
        Value::Boolean(b) => json!({"t": "bool", "v": b}),
        Value::Number(n) => json!({"t": "num", "v": number(*n)}),
        Value::String(s) => json!({"t": "str", "v": mask_addresses(&s.to_string_lossy())}),
        Value::LightUserData(p) => json!({"t": "tt2", "p": format!("{:#x}", p.0 as usize)}),
        Value::Function(f) => match fnsrc.call::<(Option<String>, Option<f32>)>(f.clone()) {
            Ok((Some(s), line)) => json!({"t": "tt6", "src": src_line(&s, line.map(|l| l as i64).unwrap_or(-1))}),
            Ok((None, _)) => json!({"t": "tt6"}),
            Err(e) => json!({"t": "tt6", "src_error": e.to_string()}),
        },
        Value::Table(t) => {
            // At the depth limit nothing about the contents is written, so nothing is read: walking a
            // nested table only to count it is what made recording cost minutes per boot.
            if depth >= TABLE_DEPTH {
                return json!({"t": "tt5"});
            }
            let entries: Vec<(Value, Value)> = match t.pairs::<Value, Value>().collect::<LuaResult<_>>() {
                Ok(e) => e,
                Err(e) => return json!({"t": "tt5", "error": e.to_string()}),
            };
            let n = entries.len();
            // Sort by key and keep the first TABLE_ENTRIES BEFORE encoding any value, so only the kept
            // values are descended into.
            let mut keyed: Vec<(String, &Value)> = entries.iter().map(|(k, val)| (key_string(k, fnsrc), val)).collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            keyed.truncate(TABLE_ENTRIES);
            let mut m = Map::new();
            for (k, val) in keyed {
                m.insert(k, encode(val, depth + 1, fnsrc));
            }
            json!({"t": "tt5", "n": n, "v": Json::Object(m)})
        }
        Value::Other(_, ty) => json!({"t": ty}),
    }
}

/// Replace the heap address in Lua's `tostring` of a reference value (`"function: 0xbc6d69620"`) with
/// `<addr>`. Scripts do build strings this way (`mrxfactionmanager.lua:370` prints
/// `"Disguise.Init " .. tostring(fn)`); the address changes on every run, retail included, so left in
/// place it would make two identical runs' traces differ.
fn mask_addresses(s: &str) -> String {
    const PREFIXES: [&str; 4] = ["table: 0x", "function: 0x", "userdata: 0x", "thread: 0x"];
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    'scan: while !rest.is_empty() {
        for p in PREFIXES {
            if let Some(after) = rest.strip_prefix(p) {
                let hex = after.bytes().take_while(u8::is_ascii_hexdigit).count();
                if hex > 0 {
                    out.push_str(&p[..p.len() - 2]);
                    out.push_str("<addr>");
                    rest = &after[hex..];
                    continue 'scan;
                }
            }
        }
        let ch = rest.chars().next().expect("rest is non-empty");
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

fn key_string(k: &Value, fnsrc: &Function) -> String {
    match k {
        Value::String(s) => s.to_string_lossy(),
        Value::Number(n) => number(*n).to_string(),
        other => encode(other, TABLE_DEPTH, fnsrc).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::mask_addresses;

    #[test]
    fn masks_reference_addresses_and_nothing_else() {
        assert_eq!(mask_addresses("Disguise.Init function: 0xbc6d69620"), "Disguise.Init function: <addr>");
        assert_eq!(mask_addresses("table: 0x1a3f2c80 / thread: 0xff"), "table: <addr> / thread: <addr>");
        // A guid printed as hex and a prefix with no hex after it are left alone.
        assert_eq!(mask_addresses("target 0x100007d2"), "target 0x100007d2");
        assert_eq!(mask_addresses("function: 0x"), "function: 0x");
        assert_eq!(mask_addresses("[VzaCon001.Objectives.001] ü"), "[VzaCon001.Objectives.001] ü");
    }
}
