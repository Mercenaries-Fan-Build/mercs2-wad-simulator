//! Chase the stray draws: each LOD rung's segments should carry masks from that rung's band
//! (resident = far tiers, P001 = mid, P002 = near), but every vehicle shows one or two groups whose
//! mask belongs to a NEIGHBOURING band. Either those are real (a segment authored to draw at another
//! tier) or our INDX->SEGM binding slips at a boundary. Print each rung's outliers with the seg_id
//! they resolved to, so the difference is visible rather than argued.
//!
//!   cargo run -p mercs2_probe --bin stray_probe -- ch_veh_tank_ztz98

use mercs2_engine::model::Model;
use mercs2_engine::wad;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if args.is_empty() {
        vec!["ch_veh_tank_ztz98".into(), "vz_veh_tank_amx30_elite".into()]
    } else {
        args
    };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(m) = Model::load(&mut w, hash) else { continue };
        println!("{name}   ({} SEGM records)", m.segm.len());

        for r in &m.rungs {
            // The rung's dominant band = the mask bits its majority of triangles carry.
            let mut per_bit = [0u32; 8];
            for d in &r.draws {
                for b in 0..8 {
                    if d.lod_mask & (1 << b) != 0 {
                        per_bit[b] += d.index_count / 3;
                    }
                }
            }
            let total: u32 = r.triangles();
            let band: u8 = (0..8)
                .filter(|&b| per_bit[b] * 4 > total) // a tier this rung really serves
                .fold(0u8, |a, b| a | (1 << b));

            println!(
                "   P{:03}  band {band:#04x}   ({} draws, {} tri)",
                r.level,
                r.draws.len(),
                total
            );
            for d in &r.draws {
                if d.lod_mask & band == 0 || (d.lod_mask & !band) & 0x7f != 0 && d.lod_mask != 0x7f {
                    // Mask reaches outside this rung's band — show what it bound to.
                    let seg = m.segm.get(d.seg_id);
                    println!(
                        "        OUTLIER grp {:3}  seg_id {:3}  mask {:#04x}  node {:3}  {:5} tri   SEGM[{}] = {:?}",
                        d.group_index,
                        d.seg_id,
                        d.lod_mask,
                        d.node,
                        d.index_count / 3,
                        d.seg_id,
                        seg.map(|s| (s.bone, s.seg_id, s.state_mask))
                    );
                }
            }
        }
        // How much of the NEAR-tier geometry does the destruction machine actually govern? A node the
        // machine never names is never hidden — so if a vehicle's near-tier hull sits on unnamed
        // nodes, blowing it up changes nothing up close.
        if let Some(sm) = &m.machine {
            let mut named: std::collections::BTreeSet<u32> = Default::default();
            for node in &sm.nodes {
                for st in &node.states {
                    named.extend(st.enter.iter().copied());
                }
            }
            let named_idx: std::collections::BTreeSet<usize> = m
                .hier
                .iter()
                .enumerate()
                .filter(|(_, h)| named.contains(&h.hash))
                .map(|(i, _)| i)
                .collect();
            let (mut gov, mut free) = (0u32, 0u32);
            for r in &m.rungs {
                for d in r.draws.iter().filter(|d| d.lod_mask & 0x03 != 0 && d.node >= 0) {
                    if named_idx.contains(&(d.node as usize)) {
                        gov += d.index_count / 3;
                    } else {
                        free += d.index_count / 3;
                    }
                }
            }
            let pct = if gov + free > 0 { gov * 100 / (gov + free) } else { 0 };
            println!("   near-tier geometry governed by the destruction machine: {gov} tri ({pct}%), ungoverned: {free} tri");
        }
        println!();
    }
}
