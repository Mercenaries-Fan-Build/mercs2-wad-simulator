//! What the retail game's hook does, decided over its memory: m2-sdk.dll installs the hook and blocks the
//! game thread; everything between "the VM called the hook" and "these events, and pause or not" is here,
//! so it is tested against laid-out memory rather than only in the game.
//!
//! The hook runs with mask call | return, and adds count (with a count of 1) only while the running frame
//! is a function that raises line events (filtered, with a line table): [`Outcome::count`] says when that
//! changes, on every call and return, and [`Retail::count_for`] what it is at attach. The VM's own line
//! hook is not used: it fires on the original author's lines (see the module doc of [`super`]). For each
//! event `luaD_callhook` reports (`ar.event` at +0, `ar.i_ci` at +0x60, `pc-v10 0x868290`):
//!
//! * **count (3)**: the running frame (`L->ci`) executes the instruction `L->savedpc` is one past; for a
//!   Lua function of a filtered module that goes to [`Debugger::instruction`].
//! * **call (0)**: a Lua function of a filtered module is entered; a C function is an engine call when its
//!   function is an engine binding ([`Retail::new`]'s map) and the calling Lua frame is in a filtered
//!   module, or from any Lua frame when the binding has an engine breakpoint, which pauses there, before
//!   the binding runs. Its arguments are the frame's `L->base..L->top`.
//! * **return (1)**: a Lua function of a filtered module returns from the instruction `L->savedpc` is one
//!   past (`OP_RETURN` stores it). A C function's results are not visible to the hook (only `luaD_poscall`
//!   `0x868850`, L in ECX and the first result in EAX, is handed where they start), so they arrive through
//!   [`Retail::poscall`], which the host calls from a detour of it.
//! * **tail return (4)** reports frames a tail call replaced; depth already accounts for them.

use super::session::Debugger;
use super::{game_text, mask_addresses, number, src_line, Arg, At, Event, FunctionKey, PauseReason};
use crate::lua_proto::{
    frame_proto, function_key, lua_frames, pc_index, source, CI_SAVEDPC, CI_SIZE, CLOSURE_ISC, CLOSURE_PROTO, OFF_SAVEDPC,
    PROTO_CODE, PROTO_LINEINFO, PROTO_SIZELINEINFO,
};
use crate::lua_stack::{Mem, CI_OFF_FUNC, OFF_BASE, OFF_BASE_CI, OFF_CI, OFF_TOP, TVALUE_SIZE};
use std::collections::HashMap;

pub const EVENT_CALL: u32 = 0;
pub const EVENT_RETURN: u32 = 1;
pub const EVENT_LINE: u32 = 2;
pub const EVENT_COUNT: u32 = 3;
pub const EVENT_TAILRETURN: u32 = 4;
/// `hookmask` bits (`L+0x36`): call, return, count.
pub const MASK_CALL: u8 = 1;
pub const MASK_RETURN: u8 = 2;
pub const MASK_COUNT: u8 = 8;
/// A C closure's function: `luaD_precall`'s C branch calls `[[[L->ci]->func]+0x10]` (`0x8687AB`-`0x8687B7`),
/// the slot its Lua branch reads the `Proto` from (`0x868646`).
pub const CCLOSURE_F: u32 = CLOSURE_PROTO;
/// More arguments than this on one engine call are refused as a misread frame.
const MAX_ARGS: u32 = 256;

/// What a hook call produced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// To send, in order.
    pub events: Vec<Event>,
    /// Pause here: the host sends [`Debugger::pause`]'s event and blocks.
    pub pause: Option<(At, PauseReason)>,
    /// From now on, report each instruction (the count hook on) or not: set on every call and return, by
    /// the frame that runs next.
    pub count: Option<bool>,
}

#[derive(Clone, Debug)]
struct ProtoInfo {
    code: u32,
    module: String,
    key: FunctionKey,
}

/// An engine call awaiting its return.
#[derive(Clone, Copy, Debug)]
struct Pending {
    depth: u32,
    closure: u32,
    seq: u64,
}

/// The retail hook's state for one attach.
#[derive(Clone, Debug, Default)]
pub struct Retail {
    bindings: HashMap<u32, String>,
    protos: HashMap<u32, ProtoInfo>,
    pending: Vec<Pending>,
}

