//! The Depot page: every item in the workspace, the selected one in full, and the New item dialog
//! (`docs/modernization/workshop_pages/depot.md`, boards `DepotBrowser` and `ShipmentCompose`).
//!
//! The page is drawn from `&Panel` and returns [`Act`]s, as every other page is; [`Panel::apply`]
//! carries them out. Choosing the workspace and opening Settings change state the app owns, so they
//! come back from `apply` for the app to act on.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{Color32, TextStyle};
use mercs2_quartermaster::lint::Severity;
use mercs2_quartermaster::names::NameTable;

use crate::depot::{self, Item, State, Workspace};
use crate::gui::theme;

/// Which items the browser lists, and how.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sort {
    Modified,
    Name,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Tiles,
    List,
}

/// The New item dialog, while it is open.
#[derive(Clone, Debug)]
pub struct NewItem {
    pub search: String,
    pub kind: &'static str,
    pub name: String,
    /// The workspace folder it goes in; empty for the workspace root.
    pub folder: String,
    pub error: Option<String>,
}

/// A name prompt for Rename or Duplicate.
#[derive(Clone, Debug)]
pub struct NamePrompt {
    pub dir: PathBuf,
    pub duplicate: bool,
    pub name: String,
    pub error: Option<String>,
}

/// The append being edited by hand: the item, the file, the text it held when the edit began, and
/// the text so far.
#[derive(Clone, Debug)]
pub struct HandEdit {
    pub dir: PathBuf,
    pub file: PathBuf,
    pub prior: String,
    pub text: String,
    pub error: Option<String>,
}

/// Each item `PHY2` file read, by path: its modified time when read and the collision it read as.
type YoursCache = std::collections::HashMap<PathBuf, (Option<SystemTime>, Result<std::rc::Rc<crate::collision::Collision>, String>)>;

#[derive(Default)]
pub struct Panel {
    /// The workspace this page last read, and what it held; `None` until one is chosen.
    pub ws: Option<Result<Workspace, String>>,
    pub selected: Option<PathBuf>,
    /// The item open in its editor; `None` is All items.
    pub open: Option<PathBuf>,
    /// The navigator's filter and state chip while an item is open (`DepotNav`).
    pub nav_filter: String,
    pub nav_state: Option<State>,
    pub hand: Option<HandEdit>,
    /// What a source GLB is, read once per path (the outfit and model forms show it).
    facts: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<crate::quartermaster::GlbFacts>>>,
    /// The spawn-list calls of each shipped script, read once per script.
    spawn_calls: std::cell::RefCell<std::collections::HashMap<String, Result<Vec<mercs2_quartermaster::spawn_lists::SpawnCall>, String>>>,
    /// Every spawn list the shipped Lua sets, read once.
    spawn_names: std::cell::OnceCell<Vec<String>>,
    /// The game stack the shipped assets are read from (the app's WADs, in mount order).
    game: Vec<PathBuf>,
    /// Shipped `PHY2` bodies, by target.
    shipped: std::cell::RefCell<std::collections::HashMap<String, Shipped>>,
    /// The item's own `PHY2`, by file, with the modification time it was read at.
    yours: std::cell::RefCell<YoursCache>,
    /// The destruction-states editor's reads and selection.
    pub states: crate::depotstates::States,
    /// The string-table editors' reads.
    pub text: crate::depottext::Text,
    /// The world editors' reads and selection.
    pub world: crate::depotworld::Worlds,
    /// The stand-in previews' reads and choices.
    pub tiny: crate::depottiny::Tiny,
    /// The sound previews' reads, and what the Depot's player plays.
    pub sound: crate::depotsound::Sound,
    /// The texture previews' reads, uploads and choices.
    pub tex: crate::depottex::Tex,
    /// The effect editors' reads and choices, and the transport of what the viewport runs.
    pub fx: crate::depotfx::Fx,
    /// The animation editors' reads and choices, and what the viewport plays.
    pub anim: crate::depotanim::Anim,
    /// Viewport: hide the collision outline; draw the shipped one rather than the item's.
    pub hide_collision: bool,
    pub show_shipped: bool,
    pub search: String,
    /// `None` is All folders.
    pub folder: Option<String>,
    pub hide_draft: bool,
    pub hide_ready: bool,
    pub kinds_off: BTreeSet<&'static str>,
    pub sort: Option<Sort>,
    pub view: Option<View>,
    pub new_item: Option<NewItem>,
    pub prompt: Option<NamePrompt>,
    pub confirm_delete: Option<PathBuf>,
    /// The last action's outcome, for the status line.
    pub status: Option<Result<String, String>>,
}

