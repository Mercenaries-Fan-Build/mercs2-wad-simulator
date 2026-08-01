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
use mercs2_quartermaster::manifest::{Contribution, Touch};
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
            Gate::Done => "Built",
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

/// A craft surface — a bench that edits ONE contribution, entered from it and returning to it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Craft {
    /// The retarget bench: source rig onto the donor's, and the bone map a Shipment records.
    Rig,
    /// The conform bench: fit an import onto a donor, host groups and hardpoints.
    Conform,
}

/// The three wardrobe heroes, in the preferred spelling — re-exported from the manifest crate so
/// the UI pills and the format's own vocabulary cannot drift. `jen`, not `jennifer`; the runtime
/// `_tOutfits` key (`jennifer`) is resolved by `manifest::wearer_table_key` at emit time.
pub use mercs2_quartermaster::manifest::WEARERS;

/// Queued by the widgets, executed by [`apply`], so rendering never borrows the game stack.
pub enum Act {
    Open,
    /// Append a contribution of this `kind` to the manifest and write it back.
    Add(&'static str),
    Remove(usize),
    /// Open the craft surface that edits this contribution — the rig bench, or the conform bench.
    ///
    /// Carries the contribution INDEX. Without it the bench had no idea what it was editing, so a
    /// commit could only ever append a new contribution, and there was no way back to the one you
    /// came from.
    Craft(Craft, usize),
    /// Set the hero whose wardrobe an outfit joins.
    SetWearer(usize, &'static str),
    /// Replace contribution `.0` with an edited copy.
    ///
    /// The form edits a CLONE and emits this when a field commits, rather than mutating the
    /// manifest under the renderer — the same rule the rest of this panel follows, and the reason
    /// rendering never has to borrow the game stack. Boxed because `Contribution` is much larger
    /// than the other variants and `Act` is moved around per frame.
    Edit(usize, Box<Contribution>),
    /// Scaffold a brand-new Shipment.
    New,
    /// Replace the Shipment's own identity block (name, version, target, load order).
    EditIdentity(Box<mercs2_quartermaster::manifest::Shipment>, Box<mercs2_quartermaster::manifest::Load>),
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
            Gate::Blocked if h > 0 => format!(
                "{h} hang and {e} error{}, neither of which the game will report.",
                plural(e)
            ),
            Gate::Blocked => format!(
                "{e} error{}, and the game will not say so.",
                plural(e)
            ),
            Gate::Advisory if w > 0 => format!(
                "{w} advisor{}, and nothing built yet.",
                if w == 1 { "y" } else { "ies" }
            ),
            Gate::Advisory => "Nothing built yet.".into(),
            // Was "Rebuilds byte-identical." — true of the BUILDER, and not something this
            // run tested. State the artifact instead; a Verify pass is what would earn the claim.
            Gate::Done => self
                .report
                .as_ref()
                .and_then(|r| r.wad.as_ref())
                .map(|w| format!("{} is on disk.", leaf(w)))
                .unwrap_or_else(|| "Nothing to ship — no overlay was produced.".into()),
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

    /// Edit the manifest, write it back, and re-check.
    ///
    /// Writes YAML whatever the file was read as, so a Shipment authored in TOML or JSON is
    /// refused rather than silently re-emitted in another format under the same filename.
    /// `to_yaml` is documented as "the one format the Quartermaster WRITES".
    pub(crate) fn mutate(
        &mut self,
        names: Option<&NameTable>,
        edit: impl FnOnce(&mut mercs2_quartermaster::manifest::Manifest),
    ) -> Result<(), String> {
        let Some(sh) = &self.shipment else {
            return Err("no shipment open".into());
        };
        if sh.format != mercs2_quartermaster::Format::Yaml {
            return Err(format!(
                "this Shipment is {:?}; editing writes YAML, so it is read-only here",
                sh.format
            ));
        }
        let (path, root) = (sh.manifest_path.clone(), sh.root.clone());
        let mut m = sh.manifest.clone();
        edit(&mut m);
        let text = mercs2_quartermaster::to_yaml(&m)?;
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        // Re-open rather than patch state in place: the linter has to see the file that is now on
        // disk, not the one we think we wrote.
        self.open_shipment(&root, names);
        Ok(())
    }

    /// Add a contribution, or replace the one at `i`. The single entry point for every surface
    /// that produces one — the Library's "Add to Shipment", a craft bench committing its work, the
    /// queue's own add menu.
    pub(crate) fn upsert_contribution(
        &mut self,
        names: Option<&NameTable>,
        i: Option<usize>,
        c: Contribution,
    ) -> Result<usize, String> {
        let mut at = i.unwrap_or(usize::MAX);
        self.mutate(names, |m| match i {
            Some(i) if i < m.contributions.len() => m.contributions[i] = c,
            _ => {
                at = m.contributions.len();
                m.contributions.push(c);
            }
        })?;
        self.selected = Some(at);
        Ok(at)
    }

    /// The root of an OPEN Shipment, creating one if there is none.
    ///
    /// This is what stops a craft bench being a dead end. Before it, the only way a bench could
    /// produce a Shipment was `shipment::write`, which always picked a fresh folder and wrote a
    /// brand-new single-contribution manifest over it — so work done with a Shipment already open
    /// either had nowhere to go or silently replaced what was there.
    ///
    /// **Nothing in the workspace scaffolded a Shipment**: `qm` has no `init`, `discover` is
    /// read-only, and the skeleton existed only in the template repo. So the scaffold is authored
    /// here — through `to_yaml`, never by formatting YAML by hand.
    pub(crate) fn ensure_shipment(
        &mut self,
        names: Option<&NameTable>,
    ) -> Result<PathBuf, String> {
        if let Some(r) = self.root() {
            return Ok(r.to_path_buf());
        }
        let dir = rfd::FileDialog::new()
            .set_title("New Shipment — pick an empty folder")
            .pick_folder()
            .ok_or("cancelled")?;
        self.scaffold(&dir, names)?;
        Ok(dir)
    }

    /// Write a fresh `manifest.yaml` + `src/` + `README.md` into `dir` and open it.
    ///
    /// Refuses a folder that already holds a manifest rather than overwriting one: this is reached
    /// from a picker, and picking the wrong folder must not destroy someone's work.
    pub(crate) fn scaffold(
        &mut self,
        dir: &Path,
        names: Option<&NameTable>,
    ) -> Result<(), String> {
        use mercs2_quartermaster::manifest::{Load, Manifest, Shipment, Target, FORMAT_VERSION};

        if mercs2_quartermaster::discover::find_manifest(dir).is_ok() {
            return Err(format!(
                "{} already holds a manifest — open it instead of scaffolding over it",
                dir.display()
            ));
        }
        // `shipment.name` is a slug AND the output filename (`build/<name>.wad`), so it cannot be
        // the folder name verbatim.
        let name = crate::shipment::slugify(
            &dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        );
        let name = if name.is_empty() { "my-shipment".to_string() } else { name };
        let m = Manifest {
            format: FORMAT_VERSION,
            shipment: Shipment {
                name: name.clone(),
                title: None,
                version: "0.1.0".into(),
                authors: Vec::new(),
                description: None,
                target: Target::Retail,
                quartermaster: None,
                license: None,
                homepage: None,
                tags: Vec::new(),
            },
            load: Load::default(),
            contributions: Vec::new(),
        };
        m.validate().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(dir.join("src"))
            .map_err(|e| format!("{}: {e}", dir.join("src").display()))?;
        let text = mercs2_quartermaster::to_yaml(&m)?;
        let path = dir.join("manifest.yaml");
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        // Only if absent — a folder may already carry the author's own notes.
        let readme = dir.join("README.md");
        if !readme.exists() {
            let _ = std::fs::write(
                &readme,
                format!(
                    "# {name}\n\nA Mercenaries 2 Shipment. Sources live in `src/`; \
                     `qm build .` writes `build/{name}.wad`.\n",
                ),
            );
        }
        self.open_shipment(dir, names);
        Ok(())
    }

    /// Copy a source file into the Shipment's `src/` and return the manifest-relative path.
    ///
    /// Every kind references its inputs by a `src/`-relative path, so a bench holding an absolute
    /// path to a scratch file has to bring the bytes along or the Shipment stops building the
    /// moment it moves machines. Collisions get a numeric suffix rather than overwriting.
    pub(crate) fn import_source(root: &Path, from: &Path) -> Result<PathBuf, String> {
        let src = root.join("src");
        std::fs::create_dir_all(&src).map_err(|e| format!("{}: {e}", src.display()))?;
        let stem = from.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "asset".into());
        let ext = from.extension().map(|s| format!(".{}", s.to_string_lossy())).unwrap_or_default();
        let mut leaf = format!("{stem}{ext}");
        let mut n = 1;
        while src.join(&leaf).exists() {
            // Same name, same bytes: reuse it rather than piling up copies.
            if std::fs::read(src.join(&leaf)).ok() == std::fs::read(from).ok() {
                return Ok(PathBuf::from("src").join(leaf));
            }
            leaf = format!("{stem}_{n}{ext}");
            n += 1;
        }
        std::fs::copy(from, src.join(&leaf))
            .map_err(|e| format!("copying {}: {e}", from.display()))?;
        Ok(PathBuf::from("src").join(leaf))
    }

    /// A suffix that does not collide with what is already queued.
    fn next_stub_index(&self) -> usize {
        self.shipment
            .as_ref()
            .map(|s| s.manifest.contributions.len() + 1)
            .unwrap_or(1)
    }

    /// How many contributions are queued — for a caller minting a non-colliding stub name.
    pub fn contribution_count(&self) -> usize {
        self.shipment.as_ref().map(|s| s.manifest.contributions.len()).unwrap_or(0)
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

/// `""` for one, `"s"` for any other count — including zero.
fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
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
        | Contribution::AddTexture { name, .. }
        | Contribution::AddSound { name, .. }
        | Contribution::AddMovie { name, .. } => name.clone(),
        Contribution::ReplaceTexture { target, .. }
        | Contribution::PatchLua { target, .. }
        | Contribution::EditStateMachine { target, .. }
        | Contribution::EditStringDb { target, .. } => target.clone(),
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


/// Sizes a person reads, not a raw byte count.
fn human_bytes(n: usize) -> String {
    if n >= 1 << 20 {
        format!("{:.1} MB", n as f64 / (1u64 << 20) as f64)
    } else if n >= 1 << 10 {
        format!("{:.1} kB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

/// A key/value row that reads left-to-right.
///
/// `theme::kv` right-aligns its value, which is right for a short number and wrong for a path: a
/// long value grows leftward until it sits on top of its own key. Here the key keeps a fixed
/// column and the value is elided from the FRONT, because the end of a path is what identifies it.
fn row(ui: &mut egui::Ui, key: &str, value: &str, colour: Color32) {
    row_of(ui, key, value, value, colour, true)
}

/// A row whose value is truncated from the END — right for a digest, where the leading characters
/// are the ones anyone actually compares.
fn row_head(ui: &mut egui::Ui, key: &str, value: &str, colour: Color32) {
    row_of(ui, key, value, value, colour, false)
}

fn row_of(ui: &mut egui::Ui, key: &str, value: &str, hover: &str, colour: Color32, from_front: bool) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(78.0, 14.0), egui::Sense::hover());
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            key,
            egui::FontId::proportional(11.0),
            theme::FAINT,
        );
        let max_chars = ((ui.available_width() / 6.2).floor() as usize).max(12);
        let n = value.chars().count();
        let shown = if n <= max_chars {
            value.to_string()
        } else if from_front {
            // A path identifies itself by its tail.
            format!("\u{2026}{}", value.chars().skip(n - (max_chars - 1)).collect::<String>())
        } else {
            format!("{}\u{2026}", value.chars().take(max_chars - 1).collect::<String>())
        };
        ui.label(egui::RichText::new(shown).monospace().size(11.0).color(colour))
            .on_hover_text(hover);
    });
}

/// Every kind the format knows, grouped by the layer it lands in.
///
/// Taken from the `Contribution` enum rather than a hand-kept list, so a kind the format gains
/// cannot silently go missing from the menu.
pub const KINDS: &[(&str, &[(&str, &str)])] = &[
    (
        "Data",
        &[
            ("add_outfit", "A wearable outfit — model plus a wardrobe row"),
            ("add_model", "A new model on a donor's rig"),
            ("add_texture", "A new texture under a name you choose"),
            ("add_sound", "A new audio bank (wavebank / soundbank / sounddb)"),
            ("add_movie", "A Scaleform movie"),
            ("replace_texture", "Replace a shipped texture, same hash"),
            ("edit_state_machine", "Rewrite a destructible's states"),
            ("edit_stringdb", "Correct or localise UI text"),
        ],
    ),
    ("Script", &[("patch_lua", "Append to a shipped script")]),
    (
        "Code",
        &[
            ("native_hook", "An ASI plugin, or a symbol to detour"),
            ("place_file", "A companion file beside a plugin"),
        ],
    ),
    ("Any", &[("raw", "Opaque bytes plus a declared blast radius")]),
];

/// What an EXISTING game asset can become the basis of, and how.
///
/// This is Plan 02's *"act on an asset — context menu → start a Shipment from this"*, which had no
/// implementation at all: there was no code path from an Inspect selection into the Quartermaster.
///
/// The asset is never the thing being written. A shipped model becomes a **donor** (its rig,
/// materials and state machine are borrowed, read-only); a shipped texture becomes a **target**.
/// That distinction is the `no-destructive-replacements` mandate expressed as a menu: nothing here
/// can produce a contribution that overwrites the asset you right-clicked.
pub fn routes_for(is_texture: bool) -> &'static [(&'static str, &'static str)] {
    if is_texture {
        &[("replace_texture", "Replace this texture")]
    } else {
        &[
            ("add_outfit", "New outfit on this donor"),
            ("add_model", "New model on this donor"),
        ]
    }
}

