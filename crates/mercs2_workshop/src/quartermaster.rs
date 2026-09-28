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
use mercs2_quartermaster::manifest::{
    CapabilityReq, ConflictDecl, Contribution, Requirement, ShipmentReq, SoundCue, Touch,
};
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

/// What a craft bench needs in order to open ON the contribution it was entered from, rather than
/// blank. Resolved by [`Panel::craft_subject`] from the manifest at the moment of entry.
///
/// This closes the gap that made "Edit rig" / "Conform" land on an empty page: `Act::Craft` carried
/// the contribution INDEX (so a commit could write back) but nothing ever read the contribution to
/// LOAD its model and donor into the bench. The subject is that missing half.
pub(crate) struct CraftSubject {
    /// Absolute path to the source model GLB (the Shipment root joined with the `src/`-relative
    /// `model`). This is what the bench imports as its pedestal / retarget source.
    pub model: PathBuf,
    /// The donor host, resolved to `(asset hash, label)` the same way the CLI does — a `0x…` donor
    /// is parsed as hex, otherwise it is `pandemic_hash_m2` of the name (leading `_` trimmed). This
    /// is the retarget TARGET (Rig bench) or the conform HOST (Conform bench). `None` when the
    /// contribution left `donor` to be auto-picked at build time.
    pub donor: Option<(u32, String)>,
    /// The convention the saved `retarget:` recorded, if any — so the bench can note that a bone map
    /// already exists (its hand edits are not yet re-applied; the import re-derives the auto map).
    pub from: Option<String>,
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

/// Facts read straight out of a source GLB — the "what is this file" an outfit/model form shows so an
/// author is not staring at blank fields for things baked into the asset (its rig, its materials, its
/// size). A LIGHT probe: it parses only the glTF JSON via [`gltf::Gltf::open`], never the model's
/// buffers or images, so it is cheap enough to cache per path and render live.
#[derive(Clone)]
pub struct GlbFacts {
    pub rig: mercs2_formats::retarget::SourceRig,
    pub joints: usize,
    pub materials: usize,
    pub embedded_textures: bool,
    pub verts: usize,
    pub tris: usize,
}

impl GlbFacts {
    /// A FOREIGN rig needs a `retarget:` to animate on the donor; a native/generic one does not.
    pub fn foreign(&self) -> bool {
        use mercs2_formats::retarget::SourceRig::*;
        matches!(self.rig, ValveBiped | Mixamo | Unreal | CallOfDuty)
    }

    /// One-line summary for a field note.
    fn summary(&self) -> String {
        format!(
            "{} \u{b7} {} joint{} \u{b7} {} material{} {}\u{b7} {} verts / {} tris",
            self.rig.label(),
            self.joints,
            if self.joints == 1 { "" } else { "s" },
            self.materials,
            if self.materials == 1 { "" } else { "s" },
            if self.embedded_textures { "(embedded textures) " } else { "" },
            self.verts,
            self.tris,
        )
    }
}

/// Read a source GLB's shape without decoding its payload. `None` when it is not a readable glTF (an
/// OBJ, or an unreadable file) — the caller simply shows nothing.
fn probe_glb(abs: &Path) -> Option<GlbFacts> {
    let g = gltf::Gltf::open(abs).ok()?;
    let mut names: Vec<String> = Vec::new();
    for skin in g.skins() {
        for joint in skin.joints() {
            if let Some(n) = joint.name() {
                names.push(n.to_string());
            }
        }
    }
    let materials = g.materials().count();
    let embedded_textures = g
        .materials()
        .any(|m| m.pbr_metallic_roughness().base_color_texture().is_some());
    let (mut verts, mut tris) = (0usize, 0usize);
    for mesh in g.meshes() {
        for prim in mesh.primitives() {
            if let Some(a) = prim.get(&gltf::Semantic::Positions) {
                verts += a.count();
            }
            tris += prim.indices().map(|a| a.count() / 3).unwrap_or(0);
        }
    }
    Some(GlbFacts {
        rig: mercs2_formats::retarget::SourceRig::detect(&names),
        joints: names.len(),
        materials,
        embedded_textures,
        verts,
        tris,
    })
}

/// The source model an outfit/model contribution imports, if it has one.
fn model_source(c: &Contribution) -> Option<&Path> {
    match c {
        // An existing-model outfit carries no source file, so it has no model to import.
        Contribution::AddOutfit { model, .. } => model.as_deref(),
        Contribution::AddModel { model, .. } => Some(model.as_path()),
        _ => None,
    }
}

/// Which contribution the facts note is advising, so the recommendation fits the kind.
enum GlbAdvice {
    Outfit { single_group: bool },
    Model,
}

/// Render the GLB facts line under the Model field plus a recommendation keyed to the kind — so the
/// author sees what the file IS (rig, materials, size) and what to do about it, instead of blank
/// fields for things the GLB already answers.
fn glb_facts_note(ui: &mut egui::Ui, facts: Option<&GlbFacts>, advice: GlbAdvice) {
    let Some(f) = facts else { return };
    theme::field_note(ui, theme::FieldState::Neutral, &f.summary());
    match advice {
        GlbAdvice::Outfit { single_group } => {
            if f.foreign() && !single_group {
                theme::field_note(
                    ui,
                    theme::FieldState::Warn,
                    "foreign rig — if it culls or teleports in-game, turn on single group",
                );
            }
        }
        GlbAdvice::Model => {
            if f.foreign() {
                theme::field_note(
                    ui,
                    theme::FieldState::Warn,
                    &format!(
                        "this GLB carries a {} rig — a character that animates belongs in \
                         add_outfit (which retargets it onto a hero); add_model hosts it rigidly",
                        f.rig.label()
                    ),
                );
            }
        }
    }
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
    /// Cache of light GLB probes keyed by ABSOLUTE source path, so the facts card does not re-read a
    /// 16 MB file every frame. A `None` value = probed and not a readable glTF.
    model_facts: std::cell::RefCell<std::collections::HashMap<PathBuf, Option<GlbFacts>>>,
    /// In-flight background build. The worker sends its [`BuildOutcome`] here; [`Panel::poll_build`]
    /// drains it. `building` is what the verb bar reads to show a spinner instead of the Build
    /// button. Both are `Default` (`None`/`false`), so the `#[derive(Default)]` above still holds.
    build_rx: Option<std::sync::mpsc::Receiver<BuildOutcome>>,
    building: bool,
}

impl Panel {
    /// Light GLB facts for an absolute source path, probed once and cached (interior mutability, so
    /// it works through the `&Panel` the render borrows).
    fn model_facts_for(&self, abs: &Path) -> Option<GlbFacts> {
        if let Some(hit) = self.model_facts.borrow().get(abs) {
            return hit.clone();
        }
        let facts = probe_glb(abs);
        self.model_facts
            .borrow_mut()
            .insert(abs.to_path_buf(), facts.clone());
        facts
    }

    pub(crate) fn blocks(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| matches!(d.severity, Severity::Error | Severity::Hang))
    }

    /// True while a background build is running — the verb bar shows a spinner instead of Build, and
    /// the app polls [`Panel::poll_build`] every frame until it clears.
    pub fn building(&self) -> bool {
        self.building
    }

    /// Fold a finished build's outcome back into the panel. Shared by the synchronous [`run_build`]
    /// and the async [`Panel::poll_build`], so the report/blocked/failed classification lives once.
    fn apply_build_outcome(&mut self, outcome: BuildOutcome) {
        match outcome {
            BuildOutcome::Report(r) => {
                self.status = match &r.wad {
                    Some(w) => format!("built {}", leaf(w)),
                    None => "built (no overlay)".into(),
                };
                self.diagnostics = r.diagnostics.clone();
                self.error = None;
                self.report = Some(r);
            }
            BuildOutcome::Blocked(ds) => {
                self.diagnostics = ds;
                self.report = None;
                self.error = None;
                self.status = "blocked".into();
            }
            BuildOutcome::Failed(msg) => {
                self.report = None;
                self.error = Some(msg);
                self.status = "build failed".into();
            }
        }
    }

