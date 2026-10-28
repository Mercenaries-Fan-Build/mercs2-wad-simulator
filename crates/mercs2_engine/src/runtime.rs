//! `GameRuntime` — the per-frame *game* update, bundled out of the world loop into one tested unit:
//! the wired fleet ([`GameplaySystems`]: physics / vehicle / combat / audio), AI, water, decals and the
//! ambient population. It holds no GPU state, so the whole game-update side is unit-testable without a
//! window. Script `Pg.Spawn`s are not realized here: the streaming executor registers them and builds
//! their entities at activation (`game_world::StreamingWorld::spawn`).

use std::cell::RefCell;
use std::rc::Rc;

use crate::audio::AudioEngine;
use mercs2_core::glam::Vec3;
use mercs2_core::{Entity, World};

use crate::gameplay::GameplaySystems;

/// The per-frame game update. Owns no GPU state.
pub struct GameRuntime {
    /// Fleet gameplay systems (physics / vehicle / combat / audio), ticked each fixed step.
    pub gameplay: GameplaySystems,
    /// The AI mechanism (recovered action ring + relation matrix). Its per-entity perception update
    /// runs each fixed step over the world (idle until AI entities carry perception components, the
    /// same data-driven way the vehicle/combat systems idle). The `Ai.*` Lua surface drives the same
    /// relation matrix once the persistent mission-Lua host shares this in.
    pub ai: crate::ai::AiWorld,
    /// The water mechanism (static watermap + swim FSM). `tick` advances every `Swimmer` against the
    /// watermap; idle until a watermap is loaded. Buoyancy is applied by the physics side.
    pub water: crate::water_sim::WaterWorld,
    /// The decal mechanism (decaltable + bounded instance pool). `tick` ages the pool and GCs expired
    /// decals; idle until decals are spawned. The render seam draws `decal.iter_live()`.
    pub decal: crate::decal::DecalWorld,
    /// The population mechanism (PgSysPopulation spawners + density + death). Ticked via
    /// [`tick_population`](Self::tick_population) (it needs the camera anchor for the death gate), which
    /// realizes each unit its spawners choose by the unit template's archetype.
    pub population: crate::population::PopulationWorld,
    /// Monotonic runtime GUID source for population-spawned actors (distinct high space so they don't
    /// collide with script-spawned handles).
    next_pop_handle: u32,
    /// Cumulative count of ambient-population actors realized this session — reported on change so a
    /// boot log shows whether the population pipeline is actually producing bodies (not just ticking).
    pop_spawned_total: u32,
    /// Combat impacts this step, stashed after their decals are spawned so the render layer can emit a
    /// matching particle burst (muzzle/impact/explosion FX live on the `Scene`, outside this bundle).
    /// Drained by [`take_render_impacts`](Self::take_render_impacts).
    render_impacts: Vec<crate::combat::Impact>,
    /// Boot-registered render/anim metadata for the population's Character unit templates, keyed by
    /// template key (see [`register_npc_model`](Self::register_npc_model)). Populated once the game has
    /// made each unit's model resident (mesh in the scene, rig+clips in the AssetStore);
    /// [`tick_population`](Self::tick_population) uses it to give a freshly spawned Character a
    /// rig-sized bind-pose `SkinPalette` (so skinning is right on the first frame, before
    /// `animation_system` runs) and a starting idle clip on its `AnimController` (so the data-driven
    /// `animation_system` advances + samples it). Empty until the preload registers.
    npc_models: std::collections::HashMap<u32, NpcModelInfo>,
    /// Every population unit template (archetype, model, faction), by template key — set with the unit
    /// catalog by [`set_population_data`](Self::set_population_data).
    unit_templates: std::collections::HashMap<u32, crate::game_world::UnitTemplate>,
}

/// Boot-registered render/anim metadata for one preloaded population NPC template (see
/// [`GameRuntime::register_npc_model`]).
pub struct NpcModelInfo {
    /// Bind-pose skinning palette, sized to the template model's bone count. Attached to a spawned
    /// actor so it renders in bind pose immediately (and stays posed if its idle never resolved).
    pub bind_palette: Vec<[[f32; 4]; 4]>,
    /// The template's resident idle clip name-hash (present in the AssetStore under the template's
    /// model).
    pub idle_clip: u32,
}

