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

use crate::audio::AudioEngine;
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
    pub ai: crate::ai::AiWorld,
    /// The water mechanism (static watermap + swim FSM). `tick` advances every `Swimmer` against the
    /// watermap; idle until a watermap is loaded. Buoyancy is applied by the physics side.
    pub water: crate::water_sim::WaterWorld,
    /// The decal mechanism (decaltable + bounded instance pool). `tick` ages the pool and GCs expired
    /// decals; idle until decals are spawned. The render seam draws `decal.iter_live()`.
    pub decal: crate::decal::DecalWorld,
    /// The population mechanism (PgSysPopulation spawners + density + death). Ticked via
    /// [`tick_population`](Self::tick_population) (it needs the camera anchor for the death gate); its
    /// emitted `SpawnRequest`s are realized through the same [`SpawnResolver`] as script spawns.
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
    /// Boot-registered render/anim metadata for the bounded set of ambient-population NPC templates,
    /// keyed by template hash (see [`register_npc_model`](Self::register_npc_model)). Populated once the
    /// game has made each faction template resident (mesh in the scene, rig+clips in the AssetStore);
    /// [`tick_population`](Self::tick_population) uses it to give a freshly spawned Character a
    /// rig-sized bind-pose `SkinPalette` (so skinning is right on the first frame, before
    /// `animation_system` runs) and a starting idle clip on its `AnimController` (so the data-driven
    /// `animation_system` advances + samples it). Empty until the preload registers.
    npc_models: std::collections::HashMap<u32, NpcModelInfo>,
}

/// Boot-registered render/anim metadata for one preloaded population NPC template (see
/// [`GameRuntime::register_npc_model`]).
pub struct NpcModelInfo {
    /// Bind-pose skinning palette, sized to the template model's bone count. Attached to a spawned
    /// actor so it renders in bind pose immediately (and stays posed if its idle never resolved).
    pub bind_palette: Vec<[[f32; 4]; 4]>,
    /// A resident idle clip name-hash for this template (present in the AssetStore under the template
    /// model), or 0 when none resolved (the actor renders static — a CONFIRM-LIVE gap).
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

/// Any unit vector perpendicular to `n` — the decal's surface tangent (the projection basis; the exact
/// roll is cosmetic).
fn perp(n: Vec3) -> Vec3 {
    let axis = if n.x.abs() < 0.9 { Vec3::X } else { Vec3::Y };
    n.cross(axis).normalize_or_zero()
}

impl GameRuntime {
    /// A runtime driving `audio` (shared with the Lua `Sound.*` forwarding so one engine is both cued
    /// and ticked). The resolver starts empty — every template resolves to a plain prop until the
    /// reflection/spawn-list data registers the vehicle/character archetypes.
    pub fn new(audio: Rc<RefCell<AudioEngine>>) -> Self {
        // SEAM-1: pre-register the recovered per-faction population templates so a hash-only spawn
        // request (population spawners carry a template HASH, not a name) resolves through
        // `by_template` → `Archetype::Character` → `spawn_character` (the full NAME-aware bundle),
        // instead of falling through to the bare `Prop` archetype. Each name is a real,
        // corpus-verified faction human (`mercs2_population::slot_table`); `populate_from_names`
        // classifies each `_hum_` name to `Character` and records it under `pandemic_hash_m2(name)`.
        let mut resolver = SpawnResolver::new();
        resolver.populate_from_names(mercs2_population::slot_table::FACTION_TEMPLATE_NAMES);
        GameRuntime {
            gameplay: GameplaySystems::new(audio),
            resolver,
            ai: crate::ai::AiWorld::new(),
            water: crate::water_sim::WaterWorld::new(),
            decal: crate::decal::DecalWorld::new(),
            population: crate::population::PopulationWorld::new(),
            next_pop_handle: 0x2000_0000,
            pop_spawned_total: 0,
            render_impacts: Vec::new(),
            npc_models: std::collections::HashMap::new(),
        }
    }

