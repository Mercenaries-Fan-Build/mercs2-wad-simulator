//! Drawn / total draw groups of real models at full, half and zero health, through the full LOD-block
//! chain and the node-enable table retail's state machine gives (`orchestrator::machine_node_enable`).
//!
//! Reading it: a plausible vehicle draws MOST of its groups when pristine, fewer when destroyed, and
//! never zero (retail shows a wreck body, which is geometry in the same container).
//!
//! ```text
//! MERCS2_GAME_DIR=<path> cargo run -p mercs2_probe --bin health_draws -- [model ...]
//! ```
use mercs2_engine::model::Model;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;

fn main() {
    let names: Vec<String> = {
        let a: Vec<String> = std::env::args().skip(1).collect();
        if a.is_empty() {
            ["ch_veh_tank_ztz98", "ch_veh_apc_wz551", "ch_veh_apc_zbd2000", "ch_veh_boat_destroyer"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            a
        }
    };
    let Some(mut w) = wad::resolve_vz_wad(None).and_then(|p| wad::open(&p).ok()) else {
        eprintln!("no vz.wad (set MERCS2_GAME_DIR or VZ_WAD)");
        std::process::exit(2);
    };

    println!("{:<26} {:>6}  {:>10}", "model", "health", "drawn");
    for name in &names {
        let hash = name
            .strip_prefix("0x")
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
        // Load the WHOLE LOD-block chain, not just the resident block. The resident block alone is
        // a vehicle's FAR-LOD proxy (`game_world.rs`: "renders 371-triangle tanks"), so measuring it
        // reports the coarse stand-in's node coverage, not what the engine draws up close.
        let Ok(m) = Model::load(&mut w, hash) else {
            println!("{name:<26}  -- no model");
            continue;
        };
        let (_, _, draws, _) = m.flatten();
        let Some(sm) = m.machine.clone() else {
            println!("{name:<26}  -- no machine");
            continue;
        };
        // Do real machines actually carry the side-effecting commands the runtime extracts as
        // intents? Count them across every state, so "we wired debris/fire" is backed by data.
        let (co, se) = (
            mercs2_formats::hash::pandemic_hash_m2("createobject"),
            mercs2_formats::hash::pandemic_hash_m2("startemitter"),
        );
        let (mut n_co, mut n_se) = (0usize, 0usize);
        for node in &sm.nodes {
            for st in &node.states {
                for (cmd, _) in orch::enter_commands(st) {
                    if cmd == co {
                        n_co += 1;
                    } else if cmd == se {
                        n_se += 1;
                    }
                }
            }
        }
        println!("{name:<26}  intents in machine: CreateObject x{n_co}, StartEmitter x{n_se}");

        let total = draws.len();
        for h in [1.0f32, 0.5, 0.0] {
            let chosen = orch::node_states_for_health(&sm, h, 0.99);
            let en = match orch::machine_node_enable(&sm, &m.hier, &chosen) {
                Ok(en) => en,
                Err(e) => {
                    eprintln!("{name}: {e}");
                    std::process::exit(1);
                }
            };
            let drawn = draws.iter().filter(|d| d.node < 0 || en.get(d.node as usize).copied().unwrap_or(true)).count();
            let flag = if drawn == 0 { "  <-- hides EVERYTHING" } else { "" };
            println!("{:<26} {h:>6}  {:>10}{flag}", if h == 1.0 { name.as_str() } else { "" }, format!("{drawn}/{total}"));
        }
    }
}
