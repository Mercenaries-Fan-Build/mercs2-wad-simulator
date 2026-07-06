//! Destruction state machine reader — turns a model's `HIER` node tree + `SWIT`
//! switch list (+ `INDX` mesh→node map) into a per-node
//! **intact / break_piece / static** classification.
//!
//! Why this exists: the engine never co-renders a destructible's intact body and
//! its break pieces — it switches between them. That switch is `SWIT`, which lists
//! the HIER nodes participating in a destruction swap. A model's `submeshes`
//! attach to HIER nodes via `INDX`, so node-state → submesh-state lets a viewer
//! show one state at a time instead of overlapping everything.
//!
//! Classification rule (deterministic, from real bytes — validated on the
//! resident2 up-crate, see tests):
//! - A **switch group** is a set of sibling HIER nodes that appear in `SWIT` and
//!   share a parent that is *not* in `SWIT` (the group roots).
//! - Within a group, the **break** root is the one whose descendants also appear
//!   in `SWIT` (the individually-addressable break panels); its whole subtree is
//!   `break_piece`. The sibling root whose children are absent from `SWIT` is the
//!   single mesh hidden on damage — its subtree is `intact`.
//! - Every node in no switch-group subtree is `static` (always rendered).
//!
//! PHY2 convex hulls corroborate (the break state should own hulls) but are NOT
//! used for per-node assignment: hull→node bbox-containment is ambiguous (hulls
//! overlap several piece bboxes). Per-node hull mapping is deferred to `SEGM`.

use crate::havok;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructionState {
    /// Always rendered (not in any switch group).
    Static,
    /// The whole, undamaged mesh — hidden when the object is destroyed.
    Intact,
    /// A fragment shown only in the destroyed state.
    BreakPiece,
}

impl DestructionState {
    pub fn as_str(self) -> &'static str {
        match self {
            DestructionState::Static => "static",
            DestructionState::Intact => "intact",
            DestructionState::BreakPiece => "break_piece",
        }
    }
}

/// A HIER node: hash, parent, local transform, and tail bounding box.
#[derive(Debug, Clone)]
pub struct HierNode {
    pub index: usize,
    pub hash: u32,
    pub parent: Option<usize>,
    /// Local 4×4 transform, row-major (row-vector convention: `p' = p · M`).
    pub local: [f32; 16],
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

/// Classification of one HIER node.
#[derive(Debug, Clone)]
pub struct NodeState {
    pub hier_node: usize,
    pub hash: u32,
    pub parent: Option<usize>,
    pub state: DestructionState,
    /// Index of the switch group this node belongs to (None for `static`).
    pub switch_group: Option<usize>,
}

/// Result of reading a model's destruction state machine.
#[derive(Debug, Clone)]
pub struct Destruction {
    pub nodes: Vec<NodeState>,
    pub switch_group_count: usize,
    /// `INDX` mesh-group → HIER node index (parallel to MESH order).
    pub indx: Vec<usize>,
    /// Convex-hull count from the model's PHY2 packfile (corroboration).
    pub hull_count: usize,
    pub warnings: Vec<String>,
}

impl Destruction {
    pub fn state_of_node(&self, node: usize) -> Option<DestructionState> {
        self.nodes.iter().find(|n| n.hier_node == node).map(|n| n.state)
    }
    /// State of the mesh at MESH-order index `mesh_group` (via INDX → node).
    pub fn state_of_mesh(&self, mesh_group: usize) -> Option<DestructionState> {
        self.indx.get(mesh_group).and_then(|&n| self.state_of_node(n))
    }
}

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() { u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) } else { 0 }
}
#[inline]
fn u16_le(b: &[u8], o: usize) -> u16 {
    if o + 2 <= b.len() { u16::from_le_bytes([b[o], b[o + 1]]) } else { 0 }
}
#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    if o + 4 <= b.len() { f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) } else { 0.0 }
}

