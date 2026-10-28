//! The Audio page — Inspect's AUDIO tab: the voice-over lines by the banks that play them, search,
//! selection, preview, export, and the routes into a Shipment
//! (`docs/modernization/workshop_pages/audio.md`).
//!
//! The clip inventory ([`AudioIndex`]) is the bundle's; which bank and cue plays each line
//! ([`Routes`]) is read from `English.wad` on a worker the first time the tab is shown. Playback and
//! the Shipment live in the app: the page asks for them through [`Panel::play`] and
//! [`Panel::shipment`], and is told what is playing as an [`audioplay::Status`].

use crate::audioexport::{self, Item};
use crate::audioidx::{AudioClip, AudioIndex};
use crate::audioplay;
use crate::audioroute::{Play, Routes};
use crate::gui::theme;
use mercs2_quartermaster::manifest::Contribution;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use winit::keyboard::KeyCode;

/// Which list the centre shows when no search is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Open {
    /// The bank at this index of [`Routes::banks`].
    Bank(usize),
    /// The lines no cue plays.
    Unplayed,
}

/// What the EXPORT card exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Selected,
    /// Every line the open bank lists.
    Bank,
    /// Every search hit.
    Hits,
}

/// Where the routing is.
enum RouteLoad {
    NotStarted,
    Loading(Receiver<Result<Routes, String>>),
    Ready(Box<Routes>),
    Failed(String),
}

enum ExportMsg {
    Progress(usize, usize),
    Done(Result<audioexport::Report, String>),
}

/// The page's state.
pub struct Panel {
    index: AudioIndex,
    /// Record index → clip of [`AudioIndex::clips`].
    by_wave: HashMap<u32, usize>,
    routes: RouteLoad,
    /// The configured `vz.wad`: `English.wad` and `Audios/` sit beside it.
    vz: Option<PathBuf>,
    pub open: Option<Open>,
    /// The search over every clip; non-empty puts the centre in search mode.
    pub search: String,
    /// Narrows the open bank's list.
    pub filter: String,
    /// Selected record indices.
    pub selected: BTreeSet<u32>,
    /// The clip the inspector and verbs act on.
    pub current: Option<u32>,
    pub scope: Scope,
    /// The folder exports go under.
    pub folder: PathBuf,
    export: Option<Receiver<ExportMsg>>,
    progress: Option<(usize, usize)>,
    /// The last export's outcome, for the EXPORT card.
    pub note: Option<Result<String, String>>,
    /// A clip the app should play, or pause / resume when it is the one playing.
    pub play: Option<u32>,
    /// A contribution the app should add to the open Shipment (scaffolding one), then show.
    pub shipment: Option<Contribution>,
    /// The app should ask for an export folder.
    pub pick_folder: bool,
}

impl Default for Panel {
    fn default() -> Panel {
        Panel {
            index: AudioIndex::default(),
            by_wave: HashMap::new(),
            routes: RouteLoad::NotStarted,
            vz: None,
            open: None,
            search: String::new(),
            filter: String::new(),
            selected: BTreeSet::new(),
            current: None,
            scope: Scope::Selected,
            folder: PathBuf::from(crate::app::EXPORT_ROOT),
            export: None,
            progress: None,
            note: None,
            play: None,
            shipment: None,
            pick_folder: false,
        }
    }
}

/// What the page's widgets and keys ask for.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Open(Open),
    Search(String),
    Filter(String),
    /// Toggle a clip in the selection (its checkbox).
    Toggle(u32),
    /// Make a clip the current one (a row click).
    Current(u32),
    SelectAll,
    Clear,
    /// Play the clip, or pause / resume it when it is playing.
    Play(u32),
    /// Play the current clip, or pause / resume it.
    PlayCurrent,
    AddLine(u32),
    /// Replace cue `play` (an index into the clip's plays) of clip `wave`.
    AddCue(u32, usize),
    AddBank(u32),
    /// An unseeded stub of a sound kind, or `replace_sound_bank` seeded with the open bank.
    AddKind(&'static str),
    Scope(Scope),
    PickFolder,
    Export,
}

/// The sound kinds ADD TO THIS DOMAIN offers.
pub const KINDS: [&str; 4] = ["add_sound", "replace_sound_bank", "replace_sound_cue", "replace_sound_wave"];

impl Panel {
    /// The inventory, once the boot thread has it.
    pub fn set_index(&mut self, index: AudioIndex) {
        self.by_wave = index.clips.iter().enumerate().filter_map(|(i, c)| c.wave_index.map(|w| (w, i))).collect();
        self.index = index;
    }

