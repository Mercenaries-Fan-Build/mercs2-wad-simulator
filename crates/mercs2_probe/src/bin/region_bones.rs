//! Compare which bones drive a body REGION, between a shipped model and an injected one.
//!
//! Why this exists. A skin can be right everywhere on average and wrong in one place, and the
//! averages will not say so. A face is ~5% of a character's vertices, so a face bound to the wrong
//! bones moves the whole-model influence census by almost nothing while being the single most
//! obvious defect in the game — the bind pose looks correct because at bind every bone transform is
//! the identity, and the head only comes apart once an animation drives the jaw and neck.
//!
//! So compare like for like: take the vertices above the neck joint in each model and report where
//! their weight MASS goes, per bone. The shipped model is the specification. Any bone carrying real
//! mass in the import but not in the donor is a bone that will move the import's face in a way
//! retail never moves its own.
//!
//!   region_bones <donor.block> <injected.block> [--bone N] [--above] [--top N]
//!
//! `--bone` selects the region's reference joint (default 20, the neck; 21 is the head). `--above`
//! takes every vertex above it in Y, which is what isolates a head; without it the region is a
//! sphere around the joint sized to the distance to its parent.

use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

/// Region vertices with their influences: (position, [(bone, weight)]).
fn region_verts(
    block: &[u8],
    y_min: f32,
    centre: [f32; 3],
    radius: f32,
    above: bool,
) -> Vec<([f64; 3], Vec<(u32, f64)>)> {
    let ucfx_len = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + ucfx_len]).expect("meshes");
    let mut out = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let p = m.positions[i];
            let inside = if above {
                p[1] >= y_min
            } else {
                let d = (p[0] - centre[0]).powi(2)
                    + (p[1] - centre[1]).powi(2)
                    + (p[2] - centre[2]).powi(2);
                d.sqrt() <= radius
            };
            if !inside {
                continue;
            }
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            let infl = (0..4)
                .filter(|&c| m.weights[i][c] > 0)
                .map(|c| (m.joints[i][c] as u32, m.weights[i][c] as f64 / tot))
                .collect();
            out.push(([p[0] as f64, p[1] as f64, p[2] as f64], infl));
        }
    }
    out
}

/// Rotate `v` about `ax` through `origin` by `angle`.
fn rot_about(v: [f64; 3], origin: [f64; 3], ax: [f64; 3], angle: f64) -> [f64; 3] {
    let p = [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];
    let (s, c) = angle.sin_cos();
    let dot = ax[0] * p[0] + ax[1] * p[1] + ax[2] * p[2];
    let cr = [
        ax[1] * p[2] - ax[2] * p[1],
        ax[2] * p[0] - ax[0] * p[2],
        ax[0] * p[1] - ax[1] * p[0],
    ];
    std::array::from_fn(|i| p[i] * c + cr[i] * s + ax[i] * dot * (1.0 - c) + origin[i])
}

/// Mean/max displacement of `verts` when bone `b` alone rotates, plus the weight mass it carries.
///
/// Only bone `b` moves, so under linear blending each vertex is displaced by its weight on `b`
/// times that bone's own motion — no need to build the full pose. Dividing by the carried mass
/// makes donor and import comparable even when one binds the region slightly more strongly.
fn deform(
    verts: &[([f64; 3], Vec<(u32, f64)>)],
    b: u32,
    origin: [f64; 3],
    ax: [f64; 3],
    angle: f64,
) -> (f64, f64, f64) {
    let mut sum = 0.0;
    let mut max = 0.0f64;
    let mut mass = 0.0;
    for (p, infl) in verts {
        let w: f64 = infl.iter().filter(|(j, _)| *j == b).map(|(_, w)| *w).sum();
        if w <= 0.0 {
            continue;
        }
        mass += w;
        let q = rot_about(*p, origin, ax, angle);
        let d = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2) + (q[2] - p[2]).powi(2)).sqrt() * w;
        sum += d;
        max = max.max(d);
    }
    (if mass > 0.0 { sum / mass } else { 0.0 }, max, mass)
}

