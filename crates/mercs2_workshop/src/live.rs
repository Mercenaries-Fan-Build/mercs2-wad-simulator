//! The Live page: a read-only mirror of the running game.
//!
//! `docs/modernization/workshop_pages/live.md` is this page's contract: every element, what it does and
//! where its data comes from. [`crate::live_link`] owns the bridge, [`crate::live_log`] the log tail,
//! [`crate::live_cam`] the camera. Nothing here writes to the game; the camera and the overlays are local.

use crate::gui::theme;
use crate::live_cam::{self, Camera, Follow, Mode};
use crate::live_link::{Entity, EntityClass, Link, LinkState, Live, Pose};
use crate::live_log::{LogState, LogTail};
use crate::live_world::Mirror;
use egui::{Color32, RichText};
use glam::Vec3;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

/// Hash to name: the worldentity's template names over the workshop's name table.
#[derive(Clone)]
pub struct Names {
    templates: Rc<HashMap<u32, String>>,
}

impl Names {
    /// `h` by name, else its hash: never a blank.
    fn of(&self, index: &HashMap<u32, String>, h: u32) -> String {
        self.templates.get(&h).or_else(|| index.get(&h)).cloned().unwrap_or_else(|| format!("0x{h:08X}"))
    }
}

/// What a lock can be on: the hero, or an entity by its guid text.
#[derive(Debug, Clone, PartialEq)]
pub enum Lock {
    None,
    Hero,
    Entity(String),
}

/// The overlay switches. The view pills with a matching overlay share its flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Overlays {
    pub terrain: bool,
    pub collision: bool,
    pub paths: bool,
    pub triggers: bool,
    pub spawns: bool,
    pub audio: bool,
    pub ai: bool,
}

impl Default for Overlays {
    fn default() -> Overlays {
        Overlays { terrain: true, collision: false, paths: false, triggers: false, spawns: false, audio: false, ai: false }
    }
}

/// What the page asks of the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Start a Shipment from this entity's model.
    OpenInShipment(String, crate::quartermaster::Subject),
}

/// The page's state.
pub struct Page {
    pub link: Link,
    pub log: LogTail,
    pub mirror: Mirror,
    /// The data overlays' worker, started the first time one is on.
    layers: Option<crate::live_overlay::Overlays>,
    /// The overlay's assets have been handed to the link.
    assets_asked: bool,
    overlay_files: Vec<String>,
    /// The map image, uploaded.
    map: Option<crate::live_map::Map>,
    pub cam: Camera,
    pub overlays: Overlays,
    pub lock: Lock,
    /// The inspected entity by guid text; `None` = the hero.
    pub selection: Option<String>,
    /// The last values of a selection the game stopped reporting.
    gone: Option<Entity>,
    pub left_width: f32,
    pub right_width: f32,
    /// Frame time, smoothed for the readouts.
    frame_s: f32,
    last_frame: Instant,
    /// Where scratch files go.
    scratch: PathBuf,
    /// The last thing a button did, or why it could not.
    note: Option<String>,
    /// This frame's camera and how it projects.
    pub frame: Option<FrameView>,
    /// The log and the bridge address are per install.
    install: Option<PathBuf>,
    open: bool,
    /// The marker under the pointer.
    picked: Option<String>,
}

/// One frame's projection, for the markers and for picking.
#[derive(Debug, Clone, Copy)]
pub struct FrameView {
    pub view: live_cam::View,
    pub vp: glam::Mat4,
    pub screen: egui::Vec2,
    pub rect: egui::Rect,
    pub shift: glam::Vec2,
}

/// Every `(name, type)` the overlays above the base carry: what mods added or replaced. Their ASET rows
/// are the only per-asset record of what is deployed; `placement.json` lists placed files.
fn overlay_assets(w: &crate::app::WadStack) -> Vec<(u32, u32)> {
    let mut assets: Vec<(u32, u32)> = w
        .wads
        .iter()
        .skip(1)
        .flat_map(|o| mercs2_engine::wad::aset_entries(o))
        .filter_map(|e| Some((e.asset_hash, mercs2_formats::types::type_hash_for_type_id(e.type_id)?)))
        .collect();
    assets.sort_unstable();
    assets.dedup();
    assets
}

/// Where the game keeps `pmc_blackbox.log` and `scripts/`: the folder above `data/vz.wad`.
pub fn install_of(wad: &Path) -> Option<PathBuf> {
    wad.parent()?.parent().map(Path::to_path_buf)
}

impl Page {
    pub fn new(scratch: PathBuf) -> Page {
        Page {
            link: Link::spawn(crate::live_link::bridge_addr(None)),
            log: LogTail::spawn(),
            mirror: Mirror::default(),
            layers: None,
            assets_asked: false,
            overlay_files: Vec::new(),
            map: None,
            cam: Camera::default(),
            overlays: Overlays::default(),
            lock: Lock::Hero,
            selection: None,
            gone: None,
            left_width: 248.0,
            right_width: 326.0,
            frame_s: 1.0 / 60.0,
            last_frame: Instant::now(),
            scratch,
            note: None,
            frame: None,
            install: None,
            open: false,
            picked: None,
        }
    }

    /// The open archive decides which game this is: its log, and where its bridge listens.
    pub fn set_wad(&mut self, wad: &str) {
        let install = install_of(Path::new(wad));
        if install != self.install {
            self.install = install;
            self.link.set_addr(crate::live_link::bridge_addr(self.install.as_deref()));
            self.set_open(self.open);
        }
    }

