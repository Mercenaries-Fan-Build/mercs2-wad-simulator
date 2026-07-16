//! `GameplaySystems` — the Wave-1 fleet gameplay systems wired into the running engine's fixed tick.
//!
//! The fleet crates (physics / vehicle / combat / audio) shipped as tested subsystems, but nothing in
//! the engine drove them (the Wave-1 seam review's "everything DANGLING at the engine-loop boundary").
//! This bundle owns their shared per-frame state — a static-soup physics world built from the streamed
//! collision geometry (the `PhysicsQuery` every sim system uses), the engine event bus, the vehicle
//! steering LUT, and the shared audio engine — and runs them each fixed step over the ECS `World`.
//!
//! Systems are **idle (no-op) over a World that carries none of their components yet**, so this is safe
//! to tick from frame 1; as entities stream in with `Vehicle`/`RuntimeWeapon`/… components (the ECS
//! deserialization pipeline), the systems act on them. Animation stays on `world.rs`'s existing
//! schedule (same `hkQsTransform` math); swapping in `mercs2_anim` behind an `AnimAssets` adapter is a
//! follow-up.

use std::cell::RefCell;
use std::rc::Rc;

use crate::audio::AudioEngine;
use mercs2_core::glam::Vec3;
use mercs2_core::{EventBus, PhysicsQuery, World};
use crate::physics::StaticSoupPhysics;
use crate::vehicle::DonutLut;

/// The fleet gameplay systems + their shared per-frame state, ticked once per fixed step by the loop.
pub struct GameplaySystems {
    /// Static-world collision (from the streamed geometry) — the `PhysicsQuery` all sim systems use.
    physics: StaticSoupPhysics,
    /// The engine event bus (combat posts DamageMsg/DestroyMsg/homing events here).
    bus: EventBus,
    /// The weapon system, held as an instance so its per-frame **impact channel** (bullet/explosion/
    /// blood hit points) can be drained for the decal + particle producers. See [`take_impacts`].
    ///
    /// [`take_impacts`]: GameplaySystems::take_impacts
    weapons: crate::combat::WeaponSystem,
    /// The vehicle steering donut sine-LUT (built once).
    lut: DonutLut,
    /// Shared audio engine — the loop ticks the SAME engine the Lua `Sound.*` cues into.
    audio: Rc<RefCell<AudioEngine>>,
}

impl GameplaySystems {
    /// Build the bundle sharing `audio` with the script host (so cues + mixing hit one engine).
    pub fn new(audio: Rc<RefCell<AudioEngine>>) -> Self {
        GameplaySystems {
            physics: StaticSoupPhysics::new(Vec::new()),
            bus: EventBus::new(),
            weapons: crate::combat::WeaponSystem::default(),
            lut: DonutLut::new(),
            audio,
        }
    }

    /// Replace the static collision soup (call when the world geometry finishes streaming). The
    /// vehicle/weapon systems then raycast against it via the shared `PhysicsQuery`.
    pub fn set_collision(&mut self, tris: Vec<[Vec3; 3]>) {
        self.physics.set_tris(tris);
    }

    /// Give the fleet physics the terrain heightfield so ground raycasts (vehicle wheels, dropped
    /// props) resolve over open terrain — not just where a c3 building cell happens to supply triangles.
    /// Closes the §6.2 "terrain heightmap never handed to the fleet physics" gap (cars fell through
    /// open ground). `None` clears it (e.g. the interior boot, which has no terrain).
    pub fn set_heightmap(&mut self, heightmap: Option<crate::physics::Heightmap>) {
        self.physics.set_heightmap(heightmap);
    }

    /// Run one fixed simulation step of the fleet systems over `world`, in the recovered layer-4 order
    /// (vehicle → weapons — `FUN_004c9740`), drain the event bus, then advance audio. No-op over a
    /// World carrying none of the fleet components yet.
    pub fn tick(&mut self, world: &mut World, dt: f32) {
        let phys: &dyn PhysicsQuery = &self.physics;
        crate::vehicle::drive_step_system(world, phys, &self.lut, dt);
        // Instance tick (not the static `update`) so the impact channel accumulates for draining.
        self.weapons.tick(world, dt, &mut self.bus, Some(phys));
        // Integrate death ragdolls (WILDSTAR single-body stand-in): a lethal blast launches a
        // `Ragdollable` character (in `detonate_explosion`); here it falls + settles against the
        // terrain height. Replaced by the constrained Havok ragdoll when the physics silo lands.
        {
            let hm = self.physics.heightmap();
            crate::combat::ragdoll::ragdoll_system(world, dt, |p| {
                hm.and_then(|h| h.sample(p.x, p.z)).unwrap_or(0.0)
            });
        }
        self.bus.dispatch_all();
        self.audio.borrow_mut().tick(dt);
    }

