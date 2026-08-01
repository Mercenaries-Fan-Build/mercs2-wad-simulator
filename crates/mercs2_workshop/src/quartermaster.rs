//! The Quartermaster page — where a Shipment is assembled, checked and built.
//!
//! Every domain's edits land in the same Shipment, so this is one page rather than a per-domain
//! surface: the queue on the left, the selected contribution in the middle, and the gate, the
//! linter and the build on the right.
//!
//! It is the UI front end to `mercs2_quartermaster` — the same work `qm` does, arranged so whatever
//! blocks the build is what you see first.
//!
//! # The rule the layout enforces
//!
//! **The gate is a state, not a count.** `build::build` returns `Err(BuildError::Blocked)` rather
//! than a number, and the standing mandate is that builds gate on an exit code, "never a printed
//! count". So Build is disabled while anything blocks and the strip says why, rather than offering
//! a button that refuses.
//!
//! # Colour contract
//!
//! Red is *blocking*, amber is *advisory*, green is *complete*. Nothing advisory is ever red, so a
//! red stripe anywhere on this page is work. Green means built and verified, which is why a ready
//! Shipment reads amber: it is valid, but nothing has been produced. Blue is neither — a missing
//! game install is a fact about the machine, not a defect in the Shipment. `HAZARD` stays out of
//! the ramp entirely; it marks irreversible actions, and nothing here is one.

use std::path::{Path, PathBuf};

use egui::Color32;

use mercs2_quartermaster::build::{self, BuildError, BuildReport};
use mercs2_quartermaster::discover::{self, LoadedShipment};
use mercs2_quartermaster::lint::{Diagnostic, Severity};
use mercs2_quartermaster::manifest::Contribution;
use mercs2_quartermaster::names::NameTable;

use crate::gui::theme;

/// What the page is asking of you right now.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Empty,
    /// Something at Error or above — `build` would return `Blocked`.
    Blocked,
    /// Valid, with advisories, and nothing built.
    Advisory,
    Done,
    /// Valid, but there is no game stack to build against.
    NoGame,
}

impl Gate {
    fn title(self) -> &'static str {
        match self {
            Gate::Empty => "Nothing queued",
            Gate::Blocked => "Build blocked",
            Gate::Advisory => "Ready to build",
            Gate::Done => "Built and verified",
            Gate::NoGame => "Checks only",
        }
    }
    pub fn colour(self) -> Color32 {
        match self {
            Gate::Empty => theme::FAINT,
            Gate::Blocked => theme::BAD,
            Gate::Advisory => theme::BRASS,
            Gate::Done => theme::GOOD,
            Gate::NoGame => theme::INFO,
        }
    }
}

/// Queued by the widgets, executed by [`apply`], so rendering never borrows the game stack.
pub enum Act {
    Open,
    Recheck,
    Build,
    Reveal,
    SendToModkit,
    Select(usize),
    OpenDoc(String),
}

#[derive(Default)]
pub struct Panel {
    shipment: Option<LoadedShipment>,
    diagnostics: Vec<Diagnostic>,
    report: Option<BuildReport>,
    /// Set when opening or building failed outright, as opposed to producing findings.
    error: Option<String>,
    selected: Option<usize>,
    status: String,
}

