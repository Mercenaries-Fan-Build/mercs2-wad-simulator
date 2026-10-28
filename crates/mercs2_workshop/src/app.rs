//! The workshop window: a native, engine-rendered developer tool.
//!
//! One winit loop over the engine's `Scene` (the SAME renderer `mercs2_game` boots — materials,
//! skinning, lighting, shadow are all the faithful path, not an export approximation). A rail
//! switches the page; this file holds the rail, Inspect and the Retarget / Fit benches:
//!
//! - browse/search every model + texture in the WAD (registry names via `index::AssetIndex`)
//! - preview a model with its real materials/textures, orbit camera, animation clips
//! - inspect layers: HIER bone tree, per-material draw groups (isolate/hide sub-strips),
//!   full-screen texture plates
//! - use a model as a donor: a bench writes a new contribution on it, never the model itself
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
    /// `--wad`: the first step of the game-archive chain ([`crate::chains::game`]). The window
    /// resolves the chain itself, so a change made in Settings re-resolves it with the same flag.
    pub wad_flag: Option<String>,
    /// Load `vz-patch.wad` beside the base (no `--no-auto-patch`).
    pub auto_patch: bool,
    /// Every `--overlay`, in argument order. See [`overlays_for`].
    pub extra_overlays: Vec<String>,
    /// A Shipment folder to open in the Quartermaster dock at startup.
    ///
    /// The dock is otherwise only reachable by clicking, which makes it unverifiable from a script —
    /// and this project's standard is that a change is shown, not asserted.
    pub shipment: Option<PathBuf>,
    /// `--ui-state`: a state to open in and a frame to capture, then exit.
    pub ui_state: Option<crate::ui_state::UiState>,
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
    /// The game's other WADs beside the base ([`side_wad_files`]), opened read-only. The game loads
    /// each in its own slot, so they never shadow the stack: a source index `wads.len() + i` names
    /// `side[i]`, and resolution reads them only after the stack.
    pub side: Vec<wad::Wad>,
    /// `side[i]`'s path.
    pub side_labels: Vec<String>,
    /// Every side WAD looked for and what became of it — what Settings' "What it opened" lists.
    pub side_report: Vec<(String, SideWad)>,
}

/// What became of one side WAD ([`side_wad_files`]).
pub(crate) enum SideWad {
    /// Opened, from this path.
    Opened(String),
    /// No such file beside the base.
    Absent,
    /// Present but it would not open: the path and the error.
    Unreadable(String),
}

/// The game's own WADs beside `vz.wad` that Inspect reads besides the stack: the shell and loading
/// levels, then one per entry of the engine's language table (`.\Data\<token>.wad`,
/// `mercs2_quartermaster::manifest::Language`).
pub(crate) fn side_wad_files() -> Vec<String> {
    let mut out = vec!["shell.wad".to_string(), "Loading.wad".to_string()];
    out.extend(mercs2_quartermaster::manifest::Language::ALL.iter().map(|l| format!("{}.wad", l.token())));
    out
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
        let mut side = Vec::new();
        let mut side_labels = Vec::new();
        let mut side_report = Vec::new();
        for file in side_wad_files() {
            let found = match mercs2_quartermaster::sound::sibling_wad(std::path::Path::new(base), &file) {
                Ok(p) => p,
                Err(_) => {
                    side_report.push((file, SideWad::Absent));
                    continue;
                }
            };
            let path = found.to_string_lossy().into_owned();
            match wad::open(&path) {
                Ok(w) => {
                    side.push(w);
                    side_labels.push(path.clone());
                    side_report.push((file, SideWad::Opened(path)));
                }
                Err(e) => side_report.push((file, SideWad::Unreadable(format!("{path}: {e}")))),
            }
        }
        Ok(WadStack { wads, labels, registry: Default::default(), tex_cache: HashMap::new(), side, side_labels, side_report })
    }

    /// No archive open: the window's state before a boot brings one in.
    pub fn empty() -> WadStack {
        WadStack {
            wads: Vec::new(),
            labels: Vec::new(),
            registry: Default::default(),
            tex_cache: HashMap::new(),
            side: Vec::new(),
            side_labels: Vec::new(),
            side_report: Vec::new(),
        }
    }

    /// Short display tag for an asset's source wad: base = "", an overlay = "+<file stem>", a side
    /// WAD = " · <file stem>".
    pub fn tag(&self, src: usize) -> String {
        if src == 0 {
            return String::new();
        }
        let stem = |p: &str| {
            std::path::Path::new(p).file_stem().and_then(|s| s.to_str()).unwrap_or("wad").to_string()
        };
        match self.labels.get(src) {
            Some(l) => format!("+{}", stem(l)),
            None => match self.side_labels.get(src - self.labels.len()) {
                Some(l) => format!(" · {}", stem(l)),
                None => format!(" · source {src}"),
            },
        }
    }

    /// The path of source `src` (a stack WAD, then the side WADs).
    pub fn source_path(&self, src: usize) -> &str {
        self.labels
            .get(src)
            .or_else(|| self.side_labels.get(src.wrapping_sub(self.labels.len())))
            .map_or("", String::as_str)
    }

    /// Source `src`'s WAD (a stack WAD, then the side WADs).
    pub fn source_mut(&mut self, src: usize) -> Option<&mut wad::Wad> {
        let n = self.wads.len();
        if src < n {
            self.wads.get_mut(src)
        } else {
            self.side.get_mut(src - n)
        }
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

    /// A string table by asset name (`english`, `french`, …) as `key_hash -> text`, resolved through
    /// the residency registry like every other asset (overlays first). Keys are hashed by their bare
    /// name — look one up with `mercs2_formats::stringdb::key_hash`.
    pub fn string_table(&mut self, name: &str) -> Result<HashMap<u32, String>, String> {
        let name_hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let WadStack { wads, registry, .. } = self;
        let container = registry
            .resolve(wads, mercs2_formats::types::TYPE_HASH_STRINGDB, name_hash)
            .and_then(|c| registry.slice(c).map(<[u8]>::to_vec))
            .ok_or_else(|| format!("string table `{name}`: not in any open wad"))?;
        let keys = mercs2_formats::ucfx::extract_chunk_body(&container, b"KEYS")
            .ok_or_else(|| format!("string table `{name}`: no KEYS chunk"))?;
        let strs = mercs2_formats::ucfx::extract_chunk_body(&container, b"STRS")
            .ok_or_else(|| format!("string table `{name}`: no STRS chunk"))?;
        let db = mercs2_formats::stringdb::parse(&keys, &strs)
            .map_err(|e| format!("string table `{name}`: {e}"))?;
        Ok(db.entries.into_iter().map(|e| (e.key_hash, e.text)).collect())
    }

    /// A shipped script's LuaQ bytecode by module name (`wifmissiondata`, `mrxrewarddata`, …),
    /// resolved through the residency registry like every other asset (overlays first).
    pub fn script_bytecode(&mut self, name: &str) -> Result<Vec<u8>, String> {
        let name_hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let WadStack { wads, registry, .. } = self;
        let container = registry
            .resolve(wads, mercs2_formats::types::TYPE_HASH_SCRIPT, name_hash)
            .and_then(|c| registry.slice(c).map(<[u8]>::to_vec))
            .ok_or_else(|| format!("script `{name}`: not in any open wad"))?;
        let lay = mercs2_formats::scripts_block::parse_container(&container)
            .map_err(|e| format!("script `{name}`: {e}"))?;
        Ok(container[lay.luaq_off..lay.csum_off].to_vec())
    }

    /// Every `worldentity` template's name by key (`0x80005FD3 -> "C4"`), from the container's
    /// `Name` records — the names `Pg.Spawn` resolves. The container resolves through the residency
    /// registry like every other asset (overlays first).
    pub fn template_names(&mut self) -> Result<HashMap<u32, String>, String> {
        use mercs2_formats::worldentity::{
            WorldEntity, RETAIL_WORLDENTITY_NAME_HASH, WORLDENTITY_TYPE_HASH,
        };
        let WadStack { wads, registry, .. } = self;
        let container = registry
            .resolve(wads, WORLDENTITY_TYPE_HASH, RETAIL_WORLDENTITY_NAME_HASH)
            .and_then(|c| registry.slice(c).map(<[u8]>::to_vec))
            .ok_or("worldentity: not in any open wad")?;
        let we = WorldEntity::parse(&container).map_err(|e| format!("worldentity: {e}"))?;
        let mut out = HashMap::new();
        for (name, keys) in we.names().map_err(|e| format!("worldentity: {e}"))? {
            for &k in keys {
                out.entry(k).or_insert_with(|| name.clone());
            }
        }
        Ok(out)
    }

    /// The `worldentity` container — the world's templates — resolved as the singleton class it is,
    /// through the same residency registry.
    pub fn worldentity(&mut self) -> Result<Vec<u8>, String> {
        let WadStack { wads, registry, .. } = self;
        registry
            .resolve_singleton(wads, mercs2_formats::worldentity::WORLDENTITY_TYPE_HASH)
            .and_then(|(_, c)| registry.slice(c).map(<[u8]>::to_vec))
            .ok_or_else(|| "no worldentity chunk in any open wad".to_string())
    }

    /// Every source in resolution order: the stack last-opened first, then the side WADs.
    pub fn resolution_order(&self) -> Vec<usize> {
        let n = self.wads.len();
        (0..n).rev().chain(n..n + self.side.len()).collect()
    }

    /// The raw containers of a model's LOD-block chain (coarsest first), from the first open wad that
    /// carries it ([`Self::resolution_order`]) — the same resolution as [`Self::model`], whose rungs
    /// they are.
    pub fn model_lods(&mut self, hash: u32) -> Result<Vec<wad::ModelLod>, String> {
        let mut last = format!("0x{hash:08X}: no model chunk in any open wad");
        for src in self.resolution_order() {
            let Some(w) = self.source_mut(src) else { continue };
            match wad::extract_model_lods(w, hash) {
                Ok(l) => return Ok(l),
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The assembled model — LOD-block chain joined through the resident block, from the first
    /// open wad that carries it ([`Self::resolution_order`]).
    pub fn model(&mut self, hash: u32) -> Result<mercs2_engine::model::Model, String> {
        let mut last = format!("0x{hash:08X}: no model chunk in any open wad");
        for src in self.resolution_order() {
            let Some(w) = self.source_mut(src) else { continue };
            match mercs2_engine::model::Model::load(w, hash) {
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
        // The stack last-opened first, then the side WADs (a texture only `shell.wad` ships).
        for src in self.resolution_order() {
            match self.texture_from(src, hash) {
                Ok(t) => {
                    self.tex_cache.insert(hash, t.clone());
                    return Ok(t);
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// The texture `hash` at full streamed resolution from source `src` alone — the copy one WAD
    /// holds, for comparing it with another WAD's.
    pub fn texture_from(&mut self, src: usize, hash: u32) -> Result<mercs2_formats::texture::TextureData, String> {
        let w = self.source_mut(src).ok_or_else(|| format!("no source {src}"))?;
        wad::extract_texture_hires(w, hash).or_else(|_| wad::extract_texture(w, hash))
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
    /// A character kit's outfits (its MIXR and the world templates that wear it) and the outfit
    /// shown, as (template name, outfit number). `None` for a model that is not a kit. A kit shows
    /// no part until an outfit is chosen — every part's switch starts hidden, as in the game.
    kit: Option<(KitOutfits, Option<(String, usize)>)>,
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

/// Full-screen texture plate view (rendered through the engine's loading-plate path).
struct TexView {
    hashes: Vec<u32>,
    labels: Vec<String>,
    idx: usize,
    /// Dims/format of the currently bound plate (for the caption).
    info: String,
    /// A single catalog row's texture: every source holding it (the row's own first), whether each
    /// copy's bytes differ from the first's, and the source shown. Empty for a model's texture list,
    /// which resolves each texture through the stack.
    copies: Vec<(usize, bool)>,
    src: Option<usize>,
}

impl TexView {
    /// A model's textures, each resolved through the stack.
    fn of_list(hashes: Vec<u32>, labels: Vec<String>) -> TexView {
        TexView { hashes, labels, idx: 0, info: String::new(), copies: Vec::new(), src: None }
    }
}

/// Every source of a texture row (`src` first, then `others`), each with whether its copy differs
/// from `src`'s — compared on the decoded texture, so two byte-identical copies in different WADs
/// read the same.
fn texture_copies(w: &mut WadStack, hash: u32, src: usize, others: &[usize]) -> Vec<(usize, bool)> {
    let first = w.texture_from(src, hash).ok();
    let mut out = vec![(src, false)];
    for &o in others {
        let differs = match (&first, w.texture_from(o, hash)) {
            (Some(a), Ok(b)) => a.mip0 != b.mip0 || a.width != b.width || a.height != b.height || a.format != b.format,
            _ => true,
        };
        out.push((o, differs));
    }
    out
}

/// One user intention, queued from EITHER a keyboard shortcut or an inspector-GUI widget and
/// executed once per frame where every `&mut` is free — so the GUI and the keys share one
/// implementation instead of two drifting copies.
/// How loud one log line is. Inferred from the message rather than declared, because every
/// `status = …` site in this file already writes in a settled house style (`"… FAILED: {e}"`,
/// `"could not …"`), and threading a level through forty call sites would be a change with no
/// behavioural payoff.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn of(msg: &str) -> LogLevel {
        let m = msg.to_ascii_lowercase();
        if m.contains("failed") || m.contains("could not") || m.contains("cannot") || m.contains("error")
        {
            LogLevel::Error
        } else if m.contains("blocked") || m.contains("warn") || m.contains("refus") {
            LogLevel::Warn
        } else {
            LogLevel::Info
        }
    }
    fn colour(self) -> egui::Color32 {
        match self {
            LogLevel::Info => crate::gui::theme::DIM,
            LogLevel::Warn => crate::gui::theme::BRASS,
            LogLevel::Error => crate::gui::theme::BAD,
        }
    }
    fn tag(self) -> &'static str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "FAIL",
        }
    }
}

/// One line of session history. The status bar shows only the LATEST message, so without this the
/// previous forty are gone — including every error that scrolled past while something else
/// succeeded. That is what made the rail's "Log" button a button with nothing behind it.
struct LogLine {
    /// Seconds since the window opened. A wall clock would be better in a bug report and worse
    /// while working; this is the one that answers "did that just happen".
    t: f32,
    level: LogLevel,
    text: String,
}

/// Keep the log bounded — a long session with a busy clip loader produces thousands of lines and
/// none of the old ones justify an unbounded allocation.
const LOG_CAP: usize = 2000;

fn push_log(log: &mut std::collections::VecDeque<LogLine>, t: f32, text: &str) {
    if log.len() == LOG_CAP {
        log.pop_front();
    }
    log.push_back(LogLine { t, level: LogLevel::of(text), text: text.to_string() });
}

enum Act {
    /// Load the browser row at `filtered[i]` (models → preview, textures → plate view).
    LoadRow(usize),
    Tier(u8),
    TierNext,
    ClipSel(usize),
    ClipNav(i32),
    /// Play this clip (hash, label) on the loaded model, adding it to the model's clip list.
    PlayClip(u32, String),
    /// Open the hero picker for the clip the Anims tab shows.
    HeroPick,
    /// Make this model (hash, row label) the hero, saved in the settings.
    SetHero(u32, String),
    HeroClose,
    ClipStop,
    PlayPause,
    GroupToggle(usize),
    /// "Start a Shipment from this…": a new Shipment in a picked folder, holding this asset's first
    /// route (`quartermaster::first_route`): the asset's label and what it is.
    StartShipment(String, crate::quartermaster::Subject),
    /// Open the Export dialog for a model (the menu's "Export…", or F10 on the loaded one).
    OpenExport(u32, String),
    /// The Export dialog's "Choose…": pick the folder it writes into.
    ExportFolder,
    /// The Export dialog's "Export": write the model as the dialog says.
    RunExport,
    /// Hide every draw group except this one / show them all again.
    IsolateGroup(usize),
    ShowAllGroups,
    /// Put switch node `.0` into its state index `.1` and re-execute the machine's SHOW/Hide.
    NodeState(usize, usize),
    /// Show a character kit's outfit `.1` of the template named `.0` (`KitOutfits::of_template`).
    Outfit(String, usize),
    /// Set the object's HEALTH fraction (0..1) and drive the destruction machine from it — pristine
    /// at full, damaged/on-fire as it drops, wreck at 0. The systemic replacement for hand-picking
    /// per-node states.
    SetHealth(f32),
    /// Toggle hiding every draw group whose diffuse is a `*_ruin*` material (the eyeball test).
    ToggleRuin,
    TexOfPreview,
    TexNav(i32),
    /// Show the plate's copy from this source (a texture more than one WAD holds).
    TexSource(usize),
    TexClose,
    /// Open a decompiled game script (corpus path) in the read-only Lua viewer.
    LuaOpen(String),
    /// Open a decompiled game script in the Lua viewer scrolled to a specific 1-based line —
    /// used by the Missions blueprint editor's "Jump to source" verb.
    LuaOpenAt(String, u32),
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
    /// Conform: draw the donor template at the origin as a visual reference (replacing the last one).
    LoadDonorRef,
    /// Load a model by hash onto the preview pedestal (Model Workbench inventory click).
    LoadModelHash(u32, String),
    /// Library: show a texture's plate straight from the catalog.
    LoadTexture(u32, String),
    /// Library routing: start a contribution of `kind` with this asset as its donor/target,
    /// scaffolding a Shipment if none is open, and go to the Shipment page.
    AddToShipment(&'static str, u32, String),
    /// Library routing: open a bench with this asset as its donor. Nothing is written until the
    /// bench saves.
    Bench(crate::quartermaster::Craft, u32, String),
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
    /// Write a buildable Shipment from the CURRENT retarget — including hand-adjusted bones, which
    /// exist nowhere but this UI and so cannot be recovered by any headless rebuild.
    ExportShipment,
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
    /// Browse / inspect / edit a Mercenaries 2 `.profile`. A rail-sibling of Inspect: the
    /// `mercs2_formats::save` reader, the `save_lua` AST and the `save_write` writer are the
    /// backbone; the panel in `src/saves.rs` is their UI front end.
    Saves,
    /// Author a mod: conform an import onto a donor template, place hardpoints, publish a patch WAD.
    Mods,
    /// Retarget a Source-rigged (ValveBiped / Mixamo / Unreal) import onto a Mercs2 HIER skeleton.
    Skeleton,
    /// The Shipment accumulator: the contribution queue, what each one touches, the linter, and the
    /// build. Every domain's edits land in the same Shipment, so this is where it is assembled.
    Quartermaster,
    /// Where the game and the reference bundle live — the two paths the tool cannot discover for
    /// itself on every platform. Last on the rail: configuration, not an authoring surface.
    Settings,
    /// The running game (`crate::live`). Pinned to the rail's foot, above Log, with a dot saying
    /// whether the game answers.
    Live,
    /// **A domain** (Plan 02 §C): one of seven curated lenses over the catalog. A single variant
    /// rather than seven, so each dispatch site gains ONE arm, not seven. A domain page IS Inspect
    /// with a lens-filtered navigator — the viewport, inspector cards and verbs all read the same
    /// loaded preview, so browsing here is browsing there, narrowed to what the domain governs.
    Domain(crate::domain::Domain),
}

impl Workbench {
    /// The rail. Mods and Skeleton are NOT peers here.
    ///
    /// Plan 02: *"'Mods' as a page disappears … Sandbox / Skeleton dissolve into craft surfaces,
    /// not top-level peers."* They are craft — they act on ONE contribution — so they are entered
    /// from the Shipment page and return to it, rather than being places you navigate to and then
    /// have to remember what you were assembling.
    const ALL: [Workbench; 7] = [
        Workbench::Inspect,
        Workbench::Saves,
        Workbench::Domain(crate::domain::Domain::Driving),
        Workbench::Domain(crate::domain::Domain::Missions),
        Workbench::Domain(crate::domain::Domain::Systems),
        Workbench::Quartermaster,
        Workbench::Settings,
    ];

    /// The domains Inspect offers as Lenses: those that browse models and are not rail pages.
    fn lenses() -> Vec<crate::domain::Domain> {
        crate::domain::Domain::ALL
            .into_iter()
            .filter(|d| d.browses_models() && !Workbench::ALL.contains(&Workbench::Domain(*d)))
            .collect()
    }

    /// The rail's foot cell that opens the log; it is not a page, so it has no variant.
    const LOG_LABEL: &'static str = "Log";

    /// The rail entry this page lights: a bench works on one Shipment contribution, so it lights
    /// Shipment; a page reached from Inspect lights Inspect.
    fn rail_entry(self) -> Workbench {
        match self {
            Workbench::Mods | Workbench::Skeleton => Workbench::Quartermaster,
            other => other,
        }
    }

    /// A domain page reuses Inspect's centre viewport, inspector cards and verbs — it is Inspect with
    /// a lens-filtered navigator. This is the single predicate that keeps those three surfaces shared.
    fn inspect_like(self) -> bool {
        matches!(self, Workbench::Inspect | Workbench::Domain(_))
    }

    /// A craft surface reached from a contribution, rather than from the rail.
    fn is_craft(self) -> bool {
        matches!(self, Workbench::Mods | Workbench::Skeleton)
    }
    /// Does this page put the 3D viewport in the centre?
    ///
    /// Drives the viewport HUD chips. It used to be spelled `wb != Quartermaster` at the one call
    /// site, so the moment a SECOND page stopped owning the viewport the "ORBIT" chip started
    /// painting over that page's heading — which is exactly what Settings then did.
    fn has_viewport(self) -> bool {
        match self {
            // Saves composes its own centre (hero card + HUD overlays + progression tabs) rather
            // than painting over the 3D viewport, so the HUD chips stay suppressed here.
            Workbench::Quartermaster | Workbench::Settings | Workbench::Saves | Workbench::Live => false,
            // A model-browsing domain shows the loaded model; audio/missions/systems do not.
            Workbench::Domain(d) => d.browses_models(),
            _ => true,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Workbench::Inspect => "Inspect",
            Workbench::Saves => "Saves",
            Workbench::Mods => "Mods",
            Workbench::Skeleton => "Skeleton",
            Workbench::Quartermaster => "Shipment",
            Workbench::Settings => "Settings",
            Workbench::Live => "Live",
            Workbench::Domain(d) => d.label(),
        }
    }
    /// The rail glyph. A METHOD, not a parallel array: the rail used to index a 6-entry `icons`
    /// list with the position in `ALL`, which has 4 — so trimming `ALL` silently gave Shipment the
    /// Mods glyph and Settings the Skeleton one. Keyed off the variant, that cannot happen.
    fn icon(self) -> crate::gui::theme::RailIcon {
        match self {
            Workbench::Inspect => crate::gui::theme::RailIcon::Inspect,
            Workbench::Saves => crate::gui::theme::RailIcon::Saves,
            Workbench::Mods => crate::gui::theme::RailIcon::Mods,
            Workbench::Skeleton => crate::gui::theme::RailIcon::Skeleton,
            Workbench::Quartermaster => crate::gui::theme::RailIcon::Quartermaster,
            Workbench::Settings => crate::gui::theme::RailIcon::Settings,
            Workbench::Live => crate::gui::theme::RailIcon::Live,
            Workbench::Domain(d) => d.rail_icon(),
        }
    }
    /// The command-bar breadcrumb verb.
    fn verb(self) -> &'static str {
        match self {
            Workbench::Inspect => "Inspecting",
            Workbench::Saves => "Saves",
            Workbench::Mods => "Fit to donor",
            Workbench::Skeleton => "Retarget rig",
            Workbench::Quartermaster => "Shipment",
            Workbench::Settings => "Settings",
            Workbench::Live => "Live",
            Workbench::Domain(d) => d.label(),
        }
    }
}

/// The page that implements a bench: Retarget is the Skeleton workbench, Fit the Mods one.
fn bench_page(bench: crate::quartermaster::Craft) -> Workbench {
    match bench {
        crate::quartermaster::Craft::Rig => Workbench::Skeleton,
        crate::quartermaster::Craft::Conform => Workbench::Mods,
    }
}

/// A section of the Settings page.
///
/// Settings used to render its whole form INSIDE the navigator, which left the centre and the
/// inspector deliberately registered and empty — two blank panels on the one page that had nothing
/// to put in them. It now behaves like every other page: the navigator lists, the centre holds the
/// subject, and the inspector is simply not registered here (see the `wb != Settings` guard on it).
#[derive(PartialEq, Clone, Copy, Debug)]
pub(crate) enum SettingsSection {
    Game,
    Data,
    Shipments,
    Shortcuts,
    About,
}

impl SettingsSection {
    pub(crate) const ALL: [SettingsSection; 5] = [
        SettingsSection::Game,
        SettingsSection::Data,
        SettingsSection::Shipments,
        SettingsSection::Shortcuts,
        SettingsSection::About,
    ];
    pub(crate) fn label(self) -> &'static str {
        match self {
            SettingsSection::Game => "Game archive",
            SettingsSection::Data => "Reference data",
            SettingsSection::Shipments => "Shipments library",
            SettingsSection::Shortcuts => "Shortcuts",
            SettingsSection::About => "About & diagnostics",
        }
    }
    pub(crate) fn blurb(self) -> &'static str {
        match self {
            SettingsSection::Game => "Which install the tool reads",
            SettingsSection::Data => "Hash-to-name bundle",
            SettingsSection::Shipments => "Where built Shipments go",
            SettingsSection::Shortcuts => "Keys this window answers",
            SettingsSection::About => "Versions and resolved paths",
        }
    }
}

/// Every key this window answers, for the Settings page to print.
///
/// Kept here beside the `match code` that implements them. It is a hand-maintained mirror, which is
/// a real cost — but the alternative was what existed before: shortcuts discoverable only by
/// pressing keys, since the verb bar names a handful and the rest are buried in an event handler.
const SHORTCUTS: &[(&str, &str)] = &[
    ("Esc", "Quit (or close the texture plate)"),
    ("Enter", "Load the selected asset"),
    ("F3", "View the preview's textures"),
    ("F4", "Next animation clip"),
    ("F10", "Export the preview (OBJ + textures)"),
    ("F11", "Cycle LOD tier"),
    ("Space", "Play / pause the clip"),
    (", / .", "Previous / next clip"),
    ("\\", "Stop the clip"),
    ("[ / ]", "Cycle draw groups"),
    ("Drag / Shift-drag", "Orbit / pan"),
];

/// The Library's "act on this asset" menu — ONE implementation, used by the browser row and by the
/// inspector heading.
///
/// Those two were separate copies, and they had drifted: the heading offered
/// `Export (OBJ + textures)` and nothing else, while the row offered load/place/export. Neither had
/// been touched since before Shipments existed, so the only verb the Library offered for an asset
/// was to write it to disk as OBJ — Plan 02's *"edit in \<domain\> / start a Shipment from this"*
/// had no implementation, and there was **no code path from an Inspect selection into `qm` at all**.
///
/// Export stays: it is asset extraction and independently useful. It is just no longer the only
/// thing you can do with an asset.
///
/// What it offers follows the asset ([`crate::quartermaster::Subject`]): a model's load, export and
/// benches, a texture's plate, or — for every other kind — `preview`, the row's own click. The
/// Shipment routes are the kinds that take that subject; a subject no kind takes offers none.
fn asset_menu(
    ui: &mut egui::Ui,
    hash: u32,
    label: &str,
    subject: crate::quartermaster::Subject,
    preview: Option<Act>,
    actions: &mut Vec<Act>,
) {
    use crate::gui::theme;
    use crate::quartermaster::Subject;
    ui.label(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), theme::BRASS));
    ui.separator();

    if subject == Subject::Texture {
        if ui.add(egui::Button::new("View plate").shortcut_text("Enter")).clicked() {
            actions.push(Act::LoadTexture(hash, label.to_string()));
            ui.close_menu();
        }
    } else if !matches!(subject, Subject::Model { .. }) {
        if let Some(p) = preview {
            if ui.add(egui::Button::new("Preview").shortcut_text("Enter")).clicked() {
                actions.push(p);
                ui.close_menu();
            }
        }
    } else {
        if ui.add(egui::Button::new("Load in viewport").shortcut_text("Enter")).clicked() {
            actions.push(Act::LoadModelHash(hash, label.to_string()));
            ui.close_menu();
        }
        if ui.add(egui::Button::new("Export\u{2026}").shortcut_text("F10")).clicked() {
            actions.push(Act::OpenExport(hash, label.to_string()));
            ui.close_menu();
        }
        ui.separator();
        // A bench borrows this model — its rig, materials and state machine — and writes a NEW
        // contribution; the asset itself is never the thing written.
        ui.label(theme::disp_text("Use as a donor \u{b7} never overwritten".to_uppercase(), theme::style_eyebrow(), theme::FAINT));
        let benches = [
            (crate::quartermaster::Craft::Rig, "Retarget a rig onto this", "opens the Retarget bench \u{b7} writes a wearable outfit in your Shipment"),
            (crate::quartermaster::Craft::Conform, "Fit an import to this", "opens the Fit bench \u{b7} writes a new model in your Shipment"),
        ];
        for (bench, title, what) in benches {
            let mut job = egui::text::LayoutJob::default();
            job.append(title, 0.0, egui::TextFormat::simple(theme::font(&egui::TextStyle::Button), theme::TX));
            job.append(&format!("\n{what}"), 0.0, egui::TextFormat::simple(theme::font(&egui::TextStyle::Small), theme::DIM));
            if ui.add(egui::Button::new(job)).clicked() {
                actions.push(Act::Bench(bench, hash, label.to_string()));
                ui.close_menu();
            }
        }
    }
    ui.separator();

    // The routing that did not exist. A shipped asset becomes a DONOR or a TARGET — never the
    // thing written — so no entry here can produce a destructive contribution.
    let routes = crate::quartermaster::routes_for(subject);
    if !routes.is_empty() {
        ui.menu_button("Add to Shipment", |ui| {
            for (kind, what) in routes {
                if ui.button(*what).clicked() {
                    actions.push(Act::AddToShipment(kind, hash, label.to_string()));
                    ui.close_menu();
                }
            }
        });
        if ui.button("Start a Shipment from this\u{2026}").clicked() {
            actions.push(Act::StartShipment(label.to_string(), subject));
            ui.close_menu();
        }
        ui.separator();
    }
    if ui.button("Copy name").clicked() {
        ui.ctx().copy_text(label.to_string());
        ui.close_menu();
    }
    if ui.button(format!("Copy hash 0x{hash:08X}")).clicked() {
        ui.ctx().copy_text(format!("0x{hash:08X}"));
        ui.close_menu();
    }
    // What you would paste into a manifest — a NAME where one is known, because a hash is one-way
    // and a manifest full of them cannot be reviewed (the M0130 preference, offered up front).
    if ui.button("Copy as manifest reference").clicked() {
        ui.ctx().copy_text(label.to_string());
        ui.close_menu();
    }
}

/// One animation clip, as the Anims tab's inspector shows it.
#[derive(Clone, Debug)]
pub(crate) struct ClipInfo {
    /// The animgroup block that lists it, and the stack WAD it was found in.
    pub block: u16,
    pub wad: String,
    /// The Havok class it is stored as, and whether its frames decode.
    pub codec: &'static str,
    pub decoded: bool,
    pub duration: f32,
    pub frames: usize,
    /// Its binding: one bone name-hash per transform track (the `trnm` list).
    pub bones: Vec<u32>,
    /// A `MANM` material animation (no Havok packfile, no bones): its decoded tracks.
    pub material: Option<mercs2_formats::manm::MaterialAnim>,
}

/// Read clip `hash`'s binding and header from the first stack WAD (last-opened first) whose
/// animgroups list it; a container with no Havok packfile is decoded as a `MANM` material
/// animation. An error names why: no animgroup lists it, or what is there does not decode.
pub(crate) fn clip_info(w: &mut WadStack, hash: u32) -> Result<ClipInfo, String> {
    use mercs2_formats::anim::AnimType;
    for i in (0..w.wads.len()).rev() {
        let wad_name = std::path::Path::new(&w.labels[i])
            .file_name()
            .map_or_else(|| w.labels[i].clone(), |f| f.to_string_lossy().into_owned());
        let ix = game_world::clip_index(&mut w.wads[i]);
        let Some(e) = ix.clips.iter().find(|c| c.name_hash == hash) else { continue };
        let (block, bones) = (e.block, e.bones.clone());
        let data = wad::decompress_block_index(&mut w.wads[i], block)?;
        let ag = mercs2_formats::animgroup::parse_animgroup(&data).map_err(|e| format!("block {block}: {e}"))?;
        let c = ag.clips.iter().find(|c| c.name_hash == hash).ok_or_else(|| format!("block {block} no longer lists 0x{hash:08X}"))?;
        if c.havok_len == 0 {
            let m = c
                .container(&data)
                .and_then(mercs2_formats::manm::parse_material_anim)
                .map_err(|e| format!("{wad_name} block {block}: {e}"))?;
            return Ok(ClipInfo {
                block,
                wad: wad_name,
                codec: "MANM material animation",
                decoded: true,
                duration: 0.0,
                frames: 0,
                bones,
                material: Some(m),
            });
        }
        let clip = c
            .packfile(&data)
            .and_then(|pk| mercs2_formats::anim::parse_anim(pk))
            .map_err(|e| format!("{wad_name} block {block}: {e}"))?;
        let codec = match clip.anim_type {
            AnimType::Interleaved => "interleaved",
            AnimType::Wavelet => "wavelet",
            AnimType::Delta => "delta-compressed",
            AnimType::Spline => "spline-compressed",
        };
        return Ok(ClipInfo { block, wad: wad_name, codec, decoded: clip.decoded, duration: clip.duration, frames: clip.num_frames, bones, material: None });
    }
    Err(format!("no animgroup of the stack lists 0x{hash:08X}"))
}

/// Why clip `hash` gave the loaded model nothing to play: its frames do not decode, it binds no bone
/// of `hier`, or it did not read — from what the Anims tab read of it; the binding's own message for
/// a clip it has not read.
fn unplayable(infos: &HashMap<u32, Result<ClipInfo, String>>, hash: u32, label: &str, hier: &[u32]) -> String {
    match infos.get(&hash) {
        Some(Ok(i)) if i.material.is_some() => format!("{label} is a material animation: it drives material values, not bones"),
        Some(Ok(i)) if !i.decoded => format!("{label} is {}: its frames are not decoded", i.codec),
        Some(Ok(i)) => {
            let n = i.bones.iter().filter(|b| hier.contains(b)).count();
            format!("{label} binds {n} of its {} bones on this model", i.bones.len())
        }
        Some(Err(e)) => format!("{label}: {e}"),
        None => format!("clip {label} is not in any animgroup bound to this rig"),
    }
}

/// The hero picker: the models a hero can be, ranked by how many of `want`'s bones their skeleton
/// holds; `then` is the clip to play once a hero is chosen.
pub(crate) struct HeroPicker {
    /// What the hero is being chosen for ("clip ALL01_…"), and its bone hashes.
    pub what: String,
    pub want: Vec<u32>,
    pub filter: String,
    pub then: Option<(u32, String)>,
}

/// What a catalog row of `kind` is, for routing it into a Shipment ([`asset_menu`]).
fn subject_of(kind: Kind, r: &crate::index::AssetRow) -> crate::quartermaster::Subject {
    use crate::quartermaster::Subject;
    use mercs2_formats::types::{TYPE_ID_EFFECT, TYPE_ID_STRINGDB};
    match kind {
        Kind::Model => Subject::Model { character: r.category() == "Character" },
        Kind::Texture => Subject::Texture,
        Kind::Animation => Subject::Animation,
        Kind::Effect if r.type_id == TYPE_ID_EFFECT => Subject::Effect,
        Kind::Text if r.type_id == TYPE_ID_STRINGDB => Subject::StringTable,
        _ => Subject::Other,
    }
}

/// The subject of the model row `label` of `hash` (a kit-template row shares its kit's hash, so the
/// label picks it out).
fn model_subject(index: &AssetIndex, hash: u32, label: &str) -> crate::quartermaster::Subject {
    let character = index
        .models()
        .iter()
        .find(|r| r.hash == hash && r.label() == label)
        .is_some_and(|r| r.category() == "Character");
    crate::quartermaster::Subject::Model { character }
}

/// The context menu for a world-state LAYER — the World domain's second subject.
///
/// A layer is never a donor or a render target, so it gets its own menu (not [`asset_menu`]): it can
/// only be switched on (`activate_layer`) or have its placements moved (`edit_world`). Both route the
/// layer by NAME through the same `Act::AddToShipment` path assets use — `seeded` seeds the layer
/// field from the label, so the resulting contribution names this exact layer.
fn layer_menu(ui: &mut egui::Ui, hash: u32, label: &str, actions: &mut Vec<Act>) {
    use crate::gui::theme;
    ui.label(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), theme::BRASS));
    ui.separator();
    ui.menu_button("Add to Shipment", |ui| {
        for (kind, what) in crate::quartermaster::routes_for_layer() {
            if ui.button(*what).clicked() {
                actions.push(Act::AddToShipment(kind, hash, label.to_string()));
                ui.close_menu();
            }
        }
    });
    ui.separator();
    if ui.button("Copy name").clicked() {
        ui.ctx().copy_text(label.to_string());
        ui.close_menu();
    }
    if ui.button(format!("Copy hash 0x{hash:08X}")).clicked() {
        ui.ctx().copy_text(format!("0x{hash:08X}"));
        ui.close_menu();
    }
}

/// Resolve HIER node-name hashes to names from `names` — the full map [`crate::index::load_all_names`]
/// returns (the GUI's `AssetIndex::names`, or the headless modes' own load). A hash the map does
/// not hold is absent from the result.
fn resolve_node_names(
    hashes: &[u32],
    names: &std::collections::HashMap<u32, String>,
) -> std::collections::HashMap<u32, String> {
    hashes.iter().filter_map(|h| names.get(h).map(|n| (*h, n.clone()))).collect()
}

/// The donor pool: EVERY named model, grouped by the Library's category — not just vehicles.
///
/// Plan 02: *"Delete the vehicle-only donor navigator — any host, not just vehicles."* A prop, a
/// building or a character is as valid a host to conform an import onto as a truck, and the old
/// vehicle-only filter simply made those hosts unreachable. Grouping by `AssetRow::category()`
/// subsumes the old behaviour — vehicles still break out by class — while adding every other host,
/// in `CATEGORY_ORDER`. `filter` narrows the pool to a domain's lens when one is active (Systems and
/// the domain pages), or takes everything when it is `None`. Unnamed rows are skipped: you cannot
/// pick a host you cannot name.
fn build_donor_inventory(
    index: &crate::index::AssetIndex,
    filter: Option<crate::domain::Domain>,
) -> Vec<(&'static str, Vec<(u32, String)>)> {
    let mut map: std::collections::HashMap<&'static str, Vec<(u32, String)>> = Default::default();
    for r in &*index.models() {
        if r.name.is_none() {
            continue;
        }
        if filter.map(|d| d.model_lens(r)).unwrap_or(true) {
            map.entry(r.category()).or_default().push((r.hash, r.label()));
        }
    }
    let mut out = Vec::new();
    for &c in crate::index::CATEGORY_ORDER {
        if let Some(mut rows) = map.remove(c) {
            rows.sort_by(|a, b| a.1.cmp(&b.1));
            out.push((c, rows));
        }
    }
    out
}

/// Whether `r` shows under Inspect's Lens `lens` (`None` = All).
fn in_lens(lens: Option<crate::domain::Domain>, r: &crate::index::AssetRow) -> bool {
    lens.map_or(true, |d| d.model_lens(r))
}

/// The keys that queue the same Act wherever they are pressed — the ones [`SHORTCUTS`] lists that
/// need nothing but the key. `, / .` step the clip and are kept out of the filter charset for it.
fn shortcut_act(code: KeyCode) -> Option<Act> {
    Some(match code {
        KeyCode::F3 => Act::TexOfPreview,
        KeyCode::F4 | KeyCode::Period => Act::ClipNav(1),
        KeyCode::Comma => Act::ClipNav(-1),
        KeyCode::Backslash => Act::ClipStop,
        KeyCode::Space => Act::PlayPause,
        KeyCode::F11 => Act::TierNext,
        _ => return None,
    })
}

/// Inspect's header crumb after `INSPECTING`: the tab, then the Lens (the Models tab with a Lens
/// on), then the loaded model.
fn inspect_crumb(kind: Kind, lens: Option<crate::domain::Domain>, loaded: Option<&str>) -> Vec<String> {
    let mut crumb = vec![kind.short().to_string()];
    if let (Kind::Model, Some(d)) = (kind, lens) {
        crumb.push(d.label().to_string());
    }
    if let Some(m) = loaded {
        crumb.push(m.to_string());
    }
    crumb
}

/// The group header a model row lists under: its faction under a Lens
/// (`AssetRow::faction_group`), its category with All.
fn list_group(
    r: &crate::index::AssetRow,
    lens: Option<crate::domain::Domain>,
    faction_labels: &HashMap<u32, String>,
) -> String {
    match lens {
        Some(_) => r.faction_group(faction_labels),
        None => r.category().to_uppercase(),
    }
}

/// The browser list: indices into `rows` (one `kind`'s rows) that pass the Lens (Models only) and
/// the name filter, in display order.
fn filter_rows(
    rows: &[crate::index::AssetRow],
    kind: Kind,
    filter: &str,
    lens: Option<crate::domain::Domain>,
    faction_labels: &HashMap<u32, String>,
) -> Vec<usize> {
    let f = filter.to_ascii_lowercase();
    let mut v: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| kind != Kind::Model || in_lens(lens, r))
        .filter(|(_, r)| f.is_empty() || r.label().to_ascii_lowercase().contains(&f))
        .map(|(i, _)| i)
        .collect();
    // Models list groups — by faction under a Lens, alphabetically; by category with All
    // (vehicles-by-class → characters → buildings → …) — then name within a group; textures stay in
    // the plain name order `apply_names` set.
    if kind == Kind::Model {
        match lens {
            Some(_) => v.sort_by(|&a, &b| {
                list_group(&rows[a], lens, faction_labels)
                    .cmp(&list_group(&rows[b], lens, faction_labels))
                    .then_with(|| rows[a].label().cmp(&rows[b].label()))
            }),
            None => v.sort_by(|&a, &b| {
                crate::index::category_order(rows[a].category())
                    .cmp(&crate::index::category_order(rows[b].category()))
                    .then_with(|| rows[a].label().cmp(&rows[b].label()))
            }),
        }
    }
    v
}

