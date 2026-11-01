//! Inspect's LAYERS tab (`docs/modernization/workshop_pages/inspect_tabs.md`, Layers): every entity of
//! a placement layer in 3D, where the layer places it.
//!
//! [`LayerView::read`] resolves the layer's entities as the world's activation does
//! (`worldutil::PlacedCatalog`): a placed entity's model is its own `ModelName`, else its template's.
//! The template's `PhysicalLink` children come with it, each at its parent's node plus its own offset
//! ([`Part`]). An entity a placed `PhysicalLink` names as its child sits at that link, not at its own
//! `Transform` ([`EntityLink`]). An entity with nothing to draw is a labelled marker of its [`kind`].
//!
//! [`LayerView::step`] loads the models a few at a time, so a layer of thousands of entities fills in
//! while the window stays live. Each instance is drawn in its template's state: the template's
//! `OSMStateParameter` records, a kit's first listed outfit, rotors at rest.
//!
//! Riders and held equipment are listed nowhere here: their pose comes from the seat and the
//! character's animation, which this view does not run, so characters stand in their bind pose.

use crate::app::{load_model_data, node_enable_with_rotors, ModelData, WadStack};
use crate::gui::theme;
use crate::index::AssetIndex;
use glam::{Mat4, Quat, Vec3, Vec4};
use mercs2_core::{AnimState, Entity, ModelRef, SkinPalette, Transform, World};
use mercs2_engine::render_state::RenderState;
use mercs2_engine::scene::Scene;
use mercs2_engine::wad;
use mercs2_engine::worldutil::{PlacedCatalog, TemplateRecords};
use mercs2_formats::mixr::Mixr;
use mercs2_formats::orchestrator as orch;
use mercs2_formats::physical_link::{placed_links_in_order, placed_physical_links, template_physical_links, PhysicalLink};
use mercs2_formats::placement::{entity_classes, placed_names};
use mercs2_formats::worldentity::WorldEntity;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// How long one frame may spend loading models.
const STEP_BUDGET: Duration = Duration::from_millis(30);
/// Deeper `PhysicalLink` chains than this are a cycle in the data.
const MAX_LINK_DEPTH: usize = 8;
/// A marker's pick radius and drawn radius, in points.
const MARKER_RADIUS: f32 = 5.0;
const MARKER_PICK_RADIUS: f32 = 9.0;
/// Labels drawn at once, the nearest markers' first, one per cell of the view: past this they cover it.
const MAX_LABELS: usize = 120;
const LABEL_CELL: egui::Vec2 = egui::vec2(80.0, 16.0);
/// A model projecting smaller than this radius in points keeps a dot, so a layer framed whole still
/// shows where its models are.
const MODEL_DOT_BELOW: f32 = 4.0;
const MODEL_DOT_RADIUS: f32 = 2.5;
/// The renderer's vertical field of view and clip planes (`Scene::set_view` from the app's camera).
const FOV_Y: f32 = 45.0;
const NEAR: f32 = 0.05;
const FAR: f32 = 40000.0;

/// The kind of an entity that draws no model, by the first of these classes it carries (its own or its
/// template's). Listed so an entity with several reads as what it is for: a patrol's spawner carries
/// a `Path` too.
const MARKER_KINDS: [(&str, &str); 19] = [
    ("LandingZone", "Landing zone"),
    ("PopulationSimpleSpawner", "Spawner"),
    ("Squad", "Squad"),
    ("LightObject", "Light"),
    ("ScrubObject", "Scrub"),
    ("RedEffectComponent", "Effect"),
    ("EffectTemplate", "Effect"),
    ("SoundEffect", "Sound"),
    ("SoundAmbience", "Sound"),
    ("LineRegion", "Region"),
    ("RoadIntersection", "Road"),
    ("Road", "Road"),
    ("Path", "Path"),
    ("AiPatrol", "Path"),
    ("ObjectScript", "Script"),
    ("PointLocation", "Point"),
    ("ObjectHint", "Hint"),
    ("Rider", "Rider"),
    ("GenericLOD", "LOD"),
];
/// The kind of an entity drawing a model, and of one whose model no archive carries.
pub const KIND_MODEL: &str = "Model";
pub const KIND_TERRAIN: &str = "Terrain";
pub const KIND_MISSING: &str = "Missing model";
/// The kind of a marker carrying none of [`MARKER_KINDS`]: a bare named location.
pub const KIND_LOCATION: &str = "Location";

/// The marker kind of an entity with `classes`.
pub fn marker_kind(classes: &BTreeSet<&str>) -> &'static str {
    MARKER_KINDS.iter().find(|(c, _)| classes.contains(c)).map_or(KIND_LOCATION, |(_, k)| k)
}

/// What the layer view reads from the world's templates, built once per archive.
pub struct Templates {
    world: TemplateRecords,
    classes: HashMap<u32, BTreeSet<String>>,
    links: BTreeMap<u32, Vec<PhysicalLink>>,
    names: HashMap<u32, String>,
    profiles: HashMap<u32, u32>,
    we: WorldEntity,
}

impl Templates {
    pub fn read(w: &mut WadStack) -> Result<Templates, String> {
        let we = WorldEntity::parse(&w.worldentity()?)?;
        let world = TemplateRecords::from_worldentity(&we)?;
        let mut classes: HashMap<u32, BTreeSet<String>> = HashMap::new();
        for c in &we.components {
            for r in &c.records {
                for &k in &r.keys {
                    classes.entry(k).or_default().insert(c.class.clone());
                }
            }
        }
        let names = world.names.iter().cloned().collect();
        let links = template_physical_links(&we)?;
        let profiles = mercs2_formats::mixr::kit_templates(&we)?.iter().filter(|t| t.profile != 0).map(|t| (t.key, t.profile)).collect();
        Ok(Templates { world, classes, links, names, profiles, we })
    }
}

/// One model an entity draws: the entity's own, or a `PhysicalLink` child's. `model` is `None` for a
/// link child with no model of its own that others may hang from.
#[derive(Clone, Debug)]
pub struct Part {
    pub template: u32,
    pub model: Option<u32>,
    /// The part this one hangs from, `None` for the root; and the node of that part's model.
    pub parent: Option<usize>,
    pub hard_point: u32,
    /// The link's offset from that node.
    pub offset: Mat4,
    world: Option<Mat4>,
    spawned: bool,
    /// Why the part does not draw.
    pub problem: Option<String>,
}

