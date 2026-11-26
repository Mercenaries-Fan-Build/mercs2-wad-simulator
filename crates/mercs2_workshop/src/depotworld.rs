//! The world editors of an item's `edit_world`, `add_placement` and `add_layer` contributions: the
//! file's records as forms beside the shipped layer, the layer's entities to pick from, and the block
//! `qm build` makes of them, drawn in the Depot's viewport.
//!
//! Files are read and written with qm's own readers and writers (`world::parse` / `write_world`,
//! `world::read_entities` / `write_entities`). Any file that reads is edited: a write rewrites only
//! what the edit changed ([`mercs2_quartermaster::lossless`]), so what an author wrote by hand (a
//! comment, a spelling of a number) stays. A write is refused when the file on disk is no longer the
//! text the edit was made from. The built block is
//! qm's (`build::world_block`, with `lint::placement_game_checks`), read on a worker.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use glam::{EulerRot, Quat};
use mercs2_formats::placement::{entity_key_set, load_model_placements, load_placements, Placement};
use mercs2_quartermaster::build as qb;
use mercs2_quartermaster::manifest::{bare_hash, Contribution};
use mercs2_quartermaster::world::{self as qw, EntityEdit, EntityFile, EntityRecord, WorldDoc};

use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};
use crate::kindform::{List, Sources};

/// Which file a world contribution's records are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// `edit_world`: moves, turns and re-models of the layer's own entities.
    Edit,
    /// `add_placement`: new entities in the named layer.
    Place,
    /// `add_layer`: new entities in a new layer, built on the template's block.
    Layer,
}

/// A world contribution: which file it is, the layer it is read against (`edit_world`'s and
/// `add_placement`'s `layer`, `add_layer`'s `template`), and the file.
pub struct Subject<'a> {
    pub which: Which,
    pub layer: &'a str,
    pub file: &'a Path,
}

pub fn of(c: &Contribution) -> Option<Subject<'_>> {
    match c {
        Contribution::EditWorld { layer, edits } => Some(Subject { which: Which::Edit, layer, file: edits.as_path() }),
        Contribution::AddPlacement { layer, entity } => Some(Subject { which: Which::Place, layer, file: entity.as_path() }),
        Contribution::AddLayer { template, entities, .. } => Some(Subject { which: Which::Layer, layer: template, file: entities.as_path() }),
        _ => None,
    }
}

/// The item's first world contribution: its index and subject.
pub fn first(item: &Item) -> Option<(usize, Subject<'_>)> {
    item.file.as_ref().ok()?.contributions.iter().enumerate().find_map(|(i, c)| of(c).map(|s| (i, s)))
}

/// The item's first `activate_layer`: its index, and the layers it names — the one it marks for
/// addition, then each it marks for removal.
pub fn activation(item: &Item) -> Option<(usize, Vec<String>)> {
    item.file.as_ref().ok()?.contributions.iter().enumerate().find_map(|(i, c)| match c {
        Contribution::ActivateLayer { layer, replaces } => Some((i, std::iter::once(layer).chain(replaces).cloned().collect())),
        _ => None,
    })
}

/// Whether the item's editor draws a layer in the viewport: a world file's build, or the layers an
/// `activate_layer` names.
pub fn draws_layer(item: &Item) -> bool {
    first(item).is_some() || activation(item).is_some()
}

/// Whether the item's editor draws in the viewport: a layer, or a terrain cell.
pub fn draws(item: &Item) -> bool {
    draws_layer(item) || terrain(item).is_some()
}

/// The item's first `replace_terrain_cell`: its index, the cell it replaces, and the cell file.
pub fn terrain(item: &Item) -> Option<(usize, &str, &Path)> {
    item.file.as_ref().ok()?.contributions.iter().enumerate().find_map(|(i, c)| match c {
        Contribution::ReplaceTerrainCell { target, cell } => Some((i, target.as_str(), cell.as_path())),
        _ => None,
    })
}

/// Where the game places a terrain cell, and the cell it ships: the `TerrainObject` placement that
/// names it (its layer, key, position and parent template, whose `TerrainFade` it draws with).
pub struct CellPlace {
    pub layer: u32,
    pub key: u32,
    pub pos: [f32; 3],
    pub parent: u32,
    pub shipped: Vec<u8>,
}

/// Read terrain cell `target` from the game stack `wads`, and the placement that puts it in the world.
pub fn read_cell_place(wads: &[PathBuf], target: &str) -> Result<CellPlace, String> {
    use mercs2_formats::types::{TYPE_HASH_TERRAIN_MESH, TYPE_ID_TERRAIN_MESH};
    let hash = mercs2_quartermaster::manifest::asset_hash(target);
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let shipped = game
        .container_for_asset(hash, TYPE_HASH_TERRAIN_MESH, TYPE_ID_TERRAIN_MESH)
        .ok_or_else(|| format!("{target:?} (0x{hash:08X}) is no terrain cell of the game stack"))?;
    let mut unread = Vec::new();
    for (layer, c) in game.layer_containers()? {
        let tiles = match mercs2_formats::placement::load_terrain_tiles(&c) {
            Ok(t) => t,
            Err(e) => {
                unread.push(format!("layer 0x{layer:08X}: {e}"));
                continue;
            }
        };
        let Some(t) = tiles.into_iter().find(|t| t.terrainmesh_hash == hash) else { continue };
        let parent = mercs2_formats::placement::placed_entities(&c)?
            .into_iter()
            .find(|e| e.key == t.key)
            .map(|e| e.parent)
            .ok_or_else(|| format!("layer 0x{layer:08X}: terrain tile 0x{:08X} has no Transform", t.key))?;
        return Ok(CellPlace { layer, key: t.key, pos: t.pos, parent, shipped });
    }
    let also = if unread.is_empty() { String::new() } else { format!("; layers whose terrain tiles did not read: {}", unread.join("; ")) };
    Err(format!("no layer of the game stack places terrain cell 0x{hash:08X}{also}"))
}

/// What a terrain cell holds, as facts: patches, draw groups, vertices, materials.
pub fn cell_facts(container: &[u8]) -> Result<[usize; 4], String> {
    let cell = mercs2_formats::terrainmesh::TerrainCell::decode(container)?;
    let groups = cell.geoms.iter().map(|g| g.prmgs.len()).sum();
    let verts = cell.geoms.iter().flat_map(|g| &g.prmgs).map(|p| p.vertex_count()).sum();
    Ok([cell.geoms.len(), groups, verts, cell.mtrl.materials.len()])
}

/// What the viewport draws for an open `replace_terrain_cell` item: the cell, shipped or yours, at
/// its placement.
#[derive(Clone)]
pub struct TerrainView {
    /// Changes whenever what is drawn changes.
    pub id: String,
    pub hash: u32,
    pub label: String,
    pub pos: [f32; 3],
    pub parent: u32,
    pub container: Rc<Vec<u8>>,
}

/// A layer the game stack carries by name (`GameStack::layer_by_name`): the PTHS path of its block,
/// its entry as a block of its own, and how many entities it places.
pub struct NamedLayer {
    pub path: String,
    pub block: Vec<u8>,
    pub entities: usize,
}

/// Read the layer named `name` from `wads`; `None` when no layer row of the stack names it.
pub fn read_named_layer(wads: &[PathBuf], name: &str) -> Result<Option<NamedLayer>, String> {
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let Some((inputs, entry)) = game.layer_by_name(name)? else { return Ok(None) };
    let block = sub_block(&inputs.block, &[entry])?;
    let entities = entity_key_set(&block).len();
    Ok(Some(NamedLayer { path: inputs.path, block, entities }))
}

/// One layer entry of a block: its index in the entry table, its name hash, and the keys it places.
pub struct Entry {
    pub index: usize,
    pub name: u32,
    pub keys: HashSet<u32>,
}

/// The block a layer needle names in the game stack, as the build reads it
/// (`GameStack::layer_block_for_edit`).
pub struct Base {
    pub path: String,
    pub block: Vec<u8>,
    pub places: Vec<Placement>,
    /// Key → its index in [`Self::places`].
    pub at: HashMap<u32, usize>,
    /// Key → its `ModelName` model hash.
    pub models: HashMap<u32, u32>,
    /// Every key a `Transform`, `ModelName` or `Name` record of the block places (M0198's set).
    pub keys: HashSet<u32>,
}

impl Base {
    /// An entity's label: its key, and its name when it has one.
    pub fn label(&self, key: u32) -> String {
        match self.at.get(&key).and_then(|&i| self.places[i].name.as_deref()) {
            Some(n) => format!("0x{key:08X} \u{b7} {n}"),
            None => format!("0x{key:08X}"),
        }
    }

    /// The key an `edit_world` entity names: a bare `0xKEY`, else a name the block's placements carry
    /// (the lowering's lookup, `build::edit_world_block`).
    pub fn key_of(&self, entity: &str) -> Option<u32> {
        bare_hash(entity).or_else(|| self.places.iter().find(|p| p.name.as_deref() == Some(entity)).map(|p| p.key))
    }

    /// The comment the extract writes after an edit's `entity` line: the entity's name.
    fn note(&self, entity: &str) -> Option<String> {
        bare_hash(entity).and_then(|k| self.at.get(&k)).and_then(|&i| self.places[i].name.clone())
    }
}

/// The layer entries of `block`, each with the keys it places.
pub fn layer_entries(block: &[u8]) -> Result<Vec<Entry>, String> {
    Ok(mercs2_formats::ucfx::block_chunk_spans(block)?
        .into_iter()
        .enumerate()
        .filter(|(_, (e, _))| e.type_hash == mercs2_formats::types::TYPE_HASH_LAYER)
        .map(|(index, (e, r))| Entry { index, name: e.name_hash, keys: entity_key_set(&block[r]) })
        .collect())
}

/// `block` with only the entries `keep` (entry-table indices, in order), as a block of its own: the
/// table of those entries, then their chunks. An entry in the compact form a ported level ships
/// (names as hashes, `be_to_le::ecs::is_compact`) is refused: the placement readers take the named
/// form, which Inspect inflates it to through the level's schemas.
pub fn sub_block(block: &[u8], keep: &[usize]) -> Result<Vec<u8>, String> {
    let spans = mercs2_formats::ucfx::block_chunk_spans(block)?;
    let chosen: Vec<_> = keep
        .iter()
        .map(|&i| spans.get(i).ok_or_else(|| format!("entry {i} is not in the block ({} entries)", spans.len())))
        .collect::<Result<_, _>>()?;
    if let Some((e, _)) = chosen.iter().find(|(_, r)| mercs2_formats::be_to_le::ecs::is_compact(&block[r.clone()])) {
        return Err(format!(
            "layer entry 0x{:08X} is in the compact form a ported level ships, which the Depot does not read",
            e.name_hash
        ));
    }
    let mut out = (chosen.len() as u32).to_le_bytes().to_vec();
    for (e, _) in &chosen {
        for w in [e.name_hash, e.type_hash, e.field_c, e.chunk_size] {
            out.extend_from_slice(&w.to_le_bytes());
        }
    }
    for (_, r) in &chosen {
        out.extend_from_slice(&block[r.clone()]);
    }
    Ok(out)
}

