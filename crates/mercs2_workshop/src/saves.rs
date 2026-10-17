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
}

/// The centre panel's current tab over `tFlowData`. Carried on the Panel so switching rails
/// keeps the modder's view of a given save sticky.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissionTab {
    /// `tFlowData.tActiveMissions` — in-progress missions (nState / nTargetsComplete / collected).
    #[default]
    Active,
    /// `tFlowData.tMyFlowData` — mission-id → flow-value flags (completed / advanced).
    Completed,
    /// `tFlowData.tCulledBindings` — bindings the flow fixpoint has removed, in order.
    Culled,
    /// `tLayerData` — active `vz_state_*` world overlays.
    Layers,
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

/// Compact money formatter — `322684096` → `$322M`, `12345` → `$12K`. Keeps the browser
/// rows a fixed width; the full value is shown in the inspector.
fn money(n: u32) -> String {
    if n >= 1_000_000 {
        format!("${}M", n / 1_000_000)
    } else if n >= 1_000 {
        format!("${}K", n / 1_000)
    } else {
        format!("${n}")
    }
}

/// `246334` seconds → `"68h25m"`. The `.profile` field is a `u32` of elapsed seconds.
fn hms(s: u32) -> String {
    let h = s / 3600;
    let m = (s % 3600) / 60;
    format!("{h}h{m:02}m")
}

/// Human-readable path tail — the last two components, slash-joined, for the browser header.
/// Keeps a long absolute path to the SaveGames folder from blowing out the panel width.
fn tail(p: &Path) -> String {
    let n: Vec<_> = p.components().rev().take(2).collect();
    n.iter()
        .rev()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The left-rail navigator: the saves browser.
///
/// On the first frame `panel.profiles` is `None` → we emit [`Act::Reload`] which populates it
/// in the next frame's reducer. From then on each row is a clickable [`theme::row_chip`]: the
/// hero letter, the file stem, and a compact meta line (`playtime · cash · upgrade N`).
pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();

    theme::eyebrow(ui, "SAVES");
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
            for e in entries {
                let selected = p.selected.as_ref() == Some(&e.path);
                let (fill, border) = match (&e.header, selected) {
                    (Ok(_), true) => (theme::BRASS_SOFT, theme::BRASS_DK),
                    (Ok(_), false) => (theme::G0, theme::LINE),
                    // There's no `BAD_SOFT`/`BAD_DK` in the palette; the darker ground + a bad
                    // border reads as "something's wrong here" without introducing new tokens.
                    (Err(_), _) => (theme::G2, theme::BAD),
                };
                let resp = theme::row_chip(ui, fill, border, |ui| {
                    ui.vertical(|ui| {
                        ui.with_layout(
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                let hero_fg = if selected { theme::BRASS } else { theme::DIM };
                                // Hero initial as a tiny bordered chip (M/C/J/?), matching the
                                // mockup's save-list badge. Drawn as a mini-frame so the letter
                                // has structure instead of floating next to the filename.
                                egui::Frame::none()
                                    .stroke(egui::Stroke::new(1.0, theme::LINE2))
                                    .rounding(egui::Rounding::same(3.0))
                                    .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                    .outer_margin(egui::Margin {
                                        right: 7.0,
                                        ..Default::default()
                                    })
                                    .show(ui, |ui| {
                                        ui.label(theme::disp_text(
                                            e.hero_letter().to_string(),
                                            9.5,
                                            hero_fg,
                                        ));
                                    });
                                let name_col = if selected { theme::BRASS } else { theme::TX };
                                ui.label(
                                    egui::RichText::new(&e.stem)
                                        .size(12.0)
                                        .color(name_col),
                                );
                            },
                        );
                        let meta = match &e.header {
                            Ok(h) => format!(
                                "{} · {} · upgrade {}",
                                hms(h.play_time_seconds),
                                money(h.cash),
                                h.upgrade_index
                            ),
                            Err(err) => format!("unreadable — {err}"),
                        };
                        ui.label(
                            egui::RichText::new(meta)
                                .monospace()
                                .size(10.0)
                                .color(theme::FAINT),
                        );
                    });
                });
                if resp.clicked() {
                    acts.push(Act::Select(e.path.clone()));
                }
            }
        }
    }

    acts
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

            // Resolve the on-disk header for the header strip. `pending` would read "as
            // currently edited" instead; here we want the baseline the modder is editing
            // against, so the AT-A-GLANCE card doesn't jump every keystroke.
            let on_disk = p
                .profiles
                .as_ref()
                .and_then(|list| list.iter().find(|e| &e.path == sel))
                .and_then(|e| e.header.as_ref().ok());

            // Filename eyebrow — the row the browser is on, repeated here so a wide centre reads cleanly.
            ui.label(
                egui::RichText::new(&stem)
                    .monospace()
                    .size(11.0)
                    .color(theme::FAINT),
            );
            match on_disk {
                None => {
                    ui.label(theme::disp_text("UNREADABLE", 20.0, theme::BAD));
                }
                Some(h) => {
                    hero_viewport(ui, h);
                }
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
                        (MissionTab::Active,    "Active",    state.active_missions.len()),
                        (MissionTab::Completed, "Completed", state.completed_flow.len()),
                        (MissionTab::Culled,    "Culled",    state.flow_chain.len()),
                        (MissionTab::Layers,    "Layers",    state.layers.len()),
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
                    MissionTab::Active => acts.extend(render_active(ui, &state.active_missions)),
                    MissionTab::Completed => render_completed(ui, &state.completed_flow),
                    MissionTab::Culled => render_culled(ui, &state.flow_chain),
                    MissionTab::Layers => render_layers(ui, &state.layers),
                }
            });
        });
    acts
}

