//! Compare the two [`NodeSeed`] variants across real models, at each health band.
//!
//! `NodeSeed` is the one part of the destruction pipeline that is **not** read from the exe — the
//! constructor's `memset` sits behind a register alias in the decomp (`model_render_gate_spec.md` §6),
//! so the default (`SwitchSlotsHidden`) was chosen from a single helicopter case. This probe puts real
//! numbers behind that choice: for each model it prints drawn/total draw groups under both seeds at
//! full / half / zero health.
//!
//! Reading it: a plausible vehicle draws MOST of its groups when pristine, fewer when destroyed, and
//! never zero (retail shows a wreck body, which is geometry in the same container). A seed that
//! yields 3/13 pristine or 0/13 destroyed is telling you it is wrong.
//!
//! ```text
//! MERCS2_VZ_WAD=<path> cargo run -p mercs2_probe --bin seedcmp -- [model ...]
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
    let Some(mut w) = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()) else {
        return eprintln!("no vz.wad (set MERCS2_VZ_WAD)");
    };

    println!("{:<26} {:>6}  {:>16}  {:>16}", "model", "health", "AllEnabled", "SwitchSlotsHidden");
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
        let hier = m.hier.clone();
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
        if std::env::var("SEEDCMP_ARGS").is_ok() {
            let mut spawned: Vec<u32> = Vec::new();
            let mut seen_args: Vec<String> = Vec::new();
            for node in &sm.nodes {
                for st in &node.states {
                    for (cmd, a) in orch::enter_commands(st) {
                        if cmd == co {
                            let joined: Vec<String> = a.iter().map(|v| format!("{v:#x}")).collect();
                            let line = joined.join(",");
                            if !seen_args.contains(&line) { seen_args.push(line); }
                            let _ = &mut spawned;
                        }
                    }
                }
            }
            for l in seen_args.iter().take(6) { println!("    CreateObject args: [{l}]"); }
        }

        let total = draws.len();
        for h in [1.0f32, 0.5, 0.0] {
            let chosen = orch::node_states_for_health(&sm, h, 0.99);
            let vis = |seed| {
                let en = orch::machine_node_enable_seeded(
                    &sm,
                    &hier,
                    &chosen,
                    seed,
                    orch::NodeScope::default(),
                );
                draws
                    .iter()
                    .filter(|d| d.node < 0 || en.get(d.node as usize).copied().unwrap_or(true))
                    .count()
            };
            let (a, s) = (vis(orch::NodeSeed::AllEnabled), vis(orch::NodeSeed::SwitchSlotsHidden));
            let flag = if s == 0 && a > 0 { "  <-- default hides EVERYTHING" } else { "" };
            println!(
                "{:<26} {h:>6}  {:>10}     {:>13}{flag}",
                if h == 1.0 { name.as_str() } else { "" },
                format!("{a}/{total}"),
                format!("{s}/{total}")
            );
        }
    }
}
