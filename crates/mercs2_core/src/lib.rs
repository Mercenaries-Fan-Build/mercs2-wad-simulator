//! `mercs2_core` — the simulation spine of the Mercenaries 2 reimplementation.
//!
//! This crate is **renderer- and asset-agnostic**: it owns the entity/component `World`, the
//! fixed-timestep clock, and the ordered system schedule. It deliberately mirrors the original
//! engine's architecture (reflection-registered components + a fixed update tick with a *defined*
//! system order). The ECS storage is provided by `hecs` purely as an implementation substrate —
//! the *shape* follows the game, and the exe is the oracle for observable behavior.
//!
//! Canonical space ≡ game space: left-handed, +Y up, +Z north, +X east
//! (docs/coordinate_systems.md). The asset-load basis transform is the identity.
//!
//! # Module map
//!
//! At the crate root: the `hecs` [`World`]/[`Entity`] and `glam` re-exports, the fixed-step clock
//! [`Time`], the ordered [`Schedule`], and the hot-path components ([`Transform`], [`ModelRef`],
//! [`AnimState`], [`SkinPalette`]).
//!
//! | Module | Owns |
//! |---|---|
//! | [`registry`] | **Keystone A** — the reflection / component-descriptor table (type-hash → class, `cdbsizes.ini` pool budgets, reflected field layout). |
//! | [`event`] | **Keystone B** — the name-hash event / RPC bus shared by GUI, Net and AI (typed args ≤ 7, immediate dispatch + a bounded deferred queue). |
//! | [`frame`] | **Keystone C** — the master frame spine: the 5-slot application-[`LayerStack`](frame::LayerStack) the engine's master tick climbs 0→4. |
//! | [`streaming`] | The world-streaming DECISION core: block residency, entity wake/hibernate, LOD tiers, the global LOD-budget governor and the population region cache. Emits a `StreamDiff` the host executes. |
//! | [`guidmap`] | Name-hash → `Entity` and guid ↔ `Entity` (the engine's resident guidmap singleton). |
//! | [`object_filter`] | The `ObjectFilter.*` script query: label boolean-expression + include/exclude sets. |
//! | [`render_state`] | The `Atmosphere` / `Bloom` / `Graphics` / `Fade` parameter state the render passes read. |
//! | [`physics_query`] | The `PhysicsQuery` collision-query seam the sim silos compile against (no leaf→leaf edge to `mercs2_physics`). |
//!
//! Name hashing is **caller-supplied** throughout: [`registry`], [`event`] and [`guidmap`] all key on
//! a precomputed `u32` (the engine hash lives at the byte-decode boundary in `mercs2_formats`), so
//! there is one implementation and no drift.
//!
//! Non-blocking gaps deliberately left open are tracked in this crate's `DEFERRED.md`.

pub use glam;
pub use hecs;
pub use hecs::{Entity, World};

mod components;
pub use components::{AnimState, ModelRef, SkinPalette, Transform};

/// The `GuidMap` (see `guidmap.rs`): the name-hash → `Entity` + guid ↔ `Entity` registry modelling the
/// engine's resident guidmap singleton (`0x385EA82C`). `Pg.GetGuidByName` / `Object.*` resolve against
/// it to reach live entities instead of a side table parsed up front.
pub mod guidmap;
pub use guidmap::{GuidMap, FIRST_DYNAMIC_GUID, HERO_GUID, LOCAL_PLAYER_GUID};

pub mod streaming;

/// The `PhysicsQuery` seam (see `physics_query.rs`): the collision-query interface the sim silos
/// (`mercs2_vehicle`/`mercs2_combat`/`mercs2_anim`) depend on so they compile against the contract,
/// not the `mercs2_physics` impl. Grounded in `physics_code_map.md` §3/§4 (raycast / getClosestPoints
/// / hkpCharacterProxy move).
pub mod physics_query;
pub use physics_query::{PhysicsQuery, RayHit};

/// Keystone A — the reflection / component-descriptor registry (see `registry.rs`): the engine's
/// component/serialization spine that keys every component class by its name-hash and carries its
/// pool budget (`cdbsizes.ini`). Keystone C is the ordered [`Schedule`] below.
pub mod registry;
pub use registry::{ComponentDescriptor, ComponentRegistry};

