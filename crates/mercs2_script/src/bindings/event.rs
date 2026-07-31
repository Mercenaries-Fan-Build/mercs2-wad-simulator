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
    // ObjectHibernation: the phase on `subject` this handler waits for (`"awake"` / `"asleep"`).
    phase: Option<String>,
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
    // Set only by the ObjectInSeat arm; kept out of the tuple, which is already seven wide.
    let mut in_seat: Option<InSeatFilter> = None;
    let (script_name, filter, timer_remaining, timer_period, subject, phase, state_match) = if kind == KIND_SCRIPT_EVENT {
        // params = { name, [filter_fn] }
        (params.get::<String>(1).ok(), params.get::<Option<Function>>(2)?, None, None, None, None, None)
    } else if kind == KIND_TIMER_RELATIVE {
        // params = { seconds }
        let secs: f32 = params.get(1).unwrap_or(0.0);
        (None, None, Some(secs), Some(secs), None, None, None)
    } else if kind == KIND_OBJECT_DEATH {
        // params = { guid } — fired by the engine when that object dies (Object.Kill / damage).
        // The handle inside the params table is whatever the producing binding pushed, i.e. now
        // lightuserdata; reading it as `Guid` accepts that (and, transitionally, an integer).
        let g = params.get::<Guid>(1).unwrap_or(Guid::NONE);
        (None, None, None, None, g.opt(), None, None)
    } else if kind == KIND_OBJECT_HIBERNATION {
        // params = { guid, phase } — fired by the streaming system when that object wakes/sleeps.
        // The awake-gate every real object script opens with:
        //   Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupEvents, {uGuid})
        // The phase is CANONICALISED here — see [`canon_phase`]; the corpus spells two phases five
        // different ways and storing them verbatim made 28 of 109 registrations unmatchable.
        let g = params.get::<Guid>(1).unwrap_or(Guid::NONE);
        let ph = params
            .get::<String>(2)
            .ok()
            .map(|s| canon_phase(&s).map(str::to_string).unwrap_or(s));
        (None, None, None, None, g.opt(), ph, None)
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
        (None, None, None, None, occ.opt(), None, None)
    } else if kind == KIND_GAME_STATE_CHANGE {
        // params = { stateName, phase } — fired by the engine's state machine (e.g. {"WaitForStreaming","exit"}).
        let st = params.get::<String>(1).ok();
        let ph = params.get::<String>(2).ok();
        (None, None, None, None, None, None, st.map(|s| (s, ph.unwrap_or_default())))
    } else {
        (None, None, None, None, None, None, None)
    };
    let mut m = mgr.borrow_mut();
    m.next += 1;
    let h = m.next;
    m.regs.insert(
        h,
        EventReg { kind, persistent, callback, cbargs, script_name, filter, timer_remaining, timer_period, subject, phase, in_seat, state_match },
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

    // Engine-driven timer pump (not a game cfunc — the render/sim loop calls this each tick).
    let m = mgr.clone();
    b.extra("__pump", lua.create_function(move |_, dt: f32| pump(&m, dt))?)?;

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
        callback.call::<()>(Variadic::from_iter(cbargs))?;
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
        callback.call::<()>(Variadic::from_iter(cbargs))?;
        if !persistent {
            mgr.borrow_mut().regs.remove(&h);
        }
    }
    Ok(())
}

/// The canonical `ObjectHibernation` phase for whatever spelling a script used.
///
/// # Why this is needed
///
/// The shipped corpus spells the phase **five** ways across 109 registrations:
///
/// | spelling | sites | meaning |
/// |---|--:|---|
/// | `"awake"` | 80 | object woke |
/// | `"a"` | 4 | object woke |
/// | `"hibernated"` | 19 | object went to sleep |
/// | `"s"` | 5 | object went to sleep |
/// | `"asleep"` | 1 | object went to sleep |
///
/// That there are exactly **two** phases is settled by what the handlers do, not by the strings:
/// every `"s"`/`"hibernated"`/`"asleep"` site is a sleep handler (`_OnAsleep` `wifpmcgarage.lua:364`,
/// `_OnHqHibernation` `mrxhqmanager.lua:185`, `_OnPmcHibernation` `wifpmcinterior.lua:2069`, and
/// `Object.Remove` on stream-out at `oilcon001.lua:1727`/`pircon001.lua:193`), and every
/// `"a"`/`"awake"` site is a wake handler.
///
/// Storing the raw string and comparing it exactly — which is what this did before — meant a producer
/// firing any single spelling could match at most one group. **28 of the 109 registrations could never
/// fire**, including `vzacon001.lua:120`, the boat gate the whole world-load state machine waits on.
///
/// # Confidence: the grouping is proven, the retail predicate is NOT recovered
///
/// `event_bus_code_map.md:36` gets as far as "installs match predicate via `vt[0](filter_args)`" and
/// does not disassemble the per-type predicate, so retail's actual comparison is unknown. What is
/// certain is that it is **not** an exact match (that would break 28 shipped sites) and **not** a
/// first-character match (`"awake"` and `"asleep"` share `'a'` yet mean opposites).
///
/// This maps by author intent, which is the observable ground truth. The one place that could differ
/// from retail is `"asleep"` — the single outlier, at `gurcon002.lua:194`. If retail happens to
/// first-char match, that site registers as *awake* and is a shipped bug; we would then be fixing a
/// bug rather than reproducing it. One site, flagged in `DEFERRED.md`, needs a live read of the
/// predicate to settle. Do not raise this to a claim of oracle grounding.
///
/// Returns `None` for a spelling not in the table above. The caller then stores the raw string, so an
/// unknown phase keeps the old exact-match behaviour instead of being forced into a bucket it may not
/// belong to — guessing which of two opposite meanings a novel spelling has is exactly the error this
/// function exists to fix.
pub fn canon_phase(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "a" | "awake" => Some(PHASE_AWAKE),
        "s" | "asleep" | "hibernated" => Some(PHASE_ASLEEP),
        _ => None,
    }
}

