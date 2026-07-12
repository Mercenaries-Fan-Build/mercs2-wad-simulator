//! Compare the body-variant structure ACROSS VEHICLE TYPES. The car renders correctly under our
//! current gate while the tank and heli invert, so the model we reverse-engineered probably fits a
//! 4-wheeler and nothing else. This lays the types side by side: the two shared body slots
//! (`0x255EAB53` / `0x75F1F74D`), their meshes/masks, and what the pristine state actually SHOWs.
//!
//!   cargo run -p mercs2_probe --bin vehcmp_probe

use mercs2_engine::{mesh, wad};
use mercs2_formats::orchestrator as orch;

const SLOT_A: u32 = 0x255E_AB53;
const SLOT_B: u32 = 0x75F1_F74D;

const MODELS: &[(&str, &str)] = &[
    ("car   ", "civ_veh_car_van_crappy"),
    ("car2  ", "global_veh_fordf150"),
    ("bike  ", "global_veh_klr650"),
    ("tank  ", "ch_veh_tank_ztz98"),
    ("heli  ", "oc_veh_helicopter_md500"),
    ("heli2 ", "ch_veh_helicopter_ka29b"),
    ("boat  ", "al_veh_boat_destroyer"),
];

fn main() {
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");

    for (kind, name) in MODELS {
        let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
        let Ok(c) = wad::extract_container(&mut w, hash) else {
            println!("{kind} {name:26} <no container>");
            continue;
        };
        let Ok((_, _, draws, _)) = mesh::build_indexed_all(&c) else { continue };
        let hier = orch::parse_hier(&c);
        let Some(sm) = orch::parse_state_machine(&c) else {
            println!("{kind} {name:26} <no state machine>");
            continue;
        };
        let lodc = mercs2_formats::model_cubeize::parse_model_header(&c)
            .map(|h| h.lod_count)
            .unwrap_or(8);
        let near = mesh::near_view_state(&draws, lodc);

        // node index for each shared slot
        let idx_of = |h: u32| hier.iter().position(|n| n.hash == h);
        // triangles + mask carried by meshes bound to that node
        let stats = |ni: Option<usize>| -> String {
            match ni {
                None => "absent".into(),
                Some(i) => {
                    let mut tris = 0u32;
                    let mut mask = 0u8;
                    for d in draws.iter().filter(|d| d.node as usize == i && d.node >= 0) {
                        tris += d.index_count / 3;
                        mask |= d.lod_mask;
                    }
                    if tris == 0 {
                        format!("n{i} (no mesh)")
                    } else {
                        format!("n{i} {tris:5}tri mask{mask:#04x}")
                    }
                }
            }
        };

        // what does each switch node's DEFAULT (pristine) state SHOW?
        let show = mercs2_formats::hash::pandemic_hash_m2("show");
        let mut shown: Vec<u32> = Vec::new();
        for node in &sm.nodes {
            let si = orch::default_state_index(node);
            let Some(st) = node.states.get(si) else { continue };
            let (mut args, mut i) = (Vec::new(), 0usize);
            while i < st.enter.len() {
                match st.enter[i] {
                    1 if i + 1 < st.enter.len() => {
                        args.push(st.enter[i + 1]);
                        i += 2;
                    }
                    2 if i + 1 < st.enter.len() => {
                        if st.enter[i + 1] == show {
                            shown.extend(args.iter().copied());
                        }
                        args.clear();
                        i += 2;
                    }
                    3 => i += 1,
                    _ => {
                        args.push(st.enter[i]);
                        i += 1;
                    }
                }
            }
        }
        let shows_a = shown.contains(&SLOT_A);
        let shows_b = shown.contains(&SLOT_B);

        println!(
            "{kind} {name:26} lod{lodc} near{near:#04x} | A(0x255EAB53) {:22} | B(0x75F1F74D) {:22} | pristine SHOWs: {}{}",
            stats(idx_of(SLOT_A)),
            stats(idx_of(SLOT_B)),
            if shows_a { "A " } else { "-- " },
            if shows_b { "B" } else { "-" },
        );
    }
    println!("\n  A/B are the two shared body slots (siblings under the body switch node).");
    println!("  The car renders correctly today; tank+heli invert. Look for what differs.");
}
