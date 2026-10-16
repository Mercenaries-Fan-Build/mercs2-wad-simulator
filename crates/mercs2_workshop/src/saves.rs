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
                        ui.horizontal(|ui| {
                            ui.label(theme::disp_text(
                                e.hero_letter().to_string(),
                                11.0,
                                if selected { theme::BRASS } else { theme::DIM },
                            ));
                            let name_col = if selected { theme::BRASS } else { theme::TX };
                            ui.label(
                                egui::RichText::new(&e.stem)
                                    .size(12.0)
                                    .color(name_col),
                            );
                        });
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
            ui.label(theme::disp_text(stem.to_uppercase(), 20.0, theme::TX));
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("inflated Lua · tFlowData + tLayerData")
                    .size(11.0)
                    .color(theme::FAINT),
            );
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

            // Tab strip. Counts come from the real `SaveState` so the modder sees at-a-glance
            // which lens is going to have content before clicking it.
            ui.horizontal(|ui| {
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
            ui.add_space(10.0);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match p.mission_tab {
                    MissionTab::Active => render_active(ui, &state.active_missions),
                    MissionTab::Completed => render_completed(ui, &state.completed_flow),
                    MissionTab::Culled => render_culled(ui, &state.flow_chain),
                    MissionTab::Layers => render_layers(ui, &state.layers),
                }
            });
        });
    acts
}

/// A single mission-id card for the Active tab — id + state code + targets + collected.
fn render_active(ui: &mut egui::Ui, missions: &[save::ActiveMission]) {
    if missions.is_empty() {
        ui.label(
            egui::RichText::new("No active missions in this save.")
                .size(11.5)
                .color(theme::FAINT),
        );
        return;
    }
    for m in missions {
        theme::row_chip(ui, theme::G0, theme::LINE, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::disp_text(m.id.clone(), 13.0, theme::TX));
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(
                                egui::RichText::new(format!("nState {}", m.state as i64))
                                    .monospace()
                                    .size(10.5)
                                    .color(theme::BRASS),
                            );
                        },
                    );
                });
                let targets = m
                    .targets_complete
                    .map(|n| format!("{} targets complete", n as i64))
                    .unwrap_or_else(|| "no targets ticked".to_string());
                ui.label(
                    egui::RichText::new(format!("{targets} · {} collected", m.collected.len()))
                        .size(10.5)
                        .color(theme::FAINT),
                );
            });
        });
    }
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