/// Canonical "the object woke" phase. Producers fire this; [`canon_phase`] folds `"a"`/`"awake"` onto it.
pub const PHASE_AWAKE: &str = "awake";
/// Canonical "the object went to sleep / streamed out" phase. [`canon_phase`] folds
/// `"s"`/`"asleep"`/`"hibernated"` onto it.
pub const PHASE_ASLEEP: &str = "hibernated";

/// Fire every `ObjectHibernation` handler registered for `(guid, phase)` — the streaming system calls
/// this when an object wakes (`"awake"`) or hibernates (`"asleep"`). This is the condition behind the
/// awake-gate that opens essentially every object script in the corpus:
/// `Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupEvents, {uGuid})` — `OnActivate` runs
/// while the object is still asleep, so real setup has to wait for this.
pub fn fire_object_hibernation(lua: &Lua, guid: u64, phase: &str) -> LuaResult<()> {
    // Canonicalise the PRODUCER's spelling too, not just the registration's. Callers reasonably pass
    // whatever the corpus reads like at the call site they are mirroring (`"a"`, `"awake"`), and a
    // producer saying `"a"` against a registration canonicalised to `"awake"` would match nothing —
    // the same failure this canonicalisation exists to remove, just moved to the other side.
    let phase = canon_phase(phase).unwrap_or(phase);
    let mgr: Mgr = match lua.app_data_ref::<Mgr>() {
        Some(m) => (*m).clone(),
        None => return Ok(()),
    };
    let fired: Vec<(i64, Function, Vec<Value>, bool)> = {
        let m = mgr.borrow();
        m.regs
            .iter()
            .filter(|(_, r)| {
                r.kind == KIND_OBJECT_HIBERNATION
                    && r.subject == Some(guid)
                    && r.phase.as_deref() == Some(phase)
            })
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
        callback.call::<()>(Variadic::from_iter(cbargs))?;
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
mod phase_tests {
    use super::*;

    /// Both spellings of each phase fold together, and the two phases stay apart.
    ///
    /// The separation matters more than the folding: `"awake"` and `"asleep"` differ by three letters
    /// in the middle and share a first character, so any matcher keyed on a prefix would collapse
    /// opposites — a wake handler firing on stream-OUT would run mission setup for an object that just
    /// left the world.
    #[test]
    fn the_five_corpus_spellings_fold_to_two_phases() {
        for wake in ["a", "awake", "AWAKE", " Awake "] {
            assert_eq!(canon_phase(wake), Some(PHASE_AWAKE), "{wake:?} means awake");
        }
        for sleep in ["s", "asleep", "hibernated", "HIBERNATED"] {
            assert_eq!(canon_phase(sleep), Some(PHASE_ASLEEP), "{sleep:?} means asleep");
        }
        assert_ne!(PHASE_AWAKE, PHASE_ASLEEP, "the two phases must never compare equal");
        // The canonical forms are themselves stable under folding, so a producer firing a canonical
        // phase matches a registration that was already canonicalised.
        assert_eq!(canon_phase(PHASE_AWAKE), Some(PHASE_AWAKE));
        assert_eq!(canon_phase(PHASE_ASLEEP), Some(PHASE_ASLEEP));
    }

    /// An unrecognised spelling is reported as unknown rather than guessed into a bucket. Registration
    /// then keeps the raw string, preserving the old exact-match behaviour for it.
    #[test]
    fn an_unknown_spelling_is_not_guessed() {
        for odd in ["", "wake", "sleeping", "dormant", "0"] {
            assert_eq!(canon_phase(odd), None, "{odd:?} is not a spelling we have evidence for");
        }
    }
}
