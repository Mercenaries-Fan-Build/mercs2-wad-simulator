//! The `add_tiny_geometry` editor's preview: the stand-in as `qm build` makes it
//! (`tiny::build_stand_in`), drawn at its cell's centre as its intact or its ruined part, each
//! object's part tinted by its slot on request; and the objects it stands in for, ringed in their
//! layer. The build and the rules' findings (`tiny::hermetic_problems`, `tiny::game_problems`) are
//! read on a worker over the game's layers (`tiny::World`), which are read once per game.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mercs2_formats::tiny_model::{self as tm, TinyModel};
use mercs2_engine::mesh::{DrawGroup, Vertex};
use mercs2_quartermaster::manifest::{asset_hash, Contribution};
use mercs2_quartermaster::tiny::{self, BuiltStandIn};

use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};

/// The item's first `add_tiny_geometry`: its index.
pub fn of(item: &Item) -> Option<usize> {
    item.file.as_ref().ok()?.contributions.iter().position(|c| matches!(c, Contribution::AddTinyGeometry { .. }))
}

/// What the viewport draws for a stand-in item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shown {
    /// The intact part, as `PgMeshTinyVP` draws it for objects whose state is 1.
    Intact,
    /// The ruined part, as `PgMeshTinyVP_Ruin` draws it for objects whose state is 3.
    Ruined,
    /// The objects it stands in for, in their layer.
    Objects,
}

/// A stand-in as the build makes it, with where it sits and what the rules say of it.
pub struct TinyAfter {
    pub built: BuiltStandIn,
    /// The cell's centre, where its `TinyGeometryObject` placement sits, unrotated.
    pub centre: [f32; 3],
    pub findings: Vec<String>,
    /// The layers that place its objects (`tiny::World::placed`), as a block of their entries: an
    /// object may be another layer's, as retail's `vz_*_tiny` stand-ins draw.
    pub objects: Vec<u8>,
}

/// The game's layers as the stand-in rules read them, and each layer's container, by name hash.
pub struct StandInWorld {
    pub world: tiny::World,
    pub layers: std::collections::BTreeMap<u32, Vec<u8>>,
}

/// A block of the layer containers `layers` (by name hash), in that order: the table of their
/// entries, then the containers. One in the compact form a ported level ships is refused.
pub fn layers_block(layers: &[(u32, &[u8])]) -> Result<Vec<u8>, String> {
    let mut out = (layers.len() as u32).to_le_bytes().to_vec();
    for (name, c) in layers {
        if mercs2_formats::be_to_le::ecs::is_compact(c) {
            return Err(format!("layer 0x{name:08X} is in the compact form a ported level ships, which the Depot does not read"));
        }
        for w in [*name, mercs2_formats::types::TYPE_HASH_LAYER, 0, c.len() as u32] {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }
    for (_, c) in layers {
        out.extend_from_slice(c);
    }
    Ok(out)
}

/// What contribution `index` of `contributions` (an item at `dir`) builds, against `sw`.
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution], index: usize, sw: &StandInWorld) -> Result<TinyAfter, String> {
    let world = &sw.world;
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let manifest = crate::depot::item_manifest(&item);
    let cs = tiny::contributions(&manifest);
    let c = cs.iter().find(|c| c.index == index).ok_or_else(|| format!("contribution {index} is no add_tiny_geometry"))?;
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let mut findings: Vec<String> = tiny::hermetic_problems(&manifest, Some(dir), &[])
        .into_iter()
        .chain(tiny::game_problems(&manifest, &mut game))
        .filter(|(i, _)| *i == index)
        .map(|(_, p)| format!("{}: {}", p.code, p.message))
        .collect();
    findings.dedup();
    let added: BTreeSet<u32> = contributions
        .iter()
        .filter_map(|c| match c {
            Contribution::AddTexture { name, .. } => Some(asset_hash(name)),
            _ => None,
        })
        .collect();
    let has_texture = |t: u32| game.has_asset(t, mercs2_formats::types::TYPE_ID_TEXTURE) || added.contains(&t);
    let built = tiny::build_stand_in(c, dir, world, &has_texture)?;
    let placing: BTreeSet<u32> = built.guids.iter().filter_map(|g| world.placed.get(g)).flatten().map(|(l, _)| *l).collect();
    let containers: Vec<(u32, &[u8])> = placing
        .iter()
        .map(|l| sw.layers.get(l).map(|c| (*l, c.as_slice())).ok_or_else(|| format!("layer 0x{l:08X} places an object but was not read")))
        .collect::<Result<_, _>>()?;
    let objects = layers_block(&containers)?;
    Ok(TinyAfter { built, centre: tiny::cell_centre(c.row, c.col), findings, objects })
}