/// Three-way segmented control over an integer field. Returns `Some(new_value)` the frame a
/// pill was clicked. The widget mirrors `theme::pill`'s shape so this reads as the same
/// control vocabulary used elsewhere on the rail.
fn seg_pills<T: Copy + PartialEq + std::fmt::Display>(
    ui: &mut egui::Ui,
    current: T,
    options: &[T],
) -> Option<T> {
    let mut chosen: Option<T> = None;
    ui.horizontal(|ui| {
        for &o in options {
            if theme::pill(ui, &o.to_string(), o == current).clicked() {
                chosen = Some(o);
            }
        }
    });
    chosen
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

/// The right-side inspector: the scalar editor (character / upgrade / cash / fuel / contract).
///
/// Live-edit: every widget change pushes a `Set*` Act into the queue; the reducer applies to
/// `Panel::pending` (a working clone of the parsed `Profile`). The on-disk bytes don't move
/// until the "Write to .profile" verb fires `Act::Write` — which `save_write`s the pending
/// profile with a fresh `ProfileHash` and reparses to keep the browser row in sync.
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

    // Filename eyebrow — the row the browser is on, repeated so a wide inspector reads cleanly.
    ui.label(
        egui::RichText::new(&entry.stem)
            .monospace()
            .size(12.5)
            .color(theme::TX),
    );
    ui.add_space(8.0);

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
    // `pending` falls back to the on-disk header when the panel hasn't cloned a working copy
    // yet (e.g. the frame right after Select, before the reducer runs).
    let pending = p.pending.as_ref().unwrap_or(on_disk);
    let dirty = profile_differs(pending, on_disk);

    // Integrity + version + dirty strip. `hash_ok` recomputes `ProfileHash` on demand
    // (CRC-32/BZIP2 over bytes [4..] per `docs/reverse_engineer/save_serialize_code_map.md`
    // §3.2); the reader doesn't cache it.
    let (hc, ht) = if on_disk.hash_ok() {
        (theme::GOOD, "checksum ok")
    } else {
        (theme::BAD, "checksum MISMATCH")
    };
    ui.horizontal(|ui| {
        theme::status_dot(ui, ht, hc);
        if dirty {
            theme::status_dot(ui, "pending edits", theme::BRASS);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!("version {}", on_disk.version))
                    .monospace()
                    .size(10.5)
                    .color(theme::FAINT),
            );
        });
    });
    ui.add_space(10.0);

    // HERO identity. Segmented control instead of a free integer: there are only three heroes
    // and four upgrade tiers, and a wrong value either breaks the spawn template or hits the
    // "Player" fallback in `FUN_00634810` with no feedback.
    theme::eyebrow(ui, "HERO");
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("hero").color(theme::DIM).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(v) = seg_pills(ui, pending.character_index, &[1u8, 2, 3]) {
                acts.push(Act::SetCharacter(v));
            }
        });
    });
    ui.label(
        egui::RichText::new(format!(
            "{} · {}",
            pending.character_index,
            hero_name(pending.character_index)
        ))
        .size(11.0)
        .color(theme::FAINT),
    );
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("upgrade").color(theme::DIM).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(v) = seg_pills(ui, pending.upgrade_index, &[0u8, 1, 2, 3]) {
                acts.push(Act::SetUpgrade(v));
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("costumes").color(theme::DIM).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(v) = seg_pills(ui, pending.unlocked_costumes, &[1u8, 2, 3, 4, 5]) {
                acts.push(Act::SetUnlockedCostumes(v));
            }
        });
    });
    ui.add_space(10.0);

    // RESOURCES. Cash and fuel are both `u32` on disk; the one-billion cap on cash comes from
    // the economy memory (money-fuel-datatype-and-cap). Fuel is clamped against capacity at
    // write time, not here, so an interim value isn't rejected while the user types.
    theme::eyebrow(ui, "RESOURCES");
    if let Some(v) = u32_row(ui, "cash", pending.cash, 1_000_000_000) {
        acts.push(Act::SetCash(v));
    }
    if let Some(v) = u32_row(ui, "fuel", pending.fuel, pending.fuel_capacity.max(1) as u64) {
        acts.push(Act::SetFuel(v));
    }
    if let Some(v) = u32_row(ui, "fuel cap", pending.fuel_capacity as u32, u16::MAX as u64) {
        acts.push(Act::SetFuelCapacity(v.min(u16::MAX as u32) as u16));
    }
    // Active contract is 15-char ASCII (NUL-padded in the 16-byte slot). `TextEdit` with a
    // `.char_limit` keeps the user from typing a value `save_write` would truncate.
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
    ui.add_space(10.0);

    // METADATA — read-only. Playtime / timestamp / slot name / size are reader-exposed facts;
    // the write path doesn't touch them (the engine stamps playtime + timestamp itself).
    theme::eyebrow(ui, "METADATA");
    theme::kv(
        ui,
        "playtime",
        egui::RichText::new(hms(pending.play_time_seconds)),
    );
    theme::kv(
        ui,
        "timestamp",
        egui::RichText::new(timestamp_utc(pending.timestamp)),
    );
    theme::kv(
        ui,
        "slot name",
        egui::RichText::new(pending.save_name().to_string()),
    );
    theme::kv(
        ui,
        "size",
        egui::RichText::new(format!("{} B", on_disk.raw_bytes().len())),
    );
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

/// The top-row verbs: Reload / Discard / Write.
///
/// "Write to .profile" is only live when there is a selection AND pending edits actually
/// differ from the on-disk header — a disabled button for a clean save tells the user they
/// have nothing queued, without them having to compare the inspector by eye.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if ui.button("Reload from disk").clicked() {
        acts.push(Act::Reload);
    }
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
