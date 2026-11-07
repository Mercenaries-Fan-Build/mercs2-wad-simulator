//! The Missions page's Debug mode (`docs/modernization/workshop_pages/missions.md`): a live run laid over the
//! graphs of the scripts it watches.
//!
//! * **The link** is one worker thread owning a `mercs2_bridge::debug::DebugClient` on the bridge port. The
//!   page sends it commands (attach, breakpoints, continue, step, detach) and it sends back every frame the
//!   host sends, in order, asking egui to repaint. Requests are answered on the game's Lua tick, so the
//!   worker, never the frame loop, waits for them. A refused request is an answer like any other: the page
//!   shows why, and the link stays up.
//! * **The scripts** a run watches are the one the page attached to and every script with a breakpoint, a
//!   library script as much as a contract. Each has its line map ([`map_script`]): the shipped chunk the
//!   corpus file was decompiled from (`lua_linemap::shipped_for`: by name, else by code), paired instruction
//!   by instruction with our compile of the script's graph. A retail position (original line) becomes the
//!   corpus line the graph's nodes were lifted from. The map names the module the way the game runs it (the
//!   chunk's source name), which is the name the attach filters by. A script with no map (a bank the map
//!   does not cover, a tree that cannot be paired) cannot be watched, and an attach that names one is
//!   refused here with its name and why.
//! * **Adding a script to a run** sends its line tables and a `debug.watch` naming every watched script, so
//!   it joins a running or a paused run alike (the pause holds).
//! * **Engine breakpoints** name engine bindings from the list the host gives right after hello
//!   (`debug.bindings`): a call of one from any script pauses the run before the binding runs, with the
//!   call and its arguments kept for the page. A pause in a script the run does not watch can only continue.
//! * **The run** counts line events per script and corpus line and keeps where the run is paused; the canvas
//!   lights a node whose line ran, with its count.

use crate::blueprint::{self, Graph};
use crate::lua_source::Script;
use mercs2_bridge::debug::{DebugClient, DebugError, Frame};
use mercs2_formats::lua_debug::{self, Arg, At, Breakpoint, Event, HostKind, PauseReason, Request, Step, MAX_FRAME_BYTES, PROTOCOL};
use mercs2_formats::lua_linemap::{shipped_for, shipped_scripts, LineMap, Shipped, CORPUS_SOURCES};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

/// How long one answer may take: a loading screen holds requests until the next Lua tick.
const ANSWER: Duration = Duration::from_secs(20);
/// How long the worker waits for a frame before it looks for commands again.
const POLL: Duration = Duration::from_millis(30);
/// The name the host shows another client while this one holds it.
const CLIENT: &str = "the Workshop's Missions page";
/// The id of the request for the host's engine binding names, sent after hello.
const NAMES_ID: &str = "names";

enum Cmd {
    /// Requests sent in order, stopping at the first refused: line tables, then an attach or a watch (and
    /// the breakpoints that need it).
    Batch(Vec<Request>),
    Send(Request),
}

enum Msg {
    Frame(Frame),
    Error(String),
}

struct Link {
    tx: Sender<Cmd>,
    rx: Receiver<Msg>,
}

fn worker(addr: String, ctx: egui::Context, rx: Receiver<Cmd>, tx: Sender<Msg>) {
    let out = |m: Msg| {
        let _ = tx.send(m);
        ctx.request_repaint();
    };
    if let Err(e) = serve(&addr, &rx, &out) {
        out(Msg::Error(e.to_string()));
    }
}

/// Hello and the host's binding names, then commands and frames until the page drops the link (its sender)
/// or the connection fails. A refused hello ends the link. Any other refusal goes to the page as the event
/// it is, after the frames that came before it; the rest of a command's requests are not sent after one.
fn serve(addr: &str, rx: &Receiver<Cmd>, out: &impl Fn(Msg)) -> Result<(), DebugError> {
    let mut c = DebugClient::connect(addr, ANSWER)?;
    let id = c.id("w");
    let (answer, before) = c.ask(&Request::Hello { id, protocol: PROTOCOL }, Instant::now() + ANSWER)?;
    before.into_iter().for_each(|f| out(Msg::Frame(f)));
    out(Msg::Frame(Frame::Debug(answer)));
    let ask = |c: &mut DebugClient, req: &Request| -> Result<bool, DebugError> {
        match c.ask(req, Instant::now() + ANSWER) {
            Ok((answer, before)) => {
                before.into_iter().for_each(|f| out(Msg::Frame(f)));
                out(Msg::Frame(Frame::Debug(answer)));
                Ok(true)
            }
            Err(DebugError::Refused { id, error, before }) => {
                before.into_iter().for_each(|f| out(Msg::Frame(f)));
                out(Msg::Frame(Frame::Debug(Event::Refused { id, error })));
                Ok(false)
            }
            Err(e) => Err(e),
        }
    };
    ask(&mut c, &Request::Bindings { id: NAMES_ID.into() })?;
    loop {
        loop {
            match rx.try_recv() {
                Ok(Cmd::Batch(reqs)) => {
                    for r in &reqs {
                        if !ask(&mut c, r)? {
                            break;
                        }
                    }
                }
                Ok(Cmd::Send(r)) => {
                    ask(&mut c, &r)?;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(()),
            }
        }
        if let Some(f) = c.frame(Instant::now() + POLL)? {
            out(Msg::Frame(f));
        }
    }
}

/// Where the link is.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Off,
    Connecting,
    /// The host answered hello; not attached.
    Ready,
    Attaching,
    Attached,
    Paused,
    Failed(String),
}

/// A script's line map. `module` is the name the game runs it under (its shipped chunk's source name).
pub struct Mapped {
    pub module: String,
    pub map: LineMap,
}

