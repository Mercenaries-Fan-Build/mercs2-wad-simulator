//! `bone-census` — every HIER node (bone / hardpoint / destruction piece) in every model
//! container of the WAD stack, aggregated into one table.
//!
//! Bone NAMES are stripped from the shipped PC data: a HIER node carries only
//! `pandemic_hash_m2(name)` (see `mercs2_formats::skeleton`). So the census is over HASHES —
//! the name column is a best-effort resolution and each row records WHERE the name came from,
//! because a bare 32-bit hash match is not by itself evidence of a name.
//!
//! Sweep:
//!   * every primary `model` ASET (`wad::model_list`), grouped by block so each block is
//!     decompressed exactly once, then `orchestrator::parse_hier` / `parse_swit` / `classify`;
//!   * every animgroup block (`wad::animgroup_blocks`), whose clip bindings name the bones they
//!     drive by the same hash (`trnm`) — that marks a node as ANIMATED and catches skeleton bones
//!     that no mesh HIER contains (e.g. the root motion-extraction track).
//!
//! Emits a CSV row per distinct node hash and a summary to stdout.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mercs2_engine::wad;
use mercs2_formats::hash::pandemic_hash_m2;
use mercs2_formats::orchestrator;

/// Everything we learned about one node hash across the whole stack.
#[derive(Default)]
struct BoneRec {
    /// Models whose HIER contains this node.
    models: BTreeSet<u32>,
    /// Times it appears as a HIER ROOT (a root's hash == m2(model name)).
    as_root: usize,
    /// Times it is listed in a SWIT (destruction switch) group.
    in_swit: usize,
    /// Times it is a leaf (no children).
    as_leaf: usize,
    /// Distinct destruction states seen (`intact` / `break_piece` / `static` / ...).
    states: BTreeSet<String>,
    /// Animation clip tracks bound to this node (`trnm`), across all animgroups.
    anim_tracks: usize,
    /// Animgroup `hkaSkeleton` bone slots (rare in retail).
    anim_skel: usize,
    depth_min: usize,
    depth_max: usize,
    /// Largest bbox diagonal seen — a proxy for "is this a real geometry piece".
    max_bbox: f32,
    /// Non-zero local translation seen at least once (a posed/offset node).
    offset: bool,
}

impl BoneRec {
    fn note_hier(&mut self, model: u32, n: &orchestrator::HierNode, depth: usize) {
        if self.models.is_empty() {
            self.depth_min = depth;
            self.depth_max = depth;
        } else {
            self.depth_min = self.depth_min.min(depth);
            self.depth_max = self.depth_max.max(depth);
        }
        self.models.insert(model);
        if n.parent.is_none() {
            self.as_root += 1;
        }
        let d = [
            n.bbox_max[0] - n.bbox_min[0],
            n.bbox_max[1] - n.bbox_min[1],
            n.bbox_max[2] - n.bbox_min[2],
        ];
        let diag = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if diag.is_finite() && diag > self.max_bbox {
            self.max_bbox = diag;
        }
        let t = [n.local[12], n.local[13], n.local[14]];
        if t.iter().any(|v| v.is_finite() && v.abs() > 1e-4) {
            self.offset = true;
        }
    }
}

/// Names the census could find for a hash, with the provenance that produced them.
struct Resolved {
    names: Vec<String>,
    source: &'static str,
}

/// One HIER node, in the model it belongs to, with the structure the aggregate census throws away.
///
/// The per-hash census answers "what is this node"; it cannot answer "what is this node NEXT TO",
/// and that is where the remaining names live. Two facts recovered here are worth more than any
/// amount of extra brute force:
///
///   * `parent` — a node whose parent is `bone_rotor_main` is a blade or a swashplate, not a wheel.
///     The parent's name collapses the candidate vocabulary from the whole dialect to a handful.
///   * `world position` — the rig is built in the model's own space, so the node's own geometry
///     SPELLS its side: x<0 is left, x>0 is right, and two nodes at mirrored ±x with matching y/z
///     are an l/r pair BY CONSTRUCTION. That is a second witness taken from the model itself,
///     independent of the hash, which is exactly what a bare hash match lacks.
struct SkelRow {
    model: u32,
    idx: usize,
    hash: u32,
    parent: Option<u32>,
    depth: usize,
    world: [f32; 3],
    leaf: bool,
    swit: bool,
}