/// (bone -> weight mass, vertex count, total mass)
fn region_mass(
    block: &[u8],
    y_min: f32,
    centre: [f32; 3],
    radius: f32,
    above: bool,
) -> (HashMap<u32, f64>, usize, f64) {
    let ucfx_len = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + ucfx_len]).expect("meshes");
    let mut mass: HashMap<u32, f64> = HashMap::new();
    let mut n = 0usize;
    let mut total = 0.0;
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let p = m.positions[i];
            let inside = if above {
                p[1] >= y_min
            } else {
                let d = (p[0] - centre[0]).powi(2) + (p[1] - centre[1]).powi(2) + (p[2] - centre[2]).powi(2);
                d.sqrt() <= radius
            };
            if !inside {
                continue;
            }
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            n += 1;
            for c in 0..4 {
                let w = m.weights[i][c] as f64;
                if w > 0.0 {
                    *mass.entry(m.joints[i][c] as u32).or_insert(0.0) += w / tot;
                    total += w / tot;
                }
            }
        }
    }
    (mass, n, total)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: region_bones <donor.block> <injected.block> [--bone N] [--above] [--top N]");
        std::process::exit(2);
    }
    let bone: usize = flag(&a, "--bone").and_then(|s| s.parse().ok()).unwrap_or(20);
    let top: usize = flag(&a, "--top").and_then(|s| s.parse().ok()).unwrap_or(14);
    let above = a.iter().any(|x| x == "--above");

    let donor = std::fs::read(&a[1]).expect("donor");
    let inj = std::fs::read(&a[2]).expect("injected");
    let skel = Skeleton::from_block(&donor).expect("skeleton");
    let b = &skel.bones[bone];
    let c = b.bind_pos();
    // 2.5x the distance to the parent is a reasonable default for a compact joint, but it is far
    // too wide for a long bone: on an elbow whose parent is a shoulder it produces a 0.7 m sphere
    // that swallows the whole torso, and every table then reads as "bone 26 dominates". Override it
    // when isolating a limb.
    let radius = flag(&a, "--radius").and_then(|s| s.parse().ok()).unwrap_or_else(|| {
        if b.parent >= 0 {
            let p = skel.bones[b.parent as usize].bind_pos();
            ((c[0] - p[0]).powi(2) + (c[1] - p[1]).powi(2) + (c[2] - p[2]).powi(2)).sqrt() * 2.5
        } else {
            0.2
        }
    });
    println!(
        "region: bone {bone} at [{:.3}, {:.3}, {:.3}], {}",
        c[0], c[1], c[2],
        if above { format!("everything above y={:.3}", c[1]) } else { format!("sphere r={radius:.3}") }
    );

    let (md, nd, td) = region_mass(&donor, c[1], c, radius, above);
    let (mi, ni, ti) = region_mass(&inj, c[1], c, radius, above);
    println!("  donor    : {nd} verts, {} bones", md.len());
    println!("  injected : {ni} verts, {} bones", mi.len());

    // Union of bones, ranked by how much the two disagree.
    let mut all: Vec<u32> = md.keys().chain(mi.keys()).copied().collect();
    all.sort_unstable();
    all.dedup();
    let mut rows: Vec<(f64, u32, f64, f64)> = all
        .iter()
        .map(|&b| {
            let d = md.get(&b).copied().unwrap_or(0.0) / td.max(1e-9) * 100.0;
            let i = mi.get(&b).copied().unwrap_or(0.0) / ti.max(1e-9) * 100.0;
            ((i - d).abs(), b, d, i)
        })
        .collect();
    rows.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());

    println!("\n  bone   donor%   inject%   delta   note");
    for (delta, b, d, i) in rows.iter().take(top) {
        // A bone the donor does not use in this region is the actionable case: retail never moves
        // this part of the body with that bone, so anything the import binds there is invented.
        let note = if *d < 0.5 && *i >= 1.0 {
            "<-- NOT USED BY RETAIL HERE"
        } else if *d >= 1.0 && *i < 0.5 {
            "(retail uses, import dropped)"
        } else {
            ""
        };
        println!("  {b:>4}  {d:7.2}  {i:8.2}  {delta:6.2}   {note}");
    }

    let invented: f64 = rows.iter().filter(|(_, _, d, i)| *d < 0.5 && *i >= 0.5).map(|(_, _, _, i)| i).sum();
    println!("\n  weight mass on bones retail does NOT use in this region: {invented:.1}%");

    if a.iter().any(|x| x == "--pose") {
        let vd = region_verts(&donor, c[1], c, radius, above);
        let vi = region_verts(&inj, c[1], c, radius, above);
        // 20 degrees about X: a nod or jaw-open for a head bone, a plausible bend anywhere else.
        let ang = 20.0f64.to_radians();
        println!("\n  per-bone DEFORMATION of this region, 20 deg about X (metres):");
        println!("  bone   donor mean   inject mean    ratio   donor max   inject max");
        let mut worst: Vec<(f64, u32, f64, f64, f64, f64)> = Vec::new();
        for &b in all.iter() {
            let bi = b as usize;
            if bi >= skel.bones.len() {
                continue;
            }
            let o = skel.bones[bi].bind_pos();
            let od = [o[0] as f64, o[1] as f64, o[2] as f64];
            let (dm, dx, dmass) = deform(&vd, b, od, [1.0, 0.0, 0.0], ang);
            let (im, ix, imass) = deform(&vi, b, od, [1.0, 0.0, 0.0], ang);
            // Ignore bones with negligible presence in both: a huge ratio carried by 0.01% of the
            // weight mass is noise, not a defect.
            if dmass < 1.0 && imass < 1.0 {
                continue;
            }
            let ratio = if dm > 1e-9 { im / dm } else { f64::INFINITY };
            worst.push((ratio, b, dm, im, dx, ix));
        }
        worst.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));
        for (ratio, b, dm, im, dx, ix) in worst.iter().take(top) {
            println!("  {b:>4}  {dm:10.4}  {im:12.4}  {ratio:7.2}x  {dx:10.4}  {ix:11.4}");
        }
    }
}
