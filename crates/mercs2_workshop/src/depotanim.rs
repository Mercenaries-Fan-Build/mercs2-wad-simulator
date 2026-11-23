//! The animation kinds' editor and preview (`docs/modernization/workshop_pages/depot_kind_editors.md`
//! §Animation): the facts of the clip an item ships and of its bone binding, its events as rows, and
//! the clip played on a hero in the Depot's viewport through the engine — for a `replace_animation`,
//! the shipped clip beside it on the same hero.
//!
//! The page reads the files and ranks the heroes; the app plays what [`Anim::want`] names and
//! reports the clock back through [`Anim::live`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use mercs2_formats::anim_container::{build_evnt, clip_pairing_problems, parse_evnt, trnm_bone_hashes, AnimEvent};
use mercs2_quartermaster::manifest::{asset_hash, Contribution};
use mercs2_quartermaster::NameTable;

use crate::depot::Item;
use crate::depottext::{poll, spawn, Loading};
use crate::gui::theme::{self, FieldState};
use crate::heroidx::Skeletons;

/// A contribution of an item: the item folder and the contribution's index.
type Key = (PathBuf, usize);
/// A clip's packfile and `trnm` body.
pub type ClipBytes = (Arc<Vec<u8>>, Arc<Vec<u8>>);
/// An events file: its path in the item, its bytes and its events.
pub type EventsFile = (PathBuf, Vec<u8>, Result<Vec<AnimEvent>, String>);
/// Models with how many of a clip's bones each holds, most first.
type Ranked = Rc<Vec<(u32, usize)>>;

pub fn of(c: &Contribution) -> bool {
    matches!(c, Contribution::AddAnimation { .. } | Contribution::ReplaceAnimation { .. })
}

/// The files and name of one animation contribution.
struct Files {
    clip: PathBuf,
    trnm: PathBuf,
    events: Option<PathBuf>,
    /// The asset the clip ships as: `name`, or a replace's `target`.
    name: String,
    replaces: bool,
}

fn files(c: &Contribution) -> Option<Files> {
    match c {
        Contribution::AddAnimation { name, clip, trnm, events } => {
            Some(Files { clip: clip.clone(), trnm: trnm.clone(), events: events.clone(), name: name.clone(), replaces: false })
        }
        Contribution::ReplaceAnimation { target, clip, trnm, events } => {
            Some(Files { clip: clip.clone(), trnm: trnm.clone(), events: events.clone(), name: target.clone(), replaces: true })
        }
        _ => None,
    }
}

/// What a clip's packfile reads as.
#[derive(Debug, Clone, PartialEq)]
pub struct Facts {
    pub class: String,
    pub duration: f32,
    pub frames: usize,
    pub tracks: usize,
    /// Whether the clip's poses decode (a spline clip reads as a header only).
    pub decoded: bool,
}

/// What a clip and its `trnm` read as.
pub struct Clip {
    pub clip: Arc<Vec<u8>>,
    pub trnm: Arc<Vec<u8>>,
    pub facts: Result<Facts, String>,
    /// The bone each track drives, by name-hash.
    pub bones: Result<Vec<u32>, String>,
}

impl Clip {
    fn of(clip: Vec<u8>, trnm: Vec<u8>) -> Clip {
        let facts = mercs2_formats::anim::parse_anim(&clip).map(|a| Facts {
            class: format!("{:?}", a.anim_type),
            duration: a.duration,
            frames: a.num_frames,
            tracks: a.num_tracks,
            decoded: a.decoded,
        });
        Clip { bones: trnm_bone_hashes(&trnm), clip: Arc::new(clip), trnm: Arc::new(trnm), facts }
    }
}

/// The item's own files.
pub struct Yours {
    pub clip: Clip,
    /// The events file's path, bytes and events: `None` when the contribution names none.
    pub events: Option<EventsFile>,
    /// M0213's findings for the three files.
    pub problems: Vec<String>,
}

/// The shipped clip a replace names, as qm's replace reads it.
pub struct Shipped {
    pub clip: Clip,
    /// The shipped `evnt` body, when the clip carries one.
    pub events: Option<Vec<u8>>,
}