/// Flat walk of a UCFX container's 20-byte descriptor table, returning every
/// leaf chunk as `(tag, abs_offset, size)`. Header: `data_off @4`, `ndesc @16`;
/// rows at `20 + d*20` = `tag[4], u0[4], size[4], u2[4], u3[4]`; a leaf's data is
/// at `data_off + u0` (u0 == 0xFFFFFFFF marks a container, skipped here).
fn leaf_chunks(buf: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    if buf.len() < 20 {
        return out;
    }
    let data_off = u32_le(buf, 4) as usize;
    let ndesc = u32_le(buf, 16) as usize;
    for d in 0..ndesc {
        let ro = 20 + d * 20;
        if ro + 20 > buf.len() {
            break;
        }
        let tag = [buf[ro], buf[ro + 1], buf[ro + 2], buf[ro + 3]];
        let u0 = u32_le(buf, ro + 4);
        let size = u32_le(buf, ro + 8) as usize;
        if u0 == 0xFFFF_FFFF {
            continue; // container marker
        }
        let abs = data_off + u0 as usize;
        if abs <= buf.len() {
            out.push((tag, abs, size));
        }
    }
    out
}

fn find_chunk<'a>(chunks: &'a [([u8; 4], usize, usize)], tag: &[u8; 4]) -> Option<(usize, usize)> {
    chunks.iter().find(|(t, _, _)| t == tag).map(|(_, o, s)| (*o, *s))
}

/// Parse the first `HIER` chunk into 176-byte node records.
/// Layout: `hash @0 (u32)`, `parent @8 (u16, 0xFFFF=root)`,
/// `tail_bbox_min @144 (f32×3)`, `tail_bbox_max @160 (f32×3)`.
pub fn parse_hier(buf: &[u8]) -> Vec<HierNode> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"HIER") else {
        return Vec::new();
    };
    let n = size / 176;
    (0..n)
        .map(|i| {
            let o = off + i * 176;
            let parent = u16_le(buf, o + 8);
            let mut local = [0.0f32; 16];
            for (k, m) in local.iter_mut().enumerate() {
                *m = f32_le(buf, o + 16 + k * 4); // 4×4 local transform @ +16
            }
            HierNode {
                index: i,
                hash: u32_le(buf, o),
                parent: (parent != 0xFFFF).then_some(parent as usize),
                local,
                bbox_min: [f32_le(buf, o + 144), f32_le(buf, o + 148), f32_le(buf, o + 152)],
                bbox_max: [f32_le(buf, o + 160), f32_le(buf, o + 164), f32_le(buf, o + 168)],
            }
        })
        .collect()
}

// ── The engine's destruction STATE MACHINE (FUN_004cf340 @0x004cf340 — the exe's only SWIT
// consumer; recovered layout in docs/destruction_orchestrator_format.md). NOT a heuristic:
// per switch node, NAMED states with explicit Enter/Exit u32 lists. ──

/// `pandemic_hash_m2("Enter")` — CHDR list selector for a state's Enter list.
pub const LIST_ENTER: u32 = 0x9DA9_7065;
/// `pandemic_hash_m2("Exit")` — CHDR list selector for a state's Exit list.
pub const LIST_EXIT: u32 = 0xDB41_017D;

/// One named state of a switch node.
#[derive(Debug, Clone, Default)]
pub struct StateDef {
    pub name_hash: u32,
    pub enter: Vec<u32>,
    pub exit: Vec<u32>,
}

/// One switch node: its name hash + named states, in authored order.
#[derive(Debug, Clone, Default)]
pub struct SwitchNodeDef {
    pub name_hash: u32,
    pub states: Vec<StateDef>,
}

/// The parsed state machine.
#[derive(Debug, Clone, Default)]
pub struct StateMachine {
    /// The `SWIT` per-slot table (`INFO.switch_count` u32s).
    pub switch_slots: Vec<u32>,
    pub nodes: Vec<SwitchNodeDef>,
}