/// The placed `PhysicalLink` an entity hangs from. Its `Transform` then says nothing about where it
/// is: `layer --frames` finds every one of them at the link in four captures, and at its `Transform`
/// only by chance. When two records name one child the last wins.
#[derive(Clone, Debug)]
pub struct EntityLink {
    /// The parent's index in [`LayerView::entities`].
    pub parent: usize,
    /// A node of the parent's model, which counts once that model is there; 0 for the entity itself.
    pub hard_point: u32,
    pub offset: Mat4,
}

/// One entity of the layer.
#[derive(Clone, Debug)]
pub struct LayerEntity {
    pub key: u32,
    pub name: Option<String>,
    /// `Transform.Parent`, 0 for none, and its `Name`.
    pub parent: u32,
    pub template: Option<String>,
    /// Where it sits: its `Transform`, or for a linked entity where the link puts it.
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    /// The position its `Transform` stores.
    pub stored_pos: [f32; 3],
    pub link: Option<EntityLink>,
    pub kind: &'static str,
    pub parts: Vec<Part>,
    /// The `terrainmesh` of a terrain tile; its vertices are in world space.
    pub terrain: Option<u32>,
    /// World-space centre and radius of what it draws, once loaded; a marker has none.
    bounds: Option<(Vec3, f32)>,
}

impl LayerEntity {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("0x{:08X}", self.key))
    }

    fn matrix(&self) -> Mat4 {
        Mat4::from_rotation_translation(Quat::from_array(self.quat), Vec3::from(self.pos))
    }

    /// Whether the viewport shows it as a marker.
    pub fn is_marker(&self) -> bool {
        self.terrain.is_none() && !self.parts.iter().any(|p| p.spawned)
    }
}

/// Where a linked chain puts `entities[i]` before any model has loaded: parent × offset, as retail
/// does while the parent sleeps.
fn resting_world(entities: &[LayerEntity], i: usize, depth: usize) -> Result<Mat4, String> {
    let e = &entities[i];
    let Some(link) = &e.link else { return Ok(e.matrix()) };
    if depth > MAX_LINK_DEPTH {
        return Err(format!("a PhysicalLink chain deeper than {MAX_LINK_DEPTH}, at entity 0x{:08X}", e.key));
    }
    Ok(resting_world(entities, link.parent, depth + 1)? * link.offset)
}

/// Add the children of `links` under part `parent`, then theirs. A child that is an entity of the
/// layer is left out: it is an entity with its [`EntityLink`] (891 of the 901 placed links in retail).
fn walk_links(t: &Templates, links: &[PhysicalLink], placed: &impl Fn(u32) -> bool, parent: usize, depth: usize, parts: &mut Vec<Part>) -> Result<(), String> {
    if depth > MAX_LINK_DEPTH {
        return Err(format!("a PhysicalLink chain deeper than {MAX_LINK_DEPTH}, at child 0x{:08X}", links[0].child));
    }
    for l in links.iter().filter(|l| !placed(l.child)) {
        let i = parts.len();
        parts.push(Part {
            template: l.child,
            model: t.world.models.get(&l.child).copied().filter(|&m| m != 0),
            parent: Some(parent),
            hard_point: l.hard_point,
            offset: Mat4::from_rotation_translation(Quat::from_array(l.quat), Vec3::from(l.pos)),
            world: None,
            spawned: false,
            problem: None,
        });
        if let Some(below) = t.links.get(&l.child) {
            walk_links(t, below, placed, i, depth + 1, parts)?;
        }
    }
    Ok(())
}

/// A model of the layer once loaded, for its instances.
struct LoadedModel {
    /// Every HIER node's world transform at rest, by name hash.
    nodes: HashMap<u32, Mat4>,
    /// The authored AABB's centre (model space) and half diagonal.
    bounds: (Vec3, f32),
    machine: Option<orch::StateMachine>,
    hier: Vec<orch::HierNode>,
    mixr: Option<Mixr>,
}

/// World transforms at rest of a model's HIER nodes, by name hash. `local` is row-major in the
/// row-vector convention, which is `glam`'s column layout.
fn node_worlds(hier: &[orch::HierNode]) -> HashMap<u32, Mat4> {
    let mut worlds: Vec<Mat4> = Vec::with_capacity(hier.len());
    let mut out = HashMap::with_capacity(hier.len());
    for n in hier {
        let local = Mat4::from_cols_array(&n.local);
        let world = n.parent.and_then(|p| worlds.get(p).copied()).map_or(local, |p| p * local);
        worlds.push(world);
        out.insert(n.hash, world);
    }
    out
}

/// A job of the layer's loader.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Job {
    Model(u32),
    Terrain(u32, [f32; 3]),
}

/// One action of the layer's inspector.
pub enum LayerAct {
    /// Select this entity and frame it.
    Select(usize),
}

/// The layer open in the viewport.
pub struct LayerView {
    pub hash: u32,
    pub label: String,
    pub block: u16,
    pub entities: Vec<LayerEntity>,
    /// World-space bounds of the entities' positions.
    pub min: Vec3,
    pub max: Vec3,
    /// Entities per kind, and how many entities use each model.
    pub kinds: BTreeMap<&'static str, usize>,
    pub models: BTreeMap<u32, usize>,
    pub selected: Option<usize>,
    /// Scroll the entity table to the selection on the next draw.
    pub follow: bool,
    /// The entity the camera was last framed on, and whether it has moved since.
    framed: Option<usize>,
    reframe: bool,
    /// Models no archive carries, and what did not load, with why.
    pub missing: BTreeSet<u32>,
    pub errors: Vec<String>,
    pending: VecDeque<Job>,
    jobs: usize,
    loaded: HashMap<u32, LoadedModel>,
    /// What the scene loaded for this view, and every entity it spawned.
    owned: Vec<u32>,
    spawned: Vec<Entity>,
    states: HashMap<(u32, u32), RenderState>,
}

