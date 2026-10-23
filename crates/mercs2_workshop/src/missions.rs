//! The Missions page: a Blueprint editor over a mission script's Lua.
//!
//! The graph is the script (see [`crate::blueprint`]): each function is a canvas, its statements
//! nodes on exec wires, its values pins and data nodes. Editing the canvas edits the Lua. Compile
//! emits the script and compiles it with the game's Lua 5.1; Add to Shipment stages the result in
//! the open Shipment as a `replace_lua` contribution.
//!
//! Left: the mission scripts (scripts whose inherit chain reaches the base task), what the open
//! script defines, and the palette — built from the corpus: the base task's lifecycle hooks, the
//! `Event.*` kinds, the objective classes `CreateChild` names, the calls the scripts make (each
//! with a real call site to start from), and Lua's own statements. Centre: the canvas. Right: the
//! selected node's execution, parameters and source. Bottom: compile state, counts and zoom.
//!
//! Canvas text comes from the Workshop type scale times the zoom, like the rest of the drawing:
//! zoomed out, it shrinks below the scale's smallest size rather than disappearing, so a whole
//! contract can be seen at once. The page's own panels keep the scale as it is.

use std::collections::{BTreeMap, HashMap};

use egui::{Color32, Pos2, Rect, Sense, Stroke, TextStyle, Vec2};

use crate::blueprint::{
    self, Category, CompileFailure, CorpusIndex, Graph, NodeId, OutKind, Palette, PaletteEntry, PinTy, Problem, Value,
    ENTRY,
};
use crate::gui::theme;
use crate::lua_source::Script;

// ───────────────────────────────────────────────────────────────────── reference data

/// What the page reads from the corpus, built once on the boot thread.
pub struct Reference {
    pub index: CorpusIndex,
    pub palette: Palette,
}

impl Reference {
    /// Index and lift the whole corpus. Any script that does not parse is an error naming it.
    pub fn build(corpus: &[(String, String, String)]) -> Result<Reference, String> {
        let pairs = || corpus.iter().map(|(p, s, _)| (p.as_str(), s.as_str()));
        let index = CorpusIndex::build(pairs())?;
        let palette = Palette::build(&index, pairs())?;
        Ok(Reference { index, palette })
    }
}

/// The module every mission task inherits from.
const TASK_BASE: &str = "mrxtask";

/// One mission script.
#[derive(Clone, Debug)]
pub struct MissionEntry {
    /// Corpus path (`lua/vz/pmccon016.lua`).
    pub path: String,
    /// File stem (`pmccon016`) — the script's name, and the `replace_lua` target.
    pub name: String,
    /// The class it inherits, as its `inherit()` call writes it.
    pub inherits: String,
}

/// Every corpus script whose inherit chain reaches the base task, by name.
fn build_catalog(index: &CorpusIndex, corpus: &[(String, String, String)]) -> Vec<MissionEntry> {
    let mut out: Vec<MissionEntry> = corpus
        .iter()
        .filter(|(path, _, _)| index.context(path).bases.iter().any(|b| b == TASK_BASE))
        .map(|(path, _, _)| MissionEntry {
            path: path.clone(),
            name: stem(path),
            inherits: index.inherits(path).unwrap_or_default().to_string(),
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    out
}

fn stem(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.strip_suffix(".lua").unwrap_or(file).to_string()
}

// ───────────────────────────────────────────────────────────────────── save overlay

/// What a loaded `.profile` says about mission progress, for the script list's badges.
#[derive(Debug, Clone, Default)]
pub struct SaveOverlay {
    pub source: String,
    /// Lower-case mission id → nState.
    pub active: BTreeMap<String, f64>,
    /// Lower-case mission id → flow flag.
    pub completed: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissionState {
    None,
    Offered,
    InProgress,
    Completed,
}

impl SaveOverlay {
    /// The state a save gives the mission named `name` (the script's file stem; save ids differ
    /// only in case).
    pub fn state_for(&self, name: &str) -> MissionState {
        let key = name.to_ascii_lowercase();
        if self.completed.get(&key).is_some_and(|v| *v != 0.0) {
            MissionState::Completed
        } else if let Some(state) = self.active.get(&key) {
            if *state == 1.0 {
                MissionState::InProgress
            } else {
                MissionState::Offered
            }
        } else {
            MissionState::None
        }
    }
}

// ───────────────────────────────────────────────────────────────────── panel state

/// A request for the host: stage `source` in the open Shipment as `replace_lua` of `target`.
pub struct ShipmentRequest {
    pub target: String,
    pub source: String,
}

pub struct Panel {
    reference: Option<Reference>,
    catalog: Vec<MissionEntry>,
    filter: String,
    open: Option<Open>,
    save: Option<SaveOverlay>,
    /// The last thing that went wrong or was done, for the footer.
    status: Option<(bool, String)>,
    /// Set by `apply` when Jump to source should fire; drained by the host.
    pub pending_lua_open: Option<(String, u32)>,
    /// Set by `apply` when the user asks to pick a save file; drained by the host.
    pub pending_save_open: bool,
    /// Set by `apply` on Add to Shipment; drained by the host, which owns the Quartermaster.
    pub pending_shipment: Option<ShipmentRequest>,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            reference: None,
            catalog: Vec::new(),
            filter: String::new(),
            open: None,
            save: None,
            status: None,
            pending_lua_open: None,
            pending_save_open: false,
            pending_shipment: None,
        }
    }
}

/// The script being edited.
struct Open {
    entry: MissionEntry,
    script: Script,
    chunk: Graph,
    /// The canvas shown: `function` nodes from the chunk down.
    path: Vec<NodeId>,
    views: HashMap<Vec<NodeId>, View>,
    selection: Vec<NodeId>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    edited: bool,
    check: Check,
    /// Copied statements, as Lua.
    clipboard: Vec<String>,
    adding: Option<Adding>,
}

#[derive(Clone)]
struct Snapshot {
    chunk: Graph,
    views: HashMap<Vec<NodeId>, View>,
}

#[derive(Clone, Debug)]
struct View {
    pan: Vec2,
    zoom: f32,
    /// Node positions in canvas units (zoom 1).
    pos: HashMap<NodeId, Pos2>,
}

impl Default for View {
    fn default() -> Self {
        View {
            pan: Vec2::ZERO,
            zoom: 1.0,
            pos: HashMap::new(),
        }
    }
}

/// What the script's last check found.
#[derive(Clone, Debug)]
enum Check {
    /// Emits, not compiled since the last edit.
    Emits,
    Problems(Vec<(Vec<NodeId>, Problem)>),
    Compiled(usize),
    Refused(String),
}

/// The add-node palette, open at a point on the canvas.
#[derive(Clone, Debug)]
struct Adding {
    /// Where the node lands, in canvas units.
    at: Pos2,
    /// Where the palette opens on screen.
    screen: Pos2,
    query: String,
    /// The pin a wire was dragged from, which the new node is wired to.
    from: Option<PinRef>,
}

/// A pin on the current canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PinRef {
    ExecIn(NodeId),
    ExecOut(NodeId, usize),
    In(NodeId, usize),
    Out(NodeId, usize),
}

const UNDO_DEPTH: usize = 50;

impl Open {
    fn canvas(&self) -> Option<&Graph> {
        canvas_at(&self.chunk, &self.path)
    }

    fn canvas_mut(&mut self) -> Option<&mut Graph> {
        let mut g = &mut self.chunk;
        for &id in &self.path {
            g = g.nodes.get_mut(id)?.sub.as_deref_mut()?;
        }
        Some(g)
    }

    fn view(&self) -> View {
        self.views.get(&self.path).cloned().unwrap_or_default()
    }

    fn view_mut(&mut self) -> &mut View {
        self.views.entry(self.path.clone()).or_default()
    }

    fn snapshot(&mut self) {
        self.undo.push(Snapshot {
            chunk: self.chunk.clone(),
            views: self.views.clone(),
        });
        if self.undo.len() > UNDO_DEPTH {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Re-emit after an edit: the footer shows whether the script still stands.
    fn recheck(&mut self) {
        self.edited = true;
        self.check = match blueprint::emit(&self.script, &self.chunk) {
            Ok(_) => Check::Emits,
            Err(_) => Check::Problems(problems_everywhere(&self.chunk, &[])),
        };
    }

    /// Keep the path and the selection valid after the chunk changed under them.
    fn settle(&mut self) {
        while canvas_at(&self.chunk, &self.path).is_none() {
            self.path.pop();
        }
        let n = self.canvas().map(|g| g.nodes.len()).unwrap_or(0);
        self.selection.retain(|&i| i < n);
    }
}

fn canvas_at<'g>(chunk: &'g Graph, path: &[NodeId]) -> Option<&'g Graph> {
    let mut g = chunk;
    for &id in path {
        g = g.nodes.get(id)?.sub.as_deref()?;
    }
    Some(g)
}

/// Every problem in every canvas, with the canvas it is in.
fn problems_everywhere(g: &Graph, path: &[NodeId]) -> Vec<(Vec<NodeId>, Problem)> {
    let mut out = Vec::new();
    for (i, n) in g.nodes.iter().enumerate() {
        if let Some(sub) = &n.sub {
            let mut p = path.to_vec();
            p.push(i);
            out.extend(problems_everywhere(sub, &p));
        }
    }
    if out.is_empty() {
        if let Err(ps) = g.emit_body() {
            out.extend(ps.into_iter().map(|p| (path.to_vec(), p)));
        }
    }
    out
}

impl Panel {
    /// Take the boot thread's reference data and build the script list from it.
    pub fn set_reference(&mut self, reference: Reference, corpus: &[(String, String, String)]) {
        self.catalog = build_catalog(&reference.index, corpus);
        self.reference = Some(reference);
    }

    /// Decode a `.profile` and keep what it says about mission progress.
    pub fn load_save(&mut self, path: &std::path::Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let profile = mercs2_formats::save::parse(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))?;
        let state = profile.save_state()?;
        let active: BTreeMap<String, f64> =
            state.active_missions.iter().map(|m| (m.id.to_ascii_lowercase(), m.state)).collect();
        let completed: BTreeMap<String, f64> =
            state.completed_flow.iter().map(|(k, v)| (k.to_ascii_lowercase(), *v)).collect();
        self.save = Some(SaveOverlay {
            source: path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string(),
            active,
            completed,
        });
        Ok(())
    }

    fn fail(&mut self, msg: impl Into<String>) {
        self.status = Some((false, msg.into()));
    }

    fn note(&mut self, msg: impl Into<String>) {
        self.status = Some((true, msg.into()));
    }
}

// ───────────────────────────────────────────────────────────────────── acts

pub enum Act {
    Pick(usize),
    Filter(String),
    OpenCanvas(Vec<NodeId>),
    /// Select nodes; `add` toggles them into what was selected.
    Select(Vec<NodeId>, bool),
    /// Open the canvas holding a node and select it.
    Reveal(Vec<NodeId>, NodeId),
    SelectAll,
    Pan(Vec2),
    /// Zoom by `factor` keeping the canvas point under `at` (relative to the canvas corner) still.
    ZoomAt { factor: f32, at: Vec2 },
    SetView { pan: Vec2, zoom: f32 },
    /// Positions for nodes that had none, from the canvas's layout.
    Place(HashMap<NodeId, Pos2>),
    Move(Vec<NodeId>, Vec2),
    SetValue { node: NodeId, pin: usize, lua: String },
    ConnectData { from: (NodeId, usize), to: (NodeId, usize) },
    BreakData((NodeId, usize)),
    ConnectExec { from: (NodeId, usize), to: NodeId },
    BreakExec((NodeId, usize)),
    BreakExecInto(NodeId),
    BeginAdd { at: Pos2, screen: Pos2, from: Option<PinRef> },
    AddQuery(String),
    CancelAdd,
    Add(PaletteEntry),
    Delete(Vec<NodeId>),
    Clone(Vec<NodeId>),
    Copy(Vec<NodeId>),
    Paste(Pos2),
    Undo,
    Redo,
    Revert,
    Compile,
    AddToShipment,
    /// Open the Lua viewer at a node's line, or the canvas function's with `None`.
    JumpToSource(Option<NodeId>),
    OpenSavePicker,
    ClearSave,
}

pub fn apply(act: Act, p: &mut Panel, corpus: &[(String, String, String)]) {
    match act {
        Act::Pick(i) => pick(p, i, corpus),
        Act::Filter(f) => p.filter = f,
        Act::OpenSavePicker => p.pending_save_open = true,
        Act::ClearSave => p.save = None,
        other => {
            let Some(o) = p.open.as_mut() else { return };
            let compiling = matches!(other, Act::Compile | Act::AddToShipment);
            match edit(o, other, p.reference.as_ref(), &mut p.pending_lua_open, &mut p.pending_shipment) {
                Err(e) => p.fail(e),
                Ok(()) if compiling => {
                    if let Check::Compiled(bytes) = o.check {
                        let msg = format!("{}.lua compiled — {bytes} bytes of bytecode", o.entry.name);
                        p.note(msg);
                    }
                }
                Ok(()) => {}
            }
        }
    }
}