    /// Drain the in-flight build if it has finished. Returns `true` on the frame it completes, so the
    /// caller can sync its own status line. A cheap no-op when no build is running.
    pub fn poll_build(&mut self) -> bool {
        let Some(rx) = &self.build_rx else { return false };
        match rx.try_recv() {
            Ok(outcome) => {
                self.build_rx = None;
                self.building = false;
                self.apply_build_outcome(outcome);
                true
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
            // The worker panicked and dropped its sender without sending. Surface it rather than
            // spin forever showing a spinner for a build that will never report.
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.build_rx = None;
                self.building = false;
                self.error = Some("the build thread stopped unexpectedly".into());
                self.status = "build failed".into();
                true
            }
        }
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
            supersedes: Vec::new(),
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

    /// The load target for a craft bench entered from contribution `i`: its source model and its
    /// resolved donor. `None` when there is no open Shipment, no such contribution, or the
    /// contribution is not one a bench edits (only `add_outfit` / `add_model` carry a model + donor).
    ///
    /// Without this, entering a bench was a page flip that carried an index and nothing else — the
    /// bench had the identity of what it was editing but not the bytes, so it opened empty.
    pub(crate) fn craft_subject(&self, i: usize) -> Option<CraftSubject> {
        let sh = self.shipment.as_ref()?;
        let c = sh.manifest.contributions.get(i)?;
        let (model, donor, retarget) = match c {
            // An existing-model outfit (no `model` file) has no source mesh to conform, so there is
            // no craft bench for it.
            Contribution::AddOutfit { model: Some(model), donor, retarget, .. } => {
                (model, donor, retarget)
            }
            Contribution::AddModel { model, donor, retarget, .. } => (model, donor, retarget),
            _ => return None,
        };
        let donor = donor.as_ref().map(|d| {
            let h = d
                .strip_prefix("0x")
                .and_then(|x| u32::from_str_radix(x, 16).ok())
                .unwrap_or_else(|| {
                    mercs2_formats::hash::pandemic_hash_m2(d.trim_start_matches('_'))
                });
            (h, d.clone())
        });
        Some(CraftSubject {
            model: sh.root.join(model),
            donor,
            from: retarget.as_ref().map(|r| r.from.clone()),
        })
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
        | Contribution::AddMovie { name, .. }
        | Contribution::AddLanguage { name, .. }
        | Contribution::AddUi { name, .. } => name.clone(),
        Contribution::AddShopItem { id, .. } => id.clone(),
        Contribution::AddSound { bank, .. } => bank.clone(),
        Contribution::ReplaceSoundBank { bank, language, .. } => {
            mercs2_quartermaster::sound::entry_name(bank, *language)
        }
        Contribution::ReplaceSoundCue { bank, language, cue, .. } => format!(
            "{} {}",
            mercs2_quartermaster::sound::entry_name(bank, *language),
            cue.name
        ),
        Contribution::ReplaceTexture { target, .. }
        | Contribution::PatchLua { target, .. }
        | Contribution::EditStateMachine { target, .. }
        | Contribution::EditStringDb { target, .. } => target.clone(),
        Contribution::EditWorld { layer, .. } => layer.clone(),
        Contribution::ActivateLayer { layer, .. } => layer.clone(),
        // `NativeHook.target` is the ENGINE, not an asset — so name it by what it actually is.
        Contribution::NativeHook { plugin, symbol, .. } => plugin
            .as_ref()
            .map(|f| leaf(f))
            .or_else(|| symbol.clone())
            .unwrap_or_else(|| "native hook".into()),
        Contribution::PlaceFile { file, .. } => leaf(file),
        Contribution::AddRuntimeDll { dll } => leaf(dll),
        Contribution::Raw { payload, .. } => leaf(payload),
        // New Contribution kinds (add_script/replace_lua/replace_phy2/add_placement/add_layer/
        // add_animation/replace_animation/add_shader/replace_shader/add_fx/replace_fx/
        // replace_terrain_cell/add_stringdb_keys/replace_stringdb_text)
        // don't have first-class workshop UI yet; fall back to the machine tag.
        other => other.kind().to_string(),
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
            ("add_sound", "A new sound bank encoded from WAV cues, loaded by the mod loader"),
            ("replace_sound_bank", "Replace every cue of a shipped sound bank, same entry name"),
            ("replace_sound_cue", "Replace one cue of a shipped sound bank with a new WAV"),
            ("add_movie", "A Scaleform movie"),
            ("add_ui", "A Scaleform movie, wired up so it appears on screen"),
            ("replace_texture", "Replace a shipped texture, same hash"),
            ("edit_state_machine", "Rewrite a destructible's states"),
            ("edit_world", "Move / rotate / re-model a layer's placed entities"),
            ("edit_stringdb", "Correct or localise UI text"),
            ("replace_phy2", "Swap a shipped model's collision (PHY2), same hash"),
            ("add_placement", "Add one entity to an existing layer"),
            ("add_layer", "Mint a whole new placement layer"),
            ("add_animation", "Add a new animation clip"),
            ("replace_animation", "Replace a shipped animation, same hash"),
            ("add_shader", "Add a compiled SM3 shader"),
            ("replace_shader", "Replace a shipped shader, same hash"),
            ("add_fx", "Add a particle effect"),
            ("replace_fx", "Replace a shipped fx, same hash"),
            ("replace_terrain_cell", "Replace a terrain cell, same hash"),
            ("add_stringdb_keys", "Add brand-new string-table keys"),
            ("replace_stringdb_text", "Rewrite strings by exact text match"),
            ("add_language", "Add a new selectable language (new base WAD)"),
        ],
    ),
    (
        "Script",
        &[
            ("patch_lua", "Append to a shipped script"),
            ("activate_layer", "Turn a hidden world-state layer on (permanent)"),
            ("add_script", "Mint a new Lua module (import-able)"),
            ("replace_lua", "Replace a shipped script's bytecode"),
            ("add_shop_item", "Add a purchasable shop item"),
        ],
    ),
    (
        "Code",
        &[
            ("native_hook", "An ASI plugin, or a symbol to detour"),
            ("place_file", "A companion file beside a plugin"),
            ("add_runtime_dll", "A runtime DLL in the game root, named after the Shipment"),
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
    // An empty subject (a blank "Add to this domain") keeps the stub's own placeholder — better a
    // named example than an empty field. Only a real routed subject overrides.
    if asset.is_empty() {
        return Some(c);
    }
    match &mut c {
        Contribution::AddOutfit { donor, .. } | Contribution::AddModel { donor, .. } => {
            *donor = Some(asset.to_string());
        }
        Contribution::ReplaceTexture { target, .. } => *target = asset.to_string(),
        // A layer routed from the World domain: `asset` is the layer NAME, so seed the field the kind
        // actually operates on (the stub's placeholder replaces/edits stay as authored defaults).
        Contribution::EditWorld { layer, .. } | Contribution::ActivateLayer { layer, .. } => {
            *layer = asset.to_string()
        }
        // A script routed from the Missions / Systems domain, or a table from anywhere: `asset` is the
        // script / table name, so it becomes the patch target. The sound kinds take no seed: the
        // Audio domain routes a script's name, which is not a bank.
        Contribution::PatchLua { target, .. } | Contribution::EditStringDb { target, .. } => {
            *target = asset.to_string()
        }
        _ => {}
    }
    Some(c)
}

/// The kinds a world-state LAYER can be routed into — the layer analogue of [`routes_for`]. A layer
/// is never the thing overwritten wholesale; it is moved in place (`edit_world`) or switched on
/// (`activate_layer`).
pub fn routes_for_layer() -> &'static [(&'static str, &'static str)] {
    &[
        ("activate_layer", "Activate this overlay"),
        ("edit_world", "Edit this layer's placements"),
    ]
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
            model: Some(PathBuf::from("src/model.glb")),
            donor: Some("pmc_hum_mattias".into()),
            textures: Textures::default(),
            retarget: None,
            single_group: false,
        },
        "add_model" => Contribution::AddModel {
            name,
            model: PathBuf::from("src/model.glb"),
            donor: Some("pmc_hum_mattias".into()),
            group: None,
            textures: Textures::default(),
            retarget: None,
            collision: mercs2_quartermaster::manifest::CollisionSource::default(),
        },
        "add_texture" => Contribution::AddTexture {
            name,
            image: PathBuf::from("src/texture.png"),
            normal_map: false,
        },
        // The category, the cues and the sessions are the author's: an empty category fails M0216,
        // an empty cue list M0215 and an empty `load_in` M0221 until they are chosen.
        "add_sound" => Contribution::AddSound {
            bank: name,
            category: String::new(),
            cues: Vec::new(),
            load_in: Vec::new(),
        },
        // The bank names a bank the game ships, so it starts empty (M0215) until it is entered.
        "replace_sound_bank" => Contribution::ReplaceSoundBank {
            bank: String::new(),
            language: None,
            category: String::new(),
            cues: Vec::new(),
        },
        "replace_sound_cue" => Contribution::ReplaceSoundCue {
            bank: String::new(),
            language: None,
            category: String::new(),
            cue: unset_cue(),
        },
        "edit_state_machine" => Contribution::EditStateMachine {
            target: "al_veh_boat_destroyer".into(),
            states: PathBuf::from("src/states.yaml"),
        },
        "edit_world" => Contribution::EditWorld {
            layer: "vz_state_pmccon004".into(),
            edits: PathBuf::from("src/world.yaml"),
        },
        "activate_layer" => Contribution::ActivateLayer {
            layer: "vz_state_pmccon004_destroyed".into(),
            replaces: vec!["vz_state_pmccon004_pristine".into()],
        },
        "edit_stringdb" => Contribution::EditStringDb {
            target: "english".into(),
            strings: PathBuf::from("src/strings.txt"),
        },
        "add_movie" => Contribution::AddMovie {
            name,
            movie: PathBuf::from("src/movie.gfx"),
        },
        "add_ui" => Contribution::AddUi {
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
            signature_guard: Default::default(),
        },
        "place_file" => Contribution::PlaceFile {
            file: PathBuf::from("src/plugin.ini"),
            dest: PlaceIn::Scripts,
        },
        // The placeholder is not `<shipment.name>.dll` (the stub does not know the name), so M0162
        // fires until the author points it at their DLL — the same "valid to serialize, loud to lint"
        // contract as every other stub.
        "add_runtime_dll" => Contribution::AddRuntimeDll {
            dll: PathBuf::from("src/runtime.dll"),
        },
        "raw" => Contribution::Raw {
            description: None,
            payload: PathBuf::from("src/payload.bin"),
            target_layer: Layer::Data,
            touches: Vec::new(),
        },
        "add_script" => Contribution::AddScript {
            name,
            source: PathBuf::from("src/module.lua"),
        },
        "replace_lua" => Contribution::ReplaceLua {
            target: "wifpmcinterior".into(),
            source: PathBuf::from("src/module.lua"),
        },
        "replace_phy2" => Contribution::ReplacePhy2 {
            target: "al_veh_boat_destroyer".into(),
            phy2: PathBuf::from("src/collision.phy2"),
        },
        "add_placement" => Contribution::AddPlacement {
            layer: "layers_static".into(),
            entity: PathBuf::from("src/placement.yaml"),
        },
        "add_layer" => Contribution::AddLayer {
            name,
            template: "layers_static".into(),
            entities: PathBuf::from("src/entities.yaml"),
        },
        "add_animation" => Contribution::AddAnimation {
            name,
            clip: PathBuf::from("src/clip.hkx"),
            trnm: PathBuf::from("src/clip.trnm"),
            events: None,
        },
        "replace_animation" => Contribution::ReplaceAnimation {
            target: "shipped_anim".into(),
            clip: PathBuf::from("src/clip.hkx"),
            trnm: PathBuf::from("src/clip.trnm"),
            events: None,
        },
        "add_shader" => Contribution::AddShader {
            name,
            blob: PathBuf::from("src/shader.bin"),
        },
        "replace_shader" => Contribution::ReplaceShader {
            target: "shipped_shader".into(),
            blob: PathBuf::from("src/shader.bin"),
        },
        "add_fx" => Contribution::AddFx {
            name,
            payload: PathBuf::from("src/effect.fxdict"),
        },
        "replace_fx" => Contribution::ReplaceFx {
            target: "shipped_fx".into(),
            payload: PathBuf::from("src/effect.fxdict"),
        },
        "replace_terrain_cell" => Contribution::ReplaceTerrainCell {
            target: "shipped_cell".into(),
            cell: PathBuf::from("src/terrain_cell.bin"),
        },
        "add_stringdb_keys" => Contribution::AddStringDbKeys {
            target: "english".into(),
            strings: PathBuf::from("src/new_keys.txt"),
        },
        "replace_stringdb_text" => Contribution::ReplaceStringDbText {
            target: "english".into(),
            pairs: PathBuf::from("src/text.pairs"),
        },
        "add_language" => Contribution::AddLanguage {
            name,
            display: "My Language".into(),
            strings: PathBuf::from("src/strings.txt"),
            base: None,
        },
        "add_shop_item" => Contribution::AddShopItem {
            id: format!("my_item_{n}"),
            name: "[my.item]".into(),
            description: String::new(),
            icon: "vehicles_tank_m1a2".into(),
            shops: vec![mercs2_quartermaster::manifest::ShopVendor::Pmc],
            catalog: mercs2_quartermaster::manifest::ShopCatalog::default(),
            item_type: Some(mercs2_quartermaster::manifest::ShopItemType::Heavy),
            cash_cost: 0,
            fuel_cost: 0,
            max_stock: 1,
            unlocked: false,
            behaviour: None,
            equipment_type: None,
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
                    // Light facts read straight from the source GLB, so the form can SHOW what the
                    // asset actually is (rig, materials, size) instead of blank fields, and pre-fill
                    // the retarget convention it detects. Cached per path, so it is read once.
                    let facts = model_source(c)
                        .map(|m| root.join(m))
                        .filter(|a| a.is_file())
                        .and_then(|a| p.model_facts_for(&a));
                    let mut edited = c.clone();
                    let commit = contribution_form(
                        ui,
                        &mut edited,
                        root,
                        &s.manifest.shipment.name,
                        names,
                        facts.as_ref(),
                    );
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
                    // "Show me what this does" — the exact YAML block this recipe writes under
                    // `contributions:`. Every Tier-1 recipe gets a visible way to see the format it
                    // lowers to, so the easy face is never opaque about the manifest it produces.
                    ui.add_space(6.0);
                    theme::advanced(ui, "what-this-writes", |ui| {
                        ui.label(theme::disp_text("writes this into manifest.yaml", 9.0, theme::FAINT));
                        ui.add_space(2.0);
                        let yaml = mercs2_quartermaster::contribution_yaml(c);
                        theme::code_block(ui, yaml.trim_end());
                    });
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

// ---- sound: add_sound / replace_sound_bank / replace_sound_cue --------------------------------

/// What the cue form says about its values. Every [`SoundCue`] field is required and none has a
/// default, so the values a new cue starts with ([`unset_cue`]) are placeholders to set.
const NEW_CUE_NOTE: &str = "every field is required and none has a default: a new cue starts with \
                            no name, no wave, 0 in every number and positional off \u{2014} set \
                            each one to the value this cue needs";

/// A cue with every field unset: no name and no wave, which the linter reports, 0 in every number
/// and positional off. [`NEW_CUE_NOTE`] tells the author these are theirs to set.
fn unset_cue() -> SoundCue {
    SoundCue {
        name: String::new(),
        wave: PathBuf::new(),
        group_gain_db: 0.0,
        cue_gain_db: 0.0,
        pitch_semitones: 0.0,
        positional: false,
        min_distance: 0.0,
        max_distance: 0.0,
        distance_exponent: 0.0,
        doppler_scale: 0.0,
        start_limit: 0,
        sound_id: 0,
        priority: 0.0,
        group_20: 0.0,
        cue_16: 0,
        clip_hash: 0,
    }
}

/// A whole number written in decimal, or in hex after `0x`, that fits in `0..=max`.
fn parse_whole<T: TryFrom<u64>>(text: &str, max: u64) -> Result<T, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("required".into());
    }
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => text.parse::<u64>(),
    };
    let v = parsed.map_err(|_| {
        format!("{text:?} is not a whole number: write it in decimal, or in hex after 0x")
    })?;
    if v > max {
        return Err(format!("{text} is out of range: this field holds 0 to {max}"));
    }
    T::try_from(v).map_err(|_| format!("{text} is out of range: this field holds 0 to {max}"))
}

