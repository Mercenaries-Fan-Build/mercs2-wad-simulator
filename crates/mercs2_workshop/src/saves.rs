//! The Saves page — browse, inspect and edit a Mercenaries 2 `.profile`.
//!
//! A rail-level sibling of Inspect: not a Shipment-contribution surface. The reader
//! (`mercs2_formats::save`), the Lua AST (`mercs2_formats::save_lua`) and the writer
//! (`mercs2_formats::save_write`) are production-ready; this module is the UI front end
//! that turns the on-disk header + inflated Lua into panels and the panels back into
//! a byte-exact round-tripped `.profile`.
//!
//! # Panels (the four free functions)
//!
//! - [`navigator`] — the save-file list (one `.profile` per row, grouped by hero).
//! - [`center`]    — the viewport card: hero model + at-a-glance HUD.
//! - [`inspector`] — the scalar editor: character / upgrade / cash / fuel / contract.
//! - [`verbs`]     — the top-row actions: reload-from-disk, back up, write to `.profile`.
//!
//! Each returns a `Vec<Act>`; the app collects them into `saves_acts` and reduces via
//! [`apply`]. The panel itself never performs IO or touches the Rail — that stays in the
//! parent, matching the Quartermaster convention.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mercs2_formats::game_paths;
use mercs2_formats::save::{self, Profile, SaveState};
use mercs2_formats::save_lua::{self, Number, Value};
use mercs2_formats::save_write;

use crate::gui::theme;

/// A single intent emitted by one of the panels, applied by [`apply`].
#[derive(Clone, Debug)]
pub enum Act {
    /// Point the panel at a specific `.profile` on disk.
    Select(PathBuf),
    /// Re-enumerate the saves folder and reparse every `.profile` header.
    Reload,
    /// Switch which `tFlowData` lens the centre is rendering.
    PickMissionTab(MissionTab),
    /// Revert pending edits back to the on-disk `Profile`. Matches the "Discard" button
    /// — no file IO, no `save_write` call, just drop the working copy.
    Discard,
    /// Serialise the pending `Profile` via `save_write::write_profile` and overwrite the
    /// selected `.profile` on disk. The round-trip is byte-exact for an unmodified
    /// profile; a mutated one gets a fresh `ProfileHash` and remains loadable.
    Write,
    /// `@0x4D` — hero (1 Mattias / 2 Chris / 3 Jen). Writes to the pending Profile.
    SetCharacter(u8),
    /// `@0x4F` — upgrade tier 0..3. Drives `_tCharacterMap[hero].templates[iUpgrade]`
    /// at spawn time (`mrxplayer.lua:167-168`).
    SetUpgrade(u8),
    /// `@0x24A` — unlocked-costume count, 1..=5. Not the active costume (that one is a
    /// runtime-only override at `+0x63` whose on-disk byte is open).
    SetUnlockedCostumes(u8),
    /// Open the saves folder in the OS file manager.
    RevealFolder,
    /// Prompt for a `.profile` anywhere on disk via a native file picker.
    OpenFilePicker,
    /// `@0x18` — PMC cash. Signed-int32-shaped per the money-fuel memory; we expose
    /// `u32` to match the reader and clamp to the 1-billion cap on write.
    SetCash(u32),
    /// `@0x1C` — PMC fuel. Clamped at write time to not exceed `fuel_capacity`.
    SetFuel(u32),
    /// `@0x2F8` — fuel capacity (u16). Changing it lifts/lowers the fuel ceiling.
    SetFuelCapacity(u16),
    /// `@0x2C` — active contract mission id (ASCII, max 15 chars). Truncated on write.
    SetActiveContract(String),
    /// Change an active mission's `nState` by rewriting the inflated Lua payload. The target
    /// value is one of `MrxTaskState._kn{Latent=0, Active=1, Completed=2, Cancelled=3}`
    /// (`resident/mrxtaskstate.lua:1-4`). The reducer decompresses pending's Lua, walks
    /// `tFlowData.tActiveMissions[<id>].nState`, serialises via `save_lua::serialize`, and
    /// calls `save_write::set_lua_payload` so the next Write dumps the right payload.
    SetActiveMissionState(String, i32),
    /// Request that the hero viewport's 3D render be produced for `(hash, model_name)` and
    /// cached in `Panel.hero_textures`. The panel-local reducer only stamps `pending_hero_render`
    /// — the actual load_model + `shot::HeroRenderer` draw + texture allocation happens
    /// post-reducer in `app.rs`, which owns the `WadStack`, the egui context, and the
    /// renderer-cache the GPU state outlives one panel-visit on.
    ///
    /// The camera to render at is read from `Panel.hero_cam[hash]` (defaulted when missing), so
    /// this variant stays `(hash, name)` — a drag just mutates `hero_cam` via [`Act::SetHeroCam`]
    /// and re-emits `EnsureHeroRender` to request the matching redraw.
    EnsureHeroRender(u32, String),
    /// Stash the user's orbit-camera for a given hero model hash. Emitted every frame the user
    /// is dragging / scrolling the viewport, before the matching `EnsureHeroRender` request;
    /// `app.rs`'s post-reducer step reads back from `Panel.hero_cam` to drive
    /// [`crate::shot::HeroRenderer::render_rgba`].
    SetHeroCam(u32, crate::shot::OrbitCam),
}

/// The four `MrxTaskState` values a mission can hold on `nState`. Mirrors
/// `resident/mrxtaskstate.lua:1-4` verbatim so a wrong numeric constant surfaces as a
/// missing `match` arm rather than silently writing `0` into the save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Latent = 0,
    Active = 1,
    Completed = 2,
    Cancelled = 3,
}

impl TaskState {
    pub const ALL: [TaskState; 4] = [
        TaskState::Latent,
        TaskState::Active,
        TaskState::Completed,
        TaskState::Cancelled,
    ];
    pub fn label(self) -> &'static str {
        match self {
            TaskState::Latent => "Latent",
            TaskState::Active => "Active",
            TaskState::Completed => "Completed",
            TaskState::Cancelled => "Cancelled",
        }
    }
    pub fn from_f64(v: f64) -> Option<Self> {
        match v as i32 {
            0 => Some(TaskState::Latent),
            1 => Some(TaskState::Active),
            2 => Some(TaskState::Completed),
            3 => Some(TaskState::Cancelled),
            _ => None,
        }
    }
    pub fn color(self) -> egui::Color32 {
        match self {
            TaskState::Latent => theme::DIM,
            TaskState::Active => theme::BRASS,
            TaskState::Completed => theme::GOOD,
            TaskState::Cancelled => theme::BAD,
        }
    }
}

/// One row in the browser: the `.profile`'s on-disk path, header-level facts the parser
/// decoded, and (optionally) the parse error if that file refused to open.
#[derive(Clone, Debug)]
pub struct ProfileEntry {
    /// Absolute path to the `.profile` on disk. Click a row → [`Act::Select`] carries this.
    pub path: PathBuf,
    /// File stem — what the game named the slot (e.g. `Mattias Nilsson_6A0E523C`). The
    /// inflated-Lua `save_name` field is a *different* identifier (the autosave tag like
    /// `auto_48E12F36`); the file-stem is the one a modder recognises.
    pub stem: String,
    /// Decoded `.profile` header, or the parse error. Carried as `Result` so the browser
    /// can render an unopenable save loudly rather than silently dropping it.
    pub header: Result<Profile, String>,
}

impl ProfileEntry {
    /// `1` Mattias / `2` Chris / `3` Jen → an initial for the browser badge. `?` when
    /// the file failed to parse or the engine ever grows a fourth hero slot.
    fn hero_letter(&self) -> char {
        match self.header.as_ref().map(|h| h.character_index).ok() {
            Some(1) => 'M',
            Some(2) => 'C',
            Some(3) => 'J',
            _ => '?',
        }
    }
}

/// All state the Saves page carries. Lives on the stack in `app.rs` next to the other panels.
#[derive(Default)]
pub struct Panel {
    /// The `.profile` folder we last enumerated (resolved via
    /// `mercs2_formats::game_paths::saves_dir_from_env`). Cached so the status line can show
    /// *where* the browser is reading from without re-resolving every frame.
    pub saves_dir: Option<PathBuf>,
    /// `None` on the first render → the navigator emits a one-shot `Reload` to populate it.
    /// After that, `Some(vec)` holds the enumeration (empty vec is a valid "no saves here").
    pub profiles: Option<Vec<ProfileEntry>>,
    /// Which row the browser has selected, if any. Carries the file path, not an index, so
    /// a `Reload` that reorders the list keeps the selection pointed at the same save.
    pub selected: Option<PathBuf>,
    /// The working copy of the currently-selected `.profile`, cloned fresh on every
    /// [`Act::Select`] / [`Act::Discard`] / [`Act::Reload`]. Edit widgets in the inspector
    /// mutate THIS copy; the on-disk `ProfileEntry::header` stays as the "last-read-from-
    /// disk" baseline so the dirty indicator and Discard can compute diffs cheaply.
    pub pending: Option<Profile>,
    /// A short free-text banner surfaced in the browser header — the last reload's outcome,
    /// or the last write's `"wrote N bytes"` / error. Replaces whatever was there before.
    pub status: String,
    /// Cached decoded `SaveState` for the currently-selected `.profile`. The parse walks
    /// the full inflated-Lua text (`profile.save_state()`), which is cheap but not free —
    /// keyed by path so switching rows invalidates it, and keyed by `Result` so a parse
    /// failure is surfaced in the centre instead of silently retried every frame.
    pub lua_state: Option<(PathBuf, Result<SaveState, String>)>,
    /// Which mission-flow tab the centre is on (Active / Completed / Culled / Layers).
    pub mission_tab: MissionTab,
    /// Rendered-hero textures keyed by model hash. Populated by `app.rs` out-of-band when
    /// `pending_hero_render` is set and the renderer walks the WAD stack. Keyed by hash so the
    /// same tier/hero combo across different saves hits the same cached texture.
    pub hero_textures: HashMap<u32, egui::TextureHandle>,
    /// `Some((hash, model_name))` when the centre wants `app.rs` to render this hero THIS frame
    /// — either a cold start (no texture yet) or an orbit-drag re-render after a camera change.
    /// `app.rs` drains it at the end of each frame; the camera to use is read from
    /// `hero_cam[hash]`. A failed cold-start load leaves the hash in `hero_render_failed`.
    pub pending_hero_render: Option<(u32, String)>,
    /// Hashes we've tried and failed to render — matched against so the panel can show a
    /// one-line error instead of an eternal spinner. Error text is in the value.
    pub hero_render_failed: HashMap<u32, String>,
    /// Per-hero orbit camera, keyed by model hash. Survives save-switches so a modder who
    /// turned the Mattias hero to the three-quarter angle doesn't have to re-orbit when they
    /// scroll through other Mattias saves. A missing key = `OrbitCam::default()` the first
    /// time the viewport paints that hero.
    pub hero_cam: HashMap<u32, crate::shot::OrbitCam>,
}

