//! The texture editors' preview: the game's texture, the author's PNG and the texture as `qm build`
//! ships it, one at a time in a viewer with channel and mip choices, as Unreal's texture editor
//! shows one.
//!
//! "As built" is decoded from the block qm's build ships for the contribution (`build::texture_block`,
//! which the lowering calls); a sprite, from the `vfx` atlas the fx merge draws it into
//! (`fx::merge_shipment`, which the lint's fx checks call), its place marked. "Shipped" is the game's
//! texture at its full streamed resolution (`wad::extract_texture_hires`, as Inspect reads it). The
//! encode error is measured between the PNG, as the build swizzles a normal map, and the built
//! texture's finest level.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use mercs2_formats::hash::pandemic_hash_m2 as m2;
use mercs2_formats::patch_wad::PatchBlock;
use mercs2_formats::texture::{parse_texture_container, TexFormat, TextureData};
use mercs2_formats::types::TYPE_HASH_TEXTURE;
use mercs2_quartermaster::game::GameStack;
use mercs2_quartermaster::manifest::{asset_hash, Contribution};

use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};
use crate::texpng::Level;

/// Whether `c` is a texture kind.
pub fn of(c: &Contribution) -> bool {
    matches!(c, Contribution::AddTexture { .. } | Contribution::ReplaceTexture { .. } | Contribution::AddFxSprite { .. })
}

/// The ids the viewer's uploads are kept by: one per plate read, across every build.
static PLATE_IDS: AtomicU32 = AtomicU32::new(1);

/// What a plate is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Shipped,
    Png,
    Built,
}

impl Side {
    fn label(self) -> &'static str {
        match self {
            Side::Shipped => "Shipped",
            Side::Png => "Your PNG",
            Side::Built => "As built",
        }
    }
}

/// One image the viewer shows.
pub struct Plate {
    pub id: u32,
    pub side: Side,
    /// Its facts: size, format, levels, bytes.
    pub facts: String,
    /// Its levels, finest first.
    pub levels: Vec<Level>,
    /// Rectangles to mark, in level-0 texels: `(label, x, y, width, height)`.
    pub marks: Vec<(String, u32, u32, u32, u32)>,
}

impl Plate {
    fn new(side: Side, facts: String, levels: Vec<Level>, marks: Vec<(String, u32, u32, u32, u32)>) -> Plate {
        Plate { id: PLATE_IDS.fetch_add(1, Ordering::Relaxed), side, facts, levels, marks }
    }

    /// A texture's plate: every level it holds, with its size, format and level count.
    fn texture(side: Side, td: &TextureData, extra: &str) -> Plate {
        let levels = crate::texpng::decode_levels(td);
        let finest = levels.first().map(|l| (l.width, l.height)).unwrap_or((0, 0));
        let mut facts = format!("{}×{} · {} · {} level(s)", td.width, td.height, format_name(td.format), levels.len());
        if finest != (td.width, td.height) {
            facts.push_str(&format!(" · finest held {}×{}", finest.0, finest.1));
        }
        facts.push_str(&format!(" · {} B", commas(td.all_mips.len())));
        facts.push_str(extra);
        Plate::new(side, facts, levels, Vec::new())
    }
}

fn format_name(f: TexFormat) -> &'static str {
    match f {
        TexFormat::Bc1 => "DXT1",
        TexFormat::Bc3 => "DXT5",
    }
}

