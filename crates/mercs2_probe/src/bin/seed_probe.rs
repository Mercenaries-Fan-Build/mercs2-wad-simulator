//! Dev bin: compare the two candidate seeds for the node-enable table (`OBJ+0x2a0`) against real
//! models. The constructor `memset` is NOT recoverable from the decomp (register alias), so the seed
//! must be chosen by evidence, not asserted.
//!
//!   AllEnabled        — every node on; only an explicit `Hide` turns one off.
//!   SwitchSlotsHidden — every SWIT participant subtree off; a `SHOW` turns it back on.
//!
//! For each model and seed, prints how many draw groups survive the full three-clause gate at LOD
//! rung 0, so a wrong seed shows up as "the tank has no hull".
//!
//!   cargo run -p mercs2_probe --bin seed_probe

use mercs2_engine::render_state::RenderState;
use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator::{self as orch, NodeScope, NodeSeed};

const MODELS: &[(&str, u32)] = &[
    ("oc_veh_helicopter_md500", 0x9FCA_E910),
    ("ch_veh_tank_ztz98", 0xF881_47A1),
    ("uh1", 0x89D8_DE72),
    ("al_veh_boat_destroyer", 0xE540_47D5),
    ("ch_veh_helicopter_ka29b", 0x0BBA_3066),
    ("pmc_hum_mattias_v3", 0xA3C1_FABC),
];

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("open vz.wad");

    const COMBOS: &[(NodeSeed, NodeScope, &str)] = &[
        (NodeSeed::AllEnabled, NodeScope::NodeOnly, "on/node"),
        (NodeSeed::AllEnabled, NodeScope::Subtree, "on/tree"),
        (NodeSeed::SwitchSlotsHidden, NodeScope::NodeOnly, "swit/node"),
        (NodeSeed::SwitchSlotsHidden, NodeScope::Subtree, "swit/tree"),
    ];

    print!("{:<26} {:>7} {:>10}", "model", "groups", "passmask");
    for (_, _, label) in COMBOS {
        print!(" {label:>10}");
    }
    println!("   <- groups drawn at LOD rung 0");

    for (name, hash) in MODELS {
        let Ok(c) = wad::extract_container(&mut w, *hash) else {
            println!("{name:<26} <no container>");
            continue;
        };
        let Ok((_, _, draws, _)) = mesh::build_indexed_all(&c) else { continue };
        let hier = orch::parse_hier(&c);
        let machine = orch::parse_state_machine(&c);
        let pass_mask = draws.iter().filter(|d| (0x01 & d.lod_mask) != 0).count();

        print!("{name:<26} {:>7} {:>10}", draws.len(), pass_mask);
        for (seed, scope, _) in COMBOS {
            let node_enable = match &machine {
                Some(sm) => {
                    let chosen: Vec<usize> =
                        sm.nodes.iter().map(orch::default_state_index).collect();
                    orch::machine_node_enable_seeded(sm, &hier, &chosen, *seed, *scope)
                }
                None => Vec::new(),
            };
            let rs = RenderState { lod: 0, view_state: 0x01, node_enable };
            let n = draws.iter().filter(|d| rs.segment_visible(d.lod_mask, d.node)).count();
            print!(" {n:>10}");
        }
        println!();
    }
    println!(
        "\n  'passmask' = clause 2 only (no destruction gating) — the ceiling.\n  \
         A variant is wrong when it drops geometry the model obviously needs (a tank's hull) or keeps\n  \
         geometry it obviously must not (a helicopter's wreck)."
    );
}