/// A stub for `kind` with an existing asset wired in as its donor or target.
pub fn seeded(kind: &str, asset: &str, n: usize) -> Option<Contribution> {
    let mut c = stub(kind, n)?;
    match &mut c {
        Contribution::AddOutfit { donor, .. } | Contribution::AddModel { donor, .. } => {
            *donor = Some(asset.to_string());
        }
        Contribution::ReplaceTexture { target, .. } => *target = asset.to_string(),
        _ => {}
    }
    Some(c)
}

/// A schema-valid stub for `kind`.
///
/// Deliberately valid enough to SERIALIZE and invalid enough to LINT: the placeholder paths do not
/// exist, so M0110 fires immediately and the panel tells the author what the contribution still
/// needs. An empty-name stub would fail `Manifest::validate` on the way back in and the page would
/// report "could not open" instead.
fn stub(kind: &str, n: usize) -> Option<Contribution> {
    use mercs2_quartermaster::manifest::{Layer, PlaceIn, Target, Textures};
    let name = format!("my_asset_{n}");
    Some(match kind {
        "add_outfit" => Contribution::AddOutfit {
            name: name.clone(),
            slug: format!("MyOutfit{n}"),
            display: "My outfit".into(),
            wearer: "mattias".into(),
            model: PathBuf::from("src/model.glb"),
            donor: Some("pmc_hum_mattias".into()),
            textures: Textures::default(),
            retarget: None,
        },
        "add_model" => Contribution::AddModel {
            name,
            model: PathBuf::from("src/model.glb"),
            donor: Some("pmc_hum_mattias".into()),
            group: None,
            textures: Textures::default(),
            retarget: None,
        },
        "add_texture" => Contribution::AddTexture {
            name,
            image: PathBuf::from("src/texture.png"),
            normal_map: false,
        },
        "add_sound" => Contribution::AddSound {
            name,
            bank: PathBuf::from("src/bank.bin"),
            sound: mercs2_quartermaster::manifest::SoundKind::Soundbank,
        },
        "edit_state_machine" => Contribution::EditStateMachine {
            target: "al_veh_boat_destroyer".into(),
            states: PathBuf::from("src/states.yaml"),
        },
        "edit_stringdb" => Contribution::EditStringDb {
            target: "english".into(),
            strings: PathBuf::from("src/strings.txt"),
        },
        "add_movie" => Contribution::AddMovie {
            name,
            movie: PathBuf::from("src/movie.gfx"),
        },
        "replace_texture" => Contribution::ReplaceTexture {
            target: "al_hum_boss_ub".into(),
            image: PathBuf::from("src/texture.png"),
        },
        "patch_lua" => Contribution::PatchLua {
            target: "wifpmcinterior".into(),
            append: PathBuf::from("src/patch.lua"),
        },
        "native_hook" => Contribution::NativeHook {
            target: Target::Retail,
            plugin: Some(PathBuf::from("src/plugin.asi")),
            symbol: None,
            touches: Vec::new(),
        },
        "place_file" => Contribution::PlaceFile {
            file: PathBuf::from("src/plugin.ini"),
            dest: PlaceIn::Scripts,
        },
        "raw" => Contribution::Raw {
            description: None,
            payload: PathBuf::from("src/payload.bin"),
            target_layer: Layer::Data,
            touches: Vec::new(),
        },
        _ => return None,
    })
}