    /// The page opened or closed: only an open page polls the game.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
        self.link.set_open(open);
        self.log.set(open, self.install.as_ref().map(|d| d.join("pmc_blackbox.log")));
    }

    /// One frame of the mirror. The world is loaded once the game has one to mirror.
    pub fn step_mirror(&mut self, scene: &mut mercs2_engine::scene::Scene, w: &mut crate::app::WadStack, wad: &str) {
        let live = self.link.snapshot();
        if !live.poses.is_empty() {
            self.mirror.start(wad);
        }
        if !self.assets_asked {
            self.assets_asked = true;
            self.link.set_assets(overlay_assets(w));
            self.overlay_files = w.labels.iter().skip(1).filter_map(|l| Path::new(l).file_name()).map(|f| f.to_string_lossy().into_owned()).collect();
        }
        let hero = live.poses.last().map(|(_, p)| Vec3::from(p.pos));
        self.mirror.step(scene, w, &live, hero);
        self.mirror.show_terrain(scene, self.overlays.terrain);
        self.step_overlays(&live, w);
    }

    /// Hand the data overlays the layers the game has, once one of them is on.
    fn step_overlays(&mut self, live: &Live, w: &crate::app::WadStack) {
        let o = self.overlays;
        if !(o.paths || o.triggers || o.spawns || o.audio) {
            return;
        }
        let Some(world) = &live.world else { return };
        let overlays = self.layers.get_or_insert_with(|| {
            let mut paths = w.labels.iter();
            let base = paths.next().cloned().unwrap_or_default();
            crate::live_overlay::Overlays::spawn(base, paths.cloned().collect())
        });
        overlays.set_layers(world.layers.iter().chain(&live.statics).cloned().collect());
    }

    fn names(&self) -> Names {
        Names { templates: self.mirror.names.clone() }
    }

    fn tick(&mut self) -> f32 {
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        self.frame_s += (dt - self.frame_s) * 0.1;
        dt
    }

    pub fn drag(&mut self, dx: f32, dy: f32, shift: bool, from: Vec3) {
        if shift {
            self.cam.pan(from, dx, dy);
            self.lock = Lock::None;
        } else {
            self.cam.drag(dx, dy);
        }
    }

    pub fn scratch_file(&self, stem: &str, ext: &str) -> PathBuf {
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        self.scratch.join(format!("{stem}-{at}.{ext}"))
    }
}

/// Everything the floating parts draw from, gathered once a frame.
struct Now<'a> {
    live: Live,
    pose: Option<Pose>,
    names: Names,
    index: &'a HashMap<u32, String>,
}

/// The pose the view draws at: one sample interval behind the newest, so ten samples a second move
/// smoothly. The readouts use the newest sample, unsmoothed.
pub fn smooth(poses: &[(Instant, Pose)], now: Instant, of: impl Fn(&Pose) -> Follow) -> Option<Follow> {
    match poses {
        [] => None,
        [(_, p)] => Some(of(p)),
        [.., (ta, a), (tb, b)] => {
            let span = tb.duration_since(*ta).as_secs_f32().max(1e-3);
            Some(live_cam::lerp_follow(of(a), of(b), now.duration_since(*tb).as_secs_f32() / span))
        }
    }
}

/// What the camera follows of the hero: the vehicle it rides, else itself.
pub fn hero_follow(p: &Pose) -> Follow {
    match &p.vehicle {
        Some(v) => Follow { pos: Vec3::from(v.pos), yaw: v.yaw },
        None => Follow { pos: Vec3::from(p.pos), yaw: p.yaw },
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    Vec3::from(a).distance(Vec3::from(b))
}

fn link_text(live: &Live) -> (String, Color32) {
    match &live.state {
        LinkState::Off => (format!("not connected · {}", live.addr), theme::live::FAINT),
        LinkState::NoWorld { name, .. } => (format!("no world · {name}"), theme::live::AMBER),
        LinkState::NoHero => ("world loading".to_string(), theme::live::AMBER),
        LinkState::Streaming => {
            let ms = live.rtt.map_or(0, |d| d.as_millis());
            (format!("streaming · {ms} ms lag"), theme::live::GREEN)
        }
        LinkState::NotAnswering => ("not answering".to_string(), theme::live::AMBER),
        LinkState::Failed(_) => ("replay failed".to_string(), theme::live::RED),
        LinkState::Replay { path, finished } => {
            let name = path.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            (format!("replay · {name}{}", if *finished { " · ended" } else { "" }), theme::live::BLUE)
        }
    }
}

fn pill(ui: &mut egui::Ui, text: String, color: Color32, dot: bool) -> egui::Response {
    egui::Frame::none()
        .stroke(egui::Stroke::new(1.0, color.gamma_multiply(0.4)))
        .fill(color.gamma_multiply(0.08))
        .rounding(egui::Rounding::same(999.0))
        .inner_margin(egui::Margin::symmetric(9.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if dot {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 3.5, color);
                }
                ui.label(RichText::new(text).small().strong().color(color));
            });
        })
        .response
}

fn tool_button(ui: &mut egui::Ui, text: &str, on: bool) -> bool {
    let fill = if on { theme::live::RAISED } else { Color32::TRANSPARENT };
    let fg = if on { theme::live::TX } else { theme::live::DIM };
    ui.add(egui::Button::new(RichText::new(text).small().color(fg)).fill(fill).stroke(egui::Stroke::new(1.0, theme::live::LINE))).clicked()
}