/// Read the block a contribution of `which` reads `layer` as from the game stack `wads`, as the
/// build does: an `add_placement`'s the block holding the layer of that name
/// (`GameStack::layer_by_name`), the others' the block that needle names.
pub fn read_base(wads: &[PathBuf], which: Which, layer: &str) -> Result<Base, String> {
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let inputs = match which {
        Which::Place => game
            .layer_by_name(layer)?
            .map(|(inputs, _)| inputs)
            .ok_or_else(|| format!("{layer:?} is not the name of a layer of the game stack"))?,
        Which::Edit | Which::Layer => {
            game.layer_block_for_edit(layer).ok_or_else(|| format!("no layer matching {layer:?} in the game stack"))?
        }
    };
    let places = load_placements(&inputs.block).map_err(|e| format!("{}: {e}", inputs.path))?;
    let at = places.iter().enumerate().map(|(i, p)| (p.key, i)).collect();
    let models = load_model_placements(&inputs.block).into_iter().map(|m| (m.key, m.model_hash)).collect();
    let keys = entity_key_set(&inputs.block);
    Ok(Base { path: inputs.path, block: inputs.block, places, at, models, keys })
}

/// The first key a runtime spawn uses (`Object.Remove` tests `(g & 0xF0000000) == 0x40000000` at
/// `0x5CDCB6`, `object_entity_core_code_map.md` §4.0); a template key sets bit 31.
pub const RUNTIME_KEYS: u32 = 0x4000_0000;

/// Every key the layers of the game stack place, with the block path of the first that places it,
/// and the key after the highest placed key below [`RUNTIME_KEYS`].
pub struct StackKeys {
    pub placed: HashMap<u32, String>,
    pub next: u32,
}

/// Read every layer block (every block an ASET row of type `TYPE_ID_LAYER` names) of each WAD of
/// `wads`.
pub fn read_stack_keys(wads: &[PathBuf]) -> Result<StackKeys, String> {
    let mut placed: HashMap<u32, String> = HashMap::new();
    for wad in wads {
        let mut file = std::fs::File::open(wad).map_err(|e| format!("{}: {e}", wad.display()))?;
        let size = file.metadata().map_err(|e| format!("{}: {e}", wad.display()))?.len();
        let archive = mercs2_formats::ffcs::load_ffcs_archive(&mut file, size).map_err(|e| format!("{}: {e}", wad.display()))?;
        let blocks: BTreeSet<u16> = archive
            .aset
            .iter()
            .filter(|e| e.type_id == mercs2_formats::types::TYPE_ID_LAYER)
            .map(|e| e.block_index())
            .collect();
        for b in blocks {
            let dec = mercs2_formats::sges::decompress_block(&mut file, &archive.indx, b)
                .map_err(|e| format!("{}: block {b}: {e:?}", wad.display()))?;
            let path = archive.paths.get(b as usize).cloned().unwrap_or_else(|| format!("block {b}"));
            for k in entity_key_set(&dec) {
                placed.entry(k).or_insert_with(|| path.clone());
            }
        }
    }
    let next = placed.keys().copied().filter(|&k| k < RUNTIME_KEYS).max().map_or(1, |k| k + 1);
    Ok(StackKeys { placed, next })
}

/// A world file as read: its text, whether it is on disk, and its records.
pub struct FileRead {
    pub prior: String,
    pub exists: bool,
    pub form: Result<Form, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Form {
    Entities(EntityFile),
    World(WorldDoc),
}

/// Read `text` as `which`'s file. An absent file reads as no records.
pub fn read_form(which: Which, text: Option<&str>) -> Result<Form, String> {
    match (which, text) {
        (Which::Edit, None) => Ok(Form::World(WorldDoc::default())),
        (Which::Edit, Some(t)) => qw::parse(t).map(Form::World),
        (_, None) => Ok(Form::Entities(EntityFile { single: false, records: Vec::new() })),
        (_, Some(t)) => qw::read_entities(t).map(Form::Entities),
    }
}

/// The text qm's writer makes of `form`; an `edit_world` file names its entities from `base`.
pub fn write_form(form: &Form, base: Option<&Base>) -> Result<String, String> {
    match form {
        Form::Entities(f) => qw::write_entities(f),
        Form::World(d) => {
            let base = base.ok_or("the layer is still being read")?;
            qw::write_world(d, |e| base.note(e))
        }
    }
}

/// Whether the file can be edited now: it reads, and an `edit_world` file's layer is read (its
/// writer names each entity from the layer).
pub fn editable(read: &FileRead, base: Option<&Base>) -> Result<(), String> {
    let form = read.form.as_ref().map_err(|e| e.clone())?;
    if read.exists && matches!(form, Form::World(_)) && base.is_none() {
        return Err("the layer is still being read".into());
    }
    Ok(())
}

/// The text an edit makes of the file: a new file as qm writes it; an existing one with only what
/// changed rewritten, so the rest of the author's text stays ([`mercs2_quartermaster::lossless`]).
pub fn edited_text(read: &FileRead, which: Which, form: &Form, base: Option<&Base>) -> Result<String, String> {
    if !read.exists {
        return write_form(form, base);
    }
    mercs2_quartermaster::lossless::rewrite(&read.prior, mercs2_quartermaster::Format::Yaml, form, |t| read_form(which, Some(t)), |f| {
        write_form(f, base)
    })
}

/// What a world contribution builds.
pub struct After {
    /// The block as built.
    pub block: Vec<u8>,
    pub summary: String,
    /// The placement lint's findings for this contribution (M0198); the build blocks on them.
    pub findings: Vec<String>,
    /// The entries of [`Self::block`] the viewport draws: those holding a changed or new entity.
    pub view: Vec<usize>,
    /// The keys whose placement the build changes or adds.
    pub marked: BTreeSet<u32>,
}

/// What contribution `index` of `contributions` (an item at `dir`) builds. `Err` is qm's refusal.
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution], index: usize) -> Result<After, String> {
    let mut game = mercs2_quartermaster::game::GameStack::open(wads).map_err(|e| format!("{e:?}"))?;
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let manifest = crate::depot::item_manifest(&item);
    let findings: Vec<String> = mercs2_quartermaster::lint::placement_game_checks(&manifest, dir, &mut game)
        .into_iter()
        .filter(|d| d.at == Some(index))
        .map(|d| format!("{}: {}", d.rule.code, d.message))
        .collect();
    let c = contributions.get(index).ok_or_else(|| format!("contribution {index} is not in the item"))?;
    let built = qb::world_block(c, dir, &mut game)?.ok_or_else(|| format!("{} builds no world block", c.kind()))?;
    let entries = layer_entries(&built.block)?;
    let (marked, view, summary) = match c {
        Contribution::EditWorld { .. } => {
            let before: HashMap<u32, ([f32; 3], [f32; 4])> =
                load_placements(&built.base.block)?.into_iter().map(|p| (p.key, (p.pos, p.quat))).collect();
            let model_before: HashMap<u32, u32> =
                load_model_placements(&built.base.block).into_iter().map(|m| (m.key, m.model_hash)).collect();
            let mut marked: BTreeSet<u32> = load_placements(&built.block)?
                .into_iter()
                .filter(|p| before.get(&p.key) != Some(&(p.pos, p.quat)))
                .map(|p| p.key)
                .collect();
            marked.extend(
                load_model_placements(&built.block)
                    .into_iter()
                    .filter(|m| model_before.get(&m.key) != Some(&m.model_hash))
                    .map(|m| m.key),
            );
            let view = entries.iter().filter(|e| marked.iter().any(|k| e.keys.contains(k))).map(|e| e.index).collect();
            let summary = format!(
                "{} edit(s): {} Transform and {} ModelName record(s) written, {} entities changed, in {}",
                built.edits,
                built.moved,
                built.reskinned,
                marked.len(),
                built.base.path
            );
            (marked, view, summary)
        }
        Contribution::AddPlacement { layer, .. } => {
            let hash = mercs2_quartermaster::manifest::asset_hash(layer);
            let entry = entries.iter().find(|e| e.name == hash).ok_or_else(|| format!("the built block holds no layer {layer}"))?;
            let before: HashSet<u32> =
                layer_entries(&built.base.block)?.into_iter().find(|e| e.name == hash).map(|e| e.keys).unwrap_or_default();
            let marked: BTreeSet<u32> = entry.keys.iter().copied().filter(|k| !before.contains(k)).collect();
            let summary = format!("{} entities inserted into layer {layer} of {}", marked.len(), built.base.path);
            (marked, vec![entry.index], summary)
        }
        _ => {
            let last = entries.last().ok_or("the built block holds no layer entry")?;
            let summary = format!(
                "{} entities in a new layer entry 0x{:08X} of {}",
                last.keys.len(),
                last.name,
                built.base.path
            );
            (last.keys.iter().copied().collect(), vec![last.index], summary)
        }
    };
    Ok(After { block: built.block, summary, findings, view, marked })
}

/// What the viewport draws for the open world item.
#[derive(Clone)]
pub struct View {
    /// Changes whenever what is drawn changes.
    pub id: String,
    /// What the camera is framed on: a new value frames the view afresh, the same one keeps the camera
    /// (an edit, or the shipped/built toggle, keeps it; another item or another named layer frames).
    pub frame: String,
    pub label: String,
    pub block: Rc<Vec<u8>>,
    pub marked: Rc<BTreeSet<u32>>,
    /// What the marked entities are, for the label ("changed", "the stand-in's objects").
    pub marked_as: &'static str,
    /// The selected record's entity.
    pub focus: Option<u32>,
}

/// What each world contribution builds, by item folder and index: the stamp it was computed from,
/// the build's number and the build.
type AfterCache = HashMap<(PathBuf, usize), (String, u64, Loading<After>)>;

/// Each world file read, by path: its modified time when read and what it read as.
type FileCache = HashMap<PathBuf, (Option<SystemTime>, Rc<FileRead>)>;

/// Each cell file read, by path: its modified time when read and its bytes.
type CellFileCache = HashMap<PathBuf, (Option<SystemTime>, Result<Rc<Vec<u8>>, String>)>;

