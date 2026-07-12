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
        Ok(WadStack { wads, labels, registry: Default::default() })
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

    /// Full streamed resolution when available (plate view).
    pub fn texture_best(&mut self, hash: u32) -> Result<mercs2_formats::texture::TextureData, String> {
        let mut last = format!("0x{hash:08X}: not in any open wad");
        for i in (0..self.wads.len()).rev() {
            let w = &mut self.wads[i];
            match wad::extract_texture_hires(w, hash).or_else(|_| wad::extract_texture(w, hash)) {
                Ok(t) => return Ok(t),
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
    /// HIER bone name-hashes (clip binding input).
    hier: Vec<u32>,
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
}

/// Top-level UI page. The Model Workbench is a full dedicated page (its own inventory + tools
/// panels + viewport), NOT a section of the asset browser.
#[derive(PartialEq, Clone, Copy)]
enum WorkMode {
    Browser,
    Workbench,
}

/// Model Workbench vehicle-class display order (helicopters first, per user).
const VEH_CLASS_ORDER: &[&str] = &[
    "helicopter", "tank", "apc", "vtol", "jet", "car", "truck", "van", "semi", "trailer", "towed",
    "motorcycle", "boat", "other",
];

/// Group the catalog's vehicle models by class for the workbench inventory.
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
    // Model Workbench page: its own mode + vehicle inventory (rebuilt when names/overlays change).
    let mut mode = WorkMode::Browser;
    let mut vehicle_inventory: Vec<(&'static str, Vec<(u32, String)>)> = build_vehicle_inventory(&index);
    let mut inventory_dirty = false;
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
        index
            .rows(kind)
            .iter()
            .enumerate()
            .filter(|(_, r)| f.is_empty() || r.label().to_ascii_lowercase().contains(&f))
            .map(|(i, _)| i)
            .collect()
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
                    match crate::import::import_model(&path) {
                        Ok(im) => {
                            let stem = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("import")
                                .to_string();
                            let label = format!("import_{stem}");
                            let hash = mercs2_formats::hash::pandemic_hash_m2(&label);
                            let md: ModelData = im.into();
                            imported.insert(hash, md.clone());
                            scene.unload_model(hash); // re-drop of the same file replaces it
                            let p = build_preview(
                                &mut w, &mut scene, &mut world, hash, label, md, &preview,
                                &placed, &index, &anim_sel, &lua_corpus,
                            );
                            cam_target = p.center;
                            cam_dist = (p.radius * 2.4).clamp(0.5, 15000.0);
                            status = format!(
                                "imported {} — {} verts, {} groups, {} textures (F6 place, F10 export)",
                                p.label,
                                p.verts,
                                p.draws.len(),
                                p.tex_hashes.len()
                            );
                            sel_bone = None;
                            preview = Some(p);
                        }
                        Err(e) => status = format!("IMPORT FAILED: {e}"),
                    }
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
                                p.clip_cache.insert(done.hash, done.clip);
                                if !bound
                                    && p.cur_clip.map(|ci| p.clip_catalog[ci].hash)
                                        == Some(done.hash)
                                {
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
                                let mats = pose::havok_palette_in_place(
                                    &p.rig,
                                    &sample,
                                    &ca.track_to_hier,
                                    ca.num_transform_tracks,
                                );
                                let _ = world.insert_one(p.entity, SkinPalette { mats });
                            }
                        }
                    }

                    // ── The inspector GUI: toolbar, browser, Details panel, texture window.
                    // Widgets queue `Act`s; the processor below executes them. ──
                    let mut hovered_bone: Option<usize> = None;
                    if inventory_dirty {
                        vehicle_inventory = build_vehicle_inventory(&index);
                        inventory_dirty = false;
                    }
                    gui.run(|ctx| {
                        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.strong("Mercenaries 2 — Workshop");
                                ui.separator();
                                // Page switch: asset Browser <-> the Model Workbench.
                                ui.selectable_value(&mut mode, WorkMode::Browser, "Browser");
                                ui.selectable_value(&mut mode, WorkMode::Workbench, "Model Workbench");
                                ui.separator();
                                if ui.add_enabled(preview.is_some(), egui::Button::new("Place"))
                                    .on_hover_text("Add the preview to the sandbox (F6)")
                                    .clicked()
                                {
                                    actions.push(Act::Place);
                                }
                                if ui.add_enabled(!placed.is_empty(), egui::Button::new("Merge"))
                                    .on_hover_text("Bake all placed instances into one model (F7)")
                                    .clicked()
                                {
                                    actions.push(Act::Merge);
                                }
                                if ui.add_enabled(preview.is_some(), egui::Button::new("Export"))
                                    .on_hover_text("OBJ + MTL + PNGs -> workshop_export/ (F10)")
                                    .clicked()
                                {
                                    actions.push(Act::Export);
                                }
                                ui.separator();
                                if ui.button("Save scene").clicked() {
                                    actions.push(Act::SaveScene);
                                }
                                if ui.button("Load scene").clicked() {
                                    actions.push(Act::LoadScene);
                                }
                                if ui.add_enabled(!placed.is_empty(), egui::Button::new("Clear"))
                                    .clicked()
                                {
                                    actions.push(Act::ClearSandbox);
                                }
                                ui.separator();
                                ui.weak("drop .obj/.gltf/.glb to import");
                            });
                        });
                        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                if names_pending {
                                    ui.spinner();
                                    ui.weak("loading name corpora…");
                                    ui.separator();
                                }
                                ui.label(status.as_str());
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
                        // ── BROWSER PAGE: the asset browser + details inspector. ──
                        if mode == WorkMode::Browser {
                        egui::SidePanel::left("browser").default_width(300.0).show(ctx, |ui| {
                            let before = (kind, filter.clone());
                            ui.horizontal(|ui| {
                                ui.selectable_value(
                                    &mut kind,
                                    Kind::Model,
                                    format!("Models ({})", index.models.len()),
                                );
                                ui.selectable_value(
                                    &mut kind,
                                    Kind::Texture,
                                    format!("Textures ({})", index.textures.len()),
                                );
                            });
                            ui.add(
                                egui::TextEdit::singleline(&mut filter)
                                    .hint_text("filter…")
                                    .desired_width(f32::INFINITY),
                            );
                            if before.0 != kind || before.1 != filter {
                                filtered = refilter(&index, kind, &filter);
                                sel = 0;
                            }
                            ui.separator();
                            let row_h = ui.text_style_height(&egui::TextStyle::Body);
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(
                                ui,
                                row_h,
                                filtered.len(),
                                |ui, range| {
                                    for vi in range {
                                        let r = &index.rows(kind)[filtered[vi]];
                                        let (hash, label) = (r.hash, r.label());
                                        let mark = if r.src > 0 { "+ " } else { "" };
                                        let row = ui
                                            .selectable_label(vi == sel, format!("{mark}{label}"));
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
                        });
                        egui::SidePanel::right("details").default_width(360.0).show(ctx, |ui| {
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                match &mut preview {
                                    None => {
                                        ui.weak("No model loaded — click one in the browser.");
                                    }
                                    Some(p) => {
                                        let (phash, plabel) = (p.hash, p.label.clone());
                                        let head = ui.heading(plabel.as_str());
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
                                        egui::CollapsingHeader::new("Info")
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                egui::Grid::new("info_grid")
                                                    .num_columns(2)
                                                    .show(ui, |ui| {
                                                        ui.label("hash");
                                                        ui.monospace(format!("0x{:08X}", p.hash));
                                                        ui.end_row();
                                                        ui.label("verts / tris");
                                                        ui.label(format!("{} / {}", p.verts, p.tris));
                                                        ui.end_row();
                                                        ui.label("draw groups");
                                                        ui.label(format!(
                                                            "{} ({} hidden)",
                                                            p.draws.len(),
                                                            p.hidden.len()
                                                        ));
                                                        ui.end_row();
                                                        ui.label("bones");
                                                        ui.label(p.rig.len().to_string());
                                                        ui.end_row();
                                                        ui.label("textures");
                                                        ui.label(p.tex_hashes.len().to_string());
                                                        ui.end_row();
                                                        ui.label("radius");
                                                        ui.label(format!("{:.2} m", p.radius));
                                                        ui.end_row();
                                                    });
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
                                            egui::CollapsingHeader::new(format!(
                                                "LOD  —  view_state 0x{:02X}  ({} of {} meshes pass)",
                                                p.tier,
                                                drawn_at(p.tier),
                                                p.draws.len()
                                            ))
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    ui.label("view_state bits:");
                                                    for b in 0..8u8 {
                                                        let bit = 1u8 << b;
                                                        let mut on = (p.tier & bit) != 0;
                                                        if ui
                                                            .checkbox(&mut on, format!("{b}"))
                                                            .on_hover_text(format!(
                                                                "bit {b} (0x{bit:02X}) — a mesh draws if                                                                  its mask shares ANY bit with view_state"
                                                            ))
                                                            .changed()
                                                        {
                                                            actions.push(Act::Tier(p.tier ^ bit));
                                                        }
                                                    }
                                                });
                                                ui.separator();
                                                ui.label("masks this model actually carries:");
                                                for (&mask, &n) in &by_mask {
                                                    let hit = (p.tier & mask) != 0;
                                                    let label = format!(
                                                        "0x{mask:02X}   {n} mesh{}   {}",
                                                        if n == 1 { "" } else { "es" },
                                                        if hit { "✔ passes" } else { "✖ filtered out" }
                                                    );
                                                    if ui
                                                        .selectable_label(hit, label)
                                                        .on_hover_text("click to set view_state to exactly this mask")
                                                        .clicked()
                                                    {
                                                        actions.push(Act::Tier(mask));
                                                    }
                                                }
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
                                            egui::CollapsingHeader::new(format!(
                                                "Destruction — engine state machine ({} nodes, {nstates} states)",
                                                sm.nodes.len()
                                            ))
                                            .default_open(true)
                                            .show(ui, |ui| {
                                                // HEALTH drives the machine — the object's real damage
                                                // axis. Full = pristine; dropping = damaged/on-fire;
                                                // 0 = wreck. Below, the per-node states it resolves to.
                                                let mut hp = p.health * 100.0;
                                                ui.horizontal(|ui| {
                                                    ui.label("Health");
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
                                                    if ui.small_button("100").clicked() {
                                                        actions.push(Act::SetHealth(1.0));
                                                    }
                                                    if ui.small_button("0").clicked() {
                                                        actions.push(Act::SetHealth(0.0));
                                                    }
                                                });
                                                ui.weak(
                                                    "drives the state machine from damage (HP→messages approximated; states are the engine's)",
                                                );
                                                let hier: std::collections::HashSet<u32> =
                                                    p.hier.iter().copied().collect();
                                                let resolve = |h: u32| {
                                                    let tag = if hier.contains(&h) { "@" } else { "" };
                                                    format!("{tag}{}", name_or_hash(&index, h))
                                                };
                                                egui::ScrollArea::vertical()
                                                    .max_height(300.0)
                                                    .id_source("machine_scroll")
                                                    .show(ui, |ui| {
                                                        for (ni, node) in sm.nodes.iter().enumerate() {
                                                            ui.strong(format!(
                                                                "node {}",
                                                                name_or_hash(&index, node.name_hash)
                                                            ));
                                                            ui.horizontal_wrapped(|ui| {
                                                                for (si, st) in
                                                                    node.states.iter().enumerate()
                                                                {
                                                                    let cur = p
                                                                        .node_state
                                                                        .get(ni)
                                                                        .copied()
                                                                        .unwrap_or(0);
                                                                    if ui
                                                                        .selectable_label(
                                                                            cur == si,
                                                                            name_or_hash(
                                                                                &index,
                                                                                st.name_hash,
                                                                            ),
                                                                        )
                                                                        .clicked()
                                                                    {
                                                                        actions.push(Act::NodeState(
                                                                            ni, si,
                                                                        ));
                                                                    }
                                                                }
                                                            });
                                                            let cur = p
                                                                .node_state
                                                                .get(ni)
                                                                .copied()
                                                                .unwrap_or(0);
                                                            if let Some(st) = node.states.get(cur) {
                                                                let enter = mercs2_formats::orchestrator::decode_script(&st.enter, resolve);
                                                                if !enter.is_empty() {
                                                                    ui.weak(format!("  enter: {enter}"));
                                                                }
                                                                let exit = mercs2_formats::orchestrator::decode_script(&st.exit, resolve);
                                                                if !exit.is_empty() {
                                                                    ui.weak(format!("  exit: {exit}"));
                                                                }
                                                            }
                                                        }
                                                        ui.weak("@name = HIER node of this model");
                                                    });
                                            });
                                        }
                                        egui::CollapsingHeader::new(format!(
                                            "Segments  \u{2014}  {} of {} drawn",
                                            p.draws.len() - p.hidden.len(),
                                            p.draws.len()
                                        ))
                                        .default_open(true)
                                        .show(ui, |ui| {
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
                                                .max_height(260.0)
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
                                                        ui.horizontal(|ui| {
                                                            let mut vis = !p.hidden.contains(&gi);
                                                            if ui.checkbox(&mut vis, "").changed() {
                                                                actions.push(Act::GroupToggle(gi));
                                                            }
                                                            let row = ui.selectable_label(
                                                                p.sel_group == gi,
                                                                format!(
                                                                    "{mark} {gi:2} seg{:3} node {:>3} {}  mask 0x{:02X}  {tris:5} tri  {tex}",
                                                                    d.seg_id, d.node, node_lbl, d.lod_mask
                                                                ),
                                                            )
                                                            .on_hover_text(&why);
                                                            if row.clicked() {
                                                                p.sel_group = gi;
                                                            }
                                                            row.context_menu(|ui| {
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
                                                        });
                                                    }
                                                });
                                        });
                                        let src = p
                                            .character_set
                                            .as_deref()
                                            .map(|c| format!("character:{c}"))
                                            .unwrap_or_else(|| "generic".into());
                                        egui::CollapsingHeader::new(format!(
                                            "Animation ({}) [{src}]",
                                            p.clip_catalog.len()
                                        ))
                                        .show(ui, |ui| {
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
                                                .max_height(200.0)
                                                .id_source("clip_scroll")
                                                .show(ui, |ui| {
                                                    for i in 0..p.clip_catalog.len() {
                                                        let e = &p.clip_catalog[i];
                                                        let (chash, chandle, clabel) = (
                                                            e.hash,
                                                            e.handles.first().copied(),
                                                            e.label.clone(),
                                                        );
                                                        let state = match p.clip_cache.get(&chash) {
                                                            Some(Some(c)) => {
                                                                format!("  ({:.2}s)", c.clip.duration)
                                                            }
                                                            Some(None) => "  (unbound)".into(),
                                                            None => String::new(),
                                                        };
                                                        // Row = hash, then the name to its right:
                                                        // corpus name or the deterministic
                                                        // procedural table-name (nothing when
                                                        // unnamed — never the hex twice).
                                                        let name = e
                                                            .name
                                                            .as_ref()
                                                            .map(|n| format!("  {n}"))
                                                            .unwrap_or_default();
                                                        let mut row = ui.selectable_label(
                                                            p.cur_clip == Some(i),
                                                            egui::RichText::new(format!(
                                                                "0x{chash:08X}{name}{state}"
                                                            ))
                                                            .monospace(),
                                                        );
                                                        if let Some(h) = chandle {
                                                            row = row.on_hover_text(format!(
                                                                "ActionTable Handle 0x{h:08X}"
                                                            ));
                                                        }
                                                        if row.clicked() {
                                                            actions.push(Act::ClipSel(i));
                                                        }
                                                        row.context_menu(|ui| {
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
                                        });
                                        // Game scripts that mention this asset — literal corpus
                                        // search hits (the needle used is shown; decompiled Lua
                                        // is game data).
                                        if !p.lua_refs.is_empty() {
                                            egui::CollapsingHeader::new(format!(
                                                "Game scripts mentioning \"{}\" ({})",
                                                p.lua_needle,
                                                p.lua_refs.len()
                                            ))
                                            .show(ui, |ui| {
                                                egui::ScrollArea::vertical()
                                                    .max_height(220.0)
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
                                            });
                                        }
                                        egui::CollapsingHeader::new(format!(
                                            "Skeleton ({})",
                                            p.rig.len()
                                        ))
                                        .show(ui, |ui| {
                                            ui.weak("hover = highlight in view, click = pin");
                                            egui::ScrollArea::vertical()
                                                .max_height(240.0)
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
                                                        let row = ui.selectable_label(
                                                            sel_bone == Some(i),
                                                            egui::RichText::new(format!(
                                                                "{}{i:3} {bname}",
                                                                "  ".repeat(depth),
                                                            ))
                                                            .monospace(),
                                                        );
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
                                        });
                                    }
                                }
                                egui::CollapsingHeader::new(format!("Sandbox ({})", placed.len()))
                                    .default_open(!placed.is_empty())
                                    .show(ui, |ui| {
                                        for i in 0..placed.len() {
                                            let mut changed = false;
                                            ui.horizontal(|ui| {
                                                let (phash, plabel) =
                                                    (placed[i].hash, placed[i].label.clone());
                                                let row = ui.selectable_label(
                                                    sel_placed == Some(i),
                                                    plabel.as_str(),
                                                );
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
                                                    if ui.button("Copy name").clicked() {
                                                        ui.ctx().copy_text(plabel.clone());
                                                        ui.close_menu();
                                                    }
                                                    if ui
                                                        .button(format!("Copy hash 0x{phash:08X}"))
                                                        .clicked()
                                                    {
                                                        ui.ctx().copy_text(format!("0x{phash:08X}"));
                                                        ui.close_menu();
                                                    }
                                                });
                                                if ui.small_button("✖").clicked() {
                                                    actions.push(Act::RemovePlaced(i));
                                                }
                                            });
                                            ui.horizontal(|ui| {
                                                let pl = &mut placed[i];
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut pl.pos.x).speed(0.05).prefix("x "))
                                                    .changed();
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut pl.pos.y).speed(0.05).prefix("y "))
                                                    .changed();
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut pl.pos.z).speed(0.05).prefix("z "))
                                                    .changed();
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut pl.yaw).speed(0.01).prefix("yaw "))
                                                    .changed();
                                                changed |= ui
                                                    .add(egui::DragValue::new(&mut pl.scale).speed(0.01).prefix("s "))
                                                    .changed();
                                            });
                                            if changed {
                                                actions.push(Act::SyncPlaced(i));
                                            }
                                            ui.separator();
                                        }
                                    });
                                // ── Mod project: queue NOVEL new-hash assets and publish them
                                // as a patch WAD (docs/modernization/workshop_publish_pipeline.md
                                // M3). Flow: preview/select the donor model → drag-drop the
                                // import → name it → Add → Publish. ──
                                egui::CollapsingHeader::new(format!(
                                    "Mod project ({})",
                                    mod_items.len()
                                ))
                                .show(ui, |ui| {
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
                                        ui.label("scale");
                                        ui.add(egui::DragValue::new(&mut conform_scale).speed(0.005).range(0.0001..=1000.0));
                                        if ui.small_button("reset").clicked() {
                                            conform_scale = 1.0;
                                            conform_t = [0.0; 3];
                                            conform_r = [0.0; 3];
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("pos");
                                        ui.add(egui::DragValue::new(&mut conform_t[0]).speed(0.02).prefix("x "));
                                        ui.add(egui::DragValue::new(&mut conform_t[1]).speed(0.02).prefix("y "));
                                        ui.add(egui::DragValue::new(&mut conform_t[2]).speed(0.02).prefix("z "));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("rot°");
                                        ui.add(egui::DragValue::new(&mut conform_r[0]).speed(1.0).prefix("x "));
                                        ui.add(egui::DragValue::new(&mut conform_r[1]).speed(1.0).prefix("y "));
                                        ui.add(egui::DragValue::new(&mut conform_r[2]).speed(1.0).prefix("z "));
                                    });
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
                            });
                        });
                        } // ── end BROWSER PAGE ──

                        // ── MODEL WORKBENCH PAGE: inventory (left) · viewport (centre) · tools
                        // (right). A dedicated page, not a section of the browser. ──
                        if mode == WorkMode::Workbench {
                            egui::SidePanel::left("wb_inventory").default_width(280.0).show(ctx, |ui| {
                                ui.heading("Model Workbench");
                                ui.weak("Pick a vehicle template to inspect its nodes, then conform your own model onto it.");
                                ui.separator();
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    if vehicle_inventory.is_empty() {
                                        ui.weak("(loading catalog…)");
                                    }
                                    for (class, rows) in &vehicle_inventory {
                                        egui::CollapsingHeader::new(format!("{class}  ({})", rows.len()))
                                            .default_open(*class == "helicopter")
                                            .show(ui, |ui| {
                                                for (hash, label) in rows {
                                                    let sel = preview.as_ref().is_some_and(|p| p.hash == *hash);
                                                    if ui.selectable_label(sel, label).clicked() {
                                                        actions.push(Act::LoadModelHash(*hash, label.clone()));
                                                    }
                                                }
                                            });
                                    }
                                });
                            });
                            egui::SidePanel::right("wb_tools").default_width(350.0).show(ctx, |ui| {
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    ui.heading("Template");
                                    match &preview {
                                        Some(p) => {
                                            ui.label(format!("{}", p.label));
                                            ui.monospace(format!("0x{:08X}", p.hash));
                                            ui.label(format!(
                                                "{} nodes · {} draw groups · {} textures",
                                                p.rig.len(), p.draws.len(), p.tex_hashes.len()
                                            ));
                                            ui.checkbox(&mut show_nodes, "show node markers")
                                                .on_hover_text("green = positioned attach node (rotor/skid/seat/tail/hardpoint) · grey = origin/structural");
                                            ui.weak("green = attach node (rotor/skid/seat) · grey = structural");
                                        }
                                        None => {
                                            ui.weak("← pick a vehicle from the inventory");
                                        }
                                    }
                                    ui.separator();

                                    ui.heading("Your model");
                                    ui.weak("drag-drop .obj / .gltf / .glb onto the window");
                                    let import_on_pedestal =
                                        preview.as_ref().is_some_and(|p| imported.contains_key(&p.hash));
                                    if import_on_pedestal {
                                        ui.colored_label(egui::Color32::from_rgb(120, 230, 140), "import loaded on pedestal");
                                    }
                                    ui.horizontal(|ui| {
                                        if ui.button("Load donor ref")
                                            .on_hover_text("place the conform donor template in the sandbox at origin as a visual anchor")
                                            .clicked() { actions.push(Act::LoadDonorRef); }
                                        if ui.button("Auto-fit")
                                            .on_hover_text("seed scale + position from the donor's real geometry envelope (skids on ground, centred)")
                                            .clicked() { actions.push(Act::ConformAutofit); }
                                        ui.checkbox(&mut conform_live, "live");
                                    });
                                    ui.separator();

                                    ui.heading("Conform transform");
                                    ui.horizontal(|ui| {
                                        ui.label("scale");
                                        ui.add(egui::DragValue::new(&mut conform_scale).speed(0.005).range(0.0001..=1000.0));
                                        if ui.small_button("reset").clicked() {
                                            conform_scale = 1.0; conform_t = [0.0; 3]; conform_r = [0.0; 3];
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("pos");
                                        ui.add(egui::DragValue::new(&mut conform_t[0]).speed(0.02).prefix("x "));
                                        ui.add(egui::DragValue::new(&mut conform_t[1]).speed(0.02).prefix("y "));
                                        ui.add(egui::DragValue::new(&mut conform_t[2]).speed(0.02).prefix("z "));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("rot°");
                                        ui.add(egui::DragValue::new(&mut conform_r[0]).speed(1.0).prefix("x "));
                                        ui.add(egui::DragValue::new(&mut conform_r[1]).speed(1.0).prefix("y "));
                                        ui.add(egui::DragValue::new(&mut conform_r[2]).speed(1.0).prefix("z "));
                                    });
                                    ui.checkbox(&mut conform_flip, "flip winding on export (fix inside-out faces)");
                                    ui.separator();

                                    ui.heading("Export");
                                    ui.horizontal(|ui| {
                                        ui.label("donor:");
                                        match &mod_donor {
                                            Some((h, l)) => { ui.monospace(format!("0x{h:08X}")); ui.label(l.as_str()); }
                                            None => { ui.weak("← click a template (sets donor)"); }
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("host group:");
                                        ui.add(egui::DragValue::new(&mut mod_group).range(0..=63));
                                    });
                                    ui.horizontal(|ui| {
                                        ui.label("name:");
                                        ui.text_edit_singleline(&mut mod_name);
                                    });
                                    let can_add = import_on_pedestal && mod_donor.is_some() && !mod_name.is_empty();
                                    if ui.add_enabled(can_add, egui::Button::new("Add to mod project")).clicked() {
                                        actions.push(Act::ModAdd(mod_name.clone()));
                                    }
                                    if !can_add {
                                        ui.weak("needs an imported model on the pedestal, a donor template, and a name");
                                    }
                                    for (i, it) in mod_items.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.monospace(format!("0x{:08X}", it.hash));
                                            ui.label(it.name.as_str());
                                            ui.weak(format!("← {} g{}", it.donor_label, it.target_group));
                                            if ui.small_button("✖").clicked() { actions.push(Act::ModRemove(i)); }
                                        });
                                    }
                                    ui.horizontal(|ui| {
                                        ui.label("output:");
                                        ui.text_edit_singleline(&mut mod_out);
                                    });
                                    let busy = publisher.is_some();
                                    if ui.add_enabled(!mod_items.is_empty() && !busy,
                                        egui::Button::new(if busy { "publishing…" } else { "Publish patch WAD" })).clicked()
                                    {
                                        actions.push(Act::Publish);
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
                                    status = match export_preview(&mut w, &imported, p) {
                                        Ok(dir) => format!("exported {} -> {dir}", p.label),
                                        Err(e) => format!("EXPORT FAILED: {e}"),
                                    };
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
                            // Workbench: every node as a spatial anchor (after the hover/pin closure
                            // releases its &mut cards). Colour by role heuristic — translated-away
                            // nodes = functional attach points (rotor/skid/seat/tail/hardpoint);
                            // nodes at the origin = structural/break-piece parents — so the user can
                            // map imported geometry onto them by sight.
                            if show_nodes {
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
                        }
                        scene.set_glow_cards(&cards);
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
                    let r = if tex_view.is_some() || (preview.is_none() && placed.is_empty()) {
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
struct ModelData {
    verts: Vec<mesh::Vertex>,
    indices: Vec<u32>,
    draws: Vec<DrawGroup>,
    stats: mesh::ModelStats,
    skin: mesh::SkinData,
    textures: TexMap,
    /// SEGM state/LOD tier bits the container carries (F11 cycles them; empty = single-tier /
    /// imported) and the bit this build used.
    tiers: Vec<u8>,
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

fn load_model_data(w: &mut WadStack, hash: u32) -> Result<ModelData, String> {
    load_model_data_tier(w, hash, 0x01)
}

/// The render state a freshly-placed instance of `hash` should have: LOD rung 0, plus the destruction
/// machine's node-enable table at its DEFAULT state (pristine). Every spawned entity needs one —
/// models are uploaded whole now, so an entity with no state draws its wreck too.
///
/// Imported/synthetic models resolve to no container; their draw groups carry `lod_mask = 0xFF` and
/// `node = -1`, so the empty state draws them in full.
fn default_render_state(w: &mut WadStack, hash: u32) -> mercs2_engine::render_state::RenderState {
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
    // RESIDENT mips only: `extract_texture_hires` walks the whole cell subtree per texture and
    // made model loads take seconds — the F3 plate view still fetches full-res on demand.
    let mut textures: TexMap = HashMap::new();
    for d in &draws {
        for h in [d.diffuse, d.normal, d.specular].into_iter().flatten() {
            if !textures.contains_key(&h) {
                if let Ok(t) = w.texture_resident(h) {
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
    let bind: Vec<[[f32; 4]; 4]> =
        if md.skin.bones.is_empty() { vec![IDENTITY] } else { md.skin.bones.clone() };
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

/// Export the preview's geometry + textures (via [`export_model_data`]).
fn export_preview(
    w: &mut WadStack,
    imported: &HashMap<u32, ModelData>,
    p: &Preview,
) -> Result<String, String> {
    let md = source_model_data(w, imported, p.hash)?;
    export_model_data(&md, &p.label)
}

/// Headless export entry (`--export <name|0xHASH>`): resolve the model through a fresh stack
/// and write it out — same code path as the in-app F10.
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

    // Textures → PNG (decoded from BC), MTL materials per draw group.
    let mut mtl = String::new();
    for (gi, d) in md.draws.iter().enumerate() {
        mtl.push_str(&format!("newmtl m{gi}\nKd 1 1 1\n"));
        if let Some(h) = d.diffuse {
            if let Some(td) = md.textures.get(&h) {
                let name = format!("tex_0x{h:08X}.png");
                let rgba = crate::texpng::decode_bc(td);
                crate::texpng::write_png(
                    dir.join(&name).to_str().unwrap_or(&name),
                    td.width,
                    td.height,
                    &rgba,
                )?;
                mtl.push_str(&format!("map_Kd {name}\n"));
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
    for (gi, d) in md.draws.iter().enumerate() {
        obj.push_str(&format!("g group{gi}\nusemtl m{gi}\n"));
        let s = d.index_start as usize;
        let e = ((d.index_start + d.index_count) as usize).min(md.indices.len());
        for tri in md.indices[s..e].chunks_exact(3) {
            let (a, b, c) = (tri[0] + 1, tri[1] + 1, tri[2] + 1);
            obj.push_str(&format!("f {a}/{a}/{a} {b}/{b}/{b} {c}/{c}/{c}\n"));
        }
    }
    std::fs::write(dir.join("model.obj"), obj).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}
