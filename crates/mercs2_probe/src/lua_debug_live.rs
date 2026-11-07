//! `lua-debug`: the Workshop Missions debugger's protocol (`mercs2_formats::lua_debug`) against a live host
//! on the bridge port (the retail game's m2-sdk.dll, through the lua-bridge), driven the way the Missions
//! page drives it: hello, the module's line tables in `debug.lines` frames, attach, then every debug frame
//! the host sends printed as one JSON line. The client is `mercs2_bridge::debug`, which checks the events
//! for gaps.
//!
//! ```text
//! lua-debug [--addr host:port] [--module <name> [--bank resident|vz|shell] [--line <corpus line>]...]
//!           [--engine <binding>]... [--then continue|wait|disconnect] [--repl] [--seconds S]
//! lua-debug [--addr host:port] --bindings
//! ```
//!
//! * `--bindings` prints every engine binding the host can break on, one per line, and exits.
//! * `--engine` sets an engine breakpoint (`Pg.Spawn`): the run pauses when any script calls the binding,
//!   and the call's `debug.engine` event (its arguments) is printed before the pause. With `--engine`,
//!   `--module` is optional. `--then continue` after an engine pause also waits for that call's
//!   `debug.engine_return`: the binding ran after the resume.
//! * `--module` names a corpus file of any bank, a library script as much as a contract. The line tables
//!   and breakpoints come from the shipped chunk it was decompiled from (`lua_linemap::shipped_for`: by
//!   name, else by code) and our compile of the file (`lua_debug::line_tables`,
//!   `lua_debug::line_breakpoints`); the attach filters by the name the game runs it under (`Init.lua`
//!   runs as `c4`).
//! * With no `--line` it records for `--seconds`, prints which functions ran with their corpus lines, and
//!   detaches: what a breakpoint can be put on.
//! * With `--line` it waits up to `--seconds` for the pause, then (`--then`): `continue` sends
//!   `debug.continue`; `wait` waits for the player to resume (F8 or the notice's Continue); `disconnect`
//!   closes the connection while paused, then connects again and checks the debugger is free.
//! * `--repl` runs a chunk while paused, over the WebSocket and over raw TCP, before resuming.
//!
//! Exit 0 when every step happened as the protocol says; 1 when one did not (a refusal, a gap, a missing
//! pause, a timeout); 2 for a usage error or no host.

use mercs2_bridge::debug::{DebugClient, DebugError, Frame};
use mercs2_formats::lua_debug::{self, Event, Request, MAX_FRAME_BYTES, PROTOCOL};
use mercs2_formats::lua_linemap::{shipped_for, shipped_scripts, FunctionKey, LineMap, CORPUS_SOURCES};
use mercs2_formats::luachunk::decode;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long one answer may take. Requests are handled on a Lua tick, so a loading screen holds them.
const ANSWER: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Then {
    Continue,
    Wait,
    Disconnect,
}

struct Options {
    addr: String,
    module: String,
    bank: Option<String>,
    lines: Vec<u32>,
    engine: Vec<String>,
    bindings: bool,
    then: Then,
    repl: bool,
    seconds: u64,
}

enum Failure {
    Usage(String),
    Gate(String),
}

fn gate(e: impl std::fmt::Display) -> Failure {
    Failure::Gate(e.to_string())
}

pub fn cli(wad: &str, args: &[String]) -> i32 {
    let opts = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("lua-debug: {e}");
            return 2;
        }
    };
    match run(Path::new(wad), &opts) {
        Ok(()) => 0,
        Err(Failure::Usage(e)) => {
            eprintln!("lua-debug: {e}");
            2
        }
        Err(Failure::Gate(e)) => {
            eprintln!("lua-debug: FAILED: {e}");
            1
        }
    }
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        addr: mercs2_bridge::DEFAULT_ADDR.to_string(),
        module: String::new(),
        bank: None,
        lines: Vec::new(),
        engine: Vec::new(),
        bindings: false,
        then: Then::Continue,
        repl: false,
        seconds: 30,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| format!("{a} needs a value"));
        match a.as_str() {
            "--addr" => o.addr = value()?,
            "--module" => o.module = value()?,
            "--bank" => o.bank = Some(value()?),
            "--line" => o.lines.push(value()?.parse().map_err(|e| format!("--line: {e}"))?),
            "--engine" => o.engine.push(value()?),
            "--bindings" => o.bindings = true,
            "--then" => {
                o.then = match value()?.as_str() {
                    "continue" => Then::Continue,
                    "wait" => Then::Wait,
                    "disconnect" => Then::Disconnect,
                    other => return Err(format!("--then {other:?}: continue, wait or disconnect")),
                }
            }
            "--repl" => o.repl = true,
            "--seconds" => o.seconds = value()?.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--wad" => {
                value()?;
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if o.module.is_empty() && o.engine.is_empty() && !o.bindings {
        return Err("--module <name>, --engine <binding> or --bindings is required".into());
    }
    if o.module.is_empty() && !o.lines.is_empty() {
        return Err("--line needs --module".into());
    }
    Ok(o)
}

