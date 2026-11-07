//! The Lua debug protocol: one message set for every host the Workshop's Missions debugger attaches to.
//!
//! Three hosts run the game's Lua and serve the bridge port: the retail game (the `lua-bridge`
//! Shipment on m2-sdk.dll), our engine (`mercs2_game`) and `mercs2_autoplay`. Each speaks this
//! protocol on the bridge's WebSocket, beside the REPL's `{id, code}` requests and its `log`/`ws`/`ack`
//! frames, so the debugger records whichever one is attached. The behaviour each message serves is
//! the contract in `docs/modernization/workshop_pages/missions.md` (§Debug protocol).
//!
//! Every message is one JSON text frame whose `type` starts with `debug.`:
//!
//! ```text
//!   client -> host   debug.hello        {id, protocol}
//!   host -> client   debug.host         {id, protocol, kind, build, attached}
//!   client -> host   debug.bindings     {id}
//!   host -> client   debug.names        {id, bindings}
//!   client -> host   debug.lines        {id, lines}
//!   client -> host   debug.attach       {id, protocol, client, modules, breakpoints, engine}
//!   client -> host   debug.watch        {id, modules}
//!   client -> host   debug.breakpoints  {id, breakpoints, engine}
//!   client -> host   debug.continue     {id}
//!   client -> host   debug.step         {id, step}
//!   client -> host   debug.detach       {id}
//!   host -> client   debug.ack          {id}
//!   host -> client   debug.refused      {id, error}
//!   host -> client   debug.module       {seq, module, stamp}
//!   host -> client   debug.stack        {seq, frames}
//!   host -> client   debug.entry        {seq, at}
//!   host -> client   debug.line         {seq, at}
//!   host -> client   debug.return       {seq, at}
//!   host -> client   debug.engine       {seq, at, binding, args}
//!   host -> client   debug.engine_return {seq, call, values}
//!   host -> client   debug.stub         {seq, at, path}
//!   host -> client   debug.paused       {seq, at, reason}
//!   host -> client   debug.resumed      {seq}
//!   host -> client   debug.detached     {seq, error}
//! ```
//!
//! A transport routes each frame a client sends by [`Inbound::classify`]: a JSON object with a `type`
//! is the debugger's, anything else the REPL's. Every request carries an `id`, and the host answers it
//! with exactly one `debug.host` (for a hello), `debug.ack` or `debug.refused`. Everything the run does is an event with a `seq` that starts at 1
//! on attach and has no gaps: [`Sequence`] fails on the first missing one, because a recording with a
//! hole would light the wrong nodes.
//!
//! A position ([`At`]) is in the host's own line space ([`HostKind::line_space`]): the retail game
//! reports the original author's lines, our hosts the decompiled corpus's. The Workshop resolves either
//! through `lua_linemap::LineMap` (built against itself for a corpus host), so the function is named
//! by [`FunctionKey`] and the instruction by its index, never by a line alone.
//!
//! Line events are raised in the corpus's line space on every host. The retail VM's own line hook fires
//! where the original author's line changes, and 16,388 of the 106,468 corpus lines with code never
//! begin such a run (measured over the shipped `vz.wad`/`shell.wad` scripts against the corpus), so it
//! cannot stop on them. Instead the client stages each filtered function's corpus line per instruction
//! ([`LineTable`]) in `debug.lines` frames ahead of its `debug.attach`, split by [`lines_frames`] (every
//! module's tables together are over 1.7 MB, past [`MAX_FRAME_BYTES`]). Every host reports each
//! instruction a filtered function executes to [`session::Debugger::instruction`], and that raises a
//! line event by the VM's own rule applied to the corpus lines: on a function's first instruction, on a
//! jump backwards, and where the line changes. A function with no table reports entry and return only.
//!
//! An engine breakpoint names an engine binding (`Pg.Spawn`, from `debug.bindings`, the names the host
//! can tell apart): the run pauses when any Lua code calls it, from any module, before the binding runs,
//! with the call's `debug.engine` event first. Its position is the calling instruction. A module the
//! attach does not filter raises nothing else, so a pause there can only continue; a step is refused.
//! `debug.watch` replaces the module filter of a running or paused attach, taking the line tables staged
//! since, so a script joins a paused run without detaching (which would resume it).
//!
//! Text from the game (module names, binding names, argument strings) is CP1252. It is carried as the
//! Unicode text of those bytes ([`game_text`]), the rule the bridge's `log` frames already follow, and
//! [`game_bytes`] gives the bytes back.

pub mod host;
pub mod retail;
pub mod session;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::lua_linemap::{FunctionKey, LineMap};
use crate::luachunk::{Constant, Proto};

/// The protocol version both sides send. A peer with another version is refused, never half-served.
/// Version 2 added engine breakpoints, `debug.bindings` and `debug.watch`.
pub const PROTOCOL: u32 = 2;

/// The `type` prefix of every debug message.
pub const TYPE_PREFIX: &str = "debug.";

/// The largest debug frame, in bytes: the lua-bridge's WebSocket message limit, which every host keeps.
pub const MAX_FRAME_BYTES: usize = 1_048_575;

/// A message that is not a valid debug message, a protocol mismatch, or a gap in the events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Which program is running the Lua.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    /// The shipped game, through the `lua-bridge` Shipment on m2-sdk.dll.
    Retail,
    /// `mercs2_game`.
    Engine,
    /// `mercs2_autoplay`.
    Autoplay,
}

/// Whose line numbers a host's positions carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineSpace {
    /// The original author's, from the shipped chunks' line info.
    Original,
    /// The decompiled corpus text's, which our hosts compile and run.
    Corpus,
}

impl HostKind {
    /// The retail game runs the shipped chunks; our hosts run the corpus text.
    pub fn line_space(self) -> LineSpace {
        match self {
            HostKind::Retail => LineSpace::Original,
            HostKind::Engine | HostKind::Autoplay => LineSpace::Corpus,
        }
    }
}