fn pick(p: &mut Panel, i: usize, corpus: &[(String, String, String)]) {
    let Some(entry) = p.catalog.get(i).cloned() else { return };
    let Some(reference) = p.reference.as_ref() else { return };
    let Some((_, src, _)) = corpus.iter().find(|(path, _, _)| *path == entry.path) else {
        p.fail(format!("{} is not in the Lua corpus", entry.path));
        return;
    };
    let script = match Script::parse(src) {
        Ok(s) => s,
        Err(e) => {
            p.fail(format!("{}: {e}", entry.path));
            return;
        }
    };
    let chunk = blueprint::lift(&script, &reference.index.context(&entry.path));
    // Open on the script's first override (a hook the game calls), else its main chunk.
    let path = chunk
        .nodes
        .iter()
        .position(|n| n.is_function() && n.category == Category::Lifecycle)
        .map(|i| vec![i])
        .unwrap_or_default();
    p.status = script.uses_goto().then(|| {
        (
            false,
            format!(
                "{} has goto/::label:: (unluac's output for a debug-stripped chunk): Lua 5.1 refuses \
                 them, so it will not compile until those jumps are rewritten",
                entry.name
            ),
        )
    });
    p.open = Some(Open {
        entry,
        script,
        chunk,
        path,
        views: HashMap::new(),
        selection: Vec::new(),
        undo: Vec::new(),
        redo: Vec::new(),
        edited: false,
        check: Check::Emits,
        clipboard: Vec::new(),
        adding: None,
    });
}