impl Panel {
    pub(crate) fn blocks(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d.severity, Severity::Error | Severity::Hang))
    }

    pub fn gate(&self, has_game: bool) -> Gate {
        if self.shipment.is_none() {
            return Gate::Empty;
        }
        if self.blocks() {
            return Gate::Blocked;
        }
        if self.report.is_some() {
            return Gate::Done;
        }
        if !has_game {
            return Gate::NoGame;
        }
        Gate::Advisory
    }

    fn counts(&self) -> (usize, usize, usize) {
        let (mut h, mut e, mut w) = (0, 0, 0);
        for d in &self.diagnostics {
            match d.severity {
                Severity::Hang => h += 1,
                Severity::Error => e += 1,
                Severity::Warning => w += 1,
                Severity::Info => {}
            }
        }
        (h, e, w)
    }

    /// How many things need doing — the rail badge, so the gate is visible from any page.
    pub fn blocking_count(&self) -> usize {
        let (h, e, _) = self.counts();
        h + e
    }

    fn gate_detail(&self, g: Gate) -> String {
        let (h, e, w) = self.counts();
        match g {
            Gate::Empty => "Open one, or export a character from the Skeleton bench.".into(),
            Gate::Blocked if h > 0 => {
                format!("{h} hang and {e} error, neither of which the game will report.")
            }
            Gate::Blocked => format!("{e} error, and the game will not say so."),
            Gate::Advisory if w > 0 => format!(
                "{w} advisor{}, and nothing built yet.",
                if w == 1 { "y" } else { "ies" }
            ),
            Gate::Advisory => "Nothing built yet.".into(),
            Gate::Done => "Rebuilds byte-identical.".into(),
            Gate::NoGame => "Checks run without a game; building needs the retail WADs.".into(),
        }
    }

    /// Point the page at a Shipment directory and check it.
    pub fn open_shipment(&mut self, root: &Path, names: Option<&NameTable>) {
        self.report = None;
        self.selected = None;
        match discover::open(root) {
            Ok(s) => {
                self.diagnostics =
                    mercs2_quartermaster::lint::lint(&s.manifest, Some(&s.root), names);
                self.status = format!(
                    "{} · {} contribution(s)",
                    s.manifest.shipment.name,
                    s.manifest.contributions.len()
                );
                self.error = None;
                self.selected = (!s.manifest.contributions.is_empty()).then_some(0);
                self.shipment = Some(s);
            }
            Err(e) => {
                self.shipment = None;
                self.diagnostics.clear();
                self.error = Some(format!("{e:?}"));
                self.status = "could not open that folder as a Shipment".into();
            }
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.shipment.as_ref().map(|s| s.root.as_path())
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    /// The findings attached to one contribution — the cross-link `Diagnostic::at` makes possible.
    fn findings_for(&self, i: usize) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.at == Some(i))
    }

    fn row_severity(&self, i: usize) -> Option<Severity> {
        self.findings_for(i).map(|d| d.severity).max()
    }

    /// The page's one-line state, for a headless run to print.
    pub fn status_line(&self, has_game: bool) -> String {
        let g = self.gate(has_game);
        format!("{} — {}", g.title(), self.gate_detail(g))
    }
}

fn sev_colour(s: Severity) -> Color32 {
    match s {
        Severity::Info => theme::INFO,
        Severity::Warning => theme::BRASS,
        // Both require action, so both are red; the chip separates them by fill.
        Severity::Error | Severity::Hang => theme::BAD,
    }
}

fn sev_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => "Info",
        Severity::Warning => "Warning",
        Severity::Error => "Error",
        Severity::Hang => "Hang",
    }
}

/// A severity chip. `Hang` is FILLED where the others are outlined: error means the mod will not
/// work, hang means the game freezes and says nothing at all, so it is not one step further down a
/// ramp.
fn sev_chip(ui: &mut egui::Ui, s: Severity) {
    let c = sev_colour(s);
    let (bg, fg) = if matches!(s, Severity::Hang) {
        (c, Color32::from_rgb(0x1a, 0x0f, 0x0c))
    } else {
        (theme::G2, c)
    };
    egui::Frame::none()
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, c))
        .rounding(3.0)
        .inner_margin(egui::Margin::symmetric(5.0, 1.0))
        .show(ui, |ui| {
            ui.label(theme::disp_text(sev_label(s).to_uppercase(), 9.0, fg));
        });
}