/// The centre's viewport-styled area, matching the mockup: a tall frame with four HUD
/// overlays — ORBIT + model chip top-left, AT-A-GLANCE top-right, hero identity chip bottom-
/// left, and a centred placeholder for the 3D hero render until the Workshop renderer is
/// wired to this surface.
fn hero_viewport(ui: &mut egui::Ui, h: &Profile) {
    let viewport_height = 280.0;
    egui::Frame::none()
        .fill(theme::G0)
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(6.0))
        .inner_margin(egui::Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            ui.set_min_height(viewport_height);
            ui.set_width(ui.available_width());

            // Top row: left-side chips, right-side AT-A-GLANCE card.
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    chip(ui, "ORBIT", theme::BRASS, theme::BRASS_SOFT, theme::BRASS_DK);
                    ui.add_space(6.0);
                    let model_label = format!(
                        "UPGRADE {} · {}",
                        h.upgrade_index,
                        upgrade_model_for(h.character_index, h.upgrade_index)
                    );
                    chip(ui, &model_label, theme::DIM, theme::G2, theme::LINE);
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    at_a_glance(ui, h);
                });
            });

            ui.add_space(8.0);
            // Hero render placeholder — a centred dashed-border frame with a hint of what
            // belongs there once the Workshop renderer is wired in. The identity chip at
            // the bottom-left is sibling to this block so it hugs the viewport floor.
            ui.vertical_centered(|ui| {
                let avail = ui.available_size();
                let render_h = (avail.y - 48.0).max(120.0);
                let render_w = render_h * 0.6;
                egui::Frame::none()
                    .fill(theme::G2)
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(0x42, 0x47, 0x4f),
                    ))
                    .rounding(egui::Rounding::same(4.0))
                    .inner_margin(egui::Margin::symmetric(10.0, 10.0))
                    .show(ui, |ui| {
                        ui.set_min_size(egui::vec2(render_w, render_h));
                        ui.vertical_centered(|ui| {
                            ui.add_space(render_h * 0.4);
                            ui.label(theme::disp_text(
                                upgrade_model_for(h.character_index, h.upgrade_index).to_uppercase(),
                                13.0,
                                theme::DIM,
                            ));
                            ui.label(
                                egui::RichText::new(
                                    "hero render — Workshop viewport wiring pending",
                                )
                                .size(10.5)
                                .color(theme::FAINT),
                            );
                        });
                    });
            });

            ui.add_space(4.0);
            // Bottom-left identity chip (hugs the viewport floor).
            ui.horizontal(|ui| {
                chip(
                    ui,
                    &format!(
                        "{} · character_index {} · upgrade {}",
                        hero_name(h.character_index),
                        h.character_index,
                        h.upgrade_index
                    ),
                    theme::DIM,
                    theme::G2,
                    theme::LINE,
                );
            });
        });
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
fn at_a_glance(ui: &mut egui::Ui, h: &Profile) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 16, 20, 220))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
        .show(ui, |ui| {
            ui.set_min_width(200.0);
            ui.label(theme::disp_text("AT A GLANCE", 9.5, theme::FAINT));
            ui.add_space(3.0);
            glance_row(ui, "contract", &h.active_contract);
            glance_row(ui, "cash", &format!("{}", h.cash));
            glance_row(ui, "fuel", &format!("{} / {}", h.fuel, h.fuel_capacity));
            glance_row(ui, "playtime", &hms_long(h.play_time_seconds));
            glance_row(ui, "timestamp", &timestamp_utc(h.timestamp));
        });
}