/// A script the page has mapped: the open one, or one with a breakpoint.
pub struct Watched {
    /// Corpus path (`lua/resident/mrxutil.lua`) and file stem.
    pub path: String,
    pub name: String,
    pub mapped: Result<Mapped, String>,
    /// Corpus lines with a breakpoint, each with what it was set on (`SpawnObject() entry`).
    pub breakpoints: BTreeMap<u32, String>,
}

/// One engine call as the host reported it.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineCall {
    pub binding: String,
    pub args: Vec<Arg>,
    /// The calling instruction.
    pub at: At,
}

/// Where the run is paused: the module as the game names it, the corpus line when its map has one, why,
/// and for an engine breakpoint the call it stopped before.
#[derive(Clone, Debug, PartialEq)]
pub struct PausedAt {
    pub module: String,
    pub line: Option<u32>,
    pub reason: PauseReason,
    pub call: Option<EngineCall>,
}

/// What the attached run has done so far, in the graphs' line space.
#[derive(Default)]
pub struct Run {
    pub events: u64,
    /// Line events per (module, lower case; corpus line).
    pub hits: HashMap<(String, u32), u32>,
    pub paused: Option<PausedAt>,
    /// The last engine call reported, which an engine breakpoint's pause follows.
    last_call: Option<EngineCall>,
    /// A position in a watched module its map could not translate (an unmapped function), the first one.
    pub unmapped: Option<String>,
}

impl Run {
    /// Fold one event in, translating positions through the map of their module.
    pub fn absorb(&mut self, ev: &Event, maps: &[&Mapped]) {
        if ev.seq().is_some() {
            self.events += 1;
        }
        let mut line = |at: &At| {
            let m = maps.iter().find(|m| m.module.eq_ignore_ascii_case(&at.module))?;
            match m.map.line(at.function, at.pc) {
                Ok(l) => Some(l),
                Err(e) => {
                    self.unmapped.get_or_insert(format!("{}: {e}", m.module));
                    None
                }
            }
        };
        match ev {
            Event::Line { at, .. } => {
                if let Some(l) = line(at) {
                    *self.hits.entry((at.module.to_ascii_lowercase(), l)).or_default() += 1;
                }
            }
            Event::Engine { at, binding, args, .. } => {
                self.last_call = Some(EngineCall { binding: binding.clone(), args: args.clone(), at: at.clone() });
            }
            Event::Paused { at, reason, .. } => {
                let call = if *reason == PauseReason::Engine { self.last_call.clone() } else { None };
                self.paused = Some(PausedAt { module: at.module.clone(), line: line(at), reason: *reason, call });
            }
            Event::Resumed { .. } | Event::Detached { .. } => self.paused = None,
            _ => {}
        }
    }
}

/// How the run marks one corpus line of a script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineRun {
    pub hits: u32,
    pub paused: bool,
    pub breakpoint: bool,
}

/// The app's one debug session: the host takes one debug client, so the Missions page drives it and every
/// page reads it (the rail shows a pause). The app owns it, so reloading a page's data never drops the
/// link (which would resume the game).
pub struct Debug {
    /// The Edit/Debug switch.
    pub on: bool,
    pub state: LinkState,
    /// What the host said it is: kind and build.
    pub host: Option<(HostKind, String)>,
    /// Every script the page has mapped, by corpus path.
    pub scripts: BTreeMap<String, Watched>,
    /// The corpus path of the open script.
    pub open: Option<String>,
    /// The corpus paths the attached run watches.
    pub attached: BTreeSet<String>,
    /// Engine breakpoints: binding names whose calls pause the run.
    pub engine: BTreeSet<String>,
    /// The engine bindings the host can break on, or why it named none; `None` before it answers.
    pub bindings: Option<Result<Vec<String>, String>>,
    pub run: Run,
    /// The open base WAD, as the host last reported it.
    pub base: Option<String>,
    /// The `vz.wad` beside it, or why there is none.
    pub vz_wad: Result<PathBuf, String>,
    /// The last thing the host refused, or the run's error.
    pub problem: Option<String>,
    /// Each bank's shipped scripts, read once per `vz.wad`.
    shipped: HashMap<String, Result<Vec<Shipped>, String>>,
    link: Option<Link>,
    /// The attach request's id and the paths it watches, so its answer moves the state on.
    attaching: Option<(String, BTreeSet<String>)>,
    /// A watch request's id and the paths it watches, which the run watches once it is answered.
    watching: Option<(String, BTreeSet<String>)>,
}

impl Default for Debug {
    fn default() -> Self {
        Debug {
            on: false,
            state: LinkState::Off,
            host: None,
            scripts: BTreeMap::new(),
            open: None,
            attached: BTreeSet::new(),
            engine: BTreeSet::new(),
            bindings: None,
            run: Run::default(),
            base: None,
            vz_wad: Err("open a level first: the line map reads the shipped script from the vz.wad beside it".into()),
            problem: None,
            shipped: HashMap::new(),
            link: None,
            attaching: None,
            watching: None,
        }
    }
}

impl Debug {
    /// The open base WAD, called each frame: the line maps read the shipped scripts from the `vz.wad`
    /// beside it, found when the base changes. Maps built from another `vz.wad` are dropped; a script with
    /// breakpoints keeps them and says it must be opened again to map.
    pub fn set_base_wad(&mut self, base: Option<&str>) {
        if self.base.as_deref() == base {
            return;
        }
        self.base = base.map(str::to_owned);
        self.vz_wad = match base {
            None => Err("open a level first: the line map reads the shipped script from the vz.wad beside it".into()),
            Some(b) => mercs2_quartermaster::sound::sibling_wad(Path::new(b), "vz.wad"),
        };
        self.shipped.clear();
        self.scripts.retain(|_, w| !w.breakpoints.is_empty());
        for w in self.scripts.values_mut() {
            w.mapped = Err(format!("the game data changed: open {} again to map it", w.name));
        }
    }

    /// Whether the debugger holds the game paused: the rail says so on every page.
    pub fn paused(&self) -> bool {
        self.state == LinkState::Paused
    }