/// A human label — what the contribution is called, not how it is built.
fn contribution_name(c: &Contribution) -> String {
    match c {
        Contribution::AddOutfit { name, .. }
        | Contribution::AddModel { name, .. }
        | Contribution::AddMovie { name, .. } => name.clone(),
        Contribution::ReplaceTexture { target, .. }
        | Contribution::PatchLua { target, .. }
        | Contribution::EditStateMachine { target, .. } => target.clone(),
        // `NativeHook.target` is the ENGINE, not an asset — so name it by what it actually is.
        Contribution::NativeHook { plugin, symbol, .. } => plugin
            .as_ref()
            .map(|f| leaf(f))
            .or_else(|| symbol.clone())
            .unwrap_or_else(|| "native hook".into()),
        Contribution::PlaceFile { file, .. } => leaf(file),
        Contribution::Raw { payload, .. } => leaf(payload),
    }
}

fn leaf(p: &Path) -> String {
    p.file_name()
        .map(|x| x.to_string_lossy().to_string())
        .unwrap_or_else(|| p.display().to_string())
}

/// The worst finding on a contribution, for the queue row's reason line.
fn worst_on<'a>(p: &'a Panel, i: usize) -> Option<&'a Diagnostic> {
    p.findings_for(i).max_by_key(|d| d.severity)
}

/// Rule titles are written as full explanations; a queue row has one line.
fn short(title: &str) -> String {
    let cut = title.split('\u{2014}').next().unwrap_or(title).trim();
    let cut = if cut.is_empty() { title } else { cut };
    if cut.chars().count() > 44 {
        format!("{}\u{2026}", cut.chars().take(43).collect::<String>())
    } else {
        cut.to_string()
    }
}

fn tally(ds: &[Diagnostic]) -> String {
    let (mut h, mut e, mut w) = (0, 0, 0);
    for d in ds {
        match d.severity {
            Severity::Hang => h += 1,
            Severity::Error => e += 1,
            Severity::Warning => w += 1,
            Severity::Info => {}
        }
    }
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(format!("{h} hang"));
    }
    if e > 0 {
        parts.push(format!("{e} error"));
    }
    if w > 0 {
        parts.push(format!("{w} warning"));
    }
    parts.join(" \u{b7} ")
}


/// A key/value row that reads left-to-right.
///
/// `theme::kv` right-aligns its value, which is right for a short number and wrong for a path: a
/// long value grows leftward until it sits on top of its own key. Here the key keeps a fixed
/// column and the value is elided from the FRONT, because the end of a path is what identifies it.
fn row(ui: &mut egui::Ui, key: &str, value: &str, colour: Color32) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(78.0, 14.0), egui::Sense::hover());
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            key,
            egui::FontId::proportional(11.0),
            theme::FAINT,
        );
        let avail = ui.available_width();
        let max_chars = ((avail / 6.2).floor() as usize).max(12);
        let n = value.chars().count();
        let shown = if n > max_chars {
            let tail: String = value.chars().skip(n - (max_chars - 1)).collect();
            format!("\u{2026}{tail}")
        } else {
            value.to_string()
        };
        ui.label(egui::RichText::new(shown).monospace().size(11.0).color(colour))
            .on_hover_text(value);
    });
}

// ─────────────────────────────────────────────────────────────────────────── navigator