    /// Start reading the routing from `English.wad` beside `vz` (once).
    pub fn ensure_routes(&mut self, vz: &Path) {
        if !matches!(self.routes, RouteLoad::NotStarted) {
            return;
        }
        self.vz = Some(vz.to_path_buf());
        let labels: Vec<String> = self.index.banks().into_iter().map(|(b, _)| b).collect();
        let vz = vz.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Routes::build(&vz, &labels));
        });
        self.routes = RouteLoad::Loading(rx);
    }

    /// Take in what the workers have finished.
    pub fn poll(&mut self) {
        if let RouteLoad::Loading(rx) = &self.routes {
            match rx.try_recv() {
                Ok(Ok(r)) => self.set_routes(r),
                Ok(Err(e)) => self.routes = RouteLoad::Failed(e),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.routes = RouteLoad::Failed("the routing worker stopped without an answer".into())
                }
            }
        }
        if let Some(rx) = &self.export {
            loop {
                match rx.try_recv() {
                    Ok(ExportMsg::Progress(d, t)) => self.progress = Some((d, t)),
                    Ok(ExportMsg::Done(r)) => {
                        self.note = Some(r.map(|r| format!("{} clip(s) and captions.csv → {}", r.clips, r.dir.display())));
                        self.export = None;
                        self.progress = None;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.note = Some(Err("the export stopped without an answer".into()));
                        self.export = None;
                        self.progress = None;
                        break;
                    }
                }
            }
        }
    }

    fn set_routes(&mut self, r: Routes) {
        if self.open.is_none() {
            self.open = (!r.banks.is_empty()).then_some(Open::Bank(0));
        }
        self.routes = RouteLoad::Ready(Box::new(r));
    }

    pub fn routes(&self) -> Option<&Routes> {
        match &self.routes {
            RouteLoad::Ready(r) => Some(r),
            _ => None,
        }
    }

    pub fn routes_error(&self) -> Option<&str> {
        match &self.routes {
            RouteLoad::Failed(e) => Some(e),
            _ => None,
        }
    }

    pub fn clip(&self, wave: u32) -> Option<&AudioClip> {
        self.by_wave.get(&wave).map(|&i| &self.index.clips[i])
    }

    /// Search mode: a search is typed.
    pub fn searching(&self) -> bool {
        !self.search.trim().is_empty()
    }

    /// Every search hit, across banks, in inventory order.
    pub fn hits(&self) -> Vec<u32> {
        self.index.search(&self.search, usize::MAX).into_iter().filter_map(|c| c.wave_index).collect()
    }

    /// The open list before the filter: the open bank's lines, or the unplayed ones.
    fn open_lines(&self) -> Vec<u32> {
        match (self.routes(), self.open) {
            (Some(r), Some(Open::Bank(i))) => r.banks.get(i).map(|b| b.lines.clone()).unwrap_or_default(),
            (Some(r), Some(Open::Unplayed)) => r.unplayed.clone(),
            _ => Vec::new(),
        }
    }

    /// The rows the table lists: the search hits, or the open list narrowed by the filter (caption,
    /// speaker or file name, case-insensitive).
    pub fn rows(&self) -> Vec<u32> {
        if self.searching() {
            return self.hits();
        }
        let f = self.filter.trim().to_lowercase();
        self.open_lines()
            .into_iter()
            .filter(|w| {
                f.is_empty()
                    || self.clip(*w).is_some_and(|c| {
                        format!("{} {} {}", c.caption, c.speaker, c.original).to_lowercase().contains(&f)
                    })
            })
            .collect()
    }

    /// The open list's name: a bank's, or the unplayed group's.
    pub fn open_name(&self) -> Option<String> {
        match (self.routes(), self.open) {
            (Some(r), Some(Open::Bank(i))) => r.banks.get(i).map(|b| b.name.clone()),
            (_, Some(Open::Unplayed)) => Some("not played by any cue".into()),
            _ => None,
        }
    }

    /// The banks that play `wave`, joined, or `—`.
    pub fn banks_of(&self, wave: u32) -> String {
        let Some(r) = self.routes() else { return String::new() };
        let names: BTreeSet<String> = r.plays.get(&wave).into_iter().flatten().map(|p| r.bank_name(p.entry)).collect();
        if names.is_empty() {
            "—".into()
        } else {
            names.into_iter().collect::<Vec<_>>().join("; ")
        }
    }

    /// The lines of the chosen export scope.
    pub fn scope_lines(&self, scope: Scope) -> Vec<u32> {
        match scope {
            Scope::Selected => self.selected.iter().copied().collect(),
            Scope::Bank => self.open_lines(),
            Scope::Hits => {
                if self.searching() {
                    self.hits()
                } else {
                    Vec::new()
                }
            }
        }
    }

    /// The scope's export folder name: the bank's or the search's.
    fn scope_name(&self, scope: Scope) -> String {
        match scope {
            Scope::Hits => format!("search_{}", self.search.trim()),
            Scope::Bank => self.open_name().unwrap_or_else(|| "audio".into()),
            Scope::Selected => format!("{}_selection", self.open_name().unwrap_or_else(|| "audio".into())),
        }
    }

    /// The line's authored name, when the inventory knows one.
    fn line_name(&self, wave: u32) -> Option<&str> {
        self.clip(wave).map(|c| c.context.as_str()).filter(|c| !c.is_empty())
    }

    /// The cue's name when a clip's authored name hashes to its guid.
    fn cue_name(&self, play: &Play) -> Option<&str> {
        let r = self.routes()?;
        r.plays
            .iter()
            .flat_map(|(w, ps)| ps.iter().map(move |p| (*w, p)))
            .filter(|(_, p)| p.guid == play.guid)
            .find_map(|(w, _)| self.line_name(w).filter(|n| mercs2_formats::hash::pandemic_hash_m2(n) == play.guid))
    }

    fn stem(&self, wave: u32) -> String {
        self.clip(wave).map(|c| c.stem().to_string()).unwrap_or_else(|| format!("{wave:05}"))
    }

    /// Why Replace this line is unavailable for `wave`, or `None`.
    pub fn line_refusal(&self, wave: u32) -> Option<&'static str> {
        let r = self.routes()?;
        (!r.plays.contains_key(&wave)).then_some("no soundbank plays this line, so replacing it would change nothing")
    }
}

