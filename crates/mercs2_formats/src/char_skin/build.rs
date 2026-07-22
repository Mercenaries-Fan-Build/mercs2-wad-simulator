//! Skin + re-pose builder: source rig → Mercs2 SKIN group. Faithful port of
//! `mercs2-mesher/src/build.js` (byte-exact to the Python that produced two in-game
//! characters). Produces:
//!   * `skin_bytes`  8 B/vertex : BLENDINDICES u8×4 (PALETTE-RELATIVE) + BLENDWEIGHT u8×4
//!   * `pos`        f32×3/vertex: re-posed POSITION
//!   * `ranges`                 : the palette range table `{u16 base, u16 count}` — written
//!                                into the SKIN group's `INFO(56)` leaf; the exact inverse
//!                                of the palette expand in [`crate::model_cubeize`].

use super::automap::{automap, Origin, Rig};
use super::mat::*;
use std::collections::HashMap;

/// Largest palette a single draw group may carry.
///
/// Was 46, described as "largest palette the shipped game uses". Measured otherwise:
/// `mercs2_probe --bin skin_census --group 3` reports **48** distinct bones in shipped
/// pmc_hum_mattias group 3 (chris 45). So 46 was below retail and fired the finger-collapse
/// unnecessarily -- on 50 Cent it destroyed 30 mapped bones (both finger ranges), taking the
/// palette from 58 mapped down to 28. Set to the measured retail maximum, not to an invented
/// number and not to the structural ceiling (BLENDINDICES is u8, so 255 slots would fit, but
/// nothing shipped comes close and an unproven jump is not worth the risk).
///
/// Raising 46 -> 64 lifted multi-influence 14.6% -> 19.4%, which is real but small: the cap was
/// never the main cause of coarse skinning. That is source detail the target rig cannot represent
/// (muscle/twist/face helper joints with no counterpart), and no palette size fixes it.
pub const PALETTE_CAP: usize = 48;

/// Run-length encode a SORTED, deduplicated bone-index list into at most [`MAX_RANGES`] runs,
/// returning `(ranges, bone -> palette slot, slot count)`.
///
/// A palette is per DRAW GROUP, not per model: the shipped format stores each group's runs in
/// its own `INFO(56)` leaf. This is shared by the whole-model palette and by the per-group
/// palettes the multi-group injector writes, so both emit byte-identical tables — a dense model
/// split across three groups gets three small palettes rather than one that overflows the cap.
pub fn build_palette_ranges(used: &[u32]) -> (Vec<(u32, u32)>, HashMap<u32, u8>, usize) {
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    if !used.is_empty() {
        let mut run = (used[0], 1u32);
        for &h in &used[1..] {
            if h == run.0 + run.1 {
                run.1 += 1;
            } else {
                ranges.push(run);
                run = (h, 1);
            }
        }
        ranges.push(run);
    }
    while ranges.len() > MAX_RANGES {
        // merge the closest neighbours; ties resolve to the LOWEST index (strict `<`).
        let mut best = 0usize;
        let mut best_gap = i64::MAX;
        for i in 0..ranges.len() - 1 {
            let gap = ranges[i + 1].0 as i64 - (ranges[i].0 as i64 + ranges[i].1 as i64);
            if gap < best_gap {
                best_gap = gap;
                best = i;
            }
        }
        let merged = (
            ranges[best].0,
            ranges[best + 1].0 + ranges[best + 1].1 - ranges[best].0,
        );
        ranges.splice(best..best + 2, [merged]);
    }
    let mut slot_of: HashMap<u32, u8> = HashMap::new();
    let mut s = 0u32;
    for &(base, count) in &ranges {
        for h in base..base + count {
            slot_of.insert(h, s as u8);
            s += 1;
        }
    }
    (ranges, slot_of, s as usize)
}
pub const MAX_RANGES: usize = 8; // range_count field in the group's INFO leaf

/// Which model→container transform was used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fitted exactly from a `dump_group_verts` container dump (the in-game-proven path).
    Exact,
    /// Estimated from bone correspondences (no dump) — good for preview, not yet proven.
    Estimated,
}

/// Target (Mercs2) skeleton as plain data. Derive from [`crate::skeleton::Skeleton`] via
/// [`TargetSkeleton::from_skeleton`] — bones are in HIER order so `i` is the global index.
#[derive(Clone)]
pub struct TargetBone {
    pub i: u32,
    pub pos: [f64; 3],
    pub parent: i32,
    pub name: String,
    /// HIER name-hash (`pandemic_hash_m2` of the bone name) — used to resolve a canonical NPC-84
    /// automap index onto THIS skeleton by identity, so a HERO donor (reordered HIER) maps right.
    pub name_hash: u32,
}

#[derive(Clone)]
pub struct TargetSkeleton {
    pub bones: Vec<TargetBone>,
    pub height: f64,
}

impl TargetSkeleton {
    pub fn tgt(&self, h: u32) -> Option<[f64; 3]> {
        self.bones.iter().find(|b| b.i == h).map(|b| b.pos)
    }
    pub fn parent_of(&self, h: u32) -> Option<i32> {
        self.bones.iter().find(|b| b.i == h).map(|b| b.parent)
    }
    /// Resolve a CANONICAL NPC-84 automap index onto THIS skeleton's own HIER index, by matching
    /// the canonical bone name-hash. Returns `None` if this donor lacks that bone. For an NPC-84
    /// donor this is the identity; for a HERO donor it re-seats indices onto the reordered HIER.
    pub fn index_by_canonical(&self, npc_hier: u32) -> Option<u32> {
        let hash = super::npc84_name_hash(npc_hier)?;
        self.bones.iter().find(|b| b.name_hash == hash).map(|b| b.i)
    }
}

