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

use mercs2_luac::rt::{Function, IntoLua, Lua, Result as LuaResult, Table, Value, Variadic};

use super::{Installed, NsBuilder, Required};
use crate::attribution::LuaFnExt;
use crate::{Guid, SharedHost};

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
    // Remaining engine condition/event kinds the game registers handlers for. The engine feeds these
    // conditions as the corresponding runtime state comes online; until then the handler is registered
    // (a valid non-nil `Event.X` constant) and simply stays dormant. Distinct stable ids (13+); only our
    // own Create/pump compares them. Sourced from every `Event.<Kind>` the Lua corpus references.
    ("Timer", 13),
    ("WeaponEvent", 14),
    ("ButtonPress", 15),
    ("ButtonReleased", 16),
    ("HumanStateTransition", 17),
    ("HumanActionComplete", 18),
    ("HumanAnimationNearlyCompleted", 19),
    ("AnimationEvent", 20),
    ("ObjectHealth", 21),
    ("ObjectHealthLessThan", 22),
    ("ObjectDelete", 23),
    ("ObjectIsReady", 24),
    ("ObjectIsGrounded", 25),
    ("ObjectIsVisible", 26),
    ("ObjectWinched", 27),
    ("PrimaryClipSize", 28),
    ("PrimaryCurrentAmmo", 29),
    ("PrimaryStoredAmmo", 30),
    ("ExplosivesCurrentAmmo", 31),
    ("ExplosivesStoredAmmo", 32),
    ("FactionTexture", 33),
    ("Minigame", 34),
    ("GuiGameTimer", 35),
    ("GuiUpdate", 36),
    ("Cash", 37),
    ("Player", 38),
    ("PosX", 39),
    ("PosZ", 40),
];

/// The `Event.ObjectInSeat` filter: `{occupant, vehicle, seat, action}`.
///
/// # The vocabulary, from the corpus (42 sites)
///
/// * **occupant** — a character guid. `0`, an unreadable value, or the string `"Hero"`
///   (`wifpmcgarage.lua:410`) mean *any occupant*.
/// * **vehicle** — a vehicle guid; `0` means *any vehicle*. `wifpmcgarage.lua:472` registers
///   `{uCharacter, 0, "d", "x"}` to catch that character leaving the driver seat of **anything**.
/// * **seat** — `"d"` driver or `"p"` passenger, case-insensitive. `"a"` is a **wildcard**: it appears
///   27 times in this filter and *never* as a real seat (`Vehicle.Enter` and `Vehicle.GetSeatByType`
///   only ever pass `"d"`/`"p"`), so it cannot be a seat type. INFERRED from that distribution, not
///   read from the exe.
/// * **action** — `"e"`/`"E"` enter, `"x"` exit. Case-insensitive.
#[derive(Debug, Clone, Default)]
struct InSeatFilter {
    /// `None` = match any vehicle.
    vehicle: Option<u64>,
    /// Lowercased seat code; `None` = match any seat.
    seat: Option<String>,
    /// Lowercased action (`"e"` / `"x"`); `None` = match either.
    action: Option<String>,
}

/// An `Event.ObjectHibernation` condition (factory `0x5F2530`, parse `0x5F2300`, poll
/// `FUN_005F2440`). The script update polls it every frame (`docs/reverse_engineer/hibernation_grid.md`
/// §6a); it is not raised by a transition.
#[derive(Debug, Clone, PartialEq)]
enum HibernationSubject {
    /// A non-zero guid: the entity test.
    Guid(u64),
    /// No guid, a layer name: the layer form.
    Layer(String),
}

#[derive(Debug, Clone, PartialEq)]
struct HibernationCondition {
    subject: HibernationSubject,
    /// The activation state it waits for: 2 when the phase's first letter, case-folded, is `'a'`
    /// (and when there is no phase); 0 for anything else. So `"awake"`/`"a"` wait for awake,
    /// `"hibernated"`/`"s"` for asleep — and the one `"asleep"` site (`gurcon002.lua:194`) waits for
    /// awake, as in retail.
    state: u8,
    /// The optional model detail level (`+0x2C`); only the awake form keeps one.
    detail: Option<i32>,
}