/// A key on the Audio tab: Space plays or pauses the current clip, Ctrl/Cmd+A selects what the table
/// lists, F10 exports.
pub fn shortcut(code: KeyCode, ctrl: bool) -> Option<Act> {
    match code {
        KeyCode::Space => Some(Act::PlayCurrent),
        KeyCode::KeyA if ctrl => Some(Act::SelectAll),
        KeyCode::F10 => Some(Act::Export),
        _ => None,
    }
}

/// The header crumb: `Audio`, then the open bank or the quoted search.
pub fn crumb(p: &Panel) -> Vec<String> {
    let mut c = vec!["Audio".to_string()];
    if p.searching() {
        c.push(format!("\"{}\"", p.search.trim()));
    } else if let Some(n) = p.open_name() {
        c.push(n);
    }
    c
}

/// Land an action on the panel.
pub fn apply(act: Act, p: &mut Panel) {
    match act {
        Act::Open(o) => {
            p.open = Some(o);
            p.filter.clear();
            p.search.clear();
            p.scope = Scope::Selected;
        }
        Act::Search(s) => {
            p.search = s;
            if !p.searching() && p.scope == Scope::Hits {
                p.scope = Scope::Selected;
            }
        }
        Act::Filter(f) => p.filter = f,
        Act::Toggle(w) => {
            if !p.selected.remove(&w) {
                p.selected.insert(w);
            }
        }
        Act::Current(w) => p.current = Some(w),
        Act::SelectAll => p.selected.extend(p.rows()),
        Act::Clear => p.selected.clear(),
        Act::Play(w) => {
            p.current = Some(w);
            p.play = Some(w);
        }
        Act::PlayCurrent => p.play = p.current,
        Act::AddLine(w) => {
            if p.line_refusal(w).is_none() {
                p.shipment = p.routes().and_then(|r| r.line_seed(w, p.line_name(w), &p.stem(w)));
            }
        }
        Act::AddCue(w, i) => {
            let play = p.routes().and_then(|r| r.plays.get(&w)).and_then(|ps| ps.get(i)).cloned();
            if let Some(play) = play {
                let name = p.cue_name(&play).map(str::to_string);
                p.shipment = p.routes().and_then(|r| r.cue_seed(&play, w, name.as_deref(), &p.stem(w)));
            }
        }
        Act::AddBank(entry) => p.shipment = p.routes().and_then(|r| r.bank_seed(entry)),
        Act::AddKind(kind) => {
            let open_bank = match (p.routes(), p.open) {
                (Some(r), Some(Open::Bank(i))) => r.banks.get(i).map(|b| b.entry),
                _ => None,
            };
            p.shipment = match (kind, open_bank) {
                ("replace_sound_bank", Some(entry)) => p.routes().and_then(|r| r.bank_seed(entry)),
                _ => crate::quartermaster::seeded(kind, "", 1),
            };
        }
        Act::Scope(s) => p.scope = s,
        Act::PickFolder => p.pick_folder = true,
        Act::Export => start_export(p),
    }
}

/// `--ui-state`'s AUDIO keys, once the routing is in: the bank by the name the page lists it under,
/// the search, the selection and the current clip. A bank or clip that does not exist is an error
/// naming what it accepts.
pub fn apply_state(p: &mut Panel, st: &crate::ui_state::UiState) -> Result<(), String> {
    if let Some(b) = &st.bank {
        let names: Vec<(&str, usize)> =
            p.routes().ok_or("the routing is not in")?.banks.iter().enumerate().map(|(i, b)| (b.name.as_str(), i)).collect();
        let i = crate::ui_state::pick("bank", b, &names)?;
        apply(Act::Open(Open::Bank(i)), p);
    }
    if let Some(s) = &st.search {
        apply(Act::Search(s.clone()), p);
    }
    for w in st.select.iter().flatten().chain(st.clip.iter()) {
        if p.clip(*w).is_none() {
            return Err(format!("no clip {w}: the inventory's records run 0..{}", p.index.clips.len()));
        }
    }
    p.selected.extend(st.select.iter().flatten().copied());
    p.current = st.clip;
    Ok(())
}

/// The export items for `lines`: each clip's shipped file name, the banks that play it, speaker,
/// caption and length.
pub fn items(p: &Panel, lines: &[u32]) -> Vec<Item> {
    lines
        .iter()
        .filter_map(|w| {
            let c = p.clip(*w)?;
            Some(Item {
                wave: *w,
                file: c.original.rsplit(['/', '\\']).next().unwrap_or(&c.original).to_string(),
                bank: p.banks_of(*w),
                speaker: c.speaker.clone(),
                caption: c.caption.clone(),
                length_s: c.duration_s,
            })
        })
        .collect()
}

/// Export the chosen scope on a worker; the EXPORT card shows its progress and outcome.
fn start_export(p: &mut Panel) {
    if p.export.is_some() {
        return;
    }
    let lines = p.scope_lines(p.scope);
    let Some(vz) = p.vz.clone() else {
        p.note = Some(Err("the game's vz.wad is not known yet".into()));
        return;
    };
    if lines.is_empty() {
        p.note = Some(Err("nothing to export: no clips are in the chosen scope".into()));
        return;
    }
    let items = items(p, &lines);
    let (folder, name) = (p.folder.clone(), p.scope_name(p.scope));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let r = export_items(&vz, &items, &folder, &name, |d, t| {
            let _ = tx.send(ExportMsg::Progress(d, t));
        });
        let _ = tx.send(ExportMsg::Done(r));
    });
    p.note = None;
    p.progress = Some((0, lines.len()));
    p.export = Some(rx);
}