/// What one contribution reads as: its files, and for a replace the shipped clip.
pub struct Read {
    pub yours: Result<Yours, String>,
    pub shipped: Option<Result<Shipped, String>>,
}

fn read_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn read_yours(dir: &Path, f: &Files) -> Result<Yours, String> {
    let clip = read_file(&dir.join(&f.clip))?;
    let trnm = read_file(&dir.join(&f.trnm))?;
    let events = match &f.events {
        None => None,
        Some(p) => {
            let bytes = read_file(&dir.join(p))?;
            let parsed = parse_evnt(&bytes);
            Some((p.clone(), bytes, parsed))
        }
    };
    let problems = clip_pairing_problems(&clip, &trnm, events.as_ref().map(|(_, b, _)| b.as_slice()));
    Ok(Yours { clip: Clip::of(clip, trnm), events, problems })
}

/// The clip, `trnm` and events of the animation asset `target` names in `game`: its own container,
/// which is what a `replace_animation` of it replaces. Refused for a MANM keyframe animation.
pub(crate) fn read_shipped(game: &[PathBuf], target: &str) -> Result<Shipped, String> {
    use mercs2_formats::anim_container::{classify, parse_container, AnimContainerKind};
    use mercs2_formats::types::{TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION};
    let mut stack = mercs2_quartermaster::game::GameStack::open(game).map_err(|e| e.to_string())?;
    let hash = asset_hash(target);
    let container = stack
        .container_for_asset(hash, TYPE_HASH_ANIMATION, TYPE_ID_ANIMATION)
        .ok_or_else(|| format!("{target} (0x{hash:08X}) is no animation of the game"))?;
    let chunks = parse_container(&container).map_err(|e| format!("the shipped {target} does not read: {e}"))?;
    if matches!(classify(&chunks)?, AnimContainerKind::Keyframe) {
        return Err(format!("the shipped {target} is a MANM keyframe animation, not a Havok clip; a replace cannot express it"));
    }
    let body = |tag: &[u8; 4]| chunks.iter().find(|c| &c.tag == tag).map(|c| c.body.clone());
    let (Some(clip), Some(trnm)) = (body(b"data"), body(b"trnm")) else {
        return Err(format!("the shipped {target} has no data or trnm chunk"));
    };
    Ok(Shipped { clip: Clip::of(clip, trnm), events: body(b"evnt") })
}

fn read(dir: &Path, f: &Files, game: &[PathBuf]) -> Result<Read, String> {
    Ok(Read { yours: read_yours(dir, f), shipped: f.replaces.then(|| read_shipped(game, &f.name)) })
}

/// The stamp a contribution's read is kept under: its fields, each file's modification time, and
/// for a replace the target and the game.
fn stamp(dir: &Path, f: &Files, game: &[PathBuf]) -> String {
    let t = |p: &Path| std::fs::metadata(dir.join(p)).and_then(|m| m.modified()).ok();
    let shipped = if f.replaces { format!("{}|{game:?}", f.name) } else { String::new() };
    format!("{}|{}|{:?}|{:?}|{:?}|{:?}|{shipped}", f.clip.display(), f.trnm.display(), f.events, t(&f.clip), t(&f.trnm), f.events.as_deref().map(t))
}

/// The app's transport commands for the clip the viewport plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnimAct {
    Play,
    Pause,
    Restart,
    Seek(f32),
    Loop(bool),
}

/// What the viewport plays: the clip of contribution `index` of the open item on `hero`, and for a
/// replace the shipped clip on a second copy of the hero beside it. `id` changes whenever what plays
/// changes.
#[derive(Clone)]
pub struct Want {
    pub id: String,
    /// What `id` is without the hero: the same item's clip keeps its clock across a hero change.
    pub clip_id: String,
    pub hero: u32,
    pub hero_label: String,
    /// The item's clip and `trnm`; `None` when its files do not read (the editor says why).
    pub yours: Option<ClipBytes>,
    /// For a replace, the shipped clip and `trnm`.
    pub shipped: Option<ClipBytes>,
    pub name_hash: u32,
    pub label: String,
}