/// The centre panel's current tab over `tFlowData`. Carried on the Panel so switching rails
/// keeps the modder's view of a given save sticky. Label vocabulary mirrors the mockup
/// (Completed / Available / Culled / Flow bindings) rather than the raw save-struct field
/// names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissionTab {
    /// Union of every mission id this save mentions, with each row's palette picked from
    /// whichever source it appears in (Completed > Available > Culled > Flow bindings). The
    /// mockup's broader "look at the whole board" view; the per-source tabs below are drill-ins.
    #[default]
    All,
    /// `tFlowData.tMyFlowData` — mission-id → flow-value flags (completed / advanced).
    Completed,
    /// `tFlowData.tActiveMissions` — in-progress missions (nState / nTargetsComplete / collected).
    Available,
    /// `tFlowData.tCulledBindings` — bindings the flow fixpoint has removed, in order.
    Culled,
    /// `tLayerData` — active `vz_state_*` world overlays. Labeled "Flow bindings" per the
    /// mockup's vocabulary (world-layer entries are the on-disk flow-binding set the engine
    /// restores on load) even though the raw field name is `tLayerData`.
    FlowBindings,
}

impl Panel {
    /// Re-enumerate the saves folder and reparse every `.profile` header. Idempotent.
    ///
    /// Resolution order mirrors `save_rollback`: `$MERCS2_SAVES_DIR`, then
    /// `Documents/My Games/Mercenaries 2/SaveGames` under `$USERPROFILE` / `$HOME`. When
    /// nothing resolves, [`profiles`] becomes `Some(vec![])` and [`status`] names the gap
    /// — the browser then reads as "configured, but empty", distinct from the first-render
    /// "not yet enumerated" state (`profiles.is_none()`).
    fn refresh(&mut self) {
        let dir = match game_paths::saves_dir_from_env() {
            Some(d) => d,
            None => {
                self.saves_dir = None;
                self.profiles = Some(Vec::new());
                self.status = format!(
                    "saves folder not found — set `{}` or ensure `{}` exists under $HOME",
                    game_paths::SAVES_DIR_VAR,
                    game_paths::SAVES_UNDER_HOME,
                );
                return;
            }
        };
        let mut entries: Vec<ProfileEntry> = Vec::new();
        let mut io_err: Option<String> = None;
        match std::fs::read_dir(&dir) {
            Ok(rd) => {
                for ent in rd.flatten() {
                    let p = ent.path();
                    if p.extension().and_then(|e| e.to_str()) != Some("profile") {
                        continue;
                    }
                    let stem = p
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    let header = match std::fs::read(&p) {
                        Ok(bytes) => save::parse(&bytes),
                        Err(e) => Err(format!("read: {e}")),
                    };
                    entries.push(ProfileEntry { path: p, stem, header });
                }
            }
            Err(e) => io_err = Some(format!("{}: {e}", dir.display())),
        }
        // Alphabetical: a browser that reorders rows on every refresh is unreadable.
        entries.sort_by(|a, b| a.stem.cmp(&b.stem));
        let n_ok = entries.iter().filter(|e| e.header.is_ok()).count();
        let n_bad = entries.len() - n_ok;
        self.status = match (io_err, n_bad) {
            (Some(e), _) => e,
            (None, 0) => format!("{n_ok} profiles"),
            (None, bad) => format!("{n_ok} profiles · {bad} unreadable"),
        };
        self.saves_dir = Some(dir);
        self.profiles = Some(entries);
        // A reload invalidates any pending edits — the on-disk bytes may have changed under
        // us, and keeping edits bound to a stale snapshot would silently blind-write them
        // over whatever the game just wrote.
        self.pending = None;
        self.repopulate_pending_from_selection();
    }

    /// Clone the parsed `Profile` of the currently-selected row into [`Self::pending`],
    /// giving the inspector a mutable working copy. No-op when nothing is selected or when
    /// the file failed to parse. Also invalidates the Lua-state cache so the centre reparses
    /// against the fresh selection on its next frame.
    fn repopulate_pending_from_selection(&mut self) {
        let Some(sel) = self.selected.as_ref() else {
            self.pending = None;
            self.lua_state = None;
            return;
        };
        let Some(list) = self.profiles.as_ref() else {
            self.pending = None;
            self.lua_state = None;
            return;
        };
        self.pending = list
            .iter()
            .find(|e| &e.path == sel)
            .and_then(|e| e.header.as_ref().ok().cloned());
        self.repopulate_lua_state_from_selection();
    }

    /// Populate `lua_state` by decoding the inflated-Lua payload of the currently-selected
    /// `.profile` via `Profile::save_state()`. Eagerly cached on every Select / Reload so the
    /// centre reads from the cache rather than re-parsing per frame — a `SaveState` walk is
    /// cheap, but the panels run at refresh rate.
    fn repopulate_lua_state_from_selection(&mut self) {
        self.lua_state = None;
        let Some(sel) = self.selected.as_ref().cloned() else {
            return;
        };
        let Some(list) = self.profiles.as_ref() else {
            return;
        };
        let Some(entry) = list.iter().find(|e| e.path == sel) else {
            return;
        };
        let result = match &entry.header {
            Ok(h) => h.save_state(),
            Err(e) => Err(e.clone()),
        };
        self.lua_state = Some((sel, result));
    }

    /// Serialise the pending `Profile` and overwrite the selected `.profile` on disk.
    /// On success, reparse the fresh bytes so the browser row + the pending baseline move
    /// forward together; on failure, leave both untouched so a retry works.
    fn write_to_disk(&mut self) {
        let (Some(path), Some(pending)) = (self.selected.as_ref().cloned(), self.pending.as_ref())
        else {
            self.status = "nothing selected to write".into();
            return;
        };
        let bytes = save_write::write_profile(pending);
        if let Err(e) = std::fs::write(&path, &bytes) {
            self.status = format!("write {}: {e}", path.display());
            return;
        }
        // Reparse so the browser row's header and `hash_ok` reflect the new on-disk truth.
        let reparsed = save::parse(&bytes);
        if let Some(list) = self.profiles.as_mut() {
            if let Some(ent) = list.iter_mut().find(|e| e.path == path) {
                ent.header = reparsed.clone();
            }
        }
        if let Ok(p) = reparsed {
            self.pending = Some(p);
        }
        self.status = format!("wrote {} ({} B)", path.display(), bytes.len());
    }
}

/// Compact money formatter — `322684096` → `$322M`, `2545064` → `$2.5M`, `12345` → `$12K`.
/// One decimal below 10 of a unit, whole numbers above. Full value is in the inspector.
fn money(n: u32) -> String {
    fn unit(n: u32, div: u32, suffix: char) -> String {
        if n < 10 * div {
            format!("${:.1}{suffix}", (n / (div / 10)) as f64 / 10.0)
        } else {
            format!("${}{suffix}", n / div)
        }
    }
    if n >= 1_000_000 {
        unit(n, 1_000_000, 'M')
    } else if n >= 1_000 {
        unit(n, 1_000, 'K')
    } else {
        format!("${n}")
    }
}

/// `246334` seconds → `"68h25m"`, `300` → `"00h05m"`. The `.profile` field is a `u32` of
/// elapsed seconds.
fn hms(s: u32) -> String {
    let h = s / 3600;
    let m = (s % 3600) / 60;
    format!("{h:02}h{m:02}m")
}

/// Browser-header path form: last three components, slash-joined, with a trailing slash.
fn tail(p: &Path) -> String {
    let n: Vec<_> = p.components().rev().take(3).collect();
    let joined = n
        .iter()
        .rev()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    format!("{joined}/")
}

/// The left-rail navigator: the saves browser.
///
/// On the first frame `panel.profiles` is `None` → we emit [`Act::Reload`] which populates it
/// in the next frame's reducer. From then on each row is a clickable [`theme::row_chip`]: the
/// hero letter, the file stem, and a compact meta line (`playtime · cash · upgrade N`).
pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();

    // Bottom-anchored verbs, allocated first via bottom_up so the list never pushes them
    // off-screen. In bottom_up the divider is added after the buttons so it sits above them.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let h = 26.0;
            let open_w = 78.0;
            let gap = ui.spacing().item_spacing.x;
            let reveal_w = (ui.available_width() - open_w - gap).max(60.0);
            let reveal = ui.add_enabled_ui(p.saves_dir.is_some(), |ui| {
                ui.add_sized(
                    [reveal_w, h],
                    egui::Button::new(egui::RichText::new("Reveal folder").size(11.5)),
                )
            });
            if reveal.inner.clicked() {
                acts.push(Act::RevealFolder);
            }
            if ui
                .add_sized(
                    [open_w, h],
                    egui::Button::new(egui::RichText::new("Open file…").size(11.5)),
                )
                .clicked()
            {
                acts.push(Act::OpenFilePicker);
            }
        });
        ui.add_space(8.0);
        let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        ui.painter().rect_filled(line, egui::Rounding::ZERO, theme::LINE);

        // Head content (eyebrow, path, status, list) stacks top-down from here upward.
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            navigator_head_and_list(ui, p, &mut acts);
        });
    });

    acts
}