/// The `ObjectFilter.*` script-query mechanism (see `object_filter.rs`): a label boolean-expression +
/// explicit include/exclude sets, minted/freed through a handle registry the script host owns.
pub mod object_filter;
pub use object_filter::{eval_label_expr, ObjectFilter, ObjectFilterRegistry};

/// Global render / post-FX parameter state (see `render_state.rs`): the sky/bloom/graphics/fade params
/// the `Atmosphere`/`Bloom`/`Graphics`/`Fade` Lua namespaces drive and the render passes read.
pub mod render_state;
pub use render_state::{AtmosphereState, BloomState, FadeState, GraphicsState, RenderState};

/// Keystone B — the serialized event / RPC bus (see `event.rs`): the one name-hash-keyed event bus
/// the engine shares across GUI (`ToggleHud`), Networking (`NetEventCallback`), and AI
/// (`DirectAction`). Typed args (≤7), an immediate dispatch path, and a bounded deferred queue
/// drained once per fixed tick from the [`Schedule`] below.
pub mod event;
pub use event::{Event, EventArg, EventBus, EventError, SubId};

/// Keystone C — the master frame spine (see `frame.rs`): the 5-layer application-layer stack the
/// engine's master tick (`FUN_004c14f0 → FUN_004c15e0`) climbs 0→4 each frame, plus the recovered
/// RunFrame stage order. Pairs with [`Time`] below (the decoupled fixed-sim accumulator).
pub mod frame;
pub use frame::{LayerStack, LayerTransition, LAYER_COUNT, LAYER_GAME};

/// Fixed-timestep simulation clock. Real frame deltas are accumulated and drained in `fixed_dt`
/// chunks so the sim advances deterministically regardless of render framerate — the way the
/// original engine ticks its update (**decoupled fixed-sim + variable-render**, RunFrame stages 3–4,
/// `docs/reverse_engineer/scheduler_tick_code_map.md` §6). `dt` equals `fixed_dt` during a step;
/// `elapsed`/`tick` count simulated time and steps.
#[derive(Clone, Copy, Debug)]
pub struct Time {
    pub fixed_dt: f32,
    pub dt: f32,
    pub elapsed: f32,
    pub tick: u64,
    /// Cap on fixed steps run per frame, to avoid a spiral-of-death after a long stall.
    pub max_steps: u32,
    /// Sim time scale applied to each real frame delta before it is accumulated — the engine's
    /// `dt * timescale` (`FUN_004c14f0`, `_DAT_0198dc48 += dt*timescale`). `1.0` = real time; `<1.0`
    /// slow-motion, `0.0` a hard pause. Render stays variable-rate regardless.
    pub timescale: f32,
    accumulator: f32,
}

impl Time {
    /// A clock ticking at `fixed_hz` (e.g. 60.0).
    pub fn new(fixed_hz: f32) -> Self {
        let fixed_dt = 1.0 / fixed_hz.max(1.0);
        Self {
            fixed_dt,
            dt: fixed_dt,
            elapsed: 0.0,
            tick: 0,
            max_steps: 8,
            timescale: 1.0,
            accumulator: 0.0,
        }
    }

    /// Fold a real (variable) frame delta into the fixed-sim accumulator, scaled by [`timescale`]
    /// (the engine's `dt * timescale`). Call once per rendered frame before draining steps.
    ///
    /// [`timescale`]: Time::timescale
    pub fn begin_frame(&mut self, real_dt: f32) {
        self.accumulator += real_dt.max(0.0) * self.timescale;
    }

    /// Consume one fixed step if the accumulator holds one and the per-frame budget isn't spent.
    /// Advances `tick`/`elapsed` and sets `dt = fixed_dt`. `steps_done` = steps already taken this
    /// frame (for the `max_steps` clamp). Private — callers use [`advance_frame`](Time::advance_frame)
    /// or [`Schedule::run_fixed`].
    fn try_step(&mut self, steps_done: u32) -> bool {
        if self.accumulator >= self.fixed_dt && steps_done < self.max_steps {
            self.accumulator -= self.fixed_dt;
            self.dt = self.fixed_dt;
            self.tick += 1;
            self.elapsed += self.fixed_dt;
            true
        } else {
            false
        }
    }