/// The header strip: link, read-only promise, session, camera mode, Snapshot and Record.
pub fn header(ctx: &egui::Context, p: &mut Page) {
    let live = p.link.snapshot();
    egui::TopBottomPanel::top("live_header")
        .frame(egui::Frame::none().fill(theme::live::HEAD).inner_margin(egui::Margin::symmetric(14.0, 6.0)))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::disp_text("LIVE WORLD", theme::style_title(), theme::live::TX));
                ui.separator();
                let (text, color) = link_text(&live);
                pill(ui, text, color, true);
                pill(ui, "read-only free-cam".to_string(), theme::live::BLUE, false)
                    .on_hover_text("Live only reads the game. It cannot spawn, target, load, or write anything.");
                if let Some(w) = &live.world {
                    let hero = if w.hero.is_empty() { "no hero" } else { &w.hero };
                    let contract = if w.last_contract.is_empty() { String::new() } else { format!(" · {}", w.last_contract) };
                    pill(ui, format!("{} · {hero} · {}{contract}", w.host, w.level), theme::live::DIM, false)
                        .on_hover_text("Host, hero, level, last completed contract");
                }
                ui.add_space(4.0);
                for m in Mode::ALL {
                    if tool_button(ui, m.label(), p.cam.mode == m) {
                        p.cam.mode = m;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let recording = live.recording.is_some();
                    if tool_button(ui, if recording { "Stop recording" } else { "Record" }, recording) {
                        if recording {
                            p.link.stop_recording();
                        } else {
                            let file = p.scratch_file("live", "replay");
                            p.note = Some(format!("recording to {}", file.display()));
                            p.link.record_to(file);
                        }
                    }
                    if tool_button(ui, "Snapshot", false) {
                        let file = p.scratch_file("live-snapshot", "json");
                        p.note = Some(match write_snapshot(&live, &p.cam, p.overlays, &file) {
                            Ok(()) => format!("snapshot {}", file.display()),
                            Err(e) => format!("SNAPSHOT FAILED: {e}"),
                        });
                    }
                    let replaying = matches!(live.state, LinkState::Replay { .. } | LinkState::Failed(_));
                    if replaying {
                        if tool_button(ui, "Leave replay", true) {
                            p.link.leave_replay();
                        }
                    } else if tool_button(ui, "Open replay…", false) {
                        if let Some(path) = rfd::FileDialog::new().add_filter("Live recording", &["replay"]).pick_file() {
                            p.link.replay(path);
                        }
                    }
                });
            });
        });
}

/// A snapshot's file: what the page knew at that moment.
#[derive(serde::Serialize)]
struct Snapshot<'a> {
    addr: &'a str,
    pose: Option<&'a Pose>,
    entities: &'a [Entity],
    world: Option<&'a crate::live_link::WorldState>,
    mission: &'a crate::live_link::Mission,
    camera_mode: &'a str,
    camera_yaw_deg: f32,
    camera_zoom: f32,
    overlays: Vec<&'a str>,
}

fn write_snapshot(live: &Live, cam: &Camera, o: Overlays, path: &Path) -> Result<(), String> {
    let mut on = Vec::new();
    for (name, flag) in overlay_rows(o) {
        if flag {
            on.push(name);
        }
    }
    let s = Snapshot {
        addr: &live.addr,
        pose: live.poses.last().map(|(_, p)| p),
        entities: &live.entities,
        world: live.world.as_ref(),
        mission: &live.mission,
        camera_mode: cam.mode.label(),
        camera_yaw_deg: cam.readout_yaw(),
        camera_zoom: cam.zoom,
        overlays: on,
    };
    let json = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))
}

fn overlay_rows(o: Overlays) -> [(&'static str, bool); 6] {
    [
        ("Terrain mesh", o.terrain),
        ("Collision (MOPP)", o.collision),
        ("Patrol paths", o.paths),
        ("Trigger volumes", o.triggers),
        ("Spawn radii", o.spawns),
        ("Audio emitters", o.audio),
    ]
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(theme::disp_text(text, theme::style_eyebrow(), theme::live::FAINT));
    ui.add_space(4.0);
}

fn row(ui: &mut egui::Ui, dot: Option<Color32>, key: &str, value: String, color: Color32) {
    ui.horizontal(|ui| {
        if let Some(c) = dot {
            let (r, _) = ui.allocate_exact_size(egui::vec2(7.0, 7.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 3.5, c);
        }
        ui.label(RichText::new(key).small().color(theme::live::DIM));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).small().monospace().color(color));
        });
    });
}