#[derive(Clone, Debug)]
pub enum Act {
    Rescan,
    Select(PathBuf),
    Search(String),
    Folder(Option<String>),
    ToggleState(State),
    ToggleKind(&'static str),
    Sort(Sort),
    View(View),
    SetState(PathBuf, State),
    Reveal(PathBuf),
    OpenNew,
    NewSearch(String),
    NewKind(&'static str),
    NewName(String),
    NewFolder(String),
    Create,
    CloseNew,
    Prompt { dir: PathBuf, duplicate: bool },
    PromptName(String),
    PromptCommit,
    PromptClose,
    AskDelete(PathBuf),
    Delete(PathBuf),
    CloseDelete,
    Open(PathBuf),
    Close,
    NavFilter(String),
    NavState(Option<State>),
    /// Replace contribution `index` of the item.
    Edit { dir: PathBuf, index: usize, c: Box<mercs2_quartermaster::manifest::Contribution> },
    ToggleCollision,
    ShowShipped(bool),
    /// Write the target's shipped `PHY2` into the item's `src/` and point contribution `index` at it.
    SaveShipped { dir: PathBuf, index: usize, target: String },
    HandStart { dir: PathBuf, file: PathBuf },
    HandText(String),
    HandSave,
    HandCancel,
    /// Select a switch node, then a state of it, of the open item's machine.
    SmNode(usize),
    SmState(usize),
    /// Write `machine` over the item's `states:` file `path`, whose text was `prior`.
    SmWrite { path: PathBuf, prior: String, machine: Box<mercs2_formats::orchestrator::StateMachine> },
    /// Write the target's shipped machine into the item's `src/` and point contribution `index` at it.
    SmStartShipped { dir: PathBuf, index: usize, target: String },
    /// Show the edited machine on the viewport's model, each node at the state in `node_state`.
    SmPreview { hash: u32, machine: Box<mercs2_formats::orchestrator::StateMachine>, node_state: Vec<usize> },
    /// A state-machine edit the editor could not make, for the status line.
    SmError(String),
    /// Write a string-table editor's file: `text` over `path`, refused when the file is no longer
    /// `prior`, the text the edit was made from.
    TextWrite { path: PathBuf, prior: String, text: String },
    /// Select record `row` of the world file `path`.
    WorldSelect { path: PathBuf, row: usize },
    /// Set the filter of the world file `path`'s entity list.
    WorldFilter { path: PathBuf, filter: String },
    /// Draw the shipped layer rather than the built one.
    WorldShipped(bool),
    /// Draw layer `index` of the `activate_layer` item at `item`.
    WorldShowLayer { item: PathBuf, index: usize },
    /// What the viewport draws of the stand-in item at `.0`.
    TinyShow(PathBuf, crate::depottiny::Shown),
    /// Tint each stand-in object's part by its slot.
    TinyTint(bool),
    /// A world edit the editor could not make, for the status line.
    WorldError(String),
    /// Open row `guid` of bank view `bank` of contribution `index` of the sound item at `item`.
    SoundOpen { item: PathBuf, index: usize, bank: usize, guid: u32 },
    /// Handled by the app: play, pause or resume the sound preview's wave `.0`.
    SoundPlay(u32),
    /// What the texture viewer of contribution `index` of the item at `item` shows.
    TexChoose { item: PathBuf, index: usize, choice: crate::depottex::Choice },
    /// The model the still render of contribution `index` of the item at `item` draws.
    TexWearer { item: PathBuf, index: usize, model: u32 },
    /// Write the form of the shipped `effect` into an `add_fx`'s effect file `path`.
    FxStart { path: PathBuf, effect: String },
    /// Run the shipped or the built effect of contribution `index` of the item at `item`.
    FxSide { item: PathBuf, index: usize, side: crate::depotfx::Side },
    /// Show `node` of the effect of contribution `index` of the item at `item`.
    FxNode { item: PathBuf, index: usize, node: crate::depotfx::Node },
    /// Handled by the app: the transport of the effect the viewport runs.
    FxPlay(crate::vfxview::VfxAct),
    /// An effect edit the editor could not make, for the status line.
    FxError(String),
    /// The hero the clip of contribution `index` of the item at `item` plays on.
    AnimHero { item: PathBuf, index: usize, model: u32 },
    /// Play the clip of contribution `index` of the item at `item` in the viewport.
    AnimFocus { item: PathBuf, index: usize },
    /// Handled by the app: the transport of the clip the viewport plays.
    AnimPlay(crate::depotanim::AnimAct),
    /// Write `events` over the events file `path`, whose bytes were `prior`.
    AnimEvents { path: PathBuf, prior: Vec<u8>, events: Vec<mercs2_formats::anim_container::AnimEvent> },
    /// Create the empty events file `path` and replace contribution `index` of the item at `item`
    /// with `c`, which names it.
    AnimAddEvents { item: PathBuf, path: PathBuf, index: usize, c: Box<mercs2_quartermaster::manifest::Contribution> },
    /// Write `files` (each new) into the item at `item` and replace its contribution `index` with
    /// `c`, which names them.
    AnimStartShipped {
        item: PathBuf,
        index: usize,
        c: Box<mercs2_quartermaster::manifest::Contribution>,
        files: Vec<(PathBuf, std::sync::Arc<Vec<u8>>)>,
    },
    /// Handled by the app: pick and save the workspace folder.
    ChooseWorkspace,
    /// Handled by the app: open Settings › Depot workspace.
    OpenSettings,
}

/// What [`Panel::apply`] hands back for the app to do.
pub enum AppAct {
    ChooseWorkspace,
    OpenSettings,
    /// Load this model into the viewport: the subject of the item just opened.
    LoadModel(u32, String),
    /// Run the viewport model `.0` through this machine, each node at the state given.
    PreviewMachine(u32, Box<mercs2_formats::orchestrator::StateMachine>, Vec<usize>),
    /// Play, pause or resume this wave of a sound preview on the Depot's player.
    PlaySound(crate::depotsound::Wave),
    /// The transport of the effect the Depot's viewport runs.
    FxPlay(crate::vfxview::VfxAct),
    /// The transport of the clip the Depot's viewport plays.
    AnimPlay(crate::depotanim::AnimAct),
}

/// A shipped `PHY2` body and the collision it reads as, or why it could not be read.
type ShippedRead = Result<std::rc::Rc<(Vec<u8>, crate::collision::Collision)>, String>;

/// A shipped `PHY2`, read on a worker the first time an item asks for it.
enum Shipped {
    Loading(std::sync::mpsc::Receiver<Result<Vec<u8>, String>>),
    Ready(Result<std::rc::Rc<(Vec<u8>, crate::collision::Collision)>, String>),
}

/// The model an item's editor shows in the viewport, by name: the target of a `replace_phy2` or of an
/// `edit_state_machine`.
fn viewport_subject(item: &Item) -> Option<&str> {
    match item.file.as_ref().ok()?.contributions.first()? {
        mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { target, .. }
        | mercs2_quartermaster::manifest::Contribution::EditStateMachine { target, .. } => Some(target),
        _ => None,
    }
}

impl Panel {
    pub fn sort(&self) -> Sort {
        self.sort.unwrap_or(Sort::Modified)
    }

    pub fn view(&self) -> View {
        self.view.unwrap_or(View::Tiles)
    }

    pub fn workspace(&self) -> Option<&Workspace> {
        self.ws.as_ref().and_then(|w| w.as_ref().ok())
    }

    /// The game stack shipped assets are read from, and the one every check and preview build runs
    /// against: the shipped game's own WADs, never a mod's overlay (`vz-patch.wad` among them). A
    /// different stack forgets what was read from the old one.
    pub fn set_game(&mut self, wads: &[String]) {
        let wads: Vec<PathBuf> = wads.iter().map(PathBuf::from).collect();
        if wads != self.game {
            self.game = wads;
            self.shipped.borrow_mut().clear();
            self.states.forget_game();
            self.text.forget_game();
            self.world.forget_game();
            self.tiny.forget_game();
            self.sound.forget_game();
            self.tex.forget_game();
            self.fx.forget_game();
            self.anim.forget_game();
        }
    }

    /// The open item, when its editor shows a model, a layer or an effect in the viewport.
    pub fn viewport_item(&self) -> Option<&Item> {
        let d = self.open.as_ref()?;
        let item = self.workspace()?.items.iter().find(|i| &i.dir == d)?;
        let shows = viewport_subject(item).is_some()
            || crate::depotworld::draws(item)
            || crate::depottiny::of(item).is_some()
            || crate::depotfx::of_item(item)
            || item.file.as_ref().is_ok_and(|f| f.contributions.iter().any(crate::depotanim::of));
        shows.then_some(item)
    }

    /// What the viewport plays for an open animation item: `None` for any other item, and while its
    /// clip or the skeletons are read, or when the clip cannot play.
    pub fn anim_want(&self, names: Option<&NameTable>) -> Option<crate::depotanim::Want> {
        let item = self.viewport_item()?;
        self.anim.want(&self.game, item, names)
    }

    /// What the viewport runs for an open effect item: `None` for any other item, and while the
    /// effect builds or when the build ships none.
    pub fn fx_want(&self) -> Option<crate::depotfx::Want> {
        let item = self.viewport_item()?;
        self.fx.want(&self.game, item)
    }

    /// What the viewport draws for an open world item, or a stand-in item's objects: `None` for any
    /// other item, and while the layer is read or built.
    pub fn world_view(&self) -> Option<crate::depotworld::View> {
        let item = self.viewport_item()?;
        if crate::depottiny::of(item).is_some() {
            return self.tiny.objects_view(&self.game, item);
        }
        self.world.view(&self.game, item)
    }

    /// Whether the open item's viewport draws a layer: a world item, or a stand-in item's objects.
    pub fn layer_open(&self) -> bool {
        self.viewport_item().is_some_and(|i| {
            crate::depotworld::draws_layer(i)
                || (crate::depottiny::of(i).is_some() && self.tiny.shown_of(i) == crate::depottiny::Shown::Objects)
        })
    }

    /// What the viewport draws of an open stand-in item's model: `None` for any other item, for the
    /// objects view, and while it builds.
    pub fn stand_in_view(&self) -> Option<crate::depottiny::StandInView> {
        let item = self.viewport_item()?;
        self.tiny.stand_in_view(&self.game, item)
    }

    /// What the viewport draws for an open `replace_terrain_cell` item: `None` for any other item, and
    /// while the cell is read.
    pub fn terrain_view(&self) -> Option<crate::depotworld::TerrainView> {
        let item = self.viewport_item()?;
        self.world.terrain_view(&self.game, item)
    }

    /// What a click on entity `key` in an open world item's viewport does: select its record, or
    /// filter the layer's entity list to it.
    pub fn world_pick(&self, key: u32) -> Option<Act> {
        let item = self.viewport_item()?;
        Some(match self.world.pick(&self.game, item, key)? {
            (path, Ok(row)) => Act::WorldSelect { path, row },
            (path, Err(filter)) => Act::WorldFilter { path, filter },
        })
    }

    /// The shipped `PHY2` of `target`: `None` while it is read on a worker (started on first ask).
    fn shipped_for(&self, target: &str) -> Option<ShippedRead> {
        let mut map = self.shipped.borrow_mut();
        let entry = map.entry(target.to_string()).or_insert_with(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let (wads, t) = (self.game.clone(), target.to_string());
            std::thread::spawn(move || {
                let _ = tx.send(if wads.is_empty() {
                    Err("no game archive is open".to_string())
                } else {
                    crate::collision::shipped_body(&wads, &t)
                });
            });
            Shipped::Loading(rx)
        });
        if let Shipped::Loading(rx) = entry {
            match rx.try_recv() {
                Ok(body) => {
                    let read = body.and_then(|b| crate::collision::read(&b).map(|c| std::rc::Rc::new((b, c))));
                    *entry = Shipped::Ready(read);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    *entry = Shipped::Ready(Err("the reader stopped without an answer".into()));
                }
            }
        }
        match entry {
            Shipped::Ready(r) => Some(r.clone()),
            Shipped::Loading(_) => None,
        }
    }

    /// The item's own `PHY2` at `abs`: `None` when the file does not exist. Re-read when it changes.
    fn yours_for(&self, abs: &Path) -> Option<Result<std::rc::Rc<crate::collision::Collision>, String>> {
        let mtime = std::fs::metadata(abs).and_then(|m| m.modified()).ok()?;
        if let Some((t, r)) = self.yours.borrow().get(abs) {
            if *t == Some(mtime) {
                return Some(r.clone());
            }
        }
        let r = std::fs::read(abs)
            .map_err(|e| format!("{}: {e}", abs.display()))
            .and_then(|b| crate::collision::read(&b))
            .map(std::rc::Rc::new);
        self.yours.borrow_mut().insert(abs.to_path_buf(), (Some(mtime), r.clone()));
        Some(r)
    }

    fn save_shipped(&self, dir: &Path, index: usize, target: &str) -> Result<String, String> {
        let shipped = self.shipped_for(target).ok_or("the shipped collision is still being read")??;
        let item = self
            .workspace()
            .and_then(|ws| ws.items.iter().find(|i| i.dir == dir))
            .ok_or_else(|| format!("{} is no longer in the workspace", dir.display()))?;
        let mut f = item.file.clone()?;
        let rel = PathBuf::from("src").join(format!("{}.phy2", target.replace(['/', '\\', ':'], "_")));
        let slot = f.contributions.get_mut(index).ok_or_else(|| format!("{} has no contribution {index}", item.name))?;
        let mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { phy2, .. } = slot else {
            return Err(format!("contribution {index} of {} is not a replace_phy2", item.name));
        };
        depot::unchanged(item)?;
        let abs = dir.join(&rel);
        if abs.exists() {
            return Err(format!("{} exists; point the item at it, or move it first", rel.display()));
        }
        write_bytes(&abs, &shipped.0)?;
        *phy2 = rel.clone();
        depot::rewrite_item(item, &f)?;
        Ok(format!("Saved the shipped collision as {}", rel.display()))
    }

    /// Write `target`'s shipped machine as `src/<target>.states.yaml` (qm's `states::extract`, as
    /// `qm extract-states` writes it) and point contribution `index` at it.
    fn start_states_from_shipped(&self, dir: &Path, index: usize, target: &str, names: Option<&NameTable>) -> Result<String, String> {
        let shipped = self.states.shipped(&self.game, target).ok_or("the shipped machine is still being read")??;
        let item = self
            .workspace()
            .and_then(|ws| ws.items.iter().find(|i| i.dir == dir))
            .ok_or_else(|| format!("{} is no longer in the workspace", dir.display()))?;
        let mut f = item.file.clone()?;
        let rel = PathBuf::from("src").join(format!("{}.states.yaml", target.replace(['/', '\\', ':'], "_")));
        let slot = f.contributions.get_mut(index).ok_or_else(|| format!("{} has no contribution {index}", item.name))?;
        let mercs2_quartermaster::manifest::Contribution::EditStateMachine { states, .. } = slot else {
            return Err(format!("contribution {index} of {} is not an edit_state_machine", item.name));
        };
        depot::unchanged(item)?;
        let abs = dir.join(&rel);
        if abs.exists() {
            return Err(format!("{} exists; point the item at it, or move it first", rel.display()));
        }
        let text = crate::depotstates::checked_text(&shipped.machine, "", names)?;
        write_bytes(&abs, text.as_bytes())?;
        *states = rel.clone();
        depot::rewrite_item(item, &f)?;
        Ok(format!("Wrote the shipped machine as {}", rel.display()))
    }

    fn facts_for(&self, abs: &Path) -> Option<crate::quartermaster::GlbFacts> {
        if let Some(hit) = self.facts.borrow().get(abs) {
            return hit.clone();
        }
        let f = crate::quartermaster::probe_glb(abs);
        self.facts.borrow_mut().insert(abs.to_path_buf(), f.clone());
        f
    }

    fn spawn_calls_for(&self, target: &str, src: &str) -> Result<Vec<mercs2_quartermaster::spawn_lists::SpawnCall>, String> {
        if let Some(hit) = self.spawn_calls.borrow().get(target) {
            return hit.clone();
        }
        let c = mercs2_quartermaster::spawn_lists::calls(src);
        self.spawn_calls.borrow_mut().insert(target.to_string(), c.clone());
        c
    }

    /// The lists a row may name: those the `vz.wad` scripts set, as qm checks them.
    fn spawn_names(&self, corpus: &[(String, String, String)]) -> Vec<String> {
        if corpus.is_empty() {
            return Vec::new();
        }
        self.spawn_names.get_or_init(|| spawn_list_names(corpus)).clone()
    }

    pub fn selected_item(&self) -> Option<&Item> {
        let ws = self.workspace()?;
        let sel = self.selected.as_ref()?;
        ws.items.iter().find(|i| &i.dir == sel)
    }

    /// Re-read the workspace at `root` (or forget it when `None`).
    pub fn refresh(&mut self, root: Option<&Path>, names: Option<&NameTable>) {
        self.ws = root.map(|r| depot::scan(r, names));
        if let Some(Ok(ws)) = &self.ws {
            let gone = |d: &Option<PathBuf>| d.as_ref().is_some_and(|d| !ws.items.iter().any(|i| &i.dir == d));
            if gone(&self.selected) {
                self.selected = None;
            }
            if gone(&self.open) {
                self.open = None;
                self.hand = None;
            }
        }
    }

    /// Has the page read the workspace now set in Settings?
    pub fn is_current(&self, root: Option<&Path>) -> bool {
        match (&self.ws, root) {
            (None, None) => true,
            (Some(Ok(ws)), Some(r)) => ws.root == r,
            (Some(Err(_)), Some(_)) => true,
            _ => false,
        }
    }

    /// The items the browser lists, in its order.
    pub fn visible(&self) -> Vec<&Item> {
        let Some(ws) = self.workspace() else { return Vec::new() };
        let q = self.search.trim().to_lowercase();
        let mut out: Vec<&Item> = ws
            .items
            .iter()
            .filter(|i| self.folder.as_ref().is_none_or(|f| &i.folder == f))
            .filter(|i| match i.state() {
                Some(State::Draft) => !self.hide_draft,
                Some(State::Ready) => !self.hide_ready,
                None => true,
            })
            .filter(|i| {
                let kinds = i.kinds();
                kinds.is_empty() || kinds.iter().any(|k| !self.kinds_off.contains(k))
            })
            .filter(|i| q.is_empty() || matches_query(i, &q))
            .collect();
        match self.sort() {
            Sort::Name => out.sort_by_key(|i| i.name.to_lowercase()),
            Sort::Modified => out.sort_by_key(|i| std::cmp::Reverse(i.modified)),
        }
        out
    }

    /// Carry out `act`. Filesystem failures land in `status`; nothing is retried or skipped.
    pub fn apply(&mut self, act: Act, root: Option<&Path>, names: Option<&NameTable>) -> Option<AppAct> {
        match act {
            Act::Rescan => {
                self.refresh(root, names);
                self.status = None;
            }
            Act::Select(d) => self.selected = Some(d),
            Act::Search(s) => self.search = s,
            Act::Folder(f) => self.folder = f,
            Act::ToggleState(State::Draft) => self.hide_draft = !self.hide_draft,
            Act::ToggleState(State::Ready) => self.hide_ready = !self.hide_ready,
            Act::ToggleKind(k) => {
                if !self.kinds_off.remove(k) {
                    self.kinds_off.insert(k);
                }
            }
            Act::Sort(s) => self.sort = Some(s),
            Act::View(v) => self.view = Some(v),
            Act::SetState(dir, state) => {
                let out = match self.workspace().and_then(|ws| ws.items.iter().find(|i| i.dir == dir)) {
                    Some(item) => depot::set_state(item, state).map(|()| format!("{} is {}", item.name, state.label())),
                    None => Err(format!("{} is no longer in the workspace", dir.display())),
                };
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::Reveal(dir) => {
                if let Err(e) = crate::quartermaster::open_in_os(&dir) {
                    self.status = Some(Err(format!("Could not open {}: {e}", dir.display())));
                }
            }
            Act::OpenNew => {
                let folder = self.folder.clone().unwrap_or_default();
                self.new_item = Some(NewItem {
                    search: String::new(),
                    kind: "patch_lua",
                    name: String::new(),
                    folder,
                    error: None,
                });
            }
            Act::NewSearch(s) => {
                if let Some(n) = &mut self.new_item {
                    n.search = s;
                }
            }
            Act::NewKind(k) => {
                if let Some(n) = &mut self.new_item {
                    n.kind = k;
                }
            }
            Act::NewName(s) => {
                if let Some(n) = &mut self.new_item {
                    n.name = s;
                    n.error = None;
                }
            }
            Act::NewFolder(f) => {
                if let Some(n) = &mut self.new_item {
                    n.folder = f;
                }
            }
            Act::Create => {
                let (Some(n), Some(ws)) = (self.new_item.clone(), self.workspace()) else { return None };
                let made = crate::quartermaster::seeded(n.kind, "", 1)
                    .ok_or_else(|| format!("no stub for {}", n.kind))
                    .and_then(|c| depot::create_item(ws, &n.folder, &n.name, vec![c]));
                match made {
                    Ok(dir) => {
                        self.status = Some(Ok(format!("Made {}", dir.display())));
                        self.new_item = None;
                        self.refresh(root, names);
                        self.selected = Some(dir);
                    }
                    Err(e) => {
                        if let Some(n) = &mut self.new_item {
                            n.error = Some(e);
                        }
                    }
                }
            }
            Act::CloseNew => self.new_item = None,
            Act::Prompt { dir, duplicate } => {
                let name = file_name(&dir);
                let name = if duplicate { format!("{name}_copy") } else { name };
                self.prompt = Some(NamePrompt { dir, duplicate, name, error: None });
            }
            Act::PromptName(s) => {
                if let Some(p) = &mut self.prompt {
                    p.name = s;
                    p.error = None;
                }
            }
            Act::PromptCommit => {
                let (Some(p), Some(ws)) = (self.prompt.clone(), self.workspace()) else { return None };
                let out = if p.duplicate {
                    duplicate_item(ws, &p.dir, &p.name)
                } else {
                    rename_item(ws, &p.dir, &p.name)
                };
                match out {
                    Ok(dir) => {
                        self.status = Some(Ok(format!(
                            "{} {}",
                            if p.duplicate { "Duplicated as" } else { "Renamed to" },
                            dir.display()
                        )));
                        self.prompt = None;
                        if !p.duplicate && self.open.as_ref() == Some(&p.dir) {
                            self.open = Some(dir.clone());
                        }
                        self.refresh(root, names);
                        self.selected = Some(dir);
                    }
                    Err(e) => {
                        if let Some(p) = &mut self.prompt {
                            p.error = Some(e);
                        }
                    }
                }
            }
            Act::PromptClose => self.prompt = None,
            Act::AskDelete(d) => self.confirm_delete = Some(d),
            Act::Delete(d) => {
                self.confirm_delete = None;
                self.status = Some(
                    trash::delete(&d)
                        .map(|()| format!("Moved {} to the trash", d.display()))
                        .map_err(|e| format!("Could not move {} to the trash: {e}", d.display())),
                );
                self.refresh(root, names);
            }
            Act::CloseDelete => self.confirm_delete = None,
            Act::Open(d) => {
                self.selected = Some(d.clone());
                self.open = Some(d.clone());
                self.hand = None;
                let subject = self.workspace().and_then(|ws| ws.items.iter().find(|i| i.dir == d)).and_then(viewport_subject);
                if let Some(target) = subject {
                    let hash = mercs2_quartermaster::manifest::asset_hash(target);
                    return Some(AppAct::LoadModel(hash, target.to_string()));
                }
            }
            Act::ToggleCollision => self.hide_collision = !self.hide_collision,
            Act::ShowShipped(on) => self.show_shipped = on,
            Act::SaveShipped { dir, index, target } => {
                let out = self.save_shipped(&dir, index, &target);
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::SmNode(n) => {
                self.states.node = n;
                self.states.state = 0;
            }
            Act::SmState(s) => self.states.state = s,
            Act::SmWrite { path, prior, machine } => {
                let out = crate::depotstates::checked_text(&machine, &prior, names)
                    .and_then(|text| write_if_unchanged(&path, &prior, &text))
                    .map(|()| format!("Wrote {}", path.display()));
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::SmStartShipped { dir, index, target } => {
                let out = self.start_states_from_shipped(&dir, index, &target, names);
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::SmPreview { hash, machine, node_state } => return Some(AppAct::PreviewMachine(hash, machine, node_state)),
            Act::SmError(e) => self.status = Some(Err(e)),
            Act::WorldSelect { path, row } => {
                self.world.selected.borrow_mut().insert(path, row);
            }
            Act::WorldFilter { path, filter } => {
                self.world.filters.borrow_mut().insert(path, filter);
            }
            Act::WorldShipped(on) => self.world.shipped.set(on),
            Act::WorldShowLayer { item, index } => {
                self.world.shown_layer.borrow_mut().insert(item, index);
            }
            Act::TinyShow(item, s) => {
                self.tiny.shown.borrow_mut().insert(item, s);
            }
            Act::TinyTint(on) => self.tiny.tint.set(on),
            Act::WorldError(e) => self.status = Some(Err(e)),
            Act::SoundOpen { item, index, bank, guid } => self.sound.set_open(item, index, bank, guid),
            Act::TexChoose { item, index, choice } => self.tex.set_choice(item, index, choice),
            Act::TexWearer { item, index, model } => self.tex.set_wearer(item, index, model),
            Act::FxStart { path, effect } => {
                let out = read_or_empty(&path)
                    .and_then(|now| if now.is_empty() { Ok(()) } else { Err(format!("{} already exists; it is left as it is", path.display())) })
                    .and_then(|()| crate::depotfx::start_text(&self.game, &effect, &path))
                    .and_then(|text| write_file(&path, &text))
                    .map(|()| format!("Wrote {} from {effect}", path.display()));
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::FxSide { item, index, side } => self.fx.set_side(item, index, side),
            Act::FxNode { item, index, node } => self.fx.set_node(item, index, node),
            Act::FxPlay(a) => return Some(AppAct::FxPlay(a)),
            Act::AnimHero { item, index, model } => self.anim.set_hero(item, index, model),
            Act::AnimFocus { item, index } => self.anim.set_focus(item, index),
            Act::AnimPlay(a) => return Some(AppAct::AnimPlay(a)),
            Act::AnimEvents { path, prior, events } => {
                self.status = Some(crate::depotanim::write_events(&path, &prior, &events));
            }
            Act::AnimAddEvents { item, path, index, c } => match crate::depotanim::create_events(&path) {
                Ok(()) => return self.apply(Act::Edit { dir: item, index, c }, root, names),
                Err(e) => self.status = Some(Err(e)),
            },
            Act::AnimStartShipped { item, index, c, files } => match crate::depotanim::write_new(&item, &files) {
                Ok(()) => return self.apply(Act::Edit { dir: item, index, c }, root, names),
                Err(e) => self.status = Some(Err(e)),
            },
            Act::FxError(e) => self.status = Some(Err(e)),
            Act::SoundPlay(id) => match self.sound.wave(id) {
                Some(w) => return Some(AppAct::PlaySound(w)),
                None => self.status = Some(Err("that wave is no longer in the preview; it was rebuilt".into())),
            },
            Act::TextWrite { path, prior, text } => {
                let out = write_if_unchanged(&path, &prior, &text).map(|()| format!("Wrote {}", path.display()));
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::Close => {
                self.open = None;
                self.hand = None;
            }
            Act::NavFilter(s) => self.nav_filter = s,
            Act::NavState(s) => self.nav_state = s,
            Act::Edit { dir, index, c } => {
                let out = match self.workspace().and_then(|ws| ws.items.iter().find(|i| i.dir == dir)) {
                    None => Err(format!("{} is no longer in the workspace", dir.display())),
                    Some(item) => item.file.clone().and_then(|mut f| {
                        let slot = f.contributions.get_mut(index).ok_or_else(|| {
                            format!("{} has no contribution {index}", item.name)
                        })?;
                        *slot = *c;
                        depot::rewrite_item(item, &f).map(|()| format!("Saved {}", item.name))
                    }),
                };
                self.status = Some(out);
                self.refresh(root, names);
            }
            Act::HandStart { dir, file } => {
                let path = dir.join(&file);
                match read_or_empty(&path) {
                    Ok(text) => self.hand = Some(HandEdit { dir, file, prior: text.clone(), text, error: None }),
                    Err(e) => self.status = Some(Err(e)),
                }
            }
            Act::HandText(s) => {
                if let Some(h) = &mut self.hand {
                    h.text = s;
                    h.error = None;
                }
            }
            Act::HandSave => {
                let h = self.hand.clone()?;
                match write_if_unchanged(&h.dir.join(&h.file), &h.prior, &h.text) {
                    Ok(()) => {
                        self.status = Some(Ok(format!("Saved {}", h.file.display())));
                        self.hand = None;
                        self.refresh(root, names);
                    }
                    Err(e) => {
                        if let Some(h) = &mut self.hand {
                            h.error = Some(e);
                        }
                    }
                }
            }
            Act::HandCancel => self.hand = None,
            Act::ChooseWorkspace => return Some(AppAct::ChooseWorkspace),
            Act::OpenSettings => return Some(AppAct::OpenSettings),
        }
        None
    }
}

/// The file's text, or empty when it does not exist yet (a new item's append).
fn read_or_empty(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    write_bytes(path, text.as_bytes())
}

/// Write `text` over `path`, refused, leaving the file as it is, unless the file still holds
/// `prior`, the text the edit was made from (empty for a file that did not exist).
fn write_if_unchanged(path: &Path, prior: &str, text: &str) -> Result<(), String> {
    if read_or_empty(path)? != prior {
        return Err(format!("{} changed on disk since it was read; it is left as it is", path.display()));
    }
    write_file(path, text)
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Every spawn list the `vz.wad` scripts of the corpus set (`lua/<dir>/…` for each of
/// [`mercs2_quartermaster::link::VZ_SOURCE_DIRS`]): the set qm checks a row's list against.
fn spawn_list_names(corpus: &[(String, String, String)]) -> Vec<String> {
    let in_vz = |path: &str| mercs2_quartermaster::link::VZ_SOURCE_DIRS.iter().any(|d| path.starts_with(&format!("lua/{d}/")));
    mercs2_quartermaster::spawn_lists::list_names(corpus.iter().filter(|(p, _, _)| in_vz(p)).map(|(_, s, _)| s.as_str()))
}

fn matches_query(i: &Item, q: &str) -> bool {
    if i.name.to_lowercase().contains(q) {
        return true;
    }
    match &i.file {
        Ok(f) => f.contributions.iter().any(|c| {
            c.kind().contains(q) || crate::quartermaster::contribution_name(c).to_lowercase().contains(q)
        }),
        Err(_) => false,
    }
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Rename an item's folder in place. Refused when the name is taken anywhere in the workspace.
fn rename_item(ws: &Workspace, dir: &Path, name: &str) -> Result<PathBuf, String> {
    depot::valid_name(name)?;
    if ws.items.iter().any(|i| i.name == name) {
        return Err(format!("an item named {name} already exists"));
    }
    let to = dir.with_file_name(name);
    std::fs::rename(dir, &to).map_err(|e| format!("{} → {}: {e}", dir.display(), to.display()))?;
    Ok(to)
}

/// Copy an item beside itself under a new name, as a Draft.
fn duplicate_item(ws: &Workspace, dir: &Path, name: &str) -> Result<PathBuf, String> {
    depot::valid_name(name)?;
    if ws.items.iter().any(|i| i.name == name) {
        return Err(format!("an item named {name} already exists"));
    }
    let to = dir.with_file_name(name);
    if to.exists() {
        return Err(format!("{} already exists", to.display()));
    }
    copy_tree(dir, &to).map_err(|e| format!("{} → {}: {e}", dir.display(), to.display()))?;
    depot::make_draft(&to)?;
    Ok(to)
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let e = entry?;
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if e.file_type()?.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────── shared look

/// The stripe and dot colour of an item: red with an error, amber with a warning, green clean.
fn check_colour(ws: &Workspace, i: &Item) -> Color32 {
    if ws.has_error(i) {
        theme::BAD
    } else if i.checks.warnings() > 0 {
        theme::BRASS_DK
    } else {
        theme::GOOD
    }
}

/// The kind's title, or the kinds' titles joined, or why there is none.
fn kind_line(i: &Item) -> String {
    match &i.file {
        Err(_) => "item.yaml does not parse".into(),
        Ok(f) if f.contributions.is_empty() => "no contributions".into(),
        Ok(f) => {
            let first = f.contributions[0].kind();
            if f.contributions.len() == 1 {
                first.to_string()
            } else {
                format!("{first} +{}", f.contributions.len() - 1)
            }
        }
    }
}

/// The note under an item's name: its worst finding, else its folder.
fn note_line(ws: &Workspace, i: &Item) -> (String, Color32) {
    if let Err(e) = &i.file {
        return (e.clone(), theme::BAD);
    }
    if ws.is_duplicate(&i.name) {
        return ("another item has this name".into(), theme::BAD);
    }
    if let Some((code, title)) = i.checks.worst() {
        let c = if i.checks.errors() > 0 { theme::BAD } else { theme::BRASS_DK };
        return (format!("{code} \u{b7} {title}"), c);
    }
    let folder = if i.folder.is_empty() { "workspace root".to_string() } else { i.folder.clone() };
    (folder, theme::DIM)
}

fn state_chip_colours(s: Option<State>) -> (Color32, Color32, Color32) {
    match s {
        Some(State::Draft) => (theme::BRASS_SOFT, theme::BRASS_DK, theme::BRASS),
        Some(State::Ready) => (theme::GOOD_SOFT, theme::GOOD_DK, theme::GOOD),
        None => (theme::G0, theme::BAD, theme::BAD),
    }
}

fn state_label(s: Option<State>) -> &'static str {
    s.map_or("Broken", |s| s.label())
}

fn age(t: Option<SystemTime>) -> String {
    let Some(t) = t else { return "unknown".into() };
    let Ok(d) = SystemTime::now().duration_since(t) else { return "just now".into() };
    let s = d.as_secs();
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

/// A glyph for an item's first kind, drawn in the tile where a thumbnail is not available.
fn paint_kind_glyph(p: &egui::Painter, kind: Option<&str>, c: egui::Pos2, col: Color32) {
    use egui::vec2;
    let s = egui::Stroke::new(1.5, col);
    let k = 1.9;
    let seg = |a: (f32, f32), b: (f32, f32)| p.line_segment([c + vec2(a.0 * k, a.1 * k), c + vec2(b.0 * k, b.1 * k)], s);
    match kind {
        Some("patch_lua" | "replace_lua" | "add_script") => {
            seg((-3.0, -6.0), (-7.0, 0.0));
            seg((-7.0, 0.0), (-3.0, 6.0));
            seg((3.0, -6.0), (7.0, 0.0));
            seg((7.0, 0.0), (3.0, 6.0));
            seg((1.5, -7.0), (-1.5, 7.0));
        }
        Some("edit_world" | "add_placement" | "add_layer" | "activate_layer" | "add_tiny_geometry") => {
            for dy in [-3.4, 0.0, 3.4] {
                seg((-7.5, dy), (0.0, dy + 3.6));
                seg((0.0, dy + 3.6), (7.5, dy));
            }
        }
        Some("add_outfit" | "add_model" | "replace_phy2" | "edit_state_machine") => {
            seg((0.0, -7.0), (6.0, -3.5));
            seg((6.0, -3.5), (6.0, 3.5));
            seg((6.0, 3.5), (0.0, 7.0));
            seg((0.0, 7.0), (-6.0, 3.5));
            seg((-6.0, 3.5), (-6.0, -3.5));
            seg((-6.0, -3.5), (0.0, -7.0));
            seg((0.0, 0.0), (6.0, -3.5));
            seg((0.0, 0.0), (-6.0, -3.5));
            seg((0.0, 0.0), (0.0, 7.0));
        }
        _ => {
            p.rect_stroke(
                egui::Rect::from_center_size(c, vec2(15.0 * k, 13.0 * k)),
                1.0,
                s,
                egui::StrokeKind::Middle,
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────── navigator

/// The left column: All items' folders and filters, or, with an item open, every item (`DepotNav`).
pub fn navigator(ui: &mut egui::Ui, p: &Panel, names: Option<&NameTable>) -> Vec<Act> {
    if p.open.is_some() {
        if let Some(acts) = states_nav(ui, p, names) {
            return acts;
        }
        return depot_nav(ui, p);
    }
    let mut acts = Vec::new();
    ui.horizontal(|ui| {
        ui.label(theme::disp_text("DEPOT", theme::style_title(), theme::TX));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(p.workspace().is_some(), egui::Button::new("New item\u{2026}")).clicked() {
                acts.push(Act::OpenNew);
            }
        });
    });
    let Some(ws) = p.workspace() else { return acts };
    ui.add_space(10.0);
    theme::scroll_vertical().id_salt("depot-filters").auto_shrink([false, false]).show(ui, |ui| {
        theme::eyebrow(ui, "Folders");
        let row = |ui: &mut egui::Ui, label: &str, n: usize, on: bool| -> bool {
            ui.horizontal(|ui| {
                let r = ui.selectable_label(on, label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(n.to_string()).monospace().color(theme::DIM));
                });
                r.clicked()
            })
            .inner
        };
        if row(ui, "All folders", ws.items.len(), p.folder.is_none()) {
            acts.push(Act::Folder(None));
        }
        for (name, n) in &ws.folders {
            if row(ui, name, *n, p.folder.as_deref() == Some(name)) {
                acts.push(Act::Folder(Some(name.clone())));
            }
        }
        if ws.items.iter().any(|i| i.folder.is_empty()) {
            let n = ws.items.iter().filter(|i| i.folder.is_empty()).count();
            if row(ui, "(workspace root)", n, p.folder.as_deref() == Some("")) {
                acts.push(Act::Folder(Some(String::new())));
            }
        }

        ui.add_space(12.0);
        theme::eyebrow(ui, "State");
        for (s, hidden) in [(State::Draft, p.hide_draft), (State::Ready, p.hide_ready)] {
            ui.horizontal(|ui| {
                let mut on = !hidden;
                if ui.checkbox(&mut on, s.label()).changed() {
                    acts.push(Act::ToggleState(s));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(ws.count(s).to_string()).monospace().color(theme::DIM));
                });
            });
        }

        ui.add_space(12.0);
        theme::eyebrow(ui, "Kind");
        let mut kinds: Vec<(&'static str, usize)> = Vec::new();
        for i in &ws.items {
            for k in i.kinds() {
                match kinds.iter_mut().find(|(x, _)| *x == k) {
                    Some((_, n)) => *n += 1,
                    None => kinds.push((k, 1)),
                }
            }
        }
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        for (k, n) in kinds {
            ui.horizontal(|ui| {
                let mut on = !p.kinds_off.contains(k);
                if ui.checkbox(&mut on, egui::RichText::new(k).monospace()).changed() {
                    acts.push(Act::ToggleKind(k));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(n.to_string()).monospace().color(theme::DIM));
                });
            });
        }
        let all = mercs2_quartermaster::manifest::Contribution::ALL_KINDS.len();
        if ui.link(format!("Every kind qm knows ({all})")).on_hover_text("Opens New item").clicked() {
            acts.push(Act::OpenNew);
        }
    });
    acts
}

/// `ShipmentDestruction`'s navigator, for an open `edit_state_machine` item whose file reads: the
/// back arrow, then the machine's switch nodes. `None` for any other item.
fn states_nav(ui: &mut egui::Ui, p: &Panel, names: Option<&NameTable>) -> Option<Vec<Act>> {
    let item = p.viewport_item()?;
    let (_, _, path) = crate::depotstates::of(item)?;
    let yours = p.states.yours(&item.dir.join(path))?.ok()?;
    let mut acts = Vec::new();
    ui.horizontal(|ui| {
        if ui.button("\u{2039} All items").clicked() {
            acts.push(Act::Close);
        }
    });
    ui.add_space(6.0);
    if let Some(n) = crate::depotstates::nodes(ui, &p.states, &yours.1, names) {
        acts.push(Act::SmNode(n));
    }
    Some(acts)
}

/// `DepotNav`: every item, Draft then Ready, with a back arrow to All items.
fn depot_nav(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(ws) = p.workspace() else { return acts };
    ui.horizontal(|ui| {
        if ui.button("\u{2039} All items").clicked() {
            acts.push(Act::Close);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("New item\u{2026}").clicked() {
                acts.push(Act::OpenNew);
            }
        });
    });
    ui.add_space(6.0);
    let mut f = p.nav_filter.clone();
    if ui
        .add(
            egui::TextEdit::singleline(&mut f)
                .hint_text("Filter by name, kind or target")
                .margin(theme::TEXT_EDIT_MARGIN)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        acts.push(Act::NavFilter(f));
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for (label, s, n) in [
            ("All", None, ws.items.len()),
            ("Draft", Some(State::Draft), ws.count(State::Draft)),
            ("Ready", Some(State::Ready), ws.count(State::Ready)),
        ] {
            if theme::lens_pill(ui, label, &n.to_string(), p.nav_state == s).clicked() {
                acts.push(Act::NavState(s));
            }
        }
    });
    ui.add_space(6.0);
    let q = p.nav_filter.trim().to_lowercase();
    theme::scroll_vertical().id_salt("depot-nav").auto_shrink([false, false]).show(ui, |ui| {
        for (label, s) in [("DRAFT", Some(State::Draft)), ("READY", Some(State::Ready)), ("BROKEN", None)] {
            if p.nav_state.is_some() && p.nav_state != s {
                continue;
            }
            let rows: Vec<&Item> = ws
                .items
                .iter()
                .filter(|i| i.state() == s)
                .filter(|i| q.is_empty() || matches_query(i, &q))
                .collect();
            if rows.is_empty() {
                continue;
            }
            ui.label(theme::disp_text(format!("{label} {}", rows.len()), theme::style_eyebrow(), theme::DIM));
            for i in rows {
                let open = p.open.as_ref() == Some(&i.dir);
                let resp = egui::Frame::NONE
                    .fill(if open { theme::BRASS_SOFT } else { Color32::TRANSPARENT })
                    .inner_margin(egui::Margin { left: 9, right: 8, top: 5, bottom: 6 })
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            if let Some(k) = i.kinds().first() {
                                ui.label(theme::disp_text(
                                    crate::quartermaster::kind_title(k).to_uppercase(),
                                    theme::style_eyebrow(),
                                    theme::DIM,
                                ));
                            }
                            ui.label(egui::RichText::new(kind_line(i)).monospace().color(theme::DIM));
                        });
                        ui.add(
                            egui::Label::new(egui::RichText::new(&i.name).color(if open { theme::BRASS } else { theme::TX }))
                                .truncate(),
                        );
                        let (note, col) = note_line(ws, i);
                        ui.add(egui::Label::new(egui::RichText::new(note).small().color(col)).truncate());
                    })
                    .response;
                ui.painter().rect_filled(
                    egui::Rect::from_min_size(resp.rect.min, egui::vec2(3.0, resp.rect.height())),
                    0.0,
                    check_colour(ws, i),
                );
                let resp = resp.interact(egui::Sense::click());
                if resp.clicked() {
                    acts.push(Act::Open(i.dir.clone()));
                }
                item_menu(&resp, i, &mut acts);
            }
            ui.add_space(6.0);
        }
    });
    ui.add(egui::Label::new(egui::RichText::new(pretty_path(&ws.root)).monospace().small().color(theme::DIM)).truncate())
        .on_hover_text(format!("{}\nThe workspace folder, set in Settings \u{203a} Depot workspace", ws.root.display()));
    acts
}

// ─────────────────────────────────────────────────────────────── centre

pub fn center(
    root: &mut egui::Ui,
    p: &Panel,
    s: crate::kindform::Sources,
) -> Vec<Act> {
    let mut acts = Vec::new();
    // An editor of a model leaves the centre to the 3D viewport; its overlay is [`viewport`].
    if p.viewport_item().is_some() {
        return acts;
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(theme::G0).inner_margin(egui::Margin::symmetric(12, 8)))
        .show(root, |ui| match &p.ws {
            None => empty_state(ui, &mut acts, None),
            Some(Err(e)) => empty_state(ui, &mut acts, Some(e)),
            Some(Ok(ws)) => match p.open.as_ref().and_then(|d| ws.items.iter().find(|i| &i.dir == d)) {
                Some(item) => editor(ui, p, item, s, &mut acts),
                None => browser(ui, p, ws, &mut acts),
            },
        });
    acts
}

/// An open item: each contribution in its kind's editor.
fn editor(
    ui: &mut egui::Ui,
    p: &Panel,
    item: &Item,
    s: crate::kindform::Sources,
    acts: &mut Vec<Act>,
) {
    let f = match &item.file {
        Ok(f) => f,
        Err(e) => {
            ui.label(egui::RichText::new(e).color(theme::BAD));
            return;
        }
    };
    theme::scroll_vertical().id_salt("depot-editor").auto_shrink([false, false]).show(ui, |ui| {
        if f.contributions.is_empty() {
            ui.label(egui::RichText::new("This item holds no contributions.").color(theme::DIM));
        }
        for (index, c) in f.contributions.iter().enumerate() {
            ui.push_id(index, |ui| {
                match c {
                    mercs2_quartermaster::manifest::Contribution::PatchLua { target, append } => {
                        append_editor(ui, p, item, target, append, s.corpus, acts)
                    }
                    mercs2_quartermaster::manifest::Contribution::SetSpawnLists { .. } => {
                        spawn_editor(ui, p, item, index, c, s.corpus, acts)
                    }
                    _ => {}
                }
                if let Some((which, target, file)) = crate::depottext::of(c) {
                    let sub = crate::depottext::Subject { which, target: &target, file };
                    for ask in crate::depottext::editor(ui, &p.text, item, sub, &p.game, &s) {
                        match ask {
                            crate::depottext::Ask::Write { path, prior, text } => acts.push(Act::TextWrite { path, prior, text }),
                        }
                    }
                }
                for ask in crate::depotsound::editor(ui, &p.sound, item, index, &p.game, s.names) {
                    acts.push(match ask {
                        crate::depotsound::Ask::Play(id) => Act::SoundPlay(id),
                        crate::depotsound::Ask::Open { item, index, bank, guid } => Act::SoundOpen { item, index, bank, guid },
                        crate::depotsound::Ask::Edit { index, c } => Act::Edit { dir: item.dir.clone(), index, c },
                    });
                }
                for ask in crate::depottex::editor(ui, &p.tex, item, index, &p.game, s.names) {
                    acts.push(match ask {
                        crate::depottex::Ask::Choose { item, index, choice } => Act::TexChoose { item, index, choice },
                        crate::depottex::Ask::Wearer { item, index, model } => Act::TexWearer { item, index, model },
                    });
                }
                let own_editor = matches!(
                    c,
                    mercs2_quartermaster::manifest::Contribution::PatchLua { .. }
                        | mercs2_quartermaster::manifest::Contribution::SetSpawnLists { .. }
                );
                theme::section(ui, "Fields", Some(c.kind()), !own_editor, |ui| {
                    let facts = crate::quartermaster::model_source(c)
                        .map(|m| item.dir.join(m))
                        .filter(|a| a.is_file())
                        .and_then(|a| p.facts_for(&a));
                    let mut edited = c.clone();
                    let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: facts.as_ref(), sources: s, template_classes: None };
                    let commit = crate::quartermaster::contribution_form(ui, &mut edited, &ctx);
                    if commit && edited != *c {
                        acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
                    }
                });
            });
        }
    });
}

/// The shipped script `target` names, as qm finds its source: the first of the `vz.wad` folders
/// ([`mercs2_quartermaster::link::VZ_SOURCE_DIRS`]) that holds `<target>.lua`.
fn shipped_script<'a>(corpus: &'a [(String, String, String)], target: &str) -> Option<(&'a str, &'a str)> {
    mercs2_quartermaster::link::VZ_SOURCE_DIRS.iter().find_map(|d| {
        let want = format!("lua/{d}/{target}.lua");
        corpus.iter().find(|(path, _, _)| *path == want).map(|(path, src, _)| (path.as_str(), src.as_str()))
    })
}