/// Carry out an act on the open script.
fn edit(
    o: &mut Open,
    act: Act,
    reference: Option<&Reference>,
    lua_open: &mut Option<(String, u32)>,
    shipment: &mut Option<ShipmentRequest>,
) -> Result<(), String> {
    match act {
        Act::OpenCanvas(path) => {
            if canvas_at(&o.chunk, &path).is_none() {
                return Err("that function is no longer in the script".into());
            }
            o.path = path;
            o.selection.clear();
            o.adding = None;
        }
        Act::Select(ids, add) => {
            if !add {
                o.selection.clear();
            }
            for id in ids {
                match o.selection.iter().position(|&s| s == id) {
                    Some(at) if add => {
                        o.selection.remove(at);
                    }
                    Some(_) => {}
                    None => o.selection.push(id),
                }
            }
        }
        Act::Reveal(path, id) => {
            o.path = path;
            o.selection = vec![id];
            o.adding = None;
        }
        Act::SelectAll => {
            o.selection = (0..o.canvas().map(|g| g.nodes.len()).unwrap_or(0)).filter(|&i| i != ENTRY).collect();
        }
        Act::Pan(d) => o.view_mut().pan += d,
        Act::ZoomAt { factor, at } => {
            let v = o.view_mut();
            let zoom = (v.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            let point = (at - v.pan) / v.zoom;
            v.pan = at - point * zoom;
            v.zoom = zoom;
        }
        Act::SetView { pan, zoom } => {
            let v = o.view_mut();
            v.pan = pan;
            v.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        }
        Act::Place(pos) => o.view_mut().pos.extend(pos),
        Act::Move(ids, d) => {
            let v = o.view_mut();
            for id in ids {
                if let Some(p) = v.pos.get_mut(&id) {
                    *p += d;
                }
            }
        }
        Act::SetValue { node, pin, lua } => change(o, |g| g.set_value(node, pin, &lua))?,
        Act::ConnectData { from, to } => change(o, |g| g.connect_data(from, to))?,
        Act::BreakData(to) => change(o, |g| {
            g.break_data(to);
            Ok(())
        })?,
        Act::ConnectExec { from, to } => change(o, |g| g.connect_exec(from, to))?,
        Act::BreakExec(from) => change(o, |g| {
            g.break_exec(from);
            Ok(())
        })?,
        Act::BreakExecInto(to) => change(o, |g| {
            let from: Vec<(NodeId, usize)> = g.exec.iter().filter(|w| w.to == to).map(|w| w.from).collect();
            for f in from {
                g.break_exec(f);
            }
            Ok(())
        })?,
        Act::BeginAdd { at, screen, from } => {
            o.adding = Some(Adding {
                at,
                screen,
                query: String::new(),
                from,
            })
        }
        Act::AddQuery(q) => {
            if let Some(a) = o.adding.as_mut() {
                a.query = q;
            }
        }
        Act::CancelAdd => o.adding = None,
        Act::Add(entry) => {
            let reference = reference.ok_or("the reference corpus is not loaded")?;
            let ctx = reference.index.context(&o.entry.path);
            let mut adding = o.adding.take();
            if entry.top_level && !o.path.is_empty() {
                // A function definition goes at the top of the script, where the main chunk shows it.
                o.path.clear();
                adding = None;
            }
            let at = adding.as_ref().map(|a| a.at).unwrap_or_else(|| free_spot(&o.view()));
            o.snapshot();
            let g = o.canvas_mut().ok_or("no canvas")?;
            let id = g.add_statement(&entry.lua, &ctx)?;
            let wired = match adding.and_then(|a| a.from) {
                Some(from) => attach(g, id, from),
                None => Ok(()),
            };
            o.view_mut().pos.insert(id, at);
            o.selection = vec![id];
            o.recheck();
            wired?;
        }
        Act::Delete(ids) => {
            let ids: Vec<NodeId> = ids.into_iter().filter(|&i| i != ENTRY).collect();
            if ids.is_empty() {
                return Ok(());
            }
            o.snapshot();
            let g = o.canvas_mut().ok_or("no canvas")?;
            let map = g.delete(&ids);
            let v = o.view_mut();
            v.pos = v.pos.iter().filter_map(|(&k, &p)| Some((map.get(k).copied().flatten()?, p))).collect();
            o.selection.clear();
            o.recheck();
        }
        Act::Clone(ids) => {
            let ids: Vec<NodeId> = ids.into_iter().filter(|&i| i != ENTRY).collect();
            if ids.is_empty() {
                return Ok(());
            }
            o.snapshot();
            let g = o.canvas_mut().ok_or("no canvas")?;
            let copies = g.clone_nodes(&ids);
            // Each copy sits below and right of its original; what came with it (its values, its
            // blocks) is laid out around it when the canvas next draws.
            let v = o.view_mut();
            for (original, &copy) in ids.iter().zip(&copies) {
                if let Some(at) = v.pos.get(original).copied() {
                    v.pos.insert(copy, at + Vec2::new(40.0, 40.0));
                }
            }
            o.selection = copies;
            o.recheck();
        }
        Act::Copy(ids) => {
            let g = o.canvas().ok_or("no canvas")?;
            o.clipboard = ids
                .iter()
                .filter(|&&i| i != ENTRY && g.nodes.get(i).is_some_and(|n| n.exec_in))
                .map(|&i| g.node_source(i))
                .collect::<Result<_, _>>()?;
        }
        Act::Paste(at) => {
            if o.clipboard.is_empty() {
                return Ok(());
            }
            let reference = reference.ok_or("the reference corpus is not loaded")?;
            let ctx = reference.index.context(&o.entry.path);
            o.snapshot();
            let lua = o.clipboard.clone();
            let g = o.canvas_mut().ok_or("no canvas")?;
            let mut ids = Vec::new();
            for l in &lua {
                ids.push(g.add_statement(l, &ctx)?);
            }
            // Pasted statements chain in the order they were copied.
            for w in ids.windows(2) {
                if let Some(next) = g.nodes[w[0]].next {
                    g.connect_exec((w[0], next), w[1])?;
                }
            }
            let v = o.view_mut();
            for (k, &id) in ids.iter().enumerate() {
                v.pos.insert(id, at + Vec2::new(0.0, 120.0 * k as f32));
            }
            o.selection = ids;
            o.recheck();
        }
        Act::Undo => {
            let snap = o.undo.pop().ok_or("nothing to undo")?;
            o.redo.push(Snapshot {
                chunk: std::mem::replace(&mut o.chunk, snap.chunk),
                views: std::mem::replace(&mut o.views, snap.views),
            });
            o.settle();
            o.recheck();
        }
        Act::Redo => {
            let snap = o.redo.pop().ok_or("nothing to redo")?;
            o.undo.push(Snapshot {
                chunk: std::mem::replace(&mut o.chunk, snap.chunk),
                views: std::mem::replace(&mut o.views, snap.views),
            });
            o.settle();
            o.recheck();
        }
        Act::Revert => {
            let reference = reference.ok_or("the reference corpus is not loaded")?;
            o.snapshot();
            o.chunk = blueprint::lift(&o.script, &reference.index.context(&o.entry.path));
            o.views.clear();
            o.settle();
            o.edited = false;
            o.check = Check::Emits;
        }
        Act::Compile => compile(o),
        Act::AddToShipment => {
            compile(o);
            match &o.check {
                Check::Compiled(_) => {
                    let source = blueprint::emit(&o.script, &o.chunk).map_err(|ps| ps[0].message.clone())?;
                    *shipment = Some(ShipmentRequest {
                        target: o.entry.name.clone(),
                        source,
                    });
                }
                Check::Refused(e) => return Err(format!("not added: the game's Lua refuses it — {e}")),
                Check::Problems(ps) => return Err(format!("not added: {}", ps[0].1.message)),
                Check::Emits => unreachable!("compile always concludes"),
            }
        }
        Act::JumpToSource(node) => {
            let line = match node {
                Some(id) => o.canvas().and_then(|g| g.nodes.get(id)).filter(|n| !n.is_new()).map(|n| n.line),
                None => match o.path.split_last() {
                    Some((&f, parent)) => canvas_at(&o.chunk, parent).and_then(|g| g.nodes.get(f)).map(|n| n.line),
                    None => Some(1),
                },
            };
            match line {
                Some(l) if l > 0 => *lua_open = Some((o.entry.path.clone(), l)),
                _ => return Err("a node made in the editor has no source line yet".into()),
            }
        }
        Act::Pick(_) | Act::Filter(_) | Act::OpenSavePicker | Act::ClearSave => {}
    }
    Ok(())
}

/// Change the current canvas with an undo point, and re-check the script.
fn change(o: &mut Open, f: impl FnOnce(&mut Graph) -> Result<(), String>) -> Result<(), String> {
    let before = Snapshot {
        chunk: o.chunk.clone(),
        views: o.views.clone(),
    };
    let g = o.canvas_mut().ok_or("no canvas")?;
    f(g)?;
    o.undo.push(before);
    if o.undo.len() > UNDO_DEPTH {
        o.undo.remove(0);
    }
    o.redo.clear();
    o.recheck();
    Ok(())
}

/// Wire a new statement to the pin a wire was dragged from: after an exec output (into the
/// middle of a chain, if it was on one), before an exec input, or from a value into its first
/// pin that takes it.
fn attach(g: &mut Graph, id: NodeId, from: PinRef) -> Result<(), String> {
    let next = g.nodes[id].next;
    match from {
        PinRef::ExecOut(n, o) => {
            let after = g.exec_target(n, o);
            g.connect_exec((n, o), id)?;
            if let (Some(after), Some(next)) = (after, next) {
                g.connect_exec((id, next), after)?;
            }
        }
        PinRef::ExecIn(n) => {
            let before = g.exec.iter().find(|w| w.to == n).map(|w| w.from);
            let next = next.ok_or("that statement ends its block, so nothing can follow it")?;
            if let Some(before) = before {
                g.connect_exec(before, id)?;
            }
            g.connect_exec((id, next), n)?;
        }
        PinRef::Out(n, k) => {
            let ty = g.nodes[n].outputs[k].ty;
            let pin = (0..g.nodes[id].inputs.len())
                .find(|&i| g.feeder(id, i).is_none() && blueprint::compatible(ty, g.nodes[id].inputs[i].ty))
                .ok_or("the new node has no pin that takes this value")?;
            g.connect_data((n, k), (id, pin))?;
        }
        PinRef::In(..) => {}
    }
    Ok(())
}

fn compile(o: &mut Open) {
    o.check = match blueprint::compile(&o.script, &o.chunk, &o.entry.name) {
        Ok((_, bytecode)) => Check::Compiled(bytecode.len()),
        Err(CompileFailure::Lua(e)) => Check::Refused(e),
        Err(CompileFailure::Graph(_)) => Check::Problems(problems_everywhere(&o.chunk, &[])),
    };
}

/// Somewhere below everything on a canvas, for a node added without a cursor.
fn free_spot(v: &View) -> Pos2 {
    let bottom = v.pos.values().map(|p| p.y).fold(0.0_f32, f32::max);
    Pos2::new(0.0, bottom + 160.0)
}

// ───────────────────────────────────────────────────────────────────── looks

const CANVAS_BG: Color32 = Color32::from_rgb(0x0c, 0x10, 0x15);
const DOT: Color32 = Color32::from_rgba_premultiplied(17, 20, 23, 36);
const NODE_TOP: Color32 = Color32::from_rgb(0x1c, 0x23, 0x2e);
const NODE_BOTTOM: Color32 = Color32::from_rgb(0x0f, 0x13, 0x1a);
const NODE_BORDER: Color32 = Color32::from_rgb(0x28, 0x30, 0x3d);
const SELECT: Color32 = Color32::from_rgb(0x1e, 0x9b, 0xff);
const LABEL: Color32 = Color32::from_rgb(0xaa, 0xb4, 0xc2);
const LABEL_STRONG: Color32 = Color32::from_rgb(0xe7, 0xec, 0xf3);
const ANNOT: Color32 = Color32::from_rgb(0x6f, 0x7d, 0x8e);
const OVERLAY_BG: Color32 = Color32::from_rgba_premultiplied(15, 18, 24, 214);
const BAD: Color32 = Color32::from_rgb(0xef, 0x50, 0x64);
const GOOD: Color32 = Color32::from_rgb(0x37, 0xd6, 0x7a);
const WARN: Color32 = Color32::from_rgb(0xf0, 0xa9, 0x2b);

fn pin_colour(t: PinTy) -> Color32 {
    match t {
        PinTy::Exec => Color32::from_rgb(0xf4, 0xf6, 0xfa),
        PinTy::Str => Color32::from_rgb(0xec, 0x48, 0x99),
        PinTy::Num => Color32::from_rgb(0x10, 0xb9, 0x81),
        PinTy::Bool => Color32::from_rgb(0xe0, 0x5a, 0x4f),
        PinTy::Obj => Color32::from_rgb(0x8b, 0x5c, 0xf6),
        PinTy::Arr => Color32::from_rgb(0xf9, 0x73, 0x16),
        PinTy::Any => Color32::from_rgb(0x8b, 0x97, 0xa6),
    }
}

/// A category's header: gradient top and bottom, title colour, icon colour, eyebrow.
struct Look {
    top: Color32,
    bottom: Color32,
    text: Color32,
    icon: Color32,
    eyebrow: &'static str,
}

fn look(c: Category) -> Look {
    let l = |top: u32, bottom: u32, text: u32, icon: u32, eyebrow| Look {
        top: rgb(top),
        bottom: rgb(bottom),
        text: rgb(text),
        icon: rgb(icon),
        eyebrow,
    };
    match c {
        Category::Chunk => l(0x4a3a7a, 0x2f2650, 0xded2ff, 0xc3b1fb, "script"),
        Category::Lifecycle => l(0x4a3a7a, 0x2f2650, 0xded2ff, 0xc3b1fb, "lifecycle"),
        Category::Base => l(0x4a3a7a, 0x2f2650, 0xded2ff, 0xc3b1fb, "base"),
        Category::Fn => l(0x1b4b6a, 0x113148, 0xc9e4f7, 0x9ec8ea, "fn"),
        Category::Vo => l(0x7a2f57, 0x4f1f39, 0xffd9ee, 0xf7b3d6, "vo"),
        Category::Objective => l(0x6a4f14, 0x493611, 0xf8e8bd, 0xf4d67a, "objective"),
        Category::Event => l(0x7a2f36, 0x4f1f26, 0xffdada, 0xf5a3a3, "event"),
        Category::Music => l(0x1e5555, 0x113636, 0xa9ede1, 0x74ddc8, "music"),
        Category::World => l(0x3b2f6a, 0x251d45, 0xd9d0ff, 0x8b5cf6, "world"),
        Category::State => l(0x3a4250, 0x232933, 0xe7ecf3, 0xf4f6fa, "state"),
        Category::Tutorial => l(0x5c4413, 0x3d2d0d, 0xf8e8bd, 0xf0a92b, "tutorial"),
        Category::Util => l(0x24445c, 0x142a3a, 0xbed6e8, 0x9ec8ea, "util"),
        Category::Faction => l(0x6a2430, 0x45161f, 0xffd6dc, 0xef5064, "faction"),
        Category::Economy => l(0x14533f, 0x0d3628, 0xbff3df, 0x10b981, "economy"),
        Category::Achievement => l(0x5f5320, 0x3e3614, 0xfbefc4, 0xf6d98a, "achievement"),
        Category::Call => l(0x2f333b, 0x1c1f25, 0xdcdfe4, 0xaab4c2, "call"),
        Category::Flow => l(0x33404f, 0x1f2833, 0xdfe7f1, 0xaab4c2, "flow"),
        Category::Variable => l(0x2e4038, 0x1b2722, 0xd6efe3, 0x74ddc8, "var"),
        Category::Data => l(0x283039, 0x181d23, 0xcbd3dd, 0x8b97a6, "value"),
        Category::Lua => l(0x4a2626, 0x2e1717, 0xf2d2d2, 0xe8763a, "lua"),
    }
}

fn rgb(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// A category's icon, drawn into `r`.
fn glyph(p: &egui::Painter, r: Rect, c: Category, col: Color32) {
    let s = Stroke::new((r.width() * 0.11).max(1.0), col);
    let at = |x: f32, y: f32| Pos2::new(r.min.x + r.width() * x, r.min.y + r.height() * y);
    let poly = |pts: &[Pos2], closed: bool| {
        for w in pts.windows(2) {
            p.line_segment([w[0], w[1]], s);
        }
        if closed {
            p.line_segment([pts[pts.len() - 1], pts[0]], s);
        }
    };
    match c {
        Category::Chunk => poly(&[at(0.3, 0.15), at(0.6, 0.15), at(0.75, 0.35), at(0.75, 0.85), at(0.3, 0.85)], true),
        Category::Lifecycle => {
            poly(&[at(0.58, 0.1), at(0.25, 0.55), at(0.5, 0.55), at(0.42, 0.9), at(0.75, 0.45), at(0.5, 0.45)], true)
        }
        Category::Base => {
            for (y, end) in [(0.3, 0.8), (0.5, 0.8), (0.7, 0.6)] {
                p.line_segment([at(0.2, y), at(end, y)], s);
            }
        }
        Category::Fn | Category::Util => {
            poly(&[at(0.2, 0.38), at(0.5, 0.23), at(0.8, 0.38), at(0.5, 0.53)], true);
            poly(&[at(0.2, 0.62), at(0.5, 0.77), at(0.8, 0.62)], false);
        }
        Category::Vo => {
            poly(&[at(0.2, 0.4), at(0.35, 0.4), at(0.55, 0.2), at(0.55, 0.8), at(0.35, 0.6), at(0.2, 0.6)], true);
            poly(&[at(0.7, 0.35), at(0.78, 0.5), at(0.7, 0.65)], false);
        }
        Category::Objective => {
            p.circle_stroke(r.center(), r.width() * 0.33, s);
            p.circle_stroke(r.center(), r.width() * 0.13, s);
        }
        Category::Event => {
            p.circle_stroke(r.center(), r.width() * 0.33, s);
            p.line_segment([r.center(), at(0.5, 0.3)], s);
            p.line_segment([r.center(), at(0.65, 0.6)], s);
        }
        Category::Music => {
            p.circle_stroke(at(0.35, 0.72), r.width() * 0.1, s);
            p.circle_stroke(at(0.7, 0.65), r.width() * 0.1, s);
            poly(&[at(0.45, 0.72), at(0.45, 0.25), at(0.8, 0.18), at(0.8, 0.65)], false);
        }
        Category::World => {
            p.circle_stroke(r.center(), r.width() * 0.33, s);
            p.line_segment([at(0.17, 0.5), at(0.83, 0.5)], s);
            p.line_segment([at(0.5, 0.17), at(0.5, 0.83)], s);
        }
        Category::Flow => {
            p.line_segment([at(0.3, 0.2), at(0.3, 0.8)], s);
            p.line_segment([at(0.3, 0.45), at(0.75, 0.25)], s);
            p.line_segment([at(0.3, 0.45), at(0.75, 0.7)], s);
        }
        Category::Variable => {
            p.rect_stroke(Rect::from_min_max(at(0.22, 0.3), at(0.78, 0.7)), 1.0, s);
        }
        Category::Lua => {
            poly(&[at(0.35, 0.25), at(0.18, 0.5), at(0.35, 0.75)], false);
            poly(&[at(0.65, 0.25), at(0.82, 0.5), at(0.65, 0.75)], false);
        }
        _ => {
            p.circle_filled(r.center(), r.width() * 0.16, col);
        }
    }
}

// ───────────────────────────────────────────────────────────────────── node geometry

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 2.0;
const HEADER_H: f32 = 34.0;
const ROW_H: f32 = 26.0;
const PAD_Y: f32 = 8.0;
const PAD_X: f32 = 11.0;
const ICON: f32 = 18.0;
const PIN_R: f32 = 5.0;
const ROUND: f32 = 10.0;
const GRID: f32 = 26.0;
const GAP_X: f32 = 56.0;
const GAP_Y: f32 = 36.0;
const INDENT_X: f32 = 48.0;
/// Screen pixels kept clear around a fitted canvas.
const FIT_MARGIN: f32 = 48.0;
/// Longest literal shown beside a pin before it is cut.
const ANNOT_CHARS: usize = 30;

/// `style` at `zoom`, in half-pixel steps: each size egui draws text at is rasterised into its
/// glyph texture, so a smooth zoom must not ask for a new size every frame.
fn scaled(style: &TextStyle, zoom: f32) -> egui::FontId {
    let mut f = theme::font(style);
    f.size = ((f.size * zoom * 2.0).round() / 2.0).max(0.5);
    f
}

fn exec_label(n: &blueprint::Node, i: usize) -> &str {
    match n.exec_outs[i].as_str() {
        "next" => "Exec",
        "body" if !n.exec_in => "Body",
        other => other,
    }
}

/// Short text for what a pin holds, shown beside it.
fn annot(v: &Value) -> Option<String> {
    let text = match v {
        Value::Literal(e) => blueprint::bare(e),
        Value::Path(p) => blueprint::bare(p),
        Value::Array(t) => format!("[{}]", t.fields().len()),
        Value::Lua(_) => "lua".to_string(),
        Value::Wired { .. } => return None,
    };
    Some(cut(&text, ANNOT_CHARS))
}

fn cut(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

/// The rows down a node's left and right sides.
fn rows(n: &blueprint::Node) -> (usize, usize) {
    (n.exec_in as usize + n.inputs.len(), n.exec_outs.len() + n.outputs.len())
}

/// A node's size in canvas units, from its text measured in the type scale.
fn measure(ui: &egui::Ui, n: &blueprint::Node) -> Vec2 {
    let w = |s: &str, style: &TextStyle| {
        ui.fonts(|f| f.layout_no_wrap(s.to_string(), theme::font(style), Color32::WHITE).size().x)
    };
    let lk = look(n.category);
    let title = PAD_X
        + ICON
        + 8.0
        + w(&n.title, &TextStyle::Body)
        + 16.0
        + w(&lk.eyebrow.to_uppercase(), &theme::style_eyebrow())
        + PAD_X;
    let mut left: f32 = 0.0;
    if n.exec_in {
        left = left.max(w("Exec", &TextStyle::Small));
    }
    for i in &n.inputs {
        let a = annot(&i.value).map(|a| 6.0 + w(&a, &TextStyle::Monospace)).unwrap_or(0.0);
        left = left.max(w(&i.label, &TextStyle::Small) + a);
    }
    let mut right: f32 = 0.0;
    for k in 0..n.exec_outs.len() {
        right = right.max(w(exec_label(n, k), &TextStyle::Small));
    }
    for o in &n.outputs {
        right = right.max(w(&o.label, &TextStyle::Small));
    }
    let side = PAD_X + PIN_R * 2.0 + 8.0;
    let body = side + left + 24.0 + right + side;
    let (l, r) = rows(n);
    Vec2::new(title.max(body).max(140.0), HEADER_H + PAD_Y * 2.0 + ROW_H * l.max(r).max(1) as f32)
}

/// Where row `i` of a side sits, in canvas units from the node's corner.
fn row_y(i: usize) -> f32 {
    HEADER_H + PAD_Y + ROW_H * (i as f32 + 0.5)
}

/// A pin's centre in canvas units.
fn pin_at(n: &blueprint::Node, pos: Pos2, size: Vec2, pin: PinRef) -> Pos2 {
    match pin {
        PinRef::ExecIn(_) => pos + Vec2::new(0.0, row_y(0)),
        PinRef::In(_, i) => pos + Vec2::new(0.0, row_y(n.exec_in as usize + i)),
        PinRef::ExecOut(_, k) => pos + Vec2::new(size.x, row_y(k)),
        PinRef::Out(_, k) => pos + Vec2::new(size.x, row_y(n.exec_outs.len() + k)),
    }
}

fn pins_of(id: NodeId, n: &blueprint::Node) -> Vec<PinRef> {
    let mut v = Vec::new();
    if n.exec_in {
        v.push(PinRef::ExecIn(id));
    }
    v.extend((0..n.inputs.len()).map(|i| PinRef::In(id, i)));
    v.extend((0..n.exec_outs.len()).map(|k| PinRef::ExecOut(id, k)));
    v.extend((0..n.outputs.len()).map(|k| PinRef::Out(id, k)));
    v
}

// ───────────────────────────────────────────────────────────────────── layout

/// The width a canvas's rows wrap at: the width of a rectangle of the canvas's shape whose area is
/// every node's footprint with its gaps, and never narrower than what the canvas shows at 1:1 or
/// than the widest node — so a function that fits stays in one row, and a long one comes out the
/// canvas's shape rather than one long strip.
fn wrap_width(sizes: &[Vec2], canvas: Vec2) -> f32 {
    let aspect = canvas.x / canvas.y.max(1.0);
    let area: f32 = sizes.iter().map(|s| (s.x + GAP_X) * (s.y + GAP_Y)).sum();
    let widest = sizes.iter().map(|s| s.x).fold(0.0, f32::max);
    (area * aspect.max(0.1)).sqrt().max(canvas.x - FIT_MARGIN * 2.0).max(widest)
}

/// Positions for a canvas: each exec chain runs left to right, a statement's values stack under
/// it, and the blocks off a statement (a branch's arms, a loop's body, a callback) chain below
/// the row they hang from. A chain row that would pass [`wrap_width`] for the `canvas` (its size
/// on screen) ends, and the chain carries on in a new row under that row's blocks, back at the
/// row's start.
fn layout(g: &Graph, sizes: &[Vec2], canvas: Vec2) -> HashMap<NodeId, Pos2> {
    struct L<'a> {
        g: &'a Graph,
        sizes: &'a [Vec2],
        pos: HashMap<NodeId, Pos2>,
        /// The x no row's statements start past, once the row holds one.
        right: f32,
    }
    impl L<'_> {
        /// Stack the data nodes feeding `id` under it from `y`; returns the stack's width and
        /// height, and collects every node placed.
        fn values(&mut self, id: NodeId, x: f32, y: f32, placed: &mut Vec<NodeId>) -> (f32, f32) {
            let mut h = 0.0_f32;
            let mut w = 0.0_f32;
            let producers: Vec<NodeId> = self
                .g
                .data
                .iter()
                .filter(|d| d.to.0 == id && !self.g.nodes[d.from.0].exec_in && d.from.0 != ENTRY)
                .map(|d| d.from.0)
                .collect();
            for p in producers {
                if self.pos.contains_key(&p) {
                    continue;
                }
                let at = Pos2::new(x + INDENT_X * 0.5, y + h);
                self.pos.insert(p, at);
                placed.push(p);
                let s = self.sizes[p];
                let (cw, ch) = self.values(p, at.x, at.y + s.y + GAP_Y * 0.5, placed);
                w = w.max(INDENT_X * 0.5 + s.x.max(cw));
                h += s.y + GAP_Y * 0.5 + ch;
            }
            (w, h)
        }

        /// Lay out the chain from `from` starting at (x, y); returns the y below everything.
        fn chain(&mut self, from: (NodeId, usize), x: f32, y: f32) -> f32 {
            let mut ids = Vec::new();
            let mut cur = self.g.exec_target(from.0, from.1);
            while let Some(id) = cur {
                if self.pos.contains_key(&id) || ids.contains(&id) {
                    break;
                }
                ids.push(id);
                cur = self.g.nodes[id].next.and_then(|n| self.g.exec_target(id, n));
            }
            let mut cx = x;
            let mut row_y = y;
            let mut bottom = y;
            let mut placed = Vec::new();
            for &id in &ids {
                let s = self.sizes[id];
                if cx > x && cx + s.x > self.right {
                    row_y = self.blocks(&placed, bottom + GAP_Y) + GAP_Y;
                    placed.clear();
                    cx = x;
                    bottom = row_y;
                }
                self.pos.insert(id, Pos2::new(cx, row_y));
                placed.push(id);
                let (vw, vh) = self.values(id, cx, row_y + s.y + GAP_Y * 0.5, &mut placed);
                bottom = bottom.max(row_y + s.y + if vh > 0.0 { GAP_Y * 0.5 + vh } else { 0.0 });
                cx += s.x.max(vw) + GAP_X;
            }
            self.blocks(&placed, bottom + GAP_Y)
        }

        /// Chain the blocks off `placed` (every exec out but `next`) one under another from `y`,
        /// each indented from the node it hangs from; returns the y below the last.
        fn blocks(&mut self, placed: &[NodeId], y: f32) -> f32 {
            let mut below = y;
            for &id in placed {
                let n = &self.g.nodes[id];
                for k in 0..n.exec_outs.len() {
                    if Some(k) == n.next {
                        continue;
                    }
                    if self.g.exec_target(id, k).is_some() {
                        let bx = self.pos[&id].x + INDENT_X;
                        below = self.chain((id, k), bx, below) + GAP_Y;
                    }
                }
            }
            below - GAP_Y
        }
    }
    let mut l = L {
        g,
        sizes,
        pos: HashMap::new(),
        right: wrap_width(sizes, canvas),
    };
    l.pos.insert(ENTRY, Pos2::ZERO);
    let mut entry_values = Vec::new();
    let (_, eh) = l.values(ENTRY, 0.0, sizes[ENTRY].y + GAP_Y * 0.5, &mut entry_values);
    let mut bottom = l.chain((ENTRY, 0), sizes[ENTRY].x + GAP_X, 0.0).max(sizes[ENTRY].y + eh);
    // Whatever no chain reaches (unwired statements, loose values) goes in rows underneath,
    // wrapping at the same width.
    let mut x = 0.0;
    bottom += GAP_Y * 2.0;
    let mut row_h: f32 = 0.0;
    for id in 0..g.nodes.len() {
        if l.pos.contains_key(&id) {
            continue;
        }
        if x > 0.0 && x + sizes[id].x > l.right {
            x = 0.0;
            bottom += row_h + GAP_Y;
            row_h = 0.0;
        }
        l.pos.insert(id, Pos2::new(x, bottom));
        x += sizes[id].x + GAP_X;
        row_h = row_h.max(sizes[id].y);
    }
    l.pos
}

/// The pan and zoom that fit `bounds` (canvas units) into a canvas of `size`, no closer than 1:1.
fn fit(bounds: Rect, size: Vec2) -> (Vec2, f32) {
    let zx = (size.x - FIT_MARGIN * 2.0) / bounds.width().max(1.0);
    let zy = (size.y - FIT_MARGIN * 2.0) / bounds.height().max(1.0);
    let zoom = zx.min(zy).clamp(MIN_ZOOM, 1.0);
    let pan = size * 0.5 - bounds.center().to_vec2() * zoom;
    (pan, zoom)
}

// ───────────────────────────────────────────────────────────────────── navigator

pub fn navigator(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    ui.label(theme::disp_text("MISSIONS", theme::style_title(), theme::TX));
    ui.add_space(6.0);
    let mut filter = p.filter.clone();
    if ui
        .add(egui::TextEdit::singleline(&mut filter).hint_text("Filter mission…").desired_width(f32::INFINITY))
        .changed()
    {
        acts.push(Act::Filter(filter));
    }
    ui.add_space(6.0);
    let Some(r) = &p.reference else {
        ui.label(egui::RichText::new("Loading the Lua corpus…").small().color(theme::FAINT));
        return acts;
    };
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        scripts(ui, p, &mut acts);
        if let Some(o) = &p.open {
            in_this_script(ui, o, &mut acts);
        }
        palette(ui, r, p.open.as_ref(), &mut acts);
        save_section(ui, p, &mut acts);
    });
    acts
}