/// Where an event happened, in the host's line space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct At {
    /// The module's name as its shipped chunk embeds it, its container (INFO) name: `gurcon002`, `vz`. On
    /// retail this is the running `Proto`'s `source` (every shipped chunk embeds exactly that); a host that
    /// names its chunks otherwise (`@GurCon002`) reports this form.
    pub module: String,
    pub function: FunctionKey,
    /// 0-based index of the instruction the event belongs to (0 on entry).
    pub pc: u32,
    /// The host's line for that instruction; `None` when the chunk carries no line info.
    pub line: Option<u32>,
    /// The frame's call depth: its index in the host VM's call-info stack, 0 being the base frame. A
    /// stack rebuilt from entries and returns alone drifts (a tail call reuses its frame, an error
    /// caught by `pcall` unwinds frames with no return), so the client cuts its stack to this.
    pub depth: u32,
}

/// A place to stop: an instruction of a function, in the host's line space.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breakpoint {
    pub module: String,
    pub function: FunctionKey,
    pub pc: u32,
}

/// The corpus line of every instruction of one function, in the host's [`FunctionKey`]: what a host
/// raises line events from. The client takes it from the line map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LineTable {
    pub module: String,
    pub function: FunctionKey,
    /// One line per instruction: exactly `function.code_len` of them.
    pub lines: Vec<u32>,
}

/// How far a step runs before pausing again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    /// To the next line event at any depth.
    Into,
    /// To the next line event at the paused depth or shallower.
    Over,
    /// To the next line event shallower than the paused depth.
    Out,
}

/// Why the host paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PauseReason {
    Breakpoint,
    Step,
    /// An engine breakpoint: a call of a binding it names, before the binding runs.
    Engine,
}

/// A Lua value as a host recorded it: the encoding `mercs2_autoplay`'s trace writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", deny_unknown_fields)]
pub enum Arg {
    #[serde(rename = "nil")]
    Nil,
    #[serde(rename = "bool")]
    Bool { v: bool },
    /// See [`number`].
    #[serde(rename = "num")]
    Num { v: Num },
    #[serde(rename = "str")]
    Str { v: String },
    /// A light userdata (an engine guid), as `0x…` hex.
    #[serde(rename = "tt2")]
    LightUserData { p: String },
    /// A table: its entry count `n` and the first entries `v` by sorted key, or the `error` reading it
    /// raised, or neither when the host did not descend into it.
    #[serde(rename = "tt5")]
    Table {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        n: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        v: Option<BTreeMap<String, Arg>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// A function: the `@module:line` it was defined at, or the `src_error` finding it raised, or
    /// neither for a C function.
    #[serde(rename = "tt6")]
    Function {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src_error: Option<String>,
    },
    #[serde(rename = "userdata")]
    Userdata,
    #[serde(rename = "thread")]
    Thread,
}

/// A value as Lua source would write it, for display: `nil`, `true`, `2.5`, `"text"`, a guid's `0x…`, and
/// for what source cannot write, `table`, `function @module:line`, `userdata`, `thread`.
impl std::fmt::Display for Arg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Arg::Nil => f.write_str("nil"),
            Arg::Bool { v } => write!(f, "{v}"),
            Arg::Num { v: Num::Number(n) } => write!(f, "{n}"),
            Arg::Num { v: Num::Text(t) } => f.write_str(t),
            Arg::Str { v } => write!(f, "{v:?}"),
            Arg::LightUserData { p } => f.write_str(p),
            Arg::Table { n: Some(n), .. } => write!(f, "table ({n} entries)"),
            Arg::Table { .. } => f.write_str("table"),
            Arg::Function { src: Some(src), .. } => write!(f, "function {src}"),
            Arg::Function { .. } => f.write_str("function"),
            Arg::Userdata => f.write_str("userdata"),
            Arg::Thread => f.write_str("thread"),
        }
    }
}

/// A Lua number as recorded: a JSON number, or the float's text when JSON has none (`NaN`, `inf`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Num {
    Number(serde_json::Number),
    Text(String),
}

/// A `lua_Number` (an `f32` in this build) as recorded: integral values as integers, the rest as the
/// float's shortest decimal (widening the bits would print `0.10000000149011612`).
pub fn number(n: f32) -> Num {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1.0e15 {
        Num::Number((n as i64).into())
    } else {
        format!("{n}")
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Num::Number)
            .unwrap_or_else(|| Num::Text(format!("{n}")))
    }
}

/// Replace the heap address in Lua's `tostring` of a reference value (`"function: 0xbc6d69620"`) with
/// `<addr>`. Scripts do build strings this way (`mrxfactionmanager.lua:370` prints
/// `"Disguise.Init " .. tostring(fn)`); the address changes on every run, retail included, so left in
/// place it would make two identical runs' records differ.
pub fn mask_addresses(s: &str) -> String {
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

/// `@Module` + line → `@module:line`, the module lowercased (a chunk is named after the spelling its
/// importer used, which is not stable across importers); an empty source (no Lua frame) stays empty.
pub fn src_line(src: &str, line: i64) -> String {
    if src.is_empty() {
        String::new()
    } else {
        format!("{}:{line}", src.to_ascii_lowercase())
    }
}

/// A client's request to a host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Request {
    /// Ask what the host is, without attaching. Answered by [`Event::Host`].
    #[serde(rename = "debug.hello")]
    Hello { id: String, protocol: u32 },
    /// Stage line tables for this client's next `debug.attach`, which consumes every table the client
    /// staged since its last attach, whether it is accepted or refused. A client sends as many as its
    /// tables need, each frame at most [`MAX_FRAME_BYTES`] ([`lines_frames`] packs them).
    #[serde(rename = "debug.lines")]
    Lines { id: String, lines: Vec<LineTable> },
    /// The engine bindings the host can break on. Answered by [`Event::Names`].
    #[serde(rename = "debug.bindings")]
    Bindings { id: String },
    /// Become the host's one debugger: install the hook for `modules` (chunk names, compared ASCII
    /// case-insensitively) with `breakpoints` and the engine breakpoints `engine` (binding names, as
    /// `debug.names` gives them), raising line events from the tables staged by `debug.lines`. It names at
    /// least one module or one engine breakpoint. `client` names the requester in a refusal another client
    /// gets while this one holds the host.
    #[serde(rename = "debug.attach")]
    Attach { id: String, protocol: u32, client: String, modules: Vec<String>, breakpoints: Vec<Breakpoint>, engine: Vec<String> },
    /// Replace the attach's module filter, running or paused: the line tables of modules it keeps stay,
    /// and the tables staged since are added.
    #[serde(rename = "debug.watch")]
    Watch { id: String, modules: Vec<String> },
    /// Replace the breakpoint set and the engine breakpoints.
    #[serde(rename = "debug.breakpoints")]
    Breakpoints { id: String, breakpoints: Vec<Breakpoint>, engine: Vec<String> },
    /// Resume a paused host until the next breakpoint.
    #[serde(rename = "debug.continue")]
    Continue { id: String },
    /// Resume a paused host for one step.
    #[serde(rename = "debug.step")]
    Step { id: String, step: Step },
    /// Remove the hook and give the host up; a paused host resumes.
    #[serde(rename = "debug.detach")]
    Detach { id: String },
}