/// One row of the AT-A-GLANCE overlay: dim label on the left, mono value right-aligned.
fn glance_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
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
/// Hover a chip to see its full state label and `nTargetsComplete` / `collected` detail. The
/// row-list that used to render per-mission with full state pills is gone; it did not scale
/// past ~a dozen missions and the mockup shows the grid.
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
    // Grid with 5 approximately-equal cells per row. Widths come from the available width so
    // the grid tracks the panel resize; each chip is a mini row_chip with a right-aligned
    // state tag.
    let cols = 5usize;
    let gutter = 6.0;
    let avail = ui.available_width();
    let cell_w = ((avail - (cols as f32 - 1.0) * gutter) / cols as f32).max(100.0);
    let mut col = 0usize;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(gutter, gutter);
        for m in missions {
            let cur = TaskState::from_f64(m.state);
            let (bg, bd, tag_bg, tag_fg, tag) = match cur {
                Some(TaskState::Completed) => (theme::G2, theme::GOOD_DK, theme::GOOD_SOFT, theme::GOOD, "CMPL"),
                Some(TaskState::Active)    => (theme::G2, theme::BRASS_DK, theme::BRASS_SOFT, theme::BRASS, "ACTV"),
                Some(TaskState::Cancelled) => (theme::G2, theme::BAD, theme::G0, theme::BAD, "CANC"),
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

/// Compact rows for `tMyFlowData` — id · value, sorted. 100s of entries so each row is slim.
fn render_completed(ui: &mut egui::Ui, flow: &std::collections::BTreeMap<String, f64>) {
    if flow.is_empty() {
        ui.label(
            egui::RichText::new("No completed-flow keys recorded.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    for (k, v) in flow {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(k)
                    .monospace()
                    .size(11.5)
                    .color(theme::TX),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", *v as i64))
                        .monospace()
                        .size(10.5)
                        .color(theme::GOOD),
                );
            });
        });
    }
}

/// `tCulledBindings` — ordered list, so render with a leading index to make the order visible.
fn render_culled(ui: &mut egui::Ui, chain: &[String]) {
    if chain.is_empty() {
        ui.label(
            egui::RichText::new("No culled bindings — this save hasn't trimmed any from the flow.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    for (i, k) in chain.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("{i:3}"))
                    .monospace()
                    .size(10.5)
                    .color(theme::FAINT),
            );
            ui.label(
                egui::RichText::new(k)
                    .monospace()
                    .size(11.5)
                    .color(theme::TX),
            );
        });
    }
}