/// The editors' reads and selection.
#[derive(Default)]
pub struct Worlds {
    bases: RefCell<HashMap<String, Loading<Base>>>,
    stack: RefCell<Option<Loading<StackKeys>>>,
    /// What each world contribution builds, by item folder and index, with what it was computed from
    /// and the build's number.
    after: RefCell<AfterCache>,
    /// The number the next build takes; each build has its own.
    builds: std::cell::Cell<u64>,
    files: RefCell<FileCache>,
    /// Sub-blocks drawn, by view id.
    drawn: RefCell<HashMap<String, Rc<Vec<u8>>>>,
    /// The selected record of each file.
    pub selected: RefCell<HashMap<PathBuf, usize>>,
    /// The filter of each `edit_world` file's entity list.
    pub filters: RefCell<HashMap<PathBuf, String>>,
    /// Layers read by name, for `activate_layer`.
    named: RefCell<HashMap<String, Loading<Option<NamedLayer>>>>,
    /// Which of an `activate_layer` item's layers the viewport draws, by item folder (0 is the one
    /// it adds).
    pub shown_layer: RefCell<HashMap<PathBuf, usize>>,
    /// Terrain cells and their placements, by target.
    cells: RefCell<HashMap<String, Loading<CellPlace>>>,
    /// Cell files, by path, with the modification time they were read at.
    cell_files: RefCell<CellFileCache>,
    /// Shipped cells, shared with the views that draw them, by target.
    shipped_cells: RefCell<HashMap<String, Rc<Vec<u8>>>>,
    /// Draw the shipped layer rather than the built one.
    pub shipped: std::cell::Cell<bool>,
}

impl Worlds {
    /// Forget what was read from the game (a different stack).
    pub fn forget_game(&self) {
        self.bases.borrow_mut().clear();
        *self.stack.borrow_mut() = None;
        self.after.borrow_mut().clear();
        self.drawn.borrow_mut().clear();
        self.named.borrow_mut().clear();
        self.cells.borrow_mut().clear();
        self.shipped_cells.borrow_mut().clear();
    }

    /// Terrain cell `target` as the game ships and places it: `None` while it is read.
    pub fn cell_place(&self, game: &[PathBuf], target: &str) -> Option<Result<Rc<CellPlace>, String>> {
        let mut map = self.cells.borrow_mut();
        let entry = map.entry(target.to_string()).or_insert_with(|| {
            let (wads, t) = (game.to_vec(), target.to_string());
            spawn(move || if wads.is_empty() { Err("no game archive is open".to_string()) } else { read_cell_place(&wads, &t) })
        });
        poll(entry)
    }

    /// The cell file at `abs`, read again when it changes.
    pub fn cell_file(&self, abs: &Path) -> Result<Rc<Vec<u8>>, String> {
        let mtime = std::fs::metadata(abs).and_then(|m| m.modified()).ok();
        if let Some((t, r)) = self.cell_files.borrow().get(abs) {
            if *t == mtime {
                return r.clone();
            }
        }
        let r = std::fs::read(abs).map(Rc::new).map_err(|e| format!("{}: {e}", abs.display()));
        self.cell_files.borrow_mut().insert(abs.to_path_buf(), (mtime, r.clone()));
        r
    }

    /// What the viewport draws for an open `replace_terrain_cell` item: yours, or with
    /// [`Self::shipped`] the shipped cell, at the cell's placement. `None` while it is read, or when
    /// the cell to draw does not read.
    pub fn terrain_view(&self, game: &[PathBuf], item: &Item) -> Option<TerrainView> {
        let (index, target, file) = terrain(item)?;
        let place = self.cell_place(game, target)?.ok()?;
        let shipped = self.shipped.get();
        let (container, which) = if shipped {
            let c = self.shipped_cells.borrow_mut().entry(target.to_string()).or_insert_with(|| Rc::new(place.shipped.clone())).clone();
            (c, "shipped".to_string())
        } else {
            let abs = item.dir.join(file);
            let bytes = self.cell_file(&abs).ok()?;
            cell_facts(&bytes).ok()?;
            let stamp = std::fs::metadata(&abs).and_then(|m| m.modified()).ok();
            (bytes, format!("yours {stamp:?}"))
        };
        let hash = mercs2_quartermaster::manifest::asset_hash(target);
        Some(TerrainView {
            id: format!("{}#{index}|{which}", item.dir.display()),
            hash,
            label: format!("{target} ({})", if shipped { "shipped" } else { "yours" }),
            pos: place.pos,
            parent: place.parent,
            container,
        })
    }

    /// The layer named `name`: `None` while it is read; `Ok(None)` when the stack has no layer of
    /// that name.
    pub fn named_layer(&self, game: &[PathBuf], name: &str) -> Option<Result<Rc<Option<NamedLayer>>, String>> {
        let mut map = self.named.borrow_mut();
        let entry = map.entry(name.to_string()).or_insert_with(|| {
            let (wads, n) = (game.to_vec(), name.to_string());
            spawn(move || if wads.is_empty() { Err("no game archive is open".to_string()) } else { read_named_layer(&wads, &n) })
        });
        poll(entry)
    }

    /// Which of `item`'s activation layers the viewport draws, within the `n` it names.
    fn shown_of(&self, item: &Item, n: usize) -> usize {
        self.shown_layer.borrow().get(&item.dir).copied().unwrap_or(0).min(n.saturating_sub(1))
    }

    /// The viewport's view of an `activate_layer` item: the layer chosen of those it names.
    fn activation_view(&self, game: &[PathBuf], item: &Item, (index, names): (usize, Vec<String>)) -> Option<View> {
        let i = self.shown_of(item, names.len());
        let read = self.named_layer(game, &names[i])?.ok()?;
        let layer = read.as_ref().as_ref()?;
        let id = format!("{}#{index}|activate|{}|{}", item.dir.display(), names[i], layer.path);
        let block = {
            let mut drawn = self.drawn.borrow_mut();
            drawn.entry(id.clone()).or_insert_with(|| Rc::new(layer.block.clone())).clone()
        };
        let what = if i == 0 { "added" } else { "removed" };
        let frame = format!("{}#{}", item.dir.display(), names[i]);
        Some(View { id, frame, label: format!("{} ({what})", names[i]), block, marked: Rc::new(BTreeSet::new()), marked_as: "", focus: None })
    }

    /// The block a contribution of `which` reads `layer` as ([`read_base`]): `None` while it is read.
    pub fn base(&self, game: &[PathBuf], which: Which, layer: &str) -> Option<Result<Rc<Base>, String>> {
        let mut map = self.bases.borrow_mut();
        let by_name = if which == Which::Place { "name" } else { "needle" };
        let entry = map.entry(format!("{by_name}:{layer}")).or_insert_with(|| {
            let (wads, n) = (game.to_vec(), layer.to_string());
            spawn(move || if wads.is_empty() { Err("no game archive is open".to_string()) } else { read_base(&wads, which, &n) })
        });
        poll(entry)
    }

    /// Every key the stack's layers place: `None` while they are read.
    pub fn stack_keys(&self, game: &[PathBuf]) -> Option<Result<Rc<StackKeys>, String>> {
        let mut slot = self.stack.borrow_mut();
        let entry = slot.get_or_insert_with(|| {
            let wads = game.to_vec();
            spawn(move || if wads.is_empty() { Err("no game archive is open".to_string()) } else { read_stack_keys(&wads) })
        });
        poll(entry)
    }

    /// What contribution `index` of `item` builds: `None` while it is computed. Recomputed when the
    /// item's contributions or any of its world files change.
    pub fn after(&self, game: &[PathBuf], item: &Item, index: usize) -> Option<Result<Rc<After>, String>> {
        self.numbered_after(game, item, index).map(|(_, r)| r)
    }

    /// [`Self::after`], with the number of the build it is.
    fn numbered_after(&self, game: &[PathBuf], item: &Item, index: usize) -> Option<(u64, Result<Rc<After>, String>)> {
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        let mut stamp = format!("{contributions:?}");
        for c in &contributions {
            if let Some(s) = of(c) {
                let t = std::fs::metadata(item.dir.join(s.file)).and_then(|m| m.modified()).ok();
                stamp.push_str(&format!("|{t:?}"));
            }
        }
        let key = (item.dir.clone(), index);
        let mut map = self.after.borrow_mut();
        if map.get(&key).is_none_or(|(s, _, _)| *s != stamp) {
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            let job = spawn(move || {
                if wads.is_empty() {
                    Err("no game archive is open".to_string())
                } else {
                    after(&wads, &dir, &contributions, index)
                }
            });
            let n = self.builds.get();
            self.builds.set(n + 1);
            map.insert(key.clone(), (stamp, n, job));
        }
        let (_, n, entry) = map.get_mut(&key)?;
        Some((*n, poll(entry)?))
    }

    /// The file at `abs`, read as `which`.
    pub fn file(&self, abs: &Path, which: Which) -> Rc<FileRead> {
        let mtime = std::fs::metadata(abs).and_then(|m| m.modified()).ok();
        if let Some((t, r)) = self.files.borrow().get(abs) {
            let same_kind = matches!((&r.form, which), (Ok(Form::World(_)), Which::Edit) | (Ok(Form::Entities(_)), Which::Place | Which::Layer) | (Err(_), _));
            if *t == mtime && same_kind {
                return r.clone();
            }
        }
        let r = Rc::new(match mtime {
            None => FileRead { prior: String::new(), exists: false, form: read_form(which, None) },
            Some(_) => match std::fs::read_to_string(abs) {
                Ok(text) => {
                    let form = read_form(which, Some(&text)).map_err(|e| format!("{}: {e}", abs.display()));
                    FileRead { prior: text, exists: true, form }
                }
                Err(e) => FileRead { prior: String::new(), exists: true, form: Err(format!("{}: {e}", abs.display())) },
            },
        });
        self.files.borrow_mut().insert(abs.to_path_buf(), (mtime, r.clone()));
        r
    }

    /// What a click on entity `key` in the viewport of `item` does: `Ok(row)`, the record of the
    /// item's first world file that places or edits it; or, for an `edit_world` entity no edit names,
    /// `Err` with the entity list's filter that finds it. With the file's path; `None` when the file
    /// or the layer is not read.
    pub fn pick(&self, game: &[PathBuf], item: &Item, key: u32) -> Option<(PathBuf, Result<usize, String>)> {
        let (_, sub) = first(item)?;
        let abs = item.dir.join(sub.file);
        let read = self.file(&abs, sub.which);
        let row = match read.form.as_ref().ok()? {
            Form::Entities(f) => Ok(f.records.iter().position(|r| r.key == key)?),
            Form::World(d) => {
                let base = self.base(game, sub.which, sub.layer)?.ok()?;
                d.edits.iter().position(|e| base.key_of(&e.entity) == Some(key)).ok_or_else(|| format!("0x{key:08x}"))
            }
        };
        Some((abs, row))
    }

    fn selected_of(&self, abs: &Path) -> Option<usize> {
        self.selected.borrow().get(abs).copied()
    }