fn commas(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What one contribution shows.
#[derive(Default)]
pub struct View {
    pub plates: Vec<Plate>,
    /// A `replace_texture`'s target and the texture the build ships for it: what the still render
    /// puts on the models that wear it.
    pub replaced: Option<(u32, std::sync::Arc<TextureData>)>,
    /// The encode error between the PNG and the built texture, per channel.
    pub error: Option<String>,
    /// What could not be read or built.
    pub problems: Vec<String>,
}

/// One block the build ships, and what it holds.
pub struct Ship {
    pub path: String,
    pub holds: Vec<String>,
}

/// What an item's texture contributions show and build.
pub struct ItemTex {
    pub views: BTreeMap<usize, View>,
    pub ships: Vec<Ship>,
    pub log: Vec<String>,
}

/// The texture a block carries under `hash`.
fn block_texture(block: &PatchBlock, hash: u32) -> Result<TextureData, String> {
    let raw = mercs2_formats::sges::decompress_sges(&block.compressed_data).map_err(|e| format!("{}: {e}", block.path_string))?;
    let (parsed, issues) = mercs2_formats::ucfx::walk_decompressed_block(&raw, &block.path_string);
    if let Some(i) = issues.first() {
        return Err(format!("{}: {}", i.context, i.detail));
    }
    let at = parsed
        .entries
        .iter()
        .position(|e| e.name_hash == hash && e.type_hash == TYPE_HASH_TEXTURE)
        .ok_or_else(|| format!("{} carries no texture 0x{hash:08X}", block.path_string))?;
    parse_texture_container(&parsed.containers[at]).map_err(|e| format!("{}: {e}", block.path_string))
}

/// The game's texture `hash` at its full streamed resolution, from the WAD last in `wads` that has it.
fn shipped_texture(wads: &[PathBuf], hash: u32) -> Result<TextureData, String> {
    let mut last = format!("no game WAD has texture 0x{hash:08X}");
    for p in wads.iter().rev() {
        let mut w = mercs2_engine::wad::open(&p.to_string_lossy())?;
        match mercs2_engine::wad::extract_texture_hires(&mut w, hash) {
            Ok(t) => return Ok(t),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// An RGBA8 image of a PNG, as the build reads it.
fn png_level(path: &Path) -> Result<Level, String> {
    let img = mercs2_quartermaster::sprite::read_png(path)?;
    let rgba = img.rgba.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    Ok(Level { level: 0, width: img.width as u32, height: img.height as u32, rgba })
}

/// The PNG as the build swizzles a normal map: `R=255, G=y, B=255, A=x` (`build_texture_from_rgba`).
fn swizzled_normal(l: &Level) -> Level {
    let rgba = l.rgba.as_chunks::<4>().0.iter().flat_map(|p| [255, p[1], 255, p[0]]).collect();
    Level { level: 0, width: l.width, height: l.height, rgba }
}

/// The mean and largest difference of each channel between two images of one size; `None` when the
/// sizes differ. A DXT1 texture carries no alpha, so its alpha is left out.
fn encode_error(png: &Level, built: &Level, alpha: bool) -> Option<String> {
    if (png.width, png.height) != (built.width, built.height) {
        return None;
    }
    let channels = if alpha { 4 } else { 3 };
    let n = (png.width * png.height) as usize;
    let mut parts = Vec::new();
    for (c, name) in ["R", "G", "B", "A"].iter().enumerate().take(channels) {
        let (mut sum, mut max) = (0u64, 0u8);
        for i in 0..n {
            let d = png.rgba[i * 4 + c].abs_diff(built.rgba[i * 4 + c]);
            sum += u64::from(d);
            max = max.max(d);
        }
        parts.push(format!("{name} {:.2} / {max}", sum as f64 / n as f64));
    }
    Some(format!("encode error, mean / largest per channel: {}", parts.join(" · ")))
}

/// What `contributions` (an item at `dir`) show and build, against the game whose `vz.wad` is the
/// first of `wads` (none open: what needs no game).
pub fn after(wads: &[PathBuf], dir: &Path, contributions: &[Contribution]) -> Result<ItemTex, String> {
    let item = crate::depot::ItemFile { state: crate::depot::State::Draft, contributions: contributions.to_vec() };
    let mut manifest = crate::depot::item_manifest(&item);
    manifest.shipment.name = crate::depotsound::SHIPMENT.to_string();
    let mut game = match wads.first() {
        Some(vz) => {
            let paths = mercs2_quartermaster::compat::game_stack_paths(vz, [&manifest])?;
            Some(GameStack::open(&paths).map_err(|e| e.to_string())?)
        }
        None => None,
    };
    let mut views = BTreeMap::new();
    let mut ships = Vec::new();
    let mut log = Vec::new();
    // The fx merge, once for the item's sprites and atlas repaint.
    let mut merged: Option<Result<(mercs2_quartermaster::sprite::Atlas, mercs2_quartermaster::fx::Merged), String>> = None;

    for (index, c) in contributions.iter().enumerate() {
        if !of(c) {
            continue;
        }
        let mut v = View::default();
        let image = match c {
            Contribution::AddTexture { image, .. } | Contribution::ReplaceTexture { image, .. } | Contribution::AddFxSprite { image, .. } => image,
            _ => continue,
        };
        let png = png_level(&dir.join(image));
        let atlas = matches!(c, Contribution::AddFxSprite { .. }) || mercs2_quartermaster::fx::repaints_atlas(c);

        if atlas {
            let m = merged.get_or_insert_with(|| {
                let Some(g) = game.as_mut() else { return Err("the vfx atlas is read from the game, and no game archive is open".into()) };
                let (base, merge) = mercs2_quartermaster::fx::merge_shipment(&manifest, dir, g)?;
                let merge = merge.map_err(|f| f.to_string())?;
                Ok((base, merge))
            });
            match m {
                Err(e) => v.problems.push(e.clone()),
                Ok((base, merge)) => {
                    let base_td = atlas_texture(base);
                    v.plates.push(Plate::texture(Side::Shipped, &base_td, " · the vfx atlas the sprites are drawn into"));
                    match &merge.atlas {
                        None => v.problems.push("the merge draws no atlas: the set adds no sprite and repaints none".into()),
                        Some(container) => match parse_texture_container(container) {
                            Err(e) => v.problems.push(format!("the merged atlas: {e}")),
                            Ok(td) => {
                                let mut plate = Plate::texture(Side::Built, &td, " · in the resident block");
                                let names: HashMap<u32, &str> = contributions
                                    .iter()
                                    .filter_map(|c| match c {
                                        Contribution::AddFxSprite { name, .. } => Some((m2(name), name.as_str())),
                                        _ => None,
                                    })
                                    .collect();
                                plate.marks = merge
                                    .placed
                                    .iter()
                                    .map(|p| {
                                        let label = names.get(&p.key).map_or_else(|| format!("0x{:08X}", p.key), |n| n.to_string());
                                        (label, p.x as u32, p.y as u32, p.width as u32, p.height as u32)
                                    })
                                    .collect();
                                if let Some(sq) = merge.square {
                                    plate.facts.push_str(&format!(" · sprites in the free square {sq}"));
                                }
                                v.plates.push(plate);
                            }
                        },
                    }
                    log.extend(merge.log.iter().cloned());
                }
            }
            match png {
                Ok(l) => v.plates.insert(1.min(v.plates.len()), Plate::new(Side::Png, format!("{}×{} · PNG", l.width, l.height), vec![l], Vec::new())),
                Err(e) => v.problems.push(e),
            }
            views.insert(index, v);
            continue;
        }

        if let Contribution::ReplaceTexture { target, .. } = c {
            match shipped_texture(wads, asset_hash(target)) {
                Ok(td) => v.plates.push(Plate::texture(Side::Shipped, &td, "")),
                Err(e) => v.problems.push(format!("{target}: {e}")),
            }
        }
        let png = match png {
            Ok(l) => {
                v.plates.push(Plate::new(Side::Png, format!("{}×{} · PNG", l.width, l.height), vec![Level { rgba: l.rgba.clone(), ..l }], Vec::new()));
                Some(l)
            }
            Err(e) => {
                v.problems.push(e);
                None
            }
        };
        match mercs2_quartermaster::build::texture_block(index, c, dir, game.as_mut(), &mut log) {
            Err(e) => v.problems.push(e.to_string()),
            Ok(None) => v.problems.push("the build ships no texture for it".into()),
            Ok(Some((block, hash))) => match block_texture(&block, hash) {
                Err(e) => v.problems.push(e),
                Ok(td) => {
                    let plate = Plate::texture(Side::Built, &td, "");
                    let normal = matches!(c, Contribution::AddTexture { normal_map: true, .. });
                    if let (Some(png), Some(built)) = (&png, plate.levels.first()) {
                        let compared = if normal { swizzled_normal(png) } else { Level { rgba: png.rgba.clone(), ..*png } };
                        v.error = encode_error(&compared, built, td.format == TexFormat::Bc3).map(|e| {
                            if normal {
                                format!("{e} (against the PNG as the build swizzles a normal map: R=1 G=y B=1 A=x)")
                            } else {
                                e
                            }
                        });
                    }
                    ships.push(Ship {
                        path: block.path_string.clone(),
                        holds: vec![format!("texture 0x{hash:08X}, {}×{} {}, {} level(s)", td.width, td.height, format_name(td.format), plate.levels.len())],
                    });
                    v.plates.push(plate);
                    if matches!(c, Contribution::ReplaceTexture { .. }) {
                        v.replaced = Some((hash, std::sync::Arc::new(td)));
                    }
                }
            },
        }
        views.insert(index, v);
    }
    Ok(ItemTex { views, ships, log })
}

/// The atlas as a texture: its whole DXT5 chain.
pub(crate) fn atlas_texture(a: &mercs2_quartermaster::sprite::Atlas) -> TextureData {
    let level0 = a.size.div_ceil(4) * a.size.div_ceil(4) * 16;
    TextureData {
        width: a.size as u32,
        height: a.size as u32,
        format: TexFormat::Bc3,
        mip0: a.body[..level0.min(a.body.len())].to_vec(),
        all_mips: a.body.clone(),
        mip_count: a.mips as u32,
    }
}

/// The models of a WAD that wear each texture: texture hash → `(model hash, the materials of its
/// `MTRL` that cite it)`, by model. Every block's entry table is read, so every model container at
/// every LOD rung is reached, as `mercs2_probe`'s `tex_users` reads them. A block or an `MTRL` that
/// does not read is an error naming the first and the count.
pub type Wearers = HashMap<u32, BTreeMap<u32, std::collections::BTreeSet<usize>>>;

fn read_wearers(vz: &Path) -> Result<Wearers, String> {
    use mercs2_engine::wad;
    let mut w = wad::open(&vz.to_string_lossy())?;
    let mut map: Wearers = HashMap::new();
    let mut failures = Vec::new();
    let blocks = wad::for_each_chunk(&mut w, |c| {
        if c.entry.type_hash != wad::MODEL_TYPE_HASH {
            return;
        }
        match mercs2_formats::texture::parse_mtrl(c.bytes, mercs2_formats::texture::MtrlSource::Model) {
            Ok(mats) => {
                for (i, m) in mats.iter().enumerate() {
                    for t in m.textures.iter().filter(|t| **t != 0 && **t != 0xFFFF_FFFF) {
                        map.entry(*t).or_default().entry(c.entry.name_hash).or_default().insert(i);
                    }
                }
            }
            Err(e) => failures.push(format!("block {} {}: model 0x{:08X} MTRL: {e}", c.block, c.path, c.entry.name_hash)),
        }
    });
    failures.extend(blocks);
    match failures.first() {
        None => Ok(map),
        Some(f) => Err(format!("{} item(s) of {} did not read; the first: {f}", failures.len(), vz.display())),
    }
}

/// Which channels the viewer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Channels {
    pub r: bool,
    pub g: bool,
    pub b: bool,
    pub a: bool,
}

impl Default for Channels {
    fn default() -> Channels {
        Channels { r: true, g: true, b: true, a: true }
    }
}

impl Channels {
    /// `rgba` as the viewer shows it: alpha alone as grey; the colour channels shown kept, the others
    /// zero; alpha off makes it opaque.
    pub fn apply(self, rgba: &[u8]) -> Vec<u8> {
        let only_alpha = self.a && !self.r && !self.g && !self.b;
        rgba.as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                if only_alpha {
                    [p[3], p[3], p[3], 255]
                } else {
                    [
                        if self.r { p[0] } else { 0 },
                        if self.g { p[1] } else { 0 },
                        if self.b { p[2] } else { 0 },
                        if self.a { p[3] } else { 255 },
                    ]
                }
            })
            .collect()
    }
}

/// The viewer's choices for one contribution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice {
    pub side: Side,
    /// The level shown: an index into the plate's levels.
    pub level: usize,
    pub channels: Channels,
}