/// The left panel: connection, the mirror against retail, the overlays.
pub fn left(ctx: &egui::Context, p: &mut Page) {
    let live = p.link.snapshot();
    let log = p.log.snapshot();
    let fps = 1.0 / p.frame_s.max(1e-4);
    let drawn = p.mirror.drawn(&live);
    let mut problems: Vec<String> = p.mirror.problems().map(|(m, e)| format!("model 0x{m:08X}: {e}")).collect();
    problems.extend(p.layers.iter().flat_map(|l| l.errors()));
    let mut width = p.left_width;
    crate::gui::held_side_panel(
        ctx,
        egui::panel::Side::Left,
        "live_mirror",
        egui::Rangef::new(220.0, 360.0),
        &mut width,
        egui::Frame::none().fill(theme::live::PANEL).inner_margin(egui::Margin::same(13.0)),
        |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                section_title(ui, "CONNECTION");
                let port = live.addr.rsplit(':').next().unwrap_or_default();
                let bridge = match &live.state {
                    LinkState::Off => theme::live::FAINT,
                    LinkState::Streaming => theme::live::GREEN,
                    _ => theme::live::AMBER,
                };
                row(ui, Some(bridge), "lua_bridge", format!(":{port}"), theme::live::DIM);
                let (c, t) = match log.state {
                    LogState::Missing => (theme::live::FAINT, "no log"),
                    LogState::Stale => (theme::live::AMBER, "stale"),
                    LogState::Growing => (theme::live::GREEN, "log tail"),
                };
                ui.scope(|ui| row(ui, Some(c), "pmc_bb", t.to_string(), theme::live::DIM))
                    .response
                    .on_hover_text("The log holds the game's Lua lines only when the game is started with PMC_VERBOSE_LOG=1");
                let (c, t) = match (log.debugger, &live.state) {
                    (false, _) => (theme::live::FAINT, "not running"),
                    (true, LinkState::NotAnswering) => (theme::live::AMBER, "stalled"),
                    (true, _) => (theme::live::GREEN, "running"),
                };
                ui.scope(|ui| row(ui, Some(c), "x32dbg", t.to_string(), theme::live::DIM))
                    .response
                    .on_hover_text("From the process table. \"stalled\" means it is running and the game stopped answering, which is inferred");
                let mirror = if p.mirror.ready() { theme::live::GREEN } else { theme::live::AMBER };
                row(ui, Some(mirror), "wgpu mirror", format!("{fps:.0} fps"), theme::live::DIM);
                ui.add_space(12.0);

                section_title(ui, "OUR RENDER VS RETAIL");
                let (hero, c) = match (live.poses.is_empty(), drawn.hero) {
                    (true, _) => ("—".to_string(), theme::live::DIM),
                    (false, true) => ("match".to_string(), theme::live::GREEN),
                    (false, false) => ("not drawn".to_string(), theme::live::AMBER),
                };
                row(ui, None, "Hero pose", hero, c);
                let count = |(n, m): (usize, usize)| (format!("{n} / {m}"), if n == m { theme::live::GREEN } else { theme::live::AMBER });
                let (t, c) = count(drawn.vehicles);
                row(ui, None, "Vehicle poses", t, c);
                let (t, c) = count(drawn.npcs);
                row(ui, None, "NPC poses", t, c);
                if let Some(w) = &live.world {
                    let (t, c) = match w.residency {
                        Some((records, blocks)) => (format!("{records} · {blocks} blocks"), theme::live::GREEN),
                        None => ("needs M2Live".to_string(), theme::live::FAINT),
                    };
                    ui.scope(|ui| row(ui, None, "Resident chunks", t, c))
                        .response
                        .on_hover_text("Records in the game's asset registry, and resident blocks: M2Live, in m2-sdk");
                    row(ui, None, "Active layers", (w.layers.len() + live.statics.len()).to_string(), theme::live::GREEN);
                    if let Some((waking, assets)) = p.mirror.loading() {
                        let busy = waking + assets > 0;
                        row(ui, None, "Mirror loading", format!("{waking} · {assets} assets"), if busy { theme::live::AMBER } else { theme::live::DIM });
                    }
                    if !live.hidden_piles.is_empty() {
                        let (shown, of) = (p.mirror.hidden_count(), live.hidden_piles.len());
                        row(ui, None, "Piles hidden", format!("{shown} / {of}"), if shown == of { theme::live::GREEN } else { theme::live::AMBER });
                    }
                    let seq = if w.blocking == 0 { "0 (idle)".to_string() } else { w.blocking.to_string() };
                    row(ui, None, "Blocking seq", seq, if w.blocking == 0 { theme::live::DIM } else { theme::live::AMBER });
                    if w.streaming {
                        row(ui, None, "Streaming", "loading".to_string(), theme::live::AMBER);
                    }
                }
                ui.add_space(12.0);

                section_title(ui, "OVERLAYS");
                let o = &mut p.overlays;
                ui.checkbox(&mut o.terrain, RichText::new("Terrain mesh").small());
                ui.checkbox(&mut o.collision, RichText::new("Collision (MOPP)").small());
                ui.checkbox(&mut o.paths, RichText::new("Patrol paths").small());
                ui.checkbox(&mut o.triggers, RichText::new("Trigger volumes").small());
                ui.checkbox(&mut o.spawns, RichText::new("Spawn radii").small());
                ui.checkbox(&mut o.audio, RichText::new("Audio emitters").small());

                ui.add_space(12.0);
                section_title(ui, "SHIPMENT RESIDENCY");
                let file = p.overlay_files.join(", ");
                match live.assets {
                    Some(t) if t.total > 0 => {
                        let color = if t.evicted > 0 { theme::live::AMBER } else { theme::live::GREEN };
                        row(ui, Some(color), &file, "live".to_string(), theme::live::DIM);
                        let text = format!("{} resident · {} registered · {} evicted · {} assets", t.resident, t.registered, t.evicted, t.total);
                        ui.label(RichText::new(text).small().monospace().color(theme::live::FAINT));
                    }
                    Some(_) => {
                        ui.label(RichText::new("no overlay in this archive").small().color(theme::live::FAINT));
                    }
                    None => {
                        ui.label(RichText::new("needs M2Live, in m2-sdk").small().color(theme::live::FAINT));
                    }
                }

                if !live.faults.is_empty() || !problems.is_empty() {
                    ui.add_space(12.0);
                    section_title(ui, "FAULTS");
                    for (of, why) in &live.faults {
                        ui.label(RichText::new(format!("{of}: {why}")).small().color(theme::live::RED));
                    }
                    for m in &problems {
                        ui.label(RichText::new(m).small().color(theme::live::RED));
                    }
                }
                if let LinkState::Failed(e) = &live.state {
                    ui.add_space(12.0);
                    ui.label(RichText::new(e).small().color(theme::live::RED));
                }
            });
        },
    );
    p.left_width = width;
}