fn navigator_head_and_list(ui: &mut egui::Ui, p: &Panel, acts: &mut Vec<Act>) {
    ui.label(theme::disp_text("SAVES", 15.0, theme::TX));
    if let Some(dir) = &p.saves_dir {
        ui.label(
            egui::RichText::new(tail(dir))
                .monospace()
                .size(10.5)
                .color(theme::FAINT),
        );
    } else {
        ui.label(
            egui::RichText::new("resolving saves folder…")
                .monospace()
                .size(10.5)
                .color(theme::FAINT),
        );
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(&p.status)
            .size(11.0)
            .color(theme::DIM),
    );
    ui.add_space(8.0);

    match &p.profiles {
        // One-shot bootstrap: first frame the panel renders, ask the reducer to load.
        None => {
            acts.push(Act::Reload);
            ui.label(
                egui::RichText::new("Loading…")
                    .size(11.5)
                    .color(theme::FAINT),
            );
        }
        Some(entries) if entries.is_empty() => {
            ui.label(
                egui::RichText::new("No .profile files in this folder.")
                    .size(11.5)
                    .color(theme::FAINT),
            );
        }
        Some(entries) => {
            ui.spacing_mut().item_spacing.y = 0.0;
            for e in entries {
                if save_row(ui, e, p.selected.as_ref() == Some(&e.path)).clicked() {
                    acts.push(Act::Select(e.path.clone()));
                }
            }
        }
    }
}

/// One flat browser row: hero badge + stem, meta line below. Selected rows get a brass wash
/// and a brass accent bar on the left edge; unreadable rows get a BAD accent bar.
fn save_row(ui: &mut egui::Ui, e: &ProfileEntry, selected: bool) -> egui::Response {
    // Reserved before the content so the fill lands underneath it once hover is known.
    let bg = ui.painter().add(egui::Shape::Noop);
    let ir = egui::Frame::none()
        .inner_margin(egui::Margin { left: 10.0, right: 8.0, top: 6.0, bottom: 6.0 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (badge_fg, badge_bd) = if selected {
                    (theme::BRASS, theme::BRASS_DK)
                } else {
                    (theme::DIM, theme::LINE2)
                };
                egui::Frame::none()
                    .stroke(egui::Stroke::new(1.0, badge_bd))
                    .rounding(egui::Rounding::same(2.0))
                    .inner_margin(egui::Margin::symmetric(3.0, 0.0))
                    .show(ui, |ui| {
                        ui.label(theme::disp_text(e.hero_letter().to_string(), 8.5, badge_fg));
                    });
                ui.add_space(2.0);
                let name_col = if selected { theme::BRASS } else { theme::TX };
                ui.add(
                    egui::Label::new(egui::RichText::new(&e.stem).size(12.5).color(name_col))
                        .truncate(),
                );
            });
            let meta = match &e.header {
                Ok(h) => {
                    let tail = if e.stem.starts_with("auto_") {
                        "auto".to_string()
                    } else {
                        format!("upgrade {}", h.upgrade_index)
                    };
                    format!("{} · {} · {tail}", hms(h.play_time_seconds), money(h.cash))
                }
                Err(err) => format!("unreadable — {err}"),
            };
            ui.label(egui::RichText::new(meta).monospace().size(10.0).color(theme::FAINT));
        });
    let resp = ir.response.interact(egui::Sense::click());
    let rect = resp.rect;
    let accent = match (&e.header, selected) {
        (Err(_), _) => Some(theme::BAD),
        (Ok(_), true) => Some(theme::BRASS),
        (Ok(_), false) => None,
    };
    let painter = ui.painter();
    let fill = if selected {
        Some(theme::BRASS_SOFT)
    } else if resp.hovered() {
        Some(theme::G2)
    } else {
        None
    };
    if let Some(f) = fill {
        painter.set(bg, egui::Shape::rect_filled(rect, egui::Rounding::same(2.0), f));
    }
    if let Some(c) = accent {
        let bar = egui::Rect::from_min_size(rect.min, egui::vec2(2.0, rect.height()));
        painter.rect_filled(bar, egui::Rounding::ZERO, c);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// The centre column: the inflated-Lua mission state view (`tFlowData` + `tLayerData`).
///
/// Read-only this commit — the tabs show what the engine serialised, and the next commit
/// turns the grid into clickable state-toggle chips (via `save_lua::parse_chunk` + surgical
/// edits + `save_write::set_lua_payload` + `write_profile`, the `save_rollback` pattern).
pub fn center(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(theme::G0)
                .inner_margin(egui::Margin::symmetric(22.0, 18.0)),
        )
        .show(ctx, |ui| {
            let Some(sel) = p.selected.as_ref() else {
                ui.label(theme::disp_text("NO SAVE OPEN", 20.0, theme::DIM));
                ui.label(
                    egui::RichText::new("Pick a save on the left to see its mission state.")
                        .size(11.5)
                        .color(theme::FAINT),
                );
                return;
            };
            let stem = sel
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            // On-disk header is the baseline the modder is editing against — the AT-A-GLANCE
            // card reads from it so cash / fuel / timestamp don't jump every keystroke. The
            // hero viewport, in contrast, reads from PENDING (what the inspector is about to
            // write): changing hero index or upgrade tier in the right-side panel must repose
            // the preview, not just the file.
            let on_disk = p
                .profiles
                .as_ref()
                .and_then(|list| list.iter().find(|e| &e.path == sel))
                .and_then(|e| e.header.as_ref().ok());
            let edit_view = p.pending.as_ref().or(on_disk);

            // Filename eyebrow — the row the browser is on, repeated here so a wide centre reads cleanly.
            ui.label(
                egui::RichText::new(&stem)
                    .monospace()
                    .size(11.0)
                    .color(theme::FAINT),
            );
            let mut hero_req: HeroReq = HeroReq::default();
            match (edit_view, on_disk) {
                (None, _) => {
                    ui.label(theme::disp_text("UNREADABLE", 20.0, theme::BAD));
                }
                (Some(ev), Some(od)) => {
                    hero_req = hero_viewport(ui, ev, od, p);
                }
                (Some(ev), None) => {
                    hero_req = hero_viewport(ui, ev, ev, p);
                }
            }
            // The viewport reports two kinds of state at once: the camera the user has already
            // moved to (always, so it's never lost on an in-flight render) and the fact that a
            // new render is wanted. SetHeroCam goes first so the reducer stamps `hero_cam`
            // before app.rs reads it for the EnsureHeroRender draw.
            if let Some((hash, cam)) = hero_req.set_cam {
                acts.push(Act::SetHeroCam(hash, cam));
            }
            if let Some((hash, name)) = hero_req.request_render {
                acts.push(Act::EnsureHeroRender(hash, name));
            }
            ui.add_space(12.0);

            let Some((_, parsed)) = p.lua_state.as_ref() else {
                ui.label(
                    egui::RichText::new("Lua not loaded yet — one frame…")
                        .size(11.5)
                        .color(theme::FAINT),
                );
                return;
            };
            let state = match parsed {
                Ok(s) => s,
                Err(e) => {
                    ui.label(
                        egui::RichText::new(format!("Lua parse failed: {e}"))
                            .size(11.5)
                            .color(theme::BAD),
                    );
                    return;
                }
            };

            // Tab strip. Mirrors the mockup: `PROGRESSION` eyebrow on the left names the
            // scope, then the four tabs with live `SaveState` counts, then an "only changed"
            // toggle right-aligned so a modder can filter the grid once a diff baseline lands.
            ui.horizontal(|ui| {
                ui.label(theme::disp_text("PROGRESSION", 10.0, theme::DIM));
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    for (tab, label, count) in [
                        (MissionTab::All,          "All missions",  all_missions_count(state)),
                        (MissionTab::Completed,    "Completed",     state.completed_flow.len()),
                        (MissionTab::Available,    "Available",     state.active_missions.len()),
                        (MissionTab::Culled,       "Culled",        state.flow_chain.len()),
                        (MissionTab::FlowBindings, "Flow bindings", state.layers.len()),
                    ] {
                        let on = p.mission_tab == tab;
                        if theme::pill(ui, &format!("{label}  {count}"), on).clicked() {
                            acts.push(Act::PickMissionTab(tab));
                        }
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Local-only cosmetic toggle for now — needs a baseline snapshot to
                    // actually filter, same dependency as the "Diff vs original" verb above.
                    let mut only_changed = false;
                    ui.add_enabled_ui(false, |ui| {
                        ui.checkbox(&mut only_changed, "only changed")
                            .on_disabled_hover_text(
                                "Filter the grid to missions whose state differs from the \
                                 on-disk baseline. Not wired yet — same baseline-snapshot \
                                 store as `Diff vs original`.",
                            );
                    });
                });
            });
            ui.add_space(10.0);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match p.mission_tab {
                    MissionTab::All => render_all_missions(ui, state),
                    MissionTab::Completed => render_completed(ui, &state.completed_flow),
                    MissionTab::Available => acts.extend(render_active(ui, &state.active_missions)),
                    MissionTab::Culled => render_culled(ui, &state.flow_chain),
                    MissionTab::FlowBindings => render_layers(ui, &state.layers),
                }
            });
        });
    acts
}

/// Outputs the hero viewport pushes back up to [`center`]: the camera the user has orbited to
/// (so it's never lost even if the renderer is a frame behind), and a flag that a new render is
/// wanted this frame. Both fields are plain `Option`s so a quiet viewport (no drag, nothing to
/// request) is `HeroReq::default()`.
#[derive(Default)]
struct HeroReq {
    /// `Some((hash, cam))` when the camera for `hash` just changed (drag or scroll this frame).
    set_cam: Option<(u32, crate::shot::OrbitCam)>,
    /// `Some((hash, model_name))` when `app.rs` should render this hero this frame.
    request_render: Option<(u32, String)>,
}