/// A finite `f32`.
fn parse_f32(text: &str) -> Result<f32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("required".into());
    }
    let v: f32 = text.parse().map_err(|_| format!("{text:?} is not a number"))?;
    if !v.is_finite() {
        return Err(format!("{text} is not a finite 32-bit float: write a finite number"));
    }
    Ok(v)
}

/// A finite `f64`.
fn parse_f64(text: &str) -> Result<f64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("required".into());
    }
    let v: f64 = text.parse().map_err(|_| format!("{text:?} is not a number"))?;
    if !v.is_finite() {
        return Err(format!("{text} is not a finite number: write a finite number"));
    }
    Ok(v)
}

/// The text of a [`number_row`] as it is typed, and the value it was started from: a draft whose
/// value changed underneath it (another edit, another contribution) is dropped.
#[derive(Clone)]
struct HeldNumber<T: Clone> {
    origin: T,
    text: String,
}

/// A number row. The text is held in egui memory while it is typed and written when the field
/// loses focus and `parse` accepts it; text `parse` refuses stays in the field with the reason.
fn number_row<T: Copy + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    hint: &str,
    show: impl Fn(T) -> String,
    parse: impl Fn(&str) -> Result<T, String>,
) -> bool {
    let id = ui.make_persistent_id(("number_row", label));
    let mut held = ui
        .data_mut(|d| d.get_temp::<HeldNumber<T>>(id))
        .filter(|h| h.origin == *value)
        .unwrap_or_else(|| HeldNumber { origin: *value, text: show(*value) });
    let state = if parse(&held.text).is_ok() {
        theme::FieldState::Neutral
    } else {
        theme::FieldState::Bad
    };
    let r = theme::text_field(ui, label, &mut held.text, hint, state);
    let parsed = parse(&held.text);
    if let Err(why) = &parsed {
        theme::field_note(ui, theme::FieldState::Bad, why);
    }
    if r.lost_focus() {
        if let Ok(v) = parsed {
            ui.data_mut(|d| d.remove::<HeldNumber<T>>(id));
            if v != *value {
                *value = v;
                return true;
            }
            return false;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, held));
    false
}

/// The category combo over the named categories of the game's tree
/// (`mercs2_audio::encode::RETAIL_CATEGORY_NAMES`). A category that is not one of them shows as
/// unchosen, with M0216.
fn sound_category_row(ui: &mut egui::Ui, category: &mut String, what: &str) -> bool {
    use mercs2_audio::encode::RETAIL_CATEGORY_NAMES;
    let options: Vec<(&str, &str)> = RETAIL_CATEGORY_NAMES.iter().map(|n| (*n, *n)).collect();
    let mut pick: &str = RETAIL_CATEGORY_NAMES
        .iter()
        .copied()
        .find(|n| *n == category.as_str())
        .unwrap_or("");
    let state = if pick.is_empty() { theme::FieldState::Bad } else { theme::FieldState::Neutral };
    let changed = theme::combo_field(ui, "Category", &mut pick, &options, state);
    if changed {
        *category = pick.to_string();
    }
    if category.is_empty() {
        theme::field_note(ui, theme::FieldState::Bad, &format!("required \u{2014} {what}"));
    } else if pick.is_empty() {
        theme::field_note(
            ui,
            theme::FieldState::Bad,
            &format!("M0216 \u{2014} {category:?} is not a category of the game's tree; pick one"),
        );
    } else {
        theme::field_note(ui, theme::FieldState::Neutral, what);
    }
    changed
}

/// The preview's `Script` rows for a bank that loads in `sessions`: each session's loader and the
/// scripts it lives in, as `blast::loader_scripts` claims them.
fn sound_loader_rows(
    sessions: &std::collections::BTreeSet<mercs2_quartermaster::manifest::LoadSession>,
    what: &str,
) -> Vec<(String, String)> {
    use mercs2_quartermaster::manifest::LoadSession;
    if sessions.is_empty() {
        return vec![("Script".to_string(), format!("no session loads {what}"))];
    }
    sessions
        .iter()
        .map(|s| {
            let text = match s {
                LoadSession::Gameplay => format!(
                    "gameplay: qm_modloader loads {what}; trampolines in wifpmcinterior and mrxsoundbootstrap"
                ),
                LoadSession::FrontEnd => format!(
                    "front end: qm_shell_modloader loads {what}; a trampoline in shell.wad's mrxsound"
                ),
            };
            ("Script".to_string(), text)
        })
        .collect()
}

/// The preview's `Script` rows for an override of `bank`: each session's loader of its wavebank for a
/// bank retail Lua loads ([`sound_loader_rows`]); for a bank the engine loads, that the engine loads
/// it by its retail name, through no loader (`sound::loader_sessions`).
fn override_loader_rows(bank: &str, language: Option<mercs2_quartermaster::manifest::Language>) -> Vec<(String, String)> {
    use mercs2_quartermaster::sound::{bank_loader, loader_sessions, BankLoader, RETAIL_LUA_LOAD_SITES};
    match bank_loader(bank, &RETAIL_LUA_LOAD_SITES) {
        BankLoader::Lua => sound_loader_rows(&loader_sessions(bank, language), "its wavebank"),
        BankLoader::Engine => vec![(
            "Script".to_string(),
            format!("none: the engine loads {bank} by its retail name, the override waves appended to its wavebank"),
        )],
    }
}

/// The sessions an `add_sound` bank loads in: one pill per session, on or off, in manifest order
/// (`LoadSession::ALL`). None on is M0221.
fn sound_load_in_row(ui: &mut egui::Ui, load_in: &mut Vec<mercs2_quartermaster::manifest::LoadSession>) -> bool {
    use mercs2_quartermaster::manifest::LoadSession;
    let mut commit = false;
    ui.horizontal(|ui| {
        let lw = theme::field_label_w(ui.available_width());
        ui.add_sized([lw, 18.0], egui::Label::new("Load in"));
        for session in LoadSession::ALL {
            let on = load_in.contains(&session);
            if theme::pill(ui, session.token(), on).clicked() {
                if on {
                    load_in.retain(|s| *s != session);
                } else {
                    load_in.push(session);
                    load_in.sort();
                }
                commit = true;
            }
        }
    });
    if load_in.is_empty() {
        theme::field_note(
            ui,
            theme::FieldState::Bad,
            "M0221 \u{2014} pick where the bank loads: gameplay (the overlay) or the front end (the shell patch)",
        );
    } else {
        theme::field_note(
            ui,
            theme::FieldState::Neutral,
            "each session's mod loader loads the bank; its block ships to that session's WAD",
        );
    }
    commit
}

/// The bank a sound override targets, and the language of a `vo_*` bank's copy. The language row
/// is shown for a bank that starts with `vo_` (`sound::is_vo_bank`, the rule retail Lua localizes
/// by); committing a bank that does not clears the language.
fn sound_bank_row(
    ui: &mut egui::Ui,
    bank: &mut String,
    language: &mut Option<mercs2_quartermaster::manifest::Language>,
) -> bool {
    use mercs2_quartermaster::manifest::Language;
    use mercs2_quartermaster::sound::{entry_name, is_vo_bank};
    let mut commit = false;
    if text_row(ui, "Bank", bank, "ui_hud", true) {
        commit = true;
        if !is_vo_bank(bank) {
            *language = None;
        }
    }
    theme::field_note(ui, theme::FieldState::Neutral, "a bank the game ships, as its Lua loads it");
    if is_vo_bank(bank) {
        let options: Vec<(Option<Language>, &str)> =
            Language::ALL.iter().map(|l| (Some(*l), l.token())).collect();
        let state = if language.is_none() { theme::FieldState::Bad } else { theme::FieldState::Neutral };
        commit |= theme::combo_field(ui, "Language", language, &options, state);
        match language {
            None => theme::field_note(
                ui,
                theme::FieldState::Bad,
                "M0217 \u{2014} a `vo_` bank has one copy per language; pick the copy this replaces",
            ),
            Some(l) => theme::field_note(
                ui,
                theme::FieldState::Neutral,
                &format!("replaces the entry {}", entry_name(bank, Some(*l))),
            ),
        }
    } else if let Some(l) = *language {
        theme::field_note(
            ui,
            theme::FieldState::Bad,
            &format!(
                "M0217 \u{2014} {bank:?} is not a `vo_` bank, so it has one copy for every \
                 language; `language: {}` does not apply",
                l.token()
            ),
        );
        if ui.button("Clear language").clicked() {
            *language = None;
            commit = true;
        }
    }
    commit
}