/// Map a combat [`ImpactKind`](crate::combat::ImpactKind) to the decal it leaves — the three
/// combat-produced `decaltable` categories.
fn impact_decal_type(kind: crate::combat::ImpactKind) -> crate::decal::DecalType {
    match kind {
        crate::combat::ImpactKind::Bullet => crate::decal::DecalType::BulletHole,
        crate::combat::ImpactKind::Explosion => crate::decal::DecalType::Scorch,
        crate::combat::ImpactKind::Blood => crate::decal::DecalType::Blood,
    }
}

/// A population spawner owner's health fraction for the `Health*` unit rules — `FUN_004E3DD0`'s
/// whole-object branch (runtime spawner `+0x48 == -1`): the owner's health `cur / max` clamped to
/// `0..=1`; 0 when the owner has no live entity, no `Health`, or `max < 1e-4` (`DAT_00B977F8`).
/// Retail's other branch (`+0x48` = a node hash: the owner's `RuntimeNodeHealth` node, u16 `max` at
/// `+4` and `cur` at `+6`) is not modelled — the spawner field that sets `+0x48` is not traced.
pub fn spawner_owner_health(world: &World, owner: Option<Entity>) -> f32 {
    const MIN_MAX: f32 = 1e-4;
    match owner.and_then(|e| world.get::<&mercs2_core::Health>(e).ok().map(|h| *h)) {
        Some(h) if h.max >= MIN_MAX => (h.cur / h.max).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// Any unit vector perpendicular to `n` — the decal's surface tangent (the projection basis; the exact
/// roll is cosmetic).
fn perp(n: Vec3) -> Vec3 {
    let axis = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    n.cross(axis).normalize_or_zero()
}

impl GameRuntime {
    /// A runtime driving `audio` (shared with the Lua `Sound.*` forwarding so one engine is both cued
    /// and ticked).
    pub fn new(audio: Rc<RefCell<AudioEngine>>) -> Self {
        GameRuntime {
            gameplay: GameplaySystems::new(audio),
            ai: crate::ai::AiWorld::new(),
            water: crate::water_sim::WaterWorld::new(),
            decal: crate::decal::DecalWorld::new(),
            population: crate::population::PopulationWorld::new(),
            next_pop_handle: 0x2000_0000,
            pop_spawned_total: 0,
            render_impacts: Vec::new(),
            npc_models: std::collections::HashMap::new(),
            unit_templates: std::collections::HashMap::new(),
        }
    }

    /// Hand the population its unit lists and unit templates ([`crate::game_world::PopulationData`]).
    /// Call before [`load_population_spawners`](Self::load_population_spawners): spawners are checked
    /// against the lists when they register.
    pub fn set_population_data(&mut self, data: crate::game_world::PopulationData) {
        self.population.units = data.catalog;
        self.unit_templates = data.units;
    }

    /// The population unit template `template`, if it is one.
    pub fn unit_template(&self, template: u32) -> Option<crate::game_world::UnitTemplate> {
        self.unit_templates.get(&template).copied()
    }

    /// Register a preloaded population character template's render/anim metadata — called once per
    /// template at boot, after the game has uploaded its model to the scene + its rig/clips to the
    /// AssetStore. `bind_palette` is the model's bind-pose skinning palette (sized to its bones);
    /// `idle_clip` is a resident idle clip name-hash. [`tick_population`](Self::tick_population) then
    /// hands each spawned Character of this template a correctly-sized `SkinPalette` + a starting idle
    /// so it renders + animates through the shared `animation_system`.
    pub fn register_npc_model(&mut self, template_hash: u32, bind_palette: Vec<[[f32; 4]; 4]>, idle_clip: u32) {
        self.npc_models.insert(template_hash, NpcModelInfo { bind_palette, idle_clip });
    }

    /// How many NPC templates have been registered as resident (for a boot-log confirmation).
    pub fn npc_model_count(&self) -> usize {
        self.npc_models.len()
    }

    /// Hand the fleet physics its static collision world (the streamed structural geometry). See
    /// [`GameplaySystems::set_collision`].
    pub fn set_collision(&mut self, tris: Vec<[Vec3; 3]>) {
        self.gameplay.set_collision(tris);
    }

    /// Hand the fleet physics the terrain heightfield (open-ground raycasts). See
    /// [`GameplaySystems::set_heightmap`].
    pub fn set_heightmap(&mut self, heightmap: Option<crate::physics::Heightmap>) {
        self.gameplay.set_heightmap(heightmap);
    }

    /// Advance the per-frame game update one fixed step over `world`: the fleet gameplay systems
    /// (physics / vehicle / combat / audio), the AI per-entity perception update (§2.4), the water swim
    /// FSM, and the decal pool aging. Every one idles until entities/content carry their components —
    /// the same data-driven way the engine's systems idle. (Population needs the camera anchor, so it's
    /// [`tick_population`](Self::tick_population).)
    ///
    /// `player` is the script host's [`mercs2_player::PlayerWorld`], passed through to the roster
    /// passes that open the recovered layer-4 order.
    pub fn tick(&mut self, world: &mut World, player: &mut mercs2_player::PlayerWorld, dt: f32) {
        self.gameplay.tick(world, player, dt);
        // Combat impacts → projected decals (bullet holes / scorch / blood) + stash for particle FX.
        // The decal pool + the render impacts are now fed by a real producer (was dead bookkeeping).
        let impacts = self.gameplay.take_impacts();
        for imp in &impacts {
            self.decal.spawn(impact_decal_type(imp.kind), imp.position, imp.normal, perp(imp.normal));
        }
        self.render_impacts.extend(impacts);
        self.ai.tick(world);
        self.water.tick(world, dt);
        self.decal.update(dt);
    }

    /// Register a hit produced outside the ECS combat tick — the local hero firing their weapon (the
    /// player is a controller, not a `RuntimeWeapon` ECS entity). Spawns the impact's decal and stashes
    /// it for the particle burst, exactly as [`tick`](Self::tick) handles the combat-system impacts.
    pub fn push_impact(&mut self, imp: crate::combat::Impact) {
        self.decal.spawn(impact_decal_type(imp.kind), imp.position, imp.normal, perp(imp.normal));
        self.render_impacts.push(imp);
    }

    /// Drain the combat impacts recorded this frame so the render layer can emit a particle burst at
    /// each (the FX sink lives on the `Scene`). Drain-then-clear.
    pub fn take_render_impacts(&mut self) -> Vec<crate::combat::Impact> {
        std::mem::take(&mut self.render_impacts)
    }

    /// Feed the ambient-population content from a world block: read its `PopulationSimpleSpawner` COMP
    /// and register each authored spawner instance into the population manager (Task B). The base layer
    /// (`layers_static`) plus any streamed `vz_state_*` overlays each carry their own spawners; call
    /// this once per loaded block, after [`set_population_data`](Self::set_population_data). Returns how
    /// many spawners were registered (0 if the block has none); an error when a spawner names no
    /// `PopulationList` or no `SpawnerWeightTypeEnum` value.
    pub fn load_population_spawners(&mut self, block: &[u8]) -> Result<usize, String> {
        crate::worldutil::register_population_spawners(block, &mut self.population)
    }

    /// Advance the population system one fixed step and realize its output. `focus` is the camera/player
    /// anchor the death-distance gate measures against; `next` is the game's random draw
    /// (`Lcg::next_draw` of the shared game random state); `owner_health(world, owner)` is a spawner
    /// owner's health fraction for the `Health*` unit rules. Each chosen unit is realized by its template's
    /// archetype — a Character (perception, faction from the template's `Association`, health,
    /// animation), a drivable Vehicle, or a Prop — with `ModelRef` = the template's `ModelName`; retired
    /// entities are despawned. Idle until spawners are registered.
    pub fn tick_population(
        &mut self,
        world: &mut World,
        dt: f32,
        focus: Vec3,
        next: &mut dyn FnMut() -> u32,
        owner_health: &dyn Fn(&World, u32) -> f32,
    ) {
        use crate::spawn::Archetype;
        let mut time = mercs2_core::Time::new(60.0);
        time.dt = dt;
        self.population.tick(world, &time, &[focus], next, owner_health);
        let mut spawned_this_tick = 0u32;
        let (mut pmin, mut pmax) = ([f32::MAX; 3], [f32::MIN; 3]);
        for req in self.population.take_requests() {
            let handle = self.next_pop_handle;
            self.next_pop_handle = self.next_pop_handle.wrapping_add(1);
            spawned_this_tick += 1;
            let tp = req.transform.translation.to_array();
            for c in 0..3 {
                pmin[c] = pmin[c].min(tp[c]);
                pmax[c] = pmax[c].max(tp[c]);
            }
            let unit = self
                .unit_templates
                .get(&req.template)
                .unwrap_or_else(|| panic!("population unit 0x{:08X}: the catalog names only loaded templates", req.template));
            let e = match unit.archetype {
                Archetype::Character => {
                    let e = crate::spawn::spawn_character(world, req.template, unit.model, req.transform);
                    crate::spawn::set_faction(world, e, unit.faction);
                    e
                }
                Archetype::Vehicle(class) => {
                    let e = crate::spawn::spawn_default_vehicle(world, class, handle, req.transform);
                    let _ = world.insert_one(e, mercs2_core::ModelRef { model: unit.model });
                    e
                }
                Archetype::Prop => world.spawn((req.transform, mercs2_core::ModelRef { model: unit.model })),
            };
            // Render + animate. A preloaded character template carries a rig-sized bind-pose
            // `SkinPalette` (correct skinning on the very first frame, before `animation_system` runs)
            // and a starting idle clip on its `AnimController` (so the data-driven `animation_system`,
            // `picker: None`, advances + samples the resident clip).
            if let Some(info) = self.npc_models.get(&req.template) {
                let _ = world.insert_one(e, mercs2_core::SkinPalette { mats: info.bind_palette.clone() });
                if let Ok(mut ctrl) = world.get::<&mut crate::anim::AnimController>(e) {
                    *ctrl = crate::anim::AnimController::playing(info.idle_clip);
                }
            }
        }
        for e in self.population.take_retired() {
            let _ = world.despawn(e);
        }
        if spawned_this_tick > 0 {
            self.pop_spawned_total += spawned_this_tick;
            println!(
                "[pop] realized {spawned_this_tick} ambient at X[{:.0},{:.0}] Y[{:.0},{:.0}] Z[{:.0},{:.0}] (session total {}); focus=({:.0},{:.0},{:.0})",
                pmin[0], pmax[0], pmin[1], pmax[1], pmin[2], pmax[2], self.pop_spawned_total, focus.x, focus.y, focus.z
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vehicle::components::{Vehicle, VehicleClass};
    use mercs2_core::Transform;

    /// `FUN_004E3DD0`'s whole-object owner health: `cur / max` clamped; 0 for no owner, no `Health`, or
    /// a `max` under `1e-4`.
    #[test]
    fn spawner_owner_health_follows_the_retail_rule() {
        let mut world = World::new();
        let half = world.spawn((mercs2_core::Health { cur: 30.0, max: 60.0 },));
        let over = world.spawn((mercs2_core::Health { cur: 90.0, max: 60.0 },));
        let tiny = world.spawn((mercs2_core::Health { cur: 1.0, max: 5e-5 },));
        let bare = world.spawn((Transform::default(),));
        assert_eq!(spawner_owner_health(&world, Some(half)), 0.5);
        assert_eq!(spawner_owner_health(&world, Some(over)), 1.0);
        assert_eq!(spawner_owner_health(&world, Some(tiny)), 0.0);
        assert_eq!(spawner_owner_health(&world, Some(bare)), 0.0);
        assert_eq!(spawner_owner_health(&world, None), 0.0);
    }

    /// Each combat `ImpactKind` maps to its `decaltable` decal (the producer→pool wire that fills the
    /// previously-empty decal pool).
    #[test]
    fn impact_kinds_map_to_their_decals() {
        use crate::combat::ImpactKind;
        use crate::decal::DecalType;
        assert_eq!(impact_decal_type(ImpactKind::Bullet), DecalType::BulletHole);
        assert_eq!(impact_decal_type(ImpactKind::Explosion), DecalType::Scorch);
        assert_eq!(impact_decal_type(ImpactKind::Blood), DecalType::Blood);
        // perp is a unit vector orthogonal to the surface normal.
        let t = perp(Vec3::Y);
        assert!((t.length() - 1.0).abs() < 1e-3 && t.dot(Vec3::Y).abs() < 1e-3);
    }

    /// A player weapon hit (the hero firing, fed via `push_impact`) becomes a drainable render impact —
    /// the wire that carries a bullet hole + particle burst from the local hero's shot into the FX sink.
    #[test]
    fn player_shot_pushes_a_render_impact() {
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        let hit = Vec3::new(2.0, 1.0, 5.0);
        rt.push_impact(crate::combat::Impact::from_hit(hit, Vec3::ZERO, Vec3::Z, false));
        let drained = rt.take_render_impacts();
        assert_eq!(drained.len(), 1, "the player shot should record exactly one render impact");
        assert_eq!(drained[0].position, hit);
        assert_eq!(drained[0].kind, crate::combat::ImpactKind::Bullet);
        // Drained: the next frame starts clean.
        assert!(rt.take_render_impacts().is_empty());
    }

    /// The AI perception update runs through `GameRuntime::tick`: a hostile observer in range makes the
    /// target's perception record show a hostile-aware observer — proving the recovered AI mechanism is
    /// wired into the per-frame game update alongside the fleet, idle until AI entities exist.
    #[test]
    fn tick_runs_ai_perception_over_the_world() {
        use crate::ai::{AiFaction, Perception, PerceptionRecord, Stimulus, Target};

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

        rt.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        assert_eq!(
            world.get::<&PerceptionRecord>(watched).unwrap().hostile_aware, 1,
            "AI perception must run through the runtime tick"
        );
    }

    /// The decal pool ages through `GameRuntime::tick` (proving `decal.update` is wired into the
    /// per-frame update): a spawned decal survives a short step and stays live.
    #[test]
    fn tick_ages_the_decal_pool() {
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        rt.decal.spawn(crate::decal::DecalType::BulletHole, Vec3::new(1.0, 0.0, 0.0), Vec3::Y, Vec3::X);
        assert_eq!(rt.decal.pool.live_count(), 1);

        let mut world = World::new();
        rt.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0); // decal.update runs inside tick
        assert_eq!(rt.decal.pool.live_count(), 1, "a fresh decal survives a short tick");
    }

    /// "Guerilla Starter 01" (`0x8000818F`, model `gr_hum_starter_1`, faction "Guerilla"), a car and the
    /// list (`Simple`) the test spawners draw from.
    const PERSON: u32 = 0x8000_818F;
    const PERSON_MODEL: u32 = 0xEEAA_BF91;
    const GUERILLA: u32 = 0x8000_2CF1;
    const CAR: u32 = 0x8000_4F44;
    const CAR_MODEL: u32 = 0x1111_2222;
    const LIST: u32 = 0x8000_0100;

    /// A runtime whose population draws from one list of `entries`, PERSON and CAR as its templates.
    fn population_runtime(entries: &[(u32, i32)]) -> GameRuntime {
        use crate::game_world::{PopulationData, UnitTemplate};
        use crate::spawn::Archetype;
        use mercs2_formats::unit_list::UnitListEntry;
        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        let list = entries.iter().map(|&(unit, weight)| UnitListEntry { unit, weight }).collect();
        rt.set_population_data(PopulationData {
            catalog: crate::population::UnitCatalog::new(
                std::collections::BTreeMap::from([(LIST, list)]),
                std::collections::BTreeSet::from([CAR]),
            )
            .unwrap(),
            units: std::collections::HashMap::from([
                (PERSON, UnitTemplate { archetype: Archetype::Character, model: PERSON_MODEL, faction: GUERILLA }),
                (CAR, UnitTemplate { archetype: Archetype::Vehicle(VehicleClass::Car), model: CAR_MODEL, faction: 0 }),
            ]),
        });
        rt
    }

    /// A spawner at `at` that fires on the first 1 s tick, drawing from LIST.
    fn list_spawner(at: Vec3) -> crate::population::SimpleSpawner {
        use crate::population::{SimpleSpawner, SpawnerFamily, UnitChoice};
        SimpleSpawner {
            interval: 1.0,
            countdown: 1.0,
            reload: 1.0,
            choice: UnitChoice { owner: 0x8000_0F00, unit_list: LIST, weighting: 1 },
            family: SpawnerFamily::Window,
            transform: Transform::from_translation(at),
            ..SimpleSpawner::default()
        }
    }

    fn tick(rt: &mut GameRuntime, world: &mut World) {
        rt.tick_population(world, 1.0, Vec3::ZERO, &mut || 0, &|_, _| 1.0); // dt 1.0 crosses the interval
    }

    /// A registered population spawner fires through `tick_population`, its list chooses the unit, and
    /// the unit's template is realized into a full **Character**: model from its `ModelName`, faction
    /// from its `Association`.
    #[test]
    fn tick_population_realizes_a_character_with_anim_and_perception() {
        use crate::ai::{AiFaction, Perception, PerceptionRecord};
        use crate::anim::{AnimController, HumanAnimationSet};
        use mercs2_core::{Health, Human, ModelRef};

        let mut rt = population_runtime(&[(PERSON, 1)]);
        rt.population.spawners.register(list_spawner(Vec3::new(10.0, 0.0, 0.0))).unwrap();
        let mut world = World::new();
        tick(&mut rt, &mut world);

        // Exactly one Character-bundle actor was realized (a person, not a prop).
        let mut people = world.query::<(&Human, &Transform, &Perception, &PerceptionRecord, &Health, &HumanAnimationSet, &AnimController, &AiFaction)>();
        let realized: Vec<_> = people.iter().collect();
        assert_eq!(realized.len(), 1, "a fired population spawner must realize ONE full Character");
        let (e, (_hum, t, _perc, _rec, health, anim, _ctrl, faction)) = realized[0];
        assert_eq!(t.translation, Vec3::new(10.0, 0.0, 0.0), "placed at the spawner's anchor");
        assert_eq!(health.max, 100.0, "killable (combat bundle)");
        assert_eq!(anim.character, PERSON, "the animation set is keyed to the unit template");
        assert_eq!(faction.0, GUERILLA, "AiFaction = the template's Association");
        drop(people);
        assert_eq!(world.get::<&ModelRef>(e).unwrap().model, PERSON_MODEL, "ModelRef = the template's ModelName");
    }

    /// A vehicle unit realizes a drivable Vehicle of its physics class, with its model.
    #[test]
    fn tick_population_realizes_a_vehicle_unit() {
        use mercs2_core::ModelRef;
        let mut rt = population_runtime(&[(CAR, 1)]);
        rt.population.spawners.register(list_spawner(Vec3::new(3.0, 0.0, 0.0))).unwrap();
        let mut world = World::new();
        tick(&mut rt, &mut world);
        let mut q = world.query::<(&Vehicle, &ModelRef)>();
        let hits: Vec<_> = q.iter().map(|(_, (v, m))| (v.class, m.model)).collect();
        assert_eq!(hits, vec![(VehicleClass::Car, CAR_MODEL)]);
    }

    /// A population spawn of a PRELOADED template is made drawable + animatable: the actor carries a
    /// `ModelRef` naming its (resident) template model, a `SkinPalette` sized to the registered rig, and
    /// an `AnimController` started on the registered resident idle clip — so `animation_system` advances
    /// + samples it. This is the render/anim seam this workstream closed (population Characters render).
    #[test]
    fn preloaded_population_template_spawns_drawable_and_animating() {
        use crate::anim::AnimController;
        use mercs2_core::{ModelRef, SkinPalette};

        let mut rt = population_runtime(&[(PERSON, 1)]);
        // Preload metadata for the template: a 3-bone bind palette + idle.
        const IDENT: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let idle = 0x1234_5678;
        rt.register_npc_model(PERSON, vec![IDENT; 3], idle);
        assert_eq!(rt.npc_model_count(), 1);
        rt.population.spawners.register(list_spawner(Vec3::new(5.0, 0.0, 0.0))).unwrap();

        let mut world = World::new();
        tick(&mut rt, &mut world);

        let mut q = world.query::<(&ModelRef, &SkinPalette, &AnimController)>();
        let hits: Vec<_> = q.iter().collect();
        assert_eq!(hits.len(), 1, "one spawned Character with the render+anim attach");
        let (_e, (mref, pal, ctrl)) = hits[0];
        assert_eq!(mref.model, PERSON_MODEL, "ModelRef names the resident template model");
        assert_eq!(pal.mats.len(), 3, "SkinPalette sized to the template's bone count");
        assert_eq!(ctrl.clip, idle, "AnimController started on the resident idle clip");
        assert!(ctrl.playing, "the controller is playing so animation_system advances it");
    }

    /// A population spawn of a template with NO preload metadata still gets its `ModelRef` (from
    /// `spawn_character`) but no `SkinPalette` — invisible until its model becomes resident, never a
    /// fabricated pose.
    #[test]
    fn unpreloaded_population_template_has_modelref_but_no_palette() {
        use mercs2_core::{ModelRef, SkinPalette};

        let mut rt = population_runtime(&[(PERSON, 1)]); // no register_npc_model
        rt.population.spawners.register(list_spawner(Vec3::new(1.0, 0.0, 0.0))).unwrap();
        let mut world = World::new();
        tick(&mut rt, &mut world);

        let mut q = world.query::<&ModelRef>();
        let hits: Vec<_> = q.iter().collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.model, PERSON_MODEL, "the Character still references its template model");
        assert!(
            world.get::<&SkinPalette>(hits[0].0).is_err(),
            "no palette without preload metadata → not drawn until resident"
        );
    }

}