    /// Fold in a real frame delta and return how many fixed sim steps to run this frame, draining the
    /// accumulator (clamped to `max_steps`; the backlog is dropped on clamp to avoid a spiral of
    /// death). Use when the **host** runs its own fixed-step body (streaming/animation) instead of a
    /// [`Schedule`] — the loop calls `advance_frame` once, then runs its systems `n` times.
    pub fn advance_frame(&mut self, real_dt: f32) -> u32 {
        self.begin_frame(real_dt);
        let mut steps = 0;
        while self.try_step(steps) {
            steps += 1;
        }
        if steps == self.max_steps {
            self.accumulator = 0.0;
        }
        steps
    }
}

/// A system: a stateful function run once per fixed tick against the whole `World`.
pub type BoxedSystem = Box<dyn FnMut(&mut World, &Time)>;

/// An ordered list of systems. Registration order **is** execution order — that's how we mirror the
/// engine's defined update sequence, rather than letting an auto-scheduler reorder work away from
/// the oracle.
#[derive(Default)]
pub struct Schedule {
    systems: Vec<(String, BoxedSystem)>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Append a named system. Named for debuggability and to keep the update order legible.
    pub fn add_system<F>(&mut self, name: impl Into<String>, sys: F) -> &mut Self
    where
        F: FnMut(&mut World, &Time) + 'static,
    {
        self.systems.push((name.into(), Box::new(sys)));
        self
    }

    /// Run every system once, in registration order.
    pub fn run_once(&mut self, world: &mut World, time: &Time) {
        for (_name, sys) in self.systems.iter_mut() {
            sys(world, time);
        }
    }

    /// Advance `time` by a real frame delta and run the schedule at a fixed timestep, accumulating
    /// leftover time across frames. Returns the number of fixed steps executed. Clamps to
    /// `time.max_steps` and drops the backlog if the clamp is hit.
    pub fn run_fixed(&mut self, world: &mut World, time: &mut Time, frame_dt: f32) -> u32 {
        time.begin_frame(frame_dt);
        let mut steps = 0;
        while time.try_step(steps) {
            self.run_once(world, time);
            steps += 1;
        }
        if steps == time.max_steps {
            time.accumulator = 0.0;
        }
        steps
    }

    pub fn system_names(&self) -> impl Iterator<Item = &str> {
        self.systems.iter().map(|(n, _)| n.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_timestep_drains_accumulator() {
        let mut world = World::new();
        let mut time = Time::new(60.0);
        let mut sched = Schedule::new();
        sched.add_system("noop", |_w, _t| {});
        // 1/60 + a hair -> exactly one step; leftover carries over.
        let n = sched.run_fixed(&mut world, &mut time, 1.0 / 60.0 + 0.001);
        assert_eq!(n, 1);
        assert_eq!(time.tick, 1);
    }

    #[test]
    fn animation_system_advances_and_writes_palette() {
        let mut world = World::new();
        let e = world.spawn((
            Transform::IDENTITY,
            ModelRef { model: 0xA3C1FABC },
            AnimState::playing(0x1234),
            SkinPalette::default(),
        ));
        let mut time = Time::new(60.0);
        let mut sched = Schedule::new();
        sched.add_system("animation", |world, time| {
            for (_e, (st, pal)) in world.query::<(&mut AnimState, &mut SkinPalette)>().iter() {
                if !st.playing {
                    continue;
                }
                st.time += time.dt * st.speed;
                pal.mats = vec![[[1.0, 0.0, 0.0, 0.0]; 4]]; // stand-in for a real sampled palette
            }
        });
        sched.run_fixed(&mut world, &mut time, 0.5); // many fixed steps
        let st = *world.get::<&AnimState>(e).unwrap();
        assert!(st.time > 0.0);
        assert_eq!(world.get::<&SkinPalette>(e).unwrap().mats.len(), 1);
    }
}