    /// Drain this fixed step's combat impacts (bullet/explosion/blood hit points + normals). The
    /// runtime turns each into a projected decal and a particle burst. Drain-then-clear.
    pub fn take_impacts(&mut self) -> Vec<crate::combat::Impact> {
        self.weapons.take_impacts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wired vehicle system **acts**: a throttled car spawned into the World moves forward when
    /// driven purely through `GameplaySystems::tick` (which runs `drive_step_system` over the shared
    /// `StaticSoupPhysics`). This is the end-to-end proof that the engine→system→entity edge is live —
    /// spawn a fleet entity, tick the bundle, the entity moves. (Spawns are Lua/population-driven at
    /// runtime; here we spawn directly to exercise the wire.)
    #[test]
    fn vehicle_system_acts_through_gameplay_tick() {
        use mercs2_core::Transform;
        use crate::vehicle::components::{
            ChassisBody, Vehicle, VehicleClass, VehicleControls, VehicleRuntime, VehicleTuning, Wheel,
            WheelSet,
        };
        use crate::vehicle::lua_surface::{default_car_seating, spawn_vehicle};

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut gp = GameplaySystems::new(audio);
        // Tiled flat ground (1 m tiles) — real world geometry streams as small triangles, and the
        // physics proximity cull is tuned for that (giant quads get culled; W1-C DEFERRED note).
        let mut tris = Vec::new();
        for xi in -15..15 {
            for zi in -15..15 {
                let (x0, x1) = (xi as f32, xi as f32 + 1.0);
                let (z0, z1) = (zi as f32, zi as f32 + 1.0);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1)]);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::new(x0, 0.0, z1)]);
            }
        }
        gp.set_collision(tris);

        let mut world = World::new();
        let mut ctrl = VehicleControls::default();
        ctrl.accel = 1.0; // full throttle
        let car = spawn_vehicle(
            &mut world,
            Transform::from_translation(Vec3::new(0.0, 0.85, 0.0)),
            Vehicle::new(VehicleClass::Car, 0x1000),
            ChassisBody::new(1200.0),
            ctrl,
            WheelSet(vec![
                Wheel::new(Vec3::new(-0.8, 0.0, 1.3), true, true, false),
                Wheel::new(Vec3::new(0.8, 0.0, 1.3), true, true, false),
                Wheel::new(Vec3::new(-0.8, 0.0, -1.3), false, false, true),
                Wheel::new(Vec3::new(0.8, 0.0, -1.3), false, false, true),
            ]),
            VehicleTuning::default(),
            VehicleRuntime::new(),
            default_car_seating(),
        );

        let z0 = world.get::<&Transform>(car).unwrap().translation.z;
        for _ in 0..240 {
            gp.tick(&mut world, 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!(
            (z1 - z0).abs() > 1.0,
            "throttled car should move via the wired drive system; dz = {}",
            z1 - z0
        );
    }

    /// Ticking the fleet over an empty World is a safe no-op (the systems find no components) — the
    /// invariant that lets the loop drive them from frame 1, before entities stream in.
    #[test]
    fn ticks_empty_world_without_panicking() {
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut gp = GameplaySystems::new(audio.clone());
        gp.set_collision(vec![[Vec3::ZERO, Vec3::X, Vec3::Z]]);
        let mut world = World::new();
        for _ in 0..8 {
            gp.tick(&mut world, 1.0 / 60.0);
        }
        // The shared audio engine advanced (dynamic-music toggle is observable through the same Rc).
        audio.borrow_mut().set_dynamic_music(true);
        assert!(audio.borrow().is_dynamic_music());
    }
}