/// `DepotLua`: the spawn lists the target of `set_spawn_lists` contribution `index` sets, each row
/// choosing another, and the shipped function beside the Lua qm writes for the rows.
fn spawn_editor(
    ui: &mut egui::Ui,
    p: &Panel,
    item: &Item,
    index: usize,
    c: &mercs2_quartermaster::manifest::Contribution,
    corpus: &[(String, String, String)],
    acts: &mut Vec<Act>,
) {
    use mercs2_quartermaster::manifest::Contribution;
    use mercs2_quartermaster::spawn_lists::{set, source_problems, wrapper, SpawnCall};
    let Contribution::SetSpawnLists { target, lists } = c else { return };
    let Some((path, src)) = shipped_script(corpus, target) else {
        theme::card(ui, "Spawn lists", None, |ui| {
            let msg = if corpus.is_empty() {
                "The reference bundle's Lua is not loaded yet.".to_string()
            } else {
                format!("No shipped vz.wad script is named {target}.")
            };
            ui.label(egui::RichText::new(msg).color(theme::BAD));
        });
        return;
    };
    let calls = p.spawn_calls_for(target, src);
    let names = p.spawn_names(corpus);
    let row = |c: &SpawnCall| lists.iter().find(|x| (x.function.as_str(), x.building.as_str(), x.group.as_deref()) == c.key());

    match &calls {
        Err(e) => {
            theme::card(ui, "Spawn lists", None, |ui| {
                ui.label(egui::RichText::new(format!("{path}: {e}")).color(theme::BAD));
            });
        }
        Ok(calls) => {
            let current = |c: &SpawnCall| row(c).map_or(c.list.clone(), |x| x.list.clone());
            let changed = calls.iter().filter(|c| current(c) != c.list).count();
            let mut functions: Vec<&str> = Vec::new();
            for c in calls.iter() {
                if !functions.contains(&c.function.as_str()) {
                    functions.push(&c.function);
                }
            }
            let badge = format!("{} calls \u{b7} {changed} changed", calls.len());
            theme::section(ui, "Spawn lists", Some(&badge), true, |ui| {
                if !functions.is_empty() {
                    ui.label(
                        egui::RichText::new(format!("Set by {} in {target}", functions.join(", ")))
                            .small()
                            .color(theme::DIM),
                    );
                }
                for problem in source_problems(lists, path, src, &names) {
                    ui.label(egui::RichText::new(problem).small().color(theme::BAD));
                }
                ui.add_space(4.0);
                egui::Grid::new("depot-spawn-lists").num_columns(5).striped(true).spacing([10.0, 4.0]).show(ui, |ui| {
                    for h in ["BUILDING", "GROUP", "SHIPPED LIST", "YOUR LIST", "LINE"] {
                        ui.label(theme::disp_text(h, theme::style_eyebrow(), theme::DIM));
                    }
                    ui.end_row();
                    for (n, call) in calls.iter().enumerate() {
                        let now = current(call);
                        let is_changed = now != call.list;
                        ui.label(egui::RichText::new(&call.building).monospace().color(theme::TX));
                        ui.label(call.group.as_deref().unwrap_or("all groups"));
                        ui.label(egui::RichText::new(&call.list).color(if is_changed { theme::DIM } else { theme::TX }));
                        ui.add_enabled_ui(call.editable.is_ok(), |ui| {
                            let text = egui::RichText::new(&now).color(if is_changed { theme::BRASS } else { theme::DIM });
                            let combo = egui::ComboBox::from_id_salt(("depot-spawn", n)).selected_text(text).width(220.0);
                            combo.show_ui(ui, |ui| {
                                for name in &names {
                                    if ui.selectable_label(*name == now, name).clicked() && *name != now {
                                        let edited = Contribution::SetSpawnLists { target: target.clone(), lists: set(lists, call, name) };
                                        acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
                                    }
                                }
                            });
                        })
                        .response
                        .on_disabled_hover_text(call.editable.clone().err().unwrap_or_default());
                        ui.label(egui::RichText::new(call.line.to_string()).monospace().color(theme::DIM));
                        ui.end_row();
                    }
                });
                ui.label(
                    egui::RichText::new(format!("The lists are every one the vz.wad scripts set ({}).", names.len()))
                        .small()
                        .color(theme::DIM),
                );
            });
        }
    }

    // The shipped function beside the Lua qm writes.
    let changed_lines: Vec<u32> = match &calls {
        Ok(calls) => calls.iter().filter(|c| row(c).is_some()).map(|c| c.line).collect(),
        Err(_) => Vec::new(),
    };
    let first_fn = match &calls {
        Ok(calls) => calls
            .iter()
            .find(|c| changed_lines.contains(&c.line))
            .or(calls.first())
            .map(|c| c.function.clone()),
        Err(_) => None,
    };
    let lua = wrapper(lists);
    ui.columns(2, |cols| {
        theme::card(&mut cols[0], &format!("Shipped \u{b7} {target}"), Some("read-only \u{b7} decompiled lines"), |ui| {
            match first_fn.as_deref().and_then(|f| function_lines(src, f)) {
                Some((start, lines)) => numbered(ui, start, &lines, &changed_lines, "depot-shipped"),
                None => {
                    ui.label(egui::RichText::new(path).monospace().color(theme::DIM));
                }
            }
        });
        theme::card(&mut cols[1], "The Lua qm writes", Some("read-only"), |ui| {
            if lua.is_empty() {
                ui.label(egui::RichText::new("No rows: no Lua.").color(theme::DIM));
            } else {
                let lines: Vec<&str> = lua.lines().collect();
                numbered(ui, 1, &lines, &[], "depot-wrapper");
            }
        });
    });
}