/// What the viewport plays, as the app last reported it.
#[derive(Clone, Debug, PartialEq)]
pub struct Live {
    /// The [`Want::id`] it plays.
    pub id: String,
    pub time: f32,
    pub duration: f32,
    pub playing: bool,
    pub looping: bool,
    /// Whether the item's clip plays on the hero: `None` when its files do not read.
    pub yours: Option<Result<(), String>>,
    /// For a replace: whether the shipped clip plays on the hero.
    pub shipped: Option<Result<(), String>>,
}

enum Skel {
    Reading(std::sync::mpsc::Receiver<crate::heroidx::Msg>, f32),
    Ready(Result<Rc<Skeletons>, String>),
}

/// The animation editors' reads and choices.
#[derive(Default)]
pub struct Anim {
    reads: RefCell<HashMap<Key, (String, Loading<Read>)>>,
    skeletons: RefCell<Option<Skel>>,
    /// The models ranked for each contribution, with the bones they were ranked by.
    ranked: RefCell<HashMap<Key, (Vec<u32>, Ranked)>>,
    hero: RefCell<HashMap<(PathBuf, usize), u32>>,
    /// The contribution of each item the viewport plays, once one is chosen; else its first.
    focus: RefCell<HashMap<PathBuf, usize>>,
    filter: RefCell<String>,
    pub live: RefCell<Option<Live>>,
}

/// What the animation editors ask of the page.
pub enum Ask {
    Hero { item: PathBuf, index: usize, model: u32 },
    Focus { item: PathBuf, index: usize },
    Play(AnimAct),
    /// Write `events` over the events file `path`, whose bytes were `prior`.
    Events { path: PathBuf, prior: Vec<u8>, events: Vec<AnimEvent> },
    /// Create the empty events file `path` and point contribution `index` of the item in `item` at
    /// it (`c`).
    AddEvents { item: PathBuf, path: PathBuf, index: usize, c: Box<Contribution> },
    /// Write `files` (each new) and replace contribution `index` of the item in `item` with `c`,
    /// which names them: a replace started from the shipped clip.
    StartShipped { item: PathBuf, index: usize, c: Box<Contribution>, files: Vec<(PathBuf, Arc<Vec<u8>>)> },
}

impl Anim {
    pub fn forget_game(&self) {
        self.reads.borrow_mut().clear();
        *self.skeletons.borrow_mut() = None;
        self.ranked.borrow_mut().clear();
    }

    /// What contribution `index` of `item` reads as: `None` while it is read, or when it is no
    /// animation kind.
    pub fn read(&self, game: &[PathBuf], item: &Item, index: usize) -> Option<Result<Rc<Read>, String>> {
        let f = item.file.as_ref().ok()?.contributions.get(index).and_then(files)?;
        let s = stamp(&item.dir, &f, game);
        let key = (item.dir.clone(), index);
        let mut map = self.reads.borrow_mut();
        if map.get(&key).is_none_or(|(old, _)| *old != s) {
            let (dir, game) = (item.dir.clone(), game.to_vec());
            map.insert(key.clone(), (s, spawn(move || read(&dir, &f, &game))));
        }
        poll(&mut map.get_mut(&key)?.1)
    }

    /// Every model's skeleton in `game`: `Err` with the fraction read while they are read.
    fn skeletons(&self, game: &[PathBuf]) -> Result<Result<Rc<Skeletons>, String>, f32> {
        let mut slot = self.skeletons.borrow_mut();
        let s = slot.get_or_insert_with(|| {
            let stack: Vec<String> = game.iter().map(|p| p.display().to_string()).collect();
            Skel::Reading(crate::heroidx::spawn(stack, Vec::new()), 0.0)
        });
        if let Skel::Reading(rx, at) = s {
            loop {
                match rx.try_recv() {
                    Ok(crate::heroidx::Msg::Progress(f, _)) => *at = f,
                    Ok(crate::heroidx::Msg::Done(r)) => {
                        *s = Skel::Ready(r.map(Rc::new));
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => return Err(*at),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        *s = Skel::Ready(Err("the skeleton reader stopped without an answer".into()));
                        break;
                    }
                }
            }
        }
        match s {
            Skel::Ready(r) => Ok(r.clone()),
            Skel::Reading(..) => unreachable!("read above"),
        }
    }

