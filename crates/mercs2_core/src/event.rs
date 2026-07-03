//! Keystone B — the serialized event / RPC bus.
//!
//! # How this maps to the exe (behavior gated, implementation free)
//!
//! The original engine routes GUI, networking, and AI through **one** shared event bus. The
//! decompiled bodies are the oracle:
//!
//! - `ToggleHud @8255e488` (GUI) and `NetEventCallback @825d3ce8` (Net) both marshal through the
//!   same four-call **quartet**: allocate an event frame (`FUN_8241d458`) → reserve N 8-byte arg
//!   slots (`FUN_82878c50`, frame cap **2048** slots) → build-and-dispatch under a 32-bit event
//!   hash + name label (`FUN_82420690`) → finalize (`FUN_8256eb28`). AI's `DirectAction` dispatches
//!   the same way. Evidence: `docs/mercs2-pdb-analysis/gui-hud.md` §"ToggleHud is a textbook bus
//!   call", `docs/mercs2-pdb-analysis/networking.md` §NetEventCallback,
//!   `docs/modernization/pangea_engine_alignment.md` §"Keystone B".
//! - An event is identified by a **32-bit name hash** (the engine hashes the event name; the
//!   `.rdata` string is carried alongside only as a debug label). Arguments are a **typed TLV
//!   stream of 4 types**: string/guid (type 0), int, float, handle. Argument count is capped at
//!   **7** on the wire.
//!
//! This module is the modern analog: a name-hash-keyed publish/subscribe bus with typed args,
//! plus a bounded deferred queue so it can be driven once per fixed tick from the [`crate::Schedule`]
//! (Keystone C), mirroring the engine's per-frame dispatch.
//!
//! ## What is verified vs. a design choice
//! - **Verified** (from the decompiled bodies): 32-bit name-hash identity; 4 typed arg kinds;
//!   argc ≤ 7; the per-frame slot cap of 2048; the bus being shared by GUI / Net / AI.
//! - **Design choice** (not byte-verified — the exe's router `FUN_82420690` collapses to
//!   unrecovered stubs, so the on-the-wire format is unknown): the in-memory [`Event`] layout, the
//!   subscription/handler model, the immediate-vs-queued split, and treating the exe's single
//!   type-0 as two distinct Rust variants ([`EventArg::Str`] and [`EventArg::Guid`]) for clarity.
//!
//! ## Name hashing is caller-supplied
//! Like [`crate::registry`], this crate is asset-agnostic: it never hashes names itself. The event
//! name-hash is a `u32` the caller precomputes with the engine hash (`pandemic_hash_m2`, which
//! lives at the byte-decode boundary in `mercs2_formats`) so there is one implementation and no
//! drift. There is therefore intentionally no `on_name`/`emit_name(&str)` convenience here — it
//! could only guess a hash, and a hash that disagrees with the engine's would silently mis-route.

use std::collections::HashMap;
use std::collections::VecDeque;

/// Maximum arguments carried by one event. The exe caps argc at 7 on the wire
/// (`NetEventCallback`/`ToggleHud`); we enforce the same cap. **Verified.**
pub const MAX_EVENT_ARGS: usize = 7;

/// Default bound on the deferred [`EventBus`] queue. The exe reserves event args from a per-frame
/// slot pool capped at 2048 (`FUN_82878c50`); we bound the deferred event queue at the same figure
/// so a runaway producer can't grow it without limit. Overflow is dropped (see [`EventBus::queue`]).
pub const DEFAULT_QUEUE_CAP: usize = 2048;

/// A single typed event argument — the modern form of the exe's 4-type TLV arg stream.
///
/// The engine has exactly four arg types; its type-0 covers both string and GUID payloads. We keep
/// [`Str`](EventArg::Str) and [`Guid`](EventArg::Guid) as **distinct** Rust variants for clarity —
/// both map back to the exe's type-0.
#[derive(Clone, Debug, PartialEq)]
pub enum EventArg {
    /// A string payload (exe type-0).
    Str(String),
    /// A 32-bit GUID / name-hash payload (exe type-0, numeric form).
    Guid(u32),
    /// A signed integer (exe int type). Widened to `i64` in Rust; the wire form is 32-bit.
    Int(i64),
    /// A floating-point value (exe float type). Widened to `f64` in Rust; the wire form is 32-bit.
    Float(f64),
    /// An engine object handle (exe handle type).
    Handle(u32),
}