/// A row of the nearby list, sorted by distance.
fn nearby(live: &Live, hero: [f32; 3]) -> Vec<(f32, &Entity)> {
    let mut v: Vec<(f32, &Entity)> =
        live.entities.iter().filter(|e| e.class != EntityClass::Hero).map(|e| (distance(e.pos, hero), e)).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

fn swatch(e: &Entity) -> Color32 {
    if e.class.is_vehicle() {
        theme::live::AMBER
    } else {
        theme::live::BLUE
    }
}

/// The right panel: the inspected entity, what is near, the log.
pub fn right(ctx: &egui::Context, p: &mut Page, index: &HashMap<u32, String>) -> Vec<Act> {
    let mut acts = Vec::new();
    let live = p.link.snapshot();
    let log = p.log.snapshot();
    let names = p.names();
    let name = |h: u32| names.of(index, h);
    let hero_row = live.entities.iter().find(|e| e.class == EntityClass::Hero);
    let selected = p.selection.as_ref().and_then(|g| live.entities.iter().find(|e| e.guid == *g)).or(if p.selection.is_none() { hero_row } else { None });
    if let (Some(s), Some(_)) = (selected, &p.selection) {
        p.gone = Some(s.clone());
    }
    let is_gone = p.selection.is_some() && selected.is_none();
    let shown = selected.or(if is_gone { p.gone.as_ref() } else { None });
    let mut width = p.right_width;
    crate::gui::held_side_panel(
        ctx,
        egui::panel::Side::Right,
        "live_inspector",
        egui::Rangef::new(300.0, 480.0),
        &mut width,
        egui::Frame::none().fill(theme::live::PANEL).inner_margin(egui::Margin::same(13.0)),
        |ui| {
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                // The foot first, so it keeps its place however tall the log is.
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let there = shown.is_some();
                    if ui.add_enabled(there, egui::Button::new("Jump cam to")).on_hover_text("Point the free-cam at this entity (local only)").clicked() {
                        if let Some(e) = shown {
                            p.cam.mode = Mode::FreeOrbit;
                            p.cam.pan_target = Vec3::from(e.pos);
                            p.lock = Lock::None;
                        }
                    }
                    let wanted = selection_lock(p.selection.as_deref());
                    let on = p.lock == wanted;
                    if ui.add_enabled(there, egui::SelectableLabel::new(on, "Lock-on")).on_hover_text("Keep this entity centred as it moves (local only)").clicked() {
                        p.lock = if on { Lock::None } else { wanted };
                        p.cam.mode = Mode::FreeOrbit;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(there, egui::Button::new("Open in Shipment")).on_hover_text("Open this entity's model in a new Shipment").clicked() {
                            if let Some(e) = shown {
                                let character = matches!(e.class, EntityClass::Hero | EntityClass::Human);
                                acts.push(Act::OpenInShipment(name(e.model), crate::quartermaster::Subject::Model { character }));
                            }
                        }
                    });
                });
                ui.separator();
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    inspector_head(ui, shown, is_gone, &name);
                    ui.add_space(10.0);
                    let hero_pos = live.poses.last().map(|(_, p)| p.pos).unwrap_or_default();
                    let rows = nearby(&live, hero_pos);
                    section_title(ui, &format!("NEARBY ENTITIES · {}", rows.len()));
                    egui::ScrollArea::vertical().id_source("live_nearby").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                        for (d, e) in rows {
                            let on = p.selection.as_deref() == Some(e.guid.as_str());
                            let r = ui.horizontal(|ui| {
                                let (s, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                                ui.painter().rect_filled(s, 2.0, swatch(e));
                                let label = format!("{} · {}", name(e.template), e.class.label());
                                let r = ui.add(egui::SelectableLabel::new(on, RichText::new(label).small()));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{d:.0} m")).small().monospace().color(theme::live::FAINT));
                                });
                                r
                            });
                            if r.inner.clicked() {
                                p.selection = Some(e.guid.clone());
                            }
                        }
                    });
                    if p.selection.is_some() && ui.small_button("Inspect the hero").clicked() {
                        p.selection = None;
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.label(theme::disp_text("LIVE LOG TAIL", theme::style_eyebrow(), theme::live::FAINT));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let t = match log.state {
                                LogState::Missing => "no log",
                                LogState::Stale => "stale",
                                LogState::Growing => "tailing",
                            };
                            ui.label(RichText::new(t).small().monospace().color(theme::live::FAINT));
                        });
                    });
                    egui::Frame::none().fill(theme::live::WELL).inner_margin(egui::Margin::same(7.0)).rounding(egui::Rounding::same(6.0)).show(ui, |ui| {
                        egui::ScrollArea::vertical().id_source("live_log").auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                            let font = theme::font(&egui::TextStyle::Small);
                            let part = |c: Color32| egui::TextFormat { font_id: font.clone(), color: c, ..Default::default() };
                            for l in &log.lines {
                                let mut job = egui::text::LayoutJob::default();
                                job.wrap.max_width = ui.available_width();
                                job.append(&l.ts, 0.0, part(theme::live::FAINT));
                                job.append(&format!(" [{}] ", l.source), 0.0, part(source_color(&l.source)));
                                job.append(&l.msg, 0.0, part(theme::live::DIM));
                                ui.label(job);
                            }
                        });
                    });
                });
            });
        },
    );
    p.right_width = width;
    acts
}

fn selection_lock(selection: Option<&str>) -> Lock {
    match selection {
        Some(g) => Lock::Entity(g.to_string()),
        None => Lock::Hero,
    }
}

fn source_color(s: &str) -> Color32 {
    match s {
        "world" => theme::live::GREEN,
        "lua" => theme::live::PINK,
        "crash" | "stall" => theme::live::RED,
        _ => theme::live::AMBER,
    }
}

fn inspector_head(ui: &mut egui::Ui, e: Option<&Entity>, gone: bool, name: &dyn Fn(u32) -> String) {
    let Some(e) = e else {
        ui.label(RichText::new("Nothing to inspect yet").color(theme::live::FAINT));
        return;
    };
    ui.label(RichText::new(name(e.template)).strong().color(theme::live::TX));
    ui.horizontal(|ui| {
        pill(ui, e.class.label().to_uppercase(), theme::live::VIOLET, false);
        if gone {
            pill(ui, "no longer reported".to_string(), theme::live::AMBER, false);
        }
    });
    ui.add_space(6.0);
    let kv = |ui: &mut egui::Ui, k: &str, v: String, c: Color32| {
        ui.horizontal(|ui| {
            ui.add_sized([74.0, 14.0], egui::Label::new(RichText::new(k).small().color(theme::live::FAINT)).truncate());
            ui.add(egui::Label::new(RichText::new(v).small().monospace().color(c)).truncate());
        });
    };
    kv(ui, "template", name(e.template), theme::live::TX);
    kv(ui, "model", name(e.model), theme::live::TX);
    kv(ui, "faction", if e.faction == 0 { "—".to_string() } else { name(e.faction) }, theme::live::TX);
    let hurt = e.health < e.max_health;
    kv(ui, "health", format!("{:.0} / {:.0}", e.health, e.max_health), if hurt { theme::live::AMBER } else { theme::live::GREEN });
    kv(ui, "position", format!("{:.1}, {:.1}, {:.1}", e.pos[0], e.pos[1], e.pos[2]), theme::live::TX);
    kv(ui, "yaw", format!("{:.0}°", e.yaw.rem_euclid(360.0)), theme::live::TX);
}