/// Parse `{[layer name], [guid], [phase], [detail]}` the way `0x5F2300` reads its arguments: a
/// string (or number) first is a layer name; a guid is taken only from a lightuserdata/userdata and
/// moves the index on only when read; the phase defaults to `"a"`; the detail is a number truncated
/// toward zero. `None` when there is neither a guid nor a layer name (`Event.Create` returns nil).
fn parse_hibernation(params: &Table) -> LuaResult<Option<HibernationCondition>> {
    let mut i = 1;
    let layer = match params.get::<Value>(i)? {
        Value::String(_) | Value::Number(_) => {
            i += 1;
            Some(params.get::<String>(1)?)
        }
        _ => None,
    };
    let guid = match params.get::<Value>(i)? {
        Value::LightUserData(_) => {
            let g = params.get::<Guid>(i)?.raw();
            i += 1;
            g
        }
        // Retail also reads a full userdata's handle (tag 7, `+0x18`); this host never makes one.
        Value::Other(_, "userdata") => {
            return Err(mercs2_luac::rt::Error::RuntimeError(
                "Event.ObjectHibernation: a full-userdata guid is not modelled by this host".into(),
            ))
        }
        _ => 0,
    };
    let phase = match params.get::<Value>(i)? {
        Value::String(_) => {
            let p = params.get::<String>(i)?;
            i += 1;
            p
        }
        _ => "a".to_string(),
    };
    let state = if phase.bytes().next().is_some_and(|c| c.eq_ignore_ascii_case(&b'a')) { 2 } else { 0 };
    // `cvttss2si` on the single-precision value (`0x59F886`).
    let detail = match params.get::<Value>(i)? {
        Value::Number(n) if state == 2 => Some(n as f32 as i32),
        _ => None,
    };
    let subject = match (guid, layer) {
        (0, None) => return Ok(None),
        (0, Some(l)) => HibernationSubject::Layer(l),
        (g, _) => HibernationSubject::Guid(g),
    };
    Ok(Some(HibernationCondition { subject, state, detail }))
}

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
    // ObjectHibernation: what the polled condition tests — see [`HibernationCondition`].
    hibernation: Option<HibernationCondition>,
    // ObjectInSeat: the `{occupant, vehicle, seat, action}` filter. `subject` holds the occupant.
    in_seat: Option<InSeatFilter>,
    // GameStateChange: the `(stateName, phase)` this handler waits for (e.g. `("WaitForStreaming",
    // "exit")`), fired by the engine's state machine via `fire_game_state_change`.
    state_match: Option<(String, String)>,
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

