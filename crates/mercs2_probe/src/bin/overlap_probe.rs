//! Do two LOD rungs ever draw the SAME node at the same tier?
//!
//! `Model::visible_draws` pools every loaded rung's segments and lets the LOD mask sort them out. If
//! the rungs are complementary (each ships the finest version of the parts it carries) that's right.
//! If they overlap — the same HIER node covered by a P001 group AND a P002 group at one tier — then
//! pooling draws the part twice at two detail levels, and the rung must be SELECTED, not pooled.
//!
//!   cargo run -p mercs2_probe --bin overlap_probe -- ch_veh_tank_ztz98

use mercs2_engine::model::Model;
use mercs2_engine::wad;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> = if args.is_empty() {
        vec!["ch_veh_tank_ztz98".into(), "vz_veh_tank_scorpion90".into(), "civ_veh_car_van_crappy".into()]
    } else { args };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(m) = Model::load(&mut w, hash) else { continue };
        println!("{name}");
        for tier in 0..m.lod_count().min(8) as u8 {
            let bit = 1u8 << tier;
            // node -> which rung levels cover it at this tier
            let mut cover: BTreeMap<i16, BTreeSet<u8>> = BTreeMap::new();
            let mut dup_tris = 0u32;
            for r in &m.rungs {
                for d in r.draws.iter().filter(|d| d.lod_mask & bit != 0 && d.node >= 0) {
                    cover.entry(d.node).or_default().insert(r.level);
                }
            }
            for r in &m.rungs {
                for d in r.draws.iter().filter(|d| d.lod_mask & bit != 0 && d.node >= 0) {
                    if cover.get(&d.node).map(|s| s.len() > 1).unwrap_or(false) {
                        dup_tris += d.index_count / 3;
                    }
                }
            }
            let contested: Vec<i16> =
                cover.iter().filter(|(_, s)| s.len() > 1).map(|(n, _)| *n).collect();
            println!(
                "   tier {tier}: {} nodes drawn, {} covered by MORE THAN ONE rung ({dup_tris} tri double-drawn)",
                cover.len(), contested.len()
            );
            for n in &contested {
                let mut parts: Vec<String> = Vec::new();
                for r in &m.rungs {
                    let t: u32 = r.draws.iter()
                        .filter(|d| d.node == *n && d.lod_mask & bit != 0)
                        .map(|d| d.index_count / 3).sum();
                    let masks: BTreeSet<u8> = r.draws.iter()
                        .filter(|d| d.node == *n && d.lod_mask & bit != 0)
                        .map(|d| d.lod_mask).collect();
                    if t > 0 {
                        parts.push(format!("P{:03}: {t} tri masks {masks:02X?}", r.level));
                    }
                }
                println!("          node {n:3}  {}", parts.join("   |   "));
            }
        }
        println!();
    }
}
