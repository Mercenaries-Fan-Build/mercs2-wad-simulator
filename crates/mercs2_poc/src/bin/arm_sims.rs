//! Throwaway: dump per-target-bone conform SIM for the arm chain, plus the LBS seam gap
//! (how far two adjacent bones' sims place the SAME point). Uses the donor .block exactly
//! like xfer_apply (ESTIMATED transform, container_verts=None).
//!   arm_sims <model.glb> <donor.block>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::mat::{len, rot_angle_deg, sub, Sim};
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn rot_only(s: &Sim) -> [f64; 9] {
    let inv = 1.0 / s.scale.max(1e-12);
    std::array::from_fn(|i| s.sr[i] * inv)
}
// relative rotation angle between two sims (deg): rot_b * rot_a^T
fn rel_deg(a: &Sim, b: &Sim) -> f64 {
    let ra = rot_only(a);
    let rb = rot_only(b);
    // ra^T
    let rat = [ra[0], ra[3], ra[6], ra[1], ra[4], ra[7], ra[2], ra[5], ra[8]];
    // rb * rat
    let mut m = [0.0f64; 9];
    for r in 0..3 {
        for c in 0..3 {
            let mut s = 0.0;
            for k in 0..3 {
                s += rb[r * 3 + k] * rat[k * 3 + c];
            }
            m[r * 3 + c] = s;
        }
    }
    rot_angle_deg(&m)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor_block = std::fs::read(&a[2]).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");

    // Resolve canonical NPC-84 arm bones -> this donor's HIER index (+ name).
    use mercs2_formats::char_skin::npc84_bone_name;
    println!("-- canonical NPC-84 arm bone -> donor HIER index --");
    for npc in [42u32, 43, 44, 45, 46, 63, 64, 65, 66, 67] {
        let nm = npc84_bone_name(npc).unwrap_or("?");
        let hier = target.index_by_canonical(npc);
        println!("   NPC {npc:2} {nm:16} -> donor h{:?}", hier);
    }

    // Which SOURCE (UE) joints map to each arm target bone?
    println!("-- source(UE) joint -> target arm bone (58,59,60,80,81,82) --");
    let mut rows: Vec<(u32, usize, String)> = Vec::new();
    for (&j, &h) in cs.full.iter() {
        if [58u32, 59, 60, 80, 81, 82].contains(&h) {
            let nm = cs.names.get(j).cloned().unwrap_or_default();
            rows.push((h, j, nm));
        }
    }
    rows.sort();
    for (h, j, nm) in &rows {
        println!("   target h{h:3} <- src joint {j:3} '{nm}'");
    }

    let bp = |h: u32| {
        let p = skel.bones[h as usize].bind_pos();
        [p[0] as f64, p[1] as f64, p[2] as f64]
    };
    let parent = |h: u32| skel.bones[h as usize].parent;

    // Right arm 55,57,58,59,60 ; left arm 77,79,80,81,82
    let chains: [(&str, &[u32]); 2] = [
        ("RIGHT", &[20, 55, 57, 58, 59, 60, 61, 62]),
        ("LEFT", &[20, 77, 79, 80, 81, 82, 83, 84]),
    ];
    for (label, chain) in chains {
        println!("\n==== {label} arm chain ====");
        println!("  h  parent   scale   rot   rank pairs | dir(src->tgt applied vs bone-dir deg) | reldeg-to-parent");
        for &h in chain {
            let par = parent(h);
            match cs.bone_sims.get(&h) {
                Some(&(sim, np)) => {
                    let ro = rot_only(&sim);
                    let deg = rot_angle_deg(&ro);
                    // relative rotation vs the parent bone's sim (LBS skew driver)
                    let reld = if par >= 0 {
                        cs.bone_sims.get(&(par as u32)).map(|&(ps, _)| rel_deg(&ps, &sim))
                    } else {
                        None
                    };
                    let reld_s = reld.map(|d| format!("{d:6.1}")).unwrap_or("   -- ".into());
                    println!(
                        "  {h:3} {par:6}  s={:.3} rot={:6.1} rank={} pairs={np:2} | reldeg={reld_s}",
                        sim.scale, deg, sim.rank
                    );
                }
                None => println!("  {h:3} {par:6}  (no sim — not a primary target bone)"),
            }
        }
        if label == "RIGHT" {
            // For each conformed forearm vertex, look at SOURCE weights mapped to target bones,
            // and compare actual posed pos vs applying the DOMINANT bone's sim alone (rigid).
            let mut nvtot = 0usize;
            let mut multi = 0usize;
            let mut bonehist: HashMap<u32, usize> = HashMap::new();
            let mut resid_lbs = 0.0f64;
            let mut resid_n = 0usize;
            for vi in 0..cs.posed.len() {
                let p = cs.posed[vi];
                // right forearm region in conformed space
                if !(p[0] > 0.44 && p[0] < 0.50 && p[1] > 0.88 && p[1] < 1.00) {
                    continue;
                }
                nvtot += 1;
                // gather source weights -> target bones
                let mut tb: Vec<(u32, f64)> = Vec::new();
                for k in 0..4 {
                    let w = glb.vweights[vi][k];
                    if w <= 0.0 { continue; }
                    if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) {
                        if let Some(e) = tb.iter_mut().find(|(b,_)| *b==h) { e.1 += w; } else { tb.push((h,w)); }
                        *bonehist.entry(h).or_default() += 1;
                    }
                }
                if tb.len() > 1 { multi += 1; }
                // dominant target bone
                if let Some(&(dh, _)) = tb.iter().max_by(|a,b| a.1.partial_cmp(&b.1).unwrap()) {
                    if let Some(&(sim, _)) = cs.bone_sims.get(&dh) {
                        let rigid = sim.apply(cs.cp[vi]);
                        resid_lbs += len(sub(rigid, p));
                        resid_n += 1;
                    }
                }
            }
            println!("  -- right forearm SOURCE-weight analysis: {nvtot} verts, {multi} multi-bone ({:.0}%) --",
                100.0*multi as f64/nvtot.max(1) as f64);
            let mut bh: Vec<_> = bonehist.into_iter().collect();
            bh.sort_by_key(|&(_,c)| std::cmp::Reverse(c));
            print!("     target-bone influence counts: ");
            for (b,c) in bh.iter().take(8) { print!("h{b}:{c} "); }
            println!();
            println!("     mean |LBS_posed - dominant_bone_rigid| = {:.4} m over {resid_n} verts (0 => single-bone rigid)",
                resid_lbs / resid_n.max(1) as f64);
            // Dump cp (container-space source), posed, and dominant target bone per vertex to TSV.
            let mut tsv = String::from("idx\tcpx\tcpy\tcpz\tpx\tpy\tpz\tdom\n");
            for vi in 0..cs.posed.len() {
                let mut tb: Vec<(u32, f64)> = Vec::new();
                for k in 0..4 {
                    let w = glb.vweights[vi][k];
                    if w <= 0.0 { continue; }
                    if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) {
                        if let Some(e) = tb.iter_mut().find(|(b,_)| *b==h) { e.1 += w; } else { tb.push((h,w)); }
                    }
                }
                let dom = tb.iter().max_by(|a,b| a.1.partial_cmp(&b.1).unwrap()).map(|&(h,_)| h as i64).unwrap_or(-1);
                let c = cs.cp[vi]; let p = cs.posed[vi];
                tsv.push_str(&format!("{vi}\t{:.5}\t{:.5}\t{:.5}\t{:.5}\t{:.5}\t{:.5}\t{dom}\n", c[0],c[1],c[2],p[0],p[1],p[2]));
            }
            std::fs::write("C:/Users/Shadow/AppData/Local/Temp/inv_B_cp_posed.tsv", tsv).unwrap();
            println!("     wrote cp/posed/dom TSV");

            // Scenario recompute for the WIDE right forearm+wrist region. Substitute sims to see
            // which bone's independent fit causes the shear. Recompute posed = LBS over source wts.
            let s58 = cs.bone_sims.get(&58).map(|&(s,_)| s);
            let recompute = |sub59: Option<Sim>, sub60: Option<Sim>| -> String {
                let mut out = String::from("cx\tcy\tcz\tpx\tpy\tpz\n");
                for vi in 0..cs.posed.len() {
                    let p0 = cs.posed[vi];
                    if !(p0[0] > 0.42 && p0[0] < 0.56 && p0[1] > 0.80 && p0[1] < 1.05) { continue; }
                    let v = cs.cp[vi];
                    let mut acc = [0.0f64; 3]; let mut tot = 0.0;
                    for k in 0..4 {
                        let w = glb.vweights[vi][k];
                        if w <= 0.0 { continue; }
                        let j = glb.vjoints[vi][k] as usize;
                        let Some(&h) = cs.full.get(&j) else { continue };
                        let sim = match h {
                            59 => sub59.or_else(|| cs.bone_sims.get(&59).map(|&(s,_)| s)),
                            60 => sub60.or_else(|| cs.bone_sims.get(&60).map(|&(s,_)| s)),
                            _ => cs.bone_sims.get(&h).map(|&(s,_)| s),
                        };
                        let Some(sim) = sim else { continue };
                        let q = sim.apply(v);
                        acc[0]+=w*q[0]; acc[1]+=w*q[1]; acc[2]+=w*q[2]; tot+=w;
                    }
                    if tot <= 0.0 { continue; }
                    out.push_str(&format!("{:.5}\t{:.5}\t{:.5}\t{:.5}\t{:.5}\t{:.5}\n",
                        v[0],v[1],v[2], acc[0]/tot,acc[1]/tot,acc[2]/tot));
                }
                out
            };
            let base=recompute(None,None);
            std::fs::write("C:/Users/Shadow/AppData/Local/Temp/sc_base.tsv", base).unwrap();
            std::fs::write("C:/Users/Shadow/AppData/Local/Temp/sc_roll58.tsv", recompute(s58,None)).unwrap();
            std::fs::write("C:/Users/Shadow/AppData/Local/Temp/sc_hand58.tsv", recompute(None,s58)).unwrap();
            std::fs::write("C:/Users/Shadow/AppData/Local/Temp/sc_both58.tsv", recompute(s58,s58)).unwrap();
            println!("     wrote scenario TSVs (base / roll->forearm / hand->forearm / both)");
            // Also compare each arm bone's sim rotation/scale spread vs bone 60
            if let Some(&(s60,_)) = cs.bone_sims.get(&60) {
                for h in [58u32,59,60] {
                    if let Some(&(s,_)) = cs.bone_sims.get(&h) {
                        println!("     bone {h}: scale {:.3}  |t-t60| {:.4} m  reldeg-to-60 {:.1}",
                            s.scale, len(sub(s.t, s60.t)), rel_deg(&s60,&s));
                    }
                }
            }
        }
        // Seam gap: evaluate consecutive bones' sims at the SHARED joint (child head = TGT of child)
        println!("  -- LBS seam gap at each joint (|sim_parent(x) - sim_child(x)| at child head) --");
        for w in chain.windows(2) {
            let (pa, ch) = (w[0], w[1]);
            let x = bp(ch); // the joint position (child head), in target space it's the shared point
            if let (Some(&(sa, _)), Some(&(sb, _))) = (cs.bone_sims.get(&pa), cs.bone_sims.get(&ch)) {
                let ga = sa.apply(x);
                let gb = sb.apply(x);
                println!(
                    "     joint {pa}->{ch} at [{:.3},{:.3},{:.3}]  gap = {:.4} m",
                    x[0], x[1], x[2],
                    len(sub(ga, gb))
                );
            }
        }
    }
}