/// Export `items`, reading their samples from `vo_stream.english.pws` beside `vz`.
pub fn export_items(
    vz: &Path,
    items: &[Item],
    folder: &Path,
    name: &str,
    progress: impl FnMut(usize, usize),
) -> Result<audioexport::Report, String> {
    let data = vz.parent().ok_or_else(|| format!("{} has no folder", vz.display()))?;
    let vo = crate::vostream::VoStream::open(data)?;
    audioexport::export(
        items,
        folder,
        name,
        |w| {
            vo.clip_pcm(w).map(|pcm| mercs2_audio::encode::Pcm16 {
                channels: pcm.channels,
                sample_rate: pcm.rate,
                samples: pcm.samples,
            })
        },
        progress,
    )
}

/// The headless `--audio-export <bank|query> <dir>`: the bank of that name (as the page lists it),
/// or else every clip the query finds, exported to `<dir>/<bank or query>`. Returns what was written.
pub fn export_headless(vz: &Path, what: &str, dir: &Path) -> Result<audioexport::Report, String> {
    let home = crate::index::require_data_home(&mut |_, _| {})?;
    let mut p = Panel::default();
    p.set_index(AudioIndex::load(&home)?);
    let labels: Vec<String> = p.index.banks().into_iter().map(|(b, _)| b).collect();
    let routes = Routes::build(vz, &labels)?;
    p.vz = Some(vz.to_path_buf());
    let bank = routes.banks.iter().position(|b| b.name.eq_ignore_ascii_case(what));
    p.routes = RouteLoad::Ready(Box::new(routes));
    let (lines, name) = match bank {
        Some(i) => {
            p.open = Some(Open::Bank(i));
            (p.scope_lines(Scope::Bank), p.scope_name(Scope::Bank))
        }
        None => {
            p.search = what.to_string();
            (p.scope_lines(Scope::Hits), p.scope_name(Scope::Hits))
        }
    };
    if lines.is_empty() {
        return Err(format!("{what:?} is no bank and finds no clip"));
    }
    export_items(vz, &items(&p, &lines), dir, &name, |d, t| eprintln!("[audio-export] {d}/{t}"))
}

// ───────────────────────────────────────────────────────────────────── views

fn secs(s: f32) -> String {
    format!("{s:.1} s")
}

/// The left column below the tab strip: the clip search, the inventory line, the banks, and the
/// sound kinds this domain starts.
pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let mut search = p.search.clone();
    if ui
        .add(egui::TextEdit::singleline(&mut search).hint_text("Search every clip").desired_width(f32::INFINITY))
        .changed()
    {
        acts.push(Act::Search(search));
    }
    ui.add_space(6.0);
    let banks = p.routes().map(|r| r.banks.len()).unwrap_or(0);
    ui.label(theme::disp_text(
        format!("{} CLIP(S) ACROSS {banks} BANKS", p.index.clips.len()),
        theme::style_eyebrow(),
        theme::FAINT,
    ));
    ui.add_space(4.0);
    match (&p.routes, p.routes()) {
        (RouteLoad::Failed(e), _) => {
            ui.label(egui::RichText::new(format!("English.wad could not be routed: {e}")).color(theme::BAD));
        }
        (_, None) => {
            // The spinner also keeps egui repainting, so `poll` runs each frame and the banks
            // appear the frame the worker lands, not on the next pointer move.
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(13.0));
                ui.label(egui::RichText::new("Reading English.wad's soundbanks…").color(theme::FAINT));
            });
        }
        (_, Some(r)) => {
            let row = |ui: &mut egui::Ui, name: &str, n: usize, on: bool| {
                let resp = ui
                    .horizontal(|ui| {
                        let col = if on { theme::BRASS } else { theme::TX };
                        ui.label(egui::RichText::new(name).color(col));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(n.to_string()).monospace().color(theme::FAINT));
                        });
                    })
                    .response;
                let resp = ui.interact(resp.rect, egui::Id::new(("audio-bank", name)), egui::Sense::click());
                if on {
                    ui.painter().rect_filled(resp.rect, egui::Rounding::same(4.0), theme::BRASS_SOFT.gamma_multiply(0.6));
                }
                resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            };
            egui::ScrollArea::vertical().max_height(ui.available_height() - 150.0).auto_shrink([false, false]).show(ui, |ui| {
                for (i, b) in r.banks.iter().enumerate() {
                    if row(ui, &b.name, b.lines.len(), !p.searching() && p.open == Some(Open::Bank(i))) {
                        acts.push(Act::Open(Open::Bank(i)));
                    }
                }
                if !r.unplayed.is_empty() && row(ui, "not played by any cue", r.unplayed.len(), !p.searching() && p.open == Some(Open::Unplayed)) {
                    acts.push(Act::Open(Open::Unplayed));
                }
            });
        }
    }
    ui.add_space(8.0);
    theme::eyebrow(ui, "Add to this domain");
    ui.horizontal_wrapped(|ui| {
        for k in KINDS {
            // A pill goes to the next row whole rather than breaking its label: its width is the
            // label's plus `theme::pill`'s margins (8 each side, 4 after).
            let label = ui.fonts(|f| f.layout_no_wrap(k.to_string(), theme::font(&theme::style_eyebrow()), theme::TX).size().x);
            if ui.available_size_before_wrap().x < label + 20.0 {
                ui.end_row();
            }
            if theme::pill(ui, k, false).on_hover_text(crate::quartermaster::kind_title(k)).clicked() {
                acts.push(Act::AddKind(k));
            }
        }
    });
    acts
}