fn section_head(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        theme::eyebrow(ui, title);
        ui.label(egui::RichText::new(note).small().color(theme::FAINT));
    });
}

fn swatch(ui: &mut egui::Ui, c: Color32, round: bool) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
    if round {
        ui.painter().circle_filled(r.center(), 4.5, c);
    } else {
        ui.painter().rect_filled(r, 2.0, c);
    }
}

/// One clickable row: swatch, label, and a faint note at the right.
fn nav_row(ui: &mut egui::Ui, colour: Color32, round: bool, label: &str, note: &str, on: bool) -> egui::Response {
    let fill = if on { theme::BRASS_SOFT } else { Color32::TRANSPARENT };
    let border = if on { theme::BRASS_DK } else { Color32::TRANSPARENT };
    let r = theme::row_chip(ui, fill, border, |ui| {
        swatch(ui, colour, round);
        ui.label(egui::RichText::new(label).color(if on { theme::TX } else { theme::DIM }));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(note).monospace().color(theme::FAINT));
        });
    });
    r.interact(Sense::click())
}

fn scripts(ui: &mut egui::Ui, p: &Panel, acts: &mut Vec<Act>) {
    section_head(ui, "Script", &format!("{} mission scripts", p.catalog.len()));
    let needle = p.filter.to_ascii_lowercase();
    egui::ScrollArea::vertical().id_source("mb-scripts").max_height(220.0).show(ui, |ui| {
        for (i, e) in p.catalog.iter().enumerate() {
            if !needle.is_empty()
                && !e.name.to_ascii_lowercase().contains(&needle)
                && !e.inherits.to_ascii_lowercase().contains(&needle)
            {
                continue;
            }
            let on = p.open.as_ref().is_some_and(|o| o.entry.path == e.path);
            let state = p.save.as_ref().map(|s| s.state_for(&e.name)).unwrap_or(MissionState::None);
            let (colour, badge) = match state {
                MissionState::Completed => (GOOD, " · done"),
                MissionState::InProgress => (SELECT, " · in progress"),
                MissionState::Offered => (WARN, " · offered"),
                MissionState::None => (look(Category::Lifecycle).icon, ""),
            };
            let resp = nav_row(ui, colour, false, &e.name, &format!("{}{badge}", e.inherits), on).on_hover_text(&e.path);
            if resp.clicked() {
                acts.push(Act::Pick(i));
            }
        }
    });
}

/// Walk every canvas of a chunk with its path.
fn each_node(g: &Graph, path: &mut Vec<NodeId>, f: &mut impl FnMut(&[NodeId], NodeId, &blueprint::Node)) {
    for (i, n) in g.nodes.iter().enumerate() {
        f(path, i, n);
        if let Some(sub) = &n.sub {
            path.push(i);
            each_node(sub, path, f);
            path.pop();
        }
    }
}

fn in_this_script(ui: &mut egui::Ui, o: &Open, acts: &mut Vec<Act>) {
    section_head(ui, "In this script", &o.entry.name);
    // Functions: the canvases.
    for (i, n) in o.chunk.nodes.iter().enumerate() {
        if !n.is_function() {
            continue;
        }
        let lk = look(n.category);
        let note = if n.category == Category::Lifecycle { "override" } else { "fn" };
        let on = o.path.first() == Some(&i);
        if nav_row(ui, lk.icon, false, &n.title, note, on).clicked() {
            acts.push(Act::OpenCanvas(vec![i]));
        }
    }
    if nav_row(ui, look(Category::Chunk).icon, false, "main chunk", "top level", o.path.is_empty()).clicked() {
        acts.push(Act::OpenCanvas(Vec::new()));
    }
    // Objectives and the globals the script sets.
    let mut objectives = Vec::new();
    let mut globals: BTreeMap<String, (Vec<NodeId>, NodeId)> = BTreeMap::new();
    let mut path = Vec::new();
    each_node(&o.chunk, &mut path, &mut |path, id, n| {
        if n.category == Category::Objective && n.exec_in {
            let name = n
                .inputs
                .iter()
                .find(|i| i.label == "sName")
                .and_then(|i| match &i.value {
                    Value::Literal(e) => Some(blueprint::bare(e).trim_matches(['"', '\'']).to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| n.title.clone());
            objectives.push((name, n.title.clone(), path.to_vec(), id));
        }
        for (k, input) in n.inputs.iter().enumerate() {
            if let (true, Value::Path(v)) = (n.is_target(k), &input.value) {
                globals.entry(blueprint::bare(v)).or_insert((path.to_vec(), id));
            }
        }
    });
    for (name, module, path, id) in objectives {
        if nav_row(ui, look(Category::Objective).icon, false, &name, &module, false).clicked() {
            acts.push(Act::Reveal(path, id));
        }
    }
    for (name, (path, id)) in globals {
        if nav_row(ui, pin_colour(PinTy::Obj), true, &name, "global", false).clicked() {
            acts.push(Act::Reveal(path, id));
        }
    }
}

fn palette(ui: &mut egui::Ui, r: &Reference, open: Option<&Open>, acts: &mut Vec<Act>) {
    section_head(ui, "Palette", "from the corpus");
    if open.is_none() {
        ui.label(egui::RichText::new("Open a script to add to it.").small().color(theme::FAINT));
    }
    let mut group = |ui: &mut egui::Ui, title: &str, note: String, entries: &[PaletteEntry]| {
        egui::CollapsingHeader::new(egui::RichText::new(title).text_style(theme::style_eyebrow()).color(theme::DIM))
            .id_source(("mb-palette", title))
            .default_open(false)
            .show(ui, |ui| {
                ui.label(egui::RichText::new(note).small().color(theme::FAINT));
                for e in entries {
                    let lk = look(e.category);
                    let uses = if e.uses > 0 { e.uses.to_string() } else { String::new() };
                    let resp = ui.dnd_drag_source(egui::Id::new(("mb-drag", title, &e.label)), e.clone(), |ui| {
                        nav_row(ui, lk.icon, e.category != Category::Objective, &e.label, &uses, false)
                    });
                    let resp = resp.response.on_hover_text(&e.lua);
                    if open.is_some() && resp.clicked() {
                        acts.push(Act::Add(e.clone()));
                    }
                }
            });
    };
    group(ui, "Lua", "Lua's own statements".into(), &r.palette.flow);
    group(
        ui,
        "Lifecycle hooks",
        format!("{} functions of the base task, by how many scripts override them", r.palette.lifecycle.len()),
        &r.palette.lifecycle,
    );
    group(ui, "Event kinds", format!("{} Event.* kinds", r.palette.events.len()), &r.palette.events);
    group(
        ui,
        "Objective classes",
        format!("{} classes CreateChild names", r.palette.objectives.len()),
        &r.palette.objectives,
    );
    for (head, entries) in &r.palette.calls {
        let total: usize = entries.iter().map(|e| e.uses).sum();
        group(ui, head, format!("{} functions · {total} call sites", entries.len()), entries);
    }
}

fn save_section(ui: &mut egui::Ui, p: &Panel, acts: &mut Vec<Act>) {
    section_head(ui, "Save", "progress badges");
    match &p.save {
        Some(s) => {
            ui.label(egui::RichText::new(&s.source).monospace().color(theme::DIM));
            if ui.button("Clear").clicked() {
                acts.push(Act::ClearSave);
            }
        }
        None => {
            if ui.button("Load .profile…").clicked() {
                acts.push(Act::OpenSavePicker);
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────── canvas

pub fn center(ctx: &egui::Context, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::CentralPanel::default().frame(egui::Frame::none().fill(CANVAS_BG)).show(ctx, |ui| {
        let Some(o) = &p.open else {
            let r = ui.available_rect_before_wrap();
            let msg = if p.reference.is_some() { "Pick a mission script on the left." } else { "Loading the Lua corpus…" };
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, msg, theme::font(&TextStyle::Body), theme::DIM);
            return;
        };
        header(ui, o, &mut acts);
        let rect = ui.available_rect_before_wrap();
        match o.canvas() {
            Some(g) => canvas(ui, rect, p, o, g, &mut acts),
            None => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "That function is no longer in the script.",
                    theme::font(&TextStyle::Body),
                    BAD,
                );
            }
        }
    });
    acts
}

/// The id under which the canvas asks itself to fit to view on its next frame.
fn fit_request() -> egui::Id {
    egui::Id::new("mb-fit")
}

fn header(ui: &mut egui::Ui, o: &Open, acts: &mut Vec<Act>) {
    egui::Frame::none()
        .fill(Color32::from_rgb(0x14, 0x19, 0x21))
        .inner_margin(egui::Margin::symmetric(12.0, 6.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let file = o.entry.path.rsplit('/').next().unwrap_or(&o.entry.path);
                if ui.link(egui::RichText::new(file).monospace().color(LABEL)).clicked() {
                    acts.push(Act::OpenCanvas(Vec::new()));
                }
                let mut g = &o.chunk;
                for (k, &id) in o.path.iter().enumerate() {
                    let Some(n) = g.nodes.get(id) else { break };
                    ui.label(egui::RichText::new("›").color(ANNOT));
                    if ui.link(egui::RichText::new(format!("{}()", n.title)).color(LABEL_STRONG)).clicked() {
                        acts.push(Act::OpenCanvas(o.path[..=k].to_vec()));
                    }
                    match n.sub.as_deref() {
                        Some(s) => g = s,
                        None => break,
                    }
                }
                ui.add_space(12.0);
                ui.menu_button("View", |ui| {
                    if ui.button("Fit to view   F").clicked() {
                        ui.memory_mut(|m| m.data.insert_temp(fit_request(), true));
                        ui.close_menu();
                    }
                    if ui.button("Actual size").clicked() {
                        acts.push(Act::SetView { pan: o.view().pan, zoom: 1.0 });
                        ui.close_menu();
                    }
                });
                if ui.button("Script").on_hover_text("Open this function in the Lua viewer").clicked() {
                    acts.push(Act::JumpToSource(None));
                }
                let add = ui.button("Add Node  Tab");
                if add.clicked() {
                    acts.push(Act::BeginAdd {
                        at: free_spot(&o.view()),
                        screen: add.rect.left_bottom(),
                        from: None,
                    });
                }
                if ui.add_enabled(!o.undo.is_empty(), egui::Button::new("Undo")).clicked() {
                    acts.push(Act::Undo);
                }
                if ui.add_enabled(!o.redo.is_empty(), egui::Button::new("Redo")).clicked() {
                    acts.push(Act::Redo);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::primary_button(ui, "Compile", true).clicked() {
                        acts.push(Act::Compile);
                    }
                    if ui
                        .add_enabled(o.edited, egui::Button::new("Revert"))
                        .on_hover_text("Back to the script as shipped (undoable)")
                        .clicked()
                    {
                        acts.push(Act::Revert);
                    }
                });
            });
        });
}

