//! What the workshop ACTUALLY renders: apply BOTH gate clauses (LOD mask vs view_state, and the
//! destruction node-enable at a given health) and list the surviving meshes with their triangles.
//! `health_probe` only applies clause 3, which hides LOD problems.
//!
//!   cargo run -p mercs2_probe --bin visible_probe -- ch_veh_tank_ztz98

use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "ch_veh_tank_ztz98".into());
    let hash = name
        .strip_prefix("0x")
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_')));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let c = wad::extract_container(&mut w, hash).expect("container");
    let (_, _, draws, _) = mesh::build_indexed_all(&c).expect("build");
    let hier = orch::parse_hier(&c);
    let machine = orch::parse_state_machine(&c);

    let lod_count = mercs2_formats::model_cubeize::parse_model_header(&c)
        .map(|h| h.lod_count)
        .unwrap_or(8);
    let near = mesh::near_view_state(&draws, lod_count);
    let masks: std::collections::BTreeSet<u8> = draws.iter().map(|d| d.lod_mask).collect();
    println!("{name}");
    println!("  lod_count (header +0x34): {lod_count}");
    println!("  masks the meshes carry:   {masks:02X?}");
    println!("  near_view_state (1<<minLOD): 0x{near:02X}\n");

    for health in [1.0f32, 0.0] {
        let node_enable = match &machine {
            Some(sm) => {
                let chosen = orch::node_states_for_health(sm, health, 0.99);
                orch::machine_node_enable(sm, &hier, &chosen)
            }
            None => Vec::new(),
        };
        println!("  ── health {health} ──");
        for &vs in &[0x01u8, near] {
            let mut tris = 0u32;
            let mut n = 0;
            for d in &draws {
                let lod_ok = (vs & d.lod_mask) != 0;
                let node_ok = d.node < 0
                    || node_enable.get(d.node as usize).copied().unwrap_or(true);
                if lod_ok && node_ok {
                    n += 1;
                    tris += d.index_count / 3;
                }
            }
            println!("     view_state 0x{vs:02X}: {n:2} meshes, {tris:5} tris drawn");
        }
    }
}
