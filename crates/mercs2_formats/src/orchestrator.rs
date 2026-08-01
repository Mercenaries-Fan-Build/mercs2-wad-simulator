//! Destruction reader: the engine's own per-model **state machine** (`parse_state_machine` —
//! `SWIT` + `NODE`/`STAT`/`CHDR`/`CEXE`, mirroring `FUN_004cf340`), plus a legacy heuristic
//! classifier kept only for callers that predate it.
//!
//! Why this exists: the engine never co-renders a destructible's intact body and its wreck — it
//! switches between them, by running a named state's enter-script (`SHOW`/`HIDE`, which act on whole
//! HIER **subtrees**). `PristineState` (`0xACB51200`) shows the intact body (`0x255EAB53`);
//! `DestroyedState` (`0x7687DF41`) shows the wreck body (`0x75F1F74D`), which is geometry in the
//! container. Use `parse_state_machine` + `machine_node_enable` for anything real.
//!
//! ★**The `intact / break_piece / static` classification below is a HEURISTIC and is SUPERSEDED.**
//! It was inferred from `SWIT` sibling structure before the real machine was recovered. It has no
//! notion of health, states, or messages, and its "static = always rendered" category is misleading
//! — a node the machine never names is still hidden as a *child* of a governed parent. Do not build
//! on it. See `docs/modernization/vehicle_model_spec.md` §5.
//!
//! ★Note also: the doc-comment's "`INDX` mesh→node map" is wrong — `INDX` is keyed by **sub-object**
//! ordinal and yields a **seg_id into `SEGM`**, whose `bone` is the node. See `vehicle_model_spec.md`
//! §2 and `model_cubeize::read_model_meshes_segm`.
//!
//! Legacy classification rule (deterministic, from real bytes — validated on the
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
        self.nodes
            .iter()
            .find(|n| n.hier_node == node)
            .map(|n| n.state)
    }
    /// State of the mesh at MESH-order index `mesh_group` (via INDX → node).
    pub fn state_of_mesh(&self, mesh_group: usize) -> Option<DestructionState> {
        self.indx
            .get(mesh_group)
            .and_then(|&n| self.state_of_node(n))
    }
}

#[inline]
fn u32_le(b: &[u8], o: usize) -> u32 {
    if o + 4 <= b.len() {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0
    }
}
#[inline]
fn u16_le(b: &[u8], o: usize) -> u16 {
    if o + 2 <= b.len() {
        u16::from_le_bytes([b[o], b[o + 1]])
    } else {
        0
    }
}
#[inline]
fn f32_le(b: &[u8], o: usize) -> f32 {
    if o + 4 <= b.len() {
        f32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    } else {
        0.0
    }
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
    chunks
        .iter()
        .find(|(t, _, _)| t == tag)
        .map(|(_, o, s)| (*o, *s))
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
                bbox_min: [
                    f32_le(buf, o + 144),
                    f32_le(buf, o + 148),
                    f32_le(buf, o + 152),
                ],
                bbox_max: [
                    f32_le(buf, o + 160),
                    f32_le(buf, o + 164),
                    f32_le(buf, o + 168),
                ],
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

/// The GLOBAL destruction-state vocabulary — the hashes the engine's `SetState` / `SetStateOnMsg`
/// (`FUN_004d3e10`) key on, identical across every destructible. Cracked; **do not re-derive**
/// (`docs/modernization/vehicle_model_spec.md` §5, `docs/destruction_orchestrator_format.md`).
///
/// This matters for editing: a state's `name_hash` is not a per-model label, it is this shared
/// address. Rename a state to a hash outside this set and the damage system's transitions no longer
/// reach it — the state becomes dead. Some members are observed-but-not-yet-named (`""`); they are
/// still real states, so an edit that uses one is fine and only a hash outside the whole set warns.
pub const STATE_VOCABULARY: &[(u32, &str)] = &[
    (0x0ACE_072A, "InitState"),
    (0x5D30_8F4F, "InitDestroyedState"),
    (0x5A6E_8927, "InitDamagedState"),
    (0xACB5_1200, "PristineState"),
    (0x1D55_75A1, "DamagedState"),
    (0x9279_1EBB, "StartDestroyedState"),
    (0x7687_DF41, "DestroyedState"),
    (0xCA26_1E5B, "GoneState"),
    // Observed across retail machines, not yet name-cracked — real states all the same.
    (0x381B_E6A4, ""),
    (0xCE60_3754, ""),
    (0xA530_B827, ""),
];

/// The cracked name of a global destruction state, or `None` for a hash outside the vocabulary (or
/// an observed-but-unnamed member).
pub fn state_name(hash: u32) -> Option<&'static str> {
    STATE_VOCABULARY
        .iter()
        .find(|(h, n)| *h == hash && !n.is_empty())
        .map(|(_, n)| *n)
}