/// A wire being dragged from a pin, kept in egui's memory between frames.
#[derive(Clone, Copy)]
struct Dragging {
    from: PinRef,
}

/// A selection rectangle being dragged, from this screen point.
#[derive(Clone, Copy)]
struct Marquee {
    start: Pos2,
}

fn canvas(ui: &mut egui::Ui, rect: Rect, p: &Panel, o: &Open, g: &Graph, acts: &mut Vec<Act>) {
    let view = o.view();
    let sizes: Vec<Vec2> = g.nodes.iter().map(|n| measure(ui, n)).collect();
    // Positions: lay the canvas out the first time it is shown; place anything new by the layout.
    let mut pos = view.pos.clone();
    let mut fit_now = ui.memory_mut(|m| m.data.remove_temp::<bool>(fit_request())).unwrap_or(false);
    if (0..g.nodes.len()).any(|id| !pos.contains_key(&id)) {
        if pos.is_empty() {
            fit_now = true;
        }
        let missing: HashMap<NodeId, Pos2> = layout(g, &sizes, rect.size()).into_iter().filter(|(k, _)| !pos.contains_key(k)).collect();
        pos.extend(missing.clone());
        acts.push(Act::Place(missing));
    }
    if fit_now {
        let bounds = (0..g.nodes.len())
            .map(|id| Rect::from_min_size(pos[&id], sizes[id]))
            .reduce(|a, b| a.union(b))
            .unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::splat(100.0)));
        let (pan, zoom) = fit(bounds, rect.size());
        acts.push(Act::SetView { pan, zoom });
    }
    let (pan, zoom) = (view.pan, view.zoom);
    let to_screen = |c: Pos2| rect.min + pan + c.to_vec2() * zoom;
    let to_canvas = |s: Pos2| ((s - rect.min - pan) / zoom).to_pos2();
    let node_rect = |id: NodeId| Rect::from_min_size(to_screen(pos[&id]), sizes[id] * zoom);
    let visible = |r: Rect| r.intersects(rect.expand(40.0));

    let painter = ui.painter_at(rect);
    paint_grid(&painter, rect, pan, zoom);

    // Background: drag pans, shift-drag selects a rectangle, wheel or pinch zooms at the cursor.
    let bg = ui.interact(rect, egui::Id::new("mb-canvas"), Sense::click_and_drag());
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let marquee_id = egui::Id::new("mb-marquee");
    if bg.drag_started() && ui.input(|i| i.modifiers.shift) {
        if let Some(at) = pointer {
            ui.memory_mut(|m| m.data.insert_temp(marquee_id, Marquee { start: at }));
        }
    }
    let marquee = ui.memory(|m| m.data.get_temp::<Marquee>(marquee_id));
    if bg.dragged() && marquee.is_none() {
        acts.push(Act::Pan(bg.drag_delta()));
    }
    if bg.clicked() {
        acts.push(Act::Select(Vec::new(), false));
    }
    if bg.hovered() {
        let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
        let factor = if (pinch - 1.0).abs() > 1e-3 { pinch } else { (scroll * 0.0025).exp() };
        if let (true, Some(at)) = ((factor - 1.0).abs() > 1e-4, pointer) {
            acts.push(Act::ZoomAt { factor, at: at - rect.min });
        }
    }
    bg.context_menu(|ui| {
        let at = ui.ctx().input(|i| i.pointer.interact_pos()).unwrap_or(rect.center());
        if ui.button("Add node…").clicked() {
            acts.push(Act::BeginAdd {
                at: to_canvas(at),
                screen: at,
                from: None,
            });
            ui.close_menu();
        }
        if ui.add_enabled(!o.clipboard.is_empty(), egui::Button::new("Paste")).clicked() {
            acts.push(Act::Paste(to_canvas(at)));
            ui.close_menu();
        }
        if ui.button("Fit to view").clicked() {
            ui.memory_mut(|m| m.data.insert_temp(fit_request(), true));
            ui.close_menu();
        }
    });
    if let (Some(payload), Some(at)) = (bg.dnd_release_payload::<PaletteEntry>(), pointer) {
        acts.push(Act::BeginAdd {
            at: to_canvas(at),
            screen: at,
            from: None,
        });
        acts.push(Act::Add((*payload).clone()));
    }

    // Nodes, then pins above them, so a press on a pin starts a wire rather than a node drag.
    let mut pin_hits: Vec<(PinRef, Rect)> = Vec::new();
    for (id, n) in g.nodes.iter().enumerate() {
        let r = node_rect(id);
        if !visible(r) {
            continue;
        }
        let resp = ui.interact(r, egui::Id::new(("mb-node", &o.path, id)), Sense::click_and_drag());
        let selected = o.selection.contains(&id);
        if resp.clicked() {
            let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
            acts.push(Act::Select(vec![id], add));
        }
        if resp.double_clicked() && n.sub.is_some() {
            let mut path = o.path.clone();
            path.push(id);
            acts.push(Act::OpenCanvas(path));
        }
        if resp.drag_started() && !selected {
            acts.push(Act::Select(vec![id], false));
        }
        if resp.dragged() {
            let ids = if selected { o.selection.clone() } else { vec![id] };
            acts.push(Act::Move(ids, resp.drag_delta() / zoom));
        }
        resp.context_menu(|ui| {
            let ids = if selected { o.selection.clone() } else { vec![id] };
            if n.sub.is_some() && ui.button("Open").clicked() {
                let mut path = o.path.clone();
                path.push(id);
                acts.push(Act::OpenCanvas(path));
                ui.close_menu();
            }
            if ui.add_enabled(n.line > 0 && !n.is_new(), egui::Button::new("Jump to source")).clicked() {
                acts.push(Act::JumpToSource(Some(id)));
                ui.close_menu();
            }
            if id != ENTRY {
                if ui.button("Clone").clicked() {
                    acts.push(Act::Clone(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Copy").clicked() {
                    acts.push(Act::Copy(ids.clone()));
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    acts.push(Act::Delete(ids));
                    ui.close_menu();
                }
            }
        });
        for pin in pins_of(id, n) {
            let c = to_screen(pin_at(n, pos[&id], sizes[id], pin));
            pin_hits.push((pin, Rect::from_center_size(c, Vec2::splat((PIN_R * 2.0 + 8.0) * zoom.max(0.6)))));
        }
    }
    let dragging_id = egui::Id::new("mb-wire");
    let alt = ui.input(|i| i.modifiers.alt);
    for &(pin, hit) in &pin_hits {
        let resp = ui.interact(hit, egui::Id::new(("mb-pin", &o.path, pin_key(pin))), Sense::click_and_drag());
        if resp.drag_started() {
            ui.memory_mut(|m| m.data.insert_temp(dragging_id, Dragging { from: pin }));
        }
        if resp.clicked() && alt {
            acts.extend(break_pin(g, pin));
        }
        resp.context_menu(|ui| {
            if ui.button("Break wires").clicked() {
                acts.extend(break_pin(g, pin));
                ui.close_menu();
            }
        });
    }

    // Wires: values, then exec on top.
    let pin_screen = |pin: PinRef| {
        let n = pin_node(pin);
        to_screen(pin_at(&g.nodes[n], pos[&n], sizes[n], pin))
    };
    for w in &g.data {
        let (a, b) = (pin_screen(PinRef::Out(w.from.0, w.from.1)), pin_screen(PinRef::In(w.to.0, w.to.1)));
        if visible(Rect::from_two_pos(a, b)) {
            let ty = g.nodes[w.from.0].outputs[w.from.1].ty;
            wire(&painter, a, b, Stroke::new(2.4 * zoom.max(0.5), pin_colour(ty)), zoom);
        }
    }
    for w in &g.exec {
        let (a, b) = (pin_screen(PinRef::ExecOut(w.from.0, w.from.1)), pin_screen(PinRef::ExecIn(w.to)));
        if visible(Rect::from_two_pos(a, b)) {
            wire(&painter, a, b, Stroke::new(3.0 * zoom.max(0.5), pin_colour(PinTy::Exec)), zoom);
        }
    }

    // Nodes.
    let problem_nodes: Vec<NodeId> = match &o.check {
        Check::Problems(ps) => ps.iter().filter(|(path, _)| *path == o.path).filter_map(|(_, p)| p.node).collect(),
        _ => Vec::new(),
    };
    for (id, n) in g.nodes.iter().enumerate() {
        let r = node_rect(id);
        if !visible(r) {
            continue;
        }
        let state = if o.selection.contains(&id) {
            Some(SELECT)
        } else if problem_nodes.contains(&id) {
            Some(BAD)
        } else {
            None
        };
        paint_node(&painter, g, id, n, r, zoom, state);
    }

    // The wire being dragged, and what releasing it does: wire into the pin under the pointer,
    // or open the palette there to add a node wired to it.
    if let Some(d) = ui.memory(|m| m.data.get_temp::<Dragging>(dragging_id)) {
        match ui.input(|i| i.pointer.interact_pos()) {
            Some(now) => {
                let a = pin_screen(d.from);
                let (s, e) = if matches!(d.from, PinRef::In(..) | PinRef::ExecIn(_)) { (now, a) } else { (a, now) };
                wire(&painter, s, e, Stroke::new(2.4 * zoom.max(0.5), pin_colour(pin_ty(g, d.from))), zoom);
                if ui.input(|i| i.pointer.any_released()) {
                    ui.memory_mut(|m| m.data.remove::<Dragging>(dragging_id));
                    let target = pin_hits.iter().find(|(_, hit)| hit.contains(now)).map(|(p, _)| *p);
                    match target {
                        Some(t) => acts.extend(connect(d.from, t)),
                        None if rect.contains(now) => acts.push(Act::BeginAdd {
                            at: to_canvas(now),
                            screen: now,
                            from: Some(d.from),
                        }),
                        None => {}
                    }
                }
            }
            None => ui.memory_mut(|m| m.data.remove::<Dragging>(dragging_id)),
        }
    }

    // The selection rectangle.
    if let (Some(m), Some(now)) = (marquee, ui.input(|i| i.pointer.interact_pos())) {
        let r = Rect::from_two_pos(m.start, now);
        painter.rect(r, 2.0, SELECT.gamma_multiply(0.12), Stroke::new(1.0, SELECT));
        if ui.input(|i| i.pointer.any_released()) {
            ui.memory_mut(|mm| mm.data.remove::<Marquee>(marquee_id));
            let ids: Vec<NodeId> = (0..g.nodes.len()).filter(|&id| node_rect(id).intersects(r)).collect();
            acts.push(Act::Select(ids, false));
        }
    }

    keys(ui, rect, o, pointer, &to_canvas, acts);
    paint_legend(&painter, rect);
    acts.extend(zoom_pill(ui, rect, zoom));
    if let Some(a) = &o.adding {
        acts.extend(add_popup(ui.ctx(), p, o, a));
    }
}

fn pin_node(p: PinRef) -> NodeId {
    match p {
        PinRef::ExecIn(n) | PinRef::ExecOut(n, _) | PinRef::In(n, _) | PinRef::Out(n, _) => n,
    }
}

fn pin_key(p: PinRef) -> (u8, NodeId, usize) {
    match p {
        PinRef::ExecIn(n) => (0, n, 0),
        PinRef::ExecOut(n, k) => (1, n, k),
        PinRef::In(n, k) => (2, n, k),
        PinRef::Out(n, k) => (3, n, k),
    }
}

fn pin_ty(g: &Graph, p: PinRef) -> PinTy {
    match p {
        PinRef::ExecIn(_) | PinRef::ExecOut(..) => PinTy::Exec,
        PinRef::In(n, i) => g.nodes[n].inputs[i].ty,
        PinRef::Out(n, k) => g.nodes[n].outputs[k].ty,
    }
}

/// The act a wire dragged between two pins makes, whichever end it started from.
fn connect(a: PinRef, b: PinRef) -> Option<Act> {
    match (a, b) {
        (PinRef::ExecOut(n, k), PinRef::ExecIn(m)) | (PinRef::ExecIn(m), PinRef::ExecOut(n, k)) => {
            Some(Act::ConnectExec { from: (n, k), to: m })
        }
        (PinRef::Out(n, k), PinRef::In(m, i)) | (PinRef::In(m, i), PinRef::Out(n, k)) => {
            Some(Act::ConnectData { from: (n, k), to: (m, i) })
        }
        _ => None,
    }
}

fn break_pin(g: &Graph, pin: PinRef) -> Vec<Act> {
    match pin {
        PinRef::In(n, i) => vec![Act::BreakData((n, i))],
        PinRef::ExecOut(n, k) => vec![Act::BreakExec((n, k))],
        PinRef::ExecIn(n) => vec![Act::BreakExecInto(n)],
        PinRef::Out(n, k) => g.data.iter().filter(|w| w.from == (n, k)).map(|w| Act::BreakData(w.to)).collect(),
    }
}

/// The canvas's keys, while the pointer is over it and no field has the keyboard.
fn keys(ui: &egui::Ui, rect: Rect, o: &Open, pointer: Option<Pos2>, to_canvas: &dyn Fn(Pos2) -> Pos2, acts: &mut Vec<Act>) {
    let Some(at) = pointer.filter(|p| rect.contains(*p)) else { return };
    if ui.ctx().wants_keyboard_input() || o.adding.is_some() {
        return;
    }
    use egui::{Key, Modifiers};
    let (tab, del, copy, paste, dup, all, redo, undo, fit) = ui.ctx().input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::Tab),
            i.consume_key(Modifiers::NONE, Key::Delete) | i.consume_key(Modifiers::NONE, Key::Backspace),
            i.consume_key(Modifiers::COMMAND, Key::C),
            i.consume_key(Modifiers::COMMAND, Key::V),
            i.consume_key(Modifiers::COMMAND, Key::D),
            i.consume_key(Modifiers::COMMAND, Key::A),
            i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
            i.consume_key(Modifiers::COMMAND, Key::Z),
            i.consume_key(Modifiers::NONE, Key::F),
        )
    });
    let sel = !o.selection.is_empty();
    if tab {
        acts.push(Act::BeginAdd {
            at: to_canvas(at),
            screen: at,
            from: None,
        });
    }
    if del && sel {
        acts.push(Act::Delete(o.selection.clone()));
    }
    if copy && sel {
        acts.push(Act::Copy(o.selection.clone()));
    }
    if paste {
        acts.push(Act::Paste(to_canvas(at)));
    }
    if dup && sel {
        acts.push(Act::Clone(o.selection.clone()));
    }
    if all {
        acts.push(Act::SelectAll);
    }
    if redo {
        acts.push(Act::Redo);
    }
    if undo {
        acts.push(Act::Undo);
    }
    if fit {
        ui.ctx().memory_mut(|m| m.data.insert_temp(fit_request(), true));
    }
}

