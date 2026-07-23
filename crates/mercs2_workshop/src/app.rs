//! The workshop window: a native, engine-rendered developer tool.
//!
//! One winit loop over the engine's `Scene` (the SAME renderer `mercs2_game` boots — materials,
//! skinning, lighting, shadow are all the faithful path, not an export approximation). The app is
//! organized as workbenches; this file implements the first one, the ASSET workbench:
//!
//! - browse/search every model + texture in the WAD (registry names via `index::AssetIndex`)
//! - preview a model with its real materials/textures, orbit camera, animation clips
//! - inspect layers: HIER bone tree, per-material draw groups (isolate/hide sub-strips),
//!   full-screen texture plates
//! - an EDITABLE sandbox: place instances, move/rotate/scale them, save/load the arrangement
//!   (`workshop_scene.json`) — experiments in isolation, no game boot required
//!
//! Future workbenches (mission design, model import/fix, AV replacement, unlock auditing) mount
//! into the same loop — see `docs/modernization/workshop_charter.md`.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3};
use mercs2_core::{AnimState, Entity, ModelRef, SkinPalette, Transform, World};
use mercs2_engine::mesh::{self, BoneRig, DrawGroup};
use mercs2_formats::anim_select::AnimSelector;
use mercs2_engine::render::{ClipAnim, TexMap};
use mercs2_engine::scene::Scene;
use mercs2_engine::{game_world, pose, wad};
use serde::{Deserialize, Serialize};
use winit::event::{ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::WindowBuilder;

use crate::index::{AssetIndex, Kind};

const IDENTITY: [[f32; 4]; 4] =
    [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

// Boot-screen text scale (the engine's own overlay pass — the in-app GUI is egui, see gui.rs).
const UI_S: f32 = 2.0; // 16px glyphs

/// Text with a 2px drop shadow — keeps the overlay legible over bright 3D content.
fn text_sh(scene: &mut Scene, x: f32, y: f32, scale: f32, color: [f32; 4], s: &str) {
    scene.ui_text(x + 2.0, y + 2.0, scale, [0.0, 0.0, 0.0, 0.9], s);
    scene.ui_text(x, y, scale, color, s);
}

pub struct Options {
    pub wadpath: String,
    /// Overlay WADs opened ON TOP of the base, in load order — the game's own patch mechanism
    /// (`data/vz-patch.wad`): a later archive's asset wins over an earlier one's.
    pub overlays: Vec<String>,
    pub names_csv: Option<PathBuf>,
}

/// The open archive set: base + overlays. Asset resolution walks the stack in REVERSE (last
/// overlay first, base last) — the retail "last-opened wins" patch rule — so DLC/patch content
/// shadows base assets exactly as in the running game.
pub(crate) struct WadStack {
    pub wads: Vec<wad::Wad>,
    pub labels: Vec<String>,
    /// The engine's block-residency + hash-keyed chunk registry. The workshop must resolve assets the
    /// way the game does — make the owning block resident, register every chunk it carries, first-wins
    /// — or it is inspecting containers in isolation rather than simulating the engine.
    /// See `mercs2_engine::registry` and `docs/modernization/model_render_gate_spec.md` §2b.
    pub registry: mercs2_engine::registry::AssetRegistry,
    /// Memo for [`WadStack::texture_best`]. Assembling a texture's full mip chain means walking its
    /// whole c3-cell subtree, and a model's materials share textures (and cells) heavily — so the
    /// uncached cost was paid once per material slot, which is what made full-res "too slow" and got
    /// the resident tail wired into model loads instead. Cached, each texture is walked once.
    tex_cache: HashMap<u32, mercs2_formats::texture::TextureData>,
}

impl WadStack {
    pub fn open(base: &str, overlays: &[String]) -> Result<WadStack, String> {
        let mut wads = vec![wad::open(base)?];
        let mut labels = vec![base.to_string()];
        for o in overlays {
            match wad::open(o) {
                Ok(w) => {
                    eprintln!("[workshop] overlay: {o}");
                    wads.push(w);
                    labels.push(o.clone());
                }
                Err(e) => eprintln!("[workshop] overlay {o}: {e} (skipped)"),
            }
        }
        Ok(WadStack { wads, labels, registry: Default::default(), tex_cache: HashMap::new() })
    }

    /// Short display tag for an asset's source wad (base = "", overlays = "+<file stem>").
    pub fn tag(&self, src: usize) -> String {
        if src == 0 {
            return String::new();
        }
        let stem = std::path::Path::new(&self.labels[src])
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("overlay");
        format!("+{stem}")
    }

    /// Model container by hash, resolved through the engine's residency registry — the owning block
    /// goes resident and registers all of its chunks, exactly as the game's block streaming does.
    pub fn extract_container(&mut self, hash: u32) -> Result<Vec<u8>, String> {
        let WadStack { wads, registry, .. } = self;
        registry
            .resolve(wads, wad::MODEL_TYPE_HASH, hash)
            .and_then(|c| registry.slice(c).map(<[u8]>::to_vec))
            .ok_or_else(|| format!("0x{hash:08X}: no model chunk in any open wad"))
    }

    /// The assembled model — LOD-block chain joined through the resident block. Overlays first.
    pub fn model(&mut self, hash: u32) -> Result<mercs2_engine::model::Model, String> {
        let mut last = format!("0x{hash:08X}: no model chunk in any open wad");
        for i in (0..self.wads.len()).rev() {
            match mercs2_engine::model::Model::load(&mut self.wads[i], hash) {
                Ok(m) => return Ok(m),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Resident-mip texture (fast path — model loads).
    pub fn texture_resident(&mut self, hash: u32) -> Result<mercs2_formats::texture::TextureData, String> {
        let mut last = format!("0x{hash:08X}: not in any open wad");
        for i in (0..self.wads.len()).rev() {
            match wad::extract_texture(&mut self.wads[i], hash) {
                Ok(t) => return Ok(t),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Full streamed resolution — the texture's higher mips assembled from the finer LOD blocks of
    /// its own c3-cell subtree (`wad::extract_texture_hires`). This is what EVERY consumer wants:
    /// the resident block alone ships a coarse mip tail, so a 1024² normal map resolves to 32×32 and
    /// a PNG written from it is a mostly-empty plate. Memoized, because the subtree walk is the only
    /// reason anything settled for the tail.
    pub fn texture_best(&mut self, hash: u32) -> Result<mercs2_formats::texture::TextureData, String> {
        if let Some(t) = self.tex_cache.get(&hash) {
            return Ok(t.clone());
        }
        let mut last = format!("0x{hash:08X}: not in any open wad");
        for i in (0..self.wads.len()).rev() {
            let w = &mut self.wads[i];
            match wad::extract_texture_hires(w, hash).or_else(|_| wad::extract_texture(w, hash)) {
                Ok(t) => {
                    self.tex_cache.insert(hash, t.clone());
                    return Ok(t);
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// Generic rig-matched clips: first wad (reverse order) whose animgroups drive this rig.
    pub fn clips_for_model(&mut self, rig: &[BoneRig]) -> Vec<ClipAnim> {
        for i in (0..self.wads.len()).rev() {
            let v = game_world::load_clips_for_model(&mut self.wads[i], rig);
            if !v.is_empty() {
                return v;
            }
        }
        Vec::new()
    }

    /// A specific clip bound to a rig — searched across the whole stack (a DLC model's clips may
    /// ship in the patch while the character tables live in the base).
    pub fn clip_for_rig(&mut self, hier: &[u32], want: u32) -> Option<ClipAnim> {
        for i in (0..self.wads.len()).rev() {
            if let Some(c) = game_world::load_clip_for_rig(&mut self.wads[i], hier, Some(want)) {
                return Some(c);
            }
        }
        None
    }

    /// The resident AnimationLookup — base first (that's where block 3185 lives).
    pub fn anim_selector(&mut self) -> Option<AnimSelector> {
        for i in 0..self.wads.len() {
            if let Some(s) = load_anim_selector(&mut self.wads[i]) {
                return Some(s);
            }
        }
        None
    }
}

/// Background clip loader: a worker thread with its OWN `WadStack`, so the per-clip animgroup
/// scan + wavelet decode never stalls the frame loop (a cold clip takes long enough to read as
/// a freeze). Results are drained once per frame; `inflight` drives the status-bar spinner and
/// dedupes repeat clicks.
struct ClipLoader {
    tx: std::sync::mpsc::Sender<ClipJob>,
    rx: std::sync::mpsc::Receiver<ClipDone>,
    /// (preview hash, clip hash) requests not yet answered.
    inflight: HashSet<(u32, u32)>,
}

struct ClipJob {
    preview: u32,
    hier: Vec<u32>,
    hash: u32,
}

struct ClipDone {
    preview: u32,
    hash: u32,
    clip: Option<ClipAnim>,
}

impl ClipLoader {
    fn spawn(base: String, overlays: Vec<String>) -> ClipLoader {
        let (tx, jrx) = std::sync::mpsc::channel::<ClipJob>();
        let (dtx, rx) = std::sync::mpsc::channel::<ClipDone>();
        std::thread::spawn(move || {
            // Own WAD handles (file positions are stateful — the render loop's stack can't be
            // shared). Opened lazily on the first job so boot isn't paying for it.
            let mut stack: Option<WadStack> = None;
            while let Ok(job) = jrx.recv() {
                if stack.is_none() {
                    match WadStack::open(&base, &overlays) {
                        Ok(s) => stack = Some(s),
                        Err(e) => {
                            eprintln!("[workshop] clip loader: {e}");
                            return;
                        }
                    }
                }
                let clip = stack.as_mut().unwrap().clip_for_rig(&job.hier, job.hash);
                if dtx.send(ClipDone { preview: job.preview, hash: job.hash, clip }).is_err() {
                    return;
                }
            }
        });
        ClipLoader { tx, rx, inflight: HashSet::new() }
    }

    /// Queue a decode+bind unless the same (preview, clip) is already in flight.
    fn request(&mut self, preview: u32, hier: &[u32], hash: u32) {
        if self.inflight.insert((preview, hash)) {
            let _ = self.tx.send(ClipJob { preview, hier: hier.to_vec(), hash });
        }
    }
}

/// Everything loaded for the model currently on the preview pedestal.
struct Preview {
    hash: u32,
    label: String,
    entity: Entity,
    rig: Vec<BoneRig>,
    /// Bind-pose palette (identity at bind) — restored when animation is switched off.
    bind: Vec<[[f32; 4]; 4]>,
    draws: Vec<DrawGroup>,
    tex_hashes: Vec<u32>,
    /// The clip catalog: the CHARACTER-SPECIFIC set resolved through the AnimationLookup chain
    /// (ActionTable Handle → CharacterName row → ASTO clip) when the model maps to a
    /// CharacterName; else the generic rig-matched animgroup clips. See
    /// `docs/modernization/human_animation_selection.md`.
    clip_catalog: Vec<ClipEntry>,
    /// Which path filled the catalog (shown in the panels).
    character_set: Option<String>,
    /// clip hash → clip decoded + bound to THIS rig (`None` = tried, not bindable).
    clip_cache: HashMap<u32, Option<ClipAnim>>,
    /// HIER bone name-hashes (clip binding input). For a retargeted import this is the TARGET
    /// (source-of-animation) skeleton's hashes IN TARGET ORDER, so a clip binds `track_to_hier`
    /// against the target rig that `retarget_source` carries.
    hier: Vec<u32>,
    /// Present only for a retargeted foreign import: `(source_rig, target_to_source)` where
    /// `source_rig` is the game skeleton the clips are authored for (e.g. Jen) and
    /// `target_to_source[j]` maps this preview's own bone `j` → the source-rig bone that drives it.
    /// When set, the per-frame sampler uses the CROSS-SKELETON retarget (`havok_palette_retarget_cross`)
    /// so the imported mesh animates without being deformed. `None` for native models.
    retarget_source: Option<(Vec<BoneRig>, Vec<usize>)>,
    /// SEGM state/LOD tiers the container carries + the built one (F11 cycles).
    tiers: Vec<u8>,
    tier: u8,
    /// The engine's named-state machine + HIER/INDX, and the CHOSEN state per switch node
    /// (defaults follow each node's own init script). Visibility = executing those states'
    /// SHOW/Hide — game data only.
    machine: Option<mercs2_formats::orchestrator::StateMachine>,
    hier_nodes: Vec<mercs2_formats::orchestrator::HierNode>,
    indx: Vec<usize>,
    node_state: Vec<usize>,
    /// The object's HEALTH fraction (1.0 = full, 0.0 = destroyed). Drives the destruction machine:
    /// `node_states_for_health` picks each node's state, so the inspector shows the real pristine →
    /// damaged → wreck progression instead of hand-picked states.
    health: f32,
    /// Eyeball-test toggle: hide `*_ruin*` sub-strips to judge whether they're legit geometry.
    hide_ruin: bool,
    /// The live node-enable table (clause 3) the gate is using — so the inspector can say WHY a mesh
    /// is hidden (LOD-mask miss vs its HIER node switched off by the destruction state).
    node_enable: Vec<bool>,
    /// The model's authored header (AABB / node count / LOD-level count / LOD distance), for the
    /// LOD panel + camera framing. `None` for imports.
    header: Option<mercs2_formats::model_cubeize::ModelHeader>,
    /// Game scripts mentioning this asset: literal search hits from the decompiled corpus
    /// (needle shown alongside — these are search results, not classifications).
    lua_needle: String,
    lua_refs: Vec<(String, Vec<String>)>,
    cur_clip: Option<usize>,
    anim_time: f32,
    playing: bool,
    sel_group: usize,
    hidden: HashSet<usize>,
    verts: usize,
    tris: usize,
    center: Vec3,
    radius: f32,
}

/// One row of the preview's clip catalog.
struct ClipEntry {
    hash: u32,
    /// Every ActionTable Handle this clip answers (character-specific path only; empty for the
    /// generic rig-matched fallback). Drives the loadout-compatibility table.
    handles: Vec<u32>,
    /// Display name: the corpus name when the hash resolves, else a deterministic PROCEDURAL
    /// name built from the animation-table rows that select this clip (see
    /// `procedural_clip_name`). `None` when neither source names it.
    name: Option<String>,
    /// `name` or the hex hash — status messages and copy actions.
    label: String,
}

/// One instance placed into the editable sandbox.
struct Placed {
    hash: u32,
    label: String,
    entity: Entity,
    pos: Vec3,
    yaw: f32,
    scale: f32,
}

/// Serialized sandbox arrangement (`workshop_scene.json`).
#[derive(Serialize, Deserialize)]
struct SceneFile {
    items: Vec<SceneItem>,
}

#[derive(Serialize, Deserialize)]
struct SceneItem {
    hash: u32,
    name: Option<String>,
    pos: [f32; 3],
    yaw: f32,
    scale: f32,
}

/// Full-screen texture plate view (rendered through the engine's loading-plate path).
struct TexView {
    hashes: Vec<u32>,
    labels: Vec<String>,
    idx: usize,
    /// Dims/format of the currently bound plate (for the caption).
    info: String,
}

const SCENE_FILE: &str = "workshop_scene.json";

/// One user intention, queued from EITHER a keyboard shortcut or an inspector-GUI widget and
/// executed once per frame where every `&mut` is free — so the GUI and the keys share one
/// implementation instead of two drifting copies.
enum Act {
    /// Load the browser row at `filtered[i]` (models → preview, textures → plate view).
    LoadRow(usize),
    Tier(u8),
    TierNext,
    ClipSel(usize),
    ClipNav(i32),
    ClipStop,
    PlayPause,
    GroupToggle(usize),
    Place,
    /// Context-menu: place a catalog model into the sandbox WITHOUT making it the preview.
    PlaceHash(u32, String),
    Merge,
    Export,
    /// Context-menu: export a catalog model directly (no preview needed).
    ExportHash(u32, String),
    SaveScene,
    LoadScene,
    ClearSandbox,
    RemovePlaced(usize),
    DuplicatePlaced(usize),
    /// Move a placed instance to the current camera target.
    SnapPlaced(usize),
    /// Push a placed instance's (GUI-edited) transform into its ECS entity.
    SyncPlaced(usize),
    /// Hide every draw group except this one / show them all again.
    IsolateGroup(usize),
    ShowAllGroups,
    /// Put switch node `.0` into its state index `.1` and re-execute the machine's SHOW/Hide.
    NodeState(usize, usize),
    /// Set the object's HEALTH fraction (0..1) and drive the destruction machine from it — pristine
    /// at full, damaged/on-fire as it drops, wreck at 0. The systemic replacement for hand-picking
    /// per-node states.
    SetHealth(f32),
    /// Toggle hiding every draw group whose diffuse is a `*_ruin*` material (the eyeball test).
    ToggleRuin,
    TexOfPreview,
    TexNav(i32),
    TexClose,
    /// Open a decompiled game script (corpus path) in the read-only Lua viewer.
    LuaOpen(String),
    /// Mod project: set the donor model for the next new asset.
    ModDonor(u32, String),
    /// Mod project: package the current imported preview as a new asset named this.
    ModAdd(String),
    /// Mod project: drop item i.
    ModRemove(usize),
    /// Publish the mod project to a patch WAD (background worker).
    Publish,
    /// Conform: seed the transform fields from the real-envelope auto-fit (donor vs import).
    ConformAutofit,
    /// Conform: place the donor template into the sandbox at the origin as a visual reference.
    LoadDonorRef,
    /// Load a model by hash onto the preview pedestal (Model Workbench inventory click).
    LoadModelHash(u32, String),
    /// Skeleton workbench: set the target character skeleton and (re)compute the bone map.
    RetargetSetTarget(u32, String),
    /// Skeleton workbench: recompute the source→target bone map from the current source + target.
    RetargetRemap,
    /// Skeleton workbench: apply the retarget — remap the import's per-vertex joints/weights onto the
    /// target HIER skeleton and rebuild the preview with the conformed skinning.
    RetargetApply,
    /// Skeleton workbench: user override — map source bone `.0` onto target bone `.1` (`None` = clear).
    RetargetManual(usize, Option<usize>),
    /// Skeleton workbench: fill the still-unmapped source bones by nearest target bone in 3D space.
    RetargetAlignPos,
    /// Open a native file picker and import the chosen .obj/.gltf/.glb (same path as drag-drop).
    ImportModel,
    /// Skeleton workbench: FAITHFUL export — re-pose the imported rig onto the target skeleton with
    /// shipped-format skinning (palette-relative BLENDINDICES + INFO(56) range table, via
    /// `mercs2_formats::char_skin`) and inject into the target donor block. Prompts for an output file.
    ExportFaithfulCharacter,
    /// Skeleton workbench: unload the current import from the GPU + the `imported` store and reset
    /// all retarget state (map / target / source path / selection), so a fresh model can be imported
    /// from a clean slate without restarting the app.
    ClearImport,
}

/// The workbench the tool is focused on. One persistent workspace: the activity rail switches which
/// workbench is active (navigator + inspector reconfigure); the viewport and camera never reset.
/// Replaces the old two-page Browser/Model-Workbench toggle.
#[derive(PartialEq, Clone, Copy)]
enum Workbench {
    /// Browse + read every asset: info, LOD, destruction, segments, animation, skeleton.
    Inspect,
    /// The editable placement scene: place/move/merge instances, save/load arrangements.
    Sandbox,
    /// Author a mod: conform an import onto a donor template, place hardpoints, publish a patch WAD.
    Mods,
    /// Retarget a Source-rigged (ValveBiped / Mixamo / Unreal) import onto a Mercs2 HIER skeleton.
    Skeleton,
}

impl Workbench {
    const ALL: [Workbench; 4] =
        [Workbench::Inspect, Workbench::Sandbox, Workbench::Mods, Workbench::Skeleton];
    fn label(self) -> &'static str {
        match self {
            Workbench::Inspect => "Inspect",
            Workbench::Sandbox => "Sandbox",
            Workbench::Mods => "Mods",
            Workbench::Skeleton => "Skeleton",
        }
    }
    /// The command-bar breadcrumb verb.
    fn verb(self) -> &'static str {
        match self {
            Workbench::Inspect => "Inspecting",
            Workbench::Sandbox => "Sandbox",
            Workbench::Mods => "Mod project",
            Workbench::Skeleton => "Retarget",
        }
    }
}

/// Model Workbench vehicle-class display order (helicopters first, per user).
const VEH_CLASS_ORDER: &[&str] = &[
    "helicopter", "tank", "apc", "vtol", "jet", "car", "truck", "van", "semi", "trailer", "towed",
    "motorcycle", "boat", "other",
];

/// Group the catalog's vehicle models by class for the workbench inventory.
/// Resolve HIER node-name hashes to names. The rainbow table (VERIFIED cracks — the human/vehicle
/// rigs, `hp_seat_*`, `bone_wheel_*`, `bone_ub`, …) is authoritative and consulted FIRST; the
/// generated `docs/data/bone_name_candidates.txt` grammar file is only a fallback for hashes the
/// rainbow table doesn't cover, so an unverified brute-force guess can never shadow a real name.
fn resolve_node_names(hashes: &[u32]) -> std::collections::HashMap<u32, String> {
    use mercs2_formats::hash::pandemic_hash_m2;
    let want_set: std::collections::BTreeSet<u32> = hashes.iter().copied().collect();
    // Verified first.
    let mut out = mercs2_engine::worldutil::rainbow_names(&want_set);
    // Fill only the gaps from the generated candidate grammar (unverified).
    if out.len() < want_set.len() {
        for root in ["docs/data/bone_name_candidates.txt", "../../docs/data/bone_name_candidates.txt"] {
            let Ok(txt) = std::fs::read_to_string(root) else { continue };
            for line in txt.lines() {
                let c = line.trim();
                if c.len() < 2 {
                    continue;
                }
                let h = pandemic_hash_m2(c);
                if want_set.contains(&h) {
                    out.entry(h).or_insert_with(|| c.to_string());
                }
            }
            break;
        }
    }
    out
}

fn build_vehicle_inventory(index: &crate::index::AssetIndex) -> Vec<(&'static str, Vec<(u32, String)>)> {
    let mut map: std::collections::HashMap<&'static str, Vec<(u32, String)>> = Default::default();
    for r in &index.models {
        if let Some(c) = r.vehicle_class() {
            map.entry(c).or_default().push((r.hash, r.label()));
        }
    }
    let mut out = Vec::new();
    for &c in VEH_CLASS_ORDER {
        if let Some(mut rows) = map.remove(c) {
            rows.sort_by(|a, b| a.1.cmp(&b.1));
            out.push((c, rows));
        }
    }
    out
}

pub fn run(opts: Options) {
    let mut clip_loader = ClipLoader::spawn(opts.wadpath.clone(), opts.overlays.clone());
    let mut w = match WadStack::open(&opts.wadpath, &opts.overlays) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("workshop: cannot open {}: {e}", opts.wadpath);
            return;
        }
    };
    // Catalog NOW (instant, hashes only); the name corpora (rainbow table + bone candidates +
    // registry, ~7s) load on a background thread and land via `apply_names` — the window opens
    // immediately instead of stalling on 70 MB of JSON.
    let mut index = AssetIndex::build(&w.wads, HashMap::new());
    eprintln!(
        "[workshop] catalog: {} models, {} textures ({} wad(s))",
        index.models.len(),
        index.textures.len(),
        w.wads.len()
    );
    // The resident AnimationLookup — the per-character clip resolver (parsed once). None on a
    // WAD without it; the preview then falls back to generic rig-matched clips.
    let anim_sel = w.anim_selector();
    if anim_sel.is_none() {
        eprintln!("[workshop] AnimationLookup not found — character-specific clips unavailable");
    }
    enum Boot {
        Prog(f32, &'static str),
        Done(HashMap<u32, String>, Vec<(String, String, String)>, f32),
    }
    let (ntx, nrx) = std::sync::mpsc::channel::<Boot>();
    {
        let csv = opts.names_csv.clone();
        let tx = ntx.clone();
        std::thread::spawn(move || {
            let t0 = std::time::Instant::now();
            let names = crate::index::load_all_names_staged(csv, |f, stage| {
                let _ = tx.send(Boot::Prog(f, stage));
            });
            let _ = tx.send(Boot::Prog(0.97, "Lua reference corpus"));
            let lua = crate::index::load_lua_corpus();
            let _ = ntx.send(Boot::Done(names, lua, t0.elapsed().as_secs_f32()));
        });
    }
    // Boot loading screen state: the app renders the engine's loading path (shell plate +
    // spinner + progress bar) until the name corpora are in, then drops into the browser.
    let mut names_pending = true;
    // Decompiled-Lua reference corpus (path, content, lowercased) — loaded on the boot thread.
    let mut lua_corpus: Vec<(String, String, String)> = Vec::new();
    let mut boot_target = 0.02f32; // stage-start fraction from the loader thread
    let mut boot_shown = 0.0f32; // eased bar position
    let mut boot_stage: &'static str = "asset catalog";

    let event_loop = EventLoop::new().expect("event loop");
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Mercenaries 2 — workshop")
            .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 810.0))
            .build(&event_loop)
            .expect("window"),
    );
    let mut scene = pollster::block_on(Scene::new(window.clone()));
    let mut world = World::new();
    // The inspector GUI (egui through the engine's overlay hook) + the frame's queued actions.
    let mut gui = crate::gui::Gui::new(scene.device(), scene.surface_format(), &window);
    let mut actions: Vec<Act> = Vec::new();
    // The game's own shell plate (shell.wad) as the workshop backdrop — the browse screen renders
    // through the engine's menu path over it until something is loaded into the 3D viewport.
    let shell_plate = match wad::shell_loading_plate(&opts.wadpath) {
        Ok(td) => Some(td),
        Err(e) => {
            eprintln!("[workshop] shell plate unavailable ({e}); flat backdrop");
            None
        }
    };
    // BOOT art: the authentic Loading.wad gold skull — the boot-mode loading shader draws it
    // pulsing on black with the warm sheen. The shell plate takes over as the browse backdrop
    // once boot completes.
    let boot_skull = std::path::Path::new(&opts.wadpath)
        .with_file_name("Loading.wad")
        .to_str()
        .and_then(|p| wad::open(p).ok())
        .and_then(|mut lw| {
            let h = mercs2_formats::hash::pandemic_hash_m2("global_loading_skull");
            wad::extract_texture(&mut lw, h).ok()
        });
    match &boot_skull {
        Some(td) => scene.set_loading_art(td),
        None => {
            eprintln!("[workshop] Loading.wad skull unavailable; boot screen without icon");
            if let Some(td) = &shell_plate {
                scene.set_loading_art(td);
            }
        }
    }

    // Browser state.
    let mut kind = Kind::Model;
    let mut filter = String::new();
    let mut filtered: Vec<usize> = (0..index.rows(kind).len()).collect();
    let mut sel: usize = 0;
    // Category headers the user has collapsed in the model list — click a header to fold its rows
    // away so only the category line remains. Persists across frames; keyed by the static category
    // name so it survives refilter/re-sort. Empty = everything expanded (the default).
    let mut collapsed_cats: HashSet<&'static str> = HashSet::new();
    let mut list_visible = true; // Tab: browse (list + filter typing) <-> edit (letter controls)

    // Viewport state.
    let mut preview: Option<Preview> = None;
    let mut placed: Vec<Placed> = Vec::new();
    // CPU-side store for models that do NOT live in a WAD: drag-drop imports + merge results.
    // Keyed by their synthetic hash; merge/export source geometry from here first.
    let mut imported: HashMap<u32, ModelData> = HashMap::new();
    let mut merge_seq = 0usize;
    // Skeleton highlighting: the bone row the pointer is over this frame (transient) + the
    // clicked/pinned one. Rendered as engine glow cards at the bone's POSED position.
    let mut sel_bone: Option<usize> = None;
    let mut sel_placed: Option<usize> = None;
    let mut tex_view: Option<TexView> = None;
    let mut lua_view: Option<crate::luaview::LuaView> = None;
    // Mod project (publish workbench M3): novel new-hash assets queued for packaging.
    let mut mod_items: Vec<crate::publish::NewModelItem> = Vec::new();
    let mut mod_name = String::new();
    let mut mod_donor: Option<(u32, String)> = None;
    let mut mod_group: usize = 0;
    // Output default: next to the base wad, NEVER vz-patch.wad (that's the DLC port's file).
    let mut mod_out: String = std::path::Path::new(&opts.wadpath)
        .parent()
        .map(|d| d.join("vz-mod.wad"))
        .unwrap_or_else(|| std::path::PathBuf::from("vz-mod.wad"))
        .to_string_lossy()
        .into_owned();
    let mut publisher: Option<crate::publish::Publisher> = None;
    // In-flight background F10 export of a WAD asset (the heavy full-clip-decode path).
    let mut exporter: Option<Exporter> = None;
    // ── Conform transform (dedicated panel): interactively scale/rotate/place the imported mesh
    // against the donor template, baked into the export. `conform_live` drives the preview entity
    // from these fields so the placement is visible against a donor reference in the sandbox. ──
    let mut conform_scale: f32 = 1.0;
    let mut conform_t: [f32; 3] = [0.0, 0.0, 0.0];
    let mut conform_r: [f32; 3] = [0.0, 0.0, 0.0]; // XYZ euler degrees
    let mut conform_flip = false; // reverse winding on export (RH→LH); toggle if faces cull inside-out
    let mut conform_live = true;
    // Workbench: render a marker at EVERY HIER node of the previewed template so functional nodes
    // (rotor hub, skids, seat, tail, hardpoints) are visible spatial anchors to map geometry onto.
    let mut show_nodes = false;
    // ★HARDPOINT EDITOR. A vehicle's INTERACTION POINTS are HIER nodes named `hp_*` — hp_seat_lt is
    // the seat you enter at, hp_fx_exhaust_* the exhaust emitters, hp_wheel_* the suspension points,
    // hp_barreltip_a the muzzle. Conform a novel model at a different SIZE and these stay where the
    // DONOR's were: on a 2x tank the seat ends up 5.5 m in the air on the turret roof and the vehicle
    // simply cannot be entered. They must be RE-PLACED on the new model, which is a spatial job — so
    // do it here, on the model, instead of guessing coordinates.
    // `hp_edits[node] = new world position`; exported as `inject_parts --node-at <node>:<x>,<y>,<z>`.
    let mut show_hardpoints = true;
    let mut hp_edits: std::collections::BTreeMap<usize, [f32; 3]> = std::collections::BTreeMap::new();
    let mut hp_selected: Option<usize> = None;
    // node_hash -> resolved name, for the loaded template (hp_* names come from the bone-name list).
    let mut hp_names: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    // The active workbench (activity rail) + the vehicle inventory the Mods navigator lists as donor
    // templates (rebuilt when names/overlays change).
    let mut wb = Workbench::Inspect;
    // Persist the user's dragged panel widths ourselves — feeding the remembered width back as
    // `default_width` each frame keeps a drag sticky even when collapsing a card would otherwise let
    // egui re-fit the panel to its (now shorter) content.
    let mut navigator_width = 300.0f32;
    let mut inspector_width = 372.0f32;
    let mut vehicle_inventory: Vec<(&'static str, Vec<(u32, String)>)> = build_vehicle_inventory(&index);
    let mut inventory_dirty = false;
    // After a retarget auto-play, many character clips (weapon/pistol variants) won't bind to the rig;
    // this counts down auto-advances to the NEXT clip until one binds, so the pedestal shows a real
    // (full-body) animation instead of stalling on a non-binding default. 0 = not seeking.
    let mut clip_seek: i32 = 0;
    // ── Skeleton workbench: retarget a Source-rigged import onto a Mercs2 HIER skeleton. ──
    // The detected source rig convention of the current import, the chosen target character skeleton
    // (name + hash), and the computed bone map (source bone → HIER bone + confidence). See retarget.rs.
    let mut retarget: Option<crate::retarget::Retarget> = None;
    let mut retarget_target: Option<(u32, String)> = None;
    // Source .glb path of the current rigged import — re-loaded at faithful-export time so the
    // exported skinning uses the RAW f32 weights + true node graph (not the preview-quantised copy).
    let mut retarget_src_path: Option<std::path::PathBuf> = None;
    // Skeleton navigator: once a target is picked the panel shows the donor/imported bone TREES; this
    // flips back to the character picker to choose a different target.
    let mut show_target_picker = false;
    let mut status = String::from("Enter loads the selected asset. Tab = edit mode. Esc quits.");

    // Orbit camera.
    let mut cam_target = Vec3::ZERO;
    let mut cam_yaw: f32 = 0.6;
    let mut cam_pitch: f32 = -0.35;
    let mut cam_dist: f32 = 8.0;
    let mut lmb = false;
    let mut shift = false;
    let mut last_cursor: Option<(f64, f64)> = None;
    let mut held: HashSet<KeyCode> = HashSet::new();

    let start = std::time::Instant::now();
    let mut last_frame = std::time::Instant::now();

    let refilter = |index: &AssetIndex, kind: Kind, filter: &str| -> Vec<usize> {
        let f = filter.to_ascii_lowercase();
        let mut v: Vec<usize> = index
            .rows(kind)
            .iter()
            .enumerate()
            .filter(|(_, r)| f.is_empty() || r.label().to_ascii_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect();
        // Models list groups by category (vehicles-by-class → characters → buildings → …), then
        // name within a group; textures stay in the plain name order `apply_names` set.
        if kind == Kind::Model {
            let rows = index.rows(kind);
            v.sort_by(|&a, &b| {
                crate::index::category_order(rows[a].category())
                    .cmp(&crate::index::category_order(rows[b].category()))
                    .then_with(|| rows[a].label().cmp(&rows[b].label()))
            });
        }
        v
    };

    event_loop
        .run(move |event, elwt| match event {
            Event::AboutToWait => scene.window.request_redraw(),
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => {
            // Feed the GUI first: when a panel captures the pointer/keyboard, the camera and the
            // app shortcuts stand down (see the `_ if gui_took_it` arm below).
            let gui_took_it = !names_pending && gui.on_event(&event);
            match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(size) => scene.resize(size),
                // Drag-drop IMPORT: .obj / .gltf / .glb dropped on the window goes straight onto
                // the preview pedestal, rendered by the engine like any game asset.
                WindowEvent::DroppedFile(path) => {
                    if names_pending {
                        return;
                    }
                    status = import_file(
                        &path, &mut w, &mut scene, &mut world, &mut imported, &mut preview,
                        &mut cam_target, &mut cam_dist, &mut retarget, &retarget_target,
                        &mut retarget_src_path, &mut wb,
                        &mut sel_bone, &placed, &index, &anim_sel, &lua_corpus,
                    );
                }
                // GUI captured this input (pointer over a panel / text into a widget): the camera
                // and app shortcuts stand down — but never leave an orbit drag stuck on.
                _ if gui_took_it => {
                    if let WindowEvent::MouseInput {
                        state: ElementState::Released,
                        button: MouseButton::Left,
                        ..
                    } = event
                    {
                        lmb = false;
                    }
                }
                WindowEvent::ModifiersChanged(m) => shift = m.state().shift_key(),
                WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                    lmb = state == ElementState::Pressed;
                }
                WindowEvent::CursorMoved { position, .. } => {
                    // Absolute-position deltas (no cursor grab): works with normal mice AND
                    // Shadow's absolute-coordinate streaming input (see memory
                    // `shadow-pc-absolute-mouse-input`).
                    if let Some((lx, ly)) = last_cursor {
                        let (dx, dy) = ((position.x - lx) as f32, (position.y - ly) as f32);
                        if lmb && tex_view.is_none() {
                            if shift {
                                // Pan the orbit target in the view plane.
                                let fwd = dir_from(cam_yaw, cam_pitch);
                                let right = Vec3::Y.cross(fwd).normalize();
                                let up = fwd.cross(right).normalize();
                                let k = cam_dist * 0.0016;
                                // Screen-right is world -right here: the renderer mirrors clip X
                                // (see Scene::render's handedness note), so flip the pan to track
                                // the cursor.
                                cam_target += right * (dx * k) - up * (dy * k);
                            } else {
                                cam_yaw += dx * 0.008;
                                cam_pitch = (cam_pitch - dy * 0.008).clamp(-1.5, 1.5);
                            }
                        }
                    }
                    last_cursor = Some((position.x, position.y));
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let lines = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                    };
                    cam_dist = (cam_dist * 0.9f32.powf(lines)).clamp(0.3, 20000.0);
                }
                WindowEvent::KeyboardInput {
                    event: KeyEvent { physical_key: PhysicalKey::Code(code), state, text, .. },
                    ..
                } => {
                    if state == ElementState::Released {
                        held.remove(&code);
                        return;
                    }
                    held.insert(code);

                    // Boot loading screen: input is Esc-to-quit only until the corpora are in.
                    if names_pending {
                        if code == KeyCode::Escape {
                            elwt.exit();
                        }
                        return;
                    }

                    // ── Texture plate view: its own small keymap. ──
                    if tex_view.is_some() {
                        match code {
                            KeyCode::Escape | KeyCode::F3 => actions.push(Act::TexClose),
                            KeyCode::BracketRight => actions.push(Act::TexNav(1)),
                            KeyCode::BracketLeft => actions.push(Act::TexNav(-1)),
                            _ => {}
                        }
                        return;
                    }

                    match code {
                        KeyCode::Escape => elwt.exit(),
                        KeyCode::Tab => {
                            list_visible = !list_visible;
                            status = if list_visible {
                                "BROWSE: type to filter, Enter loads.".into()
                            } else {
                                "EDIT: N/B select placed, WASD/QE move, R/F yaw, -/= scale, Del remove.".into()
                            };
                        }
                        // ── Browser (list visible): navigation + filter typing. ──
                        KeyCode::ArrowUp if list_visible => sel = sel.saturating_sub(1),
                        KeyCode::ArrowDown if list_visible => {
                            sel = (sel + 1).min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::PageUp if list_visible => sel = sel.saturating_sub(24),
                        KeyCode::PageDown if list_visible => {
                            sel = (sel + 24).min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::Home if list_visible => sel = 0,
                        KeyCode::End if list_visible => sel = filtered.len().saturating_sub(1),
                        KeyCode::ArrowLeft | KeyCode::ArrowRight if list_visible => {
                            kind = match kind {
                                Kind::Model => Kind::Texture,
                                Kind::Texture => Kind::Model,
                            };
                            filtered = refilter(&index, kind, &filter);
                            sel = 0;
                        }
                        KeyCode::Backspace if list_visible => {
                            filter.pop();
                            filtered = refilter(&index, kind, &filter);
                            sel = sel.min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::Enter if list_visible => actions.push(Act::LoadRow(sel)),
                        // ── Preview actions (also reachable from the inspector GUI). ──
                        KeyCode::F3 => actions.push(Act::TexOfPreview),
                        // , / . — previous / next catalog clip. Excluded from the filter charset
                        // for exactly this reason.
                        KeyCode::Comma => actions.push(Act::ClipNav(-1)),
                        KeyCode::Period => actions.push(Act::ClipNav(1)),
                        // \ — stop clip playback and return to bind pose.
                        KeyCode::Backslash => actions.push(Act::ClipStop),
                        KeyCode::BracketLeft | KeyCode::BracketRight => {
                            if let Some(p) = &mut preview {
                                if !p.draws.is_empty() {
                                    let n = p.draws.len();
                                    p.sel_group = if code == KeyCode::BracketRight {
                                        (p.sel_group + 1) % n
                                    } else {
                                        (p.sel_group + n - 1) % n
                                    };
                                }
                            }
                        }
                        KeyCode::Space => actions.push(Act::PlayPause),
                        // ── Sandbox / pipeline shortcuts: same Acts the inspector GUI queues —
                        // one implementation, in the action processor below. ──
                        KeyCode::F6 => actions.push(Act::Place),
                        KeyCode::F7 => actions.push(Act::Merge),
                        KeyCode::F8 => actions.push(Act::ClearSandbox),
                        KeyCode::F5 => actions.push(Act::SaveScene),
                        KeyCode::F9 => actions.push(Act::LoadScene),
                        KeyCode::F10 => actions.push(Act::Export),
                        KeyCode::F11 => actions.push(Act::TierNext),
                        // ── Edit mode letter controls (list hidden = letters are free). ──
                        KeyCode::KeyN | KeyCode::KeyB if !list_visible && !placed.is_empty() => {
                            let n = placed.len();
                            sel_placed = Some(match (sel_placed, code) {
                                (Some(i), KeyCode::KeyN) => (i + 1) % n,
                                (Some(i), _) => (i + n - 1) % n,
                                (None, _) => 0,
                            });
                        }
                        KeyCode::Delete if !list_visible => {
                            if let Some(i) = sel_placed {
                                let pl = placed.remove(i);
                                world.despawn(pl.entity).ok();
                                scene.forget_entity(pl.entity);
                                sel_placed = (!placed.is_empty()).then(|| i.min(placed.len() - 1));
                            }
                        }
                        KeyCode::KeyG if !list_visible => {
                            if let Some(i) = sel_placed {
                                placed[i].pos = cam_target;
                            }
                        }
                        _ => {
                            // Remaining printable keys type into the filter (browse mode only).
                            if list_visible {
                                if let Some(t) = text {
                                    let mut changed = false;
                                    for c in t.chars() {
                                        // ',' '.' '\\' are anim keys, NOT filter chars.
                                        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                                            filter.push(c.to_ascii_lowercase());
                                            changed = true;
                                        }
                                    }
                                    if changed {
                                        filtered = refilter(&index, kind, &filter);
                                        sel = 0;
                                    }
                                }
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let t = start.elapsed().as_secs_f32();
                    let dt = last_frame.elapsed().as_secs_f32().min(0.05);
                    last_frame = std::time::Instant::now();

                    // ── Boot loading screen: engine loading path (shell plate + spinner + bar)
                    // until the name corpora are in, then fall through into the browser. ──
                    if names_pending {
                        loop {
                            match nrx.try_recv() {
                                Ok(Boot::Prog(f, stage)) => {
                                    boot_target = f;
                                    boot_stage = stage;
                                }
                                Ok(Boot::Done(names, lua, secs)) => {
                                    index.apply_names(names);
                                    lua_corpus = lua;
                                    filtered = refilter(&index, kind, &filter);
                                    inventory_dirty = true; // names now resolve → build the vehicle inventory
                                    sel = sel.min(filtered.len().saturating_sub(1));
                                    names_pending = false;
                                    // Swap the boot skull out for the shell plate (browse backdrop).
                                    if let Some(td) = &shell_plate {
                                        scene.set_loading_art(td);
                                    }
                                    status = format!(
                                        "{} names loaded ({secs:.1}s) — {} models, {} textures, {} lua scripts",
                                        index.names.len(),
                                        index.models.len(),
                                        index.textures.len(),
                                        lua_corpus.len()
                                    );
                                }
                                Err(_) => break,
                            }
                        }
                        if names_pending {
                            boot_shown += (boot_target - boot_shown) * (1.0 - (-3.0 * dt).exp());
                            let (wpx, hpx) = (scene.size.width as f32, scene.size.height as f32);
                            // Retail layout: green tip line centred at ~2/3 height; bold orange
                            // "Loading" at the lower right (gently breathing like the game's).
                            const TIP_GREEN: [f32; 4] = [0.76, 0.80, 0.44, 1.0];
                            const LOAD_ORANGE: [f32; 4] = [1.0, 0.62, 0.12, 1.0];
                            let stage = format!("loading {boot_stage}");
                            let sw = stage.len() as f32 * 8.0 * UI_S;
                            text_sh(&mut scene, (wpx - sw) * 0.5, hpx * 0.66, UI_S, TIP_GREEN, &stage);
                            let ls = UI_S * 1.5; // 24px — the retail "Loading" is large + bold
                            let mut oc = LOAD_ORANGE;
                            oc[3] = 0.75 + 0.25 * (t * 2.0).sin();
                            let lw = "Loading".len() as f32 * 8.0 * ls;
                            text_sh(&mut scene, wpx - lw - 56.0, hpx * 0.84, ls, oc, "Loading");
                            match scene.render_boot(t, boot_shown.min(0.98)) {
                                Ok(())
                                | Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {}
                                Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                                Err(e) => eprintln!("surface error: {e:?}"),
                            }
                            return;
                        }
                    }

                    // ── Background clip loads: drain finished decodes into the current
                    // preview's cache (stale previews' results are dropped). ──
                    while let Ok(done) = clip_loader.rx.try_recv() {
                        clip_loader.inflight.remove(&(done.preview, done.hash));
                        if let Some(p) = &mut preview {
                            if p.hash == done.preview {
                                let bound = done.clip.is_some();
                                // A "good" auto-play clip is a NORMAL full-body one: it binds and has a
                                // sane transform-track count. The ~105-track clip is the engine's SPECIAL
                                // reference pose (game_world excludes it) and contorts the skeleton; very
                                // low counts are partial/weapon poses. Auto-seek skips both.
                                let tracks = done.clip.as_ref().map(|c| c.num_transform_tracks).unwrap_or(0);
                                let good = bound && (20..=90).contains(&tracks);
                                let is_cur = p.cur_clip.map(|ci| p.clip_catalog[ci].hash) == Some(done.hash);
                                p.clip_cache.insert(done.hash, done.clip);
                                if good && is_cur {
                                    clip_seek = 0; // found a real full-body clip — stop seeking
                                } else if !good && is_cur && clip_seek > 0 && !p.clip_catalog.is_empty() {
                                    // Auto-advance past non-binding / special / partial clips.
                                    clip_seek -= 1;
                                    let n = p.clip_catalog.len();
                                    let next = (p.cur_clip.unwrap_or(0) + 1) % n;
                                    p.cur_clip = Some(next);
                                    p.anim_time = 0.0;
                                    let h = p.clip_catalog[next].hash;
                                    clip_loader.request(p.hash, &p.hier, h);
                                } else if !bound && is_cur {
                                    let label = p
                                        .clip_catalog
                                        .iter()
                                        .find(|e| e.hash == done.hash)
                                        .map(|e| e.label.clone())
                                        .unwrap_or_else(|| format!("0x{:08X}", done.hash));
                                    status = format!(
                                        "clip {label} is not in any animgroup bound to this rig"
                                    );
                                }
                            }
                        }
                    }

                    // ── Background publish: collect the report, print the per-asset self-test,
                    // and load the written wad as an overlay so the new hashes appear in the
                    // browser immediately (the publish self-test loop). ──
                    let mut publish_msg = None;
                    if let Some(p) = &publisher {
                        match p.rx.try_recv() {
                            Ok(m) => publish_msg = Some(m),
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                publish_msg = Some(Err("publish worker died".into()))
                            }
                        }
                    }
                    if let Some(msg) = publish_msg {
                        publisher = None;
                        match msg {
                            Ok(report) => {
                                let ok = report.results.iter().filter(|(_, r)| r.is_ok()).count();
                                for (name, r) in &report.results {
                                    match r {
                                        Ok(s) => eprintln!("[publish] self-test {name}: {s}"),
                                        Err(e) => eprintln!("[publish] self-test {name}: FAIL {e}"),
                                    }
                                }
                                status = format!(
                                    "published {} ({} bytes, sha256 {}…) — self-test {}/{} OK",
                                    report.path.display(),
                                    report.bytes,
                                    &report.sha256[..16],
                                    ok,
                                    report.results.len()
                                );
                                let path_str = report.path.to_string_lossy().into_owned();
                                match wad::open(&path_str) {
                                    Ok(nw) => {
                                        w.wads.push(nw);
                                        w.labels.push(path_str);
                                        let names = std::mem::take(&mut index.names);
                                        index = AssetIndex::build(&w.wads, names);
                                        filtered = refilter(&index, kind, &filter);
                                        inventory_dirty = true;
                                        sel = sel.min(filtered.len().saturating_sub(1));
                                    }
                                    Err(e) => {
                                        status.push_str(&format!(" (overlay reload failed: {e})"))
                                    }
                                }
                            }
                            Err(e) => status = format!("PUBLISH FAILED: {e}"),
                        }
                    }

                    // ── Background F10 export: collect the finished bundle path (or error) and
                    // clear the in-flight handle so the progress window closes. ──
                    let mut export_msg = None;
                    if let Some(e) = &exporter {
                        match e.rx.try_recv() {
                            Ok(m) => export_msg = Some(m),
                            Err(std::sync::mpsc::TryRecvError::Empty) => {}
                            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                export_msg = Some(Err("export worker died".into()))
                            }
                        }
                    }
                    if let Some(msg) = export_msg {
                        let lbl = exporter.take().map(|e| e.label).unwrap_or_default();
                        status = match msg {
                            Ok(dir) => format!("exported {lbl} -> {dir}"),
                            Err(e) => format!("EXPORT FAILED: {e}"),
                        };
                    }

                    // ── Edit-mode held-key nudging of the selected placed instance. ──
                    if !list_visible {
                        if let Some(i) = sel_placed {
                            let pl = &mut placed[i];
                            let step = (cam_dist * 0.35 * dt).max(0.002);
                            if held.contains(&KeyCode::KeyW) { pl.pos.z += step; }
                            if held.contains(&KeyCode::KeyS) { pl.pos.z -= step; }
                            if held.contains(&KeyCode::KeyA) { pl.pos.x -= step; }
                            if held.contains(&KeyCode::KeyD) { pl.pos.x += step; }
                            if held.contains(&KeyCode::KeyQ) { pl.pos.y -= step; }
                            if held.contains(&KeyCode::KeyE) { pl.pos.y += step; }
                            if held.contains(&KeyCode::KeyR) { pl.yaw += 1.2 * dt; }
                            if held.contains(&KeyCode::KeyF) { pl.yaw -= 1.2 * dt; }
                            if held.contains(&KeyCode::Equal) { pl.scale *= 1.0 + 0.8 * dt; }
                            if held.contains(&KeyCode::Minus) { pl.scale /= 1.0 + 0.8 * dt; }
                            let _ = world.insert_one(
                                pl.entity,
                                Transform {
                                    translation: pl.pos,
                                    rotation: Quat::from_rotation_y(pl.yaw),
                                    scale: Vec3::splat(pl.scale),
                                },
                            );
                        }
                    }

                    // ── Conform live preview: drive the imported pedestal entity from the conform
                    // panel's scale/pos/rot so its placement against the donor reference is visible
                    // in the viewport (the same transform is baked into the export). ──
                    if conform_live {
                        if let Some(p) = &preview {
                            if imported.contains_key(&p.hash) {
                                let _ = world.insert_one(
                                    p.entity,
                                    Transform {
                                        translation: Vec3::from(conform_t),
                                        rotation: conform_quat(conform_r),
                                        scale: Vec3::splat(conform_scale),
                                    },
                                );
                            }
                        }
                    }

                    // ── Animation: sample the preview's active clip into its palette (paused =
                    // resample the held time, so scrubbing/pausing still shows the exact pose). ──
                    if let Some(p) = &mut preview {
                        if let Some(ci) = p.cur_clip {
                            let hash = p.clip_catalog[ci].hash;
                            if let Some(Some(ca)) = p.clip_cache.get(&hash) {
                                let dur = ca.clip.duration.max(1e-3);
                                if p.playing {
                                    p.anim_time = (p.anim_time + dt) % dur;
                                }
                                let sample = ca.clip.sample_local(p.anim_time);
                                let mats = match &p.retarget_source {
                                    // Retargeted import: drive its OWN (undeformed) skeleton with the
                                    // source skeleton's per-bone world rotation deltas.
                                    Some((source_rig, target_to_source)) => {
                                        pose::havok_palette_retarget_cross(
                                            &p.rig,
                                            source_rig,
                                            target_to_source,
                                            &sample,
                                            &ca.track_to_hier,
                                            ca.num_transform_tracks,
                                        )
                                    }
                                    None => pose::havok_palette_in_place(
                                        &p.rig,
                                        &sample,
                                        &ca.track_to_hier,
                                        ca.num_transform_tracks,
                                    ),
                                };
                                let _ = world.insert_one(p.entity, SkinPalette { mats });
                            }
                        }
                    }

                    // ── The inspector GUI: toolbar, browser, Details panel, texture window.
                    // Widgets queue `Act`s; the processor below executes them. ──
                    let mut hovered_bone: Option<usize> = None;
                    // Skeleton workbench hover: (bone index, is_source_tree). Resolved to a viewer
                    // highlight in the PREVIEW's current space (source before Apply, target after).
                    let mut hover_skel: Option<(usize, bool)> = None;
                    if inventory_dirty {
                        vehicle_inventory = build_vehicle_inventory(&index);
                        inventory_dirty = false;
                    }
                    use crate::gui::theme;
                    gui.run(|ctx| {
                        // ── COMMAND BAR: identity + breadcrumb (left) · scene I/O (right). Only
                        // GLOBAL actions live here; contextual verbs are on the viewport verb-bar. ──
                        egui::TopBottomPanel::top("cmdbar")
                            .exact_height(48.0)
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin { left: 14.0, right: 14.0, top: 0.0, bottom: 0.0 }),
                            )
                            .show(ctx, |ui| {
                            ui.horizontal_centered(|ui| {
                                theme::brand_mark(ui);
                                ui.add_space(6.0);
                                ui.label(theme::disp_text("MERCS 2", 16.0, theme::TX));
                                ui.label(theme::disp_text("WORKSHOP", 9.5, theme::FAINT));
                                ui.separator();
                                ui.label(theme::disp_text(wb.verb().to_uppercase(), 10.0, theme::FAINT));
                                if let Some(p) = &preview {
                                    ui.label(egui::RichText::new(&p.label).strong());
                                    ui.label(
                                        egui::RichText::new(format!("0x{:08X}", p.hash))
                                            .monospace()
                                            .color(theme::BRASS_DK),
                                    );
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Load").on_hover_text("Load a saved arrangement").clicked() {
                                            actions.push(Act::LoadScene);
                                        }
                                        if ui.button("Save").on_hover_text("Save the sandbox arrangement").clicked() {
                                            actions.push(Act::SaveScene);
                                        }
                                        theme::eyebrow(ui, "Scene");
                                        if names_pending {
                                            ui.separator();
                                            ui.add(egui::Spinner::new().size(13.0));
                                        }
                                    },
                                );
                            });
                        });
                        // ── Background-export modal: a centered "Exporting…" card while the worker
                        // decodes the clip set. Request a repaint each frame so the poll above keeps
                        // running (and the spinner animates) even without pointer input. ──
                        if let Some(e) = &exporter {
                            ctx.request_repaint();
                            egui::Window::new("Exporting")
                                .collapsible(false)
                                .resizable(false)
                                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                                .show(ctx, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Spinner::new().size(16.0));
                                        ui.add_space(6.0);
                                        ui.label(format!("Exporting {} — decoding animation set…", e.label));
                                    });
                                    ui.add_space(2.0);
                                    ui.label(
                                        egui::RichText::new("Depending on the number of animations, this might take several minutes")
                                            .weak(),
                                    );
                                });
                        }
                        // ── ACTIVITY RAIL: pick the workbench. The viewport/camera never reset when
                        // this changes — only the navigator + inspector reconfigure. ──
                        egui::SidePanel::left("rail")
                            .exact_width(66.0)
                            .resizable(false)
                            .frame(egui::Frame::none().fill(theme::G0))
                            .show(ctx, |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.add_space(6.0);
                                let icons = [
                                    theme::RailIcon::Inspect,
                                    theme::RailIcon::Sandbox,
                                    theme::RailIcon::Mods,
                                    theme::RailIcon::Skeleton,
                                ];
                                for (i, w) in Workbench::ALL.iter().enumerate() {
                                    if theme::rail_item(ui, Some(i + 1), w.label(), icons[i], wb == *w) {
                                        wb = *w;
                                    }
                                }
                                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                                    ui.add_space(6.0);
                                    theme::rail_item(ui, None, "Log", theme::RailIcon::Log, false);
                                });
                            });
                        egui::TopBottomPanel::bottom("status")
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin::symmetric(14.0, 4.0)),
                            )
                            .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if names_pending {
                                    theme::status_dot(ui, "Loading", theme::BRASS);
                                    ui.add(egui::Spinner::new().size(11.0));
                                } else {
                                    theme::status_dot(ui, "Ready", theme::GOOD);
                                }
                                ui.separator();
                                ui.label(egui::RichText::new(status.as_str()).color(theme::DIM).size(11.0));
                                // Bottom-right: background work in flight (clip decodes,
                                // mod publishing).
                                let n = clip_loader.inflight.len();
                                let publishing = publisher.is_some();
                                if n > 0 || publishing {
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            let mut parts: Vec<String> = Vec::new();
                                            if publishing {
                                                parts.push("publishing mod wad…".into());
                                            }
                                            if n == 1 {
                                                parts.push("loading clip…".into());
                                            } else if n > 1 {
                                                parts.push(format!("loading {n} clips…"));
                                            }
                                            ui.weak(parts.join(" · "));
                                            ui.spinner();
                                        },
                                    );
                                }
                            });
                        });
                        // ── VERB-BAR: the current workbench's primary actions, over the viewport.
                        // Contextual verbs live HERE (not the command bar); brass = go, hazard = the
                        // irreversible. Every verb shows its keyboard shortcut. ──
                        egui::TopBottomPanel::bottom("verbbar")
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin::symmetric(13.0, 5.0)),
                            )
                            .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                let has_preview = preview.is_some();
                                let has_placed = !placed.is_empty();
                                match wb {
                                    Workbench::Inspect => {
                                        if theme::primary_button(ui, "+ Place  F6", has_preview).clicked() {
                                            actions.push(Act::Place);
                                        }
                                        if ui.add_enabled(has_preview && exporter.is_none(), egui::Button::new("Export  F10")).clicked() {
                                            actions.push(Act::Export);
                                        }
                                        ui.separator();
                                        if ui.add_enabled(has_preview, egui::Button::new("View textures  F3")).clicked() {
                                            actions.push(Act::TexOfPreview);
                                        }
                                        if ui.add_enabled(has_preview, egui::Button::new("Next clip  F4")).clicked() {
                                            actions.push(Act::ClipNav(1));
                                        }
                                    }
                                    Workbench::Sandbox => {
                                        let has_sel = sel_placed.is_some_and(|i| i < placed.len());
                                        if theme::primary_button(ui, "+ Place  F6", has_preview).clicked() {
                                            actions.push(Act::Place);
                                        }
                                        if ui.add_enabled(has_sel, egui::Button::new("Duplicate")).clicked() {
                                            actions.push(Act::DuplicatePlaced(sel_placed.unwrap()));
                                        }
                                        if ui.add_enabled(has_sel, egui::Button::new("Delete")).clicked() {
                                            actions.push(Act::RemovePlaced(sel_placed.unwrap()));
                                            sel_placed = None;
                                        }
                                        ui.separator();
                                        if ui.add_enabled(has_placed, egui::Button::new("Merge  F7")).clicked() {
                                            actions.push(Act::Merge);
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if theme::danger_button(ui, "Clear", has_placed).clicked() {
                                                actions.push(Act::ClearSandbox);
                                            }
                                        });
                                    }
                                    Workbench::Mods => {
                                        if ui.button("Load donor ref").clicked() {
                                            actions.push(Act::LoadDonorRef);
                                        }
                                        if ui.add_enabled(has_preview, egui::Button::new("Auto-fit")).clicked() {
                                            actions.push(Act::ConformAutofit);
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let busy = publisher.is_some();
                                            let lbl = if busy { "publishing…" } else { "Publish patch WAD" };
                                            if theme::danger_button(ui, lbl, !mod_items.is_empty() && !busy).clicked() {
                                                actions.push(Act::Publish);
                                            }
                                        });
                                    }
                                    Workbench::Skeleton => {
                                        let ready = retarget.as_ref().is_some_and(|r| r.mapped_count() > 0)
                                            && retarget_target.is_some();
                                        if ui.add_enabled(retarget.is_some(), egui::Button::new("Auto-map bones")).clicked() {
                                            actions.push(Act::RetargetRemap);
                                        }
                                        if ui
                                            .add_enabled(preview.is_some(), egui::Button::new("Clear import"))
                                            .on_hover_text("Unload the current import and reset the retarget — start over with a fresh model")
                                            .clicked()
                                        {
                                            actions.push(Act::ClearImport);
                                        }
                                        let can_align = retarget.as_ref().is_some_and(|r| {
                                            !r.source_pos.is_empty() && !r.target_pos.is_empty()
                                        });
                                        if ui
                                            .add_enabled(can_align, egui::Button::new("Align by position"))
                                            .on_hover_text("Fill the still-unmapped bones by nearest target bone in 3D space")
                                            .clicked()
                                        {
                                            actions.push(Act::RetargetAlignPos);
                                        }
                                        let can_export = ready && retarget_src_path.is_some();
                                        if ui
                                            .add_enabled(can_export, egui::Button::new("Export faithful character"))
                                            .on_hover_text("Re-pose onto the target skeleton with shipped-format skinning (palette-relative BLENDINDICES + INFO(56) range table) and inject into the target donor block")
                                            .clicked()
                                        {
                                            actions.push(Act::ExportFaithfulCharacter);
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if theme::primary_button(ui, "Apply retarget", ready).clicked() {
                                                actions.push(Act::RetargetApply);
                                            }
                                        });
                                    }
                                }
                            });
                        });
                        // ── NAVIGATOR (left): the current workbench's list. The viewport keeps its
                        // camera when the workbench changes; only this + the inspector reconfigure. ──
                        let navigator_resp = egui::SidePanel::left("navigator")
                            .resizable(true)
                            .default_width(navigator_width)
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin { left: 13.0, right: 10.0, top: 12.0, bottom: 0.0 }),
                            )
                            .show(ctx, |ui| {
                          match wb {
                            Workbench::Inspect => {
                            let before = (kind, filter.clone());
                            ui.label(theme::disp_text("ASSETS", 15.0, theme::TX));
                            ui.add_space(8.0);
                            // Segmented Models / Textures toggle — two EQUAL-width segments, each a
                            // centred "LABEL  count" unit, the active one filled. Custom-drawn so the
                            // segments are exactly half the pill and the text is truly centred.
                            egui::Frame::none()
                                .fill(theme::G0)
                                .stroke(egui::Stroke::new(1.0, theme::LINE))
                                .rounding(egui::Rounding::same(7.0))
                                .inner_margin(egui::Margin::same(3.0))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.horizontal(|ui| {
                                        let seg_w = (ui.available_width() - 3.0) / 2.0;
                                        for (k, name, count) in [
                                            (Kind::Model, "Models", index.models.len()),
                                            (Kind::Texture, "Textures", index.textures.len()),
                                        ] {
                                            let on = kind == k;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                egui::vec2(seg_w, 26.0),
                                                egui::Sense::click(),
                                            );
                                            let p = ui.painter();
                                            let fill = if on {
                                                theme::G3
                                            } else if resp.hovered() {
                                                theme::G2
                                            } else {
                                                egui::Color32::TRANSPARENT
                                            };
                                            p.rect_filled(rect, egui::Rounding::same(5.0), fill);
                                            let name_col = if on { theme::TX } else { theme::DIM };
                                            let cnt_col = if on { theme::BRASS } else { theme::FAINT };
                                            let g_name = p.layout_no_wrap(
                                                name.to_uppercase(),
                                                egui::FontId::new(11.0, theme::disp()),
                                                name_col,
                                            );
                                            let g_cnt = p.layout_no_wrap(
                                                commafy(count),
                                                egui::FontId::monospace(9.5),
                                                cnt_col,
                                            );
                                            let (nsz, csz) = (g_name.size(), g_cnt.size());
                                            let gap = 6.0;
                                            let x0 = rect.center().x - (nsz.x + gap + csz.x) / 2.0;
                                            let cy = rect.center().y;
                                            p.galley(egui::pos2(x0, cy - nsz.y / 2.0), g_name, name_col);
                                            p.galley(
                                                egui::pos2(x0 + nsz.x + gap, cy - csz.y / 2.0),
                                                g_cnt,
                                                cnt_col,
                                            );
                                            if resp.clicked() {
                                                kind = k;
                                            }
                                        }
                                    });
                                });
                            ui.add_space(7.0);
                            // Filter with a search glyph.
                            egui::Frame::none()
                                .fill(theme::G0)
                                .stroke(egui::Stroke::new(1.0, theme::LINE))
                                .rounding(egui::Rounding::same(6.0))
                                .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::vec2(13.0, 13.0),
                                            egui::Sense::hover(),
                                        );
                                        let c = r.center();
                                        ui.painter().circle_stroke(
                                            c + egui::vec2(-1.5, -1.5),
                                            4.0,
                                            egui::Stroke::new(1.4, theme::FAINT),
                                        );
                                        ui.painter().line_segment(
                                            [c + egui::vec2(1.8, 1.8), c + egui::vec2(5.0, 5.0)],
                                            egui::Stroke::new(1.4, theme::FAINT),
                                        );
                                        ui.add(
                                            egui::TextEdit::singleline(&mut filter)
                                                .hint_text("filter…")
                                                .frame(false)
                                                .desired_width(f32::INFINITY),
                                        );
                                    });
                                });
                            if before.0 != kind || before.1 != filter {
                                filtered = refilter(&index, kind, &filter);
                                sel = 0;
                            }
                            ui.add_space(6.0);
                            // Grouped display: category headers interleaved with rows, all one line
                            // high so `show_rows` still virtualizes the (3000+ row) models list.
                            // `filtered` is category-sorted; a header opens each new run. `Disp::Row`
                            // carries the position in `filtered` so selection/keyboard stay unchanged.
                            enum Disp {
                                Header(&'static str, usize),
                                Row(usize),
                            }
                            let mut display: Vec<Disp> = Vec::with_capacity(filtered.len() + 32);
                            if kind == Kind::Model {
                                let rows = index.rows(kind);
                                let mut counts: HashMap<&'static str, usize> = HashMap::new();
                                for &ri in &filtered {
                                    *counts.entry(rows[ri].category()).or_default() += 1;
                                }
                                let mut last = "";
                                for (vi, &ri) in filtered.iter().enumerate() {
                                    let cat = rows[ri].category();
                                    if cat != last {
                                        display.push(Disp::Header(cat, counts[cat]));
                                        last = cat;
                                    }
                                    // A collapsed category keeps its header but drops its rows.
                                    if !collapsed_cats.contains(cat) {
                                        display.push(Disp::Row(vi));
                                    }
                                }
                            } else {
                                for vi in 0..filtered.len() {
                                    display.push(Disp::Row(vi));
                                }
                            }
                            let row_h = 19.0_f32.max(ui.text_style_height(&egui::TextStyle::Body));
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
                                ui,
                                row_h,
                                display.len(),
                                |ui, range| {
                                    for di in range {
                                        let vi = match display[di] {
                                            Disp::Header(cat, n) => {
                                                let is_collapsed = collapsed_cats.contains(cat);
                                                // Lay the header out, filling the row so the WHOLE
                                                // line is the click target, then sense a click on its
                                                // rect to toggle the fold.
                                                let ir = ui.horizontal(|ui| {
                                                    let (r, _) = ui.allocate_exact_size(
                                                        egui::vec2(7.0, 7.0),
                                                        egui::Sense::hover(),
                                                    );
                                                    let c = r.center();
                                                    // ▶ when collapsed, ▼ when expanded.
                                                    let tri = if is_collapsed {
                                                        vec![
                                                            c + egui::vec2(-2.5, -3.5),
                                                            c + egui::vec2(3.0, 0.0),
                                                            c + egui::vec2(-2.5, 3.5),
                                                        ]
                                                    } else {
                                                        vec![
                                                            c + egui::vec2(-3.5, -2.5),
                                                            c + egui::vec2(3.5, -2.5),
                                                            c + egui::vec2(0.0, 3.0),
                                                        ]
                                                    };
                                                    ui.painter().add(egui::Shape::convex_polygon(
                                                        tri,
                                                        theme::BRASS_DK,
                                                        egui::Stroke::NONE,
                                                    ));
                                                    ui.label(theme::disp_text(
                                                        cat.to_uppercase(),
                                                        10.0,
                                                        theme::FAINT,
                                                    ));
                                                    ui.label(
                                                        egui::RichText::new(n.to_string())
                                                            .monospace()
                                                            .size(9.5)
                                                            .color(theme::FAINT),
                                                    );
                                                    // Claim the rest of the row so the click target
                                                    // (and any hover fill) spans the full width.
                                                    ui.allocate_space(egui::vec2(
                                                        ui.available_width(),
                                                        1.0,
                                                    ));
                                                });
                                                let resp = ui
                                                    .interact(
                                                        ir.response.rect,
                                                        egui::Id::new(("asset_cat_hdr", cat)),
                                                        egui::Sense::click(),
                                                    )
                                                    .on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    );
                                                if resp.clicked() {
                                                    if is_collapsed {
                                                        collapsed_cats.remove(cat);
                                                    } else {
                                                        collapsed_cats.insert(cat);
                                                    }
                                                }
                                                continue;
                                            }
                                            Disp::Row(vi) => vi,
                                        };
                                        let r = &index.rows(kind)[filtered[vi]];
                                        let (hash, label) = (r.hash, r.label());
                                        let cat = if kind == Kind::Model { r.category() } else { "" };
                                        let mark = if r.src > 0 { "+ " } else { "" };
                                        let row = ui
                                            .horizontal(|ui| {
                                                if kind == Kind::Model {
                                                    ui.add_space(2.0);
                                                    row_icon(ui, cat, 15.0);
                                                    ui.add_space(5.0);
                                                }
                                                ui.selectable_label(
                                                    vi == sel,
                                                    format!("{mark}{label}"),
                                                )
                                            })
                                            .inner;
                                        if row.clicked() {
                                            sel = vi;
                                            actions.push(Act::LoadRow(vi));
                                        }
                                        row.context_menu(|ui| {
                                            match kind {
                                                Kind::Model => {
                                                    if ui.button("Load / preview").clicked() {
                                                        sel = vi;
                                                        actions.push(Act::LoadRow(vi));
                                                        ui.close_menu();
                                                    }
                                                    if ui.button("Place in sandbox").clicked() {
                                                        actions.push(Act::PlaceHash(
                                                            hash,
                                                            label.clone(),
                                                        ));
                                                        ui.close_menu();
                                                    }
                                                    if ui.button("Export (OBJ + textures)").clicked()
                                                    {
                                                        actions.push(Act::ExportHash(
                                                            hash,
                                                            label.clone(),
                                                        ));
                                                        ui.close_menu();
                                                    }
                                                }
                                                Kind::Texture => {
                                                    if ui.button("View plate").clicked() {
                                                        sel = vi;
                                                        actions.push(Act::LoadRow(vi));
                                                        ui.close_menu();
                                                    }
                                                }
                                            }
                                            ui.separator();
                                            if ui.button("Copy name").clicked() {
                                                ui.ctx().copy_text(label.clone());
                                                ui.close_menu();
                                            }
                                            if ui.button(format!("Copy hash 0x{hash:08X}")).clicked()
                                            {
                                                ui.ctx().copy_text(format!("0x{hash:08X}"));
                                                ui.close_menu();
                                            }
                                        });
                                    }
                                },
                            );
                            } // ── end Inspect navigator (asset browser) ──
                            Workbench::Sandbox => {
                                // Scene OUTLINER: the placed instances, plus a place-from-library
                                // search — the Sandbox is about arranging many objects, so this is
                                // its own list, not the asset browser.
                                ui.label(theme::disp_text("SCENE", 15.0, theme::TX));
                                ui.weak("Objects placed in this scene. Select to transform.");
                                ui.add_space(8.0);
                                egui::Frame::none()
                                    .fill(theme::G0)
                                    .stroke(egui::Stroke::new(1.0, theme::LINE))
                                    .rounding(egui::Rounding::same(6.0))
                                    .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let (r, _) = ui.allocate_exact_size(egui::vec2(13.0, 13.0), egui::Sense::hover());
                                            let c = r.center();
                                            let plus_col = if filter.is_empty() { theme::FAINT } else { theme::BRASS_DK };
                                            let s = egui::Stroke::new(1.6, plus_col);
                                            ui.painter().line_segment([c + egui::vec2(-4.0, 0.0), c + egui::vec2(4.0, 0.0)], s);
                                            ui.painter().line_segment([c + egui::vec2(0.0, -4.0), c + egui::vec2(0.0, 4.0)], s);
                                            ui.add(
                                                egui::TextEdit::singleline(&mut filter)
                                                    .hint_text("place from library…")
                                                    .frame(false)
                                                    .desired_width(f32::INFINITY),
                                            );
                                        });
                                    });
                                let f = filter.to_ascii_lowercase();
                                if !f.is_empty() {
                                    ui.add_space(4.0);
                                    egui::Frame::none()
                                        .fill(theme::G0)
                                        .stroke(egui::Stroke::new(1.0, theme::LINE))
                                        .rounding(egui::Rounding::same(6.0))
                                        .inner_margin(egui::Margin::same(4.0))
                                        .show(ui, |ui| {
                                            let mut shown = 0usize;
                                            for r in index.models.iter() {
                                                if shown >= 10 {
                                                    ui.weak("…refine the search");
                                                    break;
                                                }
                                                let label = r.label();
                                                if !label.to_ascii_lowercase().contains(&f) {
                                                    continue;
                                                }
                                                shown += 1;
                                                let clicked = ui
                                                    .horizontal(|ui| {
                                                        row_icon(ui, r.category(), 14.0);
                                                        ui.add_space(4.0);
                                                        ui.selectable_label(false, label.clone()).clicked()
                                                    })
                                                    .inner;
                                                if clicked {
                                                    actions.push(Act::PlaceHash(r.hash, label.clone()));
                                                }
                                            }
                                            if shown == 0 {
                                                ui.weak("no model matches");
                                            }
                                        });
                                }
                                ui.add_space(6.0);
                                theme::eyebrow(ui, &format!("Objects · {}", placed.len()));
                                ui.add_space(5.0);
                                if placed.is_empty() {
                                    ui.weak("Nothing placed yet — search above to add one.");
                                }
                                for i in 0..placed.len() {
                                    let (phash, plabel) = (placed[i].hash, placed[i].label.clone());
                                    let cat = index
                                        .models
                                        .iter()
                                        .find(|r| r.hash == phash)
                                        .map(|r| r.category())
                                        .unwrap_or("Other");
                                    let sel = sel_placed == Some(i);
                                    let row = ui
                                        .horizontal(|ui| {
                                            row_icon(ui, cat, 14.0);
                                            ui.add_space(4.0);
                                            let r = ui.selectable_label(
                                                sel,
                                                egui::RichText::new(&plabel).size(12.5),
                                            );
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(egui::RichText::new(format!("n{i}")).monospace().size(9.5).color(theme::FAINT));
                                                },
                                            );
                                            r
                                        })
                                        .inner;
                                    if row.clicked() {
                                        sel_placed = Some(i);
                                    }
                                    row.context_menu(|ui| {
                                        if ui.button("Snap to camera target").clicked() {
                                            actions.push(Act::SnapPlaced(i));
                                            ui.close_menu();
                                        }
                                        if ui.button("Duplicate").clicked() {
                                            actions.push(Act::DuplicatePlaced(i));
                                            ui.close_menu();
                                        }
                                        if ui.button("Remove").clicked() {
                                            actions.push(Act::RemovePlaced(i));
                                            ui.close_menu();
                                        }
                                        ui.separator();
                                        if ui.button(format!("Copy hash 0x{phash:08X}")).clicked() {
                                            ui.ctx().copy_text(format!("0x{phash:08X}"));
                                            ui.close_menu();
                                        }
                                    });
                                }
                            }
                            Workbench::Mods => {
                                ui.label(theme::disp_text("Donor templates", 15.0, theme::TX));
                                ui.weak("Pick a vehicle template — it becomes the conform donor its \
                                         container hosts the injected geometry.");
                                ui.separator();
                                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                    if vehicle_inventory.is_empty() {
                                        ui.weak("(loading catalog…)");
                                    }
                                    for (class, rows) in &vehicle_inventory {
                                        egui::CollapsingHeader::new(theme::disp_text(
                                            format!("{class}  ({})", rows.len()),
                                            13.0,
                                            theme::DIM,
                                        ))
                                        .default_open(*class == "helicopter")
                                        .show(ui, |ui| {
                                            for (hash, label) in rows {
                                                let on = preview.as_ref().is_some_and(|p| p.hash == *hash);
                                                if ui.selectable_label(on, label).clicked() {
                                                    actions.push(Act::LoadModelHash(*hash, label.clone()));
                                                }
                                            }
                                        });
                                    }
                                });
                            }
                            Workbench::Skeleton => {
                                let have_target = retarget_target.is_some();
                                // TWIN-TREE view once a target skeleton is chosen: donor (Mercs2) on top,
                                // imported (foreign) on the bottom. Hovering any bone highlights it (and the
                                // mesh it drives) in the viewer. The picker returns via "change target".
                                if have_target && !show_target_picker {
                                    if let (Some(r), Some((_, tl))) = (&retarget, &retarget_target) {
                                        ui.horizontal(|ui| {
                                            ui.label(theme::disp_text("Skeletons", 15.0, theme::TX));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if ui.small_button("change target").clicked() {
                                                    show_target_picker = true;
                                                }
                                            });
                                        });
                                        ui.weak(format!(
                                            "hover a bone to locate it · {} / {} mapped",
                                            r.mapped_count(),
                                            r.source_bones.len()
                                        ));
                                        ui.separator();
                                        // Split the height between the two trees.
                                        let half = (ui.available_height() - 30.0) * 0.5;
                                        theme::eyebrow(ui, &format!("Donor \u{2014} {tl}"));
                                        egui::ScrollArea::vertical().id_source("donor_tree").max_height(half).auto_shrink([false, false]).show(ui, |ui| {
                                            if let Some(h) = bone_tree(ui, &r.target_bones, &r.target_parents, "donor") {
                                                hover_skel = Some((h, false)); // false = donor/target tree
                                            }
                                        });
                                        ui.separator();
                                        theme::eyebrow(ui, &format!("Imported \u{2014} {}", r.convention.label()));
                                        egui::ScrollArea::vertical().id_source("import_tree").auto_shrink([false, false]).show(ui, |ui| {
                                            if let Some(h) = bone_tree(ui, &r.source_bones, &r.source_parents, "imported") {
                                                hover_skel = Some((h, true)); // true = imported/source tree
                                            }
                                        });
                                    }
                                } else {
                                    ui.label(theme::disp_text("Skeleton source", 15.0, theme::TX));
                                    ui.separator();
                                    match &retarget {
                                        Some(r) => {
                                            ui.horizontal(|ui| {
                                                ui.label("source:");
                                                ui.colored_label(theme::BRASS, r.convention.label());
                                            });
                                            ui.weak(format!("{} source bones", r.source_bones.len()));
                                        }
                                        None => {
                                            ui.weak("no rigged import loaded");
                                        }
                                    }
                                    ui.separator();
                                    theme::eyebrow(ui, "Target skeleton");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut filter)
                                            .hint_text("filter characters…")
                                            .desired_width(f32::INFINITY),
                                    );
                                    let f = filter.to_ascii_lowercase();
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        for r in index.models.iter() {
                                            let label = r.label();
                                            let ll = label.to_ascii_lowercase();
                                            let humanoid = ll.contains("hum") || ll.contains("merc")
                                                || ll.contains("char") || ll.contains("ped") || ll.contains("bip");
                                            if !humanoid || (!f.is_empty() && !ll.contains(&f)) {
                                                continue;
                                            }
                                            let on = retarget_target.as_ref().is_some_and(|(h, _)| *h == r.hash);
                                            if ui.selectable_label(on, label.clone()).clicked() {
                                                show_target_picker = false;
                                                actions.push(Act::RetargetSetTarget(r.hash, label.clone()));
                                            }
                                        }
                                    });
                                }
                            }
                          } // ── end match wb (navigator) ──
                        });
                        navigator_width = navigator_resp.response.rect.width();
                        // ── INSPECTOR (right): the current workbench's detail cards. ──
                        let inspector_resp = egui::SidePanel::right("inspector")
                            .resizable(true)
                            .default_width(inspector_width)
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin { left: 13.0, right: 12.0, top: 12.0, bottom: 0.0 }),
                            )
                            .show(ctx, |ui| {
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                              if matches!(wb, Workbench::Inspect) {
                                match &mut preview {
                                    None => {
                                        ui.weak("No model loaded — pick one in the browser at left.");
                                    }
                                    Some(p) => {
                                        let (phash, plabel) = (p.hash, p.label.clone());
                                        let head = ui.add(
                                            egui::Label::new(theme::disp_text(
                                                plabel.to_uppercase(),
                                                19.0,
                                                theme::TX,
                                            ))
                                            .sense(egui::Sense::click()),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("0x{:08X}", p.hash))
                                                .monospace()
                                                .size(11.5)
                                                .color(theme::DIM),
                                        );
                                        ui.add_space(8.0);
                                        head.context_menu(|ui| {
                                            if ui.button("Export (OBJ + textures)").clicked() {
                                                actions.push(Act::Export);
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            if ui.button("Copy name").clicked() {
                                                ui.ctx().copy_text(plabel.clone());
                                                ui.close_menu();
                                            }
                                            if ui.button(format!("Copy hash 0x{phash:08X}")).clicked() {
                                                ui.ctx().copy_text(format!("0x{phash:08X}"));
                                                ui.close_menu();
                                            }
                                        });
                                        theme::section(ui, "Info", None, true, |ui| {
                                                theme::kv(ui, "verts / tris", egui::RichText::new(format!("{} / {}", p.verts, p.tris)));
                                                theme::kv(ui, "draw groups", egui::RichText::new(format!("{} · {} hidden", p.draws.len(), p.hidden.len())));
                                                theme::kv(ui, "bones", egui::RichText::new(p.rig.len().to_string()));
                                                theme::kv(ui, "textures", egui::RichText::new(p.tex_hashes.len().to_string()));
                                                theme::kv(ui, "radius", egui::RichText::new(format!("{:.2} m", p.radius)));
                                                ui.add_space(4.0);
                                                if ui.button("View textures").clicked() {
                                                    actions.push(Act::TexOfPreview);
                                                }
                                                // Loadout/weapon compatibility for the selected
                                                // clip: every equipment key the AnimationLookup
                                                // keys this clip's Handles by — ✔ when that
                                                // loadout resolves to THIS clip, ✖ when it picks
                                                // a different one. Real table rows, nothing
                                                // inferred.
                                                if let (Some(ci), Some(sel), Some(cs)) = (
                                                    p.cur_clip,
                                                    anim_sel.as_ref(),
                                                    p.character_set.as_deref(),
                                                ) {
                                                    let e = &p.clip_catalog[ci];
                                                    const NONE: u32 =
                                                        mercs2_formats::anim_select::NONE_SENTINEL;
                                                    let mut order: Vec<String> = Vec::new();
                                                    let mut compat: HashMap<String, bool> =
                                                        HashMap::new();
                                                    let character =
                                                        AnimSelector::character_name(cs);
                                                    for &h in &e.handles {
                                                        for lc in sel.lookup_context(h, character) {
                                                            let mut parts: Vec<String> = Vec::new();
                                                            for v in [
                                                                lc.primary_equipment_class,
                                                                lc.primary_equipment_name,
                                                                lc.in_use_equipment_class,
                                                                lc.in_use_equipment_name,
                                                            ] {
                                                                if v != NONE {
                                                                    parts.push(anim_val(&index, v));
                                                                }
                                                            }
                                                            let label = if parts.is_empty() {
                                                                "any (default row)".to_string()
                                                            } else {
                                                                parts.join(" / ")
                                                            };
                                                            let ok = lc.clip == e.hash;
                                                            match compat.get_mut(&label) {
                                                                Some(x) => *x |= ok,
                                                                None => {
                                                                    order.push(label.clone());
                                                                    compat.insert(label, ok);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    if !order.is_empty() {
                                                        ui.separator();
                                                        ui.weak(format!(
                                                            "Loadout compatibility — clip 0x{:08X}",
                                                            e.hash
                                                        ))
                                                        .on_hover_text(
                                                            "AnimationLookup rows for this clip's \
                                                             ActionTable Handles: ✔ = that \
                                                             loadout resolves to this clip, ✖ = \
                                                             it picks a different clip.",
                                                        );
                                                        egui::Grid::new("clip_compat")
                                                            .num_columns(2)
                                                            .striped(true)
                                                            .show(ui, |ui| {
                                                                for label in &order {
                                                                    ui.label(label.as_str());
                                                                    if compat[label] {
                                                                        ui.colored_label(
                                                                            egui::Color32::from_rgb(
                                                                                0x7C, 0xC8, 0x4B,
                                                                            ),
                                                                            "✔",
                                                                        );
                                                                    } else {
                                                                        ui.colored_label(
                                                                            egui::Color32::from_rgb(
                                                                                0xD0, 0x60, 0x50,
                                                                            ),
                                                                            "✖",
                                                                        );
                                                                    }
                                                                    ui.end_row();
                                                                }
                                                            });
                                                    }
                                                }
                                            });
                                        // LOD — driven by the model's REAL tier masks, not invented
                                        // "rungs". A mesh's mask comes from SEGM[INDX[group]]; several
                                        // meshes usually share one mask. We show the masks the model
                                        // actually carries (with how many meshes each covers) and let
                                        // you drive `view_state` bit by bit, so every click maps to a
                                        // visible change. (The old rung/window chips were built on
                                        // masks we were reading from the WRONG records.)
                                        {
                                            use std::collections::BTreeMap;
                                            let mut by_mask: BTreeMap<u8, usize> = BTreeMap::new();
                                            for d in &p.draws {
                                                *by_mask.entry(d.lod_mask).or_default() += 1;
                                            }
                                            let drawn_at = |vs: u8| -> usize {
                                                p.draws
                                                    .iter()
                                                    .filter(|d| (vs & d.lod_mask) != 0)
                                                    .count()
                                            };
                                            let lod_badge = format!(
                                                "view_state 0x{:02X} · {}/{}",
                                                p.tier,
                                                drawn_at(p.tier),
                                                p.draws.len()
                                            );
                                            theme::section(ui, "LOD", Some(&lod_badge), true, |ui| {
                                                ui.horizontal(|ui| {
                                                    for b in 0..8u8 {
                                                        let bit = 1u8 << b;
                                                        // Chip labels are LOD levels L1..L8 (mercs1
                                                        // `GetLOD`: name digit d sets bit d-1; bit 0 =
                                                        // nearest). Toggling drives `view_state`.
                                                        if theme::bit_chip(ui, &format!("{}", b + 1), (p.tier & bit) != 0) {
                                                            actions.push(Act::Tier(p.tier ^ bit));
                                                        }
                                                    }
                                                });
                                                ui.add_space(6.0);
                                                theme::eyebrow(ui, "LOD tiers this model carries");
                                                ui.add_space(2.0);
                                                ui.weak("grouped by behaviour · bit 0 = nearest, higher = farther");
                                                ui.add_space(6.0);
                                                // Behaviour of a tier mask, read from its extremes vs the
                                                // model's tier span (research: masks partition the LOD chain
                                                // — resident=far, finer=near, 0x7F=always; see
                                                // docs/modernization/vehicle_model_spec.md §3).
                                                let union = by_mask.keys().fold(0u8, |a, &m| a | m);
                                                let umax = 7u32.saturating_sub(union.leading_zeros());
                                                let behaviour = |mask: u8| -> (u8, &'static str, &'static str) {
                                                    let near = mask & 1 != 0;
                                                    let far = (mask & (1u8 << umax)) != 0;
                                                    match (near, far) {
                                                        (true, true) => (3, "All distances", "drawn at every distance — caps / trim / structural"),
                                                        (true, false) => (0, "Close-up", "high detail — culls with distance"),
                                                        (false, true) => (2, "Distant", "low-detail far proxy — absent up close"),
                                                        (false, false) => (1, "Mid-range", "a middle distance band"),
                                                    }
                                                };
                                                let mut rows: Vec<(u8, u8, usize)> = by_mask
                                                    .iter()
                                                    .map(|(&m, &n)| (behaviour(m).0, m, n))
                                                    .collect();
                                                rows.sort_by_key(|&(r, m, _)| (r, m.trailing_zeros(), m));
                                                let mut cur_rank: Option<u8> = None;
                                                for (_rank, mask, n) in rows {
                                                    let (rank, label, hint) = behaviour(mask);
                                                    if cur_rank != Some(rank) {
                                                        if cur_rank.is_some() {
                                                            ui.add_space(6.0);
                                                        }
                                                        cur_rank = Some(rank);
                                                        ui.horizontal(|ui| {
                                                            ui.label(theme::disp_text(label.to_uppercase(), 9.5, theme::BRASS));
                                                            ui.label(egui::RichText::new(hint).size(10.0).color(theme::FAINT));
                                                        });
                                                        ui.add_space(4.0);
                                                    }
                                                    let hit = (p.tier & mask) != 0;
                                                    // Passing tiers = green (border + tint + ✔);
                                                    // filtered = neutral + faint ✖. Whole chip clicks.
                                                    let (fill, border) = if hit {
                                                        (theme::GOOD_SOFT, theme::GOOD_DK)
                                                    } else {
                                                        (theme::G0, theme::LINE)
                                                    };
                                                    let r = egui::Frame::none()
                                                        .fill(fill)
                                                        .stroke(egui::Stroke::new(1.0, border))
                                                        .rounding(egui::Rounding::same(5.0))
                                                        .inner_margin(egui::Margin::symmetric(9.0, 5.0))
                                                        .outer_margin(egui::Margin { bottom: 5.0, ..Default::default() })
                                                        .show(ui, |ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.set_width(ui.available_width());
                                                                ui.label(
                                                                    egui::RichText::new(lod_levels(mask))
                                                                        .monospace()
                                                                        .size(11.5)
                                                                        .color(if hit { theme::TX } else { theme::DIM }),
                                                                );
                                                                ui.add_space(4.0);
                                                                ui.label(
                                                                    egui::RichText::new(format!("0x{mask:02X}"))
                                                                        .monospace()
                                                                        .size(11.0)
                                                                        .color(theme::FAINT),
                                                                );
                                                                ui.add_space(8.0);
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "{n} mesh{}",
                                                                        if n == 1 { "" } else { "es" }
                                                                    ))
                                                                    .size(11.5)
                                                                    .color(theme::DIM),
                                                                );
                                                                ui.with_layout(
                                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                                    |ui| {
                                                                        if hit {
                                                                            ui.label(egui::RichText::new("✔ passes").size(11.5).color(theme::GOOD));
                                                                        } else {
                                                                            ui.label(egui::RichText::new("✖ filtered").size(11.5).color(theme::FAINT));
                                                                        }
                                                                    },
                                                                );
                                                            });
                                                        });
                                                    if r.response
                                                        .interact(egui::Sense::click())
                                                        .on_hover_text("click to set view_state to exactly this tier mask")
                                                        .clicked()
                                                    {
                                                        actions.push(Act::Tier(mask));
                                                    }
                                                }
                                                ui.add_space(2.0);
                                                ui.weak(
                                                    "LOD is one axis; destruction (below) is the other. A mesh draws only if BOTH pass.",
                                                );
                                            });
                                        }
                                        // Destruction — the ENGINE's state machine, interactive:
                                        // pick any node's state and its OWN SHOW/Hide script
                                        // re-executes over the mesh. All labels come from game
                                        // data (state/node name hashes, resolved where known).
                                        if let Some(sm) = p.machine.clone() {
                                            let nstates: usize =
                                                sm.nodes.iter().map(|n| n.states.len()).sum();
                                            let d_badge =
                                                format!("{} nodes · {nstates} states", sm.nodes.len());
                                            theme::section(ui, "Destruction", Some(&d_badge), false, |ui| {
                                                // HEALTH drives the machine — full = pristine, dropping
                                                // = damaged/on-fire, 0 = wreck.
                                                theme::eyebrow(ui, "Health");
                                                ui.add_space(4.0);
                                                let mut hp = p.health * 100.0;
                                                ui.horizontal(|ui| {
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut hp, 0.0..=100.0)
                                                                .suffix("%")
                                                                .fixed_decimals(0),
                                                        )
                                                        .changed()
                                                    {
                                                        actions.push(Act::SetHealth(hp / 100.0));
                                                    }
                                                    if ui.small_button("full").clicked() {
                                                        actions.push(Act::SetHealth(1.0));
                                                    }
                                                    if ui.small_button("wreck").clicked() {
                                                        actions.push(Act::SetHealth(0.0));
                                                    }
                                                });
                                                ui.weak("drives the state machine from damage — the states are the engine's");
                                                ui.add_space(8.0);
                                                theme::eyebrow(ui, "State machine — pick a node's state");
                                                ui.add_space(5.0);
                                                let hier: std::collections::HashSet<u32> =
                                                    p.hier.iter().copied().collect();
                                                let resolve = |h: u32| {
                                                    let tag = if hier.contains(&h) { "@" } else { "" };
                                                    format!("{tag}{}", name_or_hash(&index, h))
                                                };
                                                egui::ScrollArea::vertical()
                                                    .max_height(620.0)
                                                    .id_source("machine_scroll")
                                                    .show(ui, |ui| {
                                                        for (ni, node) in sm.nodes.iter().enumerate() {
                                                            let cur = p.node_state.get(ni).copied().unwrap_or(0);
                                                            ui.label(theme::disp_text(
                                                                name_or_hash(&index, node.name_hash),
                                                                11.0,
                                                                theme::TX,
                                                            ));
                                                            ui.add_space(3.0);
                                                            ui.horizontal_wrapped(|ui| {
                                                                for (si, st) in node.states.iter().enumerate() {
                                                                    if theme::pill(
                                                                        ui,
                                                                        &name_or_hash(&index, st.name_hash),
                                                                        cur == si,
                                                                    )
                                                                    .clicked()
                                                                    {
                                                                        actions.push(Act::NodeState(ni, si));
                                                                    }
                                                                }
                                                            });
                                                            if let Some(st) = node.states.get(cur) {
                                                                let enter = mercs2_formats::orchestrator::decode_script(&st.enter, resolve);
                                                                if !enter.is_empty() {
                                                                    ui.label(egui::RichText::new(format!("→ {enter}")).monospace().size(10.0).color(theme::FAINT));
                                                                }
                                                                let exit = mercs2_formats::orchestrator::decode_script(&st.exit, resolve);
                                                                if !exit.is_empty() {
                                                                    ui.label(egui::RichText::new(format!("← {exit}")).monospace().size(10.0).color(theme::FAINT));
                                                                }
                                                            }
                                                            ui.add_space(9.0);
                                                        }
                                                        ui.weak("@name = a HIER node of this model");
                                                    });
                                            });
                                        }
                                        let seg_badge = format!(
                                            "{} / {} drawn",
                                            p.draws.len() - p.hidden.len(),
                                            p.draws.len()
                                        );
                                        theme::section(ui, "Segments", Some(&seg_badge), false, |ui| {
                                            // THE disassembly view. Per mesh: its seg_id (INDX[group]),
                                            // its real mount NODE (from SEGM[INDX[group]]), its LOD mask,
                                            // its material \u{2014} and when it is NOT drawn, WHICH CLAUSE
                                            // gated it. This is what makes the panel map to the picture.
                                            let mut hr = p.hide_ruin;
                                            if ui
                                                .checkbox(&mut hr, "Hide *_ruin* sub-strips")
                                                .changed()
                                            {
                                                actions.push(Act::ToggleRuin);
                                            }
                                            egui::ScrollArea::vertical()
                                                .max_height(620.0)
                                                .id_source("mtrl_scroll")
                                                .show(ui, |ui| {
                                                    for gi in 0..p.draws.len() {
                                                        let d = p.draws[gi].clone();
                                                        let tris = d.index_count / 3;
                                                        let (diff, norm, spec) =
                                                            (d.diffuse, d.normal, d.specular);
                                                        let tex = diff
                                                            .map(|h| name_or_hash(&index, h))
                                                            .unwrap_or_else(|| "-".into());
                                                        // Why is this mesh not drawn? Ask the gate.
                                                        let lod_ok = (p.tier & d.lod_mask) != 0;
                                                        let node_ok = d.node < 0
                                                            || p.node_enable
                                                                .get(d.node as usize)
                                                                .copied()
                                                                .unwrap_or(true);
                                                        let hidden = p.hidden.contains(&gi);
                                                        let (mark, why) = if lod_ok && node_ok && hidden {
                                                            ("x", "hidden by hand".to_string())
                                                        } else if !lod_ok {
                                                            ("x", format!(
                                                                "LOD: mask 0x{:02X} shares no bit with view_state 0x{:02X}",
                                                                d.lod_mask, p.tier))
                                                        } else if !node_ok {
                                                            ("x", format!(
                                                                "DESTRUCTION: node {} is switched off in this state",
                                                                d.node))
                                                        } else {
                                                            ("*", "drawn".to_string())
                                                        };
                                                        let node_lbl = if d.node < 0 {
                                                            "-".to_string()
                                                        } else {
                                                            p.hier_nodes
                                                                .get(d.node as usize)
                                                                .map(|h| name_or_hash(&index, h.hash))
                                                                .unwrap_or_else(|| d.node.to_string())
                                                        };
                                                        let _ = mark;
                                                        // Framed chip, coloured by draw state: green =
                                                        // drawn, neutral+faint = filtered/hidden, brass
                                                        // border when selected. Columns: material · LOD
                                                        // tier · status + tri count (right). seg/node/
                                                        // mask are in the hover.
                                                        let drawn = lod_ok && node_ok && !hidden;
                                                        let (fill, border, statusc, statustxt) =
                                                            if !lod_ok || !node_ok {
                                                                (theme::G0, theme::LINE, theme::FAINT, "✖ filtered")
                                                            } else if hidden {
                                                                (theme::G0, theme::LINE, theme::DIM, "· hidden")
                                                            } else {
                                                                (theme::GOOD_SOFT, theme::GOOD_DK, theme::GOOD, "✔ drawn")
                                                            };
                                                        let border = if p.sel_group == gi { theme::BRASS } else { border };
                                                        let why = format!(
                                                            "{why}\nseg {} · node {} ({node_lbl}) · mask 0x{:02X} · {tex}",
                                                            d.seg_id, d.node, d.lod_mask
                                                        );
                                                        let tex_short: String = tex.chars().take(22).collect();
                                                        let resp = theme::row_chip(ui, fill, border, |ui| {
                                                            let mut vis = !hidden;
                                                            if ui.checkbox(&mut vis, "").changed() {
                                                                actions.push(Act::GroupToggle(gi));
                                                            }
                                                            ui.label(
                                                                egui::RichText::new(tex_short)
                                                                    .size(11.5)
                                                                    .color(if drawn { theme::TX } else { theme::DIM }),
                                                            );
                                                            ui.label(
                                                                egui::RichText::new(lod_levels(d.lod_mask))
                                                                    .monospace()
                                                                    .size(10.0)
                                                                    .color(theme::FAINT),
                                                            );
                                                            ui.with_layout(
                                                                egui::Layout::right_to_left(egui::Align::Center),
                                                                |ui| {
                                                                    ui.label(egui::RichText::new(statustxt).size(11.0).color(statusc));
                                                                    ui.add_space(6.0);
                                                                    ui.label(egui::RichText::new(format!("{tris} tri")).monospace().size(10.0).color(theme::DIM));
                                                                },
                                                            );
                                                        });
                                                        let resp = resp.on_hover_text(&why);
                                                        if resp.clicked() {
                                                            p.sel_group = gi;
                                                        }
                                                        resp.context_menu(|ui| {
                                                                if ui.button("Isolate (hide others)").clicked() {
                                                                    actions.push(Act::IsolateGroup(gi));
                                                                    ui.close_menu();
                                                                }
                                                                if ui.button("Show all groups").clicked() {
                                                                    actions.push(Act::ShowAllGroups);
                                                                    ui.close_menu();
                                                                }
                                                                if ui.button("Toggle visibility").clicked() {
                                                                    actions.push(Act::GroupToggle(gi));
                                                                    ui.close_menu();
                                                                }
                                                                ui.separator();
                                                                for (slot, h) in [
                                                                    ("diffuse", diff),
                                                                    ("normal", norm),
                                                                    ("specular", spec),
                                                                ] {
                                                                    if let Some(h) = h {
                                                                        if ui
                                                                            .button(format!(
                                                                                "Copy {slot} hash 0x{h:08X}"
                                                                            ))
                                                                            .clicked()
                                                                        {
                                                                            ui.ctx().copy_text(format!("0x{h:08X}"));
                                                                            ui.close_menu();
                                                                        }
                                                                    }
                                                                }
                                                                if let Some(h) = diff {
                                                                    if ui.button("Copy diffuse name").clicked() {
                                                                        ui.ctx().copy_text(name_or_hash(&index, h));
                                                                        ui.close_menu();
                                                                    }
                                                                }
                                                            });
                                                    }
                                                });
                                        }); // ── end Segments section ──
                                        let src = p
                                            .character_set
                                            .as_deref()
                                            .map(|c| format!("character:{c}"))
                                            .unwrap_or_else(|| "generic".into());
                                        let anim_badge = format!("{} clips · {src}", p.clip_catalog.len());
                                        theme::section(ui, "Animation", Some(&anim_badge), true, |ui| {
                                            if let Some(ci) = p.cur_clip {
                                                let hash = p.clip_catalog[ci].hash;
                                                if let Some(Some(c)) = p.clip_cache.get(&hash) {
                                                    let dur = c.clip.duration.max(1e-3);
                                                    ui.horizontal(|ui| {
                                                        if ui
                                                            .button(if p.playing { "⏸" } else { "▶" })
                                                            .clicked()
                                                        {
                                                            actions.push(Act::PlayPause);
                                                        }
                                                        if ui.button("⏹").clicked() {
                                                            actions.push(Act::ClipStop);
                                                        }
                                                        ui.monospace(format!(
                                                            "{:>5.2} / {dur:.2}s",
                                                            p.anim_time
                                                        ));
                                                    });
                                                    ui.add(
                                                        egui::Slider::new(&mut p.anim_time, 0.0..=dur)
                                                            .show_value(false),
                                                    );
                                                }
                                            }
                                            egui::ScrollArea::vertical()
                                                .max_height(580.0)
                                                .id_source("clip_scroll")
                                                .show(ui, |ui| {
                                                    for i in 0..p.clip_catalog.len() {
                                                        let e = &p.clip_catalog[i];
                                                        let (chash, chandle, clabel) = (
                                                            e.hash,
                                                            e.handles.first().copied(),
                                                            e.label.clone(),
                                                        );
                                                        let (dur, unbound) = match p.clip_cache.get(&chash) {
                                                            Some(Some(c)) => (Some(c.clip.duration), false),
                                                            Some(None) => (None, true),
                                                            None => (None, false),
                                                        };
                                                        let sel = p.cur_clip == Some(i);
                                                        let (fill, bord) = if sel {
                                                            (theme::BRASS_SOFT, theme::BRASS_DK)
                                                        } else {
                                                            (theme::G0, theme::LINE)
                                                        };
                                                        // Name (corpus or procedural) prominent, hash
                                                        // dim, duration / bind-state right-aligned.
                                                        let nm = e.name.clone().unwrap_or_else(|| format!("clip {}", i + 1));
                                                        let mut resp = theme::row_chip(ui, fill, bord, |ui| {
                                                            ui.label(egui::RichText::new(nm).size(11.5).color(if sel { theme::BRASS } else { theme::TX }));
                                                            ui.label(egui::RichText::new(format!("0x{chash:08X}")).monospace().size(9.5).color(theme::FAINT));
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                if let Some(d) = dur {
                                                                    ui.label(egui::RichText::new(format!("{d:.2}s")).monospace().size(10.5).color(theme::GOOD));
                                                                } else if unbound {
                                                                    ui.label(egui::RichText::new("unbound").size(10.0).color(theme::FAINT));
                                                                }
                                                            });
                                                        });
                                                        if let Some(h) = chandle {
                                                            resp = resp.on_hover_text(format!("ActionTable Handle 0x{h:08X}"));
                                                        }
                                                        if resp.clicked() {
                                                            actions.push(Act::ClipSel(i));
                                                        }
                                                        resp.context_menu(|ui| {
                                                            if ui.button("Play").clicked() {
                                                                actions.push(Act::ClipSel(i));
                                                                ui.close_menu();
                                                            }
                                                            ui.separator();
                                                            if ui.button("Copy clip name").clicked() {
                                                                ui.ctx().copy_text(clabel.clone());
                                                                ui.close_menu();
                                                            }
                                                            if ui
                                                                .button(format!("Copy clip hash 0x{chash:08X}"))
                                                                .clicked()
                                                            {
                                                                ui.ctx().copy_text(format!("0x{chash:08X}"));
                                                                ui.close_menu();
                                                            }
                                                            if let Some(h) = chandle {
                                                                if ui
                                                                    .button(format!("Copy Handle 0x{h:08X}"))
                                                                    .clicked()
                                                                {
                                                                    ui.ctx().copy_text(format!("0x{h:08X}"));
                                                                    ui.close_menu();
                                                                }
                                                            }
                                                        });
                                                    }
                                                });
                                        }); // ── end Animation section ──
                                        // Game scripts that mention this asset — literal corpus
                                        // search hits (the needle used is shown; decompiled Lua
                                        // is game data).
                                        if !p.lua_refs.is_empty() {
                                            let lua_badge =
                                                format!("\"{}\" · {}", p.lua_needle, p.lua_refs.len());
                                            theme::section(ui, "Game scripts", Some(&lua_badge), false, |ui| {
                                                egui::ScrollArea::vertical()
                                                    .max_height(520.0)
                                                    .id_source("lua_refs_scroll")
                                                    .show(ui, |ui| {
                                                        for (path, lines) in &p.lua_refs {
                                                            // Click = open the whole script in
                                                            // the read-only Lua viewer, jumped
                                                            // to the first match.
                                                            let row = ui
                                                                .link(
                                                                    egui::RichText::new(
                                                                        path.as_str(),
                                                                    )
                                                                    .monospace(),
                                                                )
                                                                .on_hover_text(
                                                                    "open in the Lua viewer",
                                                                );
                                                            if row.clicked() {
                                                                actions.push(Act::LuaOpen(
                                                                    path.clone(),
                                                                ));
                                                            }
                                                            row.context_menu(|ui| {
                                                                if ui.button("Open script").clicked() {
                                                                    actions.push(Act::LuaOpen(
                                                                        path.clone(),
                                                                    ));
                                                                    ui.close_menu();
                                                                }
                                                                if ui.button("Copy script path").clicked() {
                                                                    ui.ctx().copy_text(path.clone());
                                                                    ui.close_menu();
                                                                }
                                                            });
                                                            for l in lines {
                                                                ui.weak(format!("  {l}"));
                                                            }
                                                        }
                                                    });
                                            }); // ── end Game scripts section ──
                                        }
                                        let skel_badge = format!("{} bones", p.rig.len());
                                        theme::section(ui, "Skeleton", Some(&skel_badge), false, |ui| {
                                            ui.weak("hover = highlight in view, click = pin");
                                            egui::ScrollArea::vertical()
                                                .max_height(620.0)
                                                .id_source("hier_scroll")
                                                .show(ui, |ui| {
                                                    for (i, b) in p.rig.iter().enumerate() {
                                                        let mut depth = 0usize;
                                                        let mut cur = b.parent;
                                                        while cur >= 0 && depth < 16 {
                                                            depth += 1;
                                                            cur = p.rig[cur as usize].parent;
                                                        }
                                                        let bhash = b.name_hash;
                                                        let bname = name_or_hash(&index, bhash);
                                                        let named = index.names.contains_key(&bhash);
                                                        // Two-tone tree row: dim depth indent + index,
                                                        // then the bone name (bright if named, dim if
                                                        // it fell back to a hash).
                                                        let mut job = egui::text::LayoutJob::default();
                                                        let fmt = |size: f32, color: egui::Color32| egui::TextFormat {
                                                            font_id: egui::FontId::monospace(size),
                                                            color,
                                                            ..Default::default()
                                                        };
                                                        job.append(&"  ".repeat(depth), 0.0, fmt(11.0, theme::FAINT));
                                                        job.append(&format!("{i:>3}  "), 0.0, fmt(10.0, theme::FAINT));
                                                        job.append(&bname, 0.0, fmt(11.5, if named { theme::TX } else { theme::DIM }));
                                                        let row = ui.selectable_label(sel_bone == Some(i), job);
                                                        if row.hovered() {
                                                            hovered_bone = Some(i);
                                                        }
                                                        if row.clicked() {
                                                            sel_bone = if sel_bone == Some(i) {
                                                                None
                                                            } else {
                                                                Some(i)
                                                            };
                                                        }
                                                        row.context_menu(|ui| {
                                                            if ui
                                                                .button(format!("Copy hash 0x{bhash:08X}"))
                                                                .clicked()
                                                            {
                                                                ui.ctx().copy_text(format!("0x{bhash:08X}"));
                                                                ui.close_menu();
                                                            }
                                                            if ui.button("Copy bone name").clicked() {
                                                                ui.ctx().copy_text(bname.clone());
                                                                ui.close_menu();
                                                            }
                                                            ui.separator();
                                                            if ui
                                                                .button(if sel_bone == Some(i) {
                                                                    "Unpin highlight"
                                                                } else {
                                                                    "Pin highlight"
                                                                })
                                                                .clicked()
                                                            {
                                                                sel_bone = if sel_bone == Some(i) {
                                                                    None
                                                                } else {
                                                                    Some(i)
                                                                };
                                                                ui.close_menu();
                                                            }
                                                        });
                                                    }
                                                });
                                        }); // ── end Skeleton section ──
                                    }
                                }
                              } // ── end Inspect inspector ──
                              if matches!(wb, Workbench::Sandbox) {
                                // The SELECTED instance's transform, then scene-wide actions.
                                match sel_placed.and_then(|i| placed.get(i).map(|p| (i, p.label.clone()))) {
                                    Some((i, plabel)) => {
                                        ui.label(theme::disp_text(format!("Instance n{i} · selected"), 9.5, theme::BRASS_DK));
                                        ui.label(theme::disp_text(plabel.to_uppercase(), 18.0, theme::TX));
                                        ui.add_space(10.0);
                                        theme::card(ui, "Transform", None, |ui| {
                                            let mut changed = false;
                                            {
                                                let pl = &mut placed[i];
                                                let mut pos = [pl.pos.x, pl.pos.y, pl.pos.z];
                                                if theme::vec3_field(ui, "Location", &mut pos, 0.05) {
                                                    pl.pos = Vec3::new(pos[0], pos[1], pos[2]);
                                                    changed = true;
                                                }
                                                changed |= theme::scalar_field(ui, "Yaw", &mut pl.yaw, 0.01);
                                                let mut scale = pl.scale;
                                                if theme::scalar_field(ui, "Scale", &mut scale, 0.01) {
                                                    pl.scale = scale.clamp(0.01, 1000.0);
                                                    changed = true;
                                                }
                                            }
                                            if changed {
                                                actions.push(Act::SyncPlaced(i));
                                            }
                                            ui.add_space(7.0);
                                            ui.horizontal(|ui| {
                                                if ui.button("Duplicate").clicked() {
                                                    actions.push(Act::DuplicatePlaced(i));
                                                }
                                                if ui.button("Snap to camera").clicked() {
                                                    actions.push(Act::SnapPlaced(i));
                                                }
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if theme::danger_button(ui, "Delete", true).clicked() {
                                                        actions.push(Act::RemovePlaced(i));
                                                        sel_placed = None;
                                                    }
                                                });
                                            });
                                        });
                                    }
                                    None => {
                                        ui.label(theme::disp_text("Sandbox", 18.0, theme::TX));
                                        ui.add_space(6.0);
                                        ui.weak("Select an object in the Scene list at left to move / rotate / scale it.");
                                        ui.add_space(10.0);
                                    }
                                }
                                let sbadge = format!("{} placed", placed.len());
                                theme::card(ui, "Scene", Some(&sbadge), |ui| {
                                    if theme::primary_button(ui, "Merge to one model", !placed.is_empty()).clicked() {
                                        actions.push(Act::Merge);
                                    }
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        if ui.button("Save scene").clicked() {
                                            actions.push(Act::SaveScene);
                                        }
                                        if ui.button("Load scene").clicked() {
                                            actions.push(Act::LoadScene);
                                        }
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if theme::danger_button(ui, "Clear", !placed.is_empty()).clicked() {
                                                actions.push(Act::ClearSandbox);
                                            }
                                        });
                                    });
                                    ui.add_space(4.0);
                                    ui.weak("Merging bakes every placed instance into one model you can export or add to a mod.");
                                });
                              } // ── end Sandbox inspector ──
                              if matches!(wb, Workbench::Mods) {
                                // ── Interaction hardpoints: a vehicle's `hp_*` HIER nodes (seat you
                                // enter at, exhausts, wheels, muzzle). Conform at a different SIZE and
                                // these stay where the DONOR's were — the seat floats off the model —
                                // so re-place them HERE, on the model, then copy the --node-at args. ──
                                if let Some(p) = &preview {
                                    theme::section(ui, "Interaction hardpoints", None, true, |ui| {
                                            ui.checkbox(&mut show_hardpoints, "show markers in viewport");
                                            ui.checkbox(&mut show_nodes, "show ALL node markers")
                                                .on_hover_text("green = positioned attach node (rotor/skid/seat/tail/hardpoint) · grey = structural");
                                            if hp_names.is_empty() {
                                                let hashes: Vec<u32> = p.rig.iter().map(|b| b.name_hash).collect();
                                                hp_names = resolve_node_names(&hashes);
                                            }
                                            let mut hps: Vec<(usize, String, [f32; 3])> = Vec::new();
                                            for (i, b) in p.rig.iter().enumerate() {
                                                if let Some(n) = hp_names.get(&b.name_hash) {
                                                    if n.starts_with("hp_") {
                                                        let w = [b.world_bind[3][0], b.world_bind[3][1], b.world_bind[3][2]];
                                                        let pos = hp_edits.get(&i).copied().unwrap_or(w);
                                                        hps.push((i, n.clone(), pos));
                                                    }
                                                }
                                            }
                                            if hps.is_empty() {
                                                ui.weak("no hp_* nodes resolved for this model");
                                            } else {
                                                ui.weak(format!("{} hardpoints — the SEAT is where the player enters", hps.len()));
                                                ui.add_space(4.0);
                                                // A table: X/Y/Z column headers, then striped rows —
                                                // contiguous bands (zero inter-row spacing), each a
                                                // fixed 26px tall so the name and cells share one
                                                // centred line. No inner scroll — the panel scrolls.
                                                theme::vec3_header(ui, "hardpoint");
                                                ui.add_space(2.0);
                                                let mut hp_click: Option<usize> = None;
                                                let mut hp_edit: Option<(usize, [f32; 3])> = None;
                                                ui.vertical(|ui| {
                                                    ui.spacing_mut().item_spacing.y = 0.0;
                                                    for (ri, (idx, name, pos)) in hps.iter().enumerate() {
                                                    let selrow = hp_selected == Some(*idx);
                                                    let mut v = *pos;
                                                    let band = if selrow {
                                                        theme::BRASS_SOFT
                                                    } else {
                                                        theme::row_stripe(ri % 2 == 1)
                                                    };
                                                    egui::Frame::none()
                                                        .fill(band)
                                                        .inner_margin(egui::Margin { left: 0.0, right: 0.0, top: 2.0, bottom: 2.0 })
                                                        .show(ui, |ui| {
                                                        ui.horizontal(|ui| {
                                                        ui.spacing_mut().item_spacing.x = 4.0;
                                                        let col = if selrow { theme::BRASS } else { theme::TX };
                                                        // Left-aligned name in the shared splitter
                                                        // column, same 22px height as the cells so it
                                                        // centres on the same line.
                                                        let lw = theme::field_label_w(ui.available_width());
                                                        let clicked = ui
                                                            .allocate_ui_with_layout(
                                                                egui::vec2(lw, 22.0),
                                                                egui::Layout::left_to_right(egui::Align::Center),
                                                                |ui| {
                                                                    ui.set_min_width(lw);
                                                                    ui.set_min_height(22.0);
                                                                    ui.add(
                                                                        egui::Label::new(theme::disp_text(name, 10.5, col))
                                                                            .truncate()
                                                                            .sense(egui::Sense::click()),
                                                                    )
                                                                    .on_hover_text(name.as_str())
                                                                    .clicked()
                                                                },
                                                            )
                                                            .inner;
                                                        if clicked {
                                                            hp_click = Some(*idx);
                                                        }
                                                        if theme::vec3_field(ui, "", &mut v, 0.02) {
                                                            hp_edit = Some((*idx, v));
                                                        }
                                                        });
                                                    });
                                                    }
                                                });
                                                if let Some(idx) = hp_click {
                                                    hp_selected = if hp_selected == Some(idx) { None } else { Some(idx) };
                                                }
                                                if let Some((idx, v)) = hp_edit {
                                                    hp_edits.insert(idx, v);
                                                    hp_selected = Some(idx);
                                                }
                                                ui.add_space(4.0);
                                                ui.horizontal(|ui| {
                                                    if ui.button("reset").clicked() {
                                                        hp_edits.clear();
                                                    }
                                                    if ui.button("copy --node-at args").clicked() {
                                                        let args: Vec<String> = hp_edits
                                                            .iter()
                                                            .map(|(n, v)| format!("--node-at {n}:{:.3},{:.3},{:.3}", v[0], v[1], v[2]))
                                                            .collect();
                                                        let joined = args.join(" ");
                                                        ui.output_mut(|o| o.copied_text = joined.clone());
                                                        status = if joined.is_empty() {
                                                            "no hardpoint edits to copy".to_string()
                                                        } else {
                                                            format!("copied: {joined}")
                                                        };
                                                    }
                                                });
                                            }
                                        });
                                }
                                // ── Mod project: queue NOVEL new-hash assets and publish them
                                // as a patch WAD (docs/modernization/workshop_publish_pipeline.md
                                // M3). Flow: preview/select the donor model → drag-drop the
                                // import → name it → Add → Publish. ──
                                theme::section(ui, "Mod project", Some(&format!("{} queued", mod_items.len())), false, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label("donor:");
                                        match &mod_donor {
                                            Some((h, l)) => {
                                                ui.monospace(format!("0x{h:08X}"));
                                                ui.label(l.as_str());
                                            }
                                            None => {
                                                ui.weak("none");
                                            }
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui
                                            .button("Donor ← selected model")
                                            .on_hover_text(
                                                "use the model selected in the browser as the \
                                                 donor container (its rig/materials host the \
                                                 injected geometry)",
                                            )
                                            .clicked()
                                            && kind == Kind::Model
                                        {
                                            if let Some(&ri) = filtered.get(sel) {
                                                let row = &index.rows(Kind::Model)[ri];
                                                actions.push(Act::ModDonor(row.hash, row.label()));
                                            }
                                        }
                                        ui.label("host group:");
                                        ui.add(egui::DragValue::new(&mut mod_group).range(0..=63));
                                    });
                                    // ── Conform transform: place & scale the import against the
                                    // donor. Fields bake into the export (external_mesh_transformed);
                                    // `live` drives the pedestal so it moves in the viewport. ──
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.strong("Conform");
                                        ui.checkbox(&mut conform_live, "live preview");
                                        ui.checkbox(&mut show_nodes, "show nodes")
                                            .on_hover_text("mark every HIER node of the previewed model — green = positioned attach point (rotor/skid/seat/hardpoint), grey = origin/structural — as spatial anchors to map geometry onto");
                                        if ui
                                            .button("Load donor ref")
                                            .on_hover_text("place the donor template in the sandbox at the origin as a visual anchor")
                                            .clicked()
                                        {
                                            actions.push(Act::LoadDonorRef);
                                        }
                                        if ui
                                            .button("Auto-fit")
                                            .on_hover_text("seed scale + position from the donor's real geometry envelope (skids on the ground, centred)")
                                            .clicked()
                                        {
                                            actions.push(Act::ConformAutofit);
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if theme::scalar_field(ui, "Scale", &mut conform_scale, 0.005) {
                                            conform_scale = conform_scale.clamp(0.0001, 1000.0);
                                        }
                                        if ui.small_button("reset").clicked() {
                                            conform_scale = 1.0;
                                            conform_t = [0.0; 3];
                                            conform_r = [0.0; 3];
                                        }
                                    });
                                    theme::vec3_field(ui, "Location", &mut conform_t, 0.02);
                                    theme::vec3_field(ui, "Rotation", &mut conform_r, 1.0);
                                    ui.checkbox(&mut conform_flip, "flip winding on export (fix inside-out faces)");
                                    ui.separator();
                                    ui.horizontal(|ui| {
                                        ui.label("name:");
                                        ui.text_edit_singleline(&mut mod_name);
                                    });
                                    if !mod_name.is_empty() {
                                        let h = mercs2_formats::hash::pandemic_hash_m2(&mod_name);
                                        let clash = index.names.contains_key(&h);
                                        ui.weak(format!(
                                            "m2 hash 0x{h:08X}{}",
                                            if clash { "  — COLLIDES with an existing name!" } else { "" }
                                        ));
                                    }
                                    let is_import = preview
                                        .as_ref()
                                        .is_some_and(|p| imported.contains_key(&p.hash));
                                    let can_add =
                                        is_import && mod_donor.is_some() && !mod_name.is_empty();
                                    if ui
                                        .add_enabled(
                                            can_add,
                                            egui::Button::new("Add: package current import"),
                                        )
                                        .clicked()
                                    {
                                        actions.push(Act::ModAdd(mod_name.clone()));
                                    }
                                    if !can_add {
                                        ui.weak(
                                            "needs an imported model on the pedestal, a donor, \
                                             and a name",
                                        );
                                    }
                                    if !mod_items.is_empty() {
                                        ui.separator();
                                        let mut remove: Option<usize> = None;
                                        for (i, it) in mod_items.iter().enumerate() {
                                            ui.horizontal(|ui| {
                                                ui.monospace(format!("0x{:08X}", it.hash));
                                                ui.label(it.name.as_str());
                                                ui.weak(format!(
                                                    "← {} g{}",
                                                    it.donor_label, it.target_group
                                                ));
                                                if ui.small_button("✖").clicked() {
                                                    remove = Some(i);
                                                }
                                            });
                                        }
                                        if let Some(i) = remove {
                                            actions.push(Act::ModRemove(i));
                                        }
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label("output:");
                                        ui.text_edit_singleline(&mut mod_out);
                                    });
                                    let busy = publisher.is_some();
                                    if ui
                                        .add_enabled(
                                            !mod_items.is_empty() && !busy,
                                            egui::Button::new(if busy {
                                                "publishing…"
                                            } else {
                                                "Publish"
                                            }),
                                        )
                                        .clicked()
                                    {
                                        actions.push(Act::Publish);
                                    }
                                });
                              } // ── end Mods inspector ──
                              if matches!(wb, Workbench::Skeleton) {
                                ui.label(theme::disp_text("Skeleton retarget", 18.0, theme::TX));
                                match &retarget {
                                    None => {
                                        ui.add_space(4.0);
                                        if ui
                                            .add_enabled(!names_pending, egui::Button::new("Import rigged model…"))
                                            .clicked()
                                        {
                                            actions.push(Act::ImportModel);
                                        }
                                        ui.add_space(4.0);
                                        ui.weak(
                                            "Load a rigged model (ValveBiped / Mixamo / Unreal / Call of \
                                             Duty). The workshop reads its joints + weights, detects the \
                                             rig, auto-maps each bone onto the target Mercs2 HIER skeleton, \
                                             and writes a conformed skin palette so the mesh animates on \
                                             Mercs2 clips.",
                                        );
                                    }
                                    Some(r) => {
                                        ui.weak(format!(
                                            "{} · {} source bones",
                                            r.convention.label(),
                                            r.source_bones.len()
                                        ));
                                        match &retarget_target {
                                            Some((_, tl)) => {
                                                ui.label(format!("target: {tl}"));
                                            }
                                            None => {
                                                ui.colored_label(theme::HAZARD, "← pick a target skeleton at left");
                                            }
                                        }
                                        ui.separator();
                                        theme::eyebrow(ui, &format!(
                                            "Bone map — {} / {} mapped",
                                            r.mapped_count(),
                                            r.source_bones.len()
                                        ));
                                        egui::ScrollArea::vertical()
                                            .max_height(340.0)
                                            .id_source("retarget_map_scroll")
                                            .show(ui, |ui| {
                                                egui::Grid::new("retarget_map")
                                                    .num_columns(3)
                                                    .striped(true)
                                                    .show(ui, |ui| {
                                                        for m in &r.map {
                                                            // Hovering a mapping row ghosts the mesh + highlights
                                                            // this (source) bone in the viewer.
                                                            if ui.monospace(m.source.as_str()).hovered() {
                                                                hover_skel = Some((m.source_index, true));
                                                            }
                                                            // Target cell = manual-override dropdown: pick any
                                                            // target bone, or clear to none. Pushes RetargetManual.
                                                            let cur = m
                                                                .target_name
                                                                .clone()
                                                                .unwrap_or_else(|| "— none —".into());
                                                            egui::ComboBox::from_id_source(("tmap", m.source_index))
                                                                .selected_text(cur)
                                                                .width(160.0)
                                                                .show_ui(ui, |ui| {
                                                                    egui::ScrollArea::vertical()
                                                                        .max_height(260.0)
                                                                        .show(ui, |ui| {
                                                                            if ui
                                                                                .selectable_label(m.target_index.is_none(), "— none —")
                                                                                .clicked()
                                                                            {
                                                                                actions.push(Act::RetargetManual(m.source_index, None));
                                                                            }
                                                                            for (ti, tn) in r.target_bones.iter().enumerate() {
                                                                                if ui
                                                                                    .selectable_label(m.target_index == Some(ti), tn.as_str())
                                                                                    .clicked()
                                                                                {
                                                                                    actions.push(Act::RetargetManual(m.source_index, Some(ti)));
                                                                                }
                                                                            }
                                                                        });
                                                                });
                                                            let (ct, col) = match m.confidence {
                                                                crate::retarget::Confidence::Auto => ("auto", theme::GOOD),
                                                                crate::retarget::Confidence::Fuzzy => ("fuzzy", theme::BRASS),
                                                                crate::retarget::Confidence::Manual => ("manual", theme::INFO),
                                                                crate::retarget::Confidence::Unmapped => ("unmapped", theme::BAD),
                                                            };
                                                            ui.colored_label(col, ct);
                                                            ui.end_row();
                                                        }
                                                    });
                                            });
                                        ui.separator();
                                        theme::eyebrow(ui, "Orientation fix");
                                        egui::Grid::new("retarget_orient").num_columns(2).show(ui, |ui| {
                                            ui.label("up axis");
                                            ui.monospace(r.up_axis_label());
                                            ui.end_row();
                                            ui.label("scale");
                                            ui.monospace(format!("{:.4}", r.scale));
                                            ui.end_row();
                                        });
                                        // Animation list + transport, right here on the Skeleton page —
                                        // populated after Apply retarget grafts the target's rig + clips.
                                        if let Some(p) = &preview {
                                            ui.separator();
                                            theme::eyebrow(ui, &format!("Animation \u{2014} {} clips", p.clip_catalog.len()));
                                            clip_player_compact(ui, p, &mut actions);
                                        }
                                    }
                                }
                              } // ── end Skeleton inspector ──
                            });
                        });
                        // Remember the (possibly just-dragged) width so it survives a card collapse.
                        inspector_width = inspector_resp.response.rect.width();

                        // ── VIEWPORT HUD: status chips over the 3D (the panels are all placed now, so
                        // `available_rect` is the viewport region). Non-interactable so the camera drag
                        // works underneath. ──
                        let vp = ctx.available_rect();
                        egui::Area::new(egui::Id::new("vp_hud"))
                            .fixed_pos(vp.left_top() + egui::vec2(14.0, 12.0))
                            .interactable(false)
                            .show(ctx, |ui| {
                                ui.horizontal(|ui| {
                                    theme::chip(ui, "Orbit", true, Some(theme::BRASS));
                                    if wb == Workbench::Sandbox {
                                        theme::chip(ui, &format!("{} objects", placed.len()), false, None);
                                        if let Some(i) = sel_placed.filter(|&i| i < placed.len()) {
                                            theme::chip(ui, &format!("n{i} selected"), true, Some(theme::BRASS));
                                        }
                                    } else if let Some(p) = &preview {
                                        // Clip position — name (or bind pose) + n/total + play state.
                                        let n = p.clip_catalog.len();
                                        let clip = if n == 0 {
                                            "no clips".to_string()
                                        } else if let Some(ci) = p.cur_clip {
                                            let raw = p.clip_catalog[ci]
                                                .name
                                                .clone()
                                                .unwrap_or_else(|| format!("clip {}", ci + 1));
                                            let short: String = raw.chars().take(18).collect();
                                            format!(
                                                "{}{} · {}/{}",
                                                if p.playing { "▶ " } else { "" },
                                                short,
                                                ci + 1,
                                                n
                                            )
                                        } else {
                                            format!("bind pose · {n} clips")
                                        };
                                        theme::chip(ui, &clip, false, None);
                                    }
                                });
                            });
                        // Bone/hardpoint marker legend (top-right). Node/hardpoint markers are a
                        // Mods-workbench tool; the pinned-bone glow is the Inspect skeleton view.
                        let hp_legend = wb == Workbench::Mods && show_hardpoints;
                        let node_legend = wb == Workbench::Mods && show_nodes;
                        if hp_legend || node_legend || sel_bone.is_some() {
                            egui::Area::new(egui::Id::new("vp_legend"))
                                .fixed_pos(vp.right_top() + egui::vec2(-166.0, 12.0))
                                .interactable(false)
                                .show(ctx, |ui| {
                                    egui::Frame::none()
                                        .fill(egui::Color32::from_rgba_unmultiplied(14, 16, 20, 205))
                                        .stroke(egui::Stroke::new(1.0, theme::LINE))
                                        .rounding(egui::Rounding::same(5.0))
                                        .inner_margin(egui::Margin::symmetric(10.0, 8.0))
                                        .show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 5.0;
                                            let dot = |ui: &mut egui::Ui, c: egui::Color32, t: &str| {
                                                ui.horizontal(|ui| {
                                                    let (r, _) = ui.allocate_exact_size(
                                                        egui::vec2(8.0, 8.0),
                                                        egui::Sense::hover(),
                                                    );
                                                    ui.painter().rect_filled(
                                                        r,
                                                        egui::Rounding::same(1.0),
                                                        c,
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(t)
                                                            .size(10.5)
                                                            .color(theme::DIM),
                                                    );
                                                });
                                            };
                                            if hp_legend {
                                                dot(ui, egui::Color32::from_rgb(255, 217, 38), "seat / entry");
                                                dot(ui, egui::Color32::from_rgb(51, 153, 255), "hardpoint");
                                            }
                                            if node_legend {
                                                dot(ui, egui::Color32::from_rgb(77, 255, 115), "attach node");
                                                dot(ui, egui::Color32::from_rgb(153, 153, 179), "structural");
                                            }
                                            if sel_bone.is_some() {
                                                dot(ui, egui::Color32::from_rgb(90, 230, 255), "pinned bone");
                                            }
                                        });
                                });
                        }

                        if let Some(tv) = &tex_view {
                            egui::Window::new("Texture")
                                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -48.0])
                                .resizable(false)
                                .collapsible(false)
                                .show(ctx, |ui| {
                                    ui.label(format!(
                                        "{}  [{}/{}]  {}",
                                        tv.labels[tv.idx],
                                        tv.idx + 1,
                                        tv.hashes.len(),
                                        tv.info
                                    ));
                                    ui.horizontal(|ui| {
                                        if ui.button("◀ prev").clicked() {
                                            actions.push(Act::TexNav(-1));
                                        }
                                        if ui.button("next ▶").clicked() {
                                            actions.push(Act::TexNav(1));
                                        }
                                        if ui.button("Close").clicked() {
                                            actions.push(Act::TexClose);
                                        }
                                    });
                                });
                        }
                        if let Some(v) = &mut lua_view {
                            v.show(ctx);
                        }
                    });
                    if lua_view.as_ref().is_some_and(|v| !v.open) {
                        lua_view = None;
                    }

                    // ── Execute the frame's queued actions (keyboard + GUI, one implementation). ──
                    for act in std::mem::take(&mut actions) {
                        match act {
                            Act::LoadModelHash(hash, label) => {
                                let t0 = std::time::Instant::now();
                                match source_model_data(&mut w, &imported, hash) {
                                    Ok(md) => {
                                        let p = build_preview(
                                            &mut w, &mut scene, &mut world, hash, label.clone(), md,
                                            &preview, &placed, &index, &anim_sel, &lua_corpus,
                                        );
                                        cam_target = p.center;
                                        cam_dist = (p.radius * 2.4).clamp(1.5, 15000.0);
                                        sel_bone = None;
                                        preview = Some(p);
                                        // Loading a workbench template also sets it as the conform
                                        // donor (its container hosts the injected geometry).
                                        mod_donor = Some((hash, label.clone()));
                                        status = format!(
                                            "workbench template: {label} ({:.2}s)",
                                            t0.elapsed().as_secs_f32()
                                        );
                                    }
                                    Err(e) => status = format!("LOAD FAILED: {e}"),
                                }
                            }
                            Act::RetargetSetTarget(hash, label) => {
                                retarget_target = Some((hash, label.clone()));
                                if let Some(r) = &retarget {
                                    let (names, pos, parents) = target_bone_info(&mut w, hash);
                                    let nr = crate::retarget::Retarget::build_full(
                                        r.source_bones.clone(), r.source_pos.clone(),
                                        r.source_ibm.clone(), r.source_parents.clone(), names, pos, parents,
                                    );
                                    status = format!(
                                        "target {label}: {}/{} source bones mapped",
                                        nr.mapped_count(),
                                        nr.source_bones.len()
                                    );
                                    retarget = Some(nr);
                                } else {
                                    status = "no rigged import — use Import rigged model… first".into();
                                }
                            }
                            Act::RetargetRemap => match (&retarget, &retarget_target) {
                                (Some(r), Some((hash, _))) => {
                                    let (names, pos, parents) = target_bone_info(&mut w, *hash);
                                    let nr = crate::retarget::Retarget::build_full(
                                        r.source_bones.clone(), r.source_pos.clone(),
                                        r.source_ibm.clone(), r.source_parents.clone(), names, pos, parents,
                                    );
                                    status = format!(
                                        "remapped: {}/{} bones",
                                        nr.mapped_count(),
                                        nr.source_bones.len()
                                    );
                                    retarget = Some(nr);
                                }
                                _ => status = "need a rigged import AND a target skeleton".into(),
                            },
                            Act::RetargetApply => {
                                // RENDER-ACCURATE retarget: build the preview through the FAITHFUL
                                // char_skin path so the pedestal shows EXACTLY what the injected /
                                // shipped character looks like — re-posed onto the target skeleton
                                // with palette-relative skinning (expanded to global for the GPU).
                                // Preview == Export == in-game. The mesh is conformed into the target's
                                // bind space, so the target's own clips drive it directly (no cross
                                // retarget). The UI bone map is authoritative (fed to char_skin as
                                // overrides via `faithful_char_skin`).
                                let phash = preview.as_ref().map(|p| p.hash);
                                let ready = matches!(
                                    (&retarget, &retarget_target, &retarget_src_path),
                                    (Some(_), Some(_), Some(_))
                                );
                                let built = if let (true, Some(_)) = (ready, phash) {
                                    (|| -> Result<(mercs2_formats::char_skin::CharSkin, ModelData, String), String> {
                                        let src_path = retarget_src_path.clone().unwrap();
                                        let (thash, tl) = retarget_target.clone().unwrap();
                                        let rt = retarget.as_ref().unwrap();
                                        let mut paths = vec![opts.wadpath.clone()];
                                        paths.extend(opts.overlays.iter().cloned());
                                        let (cs, glb, _donor) =
                                            faithful_char_skin(&paths, thash, rt, &src_path)?;
                                        let target_rig = load_model_data(&mut w, thash)
                                            .map(|m| m.skin.rig)
                                            .unwrap_or_default();
                                        if target_rig.is_empty() {
                                            return Err(format!("target {tl} carries no skeleton"));
                                        }
                                        let mut md: ModelData =
                                            crate::import::char_skin_to_imported(&cs, &glb, target_rig)
                                                .into();
                                        // Texture the conformed mesh from the SOURCE file: it is the same
                                        // merged primitive stream, so its per-material draw groups + textures
                                        // + UVs map onto the re-posed verts. Re-imported (not read from the
                                        // in-app store, which a prior Apply overwrote with a bald mesh).
                                        if let Ok(src) = crate::import::import_model(&src_path) {
                                            let src: ModelData = src.into();
                                            if src.verts.len() == md.verts.len() && src.indices == md.indices {
                                                for (v, iv) in md.verts.iter_mut().zip(src.verts.iter()) {
                                                    v.uv = iv.uv;
                                                }
                                                md.draws = src.draws;
                                                md.textures = src.textures;
                                            }
                                        }
                                        Ok((cs, md, tl))
                                    })()
                                } else {
                                    Err("Apply needs a rigged import on the pedestal + a target skeleton".into())
                                };
                                match built {
                                    Ok((cs, md, tl)) => {
                                        let phash = phash.unwrap();
                                        let base = preview.as_ref().map(|p| p.label.clone()).unwrap_or_default();
                                        let base = base.split(" \u{25B8} ").next().unwrap_or(&base).to_string();
                                        let label = format!("{base} \u{25B8} {tl}");
                                        imported.insert(phash, md.clone());
                                        // Re-upload: the faithful mesh replaces the import's vertex buffer + bone count.
                                        scene.unload_model(phash);
                                        let mut p = build_preview(
                                            &mut w, &mut scene, &mut world, phash, label, md, &preview,
                                            &placed, &index, &anim_sel, &lua_corpus,
                                        );
                                        // The mesh is now in the TARGET skeleton's bind space, so the
                                        // target's clips drive it directly — clear any cross-retarget.
                                        p.retarget_source = None;
                                        cam_target = p.center;
                                        cam_dist = (p.radius * 2.4).clamp(0.5, 15000.0);
                                        // Prepend the validated full-body locomotion clips (idle/walk/run).
                                        const GOOD: [(u32, &str); 3] =
                                            [(0x24F8C8E6, "idle"), (0x53682784, "walk"), (0x867B166D, "run")];
                                        for (h, name) in GOOD.iter().rev() {
                                            if !p.clip_catalog.iter().any(|e| e.hash == *h) {
                                                p.clip_catalog.insert(
                                                    0,
                                                    ClipEntry {
                                                        hash: *h,
                                                        handles: Vec::new(),
                                                        label: (*name).into(),
                                                        name: Some((*name).into()),
                                                    },
                                                );
                                            }
                                        }
                                        // Auto-play ONLY the exact-transform path (container-dump proven).
                                        // The ESTIMATED transform nails the BIND pose (validated) but does
                                        // not capture the target bones' bind ROTATIONS, so LBS drifts under
                                        // animation — worst on complex rigs. Show the conformed BIND there;
                                        // the clip catalog stays so the user can still scrub if they want.
                                        let exact = cs.mode == mercs2_formats::char_skin::Mode::Exact;
                                        if exact {
                                            if let Some(ci) = p.clip_catalog.iter().position(|e| e.hash == 0x24F8C8E6) {
                                                p.cur_clip = Some(ci);
                                                p.anim_time = 0.0;
                                                p.playing = true;
                                                clip_loader.request(p.hash, &p.hier, p.clip_catalog[ci].hash);
                                                clip_seek = 0;
                                            }
                                        } else {
                                            p.playing = false;
                                            p.cur_clip = None;
                                        }
                                        preview = Some(p);
                                        status = format!(
                                            "retargeted onto {tl} (FAITHFUL): {} verts, palette {}/{} runs, {:?} transform — {}",
                                            cs.stats.verts,
                                            cs.palette_slots,
                                            cs.stats.range_count,
                                            cs.mode,
                                            if exact { "playing idle" } else { "showing BIND pose (estimated transform: animation needs a container dump; scrub a clip to preview it)" }
                                        );
                                    }
                                    Err(e) => status = format!("RETARGET FAILED: {e}"),
                                }
                            }
                            Act::ExportFaithfulCharacter => {
                                // FAITHFUL export: re-pose the imported rig onto the target skeleton
                                // with shipped-format skinning (palette-relative BLENDINDICES +
                                // INFO(56) range table, via mercs2_formats::char_skin — the inverse
                                // of the proven model_cubeize reader) and inject into the target donor.
                                let out = (|| -> Result<String, String> {
                                    let src_path = retarget_src_path
                                        .clone()
                                        .ok_or("import a rigged .glb first")?;
                                    let (thash, tlabel) = retarget_target
                                        .clone()
                                        .ok_or("pick a target skeleton first")?;
                                    let rt = retarget.as_ref().ok_or("pick a target skeleton first")?;
                                    // The SAME faithful path the preview renders — the UI bone map drives
                                    // it, so what you see on the pedestal is exactly what gets written.
                                    // The target container gives both the HIER skeleton and the injection
                                    // donor. Re-loads the source glb raw (exact f32 weights + node graph).
                                    let mut paths = vec![opts.wadpath.clone()];
                                    paths.extend(opts.overlays.iter().cloned());
                                    let (cs, glb, donor) = faithful_char_skin(&paths, thash, rt, &src_path)?;
                                    let report = mercs2_formats::char_skin::validate::validate(
                                        &cs,
                                        &glb.vjoints,
                                        &glb.vweights,
                                        &glb.indices,
                                    );
                                    // Host = the donor's LARGEST drawing group (the body) that fits.
                                    let host = mercs2_formats::model_inject::drawing_group_caps(&donor)
                                        .into_iter()
                                        .filter(|&(_, _, tricap)| cs.stats.tris as u32 <= tricap)
                                        .max_by_key(|&(_, vcap, _)| vcap)
                                        .map(|(ord, _, _)| ord)
                                        .ok_or_else(|| {
                                            format!(
                                                "no donor drawing group fits {} triangles — decimate",
                                                cs.stats.tris
                                            )
                                        })?;
                                    let Some(out_path) = rfd::FileDialog::new()
                                        .add_filter("model block", &["bin"])
                                        .set_title("Export faithful character block")
                                        .set_file_name(format!("{tlabel}_faithful.bin"))
                                        .save_file()
                                    else {
                                        return Ok("export cancelled".into());
                                    };
                                    let mesh = mercs2_formats::model_inject::ExternalMesh {
                                        positions: cs.pos.clone(),
                                        // CONFORMED normals (see CharSkin::nrm) - the
                                        // source field describes the pre-conform surface.
                                        normals: if cs.nrm.is_empty() {
                                            glb.normals.clone()
                                        } else {
                                            cs.nrm.clone()
                                        },
                                        uvs: glb.uvs.clone(),
                                        tris: glb.tris.clone(),
                                        joints: (0..cs.stats.verts)
                                            .map(|i| {
                                                [
                                                    cs.skin_bytes[i * 8],
                                                    cs.skin_bytes[i * 8 + 1],
                                                    cs.skin_bytes[i * 8 + 2],
                                                    cs.skin_bytes[i * 8 + 3],
                                                ]
                                            })
                                            .collect(),
                                        weights: (0..cs.stats.verts)
                                            .map(|i| {
                                                [
                                                    cs.skin_bytes[i * 8 + 4],
                                                    cs.skin_bytes[i * 8 + 5],
                                                    cs.skin_bytes[i * 8 + 6],
                                                    cs.skin_bytes[i * 8 + 7],
                                                ]
                                            })
                                            .collect(),
                                    };
                                    let new_name = mercs2_formats::hash::pandemic_hash_m2(&format!(
                                        "{tlabel}_faithful"
                                    ));
                                    let (block, _stats) =
                                        mercs2_formats::model_inject::inject_character_into_donor_block(
                                            &donor, &mesh, &cs.ranges, host, &[], new_name,
                                        )?;
                                    std::fs::write(&out_path, &block)
                                        .map_err(|e| format!("write {}: {e}", out_path.display()))?;
                                    let checks = report
                                        .checks
                                        .iter()
                                        .map(|c| format!("{}={:?}", c.title, c.status))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    Ok(format!(
                                        "EXPORTED {tlabel} — {} verts, palette {}/{} runs, host grp {host}, {:?} [{checks}] -> {}",
                                        cs.stats.verts,
                                        cs.palette_slots,
                                        cs.stats.range_count,
                                        cs.mode,
                                        out_path.display()
                                    ))
                                })();
                                status = match out {
                                    Ok(s) => s,
                                    Err(e) => format!("EXPORT FAILED: {e}"),
                                };
                            }
                            Act::RetargetManual(src, tgt) => {
                                if let Some(r) = &mut retarget {
                                    r.set_manual(src, tgt);
                                    status = match tgt.and_then(|i| r.target_bones.get(i)) {
                                        Some(name) => format!("{} → {name} (manual)", r.source_bones.get(src).cloned().unwrap_or_default()),
                                        None => format!("{} → unmapped", r.source_bones.get(src).cloned().unwrap_or_default()),
                                    };
                                }
                            }
                            Act::RetargetAlignPos => {
                                if let Some(r) = &mut retarget {
                                    let filled = r.align_by_position();
                                    status = if filled > 0 {
                                        format!("aligned by 3D position: filled {filled} unmapped bones (shown as 'fuzzy' — review)")
                                    } else {
                                        "align by position: nothing to fill (need positions on both sides + name anchors, or already fully mapped)".into()
                                    };
                                }
                            }
                            Act::LoadRow(vi) => {
                                let Some(&ri) = filtered.get(vi) else { continue };
                                let row = &index.rows(kind)[ri];
                                match kind {
                                    Kind::Model => {
                                        let t0 = std::time::Instant::now();
                                        match source_model_data(&mut w, &imported, row.hash) {
                                            Ok(md) => {
                                                let p = build_preview(
                                                    &mut w, &mut scene, &mut world, row.hash,
                                                    row.label(), md, &preview, &placed, &index,
                                                    &anim_sel, &lua_corpus,
                                                );
                                                cam_target = p.center;
                                                cam_dist = (p.radius * 2.4).clamp(1.5, 15000.0);
                                                status = format!(
                                                    "loaded {} [blk {}{}] in {:.2}s",
                                                    p.label,
                                                    row.block,
                                                    if row.src > 0 { " overlay" } else { "" },
                                                    t0.elapsed().as_secs_f32()
                                                );
                                                sel_bone = None; // bone indices belong to the old rig
                                                preview = Some(p);
                                            }
                                            Err(e) => status = format!("LOAD FAILED: {e}"),
                                        }
                                    }
                                    Kind::Texture => {
                                        let mut tv = TexView {
                                            hashes: vec![row.hash],
                                            labels: vec![row.label()],
                                            idx: 0,
                                            info: String::new(),
                                        };
                                        bind_tex_plate(&mut w, &mut scene, &mut tv);
                                        tex_view = Some(tv);
                                    }
                                }
                            }
                            a @ (Act::Tier(_) | Act::TierNext) => {
                                // The model is uploaded WHOLE, so switching LOD rung is free: it just
                                // changes `view_state` on this entity's render state. No rebuild, no
                                // re-upload, and no effect on other placed instances of the same hash
                                // (which the old per-hash `hidden_draws` path could not avoid).
                                let want = match a {
                                    Act::Tier(b) => preview.as_ref().map(|_| b),
                                    _ => preview.as_ref().and_then(|p| {
                                        (p.tiers.len() > 1).then(|| {
                                            let i =
                                                p.tiers.iter().position(|&b| b == p.tier).unwrap_or(0);
                                            p.tiers[(i + 1) % p.tiers.len()]
                                        })
                                    }),
                                };
                                let Some(bit) = want else {
                                    if preview.is_some() {
                                        status = "single-tier model (no other LOD rung)".into();
                                    }
                                    continue;
                                };
                                if let Some(p) = &mut preview {
                                    p.tier = bit;
                                    let node_enable = p
                                        .machine
                                        .as_ref()
                                        .map(|sm| {
                                            mercs2_formats::orchestrator::machine_node_enable(
                                                sm,
                                                &p.hier_nodes,
                                                &p.node_state,
                                            )
                                        })
                                        .unwrap_or_default();
                                    let rs = mercs2_engine::render_state::RenderState {
                                        lod: bit.trailing_zeros() as u8,
                                        view_state: bit,
                                        node_enable,
                                    };
                                    for (gi, d) in p.draws.iter().enumerate() {
                                        if rs.segment_visible(d.lod_mask, d.node) {
                                            p.hidden.remove(&gi);
                                        } else {
                                            p.hidden.insert(gi);
                                        }
                                    }
                                    p.node_enable = rs.node_enable.clone();
                                    scene.set_entity_render_state(p.entity, rs);
                                    let ti = p.tiers.iter().position(|&b| b == bit).unwrap_or(0);
                                    status = format!(
                                        "LOD rung {} (0x{:02X}, {}/{}) — {} of {} groups drawn",
                                        bit.trailing_zeros(),
                                        bit,
                                        ti + 1,
                                        p.tiers.len().max(1),
                                        p.draws.len() - p.hidden.len(),
                                        p.draws.len()
                                    );
                                }
                            }
                            a @ (Act::ClipSel(_) | Act::ClipNav(_)) => {
                                clip_seek = 0; // user chose a clip — stop auto-seeking
                                if let Some(p) = &mut preview {
                                    if !p.clip_catalog.is_empty() {
                                        let n = p.clip_catalog.len();
                                        let ci = match (a, p.cur_clip) {
                                            (Act::ClipSel(i), _) => i.min(n - 1),
                                            (Act::ClipNav(d), Some(i)) => {
                                                (i as i32 + d).rem_euclid(n as i32) as usize
                                            }
                                            (Act::ClipNav(d), None) => {
                                                if d > 0 { 0 } else { n - 1 }
                                            }
                                            _ => unreachable!(),
                                        };
                                        p.cur_clip = Some(ci);
                                        p.anim_time = 0.0;
                                        p.playing = true;
                                        let hash = p.clip_catalog[ci].hash;
                                        match p.clip_cache.get(&hash) {
                                            // Cold clip: decode on the loader thread; the anim
                                            // update plays it as soon as the cache fills.
                                            None => {
                                                clip_loader.request(p.hash, &p.hier, hash);
                                                status = format!(
                                                    "loading clip {}…",
                                                    p.clip_catalog[ci].label
                                                );
                                            }
                                            Some(None) => {
                                                status = format!(
                                                    "clip {} is not in any animgroup bound to this rig",
                                                    p.clip_catalog[ci].label
                                                );
                                            }
                                            Some(Some(_)) => {}
                                        }
                                    }
                                }
                            }
                            Act::ClipStop => {
                                if let Some(p) = &mut preview {
                                    p.cur_clip = None;
                                    p.playing = false;
                                    let _ = world
                                        .insert_one(p.entity, SkinPalette { mats: p.bind.clone() });
                                }
                            }
                            Act::PlayPause => {
                                if let Some(p) = &mut preview {
                                    if p.cur_clip.is_some() {
                                        p.playing = !p.playing;
                                    }
                                }
                            }
                            Act::GroupToggle(gi) => {
                                if let Some(p) = &mut preview {
                                    if gi < p.draws.len() {
                                        let hide = !p.hidden.contains(&gi);
                                        if hide {
                                            p.hidden.insert(gi);
                                        } else {
                                            p.hidden.remove(&gi);
                                        }
                                        scene.set_draw_hidden(p.hash, gi, hide);
                                    }
                                }
                            }
                            Act::Place => {
                                if let Some(p) = &preview {
                                    let pos = cam_target;
                                    let e = world.spawn((
                                        Transform {
                                            translation: pos,
                                            rotation: Quat::IDENTITY,
                                            scale: Vec3::ONE,
                                        },
                                        ModelRef { model: p.hash },
                                        AnimState::default(),
                                        SkinPalette { mats: p.bind.clone() },
                                    ));
                                    // Inherit the preview's live render state (its LOD rung and the
                                    // node states you picked), not a fresh default.
                                    let rs = scene
                                        .entity_render_state(p.entity)
                                        .cloned()
                                        .unwrap_or_else(|| default_render_state(&mut w, p.hash));
                                    scene.set_entity_render_state(e, rs);
                                    placed.push(Placed {
                                        hash: p.hash,
                                        label: p.label.clone(),
                                        entity: e,
                                        pos,
                                        yaw: 0.0,
                                        scale: 1.0,
                                    });
                                    sel_placed = Some(placed.len() - 1);
                                    status =
                                        format!("placed {} ({} in sandbox)", p.label, placed.len());
                                }
                            }
                            Act::Merge => {
                                if placed.is_empty() {
                                    status = "merge: place models first".into();
                                    continue;
                                }
                                match merge_placed(&mut w, &imported, &placed) {
                                    Ok(md) => {
                                        merge_seq += 1;
                                        let label = format!("merged_{merge_seq}");
                                        let hash = mercs2_formats::hash::pandemic_hash_m2(&label);
                                        imported.insert(hash, md.clone());
                                        scene.unload_model(hash);
                                        let p = build_preview(
                                            &mut w, &mut scene, &mut world, hash, label, md,
                                            &preview, &placed, &index, &anim_sel, &lua_corpus,
                                        );
                                        cam_target = p.center;
                                        cam_dist = (p.radius * 2.4).clamp(0.5, 15000.0);
                                        status = format!(
                                            "merged {} instance(s) -> {} ({} verts, {} groups)",
                                            placed.len(),
                                            p.label,
                                            p.verts,
                                            p.draws.len()
                                        );
                                        sel_bone = None;
                                        preview = Some(p);
                                    }
                                    Err(e) => status = format!("MERGE FAILED: {e}"),
                                }
                            }
                            Act::Export => {
                                if let Some(p) = &preview {
                                    if imported.contains_key(&p.hash) {
                                        // Import/merge → the fast OBJ writer (no clip decode); safe inline.
                                        status = match export_preview(&mut w, &opts.wadpath, &opts.overlays, &imported, &index, p) {
                                            Ok(dir) => format!("exported {} -> {dir}", p.label),
                                            Err(e) => format!("EXPORT FAILED: {e}"),
                                        };
                                    } else if exporter.is_none() {
                                        // WAD asset → the heavy rigged bundle (full clip decode) runs on a
                                        // worker so the UI stays live; the modal reports until it lands.
                                        status = format!("exporting {}…", p.label);
                                        exporter = Some(export_bundle_in_background(
                                            opts.wadpath.clone(),
                                            opts.overlays.clone(),
                                            p.hash,
                                            p.label.clone(),
                                            index.clone(),
                                            std::path::PathBuf::from("workshop_export"),
                                        ));
                                    }
                                }
                            }
                            Act::ExportHash(hash, label) => {
                                status = match source_model_data(&mut w, &imported, hash)
                                    .and_then(|md| export_model_data(&md, &label))
                                {
                                    Ok(dir) => format!("exported {label} -> {dir}"),
                                    Err(e) => format!("EXPORT FAILED: {e}"),
                                };
                            }
                            Act::PlaceHash(hash, label) => {
                                if !scene.has_model(hash) {
                                    if let Err(e) = load_gpu_only(&mut w, &mut scene, hash) {
                                        status = format!("PLACE FAILED: {e}");
                                        continue;
                                    }
                                }
                                let pos = cam_target;
                                let bones = scene.model_bone_count(hash).max(1);
                                let e = world.spawn((
                                    Transform {
                                        translation: pos,
                                        rotation: Quat::IDENTITY,
                                        scale: Vec3::ONE,
                                    },
                                    ModelRef { model: hash },
                                    AnimState::default(),
                                    SkinPalette { mats: vec![IDENTITY; bones] },
                                ));
                                let rs = default_render_state(&mut w, hash);
                                scene.set_entity_render_state(e, rs);
                                placed.push(Placed { hash, label: label.clone(), entity: e, pos, yaw: 0.0, scale: 1.0 });
                                sel_placed = Some(placed.len() - 1);
                                status = format!("placed {label} ({} in sandbox)", placed.len());
                            }
                            Act::DuplicatePlaced(i) => {
                                if let Some(src) = placed.get(i) {
                                    let pos = src.pos + Vec3::new(0.5, 0.0, 0.5);
                                    let (hash, label, yaw, scale, src_entity) =
                                        (src.hash, src.label.clone(), src.yaw, src.scale, src.entity);
                                    let bones = scene.model_bone_count(hash).max(1);
                                    let e = world.spawn((
                                        Transform {
                                            translation: pos,
                                            rotation: Quat::from_rotation_y(yaw),
                                            scale: Vec3::splat(scale),
                                        },
                                        ModelRef { model: hash },
                                        AnimState::default(),
                                        SkinPalette { mats: vec![IDENTITY; bones] },
                                    ));
                                    // Per-entity state: the duplicate can now diverge from its source.
                                    let rs = scene
                                        .entity_render_state(src_entity)
                                        .cloned()
                                        .unwrap_or_else(|| default_render_state(&mut w, hash));
                                    scene.set_entity_render_state(e, rs);
                                    placed.push(Placed { hash, label, entity: e, pos, yaw, scale });
                                    sel_placed = Some(placed.len() - 1);
                                }
                            }
                            Act::SnapPlaced(i) => {
                                if let Some(pl) = placed.get_mut(i) {
                                    pl.pos = cam_target;
                                    let _ = world.insert_one(
                                        pl.entity,
                                        Transform {
                                            translation: pl.pos,
                                            rotation: Quat::from_rotation_y(pl.yaw),
                                            scale: Vec3::splat(pl.scale),
                                        },
                                    );
                                }
                            }
                            Act::IsolateGroup(gi) => {
                                if let Some(p) = &mut preview {
                                    for g in 0..p.draws.len() {
                                        let hide = g != gi;
                                        if hide {
                                            p.hidden.insert(g);
                                        } else {
                                            p.hidden.remove(&g);
                                        }
                                        scene.set_draw_hidden(p.hash, g, hide);
                                    }
                                    status = format!("isolated draw group {gi}");
                                }
                            }
                            Act::ShowAllGroups => {
                                if let Some(p) = &mut preview {
                                    for g in 0..p.draws.len() {
                                        scene.set_draw_hidden(p.hash, g, false);
                                    }
                                    p.hidden.clear();
                                }
                            }
                            Act::SetHealth(h) => {
                                if let Some(p) = &mut preview {
                                    p.health = h.clamp(0.0, 1.0);
                                    if let Some(sm) = &p.machine {
                                        // Run the machine from health: pick each node's state, then
                                        // node-enable → render state. This IS the object's damage axis.
                                        p.node_state = mercs2_formats::orchestrator::node_states_for_health(
                                            sm, p.health, 0.99,
                                        );
                                        let node_enable =
                                            mercs2_formats::orchestrator::machine_node_enable(
                                                sm,
                                                &p.hier_nodes,
                                                &p.node_state,
                                            );
                                        let rs = mercs2_engine::render_state::RenderState {
                                            lod: p.tier.trailing_zeros() as u8,
                                            view_state: p.tier,
                                            node_enable,
                                        };
                                        for (gi, d) in p.draws.iter().enumerate() {
                                            if rs.segment_visible(d.lod_mask, d.node) {
                                                p.hidden.remove(&gi);
                                            } else {
                                                p.hidden.insert(gi);
                                            }
                                        }
                                        p.node_enable = rs.node_enable.clone();
                                        scene.set_entity_render_state(p.entity, rs);
                                        let drawn = p.draws.len() - p.hidden.len();
                                        status = format!(
                                            "health {:.0}% — {drawn}/{} groups drawn",
                                            p.health * 100.0,
                                            p.draws.len()
                                        );
                                    }
                                }
                            }
                            Act::ToggleRuin => {
                                if let Some(p) = &mut preview {
                                    p.hide_ruin = !p.hide_ruin;
                                    let mut n = 0usize;
                                    for (gi, d) in p.draws.iter().enumerate() {
                                        let ruin = d
                                            .diffuse
                                            .map(|h| name_or_hash(&index, h).to_lowercase().contains("ruin"))
                                            .unwrap_or(false);
                                        if ruin {
                                            n += 1;
                                            if p.hide_ruin {
                                                p.hidden.insert(gi);
                                                scene.set_draw_hidden(p.hash, gi, true);
                                            } else {
                                                p.hidden.remove(&gi);
                                                scene.set_draw_hidden(p.hash, gi, false);
                                            }
                                        }
                                    }
                                    status = format!(
                                        "{} {n} *_ruin* sub-strip(s)",
                                        if p.hide_ruin { "hid" } else { "restored" }
                                    );
                                }
                            }
                            Act::NodeState(ni, si) => {
                                if let Some(p) = &mut preview {
                                    if let Some(sm) = &p.machine {
                                        if let Some(slot) = p.node_state.get_mut(ni) {
                                            *slot = si;
                                        }
                                        // Re-execute the machine into a node-enable table and hand it
                                        // to the entity's render state. Node-keyed, like the engine.
                                        let node_enable =
                                            mercs2_formats::orchestrator::machine_node_enable(
                                                sm,
                                                &p.hier_nodes,
                                                &p.node_state,
                                            );
                                        let rs = mercs2_engine::render_state::RenderState {
                                            lod: p.tier.trailing_zeros() as u8,
                                            view_state: p.tier,
                                            node_enable,
                                        };
                                        for (gi, d) in p.draws.iter().enumerate() {
                                            if rs.segment_visible(d.lod_mask, d.node) {
                                                p.hidden.remove(&gi);
                                            } else {
                                                p.hidden.insert(gi);
                                            }
                                        }
                                        p.node_enable = rs.node_enable.clone();
                                        scene.set_entity_render_state(p.entity, rs);
                                        let sname = sm
                                            .nodes
                                            .get(ni)
                                            .and_then(|n| n.states.get(si))
                                            .map(|s| name_or_hash(&index, s.name_hash))
                                            .unwrap_or_default();
                                        status = format!("node {ni} -> state {sname}");
                                    }
                                }
                            }
                            Act::ImportModel => {
                                if !names_pending {
                                    if let Some(path) = rfd::FileDialog::new()
                                        .add_filter("model", &["glb", "gltf", "obj"])
                                        .set_title("Import model — .glb / .gltf / .obj")
                                        .pick_file()
                                    {
                                        status = import_file(
                                            &path, &mut w, &mut scene, &mut world, &mut imported,
                                            &mut preview, &mut cam_target, &mut cam_dist,
                                            &mut retarget, &retarget_target, &mut retarget_src_path,
                                            &mut wb, &mut sel_bone,
                                            &placed, &index, &anim_sel, &lua_corpus,
                                        );
                                    }
                                }
                            }
                            Act::ClearImport => {
                                // Unload the current import from the GPU + the in-app store and reset
                                // every retarget field, so the next import starts from a clean slate.
                                if let Some(p) = preview.take() {
                                    scene.unload_model(p.hash);
                                    imported.remove(&p.hash);
                                }
                                retarget = None;
                                retarget_target = None;
                                retarget_src_path = None;
                                sel_bone = None;
                                show_target_picker = false;
                                status = "cleared import — drop a .glb or use Import to start over".into();
                            }
                            Act::SaveScene => {
                                let f = SceneFile {
                                    items: placed
                                        .iter()
                                        .map(|p| SceneItem {
                                            hash: p.hash,
                                            name: index.names.get(&p.hash).cloned(),
                                            pos: p.pos.into(),
                                            yaw: p.yaw,
                                            scale: p.scale,
                                        })
                                        .collect(),
                                };
                                status = match serde_json::to_string_pretty(&f)
                                    .map_err(|e| e.to_string())
                                    .and_then(|s| {
                                        std::fs::write(SCENE_FILE, s).map_err(|e| e.to_string())
                                    }) {
                                    Ok(()) => {
                                        format!("saved {} item(s) -> {SCENE_FILE}", placed.len())
                                    }
                                    Err(e) => format!("SAVE FAILED: {e}"),
                                };
                            }
                            Act::LoadScene => {
                                match std::fs::read_to_string(SCENE_FILE)
                                    .map_err(|e| e.to_string())
                                    .and_then(|s| {
                                        serde_json::from_str::<SceneFile>(&s)
                                            .map_err(|e| e.to_string())
                                    }) {
                                    Ok(f) => {
                                        for pl in placed.drain(..) {
                                            world.despawn(pl.entity).ok();
                                            scene.forget_entity(pl.entity);
                                        }
                                        let mut ok = 0usize;
                                        for it in f.items {
                                            let label = it
                                                .name
                                                .clone()
                                                .unwrap_or_else(|| format!("0x{:08X}", it.hash));
                                            if !scene.has_model(it.hash) {
                                                if let Err(e) =
                                                    load_gpu_only(&mut w, &mut scene, it.hash)
                                                {
                                                    eprintln!("[load] {label}: {e}");
                                                    continue;
                                                }
                                            }
                                            let pos = Vec3::from(it.pos);
                                            let bones = scene.model_bone_count(it.hash).max(1);
                                            let e = world.spawn((
                                                Transform {
                                                    translation: pos,
                                                    rotation: Quat::from_rotation_y(it.yaw),
                                                    scale: Vec3::splat(it.scale),
                                                },
                                                ModelRef { model: it.hash },
                                                AnimState::default(),
                                                SkinPalette { mats: vec![IDENTITY; bones] },
                                            ));
                                            let rs = default_render_state(&mut w, it.hash);
                                            scene.set_entity_render_state(e, rs);
                                            placed.push(Placed {
                                                hash: it.hash,
                                                label,
                                                entity: e,
                                                pos,
                                                yaw: it.yaw,
                                                scale: it.scale,
                                            });
                                            ok += 1;
                                        }
                                        sel_placed = (!placed.is_empty()).then_some(0);
                                        status = format!("loaded {ok} item(s) from {SCENE_FILE}");
                                    }
                                    Err(e) => status = format!("LOAD FAILED: {e}"),
                                }
                            }
                            Act::ClearSandbox => {
                                for pl in placed.drain(..) {
                                    world.despawn(pl.entity).ok();
                                    scene.forget_entity(pl.entity);
                                }
                                sel_placed = None;
                                status = "sandbox cleared".into();
                            }
                            Act::RemovePlaced(i) => {
                                if i < placed.len() {
                                    let pl = placed.remove(i);
                                    world.despawn(pl.entity).ok();
                                    scene.forget_entity(pl.entity);
                                    sel_placed =
                                        (!placed.is_empty()).then(|| i.min(placed.len() - 1));
                                }
                            }
                            Act::SyncPlaced(i) => {
                                if let Some(pl) = placed.get(i) {
                                    let _ = world.insert_one(
                                        pl.entity,
                                        Transform {
                                            translation: pl.pos,
                                            rotation: Quat::from_rotation_y(pl.yaw),
                                            scale: Vec3::splat(pl.scale),
                                        },
                                    );
                                }
                            }
                            Act::TexOfPreview => {
                                if let Some(p) = &preview {
                                    if !p.tex_hashes.is_empty() {
                                        let mut tv = TexView {
                                            hashes: p.tex_hashes.clone(),
                                            labels: p
                                                .tex_hashes
                                                .iter()
                                                .map(|h| name_or_hash(&index, *h))
                                                .collect(),
                                            idx: 0,
                                            info: String::new(),
                                        };
                                        bind_tex_plate(&mut w, &mut scene, &mut tv);
                                        tex_view = Some(tv);
                                    }
                                }
                            }
                            Act::TexNav(d) => {
                                if let Some(tv) = &mut tex_view {
                                    let n = tv.hashes.len() as i32;
                                    tv.idx = (tv.idx as i32 + d).rem_euclid(n) as usize;
                                    bind_tex_plate(&mut w, &mut scene, tv);
                                }
                            }
                            Act::LuaOpen(path) => {
                                match lua_corpus.iter().find(|(p, _, _)| *p == path) {
                                    Some((_, content, _)) => {
                                        let needle = preview
                                            .as_ref()
                                            .map(|p| p.lua_needle.clone())
                                            .unwrap_or_default();
                                        lua_view = Some(crate::luaview::LuaView::new(
                                            &path, content, &needle,
                                        ));
                                    }
                                    None => status = format!("script not in corpus: {path}"),
                                }
                            }
                            Act::ModDonor(h, l) => {
                                status = format!("mod donor set: {l} (0x{h:08X})");
                                mod_donor = Some((h, l));
                            }
                            Act::ModAdd(name) => {
                                let Some(p) = &preview else { continue };
                                let Some(md) = imported.get(&p.hash) else {
                                    status = "the preview is not an imported model".into();
                                    continue;
                                };
                                let Some((donor, donor_label)) = mod_donor.clone() else {
                                    continue;
                                };
                                let hash = mercs2_formats::hash::pandemic_hash_m2(&name);
                                if mod_items.iter().any(|it| it.hash == hash) {
                                    status = format!("mod project already has 0x{hash:08X}");
                                    continue;
                                }
                                let mesh =
                                    external_mesh_transformed(md, conform_scale, conform_t, conform_r);
                                let (nv, nt) = (mesh.positions.len(), mesh.tris.len());
                                mod_items.push(crate::publish::NewModelItem {
                                    name: name.clone(),
                                    hash,
                                    donor,
                                    donor_label,
                                    target_group: mod_group,
                                    flip: conform_flip,
                                    mesh,
                                    diffuse: None,
                                    specular: None,
                                    normal: None,
                                });
                                status = format!(
                                    "queued new asset {name} (0x{hash:08X}) — {nv} verts, {nt} tris"
                                );
                            }
                            Act::ModRemove(i) => {
                                if i < mod_items.len() {
                                    mod_items.remove(i);
                                }
                            }
                            Act::Publish => {
                                if mod_items.is_empty() || publisher.is_some() {
                                    continue;
                                }
                                // If a prior publish of the same path is loaded as an overlay,
                                // drop it first — its open file handle would block the write.
                                let out_l = mod_out.to_lowercase();
                                if let Some(i) =
                                    w.labels.iter().position(|l| l.to_lowercase() == out_l)
                                {
                                    if i == 0 {
                                        status = "output path equals the BASE wad — refusing".into();
                                        continue;
                                    }
                                    w.wads.remove(i);
                                    w.labels.remove(i);
                                    let names = std::mem::take(&mut index.names);
                                    index = AssetIndex::build(&w.wads, names);
                                    filtered = refilter(&index, kind, &filter);
                                    inventory_dirty = true;
                                    sel = sel.min(filtered.len().saturating_sub(1));
                                }
                                status = format!(
                                    "publishing {} asset(s) to {mod_out}…",
                                    mod_items.len()
                                );
                                publisher = Some(crate::publish::publish_in_background(
                                    w.labels.clone(),
                                    mod_items.clone(),
                                    std::path::PathBuf::from(mod_out.clone()),
                                ));
                            }
                            Act::ConformAutofit => {
                                let Some((dh, _)) = mod_donor.as_ref().map(|(h, l)| (*h, l.clone()))
                                else {
                                    status = "auto-fit: set a donor first".into();
                                    continue;
                                };
                                let Some(imp) = preview.as_ref().and_then(|p| imported.get(&p.hash))
                                else {
                                    status = "auto-fit: the pedestal is not an imported model".into();
                                    continue;
                                };
                                match load_model_data(&mut w, dh) {
                                    Ok(donor_md) => {
                                        let (s, t) = conform_autofit(&donor_md, imp);
                                        conform_scale = s;
                                        conform_t = t;
                                        conform_r = [0.0; 3];
                                        status = format!(
                                            "auto-fit: scale {:.3}, pos [{:.2}, {:.2}, {:.2}]",
                                            s, t[0], t[1], t[2]
                                        );
                                    }
                                    Err(e) => status = format!("auto-fit: donor load failed: {e}"),
                                }
                            }
                            Act::LoadDonorRef => {
                                let Some((dh, dl)) = mod_donor.clone() else {
                                    status = "donor ref: set a donor first".into();
                                    continue;
                                };
                                if !scene.has_model(dh) {
                                    if let Err(e) = load_gpu_only(&mut w, &mut scene, dh) {
                                        status = format!("donor ref: {e}");
                                        continue;
                                    }
                                }
                                let bones = scene.model_bone_count(dh).max(1);
                                let e = world.spawn((
                                    Transform {
                                        translation: Vec3::ZERO,
                                        rotation: Quat::IDENTITY,
                                        scale: Vec3::ONE,
                                    },
                                    ModelRef { model: dh },
                                    AnimState::default(),
                                    SkinPalette { mats: vec![IDENTITY; bones] },
                                ));
                                let rs = default_render_state(&mut w, dh);
                                scene.set_entity_render_state(e, rs);
                                placed.push(Placed {
                                    hash: dh,
                                    label: format!("ref:{dl}"),
                                    entity: e,
                                    pos: Vec3::ZERO,
                                    yaw: 0.0,
                                    scale: 1.0,
                                });
                                status = format!("donor reference placed at origin: {dl}");
                            }
                            Act::TexClose => {
                                tex_view = None;
                                // Restore the shell-plate backdrop the plate view overwrote.
                                if let Some(td) = &shell_plate {
                                    scene.set_loading_art(td);
                                }
                            }
                        }
                    }

                    // ── Bone highlight: glow cards at the hovered/pinned bone's POSED position
                    // (WorldBind · Skin, so it tracks the playing animation). Cleared when
                    // nothing is hovered/pinned. ──
                    {
                        let mut cards: Vec<mercs2_engine::particles::GlowCard> = Vec::new();
                        if let Some(p) = &preview {
                            let pal = world.get::<&SkinPalette>(p.entity).ok();
                            let mut push = |i: usize, color: [f32; 4]| {
                                let Some(b) = p.rig.get(i) else { return };
                                let m = match pal.as_ref().and_then(|pl| pl.mats.get(i)) {
                                    Some(sm) => {
                                        mercs2_formats::skeleton::mat4_mul(&b.world_bind, sm)
                                    }
                                    None => b.world_bind,
                                };
                                cards.push(mercs2_engine::particles::GlowCard {
                                    pos: [m[3][0], m[3][1], m[3][2]],
                                    size: (p.radius * 0.08).clamp(0.03, 0.8),
                                    color,
                                });
                            };
                            if let Some(i) = hovered_bone {
                                push(i, [1.0, 0.82, 0.30, 0.9]); // gold: transient hover
                            }
                            if let Some(i) = sel_bone {
                                if hovered_bone != Some(i) {
                                    push(i, [0.35, 0.9, 1.0, 0.9]); // cyan: pinned
                                }
                            }
                            // Skeleton workbench: also MARK the mesh the hovered bone drives — a scatter of
                            // amber dots on a sample of the vertices weighted to it (posed, so they track the
                            // clip). Shows exactly what geometry a bone controls, for confident manual mapping.
                            if let (Workbench::Skeleton, Some((bone, is_source)), Some(r)) =
                                (wb, hover_skel, retarget.as_ref())
                            {
                                // Resolve the hovered bone into the PREVIEW's current space: before Apply the
                                // preview is the raw import (SOURCE joints/positions); after Apply it's the
                                // retargeted mesh (TARGET joints, posed by the grafted rig). `scatter_bone` is
                                // the joint index the preview verts actually use.
                                let retargeted = !p.rig.is_empty();
                                let (scatter_bone, glow_pos): (Option<usize>, Option<[f32; 3]>) = if retargeted {
                                    let t = if is_source {
                                        r.map.get(bone).and_then(|m| m.target_index)
                                    } else {
                                        Some(bone)
                                    };
                                    let gp = t.and_then(|ti| {
                                        p.rig.get(ti).map(|b| {
                                            let m = match pal.as_ref().and_then(|pl| pl.mats.get(ti)) {
                                                Some(sm) => mercs2_formats::skeleton::mat4_mul(&b.world_bind, sm),
                                                None => b.world_bind,
                                            };
                                            [m[3][0], m[3][1], m[3][2]]
                                        })
                                    });
                                    (t, gp)
                                } else if is_source {
                                    (Some(bone), r.source_pos.get(bone).copied())
                                } else {
                                    (None, None) // donor bone but no retargeted mesh yet — nothing to show
                                };
                                if let Some(gp) = glow_pos {
                                    // Layered marker: a big soft halo + a hot core so the joint
                                    // reads at a glance even through the ghosted mesh.
                                    cards.push(mercs2_engine::particles::GlowCard {
                                        pos: gp,
                                        size: (p.radius * 0.24).clamp(0.08, 1.2),
                                        color: [1.0, 0.85, 0.35, 0.35], // gold halo
                                    });
                                    cards.push(mercs2_engine::particles::GlowCard {
                                        pos: gp,
                                        size: (p.radius * 0.11).clamp(0.04, 0.5),
                                        color: [1.0, 0.95, 0.75, 1.0], // hot white-gold core
                                    });
                                }
                                if let Some(sb) = scatter_bone {
                                    if let Some(md) = imported.get(&p.hash) {
                                        let step = (md.verts.len() / 700).max(1);
                                        for v in md.verts.iter().step_by(step) {
                                            let drive: f32 = (0..4)
                                                .filter(|&k| v.joints[k] as usize == sb)
                                                .map(|k| v.weights[k] as f32 / 255.0)
                                                .sum();
                                            if drive < 0.12 {
                                                continue;
                                            }
                                            let (mut wp, mut wsum) = (glam::Vec3::ZERO, 0.0f32);
                                            for k in 0..4 {
                                                let w = v.weights[k] as f32 / 255.0;
                                                if w <= 0.0 {
                                                    continue;
                                                }
                                                let m = pal
                                                    .as_ref()
                                                    .and_then(|pl| pl.mats.get(v.joints[k] as usize))
                                                    .map(|m| glam::Mat4::from_cols_array_2d(m))
                                                    .unwrap_or(glam::Mat4::IDENTITY);
                                                wp += w * m.transform_point3(glam::Vec3::from(v.pos));
                                                wsum += w;
                                            }
                                            if wsum > 1e-6 {
                                                wp /= wsum;
                                            }
                                            // Brighter where the bone drives the vert harder.
                                            let a = 0.45 + 0.45 * drive.min(1.0);
                                            cards.push(mercs2_engine::particles::GlowCard {
                                                pos: wp.to_array(),
                                                size: (p.radius * 0.035).clamp(0.012, 0.2),
                                                color: [1.0, 0.5, 0.12, a], // amber mesh scatter
                                            });
                                        }
                                    }
                                }
                            }
                            // Workbench: every node as a spatial anchor (after the hover/pin closure
                            // releases its &mut cards). Colour by role heuristic — translated-away
                            // nodes = functional attach points (rotor/skid/seat/tail/hardpoint);
                            // nodes at the origin = structural/break-piece parents — so the user can
                            // map imported geometry onto them by sight.
                            if show_nodes && wb == Workbench::Mods {
                                // Posed world position of every node.
                                let node_pos: Vec<[f32; 3]> = p
                                    .rig
                                    .iter()
                                    .enumerate()
                                    .map(|(i, b)| {
                                        let m = match pal.as_ref().and_then(|pl| pl.mats.get(i)) {
                                            Some(sm) => mercs2_formats::skeleton::mat4_mul(&b.world_bind, sm),
                                            None => b.world_bind,
                                        };
                                        [m[3][0], m[3][1], m[3][2]]
                                    })
                                    .collect();
                                // INTERLINK: dotted segments from each node to its parent so the
                                // HIER hierarchy is legible (which nodes hang off which).
                                for (i, b) in p.rig.iter().enumerate() {
                                    if b.parent < 0 {
                                        continue;
                                    }
                                    let Some(a) = node_pos.get(b.parent as usize).copied() else {
                                        continue;
                                    };
                                    let c = node_pos[i];
                                    let d = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                                    if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 0.01 {
                                        continue; // coincident with parent — no visible link
                                    }
                                    for s in 1..5 {
                                        let f = s as f32 / 5.0;
                                        cards.push(mercs2_engine::particles::GlowCard {
                                            pos: [a[0] + d[0] * f, a[1] + d[1] * f, a[2] + d[2] * f],
                                            size: (p.radius * 0.012).clamp(0.008, 0.12),
                                            color: [0.25, 0.75, 0.85, 0.55], // teal link
                                        });
                                    }
                                }
                                // NODES: green = positioned attach point (moved off the model
                                // origin: rotor/skid/seat/tail/hardpoint), grey = origin/structural.
                                for (i, b) in p.rig.iter().enumerate() {
                                    let t = [b.world_bind[3][0], b.world_bind[3][1], b.world_bind[3][2]];
                                    let off_origin = t[0].abs() + t[1].abs() + t[2].abs() > 0.05;
                                    let color = if off_origin {
                                        [0.30, 1.0, 0.45, 0.9]
                                    } else {
                                        [0.6, 0.6, 0.7, 0.5]
                                    };
                                    cards.push(mercs2_engine::particles::GlowCard {
                                        pos: node_pos[i],
                                        size: (p.radius * 0.04).clamp(0.02, 0.4),
                                        color,
                                    });
                                }
                            }

                            // ---- INTERACTION HARDPOINTS: big, unmistakable, and at their EDITED
                            // position. These are what the player touches (the seat is the ENTRY
                            // point) -- so they must sit ON the new model, not where the donor's were.
                            if show_hardpoints && wb == Workbench::Mods {
                                for (i, b) in p.rig.iter().enumerate() {
                                    let Some(n) = hp_names.get(&b.name_hash) else { continue };
                                    if !n.starts_with("hp_") {
                                        continue;
                                    }
                                    let base = [b.world_bind[3][0], b.world_bind[3][1], b.world_bind[3][2]];
                                    let pos = hp_edits.get(&i).copied().unwrap_or(base);
                                    let is_seat = n.contains("seat");
                                    let color = if hp_selected == Some(i) {
                                        [1.0, 0.25, 0.9, 1.0]      // magenta = selected
                                    } else if is_seat {
                                        [1.0, 0.85, 0.15, 0.95]    // amber = the ENTRY point
                                    } else {
                                        [0.2, 0.6, 1.0, 0.8]       // blue = other hardpoint
                                    };
                                    cards.push(mercs2_engine::particles::GlowCard {
                                        pos,
                                        size: (p.radius * (if is_seat { 0.075 } else { 0.055 })).clamp(0.05, 0.7),
                                        color,
                                    });
                                    // A moved hardpoint gets a dotted trail back to where the DONOR
                                    // had it, so the displacement you are applying is visible.
                                    let d = [pos[0] - base[0], pos[1] - base[1], pos[2] - base[2]];
                                    if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 0.01 {
                                        for st in 1..7 {
                                            let f = st as f32 / 7.0;
                                            cards.push(mercs2_engine::particles::GlowCard {
                                                pos: [base[0] + d[0] * f, base[1] + d[1] * f, base[2] + d[2] * f],
                                                size: (p.radius * 0.012).clamp(0.008, 0.12),
                                                color: [1.0, 0.5, 0.1, 0.5],
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        scene.set_glow_cards(&cards);
                    }
                    // Ghost the mesh ONLY while a bone is hovered (in a tree or the mapping grid), so the
                    // highlighted bone reads through it; fully opaque otherwise.
                    scene.set_model_alpha(if hover_skel.is_some() || hovered_bone.is_some() { 0.4 } else { 1.0 });

                    // The Inspect preview is a persistent world entity — HIDE it in the Sandbox
                    // (drop its ModelRef so the draw query skips it) so only PLACED objects render
                    // there; otherwise it sits in the scene un-movable. Mods/Skeleton keep the
                    // preview (it is their working model), so restore its ModelRef when we leave.
                    if let Some(p) = &preview {
                        let show = wb != Workbench::Sandbox;
                        let has_ref = world.get::<&ModelRef>(p.entity).is_ok();
                        if show && !has_ref {
                            let _ = world.insert_one(p.entity, ModelRef { model: p.hash });
                        } else if !show && has_ref {
                            let _ = world.remove_one::<ModelRef>(p.entity);
                        }
                    }

                    // ── Render: plate view + empty viewport go through the menu path (shell
                    // plate backdrop); anything loaded renders the world. The GUI paints through
                    // the engine's overlay hook either way. ──
                    let mut overlay = |d: &wgpu::Device,
                                       q: &wgpu::Queue,
                                       e: &mut wgpu::CommandEncoder,
                                       v: &wgpu::TextureView,
                                       s: [u32; 2]| {
                        gui.paint(d, q, e, v, s);
                    };
                    let show_preview = preview.is_some() && wb != Workbench::Sandbox;
                    let r = if tex_view.is_some() || (!show_preview && placed.is_empty()) {
                        scene.render_menu_with(t, Some(&mut overlay))
                    } else {
                        let fwd = dir_from(cam_yaw, cam_pitch);
                        let eye = cam_target - fwd * cam_dist;
                        let view = Mat4::look_at_lh(eye, cam_target, Vec3::Y);
                        scene.set_view(view, 0.05, 40000.0);
                        scene.set_shadow(
                            cam_target.into(),
                            [-0.45, -1.0, -0.35],
                            (cam_dist * 1.5).clamp(4.0, 400.0),
                        );
                        scene.render_with(&world, Some(&mut overlay))
                    };
                    match r {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            scene.resize(scene.size)
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => elwt.exit(),
                        Err(e) => eprintln!("surface error: {e:?}"),
                    }
                }
                _ => {}
            }
            }
            _ => {}
        })
        .expect("event loop run");
}

/// Camera forward vector from yaw/pitch (the engine free-fly convention, LH +Y up).
/// Human-readable LOD-tier label for a `state_mask`. Each set bit is an LOD LEVEL — per the mercs1
/// `GetLOD` grammar (`ModelMunge/FlatModel.cpp`: a name digit `d` in `1..4` sets bit `d-1`; the
/// `_small`/`_far`/`_tiny` suffixes are cosmetic art labels the munger never reads), bit 0 = nearest.
/// `0x03` → "L1–L2", `0x70` → "L5–L7", `0x05` → "L1,L3", empty → "—".
fn lod_levels(mask: u8) -> String {
    let levels: Vec<u8> = (0..8u8).filter(|b| mask & (1 << b) != 0).map(|b| b + 1).collect();
    match levels.len() {
        0 => "—".to_string(),
        1 => format!("L{}", levels[0]),
        _ => {
            let (lo, hi) = (levels[0], *levels.last().unwrap());
            if (hi - lo + 1) as usize == levels.len() {
                format!("L{lo}–L{hi}")
            } else {
                levels.iter().map(|l| format!("L{l}")).collect::<Vec<_>>().join(",")
            }
        }
    }
}

/// Draw a crisp VECTOR glyph for a browser category (helicopter / tank / car / boat / jet / …),
/// painted with the egui painter so it stays sharp at any size — the BC-decoded store textures were
/// muddy at 14px. FILLED silhouette style (solid masses read better than outlines at ~15px); thin
/// parts (rotor, gun barrel, skids) are solid bars. Coordinates are centred on the icon (~±8).
fn paint_category_icon(p: &egui::Painter, cat: &str, rect: egui::Rect, col: egui::Color32) {
    use egui::{pos2, Shape, Stroke};
    let c = rect.center();
    // filled convex mass
    let m = |pts: &[(f32, f32)]| {
        let v: Vec<_> = pts.iter().map(|(x, y)| pos2(c.x + x, c.y + y)).collect();
        p.add(Shape::convex_polygon(v, col, Stroke::NONE));
    };
    // solid bar (a thin filled part)
    let bar = |a: (f32, f32), b: (f32, f32), w: f32| {
        p.line_segment([pos2(c.x + a.0, c.y + a.1), pos2(c.x + b.0, c.y + b.1)], Stroke::new(w, col));
    };
    // filled dot
    let dot = |o: (f32, f32), r: f32| p.circle_filled(pos2(c.x + o.0, c.y + o.1), r, col);
    match cat {
        "Helicopter" => {
            bar((-8.0, -4.5), (8.0, -4.5), 1.6); // main rotor
            bar((0.0, -4.5), (0.0, -2.0), 1.4); // mast
            m(&[(-5.5, -2.0), (3.0, -2.0), (3.5, 0.5), (-3.5, 2.0), (-5.5, 0.5)]); // cabin
            bar((3.0, -1.0), (8.0, -0.7), 1.6); // tail boom
            bar((8.0, -0.7), (8.0, -3.0), 1.4); // tail rotor
            bar((-4.0, 3.3), (3.0, 3.3), 1.4); // skid
            bar((-3.0, 2.0), (-3.0, 3.3), 1.2);
            bar((1.5, 1.6), (1.5, 3.3), 1.2);
        }
        "Tank" => {
            m(&[(-8.0, 1.5), (8.0, 1.5), (7.0, 4.0), (-7.0, 4.0)]); // tracks
            m(&[(-7.0, 1.5), (-6.0, -0.5), (6.0, -0.5), (7.0, 1.5)]); // hull
            m(&[(-3.0, -0.5), (-2.5, -3.5), (1.5, -3.5), (2.0, -0.5)]); // turret
            bar((1.5, -2.8), (8.0, -2.8), 1.6); // gun
        }
        "APC" => {
            m(&[(-7.5, 1.0), (-5.5, -2.5), (4.0, -2.5), (7.5, 1.0)]); // hull
            bar((-1.0, -2.5), (-1.0, -4.0), 1.4); // turret stub
            dot((-4.5, 2.5), 1.5);
            dot((0.0, 2.5), 1.5);
            dot((4.5, 2.5), 1.5);
        }
        "Car" | "Vehicle (other)" => {
            m(&[(-7.0, -0.2), (7.0, -0.2), (7.0, 2.0), (-7.0, 2.0)]); // lower body
            m(&[(-3.6, -0.2), (-2.0, -3.2), (3.0, -3.2), (4.0, -0.2)]); // cabin / greenhouse
            dot((-4.0, 2.6), 1.7);
            dot((4.0, 2.6), 1.7);
        }
        "Truck" | "Semi" | "Trailer" | "Towed" => {
            m(&[(-2.0, -4.0), (7.0, -4.0), (7.0, 1.5), (-2.0, 1.5)]); // box
            m(&[(-7.0, -1.0), (-2.0, -1.0), (-2.0, 1.5), (-7.0, 1.5)]); // cab
            dot((-4.5, 2.6), 1.4);
            dot((3.5, 2.6), 1.4);
        }
        "Van" => {
            m(&[(-7.0, -3.5), (5.0, -3.5), (7.0, -0.5), (7.0, 1.5), (-7.0, 1.5)]);
            dot((-4.0, 2.6), 1.4);
            dot((4.0, 2.6), 1.4);
        }
        "Motorcycle" => {
            dot((-4.0, 2.0), 2.4);
            dot((4.0, 2.0), 2.4);
            bar((-4.0, 2.0), (0.0, -1.5), 1.5);
            bar((0.0, -1.5), (4.0, 2.0), 1.5);
            bar((-2.0, -2.5), (1.0, -1.5), 1.3); // handlebar
        }
        "Boat" => {
            m(&[(-8.0, -0.5), (8.0, -0.5), (5.0, 4.0), (-5.0, 4.0)]); // hull
            m(&[(-2.5, -0.5), (-2.5, -3.5), (3.0, -3.5), (3.0, -0.5)]); // cabin
        }
        "Jet" | "VTOL" => {
            m(&[(0.0, -8.0), (1.4, -4.5), (1.1, 4.5), (-1.1, 4.5), (-1.4, -4.5)]); // fuselage
            m(&[(-1.0, -1.0), (-8.0, 2.5), (-1.0, 2.5)]); // left wing
            m(&[(1.0, -1.0), (8.0, 2.5), (1.0, 2.5)]); // right wing
            m(&[(-0.9, 4.0), (-3.5, 7.0), (-0.9, 6.5)]); // left tailplane
            m(&[(0.9, 4.0), (3.5, 7.0), (0.9, 6.5)]); // right tailplane
        }
        "Character" => {
            dot((0.0, -4.0), 2.3); // head
            m(&[(-2.6, 5.0), (-1.6, -0.5), (1.6, -0.5), (2.6, 5.0)]); // body
            bar((-3.2, 1.0), (3.2, 1.0), 1.4); // arms
        }
        "Building" => {
            m(&[(-6.0, 6.0), (-6.0, -1.5), (0.0, -6.0), (6.0, -1.5), (6.0, 6.0)]);
        }
        "Weapon" => {
            m(&[(-7.0, -2.0), (5.0, -2.0), (5.0, 0.5), (-7.0, 0.5)]); // barrel/slide
            m(&[(-4.0, 0.5), (-1.0, 0.5), (-1.0, 3.5), (-4.0, 3.5)]); // grip
        }
        "World state" => {
            dot((0.0, 0.0), 5.0);
        }
        _ => {
            // prop / other / unnamed: a small filled diamond
            m(&[(0.0, -4.0), (4.0, 0.0), (0.0, 4.0), (-4.0, 0.0)]);
        }
    }
}

/// Draw a row's category icon — a crisp painted vector glyph.
fn row_icon(ui: &mut egui::Ui, cat: &str, sz: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(sz, sz), egui::Sense::hover());
    paint_category_icon(ui.painter(), cat, rect, crate::gui::theme::DIM);
}

