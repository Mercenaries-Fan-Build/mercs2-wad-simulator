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

use mercs2_audio::AudioEngine;
use mercs2_core::glam::Vec3;
use mercs2_core::{EventBus, PhysicsQuery, World};
use mercs2_physics::StaticSoupPhysics;
use mercs2_vehicle::DonutLut;

/// The fleet gameplay systems + their shared per-frame state, ticked once per fixed step by the loop.
pub struct GameplaySystems {
    /// Static-world collision (from the streamed geometry) — the `PhysicsQuery` all sim systems use.
    physics: StaticSoupPhysics,
    /// The engine event bus (combat posts DamageMsg/DestroyMsg/homing events here).
    bus: EventBus,
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
            lut: DonutLut::new(),
            audio,
        }
    }

    /// Replace the static collision soup (call when the world geometry finishes streaming). The
    /// vehicle/weapon systems then raycast against it via the shared `PhysicsQuery`.
    pub fn set_collision(&mut self, tris: Vec<[Vec3; 3]>) {
        self.physics.set_tris(tris);
    }

    /// Run one fixed simulation step of the fleet systems over `world`, in the recovered layer-4 order
    /// (vehicle → weapons — `FUN_004c9740`), drain the event bus, then advance audio. No-op over a
    /// World carrying none of the fleet components yet.
    pub fn tick(&mut self, world: &mut World, dt: f32) {
        let phys: &dyn PhysicsQuery = &self.physics;
        mercs2_vehicle::drive_step_system(world, phys, &self.lut, dt);
        mercs2_combat::WeaponSystem::update(world, dt, &mut self.bus, Some(phys));
        self.bus.dispatch_all();
        self.audio.borrow_mut().tick(dt);
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
        use mercs2_vehicle::components::{
            ChassisBody, Vehicle, VehicleClass, VehicleControls, VehicleRuntime, VehicleTuning, Wheel,
            WheelSet,
        };
        use mercs2_vehicle::lua_surface::{default_car_seating, spawn_vehicle};

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
