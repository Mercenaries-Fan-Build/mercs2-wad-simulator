//! Throwaway measurement: reproduce xfer_apply's conform and measure, directly from geometry,
//!   (1) whether the source->container linear map is a SIMILARITY or is anisotropic (shear), and
//!   (2) whether the CONFORMED forearm has a TWIST (roll) that the source limb does not — a roll
//!       that varies with arc-position along the arm axis is a corkscrew (candy-wrapper) artifact.
//!
//! Reference frames:
//!   cp[vi]    = source vertex mapped by the global container transform, BEFORE per-bone re-pose
//!   posed[vi] = conformed vertex (after per-bone similarity LBS)
//! A similarity(cp) preserves the clean source shape, so any roll gradient in posed-vs-cp is
//! introduced by the per-bone re-pose / LBS blend.
//!
//!   armtwist <model.glb> <donor.block> <elbowBone> <wristBone>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::mat::{cross, dot, len, norm, sub};
use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let elbow: u32 = a[3].parse().unwrap();
    let wrist: u32 = a[4].parse().unwrap();
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor_block = std::fs::read(&a[2]).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");
    let bp: Vec<[f64; 3]> = skel.bones.iter().map(|b| { let p = b.bind_pos(); [p[0] as f64, p[1] as f64, p[2] as f64] }).collect();

    // ---- (1) container-map anisotropy: is source->cp a similarity? ----
    // For a similarity, |cp(a)-cp(b)| / |a-b| is CONSTANT (= scale) over every edge. Variance = shear.
    let mut ratios = Vec::new();
    let n = glb.positions.len();
    let step = (n / 4000).max(1);
    let mut i = 0;
    while i + step < n {
        let da = sub(glb.positions[i], glb.positions[i + step]);
        let dl = len(da);
        if dl > 1e-4 {
            let ca = sub(cs.cp[i], cs.cp[i + step]);
            ratios.push(len(ca) / dl);
        }
        i += step;
    }
    let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let var: f64 = ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    let (mn, mx) = ratios.iter().fold((f64::MAX, f64::MIN), |(a, b), &r| (a.min(r), b.max(r)));
    println!("container map length-ratio: mean {mean:.4}  std {:.4} ({:.1}%)  min {mn:.4} max {mx:.4}  max/min {:.3}",
        var.sqrt(), 100.0 * var.sqrt() / mean, mx / mn);
    println!("  (a pure similarity => std ~0, max/min ~1.0; large spread => anisotropic shear baked into every limb)\n");

    // ---- (1b) HANDEDNESS: does source->cp flip chirality? A reflection preserves lengths (so the
    // ratio test above passes) but Kabsch can only emit a proper rotation, absorbing the flip as a
    // systematic skew on chiral parts (hands, forearm). Compare signed tetra volumes source vs cp.
    let mut agree = 0i64;
    let mut disagree = 0i64;
    let tri = |p: [f64; 3], q: [f64; 3], r: [f64; 3], s: [f64; 3]| dot(sub(q, p), cross(sub(r, p), sub(s, p)));
    let m = glb.positions.len();
    let st = (m / 3000).max(1);
    let mut j = 0;
    while j + 3 * st < m {
        let vs = tri(glb.positions[j], glb.positions[j + st], glb.positions[j + 2 * st], glb.positions[j + 3 * st]);
        let vc = tri(cs.cp[j], cs.cp[j + st], cs.cp[j + 2 * st], cs.cp[j + 3 * st]);
        if vs.abs() > 1e-9 && vc.abs() > 1e-9 {
            if vs.signum() == vc.signum() { agree += 1; } else { disagree += 1; }
        }
        j += st;
    }
    println!("handedness (signed-volume sign, source vs cp): agree {agree}  disagree {disagree}");
    println!("  (all-agree => proper map; all-disagree => a REFLECTION Kabsch cannot represent)\n");

    // ---- (2) forearm twist: roll about the arm axis vs arc position ----
    // dominant mapped hier per vertex (through the SOURCE weights used by the conform)
    let dom = |vi: usize| -> Option<u32> {
        let mut best = (-1.0f64, u32::MAX);
        for k in 0..4 {
            let w = glb.vweights[vi][k];
            if w > best.0 {
                if let Some(&h) = cs.full.get(&(glb.vjoints[vi][k] as usize)) {
                    best = (w, h);
                }
            }
        }
        if best.1 == u32::MAX { None } else { Some(best.1) }
    };

    // forearm axis in TARGET space, elbow -> wrist
    let axp = bp[elbow as usize];
    let axis = norm(sub(bp[wrist as usize], bp[elbow as usize]));
    let seg = len(sub(bp[wrist as usize], bp[elbow as usize]));
    // build a fixed perpendicular frame (u,v) about the axis
    let seed = if axis[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = norm(cross(axis, seed));
    let v = cross(axis, u);

    // roll of a point about the axis in the (u,v) frame
    let roll = |p: [f64; 3]| -> f64 {
        let r = sub(p, axp);
        let ru = dot(r, u);
        let rv = dot(r, v);
        rv.atan2(ru)
    };
    let arc = |p: [f64; 3]| -> f64 { dot(sub(p, axp), axis) / seg };

    const NB: usize = 8;
    let mut sum = vec![0.0f64; NB];
    let mut cnt = vec![0.0f64; NB];
    let mut sabs = vec![0.0f64; NB];
    for vi in 0..n {
        let Some(h) = dom(vi) else { continue };
        if h != elbow && h != wrist && !(h == elbow + 1) {
            continue; // forearm, forearmroll, hand-side
        }
        let t = arc(cs.posed[vi]);
        if !(0.0..=1.0).contains(&t) { continue; }
        // roll delta: conformed vs source-in-container (both about the same target axis)
        let mut d = roll(cs.posed[vi]) - roll(cs.cp[vi]);
        while d > std::f64::consts::PI { d -= 2.0 * std::f64::consts::PI; }
        while d < -std::f64::consts::PI { d += 2.0 * std::f64::consts::PI; }
        let bin = ((t * NB as f64).floor() as usize).min(NB - 1);
        sum[bin] += d;
        sabs[bin] += d.abs();
        cnt[bin] += 1.0;
    }
    println!("forearm axis {elbow}(elbow)->{wrist}(wrist), len {seg:.3} m, roll(posed)-roll(cp) vs arc:");
    println!("  arc-bin   n     mean_dRoll_deg   mean|dRoll|_deg");
    for b in 0..NB {
        if cnt[b] < 1.0 { continue; }
        println!("   {:.2}-{:.2}  {:>5}    {:+8.2}        {:8.2}",
            b as f64 / NB as f64, (b + 1) as f64 / NB as f64, cnt[b] as usize,
            (sum[b] / cnt[b]).to_degrees(), (sabs[b] / cnt[b]).to_degrees());
    }
    // a twist GRADIENT = mean_dRoll changes across arc bins; report the spread
    let means: Vec<f64> = (0..NB).filter(|&b| cnt[b] > 0.0).map(|b| (sum[b] / cnt[b]).to_degrees()).collect();
    let lo = means.iter().cloned().fold(f64::MAX, f64::min);
    let hi = means.iter().cloned().fold(f64::MIN, f64::max);
    println!("\n  twist GRADIENT elbow->wrist: {:.1} deg (spread of per-bin mean roll)", hi - lo);
}