/// `patch_lua`: the shipped script it appends to, and the append, edited by hand.
fn append_editor(ui: &mut egui::Ui, p: &Panel, item: &Item, target: &str, append: &Path, corpus: &[(String, String, String)], acts: &mut Vec<Act>) {
    let append_text = read_or_empty(&item.dir.join(append));
    let shipped = shipped_script(corpus, target).map(|(path, _)| path);
    ui.columns(2, |cols| {
        let left = &mut cols[0];
        theme::card(left, &format!("Shipped \u{b7} {target}"), Some("read-only \u{b7} decompiled lines"), |ui| match shipped {
            Some(path) => {
                ui.label(egui::RichText::new(path).monospace().color(theme::DIM));
            }
            None if corpus.is_empty() => {
                ui.label(egui::RichText::new("The reference bundle's Lua is not loaded yet.").color(theme::BAD));
            }
            None => {
                ui.label(egui::RichText::new(format!("No shipped vz.wad script is named {target}.")).color(theme::BAD));
            }
        });
        let right = &mut cols[1];
        let title = format!("Your append \u{b7} {}", append.display());
        theme::card(right, &title, None, |ui| match (&p.hand, &append_text) {
            (Some(h), _) if h.dir == item.dir && h.file == append => {
                let mut t = h.text.clone();
                if ui
                    .add(egui::TextEdit::multiline(&mut t).code_editor().desired_rows(12).desired_width(f32::INFINITY))
                    .changed()
                {
                    acts.push(Act::HandText(t));
                }
                if let Some(e) = &h.error {
                    ui.label(egui::RichText::new(e).small().color(theme::BAD));
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        acts.push(Act::HandCancel);
                    }
                    if theme::primary_button(ui, "Save", true).clicked() {
                        acts.push(Act::HandSave);
                    }
                });
            }
            (_, Err(e)) => {
                ui.label(egui::RichText::new(e).color(theme::BAD));
            }
            (_, Ok(t)) => {
                if t.is_empty() {
                    ui.label(egui::RichText::new("Empty.").color(theme::DIM));
                } else {
                    let lines: Vec<&str> = t.lines().collect();
                    numbered(ui, 1, &lines, &[], "depot-append");
                }
                if ui.button("Edit by hand").clicked() {
                    acts.push(Act::HandStart { dir: item.dir.clone(), file: append.to_path_buf() });
                }
            }
        });
    });
}


/// The lines of top-level function `name` in `src`, with the line its definition starts on.
fn function_lines<'a>(src: &'a str, name: &str) -> Option<(u32, Vec<&'a str>)> {
    let script = mercs2_quartermaster::lua_source::Script::parse(src).ok()?;
    let f = script.functions().into_iter().find(|f| f.name == name)?;
    let text = &src[f.span.clone()];
    Some((f.line, text.lines().collect()))
}

/// Source lines with their numbers; `marked` lines are drawn in brass.
fn numbered(ui: &mut egui::Ui, first: u32, lines: &[&str], marked: &[u32], salt: &str) {
    theme::scroll_both().id_salt(salt).max_height(320.0).show(ui, |ui| {
        for (k, l) in lines.iter().enumerate() {
            let n = first + k as u32;
            let hot = marked.contains(&n);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{n:>4}")).monospace().color(theme::FAINT));
                ui.label(egui::RichText::new(*l).monospace().color(if hot { theme::BRASS } else { theme::TX }));
            });
        }
    });
}

fn empty_state(ui: &mut egui::Ui, acts: &mut Vec<Act>, error: Option<&String>) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.label(theme::disp_text("DEPOT WORKSPACE", theme::style_title(), theme::TX));
        ui.add_space(6.0);
        match error {
            None => {
                ui.label(
                    egui::RichText::new(
                        "The Depot keeps your items, Draft and Ready, in one folder. Choose that folder to begin.",
                    )
                    .color(theme::DIM),
                );
            }
            Some(e) => {
                ui.label(egui::RichText::new(e).color(theme::BAD));
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if theme::primary_button(ui, "Choose folder\u{2026}", true).clicked() {
                acts.push(Act::ChooseWorkspace);
            }
            if ui.button("Open Settings").clicked() {
                acts.push(Act::OpenSettings);
            }
        });
    });
}

fn browser(ui: &mut egui::Ui, p: &Panel, ws: &Workspace, acts: &mut Vec<Act>) {
    let visible = p.visible();
    ui.horizontal(|ui| {
        let folder = match &p.folder {
            None => "all folders".to_string(),
            Some(f) if f.is_empty() => "workspace root".to_string(),
            Some(f) => f.clone(),
        };
        // The counts are laid out first, at the right, so the path cut to the room left can never
        // push them, or the grid, wider than the panel. The whole path is on hover.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("{folder} \u{b7} {} items", visible.len())).small().color(theme::DIM));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(pretty_path(&ws.root)).monospace().color(theme::TX)).truncate())
                    .on_hover_text(ws.root.display().to_string());
            });
        });
    });
    for (name, dirs) in &ws.duplicates {
        let list: Vec<String> = dirs.iter().map(|d| d.display().to_string()).collect();
        ui.label(
            egui::RichText::new(format!("{name} is the name of {} items: {}", dirs.len(), list.join(", ")))
                .small()
                .color(theme::BAD),
        );
    }
    ui.add_space(8.0);
    if visible.is_empty() {
        nothing_listed(ui, ws.items.is_empty(), acts);
        return;
    }
    theme::scroll_vertical().id_salt("depot-items").auto_shrink([false, false]).show(ui, |ui| match p.view() {
        View::Tiles => tiles(ui, p, ws, &visible, acts),
        View::List => list(ui, p, ws, &visible, acts),
    });
}

/// The grid with nothing in it: an empty workspace offers New item; a filtered-out one says so.
fn nothing_listed(ui: &mut egui::Ui, empty: bool, acts: &mut Vec<Act>) {
    let h = ui.available_height();
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), h),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space((h * 0.32).max(0.0));
            let (r, _) = ui.allocate_exact_size(egui::vec2(64.0, 52.0), egui::Sense::hover());
            paint_storehouse(ui.painter(), r.center(), theme::FAINT);
            ui.add_space(10.0);
            if empty {
                ui.label(theme::disp_text("NO ITEMS YET", theme::style_title(), theme::DIM));
                ui.add_space(8.0);
                if theme::primary_button(ui, "New item\u{2026}", true).clicked() {
                    acts.push(Act::OpenNew);
                }
            } else {
                ui.label(theme::disp_text("NO ITEM MATCHES THE FILTERS", theme::style_title(), theme::DIM));
            }
        },
    );
}

/// The Depot's rail glyph (`RailLevel`'s DEPOT path), drawn large for the empty grid.
fn paint_storehouse(p: &egui::Painter, c: egui::Pos2, col: Color32) {
    let k = 2.6;
    let v = |x: f32, y: f32| c + egui::vec2(x * k, y * k);
    let s = egui::Stroke::new(1.5, col);
    let wall = [v(-7.0, -2.5), v(0.0, -6.5), v(7.0, -2.5), v(7.0, 5.5), v(-7.0, 5.5)];
    for i in 0..wall.len() {
        p.line_segment([wall[i], wall[(i + 1) % wall.len()]], s);
    }
    p.line_segment([v(-7.0, -2.5), v(7.0, -2.5)], s);
    p.line_segment([v(-3.0, 5.5), v(-3.0, 1.5)], s);
    p.line_segment([v(-3.0, 1.5), v(3.0, 1.5)], s);
    p.line_segment([v(3.0, 1.5), v(3.0, 5.5)], s);
}