/// The module's line map: the shipped chunk its corpus file was decompiled from (by name, else by code)
/// against our compile of the file; with the name the game runs it under, which the attach filters by.
fn module_map(vz: &Path, corpus: &Path, module: &str, bank: Option<&str>) -> Result<(String, LineMap), String> {
    let banks: Vec<&str> = CORPUS_SOURCES
        .iter()
        .map(|s| s.2)
        .filter(|b| bank.is_none_or(|want| want == *b) && corpus.join(b).join(format!("{module}.lua")).is_file())
        .collect();
    let bank = match banks[..] {
        [one] => one,
        [] => return Err(format!("no corpus file {module}.lua in {}", bank.unwrap_or("any bank"))),
        _ => return Err(format!("{module}.lua is in banks {banks:?}: pass --bank")),
    };
    let file = corpus.join(bank).join(format!("{module}.lua"));
    let source = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    let ours = mercs2_luac::compile(&source, module).map_err(|e| format!("compiling {}: {e}", file.display()))?;
    let ours = decode(&ours).map_err(|e| format!("our compile of {module}: {e}"))?;
    let dir = corpus.join(bank);
    let files: Vec<String> = std::fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "lua"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    let shipped = shipped_scripts(vz, bank)?;
    let retail = shipped_for(&shipped, module, &ours.main, &files)?;
    let map = LineMap::build(&retail.main, &ours.main).map_err(|e| format!("{module}: {e}"))?;
    Ok((retail.module()?, map))
}

/// The client side of one connection: every debug frame printed; frames read while waiting for something
/// else are kept, in order.
struct Client {
    c: DebugClient,
    kept: VecDeque<Frame>,
}

impl Client {
    fn new(addr: &str) -> Result<Client, Failure> {
        let c = DebugClient::connect(addr, ANSWER).map_err(|e| match e {
            DebugError::Connect(_) => Failure::Usage(e.to_string()),
            other => gate(other),
        })?;
        Ok(Client { c, kept: VecDeque::new() })
    }

    fn frame(&mut self, until: Instant) -> Result<Option<Frame>, Failure> {
        if let Some(f) = self.kept.pop_front() {
            return Ok(Some(f));
        }
        let f = self.c.frame(until).map_err(gate)?;
        if let Some(Frame::Debug(ev)) = &f {
            println!("{}", ev.to_json());
        }
        Ok(f)
    }

    /// The next debug event by `until`, or `None` at the deadline; feed lines are passed over.
    fn event(&mut self, until: Instant) -> Result<Option<Event>, Failure> {
        loop {
            match self.frame(until)? {
                None => return Ok(None),
                Some(Frame::Debug(ev)) => return Ok(Some(ev)),
                Some(Frame::Feed { .. }) => {}
            }
        }
    }

    fn ask(&mut self, req: Request) -> Result<Event, Failure> {
        let (answer, before) = self.c.ask(&req, Instant::now() + ANSWER).map_err(gate)?;
        for f in &before {
            if let Frame::Debug(ev) = f {
                println!("{}", ev.to_json());
            }
        }
        println!("{}", answer.to_json());
        self.kept.extend(before);
        Ok(answer)
    }

    fn hello(&mut self) -> Result<Event, Failure> {
        let id = self.c.id("ld");
        self.ask(Request::Hello { id, protocol: PROTOCOL })
    }
}