fn paint_grid(p: &egui::Painter, rect: Rect, pan: Vec2, zoom: f32) {
    p.rect_filled(rect, 0.0, CANVAS_BG);
    let step = GRID * zoom;
    if step >= 8.0 {
        let r = (1.3 * zoom).max(0.8);
        let x0 = rect.min.x + pan.x.rem_euclid(step);
        let mut y = rect.min.y + pan.y.rem_euclid(step);
        while y < rect.max.y {
            let mut x = x0;
            while x < rect.max.x {
                p.circle_filled(Pos2::new(x, y), r, DOT);
                x += step;
            }
            y += step;
        }
    }
    // Vignette: the canvas darkens towards its edges.
    let mut mesh = egui::Mesh::default();
    let inner = rect.shrink2(rect.size() * 0.22);
    let dark = Color32::from_rgba_premultiplied(2, 4, 6, 140);
    let corners = |r: Rect| [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()];
    for (o, i) in corners(rect).into_iter().zip(corners(inner)) {
        mesh.colored_vertex(o, dark);
        mesh.colored_vertex(i, Color32::TRANSPARENT);
    }
    for k in 0..4u32 {
        let (o0, i0, o1, i1) = (2 * k, 2 * k + 1, 2 * ((k + 1) % 4), 2 * ((k + 1) % 4) + 1);
        mesh.add_triangle(o0, i0, o1);
        mesh.add_triangle(i0, i1, o1);
    }
    p.add(egui::Shape::mesh(mesh));
}

fn wire(p: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke, zoom: f32) {
    let dx = ((b.x - a.x).abs() * 0.5).max(40.0 * zoom);
    let pts = [a, a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0), b];
    p.add(egui::epaint::CubicBezierShape::from_points_stroke(pts, false, Color32::TRANSPARENT, stroke));
}

fn paint_node(p: &egui::Painter, g: &Graph, id: NodeId, n: &blueprint::Node, r: Rect, zoom: f32, state: Option<Color32>) {
    let round = ROUND * zoom;
    let lk = look(n.category);
    p.rect_filled(r.translate(Vec2::new(0.0, 8.0 * zoom)), round, Color32::from_black_alpha(90));
    gradient(p, r, NODE_TOP, NODE_BOTTOM, egui::Rounding::same(round));
    let header = Rect::from_min_size(r.min, Vec2::new(r.width(), HEADER_H * zoom));
    gradient(p, header, lk.top, lk.bottom, egui::Rounding { nw: round, ne: round, sw: 0.0, se: 0.0 });
    match state {
        Some(c) => {
            p.rect_stroke(r, round, Stroke::new(1.5, c));
            p.rect_stroke(r.expand(3.0), round + 3.0, Stroke::new(3.0, c.gamma_multiply(0.35)));
        }
        None => {
            p.rect_stroke(r, round, Stroke::new(1.0, NODE_BORDER));
        }
    }
    if n.is_new() {
        let stripe = Rect::from_min_size(r.min + Vec2::new(0.0, HEADER_H * zoom), Vec2::new(3.0 * zoom, r.height() - HEADER_H * zoom));
        p.rect_filled(stripe, 0.0, SELECT);
    }
    // Header: icon, title, eyebrow.
    let icon = Rect::from_min_size(header.min + Vec2::new(9.0, (HEADER_H - ICON) * 0.5) * zoom, Vec2::splat(ICON * zoom));
    p.rect_filled(icon, 5.0 * zoom, Color32::from_black_alpha(70));
    glyph(p, icon.shrink(3.0 * zoom), n.category, lk.icon);
    let clip = p.with_clip_rect(header.shrink(2.0 * zoom).intersect(p.clip_rect()));
    let at = Pos2::new(icon.max.x + 8.0 * zoom, header.center().y);
    clip.text(at, egui::Align2::LEFT_CENTER, &n.title, scaled(&TextStyle::Body, zoom), lk.text);
    let at = Pos2::new(header.max.x - PAD_X * zoom, header.center().y);
    let eyebrow = scaled(&theme::style_eyebrow(), zoom);
    clip.text(at, egui::Align2::RIGHT_CENTER, lk.eyebrow.to_uppercase(), eyebrow, Color32::from_white_alpha(110));
    // Pins.
    let text = scaled(&TextStyle::Small, zoom);
    let mono = scaled(&TextStyle::Monospace, zoom);
    let body = p.with_clip_rect(r.intersect(p.clip_rect()));
    let size = r.size() / zoom;
    for pin in pins_of(id, n) {
        let c = r.min + pin_at(n, Pos2::ZERO, size, pin).to_vec2() * zoom;
        let wired = match pin {
            PinRef::ExecIn(_) => g.exec.iter().any(|w| w.to == id),
            PinRef::ExecOut(_, k) => g.exec_target(id, k).is_some(),
            PinRef::In(_, i) => g.feeder(id, i).is_some(),
            PinRef::Out(_, k) => g.data.iter().any(|w| w.from == (id, k)),
        };
        pin_shape(p, c, pin_ty(g, pin), wired, zoom);
        let f = &text;
        let (label, strong) = match pin {
            PinRef::ExecIn(_) => ("Exec".to_string(), true),
            PinRef::ExecOut(_, k) => (exec_label(n, k).to_string(), true),
            PinRef::In(_, i) => (n.inputs[i].label.clone(), false),
            PinRef::Out(_, k) => (n.outputs[k].label.clone(), false),
        };
        let colour = if strong { LABEL_STRONG } else { LABEL };
        let offset = (PIN_R + 8.0) * zoom;
        match pin {
            PinRef::ExecIn(_) | PinRef::In(..) => {
                let lr = body.text(c + Vec2::new(offset, 0.0), egui::Align2::LEFT_CENTER, &label, f.clone(), colour);
                if let PinRef::In(_, i) = pin {
                    if let Some(a) = annot(&n.inputs[i].value) {
                        body.text(Pos2::new(lr.max.x + 6.0 * zoom, c.y), egui::Align2::LEFT_CENTER, a, mono.clone(), ANNOT);
                    }
                }
            }
            _ => {
                body.text(c - Vec2::new(offset, 0.0), egui::Align2::RIGHT_CENTER, &label, f.clone(), colour);
            }
        }
    }
}

fn pin_shape(p: &egui::Painter, c: Pos2, ty: PinTy, wired: bool, zoom: f32) {
    let col = pin_colour(ty);
    let s = (PIN_R + 1.0) * zoom;
    let edge = Stroke::new(1.2, CANVAS_BG);
    if ty == PinTy::Exec {
        let pts = vec![
            c + Vec2::new(-s * 0.9, -s),
            c + Vec2::new(s * 0.3, -s),
            c + Vec2::new(s * 1.1, 0.0),
            c + Vec2::new(s * 0.3, s),
            c + Vec2::new(-s * 0.9, s),
        ];
        let (fill, stroke) = if wired { (col, edge) } else { (Color32::TRANSPARENT, Stroke::new(1.6 * zoom.max(0.6), col)) };
        p.add(egui::Shape::convex_polygon(pts, fill, stroke));
    } else if wired {
        p.circle(c, PIN_R * zoom, col, edge);
    } else {
        p.circle_stroke(c, PIN_R * zoom * 0.85, Stroke::new(2.0 * zoom.max(0.6), col));
    }
}

fn gradient(p: &egui::Painter, r: Rect, top: Color32, bottom: Color32, rounding: egui::Rounding) {
    let bands = 8;
    for i in 0..bands {
        let t = i as f32 / (bands - 1) as f32;
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        let c = Color32::from_rgb(mix(top.r(), bottom.r()), mix(top.g(), bottom.g()), mix(top.b(), bottom.b()));
        let y0 = r.min.y + r.height() * i as f32 / bands as f32;
        let y1 = r.min.y + r.height() * (i + 1) as f32 / bands as f32;
        let band = Rect::from_min_max(Pos2::new(r.min.x, y0), Pos2::new(r.max.x, y1));
        let rd = egui::Rounding {
            nw: if i == 0 { rounding.nw } else { 0.0 },
            ne: if i == 0 { rounding.ne } else { 0.0 },
            sw: if i == bands - 1 { rounding.sw } else { 0.0 },
            se: if i == bands - 1 { rounding.se } else { 0.0 },
        };
        p.rect_filled(band, rd, c);
    }
}

fn paint_legend(p: &egui::Painter, canvas: Rect) {
    let items = [PinTy::Exec, PinTy::Str, PinTy::Num, PinTy::Bool, PinTy::Obj, PinTy::Arr, PinTy::Any];
    let small = theme::font(&TextStyle::Small);
    let r = Rect::from_min_size(Pos2::new(canvas.min.x + 14.0, canvas.max.y - 44.0), Vec2::new(430.0, 30.0));
    p.rect(r, 10.0, OVERLAY_BG, Stroke::new(1.0, NODE_BORDER));
    let lr = p.text(
        Pos2::new(r.min.x + 11.0, r.center().y),
        egui::Align2::LEFT_CENTER,
        "LEGEND",
        theme::font(&theme::style_eyebrow()),
        ANNOT,
    );
    let mut x = lr.max.x + 12.0;
    for t in items {
        pin_shape(p, Pos2::new(x + 5.0, r.center().y), t, true, 0.9);
        let tr = p.text(Pos2::new(x + 14.0, r.center().y), egui::Align2::LEFT_CENTER, t.name(), small.clone(), LABEL);
        x = tr.max.x + 12.0;
    }
}

fn zoom_pill(ui: &mut egui::Ui, canvas: Rect, zoom: f32) -> Vec<Act> {
    let mut acts = Vec::new();
    let r = Rect::from_min_size(Pos2::new(canvas.max.x - 150.0, canvas.max.y - 44.0), Vec2::new(136.0, 30.0));
    let p = ui.painter();
    p.rect(r, 9.0, OVERLAY_BG, Stroke::new(1.0, NODE_BORDER));
    let minus = Rect::from_min_size(r.min + Vec2::new(3.0, 2.0), Vec2::new(28.0, 26.0));
    let plus = Rect::from_min_size(Pos2::new(r.max.x - 31.0, r.min.y + 2.0), Vec2::new(28.0, 26.0));
    let body = theme::font(&TextStyle::Body);
    p.text(minus.center(), egui::Align2::CENTER_CENTER, "−", body.clone(), LABEL);
    p.text(plus.center(), egui::Align2::CENTER_CENTER, "+", body, LABEL);
    let pct = format!("{}%", (zoom * 100.0).round() as i32);
    p.text(r.center(), egui::Align2::CENTER_CENTER, pct, theme::font(&TextStyle::Monospace), LABEL);
    let centre = canvas.size() * 0.5;
    if ui.interact(minus, egui::Id::new("mb-zoom-out"), Sense::click()).clicked() {
        acts.push(Act::ZoomAt { factor: 1.0 / 1.2, at: centre });
    }
    if ui.interact(plus, egui::Id::new("mb-zoom-in"), Sense::click()).clicked() {
        acts.push(Act::ZoomAt { factor: 1.2, at: centre });
    }
    acts
}