/// A host's answer to a request, or something the run did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Event {
    /// The answer to a hello. `build` names the exact program (the retail exe's build id, or our
    /// crate's version); `attached` is the client holding the debugger, if one does.
    #[serde(rename = "debug.host")]
    Host { id: String, protocol: u32, kind: HostKind, build: String, attached: Option<String> },
    /// The answer to `debug.bindings`: every engine binding name the host can break on, sorted.
    #[serde(rename = "debug.names")]
    Names { id: String, bindings: Vec<String> },
    /// The request was carried out.
    #[serde(rename = "debug.ack")]
    Ack { id: String },
    /// The request was not carried out, and why.
    #[serde(rename = "debug.refused")]
    Refused { id: String, error: String },
    /// A filtered module the host has loaded: sent for each one already loaded at attach, and for each
    /// one loaded later. `stamp` is [`stamp`] of what the host is running.
    #[serde(rename = "debug.module")]
    Module { seq: u64, module: String, stamp: String },
    /// The Lua frames on the stack at attach, base first.
    #[serde(rename = "debug.stack")]
    Stack { seq: u64, frames: Vec<At> },
    /// A function of a filtered module was entered.
    #[serde(rename = "debug.entry")]
    Entry { seq: u64, at: At },
    /// A new line of a filtered module is about to run.
    #[serde(rename = "debug.line")]
    Line { seq: u64, at: At },
    /// A function of a filtered module returned, from the instruction at `at`.
    #[serde(rename = "debug.return")]
    Return { seq: u64, at: At },
    /// Code at `at` called the engine binding `binding` (`Pg.Spawn`).
    #[serde(rename = "debug.engine")]
    Engine { seq: u64, at: At, binding: String, args: Vec<Arg> },
    /// The engine call recorded as event `call` returned `values`.
    #[serde(rename = "debug.engine_return")]
    EngineReturn { seq: u64, call: u64, values: Vec<Arg> },
    /// Code at `at` touched an engine name our host has no binding for (`global:<Name>`,
    /// `call:<path>`). Only our hosts send it.
    #[serde(rename = "debug.stub")]
    Stub { seq: u64, at: At, path: String },
    /// The game thread is paused at `at`.
    #[serde(rename = "debug.paused")]
    Paused { seq: u64, at: At, reason: PauseReason },
    /// The game thread runs again.
    #[serde(rename = "debug.resumed")]
    Resumed { seq: u64 },
    /// The hook is gone: the client detached (`error` is `None`), or the host gave the debugger up and
    /// says why (the client's outbox overflowed, the state closed).
    #[serde(rename = "debug.detached")]
    Detached { seq: u64, error: Option<String> },
}

/// The message's `type`, when the body is a JSON object that has one.
fn message_type(body: &[u8]) -> Result<(serde_json::Value, Option<String>), Error> {
    let v: serde_json::Value = serde_json::from_slice(body).map_err(|e| Error(format!("not JSON: {e}")))?;
    let obj = v.as_object().ok_or_else(|| Error("not a JSON object".into()))?;
    let ty = match obj.get("type") {
        None => None,
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(other) => return Err(Error(format!("`type` is not a string: {other}"))),
    };
    Ok((v, ty))
}

/// The request in `v`, an object whose `type` is `ty`.
fn debug_request(v: serde_json::Value, ty: &str) -> Result<Request, Error> {
    if !ty.starts_with(TYPE_PREFIX) {
        return Err(Error(format!("`{ty}` is not a debug request; a REPL request has no `type`")));
    }
    serde_json::from_value(v).map_err(|e| Error(format!("`{ty}`: {e}")))
}

impl Request {
    /// A frame a client sent the bridge: `Ok(None)` when it has no `type` (a REPL `{id, code}`
    /// request), the request when it is a debug one, and an error for any other `type` or a malformed
    /// debug request.
    pub fn parse(body: &[u8]) -> Result<Option<Request>, Error> {
        let (v, ty) = message_type(body)?;
        let Some(ty) = ty else { return Ok(None) };
        debug_request(v, &ty).map(Some)
    }

