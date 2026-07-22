//! Which bind pose was this model's geometry actually authored against — the raw `+80` record, or
//! the chained `+16` default pose?
//!
//! This matters because it is unfalsifiable from the rest pose. The engine skins
//! `v' = Pose_b(t) · invBind_b · v`. At rest `Pose == bind`, the two cancel, and the mesh renders
//! correctly NO MATTER which bind the geometry was authored against. The error is exactly zero until
//! an animation plays, and then it is large. So "it looks right in the viewer" is not evidence, and
//! a conform that picks the wrong bind produces precisely the reported symptom: a recognisable
//! character at rest that disfigures the moment it animates.
//!
//! `Skeleton` currently REJECTS `+80` records that put a bone on its parent, on the reasoning that a
//! zero-length bone cannot be a real bind, and falls back to the default pose for those. On
//! `pmc_hum_mattias` that is 8 nodes, and they are not arbitrary — they include the neck cluster,
//! i.e. the head. If that judgement is wrong, the head is exactly where the damage would show.
//!
//! The witness is the shipped mesh itself. A bone sits inside the geometry it drives, so for each
//! bone take the centroid of the vertices it dominates and measure how far each candidate bind puts
//! the bone from it. Whichever candidate is consistently closer is the pose the artist authored
//! against — this is the same test that established `+80` over `+16` model-wide, applied per bone so
//! it can speak about the rejected ones specifically.
//!
//!   bind_witness <model.block>

use mercs2_formats::model_cubeize::read_model_meshes;
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("usage: bind_witness <model.block>");
        std::process::exit(2);
    }
    let block = std::fs::read(&a[1]).expect("read block");
    let skel = Skeleton::from_block(&block).expect("skeleton");
    let ucfx_len = u32::from_le_bytes(block[16..20].try_into().unwrap()) as usize;
    let meshes = read_model_meshes(&block[20..20 + ucfx_len]).expect("meshes");

    // Centroid of the vertices each bone DOMINATES (is the largest influence of).
    let mut sum: HashMap<u32, [f64; 3]> = HashMap::new();
    let mut cnt: HashMap<u32, usize> = HashMap::new();
    for m in &meshes {
        if m.joints.is_empty() || m.weights.is_empty() {
            continue;
        }
        for i in 0..m.positions.len() {
            let mut best = (0u8, u32::MAX);
            for c in 0..4 {
                if m.weights[i][c] > best.0 {
                    best = (m.weights[i][c], m.joints[i][c] as u32);
                }
            }
            if best.0 == 0 {
                continue;
            }
            let p = m.positions[i];
            let e = sum.entry(best.1).or_insert([0.0; 3]);
            for k in 0..3 {
                e[k] += p[k] as f64;
            }
            *cnt.entry(best.1).or_insert(0) += 1;
        }
    }

    let dist = |a: [f32; 3], b: [f64; 3]| -> f64 {
        ((a[0] as f64 - b[0]).powi(2) + (a[1] as f64 - b[1]).powi(2) + (a[2] as f64 - b[2]).powi(2))
            .sqrt()
    };

    println!("{}: {} bones", a[1], skel.bones.len());
    println!("\nSTALE-REJECTED nodes (bind_world fell back to the default pose):");
    println!("  bone  verts   d(default)   d(raw +80)   winner");
    let (mut raw_wins, mut def_wins) = (0usize, 0usize);
    let (mut raw_err, mut def_err) = (0.0f64, 0.0f64);
    for b in skel.bones.iter() {
        if !b.bind_stale {
            continue;
        }
        let Some(&n) = cnt.get(&(b.index as u32)) else { continue };
        if n < 20 {
            continue; // too few vertices to locate a centroid meaningfully
        }
        let s = sum[&(b.index as u32)];
        let c = [s[0] / n as f64, s[1] / n as f64, s[2] / n as f64];
        let dd = dist([b.world[3][0], b.world[3][1], b.world[3][2]], c);
        let Some(r) = b.bind_world_raw else { continue };
        let dr = dist([r[3][0], r[3][1], r[3][2]], c);
        def_err += dd;
        raw_err += dr;
        let winner = if dr < dd {
            raw_wins += 1;
            "RAW +80  <-- rejection is WRONG here"
        } else {
            def_wins += 1;
            "default"
        };
        println!("  {:>4}  {n:>5}   {dd:10.4}   {dr:10.4}   {winner}", b.index);
    }
    println!(
        "\n  rejected nodes with enough geometry to judge: raw +80 closer on {raw_wins}, \
         default closer on {def_wins}"
    );
    if raw_wins + def_wins > 0 {
        println!(
            "  mean distance to dominated centroid: default {:.4} m, raw +80 {:.4} m",
            def_err / (raw_wins + def_wins) as f64,
            raw_err / (raw_wins + def_wins) as f64
        );
    }

    // Control: the same measurement over the bones that were NOT rejected. If +80 is the authoring
    // pose model-wide, it should win here decisively — and that is what makes the rejected-node
    // result interpretable rather than just noise.
    let (mut cr, mut cd, mut cre, mut cde) = (0usize, 0usize, 0.0f64, 0.0f64);
    for b in skel.bones.iter() {
        if b.bind_stale {
            continue;
        }
        let (Some(&n), Some(r)) = (cnt.get(&(b.index as u32)), b.bind_world_raw) else { continue };
        if n < 20 {
            continue;
        }
        let s = sum[&(b.index as u32)];
        let c = [s[0] / n as f64, s[1] / n as f64, s[2] / n as f64];
        let dd = dist([b.world[3][0], b.world[3][1], b.world[3][2]], c);
        let dr = dist([r[3][0], r[3][1], r[3][2]], c);
        cde += dd;
        cre += dr;
        if dr < dd {
            cr += 1
        } else {
            cd += 1
        }
    }
    println!("\nCONTROL, non-rejected bones: raw +80 closer on {cr}, default closer on {cd}");
    if cr + cd > 0 {
        println!(
            "  mean distance to dominated centroid: default {:.4} m, raw +80 {:.4} m",
            cde / (cr + cd) as f64,
            cre / (cr + cd) as f64
        );
    }
}
