//! THE JOIN: the resident block's SEGM is the master segment table for the WHOLE LOD chain.
//!
//! Each rung ships geometry + its own `INDX` (group -> seg_id) but no SEGM/HIER of its own. Resolve
//! rung N's INDX against the RESIDENT SEGM and the near-LOD masks should appear — the coarse rung
//! claims the rung-4..6 records, the fine rungs claim rung-0..3. If that holds, a model is
//! `geometry(rung) x INDX(rung) x SEGM/HIER/MTRL/machine(resident)`, and our whole "the tank has no
//! near LOD" problem was reading SEGM out of the wrong block.
//!
//!   cargo run -p mercs2_probe --bin segm_join -- ch_veh_tank_ztz98

use mercs2_engine::{mesh, wad};
use mercs2_formats::model_cubeize::parse_segm;
use mercs2_formats::orchestrator as orch;
use std::collections::BTreeMap;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let lods = wad::extract_model_lods(&mut w, hash).expect("lod chain");

    let resident = &lods[0].container;
    let segm = parse_segm(resident);
    let hier = orch::parse_hier(resident);
    println!("{name}\n  resident SEGM: {} records, HIER: {} nodes\n", segm.len(), hier.len());

    let mut claimed: Vec<bool> = vec![false; segm.len()];

    for l in &lods {
        let indx = orch::parse_indx(&l.container);
        let Ok((_, _, draws, _)) = mesh::build_indexed_all(&l.container) else { continue };

        // group -> seg_id -> RESIDENT SEGM record
        let mut by_mask: BTreeMap<u8, (usize, u32)> = BTreeMap::new(); // mask -> (groups, tris)
        let mut nodes: std::collections::BTreeSet<i16> = Default::default();
        let mut unresolved = 0usize;
        for (gi, d) in draws.iter().enumerate() {
            let Some(&seg_id) = indx.get(gi) else {
                unresolved += 1;
                continue;
            };
            let Some(seg) = segm.get(seg_id) else {
                unresolved += 1;
                continue;
            };
            claimed[seg_id] = true;
            let e = by_mask.entry(seg.state_mask).or_insert((0, 0));
            e.0 += 1;
            e.1 += d.index_count / 3;
            nodes.insert(seg.bone as i16);
        }

        println!("  P{:03} (block {}): {} groups, INDX {} entries", l.level, l.block, draws.len(), indx.len());
        for (mask, (groups, tris)) in &by_mask {
            let rungs: Vec<u32> = (0..8).filter(|b| mask & (1 << b) != 0).collect();
            println!("        mask {mask:#04x} -> rungs {rungs:?}   {groups} groups, {tris} tri");
        }
        println!("        distinct HIER nodes: {}   unresolved groups: {unresolved}", nodes.len());
    }

    let unclaimed = claimed.iter().filter(|c| !**c).count();
    println!("\n  SEGM records claimed by the chain: {}/{} ({unclaimed} unclaimed)",
        claimed.iter().filter(|c| **c).count(), segm.len());
}