// ---- navigator ----------------------------------------------------------------------

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

    // The queue is where a Shipment is composed, so the menu that composes it lives here rather
    // than behind a toolbar. Grouped by LAYER, because that is how the format groups them and it
    // says where the change lands — Data in the overlay, Script through the linker, Code as a file
    // beside the game.
    let add_menu = |ui: &mut egui::Ui, out: &mut Vec<Act>| {
        for (layer, kinds) in KINDS {
            ui.label(theme::disp_text(layer.to_uppercase(), 9.0, theme::FAINT));
            for (kind, blurb) in *kinds {
                if ui.button(*kind).on_hover_text(*blurb).clicked() {
                    out.push(Act::Add(kind));
                    ui.close_menu();
                }
            }
            ui.separator();
        }
    };

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
        let resp = resp.interact(egui::Sense::click());
        if resp.clicked() {
            acts.push(Act::Select(i));
        }
        let mut menu: Vec<Act> = Vec::new();
        resp.context_menu(|ui| {
            add_menu(ui, &mut menu);
            if ui.button("Remove this contribution").clicked() {
                menu.push(Act::Remove(i));
                ui.close_menu();
            }
        });
        acts.extend(menu);
    }

    // The empty space below the rows is still the queue, so right-clicking it adds too — otherwise
    // an empty Shipment would have nothing to right-click at all.
    let rest = ui.available_rect_before_wrap();
    if rest.height() > 4.0 {
        let bg = ui.interact(rest, ui.id().with("qm_queue_bg"), egui::Sense::click());
        let mut menu: Vec<Act> = Vec::new();
        bg.context_menu(|ui| add_menu(ui, &mut menu));
        acts.extend(menu);
    }
    acts
}

// ──────────────────────────────────────────────────────────────────────── main content

/// The selected contribution, in full.
///
/// The blast radius here is COMPUTED, never authored — only `raw` declares its own, and it is the
/// one kind that can. Showing it is the point of giving this a main area: it answers "can this
/// coexist with someone else's Shipment", which a modder cannot work out alone.
pub fn center(ctx: &egui::Context, p: &Panel, names: Option<&NameTable>) -> Vec<Act> {
    let mut acts: Vec<Act> = Vec::new();
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
            // Nothing selected: show the Shipment's OWN identity rather than a shrug. These
            // fields had no editor anywhere, and `shipment.name` decides the output filename.
            let Some(i) = p.selected.filter(|i| *i < s.manifest.contributions.len()) else {
                ui.label(theme::disp_text(
                    s.manifest.shipment.name.to_uppercase(),
                    22.0,
                    theme::TX,
                ));
                ui.label(
                    egui::RichText::new("the Shipment itself — pick a contribution at left to edit one")
                        .size(11.0)
                        .color(theme::FAINT),
                );
                ui.add_space(14.0);
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut sh = s.manifest.shipment.clone();
                    let mut load = s.manifest.load.clone();
                    if identity_form(ui, &mut sh, &mut load)
                        && (sh != s.manifest.shipment || load != s.manifest.load)
                    {
                        acts.push(Act::EditIdentity(Box::new(sh), Box::new(load)));
                    }
                });
                return;
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
                // ── WARDROBE ────────────────────────────────────────────────────────────────
                //
                // Moved off Modkit, which is Tier 1 — install, load order, deploy. Choosing which
                // hero wears an outfit is authoring, and authoring belongs to the tool that has the
                // rig, the donor and the linter. Modkit keeping its own copy is what produced two
                // independent writers of `_tOutfits` and the half-applied conflict.
                if let Contribution::AddOutfit { wearer, slug, .. } = c {
                    theme::section(ui, "Wardrobe", Some(slug), true, |ui| {
                        ui.label(
                            egui::RichText::new(
                                "`_tOutfits` has one list per hero, so the wearer is a closed set \u{2014} and the merge key is (wearer, slug), which is why retail can reuse `Original` across all three.",
                            )
                            .size(11.0)
                            .color(theme::FAINT),
                        );
                        ui.add_space(7.0);
                        ui.horizontal(|ui| {
                            for w in WEARERS {
                                if theme::pill(ui, w, w == wearer.as_str()).clicked() {
                                    acts.push(Act::SetWearer(i, w));
                                }
                            }
                        });
                    });
                }

                // ── FIELDS: the contribution, editable.
                //
                // Every field of every kind, committed through `Act::Edit` → `upsert_contribution`
                // → `mutate`, which re-writes the manifest and re-lints. The linter IS the feedback
                // loop; there is no separate "check" button to forget to press.
                let root = s.root.as_path();
                theme::section(ui, "Fields", Some(c.kind()), true, |ui| {
                    let mut edited = c.clone();
                    let commit = contribution_form(ui, &mut edited, root, names);
                    if commit && edited != *c {
                        acts.push(Act::Edit(i, Box::new(edited)));
                    }
                    // ── CRAFT ───────────────────────────────────────────────────────────────
                    //
                    // Mods and Skeleton are no longer rail peers; they act on ONE contribution, so
                    // they are entered from it and hand control back.
                    if matches!(c, Contribution::AddOutfit { .. } | Contribution::AddModel { .. }) {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button("Edit rig\u{2026}")
                                .on_hover_text("Retarget the source rig onto the donor's, and record the bone map")
                                .clicked()
                            {
                                acts.push(Act::Craft(Craft::Rig, i));
                            }
                            if ui
                                .button("Conform\u{2026}")
                                .on_hover_text("Fit the import onto the donor \u{2014} host groups and hardpoints")
                                .clicked()
                            {
                                acts.push(Act::Craft(Craft::Conform, i));
                            }
                        });
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
    acts
}