/// The footer's content, inside the app's status bar.
pub fn status(ui: &mut egui::Ui, p: &Page) {
    let live = p.link.snapshot();
    let log = p.log.snapshot();
    let (text, color) = link_text(&live);
    theme::status_dot(ui, &text, color);
    ui.separator();
    // The bridge follows the ladder whatever the log's verbosity; the log's rung stands in only when
    // nothing answers, as what the last run reached.
    let rung = match live.world.as_ref().filter(|w| w.phase >= 0) {
        Some(w) => format!("phase {} {}", w.phase, w.phase_name),
        None => log.rung.map_or_else(|| "no load seen".to_string(), |(i, n)| format!("phase {i} {n}")),
    };
    ui.label(RichText::new(format!("loadprobe · {rung}")).small().monospace().color(theme::DIM));
    ui.separator();
    let bridge = live.rtt.map_or_else(|| "—".to_string(), |d| format!("{} ms", d.as_millis()));
    let age = log.age.map_or_else(|| "—".to_string(), |d| format!("{} ms", d.as_millis().min(99_999)));
    ui.label(RichText::new(format!("bridge {bridge} · pmc_bb {age} · wgpu {:.1} ms", p.frame_s * 1000.0)).small().monospace().color(theme::DIM));
    if let Some(n) = &p.note {
        ui.separator();
        ui.label(RichText::new(n).small().color(theme::DIM));
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(RichText::new(format!("cam {} · zoom {:.0}%", p.cam.mode.label(), 100.0 / p.cam.zoom)).small().monospace().color(theme::DIM));
    });
}

/// The camera for this frame, and how it projects into the area the panels leave.
pub fn frame_view(p: &mut Page, screen: egui::Vec2, rect: egui::Rect) -> FrameView {
    let dt = p.tick();
    let live = p.link.snapshot();
    let now = Instant::now();
    let hero = smooth(&live.poses, now, hero_follow);
    let lock = match &p.lock {
        Lock::None => None,
        Lock::Hero => hero,
        Lock::Entity(g) => live.entities.iter().find(|e| e.guid == *g).map(|e| Follow { pos: Vec3::from(e.pos), yaw: e.yaw }),
    };
    let game = live.poses.last().and_then(|(_, q)| Some(live_cam::GameCamera { eye: Vec3::from(q.eye?), yaw: q.camera?.yaw, pitch: q.camera?.pitch }));
    let view = p.cam.view(dt, lock, hero, game);
    let shift = live_cam::centre_shift(screen, rect);
    let fv = FrameView { view, vp: view.view_proj(screen.x / screen.y.max(1.0), shift), screen, rect, shift };
    p.frame = Some(fv);
    fv
}

/// The floating parts of the viewport: the mission card, the mini-map, the camera readouts, the pills
/// and the markers. Drawn in the area the panels leave; the void around them is the 3D view's.
pub fn floats(ctx: &egui::Context, p: &mut Page, index: &HashMap<u32, String>, fv: FrameView) {
    let live = p.link.snapshot();
    if let Some((w, h, rgba)) = p.mirror.map.take() {
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
        p.map = Some(crate::live_map::Map { texture: ctx.load_texture("live_map", image, egui::TextureOptions::LINEAR) });
    }
    let now = Now { pose: live.poses.last().map(|(_, q)| q.clone()), names: p.names(), index, live };
    markers(ctx, p, &now, fv);
    let rect = fv.rect;
    let area = |id: &'static str, at: egui::Pos2, pivot: egui::Align2| egui::Area::new(egui::Id::new(id)).order(egui::Order::Foreground).fixed_pos(at).pivot(pivot);
    let glass = || egui::Frame::none().fill(theme::live::GLASS).stroke(egui::Stroke::new(1.0, theme::live::LINE)).rounding(egui::Rounding::same(9.0)).inner_margin(egui::Margin::same(9.0));

    area("live_mission", rect.left_top() + egui::vec2(14.0, 14.0), egui::Align2::LEFT_TOP).show(ctx, |ui| {
        glass().show(ui, |ui| {
            ui.set_width(286.0);
            mission_card(ui, &now.live);
        });
    });
    area("live_map", rect.right_top() + egui::vec2(-14.0, 14.0), egui::Align2::RIGHT_TOP).show(ctx, |ui| {
        glass().show(ui, |ui| {
            ui.set_width(196.0);
            crate::live_map::card(ui, p.map.as_ref(), &now.live, now.pose.as_ref());
        });
    });
    area("live_pills", rect.center_bottom() + egui::vec2(0.0, -56.0), egui::Align2::CENTER_BOTTOM).show(ctx, |ui| {
        glass().show(ui, |ui| {
            ui.horizontal(|ui| {
                let o = &mut p.overlays;
                let mut pill = |ui: &mut egui::Ui, on: &mut bool, label: &str, hover: &str| {
                    if ui.add(egui::SelectableLabel::new(*on, RichText::new(label).small())).on_hover_text(hover).clicked() {
                        *on = !*on;
                    }
                };
                pill(ui, &mut o.terrain, "Terrain", "Toggle solid terrain");
                pill(ui, &mut o.collision, "Collision", "Toggle the collision overlay");
                pill(ui, &mut o.ai, "AI debug", "Label nearby NPCs with their faction and health");
                pill(ui, &mut o.audio, "Audio", "Toggle audio emitters");
            });
        });
    });
    area("live_readout", rect.left_bottom() + egui::vec2(14.0, -14.0), egui::Align2::LEFT_BOTTOM).show(ctx, |ui| {
        glass().show(ui, |ui| {
            let eye = fv.view.eye.distance(fv.view.target);
            let retail = now.pose.as_ref().map_or_else(|| "—".to_string(), |q| format!("{:.0}, {:.0}, {:.0}", q.pos[0], q.pos[1], q.pos[2]));
            ui.label(RichText::new(format!("cam {eye:.0} m away · yaw {:.0}° · retail {retail}", p.cam.readout_yaw())).small().monospace().color(theme::live::DIM));
        });
    });
    area("live_zoom", rect.right_bottom() + egui::vec2(-14.0, -14.0), egui::Align2::RIGHT_BOTTOM).show(ctx, |ui| {
        glass().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("−").clicked() {
                    p.cam.step_zoom(false);
                }
                ui.label(RichText::new(format!("{:.0}%", 100.0 / p.cam.zoom)).small().monospace().color(theme::live::DIM));
                if ui.button("+").clicked() {
                    p.cam.step_zoom(true);
                }
            });
        });
    });
    let no_eye = p.cam.mode == Mode::MirrorInGame && now.pose.as_ref().is_some_and(|q| q.eye.is_none());
    let waiting = now
        .live
        .poses
        .is_empty()
        .then(|| link_text(&now.live).0)
        .or_else(|| p.mirror.note())
        .or_else(|| no_eye.then(|| "Mirror in-game needs an m2-sdk with M2Live: the game's Lua has no camera position".to_string()));
    if let Some(text) = waiting {
        egui::Area::new(egui::Id::new("live_wait")).order(egui::Order::Middle).fixed_pos(rect.center()).pivot(egui::Align2::CENTER_CENTER).show(ctx, |ui| {
            glass().show(ui, |ui| {
                ui.label(RichText::new(text).color(theme::live::DIM));
            });
        });
    }
}