/// A still render the preview asks for: `model` wearing the game's texture `target`, or the built
/// texture in its place. `id` changes whenever what is drawn changes.
#[derive(Clone)]
pub struct ShotWant {
    pub id: String,
    pub model: u32,
    pub target: u32,
    /// The built texture, to draw in the target's place; `None` draws the game's.
    pub yours: Option<std::sync::Arc<TextureData>>,
}

/// A still render: what it shows, and the image or why there is none.
pub struct Shot {
    pub id: String,
    /// What was drawn: the model and, for a character kit, the outfit.
    pub label: String,
    pub image: Result<(u32, u32, Vec<u8>), String>,
}

/// The texture editors' reads and choices.
#[derive(Default)]
pub struct Tex {
    /// The still render the open preview asks for, and the last one drawn.
    pub want: RefCell<Option<ShotWant>>,
    shot: RefCell<Option<(Shot, Option<egui::TextureHandle>)>>,
    /// The wearer each `replace_texture` preview draws, by item folder and contribution.
    wearer: RefCell<HashMap<(PathBuf, usize), u32>>,
    after: RefCell<HashMap<PathBuf, (String, Loading<ItemTex>)>>,
    choices: RefCell<HashMap<(PathBuf, usize), Choice>>,
    /// Uploads, by plate, level and channels.
    uploads: RefCell<HashMap<(u32, usize, Channels), egui::TextureHandle>>,
    /// The last plate each contribution showed, so its uploads go when it is rebuilt.
    shown: RefCell<HashMap<(PathBuf, usize), Vec<u32>>>,
    /// The models that wear each texture of the game's `vz.wad`, read once per game.
    wearers: RefCell<Option<Loading<Wearers>>>,
}