// ───────────────────────────────────────────────────────────────────── the per-kind edit form
//
// This replaces a read-only rendering. `center` used to print `source_rows()` — a
// `Vec<(String, String)>` of pre-formatted DISPLAY strings — so every field of every kind was
// text, and the only editable thing on the whole page was the wearer. Anything else meant leaving
// the tool and hand-editing YAML, which is the gap between the Quartermaster and Modkit that this
// page exists to close.

/// Does a source path resolve, and what should the field say about it?
///
/// Mirrors `discover::check_sources`, which is what the linter runs — so the answer here is the
/// same answer M0110/M0111/M0112 will give, just delivered at the click instead of at the next
/// lint pass.
fn source_state(root: &Path, rel: &Path) -> (theme::FieldState, Option<String>) {
    if rel.as_os_str().is_empty() {
        return (theme::FieldState::Bad, Some("no file chosen".into()));
    }
    if rel.is_absolute() {
        return (
            theme::FieldState::Bad,
            Some("M0111 — an absolute path leaves the Shipment; keep sources under src/".into()),
        );
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return (
            theme::FieldState::Bad,
            Some("M0111 — `..` leaves the Shipment root".into()),
        );
    }
    if !root.join(rel).is_file() {
        return (
            theme::FieldState::Bad,
            Some(format!("M0110 — {} does not exist", rel.display())),
        );
    }
    if !rel.starts_with("src") {
        return (
            theme::FieldState::Warn,
            Some("M0112 — outside src/; conventional sources live there".into()),
        );
    }
    (theme::FieldState::Good, None)
}

/// A `src/`-relative file row plus its live verdict.
fn source_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut PathBuf,
    root: &Path,
    filters: &[&str],
) -> bool {
    let (state, note) = source_state(root, value);
    let changed = theme::path_field(ui, label, value, root, filters, state);
    if let Some(n) = note {
        theme::field_note(ui, state, &n);
    }
    changed
}

/// An asset-reference row: free text, with the hash it resolves to shown live.
///
/// Every reference goes through `manifest::asset_hash`, which is a documented mandate — a bare
/// `0x…` IS the hash and anything else is hashed as a name. Hashing the *string* `"0x56130E64"`
/// yields `0xC6B71C1F`, so a field that computed its own preview differently would show one number
/// while the builder used another.
fn asset_row(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    names: Option<&NameTable>,
) -> bool {
    let empty = value.trim().is_empty();
    let state = if empty { theme::FieldState::Bad } else { theme::FieldState::Neutral };
    let r = theme::text_field(ui, label, value, "name or 0xHASH", state);
    if empty {
        theme::field_note(ui, theme::FieldState::Bad, "required");
    } else {
        let h = mercs2_quartermaster::manifest::asset_hash(value);
        match mercs2_quartermaster::manifest::bare_hash(value) {
            // M0130: a hash is one-way, so a manifest full of them cannot be reviewed. Offer the
            // name when one is known — advisory, never blocking.
            Some(_) => match names.and_then(|n| n.reverse(h)) {
                Some(name) => theme::field_note(
                    ui,
                    theme::FieldState::Warn,
                    &format!("M0130 — this is `{name}`; a name diffs, a hash does not"),
                ),
                None => theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    &format!("0x{h:08X} — no name known for this hash"),
                ),
            },
            None => theme::field_note(
                ui,
                theme::FieldState::Neutral,
                &format!("hashes to 0x{h:08X}"),
            ),
        }
    }
    r.lost_focus()
}

/// A plain required-text row (asset NAME being minted, slug, display).
fn text_row(ui: &mut egui::Ui, label: &str, value: &mut String, hint: &str, required: bool) -> bool {
    let bad = required && value.trim().is_empty();
    let state = if bad { theme::FieldState::Bad } else { theme::FieldState::Neutral };
    let r = theme::text_field(ui, label, value, hint, state);
    if bad {
        theme::field_note(ui, theme::FieldState::Bad, "required");
    }
    r.lost_focus()
}

/// An editable list of blast-radius entries (`raw.touches`, `native_hook.touches`).
fn touches_editor(ui: &mut egui::Ui, touches: &mut Vec<Touch>, required: bool) -> bool {
    let mut commit = false;
    let mut remove: Option<usize> = None;
    for (i, t) in touches.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            // State computed BEFORE the mutable borrow of the same field.
            let state = if t.0.trim().is_empty() {
                theme::FieldState::Bad
            } else {
                theme::FieldState::Neutral
            };
            let label = format!("touch {i}");
            let r = theme::text_field(ui, &label, &mut t.0, "name or 0xHASH", state);
            commit |= r.lost_focus();
            if ui.small_button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        touches.remove(i);
        commit = true;
    }
    if ui.button("+ touch").clicked() {
        touches.push(Touch(String::new()));
        commit = true;
    }
    if required && touches.is_empty() {
        theme::field_note(
            ui,
            theme::FieldState::Bad,
            "M0150 — a raw payload must declare what it touches; nothing downstream can infer it",
        );
    }
    commit
}

