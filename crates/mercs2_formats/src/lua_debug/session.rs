//! The debugger every host runs: the one-holder rule, the request handling, the module filter, breakpoints,
//! steps and event numbering. A host feeds it what its VM did and carries out the [`Effect`]s it returns;
//! nothing here touches a VM, so the three hosts (m2-sdk.dll in the retail game, `mercs2_game`,
//! `mercs2_autoplay`) decide every case the same way.
//!
//! Requests are handled on the game thread: while the game runs, from the host's tick; while it is paused,
//! from the pause loop. `from` names the client connection a request came on, so only the holder can drive
//! the run.

use super::{
    require_protocol, At, Breakpoint, Event, FunctionKey, HostKind, LineTable, PauseReason, Request, Step, PROTOCOL,
};
use std::collections::{BTreeSet, HashMap};

/// What the host must do after a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Nothing beyond sending the events.
    None,
    /// A client attached: install the hook, then report each filtered module already loaded
    /// ([`Debugger::module`]) and the stack ([`Debugger::stack`]).
    Install,
    /// The module filter changed (`debug.watch`): decide again, for each hooked state's running frame,
    /// whether it raises line events, and report each newly filtered module already loaded.
    Refilter,
    /// The paused game thread resumes ([`Debugger::resumed`] is sent by the host when it does).
    Resume,
    /// The client detached: remove the hook (and resume, if paused).
    Remove,
}

/// The events to send and what the host does.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    /// Each event goes to the client the request came from.
    pub events: Vec<Event>,
    pub effect: Effect,
}

impl Reply {
    fn answer(ev: Event) -> Reply {
        Reply { events: vec![ev], effect: Effect::None }
    }
}

/// One attached client.
#[derive(Clone, Debug)]
struct Session {
    from: u64,
    client: String,
    /// Module names, ASCII-lowercased.
    modules: Vec<String>,
    breakpoints: Vec<Breakpoint>,
    /// The engine bindings whose calls pause the run.
    engine: BTreeSet<String>,
    seq: u64,
    /// Where the run is paused.
    paused: Option<Paused>,
    /// The step in progress, and the depth it started from.
    step: Option<(Step, u32)>,
    /// The stamp last reported for each module (lowercased name).
    stamps: HashMap<String, String>,
    /// Corpus line per instruction, by (lowercased module, function).
    tables: HashMap<(String, FunctionKey), Vec<u32>>,
    /// The function each call depth last ran an instruction of, and that instruction.
    frames: Vec<Option<Frame>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Frame {
    module: String,
    function: FunctionKey,
    last_pc: u32,
}

/// A pause: its call depth, and its module, which a step needs filtered (an engine breakpoint can pause in
/// a module the attach does not filter, where no line event is raised to stop on).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Paused {
    depth: u32,
    module: String,
}

/// The tables by function; an error names a table that does not fit its function or its filter, or two
/// tables of one function that disagree.
fn index_tables(modules: &[String], lines: Vec<LineTable>) -> Result<HashMap<(String, FunctionKey), Vec<u32>>, String> {
    let mut out: HashMap<(String, FunctionKey), Vec<u32>> = HashMap::new();
    for t in lines {
        let module = t.module.to_ascii_lowercase();
        if !modules.contains(&module) {
            return Err(format!("a line table is for {}, which attach does not name", t.module));
        }
        if t.lines.len() != t.function.code_len as usize {
            return Err(format!(
                "the line table of {} {:?} has {} lines for {} instructions",
                t.module,
                t.function,
                t.lines.len(),
                t.function.code_len
            ));
        }
        match out.get(&(module.clone(), t.function)) {
            Some(known) if *known != t.lines => {
                return Err(format!("{} {:?} has two line tables that differ", t.module, t.function));
            }
            Some(_) => {}
            None => {
                out.insert((module, t.function), t.lines);
            }
        }
    }
    Ok(out)
}