/// Centre viewport: hero portrait with four HUD overlays — ORBIT + model chip top-left,
/// AT-A-GLANCE top-right, identity chip bottom-left. Chips paint on top via
/// `allocate_ui_at_rect`. Drag orbits, scroll zooms; each change emits `SetHeroCam` +
/// `EnsureHeroRender`.
///
/// `h` drives the preview (pending edit, so inspector changes repose immediately).
/// `on_disk` backs AT-A-GLANCE so cash/fuel/timestamp stay stable while editing.
fn hero_viewport(ui: &mut egui::Ui, h: &Profile, on_disk: &Profile, p: &Panel) -> HeroReq {
    let viewport_height = 360.0;
    let model_name = upgrade_model_for(h.character_index, h.upgrade_index);
    let hash = mercs2_formats::hash::pandemic_hash_m2(&model_name);
    let cached_tex = p.hero_textures.get(&hash).cloned();
    let failed_err = p.hero_render_failed.get(&hash).cloned();
    let in_flight = matches!(&p.pending_hero_render, Some((h, _)) if *h == hash);
    // Current camera for this hero — a missing entry means "never orbited this one yet", which
    // the reducer treats as `OrbitCam::default()`.
    let current_cam = p
        .hero_cam
        .get(&hash)
        .copied()
        .unwrap_or_else(crate::shot::OrbitCam::default);
    let mut out = HeroReq::default();
    // Cold start: nothing cached and nothing in flight → request the initial render. The cam
    // is read by app.rs from `hero_cam[hash]` (defaulted when absent), so no SetHeroCam needed.
    if cached_tex.is_none() && failed_err.is_none() && !in_flight {
        out.request_render = Some((hash, model_name.clone()));
    }
    let model_label = format!("UPGRADE {} · {}", h.upgrade_index, model_name);
    let identity_label = format!(
        "{} · hero={} · upgrade={}",
        hero_name(h.character_index),
        h.character_index,
        h.upgrade_index
    );
    egui::Frame::none()
        .fill(theme::G0)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            // Pin exact height — min alone lets the Frame absorb the whole column.
            ui.set_width(ui.available_width());
            ui.set_min_height(viewport_height);
            ui.set_max_height(viewport_height);
            let frame_rect = ui.max_rect();

            // Whole frame is the drag/scroll region; chips allocated after this paint over it.
            let drag_response = ui.interact(
                frame_rect,
                ui.id().with(("hero_drag", hash)),
                egui::Sense::click_and_drag(),
            );

            let render_h = (frame_rect.height() - 12.0).max(140.0);
            let render_w = (frame_rect.width() - 12.0).max(140.0);
            let painter = ui.painter_at(frame_rect);
            match (&cached_tex, &failed_err, in_flight) {
                (Some(tex), _, _) => {
                    let img_size = tex.size_vec2();
                    let scale = (render_h / img_size.y).min(render_w / img_size.x);
                    let fit = img_size * scale;
                    // Centre the portrait horizontally in the frame — tall band, narrow image.
                    let paint_rect = egui::Rect::from_center_size(frame_rect.center(), fit);
                    painter.image(
                        tex.id(),
                        paint_rect,
                        egui::Rect::from_min_max(
                            egui::pos2(0.0, 0.0),
                            egui::pos2(1.0, 1.0),
                        ),
                        egui::Color32::WHITE,
                    );
                    // Mouse-drag → yaw / pitch. 0.01 rad per pixel: a 300 px horizontal
                    // drag spins the hero one full turn-ish (3 rad), which is the "spin
                    // to look around the model" feel without being so sensitive that a
                    // 2 px wobble triggers a re-render. Pitch is clamped just inside
                    // ±π/2 so the look_at never degenerates at the poles.
                    let mut cam = current_cam;
                    let mut moved = false;
                    if drag_response.dragged() {
                        let d = drag_response.drag_delta();
                        if d.x.abs() > 0.01 || d.y.abs() > 0.01 {
                            cam.yaw += d.x * 0.01;
                            cam.pitch = (cam.pitch - d.y * 0.01).clamp(-1.5, 1.5);
                            moved = true;
                        }
                    }
                    // Scroll → zoom. Clamp to a sane range so a scroll flick can't
                    // push the camera through the model or into the far-plane.
                    if drag_response.hovered() {
                        let scroll_y = ui.input(|i| i.smooth_scroll_delta.y);
                        if scroll_y.abs() > 0.1 {
                            let factor = (1.0_f32 - scroll_y * 0.0025).clamp(0.5, 2.0);
                            cam.zoom = (cam.zoom * factor).clamp(0.3, 6.0);
                            moved = true;
                        }
                    }
                    if moved {
                        out.set_cam = Some((hash, cam));
                        out.request_render = Some((hash, model_name.clone()));
                        ui.ctx().request_repaint();
                    }
                }
                (None, Some(err), _) => {
                    let box_rect = egui::Rect::from_center_size(
                        frame_rect.center(),
                        egui::vec2(render_w, render_h),
                    );
                    painter.rect_filled(box_rect, egui::Rounding::same(4.0), theme::G2);
                    painter.rect_stroke(box_rect, egui::Rounding::same(4.0), egui::Stroke::new(1.0, theme::BAD));
                    painter.text(
                        box_rect.center() - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        model_name.to_uppercase(),
                        egui::FontId::proportional(13.0),
                        theme::BAD,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        "render failed",
                        egui::FontId::proportional(10.5),
                        theme::BAD,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 22.0),
                        egui::Align2::CENTER_CENTER,
                        err,
                        egui::FontId::monospace(9.5),
                        theme::FAINT,
                    );
                }
                (None, None, _) => {
                    let box_rect = egui::Rect::from_center_size(
                        frame_rect.center(),
                        egui::vec2(render_w, render_h),
                    );
                    painter.rect_filled(box_rect, egui::Rounding::same(4.0), theme::G2);
                    painter.rect_stroke(
                        box_rect,
                        egui::Rounding::same(4.0),
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(0x42, 0x47, 0x4f)),
                    );
                    // Spinner needs a real widget — allocate a small UI centred on the box.
                    let spinner_rect = egui::Rect::from_center_size(
                        box_rect.center() - egui::vec2(0.0, 10.0),
                        egui::vec2(32.0, 32.0),
                    );
                    ui.allocate_ui_at_rect(spinner_rect, |ui| {
                        ui.add(egui::Spinner::new().size(28.0));
                    });
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        model_name.to_uppercase(),
                        egui::FontId::proportional(13.0),
                        theme::DIM,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 30.0),
                        egui::Align2::CENTER_CENTER,
                        if in_flight { "rendering…" } else { "queuing render…" },
                        egui::FontId::proportional(10.5),
                        theme::FAINT,
                    );
                }
            }

            // ── Overlay layer: HUD chips and the AT-A-GLANCE card, placed at absolute
            // positions INSIDE the viewport frame via `allocate_ui_at_rect`. These paint ON
            // TOP of the hero render, as proper HUD overlays rather than siblings laid out
            // above it. ──

            // Top-left: ORBIT chip + model chip. ORBIT reads brass `· DRAG` once there's a
            // cached texture (orbit input will produce a new frame instantly), dim `· WAITING`
            // while the first render is still queuing — honest readout of whether a drag will
            // actually do anything right now.
            let orbit_live = cached_tex.is_some();
            let (orbit_fg, orbit_bg, orbit_stroke) = if orbit_live {
                (theme::BRASS, theme::BRASS_SOFT, theme::BRASS_DK)
            } else {
                (theme::DIM, theme::G2, theme::LINE)
            };
            let orbit_label = if orbit_live { "ORBIT · DRAG" } else { "ORBIT · WAITING" };
            let topleft = frame_rect.min + egui::vec2(2.0, 2.0);
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(topleft, egui::vec2(500.0, 30.0)),
                |ui| {
                    ui.horizontal(|ui| {
                        chip(ui, orbit_label, orbit_fg, orbit_bg, orbit_stroke);
                        chip(ui, &model_label, theme::DIM, theme::G2, theme::LINE);
                    });
                },
            );

            // Top-right: AT-A-GLANCE overlay card. Width is fixed so the card hugs the right
            // edge; `allocate_ui_at_rect` plus `top_down(Align::LEFT)` inside the card keeps
            // the rows reading left-to-right regardless of ambient layout direction.
            let overlay_w = 240.0;
            let topright = egui::pos2(frame_rect.right() - overlay_w - 2.0, frame_rect.top() + 2.0);
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(topright, egui::vec2(overlay_w, 200.0)),
                |ui| at_a_glance(ui, on_disk),
            );

            // Bottom-left: identity chip.
            let identity_h = 24.0;
            let bottomleft = egui::pos2(frame_rect.left() + 2.0, frame_rect.bottom() - identity_h - 2.0);
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(bottomleft, egui::vec2(frame_rect.width() - 4.0, identity_h)),
                |ui| {
                    ui.horizontal(|ui| {
                        chip(ui, &identity_label, theme::DIM, theme::G2, theme::LINE);
                    });
                },
            );
        });
    out
}

/// A small rounded HUD chip — label text with a background fill + border stroke. Used by the
/// viewport overlays (ORBIT, model, identity) so they share a visual vocabulary.
fn chip(ui: &mut egui::Ui, label: &str, fg: egui::Color32, bg: egui::Color32, border: egui::Color32) {
    egui::Frame::none()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(3.0))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .show(ui, |ui| {
            ui.label(theme::disp_text(label, 9.5, fg));
        });
}

/// The hero's spawn model for the given upgrade tier. Reproduces the engine's
/// `_tCharacterMap[hero].templates[iUpgrade] or base` resolution without the engine. The
/// returned string is the model-registry name a modder would look up in the Inspect tab.
/// Tier 3 for every hero maps to the hero's `_v3`; the other tiers are the template names
/// (we don't have the per-tier model mapping tabulated beyond tier 3).
fn upgrade_model_for(hero: u8, tier: u8) -> String {
    let base = hero_base_model(hero);
    let templates = hero_tier_templates(hero);
    match (hero, tier) {
        (_, 0) => base.to_string(),
        (1, 3) => "pmc_hum_mattias_v3".to_string(),
        (2, 3) => "pmc_hum_chris_v3".to_string(),
        (3, 3) => "pmc_hum_jen_v3".to_string(),
        (_, t) if (1..=3).contains(&t) => format!("{} → template", templates[t as usize]),
        _ => base.to_string(),
    }
}

/// The AT-A-GLANCE HUD card — a compact column of save-state at-a-glance values. Lives in
/// the viewport's top-right overlay slot. Reads the on-disk `Profile` so it stays stable
/// while editing (the pending Profile would make the numbers jump every keystroke).
///
/// Forces `top_down` inside the Frame — the call site positions the card inside an RTL
/// alignment slot, and without this the body inherits that direction (egui 0.28
/// `prefer_right_to_left`) and the rows render sideways instead of stacking.
fn at_a_glance(ui: &mut egui::Ui, h: &Profile) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 16, 20, 220))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.with_layout(
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_min_width(220.0);
                    ui.label(theme::disp_text("AT A GLANCE", 9.5, theme::FAINT));
                    ui.add_space(3.0);
                    glance_row(ui, "contract", &h.active_contract);
                    glance_row(ui, "cash", &format!("{}", h.cash));
                    glance_row(ui, "fuel", &format!("{} / {}", h.fuel, h.fuel_capacity));
                    glance_row(ui, "playtime", &hms_long(h.play_time_seconds));
                    glance_row(ui, "timestamp", &timestamp_utc(h.timestamp));
                },
            );
        });
}

