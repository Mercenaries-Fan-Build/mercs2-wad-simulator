//! Verify `wad::extract_model_lods`: a model's real LOD chain, and — for the finest rung — what its
//! destruction machine actually shows at pristine vs destroyed. The resident block is a low-poly
//! proxy; this is the first look at the geometry the game actually draws up close.
//!
//!   cargo run -p mercs2_probe --bin lodchain_probe -- ch_veh_tank_ztz98

use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;

const MODELS: &[&str] = &[
    "ch_veh_tank_ztz98",
    "oc_veh_helicopter_md500",
    "global_veh_klr650",
    "civ_veh_car_van_crappy",
    "pmc_hum_mattias_v3",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<String> =
        if args.is_empty() { MODELS.iter().map(|s| s.to_string()).collect() } else { args };
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let lods = match wad::extract_model_lods(&mut w, hash) {
            Ok(l) => l,
            Err(e) => {
                println!("{name}: {e}\n");
                continue;
            }
        };
        println!("{name}: {} LOD rung(s)", lods.len());
        for l in &lods {
            let Ok((_, _, draws, _)) = mesh::build_indexed_all(&l.container) else { continue };
            let tris: u32 = draws.iter().map(|d| d.index_count / 3).sum();
            let sm = orch::parse_state_machine(&l.container);
            let hier = orch::parse_hier(&l.container);
            let states = sm.as_ref().map(|s| s.nodes.len()).unwrap_or(0);

            // What survives the destruction gate at full vs zero health, ignoring the LOD mask (the
            // streamed rungs carry none — the rung IS the block).
            let mut hp = String::new();
            if let Some(sm) = &sm {
                for (label, health) in [("intact", 1.0f32), ("wrecked", 0.0)] {
                    let chosen = orch::node_states_for_health(sm, health, 0.99);
                    let en = orch::machine_node_enable(sm, &hier, &chosen);
                    let t: u32 = draws
                        .iter()
                        .filter(|d| d.node < 0 || en.get(d.node as usize).copied().unwrap_or(true))
                        .map(|d| d.index_count / 3)
                        .sum();
                    hp.push_str(&format!("  {label} {t}tri"));
                }
            }
            // Do the streamed rungs carry the SAME HIER node identity the machine keys on?
            let slots = [0x255E_AB53u32, 0x75F1_F74D, 0x510D_CB96, 0x54C5_95F0];
            let present: Vec<String> = slots
                .iter()
                .filter(|s| hier.iter().any(|n| n.hash == **s))
                .map(|s| format!("{s:#x}"))
                .collect();
            let nodes_with_geo: std::collections::BTreeSet<i16> =
                draws.iter().map(|d| d.node).filter(|&n| n >= 0).collect();
            println!(
                "   P{:03} block {:5}  {tris:6} tri  {} HIER  {} nodes w/ geo  {states} switch{hp}
           shared slots present: [{}]",
                l.level, l.block, hier.len(), nodes_with_geo.len(), present.join(", ")
            );
        }
        println!();
    }
}