    /// The models whose skeleton holds any of `bones`, most first, with how many: `Err` with the
    /// fraction read while the skeletons are read.
    fn heroes(&self, game: &[PathBuf], key: &Key, bones: &[u32]) -> Result<Result<Ranked, String>, f32> {
        let sk = match self.skeletons(game)? {
            Ok(s) => s,
            Err(e) => return Ok(Err(e)),
        };
        let mut ranked = self.ranked.borrow_mut();
        if let Some((b, r)) = ranked.get(key) {
            if b == bones {
                return Ok(Ok(r.clone()));
            }
        }
        let r = Rc::new(rank(&sk, bones));
        ranked.insert(key.clone(), (bones.to_vec(), r.clone()));
        Ok(Ok(r))
    }

    /// The hero contribution `index` of `item` plays on: the one chosen while it still ranks, else
    /// the first.
    fn chosen(&self, key: &(PathBuf, usize), heroes: &[(u32, usize)]) -> Option<u32> {
        let picked = self.hero.borrow().get(key).copied();
        picked.filter(|m| heroes.iter().any(|(h, _)| h == m)).or_else(|| heroes.first().map(|(h, _)| *h))
    }

    pub fn set_hero(&self, item: PathBuf, index: usize, model: u32) {
        self.hero.borrow_mut().insert((item, index), model);
    }

    pub fn set_focus(&self, item: PathBuf, index: usize) {
        self.focus.borrow_mut().insert(item, index);
    }

    /// The animation contribution of `item` the viewport plays.
    fn focused(&self, item: &Item) -> Option<usize> {
        let cs = &item.file.as_ref().ok()?.contributions;
        let first = cs.iter().position(of)?;
        Some(self.focus.borrow().get(&item.dir).copied().filter(|&i| cs.get(i).is_some_and(of)).unwrap_or(first))
    }

    /// What the viewport plays for `item`: `None` for an item with no animation kind, and while its
    /// files or the skeletons are read, or when its clip cannot play (the editor says why).
    pub fn want(&self, game: &[PathBuf], item: &Item, names: Option<&NameTable>) -> Option<Want> {
        let index = self.focused(item)?;
        let f = files(item.file.as_ref().ok()?.contributions.get(index)?)?;
        let read = self.read(game, item, index)?.ok()?;
        let plays = |c: &Clip| c.facts.as_ref().is_ok_and(|x| x.decoded) && c.bones.is_ok();
        let yours = read.yours.as_ref().ok().map(|y| &y.clip).filter(|c| plays(c));
        let shipped = read.shipped.as_ref().and_then(|s| s.as_ref().ok()).map(|s| &s.clip).filter(|c| plays(c));
        let bones = ranked_by(&read)?;
        let key = (item.dir.clone(), index);
        let heroes = self.heroes(game, &key, bones).ok()?.ok()?;
        let hero = self.chosen(&key, &heroes)?;
        if yours.is_none() && shipped.is_none() {
            return None;
        }
        let clip_id = format!("{}|{index}|{}", item.dir.display(), stamp(&item.dir, &f, game));
        Some(Want {
            id: format!("{clip_id}|{hero:08X}"),
            clip_id,
            hero,
            hero_label: name_of(hero, names),
            yours: yours.map(|c| (c.clip.clone(), c.trnm.clone())),
            shipped: shipped.map(|c| (c.clip.clone(), c.trnm.clone())),
            name_hash: asset_hash(&f.name),
            label: f.name,
        })
    }
}

/// The bones the heroes are ranked by: the item's clip's, else the shipped clip's.
fn ranked_by(read: &Read) -> Option<&Vec<u32>> {
    let yours = read.yours.as_ref().ok().and_then(|y| y.clip.bones.as_ref().ok());
    yours.or_else(|| read.shipped.as_ref()?.as_ref().ok()?.clip.bones.as_ref().ok())
}