/// One row of the AT-A-GLANCE overlay: dim label on the left, mono value right-aligned.
/// `ui.horizontal` caps row height to the tallest child, which keeps a nested `right_to_left`
/// from absorbing the parent's spare vertical space.
fn glance_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(label)
                .size(10.0)
                .color(theme::FAINT),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .monospace()
                    .size(10.5)
                    .color(theme::TX),
            );
        });
    });
}

/// Long-form `h/m/s` playtime (`246334` → `"68h 25m 34s"`) for the AT-A-GLANCE strip, where
/// the browser row's compact `hms` form sacrifices the seconds.
fn hms_long(s: u32) -> String {
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    format!("{h}h {m:02}m {sec:02}s")
}

/// The Active tab's 5-column grid of mission chips. Each chip is a mission id + a right-side
/// state label (LATN/ACTV/CMPL/CANC — short to keep the chip width stable); clicking the chip
/// cycles the mission to the next state via [`Act::SetActiveMissionState`], so a modder can
/// walk a chain of chips without the chip width changing between frames.
///
/// Hover a chip to see its full state label and `nTargetsComplete` / `collected` detail.
fn render_active(ui: &mut egui::Ui, missions: &[save::ActiveMission]) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if missions.is_empty() {
        ui.label(
            egui::RichText::new("No active missions in this save.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return acts;
    }
    let cols = 5usize;
    let gutter = 4.0;
    let avail = ui.available_width();
    let cell_w = ((avail - (cols as f32 - 1.0) * gutter) / cols as f32).max(100.0);
    let mut col = 0usize;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(gutter, gutter);
        for m in missions {
            let cur = TaskState::from_f64(m.state);
            let (bg, bd, tag_bg, tag_fg, tag) = match cur {
                Some(TaskState::Completed) => (theme::G2, theme::GOOD_DK, theme::GOOD_SOFT, theme::GOOD, "DONE"),
                Some(TaskState::Active)    => (theme::G2, theme::BRASS_DK, theme::BRASS_SOFT, theme::BRASS, "OPEN"),
                Some(TaskState::Cancelled) => (theme::G2, theme::BAD, theme::G0, theme::BAD, "SKIP"),
                Some(TaskState::Latent) | None => (theme::G0, theme::LINE, theme::G0, theme::DIM, "LATN"),
            };
            let next = cur
                .map(|s| TaskState::ALL[((s as usize) + 1) % TaskState::ALL.len()])
                .unwrap_or(TaskState::Latent);
            let targets_txt = m
                .targets_complete
                .map(|n| format!("{} targets complete", n as i64))
                .unwrap_or_else(|| "no targets ticked".to_string());
            let hover = format!(
                "{}\nnState {} · {} · {} collected\nClick to cycle → {}",
                m.id,
                m.state as i64,
                targets_txt,
                m.collected.len(),
                next.label()
            );
            let resp = egui::Frame::none()
                .fill(bg)
                .stroke(egui::Stroke::new(1.0, bd))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(7.0, 4.0))
                .show(ui, |ui| {
                    ui.set_width(cell_w);
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(&m.id)
                                .monospace()
                                .size(10.5)
                                .color(theme::TX),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                egui::Frame::none()
                                    .fill(tag_bg)
                                    .rounding(egui::Rounding::same(2.0))
                                    .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                    .show(ui, |ui| {
                                        ui.label(theme::disp_text(tag, 8.5, tag_fg));
                                    });
                            },
                        );
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_text(hover);
            if resp.clicked() {
                acts.push(Act::SetActiveMissionState(m.id.clone(), next as i32));
            }
            col += 1;
            if col >= cols {
                ui.end_row();
                col = 0;
            }
        }
    });
    acts
}

/// A chip-grid palette: body colours (bg + border) + state-tag pill colours (bg + fg + label).
/// Grouped so the three non-interactive tabs can share one chip-grid renderer and vary only the
/// four colours + the three-letter tag.
#[derive(Clone, Copy)]
struct ChipStyle {
    bg: egui::Color32,
    bd: egui::Color32,
    tag_bg: egui::Color32,
    tag_fg: egui::Color32,
    tag: &'static str,
}

/// Shared 5-column chip grid used by every PROGRESSION tab except Available (which is
/// interactive). One palette per tab; id left, state-tag pill right.
fn mission_chip_grid(ui: &mut egui::Ui, items: &[(String, Option<String>)], s: ChipStyle) {
    let cols = 5usize;
    let gutter = 4.0;
    // 24 px: ScrollArea vertical scrollbar + grid trailing spacing.
    let avail = (ui.available_width() - 24.0).max(100.0);
    let cell_w = (avail - (cols as f32 - 1.0) * gutter) / cols as f32;
    // 16 = chip Frame inner_margin (7+7) + stroke (1+1).
    let row_inner = (cell_w - 16.0).max(40.0);
    egui::Grid::new(ui.auto_id_with(("mission_chip_grid", items.len(), s.tag)))
        .num_columns(cols)
        .min_col_width(cell_w)
        .max_col_width(cell_w)
        .spacing(egui::vec2(gutter, gutter))
        .show(ui, |ui| {
            for (i, (id, hover)) in items.iter().enumerate() {
                let resp = egui::Frame::none()
                    .fill(s.bg)
                    .stroke(egui::Stroke::new(1.0, s.bd))
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::symmetric(7.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.set_width(row_inner);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    egui::Frame::none()
                                        .fill(s.tag_bg)
                                        .rounding(egui::Rounding::same(2.0))
                                        .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                        .show(ui, |ui| {
                                            ui.label(theme::disp_text(s.tag, 8.5, s.tag_fg));
                                        });
                                    // add_sized gives the label the full remaining width so a
                                    // short id doesn't shrink against the tag with blank space
                                    // to its left.
                                    let remaining = ui.available_width();
                                    ui.add_sized(
                                        egui::vec2(remaining, 14.0),
                                        egui::Label::new(
                                            egui::RichText::new(id)
                                                .monospace()
                                                .size(10.5)
                                                .color(theme::TX),
                                        )
                                        .truncate(),
                                    );
                                },
                            );
                        });
                    })
                    .response;
                if let Some(h) = hover {
                    let _ = resp.on_hover_text(h);
                }
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
}