/// The overlay WADs opened ON TOP of `base`, in load order — the game's own patch mechanism: the
/// `vz-patch.wad` beside it (when `auto_patch` and it exists), then `extra` (every `--overlay`). A
/// later archive's asset wins over an earlier one's.
pub fn overlays_for(base: &str, auto_patch: bool, extra: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if auto_patch {
        let auto = std::path::Path::new(base).with_file_name("vz-patch.wad");
        if auto.is_file() {
            out.push(auto.to_string_lossy().into_owned());
        }
    }
    out.extend(extra.iter().filter(|o| !out.contains(o)).cloned().collect::<Vec<_>>());
    out
}

/// The mod publisher's default output: next to the base wad, NEVER vz-patch.wad (that's the DLC
/// port's file).
fn mod_out_beside(base: &str) -> String {
    std::path::Path::new(base)
        .parent()
        .map(|d| d.join("vz-mod.wad"))
        .unwrap_or_else(|| std::path::PathBuf::from("vz-mod.wad"))
        .to_string_lossy()
        .into_owned()
}

/// Everything opened from the game archive: built on the boot thread, so the loading screen keeps
/// animating while a 2.5 GB archive's index is read.
struct Archive {
    wadpath: String,
    overlays: Vec<String>,
    w: WadStack,
    /// The catalog, without names (the bundle half of the boot brings those).
    index: AssetIndex,
    anim_sel: Option<AnimSelector>,
    /// The game's own shell plate (shell.wad): the browse backdrop.
    shell_plate: Option<mercs2_formats::texture::TextureData>,
    /// The Loading.wad gold skull: the loading screen's art.
    boot_skull: Option<mercs2_formats::texture::TextureData>,
}

/// Open the game archive at `wadpath` with `overlays` and build its catalog.
fn open_archive(wadpath: &str, overlays: &[String]) -> Result<Archive, String> {
    let mut w = WadStack::open(wadpath, overlays).map_err(|e| format!("cannot open {wadpath}: {e}"))?;
    // The catalog builds here (hashes only); names land later through `apply_names`.
    let mut index = AssetIndex::build(&w.wads, &w.side, HashMap::new());
    // The world's templates: each spawn template that picks a character-kit outfit gets its own
    // catalog row ("Jungle Elite", "VZ Sniper", …). Required — the catalog is wrong without them.
    let kit_templates = load_kit_templates(&mut w)
        .map_err(|e| format!("cannot read the world's templates (worldentity): {e}"))?;
    let kit_rows = index.add_kit_templates(&kit_templates.kits, &kit_templates.all);
    eprintln!(
        "[workshop] catalog: {} models ({kit_rows} kit template rows), {} textures ({} wad(s))",
        index.models().len(),
        index.textures().len(),
        w.wads.len()
    );
    // The resident AnimationLookup — the per-character clip resolver (parsed once). None on a
    // WAD without it; the preview then falls back to generic rig-matched clips.
    let anim_sel = w.anim_selector();
    if anim_sel.is_none() {
        eprintln!("[workshop] AnimationLookup not found — character-specific clips unavailable");
    }
    let shell_plate = match wad::shell_loading_plate(wadpath) {
        Ok(td) => Some(td),
        Err(e) => {
            eprintln!("[workshop] shell plate unavailable ({e}); flat backdrop");
            None
        }
    };
    let boot_skull = std::path::Path::new(wadpath)
        .with_file_name("Loading.wad")
        .to_str()
        .and_then(|p| wad::open(p).ok())
        .and_then(|mut lw| {
            let h = mercs2_formats::hash::pandemic_hash_m2("global_loading_skull");
            wad::extract_texture(&mut lw, h).ok()
        });
    if boot_skull.is_none() {
        eprintln!("[workshop] Loading.wad skull unavailable; boot screen without icon");
    }
    Ok(Archive {
        wadpath: wadpath.to_string(),
        overlays: overlays.to_vec(),
        w,
        index,
        anim_sel,
        shell_plate,
        boot_skull,
    })
}

/// Everything the boot thread loads from the reference bundle.
struct BootData {
    /// The bundle it was loaded from (the downloaded copy when the boot downloaded it).
    home: PathBuf,
    names: HashMap<u32, String>,
    lua: Vec<(String, String, String)>,
    missions: crate::missions::Reference,
    audio_idx: crate::audioidx::AudioIndex,
    secs: f32,
}

use crate::settingspage::Which;

enum Boot {
    Prog(f32, &'static str),
    /// The game archive is open.
    Archive(Box<Archive>),
    /// The boot is over, with the reference bundle when it loaded one.
    Done(Option<Box<BootData>>),
    /// The archive or the bundle could not be loaded; Settings shows it on that chain.
    Failed(Which, String),
}

/// Start a boot: open `archive` (base and overlays) when given, then load the reference bundle
/// when `data`. Messages arrive in that order; `Done` (or `Failed`) is the last.
fn spawn_boot(archive: Option<(String, Vec<String>)>, data: bool) -> std::sync::mpsc::Receiver<Boot> {
    let (tx, rx) = std::sync::mpsc::channel::<Boot>();
    std::thread::spawn(move || {
        let t0 = std::time::Instant::now();
        if let Some((wadpath, overlays)) = archive {
            let _ = tx.send(Boot::Prog(0.01, "asset catalog"));
            match open_archive(&wadpath, &overlays) {
                Ok(a) => {
                    let _ = tx.send(Boot::Archive(Box::new(a)));
                }
                Err(e) => {
                    let _ = tx.send(Boot::Failed(Which::Game, e));
                    return;
                }
            }
        }
        if !data {
            let _ = tx.send(Boot::Done(None));
            return;
        }
        let load = || -> Result<BootData, String> {
            let home = crate::index::require_data_home(&mut |f, stage| {
                let _ = tx.send(Boot::Prog(f, stage));
            })?;
            let names = crate::index::load_all_names_staged(|f, stage| {
                let _ = tx.send(Boot::Prog(f, stage));
            })?;
            let _ = tx.send(Boot::Prog(0.97, "Lua reference corpus"));
            let lua = crate::index::load_lua_corpus()?;
            // The Missions page's corpus index and palette: every script parsed and lifted
            // once, here rather than on the UI thread.
            let _ = tx.send(Boot::Prog(0.98, "Mission scripts"));
            let missions = crate::missions::Reference::build(&lua)?;
            Ok(BootData {
                // The captioned audio inventory — the Audio domain browses this (banks → clips
                // with captions).
                audio_idx: crate::audioidx::AudioIndex::load(&home)?,
                home,
                names,
                lua,
                missions,
                secs: t0.elapsed().as_secs_f32(),
            })
        };
        let _ = tx.send(match load() {
            Ok(data) => Boot::Done(Some(Box::new(data))),
            Err(e) => Boot::Failed(Which::Data, e),
        });
    });
    rx
}

/// Where the window is: on Settings because a chain has no answer, loading, or ready.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Setup,
    Booting,
    Ready,
}

/// What the chains ask of the window, given what it has loaded.
#[derive(Debug, PartialEq)]
enum Next {
    /// Nothing changed.
    Stay,
    /// A chain has no answer: Settings, on that section.
    Setup(SettingsSection),
    /// Load: the archive at this path (base; overlays are derived), and/or the reference bundle.
    Boot { archive: Option<PathBuf>, data: bool },
}

/// Decide what the window does with `game` and `data` when it has `loaded_wad` and `loaded_data`
/// in. A reference chain with nothing set and nothing found boots too: the boot downloads it.
fn next_step(loaded_wad: Option<&std::path::Path>, loaded_data: Option<&std::path::Path>, game: &crate::chains::Chain, data: &crate::chains::Chain) -> Next {
    let Some(g) = game.in_use() else { return Next::Setup(SettingsSection::Game) };
    let d = data.in_use();
    if d.is_none() && !data.is_empty() {
        return Next::Setup(SettingsSection::Data);
    }
    let archive = (loaded_wad != Some(g)).then(|| g.to_path_buf());
    // A new archive reloads the bundle with it: every page that reads the corpus is reset with the
    // archive, and is filled again from one boot. With nothing set and nothing found, whatever is
    // loaded came from a source that is gone.
    let data = archive.is_some()
        || match d {
            Some(d) => loaded_data != Some(d),
            None => true,
        };
    if archive.is_none() && !data {
        Next::Stay
    } else {
        Next::Boot { archive, data }
    }
}

