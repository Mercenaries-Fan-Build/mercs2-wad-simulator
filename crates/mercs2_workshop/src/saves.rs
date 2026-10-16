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
//!
//! # First-wins scope
//!
//! This commit is deliberately a scaffold: the panels render their empty states and the
//! reducer is a no-op. The subsequent commits bring the browser, the scalar inspector,
//! the write path, and the mission-state round-trip (via `save_lua::parse_chunk` →
//! surgical edits → `save_lua::serialize` → `save_write::write_profile`, the exact
//! pattern `save_rollback` demonstrates).

use std::path::PathBuf;

use crate::gui::theme;

/// A single intent emitted by one of the panels, applied by [`apply`].
#[derive(Clone, Debug)]
pub enum Act {
    /// Point the panel at a specific `.profile` on disk. Filled in once the browser lands.
    Select(PathBuf),
    /// Re-read the current selection from disk. Filled in once the browser lands.
    Reload,
}

/// All state the Saves page carries. Lives on the stack in `app.rs` next to the other panels.
#[derive(Default)]
pub struct Panel {
    /// The `.profile` the panel is currently showing, if any.
    pub selected: Option<PathBuf>,
}

/// The left-rail navigator: the saves browser (file list, grouped by hero).
///
/// Scaffold — the next commit replaces the empty state with a real `.profile` enumeration.
pub fn navigator(ui: &mut egui::Ui, _p: &Panel) -> Vec<Act> {
    let acts: Vec<Act> = Vec::new();
    theme::eyebrow(ui, "SAVES");
    ui.label(
        egui::RichText::new("My Games/Mercenaries 2/SaveGames/")
            .monospace()
            .size(10.5)
            .color(theme::FAINT),
    );
    ui.add_space(10.0);
    ui.label(
        egui::RichText::new("No saves loaded yet — the browser lands in the next commit.")
            .size(11.5)
            .color(theme::FAINT),
    );
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
pub fn inspector(ui: &mut egui::Ui, _p: &Panel) -> Vec<Act> {
    let acts: Vec<Act> = Vec::new();
    theme::eyebrow(ui, "SAVE");
    ui.label(
        egui::RichText::new("Pick a save on the left to inspect it.")
            .size(11.5)
            .color(theme::FAINT),
    );
    acts
}

/// The top-row verbs: Reload / Back up / Write to .profile.
pub fn verbs(ui: &mut egui::Ui, _p: &Panel) -> Vec<Act> {
    let acts: Vec<Act> = Vec::new();
    ui.add_enabled(false, egui::Button::new("Reload from disk"));
    ui.add_enabled(false, egui::Button::new("Back up"));
    ui.add_enabled(false, egui::Button::new("Write to .profile"));
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
        Act::Reload => {
            // Will reparse the current selection in the next commit.
        }
    }
}
