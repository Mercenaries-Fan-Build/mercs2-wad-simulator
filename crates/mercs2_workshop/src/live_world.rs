//! The Live page's mirror: the world the game is in, on the workshop's renderer.
//!
//! The engine's own streaming executor runs here with the hero as its viewer, and the layers it loads
//! are the ones the game reports, so a mission's layer swap shows up in the mirror. The humans and
//! vehicles the game reports are drawn at their reported poses, in bind pose: no bridge read gives
//! animation. This world is the mirror's own, so nothing of Inspect's shows and Inspect loses nothing.

use crate::app::{default_render_state, load_model_data, WadStack};
use crate::live_link::{Entity, EntityClass, Live};
use mercs2_core::{AnimState, Entity as Ecs, GuidMap, LayerOp, ModelRef, SkinPalette, Transform, World};
use mercs2_engine::game_world::{load_streaming_world_data, CollisionOp, StreamingWorld, StreamingWorldData};
use mercs2_engine::render::LoadProgress;
use mercs2_engine::scene::Scene;
use mercs2_formats::hash::pandemic_hash_m2;
use glam::{Quat, Vec3};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

/// The stages `load_streaming_world_data` reports.
const LOAD_STAGES: u32 = 5;
/// How many actor models may load in one frame: a load is a synchronous archive read.
const MODELS_PER_FRAME: usize = 1;
/// How far an actor moves toward its newest sample each frame, for a 2 Hz feed.
const ACTOR_EASE: f32 = 0.2;

/// What comes back from the loader thread.
struct Loaded {
    data: StreamingWorldData,
    map: Option<(u32, u32, Vec<u8>)>,
}

enum State {
    Idle,
    Loading(Receiver<Result<Box<Loaded>, String>>, Arc<LoadProgress>),
    Ready(Box<StreamingWorld>, Rc<RefCell<GuidMap>>),
    Failed(String),
}

/// One reported entity in the mirror.
struct Actor {
    entity: Ecs,
    pos: Vec3,
    yaw: f32,
    class: EntityClass,
}

/// How many of what the game reports the mirror draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Drawn {
    pub hero: bool,
    pub vehicles: (usize, usize),
    pub npcs: (usize, usize),
}

/// The mirror.
pub struct Mirror {
    state: State,
    pub world: World,
    actors: HashMap<String, Actor>,
    /// Models that would not load, and why: their entities are reported and not drawn.
    failed: HashMap<u32, String>,
    /// The dynamic layers asked of the executor.
    dynamic: HashSet<u32>,
    statics_asked: bool,
    /// Template names, by the hash `Object.GetName` returns.
    pub names: Rc<HashMap<u32, String>>,
    /// The decoded map image, until the page uploads it.
    pub map: Option<(u32, u32, Vec<u8>)>,
    wad: String,
    /// The base terrain's model.
    terrain: Option<u32>,
    /// The resident units' collision triangles, by unit.
    collision: HashMap<u64, Vec<[Vec3; 3]>>,
}

impl Default for Mirror {
    fn default() -> Mirror {
        Mirror {
            state: State::Idle,
            world: World::new(),
            actors: HashMap::new(),
            failed: HashMap::new(),
            dynamic: HashSet::new(),
            statics_asked: false,
            names: Rc::new(HashMap::new()),
            map: None,
            wad: String::new(),
            terrain: None,
            collision: HashMap::new(),
        }
    }
}

/// `key`, when `guid` is a placed entity's: placed keys sit below the runtime range (`level_editor.md`).
fn placed_key(guid: &str) -> Option<u32> {
    let k = u32::from_str_radix(guid.strip_prefix("0x")?, 16).ok()?;
    (k < 0x4000_0000).then_some(k)
}

fn load(wad: String, progress: Arc<LoadProgress>) -> Result<Box<Loaded>, String> {
    use mercs2_core::streaming::StreamingConfig;
    let cfg = StreamingConfig { block_unload_margin: 200.0, block_budget: 2, ..StreamingConfig::default() };
    let mut assets = mercs2_engine::asset::AssetSource::discover(&wad, &[])?;
    let templates = mercs2_engine::worldutil::TemplateRecords::load(&mut assets)?;
    let data = load_streaming_world_data(&wad, cfg, templates, &progress)?;
    let map = data.terrain_texture().map(crate::texpng::decode_bc);
    Ok(Box::new(Loaded { data, map }))
}

impl Mirror {
    pub fn ready(&self) -> bool {
        matches!(self.state, State::Ready(..))
    }

    /// What the viewport should say while there is no world to draw.
    pub fn note(&self) -> Option<String> {
        match &self.state {
            State::Idle | State::Ready(..) => None,
            State::Loading(_, p) => Some(format!("loading the world · {:.0}%", p.fraction() * 100.0)),
            State::Failed(e) => Some(format!("the mirror could not load: {e}")),
        }
    }

