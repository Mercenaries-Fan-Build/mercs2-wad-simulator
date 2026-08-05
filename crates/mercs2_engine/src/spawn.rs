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

/// Classify a spawn *template* to its [`Archetype`] from its authored name (Task A).
///
/// GROUNDING: `docs/modernization/object_assembly_model.md §3` — `Pg.Spawn(name|hash)` resolves the
/// name registry (`@0xDF6B88`) to a **template = a COMP set on disk**, and the archetype follows that
/// COMP set: a class carrying a vehicle-physics/controller component (`_CarPhysicsV2`/`TankPhysics`/…)
/// is a Vehicle; one carrying the Human/AI components is a Character; everything else is a Prop. The
/// component-membership signal we can read WITHOUT a per-template COMP-set extraction is the retail
/// **naming convention** (memory `aset-name-export`, hash-verified, proven): models are named
/// `<faction>_veh_<class>_<airframe>` for vehicles (classes car/truck/apc/semi/motorcycle/tank/boat/
/// helicopter/vtol/plane) and `<faction>_hum_<…>` for people. The `_veh_`/`_hum_` token IS the
/// authored membership marker (the physics-actor COMP is attached by the vehicle system keyed off it),
/// and the class token selects the [`VehicleClass`]. Names hash via `pandemic_hash_m2`, so a template
/// hash keys back to the same archetype. Anything that matches neither token stays a `Prop` — the safe
/// default (a bare rendered entity), never a fabricated vehicle/character.
pub fn classify_template(name: &str) -> Archetype {
    let n = name.to_ascii_lowercase();
    let seg = |t: &str| n.split(|c| c == '_' || c == ' ').any(|s| s == t);
    // Vehicle: the `_veh_` token, with the class token immediately after it picking the actor class.
    if let Some(class) = vehicle_class_after_veh(&n) {
        return Archetype::Vehicle(class);
    }
    // Person: the `_hum_` token (retail human-model convention).
    if seg("hum") {
        return Archetype::Character;
    }
    Archetype::Prop
}

/// The [`VehicleClass`] named by the token right after a `veh` segment, or `None` if the name carries
/// no `veh` segment. An unrecognised class token defaults to `Car` (the primary simulated actor).
fn vehicle_class_after_veh(lower_name: &str) -> Option<VehicleClass> {
    let mut segs = lower_name.split(|c| c == '_' || c == ' ');
    while let Some(s) = segs.next() {
        if s == "veh" {
            let token = segs.next().unwrap_or("");
            return Some(match token {
                "motorcycle" | "motorbike" | "bike" => VehicleClass::Bike,
                "tank" => VehicleClass::Tank,
                "boat" | "ship" | "jetski" => VehicleClass::Boat,
                "helicopter" | "heli" | "vtol" | "chopper" => VehicleClass::Helicopter,
                "plane" | "jet" => VehicleClass::Jet,
                // car/truck/apc/semi/suv/van and any other vehicle token → the Car actor.
                _ => VehicleClass::Car,
            });
        }
    }
    None
}

