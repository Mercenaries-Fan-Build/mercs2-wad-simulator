//! The ECS entity shapes a template becomes: a plain rendered **prop**, a drivable **vehicle** or an
//! AI **character**. A template's [`Archetype`] is the physics component it carries
//! (`game_world::unit_archetype`); the population spawners and the streaming executor's runtime-spawn
//! activation build the entity with [`spawn_default_vehicle`] / [`spawn_character`].

use mercs2_core::glam::Vec3;
use mercs2_core::{Entity, Transform, World};
use crate::vehicle::components::{
    ChassisBody, Vehicle, VehicleClass, VehicleControls, VehicleRuntime, VehicleTuning, Wheel, WheelSet,
};
use crate::vehicle::lua_surface::{default_car_seating, spawn_vehicle};

/// The ECS entity shape a template resolves to. Extends as more fleet archetypes land (Weapon, …);
/// today: a rendered prop, a drivable vehicle, or a full AI character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Archetype {
    /// A static/rendered prop — the render loop attaches Transform + ModelRef.
    Prop,
    /// A drivable vehicle of the given class — a full fleet bundle the drive system moves.
    Vehicle(VehicleClass),
    /// A living AI actor (person) — a full cross-system bundle: AI perception/behavior + faction +
    /// health + animation, so the actor is visible to the AI, killable by combat, and animated. This
    /// closes keystone K3 (`engine_support_inventory.md` §6.1): before it, every spawned NPC was an
    /// inert factionless prop, gating AI/faction/combat-death/animation for all population actors.
    Character,
}

/// Spawn a default drivable vehicle of `class` — the faithful component set `drive_step_system`
/// queries (a standard 4-wheel car layout; tank mass for `Tank`). Tuning defaults (MaxSpeed/suspension)
/// are confirm-live placeholders (the retail field names are stripped, per the vehicle code map).
pub fn spawn_default_vehicle(
    world: &mut World,
    class: VehicleClass,
    handle: u32,
    transform: Transform,
) -> Entity {
    let mass = if class == VehicleClass::Tank { 30_000.0 } else { 1200.0 };
    spawn_vehicle(
        world,
        transform,
        Vehicle::new(class, handle),
        ChassisBody::new(mass),
        VehicleControls::default(),
        car_wheels(),
        VehicleTuning::default(),
        VehicleRuntime::new(),
        default_car_seating(),
    )
}