/// One UCFX descriptor row (20 bytes at `20 + i*20`): tag, data offset (`0xFFFFFFFF` =
/// container), size, `u2` (valid/flags), `u3` (descendant row count — the engine's walker skips
/// subtrees with `next = i + u3 + 1`).
struct DescRow {
    tag: [u8; 4],
    u0: u32,
    size: u32,
    u3: u32,
}

fn desc_rows(buf: &[u8]) -> (usize, Vec<DescRow>) {
    let mut rows = Vec::new();
    if buf.len() < 20 {
        return (0, rows);
    }
    let data_off = u32_le(buf, 4) as usize;
    let ndesc = u32_le(buf, 16) as usize;
    for d in 0..ndesc {
        let ro = 20 + d * 20;
        if ro + 20 > buf.len() {
            break;
        }
        rows.push(DescRow {
            tag: [buf[ro], buf[ro + 1], buf[ro + 2], buf[ro + 3]],
            u0: u32_le(buf, ro + 4),
            size: u32_le(buf, ro + 8),
            u3: u32_le(buf, ro + 16),
        });
    }
    (data_off, rows)
}

/// Parse the destruction state machine from a container, mirroring `FUN_004cf340`: find the
/// container row whose IMMEDIATE children carry `NODE`/`STAT` chunks, then dispatch those
/// children in authored order (the engine walks siblings with the `u3`-skip). Returns `None`
/// when the container carries no such family (non-destructible models).
pub fn parse_state_machine(buf: &[u8]) -> Option<StateMachine> {
    let (data_off, rows) = desc_rows(buf);
    if rows.is_empty() {
        return None;
    }
    // Immediate children of row `p`: p+1, then advance past each child's subtree (u3 rows).
    let children_of = |p: usize| -> Vec<usize> {
        let end = (p + rows[p].u3 as usize + 1).min(rows.len());
        let mut out = Vec::new();
        let mut i = p + 1;
        while i < end {
            out.push(i);
            i += rows[i].u3 as usize + 1;
        }
        out
    };
    // The family parent: the container whose immediate children include a NODE row.
    let parent = (0..rows.len()).find(|&p| {
        rows[p].u3 > 0 && children_of(p).iter().any(|&c| &rows[c].tag == b"NODE")
    })?;

    let mut sm = StateMachine::default();
    let mut switch_count = 0usize;
    // Parser context: the list the next CEXE fills (true = Enter) + its expected count.
    let mut pending: Option<(bool, usize)> = None;
    for c in children_of(parent) {
        let r = &rows[c];
        if r.u0 == 0xFFFF_FFFF {
            continue; // nested container — the engine reads only leaf data here
        }
        let start = data_off + r.u0 as usize;
        let end = (start + r.size as usize).min(buf.len());
        if start > end {
            continue;
        }
        let d = &buf[start..end];
        match &r.tag {
            b"INFO" if d.len() >= 12 => {
                // [u32 skipped, u32 switch_count, u32 node_count]
                switch_count = u32_le(d, 4) as usize;
            }
            b"NODE" if d.len() >= 8 => {
                sm.nodes.push(SwitchNodeDef {
                    name_hash: u32_le(d, 0),
                    states: Vec::with_capacity(u32_le(d, 4) as usize),
                });
            }
            b"STAT" if d.len() >= 4 => {
                if let Some(n) = sm.nodes.last_mut() {
                    n.states.push(StateDef { name_hash: u32_le(d, 0), ..Default::default() });
                }
            }
            b"CHDR" if d.len() >= 8 => {
                let which = u32_le(d, 0);
                let count = u32_le(d, 4) as usize;
                match which {
                    LIST_ENTER => pending = Some((true, count)),
                    LIST_EXIT => pending = Some((false, count)),
                    _ => pending = None,
                }
            }
            b"CEXE" => {
                if let Some((enter, count)) = pending.take() {
                    let n = (d.len() / 4).min(count);
                    let list: Vec<u32> = (0..n).map(|i| u32_le(d, i * 4)).collect();
                    if let Some(st) = sm.nodes.last_mut().and_then(|nd| nd.states.last_mut()) {
                        if enter {
                            st.enter = list;
                        } else {
                            st.exit = list;
                        }
                    }
                }
            }
            b"SWIT" => {
                let n = if switch_count > 0 { switch_count.min(d.len() / 4) } else { d.len() / 4 };
                sm.switch_slots = (0..n).map(|i| u32_le(d, i * 4)).collect();
            }
            _ => {}
        }
    }
    (!sm.nodes.is_empty()).then_some(sm)
}

