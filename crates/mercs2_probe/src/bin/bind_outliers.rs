//! Find vertices bound to bones that are FAR from them, and say where they are.
//!
//! Region-by-region comparison needs you to guess the region, and a limb chain defeats the guess: a
//! sphere sized to the parent joint swallows the torso, and every table then reads "the spine
//! dominates". This asks the question globally instead - for every vertex, how far is the bone that
//! drives it? - and compares the import's distribution against the donor's, which is the
//! specification. Outliers are where skinning will tear.
//!
//!   bind_outliers <donor.block> <injected.block> [--top N]
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;

fn dominant_dist(block: &[u8], bp: &[[f32; 3]]) -> Vec<([f32; 3], f32, u32)> {
    let n = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + n]).expect("meshes");
    let mut out = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() || m.tris.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let mut best = (0u8, u32::MAX);
            for c in 0..4 {
                if m.weights[i][c] > best.0 {
                    best = (m.weights[i][c], m.joints[i][c] as u32);
                }
            }
            let Some(b) = bp.get(best.1 as usize) else { continue };
            if best.0 == 0 {
                continue;
            }
            let p = m.positions[i];
            let d = ((p[0] - b[0]).powi(2) + (p[1] - b[1]).powi(2) + (p[2] - b[2]).powi(2)).sqrt();
            out.push((p, d, best.1));
        }
    }
    out
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let top: usize = a.iter().position(|x| x == "--top").and_then(|i| a.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or(14);
    let donor = std::fs::read(&a[1]).expect("donor");
    let inj = std::fs::read(&a[2]).expect("inject");
    let skel = Skeleton::from_block(&donor).expect("skel");
    let bp: Vec<[f32; 3]> = skel.bones.iter().map(|b| b.bind_pos()).collect();

    let d = dominant_dist(&donor, &bp);
    let j = dominant_dist(&inj, &bp);
    let pct = |v: &mut Vec<f32>, q: f32| {
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        v[(((v.len() - 1) as f32) * q) as usize]
    };
    let mut dd: Vec<f32> = d.iter().map(|x| x.1).collect();
    let mut jj: Vec<f32> = j.iter().map(|x| x.1).collect();
    println!("distance from a vertex to its DOMINANT bone (metres):");
    println!("  donor   n={:<6} p50 {:.3}  p90 {:.3}  p99 {:.3}  max {:.3}", dd.len(), pct(&mut dd, 0.5), pct(&mut dd, 0.9), pct(&mut dd, 0.99), pct(&mut dd, 1.0));
    println!("  inject  n={:<6} p50 {:.3}  p90 {:.3}  p99 {:.3}  max {:.3}", jj.len(), pct(&mut jj, 0.5), pct(&mut jj, 0.9), pct(&mut jj, 0.99), pct(&mut jj, 1.0));

    // Anything past the donor's own p99 is further from its driver than retail ever puts a vertex.
    let thr = pct(&mut dd, 0.99);
    let mut bad: Vec<&([f32; 3], f32, u32)> = j.iter().filter(|x| x.1 > thr).collect();
    bad.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    println!("\n  {} of {} import verts exceed the donor p99 of {thr:.3} m ({:.1}%)", bad.len(), j.len(), 100.0 * bad.len() as f64 / j.len() as f64);
    let mut by_bone: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for b in &bad {
        *by_bone.entry(b.2).or_insert(0) += 1;
    }
    let mut bb: Vec<(u32, usize)> = by_bone.into_iter().collect();
    bb.sort_by_key(|x| std::cmp::Reverse(x.1));
    // WHERE the geometry sits relative to each bone. A bone whose dominated vertices sit further
    // from it in the import than in the donor means the conform placed that limb segment's geometry
    // off the joint - the crease then forms at the wrong point along the limb and the bend reads as
    // a broken bone, even though every weight matches retail.
    let centroid = |v: &[([f32; 3], f32, u32)]| {
        let mut acc: std::collections::HashMap<u32, ([f64; 3], usize)> = std::collections::HashMap::new();
        for (p, _, b) in v {
            let e = acc.entry(*b).or_insert(([0.0; 3], 0));
            for k in 0..3 {
                e.0[k] += p[k] as f64;
            }
            e.1 += 1;
        }
        acc
    };
    let (cd, cj) = (centroid(&d), centroid(&j));
    let mut rows: Vec<(f64, u32, f64, f64, f64, f64, usize, usize)> = Vec::new();
    for (b, (s0, n0)) in cd.iter() {
        let Some((s1, n1)) = cj.get(b) else { continue };
        if *n0 < 30 || *n1 < 30 {
            continue;
        }
        let Some(bpos) = bp.get(*b as usize) else { continue };
        let dist = |s: &[f64; 3], n: usize| {
            let c = [s[0] / n as f64, s[1] / n as f64, s[2] / n as f64];
            ((c[0] - bpos[0] as f64).powi(2) + (c[1] - bpos[1] as f64).powi(2) + (c[2] - bpos[2] as f64).powi(2)).sqrt()
        };
        // Split the offset into ALONG-BONE and RADIAL. Raw distance conflates two very different
        // things: a thicker limb legitimately pushes its surface centroid further from the bone
        // (radial), while a crease forming at the wrong point along the limb is a real defect
        // (along). 50 Cent's arms are far bulkier than the donor's, so the radial term is expected
        // to grow and says nothing - only the along-bone term is evidence.
        let axis = match skel.bones[*b as usize].parent {
            par if par >= 0 => {
                let q = bp[par as usize];
                let v = [bpos[0] - q[0], bpos[1] - q[1], bpos[2] - q[2]];
                let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if l > 1e-6 { [v[0] / l, v[1] / l, v[2] / l] } else { [0.0, 1.0, 0.0] }
            }
            _ => [0.0, 1.0, 0.0],
        };
        let split = |s: &[f64; 3], n: usize| -> (f64, f64) {
            let c = [s[0] / n as f64, s[1] / n as f64, s[2] / n as f64];
            let r = [c[0] - bpos[0] as f64, c[1] - bpos[1] as f64, c[2] - bpos[2] as f64];
            let along = r[0] * axis[0] as f64 + r[1] * axis[1] as f64 + r[2] * axis[2] as f64;
            let rad = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] - along * along).max(0.0).sqrt();
            (along, rad)
        };
        let (al0, rd0) = split(s0, *n0);
        let (al1, rd1) = split(s1, *n1);
        let _ = (dist(s0, *n0), dist(s1, *n1));
        rows.push(((al1 - al0).abs(), *b, al0, al1, rd0, rd1, *n0, *n1));
    }
    rows.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
    println!("
  dominated-vertex centroid offset from its bone, split (donor vs import):");
    println!("    bone  ALONG d/i        delta | radial d/i      (radial = limb thickness, expected to differ)");
    for (delta, b, al0, al1, rd0, rd1, _n0, _n1) in rows.iter().take(top) {
        println!("    {b:>4}  {al0:>6.3} {al1:>6.3}  {delta:6.3} | {rd0:>6.3} {rd1:>6.3}");
    }

    println!("  worst driving bones:");
    for (b, n) in bb.iter().take(top) {
        let p = bp.get(*b as usize).copied().unwrap_or([0.0; 3]);
        println!("    bone {b:>3} at [{:>6.3},{:>6.3},{:>6.3}]  drives {n} far verts", p[0], p[1], p[2]);
    }
}