/// `p` with the home folder written `~`, as the boards write the workspace.
fn pretty_path(p: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match p.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => p.display().to_string(),
        },
        _ => p.display().to_string(),
    }
}

/// The status bar on the Depot: the workspace's counts and the last action's outcome.
pub fn status(ui: &mut egui::Ui, p: &Panel) {
    match &p.ws {
        None => theme::status_dot(ui, "No workspace", theme::BRASS_DK),
        Some(Err(_)) => theme::status_dot(ui, "Workspace not found", theme::BAD),
        Some(Ok(ws)) => {
            let errs = ws.with_errors();
            if errs == 0 {
                theme::status_dot(ui, "Ready", theme::GOOD);
            } else {
                theme::status_dot(ui, "Errors", theme::BAD);
            }
            ui.separator();
            ui.label(
                egui::RichText::new(format!(
                    "{} \u{b7} {} items \u{b7} {} draft \u{b7} {} ready \u{b7} {errs} with errors",
                    pretty_path(&ws.root),
                    ws.items.len(),
                    ws.count(State::Draft),
                    ws.count(State::Ready)
                ))
                .small()
                .color(theme::DIM),
            );
        }
    }
    if let Some(s) = &p.status {
        let (t, c) = match s {
            Ok(t) => (t, theme::DIM),
            Err(t) => (t, theme::BAD),
        };
        ui.separator();
        ui.label(egui::RichText::new(t).small().color(c));
    }
}

fn item_menu(resp: &egui::Response, i: &Item, acts: &mut Vec<Act>) {
    let mut menu = Vec::new();
    theme::context_menu(resp, |ui| {
        if ui.button("New item\u{2026}").clicked() {
            menu.push(Act::OpenNew);
            ui.close();
        }
        if i.file.is_ok() && ui.button("Open").clicked() {
            menu.push(Act::Open(i.dir.clone()));
            ui.close();
        }
        ui.separator();
        match i.state() {
            Some(State::Draft) => {
                let ok = i.checks.errors() == 0;
                if ui.add_enabled(ok, egui::Button::new("Mark Ready")).clicked() {
                    menu.push(Act::SetState(i.dir.clone(), State::Ready));
                    ui.close();
                }
            }
            Some(State::Ready) if ui.button("Back to Draft").clicked() => {
                menu.push(Act::SetState(i.dir.clone(), State::Draft));
                ui.close();
            }
            _ => {}
        }
        if ui.button("Reveal folder").clicked() {
            menu.push(Act::Reveal(i.dir.clone()));
            ui.close();
        }
        if ui.button("Duplicate\u{2026}").clicked() {
            menu.push(Act::Prompt { dir: i.dir.clone(), duplicate: true });
            ui.close();
        }
        if ui.button("Rename\u{2026}").clicked() {
            menu.push(Act::Prompt { dir: i.dir.clone(), duplicate: false });
            ui.close();
        }
        ui.separator();
        if ui.button("Delete\u{2026}").clicked() {
            menu.push(Act::AskDelete(i.dir.clone()));
            ui.close();
        }
    });
    acts.extend(menu);
}

const TILE_W: f32 = 168.0;
const TILE_H: f32 = 164.0;
const THUMB_H: f32 = 92.0;
const TILE_GAP: f32 = 10.0;

fn tiles(ui: &mut egui::Ui, p: &Panel, ws: &Workspace, visible: &[&Item], acts: &mut Vec<Act>) {
    // The visible width, not the widest thing laid out so far: a long label must not push the grid
    // under the inspector.
    let avail = ui.clip_rect().width().min(ui.available_width());
    let cols = (((avail + TILE_GAP) / (TILE_W + TILE_GAP)).floor() as usize).max(1);
    let w = (avail - TILE_GAP * (cols as f32 - 1.0)) / cols as f32;
    for row in visible.chunks(cols) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = TILE_GAP;
            for i in row {
                tile(ui, p, ws, i, w, acts);
            }
        });
        ui.add_space(TILE_GAP);
    }
}

fn tile(ui: &mut egui::Ui, p: &Panel, ws: &Workspace, i: &Item, w: f32, acts: &mut Vec<Act>) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, TILE_H), egui::Sense::click());
    let selected = p.selected.as_ref() == Some(&i.dir);
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        let border = if selected { theme::BRASS } else { theme::LINE };
        painter.rect(rect, egui::CornerRadius::same(6), theme::G1, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
        let thumb = egui::Rect::from_min_size(rect.min, egui::vec2(w, THUMB_H));
        painter.rect_filled(thumb.shrink(1.0), egui::CornerRadius { nw: 5, ne: 5, sw: 0, se: 0 }, Color32::from_rgb(0x0d, 0x0e, 0x11));
        painter.line_segment([thumb.left_bottom(), thumb.right_bottom()], egui::Stroke::new(1.0, theme::LINE));
        paint_kind_glyph(&painter, i.kinds().first().copied(), thumb.center(), theme::DIM);

        let (fill, stroke, ink) = state_chip_colours(i.state());
        let chip_g = painter.layout_no_wrap(state_label(i.state()).to_string(), theme::font(&TextStyle::Small), ink);
        let chip = egui::Rect::from_min_size(thumb.min + egui::vec2(6.0, 6.0), chip_g.size() + egui::vec2(12.0, 2.0));
        painter.rect(chip, egui::CornerRadius::same(3), fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
        painter.galley(chip.min + egui::vec2(6.0, 1.0), chip_g, ink);
        painter.circle_filled(egui::pos2(thumb.right() - 10.5, thumb.top() + 10.5), 4.5, check_colour(ws, i));

        let x = rect.left() + 8.0;
        let mut y = thumb.bottom() + 6.0;
        let (note, note_col) = note_line(ws, i);
        let lines = [
            (i.name.clone(), TextStyle::Body, if selected { theme::BRASS } else { theme::TX }),
            (kind_line(i), TextStyle::Monospace, theme::DIM),
            (note, TextStyle::Small, note_col),
        ];
        for (text, style, col) in lines {
            let g = painter.layout(text, theme::font(&style), col, f32::INFINITY);
            let h = g.size().y;
            painter.with_clip_rect(egui::Rect::from_min_max(egui::pos2(x, y), egui::pos2(rect.right() - 8.0, y + h)))
                .galley(egui::pos2(x, y), g, col);
            y += h + 1.0;
        }
    }
    let resp = resp.on_hover_text(i.dir.display().to_string());
    if resp.double_clicked() {
        acts.push(Act::Open(i.dir.clone()));
    } else if resp.clicked() {
        acts.push(Act::Select(i.dir.clone()));
    }
    item_menu(&resp, i, acts);
}

fn list(ui: &mut egui::Ui, p: &Panel, ws: &Workspace, visible: &[&Item], acts: &mut Vec<Act>) {
    for (n, i) in visible.iter().enumerate() {
        let selected = p.selected.as_ref() == Some(&i.dir);
        let fill = if selected { theme::BRASS_SOFT } else { theme::row_stripe(n % 2 == 1) };
        let resp = theme::row_chip(ui, fill, if selected { theme::BRASS_DK } else { theme::LINE }, |ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.0, check_colour(ws, i));
            let (fill, stroke, ink) = state_chip_colours(i.state());
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, stroke))
                .corner_radius(egui::CornerRadius::same(3))
                .inner_margin(egui::Margin::symmetric(5, 0))
                .show(ui, |ui| ui.label(egui::RichText::new(state_label(i.state())).small().color(ink)));
            ui.add_sized(
                egui::vec2(220.0, 18.0),
                egui::Label::new(egui::RichText::new(&i.name).color(if selected { theme::BRASS } else { theme::TX })).truncate(),
            );
            ui.add_sized(egui::vec2(160.0, 18.0), egui::Label::new(egui::RichText::new(kind_line(i)).monospace().color(theme::DIM)).truncate());
            let (note, col) = note_line(ws, i);
            ui.add(egui::Label::new(egui::RichText::new(note).small().color(col)).truncate());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(age(i.modified)).small().color(theme::DIM));
            });
        });
        if resp.double_clicked() {
            acts.push(Act::Open(i.dir.clone()));
        } else if resp.clicked() {
            acts.push(Act::Select(i.dir.clone()));
        }
        item_menu(&resp, i, acts);
    }
}

// ─────────────────────────────────────────────────────────────── inspector

pub fn inspector(ui: &mut egui::Ui, p: &Panel, s: crate::kindform::Sources) -> Vec<Act> {
    let mut acts = Vec::new();
    let (Some(ws), Some(i)) = (p.workspace(), p.selected_item()) else {
        ui.label(egui::RichText::new("Select an item to see it in full.").color(theme::DIM));
        return acts;
    };
    let first = i.kinds().first().copied();
    ui.horizontal(|ui| {
        if let Some(k) = first {
            ui.label(theme::disp_text(crate::quartermaster::kind_title(k).to_uppercase(), theme::style_eyebrow(), theme::BRASS));
            ui.label(egui::RichText::new(k).monospace().color(theme::DIM));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (fill, stroke, ink) = state_chip_colours(i.state());
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, stroke))
                .corner_radius(egui::CornerRadius::same(3))
                .inner_margin(egui::Margin::symmetric(6, 0))
                .show(ui, |ui| ui.label(egui::RichText::new(state_label(i.state())).small().color(ink)));
        });
    });
    ui.label(egui::RichText::new(&i.name).text_style(TextStyle::Heading).color(theme::TX));
    let folder = if i.folder.is_empty() { "workspace root" } else { i.folder.as_str() };
    ui.label(egui::RichText::new(format!("{folder} / modified {}", age(i.modified))).small().color(theme::DIM));
    ui.add_space(8.0);

    let f = match &i.file {
        Ok(f) => f,
        Err(e) => {
            theme::card(ui, "item.yaml", None, |ui| {
                ui.label(egui::RichText::new(e).color(theme::BAD));
            });
            if ui.button("Reveal folder").clicked() {
                acts.push(Act::Reveal(i.dir.clone()));
            }
            return acts;
        }
    };

    // A destruction-states item open in its editor: its fields, the selected state, the checks.
    if let Some((index, target, path)) = crate::depotstates::of(i).filter(|_| p.viewport_item().is_some_and(|v| v.dir == i.dir)) {
        states_cards(ui, p, i, (index, target, path), s, &mut acts);
    } else if crate::depotfx::of_item(i) && p.viewport_item().is_some_and(|v| v.dir == i.dir) {
        // An effect item open in its editor: each effect contribution's fields, its preview's
        // transport, its nodes and the selected node, then what the build ships.
        for (index, c) in f.contributions.iter().enumerate() {
            if crate::depotfx::of(c) {
                ui.push_id(("fx", index), |ui| fx_cards(ui, p, i, index, s, &mut acts));
            }
        }
        crate::depotfx::after_card(ui, &p.fx, &p.game, i);
    } else if (crate::depotworld::draws(i) || crate::depottiny::of(i).is_some()) && p.viewport_item().is_some_and(|v| v.dir == i.dir) {
        // A world item open in its editor: each world contribution's fields, records and build, and
        // an activation's layers.
        for (index, c) in f.contributions.iter().enumerate() {
            let named = matches!(
                c,
                mercs2_quartermaster::manifest::Contribution::ActivateLayer { .. }
                    | mercs2_quartermaster::manifest::Contribution::ReplaceTerrainCell { .. }
                    | mercs2_quartermaster::manifest::Contribution::AddTinyGeometry { .. }
            );
            if crate::depotworld::of(c).is_some() || named {
                ui.push_id(("world", index), |ui| world_cards(ui, p, i, index, s, &mut acts));
            }
        }
    } else if f.contributions.iter().any(crate::depotanim::of) && p.viewport_item().is_some_and(|v| v.dir == i.dir) {
        // An animation item open in its editor: each clip's fields and preview.
        for (index, c) in f.contributions.iter().enumerate() {
            if crate::depotanim::of(c) {
                ui.push_id(("anim", index), |ui| anim_cards(ui, p, i, index, s, &mut acts));
            }
        }
    } else if p.viewport_item().is_some_and(|v| v.dir == i.dir) {
    // A collision swap open in its editor: its fields and file, then what the game ships.
        for (index, c) in f.contributions.iter().enumerate() {
            if matches!(c, mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { .. }) {
                ui.push_id(index, |ui| collision_cards(ui, p, i, index, s, &mut acts));
            }
        }
    } else if !f.contributions.is_empty() {
        theme::card(ui, "Target", None, |ui| {
            for c in &f.contributions {
                theme::kv(ui, c.kind(), egui::RichText::new(crate::quartermaster::contribution_name(c)));
            }
        });
        // A string-table item open in its editor: the tables as the build writes them.
        if p.open.as_ref() == Some(&i.dir) && f.contributions.iter().any(|c| crate::depottext::of(c).is_some()) {
            crate::depottext::after_card(ui, &p.text, &p.game, i);
        }
        // A sound item open in its editor: what its build ships and what the loaders load.
        if p.open.as_ref() == Some(&i.dir) {
            crate::depotsound::after_card(ui, &p.sound, &p.game, i);
            crate::depottex::after_card(ui, &p.tex, &p.game, i);
        }
    }

    theme::card(ui, "Writes into manifest.yaml", None, |ui| {
        let yaml = serde_norway::to_string(&f.contributions).unwrap_or_else(|e| format!("# cannot serialize: {e}"));
        theme::code_block(ui, yaml.trim_end());
        let files = item_files(&i.dir);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("Files: {}", files.join(", "))).small().color(theme::DIM));
    });

    theme::card(ui, "Checks", None, |ui| {
        let errs = i.checks.lint.iter().filter(|d| d.severity >= Severity::Error).count();
        let warns = i.checks.warnings();
        let (dot, note) = match (errs, warns) {
            (0, 0) => (theme::GOOD, "0 findings".to_string()),
            (0, w) => (theme::BRASS_DK, format!("{w} warning(s)")),
            (e, w) => (theme::BAD, format!("{e} error(s), {w} warning(s)")),
        };
        check_row(ui, dot, "qm lint", &note);
        for d in &i.checks.lint {
            let col = if d.severity >= Severity::Error { theme::BAD } else { theme::DIM };
            ui.horizontal_wrapped(|ui| {
                if ui.link(egui::RichText::new(d.rule.code).monospace()).on_hover_text(d.rule.url()).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(d.rule.url()));
                }
                ui.label(egui::RichText::new(&d.message).small().color(col));
            });
        }
        for (path, r) in &i.checks.lua {
            match r {
                Ok(()) => check_row(ui, theme::GOOD, "Lua compile", &path.display().to_string()),
                Err(e) => {
                    check_row(ui, theme::BAD, "Lua compile", &path.display().to_string());
                    ui.label(egui::RichText::new(e).small().color(theme::BAD));
                }
            }
        }
        check_row(ui, theme::FAINT, "qm link", "in a Shipment");
    });

    if ws.is_duplicate(&i.name) {
        ui.label(egui::RichText::new("Another item has this name; rename one of them.").color(theme::BAD));
    }

    // `DepotBrowser`: Open across the panel, then the state change and Reveal side by side.
    ui.add_space(6.0);
    let full = ui.available_width();
    if p.open.as_ref() != Some(&i.dir) && ui.add(egui::Button::new("Open").min_size(egui::vec2(full, 26.0))).clicked() {
        acts.push(Act::Open(i.dir.clone()));
    }
    let half = (full - ui.spacing().item_spacing.x) / 2.0;
    ui.horizontal(|ui| {
        match i.state() {
            Some(State::Draft) => {
                let ok = !ws.has_error(i);
                let (fg, bg) = if ok { (Color32::from_rgb(0x1c, 0x16, 0x06), theme::BRASS) } else { (theme::FAINT, theme::G2) };
                let b = egui::Button::new(egui::RichText::new("Mark Ready").color(fg).strong())
                    .fill(bg)
                    .stroke(egui::Stroke::new(1.0, if ok { theme::BRASS } else { theme::LINE }))
                    .min_size(egui::vec2(half, 26.0));
                let resp = ui.add_enabled(ok, b);
                let resp = if ok { resp } else { resp.on_disabled_hover_text("Fix the errors first") };
                if resp.clicked() {
                    acts.push(Act::SetState(i.dir.clone(), State::Ready));
                }
            }
            Some(State::Ready) if ui.add(egui::Button::new("Back to Draft").min_size(egui::vec2(half, 26.0))).clicked() => {
                acts.push(Act::SetState(i.dir.clone(), State::Draft));
            }
            _ => {}
        }
        if ui.add(egui::Button::new("Reveal folder").min_size(egui::vec2(half, 26.0))).clicked() {
            acts.push(Act::Reveal(i.dir.clone()));
        }
    });
    acts
}