/// The add-node palette: search everything the editor can add, plus calls to this script's own
/// functions.
fn add_popup(ctx: &egui::Context, p: &Panel, o: &Open, a: &Adding) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(r) = &p.reference else { return acts };
    let own = own_calls(o);
    let needle = a.query.to_ascii_lowercase();
    let matches: Vec<&PaletteEntry> = own
        .iter()
        .chain(r.palette.all())
        .filter(|e| needle.is_empty() || e.label.to_ascii_lowercase().contains(&needle))
        .take(150)
        .collect();
    let area = egui::Area::new(egui::Id::new("mb-add"))
        .fixed_pos(a.screen)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(340.0);
                ui.label(theme::disp_text("ADD NODE", theme::style_eyebrow(), theme::DIM));
                let mut q = a.query.clone();
                let field = ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search calls, events, objectives, Lua…"));
                if !field.has_focus() && q.is_empty() {
                    field.request_focus();
                }
                if field.changed() {
                    acts.push(Act::AddQuery(q));
                }
                if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(first) = matches.first() {
                        acts.push(Act::Add((*first).clone()));
                    }
                }
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for e in &matches {
                        let uses = if e.uses > 0 { e.uses.to_string() } else { String::new() };
                        let row = nav_row(ui, look(e.category).icon, e.category != Category::Objective, &e.label, &uses, false);
                        if row.on_hover_text(&e.lua).clicked() {
                            acts.push(Act::Add((*e).clone()));
                        }
                    }
                    if matches.is_empty() {
                        ui.label(egui::RichText::new("Nothing matches.").small().color(theme::FAINT));
                    }
                });
            });
        });
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let clicked_away = ctx.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !area.response.rect.contains(p)));
    if escape || clicked_away {
        acts.push(Act::CancelAdd);
    }
    acts
}

/// Calls to the functions this script defines, for the add palette.
fn own_calls(o: &Open) -> Vec<PaletteEntry> {
    o.chunk
        .nodes
        .iter()
        .filter(|n| n.is_function())
        .filter_map(|n| {
            let sub = n.sub.as_deref()?;
            let params: Vec<&str> = sub.nodes[ENTRY]
                .outputs
                .iter()
                .filter(|p| p.kind == OutKind::Var)
                .map(|p| p.label.as_str())
                .collect();
            let name = n.title.rsplit(['.', ':']).next().unwrap_or(&n.title);
            let lua = match params.split_first() {
                Some((&"self", rest)) => format!("self:{name}({})", vec!["nil"; rest.len()].join(", ")),
                _ => format!("{name}({})", vec!["nil"; params.len()].join(", ")),
            };
            Some(PaletteEntry {
                label: format!("{name} (this script)"),
                category: Category::Fn,
                lua,
                uses: 0,
                top_level: false,
            })
        })
        .collect()
}

// ───────────────────────────────────────────────────────────────────── inspector

pub fn inspector(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let mut acts = Vec::new();
    let Some(o) = &p.open else {
        ui.label(egui::RichText::new("No script open.").color(theme::FAINT));
        return acts;
    };
    let Some(g) = o.canvas() else { return acts };
    match o.selection.as_slice() {
        [one] if *one < g.nodes.len() => node_details(ui, o, g, *one, &g.nodes[*one], &mut acts),
        _ => canvas_summary(ui, o, g, &mut acts),
    }
    acts
}

fn heading(ui: &mut egui::Ui, n: &blueprint::Node, pills: &[String]) {
    let lk = look(n.category);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
        gradient(ui.painter(), r, lk.top, lk.bottom, egui::Rounding::same(8.0));
        glyph(ui.painter(), r.shrink(9.0), n.category, lk.icon);
        ui.vertical(|ui| {
            ui.label(theme::disp_text(&n.title, theme::style_title(), theme::TX));
            ui.horizontal_wrapped(|ui| {
                theme::pill(ui, lk.eyebrow, true);
                for pill in pills {
                    theme::pill(ui, pill, false);
                }
            });
        });
    });
}

fn canvas_summary(ui: &mut egui::Ui, o: &Open, g: &Graph, acts: &mut Vec<Act>) {
    let entry = &g.nodes[ENTRY];
    heading(ui, entry, &[format!("{} nodes", g.nodes.len())]);
    ui.add_space(6.0);
    let names = |kind: OutKind| entry.outputs.iter().filter(|p| p.kind == kind).map(|p| p.label.as_str()).collect::<Vec<_>>();
    let (params, captured) = (names(OutKind::Var), names(OutKind::Captured));
    if !params.is_empty() {
        theme::kv(ui, "Parameters", egui::RichText::new(params.join(", ")));
    }
    if !captured.is_empty() {
        theme::kv(ui, "Captured", egui::RichText::new(captured.join(", ")));
    }
    if let Check::Problems(ps) = &o.check {
        theme::section(ui, "Problems", Some(&ps.len().to_string()), true, |ui| {
            for (path, problem) in ps {
                let text = egui::RichText::new(&problem.message).color(BAD);
                match problem.node {
                    Some(id) => {
                        if ui.link(text).clicked() {
                            acts.push(Act::Reveal(path.clone(), id));
                        }
                    }
                    None => {
                        ui.label(text);
                    }
                }
            }
        });
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Select a node to see its execution, parameters and source. Double-click a function to open it.")
            .small()
            .color(theme::FAINT),
    );
}

fn node_details(ui: &mut egui::Ui, o: &Open, g: &Graph, id: NodeId, n: &blueprint::Node, acts: &mut Vec<Act>) {
    let mut pills = Vec::new();
    if n.line > 0 && !n.is_new() {
        pills.push(format!("line {}", n.line));
    }
    if n.is_new() {
        pills.push("new".into());
    }
    heading(ui, n, &pills);
    if let Some(callee) = n.callee() {
        ui.label(egui::RichText::new(format!("calls {callee}")).monospace().color(theme::FAINT));
    }
    ui.add_space(6.0);

    if n.exec_in || !n.exec_outs.is_empty() {
        theme::section(ui, "Execution", None, true, |ui| {
            if n.exec_in {
                let from = g.exec.iter().find(|w| w.to == id).map(|w| w.from.0);
                exec_row(ui, "Exec in", from, g, acts);
            }
            for k in 0..n.exec_outs.len() {
                exec_row(ui, exec_label(n, k), g.exec_target(id, k), g, acts);
            }
        });
    }

    if !n.inputs.is_empty() {
        theme::section(ui, "Parameters", None, true, |ui| {
            let mut group: Option<&str> = None;
            for (i, input) in n.inputs.iter().enumerate() {
                if input.group.as_deref() != group {
                    group = input.group.as_deref();
                    if let Some(g) = group {
                        ui.label(egui::RichText::new(g).small().color(theme::FAINT));
                    }
                }
                parameter(ui, o, g, id, i, input, acts);
            }
        });
    }

    if !n.outputs.is_empty() {
        theme::section(ui, "Outputs", None, true, |ui| {
            for out in &n.outputs {
                ui.horizontal(|ui| {
                    swatch(ui, pin_colour(out.ty), true);
                    ui.label(egui::RichText::new(&out.label).color(theme::TX));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let kind = match out.kind {
                            OutKind::Var => "local",
                            OutKind::Captured => "captured",
                            OutKind::Value => "value",
                        };
                        ui.label(egui::RichText::new(format!("{kind} · {}", out.ty.name())).monospace().color(theme::FAINT));
                    });
                });
            }
        });
    }

    theme::section(ui, "Source", None, true, |ui| match g.node_source(id) {
        Ok(src) => theme::code_block(ui, &src),
        Err(e) => {
            ui.label(egui::RichText::new(e).color(BAD));
        }
    });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        if ui.add_enabled(n.line > 0 && !n.is_new(), egui::Button::new("Jump to source")).clicked() {
            acts.push(Act::JumpToSource(Some(id)));
        }
        if ui.add_enabled(id != ENTRY, egui::Button::new("Clone")).clicked() {
            acts.push(Act::Clone(vec![id]));
        }
        if ui.add_enabled(id != ENTRY, egui::Button::new("Delete")).clicked() {
            acts.push(Act::Delete(vec![id]));
        }
    });
    ui.add_space(4.0);
    if theme::primary_button(ui, "Add to Shipment", true)
        .on_hover_text("Compile this script and stage it in the open Shipment as replace_lua")
        .clicked()
    {
        acts.push(Act::AddToShipment);
    }
}

fn exec_row(ui: &mut egui::Ui, label: &str, other: Option<NodeId>, g: &Graph, acts: &mut Vec<Act>) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
        match other {
            Some(_) => {
                ui.painter().rect_filled(r, 2.0, pin_colour(PinTy::Exec));
            }
            None => {
                ui.painter().rect_stroke(r, 2.0, Stroke::new(1.5, ANNOT));
            }
        }
        ui.label(egui::RichText::new(label).color(theme::TX));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match other {
            Some(o) => {
                if ui.link(&g.nodes[o].title).clicked() {
                    acts.push(Act::Select(vec![o], false));
                }
            }
            None => {
                ui.label(egui::RichText::new("unbound").italics().color(theme::FAINT));
            }
        });
    });
}

/// One parameter: a typed editor for what it holds, or the wire that feeds it.
fn parameter(ui: &mut egui::Ui, o: &Open, g: &Graph, id: NodeId, i: usize, input: &blueprint::Input, acts: &mut Vec<Act>) {
    ui.horizontal(|ui| {
        swatch(ui, pin_colour(input.ty), true);
        ui.label(egui::RichText::new(&input.label).color(theme::TX));
        ui.label(egui::RichText::new(input.ty.name()).monospace().color(theme::FAINT));
    });
    let key = egui::Id::new(("mb-param", &o.entry.path, &o.path, id, i));
    match g.feeder(id, i) {
        Some((from, out)) => {
            let producer = &g.nodes[from];
            let text = match producer.outputs[out].kind {
                OutKind::Value => format!("{} — its result", producer.title),
                _ => format!("{} — from {}", producer.outputs[out].label, producer.title),
            };
            ui.horizontal(|ui| {
                if ui.link(text).clicked() {
                    acts.push(Act::Select(vec![from], false));
                }
                if ui.small_button("Break").clicked() {
                    acts.push(Act::BreakData((id, i)));
                }
            });
        }
        None => match &input.value {
            Value::Literal(e) => literal_editor(ui, key, id, i, e, acts),
            Value::Wired { .. } => {
                ui.label(egui::RichText::new("nothing wired — wire a value in, or type one").color(WARN));
                lua_editor(ui, key, id, i, "", false, acts);
            }
            other => {
                let current = g.pin_source(id, i).unwrap_or_default();
                let multi = matches!(other, Value::Lua(_) | Value::Array(_));
                lua_editor(ui, key, id, i, &current, multi, acts);
            }
        },
    }
    ui.add_space(4.0);
}

/// Edit a literal by its kind: a string's text, a number with a stepper, a boolean by toggle.
fn literal_editor(ui: &mut egui::Ui, key: egui::Id, node: NodeId, pin: usize, e: &full_moon::ast::Expression, acts: &mut Vec<Act>) {
    use full_moon::ast::Expression;
    let lua = blueprint::bare(e);
    match e {
        Expression::String(t) => {
            let quote = lua.chars().next().filter(|c| *c == '"' || *c == '\'');
            match (quote, blueprint::string_value(t)) {
                (Some(q), Some(text)) => {
                    let current = unescape(&text);
                    let mut buf = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| current.clone());
                    let resp = ui.add(egui::TextEdit::singleline(&mut buf).font(TextStyle::Monospace).desired_width(f32::INFINITY));
                    commit(ui, Draft { key, resp, buf: &buf, current: &current }, |b| encode_lua_string(b, q), node, pin, acts);
                }
                _ => lua_editor(ui, key, node, pin, &lua, true, acts),
            }
        }
        Expression::Symbol(_) if lua == "true" || lua == "false" => {
            ui.horizontal(|ui| {
                for v in ["true", "false"] {
                    if ui.selectable_label(lua == v, v).clicked() && lua != v {
                        acts.push(Act::SetValue { node, pin, lua: v.into() });
                    }
                }
            });
        }
        Expression::Number(_) | Expression::UnaryOperator { .. } => {
            ui.horizontal(|ui| {
                let step = |d: f64| -> Option<String> {
                    let n: f64 = lua.parse().ok()?;
                    let v = n + d;
                    Some(if v.fract() == 0.0 && !lua.contains('.') { format!("{}", v as i64) } else { format!("{v}") })
                };
                if ui.button("−").clicked() {
                    if let Some(v) = step(-1.0) {
                        acts.push(Act::SetValue { node, pin, lua: v });
                    }
                }
                let mut buf = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| lua.clone());
                let resp = ui.add(egui::TextEdit::singleline(&mut buf).font(TextStyle::Monospace).desired_width(90.0));
                commit(ui, Draft { key, resp, buf: &buf, current: &lua }, |b| b.to_string(), node, pin, acts);
                if ui.button("+").clicked() {
                    if let Some(v) = step(1.0) {
                        acts.push(Act::SetValue { node, pin, lua: v });
                    }
                }
            });
        }
        _ => lua_editor(ui, key, node, pin, &lua, false, acts),
    }
}

