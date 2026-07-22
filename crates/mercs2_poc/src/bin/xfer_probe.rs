//! Precondition check for weight TRANSFER: do the shipped donor mesh and the conformed import
//! actually occupy the same space?
//!
//! The plan is to copy skin weights from the SHIPPED Mattias mesh (which measures 82.8%
//! multi-influence, i.e. retail quality) onto the conformed 50 Cent mesh, rather than inherit
//! 50 Cent's own weights (which collapse to 85.4% rigid because they reference muscle/twist/face
//! joints Mattias does not have). Both meshes are supposed to share a skeleton AND a bind pose,
//! because that is what `char_skin` establishes.
//!
//! "Supposed to" is the problem. Our own notes record spatial nearest-vertex weight transfer as a
//! TRAP — but that was between models that did NOT share a bind pose (CesiumMan Z-up vs Mattias
//! Y-up). Shared bind pose is the precondition that makes the technique valid, so it has to be
//! MEASURED, not assumed: a spatial method whose inputs are offset produces confident garbage.
//!
//!   xfer_probe <model.glb> <donor.block>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::{build_character, TargetSkeleton};
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn bbox(pts: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut mn = [f64::MAX; 3];
    let mut mx = [f64::MIN; 3];
    for p in pts {
        for c in 0..3 {
            mn[c] = mn[c].min(p[c]);
            mx[c] = mx[c].max(p[c]);
        }
    }
    (mn, mx)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: xfer_probe <model.glb> <donor.block>");
        std::process::exit(2);
    }
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor_block = std::fs::read(&a[2]).expect("donor");
    let skel = Skeleton::from_block(&donor_block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);

    // TARGET of the transfer: the conformed import.
    let cs = build_character(&glb.build_input(&target, None, HashMap::new(), false)).expect("build");
    let tgt_pts: Vec<[f64; 3]> = cs.posed.clone();

    // SOURCE of the transfer: every skinned vertex of the shipped donor, across ALL its groups —
    // group 3 alone is only the torso region, and a partial source surface would leave limbs with
    // nothing to sample from.
    let ucfx_len = u32::from_le_bytes(donor_block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&donor_block[20..20 + ucfx_len]).expect("read donor meshes");
    let mut src_pts: Vec<[f64; 3]> = Vec::new();
    let mut src_groups = 0usize;
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        src_groups += 1;
        for p in &m.positions {
            src_pts.push([p[0] as f64, p[1] as f64, p[2] as f64]);
        }
    }

    let (smn, smx) = bbox(&src_pts);
    let (tmn, tmx) = bbox(&tgt_pts);
    println!("SOURCE  shipped donor : {} verts over {} skinned groups", src_pts.len(), src_groups);
    println!("        bbox min [{:.3},{:.3},{:.3}] max [{:.3},{:.3},{:.3}]", smn[0], smn[1], smn[2], smx[0], smx[1], smx[2]);
    println!("TARGET  conformed import: {} verts", tgt_pts.len());
    println!("        bbox min [{:.3},{:.3},{:.3}] max [{:.3},{:.3},{:.3}]", tmn[0], tmn[1], tmn[2], tmx[0], tmx[1], tmx[2]);

    let overlap: f64 = (0..3)
        .map(|c| {
            let lo = smn[c].max(tmn[c]);
            let hi = smx[c].min(tmx[c]);
            let inter = (hi - lo).max(0.0);
            let uni = smx[c].max(tmx[c]) - smn[c].min(tmn[c]);
            if uni > 0.0 { inter / uni } else { 0.0 }
        })
        .product::<f64>();
    println!("\nbbox overlap (product of per-axis IoU-ish): {:.3}", overlap);

    // The real question is not bounding boxes but SURFACE proximity: for each target vertex, how
    // far is the nearest source vertex? If that distance is small relative to body size, a nearest-
    // surface weight lookup is meaningful. If it is large, the transfer would sample the wrong body
    // part and produce exactly the anatomically-scrambled weights our notes warn about.
    let height = (tmx[1] - tmn[1]).max(1e-6);
    let mut d: Vec<f64> = Vec::with_capacity(tgt_pts.len());
    for t in &tgt_pts {
        let mut best = f64::MAX;
        for s in &src_pts {
            let dd = (t[0] - s[0]).powi(2) + (t[1] - s[1]).powi(2) + (t[2] - s[2]).powi(2);
            if dd < best {
                best = dd;
            }
        }
        d.push(best.sqrt());
    }
    d.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let q = |p: f64| d[((d.len() as f64 * p) as usize).min(d.len() - 1)];
    println!("target->nearest source vertex distance, as % of body height ({:.3} m):", height);
    println!("   median {:.1}%   p90 {:.1}%   p99 {:.1}%   max {:.1}%",
        100.0 * q(0.5) / height, 100.0 * q(0.9) / height, 100.0 * q(0.99) / height, 100.0 * d[d.len() - 1] / height);
    let far = d.iter().filter(|&&x| x > 0.05 * height).count();
    println!("   vertices further than 5% of body height from ANY source vertex: {} ({:.1}%)",
        far, 100.0 * far as f64 / d.len() as f64);
    println!("\nA transfer is only meaningful where the surfaces are close. Far vertices are the ones");
    println!("that need inpainting rather than a nearest-neighbour answer.");
}
