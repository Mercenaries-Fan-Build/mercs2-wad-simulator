//! `Timer` engine binding namespace — luaL_Reg table VA 0x00b99bbc, 4 cfuncs.
//!
//! Wave-0 silo E3 seed. `REQUIRED` is the full cfunc surface this namespace must eventually back with
//! real bodies (source: the live Surface-B trace `mods/lua_trace_asi/reference/binding_map.json`;
//! `corpus_calls` = call sites observed in `docs/mercs2-luacd`). The exe is the oracle — do not trim
//! this list; a name leaves the "stubs remaining" tally only when [`install`] gives it a real body.
//!
//! The engine `Timer` table is a named stopwatch controller (`Start`/`Stop`/`Pause`/`Resume`). The
//! reimpl bans wall-clock reads (`Instant::now`) in this crate — and the required surface exposes no
//! elapsed-time query — so the *faithful, deterministic* body is the run/pause **state machine** these
//! four controls drive. State is held in a table-owned map keyed by the timer's first argument (a name
//! or handle; the no-arg form uses a single default slot), so `Start`→`Pause`→`Resume`→`Stop`
//! transitions are modelled exactly with no host clock. When a later silo wires a monotonic frame
//! clock, elapsed accounting hangs off this same map without changing the Lua-visible surface.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use mlua::{Lua, Result as LuaResult, Value, Variadic};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Timer";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Timer";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b99bbc;

pub const REQUIRED: &[Required] = &[
    Required { name: "Start", corpus_calls: 0 },
    Required { name: "Stop", corpus_calls: 0 },
    Required { name: "Pause", corpus_calls: 0 },
    Required { name: "Resume", corpus_calls: 0 },
];

/// Run state of one named timer.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Running,
    Paused,
}

type Timers = Rc<RefCell<HashMap<String, State>>>;

/// Derive the map key from the control's first argument (name/handle); no argument → single default.
fn key_of(args: &Variadic<Value>) -> String {
    match args.first() {
        Some(Value::String(s)) => s.to_string_lossy(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// All 4 controls get real bodies — a deterministic, host-free run/pause state machine.
pub fn install(lua: &Lua, _host: &SharedHost) -> LuaResult<Installed> {
    let mut b = NsBuilder::new(lua)?;

    let timers: Timers = Rc::new(RefCell::new(HashMap::new()));

    let t = timers.clone();
    b.real(
        "Start",
        lua.create_function(move |_, args: Variadic<Value>| {
            t.borrow_mut().insert(key_of(&args), State::Running);
            Ok(())
        })?,
    )?;
    let t = timers.clone();
    b.real(
        "Stop",
        lua.create_function(move |_, args: Variadic<Value>| {
            t.borrow_mut().remove(&key_of(&args));
            Ok(())
        })?,
    )?;
    let t = timers.clone();
    b.real(
        "Pause",
        lua.create_function(move |_, args: Variadic<Value>| {
            if let Some(s) = t.borrow_mut().get_mut(&key_of(&args)) {
                *s = State::Paused;
            }
            Ok(())
        })?,
    )?;
    let t = timers.clone();
    b.real(
        "Resume",
        lua.create_function(move |_, args: Variadic<Value>| {
            if let Some(s) = t.borrow_mut().get_mut(&key_of(&args)) {
                *s = State::Running;
            }
            Ok(())
        })?,
    )?;

    b.install_global(GLOBAL)
}