    pub fn connect(&mut self, addr: &str, ctx: egui::Context) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (msg_tx, msg_rx) = mpsc::channel();
        let addr = addr.to_string();
        if let Err(e) = std::thread::Builder::new().name("missions debug link".into()).spawn(move || worker(addr, ctx, cmd_rx, msg_tx)) {
            self.state = LinkState::Failed(format!("could not start the debug link: {e}"));
            return;
        }
        self.link = Some(Link { tx: cmd_tx, rx: msg_rx });
        self.state = LinkState::Connecting;
        self.problem = None;
    }

    /// Drop the link: the bridge tells the host, which removes the hook and resumes a paused game.
    pub fn disconnect(&mut self) {
        self.link = None;
        self.state = LinkState::Off;
        self.attached.clear();
        self.attaching = None;
        self.watching = None;
        self.bindings = None;
        self.run.paused = None;
    }

    fn send(&mut self, cmd: Cmd) {
        let sent = self.link.as_ref().is_some_and(|l| l.tx.send(cmd).is_ok());
        if !sent {
            self.state = LinkState::Failed("the debug link has stopped".into());
            self.link = None;
        }
    }

    /// The script at corpus path `path` is open on the page.
    pub fn open_script(&mut self, path: &str) {
        self.open = Some(path.to_string());
    }

    /// Map the script at `path` (file stem `name`, its graph `script`/`chunk`) unless it has a map already.
    /// `files` names every corpus file of its bank, which pairs a file named after no shipped script by its
    /// code. A failure is kept on the script, and shown by name.
    pub fn watch(&mut self, path: &str, name: &str, script: &Script, chunk: &Graph, files: &[&str]) {
        if self.scripts.get(path).is_some_and(|w| w.mapped.is_ok()) {
            return;
        }
        let mapped = self.map(path, name, script, chunk, files);
        match self.scripts.get_mut(path) {
            Some(w) => w.mapped = mapped,
            None => {
                let w = Watched { path: path.to_string(), name: name.to_string(), mapped, breakpoints: BTreeMap::new() };
                self.scripts.insert(path.to_string(), w);
            }
        }
    }

    fn map(&mut self, path: &str, name: &str, script: &Script, chunk: &Graph, files: &[&str]) -> Result<Mapped, String> {
        let bank = bank_of(path)?;
        let vz = self.vz_wad.clone()?;
        let shipped = self.shipped.entry(bank.to_string()).or_insert_with(|| shipped_scripts(&vz, bank));
        map_script(shipped.as_ref().map_err(String::clone)?, name, script, chunk, files)
    }

    /// The watched script whose module the game names `module`.
    pub fn script_of(&self, module: &str) -> Option<&Watched> {
        self.scripts.values().find(|w| w.mapped.as_ref().is_ok_and(|m| m.module.eq_ignore_ascii_case(module)))
    }

    /// How the run marks corpus line `line` of the script at `path`.
    pub fn line_run(&self, path: &str, line: u32) -> LineRun {
        let Some(w) = self.scripts.get(path) else { return LineRun::default() };
        let breakpoint = w.breakpoints.contains_key(&line);
        let Ok(m) = &w.mapped else { return LineRun { breakpoint, ..LineRun::default() } };
        LineRun {
            hits: self.run.hits.get(&(m.module.to_ascii_lowercase(), line)).copied().unwrap_or(0),
            paused: self.run.paused.as_ref().is_some_and(|p| p.module.eq_ignore_ascii_case(&m.module) && p.line == Some(line)),
            breakpoint,
        }
    }

    /// The corpus paths an attach from the page watches: the open script and every one with a breakpoint.
    fn wanted(&self) -> BTreeSet<String> {
        let mut paths: BTreeSet<String> = self.scripts.values().filter(|w| !w.breakpoints.is_empty()).map(|w| w.path.clone()).collect();
        paths.extend(self.open.clone());
        paths
    }

    /// Every breakpoint of the scripts at `paths`, as the host takes them.
    fn breakpoint_positions(&self, paths: &BTreeSet<String>) -> Result<Vec<Breakpoint>, String> {
        let mut out = Vec::new();
        for w in paths.iter().filter_map(|p| self.scripts.get(p)) {
            if w.breakpoints.is_empty() {
                continue;
            }
            let m = w.mapped.as_ref().map_err(|e| format!("{} has breakpoints and no line map: {e}", w.name))?;
            for &line in w.breakpoints.keys() {
                out.extend(lua_debug::line_breakpoints(&m.module, &m.map, line).map_err(|e| format!("{}: {e}", w.name))?);
            }
        }
        Ok(out)
    }

    /// The line maps of the scripts at `paths`, or an error naming each that has none (a watched script with
    /// no map would never stop on its lines).
    fn maps_of(&self, paths: &BTreeSet<String>) -> Result<Vec<&Mapped>, String> {
        let mut missing = Vec::new();
        let mut maps = Vec::new();
        for p in paths {
            match self.scripts.get(p).map(|w| (w, w.mapped.as_ref())) {
                Some((_, Ok(m))) => maps.push(m),
                Some((w, Err(e))) => missing.push(format!("{}: {e}", w.name)),
                None => missing.push(format!("{p}: not mapped")),
            }
        }
        if !missing.is_empty() {
            return Err(format!("a watched script has no line map, so the run would never stop on its lines: {}", missing.join("; ")));
        }
        Ok(maps)
    }

    /// Attach watching the open script and every script with a breakpoint (their line tables, then the
    /// module filter, the breakpoints and the engine breakpoints). Every script must have a map, or nothing
    /// is sent and the error names each that has none.
    pub fn attach(&mut self) -> Result<(), String> {
        if self.state != LinkState::Ready {
            return Err(format!("the debug link is {:?}, not ready to attach", self.state));
        }
        let paths = self.wanted();
        if paths.is_empty() && self.engine.is_empty() {
            return Err("open a script or set an engine breakpoint to attach".into());
        }
        let maps = self.maps_of(&paths)?;
        let tables = maps.iter().flat_map(|m| lua_debug::line_tables(&m.module, &m.map)).collect();
        let modules: Vec<String> = maps.iter().map(|m| m.module.clone()).collect();
        let mut reqs = lua_debug::lines_frames(tables, MAX_FRAME_BYTES, |n| format!("lines{n}")).map_err(|e| e.to_string())?;
        let id = format!("attach-{}", modules.join("-"));
        reqs.push(Request::Attach {
            id: id.clone(),
            protocol: PROTOCOL,
            client: CLIENT.into(),
            modules,
            breakpoints: self.breakpoint_positions(&paths)?,
            engine: self.engine.iter().cloned().collect(),
        });
        self.run = Run::default();
        self.attaching = Some((id, paths));
        self.state = LinkState::Attaching;
        self.problem = None;
        self.send(Cmd::Batch(reqs));
        Ok(())
    }

    /// Add the open script to the run.
    pub fn add_open(&mut self) -> Result<(), String> {
        let mut paths = self.attached.clone();
        paths.extend(self.open.clone());
        self.watch_paths(paths, Vec::new())
    }

    /// The run, running or paused, watches the scripts at `paths`: the line tables of those it does not
    /// watch yet, then a `debug.watch` naming every one, then `then` (the breakpoints that need them).
    fn watch_paths(&mut self, paths: BTreeSet<String>, then: Vec<Request>) -> Result<(), String> {
        if !matches!(self.state, LinkState::Attached | LinkState::Paused) {
            return Err(format!("the debug link is {:?}, not attached", self.state));
        }
        if self.watching.is_some() {
            return Err("the last change to what the run watches is still being answered".into());
        }
        let maps = self.maps_of(&paths)?;
        let joining: Vec<&Mapped> = paths
            .iter()
            .zip(&maps)
            .filter(|(p, _)| !self.attached.contains(*p))
            .map(|(_, m)| *m)
            .collect();
        let tables = joining.iter().flat_map(|m| lua_debug::line_tables(&m.module, &m.map)).collect();
        let modules: Vec<String> = maps.iter().map(|m| m.module.clone()).collect();
        let mut reqs = lua_debug::lines_frames(tables, MAX_FRAME_BYTES, |n| format!("wlines{n}")).map_err(|e| e.to_string())?;
        let id = format!("watch-{}", modules.join("-"));
        reqs.push(Request::Watch { id: id.clone(), modules });
        reqs.extend(then);
        self.watching = Some((id, paths));
        self.problem = None;
        self.send(Cmd::Batch(reqs));
        Ok(())
    }

    /// The scripts the run watches, with those a watch in flight is adding: what a breakpoint change must
    /// cover, or the host's set would lose a joining script's breakpoints.
    fn run_paths(&self) -> BTreeSet<String> {
        let mut paths = self.attached.clone();
        if let Some((_, joining)) = &self.watching {
            paths.extend(joining.iter().cloned());
        }
        paths
    }

    /// The breakpoint set of the scripts at `paths` and the engine breakpoints, as one request.
    fn breakpoints_request(&self, paths: &BTreeSet<String>) -> Result<Request, String> {
        Ok(Request::Breakpoints {
            id: "breakpoints".into(),
            breakpoints: self.breakpoint_positions(paths)?,
            engine: self.engine.iter().cloned().collect(),
        })
    }

    /// Whether a step can run from the pause: it is in a script the run watches (an engine breakpoint can
    /// stop in one it does not, where no line is raised to stop on).
    pub fn can_step(&self) -> bool {
        let Some(p) = &self.run.paused else { return false };
        self.state == LinkState::Paused && self.script_of(&p.module).is_some_and(|w| self.attached.contains(&w.path))
    }

    pub fn detach(&mut self) {
        self.send(Cmd::Send(Request::Detach { id: "detach".into() }));
    }

    pub fn resume(&mut self, step: Option<Step>) {
        let req = match step {
            None => Request::Continue { id: "continue".into() },
            Some(step) => Request::Step { id: "step".into(), step },
        };
        self.send(Cmd::Send(req));
    }

    /// Add or remove the engine breakpoint on binding `binding`, which must be one the host named. Attached:
    /// the host gets the new set.
    pub fn toggle_engine(&mut self, binding: &str) -> Result<(), String> {
        let adding = !self.engine.contains(binding);
        if adding {
            match &self.bindings {
                Some(Ok(names)) if names.iter().any(|n| n == binding) => {}
                Some(Ok(_)) => return Err(format!("the game has no engine binding named {binding}")),
                Some(Err(e)) => return Err(format!("the game cannot break on engine bindings: {e}")),
                None => return Err("connect to the game first: it names the engine bindings it can break on".into()),
            }
            self.engine.insert(binding.to_string());
        } else {
            self.engine.remove(binding);
        }
        let sent = match self.state {
            LinkState::Attaching => Err("the attach is still being answered: set it once it is".into()),
            LinkState::Attached | LinkState::Paused => self.breakpoints_request(&self.run_paths()).map(|r| self.send(Cmd::Send(r))),
            _ => Ok(()),
        };
        if let Err(e) = sent {
            if adding {
                self.engine.remove(binding);
            } else {
                self.engine.insert(binding.to_string());
            }
            return Err(e);
        }
        Ok(())
    }

    /// Add or remove the breakpoint on corpus line `line` of the script at `path` (set on `label`). Attached:
    /// the host gets the new set, and a script outside the run joins it (`debug.watch`), running or paused.
    pub fn toggle_breakpoint(&mut self, path: &str, line: u32, label: &str) -> Result<(), String> {
        let w = self.scripts.get_mut(path).ok_or_else(|| format!("{path} is not mapped"))?;
        let m = w.mapped.as_ref().map_err(|e| format!("no line map for {}: {e}", w.name))?;
        let adding = !w.breakpoints.contains_key(&line);
        if adding {
            lua_debug::line_breakpoints(&m.module, &m.map, line).map_err(|e| format!("{}: {e}", w.name))?;
            w.breakpoints.insert(line, label.to_string());
        } else {
            w.breakpoints.remove(&line);
        }
        let joins = adding && !self.run_paths().contains(path);
        let sent = match self.state {
            LinkState::Attached | LinkState::Paused if joins => {
                let mut paths = self.attached.clone();
                paths.insert(path.to_string());
                self.breakpoints_request(&paths).and_then(|r| self.watch_paths(paths, vec![r]))
            }
            LinkState::Attaching => Err("the attach is still being answered: set it once it is".into()),
            LinkState::Attached | LinkState::Paused => self.breakpoints_request(&self.run_paths()).map(|r| self.send(Cmd::Send(r))),
            _ => Ok(()),
        };
        if let Err(e) = sent {
            let w = self.scripts.get_mut(path).expect("found above");
            if adding {
                w.breakpoints.remove(&line);
            } else {
                w.breakpoints.insert(line, label.to_string());
            }
            return Err(e);
        }
        Ok(())
    }

    /// Take what the link sent. `true` when the run has just paused.
    pub fn poll(&mut self) -> bool {
        let Some(link) = &self.link else { return false };
        let mut msgs = Vec::new();
        while let Ok(m) = link.rx.try_recv() {
            msgs.push(m);
        }
        let mut paused_now = false;
        for m in msgs {
            match m {
                Msg::Error(e) => {
                    self.state = LinkState::Failed(e);
                    self.link = None;
                    self.attached.clear();
                    self.attaching = None;
                    self.watching = None;
                    self.bindings = None;
                    self.run.paused = None;
                }
                Msg::Frame(Frame::Feed { .. }) => {}
                Msg::Frame(Frame::Debug(ev)) => paused_now |= self.event(&ev),
            }
        }
        paused_now
    }

    fn event(&mut self, ev: &Event) -> bool {
        let maps: Vec<&Mapped> = self.scripts.values().filter_map(|w| w.mapped.as_ref().ok()).collect();
        self.run.absorb(ev, &maps);
        match ev {
            Event::Host { kind, build, attached, .. } => {
                self.host = Some((*kind, build.clone()));
                self.state = LinkState::Ready;
                if let Some(holder) = attached {
                    self.problem = Some(format!("the debugger is held by {holder}"));
                }
            }
            Event::Names { id, bindings } if id == NAMES_ID => self.bindings = Some(Ok(bindings.clone())),
            Event::Refused { id, error } if id == NAMES_ID => self.bindings = Some(Err(error.clone())),
            Event::Ack { id } if self.attaching.as_ref().is_some_and(|(a, _)| a == id) => {
                let (_, paths) = self.attaching.take().expect("checked above");
                self.attached = paths;
                self.state = LinkState::Attached;
            }
            Event::Ack { id } if self.watching.as_ref().is_some_and(|(w, _)| w == id) => {
                let (_, paths) = self.watching.take().expect("checked above");
                self.attached = paths;
            }
            Event::Refused { id, error } => {
                self.problem = Some(format!("{id}: {error}"));
                // A refusal is one of the batch in flight's requests (a line-table frame, the attach or the
                // watch, the breakpoints after it), and the link sends none of the rest: that batch is over.
                if self.attaching.take().is_some() {
                    self.state = if self.attached.is_empty() { LinkState::Ready } else { LinkState::Attached };
                }
                self.watching = None;
            }
            Event::Paused { .. } => {
                self.state = LinkState::Paused;
                return true;
            }
            Event::Resumed { .. } => self.state = LinkState::Attached,
            Event::Detached { error, .. } => {
                self.attached.clear();
                self.watching = None;
                if self.attaching.is_none() {
                    self.state = LinkState::Ready;
                }
                if let Some(e) = error {
                    self.problem = Some(format!("the host gave the debugger up: {e}"));
                }
            }
            _ => {}
        }
        false
    }
}

