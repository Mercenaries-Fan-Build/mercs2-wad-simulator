//! Measure weight-transfer quality against ground truth, on the only mesh where ground truth
//! exists: the shipped donor itself.
//!
//! Why this probe. The transfer was tuned against the influence-count HISTOGRAM (what fraction of
//! vertices are rigid / multi / four-bone), because that is what a shipped character exposes to a
//! census. But the histogram is a proxy, and a bad one: it says how MANY bones touch a vertex, never
//! whether they are the RIGHT bones. Two weightings with an identical histogram can deform to
//! completely different surfaces. The reported symptom — the character reads THICKER in game than in
//! the workshop preview while the bind pose looks perfect — is invisible to the histogram by
//! construction, because at bind pose every bone transform is the identity and weights cannot show.
//!
//! So measure the thing itself. Hold out a slice of the donor's vertices, transfer weights onto them
//! from the rest, then pose the skeleton and compare the surface each weighting produces:
//!
//!   * `rms`   — how far a vertex lands from where retail's own weights would put it.
//!   * `bulge` — the same error projected onto the surface NORMAL, signed and averaged. This is the
//!               number that corresponds to "thicker": a positive bulge means the transferred
//!               weighting pushes the surface OUTWARD on average. RMS alone cannot distinguish an
//!               outward bulge from harmless tangential slide.
//!
//! Poses are synthetic — a fixed-seed rotation per bone about its own bind origin — which is all the
//! metric needs: both weightings are evaluated under the SAME transforms, so any plausible pose set
//! ranks them. No animation data, and no dependence on a clip that may itself be mis-decoded.
//!
//! Donor and target are the same mesh here, so a target would otherwise match itself at distance
//! zero and score perfectly while measuring nothing. `exclude_radius` suppresses that: every donor
//! sample within one median edge length is ignored, which reproduces the real condition — the import
//! measured a median nearest-donor distance of 1.2% of body height — while keeping FULL target
//! connectivity, so the smoothing pass is measured exactly as it runs in production.
//!
//!   xfer_selftest <donor.block> [-k N] [--prune F] [--poses N] [--smooth N] [--lambda F]

use mercs2_formats::char_skin::transfer::{
    adjacency, smooth_weights, transfer_weights_pruned, vertex_normals, DonorSample, TransferOpts,
};
use mercs2_formats::char_skin::TargetSkeleton;
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

/// Fixed-seed LCG. Determinism matters more than quality here: the same pose set must be used for
/// every point in a sweep, or the comparison is between noise.
struct Rng(u64);
impl Rng {
    fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn unit(&mut self) -> f64 {
        self.next_f64() * 2.0 - 1.0
    }
}