impl LayerView {
    /// Read the layer whose container is `block_data`.
    pub fn read(hash: u32, label: String, block: u16, block_data: &[u8], t: &Templates) -> Result<LayerView, String> {
        let mut catalog = PlacedCatalog::default();
        let keys = catalog.add_block(block_data, &t.world)?;
        let own = entity_classes(block_data)?;
        let own_links = placed_physical_links(block_data)?;
        let placed: BTreeSet<u32> = keys.iter().copied().collect();
        let in_layer = |k: u32| placed.contains(&k);
        let names: HashMap<u32, String> = placed_names(block_data).into_iter().collect();
        let mut entities = Vec::with_capacity(keys.len());
        let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        let mut kinds: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut models: BTreeMap<u32, usize> = BTreeMap::new();
        let mut pending: Vec<Job> = Vec::new();
        for k in keys {
            let o = catalog.get(k).expect("add_block returned the key");
            let mut parts = vec![Part {
                template: o.parent,
                model: o.model,
                parent: None,
                hard_point: 0,
                offset: Mat4::IDENTITY,
                world: None,
                spawned: false,
                problem: None,
            }];
            // A component is looked up on the entity, then its template (`FUN_00649a80`).
            if let Some(links) = own_links.get(&k).or_else(|| t.links.get(&o.parent)) {
                walk_links(t, links, &in_layer, 0, 0, &mut parts)?;
            }
            let kind = if o.terrain_mesh.is_some() {
                KIND_TERRAIN
            } else if parts.iter().any(|p| p.model.is_some()) {
                KIND_MODEL
            } else {
                let mut classes: BTreeSet<&str> = BTreeSet::new();
                classes.extend(own.get(&k).into_iter().flatten().map(String::as_str));
                classes.extend(t.classes.get(&o.parent).into_iter().flatten().map(String::as_str));
                marker_kind(&classes)
            };
            *kinds.entry(kind).or_default() += 1;
            for m in parts.iter().filter_map(|p| p.model) {
                let n = models.entry(m).or_default();
                if *n == 0 {
                    pending.push(Job::Model(m));
                }
                *n += 1;
            }
            if let Some(tm) = o.terrain_mesh {
                pending.push(Job::Terrain(tm, o.pos));
            }
            entities.push(LayerEntity {
                key: k,
                name: names.get(&k).cloned(),
                parent: o.parent,
                template: t.names.get(&o.parent).cloned(),
                pos: o.pos,
                quat: o.quat,
                stored_pos: o.pos,
                link: None,
                kind,
                parts,
                terrain: o.terrain_mesh,
                bounds: None,
            });
        }
        let index_of: HashMap<u32, usize> = entities.iter().enumerate().map(|(i, e)| (e.key, i)).collect();
        for (parent, l) in placed_links_in_order(block_data)? {
            let (Some(&child), Some(&parent)) = (index_of.get(&l.child), index_of.get(&parent)) else { continue };
            let offset = Mat4::from_rotation_translation(Quat::from_array(l.quat), Vec3::from(l.pos));
            entities[child].link = Some(EntityLink { parent, hard_point: l.hard_point, offset });
        }
        for i in 0..entities.len() {
            if entities[i].link.is_some() {
                let (_, rotation, translation) = resting_world(&entities, i, 0)?.to_scale_rotation_translation();
                entities[i].pos = translation.to_array();
                entities[i].quat = rotation.to_array();
            }
            let p = Vec3::from(entities[i].pos);
            min = min.min(p);
            max = max.max(p);
        }
        if entities.is_empty() {
            (min, max) = (Vec3::ZERO, Vec3::ZERO);
        }
        Ok(LayerView {
            hash,
            label,
            block,
            entities,
            min,
            max,
            kinds,
            models,
            selected: None,
            follow: false,
            framed: None,
            reframe: false,
            missing: BTreeSet::new(),
            errors: Vec::new(),
            jobs: pending.len(),
            pending: pending.into(),
            loaded: HashMap::new(),
            owned: Vec::new(),
            spawned: Vec::new(),
            states: HashMap::new(),
        })
    }

    /// The camera that frames the whole layer: target and distance.
    pub fn framing(&self) -> (Vec3, f32) {
        let radius = ((self.max - self.min).length() * 0.5).max(5.0);
        ((self.min + self.max) * 0.5, (radius * 2.4).clamp(5.0, 15000.0))
    }

    /// The camera that frames entity `i`.
    pub fn entity_framing(&self, i: usize) -> Option<(Vec3, f32)> {
        let e = self.entities.get(i)?;
        let (c, r) = e.bounds.unwrap_or((Vec3::from(e.pos), 2.0));
        Some((c, (r * 3.5).clamp(5.0, 15000.0)))
    }

    /// Jobs done of all jobs.
    pub fn progress(&self) -> (usize, usize) {
        (self.jobs - self.pending.len(), self.jobs)
    }