/// The models of `sk` whose skeleton holds any of `bones`, most first, then by hash.
fn rank(sk: &Skeletons, bones: &[u32]) -> Vec<(u32, usize)> {
    let mut out: Vec<(u32, usize)> = sk.bones.keys().filter_map(|&m| sk.coverage(m, bones).filter(|&n| n > 0).map(|n| (m, n))).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn name_of(m: u32, names: Option<&NameTable>) -> String {
    names.and_then(|n| n.reverse(m)).map_or_else(|| format!("0x{m:08X}"), str::to_string)
}

/// The preview of contribution `index` of `item`.
pub fn editor(ui: &mut egui::Ui, a: &Anim, item: &Item, index: usize, game: &[PathBuf], names: Option<&NameTable>) -> Vec<Ask> {
    let mut asks = Vec::new();
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return asks };
    let Some(f) = files(c) else { return asks };
    theme::section(ui, "Preview", Some(c.kind()), true, |ui| {
        let read = match a.read(game, item, index) {
            None => {
                ui.label(egui::RichText::new("Reading the clip\u{2026}").color(theme::DIM));
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Some(Err(e)) => return theme::field_note(ui, FieldState::Bad, &e),
            Some(Ok(r)) => r,
        };
        if read.shipped.is_some() {
            theme::eyebrow(ui, "Yours");
        }
        match &read.yours {
            Ok(y) => {
                clip_facts(ui, &y.clip);
                for p in &y.problems {
                    theme::field_note(ui, FieldState::Bad, &format!("M0213: {p}"));
                }
            }
            Err(e) => theme::field_note(ui, FieldState::Bad, e),
        }
        if let Some(s) = &read.shipped {
            ui.add_space(4.0);
            theme::eyebrow(ui, "Shipped");
            match s {
                Ok(s) => {
                    clip_facts(ui, &s.clip);
                    if read.yours.is_err() && ui.button("Start from the shipped clip").clicked() {
                        asks.push(start_shipped(item, index, &f, s));
                    }
                }
                Err(e) => theme::field_note(ui, FieldState::Bad, e),
            }
        }
        let Some(bones) = ranked_by(&read) else { return };
        let key = (item.dir.clone(), index);
        ui.add_space(6.0);
        let hero = hero_picker(ui, a, game, &key, bones, names, &mut asks);
        ui.add_space(6.0);
        let yours = read.yours.as_ref().ok();
        if a.focused(item) != Some(index) {
            if ui.button("Play in the viewport").clicked() {
                asks.push(Ask::Focus { item: item.dir.clone(), index });
            }
        } else if hero.is_some() {
            let events = yours.and_then(|y| y.events.as_ref()).and_then(|(_, _, e)| e.as_ref().ok());
            transport(ui, a, item, game, names, events, &mut asks);
        }
        if let Some(h) = hero {
            binding(ui, a, game, bones, h, names);
        }
        if let Some(y) = yours {
            events(ui, item, index, c, &f, y, &mut asks);
        }
    });
    asks
}

/// What a clip reads as.
fn clip_facts(ui: &mut egui::Ui, c: &Clip) {
    match &c.facts {
        Err(e) => theme::field_note(ui, FieldState::Bad, &format!("the clip does not read: {e}")),
        Ok(x) => {
            theme::kv(ui, "class", egui::RichText::new(&x.class));
            theme::kv(ui, "duration", egui::RichText::new(format!("{:.3} s", x.duration)));
            theme::kv(ui, "frames", egui::RichText::new(x.frames.to_string()));
            theme::kv(ui, "tracks", egui::RichText::new(x.tracks.to_string()));
            theme::kv(ui, "decodes", egui::RichText::new(if x.decoded { "yes" } else { "no" }));
        }
    }
    if let Err(e) = &c.bones {
        theme::field_note(ui, FieldState::Bad, &format!("the trnm does not read: {e}"));
    }
}

/// The shipped clip's files written into the item's `src/` under the target's name, and the replace
/// pointed at them.
fn start_shipped(item: &Item, index: usize, f: &Files, s: &Shipped) -> Ask {
    let stem: String = f.name.chars().map(|ch| if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' { ch } else { '_' }).collect();
    let src = |ext: &str| PathBuf::from("src").join(format!("{stem}.{ext}"));
    let mut files = vec![(src("hkx"), s.clip.clip.clone()), (src("trnm"), s.clip.trnm.clone())];
    if let Some(e) = &s.events {
        files.push((src("evnt"), Arc::new(e.clone())));
    }
    let c = Contribution::ReplaceAnimation {
        target: f.name.clone(),
        clip: src("hkx"),
        trnm: src("trnm"),
        events: s.events.as_ref().map(|_| src("evnt")),
    };
    Ask::StartShipped { item: item.dir.clone(), index, c: Box::new(c), files }
}

/// Write each of `files` under `dir`; refused, writing none, when any is already there.
pub fn write_new(dir: &Path, files: &[(PathBuf, Arc<Vec<u8>>)]) -> Result<(), String> {
    if let Some((p, _)) = files.iter().find(|(p, _)| dir.join(p).exists()) {
        return Err(format!("{} already exists; it is left as it is", dir.join(p).display()));
    }
    for (p, bytes) in files {
        let path = dir.join(p);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, bytes.as_slice()).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// The models ranked by how many of the clip's bones their skeleton holds, and the one chosen.
fn hero_picker(
    ui: &mut egui::Ui,
    a: &Anim,
    game: &[PathBuf],
    key: &(PathBuf, usize),
    bones: &[u32],
    names: Option<&NameTable>,
    asks: &mut Vec<Ask>,
) -> Option<u32> {
    let heroes = match a.heroes(game, key, bones) {
        Err(at) => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(egui::RichText::new(format!("Reading every model's skeleton \u{b7} {:.0}%", at * 100.0)).color(theme::DIM));
            });
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            return None;
        }
        Ok(Err(e)) => {
            theme::field_note(ui, FieldState::Bad, &format!("the skeletons do not read: {e}"));
            return None;
        }
        Ok(Ok(h)) => h,
    };
    let chosen = a.chosen(key, &heroes);
    let Some(hero) = chosen else {
        theme::field_note(ui, FieldState::Bad, "no model's skeleton holds any bone the trnm binds");
        return None;
    };
    let covered = heroes.iter().find(|(m, _)| *m == hero).map_or(0, |(_, n)| *n);
    theme::kv(ui, "hero", egui::RichText::new(format!("{} \u{b7} {covered}/{} bones", name_of(hero, names), bones.len())));
    egui::CollapsingHeader::new(format!("Heroes \u{b7} {}", heroes.len())).id_salt(("depot-anim-heroes", key)).show(ui, |ui| {
        let mut filter = a.filter.borrow_mut();
        ui.add(egui::TextEdit::singleline(&mut *filter).hint_text("filter").desired_width(f32::INFINITY));
        let q = filter.to_lowercase();
        theme::scroll_vertical().id_salt(("depot-anim-hero-list", key)).max_height(160.0).auto_shrink([false, true]).show(ui, |ui| {
            for (m, n) in heroes.iter().filter(|(m, _)| q.is_empty() || name_of(*m, names).to_lowercase().contains(&q)) {
                ui.horizontal(|ui| {
                    if ui.selectable_label(*m == hero, egui::RichText::new(name_of(*m, names)).color(theme::TX)).clicked() && *m != hero {
                        asks.push(Ask::Hero { item: key.0.clone(), index: key.1, model: *m });
                    }
                    ui.label(egui::RichText::new(format!("{n}/{}", bones.len())).monospace().small().color(theme::DIM));
                });
            }
        });
    });
    Some(hero)
}