/// Integer with thousands separators (e.g. 3007 → "3,007").
fn commafy(n: usize) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn dir_from(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()).normalize()
}

fn name_or_hash(index: &AssetIndex, h: u32) -> String {
    index.names.get(&h).cloned().unwrap_or_else(|| format!("0x{h:08X}"))
}

/// Format an animation-table value: the tables' none-sentinel prints as "—", small integers
/// (flag/enum columns like Looping/Driven) print as numbers, hashes resolve through the name
/// index (game corpora) or fall back to hex.
fn anim_val(index: &AssetIndex, v: u32) -> String {
    if v == mercs2_formats::anim_select::NONE_SENTINEL {
        return "—".into();
    }
    if v < 0x1000 {
        return v.to_string();
    }
    name_or_hash(index, v)
}

/// Deterministic procedural clip name from the animation tables, for clips whose hash has no
/// corpus name. Every component is a game-table value resolved through the name index:
/// the FIRST ActionTable row's non-sentinel state fields (column order) joined with '.',
/// "+N" when more state rows share the clip, then the distinct loadout keys (row order,
/// equipment before Gender) in parentheses. Same table rows → same name.
fn procedural_clip_name(
    index: &AssetIndex,
    actions: &[(u32, mercs2_formats::anim_select::ActionRow)],
    contexts: &[mercs2_formats::anim_select::LookupContext],
) -> Option<String> {
    const NONE: u32 = mercs2_formats::anim_select::NONE_SENTINEL;
    let mut s = String::new();
    if let Some((_, a)) = actions.first() {
        let mut parts: Vec<String> = Vec::new();
        for v in [
            a.stance,
            a.action,
            a.aim_state,
            a.tandem,
            a.seat,
            a.target,
            a.action_direction,
            a.damage_direction,
        ] {
            if v != NONE {
                parts.push(anim_val(index, v));
            }
        }
        s = parts.join(".");
        if actions.len() > 1 {
            s.push_str(&format!("+{}", actions.len() - 1));
        }
    }
    let mut keys: Vec<String> = Vec::new();
    for c in contexts {
        for v in [
            c.primary_equipment_class,
            c.primary_equipment_name,
            c.in_use_equipment_class,
            c.in_use_equipment_name,
            c.gender,
        ] {
            if v != NONE {
                let n = anim_val(index, v);
                if !keys.contains(&n) {
                    keys.push(n);
                }
            }
        }
    }
    if !keys.is_empty() {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&format!("({})", keys.join("/")));
    }
    if s.is_empty() { None } else { Some(s) }
}