/// The contribution queue.
///
/// Each row wears its worst finding as a left stripe AND names it: `Diagnostic::at` ties a finding
/// to a contribution, so a stripe without its cause would just be a colour.
pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    ui.label(theme::disp_text("SHIPMENT", 15.0, theme::TX));
    let Some(s) = &p.shipment else {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new(
                "A Shipment is a folder that builds to one overlay WAD, leaving the base game untouched.",
            )
            .size(11.5)
            .color(theme::FAINT),
        );
        return acts;
    };
    ui.label(
        egui::RichText::new(format!(
            "{} {}",
            s.manifest.shipment.name, s.manifest.shipment.version
        ))
        .size(10.5)
        .monospace()
        .color(theme::FAINT),
    );
    ui.add_space(9.0);
    theme::eyebrow(ui, &format!("Contributions \u{b7} {}", s.manifest.contributions.len()));
    ui.add_space(5.0);

    for (i, c) in s.manifest.contributions.iter().enumerate() {
        let stripe = match p.row_severity(i) {
            Some(x) => sev_colour(x),
            // Not "nothing wrong" — "nothing to report yet". Green is earned by shipping.
            None if p.report.is_some() => theme::GOOD_DK,
            None => theme::LINE2,
        };
        let selected = p.selected == Some(i);
        let resp = egui::Frame::none()
            .fill(if selected { theme::BRASS_SOFT } else { Color32::TRANSPARENT })
            .inner_margin(egui::Margin { left: 9.0, right: 8.0, top: 6.0, bottom: 7.0 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(theme::disp_text(c.kind().to_uppercase(), 8.5, theme::DIM));
                ui.label(
                    egui::RichText::new(contribution_name(c))
                        .size(12.5)
                        .color(if selected { theme::BRASS } else { theme::TX }),
                );
                if let Some(d) = worst_on(p, i) {
                    ui.horizontal(|ui| {
                        sev_chip(ui, d.severity);
                        ui.label(
                            egui::RichText::new(d.rule.code).size(9.5).monospace().color(theme::DIM),
                        );
                    });
                    ui.label(egui::RichText::new(short(d.rule.title)).size(10.5).color(theme::DIM));
                }
            })
            .response;
        ui.painter().rect_filled(
            egui::Rect::from_min_size(resp.rect.min, egui::vec2(3.0, resp.rect.height())),
            0.0,
            stripe,
        );
        if resp.interact(egui::Sense::click()).clicked() {
            acts.push(Act::Select(i));
        }
    }
    acts
}

// ──────────────────────────────────────────────────────────────────────── main content

/// The selected contribution, in full.
///
/// The blast radius here is COMPUTED, never authored — only `raw` declares its own, and it is the
/// one kind that can. Showing it is the point of giving this a main area: it answers "can this
/// coexist with someone else's Shipment", which a modder cannot work out alone.
pub fn center(ctx: &egui::Context, p: &Panel) {
    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(theme::G0)
                .inner_margin(egui::Margin::symmetric(22.0, 18.0)),
        )
        .show(ctx, |ui| {
            let Some(s) = &p.shipment else {
                return empty_middle(
                    ui,
                    "NO SHIPMENT OPEN",
                    "Open one, or export a character from the Skeleton bench.",
                );
            };
            let Some(i) = p.selected.filter(|i| *i < s.manifest.contributions.len()) else {
                return empty_middle(ui, "PICK A CONTRIBUTION", "The queue is on the left.");
            };
            let c = &s.manifest.contributions[i];

            ui.horizontal(|ui| {
                ui.label(theme::disp_text(c.kind().to_uppercase(), 11.0, theme::BRASS));
                ui.label(egui::RichText::new(contribution_name(c)).size(22.0).color(theme::TX));
            });
            ui.label(
                egui::RichText::new(format!("contributions[{i}]"))
                    .size(10.5)
                    .monospace()
                    .color(theme::FAINT),
            );
            ui.add_space(14.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                theme::section(ui, "Source", None, true, |ui| {
                    for (k, v) in source_rows(c) {
                        row(ui, &k, &v, theme::TX);
                    }
                });

                theme::section(ui, "Blast radius", Some("computed"), true, |ui| {
                    ui.label(
                        egui::RichText::new(
                            "Derived from the contribution, not declared \u{2014} only `raw` declares its own.",
                        )
                        .size(11.0)
                        .color(theme::FAINT),
                    );
                    ui.add_space(6.0);
                    for (k, v) in blast_rows(c) {
                        row(ui, &k, &v, theme::DIM);
                    }
                });

                let mine: Vec<&Diagnostic> = p.findings_for(i).collect();
                theme::section(ui, "Findings", Some(&format!("{}", mine.len())), true, |ui| {
                    if mine.is_empty() {
                        ui.label(
                            egui::RichText::new("Nothing to report against this contribution.")
                                .size(11.5)
                                .color(theme::FAINT),
                        );
                    }
                    for d in mine {
                        ui.horizontal(|ui| {
                            sev_chip(ui, d.severity);
                            ui.label(
                                egui::RichText::new(d.rule.code)
                                    .size(10.0)
                                    .monospace()
                                    .color(theme::DIM),
                            );
                            ui.label(egui::RichText::new(d.rule.title).size(12.0).color(theme::TX));
                        });
                        ui.label(
                            egui::RichText::new(&d.message)
                                .size(10.5)
                                .monospace()
                                .color(theme::DIM),
                        );
                        // Only where the fix is mechanical — which is what keeps the linter a tool
                        // rather than a nag.
                        if let Some(fix) = &d.fix {
                            ui.label(
                                egui::RichText::new(format!("fix: {fix}"))
                                    .size(10.0)
                                    .monospace()
                                    .color(theme::GOOD),
                            );
                        }
                        ui.add_space(7.0);
                    }
                });
            });
        });
}