/// What the texture editors ask of the page.
pub enum Ask {
    Choose { item: PathBuf, index: usize, choice: Choice },
    Wearer { item: PathBuf, index: usize, model: u32 },
}

impl Tex {
    pub fn forget_game(&self) {
        self.after.borrow_mut().clear();
        self.uploads.borrow_mut().clear();
        *self.wearers.borrow_mut() = None;
    }

    /// The models that wear each texture of `game`'s `vz.wad`: `None` while they are read.
    fn wearers(&self, game: &[PathBuf]) -> Option<Result<Rc<Wearers>, String>> {
        let mut slot = self.wearers.borrow_mut();
        let entry = slot.get_or_insert_with(|| {
            let vz = game.first().cloned();
            spawn(move || read_wearers(&vz.ok_or("no game archive is open")?))
        });
        poll(entry)
    }

    /// What `item` builds and shows: `None` while it is read, or when it holds no texture kind.
    pub fn after(&self, game: &[PathBuf], item: &Item) -> Option<Result<Rc<ItemTex>, String>> {
        let contributions = item.file.as_ref().ok()?.contributions.clone();
        if !contributions.iter().any(of) {
            return None;
        }
        let mut stamp = format!("{game:?}|{contributions:?}");
        for c in &contributions {
            if let Contribution::AddTexture { image, .. } | Contribution::ReplaceTexture { image, .. } | Contribution::AddFxSprite { image, .. } = c {
                let t = std::fs::metadata(item.dir.join(image)).and_then(|m| m.modified()).ok();
                stamp.push_str(&format!("|{t:?}"));
            }
        }
        let mut map = self.after.borrow_mut();
        if map.get(&item.dir).is_none_or(|(s, _)| *s != stamp) {
            let (wads, dir) = (game.to_vec(), item.dir.clone());
            map.insert(item.dir.clone(), (stamp, spawn(move || after(&wads, &dir, &contributions))));
        }
        let (_, entry) = map.get_mut(&item.dir)?;
        poll(entry)
    }

    pub fn choice(&self, item: &Path, index: usize, plates: &[Plate]) -> Choice {
        let default = Choice { side: plates.last().map_or(Side::Built, |p| p.side), level: 0, channels: Channels::default() };
        self.choices.borrow().get(&(item.to_path_buf(), index)).copied().filter(|c| plates.iter().any(|p| p.side == c.side)).unwrap_or(default)
    }

    pub fn set_choice(&self, item: PathBuf, index: usize, choice: Choice) {
        self.choices.borrow_mut().insert((item, index), choice);
    }

    /// Draw `model` in the `replace_texture` preview of contribution `index` of `item`.
    pub fn set_wearer(&self, item: PathBuf, index: usize, model: u32) {
        self.wearer.borrow_mut().insert((item, index), model);
    }

    /// The still render the app drew for `want`.
    pub fn set_shot(&self, shot: Shot) {
        *self.shot.borrow_mut() = Some((shot, None));
    }

    /// The id of the last still render drawn.
    pub fn shot_id(&self) -> Option<String> {
        self.shot.borrow().as_ref().map(|(s, _)| s.id.clone())
    }

    /// The upload of `plate`'s level `level` through `channels`, made the first time it is shown.
    fn upload(&self, ctx: &egui::Context, plate: &Plate, level: usize, channels: Channels) -> Option<egui::TextureHandle> {
        let l = plate.levels.get(level)?;
        let key = (plate.id, level, channels);
        let mut uploads = self.uploads.borrow_mut();
        Some(
            uploads
                .entry(key)
                .or_insert_with(|| {
                    let image = egui::ColorImage::from_rgba_unmultiplied([l.width as usize, l.height as usize], &channels.apply(&l.rgba));
                    let filter = egui::TextureOptions::NEAREST;
                    ctx.load_texture(format!("depot-tex-{}-{level}", plate.id), image, filter)
                })
                .clone(),
        )
    }