    pub fn is_loading(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Load what fits in this frame and spawn what became ready.
    pub fn step(&mut self, w: &mut WadStack, scene: &mut Scene, world: &mut World, t: &Templates, index: &AssetIndex) {
        if self.pending.is_empty() {
            return;
        }
        let start = Instant::now();
        while start.elapsed() < STEP_BUDGET {
            let Some(job) = self.pending.pop_front() else { break };
            match job {
                Job::Model(m) => self.load_model(m, w, scene),
                Job::Terrain(tm, pos) => self.spawn_terrain(tm, pos, w, scene, world),
            }
        }
        self.spawn_ready(scene, world, t, index);
    }

    fn load_model(&mut self, m: u32, w: &mut WadStack, scene: &mut Scene) {
        let shipped = w.wads.iter().any(|x| wad::aset_types(x, m).iter().any(|(ty, _, _)| *ty == wad::MODEL_ASET_TYPE_ID));
        if !shipped {
            self.missing.insert(m);
            return;
        }
        let read = (|| -> Result<(ModelData, Option<Mixr>), String> {
            let md = load_model_data(w, m)?;
            let mixr = Mixr::find(&w.extract_container(m)?)?;
            Ok((md, mixr))
        })();
        let (md, mixr) = match read {
            Ok(r) => r,
            Err(e) => {
                self.errors.push(format!("model 0x{m:08X}: {e}"));
                return;
            }
        };
        if !scene.has_model(m) {
            scene.load_model(m, &md.verts, &md.indices, &md.draws, &md.textures, &md.skin);
            self.owned.push(m);
        }
        let (lo, hi) = match &md.header {
            Some(h) => (Vec3::from(h.aabb_min), Vec3::from(h.aabb_max)),
            None => (Vec3::from(md.stats.bbox_min), Vec3::from(md.stats.bbox_max)),
        };
        self.loaded.insert(
            m,
            LoadedModel {
                nodes: node_worlds(&md.hier_nodes),
                bounds: ((lo + hi) * 0.5, ((hi - lo).length() * 0.5).max(0.25)),
                machine: md.machine,
                hier: md.hier_nodes,
                mixr,
            },
        );
    }

    fn spawn_terrain(&mut self, tm: u32, pos: [f32; 3], w: &mut WadStack, scene: &mut Scene, world: &mut World) {
        let m = w.wads.iter_mut().rev().find_map(|x| mercs2_engine::game_world::load_terrainmesh_tile(x, tm, pos));
        let Some(m) = m else {
            self.errors.push(format!("terrainmesh 0x{tm:08X}: did not load"));
            return;
        };
        let radius = m.verts.iter().map(|v| (Vec3::from(v.pos) - Vec3::from(pos)).length()).fold(1.0, f32::max);
        if !scene.has_model(m.hash) {
            scene.load_terrain_model(m.hash, &m.verts, &m.indices, &m.draws, &m.textures, &m.skin);
            self.owned.push(m.hash);
        }
        let mats = scene.model_rest_palette(m.hash);
        self.spawned.push(world.spawn((Transform::IDENTITY, ModelRef { model: m.hash }, AnimState::default(), SkinPalette { mats })));
        if let Some(e) = self.entities.iter_mut().find(|e| e.terrain == Some(tm) && e.bounds.is_none()) {
            e.bounds = Some((Vec3::from(pos), radius));
        }
    }

    /// The render state of `model` on an entity of `template`.
    fn state(&mut self, model: u32, template: u32, t: &Templates, index: &AssetIndex) -> Result<RenderState, String> {
        if let Some(s) = self.states.get(&(model, template)) {
            return Ok(s.clone());
        }
        let m = &self.loaded[&model];
        let states = orch::template_states(&t.we, &[template])?;
        let chosen: Vec<usize> = match (&m.machine, &m.mixr, t.profiles.get(&template)) {
            (Some(sm), Some(mixr), Some(&profile)) => match mixr.profile_index(profile).and_then(|i| mixr.outfits(i).first().copied()) {
                Some(outfit) => mixr.outfit_node_states(sm, &m.hier, outfit)?,
                None => sm.nodes.iter().map(|n| orch::default_state_index_in(n, &states)).collect(),
            },
            (Some(sm), _, _) => sm.nodes.iter().map(|n| orch::default_state_index_in(n, &states)).collect(),
            (None, _, _) => Vec::new(),
        };
        let node_hashes: Vec<u32> = m.hier.iter().map(|n| n.hash).collect();
        let mut rotors: Vec<_> = mercs2_formats::rotor::model_rotors_in(&index.world_templates, &index.rotor_records, model, &node_hashes)?
            .iter()
            .map(mercs2_engine::vehicle::rotor::RotorController::new)
            .collect();
        let (node_enable, _) = node_enable_with_rotors(m.machine.as_ref(), &m.hier, &chosen, &mut rotors, 0.0, &states)?;
        let rs = RenderState { lod: 0, view_state: 0x01, node_enable };
        self.states.insert((model, template), rs.clone());
        Ok(rs)
    }

    /// Spawn every part whose model and parent are ready.
    fn spawn_ready(&mut self, scene: &mut Scene, world: &mut World, t: &Templates, index: &AssetIndex) {
        for ei in 0..self.entities.len() {
            let root = match self.entity_world(ei, 0) {
                None => continue,
                Some(Ok(root)) => root,
                Some(Err(e)) => {
                    self.entities[ei].parts[0].problem = Some(e);
                    continue;
                }
            };
            if self.entities[ei].link.is_some() {
                self.place_linked(ei, root);
            }
            let mut changed = false;
            for pi in 0..self.entities[ei].parts.len() {
                if self.entities[ei].parts[pi].spawned || self.entities[ei].parts[pi].problem.is_some() {
                    continue;
                }
                let at = match self.entities[ei].parts[pi].world {
                    Some(at) => at,
                    None => match self.part_world(ei, pi, root) {
                        None => continue,
                        Some(Ok(at)) => at,
                        Some(Err(e)) => {
                            self.entities[ei].parts[pi].problem = Some(e);
                            continue;
                        }
                    },
                };
                self.entities[ei].parts[pi].world = Some(at);
                let (model, template) = (self.entities[ei].parts[pi].model, self.entities[ei].parts[pi].template);
                let Some(model) = model else { continue };
                if self.missing.contains(&model) {
                    self.entities[ei].parts[pi].problem = Some(format!("model 0x{model:08X} is in no archive"));
                    continue;
                }
                if !self.loaded.contains_key(&model) {
                    continue;
                }
                let rs = match self.state(model, template, t, index) {
                    Ok(rs) => rs,
                    Err(e) => {
                        self.errors.push(format!("0x{:08X}: {e}", self.entities[ei].key));
                        self.entities[ei].parts[pi].problem = Some(e);
                        continue;
                    }
                };
                let (scale, rotation, translation) = at.to_scale_rotation_translation();
                let tf = Transform { translation, rotation, scale };
                let e = world.spawn((tf, ModelRef { model }, AnimState::default(), SkinPalette { mats: scene.model_rest_palette(model) }));
                scene.set_entity_render_state(e, rs);
                self.spawned.push(e);
                self.entities[ei].parts[pi].spawned = true;
                changed = true;
            }
            if changed {
                self.update_bounds(ei);
            }
        }
        for e in &mut self.entities {
            if e.parts.iter().any(|p| p.problem.as_deref().is_some_and(|m| m.ends_with("in no archive"))) && !e.parts.iter().any(|p| p.spawned) {
                e.kind = KIND_MISSING;
            }
        }
        self.kinds.clear();
        for e in &self.entities {
            *self.kinds.entry(e.kind).or_default() += 1;
        }
    }

    /// Move linked entity `ei` to `at`. The entity the camera was framed on asks to be framed again:
    /// its hard point can put it out of the shot.
    fn place_linked(&mut self, ei: usize, at: Mat4) {
        let (_, rotation, translation) = at.to_scale_rotation_translation();
        let e = &mut self.entities[ei];
        let moved = Vec3::from(e.pos).distance(translation) > 0.01;
        e.pos = translation.to_array();
        e.quat = rotation.to_array();
        self.reframe |= moved && self.framed == Some(ei);
    }

    /// Frame entity `i`, now and again if it moves while its parent's model loads.
    pub fn frame_entity(&mut self, i: usize) -> Option<(Vec3, f32)> {
        self.framed = Some(i);
        self.entity_framing(i)
    }

    /// The new framing of the entity asked for by [`Self::frame_entity`], once, when it has moved.
    pub fn take_reframe(&mut self) -> Option<(Vec3, f32)> {
        if !std::mem::take(&mut self.reframe) {
            return None;
        }
        self.framed.and_then(|i| self.entity_framing(i))
    }

    /// Where entity `ei` sits with its parents awake: `None` until their models have loaded, an error
    /// when a hard point cannot be found. A parent whose model no archive carries never wakes, so its
    /// child stays at parent × offset, as retail leaves the child of any sleeping parent.
    fn entity_world(&self, ei: usize, depth: usize) -> Option<Result<Mat4, String>> {
        let e = &self.entities[ei];
        let Some(link) = &e.link else { return Some(Ok(e.matrix())) };
        if depth > MAX_LINK_DEPTH {
            return Some(Err(format!("a PhysicalLink chain deeper than {MAX_LINK_DEPTH}, at entity 0x{:08X}", e.key)));
        }
        let parent = match self.entity_world(link.parent, depth + 1)? {
            Ok(parent) => parent,
            Err(e) => return Some(Err(e)),
        };
        if link.hard_point == 0 {
            return Some(Ok(parent * link.offset));
        }
        let Some(model) = self.entities[link.parent].parts[0].model else {
            return Some(Err(format!("hard point 0x{:08X} on a parent with no model", link.hard_point)));
        };
        if self.missing.contains(&model) {
            return Some(Ok(parent * link.offset));
        }
        let node = self.loaded.get(&model)?.nodes.get(&link.hard_point);
        Some(match node {
            Some(node) => Ok(parent * *node * link.offset),
            None => Err(format!("model 0x{model:08X} has no node 0x{:08X}", link.hard_point)),
        })
    }

    /// Where part `pi` of entity `ei` sits: `None` until what it depends on has loaded, an error when
    /// its hard point cannot be found.
    fn part_world(&self, ei: usize, pi: usize, root: Mat4) -> Option<Result<Mat4, String>> {
        let e = &self.entities[ei];
        let p = &e.parts[pi];
        let Some(parent) = p.parent else { return Some(Ok(root)) };
        let up = &e.parts[parent];
        if let Some(why) = &up.problem {
            return Some(Err(format!("its parent does not draw: {why}")));
        }
        let at = up.world?;
        if p.hard_point == 0 {
            return Some(Ok(at * p.offset));
        }
        let Some(model) = up.model else {
            return Some(Err(format!("hard point 0x{:08X} on a parent with no model", p.hard_point)));
        };
        let node = self.loaded.get(&model)?.nodes.get(&p.hard_point);
        Some(match node {
            Some(node) => Ok(at * *node * p.offset),
            None => Err(format!("model 0x{model:08X} has no node 0x{:08X}", p.hard_point)),
        })
    }

    fn update_bounds(&mut self, ei: usize) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for p in self.entities[ei].parts.iter().filter(|p| p.spawned) {
            let (Some(at), Some(m)) = (p.world, p.model.and_then(|m| self.loaded.get(&m))) else { continue };
            let (c, r) = m.bounds;
            let c = at.transform_point3(c);
            lo = lo.min(c - Vec3::splat(r));
            hi = hi.max(c + Vec3::splat(r));
        }
        self.entities[ei].bounds = Some(((lo + hi) * 0.5, (hi - lo).length() * 0.5 / 3f32.sqrt()));
    }

