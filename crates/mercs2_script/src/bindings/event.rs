//! `Event` engine binding namespace — luaL_Reg table VA 0x00b987f8, 4 cfuncs.
//!
//! The engine's event system, script side. The game's entire mission/contract layer is
//! `Event.Create`-driven (688 `Create` / 654 `Delete` call sites in the corpus), so this is the
//! keystone for running the real Lua. Grounded in `docs/reverse_engineer/event_bus_code_map.md`
//! (name-hash subscriber registry + typed dispatch) and the actual corpus call patterns:
//!
//! ```lua
//! -- fire-on-named-event, with an optional filter predicate on the posted data:
//! e = Event.Create(Event.ScriptEvent, {"mpPlayerLeft", function(tData) return uDriver==tData[2] end},
//!                  OnExit, {uDriver, uGuid})
//! -- one-shot timer:
//! Event.Create(Event.TimerRelative, {0.01}, _DeleteWidget, {oWidget})
//! Event.Delete(e)          -- returns nil (scripts do `e = Event.Delete(e)`)
//! Event.Post("mpPlayerLeft", tData)   -- fires matching ScriptEvent handlers
//! ```
//!
//! **What's real here:** `ScriptEvent` (Post → filter → callback) and `TimerRelative` (advanced by the
//! engine each tick via the non-tracked `Event.__pump(dt)` hook). `Create` vs `CreatePersistent` =
//! one-shot vs re-arm. The condition kinds that need world state (`ObjectProximity`/`ObjectDeath`/
//! `Boundary`/`ObjectInSeat`) register + `Delete` cleanly but do not fire yet — the engine must feed
//! their conditions (a later wiring; they show as red hooks in the harness).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use mlua::{Function, Lua, Result as LuaResult, Table, Value, Variadic};

use super::{Installed, NsBuilder, Required};
use crate::SharedHost;

/// Stable coverage key (unique per luaL_Reg table; two tables may share a Lua global).
pub const NAMESPACE: &str = "Event";
/// The Lua global table this namespace installs as.
pub const GLOBAL: &str = "Event";
/// luaL_Reg table VA in the unpacked SecuROM image (`mercs2_unpacked.exe`, base 0x00400000).
pub const TABLE_VA: u32 = 0x00b987f8;

pub const REQUIRED: &[Required] = &[
    Required { name: "Create", corpus_calls: 688 },
    Required { name: "CreatePersistent", corpus_calls: 117 },
    Required { name: "Delete", corpus_calls: 654 },
    Required { name: "Post", corpus_calls: 72 },
];

// Event-kind values. Exact game enum values are not observable from the corpus (scripts use the
// symbolic `Event.X`), so we assign stable distinct ids — only our own Create/pump compares them.
const KIND_SCRIPT_EVENT: i64 = 1;
const KIND_TIMER_RELATIVE: i64 = 2;
const KIND_TIMER_ABSOLUTE: i64 = 3;
const KIND_GAME_STATE_CHANGE: i64 = 4;
const KIND_OBJECT_DEATH: i64 = 5;
const KIND_OBJECT_PROXIMITY: i64 = 6;
const KIND_BOUNDARY: i64 = 7;
const KIND_OBJECT_IN_SEAT: i64 = 8;
const KIND_OBJECT_HIBERNATION: i64 = 9;
const KIND_OBJECT_PHYSICS_EVENT: i64 = 10;
const KIND_BUTTON: i64 = 11;
const KIND_CONTEXT_ACTION: i64 = 12;

/// The Lua-facing event constants (`Event.ScriptEvent`, …). Kept in one place so `install` and any
/// engine-side condition feeder agree.
const KINDS: &[(&str, i64)] = &[
    ("ScriptEvent", KIND_SCRIPT_EVENT),
    ("TimerRelative", KIND_TIMER_RELATIVE),
    ("TimerAbsolute", KIND_TIMER_ABSOLUTE),
    ("GameStateChange", KIND_GAME_STATE_CHANGE),
    ("ObjectDeath", KIND_OBJECT_DEATH),
    ("ObjectProximity", KIND_OBJECT_PROXIMITY),
    ("Boundary", KIND_BOUNDARY),
    ("ObjectInSeat", KIND_OBJECT_IN_SEAT),
    ("ObjectHibernation", KIND_OBJECT_HIBERNATION),
    ("ObjectPhysicsEvent", KIND_OBJECT_PHYSICS_EVENT),
    ("Button", KIND_BUTTON),
    ("ContextAction", KIND_CONTEXT_ACTION),
];