/// What the viewport draws of a stand-in.
#[derive(Clone)]
pub struct StandInView {
    /// Changes whenever what is drawn changes.
    pub id: String,
    pub hash: u32,
    pub label: String,
    pub centre: [f32; 3],
    pub ruined: bool,
    pub tint: bool,
    pub after: Rc<TinyAfter>,
}

/// What each stand-in builds, by item folder and index: the stamp it was computed from, the build's
/// number and the build.
type AfterCache = HashMap<(PathBuf, usize), (String, u64, Loading<TinyAfter>)>;

/// The stand-in editors' reads and choices.
#[derive(Default)]
pub struct Tiny {
    /// The game's layers as the stand-in rules read them, once per game.
    world: RefCell<Option<Loading<Arc<StandInWorld>>>>,
    /// What each stand-in builds, by item folder and index, with what it was computed from and the
    /// build's number.
    after: RefCell<AfterCache>,
    builds: Cell<u64>,
    /// Shared objects blocks for the objects view, by view id.
    blocks: RefCell<HashMap<String, Rc<Vec<u8>>>>,
    /// What the viewport draws, by item folder.
    pub shown: RefCell<HashMap<PathBuf, Shown>>,
    /// Tint each object's part by its slot.
    pub tint: Cell<bool>,
}

impl Tiny {
    /// Forget what was read from the game (a different stack).
    pub fn forget_game(&self) {
        *self.world.borrow_mut() = None;
        self.after.borrow_mut().clear();
        self.blocks.borrow_mut().clear();
    }

    pub fn shown_of(&self, item: &Item) -> Shown {
        self.shown.borrow().get(&item.dir).copied().unwrap_or(Shown::Intact)
    }

    /// The game's layers: `None` while they are read.
    fn world(&self, game: &[PathBuf]) -> Option<Result<Arc<StandInWorld>, String>> {
        let mut slot = self.world.borrow_mut();
        let entry = slot.get_or_insert_with(|| {
            let wads = game.to_vec();
            spawn(move || {
                if wads.is_empty() {
                    return Err("no game archive is open".to_string());
                }
                let mut game = mercs2_quartermaster::game::GameStack::open(&wads).map_err(|e| format!("{e:?}"))?;
                let layers = game.layer_containers()?;
                let world = tiny::World::read(&layers)?;
                Ok(Arc::new(StandInWorld { world, layers }))
            })
        });
        poll(entry).map(|r| r.map(|w| Arc::clone(&w)))
    }

    /// What `item`'s stand-in builds: `None` while the game's layers or the build are read.
    pub fn after(&self, game: &[PathBuf], item: &Item) -> Option<Result<Rc<TinyAfter>, String>> {
        self.numbered_after(game, item).map(|(_, r)| r)
    }