/// Edit a pin as Lua source.
fn lua_editor(ui: &mut egui::Ui, key: egui::Id, node: NodeId, pin: usize, current: &str, multi: bool, acts: &mut Vec<Act>) {
    let mut buf = ui.data_mut(|d| d.get_temp::<String>(key)).unwrap_or_else(|| current.to_string());
    let edit = if multi { egui::TextEdit::multiline(&mut buf).desired_rows(3) } else { egui::TextEdit::singleline(&mut buf) };
    let resp = ui.add(edit.code_editor().font(TextStyle::Monospace).desired_width(f32::INFINITY));
    commit(ui, Draft { key, resp, buf: &buf, current }, |b| b.to_string(), node, pin, acts);
}

/// A field's edit in progress.
struct Draft<'a> {
    key: egui::Id,
    resp: egui::Response,
    buf: &'a str,
    current: &'a str,
}

/// Keep a draft while a field is edited; when it loses focus changed, set the pin.
fn commit(ui: &mut egui::Ui, d: Draft<'_>, to_lua: impl Fn(&str) -> String, node: NodeId, pin: usize, acts: &mut Vec<Act>) {
    if d.resp.changed() {
        ui.data_mut(|m| m.insert_temp(d.key, d.buf.to_string()));
    }
    if d.resp.lost_focus() {
        if d.buf != d.current {
            acts.push(Act::SetValue { node, pin, lua: to_lua(d.buf) });
        }
        ui.data_mut(|m| m.remove::<String>(d.key));
    }
}

/// A Lua string literal's text as the user reads it: escapes resolved.
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

fn encode_lua_string(value: &str, quote: char) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push(quote);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

// ───────────────────────────────────────────────────────────────────── footer

pub fn verbs(ui: &mut egui::Ui, p: &Panel) -> Vec<Act> {
    let acts = Vec::new();
    let Some(o) = &p.open else {
        if let Some((ok, msg)) = &p.status {
            ui.label(egui::RichText::new(msg).color(if *ok { theme::DIM } else { BAD }));
        }
        return acts;
    };
    let file = o.entry.path.rsplit('/').next().unwrap_or(&o.entry.path);
    let (colour, text) = match (&o.check, o.edited) {
        (Check::Compiled(_), _) => (GOOD, format!("{file} compiled")),
        (Check::Refused(e), _) => (BAD, format!("{file}: the game's Lua refuses it — {e}")),
        (Check::Problems(ps), _) => (BAD, format!("{file}: {} problem(s) — {}", ps.len(), ps[0].1.message)),
        (Check::Emits, true) => (WARN, format!("{file} edited — compile to check")),
        (Check::Emits, false) => (theme::DIM, format!("{file} as shipped")),
    };
    let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 3.5, colour);
    ui.label(egui::RichText::new(text).color(theme::DIM));
    if let Some((ok, msg)) = &p.status {
        ui.label(egui::RichText::new(msg).color(if *ok { theme::DIM } else { BAD }));
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let zoom = format!("zoom {}%", (o.view().zoom * 100.0).round() as i32);
        ui.label(egui::RichText::new(zoom).monospace().color(LABEL));
        ui.separator();
        if let Some(g) = o.canvas() {
            let counts = format!(
                "{} nodes · {} wires · {} selected",
                g.nodes.len(),
                g.exec.len() + g.data.len(),
                o.selection.len()
            );
            ui.label(egui::RichText::new(counts).monospace().color(LABEL));
        }
    });
    acts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_lua_string_escapes_quotes_and_backslashes() {
        assert_eq!(encode_lua_string("plain", '"'), "\"plain\"");
        assert_eq!(encode_lua_string("a\"b", '"'), "\"a\\\"b\"");
        assert_eq!(encode_lua_string("a\\b", '"'), "\"a\\\\b\"");
        assert_eq!(encode_lua_string("line\n2", '"'), "\"line\\n2\"");
        assert_eq!(encode_lua_string("x\"y", '\''), "'x\"y'");
    }

    #[test]
    fn unescape_reverses_encode() {
        for s in ["plain", "a\"b", "a\\b", "line\n2", "tab\there"] {
            let lit = encode_lua_string(s, '"');
            assert_eq!(unescape(&lit[1..lit.len() - 1]), s);
        }
    }

    #[test]
    fn a_save_state_matches_the_script_name_in_any_case() {
        let mut s = SaveOverlay::default();
        s.completed.insert("pmccon001".into(), 1.0);
        s.active.insert("pmccon002".into(), 1.0);
        s.active.insert("pmccon003".into(), 0.0);
        assert_eq!(s.state_for("PmcCon001"), MissionState::Completed);
        assert_eq!(s.state_for("pmccon002"), MissionState::InProgress);
        assert_eq!(s.state_for("pmccon003"), MissionState::Offered);
        assert_eq!(s.state_for("pmccon004"), MissionState::None);
    }

    #[test]
    fn fit_frames_the_bounds_no_closer_than_actual_size() {
        let (pan, zoom) = fit(Rect::from_min_size(Pos2::ZERO, Vec2::new(100.0, 100.0)), Vec2::new(1000.0, 800.0));
        assert_eq!(zoom, 1.0);
        assert_eq!(pan, Vec2::new(450.0, 350.0));
        let (_, zoom) = fit(Rect::from_min_size(Pos2::ZERO, Vec2::new(20000.0, 400.0)), Vec2::new(1000.0, 800.0));
        assert_eq!(zoom, MIN_ZOOM);
    }

    /// Canvas text scales with the zoom, below the type scale's smallest size too, in half-pixel
    /// steps.
    #[test]
    fn canvas_text_scales_with_the_zoom_in_half_pixel_steps() {
        let body = theme::font(&TextStyle::Body).size;
        assert_eq!(scaled(&TextStyle::Body, 2.0).size, body * 2.0);
        assert_eq!(scaled(&TextStyle::Body, 0.1).size, (body * 0.1 * 2.0).round() / 2.0);
        assert!(scaled(&TextStyle::Body, 0.1).size < theme::font(&TextStyle::Small).size);
        let sizes: std::collections::BTreeSet<u32> =
            (100..=200).map(|z| (scaled(&TextStyle::Body, z as f32 / 1000.0).size * 2.0) as u32).collect();
        assert!(sizes.len() <= 3, "{sizes:?}");
    }

    /// Draw the whole page for a frame, the way the app lays it out, and apply what it asked for.
    fn frame(ctx: &egui::Context, p: &mut Panel, corpus: &[(String, String, String)]) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1000.0))),
            ..Default::default()
        };
        let mut acts = Vec::new();
        let _ = ctx.run(input, |ctx| {
            egui::SidePanel::left("nav").show(ctx, |ui| acts.extend(navigator(ui, p)));
            egui::SidePanel::right("inspector").show(ctx, |ui| acts.extend(inspector(ui, p)));
            egui::TopBottomPanel::bottom("verbs").show(ctx, |ui| ui.horizontal(|ui| acts.extend(verbs(ui, p))));
            acts.extend(center(ctx, p));
        });
        for a in acts {
            apply(a, p, corpus);
        }
    }

    /// The page opens a real contract, lays its canvas out and fits it, shows a node, compiles it,
    /// adds a node from the palette and takes it back — drawing every frame in between.
    #[test]
    fn the_page_opens_edits_and_compiles_a_real_contract() {
        let home = crate::index::data_home().expect("no workshop_data reference bundle found; the page test opens its Lua");
        let corpus = crate::index::lua_corpus_at(&home).expect("the reference bundle's Lua corpus");
        let reference = Reference::build(&corpus).expect("the corpus indexes and lifts");
        let mut p = Panel::default();
        p.set_reference(reference, &corpus);
        let i = p.catalog.iter().position(|e| e.name == "pmccon016").expect("pmccon016 is a mission script");
        apply(Act::Pick(i), &mut p, &corpus);
        assert!(p.status.is_none(), "{:?}", p.status);

        let ctx = egui::Context::default();
        theme::install(&ctx);
        frame(&ctx, &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);
        // It opens on the script's first override.
        let o = p.open.as_ref().expect("open");
        assert_eq!(o.canvas().expect("canvas").nodes[ENTRY].category, Category::Lifecycle);
        let activated = o.chunk.nodes.iter().position(|n| n.title == "Activated").expect("Activated");
        apply(Act::OpenCanvas(vec![activated]), &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);
        let o = p.open.as_ref().expect("open");
        let g = o.canvas().expect("canvas");
        assert_eq!(g.nodes[ENTRY].title, "On Activated");
        let v = o.view();
        assert!((0..g.nodes.len()).all(|id| v.pos.contains_key(&id)), "every node placed");
        assert!(v.zoom >= MIN_ZOOM && v.zoom <= 1.0, "fitted, got {}", v.zoom);

        let music = g.nodes.iter().position(|n| n.title == "MrxMusic.PlaySpecialMusic").expect("music node");
        apply(Act::Select(vec![music], false), &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);

        apply(Act::Compile, &mut p, &corpus);
        assert!(matches!(p.open.as_ref().expect("open").check, Check::Compiled(_)));

        let before = p.open.as_ref().expect("open").canvas().expect("canvas").nodes.len();
        let entry = p.reference.as_ref().expect("reference").palette.flow[0].clone();
        apply(Act::Add(entry), &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);
        let o = p.open.as_ref().expect("open");
        assert_eq!(o.canvas().expect("canvas").nodes.len(), before + 1);
        assert!(o.view().pos.contains_key(&before), "the new node is placed");
        apply(Act::Undo, &mut p, &corpus);
        frame(&ctx, &mut p, &corpus);
        assert_eq!(p.open.as_ref().expect("open").canvas().expect("canvas").nodes.len(), before);
    }

    /// The layout puts every node somewhere, a chain left to right, and a block below its row.
    #[test]
    fn layout_places_every_node_with_chains_rightward_and_blocks_below() {
        let index = CorpusIndex::build([(
            "lua/vz/x.lua",
            "function F(self)\n  a()\n  if b then\n    c()\n  end\n  d()\nend\n",
        )])
        .expect("indexes");
        let script = Script::parse("function F(self)\n  a()\n  if b then\n    c()\n  end\n  d()\nend\n").expect("parses");
        let chunk = blueprint::lift(&script, &index.context("lua/vz/x.lua"));
        let g = chunk.nodes[1].sub.as_deref().expect("canvas");
        let sizes = vec![Vec2::new(150.0, 80.0); g.nodes.len()];
        let pos = layout(g, &sizes, Vec2::new(1600.0, 900.0));
        assert_eq!(pos.len(), g.nodes.len());
        let at = |t: &str| pos[&g.nodes.iter().position(|n| n.title == t).expect(t)];
        assert!(at("a").x < at("if").x && at("if").x < at("d").x);
        assert_eq!(at("a").y, at("d").y);
        assert!(at("c").y > at("if").y);
    }

    /// A long flat function wraps into rows: the statements keep their reading order (row by row,
    /// left to right), every row starts where the first does, no row starts a statement past the
    /// wrap width, a block hangs under its own row and above the next, and the whole canvas comes
    /// out near the canvas's shape — where one unwrapped row is a strip dozens of times as wide as
    /// it is tall. A function that fits the canvas at 1:1 stays in one row (the test above).
    #[test]
    fn a_long_chain_wraps_into_rows_of_the_canvas_shape() {
        let mut src = String::from("function F(self)\n");
        for i in 0..40 {
            src.push_str(&format!("  s{i}()\n"));
            if i == 3 {
                src.push_str("  if b then\n    c()\n  end\n");
            }
        }
        src.push_str("end\n");
        let index = CorpusIndex::build([("lua/vz/x.lua", src.as_str())]).expect("indexes");
        let script = Script::parse(&src).expect("parses");
        let chunk = blueprint::lift(&script, &index.context("lua/vz/x.lua"));
        let g = chunk.nodes[1].sub.as_deref().expect("canvas");
        let sizes = vec![Vec2::new(150.0, 80.0); g.nodes.len()];
        let id = |t: &str| g.nodes.iter().position(|n| n.title == t).expect(t);
        let shape = |pos: &HashMap<NodeId, Pos2>| {
            let b = (0..g.nodes.len())
                .map(|i| Rect::from_min_size(pos[&i], sizes[i]))
                .reduce(|a, b| a.union(b))
                .expect("nodes");
            b.width() / b.height()
        };

        let canvas = Vec2::new(1600.0, 900.0);
        let pos = layout(g, &sizes, canvas);
        assert_eq!(pos.len(), g.nodes.len());
        let right = wrap_width(&sizes, canvas);
        let stmts: Vec<Pos2> = (0..40).map(|i| pos[&id(&format!("s{i}"))]).collect();
        let rows: std::collections::BTreeSet<i64> = stmts.iter().map(|p| p.y as i64).collect();
        assert!(rows.len() > 1, "the chain wraps");
        for w in stmts.windows(2) {
            assert!(w[1].y > w[0].y || (w[1].y == w[0].y && w[1].x > w[0].x), "reading order: {w:?}");
        }
        let start = stmts[0].x;
        for p in &stmts {
            assert!(p.x >= start && p.x <= right, "{p:?} starts past the wrap width {right}");
        }
        for y in &rows {
            assert!(stmts.iter().any(|p| p.y as i64 == *y && p.x == start), "row {y} starts at the chain's start");
        }
        let (iff, c) = (pos[&id("if")], pos[&id("c")]);
        let next_row = stmts.iter().map(|p| p.y).filter(|y| *y > iff.y).fold(f32::INFINITY, f32::min);
        assert!(c.y > iff.y && c.y < next_row, "the block hangs under its own row: if {iff:?}, c {c:?}, next row {next_row}");
        let wrapped = shape(&pos);
        assert!((1.0..3.0).contains(&wrapped), "wrapped canvas aspect {wrapped}");
        let strip = shape(&layout(g, &sizes, Vec2::new(100_000.0, 100.0)));
        assert!(strip > 10.0, "unwrapped, the same function is a strip: {strip}");
    }
}