/// Spawn a living AI actor — the full cross-system component bundle a person needs so it participates
/// in every actor subsystem at once (keystone K3):
/// - **humanoid** (`mercs2_core`): the `Human` marker + a default `HumanState` — this is what makes the
///   entity a *person* to every system that acts on people (combat's blood-vs-spark impact pick, the
///   animation selection key, the AI's people goals). Deliberately **not** `PlayerControlled`: retail
///   possession is applied on *attach* (`FUN_006A4060` writes the character GUID to the possession
///   field `player+0x20` at `0x006A422E`), never at spawn — `mercs2_player` owns that pairing;
/// - **AI** (`mercs2_ai`): `Perception`/`Stimulus`/`Target`/`PerceptionRecord` (seen by + sees others),
///   `AiBehavior` (unrestricted), `AiSkill`, `Squad`, and a **neutral `AiFaction(0)`** the caller
///   overrides with the real faction (`set_faction`);
/// - **combat** (`mercs2_combat`): `Health` (100) so damage/death applies;
/// - **animation** (`mercs2_anim`): `HumanAnimationSet` (keyed by the template hash as the character id)
///   + `AnimController`, so the data-driven clip picker can drive it.
///
/// The character's `ModelRef { model }` IS attached here — the render loop draws it the moment that
/// model is resident (a population unit's `model` is its template's `ModelName`; a script spawn by
/// model name passes the same hash as `template_hash`). The `SkinPalette` is attached where the
/// resident bone count is known (the population preload path in
/// [`crate::runtime::GameRuntime::tick_population`], which sizes it to the model's rig);
/// `animation_system` then rewrites it each tick from the sampled pose. `template_hash` doubles as the
/// animation character id until a template→character map lands.
pub fn spawn_character(world: &mut World, template_hash: u32, model: u32, transform: Transform) -> Entity {
    use crate::ai::{AiBehavior, AiFaction, AiSkill, Perception, PerceptionRecord, Squad, Stimulus, Target};
    use crate::anim::{AnimController, HumanAnimationSet};
    use mercs2_core::{Health, Human, HumanState};

    let e = world.spawn((
        transform,
        // humanoid identity (see the doc comment: possession is added on attach, not here)
        Human,
        HumanState::default(),
        // AI
        Perception::default(),
        Stimulus::default(),
        Target::default(),
        PerceptionRecord::default(),
        AiBehavior::default(),
        AiSkill::default(),
        Squad::default(),
        AiFaction(0), // neutral until the caller maps the spawn's faction (see set_faction)
        // combat
        Health::new(100.0),
        // A human body: on a lethal blast it launches a death ragdoll (WILDSTAR stand-in) rather than
        // freezing mid-animation. Props/vehicles omit this (they use the destruction FSM).
        crate::combat::Ragdollable,
        // animation
        HumanAnimationSet::new(template_hash),
        AnimController::default(),
    ));
    // Attached separately — the bundle above is already at hecs's max tuple arity. The render loop draws
    // it once the model is resident; the population preload path attaches the rig-sized `SkinPalette`.
    let _ = world.insert_one(e, mercs2_core::ModelRef { model });
    e
}

/// Set (override) a spawned actor's AI faction — the caller maps the population/script spawn's faction
/// channel to a faction id after [`spawn`](SpawnResolver::spawn). No-op if the entity has no
/// `AiFaction` (e.g. it resolved to a prop/vehicle).
pub fn set_faction(world: &mut World, entity: Entity, faction_id: u32) {
    let _ = world.insert_one(entity, crate::ai::AiFaction(faction_id));
}