fn mission_card(ui: &mut egui::Ui, live: &Live) {
    let m = &live.mission;
    if m.id.is_empty() {
        ui.label(theme::disp_text("FREE ROAM", theme::style_eyebrow(), theme::live::GOLD));
    } else {
        ui.horizontal(|ui| {
            ui.label(theme::disp_text(m.id.to_uppercase(), theme::style_eyebrow(), theme::live::GOLD));
            if let Some(r) = &m.race {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("CP {} / {}", r.at, r.total)).small().monospace().color(theme::live::GOLD));
                });
            }
        });
        if let Some((t, s)) = m.tasks.iter().find_map(|t| Some((t.time?, t.stop?))) {
            ui.horizontal(|ui| {
                ui.label(RichText::new(mmss(t)).small().monospace().color(theme::live::GOLD));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("of {}", mmss(s))).small().monospace().color(theme::live::GOLD));
                });
            });
            ui.add(egui::ProgressBar::new(if s > 0 { t as f32 / s as f32 } else { 0.0 }).desired_height(5.0).fill(theme::live::AMBER));
        }
        for t in m.tasks.iter().filter(|t| t.depth > 0 && t.state == 1).take(4) {
            ui.label(RichText::new(&t.name).small().color(theme::live::DIM));
        }
    }
    if let Some(w) = &live.world {
        ui.label(RichText::new(format!("{} jobs available", w.jobs)).small().color(theme::live::DIM));
    }
}

fn mmss(t: i32) -> String {
    format!("{:02}:{:02}", t / 60, t % 60)
}

/// The world-space markers, and the click that picks one.
fn markers(ctx: &egui::Context, p: &mut Page, now: &Now<'_>, fv: FrameView) {
    let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Background, egui::Id::new("live_markers"))).with_clip_rect(fv.rect);
    let font = theme::font(&egui::TextStyle::Small);
    p.picked = None;
    let pointer = ctx.pointer_latest_pos();
    let hero_pos = now.pose.as_ref().map(|q| q.pos).unwrap_or_default();
    overlay_lines(&painter, p, fv, Vec3::from(hero_pos));
    if let Some(r) = &now.live.mission.race {
        for (i, c) in r.points.iter().enumerate() {
            let n = i as u32 + 1;
            let Some((at, _)) = live_cam::project(fv.vp, Vec3::from(*c), fv.screen) else { continue };
            let (fg, label) = if n < r.at {
                (theme::live::OLIVE, "done".to_string())
            } else if n == r.at {
                (theme::live::AMBER, format!("active · {:.0} m", distance(*c, hero_pos)))
            } else {
                (theme::live::DIM, format!("{:.0} m", distance(*c, hero_pos)))
            };
            painter.circle(at, 10.0, theme::live::GLASS, egui::Stroke::new(2.0, fg));
            painter.text(at, egui::Align2::CENTER_CENTER, n.to_string(), font.clone(), fg);
            painter.text(at + egui::vec2(14.0, 0.0), egui::Align2::LEFT_CENTER, format!("CP · {label}"), font.clone(), fg);
        }
    }
    for e in &now.live.entities {
        if !fv.rect.contains(live_cam::project(fv.vp, Vec3::from(e.pos), fv.screen).map_or(egui::Pos2::ZERO, |x| x.0)) {
            continue;
        }
        let Some((at, _)) = live_cam::project(fv.vp, Vec3::from(e.pos) + Vec3::Y, fv.screen) else { continue };
        let hero = e.class == EntityClass::Hero;
        let color = if hero { theme::live::VIOLET } else { swatch(e) };
        let selected = p.selection.as_deref() == Some(e.guid.as_str());
        painter.circle(at, if hero { 5.0 } else { 4.0 }, color, egui::Stroke::new(if selected { 2.0 } else { 0.0 }, theme::live::TX));
        if p.overlays.ai && e.class == EntityClass::Human {
            let label = format!("{} · {:.0} / {:.0}", now.names.of(now.index, e.faction), e.health, e.max_health);
            painter.text(at + egui::vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, label, font.clone(), theme::live::DIM);
        }
        if hero {
            let dir = live_cam::facing(e.yaw);
            if let Some((tip, _)) = live_cam::project(fv.vp, Vec3::from(e.pos) + Vec3::Y + dir * 2.0, fv.screen) {
                painter.line_segment([at, tip], egui::Stroke::new(1.5, color));
            }
            let head = format!("{} · {:.0} / {:.0} hp", now.names.of(now.index, e.template), e.health, e.max_health);
            painter.text(at + egui::vec2(0.0, -16.0), egui::Align2::CENTER_BOTTOM, head, font.clone(), theme::live::TX);
        }
        if pointer.is_some_and(|c| (c - at).length() < 9.0) {
            p.picked = Some(e.guid.clone());
        }
    }
}

/// How far from the hero the data overlays draw.
const OVERLAY_RADIUS: f32 = 400.0;
const COLLISION_RADIUS: f32 = 60.0;
/// The edges one frame draws: the painter's cost, not the data's.
const COLLISION_EDGES: usize = 6000;

