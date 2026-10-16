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
use mercs2_formats::save::{self, Profile};

use crate::gui::theme;

/// A single intent emitted by one of the panels, applied by [`apply`].
#[derive(Clone, Debug)]
pub enum Act {
    /// Point the panel at a specific `.profile` on disk.
    Select(PathBuf),
    /// Re-enumerate the saves folder and reparse every `.profile` header.
    Reload,
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
    /// A short free-text banner surfaced in the browser header — the last reload's outcome,
    /// e.g. `"8 profiles found"` or `"saves folder not found (MERCS2_SAVES_DIR unset)"`.
    pub status: String,
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

/// The centre column: the viewport card. The hero model and the at-a-glance HUD go here
/// once the viewport wiring is scoped against `has_viewport`.
pub fn center(_ctx: &egui::Context, _p: &Panel) -> Vec<Act> {
    Vec::new()
}

/// The right-side inspector: the scalar editor (character / upgrade / cash / fuel / contract).
///
/// Scaffold — the next commits bring the real fields in, each wired to a `save_write` edit.
pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let acts: Vec<Act> = Vec::new();
    theme::eyebrow(ui, "SAVE");
    match &p.selected {
        None => {
            ui.label(
                egui::RichText::new("Pick a save on the left to inspect it.")
                    .size(11.5)
                    .color(theme::FAINT),
            );
        }
        Some(path) => {
            ui.label(
                egui::RichText::new(path.file_name().and_then(|s| s.to_str()).unwrap_or("?"))
                    .monospace()
                    .size(12.0),
            );
            ui.label(
                egui::RichText::new("Scalar editor lands in the next commit.")
                    .size(11.5)
                    .color(theme::FAINT),
            );
        }
    }
    acts
}

/// The top-row verbs: Reload / Back up / Write to .profile.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if ui.button("Reload from disk").clicked() {
        acts.push(Act::Reload);
    }
    ui.add_enabled(false, egui::Button::new("Back up"));
    ui.add_enabled(p.selected.is_some(), egui::Button::new("Write to .profile"));
    acts
}

/// Reduce the acts the four panels emit onto [`Panel`] state.
///
/// Mirrors [`crate::quartermaster::apply`]. Panel-local decisions live here; anything that
/// crosses workbenches (switching rails, opening a Shipment, etc.) goes back out to the
/// caller via a separate `Act` variant and is handled in `app.rs`.
pub fn apply(act: Act, p: &mut Panel) {
    match act {
        Act::Select(path) => p.selected = Some(path),
        Act::Reload => p.refresh(),
    }
}
