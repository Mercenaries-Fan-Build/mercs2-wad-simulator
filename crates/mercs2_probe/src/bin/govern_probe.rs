//! What does the destruction machine NOT govern on a tank?
//!
//! Wheeled vehicles have 96-98% of their near-tier geometry on nodes the switch machine names;
//! tanks only ~60%. Nodes in no switch group are `Static` — always rendered — which is correct for
//! treads and chassis and WRONG for a hull. So print, per node, what survives a wreck and what
//! doesn't: size, skin, and whether the machine can touch it.
//!
//!   cargo run -p mercs2_probe --bin govern_probe -- ch_veh_tank_ztz98

use mercs2_engine::model::Model;
use mercs2_engine::render_state::RenderState;
use mercs2_engine::wad;
use mercs2_formats::orchestrator as orch;
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let tier: u8 = match args.iter().position(|a| a == "--tier") {
        Some(i) => { let v = args[i + 1].parse().unwrap_or(0); args.drain(i..=i + 1); v }
        None => 0,
    };
    let names: Vec<String> = if args.is_empty() {
        vec!["ch_veh_tank_ztz98".into(), "civ_veh_car_van_crappy".into()]
    } else {
        args
    };
    let tex_names = load_names();
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for name in &names {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(m) = Model::load(&mut w, hash) else { continue };
        let Some(sm) = &m.machine else { continue };

        // Nodes the machine can address at all (named in any state's command list).
        let mut named: BTreeSet<u32> = BTreeSet::new();
        for n in &sm.nodes {
            for st in &n.states {
                named.extend(st.enter.iter().copied());
            }
        }
        let governed: BTreeSet<usize> = m
            .hier
            .iter()
            .enumerate()
            .filter(|(_, h)| named.contains(&h.hash))
            .map(|(i, _)| i)
            .collect();

        // Tier 0, intact vs wrecked.
        let state = |health: f32| -> RenderState {
            let chosen = orch::node_states_for_health(sm, health, 0.99);
            RenderState {
                lod: tier,
                view_state: 1u8 << tier,
                node_enable: orch::machine_node_enable(sm, &m.hier, &chosen),
            }
        };
        let draw_by_node = |rs: &RenderState| -> BTreeMap<i16, (u32, Option<u32>)> {
            let mut out: BTreeMap<i16, (u32, Option<u32>)> = BTreeMap::new();
            for (_, d) in m.visible_draws(rs) {
                let e = out.entry(d.node).or_insert((0, d.diffuse));
                e.0 += d.index_count / 3;
            }
            out
        };
        let intact = draw_by_node(&state(1.0));
        let wrecked = draw_by_node(&state(0.0));

        println!("\n=== {name} — tier 0 ===");
        let all: BTreeSet<i16> = intact.keys().chain(wrecked.keys()).copied().collect();
        let mut survives = 0u32;
        for n in all {
            let i = intact.get(&n).map(|x| x.0).unwrap_or(0);
            let wk = wrecked.get(&n).map(|x| x.0).unwrap_or(0);
            let tex = intact.get(&n).or(wrecked.get(&n)).and_then(|x| x.1);
            let verdict = match (i > 0, wk > 0) {
                (true, true) => {
                    survives += i.min(wk);
                    "SURVIVES the wreck"
                }
                (true, false) => "hidden on wreck",
                (false, true) => "appears on wreck",
                _ => "",
            };
            let g = if n >= 0 && governed.contains(&(n as usize)) { "governed" } else { "STATIC  " };
            let tname = tex
                .map(|t| tex_names.get(&t).cloned().unwrap_or_else(|| format!("0x{t:08X}")))
                .unwrap_or_default();
            if i.max(wk) < 100 {
                continue; // skip trim/detail noise
            }
            println!(
                "   node {n:3}  {g}  intact {i:6} tri  wrecked {wk:6} tri   {verdict:18}  {tname}"
            );
        }
        println!("   -> {survives} tri drawn identically whether intact or destroyed");
    }
}

fn load_names() -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    let p = "../../docs/data/aset_names.csv";
    if let Ok(s) = std::fs::read_to_string(p) {
        for line in s.lines().skip(1) {
            let mut f = line.split(',');
            if let (Some(h), Some(n)) = (f.next(), f.next()) {
                if let Ok(h) = u32::from_str_radix(h.trim_start_matches("0x"), 16) {
                    if !n.is_empty() {
                        out.insert(h, n.to_string());
                    }
                }
            }
        }
    }
    out
}