    fn numbered_after(&self, game: &[PathBuf], item: &Item) -> Option<(u64, Result<Rc<TinyAfter>, String>)> {
        let index = of(item)?;
        let world = match self.world(game)? {
            Ok(w) => w,
            Err(e) => return Some((0, Err(e))),
        };
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        let mut stamp = format!("{contributions:?}");
        if let Some(Contribution::AddTinyGeometry { model, .. }) = contributions.get(index) {
            let t = std::fs::metadata(item.dir.join(model)).and_then(|m| m.modified()).ok();
            stamp.push_str(&format!("|{t:?}"));
        }
        let key = (item.dir.clone(), index);
        let mut map = self.after.borrow_mut();
        if map.get(&key).is_none_or(|(s, _, _)| *s != stamp) {
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            let job = spawn(move || after(&wads, &dir, &contributions, index, &world));
            let n = self.builds.get();
            self.builds.set(n + 1);
            map.insert(key.clone(), (stamp, n, job));
        }
        let (_, n, entry) = map.get_mut(&key)?;
        Some((*n, poll(entry)?))
    }

    /// What the viewport draws of `item`'s stand-in: its intact or ruined part. `None` for the objects
    /// view, and while it builds.
    pub fn stand_in_view(&self, game: &[PathBuf], item: &Item) -> Option<StandInView> {
        let shown = self.shown_of(item);
        if shown == Shown::Objects {
            return None;
        }
        let (build, after) = self.numbered_after(game, item)?;
        let after = after.ok()?;
        let ruined = shown == Shown::Ruined;
        let tint = self.tint.get();
        Some(StandInView {
            id: format!("{}|build {build}|{ruined}|{tint}", item.dir.display()),
            hash: after.built.hash,
            label: format!("{} ({})", after.built.name, if ruined { "ruined" } else { "intact" }),
            centre: after.centre,
            ruined,
            tint,
            after,
        })
    }

    /// The viewport's view of `item`'s objects: the layers that place them as the game carries them,
    /// the objects ringed. `None` for the stand-in views, and while the build is read.
    pub fn objects_view(&self, game: &[PathBuf], item: &Item) -> Option<crate::depotworld::View> {
        if self.shown_of(item) != Shown::Objects {
            return None;
        }
        let (build, after) = self.numbered_after(game, item)?;
        let after = after.ok()?;
        let id = format!("{}|build {build}|objects", item.dir.display());
        let block = {
            let mut blocks = self.blocks.borrow_mut();
            let own = format!("{}|", item.dir.display());
            if !blocks.contains_key(&id) {
                blocks.retain(|k, _| !k.starts_with(&own));
                blocks.insert(id.clone(), Rc::new(after.objects.clone()));
            }
            blocks[&id].clone()
        };
        Some(crate::depotworld::View {
            id,
            frame: format!("{}|objects", item.dir.display()),
            label: "the stand-in's objects".to_string(),
            block,
            marked: Rc::new(after.built.guids.iter().copied().collect()),
            marked_as: "of them its objects",
            focus: None,
        })
    }
}

fn unhalf(h: u16) -> f32 {
    mercs2_formats::model_inject::read_f16_le(&h.to_le_bytes(), 0)
}

/// A colour of its own for each object slot: hues a golden angle apart.
pub fn slot_colour(slot: usize) -> [f32; 4] {
    let h = (slot as f32 * 0.618_034).fract() * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    [0.35 + 0.65 * r, 0.35 + 0.65 * g, 0.35 + 0.65 * b, 1.0]
}

/// A mesh as the engine loads it: its vertices, triangle indices and draws.
pub type StandInMesh = (Vec<Vertex>, Vec<u32>, Vec<DrawGroup>);