/// `ShipmentDestruction`'s right panel for the item's `edit_state_machine` (`depotstates::of`: its
/// index, target and `states:` path): its fields, then the selected state of the item's machine (or
/// Start from shipped while it has no file), then what `qm build` would say. Tells the viewport what
/// to show when the machine or the selection changes.
fn states_cards(
    ui: &mut egui::Ui,
    p: &Panel,
    item: &Item,
    (index, target, path): (usize, &str, &Path),
    s: crate::kindform::Sources,
    acts: &mut Vec<Act>,
) {
    use crate::depotstates::Ask;
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return };
    let shipped = p.states.shipped(&p.game, target);
    let abs = item.dir.join(path);
    theme::card(ui, "Destruction states", Some(c.kind()), |ui| {
        let mut edited = c.clone();
        let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: None, sources: s, template_classes: None };
        if crate::quartermaster::contribution_form(ui, &mut edited, &ctx) && edited != *c {
            acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
        }
        ui.add_space(6.0);
        match p.states.yours(&abs) {
            None => {
                ui.label(egui::RichText::new("No file yet. Start from the machine the game ships.").small().color(theme::DIM));
                let ready = matches!(shipped, Some(Ok(_)));
                if ui.add_enabled(ready, egui::Button::new("Start from shipped")).clicked() {
                    acts.push(Act::SmStartShipped { dir: item.dir.clone(), index, target: target.to_string() });
                }
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).small().color(theme::BAD));
                ui.label(egui::RichText::new("The file does not read, so the editor does not write it.").small().color(theme::DIM));
            }
            Some(Ok(yours)) => {
                let (ask, error) = crate::depotstates::editor(ui, &p.states, &yours, shipped.as_ref(), s.names);
                match ask {
                    Some(Ask::Write(m)) => acts.push(Act::SmWrite { path: abs.clone(), prior: yours.0.clone(), machine: m }),
                    None => {}
                }
                if let Some(e) = error {
                    acts.push(Act::SmError(e));
                }
                if let Some(node_state) = p.states.preview_change(&yours.0, &yours.1) {
                    let hash = mercs2_quartermaster::manifest::asset_hash(target);
                    acts.push(Act::SmPreview { hash, machine: Box::new(yours.1.clone()), node_state });
                }
            }
        }
    });
    theme::card(ui, "Machine checks", Some("as qm build reads it"), |ui| match p.states.yours(&abs) {
        Some(Ok(yours)) => crate::depotstates::checks(ui, &yours.1, shipped.as_ref()),
        Some(Err(_)) => {
            ui.label(egui::RichText::new("The file does not read; there is no machine to check.").small().color(theme::DIM));
        }
        None => {
            ui.label(egui::RichText::new("No file; there is no machine to check.").small().color(theme::DIM));
        }
    });
}

/// `ShipmentRawKind`'s right panel for contribution `index` (a `replace_phy2`): **Your collision**
/// (the fields, and what the file holds) and **Shipped collision** (what the game's model carries,
/// with Save shipped as .phy2).
/// A world contribution's fields, then its records, the selected record and what the build makes.
fn world_cards(ui: &mut egui::Ui, p: &Panel, item: &Item, index: usize, s: crate::kindform::Sources, acts: &mut Vec<Act>) {
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return };
    theme::card(ui, crate::quartermaster::kind_title(c.kind()), Some(c.kind()), |ui| {
        let mut edited = c.clone();
        let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: None, sources: s, template_classes: None };
        if crate::quartermaster::contribution_form(ui, &mut edited, &ctx) && edited != *c {
            acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
        }
    });
    if let mercs2_quartermaster::manifest::Contribution::AddTinyGeometry { .. } = c {
        for ask in crate::depottiny::tiny_cards(ui, &p.tiny, item, &p.game) {
            acts.push(match ask {
                crate::depottiny::Ask::Show(dir, s) => Act::TinyShow(dir, s),
                crate::depottiny::Ask::Tint(on) => Act::TinyTint(on),
            });
        }
        return;
    }
    let asks = match c {
        mercs2_quartermaster::manifest::Contribution::ActivateLayer { .. } => crate::depotworld::activation_cards(ui, &p.world, item, &p.game),
        mercs2_quartermaster::manifest::Contribution::ReplaceTerrainCell { .. } => crate::depotworld::terrain_cards(ui, &p.world, item, &p.game),
        _ => crate::depotworld::cards(ui, &p.world, item, index, &p.game, &s),
    };
    for ask in asks {
        acts.push(match ask {
            crate::depotworld::Ask::Write { path, prior, text } => Act::TextWrite { path, prior, text },
            crate::depotworld::Ask::Select { path, row } => Act::WorldSelect { path, row },
            crate::depotworld::Ask::Shipped(on) => Act::WorldShipped(on),
            crate::depotworld::Ask::ShowLayer { item, index } => Act::WorldShowLayer { item, index },
            crate::depotworld::Ask::Refused(e) => Act::WorldError(e),
        });
    }
}

/// An effect contribution's fields, then its preview and editor ([`crate::depotfx::cards`]).
fn fx_cards(ui: &mut egui::Ui, p: &Panel, item: &Item, index: usize, s: crate::kindform::Sources, acts: &mut Vec<Act>) {
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return };
    let after = p.fx.after(&p.game, item);
    let classes = after.as_ref().and_then(|a| a.as_ref().ok()).and_then(|a| a.classes.clone());
    theme::card(ui, crate::quartermaster::kind_title(c.kind()), Some(c.kind()), |ui| {
        let mut edited = c.clone();
        let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: None, sources: s, template_classes: classes.as_deref() };
        if crate::quartermaster::contribution_form(ui, &mut edited, &ctx) && edited != *c {
            acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
        }
    });
    for ask in crate::depotfx::cards(ui, &p.fx, item, index, &p.game, s.names) {
        acts.push(match ask {
            crate::depotfx::Ask::Write { path, prior, text } => Act::TextWrite { path, prior, text },
            crate::depotfx::Ask::Start { path, effect } => Act::FxStart { path, effect },
            crate::depotfx::Ask::Side { item, index, side } => Act::FxSide { item, index, side },
            crate::depotfx::Ask::Node { item, index, node } => Act::FxNode { item, index, node },
            crate::depotfx::Ask::Play(a) => Act::FxPlay(a),
            crate::depotfx::Ask::Refused(e) => Act::FxError(e),
        });
    }
}

/// An animation contribution's fields, then its preview: the clip's facts, the hero, the viewport's
/// transport, the bone binding and the events.
fn anim_cards(ui: &mut egui::Ui, p: &Panel, item: &Item, index: usize, s: crate::kindform::Sources, acts: &mut Vec<Act>) {
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return };
    theme::card(ui, crate::quartermaster::kind_title(c.kind()), Some(c.kind()), |ui| {
        let mut edited = c.clone();
        let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: None, sources: s, template_classes: None };
        if crate::quartermaster::contribution_form(ui, &mut edited, &ctx) && edited != *c {
            acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
        }
    });
    for ask in crate::depotanim::editor(ui, &p.anim, item, index, &p.game, s.names) {
        acts.push(match ask {
            crate::depotanim::Ask::Hero { item, index, model } => Act::AnimHero { item, index, model },
            crate::depotanim::Ask::Focus { item, index } => Act::AnimFocus { item, index },
            crate::depotanim::Ask::Play(a) => Act::AnimPlay(a),
            crate::depotanim::Ask::Events { path, prior, events } => Act::AnimEvents { path, prior, events },
            crate::depotanim::Ask::AddEvents { item, path, index, c } => Act::AnimAddEvents { item, path, index, c },
            crate::depotanim::Ask::StartShipped { item, index, c, files } => Act::AnimStartShipped { item, index, c, files },
        });
    }
}

fn collision_cards(ui: &mut egui::Ui, p: &Panel, item: &Item, index: usize, s: crate::kindform::Sources, acts: &mut Vec<Act>) {
    let Some(c) = item.file.as_ref().ok().and_then(|f| f.contributions.get(index)) else { return };
    let mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { target, phy2 } = c else { return };
    theme::card(ui, "Your collision", None, |ui| {
        let mut edited = c.clone();
        let ctx = crate::quartermaster::FormCtx { root: &item.dir, shipment_name: &item.name, facts: None, sources: s, template_classes: None };
        if crate::quartermaster::contribution_form(ui, &mut edited, &ctx) && edited != *c {
            acts.push(Act::Edit { dir: item.dir.clone(), index, c: Box::new(edited) });
        }
        ui.add_space(4.0);
        match p.yours_for(&item.dir.join(phy2)) {
            None => {
                ui.label(egui::RichText::new("No file yet. Save the shipped collision to start from it.").small().color(theme::DIM));
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).small().color(theme::BAD));
            }
            Some(Ok(col)) => collision_facts(ui, &col),
        }
    });
    theme::card(ui, "Shipped collision", Some("from the game"), |ui| match p.shipped_for(target) {
        None => {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            ui.label(egui::RichText::new("Reading the model from the game\u{2026}").small().color(theme::DIM));
        }
        Some(Err(e)) => {
            ui.label(egui::RichText::new(e).small().color(theme::BAD));
        }
        Some(Ok(s)) => {
            collision_facts(ui, &s.1);
            ui.add_space(6.0);
            if ui.button("Save shipped as .phy2").on_hover_text("Writes it into this item's src/ and points the item at it").clicked() {
                acts.push(Act::SaveShipped { dir: item.dir.clone(), index, target: target.clone() });
            }
        }
    });
}

fn collision_facts(ui: &mut egui::Ui, c: &crate::collision::Collision) {
    let mono = |s: String| egui::RichText::new(s).monospace();
    theme::kv(ui, "PHY2", mono(format!("{} B", commas(c.bytes))));
    theme::kv(ui, "havok", mono(format!("{} \u{b7} {} B", c.havok, commas(c.packfile_bytes))));
    let undecoded = c.meshes - c.meshes_decoded;
    let meshes = if undecoded == 0 {
        format!("{} \u{b7} {} tris", c.meshes, commas(c.tris))
    } else {
        format!("{} \u{b7} {} tris \u{b7} {undecoded} not decoded", c.meshes, commas(c.tris))
    };
    theme::kv(ui, "meshes", mono(meshes));
    theme::kv(ui, "hulls", mono(format!("{} convex", c.hulls)));
    theme::kv(ui, "MOPP", mono(format!("{} BV-trees", c.mopp_trees)));
    for (k, n) in [("boxes", c.boxes), ("capsules", c.capsules), ("spheres", c.spheres)] {
        if n > 0 {
            theme::kv(ui, k, mono(n.to_string()));
        }
    }
    for (name, n) in &c.other {
        theme::kv(ui, name, mono(format!("{n} \u{b7} not decoded")));
    }
    if let Some(e) = c.extent {
        theme::kv(ui, "extent", mono(format!("{:.1} \u{d7} {:.1} \u{d7} {:.1} m", e[0], e[1], e[2])));
    }
}

fn commas(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Over the 3D viewport while a collision swap is open: the overlay toggles, the outline of the
/// collision drawn on the model, and the legend of what it is.
pub fn viewport(ctx: &egui::Context, p: &Panel, vp: glam::Mat4, rect: egui::Rect, names: Option<&NameTable>) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(item) = p.viewport_item() else { return acts };
    crate::depotworld::viewport_note(ctx, &p.world, &p.game, item, rect);
    crate::depottiny::viewport_note(ctx, &p.tiny, &p.game, item, rect);
    crate::depotfx::viewport_note(ctx, &p.fx, &p.game, item, rect);
    let Some(target) = viewport_subject(item) else { return acts };
    if let Some((_, _, path)) = crate::depotstates::of(item) {
        if let Some(Ok(yours)) = p.states.yours(&item.dir.join(path)) {
            egui::Area::new(egui::Id::unique("depot_vp_states"))
                .fixed_pos(rect.left_top() + egui::vec2(14.0, 40.0))
                .show(ctx, |ui| {
                    ui.set_max_width(rect.width() - 260.0);
                    if let Some(s) = crate::depotstates::chips(ui, &p.states, &yours.1, names) {
                        acts.push(Act::SmState(s));
                    }
                });
        }
        egui::Area::new(egui::Id::unique("depot_vp_model"))
            .fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 34.0))
            .interactable(false)
            .show(ctx, |ui| {
                let hash = mercs2_quartermaster::manifest::asset_hash(target);
                ui.label(egui::RichText::new(format!("{target} \u{b7} 0x{hash:08X}")).monospace().color(theme::DIM));
            });
        return acts;
    }
    let yours_path = match item.file.as_ref().ok().and_then(|f| f.contributions.first()) {
        Some(mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { phy2, .. }) => Some(item.dir.join(phy2)),
        _ => None,
    };
    let yours = yours_path.as_deref().and_then(|a| p.yours_for(a)).and_then(Result::ok);
    let shipped = p.shipped_for(target).and_then(Result::ok);
    let (shown, source) = match (&yours, &shipped, p.show_shipped) {
        (Some(y), _, false) => (Some(&**y), "yours"),
        (_, Some(s), _) => (Some(&s.1), "shipped"),
        (Some(y), None, true) => (Some(&**y), "yours"),
        _ => (None, "\u{2014}"),
    };

    if let (false, Some(col)) = (p.hide_collision, shown) {
        let painter = ctx
            .layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::unique("depot_collision")))
            .with_clip_rect(rect);
        let size = ctx.content_rect().size();
        let edge = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(0x8f, 0xbf, 0x4f, 200));
        let fill = Color32::from_rgba_unmultiplied(0x8f, 0xbf, 0x4f, 40);
        let at = |v: [f32; 3]| crate::layerview::project(vp, glam::Vec3::from(v), size).map(|(p, _)| p);
        for o in crate::collision::outlines(&col.shapes) {
            match o {
                crate::collision::Outline::Edges(edges) => {
                    for [a, b] in edges {
                        if let (Some(a), Some(b)) = (at(a), at(b)) {
                            painter.line_segment([a, b], edge);
                        }
                    }
                }
                crate::collision::Outline::Hull(pts) => {
                    let projected: Option<Vec<[f32; 2]>> = pts.iter().map(|v| at(*v).map(|p| [p.x, p.y])).collect();
                    let Some(projected) = projected else { continue };
                    let h = crate::collision::hull_2d(projected);
                    if h.len() >= 3 {
                        let poly: Vec<egui::Pos2> = h.iter().map(|q| egui::pos2(q[0], q[1])).collect();
                        painter.add(egui::Shape::convex_polygon(poly, fill, edge));
                    }
                }
            }
        }
    }

    // Toggles, under the viewport's Orbit chip.
    egui::Area::new(egui::Id::unique("depot_vp_toggles"))
        .fixed_pos(rect.left_top() + egui::vec2(14.0, 40.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if theme::pill(ui, "Show collision", !p.hide_collision).clicked() {
                    acts.push(Act::ToggleCollision);
                }
                if yours.is_some() && shipped.is_some() {
                    if theme::pill(ui, "Yours", !p.show_shipped).clicked() {
                        acts.push(Act::ShowShipped(false));
                    }
                    if theme::pill(ui, "Shipped", p.show_shipped).clicked() {
                        acts.push(Act::ShowShipped(true));
                    }
                }
            });
        });
    // Legend, top right.
    egui::Area::new(egui::Id::unique("depot_vp_legend"))
        .fixed_pos(egui::pos2(rect.right() - 200.0, rect.top() + 12.0))
        .interactable(false)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgba_unmultiplied(14, 16, 20, 217))
                .stroke(egui::Stroke::new(1.0, theme::LINE))
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_width(168.0);
                    ui.label(theme::disp_text("COLLISION OVERLAY", theme::style_eyebrow(), theme::DIM));
                    theme::kv(ui, "source", egui::RichText::new(source));
                    if let Some(c) = shown {
                        theme::kv(ui, "meshes", egui::RichText::new(format!("{} \u{b7} {} tris", c.meshes, commas(c.tris))));
                        theme::kv(ui, "hulls", egui::RichText::new(format!("{} convex", c.hulls)));
                        theme::kv(ui, "MOPP", egui::RichText::new(format!("{} BV-trees", c.mopp_trees)));
                    }
                });
        });
    // The model, bottom left.
    egui::Area::new(egui::Id::unique("depot_vp_model"))
        .fixed_pos(egui::pos2(rect.left() + 12.0, rect.bottom() - 34.0))
        .interactable(false)
        .show(ctx, |ui| {
            let hash = mercs2_quartermaster::manifest::asset_hash(target);
            ui.label(egui::RichText::new(format!("{target} \u{b7} 0x{hash:08X}")).monospace().color(theme::DIM));
        });
    if shipped.is_none() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    acts
}

