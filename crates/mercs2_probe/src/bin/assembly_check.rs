//! Acceptance test for `mercs2_engine::model::Model` — the cross-block assembly.
//!
//! For each model: the LOD chain it loaded, and then the FULL three-clause draw gate evaluated at
//! every LOD rung x {intact, wrecked}. A correct assembly means rung 0 (the camera up close) draws
//! the detailed body, not a 371-triangle `_lod_dm` proxy, and that wrecking it swaps the geometry
//! rather than piling wreck on top of hull.
//!
//!   cargo run -p mercs2_probe --bin assembly_check

use mercs2_engine::model::Model;
use mercs2_engine::render_state::RenderState;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;

const MODELS: &[&str] = &[
    "ch_veh_tank_ztz98",
    "oc_veh_helicopter_md500",
    "global_veh_klr650",
    "civ_veh_car_van_crappy",
    "al_veh_boat_destroyer",
    "pmc_hum_mattias_v3",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> =
        if args.is_empty() { MODELS.iter().map(|s| s.to_string()).collect() } else { args };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let m = match Model::load(&mut w, hash) {
            Ok(m) => m,
            Err(e) => {
                println!("{name}: {e}\n");
                continue;
            }
        };
        println!(
            "{name}  —  {} rung(s), {} tri total, {} HIER, {} SEGM, lod_count {}",
            m.rungs.len(),
            m.triangles(),
            m.hier.len(),
            m.segm.len(),
            m.lod_count()
        );
        for r in &m.rungs {
            println!(
                "     P{:03} block {:5}  {:6} tri  serves rungs {:?}",
                r.level,
                r.block,
                r.triangles(),
                (0..8u8).filter(|b| r.lod_bits() & (1 << b) != 0).collect::<Vec<_>>()
            );
        }

        // Full gate, per LOD rung, at full and zero health.
        print!("     {:>10}", "LOD rung");
        for n in 0..m.lod_count().min(8) {
            print!(" {:>8}", n);
        }
        println!();
        for (label, health) in [("intact", 1.0f32), ("wrecked", 0.0)] {
            let node_enable = match &m.machine {
                Some(sm) => {
                    let chosen = orch::node_states_for_health(sm, health, 0.99);
                    orch::machine_node_enable(sm, &m.hier, &chosen)
                }
                None => Vec::new(),
            };
            print!("     {label:>10}");
            for n in 0..m.lod_count().min(8) {
                let rs = RenderState {
                    lod: n as u8,
                    view_state: 1u8 << (n.min(7)),
                    node_enable: node_enable.clone(),
                };
                let tris: u32 =
                    m.visible_draws(&rs).map(|(_, d)| d.index_count / 3).sum();
                print!(" {tris:>8}");
            }
            println!();
        }
        println!();
    }
}
