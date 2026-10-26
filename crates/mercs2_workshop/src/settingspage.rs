//! Settings' navigator and its Game archive and Reference data sections: each chain's rows, what it
//! opened, the "saved path not found" banner, and the buttons that change a source. The page draws
//! [`crate::chains`]; its [`Act`]s are applied by `app.rs`, which owns the window state a change
//! reloads. The rules (badges, banner, what Download needs, the settings edits) are plain functions
//! so they are tested without a window.

use std::path::Path;

use crate::app::SettingsSection;
use crate::chains::{Chain, Status, DATA_ENV, FLAG, RUN, SAVED};
use crate::gui::theme;
use crate::settings::Settings;

/// Which chain a section, act or failure belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Which {
    Game,
    Data,
}

/// What a click on a section asks of the app.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    /// Pick a source, check it, save it as the setting.
    Choose(Which),
    /// Pick a source, check it, use it for this launch only.
    RunOnly(Which),
    /// Remove the saved setting.
    ClearSaved(Which),
    /// Save the source in use (from `--wad` or this run's choice) as the setting.
    SaveRunPath(Which),
    /// Fetch the reference bundle from the latest release.
    Download,
    /// Open the source in use's folder in the OS file browser.
    Reveal(Which),
}

/// A navigator row's badge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Badge {
    /// The chain has no answer (or its source failed to load): red.
    Red,
    /// A set source below the one in use is broken: amber CHECK.
    Check,
}

/// The badge for a chain: red when it has no answer or its load `failed`, amber when a set source
/// below the one in use is broken. A reference chain with nothing set and nothing found is not red:
/// the boot downloads it (`downloads`).
pub fn badge(chain: &Chain, failed: bool, downloads: bool) -> Option<Badge> {
    if failed {
        return Some(Badge::Red);
    }
    if chain.in_use().is_none() {
        return (!(downloads && chain.is_empty())).then_some(Badge::Red);
    }
    chain.attention().map(|_| Badge::Check)
}

/// The banner over a section: the source in use is for this launch only and the saved setting is
/// broken.
#[derive(Debug, PartialEq)]
pub struct Banner {
    pub title: &'static str,
    pub text: String,
}

/// The banner for `chain`, if it has one. `thing` names the saved path ("vz.wad").
pub fn banner(chain: &Chain, thing: &str) -> Option<Banner> {
    let used = chain.rows.iter().find(|r| r.status == Status::InUse)?;
    let source = match used.label.as_str() {
        FLAG => "the one passed with --wad",
        RUN => "the one chosen for this run",
        _ => return None,
    };
    let Status::Broken { tag, .. } = &chain.row(SAVED)?.status else { return None };
    let state = match *tag {
        "FILE MISSING" | "MISSING" => "no longer exists",
        "NO MANIFEST" => "is not a complete bundle",
        _ => "cannot be used",
    };
    Some(Banner {
        title: "SAVED PATH NOT FOUND",
        text: format!("The saved {thing} {state}, so this run used {source}. Nothing on disk was changed."),
    })
}

/// Whether Download is offered on the reference chain: only when no source a person sets is set.
/// Downloading over a source someone set would quietly replace what they asked for.
pub fn download_offered(data: &Chain) -> bool {
    [RUN, DATA_ENV, SAVED].iter().all(|l| data.row(l).map_or(true, |r| r.path.is_none()))
}

/// Apply an act that edits only the saved settings (Clear saved, Save this run's path) to `s`;
/// `in_use` is the chain's answer. Returns whether `s` changed.
pub fn edit_saved(s: &mut Settings, act: Act, in_use: Option<&Path>) -> bool {
    fn slot(s: &mut Settings, w: Which) -> &mut Option<std::path::PathBuf> {
        match w {
            Which::Game => &mut s.wad_path,
            Which::Data => &mut s.data_dir,
        }
    }
    match act {
        Act::ClearSaved(w) => slot(s, w).take().is_some(),
        Act::SaveRunPath(w) => match in_use {
            Some(p) => {
                *slot(s, w) = Some(p.to_path_buf());
                true
            }
            None => false,
        },
        _ => false,
    }
}

