//! THROWAWAY AUDIT: non-circular check of the conformed HAND orientation.
//!
//! Measures the palm frame on the CONFORMED MESH (`cs.posed`) and compares it with the donor
//! skeleton's own palm frame, plus every determinant in the 6c rotation chain.
//!   handaudit <model.glb> <donor.block>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::mat::*;
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn ang(a: V3, b: V3) -> f64 {
    dot(norm(a), norm(b)).clamp(-1.0, 1.0).acos().to_degrees()
}

/// verbatim copy of `char_skin::ortho3_colvec` (it is pub(crate) there)
fn ortho3(m: [f64; 9]) -> Option<[f64; 9]> {
    let row = |i: usize| [m[i * 3], m[i * 3 + 1], m[i * 3 + 2]];
    let d = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let s = |a: [f64; 3], b: [f64; 3], k: f64| [a[0] - b[0] * k, a[1] - b[1] * k, a[2] - b[2] * k];
    let nd = |a: [f64; 3]| {
        let n = d(a, a).sqrt();
        if n < 1e-9 { None } else { Some([a[0] / n, a[1] / n, a[2] / n]) }
    };
    let u0 = nd(row(0))?;
    let mut r1 = row(1);
    r1 = s(r1, u0, d(r1, u0));
    let u1 = nd(r1)?;
    let mut r2 = row(2);
    r2 = s(r2, u0, d(r2, u0));
    r2 = s(r2, u1, d(r2, u1));
    let u2 = nd(r2)?;
    Some([u0[0], u0[1], u0[2], u1[0], u1[1], u1[2], u2[0], u2[1], u2[2]])
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let blk = std::fs::read(&a[2]).expect("block");
    let sk = Skeleton::from_block(&blk).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&sk);
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");
    println!("mode {:?}  verts {}  fit_resid {:.5}", cs.mode, cs.stats.verts, cs.stats.fit_residual);

    // ---------- recover the model->container linear map A (= `r_global` in build.rs 6c) ----------
    // srcp[j] = apply_fit(t, ibm_raw[j]); solve for t from those pairs, then A[r][c] = t[c][r].
    let mut aa: Vec<Vec<f64>> = Vec::new();
    let mut bb: Vec<Vec<f64>> = Vec::new();
    for (j, m) in glb.ibm.iter().enumerate() {
        let (Some(m), Some(&s)) = (m.as_ref(), cs.srcp.get(&j)) else { continue };
        if allclose(m, &IDENT4, 1e-6) { continue }
        let Some(iv) = inv4(m) else { continue };
        let o = origin_of(&iv);
        aa.push(vec![o[0], o[1], o[2], 1.0]);
        bb.push(vec![s[0], s[1], s[2]]);
    }
    let f = lstsq(&aa, &bb).expect("recover t");
    let t: Fit = [
        [f.x[0][0], f.x[0][1], f.x[0][2]],
        [f.x[1][0], f.x[1][1], f.x[1][2]],
        [f.x[2][0], f.x[2][1], f.x[2][2]],
        [f.x[3][0], f.x[3][1], f.x[3][2]],
    ];
    let r_global: [f64; 9] = [
        t[0][0], t[1][0], t[2][0], t[0][1], t[1][1], t[2][1], t[0][2], t[1][2], t[2][2],
    ];
    println!(
        "recovered A over {} joints (resid {:.6}): det(A) = {:+.6}  {}",
        aa.len(), f.resid_mean, det3(&r_global),
        if det3(&r_global) < 0.0 { "*** REFLECTION ***" } else { "proper" }
    );

    let name_of = |h: u32| target.bones.iter().find(|b| b.i == h).map(|b| b.name.clone()).unwrap_or_default();
    // any source joint mapped to this HIER bone that has a container-space bind position
    let src_joint = |h: u32| -> Option<usize> {
        let mut js: Vec<usize> = cs.full.iter().filter(|(_, &v)| v == h).map(|(&k, _)| k).collect();
        js.sort_unstable();
        js.into_iter().find(|j| cs.srcp.contains_key(j))
    };
    let trot = |h: u32| target.bones.iter().find(|b| b.i == h).and_then(|b| b.rot);

    // ---------- determinants along the 6c chain ----------
    println!("\n-- determinants (6c chain) --");
    for (npc, lbl) in [(44u32, "L forearm"), (46, "L hand"), (65, "R forearm"), (67, "R hand")] {
        let Some(h) = target.index_by_canonical(npc) else { println!("  {lbl}: donor lacks it"); continue };
        let dt = trot(h).map(|r| det3(&r));
        let sj = src_joint(h);
        let rs = sj
            .and_then(|j| glb.ibm.get(j).and_then(|o| *o))
            .and_then(|m| inv4(&m))
            .and_then(|iv| ortho3([iv[0], iv[1], iv[2], iv[4], iv[5], iv[6], iv[8], iv[9], iv[10]]));
        let rsc = rs.and_then(|r| ortho3(mul3(&r_global, &r)));
        let r_new = match (trot(h), rsc) {
            (Some(rt), Some(rc)) => {
                let ct = [rc[0], rc[3], rc[6], rc[1], rc[4], rc[7], rc[2], rc[5], rc[8]];
                Some(mul3(&rt, &ct))
            }
            _ => None,
        };
        println!(
            "  {lbl:<10} hier {h:>3} {:<18} srcj {:?} {:<24} det(R_tgt)={} det(R_src)={} det(A*R_src ortho)={} det(R_new)={}",
            name_of(h), sj, sj.and_then(|j| cs.names.get(j).cloned()).unwrap_or_default(),
            dt.map(|v| format!("{v:+.4}")).unwrap_or("--".into()),
            rs.map(|r| format!("{:+.4}", det3(&r))).unwrap_or("--".into()),
            rsc.map(|r| format!("{:+.4}", det3(&r))).unwrap_or("--".into()),
            r_new.map(|r| format!("{:+.4}", det3(&r))).unwrap_or("--".into()),
        );
    }

    // ---------- MESH-BASED palm frame (non-circular) ----------
    // conformed mesh centroid of the verts dominated by a given HIER bone
    let dom = |vi: usize| -> Option<u32> {
        let mut best = (-1.0f64, None);
        for k in 0..4 {
            let w = glb.vweights[vi][k];
            if w > best.0 {
                if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) { best = (w, Some(h)); }
            }
        }
        best.1
    };
    let centroid = |h: u32, pts: &[[f64; 3]]| -> Option<V3> {
        let (mut acc, mut n) = ([0.0f64; 3], 0.0);
        for vi in 0..cs.stats.verts {
            if dom(vi) == Some(h) {
                for c in 0..3 { acc[c] += pts[vi][c]; }
                n += 1.0;
            }
        }
        if n > 0.0 { Some([acc[0] / n, acc[1] / n, acc[2] / n]) } else { None }
    };

    println!("\n-- palm frame: CONFORMED MESH vs DONOR SKELETON (ground truth) --");
    for (lbl, hand, mid3, thm3, idx3) in
        [("LEFT", 46u32, 53u32, 62u32, 50u32), ("RIGHT", 67, 74, 83, 71)]
    {
        let (Some(hh), Some(hm), Some(ht), Some(hi)) = (
            target.index_by_canonical(hand), target.index_by_canonical(mid3),
            target.index_by_canonical(thm3), target.index_by_canonical(idx3),
        ) else { println!("  {lbl}: donor lacks a finger bone"); continue };
        // donor ground truth from BONE POSITIONS
        let (Some(pw), Some(pm), Some(pt)) = (target.tgt(hh), target.tgt(hm), target.tgt(ht))
        else { continue };
        let f_t = norm(sub(pm, pw));
        let n_t = norm(cross(f_t, norm(sub(pt, pw))));
        // conformed mesh
        let (Some(cw), Some(cm), Some(ct)) =
            (centroid(hh, &cs.posed), centroid(hm, &cs.posed), centroid(ht, &cs.posed))
        else { println!("  {lbl}: no conformed verts on hand/middle/thumb"); continue };
        let f_c = norm(sub(cm, cw));
        let n_c = norm(cross(f_c, norm(sub(ct, cw))));
        // pre-repose source hand, container space (what we started from)
        let (Some(sw), Some(sm), Some(st)) =
            (centroid(hh, &cs.cp), centroid(hm, &cs.cp), centroid(ht, &cs.cp))
        else { continue };
        let f_s = norm(sub(sm, sw));
        let n_s = norm(cross(f_s, norm(sub(st, sw))));
        println!("  {lbl} hand (hier {hh}/{hm}/{ht}/{hi})");
        println!("     finger dir : conformed vs donor {:6.1} deg   (source-in-container vs donor {:6.1})", ang(f_c, f_t), ang(f_s, f_t));
        println!("     PALM NORMAL: conformed vs donor {:6.1} deg   (source-in-container vs donor {:6.1})", ang(n_c, n_t), ang(n_s, n_t));
        // roll about the finger axis: angle between the normals projected perpendicular to f_t
        let perp = |v: V3, ax: V3| { let d = dot(v, ax); norm([v[0]-ax[0]*d, v[1]-ax[1]*d, v[2]-ax[2]*d]) };
        println!("     ROLL about finger axis: {:6.1} deg", ang(perp(n_c, f_t), perp(n_t, f_t)));
        // what the SECTION-6 FIT (cs.bone_sims, pre-6c) would have produced for the same points
        if let (Some(&(fit, np)), Some(jw), Some(jm), Some(jt)) =
            (cs.bone_sims.get(&hh), src_joint(hh), src_joint(hm), src_joint(ht))
        {
            let (Some(&aw), Some(&am), Some(&at)) = (cs.srcp.get(&jw), cs.srcp.get(&jm), cs.srcp.get(&jt)) else { continue };
            let (qw, qm, qt) = (fit.apply(aw), fit.apply(am), fit.apply(at));
            let f_f = norm(sub(qm, qw));
            let n_f = norm(cross(f_f, norm(sub(qt, qw))));
            println!("     [section-6 FIT, pairs={np} rank={}] finger {:6.1} deg  PALM {:6.1} deg", fit.rank, ang(f_f, f_t), ang(n_f, n_t));
        }

        // ---- AXIS-CONVENTION MISMATCH: express the SAME anatomical direction in each rig's own
        // bone-LOCAL frame. 6c's `R_tgt*(A*R_src)^-1` is only valid if these agree.
        let (Some(jw), Some(jm), Some(jt)) = (src_joint(hh), src_joint(hm), src_joint(ht)) else { continue };
        let (Some(&aw), Some(&am), Some(&at)) = (cs.srcp.get(&jw), cs.srcp.get(&jm), cs.srcp.get(&jt)) else { continue };
        let rsc = glb.ibm.get(jw).and_then(|o| *o).and_then(|m| inv4(&m))
            .and_then(|iv| ortho3([iv[0], iv[1], iv[2], iv[4], iv[5], iv[6], iv[8], iv[9], iv[10]]))
            .and_then(|r| ortho3(mul3(&r_global, &r)));
        if let (Some(rt), Some(rc)) = (trot(hh), rsc) {
            let inv_of = |m: &[f64; 9]| [m[0], m[3], m[6], m[1], m[4], m[7], m[2], m[5], m[8]];
            let d_loc = apply3(&inv_of(&rt), f_t);                       // fingers, in DONOR bone-local
            let s_loc = apply3(&inv_of(&rc), norm(sub(am, aw)));         // fingers, in SOURCE bone-local
            let dp_loc = apply3(&inv_of(&rt), norm(sub(pt, pw)));        // thumb, donor-local
            let sp_loc = apply3(&inv_of(&rc), norm(sub(at, aw)));        // thumb, source-local
            println!("     AXIS CONVENTION: finger-dir in bone-local  donor [{:+.2},{:+.2},{:+.2}]  source [{:+.2},{:+.2},{:+.2}]  -> {:.1} deg apart",
                d_loc[0], d_loc[1], d_loc[2], s_loc[0], s_loc[1], s_loc[2], ang(d_loc, s_loc));
            println!("                      thumb-dir  in bone-local  donor [{:+.2},{:+.2},{:+.2}]  source [{:+.2},{:+.2},{:+.2}]  -> {:.1} deg apart",
                dp_loc[0], dp_loc[1], dp_loc[2], sp_loc[0], sp_loc[1], sp_loc[2], ang(dp_loc, sp_loc));
        }

        // ---- PROPOSED FIX: Kabsch over the hand+finger BONE POSITIONS (geometry, not local
        // frames). Rotation only, then apply to the mesh and re-measure the palm on held-out data.
        let mut pairs: Vec<(V3, V3, f64)> = Vec::new();
        // EXCLUDE the thumb (60..62 / 81..83) so the palm-normal check below is genuinely HELD OUT.
        let fingers: Vec<u32> = if hand == 46 { (48..=59).collect() } else { (69..=80).collect() };
        for npc in std::iter::once(hand).chain(fingers) {
            let (Some(hx), ) = (target.index_by_canonical(npc), ) else { continue };
            let (Some(jx), Some(tx)) = (src_joint(hx), target.tgt(hx)) else { continue };
            let Some(&sx) = cs.srcp.get(&jx) else { continue };
            pairs.push((sx, tx, 1.0));
        }
        if let Some(g) = fit_similarity_weighted(&pairs) {
            let (qw, qm, qt) = (g.apply(aw), g.apply(am), g.apply(at));
            let f_g = norm(sub(qm, qw));
            let n_g = norm(cross(f_g, norm(sub(qt, qw))));
            // held-out landmarks: index3 + pinky3 residual under this rotation
            let mut held = 0.0f64; let mut nheld = 0.0f64;
            for npc in if hand == 46 { [60u32, 62] } else { [81, 83] } {
                let (Some(hx), ) = (target.index_by_canonical(npc), ) else { continue };
                let (Some(jx), Some(tx)) = (src_joint(hx), target.tgt(hx)) else { continue };
                let Some(&sx) = cs.srcp.get(&jx) else { continue };
                held += len(sub(g.apply(sx), tx)); nheld += 1.0;
            }
            println!("     [PROPOSED bone-Kabsch, {} pairs rank={} s={:.3}] finger {:6.1} deg  PALM {:6.1} deg  HELD-OUT thumb landmark {:.4} m",
                pairs.len(), g.rank, g.scale, ang(f_g, f_t), ang(n_g, n_t), held / nheld.max(1.0));
        }
    }
}