/// The viewport's clock for the clip, with the events marked on the scrub.
fn transport(ui: &mut egui::Ui, a: &Anim, item: &Item, game: &[PathBuf], names: Option<&NameTable>, events: Option<&Vec<AnimEvent>>, asks: &mut Vec<Ask>) {
    let want_id = a.want(game, item, names).map(|w| w.id);
    let live = a.live.borrow().clone().filter(|l| Some(&l.id) == want_id.as_ref());
    let Some(live) = live else {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new());
            ui.label(egui::RichText::new("Loading the hero in the viewport\u{2026}").color(theme::DIM));
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        return;
    };
    if let Some(Err(e)) = &live.yours {
        theme::field_note(ui, FieldState::Bad, e);
    }
    match &live.shipped {
        None => {}
        Some(Ok(())) => theme::field_note(ui, FieldState::Neutral, "yours at the centre; the shipped clip on the hero beside it"),
        Some(Err(e)) => theme::field_note(ui, FieldState::Bad, e),
    }
    ui.horizontal(|ui| {
        if ui.button(if live.playing { "Pause" } else { "Play" }).clicked() {
            asks.push(Ask::Play(if live.playing { AnimAct::Pause } else { AnimAct::Play }));
        }
        if ui.button("Restart").clicked() {
            asks.push(Ask::Play(AnimAct::Restart));
        }
        if theme::pill(ui, "Loop", live.looping).clicked() {
            asks.push(Ask::Play(AnimAct::Loop(!live.looping)));
        }
    });
    let width = (ui.available_width() - 96.0).max(0.0);
    let mut t = live.time.min(live.duration);
    let r = theme::scrub(ui, width, crate::vfxview::scrub_slider(&mut t, live.duration));
    if r.changed() {
        asks.push(Ask::Play(AnimAct::Seek(t)));
    }
    if let Some(events) = events.filter(|e| !e.is_empty()) {
        let (x0, y) = (r.rect.left(), r.rect.bottom());
        let p = ui.painter();
        for e in events {
            let x = x0 + width * (e.time / live.duration.max(1e-3)).clamp(0.0, 1.0);
            p.line_segment([egui::pos2(x, y - 4.0), egui::pos2(x, y + 2.0)], egui::Stroke::new(1.5, theme::BRASS));
        }
    }
}