pub fn run(opts: Options) {
    // The window comes FIRST. Nothing before it may set the platform's app object up (a native
    // picker here set the macOS app up ahead of winit's event loop, and winit then panicked), and a
    // chain with no answer opens on Settings instead of failing before there is a window.
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

    // The two chains (`crate::chains`): where the game archive and the reference bundle come from.
    // Re-resolved whenever Settings changes a source.
    let mut game_chain = crate::chains::game(opts.wad_flag.as_deref());
    let mut data_chain = crate::chains::data();
    // What is loaded: the archive (base path, overlays) and the bundle's folder. Empty until a boot
    // brings them in; a changed chain is compared against these to decide what to reload.
    let mut wadpath = String::new();
    let mut overlays: Vec<String> = Vec::new();
    let mut loaded_data: Option<PathBuf> = None;
    // The archive-bound state, empty until the archive is open (`Boot::Archive` fills it).
    let mut clip_loader = ClipLoader::spawn(String::new(), Vec::new());
    let mut w = WadStack::empty();
    let mut index = AssetIndex::build(&[], &[], HashMap::new());
    let mut anim_sel: Option<AnimSelector> = None;
    // The game's own shell plate (shell.wad) as the workshop backdrop — the browse screen renders
    // through the engine's menu path over it until something is loaded into the 3D viewport.
    let mut shell_plate: Option<mercs2_formats::texture::TextureData> = None;
    // The base path and overlays of an archive chain answer.
    let stack_of = |p: &std::path::Path, auto_patch: bool, extra: &[String]| -> (String, Vec<String>) {
        let s = p.to_string_lossy().into_owned();
        let o = overlays_for(&s, auto_patch, extra);
        (s, o)
    };
    // Where the window starts: loading both, or Settings on the chain with no answer.
    let first = next_step(None, None, &game_chain, &data_chain);
    let (mut phase, mut boot_rx) = match &first {
        Next::Boot { archive, data } => (
            Phase::Booting,
            Some(spawn_boot(archive.as_deref().map(|p| stack_of(p, opts.auto_patch, &opts.extra_overlays)), *data)),
        ),
        Next::Setup(_) => (Phase::Setup, None),
        Next::Stay => unreachable!("nothing is loaded at startup"),
    };
    // Set when Settings changed a source: the chains are re-resolved at the top of the next frame.
    let mut chains_dirty = false;
    // Decompiled-Lua reference corpus (path, content, lowercased) — loaded on the boot thread.
    let mut lua_corpus: Vec<(String, String, String)> = Vec::new();
    let mut boot_target = 0.02f32; // stage-start fraction from the loader thread
    let mut boot_shown = 0.0f32; // eased bar position
    let mut boot_stage: &'static str = "asset catalog";
    // The last boot's failure: which chain it belongs to and why. Settings shows that chain red
    // with the reason until the next boot starts.
    let mut boot_fail: Option<(Which, String)> = None;
    // Boots finished, and `--ui-state`'s capture waiting on a reload: (PNG, settle frames, the
    // boot count when it was asked).
    let mut boots_done: u32 = 0;
    let mut reload_capture: Option<(PathBuf, u32, u32)> = None;

    // Browser state.
    let mut kind = Kind::Model;
    // Inspect's Lens: the Models list narrowed to one domain (`None` = all models).
    let mut lens: Option<crate::domain::Domain> = None;
    let mut filter = String::new();
    let mut filtered: Vec<usize> = (0..index.rows(kind).len()).collect();
    let mut sel: usize = 0;
    // The asset list's last drawn selection, scroll offset and viewport height: a selection that
    // differs next frame is scrolled into view.
    let mut list_follow: (Option<usize>, (f32, f32)) = (None, (0.0, 0.0));
    // Group headers the user has collapsed in the model list — click a header to fold its rows
    // away so only the header line remains. Persists across frames; keyed by the header text (a
    // category, or a faction under a Lens) so it survives refilter/re-sort. Empty = everything
    // expanded (the default).
    let mut collapsed_cats: HashSet<String> = HashSet::new();

    // Viewport state.
    let mut preview: Option<Preview> = None;
    // The Fit bench's donor reference: the donor model drawn at the origin beside the import, as
    // `(model hash, its entity)`.
    let mut donor_ref: Option<(u32, Entity)> = None;
    // CPU-side store for models that do NOT live in a WAD: drag-drop imports.
    // Keyed by their synthetic hash; export sources geometry from here first.
    let mut imported: HashMap<u32, ModelData> = HashMap::new();
    // Skeleton highlighting: the bone row the pointer is over this frame (transient) + the
    // clicked/pinned one. Rendered as engine glow cards at the bone's POSED position.
    let mut sel_bone: Option<usize> = None;
    let mut tex_view: Option<TexView> = None;
    let mut lua_view: Option<crate::luaview::LuaView> = None;
    // Mod project (publish workbench M3): novel new-hash assets queued for packaging.
    let mut mod_items: Vec<crate::publish::NewModelItem> = Vec::new();
    let mut mod_name = String::new();
    let mut mod_donor: Option<(u32, String)> = None;
    let mut mod_group: usize = 0;
    // Output default: next to the base wad, NEVER vz-patch.wad (that's the DLC port's file). Set
    // when the archive opens (`mod_out_beside`).
    let mut mod_out = String::new();
    let mut publisher: Option<crate::publish::Publisher> = None;
    // In-flight background F10 export of a WAD asset (the heavy full-clip-decode path).
    let mut exporter: Option<Exporter> = None;
    // The open Export dialog, if any: what it exports and the name, format and folder chosen.
    let mut export_dialog: Option<ExportDialog> = None;
    // ── Conform transform (dedicated panel): interactively scale/rotate/place the imported mesh
    // against the donor template, baked into the export. `conform_live` drives the preview entity
    // from these fields so the placement is visible against the donor reference. ──
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
    // The Quartermaster dock. Present beside every workbench, because edits from anywhere land in
    // the same Shipment — so the surface holding them cannot be a page you navigate away to.
    let mut qm = crate::quartermaster::Panel::default();
    // The Saves page's own state (a `.profile` currently selected, if any). Lives on the stack
    // beside `qm` so switching rails never discards it.
    let mut saves = crate::saves::Panel::default();
    // Per-hero `shot::HeroRenderer`s, keyed by model hash. Each one owns its own wgpu device
    // and keeps the pipeline + mesh / texture uploads warm, so an orbit drag re-renders in
    // ~10-30 ms instead of the ~1-2 s a fresh `shot::render` call would take. Dropped only
    // when the whole workshop exits.
    let mut hero_renderers: HashMap<u32, crate::shot::HeroRenderer> = HashMap::new();
    // The Missions blueprint editor's panel. One per app session — pan/zoom/selection survive the
    // navigator tab switching away and back, which the real workflow does constantly.
    let mut missions_panel = crate::missions::Panel::default();
    // Host-provided, like the game stack: the crate never reaches into the filesystem for it.
    let qm_names = mercs2_quartermaster::names::NameTable::find_from(
        &std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
    )
    .or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().and_then(mercs2_quartermaster::names::NameTable::find_from))
    });
    // The open stack (base, then overlays), for the Shipment page and About. Set when the archive
    // opens.
    let mut qm_stack: Vec<String> = Vec::new();
    // Re-read when a boot brings a reference bundle in.
    let mut qm_corpus = crate::quartermaster::corpus_root();
    if let Some(dir) = opts.shipment.clone() {
        qm.open_shipment(&dir, qm_names.as_ref());
        eprintln!("[workshop] quartermaster: {}", qm.status_line(true));
    }
    // The active workbench (activity rail) + the vehicle inventory the Mods navigator lists as donor
    // templates (rebuilt when names/overlays change).
    // Asking for a Shipment IS asking for its page — landing on Inspect would hide what you opened.
    // A chain with no answer opens on Settings; the first boot to finish lands on `resume_wb`.
    let resume_wb = if opts.shipment.is_some() {
        Workbench::Quartermaster
    } else {
        Workbench::Inspect
    };
    let mut wb = if phase == Phase::Setup { Workbench::Settings } else { resume_wb };
    // Whether a boot has ever finished: a reload's boot returns to Settings, where it was asked for.
    let mut ever_ready = false;
    // Settings workbench: the saved document plus the outcome of the last action. `cfg_note` is the
    // ONLY feedback the page gives, so it carries the validation result (model count / name count),
    // never a bare "saved" — a settings page that cannot say what it just accepted is how a bad path
    // gets remembered and fails at the next launch instead of at the click.
    let mut cfg = crate::settings::load();
    let mut cfg_note: Option<Result<String, String>> = None;
    // The hero: the model the Anims and FaceFX tabs play on (saved in the settings), every model's
    // skeleton for ranking candidates (read on a worker the first time it is needed), the picker,
    // the clip the Anims tab shows, and each clip read so far.
    let mut hero: Option<crate::settings::Hero> = cfg.hero.clone();
    let mut skeletons: Option<crate::heroidx::Skeletons> = None;
    let mut skel_rx: Option<std::sync::mpsc::Receiver<crate::heroidx::Msg>> = None;
    let mut skel_progress: (f32, String) = (0.0, String::new());
    let mut skel_error: Option<String> = None;
    let mut hero_picker: Option<HeroPicker> = None;
    let mut anim_clip: Option<(u32, String)> = None;
    let mut clip_infos: HashMap<u32, Result<ClipInfo, String>> = HashMap::new();
    // The material animation card's playhead, in key-time units.
    let mut manm_t: f32 = 0.0;
    // Which Settings section the centre is showing. The navigator lists them. A chain with no
    // answer opens on its section.
    let mut settings_section = match first {
        Next::Setup(s) => s,
        _ => SettingsSection::Game,
    };
    // Which contribution (if any) the open bench was entered from. The index is what makes a bench
    // edit land back on the RIGHT contribution instead of appending a new one — `Act::Craft` used to
    // carry no index at all, so a bench had no idea what it was editing and no way back.
    let mut craft_subject: Option<usize> = None;
    // Whether the running game answers on its lua-bridge port: the Live rail cell's dot.
    let live_probe = crate::live::Probe::spawn(mercs2_bridge::DEFAULT_ADDR.to_string());
    // `--ui-state`: applied once the reference data is in; then frames to settle before the capture,
    // and whether the capture has been asked of the scene.
    let mut ui_state = opts.ui_state.clone();
    // Its AUDIO-tab keys, held until the tab's routing is in.
    let mut audio_ui: Option<crate::ui_state::UiState> = None;
    // A `--ui-state` that opened a string table: its capture (path, settle frames), taken once the
    // tables have read.
    let mut text_ui: Option<(PathBuf, u32)> = None;
    // A `--ui-state` that opened a clip: (capture, settle frames, clip, on a hero, frames waited).
    let mut anim_ui: Option<(PathBuf, u32, u32, bool, u32)> = None;
    let mut capture_to: Option<(PathBuf, u32)> = None;
    let mut capture_asked = false;
    // The Audio page (Inspect's AUDIO tab): the captioned inventory, set from the boot thread's
    // `BootData`, and its routing, read from English.wad the first time the tab is shown. Its acts
    // are queued here by keys as well as widgets.
    let mut audio_page = crate::audiopage::Panel::default();
    let mut audio_acts: Vec<crate::audiopage::Act> = Vec::new();
    // The Text tab's string tables: read on a worker the first time a table is opened.
    let mut text_page = crate::textpage::Panel::default();
    let mut text_acts: Vec<crate::textpage::Act> = Vec::new();
    // Ctrl (or Cmd) held: the Audio tab's select-all.
    let mut ctrl = false;
    // VO playback, both opened LAZILY on the first ▶ (English.wad + the output device are heavy, and
    // most sessions never touch audio). `vo_stream` is the record index; `vo_player` the device.
    let mut vo_stream: Option<crate::vostream::VoStream> = None;
    let mut vo_player: Option<crate::audioplay::Player> = None;
    // Remembered so a failed open (missing English.wad / .pws / device) is reported once, not per click.
    let mut vo_error: Option<String> = None;
    // In-flight reference-data download from the Settings button (which replaces the downloaded
    // copy with the pinned release's): the receiver while it runs, plus the latest progress line.
    // The automatic download when no bundle is found runs on the boot thread.
    let mut fetch_rx: Option<std::sync::mpsc::Receiver<crate::fetch::Event>> = None;
    let mut fetch_msg = String::new();
    // Persist the user's dragged panel widths ourselves — feeding the remembered width back as
    // `default_width` each frame keeps a drag sticky even when collapsing a card would otherwise let
    // egui re-fit the panel to its (now shorter) content.
    let mut navigator_width = 300.0f32;
    let mut inspector_width = 372.0f32;
    let mut vehicle_inventory: Vec<(&'static str, Vec<(u32, String)>)> = build_donor_inventory(&index, None);
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
    // ── Session log. `status` holds only the LATEST line, so every message before it was lost the
    // moment the next one landed — which is why the rail's "Log" button had nothing to show.
    //
    // History is captured by DIFFING `status` once per frame rather than by routing its forty
    // assignment sites through a helper: the sites are spread across the whole action processor and
    // several are inside closures that already borrow half the world, so the diff catches every
    // transition (including ones a future edit adds) without touching any of them.
    let mut log: std::collections::VecDeque<LogLine> = std::collections::VecDeque::new();
    let mut last_status = String::new();
    let mut log_open = false;
    // Whether the log window is showing Info lines. Warnings and errors are never filterable — a
    // log that can hide its failures is worse than no log.
    let mut log_show_info = true;

    // Orbit camera.
    let mut cam_target = Vec3::ZERO;
    let mut cam_yaw: f32 = 0.6;
    let mut cam_pitch: f32 = -0.35;
    let mut cam_dist: f32 = 8.0;
    let mut lmb = false;
    let mut shift = false;
    let mut last_cursor: Option<(f64, f64)> = None;

    let start = std::time::Instant::now();
    let mut last_frame = std::time::Instant::now();

    let refilter = |index: &AssetIndex, kind: Kind, filter: &str, lens: Option<crate::domain::Domain>| -> Vec<usize> {
        filter_rows(index.rows(kind), kind, filter, lens, &index.faction_labels)
    };

    event_loop
        .run(move |event, elwt| match event {
            Event::AboutToWait => scene.window.request_redraw(),
            Event::WindowEvent { window_id, event } if window_id == scene.window.id() => {
            // Feed the GUI first: when a panel captures the pointer/keyboard, the camera and the
            // app shortcuts stand down (see the `_ if gui_took_it` arm below).
            let gui_took_it = phase != Phase::Booting && gui.on_event(&event);
            match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(size) => scene.resize(size),
                // Drag-drop IMPORT: .obj / .gltf / .glb dropped on the window goes straight onto
                // the preview pedestal, rendered by the engine like any game asset.
                WindowEvent::DroppedFile(path) => {
                    if phase != Phase::Ready {
                        return;
                    }
                    status = import_file(
                        &path, &mut w, &mut scene, &mut world, &mut imported, &mut preview,
                        &mut cam_target, &mut cam_dist, &mut retarget, &retarget_target,
                        &mut retarget_src_path, &mut wb,
                        &mut sel_bone, donor_ref.map(|(h, _)| h), &index, &anim_sel, &lua_corpus,
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
                WindowEvent::ModifiersChanged(m) => {
                    shift = m.state().shift_key();
                    ctrl = m.state().control_key() || m.state().super_key();
                }
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
                        return;
                    }

                    // Loading screen, and Settings while a chain has no answer: Esc quits; the
                    // browse keys have nothing to browse. (Settings' own widgets take their keys
                    // through egui, above.)
                    if phase != Phase::Ready {
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

                    // ── The Audio tab: its own keys; nothing types into the asset filter, which the
                    // tab does not show. Esc still quits and ←/→ still step the tabs. ──
                    if wb == Workbench::Inspect && kind == Kind::Audio {
                        if let Some(a) = crate::audiopage::shortcut(code, ctrl) {
                            audio_acts.push(a);
                            return;
                        }
                        if !matches!(code, KeyCode::Escape | KeyCode::ArrowLeft | KeyCode::ArrowRight) {
                            return;
                        }
                    }
                    match code {
                        KeyCode::Escape => elwt.exit(),
                        // ── Browser: navigation + filter typing. ──
                        KeyCode::ArrowUp => sel = sel.saturating_sub(1),
                        KeyCode::ArrowDown => {
                            sel = (sel + 1).min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::PageUp => sel = sel.saturating_sub(24),
                        KeyCode::PageDown => {
                            sel = (sel + 24).min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::Home => sel = 0,
                        KeyCode::End => sel = filtered.len().saturating_sub(1),
                        KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                            // Cycles the whole category strip now that there are eight of them,
                            // not two. Left steps back, right steps forward, both wrapping.
                            let cur = Kind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
                            let n = Kind::ALL.len();
                            let step = if code == KeyCode::ArrowLeft { n - 1 } else { 1 };
                            kind = Kind::ALL[(cur + step) % n];
                            filtered = refilter(&index, kind, &filter, lens);
                            sel = 0;
                        }
                        KeyCode::Backspace => {
                            filter.pop();
                            filtered = refilter(&index, kind, &filter, lens);
                            sel = sel.min(filtered.len().saturating_sub(1));
                        }
                        KeyCode::Enter => actions.push(Act::LoadRow(sel)),
                        // ── Preview actions (also reachable from the inspector GUI). ──
                        _ if shortcut_act(code).is_some() => actions.extend(shortcut_act(code)),
                        KeyCode::F10 => {
                            if let Some(p) = &preview {
                                actions.push(Act::OpenExport(p.hash, p.label.clone()));
                            }
                        }
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
                        _ => {
                            // Remaining printable keys type into the filter.
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
                                    filtered = refilter(&index, kind, &filter, lens);
                                    sel = 0;
                                }
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    let t = start.elapsed().as_secs_f32();
                    let dt = last_frame.elapsed().as_secs_f32().min(0.05);
                    last_frame = std::time::Instant::now();

                    // ── Reference-data download (Settings): drain whatever the worker has said. A
                    // finished download re-resolves the chain, which loads it in place when it is
                    // now the source in use. ──
                    if let Some(rx) = &fetch_rx {
                        let mut finished = false;
                        loop {
                            match rx.try_recv() {
                                Ok(crate::fetch::Event::Progress(m)) => fetch_msg = m,
                                Ok(crate::fetch::Event::Done(p)) => {
                                    cfg_note = Some(Ok(format!("Reference data installed to {}.", p.display())));
                                    // Re-downloaded into the folder in use: same path, new files, so
                                    // the chain alone would see nothing to reload.
                                    if loaded_data.as_deref() == Some(p.as_path()) {
                                        loaded_data = None;
                                    }
                                    chains_dirty = true;
                                    finished = true;
                                }
                                Ok(crate::fetch::Event::Failed(e)) => {
                                    cfg_note = Some(Err(format!("Download failed: {e}")));
                                    finished = true;
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                // Worker vanished without a verdict — do not leave the UI spinning.
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    finished = true;
                                    break;
                                }
                            }
                        }
                        if finished {
                            fetch_rx = None;
                            fetch_msg.clear();
                        }
                    }

                    // ── A source changed in Settings: re-resolve both chains and do what they ask —
                    // nothing, unload into Settings, or reload in place. Anything tied to what is
                    // being replaced is dropped first, so no page keeps state from the old archive
                    // or bundle. ──
                    if chains_dirty && phase != Phase::Booting {
                        chains_dirty = false;
                        game_chain = crate::chains::game(opts.wad_flag.as_deref());
                        data_chain = crate::chains::data();
                        let loaded_wad = (!wadpath.is_empty()).then(|| PathBuf::from(&wadpath));
                        let next = next_step(loaded_wad.as_deref(), loaded_data.as_deref(), &game_chain, &data_chain);
                        if reload_capture.is_some() && !matches!(next, Next::Boot { .. }) {
                            eprintln!("--ui-state: run_wad / run_data started no reload ({next:?}): it names what is already loaded, or leaves a chain with no answer");
                            std::process::exit(2);
                        }
                        let (drop_archive, drop_data) = match &next {
                            Next::Stay => (false, false),
                            Next::Setup(SettingsSection::Game) => (true, true),
                            Next::Setup(_) => (false, true),
                            Next::Boot { archive, .. } => (archive.is_some(), true),
                        };
                        if drop_archive {
                            // The scene keeps uploads by model hash, and the donor path skips a hash
                            // it already has: a hash both archives share would draw the old mesh.
                            if let Some(p) = preview.take() {
                                scene.forget_entity(p.entity);
                                scene.unload_model(p.hash);
                            }
                            if let Some((h, e)) = donor_ref.take() {
                                scene.forget_entity(e);
                                scene.unload_model(h);
                            }
                            world = World::new();
                            hero_renderers.clear();
                            tex_view = None;
                            export_dialog = None;
                            craft_subject = None;
                            retarget_target = None;
                            mod_donor = None;
                            vo_stream = None;
                            vo_player = None;
                            vo_error = None;
                            w = WadStack::empty();
                            index = AssetIndex::build(&[], &[], HashMap::new());
                            anim_sel = None;
                            // Skeletons and clips are the archive's; the hero is checked against the
                            // new catalog when it lands.
                            skeletons = None;
                            skel_rx = None;
                            skel_error = None;
                            hero_picker = None;
                            anim_clip = None;
                            clip_infos.clear();
                            clip_loader = ClipLoader::spawn(String::new(), Vec::new());
                            shell_plate = None;
                            wadpath.clear();
                            overlays.clear();
                            qm_stack.clear();
                            mod_out.clear();
                            filtered = refilter(&index, kind, &filter, lens);
                            sel = 0;
                            inventory_dirty = true;
                        }
                        if drop_data {
                            lua_corpus.clear();
                            lua_view = None;
                            missions_panel = crate::missions::Panel::default();
                            // Its routing reads English.wad beside the archive, its clips the bundle.
                            audio_page = crate::audiopage::Panel::default();
                            // Its tables are the archive's, its key names the bundle's.
                            text_page = crate::textpage::Panel::default();
                            loaded_data = None;
                        }
                        match next {
                            Next::Stay => {}
                            Next::Setup(s) => {
                                phase = Phase::Setup;
                                wb = Workbench::Settings;
                                settings_section = s;
                            }
                            Next::Boot { archive, data } => {
                                boot_fail = None;
                                boot_target = 0.02;
                                boot_shown = 0.0;
                                boot_stage = if archive.is_some() { "asset catalog" } else { "reference data" };
                                boot_rx = Some(spawn_boot(
                                    archive.as_deref().map(|p| stack_of(p, opts.auto_patch, &opts.extra_overlays)),
                                    data,
                                ));
                                phase = Phase::Booting;
                            }
                        }
                    }

                    // ── Loading screen: engine loading path (shell plate + spinner + bar) while a
                    // boot runs, then into the app. A boot that fails lands on Settings with the
                    // failing chain in red and the error under it. ──
                    if phase == Phase::Booting {
                        let mut ended: Option<Result<(), (Which, String)>> = None;
                        if let Some(rx) = &boot_rx {
                            loop {
                                match rx.try_recv() {
                                    Ok(Boot::Prog(f, stage)) => {
                                        boot_target = f;
                                        boot_stage = stage;
                                    }
                                    Ok(Boot::Archive(a)) => {
                                        let Archive { wadpath: p, overlays: o, w: nw, index: ni, anim_sel: asel, shell_plate: sp, boot_skull } = *a;
                                        clip_loader = ClipLoader::spawn(p.clone(), o.clone());
                                        mod_out = mod_out_beside(&p);
                                        qm_stack = std::iter::once(p.clone()).chain(o.iter().cloned()).collect();
                                        wadpath = p;
                                        overlays = o;
                                        w = nw;
                                        index = ni;
                                        anim_sel = asel;
                                        shell_plate = sp;
                                        // BOOT art: the authentic Loading.wad gold skull — the
                                        // boot-mode loading shader draws it pulsing on black with
                                        // the warm sheen. The shell plate takes over as the browse
                                        // backdrop once the boot completes.
                                        if let Some(td) = boot_skull.as_ref().or(shell_plate.as_ref()) {
                                            scene.set_loading_art(td);
                                        }
                                        filtered = refilter(&index, kind, &filter, lens);
                                        sel = 0;
                                        inventory_dirty = true;
                                    }
                                    Ok(Boot::Done(data)) => {
                                        if let Some(data) = data {
                                            let BootData { home, names, lua, missions, audio_idx: audio, secs } = *data;
                                            index.apply_names(names);
                                            lua_corpus = lua;
                                            // The Missions blueprint editor's script list and palette
                                            // come from the corpus the boot thread indexed.
                                            missions_panel.set_reference(missions, &lua_corpus);
                                            audio_page.set_index(audio);
                                            loaded_data = Some(home);
                                            qm_corpus = crate::quartermaster::corpus_root();
                                            status = format!(
                                                "{} names loaded ({secs:.1}s) — {} models, {} textures, {} lua scripts",
                                                index.names.len(),
                                                index.models().len(),
                                                index.textures().len(),
                                                lua_corpus.len()
                                            );
                                        }
                                        filtered = refilter(&index, kind, &filter, lens);
                                        inventory_dirty = true; // names now resolve → build the vehicle inventory
                                        sel = sel.min(filtered.len().saturating_sub(1));
                                        // A saved hero this archive does not hold is cleared, and said.
                                        if let Some(h) = &hero {
                                            if !index.models().iter().any(|r| r.hash == h.hash && r.label() == h.label) {
                                                status = format!("the hero {} is not in this archive — pick another on the Anims tab", h.label);
                                                hero = None;
                                                cfg.hero = None;
                                                if let Err(e) = cfg.save() {
                                                    status = format!("{status} (settings not saved: {e})");
                                                }
                                            }
                                        }
                                        // Swap the boot skull out for the shell plate (browse backdrop).
                                        if let Some(td) = &shell_plate {
                                            scene.set_loading_art(td);
                                        }
                                        ended = Some(Ok(()));
                                        // The loader's last word: it exits after sending this, so the
                                        // next receive would see it gone and call that a failure.
                                        break;
                                    }
                                    Ok(Boot::Failed(which, e)) => {
                                        ended = Some(Err((which, e)));
                                        break;
                                    }
                                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                        let which = if wadpath.is_empty() { Which::Game } else { Which::Data };
                                        ended = Some(Err((which, "the loader stopped without a result".to_string())));
                                        break;
                                    }
                                }
                            }
                        }
                        match ended {
                            None => {}
                            Some(Ok(())) => {
                                boot_rx = None;
                                // The chains as they stand now: a boot that downloaded the bundle
                                // made the downloaded copy the data chain's answer.
                                game_chain = crate::chains::game(opts.wad_flag.as_deref());
                                data_chain = crate::chains::data();
                                phase = Phase::Ready;
                                if !ever_ready {
                                    wb = resume_wb;
                                }
                                ever_ready = true;
                                boots_done += 1;
                            }
                            Some(Err((which, e))) => {
                                eprintln!("workshop: {e}");
                                if ui_state.is_some() || reload_capture.is_some() {
                                    eprintln!("--ui-state: the window never left the loading screen: {e}");
                                    std::process::exit(2);
                                }
                                boot_rx = None;
                                // What the failed boot half-loaded goes with it.
                                if which == Which::Game {
                                    w = WadStack::empty();
                                    index = AssetIndex::build(&[], &[], HashMap::new());
                                    wadpath.clear();
                                    overlays.clear();
                                    qm_stack.clear();
                                    filtered = refilter(&index, kind, &filter, lens);
                                    sel = 0;
                                }
                                let chain = if which == Which::Game { &mut game_chain } else { &mut data_chain };
                                chain.fail_in_use(if which == Which::Game { "WON'T OPEN" } else { "UNREADABLE" }, e.clone());
                                boot_fail = Some((which, e.clone()));
                                cfg_note = Some(Err(e));
                                phase = Phase::Setup;
                                wb = Workbench::Settings;
                                settings_section = if which == Which::Game { SettingsSection::Game } else { SettingsSection::Data };
                            }
                        }
                        if phase == Phase::Booting {
                            boot_shown += (boot_target - boot_shown) * (1.0 - (-3.0 * dt).exp());
                            let (wpx, hpx) = (scene.size.width as f32, scene.size.height as f32);
                            // Retail layout: green tip line centred at ~2/3 height; bold orange
                            // "Loading" at the lower right (gently breathing like the game's).
                            const TIP_GREEN: [f32; 4] = [0.76, 0.80, 0.44, 1.0];
                            const LOAD_ORANGE: [f32; 4] = [1.0, 0.62, 0.12, 1.0];
                            let stage = format!("loading {boot_stage}");
                            let sw = stage.len() as f32 * 8.0 * UI_S;
                            text_sh(&mut scene, (wpx - sw) * 0.5, hpx * 0.66, UI_S, TIP_GREEN, &stage);
                            let ls = UI_S * 1.5; // 24px
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
                                    status = unplayable(&clip_infos, done.hash, &label, &p.hier);
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
                                        index = AssetIndex::build(&w.wads, &w.side, names);
                                        // The new overlay may carry its own worldentity.
                                        let kit_templates = match load_kit_templates(&mut w) {
                                            Ok(t) => t,
                                            Err(e) => {
                                                status.push_str(&format!(" (kit templates unreadable, no template rows: {e})"));
                                                KitTemplates::default()
                                            }
                                        };
                                        index.add_kit_templates(&kit_templates.kits, &kit_templates.all);
                                        filtered = refilter(&index, kind, &filter, lens);
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
    let mut qm_acts: Vec<crate::quartermaster::Act> = Vec::new();
                    // ── `--ui-state`: put the window in the named state, once the reference data is
                    // in (names resolve), and start the countdown to the capture. ──
                    if let Some(st) = ui_state.take() {
                        // The state opens a string table, which reads on a worker before the capture.
                        let mut waits_on_text = false;
                        // The state opens a clip: (its hash, whether a hero plays it) — the capture waits
                        // for the clip's load on the hero, or for the picker's skeletons.
                        let mut waits_on_clip: Option<(u32, bool)> = None;
                        let applied = (|| -> Result<(), String> {
                            use crate::ui_state::pick;
                            let model = |name: &str| -> Result<(u32, String), String> {
                                index
                                    .models()
                                    .iter()
                                    .find(|r| r.label() == name)
                                    .map(|r| (r.hash, r.label()))
                                    .ok_or_else(|| format!("no model named {name:?} in the catalog"))
                            };
                            if let Some(p) = &st.page {
                                let mut pages: Vec<(&str, Workbench)> =
                                    Workbench::ALL.iter().map(|w| (w.label(), *w)).collect();
                                pages.push((Workbench::Live.label(), Workbench::Live));
                                wb = pick("page", p, &pages)?;
                            }
                            if let Some(s) = &st.settings_section {
                                let sections: Vec<(&str, SettingsSection)> =
                                    SettingsSection::ALL.iter().map(|s| (s.label(), *s)).collect();
                                settings_section = pick("settings section", s, &sections)?;
                            }
                            if let Some(t) = &st.tab {
                                let tabs: Vec<(&str, Kind)> = Kind::ALL.iter().map(|k| (k.label(), *k)).collect();
                                kind = pick("tab", t, &tabs)?;
                            }
                            if let Some(l) = &st.lens {
                                let mut lenses: Vec<(&str, Option<crate::domain::Domain>)> = vec![("All", None)];
                                lenses.extend(Workbench::lenses().into_iter().map(|d| (d.label(), Some(d))));
                                lens = pick("lens", l, &lenses)?;
                            }
                            if let Some(f) = &st.filter {
                                filter = f.clone();
                            }
                            filtered = refilter(&index, kind, &filter, lens);
                            sel = 0;
                            if let Some(name) = &st.hero {
                                let (hash, label) = model(name)?;
                                hero = Some(crate::settings::Hero { hash, label });
                            }
                            if let Some(name) = &st.load {
                                if kind == Kind::Model {
                                    let (hash, label) = model(name)?;
                                    actions.push(Act::LoadModelHash(hash, label));
                                } else {
                                    // The tab's row by its label, opened as its click opens it.
                                    let rows = index.rows(kind);
                                    let vi = filtered
                                        .iter()
                                        .position(|&ri| rows[ri].label() == *name)
                                        .ok_or_else(|| format!("no {} row named {name:?}", kind.label()))?;
                                    sel = vi;
                                    actions.push(Act::LoadRow(vi));
                                    waits_on_text = rows[filtered[vi]].type_id == mercs2_formats::types::TYPE_ID_STRINGDB;
                                    if kind == Kind::Animation {
                                        waits_on_clip = Some((rows[filtered[vi]].hash, hero.is_some()));
                                    }
                                }
                            }
                            if let Some(dir) = &st.shipment {
                                qm.open_shipment(dir, qm_names.as_ref());
                                if qm.root().is_none() {
                                    return Err(format!("{}: {}", dir.display(), qm.status()));
                                }
                            }
                            if let Some(i) = st.contribution {
                                if i >= qm.contribution_count() {
                                    return Err(format!(
                                        "contribution {i} is past the Shipment's {}",
                                        qm.contribution_count()
                                    ));
                                }
                                qm_acts.push(crate::quartermaster::Act::Select(i));
                            }
                            if let (Some(b), Some(d)) = (&st.bench, &st.donor) {
                                let benches = [
                                    (bench_page(crate::quartermaster::Craft::Rig).verb(), crate::quartermaster::Craft::Rig),
                                    (bench_page(crate::quartermaster::Craft::Conform).verb(), crate::quartermaster::Craft::Conform),
                                ];
                                let bench = pick("bench", b, &benches)?;
                                let (hash, label) = model(d)?;
                                actions.push(Act::Bench(bench, hash, label));
                            }
                            Ok(())
                        })();
                        if let Err(e) = applied {
                            eprintln!("--ui-state: {e}");
                            std::process::exit(2);
                        }
                        // The AUDIO keys wait for the tab's routing (read on a worker); the capture
                        // counts down once they are in. A run-only path waits for the reload it
                        // starts, through the same chains_dirty step a Settings click takes.
                        if st.run_wad.is_some() || st.run_data.is_some() {
                            if let Some(p) = &st.run_wad {
                                crate::chains::set_run_wad(p.clone());
                            }
                            if let Some(p) = &st.run_data {
                                crate::chains::set_run_data(p.clone());
                            }
                            chains_dirty = true;
                            reload_capture = Some((st.capture.clone(), st.settle_frames, boots_done));
                        } else if st.audio() {
                            audio_page.ensure_routes(std::path::Path::new(&wadpath));
                            audio_ui = Some(st);
                        } else if waits_on_text {
                            // A string table reads on a worker; the capture waits for it.
                            text_ui = Some((st.capture.clone(), st.settle_frames));
                        } else if let Some((clip, on_hero)) = waits_on_clip {
                            anim_ui = Some((st.capture.clone(), st.settle_frames, clip, on_hero, 0));
                        } else {
                            capture_to = Some((st.capture.clone(), st.settle_frames));
                        }
                    }
                    // An ANIMS row: on a hero, wait until the clip has loaded on it (decoded or
                    // refused); without one, until the picker's skeletons have read. Frames are
                    // counted so a clip that never resolves is a failure, not a hang.
                    if let Some((to, settle, clip, on_hero, frames)) = &mut anim_ui {
                        *frames += 1;
                        let material = matches!(clip_infos.get(clip), Some(Ok(i)) if i.material.is_some());
                        let ready = if material {
                            true
                        } else if *on_hero {
                            preview.as_ref().is_some_and(|p| p.clip_cache.contains_key(clip))
                        } else {
                            skeletons.is_some() || skel_error.is_some()
                        };
                        if ready {
                            capture_to = Some((to.clone(), *settle));
                            anim_ui = None;
                        } else if *frames > 60 * 120 {
                            eprintln!("--ui-state: clip 0x{clip:08X} did not load within 120 s of frames");
                            std::process::exit(2);
                        }
                    }
                    if let Some((to, settle)) = &text_ui {
                        if let Some(e) = text_page.error() {
                            eprintln!("--ui-state: the string tables did not read: {e}");
                            std::process::exit(2);
                        }
                        if text_page.tables().is_some() {
                            capture_to = Some((to.clone(), *settle));
                            text_ui = None;
                        }
                    }
                    if let Some((to, settle, at)) = &reload_capture {
                        if phase == Phase::Ready && boots_done > *at {
                            capture_to = Some((to.clone(), *settle));
                            reload_capture = None;
                        }
                    }
                    if audio_ui.is_some() {
                        audio_page.poll();
                        if let Some(e) = audio_page.routes_error() {
                            eprintln!("--ui-state: the AUDIO tab's routing failed: {e}");
                            std::process::exit(2);
                        }
                        if audio_page.routes().is_some() {
                            let st = audio_ui.take().expect("checked above");
                            if let Err(e) = crate::audiopage::apply_state(&mut audio_page, &st) {
                                eprintln!("--ui-state: {e}");
                                std::process::exit(2);
                            }
                            capture_to = Some((st.capture.clone(), st.settle_frames));
                        }
                    }
                    let mut saves_acts: Vec<crate::saves::Act> = Vec::new();
                    // Captured below inside `gui.run` so the saves reducer (which runs after the
                    // closure returns) can allocate egui textures for the hero-portrait render.
                    let mut pending_ctx: Option<egui::Context> = None;
                    let mut missions_acts: Vec<crate::missions::Act> = Vec::new();
                    let mut hovered_bone: Option<usize> = None;
                    // Skeleton workbench hover: (bone index, is_source_tree). Resolved to a viewer
                    // highlight in the PREVIEW's current space (source before Apply, target after).
                    let mut hover_skel: Option<(usize, bool)> = None;
                    if inventory_dirty {
                        vehicle_inventory = build_donor_inventory(&index, None);
                        inventory_dirty = false;
                    }
                    // The workbench whose PANELS and MARKERS to draw this frame. Taken ONCE per
                    // frame, before the GUI runs, so the panels, the viewport HUD and the 3D marker
                    // passes cannot disagree about which bench is showing when an action switches
                    // page mid-frame.
                    let eff = wb;
                    let rail_lit = wb.rail_entry();
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
                                ui.label(theme::disp_text("MERCS 2", theme::style_title(), theme::TX));
                                ui.label(theme::disp_text("WORKSHOP", theme::style_eyebrow(), theme::FAINT));
                                ui.separator();
                                // A bench works on one contribution of the open Shipment, so its
                                // breadcrumb is that path — SHIPMENTS › shipment › contribution ›
                                // BENCH — and every crumb but the bench leads back to the Shipment.
                                if wb.is_craft() {
                                    let crumb = |ui: &mut egui::Ui, text: egui::RichText| {
                                        ui.add(egui::Button::new(text).frame(false)).on_hover_text("Back to the Shipment").clicked()
                                    };
                                    let sep = |ui: &mut egui::Ui| {
                                        ui.label(egui::RichText::new("\u{203a}").color(theme::LINE2));
                                    };
                                    let mut back = crumb(ui, theme::disp_text("SHIPMENTS", theme::style_eyebrow(), theme::DIM));
                                    sep(ui);
                                    match qm.root() {
                                        Some(root) => {
                                            let leaf = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                                            back |= crumb(ui, egui::RichText::new(leaf).color(theme::TX));
                                        }
                                        None => {
                                            ui.label(egui::RichText::new("no shipment").weak());
                                        }
                                    }
                                    if let Some(name) = craft_subject.and_then(|i| qm.contribution_title(i)) {
                                        sep(ui);
                                        back |= crumb(ui, egui::RichText::new(name).color(theme::TX));
                                    }
                                    sep(ui);
                                    ui.label(theme::disp_text(wb.verb().to_uppercase(), theme::style_eyebrow(), theme::BRASS));
                                    if back {
                                        wb = Workbench::Quartermaster;
                                    }
                                } else {
                                    ui.label(theme::disp_text(wb.verb().to_uppercase(), theme::style_eyebrow(), theme::FAINT));
                                    // The breadcrumb names what the PAGE is working on. On the
                                    // Shipment page that is the Shipment — showing the previewed
                                    // model instead pointed at whatever Inspect happened to be holding.
                                    if wb == Workbench::Quartermaster {
                                        if let Some(root) = qm.root() {
                                            ui.label(
                                                egui::RichText::new(
                                                    root.file_name()
                                                        .map(|s| s.to_string_lossy().to_string())
                                                        .unwrap_or_default(),
                                                )
                                                .strong(),
                                            );
                                        } else {
                                            ui.label(egui::RichText::new("no shipment").weak());
                                        }
                                    } else if wb == Workbench::Inspect && kind == Kind::Audio {
                                        ui.label(
                                            egui::RichText::new(crate::audiopage::crumb(&audio_page).join(" \u{203a} "))
                                                .color(theme::TX),
                                        );
                                    } else if wb == Workbench::Inspect {
                                        let loaded = preview.as_ref().map(|p| p.label.as_str());
                                        ui.label(
                                            egui::RichText::new(inspect_crumb(kind, lens, loaded).join(" \u{203a} "))
                                                .color(theme::TX),
                                        );
                                        if let Some(p) = &preview {
                                            ui.label(
                                                egui::RichText::new(format!("0x{:08X}", p.hash))
                                                    .monospace()
                                                    .color(theme::BRASS_DK),
                                            );
                                        }
                                    } else if wb == Workbench::Settings {
                                        ui.label(egui::RichText::new(settings_section.label()).color(theme::TX));
                                    } else if let Some(p) = &preview {
                                        ui.label(egui::RichText::new(&p.label).strong());
                                        ui.label(
                                            egui::RichText::new(format!("0x{:08X}", p.hash))
                                                .monospace()
                                                .color(theme::BRASS_DK),
                                        );
                                    }
                                }
                            });
                        });
                        // ── Export dialog: the name, format and folder an export writes, then Export.
                        if let Some(d) = export_dialog.as_mut() {
                            let mut open = true;
                            egui::Window::new("Export")
                                .id(egui::Id::new("export_dialog"))
                                .collapsible(false)
                                .resizable(false)
                                .open(&mut open)
                                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                                .show(ctx, |ui| {
                                    ui.set_width(420.0);
                                    ui.label(theme::disp_text(d.label.to_uppercase(), theme::style_eyebrow(), theme::BRASS));
                                    ui.add_space(6.0);
                                    let name_state = if d.name.trim().is_empty() { theme::FieldState::Bad } else { theme::FieldState::Neutral };
                                    theme::text_field(ui, "Name", &mut d.name, "folder name", name_state);
                                    ui.add_space(6.0);
                                    theme::eyebrow(ui, "Format");
                                    for f in ExportFormat::ALL {
                                        let why = d.refusal(f);
                                        let r = ui
                                            .add_enabled_ui(why.is_none(), |ui| ui.radio_value(&mut d.format, f, f.label()))
                                            .inner;
                                        if let Some(w) = why {
                                            r.on_disabled_hover_text(w);
                                        }
                                        ui.label(egui::RichText::new(f.what()).small().color(theme::DIM));
                                    }
                                    ui.add_space(6.0);
                                    theme::eyebrow(ui, "Folder");
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(d.folder.display().to_string()).monospace().color(theme::TX));
                                        if ui.button("Choose\u{2026}").clicked() {
                                            actions.push(Act::ExportFolder);
                                        }
                                    });
                                    let suffix = d.outfit.map(|n| format!("_outfit{}", n + 1)).unwrap_or_default();
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "writes {}",
                                            d.folder.join(format!("{}{suffix}", export_dir_name(&d.name))).display()
                                        ))
                                        .small()
                                        .color(theme::FAINT),
                                    );
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        let ready = !d.name.trim().is_empty() && d.refusal(d.format).is_none() && exporter.is_none();
                                        if theme::primary_button(ui, "Export", ready).clicked() {
                                            actions.push(Act::RunExport);
                                        }
                                        if exporter.is_some() {
                                            ui.label(egui::RichText::new("another export is still running").small().color(theme::DIM));
                                        }
                                    });
                                });
                            if !open {
                                export_dialog = None;
                            }
                        }
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
                        // As the Rail design draws it: the numbered workbenches from the top, Live
                        // and Log pinned to the foot.
                        egui::SidePanel::left("rail")
                            .exact_width(theme::RAIL_W)
                            .resizable(false)
                            .frame(egui::Frame::none().fill(theme::G0).inner_margin(egui::Margin::symmetric(0.0, 6.0)))
                            .show(ctx, |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                // The foot is laid out first so it keeps its place however tall the
                                // window is.
                                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                    let log_item = theme::RailItem {
                                        index: None,
                                        label: Workbench::LOG_LABEL,
                                        icon: theme::RailIcon::Log,
                                        on: false,
                                        badge: None,
                                        dot: None,
                                        locked: None,
                                    };
                                    if theme::rail_item(ui, &log_item) {
                                        log_open = !log_open;
                                    }
                                    // While a chain has no answer every page but Settings is
                                    // locked: there is nothing loaded for it to show.
                                    let lock = (phase == Phase::Setup).then(|| {
                                        if game_chain.in_use().is_none() || matches!(boot_fail, Some((Which::Game, _))) {
                                            "Set the game archive first"
                                        } else {
                                            "Set the reference data first"
                                        }
                                    });
                                    let live_item = theme::RailItem {
                                        index: None,
                                        label: Workbench::Live.label(),
                                        icon: Workbench::Live.icon(),
                                        on: wb == Workbench::Live,
                                        badge: None,
                                        dot: Some(if live_probe.reachable() { theme::GOOD } else { theme::LINE2 }),
                                        locked: lock,
                                    };
                                    if theme::rail_item(ui, &live_item) {
                                        wb = Workbench::Live;
                                    }
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                            ui.spacing_mut().item_spacing.y = 1.0;
                                            for (i, w) in Workbench::ALL.iter().enumerate() {
                                                let item = theme::RailItem {
                                                    index: Some(i + 1),
                                                    label: w.label(),
                                                    icon: w.icon(),
                                                    on: rail_lit == *w,
                                                    // The Shipment carries a count of what BLOCKS, so
                                                    // the gate is visible from whatever page you are on.
                                                    badge: (*w == Workbench::Quartermaster).then(|| qm.blocking_count()),
                                                    dot: None,
                                                    locked: lock.filter(|_| *w != Workbench::Settings),
                                                };
                                                if theme::rail_item(ui, &item) {
                                                    wb = *w;
                                                }
                                            }
                                        });
                                    });
                                });
                            });
                        egui::TopBottomPanel::bottom("status")
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin::symmetric(14.0, 4.0)),
                            )
                            .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                // On the Saves workbench with a selection, the status bar
                                // speaks about the loaded `.profile` instead of the workshop's
                                // asset-load state — matches the mockup's "LOADED · 13,404 B ·
                                // checksum ok · payload inflated". Falls through to the normal
                                // workshop status for every other page and for Saves with no
                                // selection.
                                let saves_banner = (wb == Workbench::Saves)
                                    .then(|| crate::saves::saves_status_banner(&saves))
                                    .flatten();
                                if let Some((dot_color, dot_text, detail)) = saves_banner {
                                    theme::status_dot(ui, dot_text, dot_color);
                                    ui.separator();
                                    ui.label(egui::RichText::new(detail).color(theme::DIM).small());
                                } else if phase == Phase::Setup {
                                    // What is missing, named: the footer is the one line every page shows.
                                    let mut missing = Vec::new();
                                    if game_chain.in_use().is_none() || matches!(boot_fail, Some((Which::Game, _))) {
                                        missing.push("the game archive");
                                    }
                                    if (data_chain.in_use().is_none() && !data_chain.is_empty())
                                        || matches!(boot_fail, Some((Which::Data, _)))
                                    {
                                        missing.push("the reference data");
                                    }
                                    theme::status_dot(ui, "Setup", theme::BAD);
                                    ui.separator();
                                    ui.label(
                                        egui::RichText::new(format!("Set {} to load the game", missing.join(" and ")))
                                            .color(theme::DIM)
                                            .small(),
                                    );
                                } else {
                                    theme::status_dot(ui, "Ready", theme::GOOD);
                                    ui.separator();
                                    // While a voice-over line plays, the bar says which and from where.
                                    match vo_player.as_ref().and_then(|p| p.status()) {
                                        Some(s) => {
                                            let verb = if s.paused { "paused" } else { "playing" };
                                            ui.label(
                                                egui::RichText::new(format!("{verb} {:05} from vo_stream.english.pws", s.clip))
                                                    .color(theme::DIM)
                                                    .small(),
                                            );
                                        }
                                        None => {
                                            ui.label(egui::RichText::new(status.as_str()).color(theme::DIM).small());
                                        }
                                    }
                                }
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
                        // ── NAVIGATOR (left): the current workbench's list. The viewport keeps its
                        // camera when the workbench changes; only this + the inspector reconfigure. ──
                        // Not on Live: that page is blank until it is built to its design.
                        let navigator_resp = (eff != Workbench::Live).then(|| egui::SidePanel::left("navigator")
                            .resizable(true)
                            .default_width(navigator_width)
                            // Finite max: egui 0.28's SidePanel never clamps content to the panel
                            // width, and stores the CONTENT rect back as the width (panel.rs:279,328).
                            // With the default INFINITY max, one long asset name or hash grows the
                            // panel permanently. A bounded range caps that and gives egui a real wrap
                            // width, so content wraps to the panel instead of forcing it wider.
                            .width_range(egui::Rangef::new(220.0, 360.0))
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin { left: 13.0, right: 10.0, top: 12.0, bottom: 0.0 }),
                            )
                            .show(ctx, |ui| {
                          match eff {
                            Workbench::Live => unreachable!("Live has no navigator"),
                            Workbench::Quartermaster => {
                                qm_acts.extend(crate::quartermaster::navigator(ui, &qm));
                            }
                            Workbench::Saves => {
                                saves_acts.extend(crate::saves::navigator(ui, &saves));
                            }
                            Workbench::Inspect => {
                            let before = (kind, filter.clone(), lens);
                            ui.label(theme::disp_text("ASSETS", theme::style_title(), theme::TX));
                            ui.add_space(8.0);
                            // Segmented Models / Textures toggle — two EQUAL-width segments, each a
                            // centred "LABEL  count" unit, the active one filled. Custom-drawn so the
                            // segments are exactly half the pill and the text is truly centred.
                            // Eight categories now, so the old two-EQUAL-segment pill does not
                            // fit: they WRAP, and an empty category is dimmed but still listed —
                            // "this game has no X" is information, and hiding it would make the
                            // catalog look like it only knows about the types it happens to have.
                            egui::Frame::none()
                                .fill(theme::G0)
                                .stroke(egui::Stroke::new(1.0, theme::LINE))
                                .rounding(egui::Rounding::same(7.0))
                                .inner_margin(egui::Margin::same(4.0))
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
                                    ui.horizontal_wrapped(|ui| {
                                        for k in Kind::ALL {
                                            let count = index.count(k);
                                            let on = kind == k;
                                            let name = k.short();
                                            let p0 = ui.painter();
                                            let g_name = p0.layout_no_wrap(
                                                name.to_uppercase(),
                                                theme::font(&theme::style_eyebrow()),
                                                theme::TX,
                                            );
                                            let g_cnt = p0.layout_no_wrap(
                                                commafy(count),
                                                theme::font(&egui::TextStyle::Monospace),
                                                theme::FAINT,
                                            );
                                            let gap = 5.0;
                                            let w = g_name.size().x + gap + g_cnt.size().x + 14.0;
                                            let (rect, resp) = ui.allocate_exact_size(
                                                egui::vec2(w, 24.0),
                                                egui::Sense::click(),
                                            );
                                            let empty = count == 0;
                                            let p = ui.painter();
                                            let fill = if on {
                                                theme::G3
                                            } else if resp.hovered() && !empty {
                                                theme::G2
                                            } else {
                                                egui::Color32::TRANSPARENT
                                            };
                                            p.rect_filled(rect, egui::Rounding::same(5.0), fill);
                                            let name_col = if on {
                                                theme::TX
                                            } else if empty {
                                                theme::FAINT
                                            } else {
                                                theme::DIM
                                            };
                                            let cnt_col = if on { theme::BRASS } else { theme::FAINT };
                                            let (nsz, csz) = (g_name.size(), g_cnt.size());
                                            let x0 = rect.center().x - (nsz.x + gap + csz.x) / 2.0;
                                            let cy = rect.center().y;
                                            p.galley(egui::pos2(x0, cy - nsz.y / 2.0), g_name, name_col);
                                            p.galley(
                                                egui::pos2(x0 + nsz.x + gap, cy - csz.y / 2.0),
                                                g_cnt,
                                                cnt_col,
                                            );
                                            // Say what the category previews as, so a browse-only
                                            // one is understood before it is clicked.
                                            let resp = match k.preview() {
                                                crate::index::Preview::Render => {
                                                    resp.on_hover_text("previews on the pedestal")
                                                }
                                                crate::index::Preview::Plate => {
                                                    resp.on_hover_text("previews as a texture plate")
                                                }
                                                crate::index::Preview::View(what) => resp.on_hover_text(what),
                                                crate::index::Preview::None(why) => resp
                                                    .on_hover_text(format!("browse only — {why}")),
                                            };
                                            if resp.clicked() && !empty {
                                                kind = k;
                                            }
                                        }
                                    });
                                });
                            // The Lens: the Models list narrowed to one domain's models — the
                            // domains that are not rail pages of their own.
                            if kind == Kind::Model {
                                ui.add_space(8.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing.x = 3.0;
                                    ui.label(theme::disp_text("LENS", theme::style_eyebrow(), theme::FAINT));
                                    ui.add_space(4.0);
                                    let models = index.models();
                                    for d in std::iter::once(None).chain(Workbench::lenses().into_iter().map(Some)) {
                                        let n = models.iter().filter(|r| in_lens(d, r)).count();
                                        let name = d.map_or("All", |d| d.label());
                                        if theme::lens_pill(ui, name, &commafy(n), lens == d).clicked() {
                                            lens = d;
                                        }
                                    }
                                });
                            }
                            ui.add_space(7.0);
                            // Filter with a search glyph. The Audio tab searches its clips instead.
                            if kind != Kind::Audio {
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
                            }
                            if before.0 != kind || before.1 != filter || before.2 != lens {
                                filtered = refilter(&index, kind, &filter, lens);
                                sel = 0;
                            }
                            ui.add_space(6.0);
                            // Audio is the Audio page: the voice-over lines by the banks that play
                            // them, routed from English.wad, rather than the opaque bank assets.
                            if kind == Kind::Audio {
                                audio_page.ensure_routes(std::path::Path::new(&wadpath));
                                audio_acts.extend(crate::audiopage::navigator(ui, &audio_page));
                            } else {
                            // Grouped display: category headers interleaved with rows, all one line
                            // high so `show_rows` still virtualizes the (3000+ row) models list.
                            // `filtered` is category-sorted; a header opens each new run. `Disp::Row`
                            // carries the position in `filtered` so selection/keyboard stay unchanged.
                            enum Disp {
                                Header(String, usize),
                                Row(usize),
                            }
                            let mut display: Vec<Disp> = Vec::with_capacity(filtered.len() + 32);
                            if kind == Kind::Model {
                                let rows = index.rows(kind);
                                let groups: Vec<String> = filtered
                                    .iter()
                                    .map(|&ri| list_group(&rows[ri], lens, &index.faction_labels))
                                    .collect();
                                let mut counts: HashMap<&str, usize> = HashMap::new();
                                for g in &groups {
                                    *counts.entry(g.as_str()).or_default() += 1;
                                }
                                let mut last: Option<&str> = None;
                                for (vi, g) in groups.iter().enumerate() {
                                    if last != Some(g.as_str()) {
                                        display.push(Disp::Header(g.clone(), counts[g.as_str()]));
                                        last = Some(g.as_str());
                                    }
                                    // A collapsed group keeps its header but drops its rows.
                                    if !collapsed_cats.contains(g) {
                                        display.push(Disp::Row(vi));
                                    }
                                }
                            } else {
                                for vi in 0..filtered.len() {
                                    display.push(Disp::Row(vi));
                                }
                            }
                            let row_h = 19.0_f32.max(ui.text_style_height(&egui::TextStyle::Body));
                            // A selection that moved since the last frame (keys, F4, a state file)
                            // is scrolled just into view; a click lands on a row already in view.
                            let mut list_area = egui::ScrollArea::vertical().auto_shrink([false, false]);
                            if list_follow.0 != Some(sel) {
                                if let Some(di) = display.iter().position(|d| matches!(d, Disp::Row(v) if *v == sel)) {
                                    let step = row_h + ui.spacing().item_spacing.y;
                                    let (top, (off, view_h)) = (di as f32 * step, list_follow.1);
                                    // No viewport measured yet (the first frame): put the row on top.
                                    if view_h <= 0.0 || top < off {
                                        list_area = list_area.vertical_scroll_offset(top);
                                    } else if top + step > off + view_h {
                                        list_area = list_area.vertical_scroll_offset(top + step - view_h);
                                    }
                                }
                            }
                            let list_out = list_area.show_rows(
                                ui,
                                row_h,
                                display.len(),
                                |ui, range| {
                                    for di in range {
                                        let vi = match &display[di] {
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
                                                        cat.clone(),
                                                        theme::style_eyebrow(),
                                                        theme::FAINT,
                                                    ));
                                                    ui.label(
                                                        egui::RichText::new(n.to_string())
                                                            .monospace()
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
                                                        collapsed_cats.insert(cat.clone());
                                                    }
                                                }
                                                continue;
                                            }
                                            Disp::Row(vi) => *vi,
                                        };
                                        let r = &index.rows(kind)[filtered[vi]];
                                        let (hash, label) = (r.hash, r.label());
                                        let cat = if kind == Kind::Model { r.category() } else { "" };
                                        // `+ ` marks an overlay's row; a side WAD's names its WAD after.
                                        let side = r.src >= w.wads.len();
                                        let mark = if r.src > 0 && !side { "+ " } else { "" };
                                        let row = ui
                                            .horizontal(|ui| {
                                                if kind == Kind::Model {
                                                    ui.add_space(2.0);
                                                    row_icon(ui, cat, 15.0);
                                                    ui.add_space(5.0);
                                                }
                                                let mut text = egui::text::LayoutJob::default();
                                                let body = egui::TextFormat {
                                                    font_id: egui::TextStyle::Body.resolve(ui.style()),
                                                    color: ui.visuals().text_color(),
                                                    ..Default::default()
                                                };
                                                text.append(&format!("{mark}{label}"), 0.0, body.clone());
                                                if side {
                                                    text.append(&w.tag(r.src), 0.0, egui::TextFormat { color: theme::FAINT, ..body });
                                                }
                                                ui.selectable_label(vi == sel, text)
                                            })
                                            .inner;
                                        if row.clicked() {
                                            sel = vi;
                                            actions.push(Act::LoadRow(vi));
                                        }
                                        row.context_menu(|ui| {
                                            // A layer is a set of placements you switch on or move,
                                            // not an asset you preview or donate.
                                            if kind == Kind::Layer {
                                                layer_menu(ui, hash, &label, &mut actions);
                                                return;
                                            }
                                            // ONE menu, shared with the inspector heading — the
                                            // two used to be separate copies and had drifted.
                                            asset_menu(
                                                ui,
                                                hash,
                                                &label,
                                                subject_of(kind, r),
                                                Some(Act::LoadRow(vi)),
                                                &mut actions,
                                            );
                                        });
                                    }
                                },
                            );
                            list_follow = (Some(sel), (list_out.state.offset.y, list_out.inner_rect.height()));
                            }
                            } // ── end Inspect navigator (asset browser) ──
                            Workbench::Mods => {
                                ui.label(theme::disp_text("Donor templates", theme::style_title(), theme::TX));
                                ui.weak("Pick ANY host template — vehicle, character, building or prop \
                                         — it becomes the conform donor whose container hosts the \
                                         injected geometry.");
                                ui.separator();
                                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                    if vehicle_inventory.is_empty() {
                                        ui.weak("(loading catalog…)");
                                    }
                                    for (class, rows) in &vehicle_inventory {
                                        egui::CollapsingHeader::new(theme::disp_text(
                                            format!("{class}  ({})", rows.len()),
                                            theme::style_title(),
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
                                // The twin-tree view needs BOTH a target skeleton AND an imported
                                // (retargeted) source. Gating only on the target left a dead state:
                                // enter Rig with a target set but no mesh imported and the inner
                                // `if let` on `retarget` failed with no `else`, so the whole navigator
                                // rendered NOTHING — the panel looked like it vanished. Require the
                                // import here, so every other state falls through to the picker branch
                                // below (which shows "no rigged import loaded" + the target list).
                                let have_target = retarget_target.is_some();
                                // TWIN-TREE view once a target skeleton is chosen: donor (Mercs2) on top,
                                // imported (foreign) on the bottom. Hovering any bone highlights it (and the
                                // mesh it drives) in the viewer. The picker returns via "change target".
                                if have_target && !show_target_picker && retarget.is_some() {
                                    if let (Some(r), Some((_, tl))) = (&retarget, &retarget_target) {
                                        ui.horizontal(|ui| {
                                            ui.label(theme::disp_text("Skeletons", theme::style_title(), theme::TX));
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
                                    ui.label(theme::disp_text("Skeleton source", theme::style_title(), theme::TX));
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
                                        for r in index.models().iter() {
                                            let label = r.label();
                                            let ll = label.to_ascii_lowercase();
                                            let humanoid = ll.contains("hum") || ll.contains("merc")
                                                || ll.contains("char") || ll.contains("ped") || ll.contains("bip");
                                            if !humanoid || (!f.is_empty() && !ll.contains(&f)) {
                                                continue;
                                            }
                                            let on = retarget_target.as_ref().is_some_and(|(h, l)| *h == r.hash && *l == label);
                                            if ui.selectable_label(on, label.clone()).clicked() {
                                                show_target_picker = false;
                                                actions.push(Act::RetargetSetTarget(r.hash, label.clone()));
                                            }
                                        }
                                    });
                                }
                            }
                            // ── DOMAIN: a curated lens over the catalog. The navigator is the only
                            // domain-specific surface; the viewport, inspector and verbs are Inspect's,
                            // so picking a model here shows it there exactly as browsing Inspect would.
                            Workbench::Domain(crate::domain::Domain::Missions) => {
                                // The Missions rail opens the Blueprint-style node editor rather
                                // than the generic script-list lens — the follow-up to this file
                                // keeps that lens one click away in the picker inside the editor.
                                missions_acts.extend(crate::missions::navigator(ui, &missions_panel));
                            }
                            Workbench::Domain(d) => {
                                ui.label(theme::disp_text(d.label().to_uppercase(), theme::style_title(), theme::TX));
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(d.blurb()).small().color(theme::FAINT));
                                ui.add_space(9.0);
                                if d.browses_models() {
                                    // The lens-filtered model list, grouped by the Library's own
                                    // category so a domain view and the Library never disagree.
                                    let mut rows: Vec<&crate::index::AssetRow> =
                                        index.models().iter().filter(|r| d.model_lens(r)).collect();
                                    rows.sort_by(|a, b| {
                                        crate::index::category_order(a.category())
                                            .cmp(&crate::index::category_order(b.category()))
                                            .then_with(|| a.label().cmp(&b.label()))
                                    });
                                    ui.label(theme::disp_text(
                                        format!("{} model(s)", rows.len()),
                                        theme::style_eyebrow(),
                                        theme::FAINT,
                                    ));
                                    ui.add_space(4.0);
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        let mut last_cat = "";
                                        for r in rows {
                                            let cat = r.category();
                                            if cat != last_cat {
                                                ui.add_space(4.0);
                                                theme::eyebrow(ui, cat);
                                                last_cat = cat;
                                            }
                                            let sel_now = preview.as_ref().is_some_and(|p| {
                                                let shown = p.kit.as_ref().and_then(|(_, s)| s.as_ref());
                                                r.is(p.hash, shown.map(|(t, _)| t.as_str()))
                                            });
                                            let resp = theme::row_chip(
                                                ui,
                                                if sel_now { theme::BRASS_SOFT } else { egui::Color32::TRANSPARENT },
                                                theme::LINE,
                                                |ui| {
                                                    ui.label(egui::RichText::new(r.label()).small()
                                                        .color(if sel_now { theme::BRASS } else { theme::TX }));
                                                },
                                            );
                                            if resp.clicked() {
                                                actions.push(Act::LoadModelHash(r.hash, r.label()));
                                            }
                                            // Route an asset into a Shipment or a bench — the same menu
                                            // the Library uses, so the two cannot drift.
                                            resp.context_menu(|ui| {
                                                asset_menu(ui, r.hash, &r.label(), subject_of(Kind::Model, r), None, &mut actions);
                                            });
                                        }
                                    });
                                } else {
                                    // Missions / Systems: not model rows, but NOT empty either — each
                                    // browses the real base-game content it governs.
                                    // Reserve a footer for the always-present "Add to this domain" row.
                                    let list_h = (ui.available_height() - 52.0).max(80.0);
                                    if d.browses_scripts() {
                                        // Missions / Systems: the decompiled Lua that governs this
                                        // domain, keyed off the governing needles. Click to read; right-
                                        // click to patch it (patch_lua seeded with the script name).
                                        let mut hits: Vec<&(String, String, String)> =
                                            lua_corpus.iter().filter(|(p, _, _)| d.script_lens(p)).collect();
                                        hits.sort_by(|a, b| a.0.cmp(&b.0));
                                        ui.label(theme::disp_text(format!("{} script(s)", hits.len()), theme::style_eyebrow(), theme::FAINT));
                                        ui.add_space(3.0);
                                        egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                                            for (path, _, _) in hits {
                                                let stem = path.rsplit(['/', '\\']).next().unwrap_or(path).trim_end_matches(".lua").to_string();
                                                let resp = theme::row_chip(ui, egui::Color32::TRANSPARENT, theme::LINE, |ui| {
                                                    ui.label(egui::RichText::new(&stem).monospace().color(theme::TX));
                                                });
                                                if resp.clicked() {
                                                    actions.push(Act::LuaOpen(path.clone()));
                                                }
                                                let stem2 = stem.clone();
                                                let kinds = d.kinds();
                                                resp.context_menu(|ui| {
                                                    ui.label(theme::disp_text(stem2.to_uppercase(), theme::style_eyebrow(), theme::BRASS));
                                                    ui.separator();
                                                    if ui.button("Open in Lua viewer").clicked() {
                                                        actions.push(Act::LuaOpen(path.clone()));
                                                        ui.close_menu();
                                                    }
                                                    ui.menu_button("Add to Shipment", |ui| {
                                                        for kind in kinds {
                                                            if ui.button(*kind).clicked() {
                                                                actions.push(Act::AddToShipment(kind, 0, stem2.clone()));
                                                                ui.close_menu();
                                                            }
                                                        }
                                                    });
                                                });
                                            }
                                        });
                                    }

                                    // Always-present footer: the kinds this domain writes, one click to
                                    // scaffold. The visible twin of the background right-click above.
                                    ui.add_space(6.0);
                                    theme::eyebrow(ui, "Add to this domain");
                                    ui.horizontal_wrapped(|ui| {
                                        for kind in d.kinds() {
                                            if ui.small_button(*kind).clicked() {
                                                actions.push(Act::AddToShipment(kind, 0, String::new()));
                                            }
                                        }
                                    });
                                }
                            }
                            // ── SETTINGS: the sections, the two path chains badged red when they
                            // have no answer and CHECK when a set source below the one in use is
                            // broken. ──
                            Workbench::Settings => {
                                let game_badge = crate::settingspage::badge(
                                    &game_chain,
                                    matches!(boot_fail, Some((Which::Game, _))),
                                    false,
                                );
                                let data_badge = crate::settingspage::badge(
                                    &data_chain,
                                    matches!(boot_fail, Some((Which::Data, _))),
                                    true,
                                );
                                if let Some(s) = crate::settingspage::navigator(ui, settings_section, game_badge, data_badge) {
                                    settings_section = s;
                                }
                            }
                          } // ── end match wb (navigator) ──
                        }));
                        if let Some(r) = navigator_resp {
                            navigator_width = r.response.rect.width();
                        }
                        // ── INSPECTOR (right): the current workbench's detail cards. ──
                        // ── INSPECTOR (right). NOT registered on Settings: that page has a
                        // navigator and a centre and nothing a detail panel would hold, and an
                        // empty registered panel reads as something that failed to load rather
                        // than something that was never meant to be there.
                        let inspector_resp = (eff != Workbench::Settings && eff != Workbench::Live).then(|| egui::SidePanel::right("inspector")
                            .resizable(true)
                            .default_width(inspector_width)
                            // See the navigator's note: bound the width so content wraps to the panel
                            // rather than growing it (egui 0.28 stores the content rect as the width).
                            .width_range(egui::Rangef::new(300.0, 480.0))
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin { left: 13.0, right: 12.0, top: 12.0, bottom: 0.0 }),
                            )
                            .show(ctx, |ui| {
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                              if eff == Workbench::Quartermaster {
                                  qm_acts.extend(crate::quartermaster::inspector(ui, &qm, &qm_stack, true));
                              }
                              if eff == Workbench::Saves {
                                  saves_acts.extend(crate::saves::inspector(ui, &saves));
                              }
                              // The Missions editor owns the right pane entirely — the generic
                              // Inspect-style preview panel would compete with the node details.
                              let is_missions_bench = eff == Workbench::Domain(crate::domain::Domain::Missions);
                              if is_missions_bench {
                                  missions_acts.extend(crate::missions::inspector(ui, &missions_panel));
                              }
                              // The Audio tab's right pane is the clip and the export.
                              let audio_tab = eff == Workbench::Inspect && kind == Kind::Audio;
                              if audio_tab {
                                  audio_acts.extend(crate::audiopage::inspector(ui, &audio_page));
                              }
                              // The Anims tab: the hero clips play on, and the clip chosen. The hero's
                              // own model cards follow, as for any loaded model.
                              if eff == Workbench::Inspect && kind == Kind::Animation {
                                  theme::card(ui, "Hero", hero.as_ref().map(|h| h.label.as_str()), |ui| {
                                      match &hero {
                                          None => {
                                              ui.weak("No hero \u{2014} pick the model clips play on.");
                                          }
                                          Some(h) => {
                                              ui.label(egui::RichText::new(&h.label).color(theme::TX));
                                              ui.label(egui::RichText::new(format!("0x{:08X} \u{b7} kept across tabs and launches", h.hash)).small().color(theme::DIM));
                                          }
                                      }
                                      let pick = if hero.is_some() { "Change\u{2026}" } else { "Pick\u{2026}" };
                                      if ui.button(pick).clicked() {
                                          actions.push(Act::HeroPick);
                                      }
                                  });
                                  if let Some((clip, label)) = &anim_clip {
                                      match clip_infos.get(clip) {
                                          // A material animation: every track's curve, sampled as the
                                          // retail sampler does, against one playhead.
                                          Some(Ok(ClipInfo { material: Some(m), block, wad, .. })) => {
                                              let name = |h: u32| index.names.get(&h).cloned().unwrap_or_else(|| format!("0x{h:08X}"));
                                              theme::card(ui, "Material animation", Some(&format!("0x{clip:08X}")), |ui| {
                                                  ui.label(egui::RichText::new(label).color(theme::TX));
                                                  theme::kv(ui, "groups \u{b7} tracks", egui::RichText::new(format!("{} \u{b7} {}", m.groups.len(), m.track_count())));
                                                  let (lo, hi) = m.time_span();
                                                  theme::kv(ui, "key times", egui::RichText::new(format!("{lo} \u{2013} {hi}")));
                                                  theme::kv(ui, "MANM f32 @8", egui::RichText::new(format!("{:.5}", m.header_f32())));
                                                  ui.label(
                                                      egui::RichText::new("key-time unit not established: the playhead is in key units")
                                                          .small()
                                                          .color(theme::DIM),
                                                  );
                                                  theme::kv(ui, "read from", egui::RichText::new(format!("{wad} \u{b7} block {block}")));
                                                  theme::scrub(
                                                      ui,
                                                      (ui.available_width() - 80.0).max(60.0),
                                                      egui::Slider::new(&mut manm_t, lo as f32..=hi.max(lo + 1) as f32).text("at"),
                                                  );
                                                  for g in &m.groups {
                                                      ui.add_space(6.0);
                                                      theme::eyebrow(ui, &format!("group {}", name(g.hash)));
                                                      for t in &g.tracks {
                                                          theme::kv(ui, &name(t.property), egui::RichText::new(format!("{:.4}", t.sample(manm_t))));
                                                          ui.label(
                                                              egui::RichText::new(format!("{} keys \u{b7} \u{f7} {}", t.keys.len(), t.divisor))
                                                                  .small()
                                                                  .color(theme::DIM),
                                                          );
                                                          if let Some(x) = theme::curve(ui, 28.0, lo as f32, hi.max(lo + 1) as f32, manm_t, |x| t.sample(x)) {
                                                              manm_t = x;
                                                          }
                                                      }
                                                  }
                                              });
                                          }
                                          Some(Ok(i)) => {
                                              theme::card(ui, "Clip", Some(&format!("0x{clip:08X}")), |ui| {
                                                  ui.label(egui::RichText::new(label).color(theme::TX));
                                                  theme::kv(ui, "stored as", egui::RichText::new(if i.decoded { i.codec.to_string() } else { format!("{} \u{b7} frames not decoded", i.codec) }));
                                                  theme::kv(ui, "length", egui::RichText::new(format!("{:.2} s \u{b7} {} frames", i.duration, i.frames)));
                                                  theme::kv(ui, "tracks", egui::RichText::new(i.bones.len().to_string()));
                                                  theme::kv(ui, "read from", egui::RichText::new(format!("{} \u{b7} block {}", i.wad, i.block)));
                                                  // On the hero: how many of its bones the clip drives,
                                                  // and the bones it names that the hero lacks.
                                                  if let (Some(h), Some(p)) = (&hero, &preview) {
                                                      if p.hash == h.hash {
                                                          let missing: Vec<u32> = i.bones.iter().copied().filter(|b| !p.hier.contains(b)).collect();
                                                          theme::kv(ui, "bound on the hero", egui::RichText::new(format!("{}/{}", i.bones.len() - missing.len(), i.bones.len())));
                                                          if !missing.is_empty() {
                                                              let names: Vec<String> = missing
                                                                  .iter()
                                                                  .map(|b| index.names.get(b).cloned().unwrap_or_else(|| format!("0x{b:08X}")))
                                                                  .collect();
                                                              ui.label(egui::RichText::new(format!("not on the hero: {}", names.join(", "))).small().color(theme::DIM));
                                                          }
                                                      }
                                                  }
                                              });
                                          }
                                          Some(Err(e)) => {
                                              theme::card(ui, "Clip", Some(&format!("0x{clip:08X}")), |ui| {
                                                  ui.label(egui::RichText::new(label).color(theme::TX));
                                                  ui.label(egui::RichText::new(e).color(theme::BAD));
                                              });
                                          }
                                          None => {}
                                      }
                                  }
                              }
                              // The Text tab with a string table or a font open.
                              let text_tab = eff == Workbench::Inspect
                                  && kind == Kind::Text
                                  && (text_page.table.is_some() || text_page.font.is_some());
                              if text_tab {
                                  if text_page.font.is_some() {
                                      text_acts.extend(crate::textpage::font_inspector(ui, &text_page));
                                  } else {
                                      text_acts.extend(crate::textpage::inspector(ui, &text_page));
                                  }
                              }
                              if eff.inspect_like() && !is_missions_bench && !audio_tab && !text_tab {
                                match &mut preview {
                                    None => {
                                        ui.weak("No model loaded — pick one in the browser at left.");
                                        // The active Lens: what it holds, and the contributions it
                                        // starts.
                                        if let (Workbench::Inspect, Kind::Model, Some(d)) = (eff, kind, lens) {
                                            ui.add_space(10.0);
                                            let n = index.models().iter().filter(|r| in_lens(Some(d), r)).count();
                                            theme::section(ui, &format!("{} lens", d.label()), Some(&format!("{n} models")), true, |ui| {
                                                ui.label(egui::RichText::new(d.blurb()).small().color(theme::DIM));
                                                ui.add_space(8.0);
                                                theme::eyebrow(ui, "Start a contribution from here");
                                                ui.horizontal_wrapped(|ui| {
                                                    for k in d.kinds() {
                                                        let title = crate::quartermaster::kind_title(k);
                                                        if ui.button(title).on_hover_text(*k).clicked() {
                                                            actions.push(Act::AddToShipment(*k, 0, String::new()));
                                                        }
                                                    }
                                                });
                                            });
                                        }
                                    }
                                    Some(p) => {
                                        let (phash, plabel) = (p.hash, p.label.clone());
                                        let head = ui.add(
                                            egui::Label::new(theme::disp_text(
                                                plabel.to_uppercase(),
                                                egui::TextStyle::Heading,
                                                theme::TX,
                                            ))
                                            .sense(egui::Sense::click()),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!("0x{:08X}", p.hash))
                                                .monospace()
                                                .color(theme::DIM),
                                        );
                                        ui.add_space(8.0);
                                        head.context_menu(|ui| {
                                            // The SAME menu as the browser row. These were two
                                            // copies: this one offered only OBJ export, and neither
                                            // had been revisited since before Shipments existed.
                                            asset_menu(ui, phash, &plabel, model_subject(&index, phash, &plabel), None, &mut actions);
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
                                                            ui.label(theme::disp_text(label.to_uppercase(), theme::style_eyebrow(), theme::BRASS));
                                                            ui.label(egui::RichText::new(hint).small().color(theme::FAINT));
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
                                                                        .color(if hit { theme::TX } else { theme::DIM }),
                                                                );
                                                                ui.add_space(4.0);
                                                                ui.label(
                                                                    egui::RichText::new(format!("0x{mask:02X}"))
                                                                        .monospace()
                                                                        .color(theme::FAINT),
                                                                );
                                                                ui.add_space(8.0);
                                                                ui.label(
                                                                    egui::RichText::new(format!(
                                                                        "{n} mesh{}",
                                                                        if n == 1 { "" } else { "es" }
                                                                    )).small()
                                                                    .color(theme::DIM),
                                                                );
                                                                ui.with_layout(
                                                                    egui::Layout::right_to_left(egui::Align::Center),
                                                                    |ui| {
                                                                        if hit {
                                                                            ui.label(egui::RichText::new("✔ passes").small().color(theme::GOOD));
                                                                        } else {
                                                                            ui.label(egui::RichText::new("✖ filtered").small().color(theme::FAINT));
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
                                        // Character kit — every outfit the game can give each world
                                        // template that wears this kit (`mercs2_formats::mixr`: the
                                        // exe's per-profile enumeration). Which one a spawn shows is
                                        // the game's pick; each pill shows one of them.
                                        if let Some((kit, shown)) = &p.kit {
                                            let k_badge = format!(
                                                "{} templates · {} parts",
                                                kit.templates.len(),
                                                kit.mixr.parts.len()
                                            );
                                            theme::section(ui, "Character kit", Some(&k_badge), true, |ui| {
                                                for t in &kit.templates {
                                                    let name = t.label();
                                                    theme::eyebrow(ui, &name);
                                                    ui.add_space(3.0);
                                                    match kit.mixr.template_outfits(t) {
                                                        Ok((_, outfits)) => {
                                                            ui.horizontal_wrapped(|ui| {
                                                                for k in 0..outfits.len() {
                                                                    let on = shown
                                                                        .as_ref()
                                                                        .is_some_and(|(s, i)| *s == name && *i == k);
                                                                    if theme::pill(ui, &(k + 1).to_string(), on).clicked() {
                                                                        actions.push(Act::Outfit(name.clone(), k));
                                                                    }
                                                                }
                                                            });
                                                        }
                                                        Err(e) => {
                                                            ui.weak(e);
                                                        }
                                                    }
                                                    ui.add_space(6.0);
                                                }
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
                                                    let rail = ui.spacing().slider_width;
                                                    if theme::scrub(
                                                        ui,
                                                        rail,
                                                        egui::Slider::new(&mut hp, 0.0..=100.0).suffix("%").fixed_decimals(0),
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
                                                // Tier-3: the decoded enter/exit COMMAND SCRIPTS (raw
                                                // hashes → SHOW/HIDE/SetState) are format-level detail,
                                                // hidden behind Advanced so the node/state picker stays
                                                // clean for the common case.
                                                let show_scripts = theme::advanced(
                                                    ui,
                                                    "destr-scripts",
                                                    |ui| {
                                                        ui.label(theme::disp_text(
                                                            "enter/exit command scripts, shown per selected state below",
                                                            theme::style_eyebrow(),
                                                            theme::FAINT,
                                                        ));
                                                    },
                                                );
                                                ui.add_space(3.0);
                                                egui::ScrollArea::vertical()
                                                    .max_height(620.0)
                                                    .id_source("machine_scroll")
                                                    .show(ui, |ui| {
                                                        for (ni, node) in sm.nodes.iter().enumerate() {
                                                            let cur = p.node_state.get(ni).copied().unwrap_or(0);
                                                            ui.label(theme::disp_text(
                                                                name_or_hash(&index, node.name_hash),
                                                                theme::style_eyebrow(),
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
                                                            if show_scripts {
                                                                if let Some(st) = node.states.get(cur) {
                                                                    let enter = mercs2_formats::orchestrator::decode_script(&st.enter, resolve);
                                                                    if !enter.is_empty() {
                                                                        ui.label(egui::RichText::new(format!("→ {enter}")).monospace().color(theme::FAINT));
                                                                    }
                                                                    let exit = mercs2_formats::orchestrator::decode_script(&st.exit, resolve);
                                                                    if !exit.is_empty() {
                                                                        ui.label(egui::RichText::new(format!("← {exit}")).monospace().color(theme::FAINT));
                                                                    }
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
                                                                egui::RichText::new(tex_short).small()
                                                                    .color(if drawn { theme::TX } else { theme::DIM }),
                                                            );
                                                            ui.label(
                                                                egui::RichText::new(lod_levels(d.lod_mask))
                                                                    .monospace()
                                                                    .color(theme::FAINT),
                                                            );
                                                            ui.with_layout(
                                                                egui::Layout::right_to_left(egui::Align::Center),
                                                                |ui| {
                                                                    ui.label(egui::RichText::new(statustxt).small().color(statusc));
                                                                    ui.add_space(6.0);
                                                                    ui.label(egui::RichText::new(format!("{tris} tri")).monospace().color(theme::DIM));
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
                                                    theme::scrub(
                                                        ui,
                                                        ui.available_width(),
                                                        egui::Slider::new(&mut p.anim_time, 0.0..=dur).show_value(false),
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
                                                            ui.label(egui::RichText::new(nm).small().color(if sel { theme::BRASS } else { theme::TX }));
                                                            ui.label(egui::RichText::new(format!("0x{chash:08X}")).monospace().color(theme::FAINT));
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                if let Some(d) = dur {
                                                                    ui.label(egui::RichText::new(format!("{d:.2}s")).monospace().color(theme::GOOD));
                                                                } else if unbound {
                                                                    ui.label(egui::RichText::new("unbound").small().color(theme::FAINT));
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
                                                        let fmt = |color: egui::Color32| egui::TextFormat {
                                                            font_id: theme::font(&egui::TextStyle::Monospace),
                                                            color,
                                                            ..Default::default()
                                                        };
                                                        job.append(&"  ".repeat(depth), 0.0, fmt(theme::FAINT));
                                                        job.append(&format!("{i:>3}  "), 0.0, fmt(theme::FAINT));
                                                        job.append(&bname, 0.0, fmt(if named { theme::TX } else { theme::DIM }));
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
                              if matches!(eff, Workbench::Mods) {
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
                                                hp_names = resolve_node_names(&hashes, &index.names);
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
                                                                        egui::Label::new(theme::disp_text(name, theme::style_eyebrow(), col))
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
                                            .on_hover_text("draw the donor template at the origin as a visual anchor")
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
                              if matches!(eff, Workbench::Skeleton) {
                                ui.label(theme::disp_text("Skeleton retarget", egui::TextStyle::Heading, theme::TX));
                                match &retarget {
                                    None => {
                                        ui.add_space(4.0);
                                        if ui
                                            .add_enabled(phase == Phase::Ready, egui::Button::new("Import rigged model…"))
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
                        }));
                        // Remember the (possibly just-dragged) width so it survives a card collapse.
                        // Absent on Settings, where the panel is not registered — keep the last
                        // width so returning to another page restores the drag rather than resetting it.
                        if let Some(r) = &inspector_resp {
                            inspector_width = r.response.rect.width();
                        }

                        // ── VERB-BAR: the current workbench's primary actions.
                        //
                        // Registered AFTER the navigator and inspector, and as a TOP panel, so it
                        // spans only the CENTRE column. As a `bottom` panel declared before the
                        // side panels it ran the full width of the window, which put a second
                        // full-width bar under the status bar and read as chrome belonging to the
                        // whole app rather than verbs belonging to the page.
                        //
                        // brass = go, hazard = the irreversible. Every verb shows its shortcut.
                        // Not on Live: that page is blank until it is built to its design. ──
                        if eff != Workbench::Live {
                        egui::TopBottomPanel::top("verbs")
                            .frame(
                                egui::Frame::side_top_panel(&ctx.style())
                                    .inner_margin(egui::Margin::symmetric(13.0, 5.0)),
                            )
                            .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                let has_preview = preview.is_some();
                                match eff {
                                    Workbench::Quartermaster => {
                                        qm_acts.extend(crate::quartermaster::verbs(ui, &qm, true));
                                    }
                                    Workbench::Saves => {
                                        saves_acts.extend(crate::saves::verbs(ui, &saves));
                                    }
                                    // Missions owns its own verb bar: Jump-to-source / Add node /
                                    // Compile, which is what a blueprint editor needs, not Export /
                                    // textures.
                                    Workbench::Domain(crate::domain::Domain::Missions) => {
                                        missions_acts.extend(crate::missions::verbs(ui, &missions_panel));
                                    }
                                    // The Audio tab's verbs act on its clips: Play, Select all, Add to
                                    // Shipment, Export.
                                    // An open string table's verbs act on it, not on a model; a font
                                    // has none (no kind takes one).
                                    Workbench::Inspect
                                        if kind == Kind::Text && (text_page.table.is_some() || text_page.font.is_some()) =>
                                    {
                                        text_acts.extend(crate::textpage::verbs(ui, &text_page));
                                    }
                                    Workbench::Inspect if kind == Kind::Audio => {
                                        let playing = vo_player.as_ref().and_then(|p| p.status());
                                        audio_acts.extend(crate::audiopage::verbs(ui, &audio_page, playing));
                                    }
                                    // A domain page shares Inspect's verbs — it is Inspect narrowed to
                                    // a lens, so Export / textures / clip act on the same loaded
                                    // model. (For a non-model domain there is no preview, so these
                                    // sit disabled, which reads correctly as "nothing loaded".)
                                    Workbench::Inspect | Workbench::Domain(_) => {
                                        if ui.add_enabled(has_preview, egui::Button::new("Export  F10")).clicked() {
                                            if let Some(p) = &preview {
                                                actions.push(Act::OpenExport(p.hash, p.label.clone()));
                                            }
                                        }
                                        ui.separator();
                                        if ui.add_enabled(has_preview, egui::Button::new("View textures  F3")).clicked() {
                                            actions.push(Act::TexOfPreview);
                                        }
                                        if ui.add_enabled(has_preview, egui::Button::new("Next clip  F4")).clicked() {
                                            actions.push(Act::ClipNav(1));
                                        }
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
                                            // Publishing is a DANGER verb, not a secondary one — the
                                            // Mods bench styles "Publish patch WAD" the same way.
                                            // It is the commit: everything else on this page is
                                            // reversible, and this one writes a folder an author
                                            // then ships. Rightmost, matching that bench.
                                            if theme::danger_button(ui, "Export Shipment", can_export)
                                                .on_hover_text(
                                                    "Write a buildable Shipment (manifest.yaml + src/) carrying THIS bone map, \
                                                     hand adjustments included — then `qm build` it into a patch WAD",
                                                )
                                                .clicked()
                                            {
                                                actions.push(Act::ExportShipment);
                                            }
                                            if theme::primary_button(ui, "Apply retarget", ready).clicked() {
                                                actions.push(Act::RetargetApply);
                                            }
                                        });
                                    }
                                    // The page itself is the whole surface; the command bar just says
                                    // where the file being edited actually is.
                                    Workbench::Settings => {
                                        let loc = crate::settings::config_path()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|| "(no config directory)".into());
                                        ui.label(
                                            egui::RichText::new(loc).color(theme::FAINT).small(),
                                        );
                                    }
                                    Workbench::Live => unreachable!("Live has no verb bar"),
                                }
                            });
                        });
                        }

                        // -- SETTINGS page: the navigator lists the sections, the CENTRE holds the selected one.
                        //
                        // The whole form used to live in the navigator, which left this centre panel and the
                        // inspector registered and EMPTY -- two blank panels on the one page with nothing to put
                        // in them. Settings is now shaped like every other page, and the inspector is simply not
                        // registered here (see its `wb != Settings` guard) rather than being drawn blank.
                        if eff == Workbench::Live {
                            crate::live::center(ctx);
                        }
                        if wb == Workbench::Settings {
                            egui::CentralPanel::default()
                                .frame(
                                    egui::Frame::none()
                                        .fill(theme::G0)
                                        .inner_margin(egui::Margin::symmetric(22.0, 18.0)),
                                )
                                .show(ctx, |ui| {
                                    ui.label(theme::disp_text(settings_section.label().to_uppercase(), egui::TextStyle::Heading, theme::TX));
                                    ui.add_space(12.0);
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {

                                    // Tail of a path — the part that identifies it. The full string is
                                    // always one hover away, so the chip never has to carry 80 characters.
                                    let tail = |p: &std::path::Path| -> String {
                                        let n: Vec<_> = p.components().rev().take(2).collect();
                                        n.iter()
                                            .rev()
                                            .map(|c| c.as_os_str().to_string_lossy())
                                            .collect::<Vec<_>>()
                                            .join("/")
                                    };
                                    // A path row: green when the user set it, neutral when it was found
                                    // by the fallback chain. State is the fill, not a sentence.
                                    let path_chip = |ui: &mut egui::Ui, set: bool, text: &str, hover: &str| {
                                        let (fill, bord) = if set {
                                            (theme::GOOD_SOFT, theme::GOOD_DK)
                                        } else {
                                            (theme::G2, theme::LINE)
                                        };
                                        theme::row_chip(ui, fill, bord, |ui| {
                                            ui.label(
                                                egui::RichText::new(text)
                                                    .monospace()
                                                    .color(if set { theme::TX } else { theme::DIM }),
                                            );
                                        })
                                        .on_hover_text(hover);
                                    };
                                        match settings_section {
                                            SettingsSection::Game | SettingsSection::Data => {
                                                // ── Game archive / Reference data: the chain's rows, what it
                                                // opened, and the buttons that change a source. A change marks
                                                // the chains dirty; the top of the next frame re-resolves them
                                                // and reloads in place. ──
                                                use crate::settingspage::Act as SAct;
                                                let which = if settings_section == SettingsSection::Game { Which::Game } else { Which::Data };
                                                let failed = matches!(&boot_fail, Some((f, _)) if *f == which);
                                                let auto_patch_path = std::path::Path::new(&wadpath).with_file_name("vz-patch.wad");
                                                let sacts = {
                                                    let (chain, saved_set) = match which {
                                                        Which::Game => (&game_chain, cfg.wad_path.is_some()),
                                                        Which::Data => (&data_chain, cfg.data_dir.is_some()),
                                                    };
                                                    let (opened, opened_badge) = match which {
                                                        Which::Game if !wadpath.is_empty() => {
                                                            // The file names, as the board has them: the full
                                                            // path is the chain row above (and its hover).
                                                            let name = |p: &str| {
                                                                std::path::Path::new(p)
                                                                    .file_name()
                                                                    .map(|n| n.to_string_lossy().into_owned())
                                                                    .unwrap_or_else(|| p.to_string())
                                                            };
                                                            let mut rows: Vec<(&'static str, String, bool)> = vec![("base", name(&wadpath), false)];
                                                            if overlays.is_empty() {
                                                                rows.push(("overlay", "none".into(), false));
                                                            }
                                                            for o in &overlays {
                                                                let how = if std::path::Path::new(o) == auto_patch_path { "found beside it" } else { "--overlay" };
                                                                rows.push(("overlay", format!("{} · {how}", name(o)), false));
                                                            }
                                                            // The game's other WADs, read for Inspect beside the stack.
                                                            for (file, got) in &w.side_report {
                                                                rows.push(match got {
                                                                    SideWad::Opened(p) => ("also read", name(p), false),
                                                                    SideWad::Absent => ("also read", format!("{file} · not present"), false),
                                                                    SideWad::Unreadable(e) => ("also read", format!("{file} · unreadable: {e}"), true),
                                                                });
                                                            }
                                                            rows.push((
                                                                "catalog",
                                                                format!("{} models · {} textures", commafy(index.models().len()), commafy(index.textures().len())),
                                                                false,
                                                            ));
                                                            (rows, Some(format!("{} models", commafy(index.models().len()))))
                                                        }
                                                        Which::Data => match &loaded_data {
                                                            Some(h) => (
                                                                vec![
                                                                    ("bundle", h.display().to_string(), false),
                                                                    ("names", commafy(index.names.len()), false),
                                                                    ("Lua scripts", commafy(lua_corpus.len()), false),
                                                                ],
                                                                Some(format!("{} names", commafy(index.names.len()))),
                                                            ),
                                                            None => (Vec::new(), None),
                                                        },
                                                        Which::Game => (Vec::new(), None),
                                                    };
                                                    crate::settingspage::section(
                                                        ui,
                                                        &crate::settingspage::View {
                                                            which,
                                                            chain,
                                                            failed,
                                                            opened,
                                                            opened_badge,
                                                            saved_set,
                                                            downloading: fetch_rx.is_some().then_some(fetch_msg.as_str()),
                                                        },
                                                    )
                                                };
                                                let in_use = match which {
                                                    Which::Game => game_chain.in_use().map(|p| p.to_path_buf()),
                                                    Which::Data => data_chain.in_use().map(|p| p.to_path_buf()),
                                                };
                                                for a in sacts {
                                                    match a {
                                                        // Checked before it is kept, for the same reason the
                                                        // chain checks it: a remembered bad path fails at the
                                                        // NEXT launch, far from the click that caused it.
                                                        SAct::Choose(w) | SAct::RunOnly(w) => {
                                                            let picked = match w {
                                                                Which::Game => crate::settings::pick_wad(),
                                                                Which::Data => crate::settings::pick_data_dir(),
                                                            };
                                                            let Some(p) = picked else { continue };
                                                            let checked = match w {
                                                                Which::Game => crate::settings::check_wad(&p).map_err(|e| format!("Not a game archive: {e}")),
                                                                Which::Data => crate::settings::check_data_dir(&p),
                                                            };
                                                            cfg_note = Some(match checked {
                                                                Err(e) => Err(e),
                                                                Ok(desc) if matches!(a, SAct::RunOnly(_)) => {
                                                                    match w {
                                                                        Which::Game => crate::chains::set_run_wad(p.clone()),
                                                                        Which::Data => crate::chains::set_run_data(p.clone()),
                                                                    }
                                                                    chains_dirty = true;
                                                                    Ok(format!("Using {} for this run ({desc}); nothing saved.", p.display()))
                                                                }
                                                                Ok(desc) => {
                                                                    match w {
                                                                        Which::Game => cfg.wad_path = Some(p),
                                                                        Which::Data => cfg.data_dir = Some(p),
                                                                    }
                                                                    chains_dirty = true;
                                                                    match cfg.save() {
                                                                        Err(e) => Err(format!("Could not save: {e}")),
                                                                        Ok(f) => {
                                                                            // Saved is step 3: a broken step above it still
                                                                            // stops the chain, and "saved" alone would read
                                                                            // as fixed while the section stays red.
                                                                            let now = match w {
                                                                                Which::Game => crate::chains::game(opts.wad_flag.as_deref()),
                                                                                Which::Data => crate::chains::data(),
                                                                            };
                                                                            match now.blocker() {
                                                                                Some(b) => Err(format!(
                                                                                    "Saved to {}, but {} above it is still broken, so it is not used. Use \u{201c}Use for this run only\u{2026}\u{201d}, or fix {}.",
                                                                                    f.display(),
                                                                                    b.label,
                                                                                    b.label
                                                                                )),
                                                                                None => Ok(format!("Saved to {} ({desc}).", f.display())),
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            });
                                                        }
                                                        SAct::ClearSaved(_) | SAct::SaveRunPath(_) => {
                                                            if crate::settingspage::edit_saved(&mut cfg, a, in_use.as_deref()) {
                                                                chains_dirty = true;
                                                                cfg_note = Some(
                                                                    cfg.save()
                                                                        .map(|f| format!("Saved to {}.", f.display()))
                                                                        .map_err(|e| format!("Could not save: {e}")),
                                                                );
                                                            }
                                                        }
                                                        // Fetch the pinned release's bundle into the download
                                                        // cache (`fetch::installed_dir`), replacing the copy
                                                        // there. The boot thread runs the same install when
                                                        // nothing is set and nothing is found.
                                                        SAct::Download => {
                                                            let (tx, rx) = std::sync::mpsc::channel();
                                                            fetch_rx = Some(rx);
                                                            fetch_msg = "starting…".into();
                                                            cfg_note = None;
                                                            std::thread::spawn(move || {
                                                                crate::fetch::download(|e| {
                                                                    let _ = tx.send(e);
                                                                });
                                                            });
                                                        }
                                                        SAct::Reveal(w) => {
                                                            let dir = match (w, in_use.as_deref()) {
                                                                (Which::Game, Some(p)) => p.parent().map(|d| d.to_path_buf()),
                                                                (Which::Data, Some(p)) => Some(p.to_path_buf()),
                                                                (_, None) => None,
                                                            };
                                                            if let Some(d) = dir {
                                                                if let Err(e) = crate::quartermaster::open_in_os(&d) {
                                                                    cfg_note = Some(Err(format!("Could not open {}: {e}", d.display())));
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            SettingsSection::Shipments => {
                                                // Where a built Shipment is handed to Modkit. Deliberately NOT
                                                // configurable: a folder both apps agree on IS the integration, and a
                                                // per-app override is how two tools end up disagreeing about where the
                                                // work went.
                                                theme::section(ui, "Shipments library", None, true, |ui| {
                                                    match crate::quartermaster::shipments_library() {
                                                        Some(p) => {
                                                            let exists = p.is_dir();
                                                            path_chip(
                                                                ui,
                                                                exists,
                                                                &tail(&p),
                                                                &format!(
                                                                    "{}\n\n{}",
                                                                    p.display(),
                                                                    if exists {
                                                                        "Build, then Send to Modkit, drops a copy here."
                                                                    } else {
                                                                        "Created on the first Send to Modkit."
                                                                    }
                                                                ),
                                                            );
                                                            if ui.add_enabled(exists, egui::Button::new("Reveal")).clicked() {
                                                                let _ = crate::quartermaster::open_in_os(&p);
                                                            }
                                                        }
                                                        None => {
                                                            ui.label(
                                                                egui::RichText::new(
                                                                    "No home directory to place the library in.",
                                                                ).small()
                                                                .color(theme::BAD),
                                                            );
                                                        }
                                                    }
                                                });
                                            }
                                            SettingsSection::Shortcuts => {
                                                // Written down because they were otherwise discoverable only by pressing
                                                // keys: the verb bar names a few, and the rest live in a `match code`
                                                // nobody outside this file reads.
                                                theme::section(ui, "Keys", None, true, |ui| {
                                                    for (k, what) in SHORTCUTS {
                                                        theme::kv(ui, what, egui::RichText::new(*k));
                                                    }
                                                });
                                            }
                                            SettingsSection::About => {
                                                theme::section(ui, "Build", None, true, |ui| {
                                                    theme::kv(ui, "workshop", egui::RichText::new(env!("CARGO_PKG_VERSION")));
                                                });
                                                // Plan 02: "Show the resolved stack, do not just resolve it." Which
                                                // install was actually read sits behind a large share of this project own
                                                // trap reports, and it was visible only on the Shipment page.
                                                theme::section(
                                                    ui,
                                                    "Resolved stack",
                                                    Some(&format!("{} wad(s)", qm_stack.len())),
                                                    true,
                                                    |ui| {
                                                        if qm_stack.is_empty() {
                                                            ui.label(
                                                                egui::RichText::new("No game configured.").small()
                                                                    .color(theme::BAD),
                                                            );
                                                        }
                                                        for (i, w) in qm_stack.iter().enumerate() {
                                                            theme::kv(
                                                                ui,
                                                                if i == 0 { "base" } else { "overlay" },
                                                                egui::RichText::new(w.as_str()),
                                                            );
                                                        }
                                                    },
                                                );
                                                theme::section(
                                                    ui,
                                                    "Reference data",
                                                    Some(&format!("{} names", index.names.len())),
                                                    true,
                                                    |ui| {
                                                        theme::kv(
                                                            ui,
                                                            "bundle",
                                                            egui::RichText::new(
                                                                crate::index::data_home()
                                                                    .map(|h| h.display().to_string())
                                                                    .unwrap_or_else(|e| e),
                                                            ),
                                                        );
                                                        theme::kv(ui, "models", egui::RichText::new(commafy(index.models().len())));
                                                        theme::kv(ui, "textures", egui::RichText::new(commafy(index.textures().len())));
                                                    },
                                                );
                                                theme::section(ui, "Config file", None, true, |ui| {
                                                    theme::kv(
                                                        ui,
                                                        "settings.json",
                                                        egui::RichText::new(
                                                            crate::settings::config_path()
                                                                .map(|p| p.display().to_string())
                                                                .unwrap_or_else(|| "(no config directory)".into()),
                                                        ),
                                                    );
                                                });
                                            }
                                        }
                                    // Outcome of the last action — one line, only when there IS one.
                                    if let Some(note) = &cfg_note {
                                        let (col, text) = match note {
                                            Ok(m) => (theme::GOOD, m.as_str()),
                                            Err(m) => (theme::BAD, m.as_str()),
                                        };
                                        theme::status_dot(ui, text, col);
                                    }
                                    });
                                });
                        }

                        // ── QUARTERMASTER main content: the selected contribution, in full. A
                        // CentralPanel rather than the 3D viewport, because a Shipment is read, not
                        // orbited. Added last so the side panels have already claimed their space.
                        if wb == Workbench::Quartermaster {
                            qm_acts.extend(crate::quartermaster::center(ctx, &qm, qm_names.as_ref()));
                        }
                        // Saves owns its own centre too: a composed card (hero preview + at-a-glance
                        // HUD + progression grid), not the 3D viewport (see `has_viewport`).
                        if wb == Workbench::Saves {
                            saves_acts.extend(crate::saves::center(ctx, &saves));
                            // Floating windows: the Inspect view and the delete confirmation.
                            saves_acts.extend(crate::saves::windows(ctx, &saves));
                        }

                        // ── MISSIONS main content: the Blueprint-style node canvas. Takes the
                        // full centre, not a viewport pass, so the dot-grid and node cards sit
                        // where the 3D would.
                        if wb == Workbench::Domain(crate::domain::Domain::Missions) {
                            missions_acts.extend(crate::missions::center(ctx, &missions_panel));
                        }
                        // ── AUDIO main content: the clip table, not the 3D viewport.
                        let audio_tab = wb == Workbench::Inspect && kind == Kind::Audio;
                        if audio_tab {
                            let playing = vo_player.as_ref().and_then(|p| p.status());
                            audio_acts.extend(crate::audiopage::center(ctx, &audio_page, playing));
                        }
                        // ── TEXT main content: an open string table, every language side by side, or
                        // an open font's pages.
                        let text_tab = wb == Workbench::Inspect
                            && kind == Kind::Text
                            && (text_page.table.is_some() || text_page.font.is_some());
                        if text_tab {
                            if text_page.font.is_some() {
                                text_acts.extend(crate::textpage::font_center(ctx, &mut text_page));
                            } else {
                                text_acts.extend(crate::textpage::center(ctx, &text_page));
                            }
                        }

                        // ── VIEWPORT HUD: status chips over the 3D (the panels are all placed now, so
                        // `available_rect` is the viewport region). Non-interactable so the camera drag
                        // works underneath. ──
                        //
                        // Suppressed on every page that does NOT own the viewport — there is no 3D
                        // under those, so the chips would sit on top of the page's own content
                        // rather than over a scene. Asked as a question about the workbench rather
                        // than as `!= Quartermaster`, which silently stopped being the whole answer
                        // the moment Settings also took the centre.
                        let vp = ctx.available_rect();
                        if eff.has_viewport() && wb != Workbench::Settings && !audio_tab && !text_tab {
                        egui::Area::new(egui::Id::new("vp_hud"))
                            .fixed_pos(vp.left_top() + egui::vec2(14.0, 12.0))
                            .interactable(false)
                            .show(ctx, |ui| {
                                ui.horizontal(|ui| {
                                    theme::chip(ui, "Orbit", true, Some(theme::BRASS));
                                    if let Some(p) = &preview {
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
                        let hp_legend = eff == Workbench::Mods && show_hardpoints;
                        let node_legend = eff == Workbench::Mods && show_nodes;
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
                                                        egui::RichText::new(t).small()
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

                        // ── The hero picker: every model, those whose skeleton holds the most of the
                        // bones it is chosen for first.
                        if let Some(hp) = &mut hero_picker {
                            let mut open = true;
                            egui::Window::new("Choose the hero")
                                .id(egui::Id::new("hero_picker"))
                                .open(&mut open)
                                .default_size([480.0, 540.0])
                                .resizable(true)
                                .show(ctx, |ui| {
                                    ui.label(egui::RichText::new(format!("The model {} plays on, kept across tabs and launches.", hp.what)).color(theme::DIM));
                                    if !hp.want.is_empty() {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "Models whose skeleton holds the most of its {} bones come first; one holding none cannot be chosen.",
                                                hp.want.len()
                                            ))
                                            .small()
                                            .color(theme::DIM),
                                        );
                                    }
                                    ui.add(egui::TextEdit::singleline(&mut hp.filter).hint_text("filter models").desired_width(f32::INFINITY));
                                    ui.add_space(4.0);
                                    if !hp.want.is_empty() && skeletons.is_none() {
                                        match &skel_error {
                                            Some(e) => {
                                                ui.label(egui::RichText::new(format!("The skeletons did not read: {e}")).color(theme::BAD));
                                            }
                                            None => {
                                                ui.horizontal(|ui| {
                                                    ui.add(egui::Spinner::new());
                                                    ui.label(
                                                        egui::RichText::new(format!(
                                                            "Reading every model's skeleton\u{2026} {:.0}% {}",
                                                            skel_progress.0 * 100.0,
                                                            skel_progress.1
                                                        ))
                                                        .color(theme::FAINT),
                                                    );
                                                });
                                            }
                                        }
                                        return;
                                    }
                                    let needle = hp.filter.trim().to_lowercase();
                                    let models = index.models();
                                    let mut rows: Vec<(usize, Option<usize>)> = models
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, r)| needle.is_empty() || r.label().to_lowercase().contains(&needle))
                                        .map(|(i, r)| (i, skeletons.as_ref().filter(|_| !hp.want.is_empty()).and_then(|s| s.coverage(r.hash, &hp.want))))
                                        .collect();
                                    rows.sort_by(|a, b| {
                                        b.1.unwrap_or(0)
                                            .cmp(&a.1.unwrap_or(0))
                                            .then_with(|| models[a.0].label().to_lowercase().cmp(&models[b.0].label().to_lowercase()))
                                    });
                                    let row_h = 20.0;
                                    egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
                                        for &(mi, cov) in &rows[range] {
                                            let r = &models[mi];
                                            let fits = hp.want.is_empty() || cov.unwrap_or(0) > 0;
                                            let on = hero.as_ref().is_some_and(|h| h.hash == r.hash && h.label == r.label());
                                            let text = match cov {
                                                Some(n) if !hp.want.is_empty() => format!("{}   {n}/{} bones", r.label(), hp.want.len()),
                                                _ => r.label(),
                                            };
                                            let col = if fits { theme::TX } else { theme::FAINT };
                                            if ui.selectable_label(on, egui::RichText::new(text).color(col)).clicked() {
                                                actions.push(Act::SetHero(r.hash, r.label()));
                                            }
                                        }
                                    });
                                });
                            if !open {
                                actions.push(Act::HeroClose);
                            }
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
                                    // A texture more than one WAD holds: every copy, the one shown
                                    // marked, a copy whose bytes differ from the first said so.
                                    if tv.copies.len() > 1 {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(egui::RichText::new("copies").color(theme::FAINT));
                                            for &(s, differs) in &tv.copies {
                                                let file = std::path::Path::new(w.source_path(s))
                                                    .file_name()
                                                    .map_or_else(|| format!("source {s}"), |f| f.to_string_lossy().into_owned());
                                                let text = if differs { format!("{file} \u{b7} differs") } else { file };
                                                if ui.selectable_label(tv.src == Some(s), text).clicked() {
                                                    actions.push(Act::TexSource(s));
                                                }
                                            }
                                        });
                                    }
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
                        // ── LOG: the session's status history. Virtualised the same way the Lua
                        // viewer is, because a long session reaches the 2000-line cap and drawing
                        // every row each frame would be paid on every frame the window is open.
                        if log_open {
                            let mut open = true;
                            egui::Window::new("Log")
                                .id(egui::Id::new("log_view"))
                                .open(&mut open)
                                .default_size([720.0, 420.0])
                                .resizable(true)
                                .show(ctx, |ui| {
                                    let errs = log.iter().filter(|l| l.level == LogLevel::Error).count();
                                    let warns = log.iter().filter(|l| l.level == LogLevel::Warn).count();
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{} lines", log.len()))
                                                .color(theme::DIM).small(),
                                        );
                                        if errs > 0 {
                                            theme::status_dot(ui, &format!("{errs} failed"), theme::BAD);
                                        }
                                        if warns > 0 {
                                            theme::status_dot(ui, &format!("{warns} warned"), theme::BRASS);
                                        }
                                        ui.separator();
                                        // Only Info is hideable. A filter that can suppress a
                                        // failure turns the log into a source of false confidence.
                                        if theme::pill(ui, "info", log_show_info).clicked() {
                                            log_show_info = !log_show_info;
                                        }
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui.button("Copy all").clicked() {
                                                    let all: String = log
                                                        .iter()
                                                        .map(|l| {
                                                            format!(
                                                                "[{:>8.2}] {} {}\n",
                                                                l.t,
                                                                l.level.tag(),
                                                                l.text
                                                            )
                                                        })
                                                        .collect();
                                                    ui.ctx().copy_text(all);
                                                }
                                            },
                                        );
                                    });
                                    ui.separator();
                                    let rows: Vec<&LogLine> = log
                                        .iter()
                                        .filter(|l| log_show_info || l.level != LogLevel::Info)
                                        .collect();
                                    if rows.is_empty() {
                                        ui.add_space(12.0);
                                        ui.vertical_centered(|ui| {
                                            ui.label(
                                                egui::RichText::new("Nothing logged yet.")
                                                    .color(theme::FAINT),
                                            );
                                        });
                                        return;
                                    }
                                    let h = ui.text_style_height(&egui::TextStyle::Monospace);
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        // Newest line is the interesting one, so open at the end.
                                        .stick_to_bottom(true)
                                        .show_rows(ui, h, rows.len(), |ui, range| {
                                            for i in range {
                                                let l = rows[i];
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!("{:>8.2}", l.t))
                                                            .monospace()
                                                            .color(theme::FAINT),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(l.level.tag())
                                                            .monospace()
                                                            .color(l.level.colour()),
                                                    );
                                                    ui.label(
                                                        egui::RichText::new(&l.text)
                                                            .monospace()
                                                            .color(if l.level == LogLevel::Info {
                                                                theme::TX
                                                            } else {
                                                                l.level.colour()
                                                            }),
                                                    );
                                                });
                                            }
                                        });
                                });
                            log_open = open;
                        }
                        }
                        // The Lua viewer is a floating window and must render on EVERY page — gating
                        // it behind the viewport (above) meant a script opened from Missions / Systems
                        // never appeared. `ctx` is captured from `gui.run`, so it is in scope here.
                        if let Some(v) = &mut lua_view {
                            v.show(ctx);
                        }
                        // Captured here (where `ctx` is in scope) so the reducer below `gui.run`
                        // can hand it to the hero-render helper — the render needs the egui
                        // context to allocate a texture handle, and the reducer owns the
                        // `WadStack` + Panel the render reads from.
                        pending_ctx = Some(ctx.clone());
                    });

                    for act in std::mem::take(&mut qm_acts) {
                        // Switching pages is the app's business, not the panel's — it owns `wb`.
                        // Entering a craft bench FROM a contribution. It now carries the index, so
                        // the bench knows what it is editing and a commit can land back on that
                        // contribution instead of appending a new one. Previously this was a
                        // one-way, index-less page flip — the bench never learned its subject and
                        // there was no route back, which is how a "dissolved" page became a trap.
                        if let crate::quartermaster::Act::Craft(c, at) = act {
                            let bench = bench_page(c);
                            craft_subject = Some(at);

                            // Load the contribution's OWN model and donor into the bench, so it
                            // opens on what you clicked instead of blank. Carrying only the index
                            // told the bench WHICH contribution a commit writes back to; it never
                            // read that contribution to pull its bytes IN. That missing half is why
                            // "Edit rig" / "Conform" landed on an empty page — the bench knew its
                            // subject's identity but had none of its content.
                            match qm.craft_subject(at) {
                                Some(subject) if subject.model.is_file() => {
                                    // The import becomes the pedestal (Conform) / retarget source
                                    // (Rig). `import_file` flips the workbench to Skeleton for a
                                    // rigged model; we re-assert the bench below, after the load.
                                    let s = import_file(
                                        &subject.model, &mut w, &mut scene, &mut world,
                                        &mut imported, &mut preview, &mut cam_target, &mut cam_dist,
                                        &mut retarget, &retarget_target, &mut retarget_src_path,
                                        &mut wb, &mut sel_bone, donor_ref.map(|(h, _)| h), &index, &anim_sel,
                                        &lua_corpus,
                                    );
                                    // The donor is the retarget TARGET (Rig) or the conform HOST
                                    // (Conform), resolved from the manifest name in `craft_subject`.
                                    let donor_note = match (&subject.donor, c) {
                                        (Some((dh, dl)), crate::quartermaster::Craft::Rig) => {
                                            actions.push(Act::RetargetSetTarget(*dh, dl.clone()));
                                            format!("donor {dl}")
                                        }
                                        (Some((dh, dl)), crate::quartermaster::Craft::Conform) => {
                                            mod_donor = Some((*dh, dl.clone()));
                                            format!("donor {dl}")
                                        }
                                        _ => "no donor (auto-pick at build)".into(),
                                    };
                                    // The saved bone map's hand edits are not re-applied yet — the
                                    // import re-derives the auto map — so say the map exists rather
                                    // than imply it was restored.
                                    let map_note = subject
                                        .from
                                        .as_ref()
                                        .map(|f| format!(" · saved map from {f} (re-derived)"))
                                        .unwrap_or_default();
                                    status = format!(
                                        "{} — contributions[{at}]: {s} · {donor_note}{map_note}",
                                        bench.verb()
                                    );
                                }
                                Some(subject) => {
                                    status = format!(
                                        "{} — contributions[{at}]: source model missing at {}",
                                        bench.verb(),
                                        subject.model.display()
                                    );
                                }
                                None => {
                                    status = format!(
                                        "{} — contributions[{at}] carries no model to load",
                                        bench.verb()
                                    );
                                }
                            }
                            // The bench asked for, regardless of what `import_file` set the workbench to.
                            wb = bench;
                            continue;
                        }
                        crate::quartermaster::apply(
                            act,
                            &mut qm,
                            &qm_stack,
                            qm_names.as_ref(),
                            qm_corpus.as_deref(),
                            &mut status,
                        );
                    }
                    // Fold the Missions editor's queued actions back into its panel. The panel
                    // needs the lua corpus so Act::PickMission can parse the picked script in
                    // place; we pass the same slice the rest of the app reads.
                    for act in missions_acts.drain(..) {
                        crate::missions::apply(act, &mut missions_panel, &lua_corpus);
                    }
                    // The Audio page: its acts, its workers, and what it asks of the host.
                    for act in audio_acts.drain(..) {
                        crate::audiopage::apply(act, &mut audio_page);
                    }
                    audio_page.poll();
                    for act in text_acts.drain(..) {
                        match act {
                            crate::textpage::Act::Ship(hash, label) => {
                                actions.push(Act::AddToShipment("edit_stringdb", hash, label));
                            }
                            a => crate::textpage::apply(a, &mut text_page),
                        }
                    }
                    text_page.poll();
                    // Every model's skeleton, for ranking hero candidates.
                    let mut skel_done = false;
                    if let Some(rx) = &skel_rx {
                        loop {
                            match rx.try_recv() {
                                Ok(crate::heroidx::Msg::Progress(f, wad)) => skel_progress = (f, wad),
                                Ok(crate::heroidx::Msg::Done(Ok(s))) => {
                                    skeletons = Some(s);
                                    skel_done = true;
                                    break;
                                }
                                Ok(crate::heroidx::Msg::Done(Err(e))) => {
                                    skel_error = Some(e);
                                    skel_done = true;
                                    break;
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    skel_error = Some("the skeleton reader stopped".into());
                                    skel_done = true;
                                    break;
                                }
                            }
                        }
                    }
                    if skel_done {
                        skel_rx = None;
                    }
                    if let Some(wave) = audio_page.play.take() {
                        // Lazily open the record index (English.wad + .pws) and the device on the
                        // first play — both are heavy and most sessions never play audio.
                        if vo_error.is_none() && vo_stream.is_none() {
                            let data_dir = std::path::Path::new(&wadpath)
                                .parent()
                                .map(|p| p.to_path_buf())
                                .unwrap_or_default();
                            match crate::vostream::VoStream::open(&data_dir) {
                                Ok(s) => vo_stream = Some(s),
                                Err(e) => vo_error = Some(e),
                            }
                        }
                        if vo_error.is_none() && vo_player.is_none() {
                            match crate::audioplay::Player::start() {
                                Some(p) => vo_player = Some(p),
                                None => vo_error = Some("no audio output device".into()),
                            }
                        }
                        match (&vo_stream, &vo_player, &vo_error) {
                            (_, _, Some(e)) => status = format!("AUDIO: {e}"),
                            (Some(vo), Some(player), None) => match player.status() {
                                // The clip already playing pauses, and a paused one resumes.
                                Some(s) if s.clip == wave && !s.paused => player.pause(),
                                Some(s) if s.clip == wave => player.resume(),
                                _ => match vo.clip_pcm(wave) {
                                    Ok(pcm) => player.play(wave, pcm.samples, pcm.rate, pcm.channels),
                                    Err(e) => status = format!("AUDIO: {e}"),
                                },
                            },
                            _ => {}
                        }
                    }
                    // Add to Shipment: into the open Shipment (one is scaffolded if none is open);
                    // a line, cue or bank it already overrides is updated in place. Then its page.
                    if let Some(c) = audio_page.shipment.take() {
                        use mercs2_quartermaster::manifest::{asset_hash, Contribution as C};
                        let kind = c.kind();
                        let out = (|| -> Result<String, String> {
                            let dir = qm.ensure_shipment(qm_names.as_ref())?;
                            let existing = qm.find_contribution(|o| match (&c, o) {
                                (C::ReplaceSoundWave { line: a, .. }, C::ReplaceSoundWave { line: b, .. }) => asset_hash(a) == asset_hash(b),
                                (C::ReplaceSoundCue { cue: a, .. }, C::ReplaceSoundCue { cue: b, .. }) => asset_hash(&a.name) == asset_hash(&b.name),
                                (C::ReplaceSoundBank { bank: a, .. }, C::ReplaceSoundBank { bank: b, .. }) => asset_hash(a) == asset_hash(b),
                                _ => false,
                            });
                            let at = qm.upsert_contribution(qm_names.as_ref(), existing, c)?;
                            Ok(format!("contributions[{at}] {kind} — {}", dir.display()))
                        })();
                        match out {
                            Ok(m) => {
                                wb = Workbench::Quartermaster;
                                status = m;
                            }
                            Err(e) => status = format!("ADD TO SHIPMENT FAILED: {e}"),
                        }
                    }
                    if std::mem::take(&mut audio_page.pick_folder) {
                        if let Some(dir) = rfd::FileDialog::new()
                            .set_title("Export voice-over clips into…")
                            .set_directory(&audio_page.folder)
                            .pick_folder()
                        {
                            audio_page.folder = dir;
                        }
                    }
                    // Drain any outer intents the Missions panel asked the host to carry out.
                    // `pending_lua_open` is the Blueprint jump-to-source → luaview bridge.
                    if let Some((path, line)) = missions_panel.pending_lua_open.take() {
                        actions.push(Act::LuaOpenAt(path, line));
                    }
                    // Add to Shipment: the compiled script goes into the open Shipment's src/ (one
                    // is scaffolded if none is open) and a replace_lua contribution names it —
                    // updated in place if this script already has one.
                    if let Some(req) = missions_panel.pending_shipment.take() {
                        let out = (|| -> Result<String, String> {
                            let dir = qm.ensure_shipment(qm_names.as_ref())?;
                            let rel = std::path::Path::new("src").join(format!("{}.lua", req.target));
                            let file = dir.join(&rel);
                            std::fs::create_dir_all(dir.join("src")).map_err(|e| format!("{}: {e}", dir.display()))?;
                            std::fs::write(&file, &req.source).map_err(|e| format!("{}: {e}", file.display()))?;
                            let existing = qm.find_contribution(|c| {
                                matches!(c, mercs2_quartermaster::manifest::Contribution::ReplaceLua { target, .. } if *target == req.target)
                            });
                            let c = mercs2_quartermaster::manifest::Contribution::ReplaceLua { target: req.target.clone(), source: rel };
                            let at = qm.upsert_contribution(qm_names.as_ref(), existing, c)?;
                            Ok(format!("contributions[{at}] replace_lua {} — {}", req.target, file.display()))
                        })();
                        status = match out {
                            Ok(m) => m,
                            Err(e) => format!("ADD TO SHIPMENT FAILED: {e}"),
                        };
                    }
                    // The Missions panel's "Load .profile…" verb asks the host for a native file
                    // picker. We do it inline here so the modal lives on the UI thread.
                    if std::mem::take(&mut missions_panel.pending_save_open) {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("profile", &["profile"])
                            .set_title("Load Mercenaries 2 save (.profile)")
                            .set_directory(
                                mercs2_formats::game_paths::saves_dir_from_env()
                                    .unwrap_or_else(|| std::path::PathBuf::from(".")),
                            )
                            .pick_file()
                        {
                            if let Err(e) = missions_panel.load_save(&path) {
                                status = format!("missions: {e}");
                            }
                        }
                    }
                    // A background build (Act::Build) reports back here. While it runs, the verb
                    // bar's spinner keeps egui repainting, so this polls every frame until it lands;
                    // the frame it completes, sync the app's status line to the panel's.
                    if qm.poll_build() {
                        status = qm.status().to_string();
                    }
                    // Saves follows the same drain-and-apply pattern: the panels emitted intents
                    // this frame, and the reducer lands them on the owned Panel so the next frame
                    // reads from the mutated state. Switching to another workbench never flushes
                    // `saves`; it is a long-lived piece of app state, not a per-frame scratchpad.
                    for act in std::mem::take(&mut saves_acts) {
                        crate::saves::apply(act, &mut saves);
                    }
                    // ── Saves hero render: the Panel's own reducer only stamps a request slot;
                    // the real `load_model_data` + `shot::HeroRenderer::render_rgba` +
                    // `TextureHandle::set` runs via the egui-context stashed by `gui.run` on this
                    // frame. First render per hash pays the ~500 ms pipeline + mesh upload; every
                    // subsequent render (drag-orbit tick) is a camera-uniform write + draw +
                    // ~2.6 MB readback into a reused TextureHandle, usually under ~30 ms.
                    // English string table for display names (loadout tokens). Loaded once, the
                    // first frame the Saves page has a save list.
                    if saves.profiles.is_some() && saves.english.is_none() {
                        saves.english = Some(w.string_table("english"));
                    }
                    // Template names for the save's carried weapons (worldentity `Name` records).
                    if saves.profiles.is_some() && saves.templates.is_none() {
                        saves.templates = Some(w.template_names());
                    }
                    // The mission, reward and flow scripts the contract menu runs.
                    if saves.profiles.is_some() && saves.rules.is_none() {
                        saves.rules = Some(crate::game_rules::GameRules::load(&mut w));
                    }
                    if let (Some((hash, name)), Some(ctx)) =
                        (saves.pending_hero_render.take(), pending_ctx.as_ref())
                    {
                        // Camera read lazily: hero_cam gets populated by SetHeroCam on drag; a
                        // cold-start (no drag yet) falls through to OrbitCam::default().
                        let cam = saves
                            .hero_cam
                            .get(&hash)
                            .copied()
                            .unwrap_or_default();
                        match ensure_hero_render(
                            &mut w,
                            hash,
                            &name,
                            &cam,
                            saves.hero_view_px,
                            &mut hero_renderers,
                            saves.hero_textures.get(&hash).cloned(),
                            ctx,
                        ) {
                            Ok(tex) => {
                                saves.hero_textures.insert(hash, tex);
                            }
                            Err(e) => {
                                saves.hero_render_failed.insert(hash, e);
                            }
                        }
                    }
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
                                            &preview, donor_ref.map(|(h, _)| h), &index, &anim_sel, &lua_corpus,
                                        );
                                        cam_target = p.center;
                                        cam_dist = (p.radius * 2.4).clamp(1.5, 15000.0);
                                        sel_bone = None;
                                        preview = Some(p);
                                        // The loaded row is the cursor row, however it was loaded
                                        // (a click, the menu, Enter), when the list shows it.
                                        let rows = index.rows(kind);
                                        if let Some(vi) = filtered
                                            .iter()
                                            .position(|&ri| rows[ri].hash == hash && rows[ri].label() == label)
                                        {
                                            sel = vi;
                                        }
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
                                    let (names, pos, parents) = target_bone_info(&mut w, hash, &index.names);
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
                                    let (names, pos, parents) = target_bone_info(&mut w, *hash, &index.names);
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
                                        let mut paths = vec![wadpath.clone()];
                                        paths.extend(overlays.iter().cloned());
                                        let (cs, glb, _donor) =
                                            faithful_char_skin(&paths, thash, rt, &src_path)?;
                                        let target_rig = load_model_data(&mut w, thash)
                                            .map(|m| m.skin.rig)
                                            .unwrap_or_default();
                                        if target_rig.is_empty() {
                                            return Err(format!("target {tl} carries no skeleton"));
                                        }
                                        // Texture the conformed mesh from the SOURCE file's MATERIALS,
                                        // keyed by the material index each `CharGlbData` part already
                                        // carries. This used to re-import the file and copy its draws
                                        // across only if the two vertex streams matched byte for byte —
                                        // which a multi-mesh character never does (the two importers
                                        // merge in different orders), so every one of them previewed
                                        // bald. The part partition needs no such matching.
                                        let mats = match crate::import::load_material_textures(&src_path) {
                                            Ok(m) => m,
                                            Err(e) => {
                                                eprintln!(
                                                    "[skeleton] source materials unreadable ({e}); previewing untextured"
                                                );
                                                Default::default()
                                            }
                                        };
                                        let md: ModelData =
                                            crate::import::char_skin_to_imported(&cs, &glb, target_rig, Some(&mats))
                                                .into();
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
                                            donor_ref.map(|(h, _)| h), &index, &anim_sel, &lua_corpus,
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
                                    let mut paths = vec![wadpath.clone()];
                                    paths.extend(overlays.iter().cloned());
                                    let (glb, donor, opts_lower) =
                                        faithful_lower_opts(&paths, thash, rt, &src_path)?;
                                    let Some(out_path) = rfd::FileDialog::new()
                                        .add_filter("model block", &["bin"])
                                        .set_title("Export faithful character block")
                                        .set_file_name(format!("{tlabel}_faithful.bin"))
                                        .save_file()
                                    else {
                                        return Ok("export cancelled".into());
                                    };
                                    let new_name = mercs2_formats::hash::pandemic_hash_m2(&format!(
                                        "{tlabel}_faithful"
                                    ));
                                    // ONE lowering, shared with the Quartermaster. This arm used to
                                    // hand-roll its own: single-host-or-error, `&[]` repoints, its
                                    // own ExternalMesh construction byte-for-byte identical to
                                    // char_lower's. It could not export what the Quartermaster
                                    // ships — this very fixture needs five host groups — so
                                    // "preview == shipped" was a claim with two code paths behind
                                    // it. Now there is one.
                                    let low = mercs2_formats::char_lower::character_into_donor(
                                        &donor, &glb, new_name, &opts_lower,
                                    )?;
                                    std::fs::write(&out_path, &low.block)
                                        .map_err(|e| format!("write {}: {e}", out_path.display()))?;
                                    for w in &low.warnings {
                                        eprintln!("faithful export: WARNING - {w}");
                                    }
                                    let checks = low
                                        .report
                                        .checks
                                        .iter()
                                        .map(|c| format!("{}={:?}", c.title, c.status))
                                        .collect::<Vec<_>>()
                                        .join(" ");
                                    Ok(format!(
                                        "EXPORTED {tlabel} - {} verts, palette {}/{} runs, hosts {:?}, {:?} [{checks}] -> {}",
                                        low.skin.stats.verts,
                                        low.skin.palette_slots,
                                        low.skin.stats.range_count,
                                        low.hosts,
                                        low.skin.mode,
                                        out_path.display()
                                    ))
                                })();
                                status = match out {
                                    Ok(s) => s,
                                    Err(e) => format!("EXPORT FAILED: {e}"),
                                };
                            }
                            Act::ExportShipment => {
                                // Driven by the LIVE retarget, which is the whole point: the
                                // convention table is derivable from the source file (the
                                // Quartermaster derives it itself when no map is supplied), so a
                                // map carrying only that adds nothing. What a rebuild cannot
                                // recover is the author moving a bone here — that decision exists
                                // nowhere but in this UI.
                                let out = (|| -> Result<String, String> {
                                    let src_path = retarget_src_path
                                        .clone()
                                        .ok_or("import a rigged .glb first")?;
                                    let (_thash, tlabel) = retarget_target
                                        .clone()
                                        .ok_or("pick a target skeleton first")?;
                                    let rt = retarget.as_ref().ok_or("pick a target skeleton first")?;
                                    // ADD to the open Shipment, scaffolding one only if there is
                                    // none. This used to unconditionally pick a folder and write a
                                    // whole new single-contribution manifest over it, which meant a
                                    // retarget could never join a Shipment already being assembled
                                    // — it replaced it, and re-pointed the dock at the replacement.
                                    let dir = qm.ensure_shipment(qm_names.as_ref())?;
                                    // Default the asset name off the Shipment folder, so the common
                                    // case needs no extra prompt; it is a plain YAML field the
                                    // contribution form can edit afterwards.
                                    let asset = dir
                                        .file_name()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "my_outfit".into());
                                    // The bytes have to travel with the manifest or the Shipment
                                    // stops building the moment it changes machines.
                                    let model =
                                        crate::quartermaster::Panel::import_source(&dir, &src_path)?;
                                    let (c, st) = crate::shipment::outfit_contribution(
                                        &asset, "mattias", &tlabel, model, rt,
                                    )?;
                                    // Replace the contribution this bench was ENTERED from, if it
                                    // was entered from one; otherwise append. Always appending is
                                    // what made "edit the rig" produce a second copy of the outfit
                                    // rather than updating the one being worked on.
                                    let at =
                                        qm.upsert_contribution(qm_names.as_ref(), craft_subject, c)?;
                                    // Terse state, the way the status line reads elsewhere. The
                                    // hand-adjusted count leads because it is the reason the map
                                    // is worth writing: the convention table is derivable, that is
                                    // not.
                                    Ok(format!(
                                        "SHIPMENT contributions[{at}] {asset} · {} rows · {} hand-adjusted{} → {}",
                                        st.rows,
                                        st.manual,
                                        if st.unnamed > 0 {
                                            format!(" · {} unnamed → automap", st.unnamed)
                                        } else {
                                            String::new()
                                        },
                                        dir.display()
                                    ))
                                })();
                                status = match out {
                                    Ok(s) => s,
                                    Err(e) => format!("SHIPMENT EXPORT FAILED: {e}"),
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
                                                    row.label(), md, &preview, donor_ref.map(|(h, _)| h), &index,
                                                    &anim_sel, &lua_corpus,
                                                );
                                                cam_target = p.center;
                                                cam_dist = (p.radius * 2.4).clamp(1.5, 15000.0);
                                                status = format!(
                                                    "loaded {} [blk {}{}] in {:.2}s",
                                                    p.label,
                                                    row.block,
                                                    w.tag(row.src),
                                                    t0.elapsed().as_secs_f32()
                                                );
                                                sel_bone = None; // bone indices belong to the old rig
                                                preview = Some(p);
                                            }
                                            Err(e) => status = format!("LOAD FAILED: {e}"),
                                        }
                                    }
                                    Kind::Texture => {
                                        let copies = texture_copies(&mut w, row.hash, row.src, &row.others);
                                        let mut tv = TexView {
                                            copies,
                                            src: Some(row.src),
                                            ..TexView::of_list(vec![row.hash], vec![row.label()])
                                        };
                                        bind_tex_plate(&mut w, &mut scene, &mut tv);
                                        tex_view = Some(tv);
                                    }
                                    // A clip: played on the hero, or the hero picker first when there is
                                    // none, ranked by the clip's bones.
                                    Kind::Animation => {
                                        let (clip, label) = (row.hash, row.label());
                                        let info = clip_infos.entry(clip).or_insert_with(|| clip_info(&mut w, clip)).clone();
                                        anim_clip = Some((clip, label.clone()));
                                        tex_view = None;
                                        manm_t = 0.0;
                                        match (&hero, info) {
                                            (_, Err(e)) => status = format!("{label}: {e}"),
                                            // A material animation drives no bone: its tracks show
                                            // in the inspector, nothing loads on the hero.
                                            (_, Ok(ClipInfo { material: Some(m), .. })) => {
                                                manm_t = m.time_span().0 as f32;
                                                status = format!(
                                                    "{label}: material animation \u{b7} {} groups \u{b7} {} tracks",
                                                    m.groups.len(),
                                                    m.track_count()
                                                );
                                            }
                                            (None, Ok(i)) => {
                                                if skeletons.is_none() && skel_rx.is_none() {
                                                    skel_error = None;
                                                    skel_rx = Some(crate::heroidx::spawn(w.labels.clone(), w.side_labels.clone()));
                                                }
                                                hero_picker = Some(HeroPicker {
                                                    what: format!("clip {label}"),
                                                    want: i.bones,
                                                    filter: String::new(),
                                                    then: Some((clip, label.clone())),
                                                });
                                                status = format!("{label}: pick the hero it plays on");
                                            }
                                            (Some(h), Ok(_)) => {
                                                if preview.as_ref().map(|p| (p.hash, p.label.as_str())) != Some((h.hash, h.label.as_str())) {
                                                    actions.push(Act::LoadModelHash(h.hash, h.label.clone()));
                                                }
                                                actions.push(Act::PlayClip(clip, label));
                                            }
                                        }
                                    }
                                    // A string table: every table's text side by side, this one first.
                                    Kind::Text if row.type_id == mercs2_formats::types::TYPE_ID_STRINGDB => {
                                        let mut sources = Vec::new();
                                        for r in index.rows(Kind::Text) {
                                            if r.type_id != mercs2_formats::types::TYPE_ID_STRINGDB {
                                                continue;
                                            }
                                            for src in std::iter::once(r.src).chain(r.others.iter().copied()) {
                                                sources.push(crate::textpage::Source {
                                                    hash: r.hash,
                                                    label: r.label(),
                                                    path: w.source_path(src).to_string(),
                                                });
                                            }
                                        }
                                        let lua: Vec<String> = lua_corpus.iter().map(|(_, c, _)| c.clone()).collect();
                                        text_page.ensure(sources, index.names.clone(), lua);
                                        text_page.table = Some(row.hash);
                                        text_page.font = None;
                                        tex_view = None;
                                        status = format!("string table {}", row.label());
                                    }
                                    // A font: its pages with every glyph's box, read from the WAD
                                    // the row previews from; its pages resolve like any texture.
                                    Kind::Text if row.type_id == mercs2_formats::types::TYPE_ID_FONT => {
                                        let (hash, label, src) = (row.hash, row.label(), row.src);
                                        let opened = (|| -> Result<crate::textpage::FontView, String> {
                                            let file = std::path::Path::new(w.source_path(src))
                                                .file_name()
                                                .map_or_else(|| format!("source {src}"), |f| f.to_string_lossy().into_owned());
                                            let wad = w.source_mut(src).ok_or_else(|| format!("no source {src}"))?;
                                            let c = wad::extract_container_typed(wad, hash, mercs2_formats::types::TYPE_HASH_FONT)?;
                                            let font = mercs2_formats::font::parse_font(&c)?;
                                            let pages = font
                                                .pages
                                                .iter()
                                                .map(|&t| (name_or_hash(&index, t), w.texture_best(t).map(|td| crate::texpng::decode_bc(&td))))
                                                .collect();
                                            Ok(crate::textpage::FontView::new(hash, label.clone(), file, font, pages))
                                        })();
                                        match opened {
                                            Ok(f) => {
                                                status = format!("font {label}: {} glyphs on {} pages", f.font.glyphs.len(), f.pages.len());
                                                text_page.font = Some(f);
                                                text_page.table = None;
                                                tex_view = None;
                                            }
                                            Err(e) => status = format!("FONT FAILED: {label}: {e}"),
                                        }
                                    }
                                    // A category with no faithful preview SAYS so rather than silently doing
                                    // nothing. It is browsable, and that is the honest state of it — the
                                    // standing rule against affordances that look like they work.
                                    other => {
                                        if let crate::index::Preview::None(why) = other.preview() {
                                            status = format!("{} — {why}", row.label());
                                        }
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
                            // On the Anims tab the clip keys step the tab's rows: each plays on the hero.
                            Act::ClipNav(d) if wb == Workbench::Inspect && kind == Kind::Animation => {
                                if !filtered.is_empty() {
                                    let n = filtered.len() as i32;
                                    sel = (sel as i32 + d).rem_euclid(n) as usize;
                                    actions.push(Act::LoadRow(sel));
                                }
                            }
                            Act::PlayClip(clip, label) => {
                                let Some(p) = &mut preview else {
                                    status = format!("{label}: no model is loaded to play it on");
                                    continue;
                                };
                                let ci = match p.clip_catalog.iter().position(|e| e.hash == clip) {
                                    Some(i) => i,
                                    None => {
                                        p.clip_catalog.push(ClipEntry {
                                            hash: clip,
                                            handles: Vec::new(),
                                            label: label.clone(),
                                            name: index.names.get(&clip).cloned(),
                                        });
                                        p.clip_catalog.len() - 1
                                    }
                                };
                                actions.push(Act::ClipSel(ci));
                            }
                            Act::HeroPick => {
                                let (what, want, then) = match &anim_clip {
                                    Some((clip, label)) => match clip_infos.get(clip) {
                                        Some(Ok(i)) if i.material.is_none() => {
                                            (format!("clip {label}"), i.bones.clone(), Some((*clip, label.clone())))
                                        }
                                        _ => (format!("clip {label}"), Vec::new(), None),
                                    },
                                    None => ("the Anims and FaceFX tabs".to_string(), Vec::new(), None),
                                };
                                if skeletons.is_none() && skel_rx.is_none() {
                                    skel_error = None;
                                    skel_rx = Some(crate::heroidx::spawn(w.labels.clone(), w.side_labels.clone()));
                                }
                                hero_picker = Some(HeroPicker { what, want, filter: String::new(), then });
                            }
                            Act::HeroClose => hero_picker = None,
                            Act::SetHero(hash, label) => {
                                let want = hero_picker.as_ref().map_or(Vec::new(), |hp| hp.want.clone());
                                // A model holding none of the bones it is chosen for is refused.
                                if !want.is_empty() {
                                    match skeletons.as_ref().and_then(|s| s.coverage(hash, &want)) {
                                        Some(0) => {
                                            status = format!("{label} holds none of the {} bones it would play — not made the hero", want.len());
                                            continue;
                                        }
                                        None => {
                                            status = format!("{label}: its skeleton is not read yet");
                                            continue;
                                        }
                                        Some(_) => {}
                                    }
                                }
                                let h = crate::settings::Hero { hash, label: label.clone() };
                                hero = Some(h.clone());
                                cfg.hero = Some(h);
                                status = match cfg.save() {
                                    Ok(p) => format!("hero {label} (saved in {})", p.display()),
                                    Err(e) => format!("hero {label} — SETTINGS NOT SAVED: {e}"),
                                };
                                actions.push(Act::LoadModelHash(hash, label));
                                if let Some(then) = hero_picker.take().and_then(|hp| hp.then) {
                                    actions.push(Act::PlayClip(then.0, then.1));
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
                                                status = unplayable(&clip_infos, hash, &p.clip_catalog[ci].label, &p.hier);
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
                            Act::StartShipment(label, subject) => {
                                // The menu offers this only for a subject some kind takes.
                                let Some(kind) = crate::quartermaster::first_route(subject) else {
                                    status = format!("START A SHIPMENT FAILED: no kind takes {label}");
                                    continue;
                                };
                                let Some(dir) = rfd::FileDialog::new()
                                    .set_title("New Shipment — pick an empty folder")
                                    .pick_folder()
                                else {
                                    continue;
                                };
                                match qm.start_from(&dir, qm_names.as_ref(), kind, &label) {
                                    Ok(at) => {
                                        wb = Workbench::Quartermaster;
                                        status = format!(
                                            "new Shipment {} — contributions[{at}] {kind} from {label}",
                                            dir.display()
                                        );
                                    }
                                    Err(e) => status = format!("START A SHIPMENT FAILED: {e}"),
                                }
                            }
                            Act::OpenExport(hash, label) => {
                                // The outfit on show, when this is the loaded kit-template row.
                                let outfit = preview
                                    .as_ref()
                                    .filter(|p| p.hash == hash && p.label == label)
                                    .and_then(|p| p.kit.as_ref())
                                    .and_then(|(_, shown)| shown.as_ref())
                                    .map(|(_, n)| *n);
                                export_dialog =
                                    Some(ExportDialog::new(hash, &label, outfit, imported.contains_key(&hash)));
                            }
                            Act::ExportFolder => {
                                if let Some(d) = export_dialog.as_mut() {
                                    if let Some(f) = rfd::FileDialog::new()
                                        .set_title("Export into…")
                                        .set_directory(&d.folder)
                                        .pick_folder()
                                    {
                                        d.folder = f;
                                    }
                                }
                            }
                            Act::RunExport => {
                                let Some(d) = export_dialog.take() else { continue };
                                if let Some(why) = d.refusal(d.format) {
                                    status = format!("EXPORT FAILED: {why}");
                                    export_dialog = Some(d);
                                    continue;
                                }
                                match d.format {
                                    // The OBJ writer decodes nothing heavy; safe inline.
                                    ExportFormat::Obj => {
                                        status = match source_model_data(&mut w, &imported, d.hash)
                                            .and_then(|md| export_model_data(&md, &d.folder, &d.name))
                                        {
                                            Ok(dir) => format!("exported {} -> {dir}", d.label),
                                            Err(e) => format!("EXPORT FAILED: {e}"),
                                        };
                                    }
                                    // The rigged bundle decodes every clip, so it runs on a worker
                                    // and the progress window reports until it lands.
                                    ExportFormat::Bundle => {
                                        if exporter.is_some() {
                                            status = "EXPORT FAILED: another export is still running".into();
                                            export_dialog = Some(d);
                                            continue;
                                        }
                                        status = format!("exporting {}…", d.label);
                                        exporter = Some(export_bundle_in_background(
                                            wadpath.clone(),
                                            overlays.clone(),
                                            d.hash,
                                            d.label.clone(),
                                            d.name.clone(),
                                            index.clone(),
                                            d.folder.clone(),
                                            d.outfit,
                                        ));
                                    }
                                }
                            }
                            // ── Library routing: an asset becomes a contribution.
                            //
                            // Plan 02 line 88 specified this and it had no implementation — there
                            // was no code path from an Inspect selection into `qm` whatsoever, so
                            // the only verb the Library offered for an asset was OBJ export.
                            Act::AddToShipment(kind, _hash, label) => {
                                let out = (|| -> Result<String, String> {
                                    // Scaffolds one if none is open, so this is never a dead end.
                                    let dir = qm.ensure_shipment(qm_names.as_ref())?;
                                    let n = qm.contribution_count() + 1;
                                    let c = crate::quartermaster::seeded(kind, &label, n)
                                        .ok_or_else(|| format!("no stub for `{kind}`"))?;
                                    let at =
                                        qm.upsert_contribution(qm_names.as_ref(), None, c)?;
                                    Ok(format!(
                                        "contributions[{at}] {kind} from {label} — {}",
                                        dir.display()
                                    ))
                                })();
                                match out {
                                    Ok(m) => {
                                        // Go to the page that now holds it: the contribution is a
                                        // deliberately-unfinished stub and the form is where it
                                        // gets finished.
                                        wb = Workbench::Quartermaster;
                                        status = m;
                                    }
                                    Err(e) => status = format!("ADD TO SHIPMENT FAILED: {e}"),
                                }
                            }
                            Act::Bench(bench, hash, label) => {
                                // The work happens in the bench and nothing is written until it
                                // saves: opened from the Library it has no contribution yet, and its
                                // save appends the new one.
                                craft_subject = None;
                                wb = bench_page(bench);
                                match bench {
                                    // The rig bench needs a TARGET skeleton; the asset is it.
                                    crate::quartermaster::Craft::Rig => {
                                        actions.push(Act::RetargetSetTarget(hash, label.clone()));
                                    }
                                    crate::quartermaster::Craft::Conform => {
                                        actions.push(Act::ModDonor(hash, label.clone()));
                                    }
                                }
                                status = format!("{} — donor {label}", wb.verb());
                            }
                            Act::LoadTexture(hash, label) => {
                                // The plate is bound here: an unbound view showed the previous
                                // plate (or the shell art) under this texture's caption.
                                let row = index.textures().iter().find(|r| r.hash == hash).cloned();
                                let mut tv = TexView::of_list(vec![hash], vec![label.clone()]);
                                if let Some(r) = row {
                                    tv.copies = texture_copies(&mut w, hash, r.src, &r.others);
                                    tv.src = Some(r.src);
                                }
                                bind_tex_plate(&mut w, &mut scene, &mut tv);
                                tex_view = Some(tv);
                                status = format!("texture {label}");
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
                            Act::Outfit(template, n) => {
                                if let Some(p) = &mut preview {
                                    status = match show_outfit(p, &mut scene, &template, n) {
                                        Ok(s) => s,
                                        Err(e) => format!("OUTFIT FAILED: {e}"),
                                    };
                                }
                            }
                            Act::ImportModel => {
                                if phase == Phase::Ready {
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
                                            donor_ref.map(|(h, _)| h), &index, &anim_sel, &lua_corpus,
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
                            Act::TexOfPreview => {
                                if let Some(p) = &preview {
                                    if !p.tex_hashes.is_empty() {
                                        let mut tv = TexView::of_list(
                                            p.tex_hashes.clone(),
                                            p.tex_hashes.iter().map(|h| name_or_hash(&index, *h)).collect(),
                                        );
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
                            Act::TexSource(s) => {
                                if let Some(tv) = &mut tex_view {
                                    tv.src = Some(s);
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
                            Act::LuaOpenAt(path, line) => {
                                match lua_corpus.iter().find(|(p, _, _)| *p == path) {
                                    Some((_, content, _)) => {
                                        let mut v = crate::luaview::LuaView::new(&path, content, "");
                                        v.scroll_to_line(line);
                                        lua_view = Some(v);
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
                                    index = AssetIndex::build(&w.wads, &w.side, names);
                                    // The dropped overlay may have carried the worldentity in use.
                                    let kit_templates = match load_kit_templates(&mut w) {
                                        Ok(t) => t,
                                        Err(e) => {
                                            status = format!("kit templates unreadable, no template rows: {e}");
                                            KitTemplates::default()
                                        }
                                    };
                                    index.add_kit_templates(&kit_templates.kits, &kit_templates.all);
                                    filtered = refilter(&index, kind, &filter, lens);
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
                                // One reference at a time: the new donor replaces the last.
                                if let Some((old, oe)) = donor_ref.replace((dh, e)) {
                                    world.despawn(oe).ok();
                                    scene.forget_entity(oe);
                                    if old != dh && preview.as_ref().map(|p| p.hash) != Some(old) {
                                        scene.unload_model(old);
                                    }
                                }
                                status = format!("donor reference at origin: {dl}");
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
                            if show_nodes && eff == Workbench::Mods {
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
                            if show_hardpoints && eff == Workbench::Mods {
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

                    // ── Session log: record `status` whenever it CHANGED this frame. Runs after the
                    // action processor, so a click and the outcome it produced land on one line.
                    if status != last_status {
                        push_log(&mut log, start.elapsed().as_secs_f32(), &status);
                        last_status.clear();
                        last_status.push_str(&status);
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
                    // `--ui-state`: once the window has settled, the next frame presented is read back.
                    if let Some((_, frames)) = capture_to.as_mut() {
                        if *frames > 0 {
                            *frames -= 1;
                        } else if !capture_asked {
                            if let Err(e) = scene.request_capture() {
                                eprintln!("--ui-state: {e}");
                                std::process::exit(2);
                            }
                            capture_asked = true;
                        }
                    }
                    let r = if tex_view.is_some() || (preview.is_none() && donor_ref.is_none()) {
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
                    if let Some(frame) = scene.take_capture() {
                        let (path, _) = capture_to.take().expect("a capture is only asked for with a path");
                        let written = frame.and_then(|f| {
                            crate::texpng::write_png(&path.to_string_lossy(), f.width, f.height, &f.rgba)
                        });
                        match written {
                            Ok(()) => {
                                println!("--ui-state: captured {}", path.display());
                                elwt.exit();
                            }
                            Err(e) => {
                                eprintln!("--ui-state: capture to {}: {e}", path.display());
                                std::process::exit(2);
                            }
                        }
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
    let got = match tv.src {
        Some(s) => w.texture_from(s, h),
        None => w.texture_best(h),
    };
    match got {
        Ok(td) => {
            tv.info = format!("{}x{}  {:?}  {} mips", td.width, td.height, td.format, td.mip_count);
            scene.set_loading_art(&td);
        }
        Err(e) => tv.info = format!("EXTRACT FAILED: {e}"),
    }
}

/// Everything `load_preview` and the donor-reference loader share: container → indexed geometry →
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
/// The donor, the source, and the [`LowerOpts`] to lower it with — the export's half of the
/// preview.
///
/// [`LowerOpts`]: mercs2_formats::char_lower::LowerOpts
///
/// Exists so `Act::ExportFaithfulCharacter` can call the SAME
/// `mercs2_formats::char_lower::character_into_donor` the Quartermaster calls, instead of the
/// hand-rolled single-host injection it used to carry. That copy could not export what the
/// Quartermaster ships — the working fixture needs five host groups — so "what you see on the
/// pedestal is exactly what gets written" was true of the conform and false of everything after it.
///
/// The preview path (`faithful_char_skin`) still exists and still runs the same
/// `convention_overrides` + donor-transfer policy; this returns that policy as data rather than as
/// an already-built `CharSkin`, because the lowering wants to run `build_character` itself.
pub(crate) fn faithful_lower_opts(
    wad_paths: &[String],
    thash: u32,
    rt: &crate::retarget::Retarget,
    src_path: &std::path::Path,
) -> Result<
    (
        mercs2_formats::char_skin::CharGlbData,
        Vec<u8>,
        mercs2_formats::char_lower::LowerOpts,
    ),
    String,
> {
    let donor = crate::publish::donor_block(wad_paths, thash)?;
    let skel = mercs2_formats::skeleton::Skeleton::from_block(&donor)?;
    let target = mercs2_formats::char_skin::TargetSkeleton::from_skeleton(&skel);
    let glb = crate::import::load_char_glb(src_path)?;
    let opts = mercs2_formats::char_lower::LowerOpts {
        overrides: rt.convention_overrides(target.bones.len()),
        native_rig: rt.convention == crate::retarget::SourceRig::Pandemic,
        // The Workshop export writes a bare model block, not a WAD, so there is nowhere to put a
        // texture block beside it. Skins are bound by the Quartermaster's `textures:`.
        repoints: Vec::new(),
        part_material_textures: std::collections::HashMap::new(),
        // The Workshop preview/export always uses the faithful multi-group path; single-group is a
        // Quartermaster build choice (`AddOutfit::single_group`) for placement-stable foreign rigs.
        single_host: false,
        atlas_cells: std::collections::HashMap::new(),
    };
    Ok((glb, donor, opts))
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
    // ValveBiped joins CoD on the full-override path. Its names ARE readable by the generic mapper,
    // but `automap`'s `ladder_assign` spreads a chain across the rungs by INDEX (`k*rungs/n`) with no
    // idea where the joints physically sit. ValveBiped carries FOUR spine rungs (it skips Spine3)
    // against Pandemic's three, so the overflow doubles up the BOTTOM rung and every rung above the
    // waist lands low — Spine1 -5.8%, Spine2 -6.3%, Spine4 -14.0% of body height — compressing the
    // torso into the pelvis. `automap` is parity-locked to Logan's mapper (a ValveBiped fixture pins
    // this exact behaviour), so the ladder is corrected HERE, in the convention's hand-verified table,
    // rather than by forking the shared mapper.
    // Pandemic (NATIVE) joins them, for the strongest reason of the three: there is nothing to
    // retarget. A model authored on the exported base kit is ALREADY on the donor's skeleton, so the
    // explicit table is a pure identity by bone name and feeding it as full overrides is what keeps
    // the authoring round-trip lossless. Left on the automap path the game's own rig was detected as
    // `generic`, collapsed 116 bones onto 28, and came back 1.224 m tall with the skull drawn out
    // into a spike — measured on a re-imported `pmc_hum_mattias` bundle.
    // Lifted to `mercs2_formats::retarget::convention_overrides`, where the Quartermaster can
    // reach it too. It lived here, in a binary crate, which is why the headless build silently
    // used the generic automap while this preview used the hand-verified table — the two paths
    // disagreed on 21 of 45 source joints and nothing said so.
    let overrides = rt.convention_overrides(target.bones.len());
    let mut cs = mercs2_formats::char_skin::build_character(&glb.build_input(&target, None, overrides, false))?;
    // DONOR TRANSFER. build_character carries the SOURCE rig's weights across the bone map, which is
    // fuzzy on the limbs for a mismatched rig (a 119-bone Unreal mannequin onto a 116-bone Pandemic
    // skeleton) and tears the arms. Overwrite them by sampling the RETAIL donor's own weights at each
    // conformed vertex -- the exact step the CLI did and this path used to skip, which is why the
    // workshop preview showed broken arms the shipped asset does not have.
    //
    // NOT for a NATIVE rig. Donor transfer exists to repair weights a fuzzy bone map mangled; on a
    // model authored against the exported base kit there is no fuzzy map and its weights ALREADY ARE
    // the retail ones. Resampling them from the donor would throw away every weight the author
    // deliberately painted on new geometry and silently replace it with whatever the stock body does
    // at that point in space — the one thing an authoring round-trip must never do.
    if rt.convention == crate::retarget::SourceRig::Pandemic {
        eprintln!("faithful_char_skin: native rig — keeping the author's own weights (no donor transfer)");
    } else {
        match mercs2_formats::char_skin::donor_transfer::apply_donor_transfer(
            &mut cs,
            &glb.tris,
            &donor,
            &mercs2_formats::char_skin::donor_transfer::DonorTransferOpts::default(),
        ) {
            Ok(msg) => eprintln!("faithful_char_skin: {msg}"),
            Err(e) => eprintln!("faithful_char_skin: donor transfer skipped ({e}); using conform weights"),
        }
    }
    Ok((cs, glb, donor))
}

/// The target HIER bone names AND their bind-pose positions (translation of `world_bind`), index-
/// aligned. Uses `skin.rig` — the SAME rig `RetargetApply` grafts onto the import — so the retarget's
/// target indices line up with what actually gets rendered. Positions feed the spatial align.
pub(crate) fn target_bone_info(
    w: &mut WadStack,
    hash: u32,
    // The full name map (`crate::index::load_all_names`): the GUI's `AssetIndex::names`, or the
    // headless mode's own load.
    all_names: &std::collections::HashMap<u32, String>,
) -> (Vec<String>, Vec<[f32; 3]>, Vec<i32>) {
    let Ok(md) = load_model_data(w, hash) else { return (Vec::new(), Vec::new(), Vec::new()) };
    let hashes: Vec<u32> = md.skin.rig.iter().map(|b| b.name_hash).collect();
    let names_map = resolve_node_names(&hashes, all_names);
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

/// A character kit's part mixer (`mercs2_formats::mixr`) and the world templates that wear it.
pub(crate) struct KitOutfits {
    pub(crate) mixr: mercs2_formats::mixr::Mixr,
    pub(crate) templates: Vec<mercs2_formats::mixr::KitTemplate>,
}

impl KitOutfits {
    /// A template of this kit by its `Name`: [`Mixr::template_outfits`](mercs2_formats::mixr::Mixr::template_outfits).
    /// An error when no template of the kit has the name, or its profile is not the kit's.
    pub(crate) fn of_template(&self, name: &str) -> Result<(usize, Vec<u32>), String> {
        let t = self
            .templates
            .iter()
            .find(|t| t.name.as_deref() == Some(name))
            .ok_or_else(|| format!("no template of this kit is named {name:?}"))?;
        self.mixr.template_outfits(t)
    }
}

/// The kit data of `hash`, or `None` when its container carries no `MIXR`.
pub(crate) fn kit_outfits(w: &mut WadStack, hash: u32) -> Result<Option<KitOutfits>, String> {
    let templates = load_kit_templates(w)?;
    kit_outfits_with(w, hash, &templates.kits)
}

/// [`kit_outfits`] with the world's templates already read ([`load_kit_templates`]).
pub(crate) fn kit_outfits_with(
    w: &mut WadStack,
    hash: u32,
    templates: &[mercs2_formats::mixr::KitTemplate],
) -> Result<Option<KitOutfits>, String> {
    let c = w.extract_container(hash)?;
    let Some(mixr) = mercs2_formats::mixr::Mixr::find(&c)? else { return Ok(None) };
    let templates = templates.iter().filter(|t| t.model == hash).cloned().collect();
    Ok(Some(KitOutfits { mixr, templates }))
}

/// The open stack's `worldentity` templates that name a model, and among them every one that picks
/// a character-kit outfit: a non-zero `ModelMixerProfile` on a model whose container carries a
/// `MIXR`. A profile on a model with no mixer is passed over at spawn (`0x004D0393`: no mixer
/// object, no outfit) — retail has six, the story characters among them — so those templates are
/// not kit templates.
pub(crate) fn load_kit_templates(w: &mut WadStack) -> Result<KitTemplates, String> {
    let we = mercs2_formats::worldentity::WorldEntity::parse(&w.worldentity()?)?;
    let all = mercs2_formats::mixr::kit_templates(&we)?;
    let mut is_kit: HashMap<u32, bool> = HashMap::new();
    let mut out = Vec::new();
    for t in all.iter().cloned() {
        if t.profile == 0 {
            continue;
        }
        let kit = match is_kit.get(&t.model) {
            Some(&k) => k,
            None => {
                // A model no open wad carries (retail templates name 307 such) has no MIXR.
                let present = w
                    .wads
                    .iter()
                    .any(|x| wad::aset_types(x, t.model).iter().any(|(ty, _, _)| *ty == wad::MODEL_ASET_TYPE_ID));
                let k = present && {
                    let c = w.extract_container(t.model)?;
                    mercs2_formats::mixr::Mixr::find(&c)?.is_some()
                };
                is_kit.insert(t.model, k);
                k
            }
        };
        if kit {
            out.push(t);
        }
    }
    Ok(KitTemplates { kits: out, all })
}

/// The world's templates that name a model: `all` of them, and the `kits` among them — those
/// that pick an outfit of a character kit.
#[derive(Default)]
pub(crate) struct KitTemplates {
    pub kits: Vec<mercs2_formats::mixr::KitTemplate>,
    pub all: Vec<mercs2_formats::mixr::KitTemplate>,
}

/// The render state that shows exactly `outfit` of kit `hash` at rung 0: the spawn path's per-part
/// `visible` states (`Mixr::outfit_node_states`) run through the same state machine as
/// [`default_render_state`].
pub(crate) fn outfit_render_state(
    w: &mut WadStack,
    hash: u32,
    mixr: &mercs2_formats::mixr::Mixr,
    outfit: u32,
) -> Result<mercs2_engine::render_state::RenderState, String> {
    use mercs2_formats::orchestrator as orch;
    let c = w.extract_container(hash)?;
    let hier = orch::parse_hier(&c);
    let sm = orch::parse_state_machine(&c).ok_or_else(|| format!("kit 0x{hash:08X} has no state machine"))?;
    let chosen = mixr.outfit_node_states(&sm, &hier, outfit)?;
    Ok(mercs2_engine::render_state::RenderState {
        lod: 0,
        view_state: 0x01,
        node_enable: orch::machine_node_enable(&sm, &hier, &chosen),
    })
}

/// Show outfit `n` of `template` on a kit preview: its per-switch states (`Mixr::outfit_node_states`)
/// re-executed through the preview's state machine into the entity's render state. Returns the
/// status line, or why the outfit cannot be shown.
fn show_outfit(p: &mut Preview, scene: &mut Scene, template: &str, n: usize) -> Result<String, String> {
    let Some((kit, shown)) = &mut p.kit else { return Err(format!("{} is not a character kit", p.label)) };
    let sm = p.machine.as_ref().ok_or_else(|| format!("kit {} has no state machine", p.label))?;
    let (pi, outfits) = kit.of_template(template)?;
    let outfit = *outfits
        .get(n)
        .ok_or_else(|| format!("{template:?} has {} outfit(s); #{n} does not exist", outfits.len()))?;
    p.node_state = kit.mixr.outfit_node_states(sm, &p.hier_nodes, outfit)?;
    let node_enable = mercs2_formats::orchestrator::machine_node_enable(sm, &p.hier_nodes, &p.node_state);
    let rs = mercs2_engine::render_state::RenderState { lod: p.tier.trailing_zeros() as u8, view_state: p.tier, node_enable };
    for (gi, d) in p.draws.iter().enumerate() {
        if rs.segment_visible(d.lod_mask, d.node) {
            p.hidden.remove(&gi);
        } else {
            p.hidden.insert(gi);
        }
    }
    p.node_enable = rs.node_enable.clone();
    scene.set_entity_render_state(p.entity, rs);
    let parts = kit.mixr.parts_of(outfit);
    *shown = Some((template.to_string(), n));
    Ok(format!("{template} — outfit {} of {} (profile {pi}): parts {parts:?}", n + 1, outfits.len()))
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

/// GPU-upload a model by hash (used by the Fit bench's donor reference for a model not previewed yet).
fn load_gpu_only(w: &mut WadStack, scene: &mut Scene, hash: u32) -> Result<(), String> {
    let md = load_model_data(w, hash)?;
    scene.load_model(hash, &md.verts, &md.indices, &md.draws, &md.textures, &md.skin);
    Ok(())
}

/// Put `md` on the preview pedestal as `hash`: upload to the GPU, spawn its entity at the
/// origin, tear down the previous preview (keeping its model resident if the donor reference still
/// draws it). Works for WAD models and drag-drop imports alike.
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
            if ui.selectable_label(false, egui::RichText::new(names[i].as_str()).monospace()).hovered() {
                *hov = Some(i);
            }
        } else {
            let r = egui::CollapsingHeader::new(egui::RichText::new(names[i].as_str()).monospace())
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
    keep: Option<u32>,
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
        w, scene, world, hash, label, md, &*preview, keep, index, anim_sel, lua_corpus,
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
            .map(|(h, _)| target_bone_info(w, *h, &index.names))
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
    keep: Option<u32>,
    index: &AssetIndex,
    anim_sel: &Option<AnimSelector>,
    lua_corpus: &[(String, String, String)],
) -> Preview {

    // Tear down the previous preview AFTER the new load succeeded (a failed load keeps the old one).
    // Its model stays loaded when `keep` (the Fit bench's donor reference) still draws it.
    if let Some(p) = old {
        world.despawn(p.entity).ok();
        scene.forget_entity(p.entity);
        if p.hash != hash && keep != Some(p.hash) {
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

    // A character kit (its container carries a MIXR) shows nothing until an outfit is chosen; a row
    // labelled by one of its templates opens on that template's first outfit.
    let kit = match kit_outfits_with(w, hash, &index.kit_templates) {
        Ok(k) => k.map(|k| (k, None)),
        Err(e) => {
            eprintln!("[workshop] ERROR: kit data of {label} (0x{hash:08X}) unreadable: {e}");
            None
        }
    };
    let opens_outfit = kit
        .as_ref()
        .is_some_and(|(k, _)| k.templates.iter().any(|t| t.name.as_deref() == Some(label.as_str())));

    let mut p = Preview {
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
        kit,
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
    };
    if opens_outfit {
        let template = p.label.clone();
        if let Err(e) = show_outfit(&mut p, scene, &template, 0) {
            eprintln!("[workshop] ERROR: outfit 1 of {template:?}: {e}");
        }
    }
    p
}

/// The folder [`ExportDialog`] proposes, and the one the headless exports write under.
pub(crate) const EXPORT_ROOT: &str = "workshop_export";

/// What the Export dialog writes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExportFormat {
    /// [`export_bundle_by_hash`]: glTF with the rig and every clip, each LOD rung's original
    /// container bytes, the textures, and the manifest that reassembles them.
    Bundle,
    /// [`export_model_data`]: an OBJ per LOD rung with MTL materials and decoded PNG textures.
    Obj,
}

impl ExportFormat {
    const ALL: [ExportFormat; 2] = [ExportFormat::Bundle, ExportFormat::Obj];
    fn label(self) -> &'static str {
        match self {
            ExportFormat::Bundle => "Lossless bundle",
            ExportFormat::Obj => "OBJ + MTL + PNG",
        }
    }
    fn what(self) -> &'static str {
        match self {
            ExportFormat::Bundle => {
                "glTF with the rig and every clip, each LOD rung's original bytes, textures, and a manifest to rebuild it"
            }
            ExportFormat::Obj => "a static mesh per LOD rung, materials, and decoded textures",
        }
    }
}

/// The Export dialog: what is being exported, and the name, format and folder chosen for it.
struct ExportDialog {
    hash: u32,
    label: String,
    name: String,
    format: ExportFormat,
    folder: PathBuf,
    /// The kit outfit to write, for a kit-template row shown in the viewport.
    outfit: Option<usize>,
    /// A drag-dropped import: it has no LOD-rung container, so only OBJ applies.
    imported: bool,
}

impl ExportDialog {
    fn new(hash: u32, label: &str, outfit: Option<usize>, imported: bool) -> ExportDialog {
        ExportDialog {
            hash,
            label: label.to_string(),
            name: label.to_string(),
            format: if imported { ExportFormat::Obj } else { ExportFormat::Bundle },
            folder: PathBuf::from(EXPORT_ROOT),
            outfit,
            imported,
        }
    }

    /// Why `format` cannot be written for this asset, if it cannot.
    fn refusal(&self, format: ExportFormat) -> Option<&'static str> {
        (self.imported && format == ExportFormat::Bundle)
            .then_some("an imported mesh has no LOD-rung container to bundle")
    }
}

/// `name` as a folder name: anything but ASCII letters, digits, `_` and `-` becomes `_`.
pub(crate) fn export_dir_name(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect()
}

/// Headless OBJ export (`--export <name|0xHASH>`): resolve the model through a fresh stack and
/// write it out as OBJ + textures under [`EXPORT_ROOT`]. The rigged sibling is `--export-bundle`.
pub(crate) fn export_by_hash(
    base: &str,
    overlays: &[String],
    hash: u32,
    label: &str,
) -> Result<String, String> {
    let mut w = WadStack::open(base, overlays)?;
    let md = load_model_data(&mut w, hash)?;
    export_model_data(&md, std::path::Path::new(EXPORT_ROOT), label)
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
    // The folder name under `outroot` (made safe by `export_dir_name`).
    name: &str,
    // The name catalog — also the source of each clip's procedural (ActionTable-derived) name.
    index: &AssetIndex,
    outroot: &std::path::Path,
    // For a kit template's row (`label` names a template of this kit): which of its outfits to write
    // (0-based; the first when `None`). Ignored for any other model.
    outfit: Option<usize>,
) -> Result<String, String> {
    let mut w = WadStack::open(base, overlays)?;
    let md = load_model_data(&mut w, hash)?;
    // A kit template's bundle is the template's outfit, every LOD tier of it — the groups whose node
    // the outfit's state machine leaves enabled — not the whole kit with every head on.
    let mut suffix = String::new();
    let draws: Vec<DrawGroup> =
        if index.kit_templates.iter().any(|t| t.model == hash && t.name.as_deref() == Some(label)) {
            let kit = kit_outfits_with(&mut w, hash, &index.kit_templates)?
                .ok_or_else(|| format!("{label}: its model 0x{hash:08X} carries no MIXR"))?;
            let (_, outfits) = kit.of_template(label)?;
            let n = outfit.unwrap_or(0);
            let o = *outfits
                .get(n)
                .ok_or_else(|| format!("{label:?} has {} outfit(s); #{n} does not exist", outfits.len()))?;
            let rs = outfit_render_state(&mut w, hash, &kit.mixr, o)?;
            suffix = format!("_outfit{}", n + 1);
            md.draws.iter().filter(|d| rs.node_visible(d.node)).cloned().collect()
        } else {
            md.draws.clone()
        };
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
    let dir = outroot.join(format!("{}{suffix}", export_dir_name(name)));
    crate::bundle::export_bundle(
        &dir,
        label,
        hash,
        &lods,
        &md.verts,
        &md.indices,
        &draws,
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
    name: String,
    index: AssetIndex,
    outroot: std::path::PathBuf,
    outfit: Option<usize>,
) -> Exporter {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker_label = label.clone();
    std::thread::spawn(move || {
        let _ = tx.send(export_bundle_by_hash(&base, &overlays, hash, &worker_label, &name, &index, &outroot, outfit));
    });
    Exporter { rx, label }
}

/// Write a model to `<outroot>/<name>/` as OBJ + MTL + decoded PNG textures (game-space
/// coordinates, one `usemtl` per draw group). Works for WAD assets and imports —
/// the hand-off point into the UCFX/patch pipeline.
fn export_model_data(md: &ModelData, outroot: &std::path::Path, name: &str) -> Result<String, String> {
    let dir = outroot.join(export_dir_name(name));
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

/// Render the hero portrait for the Saves page at the given [`crate::shot::OrbitCam`] and return
/// an updated `egui::TextureHandle`.
///
/// Pipeline:
/// 1. Look up (or lazily build) a per-hash [`crate::shot::HeroRenderer`] in `renderers`. The
///    first miss pays the ~500 ms model-load + pipeline + mesh-upload cost; subsequent calls
///    for the same hash reuse the stashed renderer.
/// 2. Call `render_rgba(cam, size)` to get back one RGBA8 frame at the viewport's pixel size —
///    the renderer rewrites the camera uniform (and rebuilds its targets only when `size`
///    changed), so this part is on the order of a draw + readback.
/// 3. Build an `egui::ColorImage` from the bytes. If `existing_tex` is `Some`, mutate it in
///    place via `TextureHandle::set` so egui reuses the GPU allocation (and the Image widget
///    binding doesn't flicker on drag); otherwise allocate a new handle via `ctx.load_texture`.
///
/// Draw ranges carry their decoded diffuse/normal/specular — `md.textures` already holds the
/// FULL mip chain (`WadStack::texture_best` assembled it), and `texpng::decode_bc` resolves
/// DXT1/DXT5 into RGBA8. A `None` slot (unassigned material texture) falls back to the shot
/// shader's flat sampler.
fn ensure_hero_render(
    w: &mut WadStack,
    hash: u32,
    name: &str,
    cam: &crate::shot::OrbitCam,
    size: (u32, u32),
    renderers: &mut HashMap<u32, crate::shot::HeroRenderer>,
    existing_tex: Option<egui::TextureHandle>,
    ctx: &egui::Context,
) -> Result<egui::TextureHandle, String> {
    if size.0 == 0 || size.1 == 0 {
        return Err(format!("hero viewport size not reported before render ({}x{})", size.0, size.1));
    }
    // Build-or-fetch the per-hash renderer.
    if !renderers.contains_key(&hash) {
        let md = crate::app::load_model_data(w, hash)
            .map_err(|e| format!("load `{name}` (0x{hash:08X}): {e}"))?;
        if md.verts.is_empty() || md.indices.is_empty() || md.draws.is_empty() {
            return Err(format!(
                "model `{name}` loaded with no geometry (verts={}, indices={}, draws={})",
                md.verts.len(),
                md.indices.len(),
                md.draws.len()
            ));
        }
        // SV from the model's own BLENDINDICES/BLENDWEIGHT. An identity palette on a skinned
        // model collapses to a bind pose, which is what a portrait wants — pose/animation is a
        // non-goal for this round.
        let sv: Vec<crate::shot::SV> = md
            .verts
            .iter()
            .map(|v| crate::shot::SV {
                pos: v.pos,
                _p0: 0.0,
                normal: v.normal,
                _p1: 0.0,
                uv: v.uv,
                _p2: [0.0, 0.0],
                tangent: v.tangent,
                joints: [
                    v.joints[0] as u32,
                    v.joints[1] as u32,
                    v.joints[2] as u32,
                    v.joints[3] as u32,
                ],
                weights: [
                    v.weights[0] as f32 / 255.0,
                    v.weights[1] as f32 / 255.0,
                    v.weights[2] as f32 / 255.0,
                    v.weights[3] as f32 / 255.0,
                ],
            })
            .collect();
        const IDENT: [[f32; 4]; 4] = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let palette: Vec<[[f32; 4]; 4]> = vec![IDENT; md.skin.rig.len().max(1)];
        let draws: Vec<crate::shot::DrawTex> = md
            .draws
            .iter()
            .filter(|d| d.index_count > 0)
            .map(|d| crate::shot::DrawTex {
                index_start: d.index_start,
                index_count: d.index_count,
                diffuse: d
                    .diffuse
                    .and_then(|h| md.textures.get(&h))
                    .map(crate::texpng::decode_bc),
                normal: d
                    .normal
                    .and_then(|h| md.textures.get(&h))
                    .map(crate::texpng::decode_bc),
                specular: d
                    .specular
                    .and_then(|h| md.textures.get(&h))
                    .map(crate::texpng::decode_bc),
            })
            .collect();
        if draws.is_empty() {
            return Err(format!(
                "model `{name}` has {} draws but every one is empty — nothing to render",
                md.draws.len()
            ));
        }
        let r = crate::shot::HeroRenderer::new(
            &sv,
            &md.indices,
            &palette,
            (md.stats.bbox_min, md.stats.bbox_max),
            &draws,
        );
        renderers.insert(hash, r);
    }

    // Draw at the requested camera.
    let renderer = renderers.get_mut(&hash).expect("just inserted");
    let rgba = renderer.render_rgba(cam, size);
    let img = egui::ColorImage::from_rgba_unmultiplied([size.0 as usize, size.1 as usize], &rgba);
    // Reuse the existing texture handle on a redraw so the Image widget stays bound to the same
    // id — otherwise a drag-orbit frame-swap can briefly show the old texture at the new rect
    // size while egui catches up to the new allocation.
    if let Some(mut tex) = existing_tex {
        tex.set(img, egui::TextureOptions::LINEAR);
        Ok(tex)
    } else {
        Ok(ctx.load_texture(format!("hero_{hash:08X}"), img, egui::TextureOptions::LINEAR))
    }
}

#[cfg(test)]
mod rail_tests {
    use super::{bench_page, filter_rows, in_lens, Kind, Workbench};
    use crate::domain::Domain;
    use crate::index::AssetRow;

    /// Every page, through a match the compiler checks: a variant added to `Workbench` does not
    /// build here until it is listed, so it cannot dodge the reachability test.
    fn every_page() -> Vec<Workbench> {
        fn listed(w: Workbench) {
            match w {
                Workbench::Inspect
                | Workbench::Saves
                | Workbench::Mods
                | Workbench::Skeleton
                | Workbench::Quartermaster
                | Workbench::Settings
                | Workbench::Live
                | Workbench::Domain(_) => {}
            }
        }
        let mut pages = vec![
            Workbench::Inspect,
            Workbench::Saves,
            Workbench::Mods,
            Workbench::Skeleton,
            Workbench::Quartermaster,
            Workbench::Settings,
            Workbench::Live,
        ];
        pages.extend(Domain::ALL.into_iter().map(Workbench::Domain));
        pages.iter().for_each(|w| listed(*w));
        pages
    }

    /// The rail lists the mockup's seven entries, in its order; Live and Log sit at its foot.
    #[test]
    fn the_rail_is_in_the_mockups_order() {
        let labels: Vec<&str> = Workbench::ALL.iter().map(|w| w.label()).collect();
        assert_eq!(labels, ["Inspect", "Saves", "Driving", "Missions", "Systems", "Shipment", "Settings"]);
        assert!(!Workbench::ALL.contains(&Workbench::Live), "Live is pinned to the foot, not listed");
    }

    /// Inspect's Lens row offers the model domains that are not rail pages, in the mockup's order.
    #[test]
    fn inspect_offers_the_mockups_lenses() {
        let labels: Vec<&str> = Workbench::lenses().iter().map(|d| d.label()).collect();
        assert_eq!(labels, ["World", "Characters", "Weapons"]);
    }

    /// Every page is reached from somewhere: the rail, the rail's foot (Live), or a bench opened
    /// on a contribution. A domain that is not a page is a Lens.
    #[test]
    fn every_page_is_reachable() {
        let mut reached: Vec<Workbench> = Workbench::ALL.to_vec();
        reached.push(Workbench::Live);
        reached.push(bench_page(crate::quartermaster::Craft::Rig));
        reached.push(bench_page(crate::quartermaster::Craft::Conform));
        for page in every_page() {
            match page {
                Workbench::Domain(d) if !reached.contains(&page) => {
                    assert!(Workbench::lenses().contains(&d), "{} is neither a page nor a Lens", d.label());
                }
                _ => assert!(reached.contains(&page), "{} is not reachable", page.label()),
            }
        }
    }

    /// A bench works on a Shipment contribution, so it lights Shipment; every rail page lights itself.
    #[test]
    fn a_bench_lights_shipment() {
        for b in [crate::quartermaster::Craft::Rig, crate::quartermaster::Craft::Conform] {
            assert!(bench_page(b) == Workbench::Mods || bench_page(b) == Workbench::Skeleton);
            assert!(bench_page(b).rail_entry() == Workbench::Quartermaster);
        }
        for w in Workbench::ALL {
            assert!(w.rail_entry() == w, "{} does not light itself", w.label());
        }
    }

    fn row(name: &str) -> AssetRow {
        AssetRow { hash: 0, type_id: 0, block: 0, src: 0, others: Vec::new(), name: Some(name.to_string()), template: None, faction: None }
    }

    /// A Lens pill's count is the number of rows the list shows under it, and the Lenses never
    /// claim a row twice.
    #[test]
    fn a_lens_count_is_what_the_lens_lists() {
        let rows = [
            row("al_veh_helicopter_havoc"),
            row("pmc_hum_mattias"),
            row("al_bld_hq_01"),
            row("us_wpn_rifle_m4"),
        ];
        let mut lensed = 0;
        for lens in std::iter::once(None).chain(Workbench::lenses().into_iter().map(Some)) {
            let n = rows.iter().filter(|r| in_lens(lens, r)).count();
            assert_eq!(n, filter_rows(&rows, Kind::Model, "", lens, &std::collections::HashMap::new()).len());
            if lens.is_some() {
                assert_eq!(n, 1, "{:?} should hold exactly one of the four", lens.map(|d| d.label()));
                lensed += n;
            } else {
                assert_eq!(n, rows.len());
            }
        }
        // The helicopter is Driving's: a rail page, not a Lens.
        assert_eq!(lensed, 3);
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use crate::chains::{resolve, Chain, Step};
    use std::path::Path;

    /// A one-step chain: `set` usable when `ok`, broken otherwise; nothing set when `None`.
    fn chain(set: Option<&str>, ok: bool) -> Chain {
        resolve(vec![Step::Set { label: "saved setting".into(), set: set.map(PathBuf::from) }], |p| {
            if ok {
                Ok(p.to_path_buf())
            } else {
                Err(("FILE MISSING", format!("{} is gone", p.display())))
            }
        })
    }

    #[test]
    fn startup_opens_settings_on_the_first_chain_with_no_answer() {
        assert_eq!(next_step(None, None, &chain(None, false), &chain(Some("/d"), true)), Next::Setup(SettingsSection::Game));
        assert_eq!(next_step(None, None, &chain(Some("/g"), true), &chain(Some("/d"), false)), Next::Setup(SettingsSection::Data));
        assert_eq!(
            next_step(None, None, &chain(Some("/g"), true), &chain(Some("/d"), true)),
            Next::Boot { archive: Some(PathBuf::from("/g")), data: true }
        );
        // No reference data set or found: the boot downloads it.
        assert_eq!(
            next_step(None, None, &chain(Some("/g"), true), &chain(None, false)),
            Next::Boot { archive: Some(PathBuf::from("/g")), data: true }
        );
    }

    #[test]
    fn a_changed_source_reloads_what_it_feeds_and_nothing_else() {
        let (g, d) = (Some(Path::new("/g")), Some(Path::new("/d")));
        assert_eq!(next_step(g, d, &chain(Some("/g"), true), &chain(Some("/d"), true)), Next::Stay);
        assert_eq!(
            next_step(g, d, &chain(Some("/g"), true), &chain(Some("/d2"), true)),
            Next::Boot { archive: None, data: true },
            "a new bundle reloads only the bundle"
        );
        assert_eq!(
            next_step(g, d, &chain(Some("/g2"), true), &chain(Some("/d"), true)),
            Next::Boot { archive: Some(PathBuf::from("/g2")), data: true },
            "a new archive reloads the bundle with it"
        );
        assert_eq!(
            next_step(g, d, &chain(Some("/gone"), false), &chain(Some("/d"), true)),
            Next::Setup(SettingsSection::Game),
            "a source broken under a running archive unloads it into Settings"
        );
    }
}

#[cfg(test)]
mod inspect_tests {
    use super::*;
    use crate::domain::Domain;

    /// A row's menu follows what the row is: a font, a UI movie, a FaceFX asset and the fxdict take
    /// no Shipment route; a string table, an effect and a clip take theirs.
    #[test]
    fn a_rows_subject_follows_its_kind_and_type() {
        use crate::quartermaster::Subject;
        use mercs2_formats::types::*;
        let row = |type_id: u32| crate::index::AssetRow {
            hash: 1,
            type_id,
            block: 0,
            src: 0,
            others: Vec::new(),
            name: Some("x".into()),
            template: None,
            faction: None,
        };
        assert_eq!(subject_of(Kind::Text, &row(TYPE_ID_STRINGDB)), Subject::StringTable);
        assert_eq!(subject_of(Kind::Text, &row(TYPE_ID_FONT)), Subject::Other);
        assert_eq!(subject_of(Kind::Effect, &row(TYPE_ID_EFFECT)), Subject::Effect);
        assert_eq!(subject_of(Kind::Effect, &row(TYPE_ID_FX_DICTIONARY)), Subject::Other);
        assert_eq!(subject_of(Kind::Animation, &row(TYPE_ID_ANIMATION)), Subject::Animation);
        assert_eq!(subject_of(Kind::UiGfx, &row(TYPE_ID_CFX_PACK)), Subject::Other);
        assert_eq!(subject_of(Kind::FaceFx, &row(5)), Subject::Other);
        assert_eq!(subject_of(Kind::Texture, &row(TYPE_ID_TEXTURE)), Subject::Texture);
    }

    #[test]
    fn the_crumb_names_the_tab_the_lens_and_the_loaded_model() {
        assert_eq!(inspect_crumb(Kind::Model, None, None), ["Models"]);
        assert_eq!(inspect_crumb(Kind::Model, Some(Domain::Characters), None), ["Models", "Characters"]);
        assert_eq!(
            inspect_crumb(Kind::Model, Some(Domain::Characters), Some("pmc_hum_mattias")),
            ["Models", "Characters", "pmc_hum_mattias"]
        );
        // A Lens narrows models only, so on another tab it is not part of where you are.
        assert_eq!(inspect_crumb(Kind::Audio, Some(Domain::Characters), None), ["Audio"]);
    }

    /// F4 is "Next clip" in SHORTCUTS and on the verb bar, and was bound to nothing.
    #[test]
    fn f4_steps_the_clip_and_every_listed_key_acts() {
        assert!(matches!(shortcut_act(KeyCode::F4), Some(Act::ClipNav(1))));
        for code in [KeyCode::F3, KeyCode::F11, KeyCode::Space, KeyCode::Comma, KeyCode::Period, KeyCode::Backslash] {
            assert!(shortcut_act(code).is_some(), "{code:?} is listed in SHORTCUTS but does nothing");
        }
    }

    #[test]
    fn the_export_dialog_starts_from_the_asset_and_refuses_a_bundle_of_an_import() {
        let d = ExportDialog::new(1, "pmc_hum_mattias", None, false);
        assert_eq!(d.name, "pmc_hum_mattias");
        assert_eq!(d.format, ExportFormat::Bundle);
        assert_eq!(d.folder, PathBuf::from(EXPORT_ROOT));
        assert!(d.refusal(ExportFormat::Bundle).is_none());
        let i = ExportDialog::new(2, "import_box", None, true);
        assert_eq!(i.format, ExportFormat::Obj);
        assert!(i.refusal(ExportFormat::Bundle).is_some());
        assert!(i.refusal(ExportFormat::Obj).is_none());
    }

    #[test]
    fn an_obj_export_lands_under_the_chosen_folder_and_name() {
        let dir = std::env::temp_dir().join("m2ws_export_obj");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("tri.obj");
        std::fs::write(&src, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let md: ModelData = crate::import::import_model(&src).unwrap().into();
        let out = dir.join("chosen");
        let written = export_model_data(&md, &out, "my box/v2").unwrap();
        let expect = out.join("my_box_v2");
        assert_eq!(PathBuf::from(written), expect);
        assert!(expect.join("model.obj").is_file(), "no model.obj");
        assert!(expect.join("model.mtl").is_file(), "no model.mtl");
    }
}

/// The retail `vz.wad` the game-gated tests read: `vz_wad` in the nearest `.mercs2-local.toml`
/// (`scripts/find-vz-wad.sh --write`). Panics naming what is missing.
#[cfg(all(test, feature = "retail"))]
pub(crate) fn retail_vz_wad() -> String {
    mercs2_formats::game_paths::local_config_vz_wad(std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap_or_else(|e| panic!("{e}"))
        .to_string_lossy()
        .into_owned()
}

#[cfg(all(test, feature = "retail"))]
mod side_wad_retail_tests {
    use super::{AssetIndex, SideWad, WadStack};
    use crate::index::Kind;

    /// Inspect reads the game's other WADs beside the stack: the shell's UI movies and a language
    /// WAD's string table are rows, named by the WAD they come from, and every side WAD of the
    /// retail install opened.
    #[test]
    fn the_catalog_holds_the_shell_movies_and_a_languages_string_table() {
        let w = WadStack::open(&super::retail_vz_wad(), &[]).expect("open retail vz.wad");
        for (file, got) in &w.side_report {
            assert!(!matches!(got, SideWad::Unreadable(_)), "{file} did not open");
        }
        let index = AssetIndex::build(&w.wads, &w.side, std::collections::HashMap::new());
        let is = |src: usize, file: &str| {
            std::path::Path::new(w.source_path(src))
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file))
        };
        assert!(index.rows(Kind::UiGfx).iter().any(|r| is(r.src, "shell.wad")), "no UI movie from shell.wad");
        // `french` ships in shell.wad and French.wad: one row, previewing the first side WAD's copy
        // (shell.wad opens first) and naming French.wad as another source.
        let french = mercs2_formats::hash::pandemic_hash_m2("french");
        let row = index.rows(Kind::Text).iter().find(|r| r.hash == french).expect("no `french` string table");
        assert!(is(row.src, "shell.wad"), "`french` previews from {}", w.source_path(row.src));
        assert!(row.others.iter().any(|&s| is(s, "french.wad")), "`french` does not name French.wad");
    }

    /// A texture vz.wad and shell.wad both ship (`_shell_font_glyphs_f0`, IDENTICAL in
    /// `docs/fixpack/wad_duplicate_inventory.md`) lists both copies, previews vz.wad's, reads the
    /// same from either, and is not marked as differing.
    #[test]
    fn a_texture_in_two_wads_offers_both_copies() {
        let mut w = WadStack::open(&super::retail_vz_wad(), &[]).expect("open retail vz.wad");
        let index = AssetIndex::build(&w.wads, &w.side, std::collections::HashMap::new());
        let hash = mercs2_formats::hash::pandemic_hash_m2("_shell_font_glyphs_f0");
        let row = index.textures().iter().find(|r| r.hash == hash).expect("no _shell_font_glyphs_f0 row").clone();
        assert_eq!(row.src, 0, "previews from {}", w.source_path(row.src));
        let copies = super::texture_copies(&mut w, hash, row.src, &row.others);
        assert!(copies.len() >= 2, "{copies:?}");
        assert!(copies.iter().any(|&(s, _)| w.source_path(s).to_ascii_lowercase().ends_with("shell.wad")));
        assert!(copies.iter().all(|&(_, differs)| !differs), "{copies:?}");
    }
}

#[cfg(all(test, feature = "retail"))]
mod export_retail_tests {
    use super::{export_bundle_by_hash, AssetIndex, WadStack};

    /// The Export dialog's Lossless bundle writes under the folder and name chosen in it.
    #[test]
    fn a_bundle_export_lands_under_the_chosen_folder_and_name() {
        let wad = super::retail_vz_wad();
        let w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let index = AssetIndex::build(&w.wads, &w.side, std::collections::HashMap::new());
        let out = std::env::temp_dir().join("m2ws_export_bundle");
        let _ = std::fs::remove_dir_all(&out);
        let name = "global_weapon_aarocket";
        let hash = mercs2_formats::hash::pandemic_hash_m2(name);
        let written = export_bundle_by_hash(&wad, &[], hash, name, "rocket copy", &index, &out, None)
            .unwrap_or_else(|e| panic!("{e}"));
        let expect = out.join("rocket_copy");
        assert_eq!(std::path::PathBuf::from(written), expect);
        assert!(expect.join("manifest.json").is_file(), "no manifest.json under {}", expect.display());
    }
}

#[cfg(all(test, feature = "retail"))]
mod string_table_retail_tests {
    use super::WadStack;
    use mercs2_formats::stringdb::key_hash;

    /// `english` resolves through the WAD stack and the save's loadout keys read as the game shows
    /// them. Expected text from `mercs2_probe --bin stringdb_dump --filter english` on retail vz.wad.
    #[test]
    fn english_table_names_the_loadout_keys() {
        let wad = super::retail_vz_wad();
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let t = w.string_table("english").expect("english string table");
        assert_eq!(t.len(), 18_299);
        for (key, text) in [
            ("[vehicle.wz10]", "Warsong Attack Helicopter"),
            ("[support.airstrike.bunkerbuster.name]", "Bunker Buster"),
            ("[vehicle.veyronassault]", "The Urban Commando"),
        ] {
            assert_eq!(t.get(&key_hash(key)).map(String::as_str), Some(text), "{key}");
        }
    }

    /// The save's carried-weapon keys name their `worldentity` templates.
    #[test]
    fn worldentity_names_the_carried_weapon_templates() {
        let wad = super::retail_vz_wad();
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let names = w.template_names().expect("worldentity template names");
        for (key, name) in [
            (0x8000_5FD3u32, "C4"),
            (0x8000_6ED0, "Fuel-Air RPG"),
            (0x8000_569E, "Light MG"),
            (0x8000_2331, "Grenade"),
        ] {
            assert_eq!(names.get(&key).map(String::as_str), Some(name), "0x{key:08X}");
        }
    }

    /// The mission, reward and flow scripts come out of the resident and scripts_vz blocks as LuaQ
    /// the game's own VM accepts.
    #[test]
    fn mission_scripts_resolve_as_loadable_bytecode() {
        let wad = super::retail_vz_wad();
        let mut w = WadStack::open(&wad, &[]).expect("open retail vz.wad");
        let lua = mercs2_engine::script::mercs2_luac::rt::Lua::new().unwrap();
        for name in ["wifmissiondata", "wifmissionflow", "mrxrewarddata", "mrxmissionflow"] {
            let code = w.script_bytecode(name).unwrap_or_else(|e| panic!("{e}"));
            assert_eq!(&code[..4], b"\x1bLua", "{name}");
            lua.load(&code).set_name(name).into_function().unwrap_or_else(|e| panic!("{name}: {e}"));
        }
    }
}