fn check_row(ui: &mut egui::Ui, dot: Color32, label: &str, note: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, dot);
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(note).monospace().color(theme::DIM)).truncate());
        });
    });
}

/// The item's files, relative to its folder, in path order.
fn item_files(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(base, &p, out);
            } else if let Ok(r) = p.strip_prefix(base) {
                out.push(r.display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out
}

// ─────────────────────────────────────────────────────────────── verbs

/// The bar over the centre. All items (`DepotBrowser`): search, Re-check all, then Sort and the
/// Tiles/List switch at the right. An open item (`DepotLua`'s toolbar): New item, Re-check, Reveal
/// folder, then Mark Ready or Back to Draft at the right.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(ws) = p.workspace() else { return acts };
    let open = p.open.as_ref().and_then(|d| ws.items.iter().find(|i| &i.dir == d));
    match open {
        None => {
            let mut q = p.search.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut q)
                        .hint_text("Search name, kind or target")
                        .margin(theme::TEXT_EDIT_MARGIN)
                        .desired_width(300.0),
                )
                .changed()
            {
                acts.push(Act::Search(q));
            }
            if ui.button("Re-check all").clicked() {
                acts.push(Act::Rescan);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for v in [View::List, View::Tiles] {
                    let label = if v == View::Tiles { "Tiles" } else { "List" };
                    if theme::pill(ui, label, p.view() == v).clicked() {
                        acts.push(Act::View(v));
                    }
                }
                let sort_label = match p.sort() {
                    Sort::Modified => "Modified",
                    Sort::Name => "Name",
                };
                egui::ComboBox::from_id_salt("depot-sort").selected_text(sort_label).show_ui(ui, |ui| {
                    if ui.selectable_label(p.sort() == Sort::Modified, "Modified").clicked() {
                        acts.push(Act::Sort(Sort::Modified));
                    }
                    if ui.selectable_label(p.sort() == Sort::Name, "Name").clicked() {
                        acts.push(Act::Sort(Sort::Name));
                    }
                });
                ui.label(egui::RichText::new("Sort").small().color(theme::DIM));
            });
        }
        Some(i) => {
            if ui.button("New item\u{2026}").clicked() {
                acts.push(Act::OpenNew);
            }
            if ui.button("Re-check").clicked() {
                acts.push(Act::Rescan);
            }
            if ui.button("Reveal folder").clicked() {
                acts.push(Act::Reveal(i.dir.clone()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match i.state() {
                Some(State::Draft) => {
                    let ok = !ws.has_error(i);
                    let r = theme::primary_button(ui, "Mark Ready", ok);
                    let r = if ok { r } else { r.on_disabled_hover_text("Fix the errors first") };
                    if r.clicked() {
                        acts.push(Act::SetState(i.dir.clone(), State::Ready));
                    }
                }
                Some(State::Ready) if ui.button("Back to Draft").clicked() => {
                    acts.push(Act::SetState(i.dir.clone(), State::Draft));
                }
                _ => {}
            });
        }
    }
    acts
}

// ─────────────────────────────────────────────────────────────── windows

pub fn windows(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    match (&p.new_item, p.workspace()) {
        (Some(n), Some(ws)) => new_item_window(ctx, n, ws, &mut acts),
        _ => ctx.data_mut(|d| d.remove::<bool>(egui::Id::unique("depot.new_item.opened"))),
    }
    if let Some(pr) = &p.prompt {
        let title = if pr.duplicate { "Duplicate item" } else { "Rename item" };
        egui::Window::new(title)
            .id(egui::Id::unique("depot.name_prompt"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(pr.dir.display().to_string()).small().color(theme::DIM));
                let mut name = pr.name.clone();
                let resp = ui.add(egui::TextEdit::singleline(&mut name).margin(theme::TEXT_EDIT_MARGIN).desired_width(280.0));
                if resp.changed() {
                    acts.push(Act::PromptName(name));
                }
                if let Some(e) = &pr.error {
                    ui.label(egui::RichText::new(e).small().color(theme::BAD));
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        acts.push(Act::PromptClose);
                    }
                    let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if theme::primary_button(ui, if pr.duplicate { "Duplicate" } else { "Rename" }, true).clicked() || enter {
                        acts.push(Act::PromptCommit);
                    }
                });
            });
    }
    if let Some(d) = &p.confirm_delete {
        egui::Window::new("Delete item")
            .id(egui::Id::unique("depot.confirm_delete"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!("Move {} to the trash?", d.display()));
                ui.label(egui::RichText::new("Copies already in a Shipment are not touched.").small().color(theme::DIM));
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        acts.push(Act::CloseDelete);
                    }
                    if theme::danger_button(ui, "Move to trash", true).clicked() {
                        acts.push(Act::Delete(d.clone()));
                    }
                });
            });
    }
    acts
}

/// Where a layer's kinds land, as the Shipment page's add menu groups them.
fn layer_note(layer: &str) -> &'static str {
    match layer {
        "Data" => "into the overlay WAD",
        "Script" => "through the linker",
        "Code" => "a file beside the game",
        _ => "",
    }
}

/// The kinds the dialog lists for `search`, in `KINDS` order: `(layer, kind, title, description)`.
fn shown_kinds(search: &str) -> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    let needle = search.trim().to_lowercase();
    crate::quartermaster::KINDS
        .iter()
        .flat_map(|(layer, ks)| ks.iter().map(move |(k, t, d)| (*layer, *k, *t, *d)))
        .filter(|(_, k, t, d)| {
            needle.is_empty() || k.contains(&needle) || t.to_lowercase().contains(&needle) || d.to_lowercase().contains(&needle)
        })
        .collect()
}

const NEW_W: f32 = 780.0;
const NEW_H: f32 = 560.0;
const NEW_LEFT_W: f32 = 300.0;

/// `ShipmentCompose`: the kinds on the left, the chosen one on the right, Create at its foot.
fn new_item_window(ctx: &egui::Context, n: &NewItem, ws: &Workspace, acts: &mut Vec<Act>) {
    let shown = shown_kinds(&n.search);
    // ↑ ↓ move through the listed kinds, Enter creates, Esc closes.
    let (up, down, enter, esc) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if esc {
        acts.push(Act::CloseNew);
    }
    // The kind a key moved to is scrolled into view once, when its row is next drawn; a click
    // needs no scroll, and scrolling every frame would fight the wheel.
    let scroll_id = egui::Id::unique("depot.new_item.scroll_to");
    // The dialog opens scrolled to the kind it starts on.
    let opened_id = egui::Id::unique("depot.new_item.opened");
    if !ctx.data(|d| d.get_temp::<bool>(opened_id).unwrap_or(false)) {
        ctx.data_mut(|d| {
            d.insert_temp(opened_id, true);
            d.insert_temp(scroll_id, n.kind.to_string());
        });
    }
    if up || down {
        let at = shown.iter().position(|(_, k, _, _)| *k == n.kind);
        let next = match (at, up) {
            (Some(0), true) | (None, _) => 0,
            (Some(i), true) => i - 1,
            (Some(i), false) => (i + 1).min(shown.len().saturating_sub(1)),
        };
        if let Some((_, k, _, _)) = shown.get(next) {
            acts.push(Act::NewKind(k));
            ctx.data_mut(|d| d.insert_temp(scroll_id, k.to_string()));
        }
    }
    if enter {
        acts.push(Act::Create);
    }

    egui::Window::new("New Depot item")
        .id(egui::Id::unique("depot.new_item"))
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(egui::vec2(NEW_W, NEW_H))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(
            egui::Frame::NONE
                .fill(theme::G2)
                .stroke(egui::Stroke::new(1.0, theme::LINE2))
                .corner_radius(egui::CornerRadius::same(8))
                .shadow(egui::epaint::Shadow { offset: [0, 24], blur: 60, spread: 0, color: Color32::from_black_alpha(180) }),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            // Header: title and close.
            egui::Frame::NONE.inner_margin(egui::Margin { left: 14, right: 12, top: 10, bottom: 10 }).show(ui, |ui| {
                ui.set_width(NEW_W - 26.0);
                ui.horizontal(|ui| {
                    ui.label(theme::disp_text("NEW DEPOT ITEM", theme::style_title(), theme::TX));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::click());
                        let fill = if resp.hovered() { theme::LINE2 } else { theme::G3 };
                        ui.painter().rect_filled(r, egui::CornerRadius::same(4), fill);
                        let s = egui::Stroke::new(1.5, theme::DIM);
                        let c = r.center();
                        ui.painter().line_segment([c + egui::vec2(-4.0, -4.0), c + egui::vec2(4.0, 4.0)], s);
                        ui.painter().line_segment([c + egui::vec2(4.0, -4.0), c + egui::vec2(-4.0, 4.0)], s);
                        if resp.on_hover_text("Close (Esc)").clicked() {
                            acts.push(Act::CloseNew);
                        }
                    });
                });
            });
            let top = ui.cursor().top();
            ui.painter().hline(ui.max_rect().x_range(), top, egui::Stroke::new(1.0, theme::LINE));
            let body_h = NEW_H - (top - ui.min_rect().top());

            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                // Left: search and the kinds.
                ui.allocate_ui_with_layout(egui::vec2(NEW_LEFT_W, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin { left: 12, right: 8, top: 10, bottom: 10 }).show(ui, |ui| {
                        ui.set_width(NEW_LEFT_W - 20.0);
                        ui.set_height(body_h - 20.0);
                        let mut q = n.search.clone();
                        let field = egui::TextEdit::singleline(&mut q)
                            .hint_text("Search kinds")
                            .margin(theme::TEXT_EDIT_MARGIN)
                            .desired_width(f32::INFINITY);
                        if ui.add(field).changed() {
                            acts.push(Act::NewSearch(q));
                        }
                        ui.add_space(6.0);
                        theme::scroll_vertical().id_salt("depot-new-kinds").auto_shrink([false, false]).show(ui, |ui| {
                            let mut last = "";
                            if shown.is_empty() {
                                ui.label(egui::RichText::new("No kind matches.").color(theme::DIM));
                            }
                            for (layer, k, _, blurb) in &shown {
                                if *layer != last {
                                    if !last.is_empty() {
                                        ui.add_space(8.0);
                                    }
                                    last = layer;
                                    ui.horizontal(|ui| {
                                        ui.label(theme::disp_text(layer.to_uppercase(), theme::style_eyebrow(), theme::BRASS_DK));
                                        let note = layer_note(layer);
                                        if !note.is_empty() {
                                            ui.label(egui::RichText::new(format!("\u{b7} {note}")).small().color(theme::FAINT));
                                        }
                                    });
                                }
                                let on = n.kind == *k;
                                let resp = egui::Frame::NONE
                                    .fill(if on { theme::BRASS_SOFT } else { Color32::TRANSPARENT })
                                    .corner_radius(egui::CornerRadius::same(4))
                                    .inner_margin(egui::Margin::symmetric(8, 5))
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(egui::RichText::new(*k).monospace().color(if on { theme::BRASS } else { theme::TX }));
                                        ui.add(egui::Label::new(egui::RichText::new(*blurb).small().color(theme::DIM)).truncate());
                                    })
                                    .response
                                    .interact(egui::Sense::click());
                                // Held until the row is in view: a window's first frames lay out
                                // before it has its size, and a scroll asked for then lands short.
                                if on && ctx.data(|d| d.get_temp::<String>(scroll_id)).as_deref() == Some(*k) {
                                    if ui.clip_rect().contains_rect(resp.rect) {
                                        ctx.data_mut(|d| d.remove::<String>(scroll_id));
                                    } else {
                                        resp.scroll_to_me(Some(egui::Align::Center));
                                    }
                                }
                                if resp.clicked() {
                                    acts.push(Act::NewKind(k));
                                }
                            }
                        });
                    });
                });
                let (sep_top, sep_bot) = (ui.min_rect().top(), ui.min_rect().top() + body_h);
                let x = ui.cursor().left();
                ui.painter().vline(x, egui::Rangef::new(sep_top, sep_bot), egui::Stroke::new(1.0, theme::LINE));

                // Right: the chosen kind, what it writes, the name and folder, Create.
                let right_w = NEW_W - NEW_LEFT_W;
                ui.allocate_ui_with_layout(egui::vec2(right_w, body_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    egui::Frame::NONE
                        .fill(theme::G1)
                        .corner_radius(egui::CornerRadius { nw: 0, ne: 0, sw: 0, se: 8 })
                        .inner_margin(egui::Margin { left: 16, right: 16, top: 12, bottom: 12 })
                        .show(ui, |ui| {
                            ui.set_width(right_w - 32.0);
                            ui.set_height(body_h - 24.0);
                            new_item_detail(ui, n, ws, acts);
                        });
                });
            });
        });
}