/// `tMyFlowData` chips — the completed story beats. Palette matches the mockup's green DONE pill.
fn render_completed(ui: &mut egui::Ui, flow: &std::collections::BTreeMap<String, f64>) {
    if flow.is_empty() {
        ui.label(
            egui::RichText::new("No completed-flow keys recorded.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    let items: Vec<(String, Option<String>)> = flow
        .iter()
        .map(|(k, v)| (k.clone(), Some(format!("{k}\nvalue {}", *v as i64))))
        .collect();
    mission_chip_grid(
        ui,
        &items,
        ChipStyle {
            bg: theme::G2,
            bd: theme::GOOD_DK,
            tag_bg: theme::GOOD_SOFT,
            tag_fg: theme::GOOD,
            tag: "DONE",
        },
    );
}

/// `tCulledBindings` chips — bindings the save trimmed out of the flow. Red/BAD palette so a
/// culled chip reads as a negative state, consistent with the mockup's SKIP styling.
fn render_culled(ui: &mut egui::Ui, chain: &[String]) {
    if chain.is_empty() {
        ui.label(
            egui::RichText::new("No culled bindings — this save hasn't trimmed any from the flow.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    let items: Vec<(String, Option<String>)> = chain
        .iter()
        .enumerate()
        .map(|(i, k)| (k.clone(), Some(format!("#{i} {k}\nculled from flow chain"))))
        .collect();
    mission_chip_grid(
        ui,
        &items,
        ChipStyle {
            bg: theme::G2,
            bd: theme::BAD,
            tag_bg: theme::G0,
            tag_fg: theme::BAD,
            tag: "SKIP",
        },
    );
}

/// `tLayerData` chips — the active `vz_state_*` world-overlay set the streamer restores. Dim
/// LAYR tag so a layer chip reads as neutral metadata, not a completion state.
fn render_layers(ui: &mut egui::Ui, layers: &[String]) {
    if layers.is_empty() {
        ui.label(
            egui::RichText::new("No world overlays in this save.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    let items: Vec<(String, Option<String>)> = layers
        .iter()
        .map(|l| (l.clone(), Some(format!("{l}\nworld overlay restored on load"))))
        .collect();
    mission_chip_grid(
        ui,
        &items,
        ChipStyle {
            bg: theme::G0,
            bd: theme::LINE,
            tag_bg: theme::G0,
            tag_fg: theme::DIM,
            tag: "LAYR",
        },
    );
}

/// Which bucket a mission falls into for the "All missions" view. First-match-wins priority:
/// a completed mission is DONE even if it also appears in `tLayerData`; a culled one is SKIP
/// even if the engine still has an overlay for it. Keep this in sync with [`all_missions`].
#[derive(Clone, Copy)]
enum AllMissionBucket {
    Done,
    Open,
    Skip,
    Layer,
}

impl AllMissionBucket {
    fn style(self) -> ChipStyle {
        match self {
            AllMissionBucket::Done => ChipStyle {
                bg: theme::G2,
                bd: theme::GOOD_DK,
                tag_bg: theme::GOOD_SOFT,
                tag_fg: theme::GOOD,
                tag: "DONE",
            },
            AllMissionBucket::Open => ChipStyle {
                bg: theme::G2,
                bd: theme::BRASS_DK,
                tag_bg: theme::BRASS_SOFT,
                tag_fg: theme::BRASS,
                tag: "OPEN",
            },
            AllMissionBucket::Skip => ChipStyle {
                bg: theme::G2,
                bd: theme::BAD,
                tag_bg: theme::G0,
                tag_fg: theme::BAD,
                tag: "SKIP",
            },
            AllMissionBucket::Layer => ChipStyle {
                bg: theme::G0,
                bd: theme::LINE,
                tag_bg: theme::G0,
                tag_fg: theme::DIM,
                tag: "LAYR",
            },
        }
    }
}

/// Union of every mission id this save mentions, deduped, bucketed by state. Priority:
/// Completed > Available > Culled > Flow bindings — a mission that is both completed and
/// present as a world overlay reads as DONE, not LAYR, because that's what matters when
/// scanning the board.
fn all_missions(state: &SaveState) -> Vec<(String, AllMissionBucket)> {
    use std::collections::BTreeMap;
    let mut by_id: BTreeMap<String, AllMissionBucket> = BTreeMap::new();
    for k in state.completed_flow.keys() {
        by_id.insert(k.clone(), AllMissionBucket::Done);
    }
    for m in &state.active_missions {
        by_id.entry(m.id.clone()).or_insert(AllMissionBucket::Open);
    }
    for k in &state.flow_chain {
        by_id.entry(k.clone()).or_insert(AllMissionBucket::Skip);
    }
    for l in &state.layers {
        by_id.entry(l.clone()).or_insert(AllMissionBucket::Layer);
    }
    by_id.into_iter().collect()
}

/// Count for the tab strip. Cheap — the real walk happens in [`all_missions`].
fn all_missions_count(state: &SaveState) -> usize {
    use std::collections::BTreeSet;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    seen.extend(state.completed_flow.keys().map(|s| s.as_str()));
    seen.extend(state.active_missions.iter().map(|m| m.id.as_str()));
    seen.extend(state.flow_chain.iter().map(|s| s.as_str()));
    seen.extend(state.layers.iter().map(|s| s.as_str()));
    seen.len()
}

/// The "All missions" tab — one chip per unique mission id across every source in the save,
/// each tagged by whichever bucket it fell into. Groups the chips by bucket (DONE first, then
/// OPEN, SKIP, LAYR) so the eye can scan completion density; within each group the ids are
/// alphabetical to match the per-source tabs.
fn render_all_missions(ui: &mut egui::Ui, state: &SaveState) {
    let rows = all_missions(state);
    if rows.is_empty() {
        ui.label(
            egui::RichText::new("No mission ids in this save yet.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    for bucket in [
        AllMissionBucket::Done,
        AllMissionBucket::Open,
        AllMissionBucket::Skip,
        AllMissionBucket::Layer,
    ] {
        let tag = bucket.style().tag;
        let items: Vec<(String, Option<String>)> = rows
            .iter()
            .filter(|(_, b)| matches!((b, bucket),
                (AllMissionBucket::Done, AllMissionBucket::Done)
                | (AllMissionBucket::Open, AllMissionBucket::Open)
                | (AllMissionBucket::Skip, AllMissionBucket::Skip)
                | (AllMissionBucket::Layer, AllMissionBucket::Layer)
            ))
            .map(|(id, _)| (id.clone(), Some(format!("{id}\n{tag}"))))
            .collect();
        if items.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(theme::disp_text(tag, 9.5, theme::FAINT));
        ui.add_space(2.0);
        mission_chip_grid(ui, &items, bucket.style());
    }
}

/// `1`/`2`/`3` → the hero's full name as the engine resolves it (`SHELL.SelectCharacter.*`
/// in the shell UI; see `memory/shell-menu-and-ui-pass.md`). `?` for anything else so a
/// future hero slot shows loudly rather than being silently mapped to Mattias.
fn hero_name(index: u8) -> &'static str {
    match index {
        1 => "Mattias Nilsson",
        2 => "Chris Jacobs",
        3 => "Jennifer Mui",
        _ => "?",
    }
}

/// `1`/`2`/`3` → the hero's **base** spawn model. Per `_tCharacterMap[hero].base` in
/// `docs/mercs2-luacd/src/resident/mrxplayer.lua:22,43,59` — the model the engine spawns
/// when `iUpgrade == 0`, which is what the HERO dropdown shows alongside the display name.
fn hero_base_model(index: u8) -> &'static str {
    match index {
        1 => "pmc_hum_mattias",
        2 => "pmc_hum_chris",
        3 => "pmc_hum_jen",
        _ => "",
    }
}

/// `_tCharacterMap[hero].templates` — the four `iUpgrade` template names the engine passes
/// through at spawn (`mrxplayer.lua:167-168`: `tTemplates[iUpgrade] or tCharacterData.base`).
/// Tier 0 falls back to the base template; tiers 1-3 are the hero's progression templates.
/// Source: `mrxplayer.lua:23-27, 44-48, 60-64`.
fn hero_tier_templates(index: u8) -> [&'static str; 4] {
    match index {
        1 => ["mattias", "mattiasupgrade1", "mattiasupgrade2", "mattiasupgrade3"],
        2 => ["chris", "chrisupgrade1", "chrisupgrade2", "chrisupgrade3"],
        3 => ["jen", "jenupgrade1", "jenupgrade2", "jenupgrade3"],
        _ => ["", "", "", ""],
    }
}

/// `_tOutfits[hero]` — the base-game costumes a hero can switch to at the mirror. All three
/// heroes' tables extracted verbatim from `docs/mercs2-luacd/src/vz/wifpmcinterior.lua`:
///   - Mattias: lines 215-241 (5 outfits)
///   - Chris:   lines 156-182 (5 outfits)
///   - Jen:     lines 183-214 (6 outfits — Jen is the only hero with ChickenSuit AND five
///              non-chicken skins; the retail Lua table is keyed `jennifer`, not `jen`.)
///
/// Order matches the source so the UNLOCKED COUNT prefix-reveal lines up with what the
/// engine's `SetAvailableCostumes` surfaces at the mirror.
fn hero_outfits(index: u8) -> &'static [(&'static str, &'static str)] {
    match index {
        1 => &[
            ("Original",    "pmc_hum_mattias"),
            ("MetalHead",   "pmc_hum_mattias_v3"),
            ("Suit",        "pmc_hum_mattias_v2"),
            ("Jacket",      "pmc_hum_mattias_v4"),
            ("ChickenSuit", "pmc_hum_mattias_chickensuit"),
        ],
        2 => &[
            ("Original",    "pmc_hum_chris"),
            ("Vacation",    "pmc_hum_chris_v2"),
            ("Commando",    "pmc_hum_chris_v3"),
            ("OffDuty",     "pmc_hum_chris_v4"),
            ("ChickenSuit", "pmc_hum_chris_chickensuit"),
        ],
        3 => &[
            ("Original",    "pmc_hum_jen"),
            ("Rebel",       "pmc_hum_jen_v3"),
            ("Tactical",    "pmc_hum_jen_v5"),
            ("NoJacket",    "pmc_hum_jen_v2"),
            ("CatSuit",     "pmc_hum_jen_v4"),
            ("ChickenSuit", "pmc_hum_jen_chickensuit"),
        ],
        _ => &[],
    }
}

/// Format a Unix-epoch `u32` as `YYYY-MM-DD HH:MM UTC`. The `.profile` field at `@0x24`
/// is a UTC timestamp (`docs/reverse_engineer/save_serialize_code_map.md` §4). Hand-rolled
/// civil-from-days conversion (Howard Hinnant's standard trick) so the Workshop doesn't
/// take a chrono / humantime dep just for one line in the inspector.
fn timestamp_utc(t: u32) -> String {
    let total = t as i64;
    let (days, secs_in_day) = (total.div_euclid(86_400), total.rem_euclid(86_400));
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    // Civil from days (Hinnant): shift so year 0000-03-01 is day 0, then invert.
    let z = days + 719_468;
    let era = if z >= 0 { z / 146_097 } else { (z - 146_096) / 146_097 };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y_shift = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y_shift + if month <= 2 { 1 } else { 0 };
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// A labelled unsigned-integer scalar edit. Clamps to `[0, max]` so a `u32` can't underflow.
fn u32_row(
    ui: &mut egui::Ui,
    key: &str,
    value: u32,
    max: u64,
) -> Option<u32> {
    let mut v = value as i64;
    let mut edited: Option<u32> = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(key).color(theme::DIM).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui.add(
                egui::DragValue::new(&mut v)
                    .range(0..=max as i64)
                    .speed(1.0),
            );
            if resp.changed() {
                edited = Some(v.clamp(0, max as i64) as u32);
            }
        });
    });
    edited
}

/// The right-side inspector: a stack of collapsible `theme::section` cards matching the mockup
/// (SAVE strip + HERO / STORY UPGRADE / WARDROBE / RESOURCES / COLLECTIBLES / METADATA).
///
/// Live-edit: every widget change pushes a `Set*` Act; the reducer applies to `Panel::pending`
/// (a working clone of the parsed `Profile`). The on-disk bytes don't move until Write.
pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    // Force every child (checksum banner + every `theme::section` Frame) to span the full
    // inspector width even when collapsed. Without this a `theme::section` with no body
    // (METADATA default-closed) shrinks to just the header text's bounding box and the
    // card reads as "narrower than the others", which looks like a layout bug.
    ui.set_width(ui.available_width());
    theme::eyebrow(ui, "SAVE");

    let selected = p.selected.as_ref();
    let entry = selected.and_then(|path| {
        p.profiles
            .as_ref()
            .and_then(|list| list.iter().find(|e| &e.path == path))
    });
    let Some(entry) = entry else {
        ui.label(
            egui::RichText::new("Pick a save on the left to inspect it.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return acts;
    };

    ui.label(
        egui::RichText::new(&entry.stem)
            .monospace()
            .size(12.5)
            .color(theme::TX),
    );
    ui.add_space(6.0);

    let on_disk = match &entry.header {
        Ok(h) => h,
        Err(e) => {
            ui.label(
                egui::RichText::new(format!("Parse failed: {e}"))
                    .size(11.5)
                    .color(theme::BAD),
            );
            return acts;
        }
    };
    let pending = p.pending.as_ref().unwrap_or(on_disk);
    let dirty = profile_differs(pending, on_disk);

    // Checksum / version banner. Lives above the sections rather than inside one so a
    // broken checksum reads loudly first.
    let (hc, ht) = if on_disk.hash_ok() {
        (theme::GOOD, "CHECKSUM OK")
    } else {
        (theme::BAD, "CHECKSUM MISMATCH")
    };
    egui::Frame::none()
        .fill(theme::G2)
        .stroke(egui::Stroke::new(1.0, if on_disk.hash_ok() { theme::GOOD_DK } else { theme::BAD }))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(10.0, 7.0))
        .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                theme::status_dot(ui, ht, hc);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("version {}", on_disk.version))
                            .monospace()
                            .size(10.0)
                            .color(theme::FAINT),
                    );
                });
            });
            ui.label(
                egui::RichText::new("CRC-32/BZIP2 checksum verified")
                    .size(10.5)
                    .color(theme::FAINT),
            );
            if dirty {
                ui.add_space(2.0);
                theme::status_dot(ui, "pending edits", theme::BRASS);
            }
        });

    // HERO — dropdown over the three real heroes, carrying the hero's base model name so a
    // modder sees the file the engine will spawn by default without opening the Inspect tab.
    let mut hero_edit: Option<u8> = None;
    theme::section(ui, "HERO", None, true, |ui| {
        ui.label(
            egui::RichText::new("Playing as")
                .size(10.0)
                .color(theme::FAINT),
        );
        egui::ComboBox::from_id_source("saves.hero")
            .selected_text(format!(
                "{} · {}",
                hero_name(pending.character_index),
                hero_base_model(pending.character_index),
            ))
            .width(ui.available_width() - 2.0)
            .show_ui(ui, |ui| {
                for i in [1u8, 2, 3] {
                    let on = pending.character_index == i;
                    if ui
                        .selectable_label(on, format!("{} · {}", hero_name(i), hero_base_model(i)))
                        .clicked()
                    {
                        hero_edit = Some(i);
                    }
                }
            });
    });
    if let Some(v) = hero_edit {
        acts.push(Act::SetCharacter(v));
    }

    // WARDROBE — active override dropdown (runtime-only on disk, surfaced for a future write
    // byte at `+0x4E`) + an UNLOCKED COUNT stepper over `@0x24A` + a prefix-unlocked list.
    // When the hero's `_tOutfits` table isn't catalogued (Chris / Jen for now), the panel
    // says so plainly rather than rendering an empty list under a stepper of "/ 1".
    let mut costumes_edit: Option<u8> = None;
    theme::section(ui, "WARDROBE", None, true, |ui| {
        let outfits = hero_outfits(pending.character_index);
        if outfits.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "No _tOutfits.<{}> catalogued yet — Mattias's table is \
                     extracted from wifpmcinterior.lua:215; the other heroes' \
                     tables live in the same file and will land in a later commit.",
                    hero_name(pending.character_index).split_whitespace().next().unwrap_or("hero").to_lowercase()
                ))
                .size(10.5)
                .color(theme::FAINT),
            );
            theme::kv(
                ui,
                "unlocked count",
                egui::RichText::new(format!("{}", pending.unlocked_costumes)),
            );
            return;
        }
        ui.label(
            egui::RichText::new("Active override")
                .size(10.0)
                .color(theme::FAINT),
        );
        egui::ComboBox::from_id_source("saves.wardrobe.active")
            .selected_text("None  ·  (use story upgrade)")
            .width(ui.available_width() - 2.0)
            .show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "None  ·  (use story upgrade)");
                for (name, model) in outfits {
                    let _ = ui.selectable_label(false, format!("{name}  ·  {model}"));
                }
            });
        ui.label(
            egui::RichText::new("runtime-only · not persisted")
                .size(9.5)
                .color(theme::FAINT),
        );
        ui.add_space(8.0);

        let cap = outfits.len() as u8;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("UNLOCKED COUNT")
                    .size(10.0)
                    .color(theme::DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("/ {cap}"))
                        .monospace()
                        .size(10.5)
                        .color(theme::FAINT),
                );
                if ui.small_button("+").clicked() {
                    costumes_edit = Some(pending.unlocked_costumes.saturating_add(1).min(cap));
                }
                ui.label(
                    egui::RichText::new(format!("{}", pending.unlocked_costumes))
                        .monospace()
                        .size(12.0),
                );
                if ui.small_button("−").clicked() {
                    costumes_edit = Some(pending.unlocked_costumes.saturating_sub(1).max(1));
                }
            });
        });
        ui.label(
            egui::RichText::new(
                "The byte stores a count — the first N entries of _tOutfits.<hero> \
                 become selectable at the mirror.",
            )
            .size(9.5)
            .color(theme::FAINT),
        );
        ui.add_space(6.0);
        for (i, (name, model)) in outfits.iter().enumerate() {
            let on = (i as u8) < pending.unlocked_costumes;
            let (fill, border, dot_fg) = if on {
                (theme::G0, theme::LINE, theme::BRASS)
            } else {
                (theme::G0, theme::LINE2, theme::FAINT)
            };
            theme::row_chip(ui, fill, border, |ui| {
                ui.label(theme::disp_text(if on { "●" } else { "○" }, 11.0, dot_fg));
                ui.label(
                    egui::RichText::new(*name)
                        .size(11.5)
                        .color(if on { theme::TX } else { theme::DIM }),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(*model)
                            .monospace()
                            .size(10.0)
                            .color(theme::FAINT),
                    );
                });
            });
        }
    });
    if let Some(v) = costumes_edit {
        acts.push(Act::SetUnlockedCostumes(v));
    }

    // RESOURCES — cash / fuel / fuel cap / active contract.
    theme::section(ui, "RESOURCES", None, true, |ui| {
        if let Some(v) = u32_row(ui, "cash", pending.cash, 1_000_000_000) {
            acts.push(Act::SetCash(v));
        }
        if let Some(v) = u32_row(ui, "fuel", pending.fuel, pending.fuel_capacity.max(1) as u64) {
            acts.push(Act::SetFuel(v));
        }
        if let Some(v) = u32_row(ui, "fuel cap", pending.fuel_capacity as u32, u16::MAX as u64) {
            acts.push(Act::SetFuelCapacity(v.min(u16::MAX as u32) as u16));
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("active").color(theme::DIM).size(12.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let mut buf = pending.active_contract.clone();
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut buf)
                        .char_limit(15)
                        .desired_width(140.0)
                        .font(egui::TextStyle::Monospace),
                );
                if resp.changed() {
                    acts.push(Act::SetActiveContract(buf));
                }
            });
        });
    });

    // COLLECTIBLES — a per-mission breakdown of `active_missions[].collected` GUIDs. This is
    // the real shape the save tracks: each active mission carries its own `tCollected` Vec<u32>
    // of ASET entity ids, so the panel surfaces mission-id → count rows + a running total. The
    // mockup's per-TYPE breakdown (toolboxes / parts / cards) wants an ASET registry classifier
    // the Workshop doesn't ship yet; honest to say "per-mission for now" instead of inventing
    // caps the game doesn't publish.
    let state = p
        .lua_state
        .as_ref()
        .and_then(|(_, r)| r.as_ref().ok());
    let collected_total: usize = state.map(|s| s.collected_count()).unwrap_or(0);
    theme::section(
        ui,
        "COLLECTIBLES",
        Some(&format!("{collected_total} total")),
        true,
        |ui| {
            ui.label(
                egui::RichText::new(
                    "ASET GUIDs in active_missions.collected, one row per contributing mission.",
                )
                .size(10.5)
                .color(theme::FAINT),
            );
            ui.add_space(5.0);
            match state {
                None => {
                    ui.label(
                        egui::RichText::new("Lua not loaded — pick a save to populate.")
                            .size(11.0)
                            .color(theme::FAINT),
                    );
                }
                Some(s) => {
                    let mut rows: Vec<(&str, usize)> = s
                        .active_missions
                        .iter()
                        .filter(|m| !m.collected.is_empty())
                        .map(|m| (m.id.as_str(), m.collected.len()))
                        .collect();
                    if rows.is_empty() {
                        ui.label(
                            egui::RichText::new("No tCollected entries in any active mission.")
                                .size(11.0)
                                .color(theme::FAINT),
                        );
                        return;
                    }
                    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
                    for (id, n) in rows {
                        theme::row_chip(ui, theme::G0, theme::LINE, |ui| {
                            ui.label(
                                egui::RichText::new(id)
                                    .monospace()
                                    .size(11.5)
                                    .color(theme::TX),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{n}"))
                                            .monospace()
                                            .size(11.0)
                                            .color(theme::BRASS),
                                    );
                                },
                            );
                        });
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Per-type breakdown (toolbox / part / card) needs an ASET-id → \
                             collectible-type classifier that the Workshop doesn't ship yet.",
                        )
                        .size(9.5)
                        .color(theme::FAINT),
                    );
                }
            }
        },
    );

    // METADATA — read-only; the engine stamps playtime and timestamp itself.
    theme::section(ui, "METADATA", None, false, |ui| {
        theme::kv(ui, "playtime", egui::RichText::new(hms(pending.play_time_seconds)));
        theme::kv(ui, "timestamp", egui::RichText::new(timestamp_utc(pending.timestamp)));
        theme::kv(ui, "slot name", egui::RichText::new(pending.save_name().to_string()));
        theme::kv(ui, "size", egui::RichText::new(format!("{} B", on_disk.raw_bytes().len())));
    });

    acts
}