    /// Drop the uploads of the plates `key` showed before it was rebuilt.
    fn retire(&self, key: (PathBuf, usize), plates: &[Plate]) {
        let ids: Vec<u32> = plates.iter().map(|p| p.id).collect();
        let mut shown = self.shown.borrow_mut();
        if let Some(old) = shown.insert(key, ids.clone()) {
            if old != ids {
                self.uploads.borrow_mut().retain(|(id, _, _), _| !old.contains(id) || ids.contains(id));
            }
        }
    }
}

/// The largest side of the viewer's image.
const VIEW_MAX: f32 = 512.0;

/// The preview of contribution `index` of `item`.
pub fn editor(ui: &mut egui::Ui, t: &Tex, item: &Item, index: usize, game: &[PathBuf], names: Option<&mercs2_quartermaster::NameTable>) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return asks };
    if !of(c) {
        return asks;
    }
    theme::section(ui, "Preview", Some(c.kind()), true, |ui| {
        let after = match t.after(game, item) {
            None => {
                ui.label(egui::RichText::new("Reading the game's texture and building\u{2026}").color(theme::DIM));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Some(Err(e)) => {
                theme::field_note(ui, FieldState::Bad, &e);
                return;
            }
            Some(Ok(a)) => a,
        };
        let Some(v) = after.views.get(&index) else { return };
        for p in &v.problems {
            theme::field_note(ui, FieldState::Bad, p);
        }
        if v.plates.is_empty() {
            return;
        }
        t.retire((item.dir.clone(), index), &v.plates);
        let mut choice = t.choice(&item.dir, index, &v.plates);
        let plate = v.plates.iter().find(|p| p.side == choice.side).unwrap_or(&v.plates[0]);
        let level = choice.level.min(plate.levels.len().saturating_sub(1));
        let before = choice;

        // The plates, then the channels and the level.
        ui.horizontal_wrapped(|ui| {
            for p in &v.plates {
                if theme::pill(ui, p.side.label(), p.side == plate.side).clicked() {
                    choice.side = p.side;
                }
            }
            ui.add_space(12.0);
            for (label, on) in [("R", &mut choice.channels.r), ("G", &mut choice.channels.g), ("B", &mut choice.channels.b), ("A", &mut choice.channels.a)] {
                if theme::pill(ui, label, *on).clicked() {
                    *on = !*on;
                }
            }
        });
        if plate.levels.len() > 1 {
            ui.horizontal_wrapped(|ui| {
                theme::eyebrow(ui, "Level");
                for (i, l) in plate.levels.iter().enumerate() {
                    if theme::pill(ui, &format!("{} \u{b7} {}\u{d7}{}", l.level, l.width, l.height), i == level).clicked() {
                        choice.level = i;
                    }
                }
            });
        }
        if choice != before {
            asks.push(Ask::Choose { item: item.dir.clone(), index, choice });
        }

        let Some(l) = plate.levels.get(level) else { return };
        // A replaced texture's preview shows the engine's render of a model wearing it beside it.
        let wear = match c {
            Contribution::ReplaceTexture { target, .. } if !mercs2_quartermaster::fx::repaints_atlas(c) => {
                Some(Wear { item, index, target: asset_hash(target), shipped: plate.side == Side::Shipped })
            }
            _ => None,
        };
        const GAP: f32 = 8.0;
        let side = match wear {
            Some(_) => ((ui.available_width() - GAP) / 2.0).min(VIEW_MAX),
            None => ui.available_width().min(VIEW_MAX),
        };
        ui.horizontal_top(|ui| {
            image_plate(ui, t, plate, (l, level), choice.channels, side);
            if let Some(wear) = &wear {
                ui.add_space(GAP);
                ui.vertical(|ui| shot_panel(ui, t, game, v, wear, names, side));
            }
        });
        ui.label(egui::RichText::new(&plate.facts).monospace().small().color(theme::DIM));
        if let Some(e) = &v.error {
            ui.label(egui::RichText::new(e).small().color(theme::DIM));
        }
        if let Some(wear) = &wear {
            ui.add_space(6.0);
            wearer_list(ui, t, game, wear, names, &mut asks);
        }
    });
    asks
}

/// One level of a plate through `channels`, fitted into a square of `side`, over a checkerboard,
/// its marks drawn.
fn image_plate(ui: &mut egui::Ui, t: &Tex, plate: &Plate, (l, level): (&Level, usize), channels: Channels, side: f32) {
    let scale = side / l.width.max(l.height) as f32;
    let size = egui::vec2(l.width as f32 * scale, l.height as f32 * scale);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let p = ui.painter_at(rect);
    // A checkerboard behind the alpha.
    let cell = 8.0;
    let (cols, rows) = ((size.x / cell).ceil() as i32, (size.y / cell).ceil() as i32);
    for y in 0..rows {
        for x in 0..cols {
            let col = if (x + y) % 2 == 0 { theme::G2 } else { theme::G3 };
            let r = egui::Rect::from_min_size(rect.min + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell, cell));
            p.rect_filled(r.intersect(rect), egui::CornerRadius::ZERO, col);
        }
    }
    if let Some(tex) = t.upload(ui.ctx(), plate, level, channels) {
        p.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
    }
    // Marks are in level-0 texels.
    let full = plate.levels.first().map_or(1.0, |f| f.width.max(f.height) as f32);
    let k = size.x.max(size.y) / full;
    for (label, x, y, w, h) in &plate.marks {
        let r = egui::Rect::from_min_size(rect.min + egui::vec2(*x as f32 * k, *y as f32 * k), egui::vec2(*w as f32 * k, *h as f32 * k));
        p.rect_stroke(r, egui::CornerRadius::ZERO, egui::Stroke::new(1.5, theme::BRASS), egui::StrokeKind::Outside);
        // Below the mark, or above it when it sits at the bottom of the image.
        let (at, align) = if r.bottom() + 14.0 <= rect.bottom() {
            (r.left_bottom() + egui::vec2(0.0, 2.0), egui::Align2::LEFT_TOP)
        } else {
            (r.left_top() - egui::vec2(0.0, 2.0), egui::Align2::LEFT_BOTTOM)
        };
        p.text(at, align, label, theme::font(&egui::TextStyle::Small), theme::BRASS);
    }
}