/// Decode a state's Enter/Exit COMMAND SCRIPT into readable calls. Token grammar (observed on
/// retail vehicles, e.g. `al_veh_truck_hmmwv_avenger`):
/// `0x1 <arg>` pushes an argument, `0x2 <command>` invokes the command with the pushed args,
/// `0x3` ends the script. Commands/args are m2 hashes (SHOW/Hide/SetState/StartEmitter/
/// StopEmitter/PropTemplate/KILL, effect names, HIER node hashes, state hashes) — `resolve`
/// maps hash → display name.
pub fn decode_script(list: &[u32], resolve: impl Fn(u32) -> String) -> String {
    let mut calls: Vec<String> = Vec::new();
    let mut args: Vec<String> = Vec::new();
    let mut i = 0;
    while i < list.len() {
        match list[i] {
            1 if i + 1 < list.len() => {
                args.push(resolve(list[i + 1]));
                i += 2;
            }
            2 if i + 1 < list.len() => {
                calls.push(format!("{}({})", resolve(list[i + 1]), args.join(", ")));
                args.clear();
                i += 2;
            }
            3 => i += 1,
            other => {
                args.push(resolve(other));
                i += 1;
            }
        }
    }
    if !args.is_empty() {
        calls.push(format!("?({})", args.join(", ")));
    }
    calls.join("; ")
}

/// The default state of a switch node, resolved from the GAME DATA: every observed machine's
/// first state is an init stub whose enter script is `SetState(<target>, self)` — follow it.
/// Falls back to state 0 when the pattern is absent.
pub fn default_state_index(node: &SwitchNodeDef) -> usize {
    let setstate = crate::hash::pandemic_hash_m2("setstate");
    let Some(first) = node.states.first() else { return 0 };
    let mut args: Vec<u32> = Vec::new();
    let l = &first.enter;
    let mut i = 0;
    while i < l.len() {
        match l[i] {
            1 if i + 1 < l.len() => {
                args.push(l[i + 1]);
                i += 2;
            }
            2 if i + 1 < l.len() => {
                if l[i + 1] == setstate {
                    if let Some(&target) = args.first() {
                        if let Some(pos) = node.states.iter().position(|s| s.name_hash == target) {
                            return pos;
                        }
                    }
                }
                args.clear();
                i += 2;
            }
            _ => i += 1,
        }
    }
    0
}