impl Session {
    fn next(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

/// A host's debugger: idle, or held by one client.
#[derive(Clone, Debug)]
pub struct Debugger {
    kind: HostKind,
    build: String,
    session: Option<Session>,
    /// Line tables each connection has staged (`debug.lines`) for its next attach or watch.
    staged: HashMap<u64, Vec<LineTable>>,
    /// The engine binding names the host can break on, or why it has none; `None` until the host says.
    bindings: Option<Result<BTreeSet<String>, String>>,
}

impl Debugger {
    pub fn new(kind: HostKind, build: impl Into<String>) -> Debugger {
        Debugger { kind, build: build.into(), session: None, staged: HashMap::new(), bindings: None }
    }

    /// The engine binding names the host can break on, or why it cannot name them.
    pub fn set_bindings(&mut self, bindings: Result<BTreeSet<String>, String>) {
        self.bindings = Some(bindings);
    }

    /// `engine` as a set, or why one of its names cannot be broken on.
    fn engine_set(&self, engine: Vec<String>) -> Result<BTreeSet<String>, String> {
        if engine.is_empty() {
            return Ok(BTreeSet::new());
        }
        let known = match &self.bindings {
            Some(Ok(k)) => k,
            Some(Err(e)) => return Err(format!("this host cannot break on engine bindings: {e}")),
            None => return Err("this host has not named its engine bindings".into()),
        };
        let unknown: Vec<&str> = engine.iter().filter(|b| !known.contains(*b)).map(String::as_str).collect();
        if !unknown.is_empty() {
            return Err(format!("no engine binding is named {} (debug.bindings lists them)", unknown.join(", ")));
        }
        Ok(engine.into_iter().collect())
    }

    /// Whether a call of engine binding `binding` pauses the run.
    pub fn breaks_on(&self, binding: &str) -> bool {
        self.session.as_ref().is_some_and(|s| s.engine.contains(binding))
    }

    pub fn attached(&self) -> bool {
        self.session.is_some()
    }

    /// The client connection holding the debugger: the only one its run events go to.
    pub fn holder(&self) -> Option<u64> {
        self.session.as_ref().map(|s| s.from)
    }

    pub fn paused(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.paused.is_some())
    }

    /// Whether `function` of `module` raises line events: it is filtered and has a line table. Only such a
    /// function needs its instructions reported.
    pub fn has_lines(&self, module: &str, function: FunctionKey) -> bool {
        self.session.as_ref().is_some_and(|s| s.tables.contains_key(&(module.to_ascii_lowercase(), function)))
    }

    /// Whether events in `module` are reported. Hosts ask before reading anything else about a position.
    pub fn filtered(&self, module: &str) -> bool {
        self.session.as_ref().is_some_and(|s| s.modules.iter().any(|m| m.eq_ignore_ascii_case(module)))
    }

    /// Handle a request from connection `from`.
    pub fn request(&mut self, from: u64, req: Request) -> Reply {
        let id = req.id().to_string();
        let refuse = |error: String| Reply::answer(Event::Refused { id: id.clone(), error });
        match req {
            Request::Hello { protocol, .. } => match require_protocol(protocol) {
                Err(e) => refuse(e.to_string()),
                Ok(()) => Reply::answer(Event::Host {
                    id,
                    protocol: PROTOCOL,
                    kind: self.kind,
                    build: self.build.clone(),
                    attached: self.session.as_ref().map(|s| s.client.clone()),
                }),
            },
            Request::Bindings { .. } => match &self.bindings {
                Some(Ok(names)) => Reply::answer(Event::Names { id, bindings: names.iter().cloned().collect() }),
                Some(Err(e)) => refuse(format!("this host cannot name its engine bindings: {e}")),
                None => refuse("this host has not named its engine bindings".into()),
            },
            Request::Lines { lines, .. } => {
                self.staged.entry(from).or_default().extend(lines);
                Reply::answer(Event::Ack { id })
            }
            Request::Attach { protocol, client, modules, breakpoints, engine, .. } => {
                // Consumed whatever the answer: a refused attach's tables are not carried to the next.
                let lines = self.staged.remove(&from).unwrap_or_default();
                if let Err(e) = require_protocol(protocol) {
                    return refuse(e.to_string());
                }
                if let Some(s) = &self.session {
                    return refuse(format!("the debugger is held by {}; detach it first", s.client));
                }
                if modules.is_empty() && engine.is_empty() {
                    return refuse("attach names no module and no engine breakpoint".into());
                }
                if modules.iter().any(|m| m.is_empty()) {
                    return refuse("attach names an empty module name".into());
                }
                let engine = match self.engine_set(engine) {
                    Ok(e) => e,
                    Err(e) => return refuse(e),
                };
                let modules: Vec<String> = modules.iter().map(|m| m.to_ascii_lowercase()).collect();
                let tables = match index_tables(&modules, lines) {
                    Ok(t) => t,
                    Err(e) => return refuse(e),
                };
                self.session = Some(Session {
                    from,
                    client,
                    modules,
                    breakpoints,
                    engine,
                    seq: 0,
                    paused: None,
                    step: None,
                    stamps: HashMap::new(),
                    tables,
                    frames: Vec::new(),
                });
                Reply { events: vec![Event::Ack { id }], effect: Effect::Install }
            }
            Request::Breakpoints { breakpoints, engine, .. } => {
                if let Err(e) = self.held_by(from) {
                    return refuse(e);
                }
                let engine = match self.engine_set(engine) {
                    Ok(e) => e,
                    Err(e) => return refuse(e),
                };
                let s = self.session.as_mut().expect("held");
                s.breakpoints = breakpoints;
                s.engine = engine;
                Reply::answer(Event::Ack { id })
            }
            Request::Watch { modules, .. } => {
                // Consumed whatever the answer, as an attach consumes them.
                let lines = self.staged.remove(&from).unwrap_or_default();
                if let Err(e) = self.held_by(from) {
                    return refuse(e);
                }
                if modules.iter().any(|m| m.is_empty()) {
                    return refuse("watch names an empty module name".into());
                }
                let s = self.session.as_mut().expect("held");
                if modules.is_empty() && s.engine.is_empty() {
                    return refuse("watch names no module, and the attach has no engine breakpoint".into());
                }
                let modules: Vec<String> = modules.iter().map(|m| m.to_ascii_lowercase()).collect();
                let staged = match index_tables(&modules, lines) {
                    Ok(t) => t,
                    Err(e) => return refuse(e),
                };
                let mut tables: HashMap<(String, FunctionKey), Vec<u32>> =
                    s.tables.iter().filter(|((m, _), _)| modules.contains(m)).map(|(k, v)| (k.clone(), v.clone())).collect();
                for (key, lines) in staged {
                    match tables.get(&key) {
                        Some(known) if *known != lines => {
                            return refuse(format!("{} {:?} has a line table that differs from the one attached", key.0, key.1))
                        }
                        Some(_) => {}
                        None => {
                            tables.insert(key, lines);
                        }
                    }
                }
                // A module no longer filtered is reported again (with its stamp) if it is filtered again.
                s.stamps.retain(|m, _| modules.contains(m));
                s.modules = modules;
                s.tables = tables;
                Reply { events: vec![Event::Ack { id }], effect: Effect::Refilter }
            }
            req => {
                if let Err(e) = self.held_by(from) {
                    return refuse(e);
                }
                let s = self.session.as_mut().expect("held");
                match req {
                    Request::Continue { .. } | Request::Step { .. } if s.paused.is_none() => {
                        refuse("the run is not paused".into())
                    }
                    Request::Continue { .. } => {
                        s.step = None;
                        Reply { events: vec![Event::Ack { id }], effect: Effect::Resume }
                    }
                    Request::Step { step, .. } => {
                        let p = s.paused.clone().expect("checked above");
                        if !s.modules.contains(&p.module.to_ascii_lowercase()) {
                            return refuse(format!(
                                "the run is paused in {}, which this attach does not watch, so a step has no line to \
                                 stop at: Continue, or watch {}",
                                p.module, p.module
                            ));
                        }
                        s.step = Some((step, p.depth));
                        Reply { events: vec![Event::Ack { id }], effect: Effect::Resume }
                    }
                    Request::Detach { .. } => {
                        let seq = s.next();
                        self.session = None;
                        Reply { events: vec![Event::Ack { id }, Event::Detached { seq, error: None }], effect: Effect::Remove }
                    }
                    Request::Hello { .. }
                    | Request::Bindings { .. }
                    | Request::Lines { .. }
                    | Request::Attach { .. }
                    | Request::Breakpoints { .. }
                    | Request::Watch { .. } => {
                        unreachable!("handled above")
                    }
                }
            }
        }
    }

    /// `Ok` when connection `from` holds the debugger, else why it cannot drive it.
    fn held_by(&self, from: u64) -> Result<(), String> {
        match &self.session {
            None => Err("no client is attached".into()),
            Some(s) if s.from != from => Err(format!("the debugger is held by {}", s.client)),
            Some(_) => Ok(()),
        }
    }

    /// A connection closed: what it staged is dropped, and if it held the debugger, the debugger is free
    /// again and the hook must go. Nothing can be sent. `false` when `from` did not hold it.
    pub fn disconnected(&mut self, from: u64) -> bool {
        self.staged.remove(&from);
        if self.holder() == Some(from) {
            self.session = None;
            true
        } else {
            false
        }
    }

    /// The host gives the debugger up (an event could not be delivered, the state closed): the last event,
    /// which the host sends if it still can, then remove the hook.
    pub fn fail(&mut self, error: String) -> Option<Event> {
        let mut s = self.session.take()?;
        Some(Event::Detached { seq: s.next(), error: Some(error) })
    }

    /// A module the host has loaded, with its stamp: reported once per stamp while attached and filtered.
    pub fn module(&mut self, module: &str, stamp: &str) -> Option<Event> {
        if !self.filtered(module) {
            return None;
        }
        let s = self.session.as_mut().expect("filtered implies attached");
        let key = module.to_ascii_lowercase();
        if s.stamps.get(&key).is_some_and(|known| known == stamp) {
            return None;
        }
        s.stamps.insert(key, stamp.to_string());
        Some(Event::Module { seq: s.next(), module: module.to_string(), stamp: stamp.to_string() })
    }

    /// The Lua frames on the stack at attach, base first (every frame: the client keeps those in its
    /// modules).
    pub fn stack(&mut self, frames: Vec<At>) -> Option<Event> {
        let s = self.session.as_mut()?;
        Some(Event::Stack { seq: s.next(), frames })
    }

    fn event(&mut self, at: &At, make: impl FnOnce(u64) -> Event) -> Option<Event> {
        if !self.filtered(&at.module) {
            return None;
        }
        let s = self.session.as_mut().expect("filtered implies attached");
        Some(make(s.next()))
    }

    pub fn entry(&mut self, at: At) -> Option<Event> {
        self.event(&at.clone(), |seq| Event::Entry { seq, at })
    }

    pub fn ret(&mut self, at: At) -> Option<Event> {
        self.event(&at.clone(), |seq| Event::Return { seq, at })
    }

    /// An instruction a function of a filtered module is about to execute. When it starts a corpus line by
    /// the VM's line-hook rule (the function's first instruction, a jump backwards, or a different line
    /// from the frame's previous instruction): the line event, and whether the run pauses there (a
    /// breakpoint, else a finished step). A function with no line table raises nothing. An error is a
    /// position the function cannot hold; the host gives the debugger up with it ([`Debugger::fail`]).
    pub fn instruction(&mut self, at: At) -> Result<(Option<Event>, Option<PauseReason>), String> {
        if !self.filtered(&at.module) {
            return Ok((None, None));
        }
        let s = self.session.as_mut().expect("filtered implies attached");
        let module = at.module.to_ascii_lowercase();
        let Some(table) = s.tables.get(&(module.clone(), at.function)) else { return Ok((None, None)) };
        let Some(&line) = table.get(at.pc as usize) else {
            return Err(format!("{} {:?} has no instruction {}", at.module, at.function, at.pc));
        };
        let depth = at.depth as usize;
        s.frames.truncate(depth + 1);
        if s.frames.len() <= depth {
            s.frames.resize(depth + 1, None);
        }
        let starts = match &s.frames[depth] {
            Some(f) if f.module == module && f.function == at.function => {
                at.pc == 0 || at.pc <= f.last_pc || table[f.last_pc as usize] != line
            }
            _ => true,
        };
        s.frames[depth] = Some(Frame { module, function: at.function, last_pc: at.pc });
        if !starts {
            return Ok((None, None));
        }
        let ev = Event::Line { seq: s.next(), at: at.clone() };
        let hit = s.breakpoints.iter().any(|b| b.module.eq_ignore_ascii_case(&at.module) && b.function == at.function && b.pc == at.pc);
        let stepped = match s.step {
            None => false,
            Some((Step::Into, _)) => true,
            Some((Step::Over, from)) => at.depth <= from,
            Some((Step::Out, from)) => at.depth < from,
        };
        let reason = if hit {
            Some(PauseReason::Breakpoint)
        } else if stepped {
            Some(PauseReason::Step)
        } else {
            None
        };
        Ok((Some(ev), reason))
    }

    /// The run pauses at `at` (after [`Debugger::line`] said so): the event, sent before the host blocks.
    pub fn pause(&mut self, at: At, reason: PauseReason) -> Option<Event> {
        let s = self.session.as_mut()?;
        s.paused = Some(Paused { depth: at.depth, module: at.module.clone() });
        s.step = None;
        Some(Event::Paused { seq: s.next(), at, reason })
    }

    /// The paused game thread runs again; `None` when the client detached while paused.
    pub fn resumed(&mut self) -> Option<Event> {
        let s = self.session.as_mut()?;
        s.paused = None;
        Some(Event::Resumed { seq: s.next() })
    }

    /// Code at `at` called engine binding `binding`: the event (whose `seq` an `engine_return` names) when
    /// `at` is in a filtered module or the binding has an engine breakpoint, and in that second case the
    /// pause, which the host makes ([`Debugger::pause`]) before the binding runs. A host asks
    /// [`Debugger::filtered`] or [`Debugger::breaks_on`] before reading the call's position and arguments.
    pub fn engine(&mut self, at: At, binding: String, args: Vec<super::Arg>) -> (Option<Event>, Option<PauseReason>) {
        let breaks = self.breaks_on(&binding);
        if !breaks && !self.filtered(&at.module) {
            return (None, None);
        }
        let s = self.session.as_mut().expect("filtered or breaking implies attached");
        (Some(Event::Engine { seq: s.next(), at, binding, args }), breaks.then_some(PauseReason::Engine))
    }

    /// The engine call recorded as `call` returned `values`.
    pub fn engine_return(&mut self, call: u64, values: Vec<super::Arg>) -> Option<Event> {
        let s = self.session.as_mut()?;
        Some(Event::EngineReturn { seq: s.next(), call, values })
    }

    pub fn stub(&mut self, at: At, path: String) -> Option<Event> {
        self.event(&at.clone(), |seq| Event::Stub { seq, at, path })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FunctionKey, Sequence};
    use super::*;

    const A: u64 = 1;
    const B: u64 = 2;

    fn key() -> FunctionKey {
        FunctionKey { line_defined: 10, last_line_defined: 40, code_len: 52 }
    }

    fn at(module: &str, pc: u32, depth: u32) -> At {
        At { module: module.into(), function: key(), pc, line: Some(12), depth }
    }

    fn stage(d: &mut Debugger, from: u64, lines: Vec<LineTable>) {
        let r = d.request(from, Request::Lines { id: "l".into(), lines });
        assert!(matches!(&r.events[..], [Event::Ack { .. }]), "{r:?}");
    }

    fn attach(d: &mut Debugger, from: u64, bps: Vec<Breakpoint>) -> Reply {
        // Each instruction on its own corpus line, so every instruction starts one.
        stage(d, from, vec![LineTable { module: "gurcon002".into(), function: key(), lines: (100..152).collect() }]);
        d.request(
            from,
            Request::Attach {
                id: "a".into(),
                protocol: PROTOCOL,
                client: format!("client {from}"),
                modules: vec!["GurCon002".into()],
                breakpoints: bps,
                engine: vec![],
            },
        )
    }

    fn line(d: &mut Debugger, at: At) -> (Option<Event>, Option<PauseReason>) {
        d.instruction(at).unwrap()
    }

    fn attach_only(d: &mut Debugger) -> Reply {
        let attach = Request::Attach {
            id: "a".into(),
            protocol: PROTOCOL,
            client: "c".into(),
            modules: vec!["gurcon002".into()],
            breakpoints: Vec::new(),
            engine: Vec::new(),
        };
        d.request(A, attach)
    }

    fn attach_lines(d: &mut Debugger, lines: Vec<LineTable>) -> Reply {
        stage(d, A, lines);
        attach_only(d)
    }

    fn small() -> FunctionKey {
        FunctionKey { line_defined: 1, last_line_defined: 9, code_len: 6 }
    }

    fn at_small(pc: u32, depth: u32) -> At {
        At { module: "gurcon002".into(), function: small(), pc, line: None, depth }
    }

    #[test]
    fn a_line_starts_where_the_corpus_line_changes_or_the_code_jumps_back() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach_lines(&mut d, vec![LineTable { module: "gurcon002".into(), function: small(), lines: vec![2, 2, 3, 3, 3, 4] }]);
        let starts = |d: &mut Debugger, pc, depth| line(d, at_small(pc, depth)).0.is_some();
        assert!(starts(&mut d, 0, 1), "the first instruction");
        assert!(!starts(&mut d, 1, 1), "same line");
        assert!(starts(&mut d, 2, 1), "line 2 -> 3");
        assert!(!starts(&mut d, 3, 1));
        assert!(starts(&mut d, 2, 1), "a jump back, on the same line");
        assert!(!starts(&mut d, 4, 1), "a jump forward within the line");
        assert!(starts(&mut d, 5, 1), "line 3 -> 4");
        assert!(starts(&mut d, 3, 2), "a frame never seen at this depth");
        assert!(!starts(&mut d, 4, 2));
        assert!(starts(&mut d, 4, 1), "back at depth 1, pc 4 after pc 5 is a jump back");
        assert!(starts(&mut d, 3, 2), "depth 2 was forgotten when depth 1 ran");
    }