    pub fn id(&self) -> &str {
        match self {
            Request::Hello { id, .. }
            | Request::Bindings { id }
            | Request::Lines { id, .. }
            | Request::Attach { id, .. }
            | Request::Watch { id, .. }
            | Request::Breakpoints { id, .. }
            | Request::Continue { id }
            | Request::Step { id, .. }
            | Request::Detach { id } => id,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a debug request always serializes")
    }
}

impl Event {
    /// A frame the bridge sent a client: `Ok(None)` for a frame that is not a debug event (the REPL's
    /// `log`, `ws` and `ack` frames), the event when it is one, and an error for a malformed one.
    pub fn parse(body: &[u8]) -> Result<Option<Event>, Error> {
        let (v, ty) = message_type(body)?;
        let ty = ty.ok_or_else(|| Error("a bridge frame without `type`".into()))?;
        if !ty.starts_with(TYPE_PREFIX) {
            return Ok(None);
        }
        serde_json::from_value(v).map(Some).map_err(|e| Error(format!("`{ty}`: {e}")))
    }

    /// The event's place in the run, or `None` for an answer to a request.
    pub fn seq(&self) -> Option<u64> {
        match self {
            Event::Host { .. } | Event::Names { .. } | Event::Ack { .. } | Event::Refused { .. } => None,
            Event::Module { seq, .. }
            | Event::Stack { seq, .. }
            | Event::Entry { seq, .. }
            | Event::Line { seq, .. }
            | Event::Return { seq, .. }
            | Event::Engine { seq, .. }
            | Event::EngineReturn { seq, .. }
            | Event::Stub { seq, .. }
            | Event::Paused { seq, .. }
            | Event::Resumed { seq }
            | Event::Detached { seq, .. } => Some(*seq),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a debug event always serializes")
    }
}

/// The line tables a client stages for `module` from its line map: one per function a host can name
/// unambiguously, keyed as the host reports it. A function that does not map has none, and neither do
/// functions that share a key while their corpus lines differ (`LineMap::function` refuses them); both
/// report entry and return only.
pub fn line_tables(module: &str, map: &LineMap) -> Vec<LineTable> {
    let mut seen = std::collections::HashSet::new();
    map.functions
        .iter()
        .filter(|f| seen.insert(f.key))
        .filter_map(|f| map.function(f.key).ok())
        .map(|f| LineTable { module: module.to_string(), function: f.key, lines: f.lines.clone() })
        .collect()
}

/// The breakpoints that stop on corpus line `line` of `module`: every instruction on it. The host
/// matches a breakpoint only where a line event starts, so the run pauses where the VM's own line hook
/// would, once per run of the line's instructions. An error when no instruction of a function with a
/// line table ([`line_tables`]) is on that line.
pub fn line_breakpoints(module: &str, map: &LineMap, line: u32) -> Result<Vec<Breakpoint>, Error> {
    let mut out = Vec::new();
    let mut unnamed = Vec::new();
    for (function, pc) in map.positions(line) {
        match map.function(function) {
            Ok(_) => out.push(Breakpoint { module: module.to_string(), function, pc }),
            Err(e) => unnamed.push(e.to_string()),
        }
    }
    if out.is_empty() {
        return Err(Error(if unnamed.is_empty() {
            format!("{module}:{line} has no instruction of a function that maps onto the corpus lines")
        } else {
            format!("{module}:{line} is only in functions a host cannot name: {}", unnamed.join("; "))
        }));
    }
    Ok(out)
}

/// `tables` as `debug.lines` requests, in order, each at most `max` bytes as JSON (the protocol's limit is
/// [`MAX_FRAME_BYTES`]); `id(n)` names the n-th. An error names a table too large for a frame of its own.
pub fn lines_frames(tables: Vec<LineTable>, max: usize, mut id: impl FnMut(usize) -> String) -> Result<Vec<Request>, Error> {
    // (id, tables, the frame's JSON length): a table adds its own JSON, and a comma after the first.
    let mut frames: Vec<(String, Vec<LineTable>, usize)> = Vec::new();
    for t in tables {
        let len = serde_json::to_string(&t).expect("a line table always serializes").len();
        match frames.last_mut() {
            Some((_, lines, size)) if *size + 1 + len <= max => {
                *size += 1 + len;
                lines.push(t);
                continue;
            }
            _ => {}
        }
        let frame_id = id(frames.len());
        let size = Request::Lines { id: frame_id.clone(), lines: Vec::new() }.to_json().len() + len;
        if size > max {
            return Err(Error(format!(
                "the line table of {} {:?} alone makes a {size}-byte frame, over the {max}-byte limit",
                t.module, t.function
            )));
        }
        frames.push((frame_id, vec![t], size));
    }
    Ok(frames.into_iter().map(|(id, lines, _)| Request::Lines { id, lines }).collect())
}

/// What one frame a client sent the bridge port is: the rule every host's transport routes by.
#[derive(Clone, Debug, PartialEq)]
pub enum Inbound {
    /// The REPL's: anything that is not a JSON object with a `type`. The REPL keeps byte semantics for
    /// `code` (raw CP1252 bytes, which are not JSON), so whether such a frame is a valid `{id, code}`
    /// request is the REPL's to decide.
    Repl,
    Debug(Request),
    /// A JSON object with a `type` that is not a valid debug request.
    Malformed { id: Option<String>, error: Error },
}

impl Inbound {
    pub fn classify(body: &[u8]) -> Inbound {
        let v = match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(v @ serde_json::Value::Object(_)) => v,
            _ => return Inbound::Repl,
        };
        let Some(ty) = v.get("type") else { return Inbound::Repl };
        let id = v.get("id").and_then(serde_json::Value::as_str).map(str::to_owned);
        let result = match ty {
            serde_json::Value::String(ty) => {
                let ty = ty.clone();
                debug_request(v, &ty)
            }
            other => Err(Error(format!("`type` is not a string: {other}"))),
        };
        match result {
            Ok(r) => Inbound::Debug(r),
            Err(error) => Inbound::Malformed { id, error },
        }
    }