fn empty_middle(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(90.0);
        ui.label(theme::disp_text(title, 13.0, theme::FAINT));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(sub).size(12.0).color(theme::FAINT));
    });
}

/// The contribution's own inputs, as the author wrote them.
fn source_rows(c: &Contribution) -> Vec<(String, String)> {
    // The path the AUTHOR wrote. An absolute scratch path is not what they recognise.
    let abs = |p: &PathBuf| p.display().to_string();
    match c {
        Contribution::AddOutfit {
            model, donor, wearer, slug, retarget, textures, ..
        } => {
            let mut v = vec![("Model".to_string(), abs(model))];
            if let Some(d) = donor {
                v.push(("Donor".into(), d.clone()));
            }
            v.push(("Wearer".into(), format!("{wearer} / {slug}")));
            if let Some(r) = retarget {
                let n = r.bones.as_ref().map(|b| b.len()).unwrap_or(0);
                v.push(("Retarget".into(), format!("{} \u{b7} {n} bone rows", r.from)));
            }
            for (k, t) in [
                ("Diffuse", &textures.diffuse),
                ("Specular", &textures.specular),
                ("Normal", &textures.normal),
            ] {
                if let Some(t) = t {
                    v.push((k.to_string(), abs(t)));
                }
            }
            v
        }
        Contribution::AddModel { model, donor, retarget, .. } => {
            let mut v = vec![("Model".to_string(), abs(model))];
            if let Some(d) = donor {
                v.push(("Donor".into(), d.clone()));
            }
            if let Some(r) = retarget {
                v.push(("Retarget".into(), r.from.clone()));
            }
            v
        }
        Contribution::AddMovie { movie, .. } => vec![("Movie".to_string(), abs(movie))],
        Contribution::ReplaceTexture { target, image } => vec![
            ("Target".to_string(), target.clone()),
            ("Image".to_string(), abs(image)),
        ],
        Contribution::PatchLua { target, append, .. } => vec![
            ("Target".to_string(), target.clone()),
            ("Append".to_string(), abs(append)),
        ],
        Contribution::EditStateMachine { target, .. } => {
            vec![("Target".to_string(), target.clone())]
        }
        Contribution::NativeHook { plugin, symbol, target, .. } => {
            let mut v = vec![("Engine".to_string(), format!("{target:?}").to_lowercase())];
            if let Some(f) = plugin {
                v.push(("Plugin".into(), abs(f)));
            }
            if let Some(sym) = symbol {
                v.push(("Symbol".into(), sym.clone()));
            }
            v
        }
        Contribution::PlaceFile { file, dest } => vec![
            ("File".to_string(), abs(file)),
            ("Destination".to_string(), format!("{dest:?}")),
        ],
        Contribution::Raw { payload, target_layer, .. } => vec![
            ("Payload".to_string(), abs(payload)),
            ("Layer".to_string(), format!("{target_layer:?}")),
        ],
    }
}