/// True when the pending Profile differs from the on-disk one on any **editable** field.
/// Internal-only — the inspector uses it to light the "pending edits" dot.
fn profile_differs(pending: &Profile, on_disk: &Profile) -> bool {
    pending.character_index != on_disk.character_index
        || pending.upgrade_index != on_disk.upgrade_index
        || pending.unlocked_costumes != on_disk.unlocked_costumes
        || pending.cash != on_disk.cash
        || pending.fuel != on_disk.fuel
        || pending.fuel_capacity != on_disk.fuel_capacity
        || pending.active_contract != on_disk.active_contract
}

/// The top-row verbs: Reload / Back up / Diff vs original / Discard / Write.
///
/// Order matches the mockup. "Back up" and "Diff vs original" are stubs for now (disabled
/// with explanatory hover text) — the backup wants a destination picker + versioned filename
/// scheme, and the diff wants a baseline-snapshot store, both deferrable. "Write to .profile"
/// is only live when there is a selection AND pending edits actually differ from the on-disk
/// header, so a disabled button for a clean save tells the user they have nothing queued
/// without them having to compare the inspector by eye.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if ui.button("Reload from disk").clicked() {
        acts.push(Act::Reload);
    }
    ui.add_enabled(false, egui::Button::new("Back up"))
        .on_disabled_hover_text(
            "Copy the current `.profile` to a timestamped sibling. Not wired yet — \
             needs a destination picker and a versioned filename scheme.",
        );
    ui.add_enabled(false, egui::Button::new("Diff vs original"))
        .on_disabled_hover_text(
            "Byte-diff the pending Profile against an on-launch snapshot of the on-disk \
             bytes. Not wired yet — needs the baseline snapshot store.",
        );
    let dirty = match (&p.pending, &p.selected) {
        (Some(pend), Some(_)) => p
            .profiles
            .as_ref()
            .and_then(|list| list.iter().find(|e| Some(&e.path) == p.selected.as_ref()))
            .and_then(|e| e.header.as_ref().ok())
            .is_some_and(|on_disk| profile_differs(pend, on_disk)),
        _ => false,
    };
    if ui.add_enabled(dirty, egui::Button::new("Discard edits")).clicked() {
        acts.push(Act::Discard);
    }
    if theme::primary_button(ui, "Write to .profile", dirty).clicked() {
        acts.push(Act::Write);
    }
    acts
}