    /// A model the mirror could not load, for the faults list.
    pub fn problems(&self) -> impl Iterator<Item = (&u32, &String)> {
        self.failed.iter()
    }

    /// Reloads from `wad` if the archive changed; the mirror is that archive's.
    pub fn start(&mut self, wad: &str) {
        if self.wad == wad && !matches!(self.state, State::Failed(_)) {
            return;
        }
        *self = Mirror { wad: wad.to_string(), ..Mirror::default() };
        let progress = Arc::new(LoadProgress::new(LOAD_STAGES));
        let (tx, rx) = std::sync::mpsc::channel();
        let (path, p) = (wad.to_string(), progress.clone());
        std::thread::Builder::new()
            .name("live-mirror".into())
            .spawn(move || {
                let _ = tx.send(load(path, p));
            })
            .expect("spawn the mirror loader");
        self.state = State::Loading(rx, progress);
    }

    /// The scene's lights and sky are the world's while the page is open.
    pub fn enter(&self, scene: &mut Scene) {
        if let State::Ready(s, _) = &self.state {
            s.present_lights(scene);
        }
    }

    /// The other pages' scene is lit as they left it.
    pub fn leave(&self, scene: &mut Scene) {
        scene.set_lights(Vec::new());
        scene.set_spot_lights(Vec::new());
        scene.clear_fog();
        scene.set_terrain_hidden(None);
    }

    fn finish(&mut self, loaded: Loaded, scene: &mut Scene) {
        let Loaded { data, map } = loaded;
        self.map = map;
        self.names = Rc::new(data.template_names().iter().map(|(_, n)| (pandemic_hash_m2(n), n.clone())).collect());
        let block_layers = data.static_block_layers().to_vec();
        let guids = Rc::new(RefCell::new(GuidMap::new()));
        let (terrain, stream) = data.into_streaming_world(guids.clone());
        self.terrain = Some(terrain.hash);
        scene.load_model(terrain.hash, &terrain.verts, &terrain.indices, &terrain.draws, &terrain.textures, &terrain.skin);
        self.world.spawn((
            Transform::IDENTITY,
            ModelRef { model: terrain.hash },
            AnimState::default(),
            SkinPalette { mats: scene.model_rest_palette(terrain.hash) },
        ));
        {
            let mut g = guids.borrow_mut();
            g.request_layer(LayerOp::Load, pandemic_hash_m2("vz_base"), false);
            for &l in &block_layers {
                g.request_layer(LayerOp::Load, l, false);
            }
        }
        self.state = State::Ready(Box::new(stream), guids);
        self.enter(scene);
    }

    /// One frame: take a finished load, ask for the layers the game has, stream around the hero, and
    /// put the reported entities where the game says they are.
    pub fn step(&mut self, scene: &mut Scene, w: &mut WadStack, live: &Live, hero: Option<Vec3>) {
        if let State::Loading(rx, _) = &self.state {
            match rx.try_recv() {
                Ok(Ok(l)) => self.finish(*l, scene),
                Ok(Err(e)) => self.state = State::Failed(e),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => self.state = State::Failed("the loader thread died".to_string()),
            }
        }
        let Some(viewer) = hero else { return };
        self.ask_layers(live);
        self.sync_actors(scene, w, live);
        let State::Ready(stream, _) = &mut self.state else { return };
        if let Err(e) = stream.step(scene, &mut self.world, viewer.to_array(), &|_| false) {
            self.state = State::Failed(e);
            return;
        }
        // The executor logs each unit's collision until someone takes it, so this is drained every
        // frame whether or not the overlay is showing.
        for op in stream.take_collision_ops() {
            match op {
                CollisionOp::Insert(k, tris) => {
                    self.collision.insert(k, tris);
                }
                CollisionOp::Remove(k) => {
                    self.collision.remove(&k);
                }
            }
        }
    }

    /// The edges of the collision around `at`, at most `max`: the overlay draws these.
    pub fn collision_edges(&self, at: Vec3, radius: f32, max: usize) -> Vec<[Vec3; 2]> {
        let mut out = Vec::new();
        for tri in self.collision.values().flatten() {
            if tri[0].distance_squared(at) > radius * radius {
                continue;
            }
            out.push([tri[0], tri[1]]);
            out.push([tri[1], tri[2]]);
            out.push([tri[2], tri[0]]);
            if out.len() >= max {
                break;
            }
        }
        out
    }

    /// Terrain on or off: the base mesh and every hi-res tile.
    pub fn show_terrain(&self, scene: &mut Scene, on: bool) {
        scene.set_terrain_hidden(if on { None } else { self.terrain });
    }

