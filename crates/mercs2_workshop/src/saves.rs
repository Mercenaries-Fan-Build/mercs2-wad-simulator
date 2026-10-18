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
use mercs2_formats::save_lua::{self, Value};
use mercs2_formats::save_write;

use crate::gui::theme;
use crate::save_flow::{self, ContractState, Outcome};

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
    /// Render the hero viewport in `hero_outfits(hero)[i]` instead of the upgrade-tier model,
    /// or back to the upgrade-tier model on `None`. Viewport-only; nothing is written.
    SetWardrobePreview(Option<usize>),
    /// Open the saves folder in the OS file manager.
    RevealFolder,
    /// Prompt for a `.profile` anywhere on disk via a native file picker.
    OpenFilePicker,
    /// Copy a `.profile` into `<saves>/Backups/`, verified by sha256.
    Backup(PathBuf),
    /// Ask to delete a `.profile`: opens the confirmation window.
    RequestDelete(PathBuf),
    /// Close the delete confirmation without deleting.
    CancelDelete,
    /// Move a `.profile` to the OS trash (the confirmation's "Move to trash").
    ConfirmDelete(PathBuf),
    /// Open the readable Inspect window for a `.profile`.
    Inspect(PathBuf),
    /// Close the Inspect window.
    CloseInspect,
    /// `@0x18` — PMC cash. Signed-int32-shaped per the money-fuel memory; we expose
    /// `u32` to match the reader and clamp to the 1-billion cap on write.
    SetCash(u32),
    /// `@0x1C` — PMC fuel. Clamped at write time to not exceed `fuel_capacity`.
    SetFuel(u32),
    /// `@0x2F8` — fuel capacity (u16). Changing it lifts/lowers the fuel ceiling.
    SetFuelCapacity(u16),
    /// `@0x2C` — active contract mission id (ASCII, max 15 chars). Truncated on write.
    SetActiveContract(String),
    /// Open the contract window for a mission id.
    InspectContract(String),
    /// Close the contract window.
    CloseContractInspect,
    /// Work out everything moving a contract to a state changes in the pending save, and show it
    /// for confirmation (`save_flow::transition`).
    PreviewContractState(String, ContractState),
    /// Write the previewed contract change into the pending save.
    ApplyContractState,
    /// Close the contract change preview without applying it.
    CancelContractState,
    /// Set `tStatsData.nCompletedToolboxes` in the pending Lua payload.
    SetCompletedToolboxes(u32),
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
    /// The hero viewport frame's size in physical pixels; `app.rs` renders the hero at this
    /// size so the render covers the whole frame.
    SetHeroViewSize(u32, u32),
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
    /// The same payload as a full Lua tree (`save_lua`), for the read-only inspector sections
    /// that show every top-level table. Refreshed alongside `lua_state`.
    pub lua_tree: Option<Result<save_lua::Value, String>>,
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
    /// WARDROBE "Active override" pick: an index into `hero_outfits(hero)` the viewport renders
    /// instead of the upgrade-tier model. Viewport-only — the costume override is a runtime
    /// field (profile object `+0x63`) whose `.profile` byte isn't located, so nothing is
    /// written. Cleared when the selection or hero changes.
    pub wardrobe_preview: Option<usize>,
    /// Hero viewport frame size in physical pixels, reported by the viewport every time it
    /// changes. `(0, 0)` until the viewport first paints.
    pub hero_view_px: (u32, u32),
    /// The game's English string table (`key_hash -> text`), loaded by `app.rs` from the open WAD
    /// stack. `None` until loaded; `Some(Err)` names why it couldn't be.
    pub english: Option<Result<HashMap<u32, String>, String>>,
    /// `worldentity` template names by key, loaded by `app.rs` from the open WAD stack — names the
    /// save's carried weapons. `None` until loaded; `Some(Err)` names why it couldn't be.
    pub templates: Option<Result<HashMap<u32, String>, String>>,
    /// The `.profile` awaiting delete confirmation.
    pub confirm_delete: Option<PathBuf>,
    /// The save the Inspect window shows, decoded once when it is opened.
    pub inspecting: Option<crate::save_inspect::Inspected>,
    /// The game's mission, reward and flow rules, run from the open WAD stack by `app.rs`.
    /// `None` until loaded; `Some(Err)` names why they couldn't be.
    pub rules: Option<Result<crate::game_rules::GameRules, String>>,
    /// The contract whose window is open.
    pub contract_inspect: Option<ContractInspect>,
    /// A contract state change awaiting confirmation.
    pub contract_change: Option<ContractChange>,
}

/// The contract window: a mission id and the flow bindings whose consequence unlocks it, worked
/// out once when the window opens.
pub struct ContractInspect {
    pub id: String,
    pub unlocked_by: Result<Vec<String>, String>,
}

/// A previewed contract state change: everything it does to the pending save.
pub struct ContractChange {
    pub id: String,
    pub to: ContractState,
    pub outcome: Result<Outcome, String>,
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
    /// The name tables that have loaded, for the read-only topic views.
    pub(crate) fn lookups(&self) -> crate::save_tables::Lookups<'_> {
        crate::save_tables::Lookups {
            english: self.english.as_ref().and_then(|r| r.as_ref().ok()),
            templates: self.templates.as_ref().and_then(|r| r.as_ref().ok()),
        }
    }

    fn repopulate_lua_state_from_selection(&mut self) {
        self.lua_state = None;
        self.lua_tree = None;
        let Some(sel) = self.selected.as_ref().cloned() else {
            return;
        };
        let Some(list) = self.profiles.as_ref() else {
            return;
        };
        let Some(entry) = list.iter().find(|e| e.path == sel) else {
            return;
        };
        // Decode the PENDING profile when there is one: Lua edits (mission states, toolboxes)
        // land there, and decoding the on-disk header would show the pre-edit values.
        let source = match (&self.pending, &entry.header) {
            (Some(p), _) => Ok(p),
            (None, Ok(h)) => Ok(h),
            (None, Err(e)) => Err(e.clone()),
        };
        let state = source.clone().and_then(|p| p.save_state());
        let tree = source.and_then(|p| {
            let bytes = p.decompress_lua()?;
            let text = std::str::from_utf8(&bytes).map_err(|e| format!("lua is not utf-8: {e}"))?;
            save_lua::parse_chunk(text).map_err(|e| format!("parse lua: {e}"))
        });
        self.lua_state = Some((sel, state));
        self.lua_tree = Some(tree);
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

    // Geometry follows the mockup (Saves.dc.html). The SidePanel's inner margins (13 left,
    // 10 right, 12 top, 0 bottom) are the mockup's header padding, so offsets below are
    // relative to the panel's content rect.
    //
    // Footer: full-width divider, 6px, then the verbs inset 10px from both panel edges with a
    // 4px gap, then 10px to the panel bottom. Allocated first via bottom_up so the list never
    // pushes them off-screen; in bottom_up the divider is added after the buttons.
    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.add_space(10.0);
        let h = 24.0;
        let gap = 4.0;
        let label = |s: &str| egui::RichText::new(s).small().color(theme::TX);
        let open_text_w = ui.fonts(|f| {
            f.layout_no_wrap("Open file…".into(), theme::font(&egui::TextStyle::Small), theme::TX)
                .size()
                .x
        });
        let open_w = open_text_w + 2.0 * 8.0 + 2.0;
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::hover());
        // Content rect starts 13px in; the mockup's verbs start 10px in.
        let left = row.left() - 3.0;
        let open_rect =
            egui::Rect::from_min_max(egui::pos2(row.right() - open_w, row.top()), row.max);
        let reveal_rect = egui::Rect::from_min_max(
            egui::pos2(left, row.top()),
            egui::pos2(open_rect.left() - gap, row.bottom()),
        );
        // Placed through a child UI: `ui.put` on the panel's own UI would grow the panel's
        // measured content by the 3px overhang, and egui's SidePanel stores that measured width
        // for the next frame, so the panel crept wider every frame and undid any drag.
        let mut verbs_ui = ui.child_ui(
            reveal_rect.union(open_rect),
            egui::Layout::left_to_right(egui::Align::Center),
            None,
        );
        let reveal = verbs_ui.add_enabled_ui(p.saves_dir.is_some(), |ui| {
            ui.put(
                reveal_rect,
                egui::Button::new(label("Reveal folder"))
                    .fill(theme::G3)
                    .stroke(egui::Stroke::new(1.0, theme::LINE2))
                    .rounding(egui::Rounding::same(4.0)),
            )
        });
        if reveal.inner.clicked() {
            acts.push(Act::RevealFolder);
        }
        if verbs_ui
            .put(
                open_rect,
                egui::Button::new(label("Open file…"))
                    .fill(theme::G2)
                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                    .rounding(egui::Rounding::same(4.0)),
            )
            .clicked()
        {
            acts.push(Act::OpenFilePicker);
        }
        ui.add_space(6.0);
        let (line, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
        let full = ui.clip_rect();
        ui.painter().hline(full.x_range(), line.center().y, egui::Stroke::new(1.0, theme::LINE));

        // Head content (eyebrow, path, status, list) stacks top-down from here upward.
        ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
            navigator_head_and_list(ui, p, &mut acts);
        });
    });

    acts
}