impl Retail {
    /// `bindings`: every engine binding's function address and its name (`Pg.Spawn`), from the game's
    /// registered namespaces.
    pub fn new(bindings: HashMap<u32, String>) -> Retail {
        Retail { bindings, protos: HashMap::new(), pending: Vec::new() }
    }

    /// The module and key of the function at `p`, cached by address and checked against its code pointer
    /// (a freed `Proto`'s address can be reused).
    fn info<M: Mem>(&mut self, m: &M, p: u32) -> Result<ProtoInfo, String> {
        let code = m.read_u32(p + PROTO_CODE);
        if let Some(i) = self.protos.get(&p).filter(|i| i.code == code) {
            return Ok(i.clone());
        }
        let i = ProtoInfo { code, module: game_text(&source(m, p)?), key: function_key(m, p)? };
        self.protos.insert(p, i.clone());
        Ok(i)
    }

    fn at<M: Mem>(&mut self, m: &M, p: u32, pc: u32, depth: u32) -> Result<At, String> {
        let i = self.info(m, p)?;
        let sizeline = m.read_u32(p + PROTO_SIZELINEINFO);
        let line = (pc < sizeline).then(|| m.read_u32(m.read_u32(p + PROTO_LINEINFO) + pc * 4));
        Ok(At { module: i.module, function: i.key, pc, line, depth })
    }

    /// Whether the frame at `ci` raises line events.
    fn counts<M: Mem>(&mut self, m: &M, ci: u32, dbg: &Debugger) -> Result<bool, String> {
        let Some(p) = frame_proto(m, ci)? else { return Ok(false) };
        let i = self.info(m, p)?;
        Ok(dbg.has_lines(&i.module, i.key))
    }

    /// Whether state `l`'s running frame needs the count hook, at attach.
    pub fn count_for<M: Mem>(&mut self, m: &M, l: u32, dbg: &Debugger) -> Result<bool, String> {
        self.counts(m, m.read_u32(l + OFF_CI), dbg)
    }

    /// The hook was called on state `l` with `event` at call depth `depth` (`ar.i_ci`).
    pub fn hook<M: Mem>(&mut self, m: &M, l: u32, event: u32, depth: u32, dbg: &mut Debugger) -> Result<Outcome, String> {
        let mut out = Outcome::default();
        let ci = m.read_u32(l + OFF_CI);
        match event {
            // The frame entered runs next; on a return, the caller does (a `pcall` that caught an error
            // returns through its own C frame, so the frame it lands in is set here too).
            EVENT_CALL => out.count = Some(self.counts(m, ci, dbg)?),
            EVENT_RETURN => {
                let caller = ci.checked_sub(CI_SIZE).filter(|c| *c >= m.read_u32(l + OFF_BASE_CI));
                out.count = Some(match caller {
                    Some(c) => self.counts(m, c, dbg)?,
                    None => false,
                });
            }
            _ => {}
        }
        match event {
            EVENT_COUNT => {
                let Some(p) = frame_proto(m, ci)? else { return Ok(out) };
                if !dbg.filtered(&self.info(m, p)?.module) {
                    return Ok(out);
                }
                let at = self.at(m, p, pc_index(m, p, m.read_u32(l + OFF_SAVEDPC))?, depth)?;
                let (ev, reason) = dbg.instruction(at.clone())?;
                out.events.extend(ev);
                out.pause = reason.map(|r| (at, r));
            }
            EVENT_CALL => match frame_proto(m, ci)? {
                Some(p) => {
                    if dbg.filtered(&self.info(m, p)?.module) {
                        let at = self.at(m, p, 0, depth)?;
                        out.events.extend(dbg.entry(at));
                    }
                }
                None => {
                    let closure = m.read_u32(m.read_u32(ci + CI_OFF_FUNC));
                    if m.read_u8(closure + CLOSURE_ISC) == 0 {
                        return Ok(out);
                    }
                    let Some(binding) = self.bindings.get(&m.read_u32(closure + CCLOSURE_F)).cloned() else {
                        return Ok(out);
                    };
                    let caller = ci.checked_sub(CI_SIZE).filter(|c| *c >= m.read_u32(l + OFF_BASE_CI));
                    let Some(caller) = caller else { return Ok(out) };
                    let Some(cp) = frame_proto(m, caller)? else { return Ok(out) };
                    // An engine breakpoint stops a call from any module; otherwise only a filtered one reports.
                    if !dbg.breaks_on(&binding) && !dbg.filtered(&self.info(m, cp)?.module) {
                        return Ok(out);
                    }
                    let caller_depth = (caller - m.read_u32(l + OFF_BASE_CI)) / CI_SIZE;
                    let at = self.at(m, cp, pc_index(m, cp, m.read_u32(caller + CI_SAVEDPC))?, caller_depth)?;
                    let args = self.values(m, m.read_u32(l + OFF_BASE), m.read_u32(l + OFF_TOP))?;
                    self.pending.retain(|q| q.depth < depth);
                    let (ev, pause) = dbg.engine(at.clone(), binding, args);
                    if let Some(ev) = ev {
                        let seq = ev.seq().expect("an engine event has a seq");
                        self.pending.push(Pending { depth, closure, seq });
                        out.events.push(ev);
                    }
                    out.pause = pause.map(|r| (at, r));
                }
            },
            EVENT_RETURN => {
                if let Some(p) = frame_proto(m, ci)? {
                    if dbg.filtered(&self.info(m, p)?.module) {
                        let at = self.at(m, p, pc_index(m, p, m.read_u32(l + OFF_SAVEDPC))?, depth)?;
                        out.events.extend(dbg.ret(at));
                    }
                }
            }
            EVENT_TAILRETURN | EVENT_LINE => {}
            other => return Err(format!("the hook was called with event {other}")),
        }
        Ok(out)
    }