/// Edit every field of one contribution. Returns true when a change should be written back.
///
/// The caller passes a CLONE; this mutates it, and the caller compares and emits `Act::Edit`.
fn contribution_form(
    ui: &mut egui::Ui,
    c: &mut Contribution,
    root: &Path,
    names: Option<&NameTable>,
) -> bool {
    use mercs2_quartermaster::manifest::{Layer, PlaceIn, Target};
    let mut commit = false;

    match c {
        Contribution::AddOutfit {
            name, slug, display, wearer, model, donor, textures, retarget,
        } => {
            commit |= text_row(ui, "Asset name", name, "pmc_hum_my_outfit", true);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                &format!(
                    "0x{:08X} — what Player.SetOutfit receives",
                    mercs2_quartermaster::manifest::asset_hash(name)
                ),
            );
            commit |= text_row(ui, "Wardrobe slug", slug, "MyOutfit", true);
            commit |= text_row(ui, "Display", display, "My outfit", true);
            // A CLOSED set: `_tOutfits` has one list per hero, so a fourth name creates a wardrobe
            // nothing reads. M0140 is unrepresentable here rather than merely checked.
            ui.horizontal(|ui| {
                let lw = theme::field_label_w(ui.available_width());
                ui.add_space(lw + 4.0);
                for w in WEARERS {
                    if theme::pill(ui, w, w == wearer.as_str()).clicked() {
                        *wearer = w.to_string();
                        commit = true;
                    }
                }
            });
            commit |= source_row(ui, "Model", model, root, &["glb", "gltf", "obj"]);
            let mut d = donor.clone().unwrap_or_default();
            if asset_row(ui, "Donor", &mut d, names) {
                *donor = (!d.trim().is_empty()).then_some(d);
                commit = true;
            }
            // An outfit's donor is OPTIONAL: omit it and the build hosts on the wearer's own hero
            // model. Say so, so the empty field does not read as unfinished.
            if donor.is_none() {
                theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    &format!("optional — auto-picks pmc_hum_{wearer}. Set one for a variant host."),
                );
            }
            ui.add_space(4.0);
            theme::eyebrow(ui, "Skin — omit a slot to keep the donor's");
            for (lbl, slot) in [
                ("Diffuse", &mut textures.diffuse),
                ("Normal", &mut textures.normal),
                ("Specular", &mut textures.specular),
            ] {
                commit |= optional_source_row(ui, lbl, slot, root, &["png"]);
            }
            commit |= retarget_summary(ui, retarget);
        }
        Contribution::AddModel { name, model, donor, group, textures, retarget } => {
            commit |= text_row(ui, "Asset name", name, "my_custom_helipad", true);
            commit |= source_row(ui, "Model", model, root, &["glb", "gltf", "obj"]);
            let mut d = donor.clone().unwrap_or_default();
            if asset_row(ui, "Donor", &mut d, names) {
                *donor = (!d.trim().is_empty()).then_some(d);
                commit = true;
            }
            // The host draw group. Previously unrecordable, so a conform could be previewed and
            // then not expressed.
            let mut g = group.unwrap_or(0) as f32;
            if theme::scalar_field(ui, "Host group", &mut g, 1.0) {
                *group = Some((g.max(0.0) as u32).min(63));
                commit = true;
            }
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "0..63 — the donor draw group the geometry replaces",
            );
            ui.add_space(4.0);
            theme::eyebrow(ui, "Skin — omit to wear the donor's materials");
            for (lbl, slot) in [
                ("Diffuse", &mut textures.diffuse),
                ("Normal", &mut textures.normal),
                ("Specular", &mut textures.specular),
            ] {
                commit |= optional_source_row(ui, lbl, slot, root, &["png"]);
            }
            commit |= retarget_summary(ui, retarget);
        }
        Contribution::AddTexture { name, image, normal_map } => {
            commit |= text_row(ui, "Asset name", name, "my_custom_decal", true);
            commit |= source_row(ui, "Image", image, root, &["png"]);
            ui.horizontal(|ui| {
                let lw = theme::field_label_w(ui.available_width());
                ui.add_space(lw + 4.0);
                if theme::pill(ui, "normal map", *normal_map).clicked() {
                    *normal_map = !*normal_map;
                    commit = true;
                }
            });
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                if *normal_map {
                    "DXT5nm, R=1 G=ny B=1 A=nx — matches the preview encoder"
                } else {
                    "BC1, or BC3 when the image carries real alpha"
                },
            );
        }
        Contribution::AddSound { name, bank, sound } => {
            use mercs2_quartermaster::manifest::SoundKind;
            commit |= text_row(ui, "Asset name", name, "amb_myjungle", true);
            commit |= source_row(ui, "Bank", bank, root, &[]);
            commit |= theme::combo_field(
                ui,
                "Table",
                sound,
                &[
                    (SoundKind::Wavebank, "wavebank"),
                    (SoundKind::Soundbank, "soundbank"),
                    (SoundKind::Sounddb, "sounddb"),
                ],
                theme::FieldState::Neutral,
            );
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "the bytes ship verbatim — nothing here encodes audio, so the bank must already                  be one the game accepts",
            );
        }
        Contribution::AddMovie { name, movie } => {
            commit |= text_row(ui, "Asset name", name, "my_menu", true);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a NAME, not a filename — retail's are bare (`topbar`, `pause_menu`)",
            );
            commit |= source_row(ui, "Movie", movie, root, &["gfx", "cfx", "swf"]);
        }
        Contribution::ReplaceTexture { target, image } => {
            commit |= asset_row(ui, "Target", target, names);
            commit |= source_row(ui, "Image", image, root, &["png"]);
        }
        Contribution::PatchLua { target, append } => {
            commit |= text_row(ui, "Target script", target, "wifpmcinterior", true);
            commit |= source_row(ui, "Append", append, root, &["lua"]);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a declared MUTATION — relinked across every installed Shipment at deploy",
            );
        }
        Contribution::EditStateMachine { target, states } => {
            commit |= asset_row(ui, "Target", target, names);
            commit |= source_row(ui, "States", states, root, &["yaml", "yml"]);
        }
        Contribution::EditStringDb { target, strings } => {
            commit |= asset_row(ui, "Target table", target, names);
            commit |= source_row(ui, "Strings", strings, root, &["txt"]);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "one `[Bracket.Key] = New text` per line. Same-hash, last-wins.",
            );
        }
        Contribution::NativeHook { target, plugin, symbol, touches } => {
            // `both` is reserved and rejected in v1, so it is not offered.
            commit |= theme::combo_field(
                ui,
                "Engine",
                target,
                &[(Target::Retail, "retail"), (Target::Reimpl, "reimpl")],
                theme::FieldState::Neutral,
            );
            let mut p = plugin.clone().unwrap_or_default();
            if source_row(ui, "Plugin", &mut p, root, &["asi"]) {
                *plugin = (!p.as_os_str().is_empty()).then_some(p);
                commit = true;
            }
            // The builder chooses the destination; there is no `dest` field, which is what keeps
            // `Mercenaries2.exe` and `data/vz.wad` unreachable by construction.
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "placed in scripts/ — the loader's own glob. There is no destination to choose.",
            );
            let mut s = symbol.clone().unwrap_or_default();
            if text_row(ui, "Symbol", &mut s, "optional", false) {
                *symbol = (!s.trim().is_empty()).then_some(s);
                commit = true;
            }
            if plugin.is_none() && symbol.is_none() {
                theme::field_note(
                    ui,
                    theme::FieldState::Bad,
                    "M0161 — supply a plugin, a symbol, or both",
                );
            }
            ui.add_space(4.0);
            theme::eyebrow(ui, "Declared blast radius");
            commit |= touches_editor(ui, touches, false);
        }
        Contribution::PlaceFile { file, dest } => {
            commit |= source_row(ui, "File", file, root, &[]);
            // Live, against the builder's OWN refusals rather than a copy of them.
            if let Some(n) = file.file_name().map(|s| s.to_string_lossy().to_string()) {
                if let Some(why) = mercs2_quartermaster::build::companion_name_refusal(&n) {
                    theme::field_note(ui, theme::FieldState::Bad, &format!("M0162 — {why}"));
                }
            }
            commit |= theme::combo_field(
                ui,
                "Destination",
                dest,
                &[
                    (PlaceIn::GameRoot, "game root"),
                    (PlaceIn::Scripts, "scripts/"),
                    (PlaceIn::Plugins, "plugins/"),
                    (PlaceIn::Update, "update/"),
                    (PlaceIn::OnBoot, "scripts/OnBoot"),
                    (PlaceIn::OnLoad, "scripts/OnLoad"),
                    (PlaceIn::OnKey, "scripts/OnKey"),
                ],
                theme::FieldState::Neutral,
            );
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a NAME from a closed set, never a path — that is the security property",
            );
        }
        Contribution::Raw { description, payload, target_layer, touches } => {
            let mut d = description.clone().unwrap_or_default();
            if text_row(ui, "Description", &mut d, "what these bytes are", false) {
                *description = (!d.trim().is_empty()).then_some(d);
                commit = true;
            }
            commit |= source_row(ui, "Payload", payload, root, &[]);
            commit |= theme::combo_field(
                ui,
                "Layer",
                target_layer,
                &[
                    (Layer::Data, "data"),
                    (Layer::Script, "script"),
                    (Layer::Code, "code"),
                    (Layer::Runtime, "runtime"),
                ],
                if *target_layer == Layer::Data {
                    theme::FieldState::Neutral
                } else {
                    theme::FieldState::Bad
                },
            );
            if *target_layer != Layer::Data {
                theme::field_note(
                    ui,
                    theme::FieldState::Bad,
                    "only `data` lowers — the overlay is a WAD, and that is the only layer a WAD \
                     holds. Use patch_lua / native_hook / place_file instead.",
                );
            }
            ui.add_space(4.0);
            theme::eyebrow(ui, "Declared blast radius — must match the payload exactly");
            commit |= touches_editor(ui, touches, true);
        }
    }
    commit
}