    /// The answer to a malformed frame: `debug.refused` with its `id`, or `None` when it has no string
    /// `id` to answer (it can only be logged).
    pub fn refusal(&self) -> Option<Event> {
        match self {
            Inbound::Malformed { id: Some(id), error } => Some(Event::Refused { id: id.clone(), error: error.to_string() }),
            _ => None,
        }
    }
}

/// Refuse a peer that speaks another protocol version.
pub fn require_protocol(theirs: u32) -> Result<(), Error> {
    if theirs == PROTOCOL {
        Ok(())
    } else {
        Err(Error(format!("the peer speaks debug protocol {theirs}, this side speaks {PROTOCOL}; update the older side")))
    }
}

/// The gap check on a connection's events: each attach numbers its events from 1, and `debug.detached` is an
/// attach's last, so the count starts again after one.
#[derive(Clone, Debug)]
pub struct Sequence {
    next: u64,
}

impl Default for Sequence {
    fn default() -> Self {
        Sequence { next: 1 }
    }
}

impl Sequence {
    /// Accept `ev` if it is the next event (answers to requests carry no `seq` and always pass).
    pub fn check(&mut self, ev: &Event) -> Result<(), Error> {
        let Some(seq) = ev.seq() else { return Ok(()) };
        if seq != self.next {
            return Err(Error(format!(
                "event seq {seq} arrived where {} was due: the recording has a gap and cannot be trusted",
                self.next
            )));
        }
        self.next = if matches!(ev, Event::Detached { .. }) { 1 } else { self.next + 1 };
        Ok(())
    }
}

/// The identity of a module's program: `sha256:` and the hex digest of every function in pre-order
/// (main function first, then each child's tree in order), each as its upvalue count, parameter count,
/// vararg flag and stack size, its code, its constants and its child count. That is exactly what
/// `lua_linemap` compares, so two chunks that differ only in line info, local and upvalue names or
/// source name have one stamp. A host computes it from the `Proto` tree it is running.
pub fn stamp(main: &Proto) -> String {
    fn feed(p: &Proto, h: &mut Sha256) {
        h.update([p.nups, p.num_params, p.is_vararg, p.max_stack]);
        h.update((p.code.len() as u32).to_le_bytes());
        for w in &p.code {
            h.update(w.to_le_bytes());
        }
        h.update((p.constants.len() as u32).to_le_bytes());
        for k in &p.constants {
            match k {
                Constant::Nil => h.update([0u8]),
                Constant::Boolean(b) => h.update([1u8, *b as u8]),
                Constant::Number(bits) => {
                    h.update([3u8]);
                    h.update(bits.to_le_bytes());
                }
                Constant::String(s) => {
                    h.update([4u8]);
                    h.update((s.len() as u32).to_le_bytes());
                    h.update(s);
                }
            }
        }
        h.update((p.protos.len() as u32).to_le_bytes());
        for c in &p.protos {
            feed(c, h);
        }
    }
    let mut h = Sha256::new();
    feed(main, &mut h);
    let digest = h.finalize();
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// CP1252's 0x80..=0x9F as Unicode; the five bytes CP1252 leaves undefined keep their own code point.
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D,
    0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
    0x0153, 0x009D, 0x017E, 0x0178,
];

/// Game text (CP1252 bytes) as the Unicode text it shows.
pub fn game_text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => char::from_u32(u32::from(CP1252_HIGH[usize::from(b - 0x80)])).expect("table holds BMP scalars"),
            _ => char::from(b),
        })
        .collect()
}