    /// What the viewport draws for `item`'s first world contribution: the built block's changed
    /// entries, or with [`Self::shipped`] the same entries as the game ships them. `None` while
    /// nothing is read, or when there is nothing to draw.
    pub fn view(&self, game: &[PathBuf], item: &Item) -> Option<View> {
        if let Some(a) = activation(item).filter(|_| first(item).is_none()) {
            return self.activation_view(game, item, a);
        }
        let (index, sub) = first(item)?;
        let abs = item.dir.join(sub.file);
        let (build, after) = self.numbered_after(game, item, index)?;
        let after = after.ok()?;
        let base = self.base(game, sub.which, sub.layer).and_then(Result::ok);
        let shipped = self.shipped.get() && sub.which == Which::Edit;
        let (source, keep): (&[u8], &[usize]) = match (&base, shipped) {
            (Some(b), true) => (&b.block, &after.view),
            _ => (&after.block, &after.view),
        };
        if keep.is_empty() {
            return None;
        }
        let id = format!("{}#{index}|{shipped}|build {build}|{keep:?}", item.dir.display());
        let block = {
            let mut drawn = self.drawn.borrow_mut();
            match drawn.get(&id) {
                Some(b) => b.clone(),
                None => {
                    let b = Rc::new(sub_block(source, keep).ok()?);
                    let own = format!("{}#", item.dir.display());
                    drawn.retain(|k, _| !k.starts_with(&own));
                    drawn.insert(id.clone(), b.clone());
                    b
                }
            }
        };
        let read = self.file(&abs, sub.which);
        let focus = self.selected_of(&abs).and_then(|i| match read.form.as_ref().ok()? {
            Form::Entities(f) => f.records.get(i).map(|r| r.key),
            Form::World(d) => d.edits.get(i).and_then(|e| base.as_ref().and_then(|b| b.key_of(&e.entity))),
        });
        let label = if shipped { format!("{} (shipped)", sub.layer) } else { format!("{} (after qm build)", sub.layer) };
        Some(View { id, frame: item.dir.display().to_string(), label, block, marked: Rc::new(after.marked.clone()), marked_as: "changed", focus })
    }
}

/// What the editor asks the page to do.
pub enum Ask {
    /// Write `text` over `path`, whose content was `prior` when the edit was made.
    Write { path: PathBuf, prior: String, text: String },
    Select { path: PathBuf, row: usize },
    Shipped(bool),
    /// Draw layer `index` of the `activate_layer` item at `item` (0 is the one it adds).
    ShowLayer { item: PathBuf, index: usize },
    /// An edit qm's writer refused, for the status line.
    Refused(String),
}

/// The layers the item's own `add_layer`s ship, by name hash: a name the stack lacks but one of these
/// ships is not M0194's.
fn shipped_here(item: &Item) -> HashSet<u32> {
    item.file
        .as_ref()
        .map(|f| {
            f.contributions
                .iter()
                .filter_map(|c| match c {
                    Contribution::AddLayer { name, .. } => Some(mercs2_quartermaster::manifest::asset_hash(name)),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// What a name the game stack has no layer of is: shipped by the item's own `add_layer`, or M0194.
fn missing_text(name: &str, ships: &HashSet<u32>) -> (FieldState, String) {
    if ships.contains(&mercs2_quartermaster::manifest::asset_hash(name)) {
        (FieldState::Neutral, "the game stack has no layer of this name; this item's add_layer ships it".into())
    } else {
        (FieldState::Bad, "M0194: no layer of the game stack has this name".into())
    }
}

fn missing_note(ui: &mut egui::Ui, name: &str, ships: &HashSet<u32>) {
    let (st, text) = missing_text(name, ships);
    theme::field_note(ui, st, &text);
}

/// The inspector card of a `replace_terrain_cell` item: where the game places the cell, the shipped
/// cell and yours side by side, and which the viewport draws.
pub fn terrain_cards(ui: &mut egui::Ui, w: &Worlds, item: &Item, game: &[PathBuf]) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some((_, target, file)) = terrain(item) else { return asks };
    let yours = w.cell_file(&item.dir.join(file)).and_then(|b| cell_facts(&b));
    theme::card(ui, "Terrain cell", Some(target), |ui| {
        let place = w.cell_place(game, target);
        match &place {
            None => waiting(ui, &format!("Reading {target}\u{2026}")),
            Some(Err(e)) => theme::field_note(ui, FieldState::Bad, e),
            Some(Ok(p)) => {
                theme::kv(ui, "placed by", mono(format!("0x{:08X} in layer 0x{:08X}", p.key, p.layer)));
                theme::kv(ui, "at", mono(fmt3(p.pos)));
            }
        }
        let shipped = place.and_then(Result::ok).map(|p| cell_facts(&p.shipped));
        ui.add_space(4.0);
        egui::Grid::new(("depot-cell-facts", item.dir.clone())).num_columns(3).spacing([12.0, 2.0]).show(ui, |ui| {
            ui.label("");
            ui.label(egui::RichText::new("SHIPPED").small().color(theme::FAINT));
            ui.label(egui::RichText::new("YOURS").small().color(theme::FAINT));
            ui.end_row();
            for (k, what) in ["patches", "draw groups", "vertices", "materials"].into_iter().enumerate() {
                ui.label(egui::RichText::new(what).color(theme::DIM));
                let cell = |f: &Option<Result<[usize; 4], String>>| match f {
                    Some(Ok(v)) => mono(v[k].to_string()),
                    Some(Err(_)) => mono("\u{2014}").color(theme::BAD),
                    None => mono("\u{2026}").color(theme::DIM),
                };
                ui.label(cell(&shipped));
                ui.label(cell(&Some(yours.clone())));
                ui.end_row();
            }
        });
        if let Err(e) = &yours {
            theme::field_note(ui, FieldState::Bad, e);
        }
        if let Some(Err(e)) = &shipped {
            theme::field_note(ui, FieldState::Bad, &format!("shipped: {e}"));
        }
        ui.horizontal(|ui| {
            theme::field_label(ui, "Viewport");
            let on = w.shipped.get();
            if theme::pill(ui, "Yours", !on).clicked() && on {
                asks.push(Ask::Shipped(false));
            }
            if theme::pill(ui, "Shipped", on).clicked() && !on {
                asks.push(Ask::Shipped(true));
            }
        });
    });
    asks
}

/// The inspector card of an `activate_layer` item: each layer it names, as the game stack carries
/// it, with the one the viewport draws chosen.
pub fn activation_cards(ui: &mut egui::Ui, w: &Worlds, item: &Item, game: &[PathBuf]) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some((_, names)) = activation(item) else { return asks };
    let shown = w.shown_of(item, names.len());
    let ships = shipped_here(item);
    theme::card(ui, "Layers", Some(&format!("{}", names.len())), |ui| {
        for (i, name) in names.iter().enumerate() {
            ui.push_id(i, |ui| {
                ui.horizontal(|ui| {
                    let what = if i == 0 { "ADDED" } else { "REMOVED" };
                    ui.label(theme::disp_text(what, theme::style_eyebrow(), if i == 0 { theme::GOOD } else { theme::DIM }));
                    ui.add(egui::Label::new(mono(name.as_str())).truncate());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::pill(ui, "Show", i == shown).clicked() && i != shown {
                            asks.push(Ask::ShowLayer { item: item.dir.clone(), index: i });
                        }
                    });
                });
                match w.named_layer(game, name) {
                    None => waiting(ui, "Reading\u{2026}"),
                    Some(Err(e)) => theme::field_note(ui, FieldState::Bad, &e),
                    Some(Ok(l)) => match l.as_ref() {
                        None => missing_note(ui, name, &ships),
                        Some(l) => {
                            ui.label(egui::RichText::new(format!("{} \u{b7} {} entities", l.path, l.entities)).small().color(theme::DIM));
                        }
                    },
                }
                ui.add_space(4.0);
            });
        }
    });
    asks
}

/// Degrees about Y, X and Z (yaw, pitch, roll) of a unit quaternion `[x, y, z, w]`.
pub fn euler_deg(q: [f32; 4]) -> [f32; 3] {
    let (y, x, z) = Quat::from_array(q).to_euler(EulerRot::YXZ);
    [y.to_degrees(), x.to_degrees(), z.to_degrees()]
}

/// The unit quaternion of yaw, pitch and roll in degrees.
pub fn quat_deg(e: [f32; 3]) -> [f32; 4] {
    Quat::from_euler(EulerRot::YXZ, e[0].to_radians(), e[1].to_radians(), e[2].to_radians()).to_array()
}

/// A value edited in `body`, held while the pointer is down or a field has focus, and handed back
/// once it settles changed.
fn settle<T: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    id: egui::Id,
    stored: &T,
    body: impl FnOnce(&mut egui::Ui, &mut T),
) -> Option<T> {
    let mut work: T =
        ui.data(|d| d.get_temp::<(T, T)>(id)).filter(|(s, _)| s == stored).map(|(_, w)| w).unwrap_or_else(|| stored.clone());
    body(ui, &mut work);
    let idle = !ui.input(|i| i.pointer.any_down()) && ui.ctx().memory(|m| m.focused().is_none());
    if idle && work != *stored {
        ui.data_mut(|d| d.insert_temp(id, (stored.clone(), stored.clone())));
        return Some(work);
    }
    ui.data_mut(|d| d.insert_temp(id, (stored.clone(), work)));
    None
}

/// The farthest world coordinate as a cell shows it.
const FARTHEST: &str = "-3900.000";

/// The narrowest row three X/Y/Z cells of [`FARTHEST`] fit across, in the faces `ui` draws with
/// (`vec3_rows_fit_the_column`).
fn vec3_row(ui: &egui::Ui) -> f32 {
    theme::vec3_cells_width(ui, FARTHEST)
}

/// A caption, then X, Y and Z: across one row where the column is [`vec3_row`] wide, else one row
/// per axis. Returns whether a value changed.
fn vec3_rows(ui: &mut egui::Ui, caption: &str, v: &mut [f32; 3], speed: f32) -> bool {
    theme::field_label(ui, caption);
    if ui.available_width() >= vec3_row(ui) {
        return theme::vec3_field(ui, "", v, speed);
    }
    let mut changed = false;
    for (i, axis) in ["X", "Y", "Z"].into_iter().enumerate() {
        changed |= theme::scalar_field(ui, axis, &mut v[i], speed);
    }
    changed
}

fn mono(s: impl Into<String>) -> egui::RichText {
    egui::RichText::new(s.into()).monospace()
}

fn fmt3(v: [f32; 3]) -> String {
    format!("{:.3}, {:.3}, {:.3}", v[0], v[1], v[2])
}

fn waiting(ui: &mut egui::Ui, what: &str) {
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(egui::RichText::new(what).small().color(theme::DIM));
    });
}