/// Is this hash a known global destruction state at all (named or not)? An edit that names a state
/// outside this set has decoupled it from the engine's `SetState` transitions.
pub fn is_known_state(hash: u32) -> bool {
    STATE_VOCABULARY.iter().any(|(h, _)| *h == hash)
}

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
    let parent = (0..rows.len())
        .find(|&p| rows[p].u3 > 0 && children_of(p).iter().any(|&c| &rows[c].tag == b"NODE"))?;

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
                    n.states.push(StateDef {
                        name_hash: u32_le(d, 0),
                        ..Default::default()
                    });
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
                let n = if switch_count > 0 {
                    switch_count.min(d.len() / 4)
                } else {
                    d.len() / 4
                };
                sm.switch_slots = (0..n).map(|i| u32_le(d, i * 4)).collect();
            }
            _ => {}
        }
    }
    (!sm.nodes.is_empty()).then_some(sm)
}

/// Re-emit a model container with its destruction state machine REGENERATED from `sm`.
///
/// The whole family is rebuilt every time — descriptor rows and leaf data both — from the parsed
/// model, rather than overlaid onto the existing leaves. That is what lets an edit **add or remove**
/// nodes and states, not only rename or rewrite them: a shape change is just a different number of
/// generated leaves, and the container-subtree splice (grow the descriptor table, re-base `data_off`,
/// bump the family parent's and every ancestor's descendant count, re-tile the data, recompute the
/// `CSUM`) is mechanical over the flat, contiguous family the survey established.
///
/// The regeneration follows retail's exact canonical layout, measured across all 1,311 destructibles
/// (`tests/state_machine_layout_survey.rs`): `INFO[5, switch_count, node_count]`; `SWIT` either right
/// after `INFO` or last (its original position is preserved); per node a `NODE[hash, state_count]`;
/// per state a `STAT[hash]`, always an Enter `CHDR`+`CEXE`, and an Exit `CHDR`+`CEXE` **iff** the exit
/// list is non-empty (no retail state has an empty Exit, or an empty Enter). Because that layout is
/// exactly what retail emits, a no-op — `sm` parsed straight back out — reproduces the container
/// byte-for-byte (proven in `tests/state_machine_writer.rs`).
pub fn serialize_state_machine(original: &[u8], sm: &StateMachine) -> Result<Vec<u8>, String> {
    let (data_off, rows) = desc_rows(original);
    if rows.is_empty() {
        return Err("container has no descriptor table".into());
    }
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
    let parent = (0..rows.len())
        .find(|&p| rows[p].u3 > 0 && children_of(p).iter().any(|&c| &rows[c].tag == b"NODE"))
        .ok_or("container carries no destruction family")?;

    // The family occupies a CONSECUTIVE run of descriptor rows: the parent's children are all leaves
    // (u3 == 0), so `children_of` advances by one each step and they are `parent+1 ..= parent+u3`.
    let old_n = rows[parent].u3 as usize;
    let fam_first = parent + 1;
    let fam_last = parent + old_n;
    if fam_last >= rows.len() {
        return Err("destruction family runs past the descriptor table".into());
    }
    let fam_rows = &rows[fam_first..=fam_last];
    if fam_rows.iter().any(|r| r.u0 == 0xFFFF_FFFF) {
        return Err("destruction family nests a container — this writer models only flat families".into());
    }

    // SWIT position: retail puts it either right after INFO (ordinal 1) or last. Preserve it; a
    // family with no SWIT leaf gets none.
    let swit_ordinal = fam_rows.iter().position(|r| &r.tag == b"SWIT");
    let swit_first = swit_ordinal == Some(1);
    let has_swit = swit_ordinal.is_some();

    // Regenerate the leaves in canonical order.
    let u32v = |v: u32| v.to_le_bytes().to_vec();
    let words = |list: &[u32]| -> Vec<u8> { list.iter().flat_map(|w| w.to_le_bytes()).collect() };
    let swit_leaf = || ([b'S', b'W', b'I', b'T'], words(&sm.switch_slots));
    let mut new_leaves: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    // INFO = [5, switch_count, node_count]; word0 is the constant the survey pinned.
    let mut info = u32v(5);
    info.extend_from_slice(&(sm.switch_slots.len() as u32).to_le_bytes());
    info.extend_from_slice(&(sm.nodes.len() as u32).to_le_bytes());
    new_leaves.push(([b'I', b'N', b'F', b'O'], info));
    if has_swit && swit_first {
        new_leaves.push(swit_leaf());
    }
    for node in &sm.nodes {
        let mut n = u32v(node.name_hash);
        n.extend_from_slice(&(node.states.len() as u32).to_le_bytes());
        new_leaves.push(([b'N', b'O', b'D', b'E'], n));
        for st in &node.states {
            new_leaves.push(([b'S', b'T', b'A', b'T'], u32v(st.name_hash)));
            // A list emits its CHDR/CEXE leaf IFF it is non-empty — retail's exact rule, measured
            // across 82,790 states (0 leaves with an empty list, 31,311 states with no leaves at
            // all). Enter always precedes Exit.
            for (selector, list) in [(LIST_ENTER, &st.enter), (LIST_EXIT, &st.exit)] {
                if list.is_empty() {
                    continue;
                }
                let mut chdr = u32v(selector);
                chdr.extend_from_slice(&(list.len() as u32).to_le_bytes());
                new_leaves.push(([b'C', b'H', b'D', b'R'], chdr));
                new_leaves.push(([b'C', b'E', b'X', b'E'], words(list)));
            }
        }
    }
    if has_swit && !swit_first {
        new_leaves.push(swit_leaf());
    }
    let new_n = new_leaves.len();
    let row_delta = new_n as i64 - old_n as i64;

    // The original family's contiguous byte span, and the new one re-tiled from the same start.
    let fam_lo = fam_rows.iter().map(|r| r.u0 as usize).min().unwrap();
    let fam_hi = fam_rows.iter().map(|r| r.u0 as usize + r.size as usize).max().unwrap();
    let old_fam_len = fam_hi - fam_lo;
    let mut new_family_data = Vec::new();
    let mut new_offsets: Vec<(u32, u32)> = Vec::with_capacity(new_n); // (off, size), row-relative
    let mut cursor = fam_lo;
    for (_, data) in &new_leaves {
        new_offsets.push((cursor as u32, data.len() as u32));
        new_family_data.extend_from_slice(data);
        cursor += data.len();
    }
    let data_delta = new_family_data.len() as i64 - old_fam_len as i64;

    let has_csum = original.len() >= 8 && &original[original.len() - 8..original.len() - 4] == b"CSUM";
    let data_end = if has_csum { original.len() - 8 } else { original.len() };
    if data_off > data_end || fam_hi > data_end - data_off {
        return Err("data offset/family runs past the container's data region".into());
    }

    // A leaf whose data sits AFTER the family shifts by the family's size delta; anything before is
    // untouched; a container row (0xFFFFFFFF) has no data offset.
    let remap_off = |off: u32| -> u32 {
        if off != 0xFFFF_FFFF && off as usize >= fam_hi {
            (off as i64 + data_delta) as u32
        } else {
            off
        }
    };
    // `i` is an ancestor of the family parent when the parent lies inside its subtree.
    let is_ancestor = |i: usize| i < parent && parent <= i + rows[i].u3 as usize;

    let new_ndesc = (rows.len() as i64 + row_delta) as usize;
    let new_data_off = (data_off as i64 + 20 * row_delta) as usize;

    // ── Header: UCFX, new data offset, the two words we do not model, new descriptor count. ──
    let mut out = Vec::with_capacity(new_data_off + new_family_data.len() + 8);
    out.extend_from_slice(&original[0..4]);
    out.extend_from_slice(&(new_data_off as u32).to_le_bytes());
    out.extend_from_slice(&original[8..16]);
    out.extend_from_slice(&(new_ndesc as u32).to_le_bytes());

    // Copy an original descriptor row, patching its data offset and (for the parent / an ancestor)
    // its descendant count. Its `w12` sibling-count word is untouched — inserting into the family
    // subtree changes no other row's sibling count.
    let emit_original = |out: &mut Vec<u8>, i: usize| {
        let ro = 20 + i * 20;
        let mut row: [u8; 20] = original[ro..ro + 20].try_into().unwrap();
        let off = u32_le(&row, 4);
        row[4..8].copy_from_slice(&remap_off(off).to_le_bytes());
        if i == parent {
            row[16..20].copy_from_slice(&(new_n as u32).to_le_bytes());
        } else if is_ancestor(i) {
            let u3 = (rows[i].u3 as i64 + row_delta) as u32;
            row[16..20].copy_from_slice(&u3.to_le_bytes());
        }
        out.extend_from_slice(&row);
    };

    for i in 0..=parent {
        emit_original(&mut out, i);
    }
    // New family rows: siblings at the parent's child level, so w12 = (count-1-ordinal), u3 = 0.
    for (j, ((tag, _), (off, size))) in new_leaves.iter().zip(&new_offsets).enumerate() {
        out.extend_from_slice(tag);
        out.extend_from_slice(&off.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&((new_n - 1 - j) as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
    for i in fam_last + 1..rows.len() {
        emit_original(&mut out, i);
    }

    // Any padding between the descriptor table and the data region is preserved verbatim.
    let orig_table_end = 20 + rows.len() * 20;
    if data_off > orig_table_end {
        out.extend_from_slice(&original[orig_table_end..data_off]);
    }
    debug_assert_eq!(out.len(), new_data_off, "descriptor table + padding must land at data_off");

    // Data region: untouched head, the regenerated family, untouched tail.
    let region = &original[data_off..data_end];
    out.extend_from_slice(&region[..fam_lo]);
    out.extend_from_slice(&new_family_data);
    out.extend_from_slice(&region[fam_hi..]);

    if has_csum {
        let csum = crate::crc32::crc32_mercs2(&out);
        out.extend_from_slice(b"CSUM");
        out.extend_from_slice(&csum.to_le_bytes());
    }
    Ok(out)
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
    let Some(first) = node.states.first() else {
        return 0;
    };
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

// ── Health-driven destruction: run the state machine as the engine does, from HEALTH. ───────────
// The engine drives each switch node through its state graph via DAMAGE MESSAGES (SetStateOnMsg),
// against native Health/RuntimeNodeHealth components. We reconstruct that: given how much damage has
// been dealt (a health fraction), deliver the corresponding messages and follow the transitions to
// the state each node lands in. This is the real mechanism; only the HP→which-message THRESHOLDS are
// approximated (the per-hit HP math is string-only/live-only — see object_assembly_model.md §7).

/// Global destruction state vocabulary — shared across all vehicles (verified on the live tank machine
/// + the jul08 devkit strings). Terminal = the object is wrecked; pristine = full health.
pub const STATE_PRISTINE: u32 = 0xACB5_1200;
pub const STATE_WRECK: u32 = 0x9279_1EBB;
pub const STATE_TERMINAL: u32 = 0xCA26_1E5B;

/// Decode a state enter/exit script (`0x1`=arg, `0x2`=cmd, `0x3`=end) into `(command_hash, args)`.
fn commands(script: &[u32]) -> Vec<(u32, Vec<u32>)> {
    let mut out = Vec::new();
    let mut args: Vec<u32> = Vec::new();
    let mut i = 0;
    while i < script.len() {
        match script[i] {
            1 if i + 1 < script.len() => {
                args.push(script[i + 1]);
                i += 2;
            }
            2 if i + 1 < script.len() => {
                out.push((script[i + 1], std::mem::take(&mut args)));
                i += 2;
            }
            3 => i += 1,
            _ => {
                args.push(script[i]);
                i += 1;
            }
        }
    }
    out
}

fn is_terminal(target: u32) -> bool {
    target == STATE_WRECK || target == STATE_TERMINAL
}

/// The state one switch node settles in after `delivered` damage messages have fired. Starts at the
/// node's pristine state and follows transitions until stable, with engine-plausible priority:
/// a delivered terminal transition (→ wreck) wins over an unconditional passthrough `SetState`,
/// which wins over a delivered non-terminal transition (→ on-fire/damaged).
fn simulate_node_state(node: &SwitchNodeDef, delivered: &std::collections::HashSet<u32>) -> usize {
    let setstate = crate::hash::pandemic_hash_m2("setstate");
    let setstateonmsg = crate::hash::pandemic_hash_m2("setstateonmsg");
    let find = |t: u32| node.states.iter().position(|s| s.name_hash == t);
    let mut cur = default_state_index(node);
    for _ in 0..node.states.len() * 2 + 4 {
        let Some(state) = node.states.get(cur) else {
            break;
        };
        let (mut terminal, mut passthrough, mut minor) = (None, None, None);
        for (cmd, args) in commands(&state.enter) {
            if cmd == setstate {
                if let Some(&t) = args.first() {
                    passthrough = find(t);
                }
            } else if cmd == setstateonmsg {
                if let (Some(&t), Some(&msg)) = (args.first(), args.get(1)) {
                    if delivered.contains(&msg) {
                        if is_terminal(t) {
                            terminal = find(t);
                        } else {
                            minor = find(t);
                        }
                    }
                }
            }
        }
        let next = terminal.or(passthrough).or(minor);
        match next {
            Some(n) if n != cur => cur = n,
            _ => break,
        }
    }
    cur
}

/// The distinct damage-event message hashes, classified by what they do **from full health**: a
/// message is `terminal` (→ wreck/destroyed) if any node's PRISTINE state routes it to a terminal
/// state; `minor` (→ increasingly-damaged / fire) otherwise. Classifying from the pristine states
/// (not every state) is what keeps the death messages out of the minor band — the same message hash
/// can lead somewhere else once already damaged.
pub fn damage_messages(sm: &StateMachine) -> (Vec<u32>, Vec<u32>) {
    let setstateonmsg = crate::hash::pandemic_hash_m2("setstateonmsg");
    let mut terminal: Vec<u32> = Vec::new();
    let mut minor: Vec<u32> = Vec::new();
    for node in &sm.nodes {
        let Some(pristine) = node.states.get(default_state_index(node)) else {
            continue;
        };
        for (cmd, args) in commands(&pristine.enter) {
            if cmd != setstateonmsg {
                continue;
            }
            if let (Some(&t), Some(&msg)) = (args.first(), args.get(1)) {
                if is_terminal(t) {
                    if !terminal.contains(&msg) {
                        terminal.push(msg);
                    }
                } else if !minor.contains(&msg) {
                    minor.push(msg);
                }
            }
        }
    }
    // Death dominates: a message that ever means "wreck" is never treated as minor.
    minor.retain(|m| !terminal.contains(m));
    (minor, terminal)
}

/// Per-switch-node chosen state for a given HEALTH fraction (1.0 = full, 0.0 = destroyed). Delivers
/// the machine's damage messages by band — none at full health, the `minor` set once damaged, and the
/// `terminal` set at zero — then runs [`simulate_node_state`]. Feed the result to
/// [`machine_node_enable`] to get the render node-enable table.
///
/// The band thresholds are our approximation of the (live-only) HP math; the STATES reached are the
/// engine's own. `damaged_below` = health fraction under which minor damage shows.
pub fn node_states_for_health(sm: &StateMachine, health: f32, damaged_below: f32) -> Vec<usize> {
    let (minor, terminal) = damage_messages(sm);
    let mut delivered = std::collections::HashSet::new();
    if health <= 0.0 {
        delivered.extend(minor.iter().copied());
        delivered.extend(terminal.iter().copied());
    } else if health < damaged_below {
        delivered.extend(minor.iter().copied());
    }
    node_states_for_delivered(sm, &delivered)
}

/// Per-switch-node chosen state for an explicit set of **already-delivered** damage messages.
///
/// This is the stateful entry point [`node_states_for_health`] is built on, and it is the one a
/// runtime should use. The difference matters: `node_states_for_health` derives the delivered set
/// from the *current* health fraction alone, so restoring health walks the machine **backwards** —
/// a shed door would reattach. Retail delivers messages once and the machine only ever moves
/// forward, so a caller that keeps a monotonically-growing `delivered` set gets the faithful
/// behaviour. See `mercs2_destruction`, which owns that set per entity.
pub fn node_states_for_delivered(
    sm: &StateMachine,
    delivered: &std::collections::HashSet<u32>,
) -> Vec<usize> {
    sm.nodes
        .iter()
        .map(|n| simulate_node_state(n, delivered))
        .collect()
}

/// The `(command_hash, args)` pairs of a state's **enter** script, in order.
///
/// Exposed so a runtime can react to the side-effecting commands the engine's enter-scripts carry —
/// `CreateObject` (debris) and `StartEmitter` (fire) — which `machine_node_enable` deliberately
/// ignores because they are not `SHOW`/`HIDE`.
pub fn enter_commands(state: &StateDef) -> Vec<(u32, Vec<u32>)> {
    commands(&state.enter)
}

/// GROUND-TRUTH per-**HIER-node** enable flags from the engine state machine — the runtime's
/// `OBJ+0x2a0` node-enable table, which SEGM records index by their signed `node` field (clause 3 of
/// the draw gate; see `mercs2_engine::render_state`).
///
/// No classification heuristics. The seeding is DATA: every `SWIT` participant subtree starts
/// **hidden**, everything else starts visible; then each switch node's CHOSEN state executes its
/// enter-script `SHOW`/`Hide` commands over the HIER, flipping whole subtrees. So a pristine body
/// (not under a switch slot) is enabled by default and break pieces stay disabled until a state shows
/// them. `chosen[i]` = state index for `sm.nodes[i]` (see [`default_state_index`]).
pub fn machine_node_enable(sm: &StateMachine, hier: &[HierNode], chosen: &[usize]) -> Vec<bool> {
    machine_node_enable_seeded(sm, hier, chosen, NodeSeed::default(), NodeScope::default())
}

/// How the node-enable table (`OBJ+0x2a0`) is INITIALISED before any state's enter script runs.
///
/// **This is not settled from the exe.** The constructor's `memset` sits behind a register alias in
/// the decomp (see `docs/modernization/model_render_gate_spec.md` §6), so the seed is chosen here and
/// validated against real models. Do not describe either variant as "ground truth".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NodeSeed {
    /// Every node starts ENABLED; only an explicit `Hide` in an entered state's script turns one off.
    AllEnabled,
    /// Every `SWIT` participant subtree starts HIDDEN; a `SHOW` must turn it back on.
    ///
    /// The DEFAULT, on evidence rather than proof: `oc_veh_helicopter_md500`'s default states contain
    /// no `Hide` command at all, so under [`NodeSeed::AllEnabled`] its wreck renders on top of the
    /// intact body. Only this seeding suppresses it. `ch_veh_tank_ztz98` is indifferent (its default
    /// state Hides its break pieces explicitly).
    #[default]
    SwitchSlotsHidden,
}

/// Whether a `SHOW`/`Hide` command applies to the named HIER node alone, or to its whole subtree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NodeScope {
    /// The command marks only the node it names.
    NodeOnly,
    /// The command marks the node and every descendant.
    ///
    /// THE DEFAULT, and it must be. A switch slot names a *grouping* node; the geometry hangs on its
    /// CHILDREN. `ch_veh_tank_ztz98`'s turret slot is node 4 (`0x54C595F0`, no geometry of its own) and
    /// the turret/barrel meshes live on child nodes 5 and 11. Marking node-only leaves those children
    /// untouched, so the turret renders identically at 100% and 0% health while the hull (whose mesh
    /// IS on the named node) switches — the exact symptom that exposed this.
    #[default]
    Subtree,
}

/// [`machine_node_enable`] with an explicit seed — the knob for comparing the variants against real
/// models rather than asserting one.
pub fn machine_node_enable_seeded(
    sm: &StateMachine,
    hier: &[HierNode],
    chosen: &[usize],
    seed: NodeSeed,
    scope: NodeScope,
) -> Vec<bool> {
    machine_node_hidden(sm, hier, chosen, seed, scope)
        .into_iter()
        .map(|h| !h)
        .collect()
}

/// Per-mesh-group visibility: [`machine_node_enable`] resolved through `INDX` (mesh group → HIER
/// node). Kept for callers that reason in draw groups; the engine itself gates per SEGM record.
pub fn machine_group_visibility(
    sm: &StateMachine,
    hier: &[HierNode],
    indx: &[usize],
    chosen: &[usize],
) -> Vec<bool> {
    let hidden = machine_node_hidden(sm, hier, chosen, NodeSeed::default(), NodeScope::default());
    indx.iter()
        .map(|&n| hidden.get(n).map(|h| !h).unwrap_or(true))
        .collect()
}

fn machine_node_hidden(
    sm: &StateMachine,
    hier: &[HierNode],
    chosen: &[usize],
    seed: NodeSeed,
    scope: NodeScope,
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
    fn mark(children: &[Vec<usize>], hidden: &mut [bool], root: usize, v: bool, scope: NodeScope) {
        if scope == NodeScope::NodeOnly {
            if root < hidden.len() {
                hidden[root] = v;
            }
            return;
        }
        let mut stack = vec![root];
        while let Some(x) = stack.pop() {
            if x < hidden.len() {
                hidden[x] = v;
                stack.extend_from_slice(&children[x]);
            }
        }
    }
    let mut hidden = vec![false; hier.len()];
    if seed == NodeSeed::SwitchSlotsHidden {
        for &slot in &sm.switch_slots {
            if let Some(&i) = hash_to_idx.get(&slot) {
                mark(&children, &mut hidden, i, true, scope);
            }
        }
    }
    for (ni, node) in sm.nodes.iter().enumerate() {
        let si = chosen
            .get(ni)
            .copied()
            .unwrap_or(0)
            .min(node.states.len().saturating_sub(1));
        let Some(st) = node.states.get(si) else {
            continue;
        };
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
                                mark(&children, &mut hidden, idx, cmd == hide, scope);
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
    hidden
}

/// Parse the first `SWIT` chunk as a flat u32 node-hash list.
pub fn parse_swit(buf: &[u8]) -> Vec<u32> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"SWIT") else {
        return Vec::new();
    };
    (0..size / 4).map(|i| u32_le(buf, off + i * 4)).collect()
}

/// Parse the first `INDX` chunk as a u16 array: **drawing-group order → SEG_ID** (an index into the
/// `SEGM` record array), NOT a HIER node index.
///
/// The old "→ HIER node" reading was wrong and is what scattered vehicle parts across the model.
/// `SEGM[INDX[group]]` gives the group's real `{node, seg_id, lod_mask}`; the node is the attachment
/// bone (a tank barrel's mount at turret height), the mask its LOD tier. A model carries far more
/// SEGM records than groups (tank: 130 vs 12), so indexing SEGM by the group/sub-object ordinal reads
/// unrelated records. Validated on `ch_veh_tank_ztz98` against the HIER node whose own bbox matches
/// each mesh (`mercs2_probe --bin segfix_probe`).
pub fn parse_indx(buf: &[u8]) -> Vec<usize> {
    let chunks = leaf_chunks(buf);
    let Some((off, size)) = find_chunk(&chunks, b"INDX") else {
        return Vec::new();
    };
    (0..size / 2)
        .map(|i| u16_le(buf, off + i * 2) as usize)
        .collect()
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
            let node = collision
                .get(hi)
                .copied()
                .unwrap_or(0)
                .min(hier.len().saturating_sub(1));
            let m = world.get(node).copied().unwrap_or_else(|| {
                let mut id = [0.0; 16];
                id[0] = 1.0;
                id[5] = 1.0;
                id[10] = 1.0;
                id[15] = 1.0;
                id
            });
            out.push(GroundedHull {
                node,
                vertices: hull
                    .vertices
                    .iter()
                    .map(|v| transform_point(*v, &m))
                    .collect(),
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
    let swit_idx: std::collections::HashSet<usize> = swit
        .iter()
        .filter_map(|w| hash_to_idx.get(w).copied())
        .collect();

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
        let break_root = roots.iter().copied().max_by_key(|&r| {
            subtree(r)
                .into_iter()
                .filter(|x| *x != r && swit_idx.contains(x))
                .count()
        });
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
    let break_nodes = state
        .iter()
        .filter(|(s, _)| *s == DestructionState::BreakPiece)
        .count();
    if hull_count > 0 && break_nodes == 0 {
        warnings.push(format!(
            "{hull_count} PHY2 hulls but no break_piece nodes classified"
        ));
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
            assert_eq!(
                st(i),
                DestructionState::Intact,
                "node{i} (node2 subtree) is intact"
            );
        }
        assert_eq!(st(3), DestructionState::BreakPiece);
        for i in [4, 5, 6, 7, 8] {
            assert_eq!(
                st(i),
                DestructionState::BreakPiece,
                "node{i} (node3 subtree) is a break piece"
            );
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
        assert_eq!(
            g.iter().map(|h| h.node).collect::<Vec<_>>(),
            vec![8, 7, 6, 5, 4, 2]
        );
        // every grounded vertex sits within the crate render AABB (±small margin)
        for h in &g {
            for v in &h.vertices {
                assert!(
                    v[0] >= -0.98 && v[0] <= 0.98,
                    "x out (node {}): {v:?}",
                    h.node
                );
                assert!(
                    v[1] >= -0.12 && v[1] <= 1.2,
                    "y out (node {}): {v:?}",
                    h.node
                );
                assert!(
                    v[2] >= -0.62 && v[2] <= 0.62,
                    "z out (node {}): {v:?}",
                    h.node
                );
            }
        }
    }
}