/// Every field of one cue, one row each.
fn sound_cue_fields(ui: &mut egui::Ui, cue: &mut SoundCue, root: &Path) -> bool {
    let mut commit = false;
    theme::field_note(ui, theme::FieldState::Neutral, NEW_CUE_NOTE);
    commit |= text_row(ui, "Name", &mut cue.name, "ui_my_click", true);
    if !cue.name.is_empty() {
        theme::field_note(
            ui,
            theme::FieldState::Neutral,
            &format!(
                "0x{:08X} \u{2014} the guid Sound.CueSound looks up",
                mercs2_formats::hash::pandemic_hash_m2(&cue.name)
            ),
        );
    }
    commit |= source_row(ui, "Wave", &mut cue.wave, root, &["wav"]);
    theme::field_note(
        ui,
        theme::FieldState::Neutral,
        "uncompressed 16-bit PCM, mono or stereo",
    );
    let float = |v: f32| v.to_string();
    let double = |v: f64| v.to_string();
    let hex = |v: u32| format!("0x{v:08X}");
    commit |= number_row(ui, "Group gain dB", &mut cue.group_gain_db, "dB", double, parse_f64);
    theme::field_note(ui, theme::FieldState::Neutral, "the sound instance's base volume, in dB");
    commit |= number_row(ui, "Cue gain dB", &mut cue.cue_gain_db, "dB", double, parse_f64);
    theme::field_note(ui, theme::FieldState::Neutral, "the cue's gain, in dB");
    commit |= number_row(ui, "Pitch", &mut cue.pitch_semitones, "semitones", float, parse_f32);
    theme::field_note(ui, theme::FieldState::Neutral, "the base pitch, in semitones");
    ui.horizontal(|ui| {
        theme::field_label(ui, "Positional");
        commit |= ui.checkbox(&mut cue.positional, "").changed();
    });
    theme::field_note(
        ui,
        theme::FieldState::Neutral,
        if cue.positional {
            "plays from its emitter's own source, positioned, when it has one"
        } else {
            "plays from the shared 2D source"
        },
    );
    commit |= number_row(ui, "Min distance", &mut cue.min_distance, "full volume up to", float, parse_f32);
    commit |= number_row(ui, "Max distance", &mut cue.max_distance, "silent from", float, parse_f32);
    commit |= number_row(
        ui,
        "Distance exp.",
        &mut cue.distance_exponent,
        "fall-off exponent",
        float,
        parse_f32,
    );
    commit |= number_row(ui, "Doppler scale", &mut cue.doppler_scale, "0..", float, parse_f32);
    commit |= number_row(
        ui,
        "Start limit",
        &mut cue.start_limit,
        "0 starts every time",
        |v: u8| v.to_string(),
        |t| parse_whole(t, u64::from(u8::MAX)),
    );
    theme::field_note(
        ui,
        theme::FieldState::Neutral,
        "starts only while fewer than this many instances play; 0 starts it every time",
    );
    commit |= number_row(
        ui,
        "Sound id",
        &mut cue.sound_id,
        "0x… or decimal",
        hex,
        |t| parse_whole(t, u64::from(u32::MAX)),
    );
    commit |= number_row(ui, "Priority", &mut cue.priority, "voice priority", float, parse_f32);
    commit |= number_row(ui, "Group +0x20", &mut cue.group_20, "carried as written", float, parse_f32);
    commit |= number_row(
        ui,
        "Cue +0x16",
        &mut cue.cue_16,
        "carried as written",
        |v: u16| v.to_string(),
        |t| parse_whole(t, u64::from(u16::MAX)),
    );
    commit |= number_row(
        ui,
        "Clip hash",
        &mut cue.clip_hash,
        "0x… or decimal",
        hex,
        |t| parse_whole(t, u64::from(u32::MAX)),
    );
    commit
}

/// The cues of a bank, each with its fields and a remove button, and a button that appends an
/// [`unset_cue`].
fn sound_cue_list(ui: &mut egui::Ui, cues: &mut Vec<SoundCue>, root: &Path) -> bool {
    let mut commit = false;
    let mut remove: Option<usize> = None;
    if cues.is_empty() {
        theme::field_note(
            ui,
            theme::FieldState::Bad,
            "M0215 \u{2014} a bank declares at least one cue",
        );
    }
    for (i, cue) in cues.iter_mut().enumerate() {
        ui.push_id(("sound_cue", i), |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                theme::eyebrow(ui, &format!("Cue {i}"));
                if ui.small_button("\u{2715}").on_hover_text("Remove this cue").clicked() {
                    remove = Some(i);
                }
            });
            commit |= sound_cue_fields(ui, cue, root);
        });
    }
    if let Some(i) = remove {
        cues.remove(i);
        commit = true;
    }
    if ui.button("+ cue").clicked() {
        cues.push(unset_cue());
        commit = true;
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
    shipment_name: &str,
    names: Option<&NameTable>,
    facts: Option<&GlbFacts>,
) -> bool {
    use mercs2_quartermaster::manifest::{Layer, PlaceIn, Target};
    let mut commit = false;

    match c {
        Contribution::AddOutfit {
            name, slug, display, wearer, model, donor, textures, retarget, single_group,
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
            // Wearer is chosen once, in the WARDROBE section above — it owns the (wearer, slug) merge
            // key and the single `_tOutfits` writer. Duplicating the pills here made two writers of the
            // same field, the exact split that section's comment exists to prevent. `wearer` is still
            // read below for the donor auto-pick note.
            // The model FILE is OPTIONAL: pick one to INJECT a new mesh, or leave it empty to wear a
            // model the game already ships (named by "Asset name" above). Edited through a temp since
            // the field is `Option<PathBuf>`; an empty path maps back to `None`.
            let mut model_path = model.clone().unwrap_or_default();
            if source_row(ui, "Model (optional)", &mut model_path, root, &["glb", "gltf", "obj"]) {
                commit = true;
                // Name the asset after the model file, while the fields still hold their `stub()`
                // placeholders — an author who picks RuMerc1.glb means the outfit is RuMerc1, not
                // my_asset_2. Only the mint-pattern defaults are replaced; a real value is left alone.
                if let Some(stem) = model_path.file_stem().map(|s| s.to_string_lossy().to_string()) {
                    if name.is_empty() || name.starts_with("my_asset") {
                        *name = stem.clone();
                    }
                    if slug.is_empty() || slug.starts_with("MyOutfit") {
                        *slug = stem.clone();
                    }
                    if display.is_empty() || display == "My outfit" {
                        *display = stem;
                    }
                }
                // Also read what the GLB declares: a FOREIGN rig with no `retarget:` would be lowered
                // rigidly (won't animate), so fill in the detected convention. Detection reads the
                // joint names in the file; it is not a guess.
                if let Some(f) = probe_glb(&root.join(&model_path)) {
                    if f.foreign() && retarget.is_none() {
                        *retarget = Some(mercs2_quartermaster::manifest::Retarget {
                            from: f.rig.slug().to_string(),
                            bones: None,
                        });
                    }
                }
            }
            // Empty path -> wear an existing model (no injection); a file -> inject it.
            *model = (!model_path.as_os_str().is_empty()).then_some(model_path);
            if model.is_none() {
                theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    "empty — wear the existing in-game model named above (nothing is injected)",
                );
            }
            glb_facts_note(ui, facts, GlbAdvice::Outfit { single_group: *single_group });
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
            ui.add_space(6.0);
            // Single-group: the placement-stable path for a DENSE foreign rig (>~48 bones), which
            // otherwise takes the multi-group split and can cull/teleport. It bakes the GLB's own
            // per-material maps into one atlas — so the manual Skin slots are replaced, not ignored.
            ui.horizontal(|ui| {
                theme::field_label(ui, "Single group");
                if theme::pill(ui, if *single_group { "on" } else { "off" }, *single_group).clicked() {
                    *single_group = !*single_group;
                    commit = true;
                }
            });
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                if *single_group {
                    "one draw group on the source's own weights; wears an atlas baked from the GLB's \
                     own materials. For a dense foreign rig that would otherwise cull or teleport."
                } else {
                    "faithful multi-group split (per-material skins below). Turn on for a dense \
                     foreign rig that culls or teleports as the camera moves."
                },
            );
            ui.add_space(6.0);
            // With single_group the atlas is baked from the GLB's own materials, so the manual slots
            // do nothing — hide them rather than offer dead controls.
            if !*single_group {
                theme::eyebrow(ui, "Skin — omit a slot to keep the donor's");
                for (lbl, slot) in [
                    ("Diffuse", &mut textures.diffuse),
                    ("Normal", &mut textures.normal),
                    ("Specular", &mut textures.specular),
                ] {
                    commit |= optional_source_row(ui, lbl, slot, root, &["png"]);
                }
            }
            commit |= retarget_summary(ui, retarget);
        }
        Contribution::AddModel { name, model, donor, group, textures, retarget, collision: _ } => {
            commit |= text_row(ui, "Asset name", name, "my_custom_helipad", true);
            if source_row(ui, "Model", model, root, &["glb", "gltf", "obj"]) {
                commit = true;
                // Name the asset after the model file while the name is still the stub placeholder.
                if let Some(stem) = model.file_stem().map(|s| s.to_string_lossy().to_string()) {
                    if name.is_empty() || name.starts_with("my_asset") {
                        *name = stem;
                    }
                }
            }
            glb_facts_note(ui, facts, GlbAdvice::Model);
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
        Contribution::AddShopItem { .. } => {
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "shop items are edited in the manifest, not the workshop bench",
            );
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
        Contribution::AddSound { bank, category, cues, load_in } => {
            commit |= text_row(ui, "Bank", bank, "my_sounds", true);
            if mercs2_quartermaster::sound::is_vo_bank(bank) {
                theme::field_note(
                    ui,
                    theme::FieldState::Bad,
                    "M0215 — retail Lua appends the language to a `vo_` bank's name before loading \
                     it; name the bank without `vo_`",
                );
            } else {
                theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    &format!(
                        "0x{:08X} — the entry of its soundbank, sounddb and wavebank, and the name \
                         the mod loader loads",
                        mercs2_quartermaster::manifest::asset_hash(bank)
                    ),
                );
            }
            commit |= sound_category_row(ui, category, "the category every cue's group is in");
            commit |= sound_load_in_row(ui, load_in);
            commit |= sound_cue_list(ui, cues, root);
        }
        Contribution::ReplaceSoundBank { bank, language, category, cues } => {
            commit |= sound_bank_row(ui, bank, language);
            commit |= sound_category_row(ui, category, "the category every cue's group is in");
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "the bank's soundbank and sounddb are encoded from these cues; a cue of the game's \
                 bank not declared here is gone",
            );
            commit |= sound_cue_list(ui, cues, root);
        }
        Contribution::ReplaceSoundCue { bank, language, category, cue } => {
            commit |= sound_bank_row(ui, bank, language);
            commit |= sound_category_row(ui, category, "the category of the cue's new group");
            theme::eyebrow(ui, "Cue — its name is the game's cue it replaces");
            commit |= sound_cue_fields(ui, cue, root);
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
        Contribution::AddUi { name, movie } => {
            commit |= text_row(ui, "Asset name", name, "my_menu", true);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a NAME, not a filename — retail's are bare (`topbar`, `pause_menu`)",
            );
            commit |= source_row(ui, "Movie", movie, root, &["gfx", "cfx", "swf"]);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "add_movie + a FlashWidget that plays it, baked into the mod loader — the movie \
                 appears; its rect and hide trigger are yours to add in Lua",
            );
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
        Contribution::EditWorld { layer, edits } => {
            commit |= text_row(ui, "Layer", layer, "vz_state_pmccon004", true);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a PTHS-path needle: a vz_state overlay, or `layers_static`",
            );
            commit |= source_row(ui, "Edits", edits, root, &["yaml", "yml"]);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "per-entity pos / quat / model. Extract a baseline: `qm extract-world <layer>`",
            );
        }
        Contribution::ActivateLayer { layer, replaces } => {
            commit |= text_row(ui, "Layer", layer, "vz_state_pmccon004_destroyed", true);
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "MarkForAddition at runtime — a CASE-SENSITIVE layer name, not a path. Baked into \
                 the mod loader; the world never grows a per-mod script.",
            );
            // `replaces` is a list of layer NAMES (MarkForRemoval, in order). Names carry no commas,
            // so a comma-separated row is a faithful, reversible edit.
            let mut joined = replaces.join(", ");
            if text_row(ui, "Replaces", &mut joined, "vz_state_pmccon004_pristine", false) {
                *replaces = joined
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .collect();
                commit = true;
            }
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "the pristine / prior overlay this one supersedes (MarkForRemoval) — comma-separated, \
                 optional",
            );
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
        Contribution::AddLanguage { name, display, strings, base } => {
            commit |= theme::text_field(ui, "Language", name, "polski", theme::FieldState::Neutral)
                .lost_focus();
            commit |=
                theme::text_field(ui, "Display", display, "Polski", theme::FieldState::Neutral)
                    .lost_focus();
            commit |= source_row(ui, "Strings", strings, root, &["strings", "txt"]);
            let mut b = base.clone().unwrap_or_default();
            if theme::text_field(ui, "Base", &mut b, "english (default)", theme::FieldState::Neutral)
                .lost_focus()
            {
                *base = (!b.is_empty()).then_some(b);
                commit = true;
            }
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "forks `base` (default english) into a new `.\\Data\\<name>.wad` + overlay stringdb.",
            );
        }
        Contribution::NativeHook { target, plugin, symbol, touches, signature_guard: _ } => {
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
        Contribution::AddRuntimeDll { dll } => {
            commit |= source_row(ui, "DLL", dll, root, &["dll"]);
            // Live, against the builder's OWN refusal rather than a copy of it.
            if let Some(n) = dll.file_name().map(|s| s.to_string_lossy().to_string()) {
                match mercs2_quartermaster::build::runtime_dll_name_refusal(&n, shipment_name) {
                    Some(why) => {
                        theme::field_note(ui, theme::FieldState::Bad, &format!("M0162 — {why}"))
                    }
                    None => theme::field_note(
                        ui,
                        theme::FieldState::Good,
                        "placed in the game root under this name",
                    ),
                }
            }
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                "a runtime DLL is named after its Shipment, so a Shipment ships at most one",
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
        // Newer Contribution kinds without bespoke workshop editors yet. Manifest is still fully
        // editable as YAML; this panel just doesn't offer field-level UI for them today.
        other => {
            theme::field_note(
                ui,
                theme::FieldState::Neutral,
                &format!(
                    "No field editor yet for `{}` — edit the manifest.yaml directly.",
                    other.kind()
                ),
            );
        }
    }
    commit
}