/// Extract + decode a texture (full streamed resolution when available) and bind it as the
/// engine's letterboxed plate; records dims/format into the caption.
fn bind_tex_plate(w: &mut WadStack, scene: &mut Scene, tv: &mut TexView) {
    let h = tv.hashes[tv.idx];
    match w.texture_best(h) {
        Ok(td) => {
            tv.info = format!("{}x{}  {:?}  {} mips", td.width, td.height, td.format, td.mip_count);
            scene.set_loading_art(&td);
        }
        Err(e) => tv.info = format!("EXTRACT FAILED: {e}"),
    }
}

/// Everything `load_preview` and the sandbox loader share: container → indexed geometry →
/// textures → skin (native metres) → clips.
#[derive(Clone)]
pub(crate) struct ModelData {
    pub(crate) verts: Vec<mesh::Vertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) draws: Vec<DrawGroup>,
    pub(crate) stats: mesh::ModelStats,
    pub(crate) skin: mesh::SkinData,
    pub(crate) textures: TexMap,
    /// SEGM state/LOD tier bits the container carries (F11 cycles them; empty = single-tier /
    /// imported) and the bit this build used.
    pub(crate) tiers: Vec<u8>,
    tier: u8,
    /// The ENGINE's destruction state machine (named states + Enter/Exit command scripts) —
    /// docs/destruction_orchestrator_format.md. None = non-destructible. Together with
    /// `hier_nodes` + `indx` it drives GROUND-TRUTH group visibility (no heuristics): break
    /// pieces hide because the entered state's script says so.
    machine: Option<mercs2_formats::orchestrator::StateMachine>,
    hier_nodes: Vec<mercs2_formats::orchestrator::HierNode>,
    indx: Vec<usize>,
    /// The container's top-level model header (authored AABB, node count, LOD-level count, LOD
    /// distance) — the per-model data the engine's LOD selector reads. `None` for imports. This is
    /// what makes the workshop AWARE of the model instead of assuming one global rule; the authored
    /// AABB frames the camera correctly (break-piece anchors no longer inflate the radius).
    header: Option<mercs2_formats::model_cubeize::ModelHeader>,
}

