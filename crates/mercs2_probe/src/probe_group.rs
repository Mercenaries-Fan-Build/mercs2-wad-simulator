//! STEP-1 evidence probe: for given (block, ordinal) pairs, decode the DECL64
//! group and report vert/tri counts, weight-sum-to-255 fraction, decl stride,
//! distinct dominant bones (resolved to name-hash + world Y), and Y-bbox.
//! Read-only. Usage: probe_group <block.bin> <ord> [<ord> ...]

use mercs2_formats::model_inject::{extract_group_mesh, survey_groups};
use mercs2_formats::skeleton::Skeleton;
use std::collections::HashMap;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = &a[1];
    let block = std::fs::read(path).expect("read");
    let skel = Skeleton::from_block(&block).expect("skel");
    let groups = survey_groups(&block).expect("survey");
    eprintln!("{path}  ({} bones)", skel.bones.len());
    for ord_s in &a[2..] {
        let ord: usize = ord_s.parse().unwrap();
        let g = &groups[ord];
        // weight sanity from extracted mesh
        match extract_group_mesh(&block, ord) {
            Ok(m) => {
                let n = m.weights.len();
                let sum255 = m
                    .weights
                    .iter()
                    .filter(|w| w[0] as u32 + w[1] as u32 + w[2] as u32 + w[3] as u32 == 255)
                    .count();
                // dominant-bone histogram (resolve names + world Y)
                let mut hist: HashMap<u8, usize> = HashMap::new();
                for (j, w) in m.joints.iter().zip(m.weights.iter()) {
                    let dom = (0..4).max_by_key(|&i| w[i]).unwrap();
                    *hist.entry(j[dom]).or_insert(0) += 1;
                }
                let mut h: Vec<(u8, usize)> = hist.into_iter().collect();
                h.sort_by(|a, b| b.1.cmp(&a.1));
                eprintln!(
                    "  ord{ord}: stride={} vc={} tris={} ic={} sum255={}/{} ({:.1}%) Y[{:.2}..{:.2}]",
                    g.stride,
                    m.positions.len(),
                    m.tris.len(),
                    g.index_count,
                    sum255,
                    n,
                    100.0 * sum255 as f32 / n.max(1) as f32,
                    g.y_min,
                    g.y_max
                );
                eprint!("    dom bones:");
                for (bi, cnt) in h.iter().take(6) {
                    let (nh, by) = skel
                        .bones
                        .get(*bi as usize)
                        .map(|b| (b.name_hash, b.world_pos()[1]))
                        .unwrap_or((0, f32::NAN));
                    eprint!(" [b{bi}x{cnt} h={nh:#010x} y={by:.2}]");
                }
                eprintln!();
            }
            Err(e) => eprintln!("  ord{ord}: stride={} NOT-DECL64 ({e})", g.stride),
        }
    }
}