/// `Event.Create` / `Event.CreatePersistent` body: register a handler, return its integer handle
/// (`None` = the condition's arguments were rejected and nothing was registered → nil).
fn make(
    mgr: &Mgr,
    kind: i64,
    params: Table,
    callback: Function,
    cbargs: Option<Table>,
    persistent: bool,
) -> LuaResult<Option<i64>> {
    let cbargs = seq_values(&cbargs)?;
    // Set only by the ObjectInSeat / ObjectHibernation arms; kept out of the tuple.
    let mut in_seat: Option<InSeatFilter> = None;
    let mut hibernation: Option<HibernationCondition> = None;
    let (script_name, filter, timer_remaining, timer_period, subject, state_match) = if kind == KIND_SCRIPT_EVENT {
        // params = { name, [filter_fn] }
        (params.get::<String>(1).ok(), params.get::<Option<Function>>(2)?, None, None, None, None)
    } else if kind == KIND_TIMER_RELATIVE {
        // params = { seconds }
        let secs: f32 = params.get(1).unwrap_or(0.0);
        (None, None, Some(secs), Some(secs), None, None)
    } else if kind == KIND_OBJECT_DEATH {
        // params = { guid } — fired by the engine when that object dies (Object.Kill / damage).
        // The handle inside the params table is whatever the producing binding pushed, i.e. now
        // lightuserdata; reading it as `Guid` accepts that (and, transitionally, an integer).
        let g = params.get::<Guid>(1).unwrap_or(Guid::NONE);
        (None, None, None, None, g.opt(), None)
    } else if kind == KIND_OBJECT_HIBERNATION {
        // params = { [layer name], [guid], [phase], [detail] } — polled by `__pump` against the
        // host, see [`HibernationCondition`]. The awake-gate every real object script opens with:
        //   Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupEvents, {uGuid})
        // Neither a guid nor a layer name: no handler, and `Create` returns nil.
        let Some(c) = parse_hibernation(&params)? else { return Ok(None) };
        hibernation = Some(c);
        (None, None, None, None, None, None)
    } else if kind == KIND_OBJECT_IN_SEAT {
        // params = { occupant, vehicle, seat, action } — see [`InSeatFilter`] for the vocabulary.
        //
        // The occupant is read as a `Guid`, so the string `"Hero"` (`wifpmcgarage.lua:410`) and a
        // missing value both land as `Guid::NONE` → any-occupant. That is a deliberate widening: this
        // module cannot resolve "Hero" without the host, and with a single local player "the hero" and
        // "any character" select the same object. In split-screen co-op they would not — recorded in
        // `DEFERRED.md`.
        let occ = params.get::<Guid>(1).unwrap_or(Guid::NONE);
        let veh = params.get::<Guid>(2).unwrap_or(Guid::NONE);
        let f = InSeatFilter {
            vehicle: veh.opt(),
            seat: params.get::<String>(3).ok().map(|s| s.to_ascii_lowercase()).filter(|s| s != "a"),
            action: params.get::<String>(4).ok().map(|s| s.to_ascii_lowercase()).filter(|s| !s.is_empty()),
        };
        in_seat = Some(f);
        (None, None, None, None, occ.opt(), None)
    } else if kind == KIND_GAME_STATE_CHANGE {
        // params = { stateName, phase } — fired by the engine's state machine (e.g. {"WaitForStreaming","exit"}).
        let st = params.get::<String>(1).ok();
        let ph = params.get::<String>(2).ok();
        (None, None, None, None, None, st.map(|s| (s, ph.unwrap_or_default())))
    } else {
        (None, None, None, None, None, None)
    };
    let mut m = mgr.borrow_mut();
    m.next += 1;
    let h = m.next;
    m.regs.insert(
        h,
        EventReg { kind, persistent, callback, cbargs, script_name, filter, timer_remaining, timer_period, subject, hibernation, in_seat, state_match },
    );
    Ok(Some(h))
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
            Some(f) => f
                .call_attr::<bool>(data.clone(), "Event.ScriptEvent.Filter")
                .unwrap_or(false),
            None => true,
        };
        if pass {
            callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.ScriptEvent")?;
            if !persistent {
                mgr.borrow_mut().regs.remove(&h);
            }
        }
    }
    Ok(())
}