/// The Shipment's OWN identity — what it is called, who wrote it, and how it orders against others.
///
/// Shown where "pick a contribution" used to be. That space was doing nothing, and these fields had
/// no editor at all: `shipment.name` decides the output filename (`_build/<name>.wad`) and every
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
                &format!("builds to _build/{}.wad", sh.name),
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

    let own = sh.name.clone();
    theme::section(ui, "Dependencies", None, false, |ui| {
        ui.label(
            egui::RichText::new(
                "`requires` names what must be installed for this Shipment to work — a Shipment \
                 (optionally within a version range) or a capability some Shipment provides. A \
                 Shipment loads after the Shipments it requires. `conflicts` names a Shipment \
                 (optionally within a range) that cannot be installed alongside this.",
            )
            .size(11.0)
            .color(theme::FAINT),
        );
        ui.add_space(6.0);
        theme::eyebrow(ui, "Requires");
        commit |= requires_editor(ui, &mut load.requires, &own);
        ui.add_space(6.0);
        theme::eyebrow(ui, "Conflicts");
        commit |= conflicts_editor(ui, &mut load.conflicts, &own);
    });

    commit
}

// ---- dependencies: `load.requires` / `load.conflicts` ------------------------------------

/// The forms a `load.requires` entry is edited as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequireForm {
    /// A Shipment, any version.
    Shipment,
    /// A Shipment within a semver range.
    ShipmentRange,
    /// Any Shipment that provides a capability token.
    Capability,
}

/// One requirement or conflict row as the author is typing it. Kept apart from the manifest until
/// it is valid, so a half-typed range is never written — a manifest that fails validation could not
/// be reopened — and is never thrown away either.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DepDraft {
    pub form: RequireForm,
    /// A Shipment name, or a capability token.
    pub target: String,
    /// The range; used only by [`RequireForm::ShipmentRange`].
    pub range: String,
}

impl DepDraft {
    /// The draft of an existing requirement. The `{ name, version }` form is not a requirement form
    /// (validation refuses it, so an opened manifest never holds one); it is shown as the ranged form
    /// it has to become.
    pub(crate) fn of_requirement(r: &Requirement) -> DepDraft {
        let (form, target, range) = match r {
            Requirement::Shipment(n) => (RequireForm::Shipment, n.clone(), String::new()),
            Requirement::ShipmentRange(r) => {
                (RequireForm::ShipmentRange, r.shipment.clone(), r.version.clone())
            }
            Requirement::Capability(c) => (RequireForm::Capability, c.capability.clone(), String::new()),
            Requirement::Compatible(c) => (RequireForm::ShipmentRange, c.name.clone(), c.version.clone()),
        };
        DepDraft { form, target, range }
    }

    /// The draft of an existing conflict. A conflict has no capability form.
    pub(crate) fn of_conflict(c: &ConflictDecl) -> DepDraft {
        match c {
            ConflictDecl::Name(n) => DepDraft {
                form: RequireForm::Shipment,
                target: n.clone(),
                range: String::new(),
            },
            ConflictDecl::Range(r) => DepDraft {
                form: RequireForm::ShipmentRange,
                target: r.shipment.clone(),
                range: r.version.clone(),
            },
        }
    }
}

/// Why `name` cannot be referred to from this Shipment's `requires` / `conflicts`, with the code
/// validation reports it under — the same two rules `Manifest::validate` applies.
fn reference_problem(name: &str, own: &str) -> Option<String> {
    if !mercs2_quartermaster::manifest::is_slug(name) {
        return Some(format!(
            "M0100 — {name:?} is not a Shipment name: names are slugs, lowercase letters, digits and \
             single hyphens"
        ));
    }
    if name == own {
        return Some("M0173 — a Shipment cannot require or conflict with itself".into());
    }
    None
}

/// The range parsed the one way qm parses every range (M0172).
fn range_problem(range: &str) -> Option<String> {
    mercs2_quartermaster::manifest::parse_range(range)
        .err()
        .map(|e| format!("M0172 — {range:?} is not a valid semver range: {e}"))
}

/// The `load.requires` entry a draft stands for, or why it cannot be written yet.
pub(crate) fn requirement_from_draft(d: &DepDraft, own: &str) -> Result<Requirement, String> {
    let target = d.target.trim();
    match d.form {
        RequireForm::Capability => {
            if target.is_empty() {
                return Err("name the capability token".into());
            }
            Ok(Requirement::Capability(CapabilityReq {
                capability: target.to_string(),
            }))
        }
        RequireForm::Shipment => match reference_problem(target, own) {
            Some(why) => Err(why),
            None => Ok(Requirement::Shipment(target.to_string())),
        },
        RequireForm::ShipmentRange => {
            if let Some(why) = reference_problem(target, own).or_else(|| range_problem(d.range.trim())) {
                return Err(why);
            }
            Ok(Requirement::ShipmentRange(ShipmentReq {
                shipment: target.to_string(),
                version: d.range.trim().to_string(),
            }))
        }
    }
}

/// The `load.conflicts` entry a draft stands for, or why it cannot be written yet.
pub(crate) fn conflict_from_draft(d: &DepDraft, own: &str) -> Result<ConflictDecl, String> {
    let target = d.target.trim();
    match d.form {
        RequireForm::Capability => Err("a conflict names a Shipment, not a capability".into()),
        RequireForm::Shipment => match reference_problem(target, own) {
            Some(why) => Err(why),
            None => Ok(ConflictDecl::Name(target.to_string())),
        },
        RequireForm::ShipmentRange => {
            if let Some(why) = reference_problem(target, own).or_else(|| range_problem(d.range.trim())) {
                return Err(why);
            }
            Ok(ConflictDecl::Range(ShipmentReq {
                shipment: target.to_string(),
                version: d.range.trim().to_string(),
            }))
        }
    }
}

/// A draft kept in egui's temp memory, remembering the entry it was started from so a draft left
/// over from another Shipment (or from before a list changed) is dropped rather than shown.
#[derive(Clone)]
struct HeldDraft<T: Clone> {
    origin: Option<T>,
    draft: DepDraft,
}

/// Edit one row. `origin` is the committed entry (`None` for the "add" row). Returns the entry to
/// write when the draft is valid, changed, and the author has finished with it (a field lost focus,
/// the form changed, or `Add` was pressed).
fn dep_row<T: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    id: egui::Id,
    origin: Option<&T>,
    to_draft: impl Fn(&T) -> DepDraft,
    from_draft: impl Fn(&DepDraft) -> Result<T, String>,
    forms: &[(RequireForm, &str)],
    empty: DepDraft,
) -> Option<T> {
    let fresh = HeldDraft {
        origin: origin.cloned(),
        draft: origin.map(&to_draft).unwrap_or(empty),
    };
    let mut held: HeldDraft<T> = ui
        .data_mut(|d| d.get_temp::<HeldDraft<T>>(id))
        .filter(|h| h.origin.as_ref() == origin)
        .unwrap_or(fresh);
    let mut finished = false;
    ui.push_id(id, |ui| {
        finished |= theme::combo_field(ui, "form", &mut held.draft.form, forms, theme::FieldState::Neutral);
        let result = from_draft(&held.draft);
        let state = if result.is_ok() { theme::FieldState::Neutral } else { theme::FieldState::Bad };
        let hint = match held.draft.form {
            RequireForm::Capability => "capability-token",
            _ => "other-shipment",
        };
        finished |= theme::text_field(ui, "name", &mut held.draft.target, hint, state).lost_focus();
        if held.draft.form == RequireForm::ShipmentRange {
            finished |= theme::text_field(ui, "range", &mut held.draft.range, "^1.0.0", state).lost_focus();
        }
        if let Err(why) = &result {
            theme::field_note(ui, theme::FieldState::Bad, why);
        }
        if origin.is_none() {
            finished = result.is_ok() && ui.button("Add").clicked();
        }
    });
    let wanted = from_draft(&held.draft).ok();
    let changed = wanted.as_ref() != origin;
    if finished && changed {
        if let Some(entry) = wanted {
            ui.data_mut(|d| d.remove::<HeldDraft<T>>(id));
            return Some(entry);
        }
    }
    ui.data_mut(|d| d.insert_temp(id, held));
    None
}

