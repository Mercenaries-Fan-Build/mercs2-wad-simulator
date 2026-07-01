//! STEP-1 SURVEY for the FRANKENSTEIN kitbash. Parses every drawing group of
//! mattias_v2 (target), chris_v2 (head donor) and jen (torso donor): per group the
//! vertex/index counts, Y-bounds (height axis) and BLENDINDICES dominant-bone
//! histogram, with each dominant bone's NAME-HASH resolved from that model's HIER.
//! Read-only survey; emits no model. DO NOT deploy.
//!
//! Usage: survey_frankenstein <block.bin> [<block.bin> ...]

use mercs2_formats::model_inject::survey_groups;
use mercs2_formats::skeleton::Skeleton;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    for path in &args[1..] {
        let block = std::fs::read(path).expect("read block");
        let skel = Skeleton::from_block(&block).expect("skeleton");
        let groups = survey_groups(&block).expect("survey");
        eprintln!("==================================================================");
        eprintln!("{path}");
        eprintln!("  skeleton: {} bones, height {:.4}", skel.bones.len(), skel.height());
        eprintln!("  {} drawing groups", groups.len());
        for g in &groups {
            if g.stride != 40 {
                eprintln!(
                    "  ord{:>2}: stride={} vc={} ic={} draws={} (non-DECL64, skipped)",
                    g.ordinal, g.stride, g.vertex_count, g.index_count, g.draws
                );
                continue;
            }
            // resolve top dominant bones -> name-hash + world Y
            let mut bone_str = String::new();
            for (bi, cnt) in g.bone_hist.iter().take(4) {
                let (nh, by) = skel
                    .bones
                    .get(*bi as usize)
                    .map(|b| (b.name_hash, b.world_pos()[1]))
                    .unwrap_or((0, f32::NAN));
                bone_str.push_str(&format!(" [b{bi}x{cnt} h={nh:#010x} y={by:.2}]"));
            }
            eprintln!(
                "  ord{:>2}: vc={:>5} ic={:>6} draws={} Y[{:.2}..{:.2}] bbox x[{:.2}..{:.2}] z[{:.2}..{:.2}]{}",
                g.ordinal,
                g.vertex_count,
                g.index_count,
                g.draws,
                g.y_min,
                g.y_max,
                g.bbox_min[0],
                g.bbox_max[0],
                g.bbox_min[2],
                g.bbox_max[2],
                bone_str
            );
        }
    }
}