/// The centre: the open bank's (or the search's) clips, the selection bar and the now-playing card.
pub fn center(ctx: &egui::Context, p: &Panel, playing: Option<audioplay::Status>) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(theme::G0).inner_margin(egui::Margin::symmetric(16.0, 12.0)))
        .show(ctx, |ui| {
            if let Some(e) = p.routes_error() {
                ui.label(egui::RichText::new(format!("English.wad: {e}")).color(theme::BAD));
                return;
            }
            let rows = p.rows();
            // Header: the list's name and size, and the bank filter.
            ui.horizontal(|ui| {
                let title = if p.searching() { format!("SEARCH \"{}\"", p.search.trim()) } else { p.open_name().unwrap_or_default().to_uppercase() };
                ui.label(theme::disp_text(title, theme::style_title(), theme::TX));
                ui.label(egui::RichText::new(format!("{} clips", rows.len())).color(theme::DIM));
                if !p.searching() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mut f = p.filter.clone();
                        if ui.add(egui::TextEdit::singleline(&mut f).hint_text("Filter this bank").desired_width(200.0)).changed() {
                            acts.push(Act::Filter(f));
                        }
                    });
                }
            });
            ui.add_space(6.0);
            // Selection bar.
            ui.horizontal(|ui| {
                let total: f32 = p.selected.iter().filter_map(|w| p.clip(*w)).map(|c| c.duration_s).sum();
                ui.label(theme::disp_text(format!("{} SELECTED", p.selected.len()), theme::style_eyebrow(), theme::BRASS));
                ui.label(egui::RichText::new(secs(total)).monospace().color(theme::DIM));
                if ui.add_enabled(!rows.is_empty(), egui::Button::new(format!("select all {}", rows.len()))).clicked() {
                    acts.push(Act::SelectAll);
                }
                if ui.add_enabled(!p.selected.is_empty(), egui::Button::new("clear")).clicked() {
                    acts.push(Act::Clear);
                }
            });
            ui.add_space(4.0);
            let now = playing.filter(|s| p.clip(s.clip).is_some());
            let card_h = if now.is_some() { 92.0 } else { 0.0 };
            // Table.
            let search = p.searching();
            // Column widths: checkbox, ▶, WAVE, SPEAKER, (BANK), CAPTION takes the rest, LENGTH.
            let widths = [24.0, 30.0, 60.0, 84.0];
            let (bank_w, len_w) = (120.0, 56.0);
            // A left-aligned cell of `w`.
            fn cell(ui: &mut egui::Ui, w: f32, h: f32, add: impl FnOnce(&mut egui::Ui)) {
                ui.allocate_ui_with_layout(egui::vec2(w, h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.set_width(w);
                    add(ui);
                });
            }
            let eyebrow = |t: &str| egui::Label::new(theme::disp_text(t, theme::style_eyebrow(), theme::FAINT));
            ui.horizontal(|ui| {
                ui.add_space(widths[0] + widths[1] + ui.spacing().item_spacing.x * 2.0);
                cell(ui, widths[2], 16.0, |ui| {
                    ui.add(eyebrow("WAVE"));
                });
                cell(ui, widths[3], 16.0, |ui| {
                    ui.add(eyebrow("SPEAKER"));
                });
                if search {
                    cell(ui, bank_w, 16.0, |ui| {
                        ui.add(eyebrow("BANK"));
                    });
                }
                let cap_w = (ui.available_width() - len_w - ui.spacing().item_spacing.x).max(40.0);
                cell(ui, cap_w, 16.0, |ui| {
                    ui.add(eyebrow("CAPTION"));
                });
                cell(ui, len_w, 16.0, |ui| {
                    ui.add(eyebrow("LENGTH"));
                });
            });
            let row_h = 26.0;
            egui::ScrollArea::vertical().max_height(ui.available_height() - card_h).auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                rows.len(),
                |ui, range| {
                    for &w in &rows[range] {
                        let Some(c) = p.clip(w) else { continue };
                        let is_playing = now.is_some_and(|s| s.clip == w);
                        let on = p.selected.contains(&w);
                        let fill = if is_playing {
                            theme::BRASS_SOFT
                        } else if on || p.current == Some(w) {
                            theme::G2
                        } else {
                            egui::Color32::TRANSPARENT
                        };
                        // The row's own click (make it current) is sensed FIRST, and its checkbox, ▶
                        // and cells laid over it after: in egui the widget registered later is on top,
                        // so each keeps its own click and the row gets the rest. Sensed after its
                        // children, the row took every click, ▶ and checkbox included. The cells go
                        // straight into the row's own layout, which centres each on the row's height;
                        // a `horizontal` in between is only `interact_size.y` tall, so it was the one
                        // centred, and the taller ▶ hung out of the row's foot.
                        let (rect, row) =
                            ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h - 2.0), egui::Sense::click());
                        ui.painter().rect_filled(rect, egui::Rounding::same(3.0), fill);
                        let mut content = ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center), None);
                        {
                                let ui = &mut content;
                                let mut checked = on;
                                if ui.add_sized([widths[0], row_h - 4.0], egui::Checkbox::without_text(&mut checked)).changed() {
                                    acts.push(Act::Toggle(w));
                                }
                                let glyph = if is_playing && !now.is_some_and(|s| s.paused) { "\u{23F8}" } else { "\u{25B6}" };
                                if ui.add_sized([widths[1], row_h - 4.0], egui::Button::new(glyph).small()).clicked() {
                                    acts.push(Act::Play(w));
                                }
                                let col = if is_playing { theme::BRASS } else { theme::TX };
                                let h = row_h - 4.0;
                                cell(ui, widths[2], h, |ui| {
                                    ui.label(egui::RichText::new(format!("{w:05}")).monospace().color(theme::DIM));
                                });
                                cell(ui, widths[3], h, |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(&c.speaker).color(theme::DIM)).truncate());
                                });
                                if search {
                                    cell(ui, bank_w, h, |ui| {
                                        ui.add(egui::Label::new(egui::RichText::new(p.banks_of(w)).color(theme::DIM)).truncate());
                                    });
                                }
                                let caption = if c.caption.is_empty() { c.stem().to_string() } else { c.caption.clone() };
                                let cap_w = (ui.available_width() - len_w - ui.spacing().item_spacing.x).max(40.0);
                                cell(ui, cap_w, h, |ui| {
                                    ui.add(egui::Label::new(egui::RichText::new(caption).color(col)).truncate())
                                        .on_hover_text(&c.caption);
                                });
                                cell(ui, len_w, h, |ui| {
                                    ui.label(egui::RichText::new(secs(c.duration_s)).monospace().color(theme::DIM));
                                });
                        }
                        if row.clicked() {
                            acts.push(Act::Current(w));
                        }
                    }
                },
            );
            // Now playing.
            if let Some(s) = now {
                if let Some(c) = p.clip(s.clip) {
                    ui.add_space(6.0);
                    theme::card(ui, &format!("Clip {:05} · {}", s.clip, if s.paused { "paused" } else { "playing" }), None, |ui| {
                        ui.label(egui::RichText::new(format!("\u{201C}{}\u{201D}", c.caption)).color(theme::TX));
                        ui.horizontal(|ui| {
                            let frac = if s.length_s > 0.0 { (s.position_s / s.length_s).clamp(0.0, 1.0) } else { 0.0 };
                            ui.add(egui::ProgressBar::new(frac).desired_width(ui.available_width() - 90.0));
                            ui.label(egui::RichText::new(format!("{:.1} / {:.1} s", s.position_s, s.length_s)).monospace().color(theme::DIM));
                        });
                    });
                }
            }
        });
    acts
}