impl From<crate::import::Imported> for ModelData {
    fn from(i: crate::import::Imported) -> ModelData {
        ModelData {
            verts: i.verts,
            indices: i.indices,
            draws: i.draws,
            stats: i.stats,
            skin: i.skin,
            textures: i.textures,
            tiers: Vec::new(),
            tier: 0x01,
            machine: None,
            hier_nodes: Vec::new(),
            indx: Vec::new(),
            header: None,
        }
    }
}

/// CPU-side geometry for a model, wherever it lives: the in-app import/merge store first, else
/// re-read from the WAD stack. This is what merge and export operate on.
fn source_model_data(
    w: &mut WadStack,
    imported: &HashMap<u32, ModelData>,
    hash: u32,
) -> Result<ModelData, String> {
    if let Some(md) = imported.get(&hash) {
        return Ok(md.clone());
    }
    load_model_data(w, hash)
}

pub(crate) fn load_model_data(w: &mut WadStack, hash: u32) -> Result<ModelData, String> {
    load_model_data_tier(w, hash, 0x01)
}

/// The target character skeleton's HIER bone names, in bone order — the Skeleton workbench feeds
/// these to `retarget::Retarget::build` as the map's right-hand side. Empty when the model has no
/// rig or fails to load.
/// Run the FAITHFUL character skinning (`mercs2_formats::char_skin`) for the Skeleton workbench.
/// The workshop's Retarget bone map (auto-mapped + user-curated in the UI) is the AUTHORITATIVE
/// source→target mapping — it is fed to `char_skin` as full overrides, so what the user sees and
/// edits is exactly what the preview renders and the export writes (preview == export == in-game).
/// Returns the `char_skin` result, the raw source glb data, and the target donor block (for
/// injection / the target rig). `target_index == global HIER index` because both the workshop's
/// `target_bone_info` and `TargetSkeleton::from_skeleton` enumerate the target's HIER in order.
pub(crate) fn faithful_char_skin(
    wad_paths: &[String],
    thash: u32,
    rt: &crate::retarget::Retarget,
    src_path: &std::path::Path,
) -> Result<
    (
        mercs2_formats::char_skin::CharSkin,
        mercs2_formats::char_skin::CharGlbData,
        Vec<u8>,
    ),
    String,