/// GROUND-TRUTH per-mesh-group visibility from the engine state machine — no classification
/// heuristics: every `SWIT` participant subtree starts hidden, then each switch node's CHOSEN
/// state executes its enter-script `SHOW`/`Hide` commands over the HIER; mesh groups map through
/// `INDX`. `chosen[i]` = state index for `sm.nodes[i]` (see [`default_state_index`]).
pub fn machine_group_visibility(
    sm: &StateMachine,
    hier: &[HierNode],
    indx: &[usize],
    chosen: &[usize],
) -> Vec<bool> {
    let show = crate::hash::pandemic_hash_m2("show");
    let hide = crate::hash::pandemic_hash_m2("hide");
    let hash_to_idx: std::collections::HashMap<u32, usize> =
        hier.iter().map(|h| (h.hash, h.index)).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); hier.len()];
    for h in hier {
        if let Some(p) = h.parent {
            if p < hier.len() {
                children[p].push(h.index);
            }
        }
    }
    fn mark(children: &[Vec<usize>], hidden: &mut [bool], root: usize, v: bool) {
        let mut stack = vec![root];
        while let Some(x) = stack.pop() {
            if x < hidden.len() {
                hidden[x] = v;
                stack.extend_from_slice(&children[x]);
            }
        }
    }
    let mut hidden = vec![false; hier.len()];
    for &slot in &sm.switch_slots {
        if let Some(&i) = hash_to_idx.get(&slot) {
            mark(&children, &mut hidden, i, true);
        }
    }
    for (ni, node) in sm.nodes.iter().enumerate() {
        let si = chosen
            .get(ni)
            .copied()
            .unwrap_or(0)
            .min(node.states.len().saturating_sub(1));
        let Some(st) = node.states.get(si) else { continue };
        let mut args: Vec<u32> = Vec::new();
        let l = &st.enter;
        let mut i = 0;
        while i < l.len() {
            match l[i] {
                1 if i + 1 < l.len() => {
                    args.push(l[i + 1]);
                    i += 2;
                }
                2 if i + 1 < l.len() => {
                    let cmd = l[i + 1];
                    if cmd == show || cmd == hide {
                        for a in &args {
                            if let Some(&idx) = hash_to_idx.get(a) {
                                mark(&children, &mut hidden, idx, cmd == hide);
                            }
                        }
                    }
                    args.clear();
                    i += 2;
                }
                3 => i += 1,
                _ => {
                    args.push(l[i]);
                    i += 1;
                }
            }
        }
    }
    indx.iter().map(|&n| hidden.get(n).map(|h| !h).unwrap_or(true)).collect()
}

/// Parse the first `SWIT` chunk as a flat u32 node-hash list.
pub fn parse_swit(buf: &[u8]) -> Vec<u32> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"SWIT") else {
        return Vec::new();
    };
    (0..size / 4).map(|i| u32_le(buf, off + i * 4)).collect()
}

/// Parse the first `INDX` chunk as a u16 array (MESH-group order → HIER node index).
pub fn parse_indx(buf: &[u8]) -> Vec<usize> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"INDX") else {
        return Vec::new();
    };
    (0..size / 2).map(|i| u16_le(buf, off + i * 2) as usize).collect()
}

/// Parse `SEGM` → the **collision/segment node indices** in first-appearance
/// order. SEGM is a list of 4-byte records whose first byte is a HIER node index;
/// the distinct nodes referenced are exactly those that own a PHY2 collision hull
/// (reverse-engineered + validated: the crate references `{2,4,5,6,7,8}` = its 6
/// hulls). Each record is `{u8 node, u8 0, u8 seg, u8 type}`.
pub fn parse_segm(buf: &[u8]) -> Vec<usize> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"SEGM") else {
        return Vec::new();
    };
    let mut out: Vec<usize> = Vec::new();
    for i in 0..size / 4 {
        let node = buf.get(off + i * 4).copied().unwrap_or(0) as usize;
        if !out.contains(&node) {
            out.push(node);
        }
    }
    out
}

// ── transforms ───────────────────────────────────────────────────────────────

/// Row-major 4×4 multiply, row-vector convention: returns `a · b`.
fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut r = [0.0f32; 16];
    for row in 0..4 {
        for col in 0..4 {
            r[row * 4 + col] = (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum();
        }
    }
    r
}

/// Transform a point by a row-major matrix (row-vector: `p' = [p,1] · M`).
fn transform_point(p: [f32; 3], m: &[f32; 16]) -> [f32; 3] {
    [
        p[0] * m[0] + p[1] * m[4] + p[2] * m[8] + m[12],
        p[0] * m[1] + p[1] * m[5] + p[2] * m[9] + m[13],
        p[0] * m[2] + p[1] * m[6] + p[2] * m[10] + m[14],
    ]
}

/// World transform per node = `local · parent_world` (HIER is parent-ordered).
fn world_matrices(hier: &[HierNode]) -> Vec<[f32; 16]> {
    let mut world = vec![[0.0f32; 16]; hier.len()];
    for (i, node) in hier.iter().enumerate() {
        world[i] = match node.parent {
            Some(p) if p < i => mat4_mul(&node.local, &world[p]),
            _ => node.local,
        };
    }
    world
}

