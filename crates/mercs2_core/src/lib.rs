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

pub use glam;
pub use hecs;
pub use hecs::{Entity, World};

mod components;
pub use components::{AnimState, ModelRef, SkinPalette, Transform};

pub mod streaming;

/// Keystone A — the reflection / component-descriptor registry (see `registry.rs`): the engine's
/// component/serialization spine that keys every component class by its name-hash and carries its
/// pool budget (`cdbsizes.ini`). Keystone C is the ordered [`Schedule`] below.
pub mod registry;
pub use registry::{ComponentDescriptor, ComponentRegistry};

/// Fixed-timestep simulation clock. Real frame deltas are accumulated and drained in `fixed_dt`
/// chunks so the sim advances deterministically regardless of render framerate — the way the
/// original engine ticks its update. `dt` equals `fixed_dt` during a step; `elapsed`/`tick` count
/// simulated time and steps.
#[derive(Clone, Copy, Debug)]
pub struct Time {
    pub fixed_dt: f32,
    pub dt: f32,
    pub elapsed: f32,
    pub tick: u64,
    /// Cap on fixed steps run per frame, to avoid a spiral-of-death after a long stall.
    pub max_steps: u32,
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
            accumulator: 0.0,
        }
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
        time.accumulator += frame_dt.max(0.0);
        let mut steps = 0;
        while time.accumulator >= time.fixed_dt && steps < time.max_steps {
            time.accumulator -= time.fixed_dt;
            time.dt = time.fixed_dt;
            time.tick += 1;
            time.elapsed += time.fixed_dt;
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