> {
    let donor = crate::publish::donor_block(wad_paths, thash)?;
    let skel = mercs2_formats::skeleton::Skeleton::from_block(&donor)?;
    let target = mercs2_formats::char_skin::TargetSkeleton::from_skeleton(&skel);
    let glb = crate::import::load_char_glb(src_path)?;
    // char_skin::build_character runs Logan's automap on the FULL node graph, FINGER-COLLAPSES in
    // NPC-84 space, then resolves onto the donor's HIER by name — so it stays correct on HERO donors
    // and keeps big finger rigs (50 Cent → mattias) under the palette cap. For non-CoD rigs we let it
    // do that and only layer the user's MANUAL overrides on top. CoD's `j_*` naming it can't read, so
    // there we feed the hand-verified explicit table as full overrides.
    let overrides: std::collections::HashMap<usize, Option<u32>> =
        if rt.convention == crate::retarget::SourceRig::CallOfDuty {
            let table = rt.joint_table(target.bones.len().max(1));
            table.iter().enumerate().map(|(j, &t)| (j, Some(t as u32))).collect()
        } else {
            rt.map
                .iter()
                .filter(|m| m.confidence == crate::retarget::Confidence::Manual)
                .map(|m| (m.source_index, m.target_index.map(|t| t as u32)))
                .collect()
        };
    let mut cs = mercs2_formats::char_skin::build_character(&glb.build_input(&target, None, overrides, false))?;
    // DONOR TRANSFER. build_character carries the SOURCE rig's weights across the bone map, which is
    // fuzzy on the limbs for a mismatched rig (a 119-bone Unreal mannequin onto a 116-bone Pandemic
    // skeleton) and tears the arms. Overwrite them by sampling the RETAIL donor's own weights at each
    // conformed vertex -- the exact step the CLI did and this path used to skip, which is why the
    // workshop preview showed broken arms the shipped asset does not have.
    match mercs2_formats::char_skin::donor_transfer::apply_donor_transfer(
        &mut cs,
        &glb.tris,
        &donor,
        &mercs2_formats::char_skin::donor_transfer::DonorTransferOpts::default(),
    ) {
        Ok(msg) => eprintln!("faithful_char_skin: {msg}"),
        Err(e) => eprintln!("faithful_char_skin: donor transfer skipped ({e}); using conform weights"),
    }
    Ok((cs, glb, donor))
}