/// `tLayerData` — the active `vz_state_*` overlays, in order. Shown as mono text; this is
/// the overlay set the streamer restores on load.
fn render_layers(ui: &mut egui::Ui, layers: &[String]) {
    if layers.is_empty() {
        ui.label(
            egui::RichText::new("No world overlays in this save.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    for l in layers {
        ui.label(
            egui::RichText::new(l)
                .monospace()
                .size(11.5)
                .color(theme::TX),
        );
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

/// `_tOutfits[hero]` — the base-game costumes a hero can switch to at the mirror. Source:
/// `docs/mercs2-luacd/src/vz/wifpmcinterior.lua:215-241` (Mattias). Chris / Jen tables live
/// elsewhere in the same file; populated here only for Mattias to start — the other two
/// return an empty slice so the WARDROBE section renders honestly rather than inventing.
fn hero_outfits(index: u8) -> &'static [(&'static str, &'static str)] {
    match index {
        1 => &[
            ("Original",    "pmc_hum_mattias"),
            ("MetalHead",   "pmc_hum_mattias_v3"),
            ("Suit",        "pmc_hum_mattias_v2"),
            ("Jacket",      "pmc_hum_mattias_v4"),
            ("ChickenSuit", "pmc_hum_mattias_chickensuit"),
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

    // STORY UPGRADE — 2×2 grid of tier cards. The template name on each card is the real
    // `_tCharacterMap[hero].templates[iUpgrade]` string the engine spawns from (or `base`
    // for tier 0), so a modder knows what their edit swaps.
    let mut upgrade_edit: Option<u8> = None;
    theme::section(ui, "STORY UPGRADE", Some("0 fresh → 3 endgame"), true, |ui| {
        let templates = hero_tier_templates(pending.character_index);
        egui::Grid::new("saves.upgrade.grid")
            .num_columns(2)
            .spacing(egui::vec2(6.0, 6.0))
            .show(ui, |ui| {
                for tier in 0u8..4 {
                    if tier_card(
                        ui,
                        tier,
                        templates[tier as usize],
                        pending.upgrade_index == tier,
                    ) {
                        upgrade_edit = Some(tier);
                    }
                    if tier == 1 {
                        ui.end_row();
                    }
                }
            });
    });
    if let Some(v) = upgrade_edit {
        acts.push(Act::SetUpgrade(v));
    }

    // WARDROBE — active override dropdown (runtime-only on disk, surfaced for a future write
    // byte at `+0x4E`) + an UNLOCKED COUNT stepper over `@0x24A` + a prefix-unlocked list.
    let mut costumes_edit: Option<u8> = None;
    theme::section(ui, "WARDROBE", None, true, |ui| {
        let outfits = hero_outfits(pending.character_index);
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

        let cap = outfits.len().max(1) as u8;
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

    // COLLECTIBLES — the GUID count in `active_missions.collected`. A total across all active
    // missions for now; per-type breakdown (toolboxes / parts / cards) needs collectible-type
    // classification from the ASET registry and is TBD.
    let collected_total: usize = p
        .lua_state
        .as_ref()
        .and_then(|(_, r)| r.as_ref().ok())
        .map(|s| s.collected_count())
        .unwrap_or(0);
    theme::section(
        ui,
        "COLLECTIBLES",
        Some(&format!("{collected_total} total")),
        true,
        |ui| {
            ui.label(
                egui::RichText::new("GUIDs in active_missions.collected (total across tCollected)")
                    .size(10.5)
                    .color(theme::FAINT),
            );
            ui.add_space(3.0);
            theme::kv(ui, "collected", egui::RichText::new(format!("{collected_total}")));
            ui.label(
                egui::RichText::new(
                    "Per-type breakdown (toolboxes / parts / cards) needs collectible-type \
                     classification from the ASET registry — TBD.",
                )
                .size(9.5)
                .color(theme::FAINT),
            );
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

/// One tier card in the STORY UPGRADE 2×2 grid. Returns true the frame the card was clicked.
/// Mirrors the mockup: "TIER N" eyebrow (brass when selected) + the real template name in
/// mono below it, brass-bordered when the card is the current tier.
fn tier_card(ui: &mut egui::Ui, tier: u8, template: &str, on: bool) -> bool {
    let (fill, border, hd, body) = if on {
        (theme::BRASS_SOFT, theme::BRASS_DK, theme::BRASS, theme::BRASS)
    } else {
        (theme::G0, theme::LINE, theme::FAINT, theme::DIM)
    };
    let resp = egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(egui::Rounding::same(4.0))
        .inner_margin(egui::Margin::symmetric(8.0, 6.0))
        .show(ui, |ui| {
            ui.set_min_width(120.0);
            ui.vertical(|ui| {
                ui.label(theme::disp_text(format!("TIER {tier}"), 9.5, hd));
                ui.label(
                    egui::RichText::new(if template.is_empty() { "—" } else { template })
                        .monospace()
                        .size(11.0)
                        .color(body),
                );
            });
        })
        .response
        .interact(egui::Sense::click());
    resp.clicked()
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