/// The right column: the current clip, and the export.
pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    match p.current.and_then(|w| p.clip(w).map(|c| (w, c))) {
        None => {
            ui.weak("No clip chosen — pick a line in the table.");
            ui.add_space(10.0);
        }
        Some((w, c)) => {
            theme::card(ui, "Clip", Some(&format!("{w:05}")), |ui| {
                theme::kv(ui, "speaker", egui::RichText::new(&c.speaker));
                theme::kv(ui, "bank", egui::RichText::new(p.banks_of(w)));
                // The shipped file name is long: its own line, under its label.
                ui.label(egui::RichText::new("source").color(theme::DIM));
                ui.add(
                    egui::Label::new(egui::RichText::new(c.original.rsplit('/').next().unwrap_or(&c.original)).monospace())
                        .wrap(),
                );
                theme::kv(ui, "stream", egui::RichText::new("vo_stream.english.pws"));
                theme::kv(ui, "length", egui::RichText::new(secs(c.duration_s)));
                if let Some(r) = p.routes() {
                    ui.add_space(4.0);
                    theme::eyebrow(ui, "Cues that play it");
                    match r.plays.get(&w) {
                        None => {
                            ui.label(egui::RichText::new("none").color(theme::FAINT));
                        }
                        Some(ps) => {
                            for play in ps {
                                let name = p.cue_name(play).map(str::to_string).unwrap_or_else(|| format!("0x{:08X}", play.guid));
                                let form = if play.multi_track { "multi-track" } else { "single-track" };
                                ui.label(
                                    egui::RichText::new(format!("{name} · {} · {form} · {} wave(s)", r.bank_name(play.entry), play.waves))
                                        .small()
                                        .color(theme::DIM),
                                );
                            }
                        }
                    }
                }
            });
        }
    }
    let scope_n = |s: Scope| p.scope_lines(s).len();
    theme::card(ui, "Export", None, |ui| {
        let mut scopes = vec![(Scope::Selected, format!("{} selected", scope_n(Scope::Selected)))];
        if p.open.is_some() {
            scopes.push((Scope::Bank, format!("bank · {}", scope_n(Scope::Bank))));
        }
        if p.searching() {
            scopes.push((Scope::Hits, format!("search hits · {}", scope_n(Scope::Hits))));
        }
        ui.horizontal_wrapped(|ui| {
            for (s, label) in scopes {
                if theme::pill(ui, &label, p.scope == s).clicked() {
                    acts.push(Act::Scope(s));
                }
            }
        });
        theme::kv(ui, "format", egui::RichText::new("WAV, 16-bit PCM"));
        theme::kv(ui, "names", egui::RichText::new("as shipped"));
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("into").color(theme::DIM));
            ui.label(egui::RichText::new(p.folder.display().to_string()).monospace());
            if ui.button("Choose folder…").clicked() {
                acts.push(Act::PickFolder);
            }
        });
        ui.label(egui::RichText::new("captions.csv beside the clips").small().color(theme::FAINT));
        if let Some((d, t)) = p.progress {
            ui.add(egui::ProgressBar::new(d as f32 / t.max(1) as f32).text(format!("{d} / {t}")));
        }
        match &p.note {
            Some(Ok(m)) => {
                ui.label(egui::RichText::new(m).small().color(theme::GOOD));
            }
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).small().color(theme::BAD));
            }
            None => {}
        }
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Headless: --audio-export <bank|query> <dir>").small().color(theme::FAINT));
    });
    acts
}