/// Which `replace_texture` preview the wearers are of, and which texture the render puts on them.
struct Wear<'a> {
    item: &'a Item,
    index: usize,
    target: u32,
    /// Draw the game's texture rather than the built one.
    shipped: bool,
}

/// Each model wearing a texture with its materials that cite it, and the model the render draws; or
/// why the wearers could not be read.
type Wearing = Result<(Vec<(u32, Vec<usize>)>, Option<u32>), String>;

/// The models of the game's `vz.wad` whose materials cite the target, and the one the render draws:
/// the one chosen, else the first. `None` while they are read.
fn wearing(t: &Tex, game: &[PathBuf], wear: &Wear<'_>) -> Option<Wearing> {
    let w = match t.wearers(game)? {
        Err(e) => return Some(Err(e)),
        Ok(w) => w,
    };
    let models: Vec<(u32, Vec<usize>)> = w.get(&wear.target).into_iter().flatten().map(|(m, mats)| (*m, mats.iter().copied().collect())).collect();
    let key = (wear.item.dir.clone(), wear.index);
    let chosen = t.wearer.borrow().get(&key).copied().filter(|m| models.iter().any(|(x, _)| x == m)).or_else(|| models.first().map(|(m, _)| *m));
    Some(Ok((models, chosen)))
}

fn name_of(m: u32, names: Option<&mercs2_quartermaster::NameTable>) -> String {
    names.and_then(|n| n.reverse(m)).map_or_else(|| format!("0x{m:08X}"), str::to_string)
}

/// A square of `side` holding a spinner and what it waits for.
fn waiting(ui: &mut egui::Ui, side: f32, what: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter().rect_filled(rect, egui::CornerRadius::same(4), theme::G0);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(12.0)).layout(egui::Layout::top_down(egui::Align::Center)));
    child.add_space(side / 2.0 - 36.0);
    child.add(egui::Spinner::new().size(28.0));
    child.add(egui::Label::new(egui::RichText::new(what).small().color(theme::DIM)).wrap().selectable(false));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
}

/// The engine's still render of the chosen wearer, wearing the built texture (or, while Shipped is
/// chosen, the game's), in a square of `side`; a spinner while the wearers are read or it is drawn.
fn shot_panel(ui: &mut egui::Ui, t: &Tex, game: &[PathBuf], v: &View, wear: &Wear<'_>, names: Option<&mercs2_quartermaster::NameTable>, side: f32) {
    let model = match wearing(t, game, wear) {
        None => return waiting(ui, side, "Reading every model's materials in vz.wad\u{2026}"),
        Some(Err(e)) => return theme::field_note(ui, FieldState::Bad, &e),
        Some(Ok((_, None))) => return theme::field_note(ui, FieldState::Neutral, "no model of vz.wad wears it"),
        Some(Ok((_, Some(m)))) => m,
    };
    let Some((target, built)) = &v.replaced else { return };
    let build = v.plates.iter().find(|p| p.side == Side::Built).map_or(0, |p| p.id);
    let want = ShotWant {
        id: format!("{}|{}|{model:08X}|{}", wear.item.dir.display(), wear.index, if wear.shipped { "shipped".to_string() } else { format!("build {build}") }),
        model,
        target: *target,
        yours: (!wear.shipped).then(|| built.clone()),
    };
    let mut shot = t.shot.borrow_mut();
    match shot.as_mut().filter(|(s, _)| s.id == want.id) {
        None => waiting(ui, side, &format!("Rendering {} in the engine\u{2026}", name_of(model, names))),
        Some((s, handle)) => match &s.image {
            Err(e) => theme::field_note(ui, FieldState::Bad, e),
            Ok((w, h, rgba)) => {
                let tex = handle.get_or_insert_with(|| {
                    let image = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], rgba);
                    ui.ctx().load_texture(format!("depot-shot-{}", s.id), image, egui::TextureOptions::LINEAR)
                });
                let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side * *h as f32 / (*w).max(1) as f32), egui::Sense::hover());
                ui.painter().image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                ui.set_max_width(side);
                ui.add(egui::Label::new(egui::RichText::new(&s.label).small().color(theme::DIM)).wrap().selectable(false));
            }
        },
    }
    *t.want.borrow_mut() = Some(want);
}