/// The Shipment's OWN identity — what it is called, who wrote it, and how it orders against others.
///
/// Shown where "pick a contribution" used to be. That space was doing nothing, and these fields had
/// no editor at all: `shipment.name` decides the output filename (`build/<name>.wad`) and every
/// cross-Shipment reference, and was reachable only by hand-editing YAML.
fn identity_form(
    ui: &mut egui::Ui,
    sh: &mut mercs2_quartermaster::manifest::Shipment,
    load: &mut mercs2_quartermaster::manifest::Load,
) -> bool {
    use mercs2_quartermaster::manifest::{Target, MAX_NAME_LEN};
    let mut commit = false;

    // Badge cloned out first: `section` holds it across the closure that also mutates `sh`.
    let badge = sh.version.clone();
    theme::section(ui, "Identity", Some(&badge), true, |ui| {
        // The slug rule is enforced in the WIDGET, not left to M0100: this string becomes a
        // filename, so an invalid one is a build that cannot name its own output.
        let slug_ok = !sh.name.is_empty()
            && sh.name.len() <= MAX_NAME_LEN
            && !sh.name.starts_with('-')
            && !sh.name.ends_with('-')
            && !sh.name.contains("--")
            && sh
                .name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        let st = if slug_ok { theme::FieldState::Good } else { theme::FieldState::Bad };
        commit |= theme::text_field(ui, "Name", &mut sh.name, "my-shipment", st).lost_focus();
        if slug_ok {
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                &format!("builds to build/{}.wad", sh.name),
            );
        } else {
            theme::field_note(
                ui,
                theme::FieldState::Bad,
                "M0100 — lowercase, digits and single hyphens only (it becomes the filename)",
            );
        }

        let mut title = sh.title.clone().unwrap_or_default();
        if theme::text_field(ui, "Title", &mut title, "My Shipment", theme::FieldState::Neutral)
            .lost_focus()
        {
            sh.title = (!title.trim().is_empty()).then_some(title);
            commit = true;
        }
        let vst = if sh.version.trim().is_empty() {
            theme::FieldState::Bad
        } else {
            theme::FieldState::Neutral
        };
        commit |= theme::text_field(ui, "Version", &mut sh.version, "1.0.0", vst).lost_focus();

        let mut desc = sh.description.clone().unwrap_or_default();
        if theme::text_field(ui, "Description", &mut desc, "what it does", theme::FieldState::Neutral)
            .lost_focus()
        {
            sh.description = (!desc.trim().is_empty()).then_some(desc);
            commit = true;
        }

        let mut authors = sh.authors.join(", ");
        if theme::text_field(ui, "Authors", &mut authors, "you, someone else", theme::FieldState::Neutral)
            .lost_focus()
        {
            sh.authors = authors
                .split(',')
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect();
            commit = true;
        }

        // `both` is reserved and rejected in v1, so it is not offered — the same reason the
        // native_hook engine picker omits it.
        commit |= theme::combo_field(
            ui,
            "Target",
            &mut sh.target,
            &[(Target::Retail, "retail"), (Target::Reimpl, "reimpl")],
            theme::FieldState::Neutral,
        );

        let mut lic = sh.license.clone().unwrap_or_default();
        if theme::text_field(ui, "License", &mut lic, "MIT", theme::FieldState::Neutral).lost_focus() {
            sh.license = (!lic.trim().is_empty()).then_some(lic);
            commit = true;
        }
        let mut home = sh.homepage.clone().unwrap_or_default();
        if theme::text_field(ui, "Homepage", &mut home, "https://…", theme::FieldState::Neutral)
            .lost_focus()
        {
            sh.homepage = (!home.trim().is_empty()).then_some(home);
            commit = true;
        }
        let mut tags = sh.tags.join(", ");
        if theme::text_field(ui, "Tags", &mut tags, "outfit, character", theme::FieldState::Neutral)
            .lost_focus()
        {
            sh.tags = tags
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            commit = true;
        }
    });

    theme::section(ui, "Load order", None, false, |ui| {
        ui.label(
            egui::RichText::new(
                "Names of other Shipments. `after`/`before` constrain the deploy-time link order; \
                 `conflicts` declares one that cannot be installed alongside this.",
            )
            .size(11.0)
            .color(theme::FAINT),
        );
        ui.add_space(6.0);
        for (lbl, list) in [
            ("After", &mut load.after),
            ("Before", &mut load.before),
            ("Conflicts", &mut load.conflicts),
        ] {
            let mut joined = list.join(", ");
            if theme::text_field(ui, lbl, &mut joined, "other-shipment", theme::FieldState::Neutral)
                .lost_focus()
            {
                *list = joined
                    .split(',')
                    .map(|x| x.trim().to_string())
                    .filter(|x| !x.is_empty())
                    .collect();
                commit = true;
            }
        }
    });

    commit
}

/// An optional `src/` file (a texture slot). Absent is a meaningful value, so the widget owns its
/// own clear button — nesting one beside `path_field` put the two in separate horizontal layouts.
fn optional_source_row(
    ui: &mut egui::Ui,
    label: &str,
    slot: &mut Option<PathBuf>,
    root: &Path,
    filters: &[&str],
) -> bool {
    let (state, note) = match slot.as_deref() {
        None => (theme::FieldState::Neutral, None),
        Some(rel) => source_state(root, rel),
    };
    let changed = theme::opt_path_field(ui, label, slot, root, filters, state);
    if let Some(n) = note {
        theme::field_note(ui, state, &n);
    }
    changed
}

/// The retarget sub-block, read-only plus a way in.
///
/// The bone map is not hand-editable here on purpose: it is produced by the rig bench, where a bone
/// can be seen. What this shows is whether one is recorded, because a Shipment carrying only
/// `from:` rebuilds to something the author never approved.
fn retarget_summary(
    ui: &mut egui::Ui,
    rt: &mut Option<mercs2_quartermaster::manifest::Retarget>,
) -> bool {
    ui.add_space(4.0);
    match rt {
        None => {
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "No retarget — the source is assumed already hero-rigged.",
            );
            false
        }
        Some(r) => {
            let n = r.bones.as_ref().map(|b| b.len()).unwrap_or(0);
            theme::kv(ui, "retarget from", egui::RichText::new(&r.from));
            theme::kv(ui, "bone rows", egui::RichText::new(n.to_string()));
            if n == 0 {
                theme::field_note(
                    ui,
                    theme::FieldState::Warn,
                    "no bone map recorded — a rebuild will re-derive it and may differ from what \
                     you approved",
                );
            }
            false
        }
    }
}