fn navigator_head_and_list(ui: &mut egui::Ui, p: &Panel, acts: &mut Vec<Act>) {
    // Header spacing per the mockup: path 2px under the title, count 12px under the path,
    // list 5px under the count.
    ui.spacing_mut().item_spacing.y = 0.0;
    ui.label(theme::disp_text("SAVES", theme::style_title(), theme::TX));
    ui.add_space(2.0);
    let path = match &p.saves_dir {
        Some(dir) => tail(dir),
        None => "resolving saves folder…".to_string(),
    };
    ui.label(egui::RichText::new(path).monospace().color(theme::FAINT));
    ui.add_space(12.0);
    ui.label(theme::disp_text(&p.status, theme::style_eyebrow(), theme::DIM));
    ui.add_space(5.0);

    match &p.profiles {
        // One-shot bootstrap: first frame the panel renders, ask the reducer to load.
        None => {
            acts.push(Act::Reload);
            ui.label(
                egui::RichText::new("Loading…").small()
                    .color(theme::FAINT),
            );
        }
        Some(entries) if entries.is_empty() => {
            ui.label(
                egui::RichText::new("No .profile files in this folder.").small()
                    .color(theme::FAINT),
            );
        }
        Some(entries) => {
            ui.spacing_mut().item_spacing.y = 4.0;
            for e in entries {
                let resp = save_row(ui, e, p.selected.as_ref() == Some(&e.path));
                if resp.clicked() {
                    acts.push(Act::Select(e.path.clone()));
                }
                resp.context_menu(|ui| save_context_menu(ui, &e.path, acts));
            }
        }
    }
}

/// The right-click menu of a save row.
fn save_context_menu(ui: &mut egui::Ui, path: &Path, acts: &mut Vec<Act>) {
    if ui.button("Inspect…").clicked() {
        acts.push(Act::Inspect(path.to_path_buf()));
        ui.close_menu();
    }
    if ui.button("Back up").clicked() {
        acts.push(Act::Backup(path.to_path_buf()));
        ui.close_menu();
    }
    if ui.button("Delete…").clicked() {
        acts.push(Act::RequestDelete(path.to_path_buf()));
        ui.close_menu();
    }
    ui.separator();
    ui.add_enabled(false, egui::Button::new("New"))
        .on_disabled_hover_text("Creating a save from scratch isn't supported yet.");
}

/// The Saves page's floating windows: the Inspect view and the delete confirmation.
pub fn windows(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    if let Some(path) = &p.confirm_delete {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        egui::Window::new("Delete save?")
            .id(egui::Id::new("saves.confirm_delete"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(format!("Move “{name}” to the trash?")).color(theme::TX));
                ui.label(egui::RichText::new(path.display().to_string()).small().color(theme::FAINT));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Move to trash").clicked() {
                        acts.push(Act::ConfirmDelete(path.clone()));
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Act::CancelDelete);
                    }
                });
            });
    }
    if let Some(ins) = &p.inspecting {
        if !crate::save_inspect::show(ctx, ins, p) {
            acts.push(Act::CloseInspect);
        }
    }
    if let Some(ci) = &p.contract_inspect {
        let mut open = true;
        egui::Window::new(format!("Contract — {}", ci.id))
            .id(egui::Id::new("saves.contract_inspect"))
            .open(&mut open)
            .default_size([460.0, 520.0])
            .vscroll(true)
            .show(ctx, |ui| contract_window(ui, ci, p));
        if !open {
            acts.push(Act::CloseContractInspect);
        }
    }
    if let Some(ch) = &p.contract_change {
        egui::Window::new(format!("{} → {}", ch.id, ch.to.label()))
            .id(egui::Id::new("saves.contract_change"))
            .collapsible(false)
            .default_size([520.0, 420.0])
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                match &ch.outcome {
                    Ok(out) => {
                        ui.label(
                            egui::RichText::new(format!("{} changes to the pending save", out.changes.len())).color(theme::TX),
                        );
                        ui.add_space(4.0);
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for line in &out.changes {
                                ui.label(egui::RichText::new(line).monospace().color(theme::DIM));
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(egui::RichText::new(e).color(theme::BAD));
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.add_enabled(ch.outcome.is_ok(), egui::Button::new("Apply")).clicked() {
                        acts.push(Act::ApplyContractState);
                    }
                    if ui.button("Cancel").clicked() {
                        acts.push(Act::CancelContractState);
                    }
                });
            });
    }
    acts
}

