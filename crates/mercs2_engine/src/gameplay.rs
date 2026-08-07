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

    /// The shared static-world `PhysicsQuery` (the fleet collision soup + terrain heightfield). The game
    /// layer steps the constrained multi-body death ragdoll ([`death_ragdoll`]) against THIS same query,
    /// so a settling corpse collides with the identical world geometry the player and vehicles do.
    pub fn physics(&self) -> &StaticSoupPhysics {
        &self.physics
    }

    /// Mutable access to the fleet's persistent collision broadphase, so the game layer can feed it
    /// INCREMENTAL streaming deltas — [`StaticSoupPhysics::insert_unit`] on a prop/building WAKE,
    /// [`StaticSoupPhysics::remove_unit`] on a HIBERNATE — instead of re-handing the whole soup through
    /// [`set_collision`](Self::set_collision) (which rebuilds the grid from scratch) every streaming tick.
    pub fn physics_mut(&mut self) -> &mut StaticSoupPhysics {
        &mut self.physics
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
        // Death ragdolls: the weapon/damage pass above lowers `Health` and (on a lethal blast, in
        // `detonate_explosion`) flags the victim with `combat::Ragdoll` carrying its blast-seed velocity.
        // The faithful **constrained multi-body** ragdoll is snapped onto the victim's posed skeleton and
        // stepped by the GAME layer (`world.rs`), which owns the model rigs the seed/read-back need — see
        // [`death_ragdoll`]. It steps against THIS same `physics` soup (via [`GameplaySystems::physics`]),
        // so corpses collide with the identical world the player/vehicles do. (The old single-rigid-body
        // stand-in that ran here is superseded.)
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

/// The **death → constrained multi-body ragdoll** seam: snap the recovered
/// [`mercs2_physics::ragdoll::Ragdoll`] onto a killed character's current posed skeleton, step it against
/// the world, and read it back into the skin. It lives here (engine, which owns both the physics ragdoll
/// and the [`mercs2_anim::ragdoll`] skeleton seam) but is DRIVEN by the game layer (`world.rs`), which
/// supplies the per-model rig + posed skin the seed/read-back need. No new ragdoll math — this is pure
/// glue over the W6 API.
///
/// Spaces: the anim seam works in the entity's **model space** (`model_pose[b]` = bone `b`'s model-space
/// matrix, the pose before the inverse-bind skin multiply). The physics ragdoll simulates in **world
/// space** so it collides with the real world soup. This module bridges the two by the entity's
/// `Transform`: seeds are pushed model→world at spawn, and the stepped bodies are pulled world→model
/// before the write-back. `SkinPalette.mats` holds the SKIN palette (`InvBind[b] · model[b]`), so
/// [`model_pose_from_skin`] reconstructs the model pose to seed from, and [`recompose_skin`] rebuilds the
/// palette after the write-back.
pub mod death_ragdoll {
    use mercs2_anim::pose::BoneRig;
    use mercs2_anim::ragdoll::{body_seeds, write_back_model_pose};
    use mercs2_core::glam::{Quat, Vec3};
    use mercs2_core::{PhysicsQuery, Transform};
    use mercs2_formats::skeleton::mat4_mul;
    use mercs2_physics::ragdoll::{BodySeed, Ragdoll, RagdollDef};

    /// Componentwise scale, guarding against a zero component (degenerate transforms).
    fn safe_scale(s: Vec3) -> Vec3 {
        Vec3::new(
            if s.x.abs() > 1e-6 { s.x } else { 1.0 },
            if s.y.abs() > 1e-6 { s.y } else { 1.0 },
            if s.z.abs() > 1e-6 { s.z } else { 1.0 },
        )
    }

    /// Model-space `(pos, rot)` → world, by the entity `Transform` (scale ⊙, then rotate, then translate).
    fn to_world(tf: &Transform, mp: Vec3, mr: Quat) -> (Vec3, Quat) {
        (tf.translation + tf.rotation * (safe_scale(tf.scale) * mp), tf.rotation * mr)
    }

    /// World-space `(pos, rot)` → model space (the inverse of [`to_world`]).
    fn to_model(tf: &Transform, wp: Vec3, wr: Quat) -> (Vec3, Quat) {
        let inv = tf.rotation.inverse();
        ((inv * (wp - tf.translation)) / safe_scale(tf.scale), inv * wr)
    }

    /// Reconstruct the model-space pose from a skin palette: `model[b] = WorldBind[b] · Skin[b]` (the
    /// inverse of the `Skin[b] = InvBind[b] · model[b]` the pose pipeline stored in `SkinPalette.mats`).
    pub fn model_pose_from_skin(rig: &[BoneRig], skin: &[[[f32; 4]; 4]]) -> Vec<[[f32; 4]; 4]> {
        let n = rig.len().min(skin.len());
        (0..n).map(|b| mat4_mul(&rig[b].world_bind, &skin[b])).collect()
    }

    /// Rebuild the skin palette from a model-space pose: `Skin[b] = InvBind[b] · model[b]` — what the
    /// renderer consumes from `SkinPalette.mats`. Writes `rig.len().min(model_pose.len())` entries.
    pub fn recompose_skin(rig: &[BoneRig], model_pose: &[[[f32; 4]; 4]], out: &mut Vec<[[f32; 4]; 4]>) {
        let n = rig.len().min(model_pose.len());
        out.clear();
        out.reserve(n);
        for b in 0..n {
            out.push(mat4_mul(&rig[b].inv_bind, &model_pose[b]));
        }
    }

    /// **Alive → ragdoll snap** (`SetBodyToRagdoll`). Spawn the recovered 11-body human ragdoll seeded
    /// from `model_pose` (the victim's current posed skeleton) via [`body_seeds`], lifted model→world by
    /// `tf`, with every body's initial velocity `seed_velocity` (the blast seed, or zero). Returns the
    /// live ragdoll plus a working model-pose buffer (a copy of `model_pose`; its non-ragdoll bones stay
    /// frozen at the death frame while the driven bones are overwritten each step). `None` when any of the
    /// 11 ragdoll bones is absent from this rig — i.e. the character isn't ragdollable.
    pub fn spawn(
        rig: &[BoneRig],
        model_pose: &[[[f32; 4]; 4]],
        tf: &Transform,
        seed_velocity: Vec3,
    ) -> Option<(Ragdoll, Vec<[[f32; 4]; 4]>)> {
        let def = RagdollDef::human();
        let seeds_model = body_seeds(rig, model_pose, &def.bone_hashes());
        let mut seeds = Vec::with_capacity(seeds_model.len());
        for s in &seeds_model {
            let (mp, mr) = (*s)?; // any missing ragdoll bone => not ragdollable
            let (wp, wr) = to_world(tf, mp, mr);
            seeds.push(BodySeed { position: wp, orientation: wr, velocity: seed_velocity });
        }
        Some((Ragdoll::spawn(&def, &seeds), model_pose.to_vec()))
    }

    /// **Ragdoll → skin read-back.** Step the ragdoll one fixed tick against `phys`, pull each driven body
    /// world→model by `tf`, and write it into `model_pose` via [`write_back_model_pose`] (non-ragdoll
    /// bones untouched). Recompose the skin palette from `model_pose` afterwards with [`recompose_skin`].
    pub fn step_writeback(
        rd: &mut Ragdoll,
        rig: &[BoneRig],
        model_pose: &mut [[[f32; 4]; 4]],
        tf: &Transform,
        phys: &dyn PhysicsQuery,
        dt: f32,
    ) {
        rd.step(phys, dt);
        let driven: Vec<(u32, Vec3, Quat)> = rd
            .bone_transforms()
            .into_iter()
            .map(|(h, wp, wr)| {
                let (mp, mr) = to_model(tf, wp, wr);
                (h, mp, mr)
            })
            .collect();
        write_back_model_pose(rig, model_pose, &driven);
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

    /// End-to-end **death → multi-body ragdoll** over the [`death_ragdoll`] seam the game layer drives: a
    /// killed rigged character is snapped onto its posed skeleton, spawns the recovered 11-body ragdoll,
    /// and — stepped against a floor — its bone transforms **diverge** from the seeded (bind) pose and
    /// then **settle**, with the read-back landing on the entity's model pose.
    #[test]
    fn killed_rigged_entity_ragdoll_diverges_then_settles() {
        use crate::physics::StaticSoupPhysics;
        use mercs2_anim::pose::BoneRig;
        use mercs2_core::glam::Vec3;
        use mercs2_core::Transform;
        use mercs2_physics::ragdoll::RagdollDef;

        const ID: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

        // A rig carrying the 11 recovered ragdoll bones; the model pose stands them upright (identity
        // rotation, anatomical heights) — the "current posed skeleton" the death path snaps onto.
        let def = RagdollDef::human();
        let ys = [1.0, 1.35, 1.65, 1.45, 1.20, 1.45, 1.20, 0.70, 0.35, 0.70, 0.35];
        let xs = [0.0, 0.0, 0.0, 0.2, 0.25, -0.2, -0.25, 0.1, 0.1, -0.1, -0.1];
        let rig: Vec<BoneRig> = def
            .bodies
            .iter()
            .map(|b| BoneRig {
                parent: -1,
                name_hash: b.name_hash,
                world_bind: ID,
                inv_bind: ID,
                local_bind: ID,
            })
            .collect();
        let model_pose: Vec<[[f32; 4]; 4]> = (0..11)
            .map(|k| {
                let mut m = ID;
                m[3] = [xs[k], ys[k], 0.0, 1.0];
                m
            })
            .collect();

        // Flat floor of small triangles for the corpse to settle on.
        let mut tris = Vec::new();
        let mut x = -3.0f32;
        while x < 3.0 {
            let mut z = -3.0f32;
            while z < 3.0 {
                let a = Vec3::new(x, 0.0, z);
                let b = Vec3::new(x + 0.5, 0.0, z);
                let c = Vec3::new(x + 0.5, 0.0, z + 0.5);
                let d = Vec3::new(x, 0.0, z + 0.5);
                tris.push([a, c, b]);
                tris.push([a, d, c]);
                z += 0.5;
            }
            x += 0.5;
        }
        let phys = StaticSoupPhysics::new(tris);
        let tf = Transform::IDENTITY;

        let (mut rd, mut work_pose) =
            death_ragdoll::spawn(&rig, &model_pose, &tf, Vec3::ZERO).expect("all 11 bones present");
        assert_eq!(rd.body_count(), 11);
        let seeded = rd.bone_transforms();

        let mut steps = 0;
        while !rd.settled() && steps < 2000 {
            death_ragdoll::step_writeback(&mut rd, &rig, &mut work_pose, &tf, &phys, 1.0 / 60.0);
            steps += 1;
        }
        assert!(rd.settled(), "the ragdoll came to rest ({steps} steps)");

        // Diverged: the settled bodies are no longer at their seeded (upright/bind) positions.
        let settled = rd.bone_transforms();
        let moved: f32 = seeded
            .iter()
            .zip(&settled)
            .map(|((_, p0, _), (_, p1, _))| (*p1 - *p0).length())
            .sum();
        assert!(moved > 0.1, "ragdoll bones diverged from the seeded pose (moved {moved})");

        // Read-back landed on the model pose: at least one driven bone's model matrix changed from the
        // frozen death-frame pose, and the recomposed skin palette has an entry per bone.
        let changed = (0..11).any(|b| work_pose[b][3] != model_pose[b][3]);
        assert!(changed, "the write-back overwrote the driven bones' model pose");
        let mut skin = Vec::new();
        death_ragdoll::recompose_skin(&rig, &work_pose, &mut skin);
        assert_eq!(skin.len(), rig.len(), "one skin matrix per bone");
    }
}