/// The inspector cards of world contribution `index` of `item`: its records, the selected record's
/// form, the layer's entities to pick from, and what the build makes.
pub fn cards(ui: &mut egui::Ui, w: &Worlds, item: &Item, index: usize, game: &[PathBuf], s: &Sources) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return asks };
    let Some(sub) = of(c) else { return asks };
    let abs = item.dir.join(sub.file);
    let base = w.base(game, sub.which, sub.layer);
    let base_ok = base.as_ref().and_then(|r| r.as_ref().ok()).cloned();
    let read = w.file(&abs, sub.which);
    let can = editable(&read, base_ok.as_deref());
    let title = if sub.which == Which::Edit { "Edits" } else { "Entities" };
    let badge = sub.file.display().to_string();
    let mut form = read.form.as_ref().ok().cloned();
    theme::card(ui, title, Some(&badge), |ui| {
        match &base {
            None => waiting(ui, &format!("Reading {}\u{2026}", sub.layer)),
            Some(Err(e)) => theme::field_note(ui, FieldState::Bad, e),
            Some(Ok(b)) => {
                ui.label(egui::RichText::new(format!("{} \u{b7} {} entities", b.path, b.places.len())).small().color(theme::DIM));
            }
        }
        if let Err(e) = &read.form {
            theme::field_note(ui, FieldState::Bad, e);
            return;
        }
        if let Err(e) = &can {
            theme::field_note(ui, if base_ok.is_none() && sub.which == Which::Edit { FieldState::Neutral } else { FieldState::Warn }, e);
        }
        if let Some(f) = &form {
            records(ui, w, &abs, f, base_ok.as_deref(), &mut asks);
        }
    });
    let editable_now = can.is_ok();
    let write = |f: &Form, asks: &mut Vec<Ask>| match edited_text(&read, sub.which, f, base_ok.as_deref()) {
        Ok(text) => asks.push(Ask::Write { path: abs.clone(), prior: read.prior.clone(), text }),
        Err(e) => asks.push(Ask::Refused(format!("{}: {e}; it is left as it was", abs.display()))),
    };
    if let Some(f) = form.as_mut() {
        let sel = w.selected_of(&abs);
        match f {
            Form::World(d) => {
                if let Some(i) = sel.filter(|&i| i < d.edits.len()) {
                    theme::card(ui, "Edit", Some(&d.edits[i].entity), |ui| {
                        ui.add_enabled_ui(editable_now, |ui| {
                            if let Some(next) = edit_form(ui, (&abs, i), d, base_ok.as_deref(), s) {
                                write(&Form::World(next), &mut asks);
                            }
                        });
                    });
                }
                if let Some(b) = &base_ok {
                    theme::card(ui, "Layer entities", Some(&format!("{}", b.places.len())), |ui| {
                        if let Some(key) = entity_picker(ui, (w, &abs), b, d) {
                            let mut next = d.clone();
                            next.edits.push(EntityEdit { entity: format!("0x{key:08X}"), pos: None, quat: None, model: None });
                            if editable_now {
                                asks.push(Ask::Select { path: abs.clone(), row: next.edits.len() - 1 });
                                write(&Form::World(next), &mut asks);
                            }
                        }
                    });
                }
            }
            Form::Entities(f) => {
                let stack = w.stack_keys(game);
                let stack_ok = stack.as_ref().and_then(|r| r.as_ref().ok()).cloned();
                if let Some(i) = sel.filter(|&i| i < f.records.len()) {
                    theme::card(ui, "Entity", Some(&format!("0x{:08X}", f.records[i].key)), |ui| {
                        ui.add_enabled_ui(editable_now, |ui| {
                            let ctx = KeyCtx { stack: stack_ok.as_deref() };
                            if let Some(next) = record_form(ui, (&abs, i), f, ctx, s) {
                                write(&Form::Entities(next), &mut asks);
                            }
                        });
                    });
                }
                ui.horizontal(|ui| {
                    let ready = editable_now && stack_ok.is_some();
                    if theme::primary_button(ui, "+ Add entity", ready).clicked() {
                        if let Some(sk) = &stack_ok {
                            let mut next = f.clone();
                            next.records.push(new_record(f, base_ok.as_deref(), sk));
                            next.single = false;
                            asks.push(Ask::Select { path: abs.clone(), row: next.records.len() - 1 });
                            write(&Form::Entities(next), &mut asks);
                        }
                    }
                    match &stack {
                        None => waiting(ui, "Reading the keys the game places\u{2026}"),
                        Some(Err(e)) => {
                            ui.label(egui::RichText::new(e).small().color(theme::BAD));
                        }
                        Some(Ok(_)) => {}
                    }
                });
            }
        }
    }
    after_card(ui, w, item, index, game, sub.which, &mut asks);
    asks
}

/// The file's records: one line each, the selected one highlighted; a click selects.
fn records(ui: &mut egui::Ui, w: &Worlds, abs: &Path, f: &Form, base: Option<&Base>, asks: &mut Vec<Ask>) {
    let n = match f {
        Form::World(d) => d.edits.len(),
        Form::Entities(e) => e.records.len(),
    };
    if n == 0 {
        ui.label(egui::RichText::new("No records yet.").color(theme::DIM));
        return;
    }
    let sel = w.selected_of(abs);
    let row_h = 20.0;
    egui::ScrollArea::vertical().id_salt(("depot-world-rows", abs.to_path_buf())).max_height(220.0).auto_shrink([false, true]).show_rows(
        ui,
        row_h,
        n,
        |ui, range| {
            for i in range {
                let text = match f {
                    Form::World(d) => {
                        let e = &d.edits[i];
                        let who = base.and_then(|b| b.key_of(&e.entity).map(|k| b.label(k))).unwrap_or_else(|| e.entity.clone());
                        let what: Vec<&str> = [e.pos.map(|_| "move"), e.quat.map(|_| "turn"), e.model.as_ref().map(|_| "re-model")]
                            .into_iter()
                            .flatten()
                            .collect();
                        format!("{who}   {}", if what.is_empty() { "no change".to_string() } else { what.join(" \u{b7} ") })
                    }
                    Form::Entities(e) => {
                        let r = &e.records[i];
                        format!("0x{:08X}   {}", r.key, r.model)
                    }
                };
                let resp = ui.add_sized(
                    [ui.available_width(), row_h - 2.0],
                    egui::Button::selectable(sel == Some(i), mono(text).color(if sel == Some(i) { theme::BRASS } else { theme::TX })),
                );
                if resp.clicked() {
                    asks.push(Ask::Select { path: abs.to_path_buf(), row: i });
                }
            }
        },
    );
}

/// The selected `edit_world` edit's form: the entity's shipped placement, then a move, a turn and a
/// re-model, each added or removed on its own; and Remove. Returns the file with the edit changed.
fn edit_form(ui: &mut egui::Ui, (abs, i): (&Path, usize), d: &WorldDoc, base: Option<&Base>, s: &Sources) -> Option<WorldDoc> {
    let e = &d.edits[i];
    let id = egui::Id::unique(("depot-world-edit", abs.to_path_buf(), i));
    let shipped = base.and_then(|b| b.key_of(&e.entity).and_then(|k| b.at.get(&k).map(|&j| (k, &b.places[j]))));
    let model_name = |h: u32| s.names.and_then(|n| n.reverse(h)).map(str::to_string).unwrap_or_else(|| format!("0x{h:08X}"));
    match (shipped, base) {
        (Some((k, p)), Some(b)) => {
            theme::kv(ui, "entity", mono(b.label(k)));
            theme::kv(ui, "shipped position", mono(fmt3(p.pos)));
            theme::kv(ui, "shipped rotation \u{b0}", mono(fmt3(euler_deg(p.quat))));
            let m = b.models.get(&k).map_or("\u{2014}".to_string(), |&h| model_name(h));
            theme::kv(ui, "shipped model", mono(m));
        }
        (None, Some(_)) => theme::field_note(ui, FieldState::Bad, &format!("{:?} is neither a key nor a name of an entity this layer places", e.entity)),
        _ => {}
    }
    let shipped_pos = shipped.map_or([0.0; 3], |(_, p)| p.pos);
    let shipped_quat = shipped.map_or([0.0, 0.0, 0.0, 1.0], |(_, p)| p.quat);
    let shipped_model = shipped.and_then(|(k, _)| base?.models.get(&k)).map(|&h| model_name(h));
    let mut next = e.clone();
    let mut changed = false;
    ui.add_space(4.0);

    let mut pos = e.pos;
    changed |= crate::kindform::optional(ui, "Position", &mut pos, || shipped_pos, |ui, p| {
        match settle(ui, id.with("pos"), p, |ui, v| {
            vec3_rows(ui, "Position", v, 0.05);
        }) {
            Some(v) => {
                *p = v;
                true
            }
            None => false,
        }
    });
    next.pos = pos;

    let mut quat = e.quat;
    changed |= crate::kindform::optional(ui, "Rotation", &mut quat, || shipped_quat, |ui, q| {
        match settle(ui, id.with("rot"), &euler_deg(*q), |ui, v| {
            vec3_rows(ui, "Yaw, pitch, roll \u{b0}", v, 0.5);
        }) {
            Some(v) => {
                *q = quat_deg(v);
                true
            }
            None => false,
        }
    });
    next.quat = quat;

    let mut model = e.model.clone();
    changed |= crate::kindform::optional(
        ui,
        "Model",
        &mut model,
        || shipped_model.clone().unwrap_or_default(),
        |ui, m| crate::kindform::pick(ui, "Model", m, List::Models, s, true, "a model name or 0x\u{2026}"),
    );
    next.model = model;

    ui.add_space(4.0);
    let mut out = d.clone();
    if ui.small_button("Remove edit").clicked() {
        out.edits.remove(i);
        return Some(out);
    }
    if changed && next != *e {
        out.edits[i] = next;
        return Some(out);
    }
    None
}

/// The layer's entities, filtered; Edit adds an edit of one. Returns the key picked.
fn entity_picker(ui: &mut egui::Ui, (w, abs): (&Worlds, &Path), b: &Base, d: &WorldDoc) -> Option<u32> {
    let mut q: String = w.filters.borrow().get(abs).cloned().unwrap_or_default();
    ui.add(egui::TextEdit::singleline(&mut q).hint_text("Filter by key or name").margin(theme::TEXT_EDIT_MARGIN).desired_width(f32::INFINITY));
    let edited: HashSet<u32> = d.edits.iter().filter_map(|e| b.key_of(&e.entity)).collect();
    let ql = q.to_lowercase();
    let hits: Vec<&Placement> = b
        .places
        .iter()
        .filter(|p| ql.is_empty() || format!("0x{:08x}", p.key).contains(&ql) || p.name.as_deref().is_some_and(|n| n.to_lowercase().contains(&ql)))
        .collect();
    ui.label(egui::RichText::new(format!("{} shown", hits.len())).small().color(theme::DIM));
    let mut picked = None;
    let row_h = 20.0;
    egui::ScrollArea::vertical().id_salt(("depot-world-picker", abs.to_path_buf())).max_height(240.0).auto_shrink([false, true]).show_rows(
        ui,
        row_h,
        hits.len(),
        |ui, range| {
            for p in &hits[range] {
                ui.horizontal(|ui| {
                    let has = edited.contains(&p.key);
                    if ui.add_enabled(!has, egui::Button::new("Edit").small()).clicked() {
                        picked = Some(p.key);
                    }
                    ui.add(egui::Label::new(mono(b.label(p.key)).color(if has { theme::DIM } else { theme::TX })).truncate());
                });
            }
        },
    );
    w.filters.borrow_mut().insert(abs.to_path_buf(), q);
    picked
}

/// What a record's key is checked against.
#[derive(Clone, Copy)]
struct KeyCtx<'a> {
    stack: Option<&'a StackKeys>,
}