    /// Everything the view put in the scene comes out, but for `keep`, a model something else draws.
    pub fn teardown(self, scene: &mut Scene, world: &mut World, keep: &[u32]) {
        for e in self.spawned {
            world.despawn(e).ok();
            scene.forget_entity(e);
        }
        for m in self.owned {
            if !keep.contains(&m) {
                scene.unload_model(m);
            }
        }
    }
}

/// The renderer's view-projection for the app's orbit camera (`Scene::render_with`: `perspective_lh`
/// with clip X mirrored).
pub fn view_proj(eye: Vec3, target: Vec3, aspect: f32) -> Mat4 {
    Mat4::from_scale(Vec3::new(-1.0, 1.0, 1.0)) * Mat4::perspective_lh(FOV_Y.to_radians(), aspect, NEAR, FAR) * Mat4::look_at_lh(eye, target, Vec3::Y)
}

/// `p` on a screen `size` points big, with its clip depth; `None` behind the camera.
fn project(vp: Mat4, p: Vec3, size: egui::Vec2) -> Option<(egui::Pos2, f32)> {
    let c: Vec4 = vp * p.extend(1.0);
    (c.w > NEAR).then(|| {
        let (x, y) = (c.x / c.w, c.y / c.w);
        (egui::pos2((x + 1.0) * 0.5 * size.x, (1.0 - y) * 0.5 * size.y), c.w)
    })
}

/// The entity under `cursor`: what the view draws there, nearest first. A drawn entity answers
/// within its bounding sphere as projected, a marker within [`MARKER_PICK_RADIUS`].
pub fn pick(entities: &[LayerEntity], vp: Mat4, size: egui::Vec2, cursor: egui::Pos2) -> Option<usize> {
    let px_per_unit = size.y * 0.5 / (FOV_Y.to_radians() * 0.5).tan();
    let mut best: Option<(usize, f32)> = None;
    for (i, e) in entities.iter().enumerate() {
        let (centre, radius) = e.bounds.unwrap_or((Vec3::from(e.pos), 0.0));
        let Some((at, depth)) = project(vp, centre, size) else { continue };
        let reach = if e.bounds.is_some() { radius * px_per_unit / depth } else { MARKER_PICK_RADIUS };
        if at.distance(cursor) <= reach && best.is_none_or(|(_, d)| depth < d) {
            best = Some((i, depth));
        }
    }
    best.map(|(i, _)| i)
}

/// The markers, their labels and the selection ring, over the viewport `rect`.
pub fn overlay(ctx: &egui::Context, view: &LayerView, vp: Mat4, rect: egui::Rect) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("layer_markers"))).with_clip_rect(rect);
    let size = ctx.screen_rect().size();
    let font = theme::font(&egui::TextStyle::Small);
    let px_per_unit = size.y * 0.5 / (FOV_Y.to_radians() * 0.5).tan();
    let mut labels: Vec<(f32, egui::Pos2, &str)> = Vec::new();
    for e in &view.entities {
        let Some((at, depth)) = project(vp, e.bounds.map_or(Vec3::from(e.pos), |b| b.0), size) else { continue };
        if !rect.contains(at) {
            continue;
        }
        if e.is_marker() {
            let col = if e.kind == KIND_MISSING { theme::BAD } else { theme::DIM };
            painter.circle_filled(at, MARKER_RADIUS, col);
            labels.push((depth, at, e.kind));
        } else if e.bounds.is_some_and(|(_, r)| r * px_per_unit / depth < MODEL_DOT_BELOW) {
            painter.circle_filled(at, MODEL_DOT_RADIUS, theme::BRASS);
        }
    }
    labels.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut taken: HashSet<(i32, i32)> = HashSet::new();
    let mut drawn = 0;
    for (_, at, text) in labels {
        if drawn == MAX_LABELS {
            break;
        }
        if taken.insert(((at.x / LABEL_CELL.x) as i32, (at.y / LABEL_CELL.y) as i32)) {
            painter.text(at + egui::vec2(MARKER_RADIUS + 3.0, 0.0), egui::Align2::LEFT_CENTER, text, font.clone(), theme::TX);
            drawn += 1;
        }
    }
    if let Some(e) = view.selected.and_then(|i| view.entities.get(i)) {
        let (centre, radius) = e.bounds.unwrap_or((Vec3::from(e.pos), 0.0));
        if let Some((at, depth)) = project(vp, centre, size) {
            let r = (radius * px_per_unit / depth).max(MARKER_RADIUS + 4.0);
            painter.circle_stroke(at, r, egui::Stroke::new(1.5, theme::BRASS));
        }
    }
}