/// Rows plus an add row for any `load.requires` / `load.conflicts`-shaped list. Returns true when
/// the list changed and should be written.
fn dep_list<T: Clone + PartialEq + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    salt: &str,
    list: &mut Vec<T>,
    to_draft: impl Fn(&T) -> DepDraft + Copy,
    from_draft: impl Fn(&DepDraft) -> Result<T, String> + Copy,
    forms: &[(RequireForm, &str)],
) -> bool {
    let mut commit = false;
    let mut remove = None;
    for i in 0..list.len() {
        let id = ui.make_persistent_id((salt, i));
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if let Some(entry) =
                    dep_row(ui, id, Some(&list[i]), to_draft, from_draft, forms, to_draft(&list[i]))
                {
                    list[i] = entry;
                    commit = true;
                }
            });
            if ui.small_button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        list.remove(i);
        commit = true;
    }
    let add_id = ui.make_persistent_id((salt, "add"));
    let empty = DepDraft {
        form: RequireForm::Shipment,
        target: String::new(),
        range: String::new(),
    };
    if let Some(entry) = dep_row(ui, add_id, None, to_draft, from_draft, forms, empty) {
        list.push(entry);
        commit = true;
    }
    commit
}

/// `load.requires`: a Shipment, a Shipment within a range, or a capability.
fn requires_editor(ui: &mut egui::Ui, requires: &mut Vec<Requirement>, own: &str) -> bool {
    dep_list(
        ui,
        "qm-requires",
        requires,
        DepDraft::of_requirement,
        |d| requirement_from_draft(d, own),
        &[
            (RequireForm::Shipment, "Shipment, any version"),
            (RequireForm::ShipmentRange, "Shipment within a range"),
            (RequireForm::Capability, "capability"),
        ],
    )
}