/// The navigator: the sections, each with its sub-line; the Game archive and Reference data rows
/// carry their [`Badge`]. Returns the section clicked.
pub fn navigator(ui: &mut egui::Ui, current: SettingsSection, game: Option<Badge>, data: Option<Badge>) -> Option<SettingsSection> {
    ui.label(theme::disp_text("SETTINGS", theme::style_title(), theme::TX));
    ui.add_space(4.0);
    ui.label(egui::RichText::new("The paths the tool cannot discover for itself on every platform.").small().color(theme::FAINT));
    ui.add_space(9.0);
    let mut picked = None;
    for s in SettingsSection::ALL {
        let on = current == s;
        let b = match s {
            SettingsSection::Game => game,
            SettingsSection::Data => data,
            _ => None,
        };
        let r = egui::Frame::none()
            .fill(if on { theme::BRASS_SOFT } else { egui::Color32::TRANSPARENT })
            .inner_margin(egui::Margin { left: 9.0, right: 8.0, top: 6.0, bottom: 7.0 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        let col = match (b, on) {
                            (Some(Badge::Red), _) => theme::BAD,
                            (_, true) => theme::BRASS,
                            _ => theme::TX,
                        };
                        ui.label(egui::RichText::new(s.label()).color(col));
                        ui.label(egui::RichText::new(s.blurb()).small().color(theme::FAINT));
                    });
                    if let Some(b) = b {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (text, col) = match b {
                                Badge::Red => ("SET UP", theme::BAD),
                                Badge::Check => ("CHECK", theme::BRASS),
                            };
                            egui::Frame::none()
                                .stroke(egui::Stroke::new(1.0, col))
                                .rounding(egui::Rounding::same(3.0))
                                .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                .show(ui, |ui| {
                                    ui.label(theme::disp_text(text, theme::style_eyebrow(), col));
                                });
                        });
                    }
                });
            })
            .response;
        if on {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(r.rect.min, egui::vec2(3.0, r.rect.height())),
                0.0,
                theme::BRASS,
            );
        }
        if r.interact(egui::Sense::click()).clicked() {
            picked = Some(s);
        }
    }
    picked
}

/// What a chain section draws.
pub struct View<'a> {
    pub which: Which,
    pub chain: &'a Chain,
    /// The last load of this chain's source failed.
    pub failed: bool,
    /// "WHAT IT OPENED": key → value rows, empty while nothing is loaded, and its badge.
    pub opened: Vec<(&'static str, String)>,
    pub opened_badge: Option<String>,
    /// A saved setting exists (Clear saved is enabled).
    pub saved_set: bool,
    /// Reference data only: the download's progress line while it runs.
    pub downloading: Option<&'a str>,
}