/// The inspector of the open layer.
pub fn inspector(ui: &mut egui::Ui, view: &mut LayerView, index: &AssetIndex) -> Vec<LayerAct> {
    let mut acts = Vec::new();
    let name = |h: u32| index.names.get(&h).cloned().unwrap_or_else(|| format!("0x{h:08X}"));
    theme::card(ui, "Layer", Some(&format!("0x{:08X}", view.hash)), |ui| {
        ui.add(egui::Label::new(egui::RichText::new(&view.label).color(theme::TX)).truncate());
        theme::kv(ui, "block", egui::RichText::new(view.block.to_string()));
        theme::kv(ui, "entities", egui::RichText::new(view.entities.len().to_string()));
        let linked = view.entities.iter().filter(|e| e.link.is_some()).count();
        if linked > 0 {
            theme::kv(ui, "linked", egui::RichText::new(linked.to_string()));
        }
        let (done, all) = view.progress();
        if done < all {
            theme::kv(ui, "loading", egui::RichText::new(format!("{done} / {all}")));
        }
        ui.add_space(4.0);
        theme::eyebrow(ui, "By kind");
        for (kind, n) in &view.kinds {
            theme::kv(ui, kind, egui::RichText::new(n.to_string()));
        }
        if !view.errors.is_empty() {
            ui.add_space(4.0);
            for e in &view.errors {
                ui.add(egui::Label::new(egui::RichText::new(e).small().color(theme::BAD)).truncate());
            }
        }
    });
    if !view.models.is_empty() {
        theme::card(ui, "Models", Some(&view.models.len().to_string()), |ui| {
            egui::ScrollArea::vertical().id_source("layer_models").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                let mut rows: Vec<(u32, usize)> = view.models.iter().map(|(m, n)| (*m, *n)).collect();
                rows.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
                for (m, n) in rows {
                    let missing = view.missing.contains(&m);
                    let col = if missing { theme::BAD } else { theme::DIM };
                    ui.add(egui::Label::new(egui::RichText::new(format!("{n} \u{d7} {}", name(m))).small().color(col)).truncate());
                }
            });
        });
    }
    if let Some(e) = view.selected.and_then(|i| view.entities.get(i)) {
        theme::card(ui, "Entity", Some(&format!("0x{:08X}", e.key)), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(e.label()).color(theme::TX)).truncate());
            theme::kv(ui, "kind", egui::RichText::new(e.kind));
            let template = e.template.clone().unwrap_or_else(|| if e.parent == 0 { "none".into() } else { format!("0x{:08X}", e.parent) });
            theme::kv(ui, "template", egui::RichText::new(template));
            theme::kv(ui, "position", egui::RichText::new(format!("{:.1}, {:.1}, {:.1}", e.pos[0], e.pos[1], e.pos[2])));
            match &e.link {
                None => theme::kv(ui, "frame", egui::RichText::new("world")),
                Some(link) => {
                    theme::kv(ui, "frame", egui::RichText::new("linked"));
                    theme::kv(ui, "linked to", egui::RichText::new(view.entities[link.parent].label()));
                    if link.hard_point != 0 {
                        theme::kv(ui, "hard point", egui::RichText::new(name(link.hard_point)));
                    }
                    theme::kv(ui, "stored", egui::RichText::new(format!("{:.1}, {:.1}, {:.1}", e.stored_pos[0], e.stored_pos[1], e.stored_pos[2])));
                }
            }
            for p in &e.parts {
                if let Some(m) = p.model {
                    let col = if p.problem.is_some() { theme::BAD } else { theme::DIM };
                    ui.add(egui::Label::new(egui::RichText::new(name(m)).small().color(col)).truncate());
                }
                if let Some(why) = &p.problem {
                    ui.add(egui::Label::new(egui::RichText::new(why).small().color(theme::BAD)).truncate());
                }
            }
        });
    }
    let selected = view.selected;
    let follow = std::mem::take(&mut view.follow);
    theme::card(ui, "Entities", Some(&view.entities.len().to_string()), |ui| {
        const ROW: f32 = 20.0;
        let mut area = egui::ScrollArea::vertical().id_source("layer_entities").max_height(320.0).auto_shrink([false, true]);
        if let (true, Some(i)) = (follow, selected) {
            area = area.vertical_scroll_offset(i as f32 * ROW);
        }
        area.show_rows(ui, ROW, view.entities.len(), |ui, rows| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in rows {
                let e = &view.entities[i];
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW), egui::Sense::click());
                if selected == Some(i) {
                    ui.painter().rect_filled(rect, 2.0, theme::BRASS.gamma_multiply(0.18));
                } else if resp.hovered() {
                    ui.painter().rect_filled(rect, 2.0, theme::LINE);
                }
                let font = theme::font(&egui::TextStyle::Small);
                let kind_w = 84.0;
                let left = egui::Rect::from_min_max(rect.min, egui::pos2((rect.max.x - kind_w).max(rect.min.x), rect.max.y));
                ui.painter().with_clip_rect(left.intersect(ui.clip_rect())).text(
                    left.left_center() + egui::vec2(4.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    e.label(),
                    font.clone(),
                    theme::TX,
                );
                ui.painter().text(rect.right_center() - egui::vec2(4.0, 0.0), egui::Align2::RIGHT_CENTER, e.kind, font, theme::DIM);
                let hover = format!(
                    "{}\n{} \u{b7} {}\n{:.1}, {:.1}, {:.1}",
                    e.label(),
                    e.kind,
                    e.template.as_deref().unwrap_or("no template"),
                    e.pos[0],
                    e.pos[1],
                    e.pos[2]
                );
                if resp.on_hover_text(hover).clicked() {
                    acts.push(LayerAct::Select(i));
                }
            }
        });
    });
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(key: u32, pos: [f32; 3], bounds: Option<(Vec3, f32)>) -> LayerEntity {
        LayerEntity {
            key,
            name: None,
            parent: 0,
            template: None,
            pos,
            quat: [0.0, 0.0, 0.0, 1.0],
            stored_pos: pos,
            link: None,
            kind: KIND_LOCATION,
            parts: Vec::new(),
            terrain: None,
            bounds,
        }
    }

    /// A chain of links resolves from its root, whatever the children's own `Transform`s say.
    #[test]
    fn a_linked_chain_resolves_from_its_root_before_any_model() {
        let mut root = entity(1, [10.0, 0.0, 0.0], None);
        root.quat = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2).to_array();
        let mut mid = entity(2, [0.0, 99.0, 0.0], None);
        mid.link = Some(EntityLink { parent: 0, hard_point: 0, offset: Mat4::from_translation(Vec3::new(0.0, 0.0, 1.0)) });
        let mut tip = entity(3, [0.0, 0.0, 0.0], None);
        tip.link = Some(EntityLink { parent: 1, hard_point: 0x125C_0227, offset: Mat4::from_translation(Vec3::new(0.0, -2.0, 0.0)) });
        let list = [root, mid, tip];
        let (_, _, at) = resting_world(&list, 2, 0).unwrap().to_scale_rotation_translation();
        assert!((at - Vec3::new(11.0, -2.0, 0.0)).length() < 1e-5, "{at}");
    }

    /// A cycle of links fails by name instead of recursing for ever.
    #[test]
    fn a_cycle_of_links_is_an_error() {
        let link = |parent| Some(EntityLink { parent, hard_point: 0, offset: Mat4::IDENTITY });
        let mut a = entity(1, [0.0; 3], None);
        a.link = link(1);
        let mut b = entity(2, [0.0; 3], None);
        b.link = link(0);
        assert!(resting_world(&[a, b], 0, 0).unwrap_err().contains("deeper than"));
    }

    /// A patrol's spawner reads as a spawner though it carries a `Path`; a bare name is a location.
    #[test]
    fn a_marker_reads_as_its_first_listed_class() {
        let set = |c: &[&'static str]| c.iter().copied().collect::<BTreeSet<&str>>();
        assert_eq!(marker_kind(&set(&["AiPatrol", "Path", "PopulationSimpleSpawner"])), "Spawner");
        assert_eq!(marker_kind(&set(&["Path"])), "Path");
        assert_eq!(marker_kind(&set(&["LightObject", "HibernationControl"])), "Light");
        assert_eq!(marker_kind(&set(&["HibernationControl"])), KIND_LOCATION);
    }

    /// The projection is the renderer's: X is mirrored, so a point to the camera's right lands left
    /// of centre, and a point behind the camera does not project.
    #[test]
    fn projection_mirrors_x_and_drops_what_is_behind() {
        let size = egui::vec2(800.0, 600.0);
        let vp = view_proj(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO, 800.0 / 600.0);
        let (c, _) = project(vp, Vec3::ZERO, size).unwrap();
        assert!((c.x - 400.0).abs() < 0.01 && (c.y - 300.0).abs() < 0.01, "{c:?}");
        let (right, _) = project(vp, Vec3::new(1.0, 0.0, 0.0), size).unwrap();
        assert!(right.x < 400.0, "{right:?}");
        let (up, _) = project(vp, Vec3::new(0.0, 1.0, 0.0), size).unwrap();
        assert!(up.y < 300.0, "{up:?}");
        assert!(project(vp, Vec3::new(0.0, 0.0, -20.0), size).is_none());
    }

    /// A click answers with the nearest thing under it, a drawn entity within its projected sphere and
    /// a marker within a few points.
    #[test]
    fn a_click_picks_the_nearest_entity_under_it() {
        let size = egui::vec2(800.0, 600.0);
        let vp = view_proj(Vec3::new(0.0, 0.0, -10.0), Vec3::ZERO, 800.0 / 600.0);
        let centre = egui::pos2(400.0, 300.0);
        let far = entity(1, [0.0, 0.0, 10.0], Some((Vec3::new(0.0, 0.0, 10.0), 2.0)));
        let near = entity(2, [0.0, 0.0, 0.0], Some((Vec3::ZERO, 1.0)));
        let marker = entity(3, [3.0, 0.0, 0.0], None);
        let all = [far, near, marker];
        assert_eq!(pick(&all, vp, size, centre), Some(1), "the nearer of two under the cursor");
        let (at, _) = project(vp, Vec3::new(3.0, 0.0, 0.0), size).unwrap();
        assert_eq!(pick(&all, vp, size, at + egui::vec2(6.0, 0.0)), Some(2), "a marker within its pick radius");
        assert_eq!(pick(&all, vp, size, at + egui::vec2(40.0, 0.0)), None, "nothing there");
    }

    /// A node's world transform runs down its parents, and a child's offset lands on it.
    #[test]
    fn node_worlds_chain_through_their_parents() {
        let translated = |x: f32, y: f32| Mat4::from_translation(Vec3::new(x, y, 0.0)).to_cols_array();
        let node = |index: usize, hash: u32, parent: Option<usize>, local: [f32; 16]| orch::HierNode { index, hash, parent, local, bbox_min: [0.0; 3], bbox_max: [0.0; 3] };
        let nodes = node_worlds(&[node(0, 10, None, translated(1.0, 0.0)), node(1, 11, Some(0), translated(0.0, 2.0))]);
        assert_eq!(nodes[&11].transform_point3(Vec3::ZERO), Vec3::new(1.0, 2.0, 0.0));
    }

    /// A seeded view frames its bounds, with a floor for a layer of one entity.
    #[test]
    fn a_layer_of_one_entity_still_frames_it() {
        let v = LayerView {
            hash: 1,
            label: String::new(),
            block: 0,
            entities: vec![entity(1, [10.0, 0.0, 0.0], None)],
            min: Vec3::new(10.0, 0.0, 0.0),
            max: Vec3::new(10.0, 0.0, 0.0),
            kinds: BTreeMap::new(),
            models: BTreeMap::new(),
            selected: None,
            follow: false,
            framed: None,
            reframe: false,
            missing: BTreeSet::new(),
            errors: Vec::new(),
            pending: VecDeque::new(),
            jobs: 0,
            loaded: HashMap::new(),
            owned: Vec::new(),
            spawned: Vec::new(),
            states: HashMap::new(),
        };
        let (target, dist) = v.framing();
        assert_eq!(target, Vec3::new(10.0, 0.0, 0.0));
        assert_eq!(dist, 12.0);
        assert_eq!(v.entity_framing(0), Some((Vec3::new(10.0, 0.0, 0.0), 7.0)));
    }
}