/// `load.conflicts`: a Shipment, or a Shipment within a range.
fn conflicts_editor(ui: &mut egui::Ui, conflicts: &mut Vec<ConflictDecl>, own: &str) -> bool {
    dep_list(
        ui,
        "qm-conflicts",
        conflicts,
        DepDraft::of_conflict,
        |d| conflict_from_draft(d, own),
        &[
            (RequireForm::Shipment, "Shipment, any version"),
            (RequireForm::ShipmentRange, "Shipment within a range"),
        ],
    )
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
            theme::kv(ui, "Retarget", egui::RichText::new(format!("{} rig", r.from)));
            if n == 0 {
                // A convention (valve/mixamo/…) has a deterministic table, so from-only is complete
                // and reproducible — informational, not a warning. Only hand-adjusted bones need to
                // be recorded, and Edit rig is where you do (and save) that.
                theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    "the build derives the bone map from this convention's table. Use Edit rig to \
                     hand-adjust and record specific bones.",
                );
            } else {
                theme::field_note(
                    ui,
                    theme::FieldState::Neutral,
                    &format!("{n} hand-recorded bone rows — the build uses these exactly"),
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
        Contribution::AddShopItem { id, shops, .. } => vec![(
            "Script".to_string(),
            format!("shop item {id}  (catalog + {} reward row(s))", shops.len()),
        )],
        Contribution::AddMovie { name, .. } => {
            vec![("Writes".to_string(), format!("cfx_pack {name}  (new hash)"))]
        }
        Contribution::AddUi { name, .. } => vec![
            ("Writes".to_string(), format!("cfx_pack {name}  (new hash)")),
            (
                "Script".to_string(),
                "qm_modloader plays it; one trampoline in wifpmcinterior".to_string(),
            ),
        ],
        Contribution::AddTexture { name, .. } => {
            vec![("Writes".to_string(), format!("texture {name}  (new hash)"))]
        }
        // The same claims `blast::claims` makes: the bank's entry, each cue's guid, and the loader
        // scripts of each session it loads in.
        Contribution::AddSound { bank, cues, load_in, .. } => {
            let mut rows = vec![("Writes".to_string(), format!("sound bank {bank}  (new hash)"))];
            rows.extend(sound_loader_rows(&load_in.iter().copied().collect(), "the bank"));
            rows.extend(cues.iter().map(|c| {
                ("Writes".to_string(), format!("sound cue {}  (new guid)", c.name))
            }));
            rows.push((
                "Merge".to_string(),
                "keyed on cue name \u{2014} FindCue answers with the first loaded table".to_string(),
            ));
            rows
        }
        Contribution::ReplaceSoundBank { bank, language, cues, .. } => {
            let entry = mercs2_quartermaster::sound::entry_name(bank, *language);
            let mut rows = vec![("Writes".to_string(), format!("sound bank {entry}  \u{2014} EXCLUSIVE"))];
            rows.extend(override_loader_rows(bank, *language));
            rows.extend(cues.iter().map(|c| {
                ("Writes".to_string(), format!("sound cue {}  \u{2014} EXCLUSIVE", c.name))
            }));
            rows
        }
        Contribution::ReplaceSoundCue { bank, language, cue, .. } => {
            let mut rows = vec![(
                "Writes".to_string(),
                format!(
                    "sound cue {} in {}  \u{2014} EXCLUSIVE",
                    cue.name,
                    mercs2_quartermaster::sound::entry_name(bank, *language)
                ),
            )];
            rows.extend(override_loader_rows(bank, *language));
            rows
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
        Contribution::EditWorld { layer, .. } => vec![
            ("Writes".to_string(), format!("placement layer {layer}")),
            ("Merge".to_string(), "last-wins \u{2014} the later overlay's edits win".to_string()),
        ],
        Contribution::ActivateLayer { layer, .. } => vec![
            ("Activates".to_string(), format!("world layer {layer}")),
            (
                "Merge".to_string(),
                "baked into the mod loader \u{2014} N activations share one trampoline".to_string(),
            ),
        ],
        Contribution::EditStringDb { target, .. } => vec![
            ("Writes".to_string(), format!("stringdb {target}")),
            ("Merge".to_string(), "last-wins \u{2014} load order is the answer".to_string()),
        ],
        Contribution::AddLanguage { name, .. } => vec![
            ("Adds".to_string(), format!("language {name}")),
            (
                "Merge".to_string(),
                "new base .\\Data\\<name>.wad + overlay stringdb \u{2014} additive".to_string(),
            ),
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
        Contribution::AddRuntimeDll { dll } => vec![(
            "Writes".to_string(),
            format!("file {} in the game root  \u{2014} EXCLUSIVE", leaf(dll)),
        )],
        Contribution::Raw { touches, .. } => touches
            .iter()
            .map(|t| ("Declares".to_string(), t.0.clone()))
            .collect(),
        // Fallback for kinds without a bespoke blast row yet; show the kind so the panel is
        // non-empty rather than hiding the contribution.
        other => vec![("Kind".to_string(), other.kind().to_string())],
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
        // The language WADs the open Shipment reads: the build opens them after the rows above.
        if let (Some(s), false) = (&p.shipment, wad_stack.is_empty()) {
            match build_stack_paths(wad_stack, &s.manifest) {
                Ok(paths) => {
                    for w in &paths[wad_stack.len()..] {
                        row(ui, "language", &leaf(w), theme::DIM);
                    }
                }
                Err(e) => {
                    ui.label(egui::RichText::new(e).size(11.0).color(theme::BAD));
                }
            }
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
        if p.building {
            // Build is off on a worker thread. Show a live spinner + label in place of the button,
            // and keep egui repainting so the spinner animates and the app polls for completion.
            ui.add(egui::Spinner::new());
            ui.label(egui::RichText::new("Building…").color(theme::BRASS));
            ui.ctx().request_repaint();
        } else if p.report.is_some() {
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

/// The decompiled Lua corpus, needed by any script-touching contribution. It rides in the reference
/// bundle as `workshop_data/lua` — the same `vz/resident/shell` tree
/// [`mercs2_quartermaster::link::base_source_path`] walks, copied verbatim by `--pack-data` — and is
/// resolved through [`crate::index::data_home`], the one place every other bundled input already
/// comes from (names, ECS, Ess).
///
/// No source-tree fallback: a Workshop that cannot find its bundle should fail loudly (the linker's
/// own "needs the corpus" error), not quietly link against a checkout path a released build could
/// never reach. That guess is exactly what let the released `qm`/Workshop ship unable to link while
/// "working" in dev.
pub fn corpus_root() -> Option<PathBuf> {
    let lua = crate::index::data_home()?.join("lua");
    lua.is_dir().then_some(lua)
}

/// The WADs a build of `manifest` opens, in open order: the configured stack (`vz.wad`, then
/// `vz-patch.wad` and each `--overlay`), then the WAD of each language the Shipment reads — the
/// entries [`mercs2_quartermaster::compat::game_stack_paths`] lists after `vz.wad`. `GameStack`
/// resolves the last-opened WAD first, so a language's copy of an entry is read over the base's.
pub fn build_stack_paths(
    wad_stack: &[String],
    manifest: &mercs2_quartermaster::manifest::Manifest,
) -> Result<Vec<PathBuf>, String> {
    let Some(vz) = wad_stack.first() else {
        return Err(
            "build needs the game's vz.wad: none is configured \u{2014} set it in Settings".into(),
        );
    };
    let game = mercs2_quartermaster::compat::game_stack_paths(Path::new(vz), [manifest])
        .map_err(|e| format!("build needs the game stack: {e}"))?;
    let mut paths: Vec<PathBuf> = wad_stack.iter().map(PathBuf::from).collect();
    paths.extend(game.into_iter().skip(1));
    Ok(paths)
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
            // Ignore a re-click while one is already running rather than spawn a second.
            if p.building {
                return;
            }
            let Some(s) = p.shipment.clone() else {
                *status = "no Shipment open to build".into();
                return;
            };
            // Everything the build reads is cloned so it can move to a worker thread. Opening the
            // WADs, linting and linking Lua takes seconds; doing it inline froze the frame (and any
            // "Building…" spinner with it), which is the whole bug. The game stack is opened ON the
            // worker for the same reason — `GameStack::open` decompresses index tables.
            // The stack is [`build_stack_paths`]: the one the inspector lists.
            let paths = build_stack_paths(wad_stack, &s.manifest);
            let names = names.cloned();
            let corpus = corpus.map(Path::to_path_buf);
            let (tx, rx) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("qm-build".into())
                .spawn(move || {
                    let outcome = match paths.map(|p| mercs2_quartermaster::game::GameStack::open(&p)) {
                        Ok(Ok(mut g)) => {
                            run_build_outcome(&s, Some(&mut g), names.as_ref(), corpus.as_deref())
                        }
                        Ok(Err(e)) => {
                            BuildOutcome::Failed(format!("build needs a readable game stack: {e:?}"))
                        }
                        Err(e) => BuildOutcome::Failed(e),
                    };
                    // A send error just means the panel was dropped (app closing) — nothing to do.
                    let _ = tx.send(outcome);
                });
            match spawned {
                Ok(_) => {
                    p.build_rx = Some(rx);
                    p.building = true;
                    p.status = "Building…".into();
                }
                Err(e) => {
                    p.error = Some(format!("could not start the build thread: {e}"));
                    p.status = "build failed".into();
                }
            }
            *status = p.status.clone();
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

/// The classified result of one build, sent from the worker thread back to the UI.
///
/// Distinct from `Result<BuildReport, BuildError>` on purpose: `Blocked` is the linter doing its job
/// (findings replace the current set, the gate turns red — not a failure), and this type only holds
/// thread-sendable data, so a `BuildError` is rendered to a `String` on the worker rather than
/// carried across the channel.
enum BuildOutcome {
    Report(BuildReport),
    Blocked(Vec<Diagnostic>),
    Failed(String),
}

/// Run a build and classify its result. Runs on the worker thread for [`Act::Build`]; also the body
/// of the synchronous [`run_build`].
fn run_build_outcome(
    s: &LoadedShipment,
    game: Option<&mut mercs2_quartermaster::game::GameStack>,
    names: Option<&NameTable>,
    corpus: Option<&Path>,
) -> BuildOutcome {
    match build::build(s, game, names, None, corpus) {
        Ok(r) => BuildOutcome::Report(r),
        Err(BuildError::Blocked(ds)) => BuildOutcome::Blocked(ds),
        Err(e) => BuildOutcome::Failed(format!("{e:?}")),
    }
}

/// Run a build synchronously and fold the outcome back in. Retained for callers that already hold an
/// open `GameStack`; the interactive verb-bar path builds on a worker thread instead (see
/// [`Act::Build`]) so the UI keeps painting.
pub fn run_build(
    p: &mut Panel,
    game: Option<&mut mercs2_quartermaster::game::GameStack>,
    names: Option<&NameTable>,
    corpus: Option<&Path>,
) {
    let Some(s) = p.shipment.clone() else { return };
    let outcome = run_build_outcome(&s, game, names, corpus);
    p.apply_build_outcome(outcome);
}

/// Where the Workshop drops a Shipment for Modkit to find: **Modkit's own data root**.
///
/// This used to be `%LOCALAPPDATA%/mercs2/shipments` — a location invented here that Modkit never
/// reads, so "Send to Modkit" wrote into the void. Modkit keeps its state under
/// `%APPDATA%/mercs2-modkit/` (`staging`, `deployed`, `bin`, …), so `shipments/` belongs beside
/// those, in the layout Modkit already owns.
///
/// A folder both apps agree on is the durable contract; the deep link (`mercs2-modkit://ship`) is
/// fired over it so Modkit ingests the drop immediately instead of waiting for a hand-add. Nothing
/// here writes into a game folder — install and undo stay Modkit's job, with the placement record to
/// match.
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
    // Hand off via the deep link so Modkit ingests the Shipment through Quartermaster and merges it
    // into the deployed WAD — the point of this button over Reveal. Only when Modkit is clearly not
    // installed (its scheme is unregistered) do we fall back to opening the folder, which is the old
    // manual-add behavior and avoids an OS "no app for this link" dialog.
    if modkit_scheme_registered() {
        let url = modkit_ship_url(&dest);
        if fire_deep_link(&url).is_err() {
            let _ = open_in_os(&dest);
        }
    } else {
        let _ = open_in_os(&dest);
    }
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

/// The custom URL scheme Modkit registers. `send_to_modkit` fires `<scheme>://ship?path=…` so Modkit
/// ingests the handed-off Shipment through Quartermaster instead of the user re-adding it by hand.
const MODKIT_SCHEME: &str = "mercs2-modkit";

/// Build the deep-link URL that tells Modkit to ingest the Shipment at `dest`.
///
/// Shape: `mercs2-modkit://ship?path=<percent-encoded absolute path>`. Split out from the firing so
/// the (fiddly, Windows-path) encoding is unit-testable without spawning anything.
fn modkit_ship_url(dest: &Path) -> String {
    format!(
        "{MODKIT_SCHEME}://ship?path={}",
        percent_encode(&dest.to_string_lossy())
    )
}

/// Percent-encode everything outside the RFC 3986 unreserved set, so a Windows path (spaces,
/// backslashes, the drive colon) survives inside a URL query. Deliberately tiny — a whole crate for
/// one query parameter is not worth the dependency.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Best-effort check that Modkit has registered its URL scheme. Only definitive on Windows, via the
/// HKCR key the installer writes; elsewhere we assume yes and let the OS URL handler sort it out.
///
/// The point is the fallback: without this, firing the link on a machine that has never installed
/// Modkit pops an OS "no app for this link" dialog. When the key is absent we open the folder
/// instead — the same manual-add path this button had before it learned the deep link.
fn modkit_scheme_registered() -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("reg")
            .args(["query", &format!("HKCR\\{MODKIT_SCHEME}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            // If the probe itself won't run, prefer firing the link — that is the intended path, and
            // the OS handler is the next line of defense.
            .unwrap_or(true)
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Fire a URL through the OS's registered handler for its scheme. Mirrors [`open_in_os`] but hands
/// the shell a URL rather than a filesystem path.
fn fire_deep_link(url: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
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

    /// The deep-link URL Modkit receives must survive a Windows path: spaces, backslashes and the
    /// drive colon are all reserved in a query and have to come back byte-for-byte, or Modkit
    /// resolves the wrong folder (or none).
    #[test]
    fn modkit_ship_url_percent_encodes_a_windows_path() {
        let url = modkit_ship_url(Path::new(r"C:\Users\Ada\AppData\Roaming\mercs2-modkit\shipments\My Mod"));
        assert_eq!(
            url,
            "mercs2-modkit://ship?path=C%3A%5CUsers%5CAda%5CAppData%5CRoaming%5Cmercs2-modkit%5Cshipments%5CMy%20Mod"
        );
        // Unreserved characters (letters, digits, - _ . ~) are left as-is; everything else is %XX.
        assert!(percent_encode("aZ0-_.~").eq("aZ0-_.~"));
        assert_eq!(percent_encode("/ :\\"), "%2F%20%3A%5C");
    }

    /// ★ The World-domain → Shipment path: routing a LAYER by name through the two overlay kinds
    /// must seed the `layer` field of the resulting contribution, or the scaffolded edit would name
    /// the stub's placeholder instead of the layer the author clicked.
    #[test]
    fn a_routed_layer_seeds_the_layer_field_of_its_kind() {
        // The routes the World domain's layer menu offers.
        let kinds: Vec<&str> = routes_for_layer().iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds, vec!["activate_layer", "edit_world"]);

        match seeded("activate_layer", "vz_state_pmccon004_destroyed", 1).unwrap() {
            Contribution::ActivateLayer { layer, .. } => {
                assert_eq!(layer, "vz_state_pmccon004_destroyed", "the routed layer name must seed `layer`")
            }
            other => panic!("activate_layer seeded the wrong kind: {other:?}"),
        }
        match seeded("edit_world", "vz_state_pmccon004", 1).unwrap() {
            Contribution::EditWorld { layer, .. } => {
                assert_eq!(layer, "vz_state_pmccon004", "the routed layer name must seed `layer`")
            }
            other => panic!("edit_world seeded the wrong kind: {other:?}"),
        }
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
            model: Some(PathBuf::from(model)),
            donor: Some("pmc_hum_mattias".into()),
            textures: Textures::default(),
            retarget: Some(QmRetarget { from: "mixamo".into(), bones: Some(bones) }),
            single_group: false,
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

    // ---- dependencies editor ------------------------------------------------------------

    fn draft(form: RequireForm, target: &str, range: &str) -> DepDraft {
        DepDraft {
            form,
            target: target.into(),
            range: range.into(),
        }
    }

    /// Every requirement form round-trips through the editor's draft unchanged.
    #[test]
    fn every_requirement_form_round_trips_through_a_draft() {
        for r in [
            Requirement::Shipment("lua-bridge".into()),
            Requirement::ShipmentRange(ShipmentReq {
                shipment: "lua-bridge".into(),
                version: "^1.0.0".into(),
            }),
            Requirement::Capability(CapabilityReq {
                capability: "widescreen".into(),
            }),
        ] {
            let d = DepDraft::of_requirement(&r);
            assert_eq!(requirement_from_draft(&d, "ess"), Ok(r.clone()), "{d:?}");
        }
        for c in [
            ConflictDecl::Name("old-ess".into()),
            ConflictDecl::Range(ShipmentReq {
                shipment: "old-ess".into(),
                version: "<0.7".into(),
            }),
        ] {
            let d = DepDraft::of_conflict(&c);
            assert_eq!(conflict_from_draft(&d, "ess"), Ok(c.clone()), "{d:?}");
        }
    }

    /// A bad range is refused with M0172 and the SAME parse error qm's validation gives — one semver
    /// implementation, not a second one in the UI.
    #[test]
    fn a_bad_range_is_m0172_with_qms_own_message() {
        let bad = ">= one";
        let want = mercs2_quartermaster::manifest::parse_range(bad).unwrap_err();
        let err = requirement_from_draft(&draft(RequireForm::ShipmentRange, "lua-bridge", bad), "ess")
            .unwrap_err();
        assert!(err.starts_with("M0172") && err.contains(&want), "{err}");
        let err = conflict_from_draft(&draft(RequireForm::ShipmentRange, "old-ess", bad), "ess")
            .unwrap_err();
        assert!(err.starts_with("M0172") && err.contains(&want), "{err}");

        // The manifest validator refuses the same range with the same text.
        let text = format!(
            "format: 2\nshipment: {{ name: ess, version: 0.7.0, target: retail }}\n\
             load: {{ requires: [{{ shipment: lua-bridge, version: \"{bad}\" }}] }}\n"
        );
        match mercs2_quartermaster::from_str(&text, mercs2_quartermaster::Format::Yaml) {
            Err(mercs2_quartermaster::ReadError::Validate(v)) => {
                assert_eq!(v.code(), Some("M0172"));
                assert!(v.to_string().contains(&want));
            }
            other => panic!("expected a validation failure, got {other:?}"),
        }
    }

    /// The two reference rules validation applies: a slug, and not this Shipment.
    #[test]
    fn a_draft_names_a_real_shipment_that_is_not_this_one() {
        let e = requirement_from_draft(&draft(RequireForm::Shipment, "Lua Bridge", ""), "ess")
            .unwrap_err();
        assert!(e.starts_with("M0100"), "{e}");
        let e = requirement_from_draft(&draft(RequireForm::Shipment, "ess", ""), "ess").unwrap_err();
        assert!(e.starts_with("M0173"), "{e}");
        let e = conflict_from_draft(&draft(RequireForm::ShipmentRange, "ess", "^1"), "ess").unwrap_err();
        assert!(e.starts_with("M0173"), "{e}");
        assert!(requirement_from_draft(&draft(RequireForm::Capability, "  ", ""), "ess").is_err());
        assert!(
            conflict_from_draft(&draft(RequireForm::Capability, "widescreen", ""), "ess").is_err(),
            "a conflict has no capability form"
        );
    }

    /// Whitespace around a name or range is not written into the manifest.
    #[test]
    fn a_draft_is_trimmed() {
        assert_eq!(
            requirement_from_draft(&draft(RequireForm::ShipmentRange, " lua-bridge ", " ^1.0.0 "), "ess"),
            Ok(Requirement::ShipmentRange(ShipmentReq {
                shipment: "lua-bridge".into(),
                version: "^1.0.0".into(),
            }))
        );
    }

    /// The `{ name, version }` form (which validation refuses) is shown as the ranged form it must
    /// become.
    #[test]
    fn the_name_version_form_is_shown_as_a_ranged_requirement() {
        let r = Requirement::Compatible(mercs2_quartermaster::manifest::CompatibleReq {
            name: "lua-bridge".into(),
            version: "^1.0.0".into(),
        });
        assert_eq!(
            DepDraft::of_requirement(&r),
            draft(RequireForm::ShipmentRange, "lua-bridge", "^1.0.0")
        );
    }

    /// Entries the editor produces are what the manifest takes: written through the page's own
    /// `mutate`, the Shipment reopens and validates with every form in place.
    #[test]
    fn edited_requires_and_conflicts_are_written_and_reopen() {
        let d = tmp("deps");
        let mut p = Panel::default();
        p.scaffold(&d, None).expect("scaffold");
        let own = p.shipment.as_ref().unwrap().manifest.shipment.name.clone();
        let requires: Vec<Requirement> = [
            draft(RequireForm::Shipment, "lua-bridge", ""),
            draft(RequireForm::ShipmentRange, "ess", ">=0.7, <1"),
            draft(RequireForm::Capability, "widescreen", ""),
        ]
        .iter()
        .map(|x| requirement_from_draft(x, &own).expect("valid"))
        .collect();
        let conflicts: Vec<ConflictDecl> = [
            draft(RequireForm::Shipment, "old-ui", ""),
            draft(RequireForm::ShipmentRange, "ess", "<0.7"),
        ]
        .iter()
        .map(|x| conflict_from_draft(x, &own).expect("valid"))
        .collect();
        p.mutate(None, |m| {
            m.load.requires = requires.clone();
            m.load.conflicts = conflicts.clone();
        })
        .expect("write");
        let s = p.shipment.as_ref().expect("the edited Shipment reopens");
        assert_eq!(s.manifest.load.requires, requires);
        assert_eq!(s.manifest.load.conflicts, conflicts);
    }

    /// Reached from a folder picker, so picking the wrong folder must not destroy someone's work.
    #[test]
    fn scaffold_refuses_to_overwrite_an_existing_manifest() {
        let d = tmp("nooverwrite");
        let mut p = Panel::default();
        p.scaffold(&d, None).unwrap();
        std::fs::write(d.join("manifest.yaml"), "format: 2\n# hand-edited\n").unwrap();

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
            Contribution::AddOutfit { model, .. } => {
                assert_eq!(model, &Some(PathBuf::from("src/c.glb")))
            }
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

    /// Every name the category combo offers is a category of the game's tree: the encoder refuses a
    /// group whose category hash is not in `RETAIL_CATEGORIES`.
    #[test]
    fn every_offered_sound_category_hashes_into_the_tree() {
        use mercs2_audio::encode::{RETAIL_CATEGORIES, RETAIL_CATEGORY_NAMES};
        for name in RETAIL_CATEGORY_NAMES {
            let h = mercs2_formats::hash::pandemic_hash_m2(name);
            assert!(
                RETAIL_CATEGORIES.iter().any(|c| c.category == h),
                "{name} = 0x{h:08X} is not in RETAIL_CATEGORIES"
            );
        }
    }

    /// Each sound stub is written by the page's own `mutate` and reopens unchanged, with the
    /// linter reporting what the author still has to enter.
    #[test]
    fn sound_stubs_are_written_and_reopen() {
        for kind in ["add_sound", "replace_sound_bank", "replace_sound_cue"] {
            let d = tmp(&format!("sound_stub_{kind}"));
            let mut p = Panel::default();
            p.scaffold(&d, None).expect("scaffold");
            let c = stub(kind, 1).expect("stub");
            p.mutate(None, |m| m.contributions.push(c.clone())).expect("write");
            let s = p.shipment.as_ref().expect("the Shipment reopens");
            assert_eq!(s.manifest.contributions, vec![c], "{kind} changed across the round trip");
            let codes: Vec<&str> = p.findings_for(0).map(|d| d.rule.code).collect();
            assert!(codes.contains(&"M0216"), "{kind}: the unchosen category is not reported: {codes:?}");
            assert!(codes.contains(&"M0215"), "{kind}: the missing name or cues are not reported: {codes:?}");
            assert_eq!(codes.contains(&"M0221"), kind == "add_sound", "{kind}: the unchosen load_in: {codes:?}");
        }
    }

    /// The preview names each session's loader for a sound kind: an `add_sound` its `load_in`, an
    /// override the sessions retail Lua loads its bank in (`ui_shell`: the front end only; `ui_hud`:
    /// both), and none for a bank the engine loads (`veh_jeep`).
    #[test]
    fn sound_previews_name_each_sessions_loader() {
        use mercs2_quartermaster::manifest::LoadSession;
        let scripts = |c: &Contribution| -> Vec<String> {
            blast_rows(c).into_iter().filter(|(k, _)| k == "Script").map(|(_, v)| v).collect()
        };
        let Contribution::AddSound { bank, category, cues, .. } = stub("add_sound", 1).unwrap() else {
            panic!("the add_sound stub")
        };
        let added = |load_in: Vec<LoadSession>| Contribution::AddSound {
            bank: bank.clone(),
            category: category.clone(),
            cues: cues.clone(),
            load_in,
        };
        let front = scripts(&added(vec![LoadSession::FrontEnd]));
        assert_eq!(front.len(), 1);
        assert!(front[0].starts_with("front end: qm_shell_modloader"), "{front:?}");
        assert_eq!(scripts(&added(LoadSession::ALL.to_vec())).len(), 2);
        assert_eq!(scripts(&added(Vec::new())), vec!["no session loads the bank".to_string()]);

        let cue = |bank: &str| {
            let Contribution::ReplaceSoundCue { category, cue, .. } = stub("replace_sound_cue", 1).unwrap() else {
                panic!("the replace_sound_cue stub")
            };
            Contribution::ReplaceSoundCue { bank: bank.into(), language: None, category, cue }
        };
        let shell = scripts(&cue("ui_shell"));
        assert_eq!(shell.len(), 1, "{shell:?}");
        assert!(shell[0].starts_with("front end:"), "{shell:?}");
        assert_eq!(scripts(&cue("ui_hud")).len(), 2);
        let engine = scripts(&cue("veh_jeep"));
        assert_eq!(engine.len(), 1, "{engine:?}");
        assert!(engine[0].starts_with("none: the engine loads veh_jeep"), "{engine:?}");
    }

    /// A whole-number field takes decimal or `0x` hex and refuses a value its type cannot hold.
    #[test]
    fn whole_number_fields_take_decimal_or_hex() {
        assert_eq!(parse_whole::<u32>("0xEA1343AA", u64::from(u32::MAX)), Ok(0xEA13_43AA));
        assert_eq!(parse_whole::<u32>("0XEA1343AA", u64::from(u32::MAX)), Ok(0xEA13_43AA));
        assert_eq!(parse_whole::<u32>(" 1024 ", u64::from(u32::MAX)), Ok(1024));
        assert_eq!(parse_whole::<u8>("255", u64::from(u8::MAX)), Ok(255));
        assert!(parse_whole::<u8>("256", u64::from(u8::MAX)).unwrap_err().contains("out of range"));
        assert!(parse_whole::<u16>("0x10000", u64::from(u16::MAX)).unwrap_err().contains("out of range"));
        assert_eq!(parse_whole::<u32>("", u64::from(u32::MAX)), Err("required".to_string()));
        assert!(parse_whole::<u32>("-1", u64::from(u32::MAX)).is_err());
        assert!(parse_whole::<u32>("0xZZ", u64::from(u32::MAX)).is_err());
    }

    /// A float field takes a finite number only: the builder refuses a non-finite cue field.
    #[test]
    fn float_fields_take_finite_numbers_only() {
        assert_eq!(parse_f32("-6.5"), Ok(-6.5));
        assert_eq!(parse_f64("-6.02"), Ok(-6.02));
        assert!(parse_f32("1e40").is_err(), "overflows f32 to infinity");
        assert!(parse_f32("NaN").is_err());
        assert!(parse_f64("inf").is_err());
        assert_eq!(parse_f64(""), Err("required".to_string()));
    }

    /// The build keeps the configured stack in its order (base, patch, overlays) and opens the WAD
    /// of each language the Shipment reads after it; with no language, it is the configured stack.
    #[test]
    fn the_build_stack_is_the_configured_stack_then_the_language_wads() {
        use mercs2_quartermaster::manifest::Language;
        let data = tmp("build_stack").join("data");
        std::fs::create_dir_all(&data).unwrap();
        for f in ["vz.wad", "vz-patch.wad", "French.wad"] {
            std::fs::write(data.join(f), b"").unwrap();
        }
        let overlay = tmp("build_stack_overlay").join("mine.wad");
        std::fs::write(&overlay, b"").unwrap();
        let stack: Vec<String> = [data.join("vz.wad"), data.join("vz-patch.wad"), overlay.clone()]
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        let d = tmp("build_stack_shipment");
        let mut p = Panel::default();
        p.scaffold(&d, None).expect("scaffold");
        let plain = p.shipment.as_ref().unwrap().manifest.clone();
        assert_eq!(
            build_stack_paths(&stack, &plain).unwrap(),
            stack.iter().map(PathBuf::from).collect::<Vec<_>>()
        );

        let mut c = stub("replace_sound_bank", 1).unwrap();
        if let Contribution::ReplaceSoundBank { bank, language, .. } = &mut c {
            *bank = "vo_mattias".into();
            *language = Some(Language::French);
        }
        p.mutate(None, |m| m.contributions.push(c)).expect("write");
        let french = p.shipment.as_ref().unwrap().manifest.clone();
        assert_eq!(
            build_stack_paths(&stack, &french).unwrap(),
            vec![data.join("vz.wad"), data.join("vz-patch.wad"), overlay, data.join("French.wad")]
        );

        assert!(build_stack_paths(&[], &plain).unwrap_err().contains("vz.wad"));
        std::fs::remove_file(data.join("French.wad")).unwrap();
        assert!(build_stack_paths(&stack, &french).unwrap_err().contains("french"));
    }

    /// Every kind a domain offers under "Add to Shipment" has a stub for its button to add.
    #[test]
    fn every_domain_kind_has_a_stub() {
        for d in crate::domain::Domain::ALL {
            for k in d.kinds() {
                assert!(stub(k, 1).is_some(), "{k} is offered by a domain but has no stub");
            }
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