/// The target HIER bone names AND their bind-pose positions (translation of `world_bind`), index-
/// aligned. Uses `skin.rig` — the SAME rig `RetargetApply` grafts onto the import — so the retarget's
/// target indices line up with what actually gets rendered. Positions feed the spatial align.
pub(crate) fn target_bone_info(w: &mut WadStack, hash: u32) -> (Vec<String>, Vec<[f32; 3]>, Vec<i32>) {
    let Ok(md) = load_model_data(w, hash) else { return (Vec::new(), Vec::new(), Vec::new()) };
    let hashes: Vec<u32> = md.skin.rig.iter().map(|b| b.name_hash).collect();
    let names_map = resolve_node_names(&hashes);
    let names = md
        .skin
        .rig
        .iter()
        .map(|b| names_map.get(&b.name_hash).cloned().unwrap_or_else(|| format!("0x{:08X}", b.name_hash)))
        .collect();
    let pos = md
        .skin
        .rig
        .iter()
        .map(|b| [b.world_bind[3][0], b.world_bind[3][1], b.world_bind[3][2]])
        .collect();
    let parents = md.skin.rig.iter().map(|b| b.parent).collect();
    (names, pos, parents)
}

/// The render state a freshly-placed instance of `hash` should have: LOD rung 0, plus the destruction
/// machine's node-enable table at its DEFAULT state (pristine). Every spawned entity needs one —
/// models are uploaded whole now, so an entity with no state draws its wreck too.
///
/// Imported/synthetic models resolve to no container; their draw groups carry `lod_mask = 0xFF` and
/// `node = -1`, so the empty state draws them in full.
pub(crate) fn default_render_state(w: &mut WadStack, hash: u32) -> mercs2_engine::render_state::RenderState {
    use mercs2_formats::orchestrator as orch;
    let Ok(c) = w.extract_container(hash) else {
        return mercs2_engine::render_state::RenderState::rung0(0);
    };
    let hier = orch::parse_hier(&c);
    let node_enable = orch::parse_state_machine(&c)
        .map(|sm| {
            let chosen: Vec<usize> = sm.nodes.iter().map(orch::default_state_index).collect();
            orch::machine_node_enable(&sm, &hier, &chosen)
        })
        .unwrap_or_default();
    mercs2_engine::render_state::RenderState { lod: 0, view_state: 0x01, node_enable }
}