/// The stand-in's intact or ruined part as a mesh: its sub-object of that role's `HIER` node, each
/// group's strip as triangles, positions relative to the placement, each draw on its material's
/// texture and pixel shader; with `tint`, each vertex coloured by its object's slot.
pub fn stand_in_mesh(model: &TinyModel, ruined: bool, tint: bool) -> Result<StandInMesh, String> {
    let want = mercs2_formats::hash::pandemic_hash_m2(if ruined { tm::NODE_RUIN } else { tm::NODE_PRISTINE });
    let (mut verts, mut indices, mut draws) = (Vec::new(), Vec::new(), Vec::new());
    for (si, sub) in model.sub_objects.iter().enumerate() {
        let node = model.nodes.get(sub.node as usize).ok_or_else(|| format!("sub-object {si} hangs from node {}, which the model lacks", sub.node))?;
        if node.name_hash != want {
            continue;
        }
        for (gi, g) in sub.groups.iter().enumerate() {
            let base = verts.len() as u32;
            for v in &g.vertices {
                let slot = unhalf(v.position[3]) as usize;
                verts.push(Vertex {
                    pos: [unhalf(v.position[0]), unhalf(v.position[1]), unhalf(v.position[2])],
                    color: if tint { slot_colour(slot) } else { [1.0; 4] },
                    uv: [unhalf(v.uv[0]), unhalf(v.uv[1])],
                    normal: [unhalf(v.normal[0]), unhalf(v.normal[1]), unhalf(v.normal[2])],
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    joints: [0; 4],
                    weights: [255, 0, 0, 0],
                });
            }
            let start = indices.len() as u32;
            for k in 0..g.strip.len().saturating_sub(2) {
                let (a, b, c) = (g.strip[k], g.strip[k + 1], g.strip[k + 2]);
                if a == b || b == c || a == c {
                    continue;
                }
                let t = if k % 2 == 0 { [a, b, c] } else { [b, a, c] };
                indices.extend(t.iter().map(|&i| base + i as u32));
            }
            let m = g.prims.first().ok_or_else(|| format!("sub-object {si} group {gi} has no primitive record"))?.material as usize;
            let mat = model.materials.get(m).ok_or_else(|| format!("sub-object {si} group {gi} names material {m}, which the model lacks"))?;
            draws.push(DrawGroup {
                index_start: start,
                index_count: indices.len() as u32 - start,
                diffuse: mat.textures.first().copied(),
                textures: mat.textures.clone(),
                material_flags: mat.flags,
                shader_key: mat.pixel_shader,
                group_index: draws.len(),
                ..DrawGroup::default()
            });
        }
    }
    if draws.is_empty() {
        return Err(format!("the stand-in has no {} part", if ruined { "ruined" } else { "intact" }));
    }
    Ok((verts, indices, draws))
}

/// The inspector card of a stand-in item: what the build makes, the rules' findings, and what the
/// viewport draws.
pub fn tiny_cards(ui: &mut egui::Ui, t: &Tiny, item: &Item, game: &[PathBuf]) -> Vec<Ask> {
    let mut asks = Vec::new();
    theme::card(ui, "Stand-in", None, |ui| {
        match t.after(game, item) {
            None => {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::new());
                    ui.label(egui::RichText::new("Reading the game's layers and building the stand-in\u{2026}").small().color(theme::DIM));
                });
            }
            Some(Err(e)) => theme::field_note(ui, FieldState::Bad, &format!("qm build: {e}")),
            Some(Ok(a)) => {
                let mono = |s: String| egui::RichText::new(s).monospace();
                theme::kv(ui, "model", mono(a.built.name.clone()));
                theme::kv(ui, "hash", mono(format!("0x{:08X}", a.built.hash)));
                theme::kv(ui, "at", mono(format!("{:.1}, {:.1}, {:.1}", a.centre[0], a.centre[1], a.centre[2])));
                theme::kv(ui, "objects \u{b7} slots", mono(format!("{} \u{b7} {}", a.built.guids.len(), a.built.plan.list.len())));
                let groups: usize = a.built.model.sub_objects.iter().map(|s| s.groups.len()).sum();
                theme::kv(ui, "sub-objects \u{b7} groups", mono(format!("{} \u{b7} {groups}", a.built.model.sub_objects.len())));
                for f in &a.findings {
                    theme::field_note(ui, FieldState::Bad, f);
                }
                ui.add_space(4.0);
                for (i, (g, slot)) in a.built.guids.iter().zip(&a.built.plan.slot_of).enumerate() {
                    ui.horizontal(|ui| {
                        if t.tint.get() {
                            let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                            let c = slot_colour(*slot as usize);
                            ui.painter().rect_filled(r, 2.0, egui::Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8));
                        }
                        ui.label(mono(format!("objects[{i}] 0x{g:08X} \u{b7} slot {slot}")).color(theme::DIM));
                    });
                }
            }
        }
        ui.add_space(4.0);
        let shown = t.shown_of(item);
        ui.horizontal_wrapped(|ui| {
            theme::field_label(ui, "Viewport");
            for (label, s) in [("Intact", Shown::Intact), ("Ruined", Shown::Ruined), ("Objects", Shown::Objects)] {
                if theme::pill(ui, label, shown == s).clicked() && shown != s {
                    asks.push(Ask::Show(item.dir.clone(), s));
                }
            }
        });
        if shown != Shown::Objects {
            ui.horizontal(|ui| {
                theme::field_label(ui, "Colour");
                if theme::pill(ui, "By object", t.tint.get()).clicked() {
                    asks.push(Ask::Tint(!t.tint.get()));
                }
            });
        }
    });
    asks
}

