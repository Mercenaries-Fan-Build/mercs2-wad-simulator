//! Rung 1 of the amx30_elite renders a perfect tank; rungs 0/2/4 fling its treads into the air.
//! Correct geometry, wrong transform — the signature of a rigid (MESH) accessory placed against the
//! wrong bone. Rigid meshes are authored in BONE-LOCAL space and must be multiplied by the bone's
//! world matrix; skinned (SKIN) meshes are already in model space. Print every mesh with its mask,
//! the SEGM row it bound to, its bone, and whether it is rigid — and compare the good tier's meshes
//! against the broken ones.
//!
//!   cargo run -p mercs2_probe --bin rigid_probe -- vz_veh_tank_amx30_elite

use mercs2_engine::wad;
use mercs2_formats::model_cubeize::{parse_segm, read_model_meshes_segm};

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "vz_veh_tank_amx30_elite".into());
    let hash = mercs2_formats::hash::pandemic_hash_m2(name.trim_start_matches('_'));
    let mut w = wad::registry_vz_wad().and_then(|p| wad::open(&p).ok()).expect("vz.wad");
    let lods = wad::extract_model_lods(&mut w, hash).expect("chain");
    let resident = lods[0].container.clone();
    let segm = parse_segm(&resident);

    println!("{name}: {} SEGM rows\n", segm.len());
    for l in &lods {
        let res = if l.level == 0 { None } else { Some(resident.as_slice()) };
        let Ok(meshes) = read_model_meshes_segm(&l.container, res.map(|_| segm.as_slice())) else {
            continue;
        };
        let indx = mercs2_formats::orchestrator::parse_indx(&l.container);
        println!("P{:03}  {} meshes, INDX has {} entries{}",
            l.level, meshes.len(), indx.len(),
            if meshes.len() > indx.len() {
                format!("   <-- {} mesh(es) have NO INDX row and fall back to sub_object", meshes.len() - indx.len())
            } else { String::new() });
        for (gi, m) in meshes.iter().enumerate() {
            if gi >= indx.len() {
                let seg = segm.get(m.seg_id);
                println!("        gi {gi:2} sub_object {} -> seg_id {} => bone {:?} mask {:?}  ({} verts)",
                    m.sub_object, m.seg_id,
                    seg.map(|s| s.bone), seg.map(|s| s.state_mask), m.positions.len());
            }
        }
        let mut rigid_by_mask: std::collections::BTreeMap<u8, (usize, usize)> = Default::default();
        for m in &meshes {
            let seg = segm.get(m.seg_id);
            let mask = seg.map(|s| s.state_mask).unwrap_or(0);
            let e = rigid_by_mask.entry(mask).or_insert((0, 0));
            if m.rigid {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
        for (mask, (r, s)) in &rigid_by_mask {
            println!("     mask {mask:#04x}:  {r:2} RIGID (bone-local)   {s:2} skinned");
        }
    }
}
