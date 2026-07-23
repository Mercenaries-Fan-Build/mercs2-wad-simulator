//! Throwaway measurement: reproduce xfer_apply's exact conform (build_character on the donor-derived
//! skeleton, None container, no overrides) and dump the per-target-bone re-pose SIMILARITY for a
//! chain of bones, decomposing each bone's rotation relative to its parent into SWING vs TWIST about
//! the target arm axis. A bind-pose forearm "twist" is a per-bone-roll defect that centroid probes
//! cannot see.
//!
//!   armfit_dump <model.glb> <donor.block> <b0,b1,b2,...>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::mat::{
    cross, dot, kabsch_rot, len, mul3, norm, rot_angle_deg, sub, transpose3,
};
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn rot_axis_angle(m: &[f64; 9]) -> ([f64; 3], f64) {
    // angle
    let tr = m[0] + m[4] + m[8];
    let c = ((tr - 1.0) * 0.5).clamp(-1.0, 1.0);
    let ang = c.acos();
    // axis from skew part
    let ax = [m[7] - m[5], m[2] - m[6], m[3] - m[1]];
    let l = len(ax);
    if l < 1e-9 {
        return ([0.0, 0.0, 1.0], ang);
    }
    ([ax[0] / l, ax[1] / l, ax[2] / l], ang)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let chain: Vec<u32> = a[3].split(',').filter_map(|s| s.parse().ok()).collect();
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor_block = std::fs::read(&a[2]).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");

    let bp: Vec<[f64; 3]> = skel
        .bones
        .iter()
        .map(|b| {
            let p = b.bind_pos();
            [p[0] as f64, p[1] as f64, p[2] as f64]
        })
        .collect();

    println!("chain {:?}", chain);
    println!(" bone   scale  rank pairs  rotDeg | relParentDeg  swingDeg  twistDeg  (twist about target arm axis)");
    for (idx, &h) in chain.iter().enumerate() {
        let Some(&(sim, np)) = cs.bone_sims.get(&h) else {
            println!(" {h:>4}   -- no sim --");
            continue;
        };
        let inv = 1.0 / sim.scale.max(1e-12);
        let r: [f64; 9] = std::array::from_fn(|i| sim.sr[i] * inv);
        let rot_deg = rot_angle_deg(&r);
        // relative rotation to parent bone in chain
        let mut relstr = String::from("      (root)");
        if idx > 0 {
            let ph = chain[idx - 1];
            if let Some(&(psim, _)) = cs.bone_sims.get(&ph) {
                let pinv = 1.0 / psim.scale.max(1e-12);
                let pr: [f64; 9] = std::array::from_fn(|i| psim.sr[i] * pinv);
                // Rrel = R * Rparent^T  (child relative to parent)
                let rrel = mul3(&r, &transpose3(&pr));
                let rel_deg = rot_angle_deg(&rrel);
                // arm axis = target bind direction bone(idx-1) -> bone(idx)
                let axis = norm(sub(bp[h as usize], bp[ph as usize]));
                let (ax, ang) = rot_axis_angle(&rrel);
                let twist = (ang * dot(ax, axis)).to_degrees();
                // swing = component perpendicular
                let perp = cross(ax, axis);
                let swing = (ang * len(perp)).to_degrees();
                relstr = format!("     {rel_deg:7.1}   {swing:7.1}   {twist:7.1}");
            }
        }
        println!(
            " {h:>4}   {:.3}   {}   {np:>3}   {rot_deg:6.1} |{relstr}",
            sim.scale, sim.rank
        );
    }

    // Which source joints map to each chain bone (via cs.full), whether each has a sim, and the
    // origin — so a bone with NO transform can be traced to "no source joint mapped here".
    println!("\n mapping per chain bone (donor HIER):");
    // canonical identity of each donor bone by name-hash
    let canon = |h: u32| -> String {
        let nh = skel.bones[h as usize].name_hash;
        for (ci, nm) in mercs2_formats::char_skin::NPC84_NAMES.iter().enumerate() {
            if !nm.starts_with("hash_")
                && mercs2_formats::hash::pandemic_hash_m2(nm) == nh
            {
                return format!("{nm}(npc{ci})");
            }
        }
        format!("0x{nh:08X}")
    };
    for &h in &chain {
        println!("  HIER {h:>3} = {}", canon(h));
    }
    for &h in &chain {
        let js: Vec<usize> = cs.full.iter().filter(|(_, &v)| v == h).map(|(&j, _)| j).collect();
        let has_sim = cs.bone_sims.contains_key(&h);
        let names: Vec<String> = js
            .iter()
            .map(|&j| cs.names.get(j).cloned().unwrap_or_default())
            .collect();
        println!("  {h:>3}  sim={has_sim:<5} src_joints={js:?}  names={names:?}");
    }

    // ---- CONDITIONING of each bone's MLS similarity fit ----
    // Replicate the control set: one point per used target bone = (srcp[primary], tgt, weight).
    // rank counts sig>1e-9*sig0; conditioning sig2/sig0 says how well the ROLL DOF is pinned.
    use mercs2_formats::char_skin::automap::Origin;
    let rank_of = |o: Option<&Origin>| match o {
        Some(Origin::Manual) => 0u8,
        Some(Origin::Auto) => 1,
        Some(Origin::Inherited) => 2,
        _ => 3,
    };
    // primary joint per hier (lowest-index among minimal rank), matching build.rs
    let mut sorted_full: Vec<usize> = cs.full.keys().copied().collect();
    sorted_full.sort_unstable();
    let mut primary: HashMap<u32, usize> = HashMap::new();
    for &j in &sorted_full {
        if !cs.srcp.contains_key(&j) {
            continue;
        }
        let h = cs.full[&j];
        let r = rank_of(cs.origin.get(&j));
        match primary.get(&h) {
            Some(&cur) if rank_of(cs.origin.get(&cur)) <= r => {}
            _ => {
                primary.insert(h, j);
            }
        }
    }
    let tgt = |h: u32| -> [f64; 3] { [bp[h as usize][0], bp[h as usize][1], bp[h as usize][2]] };
    // control points
    let control: Vec<(u32, [f64; 3], [f64; 3])> = primary
        .iter()
        .filter_map(|(&h, &pj)| Some((h, *cs.srcp.get(&pj)?, tgt(h))))
        .collect();
    let ys: Vec<f64> = skel.bones.iter().map(|b| b.bind_pos()[1] as f64).collect();
    let height = ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min);
    let sigma = height.abs().max(0.1) * 0.16;
    let two_sig2 = 2.0 * sigma * sigma;
    const W_ANCHOR: f64 = 3.0;
    println!("\n fit conditioning (sig0>=sig1>=sig2; roll DOF ~ sig2/sig0):");
    println!("  bone   sig0     sig1     sig2     sig2/sig0   sig1/sig0   effN");
    let mut cond_bones: Vec<u32> = chain.clone();
    for extra in [3u32, 17, 20, 10] {
        // hips/spine/chest/thigh references (well-spread)
        if !cond_bones.contains(&extra) {
            cond_bones.push(extra);
        }
    }
    for &h in &cond_bones {
        let Some(&anchor) = primary.get(&h).and_then(|pj| cs.srcp.get(pj)) else { continue };
        let mut ms = [0.0; 3];
        let mut md = [0.0; 3];
        let mut wsum = 0.0;
        let mut effn = 0.0;
        let mut ws: Vec<([f64; 3], [f64; 3], f64)> = Vec::new();
        for &(ch, cs_, cd) in &control {
            let d = sub(cs_, anchor);
            let mut w = (-dot(d, d) / two_sig2).exp();
            if ch == h {
                w *= W_ANCHOR;
            }
            if w > 1e-6 {
                ws.push((cs_, cd, w));
                wsum += w;
            }
        }
        for &(s, d, w) in &ws {
            for c in 0..3 {
                ms[c] += w * s[c] / wsum;
                md[c] += w * d[c] / wsum;
            }
            effn += w;
        }
        let mut hmat = [0.0; 9];
        for &(s, d, w) in &ws {
            let sc = sub(s, ms);
            let dc = sub(d, md);
            for r in 0..3 {
                for c in 0..3 {
                    hmat[r * 3 + c] += w * dc[r] * sc[c];
                }
            }
        }
        let (_r, sig) = kabsch_rot(&hmat);
        let s0 = sig[0].abs();
        // source-cloud principal axis (max spread) = eigenvector of H^T H with largest eigenvalue.
        // A rotation about THIS axis is the well-constrained one; the LEAST-constrained rotation is
        // about the axis of MOST spread only if colinear -> actually: colinear cloud along A leaves
        // rotation about A free. So report alignment of the max-spread source axis with the source
        // arm axis; ~1.0 => the free (roll) DOF is exactly arm roll -> visible forearm twist.
        use mercs2_formats::char_skin::mat::{sym_eigen3, transpose3 as tr, mul3 as mm};
        let hth = mm(&tr(&hmat), &hmat);
        let (ew, ev) = sym_eigen3(&hth);
        let mut oi = [0usize, 1, 2];
        oi.sort_by(|&i, &j| ew[j].partial_cmp(&ew[i]).unwrap());
        let top = oi[0];
        let maxdir = norm([ev[top], ev[3 + top], ev[6 + top]]);
        // source arm axis at this bone: primary(h)->primary(child in chain), else use anchor->tgt dir
        let arm_src = {
            // find next chain bone after h with a primary
            let mut d = [0.0, 0.0, 0.0];
            if let Some(pos) = chain.iter().position(|&x| x == h) {
                if pos + 1 < chain.len() {
                    let nh = chain[pos + 1];
                    if let (Some(&pa), Some(&pb)) = (
                        primary.get(&h).and_then(|j| cs.srcp.get(j)),
                        primary.get(&nh).and_then(|j| cs.srcp.get(j)),
                    ) {
                        d = norm(sub(pb, pa));
                    }
                }
            }
            d
        };
        let align = if len(arm_src) > 0.5 { dot(maxdir, arm_src).abs() } else { -1.0 };
        println!(
            "  {h:>4}  {:.4}  {:.4}  {:.4}   {:8.4}   {:8.4}   {:.1}   armAxisAlign={:.3}",
            sig[0],
            sig[1],
            sig[2].abs(),
            if s0 > 0.0 { sig[2].abs() / s0 } else { 0.0 },
            if s0 > 0.0 { sig[1].abs() / s0 } else { 0.0 },
            effn / wsum * ws.len() as f64,
            align,
        );
    }

    // Also print the donor's own bind directions along the chain (target) and the conformed
    // segment directions, so a twist can be seen as a change in the local frame if present.
    println!("\n target arm axis (bind dir) per segment:");
    for i in 1..chain.len() {
        let d = norm(sub(bp[chain[i] as usize], bp[chain[i - 1] as usize]));
        println!(
            "  {:>3}->{:<3}  [{:+.3},{:+.3},{:+.3}]  len {:.3}",
            chain[i - 1],
            chain[i],
            d[0],
            d[1],
            d[2],
            len(sub(bp[chain[i] as usize], bp[chain[i - 1] as usize]))
        );
    }
}