/// What the contribution touches, and how it merges with someone else's.
fn blast_rows(c: &Contribution) -> Vec<(String, String)> {
    match c {
        Contribution::AddOutfit { name, wearer, slug, .. } => vec![
            ("Writes".to_string(), format!("model {name}  (new hash)")),
            (
                "Writes".to_string(),
                format!("_tOutfits[{wearer}]  ordered-list, key ({wearer},{slug})"),
            ),
            ("Reads".to_string(), "_nAvailableCostumes".to_string()),
        ],
        Contribution::AddModel { name, .. } => {
            vec![("Writes".to_string(), format!("model {name}  (new hash)"))]
        }
        Contribution::AddMovie { name, .. } => {
            vec![("Writes".to_string(), format!("cfx_pack {name}  (new hash)"))]
        }
        Contribution::ReplaceTexture { target, .. } => vec![
            ("Writes".to_string(), format!("texture {target}")),
            (
                "Merge".to_string(),
                "last-wins \u{2014} load order is the answer".to_string(),
            ),
        ],
        Contribution::PatchLua { target, .. } => vec![
            ("Writes".to_string(), format!("script {target}")),
            (
                "Merge".to_string(),
                "relinked across the installed set at install".to_string(),
            ),
        ],
        Contribution::EditStateMachine { target, .. } => {
            vec![("Writes".to_string(), format!("state machine {target}"))]
        }
        // Discovery is filesystem order, so two plugins on one address have no load order that
        // resolves them — it has to be exclusive.
        Contribution::NativeHook { symbol, .. } => vec![(
            "Writes".to_string(),
            format!(
                "hook {}  \u{2014} EXCLUSIVE",
                symbol.clone().unwrap_or_else(|| "(plugin)".into())
            ),
        )],
        Contribution::PlaceFile { file, .. } => {
            vec![("Writes".to_string(), format!("file {}", leaf(file)))]
        }
        Contribution::Raw { touches, .. } => touches
            .iter()
            .map(|t| ("Declares".to_string(), t.0.clone()))
            .collect(),
    }
}

// ───────────────────────────────────────────────────────────────────────── right panel