/// The models of the game's `vz.wad` whose materials cite the target, each with the materials that
/// do; a click chooses the one the render draws.
fn wearer_list(ui: &mut egui::Ui, t: &Tex, game: &[PathBuf], wear: &Wear<'_>, names: Option<&mercs2_quartermaster::NameTable>, asks: &mut Vec<Ask>) {
    let Some(Ok((models, chosen))) = wearing(t, game, wear) else { return };
    theme::eyebrow(ui, &format!("Worn by {} model(s) in vz.wad", models.len()));
    for (model, mats) in &models {
        let mats: Vec<String> = mats.iter().map(|m| m.to_string()).collect();
        ui.horizontal(|ui| {
            if ui.selectable_label(chosen == Some(*model), egui::RichText::new(name_of(*model, names)).color(theme::TX)).clicked() {
                asks.push(Ask::Wearer { item: wear.item.dir.clone(), index: wear.index, model: *model });
            }
            ui.label(egui::RichText::new(format!("0x{model:08X}")).monospace().small().color(theme::FAINT));
            ui.label(egui::RichText::new(format!("materials {}", mats.join(", "))).small().color(theme::DIM));
        });
    }
}

/// The inspector's card: each block the texture build ships, and the build's log.
pub fn after_card(ui: &mut egui::Ui, t: &Tex, game: &[PathBuf], item: &Item) {
    let Some(after) = t.after(game, item) else {
        if item.file.as_ref().is_ok_and(|f| f.contributions.iter().any(of)) {
            theme::card(ui, "Texture build", None, |ui| {
                ui.label(egui::RichText::new("Building\u{2026}").color(theme::DIM));
            });
        }
        return;
    };
    theme::card(ui, "Texture build", Some("qm build"), |ui| match after.as_ref() {
        Err(e) => theme::field_note(ui, FieldState::Bad, e),
        Ok(a) => {
            for s in &a.ships {
                theme::kv(ui, "overlay", egui::RichText::new(&s.path).monospace());
                for h in &s.holds {
                    ui.label(egui::RichText::new(h).monospace().small().color(theme::DIM));
                }
            }
            for line in &a.log {
                ui.label(egui::RichText::new(line).small().color(theme::FAINT));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w`×`h` image whose texels grade by position.
    fn gradient(w: u32, h: u32) -> Vec<u8> {
        (0..h).flat_map(|y| (0..w).flat_map(move |x| [(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, 128, 255])).collect()
    }

    fn item(tag: &str, w: u32, h: u32) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depottex_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        crate::texpng::write_png(&d.join("src/mine.png").to_string_lossy(), w, h, &gradient(w, h)).unwrap();
        d
    }

    /// A new texture builds with no game open: its PNG beside the texture the build ships, decoded at
    /// every level, a DXT1 one of the opaque PNG, and the encode error measured on its finest level.
    #[test]
    fn a_new_texture_shows_its_png_beside_the_texture_the_build_ships() {
        let dir = item("add", 64, 32);
        let c = Contribution::AddTexture { name: "depot_test_tex".into(), image: "src/mine.png".into(), normal_map: false };
        let a = after(&[], &dir, &[c]).unwrap();
        let v = &a.views[&0];
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        let sides: Vec<Side> = v.plates.iter().map(|p| p.side).collect();
        assert_eq!(sides, vec![Side::Png, Side::Built]);
        let built = &v.plates[1];
        let dims: Vec<(u32, u32)> = built.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(dims, vec![(64, 32), (32, 16), (16, 8), (8, 4)], "the chain stops at the level whose shorter side is 4");
        assert!(built.facts.contains("DXT1"), "{}", built.facts);
        let e = v.error.as_deref().expect("an encode error");
        assert!(e.starts_with("encode error") && !e.contains(" A "), "{e}");
        assert_eq!(a.ships.len(), 1);
        assert_eq!(a.ships[0].path, format!("blocks\\VZ\\mod_{:08x}.block", m2("depot_test_tex")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A normal map's error is measured against the PNG as the build swizzles it, so a faithful encode
    /// reads small; against the unswizzled PNG it would read large.
    #[test]
    fn a_normal_maps_error_is_measured_against_its_swizzle() {
        let dir = item("normal", 32, 32);
        let c = Contribution::AddTexture { name: "depot_test_nm".into(), image: "src/mine.png".into(), normal_map: true };
        let a = after(&[], &dir, &[c]).unwrap();
        let v = &a.views[&0];
        let e = v.error.as_deref().expect("an encode error");
        assert!(e.contains("swizzles"), "{e}");
        let built = v.plates[1].levels.first().unwrap();
        let png = png_level(&dir.join("src/mine.png")).unwrap();
        let mean_r = |l: &Level| l.rgba.as_chunks::<4>().0.iter().map(|p| f64::from(p[0])).sum::<f64>() / (l.rgba.len() / 4) as f64;
        assert!((mean_r(built) - 255.0).abs() < 2.0, "R of a DXT5nm is 1");
        assert!((mean_r(&png) - 255.0).abs() > 50.0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn channels_show_alpha_alone_as_grey_and_drop_what_is_off() {
        let px = [10, 20, 30, 40];
        assert_eq!(Channels::default().apply(&px), px.to_vec());
        assert_eq!(Channels { r: false, g: false, b: false, a: true }.apply(&px), vec![40, 40, 40, 255]);
        assert_eq!(Channels { r: true, g: false, b: true, a: false }.apply(&px), vec![10, 0, 30, 255]);
    }

    /// Every level of a chain decodes at its own size, finest first, to the count the container
    /// declares; a chain holding only its coarser levels starts at the finest it holds.
    #[test]
    fn every_level_of_a_chain_decodes() {
        let (w, h) = (64usize, 32usize);
        let px: Vec<f32> = gradient(w as u32, h as u32).iter().map(|v| f32::from(*v)).collect();
        let body = mercs2_formats::texture_encode::mip_chain(w, h, 4, &px, mercs2_formats::texture_encode::encode_bc3);
        let count = mercs2_formats::texture_encode::mip_count(w, h) as u32;
        let td = TextureData { width: 64, height: 32, format: TexFormat::Bc3, mip0: body.clone(), all_mips: body.clone(), mip_count: count };
        let levels = crate::texpng::decode_levels(&td);
        let dims: Vec<(u32, u32, u32)> = levels.iter().map(|l| (l.level, l.width, l.height)).collect();
        assert_eq!(dims, vec![(0, 64, 32), (1, 32, 16), (2, 16, 8), (3, 8, 4)]);
        assert_eq!(levels[0].rgba.len(), 64 * 32 * 4);
        let tail = body[64 * 32 + 32 * 16..].to_vec();
        let td = TextureData { mip0: tail.clone(), all_mips: tail, ..td };
        let dims: Vec<u32> = crate::texpng::decode_levels(&td).iter().map(|l| l.level).collect();
        assert_eq!(dims, vec![2, 3]);
    }
}

/// Against the game named by `.mercs2-local.toml`.
#[cfg(all(test, feature = "retail"))]
mod retail_tests {
    use super::*;

    fn vz() -> PathBuf {
        mercs2_formats::game_paths::local_config_vz_wad(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{e}"))
    }

    /// A replaced texture: the game's at full resolution, the PNG (here the game's own finest level,
    /// written out) and the texture the build ships at the game's size and format, whose finest level
    /// is the PNG to within the encode.
    #[test]
    fn a_replaced_texture_shows_the_games_beside_the_built_one() {
        let target = "al_hum_boss_ub";
        let shipped = shipped_texture(&[vz()], asset_hash(target)).unwrap_or_else(|e| panic!("{e}"));
        let finest = crate::texpng::decode_levels(&shipped).remove(0);
        assert_eq!((finest.width, finest.height), (shipped.width, shipped.height), "the full resolution is read");
        let dir = std::env::temp_dir().join(format!("m2ws_depottex_replace_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        crate::texpng::write_png(&dir.join("src/mine.png").to_string_lossy(), finest.width, finest.height, &finest.rgba).unwrap();
        let c = Contribution::ReplaceTexture { target: target.into(), image: "src/mine.png".into() };
        let a = after(&[vz()], &dir, &[c]).unwrap_or_else(|e| panic!("{e}"));
        let v = &a.views[&0];
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        assert_eq!(v.plates.iter().map(|p| p.side).collect::<Vec<_>>(), vec![Side::Shipped, Side::Png, Side::Built]);
        let built = v.plates[2].levels.first().unwrap();
        assert_eq!((built.width, built.height), (shipped.width, shipped.height));
        assert!(v.plates[2].facts.contains(format_name(shipped.format)), "{}", v.plates[2].facts);
        eprintln!("{target}: {}x{} {}; {}", shipped.width, shipped.height, format_name(shipped.format), v.error.as_deref().unwrap_or(""));
        let mean = |s: &str| s.split(" · ").filter_map(|p| p.split_whitespace().nth(1)).filter_map(|n| n.parse::<f64>().ok()).fold(0.0, f64::max);
        assert!(mean(v.error.as_deref().unwrap()) < 8.0, "{:?}", v.error);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// The models that wear a texture, as `mercs2_probe`'s `tex_users` reads them: `al_hum_boss_ub` is
    /// cited by materials 2, 3 and 8 of `al_hum_boss` alone.
    #[test]
    fn the_models_that_wear_a_texture_are_read_from_every_models_materials() {
        let w = read_wearers(&vz()).unwrap_or_else(|e| panic!("{e}"));
        let models = &w[&asset_hash("al_hum_boss_ub")];
        assert_eq!(models.len(), 1, "{models:?}");
        assert_eq!(models[&m2("al_hum_boss")].iter().copied().collect::<Vec<_>>(), vec![2, 3, 8]);
    }

    /// A sprite: the game's atlas, the PNG, and the atlas the merge draws with the sprite marked where
    /// it went, its texels there the PNG's.
    #[test]
    fn a_sprite_is_marked_where_the_merge_draws_it() {
        let dir = std::env::temp_dir().join(format!("m2ws_depottex_sprite_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let px: Vec<u8> = (0..32 * 32).flat_map(|i| [200, (i % 32 * 8) as u8, 40, 255]).collect();
        crate::texpng::write_png(&dir.join("src/spark.png").to_string_lossy(), 32, 32, &px).unwrap();
        let c = Contribution::AddFxSprite { name: "depot_test_spark".into(), image: "src/spark.png".into() };
        let a = after(&[vz()], &dir, &[c]).unwrap_or_else(|e| panic!("{e}"));
        let v = &a.views[&0];
        assert!(v.problems.is_empty(), "{:?}", v.problems);
        assert_eq!(v.plates.iter().map(|p| p.side).collect::<Vec<_>>(), vec![Side::Shipped, Side::Png, Side::Built]);
        let built = &v.plates[2];
        let [(label, x, y, w, h)] = built.marks.as_slice() else { panic!("one mark: {:?}", built.marks) };
        assert_eq!((label.as_str(), *w, *h), ("depot_test_spark", 32, 32));
        let l0 = &built.levels[0];
        let at = ((y + 16) * l0.width + x + 16) as usize * 4;
        let texel = &l0.rgba[at..at + 4];
        assert!(texel[0].abs_diff(200) < 12 && texel[2].abs_diff(40) < 12, "{texel:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