/// Each track's bone, and whether the hero's skeleton holds it.
fn binding(ui: &mut egui::Ui, a: &Anim, game: &[PathBuf], bones: &[u32], hero: u32, names: Option<&NameTable>) {
    let Ok(Ok(sk)) = a.skeletons(game) else { return };
    let held = sk.bones.get(&hero);
    let missing = bones.iter().filter(|b| !held.is_some_and(|h| h.contains(b))).count();
    egui::CollapsingHeader::new(format!("Bone binding \u{b7} {} tracks \u{b7} {missing} not on the hero", bones.len())).id_salt(("depot-anim-binding", hero)).show(ui, |ui| {
        theme::scroll_vertical().id_salt("depot-anim-binding-list").max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
            egui::Grid::new("depot-anim-binding-grid").num_columns(3).striped(true).spacing([10.0, 2.0]).show(ui, |ui| {
                for (t, b) in bones.iter().enumerate() {
                    let on = held.is_some_and(|h| h.contains(b));
                    ui.label(egui::RichText::new(t.to_string()).monospace().color(theme::DIM));
                    ui.label(egui::RichText::new(name_of(*b, names)).monospace().color(theme::TX));
                    ui.label(egui::RichText::new(if on { "on the hero" } else { "not on the hero" }).small().color(if on { theme::DIM } else { theme::BAD }));
                    ui.end_row();
                }
            });
        });
    });
}