    /// Make the executor's layers the game's: `_tLoadedLayers` dynamic, the master script's list static.
    fn ask_layers(&mut self, live: &Live) {
        let State::Ready(_, guids) = &self.state else { return };
        let mut g = guids.borrow_mut();
        if !self.statics_asked && !live.statics.is_empty() {
            for l in &live.statics {
                g.request_layer(LayerOp::Load, pandemic_hash_m2(l), false);
            }
            self.statics_asked = true;
        }
        let Some(world) = &live.world else { return };
        let wanted: HashSet<u32> = world.layers.iter().map(|l| pandemic_hash_m2(l)).collect();
        for &l in wanted.difference(&self.dynamic) {
            g.request_layer(LayerOp::Load, l, true);
        }
        for &l in self.dynamic.difference(&wanted) {
            g.request_layer(LayerOp::Unload, l, true);
        }
        self.dynamic = wanted;
    }

    fn sync_actors(&mut self, scene: &mut Scene, w: &mut WadStack, live: &Live) {
        let mut budget = MODELS_PER_FRAME;
        // No bridge read gives a seat pose, so a rider is not drawn standing through the roof.
        let riding = live.poses.last().is_some_and(|(_, p)| p.vehicle.is_some());
        let shown = |e: &&Entity| !(riding && e.class == EntityClass::Hero);
        for e in live.entities.iter().filter(shown) {
            if let Some(a) = self.actors.get_mut(&e.guid) {
                a.pos = a.pos.lerp(Vec3::from(e.pos), ACTOR_EASE);
                a.yaw = e.yaw;
                if let Ok(mut t) = self.world.get::<&mut Transform>(a.entity) {
                    t.translation = a.pos;
                    t.rotation = Quat::from_rotation_y(a.yaw.to_radians());
                }
                continue;
            }
            if budget > 0 && self.spawn_actor(e, scene, w) {
                budget -= 1;
            }
        }
        let reported: HashSet<&str> = live.entities.iter().filter(shown).map(|e| e.guid.as_str()).collect();
        let gone: Vec<String> = self.actors.keys().filter(|g| !reported.contains(g.as_str())).cloned().collect();
        for g in gone {
            if let Some(a) = self.actors.remove(&g) {
                let _ = self.world.despawn(a.entity);
                scene.forget_entity(a.entity);
            }
        }
    }

    /// Bring one entity into the mirror; true when that cost a load. A placed entity's own copy steps
    /// aside, so the game's position is the only one drawn.
    fn spawn_actor(&mut self, e: &Entity, scene: &mut Scene, w: &mut WadStack) -> bool {
        if self.failed.contains_key(&e.model) {
            return false;
        }
        let loaded = !scene.has_model(e.model);
        if loaded {
            match load_model_data(w, e.model) {
                Ok(md) => scene.load_model(e.model, &md.verts, &md.indices, &md.draws, &md.textures, &md.skin),
                Err(why) => {
                    self.failed.insert(e.model, why);
                    return true;
                }
            }
        }
        if let (Some(k), State::Ready(stream, _)) = (placed_key(&e.guid), &mut self.state) {
            stream.adopt(k);
        }
        let pos = Vec3::from(e.pos);
        let entity = self.world.spawn((
            Transform { translation: pos, rotation: Quat::from_rotation_y(e.yaw.to_radians()), scale: Vec3::ONE },
            ModelRef { model: e.model },
            AnimState::default(),
            SkinPalette { mats: scene.model_rest_palette(e.model) },
        ));
        scene.set_entity_render_state(entity, default_render_state(w, e.model));
        self.actors.insert(e.guid.clone(), Actor { entity, pos, yaw: e.yaw, class: e.class });
        loaded
    }

    /// The contract's pose rows: of what is reported, how much the mirror draws.
    pub fn drawn(&self, live: &Live) -> Drawn {
        let mut d = Drawn::default();
        let riding = live.poses.last().is_some_and(|(_, p)| p.vehicle.is_some());
        for e in &live.entities {
            let there = self.actors.get(&e.guid).is_some_and(|a| a.class == e.class);
            match e.class {
                EntityClass::Hero => d.hero = there || riding,
                EntityClass::Human => {
                    d.npcs.1 += 1;
                    d.npcs.0 += usize::from(there);
                }
                _ => {
                    d.vehicles.1 += 1;
                    d.vehicles.0 += usize::from(there);
                }
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_keys_are_below_the_runtime_range() {
        assert_eq!(placed_key("0x000C9901"), Some(0xC9901));
        assert_eq!(placed_key("0x10001F5B"), Some(0x1000_1F5B));
        assert_eq!(placed_key("0x40005C68"), None);
        assert_eq!(placed_key("nonsense"), None);
    }

    #[test]
    fn a_new_mirror_reports_nothing_drawn() {
        let e = Entity {
            guid: "0x1".into(), class: EntityClass::Car, pos: [0.0; 3], yaw: 0.0, health: 1.0, max_health: 1.0, template: 0, model: 0, faction: 0, name_key: String::new(),
        };
        let live = Live { entities: vec![e], ..Live::default() };
        let d = Mirror::default().drawn(&live);
        assert_eq!(d.vehicles, (0, 1));
        assert!(!d.hero);
    }
}