fn empty_middle(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(90.0);
        ui.label(theme::disp_text(title, 13.0, theme::FAINT));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(sub).size(12.0).color(theme::FAINT));
    });
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
        Contribution::AddTexture { name, .. } => {
            vec![("Writes".to_string(), format!("texture {name}  (new hash)"))]
        }
        Contribution::AddSound { name, sound, .. } => vec![(
            "Writes".to_string(),
            format!("{} {name}  (new hash)", format!("{sound:?}").to_lowercase()),
        )],
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
        Contribution::EditStringDb { target, .. } => vec![
            ("Writes".to_string(), format!("stringdb {target}")),
            ("Merge".to_string(), "last-wins \u{2014} load order is the answer".to_string()),
        ],
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
            row(ui, if i == 0 { "base" } else { "overlay" }, &leaf(Path::new(w)), theme::DIM);
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
            // The overlay, then its size and digest as their OWN rows. Using the filename as a
            // key made it a 19-character label in a 78px column, so it ran across its own value.
            if let Some(w) = &r.wad {
                let name = leaf(w);
                row(ui, "Overlay", &name, theme::TX);
                if let Some(pl) = r.placements.iter().find(|p| p.name == name) {
                    row(ui, "Size", &human_bytes(pl.bytes), theme::DIM);
                    row_head(ui, "sha256", &pl.sha256, theme::DIM);
                }
            }
            // Anything that is NOT the overlay — an .asi and its companions — and where it goes.
            let overlay = r.wad.as_ref().map(|w| leaf(w));
            for pl in r.placements.iter().filter(|p| Some(&p.name) != overlay.as_ref()) {
                row(
                    ui,
                    &pl.name,
                    &format!("{} \u{b7} {:?}", human_bytes(pl.bytes), pl.destination),
                    theme::DIM,
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
    if ui.button("New").on_hover_text("Scaffold manifest.yaml + src/ in an empty folder").clicked() {
        acts.push(Act::New);
    }
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
        // The stub is schema-valid and deliberately NOT lint-clean: its placeholder paths do not
        // exist, so M0110 fires straight away and the panel tells the author what it still needs.
        Act::Add(kind) => {
            let Some(c) = stub(kind, p.next_stub_index()) else {
                p.error = Some(format!("no stub for `{kind}`"));
                return;
            };
            match p.mutate(names, |m| m.contributions.push(c)) {
                Ok(()) => *status = p.status.clone(),
                Err(e) => {
                    p.error = Some(e);
                    *status = "could not write the manifest".into();
                }
            }
        }
        // Handled by the caller, which owns the workbench: the panel cannot switch pages itself.
        Act::Craft(..) => {}
        Act::SetWearer(i, w) => match p.mutate(names, |m| {
            if let Some(Contribution::AddOutfit { wearer, .. }) = m.contributions.get_mut(i) {
                *wearer = w.to_string();
            }
        }) {
            Ok(()) => *status = p.status.clone(),
            Err(e) => {
                p.error = Some(e);
                *status = "could not write the manifest".into();
            }
        },
        Act::Edit(i, c) => match p.upsert_contribution(names, Some(i), *c) {
            Ok(_) => *status = p.status.clone(),
            Err(e) => {
                p.error = Some(e);
                *status = "could not write the manifest".into();
            }
        },
        Act::EditIdentity(sh, load) => match p.mutate(names, |m| {
            m.shipment = *sh;
            m.load = *load;
        }) {
            Ok(()) => *status = p.status.clone(),
            Err(e) => {
                p.error = Some(e);
                *status = "could not write the manifest".into();
            }
        },
        Act::New => {
            if let Some(dir) = rfd::FileDialog::new()
                .set_title("New Shipment — pick an empty folder")
                .pick_folder()
            {
                match p.scaffold(&dir, names) {
                    Ok(()) => *status = p.status.clone(),
                    Err(e) => {
                        p.error = Some(e);
                        *status = "could not scaffold a Shipment".into();
                    }
                }
            }
        }
        Act::Remove(i) => match p.mutate(names, |m| {
            if i < m.contributions.len() {
                m.contributions.remove(i);
            }
        }) {
            Ok(()) => *status = p.status.clone(),
            Err(e) => {
                p.error = Some(e);
                *status = "could not write the manifest".into();
            }
        },
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

/// Where the Workshop drops a Shipment for Modkit to find: **Modkit's own data root**.
///
/// This used to be `%LOCALAPPDATA%/mercs2/shipments` — a location invented here that Modkit never
/// reads, so "Send to Modkit" wrote into the void. Modkit keeps its state under
/// `%APPDATA%/mercs2-modkit/` (`staging`, `deployed`, `bin`, …), so `shipments/` belongs beside
/// those, in the layout Modkit already owns.
///
/// A folder both apps agree on IS the integration. A deep link would only be convenience over it,
/// and needs two Tauri plugins Modkit does not carry. Nothing here writes into a game folder —
/// install and undo stay Modkit's job, with the placement record to match.
pub fn shipments_library() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from)?;
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("mercs2-modkit").join("shipments"))
}