/// Why an event operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventError {
    /// Pushing this argument would exceed [`MAX_EVENT_ARGS`] (the wire argc cap of 7).
    TooManyArgs,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::TooManyArgs => {
                write!(f, "event exceeds the {MAX_EVENT_ARGS}-argument wire cap")
            }
        }
    }
}

impl std::error::Error for EventError {}

/// One event: a 32-bit name-hash identity plus up to [`MAX_EVENT_ARGS`] typed arguments.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Event {
    /// The engine name-hash identifying this event (caller-supplied; see module docs).
    pub name_hash: u32,
    /// The typed argument list. Never longer than [`MAX_EVENT_ARGS`] when built through
    /// [`Event::try_push`] / the [`EmitBuilder`]; the field is public for ergonomic reads, so use
    /// [`Event::args_len`] and the guarded pushers to keep the invariant.
    pub args: Vec<EventArg>,
}

impl Event {
    /// A new argument-less event under `name_hash`.
    pub fn new(name_hash: u32) -> Self {
        Self {
            name_hash,
            args: Vec::new(),
        }
    }

    /// Append an argument, enforcing the ≤7 wire cap. Returns `Err(TooManyArgs)` on the 8th arg,
    /// leaving the event unchanged — the strict counterpart to the builder's lenient
    /// [`EmitBuilder::arg`] (which clamps).
    pub fn try_push(&mut self, arg: EventArg) -> Result<&mut Self, EventError> {
        if self.args.len() >= MAX_EVENT_ARGS {
            return Err(EventError::TooManyArgs);
        }
        self.args.push(arg);
        Ok(self)
    }

    /// Consuming builder form of [`Event::try_push`], for chaining: `Event::new(h).with(a)?.with(b)?`.
    pub fn with(mut self, arg: EventArg) -> Result<Self, EventError> {
        self.try_push(arg)?;
        Ok(self)
    }

    /// Current argument count.
    pub fn args_len(&self) -> usize {
        self.args.len()
    }
}

/// A handle to a registered subscription, returned by [`EventBus::on`] and accepted by
/// [`EventBus::unsubscribe`]. Opaque and monotonic — ids are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubId(u64);

/// A subscriber callback. `FnMut` so a handler may carry and mutate its own state across events.
type Handler = Box<dyn FnMut(&Event)>;

/// The engine event / RPC bus: a name-hash-keyed publish/subscribe hub with typed events, an
/// immediate dispatch path, and a bounded deferred queue drained once per tick.
///
/// See the [module docs](self) for how this maps to the exe.
pub struct EventBus {
    /// name-hash → its subscribers, in registration order (which is also dispatch order).
    handlers: HashMap<u32, Vec<(SubId, Handler)>>,
    /// Events queued for the next [`dispatch_all`](EventBus::dispatch_all).
    queue: VecDeque<Event>,
    /// Upper bound on `queue`; further [`queue`](EventBus::queue) calls are dropped and counted.
    queue_cap: usize,
    /// Monotonic source of [`SubId`]s.
    next_sub: u64,
    /// How many events were dropped because the queue was full — a health signal for tuning the cap.
    dropped: u64,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// A bus with the default queue bound ([`DEFAULT_QUEUE_CAP`]).
    pub fn new() -> Self {
        Self::with_queue_cap(DEFAULT_QUEUE_CAP)
    }

    /// A bus with an explicit deferred-queue bound.
    pub fn with_queue_cap(queue_cap: usize) -> Self {
        Self {
            handlers: HashMap::new(),
            queue: VecDeque::new(),
            queue_cap,
            next_sub: 0,
            dropped: 0,
        }
    }

    /// Subscribe `handler` to every event with `name_hash`. Multiple handlers may share a hash;
    /// they fire in subscription order. Returns a [`SubId`] for later [`unsubscribe`](Self::unsubscribe).
    pub fn on<F>(&mut self, name_hash: u32, handler: F) -> SubId
    where
        F: FnMut(&Event) + 'static,
    {
        let id = SubId(self.next_sub);
        self.next_sub += 1;
        self.handlers
            .entry(name_hash)
            .or_default()
            .push((id, Box::new(handler)));
        id
    }