/// Source mesh + rig, as plain data (a glTF adapter fills this — keeps `char_skin`
/// glTF-free). All matrices are ROW-MAJOR f64.
pub struct BuildInput<'a> {
    pub rig: Rig<'a>,
    /// POSITION per vertex, raw model space.
    pub positions: &'a [[f64; 3]],
    /// JOINTS_0 per vertex (joint indices).
    pub vjoints: &'a [[u16; 4]],
    /// WEIGHTS_0 per vertex.
    pub vweights: &'a [[f64; 4]],
    /// Triangle indices (for stats/validation); may be empty.
    pub indices: &'a [u32],
    /// node index → world matrix (row-major), for scene-space bind positions.
    pub node_world: &'a [[f64; 16]],
    /// node index → child node indices (for direction alignment).
    pub node_children: &'a [Vec<usize>],
    /// joint index → inverse-bind matrix (row-major), None when absent.
    pub ibm: &'a [Option<[f64; 16]>],
    pub skeleton: &'a TargetSkeleton,
    /// Container vertices from `dump_group_verts` (EXACT transform) — None = ESTIMATED.
    pub container_verts: Option<&'a [[f64; 3]]>,
    /// Manual retarget overrides: source joint → Some(hier) or None (drop).
    pub overrides: HashMap<usize, Option<u32>>,
    pub shared_bind_anchor: bool,
}

/// Owned holder for a parsed source rig + mesh — the glTF-free interchange between a glTF
/// adapter (the CLI's serde_json reader, or the workshop's `gltf`-crate reader) and
/// [`build_character`]. All matrices ROW-MAJOR f64. Build a [`BuildInput`] from it with
/// [`CharGlbData::build_input`].
#[derive(Clone, Default)]
pub struct CharGlbData {
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub tris: Vec<[u32; 3]>,
    pub indices: Vec<u32>,
    pub vjoints: Vec<[u16; 4]>,
    pub vweights: Vec<[f64; 4]>,
    pub joint_nodes: Vec<usize>,
    pub node_parent: Vec<i32>,
    pub node_name: Vec<String>,
    pub node_children: Vec<Vec<usize>>,
    pub node_world: Vec<[f64; 16]>,
    pub ibm: Vec<Option<[f64; 16]>>,
}

impl CharGlbData {
    /// Borrow this data as a [`BuildInput`] for [`build_character`].
    pub fn build_input<'a>(
        &'a self,
        skeleton: &'a TargetSkeleton,
        container_verts: Option<&'a [[f64; 3]]>,
        overrides: HashMap<usize, Option<u32>>,
        shared_bind_anchor: bool,
    ) -> BuildInput<'a> {
        BuildInput {
            rig: Rig {
                joint_nodes: &self.joint_nodes,
                node_parent: &self.node_parent,
                node_name: &self.node_name,
            },
            positions: &self.positions,
            vjoints: &self.vjoints,
            vweights: &self.vweights,
            indices: &self.indices,
            node_world: &self.node_world,
            node_children: &self.node_children,
            ibm: &self.ibm,
            skeleton,
            container_verts,
            overrides,
            shared_bind_anchor,
        }
    }
}

/// Everything needed to author the SKIN group + validate it.
pub struct CharSkin {
    /// nv×8: BLENDINDICES (palette-relative) + BLENDWEIGHT.
    pub skin_bytes: Vec<u8>,
    /// nv re-posed positions (container space), f32 as stored.
    pub pos: Vec<[f32; 3]>,
    /// Palette range table `{base, count}` for the `INFO(56)` leaf.
    pub ranges: Vec<(u16, u16)>,
    pub palette_slots: usize,
    pub mode: Mode,
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
    pub stats: Stats,
    // ---- internals kept for validation + the round-trip test ----
    /// final joint → HIER (post finger-collapse).
    pub full: HashMap<usize, u32>,
    /// HIER → palette slot.
    pub slot_of: HashMap<u32, u8>,
    pub origin: HashMap<usize, Origin>,
    /// container-space source positions per re-posed vertex (pre-transform anchor).
    pub cp: Vec<[f64; 3]>,
    /// re-posed positions in f64 (for validation precision).
    pub posed: Vec<[f64; 3]>,
    /// source joint → its bind position in container space.
    pub srcp: HashMap<usize, [f64; 3]>,
    pub names: Vec<String>,
    /// copy of the target skeleton bones (for validation lookups).
    pub skeleton_bones: Vec<TargetBone>,
    /// per-target-bone re-pose transform + the size of the correspondence set it was fitted
    /// from (diagnostics).
    pub bone_sims: HashMap<u32, (Sim, usize)>,
}