/// A standard 4-wheel car layout (front steered/unpowered, rear powered) — the hardpoints
/// `drive_step_system`'s per-axle raycasts use.
fn car_wheels() -> WheelSet {
    WheelSet(vec![
        Wheel::new(Vec3::new(-0.8, 0.0, 1.3), true, true, false),
        Wheel::new(Vec3::new(0.8, 0.0, 1.3), true, true, false),
        Wheel::new(Vec3::new(-0.8, 0.0, -1.3), false, false, true),
        Wheel::new(Vec3::new(0.8, 0.0, -1.3), false, false, true),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Character` template resolves to a full AI actor carrying every cross-system component (K3):
    /// AI perception/behavior, faction, health, animation — so it is seen by AI, killable, and animated.
    /// `set_faction` then overrides the neutral default with the real faction.
    #[test]
    fn character_template_spawns_the_full_actor_bundle() {
        use crate::ai::{AiBehavior, AiFaction, Perception, PerceptionRecord, Stimulus, Target};
        use crate::anim::{AnimController, HumanAnimationSet};
        use mercs2_core::Health;

        let npc_tpl = mercs2_formats::hash::pandemic_hash_m2("vz_soldier");
        let mut world = World::new();
        let npc = spawn_character(&mut world, npc_tpl, npc_tpl, Transform::from_translation(Vec3::new(3.0, 0.0, 4.0)));

        // AI: sees + is seen.
        assert!(world.get::<&Perception>(npc).is_ok(), "actor must perceive");
        assert!(world.get::<&Stimulus>(npc).is_ok() && world.get::<&Target>(npc).is_ok(), "actor is a target");
        assert!(world.get::<&PerceptionRecord>(npc).is_ok() && world.get::<&AiBehavior>(npc).is_ok());
        // combat: killable.
        assert_eq!(world.get::<&Health>(npc).unwrap().max, 100.0);
        // animation: drivable by the picker.
        assert_eq!(world.get::<&HumanAnimationSet>(npc).unwrap().character, npc_tpl);
        assert!(world.get::<&AnimController>(npc).is_ok());
        // faction: neutral by default, then overridden.
        assert_eq!(world.get::<&AiFaction>(npc).unwrap().0, 0);
        set_faction(&mut world, npc, 7);
        assert_eq!(world.get::<&AiFaction>(npc).unwrap().0, 7, "caller maps the spawn faction");
    }

    /// A spawned character is a **person**: it carries the `Human` marker + a default `HumanState`, the
    /// vocabulary every people-acting system queries. It is deliberately NOT `PlayerControlled` — retail
    /// possession is applied on attach (`FUN_006A4060` writes the character GUID to `player+0x20`),
    /// never at spawn.
    #[test]
    fn character_spawns_as_a_human_but_unpossessed() {
        use mercs2_core::{HumanState, PlayerControlled, ANY_STATE};

        let tpl = mercs2_formats::hash::pandemic_hash_m2("vz_civilian");
        let mut world = World::new();
        let npc = spawn_character(&mut world, tpl, tpl, Transform::IDENTITY);

        assert!(world.get::<&mercs2_core::Human>(npc).is_ok(), "a spawned character is a person");
        let st = *world.get::<&HumanState>(npc).unwrap();
        assert_eq!(st, HumanState::default());
        assert_eq!(st.stance, ANY_STATE, "no stance until a Human.SetState call");
        assert!(st.can_fire(), "spawns armed and unlocked");
        assert!(
            world.get::<&PlayerControlled>(npc).is_err(),
            "possession is applied on attach, not at spawn"
        );
    }

    /// Cross-system: a vehicle is damageable. Bullets lower its pool and a big enough hit destroys it —
    /// the combat applier finds it because `spawn_vehicle` now bundles `Health`.
    #[test]
    fn vehicle_takes_damage_and_dies() {
        use crate::combat::damage::apply_hit;
        use crate::combat::DamageKey;
        use crate::vehicle::DEFAULT_VEHICLE_HEALTH;
        use mercs2_core::event::EventBus;
        use mercs2_core::Health;

        let mut world = World::new();
        let mut bus = EventBus::new();
        let car = spawn_default_vehicle(&mut world, VehicleClass::Car, 0x3000, Transform::IDENTITY);

        // A rifle burst wounds it.
        let applied = apply_hit(&mut world, &mut bus, car, None, 50.0, DamageKey::BulletLarge);
        assert_eq!(applied, 50.0, "the applier must see the vehicle's Health");
        let h = *world.get::<&Health>(car).unwrap();
        assert_eq!(h.cur, DEFAULT_VEHICLE_HEALTH - 50.0);
        assert!(!h.is_dead());

        // A rocket finishes it: the pool floors at zero and the vehicle reads as destroyed. What
        // happens next (wreck FSM / part shedding) is the deferred destruction subsystem's job.
        apply_hit(&mut world, &mut bus, car, None, 10_000.0, DamageKey::RocketLarge);
        let h = *world.get::<&Health>(car).unwrap();
        assert_eq!(h.cur, 0.0);
        assert!(h.is_dead(), "vehicle destroyed");
        // Dead is dead: a further hit applies nothing.
        assert_eq!(apply_hit(&mut world, &mut bus, car, None, 25.0, DamageKey::BulletLarge), 0.0);
    }

    /// Cross-system FX predicate: shooting a **vehicle** sparks a bullet hole, shooting a **character**
    /// sprays blood. Both are `Health`-bearing, so this can only be right if the predicate is `Human`.
    #[test]
    fn shooting_a_vehicle_sparks_but_shooting_a_character_bleeds() {
        use crate::combat::firing::weapon_firing_system_impacts;
        use crate::combat::stats::WeaponStats;
        use crate::combat::{components::RuntimeWeapon, ImpactKind};
        use mercs2_core::event::EventBus;
        use mercs2_core::physics_query::{ClosestPoint, RayHit};
        use mercs2_core::PhysicsQuery;

        /// Reports a single entity straight ahead of the muzzle.
        struct HitStub(Entity);
        impl PhysicsQuery for HitStub {
            fn raycast(&self, origin: Vec3, dir: Vec3, _max: f32) -> Option<RayHit> {
                Some(RayHit { point: origin + dir * 8.0, normal: -dir, distance: 8.0, entity: Some(self.0) })
            }
            fn closest_point(&self, _p: Vec3, _m: f32) -> Option<ClosestPoint> {
                None
            }
            fn move_character(&self, pos: Vec3, delta: Vec3, _r: f32, _h: f32, _s: f32) -> Vec3 {
                pos + delta
            }
        }

        /// Fire one shot at `target` and return the impact kind it produced.
        fn shoot_at(world: &mut World, target: Entity) -> ImpactKind {
            let shooter = world.spawn(());
            let mut w = RuntimeWeapon::new(shooter, WeaponStats::default());
            w.trigger_down = true;
            w.muzzle = Vec3::ZERO;
            w.aim_dir = Vec3::Z;
            world.spawn((w,));
            let mut bus = EventBus::new();
            let mut impacts = Vec::new();
            weapon_firing_system_impacts(world, 1.0 / 60.0, &mut bus, Some(&HitStub(target)), &mut impacts);
            assert_eq!(impacts.len(), 1);
            impacts[0].kind
        }

        let mut world = World::new();
        let car = spawn_default_vehicle(&mut world, VehicleClass::Car, 0x4000, Transform::IDENTITY);
        assert_eq!(shoot_at(&mut world, car), ImpactKind::Bullet, "a vehicle does not bleed");

        let mut world = World::new();
        let tpl = mercs2_formats::hash::pandemic_hash_m2("vz_soldier");
        let npc = spawn_character(&mut world, tpl, tpl, Transform::IDENTITY);
        assert_eq!(shoot_at(&mut world, npc), ImpactKind::Blood, "a person bleeds");
    }

    /// A default vehicle carries the drive system's components, and throttled it is driven forward by
    /// the wired `GameplaySystems::tick` — the entity a vehicle template's spawn activation builds.
    #[test]
    fn default_vehicle_drives_through_gameplay_tick() {
        use crate::gameplay::GameplaySystems;
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut world = World::new();
        let car = spawn_default_vehicle(&mut world, VehicleClass::Car, 1, Transform::from_translation(Vec3::new(0.0, 0.85, 0.0)));
        assert!(world.get::<&Vehicle>(car).is_ok() && world.get::<&WheelSet>(car).is_ok(), "the drive system's components");
        world.get::<&mut VehicleControls>(car).unwrap().accel = 1.0; // throttle

        let audio = Rc::new(RefCell::new(crate::audio::AudioEngine::default()));
        let mut gp = GameplaySystems::new(audio);
        let mut tris = Vec::new(); // tiled ground (small triangles, as real geometry streams)
        for xi in -15..15 {
            for zi in -15..15 {
                let (x0, x1) = (xi as f32, xi as f32 + 1.0);
                let (z0, z1) = (zi as f32, zi as f32 + 1.0);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z0), Vec3::new(x1, 0.0, z1)]);
                tris.push([Vec3::new(x0, 0.0, z0), Vec3::new(x1, 0.0, z1), Vec3::new(x0, 0.0, z1)]);
            }
        }
        gp.set_collision(tris);

        let z0 = world.get::<&Transform>(car).unwrap().translation.z;
        for _ in 0..240 {
            gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!((z1 - z0).abs() > 1.0, "throttled vehicle should drive; dz = {}", z1 - z0);
    }
}