    #[test]
    fn a_function_without_a_table_raises_no_line() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach_lines(&mut d, Vec::new());
        assert_eq!(line(&mut d, at_small(0, 1)), (None, None));
        assert!(d.entry(at_small(0, 1)).is_some(), "entry and return still report");
    }

    #[test]
    fn an_instruction_past_the_table_gives_the_debugger_up() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach_lines(&mut d, vec![LineTable { module: "gurcon002".into(), function: small(), lines: vec![1; 6] }]);
        let e = d.instruction(at_small(6, 1)).unwrap_err();
        assert!(e.contains("no instruction 6"), "{e}");
    }

    #[test]
    fn line_tables_must_fit_their_function_and_filter() {
        let t = |module: &str, lines: Vec<u32>| LineTable { module: module.into(), function: small(), lines };
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        assert!(refused(&attach_lines(&mut d, vec![t("gurcon002", vec![1; 5])])).contains("5 lines for 6 instructions"));
        assert!(refused(&attach_lines(&mut d, vec![t("other", vec![1; 6])])).contains("does not name"));
        assert!(refused(&attach_lines(&mut d, vec![t("gurcon002", vec![1; 6]), t("GURCON002", vec![2; 6])])).contains("differ"));
        assert!(!d.attached());
        assert!(matches!(&attach_lines(&mut d, vec![t("gurcon002", vec![1; 6]), t("GURCON002", vec![1; 6])]).events[..], [Event::Ack { .. }]));
    }

    #[test]
    fn staged_tables_add_up_and_the_next_attach_consumes_them() {
        let t = |lines: Vec<u32>| LineTable { module: "gurcon002".into(), function: small(), lines };
        let other = FunctionKey { line_defined: 20, last_line_defined: 30, code_len: 2 };
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        // Two frames' tables make one attach's.
        stage(&mut d, A, vec![t(vec![1; 6])]);
        stage(&mut d, A, vec![LineTable { module: "gurcon002".into(), function: other, lines: vec![21, 22] }]);
        assert!(matches!(&attach_only(&mut d).events[..], [Event::Ack { .. }]));
        assert!(d.has_lines("gurcon002", small()) && d.has_lines("gurcon002", other));
        d.request(A, Request::Detach { id: "d".into() });

        // Consumed by that attach: the next has none.
        attach_only(&mut d);
        assert!(!d.has_lines("gurcon002", small()));
        d.request(A, Request::Detach { id: "d".into() });

        // A refused attach consumes them too; so does a disconnect. Another client's stay its own.
        stage(&mut d, A, vec![t(vec![1; 5])]);
        assert!(refused(&attach_only(&mut d)).contains("5 lines"));
        assert!(matches!(&attach_only(&mut d).events[..], [Event::Ack { .. }]));
        d.request(A, Request::Detach { id: "d".into() });
        stage(&mut d, A, vec![t(vec![1; 5])]);
        stage(&mut d, B, vec![t(vec![2; 6])]);
        assert!(!d.disconnected(A));
        assert!(matches!(&attach_only(&mut d).events[..], [Event::Ack { .. }]), "A's bad table went with its connection");
        d.request(A, Request::Detach { id: "d".into() });
        let b = Request::Attach {
            id: "b".into(),
            protocol: PROTOCOL,
            client: "b".into(),
            modules: vec!["gurcon002".into()],
            breakpoints: Vec::new(),
            engine: Vec::new(),
        };
        assert!(matches!(&d.request(B, b).events[..], [Event::Ack { .. }]));
        assert!(d.has_lines("gurcon002", small()));
    }

    fn bp(pc: u32) -> Breakpoint {
        Breakpoint { module: "gurcon002".into(), function: key(), pc }
    }

    fn refused(r: &Reply) -> &str {
        match &r.events[..] {
            [Event::Refused { error, .. }] => error,
            other => panic!("not a refusal: {other:?}"),
        }
    }

    #[test]
    fn hello_names_the_host_and_its_holder() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        let hello = |d: &mut Debugger| d.request(B, Request::Hello { id: "h".into(), protocol: PROTOCOL });
        assert!(matches!(&hello(&mut d).events[..], [Event::Host { attached: None, kind: HostKind::Retail, .. }]));
        attach(&mut d, A, Vec::new());
        assert!(matches!(&hello(&mut d).events[..], [Event::Host { attached: Some(c), .. }] if c == "client 1"));
        let r = d.request(B, Request::Hello { id: "h".into(), protocol: PROTOCOL + 1 });
        assert!(refused(&r).contains("protocol"));
    }

    #[test]
    fn one_client_holds_the_debugger() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        assert_eq!(attach(&mut d, A, Vec::new()).effect, Effect::Install);
        assert!(refused(&attach(&mut d, B, Vec::new())).contains("held by client 1"));
        let r = d.request(B, Request::Detach { id: "d".into() });
        assert!(refused(&r).contains("held by client 1"));
        assert!(!d.disconnected(B));
        assert!(d.disconnected(A));
        assert_eq!(attach(&mut d, B, Vec::new()).effect, Effect::Install);
    }

    #[test]
    fn attach_needs_modules_and_the_protocol() {
        let mut d = Debugger::new(HostKind::Autoplay, "1.0");
        let r = d.request(
            A,
            Request::Attach { id: "a".into(), protocol: PROTOCOL, client: "c".into(), modules: vec![], breakpoints: vec![], engine: vec![] },
        );
        assert!(refused(&r).contains("no module and no engine breakpoint"));
        let r = d.request(
            A,
            Request::Attach { id: "a".into(), protocol: 99, client: "c".into(), modules: vec!["m".into()], breakpoints: vec![], engine: vec![] },
        );
        assert!(refused(&r).contains("protocol 99"));
        assert!(!d.attached());
    }

    #[test]
    fn only_filtered_modules_report() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        assert_eq!(d.entry(at("gurcon002", 0, 1)), None, "nothing before attach");
        attach(&mut d, A, Vec::new());
        assert!(d.entry(at("GURCON002", 0, 1)).is_some(), "names compare case-insensitively");
        assert_eq!(d.entry(at("mrxmissionflow", 0, 1)), None);
        assert_eq!(line(&mut d,at("mrxmissionflow", 3, 1)), (None, None));
    }

    #[test]
    fn a_breakpoint_pauses_and_continue_resumes() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach(&mut d, A, vec![bp(7)]);
        assert_eq!(line(&mut d,at("gurcon002", 6, 2)).1, None);
        let (ev, reason) = line(&mut d,at("gurcon002", 7, 2));
        assert!(ev.is_some());
        assert_eq!(reason, Some(PauseReason::Breakpoint));
        d.pause(at("gurcon002", 7, 2), PauseReason::Breakpoint);
        assert!(d.paused());
        assert_eq!(d.request(A, Request::Continue { id: "c".into() }).effect, Effect::Resume);
        assert!(matches!(d.resumed(), Some(Event::Resumed { .. })));
        assert!(!d.paused());
        assert_eq!(line(&mut d,at("gurcon002", 8, 2)).1, None, "continue does not stop on the next line");
    }

    #[test]
    fn continue_and_step_need_a_paused_run() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach(&mut d, A, Vec::new());
        assert!(refused(&d.request(A, Request::Continue { id: "c".into() })).contains("not paused"));
        assert!(refused(&d.request(A, Request::Step { id: "s".into(), step: Step::Into })).contains("not paused"));
    }

    #[test]
    fn steps_stop_by_depth() {
        let cases = [
            (Step::Into, [(3, true), (2, true), (1, true)]),
            (Step::Over, [(3, false), (2, true), (1, true)]),
            (Step::Out, [(3, false), (2, false), (1, true)]),
        ];
        for (step, expect) in cases {
            for (depth, stops) in expect {
                let mut d = Debugger::new(HostKind::Engine, "1.0");
                attach(&mut d, A, Vec::new());
                line(&mut d,at("gurcon002", 1, 2));
                d.pause(at("gurcon002", 1, 2), PauseReason::Breakpoint);
                d.request(A, Request::Step { id: "s".into(), step });
                d.resumed();
                let reason = line(&mut d,at("gurcon002", 2, depth)).1;
                assert_eq!(reason == Some(PauseReason::Step), stops, "{step:?} from depth 2, line at depth {depth}");
            }
        }
    }

    #[test]
    fn a_breakpoint_outranks_a_step() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        attach(&mut d, A, vec![bp(9)]);
        d.pause(at("gurcon002", 1, 2), PauseReason::Breakpoint);
        d.request(A, Request::Step { id: "s".into(), step: Step::Into });
        d.resumed();
        assert_eq!(line(&mut d,at("gurcon002", 9, 3)).1, Some(PauseReason::Breakpoint));
    }

    #[test]
    fn a_module_is_reported_once_per_stamp() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        attach(&mut d, A, Vec::new());
        assert!(d.module("gurcon002", "sha256:aa").is_some());
        assert_eq!(d.module("GurCon002", "sha256:aa"), None);
        assert!(d.module("gurcon002", "sha256:bb").is_some(), "a reload with other code is reported again");
        assert_eq!(d.module("mrxmissionflow", "sha256:cc"), None);
    }

    #[test]
    fn the_events_of_one_attach_have_no_gaps() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        let mut check = Sequence::default();
        let mut out = attach(&mut d, A, vec![bp(2)]).events;
        out.extend(d.module("gurcon002", "sha256:aa"));
        out.extend(d.stack(vec![at("gurcon002", 0, 1)]));
        out.extend(d.entry(at("gurcon002", 0, 2)));
        out.extend(d.entry(at("other", 0, 3)));
        let (ev, reason) = line(&mut d,at("gurcon002", 2, 2));
        out.extend(ev);
        out.extend(d.pause(at("gurcon002", 2, 2), reason.unwrap()));
        out.extend(d.request(A, Request::Continue { id: "c".into() }).events);
        out.extend(d.resumed());
        out.extend(d.engine(at("gurcon002", 3, 2), "Pg.Spawn".into(), Vec::new()).0);
        out.extend(d.engine_return(5, Vec::new()));
        out.extend(d.ret(at("gurcon002", 51, 2)));
        out.extend(d.request(A, Request::Detach { id: "d".into() }).events);
        for e in &out {
            check.check(e).unwrap_or_else(|err| panic!("{err}: {out:?}"));
        }
        assert!(matches!(out.last(), Some(Event::Detached { error: None, .. })));
        assert!(!d.attached());
    }

    #[test]
    fn a_failing_host_detaches_with_its_reason() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        assert_eq!(d.fail("x".into()), None);
        attach(&mut d, A, Vec::new());
        assert!(matches!(d.fail("outbox full".into()), Some(Event::Detached { error: Some(e), .. }) if e == "outbox full"));
        assert!(!d.attached());
    }

    fn engine_attach(d: &mut Debugger, engine: Vec<String>) -> Reply {
        let attach = Request::Attach {
            id: "a".into(),
            protocol: PROTOCOL,
            client: "c".into(),
            modules: vec!["gurcon002".into()],
            breakpoints: Vec::new(),
            engine,
        };
        d.request(A, attach)
    }

    #[test]
    fn engine_breakpoints_name_bindings_the_host_has() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        assert!(refused(&d.request(A, Request::Bindings { id: "b".into() })).contains("has not named"));
        assert!(refused(&engine_attach(&mut d, vec!["Pg.Spawn".into()])).contains("has not named"));
        d.set_bindings(Err("no luaL_Reg tables".into()));
        assert!(refused(&engine_attach(&mut d, vec!["Pg.Spawn".into()])).contains("no luaL_Reg tables"));
        d.set_bindings(Ok(["Pg.Spawn".to_string(), "Object.GetYaw".to_string()].into()));
        let r = d.request(A, Request::Bindings { id: "b".into() });
        assert_eq!(r.events, vec![Event::Names { id: "b".into(), bindings: vec!["Object.GetYaw".into(), "Pg.Spawn".into()] }]);
        assert!(refused(&engine_attach(&mut d, vec!["Pg.Spwan".into()])).contains("no engine binding is named Pg.Spwan"));
        assert!(!d.attached());
        assert_eq!(engine_attach(&mut d, vec!["Pg.Spawn".into()]).effect, Effect::Install);
        assert!(d.breaks_on("Pg.Spawn") && !d.breaks_on("Object.GetYaw"));
        // The set is replaced with the line breakpoints, and an unknown name leaves it as it was.
        assert!(refused(&d.request(A, Request::Breakpoints { id: "b".into(), breakpoints: vec![], engine: vec!["x".into()] })).contains("named x"));
        assert!(d.breaks_on("Pg.Spawn"));
        d.request(A, Request::Breakpoints { id: "b".into(), breakpoints: vec![], engine: vec!["Object.GetYaw".into()] });
        assert!(!d.breaks_on("Pg.Spawn") && d.breaks_on("Object.GetYaw"));
    }

    #[test]
    fn an_engine_call_reports_from_a_filtered_module_and_pauses_from_any() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        d.set_bindings(Ok(["Pg.Spawn".to_string(), "Object.GetYaw".to_string()].into()));
        engine_attach(&mut d, vec!["Pg.Spawn".into()]);
        let (ev, why) = d.engine(at("mrxutil", 4, 2), "Pg.Spawn".into(), Vec::new());
        assert!(matches!(ev, Some(Event::Engine { .. })) && why == Some(PauseReason::Engine));
        let (ev, why) = d.engine(at("gurcon002", 4, 2), "Object.GetYaw".into(), Vec::new());
        assert!(matches!(ev, Some(Event::Engine { .. })) && why.is_none(), "a filtered module reports every binding");
        assert_eq!(d.engine(at("mrxutil", 4, 2), "Object.GetYaw".into(), Vec::new()), (None, None));

        // Paused in a module the attach does not filter: no line to step to, so only Continue.
        d.pause(at("mrxutil", 4, 2), PauseReason::Engine);
        let r = d.request(A, Request::Step { id: "s".into(), step: Step::Over });
        let e = refused(&r);
        assert!(e.contains("paused in mrxutil, which this attach does not watch"), "{e}");
        assert_eq!(d.request(A, Request::Continue { id: "c".into() }).effect, Effect::Resume);
        d.resumed();
        d.pause(at("gurcon002", 4, 2), PauseReason::Engine);
        assert_eq!(d.request(A, Request::Step { id: "s".into(), step: Step::Over }).effect, Effect::Resume);
    }

    #[test]
    fn watch_changes_the_filter_of_a_paused_run_and_keeps_the_tables_it_still_needs() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        attach(&mut d, A, Vec::new());
        assert!(d.has_lines("gurcon002", key()));
        d.pause(at("gurcon002", 2, 1), PauseReason::Breakpoint);
        let other = FunctionKey { line_defined: 5, last_line_defined: 9, code_len: 2 };
        stage(&mut d, A, vec![LineTable { module: "MrxUtil".into(), function: other, lines: vec![6, 7] }]);
        let r = d.request(A, Request::Watch { id: "w".into(), modules: vec!["gurcon002".into(), "MrxUtil".into()] });
        assert_eq!((r.events, r.effect), (vec![Event::Ack { id: "w".into() }], Effect::Refilter));
        assert!(d.filtered("mrxutil") && d.has_lines("mrxutil", other) && d.has_lines("gurcon002", key()));
        assert!(d.paused(), "the pause holds");
        // Dropping a module drops its tables; only the holder can watch; an empty watch needs an engine breakpoint.
        d.request(A, Request::Watch { id: "w".into(), modules: vec!["mrxutil".into()] });
        assert!(!d.filtered("gurcon002") && !d.has_lines("gurcon002", key()));
        assert!(refused(&d.request(B, Request::Watch { id: "w".into(), modules: vec![] })).contains("held by"));
        assert!(refused(&d.request(A, Request::Watch { id: "w".into(), modules: vec![] })).contains("no engine breakpoint"));
        // A staged table for a module the watch does not name refuses it, and is consumed.
        stage(&mut d, A, vec![LineTable { module: "other".into(), function: other, lines: vec![1, 1] }]);
        assert!(refused(&d.request(A, Request::Watch { id: "w".into(), modules: vec!["mrxutil".into()] })).contains("other"));
        assert_eq!(d.request(A, Request::Watch { id: "w".into(), modules: vec!["mrxutil".into()] }).effect, Effect::Refilter);
    }

    #[test]
    fn breakpoints_are_replaced_by_the_holder_only() {
        let mut d = Debugger::new(HostKind::Engine, "1.0");
        attach(&mut d, A, vec![bp(2)]);
        assert!(refused(&d.request(B, Request::Breakpoints { id: "b".into(), breakpoints: vec![], engine: vec![] })).contains("held by"));
        d.request(A, Request::Breakpoints { id: "b".into(), breakpoints: vec![bp(5)], engine: vec![] });
        assert_eq!(line(&mut d,at("gurcon002", 2, 1)).1, None);
        assert_eq!(line(&mut d,at("gurcon002", 5, 1)).1, Some(PauseReason::Breakpoint));
    }
}