#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub verts: usize,
    pub tris: usize,
    pub palette_slots: usize,
    pub range_count: usize,
    pub collapsed_fingers: bool,
    pub height: f64,
    pub multi_influence: usize,
    pub source_multi_influence: usize,
    pub influence_retained: f64,
    pub mean_displacement: f64,
    pub align_mean_deg: f64,
    pub align_max_deg: f64,
    /// Number of target bones that got a fitted re-pose transform.
    pub rotated_bones: usize,
    /// Bones whose correspondence cloud was rank-deficient, so the transform fell back to the
    /// parent's rotation plus a shortest-arc correction.
    pub rejected_alignments: usize,
    pub fit_residual: f64,
}

/// Fingers collapse into their parent hand when the palette would overflow.
fn finger_to_hand(h: u32) -> Option<u32> {
    if (48..63).contains(&h) {
        Some(46)
    } else if (69..84).contains(&h) {
        Some(67)
    } else {
        None
    }
}

/// Build the SKIN group data for a source mesh. Faithful port of `buildCharacter`.
pub fn build_character(inp: &BuildInput) -> Result<CharSkin, String> {
    let nv = inp.positions.len();
    let mut warn: Vec<String> = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    // ---- retarget ----
    let am = automap(&inp.rig);
    let names = am.names.clone();
    let sk = inp.skeleton;

    // used-bone collector (optionally finger-collapsed) — drives the palette overflow decision below
    // and the range-table build later.
    let collect = |full: &HashMap<usize, u32>, collapse: bool| -> Vec<u32> {
        let mut set: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
        for vi in 0..nv {
            for k in 0..4 {
                if inp.vweights[vi][k] > 0.0 {
                    if let Some(&h) = full.get(&(inp.vjoints[vi][k] as usize)) {
                        let h = if collapse { finger_to_hand(h).unwrap_or(h) } else { h };
                        set.insert(h);
                    }
                }
            }
        }
        set.into_iter().collect()
    };

    // `automap` yields CANONICAL NPC-84 indices. A HERO donor (mattias_v2) reorders/extends the HIER,
    // so: (1) build the NPC-84 map, (2) finger-collapse in NPC-84 space (48-62→46, 69-83→67 when the
    // palette would overflow), (3) RESOLVE each canonical index onto THIS donor's own HIER by
    // name-hash, (4) apply donor-space overrides. This keeps char_skin target-agnostic — the palette,
    // BLENDINDICES and re-pose all end up in the donor's actual HIER.
    let mut full_npc: HashMap<usize, u32> = HashMap::new();
    let mut origin: HashMap<usize, Origin> = HashMap::new();
    for (&j, &h) in &am.inherited {
        full_npc.insert(j, h);
        origin.insert(j, Origin::Inherited);
    }
    for (&j, &h) in &am.mapped {
        full_npc.insert(j, h);
        origin.insert(j, Origin::Auto);
    }
    let mut collapsed_fingers = false;
    {
        let used = collect(&full_npc, false);
        if used.len() > PALETTE_CAP && collect(&full_npc, true).len() < used.len() {
            for h in full_npc.values_mut() {
                if let Some(m) = finger_to_hand(*h) {
                    *h = m;
                }
            }
            collapsed_fingers = true;
        }
    }
    let mut full: HashMap<usize, u32> = HashMap::new();
    for (&j, &npc) in &full_npc {
        match sk.index_by_canonical(npc) {
            Some(ti) => {
                full.insert(j, ti);
            }
            None => {
                origin.remove(&j); // this donor lacks the canonical bone → drop
            }
        }
    }
    for (&j, &v) in &inp.overrides {
        match v {
            Some(ti) => {
                full.insert(j, ti);
                origin.insert(j, Origin::Manual);
            }
            None => {
                full.remove(&j);
                origin.insert(j, Origin::Dropped);
            }
        }
    }

    // ---- bind positions in RAW MODEL space (from non-identity IBMs) ----
    let mut ibm_raw: HashMap<usize, [f64; 3]> = HashMap::new();
    for (j, ibm) in inp.ibm.iter().enumerate() {
        if let Some(m) = ibm {
            if allclose(m, &IDENT4, 1e-6) {
                continue;
            }
            if let Some(inv) = inv4(m) {
                ibm_raw.insert(j, origin_of(&inv));
            }
        }
    }

    // node world origin per joint (scene space)
    let node_pos = |j: usize| -> Option<[f64; 3]> {
        let node = inp.rig.joint_nodes[j];
        inp.node_world.get(node).map(origin_of)
    };

    // ---- 1. model -> container transform ----
    let t: Fit;
    let mode;
    let fit_resid;
    if let Some(cv) = inp.container_verts {
        if cv.len() != nv {
            return Err(format!(
                "vertex count mismatch: glb mesh has {nv}, container group has {}",
                cv.len()
            ));
        }
        let a: Vec<Vec<f64>> = inp.positions.iter().map(|p| vec![p[0], p[1], p[2], 1.0]).collect();
        let b: Vec<Vec<f64>> = cv.iter().map(|p| vec![p[0], p[1], p[2]]).collect();
        let f = lstsq(&a, &b)?;
        t = fit_from_lstsq(&f.x);
        fit_resid = f.resid_mean;
        mode = Mode::Exact;
        if f.resid_mean > 0.01 {
            warn.push(format!(
                "model->container fit residual is {:.4} -- expected ~0. The container was \
                 probably built from a DIFFERENT model than this .glb.",
                f.resid_mean
            ));
        }
    } else {
        // The estimated similarity fit is a least-squares over bone correspondences — so a rig with
        // dozens of FACE + FINGER bones (50 Cent: ~60 of 114) all clustered in the head/hands would
        // DOMINATE it and skew the whole-body scale (mesh came out ~12% short). Fit only from the
        // well-spread BODY/LIMB bones (canonical NPC-84: hips/legs/spine/neck/head 0..21 + upper-arm/
        // forearm/hand 42..46 & 63..67), excluding the face (22..41) and fingers (48..62, 69..83).
        let is_fit_bone = |npc: u32| npc <= 21 || (42..=46).contains(&npc) || (63..=67).contains(&npc);
        let mut src = Vec::new();
        let mut dst = Vec::new();
        // deterministic order (sorted joints) so the estimate is reproducible
        let mut keys: Vec<usize> = full.keys().copied().collect();
        keys.sort_unstable();
        for spread_only in [true, false] {
            // pass 1: spread body/limb bones, ONE correspondence per target (so 15 collapsed fingers
            // sharing a hand, or several source spines sharing a rung, count once). pass 2 (fallback):
            // all mapped joints, if a sparse rig left too few.
            let mut used_tgt = std::collections::HashSet::new();
            for &j in &keys {
                let h = full[&j];
                if spread_only
                    && (!full_npc.get(&j).is_some_and(|&npc| is_fit_bone(npc)) || !used_tgt.insert(h))
                {
                    continue;
                }
                if let (Some(p), Some(d)) = (ibm_raw.get(&j).copied(), sk.tgt(h)) {
                    src.push(p);
                    dst.push(d);
                }
            }
            if src.len() >= 8 {
                break;
            }
            src.clear();
            dst.clear();
        }
        if src.len() < 8 {
            return Err(format!(
                "cannot estimate the model transform: only {} mapped joints have a usable \
                 inverse-bind matrix (need 8). Supply a container vertex dump.",
                src.len()
            ));
        }
        let sim = fit_similarity(&src, &dst)?;
        t = sim.t;
        fit_resid = sim.resid_mean;
        mode = Mode::Estimated;
        notes.push(
            "No container dump supplied: the transform was ESTIMATED from bone \
             correspondences. Export with a dump_group_verts TSV for the exact path."
                .into(),
        );
    }
    let to_container = |p: [f64; 3]| apply_fit(&t, p);
    let cp: Vec<[f64; 3]> = match inp.container_verts {
        Some(cv) => cv.to_vec(),
        None => inp.positions.iter().map(|&p| to_container(p)).collect(),
    };

    // ---- 2. scene -> container transform (for IBM-less joints) ----
    let mut pairs_s: Vec<Vec<f64>> = Vec::new();
    let mut pairs_c: Vec<Vec<f64>> = Vec::new();
    for j in 0..names.len() {
        let (Some(raw), Some(np)) = (ibm_raw.get(&j).copied(), node_pos(j)) else {
            continue;
        };
        pairs_s.push(vec![np[0], np[1], np[2], 1.0]);
        let c = to_container(raw);
        pairs_c.push(vec![c[0], c[1], c[2]]);
    }
    let s_fit: Option<Fit> = if pairs_s.len() >= 8 {
        lstsq(&pairs_s, &pairs_c).ok().map(|r| fit_from_lstsq(&r.x))
    } else {
        None
    };
    let bind_container = |j: usize| -> Option<[f64; 3]> {
        if let Some(raw) = ibm_raw.get(&j).copied() {
            return Some(to_container(raw));
        }
        if let (Some(sf), Some(np)) = (s_fit.as_ref(), node_pos(j)) {
            if !allclose(&np, &[0.0, 0.0, 0.0], 1e-6) {
                return Some(apply_fit(sf, np));
            }
        }
        None
    };

    // ---- 3. palette (donor HIER indices; fingers already collapsed in NPC-84 space) ----
    let used = collect(&full, false);
    let (ranges, slot_of, palette_slots) = build_palette_ranges(&used);
    if palette_slots > PALETTE_CAP {
        warn.push(format!(
            "palette is {palette_slots} slots, above the {PALETTE_CAP} the game ships. The \
             HIGHEST slots silently unbind: an extremity will be stranded in space."
        ));
    }
    let min_hier = used.first().copied().unwrap_or(0);

    // ---- 4. skin.bin ----
    let mut skin_bytes = vec![0u8; nv * 8];
    let mut multi_influence = 0usize;
    let mut source_multi = 0usize;
    for vi in 0..nv {
        let mut set = std::collections::HashSet::new();
        for k in 0..4 {
            if inp.vweights[vi][k] > 0.0 {
                set.insert(inp.vjoints[vi][k]);
            }
        }
        if set.len() > 1 {
            source_multi += 1;
        }
    }
    for vi in 0..nv {
        // insertion-ordered slot accumulation (mirrors JS Map iteration order)
        let mut pairs: Vec<(u8, f64)> = Vec::new();
        for k in 0..4 {
            let w = inp.vweights[vi][k];
            if w <= 0.0 {
                continue;
            }
            let Some(&h) = full.get(&(inp.vjoints[vi][k] as usize)) else {
                continue;
            };
            let Some(&sl) = slot_of.get(&h) else { continue };
            if let Some(e) = pairs.iter_mut().find(|(ps, _)| *ps == sl) {
                e.1 += w;
            } else {
                pairs.push((sl, w));
            }
        }
        if pairs.is_empty() {
            pairs.push((*slot_of.get(&min_hier).unwrap_or(&0), 1.0));
        }
        // stable sort by weight desc
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.truncate(4);
        if pairs.len() > 1 {
            multi_influence += 1;
        }
        let tot: f64 = pairs.iter().map(|p| p.1).sum();
        let scaled: Vec<(u8, f64)> = pairs.iter().map(|&(sl, w)| (sl, 255.0 * w / tot)).collect();
        let mut q: Vec<(u8, i64)> = scaled.iter().map(|&(sl, x)| (sl, x.floor() as i64)).collect();
        let rem = 255 - q.iter().map(|p| p.1).sum::<i64>();
        // indices sorted by fractional part desc (stable)
        let mut order: Vec<usize> = (0..scaled.len()).collect();
        order.sort_by(|&a, &b| {
            let fb = scaled[b].1 - scaled[b].1.floor();
            let fa = scaled[a].1 - scaled[a].1.floor();
            fb.partial_cmp(&fa).unwrap_or(std::cmp::Ordering::Equal)
        });
        for i in 0..rem as usize {
            let idx = order[i % q.len()];
            q[idx].1 += 1;
        }
        for i in 0..q.len() {
            skin_bytes[vi * 8 + i] = q[i].0;
            skin_bytes[vi * 8 + 4 + i] = q[i].1 as u8;
        }
        let sum: i64 = (0..4).map(|i| skin_bytes[vi * 8 + 4 + i] as i64).sum();
        if sum != 255 {
            return Err(format!("vertex {vi}: weights sum to {sum}, must be exactly 255"));
        }
    }

    // ---- 5. source bind positions in container space ----
    let mut srcp: HashMap<usize, [f64; 3]> = HashMap::new();
    let mut sorted_full: Vec<usize> = full.keys().copied().collect();
    sorted_full.sort_unstable();
    let jidx: HashMap<usize, usize> = inp
        .rig
        .joint_nodes
        .iter()
        .enumerate()
        .map(|(i, &n)| (n, i))
        .collect();
    for &j in &sorted_full {
        let mut p = bind_container(j);
        if p.is_none() {
            // inherit nearest ancestor with a position so the surface stays continuous
            let mut cur = inp.rig.joint_nodes[j] as i32;
            loop {
                let par = inp.rig.node_parent.get(cur as usize).copied().unwrap_or(-1);
                if par < 0 {
                    break;
                }
                cur = par;
                if let Some(&pj) = jidx.get(&(cur as usize)) {
                    if let Some(q) = bind_container(pj) {
                        p = Some(q);
                        break;
                    }
                }
            }
        }
        if let Some(p) = p {
            srcp.insert(j, p);
        }
    }

    // ---- optional: shared bind anchor per target bone ----
    if inp.shared_bind_anchor {
        let rank = |o: Option<&Origin>| match o {
            Some(Origin::Manual) => 0,
            Some(Origin::Auto) => 1,
            Some(Origin::Inherited) => 2,
            Some(Origin::Dropped) => 3,
            None => 2,
        };
        let mut anchor: HashMap<u32, (usize, i32)> = HashMap::new();
        let mut keys: Vec<usize> = srcp.keys().copied().collect();
        keys.sort_unstable();
        for j in keys {
            let h = full[&j];
            let rk = rank(origin.get(&j));
            match anchor.get(&h) {
                Some(&(_, cr)) if cr <= rk => {}
                _ => {
                    anchor.insert(h, (j, rk));
                }
            }
        }
        let srcp_keys: Vec<usize> = srcp.keys().copied().collect();
        for j in srcp_keys {
            if let Some(&(aj, _)) = anchor.get(&full[&j]) {
                if aj != j {
                    if let Some(&pos) = srcp.get(&aj) {
                        srcp.insert(j, pos);
                    }
                }
            }
        }
    }

    // ---- 6. one locally-fitted SIMILARITY per TARGET BONE ----
    //
    // The mesher displaced every vertex by `TGT[h] − SRCP[j]` (plus a rotation only where the
    // joint had exactly ONE mapped child). Two consequences, both visible on screen:
    //
    //  * **no scale** — a source thigh 0.35 m long whose target thigh is 0.48 m keeps its
    //    length while both endpoints are pinned to the target, so the knee is a 13 cm
    //    LBS-smeared taffy pull rather than a joint;
    //  * **teleport on a weak correspondence** — Mercs2's `Bone_Chest` sits at y 1.233 with
    //    nothing above it until `bone_neck` at 1.547, while a normal 4-rung source spine puts
    //    its top rung at ~1.52. Snapping that rung onto 1.233 drags the whole ribcage down
    //    28 cm while the neck/clavicles (well-matched) stay put — that IS the giraffe neck
    //    and the torn shoulders.
    //
    // Instead, fit ONE similarity per target bone by **moving least squares**: a weighted
    // Umeyama over the WHOLE bone-correspondence set, with a Gaussian falloff in source space
    // centred on the bone (Schaefer et al., *Image Deformation Using Moving Least Squares*).
    // That gives, for free, the three properties a 2-ring graph neighbourhood cannot:
    //
    //  * **smoothness** — neighbouring bones see nearly the same weighted set, so they move by
    //    nearly the same amount: no tears at the shoulder, no giraffe neck;
    //  * **outlier tolerance** — one anatomically wrong correspondence (the spine ladder puts a
    //    source chest rung on `Bone_Spine2`) is a single vote among ~10 nearby ones instead of a
    //    hard constraint, so a bad automap rung can no longer explode a segment;
    //  * **locality** — the far side of the body is exponentially suppressed, so the thigh still
    //    gets its own 1.45× length scale while the torso stays at ~1.0.
    //
    // Every source joint sharing a target bone shares its transform, so a rig that inherits 25
    // face joints onto `Bone_Head` no longer collapses them onto the head origin.
    const SCALE_CLAMP: (f64, f64) = (0.4, 2.5);
    /// Gaussian falloff width as a fraction of the target skeleton's height.
    const MLS_SIGMA_FRAC: f64 = 0.16;
    /// Extra weight on the bone's own correspondence, so it still tracks its target bone.
    const W_ANCHOR: f64 = 3.0;

    // joint-level adjacency (nearest joint ancestor / descendants), so non-joint nodes between
    // two joints do not break the walk.
    let joint_parent: Vec<Option<usize>> = (0..names.len())
        .map(|j| {
            let mut cur = inp.rig.joint_nodes[j] as i32;
            loop {
                let par = inp.rig.node_parent.get(cur as usize).copied().unwrap_or(-1);
                if par < 0 {
                    return None;
                }
                cur = par;
                if let Some(&pj) = jidx.get(&(cur as usize)) {
                    return Some(pj);
                }
            }
        })
        .collect();
    let mut joint_children: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    for (j, p) in joint_parent.iter().enumerate() {
        if let Some(p) = p {
            joint_children[*p].push(j);
        }
    }

    // primary source joint per target bone: Manual > Auto > Inherited, then lowest index.
    let rank_of = |o: Option<&Origin>| match o {
        Some(Origin::Manual) => 0u8,
        Some(Origin::Auto) => 1,
        Some(Origin::Inherited) => 2,
        _ => 3,
    };
    let mut primary: HashMap<u32, usize> = HashMap::new();
    for &j in &sorted_full {
        if !srcp.contains_key(&j) {
            continue;
        }
        let h = full[&j];
        let r = rank_of(origin.get(&j));
        match primary.get(&h) {
            Some(&cur) if rank_of(origin.get(&cur)) <= r => {}
            _ => {
                primary.insert(h, j);
            }
        }
    }
    // deterministic order: by source depth of the primary joint, then bone index.
    let depth_of = |mut j: usize| -> usize {
        let mut d = 0;
        while let Some(p) = joint_parent[j] {
            j = p;
            d += 1;
            if d > 512 {
                break;
            }
        }
        d
    };
    let mut bone_order: Vec<u32> = primary.keys().copied().collect();
    bone_order.sort_by_key(|&h| (depth_of(primary[&h]), h));

    let mut xform: HashMap<u32, Sim> = HashMap::new();
    let mut bone_sims: HashMap<u32, (Sim, usize)> = HashMap::new();
    let mut angles: Vec<f64> = Vec::new();
    let mut weak_bones = 0usize;
    // one correspondence per USED target bone — the MLS control-point set.
    let mut control: Vec<(u32, V3, V3, f64)> = bone_order
        .iter()
        .filter_map(|&h| Some((h, *srcp.get(&primary[&h])?, sk.tgt(h)?, 1.0)))
        .collect();

    // ---- 6a. donor-MESH landmark: the crown ----
    //
    // Every correspondence above is joint→joint, so the re-pose can only ever be as right as the
    // donor's joint placement. A donor whose head JOINT sits at the chin (Khronos RiggedFigure:
    // its head joint is 0.30 m below its own crown, where a Mercs2 head bone is 0.167 m below the
    // crown) therefore lands a correctly-retargeted skeleton under a head that overshoots the top
    // of the character by 13 cm — 2.00 m against the 1.82–1.85 m every shipped Mercs2 hero
    // measures (mattias_v2 1.847 / jen 1.850 / chris 1.820, crown−Bone_Head 0.181 / 0.165 /
    // 0.156). No joint-only method can see this: it needs the donor's MESH.
    //
    // So add ONE more control point taken from the geometry — the top of the head cloud — paired
    // with where a Mercs2 crown actually is (Bone_Head + CROWN_ABOVE_HEAD_BONE, keeping the
    // donor's own lateral offset so this constrains height only). It is an ordinary MLS control
    // point, so the head bone obeys it strongly, the neck partially, and the torso not at all —
    // the correction blends instead of stepping.
    const CROWN_ABOVE_HEAD_BONE: f64 = 0.167;
    /// Weight of the crown landmark relative to a joint control point. Twice `W_ANCHOR`: a mesh
    /// extent measured over the whole head cloud is a stronger statement about where the surface
    /// is than one joint position. Measured sweep (RiggedFigure height / 50 Cent height /
    /// 50 Cent edges >1.5x): 0 -> 2.003 / 1.820 / 1.37%; 1 -> 1.943 / 1.822 / 1.29%;
    /// 6 -> 1.910 / 1.826 / 1.12%; 30 -> 1.900 / 1.828 / 0.73%. Flat past 6, so the choice is
    /// not sensitive; every metric moves the right way.
    const W_CROWN: f64 = 6.0;
    let mut crown_note: Option<(f64, f64)> = None;
    if let (Some(head_h), Some(head_t)) = (sk.index_by_canonical(21), sk.index_by_canonical(21).and_then(|h| sk.tgt(h))) {
        if let Some(&hj) = primary.get(&head_h) {
            if let Some(&head_s) = srcp.get(&hj) {
                // topmost container-space vertex whose dominant influence is the head bone
                let mut crown: Option<[f64; 3]> = None;
                for vi in 0..nv {
                    let mut best = (-1.0f64, u32::MAX);
                    for k in 0..4 {
                        let w = inp.vweights[vi][k];
                        if w > best.0 {
                            if let Some(&h) = full.get(&(inp.vjoints[vi][k] as usize)) {
                                best = (w, h);
                            }
                        }
                    }
                    if best.1 == head_h && crown.map_or(true, |c| cp[vi][1] > c[1]) {
                        crown = Some(cp[vi]);
                    }
                }
                if let Some(c) = crown {
                    let have = c[1] - head_s[1];
                    if have > 1e-3 {
                        let dst = [
                            head_t[0] + (c[0] - head_s[0]),
                            head_t[1] + CROWN_ABOVE_HEAD_BONE,
                            head_t[2] + (c[2] - head_s[2]),
                        ];
                        crown_note = Some((have, CROWN_ABOVE_HEAD_BONE));
                        control.push((head_h, c, dst, W_CROWN));
                    }
                }
            }
        }
    }
    if let Some((have, want)) = crown_note {
        notes.push(format!(
            "donor crown sits {have:.3} m above its head joint; a Mercs2 head bone is {want:.3} m \
             below the crown, so a crown landmark was added to the re-pose (height control)"
        ));
    }
    let sigma = (sk.height.abs().max(0.1)) * MLS_SIGMA_FRAC;
    let two_sig2 = 2.0 * sigma * sigma;
    for &h in &bone_order {
        let p = primary[&h];
        let Some(&anchor_s) = srcp.get(&p) else { continue };
        let Some(anchor_d) = sk.tgt(h) else { continue };
        let mut pairs: Vec<(V3, V3, f64)> = Vec::with_capacity(control.len());
        for &(ch, cs_, cd, wextra) in &control {
            let d2 = {
                let v = sub(cs_, anchor_s);
                dot(v, v)
            };
            let mut w = (-d2 / two_sig2).exp();
            if ch == h {
                w *= W_ANCHOR;
            }
            w *= wextra;
            if w > 1e-6 {
                pairs.push((cs_, cd, w));
            }
        }
        // the bone whose transform anchors a degenerate fit: nearest mapped ancestor's bone.
        let up_bone = {
            let mut cur = joint_parent[p];
            let mut found = None;
            while let Some(j) = cur {
                if let Some(&hj) = full.get(&j) {
                    if hj != h {
                        found = Some(hj);
                        break;
                    }
                }
                cur = joint_parent[j];
            }
            found
        };
        let parent_sim = up_bone.and_then(|b| xform.get(&b).copied());
        let fitted = fit_similarity_weighted(&pairs);
        let sim = match fitted {
            // rank >= 2: the correspondence cloud spans a plane, so the rotation is fully
            // determined. Keep the MLS translation — deliberately NOT re-anchored on `TGT[h]`,
            // which is what averages a weak correspondence instead of obeying it.
            Some(f) if pairs.len() >= 3 && f.rank >= 2 && f.scale.is_finite() => {
                let s = f.scale.clamp(SCALE_CLAMP.0, SCALE_CLAMP.1);
                if (s - f.scale).abs() > 1e-9 {
                    let k = s / f.scale;
                    let sr: [f64; 9] = std::array::from_fn(|i| f.sr[i] * k);
                    // keep the anchor fixed under the clamped scale
                    Sim { sr, t: sub(f.apply(anchor_s), apply3(&sr, anchor_s)), scale: s, rank: f.rank }
                } else {
                    f
                }
            }
            // Colinear (or single) correspondences leave the twist about the chain axis free.
            // Take the parent bone's rotation and apply only the shortest-arc correction that
            // lines the chain up, then anchor the bone exactly on its target.
            other => {
                weak_bones += 1;
                let base = parent_sim.map(|s| {
                    let inv = 1.0 / s.scale.max(1e-12);
                    let r: [f64; 9] = std::array::from_fn(|i| s.sr[i] * inv);
                    (r, s.scale)
                });
                let (r_par, s_par) = base.unwrap_or((EYE3, 1.0));
                // dominant direction = the neighbour furthest from the anchor
                let far = pairs[1..]
                    .iter()
                    .max_by(|a, b| {
                        len(sub(a.0, anchor_s))
                            .partial_cmp(&len(sub(b.0, anchor_s)))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .copied();
                let (r, s) = match far {
                    Some((s_n, d_n, _)) if len(sub(s_n, anchor_s)) > 1e-6 && len(sub(d_n, anchor_d)) > 1e-6 => {
                        let u_s = norm(apply3(&r_par, sub(s_n, anchor_s)));
                        let u_d = norm(sub(d_n, anchor_d));
                        let corr = align_rot(u_s, u_d);
                        let ratio = len(sub(d_n, anchor_d)) / len(sub(s_n, anchor_s));
                        (mul3(&corr, &r_par), ratio.clamp(SCALE_CLAMP.0, SCALE_CLAMP.1))
                    }
                    _ => (r_par, s_par),
                };
                let sr: [f64; 9] = std::array::from_fn(|i| r[i] * s);
                Sim { sr, t: sub(anchor_d, apply3(&sr, anchor_s)), scale: s, rank: other.map_or(0, |f| f.rank) }
            }
        };
        let inv = 1.0 / sim.scale.max(1e-12);
        let rot_only: [f64; 9] = std::array::from_fn(|i| sim.sr[i] * inv);
        angles.push(rot_angle_deg(&rot_only));
        bone_sims.insert(h, (sim, pairs.len()));
        xform.insert(h, sim);
    }

    // ---- 7. pos.bin — LBS over the per-target-bone transforms ----
    let mut pos = vec![[0.0f32; 3]; nv];
    let mut posed = vec![[0.0f64; 3]; nv];
    let mut moved_sum = 0.0;
    for vi in 0..nv {
        let v = cp[vi];
        let mut acc = [0.0f64; 3];
        let mut tot = 0.0;
        for k in 0..4 {
            let w = inp.vweights[vi][k];
            if w <= 0.0 {
                continue;
            }
            let j = inp.vjoints[vi][k] as usize;
            let (Some(&h), Some(sim)) = (full.get(&j), full.get(&j).and_then(|h| xform.get(h)))
            else {
                continue;
            };
            let _ = h;
            let q = sim.apply(v);
            acc[0] += w * q[0];
            acc[1] += w * q[1];
            acc[2] += w * q[2];
            tot += w;
        }
        let p = if tot > 0.0 {
            [acc[0] / tot, acc[1] / tot, acc[2] / tot]
        } else {
            v
        };
        posed[vi] = p;
        moved_sum += len(sub(p, v));
        pos[vi] = [p[0] as f32, p[1] as f32, p[2] as f32];
    }
    let rejected = weak_bones;
    let ys: Vec<f64> = posed.iter().map(|p| p[1]).collect();
    let height = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
        - ys.iter().cloned().fold(f64::INFINITY, f64::min);

    let ranges_u16: Vec<(u16, u16)> = ranges.iter().map(|&(b, c)| (b as u16, c as u16)).collect();
    let stats = Stats {
        verts: nv,
        tris: inp.indices.len() / 3,
        palette_slots,
        range_count: ranges.len(),
        collapsed_fingers,
        height,
        multi_influence,
        source_multi_influence: source_multi,
        influence_retained: if source_multi > 0 {
            multi_influence as f64 / source_multi as f64
        } else {
            1.0
        },
        mean_displacement: moved_sum / nv as f64,
        align_mean_deg: if angles.is_empty() {
            0.0
        } else {
            angles.iter().sum::<f64>() / angles.len() as f64
        },
        align_max_deg: angles.iter().cloned().fold(0.0, f64::max),
        rotated_bones: angles.len(),
        rejected_alignments: rejected,
        fit_residual: fit_resid,
    };

    Ok(CharSkin {
        skin_bytes,
        pos,
        ranges: ranges_u16,
        palette_slots,
        mode,
        warnings: warn,
        notes,
        stats,
        full,
        slot_of,
        origin,
        cp,
        posed,
        srcp,
        names,
        skeleton_bones: sk.bones.clone(),
        bone_sims,
    })
}

/// Convert `lstsq`'s `m×k` solution (row-per-input-column) into the `Fit` shape
/// `apply_fit` expects (`T[row][out]`).
fn fit_from_lstsq(x: &[Vec<f64>]) -> Fit {
    [
        [x[0][0], x[0][1], x[0][2]],
        [x[1][0], x[1][1], x[1][2]],
        [x[2][0], x[2][1], x[2][2]],
        [x[3][0], x[3][1], x[3][2]],
    ]
}

struct Similarity {
    t: Fit,
    resid_mean: f64,
}

/// Best-fit SIMILARITY (uniform scale · rotation + translation) mapping src → dst, via the
/// **Umeyama** closed form (see [`fit_similarity_weighted`]).
///
/// This replaces the mesher's original `fitSimilarity`, which polar-decomposed the fitted
/// *general affine* and took the mean of its singular values as the scale. That is only the
/// least-squares optimum when the affine is already a similarity. Two humanoid rigs never
/// are: on `50cent → mattias_v2` the fitted affine has 3.0× anisotropy, and the old route
/// returned a rotation **22.9° off** the optimum with the scale **17.5% short** (mean
/// residual 0.185 m vs 0.094 m optimal). That tilt and shrink were the whole model's error
/// budget before any per-bone work started.
fn fit_similarity(src: &[[f64; 3]], dst: &[[f64; 3]]) -> Result<Similarity, String> {
    let pairs: Vec<([f64; 3], [f64; 3], f64)> =
        src.iter().zip(dst.iter()).map(|(&s, &d)| (s, d, 1.0)).collect();
    let sim = fit_similarity_weighted(&pairs).ok_or("fit_similarity: empty correspondence set")?;
    let sr = sim.sr;
    let t: Fit = [
        [sr[0], sr[3], sr[6]],
        [sr[1], sr[4], sr[7]],
        [sr[2], sr[5], sr[8]],
        [sim.t[0], sim.t[1], sim.t[2]],
    ];
    let mut sum = 0.0;
    for i in 0..src.len() {
        sum += len(sub(apply_fit(&t, src[i]), dst[i]));
    }
    Ok(Similarity {
        t,
        resid_mean: sum / src.len() as f64,
    })
}