fn new_item_detail(ui: &mut egui::Ui, n: &NewItem, ws: &Workspace, acts: &mut Vec<Act>) {
    let (title, blurb) = crate::quartermaster::KINDS
        .iter()
        .flat_map(|(_, ks)| ks.iter())
        .find(|(k, _, _)| *k == n.kind)
        .map(|(_, t, d)| (*t, *d))
        .unwrap_or(("", ""));
    ui.label(egui::RichText::new(n.kind).text_style(TextStyle::Heading).monospace().color(theme::TX));
    ui.label(egui::RichText::new(format!("{title} \u{b7} {blurb}")).color(theme::DIM));
    ui.add_space(12.0);
    ui.label(theme::disp_text("WRITES INTO MANIFEST.YAML", theme::style_eyebrow(), theme::DIM));
    let stub = crate::quartermaster::seeded(n.kind, "", 1)
        .map(|c| serde_norway::to_string(&vec![c]).unwrap_or_else(|e| format!("# cannot serialize: {e}")))
        .unwrap_or_else(|| format!("# no stub for {}", n.kind));
    egui::Frame::NONE
        .fill(theme::G0)
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(8, 6))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            theme::scroll_vertical().id_salt("depot-new-stub").max_height(170.0).show(ui, |ui| {
                ui.add(egui::Label::new(egui::RichText::new(stub.trim_end()).monospace().color(theme::DIM)).selectable(true));
            });
        });
    ui.add_space(12.0);
    ui.label(theme::disp_text("NAME", theme::style_eyebrow(), theme::DIM));
    let mut name = n.name.clone();
    let resp = ui.add(
        egui::TextEdit::singleline(&mut name)
            .hint_text("the item's folder name")
            .margin(theme::TEXT_EDIT_MARGIN)
            .desired_width(f32::INFINITY),
    );
    if resp.changed() {
        acts.push(Act::NewName(name));
    }
    ui.add_space(8.0);
    ui.label(theme::disp_text("FOLDER", theme::style_eyebrow(), theme::DIM));
    let shown = if n.folder.is_empty() { "(workspace root)" } else { n.folder.as_str() };
    egui::ComboBox::from_id_salt("depot-new-folder")
        .selected_text(shown)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            if ui.selectable_label(n.folder.is_empty(), "(workspace root)").clicked() {
                acts.push(Act::NewFolder(String::new()));
            }
            for (f, _) in &ws.folders {
                if ui.selectable_label(&n.folder == f, f).clicked() {
                    acts.push(Act::NewFolder(f.clone()));
                }
            }
        });
    // Create sits at the foot of the pane.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
        ui.label(egui::RichText::new("\u{2191} \u{2193} move \u{b7} Enter create \u{b7} Esc close").small().color(theme::FAINT));
        ui.add_space(4.0);
        let create = egui::Button::new(
            egui::RichText::new(format!("Create {} draft", n.kind)).color(Color32::from_rgb(0x1c, 0x16, 0x06)).strong(),
        )
        .fill(theme::BRASS)
        .stroke(egui::Stroke::new(1.0, theme::BRASS))
        .min_size(egui::vec2(ui.available_width(), 30.0));
        if ui.add(create).clicked() {
            acts.push(Act::Create);
        }
        if let Some(e) = &n.error {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(e).small().color(theme::BAD));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("m2ws_depotpage_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make(root: &Path, folder: &str, name: &str, kind: &'static str) -> PathBuf {
        let ws = depot::scan(root, None).unwrap();
        let c = crate::quartermaster::seeded(kind, "", 1).unwrap();
        depot::create_item(&ws, folder, name, vec![c]).unwrap()
    }

    #[test]
    fn filters_narrow_by_folder_state_kind_and_search() {
        let root = temp("filters");
        make(&root, "missions", "spawn_lists", "patch_lua");
        make(&root, "world", "site", "edit_world");
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        assert_eq!(p.visible().len(), 2);

        p.apply(Act::Folder(Some("world".into())), Some(&root), None);
        assert_eq!(p.visible().iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["site"]);
        p.apply(Act::Folder(None), Some(&root), None);

        p.apply(Act::ToggleKind("edit_world"), Some(&root), None);
        assert_eq!(p.visible().iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["spawn_lists"]);
        p.apply(Act::ToggleKind("edit_world"), Some(&root), None);

        p.apply(Act::Search("GURCON".into()), Some(&root), None);
        assert!(p.visible().is_empty(), "the stub's target is not gurcon002");
        p.apply(Act::Search("spawn".into()), Some(&root), None);
        assert_eq!(p.visible().len(), 1);
        p.apply(Act::Search(String::new()), Some(&root), None);

        p.apply(Act::ToggleState(State::Draft), Some(&root), None);
        assert!(p.visible().is_empty());
    }

    #[test]
    fn new_item_makes_a_draft_and_selects_it_and_refuses_a_taken_name() {
        let root = temp("new");
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        p.apply(Act::OpenNew, Some(&root), None);
        p.apply(Act::NewKind("patch_lua"), Some(&root), None);
        p.apply(Act::NewName("gurcon002_spawn_lists".into()), Some(&root), None);
        p.apply(Act::Create, Some(&root), None);
        assert!(p.new_item.is_none());
        let dir = root.join("gurcon002_spawn_lists");
        assert_eq!(p.selected.as_ref(), Some(&dir));
        assert_eq!(depot::read_item(&dir).unwrap().state, State::Draft);

        p.apply(Act::OpenNew, Some(&root), None);
        p.apply(Act::NewName("gurcon002_spawn_lists".into()), Some(&root), None);
        p.apply(Act::Create, Some(&root), None);
        assert!(p.new_item.as_ref().unwrap().error.as_ref().unwrap().contains("already exists"));
    }

    #[test]
    fn rename_and_duplicate_keep_names_unique_and_a_duplicate_is_a_draft() {
        let root = temp("rename");
        let dir = make(&root, "", "a", "patch_lua");
        let mut f = depot::read_item(&dir).unwrap();
        f.state = State::Ready;
        depot::write_item(&dir, &f).unwrap();
        let mut p = Panel::default();
        p.refresh(Some(&root), None);

        p.apply(Act::Prompt { dir: dir.clone(), duplicate: true }, Some(&root), None);
        assert_eq!(p.prompt.as_ref().unwrap().name, "a_copy");
        p.apply(Act::PromptCommit, Some(&root), None);
        let copy = root.join("a_copy");
        assert_eq!(depot::read_item(&copy).unwrap().state, State::Draft);
        assert_eq!(depot::read_item(&dir).unwrap().state, State::Ready);

        p.apply(Act::Prompt { dir: copy.clone(), duplicate: false }, Some(&root), None);
        p.apply(Act::PromptName("a".into()), Some(&root), None);
        p.apply(Act::PromptCommit, Some(&root), None);
        assert!(p.prompt.as_ref().unwrap().error.as_ref().unwrap().contains("already exists"));
        p.apply(Act::PromptName("b".into()), Some(&root), None);
        p.apply(Act::PromptCommit, Some(&root), None);
        assert!(root.join("b").join(depot::ITEM_FILE).is_file());
        assert!(!copy.exists());
    }

    #[test]
    fn mark_ready_is_refused_with_errors_and_the_reason_is_shown() {
        let root = temp("ready");
        let dir = make(&root, "", "a", "patch_lua");
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        p.apply(Act::SetState(dir.clone(), State::Ready), Some(&root), None);
        assert!(matches!(&p.status, Some(Err(e)) if e.contains("check error")), "{:?}", p.status);
        assert_eq!(depot::read_item(&dir).unwrap().state, State::Draft);
    }

    /// A choice in the spawn table is the edit of the item's `set_spawn_lists` rows: written into
    /// `item.yaml` in place, so a comment the author wrote stays; choosing the shipped list again
    /// takes the row out.
    #[test]
    fn choosing_a_spawn_list_rewrites_the_rows_in_place() {
        use mercs2_quartermaster::manifest::Contribution;
        use mercs2_quartermaster::spawn_lists::{calls, set};
        let root = temp("spawn");
        let ws = depot::scan(&root, None).unwrap();
        let c = Contribution::SetSpawnLists { target: "m".into(), lists: Vec::new() };
        let dir = depot::create_item(&ws, "", "lists", vec![c]).unwrap();
        let yaml = dir.join(depot::ITEM_FILE);
        let by_hand = format!("# the tower's roof\n{}", std::fs::read_to_string(&yaml).unwrap());
        std::fs::write(&yaml, &by_hand).unwrap();
        let script = "function Setup()\n  Tower = 1\n  Ai.TweakAttachedSpawnersInGroup(Tower, \"Rooftop\", {SpawnList = \"A\"})\nend\n";
        let call = calls(script).unwrap().remove(0);
        let mut p = Panel::default();
        p.refresh(Some(&root), None);

        let rows = set(&[], &call, "B");
        let edit = |lists| Act::Edit { dir: dir.clone(), index: 0, c: Box::new(Contribution::SetSpawnLists { target: "m".into(), lists }) };
        p.apply(edit(rows.clone()), Some(&root), None);
        assert!(matches!(&p.status, Some(Ok(_))), "{:?}", p.status);
        let text = std::fs::read_to_string(&yaml).unwrap();
        assert!(text.starts_with("# the tower's roof\n"), "{text}");
        assert!(text.contains("  - function: Setup\n    building: Tower\n    group: Rooftop\n    list: B\n"), "{text}");
        assert_eq!(p.workspace().unwrap().items[0].checks.errors(), 0, "the rows lint");

        p.apply(edit(set(&rows, &call, "A")), Some(&root), None);
        assert_eq!(std::fs::read_to_string(&yaml).unwrap(), by_hand, "back to the shipped list");
    }

    /// The shipped corpus: gurcon002's nine `InGroup` calls (lines 530–561), the six lists the
    /// `vz.wad` scripts set, and qm's check of rows against the corpus folder the bundle ships.
    #[test]
    fn the_shipped_corpus_reads_as_the_board_shows_and_qm_checks_it() {
        use mercs2_quartermaster::manifest::{Contribution, SpawnListSet};
        let home = crate::index::data_home()
            .unwrap_or_else(|e| panic!("the spawn-list gate reads the reference bundle's Lua corpus: {e}"));
        let corpus = crate::index::lua_corpus_at(&home).expect("the reference bundle's Lua corpus");
        let (path, src) = shipped_script(&corpus, "gurcon002").expect("gurcon002 is in the corpus");
        assert_eq!(path, "lua/vz/gurcon002.lua");
        let c = mercs2_quartermaster::spawn_lists::calls(src).unwrap();
        let in_group: Vec<u32> = c.iter().filter(|c| c.group.is_some()).map(|c| c.line).collect();
        assert_eq!(in_group, [530, 533, 536, 543, 546, 549, 555, 558, 561]);
        assert!(c.iter().filter(|c| c.group.is_some()).all(|c| c.function == "SetupDangerousObjBuildings" && c.editable.is_ok()));
        let names = spawn_list_names(&corpus);
        assert_eq!(
            names,
            [
                "Spawnlist (Guerilla Ground)",
                "Spawnlist (VZ AA)",
                "Spawnlist (VZ Balcony)",
                "Spawnlist (VZ Elite)",
                "Spawnlist (VZ Ground)",
                "Spawnlist (VZ Tower)"
            ]
        );

        let roof = &c[0];
        let rows = |list: &str| {
            vec![SpawnListSet { function: roof.function.clone(), building: roof.building.clone(), group: roof.group.clone(), list: list.into() }]
        };
        let manifest = |lists| {
            let contributions = vec![Contribution::SetSpawnLists { target: "gurcon002".into(), lists }];
            depot::item_manifest(&depot::ItemFile { state: State::Draft, contributions })
        };
        let lua = home.join("lua");
        assert_eq!(mercs2_quartermaster::lint::spawn_list_source_checks(&manifest(rows("Spawnlist (VZ Elite)")), &lua), vec![]);
        let bad = mercs2_quartermaster::lint::spawn_list_source_checks(&manifest(rows("Spawnlist (VZ Elit)")), &lua);
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].message.contains("no shipped script sets the list \"Spawnlist (VZ Elit)\""), "{}", bad[0].message);
    }

    /// A string-table editor write lands when the file is still what the edit was made from, and
    /// is refused, leaving the file as it is, when it changed on disk since.
    #[test]
    fn a_text_write_is_refused_when_the_file_changed_since_it_was_read() {
        let root = temp("text");
        let dir = make(&root, "", "t", "edit_stringdb");
        let path = dir.join("src/strings.txt");
        let mut p = Panel::default();
        p.refresh(Some(&root), None);

        p.apply(Act::TextWrite { path: path.clone(), prior: String::new(), text: "[A] = a\n".into() }, Some(&root), None);
        assert!(matches!(&p.status, Some(Ok(_))), "{:?}", p.status);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[A] = a\n");

        std::fs::write(&path, "[A] = edited elsewhere\n").unwrap();
        p.apply(Act::TextWrite { path: path.clone(), prior: "[A] = a\n".into(), text: "[A] = b\n".into() }, Some(&root), None);
        assert!(matches!(&p.status, Some(Err(e)) if e.contains("changed on disk")), "{:?}", p.status);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[A] = edited elsewhere\n");
    }

    #[test]
    fn a_form_edit_rewrites_the_contribution_and_keeps_the_state() {
        let root = temp("edit");
        let dir = make(&root, "", "a", "patch_lua");
        let mut f = depot::read_item(&dir).unwrap();
        f.state = State::Ready;
        depot::write_item(&dir, &f).unwrap();
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        let c = mercs2_quartermaster::manifest::Contribution::PatchLua { target: "gurcon002".into(), append: "src/x.lua".into() };
        p.apply(Act::Edit { dir: dir.clone(), index: 0, c: Box::new(c.clone()) }, Some(&root), None);
        let f = depot::read_item(&dir).unwrap();
        assert_eq!(f.contributions, vec![c]);
        assert_eq!(f.state, State::Ready);
        p.apply(Act::Edit { dir: dir.clone(), index: 5, c: Box::new(f.contributions[0].clone()) }, Some(&root), None);
        assert!(matches!(&p.status, Some(Err(e)) if e.contains("no contribution 5")));
    }

    /// A form edit made after `item.yaml` changed on disk is refused, and the hand edit stays.
    #[test]
    fn a_form_edit_is_refused_once_the_item_file_changed_on_disk() {
        let root = temp("edit_stale");
        let dir = make(&root, "", "a", "patch_lua");
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        let by_hand = format!("# by hand\n{}", std::fs::read_to_string(dir.join("item.yaml")).unwrap());
        std::fs::write(dir.join("item.yaml"), &by_hand).unwrap();
        let c = mercs2_quartermaster::manifest::Contribution::PatchLua { target: "gurcon002".into(), append: "src/x.lua".into() };
        p.apply(Act::Edit { dir: dir.clone(), index: 0, c: Box::new(c) }, Some(&root), None);
        assert!(matches!(&p.status, Some(Err(e)) if e.contains("changed on disk")), "{:?}", p.status);
        assert_eq!(std::fs::read_to_string(dir.join("item.yaml")).unwrap(), by_hand);
    }

    /// Edit by hand saves over the file it opened, and is refused, keeping the editor open and the
    /// file as it is, when the file changed on disk after it opened.
    #[test]
    fn a_hand_save_is_refused_once_the_file_changed_on_disk() {
        let root = temp("hand");
        let dir = make(&root, "", "h", "patch_lua");
        let file = PathBuf::from("src/a.lua");
        std::fs::write(dir.join(&file), "local a = 1\n").unwrap();
        let mut p = Panel::default();
        p.refresh(Some(&root), None);
        p.apply(Act::HandStart { dir: dir.clone(), file: file.clone() }, Some(&root), None);
        p.apply(Act::HandText("local a = 2\n".into()), Some(&root), None);
        p.apply(Act::HandSave, Some(&root), None);
        assert_eq!(std::fs::read_to_string(dir.join(&file)).unwrap(), "local a = 2\n");
        assert!(p.hand.is_none());

        p.apply(Act::HandStart { dir: dir.clone(), file: file.clone() }, Some(&root), None);
        std::fs::write(dir.join(&file), "local a = 3 -- elsewhere\n").unwrap();
        p.apply(Act::HandText("local a = 4\n".into()), Some(&root), None);
        p.apply(Act::HandSave, Some(&root), None);
        assert!(p.hand.as_ref().and_then(|h| h.error.as_deref()).is_some_and(|e| e.contains("changed on disk")), "{:?}", p.hand);
        assert_eq!(std::fs::read_to_string(dir.join(&file)).unwrap(), "local a = 3 -- elsewhere\n");
    }

    /// Save shipped as .phy2 on the retail destroyer: the body lands in the item's `src/`, the
    /// item points at it, and the missing-source finding goes.
    #[cfg(feature = "retail")]
    #[test]
    fn save_shipped_writes_the_destroyer_collision_into_the_item() {
        let root = temp("phy2");
        let ws = depot::scan(&root, None).unwrap();
        let c = mercs2_quartermaster::manifest::Contribution::ReplacePhy2 {
            target: "al_veh_boat_destroyer".into(),
            phy2: PathBuf::from("src/collision.phy2"),
        };
        let dir = depot::create_item(&ws, "", "destroyer_collision", vec![c]).unwrap();
        let mut p = Panel::default();
        p.set_game(&[crate::app::retail_vz_wad()]);
        p.refresh(Some(&root), None);
        assert!(p.workspace().unwrap().items[0].checks.lint.iter().any(|d| d.rule.code == "M0110"));
        let started = std::time::Instant::now();
        let shipped = loop {
            if let Some(r) = p.shipped_for("al_veh_boat_destroyer") {
                break r.expect("the destroyer's PHY2 reads");
            }
            assert!(started.elapsed().as_secs() < 120, "the shipped read did not finish");
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        p.apply(
            Act::SaveShipped { dir: dir.clone(), index: 0, target: "al_veh_boat_destroyer".into() },
            Some(&root),
            None,
        );
        assert!(matches!(&p.status, Some(Ok(_))), "{:?}", p.status);
        let saved = dir.join("src/al_veh_boat_destroyer.phy2");
        assert_eq!(std::fs::read(&saved).unwrap(), shipped.0);
        let f = depot::read_item(&dir).unwrap();
        assert!(matches!(&f.contributions[0], mercs2_quartermaster::manifest::Contribution::ReplacePhy2 { phy2, .. } if phy2 == &PathBuf::from("src/al_veh_boat_destroyer.phy2")));
        let item = &p.workspace().unwrap().items[0];
        assert!(!item.checks.lint.iter().any(|d| d.rule.code == "M0110"), "{:?}", item.checks.lint);
        assert_eq!(p.yours_for(&saved).unwrap().unwrap().tris, 1078);

        // Saved again, the file it wrote is there: refused, and the file is left as it is.
        std::fs::write(&saved, b"by hand").unwrap();
        p.apply(
            Act::SaveShipped { dir: dir.clone(), index: 0, target: "al_veh_boat_destroyer".into() },
            Some(&root),
            None,
        );
        assert!(matches!(&p.status, Some(Err(e)) if e.contains("exists")), "{:?}", p.status);
        assert_eq!(std::fs::read(&saved).unwrap(), b"by hand");
    }

    #[test]
    fn the_page_rereads_when_the_workspace_setting_changes() {
        let a = temp("cur_a");
        let b = temp("cur_b");
        let mut p = Panel::default();
        assert!(p.is_current(None));
        assert!(!p.is_current(Some(&a)));
        p.refresh(Some(&a), None);
        assert!(p.is_current(Some(&a)));
        assert!(!p.is_current(Some(&b)));
        assert!(!p.is_current(None));
    }
}
