//! Export a model's DRAWN geometry as OBJ, optionally posed, so two builds can be compared visually
//! under identical conditions.
//!
//! In-game screenshots are not a controlled comparison: the camera moves, so a shot from above and
//! behind cannot be judged against a front-on one, and a change can look like a regression purely
//! because the head turned. This writes the same view every time, from the shipped bytes.
//!
//! "Drawn" matters. Injection neutralises the groups it does not fill by zeroing PRMT draw counts,
//! but leaves their vertex and index buffers in place, so a naive export shows the donor's leftover
//! head and hands sitting inside the import — geometry the engine never rasterises.
//!
//! `--pose N` applies the same fixed-seed per-bone rotation set the self-test uses. Weights only
//! affect the surface once bones move, so an unposed export cannot show a skinning defect at all.
//!
//!   pose_export <block.bin> <out.obj> [--pose N] [--amp DEG]

use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::model_inject::group_draw_report;
use mercs2_formats::skeleton::Skeleton;
use std::io::Write;

fn flag<'a>(a: &'a [String], name: &str) -> Option<&'a str> {
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1)).map(|s| s.as_str())
}

struct Rng(u64);
impl Rng {
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (((self.0 >> 11) as f64) / ((1u64 << 53) as f64)) * 2.0 - 1.0
    }
}

fn rot_about(v: [f64; 3], o: [f64; 3], ax: [f64; 3], ang: f64) -> [f64; 3] {
    let p = [v[0] - o[0], v[1] - o[1], v[2] - o[2]];
    let (s, c) = ang.sin_cos();
    let d = ax[0] * p[0] + ax[1] * p[1] + ax[2] * p[2];
    let cr = [
        ax[1] * p[2] - ax[2] * p[1],
        ax[2] * p[0] - ax[0] * p[2],
        ax[0] * p[1] - ax[1] * p[0],
    ];
    std::array::from_fn(|i| p[i] * c + cr[i] * s + ax[i] * d * (1.0 - c) + o[i])
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: pose_export <block.bin> <out.obj> [--pose N] [--amp DEG]");
        std::process::exit(2);
    }
    let pose: i64 = flag(&a, "--pose").and_then(|s| s.parse().ok()).unwrap_or(-1);
    let amp: f64 = flag(&a, "--amp").and_then(|s| s.parse().ok()).unwrap_or(25.0);

    let block = std::fs::read(&a[1]).expect("read");
    let payload: &[u8] = if block.len() > 4 && &block[0..4] == b"UCFX" {
        &block
    } else {
        let n = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
        &block[20..20 + n]
    };
    let skel = Skeleton::from_block(&block).expect("skeleton");
    let meshes = read_model_meshes(payload).expect("meshes");
    let report = group_draw_report(payload).expect("draw report");
    let drawn: std::collections::HashSet<usize> =
        report.iter().filter(|r| r.1 > 0 && r.2 > 0).map(|r| r.0).collect();

    // Per-bone rotation, identical to the self-test's construction so the two agree.
    let nb = skel.bones.len();
    let (mut axis, mut ang) = (vec![[0.0f64; 3]; nb], vec![0.0f64; nb]);
    if pose >= 0 {
        let mut rng = Rng(0x5EED_0000 + pose as u64);
        for b in 0..nb {
            let mut v = [rng.unit(), rng.unit(), rng.unit()];
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
            for i in 0..3 {
                v[i] /= l;
            }
            axis[b] = v;
            ang[b] = rng.unit() * amp.to_radians();
        }
    }
    let origin: Vec<[f64; 3]> = skel
        .bones
        .iter()
        .map(|b| {
            let p = b.bind_pos();
            [p[0] as f64, p[1] as f64, p[2] as f64]
        })
        .collect();

    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2]).expect("create"));
    let mut base = 1u32;
    let (mut nv, mut nf, mut ng) = (0usize, 0usize, 0usize);
    for m in &meshes {
        if !drawn.contains(&m.group_index) || m.tris.is_empty() {
            continue;
        }
        ng += 1;
        for i in 0..m.positions.len() {
            let p = m.positions[i];
            let mut v = [p[0] as f64, p[1] as f64, p[2] as f64];
            if pose >= 0 && !m.joints.is_empty() && !m.weights.is_empty() {
                let tot: f64 = (0..4).map(|c| m.weights[i][c] as f64).sum();
                if tot > 0.0 {
                    let mut acc = [0.0f64; 3];
                    for c in 0..4 {
                        let w = m.weights[i][c] as f64 / tot;
                        if w <= 0.0 {
                            continue;
                        }
                        let bi = m.joints[i][c] as usize;
                        if bi >= nb {
                            continue;
                        }
                        let q = rot_about(v, origin[bi], axis[bi], ang[bi]);
                        for k in 0..3 {
                            acc[k] += w * q[k];
                        }
                    }
                    v = acc;
                }
            }
            writeln!(out, "v {:.6} {:.6} {:.6}", v[0], v[1], v[2]).unwrap();
            nv += 1;
        }
        for t in &m.tris {
            writeln!(out, "f {} {} {}", base + t[0], base + t[1], base + t[2]).unwrap();
            nf += 1;
        }
        base += m.positions.len() as u32;
    }
    println!("{} -> {}: {ng} drawn groups, {nv} verts, {nf} tris, pose {pose}", a[1], a[2]);
}
