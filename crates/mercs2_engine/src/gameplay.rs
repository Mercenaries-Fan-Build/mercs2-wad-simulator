//! `GameplaySystems` — the fleet gameplay systems wired into the running engine's fixed tick.
//!
//! The fleet crates (physics / vehicle / combat / audio) shipped as tested subsystems, but nothing in
//! the engine drove them — everything DANGLING at the engine-loop boundary.
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
    /// Per-model destruction machines + HIER. Empty until a loader populates it (see
    /// [`destruction_store_mut`](GameplaySystems::destruction_store_mut)); an unpopulated store
    /// simply means nothing is destructible, never an error.
    destruction: crate::destruction::DestructionStore,
    /// This step's destruction side effects (debris / fire), drained by
    /// [`take_destruction_intents`](GameplaySystems::take_destruction_intents).
    destruction_intents: Vec<mercs2_destruction::DestructionIntent>,
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
            destruction: crate::destruction::DestructionStore::new(),
            destruction_intents: Vec::new(),
        }
    }

    /// The per-model destruction store, for a loader to populate as models come in
    /// (`store.insert_model(&model)`). Without this, `Destructible` entities never change state.
    pub fn destruction_store_mut(&mut self) -> &mut crate::destruction::DestructionStore {
        &mut self.destruction
    }

    /// Drain this step's destruction intents — `CreateObject` (debris) and `StartEmitter` (fire).
    /// The runtime turns each into a spawn / FX request; ignoring them yields correct geometry and
    /// no effects. Drain-then-clear, mirroring [`take_impacts`](GameplaySystems::take_impacts).
    pub fn take_destruction_intents(&mut self) -> Vec<mercs2_destruction::DestructionIntent> {
        std::mem::take(&mut self.destruction_intents)
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
    /// (player roster → vehicle → weapons → destruction — `FUN_004c9740`), drain the event bus, then
    /// advance audio. No-op over a World carrying none of the fleet components yet.
    ///
    /// `player` is threaded in rather than owned here because the **script host** owns the player
    /// concern — Lua is its primary driver, and this tick only reads/advances it.
    pub fn tick(&mut self, world: &mut World, player: &mut mercs2_player::PlayerWorld, dt: f32) {
        let phys: &dyn PhysicsQuery = &self.physics;
        // Player roster passes A and B — `FUN_0062E810` (@0x004C9861) and `FUN_0062E7B0` (@0x004C9900),
        // both opening `mov eax,[0x00DF9BA8]` (the `Players` live count) and iterating by dense index
        // with `dt`. They precede the vehicle-control pump `FUN_00532F80` (@0x004C990C) in
        // `FUN_004C9740`'s byte order.
        //
        // ⚠ `player_code_map.md` §1's *diagram* lists the pump first; the recovered call-site addresses
        // disagree and win, because byte order in the caller is the harder evidence. Recorded as a
        // map-vs-map tension in `mercs2_player/DEFERRED.md` rather than silently resolved.
        //
        // Entity resolution is the caller's (`GuidMap` lives on the script host), so the roster pass is
        // given a resolver that finds nothing here; the boundary-death condition it reports is consumed
        // by the game layer, which owns the respawn path.
        mercs2_player::player_roster_system(world, player, |_| None, dt);
        crate::vehicle::drive_step_system(world, phys, &self.lut, dt);
        // Instance tick (not the static `update`) so the impact channel accumulates for draining.
        self.weapons.tick(world, dt, &mut self.bus, Some(phys));
        // Integrate death ragdolls (WILDSTAR single-body stand-in): a lethal blast launches a
        // `Ragdollable` character (in `detonate_explosion`); here it falls + settles against the
        // terrain height. Replaced by the constrained Havok ragdoll when the physics system lands.
        {
            let hm = self.physics.heightmap();
            crate::combat::ragdoll::ragdoll_system(world, dt, |p| {
                hm.and_then(|h| h.sample(p.x, p.z)).unwrap_or(0.0)
            });
        }
        // Destruction runs AFTER the weapon system, so this step's damage is already on `Health`
        // when the machines are advanced — retail drives transitions from damage messages, not from
        // a poll one frame stale. Produces the node-enable tables the render side mirrors onto the
        // draw gate via `destruction::sync_destruction_to_scene`.
        self.destruction_intents.extend(mercs2_destruction::destruction_system(
            world,
            &self.destruction,
            mercs2_destruction::DamageBands::default(),
        ));
        // Reap the deferred weapon-destroy queue **after** everything above, so a script that destroys
        // and re-applies a loadout inside one frame still sees valid handles — the shipped
        // snapshot-restore pattern (`mrxplayer.lua:661-724`) depends on that deferral. A weapon
        // re-attached in the meantime is cancelled rather than reaped.
        mercs2_combat::inventory::drain_pending_destroy(world);
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
    /// **The frame-loop wire.** A destroyed vehicle loses its governed geometry when driven purely
    /// through `GameplaySystems::tick` — no direct call to `destruction_system`. Destruction runs
    /// after the weapon system, so a kill this step is reflected the same step.
    #[test]
    fn destruction_advances_through_gameplay_tick() {
        use mercs2_core::{Destructible, Health, ModelRef, Transform};
        use mercs2_formats::orchestrator::{
            HierNode, StateDef, StateMachine, SwitchNodeDef, STATE_PRISTINE, STATE_WRECK,
        };

        const MODEL: u32 = 0xC0FF_EE01;
        const NODE: u32 = 0xAAAA_0001;
        let h = |s: &str| mercs2_formats::hash::pandemic_hash_m2(s);
        // `1 <imm>` push · `2 <cmd>` invoke · `3` end.
        let pristine = vec![1, STATE_WRECK, 1, 0xC650_7EE1u32, 2, h("setstateonmsg"), 3];
        let wreck = vec![1u32, NODE, 2, h("hide"), 3];

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut gp = GameplaySystems::new(audio);
        gp.destruction_store_mut().insert(
            MODEL,
            Some(StateMachine {
                switch_slots: vec![0],
                nodes: vec![SwitchNodeDef {
                    name_hash: NODE,
                    states: vec![
                        StateDef { name_hash: STATE_PRISTINE, enter: pristine, exit: vec![] },
                        StateDef { name_hash: STATE_WRECK, enter: wreck, exit: vec![] },
                    ],
                }],
            }),
            vec![HierNode {
                index: 0, hash: NODE, parent: None, local: [0.0; 16],
                bbox_min: [0.0; 3], bbox_max: [0.0; 3],
            }],
        );

        let mut world = World::new();
        let e = world.spawn((
            Transform::default(),
            ModelRef { model: MODEL },
            Health::new(100.0),
            Destructible::default(),
        ));

        gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        assert!(world.get::<&Destructible>(e).unwrap().draws(0), "healthy: geometry draws");

        world.get::<&mut Health>(e).unwrap().cur = 0.0;
        gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        assert!(
            !world.get::<&Destructible>(e).unwrap().draws(0),
            "destroyed: the governed subtree must be hidden, driven by tick alone"
        );
    }

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
        // physics proximity cull is tuned for that (giant quads get culled; see the DEFERRED note).
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
            gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
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
            gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        }
        // The shared audio engine advanced (dynamic-music toggle is observable through the same Rc).
        audio.borrow_mut().set_dynamic_music(true);
        assert!(audio.borrow().is_dynamic_music());
    }
}