/// The contract window's body: what the game's mission data says about the contract, and where
/// it stands in this save.
fn contract_window(ui: &mut egui::Ui, ci: &ContractInspect, p: &Panel) {
    let (Some(Ok(rules)), Some(Ok(Value::Table(root)))) = (&p.rules, &p.lua_tree) else {
        ui.label(egui::RichText::new("Mission rules or save data not loaded.").color(theme::BAD));
        return;
    };
    let Some(m) = rules.missions.get(&ci.id) else {
        ui.label(egui::RichText::new(format!("{} is not in WifMissionData.", ci.id)).color(theme::BAD));
        return;
    };
    let title = p.lookups().text(&m.title_key()).map(str::to_string);
    ui.label(theme::disp_text(title.clone().unwrap_or_else(|| m.id.clone()), theme::style_title(), theme::TX));
    let flow = |k: &str| match root.get("tFlowData") {
        Some(Value::Table(f)) => match f.get(k) {
            Some(Value::Table(t)) => Some(t),
            _ => None,
        },
        _ => None,
    };
    let keys = flow("tMyFlowData");
    let count = keys.and_then(|k| match k.get(&m.id) {
        Some(Value::Number(n)) => Some(n.value),
        _ => None,
    });
    let fired: Vec<String> = match flow("tCulledBindings") {
        Some(t) => t
            .iter_entries()
            .filter_map(|e| match &e.value {
                Value::String(s) => Some(s.value.clone()),
                _ => None,
            })
            .collect(),
        None => Vec::new(),
    };
    let yes_no = |b: bool| if b { "yes" } else { "no" }.to_string();
    let rows: Vec<(&str, String)> = vec![
        ("State", save_flow::state_of(root, &m.id).label().to_string()),
        ("Completions", count.map(|n| (n as i64).to_string()).unwrap_or_else(|| "0".into())),
        ("Kind", match m.contract {
            Some(true) => "Contract".into(),
            Some(false) => "Job".into(),
            None => "—".into(),
        }),
        ("Faction", m.faction.clone().unwrap_or_else(|| "—".into())),
        ("Starter", match &m.starter {
            Some(s) if m.has_briefing => format!("{s} (briefing)"),
            Some(s) => s.clone(),
            None => "—".into(),
        }),
        ("Critical path", yes_no(m.critical_path)),
        ("Repeatable", yes_no(m.repeatable)),
        ("Levels", m.levels.map(|n| n.to_string()).unwrap_or_else(|| "—".into())),
        ("Title key", m.title_key()),
    ];
    egui::Grid::new("contract.facts").num_columns(2).spacing([12.0, 3.0]).striped(true).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(egui::RichText::new(k).color(theme::DIM));
            ui.label(egui::RichText::new(v).monospace().color(theme::TX));
            ui.end_row();
        }
    });

    let section = |ui: &mut egui::Ui, name: &str| {
        ui.add_space(10.0);
        ui.label(theme::disp_text(name, theme::style_eyebrow(), theme::FAINT));
    };

    section(ui, "UNLOCKED BY");
    match &ci.unlocked_by {
        Ok(v) if v.is_empty() => {
            ui.label(egui::RichText::new("No flow binding unlocks it.").color(theme::DIM));
        }
        Ok(v) => {
            for b in v {
                let state = if fired.contains(b) { "fired" } else { "not fired" };
                ui.label(egui::RichText::new(format!("{b}  ({state})")).monospace().color(theme::TX));
            }
        }
        Err(e) => {
            ui.label(egui::RichText::new(e).color(theme::BAD));
        }
    }

    if !m.milestones.is_empty() {
        section(ui, "MILESTONES");
        egui::Grid::new("contract.milestones").num_columns(3).spacing([12.0, 2.0]).show(ui, |ui| {
            for ms in &m.milestones {
                let held = keys.is_some_and(|k| k.get(&ms.key).is_some());
                ui.label(egui::RichText::new(&ms.key).monospace().color(theme::TX));
                ui.label(egui::RichText::new(format!("at {}", ms.at)).color(theme::DIM));
                ui.label(egui::RichText::new(if held { "held" } else { "—" }).color(if held { theme::GOOD } else { theme::FAINT }));
                ui.end_row();
            }
        });
    }

    let reward_keys: Vec<&str> = std::iter::once(m.id.as_str()).chain(m.milestones.iter().map(|ms| ms.key.as_str())).collect();
    let rewarded: Vec<(&str, &crate::game_rules::Reward)> =
        reward_keys.iter().filter_map(|k| rules.rewards.get(*k).map(|r| (*k, r))).collect();
    if !rewarded.is_empty() {
        section(ui, "REWARDS");
        for (k, r) in rewarded {
            let mut parts = Vec::new();
            if let Some(c) = r.cash {
                parts.push(format!("${}", thousands(c as u64)));
            }
            if let Some(f) = r.fuel {
                parts.push(format!("{f} fuel"));
            }
            for (f, n) in &r.attitude {
                parts.push(format!("{f} {n:+}"));
            }
            for (id, f) in &r.support {
                parts.push(format!("{id} @ {f}"));
            }
            for (id, f) in &r.equipment {
                parts.push(format!("{id} ({f})"));
            }
            for (id, q) in &r.stockpile {
                parts.push(format!("{q} × {id}"));
            }
            ui.label(egui::RichText::new(format!("{k}: {}", parts.join(" · "))).monospace().color(theme::TX));
        }
    }

    section(ui, "WORLD LAYERS");
    let loaded: Vec<String> = match root.get("tLayerData") {
        Some(Value::Table(t)) => t
            .iter_entries()
            .filter_map(|e| match &e.value {
                Value::String(s) => Some(s.value.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    for l in m.world_layers() {
        let on = loaded.contains(&l.to_lowercase());
        ui.label(
            egui::RichText::new(format!("{l}  {}", if on { "(in this save)" } else { "(not in this save)" }))
                .monospace()
                .color(if on { theme::TX } else { theme::FAINT }),
        );
    }

    if let Some(entry) = flow("tActiveMissions").and_then(|a| a.get(&m.id)) {
        section(ui, "ACTIVE ENTRY");
        let mut text = save_lua::serialize(entry);
        ui.add(egui::TextEdit::multiline(&mut text).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).interactive(false));
    }
}

/// Copy `src` into `backups` as `<stem> <YYYY-MM-DD HH-MM-SS UTC>.profile` (a ` (n)` suffix
/// when that name is taken), never overwriting, and verify the copy's sha256 against the source.
fn backup_profile(src: &Path, backups: &Path, now_unix: i64) -> Result<PathBuf, String> {
    let bytes = std::fs::read(src).map_err(|e| format!("read {}: {e}", src.display()))?;
    let want = crate::publish::sha256_hex(&bytes);
    std::fs::create_dir_all(backups).map_err(|e| format!("{}: {e}", backups.display()))?;
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "save".into());
    let stamp = backup_stamp(now_unix);
    let mut out = backups.join(format!("{stem} {stamp}.profile"));
    let mut n = 2;
    while out.exists() {
        out = backups.join(format!("{stem} {stamp} ({n}).profile"));
        n += 1;
    }
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&out)
            .map_err(|e| format!("{}: {e}", out.display()))?;
        f.write_all(&bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    let got = crate::publish::sha256_hex(&std::fs::read(&out).map_err(|e| format!("{}: {e}", out.display()))?);
    if got != want {
        return Err(format!("{}: copy sha256 {got} != source {want}", out.display()));
    }
    Ok(out)
}

/// One flat browser row: hero badge + stem, meta line below. Mockup geometry: the list sits
/// 7px from the panel's left edge and 4px from its right (the content rect is 13/10 in, so the
/// row box bleeds 6px out on both sides), rows pad 6px top/bottom and 8px left/right, with no
/// gap between rows. Selected rows get a brass wash + 3px accent bar, unreadable rows a BAD bar.
fn save_row(ui: &mut egui::Ui, e: &ProfileEntry, selected: bool) -> egui::Response {
    const BLEED: f32 = 6.0;
    const PAD_X: f32 = 8.0;
    const PAD_Y: f32 = 6.0;
    // Background slots reserved first so they paint under the content; filled once the row's
    // measured height and hover state are known.
    let bg_slot = ui.painter().add(egui::Shape::Noop);
    let bar_slot = ui.painter().add(egui::Shape::Noop);
    let top = ui.cursor().min;
    let width = ui.available_width();
    let content_max = egui::Rect::from_min_max(
        egui::pos2(top.x - BLEED + PAD_X, top.y + PAD_Y),
        egui::pos2(top.x + width + BLEED - PAD_X, top.y + PAD_Y + 1000.0),
    );
    let mut child = ui.child_ui(content_max, egui::Layout::top_down(egui::Align::LEFT), None);
    child.spacing_mut().item_spacing.y = 2.0;
    // horizontal() otherwise pads its row to interact_size.y and centres the text in it,
    // which pushes the name line down and makes the top padding look heavier.
    child.spacing_mut().interact_size.y = 0.0;
    // Selectable labels would take the press for text selection and the row never sees a click.
    child.style_mut().interaction.selectable_labels = false;
    child.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let badge_fg = if selected { theme::BRASS } else { theme::DIM };
        egui::Frame::none()
            .stroke(egui::Stroke::new(1.0, theme::LINE2))
            .rounding(egui::Rounding::same(2.0))
            .inner_margin(egui::Margin::symmetric(4.0, 0.0))
            .show(ui, |ui| {
                ui.label(theme::disp_text(e.hero_letter().to_string(), theme::style_eyebrow(), badge_fg));
            });
        let name_col = if selected { theme::BRASS } else { theme::TX };
        ui.add(
            egui::Label::new(egui::RichText::new(&e.stem).color(name_col)).truncate(),
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
    child.add(
        egui::Label::new(egui::RichText::new(meta).monospace().color(theme::FAINT))
            .truncate(),
    );

    let row_h = child.min_rect().height() + 2.0 * PAD_Y;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, row_h), egui::Sense::click());
    let paint_rect = rect.expand2(egui::vec2(BLEED, 0.0));
    let fill = if selected {
        Some(theme::BRASS_SOFT)
    } else if resp.hovered() {
        Some(theme::G2)
    } else {
        None
    };
    if let Some(f) = fill {
        ui.painter().set(bg_slot, egui::Shape::rect_filled(paint_rect, egui::Rounding::ZERO, f));
    }
    let accent = match (&e.header, selected) {
        (Err(_), _) => Some(theme::BAD),
        (Ok(_), true) => Some(theme::BRASS),
        (Ok(_), false) => None,
    };
    if let Some(c) = accent {
        let bar = egui::Rect::from_min_size(paint_rect.min, egui::vec2(3.0, row_h));
        ui.painter().set(bar_slot, egui::Shape::rect_filled(bar, egui::Rounding::ZERO, c));
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
                ui.label(theme::disp_text("NO SAVE OPEN", egui::TextStyle::Heading, theme::DIM));
                ui.label(
                    egui::RichText::new("Pick a save on the left to see its mission state.").small()
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
                    .color(theme::FAINT),
            );
            let mut hero_req: HeroReq = HeroReq::default();
            match (edit_view, on_disk) {
                (None, _) => {
                    ui.label(theme::disp_text("UNREADABLE", egui::TextStyle::Heading, theme::BAD));
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
            // Before EnsureHeroRender so app.rs renders at the new size this same frame.
            if let Some((w, h)) = hero_req.view_px {
                acts.push(Act::SetHeroViewSize(w, h));
            }
            if let Some((hash, name)) = hero_req.request_render {
                acts.push(Act::EnsureHeroRender(hash, name));
            }
            ui.add_space(12.0);

            let Some((_, parsed)) = p.lua_state.as_ref() else {
                ui.label(
                    egui::RichText::new("Lua not loaded yet — one frame…").small()
                        .color(theme::FAINT),
                );
                return;
            };
            let state = match parsed {
                Ok(s) => s,
                Err(e) => {
                    ui.label(
                        egui::RichText::new(format!("Lua parse failed: {e}")).small()
                            .color(theme::BAD),
                    );
                    return;
                }
            };

            // Tab strip. Mirrors the mockup: `PROGRESSION` eyebrow on the left names the
            // scope, then the four tabs with live `SaveState` counts, then an "only changed"
            // toggle right-aligned so a modder can filter the grid once a diff baseline lands.
            ui.horizontal(|ui| {
                ui.label(theme::disp_text("PROGRESSION", theme::style_eyebrow(), theme::DIM));
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let all_count = match &p.rules {
                        Some(Ok(r)) => Some(r.missions.len()),
                        _ => None,
                    };
                    for (tab, label, count) in [
                        (MissionTab::All,          "All missions",  all_count),
                        (MissionTab::Completed,    "Completed",     Some(state.completed_flow.len())),
                        (MissionTab::Available,    "Available",     Some(state.active_missions.len())),
                        (MissionTab::Culled,       "Culled",        Some(state.flow_chain.len())),
                        (MissionTab::FlowBindings, "Flow bindings", Some(state.layers.len())),
                    ] {
                        let on = p.mission_tab == tab;
                        let text = match count {
                            Some(n) => format!("{label}  {n}"),
                            None => label.to_string(),
                        };
                        if theme::pill(ui, &text, on).clicked() {
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
            if let Some(Err(e)) = &p.rules {
                ui.label(egui::RichText::new(format!("Mission rules: {e}")).small().color(theme::BAD));
            }
            ui.add_space(10.0);

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                match p.mission_tab {
                    MissionTab::All => acts.extend(render_all_missions(ui, p)),
                    MissionTab::Completed => acts.extend(render_completed(ui, &state.completed_flow, p)),
                    MissionTab::Available => acts.extend(render_active(ui, &state.active_missions, p)),
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
    /// `Some((w, h))` when the viewport frame's pixel size differs from `Panel.hero_view_px`.
    view_px: Option<(u32, u32)>,
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
    let outfit = p
        .wardrobe_preview
        .and_then(|i| hero_outfits(h.character_index).get(i).copied());
    let model_name = match outfit {
        Some((_, model)) => model.to_string(),
        None => upgrade_model_for(h.character_index, h.upgrade_index),
    };
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
    let model_label = match outfit {
        Some((name, _)) => format!("OUTFIT {name} · {model_name}"),
        None => format!("UPGRADE {} · {}", h.upgrade_index, model_name),
    };
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
        // 1px so the frame's own border stays visible around the render.
        .inner_margin(egui::Margin::same(1.0))
        .show(ui, |ui| {
            // Pin exact height — min alone lets the Frame absorb the whole column.
            ui.set_width(ui.available_width());
            ui.set_min_height(viewport_height);
            ui.set_max_height(viewport_height);
            let frame_rect = ui.max_rect();

            // The render is made at the frame's own pixel size, so it covers the whole frame.
            let ppp = ui.ctx().pixels_per_point();
            let want_px = (
                (frame_rect.width() * ppp).round().clamp(1.0, 4096.0) as u32,
                (frame_rect.height() * ppp).round().clamp(1.0, 4096.0) as u32,
            );
            if want_px != p.hero_view_px {
                out.view_px = Some(want_px);
            }
            let stale_size = cached_tex.as_ref().is_some_and(|t| {
                let [w, h] = t.size();
                (w as u32, h as u32) != want_px
            });
            // Cold start or frame resized → request a render. The cam is read by app.rs from
            // `hero_cam[hash]` (defaulted when absent), so no SetHeroCam needed.
            if failed_err.is_none() && !in_flight && (cached_tex.is_none() || stale_size) {
                out.request_render = Some((hash, model_name.clone()));
            }

            // Whole frame is the drag/scroll region; chips allocated after this paint over it.
            let drag_response = ui.interact(
                frame_rect,
                ui.id().with(("hero_drag", hash)),
                egui::Sense::click_and_drag(),
            );

            let box_size = egui::vec2(
                (frame_rect.width() * 0.5).clamp(200.0, 360.0),
                (frame_rect.height() - 80.0).max(120.0),
            );
            let painter = ui.painter_at(frame_rect);
            match (&cached_tex, &failed_err, in_flight) {
                (Some(tex), _, _) => {
                    egui::Image::from_texture(egui::load::SizedTexture::from_handle(tex))
                        .rounding(egui::Rounding::same(5.0))
                        .paint_at(ui, frame_rect);
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
                    let box_rect = egui::Rect::from_center_size(frame_rect.center(), box_size);
                    painter.rect_filled(box_rect, egui::Rounding::same(4.0), theme::G2);
                    painter.rect_stroke(box_rect, egui::Rounding::same(4.0), egui::Stroke::new(1.0, theme::BAD));
                    painter.text(
                        box_rect.center() - egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        model_name.to_uppercase(),
                        theme::font(&egui::TextStyle::Body),
                        theme::BAD,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        "render failed",
                        theme::font(&egui::TextStyle::Small),
                        theme::BAD,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 22.0),
                        egui::Align2::CENTER_CENTER,
                        err,
                        theme::font(&egui::TextStyle::Monospace),
                        theme::FAINT,
                    );
                }
                (None, None, _) => {
                    let box_rect = egui::Rect::from_center_size(frame_rect.center(), box_size);
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
                        theme::font(&egui::TextStyle::Body),
                        theme::DIM,
                    );
                    painter.text(
                        box_rect.center() + egui::vec2(0.0, 30.0),
                        egui::Align2::CENTER_CENTER,
                        if in_flight { "rendering…" } else { "queuing render…" },
                        theme::font(&egui::TextStyle::Small),
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
            // Overlays sit 10px in from the frame edges (mockup).
            const INSET: f32 = 10.0;
            let topleft = frame_rect.min + egui::vec2(INSET, INSET);
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(topleft, egui::vec2(500.0, 30.0)),
                |ui| {
                    ui.horizontal(|ui| {
                        chip(ui, orbit_label, orbit_fg, orbit_bg, orbit_stroke);
                        chip(ui, &model_label, theme::DIM, theme::G2, theme::LINE);
                    });
                },
            );

            // Top-right: AT-A-GLANCE overlay card, sized to its content and anchored right.
            let rows = glance_rows(on_disk);
            let overlay_w = glance_card_width(ui, &rows);
            let topright =
                egui::pos2(frame_rect.right() - overlay_w - INSET, frame_rect.top() + INSET);
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(topright, egui::vec2(overlay_w, 200.0)),
                |ui| at_a_glance(ui, &rows, overlay_w),
            );

            // Bottom-left: identity chip.
            let identity_h = 24.0;
            let bottomleft = egui::pos2(
                frame_rect.left() + INSET,
                frame_rect.bottom() - identity_h - INSET,
            );
            ui.allocate_ui_at_rect(
                egui::Rect::from_min_size(
                    bottomleft,
                    egui::vec2(frame_rect.width() - 2.0 * INSET, identity_h),
                ),
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
            ui.label(theme::disp_text(label, theme::style_eyebrow(), fg));
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

/// AT-A-GLANCE card geometry, from the mockup (`Saves.dc.html:72-78`): 8px/10px padding, a
/// 68px label column, 8px column gap, 200px minimum width. Labels are `Small`, values
/// `Monospace` (both 11px).
const GLANCE_PAD_X: f32 = 10.0;
const GLANCE_PAD_Y: f32 = 8.0;
const GLANCE_LABEL_W: f32 = 68.0;
const GLANCE_COL_GAP: f32 = 8.0;
const GLANCE_MIN_W: f32 = 200.0;

/// The AT-A-GLANCE rows. Reads the on-disk `Profile` so the numbers stay stable while editing.
fn glance_rows(h: &Profile) -> [(&'static str, String); 5] {
    let ts = timestamp_utc(h.timestamp);
    [
        ("contract", h.active_contract.clone()),
        ("cash", thousands(h.cash as u64)),
        (
            "fuel",
            format!("{} / {}", thousands(h.fuel as u64), thousands(h.fuel_capacity as u64)),
        ),
        ("playtime", hms_long(h.play_time_seconds)),
        ("timestamp", ts.trim_end_matches(" UTC").to_string()),
    ]
}

/// Card width: padding + label column + gap + the widest value, at least `GLANCE_MIN_W`.
fn glance_card_width(ui: &egui::Ui, rows: &[(&'static str, String)]) -> f32 {
    let widest = ui.fonts(|f| {
        rows.iter()
            .map(|(_, v)| {
                f.layout_no_wrap(v.clone(), theme::font(&egui::TextStyle::Monospace), theme::TX)
                    .size()
                    .x
            })
            .fold(0.0, f32::max)
    });
    (2.0 * GLANCE_PAD_X + GLANCE_LABEL_W + GLANCE_COL_GAP + widest).max(GLANCE_MIN_W)
}

/// The AT-A-GLANCE HUD card in the viewport's top-right slot: title, divider, then label/value
/// rows with values left-aligned in their own column.
fn at_a_glance(ui: &mut egui::Ui, rows: &[(&'static str, String)], width: f32) {
    egui::Frame::none()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 16, 20, 217))
        .stroke(egui::Stroke::new(1.0, theme::LINE))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(GLANCE_PAD_X, GLANCE_PAD_Y))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                let inner_w = width - 2.0 * GLANCE_PAD_X;
                ui.set_width(inner_w);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                ui.spacing_mut().interact_size.y = 0.0;
                ui.style_mut().interaction.selectable_labels = false;
                ui.label(theme::disp_text("AT A GLANCE", theme::style_eyebrow(), theme::FAINT));
                ui.add_space(3.0);
                let (line, _) =
                    ui.allocate_exact_size(egui::vec2(inner_w, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(line, egui::Rounding::ZERO, theme::LINE);
                ui.add_space(5.0);
                for (label, value) in rows {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(GLANCE_LABEL_W, 0.0),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(GLANCE_LABEL_W);
                                ui.label(
                                    egui::RichText::new(*label)
                                        .small()
                                        .color(theme::FAINT),
                                );
                            },
                        );
                        ui.add_space(GLANCE_COL_GAP);
                        ui.label(
                            egui::RichText::new(value)
                                .monospace()
                                .color(theme::TX),
                        );
                    });
                }
            });
        });
}

/// `322684096` → `"322,684,096"`.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Long-form `h/m/s` playtime (`246334` → `"68h 25m 34s"`) for the AT-A-GLANCE strip, where
/// the browser row's compact `hms` form sacrifices the seconds.
pub(crate) fn hms_long(s: u32) -> String {
    let h = s / 3600;
    let m = (s % 3600) / 60;
    let sec = s % 60;
    format!("{h}h {m:02}m {sec:02}s")
}

/// The Available tab: one chip per `tActiveMissions` entry, OPEN once accepted (`nState` 1) and
/// OFFR while it waits at its briefing — the one distinction the game reads back on load
/// (`mrxmissionflow.lua:237`). Right-click a chip for the contract menu.
fn render_active(ui: &mut egui::Ui, missions: &[save::ActiveMission], p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if missions.is_empty() {
        ui.label(
            egui::RichText::new("No active missions in this save.").small()
                .color(theme::FAINT),
        );
        return acts;
    }
    let chips: Vec<Chip> = missions
        .iter()
        .map(|m| {
            let state = if m.state == 1.0 { ContractState::InProgress } else { ContractState::Offered };
            let targets_txt = m
                .targets_complete
                .map(|n| format!("{} targets complete", n as i64))
                .unwrap_or_else(|| "no targets ticked".to_string());
            let hover = format!(
                "{}\n{} · nState {} · {} · {} collected",
                mission_title(p, &m.id),
                state.label(),
                m.state as i64,
                targets_txt,
                m.collected.len(),
            );
            Chip { id: m.id.clone(), hover: Some(hover), style: state_style(state) }
        })
        .collect();
    for (id, resp) in mission_chip_grid(ui, "available", &chips) {
        contract_menu(&resp, &id, p, &mut acts);
    }
    acts
}

/// `id`'s title from the English table (`[<id>.Title]`, `WifMissionData.GetMissionTitle`), or
/// the id when the table or the key is missing.
fn mission_title(p: &Panel, id: &str) -> String {
    match p.lookups().text(&format!("[{id}.Title]")) {
        Some(t) => format!("{id} — {t}"),
        None => id.to_string(),
    }
}

/// The chip palette for a contract state.
fn state_style(s: ContractState) -> ChipStyle {
    match s {
        ContractState::Completed => {
            ChipStyle { bg: theme::G2, bd: theme::GOOD_DK, tag_bg: theme::GOOD_SOFT, tag_fg: theme::GOOD, tag: "DONE" }
        }
        ContractState::InProgress => {
            ChipStyle { bg: theme::G2, bd: theme::BRASS_DK, tag_bg: theme::BRASS_SOFT, tag_fg: theme::BRASS, tag: "OPEN" }
        }
        ContractState::Offered => {
            ChipStyle { bg: theme::G0, bd: theme::BRASS_DK, tag_bg: theme::G0, tag_fg: theme::BRASS, tag: "OFFR" }
        }
        ContractState::Locked => {
            ChipStyle { bg: theme::G0, bd: theme::LINE, tag_bg: theme::G0, tag_fg: theme::DIM, tag: "LOCK" }
        }
    }
}

/// The right-click menu on a mission chip: open the contract window, or move the contract to
/// another state. A state the save can't move to from here shows disabled, with the reason.
fn contract_menu(resp: &egui::Response, id: &str, p: &Panel, acts: &mut Vec<Act>) {
    let (Some(Ok(rules)), Some(Ok(Value::Table(root)))) = (&p.rules, &p.lua_tree) else {
        return;
    };
    if !rules.missions.contains_key(id) {
        return;
    }
    resp.context_menu(|ui| {
        if ui.button("Inspect contract…").clicked() {
            acts.push(Act::InspectContract(id.to_string()));
            ui.close_menu();
        }
        ui.separator();
        ui.label(theme::disp_text("SET STATE", theme::style_eyebrow(), theme::FAINT));
        for s in ContractState::ALL {
            match save_flow::unavailable(rules, root, id, s) {
                None => {
                    if ui.button(s.label()).clicked() {
                        acts.push(Act::PreviewContractState(id.to_string(), s));
                        ui.close_menu();
                    }
                }
                Some(why) => {
                    ui.add_enabled(false, egui::Button::new(s.label())).on_disabled_hover_text(why);
                }
            }
        }
    });
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

/// Narrowest a mission chip gets before the grid drops a column.
const CHIP_MIN_W: f32 = 150.0;

/// Columns of mission chips that fit `avail` px at `CHIP_MIN_W` each, between 2 and 5.
fn chip_columns(avail: f32, gutter: f32) -> usize {
    (((avail + gutter) / (CHIP_MIN_W + gutter)).floor() as usize).clamp(2, 5)
}

/// One chip in a PROGRESSION grid: id, hover text, palette.
struct Chip {
    id: String,
    hover: Option<String>,
    style: ChipStyle,
}

/// Shared chip grid for every PROGRESSION tab: id left, state-tag pill right. Returns each chip's
/// id with its response, for the callers that attach the contract menu.
fn mission_chip_grid(ui: &mut egui::Ui, salt: &str, items: &[Chip]) -> Vec<(String, egui::Response)> {
    let gutter = 4.0;
    // 24 px: ScrollArea vertical scrollbar + grid trailing spacing.
    let avail = (ui.available_width() - 24.0).max(100.0);
    let cols = chip_columns(avail, gutter);
    let cell_w = (avail - (cols as f32 - 1.0) * gutter) / cols as f32;
    // 16 = chip Frame inner_margin (7+7) + stroke (1+1).
    let row_inner = (cell_w - 16.0).max(40.0);
    let mut out = Vec::with_capacity(items.len());
    egui::Grid::new(ui.auto_id_with(("mission_chip_grid", items.len(), salt)))
        .num_columns(cols)
        .min_col_width(cell_w)
        .max_col_width(cell_w)
        .spacing(egui::vec2(gutter, gutter))
        .show(ui, |ui| {
            for (i, Chip { id, hover, style: s }) in items.iter().enumerate() {
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
                                            ui.label(theme::disp_text(s.tag, theme::style_eyebrow(), s.tag_fg));
                                        });
                                    // Claim the full remaining width with a left_to_right child so
                                    // the id starts at the chip's left edge (add_sized centres).
                                    let remaining = ui.available_width();
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(remaining, 14.0),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(id)
                                                        .monospace()
                                                        .color(theme::TX),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                },
                            );
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                let resp = match hover {
                    Some(h) => resp.on_hover_text(h),
                    None => resp,
                };
                out.push((id.clone(), resp));
                if (i + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
    out
}

/// `tMyFlowData` chips — every key the flow has awarded, with its count. Mission keys carry the
/// contract menu.
fn render_completed(ui: &mut egui::Ui, flow: &std::collections::BTreeMap<String, f64>, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    if flow.is_empty() {
        ui.label(
            egui::RichText::new("No completed-flow keys recorded.").small()
                .color(theme::FAINT),
        );
        return acts;
    }
    let chips: Vec<Chip> = flow
        .iter()
        .map(|(k, v)| Chip {
            id: k.clone(),
            hover: Some(format!("{}\nvalue {}", mission_title(p, k), *v as i64)),
            style: state_style(ContractState::Completed),
        })
        .collect();
    for (id, resp) in mission_chip_grid(ui, "completed", &chips) {
        contract_menu(&resp, &id, p, &mut acts);
    }
    acts
}

/// `tCulledBindings` chips — the flow bindings that have fired, in firing order.
fn render_culled(ui: &mut egui::Ui, chain: &[String]) {
    if chain.is_empty() {
        ui.label(
            egui::RichText::new("No culled bindings — this save hasn't trimmed any from the flow.").small()
                .color(theme::FAINT),
        );
        return;
    }
    let chips: Vec<Chip> = chain
        .iter()
        .enumerate()
        .map(|(i, k)| Chip {
            id: k.clone(),
            hover: Some(format!("#{i} {k}\nfired, culled from the flow")),
            style: ChipStyle { bg: theme::G0, bd: theme::LINE, tag_bg: theme::G0, tag_fg: theme::DIM, tag: "BIND" },
        })
        .collect();
    mission_chip_grid(ui, "culled", &chips);
}

/// `tLayerData` chips — the active `vz_state_*` world-overlay set the streamer restores. Dim
/// LAYR tag so a layer chip reads as neutral metadata, not a completion state.
fn render_layers(ui: &mut egui::Ui, layers: &[String]) {
    if layers.is_empty() {
        ui.label(
            egui::RichText::new("No world overlays in this save.").small()
                .color(theme::FAINT),
        );
        return;
    }
    let chips: Vec<Chip> = layers
        .iter()
        .map(|l| Chip {
            id: l.clone(),
            hover: Some(format!("{l}\nworld overlay restored on load")),
            style: ChipStyle { bg: theme::G0, bd: theme::LINE, tag_bg: theme::G0, tag_fg: theme::DIM, tag: "LAYR" },
        })
        .collect();
    mission_chip_grid(ui, "layers", &chips);
}

/// The "All missions" tab — every mission in `WifMissionData`, grouped by its state in this save
/// (completed, in progress, offered, not unlocked). Right-click a chip for the contract menu.
fn render_all_missions(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let rules = match &p.rules {
        None => {
            ui.label(egui::RichText::new("Loading the game's mission rules…").small().color(theme::FAINT));
            return acts;
        }
        // `center` shows the load error above the tabs.
        Some(Err(_)) => return acts,
        Some(Ok(r)) => r,
    };
    let Some(Ok(Value::Table(root))) = &p.lua_tree else {
        return acts;
    };
    for s in ContractState::ALL {
        let chips: Vec<Chip> = rules
            .missions
            .keys()
            .filter(|id| save_flow::state_of(root, id) == s)
            .map(|id| Chip { id: id.clone(), hover: Some(format!("{}\n{}", mission_title(p, id), s.label())), style: state_style(s) })
            .collect();
        if chips.is_empty() {
            continue;
        }
        ui.add_space(4.0);
        ui.label(theme::disp_text(s.label().to_uppercase(), theme::style_eyebrow(), theme::FAINT));
        ui.add_space(2.0);
        for (id, resp) in mission_chip_grid(ui, s.label(), &chips) {
            contract_menu(&resp, &id, p, &mut acts);
        }
    }
    acts
}

/// `1`/`2`/`3` → the hero's full name as the engine resolves it (`SHELL.SelectCharacter.*`
/// in the shell UI; see `memory/shell-menu-and-ui-pass.md`). `?` for anything else so a
/// future hero slot shows loudly rather than being silently mapped to Mattias.
pub(crate) fn hero_name(index: u8) -> &'static str {
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
pub(crate) fn timestamp_utc(t: u32) -> String {
    let (year, month, day, hour, minute, _) = civil_utc(t as i64);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// A backup filename stamp, `2026-10-17 18-42-05 UTC` — no `:`, which Windows forbids in names.
fn backup_stamp(t: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_utc(t);
    format!("{year:04}-{month:02}-{day:02} {hour:02}-{minute:02}-{second:02} UTC")
}

/// Unix seconds to UTC `(year, month, day, hour, minute, second)`.
fn civil_utc(total: i64) -> (i64, u64, u64, i64, i64, i64) {
    let (days, secs_in_day) = (total.div_euclid(86_400), total.rem_euclid(86_400));
    let hour = secs_in_day / 3600;
    let minute = (secs_in_day % 3600) / 60;
    let second = secs_in_day % 60;
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
    (year, month, day, hour, minute, second)
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
        ui.label(egui::RichText::new(key).color(theme::DIM));
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
/// (SAVE strip + HERO / WARDROBE / RESOURCES / COLLECTIBLES / METADATA).
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
            egui::RichText::new("Pick a save on the left to inspect it.").small()
                .color(theme::FAINT),
        );
        return acts;
    };

    ui.label(
        egui::RichText::new(&entry.stem)
            .monospace().text_style(theme::style_mono_body())
            .color(theme::TX),
    );
    ui.add_space(6.0);

    let on_disk = match &entry.header {
        Ok(h) => h,
        Err(e) => {
            ui.label(
                egui::RichText::new(format!("Parse failed: {e}")).small()
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
                            .color(theme::FAINT),
                    );
                });
            });
            ui.label(
                egui::RichText::new("CRC-32/BZIP2 checksum verified").small()
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
            egui::RichText::new("Playing as").small()
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

    // WARDROBE — UNLOCKED COUNT stepper over `@0x24A` + the hero's `_tOutfits` list. Clicking a
    // skin makes it the active skin and, because unlocks are a prefix count, raises the count
    // to include it. The active skin drives the viewport only: the costume override is runtime
    // field `+0x63` whose `.profile` byte isn't located. A hero with no catalogued `_tOutfits`
    // table gets a plain note instead of an empty list under a stepper of "/ 1".
    let mut costumes_edit: Option<u8> = None;
    let mut active_edit: Option<usize> = None;
    theme::section(ui, "WARDROBE", None, true, |ui| {
        let outfits = hero_outfits(pending.character_index);
        if outfits.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "No _tOutfits.<{}> catalogued yet — Mattias's table is \
                     extracted from wifpmcinterior.lua:215; the other heroes' \
                     tables live in the same file and will land in a later commit.",
                    hero_name(pending.character_index).split_whitespace().next().unwrap_or("hero").to_lowercase()
                )).small()
                .color(theme::FAINT),
            );
            theme::kv(
                ui,
                "unlocked count",
                egui::RichText::new(format!("{}", pending.unlocked_costumes)),
            );
            return;
        }
        // With no pick, the active skin is the outfit whose model the upgrade tier spawns (tiers
        // whose template isn't an outfit leave nothing highlighted).
        let default_model = upgrade_model_for(pending.character_index, pending.upgrade_index);
        let active = p
            .wardrobe_preview
            .filter(|&i| i < outfits.len())
            .or_else(|| outfits.iter().position(|(_, m)| *m == default_model));

        let cap = outfits.len() as u8;
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("UNLOCKED COUNT").small()
                    .color(theme::DIM),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("/ {cap}"))
                        .monospace()
                        .color(theme::FAINT),
                );
                if ui.small_button("+").clicked() {
                    costumes_edit = Some(pending.unlocked_costumes.saturating_add(1).min(cap));
                }
                ui.label(
                    egui::RichText::new(format!("{}", pending.unlocked_costumes))
                        .monospace().text_style(theme::style_mono_body()),
                );
                // A picked skin stays unlocked: the count can't drop below it.
                let floor = p
                    .wardrobe_preview
                    .filter(|&i| i < outfits.len())
                    .map_or(1, |i| i as u8 + 1);
                if ui.small_button("−").clicked() {
                    costumes_edit = Some(pending.unlocked_costumes.saturating_sub(1).max(floor));
                }
            });
        });
        ui.add_space(6.0);
        for (i, (name, model)) in outfits.iter().enumerate() {
            let unlocked = (i as u8) < pending.unlocked_costumes;
            let is_active = active == Some(i);
            let (fill, border, dot_fg, name_fg) = match (is_active, unlocked) {
                (true, _) => (theme::BRASS_SOFT, theme::BRASS_DK, theme::BRASS, theme::BRASS),
                (false, true) => (theme::G0, theme::LINE, theme::BRASS, theme::TX),
                (false, false) => (theme::G0, theme::LINE2, theme::FAINT, theme::DIM),
            };
            let resp = theme::row_chip(ui, fill, border, |ui| {
                // Selectable labels would take the press and the row would never see a click.
                ui.style_mut().interaction.selectable_labels = false;
                ui.label(theme::disp_text(if unlocked { "●" } else { "○" }, theme::style_eyebrow(), dot_fg));
                ui.label(egui::RichText::new(*name).small().color(name_fg));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(*model)
                            .monospace()
                            .color(theme::FAINT),
                    );
                });
            });
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !is_active {
                active_edit = Some(i);
                if !unlocked {
                    costumes_edit = Some(i as u8 + 1);
                }
            }
        }
    });
    if let Some(v) = costumes_edit {
        acts.push(Act::SetUnlockedCostumes(v));
    }
    if let Some(i) = active_edit {
        acts.push(Act::SetWardrobePreview(Some(i)));
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
            ui.label(egui::RichText::new("active").color(theme::DIM));
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
    // COLLECTIBLES — the toolbox counter (`tStatsData.nCompletedToolboxes` of 100), plus any
    // collect-job GUIDs an in-progress mission is holding (`tActiveMissions[id].tCollected`).
    let total = save::TOTAL_TOOLBOXES;
    theme::section(ui, "COLLECTIBLES", Some("tStatsData"), true, |ui| {
        let Some(s) = state else {
            ui.label(
                egui::RichText::new("Lua not loaded — pick a save to populate.").small()
                    .color(theme::FAINT),
            );
            return;
        };
        match s.completed_toolboxes {
            None => {
                ui.label(
                    egui::RichText::new("This save has no tStatsData.nCompletedToolboxes.").small()
                        .color(theme::FAINT),
                );
            }
            Some(n) => {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Toolboxes").color(theme::DIM));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Only raises: the counter is uncapped and a save can hold more than 100.
                        if ui
                            .add_enabled(n < total, egui::Button::new("Set all").small())
                            .on_hover_text(format!("Set the counter to {total}"))
                            .clicked()
                        {
                            acts.push(Act::SetCompletedToolboxes(total));
                        }
                        ui.label(
                            egui::RichText::new(format!("of {total}")).small()
                                .color(theme::FAINT),
                        );
                        let mut v = n as i64;
                        if ui
                            .add(egui::DragValue::new(&mut v).range(0..=9_999).speed(1.0))
                            .changed()
                        {
                            acts.push(Act::SetCompletedToolboxes(v.clamp(0, 9_999) as u32));
                        }
                    });
                });
            }
        }
        let held: Vec<(&str, usize)> = s
            .active_missions
            .iter()
            .filter(|m| !m.collected.is_empty())
            .map(|m| (m.id.as_str(), m.collected.len()))
            .collect();
        for (id, k) in held {
            theme::kv(
                ui,
                &format!("{id} · picked up"),
                egui::RichText::new(format!("{k}")).monospace(),
            );
        }
    });

    // Read-only topics, shared with the Inspect window (save_tables.rs).
    lookup_errors(ui, p);
    match &p.lua_tree {
        Some(Ok(tree)) => match tree.as_table() {
            Ok(root) => crate::save_tables::panel_sections(
                ui,
                &crate::save_tables::Ctx { root, profile: Some(pending), lk: p.lookups() },
            ),
            Err(e) => {
                ui.label(egui::RichText::new(format!("Save Lua is not a table: {e}")).small().color(theme::BAD));
            }
        },
        Some(Err(e)) => {
            ui.label(egui::RichText::new(format!("Save Lua unreadable: {e}")).small().color(theme::BAD));
        }
        None => {}
    }

    // METADATA — read-only; the engine stamps playtime and timestamp itself.
    theme::section(ui, "METADATA", None, false, |ui| {
        if let Some(s) = state {
            theme::kv(ui, "time elapsed", egui::RichText::new(hms_long(s.time_elapsed_secs as u32)));
        }
        theme::kv(ui, "playtime", egui::RichText::new(hms(pending.play_time_seconds)));
        theme::kv(ui, "timestamp", egui::RichText::new(timestamp_utc(pending.timestamp)));
        theme::kv(ui, "slot name", egui::RichText::new(pending.save_name().to_string()));
        theme::kv(ui, "size", egui::RichText::new(format!("{} B", on_disk.raw_bytes().len())));
    });

    acts
}