#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;
    use mercs2_formats::types::{TYPE_HASH_LAYER, TYPE_ID_LAYER};

    fn stack() -> WadStack {
        let start = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = mercs2_formats::game_paths::local_config_vz_wad(start).unwrap_or_else(|e| panic!("{e}"));
        WadStack::open(path.to_str().expect("a UTF-8 path"), &[]).unwrap_or_else(|e| panic!("{e}"))
    }

    fn layer_hashes(w: &WadStack) -> BTreeSet<u32> {
        w.wads.iter().flat_map(wad::all_asets).filter(|(_, ty, _)| *ty == TYPE_ID_LAYER).map(|(h, _, _)| h).collect()
    }

    /// The spec's reference layer: its 361 entities, 336 with a template, every one a model or a
    /// labelled marker.
    #[test]
    fn the_reference_layer_reads_every_entity() {
        let mut w = stack();
        let t = Templates::read(&mut w).unwrap_or_else(|e| panic!("{e}"));
        let hash = mercs2_formats::hash::pandemic_hash_m2("vz_state_pmccon004");
        let c = w.typed_container(TYPE_HASH_LAYER, hash).unwrap_or_else(|e| panic!("{e}"));
        let v = LayerView::read(hash, "vz_state_pmccon004".into(), 0, &c, &t).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(v.entities.len(), 361);
        assert_eq!(v.entities.iter().filter(|e| e.parent != 0).count(), 336);
        let models = v.entities.iter().filter(|e| e.kind == KIND_MODEL).count();
        assert_eq!(models + v.kinds.iter().filter(|(k, _)| **k != KIND_MODEL).map(|(_, n)| n).sum::<usize>(), 361);
        assert!(models >= 286, "{models}: the entities with their own or their template's model, and those with only link children");
    }

    /// Every layer of the stack reads, and every link's quaternion is a rotation, which pins where the
    /// record keeps it.
    #[test]
    fn every_layer_reads_and_every_link_is_a_rotation() {
        let mut w = stack();
        let t = Templates::read(&mut w).unwrap_or_else(|e| panic!("{e}"));
        let (mut entities, mut failed) = (0usize, Vec::new());
        for h in layer_hashes(&w) {
            match w.typed_container(TYPE_HASH_LAYER, h).and_then(|c| LayerView::read(h, String::new(), 0, &c, &t)) {
                Ok(v) => entities += v.entities.len(),
                Err(e) => failed.push(format!("0x{h:08X}: {e}")),
            }
        }
        assert!(failed.is_empty(), "{failed:#?}");
        assert_eq!(entities, 101_299);
        for l in t.links.values().flatten() {
            let n: f32 = l.quat.iter().map(|q| q * q).sum();
            assert!((n - 1.0).abs() < 1e-3, "{l:?}");
        }
    }

    /// Retail's own positions, from the `updated-chris-exports` captures. Light `0x0010D532` stores
    /// (0, 5.97, 0) and sits at its telephone pole's link, 4 m below the pole's root, while the pole
    /// sleeps. Light `0x000CC64D` sits at pump `0x000F809F`'s hard point once the pump has woken. The
    /// layer's bounds hold no entity at the origin.
    #[test]
    fn linked_entities_sit_where_retail_puts_them() {
        let mut w = stack();
        let t = Templates::read(&mut w).unwrap_or_else(|e| panic!("{e}"));
        let view = |w: &mut WadStack, layer: u32| {
            let c = w.typed_container(TYPE_HASH_LAYER, layer).unwrap_or_else(|e| panic!("{e}"));
            LayerView::read(layer, String::new(), 0, &c, &t).unwrap_or_else(|e| panic!("{e}"))
        };
        let near = |a: [f32; 3], b: [f32; 3]| (Vec3::from(a) - Vec3::from(b)).length() < 0.01;

        let poles = view(&mut w, 0xC9DC992F);
        let light = poles.entities.iter().find(|e| e.key == 0x0010_D532).expect("the light");
        assert!(light.link.is_some());
        assert!(near(light.stored_pos, [-0.000488, 5.967201, 0.0]), "{:?}", light.stored_pos);
        assert!(near(light.pos, [2424.3787, -37.7539, 1062.9254]), "{:?}", light.pos);
        assert!(poles.entities.iter().all(|e| Vec3::from(e.pos).length() > 100.0));

        let mut pumps = view(&mut w, 0x2A6B3A36);
        let i = pumps.entities.iter().position(|e| e.key == 0x000C_C64D).expect("the light");
        let link = pumps.entities[i].link.clone().expect("linked");
        let model = pumps.entities[link.parent].parts[0].model.expect("a model");
        assert!(pumps.entity_world(i, 0).is_none(), "waits for the pump's model");
        let c = w.extract_container(model).unwrap_or_else(|e| panic!("{e}"));
        let hier = orch::parse_hier(&c);
        pumps.loaded.insert(model, LoadedModel { nodes: node_worlds(&hier), bounds: (Vec3::ZERO, 1.0), machine: None, hier, mixr: None });
        let at = pumps.entity_world(i, 0).expect("the model is there").unwrap_or_else(|e| panic!("{e}"));
        let (_, _, translation) = at.to_scale_rotation_translation();
        assert!(near(translation.to_array(), [1935.6718, -31.433954, 1498.9756]), "{translation}");

        // Framed before the pump's model arrived, the light asks to be framed where it ended up, once.
        let (before, _) = pumps.frame_entity(i).expect("the entity");
        assert!(pumps.take_reframe().is_none());
        pumps.place_linked(i, at);
        let (after, _) = pumps.take_reframe().expect("it moved");
        assert!(near(after.to_array(), [1935.6718, -31.433954, 1498.9756]) && !near(before.to_array(), after.to_array()));
        assert!(pumps.take_reframe().is_none());
    }

    /// Every link's hard point names a node of its parent's model, so the view places every child
    /// that ships. Printed by count: the misses are the record of what a hard point can be.
    #[test]
    fn hard_points_name_nodes_of_their_parents() {
        let mut w = stack();
        let t = Templates::read(&mut w).unwrap_or_else(|e| panic!("{e}"));
        let (mut found, mut none) = (0usize, Vec::new());
        for (template, links) in &t.links {
            let Some(&model) = t.world.models.get(template).filter(|&&m| m != 0) else { continue };
            let Ok(c) = w.extract_container(model) else { continue };
            let nodes: BTreeSet<u32> = orch::parse_hier(&c).iter().map(|n| n.hash).collect();
            for l in links.iter().filter(|l| l.hard_point != 0) {
                if nodes.contains(&l.hard_point) {
                    found += 1;
                } else {
                    none.push((*template, l.hard_point, l.child));
                }
            }
        }
        println!("[layer] hard points on their parent's model: {found}; not: {}", none.len());
        for (t, h, c) in none.iter().take(30) {
            println!("[layer]   template 0x{t:08X} hard point 0x{h:08X} child 0x{c:08X}");
        }
        assert!(found > 0);
    }
}