/// The overlays that are lines and points in the world.
fn overlay_lines(painter: &egui::Painter, p: &Page, fv: FrameView, hero: Vec3) {
    let project = |v: Vec3| live_cam::project(fv.vp, v, fv.screen).map(|x| x.0);
    let near = |v: &Vec3| v.distance_squared(hero) < OVERLAY_RADIUS * OVERLAY_RADIUS;
    let line = |points: &[Vec3], color: Color32, closed: bool| {
        if !points.iter().any(near) {
            return;
        }
        let mut at: Vec<Option<egui::Pos2>> = points.iter().map(|v| project(*v)).collect();
        if closed {
            at.push(at.first().copied().flatten());
        }
        for w in at.windows(2) {
            if let [Some(a), Some(b)] = w {
                painter.line_segment([*a, *b], egui::Stroke::new(1.5, color));
            }
        }
    };
    let o = p.overlays;
    if o.collision {
        for [a, b] in p.mirror.collision_edges(hero, COLLISION_RADIUS, COLLISION_EDGES) {
            if let (Some(a), Some(b)) = (project(a), project(b)) {
                painter.line_segment([a, b], egui::Stroke::new(1.0, theme::live::RED.gamma_multiply(0.7)));
            }
        }
    }
    let Some(layers) = &p.layers else { return };
    let marks = layers.marks();
    if o.paths {
        for path in &marks.paths {
            line(path, theme::live::AMBER, false);
        }
    }
    if o.triggers {
        for region in &marks.regions {
            line(region, theme::live::BLUE, true);
        }
    }
    let dots = |points: &[Vec3], color: Color32| {
        for v in points.iter().filter(|v| near(v)) {
            if let Some(a) = project(*v) {
                painter.circle_filled(a, 3.0, color);
            }
        }
    };
    if o.spawns {
        dots(&marks.spawners, theme::live::ORANGE);
    }
    if o.audio {
        dots(&marks.sounds, theme::live::TEAL);
    }
}

/// A click on the viewport: the marker under it becomes the selection.
pub fn click(p: &mut Page) {
    if let Some(g) = p.picked.take() {
        p.selection = Some(g);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pose(x: f32) -> Pose {
        Pose { pos: [x, 0.0, 0.0], yaw: 0.0, health: 1.0, max_health: 1.0, vehicle: None, camera: None, eye: None }
    }

    #[test]
    fn the_install_is_above_data() {
        assert_eq!(install_of(Path::new("/g/Mercs2/data/vz.wad")), Some(PathBuf::from("/g/Mercs2")));
    }

    #[test]
    fn the_view_runs_one_interval_behind_the_newest_sample() {
        let t0 = Instant::now();
        let t1 = t0 + Duration::from_millis(100);
        let poses = vec![(t0, pose(0.0)), (t1, pose(10.0))];
        let at = |ms| smooth(&poses, t1 + Duration::from_millis(ms), hero_follow).unwrap().pos.x;
        assert!((at(0) - 0.0).abs() < 1e-3, "when the new sample lands the view is still at the old one");
        assert!((at(50) - 5.0).abs() < 1e-3);
        assert!((at(100) - 10.0).abs() < 1e-3);
        assert!((at(500) - 10.0).abs() < 1e-3, "a late sample holds, it never overshoots");
    }

    #[test]
    fn a_rider_follows_the_vehicle() {
        let mut p = pose(1.0);
        p.vehicle = Some(crate::live_link::VehiclePose { guid: "0x1".into(), pos: [9.0, 9.0, 9.0], yaw: 30.0 });
        assert_eq!(hero_follow(&p), Follow { pos: Vec3::splat(9.0), yaw: 30.0 });
    }

    #[test]
    fn a_template_name_wins_and_an_unknown_hash_stays_visible() {
        let names = Names { templates: Rc::new(HashMap::from([(1, "mattiasupgrade4".to_string())])) };
        let index = HashMap::from([(1, "other".to_string()), (2, "pmc_hum_mattias_v4".to_string())]);
        assert_eq!(names.of(&index, 1), "mattiasupgrade4");
        assert_eq!(names.of(&index, 2), "pmc_hum_mattias_v4");
        assert_eq!(names.of(&index, 0x25C98327), "0x25C98327");
    }

    #[test]
    fn nearby_sorts_by_distance_and_leaves_out_the_hero() {
        let e = |g: &str, class, x| Entity {
            guid: g.into(), class, pos: [x, 0.0, 0.0], yaw: 0.0, health: 1.0, max_health: 1.0, template: 0, model: 0, faction: 0, name_key: String::new(),
        };
        let live = Live { entities: vec![e("h", EntityClass::Hero, 0.0), e("far", EntityClass::Car, 50.0), e("near", EntityClass::Human, 5.0)], ..Live::default() };
        let got: Vec<&str> = nearby(&live, [0.0; 3]).iter().map(|(_, e)| e.guid.as_str()).collect();
        assert_eq!(got, ["near", "far"]);
    }

    #[test]
    fn each_link_state_says_something_different() {
        let texts: Vec<String> = [
            LinkState::Off,
            LinkState::NoWorld { phase: 7, name: "Soundbanks ready".into() },
            LinkState::NoHero,
            LinkState::Streaming,
            LinkState::NotAnswering,
            LinkState::Failed("x".into()),
            LinkState::Replay { path: "a.replay".into(), finished: false },
        ]
        .into_iter()
        .map(|state| link_text(&Live { state, ..Live::default() }).0)
        .collect();
        let unique: std::collections::HashSet<&String> = texts.iter().collect();
        assert_eq!(unique.len(), texts.len(), "{texts:?}");
    }

    #[test]
    fn a_snapshot_round_trips_what_the_page_knew() {
        let path = std::env::temp_dir().join(format!("live_snapshot_{}.json", std::process::id()));
        let live = Live { addr: "127.0.0.1:27050".into(), poses: vec![(Instant::now(), pose(3.0))], ..Live::default() };
        write_snapshot(&live, &Camera::default(), Overlays::default(), &path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["addr"], "127.0.0.1:27050");
        assert_eq!(v["pose"]["pos"][0], 3.0);
        assert_eq!(v["camera_mode"], "Free orbit");
        assert_eq!(v["overlays"][0], "Terrain mesh");
        std::fs::remove_file(&path).unwrap();
    }
}
