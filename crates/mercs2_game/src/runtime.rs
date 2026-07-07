//! `GameRuntime` — the connection layer that binds the game's script-driven spawns to the fleet
//! gameplay systems, bundled out of the world loop into one tested unit.
//!
//! The render loop owns the window + GPU; this owns the per-frame *game* update: realize the spawn
//! intents the mission Lua recorded (`GameScriptHost::take_new_spawns` → [`SpawnResolver`] → the right
//! ECS archetype) and tick the wired fleet ([`GameplaySystems`]: physics / vehicle / combat / audio).
//! It holds no GPU state, so the whole game-update side is unit-testable without a window — the loop
//! feeds it the drained requests + `dt` and attaches visuals to whatever entities it returns.
//!
//! This is where the persistent mission-Lua host plugs in: the loop drains the host's recorded
//! `Pg.Spawn`s each frame, hands them to [`realize_spawns`](GameRuntime::realize_spawns), and attaches
//! a `ModelRef` to each returned entity. Until that host runs in the TPS loop the resolver is empty
//! (every template is a plain prop) and no requests arrive — the seam is proven by test.

use std::cell::RefCell;
use std::rc::Rc;

use mercs2_audio::AudioEngine;
use mercs2_core::glam::{Quat, Vec3};
use mercs2_core::{Entity, Transform, World};

use crate::gameplay::GameplaySystems;
use crate::script_host::SpawnRequest;
use crate::spawn::SpawnResolver;

/// The per-frame game update: fleet gameplay systems + the template→entity spawn resolver. Owns no GPU
/// state (the render loop attaches visuals to the entities [`realize_spawns`](Self::realize_spawns)
/// returns).
pub struct GameRuntime {
    /// Fleet gameplay systems (physics / vehicle / combat / audio), ticked each fixed step.
    pub gameplay: GameplaySystems,
    /// Template-name-hash → ECS archetype (populated from the reflection registry / spawn-list data;
    /// `register` until that's threaded).
    pub resolver: SpawnResolver,
    /// The AI mechanism (recovered action ring + relation matrix). Its per-entity perception update
    /// runs each fixed step over the world (idle until AI entities carry perception components, the
    /// same data-driven way the vehicle/combat systems idle). The `Ai.*` Lua surface drives the same
    /// relation matrix once the persistent mission-Lua host shares this in.
    pub ai: mercs2_ai::AiWorld,
}

impl GameRuntime {
    /// A runtime driving `audio` (shared with the Lua `Sound.*` forwarding so one engine is both cued
    /// and ticked). The resolver starts empty — every template resolves to a plain prop until the
    /// reflection/spawn-list data registers the vehicle/character archetypes.
    pub fn new(audio: Rc<RefCell<AudioEngine>>) -> Self {
        GameRuntime {
            gameplay: GameplaySystems::new(audio),
            resolver: SpawnResolver::new(),
            ai: mercs2_ai::AiWorld::new(),
        }
    }

    /// Hand the fleet physics its static collision soup (the streamed structural geometry). See
    /// [`GameplaySystems::set_collision`].
    pub fn set_collision(&mut self, tris: Vec<[Vec3; 3]>) {
        self.gameplay.set_collision(tris);
    }

    /// Realize recorded spawn intents into ECS entities. Each request's template name is hashed
    /// (`pandemic_hash_m2`) and routed through the resolver → the right archetype: a drivable `Vehicle`
    /// bundle the fleet drive system moves, or a plain `Prop`. The final transform is the request's
    /// `pos` + `yaw` (after any `Object.SetPosition`/`SetYaw`). Returns `(entity, template_hash)` per
    /// request so the render layer can attach the visual (`ModelRef` + `scene.load_model`); the
    /// ECS/gameplay side is fully materialized here.
    pub fn realize_spawns(&self, world: &mut World, requests: &[SpawnRequest]) -> Vec<(Entity, u32)> {
        requests
            .iter()
            .map(|r| {
                let tpl = mercs2_formats::hash::pandemic_hash_m2(&r.template);
                let mut t = Transform::from_translation(Vec3::from(r.pos));
                t.rotation = Quat::from_rotation_y(r.yaw);
                let e = self.resolver.spawn(world, tpl, r.guid as u32, t);
                (e, tpl)
            })
            .collect()
    }