/// The gate, the stack that was read, the problems, and the build's output.
pub fn inspector(ui: &mut egui::Ui, p: &Panel, wad_stack: &[String], has_game: bool) -> Vec<Act> {
    let mut acts = Vec::new();
    let gate = p.gate(has_game);
    let gc = gate.colour();

    // The gate first, because it decides whether Build is even live.
    egui::Frame::none()
        .fill(theme::G2)
        .stroke(egui::Stroke::new(1.0, gc))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(11.0, 9.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.5, gc);
                ui.label(theme::disp_text(gate.title().to_uppercase(), 11.5, gc));
            });
            ui.label(egui::RichText::new(p.gate_detail(gate)).size(11.5).color(theme::DIM));
        });
    ui.add_space(10.0);

    if let Some(e) = &p.error {
        ui.label(egui::RichText::new(e).size(11.0).color(theme::BAD));
        ui.add_space(6.0);
    }

    // Naming the stack rather than merely resolving it: which install was read sits behind a large
    // share of this project's own trap reports.
    theme::section(ui, "Game stack", None, true, |ui| {
        if wad_stack.is_empty() {
            ui.label(
                egui::RichText::new("Not configured \u{2014} set it in Settings.")
                    .size(11.0)
                    .italics()
                    .color(theme::FAINT),
            );
        }
        for (i, w) in wad_stack.iter().enumerate() {
            theme::kv(
                ui,
                if i == 0 { "base" } else { "overlay" },
                egui::RichText::new(leaf(Path::new(w)))
                    .monospace()
                    .size(10.5)
                    .color(theme::DIM),
            );
        }
    });

    if !p.diagnostics.is_empty() {
        let badge = tally(&p.diagnostics);
        theme::section(ui, "Problems", Some(&badge), true, |ui| {
            for d in &p.diagnostics {
                ui.horizontal(|ui| {
                    sev_chip(ui, d.severity);
                    // Every rule carries a published write-up; the code is the way in.
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(d.rule.code)
                                    .size(10.0)
                                    .monospace()
                                    .color(theme::DIM)
                                    .underline(),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text(d.rule.url())
                        .clicked()
                    {
                        acts.push(Act::OpenDoc(d.rule.url()));
                    }
                });
                ui.label(egui::RichText::new(d.rule.title).size(11.5).color(theme::TX));
                if let Some(i) = d.at {
                    if ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(format!("contributions[{i}]"))
                                    .size(10.0)
                                    .monospace()
                                    .color(theme::FAINT),
                            )
                            .sense(egui::Sense::click()),
                        )
                        .on_hover_text("Show this contribution")
                        .clicked()
                    {
                        acts.push(Act::Select(i));
                    }
                }
                ui.add_space(7.0);
            }
        });
    }

    if let Some(r) = &p.report {
        theme::section(ui, "Output", None, true, |ui| {
            if let Some(w) = &r.wad {
                theme::kv(
                    ui,
                    "Overlay",
                    egui::RichText::new(leaf(w)).monospace().size(10.5).color(theme::TX),
                );
            }
            for pl in &r.placements {
                theme::kv(
                    ui,
                    &pl.name,
                    egui::RichText::new(format!(
                        "{} B \u{b7} {}",
                        pl.bytes,
                        &pl.sha256[..16.min(pl.sha256.len())]
                    ))
                    .monospace()
                    .size(10.0)
                    .color(theme::DIM),
                );
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "The script half relinks across every installed Shipment at install, so this WAD is only valid standalone.",
                )
                .size(10.0)
                .color(theme::FAINT),
            );
        });
    }
    acts
}

/// The verb bar for this page.
pub fn verbs(ui: &mut egui::Ui, p: &Panel, has_game: bool) -> Vec<Act> {
    let mut acts = Vec::new();
    if ui.button("Open Shipment").clicked() {
        acts.push(Act::Open);
    }
    if ui
        .add_enabled(p.shipment.is_some(), egui::Button::new("Re-check"))
        .clicked()
    {
        acts.push(Act::Recheck);
    }
    if p.report.is_some() && ui.button("Reveal").clicked() {
        acts.push(Act::Reveal);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if p.report.is_some() {
            // A handoff, not a deploy: Modkit owns install and undo, so this is reversible and does
            // not wear the hazard treatment.
            if theme::primary_button(ui, "Send to Modkit", true).clicked() {
                acts.push(Act::SendToModkit);
            }
        } else {
            let can = p.shipment.is_some() && !p.blocks() && has_game;
            let r = theme::primary_button(ui, "Build", can);
            if r.clicked() {
                acts.push(Act::Build);
            }
            if !can && p.blocks() {
                r.on_hover_text("Fix what blocks first.");
            } else if !can && !has_game {
                r.on_hover_text("No game configured.");
            }
        }
    });
    acts
}

// ──────────────────────────────────────────────────────────────────────────── actions

/// The decompiled Lua corpus, needed by any script-touching contribution.
///
/// Walked for rather than configured: it is vendored in-tree, and a build that silently skipped the
/// script half would produce a WAD that looks complete and does nothing.
pub fn corpus_root() -> Option<PathBuf> {
    let mut dir: Option<&Path> = Some(Path::new(env!("CARGO_MANIFEST_DIR")));
    while let Some(d) = dir {
        let c = d.join("crates/mercs2_script/corpus/mercs2-luacd/src");
        if c.is_dir() {
            return Some(c);
        }
        dir = d.parent();
    }
    None
}