/// A red line for each name table that failed to load (names then show raw).
pub(crate) fn lookup_errors(ui: &mut egui::Ui, p: &Panel) {
    for (what, t) in [("display names", &p.english), ("template names", &p.templates)] {
        if let Some(Err(e)) = t {
            ui.label(egui::RichText::new(format!("No {what}: {e}")).small().color(theme::BAD));
        }
    }
}

/// True when the pending Profile differs from the on-disk one on any **editable** field, or in
/// its raw bytes (Lua-payload edits — mission states, toolboxes — rewrite `raw` via
/// `set_lua_payload` and touch no header field). Gates the Write verb.
fn profile_differs(pending: &Profile, on_disk: &Profile) -> bool {
    pending.raw_bytes() != on_disk.raw_bytes()
        || pending.character_index != on_disk.character_index
        || pending.upgrade_index != on_disk.upgrade_index
        || pending.unlocked_costumes != on_disk.unlocked_costumes
        || pending.cash != on_disk.cash
        || pending.fuel != on_disk.fuel
        || pending.fuel_capacity != on_disk.fuel_capacity
        || pending.active_contract != on_disk.active_contract
}

/// The top-row verbs: Reload / Back up / Diff vs original / Discard / Write.
///
/// Order matches the mockup. "Back up" copies the selected save into `SaveGames/Backups/`;
/// "Diff vs original" is disabled with hover text — it needs a baseline-snapshot store. "Write to .profile"
/// is only live when there is a selection AND pending edits actually differ from the on-disk
/// header, so a disabled button for a clean save tells the user they have nothing queued
/// without them having to compare the inspector by eye.
pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
    if ui.button("Reload from disk").clicked() {
        acts.push(Act::Reload);
    }
    if ui
        .add_enabled(p.selected.is_some(), egui::Button::new("Back up"))
        .on_hover_text("Copy this .profile into SaveGames/Backups/ (sha256-verified)")
        .on_disabled_hover_text("Select a save to back it up")
        .clicked()
    {
        if let Some(path) = &p.selected {
            acts.push(Act::Backup(path.clone()));
        }
    }
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
            p.wardrobe_preview = None;
            p.repopulate_pending_from_selection();
        }
        Act::Reload => p.refresh(),
        Act::PickMissionTab(t) => p.mission_tab = t,
        Act::Discard => p.repopulate_pending_from_selection(),
        Act::Write => p.write_to_disk(),
        Act::SetCharacter(v) => {
            p.wardrobe_preview = None;
            mutate_pending(p, |pr| pr.character_index = v)
        }
        Act::SetWardrobePreview(i) => p.wardrobe_preview = i,
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
        Act::InspectContract(id) => {
            let unlocked_by = match (&p.rules, &p.lua_tree) {
                (Some(Ok(rules)), Some(Ok(Value::Table(root)))) => save_flow::unlocked_by(rules, root, &id),
                (Some(Err(e)), _) => Err(format!("mission rules: {e}")),
                (None, _) => Err("mission rules not loaded".into()),
                (_, Some(Err(e))) => Err(e.clone()),
                (_, _) => Err("no save data".into()),
            };
            p.contract_inspect = Some(ContractInspect { id, unlocked_by });
        }
        Act::CloseContractInspect => p.contract_inspect = None,
        Act::PreviewContractState(id, to) => {
            let outcome = match (&p.rules, &p.lua_tree, &p.pending) {
                (Some(Ok(rules)), Some(Ok(lua)), Some(header)) => save_flow::transition(lua, header, rules, &id, to),
                (Some(Err(e)), _, _) => Err(format!("mission rules: {e}")),
                (None, _, _) => Err("mission rules not loaded".into()),
                (_, Some(Err(e)), _) => Err(e.clone()),
                (_, _, _) => Err("no save selected".into()),
            };
            p.contract_change = Some(ContractChange { id, to, outcome });
        }
        Act::CancelContractState => p.contract_change = None,
        Act::ApplyContractState => {
            let Some(ContractChange { id, to, outcome: Ok(out) }) = p.contract_change.take() else {
                p.status = "no contract change to apply".into();
                return;
            };
            let Some(pr) = p.pending.as_mut() else {
                p.status = "no save selected — nothing to edit".into();
                return;
            };
            let mut next = pr.clone();
            if let Err(e) = save_write::set_lua_payload(&mut next, save_lua::serialize(&out.lua).as_bytes()) {
                p.status = format!("set payload: {e}");
                return;
            }
            next.cash = out.cash;
            next.fuel = out.fuel;
            next.unlocked_costumes = out.costumes;
            next.active_contract = out.last_mission;
            *pr = next;
            p.status = format!("{id} → {}: {} changes", to.label(), out.changes.len());
            p.repopulate_lua_state_from_selection();
        }
        Act::SetCompletedToolboxes(n) => {
            mutate_pending_lua(p, |root| set_completed_toolboxes(root, n))
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
        Act::SetHeroViewSize(w, h) => p.hero_view_px = (w, h),
        Act::RevealFolder => {
            if let Some(dir) = &p.saves_dir {
                #[cfg(target_os = "macos")]
                let opener = "open";
                #[cfg(target_os = "windows")]
                let opener = "explorer";
                #[cfg(all(unix, not(target_os = "macos")))]
                let opener = "xdg-open";
                if let Err(e) = std::process::Command::new(opener).arg(dir).spawn() {
                    p.status = format!("reveal folder failed: {opener}: {e}");
                }
            }
        }
        Act::Backup(path) => {
            let Some(dir) = p.saves_dir.clone() else {
                p.status = "backup failed: the saves folder is unknown".into();
                return;
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            p.status = match backup_profile(&path, &dir.join("Backups"), now) {
                Ok(out) => format!("backed up to Backups/{}", out.file_name().unwrap_or_default().to_string_lossy()),
                Err(e) => format!("backup failed: {e}"),
            };
        }
        Act::RequestDelete(path) => p.confirm_delete = Some(path),
        Act::CancelDelete => p.confirm_delete = None,
        Act::ConfirmDelete(path) => {
            p.confirm_delete = None;
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            match trash::delete(&path) {
                Ok(()) if path.exists() => {
                    p.status = format!("delete failed: {name} is still there after moving it to the trash");
                }
                Ok(()) => {
                    if p.selected.as_ref() == Some(&path) {
                        p.selected = None;
                    }
                    if p.inspecting.as_ref().is_some_and(|i| i.path == path) {
                        p.inspecting = None;
                    }
                    p.refresh();
                    p.status = format!("moved {name} to the trash");
                }
                Err(e) => p.status = format!("delete failed: {name}: {e}"),
            }
        }
        Act::Inspect(path) => p.inspecting = Some(crate::save_inspect::Inspected::load(&path)),
        Act::CloseInspect => p.inspecting = None,
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

/// Set `root.tStatsData.nCompletedToolboxes` to `n`. Errors when the table or key is missing or
/// not a number rather than inventing either.
fn set_completed_toolboxes(root: &mut Value, n: u32) -> Result<(), String> {
    let stats = root
        .as_table_mut()
        .map_err(|e| e.to_string())?
        .get_mut("tStatsData")
        .ok_or_else(|| "root has no tStatsData".to_string())?
        .as_table_mut()
        .map_err(|e| e.to_string())?;
    match stats.get_mut("nCompletedToolboxes") {
        Some(Value::Number(num)) => {
            num.value = n as f64;
            num.source = Some(format!("{n}.000000"));
            Ok(())
        }
        Some(other) => Err(format!(
            "tStatsData.nCompletedToolboxes is a {:?}, not a number",
            other.kind()
        )),
        None => Err("tStatsData has no nCompletedToolboxes".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{hms, money, thousands};

    #[test]
    fn thousands_groups_by_three() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(700), "700");
        assert_eq!(thousands(4_945), "4,945");
        assert_eq!(thousands(200_000), "200,000");
        assert_eq!(thousands(322_684_096), "322,684,096");
    }

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

#[cfg(test)]
mod chip_grid_tests {
    use super::chip_columns;

    #[test]
    fn columns_follow_width_between_two_and_five() {
        assert_eq!(chip_columns(200.0, 4.0), 2, "never fewer than 2");
        assert_eq!(chip_columns(3.0 * 154.0 - 4.0, 4.0), 3);
        assert_eq!(chip_columns(4.0 * 154.0 - 4.0, 4.0), 4);
        assert_eq!(chip_columns(5.0 * 154.0 - 4.0, 4.0), 5);
        assert_eq!(chip_columns(2000.0, 4.0), 5, "never more than 5");
    }
}

#[cfg(test)]
mod backup_tests {
    use super::{backup_profile, backup_stamp};
    use mercs2_formats::game_paths;

    #[test]
    fn backup_copies_verifies_and_never_overwrites() {
        let src = game_paths::save_fixtures().join("Chris Jacobs_6A499ED6.profile");
        let dir = std::env::temp_dir().join(format!("mercs2-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let t = 1_792_079_680; // 2026-10-15 15:54:40 UTC
        let a = backup_profile(&src, &dir, t).expect("first backup");
        let b = backup_profile(&src, &dir, t).expect("second backup, same second");
        assert_eq!(
            a.file_name().unwrap().to_string_lossy(),
            "Chris Jacobs_6A499ED6 2026-10-15 15-54-40 UTC.profile"
        );
        assert_eq!(
            b.file_name().unwrap().to_string_lossy(),
            "Chris Jacobs_6A499ED6 2026-10-15 15-54-40 UTC (2).profile"
        );
        let want = std::fs::read(&src).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), want);
        assert_eq!(std::fs::read(&b).unwrap(), want);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backup_stamp_has_no_colons() {
        assert_eq!(backup_stamp(0), "1970-01-01 00-00-00 UTC");
        assert!(!backup_stamp(1_792_079_680).contains(':'));
    }
}

#[cfg(test)]
mod lua_edit_tests {
    use super::{apply, profile_differs, Act, Panel};
    use mercs2_formats::{game_paths, save};

    fn fixture(name: &str) -> save::Profile {
        let path = game_paths::save_fixtures().join(name);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        save::parse(&bytes).expect("fixture parses")
    }

    #[test]
    fn toolbox_edit_rewrites_the_lua_and_enables_write() {
        let on_disk = fixture("Mattias Nilsson_63430745.profile");
        assert_eq!(on_disk.save_state().unwrap().completed_toolboxes, Some(4));
        let mut p = Panel { pending: Some(on_disk.clone()), ..Default::default() };
        assert!(!profile_differs(p.pending.as_ref().unwrap(), &on_disk));

        apply(Act::SetCompletedToolboxes(100), &mut p);

        let pending = p.pending.as_ref().unwrap();
        assert_eq!(pending.save_state().unwrap().completed_toolboxes, Some(100), "status: {}", p.status);
        assert!(profile_differs(pending, &on_disk), "a Lua-only edit must enable Write");
    }

    /// The panel's displayed state follows the pending edit, not the on-disk header.
    #[test]
    fn an_edit_shows_in_the_displayed_state_and_tree() {
        let name = "Mattias Nilsson_63430745.profile";
        let path = game_paths::save_fixtures().join(name);
        let mut p = Panel {
            profiles: Some(vec![super::ProfileEntry {
                path: path.clone(),
                stem: name.trim_end_matches(".profile").to_string(),
                header: Ok(fixture(name)),
            }]),
            ..Default::default()
        };
        apply(Act::Select(path), &mut p);
        apply(Act::SetCompletedToolboxes(77), &mut p);

        let shown = p.lua_state.as_ref().unwrap().1.as_ref().unwrap();
        assert_eq!(shown.completed_toolboxes, Some(77), "status: {}", p.status);
        let tree = p.lua_tree.as_ref().unwrap().as_ref().unwrap().as_table().unwrap();
        let stats = match tree.get("tStatsData") {
            Some(mercs2_formats::save_lua::Value::Table(t)) => t,
            _ => panic!("no tStatsData"),
        };
        match stats.get("nCompletedToolboxes") {
            Some(mercs2_formats::save_lua::Value::Number(n)) => assert_eq!(n.value, 77.0),
            other => panic!("nCompletedToolboxes: {other:?}"),
        }
    }
}

#[cfg(all(test, feature = "retail"))]
mod contract_menu_retail_tests {
    use super::{apply, center, profile_differs, windows, Act, Panel};
    use crate::save_flow::ContractState;
    use mercs2_formats::{game_paths, save};

    fn panel_on(name: &str) -> (Panel, save::Profile) {
        let path = game_paths::save_fixtures().join(name);
        let on_disk = save::parse(&std::fs::read(&path).unwrap()).unwrap();
        let wad = crate::app::retail_vz_wad();
        let mut w = crate::app::WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let mut p = Panel {
            profiles: Some(vec![super::ProfileEntry {
                path: path.clone(),
                stem: name.trim_end_matches(".profile").to_string(),
                header: Ok(on_disk.clone()),
            }]),
            rules: Some(crate::game_rules::GameRules::load(&mut w)),
            english: Some(w.string_table("english")),
            ..Default::default()
        };
        apply(Act::Select(path), &mut p);
        (p, on_disk)
    }

    /// Preview then Apply writes each change into the pending save. OilCon020 pays nothing and
    /// unlocks OilCon021 (`wifmissionflow.lua:182`), which pays 25,000 (`mrxrewarddata.lua:859-860`).
    #[test]
    fn applying_previewed_completions_edits_the_pending_save() {
        let (mut p, on_disk) = panel_on("auto_6A0BE454.profile");
        for id in ["OilCon020", "OilCon021"] {
            apply(Act::PreviewContractState(id.into(), ContractState::Completed), &mut p);
            let change = p.contract_change.as_ref().expect("preview opened");
            assert!(change.outcome.is_ok(), "{id}: {:?}", change.outcome.as_ref().err());
            apply(Act::ApplyContractState, &mut p);
            assert!(p.contract_change.is_none());
            let state = p.lua_state.as_ref().unwrap().1.as_ref().unwrap();
            assert!(state.completed_flow.contains_key(id), "status: {}", p.status);
        }
        let pending = p.pending.as_ref().unwrap();
        assert_eq!(pending.cash, on_disk.cash + 25_000);
        assert!(profile_differs(pending, &on_disk));
    }

    /// The centre, the contract window and the change preview all render on a real save.
    #[test]
    fn the_contract_ui_renders() {
        let (mut p, _) = panel_on("Mattias Nilsson_63430745.profile");
        apply(Act::InspectContract("PmcCon002".into()), &mut p);
        assert!(p.contract_inspect.as_ref().unwrap().unlocked_by.as_ref().is_ok_and(|v| v == &["OilCon001_GurCon001"]));
        apply(Act::PreviewContractState("PmcCon001".into(), ContractState::Locked), &mut p);
        let ctx = egui::Context::default();
        crate::gui::theme::install(&ctx);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |c| {
                let _ = center(c, &p);
                let _ = windows(c, &p);
            });
        }
    }
}

#[cfg(test)]
mod navigator_layout_tests {
    use super::{navigator, Panel};

    /// The navigator must not report content wider than its panel: egui's SidePanel stores the
    /// measured width for the next frame, so any overhang makes the panel grow every frame.
    #[test]
    fn navigator_keeps_a_resizable_panel_at_its_width() {
        let ctx = egui::Context::default();
        crate::gui::theme::install(&ctx);
        let panel = Panel {
            saves_dir: Some(std::path::PathBuf::from("/x/My Games/Mercenaries 2/SaveGames")),
            ..Default::default()
        };
        let mut width = 0.0;
        for _ in 0..30 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                let r = egui::SidePanel::left("nav_test")
                    .resizable(true)
                    .default_width(300.0)
                    .width_range(egui::Rangef::new(220.0, 360.0))
                    .show(ctx, |ui| {
                        let _ = navigator(ui, &panel);
                    });
                width = r.response.rect.width();
            });
        }
        assert!((width - 300.0).abs() < 0.5, "panel crept to {width}");
    }
}