/// One registered event handler.
struct EventReg {
    kind: i64,
    persistent: bool,
    callback: Function,
    cbargs: Vec<Value>,
    // ScriptEvent:
    script_name: Option<String>,
    filter: Option<Function>,
    // TimerRelative:
    timer_remaining: Option<f32>,
    timer_period: Option<f32>,
    // Condition kinds (ObjectDeath/…): the subject GUID the engine fires against.
    subject: Option<u64>,
}

/// The script-side event manager: a handle→registration table. Shared (`Rc<RefCell>`) across the
/// Create/Delete/Post/__pump closures. Single-threaded (the VM + engine share the main thread).
#[derive(Default)]
struct EventManager {
    next: i64,
    regs: BTreeMap<i64, EventReg>,
}

type Mgr = Rc<RefCell<EventManager>>;

/// Convert the optional callback-args table into an owned `Vec<Value>` (the sequence part).
fn seq_values(t: &Option<Table>) -> LuaResult<Vec<Value>> {
    match t {
        Some(t) => t.clone().sequence_values::<Value>().collect(),
        None => Ok(Vec::new()),
    }
}

/// `Event.Create` / `Event.CreatePersistent` body: register a handler, return its integer handle.
fn make(
    mgr: &Mgr,
    kind: i64,
    params: Table,
    callback: Function,
    cbargs: Option<Table>,
    persistent: bool,
) -> LuaResult<i64> {
    let cbargs = seq_values(&cbargs)?;
    let (script_name, filter, timer_remaining, timer_period, subject) = if kind == KIND_SCRIPT_EVENT {
        // params = { name, [filter_fn] }
        (params.get::<String>(1).ok(), params.get::<Option<Function>>(2)?, None, None, None)
    } else if kind == KIND_TIMER_RELATIVE {
        // params = { seconds }
        let secs: f32 = params.get(1).unwrap_or(0.0);
        (None, None, Some(secs), Some(secs), None)
    } else if kind == KIND_OBJECT_DEATH {
        // params = { guid } — fired by the engine when that object dies (Object.Kill / damage).
        let g: Option<i64> = params.get(1).ok();
        (None, None, None, None, g.map(|x| x as u64))
    } else {
        (None, None, None, None, None)
    };
    let mut m = mgr.borrow_mut();
    m.next += 1;
    let h = m.next;
    m.regs.insert(
        h,
        EventReg { kind, persistent, callback, cbargs, script_name, filter, timer_remaining, timer_period, subject },
    );
    Ok(h)
}

/// `Event.Post(name, data)` body: fire every `ScriptEvent` handler for `name` whose filter (if any)
/// accepts `data`. One-shot handlers are removed after firing. Reentrancy-safe: callbacks (which may
/// `Event.Create`/`Delete`) run **after** the manager borrow is dropped.
fn post(mgr: &Mgr, name: &str, data: Value) -> LuaResult<()> {
    // Snapshot the candidate handlers under a short borrow — clone the fn handles out.
    let candidates: Vec<(i64, Option<Function>, Function, Vec<Value>, bool)> = {
        let m = mgr.borrow();
        m.regs
            .iter()
            .filter(|(_, r)| r.kind == KIND_SCRIPT_EVENT && r.script_name.as_deref() == Some(name))
            .map(|(h, r)| (*h, r.filter.clone(), r.callback.clone(), r.cbargs.clone(), r.persistent))
            .collect()
    };
    for (h, filter, callback, cbargs, persistent) in candidates {
        let pass = match filter {
            Some(f) => f.call::<bool>(data.clone()).unwrap_or(false),
            None => true,
        };
        if pass {
            callback.call::<()>(Variadic::from_iter(cbargs))?;
            if !persistent {
                mgr.borrow_mut().regs.remove(&h);
            }
        }
    }
    Ok(())
}