/// What `key` (record `i` of `f`) is, as notes: M0198's file and game cases, and which key space it
/// is in.
fn key_notes(key: u32, i: usize, f: &EntityFile, ctx: KeyCtx) -> Vec<(FieldState, String)> {
    let mut out = Vec::new();
    if let Some(j) = f.records.iter().enumerate().position(|(j, r)| j != i && r.key == key) {
        out.push((FieldState::Bad, format!("M0198: record {} has this key too", j + 1)));
    }
    if let Some(path) = ctx.stack.and_then(|s| s.placed.get(&key)) {
        out.push((FieldState::Bad, format!("M0198: {path} places this key")));
    }
    if key & 0x8000_0000 != 0 {
        out.push((FieldState::Warn, "bit 31 set: the template key space".to_string()));
    } else if key & 0xF000_0000 == RUNTIME_KEYS {
        out.push((FieldState::Warn, "0x4xxxxxxx: the runtime spawn key space".to_string()));
    }
    out
}

/// The selected entity record's form: its key, model, position, orientation and name; and Remove
/// (not for the file's only record: an entity file lists at least one). Returns the file changed.
fn record_form(ui: &mut egui::Ui, (abs, i): (&Path, usize), f: &EntityFile, ctx: KeyCtx, s: &Sources) -> Option<EntityFile> {
    let r = &f.records[i];
    let id = egui::Id::unique(("depot-world-record", abs.to_path_buf(), i));
    let mut next = r.clone();
    let mut changed = false;

    let key_text = format!("0x{:08X}", r.key);
    let mut typed: String =
        ui.data(|d| d.get_temp::<(String, String)>(id.with("key"))).filter(|(o, _)| *o == key_text).map(|(_, t)| t).unwrap_or_else(|| key_text.clone());
    let parsed = bare_hash(&typed);
    let resp = theme::text_field(ui, "Key", &mut typed, "0x\u{2026}", if parsed.is_some() { FieldState::Neutral } else { FieldState::Bad });
    if parsed.is_none() {
        theme::field_note(ui, FieldState::Bad, "a key is a bare 0x and up to 8 hex digits");
    }
    for (st, note) in key_notes(parsed.unwrap_or(r.key), i, f, ctx) {
        theme::field_note(ui, st, &note);
    }
    if resp.lost_focus() {
        if let Some(k) = parsed.filter(|&k| k != r.key) {
            next.key = k;
            changed = true;
        }
    }
    ui.data_mut(|d| d.insert_temp(id.with("key"), (key_text, typed)));

    let mut model = r.model.clone();
    if crate::kindform::pick(ui, "Model", &mut model, List::Models, s, true, "a model name or 0x\u{2026}") && model != r.model {
        next.model = model;
        changed = true;
    }

    if let Some(v) = settle(ui, id.with("pos"), &r.pos, |ui, v| {
        vec3_rows(ui, "Position", v, 0.05);
    }) {
        next.pos = v;
        changed = true;
    }

    theme::field_label(ui, "Orientation");
    ui.horizontal(|ui| {
        for (label, on) in [("None", r.quat.is_none() && r.yaw.is_none()), ("Yaw", r.yaw.is_some()), ("Rotation", r.quat.is_some())] {
            if theme::pill(ui, label, on).clicked() && !on {
                let q = r.quat.unwrap_or_else(|| r.yaw.map_or([0.0, 0.0, 0.0, 1.0], |y| quat_deg([y, 0.0, 0.0])));
                (next.yaw, next.quat) = match label {
                    "Yaw" => (Some(euler_deg(q)[0]), None),
                    "Rotation" => (None, Some(q)),
                    _ => (None, None),
                };
                changed = true;
            }
        }
    });
    if let Some(y) = r.yaw {
        if let Some(v) = settle(ui, id.with("yaw"), &y, |ui, v| {
            theme::scalar_field(ui, "Yaw \u{b0}", v, 0.5);
        }) {
            next.yaw = Some(v);
            changed = true;
        }
    }
    if let Some(q) = r.quat {
        if let Some(v) = settle(ui, id.with("rot"), &euler_deg(q), |ui, v| {
            vec3_rows(ui, "Yaw, pitch, roll \u{b0}", v, 0.5);
        }) {
            next.quat = Some(quat_deg(v));
            changed = true;
        }
    }
    if r.quat.is_some() && r.yaw.is_some() {
        theme::field_note(ui, FieldState::Bad, "both `quat` and `yaw`: qm refuses the record; choose one");
    }

    let mut name = r.name.clone();
    if crate::kindform::optional(ui, "Name", &mut name, || format!("modent_{:08x}", r.key), |ui, n| {
        theme::text_field(ui, "Name", n, "", if n.is_empty() { FieldState::Bad } else { FieldState::Neutral }).lost_focus()
    }) {
        next.name = name;
        changed = true;
    }
    if r.name.is_none() {
        theme::field_note(ui, FieldState::Neutral, &format!("the build names it modent_{:08x}", r.key));
    }

    ui.add_space(4.0);
    let mut out = f.clone();
    let only = f.records.len() == 1;
    if ui
        .add_enabled(!only, egui::Button::new("Remove entity").small())
        .on_disabled_hover_text("an entity file lists at least one entity")
        .clicked()
    {
        out.records.remove(i);
        if out.records.len() == 1 && f.single {
            out.single = true;
        }
        return Some(out);
    }
    if changed && next != *r {
        out.records[i] = next;
        return Some(out);
    }
    None
}

/// A new record: the key after the highest the game stack and the file place, the last record's model
/// and position (or the layer's first entity's position), unrotated and unnamed.
fn new_record(f: &EntityFile, base: Option<&Base>, stack: &StackKeys) -> EntityRecord {
    let in_file: HashSet<u32> = f.records.iter().map(|r| r.key).collect();
    let mut key = f.records.iter().map(|r| r.key).filter(|&k| k < RUNTIME_KEYS).max().map_or(stack.next, |k| (k + 1).max(stack.next));
    while in_file.contains(&key) || stack.placed.contains_key(&key) || base.is_some_and(|b| b.keys.contains(&key)) {
        key += 1;
    }
    let last = f.records.last();
    EntityRecord {
        key,
        model: last.map(|r| r.model.clone()).unwrap_or_default(),
        pos: last.map(|r| r.pos).or_else(|| base.and_then(|b| b.places.first()).map(|p| p.pos)).unwrap_or([0.0; 3]),
        quat: None,
        yaw: None,
        name: None,
    }
}

/// What the build makes of the contribution, and which layer the viewport draws.
fn after_card(ui: &mut egui::Ui, w: &Worlds, item: &Item, index: usize, game: &[PathBuf], which: Which, asks: &mut Vec<Ask>) {
    theme::card(ui, "After qm build", None, |ui| {
        match w.after(game, item, index) {
            None => waiting(ui, "Building the layer\u{2026}"),
            Some(Err(e)) => theme::field_note(ui, FieldState::Bad, &format!("qm build: {e}")),
            Some(Ok(a)) => {
                ui.label(egui::RichText::new(&a.summary).small().color(theme::TX));
                for f in &a.findings {
                    theme::field_note(ui, FieldState::Bad, f);
                }
            }
        }
        if which == Which::Edit {
            ui.horizontal(|ui| {
                theme::field_label(ui, "Viewport");
                let shipped = w.shipped.get();
                if theme::pill(ui, "After qm build", !shipped).clicked() && shipped {
                    asks.push(Ask::Shipped(false));
                }
                if theme::pill(ui, "Shipped", shipped).clicked() && !shipped {
                    asks.push(Ask::Shipped(true));
                }
            });
        }
    });
}

/// Over the viewport while the open world item has no layer drawn: that it is building, or qm's
/// refusal.
pub fn viewport_note(ctx: &egui::Context, w: &Worlds, game: &[PathBuf], item: &Item, rect: egui::Rect) {
    if w.view(game, item).is_some() || w.terrain_view(game, item).is_some() {
        return;
    }
    let (text, colour) = match (first(item), activation(item)) {
        (Some((index, _)), _) => match w.after(game, item, index) {
            None => ("Building the layer\u{2026}".to_string(), theme::DIM),
            Some(Err(e)) => (format!("qm build: {e}"), theme::BAD),
            Some(Ok(a)) => match sub_block(&a.block, &a.view) {
                Err(e) => (e, theme::BAD),
                Ok(_) => return,
            },
        },
        (None, Some((_, names))) => {
            let name = &names[w.shown_of(item, names.len())];
            match w.named_layer(game, name) {
                None => (format!("Reading {name}\u{2026}"), theme::DIM),
                Some(Err(e)) => (e, theme::BAD),
                Some(Ok(l)) if l.is_none() => {
                    let (st, text) = missing_text(name, &shipped_here(item));
                    (format!("{name}: {text}"), if st == FieldState::Bad { theme::BAD } else { theme::DIM })
                }
                Some(Ok(_)) => return,
            }
        }
        (None, None) => {
            let Some((_, target, file)) = terrain(item) else { return };
            match w.cell_place(game, target) {
                None => (format!("Reading {target}\u{2026}"), theme::DIM),
                Some(Err(e)) => (e, theme::BAD),
                Some(Ok(_)) => match w.cell_file(&item.dir.join(file)).and_then(|b| cell_facts(&b)) {
                    Err(e) => (e, theme::BAD),
                    Ok(_) => return,
                },
            }
        }
    };
    egui::Area::new(egui::Id::unique("depot_world_note"))
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

/// Over the viewport while a terrain cell is drawn: which, and where.
pub fn terrain_label(ctx: &egui::Context, v: &TerrainView, rect: egui::Rect) {
    egui::Area::new(egui::Id::unique("depot_terrain_label"))
        .fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 34.0))
        .interactable(false)
        .show(ctx, |ui| {
            ui.label(mono(format!("{} \u{b7} 0x{:08X} \u{b7} at {}", v.label, v.hash, fmt3(v.pos))).color(theme::DIM));
        });
}