    /// Remove a subscription by id. Returns `true` if it existed.
    pub fn unsubscribe(&mut self, id: SubId) -> bool {
        for list in self.handlers.values_mut() {
            if let Some(pos) = list.iter().position(|(sid, _)| *sid == id) {
                let _ = list.remove(pos); // drop the boxed handler
                return true;
            }
        }
        false
    }

    /// Dispatch `event` **immediately** and synchronously to all handlers for its hash. An event
    /// whose hash has no subscribers is a silent no-op (as on the engine bus).
    pub fn emit(&mut self, event: Event) {
        self.dispatch(&event);
    }

    /// Begin building an event to `name_hash` fluently: `bus.emit_hashed(h).arg(a).arg(b).send()`.
    /// See [`EmitBuilder`] for `send` (immediate) vs `queue` (deferred).
    pub fn emit_hashed(&mut self, name_hash: u32) -> EmitBuilder<'_> {
        EmitBuilder {
            bus: self,
            event: Event::new(name_hash),
            overflowed: false,
        }
    }

    /// Enqueue `event` for deferred delivery on the next [`dispatch_all`](Self::dispatch_all).
    /// Returns `false` (and increments [`dropped_count`](Self::dropped_count)) if the queue is at
    /// [`queue_cap`](Self::with_queue_cap) — overflow is dropped, never blocked, so a stuck consumer
    /// can't stall producers.
    pub fn queue(&mut self, event: Event) -> bool {
        if self.queue.len() >= self.queue_cap {
            self.dropped += 1;
            return false;
        }
        self.queue.push_back(event);
        true
    }

    /// Drain the deferred queue in FIFO order, dispatching each event to its handlers. Returns the
    /// number of events dispatched. Events queued *during* this drain (e.g. by a handler) land in
    /// the now-empty queue and are delivered on the next call — one tick later — matching the
    /// engine's per-frame model. Drive this once per fixed tick from the [`crate::Schedule`].
    pub fn dispatch_all(&mut self) -> usize {
        let drained: VecDeque<Event> = std::mem::take(&mut self.queue);
        let n = drained.len();
        for event in drained {
            self.dispatch(&event);
        }
        n
    }

    /// Number of events currently waiting in the deferred queue.
    pub fn queued_len(&self) -> usize {
        self.queue.len()
    }

    /// Total events dropped so far due to a full queue.
    pub fn dropped_count(&self) -> u64 {
        self.dropped
    }

    /// Number of handlers registered for `name_hash`.
    pub fn handler_count(&self, name_hash: u32) -> usize {
        self.handlers.get(&name_hash).map_or(0, |l| l.len())
    }

    /// Fire an event at every handler for its hash (shared by [`emit`](Self::emit) and
    /// [`dispatch_all`](Self::dispatch_all)).
    fn dispatch(&mut self, event: &Event) {
        if let Some(list) = self.handlers.get_mut(&event.name_hash) {
            for (_id, handler) in list.iter_mut() {
                handler(event);
            }
        }
    }
}

/// Fluent builder returned by [`EventBus::emit_hashed`]. Push args with [`arg`](Self::arg), then
/// [`send`](Self::send) (immediate) or [`queue`](Self::queue) (deferred).
///
/// [`arg`](Self::arg) is **lenient**: an argument past the ≤7 cap is dropped and flagged on
/// [`overflowed`](Self::overflowed), so a fluent chain never panics or errors mid-build. Use
/// [`Event::try_push`] directly when you want a hard `Err` on overflow instead.
pub struct EmitBuilder<'a> {
    bus: &'a mut EventBus,
    event: Event,
    overflowed: bool,
}

impl<'a> EmitBuilder<'a> {
    /// Append an argument, clamping at [`MAX_EVENT_ARGS`]. A dropped arg sets [`overflowed`](Self::overflowed).
    pub fn arg(mut self, arg: EventArg) -> Self {
        if self.event.try_push(arg).is_err() {
            self.overflowed = true;
        }
        self
    }

    /// Whether any argument was dropped for exceeding the ≤7 cap.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Finish building without dispatching, yielding the [`Event`].
    pub fn build(self) -> Event {
        self.event
    }

    /// Dispatch the built event immediately (synchronous) via [`EventBus::emit`].
    pub fn send(self) {
        let EmitBuilder { bus, event, .. } = self;
        bus.emit(event);
    }