/// Poll every `ObjectHibernation` condition against the host and fire the ones that hold; one-shot
/// handlers are removed (`FUN_005EB560`). The host borrow is dropped before any callback runs.
fn poll_hibernation(mgr: &Mgr, host: &SharedHost) -> LuaResult<()> {
    let regs: Vec<(i64, HibernationCondition)> = {
        let m = mgr.borrow();
        m.regs.iter().filter_map(|(h, r)| r.hibernation.clone().map(|c| (*h, c))).collect()
    };
    let due: Vec<i64> = {
        let host = host.borrow();
        regs.into_iter()
            .filter(|(_, c)| match &c.subject {
                HibernationSubject::Guid(g) => host.object_hibernation_passes(*g, c.state, c.detail),
                HibernationSubject::Layer(l) => host.layer_hibernation_passes(l, c.state, c.detail),
            })
            .map(|(h, _)| h)
            .collect()
    };
    for h in due {
        // A callback fired earlier this poll may have deleted this handler.
        let Some((callback, cbargs, persistent)) = mgr.borrow().regs.get(&h).map(|r| (r.callback.clone(), r.cbargs.clone(), r.persistent)) else {
            continue;
        };
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
        callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.ObjectHibernation")?;
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
        callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.TimerRelative")?;
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
    //
    // Two deliberate non-`Guid` types here (see the nil-handle contract in `super`):
    // - `kind` is a **constant** (`Event.ScriptEvent`, …), not an engine handle. A nil there is a real
    //   script bug — a misspelled `Event.Xxx` — and must keep raising rather than silently registering
    //   a handler under kind 0.
    // - the returned event handle is minted by this module's own `EventManager`, not by the engine's
    //   GUID allocator. The corpus only ever stores it and hands it back to `Event.Delete`
    //   (`e = Event.Delete(e)`); no call site type-checks it, so there is no evidence it is a
    //   lightuserdata in retail and no reason to make it one here.
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

    // Engine-driven timer pump (not a game cfunc — the render/sim loop calls this each tick). It is
    // also the one place game time advances: the clock moves first, so a timer callback that reads
    // `Sys.MainTime` sees the tick it fired on. Then the polled conditions.
    let m = mgr.clone();
    let clock = super::script_clock(lua);
    let h = host.clone();
    b.extra(
        "__pump",
        lua.create_function(move |_, dt: f32| {
            clock.advance(dt as f64);
            pump(&m, dt)?;
            poll_hibernation(&m, &h)
        })?,
    )?;

    b.install_global(GLOBAL)
}

/// Fire every `ObjectDeath` handler registered for `guid` (one-shot handlers are removed). The engine
/// calls this when an object dies — today from the `Object.Kill` binding; later from the damage
/// solver / destruction FSM. No-op if the event system isn't installed. This is the condition-feed
/// pattern the other condition kinds (Proximity/Boundary/InSeat) will follow.
/// Fire every `GameStateChange` handler waiting on `(state, phase)` — the engine's state machine
/// calls this when a requested game state reaches that phase (e.g. `("WaitForStreaming", "exit")`),
/// advancing the `MrxState` load chain (`_StateComplete` → next state → … → GlobalEnter/GlobalExit).
pub fn fire_game_state_change(lua: &Lua, state: &str, phase: &str) -> LuaResult<()> {
    let mgr: Mgr = match lua.app_data_ref::<Mgr>() {
        Some(m) => (*m).clone(),
        None => return Ok(()),
    };
    let fired: Vec<(i64, Function, Vec<Value>, bool)> = {
        let m = mgr.borrow();
        m.regs
            .iter()
            .filter(|(_, r)| {
                r.kind == KIND_GAME_STATE_CHANGE
                    && r.state_match.as_ref().is_some_and(|(s, p)| s == state && p == phase)
            })
            .map(|(h, r)| (*h, r.callback.clone(), r.cbargs.clone(), r.persistent))
            .collect()
    };
    for (h, callback, cbargs, persistent) in fired {
        callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.GameStateChange")?;
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
    }
    Ok(())
}

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
        callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.ObjectDeath")?;
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
    }
    Ok(())
}


/// Fire every `ObjectInSeat` handler matching `(occupant, vehicle, seat, action)` — the engine calls
/// this when a character takes or leaves a seat (`Vehicle.Enter` / `Vehicle.Exit`).
///
/// `action` is `"e"` (entered) or `"x"` (exited); `seat` is the real seat code (`"d"`/`"p"`), never the
/// `"a"` wildcard — wildcards belong to the *filter*, not to the event.
///
/// # Callback arguments
///
/// The handler is called with its registered `cbargs` **followed by `(occupant, vehicle)`**. That is
/// not a guess: `wifpmcgarage.lua` registers `_OnVehicleExit` with `{vRegion, nSlot}` and declares
/// `function _OnVehicleExit(vRegion, nSlot, uCharacter, uVehicle)` (`:523`) — two cbargs, then the two
/// appended values, in that order. It is consistent with `vzacon001.lua`'s
/// `EnsureHeroesInBoat(self, uOccupant)` registered with `{self}`, which simply ignores the trailing
/// vehicle the way Lua ignores any extra argument.
pub fn fire_object_in_seat(
    lua: &Lua,
    occupant: u64,
    vehicle: u64,
    seat: &str,
    action: &str,
) -> LuaResult<()> {
    let mgr: Mgr = match lua.app_data_ref::<Mgr>() {
        Some(m) => (*m).clone(),
        None => return Ok(()),
    };
    let seat = seat.to_ascii_lowercase();
    let action = action.to_ascii_lowercase();
    let fired: Vec<(i64, Function, Vec<Value>, bool)> = {
        let m = mgr.borrow();
        m.regs
            .iter()
            .filter(|(_, r)| {
                if r.kind != KIND_OBJECT_IN_SEAT {
                    return false;
                }
                let Some(f) = r.in_seat.as_ref() else { return false };
                // `None` on any field is the wildcard — see `InSeatFilter`.
                r.subject.is_none_or(|s| s == occupant)
                    && f.vehicle.is_none_or(|v| v == vehicle)
                    && f.seat.as_deref().is_none_or(|s| s == seat)
                    && f.action.as_deref().is_none_or(|a| a == action)
            })
            .map(|(h, r)| (*h, r.callback.clone(), r.cbargs.clone(), r.persistent))
            .collect()
    };
    for (h, callback, mut cbargs, persistent) in fired {
        // Through `Guid`, not a hand-built `LightUserData`: that is the one place the handle→Lua
        // representation is defined (non-zero → lightuserdata, 0 → nil), and the scripts receiving
        // these values type-check them with `type(u) == "userdata"`.
        cbargs.push(Guid::from(occupant).into_lua(lua)?);
        cbargs.push(Guid::from(vehicle).into_lua(lua)?);
        callback.call_attr::<()>(Variadic::from_iter(cbargs), "Event.ObjectInSeat")?;
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
    }
    Ok(())
}