/// The corpus bank a Workshop corpus path names (`lua/vz/pmccon016.lua` → `vz`), when the line map
/// covers it.
pub fn bank_of(path: &str) -> Result<&str, String> {
    let mut parts = path.split('/');
    let (Some(tag), Some(bank)) = (parts.next(), parts.next()) else {
        return Err(format!("{path} names no bank"));
    };
    if tag != "lua" || !CORPUS_SOURCES.iter().any(|(_, _, b)| *b == bank) {
        let banks: Vec<&str> = CORPUS_SOURCES.iter().map(|s| s.2).collect();
        return Err(format!("the line map covers the base game's blocks ({}); {path} is not in one", banks.join(", ")));
    }
    Ok(bank)
}

/// The line map of the graph on screen: the shipped script of its bank that corpus file `name` was
/// decompiled from (`files`: the bank's corpus file names), against our compile of the graph.
pub fn map_script(shipped: &[Shipped], name: &str, script: &Script, chunk: &Graph, files: &[&str]) -> Result<Mapped, String> {
    let (_, bytecode) = blueprint::compile(script, chunk, name).map_err(|e| format!("compiling {name}: {e:?}"))?;
    let ours = mercs2_formats::luachunk::decode(&bytecode).map_err(|e| format!("our compile of {name}: {e}"))?;
    let retail = shipped_for(shipped, name, &ours.main, files)?;
    let map = LineMap::build(&retail.main, &ours.main).map_err(|e| format!("{name}: {e}"))?;
    Ok(Mapped { module: retail.module()?, map })
}