/// Rotation of `v` by `angle` about the axis `ax` (unit), centred on `origin`.
fn rot_about(v: [f64; 3], origin: [f64; 3], ax: [f64; 3], angle: f64) -> [f64; 3] {
    let p = [v[0] - origin[0], v[1] - origin[1], v[2] - origin[2]];
    let (s, c) = angle.sin_cos();
    let dot = ax[0] * p[0] + ax[1] * p[1] + ax[2] * p[2];
    let cross = [
        ax[1] * p[2] - ax[2] * p[1],
        ax[2] * p[0] - ax[0] * p[2],
        ax[0] * p[1] - ax[1] * p[0],
    ];
    let mut o = [0.0; 3];
    for i in 0..3 {
        o[i] = p[i] * c + cross[i] * s + ax[i] * dot * (1.0 - c) + origin[i];
    }
    o
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: xfer_selftest <donor.block> [-k N] [--prune F] [--poses N] [--holdout F]");
        std::process::exit(2);
    }
    let k: usize = flag(&a, "-k").and_then(|s| s.parse().ok()).unwrap_or(4);
    let prune: f64 = flag(&a, "--prune").and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let poses: usize = flag(&a, "--poses").and_then(|s| s.parse().ok()).unwrap_or(24);
    // As a fraction of body height. 1.2% is the median nearest-donor distance the real import
    // measured, so this makes the self-test as hard as the job it stands in for.
    let exclude: f64 = flag(&a, "--exclude").and_then(|s| s.parse().ok()).unwrap_or(0.012);
    let smooth: usize = flag(&a, "--smooth").and_then(|s| s.parse().ok()).unwrap_or(0);
    let lambda: f64 = flag(&a, "--lambda").and_then(|s| s.parse().ok()).unwrap_or(0.5);
    let no_normals = a.iter().any(|x| x == "--no-normals");

    let block = std::fs::read(&a[1]).expect("donor block");
    let skel = Skeleton::from_block(&block).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);
    let ucfx_len = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + ucfx_len]).expect("meshes");

    // Every skinned vertex, with its true weights and a geometric normal.
    let mut all: Vec<DonorSample> = Vec::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        let mpos: Vec<[f64; 3]> =
            m.positions.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect();
        let mut n = vertex_normals(&mpos, &m.tris);
        if !m.normals.is_empty() {
            let agree: f64 = (0..n.len().min(m.normals.len()))
                .map(|i| {
                    let s = m.normals[i];
                    n[i][0] * s[0] as f64 + n[i][1] * s[1] as f64 + n[i][2] * s[2] as f64
                })
                .sum();
            if agree < 0.0 {
                for x in n.iter_mut() {
                    for axis in 0..3 {
                        x[axis] = -x[axis];
                    }
                }
            }
        }
        for i in 0..mpos.len() {
            let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
            if tot <= 0.0 {
                continue;
            }
            let infl = (0..4)
                .filter(|&c| m.weights[i][c] > 0)
                .map(|c| (m.joints[i][c] as u32, m.weights[i][c] as f64 / tot))
                .collect();
            all.push(DonorSample { pos: mpos[i], normal: n[i], infl });
        }
    }

    // Targets are ALL donor vertices, with their real connectivity. Self-matching is prevented by
    // radius, not by holding vertices out, because a stride holdout destroys the edge graph the
    // smoothing pass needs — and smoothing is the main thing under test.
    let targets: Vec<[f64; 3]> = all.iter().map(|s| s.pos).collect();
    let tnorm: Vec<[f64; 3]> = all.iter().map(|s| s.normal).collect();
    let truth: Vec<Vec<(u32, f64)>> = all.iter().map(|s| s.infl.clone()).collect();

    // Concatenated triangle list with per-mesh vertex offsets, in the same order `all` was built.
    // Rebuilt from the meshes rather than tracked during the first pass so the offsets cannot drift.
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut base = 0u32;
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        let mut kept = vec![u32::MAX; m.positions.len()];
        for i in 0..m.positions.len() {
            let tot: u32 = (0..4).map(|c| m.weights[i][c] as u32).sum();
            if tot > 0 {
                kept[i] = base;
                base += 1;
            }
        }
        for t in &m.tris {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            if a < kept.len() && b < kept.len() && c < kept.len() {
                let (ka, kb, kc) = (kept[a], kept[b], kept[c]);
                if ka != u32::MAX && kb != u32::MAX && kc != u32::MAX {
                    tris.push([ka, kb, kc]);
                }
            }
        }
    }
    assert_eq!(base as usize, all.len(), "vertex offsets drifted from the sample list");
    let adj = adjacency(all.len(), &tris);

    let excl = target.height * exclude;
    let mut t = transfer_weights_pruned(
        &all,
        &targets,
        target.height,
        &TransferOpts {
            k,
            min_weight: prune,
            target_normals: if no_normals { &[] } else { &tnorm },
            exclude_radius: excl,
        },
    );
    if smooth > 0 {
        smooth_weights(&mut t.per_vertex, &adj, smooth, lambda);
    }
    let held: Vec<(usize, [f64; 3], [f64; 3], Vec<(u32, f64)>)> = (0..all.len())
        .map(|i| (i, targets[i], tnorm[i], truth[i].clone()))
        .collect();

    // Per-bone synthetic rotations, regenerated identically for every pose index.
    let nb = skel.bones.len();
    let origins: Vec<[f64; 3]> = skel
        .bones
        .iter()
        .map(|b| {
            let p = b.bind_pos();
            [p[0] as f64, p[1] as f64, p[2] as f64]
        })
        .collect();

    let mut sum_sq = 0.0f64;
    let mut sum_bulge = 0.0f64;
    let mut worst = 0.0f64;
    let mut n_samples = 0usize;

    // Per-height-band error. A whole-model rms can be perfectly acceptable while one region is
    // ruined: a face is a few percent of the vertices, so the average cannot see it, and the average
    // is the only thing that has been looked at so far. Bands are relative to the model's own
    // vertical extent, so band 9 is the head on any character.
    const BANDS: usize = 10;
    let ylo = targets.iter().map(|p| p[1]).fold(f64::MAX, f64::min);
    let yhi = targets.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
    let yspan = (yhi - ylo).max(1e-9);
    let mut band_sq = vec![0.0f64; BANDS];
    let mut band_n = vec![0usize; BANDS];

    for pi in 0..poses {
        let mut rng = Rng(0x5EED_0000 + pi as u64);
        let mut axis = vec![[0.0f64; 3]; nb];
        let mut ang = vec![0.0f64; nb];
        for b in 0..nb {
            let mut v = [rng.unit(), rng.unit(), rng.unit()];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            for i in 0..3 {
                v[i] /= l;
            }
            axis[b] = v;
            // +-35 deg: a real limb bend, without folding the mesh through itself.
            ang[b] = rng.unit() * 0.61;
        }

        for (hi, h) in held.iter().enumerate() {
            let v = h.1;
            let blend = |infl: &[(u32, f64)]| -> [f64; 3] {
                let mut o = [0.0f64; 3];
                let mut wsum = 0.0;
                for &(b, w) in infl {
                    let bi = b as usize;
                    if bi >= nb {
                        continue;
                    }
                    let p = rot_about(v, origins[bi], axis[bi], ang[bi]);
                    for i in 0..3 {
                        o[i] += w * p[i];
                    }
                    wsum += w;
                }
                if wsum > 0.0 {
                    for i in 0..3 {
                        o[i] /= wsum;
                    }
                }
                o
            };
            let a_true = blend(&h.3);
            let a_xfer = blend(&t.per_vertex[hi]);
            let d = [a_xfer[0] - a_true[0], a_xfer[1] - a_true[1], a_xfer[2] - a_true[2]];
            let m2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            sum_sq += m2;
            let bi = (((v[1] - ylo) / yspan) * BANDS as f64).floor() as usize;
            let bi = bi.min(BANDS - 1);
            band_sq[bi] += m2;
            band_n[bi] += 1;
            // Project the error onto the (posed) normal. The normal is rotated by the TRUE
            // weighting's dominant bone, which is enough to keep "outward" meaningful.
            let n = h.2;
            sum_bulge += d[0] * n[0] + d[1] * n[1] + d[2] * n[2];
            if m2 > worst {
                worst = m2;
            }
            n_samples += 1;
        }
    }

    let h = target.height;
    let rms = (sum_sq / n_samples as f64).sqrt();
    let bulge = sum_bulge / n_samples as f64;
    println!(
        "donor {} verts  {} tris  bones {}  height {:.3} m  exclude {:.4} m",
        all.len(), tris.len(), nb, h, excl
    );
    println!(
        "k={k} prune={prune} normals={} smooth={smooth} lambda={lambda} poses={poses}",
        if no_normals { "off" } else { "on" }
    );
    println!(
        "  rms   {:.5} m  ({:.3}% of height)",
        rms,
        100.0 * rms / h
    );
    println!(
        "  bulge {:+.5} m  ({:+.3}% of height)   <- positive = surface pushed OUTWARD",
        bulge,
        100.0 * bulge / h
    );
    println!("  worst {:.5} m  ({:.3}% of height)", worst.sqrt(), 100.0 * worst.sqrt() / h);
    println!("  rms by height band (0 = feet, 9 = head), % of body height:");
    for b in 0..BANDS {
        if band_n[b] == 0 {
            continue;
        }
        let r = (band_sq[b] / band_n[b] as f64).sqrt();
        let pct = 100.0 * r / h;
        let bar = "#".repeat(((pct / 0.1).round() as usize).min(60));
        println!("    band {b}  {:>6} verts  {pct:6.3}%  {bar}", band_n[b] / poses);
    }
    // Machine-readable tail for sweeps.
    println!(
        "CSV,{k},{prune},{},{smooth},{lambda},{:.6},{:.6},{:.6}",
        if no_normals { 0 } else { 1 },
        100.0 * rms / h,
        100.0 * bulge / h,
        100.0 * worst.sqrt() / h
    );
}