/// The 3-rung cross-fade `view_state` centred on rung `n`: `1<<(n-1) | 1<<n | 1<<(n+1)`, exactly
/// `FUN_0047724e` (the `n-1` term drops off at rung 0; `n+1` past bit 7 falls out of the byte).
fn window_view_state(n: u8) -> u8 {
    let bit = |i: i32| -> u8 { if (0..8).contains(&i) { 1u8 << i } else { 0 } };
    bit(n as i32 - 1) | bit(n as i32) | bit(n as i32 + 1)
}

/// The engine render state a preview should have: `view_state` = the selected LOD-rung bit, and the
/// destruction machine's node-enable table for the chosen per-node states. A model with no machine
/// gets an empty table, which passes clause 3 for every segment.
fn preview_render_state(md: &ModelData, node_state: &[usize]) -> mercs2_engine::render_state::RenderState {
    let node_enable = md
        .machine
        .as_ref()
        .map(|sm| mercs2_formats::orchestrator::machine_node_enable(sm, &md.hier_nodes, node_state))
        .unwrap_or_default();
    mercs2_engine::render_state::RenderState {
        lod: md.tier.trailing_zeros() as u8,
        view_state: md.tier,
        node_enable,
    }
}

/// Load a model at a specific SEGM state/LOD tier (`active_bit` of `build_indexed_state`) —
/// F11's rebuild path. Falls back to the container's first tier if the requested bit is absent.
fn load_model_data_tier(w: &mut WadStack, hash: u32, want_bit: u8) -> Result<ModelData, String> {
    // A model is scattered across BLOCKS, not held in one container: the resident block ships the
    // object (HIER, SEGM, MTRL, physics, destruction machine) plus its coarsest meshes, and each
    // finer `_P00N_Q(3-N)` block ships geometry + an INDX that names rows in the RESIDENT block's
    // SEGM. Loading only the resident block is what made every vehicle a low-poly far-LOD proxy —
    // a 371-triangle tank in a `_lod_dm` skin. Bind each rung against the resident block and merge
    // them into one buffer; the draw gate then selects per segment, which is what it is for.
    // `Model` owns the cross-block rules — binding each rung's INDX against the resident SEGM, and
    // clearing the tier bits a finer block re-authors so the rungs refine rather than double-draw.
    // Same assembly the game world loads through; visibility stays a per-frame gate decision.
    let m = w.model(hash)?;
    let container = m.resident.clone();
    let tiers = mesh::state_tiers(&container);
    let (verts, indices, draws, stats) = m.flatten();
    // The engine's own named-state machine + the HIER/INDX it acts on — ground-truth
    // destruction visibility comes from executing its scripts, nothing is classified.
    let machine = mercs2_formats::orchestrator::parse_state_machine(&container);
    let hier_nodes = mercs2_formats::orchestrator::parse_hier(&container);
    let indx = mercs2_formats::orchestrator::parse_indx(&container);
    let header = mercs2_formats::model_cubeize::parse_model_header(&container);
    // Rung 0 — the closest tier, what you see standing next to the thing. It used to be empty for
    // vehicles (their near geometry was in a block we never opened), so a "pick the fullest tier"
    // heuristic stood in for it. With the chain assembled, tier 0 is real geometry and needs no
    // guess.
    let tier = if want_bit != 0x01 && tiers.contains(&want_bit) { want_bit } else { 0x01 };
    // FULL mip chain, all three slots. The resident block ships only a coarse mip tail (a 1024²
    // normal map arrives as 1,360 B = 32×32), so loading the resident texture here was serving the
    // preview a blurred model and writing near-empty PNGs on export. `texture_best` assembles the
    // higher mips from the finer LOD blocks of the texture's own cell subtree and memoizes, so the
    // subtree walk that made this "too slow" is now paid once per texture per session.
    let mut textures: TexMap = HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal, d.specular].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = w.texture_best(h) {
                    textures.insert(h, t);
                }
            }
        }
    }
    let mut skin = stats.skin_data();
    // Native metres, entity Transform places it — same convention as the streaming world path.
    skin.center = [0.0, 0.0, 0.0];
    skin.scale = 1.0;
    Ok(ModelData {
        verts,
        indices,
        draws,
        stats,
        skin,
        textures,
        tiers,
        tier,
        machine,
        hier_nodes,
        indx,
        header,
    })
}

/// Parse the resident AnimationLookup once (base resident block 3185, else any resident-named
/// block) — the same discovery order as the game's `resolve_player_idle`.
pub(crate) fn load_anim_selector(w: &mut wad::Wad) -> Option<AnimSelector> {
    if let Ok(dec) = wad::decompress_block_index(w, 3185) {
        if let Some(s) = AnimSelector::from_resident_block(&dec) {
            return Some(s);
        }
    }
    let resident: Vec<usize> = {
        let paths = wad::block_paths(w);
        paths
            .iter()
            .enumerate()
            .filter(|(i, p)| *i != 3185 && p.to_ascii_lowercase().contains("resident"))
            .map(|(i, _)| i)
            .collect()
    };
    for i in resident {
        if let Ok(dec) = wad::decompress_block_index(w, i as u16) {
            if let Some(s) = AnimSelector::from_resident_block(&dec) {
                return Some(s);
            }
        }
    }
    None
}

/// Game-script references for an asset: corpus files containing its NAME (exact substring; when
/// the full name has no hits, the name's last two `_` segments are tried — the needle actually
/// used is returned and displayed with the results).
fn lua_references(
    corpus: &[(String, String, String)],
    label: &str,
) -> (String, Vec<(String, Vec<String>)>) {
    let full = label.to_ascii_lowercase();
    let segs: Vec<&str> = full.split('_').collect();
    let tail =
        if segs.len() >= 2 { segs[segs.len() - 2..].join("_") } else { full.clone() };
    for needle in [full.clone(), tail] {
        if needle.len() < 4 || needle.starts_with("0x") {
            continue;
        }
        let mut hits: Vec<(String, Vec<String>)> = Vec::new();
        for (path, content, lower) in corpus {
            if lower.contains(&needle) {
                let lines: Vec<String> = content
                    .lines()
                    .filter(|l| l.to_ascii_lowercase().contains(&needle))
                    .take(3)
                    .map(|l| l.trim().chars().take(96).collect())
                    .collect();
                hits.push((path.clone(), lines));
                if hits.len() >= 12 {
                    break;
                }
            }
        }
        if !hits.is_empty() {
            return (needle, hits);
        }
    }
    (full, Vec::new())
}

/// CharacterName candidates from a model label: the tail after `hum_` (else the whole label),
/// progressively stripping `_suffix` segments — `pmc_hum_mattias_v3` → `mattias_v3` → `mattias`.
/// Every clip the GAME associates with this asset — the set an export must ship.
///
/// The generic rig-matched loader (`clips_for_model`) is the wrong source here: all humans share the
/// same HIER, so it matches by skeleton coverage and is deliberately capped at `MAX_AUTO_CLIPS` (6)
/// to bound decode cost in the preview. That is a working set, not the character's animation set —
/// Mattias has ~100 clips and Chris ~111, and they are DISJOINT.
///
/// The authoritative source is the AnimationLookup table, keyed by `CharacterName = m2(name)`, which
/// is how the engine itself picks a clip (see the human-animation-selection chain). Fall back to the
/// generic set only for a rig the tables do not name — props, vehicles, unnamed skeletons.
pub(crate) fn clips_for_export(
    w: &mut WadStack,
    label: &str,
    rig: &[BoneRig],
    index: &AssetIndex,
) -> (Vec<ClipAnim>, HashMap<u32, String>) {
    let mut names: HashMap<u32, String> = HashMap::new();
    if rig.is_empty() {
        return (Vec::new(), names);
    }
    let hier: Vec<u32> = rig.iter().map(|b| b.name_hash).collect();

    // Character-specific: walk the label's name candidates (pmc_hum_mattias_v3 -> mattias_v3 ->
    // mattias) until one names rows in the table, exactly as the preview's catalog does.
    let mut want: Vec<u32> = Vec::new();
    if let Some(sel) = w.anim_selector() {
        for cand in character_candidates(label) {
            let character = AnimSelector::character_name(&cand);
            let rows = sel.character_clips(character);
            if rows.is_empty() {
                continue;
            }
            // A clip answers SEVERAL handles (equipment variants share clips); gather them all, as
            // the name is derived from the game states those handles play.
            let mut handles: HashMap<u32, Vec<u32>> = HashMap::new();
            for r in &rows {
                let hs = handles.entry(r.clip).or_default();
                if hs.is_empty() {
                    want.push(r.clip);
                }
                if !hs.contains(&r.handle) {
                    hs.push(r.handle);
                }
            }
            // Clip names are stripped on disk, so most resolve to nothing in the catalog. Fall back
            // to the PROCEDURAL name the preview shows — Stance.Action.AimState... read straight out
            // of the ActionTable, i.e. game-table values, not invented labels.
            for &clip in &want {
                let hs = &handles[&clip];
                let mut actions = Vec::new();
                let mut contexts = Vec::new();
                for &h in hs {
                    actions.extend(sel.handle_actions(h).into_iter().map(|a| (h, a)));
                    contexts
                        .extend(sel.lookup_context(h, character).into_iter().filter(|c| c.clip == clip));
                }
                if let Some(n) = index
                    .names
                    .get(&clip)
                    .cloned()
                    .or_else(|| procedural_clip_name(index, &actions, &contexts))
                {
                    names.insert(clip, n);
                }
            }
            break;
        }
    }
    if want.is_empty() {
        // Not a table-named character: the rig-matched animgroup set is all there is.
        let generic = w.clips_for_model(rig);
        for c in &generic {
            if let Some(n) = index.names.get(&c.name_hash) {
                names.insert(c.name_hash, n.clone());
            }
        }
        return (generic, names);
    }
    // A table row can name a clip this rig cannot bind; `clip_for_rig` still returns it (with zero
    // tracks resolved), and the bundle records it as present-but-unbound rather than shipping a
    // dead T-pose animation.
    (want.iter().filter_map(|&h| w.clip_for_rig(&hier, h)).collect(), names)
}

