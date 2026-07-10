//! Verify the health-driven destruction driver: print each switch node's chosen state at a few
//! health levels, and how many draw groups the resulting node-enable keeps.
use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;
fn main(){
    let name = std::env::args().nth(1).unwrap_or_else(||"ch_veh_tank_ztz98".into());
    let hash = name.strip_prefix("0x").and_then(|h|u32::from_str_radix(h,16).ok())
        .unwrap_or_else(||mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w = wad::registry_vz_wad().and_then(|p|wad::open(&p).ok()).unwrap();
    let c = wad::extract_container(&mut w, hash).unwrap();
    let (_,_,draws,_) = mesh::build_indexed_all(&c).unwrap();
    let hier = orch::parse_hier(&c);
    let Some(sm) = orch::parse_state_machine(&c) else { return println!("{name}: no machine") };
    let (minor,terminal) = orch::damage_messages(&sm);
    println!("{name}: {} switch nodes; minor msgs {:?}; terminal msgs {:?}", sm.nodes.len(),
        minor.iter().map(|m|format!("{m:#010x}")).collect::<Vec<_>>(),
        terminal.iter().map(|m|format!("{m:#010x}")).collect::<Vec<_>>());
    for h in [1.0f32, 0.5, 0.0] {
        let chosen = orch::node_states_for_health(&sm, h, 0.99);
        let en = orch::machine_node_enable(&sm, &hier, &chosen);
        let vis = draws.iter().filter(|d| d.node<0 || en.get(d.node as usize).copied().unwrap_or(true)).count();
        let states: Vec<String> = sm.nodes.iter().zip(&chosen)
            .map(|(n,&s)| format!("{:#010x}", n.states.get(s).map(|st|st.name_hash).unwrap_or(0))).collect();
        println!("  health {h:>4}: {vis}/{} groups drawn; node states {:?}", draws.len(), states);
    }
}