/// A collision hull placed in model space (verts transformed by its HIER node).
#[derive(Debug, Clone)]
pub struct GroundedHull {
    /// HIER node this hull belongs to.
    pub node: usize,
    pub vertices: Vec<[f32; 3]>,
}

/// Decode a model container's PHY2 hulls and place each in model space using its
/// owning HIER node's world transform. The hull→node map comes from SEGM: the
/// collision nodes (descending index) correspond to the PHY2 hull order. Returns
/// empty if the container has no SEGM/HIER/PHY2 (e.g. a non-destructible model).
pub fn grounded_hulls(buf: &[u8]) -> Vec<GroundedHull> {
    let hier = parse_hier(buf);
    let mut collision = parse_segm(buf);
    if hier.is_empty() || collision.is_empty() {
        return Vec::new();
    }
    collision.sort_unstable_by(|a, b| b.cmp(a)); // descending: hull[i] → collision[i]
    let world = world_matrices(&hier);
    let packfiles = havok::find_packfiles(buf);
    let mut out = Vec::new();
    let mut hi = 0;
    for (_, pf) in &packfiles {
        for hull in pf.hulls() {
            let node = collision.get(hi).copied().unwrap_or(0).min(hier.len().saturating_sub(1));
            let m = world.get(node).copied().unwrap_or_else(|| {
                let mut id = [0.0; 16];
                id[0] = 1.0; id[5] = 1.0; id[10] = 1.0; id[15] = 1.0;
                id
            });
            out.push(GroundedHull {
                node,
                vertices: hull.vertices.iter().map(|v| transform_point(*v, &m)).collect(),
            });
            hi += 1;
        }
    }
    out
}