/// `Event.__pump(dt)` body (engine-driven, not a game cfunc): advance `TimerRelative` handlers and
/// fire the due ones. One-shot timers are removed; persistent ones re-arm to their period.
fn pump(mgr: &Mgr, dt: f32) -> LuaResult<()> {
    let due: Vec<(i64, Function, Vec<Value>, bool)> = {
        let mut m = mgr.borrow_mut();
        let mut due = Vec::new();
        for (h, r) in m.regs.iter_mut() {
            if let Some(rem) = r.timer_remaining.as_mut() {
                *rem -= dt;
                if *rem <= 0.0 {
                    due.push((*h, r.callback.clone(), r.cbargs.clone(), r.persistent));
                }
            }
        }
        due
    };
    for (h, callback, cbargs, persistent) in due {
        callback.call::<()>(Variadic::from_iter(cbargs))?;
        let mut m = mgr.borrow_mut();
        if persistent {
            if let Some(r) = m.regs.get_mut(&h) {
                r.timer_remaining = r.timer_period;
            }
        } else {
            m.regs.remove(&h);
        }
    }
    Ok(())
}

pub fn install(lua: &Lua, host: &SharedHost) -> LuaResult<Installed> {
    let _ = host; // Event is script-side (holds Lua callbacks); condition kinds feed in via the engine later.
    let mut b = NsBuilder::new(lua)?;

    // Event-kind enum values (constants, not coverage-tracked cfuncs).
    for (name, val) in KINDS {
        b.value(name, *val)?;
    }

    let mgr: Mgr = Rc::new(RefCell::new(EventManager::default()));
    // Share the manager so other bindings (Object.Kill -> ObjectDeath) and the engine tick can fire
    // condition events into it, via `fire_object_death` etc.
    lua.set_app_data(mgr.clone());

    // Create(kind, params, callback, [args]) -> handle
    let m = mgr.clone();
    b.real(
        "Create",
        lua.create_function(
            move |_, (kind, params, callback, cbargs): (i64, Table, Function, Option<Table>)| {
                make(&m, kind, params, callback, cbargs, false)
            },
        )?,
    )?;

    // CreatePersistent(kind, params, callback, [args]) -> handle (re-arms / survives one fire)
    let m = mgr.clone();
    b.real(
        "CreatePersistent",
        lua.create_function(
            move |_, (kind, params, callback, cbargs): (i64, Table, Function, Option<Table>)| {
                make(&m, kind, params, callback, cbargs, true)
            },
        )?,
    )?;

    // Delete(handle) -> nil  (scripts do `e = Event.Delete(e)`; nil-safe)
    let m = mgr.clone();
    b.real(
        "Delete",
        lua.create_function(move |_, h: Option<i64>| {
            if let Some(h) = h {
                m.borrow_mut().regs.remove(&h);
            }
            Ok(Value::Nil)
        })?,
    )?;

    // Post(name, [data]) -> fire matching ScriptEvent handlers
    let m = mgr.clone();
    b.real(
        "Post",
        lua.create_function(move |_, (name, data): (String, Option<Value>)| {
            post(&m, &name, data.unwrap_or(Value::Nil))
        })?,
    )?;

    // Engine-driven timer pump (not a game cfunc — the render/sim loop calls this each tick).
    let m = mgr.clone();
    b.extra("__pump", lua.create_function(move |_, dt: f32| pump(&m, dt))?)?;

    b.install_global(GLOBAL)
}

/// Fire every `ObjectDeath` handler registered for `guid` (one-shot handlers are removed). The engine
/// calls this when an object dies — today from the `Object.Kill` binding; later from the damage
/// solver / destruction FSM. No-op if the event system isn't installed. This is the condition-feed
/// pattern the other condition kinds (Proximity/Boundary/InSeat) will follow.
pub fn fire_object_death(lua: &Lua, guid: u64) -> LuaResult<()> {
    let mgr: Mgr = match lua.app_data_ref::<Mgr>() {
        Some(m) => (*m).clone(),
        None => return Ok(()),
    };
    let fired: Vec<(i64, Function, Vec<Value>, bool)> = {
        let m = mgr.borrow();
        m.regs
            .iter()
            .filter(|(_, r)| r.kind == KIND_OBJECT_DEATH && r.subject == Some(guid))
            .map(|(h, r)| (*h, r.callback.clone(), r.cbargs.clone(), r.persistent))
            .collect()
    };
    for (h, callback, cbargs, persistent) in fired {
        callback.call::<()>(Variadic::from_iter(cbargs))?;
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
    }
    Ok(())
}