/// Number of event handlers still registered. The engine doesn't need this; tooling does — a script
/// that re-registers on every stream-in without `Event.Delete`ing on stream-out shows up here as a
/// count that climbs each cycle (the duplicate-handler leak).
pub fn live_handle_count(lua: &Lua) -> usize {
    match lua.app_data_ref::<Mgr>() {
        Some(m) => m.borrow().regs.len(),
        None => 0,
    }
}

#[cfg(test)]
mod hibernation_parse_tests {
    use super::*;

    fn parse(lua: &Lua, src: &str) -> Option<HibernationCondition> {
        lua.globals().set("__g", Guid(0x4000_0010)).unwrap();
        let t: Table = lua.load(&format!("return {src}")).eval().unwrap();
        parse_hibernation(&t).unwrap()
    }

    /// The phase's first letter, case-folded, picks the state — so `"asleep"` waits for AWAKE, as
    /// retail's `tolower(phase[0]) == 'a'` (`0x5F240B`) does — and a missing phase means awake.
    #[test]
    fn the_phase_is_its_first_letter() {
        let lua = Lua::new().unwrap();
        for (p, state) in [("\"awake\"", 2), ("\"a\"", 2), ("\"Awake\"", 2), ("\"asleep\"", 2), ("\"hibernated\"", 0), ("\"s\"", 0)] {
            let c = parse(&lua, &format!("{{__g, {p}}}")).unwrap();
            assert_eq!((c.subject, c.state), (HibernationSubject::Guid(0x4000_0010), state), "{p}");
        }
        assert_eq!(parse(&lua, "{__g}").unwrap().state, 2, "no phase: awake");
    }

    /// The detail is the argument after the phase, truncated toward zero, and only the awake form
    /// keeps it (`0x5F241A` forces -1 otherwise).
    #[test]
    fn the_detail_follows_the_phase_and_only_for_awake() {
        let lua = Lua::new().unwrap();
        assert_eq!(parse(&lua, "{__g, \"awake\", 2.9}").unwrap().detail, Some(2));
        assert_eq!(parse(&lua, "{__g, \"awake\"}").unwrap().detail, None);
        assert_eq!(parse(&lua, "{__g, \"hibernated\", 2}").unwrap().detail, None);
    }

    /// A string first is a layer name; the guid read after it does not consume a non-guid argument,
    /// so the phase is the second argument. Neither guid nor layer: no condition.
    #[test]
    fn the_layer_form_and_the_empty_form() {
        let lua = Lua::new().unwrap();
        let c = parse(&lua, "{\"Vz_State_X\", \"hibernated\"}").unwrap();
        assert_eq!((c.subject, c.state), (HibernationSubject::Layer("Vz_State_X".into()), 0));
        let c = parse(&lua, "{\"Vz_State_X\", __g}").unwrap();
        assert_eq!(c.subject, HibernationSubject::Guid(0x4000_0010), "a guid wins over the layer");
        assert_eq!(parse(&lua, "{nil, \"awake\"}"), None);
    }
}