/// Execute one queued action.
pub fn apply(
    act: Act,
    p: &mut Panel,
    wad_stack: &[String],
    names: Option<&NameTable>,
    corpus: Option<&Path>,
    status: &mut String,
) {
    match act {
        Act::Select(i) => p.selected = Some(i),
        Act::Open => {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("Open a Shipment folder")
                .pick_folder()
            {
                p.open_shipment(&dir, names);
                *status = p.status.clone();
            }
        }
        Act::Recheck => {
            if let Some(root) = p.root().map(|r| r.to_path_buf()) {
                p.open_shipment(&root, names);
                *status = p.status.clone();
            }
        }
        Act::Build => {
            let paths: Vec<PathBuf> = wad_stack.iter().map(PathBuf::from).collect();
            match mercs2_quartermaster::game::GameStack::open(&paths) {
                Ok(mut g) => {
                    run_build(p, Some(&mut g), names, corpus);
                    *status = p.status.clone();
                }
                Err(e) => {
                    p.error = Some(format!("game stack: {e:?}"));
                    *status = "build needs a readable game stack".into();
                }
            }
        }
        Act::Reveal => {
            if let Some(w) = p.report.as_ref().and_then(|r| r.wad.as_ref()) {
                let _ = open_in_os(w.parent().unwrap_or(Path::new(".")));
            }
        }
        Act::SendToModkit => match send_to_modkit(p) {
            Ok(dest) => {
                p.status = format!("sent to {}", dest.display());
                *status = p.status.clone();
            }
            Err(e) => {
                p.error = Some(e);
                *status = "could not send to Modkit".into();
            }
        },
        Act::OpenDoc(url) => {
            let _ = open_in_os(Path::new(&url));
        }
    }
}

/// Run a build and fold the outcome back in.
///
/// `Blocked` is not an error here — it is the linter doing its job, so the findings replace the
/// current set and the gate turns red rather than a dialog appearing.
pub fn run_build(
    p: &mut Panel,
    game: Option<&mut mercs2_quartermaster::game::GameStack>,
    names: Option<&NameTable>,
    corpus: Option<&Path>,
) {
    let Some(s) = &p.shipment else { return };
    match build::build(s, game, names, None, corpus) {
        Ok(r) => {
            p.status = match &r.wad {
                Some(w) => format!("built {}", leaf(w)),
                None => "built (no overlay)".into(),
            };
            p.diagnostics = r.diagnostics.clone();
            p.error = None;
            p.report = Some(r);
        }
        Err(BuildError::Blocked(ds)) => {
            p.diagnostics = ds;
            p.report = None;
            p.error = None;
            p.status = "blocked".into();
        }
        Err(e) => {
            p.report = None;
            p.error = Some(format!("{e:?}"));
            p.status = "build failed".into();
        }
    }
}

/// Where the Workshop drops a built Shipment for Modkit to find.
///
/// A folder both apps agree on IS the integration; a deep link would only be convenience over it,
/// and needs two Tauri plugins Modkit does not carry. Modkit already drives `qm`, so nothing here
/// writes into a game folder — install and undo stay its job, with the undo record to match.
pub fn shipments_library() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("mercs2").join("shipments"))
}

fn send_to_modkit(p: &Panel) -> Result<PathBuf, String> {
    let root = p.root().ok_or("no shipment open")?;
    let lib = shipments_library().ok_or("no home directory to place the shipments library in")?;
    let dest = lib.join(root.file_name().ok_or("the shipment folder has no name")?);
    copy_tree(root, &dest).map_err(|e| format!("copying the shipment: {e}"))?;
    Ok(dest)
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

fn open_in_os(target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(target)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(target)
            .spawn()
            .map(|_| ())
    }
}