/// The corpus line a breakpoint on entry to the top-level function defined at corpus line `line_defined`
/// stops on: its first instruction's. A function that does not map, or that the host cannot tell from
/// another, has none, and the error says why.
pub fn entry_line(m: &Mapped, line_defined: u32) -> Result<u32, String> {
    let found: Vec<_> = m.map.functions.iter().filter(|f| f.path.len() == 1 && f.line_defined == line_defined).collect();
    let f = match found[..] {
        [one] => one,
        [] => return Err(format!("{} has no function defined at line {line_defined}", m.module)),
        _ => return Err(format!("{} defines {} functions at line {line_defined}", m.module, found.len())),
    };
    m.map.function(f.key).map_err(|e| e.to_string())?;
    f.lines.first().copied().ok_or_else(|| format!("{} line {line_defined}: the function has no instructions", m.module))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_formats::lua_linemap::FunctionKey;
    use mercs2_formats::luachunk::{Constant, Proto};

    fn p(line_defined: u32, last: u32, code: Vec<u32>, lines: Vec<u32>, protos: Vec<Proto>) -> Proto {
        Proto {
            source: None,
            line_defined,
            last_line_defined: last,
            nups: 0,
            num_params: 0,
            is_vararg: 0,
            max_stack: 2,
            code,
            constants: vec![Constant::String(b"Ai".to_vec())],
            protos,
            line_info: lines,
            loc_vars: vec![],
            upvalues: vec![],
        }
    }

    /// Retail: a function at retail lines 500-503; ours: the same code at corpus lines 62-64.
    fn mapped(module: &str) -> Mapped {
        let retail = p(0, 0, vec![0x24, 0x1e], vec![10, 10], vec![p(500, 503, vec![0x05, 0x1c, 0x1e], vec![501, 502, 503], vec![])]);
        let ours = p(0, 0, vec![0x24, 0x1e], vec![4, 4], vec![p(62, 64, vec![0x05, 0x1c, 0x1e], vec![63, 63, 64], vec![])]);
        Mapped { module: module.into(), map: LineMap::build(&retail, &ours).unwrap() }
    }

    fn watched(path: &str, module: &str) -> Watched {
        Watched { path: path.into(), name: module.into(), mapped: Ok(mapped(module)), breakpoints: BTreeMap::new() }
    }

    fn at(module: &str, pc: u32) -> At {
        At { module: module.into(), function: FunctionKey { line_defined: 500, last_line_defined: 503, code_len: 3 }, pc, line: Some(501 + pc), depth: 1 }
    }

    /// A debug session holding the two scripts, with the link's commands kept for the test.
    fn session() -> (Debug, Receiver<Cmd>) {
        let (tx, rx) = mpsc::channel();
        let (_msg_tx, msg_rx) = mpsc::channel();
        let mut d = Debug { link: Some(Link { tx, rx: msg_rx }), ..Debug::default() };
        d.scripts.insert("lua/vz/pmccon016.lua".into(), watched("lua/vz/pmccon016.lua", "pmccon016"));
        d.scripts.insert("lua/resident/mrxutil.lua".into(), watched("lua/resident/mrxutil.lua", "MrxUtil"));
        (d, rx)
    }

    fn sent(rx: &Receiver<Cmd>) -> Vec<Request> {
        let mut out = Vec::new();
        while let Ok(c) = rx.try_recv() {
            match c {
                Cmd::Batch(reqs) => out.extend(reqs),
                Cmd::Send(r) => out.push(r),
            }
        }
        out
    }

    #[test]
    fn line_events_count_per_module_and_a_pause_names_its_module_and_line() {
        let (a, b) = (mapped("pmccon016"), mapped("MrxUtil"));
        let mut r = Run::default();
        for (seq, pc) in [(1, 0), (2, 2), (3, 0)] {
            r.absorb(&Event::Line { seq, at: at("PmcCon016", pc) }, &[&a, &b]);
        }
        r.absorb(&Event::Line { seq: 4, at: at("mrxutil", 2) }, &[&a, &b]);
        r.absorb(&Event::Line { seq: 5, at: at("other", 0) }, &[&a, &b]);
        assert_eq!(r.hits, HashMap::from([(("pmccon016".into(), 63), 2), (("pmccon016".into(), 64), 1), (("mrxutil".into(), 64), 1)]));
        assert_eq!(r.events, 5);
        r.absorb(&Event::Paused { seq: 6, at: at("MrxUtil", 2), reason: lua_debug::PauseReason::Breakpoint }, &[&a, &b]);
        assert_eq!(r.paused, Some(PausedAt { module: "MrxUtil".into(), line: Some(64), reason: PauseReason::Breakpoint, call: None }));
        r.absorb(&Event::Resumed { seq: 7 }, &[&a, &b]);
        assert_eq!(r.paused, None);
        // A pause in a module no map holds still says where.
        r.absorb(&Event::Paused { seq: 8, at: at("other", 0), reason: lua_debug::PauseReason::Step }, &[&a]);
        assert_eq!(r.paused, Some(PausedAt { module: "other".into(), line: None, reason: PauseReason::Step, call: None }));
        // An engine breakpoint's pause carries the call it stopped before.
        let args = vec![Arg::Str { v: "Panhard (Assault)".into() }];
        r.absorb(&Event::Engine { seq: 9, at: at("other", 1), binding: "Pg.Spawn".into(), args: args.clone() }, &[&a]);
        r.absorb(&Event::Paused { seq: 10, at: at("other", 1), reason: PauseReason::Engine }, &[&a]);
        let call = r.paused.as_ref().and_then(|p| p.call.clone()).expect("the call");
        assert_eq!((call.binding.as_str(), call.args, call.at.pc), ("Pg.Spawn", args, 1));
    }

    #[test]
    fn a_position_the_map_cannot_translate_is_kept_and_not_counted() {
        let m = mapped("pmccon016");
        let mut r = Run::default();
        let mut bad = at("pmccon016", 0);
        bad.function.code_len = 9;
        r.absorb(&Event::Line { seq: 1, at: bad }, &[&m]);
        assert!(r.hits.is_empty());
        assert!(r.unmapped.as_deref().is_some_and(|e| e.contains("pmccon016: ") && e.contains("no function defined")), "{:?}", r.unmapped);
        let mut none = Run::default();
        none.absorb(&Event::Line { seq: 1, at: at("pmccon016", 0) }, &[]);
        assert!(none.hits.is_empty());
    }

    #[test]
    fn the_bank_comes_from_the_corpus_path() {
        assert_eq!(bank_of("lua/vz/pmccon016.lua"), Ok("vz"));
        assert_eq!(bank_of("lua/shell/mrxguishell.lua"), Ok("shell"));
        assert!(bank_of("dlc/dlc01/foo.lua").unwrap_err().contains("not in one"));
        assert!(bank_of("lua/level_x/foo.lua").unwrap_err().contains("resident, vz, shell"));
    }

    #[test]
    fn the_session_follows_the_host() {
        let (mut d, _rx) = session();
        d.event(&Event::Host { id: "w1".into(), protocol: PROTOCOL, kind: HostKind::Retail, build: "v11".into(), attached: None });
        assert_eq!(d.state, LinkState::Ready);
        d.open_script("lua/vz/pmccon016.lua");
        d.attach().unwrap();
        assert_eq!(d.state, LinkState::Attaching);
        d.event(&Event::Ack { id: "lines0".into() });
        assert_eq!(d.state, LinkState::Attaching, "the line tables' ack is not the attach's");
        d.event(&Event::Ack { id: "attach-pmccon016".into() });
        assert_eq!(d.state, LinkState::Attached);
        assert_eq!(d.attached, BTreeSet::from(["lua/vz/pmccon016.lua".to_string()]));
        assert!(d.event(&Event::Paused { seq: 1, at: at("pmccon016", 0), reason: lua_debug::PauseReason::Breakpoint }));
        assert_eq!(d.state, LinkState::Paused);
        assert_eq!(d.line_run("lua/vz/pmccon016.lua", 63), LineRun { hits: 0, paused: true, breakpoint: false });
        assert!(!d.line_run("lua/resident/mrxutil.lua", 63).paused, "the pause is in the other script");
        assert!(d.paused(), "the rail shows the pause");
        d.event(&Event::Resumed { seq: 2 });
        assert!(!d.paused());
        d.event(&Event::Detached { seq: 3, error: Some("client 3 could not take event 9".into()) });
        assert_eq!(d.state, LinkState::Ready);
        assert!(d.attached.is_empty());
        assert!(d.problem.as_deref().is_some_and(|p| p.contains("gave the debugger up")));
    }

    #[test]
    fn an_attach_watches_the_open_script_and_every_script_with_a_breakpoint() {
        let (mut d, rx) = session();
        d.state = LinkState::Ready;
        d.open_script("lua/vz/pmccon016.lua");
        d.toggle_breakpoint("lua/resident/mrxutil.lua", 63, "SpawnObject() entry").unwrap();
        d.attach().unwrap();
        let reqs = sent(&rx);
        let Some(Request::Attach { modules, breakpoints, .. }) = reqs.last() else { panic!("{reqs:?}") };
        assert_eq!(modules, &vec!["MrxUtil".to_string(), "pmccon016".to_string()]);
        assert_eq!(breakpoints.iter().map(|b| (b.module.as_str(), b.pc)).collect::<Vec<_>>(), vec![("MrxUtil", 0), ("MrxUtil", 1)]);
        let tables: Vec<&str> = reqs
            .iter()
            .flat_map(|r| match r {
                Request::Lines { lines, .. } => lines.iter().map(|t| t.module.as_str()).collect(),
                _ => Vec::new(),
            })
            .collect();
        assert!(tables.contains(&"MrxUtil") && tables.contains(&"pmccon016"), "every watched script's lines go first: {tables:?}");
    }

    #[test]
    fn a_script_with_no_map_refuses_the_attach_by_name() {
        let (mut d, rx) = session();
        d.state = LinkState::Ready;
        d.scripts.get_mut("lua/resident/mrxutil.lua").unwrap().breakpoints.insert(63, "x".into());
        d.scripts.get_mut("lua/resident/mrxutil.lua").unwrap().mapped = Err("function [4] differs".into());
        d.open_script("lua/vz/pmccon016.lua");
        let e = d.attach().unwrap_err();
        assert!(e.contains("MrxUtil: function [4] differs"), "{e}");
        assert!(sent(&rx).is_empty(), "nothing is sent");
        assert_eq!(d.state, LinkState::Ready);
    }

    #[test]
    fn a_breakpoint_in_a_new_script_joins_a_paused_run_through_a_watch() {
        let (mut d, rx) = session();
        d.state = LinkState::Attached;
        d.attached = BTreeSet::from(["lua/vz/pmccon016.lua".to_string()]);
        // In the run already: only the breakpoint set is sent.
        d.toggle_breakpoint("lua/vz/pmccon016.lua", 63, "GetVehicles()").unwrap();
        assert!(matches!(&sent(&rx)[..], [Request::Breakpoints { breakpoints, .. }] if breakpoints.len() == 2));

        // A new script while paused: its tables only, the watch naming both, then the breakpoints; the pause holds.
        d.state = LinkState::Paused;
        d.run.events = 7;
        d.toggle_breakpoint("lua/resident/mrxutil.lua", 63, "SpawnObject() entry").unwrap();
        let reqs = sent(&rx);
        let tables: Vec<&str> = reqs
            .iter()
            .flat_map(|r| match r {
                Request::Lines { lines, .. } => lines.iter().map(|t| t.module.as_str()).collect(),
                _ => Vec::new(),
            })
            .collect();
        assert!(!tables.is_empty() && tables.iter().all(|m| *m == "MrxUtil"), "only the joining script's tables: {tables:?}");
        let [.., Request::Watch { modules, .. }, Request::Breakpoints { breakpoints, .. }] = &reqs[..] else { panic!("{reqs:?}") };
        assert_eq!(modules, &vec!["MrxUtil".to_string(), "pmccon016".to_string()]);
        assert_eq!(breakpoints.len(), 4);
        assert_eq!((d.state.clone(), d.run.events), (LinkState::Paused, 7));
        assert!(!d.attached.contains("lua/resident/mrxutil.lua"), "watched once the host answers");
        d.event(&Event::Ack { id: "watch-MrxUtil-pmccon016".into() });
        assert_eq!(d.attached.len(), 2);

        // Before a watch is answered, a breakpoint change still carries the joining script's breakpoints.
        d.attached = BTreeSet::from(["lua/vz/pmccon016.lua".to_string()]);
        d.scripts.get_mut("lua/resident/mrxutil.lua").unwrap().breakpoints.clear();
        d.toggle_breakpoint("lua/resident/mrxutil.lua", 63, "SpawnObject() entry").unwrap();
        sent(&rx);
        d.toggle_breakpoint("lua/vz/pmccon016.lua", 63, "GetVehicles()").unwrap();
        let reqs = sent(&rx);
        let [Request::Breakpoints { breakpoints, .. }] = &reqs[..] else { panic!("{reqs:?}") };
        assert!(breakpoints.iter().any(|b| b.module == "MrxUtil"), "the joining script's breakpoints stay: {breakpoints:?}");
        d.event(&Event::Ack { id: "watch-MrxUtil-pmccon016".into() });

        // A refused watch leaves the run watching what it did.
        d.open_script("lua/resident/mrxutil.lua");
        d.attached = BTreeSet::from(["lua/vz/pmccon016.lua".to_string()]);
        d.add_open().unwrap();
        assert!(d.add_open().unwrap_err().contains("still being answered"));
        d.event(&Event::Refused { id: "wlines0".into(), error: "x".into() });
        assert_eq!((d.state.clone(), d.attached.len()), (LinkState::Paused, 1));
        assert!(d.add_open().is_ok(), "the refusal ended that watch");
    }

    #[test]
    fn engine_breakpoints_name_the_hosts_bindings_and_a_pause_outside_the_run_only_continues() {
        let (mut d, rx) = session();
        assert!(d.toggle_engine("Pg.Spawn").unwrap_err().contains("connect to the game first"));
        d.event(&Event::Names { id: NAMES_ID.into(), bindings: vec!["Object.GetYaw".into(), "Pg.Spawn".into()] });
        assert!(d.toggle_engine("Pg.Spwan").unwrap_err().contains("no engine binding named Pg.Spwan"));
        d.toggle_engine("Pg.Spawn").unwrap();
        assert!(sent(&rx).is_empty(), "nothing to send before an attach");

        // An attach with only an engine breakpoint and no script.
        d.state = LinkState::Ready;
        d.attach().unwrap();
        let reqs = sent(&rx);
        let [Request::Attach { modules, engine, .. }] = &reqs[..] else { panic!("{reqs:?}") };
        assert!(modules.is_empty());
        assert_eq!(engine, &vec!["Pg.Spawn".to_string()]);
        d.event(&Event::Ack { id: "attach-".into() });
        assert_eq!(d.state, LinkState::Attached);

        // Changing the set while attached sends it with the line breakpoints.
        d.toggle_engine("Object.GetYaw").unwrap();
        assert!(matches!(&sent(&rx)[..], [Request::Breakpoints { engine, .. }] if engine.len() == 2));

        // Paused before a call from a script the run does not watch: only Continue.
        d.event(&Event::Engine { seq: 1, at: at("mrxutil", 2), binding: "Pg.Spawn".into(), args: vec![] });
        d.event(&Event::Paused { seq: 2, at: at("mrxutil", 2), reason: PauseReason::Engine });
        assert!(d.paused() && !d.can_step());
        d.attached.insert("lua/resident/mrxutil.lua".into());
        assert!(d.can_step(), "once the script is watched a step has lines to stop on");

        // A host that cannot name its bindings says why.
        d.event(&Event::Refused { id: NAMES_ID.into(), error: "no luaL_Reg tables".into() });
        assert_eq!(d.bindings, Some(Err("no luaL_Reg tables".into())));
        assert_eq!(d.state, LinkState::Paused, "the names' refusal is not the run's");
    }

    #[test]
    fn a_refused_attach_leaves_the_run_as_it_was() {
        let (mut d, _rx) = session();
        d.state = LinkState::Ready;
        d.open_script("lua/vz/pmccon016.lua");
        d.attach().unwrap();
        d.event(&Event::Refused { id: "attach-pmccon016".into(), error: "the debugger is held by x".into() });
        assert_eq!(d.state, LinkState::Ready);
        assert!(d.problem.as_deref().is_some_and(|p| p.contains("held by x")));

        // A refused line-table frame ends the attach too: the link sends nothing after it.
        d.attach().unwrap();
        d.event(&Event::Refused { id: "lines0".into(), error: "a table does not fit its function".into() });
        assert_eq!(d.state, LinkState::Ready);
    }

    #[test]
    fn breakpoints_become_every_instruction_on_their_line() {
        let (mut d, _rx) = session();
        d.toggle_breakpoint("lua/vz/pmccon016.lua", 63, "f()").unwrap();
        let paths = BTreeSet::from(["lua/vz/pmccon016.lua".to_string()]);
        assert_eq!(d.breakpoint_positions(&paths).unwrap().iter().map(|b| b.pc).collect::<Vec<_>>(), vec![0, 1]);
        let e = d.toggle_breakpoint("lua/vz/pmccon016.lua", 99, "f()").unwrap_err();
        assert!(e.contains("99"), "{e}");
        assert!(!d.scripts["lua/vz/pmccon016.lua"].breakpoints.contains_key(&99));
    }

    #[test]
    fn a_function_breaks_on_entry_at_its_first_instructions_line() {
        let m = mapped("MrxUtil");
        assert_eq!(entry_line(&m, 62), Ok(63));
        assert!(entry_line(&m, 70).unwrap_err().contains("no function defined at line 70"));
    }
}