/// Template name-hash → [`Archetype`]. Populated from template names by [`classify_template`]
/// ([`register_name`](SpawnResolver::register_name) / [`populate_from_names`](SpawnResolver::populate_from_names)),
/// or by explicit [`register`](SpawnResolver::register). A script `Pg.Spawn` whose template name is in
/// hand also resolves through [`classify_template`] on the fly ([`resolve`](SpawnResolver::resolve)),
/// so it need not be pre-registered.
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

    /// Classify `name` via [`classify_template`] and record it under `pandemic_hash_m2(name)` so a
    /// later hash-only spawn (e.g. a population request carrying a template hash) resolves to the same
    /// archetype. Returns the archetype. `Prop` results are not stored (that is `archetype`'s default),
    /// keeping the table to the vehicles/characters that actually need an override.
    pub fn register_name(&mut self, name: &str) -> Archetype {
        let arch = classify_template(name);
        if arch != Archetype::Prop {
            self.by_template
                .insert(mercs2_formats::hash::pandemic_hash_m2(name), arch);
        }
        arch
    }

    /// Bulk-populate `by_template` from a set of template names (e.g. the ASET/name-registry roster),
    /// classifying each by [`classify_template`]. Returns how many resolved to a non-`Prop` archetype
    /// (the ones actually recorded).
    pub fn populate_from_names<I, S>(&mut self, names: I) -> usize
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        names
            .into_iter()
            .filter(|n| self.register_name(n.as_ref()) != Archetype::Prop)
            .count()
    }

    /// Number of templates with a recorded (non-`Prop`) archetype.
    pub fn len(&self) -> usize {
        self.by_template.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_template.is_empty()
    }

    /// The archetype a template resolves to (`Prop` if unregistered).
    pub fn archetype(&self, template_hash: u32) -> Archetype {
        self.by_template.get(&template_hash).copied().unwrap_or(Archetype::Prop)
    }

    /// Resolve a spawn to its archetype, preferring a recorded `by_template` entry and otherwise
    /// classifying the template *name* on the fly (so a script `Pg.Spawn("civ_veh_car_…")` becomes a
    /// `Vehicle` without pre-registration). `Prop` when neither a record nor a name is available.
    pub fn resolve(&self, template_hash: u32, name: Option<&str>) -> Archetype {
        if let Some(a) = self.by_template.get(&template_hash) {
            return *a;
        }
        match name {
            Some(n) => classify_template(n),
            None => Archetype::Prop,
        }
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
        self.spawn_named(world, template_hash, None, handle, transform)
    }

    /// Like [`spawn`](Self::spawn), but with the template *name* in hand so an unregistered template is
    /// classified on the fly via [`classify_template`] (the script `Pg.Spawn` path). `template_hash`
    /// should be `pandemic_hash_m2(name)` when `name` is `Some`.
    pub fn spawn_named(
        &self,
        world: &mut World,
        template_hash: u32,
        name: Option<&str>,
        handle: u32,
        transform: Transform,
    ) -> Entity {
        match self.resolve(template_hash, name) {
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
/// The character's `ModelRef` IS attached here — a faction human's mesh is stored in the WAD under its
/// template hash, so `ModelRef{ model: template_hash }` names the model the render loop draws the moment
/// that model is resident. The `SkinPalette` is attached where the resident bone count is known (the
/// population preload path in [`crate::runtime::GameRuntime::tick_population`], which sizes it to the
/// template's rig); `animation_system` then rewrites it each tick from the sampled pose. `template_hash`
/// doubles as the animation character id until a template→character map lands.
pub fn spawn_character(world: &mut World, template_hash: u32, transform: Transform) -> Entity {
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
    // The model IS the template (faction human mesh keyed by the template hash). Attached separately —
    // the bundle above is already at hecs's max tuple arity. The render loop draws it once the model is
    // resident; the population preload path attaches the rig-sized `SkinPalette`.
    let _ = world.insert_one(e, mercs2_core::ModelRef { model: template_hash });
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

    /// A spawned character is a **person**: it carries the `Human` marker + a default `HumanState`, the
    /// vocabulary every people-acting system queries. It is deliberately NOT `PlayerControlled` — retail
    /// possession is applied on attach (`FUN_006A4060` writes the character GUID to `player+0x20`),
    /// never at spawn.
    #[test]
    fn character_spawns_as_a_human_but_unpossessed() {
        use mercs2_core::{HumanState, PlayerControlled, ANY_STATE};

        let tpl = mercs2_formats::hash::pandemic_hash_m2("vz_civilian");
        let mut world = World::new();
        let npc = spawn_character(&mut world, tpl, Transform::IDENTITY);

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
        let npc = spawn_character(&mut world, tpl, Transform::IDENTITY);
        assert_eq!(shoot_at(&mut world, npc), ImpactKind::Blood, "a person bleeds");
    }

    /// The name-convention classifier (Task A): `_veh_<class>` → the right `VehicleClass`, `_hum_` →
    /// Character, anything else → Prop. Grounded in the retail naming convention (`aset-name-export`).
    #[test]
    fn classify_template_by_naming_convention() {
        use VehicleClass::*;
        assert_eq!(classify_template("civ_veh_car_sedan_a"), Archetype::Vehicle(Car));
        assert_eq!(classify_template("pmc_veh_truck_flatbed"), Archetype::Vehicle(Car));
        assert_eq!(classify_template("ch_veh_tank_t72"), Archetype::Vehicle(Tank));
        assert_eq!(classify_template("vz_veh_apc_stryker"), Archetype::Vehicle(Car));
        assert_eq!(classify_template("civ_veh_motorcycle_a"), Archetype::Vehicle(Bike));
        assert_eq!(classify_template("pr_veh_boat_patrol"), Archetype::Vehicle(Boat));
        assert_eq!(classify_template("oc_veh_helicopter_mi26"), Archetype::Vehicle(Helicopter));
        assert_eq!(classify_template("al_veh_vtol_harrier"), Archetype::Vehicle(Helicopter));
        assert_eq!(classify_template("pmc_veh_plane_c130"), Archetype::Vehicle(Jet));
        assert_eq!(classify_template("vz_hum_soldier_a"), Archetype::Character);
        assert_eq!(classify_template("civ_hum_male_business"), Archetype::Character);
        // Non-vehicle/non-human tokens (incl. the tricky "vehicle depot" prop) stay Prop.
        assert_eq!(classify_template("global_trashcana"), Archetype::Prop);
        assert_eq!(classify_template("jungle_env_plantlarge04"), Archetype::Prop);
    }

    /// `register_name`/`populate_from_names` record the classified archetype under the template hash,
    /// and `resolve` also classifies an unregistered name on the fly.
    #[test]
    fn resolver_populates_and_resolves_by_name() {
        let mut r = SpawnResolver::new();
        let n = r.populate_from_names(["ch_veh_tank_t72", "vz_hum_soldier_a", "global_barrel"]);
        assert_eq!(n, 2, "only the tank + soldier are non-Prop");
        assert_eq!(r.len(), 2, "Props are not stored");

        // Recorded: hash-only lookup resolves.
        let tank = mercs2_formats::hash::pandemic_hash_m2("ch_veh_tank_t72");
        assert_eq!(r.archetype(tank), Archetype::Vehicle(VehicleClass::Tank));
        // On-the-fly: an unregistered name still classifies through resolve().
        let car = mercs2_formats::hash::pandemic_hash_m2("civ_veh_car_sedan_a");
        assert_eq!(r.resolve(car, Some("civ_veh_car_sedan_a")), Archetype::Vehicle(VehicleClass::Car));
        // Hash-only with no name and no record → Prop.
        assert_eq!(r.resolve(0xDEAD_BEEF, None), Archetype::Prop);
    }

    /// `spawn_named` materializes a Vehicle from a bare template NAME (no pre-registration) — the
    /// script `Pg.Spawn` path.
    #[test]
    fn spawn_named_classifies_and_spawns_a_vehicle() {
        let r = SpawnResolver::new();
        let name = "pmc_veh_car_sedan";
        let h = mercs2_formats::hash::pandemic_hash_m2(name);
        let mut world = World::new();
        let car = r.spawn_named(&mut world, h, Some(name), 0x9000, Transform::IDENTITY);
        assert!(world.get::<&Vehicle>(car).is_ok(), "named vehicle template spawns a Vehicle");
        assert!(world.get::<&WheelSet>(car).is_ok());
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
            gp.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!((z1 - z0).abs() > 1.0, "resolved+throttled vehicle should drive; dz = {}", z1 - z0);
    }
}