fn run(vz: &Path, o: &Options) -> Result<(), Failure> {
    // The module's map, its tables and its line breakpoints, when one is named.
    let mut watched: Option<(String, LineMap)> = None;
    let mut tables = Vec::new();
    let mut breakpoints = Vec::new();
    if !o.module.is_empty() {
        let corpus = mercs2_engine::script::corpus::root().ok_or_else(|| Failure::Usage("the vendored Lua corpus is missing".into()))?;
        let (module, map) = module_map(vz, &corpus, &o.module, o.bank.as_deref()).map_err(Failure::Usage)?;
        tables = lua_debug::line_tables(&module, &map);
        for &line in &o.lines {
            breakpoints.extend(lua_debug::line_breakpoints(&module, &map, line).map_err(|e| Failure::Usage(e.to_string()))?);
        }
        eprintln!(
            "lua-debug: {} (running as {module}): {} of {} functions have line tables; {} breakpoint position(s)",
            o.module,
            tables.len(),
            map.functions.len(),
            breakpoints.len()
        );
        watched = Some((module, map));
    }

    let mut c = Client::new(&o.addr)?;
    match c.hello()? {
        Event::Host { attached: Some(holder), .. } => return Err(gate(format!("the debugger is already held by {holder}"))),
        Event::Host { kind, build, .. } => eprintln!("lua-debug: host {kind:?} {build}"),
        other => return Err(gate(format!("a hello answered with {other:?}"))),
    }
    if o.bindings {
        let id = c.c.id("ld");
        let (answer, _) = c.c.ask(&Request::Bindings { id }, Instant::now() + ANSWER).map_err(gate)?;
        let Event::Names { bindings, .. } = answer else { return Err(gate(format!("debug.bindings answered with {answer:?}"))) };
        for b in &bindings {
            println!("{b}");
        }
        eprintln!("lua-debug: {} engine bindings", bindings.len());
        return Ok(());
    }
    for f in lua_debug::lines_frames(tables, MAX_FRAME_BYTES, |n| format!("lines{n}")).map_err(gate)? {
        c.ask(f)?;
    }
    let id = c.c.id("ld");
    let modules = watched.iter().map(|(m, _)| m.clone()).collect();
    let attach =
        Request::Attach { id, protocol: PROTOCOL, client: "mercs2_probe lua-debug".into(), modules, breakpoints, engine: o.engine.clone() };
    c.ask(attach)?;

    let until = Instant::now() + Duration::from_secs(o.seconds);
    if o.lines.is_empty() && o.engine.is_empty() {
        let Some((_, map)) = &watched else { unreachable!("parse requires --module, --engine or --bindings") };
        let mut entered: BTreeMap<(u32, u32, u32), usize> = BTreeMap::new();
        while let Some(ev) = c.event(until)? {
            if let Event::Entry { at, .. } = ev {
                *entered.entry((at.function.line_defined, at.function.last_line_defined, at.function.code_len)).or_default() += 1;
            }
        }
        for ((a, b, n), count) in &entered {
            let key = FunctionKey { line_defined: *a, last_line_defined: *b, code_len: *n };
            let corpus = match map.function(key) {
                Ok(f) => format!("corpus lines {}-{}", f.lines.iter().min().unwrap_or(&0), f.lines.iter().max().unwrap_or(&0)),
                Err(e) => e.to_string(),
            };
            eprintln!("lua-debug: entered {count}x: retail lines {a}-{b} ({n} instructions), {corpus}");
        }
        return detach(&mut c);
    }
    let mut call = None;
    let paused = loop {
        match c.event(until)? {
            Some(ev @ Event::Paused { .. }) => break ev,
            Some(ev @ Event::Engine { .. }) => call = Some(ev),
            Some(_) => {}
            None => return Err(gate(format!("no pause on lines {:?} or bindings {:?} within {}s", o.lines, o.engine, o.seconds))),
        }
    };
    if let (Event::Paused { reason: lua_debug::PauseReason::Engine, .. }, Some(call)) = (&paused, &call) {
        eprintln!("lua-debug: the call: {}", call.to_json());
    }
    eprintln!("lua-debug: paused: {}", paused.to_json());
    if o.repl {
        repl_while_paused(&mut c, &o.addr)?;
    }
    // Paused before an engine call: once resumed, the binding runs and its results must arrive.
    let returns = match (&paused, &call) {
        (Event::Paused { reason: lua_debug::PauseReason::Engine, .. }, Some(Event::Engine { seq, .. })) => Some(*seq),
        _ => None,
    };
    match o.then {
        Then::Continue => {
            let id = c.c.id("ld");
            c.ask(Request::Continue { id })?;
            wait_for(&mut c, "debug.resumed after the continue", |e| matches!(e, Event::Resumed { .. }), Instant::now() + ANSWER)?;
            if let Some(seq) = returns {
                let what = format!("debug.engine_return of call {seq} after the resume");
                wait_for(&mut c, &what, |e| matches!(e, Event::EngineReturn { call, .. } if *call == seq), Instant::now() + ANSWER)?;
                eprintln!("lua-debug: the paused call returned");
            }
            detach(&mut c)
        }
        Then::Wait => {
            eprintln!("lua-debug: waiting for the player to resume (F8, fn+F8 on a Mac keyboard, or the notice's Continue)");
            let until = Instant::now() + Duration::from_secs(o.seconds);
            wait_for(&mut c, "the player's resume", |e| matches!(e, Event::Resumed { .. }), until)?;
            detach(&mut c)
        }
        Then::Disconnect => {
            drop(c);
            eprintln!("lua-debug: disconnected while paused; asking a new connection who holds the debugger");
            let until = Instant::now() + ANSWER;
            loop {
                let mut again = Client::new(&o.addr)?;
                match again.hello()? {
                    Event::Host { attached: None, .. } => return Ok(()),
                    Event::Host { attached: Some(h), .. } if Instant::now() < until => {
                        eprintln!("lua-debug: still held by {h}");
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    other => return Err(gate(format!("the debugger is not free after the disconnect: {other:?}"))),
                }
            }
        }
    }
}

fn wait_for(c: &mut Client, what: &str, want: impl Fn(&Event) -> bool, until: Instant) -> Result<(), Failure> {
    loop {
        match c.event(until)? {
            Some(ev) if want(&ev) => return Ok(()),
            Some(Event::Detached { error: Some(e), .. }) => return Err(gate(format!("detached with an error: {e}"))),
            Some(_) => {}
            None => return Err(gate(format!("no {what}"))),
        }
    }
}

fn detach(c: &mut Client) -> Result<(), Failure> {
    let id = c.c.id("ld");
    c.ask(Request::Detach { id })?;
    wait_for(c, "debug.detached after the detach", |e| matches!(e, Event::Detached { error: None, .. }), Instant::now() + ANSWER)
}

/// While paused, a chunk sent over the WebSocket and one over raw TCP both run.
fn repl_while_paused(c: &mut Client, addr: &str) -> Result<(), Failure> {
    let marker = "lua-debug: a WebSocket chunk ran while paused";
    let req = serde_json::json!({ "id": "ld-repl", "code": format!("Loader.WsSend({marker:?})") });
    c.c.send_text(req.to_string().as_bytes()).map_err(gate)?;
    let until = Instant::now() + ANSWER;
    loop {
        match c.c.frame(until).map_err(gate)? {
            Some(Frame::Feed { kind, line }) if kind == "ws" && line == marker => break,
            Some(Frame::Debug(ev @ Event::Resumed { .. })) => {
                return Err(gate(format!("the game resumed before the chunk's line arrived: {}", ev.to_json())));
            }
            Some(other) => c.kept.push_back(other),
            None => return Err(gate("the WebSocket chunk did not run while paused")),
        }
    }
    eprintln!("lua-debug: {marker}");
    let mut raw = mercs2_bridge::Bridge::connect(addr, Duration::from_millis(60), ANSWER)
        .map_err(|e| gate(format!("raw TCP: {e:?}")))?;
    let reply = raw.eval("return 6 * 7").map_err(|e| gate(format!("raw TCP chunk while paused: {e:?}")))?;
    if !reply.contains("42") {
        return Err(gate(format!("the raw TCP chunk answered {reply:?}")));
    }
    eprintln!("lua-debug: a raw TCP chunk ran while paused: {reply:?}");
    Ok(())
}