/// The verb bar: Play, Select bank (or hits), Add to Shipment, Export.
pub fn verbs(ui: &mut egui::Ui, p: &Panel, playing: Option<audioplay::Status>) -> Vec<Act> {
    let mut acts = Vec::new();
    let cur = p.current;
    let playing_cur = playing.filter(|s| Some(s.clip) == cur);
    let play_label = match playing_cur {
        Some(s) if !s.paused => "Pause  Space",
        Some(_) => "Resume  Space",
        None => "Play  Space",
    };
    if ui.add_enabled(cur.is_some(), egui::Button::new(play_label)).clicked() {
        acts.push(Act::PlayCurrent);
    }
    let n = p.rows().len();
    let select = if p.searching() { "Select hits  Ctrl A" } else { "Select bank  Ctrl A" };
    if ui.add_enabled(n > 0, egui::Button::new(select)).clicked() {
        acts.push(Act::SelectAll);
    }
    ui.separator();
    ui.add_enabled_ui(cur.is_some() && p.routes().is_some(), |ui| {
        ui.menu_button("Add to Shipment", |ui| {
            let (Some(w), Some(r)) = (cur, p.routes()) else { return };
            let refusal = p.line_refusal(w);
            let line = ui.add_enabled(refusal.is_none(), egui::Button::new("Replace this line"));
            let line = match refusal {
                Some(why) => line.on_disabled_hover_text(why),
                None => line.on_hover_text("replace_sound_wave: every group that plays it plays the new wave"),
            };
            if line.clicked() {
                acts.push(Act::AddLine(w));
                ui.close_menu();
            }
            let mut banks = BTreeSet::new();
            for (i, play) in r.plays.get(&w).into_iter().flatten().enumerate() {
                let name = p.cue_name(play).map(str::to_string).unwrap_or_else(|| format!("0x{:08X}", play.guid));
                if ui.button(format!("Replace cue {name} (plays {} wave(s))", play.waves)).clicked() {
                    acts.push(Act::AddCue(w, i));
                    ui.close_menu();
                }
                banks.insert(play.entry);
            }
            for entry in banks {
                if ui.button(format!("Replace its bank {}", r.bank_name(entry))).clicked() {
                    acts.push(Act::AddBank(entry));
                    ui.close_menu();
                }
            }
        });
    });
    let k = p.scope_lines(p.scope).len();
    if ui.add_enabled(k > 0 && p.export.is_none(), egui::Button::new(format!("Export {k} clips  F10"))).clicked() {
        acts.push(Act::Export);
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_audio::wave::{WaveData, WaveRecord, WavebankFile};

    /// A panel over three clips of a two-bank routing, without English.wad.
    fn panel() -> Panel {
        let mut p = Panel::default();
        let clip = |w: u32, speaker: &str, caption: &str, context: &str| AudioClip {
            original: format!("vo_misc/{w:05}__{context}.wav"),
            bank: "vo_misc".into(),
            speaker: speaker.into(),
            caption: caption.into(),
            duration_s: 1.0 + w as f32,
            context: context.into(),
            wave_index: Some(w),
        };
        p.set_index(AudioIndex::from_clips(vec![
            clip(0, "Chris", "Now the CIA.", "c0"),
            clip(1, "Chris", "Rat bastard.", ""),
            clip(2, "Jen", "The CIA again.", "j2"),
        ]));
        let wb = WavebankFile {
            bank_hash: mercs2_formats::hash::pandemic_hash_m2("vo_stream"),
            stream_name: Some("vo_stream.english.pws".into()),
            records: (0..4)
                .map(|i| WaveRecord {
                    clip_hash: 0x100 + i,
                    channels: 1,
                    format: 4,
                    sample_rate: 22050,
                    frames: 0,
                    data: WaveData::Streamed { offset: 0, size: 0, word_1c: 0 },
                })
                .collect(),
        };
        let mut r = Routes::from_tables(wb, Vec::new(), &[]);
        r.banks = vec![
            crate::audioroute::Bank { name: "vo_Chris".into(), entry: 1, lines: vec![0, 1] },
            crate::audioroute::Bank { name: "vo_Jen".into(), entry: 2, lines: vec![2] },
        ];
        r.unplayed = vec![3];
        for (w, e) in [(0u32, 1u32), (1, 1), (2, 2)] {
            r.plays.insert(w, vec![Play { entry: e, cue: 0, guid: 7, group: 0, multi_track: false, waves: 1 }]);
        }
        p.set_routes(r);
        p
    }

    /// The open bank lists its lines, the filter narrows them, a search lists hits across banks (no
    /// cap) and the crumb names what is shown; opening a bank clears the search and the filter.
    #[test]
    fn banks_filter_and_search_list_the_rows() {
        let mut p = panel();
        assert_eq!(p.open, Some(Open::Bank(0)), "the first bank opens when the routing lands");
        assert_eq!(p.rows(), vec![0, 1]);
        assert_eq!(crumb(&p), ["Audio", "vo_Chris"]);
        apply(Act::Filter("rat".into()), &mut p);
        assert_eq!(p.rows(), vec![1]);
        apply(Act::Search("cia".into()), &mut p);
        assert_eq!(p.rows(), vec![0, 2], "hits across banks");
        assert_eq!(crumb(&p), ["Audio", "\"cia\""]);
        assert_eq!(p.banks_of(2), "vo_Jen");
        apply(Act::Open(Open::Unplayed), &mut p);
        assert!(p.search.is_empty() && p.filter.is_empty());
        assert_eq!(p.open_lines(), vec![3]);
        assert_eq!(crumb(&p), ["Audio", "not played by any cue"]);
    }

    /// The checkbox toggles; Select all takes every listed row; Clear empties; the scopes count the
    /// selection, the bank and the hits; keys map to play, select all and export.
    #[test]
    fn selection_scopes_and_keys() {
        let mut p = panel();
        apply(Act::Toggle(1), &mut p);
        apply(Act::Toggle(0), &mut p);
        apply(Act::Toggle(1), &mut p);
        assert_eq!(p.selected, BTreeSet::from([0]));
        apply(Act::SelectAll, &mut p);
        assert_eq!(p.selected, BTreeSet::from([0, 1]));
        assert_eq!(p.scope_lines(Scope::Bank), vec![0, 1]);
        assert!(p.scope_lines(Scope::Hits).is_empty(), "no hits without a search");
        apply(Act::Search("cia".into()), &mut p);
        assert_eq!(p.scope_lines(Scope::Hits), vec![0, 2]);
        apply(Act::Clear, &mut p);
        assert!(p.selected.is_empty());
        assert_eq!(shortcut(KeyCode::Space, false), Some(Act::PlayCurrent));
        assert_eq!(shortcut(KeyCode::KeyA, true), Some(Act::SelectAll));
        assert_eq!(shortcut(KeyCode::KeyA, false), None, "a bare A is not a shortcut");
        assert_eq!(shortcut(KeyCode::F10, false), Some(Act::Export));
        apply(Act::Current(2), &mut p);
        apply(Act::PlayCurrent, &mut p);
        assert_eq!(p.play, Some(2));
    }

    /// What clicking at `pos` in the centre asks for: a frame to lay out, one to move the pointer
    /// there, one to press and one to release, each drawn as the app draws it.
    fn click_at(p: &Panel, pos: egui::Pos2) -> Vec<Act> {
        let ctx = egui::Context::default();
        theme::install(&ctx);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::default(),
        };
        let mut acts = Vec::new();
        for events in [vec![], vec![egui::Event::PointerMoved(pos)], vec![button(true)], vec![button(false)]] {
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| acts.extend(center(ctx, p, None)));
        }
        acts
    }

    /// Clicking in a row: its ▶ plays that line, its checkbox toggles it, and anywhere else on it
    /// (the caption) makes it the current line — each widget gets its own click, the row the rest.
    #[test]
    fn a_rows_play_button_and_checkbox_take_their_own_clicks() {
        let p = panel();
        // Row 0's band: every y where a caption click makes it current. egui also gives a click
        // just outside every widget to the closest one, so probe the band's middle, not its edge.
        let band: Vec<f32> = (0..800)
            .step_by(2)
            .map(|y| y as f32)
            .filter(|y| click_at(&p, egui::pos2(700.0, *y)).contains(&Act::Current(0)))
            .collect();
        let y = (band.first().expect("a click on the first row's caption makes it current") + band.last().unwrap()) / 2.0;
        let mut seen = Vec::new();
        for x in (0..700).step_by(3) {
            seen.extend(click_at(&p, egui::pos2(x as f32, y)));
        }
        assert!(seen.contains(&Act::Play(0)), "no click on row 0 plays it: {seen:?}");
        assert!(seen.contains(&Act::Toggle(0)), "no click on row 0 toggles it: {seen:?}");
        let caption = click_at(&p, egui::pos2(700.0, y));
        assert!(caption.contains(&Act::Current(0)) && !caption.contains(&Act::Play(0)), "{caption:?}");
    }

    /// Replace this line asks for a replace_sound_wave by the line's name; a line no cue plays is
    /// refused with the reason and asks for nothing; export items carry the shipped file name and the
    /// banks that play the clip.
    #[test]
    fn shipment_routes_and_export_items() {
        let mut p = panel();
        apply(Act::AddLine(0), &mut p);
        let Some(Contribution::ReplaceSoundWave { line, .. }) = p.shipment.take() else { panic!("a line") };
        assert_eq!(line, "0x00000100", "context c0 does not hash to the clip hash, so the hash");
        assert!(p.line_refusal(3).is_some());
        apply(Act::AddLine(3), &mut p);
        assert!(p.shipment.is_none());
        apply(Act::AddKind("add_sound"), &mut p);
        assert!(matches!(p.shipment.take(), Some(Contribution::AddSound { .. })));
        let it = items(&p, &[2]);
        assert_eq!((it[0].file.as_str(), it[0].bank.as_str(), it[0].caption.as_str()), ("00002__j2.wav", "vo_Jen", "The CIA again."));
    }
}