    /// `luaD_poscall(L, firstResult)` is about to finish the frame at `L->ci`: when it is an engine call
    /// recorded at this depth, its results `firstResult..L->top` as that call's `engine_return`.
    pub fn poscall<M: Mem>(&mut self, m: &M, l: u32, first_result: u32, dbg: &mut Debugger) -> Result<Option<Event>, String> {
        let ci = m.read_u32(l + OFF_CI);
        let depth = (ci - m.read_u32(l + OFF_BASE_CI)) / CI_SIZE;
        self.pending.retain(|q| q.depth <= depth);
        let Some(top) = self.pending.last().copied() else { return Ok(None) };
        let closure = m.read_u32(m.read_u32(ci + CI_OFF_FUNC));
        if top.depth != depth || top.closure != closure {
            return Ok(None);
        }
        self.pending.pop();
        let values = self.values(m, first_result, m.read_u32(l + OFF_TOP))?;
        Ok(dbg.engine_return(top.seq, values))
    }

    /// The Lua frames on the stack at attach.
    pub fn stack<M: Mem>(&mut self, m: &M, l: u32, dbg: &mut Debugger) -> Result<Option<Event>, String> {
        let mut frames = Vec::new();
        for f in lua_frames(m, l)? {
            frames.push(self.at(m, f.proto, f.pc, f.depth)?);
        }
        Ok(dbg.stack(frames))
    }

    fn values<M: Mem>(&mut self, m: &M, from: u32, to: u32) -> Result<Vec<Arg>, String> {
        if to < from || !(to - from).is_multiple_of(TVALUE_SIZE) || (to - from) / TVALUE_SIZE > MAX_ARGS {
            return Err(format!("the value range {from:#x}..{to:#x} is not a run of stack slots"));
        }
        (0..(to - from) / TVALUE_SIZE).map(|i| self.value(m, from + i * TVALUE_SIZE)).collect()
    }