/// The events file as rows: time, name and category, each removable and movable, and a row to add.
fn events(ui: &mut egui::Ui, item: &Item, index: usize, c: &Contribution, f: &Files, read: &Yours, asks: &mut Vec<Ask>) {
    theme::section(ui, "Events", read.events.as_ref().map(|(_, _, e)| e.as_ref().map_or(0, Vec::len).to_string()).as_deref(), true, |ui| {
        let Some((path, prior, parsed)) = &read.events else {
            if ui.button("+ Add events").clicked() {
                let path = f.clip.with_extension("evnt");
                let mut edited = c.clone();
                if let Contribution::AddAnimation { events, .. } | Contribution::ReplaceAnimation { events, .. } = &mut edited {
                    *events = Some(path.clone());
                }
                asks.push(Ask::AddEvents { item: item.dir.clone(), path: item.dir.join(&path), index, c: Box::new(edited) });
            }
            return;
        };
        let mut rows = match parsed {
            Ok(e) => e.clone(),
            Err(e) => return theme::field_note(ui, FieldState::Bad, &format!("{}: {e}", path.display())),
        };
        let before = rows.clone();
        let blank = || AnimEvent { time: 0.0, name: String::new(), category: String::new() };
        crate::kindform::list(ui, "event", &mut rows, blank, |ui, _, e| {
            let mut changed = false;
            ui.horizontal(|ui| {
                theme::field_label(ui, "Time");
                changed |= ui.add(egui::DragValue::new(&mut e.time).speed(0.01).range(0.0..=f32::MAX).suffix(" s")).changed();
            });
            ui.horizontal(|ui| {
                theme::field_label(ui, "Name");
                changed |= ui.add(egui::TextEdit::singleline(&mut e.name).hint_text("opendoor").desired_width(f32::INFINITY)).changed();
            });
            ui.horizontal(|ui| {
                theme::field_label(ui, "Category");
                changed |= ui.add(egui::TextEdit::singleline(&mut e.category).hint_text("sound").desired_width(f32::INFINITY)).changed();
            });
            changed
        });
        if rows != before {
            asks.push(Ask::Events { path: item.dir.join(path), prior: prior.clone(), events: rows });
        }
    });
}

/// Write `events` over `path` when it still holds `prior`; refused, the file left as it is, when it
/// changed on disk since it was read.
pub fn write_events(path: &Path, prior: &[u8], events: &[AnimEvent]) -> Result<String, String> {
    let now = read_file(path)?;
    if now != prior {
        return Err(format!("{} changed on disk since it was read; it is left as it is", path.display()));
    }
    let bytes = build_evnt(events)?;
    std::fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("Wrote {} event(s) to {}", events.len(), path.display()))
}

/// Create the empty events file `path`; refused when a file is already there.
pub fn create_events(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} already exists; name it in Events instead", path.display()));
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, build_evnt(&[])?).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depotanim_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn events_are_written_as_rows_and_a_stale_write_is_refused() {
        let dir = temp("events");
        let path = dir.join("src/c.evnt");
        create_events(&path).unwrap();
        assert!(create_events(&path).unwrap_err().contains("already exists"));
        let prior = std::fs::read(&path).unwrap();
        let rows = vec![AnimEvent { time: 0.2, name: "opendoor".into(), category: String::new() }];
        write_events(&path, &prior, &rows).unwrap();
        assert_eq!(parse_evnt(&std::fs::read(&path).unwrap()).unwrap(), rows);
        let e = write_events(&path, &prior, &[]).unwrap_err();
        assert!(e.contains("changed on disk"), "{e}");
        assert_eq!(parse_evnt(&std::fs::read(&path).unwrap()).unwrap(), rows);
    }

    /// Starting a replace from the shipped clip writes its files and refuses, writing none, when one
    /// is already there.
    #[test]
    fn the_shipped_clip_is_written_only_into_new_files() {
        let dir = temp("start");
        let files = vec![(PathBuf::from("src/a.hkx"), Arc::new(vec![1u8, 2])), (PathBuf::from("src/a.trnm"), Arc::new(vec![3u8]))];
        write_new(&dir, &files).unwrap();
        assert_eq!(std::fs::read(dir.join("src/a.trnm")).unwrap(), [3]);
        std::fs::remove_file(dir.join("src/a.hkx")).unwrap();
        let e = write_new(&dir, &files).unwrap_err();
        assert!(e.contains("already exists"), "{e}");
        assert!(!dir.join("src/a.hkx").exists(), "nothing is written when one file is there");
    }

    #[test]
    fn heroes_rank_by_how_many_of_the_clips_bones_they_hold() {
        let mut sk = Skeletons::default();
        sk.bones.insert(3, vec![10, 11, 12]);
        sk.bones.insert(1, vec![10, 11]);
        sk.bones.insert(2, vec![10, 11, 99]);
        sk.bones.insert(4, vec![99]);
        assert_eq!(rank(&sk, &[10, 11, 12]), vec![(3, 3), (1, 2), (2, 2)]);
    }
}
