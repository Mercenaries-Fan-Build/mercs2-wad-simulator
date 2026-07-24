//! Measure a character's LIMB GIRTH along its own bone axis, so builds can be compared across models.
//!
//! "He looks too narrow" is a shape claim, and shape claims need a number. This walks a bone segment
//! (e.g. thigh -> calf) and reports the mean radial distance of nearby vertices from that axis, in
//! slabs. Because it uses each model's OWN skeleton to define the axis, and normalises nothing, the
//! numbers are directly comparable between characters that share the Pandemic rig.
//!
//! Usage: `limb_girth <block-or-ucfx> [<block2> ...]`  — prints a girth profile per model.
//!
//! ⚠ Only groups the engine actually DRAWS (`prmt_draw > 0`) are measured. An injected character
//! block still carries every one of the donor's groups; injection only zeroes the draw count on the
//! ones it displaces. Measuring all groups therefore averages the import together with the donor's
//! leftover, undrawn body — which read as an import whose thigh matched the donor's to 0.1 mm,
//! because it WAS the donor's thigh.

use mercs2_formats::char_skin::TargetSkeleton;
use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;

fn v(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn len(a: [f64; 3]) -> f64 { dot(a, a).sqrt() }
fn norm(a: [f64; 3]) -> [f64; 3] { let n = len(a).max(1e-12); [a[0] / n, a[1] / n, a[2] / n] }

/// Accept either a wrapped block (20-byte header + UCFX) or a raw dumped UCFX container.
fn ucfx_of(bytes: &[u8]) -> &[u8] {
    if bytes.len() > 4 && &bytes[0..4] == b"UCFX" {
        bytes
    } else {
        let n = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        &bytes[20..20 + n]
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: limb_girth <block-or-ucfx> [<block2> ...]");
        std::process::exit(2);
    }
    // (canonical bone at the top of the segment, canonical bone at the bottom, label)
    let segments = [(6u32, 7u32, "L thigh"), (10, 11, "R thigh"), (3, 6, "hips")];
    println!("mean radial distance (mm) of mesh from the bone axis, in slabs top->bottom\n");
    for path in &args {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => { println!("{path}: {e}"); continue; }
        };
        let ucfx = ucfx_of(&bytes);
        let meshes = match read_model_meshes(ucfx) { Ok(m) => m, Err(e) => { println!("{path}: {e}"); continue; } };
        // Skeleton::from_block wants the wrapped form; rebuild one if we were handed raw UCFX.
        let wrapped: Vec<u8> = if &bytes[0..4] == b"UCFX" {
            let mut w = vec![0u8; 20];
            w[16..20].copy_from_slice(&(ucfx.len() as u32).to_le_bytes());
            w.extend_from_slice(ucfx);
            w
        } else { bytes.clone() };
        let sk = match Skeleton::from_block(&wrapped) { Ok(s) => s, Err(e) => { println!("{path}: skeleton: {e}"); continue; } };
        let ts = TargetSkeleton::from_skeleton(&sk);
        // Carry each vertex's DOMINANT bone (global HIER, as `read_model_meshes` returns) alongside
        // its position. A pure radius window cannot measure a thigh: at any usable cutoff the slab
        // also catches the far leg and the pelvis, and the resulting median is a mixture, not a
        // surface. Measured symptom — the window read a 221-248 mm "thigh radius" (a 1.5 m
        // circumference) on one side while the other side read a plausible 118 mm, and the mixture
        // shifted whenever nearby geometry moved, so the number tracked the neighbourhood rather
        // than the limb. Skinning is the non-circular answer: a vertex weighted to the thigh IS
        // thigh flesh. Models with no skin data fall back to the window.
        let mut pts: Vec<([f64; 3], u32)> = Vec::new();
        for m in meshes.iter().filter(|m| m.prmt_draw > 0) {
            for (vi, p) in m.positions.iter().enumerate() {
                let dom = match (m.joints.get(vi), m.weights.get(vi)) {
                    (Some(j), Some(w)) => {
                        let k = (0..4).max_by_key(|&k| w[k]).unwrap_or(0);
                        if w[k] == 0 { u32::MAX } else { j[k] as u32 }
                    }
                    _ => u32::MAX,
                };
                pts.push(([p[0] as f64, p[1] as f64, p[2] as f64], dom));
            }
        }
        let skinned = pts.iter().filter(|(_, d)| *d != u32::MAX).count();
        let name = std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ys: Vec<f64> = pts.iter().map(|(p, _)| p[1]).collect();
        let h = ys.iter().cloned().fold(f64::MIN, f64::max) - ys.iter().cloned().fold(f64::MAX, f64::min);
        println!("== {name}  ({} verts, {skinned} skinned, height {h:.3} m)", pts.len());
        for (top_npc, bot_npc, lbl) in segments {
            let (Some(th), Some(bh)) = (ts.index_by_canonical(top_npc), ts.index_by_canonical(bot_npc)) else {
                println!("   {lbl:<9} (bone missing)"); continue;
            };
            let (Some(top), Some(bot)) = (ts.tgt(th), ts.tgt(bh)) else { continue };
            let ax = norm(v(bot, top));
            let l = len(v(bot, top));
            let mut row = String::new();
            for (f0, f1) in [(0.05, 0.30), (0.30, 0.55), (0.55, 0.80)] {
                let mut rs: Vec<f64> = Vec::new();
                for (p, dom) in &pts {
                    // Only flesh SKINNED to this bone counts. `skinned == 0` (no BLENDINDICES) keeps
                    // the old radius-window behaviour so unskinned dumps still report something.
                    if skinned > 0 && *dom != th { continue; }
                    let r = v(*p, top);
                    let t = dot(r, ax);
                    if t < f0 * l || t > f1 * l { continue; }
                    let perp = [r[0] - ax[0] * t, r[1] - ax[1] * t, r[2] - ax[2] * t];
                    let rr = len(perp);
                    if rr < 0.30 { rs.push(rr); }
                }
                if rs.len() < 10 { row.push_str("      n/a[0]"); continue; }
                rs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                // median is robust to the far leg / stray geometry sneaking into the slab. The
                // SAMPLE COUNT ships with it: a median over a handful of vertices is noise, and
                // without the count there is no way to tell that from a real measurement.
                row.push_str(&format!("{:9.1}[{}]", rs[rs.len() / 2] * 1000.0, rs.len()));
            }
            println!("   {lbl:<9}{row}");
        }
    }
}