    /// Advance the per-frame game update one fixed step over `world`: the fleet gameplay systems
    /// (physics / vehicle / combat / audio) plus the AI per-entity perception update (§2.4). Both idle
    /// until entities carry their components.
    pub fn tick(&mut self, world: &mut World, dt: f32) {
        self.gameplay.tick(world, dt);
        self.ai.tick(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spawn::Archetype;
    use mercs2_vehicle::components::{Vehicle, VehicleClass, VehicleControls};

    fn tiled_ground() -> Vec<[Vec3; 3]> {
        let mut tris = Vec::new();
        for xi in -15..15 {
            for zi in -15..15 {
                let (x0, x1) = (xi as f32, xi as f32 + 1.0);
                let (z0, z1) = (zi as f32, zi as f32 + 1.0);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1)]);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::new(x0, 0.0, z1)]);
            }
        }
        tris
    }

    fn car_request(template: &str, pos: [f32; 3]) -> SpawnRequest {
        SpawnRequest { guid: 0x1000_0001, template: template.into(), name: "car".into(), pos, yaw: 0.0 }
    }

    /// The full runtime path: a recorded `Pg.Spawn` of a registered vehicle template is realized into a
    /// drivable ECS entity and, once throttled, driven forward by the runtime's own `tick` — exactly
    /// what a mission `MrxUtil.SpawnActor("...car...")` → `take_new_spawns` will produce at runtime.
    #[test]
    fn realizes_a_recorded_spawn_into_a_drivable_vehicle() {
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        let tpl = mercs2_formats::hash::pandemic_hash_m2("mission_getaway_car");
        rt.resolver.register(tpl, Archetype::Vehicle(VehicleClass::Car));
        rt.set_collision(tiled_ground());

        let mut world = World::new();
        let realized = rt.realize_spawns(&mut world, &[car_request("mission_getaway_car", [0.0, 0.85, 0.0])]);
        assert_eq!(realized.len(), 1);
        let (car, hash) = realized[0];
        assert_eq!(hash, tpl, "returned template hash must match for the visual attach");
        assert!(world.get::<&Vehicle>(car).is_ok(), "vehicle template must realize a Vehicle entity");

        world.get::<&mut VehicleControls>(car).unwrap().accel = 1.0; // throttle
        let z0 = world.get::<&Transform>(car).unwrap().translation.z;
        for _ in 0..240 {
            rt.tick(&mut world, 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!((z1 - z0).abs() > 1.0, "realized+throttled vehicle should drive; dz = {}", z1 - z0);
    }

    /// The AI perception update runs through `GameRuntime::tick`: a hostile observer in range makes the
    /// target's perception record show a hostile-aware observer — proving the recovered AI mechanism is
    /// wired into the per-frame game update alongside the fleet, idle until AI entities exist.
    #[test]
    fn tick_runs_ai_perception_over_the_world() {
        use mercs2_ai::{AiFaction, Perception, PerceptionRecord, Stimulus, Target};

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        rt.ai.set_relation(1, 2, -100); // faction 1 hostile to 2

        let mut world = World::new();
        world.spawn((Perception::default(), Transform::from_translation(Vec3::ZERO), AiFaction(1)));
        let watched = world.spawn((
            PerceptionRecord::default(),
            Target::default(),
            Stimulus::default(),
            Transform::from_translation(Vec3::new(30.0, 0.0, 0.0)),
            AiFaction(2),
        ));

        rt.tick(&mut world, 1.0 / 60.0);
        assert_eq!(
            world.get::<&PerceptionRecord>(watched).unwrap().hostile_aware, 1,
            "AI perception must run through the runtime tick"
        );
    }

    /// An unregistered template realizes a plain prop (bare Transform, no Vehicle) — the render loop
    /// attaches a `ModelRef`; the fleet leaves it alone.
    #[test]
    fn unregistered_template_realizes_a_plain_prop() {
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let rt = GameRuntime::new(audio);
        let mut world = World::new();
        let realized = rt.realize_spawns(&mut world, &[car_request("some_barrel_prop", [1.0, 0.0, 2.0])]);
        let (prop, _) = realized[0];
        assert!(world.get::<&Vehicle>(prop).is_err(), "unregistered template must be a plain prop");
        let t = world.get::<&Transform>(prop).unwrap();
        assert_eq!(t.translation, Vec3::new(1.0, 0.0, 2.0), "prop must sit at the requested position");
    }
}