/// The CP1252 bytes [`game_text`] made `text` from; an error names a character CP1252 has no byte for.
pub fn game_bytes(text: &str) -> Result<Vec<u8>, Error> {
    text.chars()
        .map(|c| {
            let cp = u32::from(c);
            if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
                return Ok(cp as u8);
            }
            CP1252_HIGH
                .iter()
                .position(|&h| u32::from(h) == cp)
                .map(|i| 0x80 + i as u8)
                .ok_or_else(|| Error(format!("{c:?} (U+{cp:04X}) has no CP1252 byte")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(a: u32, b: u32, n: u32) -> FunctionKey {
        FunctionKey { line_defined: a, last_line_defined: b, code_len: n }
    }

    fn at(pc: u32, depth: u32) -> At {
        At { module: "gurcon002".into(), function: key(10, 40, 52), pc, line: Some(12), depth }
    }

    fn proto(code: Vec<u32>, constants: Vec<Constant>, protos: Vec<Proto>) -> Proto {
        Proto {
            source: Some(b"@a".to_vec()),
            line_defined: 0,
            last_line_defined: 0,
            nups: 0,
            num_params: 0,
            is_vararg: 2,
            max_stack: 2,
            line_info: vec![1; code.len()],
            code,
            constants,
            protos,
            loc_vars: Vec::new(),
            upvalues: Vec::new(),
        }
    }

    #[test]
    fn every_request_round_trips() {
        let bp = Breakpoint { module: "gurcon002".into(), function: key(10, 40, 52), pc: 7 };
        let all = [
            Request::Hello { id: "1".into(), protocol: PROTOCOL },
            Request::Lines {
                id: "1l".into(),
                lines: vec![LineTable { module: "gurcon002".into(), function: key(10, 40, 3), lines: vec![11, 11, 12] }],
            },
            Request::Attach {
                id: "2".into(),
                protocol: PROTOCOL,
                client: "workshop".into(),
                modules: vec!["gurcon002".into()],
                breakpoints: vec![bp.clone()],
                engine: vec!["Pg.Spawn".into()],
            },
            Request::Bindings { id: "2b".into() },
            Request::Watch { id: "2w".into(), modules: vec!["gurcon002".into(), "mrxutil".into()] },
            Request::Breakpoints { id: "3".into(), breakpoints: vec![bp], engine: vec![] },
            Request::Continue { id: "4".into() },
            Request::Step { id: "5".into(), step: Step::Over },
            Request::Detach { id: "6".into() },
        ];
        for r in all {
            assert_eq!(Request::parse(r.to_json().as_bytes()), Ok(Some(r.clone())), "{}", r.to_json());
        }
    }

    #[test]
    fn every_event_round_trips() {
        let all = [
            Event::Host { id: "1".into(), protocol: PROTOCOL, kind: HostKind::Retail, build: "pc-v10".into(), attached: None },
            Event::Names { id: "1b".into(), bindings: vec!["Object.GetPosition".into(), "Pg.Spawn".into()] },
            Event::Ack { id: "2".into() },
            Event::Refused { id: "3".into(), error: "held by workshop".into() },
            Event::Module { seq: 1, module: "gurcon002".into(), stamp: "sha256:00".into() },
            Event::Stack { seq: 2, frames: vec![at(3, 1), at(9, 2)] },
            Event::Entry { seq: 3, at: at(0, 3) },
            Event::Line { seq: 4, at: at(1, 3) },
            Event::Engine {
                seq: 5,
                at: at(4, 3),
                binding: "Pg.Spawn".into(),
                args: vec![Arg::Str { v: "pmc_hum_mattias".into() }, Arg::Num { v: number(800000.0) }],
            },
            Event::EngineReturn { seq: 6, call: 5, values: vec![Arg::LightUserData { p: "0x100007d2".into() }] },
            Event::Stub { seq: 7, at: at(5, 3), path: "call:Hud.Foo".into() },
            Event::Paused { seq: 8, at: at(6, 3), reason: PauseReason::Breakpoint },
            Event::Resumed { seq: 9 },
            Event::Return { seq: 10, at: at(51, 3) },
            Event::Detached { seq: 11, error: None },
        ];
        let mut seq = Sequence::default();
        for e in all {
            assert_eq!(Event::parse(e.to_json().as_bytes()), Ok(Some(e.clone())), "{}", e.to_json());
            seq.check(&e).unwrap();
        }
    }

    #[test]
    fn the_wire_text_is_pinned() {
        let e = Event::Line { seq: 4, at: at(1, 3) };
        assert_eq!(
            e.to_json(),
            r#"{"type":"debug.line","seq":4,"at":{"module":"gurcon002","function":{"line_defined":10,"last_line_defined":40,"code_len":52},"pc":1,"line":12,"depth":3}}"#
        );
        let r = Request::Step { id: "q1".into(), step: Step::Into };
        assert_eq!(r.to_json(), r#"{"type":"debug.step","id":"q1","step":"into"}"#);
    }

    #[test]
    fn repl_frames_are_not_debug_messages() {
        assert_eq!(Request::parse(br#"{"id":"q1","code":"return 1"}"#), Ok(None));
        assert_eq!(Event::parse(br#"{"type":"log","line":"hi"}"#), Ok(None));
        assert_eq!(Event::parse(br#"{"type":"ack","id":"q1","status":"queued"}"#), Ok(None));
    }

    #[test]
    fn malformed_debug_messages_are_errors() {
        // A typed request that is not a debug one, an unknown debug type, a missing field, an unknown
        // field, and a frame that is not an object.
        assert!(Request::parse(br#"{"type":"log","id":"q1"}"#).is_err());
        assert!(Request::parse(br#"{"type":"debug.pause","id":"q1"}"#).is_err());
        assert!(Request::parse(br#"{"type":"debug.step","id":"q1"}"#).is_err());
        assert!(Request::parse(br#"{"type":"debug.continue","id":"q1","extra":1}"#).is_err());
        assert!(Request::parse(b"[1]").is_err());
        assert!(Event::parse(br#"{"line":"no type"}"#).is_err());
        assert!(Event::parse(br#"{"type":"debug.line","seq":1}"#).is_err());
        let bad_at = br#"{"type":"debug.line","seq":1,"at":{"module":"m","function":{"line_defined":1,"last_line_defined":2,"code_len":3},"pc":0,"line":null,"depth":0,"x":1}}"#;
        assert!(Event::parse(bad_at).is_err());
    }

    fn lp(line_defined: u32, last: u32, code: Vec<u32>, lines: Vec<u32>, protos: Vec<Proto>) -> Proto {
        Proto { line_defined, last_line_defined: last, line_info: lines, ..proto(code, vec![Constant::String(b"Ai".to_vec())], protos) }
    }

    /// Retail and ours with the same code: a mapped function, two that share a key and differ in their
    /// corpus lines, and one whose code differs (unmapped).
    fn tree() -> LineMap {
        let side = |a: u32, twin: [u32; 2], other_code: u32| {
            lp(0, 0, vec![0x24, 0x24, 0x24, 0x24, 0x1e], vec![1; 5], vec![
                lp(500, 503, vec![0x05, 0x1c, 0x1e], vec![a, a, a + 1], vec![]),
                lp(530, 530, vec![0x1e], vec![twin[0]], vec![]),
                lp(530, 530, vec![0x1e], vec![twin[1]], vec![]),
                lp(600, 601, vec![other_code, 0x1e], vec![80, 80], vec![]),
            ])
        };
        LineMap::build(&side(501, [530, 530], 0x05), &side(63, [70, 71], 0x06)).unwrap()
    }

    #[test]
    fn line_tables_hold_only_functions_a_host_can_name() {
        let m = tree();
        let keys: Vec<FunctionKey> = line_tables("gurcon002", &m).iter().map(|t| t.function).collect();
        assert_eq!(keys, vec![key(0, 0, 5), key(500, 503, 3)], "the twins and the unmapped function have none");
        let t = &line_tables("gurcon002", &m)[1];
        assert_eq!((t.module.as_str(), t.lines.clone()), ("gurcon002", vec![63, 63, 64]));
    }

    #[test]
    fn a_line_breakpoint_is_every_instruction_on_the_line() {
        let m = tree();
        let bps = line_breakpoints("gurcon002", &m, 63).unwrap();
        let at: Vec<(FunctionKey, u32)> = bps.iter().map(|b| (b.function, b.pc)).collect();
        assert_eq!(at, vec![(key(500, 503, 3), 0), (key(500, 503, 3), 1)]);
        assert!(line_breakpoints("gurcon002", &m, 99).unwrap_err().to_string().contains("no instruction"));
        let e = line_breakpoints("gurcon002", &m, 70).unwrap_err().to_string();
        assert!(e.contains("cannot name") && e.contains("told apart"), "{e}");
        // A line breakpoint pauses once per run of its instructions: pc 1 continues the line pc 0 began.
        let mut d = session::Debugger::new(HostKind::Retail, "pc-v10");
        d.request(1, Request::Lines { id: "l".into(), lines: line_tables("gurcon002", &m) });
        let attach =
            Request::Attach { id: "a".into(), protocol: PROTOCOL, client: "w".into(), modules: vec!["gurcon002".into()], breakpoints: bps, engine: vec![] };
        d.request(1, attach);
        let at = |pc| At { module: "gurcon002".into(), function: key(500, 503, 3), pc, line: None, depth: 1 };
        let pauses: Vec<bool> = (0..3).map(|pc| d.instruction(at(pc)).unwrap().1.is_some()).collect();
        assert_eq!(pauses, vec![true, false, false]);
    }

    #[test]
    fn line_tables_pack_into_frames_under_the_limit_in_order() {
        // Tables and ids of one length each, so the frame sizes are exact.
        let table = |n: u32| LineTable { module: "gurcon002".into(), function: key(10 + n, 50 + n, 50), lines: vec![1000 + n; 50] };
        let tables: Vec<LineTable> = (0..40).map(table).collect();
        let one = serde_json::to_string(&tables[0]).unwrap().len();
        assert!(tables.iter().all(|t| serde_json::to_string(t).unwrap().len() == one));
        let id = |n: usize| format!("l{n:02}");
        // Room for exactly three tables a frame.
        let envelope = Request::Lines { id: id(0), lines: Vec::new() }.to_json().len();
        let max = envelope + 3 * one + 2;
        let frames = lines_frames(tables.clone(), max, id).unwrap();
        assert_eq!(frames.len(), 14, "40 tables, three a frame");
        let mut back = Vec::new();
        for (n, f) in frames.iter().enumerate() {
            assert!(f.to_json().len() <= max, "frame {n} is {} bytes", f.to_json().len());
            let Request::Lines { id: got, lines } = f else { panic!("{f:?}") };
            assert_eq!(got, &id(n));
            if n < 13 {
                assert_eq!((lines.len(), f.to_json().len()), (3, max), "full frames fill the limit exactly");
            }
            back.extend(lines.iter().cloned());
        }
        assert_eq!(back, tables);
        assert_eq!(lines_frames(Vec::new(), max, id).unwrap(), Vec::new());
        let e = lines_frames(vec![table(1)], envelope + one - 1, id).unwrap_err();
        assert!(e.to_string().contains("alone makes a"), "{e}");
    }

    #[test]
    fn inbound_frames_route_by_type() {
        let continue_ = br#"{"type":"debug.continue","id":"c1"}"#;
        assert_eq!(Inbound::classify(continue_), Inbound::Debug(Request::Continue { id: "c1".into() }));
        // REPL requests, including one whose `code` carries a raw CP1252 byte (not UTF-8, so not JSON).
        assert_eq!(Inbound::classify(br#"{"id":"q1","code":"return 1"}"#), Inbound::Repl);
        assert_eq!(Inbound::classify(b"{\"id\":\"q2\",\"code\":\"caf\xe9\"}"), Inbound::Repl);
        assert_eq!(Inbound::classify(b"not json"), Inbound::Repl);
        assert_eq!(Inbound::classify(b"[1]"), Inbound::Repl);
        // Typed frames that are not valid debug requests: refused with their id when they have one.
        let bad = Inbound::classify(br#"{"type":"debug.step","id":"s1"}"#);
        assert!(matches!(&bad, Inbound::Malformed { id: Some(id), .. } if id == "s1"), "{bad:?}");
        assert!(matches!(bad.refusal(), Some(Event::Refused { id, error }) if id == "s1" && error.contains("debug.step")));
        let other = Inbound::classify(br#"{"type":"log","id":"l1"}"#);
        assert!(matches!(other.refusal(), Some(Event::Refused { error, .. }) if error.contains("not a debug request")));
        let untyped = Inbound::classify(br#"{"type":7,"id":"n1"}"#);
        assert!(matches!(untyped.refusal(), Some(Event::Refused { error, .. }) if error.contains("not a string")));
        let no_id = Inbound::classify(br#"{"type":"debug.continue"}"#);
        assert!(matches!(no_id, Inbound::Malformed { id: None, .. }));
        assert_eq!(no_id.refusal(), None);
        assert_eq!(Inbound::Repl.refusal(), None);
    }

    #[test]
    fn another_protocol_version_is_refused() {
        assert!(require_protocol(PROTOCOL).is_ok());
        let e = require_protocol(PROTOCOL + 1).unwrap_err();
        assert!(e.to_string().contains(&format!("protocol {}", PROTOCOL + 1)), "{e}");
    }

    #[test]
    fn a_gap_in_the_events_is_an_error() {
        let mut s = Sequence::default();
        s.check(&Event::Ack { id: "a".into() }).unwrap();
        s.check(&Event::Resumed { seq: 1 }).unwrap();
        let e = s.check(&Event::Resumed { seq: 3 }).unwrap_err();
        assert!(e.to_string().contains("seq 3 arrived where 2 was due"), "{e}");
        assert!(Sequence::default().check(&Event::Resumed { seq: 2 }).is_err());
    }

    #[test]
    fn the_count_starts_again_after_an_attachs_last_event() {
        let mut s = Sequence::default();
        for seq in 1..=3 {
            s.check(&Event::Resumed { seq }).unwrap();
        }
        s.check(&Event::Detached { seq: 4, error: None }).unwrap();
        s.check(&Event::Ack { id: "attach".into() }).unwrap();
        s.check(&Event::Resumed { seq: 1 }).unwrap();
        let e = s.check(&Event::Resumed { seq: 5 }).unwrap_err();
        assert!(e.to_string().contains("seq 5 arrived where 2 was due"), "{e}");
    }

    #[test]
    fn arguments_keep_autoplays_encoding() {
        // Lines as `mercs2_autoplay`'s trace writes them (trace.rs `encode`).
        for json in [
            r#"{"t":"nil"}"#,
            r#"{"t":"bool","v":true}"#,
            r#"{"t":"num","v":800000}"#,
            r#"{"t":"num","v":0.1}"#,
            r#"{"t":"num","v":"NaN"}"#,
            r#"{"t":"str","v":"CashAdded"}"#,
            r#"{"t":"tt2","p":"0x100007d2"}"#,
            r#"{"t":"tt5"}"#,
            r#"{"t":"tt5","n":2,"v":{"1":{"t":"str","v":"a"},"x":{"t":"tt5"}}}"#,
            r#"{"t":"tt5","error":"boom"}"#,
            r#"{"t":"tt6"}"#,
            r#"{"t":"tt6","src":"@mrxmissionflow:132"}"#,
            r#"{"t":"tt6","src_error":"boom"}"#,
            r#"{"t":"userdata"}"#,
            r#"{"t":"thread"}"#,
        ] {
            let a: Arg = serde_json::from_str(json).unwrap_or_else(|e| panic!("{json}: {e}"));
            assert_eq!(serde_json::to_string(&a).unwrap(), json);
        }
        assert!(serde_json::from_str::<Arg>(r#"{"t":"tt9"}"#).is_err());
    }

    #[test]
    fn arguments_display_as_lua_writes_them() {
        let shown: Vec<String> = [
            r#"{"t":"nil"}"#,
            r#"{"t":"bool","v":true}"#,
            r#"{"t":"num","v":2.5}"#,
            r#"{"t":"num","v":"NaN"}"#,
            r#"{"t":"str","v":"Panhard \"Assault\""}"#,
            r#"{"t":"tt2","p":"0x40000915"}"#,
            r#"{"t":"tt5","n":2}"#,
            r#"{"t":"tt5"}"#,
            r#"{"t":"tt6","src":"@mrxutil:447"}"#,
            r#"{"t":"tt6"}"#,
        ]
        .iter()
        .map(|j| serde_json::from_str::<Arg>(j).unwrap().to_string())
        .collect();
        assert_eq!(
            shown,
            [
                "nil",
                "true",
                "2.5",
                "NaN",
                r#""Panhard \"Assault\"""#,
                "0x40000915",
                "table (2 entries)",
                "table",
                "function @mrxutil:447",
                "function"
            ]
        );
    }

    #[test]
    fn numbers_are_written_as_autoplay_writes_them() {
        let text = |n: f32| serde_json::to_string(&number(n)).unwrap();
        assert_eq!(text(800000.0), "800000");
        assert_eq!(text(-3.0), "-3");
        assert_eq!(text(0.1), "0.1");
        assert_eq!(text(f32::NAN), "\"NaN\"");
        assert_eq!(text(f32::INFINITY), "\"inf\"");
    }

    #[test]
    fn masks_reference_addresses_and_nothing_else() {
        assert_eq!(mask_addresses("Disguise.Init function: 0xbc6d69620"), "Disguise.Init function: <addr>");
        assert_eq!(mask_addresses("table: 0x1a3f2c80 / thread: 0xff"), "table: <addr> / thread: <addr>");
        // A guid printed as hex and a prefix with no hex after it are left alone.
        assert_eq!(mask_addresses("target 0x100007d2"), "target 0x100007d2");
        assert_eq!(mask_addresses("function: 0x"), "function: 0x");
        assert_eq!(mask_addresses("[VzaCon001.Objectives.001] ü"), "[VzaCon001.Objectives.001] ü");
        assert_eq!(src_line("@MrxMissionFlow", 132), "@mrxmissionflow:132");
        assert_eq!(src_line("", 3), "");
    }

    #[test]
    fn the_stamp_covers_what_the_line_map_compares_and_nothing_else() {
        let child = proto(vec![0x1E, 0x1E], vec![Constant::Number(0x3F80_0000)], Vec::new());
        let base = proto(vec![0x01, 0x1E], vec![Constant::String(b"Pg".to_vec()), Constant::Boolean(true)], vec![child]);
        let s = stamp(&base);
        assert!(s.starts_with("sha256:") && s.len() == 7 + 64, "{s}");

        // Line info, line-defined fields, source, locals and upvalue names do not change it.
        let mut same = base.clone();
        same.line_info = vec![9, 9];
        same.line_defined = 5;
        same.last_line_defined = 6;
        same.source = None;
        same.protos[0].line_info = vec![7, 8];
        same.upvalues = vec![b"x".to_vec()];
        assert_eq!(stamp(&same), s);

        // Every field the line map compares does, in the main function and in a child.
        let edits: [fn(&mut Proto); 8] = [
            |p| p.nups = 1,
            |p| p.num_params = 1,
            |p| p.is_vararg = 0,
            |p| p.max_stack = 3,
            |p| p.code[0] = 0x02,
            |p| p.constants[0] = Constant::String(b"Pt".to_vec()),
            |p| p.protos[0].constants[0] = Constant::Number(0x4000_0000),
            |p| p.protos.push(proto(Vec::new(), Vec::new(), Vec::new())),
        ];
        for (i, edit) in edits.iter().enumerate() {
            let mut p = base.clone();
            edit(&mut p);
            assert_ne!(stamp(&p), s, "edit {i}");
        }
    }

    #[test]
    fn game_text_is_cp1252_both_ways() {
        let bytes: Vec<u8> = (0u8..=255).collect();
        let text = game_text(&bytes);
        assert_eq!(game_bytes(&text).unwrap(), bytes);
        assert_eq!(game_text(&[0x80, 0xFC, 0x9D]), "\u{20AC}\u{FC}\u{9D}");
        assert!(game_bytes("\u{0100}").is_err());
    }
}