/// world = parent_world * local, walking down (parent index < own index is guaranteed by the exporter).
fn world_positions(hier: &[orchestrator::HierNode]) -> Vec<[f32; 3]> {
    let mut w: Vec<[f32; 16]> = Vec::with_capacity(hier.len());
    for n in hier {
        let m = match n.parent {
            Some(p) if p < w.len() => mat_mul(&w[p], &n.local),
            _ => n.local,
        };
        w.push(m);
    }
    w.iter().map(|m| [m[12], m[13], m[14]]).collect()
}

/// column-major 4x4 multiply (matching the engine's `local` layout).
fn mat_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            o[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    o
}

pub fn bone_census(
    wadpaths: &[String],
    csv_out: Option<String>,
    names_file: Option<String>,
    skeleton_csv: Option<String>,
) -> Result<(), String> {
    let mut bones: HashMap<u32, BoneRec> = HashMap::new();
    let mut skel: Vec<SkelRow> = Vec::new();
    // Every model we successfully parsed, and its root hash (for the root-name cross-check).
    let mut model_roots: HashMap<u32, u32> = HashMap::new();
    let mut n_models_seen = 0usize;
    let mut n_models_parsed = 0usize;
    let mut n_models_no_hier = 0usize;
    let mut n_animgroups = 0usize;
    let mut n_clips = 0usize;

    for wadpath in wadpaths {
        let mut w = wad::open(wadpath).map_err(|e| format!("open {wadpath}: {e}"))?;

        // ---- models, grouped by block so each block decompresses once ----
        let mut by_block: BTreeMap<u16, Vec<u32>> = BTreeMap::new();
        for (hash, block) in wad::model_list_all(&w) {
            by_block.entry(block).or_default().push(hash);
        }
        let n_blocks = by_block.len();
        eprintln!(
            "[{wadpath}] {} model assets in {n_blocks} blocks",
            by_block.values().map(|v| v.len()).sum::<usize>()
        );

        for (bi, (block, models)) in by_block.into_iter().enumerate() {
            if bi % 100 == 0 {
                eprintln!("  block {bi}/{n_blocks} ...");
            }
            let Ok(dec) = wad::decompress_block_index(&mut w, block) else { continue };
            for m in models {
                n_models_seen += 1;
                // Fast path: slice the model out of the already-decompressed block.
                let mut container = wad::model_span_in(&dec, m);
                let mut hier = container.as_deref().map(orchestrator::parse_hier).unwrap_or_default();
                // Robust fallback: full container extraction resolves models that model_span_in can't
                // slice from their primary block alone (multi-block models, e.g. al_veh_boat_destroyer).
                if hier.is_empty() {
                    if let Ok(c) = wad::extract_container(&mut w, m) {
                        hier = orchestrator::parse_hier(&c);
                        container = Some(c);
                    }
                }
                let Some(container) = container else { continue };
                if hier.is_empty() {
                    n_models_no_hier += 1;
                    continue;
                }
                n_models_parsed += 1;

                let swit: BTreeSet<u32> = orchestrator::parse_swit(&container).into_iter().collect();
                let dest = orchestrator::classify(&container);

                // depth + child census in one pass (parent[i] < i is guaranteed by the exporter)
                let mut depth = vec![0usize; hier.len()];
                let mut has_kids = vec![false; hier.len()];
                for n in &hier {
                    if let Some(p) = n.parent {
                        if p < hier.len() {
                            depth[n.index] = depth[p] + 1;
                            has_kids[p] = true;
                        }
                    }
                }
                let wpos = if skeleton_csv.is_some() { world_positions(&hier) } else { Vec::new() };
                for n in &hier {
                    if n.parent.is_none() {
                        model_roots.insert(m, n.hash);
                    }
                    if skeleton_csv.is_some() {
                        skel.push(SkelRow {
                            model: m,
                            idx: n.index,
                            hash: n.hash,
                            parent: n.parent.and_then(|p| hier.get(p)).map(|p| p.hash),
                            depth: depth[n.index],
                            world: wpos.get(n.index).copied().unwrap_or([0.0; 3]),
                            leaf: !has_kids[n.index],
                            swit: swit.contains(&n.hash),
                        });
                    }
                    let rec = bones.entry(n.hash).or_default();
                    rec.note_hier(m, n, depth[n.index]);
                    if swit.contains(&n.hash) {
                        rec.in_swit += 1;
                    }
                    if !has_kids[n.index] {
                        rec.as_leaf += 1;
                    }
                    if let Some(s) = dest.as_ref().and_then(|d| d.state_of_node(n.index)) {
                        rec.states.insert(s.as_str().to_string());
                    }
                }
            }
        }

        // ---- animgroups: which nodes are actually DRIVEN by animation ----
        for blk in wad::animgroup_blocks(&w) {
            let Ok(data) = wad::decompress_block_index(&mut w, blk) else { continue };
            let Ok(ag) = mercs2_formats::animgroup::parse_animgroup(&data) else { continue };
            n_animgroups += 1;
            // Retail animgroups usually ship no `hkaSkeleton` — parents/pose live in the mesh HIER.
            if let Some(sk) = &ag.skeleton {
                for &h in &sk.bone_name_hashes {
                    bones.entry(h).or_default().anim_skel += 1;
                }
            }
            for c in &ag.clips {
                n_clips += 1;
                for &h in &c.binding.track_to_bone_hash {
                    bones.entry(h).or_default().anim_tracks += 1;
                }
            }
        }
    }

    // ---------- name resolution ----------
    let all: BTreeSet<u32> = bones.keys().copied().collect();
    let mut resolved: HashMap<u32, Resolved> = HashMap::new();

    // (a) rainbow table — one pass, not one scan per hash.
    let rb = load_rainbow(&all);
    for (h, names) in rb {
        resolved.insert(h, Resolved { names, source: "rainbow" });
    }

    // (b) candidate grammar file (brute-forced strings). ONLY fills hashes the rainbow missed, and
    //     is tagged as such — these are unverified guesses, not evidence.
    if let Some(p) = &names_file {
        let text = std::fs::read_to_string(p).map_err(|e| format!("read {p}: {e}"))?;
        for line in text.lines() {
            let cand = line.trim();
            if cand.len() < 2 {
                continue;
            }
            let h = pandemic_hash_m2(cand);
            if !all.contains(&h) {
                continue;
            }
            match resolved.get_mut(&h) {
                Some(r) => {
                    if !r.names.iter().any(|n| n == cand) {
                        r.names.push(cand.to_string());
                    }
                }
                None => {
                    resolved.insert(
                        h,
                        Resolved { names: vec![cand.to_string()], source: "candidate" },
                    );
                }
            }
        }
    }

    // ---------- report ----------
    let named = bones.keys().filter(|h| resolved.contains_key(h)).count();
    let rainbow_named = bones
        .keys()
        .filter(|h| resolved.get(h).map(|r| r.source == "rainbow").unwrap_or(false))
        .count();
    let animated = bones.values().filter(|r| r.anim_tracks > 0).count();
    let mesh_only = bones.values().filter(|r| r.anim_tracks == 0 && !r.models.is_empty()).count();
    let anim_only = bones.values().filter(|r| r.models.is_empty()).count();

    println!("\n=== BONE CENSUS ===");
    println!("wads scanned         : {}", wadpaths.len());
    println!("model assets seen    : {n_models_seen}");
    println!("  with a HIER        : {n_models_parsed}");
    println!("  no HIER / no span  : {}", n_models_seen - n_models_parsed);
    println!("  (empty HIER)       : {n_models_no_hier}");
    println!("animgroups / clips   : {n_animgroups} / {n_clips}");
    println!("DISTINCT NODE HASHES : {}", bones.len());
    println!("  named (any source) : {named}  (rainbow-backed: {rainbow_named})");
    println!("  animation-driven   : {animated}");
    println!("  mesh-only (no anim): {mesh_only}");
    println!("  anim-only (no mesh): {anim_only}");

    if let Some(path) = &csv_out {
        let mut out = String::from(
            "hash,name,name_source,name_candidates,n_models,as_root,in_swit,as_leaf,anim_tracks,anim_skel,depth_min,depth_max,max_bbox,offset,states,example_models\n",
        );
        // Most-shared nodes first — the shared human/vehicle rig bones float to the top.
        let mut rows: Vec<(&u32, &BoneRec)> = bones.iter().collect();
        rows.sort_by(|a, b| {
            b.1.models
                .len()
                .cmp(&a.1.models.len())
                .then(b.1.anim_tracks.cmp(&a.1.anim_tracks))
                .then(a.0.cmp(b.0))
        });
        for (h, r) in rows {
            let (name, source, ncand) = match resolved.get(h) {
                Some(res) => (
                    res.names.first().cloned().unwrap_or_default(),
                    res.source,
                    res.names.len(),
                ),
                None => (String::new(), "", 0),
            };
            let ex: Vec<String> =
                r.models.iter().take(3).map(|m| format!("0x{m:08X}")).collect();
            let states: Vec<&str> = r.states.iter().map(|s| s.as_str()).collect();
            out.push_str(&format!(
                "0x{h:08X},{name},{source},{ncand},{},{},{},{},{},{},{},{},{:.2},{},{},{}\n",
                r.models.len(),
                r.as_root,
                r.in_swit,
                r.as_leaf,
                r.anim_tracks,
                r.anim_skel,
                r.depth_min,
                r.depth_max,
                r.max_bbox,
                u8::from(r.offset),
                states.join("|"),
                ex.join("|"),
            ));
        }
        std::fs::write(path, out).map_err(|e| format!("write {path}: {e}"))?;
        println!("\ncsv -> {path}  ({} rows)", bones.len());
    }

    // ---- per-model skeleton: the structure the aggregate census cannot carry ----
    if let Some(path) = &skeleton_csv {
        let mut out = String::from(
            "model,node_idx,hash,parent,depth,wx,wy,wz,leaf,in_swit\n",
        );
        for r in &skel {
            out.push_str(&format!(
                "0x{:08X},{},0x{:08X},{},{},{:.4},{:.4},{:.4},{},{}\n",
                r.model,
                r.idx,
                r.hash,
                r.parent.map(|p| format!("0x{p:08X}")).unwrap_or_default(),
                r.depth,
                r.world[0],
                r.world[1],
                r.world[2],
                u8::from(r.leaf),
                u8::from(r.swit),
            ));
        }
        std::fs::write(path, out).map_err(|e| format!("write {path}: {e}"))?;
        println!("skeleton -> {path}  ({} rows)", skel.len());
    }

    // A model's ROOT node hash should equal m2(model name) — so every root we can name is a
    // FREE second witness for the hash algorithm, and roots we cannot name are the naming gap.
    let named_roots = model_roots
        .values()
        .filter(|h| resolved.get(h).map(|r| r.source == "rainbow").unwrap_or(false))
        .count();
    println!(
        "model roots: {} ({named_roots} rainbow-named, {} unnamed)",
        model_roots.len(),
        model_roots.len() - named_roots
    );
    Ok(())
}

/// Load `tools/rainbow_table.json` ONCE and pull the `pandemic_hash_m2` entries for `want`.
/// (`worldutil::rainbow_names` re-scans the 70 MB text per hash — fine for one model's 100 nodes,
/// quadratic for a whole-stack census.)
fn load_rainbow(want: &BTreeSet<u32>) -> HashMap<u32, Vec<String>> {
    let mut out = HashMap::new();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../rainbow_table.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("[bone-census] no rainbow table at {path} — names will be blank");
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return out };
    let Some(map) = v.get("pandemic_hash_m2").and_then(|m| m.as_object()) else { return out };
    for (k, val) in map {
        let Some(h) = k.strip_prefix("0x").and_then(|s| u32::from_str_radix(s, 16).ok()) else {
            continue;
        };
        if !want.contains(&h) {
            continue;
        }
        let names: Vec<String> = match val {
            serde_json::Value::Array(a) => {
                a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect()
            }
            serde_json::Value::String(s) => vec![s.clone()],
            _ => continue,
        };
        if !names.is_empty() {
            out.insert(h, names);
        }
    }
    out
}