    /// A stack value in `mercs2_autoplay`'s encoding. Tables are not descended into.
    fn value<M: Mem>(&mut self, m: &M, tv: u32) -> Result<Arg, String> {
        let (v, tt) = (m.read_u32(tv), m.read_u32(tv + 4));
        Ok(match tt {
            0 => Arg::Nil,
            1 => Arg::Bool { v: v != 0 },
            2 => Arg::LightUserData { p: format!("{v:#x}") },
            3 => Arg::Num { v: number(f32::from_bits(v)) },
            4 => Arg::Str { v: mask_addresses(&game_text(&crate::lua_proto::read_tstring(m, v)?)) },
            5 => Arg::Table { n: None, v: None, error: None },
            6 if m.read_u8(v + CLOSURE_ISC) != 0 => Arg::Function { src: None, src_error: None },
            6 => {
                let p = m.read_u32(v + CLOSURE_PROTO);
                let i = self.info(m, p)?;
                Arg::Function { src: Some(src_line(&i.module, i.key.line_defined as i64)), src_error: None }
            }
            7 => Arg::Userdata,
            8 => Arg::Thread,
            other => return Err(format!("the stack value at {tv:#x} has type tag {other}")),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{session::Effect, Breakpoint, LineTable, Request, PROTOCOL};
    use super::*;
    use crate::lua_proto::layout::{alloc, closure, lay, le, tstring};
    use crate::lua_stack::fake::FakeMem;
    use crate::luachunk::{Constant, Proto};

    const BINDING: u32 = 0x0059_1000;

    fn proto(source: &[u8], line_defined: u32, code: usize) -> Proto {
        Proto {
            source: Some(source.to_vec()),
            line_defined,
            last_line_defined: line_defined + 10,
            nups: 0,
            num_params: 0,
            is_vararg: 0,
            max_stack: 4,
            code: vec![0x1E; code],
            constants: vec![Constant::Nil],
            protos: Vec::new(),
            line_info: (0..code as u32).map(|i| 300 + i).collect(),
            loc_vars: Vec::new(),
            upvalues: Vec::new(),
        }
    }

    /// A state running `frames` (closures, base first), the top frame at `savedpc`, its stack holding
    /// `slots` from `L->base`.
    struct World {
        m: FakeMem,
        next: u32,
        l: u32,
        /// The C function at the base of every stack.
        base: u32,
    }

    impl World {
        fn new() -> World {
            let (mut m, mut next) = (FakeMem::default(), 0x0010_0000);
            let base = closure(&mut m, &mut next, true, 0);
            World { m, next, l: 0x0050_0000, base }
        }

        fn lua(&mut self, p: &Proto) -> (u32, u32) {
            let pa = lay(&mut self.m, &mut self.next, p);
            (closure(&mut self.m, &mut self.next, false, pa), pa)
        }

        fn c(&mut self, f: u32) -> u32 {
            closure(&mut self.m, &mut self.next, true, f)
        }

        fn code(&self, p: u32) -> u32 {
            self.m.read_u32(p + PROTO_CODE)
        }

        /// `frames` above the base C frame: (closure, saved pc address); the last is the running frame.
        fn state(&mut self, frames: &[(u32, u32)], slots: &[(u32, u32)]) {
            let frames: Vec<(u32, u32)> = std::iter::once((self.base, 0)).chain(frames.iter().copied()).collect();
            let mut stack = Vec::new();
            for (cl, _) in &frames {
                stack.extend_from_slice(&[*cl, 6]);
            }
            let base_slot = stack.len() as u32 / 2;
            for (v, tt) in slots {
                stack.extend_from_slice(&[*v, *tt]);
            }
            let stack_at = alloc(&mut self.m, &mut self.next, le(&stack));
            let mut cis = Vec::new();
            for (i, (_, pc)) in frames.iter().enumerate() {
                let mut ci = vec![0u32; (CI_SIZE / 4) as usize];
                ci[(CI_OFF_FUNC / 4) as usize] = stack_at + i as u32 * TVALUE_SIZE;
                ci[(CI_SAVEDPC / 4) as usize] = *pc;
                cis.extend(ci);
            }
            let ci_at = alloc(&mut self.m, &mut self.next, le(&cis));
            let mut st = vec![0u32; 0x40 / 4];
            st[(OFF_TOP / 4) as usize] = stack_at + (base_slot + slots.len() as u32) * TVALUE_SIZE;
            st[(OFF_BASE / 4) as usize] = stack_at + base_slot * TVALUE_SIZE;
            st[(OFF_CI / 4) as usize] = ci_at + (frames.len() as u32 - 1) * CI_SIZE;
            st[(OFF_BASE_CI / 4) as usize] = ci_at;
            st[(OFF_SAVEDPC / 4) as usize] = frames.last().unwrap().1;
            self.m.map(self.l, le(&st));
        }
    }

    fn attach(d: &mut Debugger, key: FunctionKey, lines: Vec<u32>, bps: Vec<Breakpoint>) {
        d.request(1, Request::Lines { id: "l".into(), lines: vec![LineTable { module: "@gurcon002".into(), function: key, lines }] });
        let r = d.request(
            1,
            Request::Attach {
                id: "a".into(),
                protocol: PROTOCOL,
                client: "w".into(),
                modules: vec!["@gurcon002".into()],
                breakpoints: bps,
                engine: vec![],
            },
        );
        assert_eq!(r.effect, Effect::Install);
    }

    fn key(line_defined: u32, code: u32) -> FunctionKey {
        FunctionKey { line_defined, last_line_defined: line_defined + 10, code_len: code }
    }

    #[test]
    fn instructions_of_a_filtered_function_raise_corpus_lines_and_stop_at_breakpoints() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        let bp = Breakpoint { module: "@gurcon002".into(), function: key(40, 4), pc: 2 };
        attach(&mut d, key(40, 4), vec![7, 7, 8, 8], vec![bp]);
        let mut r = Retail::new(HashMap::new());
        let mut lines = Vec::new();
        for pc in 0..4 {
            w.state(&[(cl, w.code(p) + (pc + 1) * 4)], &[]);
            let o = r.hook(&w.m, w.l, EVENT_COUNT, 1, &mut d).unwrap();
            for e in &o.events {
                if let Event::Line { at, .. } = e {
                    lines.push((at.pc, at.line, at.depth));
                }
            }
            assert_eq!(o.pause.as_ref().map(|(at, why)| (at.pc, *why)), (pc == 2).then_some((2, PauseReason::Breakpoint)));
        }
        assert_eq!(lines, vec![(0, Some(300), 1), (2, Some(302), 1)], "corpus lines 7 and 8 start at pc 0 and 2");
    }

    #[test]
    fn an_unfiltered_module_raises_nothing() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@mrxmissionflow", 40, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::new());
        w.state(&[(cl, w.code(p) + 4)], &[]);
        for ev in [EVENT_COUNT, EVENT_CALL, EVENT_RETURN] {
            let o = r.hook(&w.m, w.l, ev, 1, &mut d).unwrap();
            assert!(o.events.is_empty() && o.pause.is_none() && o.count != Some(true), "event {ev}: {o:?}");
        }
    }

    #[test]
    fn the_count_hook_is_on_only_while_a_function_with_lines_runs() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let (other, op) = w.lua(&proto(b"@mrxmissionflow", 5, 4));
        let (untabled, up) = w.lua(&proto(b"@gurcon002", 60, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::from([(BINDING, "Pg.Spawn".to_string())]));
        let spawn = w.c(BINDING);
        let count = |r: &mut Retail, w: &World, d: &mut Debugger, ev| {
            let depth = (w.m.read_u32(w.l + OFF_CI) - w.m.read_u32(w.l + OFF_BASE_CI)) / CI_SIZE;
            r.hook(&w.m, w.l, ev, depth, d).unwrap().count
        };

        w.state(&[(other, w.code(op) + 4)], &[]);
        assert!(!r.count_for(&w.m, w.l, &d).unwrap(), "attach in an unfiltered frame");
        assert_eq!(count(&mut r, &w, &mut d, EVENT_CALL), Some(false), "entering an unfiltered function");
        w.state(&[(other, w.code(op) + 4), (cl, w.code(p))], &[]);
        assert!(r.count_for(&w.m, w.l, &d).unwrap(), "attach in a function with lines");
        assert_eq!(count(&mut r, &w, &mut d, EVENT_CALL), Some(true), "entering a function with lines");
        w.state(&[(other, w.code(op) + 4), (untabled, w.code(up))], &[]);
        assert_eq!(count(&mut r, &w, &mut d, EVENT_CALL), Some(false), "a filtered function with no table");
        w.state(&[(cl, w.code(p) + 4), (spawn, 0)], &[]);
        assert_eq!(count(&mut r, &w, &mut d, EVENT_CALL), Some(false), "entering a C function");
        assert_eq!(count(&mut r, &w, &mut d, EVENT_RETURN), Some(true), "returning into a function with lines");
        w.state(&[(other, w.code(op) + 4), (cl, w.code(p) + 4 * 4)], &[]);
        assert_eq!(count(&mut r, &w, &mut d, EVENT_RETURN), Some(false), "returning into an unfiltered function");
        assert_eq!(count(&mut r, &w, &mut d, EVENT_COUNT), None, "an instruction changes nothing");
    }

    #[test]
    fn entry_and_return_of_a_filtered_function() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::new());
        w.state(&[(cl, w.code(p))], &[]);
        let o = r.hook(&w.m, w.l, EVENT_CALL, 1, &mut d).unwrap();
        assert!(matches!(&o.events[..], [Event::Entry { at, .. }] if at.pc == 0 && at.function == key(40, 4)));
        w.state(&[(cl, w.code(p) + 4 * 4)], &[]);
        let o = r.hook(&w.m, w.l, EVENT_RETURN, 1, &mut d).unwrap();
        assert!(matches!(&o.events[..], [Event::Return { at, .. }] if at.pc == 3));
    }

    #[test]
    fn an_engine_call_from_a_filtered_frame_and_its_results() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let name = tstring(&mut w.m, &mut w.next, b"pmc_hum_mattias");
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::from([(BINDING, "Pg.Spawn".to_string())]));
        let spawn = w.c(BINDING);
        // gurcon002 at instruction 1 (saved pc one past it) calls Pg.Spawn("pmc_hum_mattias", 2.5, nil, true, guid).
        let args = [(name, 4), (2.5f32.to_bits(), 3), (0, 0), (1, 1), (0x4000_0915, 2)];
        w.state(&[(cl, w.code(p) + 2 * 4), (spawn, 0)], &args);
        let o = r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap();
        let [Event::Engine { seq, at, binding, args }] = &o.events[..] else { panic!("{:?}", o.events) };
        assert_eq!((binding.as_str(), at.pc, at.depth), ("Pg.Spawn", 1, 1));
        assert_eq!(
            serde_json::to_string(args).unwrap(),
            r#"[{"t":"str","v":"pmc_hum_mattias"},{"t":"num","v":2.5},{"t":"nil"},{"t":"bool","v":true},{"t":"tt2","p":"0x40000915"}]"#
        );
        // The binding returns its guid: poscall is handed where the result starts.
        let guid = (0x4000_0a01u32, 2u32);
        w.state(&[(cl, w.code(p) + 2 * 4), (spawn, 0)], &[guid]);
        let first = w.m.read_u32(w.l + OFF_BASE);
        let ret = r.poscall(&w.m, w.l, first, &mut d).unwrap();
        assert!(matches!(ret, Some(Event::EngineReturn { call, ref values, .. }) if call == *seq && values == &vec![Arg::LightUserData { p: "0x40000a01".into() }]));
        assert_eq!(r.poscall(&w.m, w.l, first, &mut d).unwrap(), None, "each call returns once");
    }

    #[test]
    fn a_c_function_that_is_not_a_binding_or_not_called_from_a_filtered_frame_is_not_recorded() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let (other, op) = w.lua(&proto(b"@mrxmissionflow", 5, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::from([(BINDING, "Pg.Spawn".to_string())]));
        let stdlib = w.c(0x0086_36a0);
        w.state(&[(cl, w.code(p) + 4), (stdlib, 0)], &[]);
        assert!(r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap().events.is_empty(), "string.byte is no binding");
        let spawn = w.c(BINDING);
        w.state(&[(other, w.code(op) + 4), (spawn, 0)], &[]);
        assert!(r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap().events.is_empty(), "unfiltered caller");
    }

    #[test]
    fn an_engine_breakpoint_pauses_a_call_from_any_module_before_the_binding_runs() {
        let mut w = World::new();
        let (other, op) = w.lua(&proto(b"@mrxutil", 5, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        d.set_bindings(Ok(["Pg.Spawn".to_string()].into()));
        let r = d.request(
            1,
            Request::Attach {
                id: "a".into(),
                protocol: PROTOCOL,
                client: "w".into(),
                modules: vec![],
                breakpoints: vec![],
                engine: vec!["Pg.Spawn".into()],
            },
        );
        assert_eq!(r.effect, Effect::Install, "an attach with only an engine breakpoint: {:?}", r.events);
        let mut r = Retail::new(HashMap::from([(BINDING, "Pg.Spawn".to_string())]));
        let spawn = w.c(BINDING);
        let name = tstring(&mut w.m, &mut w.next, b"Panhard (Assault)");
        // mrxutil, which nothing filters, calls Pg.Spawn("Panhard (Assault)") from instruction 2.
        w.state(&[(other, w.code(op) + 3 * 4), (spawn, 0)], &[(name, 4)]);
        let o = r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap();
        let [Event::Engine { at, binding, args, .. }] = &o.events[..] else { panic!("{:?}", o.events) };
        assert_eq!((at.module.as_str(), at.pc, at.depth, binding.as_str()), ("@mrxutil", 2, 1, "Pg.Spawn"));
        assert_eq!(args, &vec![Arg::Str { v: "Panhard (Assault)".into() }]);
        assert_eq!(o.pause.as_ref().map(|(at, why)| (at.pc, *why)), Some((2, PauseReason::Engine)));
        // Another binding from the same frame is neither recorded nor a pause.
        let mut r2 = Retail::new(HashMap::from([(BINDING, "Object.GetYaw".to_string())]));
        w.state(&[(other, w.code(op) + 3 * 4), (spawn, 0)], &[]);
        let o = r2.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap();
        assert!(o.events.is_empty() && o.pause.is_none(), "{o:?}");
    }

    #[test]
    fn a_lua_return_at_the_depth_of_a_lost_engine_call_is_not_its_result() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::from([(BINDING, "Pg.Spawn".to_string())]));
        let spawn = w.c(BINDING);
        w.state(&[(cl, w.code(p) + 4), (spawn, 0)], &[]);
        r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap();
        // The binding raised (no poscall); later a Lua function returns at the same depth.
        let (lua2, p2) = w.lua(&proto(b"@gurcon002", 40, 4));
        w.state(&[(cl, w.code(p) + 4), (lua2, w.code(p2) + 4)], &[(0, 0)]);
        let first = w.m.read_u32(w.l + OFF_BASE);
        assert_eq!(r.poscall(&w.m, w.l, first, &mut d).unwrap(), None);
    }

    #[test]
    fn the_stack_at_attach_names_every_lua_frame() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let (other, op) = w.lua(&proto(b"@mrxmissionflow", 5, 4));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::new());
        w.state(&[(other, w.code(op) + 2 * 4), (cl, w.code(p) + 3 * 4)], &[]);
        let Some(Event::Stack { frames, .. }) = r.stack(&w.m, w.l, &mut d).unwrap() else { panic!() };
        let got: Vec<(String, u32, u32)> = frames.into_iter().map(|a| (a.module, a.pc, a.depth)).collect();
        assert_eq!(got, vec![("@mrxmissionflow".into(), 1, 1), ("@gurcon002".into(), 2, 2)]);
    }

    #[test]
    fn a_lua_function_argument_names_where_it_was_defined() {
        let mut w = World::new();
        let (cl, p) = w.lua(&proto(b"@gurcon002", 40, 4));
        let (cb, _) = w.lua(&proto(b"@GurCon002", 77, 2));
        let mut d = Debugger::new(super::super::HostKind::Retail, "pc-v10");
        attach(&mut d, key(40, 4), vec![1; 4], Vec::new());
        let mut r = Retail::new(HashMap::from([(BINDING, "Event.Create".to_string())]));
        let f = w.c(BINDING);
        let c_fn = w.c(0x1234);
        w.state(&[(cl, w.code(p) + 4), (f, 0)], &[(cb, 6), (c_fn, 6)]);
        let o = r.hook(&w.m, w.l, EVENT_CALL, 2, &mut d).unwrap();
        let [Event::Engine { args, .. }] = &o.events[..] else { panic!() };
        assert_eq!(serde_json::to_string(args).unwrap(), r#"[{"t":"tt6","src":"@gurcon002:77"},{"t":"tt6"}]"#);
    }
}