    /// Register a preloaded NPC template's render/anim metadata — called once per faithful faction
    /// template at boot, after the game has uploaded its mesh to the scene + its rig/clips to the
    /// AssetStore. `bind_palette` is the template's bind-pose skinning palette (sized to its bones);
    /// `idle_clip` is a resident idle clip name-hash (0 = none). [`tick_population`](Self::tick_population)
    /// then hands each spawned Character of this template a correctly-sized `SkinPalette` + a starting
    /// idle so it renders + animates through the shared `animation_system`.
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
                // Resolve with the template NAME in hand: a registered archetype wins, else the name is
                // classified on the fly (so `Pg.Spawn("..._veh_car_...")` becomes a drivable Vehicle
                // without pre-registration — Task A).
                let e = self.resolver.spawn_named(world, tpl, Some(&r.template), r.guid as u32, t);
                (e, tpl)
            })
            .collect()
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
    /// this once per loaded block. Returns how many spawners were registered (0 if the block has none).
    /// Before this, `population.spawners` started empty → no crowds/traffic; this is the missing feed.
    pub fn load_population_spawners(&mut self, block: &[u8]) -> usize {
        crate::worldutil::register_population_spawners(block, &mut self.population.spawners)
    }

    /// Advance the population system one fixed step and realize its output. `focus` is the camera/player
    /// anchor the death-distance gate measures against. Emitted `SpawnRequest`s are materialized through
    /// the shared [`SpawnResolver`] (a template hash → the right ECS archetype, exactly as script
    /// `Pg.Spawn`s are), and retired entities are despawned. Idle until spawners are registered.
    pub fn tick_population(&mut self, world: &mut World, dt: f32, focus: Vec3) {
        let mut time = mercs2_core::Time::new(60.0);
        time.dt = dt;
        self.population.tick(world, &time, &[focus]);
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
            // SEAM-1: `req.template` is now a real faction human template hash (CF-1), and the resolver
            // pre-registered every faction template as `Character` in `new()`, so this hash-only spawn
            // routes through `spawn_character` → the full Human/HumanState/perception/AnimController+
            // HumanAnimationSet/Health/AiFaction bundle (not the bare Prop it used to make). A vehicle
            // channel (template 0) or any unregistered hash still falls through to Prop.
            let e = self.resolver.spawn(world, req.template, handle, req.transform);
            // Map the spawn's faction channel onto the actor's AiFaction (CF-2/K3: no longer dropped).
            // 0 is the neutral/unset id, so offset by +1 to keep channel Vz(0) distinct from neutral.
            crate::spawn::set_faction(world, e, req.faction as u32 + 1);
            // Render + animate. `spawn_character` already attached `ModelRef{template_hash}`; the model
            // is resident under that hash (preloaded at boot), so the actor draws. A preloaded template
            // also carries a rig-sized bind-pose `SkinPalette` (correct skinning on the very first frame,
            // before `animation_system` runs) and a starting idle clip on its `AnimController` (so the
            // data-driven `animation_system`, `picker: None`, advances + samples the resident clip). A
            // template with no metadata (a prop/vehicle channel, or one that failed to preload) is left
            // as-is: it stays invisible until/unless its model becomes resident — no fabricated pose.
            if let Some(info) = self.npc_models.get(&req.template) {
                let _ = world.insert_one(e, mercs2_core::SkinPalette { mats: info.bind_palette.clone() });
                if info.idle_clip != 0 {
                    if let Ok(mut ctrl) = world.get::<&mut crate::anim::AnimController>(e) {
                        *ctrl = crate::anim::AnimController::playing(info.idle_clip);
                    }
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
    use crate::spawn::Archetype;
    use crate::vehicle::components::{Vehicle, VehicleClass, VehicleControls};

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
            rt.tick(&mut world, &mut mercs2_player::PlayerWorld::new(), 1.0 / 60.0);
        }
        let z1 = world.get::<&Transform>(car).unwrap().translation.z;
        assert!((z1 - z0).abs() > 1.0, "realized+throttled vehicle should drive; dz = {}", z1 - z0);
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

    /// A registered population spawner fires through `tick_population` and its request is realized into
    /// a full **Character** (not a bare Prop) via the shared resolver — proving CF-1 (real template) +
    /// SEAM-1 (Character routing) + CF-2 (faction mapped onto `AiFaction`) end to end, the whole point
    /// of the "no more invisible template-0 props" work.
    #[test]
    fn tick_population_realizes_a_character_with_anim_and_perception() {
        use crate::ai::{AiFaction, Perception, PerceptionRecord};
        use crate::anim::{AnimController, HumanAnimationSet};
        use crate::population::{SimpleSpawner, SpawnFaction, SpawnerFamily};
        use mercs2_core::{Health, Human};

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);
        rt.population
            .spawners
            .register(SimpleSpawner {
                interval: 1.0,
                countdown: 1.0,
                reload: 1.0,
                faction: SpawnFaction::Gur, // guerrilla channel → gr_hum_starter_1
                family: SpawnerFamily::Window,
                transform: Transform::from_translation(Vec3::new(10.0, 0.0, 0.0)),
                ..SimpleSpawner::default()
            })
            .unwrap();

        let mut world = World::new();
        rt.tick_population(&mut world, 1.0, Vec3::ZERO); // dt 1.0 crosses the 1.0s interval → fires

        // Exactly one Character-bundle actor was realized (a person, not a prop).
        let mut people = world.query::<(&Human, &Transform, &Perception, &PerceptionRecord, &Health, &HumanAnimationSet, &AnimController, &AiFaction)>();
        let realized: Vec<_> = people.iter().collect();
        assert_eq!(realized.len(), 1, "a fired population spawner must realize ONE full Character");
        let (_e, (_hum, t, _perc, _rec, health, anim, _ctrl, faction)) = realized[0];

        // Placed at the spawner's anchor.
        assert_eq!(t.translation, Vec3::new(10.0, 0.0, 0.0));
        // Killable (combat bundle).
        assert_eq!(health.max, 100.0);
        // Animatable: the anim set is keyed by the resolved template hash (gr_hum_starter_1).
        assert_eq!(
            anim.character,
            mercs2_formats::hash::pandemic_hash_m2("gr_hum_starter_1"),
            "the Character's animation set is keyed to the resolved faction template"
        );
        // Faction mapped onto AiFaction (CF-2): channel Gur(3) → id 3+1 = 4 (neutral 0 kept distinct).
        assert_eq!(faction.0, SpawnFaction::Gur as u32 + 1, "spawn faction channel maps to AiFaction");
    }

    /// A population spawn of a PRELOADED template is made drawable + animatable: the actor carries a
    /// `ModelRef` naming its (resident) template model, a `SkinPalette` sized to the registered rig, and
    /// an `AnimController` started on the registered resident idle clip — so `animation_system` advances
    /// + samples it. This is the render/anim seam this workstream closed (population Characters render).
    #[test]
    fn preloaded_population_template_spawns_drawable_and_animating() {
        use crate::anim::AnimController;
        use crate::population::{SimpleSpawner, SpawnFaction, SpawnerFamily};
        use mercs2_core::{ModelRef, SkinPalette};

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio);

        // Preload metadata for the guerrilla template (gr_hum_starter_1): a 3-bone bind palette + idle.
        let gr = mercs2_formats::hash::pandemic_hash_m2("gr_hum_starter_1");
        const IDENT: [[f32; 4]; 4] =
            [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
        let idle = 0x1234_5678;
        rt.register_npc_model(gr, vec![IDENT; 3], idle);
        assert_eq!(rt.npc_model_count(), 1);

        rt.population
            .spawners
            .register(SimpleSpawner {
                interval: 1.0,
                countdown: 1.0,
                reload: 1.0,
                faction: SpawnFaction::Gur, // → gr_hum_starter_1
                family: SpawnerFamily::Window,
                transform: Transform::from_translation(Vec3::new(5.0, 0.0, 0.0)),
                ..SimpleSpawner::default()
            })
            .unwrap();

        let mut world = World::new();
        rt.tick_population(&mut world, 1.0, Vec3::ZERO);

        let mut q = world.query::<(&ModelRef, &SkinPalette, &AnimController)>();
        let hits: Vec<_> = q.iter().collect();
        assert_eq!(hits.len(), 1, "one spawned Character with the render+anim attach");
        let (_e, (mref, pal, ctrl)) = hits[0];
        assert_eq!(mref.model, gr, "ModelRef names the resident template model");
        assert_eq!(pal.mats.len(), 3, "SkinPalette sized to the template's bone count");
        assert_eq!(ctrl.clip, idle, "AnimController started on the resident idle clip");
        assert!(ctrl.playing, "the controller is playing so animation_system advances it");
    }

    /// A population spawn of a template with NO preload metadata still gets its `ModelRef` (from
    /// `spawn_character`) but no `SkinPalette` — invisible until its model becomes resident, never a
    /// fabricated pose. (Vz IS pre-registered as a Character, just not preloaded here.)
    #[test]
    fn unpreloaded_population_template_has_modelref_but_no_palette() {
        use crate::population::{SimpleSpawner, SpawnFaction, SpawnerFamily};
        use mercs2_core::{ModelRef, SkinPalette};

        let audio = Rc::new(RefCell::new(AudioEngine::default()));
        let mut rt = GameRuntime::new(audio); // no register_npc_model
        rt.population
            .spawners
            .register(SimpleSpawner {
                interval: 1.0,
                countdown: 1.0,
                reload: 1.0,
                faction: SpawnFaction::Vz,
                family: SpawnerFamily::Window,
                transform: Transform::from_translation(Vec3::new(1.0, 0.0, 0.0)),
                ..SimpleSpawner::default()
            })
            .unwrap();

        let mut world = World::new();
        rt.tick_population(&mut world, 1.0, Vec3::ZERO);

        // The Vz on-foot channel resolves to the recovered faction human `vz_hum_soldierelite_a`
        // (mercs2_population::slot_table; the old `vz_hum_soldier_a` hashed to no vz.wad asset and was
        // replaced). This is the template the single Vz spawner emits, gate or no gate.
        let vz = mercs2_formats::hash::pandemic_hash_m2("vz_hum_soldierelite_a");
        let mut q = world.query::<&ModelRef>();
        let hits: Vec<_> = q.iter().collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1.model, vz, "the Character still references its template model");
        assert!(
            world.get::<&SkinPalette>(hits[0].0).is_err(),
            "no palette without preload metadata → not drawn until resident"
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