/// The camera that frames the entities `marked` names in `view` (all of them when it names none):
/// target and distance, as [`crate::layerview::LayerView::framing`] frames a layer.
pub fn framing(view: &crate::layerview::LayerView, marked: &BTreeSet<u32>) -> (glam::Vec3, f32) {
    let pts: Vec<glam::Vec3> = view.entities.iter().filter(|e| marked.contains(&e.key)).map(|e| glam::Vec3::from(e.pos)).collect();
    if pts.is_empty() {
        return view.framing();
    }
    let (min, max) = pts.iter().fold((glam::Vec3::splat(f32::MAX), glam::Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let radius = ((max - min).length() * 0.5).max(5.0);
    ((min + max) * 0.5, (radius * 2.4).clamp(5.0, 15000.0))
}

/// Over the viewport: a ring on each entity the build changes or adds, a larger one on the selected.
pub fn marks(ctx: &egui::Context, view: &crate::layerview::LayerView, v: &View, vp: glam::Mat4, rect: egui::Rect) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("depot_world_marks"))).with_clip_rect(rect);
    let size = ctx.content_rect().size();
    for e in &view.entities {
        let focus = v.focus == Some(e.key);
        if !focus && !v.marked.contains(&e.key) {
            continue;
        }
        if let Some((p, _)) = crate::layerview::project(vp, glam::Vec3::from(e.pos), size) {
            let (r, col) = if focus { (11.0, theme::BRASS) } else { (7.0, theme::BRASS_DK) };
            painter.circle_stroke(p, r, egui::Stroke::new(2.0, col));
        }
    }
    egui::Area::new(egui::Id::unique("depot_world_label"))
        .fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 34.0))
        .interactable(false)
        .show(ctx, |ui| {
            let present = view.entities.iter().filter(|e| v.marked.contains(&e.key)).count();
            let changed = if v.marked.is_empty() { String::new() } else { format!(" \u{b7} {present} {}", v.marked_as) };
            ui.label(mono(format!("{} \u{b7} {} entities{changed}", v.label, view.entities.len())).color(theme::DIM));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(prior: &str, which: Which) -> FileRead {
        FileRead { prior: prior.to_string(), exists: true, form: read_form(which, Some(prior)) }
    }

    /// A file that reads is editable, however it is written; an absent file is editable and reads
    /// as no records; an `edit_world` file waits for its layer.
    #[test]
    fn a_file_that_reads_is_editable() {
        let ours = "- key: \"0x00000001\"\n  model: a\n  pos: [0.0, 0.0, 0.0]\n";
        assert!(editable(&read(ours, Which::Place), None).is_ok());
        let theirs = "- key: \"0x00000001\"\n  model: a\n  pos: [0, 0, 0]   # by hand\n";
        assert!(editable(&read(theirs, Which::Place), None).is_ok());
        let none = FileRead { prior: String::new(), exists: false, form: read_form(Which::Layer, None) };
        assert!(editable(&none, None).is_ok());
        assert_eq!(none.form.unwrap(), Form::Entities(EntityFile { single: false, records: Vec::new() }));
        let world = read(&format!("{}\nedits: []\n", qw::WORLD_HEADER), Which::Edit);
        assert!(editable(&world, None).unwrap_err().contains("still being read"));
    }

    /// A hand-written entity file edited from the form: the edited value changes, an added record
    /// takes qm's text, and the comments and spelling around them stay.
    #[test]
    fn an_edit_to_a_hand_written_file_keeps_its_text() {
        let theirs = "# barrels by the dock\n- key: \"0x00000001\"\n  model: a\n  pos: [0, 0, 0]   # by hand\n";
        let r = read(theirs, Which::Place);
        let Ok(Form::Entities(mut f)) = r.form.clone() else { panic!("{:?}", r.form.as_ref().err()) };
        f.records[0].pos[1] = -13.18;
        let mut added = f.records[0].clone();
        added.key = 2;
        f.records.push(added);
        let text = edited_text(&r, Which::Place, &Form::Entities(f.clone()), None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            text,
            "# barrels by the dock\n- key: \"0x00000001\"\n  model: a\n  pos: [0, -13.18, 0]   # by hand\n- key: \"0x00000002\"\n  model: a\n  pos: [0.0, -13.18, 0.0]\n"
        );
        assert_eq!(read_form(Which::Place, Some(&text)).unwrap(), Form::Entities(f));
    }

    /// Yaw, pitch and roll round-trip through the quaternion, and a yaw alone is the record's `yaw`.
    #[test]
    fn euler_angles_round_trip_and_a_yaw_is_a_turn_about_y() {
        let e = [30.0, -10.0, 5.0];
        let back = euler_deg(quat_deg(e));
        for k in 0..3 {
            assert!((back[k] - e[k]).abs() < 1e-3, "{back:?}");
        }
        let q = quat_deg([90.0, 0.0, 0.0]);
        let h = 45f32.to_radians();
        assert!((q[1] - h.sin()).abs() < 1e-6 && (q[3] - h.cos()).abs() < 1e-6 && q[0].abs() < 1e-6 && q[2].abs() < 1e-6, "{q:?}");
    }

    /// A new record's key is past every key the stack, the layer and the file place, below the
    /// runtime key space.
    #[test]
    fn a_new_record_takes_a_key_nothing_places() {
        let stack = StackKeys { placed: [(0x10, "a".to_string()), (0x12, "b".to_string())].into(), next: 0x11 };
        let f = EntityFile { single: false, records: Vec::new() };
        assert_eq!(new_record(&f, None, &stack).key, 0x11);
        let f = EntityFile { single: false, records: vec![EntityRecord { key: 0x11, model: "m".into(), pos: [1.0, 2.0, 3.0], quat: None, yaw: None, name: None }] };
        let r = new_record(&f, None, &stack);
        assert_eq!((r.key, r.model.as_str(), r.pos), (0x13, "m", [1.0, 2.0, 3.0]));
    }

    /// A key two records share and a key a layer of the stack places are M0198; the template and
    /// runtime key spaces are warned.
    #[test]
    fn key_notes_name_m0198_and_the_key_spaces() {
        let f = EntityFile {
            single: false,
            records: vec![
                EntityRecord { key: 5, model: "m".into(), pos: [0.0; 3], quat: None, yaw: None, name: None },
                EntityRecord { key: 5, model: "m".into(), pos: [0.0; 3], quat: None, yaw: None, name: None },
            ],
        };
        let stack = StackKeys { placed: [(7, "layers_static".to_string())].into(), next: 8 };
        let ctx = KeyCtx { stack: Some(&stack) };
        assert!(key_notes(5, 0, &f, ctx).iter().any(|(s, n)| *s == FieldState::Bad && n.contains("record 2")));
        assert!(key_notes(7, 0, &f, ctx).iter().any(|(s, n)| *s == FieldState::Bad && n.starts_with("M0198") && n.contains("layers_static")));
        assert!(key_notes(0x8000_0001, 0, &f, ctx).iter().any(|(_, n)| n.contains("template")));
        assert!(key_notes(0x4000_0001, 0, &f, ctx).iter().any(|(_, n)| n.contains("runtime")));
    }

    /// How wide `f` draws in a column `w` wide.
    fn drawn_width(w: f32, mut f: impl FnMut(&mut egui::Ui)) -> f32 {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 2000.0))), ..Default::default() };
        let mut out = 0.0;
        crate::gui::run_headless(&ctx, input, |ui| {
            out = ui.allocate_ui_with_layout(egui::vec2(w, 2000.0), egui::Layout::top_down(egui::Align::Min), |ui| f(ui)).response.rect.width();
        });
        out
    }

    /// A form under test: its name and what draws it.
    type FormPart<'a> = (&'a str, Box<dyn Fn(&mut egui::Ui) + 'a>);

    /// Every row of the two record forms fits a card of the inspector at its narrowest (a 300 px
    /// panel: a 253 px card) and at its default width (a 324 px card).
    #[test]
    fn the_record_forms_fit_the_inspector() {
        let s = Sources { index: None, corpus: &[], names: None, levels: &[] };
        let d = WorldDoc {
            edits: vec![EntityEdit { entity: "0x000B4E38".into(), pos: Some([-3086.907, 105.746, -2615.777]), quat: Some([0.0, 0.70710677, 0.0, 0.70710677]), model: Some("pmc_hum_mattias".into()) }],
        };
        let f = EntityFile {
            single: false,
            records: vec![EntityRecord { key: 0x7E00_0001, model: "global_barrela".into(), pos: [-3095.0, 105.746, -2610.0], quat: None, yaw: Some(45.0), name: None }],
        };
        let unset = WorldDoc { edits: vec![EntityEdit { entity: "0x000B4E38".into(), pos: None, quat: None, model: None }] };
        let parts: Vec<FormPart<'_>> = vec![
            ("edit form", Box::new(|ui: &mut egui::Ui| {
                edit_form(ui, (Path::new("/w.yaml"), 0), &d, None, &s);
            })),
            ("edit form, nothing set", Box::new(|ui: &mut egui::Ui| {
                edit_form(ui, (Path::new("/w.yaml"), 0), &unset, None, &s);
            })),
            ("record form", Box::new(|ui: &mut egui::Ui| {
                let ctx = KeyCtx { stack: None };
                record_form(ui, (Path::new("/e.yaml"), 0), &f, ctx, &s);
            })),
        ];
        for w in [253.0, 324.0] {
            let wide: Vec<String> =
                parts.iter().map(|(n, p)| (n, drawn_width(w, p))).filter(|(_, x)| *x > w + 0.5).map(|(n, x)| format!("{n}: {x}")).collect();
            assert!(wide.is_empty(), "wider than {w}: {wide:?}");
        }
    }

    /// [`vec3_row`] holds three cells of the farthest world coordinate, and a narrower column takes
    /// one row per axis.
    #[test]
    fn vec3_rows_fit_the_column() {
        let mut row = 0.0;
        drawn_width(1200.0, |ui| row = vec3_row(ui));
        let mut far = [-3900.0f32, -103.0, -3900.0];
        let across = drawn_width(row, |ui| {
            theme::vec3_field(ui, "", &mut far, 0.05);
        });
        assert!(across <= row, "{across} > {row}");
        for w in [200.0, row - 1.0] {
            let x = drawn_width(w, |ui| {
                vec3_rows(ui, "Position", &mut far, 0.05);
            });
            assert!(x <= w + 0.5, "{w}: {x}");
        }
    }

    /// A click on an entity an entity file places selects its record; on any other, nothing.
    #[test]
    fn a_click_selects_the_record_that_places_the_entity() {
        let dir = std::env::temp_dir().join(format!("depotworld-pick-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/e.yaml"), "- key: \"0x7E000001\"\n  model: a\n  pos: [0.0, 0.0, 0.0]\n- key: \"0x7E000002\"\n  model: a\n  pos: [1.0, 0.0, 0.0]\n").unwrap();
        let contributions = vec![Contribution::AddLayer { name: "l".into(), template: "layers_static".into(), entities: "src/e.yaml".into() }];
        let item = Item {
            name: "pick".into(),
            dir: dir.clone(),
            folder: String::new(),
            file: Ok(crate::depot::ItemFile { state: crate::depot::State::Draft, contributions }),
            text: None,
            modified: None,
            checks: crate::depot::Checks::default(),
        };
        let w = Worlds::default();
        assert_eq!(w.pick(&[], &item, 0x7E00_0002), Some((dir.join("src/e.yaml"), Ok(1))));
        assert_eq!(w.pick(&[], &item, 0x1234), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A block of chosen entries holds those entries' chunks, under a table of just them.
    #[test]
    fn a_sub_block_keeps_only_the_chosen_entries() {
        let mut block = 3u32.to_le_bytes().to_vec();
        for (name, size) in [(1u32, 2u32), (2, 3), (3, 1)] {
            for w in [name, mercs2_formats::types::TYPE_HASH_LAYER, 0, size] {
                block.extend_from_slice(&w.to_le_bytes());
            }
        }
        block.extend_from_slice(&[0xA, 0xA, 0xB, 0xB, 0xB, 0xC]);
        let sub = sub_block(&block, &[2, 0]).unwrap();
        let spans = mercs2_formats::ucfx::block_chunk_spans(&sub).unwrap();
        assert_eq!(spans.iter().map(|(e, _)| e.name_hash).collect::<Vec<_>>(), vec![3, 1]);
        assert_eq!(&sub[spans[0].1.clone()], &[0xC]);
        assert_eq!(&sub[spans[1].1.clone()], &[0xA, 0xA]);
        assert!(sub_block(&block, &[3]).is_err());
    }

    /// An entry in the compact form (CHDR's names-as-hashes flag) is refused, naming it.
    #[test]
    fn a_compact_layer_entry_is_refused() {
        // UCFX, data area at 40, one row: CHDR at 0, 8 bytes; its flags word has names-as-hashes set.
        let mut c = b"UCFX".to_vec();
        for w in [40u32, 0, 0, 1] {
            c.extend_from_slice(&w.to_le_bytes());
        }
        c.extend_from_slice(b"CHDR");
        for w in [0u32, 8, 0, 0] {
            c.extend_from_slice(&w.to_le_bytes());
        }
        c.extend_from_slice(&0u32.to_le_bytes());
        c.extend_from_slice(&mercs2_formats::worldentity::CHDR_NAMES_AS_HASHES.to_le_bytes());
        assert!(mercs2_formats::be_to_le::ecs::is_compact(&c));
        let mut block = 1u32.to_le_bytes().to_vec();
        for w in [0xABCDu32, mercs2_formats::types::TYPE_HASH_LAYER, 0, c.len() as u32] {
            block.extend_from_slice(&w.to_le_bytes());
        }
        block.extend_from_slice(&c);
        let e = sub_block(&block, &[0]).unwrap_err();
        assert!(e.contains("0x0000ABCD") && e.contains("compact form"), "{e}");
    }

    /// A name the stack lacks is M0194 unless the item's own `add_layer` ships it.
    #[test]
    fn a_missing_layer_is_m0194_unless_the_item_ships_it() {
        let ships: HashSet<u32> = [mercs2_quartermaster::manifest::asset_hash("vz_state_mine")].into();
        assert!(missing_text("VZ_State_Mine", &ships).0 == FieldState::Neutral);
        let (st, text) = missing_text("vz_state_other", &ships);
        assert!(st == FieldState::Bad && text.starts_with("M0194"), "{text}");
    }

    #[cfg(feature = "retail")]
    fn retail_wads() -> Vec<PathBuf> {
        vec![mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))]
    }

    /// Against retail: an `edit_world` that moves one entity builds through qm into a block whose
    /// drawn entries hold that entity at its new position, and only it is marked; LayerView reads the
    /// drawn entries.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_edit_builds_and_its_changed_entry_is_drawn() {
        let wads = retail_wads();
        let base = read_base(&wads, Which::Edit, "vz_state_pmccon004").unwrap_or_else(|e| panic!("{e}"));
        let target = base.places[0].clone();
        let dir = std::env::temp_dir().join(format!("depotworld-edit-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let moved = [target.pos[0] + 12.5, target.pos[1], target.pos[2]];
        let doc = WorldDoc { edits: vec![EntityEdit { entity: format!("0x{:08X}", target.key), pos: Some(moved), quat: None, model: None }] };
        std::fs::write(dir.join("src/world.yaml"), write_form(&Form::World(doc), Some(&base)).unwrap()).unwrap();
        let c = vec![Contribution::EditWorld { layer: "vz_state_pmccon004".into(), edits: "src/world.yaml".into() }];
        let a = after(&wads, &dir, &c, 0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.marked, BTreeSet::from([target.key]), "{}", a.summary);
        assert!(a.findings.is_empty(), "{:?}", a.findings);
        let drawn = sub_block(&a.block, &a.view).unwrap();
        let placed = load_placements(&drawn).unwrap();
        assert_eq!(placed.iter().find(|p| p.key == target.key).map(|p| p.pos), Some(moved));
        let mut stack = crate::app::WadStack::open(wads[0].to_str().expect("a UTF-8 path"), &[]).unwrap_or_else(|e| panic!("{e}"));
        let t = crate::layerview::Templates::read(&mut stack).unwrap_or_else(|e| panic!("{e}"));
        let v = crate::layerview::LayerView::read(0, String::new(), 0, &drawn, &t).unwrap_or_else(|e| panic!("{e}"));
        assert!(v.entities.iter().any(|e| e.key == target.key && e.stored_pos == moved));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Against retail: a layer read by name is the entry Inspect reads for it (`vz_state_pmccon004`'s
    /// 361 entities, `layerview::the_reference_layer_reads_every_entity`); a name no layer carries
    /// reads as none.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_named_layer_draws_as_inspect_reads_it() {
        let wads = retail_wads();
        let l = read_named_layer(&wads, "vz_state_pmccon004").unwrap_or_else(|e| panic!("{e}")).expect("the layer");
        assert_eq!(l.entities, 361, "{}", l.path);
        let mut stack = crate::app::WadStack::open(wads[0].to_str().expect("a UTF-8 path"), &[]).unwrap_or_else(|e| panic!("{e}"));
        let t = crate::layerview::Templates::read(&mut stack).unwrap_or_else(|e| panic!("{e}"));
        let v = crate::layerview::LayerView::read(0, String::new(), 0, &l.block, &t).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(v.entities.len(), 361);
        assert!(read_named_layer(&wads, "vz_state_no_such_layer_depot").unwrap().is_none());
    }

    /// Against retail: a terrain cell named by its bare hash is found where a `TerrainObject`
    /// placement puts it, and the shipped cell reads.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_terrain_cell_is_found_where_the_game_places_it() {
        let wads = retail_wads();
        let mut game = mercs2_quartermaster::game::GameStack::open(&wads).unwrap_or_else(|e| panic!("{e:?}"));
        let tile = game
            .layer_containers()
            .unwrap_or_else(|e| panic!("{e}"))
            .values()
            .find_map(|c| mercs2_formats::placement::load_terrain_tiles(c).ok()?.into_iter().next())
            .expect("a layer places a terrain cell");
        let target = format!("0x{:08X}", tile.terrainmesh_hash);
        let place = read_cell_place(&wads, &target).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((place.key, place.pos), (tile.key, tile.pos));
        let [patches, groups, verts, _] = cell_facts(&place.shipped).unwrap_or_else(|e| panic!("{e}"));
        assert!(patches > 0 && groups > 0 && verts > 0);
        eprintln!("terrain cell {target}: tile 0x{:08X} at {:?}, {patches} patches, {verts} vertices", tile.key, tile.pos);
        assert!(read_cell_place(&wads, "0x00000001").err().is_some_and(|e| e.contains("no terrain cell")));
    }

    /// What `view` gives once its build is done, polled as the page polls it each frame.
    #[cfg(feature = "retail")]
    fn settled_view(w: &Worlds, wads: &[PathBuf], item: &Item) -> View {
        let start = std::time::Instant::now();
        loop {
            if let Some(v) = w.view(wads, item) {
                return v;
            }
            if let Some(Err(e)) = w.after(wads, item, 0) {
                panic!("{e}");
            }
            assert!(start.elapsed() < std::time::Duration::from_secs(120), "no view after 120 s");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Against retail, the edit loop: an edit written over the item's file makes a new view, whose
    /// drawn block holds the entity where the second edit put it.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_edit_written_again_draws_the_new_position() {
        let wads = retail_wads();
        let base = read_base(&wads, Which::Edit, "vz_state_pmccon004").unwrap_or_else(|e| panic!("{e}"));
        let target = base.places[0].clone();
        let dir = std::env::temp_dir().join(format!("depotworld-loop-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let file = dir.join("src/world.yaml");
        let contributions = vec![Contribution::EditWorld { layer: "vz_state_pmccon004".into(), edits: "src/world.yaml".into() }];
        let item = Item {
            name: "loop".into(),
            dir: dir.clone(),
            folder: String::new(),
            file: Ok(crate::depot::ItemFile { state: crate::depot::State::Draft, contributions }),
            text: None,
            modified: None,
            checks: crate::depot::Checks::default(),
        };
        let write = |x: f32| {
            let doc = WorldDoc { edits: vec![EntityEdit { entity: format!("0x{:08X}", target.key), pos: Some([x, target.pos[1], target.pos[2]]), quat: None, model: None }] };
            std::fs::write(&file, write_form(&Form::World(doc), Some(&base)).unwrap()).unwrap();
        };
        let drawn_x = |v: &View| load_placements(&v.block).unwrap().into_iter().find(|p| p.key == target.key).map(|p| p.pos[0]);
        let w = Worlds::default();
        write(target.pos[0] + 10.0);
        let first = settled_view(&w, &wads, &item);
        assert_eq!(drawn_x(&first), Some(target.pos[0] + 10.0));
        write(target.pos[0] + 20.0);
        let second = settled_view(&w, &wads, &item);
        assert_ne!(second.id, first.id);
        assert_eq!(drawn_x(&second), Some(target.pos[0] + 20.0));
        // A click on the edited entity selects its edit; on another, filters the entity list to it.
        assert_eq!(w.pick(&wads, &item, target.key), Some((file.clone(), Ok(0))));
        let other = base.places[1].key;
        assert_eq!(w.pick(&wads, &item, other), Some((file.clone(), Err(format!("0x{other:08x}")))));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Against retail: an `add_layer` builds through qm into a new last entry holding exactly the
    /// file's keys, which is what the viewport draws.
    #[cfg(feature = "retail")]
    #[test]
    fn a_retail_new_layer_is_its_own_drawn_entry() {
        let wads = retail_wads();
        let stack = read_stack_keys(&wads).unwrap_or_else(|e| panic!("{e}"));
        assert!(stack.placed.len() >= 101_299, "{} keys", stack.placed.len());
        assert!(!stack.placed.contains_key(&stack.next) && stack.next < RUNTIME_KEYS);
        let f = EntityFile {
            single: false,
            records: vec![
                EntityRecord { key: stack.next, model: "global_barrela".into(), pos: [10.0, 2.0, 30.0], quat: None, yaw: Some(90.0), name: None },
                EntityRecord { key: stack.next + 1, model: "global_barrela".into(), pos: [12.0, 2.0, 30.0], quat: None, yaw: None, name: Some("second".into()) },
            ],
        };
        let dir = std::env::temp_dir().join(format!("depotworld-layer-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/e.yaml"), write_form(&Form::Entities(f), None).unwrap()).unwrap();
        let c = vec![Contribution::AddLayer { name: "vz_state_depot_test".into(), template: "layers_static".into(), entities: "src/e.yaml".into() }];
        let a = after(&wads, &dir, &c, 0).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(a.marked, BTreeSet::from([stack.next, stack.next + 1]), "{}", a.summary);
        let entries = layer_entries(&a.block).unwrap();
        assert_eq!(a.view, vec![entries.last().unwrap().index]);
        assert_eq!(entries.last().unwrap().name, mercs2_quartermaster::manifest::asset_hash("vz_state_depot_test"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
