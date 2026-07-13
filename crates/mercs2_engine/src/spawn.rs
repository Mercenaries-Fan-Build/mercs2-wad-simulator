//! `SpawnResolver` — turns a spawn *template* into the right ECS entity archetype.
//!
//! The connection layer's remaining edge: the game's Lua (`Pg.Spawn(template, x,y,z,yaw)`) and the
//! population spawners create actors by *template name*, and the engine must materialize each as the
//! correct ECS entity — a plain rendered **prop** (Transform + ModelRef, the render loop's existing
//! path) or a full **fleet entity** (e.g. a `Vehicle` bundle the wired `drive_step_system` moves). This
//! resolver is that mapping. Vehicles/weapons aren't authored in the static world blocks — they're
//! *spawned* — so this is the piece that lets a `Pg.Spawn("...car...")` become a drivable entity.
//!
//! The template→archetype table is ultimately populated from the reflection registry (a class carrying
//! `_CarPhysicsV2`/vehicle components resolves to `Vehicle`) / the spawn-list data; until that data is
//! threaded through, callers `register` templates explicitly (the `Pg.Spawn` realize path + tests do).

use std::collections::HashMap;

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

/// Template name-hash → [`Archetype`]. Populated from the reflection registry / spawn-list data;
/// `register` until that's threaded.
#[derive(Default)]
pub struct SpawnResolver {
    by_template: HashMap<u32, Archetype>,
}

impl SpawnResolver {
    pub fn new() -> Self {
        SpawnResolver::default()
    }

    /// Declare that `template_hash` (`pandemic_hash_m2` of the template name) spawns `arch`.
    pub fn register(&mut self, template_hash: u32, arch: Archetype) {
        self.by_template.insert(template_hash, arch);
    }

    /// The archetype a template resolves to (`Prop` if unregistered).
    pub fn archetype(&self, template_hash: u32) -> Archetype {
        self.by_template.get(&template_hash).copied().unwrap_or(Archetype::Prop)
    }

    /// Materialize `template_hash` into `world` at `transform`, returning the entity. A `Vehicle`
    /// archetype spawns the full drivable bundle (the wired `drive_step_system` then moves it); a
    /// `Prop` spawns a bare Transform (the render loop adds `ModelRef`). `handle` = the runtime GUID.
    pub fn spawn(
        &self,
        world: &mut World,
        template_hash: u32,
        handle: u32,
        transform: Transform,
    ) -> Entity {
        match self.archetype(template_hash) {
            Archetype::Vehicle(class) => spawn_default_vehicle(world, class, handle, transform),
            Archetype::Character => spawn_character(world, template_hash, transform),
            Archetype::Prop => world.spawn((transform,)),
        }
    }
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
/// - **AI** (`mercs2_ai`): `Perception`/`Stimulus`/`Target`/`PerceptionRecord` (seen by + sees others),
///   `AiBehavior` (unrestricted), `AiSkill`, `Squad`, and a **neutral `AiFaction(0)`** the caller
///   overrides with the real faction (`set_faction`);
/// - **combat** (`mercs2_combat`): `Health` (100) so damage/death applies;
/// - **animation** (`mercs2_anim`): `HumanAnimationSet` (keyed by the template hash as the character id)
///   + `AnimController`, so the data-driven clip picker can drive it.
///
/// The render layer adds `ModelRef`/`SkinPalette` (model resolution is the render seam). `template_hash`
/// doubles as the animation character id until a template→character map lands.
pub fn spawn_character(world: &mut World, template_hash: u32, transform: Transform) -> Entity {
    use crate::ai::{AiBehavior, AiFaction, AiSkill, Perception, PerceptionRecord, Squad, Stimulus, Target};
    use crate::anim::{AnimController, HumanAnimationSet};
    use crate::combat::components::Health;

    world.spawn((
        transform,
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
        // animation
        HumanAnimationSet::new(template_hash),
        AnimController::default(),
    ))
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
        use crate::combat::components::Health;

        let npc_tpl = mercs2_formats::hash::pandemic_hash_m2("vz_soldier");
        let mut r = SpawnResolver::new();
        r.register(npc_tpl, Archetype::Character);

        let mut world = World::new();
        let npc = r.spawn(&mut world, npc_tpl, 0x2000, Transform::from_translation(Vec3::new(3.0, 0.0, 4.0)));

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

    /// The resolver routes a registered vehicle template to a `Vehicle` entity and everything else to
    /// a plain prop — the `Pg.Spawn`→entity mapping the mission/population path will drive.
    #[test]
    fn resolves_vehicle_template_vs_prop() {
        let car_tpl = mercs2_formats::hash::pandemic_hash_m2("civilian_sedan");
        let mut r = SpawnResolver::new();
        r.register(car_tpl, Archetype::Vehicle(VehicleClass::Car));

        let mut world = World::new();
        let car = r.spawn(&mut world, car_tpl, 0x1000, Transform::from_translation(Vec3::new(0.0, 0.85, 0.0)));
        assert!(world.get::<&Vehicle>(car).is_ok(), "vehicle template must spawn a Vehicle entity");
        assert!(world.get::<&WheelSet>(car).is_ok(), "vehicle must carry wheels for the drive system");

        let prop = r.spawn(&mut world, 0xDEAD_BEEF, 0x1001, Transform::IDENTITY);
        assert!(world.get::<&Vehicle>(prop).is_err(), "unregistered template is a plain prop");
        assert_eq!(r.archetype(0xDEAD_BEEF), Archetype::Prop);
    }

    /// The full spawn path end-to-end: a template resolved to a vehicle, throttled, is driven forward
    /// by the wired `GameplaySystems::tick`. Proves resolver output is a genuinely drivable entity —
    /// exactly what a mission/population `Pg.Spawn("...car...")` will produce at runtime.
    #[test]
    fn resolved_vehicle_drives_through_gameplay_tick() {
        use crate::gameplay::GameplaySystems;
        use std::cell::RefCell;
        use std::rc::Rc;

        let tpl = mercs2_formats::hash::pandemic_hash_m2("test_car");
        let mut r = SpawnResolver::new();
        r.register(tpl, Archetype::Vehicle(VehicleClass::Car));

        let mut world = World::new();
        let car = r.spawn(&mut world, tpl, 1, Transform::from_translation(Vec3::new(0.0, 0.85, 0.0)));
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
            gp.tick(&mut world, 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!((z1 - z0).abs() > 1.0, "resolved+throttled vehicle should drive; dz = {}", z1 - z0);
    }
}
