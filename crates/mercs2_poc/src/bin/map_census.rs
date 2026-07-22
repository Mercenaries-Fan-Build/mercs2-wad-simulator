//! Where does joint-mapping quality actually get lost?
//!
//! The automap is name-pattern matching with an "inherit from the nearest mapped ancestor"
//! fallback. That fallback is not neutral: an inherited joint does not get its OWN target bone,
//! it is merged into an ancestor's, so several source joints share one palette slot, their
//! weights sum, one dominates, and the vertex goes rigid. Measured downstream: 85.4% of 50 Cent's
//! vertices end up bound to exactly one bone, against 17.2%/6.7% in shipped characters.
//!
//! Rather than argue about heuristics, count it: of N source joints, how many map DIRECTLY, how
//! many are INHERITED (i.e. collapsed), how many distinct target bones are used out of those
//! available, and which targets are the collapse points.
//!
//!   map_census <model.glb> <donor.block>

#[path = "../gltf.rs"]
mod gltf;

use mercs2_formats::char_skin::automap::{automap, Rig};
use mercs2_formats::char_skin::TargetSkeleton;
use mercs2_formats::skeleton::Skeleton;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 3 {
        eprintln!("usage: map_census <model.glb> <donor.block>");
        std::process::exit(2);
    }
    let glb = gltf::load_char_glb(&a[1]).expect("glb");
    let donor = std::fs::read(&a[2]).expect("donor");
    let skel = Skeleton::from_block(&donor).expect("skeleton");
    let target = TargetSkeleton::from_skeleton(&skel);

    let rig = Rig {
        joint_nodes: &glb.joint_nodes,
        node_parent: &glb.node_parent,
        node_name: &glb.node_name,
    };
    let am = automap(&rig);

    let n = glb.joint_nodes.len();
    let direct = am.mapped.len();
    let inherited = am.inherited.len();
    let unmapped = n - direct - inherited;

    // How many source joints land on each target bone? >1 means a collapse point.
    let mut per_target: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (&j, &h) in am.mapped.iter().chain(am.inherited.iter()) {
        per_target.entry(h).or_default().push(j);
    }
    let distinct: BTreeSet<u32> = per_target.keys().copied().collect();

    println!("source joints        : {n}");
    println!("  mapped DIRECTLY    : {direct} ({:.0}%)", 100.0 * direct as f64 / n as f64);
    println!("  INHERITED (merged) : {inherited} ({:.0}%)", 100.0 * inherited as f64 / n as f64);
    println!("  unmapped           : {unmapped}");
    println!("target bones available: {}", target.bones.len());
    println!("target bones USED     : {} ({:.0}% of the rig)", distinct.len(),
        100.0 * distinct.len() as f64 / target.bones.len().max(1) as f64);

    let mut collapse: Vec<(usize, u32)> = per_target.iter().map(|(&h, v)| (v.len(), h)).collect();
    collapse.sort_by(|a, b| b.0.cmp(&a.0));
    println!("\nworst collapse points (source joints sharing ONE target bone):");
    for (cnt, h) in collapse.iter().take(10) {
        if *cnt < 2 { break; }
        let names: Vec<&str> = per_target[h].iter().take(6).map(|&j| am.names[j].as_str()).collect();
        println!("  bone {h:3}  <- {cnt:3} joints   e.g. {}", names.join(", "));
    }

    // Where does the count go between automap and the WRITTEN skin? The palette is RLE'd into at
    // most MAX_RANGES runs and finger-collapsed, so bones can be lost AFTER mapping succeeded.
    let cs = mercs2_formats::char_skin::build_character(
        &glb.build_input(&target, None, std::collections::HashMap::new(), false)).expect("build");
    let full_targets: BTreeSet<u32> = cs.full.values().copied().collect();
    let palette: BTreeSet<u16> = cs.ranges.iter().flat_map(|&(b, c)| (b..b + c)).collect();
    println!("
-- downstream of the automap --");
    println!("  automap distinct targets      : {}", distinct.len());
    println!("  after finger-collapse (cs.full): {}", full_targets.len());
    println!("  covered by the written palette : {} slots over {} runs", palette.len(), cs.ranges.len());
    let lost: Vec<u32> = full_targets.iter().copied().filter(|h| !palette.contains(&(*h as u16))).collect();
    println!("  mapped bones NOT in the palette: {} -> {:?}", lost.len(), &lost[..lost.len().min(12)]);

    let merged: usize = per_target.values().filter(|v| v.len() > 1).map(|v| v.len() - 1).sum();
    println!("\njoints lost to merging: {merged} of {n} ({:.0}%)", 100.0 * merged as f64 / n as f64);
    println!("If the rigs are comparable in size, near-1:1 mapping is available and merging this");
    println!("hard is a CHOICE the fallback makes, not a constraint the target skeleton imposes.");
}