/// The Workshop's bottom status bar hands off to this when the Saves rail tab is active
/// and a save is selected — matching the mockup's `LOADED · 13,404 B · checksum ok ·
/// payload inflated`. Returns `(dot_color, dot_label, detail)`; `None` falls back to the
/// Workshop's normal "Ready / Loading" banner.
pub fn saves_status_banner(p: &Panel) -> Option<(egui::Color32, &'static str, String)> {
    let selected = p.selected.as_ref()?;
    let entry = p
        .profiles
        .as_ref()
        .and_then(|list| list.iter().find(|e| &e.path == selected))?;
    match &entry.header {
        Ok(h) => {
            let bytes = h.raw_bytes().len();
            let checksum = if h.hash_ok() { "checksum ok" } else { "checksum MISMATCH" };
            let payload = match &p.lua_state {
                Some((_, Ok(_))) => "payload inflated",
                Some((_, Err(_))) => "payload decompression failed",
                None => "payload not yet decompressed",
            };
            let dot_color = if h.hash_ok() { theme::GOOD } else { theme::BAD };
            Some((
                dot_color,
                if h.hash_ok() { "LOADED" } else { "LOADED · HASH BAD" },
                format!("{bytes} B · {checksum} · {payload}"),
            ))
        }
        Err(e) => Some((theme::BAD, "ERROR", format!("parse failed: {e}"))),
    }
}

/// Reduce the acts the four panels emit onto [`Panel`] state.
///
/// Mirrors [`crate::quartermaster::apply`]. Panel-local decisions live here; anything that
/// crosses workbenches (switching rails, opening a Shipment, etc.) goes back out to the
/// caller via a separate `Act` variant and is handled in `app.rs`.
pub fn apply(act: Act, p: &mut Panel) {
    match act {
        Act::Select(path) => {
            p.selected = Some(path);
            p.repopulate_pending_from_selection();
        }
        Act::Reload => p.refresh(),
        Act::PickMissionTab(t) => p.mission_tab = t,
        Act::Discard => p.repopulate_pending_from_selection(),
        Act::Write => p.write_to_disk(),
        Act::SetCharacter(v) => mutate_pending(p, |pr| pr.character_index = v),
        Act::SetUpgrade(v) => mutate_pending(p, |pr| pr.upgrade_index = v),
        Act::SetUnlockedCostumes(v) => mutate_pending(p, |pr| pr.unlocked_costumes = v),
        Act::SetCash(v) => mutate_pending(p, |pr| pr.cash = v),
        Act::SetFuel(v) => mutate_pending(p, |pr| pr.fuel = v),
        Act::SetFuelCapacity(v) => mutate_pending(p, |pr| pr.fuel_capacity = v),
        Act::SetActiveContract(mut s) => mutate_pending(p, |pr| {
            // The field slot is 16 bytes NUL-padded; `save_write` truncates past 15 chars.
            // Clip here too so the inspector shows what will actually land on disk.
            s.truncate(15);
            pr.active_contract = s;
        }),
        Act::SetActiveMissionState(id, state) => {
            mutate_pending_lua(p, |root| set_active_mission_state(root, &id, state))
        }
        Act::EnsureHeroRender(hash, name) => {
            // A previously-failed load always suppresses: the model can't be loaded; retrying
            // would just thrash the WAD stack and keep the panel locked on "render failed".
            if p.hero_render_failed.contains_key(&hash) {
                return;
            }
            // The pending slot is "the single frame we want next". For drag-orbit, each new
            // request overwrites the previous one so the renderer always targets the latest
            // `hero_cam[hash]` (an in-flight render is for a strictly older cam). The
            // post-reducer in app.rs clears this slot the frame it fires the render.
            p.pending_hero_render = Some((hash, name));
        }
        Act::SetHeroCam(hash, cam) => {
            // Camera state is persisted immediately and separately from the render request so
            // that an in-flight render (one frame behind) doesn't throw away the user's drag.
            p.hero_cam.insert(hash, cam);
        }
        Act::RevealFolder => {
            if let Some(dir) = &p.saves_dir {
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(dir).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(dir).spawn();
                #[cfg(all(unix, not(target_os = "macos")))]
                let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
            }
        }
        Act::OpenFilePicker => {
            let mut d = rfd::FileDialog::new()
                .set_title("Pick a .profile")
                .add_filter("Mercenaries 2 profile", &["profile"]);
            if let Some(dir) = &p.saves_dir {
                d = d.set_directory(dir);
            }
            if let Some(path) = d.pick_file() {
                p.selected = Some(path);
                p.repopulate_pending_from_selection();
            }
        }
    }
}

/// Small helper so the `Act::Set*` arms don't each have to open-code the `Option` dance.
/// `apply` runs after `repopulate_pending_from_selection`, so `pending` is populated for
/// every edit except the frame right after a parse failure — in which case we silently
/// drop the edit rather than resurrect a working copy for a Profile we can't write.
fn mutate_pending(p: &mut Panel, f: impl FnOnce(&mut Profile)) {
    if let Some(pr) = p.pending.as_mut() {
        f(pr);
    }
}

/// Decompress pending's Lua payload, hand the AST to `edit` for surgical mutation,
/// serialise back via `save_lua::serialize`, and re-stamp the pending Profile via
/// `save_write::set_lua_payload`. On any step's failure the pending Profile is left
/// untouched and the status line names the step that failed — the UX for an unwritable
/// save is "nothing moves + you see why" rather than "silent partial update".
///
/// After a successful edit, refreshes `lua_state` so the centre's tabs reflect the new
/// `SaveState` on the next frame.
fn mutate_pending_lua(p: &mut Panel, edit: impl FnOnce(&mut Value) -> Result<(), String>) {
    let Some(pr) = p.pending.as_mut() else {
        p.status = "no save selected — nothing to edit".into();
        return;
    };
    let lua_bytes = match pr.decompress_lua() {
        Ok(b) => b,
        Err(e) => {
            p.status = format!("decompress lua: {e}");
            return;
        }
    };
    let text = match std::str::from_utf8(&lua_bytes) {
        Ok(s) => s,
        Err(e) => {
            p.status = format!("lua is not utf-8: {e}");
            return;
        }
    };
    let mut ast = match save_lua::parse_chunk(text) {
        Ok(v) => v,
        Err(e) => {
            p.status = format!("parse lua: {e}");
            return;
        }
    };
    if let Err(e) = edit(&mut ast) {
        p.status = format!("edit lua: {e}");
        return;
    }
    let serialised = save_lua::serialize(&ast);
    if let Err(e) = save_write::set_lua_payload(pr, serialised.as_bytes()) {
        p.status = format!("set payload: {e}");
        return;
    }
    // Refresh the centre's cached `SaveState` so the tabs see the new flow data on the
    // next frame. Keyed by `selected`, which hasn't changed, so no reparse of the browser
    // row — just this one `SaveState` walk.
    p.repopulate_lua_state_from_selection();
}

/// Walk `root.tFlowData.tActiveMissions[<mission_id>].nState` and set it to `new`.
/// Errors with a readable message when any step in the chain is missing or the wrong type —
/// the Lua surface is well-shaped on real saves, but a Shipment-provided mod can produce
/// surprising structures and the Workshop should name what it found instead of panicking.
fn set_active_mission_state(root: &mut Value, mission_id: &str, new: i32) -> Result<(), String> {
    let flow = root
        .as_table_mut()
        .map_err(|e| e.to_string())?
        .get_mut("tFlowData")
        .ok_or_else(|| "root has no tFlowData".to_string())?
        .as_table_mut()
        .map_err(|e| e.to_string())?;
    let active = flow
        .get_mut("tActiveMissions")
        .ok_or_else(|| "tFlowData has no tActiveMissions".to_string())?
        .as_table_mut()
        .map_err(|e| e.to_string())?;
    let mission = active
        .get_mut(mission_id)
        .ok_or_else(|| format!("tActiveMissions has no entry `{mission_id}`"))?
        .as_table_mut()
        .map_err(|e| e.to_string())?;
    match mission.get_mut("nState") {
        Some(Value::Number(n)) => {
            n.value = new as f64;
            n.source = Some(format!("{new}.000000"));
            Ok(())
        }
        Some(other) => Err(format!(
            "{mission_id}.nState is a {:?}, not a number",
            other.kind()
        )),
        None => Err(format!("{mission_id} has no nState")),
    }
}

#[cfg(test)]
mod tests {
    use super::{hms, money};

    #[test]
    fn money_formats() {
        assert_eq!(money(0), "$0");
        assert_eq!(money(999), "$999");
        assert_eq!(money(50_000), "$50K");
        assert_eq!(money(200_000), "$200K");
        assert_eq!(money(2_545_064), "$2.5M");
        assert_eq!(money(2_700_000), "$2.7M");
        assert_eq!(money(322_684_096), "$322M");
    }

    #[test]
    fn hms_zero_pads() {
        assert_eq!(hms(300), "00h05m");
        assert_eq!(hms(24_266), "06h44m");
        assert_eq!(hms(246_334), "68h25m");
    }
}
