//! Throwaway diagnostic: run char_skin on a glb against a JSON target skeleton and dump the
//! per-bone re-pose for the neck/head chain + the validation battery. Usage:
//!   char_diag <model.glb> <skeleton_npc84.json>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::mat::{dot, len, norm, sub};
use mercs2_formats::char_skin::{build_character, validate, TargetBone, TargetSkeleton};
use std::collections::HashMap;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let glb = gltf::load_char_glb(&args[1]).expect("glb");
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&args[2]).expect("json")).unwrap();
    let bones: Vec<TargetBone> = j["bones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let p = b["pos"].as_array().unwrap();
            let name = b["name"].as_str().unwrap().to_string();
            // Real HIER hash: prefer an explicit "name_hash", else the 0xHHHHHHHH embedded in an
            // export-bundle node name (node7_LOD0_0x24C5009C), else hash the resolved name.
            let name_hash = b["name_hash"].as_u64().map(|h| h as u32).unwrap_or_else(|| {
                name.rfind("0x")
                    .and_then(|i| u32::from_str_radix(&name[i + 2..].trim_end_matches(|c: char| !c.is_ascii_hexdigit()), 16).ok())
                    .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(&name))
            });
            TargetBone {
                i: b["i"].as_u64().unwrap() as u32,
                pos: [p[0].as_f64().unwrap(), p[1].as_f64().unwrap(), p[2].as_f64().unwrap()],
                parent: b["parent"].as_i64().unwrap() as i32,
                name_hash,
                name,
            }
        })
        .collect();
    let ys: Vec<f64> = bones.iter().map(|b| b.pos[1]).collect();
    let height = ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min);
    let target = TargetSkeleton { bones, height };

    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");
    println!("mode {:?}  verts {}  palette {}/{} runs  height {:.3}", cs.mode, cs.stats.verts, cs.palette_slots, cs.stats.range_count, cs.stats.height);
    println!("alignments: {} rotated ({:.1}..{:.1} deg mean {:.1}), {} rejected", cs.stats.rotated_bones, 0.0, cs.stats.align_max_deg, cs.stats.align_mean_deg, cs.stats.rejected_alignments);

    // per-joint mapping + SRCP for the neck/head region
    println!("\n-- joint -> HIER (name)  SRCP(container)  --");
    let name_of = |h: u32| target.bones.iter().find(|b| b.i == h).map(|b| b.name.clone()).unwrap_or_default();
    let mut js: Vec<usize> = cs.full.keys().copied().collect();
    js.sort();
    for jj in js {
        let h = cs.full[&jj];
        let nm = cs.names.get(jj).cloned().unwrap_or_default();
        let low = nm.to_lowercase();
        if low.contains("neck") || low.contains("head") || low.contains("spine") || low.contains("torso") {
            let srcp = cs.srcp.get(&jj).map(|p| format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2])).unwrap_or("--".into());
            let tgt = target.tgt(h).map(|p| format!("[{:.3},{:.3},{:.3}]", p[0], p[1], p[2])).unwrap_or_default();
            println!("  j{jj:3} {nm:16} -> {h:3} {:16}  SRCP {srcp}  TGT {tgt}", name_of(h));
        }
    }

    // re-posed bbox of the HEAD-weighted verts vs the target head position
    let head_hier = target.bones.iter().find(|b| b.name.eq_ignore_ascii_case("Bone_Head")).map(|b| b.i);
    if let Some(hh) = head_hier {
        let mut mn = [f64::MAX; 3];
        let mut mx = [f64::MIN; 3];
        let mut n = 0;
        for vi in 0..cs.stats.verts {
            // dominant bone
            let mut best = (-1.0f64, u32::MAX);
            for k in 0..4 {
                let w = glb.vweights[vi][k];
                if w > best.0 {
                    if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) {
                        best = (w, h);
                    }
                }
            }
            if best.1 == hh {
                for c in 0..3 { mn[c] = mn[c].min(cs.posed[vi][c]); mx[c] = mx[c].max(cs.posed[vi][c]); }
                n += 1;
            }
        }
        if n > 0 {
            println!("\nHEAD verts (dom=Bone_Head {hh}): {n} verts, reposed bbox min[{:.3},{:.3},{:.3}] max[{:.3},{:.3},{:.3}]", mn[0],mn[1],mn[2],mx[0],mx[1],mx[2]);
            println!("  Bone_Head target pos: {:?}", target.tgt(hh).unwrap());
        }
    }

    // limb directions with RESOLVED (hero) indices — the built-in check uses NPC-84 indices and
    // silently finds nothing on a hero target.
    println!("\n-- limb direction (resolved) --");
    let centroid = |dom_h: u32| -> Option<[f64; 3]> {
        let (mut acc, mut n) = ([0.0f64; 3], 0.0);
        for vi in 0..cs.stats.verts {
            let mut best = (-1.0f64, u32::MAX);
            for k in 0..4 {
                let w = glb.vweights[vi][k];
                if w > best.0 {
                    if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) {
                        best = (w, h);
                    }
                }
            }
            if best.1 == dom_h {
                for c in 0..3 { acc[c] += cs.posed[vi][c]; }
                n += 1.0;
            }
        }
        if n > 0.0 { Some([acc[0]/n, acc[1]/n, acc[2]/n]) } else { None }
    };
    for (label, np, nd) in [("L arm", 43u32, 46u32), ("R arm", 64, 67), ("L leg", 6, 8), ("R leg", 10, 12)] {
        let (Some(pi), Some(di)) = (target.index_by_canonical(np), target.index_by_canonical(nd)) else { println!("  {label}: unresolved"); continue; };
        let (mp, md, tp, td) = (centroid(pi), centroid(di), target.tgt(pi), target.tgt(di));
        if let (Some(mp), Some(md), Some(tp), Some(td)) = (mp, md, tp, td) {
            let mdir = norm(sub(md, mp));
            let tdir = norm(sub(td, tp));
            let deg = dot(mdir, tdir).clamp(-1.0, 1.0).acos() * 180.0 / std::f64::consts::PI;
            println!("  {label}: mesh vs bone {deg:.1} deg  (mesh {mp:?}->{md:?})", );
        } else {
            println!("  {label}: no verts (mesh0 may not include this limb)");
        }
    }

    // PALETTE render-consistency, SET-BASED. skin_bytes[k] does NOT positionally correspond to
    // vjoints[k] (duplicate slots are merged and the pairs are re-sorted by weight), so compare the
    // decoded SET of global bones against the intended SET.
    {
        let palette = mercs2_formats::char_skin::expand_ranges(&cs.ranges);
        let mut bad = 0usize;
        let mut max_slot = 0u8;
        for vi in 0..cs.stats.verts {
            let mut decoded: Vec<u16> = Vec::new();
            for k in 0..4 {
                let w = cs.skin_bytes[vi * 8 + 4 + k];
                if w == 0 { continue; }
                let slot = cs.skin_bytes[vi * 8 + k];
                max_slot = max_slot.max(slot);
                if let Some(&g) = palette.get(slot as usize) { decoded.push(g); }
            }
            let mut intended: Vec<u16> = Vec::new();
            for k in 0..4 {
                if glb.vweights[vi][k] <= 0.0 { continue; }
                if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) { intended.push(h as u16); }
            }
            decoded.sort_unstable(); decoded.dedup();
            intended.sort_unstable(); intended.dedup();
            // truncation to 4 influences can legitimately drop the smallest; only flag a decoded
            // bone the vertex was never weighted to.
            if decoded.iter().any(|d| !intended.contains(d)) { bad += 1; }
        }
        println!("\n-- palette render check: {} slots, max slot used {}, {} vertices decode to a bone they are NOT weighted to (want 0) --", cs.palette_slots, max_slot, bad);
        println!("   ranges: {:?}", cs.ranges);
    }

    // Dump the re-posed BIND mesh + the SOURCE mesh (container space, pre-repose) as OBJ.
    let prefix = std::env::args().nth(3).unwrap_or_else(|| "char_diag".into());
    {
        let write = |name: &str, pts: &[[f64; 3]]| {
            let mut obj = String::new();
            for p in pts {
                obj.push_str(&format!("v {} {} {}\n", p[0], p[1], p[2]));
            }
            for t in &glb.tris {
                obj.push_str(&format!("f {} {} {}\n", t[0] + 1, t[1] + 1, t[2] + 1));
            }
            std::fs::write(name, obj).unwrap();
        };
        write(&format!("{prefix}_bind.obj"), &cs.posed);
        write(&format!("{prefix}_src.obj"), &cs.cp);
        println!("\nwrote {prefix}_bind.obj / {prefix}_src.obj ({} verts, {} tris)", cs.posed.len(), glb.tris.len());
        let bb = |pts: &[[f64; 3]]| {
            let mut mn = [f64::MAX; 3];
            let mut mx = [f64::MIN; 3];
            for p in pts { for c in 0..3 { mn[c] = mn[c].min(p[c]); mx[c] = mx[c].max(p[c]); } }
            format!("x[{:.3},{:.3}] y[{:.3},{:.3}] z[{:.3},{:.3}]", mn[0],mx[0],mn[1],mx[1],mn[2],mx[2])
        };
        println!("  src  bbox {}", bb(&cs.cp));
        println!("  bind bbox {}", bb(&cs.posed));
    }

    // --- fit quality + per-bone displacement (TGT - SRCP) ---
    println!("\n-- fit residual {:.4} m; per-bone displacement |TGT-SRCP| --", cs.stats.fit_residual);
    {
        let mut rows: Vec<(f64, String)> = Vec::new();
        let mut js: Vec<usize> = cs.full.keys().copied().collect();
        js.sort();
        for jj in js {
            let h = cs.full[&jj];
            let (Some(&s), Some(t)) = (cs.srcp.get(&jj), target.tgt(h)) else { continue };
            let d = len(sub(t, s));
            rows.push((d, format!("j{jj:3} {:20} -> {:24} d={:.3}  SRCP y{:.3} TGT y{:.3}",
                cs.names.get(jj).cloned().unwrap_or_default(), name_of(h), d, s[1], t[1])));
        }
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let n = rows.len() as f64;
        let mean = rows.iter().map(|r| r.0).sum::<f64>() / n;
        println!("  mean displacement over {} mapped joints: {:.3} m", rows.len(), mean);
        for r in rows.iter().take(12) { println!("   {}", r.1); }
    }

    // --- bone-segment length: source (SRCP) vs target, for the mapped chain ---
    println!("\n-- segment length source vs target (mapped parent/child pairs) --");
    {
        let jidx: std::collections::HashMap<usize, usize> = glb.joint_nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let mut js: Vec<usize> = cs.full.keys().copied().collect();
        js.sort();
        let mut rows = Vec::new();
        for &jj in &js {
            let h = cs.full[&jj];
            // find nearest mapped ancestor joint with a DIFFERENT target bone
            let mut cur = glb.joint_nodes[jj] as i32;
            let mut pj = None;
            loop {
                let p = glb.node_parent.get(cur as usize).copied().unwrap_or(-1);
                if p < 0 { break; }
                cur = p;
                if let Some(&k) = jidx.get(&(cur as usize)) {
                    if let Some(&ph) = cs.full.get(&k) { if ph != h { pj = Some((k, ph)); } break; }
                }
            }
            let Some((k, ph)) = pj else { continue };
            let (Some(&sc), Some(&sp)) = (cs.srcp.get(&jj), cs.srcp.get(&k)) else { continue };
            let (Some(tc), Some(tp)) = (target.tgt(h), target.tgt(ph)) else { continue };
            let ls = len(sub(sc, sp));
            let lt = len(sub(tc, tp));
            if ls < 1e-4 && lt < 1e-4 { continue; }
            rows.push((if ls > 1e-6 { lt / ls } else { f64::INFINITY }, format!("{:18} -> {:18}  src {:.3}  tgt {:.3}  ratio {:.2}",
                cs.names.get(k).cloned().unwrap_or_default(), cs.names.get(jj).cloned().unwrap_or_default(), ls, lt, if ls > 1e-6 { lt/ls } else { f64::INFINITY })));
        }
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        for r in &rows { println!("   {}", r.1); }
    }

    // --- machine-readable correspondence dump for independent analysis ---
    {
        use mercs2_formats::char_skin::mat::{inv4, origin_of, IDENT4, allclose};
        let mut rows = Vec::new();
        for jj in 0..glb.joint_nodes.len() {
            let ibm_raw = glb.ibm.get(jj).and_then(|o| o.as_ref()).and_then(|m| {
                if allclose(m, &IDENT4, 1e-6) { None } else { inv4(m).map(|iv| origin_of(&iv)) }
            });
            let node = glb.joint_nodes[jj];
            let nw = glb.node_world.get(node).map(origin_of);
            let h = cs.full.get(&jj).copied();
            let tp = h.and_then(|h| target.tgt(h));
            rows.push(serde_json::json!({
                "j": jj,
                "name": cs.names.get(jj).cloned().unwrap_or_default(),
                "node": node,
                "parent": glb.node_parent.get(node).copied().unwrap_or(-1),
                "ibm_raw": ibm_raw,
                "node_world": nw,
                "hier": h,
                "tgt": tp,
                "srcp": cs.srcp.get(&jj).copied(),
            }));
        }
        let out = serde_json::json!({
            "joints": rows,
            "vjoints": glb.vjoints,
            "vweights": glb.vweights,
            "fit_residual": cs.stats.fit_residual,
        });
        std::fs::write(format!("{prefix}_corr.json"), serde_json::to_string(&out).unwrap()).unwrap();
    }

    println!("\n-- per-target-bone re-pose (scale / rot deg / pairs / rank) --");
    {
        let mut hs: Vec<u32> = cs.bone_sims.keys().copied().collect();
        hs.sort();
        for h in hs {
            let (sim, np) = cs.bone_sims[&h];
            let inv = 1.0 / sim.scale.max(1e-12);
            let r: [f64; 9] = std::array::from_fn(|i| sim.sr[i] * inv);
            let deg = mercs2_formats::char_skin::mat::rot_angle_deg(&r);
            let flag = if deg > 45.0 || sim.scale > 1.8 || sim.scale < 0.6 { " <<<" } else { "" };
            println!("   h{h:3} {:26} s={:.3} rot={:6.1} pairs={np:2} rank={}{flag}", name_of(h), sim.scale, deg, sim.rank);
        }
    }

    // SEAM DISCONTINUITY: at each shared joint, how far apart do the two adjoining bones'
    // transforms send the SAME source point? That gap is exactly the tear/stretch LBS has to
    // smear over, so it is the objective measure of "torn shoulder / taffy knee".
    println!("\n-- seam discontinuity at shared joints --");
    {
        let jidx: std::collections::HashMap<usize, usize> =
            glb.joint_nodes.iter().enumerate().map(|(i, &n)| (n, i)).collect();
        let jparent = |j: usize| -> Option<usize> {
            let mut cur = glb.joint_nodes[j] as i32;
            loop {
                let p = glb.node_parent.get(cur as usize).copied().unwrap_or(-1);
                if p < 0 { return None; }
                cur = p;
                if let Some(&pj) = jidx.get(&(cur as usize)) { return Some(pj); }
            }
        };
        let mut rows: Vec<(f64, String)> = Vec::new();
        let mut js: Vec<usize> = cs.full.keys().copied().collect();
        js.sort();
        for &j in &js {
            let h = cs.full[&j];
            let Some(&sp) = cs.srcp.get(&j) else { continue };
            let mut cur = jparent(j);
            while let Some(pj) = cur {
                if let Some(&ph) = cs.full.get(&pj) {
                    if ph != h {
                        if let (Some(&(a, _)), Some(&(b, _))) = (cs.bone_sims.get(&h), cs.bone_sims.get(&ph)) {
                            let d = len(sub(a.apply(sp), b.apply(sp)));
                            rows.push((d, format!("{:20} | {:24} vs {:24} gap {:.4}",
                                cs.names.get(j).cloned().unwrap_or_default(), name_of(h), name_of(ph), d)));
                        }
                        break;
                    }
                }
                cur = jparent(pj);
            }
        }
        rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let n = rows.len().max(1) as f64;
        let mean = rows.iter().map(|r| r.0).sum::<f64>() / n;
        let max = rows.first().map(|r| r.0).unwrap_or(0.0);
        println!("  SEAM mean {:.4} m  max {:.4} m  over {} joints", mean, max, rows.len());
        // LANDMARK: how far each bone's own anchor lands from its target bone position — the
        // conformance measure that trades off against SEAM.
        let mut lm: Vec<f64> = Vec::new();
        for (&h, &(sim, _)) in cs.bone_sims.iter() {
            let anchor = js.iter().find(|&&j| cs.full.get(&j) == Some(&h) && cs.srcp.contains_key(&j));
            if let (Some(&j), Some(t)) = (anchor, target.tgt(h)) {
                lm.push(len(sub(sim.apply(cs.srcp[&j]), t)));
            }
        }
        lm.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let lmean = lm.iter().sum::<f64>() / lm.len().max(1) as f64;
        println!("  LANDMARK mean {:.4} m  max {:.4} m  over {} bones", lmean, lm.last().copied().unwrap_or(0.0), lm.len());
        for r in rows.iter().take(6) { println!("    {}", r.1); }
    }

    println!("\n-- validation --");
    let rep = validate::validate(&cs, &glb.vjoints, &glb.vweights, &glb.indices);
    for c in &rep.checks {
        println!("  [{:?}] {}: {}", c.status, c.title, c.text);
    }
}