/// Read a model container's destruction state machine. Returns `None` if the
/// container has no `SWIT` (a non-destructible model — caller treats all as static).
pub fn classify(buf: &[u8]) -> Option<Destruction> {
    let hier = parse_hier(buf);
    let swit = parse_swit(buf);
    if hier.is_empty() || swit.is_empty() {
        return None;
    }

    let mut warnings = Vec::new();
    let n = hier.len();
    let hash_to_idx: std::collections::HashMap<u32, usize> =
        hier.iter().map(|h| (h.hash, h.index)).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for h in &hier {
        if let Some(p) = h.parent {
            if p < n {
                children[p].push(h.index);
            }
        }
    }
    // SWIT node indices present in this HIER.
    let swit_idx: std::collections::HashSet<usize> =
        swit.iter().filter_map(|w| hash_to_idx.get(w).copied()).collect();

    let subtree = |root: usize| -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        while let Some(x) = stack.pop() {
            out.push(x);
            stack.extend_from_slice(&children[x]);
        }
        out
    };

    // Switch-group roots: in SWIT, parent not in SWIT. Group by parent.
    let mut by_parent: std::collections::BTreeMap<Option<usize>, Vec<usize>> = Default::default();
    for &i in &swit_idx {
        if hier[i].parent.map_or(true, |p| !swit_idx.contains(&p)) {
            by_parent.entry(hier[i].parent).or_default().push(i);
        }
    }

    let mut state = vec![(DestructionState::Static, None::<usize>); n];
    for (group, (_parent, roots)) in by_parent.iter().enumerate() {
        // Break root = the one with the most descendants also in SWIT.
        let break_root = roots
            .iter()
            .copied()
            .max_by_key(|&r| subtree(r).into_iter().filter(|x| *x != r && swit_idx.contains(x)).count());
        for &r in roots {
            let s = if Some(r) == break_root {
                DestructionState::BreakPiece
            } else {
                DestructionState::Intact
            };
            for x in subtree(r) {
                state[x] = (s, Some(group));
            }
        }
        if roots.len() == 1 {
            warnings.push(format!(
                "switch group {group} has a single root (node {}) — no intact/break sibling pair",
                roots[0]
            ));
        }
    }

    let nodes = hier
        .iter()
        .map(|h| NodeState {
            hier_node: h.index,
            hash: h.hash,
            parent: h.parent,
            state: state[h.index].0,
            switch_group: state[h.index].1,
        })
        .collect();

    // PHY2 corroboration: total convex hulls in the model's packfile(s).
    let hull_count: usize = havok::find_packfiles(buf)
        .iter()
        .map(|(_, pf)| pf.hulls().count())
        .sum();
    let break_nodes = state.iter().filter(|(s, _)| *s == DestructionState::BreakPiece).count();
    if hull_count > 0 && break_nodes == 0 {
        warnings.push(format!("{hull_count} PHY2 hulls but no break_piece nodes classified"));
    }

    Some(Destruction {
        nodes,
        switch_group_count: by_parent.len(),
        indx: parse_indx(buf),
        hull_count,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground truth on the resident2 up-crate container: HIER tree
    /// `node1 → {node2→(9,10,11,12)}, {node3→(4..8)}`; SWIT lists
    /// `[node2, node3, node4..8]`. So node2 subtree = intact, node3 subtree =
    /// break_piece, node0/node1 = static; 6 PHY2 hulls corroborate the break state.
    #[test]
    fn crate_swit_classifies_intact_vs_break() {
        let buf = include_bytes!("../tests/fixtures/crate_container_le.bin");
        let d = classify(buf).expect("crate has a SWIT switch group");

        assert_eq!(d.switch_group_count, 1);
        assert_eq!(d.hull_count, 6, "6 break-piece PHY2 hulls");
        assert!(d.warnings.is_empty(), "warnings: {:?}", d.warnings);

        let st = |i: usize| d.state_of_node(i).unwrap();
        assert_eq!(st(0), DestructionState::Static);
        assert_eq!(st(1), DestructionState::Static);
        assert_eq!(st(2), DestructionState::Intact);
        for i in [9, 10, 11, 12] {
            assert_eq!(st(i), DestructionState::Intact, "node{i} (node2 subtree) is intact");
        }
        assert_eq!(st(3), DestructionState::BreakPiece);
        for i in [4, 5, 6, 7, 8] {
            assert_eq!(st(i), DestructionState::BreakPiece, "node{i} (node3 subtree) is a break piece");
        }
        // every intact/break node carries the same switch group; static carries none.
        assert_eq!(d.nodes[4].switch_group, Some(0));
        assert_eq!(d.nodes[0].switch_group, None);
    }

    #[test]
    fn non_destructible_returns_none() {
        // A buffer with no SWIT chunk → not a destruction orchestrator.
        assert!(classify(&[0u8; 64]).is_none());
    }

    /// SEGM names the crate's 6 collision nodes {2,4,5,6,7,8}; grounding each
    /// PHY2 hull by its HIER node world transform places all 6 inside the render
    /// crate (4 side panels + lid + floor + intact body) — the reversed solution.
    #[test]
    fn crate_segm_grounds_all_hulls() {
        let buf = include_bytes!("../tests/fixtures/crate_container_le.bin");

        let mut nodes = parse_segm(buf);
        nodes.sort_unstable();
        assert_eq!(nodes, vec![2, 4, 5, 6, 7, 8], "SEGM collision nodes");

        let g = grounded_hulls(buf);
        assert_eq!(g.len(), 6, "6 grounded hulls");
        // hull[i] → descending collision node: 8,7,6,5,4,2
        assert_eq!(g.iter().map(|h| h.node).collect::<Vec<_>>(), vec![8, 7, 6, 5, 4, 2]);
        // every grounded vertex sits within the crate render AABB (±small margin)
        for h in &g {
            for v in &h.vertices {
                assert!(v[0] >= -0.98 && v[0] <= 0.98, "x out (node {}): {v:?}", h.node);
                assert!(v[1] >= -0.12 && v[1] <= 1.2, "y out (node {}): {v:?}", h.node);
                assert!(v[2] >= -0.62 && v[2] <= 0.62, "z out (node {}): {v:?}", h.node);
            }
        }
    }
}