/// The tail of a path — the part that identifies it; the full path is one hover away.
fn tail(p: &Path) -> String {
    let n: Vec<_> = p.components().rev().take(3).collect();
    n.iter().rev().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

/// A Game archive or Reference data section. Returns what was clicked.
pub fn section(ui: &mut egui::Ui, v: &View<'_>) -> Vec<Act> {
    let mut acts = Vec::new();
    let w = v.which;
    let (thing, title) = match w {
        Which::Game => ("vz.wad", "Where vz.wad came from"),
        Which::Data => ("reference folder", "Where the data came from"),
    };
    if let Some(b) = banner(v.chain, thing) {
        egui::Frame::none()
            .fill(theme::G2)
            .stroke(egui::Stroke::new(1.0, theme::BRASS))
            .rounding(egui::Rounding::same(6.0))
            .inner_margin(egui::Margin::symmetric(11.0, 9.0))
            .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                    ui.painter().circle_filled(r.center(), 4.5, theme::BRASS);
                    ui.vertical(|ui| {
                        ui.set_max_width(ui.available_width() - 260.0);
                        ui.label(theme::disp_text(b.title, theme::style_eyebrow(), theme::BRASS));
                        ui.label(egui::RichText::new(&b.text).small().color(theme::DIM));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.button("Clear saved").clicked() {
                            acts.push(Act::ClearSaved(w));
                        }
                        if theme::primary_button(ui, "Save this run's path", true).clicked() {
                            acts.push(Act::SaveRunPath(w));
                        }
                    });
                });
            });
    }
    let red = badge(v.chain, v.failed, w == Which::Data) == Some(Badge::Red);
    theme::section(ui, title, Some("first match wins"), true, |ui| {
        for (i, r) in v.chain.rows.iter().enumerate() {
            let below_use = v.chain.in_use().is_some();
            let (fill, border, accent, tag) = match &r.status {
                Status::InUse => (theme::GOOD_SOFT, theme::GOOD_DK, theme::GOOD, Some("IN USE")),
                Status::Broken { tag, .. } if below_use => (egui::Color32::TRANSPARENT, theme::BRASS_DK, theme::BRASS, Some(*tag)),
                Status::Broken { tag, .. } => (egui::Color32::TRANSPARENT, theme::BAD, theme::BAD, Some(*tag)),
                _ => (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT, theme::FAINT, None),
            };
            let note = match &r.status {
                Status::NotSet => Some("not set"),
                Status::NotFound => Some("not found"),
                Status::NotReached => Some("not reached"),
                Status::NotOnPlatform => Some("not on this platform"),
                _ => None,
            };
            let resp = egui::Frame::none()
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, border))
                .rounding(egui::Rounding::same(4.0))
                .inner_margin(egui::Margin::symmetric(8.0, 5.0))
                .outer_margin(egui::Margin { bottom: 3.0, ..Default::default() })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        // A left-aligned column `w` wide (`add_sized` would centre its widget).
                        let col = |ui: &mut egui::Ui, w: f32, text: egui::RichText| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(w, ui.spacing().interact_size.y),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_width(w);
                                    ui.add(egui::Label::new(text).truncate());
                                },
                            );
                        };
                        col(ui, 20.0, egui::RichText::new((i + 1).to_string()).monospace().color(accent));
                        let lc = if tag.is_some() { theme::TX } else { theme::DIM };
                        col(ui, 190.0, egui::RichText::new(&r.label).color(lc));
                        let tag_w = 110.0;
                        let path_w = (ui.available_width() - tag_w - ui.spacing().item_spacing.x).max(40.0);
                        match (&r.path, note) {
                            (Some(p), None) => {
                                let c = if tag.is_some() { theme::TX } else { theme::DIM };
                                col(ui, path_w, egui::RichText::new(tail(p)).monospace().color(c));
                            }
                            (Some(p), Some(n)) => {
                                col(ui, path_w, egui::RichText::new(format!("{}  ·  {n}", tail(p))).monospace().color(theme::FAINT));
                            }
                            (None, Some(n)) => col(ui, path_w, egui::RichText::new(n).small().color(theme::FAINT)),
                            (None, None) => ui.add_space(path_w),
                        }
                        if let Some(t) = tag {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(theme::disp_text(t, theme::style_eyebrow(), accent));
                            });
                        }
                    });
                })
                .response;
            let hover = match (&r.path, &r.status) {
                (Some(p), Status::Broken { why, .. }) => format!("{}\n\n{why}", p.display()),
                (Some(p), _) => p.display().to_string(),
                (None, Status::Broken { why, .. }) => why.clone(),
                (None, _) => String::new(),
            };
            if !hover.is_empty() {
                resp.on_hover_text(hover);
            }
        }
        if red {
            ui.add_space(4.0);
            let why = match v.chain.blocker() {
                Some(r) => match &r.status {
                    Status::Broken { why, .. } => format!("{}: {why}", r.label),
                    _ => unreachable!("a blocker is broken"),
                },
                None => format!("No {thing} is set, and none was found."),
            };
            ui.label(egui::RichText::new(why).small().color(theme::BAD));
        }
    });
    if !v.opened.is_empty() {
        theme::section(ui, "What it opened", v.opened_badge.as_deref(), true, |ui| {
            for (k, val) in &v.opened {
                theme::kv(ui, k, egui::RichText::new(val.as_str()).color(theme::DIM));
            }
        });
    }
    ui.horizontal(|ui| {
        let choose = match w {
            Which::Game => "Choose vz.wad\u{2026}",
            Which::Data => "Choose folder\u{2026}",
        };
        if ui.button(choose).on_hover_text("Saved: used on every launch").clicked() {
            acts.push(Act::Choose(w));
        }
        if ui.button("Use for this run only\u{2026}").on_hover_text("Nothing is saved").clicked() {
            acts.push(Act::RunOnly(w));
        }
        if ui.add_enabled(v.saved_set, egui::Button::new("Clear saved")).clicked() {
            acts.push(Act::ClearSaved(w));
        }
        if w == Which::Data && download_offered(v.chain) {
            let busy = v.downloading.is_some();
            let label = if busy { "Downloading\u{2026}" } else { "Download" };
            if ui
                .add_enabled(!busy, egui::Button::new(label))
                .on_hover_text("Fetch the reference bundle published with the latest release and install it for this user")
                .clicked()
            {
                acts.push(Act::Download);
            }
        }
        if ui.add_enabled(v.chain.in_use().is_some(), egui::Button::new("Reveal folder")).clicked() {
            acts.push(Act::Reveal(w));
        }
    });
    if let Some(m) = v.downloading {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(11.0));
            ui.label(egui::RichText::new(m).color(theme::DIM).small());
        });
    }
    acts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chains::{resolve, Step};
    use std::path::PathBuf;

    /// A chain from `(label, set, usable)` steps; `usable` decides the check.
    fn chain(steps: &[(&str, Option<&str>, bool)]) -> Chain {
        let ok: Vec<String> = steps.iter().filter(|s| s.2).filter_map(|s| s.1.map(String::from)).collect();
        resolve(
            steps.iter().map(|(l, p, _)| Step::Set { label: l.to_string(), set: p.map(PathBuf::from) }).collect(),
            |p| {
                if ok.iter().any(|o| Path::new(o) == p) {
                    Ok(p.to_path_buf())
                } else {
                    Err(("FILE MISSING", format!("{} is gone", p.display())))
                }
            },
        )
    }

    #[test]
    fn the_banner_shows_when_a_launch_only_source_covers_a_missing_saved_path() {
        let c = chain(&[(FLAG, Some("/g/vz.wad"), true), (RUN, None, false), (SAVED, Some("/old/vz.wad"), false)]);
        let b = banner(&c, "vz.wad").expect("banner");
        assert_eq!(b.title, "SAVED PATH NOT FOUND");
        assert!(b.text.contains("no longer exists") && b.text.contains("--wad"), "{}", b.text);
        assert_eq!(badge(&c, false, false), Some(Badge::Check));
        // The saved path is fine: no banner. The saved path is the one in use: no banner.
        let fine = chain(&[(FLAG, Some("/g/vz.wad"), true), (SAVED, Some("/s/vz.wad"), true)]);
        assert_eq!(banner(&fine, "vz.wad"), None);
        let saved = chain(&[(FLAG, None, false), (SAVED, Some("/s/vz.wad"), true)]);
        assert_eq!(banner(&saved, "vz.wad"), None);
        assert_eq!(badge(&saved, false, false), None);
    }

    #[test]
    fn a_chain_with_no_answer_is_red_unless_it_is_the_empty_reference_chain() {
        let blocked = chain(&[(SAVED, Some("/old"), false), ("beside the exe", None, false)]);
        assert_eq!(badge(&blocked, false, false), Some(Badge::Red));
        assert_eq!(badge(&blocked, false, true), Some(Badge::Red), "a broken set source is red even for data");
        let empty = chain(&[(SAVED, None, false)]);
        assert_eq!(badge(&empty, false, false), Some(Badge::Red), "no game archive anywhere is red");
        assert_eq!(badge(&empty, false, true), None, "no reference data anywhere downloads");
        let fine = chain(&[(SAVED, Some("/s"), true)]);
        assert_eq!(badge(&fine, true, false), Some(Badge::Red), "a failed load is red");
    }

    #[test]
    fn download_is_offered_only_when_no_data_source_is_set() {
        assert!(download_offered(&chain(&[(RUN, None, false), (DATA_ENV, None, false), (SAVED, None, false)])));
        assert!(!download_offered(&chain(&[(RUN, None, false), (DATA_ENV, Some("/x"), false), (SAVED, None, false)])));
        assert!(!download_offered(&chain(&[(RUN, None, false), (DATA_ENV, None, false), (SAVED, Some("/s"), true)])));
    }

    #[test]
    fn save_this_runs_path_and_clear_saved_write_the_settings_file() {
        let dir = std::env::temp_dir().join(format!("m2ws_settingspage_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("settings.json");
        let mut s = Settings { wad_path: Some(PathBuf::from("/old/vz.wad")), data_dir: None };
        assert!(edit_saved(&mut s, Act::SaveRunPath(Which::Game), Some(Path::new("/g/vz.wad"))));
        s.save_to(&file).expect("save");
        assert_eq!(crate::settings::load_from(&file).wad_path, Some(PathBuf::from("/g/vz.wad")));
        assert!(edit_saved(&mut s, Act::ClearSaved(Which::Game), None));
        s.save_to(&file).expect("save");
        assert_eq!(crate::settings::load_from(&file).wad_path, None);
        assert!(!edit_saved(&mut s, Act::ClearSaved(Which::Game), None), "nothing left to clear");
        assert!(!edit_saved(&mut s, Act::SaveRunPath(Which::Data), None), "nothing in use to save");
    }
}