fn send_to_modkit(p: &Panel) -> Result<PathBuf, String> {
    let root = p.root().ok_or("no shipment open")?;
    let lib = shipments_library().ok_or("no home directory to place the shipments library in")?;
    let dest = lib.join(root.file_name().ok_or("the shipment folder has no name")?);
    // Copying a folder INTO itself walks forever and fills the disk, so refuse rather than trust
    // that the library never overlaps the Shipment.
    if dest.starts_with(root) || root.starts_with(&dest) {
        return Err(format!(
            "this Shipment already lives in the library at {} — nothing to send",
            dest.display()
        ));
    }
    copy_tree(root, &dest).map_err(|e| format!("copying the shipment: {e}"))?;
    // Open it, because Modkit has no watcher and no deep link: the handoff ends with a person
    // adding it, so the folder had better be in front of them.
    let _ = open_in_os(&dest);
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

pub fn open_in_os(target: &Path) -> std::io::Result<()> {
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

// ─────────────────────────────────────────────────────────────────────────────────────── tests
//
// Unit tests rather than an integration test: `mercs2_workshop` is a BINARY crate with no
// `src/lib.rs`, so `tests/` can only reach other crates. These need `Panel`'s own methods.

#[cfg(test)]
mod tests {
    use super::*;
    use mercs2_quartermaster::manifest::{Contribution, Retarget as QmRetarget, Textures};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mercs2_qm_test_{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn outfit(model: &str, bone_target: &str) -> Contribution {
        let mut bones = std::collections::BTreeMap::new();
        bones.insert("bip01_spine".to_string(), Some(bone_target.to_string()));
        bones.insert("bip01_tail".to_string(), None);
        Contribution::AddOutfit {
            name: "my_asset".into(),
            slug: "MyAsset".into(),
            display: "My asset".into(),
            wearer: "mattias".into(),
            model: PathBuf::from(model),
            donor: Some("pmc_hum_mattias".into()),
            textures: Textures::default(),
            retarget: Some(QmRetarget { from: "mixamo".into(), bones: Some(bones) }),
        }
    }

    /// Nothing in the workspace scaffolded a Shipment before this — `qm` has no `init` and
    /// `discover` is read-only — so a craft bench had nowhere to put its work.
    #[test]
    fn scaffold_writes_a_shipment_that_opens_and_validates() {
        let d = tmp("scaffold");
        let mut p = Panel::default();
        p.scaffold(&d, None).expect("scaffold");

        assert!(d.join("manifest.yaml").is_file(), "no manifest");
        assert!(d.join("src").is_dir(), "no src/");
        assert!(d.join("README.md").is_file(), "no README");
        assert_eq!(p.root(), Some(d.as_path()), "not opened");
        let s = p.shipment.as_ref().expect("opened");
        // The folder name becomes a SLUG, not the name verbatim: it is also `build/<name>.wad`.
        assert_eq!(s.manifest.shipment.name, "mercs2-qm-test-scaffold");
        s.manifest.validate().expect("scaffolded manifest must validate");
        assert!(s.manifest.contributions.is_empty());
    }

    /// Reached from a folder picker, so picking the wrong folder must not destroy someone's work.
    #[test]
    fn scaffold_refuses_to_overwrite_an_existing_manifest() {
        let d = tmp("nooverwrite");
        let mut p = Panel::default();
        p.scaffold(&d, None).unwrap();
        std::fs::write(d.join("manifest.yaml"), "format: 1\n# hand-edited\n").unwrap();

        let mut q = Panel::default();
        let err = q.scaffold(&d, None).expect_err("must refuse");
        assert!(err.contains("already holds a manifest"), "{err}");
        let text = std::fs::read_to_string(d.join("manifest.yaml")).unwrap();
        assert!(text.contains("# hand-edited"), "the existing manifest was clobbered");
    }

    #[test]
    fn upsert_appends_then_replaces_in_place() {
        let d = tmp("upsert");
        let mut p = Panel::default();
        p.scaffold(&d, None).unwrap();

        let at = p.upsert_contribution(None, None, outfit("src/a.glb", "Bone_Chest")).unwrap();
        assert_eq!(at, 0);
        assert_eq!(p.shipment.as_ref().unwrap().manifest.contributions.len(), 1);

        let at2 = p.upsert_contribution(None, None, outfit("src/b.glb", "Bone_Chest")).unwrap();
        assert_eq!(at2, 1, "second add must APPEND");
        assert_eq!(p.shipment.as_ref().unwrap().manifest.contributions.len(), 2);

        // Replacing index 0 must not grow the list — this is the path a craft bench takes when it
        // re-commits work on a contribution the author entered it from.
        p.upsert_contribution(None, Some(0), outfit("src/c.glb", "Bone_Chest")).unwrap();
        let m = &p.shipment.as_ref().unwrap().manifest;
        assert_eq!(m.contributions.len(), 2, "replace must not append");
        match &m.contributions[0] {
            Contribution::AddOutfit { model, .. } => assert_eq!(model, &PathBuf::from("src/c.glb")),
            other => panic!("wrong kind: {}", other.kind()),
        }
    }

    /// ★ The regression this rewrite exists for.
    ///
    /// The old emitter built YAML with `format!` and its `yaml_scalar` guard was never called, so a
    /// target bone with no known name went out as a BARE `0xE54047D5`. 21 of `pmc_hum_mattias`'s
    /// 116 bones have no name in any corpus here, so this is the normal case, not an edge one — and
    /// a bare `0x…` scalar is exactly what YAML may read back as something other than a string.
    #[test]
    fn a_bare_hash_bone_target_survives_the_yaml_round_trip_as_a_string() {
        let d = tmp("barehash");
        let mut p = Panel::default();
        p.scaffold(&d, None).unwrap();
        p.upsert_contribution(None, None, outfit("src/a.glb", "0xE54047D5")).unwrap();

        // Re-read from DISK, not from memory: the question is what the file says.
        let mut q = Panel::default();
        q.open_shipment(&d, None);
        let m = &q.shipment.as_ref().expect("re-opened").manifest;
        let Contribution::AddOutfit { retarget: Some(rt), .. } = &m.contributions[0] else {
            panic!("kind changed across the round trip");
        };
        let bones = rt.bones.as_ref().expect("bones dropped");
        assert_eq!(
            bones.get("bip01_spine").and_then(|o| o.as_deref()),
            Some("0xE54047D5"),
            "bare hash did not survive as a string"
        );
        // `~` (drop this bone) has to survive as an explicit null, not vanish.
        assert!(bones.contains_key("bip01_tail"), "the dropped-bone row disappeared");
        assert_eq!(bones.get("bip01_tail").unwrap(), &None);
    }

    /// ★ Every kind the FORMAT knows must be offerable from the UI, and must produce a stub.
    ///
    /// This is the drift the whole crate keeps producing: `edit_state_machine` parsed, claimed a
    /// blast radius and had linter rules, while being absent from `KINDS` and from `stub()` — so
    /// there was no way to add one. Nothing failed; it simply was not there. Same shape as the rail
    /// indexing a 6-entry icon array with a 4-entry workbench list.
    #[test]
    fn the_add_menu_offers_every_kind_the_format_knows() {
        let offered: std::collections::BTreeSet<&str> =
            KINDS.iter().flat_map(|(_, ks)| ks.iter().map(|(k, _)| *k)).collect();
        let missing: Vec<&&str> = Contribution::ALL_KINDS
            .iter()
            .filter(|k| !offered.contains(**k))
            .collect();
        assert!(missing.is_empty(), "kinds the UI cannot add: {missing:?}");

        // And every offered kind must actually build one, or the menu entry is a dead button.
        for k in &offered {
            let c = stub(k, 1).unwrap_or_else(|| panic!("`{k}` is offered but has no stub"));
            assert_eq!(&c.kind(), k, "stub for `{k}` produced a {} instead", c.kind());
        }
    }

    /// The queue menu groups kinds by layer; a kind in two groups would show up twice.
    #[test]
    fn no_kind_is_offered_twice() {
        let mut seen = std::collections::BTreeSet::new();
        for (_, ks) in KINDS {
            for (k, _) in *ks {
                assert!(seen.insert(*k), "`{k}` is listed in more than one layer group");
            }
        }
    }

    #[test]
    fn import_source_copies_dedupes_and_suffixes() {
        let d = tmp("import");
        let mut p = Panel::default();
        p.scaffold(&d, None).unwrap();
        let ext = tmp("import_ext");

        let a = ext.join("model.glb");
        std::fs::write(&a, b"AAAA").unwrap();
        let r1 = Panel::import_source(&d, &a).unwrap();
        assert_eq!(r1, PathBuf::from("src").join("model.glb"));
        assert_eq!(std::fs::read(d.join(&r1)).unwrap(), b"AAAA");

        // Same name, same bytes -> reuse rather than pile up copies.
        let r2 = Panel::import_source(&d, &a).unwrap();
        assert_eq!(r2, r1, "identical file should not be duplicated");

        // Same name, DIFFERENT bytes -> suffix, never overwrite.
        let b = tmp("import_ext2").join("model.glb");
        std::fs::write(&b, b"BBBB").unwrap();
        let r3 = Panel::import_source(&d, &b).unwrap();
        assert_ne!(r3, r1, "a different file must not overwrite");
        assert_eq!(std::fs::read(d.join(&r1)).unwrap(), b"AAAA", "original was overwritten");
        assert_eq!(std::fs::read(d.join(&r3)).unwrap(), b"BBBB");
    }
}