pub(crate) fn character_candidates(label: &str) -> Vec<String> {
    let tail = label.find("hum_").map(|i| &label[i + 4..]).unwrap_or(label);
    let mut v = vec![tail.to_string()];
    let mut cur = tail.to_string();
    while let Some(i) = cur.rfind('_') {
        cur.truncate(i);
        v.push(cur.clone());
    }
    // Model-name → CharacterName aliases: Jennifer's models are `pmc_hum_jen_*` but her
    // AnimationLookup CharacterName is m2("jennifer").
    if v.iter().any(|c| c == "jen") {
        v.push("jennifer".into());
    }
    v
}

/// Imported preview → `model_inject` input: engine verts back to plain positions/normals/uvs,
/// triangles from the index buffer. Rigid (empty joints/weights = bone-0 bind in the donor) —
/// skinned weight transfer is its own workstream. Donor-frame fit is the source file's job.
fn external_mesh_of(md: &ModelData) -> mercs2_formats::model_inject::ExternalMesh {
    external_mesh_transformed(md, 1.0, [0.0; 3], [0.0; 3])
}

/// Conform rotation from XYZ euler degrees (the panel's rotation fields).
fn conform_quat(r_deg: [f32; 3]) -> Quat {
    Quat::from_euler(
        glam::EulerRot::XYZ,
        r_deg[0].to_radians(),
        r_deg[1].to_radians(),
        r_deg[2].to_radians(),
    )
}

/// Bake the conform panel's interactive transform (uniform scale → rotate → translate) into the
/// mesh handed to the conform injector, so what the user positions against the template IS what
/// ships. Normals are rotated (scale is uniform, translation ignored for normals).
fn external_mesh_transformed(
    md: &ModelData,
    scale: f32,
    t: [f32; 3],
    r_deg: [f32; 3],
) -> mercs2_formats::model_inject::ExternalMesh {
    let q = conform_quat(r_deg);
    let s = if scale.abs() > 1e-6 { scale } else { 1.0 };
    let tv = Vec3::from(t);
    let tp = |p: [f32; 3]| {
        let v = q * (Vec3::from(p) * s) + tv;
        [v.x, v.y, v.z]
    };
    let tn = |n: [f32; 3]| {
        let v = (q * Vec3::from(n)).normalize_or_zero();
        [v.x, v.y, v.z]
    };
    mercs2_formats::model_inject::ExternalMesh {
        positions: md.verts.iter().map(|v| tp(v.pos)).collect(),
        normals: md.verts.iter().map(|v| tn(v.normal)).collect(),
        uvs: md.verts.iter().map(|v| v.uv).collect(),
        tris: md.indices.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect(),
        joints: Vec::new(),
        weights: Vec::new(),
    }
}

/// Axis-aligned bbox (min,max) over a ModelData's vertex positions — the REAL geometry envelope
/// used to seed the conform auto-fit (mirrors `inject_static`'s real-envelope fit, not the padded
/// top-INFO bbox).
fn model_pos_bbox(md: &ModelData) -> ([f32; 3], [f32; 3]) {
    let mut mn = [f32::MAX; 3];
    let mut mx = [f32::MIN; 3];
    for v in &md.verts {
        for k in 0..3 {
            mn[k] = mn[k].min(v.pos[k]);
            mx[k] = mx[k].max(v.pos[k]);
        }
    }
    (mn, mx)
}

/// Seed the conform transform so the import fills the donor's real geometry envelope: uniform
/// scale to the tightest axis, centred in X/Z, bottom-aligned in Y (feet/skids on the ground).
/// Returns (scale, translate) for the panel fields; rotation is left to the user.
fn conform_autofit(donor: &ModelData, import: &ModelData) -> (f32, [f32; 3]) {
    let (tmin, tmax) = model_pos_bbox(donor);
    let (mmin, mmax) = model_pos_bbox(import);
    if tmin[0] > tmax[0] || mmin[0] > mmax[0] {
        return (1.0, [0.0; 3]);
    }
    let mut s = f32::MAX;
    for k in 0..3 {
        let md = mmax[k] - mmin[k];
        if md > 1e-4 {
            s = s.min((tmax[k] - tmin[k]).abs() / md);
        }
    }
    if !s.is_finite() || s <= 0.0 {
        s = 1.0;
    }
    let mcen = [(mmin[0] + mmax[0]) * 0.5, mmin[1], (mmin[2] + mmax[2]) * 0.5];
    let tgt = [(tmin[0] + tmax[0]) * 0.5, tmin[1], (tmin[2] + tmax[2]) * 0.5];
    // translate = target - scale*mcen (so mesh min-Y and X/Z centre land on the envelope).
    let t = [tgt[0] - s * mcen[0], tgt[1] - s * mcen[1], tgt[2] - s * mcen[2]];
    (s, t)
}

/// GPU-upload a model by hash (used by the sandbox scene loader for models not previewed yet).
fn load_gpu_only(w: &mut WadStack, scene: &mut Scene, hash: u32) -> Result<(), String> {
    let md = load_model_data(w, hash)?;
    scene.load_model(hash, &md.verts, &md.indices, &md.draws, &md.textures, &md.skin);
    Ok(())
}

/// Put `md` on the preview pedestal as `hash`: upload to the GPU, spawn its entity at the
/// origin, tear down the previous preview (keeping its model resident if the sandbox still uses
/// it). Works for WAD models, drag-drop imports, and merge results alike.
#[allow(clippy::too_many_arguments)]
/// Render a bone hierarchy (names + parent indices, `-1` = root) as a collapsible tree. Returns the
/// index of the bone whose row is hovered this frame (for the viewer highlight), if any. `id` keeps
/// two trees in the same panel from colliding.
fn bone_tree(ui: &mut egui::Ui, names: &[String], parents: &[i32], id: &str) -> Option<usize> {
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..names.len() {
        match parents.get(i).copied().unwrap_or(-1) {
            p if p >= 0 && (p as usize) < names.len() => children[p as usize].push(i),
            _ => roots.push(i),
        }
    }
    fn draw(ui: &mut egui::Ui, i: usize, names: &[String], kids: &[Vec<usize>], id: &str, hov: &mut Option<usize>) {
        if kids[i].is_empty() {
            if ui.selectable_label(false, egui::RichText::new(names[i].as_str()).monospace().size(11.0)).hovered() {
                *hov = Some(i);
            }
        } else {
            let r = egui::CollapsingHeader::new(egui::RichText::new(names[i].as_str()).monospace().size(11.0))
                .id_source((id, i))
                .default_open(true)
                .show(ui, |ui| {
                    for &c in &kids[i] {
                        draw(ui, c, names, kids, id, hov);
                    }
                });
            if r.header_response.hovered() {
                *hov = Some(i);
            }
        }
    }
    let mut hov = None;
    for r in roots {
        draw(ui, r, names, &children, id, &mut hov);
    }
    hov
}

/// Compact clip player for the Skeleton page: transport for the current clip + a click-to-play
/// list. Shares the ClipSel / PlayPause / ClipStop actions with the Inspect workbench, so playback
/// state stays in sync no matter which panel drives it.
fn clip_player_compact(ui: &mut egui::Ui, p: &Preview, actions: &mut Vec<Act>) {
    if p.clip_catalog.is_empty() {
        ui.weak("no clips — Apply retarget onto a target skeleton first");
        return;
    }
    if let Some(ci) = p.cur_clip {
        let hash = p.clip_catalog[ci].hash;
        match p.clip_cache.get(&hash) {
            Some(Some(c)) => {
                let dur = c.clip.duration.max(1e-3);
                ui.horizontal(|ui| {
                    if ui.button(if p.playing { "\u{23F8}" } else { "\u{25B6}" }).clicked() {
                        actions.push(Act::PlayPause);
                    }
                    if ui.button("\u{23F9}").clicked() {
                        actions.push(Act::ClipStop);
                    }
                    ui.monospace(format!("{:>4.1} / {dur:.1}s", p.anim_time));
                });
            }
            Some(None) => {
                ui.colored_label(crate::gui::theme::BAD, "clip not bound to this rig");
            }
            None => {
                ui.weak("loading clip\u{2026}");
            }
        }
    }
    egui::ScrollArea::vertical()
        .max_height(340.0)
        .id_source("skel_clip_scroll")
        .show(ui, |ui| {
            for i in 0..p.clip_catalog.len() {
                let sel = p.cur_clip == Some(i);
                if ui.selectable_label(sel, p.clip_catalog[i].label.as_str()).clicked() {
                    actions.push(Act::ClipSel(i));
                }
            }
        });
}

/// Import a foreign model file (.obj / .gltf / .glb) onto the preview pedestal. The ONE shared path
/// for both the Import button and drag-drop, so they can never drift. Returns the status line.
#[allow(clippy::too_many_arguments)]
fn import_file(
    path: &std::path::Path,
    w: &mut WadStack,
    scene: &mut Scene,
    world: &mut World,
    imported: &mut HashMap<u32, ModelData>,
    preview: &mut Option<Preview>,
    cam_target: &mut Vec3,
    cam_dist: &mut f32,
    retarget: &mut Option<crate::retarget::Retarget>,
    retarget_target: &Option<(u32, String)>,
    retarget_src_path: &mut Option<std::path::PathBuf>,
    wb: &mut Workbench,
    sel_bone: &mut Option<usize>,
    placed: &[Placed],
    index: &AssetIndex,
    anim_sel: &Option<AnimSelector>,
    lua_corpus: &[(String, String, String)],
) -> String {
    let im = match crate::import::import_model(path) {
        Ok(im) => im,
        Err(e) => return format!("IMPORT FAILED: {e}"),
    };
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("import").to_string();
    let label = format!("import_{stem}");
    let hash = mercs2_formats::hash::pandemic_hash_m2(&label);
    // A rigged import feeds the Skeleton workbench: detect the source rig and (re)build the bone map.
    let src_joints = im.skin_joints.clone();
    let src_pos = im.skin_joint_pos.clone();
    let src_ibm = im.skin_ibm.clone();
    let src_parents = im.skin_parents.clone();
    let md: ModelData = im.into();
    imported.insert(hash, md.clone());
    scene.unload_model(hash); // re-import of the same file replaces it
    let p = build_preview(
        w, scene, world, hash, label, md, &*preview, placed, index, anim_sel, lua_corpus,
    );
    *cam_target = p.center;
    *cam_dist = (p.radius * 2.4).clamp(0.5, 15000.0);
    let status = if src_joints.is_empty() {
        *retarget = None;
        *retarget_src_path = None;
        format!(
            "imported {} — {} verts, {} groups, {} textures (F6 place, F10 export)",
            p.label, p.verts, p.draws.len(), p.tex_hashes.len()
        )
    } else {
        let (target_names, target_pos, target_parents) = retarget_target
            .as_ref()
            .map(|(h, _)| target_bone_info(w, *h))
            .unwrap_or_default();
        let r = crate::retarget::Retarget::build_full(
            src_joints, src_pos, src_ibm, src_parents, target_names, target_pos, target_parents,
        );
        let s = format!(
            "imported RIGGED {} — {} source bones ({}); Skeleton workbench → pick a target + Apply",
            p.label,
            r.source_bones.len(),
            r.convention.label()
        );
        *retarget = Some(r);
        *retarget_src_path = Some(path.to_path_buf());
        *wb = Workbench::Skeleton;
        s
    };
    *sel_bone = None;
    *preview = Some(p);
    status
}

fn build_preview(
    w: &mut WadStack,
    scene: &mut Scene,
    world: &mut World,
    hash: u32,
    label: String,
    md: ModelData,
    old: &Option<Preview>,
    placed: &[Placed],
    index: &AssetIndex,
    anim_sel: &Option<AnimSelector>,
    lua_corpus: &[(String, String, String)],
) -> Preview {

    // Tear down the previous preview AFTER the new load succeeded (a failed load keeps the old one).
    if let Some(p) = old {
        world.despawn(p.entity).ok();
        scene.forget_entity(p.entity);
        if p.hash != hash && !placed.iter().any(|pl| pl.hash == p.hash) {
            scene.unload_model(p.hash);
        }
    }

    scene.load_model(hash, &md.verts, &md.indices, &md.draws, &md.textures, &md.skin);
    // GROUND-TRUTH visibility, exactly the engine's three-clause draw gate, evaluated per segment
    // against THIS entity's state — not baked into the vertex buffer and not keyed by model hash.
    //  - clause 2: the LOD-rung mask vs `view_state` (the tier the preview starts at).
    //  - clause 3: the node-enable table the destruction machine writes. Keyed by the SEGM record's
    //    node, NOT by `INDX[group]` — they disagree (md500: 5 of 19 groups), and it is precisely that
    //    disagreement that used to leave the wreck on screen next to the intact body.
    let node_state: Vec<usize> = md
        .machine
        .as_ref()
        .map(|sm| sm.nodes.iter().map(mercs2_formats::orchestrator::default_state_index).collect())
        .unwrap_or_default();
    let rs = preview_render_state(&md, &node_state);
    let rs_node_enable = rs.node_enable.clone();
    let mut hidden: HashSet<usize> = HashSet::new();
    for (gi, d) in md.draws.iter().enumerate() {
        // Clear any stale per-model override from a previous preview of this hash; the gate decides.
        scene.set_draw_hidden(hash, gi, false);
        if !rs.segment_visible(d.lod_mask, d.node) {
            hidden.insert(gi);
        }
    }
    let mut bind: Vec<[[f32; 4]; 4]> =
        if md.skin.bones.is_empty() { vec![IDENTITY] } else { md.skin.bones.clone() };
    // Imported rigged meshes carry a single-entry identity skin, but their per-vertex joints index a
    // full source (or, post-retarget, target) palette. Pad with identity so every referenced joint
    // resolves — an out-of-range read returns a zero matrix on the GPU, which collapses those
    // vertices onto the origin (the "smooshed spike"). At identity the mesh renders at its rest pose.
    // No-op for WAD models, whose palette already covers their joints.
    let max_joint = md.verts.iter().flat_map(|v| v.joints).map(|j| j as usize).max().unwrap_or(0);
    if bind.len() <= max_joint {
        bind.resize(max_joint + 1, IDENTITY);
    }
    let entity = world.spawn((
        Transform::IDENTITY,
        ModelRef { model: hash },
        AnimState::default(),
        SkinPalette { mats: bind.clone() },
    ));
    scene.set_entity_render_state(entity, rs);

    // Frame the camera from the AUTHORED model AABB (the header the engine's LOD selector reads),
    // not the built-geometry bbox: the latter spans every break-piece anchor (ejected far from the
    // body), which over-inflated the orbit radius (destroyer read ~90 m). Fall back to the built
    // bbox for imports / headerless models.
    let (bmin, bmax) = match &md.header {
        Some(h) => (Vec3::from(h.aabb_min), Vec3::from(h.aabb_max)),
        None => (Vec3::from(md.stats.bbox_min), Vec3::from(md.stats.bbox_max)),
    };
    let center = (bmin + bmax) * 0.5;
    let radius = ((bmax - bmin).length() * 0.5).max(0.5);
    let mut tex_hashes: Vec<u32> = md.textures.keys().copied().collect();
    tex_hashes.sort_unstable();
    let tris = md.indices.len() / 3;
    let (lua_needle, lua_refs) = lua_references(lua_corpus, &label);

    // Clip catalog: prefer the CHARACTER-SPECIFIC AnimationLookup set — every character (merc,
    // NPC, DLC costume) has its own rows keyed by CharacterName = m2(name), so two humans with
    // the same skeleton still get THEIR clips (which the generic rig-coverage loader cannot do —
    // all humans share the HIER, so it returned the same animgroup for everyone).
    let hier: Vec<u32> = md.skin.rig.iter().map(|b| b.name_hash).collect();
    let mut clip_cache: HashMap<u32, Option<ClipAnim>> = HashMap::new();
    let mut clip_catalog: Vec<ClipEntry> = Vec::new();
    let mut character_set = None;
    if let Some(sel) = anim_sel {
        for cand in character_candidates(&label) {
            let character = AnimSelector::character_name(&cand);
            let rows = sel.character_clips(character);
            if !rows.is_empty() {
                // One catalog row per clip; a clip can answer SEVERAL Handles (equipment
                // variants share clips) — gather them all for the context panel.
                let mut order: Vec<u32> = Vec::new();
                let mut handles: HashMap<u32, Vec<u32>> = HashMap::new();
                for r in &rows {
                    let hs = handles.entry(r.clip).or_default();
                    if hs.is_empty() {
                        order.push(r.clip);
                    }
                    if !hs.contains(&r.handle) {
                        hs.push(r.handle);
                    }
                }
                for clip in order {
                    let hs = &handles[&clip];
                    let mut actions = Vec::new();
                    let mut contexts = Vec::new();
                    for &h in hs {
                        actions.extend(sel.handle_actions(h).into_iter().map(|a| (h, a)));
                        contexts.extend(
                            sel.lookup_context(h, character).into_iter().filter(|c| c.clip == clip),
                        );
                    }
                    let name = index
                        .names
                        .get(&clip)
                        .cloned()
                        .or_else(|| procedural_clip_name(index, &actions, &contexts));
                    clip_catalog.push(ClipEntry {
                        hash: clip,
                        handles: hs.clone(),
                        label: name.clone().unwrap_or_else(|| format!("0x{clip:08X}")),
                        name,
                    });
                }
                character_set = Some(cand);
                break;
            }
        }
    }
    if clip_catalog.is_empty() && !hier.is_empty() {
        // Generic fallback (props/vehicles/unnamed rigs): rig-matched animgroup clips, already
        // decoded — seed the cache directly.
        for ca in w.clips_for_model(&md.skin.rig) {
            let name = index.names.get(&ca.name_hash).cloned();
            clip_catalog.push(ClipEntry {
                hash: ca.name_hash,
                handles: Vec::new(),
                label: name.clone().unwrap_or_else(|| format!("0x{:08X}", ca.name_hash)),
                name,
            });
            clip_cache.insert(ca.name_hash, Some(ca));
        }
    }

    Preview {
        hash,
        label,
        entity,
        rig: md.skin.rig.clone(),
        bind,
        draws: md.draws,
        tex_hashes,
        clip_catalog,
        character_set,
        clip_cache,
        hier,
        retarget_source: None,
        tiers: md.tiers.clone(),
        tier: md.tier,
        machine: md.machine.clone(),
        hier_nodes: md.hier_nodes.clone(),
        indx: md.indx.clone(),
        node_state,
        health: 1.0,
        hide_ruin: false,
        node_enable: rs_node_enable,
        header: md.header,
        lua_needle,
        lua_refs,
        cur_clip: None,
        anim_time: 0.0,
        playing: false,
        sel_group: 0,
        hidden,
        verts: md.verts.len(),
        tris,
        center,
        radius,
    }
}

/// Bake every placed sandbox instance into ONE static model: vertices transformed by each
/// instance's Transform, index ranges concatenated, draw groups renumbered, texture maps
/// unioned. Skinned sources are baked at BIND POSE and re-weighted to bone 0 (identity), so the
/// merged model renders correctly with no rig.
fn merge_placed(
    w: &mut WadStack,
    imported: &HashMap<u32, ModelData>,
    placed: &[Placed],
) -> Result<ModelData, String> {
    let mut verts: Vec<mesh::Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut draws: Vec<DrawGroup> = Vec::new();
    let mut textures: TexMap = HashMap::new();
    for pl in placed {
        let md = source_model_data(w, imported, pl.hash)
            .map_err(|e| format!("{}: {e}", pl.label))?;
        let m = Mat4::from_scale_rotation_translation(
            Vec3::splat(pl.scale),
            Quat::from_rotation_y(pl.yaw),
            pl.pos,
        );
        let base = verts.len() as u32;
        for v in &md.verts {
            let mut nv = *v;
            nv.pos = m.transform_point3(Vec3::from(v.pos)).into();
            nv.normal = m.transform_vector3(Vec3::from(v.normal)).normalize_or_zero().into();
            // Static bake: everything weights to bone 0 (identity palette) — see doc comment.
            nv.joints = [0, 0, 0, 0];
            nv.weights = [255, 0, 0, 0];
            verts.push(nv);
        }
        for d in &md.draws {
            let start = indices.len() as u32;
            let s = d.index_start as usize;
            let e = (d.index_start + d.index_count) as usize;
            indices.extend(md.indices[s.min(md.indices.len())..e.min(md.indices.len())].iter().map(|&i| base + i));
            draws.push(DrawGroup {
                index_start: start,
                index_count: indices.len() as u32 - start,
                diffuse: d.diffuse,
                specular: d.specular,
                normal: d.normal,
                group_index: draws.len(),
                ..Default::default()
            });
        }
        for (h, t) in &md.textures {
            textures.entry(*h).or_insert_with(|| t.clone());
        }
    }
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in &verts {
        for k in 0..3 {
            bmin[k] = bmin[k].min(v.pos[k]);
            bmax[k] = bmax[k].max(v.pos[k]);
        }
    }
    let stats = mesh::ModelStats {
        meshes: draws.len(),
        vertices: verts.len(),
        skipped: 0,
        bbox_min: bmin,
        bbox_max: bmax,
        fit_center: [0.0; 3],
        fit_scale: 1.0,
        bones: Vec::new(),
        rig: Vec::new(),
        prelit: false,
    };
    let mut skin = mesh::SkinData::identity();
    skin.center = [0.0; 3];
    skin.scale = 1.0;
    Ok(ModelData {
        verts,
        indices,
        draws,
        stats,
        skin,
        textures,
        tiers: Vec::new(),
        tier: 0x01,
        machine: None,
        hier_nodes: Vec::new(),
        indx: Vec::new(),
        header: None,
    })
}

/// F10 export. For a WAD asset this writes the LOSSLESS RIGGED bundle — skeleton, joints, skin
/// weights, inverse-bind matrices, and the character's full clip set as glTF animations (via
/// [`export_bundle_by_hash`]) — so a re-import lands with its rig intact, not stripped to static
/// geometry. Imports and merge results have no LOD-rung container to bundle, so they fall back to
/// the OBJ + textures writer ([`export_model_data`]).
fn export_preview(
    w: &mut WadStack,
    base: &str,
    overlays: &[String],
    imported: &HashMap<u32, ModelData>,
    index: &AssetIndex,
    p: &Preview,
) -> Result<String, String> {
    // No raw rung bytes for an import/merge — the bundle path can't run, so OBJ it is.
    if imported.contains_key(&p.hash) {
        let md = source_model_data(w, imported, p.hash)?;
        return export_model_data(&md, &p.label);
    }
    export_bundle_by_hash(base, overlays, p.hash, &p.label, index, std::path::Path::new("workshop_export"))
}

/// Headless OBJ export (`--export <name|0xHASH>`): resolve the model through a fresh stack and
/// write it out as OBJ + textures. The rigged sibling is `--export-bundle` (and the in-app F10).
pub(crate) fn export_by_hash(
    base: &str,
    overlays: &[String],
    hash: u32,
    label: &str,
) -> Result<String, String> {
    let mut w = WadStack::open(base, overlays)?;
    let md = load_model_data(&mut w, hash)?;
    export_model_data(&md, label)
}

/// LOSSLESS bundle export: editable glTF (+PNG skins) alongside the ORIGINAL container bytes of
/// every LOD rung, plus the manifest that maps one onto the other. See `bundle.rs` — the point is
/// that chunks we have not reversed (PHY2/CHDR/CEXE/SWIT/...) survive byte-exact, so the asset can
/// always be rebuilt even where our understanding is incomplete.
pub(crate) fn export_bundle_by_hash(
    base: &str,
    overlays: &[String],
    hash: u32,
    label: &str,
    // The name catalog — also the source of each clip's procedural (ActionTable-derived) name.
    index: &AssetIndex,
    outroot: &std::path::Path,
) -> Result<String, String> {
    let mut w = WadStack::open(base, overlays)?;
    let md = load_model_data(&mut w, hash)?;
    // The LOD chain, raw. Resolution walks the whole open stack (base + patch overlays).
    let mut lods: Vec<mercs2_engine::wad::ModelLod> = Vec::new();
    for wad in w.wads.iter_mut().rev() {
        if let Ok(l) = mercs2_engine::wad::extract_model_lods(wad, hash) {
            lods = l;
            break;
        }
    }
    if lods.is_empty() {
        return Err(format!("no model container for 0x{hash:08X}"));
    }
    // The character's FULL animation set from the AnimationLookup tables (~100 for Mattias), not the
    // 6-clip working set the preview's generic loader is capped at.
    let (clips, clip_names) = clips_for_export(&mut w, label, &md.skin.rig, index);
    let safe: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let dir = outroot.join(&safe);
    crate::bundle::export_bundle(
        &dir,
        label,
        hash,
        &lods,
        &md.verts,
        &md.indices,
        &md.draws,
        &md.textures,
        &md.hier_nodes,
        md.header.as_ref(),
        &md.skin.rig,
        &clips,
        &clip_names,
        &index.names,
    )?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Handle to an in-flight background bundle export (poll `rx` once per frame). Mirrors
/// [`crate::publish::Publisher`].
pub(crate) struct Exporter {
    pub rx: std::sync::mpsc::Receiver<Result<String, String>>,
    /// The asset being exported — for the progress window and the completion message.
    pub label: String,
}

/// Run a WAD-asset bundle export off the UI thread. The bundle path opens its OWN wad stack and
/// touches neither the app's live stack nor the GPU, so it is safe to hand to a worker — and it
/// MUST be, because decoding a character's full clip set (~100 for Mattias/Jen) takes seconds and
/// blocked the event loop, painting the window "Not Responding" until it finished.
pub(crate) fn export_bundle_in_background(
    base: String,
    overlays: Vec<String>,
    hash: u32,
    label: String,
    index: AssetIndex,
    outroot: std::path::PathBuf,
) -> Exporter {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_label = label.clone();
    std::thread::spawn(move || {
        let _ = tx.send(export_bundle_by_hash(&base, &overlays, hash, &worker_label, &index, &outroot));
    });
    Exporter { rx, label }
}

/// Write a model to `workshop_export/<label>/` as OBJ + MTL + decoded PNG textures (game-space
/// coordinates, one `usemtl` per draw group). Works for WAD assets, imports, and merge results —
/// the hand-off point into the UCFX/patch pipeline.
fn export_model_data(md: &ModelData, label: &str) -> Result<String, String> {
    let safe: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let dir = std::path::PathBuf::from("workshop_export").join(&safe);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Textures → PNG (decoded from BC), MTL materials per draw group. Every slot the material binds
    // is written, not just the diffuse: the normal (`_nm`) and specular (`_sm`) maps are half the
    // look of these assets and a modder can't re-author what the export never handed them.
    let mut mtl = String::new();
    let mut written: HashMap<u32, String> = HashMap::new();
    let mut emit = |h: u32, dir: &std::path::Path| -> Result<Option<String>, String> {
        if let Some(n) = written.get(&h) {
            return Ok(Some(n.clone()));
        }
        let Some(td) = md.textures.get(&h) else { return Ok(None) };
        let name = format!("tex_0x{h:08X}.png");
        // Decoded dims, NOT the declared ones — a texture whose higher mips never streamed covers
        // only a smaller surface, and writing it at the declared size yields a mostly-empty plate.
        let (w, h_px, rgba) = crate::texpng::decode_bc(td);
        crate::texpng::write_png(dir.join(&name).to_str().unwrap_or(&name), w, h_px, &rgba)?;
        written.insert(h, name.clone());
        Ok(Some(name))
    };
    for (gi, d) in md.draws.iter().enumerate() {
        mtl.push_str(&format!("newmtl m{gi}\nKd 1 1 1\n"));
        for (key, slot) in [("map_Kd", d.diffuse), ("map_Bump", d.normal), ("map_Ks", d.specular)] {
            if let Some(h) = slot {
                if let Some(name) = emit(h, &dir)? {
                    mtl.push_str(&format!("{key} {name}\n"));
                }
            }
        }
        mtl.push('\n');
    }
    std::fs::write(dir.join("model.mtl"), mtl).map_err(|e| e.to_string())?;

    // OBJ: shared vertex block, faces per group (1-based indices, game space verbatim).
    let mut obj = String::with_capacity(md.verts.len() * 48);
    obj.push_str("mtllib model.mtl\n");
    for v in &md.verts {
        obj.push_str(&format!("v {} {} {}\n", v.pos[0], v.pos[1], v.pos[2]));
    }
    for v in &md.verts {
        obj.push_str(&format!("vt {} {}\n", v.uv[0], 1.0 - v.uv[1]));
    }
    for v in &md.verts {
        obj.push_str(&format!("vn {} {} {}\n", v.normal[0], v.normal[1], v.normal[2]));
    }
    // One OBJ object per LOD rung. The rungs REFINE each other — the same HIER node is re-authored
    // at each detail level, so the resident block's 736-triangle van body and P001's 9,360-triangle
    // version occupy the same space. Emitting them flat stacked every detail level on top of the
    // others. Nothing is dropped: each rung becomes its own `o LOD<n>` object (0 = resident/coarsest)
    // that a modeller can isolate, hide, or edit independently.
    let mut rungs: Vec<u8> = md.draws.iter().map(|d| d.rung).collect();
    rungs.sort_unstable();
    rungs.dedup();
    for rung in rungs {
        obj.push_str(&format!("o LOD{rung}\n"));
        for (gi, d) in md.draws.iter().enumerate().filter(|(_, d)| d.rung == rung) {
            obj.push_str(&format!("g LOD{rung}_group{gi}_seg{}_node{}\nusemtl m{gi}\n", d.seg_id, d.node));
            let s = d.index_start as usize;
            let e = ((d.index_start + d.index_count) as usize).min(md.indices.len());
            for tri in md.indices[s..e].chunks_exact(3) {
                let (a, b, c) = (tri[0] + 1, tri[1] + 1, tri[2] + 1);
                obj.push_str(&format!("f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}\n"));
            }
        }
    }
    std::fs::write(dir.join("model.obj"), obj).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}