/// What the stand-in card asks the page to do.
pub enum Ask {
    Show(PathBuf, Shown),
    Tint(bool),
}

/// Over the viewport while a stand-in item has nothing drawn: that it builds, or qm's refusal.
pub fn viewport_note(ctx: &egui::Context, t: &Tiny, game: &[PathBuf], item: &Item, rect: egui::Rect) {
    if of(item).is_none() || t.stand_in_view(game, item).is_some() || t.objects_view(game, item).is_some() {
        return;
    }
    let (text, colour) = match t.after(game, item) {
        None => ("Building the stand-in\u{2026}".to_string(), theme::DIM),
        Some(Err(e)) => (format!("qm build: {e}"), theme::BAD),
        Some(Ok(_)) => return,
    };
    egui::Area::new(egui::Id::unique("depot_tiny_note"))
        .fixed_pos(rect.left_top() + egui::vec2(24.0, rect.height() * 0.4))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(theme::G1)
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    ui.set_max_width(rect.width() - 72.0);
                    ui.label(egui::RichText::new(text).color(colour));
                });
        });
}

/// Over the viewport while a stand-in part is drawn: which, and where.
pub fn label(ctx: &egui::Context, v: &StandInView, rect: egui::Rect) {
    egui::Area::new(egui::Id::unique("depot_tiny_label"))
        .fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 34.0))
        .interactable(false)
        .show(ctx, |ui| {
            let c = v.centre;
            ui.label(egui::RichText::new(format!("{} \u{b7} 0x{:08X} \u{b7} at {:.1}, {:.1}, {:.1}", v.label, v.hash, c[0], c[1], c[2])).monospace().color(theme::DIM));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each slot gets a colour, bright enough to read on a texture, and neighbours differ.
    #[test]
    fn slot_colours_differ_between_neighbours() {
        let c: Vec<[f32; 4]> = (0..12).map(slot_colour).collect();
        for w in c.windows(2) {
            assert_ne!(w[0], w[1]);
        }
        assert!(c.iter().flatten().all(|v| (0.35..=1.0).contains(v)));
    }

    /// Against retail, the Depot's own path: a retail stand-in taken apart by `qm extract-tiny` and
    /// built back through `after` is the shipped model, the rules name the cell and the key it
    /// shares with the shipped one (M0247, M0250), and the objects view draws every object.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_stand_in_rebuilt_in_the_depot_is_the_shipped_one() {
        let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
        let wads = vec![wad];
        let name = "vz_merida_tiny_tinygeometry_tgr12_tgc29_0x00144ece";
        let dir = std::env::temp_dir().join(format!("depottiny-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let mut game = mercs2_quartermaster::game::GameStack::open(&wads).unwrap_or_else(|e| panic!("{e:?}"));
        let layers = game.layer_containers().unwrap_or_else(|e| panic!("{e}"));
        let world = StandInWorld { world: tiny::World::read(&layers).unwrap_or_else(|e| panic!("{e}")), layers };
        let container = game
            .container_for_asset(asset_hash(name), mercs2_formats::types::TYPE_HASH_MODEL, mercs2_formats::types::TYPE_ID_MODEL)
            .unwrap_or_else(|| panic!("{name} in the game stack"));
        let model = TinyModel::decode(&container).unwrap_or_else(|e| panic!("{e}"));
        let d = tiny::decompose(&model, 12, 29, &|_| None).unwrap_or_else(|e| panic!("{e}"));
        std::fs::write(dir.join("src/stand_in.glb"), &d.glb).unwrap();
        let contributions = vec![Contribution::AddTinyGeometry {
            layer: "vz_merida_tiny".into(),
            cell: mercs2_quartermaster::manifest::TinyCell { row: 12, col: 29 },
            key: 0x0014_4ECE,
            objects: d.objects.iter().map(|g| format!("0x{g:08X}")).collect(),
            model: "src/stand_in.glb".into(),
        }];
        let a = after(&wads, &dir, &contributions, 0, &world).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.built.hash, asset_hash(name));
        assert!(a.findings.iter().any(|f| f.starts_with("M0247")), "{:?}", a.findings);
        assert!(a.findings.iter().any(|f| f.starts_with("M0250")), "{:?}", a.findings);
        let mut stack = crate::app::WadStack::open(wads[0].to_str().expect("a UTF-8 path"), &[]).unwrap_or_else(|e| panic!("{e}"));
        let t = crate::layerview::Templates::read(&mut stack).unwrap_or_else(|e| panic!("{e}"));
        let v = crate::layerview::LayerView::read(0, String::new(), 0, &a.objects, &t).unwrap_or_else(|e| panic!("{e}"));
        let drawn: BTreeSet<u32> = v.entities.iter().map(|e| e.key).collect();
        let missing: Vec<String> = a.built.guids.iter().filter(|g| !drawn.contains(g)).map(|g| format!("0x{g:08X}")).collect();
        assert!(missing.is_empty(), "objects the view does not draw: {missing:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Against retail: a shipped stand-in (the model its placement's `ModelName` names) draws its
    /// intact part as triangles, each draw on its material's texture and pixel shader.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_stand_in_builds_and_draws_its_intact_part() {
        let wad = mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"));
        let wads = vec![wad];
        let mut game = mercs2_quartermaster::game::GameStack::open(&wads).unwrap_or_else(|e| panic!("{e:?}"));
        let layers = game.layer_containers().unwrap_or_else(|e| panic!("{e}"));
        let world = tiny::World::read(&layers).unwrap_or_else(|e| panic!("{e}"));
        let &(layer, key, _) = world.stand_ins.first().expect("a retail stand-in");
        // The placement's ModelName record names the stand-in's model.
        let records = mercs2_formats::placement_build::read_layer_records(&layers[&layer]).unwrap_or_else(|e| panic!("{e}"));
        let model_hash = records.models.iter().find(|(k, _)| *k == key).map(|(_, m)| *m).expect("the stand-in's ModelName");
        let container = game
            .container_for_asset(model_hash, mercs2_formats::types::TYPE_HASH_MODEL, mercs2_formats::types::TYPE_ID_MODEL)
            .unwrap_or_else(|| panic!("model 0x{model_hash:08X} in the game stack"));
        let model = TinyModel::decode(&container).unwrap_or_else(|e| panic!("0x{model_hash:08X}: {e}"));
        let (verts, indices, draws) = stand_in_mesh(&model, false, true).unwrap_or_else(|e| panic!("0x{model_hash:08X}: {e}"));
        assert!(!draws.is_empty() && indices.len() % 3 == 0 && !indices.is_empty());
        assert!(indices.iter().all(|&i| (i as usize) < verts.len()));
        assert!(draws.iter().all(|d| d.diffuse.is_some() && d.shader_key != 0));
        eprintln!("stand-in 0x{model_hash:08X}: {} vertices, {} triangles, {} draws", verts.len(), indices.len() / 3, draws.len());
    }
}