    /// Enqueue the built event for deferred delivery via [`EventBus::queue`]. Returns `false` if the
    /// queue was full (the event was dropped).
    pub fn queue(self) -> bool {
        let EmitBuilder { bus, event, .. } = self;
        bus.queue(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    // A few stand-in event hashes (in the real engine these come from pandemic_hash_m2(name),
    // e.g. ToggleHud = 0x73962eb2). Values here are opaque keys.
    const TOGGLE_HUD: u32 = 0x7396_2EB2;
    const NET_EVENT: u32 = 0x0000_1234;

    #[test]
    fn emit_reaches_handler_with_typed_args() {
        let seen = Rc::new(RefCell::new(Vec::<EventArg>::new()));
        let sink = seen.clone();
        let mut bus = EventBus::new();
        bus.on(TOGGLE_HUD, move |ev| {
            assert_eq!(ev.name_hash, TOGGLE_HUD);
            sink.borrow_mut().extend(ev.args.iter().cloned());
        });

        bus.emit_hashed(TOGGLE_HUD)
            .arg(EventArg::Int(1))
            .arg(EventArg::Handle(0xABCD))
            .send();

        // Typed args round-trip intact.
        assert_eq!(
            *seen.borrow(),
            vec![EventArg::Int(1), EventArg::Handle(0xABCD)]
        );
    }

    #[test]
    fn all_four_arg_kinds_round_trip() {
        let got = Rc::new(RefCell::new(Vec::new()));
        let sink = got.clone();
        let mut bus = EventBus::new();
        bus.on(NET_EVENT, move |ev| sink.borrow_mut().extend(ev.args.iter().cloned()));

        bus.emit_hashed(NET_EVENT)
            .arg(EventArg::Str("hello".into()))
            .arg(EventArg::Guid(0xDEAD_BEEF))
            .arg(EventArg::Int(-7))
            .arg(EventArg::Float(1.5))
            .arg(EventArg::Handle(42))
            .send();

        assert_eq!(
            *got.borrow(),
            vec![
                EventArg::Str("hello".into()),
                EventArg::Guid(0xDEAD_BEEF),
                EventArg::Int(-7),
                EventArg::Float(1.5),
                EventArg::Handle(42),
            ]
        );
    }

    #[test]
    fn multiple_handlers_per_hash_fire_in_order() {
        let order = Rc::new(RefCell::new(Vec::new()));
        let (a, b) = (order.clone(), order.clone());
        let mut bus = EventBus::new();
        bus.on(TOGGLE_HUD, move |_| a.borrow_mut().push(1));
        bus.on(TOGGLE_HUD, move |_| b.borrow_mut().push(2));

        bus.emit(Event::new(TOGGLE_HUD));

        assert_eq!(*order.borrow(), vec![1, 2]);
        assert_eq!(bus.handler_count(TOGGLE_HUD), 2);
    }

    #[test]
    fn unknown_hash_is_a_noop() {
        let mut bus = EventBus::new();
        let count = Rc::new(RefCell::new(0));
        let c = count.clone();
        bus.on(TOGGLE_HUD, move |_| *c.borrow_mut() += 1);

        // No handler for this hash — must not panic and must not touch the TOGGLE_HUD handler.
        bus.emit(Event::new(0xFFFF_0000));
        assert_eq!(*count.borrow(), 0);
    }

    #[test]
    fn seven_arg_guard_strict_and_lenient() {
        // Strict path: the 8th try_push errors, leaving the event at 7 args.
        let mut ev = Event::new(NET_EVENT);
        for i in 0..MAX_EVENT_ARGS {
            assert!(ev.try_push(EventArg::Int(i as i64)).is_ok());
        }
        assert_eq!(ev.args_len(), MAX_EVENT_ARGS);
        assert_eq!(ev.try_push(EventArg::Int(99)), Err(EventError::TooManyArgs));
        assert_eq!(ev.args_len(), MAX_EVENT_ARGS);

        // Lenient path: the builder clamps and flags overflow rather than erroring.
        let mut bus = EventBus::new();
        let b = bus
            .emit_hashed(NET_EVENT)
            .arg(EventArg::Int(0))
            .arg(EventArg::Int(1))
            .arg(EventArg::Int(2))
            .arg(EventArg::Int(3))
            .arg(EventArg::Int(4))
            .arg(EventArg::Int(5))
            .arg(EventArg::Int(6))
            .arg(EventArg::Int(7)); // 8th — dropped
        assert!(b.overflowed());
        assert_eq!(b.build().args_len(), MAX_EVENT_ARGS);
    }

    #[test]
    fn queue_defers_and_dispatch_all_drains_in_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let mut bus = EventBus::new();
        bus.on(NET_EVENT, move |ev| {
            if let Some(EventArg::Int(n)) = ev.args.first() {
                sink.borrow_mut().push(*n);
            }
        });

        // Queued events do not fire until dispatch_all.
        for n in 0..5 {
            assert!(bus.emit_hashed(NET_EVENT).arg(EventArg::Int(n)).queue());
        }
        assert!(log.borrow().is_empty());
        assert_eq!(bus.queued_len(), 5);

        let dispatched = bus.dispatch_all();
        assert_eq!(dispatched, 5);
        assert_eq!(*log.borrow(), vec![0, 1, 2, 3, 4]); // FIFO order
        assert_eq!(bus.queued_len(), 0);

        // Second drain of an empty queue is a no-op.
        assert_eq!(bus.dispatch_all(), 0);
    }

    #[test]
    fn queue_is_bounded_and_counts_drops() {
        let mut bus = EventBus::with_queue_cap(2);
        assert!(bus.queue(Event::new(NET_EVENT)));
        assert!(bus.queue(Event::new(NET_EVENT)));
        assert!(!bus.queue(Event::new(NET_EVENT))); // full → dropped
        assert_eq!(bus.dropped_count(), 1);
        assert_eq!(bus.queued_len(), 2);
    }

    #[test]
    fn unsubscribe_stops_delivery() {
        let hits = Rc::new(RefCell::new(0));
        let c = hits.clone();
        let mut bus = EventBus::new();
        let id = bus.on(TOGGLE_HUD, move |_| *c.borrow_mut() += 1);

        bus.emit(Event::new(TOGGLE_HUD));
        assert_eq!(*hits.borrow(), 1);

        assert!(bus.unsubscribe(id));
        bus.emit(Event::new(TOGGLE_HUD));
        assert_eq!(*hits.borrow(), 1); // no further delivery
        assert!(!bus.unsubscribe(id)); // already gone
    }

    #[test]
    fn handler_queued_event_delivers_next_dispatch() {
        // A handler that re-queues must not deadlock or recurse; the new event lands next tick.
        let rounds = Rc::new(RefCell::new(Vec::new()));
        let sink = rounds.clone();
        let mut bus = EventBus::new();
        bus.emit_hashed(NET_EVENT).arg(EventArg::Int(1)).queue();
        // (handler can't hold &mut bus, so re-queue is modeled by the driver below)

        let n1 = bus.dispatch_all();
        sink.borrow_mut().push(n1);
        // Simulate a follow-up produced after the first drain.
        bus.emit_hashed(NET_EVENT).arg(EventArg::Int(2)).queue();
        let n2 = bus.dispatch_all();
        sink.borrow_mut().push(n2);

        assert_eq!(*rounds.borrow(), vec![1, 1]);
    }

    #[test]
    fn driven_from_a_schedule_once_per_tick() {
        // Wiring check: the bus drains exactly once per fixed tick when driven from a system,
        // mirroring the engine's per-frame dispatch (Keystone C × Keystone B).
        use crate::{Schedule, Time, World};
        use std::rc::Rc;

        let ticks = Rc::new(RefCell::new(0u32));
        let bus = Rc::new(RefCell::new(EventBus::new()));
        let seen = Rc::new(RefCell::new(0u32));

        let s = seen.clone();
        bus.borrow_mut().on(NET_EVENT, move |_| *s.borrow_mut() += 1);

        // Producer + drainer wired as a system driven by the fixed-tick Schedule.
        let (bus_p, ticks_p) = (bus.clone(), ticks.clone());
        let mut sched = Schedule::new();
        sched.add_system("events", move |_w: &mut World, _t: &Time| {
            let mut b = bus_p.borrow_mut();
            b.emit_hashed(NET_EVENT).arg(EventArg::Int(0)).queue();
            b.dispatch_all();
            *ticks_p.borrow_mut() += 1;
        });

        let mut world = World::new();
        let mut time = Time::new(60.0);
        let steps = sched.run_fixed(&mut world, &mut time, 3.0 / 60.0 + 0.0005); // ~3 fixed steps
        assert_eq!(steps, 3);
        assert_eq!(*ticks.borrow(), 3);
        assert_eq!(*seen.borrow(), 3); // one event delivered per tick
    }
}
