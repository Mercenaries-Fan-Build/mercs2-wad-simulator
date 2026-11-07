//! What a host does with each piece of queued work, while running or paused: which frames to send to whom,
//! and whether to install the hook, remove it, or resume a paused game thread. The host carries a
//! [`Step`] out; every way out of a pause (continue, step, detach, the holder disconnecting, the host
//! giving up) is decided here and tested.

use super::session::{Debugger, Effect};
use super::{Event, Request};

/// One item a transport queued for the game thread.
#[derive(Clone, Debug, PartialEq)]
pub enum Work {
    /// A request from client connection `.0`.
    Request(u64, Request),
    /// Client connection `.0` closed.
    Disconnected(u64),
}

/// What the host must do.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Step {
    /// Each frame and the client connection it goes to, in order.
    pub sends: Vec<(u64, Event)>,
    /// A client attached: install the hook and report the attach state.
    pub install: bool,
    /// The module filter changed: decide each hooked state's count hook again, and report each newly
    /// filtered module already loaded.
    pub refilter: bool,
    /// Remove the hook.
    pub remove: bool,
    /// A paused game thread resumes.
    pub resume: bool,
}

/// Handle `work`. `unavailable` is why this host cannot debug at all (its sites did not resolve); an attach
/// is then refused with it.
pub fn handle(dbg: &mut Debugger, work: Work, unavailable: Option<&str>) -> Step {
    match work {
        Work::Disconnected(conn) => {
            if dbg.disconnected(conn) {
                Step { remove: true, resume: true, ..Step::default() }
            } else {
                Step::default()
            }
        }
        Work::Request(conn, req) => {
            if let (Request::Attach { id, .. }, Some(why)) = (&req, unavailable) {
                return Step { sends: vec![(conn, Event::Refused { id: id.clone(), error: why.to_string() })], ..Step::default() };
            }
            let reply = dbg.request(conn, req);
            let mut step = Step { sends: reply.events.into_iter().map(|e| (conn, e)).collect(), ..Step::default() };
            match reply.effect {
                Effect::None => {}
                Effect::Install => step.install = true,
                Effect::Refilter => step.refilter = true,
                Effect::Resume => step.resume = true,
                Effect::Remove => {
                    step.remove = true;
                    step.resume = true;
                }
            }
            step
        }
    }
}

/// The host cannot go on (a frame could not be delivered, memory could not be read): the last frame to the
/// holder, which the host sends if it still can, then the hook comes off and a paused game resumes.
pub fn give_up(dbg: &mut Debugger, why: String) -> Step {
    let holder = dbg.holder();
    let sends = match (dbg.fail(why), holder) {
        (Some(ev), Some(conn)) => vec![(conn, ev)],
        _ => Vec::new(),
    };
    Step { sends, install: false, refilter: false, remove: true, resume: true }
}

#[cfg(test)]
mod tests {
    use super::super::{At, FunctionKey, HostKind, PauseReason, Step as StepKind, PROTOCOL};
    use super::*;

    const A: u64 = 1;
    const B: u64 = 2;

    fn attach(id: &str) -> Request {
        Request::Attach {
            id: id.into(),
            protocol: PROTOCOL,
            client: "w".into(),
            modules: vec!["m".into()],
            breakpoints: Vec::new(),
            engine: Vec::new(),
        }
    }

    /// A debugger held by A and paused.
    fn paused() -> Debugger {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        assert!(handle(&mut d, Work::Request(A, attach("a")), None).install);
        let at = At { module: "m".into(), function: FunctionKey { line_defined: 1, last_line_defined: 2, code_len: 3 }, pc: 0, line: None, depth: 1 };
        d.pause(at, PauseReason::Breakpoint);
        d
    }

    #[test]
    fn continue_and_step_resume_a_paused_run() {
        for req in [Request::Continue { id: "c".into() }, Request::Step { id: "s".into(), step: StepKind::Over }] {
            let mut d = paused();
            let s = handle(&mut d, Work::Request(A, req), None);
            assert!(s.resume && !s.remove, "{s:?}");
            assert!(matches!(&s.sends[..], [(A, Event::Ack { .. })]));
        }
    }

    #[test]
    fn detach_while_paused_removes_the_hook_and_resumes() {
        let mut d = paused();
        let s = handle(&mut d, Work::Request(A, Request::Detach { id: "d".into() }), None);
        assert!(s.remove && s.resume);
        assert!(matches!(&s.sends[..], [(A, Event::Ack { .. }), (A, Event::Detached { error: None, .. })]));
        assert!(!d.attached());
    }

    #[test]
    fn the_holder_disconnecting_while_paused_removes_the_hook_and_resumes() {
        let mut d = paused();
        assert_eq!(handle(&mut d, Work::Disconnected(B), None), Step::default(), "another client leaving changes nothing");
        let s = handle(&mut d, Work::Disconnected(A), None);
        assert!(s.remove && s.resume && s.sends.is_empty());
        assert!(!d.attached());
    }

    #[test]
    fn giving_up_while_paused_tells_the_holder_and_resumes() {
        let mut d = paused();
        let s = give_up(&mut d, "outbox full".into());
        assert!(s.remove && s.resume);
        assert!(matches!(&s.sends[..], [(A, Event::Detached { error: Some(e), .. })] if e == "outbox full"));
        assert!(!d.attached());
        let s = give_up(&mut d, "again".into());
        assert!(s.sends.is_empty() && s.remove && s.resume, "nothing to tell once detached");
    }

    #[test]
    fn another_client_cannot_resume_a_paused_run() {
        let mut d = paused();
        let s = handle(&mut d, Work::Request(B, Request::Continue { id: "c".into() }), None);
        assert!(!s.resume && !s.remove);
        assert!(matches!(&s.sends[..], [(B, Event::Refused { .. })]));
        assert!(d.paused());
    }

    #[test]
    fn an_unavailable_host_refuses_attach_and_still_says_hello() {
        let mut d = Debugger::new(HostKind::Retail, "pc-v10");
        let s = handle(&mut d, Work::Request(A, attach("a")), Some("luaU_undump matched 0 time(s)"));
        assert!(!s.install && !d.attached());
        assert!(matches!(&s.sends[..], [(A, Event::Refused { error, .. })] if error.contains("luaU_undump")));
        let s = handle(&mut d, Work::Request(A, Request::Hello { id: "h".into(), protocol: PROTOCOL }), Some("x"));
        assert!(matches!(&s.sends[..], [(A, Event::Host { .. })]));
    }
}
